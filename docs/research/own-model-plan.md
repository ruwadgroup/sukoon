# Proposal: A Speech-Preserving, Real-Time Music-Separation Model for Sukoon

| Field             | Value                                                                                                                                                                                                                         |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Document type** | Technical / research proposal                                                                                                                                                                                                 |
| **Status**        | Draft for review — not yet implemented                                                                                                                                                                                        |
| **Version**       | 0.4                                                                                                                                                                                                                           |
| **Date**          | 2026-06-06                                                                                                                                                                                                                    |
| **Maintainers**   | Sukoon project                                                                                                                                                                                                                |
| **Related**       | [`extension-trials.md`](./extension-trials.md), [`design-considerations.md`](../design-considerations.md), [`ARCHITECTURE.md`](../../ARCHITECTURE.md), [`ROADMAP.md`](../../ROADMAP.md), [`LICENSING.md`](../../LICENSING.md) |

---

## Abstract

Sukoon removes background **music** from media while preserving **speech**, including Qur'anic
recitation and other narration. The **shipped** engines are an existing pre-trained pair:
**DeepFilterNet 3** runs the real-time, in-page **extension** path (a speech _enhancer_, not a true
separator), while **MDX-Net (Kim Vocal 2)** — a 2-stem voice/instrumental model on ONNX Runtime —
handles **file** separation in the CLI and desktop (see
[`architecture/engines.md`](../architecture/engines.md), and the live-path post-mortem in
[`extension-trials.md`](./extension-trials.md) for why the browser dropped to a single engine). This proposal specifies a separate,
forward-looking plan to train Sukoon's **own** open-source separation model with four properties:
(1) speech and recitation are preserved as an explicit, mathematically-enforced objective; (2) three
quality/latency tiers (Fast, Balanced, HQ) are served from a single elastically-sized checkpoint;
(3) the project's listening modes (`remove-all`, `keep-vocals`, `keep-percussion`,
`preserve-effects`) are produced natively as a linear post-process over a five-stem decomposition;
and (4) the Fast tier runs faster than real time with bounded low latency for live, in-page
filtering.

The design is a causal, single-path **TFC-TDF U-Net** in the STFT domain (after RT-STT, arXiv:2511.13146)
with a softmax multi-stem masking head that guarantees mixture consistency, trained by
**knowledge distillation** from offline teacher separators (Mel-Band RoFormer, BandIt Plus, Demucs v4,
DeepFilterNet) and supervised by labelled and synthetically-remixed data. The three tiers are nested
sub-models of one supernet (slimmable / matryoshka training with in-place distillation). The proposal
is scoped to a **near-zero training budget** using free compute (Kaggle, Colab, local Apple silicon)
and pre-trained models for the heaviest tier; a clearly-marked [Funding](#15-funding-and-sponsorship)
section describes how donations — used strictly for compute, data, and storage — would improve the
result.

---

## 1. Background and motivation

Sukoon is a monorepo with **one engine and many shells**: the separation logic lives once in
`sukoon-core`, and each platform (CLI, browser extension, desktop, mobile) is a thin adapter
(see [`ARCHITECTURE.md`](../../ARCHITECTURE.md)). The engines today **wrap third-party models**, and
the two shipped paths now differ by design:

- **Extension (live).** After a whole browser-separator ladder — High/Lite MDX-Net via an MSE tap +
  WebCodecs decode + an offscreen ONNX-Runtime document, holding the video behind a priming buffer —
  was built and then **removed** for live-stream fragility (the full post-mortem is in
  [`extension-trials.md`](./extension-trials.md)), the extension ships **only DeepFilterNet 3**. It is
  compiled to WASM through the `deep_filter` crate's pure-Rust `tract` runtime (`@sukoon/dfn-wasm`,
  model embedded, no network), streaming **48 kHz mono in 480-sample hops** (~10 ms/hop plus the
  model's ~2-frame look-ahead). Attenuation is **capped** (≤ 40 dB) and DFN's **post-filter**
  (`β = 0.02`) is enabled to curb the residual musical-noise/warble — currently the cheapest lever
  toward offline quality.
- **Files (CLI / desktop).** **MDX-Net (Kim Vocal 2)**, a pre-trained 2-stem voice/instrumental
  separator, run via **ONNX Runtime** — latency and live-sync don't apply to file processing. (The
  native build can't reuse the browser's `tract` path: a desktop-toolchain optimizer bug on the DFN
  graph forces ORT there.)

This is a deliberate floor, not a ceiling we are content with — and its structural limits are exactly
what motivate a purpose-built model:

- **No control over the separation taxonomy.** DeepFilterNet is a speech _enhancer_ — a two-stage
  ERB-gain + deep-filtering model that suppresses everything non-speech. It emits one enhanced speech
  signal, not named stems, so it cannot, for example, keep a hand-drum while removing melodic
  instruments.
- **No native support for the listening modes.** The halal-aware modes
  ([`docs/halal-aware`](../halal-aware/index.md)) can only be approximated, not produced directly by
  the model.
- **No project-owned tier that is simultaneously high-quality and real-time.** Strong separators
  (BandIt, RoFormer, Demucs) are offline; the real-time model is a denoiser.
- **No specialisation for recitation**, the project's most important and most acoustically delicate
  case, which existing models rarely see in training.

A purpose-built model addresses all four. This document specifies how, under tight resource
constraints.

---

## 2. Problem statement and objectives

### 2.1 Objectives

- **O1 — Speech/recitation preservation.** Never remove or audibly degrade speech or recitation; a
  binding product constraint ([`design-considerations.md`](../design-considerations.md) §4),
  enforced by §6.5–6.6.
- **O2 — Native listening modes.** Produce a decomposition from which all four modes follow as a
  fixed linear map (§6.2).
- **O3 — Quality/latency tiers.** Serve Fast, Balanced, and HQ operating points from one checkpoint
  (§6.4).
- **O4 — Real-time Fast tier.** Real-time factor (RTF) < 0.5 on a mid-range CPU, algorithmic latency
  ≤ ~25 ms, stateful streaming (§9.3).
- **O5 — Trainable under near-zero budget.** Executable primarily on free compute (§12).

### 2.2 Feasibility constraint (a design principle)

A single fixed network **cannot** simultaneously match the best offline quality and the lowest
latency; this quality–latency trade-off is consistent across the literature (§3, Table 1) — the
best offline model (DTTNet, 8.06 dB) and the best real-time model (RT-STT, 5.17 dB) differ by
~3 dB SDR. The proposal therefore treats the three tiers as **operating points on a quality–latency
frontier**, served by one elastic model, and targets the one place with published headroom:
**raising the quality of the real-time operating point** via distillation, since RT-STT attains the
real-time state of the art with only 383 K parameters and ~1 ms/frame, implying spare capacity.

---

## 3. Related work and state of the art

All figures are from the cited papers on MUSDB18-HQ unless noted.

**Table 1 — reference systems.**

| System                     | Regime     | SDR (all) | Params | Algo. latency | Per-frame compute    |
| -------------------------- | ---------- | --------- | ------ | ------------- | -------------------- |
| DTTNet (offline)           | non-causal | 8.06 dB   | 20 M   | ~6 s          | heavy                |
| Demucs v4 / HT-Demucs      | non-causal | 7.52 dB   | 41 M   | ~7.8 s        | heavy                |
| HS-TasNet (real-time, '24) | causal     | 4.65 dB   | 42 M   | 23 ms         | 3.9 ms/frame         |
| RT-STT (real-time, '25)    | causal     | 5.17 dB   | 383 K  | 23 ms         | 1.01 ms/frame (fp16) |

**Architectural lineage.**

- **TFC-TDF U-Net** (KUIELab-MDX-Net, TFC-TDF-UNet v3): a U-Net over the complex spectrogram whose
  blocks combine **Time-Frequency Convolutions (TFC)** — stacked 2-D convolutions over the
  (time × frequency) image — with a **Time-Distributed Fully-connected (TDF)** bottleneck that
  reduces and restores the frequency axis with two linear layers, capturing long-range frequency
  structure that convolutions miss.
- **BSRNN / Band-Split RNN** (Luo & Yu): split the spectrum into sub-bands, run per-band MLPs and
  cross-band/temporal RNNs; the basis of BandIt and BandIt Plus for cinematic dialogue/music/effects.
- **Mel-Band RoFormer** (Wang et al.): band-split transformer with rotary position embeddings;
  8.98 dB vocals, SDX'23 winner — the strongest vocal/instrument teacher.
- **DeepFilterNet** (Schröter et al.): two-stage causal full-band (48 kHz) speech enhancement —
  ERB-gain envelope enhancement followed by **deep filtering** (a learned complex multi-frame filter
  of order N applied per frequency bin); 20 ms window, 10 ms hop, 40 ms latency, RTF ≈ 0.02 on a
  mobile CPU, ≈ 0.19 on an i5. Informs the speech-protection path.
- **RT-STT** (Wu et al., the primary backbone reference): a causal **single-path** TFC-TDF U-Net.
  **Availability (important):** as of this revision RT-STT has released **no code and no weights — the
  arXiv paper only** (Nanjing University, 17 Nov 2025), and the paper omits epochs, optimizer, and
  training duration. So "adopting RT-STT" here means **reimplementing the architecture from the text
  and training it ourselves** (the plan never depended on a downloadable checkpoint — see §6.7/§7.2,
  it trains its own student by distillation). The runnable cross-check is **HS-TasNet**, whose
  architecture _is_ published as code ([lucidrains/HS-TasNet](https://github.com/lucidrains/HS-TasNet),
  [temismink/HS-Tasnet](https://github.com/temismink/HS-Tasnet)); it is heavier
  (42 M vs 383 K) and ~0.5 dB lower, but it is a known-runnable fallback backbone if the RT-STT
  reimplementation stalls (§14). Empirical findings adopted here: (i) at small scale, time-only single-path RNN modelling beats
  dual-path time/frequency modelling (5.17 vs 5.16 dB) and is faster (5.8 vs 6.7 ms/1024-sample
  frame); (ii) **channel-expansion joint modelling** of sources lifts quality (5.17 vs 4.92 dB
  without); (iii) depth 1 with L = 3 single-path repeats is the sweet spot (depth 2 → 5.51 dB but
  12.11 ms; L = 4 → +0.01 dB only); (iv) **fp16 PTQ** cuts inference 82.6 % (5.80 → 1.01 ms) with no
  measurable quality loss, whereas int8 (1.06 ms) degraded quality.

**Elastic inference and distillation.** Slimmable networks (switchable widths with per-width
normalisation), once-for-all / sandwich-rule training, and matryoshka models (MatMamba, matryoshka
student models) let one supernet serve many cost points. Knowledge distillation — soft-target and
feature matching — is the established route to compact, high-quality students; cross-architecture KD
for speech enhancement (e.g. CMGAN → U-Net) and multi-view attention transfer report large
parameter/FLOP reductions at parity.

---

## 4. Proposed system — overview

The **Sukoon Separator** decomposes input audio into five stems and is evaluated at one of three
tiers selected at inference:

```
                          ┌─────────────────────────────┐
   input audio  ────────► │      Sukoon Separator       │ ──► speech         (always preserved)
   (stream or file)       │   single elastic checkpoint │ ──► vocals_sung
                          │                             │ ──► percussion
   tier:                  │   Fast     (causal, RT)     │ ──► instruments
     Fast / Balanced / HQ │   Balanced (short lookahead)│ ──► effects
                          │   HQ       (non-causal)     │
   mode (linear mix):     └─────────────────────────────┘
     output = G · [speech, vocals_sung, percussion, instruments, effects]ᵀ
```

The Fast tier targets live, in-page filtering; HQ targets files and batch. All tiers share
one weight set and one taxonomy, so a model improvement benefits every tier and shell at once.

---

## 5. Signal model and notation

Let the input be `x ∈ ℝ^{C₀ × L}` (C₀ channels, L samples). The analysis STFT uses window length
`N`, hop `H`, and a window `w` (periodic Hann), producing a complex spectrogram
`X ∈ ℂ^{C₀ × F × T}` with `F = N/2 + 1` bins and `T = ⌈L/H⌉` frames. Following RT-STT, real and
imaginary parts are stacked along the channel axis, `X̃ ∈ ℝ^{C × F × T}` with `C = 2·C₀`, and the
upper bins carrying little energy are trimmed to `F' ≤ F` (RT-STT: `F = 513 → F' = 384`).

**Tier-default front-end parameters.**

| Symbol | Meaning          | Fast (44.1 kHz) | Notes                                        |
| ------ | ---------------- | --------------- | -------------------------------------------- |
| `N`    | STFT window      | 1024 (~23 ms)   | algorithmic latency ≈ `N` for a causal model |
| `H`    | hop              | 512 (~11.6 ms)  | 50 % overlap                                 |
| `w`    | analysis window  | periodic Hann   | sqrt-Hann split between analysis/synthesis   |
| `F'`   | retained bins    | 384             | from `F = 513`                               |
| `T`    | frames per chunk | 64 (train)      | streaming uses `T = 1` per step              |
| `S`    | output stems     | 5               | RT-STT uses 4                                |
| `C₀`   | audio channels   | 2 (stereo)      | → `C = 4` input planes                       |

The model predicts `S` masks `M ∈ ℝ^{S × C × F' × T}` and forms stem spectrograms
`Ŝ_k = M_k ⊙ X̃`. The mask head applies a softmax over the stem axis `k` (§6.5), so
`Σ_k M_k = 𝟙` elementwise.

---

## 6. Architecture and objective

### 6.1 Network topology (causal single-path TFC-TDF U-Net)

A shallow U-Net (depth 1; deeper is too slow per RT-STT) with element-wise-multiplicative skip
connections:

```
            X̃ ∈ ℝ^{C×F'×T}
                 │ 1×1 conv  C→g                     (g = channel increment, default 16)
        ┌────────▼─────────┐
ENCODER │  Medium TFC-TDF  │ ──────────────┐  (skip: element-wise ⊙)
        │  1×1 conv  g→2g  │               │
        └────────┬─────────┘               │
        ┌────────▼─────────┐               │
LATENT  │  Medium TFC-TDF  │               │
        │  Single-Path × L │  (L = 3)      │
        │  1×1 conv → S·2g  │               │
        │  Softmax over S   │               │
        └────────┬─────────┘               │
        ┌────────▼─────────┐               │
DECODER │  ChanExp 1×1 conv │◄──────────────┘
        │  ChanExp Med TFC-TDF
        │  ChanExp 1×1 conv → S·(C/2)   (per-stem complex masks)
        └────────┬─────────┘
                 ▼
       M ∈ ℝ^{S×C×F'×T}  →  Ŝ_k = M_k ⊙ X̃
```

All time-axis operations are **causal**: convolutions are left-padded only (no future frames), the
recurrent core is unidirectional, and normalisation uses running/causal statistics (§9.3). The HQ
tier relaxes causality (bidirectional core, symmetric padding, optional look-ahead frames).

### 6.2 Medium TFC-TDF block

The block (a streamlined TFC-TDF v3) operates on a tensor `∈ ℝ^{c × T × F'}`:

```
in ─┬─► 3×3 conv ─► 3×3 conv ─┬─► [TDF: FC(F'→F'/16) ─► FC(F'/16→F')] ⊕ ─► 3×3 conv ─► 3×3 conv ─┬─► ⊕ ─► out
    │                          (residual over TDF)                                                  │
    └────────────────── 3×3 conv (residual projection) ─────────────────────────────────────────────┘
```

- **TFC** = the four 3×3 convolutions (local time-frequency structure), each with GN + activation.
- **TDF** = a residual bottleneck of two fully-connected layers across the frequency axis
  (`F' → F'/16 → F'`), modelling long-range frequency dependencies a 3×3 kernel cannot reach.
- A 3×3-conv residual projection is added at the block output.

"Medium" = fewer layers and a smaller TDF latent (`F'/16`) than TFC-TDF v3, chosen for latency.

### 6.3 Single-path temporal module

Repeated `L = 3` times in the latent. Each module is two RNN sub-blocks; each sub-block is:

```
u ─► GroupNorm ─► LSTM (unidirectional, causal) ─► FC (restore shape) ─► ⊕u
```

Unlike the dual-path module of DTTNet (which alternates reshaping between time and frequency axes),
the single-path module **only models the time axis**. RT-STT's analysis: at small window sizes the
frequency resolution is coarse (so per-frequency modelling helps little) while the time resolution is
fine (so temporal modelling matters more), and dropping the axis-reorder removes its latency. The
LSTM state is the object carried across hops during streaming (§9.3).

### 6.4 Decoder feature fusion (channel-expansion joint modelling)

Rather than convolving each source independently in the decoder, the `S` source feature maps are
concatenated along the channel axis and convolved jointly ("channel expansion"). This lets the
decoder model inter-source dependencies (e.g. that energy removed from `instruments` should appear in
`speech` or be suppressed, not vanish), and is faster on parallel hardware. RT-STT reports +0.25 dB
from this alone.

### 6.5 Multi-stem masking head and mixture consistency

The head produces `S` masks via a softmax over the stem axis:

```
M_k = softmax_k( Z_k ),     Σ_{k=1}^{S} M_k = 𝟙   (elementwise over C,F',T)
Ŝ_k = M_k ⊙ X̃
```

Because the masks form a partition of unity, the stems reconstruct the input **exactly**:

```
Σ_k Ŝ_k = (Σ_k M_k) ⊙ X̃ = X̃.            (mixture consistency)
```

Consequence for O1: no stem can fabricate or annihilate content — energy is only **routed** between
stems. The speech present in the mixture cannot be hallucinated away; at worst it is mis-routed,
which §6.6 penalises directly.

### 6.6 Listening modes as a stem-gain matrix

Modes are a fixed linear map `G ∈ ℝ^{O × S}` (default `O = 1` output) applied to the stem vector:

```
output = Σ_k G[:,k] · Ŝ_k
```

| Mode               | speech | vocals_sung | percussion | instruments | effects |
| ------------------ | :----: | :---------: | :--------: | :---------: | :-----: |
| `remove-all`       |   1    |      0      |     0      |      0      |    0    |
| `keep-vocals`      |   1    |      1      |     0      |      0      |    0    |
| `keep-percussion`  |   1    |      0      |     1      |      0      |    0    |
| `preserve-effects` |   1    |    (opt)    |   (opt)    |      0      |    1    |

The gains may be **continuous** in `[0, 1]`, so the four named modes are presets of a general per-stem
mixer (e.g. a UI slider that fades instruments). The nasheed case is handled structurally because
`vocals_sung` is a dedicated stem.

### 6.7 Training objective

For predicted stems `Ŝ_k` and targets `s_k` (ground-truth or teacher), the total loss is

```
L = Σ_k [ λ_sisdr·L_sisdr(ŝ_k, s_k) + λ_wav·‖ŝ_k − s_k‖₁ + λ_mr·L_mrstft(ŝ_k, s_k) ]
    + λ_kd·L_KD + λ_sd·L_SD + λ_mode·L_mode + λ_vp·L_vp
```

with the components:

- **Scale-invariant SDR** (time domain):
  ```
  L_sisdr = −10 log₁₀( ‖α s‖² / ‖α s − ŝ‖² ),   α = ⟨ŝ, s⟩ / ‖s‖².
  ```
- **Multi-resolution STFT loss** over FFT sizes `m ∈ {512, 1024, 2048}` (spectral convergence +
  log-magnitude):
  ```
  L_mrstft = Σ_m [ ‖|S_m| − |Ŝ_m|‖_F / ‖|S_m|‖_F  +  (1/Nₘ)‖log|S_m| − log|Ŝ_m|‖₁ ].
  ```
- **Knowledge distillation** from a teacher ensemble (soft targets + optional decoder-feature
  matching): `L_KD = Σ_k ‖ŝ_k − s_k^{teacher}‖₁ + γ·‖φ(ŝ_k) − φ(s_k^{teacher})‖²`.
- **Self-distillation** for elastic training: each sub-model matches the full model's output,
  `L_SD = Σ_k ‖ŝ_k^{sub} − stopgrad(ŝ_k^{full})‖₁` (§6.8).
- **Mode-recombination consistency:** the reconstructed `remove-all` output must equal the direct
  speech target, `L_mode = ‖ G_remove-all·Ŝ − s_speech ‖₁`.
- **Asymmetric voice-preservation penalty** (encodes "never clip the voice"): with `α ≫ 1` (default
  8), penalise speech **under-estimation** (clipping) far more than over-estimation, and penalise
  speech energy leaking into removed stems:
  ```
  L_vp = α·‖ relu(s_speech − ŝ_speech) ‖₁ + ‖ relu(ŝ_speech − s_speech) ‖₁
       + α·Σ_{k∈removed} ‖ Π_speech(ŝ_k) ‖₁,
  ```
  where `Π_speech` projects onto speech-dominant time-frequency cells (from the speech target mask).

Default weights (to be tuned): `λ_sisdr = 1, λ_wav = 1, λ_mr = 1, λ_kd = 1, λ_sd = 0.5,
λ_mode = 0.5, λ_vp = 1` with `α = 8`.

### 6.8 Multi-tier elastic training

The three tiers are nested sub-models of one supernet, elastic along three axes: **width**
(channel-increment `g`, sliced as the first `g_sub ≤ g` channels), **depth** (number of single-path
repeats `L_sub ≤ L`), and **causality/look-ahead** (Fast = 0 future frames; Balanced = a few;
HQ = bidirectional). Training uses the **sandwich rule** per step — always update the smallest and
largest configurations plus one random middle — with **in-place distillation** (the full model's
output is the soft target for the sub-models, `L_SD`). Normalisation statistics are
configuration-specific (switchable GroupNorm) to avoid width interference.

Tier presets:

| Tier     | `g_sub` | `L_sub` | Look-ahead | Causal | Use            |
| -------- | ------- | ------- | ---------- | ------ | -------------- |
| Fast     | 16      | 3       | 0          | yes    | live streaming |
| Balanced | 24      | 4       | 2 frames   | yes    | near-RT files  |
| HQ       | 32+     | 6+      | full       | no     | offline/files  |

If joint elastic training proves unstable, the fallback is a **distilled family** of three fixed-size
students sharing the taxonomy, pipeline, and teachers — identical product surface, more storage.

---

## 7. Data strategy

No human labelling is required; three sources are combined.

### 7.1 Existing labelled datasets

**Table 2 — datasets.**

| Dataset    | Content                               | Stems                         | Hours / size     | Licence      |
| ---------- | ------------------------------------- | ----------------------------- | ---------------- | ------------ |
| DnR v3     | cinematic mixtures, **30+ languages** | dialogue / music / effects    | large (~100 GB+) | per-source\* |
| MUSDB18-HQ | music multitrack, 150 songs, 44.1 kHz | vocals / drums / bass / other | ~10 h, ~30 GB    | CC BY-NC-SA  |
| MoisesDB   | music, 240 tracks, fine-grained stems | up to 11 categories           | ~tens of GB      | CC BY-NC-SA  |

\* DnR is assembled from LibriVox (speech), the Free Music Archive (music), and FSD50K (effects);
licences vary per underlying source and are recorded in provenance (§11).

DnR v3 supplies the **top-level speech/music/effects** split (and multilingual speech, matching the
user base); MUSDB18-HQ and MoisesDB supply the **fine music sub-stems** needed to separate
`percussion` from `instruments` and to derive `vocals_sung`.

### 7.2 Teacher pseudo-labelling

Pre-trained teachers generate stem targets on unlabelled audio:

| Target stem(s)         | Teacher(s)                   |
| ---------------------- | ---------------------------- |
| speech / denoise       | DeepFilterNet 3              |
| vocals / instruments   | Mel-Band RoFormer, Demucs v4 |
| dialogue/music/effects | BandIt Plus                  |

**Agreement filtering.** For a segment, run `K` teachers, align their estimates of a common stem, and
keep the segment only if the minimum pairwise SI-SDR between teacher estimates exceeds a threshold
`τ` (default 8 dB); otherwise discard. This yields higher-quality, less biased targets than any single
teacher. Corpora include multilingual speech and podcasts, music, nasheed audio, and — the priority —
**Qur'anic recitation**, for which no separation dataset exists, making the resulting pseudo-labelled
set the project's distinctive asset.

### 7.3 Synthetic remixing

Mix licence-clean isolated stems and keep the mix as input and the stems as exact labels:

```
x = Σ_k a_k · (h_k * s_k),     with per-stem gain a_k (random SNR), room IR h_k, codec round-trip.
```

- **Sources:** public-domain / CC0 / CC-BY speech (LibriVox), recitation where licensing permits,
  CC0/CC-BY music, isolated percussion loops (for `keep-percussion`), FSD50K-style effects.
- **Randomisation:** stem SNR ∈ [−5, +15] dB, room impulse responses (reverberation), and
  MP3/AAC/Opus encode→decode round-trips so training audio matches what users feed in.
- Provides volume, perfectly clean labels, the percussion/effects stems, and a fully licence-clean
  corpus (relevant if a permissive retrain is ever wanted, §11).

---

## 8. Training methodology

**Table 3 — default hyperparameters (Fast tier; adapted from RT-STT/DTTNet).**

| Item              | Value                                                                                                                                                                                                                                                              |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Sample rate       | 44.1 kHz (Fast/Balanced/HQ music); 48 kHz speech-protect path                                                                                                                                                                                                      |
| STFT              | `N=1024`, `H=512`, periodic Hann, bins trimmed to `F'=384`                                                                                                                                                                                                         |
| Chunk size        | 32 256 samples (`T=64` frames)                                                                                                                                                                                                                                     |
| Optimiser         | AdamW, lr `1e-4`, gradient clip L2-norm 3                                                                                                                                                                                                                          |
| Precision         | mixed precision (eases later fp16 PTQ)                                                                                                                                                                                                                             |
| Batch             | 8 per GPU × 4 GPUs = **effective 32** (RT-STT's setup). On free tiers (Kaggle T4×2, 16 GB) the per-step batch is far smaller — use **gradient accumulation** to keep the effective batch near 32, since small effective batches are known to hurt MSS convergence. |
| Channel increment | `g=16`; single-path repeats `L=3`; depth 1                                                                                                                                                                                                                         |
| Sources           | `S=5`                                                                                                                                                                                                                                                              |
| Augmentation      | pitch shift {−2,−1,0,1,2} st; time-stretch {±20,±10,0}%; random SNR, room IR, codec round-trips                                                                                                                                                                    |
| Fine-tune         | add L1 regularisation (per RT-STT)                                                                                                                                                                                                                                 |

**Curriculum.**

1. **Pre-train** on synthetic + pseudo-labelled data (broad coverage, perfect/clean labels).
2. **Fine-tune** on DnR v3 (realistic cinematic dialogue/music/effects, multilingual).
3. **Specialise** on the recitation/nasheed corpus (the release-deciding case), with `λ_vp` raised.

**Elastic schedule.** Apply the sandwich rule + in-place distillation (§6.8) from the start, so the
nested tiers co-train rather than being distilled afterward.

---

## 9. Optimisation and real-time inference

### 9.1 Quantisation

Per RT-STT, **fp16 post-training quantisation** is the operating point: it removed 82.6 % of
inference time (5.80 → 1.01 ms per 1024-sample frame on an RTX 3080 Ti) with no measurable SDR loss,
whereas int8 (QAT+PTQ) reached only 1.06 ms and degraded quality. We therefore target fp16 for
deployment; mixed-precision training makes PTQ smooth. Structured pruning and channel slimming come
free from the elastic Fast configuration.

### 9.2 Export and runtimes

The graph (2-D convs + LSTM + linear) exports to **ONNX** and runs via:

- **`tract` + WASM/SIMD** for the **browser extension** — the pure-Rust path already proven by
  `@sukoon/dfn-wasm` (DFN runs in `tract`-WASM in the page today);
- **`ort`** (ONNX Runtime) for **native CLI/desktop**, which is what the DFN engine already uses
  there (the desktop toolchain hits a `tract` optimizer bug on this graph, so ORT is the native path);
- **Core ML / NNAPI** on mobile (note: TensorRT, used for RT-STT's headline number, is CUDA-only;
  on Apple silicon and the browser the equivalent path is Core ML / ONNX-Runtime fp16).

### 9.3 Streaming runtime

Frame-synchronous, stateful, causal processing:

```
state.lstm ← 0;  ring ← zeros(N);  ola ← zeros(N)          # init
for each input hop x_t (H samples):
    ring ← shift_in(ring, x_t)                              # sliding analysis window
    X_t  ← RFFT(w_analysis ⊙ ring)            ∈ ℂ^{C₀×F}
    X̃_t ← trim_to_F'(stack(Re X_t, Im X_t))  ∈ ℝ^{C×F'}
    M_t, state.lstm ← model.step(X̃_t, state.lstm)          # one causal frame, T=1
    Ŝ_t ← M_t ⊙ X̃_t
    o_t ← G · Ŝ_t                                           # mode mix → ℂ^{C₀×F}
    y   ← w_synth ⊙ IRFFT(o_t)
    ola ← overlap_add(ola, y, hop=H)
    emit oldest H samples of ola                            # delayed by look-ahead
```

- **Algorithmic latency** = `N + d·H`, where `d` = look-ahead frames (Fast `d=0` → `N` = 1024 samples
  ≈ 23 ms at 44.1 kHz). Balanced adds `d=2` (~46 ms).
- **Real-time budget:** one step must complete within `H/sr` (= 11.6 ms at `H=512`/44.1 kHz). RT-STT's
  ~1 ms/frame gives RTF ≈ 0.09 — large CPU/WASM margin.
- **Perfect reconstruction:** sqrt-Hann split between analysis and synthesis with 50 % overlap
  satisfies COLA; no normalisation drift.
- **Statefulness:** the LSTM hidden/cell state and the OLA buffer are the only carried state; normalise
  with causal running statistics (after DeepFilterNet's causal instance norm) so streaming matches
  offline.
- **Ad handling:** the model runs uniformly over all audio, including ads (per
  [`design-considerations.md`](../design-considerations.md) §1); ads play in full with music removed.

---

## 10. Evaluation plan

- **Metrics.**
  - **cSDR** (chunk-level SDR; median over 1-s chunks) and **uSDR** (utterance-level, per-song mean) —
    the SiSEC/MDX conventions, for comparability with Table 1.
  - **SI-SDR** improvement per stem.
  - **RTF** = (processing time)/(audio duration) and **algorithmic latency**, measured on CPU, WASM,
    and mobile.
  - **Voice-preservation metrics** on recitation: speech-clipping rate, speech-to-residual ratio.
  - **Mode correctness:** ‖mode-mix − direct target‖ for each preset.
- **Datasets.** MUSDB18-HQ (vocals/drums/bass/other) and DnR test (dialogue/music/effects) for
  research comparability; the in-house recitation/nasheed set for the release gate.
- **Tier targets (Phase-0 gate).**

  | Tier     | Latency  | RTF (mid CPU) | SDR target                       |
  | -------- | -------- | ------------- | -------------------------------- |
  | Fast     | ≤ ~25 ms | < 0.5         | ≥ 5.2 dB (stretch ≥ 6 dB via KD) |
  | Balanced | ≤ ~50 ms | < 1.0         | ≥ 6.5 dB                         |
  | HQ       | offline  | n/a           | ≥ 7.5 dB                         |

- Hooks into the repo's [`model-eval`](../contributing/model-eval.md) gate; voice-preservation is a
  hard gate that can block a release regardless of SDR.

---

## 11. Licensing and ethics

- **Release decision.** Trained weights are released **open-source / non-commercial**
  (CC-BY-NC-SA-compatible). Because MUSDB18, MoisesDB, and share-alike teacher weights (e.g. BandIt v2)
  carry non-commercial / share-alike terms, and Sukoon releases under compatible terms, **all** of
  these datasets and teachers are usable. Discipline retained: release under a licence compatible with
  the most restrictive input.
- **Provenance.** Each checkpoint records the datasets and teachers it touched, surfaced via the model
  registry (extending `License::bundle_safe` in [`registry.rs`](../../packages/core/src/registry.rs)),
  preserving the option of a later clean-room, permissively-licensed retrain (synthetic-only data,
  permissive teachers) if commercial distribution is ever reconsidered.
- **Religious framing.** Per [`design-considerations.md`](../design-considerations.md) §5, the project
  presents documented scholarly positions and **issues no rulings**; the default mode is the broadest
  common denominator, not an endorsement.

---

## 12. Compute and budget

Designed for a **near-zero monetary budget**. The unconstrained programme would need thousands of
GPU-hours; instead the plan uses free compute and wraps a pre-trained model for the HQ tier rather
than training it.

### 12.1 Free resources

| Resource            | Free allowance                       | Used for                                    |
| ------------------- | ------------------------------------ | ------------------------------------------- |
| Kaggle Notebooks    | ~30 GPU-h/week (T4×2 or P100, 16 GB) | Fast/small-model training — the main engine |
| Google Colab (free) | T4, session-limited                  | short runs, debugging                       |
| Local Apple silicon | unlimited (electricity)              | data pipeline; overnight small-model runs   |

### 12.2 Per-phase GPU-hour estimates and feasibility

Anchored to RT-STT (4× RTX 3090, days, for a 383 K model). Ranges are ±2× and include iteration.

| Work item                          | Est. GPU-hours             | Free-compute feasibility                          |
| ---------------------------------- | -------------------------- | ------------------------------------------------- |
| Data pipeline + synthetic remixing | 0 (CPU)                    | Yes — local                                       |
| Targeted teacher pseudo-labelling  | 150–600 (∝ hours labelled) | Yes, slowly — prioritise recitation               |
| Fast tier (≈ RT-STT) training      | 300–500                    | Yes — Kaggle over several weeks                   |
| Recitation fine-tune               | 50–150                     | Yes                                               |
| Five-stem + DnR (Phase C)          | 500–1 500                  | Partly — prototype free, full scale needs funding |
| Distillation (Phase D)             | 500–1 000                  | Partly                                            |
| Elastic supernet (Phase E)         | 1 000–3 000                | Needs funding — prototype small for free first    |
| HQ tier from scratch (20–40 M)     | 2 000–5 000                | Needs funding — until then, wrap a pre-trained HQ |

**Storage:** MUSDB18-HQ ~30 GB, DnR ~100 GB+, pseudo-labelled stems scale with corpus hours
(terabytes at full scale) — an external SSD or sponsored cloud storage is required beyond the smallest
experiments.

### 12.3 Interim strategy

Ship the product now on pre-trained models (already scaffolded), train the owned Fast model
incrementally on free compute, and provide the HQ tier via an existing separator until funding allows
training a bespoke one.

**One cheap exception worth flagging.** The Fast tier (Phase B, ~300–500 GPU-h) is the keystone — it
yields the first owned engine — and it is the one phase that does **not** need months of free-tier
patience. The same run is **~$100–250 on a marketplace 24 GB GPU** (RTX 3090/4090 at ~$0.2–0.4/h on
Vast.ai/RunPod), finishing in **2–5 days**, versus ~10–17 weeks at Kaggle's ~30 GPU-h/week on weaker
T4×2 cards that can't hold the effective batch. For the project's highest-leverage milestone, that is
the cheapest meaningful spend available — see the **Small** row in §15.

---

## 13. Project plan

| Phase | Goal                                                                                                                                | Exit criteria                                                                                                                                                                                                                   |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A** | Data pipeline + synthetic-remix corpus; targeted teacher pseudo-labelling; provenance.                                              | Reproducible labelled data; provenance recorded.                                                                                                                                                                                |
| **B** | **Reimplement** RT-STT from the paper (no code/weights exist — §3), 4-stem MUSDB, fp16 ONNX export, wire behind the `Engine` trait. | Real-time on CPU; **SDR within ±0.5–1.0 dB of paper** (widened: no reference impl to diff against, paper omits epochs/optimizer/duration); live in CLI/extension. Fall back to HS-TasNet (published code) if it won't converge. |
| **C** | Five-stem taxonomy + DnR; mode-gain matrix; asymmetric voice loss.                                                                  | Modes pass correctness tests; recitation voice-preservation meets gate.                                                                                                                                                         |
| **D** | Teacher-ensemble distillation into the small tiers.                                                                                 | Fast-tier SDR exceeds vanilla RT-STT (stretch ≥ 6 dB).                                                                                                                                                                          |
| **E** | Elastic supernet → Fast/Balanced/HQ from one checkpoint (fallback: distilled family).                                               | Each tier meets its §10 target.                                                                                                                                                                                                 |
| **F** | Optimise (fp16/prune/ONNX/WASM/Core ML/NNAPI); streaming runtime; mobile.                                                           | Latency/RTF targets met on all platforms.                                                                                                                                                                                       |
| **G** | Phase-0 evaluation gate; scholarly review of modes; provenance/licence review.                                                      | Repo gates green; advisor sign-off.                                                                                                                                                                                             |

Phase B is the keystone: it validates the streaming runtime end-to-end and yields an owned Fast engine
before any expenditure.

---

## 14. Risks and mitigations

| Risk                                                                                                     | Mitigation                                                                                                                                                                                                                                 |
| -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **RT-STT reimplementation doesn't reproduce 5.17 dB** (no public code/weights; missing training details) | Treat Phase B as research with widened ±0.5–1.0 dB tolerance and extra iteration budget; cross-check against **HS-TasNet's published architecture code**; remember the student is trained by distillation, not by matching RT-STT weights. |
| **Backbone reimplementation stalls entirely**                                                            | Switch backbone to **HS-TasNet** ([lucidrains](https://github.com/lucidrains/HS-TasNet)) — runnable code today; heavier (42 M) and ~0.5 dB lower, but unblocks the streaming-separator path.                                               |
| Free compute too slow for larger models                                                                  | Keep custom training to small tiers; pre-trained models for HQ. The keystone Fast tier escapes free-tier limits for **~$100–250** on a rented GPU (§12.3).                                                                                 |
| Recitation data scarce / licence-restricted                                                              | Combine pseudo-labelling + synthetic remixing; per-item provenance.                                                                                                                                                                        |
| Elastic training unstable                                                                                | Fall back to a distilled family of fixed-size students.                                                                                                                                                                                    |
| Voice clipped on nasheed content                                                                         | Dedicated stem + softmax consistency + asymmetric penalty + recitation gate.                                                                                                                                                               |
| fp16/quantisation regression off-CUDA                                                                    | Validate Core ML / ONNX-Runtime fp16 parity; keep fp32 fallback.                                                                                                                                                                           |
| Owned model below the existing wrap initially                                                            | Treat as additive; ship pre-trained models until it surpasses them.                                                                                                                                                                        |

---

## 15. Funding and sponsorship

> **This section is the difference between "what can be done slowly, for free" and "what can be done
> well." Sukoon is, and will remain, free and open-source software. It is offered for the benefit of
> the Muslim community — and of anyone who wishes to listen to lectures, audiobooks, and other speech
> with background music removed and the voice, including recitation, preserved — as an act of ongoing
> charity (ṣadaqah jāriyah), seeking only the pleasure of Allah.**

The binding constraint on this work is money for **compute, data storage, and bandwidth** (§12).
Donations would be used **strictly** for the project's stated purpose and nothing else:

- **GPU compute** to train the Fast/Balanced models without free-tier limits and, in time, a bespoke
  HQ tier (Phases C–E), rather than relying entirely on capped free resources;
- **large-scale teacher pseudo-labelling** and **storage** for the resulting stem corpora (terabytes
  at full scale, §12.2);
- expansion and careful curation of the **recitation/nasheed** dataset — the project's most valuable
  and most labour-intensive asset.

Funding maps to outcomes as follows:

| Approx. funding   | Enables                                                                                                                                    |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Small (~$100–250) | The keystone Fast tier (Phase B) trained on a rented 24 GB GPU in days instead of ~10–17 free-tier weeks; plus a larger recitation corpus. |
| Moderate          | Teacher labelling at scale; the five-stem and distillation phases (C–D).                                                                   |
| Larger            | A bespoke HQ tier and the elastic supernet (E), trained from scratch and openly released.                                                  |

Commitments:

- **Code and trained weights remain free and open** under the project's licence — funding buys
  _faster and better_, never _exclusive_.
- **Transparency:** compute spending and what it produced will be reported publicly.
- **No rulings:** funding does not change the project's posture
  ([`design-considerations.md`](../design-considerations.md) §5).

> _If you wish to support this work, please see the project's funding links in the repository._
> _A specific donation channel will be added here once established._

---

## 16. Conclusion and immediate next steps

The proposal converts an ambitious aim — a fast, high-quality, mode-aware, recitation-safe model —
into a realistic, frontier-aware programme executable on free compute, with funding as an accelerant
rather than a prerequisite. Recommended first steps:

1. Build the synthetic-remix data pipeline (Phase A) — local, CPU-only, reusable.
2. Assemble the recitation/nasheed evaluation set early — the release-deciding gate.
3. Reproduce RT-STT (Phase B) on free compute as the first owned engine.
4. Stand up small-scale teacher pseudo-labelling to validate ensemble agreement before scaling.
5. Continue shipping the product on pre-trained models in the interim.

---

## References

- Wu, Liu, Pan, Tang, Wu. _Towards Practical Real-Time Low-Latency Music Source Separation_ (RT-STT).
  arXiv:2511.13146, 2025.
- Venkatesh, Benilov, Coleman, Roskam. _Real-time Low-latency Music Source Separation using Hybrid
  Spectrogram-TasNet_ (HS-TasNet). ICASSP 2024.
- Chen, Vekkot, Shukla. _Music Source Separation Based on a Lightweight Deep Learning Framework
  (DTTNet)_. ICASSP 2024.
- Kim, Lee. _Sound Demixing Challenge 2023 — TFC-TDF-UNet v3_. arXiv:2306.09382, 2023; Kim et al.
  _KUIELab-MDX-Net_. arXiv:2111.12203, 2021.
- Luo, Yu. _Music Source Separation with Band-Split RNN_. IEEE/ACM TASLP 2023.
- Wang, Lu, Kong, Hung. _Mel-Band RoFormer for Music Source Separation_. arXiv:2310.01809, 2023.
- Schröter et al. _DeepFilterNet (1/2/3)_. arXiv:2110.05588 / 2205.05474 / 2305.08227.
- Défossez et al. _Hybrid Transformers for Music Source Separation (Demucs v4)_. ICASSP 2023.
- Kwatcharasupat et al. _Remastering Divide and Remaster (DnR v3)_. arXiv:2407.07275, 2024.
- _MUSDB18 / MUSDB18-HQ_ (Zenodo, 2019); Pereira et al. _MoisesDB_. arXiv:2307.15913, 2023 — both
  CC BY-NC-SA, non-commercial.
- Yu et al. _Slimmable Neural Networks_ (ICLR 2019); Cai et al. _Once-for-All_ (ICLR 2020);
  _MatMamba_ (arXiv:2410.06718); _Matryoshka Model Learning for Improved Elastic Student Models_
  (arXiv:2505.23337).
