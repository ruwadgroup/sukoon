//! FFmpeg wrapper for audio extraction, encoding, and remuxing.

use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::{Error, Result};

/// Name (or absolute path) of the ffmpeg binary. Override with `SUKOON_FFMPEG`.
fn ffmpeg_bin() -> String {
    std::env::var("SUKOON_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string())
}

/// Extract the audio track to a stereo float WAV at the engine sample rate.
pub fn extract_audio(input: &Path, out: &Path, sample_rate: u32) -> Result<()> {
    let ar = sample_rate.to_string();
    run(&[
        "-y",
        "-i",
        path(input)?,
        "-vn",
        "-ac",
        "2",
        "-ar",
        &ar,
        "-c:a",
        "pcm_f32le",
        path(out)?,
    ])
}

/// Remux `clean_audio` onto `original` without re-encoding video.
pub fn remux_audio(original: &Path, clean_audio: &Path, out: &Path) -> Result<()> {
    run(&[
        "-y",
        "-i",
        path(original)?,
        "-i",
        path(clean_audio)?,
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-shortest",
        path(out)?,
    ])
}

/// Whether the input has a video stream.
pub fn has_video_stream(input: &Path) -> Result<bool> {
    let probe = std::env::var("SUKOON_FFPROBE").unwrap_or_else(|_| "ffprobe".to_string());
    let output = Command::new(probe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .arg(input)
        .output()
        .map_err(|e| Error::Ffmpeg(format!("failed to spawn ffprobe: {e}")))?;
    Ok(!output.stdout.is_empty())
}

/// Streaming AAC encoder fed raw interleaved `f32` PCM over a pipe.
pub struct PcmEncoder {
    child: Child,
    tx: Option<std::sync::mpsc::SyncSender<Vec<u8>>>,
    writer: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    label: &'static str,
}

impl PcmEncoder {
    /// Spawn an encoder that muxes the PCM stream onto `video_source`'s video stream and writes
    /// `out`. The stream on stdin is `channels`×`sample_rate` little-endian f32, interleaved.
    pub fn spawn_remux(
        video_source: &Path,
        channels: usize,
        sample_rate: u32,
        out: &Path,
    ) -> Result<Self> {
        let ar = sample_rate.to_string();
        let ac = channels.to_string();
        let child = Command::new(ffmpeg_bin())
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(["-i", path(video_source)?])
            .args(["-f", "f32le", "-ar", &ar, "-ac", &ac, "-i", "pipe:0"])
            .args([
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-shortest",
            ])
            .arg(path(out)?)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Ffmpeg(format!("failed to spawn ffmpeg encoder: {e}")))?;
        Self::wrap(child, "remux")
    }

    /// Spawn an audio-only encoder (input has no video stream): encode the PCM stream to `out`.
    pub fn spawn_audio(channels: usize, sample_rate: u32, out: &Path) -> Result<Self> {
        let ar = sample_rate.to_string();
        let ac = channels.to_string();
        let child = Command::new(ffmpeg_bin())
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(["-f", "f32le", "-ar", &ar, "-ac", &ac, "-i", "pipe:0"])
            .args(["-c:a", "aac", "-b:a", "192k"])
            .arg(path(out)?)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Ffmpeg(format!("failed to spawn ffmpeg encoder: {e}")))?;
        Self::wrap(child, "encode")
    }

    fn wrap(mut child: Child, label: &'static str) -> Result<Self> {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Ffmpeg(format!("ffmpeg {label} gave no stdin pipe")))?;
        // Bounded queue lets the producer run ahead without holding the whole stem.
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
        let writer = std::thread::spawn(move || -> std::io::Result<()> {
            use std::io::Write;
            for bytes in rx {
                stdin.write_all(&bytes)?;
            }
            stdin.flush()
        });
        Ok(Self {
            child,
            tx: Some(tx),
            writer: Some(writer),
            label,
        })
    }

    /// Feed one planar block's first `len` frames to the writer thread.
    pub fn write_planar(&mut self, block: &[Vec<f32>], len: usize) -> Result<()> {
        let mut bytes = Vec::with_capacity(len * block.len() * 4);
        for frame in 0..len {
            for plane in block {
                let s = plane.get(frame).copied().unwrap_or(0.0);
                bytes.extend_from_slice(&s.to_le_bytes());
            }
        }
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| Error::Ffmpeg("encoder input already closed".into()))?;
        tx.send(bytes).map_err(|_| {
            // The writer thread ended early — the encoder almost certainly died. Surface it.
            Error::Ffmpeg(format!("ffmpeg {} closed its input early", self.label))
        })
    }

    /// Close the input (EOF), flush the writer thread, and wait for the encoder to finish.
    pub fn finish(mut self) -> Result<()> {
        drop(self.tx.take()); // closes the channel → writer thread drains and exits
        if let Some(writer) = self.writer.take() {
            writer
                .join()
                .map_err(|_| {
                    Error::Ffmpeg(format!("ffmpeg {} writer thread panicked", self.label))
                })?
                .map_err(|e| Error::Ffmpeg(format!("write to ffmpeg {}: {e}", self.label)))?;
        }
        let status = self
            .child
            .wait()
            .map_err(|e| Error::Ffmpeg(format!("wait for ffmpeg {}: {e}", self.label)))?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Ffmpeg(format!(
                "ffmpeg {} exited with {status}",
                self.label
            )))
        }
    }
}

impl Drop for PcmEncoder {
    /// Reap the writer and child if an error path skips [`finish`](PcmEncoder::finish).
    fn drop(&mut self) {
        drop(self.tx.take());
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        let _ = self.child.wait();
    }
}

fn run(args: &[&str]) -> Result<()> {
    // Keep FFmpeg quiet and prevent it from grabbing the terminal.
    let status = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        .args(args)
        .status()
        .map_err(|e| Error::Ffmpeg(format!("failed to spawn ffmpeg: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Ffmpeg(format!("ffmpeg exited with {status}")))
    }
}

fn path(p: &Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| Error::Ffmpeg(format!("non-UTF-8 path: {}", p.display())))
}
