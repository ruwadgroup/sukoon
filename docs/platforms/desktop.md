# Desktop

Tauri shell (Rust core + web UI) for Windows, macOS, and Linux. Source:
[`apps/desktop`](../../apps/desktop).

## What it does

**Drag-and-drop cleaning** — drop a file, pick Fast/HQ, get a clean output. The `clean_file` Tauri
command runs separation in `sukoon-core`. This is where true **MDX-Net** separation lives — a file
operation (the live extension runs the real-time engine only).

## Why Tauri

~10 MB shell vs Electron's ~100 MB, native webview, Rust backend so it links `sukoon-core` directly.
The `src-tauri` crate is intentionally **outside** the root Cargo workspace (Tauri carries its own
build config).

## Hardware floor

Windows 10/11 64-bit, 4 GB RAM. Fast mode runs on virtually anything; HQ mode wants 8 GB RAM or a
GPU.

## Packaging

- Bundles FFmpeg (LGPL); downloads the MDX-Net weights (~67 MB) on first use.
- Auto-selects a GPU accelerator for HQ when present, else CPU: CoreML (macOS), DirectML (any
  Windows DX12 GPU), CUDA (Linux NVIDIA, `--features cuda`). A GPU lifts HQ from ~4× to ~12–15×
  real-time (Fast runs ~180× on CPU and needs no GPU). See
  [reference/performance.md](../reference/performance.md).
- Targets: `.msi` (Windows), `.dmg` (macOS, notarized), AppImage/Flatpak (Linux).

```bash
pnpm --filter @sukoon/desktop dev
pnpm --filter @sukoon/desktop build
```
