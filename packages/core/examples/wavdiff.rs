//! Compare two WAVs sample-for-sample (dev tooling). Reports each signal's RMS and the RMS of
//! their difference in dBFS, plus the residual relative to signal A — a proxy for "are these two
//! separations effectively identical". Usage:
//!
//!   cargo run -p sukoon-core --release --features onnx --example wavdiff -- a.wav b.wav

use sukoon_core::AudioBuffer;

fn rms(planes: &[Vec<f32>]) -> f64 {
    let mut sum = 0.0f64;
    let mut n = 0usize;
    for ch in planes {
        for &s in ch {
            sum += (s as f64) * (s as f64);
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        (sum / n as f64).sqrt()
    }
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let a = AudioBuffer::read_wav(args.next().ok_or("need a.wav")?)?;
    let b = AudioBuffer::read_wav(args.next().ok_or("need b.wav")?)?;

    let ch = a.channel_count().min(b.channel_count());
    let frames = a.frame_count().min(b.frame_count());
    let diff: Vec<Vec<f32>> = (0..ch)
        .map(|c| {
            (0..frames)
                .map(|i| a.channels[c][i] - b.channels[c][i])
                .collect()
        })
        .collect();

    let rms_a = rms(&a.channels);
    let rms_b = rms(&b.channels);
    let rms_d = rms(&diff);
    println!("A   RMS: {:.2} dBFS", db(rms_a));
    println!("B   RMS: {:.2} dBFS", db(rms_b));
    println!("A-B RMS: {:.2} dBFS", db(rms_d));
    println!("residual relative to A: {:.2} dB", db(rms_d) - db(rms_a));
    Ok(())
}
