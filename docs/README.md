# Sukoon Docs

Sukoon removes background music from media while keeping the human voice. File tools (CLI, desktop,
web) run true separation through one shared Rust engine (`sukoon-core`); the browser extension runs
DeepFilterNet in real time, on-device. These docs are organized by lane — pick the one that matches
what you're doing.

## Start here

1. **[Installation](./start/installation.md)** — get the toolchain, FFmpeg, and models in place.
2. **[Quickstart](./start/quickstart.md)** — clean your first file in a minute.
3. **[Concepts](./start/concepts.md)** — the engines, modes, live vs file, caching.

## Choose a lane

| Lane                                        | Read when you're working on…                                                                                 |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| **[Architecture](./architecture/index.md)** | The engine, the pipeline, adding a model, how it all fits.                                                   |
| **[Platforms](./platforms/index.md)**       | The extension, desktop, mobile, or web shell.                                                                |
| **[Halal-aware](./halal-aware/index.md)**   | The separation modes and the scholarly positions behind them.                                                |
| **[Reference](./reference/cli.md)**         | CLI flags, the core API, configuration/env vars, [performance & device support](./reference/performance.md). |
| **[Contributing](./contributing/index.md)** | Setup, the eval workflow, the rules that matter.                                                             |

## Two documents everyone should read

- **[Design considerations](./design-considerations.md)** — the _why_, including the ethical and
  legal constraints (ads, downloading, privacy, rulings). These are binding.
- **[LICENSING.md](../LICENSING.md)** — model-weight and FFmpeg licensing. Read before shipping a
  binary.

## The one rule

> File-separation logic lives **only** in `sukoon-core` — no shell reimplements it. (The extension is
> the exception: its real-time engine runs in the browser, not through `sukoon-core`.)
