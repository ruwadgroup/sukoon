//! Pipeline-level tests that don't require FFmpeg or model weights (dry mode).

use sukoon_core::engine::EngineKind;
use sukoon_core::{AudioBuffer, Pipeline, PipelineOptions, SeparationMode};

#[test]
fn engine_kind_round_trips_through_ids() {
    for kind in [EngineKind::Fast, EngineKind::Hq, EngineKind::Fallback] {
        assert_eq!(EngineKind::from_id(kind.id()), Some(kind));
    }
    assert_eq!(EngineKind::from_id("dfn"), Some(EngineKind::Fast));
    assert_eq!(EngineKind::from_id("nope"), None);
}

// These build real engines. In dry mode (no `onnx` feature) that's free; with `onnx` it would
// download/verify real weights, which is an integration concern, not a unit test — so they're
// scoped to dry mode.
#[cfg(not(feature = "onnx"))]
#[test]
fn pipeline_loads_each_engine_in_dry_mode() {
    for kind in [EngineKind::Fast, EngineKind::Hq, EngineKind::Fallback] {
        let pipeline = Pipeline::new(PipelineOptions {
            engine: kind,
            mode: SeparationMode::RemoveAll,
            use_cache: false,
        })
        .expect("engine should load in dry mode");
        assert_eq!(pipeline.engine_id(), kind.id());
    }
}

#[cfg(not(feature = "onnx"))]
#[test]
fn only_fast_engine_is_realtime() {
    let fast = Pipeline::new(PipelineOptions {
        engine: EngineKind::Fast,
        ..Default::default()
    })
    .unwrap();
    let hq = Pipeline::new(PipelineOptions {
        engine: EngineKind::Hq,
        ..Default::default()
    })
    .unwrap();
    assert!(fast.realtime_capable());
    assert!(!hq.realtime_capable());
}

#[test]
fn audio_buffer_geometry() {
    let buf = AudioBuffer::silent(2, 44_100, 44_100);
    assert_eq!(buf.channel_count(), 2);
    assert_eq!(buf.frame_count(), 44_100);
    assert!((buf.duration_secs() - 1.0).abs() < 1e-9);
}
