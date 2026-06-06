# Quickstart

Assuming you've [installed](./installation.md) `sukoon` and FFmpeg.

## Clean one file

```bash
# HQ (MDX-Net) is the default → vlog.clean.mp4
sukoon clean vlog.mp4
```

The video stream is copied losslessly; only the audio is replaced with the speech-only stem.

## Pick the engine

```bash
# HQ (MDX-Net via ONNX) is the default — explicit form, with output path
sukoon clean lecture.mp4 --engine hq -o lecture.clean.mp4

# Fast (DeepFilterNet) — the real-time speech enhancer; requires a build with `--features dfn`
sukoon clean talk.mp4 --engine fast

# Low-RAM fallback (MDX-lite, UVR 9482 — real, smaller, lower quality than HQ)
sukoon clean old.mp4 --engine fallback
```

## Pick a mode

```bash
sukoon clean nasheed.mp4 --mode remove-all     # default: drop the instrumental, keep the voice
sukoon clean nasheed.mp4 --mode keep-vocals    # same result on the 2-stem MDX engine
```

The 2-stem MDX-Net engine separates voice from instrumental, so `remove-all` and `keep-vocals` are
effectively identical: keep the vocal stem, drop the music. Multi-stem modes like
`keep-percussion` and `preserve-effects` need a multi-stem engine and are not implemented yet.

See [halal-aware modes](../halal-aware/index.md) for what each mode means.

The first run downloads the model weights automatically (with a live progress indicator); after
that it's offline. For how fast each engine is and which devices it runs on, see
[performance & device support](../reference/performance.md).

## Batch a folder

```bash
sukoon batch ./vlogs --out ./vlogs-clean --engine hq
```

## Audio-only files

`clean` works on bare audio too (`.mp3`, `.wav`, …) — it writes the cleaned audio directly when
there's no video stream.

```bash
sukoon clean podcast.mp3 -o podcast.clean.mp3
```

## Logging

```bash
sukoon clean vlog.mp4 -v        # debug
RUST_LOG=sukoon=trace sukoon clean vlog.mp4
```

Next: **[Concepts](./concepts.md)**.
