# Concepts

The mental model behind Sukoon. Short, because the design is deliberately small.

## The job

Remove **instrumental background music** from media, keep **speech** (dialogue, narration,
lectures, recitation, vocals). Sukoon swaps the audio track; it never re-encodes or alters the
video frames.

## Engines

| Context                 | Engine                               | Notes                                                                |
| ----------------------- | ------------------------------------ | -------------------------------------------------------------------- |
| **Extension** (live)    | DeepFilterNet — real-time, on-device | Speech-preserving enhancer in an AudioWorklet; in sync, no download. |
| **Files** (CLI/desktop) | MDX-Net (Kim Vocal 2); UVR 9482      | True 2-stem voice/instrumental separation via ONNX Runtime.          |

In the browser, real-time wins (DeepFilterNet); true separation (MDX-Net) is a file operation. The
in-browser separators were tried and removed
([extension trials](../research/extension-trials.md)); a real-time, best-quality separator is the goal
of [Sukoon's own model](../research/own-model-plan.md). Full detail:
[architecture/engines.md](../architecture/engines.md).

## Modes

For the current 2-stem engine:

- `remove-all` (default) and `keep-vocals` both resolve to "keep the voice/vocal stem, drop the
  instrumental." They're the meaningful modes today.
- `keep-percussion` and `preserve-effects` would need a multi-stem engine and are **not
  implemented** — placeholders, not shipping features.

The behaviour maps to the broadest scholarly position — [halal-aware/index.md](../halal-aware/index.md).

## Live vs file processing

- **File mode** (CLI, desktop files, batch): decode → separate → remux a new file. Quality-first.
- **Live mode** (extension, in-app players): audio is routed through a processing node while the
  source plays. The extension runs **DeepFilterNet in real time**. Music is removed from all audio,
  including ads (ads still play in full) — see
  [platforms/extension.md](../platforms/extension.md#ad-handling).

## Caching

Cleaned stems are cached by **content hash** (input bytes + engine + mode), local-only by default.
First run processes; replays are instant. [architecture/core.md](../architecture/core.md#caching).

## On-device by default

Audio is processed **entirely on-device** — nothing is uploaded. The reasoning — and the other
binding constraints — are in [design-considerations.md](../design-considerations.md).
