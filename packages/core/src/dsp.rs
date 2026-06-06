//! STFT / ISTFT front-end for the MDX-Net engine.
//!
//! MDX-Net consumes a complex spectrogram produced exactly like `torch.stft(center=True)` with a
//! periodic Hann window, and its output is inverted back with `torch.istft`. We replicate that:
//! reflect-padding by `n_fft/2`, hop framing, and overlap-add synthesis normalized by the summed
//! squared window (the COLA condition), so an STFT→ISTFT round-trip is the identity to within FFT
//! precision.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// A reusable STFT/ISTFT plan for a fixed `(n_fft, hop)`.
pub struct Stft {
    n_fft: usize,
    hop: usize,
    window: Vec<f32>,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
}

impl Stft {
    /// Number of frequency bins (`n_fft / 2 + 1`).
    pub fn n_bins(&self) -> usize {
        self.n_fft / 2 + 1
    }

    /// Build a plan with a periodic Hann window of length `n_fft`.
    pub fn new(n_fft: usize, hop: usize) -> Self {
        // Periodic Hann (matches torch's `periodic=True`): denominator is `n_fft`, not `n_fft-1`.
        let window = (0..n_fft)
            .map(|i| {
                let x = std::f32::consts::PI * i as f32 / n_fft as f32;
                x.sin().powi(2)
            })
            .collect();
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n_fft);
        let inv = planner.plan_fft_inverse(n_fft);
        Self {
            n_fft,
            hop,
            window,
            fwd,
            inv,
        }
    }

    /// Number of frames `torch.stft(center=True)` produces for a signal of `len` samples.
    pub fn num_frames(&self, len: usize) -> usize {
        len / self.hop + 1
    }

    /// Forward STFT of a mono signal. Returns a row-major `[frames][n_bins]` complex spectrogram.
    ///
    /// `center=True`: the signal is reflect-padded by `n_fft/2` on each side before framing, so
    /// frame `t` is centered on sample `t*hop`.
    pub fn forward(&self, signal: &[f32]) -> Vec<Vec<Complex32>> {
        let pad = self.n_fft / 2;
        let padded = reflect_pad(signal, pad);
        let frames = self.num_frames(signal.len());
        let mut scratch_in = self.fwd.make_input_vec();
        let mut out = Vec::with_capacity(frames);

        for f in 0..frames {
            let start = f * self.hop;
            for i in 0..self.n_fft {
                scratch_in[i] = padded[start + i] * self.window[i];
            }
            let mut spectrum = self.fwd.make_output_vec();
            self.fwd
                .process(&mut scratch_in, &mut spectrum)
                .expect("rfft forward");
            out.push(spectrum);
        }
        out
    }

    /// Inverse STFT back to a mono signal of length `out_len`, undoing [`forward`](Self::forward).
    pub fn inverse(&self, spec: &[Vec<Complex32>], out_len: usize) -> Vec<f32> {
        let pad = self.n_fft / 2;
        let padded_len = out_len + 2 * pad;
        let mut ola = vec![0.0f32; padded_len];
        let mut wsum = vec![0.0f32; padded_len];
        let mut scratch_out = self.inv.make_output_vec();

        for (f, frame) in spec.iter().enumerate() {
            let mut spectrum = frame.clone();
            // realfft requires the imaginary parts of the DC and Nyquist bins to be zero.
            spectrum[0].im = 0.0;
            if let Some(last) = spectrum.last_mut() {
                last.im = 0.0;
            }
            self.inv
                .process(&mut spectrum, &mut scratch_out)
                .expect("rfft inverse");
            let start = f * self.hop;
            for i in 0..self.n_fft {
                // realfft's inverse is unnormalized (scaled by n_fft); fold the 1/n_fft in here.
                let s = scratch_out[i] / self.n_fft as f32;
                ola[start + i] += s * self.window[i];
                wsum[start + i] += self.window[i] * self.window[i];
            }
        }

        // Normalize by the summed squared window (COLA) and strip the center padding.
        let mut out = vec![0.0f32; out_len];
        for i in 0..out_len {
            let w = wsum[pad + i];
            out[i] = if w > 1e-8 { ola[pad + i] / w } else { 0.0 };
        }
        out
    }
}

/// Reflect-pad a signal by `pad` samples on each side (matching torch's default `center` padding).
fn reflect_pad(signal: &[f32], pad: usize) -> Vec<f32> {
    let n = signal.len();
    let mut out = Vec::with_capacity(n + 2 * pad);
    // Left: reflect without repeating the edge sample (signal[pad], ..., signal[1]).
    for i in 0..pad {
        let idx = reflect_index(i as isize - pad as isize, n);
        out.push(signal[idx]);
    }
    out.extend_from_slice(signal);
    for i in 0..pad {
        let idx = reflect_index((n + i) as isize, n);
        out.push(signal[idx]);
    }
    out
}

/// Reflect an index into `[0, n)` the way `np.pad(mode="reflect")` does (no edge repeat).
fn reflect_index(mut i: isize, n: usize) -> usize {
    if n == 1 {
        return 0;
    }
    let period = 2 * (n as isize - 1);
    i = ((i % period) + period) % period;
    if i >= n as isize {
        (period - i) as usize
    } else {
        i as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stft_istft_round_trips() {
        // Smaller params than MDX so the test is fast, same framing math.
        let n_fft = 512;
        let hop = 128;
        let stft = Stft::new(n_fft, hop);

        let len = 4096;
        let signal: Vec<f32> = (0..len)
            .map(|i| (i as f32 * 0.05).sin() * 0.6 + (i as f32 * 0.013).sin() * 0.3)
            .collect();

        let spec = stft.forward(&signal);
        assert_eq!(spec.len(), stft.num_frames(len));
        assert_eq!(spec[0].len(), stft.n_bins());

        let recon = stft.inverse(&spec, len);
        assert_eq!(recon.len(), len);

        // Interior samples reconstruct cleanly (edges are dominated by padding/COLA roll-off).
        let mut max_err = 0.0f32;
        for i in n_fft..(len - n_fft) {
            max_err = max_err.max((recon[i] - signal[i]).abs());
        }
        assert!(max_err < 1e-3, "round-trip error too large: {max_err}");
    }
}
