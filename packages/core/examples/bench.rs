//! Engine micro-benchmark (dev tooling, not shipped).
//!
//! Isolates the separation hot path from FFmpeg I/O: read a WAV, run the engine N times, report
//! per-run wall time and the real-time factor. The engine auto-selects its accelerator; set
//! `SUKOON_CPU_ONLY=1` to force the CPU path for an apples-to-apples comparison. Usage:
//!
//!   SUKOON_MODELS_DIR=/tmp/sukoon-models \
//!   cargo run -p sukoon-core --release --features onnx --example bench -- samples/orig.wav [runs]

use std::time::Instant;

use sukoon_core::{AudioBuffer, EngineKind};

fn engine_kind() -> EngineKind {
    match std::env::var("SUKOON_BENCH_ENGINE").as_deref() {
        Ok(s) => EngineKind::from_id(s).unwrap_or(EngineKind::Hq),
        _ => EngineKind::Hq,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "samples/orig.wav".into());
    let runs: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);

    let buf = AudioBuffer::read_wav(&path)?;
    let secs = buf.frame_count() as f64 / buf.sample_rate as f64;
    println!(
        "input: {path}  ({:.1}s, {} ch, {} Hz)",
        secs,
        buf.channel_count(),
        buf.sample_rate
    );

    let t0 = Instant::now();
    let engine = engine_kind().build()?;
    println!(
        "engine `{}` loaded in {:.2}s",
        engine.id(),
        t0.elapsed().as_secs_f64()
    );

    let mut best = f64::INFINITY;
    let mut last = None;
    for i in 0..runs {
        let t = Instant::now();
        let sep = engine.separate(&buf)?;
        let dt = t.elapsed().as_secs_f64();
        best = best.min(dt);
        println!("run {i}: {dt:.2}s  ({:.2}x realtime)", secs / dt);
        last = Some(sep);
    }
    println!("best: {best:.2}s  ({:.2}x realtime)", secs / best);

    // Optionally write the speech stem so two runs (e.g. CPU vs CoreML) can be diffed for quality.
    if let (Some(out), Some(sep)) = (std::env::var_os("SUKOON_OUT"), last) {
        sep.speech.write_wav(&out)?;
        println!("wrote speech stem -> {}", out.to_string_lossy());
    }
    Ok(())
}
