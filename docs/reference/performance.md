# Performance & device support

How fast Sukoon is, and what it runs on. All figures below are **measured** on Apple Silicon
unless noted; treat them as a realistic baseline, not a guarantee for every machine. Speeds are
quoted as a **real-time multiple** — "180× real-time" means one minute of audio is cleaned in
about a third of a second.

> The honest summary: the **Fast** engine is faster than real-time on basically any modern CPU and
> needs no GPU; the **HQ** engine is heavier and wants a GPU but runs on CPU too. Everything runs
> on-device.

## How fast is it?

| Engine                         | Model  | Speed (real-time multiple)                                        | Where the time goes                                           |
| ------------------------------ | ------ | ----------------------------------------------------------------- | ------------------------------------------------------------- |
| **Fast** — DeepFilterNet       | ~8 MB  | **~180× on CPU** (no GPU needed; CPU beats GPU for this tiny net) | Almost all inference; runs real-time on a single modern core. |
| **HQ** — MDX-Net (Kim Vocal 2) | ~67 MB | **~4× on CPU**, **~12–15× on GPU** (auto-selected)                | Inference-bound; the GPU is a ~3× win.                        |
| **Fallback** — MDX-lite (9482) | ~30 MB | between Fast and HQ; for low-RAM machines                         | Same path as HQ, smaller model.                               |

**Concrete end-to-end number.** A full **24-minute (1429 s) video** cleaned with the Fast engine
finishes in **~19 s end-to-end** — that's **~74× real-time**, and it includes everything: FFmpeg
audio extract, separation, and re-encoding the cleaned audio back into the MP4.

Why "only" ~74× end-to-end when the engine itself does ~180×? Because once separation is that fast,
the **AAC re-encode** (~14 s of those 19 s for a 24-min clip) becomes the wall-clock floor. Sukoon
runs that encode **concurrently** with separation — the engine streams cleaned audio into FFmpeg as
it's produced — so the encode overlaps the inference instead of adding to it. The video itself is
never re-encoded (`-c:v copy`).

- A 90-second clip on the **HQ** engine takes **~6 s** on an Apple M4 (CoreML), or ~22 s on CPU.
- **Long files** are processed in bounded-memory blocks, so peak RAM stays roughly constant (≈ one
  block) no matter how long the input is.
- **Repeat runs are instant** — cleaned audio is cached by content hash (input + engine + mode).
- **First run** downloads the model weights automatically, with a live progress indicator. After
  that, everything is offline.

## What devices can run it?

### Fast engine (DeepFilterNet) — runs almost anywhere

The Fast engine is a tiny recurrent network designed for real-time speech enhancement on a single
CPU core. It needs **no GPU and no special hardware**.

- ✅ **Laptops and desktops** (any modern x86-64 or ARM CPU) — comfortably faster than real-time.
- ✅ **Phones / tablets** (mid-range and newer) — this is the engine the mobile and live-playback
  shells use, precisely because it's light and real-time.
- ✅ **Low-power / older machines** — it's ~8 MB and CPU-only; it degrades gracefully rather than
  failing.

This is the **on-device, low-latency, low-power** path: no GPU, and no network after the one-time
weight download.

### HQ engine (MDX-Net) — wants a desktop, ideally with a GPU

The HQ engine is a ~67 MB U-Net doing true voice/instrumental separation. It's heavier and benefits
a lot from a GPU.

- ✅ **Desktop / laptop with a GPU** — best experience. Acceleration is auto-selected per platform
  (see below).
- ✅ **CPU-only desktop / laptop** — works fine for files and batch (~4× real-time), just without
  the GPU speedup.
- ⚠️ **Phones / weak devices** — possible but slow; prefer the Fast engine on-device.
- 💾 Needs a few hundred MB of RAM for inference (the **Fallback** MDX-lite trims the model for
  tighter memory budgets).

## Hardware acceleration

Acceleration is **automatic** — the core picks the best backend for the platform and silently falls
back to CPU if anything is missing. There are no performance knobs to learn.

| Platform    | Accelerator                                      | Notes                                                         |
| ----------- | ------------------------------------------------ | ------------------------------------------------------------- |
| **macOS**   | CoreML (GPU / Neural Engine)                     | On by default. ~3× faster on HQ, validated < −59 dB residual. |
| **Windows** | DirectML (any Direct3D 12 GPU: NVIDIA/AMD/Intel) | On by default; ubiquitous on Windows 10+.                     |
| **Linux**   | CUDA (NVIDIA)                                    | Opt-in build (`--features cuda`); default build is pure CPU.  |
| _any_       | CPU                                              | Always available as the fallback.                             |

Notes:

- The **Fast engine deliberately stays on CPU** even when a GPU is present — for a small recurrent
  model the GPU launch overhead makes it _slower_ (~132× on CoreML vs ~180× on CPU, measured), and
  staying on CPU leaves the GPU free for the HQ path.
- Set `SUKOON_CPU_ONLY=1` to force CPU everywhere (useful when a GPU driver misbehaves).
