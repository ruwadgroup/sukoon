# Design considerations

This document records the **constraints we deliberately designed around** — the _why_ behind
Sukoon's architecture. Many of these are ethical and religious considerations, not just technical
ones. They are binding: a change that violates one of these is not a tradeoff to weigh, it's a
non-starter. Contributors and reviewers should treat this page as the rationale behind the rules in
[CONTRIBUTING.md](../CONTRIBUTING.md) and [GOVERNANCE.md](../GOVERNANCE.md).

---

## 1. Ads play in full — with their music removed

**Decision:** Sukoon never blocks, skips, mutes, or disables advertisements — every ad plays in
full. The music-removal engine runs **uniformly over all audio**, so an ad's background music is
removed like everything else. Sukoon does not special-case ads in either direction: it neither
suppresses them nor exempts them.

**The distinction that matters:** _blocking_ an ad (preventing it from playing) is categorically
different from _processing the audio of an ad that does play_. Sukoon does the latter, never the
former. The user still sees and hears the complete ad; only its background music is gone.

**Trade-off — taken on deliberately.** Removing music from an ad **alters the ad's audio**, and that
is a real compliance risk we are choosing to accept:

- **Copyright / altering others' content.** An ad is third-party content delivered by the platform;
  modifying its audio (even only on-device, even only the music) is an unauthorized-modification
  question regardless of what the ad contains.
- **Platform Terms of Service.** YouTube's terms prohibit interfering with ads. Altering ad audio
  plausibly falls under that.
- **Browser-store policy.** Chrome Web Store / Firefox AMO scrutinize extensions that modify ads.
  Removing music from ads may draw review even though we never block them.

> This is _not_ a "protect the creator's revenue / _rizq_" argument — ads frequently contain music
> and other impermissible content, so we make no claim an ad is worth preserving on its merits. The
> point is only to be honest that we _do_ modify ad audio, and that doing so carries the risks above.

**Consequence in code:** there is **no** ad-bypass path — the engine processes whatever is playing.
Sukoon must still never block, skip, mute, or disable an ad; PRs adding ad-blocking behavior will be
closed.

---

## 2. Why we never download the video

**Decision:** On YouTube, Sukoon processes audio **live, in the page, as it plays — in real time,
frame by frame**. It does not read ahead, download, save, re-host, or rip the video or audio, and
never creates a persistent copy. It only changes what comes out of the speakers, in real time, on the
user's own device, for their own listening — the publishing channel (the user watching on YouTube) is
untouched.

**Why:**

- **The permissibility of downloading is itself questionable.** Saving a copy of someone's video
  — especially to redistribute or to bypass how they chose to publish it — raises both
  rights-of-the-creator concerns and, for many users, a personal _halal_ concern. Rather than make
  that decision for the user, we avoid it entirely: we never create a copy.
- **YouTube's Terms of Service** prohibit downloading content except through features YouTube
  provides. Building download/rip functionality would put the project — and its users — on the
  wrong side of those terms and invite takedowns.
- **It keeps the ethical story clean.** "We don't copy the video, we don't block ads, we just
  filter what you hear" is a position we can state plainly and defend.

**The distinction that matters:**

| Live filtering (what Sukoon does on YouTube) | Downloading/ripping (what Sukoon does **not** do) |
| -------------------------------------------- | ------------------------------------------------- |
| Audio is processed in real time, in memory   | A persistent copy of the media is created         |
| Nothing is saved or redistributed            | A file exists that can be shared/re-hosted        |
| The creator's publishing choice is respected | The publishing channel is bypassed                |
| No ToS conflict                              | Conflicts with YouTube ToS                        |

**Where file processing _is_ allowed:** the desktop/CLI/web tools clean **files the user already
has** (their own recordings, downloads they obtained elsewhere, content they own). That is the
user's media and the user's decision. Sukoon provides the tool; it does not source the media.

---

## 3. Privacy: fully on-device

**Decision:** Audio is processed **entirely on the user's device**. Nothing is uploaded — there is no
cloud service.

**Why:**

- People watch private, personal, and religious content. Sending that audio to a server would be a
  betrayal of trust and a needless privacy risk.
- The engines run locally on ordinary hardware — DeepFilterNet is real-time on a CPU; MDX-Net is
  ~4× real-time on a CPU and ~12–15× with a GPU — so there is no need to upload. See
  [reference/performance.md](./reference/performance.md).

**Consequence:** the cache is local-only (see [SECURITY.md](../SECURITY.md)).

---

## 4. We never remove the voice

**Decision:** Speech, narration, lectures, and **recitation** are always preserved. The product
removes _instrumental background music_, not vocals.

**Why & the hard edge case:** nasheed-style content, where a recited/sung vocal sits over
instrumental backing, is acoustically and _philosophically_ blurry — the thing one person wants
removed (instruments) is entangled with the thing that must be kept (the voice). We treat
"preserve the voice" as inviolable — the engine's attenuation is capped rather than risk clipping
recitation, and the [scholarly framing](./halal-aware/index.md) is documented. A dedicated test
corpus exists for exactly this case.

---

## 5. Sukoon presents scholarly positions — it does not issue rulings

**Decision:** Sukoon's core behaviour — **remove the instrumental music, always keep the voice** —
maps to the broadest common position, shipped with a short, sourced explanation. It is the default
and, today, effectively the only mode the working engine implements. It is **not** an endorsement of
one ruling over another.

**Why:** Muslims follow different qualified scholars in good faith. A software project is not a
_mufti_. Picking a side and presenting it as "the" ruling would mislead users and exceed our
competence. We present the behaviour neutrally and cite sources; a scholarly advisor reviews the
framing before it ships ([GOVERNANCE.md](../GOVERNANCE.md), [scholarly-positions.md](./halal-aware/scholarly-positions.md)).

**Scope note (current):** finer instrument-level toggles (keep percussion/duff, preserve effects)
would require a multi-stem engine and are **not implemented**. They remain possible future options;
the `SeparationMode` enum keeps placeholders for them, but the shipping engine only does
remove-music / keep-voice. We would rather do that one thing well than expose modes the model can't
honestly honour.

---

## 6. Quality separation now; low-friction live paths later

**Decision (native):** The default native engine is **MDX-Net (Kim Vocal 2)**, running on **ONNX
Runtime** — true voice/instrumental separation, on-device, the default for `clean`.

**Decision (Chrome extension):** the extension ships **one engine — DeepFilterNet, real-time** —
because the browser can't run a heavy separator in real time on a live element we never download. Its
attenuation is capped (gentle) so its speech-enhancer objective is less likely to thin melodic
recitation. We built and **removed** in-browser MDX separators (an offscreen ONNX Runtime fed by an
MSE tap reading buffered-ahead audio); they were too fragile on live YouTube — see
[research/extension-trials.md](./research/extension-trials.md). The path to a real-time, best-quality
separator is [Sukoon's own model](./research/own-model-plan.md). See
[platforms/extension.md](./platforms/extension.md).

**Why the lineup changed (a deliberate technical pivot):** the original plan was DeepFilterNet (Fast,
real-time) plus BandIt Plus (HQ). Two things forced a change:

- **DeepFilterNet's pure-Rust runtime won't load its model on our toolchain.** Its `tract` backend
  hits an optimizer bug (`duplicate name /convt3/Conv.bias` during graph codegen) on the bundled
  DeepFilterNet3 model — and it reproduces even with DeepFilterNet's _own_ pinned tract versions
  (across tract 0.21.x). Vendoring and patching the whole crate to work around it was
  disproportionate, so the real-time engine instead runs the three DFN3 ONNX graphs through **ONNX
  Runtime** (the same runtime MDX uses), keeping only `deep_filter`'s DSP. It's functionally
  validated but not yet bit-exact-verified against the upstream reference.
- **MDX-Net is the right tool for the actual goal.** It does direct voice/instrumental separation,
  runs reliably via ONNX Runtime, and is fast enough for files and batch (see Section 3), which is
  where separation quality matters most. BandIt Plus was dropped in its favour.
- **DeepFilterNet is not a true separator, but it is the only real-time engine.** As a speech enhancer
  it can thin melodic recitation, so it is not the _quality_ ceiling — but it is the one engine that
  keeps live audio/video in sync, so in the extension it is the only engine (gently tuned). True
  separation stays a file operation (MDX-Net in the CLI/desktop). See
  [reference/performance.md](./reference/performance.md).

---

## 7. Model-weight licensing is a shipping constraint, not an afterthought

**Decision:** The engine registry records each model's **license**, and `bundle_safe()` lets a build
refuse to embed a weight that shouldn't be redistributed — a share-alike weight (CC-BY-SA-4.0) or a
community model of unverified provenance (`CommunityDownloadOnly`, like the MDX weights). Those are
**downloaded at runtime and SHA-256-verified, never bundled** into the binary.

**Why:** "the code is MIT" does not mean "the weights are MIT." Getting this wrong means shipping a
binary that violates a license. It's encoded in [`registry.rs`](../packages/core/src/registry.rs)
and explained in [LICENSING.md](../LICENSING.md) so it can't be quietly forgotten.

---

## 8. iOS framing

**Decision:** The iOS app is framed as an **audio-editing / media-accessibility** tool. It never
describes itself as circumventing or censoring other apps.

**Why:** App Store review rejects tools framed as altering other services. The _function_ (clean
the audio of media the user is playing/processing) is legitimate; the _framing_ must match how
Apple categorizes media tools, or it won't ship.

---

## Summary: the lines we will not cross

1. We do not touch ads.
2. We do not download/rip or persist YouTube video — we only process the stream the browser already
   buffered, in memory, and discard it.
3. We do not upload audio — processing is fully on-device.
4. We do not remove the voice / recitation.
5. We do not declare religious rulings.
6. We do not bundle model weights that shouldn't be redistributed (share-alike or unverified
   community weights) into closed binaries — we download and verify them at runtime instead.

Everything else is a normal engineering tradeoff. These six are not.
