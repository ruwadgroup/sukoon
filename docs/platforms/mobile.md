# Mobile (Android & iOS)

Strategic priority — the core markets (Bangladesh, Indonesia, Pakistan, Egypt, Gulf) are
Android-first. Mobile shells embed `sukoon-core` via FFI. File separation uses **MDX-Net**
(Kim Vocal 2), on-device. `sukoon-core` also has a working real-time engine —
**DeepFilterNet** (~180× real-time on CPU; ~8 MB model) — which runs on-device in real time
on a single CPU core with **no GPU required**, so it's a good fit for mid-range-and-newer phones.
Building this live path into the mobile shells is the remaining (new) work. See
[reference/performance.md](../reference/performance.md).

> Status: planned. The shells aren't scaffolded yet; this is the spec. See
> [ROADMAP Phase 3/4](../../ROADMAP.md#phase-3--android).

## Android

- Kotlin shell + `sukoon-core` via **JNI** (the core compiled as a shared lib for `arm64-v8a`,
  `armeabi-v7a`, `x86_64`).
- MDX-Net runs via **ONNX Runtime** for file cleaning; the core's real-time **DeepFilterNet**
  engine exists, but the live path isn't wired into the Android shell yet, so live filtering is pending.
- Features: pick local video/audio, **share-sheet** ("clean this video"), built-in player, local cache.
- MDX-Net runs on-device where the device can run it at usable speed.
- Floor: Android 8+. File cleaning usable on mid-range hardware.

## iOS

- Swift shell + `sukoon-core` via the C ABI.
- MDX-Net via **ONNX Runtime** for file cleaning; the core's real-time **DeepFilterNet**
  engine exists, but the live path isn't wired into the iOS shell yet.
- Share-extension to process videos from other apps; built-in player.
- **Framing matters:** present as an _audio-editing / media-accessibility_ tool. Never describe it
  as circumventing or censoring other apps, or App Store review will reject it. See
  [design-considerations §8](../design-considerations.md#8-ios-framing).

## Shared constraints

- We don't download YouTube video on mobile either — live handling runs the on-device engine, not
  ripping. See [design-considerations §2](../design-considerations.md#2-why-we-never-download-the-video).
- Fully on-device.
