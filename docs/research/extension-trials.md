# Extension engine trials — what we tried in the browser

A record of every approach we built to remove music **inside the browser, on a live YouTube
`<video>`**, why each was attempted, and why all but one were removed. The shipped extension now runs
**only the real-time DeepFilterNet engine** (Trial A). This page exists so we don't relitigate dead
ends and so the case for [Sukoon's own model](./own-model-plan.md) is grounded in evidence.

> The file tools (CLI, desktop) still use the MDX-Net separators — they process **files**,
> where latency and live-stream fragility don't apply. Everything below is specific to the
> **extension's live path**.

---

## The core constraint

A real separator (MDX-Net / Demucs / RoFormer) is **not real-time** in the browser and needs a
look-ahead window to produce a clean result. A live `<video>` gives you audio exactly as it plays.
So every "quality" approach had to somehow get audio **before** the playhead reached it — without
downloading the video — and then play the cleaned result back **in lip-sync**. That is the entire
source of difficulty. The real-time enhancer (DFN) sidesteps it by processing frame-by-frame with
~10 ms latency; everything else fought the look-ahead/sync problem and lost.

---

## Trial A — DeepFilterNet, real-time in-page (✅ shipped)

A speech **enhancer** (keeps voice, suppresses music/noise) small enough to run frame-by-frame in
the page's audio thread.

```
<video> ─► MediaElementSource ─► [dfn-processor AudioWorklet] ─► loudness tail ─► destination
              (real-time, ~10 ms, in sync, no download, works on every stream incl. ads)
```

- **Runtime:** DeepFilterNet 3 compiled to **WASM via `tract`** (`@sukoon/dfn-wasm`) with the model
  embedded, 48 kHz / 480-sample-hop streaming. The content script compiles the WASM and hands the
  module to the worklet (worklets can't fetch). (Note: the _native_ engine can't use `tract` — it
  hits an optimizer bug on the desktop toolchain and runs DFN via ONNX Runtime instead — but
  `tract`-in-WASM works in the browser.) Attenuation **capped** so the enhancer doesn't thin melody.
- **History:** this was the original single-mode version that "worked fairly." A later GPT pass
  replaced it with a chunked relay (Trial B) and broke the extension; we **restored** DFN as the
  floor in `1424034`, and `bec7e47` made it the _only_ engine.
- **Outcome:** works reliably. It's a _speech enhancer_, not a true separator, so on dense music it's
  gentler than a real separator — but it never desyncs, never downloads, never stalls. **This is the
  shipped engine.**

---

## Trial B — Offscreen MDX via a relay worklet (❌ removed)

First attempt at "real quality": capture live audio in a worklet, ship batches to an offscreen
document running MDX-Net on ONNX Runtime, play the separated stem back with a priming buffer.

```
<video> ─► MediaElementSource ─► [relay AudioWorklet] ─► SW ─► offscreen (ORT MDX) ─► back ─► play
```

- **Why it failed:** MDX needs a multi-second chunk, so the relay had to buffer and the output lagged
  the picture. A "max sync latency" guard then **fail-opened** to the original audio whenever it
  couldn't keep up — which was always — so the extension effectively did nothing and surfaced a
  "companion needed" state. Audio/video were never in sync.

---

## Trial C — Synced separator via the MSE tap (❌ removed)

The most-developed approach. Read the audio the browser **already buffered ahead** of the playhead
(no separate download), separate it on large chunks, and schedule the cleaned PCM back in lip-sync,
holding the video with a spinner until the first clean chunk is ready.

```
MAIN world (document_start):
  hook MediaSource.addSourceBuffer / SourceBuffer.appendBuffer
  └─► copy each appended audio segment (Opus in WebM), ~15–50 s buffered ahead
            │  window.postMessage
            ▼
content (isolated) ─► chrome.runtime port ─► service worker ─► offscreen document
                                                                   │
        offscreen: WebCodecs AudioDecoder (Opus) ─► mono ─► resample 48→44.1k
                   ─► MDX-Net separate on ~8 s look-ahead chunks (ORT WebGPU/WASM)
                   ─► cleaned PCM {startSec, pcm}
            │  back through SW → content
            ▼
SyncedPlayer: mute original (<video> via gain 0); schedule AudioBufferSources at video.currentTime;
              HOLD with a spinner until first clean chunk; re-buffer if the lead runs low; resync on
              seek / rate change / pause / hard-refresh-at-timestamp.
```

What we built for this, each its own sub-problem:

- **MSE tap** (`mse-hook.js`, MAIN world): hooked `appendBuffer`, kept a rolling buffer, and replayed
  the buffered-ahead segments to a late-starting consumer.
- **WebCodecs decode** in the offscreen, `preferredOutputLocation: "cpu"` to dodge a WebGPU readback race.
- **SyncedPlayer**: anchor `ctx.currentTime ↔ video.currentTime`, hold-with-spinner, drift detection,
  in-window seek handling, gesture-driven autoplay recovery.
- **Edge cases** handled: seek, `?t=` / hard-refresh resume at a timestamp, pause/play, rate change,
  stall recovery.

**Why it failed — a pile-up of fragility:**

- **Startup race:** the offscreen processed port messages without awaiting, so the replayed backlog
  arrived before the separator was constructed and was silently dropped → no clean audio → fallback.
  (Fixed late with a queue, but it proved how brittle the pipeline was.)
- **Service-worker lifecycle:** the relay port churned/disconnected, breaking the stream.
- **Ad churn:** swapping engines on every ad break tore down and rebuilt the chain, dropping the port
  and racing the playhead (`hold at 398 → 0 → 398`).
- **Network reality:** on the test connection, `googlevideo` 403'd the adaptive Opus stream, so there
  was often no separate audio buffer to tap at all (see Trial D).
- **Sync drift:** keeping scheduled buffers locked to a moving, seekable, variable-rate playhead is a
  constant fight.

**Other attempts made along the way (inside this path):**

- **Decode in the content script first**, then moved to the offscreen document — a content-side
  WebCodecs "decode probe" came before the full offscreen pipeline.
- **Considered reverse-engineering YouTube / fetching the media segments ourselves**, then rejected
  it in favour of the MSE tap (read what the browser already buffered — no separate download).
- **Cross-world `postMessage` with a transfer list** silently dropped the segment bytes between the
  MAIN and isolated worlds; switched to copying.
- **WebGPU read-back race** (`Buffer was unmapped…`) → forced `preferredOutputLocation: "cpu"`.
- **Autoplay-blocked `AudioContext`** → gesture listeners to resume + an autoplay-safe `video.play()`.
- **Larger look-ahead chunks** (per feedback) to give the separator more context per inference.
- **Hold-with-spinner UX**, iterated three times: a "Buffering…" label → YouTube's native spinner →
  finally an opaque, delayed, fading overlay so it didn't fight YouTube's own spinner.

Even with each individual bug fixed, the approach stayed a house of cards: too many independent
failure modes (SW, decode, network, ads, drift) for something that must "just work."

---

## Trial D — Muxed / AAC (itag 18) demux (❌ removed)

A patch for Trial C: when the adaptive Opus stream is blocked, YouTube falls back to a **muxed
`video/mp4`** stream (AAC audio inside the video buffer). Tap that buffer instead and demux the audio.

```
MSE tap (video/mp4) ─► mp4box: extract AAC track + AudioSpecificConfig
                     ─► WebCodecs AudioDecoder (mp4a.40.2) ─► same separate path as Trial C
```

- Added `mp4box` and a fMP4/AAC reader; the separator handled both WebM/Opus and MP4/AAC at variable
  decode rates.
- **Why it went:** it made Trial C work on a degraded network, but it inherited all of Trial C's
  fragility and added a heavy dependency. When Trial C was removed, this went with it.

---

## Trial E — Loudness stabilization (lesson kept)

Independent of which engine produced the audio, the kept voice had **unstable volume**.

```
Tried:  cleaned ─► RMS auto-leveler (43 ms window, chase a target RMS) ─► limiter ─► out
Result: gain swung ±4–9 dB (measured) — audible "pumping", because it fights speech dynamics.

Shipped: cleaned ─► fixed makeup gain (+1.6 dB) ─► true peak limiter ─► out
Result: stable, natural dynamics; the limiter only catches near-clip peaks.
```

- **Lesson (kept in the shipped DFN tail):** don't auto-level — a fixed gain plus a limiter is the
  right call for speech/recitation.

---

## Trial F — Offline model validation (how we decided)

To separate _model_ quality from _pipeline_ quality, we ran the real models **offline** (Node +
`onnxruntime-node`) on real audio pulled with `yt-dlp`, and measured.

```
yt-dlp clip ─► ffmpeg → mono 44.1k f32 ─► demixMono (real MDX-lite / Kim Vocal 2)
            ─► measure: RMS envelope, per-chunk level, clipping, mix−vocal "removed" stem
```

Findings:

- The **STFT + overlap-add + 8 s chunking is level-faithful** — per-segment level matched
  whole-signal processing within **0.1 dB**. The chunking was never the problem.
- **Separation strength is content-dependent.** On a vocals-over-instrumental nasheed the model pulled
  out a clear stem; on a speech-dominant clip it removed only ~0.27 dB (the removable music was
  ~16 dB below the mix). Quality on real, varied content was inconsistent.
- **Raw output can exceed 0 dBFS** (peaks 1.2–1.6), so a limiter is mandatory (Trial E).

**Conclusion:** the ceiling isn't the plumbing — it's the **model**. A generic vocal separator is
neither reliably real-time in the browser nor reliably good on the media people actually watch — vlogs,
documentaries, news, podcasts, films/series — which carry anything from a light background bed to a
full musical score, all while the voice (narration, dialogue, or a nasheed vocal) must survive
untouched. That is exactly what a **purpose-built model** would fix →
[own-model-plan.md](./own-model-plan.md).

---

## Where it landed

| Trial | Approach                       | Verdict                                            |
| ----- | ------------------------------ | -------------------------------------------------- |
| A     | DFN real-time in-page          | ✅ **Shipped** — the only engine in the extension  |
| B     | Offscreen MDX relay (chunked)  | ❌ Removed — always out of sync                    |
| C     | Synced separator via MSE tap   | ❌ Removed — too many live-pipeline failure modes  |
| D     | Muxed/AAC demux (mp4box)       | ❌ Removed — patched C; same fragility + heavy dep |
| E     | Loudness: fixed gain + limiter | ✅ Kept (in the DFN tail); auto-leveler rejected   |
| F     | Offline model validation       | 📋 Decided the direction: build our own model      |

The extension is intentionally small now: one real-time engine, on/off per video, fully on-device.
The path to genuine _separation_ quality in the browser is **Sukoon's own model**, not more browser
plumbing.
