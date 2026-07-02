//! Streaming separation for live sources: push noisy PCM in arbitrary-size chunks, pull cleaned
//! PCM out, with output stream positions tracking input positions 1:1.
//!
//! [`RealtimeSeparator`] wraps a resident [`Engine`] that reports a [`ChunkPlan`]: input is
//! resampled to the engine's rate, assembled into `context`-padded windows (mirroring the offline
//! `demix` padding, including the leading `context` zeros on the first block), separated one
//! `align` block at a time for lowest latency, and resampled back. The resamplers keep persistent
//! state across pushes, so cleaned sample `n` lines up with noisy sample `n` within a fraction of
//! a sample (the alignment tests pin this).
//!
//! Everything here is blocking, synchronous code: the caller owns threads.

use std::sync::Arc;

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::engine::{ChunkPlan, Engine};
use crate::{AudioBuffer, Error, Result};

/// Streaming is stereo end to end: the wire format and every streamable engine are 2-channel.
const CHANNELS: usize = 2;

/// A push-in/pull-out streaming front-end over a block-capable [`Engine`].
pub struct RealtimeSeparator {
    engine: Arc<dyn Engine>,
    plan: ChunkPlan,
    input_rate: u32,
    engine_rate: u32,
    /// `input_rate` to `engine_rate`; `None` when the rates already match.
    resample_in: Option<StreamResampler>,
    /// `engine_rate` back to `input_rate`; `None` when the rates already match.
    resample_out: Option<StreamResampler>,
    /// Engine-rate input awaiting block assembly. Always starts with the `context` frames that
    /// precede the next emit position (zeros at stream start, matching `demix`'s front padding).
    assembly: Vec<Vec<f32>>,
}

impl RealtimeSeparator {
    /// Wrap `engine` for streaming input at `input_rate` Hz (interleaved stereo f32).
    ///
    /// Fails if the engine does not report a [`ChunkPlan`] (no bounded-memory streaming mode).
    pub fn new(engine: Arc<dyn Engine>, input_rate: u32) -> Result<Self> {
        let plan = engine.chunk_plan().ok_or_else(|| {
            Error::Engine(format!(
                "engine `{}` does not support streaming",
                engine.id()
            ))
        })?;
        let engine_rate = engine.target_sample_rate();
        if input_rate == 0 || engine_rate == 0 {
            return Err(Error::Engine("sample rates must be non-zero".into()));
        }
        let (resample_in, resample_out) = if input_rate == engine_rate {
            (None, None)
        } else {
            (
                Some(StreamResampler::new(input_rate, engine_rate)?),
                Some(StreamResampler::new(engine_rate, input_rate)?),
            )
        };
        Ok(Self {
            plan,
            input_rate,
            engine_rate,
            resample_in,
            resample_out,
            assembly: vec![vec![0.0; plan.context]; CHANNELS],
            engine,
        })
    }

    /// Emit granularity in input-rate frames: cleaned audio arrives in blocks of about this size.
    pub fn block_frames(&self) -> usize {
        self.to_input_rate(self.plan.align)
    }

    /// Input frames that must arrive before the first cleaned block can come out (block assembly
    /// latency; excludes inference time and resampler chunking slack).
    pub fn latency_frames(&self) -> usize {
        self.to_input_rate(self.plan.align + self.plan.context)
    }

    fn to_input_rate(&self, frames: usize) -> usize {
        (frames as u64 * u64::from(self.input_rate)).div_ceil(u64::from(self.engine_rate)) as usize
    }

    /// Feed interleaved stereo input frames; returns whatever cleaned interleaved frames became
    /// available (possibly none). Output positions track input positions 1:1 across calls.
    pub fn push(&mut self, interleaved: &[f32]) -> Result<Vec<f32>> {
        if interleaved.len() % CHANNELS != 0 {
            return Err(Error::Engine(
                "interleaved stereo input must have an even sample count".into(),
            ));
        }
        let planar = deinterleave(interleaved);
        match &mut self.resample_in {
            Some(rs) => rs.push(&planar, &mut self.assembly)?,
            None => {
                for (dst, src) in self.assembly.iter_mut().zip(&planar) {
                    dst.extend_from_slice(src);
                }
            }
        }

        // Window i = signal[emit_pos - context .. emit_pos + align + context]; `assembly` already
        // holds the leading `context` carry, so a window is ready once it reaches this length.
        let window_len = 2 * self.plan.context + self.plan.align;
        let mut cleaned: Vec<Vec<f32>> = vec![Vec::new(); CHANNELS];
        while self.assembly[0].len() >= window_len {
            let window = AudioBuffer {
                channels: self
                    .assembly
                    .iter()
                    .map(|ch| ch[..window_len].to_vec())
                    .collect(),
                sample_rate: self.engine_rate,
            };
            let speech = self
                .engine
                .separate_block(&window, self.plan.context, self.plan.align)?;
            append_stereo(&mut cleaned, &speech, self.plan.align)?;
            for ch in &mut self.assembly {
                ch.drain(..self.plan.align);
            }
        }

        let out = match &mut self.resample_out {
            Some(rs) => {
                let mut out = vec![Vec::new(); CHANNELS];
                rs.push(&cleaned, &mut out)?;
                out
            }
            None => cleaned,
        };
        Ok(interleave(&out))
    }

    /// Clear all accumulated state (block carry and resampler tails), e.g. on flush/seek. The next
    /// push restarts the stream from position 0.
    pub fn reset(&mut self) {
        for ch in &mut self.assembly {
            ch.clear();
            ch.resize(self.plan.context, 0.0);
        }
        if let Some(rs) = &mut self.resample_in {
            rs.reset();
        }
        if let Some(rs) = &mut self.resample_out {
            rs.reset();
        }
    }
}

/// Input frames fed to each resampler `process` call. Small enough to keep per-push latency low,
/// large enough that the sinc convolution stays efficient.
const RESAMPLE_CHUNK: usize = 1024;

/// One direction of persistent-state streaming resampling.
///
/// `SincFixedIn` emits position-aligned output by construction: it primes its interpolation index
/// half a filter length back and withholds the trailing filter tail until the next chunk, so the
/// filter's group delay shows up as "needs more input", never as leading junk to drop. Output
/// frame `n` therefore already sits at input position `n / ratio` (verified empirically); dropping
/// `output_delay()` frames on top of that would misalign the stream, so nothing is dropped here.
/// The alignment tests in this module pin that behaviour.
struct StreamResampler {
    inner: SincFixedIn<f32>,
    /// Input frames awaiting a full [`RESAMPLE_CHUNK`].
    pending: Vec<Vec<f32>>,
}

impl StreamResampler {
    fn new(from: u32, to: u32) -> Result<Self> {
        // High-quality sinc interpolation sized for realtime blocks: a 128-tap filter keeps the
        // added latency under 1.5 ms while staying well above audible aliasing for 44.1k/48k.
        let params = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: 0.95,
            oversampling_factor: 256,
            interpolation: SincInterpolationType::Linear,
            window: WindowFunction::BlackmanHarris2,
        };
        let inner = SincFixedIn::new(
            f64::from(to) / f64::from(from),
            1.0,
            params,
            RESAMPLE_CHUNK,
            CHANNELS,
        )
        .map_err(|e| Error::Engine(format!("resampler init {from}→{to}Hz: {e}")))?;
        Ok(Self {
            inner,
            pending: vec![Vec::new(); CHANNELS],
        })
    }

    /// Append `planar` input and process every full chunk, extending `out` with the output.
    fn push(&mut self, planar: &[Vec<f32>], out: &mut [Vec<f32>]) -> Result<()> {
        for (dst, src) in self.pending.iter_mut().zip(planar) {
            dst.extend_from_slice(src);
        }
        while self.pending[0].len() >= RESAMPLE_CHUNK {
            let input: Vec<&[f32]> = self
                .pending
                .iter()
                .map(|ch| &ch[..RESAMPLE_CHUNK])
                .collect();
            let produced = self
                .inner
                .process(&input, None)
                .map_err(|e| Error::Engine(format!("resample: {e}")))?;
            for ch in &mut self.pending {
                ch.drain(..RESAMPLE_CHUNK);
            }
            for (dst, src) in out.iter_mut().zip(&produced) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.inner.reset();
        for ch in &mut self.pending {
            ch.clear();
        }
    }
}

fn deinterleave(interleaved: &[f32]) -> Vec<Vec<f32>> {
    let frames = interleaved.len() / CHANNELS;
    let mut planar: Vec<Vec<f32>> = (0..CHANNELS).map(|_| Vec::with_capacity(frames)).collect();
    for frame in interleaved.chunks_exact(CHANNELS) {
        for (ch, &s) in planar.iter_mut().zip(frame) {
            ch.push(s);
        }
    }
    planar
}

fn interleave(planar: &[Vec<f32>]) -> Vec<f32> {
    let frames = planar.first().map_or(0, Vec::len);
    let mut out = Vec::with_capacity(frames * CHANNELS);
    for i in 0..frames {
        for ch in planar {
            out.push(ch[i]);
        }
    }
    out
}

/// Append one separated block, coercing the engine's channel shape back to stereo.
fn append_stereo(dst: &mut [Vec<f32>], speech: &AudioBuffer, emit_len: usize) -> Result<()> {
    if speech.frame_count() != emit_len {
        return Err(Error::Engine(format!(
            "engine emitted {} frames, expected {emit_len}",
            speech.frame_count()
        )));
    }
    for (c, dst_ch) in dst.iter_mut().enumerate() {
        let src = speech
            .channels
            .get(c)
            .or_else(|| speech.channels.first())
            .ok_or_else(|| Error::Engine("engine emitted no channels".into()))?;
        dst_ch.extend_from_slice(src);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Separation;

    /// A pass-through engine with an MDX-shaped chunk plan: `separate_block` returns the emit
    /// window slice, so any deviation between output and input is the streamer's own doing.
    struct IdentityEngine {
        rate: u32,
        plan: ChunkPlan,
    }

    impl Engine for IdentityEngine {
        fn id(&self) -> &'static str {
            "identity"
        }
        fn target_sample_rate(&self) -> u32 {
            self.rate
        }
        fn realtime_capable(&self) -> bool {
            true
        }
        fn separate(&self, input: &AudioBuffer) -> Result<Separation> {
            Ok(Separation {
                speech: input.clone(),
                music: None,
                effects: None,
            })
        }
        fn chunk_plan(&self) -> Option<ChunkPlan> {
            Some(self.plan)
        }
        fn separate_block(
            &self,
            window: &AudioBuffer,
            context: usize,
            emit_len: usize,
        ) -> Result<AudioBuffer> {
            assert_eq!(window.frame_count(), emit_len + 2 * context);
            Ok(AudioBuffer {
                channels: window
                    .channels
                    .iter()
                    .map(|ch| ch[context..context + emit_len].to_vec())
                    .collect(),
                sample_rate: window.sample_rate,
            })
        }
    }

    fn identity(rate: u32, context: usize, align: usize) -> Arc<dyn Engine> {
        Arc::new(IdentityEngine {
            rate,
            plan: ChunkPlan {
                context,
                align,
                block_frames: align,
            },
        })
    }

    /// Interleaved stereo where each frame encodes its own stream position, so any misalignment
    /// or channel swap is immediately visible.
    fn position_signal(frames: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            out.push(i as f32 * 1e-3);
            out.push(i as f32 * -1e-3);
        }
        out
    }

    fn stream_all(sep: &mut RealtimeSeparator, input: &[f32], chunk: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for piece in input.chunks(chunk) {
            out.extend(sep.push(piece).unwrap());
        }
        out
    }

    #[test]
    fn same_rate_stream_is_bit_exact_and_aligned() {
        let (context, align) = (64, 256);
        let mut sep = RealtimeSeparator::new(identity(48_000, context, align), 48_000).unwrap();
        assert_eq!(sep.block_frames(), align);
        assert_eq!(sep.latency_frames(), align + context);

        let frames = 2_000;
        let input = position_signal(frames);
        // Odd-sized pushes so chunk boundaries never line up with block boundaries.
        let out = stream_all(&mut sep, &input, 2 * 17);

        // Blocks come out once `align + context` input frames past them have arrived.
        let expect_frames = (frames - context) / align * align;
        assert_eq!(out.len(), expect_frames * 2);
        assert_eq!(out, input[..out.len()]);
    }

    #[test]
    fn reset_restarts_the_stream_cleanly() {
        let mut sep = RealtimeSeparator::new(identity(48_000, 64, 256), 48_000).unwrap();

        // Pollute all internal state, then reset.
        let noise: Vec<f32> = (0..1_500 * 2)
            .map(|i| ((i * 7919) % 100) as f32 * 0.01)
            .collect();
        let _ = stream_all(&mut sep, &noise, 2 * 33);
        sep.reset();

        let input = position_signal(2_000);
        let out = stream_all(&mut sep, &input, 2 * 17);
        assert_eq!(
            out,
            input[..out.len()],
            "post-reset output must match a fresh stream"
        );
    }

    #[test]
    fn resampled_stream_tracks_input_positions() {
        // MDX-shaped plan (context 3072, engine rate 44.1k) with a small align to keep it fast.
        let (context, align) = (3_072, 8_192);
        let mut sep = RealtimeSeparator::new(identity(44_100, context, align), 48_000).unwrap();
        assert!(sep.block_frames() >= align, "align scales up at 48k");

        // A 220 Hz stereo sine (right at half amplitude), pushed in uneven chunks.
        let frames = 120_000; // 2.5 s at 48 kHz
        let mut input = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin() * 0.8;
            input.push(s);
            input.push(s * 0.5);
        }
        let out = stream_all(&mut sep, &input, 2 * 601);

        let out_frames = out.len() / 2;
        assert!(out_frames > 0, "stream must produce output");
        assert!(out_frames <= frames, "output cannot run ahead of input");
        assert!(
            frames - out_frames <= sep.latency_frames() + 2 * RESAMPLE_CHUNK,
            "only assembly + resampler chunking may be withheld (got {} frames behind)",
            frames - out_frames
        );

        // Position alignment: cleaned frame n must match noisy frame n. Skip the onset, where the
        // abrupt sine start is band-limited by the resampler.
        let mut max_err = 0.0f32;
        for i in 1_000..out.len() {
            max_err = max_err.max((out[i] - input[i]).abs());
        }
        assert!(
            max_err < 0.05,
            "worst 48k round-trip deviation {max_err} too large"
        );

        // Determinism across reset: the same input replays to the same output.
        sep.reset();
        let again = stream_all(&mut sep, &input, 2 * 601);
        assert_eq!(out, again, "reset must fully restore initial state");
    }

    #[test]
    fn rejects_engines_without_a_chunk_plan_and_odd_input() {
        struct NoPlan;
        impl Engine for NoPlan {
            fn id(&self) -> &'static str {
                "noplan"
            }
            fn target_sample_rate(&self) -> u32 {
                48_000
            }
            fn realtime_capable(&self) -> bool {
                false
            }
            fn separate(&self, input: &AudioBuffer) -> Result<Separation> {
                Ok(Separation {
                    speech: input.clone(),
                    music: None,
                    effects: None,
                })
            }
        }
        assert!(RealtimeSeparator::new(Arc::new(NoPlan), 48_000).is_err());

        let mut sep = RealtimeSeparator::new(identity(48_000, 64, 256), 48_000).unwrap();
        assert!(
            sep.push(&[0.0; 3]).is_err(),
            "odd sample counts are not stereo frames"
        );
    }
}
