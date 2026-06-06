# Browser Extension

Chrome (Firefox planned), Manifest V3. On-device, **real-time** music reduction on YouTube, Facebook,
Instagram, X, and any HTML5 `<video>`. One engine, nothing fetched at runtime. Source:
[`apps/extension`](../../apps/extension).

## Supported sites

Each platform is a small **site adapter** ([`src/adapters`](../../apps/extension/src/adapters)) that
knows how to find the playing video, derive a stable per-video key, and inject the toggle — paired
with a **per-platform button** ([`src/adapters/mounts`](../../apps/extension/src/adapters/mounts))
designed and inserted to feel native:

| Site                  | Toggle placement                                            |
| --------------------- | ----------------------------------------------------------- |
| **YouTube**           | Watch action row (next to like/dislike) and the Shorts rail |
| **Facebook**          | The post's like/comment/share row                           |
| **Instagram**         | The post action row (like/comment/share/save)               |
| **X (Twitter)**       | The tweet action bar (`[role="group"]`)                     |
| **Generic `<video>`** | A small player overlay (no site toolbar to match)           |

The adapter is chosen by hostname, falling back to the generic catch-all. Per-video state is keyed by
a **namespaced** id (`yt:` / `fb:` / `ig:` / `x:` / `web:<host><path>`) so platforms never collide.
Where a platform's action bar can't be found, the button degrades to the player overlay.

The social DOMs (Facebook/Instagram/X) use obfuscated, frequently-changing class names, so their
injection anchors are best-effort and isolated in each `mounts/<site>.tsx` for easy tuning.

## Pipeline

```
<video> ─► MediaElementSource ─► [dfn-processor AudioWorklet] ─► loudness tail ─► destination
              (real-time, ~10 ms, in sync — works on every stream, including ads)
```

The only engine is **DeepFilterNet**, run frame-by-frame in the page's audio thread (the
`dfn-processor` AudioWorklet over the `@sukoon/dfn-wasm` build): ~10 ms latency, ~13 MB WASM embedded,
CPU-only. It is a speech _enhancer_ (keeps voice, suppresses music +
noise), so its attenuation is **capped (gentle by default)** to avoid thinning melodic recitation.
Because it processes live audio in place, it never reads ahead, never holds the video, and never
desyncs.

When removal is off (or on any failure) the source routes straight to the destination, so YouTube
audio is never lost.

- A **"Reduce music" toggle** is injected natively per platform (see **Supported sites**). State is
  **per video** (`chrome.storage.local`, keyed by the namespaced media key) with a synced default;
  each tab is independent.
- On the generic catch-all, only **origin-safe** media is claimed (`blob:`/`data:`/`mediastream:` or
  same-origin). A cross-origin `<video>` is left untouched, because tapping it through Web Audio would
  taint the graph and silence it.
- There is **no engine picker** — one real-time engine. True multi-stem separation lives in the
  native/file tools, not on the live stream.

> **The in-browser separators were tried and removed.** We built MDX-Net Lite/High running in an
> offscreen document, fed by an MSE tap that read the audio the browser buffered _ahead_ of the
> playhead (plus a muxed-AAC demux for blocked streams), with a synced player that held the video
> until the first clean chunk. It was too fragile on live YouTube (service-worker lifecycle, decode
> races, ad churn, network 403s, sync drift) and the separation quality was inconsistent on real
> content. Full write-up: [extension trials](../research/extension-trials.md). The path to real
> separation quality is [Sukoon's own model](../research/own-model-plan.md).

## Ad handling

**Sukoon removes background music from all audio, including ads.** The real-time engine runs
uniformly over whatever is playing; ads are not special-cased. Sukoon **never blocks, skips, mutes,
or disables ads** — they play in full, just with their background music reduced like everything else.

Trade-off to be aware of: reducing music in an ad **alters the ad's audio**, which can run into
copyright, YouTube's Terms of Service, and browser-store policies on modifying ads. Shipping this
as-is means accepting that risk. (This is _not_ a "protect ad revenue" argument; it's about
store/platform compliance.)

## We don't download the video

The extension processes audio **in the page, as it plays** — in real time, frame by frame. It does
not read ahead, download, save, or re-host anything. See
[design-considerations §2](../design-considerations.md#2-why-we-never-download-the-video).

## Store compliance

- Removes music from all audio including ads, but never blocks, skips, mutes, or disables ads. Be
  aware this alters ad audio (see **Ad handling**) and may conflict with store/platform policies.
- **Permissions:** `storage` and `unlimitedStorage`; host access is `<all_urls>` so the generic
  catch-all can run on any video player (the named platforms get a native toggle). Audio is captured
  in-page via `MediaElementSource` (no `tabCapture` prompt). The DFN WASM is bundled — **no
  remotely-hosted code**. No `webRequest` ad rules.
- All processing is local; nothing is uploaded.

## Build

```bash
pnpm --filter @sukoon/extension build   # → dist/
```

Load `dist/` as an unpacked extension.
