# @sukoon/extension

The Sukoon browser extension (Chrome; Firefox planned, Manifest V3). Removes instrumental background
music from YouTube **in real time, on-device**, while keeping speech and recitation. UI is **React**,
sharing the [`@sukoon/ui`](../../packages/ui) design system with the desktop app.

All audio that plays, **including ads**, is processed the same way. Sukoon never blocks, skips,
mutes, or disables ads.

## How it works

```
<video> ──► MediaElementSource ──► [dfn-processor AudioWorklet] ──► loudness tail ──► speakers
              (real-time, ~10 ms, on-device)
```

One engine: **DeepFilterNet** (a speech-preserving enhancer) compiled to WASM (`@sukoon/dfn-wasm`)
and run frame-by-frame in the page's audio thread. Its attenuation is capped (gentle) so it doesn't
thin melodic recitation.

- [`src/audio-graph.ts`](./src/audio-graph.ts) — Web Audio routing: `source → dfn-processor →
loudness (fixed gain + limiter) → destination`. Autoplay-aware, follows the active `<video>`
  (watch + Shorts), and emits a status stream (`off → loading → active`) the UI reflects.
- [`worklet/processor.js`](./worklet/processor.js) — the `dfn-processor` AudioWorklet; the content
  script compiles the DFN WASM and hands the module in (worklets can't fetch).
- [`src/loudness.ts`](./src/loudness.ts) — fixed makeup gain + a true peak limiter (no auto-leveling,
  which pumped).
- [`src/prefs.ts`](./src/prefs.ts) — **per-video** on/off (`chrome.storage.local`, keyed by video id)
  with a synced default; each tab is independent.
- [`src/yt-button.tsx`](./src/yt-button.tsx) + [`src/yt-mount.ts`](./src/yt-mount.ts) — a React
  on/off control injected **into YouTube's own UI** (a pill on watch pages, a stacked action on
  Shorts), inside a Shadow DOM so styles don't leak either way.
- [`src/popup.tsx`](./src/popup.tsx) — the React popup (logo + per-video on/off toggle).
- [`src/content.ts`](./src/content.ts) — wires it together, tracking the active video across SPA
  navigation and Shorts scrolling.
- [`src/background.ts`](./src/background.ts) — first-run defaults; reloads open YouTube tabs after an
  install/update so content scripts re-bind.

Because a React bundle needs a shared vendor chunk a _classic_ content script can't `import`, the
manifest registers [`public/content-loader.js`](./public/content-loader.js), a tiny shim that
dynamic-imports the real ES-module `content.js`. The worklet + DFN WASM are staged into `dist/` by
`scripts/bundle-worklet.mjs`. See [docs/platforms/extension.md](../../docs/platforms/extension.md).

> **We tried in-browser separators (MDX-Net) and removed them** — too fragile on live YouTube. The
> full write-up is in [docs/research/extension-trials.md](../../docs/research/extension-trials.md);
> the path to real separation quality is
> [Sukoon's own model](../../docs/research/own-model-plan.md).

## Why we don't download the video

Sukoon processes the audio **as it plays, in real time**. It does not read ahead, download, copy, or
re-host the video — a deliberate design and ethical choice. See
[docs/design-considerations.md](../../docs/design-considerations.md#2-why-we-never-download-the-video).

## Develop

```bash
pnpm install
pnpm --filter @sukoon/extension build    # bundle + stage worklet/wasm → dist/
pnpm --filter @sukoon/extension dev      # watch mode for TS/React
```

Load `dist/` (with `manifest.json`) as an unpacked extension in `chrome://extensions`. Nothing is
fetched at runtime.

## Store compliance

Processing ad audio the same as everything else alters it, which can conflict with store/platform
policies on modifying ads; see
[docs/platforms/extension.md](../../docs/platforms/extension.md#ad-handling). Permissions are minimal:
`storage` + `unlimitedStorage`, YouTube host only.
