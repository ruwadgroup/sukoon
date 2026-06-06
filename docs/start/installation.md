# Installation

The smallest path to a working `sukoon` build.

## System requirements

| Tool   | Version          | Notes                                                                 |
| ------ | ---------------- | --------------------------------------------------------------------- |
| Rust   | stable ≥ 1.80    | Core, CLI, desktop. `rustup` recommended.                             |
| FFmpeg | ≥ 6 (LGPL build) | Runtime dependency for audio I/O. Audio-only path needs no GPL parts. |
| Node   | ≥ 22             | For the JS shells (extension, desktop UI, web).                       |
| pnpm   | ≥ 10             | JS workspace manager.                                                 |

## Clone and build the core + CLI

```bash
git clone https://github.com/ruwadgroup/sukoon.git
cd sukoon
cargo build --release -p sukoon-cli
```

This builds in **dry mode** so it compiles without model weights — useful for development and CI.
For real separation, enable the `onnx` feature. This turns on the HQ MDX-Net engine and the low-RAM
`mdx-lite` fallback (via ONNX Runtime):

```bash
cargo build --release -p sukoon-cli --features onnx
```

To also build the real-time **Fast (DeepFilterNet)** engine, add the `dfn` feature (it implies
`onnx`); without it, `--engine fast` is a passthrough stub:

```bash
cargo build --release -p sukoon-cli --features dfn
```

## FFmpeg

Sukoon calls the `ffmpeg`/`ffprobe` binaries.

- **macOS:** `brew install ffmpeg`
- **Debian/Ubuntu:** `sudo apt-get install ffmpeg`
- **Windows:** download an LGPL build and add it to `PATH`, or set `SUKOON_FFMPEG` /
  `SUKOON_FFPROBE` to the binary paths.

Ship an **LGPL** build with any distributable — see [LICENSING.md](../../LICENSING.md).

## Models

Model weights download on first use to `SUKOON_MODELS_DIR` (defaults to a temp dir), with a live
progress indicator (downloaded/total MB, %); nothing is bundled. Each model's URL, checksum, and
license live in the [registry](../../packages/core/src/registry.rs). List them:

```bash
sukoon engines
```

## JS workspaces (optional)

```bash
pnpm install                 # installs extension/desktop/web deps + the lint/format toolchain
pnpm --filter @sukoon/extension build
```

## Verify

```bash
./target/release/sukoon --version
./target/release/sukoon engines
```

Next: **[Quickstart](./quickstart.md)**.
