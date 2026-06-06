# Model evaluation

Separation quality is the whole product. Numbers like SDR are useful, but **subjective listening on
real content** is what decides whether Sukoon is good. This page is how we evaluate it — and how you
can help most.

## The eval corpus

We maintain a small set of representative clips. Each should be short, license-clear, and labeled
with what's hard about it:

| Category                  | Why it's in the corpus                                         |
| ------------------------- | -------------------------------------------------------------- |
| Light-music vlog          | The common case; should be near-perfect.                       |
| Lecture / talk            | Sparse speech over a bed; tests under-removal.                 |
| **Nasheed**               | Voice over instrumental backing — the hardest, most important. |
| Podcast w/ synth underlay | Continuous low music; tests leakage.                           |
| Abrupt music↔silence      | Transitions; tests clicks/artifacts.                           |
| Dense/loud mix            | Stress test for the MDX-Net file separator.                    |

Keep eval clips **local** — never commit media (it's git-ignored).

## How to report a quality problem

Use the [separation-quality issue template](https://github.com/ruwadgroup/sukoon/issues/new?template=model_eval.yml).
Include:

- **Tool**: browser extension (DeepFilterNet) or file separator (MDX-Net / UVR fallback).
- **Problem kind**: music left in / speech damaged / artifacts / nasheed edge case.
- **Timestamps** and what you heard vs expected.
- A short clip or a public link + timestamps, if possible.
- Platform + hardware.

## The gate

A change to either separator should not regress the corpus. The Phase 0 gate
([ROADMAP](../../ROADMAP.md#phase-0--validation)) is: subjective "clean speech, music gone" ≥ 4/5 on
≥ 80% of MDX-Net file-separator samples. The extension's real-time DeepFilterNet engine can be
eval'd the same way, though the gate itself is defined on the file separator.

## Subjective beats SDR here

A residual "musical noise" or a quick gating artifact in the speech stem is more disturbing to a
listener than slightly muffled speech, even if SDR says otherwise. When in doubt, trust careful
listening — and prefer `--overlap 0.5` for final-pass quality over the faster preview setting.

## The nasheed principle

When the voice and the instruments are entangled, **keep the voice**. We would rather leave a little
backing audible than clip recitation. See
[design-considerations §4](../design-considerations.md#4-we-never-remove-the-voice).
