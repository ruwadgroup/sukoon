//! End-to-end integration test for the real ONNX pipeline.
//!
//! Unlike the unit tests, this drives `Pipeline::clean_file` through the whole flow
//! (FFmpeg extract → decode → MDX inference → encode → write). It needs the MDX weights and an
//! `ffmpeg`/`ffprobe` on `PATH`, so it's `#[ignore]` by default — run it explicitly:
//!
//!   SUKOON_MODELS_DIR=/tmp/sukoon-models \
//!   cargo test -p sukoon-core --features onnx --test e2e_onnx -- --ignored --nocapture
#![cfg(feature = "onnx")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sukoon_core::{AudioBuffer, EngineKind, Pipeline, PipelineOptions, Progress, SeparationMode};

/// Where the MDX weights live; mirrors the registry default override.
fn model_present() -> bool {
    let dir = std::env::var_os("SUKOON_MODELS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("sukoon").join("models"));
    dir.join("mdx").join("model.onnx").exists()
}

/// Whether the DeepFilterNet bundle has been extracted locally.
fn dfn_present() -> bool {
    let dir = std::env::var_os("SUKOON_MODELS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("sukoon").join("models"));
    dir.join("deepfilternet").join("enc.onnx").exists()
}

fn ffmpeg_present() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "needs MDX weights + ffmpeg on PATH"]
fn mdx_cleans_real_audio_with_progress() {
    if !model_present() || !ffmpeg_present() {
        eprintln!("skipping: MDX weights or ffmpeg unavailable");
        return;
    }

    let dir = std::env::temp_dir().join(format!("sukoon-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");

    // A 3 s stereo mix of tones — enough to span several MDX chunks at 44.1 kHz.
    let sr = 44_100;
    let n = sr * 3;
    let mut buf = AudioBuffer::silent(2, n, sr as u32);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.3
            + (2.0 * std::f32::consts::PI * 660.0 * t).sin() * 0.2;
        buf.channels[0][i] = s;
        buf.channels[1][i] = s;
    }
    buf.write_wav(&input).unwrap();

    // Track progress events delivered during the run.
    let seen_separate = Arc::new(AtomicUsize::new(0));
    let saw_done = Arc::new(AtomicUsize::new(0));
    let (s1, s2) = (seen_separate.clone(), saw_done.clone());

    let pipeline = Pipeline::new(PipelineOptions {
        engine: EngineKind::Hq,
        mode: SeparationMode::RemoveAll,
        use_cache: false,
    })
    .unwrap()
    .on_progress(move |p| match p {
        Progress::Separate { chunk, total } => {
            assert!(chunk < total, "chunk {chunk} should be < total {total}");
            s1.fetch_add(1, Ordering::SeqCst);
        }
        Progress::Done => {
            s2.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    });

    pipeline
        .clean_file(&input, &output)
        .expect("clean_file failed");

    // The cleaned WAV exists, is non-empty, and decodes to the same geometry/rate.
    let cleaned = AudioBuffer::read_wav(&output).expect("read cleaned wav");
    assert_eq!(cleaned.sample_rate, sr as u32);
    assert!(cleaned.frame_count() > 0, "cleaned audio is empty");
    // Output length within a chunk of the input (MDX crops back to the original length).
    assert!(
        (cleaned.frame_count() as i64 - n as i64).abs() < sr as i64,
        "cleaned length {} drifted from {n}",
        cleaned.frame_count()
    );

    assert!(
        seen_separate.load(Ordering::SeqCst) > 0,
        "no Separate progress events"
    );
    assert_eq!(
        saw_done.load(Ordering::SeqCst),
        1,
        "expected exactly one Done event"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs DeepFilterNet weights"]
fn dfn_streamed_output_matches_whole_buffer() {
    // The overlapped pipeline streams DFN's speech stem out window-by-window via `separate_into`,
    // which must be byte-identical to the whole-buffer `separate` — the overlap is a wall-clock
    // optimization, not an audio change. Verify the two produce exactly the same samples.
    if !dfn_present() {
        eprintln!("skipping: DeepFilterNet weights unavailable");
        return;
    }

    let engine = EngineKind::Fast.build().unwrap();

    // ~12 s stereo mix — spans several `NET_CHUNK` windows so chunk joins are exercised.
    let sr = 48_000usize;
    let n = sr * 12;
    let mut sig = AudioBuffer::silent(2, n, sr as u32);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * 180.0 * t).sin() * 0.3
            + (2.0 * std::f32::consts::PI * 2_000.0 * t).sin() * 0.15;
        sig.channels[0][i] = s;
        sig.channels[1][i] = s * 0.8;
    }

    // Whole-buffer reference.
    let reference = engine.separate(&sig).unwrap().speech;

    // Streamed: collect every emitted window into channel planes.
    let mut streamed: Vec<Vec<f32>> = Vec::new();
    let mut noop = |_: usize, _: usize| {};
    engine
        .separate_into(&sig, &mut noop, &mut |block, len| {
            if streamed.is_empty() {
                streamed = vec![Vec::new(); block.len()];
            }
            for (dst, src) in streamed.iter_mut().zip(block) {
                dst.extend_from_slice(&src[..len]);
            }
            Ok(())
        })
        .unwrap();

    assert_eq!(
        streamed.len(),
        reference.channels.len(),
        "channel count differs"
    );
    for c in 0..streamed.len() {
        assert_eq!(
            streamed[c].len(),
            reference.channels[c].len(),
            "channel {c} length differs"
        );
        for i in 0..streamed[c].len() {
            assert_eq!(
                streamed[c][i], reference.channels[c][i],
                "sample {i} of channel {c} differs (streamed vs whole-buffer)"
            );
        }
    }
}

#[test]
#[ignore = "needs MDX weights"]
fn streaming_matches_whole_buffer() {
    if !model_present() {
        eprintln!("skipping: MDX weights unavailable");
        return;
    }

    let engine = EngineKind::Hq.build().unwrap();
    let plan = engine.chunk_plan().expect("MDX must expose a chunk plan");
    let trim = plan.context;
    let align = plan.align;

    // ~20 s stereo mix — several `align` blocks, so multiple boundary joins are exercised.
    let sr = 44_100usize;
    let n = sr * 20;
    let mut sig = AudioBuffer::silent(2, n, sr as u32);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * 200.0 * t).sin() * 0.3
            + (2.0 * std::f32::consts::PI * 700.0 * t).sin() * 0.2;
        sig.channels[0][i] = s;
        sig.channels[1][i] = s * 0.9; // make the two channels differ
    }

    // Reference: whole-buffer separation.
    let reference = engine.separate(&sig).unwrap().speech;

    // Streaming: emit one `align` block at a time, feeding the surrounding `trim` context.
    let total_emit = n.div_ceil(align) * align;
    let mut streamed = vec![vec![0.0f32; 0]; 2];
    let mut e = 0usize;
    while e < total_emit {
        let emit = align.min(total_emit - e);
        let win_len = emit + 2 * trim;
        let mut win = AudioBuffer::silent(2, win_len, sr as u32);
        for c in 0..2 {
            for j in 0..win_len {
                let idx = e as i64 - trim as i64 + j as i64;
                if idx >= 0 && (idx as usize) < n {
                    win.channels[c][j] = sig.channels[c][idx as usize];
                }
            }
        }
        let block = engine.separate_block(&win, trim, emit).unwrap();
        for c in 0..2 {
            streamed[c].extend_from_slice(&block.channels[c][..emit]);
        }
        e += emit;
    }

    // Compare the central `n` frames.
    let mut max_err = 0.0f32;
    for c in 0..2 {
        for i in 0..n {
            max_err = max_err.max((reference.channels[c][i] - streamed[c][i]).abs());
        }
    }
    assert!(
        max_err < 1e-4,
        "streaming diverged from whole-buffer: max abs error {max_err}"
    );
}
