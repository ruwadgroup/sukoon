# sukoon-cli

The `sukoon` command-line tool. Removes background music from a file or a whole folder, keeping
speech. A thin wrapper over [`sukoon-core`](../core).

## Install

```bash
# From the repo, with real inference enabled
cargo install --path packages/cli --features onnx

# Once published
cargo install sukoon-cli --features onnx
```

Requires an FFmpeg binary on `PATH` (LGPL build recommended — see [LICENSING.md](../../LICENSING.md)).
Build with `--features onnx` for real MDX separation (HQ + the `mdx-lite` fallback), or
`--features dfn` to also enable the real-time **Fast (DeepFilterNet)** engine; without these the CLI
runs in dry passthrough mode.

## Usage

On the **first run**, the required model weights download automatically with a live progress line;
after that everything is offline. See [docs/reference/performance.md](../../docs/reference/performance.md)
for speeds and the device matrix.

```bash
# Clean one file (HQ / MDX-Net, on-device) → vlog.clean.mp4. Weights download on first run.
sukoon clean vlog.mp4

# Choose the output path
sukoon clean lecture.mp4 -o lecture.clean.mp4

# Batch a folder
sukoon batch ./vlogs --out ./vlogs-clean

# List engines + the models and licenses they use
sukoon engines
```

### Engines

| `--engine`     | Model                         | Status                                     | Best for                          |
| -------------- | ----------------------------- | ------------------------------------------ | --------------------------------- |
| `hq` (default) | MDX-Net Kim Vocal 2           | ✅ ~4× CPU / ~12–15× GPU                   | Dense music, nasheeds, files      |
| `fast`         | DeepFilterNet 3               | ✅ ~180× real-time (CPU); `--features dfn` | Live/real-time speech enhancement |
| `fallback`     | MDX-Net UVR 9482 (`mdx-lite`) | ✅ low-RAM (~30 MB); lower quality         | Old / memory-constrained hardware |

MDX inference auto-uses your GPU where available (CoreML on macOS, DirectML on Windows, CUDA on Linux
with `--features cuda`) and falls back to CPU otherwise. The **Fast (DeepFilterNet)** engine is
**CPU-preferred** (faster than the GPU for this small recurrent model). Nothing to configure.

### Modes

`remove-all` (default) and `keep-vocals` both keep the voice and drop the instrumental (the same
thing for the current 2-stem engine). `keep-percussion` / `preserve-effects` are placeholders, not
yet implemented. See [docs/halal-aware](../../docs/halal-aware/index.md).

## Logging

`-v` for debug, `-vv` for trace. Or set `RUST_LOG=sukoon=debug`.
