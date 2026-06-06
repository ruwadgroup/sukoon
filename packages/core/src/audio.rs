//! WAV decode/encode for pipeline audio I/O.

use std::path::Path;

use crate::{AudioBuffer, Error, Result};

/// Convert an integer PCM sample of `bits` width to `f32` in `[-1, 1)`.
fn int_to_f32(sample: i32, bits: u16) -> f32 {
    // hound sign-extends to i32; scale by the full-scale magnitude for the bit depth.
    let scale = 1i64 << (bits - 1);
    (sample as f64 / scale as f64) as f32
}

impl AudioBuffer {
    /// Read a WAV file into a planar `f32` buffer.
    ///
    /// Handles 16/24/32-bit integer PCM and 32-bit IEEE float, mono or multi-channel. Interleaved
    /// samples are deinterleaved into one `Vec` per channel.
    pub fn read_wav(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut reader = hound::WavReader::open(path)
            .map_err(|e| Error::Engine(format!("open wav {}: {e}", path.display())))?;
        let spec = reader.spec();
        let channels = spec.channels as usize;
        if channels == 0 {
            return Err(Error::Engine("wav has zero channels".into()));
        }

        // Collect interleaved samples as f32, then deinterleave.
        let interleaved: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .collect::<std::result::Result<_, _>>()
                .map_err(|e| Error::Engine(format!("read float wav: {e}")))?,
            hound::SampleFormat::Int => reader
                .samples::<i32>()
                .map(|s| s.map(|v| int_to_f32(v, spec.bits_per_sample)))
                .collect::<std::result::Result<_, _>>()
                .map_err(|e| Error::Engine(format!("read int wav: {e}")))?,
        };

        let frames = interleaved.len() / channels;
        let mut planar = vec![Vec::with_capacity(frames); channels];
        for (i, sample) in interleaved.into_iter().enumerate() {
            planar[i % channels].push(sample);
        }

        Ok(Self {
            channels: planar,
            sample_rate: spec.sample_rate,
        })
    }

    /// Resample this buffer to `target_rate`, preserving channel count.
    pub fn resample(&self, target_rate: u32) -> Result<AudioBuffer> {
        use rubato::{FftFixedIn, Resampler};

        let frames = self.frame_count();
        if target_rate == self.sample_rate || self.sample_rate == 0 || frames == 0 {
            return Ok(self.clone());
        }
        let channels = self.channel_count();

        // Feed fixed chunks, padding the final partial chunk with zeros.
        let chunk = 1024usize;
        let mut resampler = FftFixedIn::<f32>::new(
            self.sample_rate as usize,
            target_rate as usize,
            chunk,
            2,
            channels,
        )
        .map_err(|e| {
            Error::Engine(format!(
                "resampler init {}→{target_rate}Hz: {e}",
                self.sample_rate
            ))
        })?;

        let mut out: Vec<Vec<f32>> = vec![Vec::new(); channels];
        let mut inbuf: Vec<Vec<f32>> = vec![vec![0.0f32; chunk]; channels];
        let mut pos = 0usize;
        while pos < frames {
            let need = resampler.input_frames_next();
            for (ch, inbuf_ch) in inbuf.iter_mut().enumerate() {
                if inbuf_ch.len() != need {
                    inbuf_ch.resize(need, 0.0);
                }
                for (i, slot) in inbuf_ch.iter_mut().enumerate() {
                    *slot = self.channels[ch].get(pos + i).copied().unwrap_or(0.0);
                }
            }
            let produced = resampler
                .process(&inbuf, None)
                .map_err(|e| Error::Engine(format!("resample: {e}")))?;
            for (ch, plane) in produced.into_iter().enumerate() {
                out[ch].extend(plane);
            }
            pos += need;
        }

        // Keep output length predictable despite resampler latency.
        let expected = ((frames as u64 * target_rate as u64) / self.sample_rate as u64) as usize;
        for plane in &mut out {
            plane.resize(expected, 0.0);
        }

        Ok(AudioBuffer {
            channels: out,
            sample_rate: target_rate,
        })
    }

    /// Write this buffer to a 32-bit float WAV (lossless within the pipeline).
    pub fn write_wav(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let channels = self.channel_count();
        if channels == 0 {
            return Err(Error::Engine("cannot write wav with zero channels".into()));
        }
        let spec = hound::WavSpec {
            channels: channels as u16,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| Error::Engine(format!("create wav {}: {e}", path.display())))?;

        // Interleave channel planes back together.
        let frames = self.frame_count();
        for frame in 0..frames {
            for ch in &self.channels {
                writer
                    .write_sample(ch[frame])
                    .map_err(|e| Error::Engine(format!("write wav sample: {e}")))?;
            }
        }
        writer
            .finalize()
            .map_err(|e| Error::Engine(format!("finalize wav: {e}")))?;
        Ok(())
    }
}

/// Seekable block reader for bounded-memory WAV streaming.
pub struct WavBlockReader {
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    spec: hound::WavSpec,
    total_frames: u64,
}

impl WavBlockReader {
    /// Open a WAV for block reading.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let reader = hound::WavReader::open(path)
            .map_err(|e| Error::Engine(format!("open wav {}: {e}", path.display())))?;
        let spec = reader.spec();
        if spec.channels == 0 {
            return Err(Error::Engine("wav has zero channels".into()));
        }
        let total_frames = reader.len() as u64 / spec.channels as u64;
        Ok(Self {
            reader,
            spec,
            total_frames,
        })
    }

    /// Channel count.
    pub fn channel_count(&self) -> usize {
        self.spec.channels as usize
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.spec.sample_rate
    }

    /// Total number of frames in the file.
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// Read up to `len` frames starting at frame `start`, planar. Returns fewer frames if the
    /// request runs past the end of the file (the caller zero-pads as needed).
    pub fn read_range(&mut self, start: u64, len: usize) -> Result<AudioBuffer> {
        let channels = self.channel_count();
        let avail = self.total_frames.saturating_sub(start) as usize;
        let want = len.min(avail);
        let mut planar = vec![vec![0.0f32; 0]; channels];
        for p in &mut planar {
            p.reserve(want);
        }
        if want == 0 {
            return Ok(AudioBuffer {
                channels: planar,
                sample_rate: self.spec.sample_rate,
            });
        }

        self.reader
            .seek(start as u32)
            .map_err(|e| Error::Engine(format!("wav seek to {start}: {e}")))?;

        let take = want * channels;
        let mut idx = 0usize;
        match self.spec.sample_format {
            hound::SampleFormat::Float => {
                for s in self.reader.samples::<f32>().take(take) {
                    let v = s.map_err(|e| Error::Engine(format!("read float wav: {e}")))?;
                    planar[idx % channels].push(v);
                    idx += 1;
                }
            }
            hound::SampleFormat::Int => {
                let bits = self.spec.bits_per_sample;
                for s in self.reader.samples::<i32>().take(take) {
                    let v = s.map_err(|e| Error::Engine(format!("read int wav: {e}")))?;
                    planar[idx % channels].push(int_to_f32(v, bits));
                    idx += 1;
                }
            }
        }
        Ok(AudioBuffer {
            channels: planar,
            sample_rate: self.spec.sample_rate,
        })
    }
}

/// A streaming 32-bit-float WAV writer for bounded-memory output: write speech blocks as they're
/// produced instead of buffering the whole stem in RAM.
pub struct WavBlockWriter {
    writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
}

impl WavBlockWriter {
    /// Create a writer for `channels` planes at `sample_rate`.
    pub fn create(path: impl AsRef<Path>, channels: usize, sample_rate: u32) -> Result<Self> {
        let path = path.as_ref();
        let spec = hound::WavSpec {
            channels: channels as u16,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let writer = hound::WavWriter::create(path, spec)
            .map_err(|e| Error::Engine(format!("create wav {}: {e}", path.display())))?;
        Ok(Self { writer })
    }

    /// Append the first `len` frames of a planar block, interleaving channels.
    pub fn write_block(&mut self, block: &[Vec<f32>], len: usize) -> Result<()> {
        for frame in 0..len {
            for ch in block {
                self.writer
                    .write_sample(ch.get(frame).copied().unwrap_or(0.0))
                    .map_err(|e| Error::Engine(format!("write wav sample: {e}")))?;
            }
        }
        Ok(())
    }

    /// Flush and finalize the WAV header.
    pub fn finalize(self) -> Result<()> {
        self.writer
            .finalize()
            .map_err(|e| Error::Engine(format!("finalize wav: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("sukoon-wavtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rt.wav");

        // A 2-channel ramp/sine so we exercise both planes.
        let frames = 1000;
        let mut buf = AudioBuffer::silent(2, frames, 48_000);
        for i in 0..frames {
            buf.channels[0][i] = (i as f32 / frames as f32) * 2.0 - 1.0; // ramp
            buf.channels[1][i] = (i as f32 * 0.1).sin() * 0.5; // sine
        }

        buf.write_wav(&path).unwrap();
        let back = AudioBuffer::read_wav(&path).unwrap();

        assert_eq!(back.channel_count(), 2);
        assert_eq!(back.frame_count(), frames);
        assert_eq!(back.sample_rate, 48_000);
        for ch in 0..2 {
            for i in 0..frames {
                assert!((back.channels[ch][i] - buf.channels[ch][i]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn resample_same_rate_is_identity() {
        let mut buf = AudioBuffer::silent(1, 256, 44_100);
        for i in 0..256 {
            buf.channels[0][i] = (i as f32 * 0.1).sin();
        }
        let out = buf.resample(44_100).unwrap();
        assert_eq!(out.sample_rate, 44_100);
        assert_eq!(
            out.channels, buf.channels,
            "same-rate resample must be byte-identical"
        );
    }

    #[test]
    fn resample_preserves_frequency_and_length() {
        // 1 kHz sine at 48 kHz for 1 s, downsampled to 44.1 kHz. Frequency must be preserved and
        // the frame count must scale by the rate ratio (within a few FFT-block frames).
        let src_rate = 48_000usize;
        let dst_rate = 44_100usize;
        let n = src_rate; // 1 s
        let freq = 1_000.0f32;
        let mut buf = AudioBuffer::silent(1, n, src_rate as u32);
        for i in 0..n {
            buf.channels[0][i] =
                (2.0 * std::f32::consts::PI * freq * i as f32 / src_rate as f32).sin();
        }

        let out = buf.resample(dst_rate as u32).unwrap();
        assert_eq!(out.sample_rate, dst_rate as u32);

        let expected = (n as f64 * dst_rate as f64 / src_rate as f64).round() as usize;
        let got = out.frame_count();
        assert!(
            (got as i64 - expected as i64).abs() < 4096,
            "length {got} not within a few blocks of {expected}"
        );

        // Count zero crossings (rising) in the steady middle region → estimate frequency.
        let plane = &out.channels[0];
        let lo = plane.len() / 4;
        let hi = plane.len() * 3 / 4;
        let mut crossings = 0;
        for i in lo + 1..hi {
            if plane[i - 1] <= 0.0 && plane[i] > 0.0 {
                crossings += 1;
            }
        }
        let span_secs = (hi - lo) as f32 / dst_rate as f32;
        let est = crossings as f32 / span_secs;
        assert!(
            (est - freq).abs() / freq < 0.02,
            "estimated {est} Hz too far from {freq} Hz"
        );
    }

    #[test]
    fn resample_round_trip_length() {
        // 48k → 16k → 48k returns to ~original length with bounded amplitude.
        let n = 48_000usize;
        let mut buf = AudioBuffer::silent(2, n, 48_000);
        for i in 0..n {
            let s = (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48_000.0).sin() * 0.7;
            buf.channels[0][i] = s;
            buf.channels[1][i] = s;
        }
        let down = buf.resample(16_000).unwrap();
        let up = down.resample(48_000).unwrap();
        assert!(
            (up.frame_count() as i64 - n as i64).abs() < 8192,
            "round-trip length {} drifted from {n}",
            up.frame_count()
        );
        let peak = up.channels[0].iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!(peak < 1.05, "round-trip peak {peak} unexpectedly large");
    }
}
