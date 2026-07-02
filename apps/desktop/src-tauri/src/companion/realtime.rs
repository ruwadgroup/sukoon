//! Server side of the companion realtime streaming protocol
//! (docs/research/companion-realtime-protocol.md): one separation session per connection, fed by
//! binary audio frames and controlled by `rt_*` JSON messages.
//!
//! Inference is heavy and blocking, so each [`Session`] owns a dedicated worker thread wrapping a
//! [`RealtimeSeparator`]. The WS read loop forwards decoded input through a command channel
//! (capped at ~30 s of audio via a shared frame counter) and the worker pushes cleaned frames and
//! acks back through the connection's writer channel. Epochs make flushes race-free: the read
//! loop bumps the shared epoch immediately, so the worker discards any stale backlog without
//! running inference on it, and never emits old-epoch audio after acking the flush.

use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use sukoon_core::engine::EngineKind;
use sukoon_core::stream::RealtimeSeparator;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::EngineService;

/// Wire sample rate: the extension always streams 48 kHz.
pub const SAMPLE_RATE: u32 = 48_000;
/// Wire channel count: interleaved stereo.
pub const CHANNELS: usize = 2;

const HEADER_LEN: usize = 16;
const KIND_NOISY: u8 = 1;
const KIND_CLEANED: u8 = 2;
const BYTES_PER_FRAME: usize = 4 * CHANNELS;
/// Input backlog cap (~30 s of audio): past this the engine is hopelessly behind realtime and the
/// client is told to fall back (`rt_overloaded`).
const MAX_QUEUED_FRAMES: usize = 30 * SAMPLE_RATE as usize;

/// A decoded noisy-input audio frame (binary kind 1).
pub struct InputFrame {
    epoch: u16,
    samples: Vec<f32>,
}

impl InputFrame {
    /// Decode the 16-byte little-endian header plus interleaved f32 payload. Returns `None` for
    /// anything malformed; the caller logs and drops it (wire input must never panic).
    pub fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() < HEADER_LEN {
            return None;
        }
        let (header, body) = payload.split_at(HEADER_LEN);
        if header[0] != KIND_NOISY
            || header[1] as usize != CHANNELS
            || body.len() % BYTES_PER_FRAME != 0
        {
            return None;
        }
        // header[4..16] = reserved + startSample. Input is contiguous within an epoch, so output
        // positions are tracked server-side from cumulative counts and startSample is not needed.
        let epoch = u16::from_le_bytes([header[2], header[3]]);
        let samples = body
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        Some(Self { epoch, samples })
    }

    fn frames(&self) -> usize {
        self.samples.len() / CHANNELS
    }
}

/// Encode a cleaned-output audio frame (binary kind 2) covering
/// `[start_sample, start_sample + n)` of the epoch's 48 kHz stream.
fn cleaned_frame(epoch: u16, start_sample: u64, samples: &[f32]) -> Message {
    let mut buf = Vec::with_capacity(HEADER_LEN + samples.len() * 4);
    buf.push(KIND_CLEANED);
    buf.push(CHANNELS as u8);
    buf.extend_from_slice(&epoch.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&start_sample.to_le_bytes());
    for s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    Message::Binary(buf)
}

enum Cmd {
    Audio(InputFrame),
    Flush { epoch: u16 },
}

/// Handle to one realtime session. Dropping it tears the worker down: the worker stops emitting
/// immediately, discards its backlog without inference, and exits once the channel drains.
pub struct Session {
    alive: Arc<AtomicBool>,
    epoch: Arc<AtomicU16>,
    queued: Arc<AtomicUsize>,
    tx: mpsc::UnboundedSender<Cmd>,
}

impl Session {
    /// Spawn the session worker. The worker loads the engine itself (which can block on a
    /// first-use weight download) and then replies `rt_started` or `error`, so the read loop
    /// never blocks on engine setup.
    pub fn start(app: AppHandle, kind: EngineKind, out: mpsc::UnboundedSender<Message>) -> Self {
        let alive = Arc::new(AtomicBool::new(true));
        let epoch = Arc::new(AtomicU16::new(0));
        let queued = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = Worker {
            app,
            kind,
            out: out.clone(),
            alive: alive.clone(),
            epoch: epoch.clone(),
            queued: queued.clone(),
        };
        if let Err(e) = std::thread::Builder::new()
            .name("companion-rt".into())
            .spawn(move || worker.run(rx))
        {
            alive.store(false, Ordering::Release);
            let reply = json!({
                "type": "error",
                "error": format!("could not start realtime worker: {e}"),
            });
            let _ = out.send(Message::Text(reply.to_string()));
        }
        Self {
            alive,
            epoch,
            queued,
            tx,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Queue one input frame for separation. Frames for a stale epoch or a dead session are
    /// discarded silently. Returns `false` only when the input cap is exceeded; the caller then
    /// sends `rt_overloaded` and drops the session.
    pub fn push(&self, frame: InputFrame) -> bool {
        if !self.is_alive() || frame.epoch != self.epoch.load(Ordering::Acquire) {
            return true;
        }
        let frames = frame.frames();
        if self.queued.fetch_add(frames, Ordering::AcqRel) + frames > MAX_QUEUED_FRAMES {
            self.queued.fetch_sub(frames, Ordering::AcqRel);
            return false;
        }
        if self.tx.send(Cmd::Audio(frame)).is_err() {
            self.queued.fetch_sub(frames, Ordering::AcqRel);
        }
        true
    }

    /// Switch to `epoch`: the worker discards the stale backlog, resets the separator so
    /// `startSample` restarts at 0, and acks with `rt_flushed`.
    pub fn flush(&self, epoch: u16) {
        self.epoch.store(epoch, Ordering::Release);
        let _ = self.tx.send(Cmd::Flush { epoch });
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Also closes `tx`, so the worker drains (skipping inference, alive is false) and exits.
        self.alive.store(false, Ordering::Release);
    }
}

struct Worker {
    app: AppHandle,
    kind: EngineKind,
    out: mpsc::UnboundedSender<Message>,
    alive: Arc<AtomicBool>,
    epoch: Arc<AtomicU16>,
    queued: Arc<AtomicUsize>,
}

impl Worker {
    fn run(self, mut rx: mpsc::UnboundedReceiver<Cmd>) {
        let mut separator = match self.build_separator() {
            Ok(s) => s,
            Err(e) => return self.fail(e),
        };
        let block = separator.block_frames() as u64;
        // Estimated end-to-end latency: block assembly plus a conservative half-block inference
        // margin, all in 48 kHz samples.
        let latency = separator.latency_frames() as u64 + block / 2;
        self.send(json!({
            "type": "rt_started",
            "engine": engine_name(self.kind),
            "blockSamples": block,
            "latencySamples": latency,
        }));

        let mut out_pos: u64 = 0;
        while let Some(cmd) = rx.blocking_recv() {
            match cmd {
                Cmd::Audio(frame) => {
                    self.queued.fetch_sub(frame.frames(), Ordering::AcqRel);
                    if !self.alive.load(Ordering::Acquire)
                        || frame.epoch != self.epoch.load(Ordering::Acquire)
                    {
                        continue; // stale backlog after a flush/stop: skip without inference
                    }
                    match separator.push(&frame.samples) {
                        Ok(cleaned) => {
                            if cleaned.is_empty() {
                                continue;
                            }
                            let frames = (cleaned.len() / CHANNELS) as u64;
                            if self.alive.load(Ordering::Acquire) {
                                let _ =
                                    self.out.send(cleaned_frame(frame.epoch, out_pos, &cleaned));
                            }
                            out_pos += frames;
                        }
                        Err(e) => return self.fail(format!("realtime separation failed: {e}")),
                    }
                }
                Cmd::Flush { epoch } => {
                    separator.reset();
                    out_pos = 0;
                    self.send(json!({ "type": "rt_flushed", "epoch": epoch }));
                }
            }
        }
        self.alive.store(false, Ordering::Release);
    }

    fn build_separator(&self) -> Result<RealtimeSeparator, String> {
        let engine = self.app.state::<EngineService>().engine(self.kind)?;
        RealtimeSeparator::new(engine, SAMPLE_RATE).map_err(|e| e.to_string())
    }

    fn send(&self, reply: Value) {
        let _ = self.out.send(Message::Text(reply.to_string()));
    }

    fn fail(&self, error: String) {
        self.alive.store(false, Ordering::Release);
        self.send(json!({ "type": "error", "error": error }));
    }
}

/// The friendly engine name the companion protocol speaks (`ready.engines`, `rt_start.engine`).
fn engine_name(kind: EngineKind) -> &'static str {
    match kind {
        EngineKind::Fast => "fast",
        EngineKind::Hq => "hq",
        EngineKind::Fallback => "fallback",
    }
}
