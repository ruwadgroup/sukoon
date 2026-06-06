# sukoon-core

The shared audio engine behind Sukoon's file tools. **This is the only place separation logic
lives** — the CLI, desktop, and mobile shells call into it. (The browser extension is the exception:
it runs DeepFilterNet via its own `@sukoon/dfn-wasm` build, not core.)

It owns the full pipeline:

```
decode ─► extract audio (FFmpeg) ─► separate (Engine) ─► keep speech, drop music ─► remux (FFmpeg)
```

## Concepts

- [`Pipeline`] — the one entry point. Construct with [`PipelineOptions`], call `clean_file`.
- [`Engine`] — the separation trait. Implementations: `MdxNet` (HQ default, **and** the low-RAM
  `mdx-lite` fallback — same engine, smaller model), and `DeepFilterNet` (Fast, the real-time speech
  enhancer behind the `dfn` feature; a passthrough stub without it).
- [`registry`] — model URLs, checksums, and **licenses** (so builds won't bundle a share-alike weight).
- [`cache`] — content-hash keyed cache of cleaned stems.

## Usage

```rust
use sukoon_core::{Pipeline, PipelineOptions, EngineKind, SeparationMode};

let pipeline = Pipeline::new(PipelineOptions {
    engine: EngineKind::Hq, // MDX-Net — the working engine
    mode: SeparationMode::RemoveAll,
    use_cache: true,
})?;

pipeline.clean_file("vlog.mp4", "vlog.clean.mp4")?;
# Ok::<(), sukoon_core::Error>(())
```

## Features

| Feature | Default | Effect                                                                                                                                                                                                            |
| ------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `onnx`  | off     | Enables real ONNX Runtime inference (MDX-Net HQ + `mdx-lite` fallback, STFT front-end, first-use weight download). Without it, engines run in **dry passthrough** mode so shells and tests build without weights. |
| `dfn`   | off     | Enables the real-time **DeepFilterNet** Fast engine (`deep_filter` DSP + three DFN3 ONNX graphs via ORT). Implies `onnx`; without it the Fast engine is a passthrough stub.                                       |
| `cuda`  | off     | Adds the NVIDIA CUDA execution provider (opt-in; default build stays pure-CPU). Implies `onnx`.                                                                                                                   |

```bash
cargo build -p sukoon-core            # dry mode (no model weights needed)
cargo build -p sukoon-core --features onnx   # real MDX inference (HQ + fallback)
cargo build -p sukoon-core --features dfn    # also the real-time DeepFilterNet Fast engine
```

## Runtime requirements

- An **FFmpeg** binary on `PATH` (or set `SUKOON_FFMPEG` / `SUKOON_FFPROBE`). Ship an **LGPL**
  build — see [LICENSING.md](../../LICENSING.md).
- Model weights download on first use to `SUKOON_MODELS_DIR` (defaults to a temp dir).

## Status

Alpha. The pipeline is real end-to-end (decode → separate → encode → remux, with a content cache and
[`Pipeline::on_progress`] events). The **HQ engine (MDX-Net)** and the **low-RAM `mdx-lite`
fallback** run genuine ONNX inference behind `--features onnx`; the real-time **Fast engine
(DeepFilterNet)** runs behind `--features dfn` (`deep_filter` DSP + DFN3 ONNX graphs via ORT). MDX
inference **auto-selects the platform GPU** (CoreML/DirectML/CUDA) with CPU fallback; DeepFilterNet
is **CPU-preferred** (faster than the GPU for this small recurrent model) — all no-configuration. The
DeepFilterNet path is functionally validated but not yet bit-exact-verified against the upstream
reference. See [the engine roadmap](../../ROADMAP.md#engine-roadmap),
[docs/architecture/engines.md](../../docs/architecture/engines.md), and
[docs/reference/performance.md](../../docs/reference/performance.md) for speeds and the device matrix.
