# Configuration

Sukoon is configured mostly by environment variables — there's no config file to maintain for the
core/CLI.

## Environment variables

| Variable            | Default               | Used by | Purpose                                                                              |
| ------------------- | --------------------- | ------- | ------------------------------------------------------------------------------------ |
| `SUKOON_FFMPEG`     | `ffmpeg` (on `PATH`)  | core    | Path to the ffmpeg binary.                                                           |
| `SUKOON_FFPROBE`    | `ffprobe` (on `PATH`) | core    | Path to the ffprobe binary.                                                          |
| `SUKOON_MODELS_DIR` | `<tmp>/sukoon/models` | core    | Where model weights are cached (downloaded on first use).                            |
| `SUKOON_CACHE_DIR`  | `<tmp>/sukoon/cache`  | core    | Where cleaned stems are cached (local).                                              |
| `SUKOON_CPU_ONLY`   | unset                 | core    | Set to `1` to force CPU inference (support escape hatch if a GPU driver misbehaves). |
| `RUST_LOG`          | `sukoon=info`         | cli     | Log filter (`sukoon=debug`, `trace`, …).                                             |

> **Performance is automatic.** The core sizes its thread pool and selects the best hardware
> accelerator for the platform itself (CoreML on macOS, DirectML on Windows, CUDA on Linux with
> `--features cuda`) — there are no tuning knobs. `SUKOON_CPU_ONLY` is the only override, and only
> exists for support. See [performance & device support](./performance.md) for the figures and the
> acceleration matrix.

## Cargo features

| Feature | Crate       | Effect                                                                                                                                                                                                |
| ------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `onnx`  | sukoon-core | Enables real ONNX inference — the HQ MDX-Net engine and the `mdx-lite` fallback (else dry passthrough). Weights download on first use.                                                                |
| `dfn`   | sukoon-core | Enables the real DeepFilterNet **Fast** engine (`deep_filter` DSP + three DFN3 ONNX graphs via ORT). Implies `onnx`; without it the Fast engine is a passthrough stub. Weights download on first use. |
| `cuda`  | sukoon-core | Adds the NVIDIA CUDA execution provider (Linux/Windows + NVIDIA GPU). Opt-in; default build is pure-CPU.                                                                                              |

```bash
cargo build -p sukoon-cli --features onnx     # HQ + fallback MDX engines
cargo build -p sukoon-cli --features dfn      # also the real-time Fast (DeepFilterNet) engine
```

## Pipeline options

Set programmatically via `PipelineOptions { engine, mode, use_cache }`, or from the CLI via
`--engine` / `--mode` / `--no-cache`. See [core-api.md](./core-api.md) and [cli.md](./cli.md).
