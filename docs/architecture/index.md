# Architecture

The high-level map lives in [`ARCHITECTURE.md`](../../ARCHITECTURE.md) at the repo root. This
section is the detail.

- **[Core](./core.md)** — the `sukoon-core` Rust crate: types, the pipeline, caching.
- **[Pipeline](./pipeline.md)** — FFmpeg I/O, chunking, A/V sync, the FFmpeg build question.
- **[Engines](./engines.md)** — the `Engine` trait, the three models, and the model registry.

## The shape in one line

```
decode ─► extract audio (FFmpeg) ─► Engine.separate() ─► keep speech, drop music ─► remux (FFmpeg)
```

Everything that decides _what to keep_ is in `sukoon-core`. Shells are adapters. See
[the one rule](../README.md#the-one-rule).
