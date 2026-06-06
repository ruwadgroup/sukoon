# Core (`sukoon-core`)

The Rust crate that owns separation. Source: [`packages/core`](../../packages/core).

## Key types

| Type              | Role                                                                                                                            |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `AudioBuffer`     | Planar f32 samples + sample rate. The currency between FFmpeg and engines.                                                      |
| `Engine` (trait)  | A separation backend. `separate(&AudioBuffer) -> Separation`.                                                                   |
| `Separation`      | The output stems: `speech` (kept), optional `music`/`effects`.                                                                  |
| `EngineKind`      | `Fast` / `Hq` / `Fallback` → builds the concrete engine.                                                                        |
| `Pipeline`        | Orchestration: decode → separate → remux. The one entry point for shells.                                                       |
| `PipelineOptions` | `engine`, `mode`, `use_cache`.                                                                                                  |
| `Progress`        | Pipeline stage events (`Extract` / `Separate { chunk, total }` / `Encode` / `Remux` / `Done`), via `Pipeline::on_progress(cb)`. |
| `SeparationMode`  | `RemoveAll` (default) / `KeepVocals`; `KeepPercussion` / `PreserveEffects` are placeholders.                                    |

## Lifecycle

```rust
let pipeline = Pipeline::new(PipelineOptions {
    engine: EngineKind::Hq, // MDX-Net — the working engine
    mode: SeparationMode::RemoveAll,
    use_cache: true,
})?;
pipeline.clean_file("in.mp4", "out.mp4")?;
```

`Pipeline::new` loads the engine **once** (the model stays resident). Reuse one pipeline across many
files in a batch — don't reconstruct per file. On first use the engine's weights **download
automatically** with a live progress indicator (downloaded/total MB, %); nothing is bundled.

Long files are processed with **bounded-memory block streaming** (peak RAM ≈ one block, independent
of file length), and the cleaned PCM is streamed into FFmpeg as it's produced so the audio re-encode
overlaps separation rather than adding to it. See [pipeline.md](./pipeline.md#chunking--bounded-memory-streaming)
and [performance numbers](../reference/performance.md).

## Dry mode vs `onnx`

Without the `onnx` feature, engines run as **passthrough** (speech = input). This lets shells,
tests, and CI build and run without model weights. Enable real inference with
`--features sukoon-core/onnx` (or `--features onnx` on `sukoon-cli`): the **HQ / MDX-Net** engine and
the **low-RAM `mdx-lite` Fallback** then do genuine separation. The **Fast / DeepFilterNet** engine
needs the **`dfn`** feature (which implies `onnx`); without it the Fast engine stays a passthrough
stub. See [engines.md](./engines.md).

## Caching

`cache::CacheKey` is a SHA-256 over **input bytes + engine id + mode id**. Changing any of the three
is a cache miss, so switching engine or mode never serves a stale result. The default cache lives at
`SUKOON_CACHE_DIR` (temp dir otherwise) and is **local-only**, for privacy. (This result cache is
separate from the downloaded model weights.)

## Error model

One `Error` enum (`Ffmpeg`, `Engine`, `ModelUnavailable`, `Io`, `Other`) with a `Result<T>` alias.
Shells surface these to users; the CLI adds context with `anyhow`.
