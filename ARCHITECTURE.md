# Architecture

Sukoon is a **monorepo with one engine and many shells**. Everything that decides _what audio
to keep and what to remove_ lives in one place — `sukoon-core` — and every platform is a thin
adapter that feeds bytes into it and renders the result.

This document is the map. Per-area depth lives under [`docs/architecture/`](./docs/architecture).

```
                         ┌────────────────────────────────────────────┐
                         │              sukoon-core (Rust)            │
                         │                                            │
   video / audio  ─────► │  decode ─► extract audio (FFmpeg)          │
   stream                │     │                                      │
                         │     ▼                                      │
                         │  Engine trait ──┬─ MDX-Net (hq) ✅working  │
                         │   (ONNX Runtime) ├─ MDX-lite (fallback) ✅  │
                         │     │            └─ DeepFilterNet ✅        │
                         │     ▼                                      │
                         │  keep speech stem · drop music stem        │
                         │     │                                      │
                         │     ▼                                      │
                         │  remux clean audio (FFmpeg, -c:v copy)     │
                         └───────────────┬────────────────────────────┘
                                         │  FFI / CLI / WASM / HTTP
        ┌────────────────┬───────────────┼───────────────┐
        ▼                ▼               ▼               ▼
   sukoon-cli       extension         desktop         mobile
   (native)         (MV3, real-time   (Tauri, files)  (JNI / Core ML)
                     DeepFilterNet)
```

> The extension is the one shell that does **not** embed `sukoon-core`: a real-time browser separator
> in Rust→WASM isn't there yet, so it runs DeepFilterNet via the `@sukoon/dfn-wasm` build. Everything
> that processes **files** (CLI, desktop) goes through core.

## The one rule

> **Separation logic is implemented exactly once, in `sukoon-core`. No shell reimplements it.**

A shell may choose _which_ engine to run and _how_ to present
progress — but the pipeline (decode → separate → remux) is core's job. When core improves,
every platform improves with a version bump.

## Components

| Component     | Language           | Consumes core via                     | Responsibility                                                   |
| ------------- | ------------------ | ------------------------------------- | ---------------------------------------------------------------- |
| `sukoon-core` | Rust               | —                                     | Decode, separate, remux. Engine registry. Caching.               |
| `sukoon-cli`  | Rust               | crate dependency                      | File + batch processing from a terminal.                         |
| Extension     | TS                 | own DeepFilterNet WASM (AudioWorklet) | Live YouTube filtering, real-time; music removed from all audio. |
| Desktop       | Rust + web (Tauri) | crate dependency                      | GUI; bundles FFmpeg, downloads MDX weights; cleans files.        |
| Mobile        | Kotlin / Swift     | FFI (JNI / C ABI)                     | On-device separation; share-sheet.                               |

## Engines behind one trait

Core exposes a single `Engine` trait. Today:

- **MDX-Net (Kim Vocal 2)** — true voice/instrumental separation via ONNX Runtime. The **working
  engine** and the default for `clean`; ~4× faster than real-time on CPU and ~12–15× with the
  platform GPU accelerator (CoreML/DirectML/CUDA, auto-selected), for files and batch.
- **DeepFilterNet** — the tiny, real-time speech enhancer (keeps speech, suppresses music + noise;
  ~180× real-time on CPU, the only real-time engine). In core it's wired
  behind the `dfn` cargo feature; its pure-Rust `tract` runtime can't load the DFN3 model on the
  desktop toolchain, so core reuses only `deep_filter`'s DSP and runs the three DFN3 ONNX graphs via
  ONNX Runtime. **In the browser extension it is the only engine** — `tract`→WASM works there — run
  real-time in an AudioWorklet with its attenuation capped so it doesn't thin melodic recitation.
- **MDX-Net UVR 9482 (`mdx-lite`)** — the real low-RAM fallback. Same `MdxNet` engine and ONNX
  contract as HQ, only smaller (`dim_f` 2048 vs 3072, ~30 MB); lower quality than HQ, as expected.

The browser **separators were tried and removed** (see
[extension trials](./docs/research/extension-trials.md)); the path to a real-time, best-quality
separator is [Sukoon's own model](./docs/research/own-model-plan.md). Multi-stem modes
(keep-percussion / preserve-effects) would need a multi-stem engine and are not implemented.

Adding an engine means implementing the trait and registering it — nothing else changes. See
[docs/architecture/engines.md](./docs/architecture/engines.md).

## Real-time vs file processing

There are two execution shapes:

1. **File mode** (CLI, desktop files, batch): decode the whole input, separate, remux
   to a new file. Quality-first; latency doesn't matter. Long files use **bounded-memory block
   streaming** (peak RAM ≈ one block, independent of length), and the AAC audio re-encode runs
   **concurrently** with separation — the engine streams cleaned PCM into FFmpeg as it's produced,
   so the encode overlaps rather than adds. Video is never re-encoded (`-c:v copy`). See
   [docs/reference/performance.md](./docs/reference/performance.md).
2. **Live mode** (extension, in-app players): the source keeps playing; audio is routed through
   a processing node (a Web Audio graph in the browser, an audio tap on mobile) with the model
   toggled on/off. The Chrome extension runs **DeepFilterNet in real time**, on-device; true
   separation is a file operation (desktop) until [Sukoon's own model](./docs/research/own-model-plan.md)
   lands.

**Ad handling:** the engine runs uniformly over all audio, so ads play in full with their
background music removed like everything else — never blocked, skipped, or muted. This alters ad
audio (a compliance trade-off); details in
[docs/platforms/extension.md](./docs/platforms/extension.md#ad-handling).

## Caching

Cleaned stems are cached by **content hash** (local-only by default). First run processes; every
replay is instant. Retention is user-configurable. See
[docs/architecture/core.md](./docs/architecture/core.md#caching).

## Where to read next

- [docs/architecture/core.md](./docs/architecture/core.md) — the Rust crate internals.
- [docs/architecture/pipeline.md](./docs/architecture/pipeline.md) — FFmpeg I/O, chunking, A/V sync.
- [docs/architecture/engines.md](./docs/architecture/engines.md) — the engine trait, models, registry.
- [docs/platforms/](./docs/platforms) — per-shell specs.
