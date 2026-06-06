# Engines

The separation backends and how to add one. Source:
[`packages/core/src/engine`](../../packages/core/src/engine).

## The trait

```rust
pub trait Engine: Send + Sync {
    fn id(&self) -> &'static str;
    fn target_sample_rate(&self) -> u32;   // pipeline resamples to this
    fn realtime_capable(&self) -> bool;    // gates live/extension use
    fn separate(&self, input: &AudioBuffer) -> Result<Separation>;
}
```

A stateful engine (like MDX-Net) may carry state internally across the chunks it processes; the
pipeline hands it a whole decoded buffer and the engine chunks as it needs to.

## The engines

| Engine          | Kind       | Approach                  | SR      | Realtime | Status                                                   |
| --------------- | ---------- | ------------------------- | ------- | -------- | -------------------------------------------------------- |
| `MdxNet`        | `Hq`       | 2-stem voice/instrumental | 44.1kHz | ❌       | ✅ **Working.** Default for `clean`; files/batch.        |
| `DeepFilterNet` | `Fast`     | Speech enhancement (ORT)  | 48 kHz  | ✅       | ✅ **Working** behind `--features dfn`; the live engine. |
| `MdxNet` (lite) | `Fallback` | 2-stem voice/instrumental | 44.1kHz | ❌       | ✅ **Working.** Low-RAM UVR 9482; reuses the MDX path.   |

- **MDX-Net (Kim Vocal 2)** is the real HQ separation engine. It runs via **ONNX Runtime**: the
  STFT/ISTFT front-end ([`dsp.rs`](../../packages/core/src/dsp.rs)) packs a complex spectrogram
  `[1, 4, dim_f, dim_t]`, the model predicts the vocal stem, and the music stem is `input − vocals`.
  Weights (~67 MB) download automatically on first use (with a live progress indicator) and are
  never bundled. ~4× faster than real-time on CPU, and **~12–15× with hardware acceleration** (see
  below) — e.g. ~6 s for a 90 s clip on an Apple M4 via CoreML. Full numbers and a device matrix
  live in [docs/reference/performance.md](../reference/performance.md).
- **DeepFilterNet** is the tiny real-time **Fast** engine — a speech _enhancer_ (keeps speech,
  suppresses music + noise), the **only real-time engine** (~180× real-time on CPU). It is wired up
  behind the **`dfn` cargo feature** (which implies `onnx`); without that feature the Fast engine is
  a passthrough stub. **Why it's wired the way it is:** DeepFilterNet ships a pure-Rust `tract`
  runtime, but that runtime can't load the DFN3 model on current toolchains (a tract optimizer bug,
  `duplicate name /convt3/Conv.bias`, that reproduces across tract 0.21.x). So Sukoon reuses only
  `deep_filter`'s **DSP** — the [`DFState`](../../packages/core/src/engine/deepfilternet.rs) STFT /
  ERB / band-norm front-end, the exact upstream recipe — and runs the three DFN3 ONNX graphs (`enc`,
  `erb_dec`, `df_dec`) through ONNX Runtime. Weights are a 3-file ONNX bundle (~8 MB) shipped in a
  gzip tar, **Apache-2.0** (bundle-safe), downloaded automatically (with a live progress indicator)
  and extracted on first use from a commit-pinned URL.

  **Honesty note.** The implementation is functionally validated — it runs, clean speech is
  preserved within ~0.2 dB, and pure music is suppressed ~26 dB — but it is **not bit-exact-verified**
  against the upstream reference, and LSNR stage-gating is simplified (both stages are applied
  unconditionally). These are noted as future refinements.

  **Chrome-extension note.** In the extension, DeepFilterNet is the **only** engine — real-time, run
  frame-by-frame in the page's audio thread (the `dfn-processor` AudioWorklet over the
  `@sukoon/dfn-wasm` build: here `tract`→WASM with the model embedded, which works in the browser even
  though it doesn't on the desktop toolchain). Its attenuation is **capped (gentle)** so it doesn't
  thin melodic recitation. Earlier in-browser MDX separators were built and **removed** as too
  fragile on live streams — see [extension trials](../research/extension-trials.md). True separation
  runs in the file tools
  (CLI/desktop, all on-device); a real-time, best-quality separator awaits
  [Sukoon's own model](../research/own-model-plan.md).

- **MDX-Net UVR 9482 (Fallback, low-RAM)** is the real low-RAM fallback (`id = "mdx-lite"`). It is
  the same `MdxNet` engine and ONNX contract as the HQ model — only `dim_f` differs (2048 vs the HQ
  model's 3072) — so it reuses the whole STFT/demix path. Weights are ~29.7 MB (vs ~67 MB),
  download-only (community UVR weights, never bundled). Quality is lower than HQ (an older, lower-SDR
  model), which is the expected trade-off for a low-RAM fallback.

### Acceleration

Profiling the MDX demix shows the model inference is **~98%** of the wall time (the STFT/ISTFT
front-end is <2%), so the only lever that matters is the inference itself. The shared session
builder ([`engine::build_session`](../../packages/core/src/engine/mod.rs)) configures this
**automatically — there are no performance knobs for the user**:

- Full graph optimization (operator fusion/constant folding; ORT doesn't enable this by default).
- Intra-op threads sized to the machine.
- The best hardware accelerator for the platform, registered **non-fatally** (any unsupported op or
  missing driver silently falls back to CPU, so it can only ever make things faster). This applies
  to the **MDX** engines; the **DeepFilterNet Fast engine is CPU-preferred** — measured CoreML is
  _slower_ for DFN (~132× real-time on CoreML vs ~180× on CPU: it's a small recurrent model, so GPU
  launch overhead and limited RNN-op support outweigh any gain), and a real-time engine is better off
  leaving the GPU free for the HQ path, so DFN builds CPU sessions while MDX uses the accelerator:

  | Platform | Provider                   | Default                         | Notes                                                                                                                                    |
  | -------- | -------------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
  | macOS    | CoreML (GPU/Neural Engine) | ✅ on                           | Validated ~3× faster than CPU with a <-59 dB residual vs the CPU output. Compiled model cached on disk so one-shot runs don't recompile. |
  | Windows  | DirectML (any DX12 GPU)    | ✅ on                           | NVIDIA/AMD/Intel; DX12 is ubiquitous on Windows 10+.                                                                                     |
  | Linux    | CUDA (NVIDIA)              | ⏳ build-time `--features cuda` | Default Linux build stays pure-CPU/portable; distributors targeting NVIDIA opt in.                                                       |

  The lone escape hatch, for support when a GPU driver misbehaves, is `SUKOON_CPU_ONLY=1`.

> **Modes.** The product goal is "remove music, keep the voice," so `remove-all` and `keep-vocals`
> are the meaningful modes — and for the 2-stem MDX engine they resolve to the same thing (keep the
> vocal stem). `keep-percussion` and `preserve-effects` would need a multi-stem engine and are not
> implemented; they remain placeholders, not shipping features.

## Model registry

[`registry.rs`](../../packages/core/src/registry.rs) is the single audited source of truth:
URL, SHA-256, size, MDX front-end params, and **license** per model. `Model::ensure_local()`
downloads on first use and verifies the SHA-256 before the file is moved into place.

```rust
pub enum License { Mit, Apache2, PermissiveVerifyDataset, CcBySa4, CommunityDownloadOnly }
impl License {
    pub fn bundle_safe(self) -> bool {
        !matches!(self, License::CcBySa4 | License::CommunityDownloadOnly)
    }
}
```

`bundle_safe()` is how a build refuses to embed a weight that shouldn't be redistributed inside a
binary — a share-alike weight (CC-BY-SA-4.0) or a community model of unverified provenance
(`CommunityDownloadOnly`, e.g. the MDX weights). Those are fetched at runtime, never bundled. This
is a shipping constraint, not a nicety — see
[design-considerations §7](../design-considerations.md#7-model-weight-licensing-is-a-shipping-constraint-not-an-afterthought)
and [LICENSING.md](../../LICENSING.md).

## Adding an engine

1. Implement `Engine` in `src/engine/your_engine.rs`.
2. Add a variant to `EngineKind` (or register dynamically) and wire `build()`.
3. Add a `Model` entry to the registry with its real checksum and license.
4. Note it in `CHANGELOG.md` (and `LICENSING.md` if the license is new).

Nothing else changes — shells pick it up by id.

## Inference status

The **MDX-Net HQ engine** and the **low-RAM Fallback** (`mdx-lite`, UVR 9482) are real behind the
`onnx` feature (STFT → ONNX model → ISTFT, with an on-disk content cache). The **Fast
(DeepFilterNet)** engine is real behind the **`dfn` feature** (which implies `onnx`): `deep_filter`
DSP + the three DFN3 ONNX graphs run via ORT. Without `onnx` (or, for the Fast engine, without
`dfn`), the relevant engine runs in dry/passthrough mode so shells and tests build without weights.
See [ROADMAP](../../ROADMAP.md#engine-roadmap).
