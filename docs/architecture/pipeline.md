# Pipeline

How audio gets in, through the model, and back into the video. Source:
[`pipeline.rs`](../../packages/core/src/pipeline.rs) +
[`ffmpeg.rs`](../../packages/core/src/ffmpeg.rs).

## Steps

```
1. extract_audio   ffmpeg -i in.mp4 -vn -ac 2 -ar <engine_sr> -c:a pcm_f32le in.wav
2. separate        decode in.wav → AudioBuffer → Engine.separate() → speech stem → speech.wav
3. remux           ffmpeg -i in.mp4 -i speech.wav -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac out.mp4
```

If the input has no video stream, step 3 is skipped and the cleaned audio is written directly.

Steps 2 and 3 **overlap**: the engine streams cleaned PCM into FFmpeg as it's produced, so the AAC
audio re-encode runs **concurrently** with separation rather than adding to total time. The video is
never re-encoded (`-c:v copy`). A full 24-minute (1429 s) video runs through the Fast engine in
~19 s (~74× real-time) end-to-end — extract + separation + the overlapped re-encode (~14 s of which
hides under separation). Full figures: [docs/reference/performance.md](../reference/performance.md).

## Why these choices

- **`-c:v copy`** — never re-encode video. Fast and lossless; Sukoon only touches audio.
- **Extract at the engine's rate** — FFmpeg resamples (soxr-quality) straight to the engine's native
  rate (`<engine_sr>` — 44.1 kHz for MDX-Net), so the pipeline never resamples in Rust. Output is
  32-bit float (`pcm_f32le`) to feed the engine without quantization.
- **`-shortest`** — guards against fractional length mismatches at chunk boundaries. If drift ever
  appears, `aresample=async=1000` on the audio fixes it.
- **Explicit stream maps** (`-map 0:v:0 -map 1:a:0`) — keeps A/V sync deterministic.

## Chunking & bounded-memory streaming

On the first separation the engine's weights **download automatically** with a live progress
indicator (downloaded/total MB, %); nothing is bundled, and subsequent runs reuse the cached file.

A stateful engine chunks internally: MDX-Net processes ~6 s windows with overlap-add (trim
`n_fft/2` each side), so boundary artifacts are avoided.

For short clips the pipeline decodes the whole track into an `AudioBuffer`. For a **long** file
processed by a streaming-capable engine (MDX) already at the engine's sample rate, it instead
**streams**: a `WavBlockReader` feeds the engine one block at a time via `Engine::chunk_plan` +
`separate_block`, and a `WavBlockWriter` writes each cleaned block as it's produced. Peak memory is
bounded by one block (~32 MB) regardless of file length — an hour of stereo audio no longer needs
multiple GB resident. Because each block is handed the surrounding STFT context (`n_fft/2` on each
side), the streamed output is **byte-identical** to the whole-buffer path (an equivalence test
guards this). Inputs needing a resample, or files smaller than one block, use the whole-buffer path.

## FFmpeg build

Ship an **LGPL** FFmpeg and link dynamically (or invoke the system binary). The audio-only path
needs no GPL-only components, which keeps Sukoon's Apache-2.0 license clean. Document the exact
build flags you ship. See [LICENSING.md](../../LICENSING.md#2-ffmpeg).

Override the binaries with `SUKOON_FFMPEG` and `SUKOON_FFPROBE`.
