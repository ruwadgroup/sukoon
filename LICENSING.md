# Licensing & third-party notices

Sukoon's **own source code** is licensed under [Apache-2.0](./LICENSE).

That license covers the code in this repository. It does **not** automatically cover the
models, datasets, and binaries Sukoon integrates at runtime. Those carry their own terms,
and some of them materially affect how you may ship a binary. **Read this page before you
distribute any build, and get legal review before a commercial release.**

---

## 1. Separation model weights

Sukoon does not bundle model weights in source control. They are downloaded on first use.
The license that applies is the license of the weights you choose to ship or host.

| Model                             | Role in Sukoon                     | Code license       | Weights license                       | Safe to bundle in a closed binary?                                          |
| --------------------------------- | ---------------------------------- | ------------------ | ------------------------------------- | --------------------------------------------------------------------------- |
| **MDX-Net (Kim Vocal 2)** (UVR)   | File tools separator (**default**) | MIT (MDX-Net code) | **Community / unverified provenance** | ❌ **No** — download at runtime; do not bundle                              |
| **DeepFilterNet 3**               | Browser-extension engine           | Apache-2.0         | Apache-2.0                            | ✅ Yes (bundle-safe) — but downloaded on first use to keep the binary small |
| **MDX-Net UVR 9482** (`mdx-lite`) | File tools low-RAM fallback        | MIT (MDX-Net code) | **Community / unverified provenance** | ❌ **No** — download at runtime; do not bundle                              |

**Action items before launch:**

- The **MDX-Net (Kim Vocal 2)** weights are community-trained and distributed through the UVR /
  `TRvlvr/model_repo` project; their training data and redistribution terms are **not formally
  verified**. Sukoon downloads and SHA-256-verifies them at runtime and **never bundles** them.
  Do not embed them in a distributed binary. If you need bundled separation weights, ship a model
  whose license and provenance you've verified yourself.
- The **low-RAM fallback** (`mdx-lite`, UVR MDX-Net 9482) is the same class of weight
  (`CommunityDownloadOnly`), so the same action item applies — download at runtime, never embed it
  in a distributed binary.
- The **DeepFilterNet 3** browser-extension weights are **Apache-2.0** and therefore bundle-safe, but
  Sukoon still downloads (and extracts) the 3-file ONNX bundle on first use to keep the binary small.
- The model registry in `sukoon-core` records each model's license alongside its checksum, and
  `bundle_safe()` returns `false` for the MDX weights (`CommunityDownloadOnly`) and any share-alike
  (CC-BY-SA-4.0) weight, so a build can refuse to embed them in a closed target. See
  [docs/architecture/engines.md](./docs/architecture/engines.md#model-registry).

## 2. FFmpeg

Sukoon shells out to / links FFmpeg for audio extraction and remuxing.

- FFmpeg is **LGPL-2.1+** by default, but **GPL** if built with `--enable-gpl` (or with
  certain codecs such as `libx264`). Sukoon's audio-only pipeline does **not** need any
  GPL-only component.
- **Ship an LGPL build** and link it dynamically, or invoke the system `ffmpeg` binary, to
  keep Sukoon's Apache-2.0 license clean.
- Document the exact FFmpeg build flags you ship. See
  [docs/architecture/pipeline.md](./docs/architecture/pipeline.md#ffmpeg-build).

## 3. ONNX Runtime

The inference backend (`ort` crate → ONNX Runtime) is **MIT**. No redistribution concerns.

## 4. Summary for distributors

| If you are shipping…                  | Then…                                                                              |
| ------------------------------------- | ---------------------------------------------------------------------------------- |
| The CLI / desktop app (MDX-Net)       | Your code + LGPL FFmpeg are clean; MDX weights download at runtime — don't bundle. |
| A binary with bundled MDX-Net weights | Don't use the MDX community weights; ship a model whose license you've verified.   |
| The browser extension (DeepFilterNet) | Apache-2.0 weights are bundle-safe; still publish your model attributions.         |

This document is engineering guidance, not legal advice. When in doubt, ask a lawyer who
understands open-source and ML weight licensing.
