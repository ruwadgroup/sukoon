# @sukoon/desktop

The Sukoon desktop app — a [Tauri](https://tauri.app) shell (Rust core + web UI, ~10 MB vs
Electron's ~100 MB) over [`sukoon-core`](../../packages/core). Windows first, then macOS and Linux.

**Drag-and-drop cleaning.** Drop a video/audio file, pick Fast or HQ, get a clean output. Fast
(~180× real-time, CPU-only) runs everywhere; HQ runs locally if the hardware allows (a GPU lifts it
from ~4× to ~12–15× real-time via auto-selected CoreML/DirectML/CUDA). See
[docs/reference/performance.md](../../docs/reference/performance.md).

All separation happens in `sukoon-core` via the `clean_file` Tauri command — the shell never
reimplements it.

## Status

Planned (scaffold present). The `src-tauri` crate is intentionally **outside** the root Cargo
workspace because Tauri carries its own build config.

## Develop

```bash
pnpm install
pnpm --filter @sukoon/desktop dev     # tauri dev
pnpm --filter @sukoon/desktop build   # produces msi / dmg / AppImage
```

Requires the [Tauri prerequisites](https://tauri.app/start/prerequisites/) and an FFmpeg binary
(LGPL build — see [LICENSING.md](../../LICENSING.md)).
