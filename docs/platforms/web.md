# Web

Upload → clean → download, plus the SEO landing page that drives installs of the heavier apps.
Source: [`apps/web`](../../apps/web).

> Status: planned (scaffold present).

## Flow

```
user picks file ─► extract audio (ffmpeg.wasm) ─► separate in-browser (MDX-Net via WASM)
                ─► remux in-browser ─► download
```

- Everything runs **in the browser, on-device** — the file never leaves the device. (MDX-Net is a
  _file_ operation here, so the latency that ruled it out for the live extension doesn't apply.)
- Audio is processed; the video is remuxed locally (`-c:v copy`).

## Why it exists

Lowest-friction trial surface and the SEO entry point. Put the use-cases (vlogs, lectures, podcasts,
documentaries) in the page description/keywords; the landing page converts to extension/desktop/mobile
installs.

## Run

```bash
pnpm --filter @sukoon/web dev
```
