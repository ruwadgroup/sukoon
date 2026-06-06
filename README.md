<div align="center">

# Sukoon · سكون

**A privacy-first, open-source media filter that removes background music while keeping speech clear.**
**On-device by default. Creator-friendly. Halal-aware.**

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)
[![CI](https://github.com/tamimbinhakim/sukoon/actions/workflows/ci.yml/badge.svg)](https://github.com/tamimbinhakim/sukoon/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/core-rust-orange.svg?logo=rust&logoColor=white)](./packages/core)
[![pnpm](https://img.shields.io/badge/pnpm-monorepo-f69220.svg?logo=pnpm&logoColor=white)](https://pnpm.io/)
[![Conventional Commits](https://img.shields.io/badge/conventional_commits-1.0.0-fa6673.svg)](https://www.conventionalcommits.org)
[![Status: 0.1.0-alpha](https://img.shields.io/badge/status-0.1.0--alpha-red.svg)](#status)
[![Sponsor](https://img.shields.io/badge/sponsor-%E2%9D%A4-ec4899.svg?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/tamimbinhakim)

[**Quickstart**](./docs/start/quickstart.md) · [**Why Sukoon**](./docs/start/concepts.md) · [**Architecture**](./ARCHITECTURE.md) · [**Docs**](./docs) · [**Roadmap**](./ROADMAP.md)

</div>

## What Sukoon is

Sukoon removes instrumental **background music** from videos and audio while preserving **speech** — dialogue, narration, lectures, recitation, and vocals. It exists for Muslims who want to consume mainstream media (vlogs, news, documentaries, podcasts, lectures) without the background music many consider impermissible — and for anyone who simply finds background tracks distracting.

The whole thing is built around one shared engine — **Sukoon Core**, a Rust audio library wrapping FFmpeg and pluggable separation models — and a set of thin platform shells around it: a browser extension, a desktop app, mobile apps, and a web uploader. The file tools share that one engine. (The browser extension is the exception — a real-time separator in Rust→WASM isn't there yet, so it runs DeepFilterNet via its own WASM build.)

```
input.mp4 ──► FFmpeg (extract stereo at the engine's rate: 44.1/48 kHz)
           ──► Separation engine ──┬─► keep: speech stem
                                   └─► discard: music stem
           ──► FFmpeg (remux clean audio, -c:v copy) ──► output.mp4
```

## Core principles

These are not marketing lines — they are baked into the engineering decisions and enforced in review. The full rationale, including the ethical and religious constraints (why ads must keep playing, why we never download the video), is in [docs/design-considerations.md](./docs/design-considerations.md).

1. **Privacy-first.** Audio is processed **entirely on-device** — nothing is uploaded, there is no cloud service.
2. **We don't block ads.** Ads are **never** blocked, skipped, muted, or disabled — they always play in full. Music removal is applied uniformly to all audio, so an ad's **background music is removed like everything else**. Note this _alters ad audio_, which can run into **copyright**, platform **Terms of Service**, and browser-store policy on modifying ads — a compliance trade-off taken on deliberately. (This is _not_ a "protect the creator's revenue" argument.)
3. **Real-time in the browser, true separation for files.** The browser extension removes music **live, on-device** with a real-time, speech-preserving engine (DeepFilterNet). For files, the native tools (CLI / desktop) run a true vocal/instrumental separator (MDX-Net). A purpose-built, best-quality model is the next milestone — see the [roadmap](./ROADMAP.md).
4. **Halal-aware, not prescriptive.** The core behaviour — remove the instrumental music, always keep the voice/recitation — maps to the broadest common position. Sukoon presents this with sourced explanations and does not assert a single ruling.

## Engines

One interface, the right engine per context:

| Context                      | Engine                                               | Notes                                                                                                                                                         |
| ---------------------------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Browser extension** (live) | **DeepFilterNet** — real-time, on-device             | A speech-preserving enhancer in an AudioWorklet. YouTube, Facebook, Instagram, X, and any HTML5 `<video>`. Works on every stream, including ads.              |
| **Files** (CLI · desktop)    | **MDX-Net (Kim Vocal 2)**; UVR 9482 low-RAM fallback | True 2-stem voice/instrumental separation via ONNX Runtime; weights download + SHA-256-verify on first use, never bundled.                                    |
| **Next**                     | **Sukoon's own model** 🔬                            | A purpose-built, speech-preserving separator — the path to the best quality (and eventually a real-time _separator_ in the browser). [Roadmap](./ROADMAP.md). |

We tried running the heavy separators **inside the browser on the live stream** (an MSE tap feeding an
offscreen ONNX Runtime, played back in lip-sync). It was too fragile to ship and was removed — the
full write-up is in [extension trials](./docs/research/extension-trials.md). True separation now lives
in the file tools, and the long-term answer is [Sukoon's own model](./docs/research/own-model-plan.md).

Speeds, the device matrix, and acceleration details: **[docs/reference/performance.md](./docs/reference/performance.md)**.

## What Sukoon is not

- **Not an ad blocker.** It never blocks, skips, or mutes ads — they play in full, with their background music removed like everything else. See [docs/platforms/extension.md](./docs/platforms/extension.md#ad-handling).
- **Not a video editor.** It only swaps the audio track; video frames are copied losslessly.
- **Not a model trainer.** Sukoon integrates existing open models. Fine-tuning is a future, opt-in research track ([ROADMAP](./ROADMAP.md)).

## Packages

| Package                                 | What it is                                                                                                                                  | Status  |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| [`sukoon-core`](./packages/core)        | Rust audio engine — FFmpeg I/O + pluggable separation backends (ONNX Runtime). The single source of truth.                                  | v0.1 α  |
| [`sukoon-cli`](./packages/cli)          | `sukoon` command-line tool — clean a file or batch a folder. Thin wrapper over core.                                                        | v0.1 α  |
| [`@sukoon/extension`](./apps/extension) | Manifest V3 browser extension — real-time, on-device music removal (DeepFilterNet) on YouTube, Facebook, Instagram, X, and any HTML5 video. | v0.1 α  |
| [`@sukoon/desktop`](./apps/desktop)     | Tauri desktop app (Windows/macOS/Linux). Drag-and-drop file cleaning (MDX-Net).                                                             | planned |
| [`@sukoon/web`](./apps/web)             | Upload-and-clean web app + landing page. Runs MDX-Net client-side (WASM), on-device.                                                        | planned |

Mobile shells (Android/iOS, on-device separation) are tracked in the [ROADMAP](./ROADMAP.md#phase-3--android) and embed `sukoon-core` via FFI.

## Quickstart

```bash
# Clone and build the core + CLI with real inference
git clone https://github.com/tamimbinhakim/sukoon.git
cd sukoon
cargo build --release -p sukoon-cli --features onnx

# Clean a single video (MDX/Kim Vocal, on-device). Weights download on first run.
./target/release/sukoon clean input.mp4 -o output.mp4

# Batch a whole folder
./target/release/sukoon batch ./vlogs --out ./vlogs-clean
```

Full setup — including FFmpeg, model downloads, and the JS workspaces — is in [docs/start/installation.md](./docs/start/installation.md).

## Status

Sukoon is in **early alpha** (`0.1.0-alpha`). The Rust core, CLI, and browser extension are the first targets; desktop, web, and mobile follow the [phased roadmap](./ROADMAP.md). APIs and the model interface will change before `0.1.0`. Pin commits if you build on it now.

## Contributing

The most valuable contributions early on are **evaluation clips** and **honest quality reports** — "MDX left music under this nasheed," "the voice got clipped on this lecture." See [CONTRIBUTING.md](./CONTRIBUTING.md) and the [model-eval guide](./docs/contributing/model-eval.md). Scholarly review of the halal-aware framing is coordinated through [GOVERNANCE.md](./GOVERNANCE.md).

## License

Sukoon's own code is [Apache-2.0](./LICENSE). It integrates third-party models and FFmpeg, each under its own terms — read [LICENSING.md](./LICENSING.md) **before** shipping a binary, especially regarding model weights and FFmpeg build flags.

<div align="center"><sub>Built with the intention that it benefits the Ummah. <code>سكون</code> — stillness.</sub></div>
