# Roadmap

Sukoon ships in phases, each gated on a concrete, testable outcome. The ordering optimizes for
**a shippable v1 fast** — on-device, no servers — then expands toward the Android-first core markets.

Status legend: ✅ done · 🚧 in progress · 📋 planned · 🔬 research

---

## Sukoon's own model (planned, funding-gated)

The **planned** path to genuine quality is training Sukoon's **own** speech-preserving separation
model — but it is **uncertain**: it depends on raising the funding below, so it's a direction, not a
commitment. It would give the best quality on the media people actually watch — **vlogs with
background tracks, documentaries and films/series with musical scores, news with stingers, podcasts
with intro beds, music-heavy explainers and commentary** — and a real _separator_, not just an
enhancer, that can eventually run in real time. The hard requirement is the flip side: **never damage
a voice**, including the toughest case where a recited or sung vocal (nasheed) sits over a backing
track. The browser [engine trials](./docs/research/extension-trials.md) showed off-the-shelf
separators are the ceiling: unreliable live, inconsistent on real content. A purpose-built model is
the way past it — if it can be funded.

> **Funding needed: ~$15,000** (current estimate), and **the work is blocked until it's raised** — for
> GPU compute, datasets, and storage. The [model proposal](./docs/research/own-model-plan.md) is
> designed to start on **free** compute (Kaggle/Colab/Apple silicon), but reaching the HQ tier and the
> elastic supernet needs paid GPU-hours. Every sponsored dollar goes **only** to compute, data, and
> storage. Until then, the shipped engine stays the real-time DeepFilterNet, and this remains a
> proposal.

---

## Phase 0 — Validation

🚧 _In progress._ Prove the approach on real content before building shells.

- [x] Stand up `sukoon-core` with the **MDX-Net HQ engine** via ONNX Runtime (real separation).
- [x] Build `sukoon-cli` to process files and batches.
- [x] Wire the real-time **Fast** engine (DeepFilterNet via ONNX Runtime, behind `--features dfn`)
      and the real low-RAM **`mdx-lite`** fallback (UVR 9482).
- [ ] Assemble a ~30-clip eval corpus: light-music vlog, lecture, nasheed, podcast, abrupt
      music↔silence transitions. See [docs/contributing/model-eval.md](./docs/contributing/model-eval.md).
- [ ] Licensing review: MDX weights (community, download-only), FFmpeg build flags ([LICENSING.md](./LICENSING.md)).

**Gate:** subjective "clean speech, music gone" ≥ 4/5 on ≥ 80% of HQ-mode samples.

## Phase 1 — Extension + Windows desktop app

🚧 _In progress — first public release._

- [x] Chrome MV3 extension: **real-time** YouTube music reduction (DeepFilterNet in an AudioWorklet),
      per-video on/off, on-device with no remote code. The in-browser MDX separators were built and
      **removed** as unreliable on live YouTube — see
      [extension trials](./docs/research/extension-trials.md).
- [x] **Beyond YouTube** — the same real-time engine on more sites, each a small site adapter with a
      natively-injected toggle: **Facebook**, **Instagram**, **X (Twitter)**, plus a **generic HTML5
      `<video>`** catch-all (player overlay) so it works on normal video players anywhere. _TikTok
      still pending._
- [ ] Firefox port; store listings + privacy policy.
- [ ] Tauri Windows app (MDX-Net local; weights download on first use) — true separation for file
      cleaning, off the live stream.

**Gate:** real-time on a mid-range laptop with no audible glitch at the ad→content transition.

## Phase 2 — Web app

📋 _Planned._

- [ ] `@sukoon/web`: upload → clean → download, running **MDX-Net client-side (WASM)** — plus the SEO
      landing page.

**Gate:** a 3-minute file cleaned entirely in the browser on a mid-range laptop.

## Phase 3 — Android

📋 _Planned — core-market expansion._

- [ ] Kotlin shell embedding `sukoon-core` via JNI.
- [ ] On-device real-time engine (DeepFilterNet or successor; TFLite + NNAPI / GPU delegate).
- [ ] Share-sheet ("clean this video"), built-in player with a live toggle.

**Gate:** on-device separation usable on a mid-range Android phone.

## Phase 4 — iOS + macOS/Linux desktop

📋 _Planned._

- [ ] Swift shell with an on-device real-time engine via Core ML; share extension.
- [ ] Tauri builds for macOS (notarized) and Linux (AppImage + Flatpak).

## Phase 5 — Multi-stem modes, batch & polish

🔬 _Research / ongoing._

- [ ] Multi-stem engine to make finer modes real: _keep percussion (duff)_ / _preserve sound
      effects_. (Today's 2-stem MDX engine only does remove-music / keep-voice.)
- [ ] Sensitivity slider; nasheed mode.
- [ ] Batch/creator workflow for cleaning archives.
- [ ] Optional speech-enhancement post-pass for residual artifacts.
- [ ] 🔬 **Sukoon's own model** — strip background music from everyday media (vlogs, documentaries,
      podcasts, film/series scores) while keeping every voice intact, narration to nasheed vocals. The
      highest-leverage quality gain available — **planned but funding-gated**. See
      [Sukoon's own model](#sukoons-own-model-planned-funding-gated) (≈ $15,000) and the
      [model proposal](./docs/research/own-model-plan.md).

---

## Engine roadmap

| Engine                | Role                          | Status                             |
| --------------------- | ----------------------------- | ---------------------------------- |
| MDX-Net (Kim Vocal 2) | HQ voice/instrumental (files) | ✅                                 |
| DeepFilterNet 3 (ORT) | Fast / real-time enhancement  | ✅                                 |
| MDX-Net UVR 9482      | Low-RAM fallback (`mdx-lite`) | ✅                                 |
| **Sukoon model**      | Speech-preserving separator   | 🔬 planned, funding-gated (≈ $15k) |

> **In the extension, the only engine is DeepFilterNet** (real-time enhancement) — the MDX separators
> run in the native/file tools (CLI, desktop), not on the live stream
> ([why](./docs/research/extension-trials.md)). DeepFilterNet runs via ONNX Runtime (its bundled
> `tract` runtime can't load the model on current toolchains), reusing `deep_filter`'s DSP; it's
> CPU-preferred (~180× real-time, no GPU needed). Speeds and device support:
> [docs/reference/performance.md](./docs/reference/performance.md).

## Non-goals

- Ad blocking / skipping / muting — ever.
- A built-in video editor.
- Uploading or storing user media on a server — Sukoon is **on-device**.
- Declaring religious rulings (see [GOVERNANCE.md](./GOVERNANCE.md)).

Dates are intentionally omitted — this is an open-source effort. Phase ordering and gates are the
commitment; timing depends on contributors. Want to own a phase? Open an issue.
