# Changelog

All notable changes to Sukoon are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org). Pre-1.0, minor versions may break.

## [Unreleased]

### Added

- Initial monorepo scaffold: `sukoon-core` (Rust engine), `sukoon-cli`, `@sukoon/extension`
  (MV3), `@sukoon/desktop` (Tauri), `@sukoon/web`, and `sukoon-cloud` (Modal worker).
- **Working HQ engine: MDX-Net (Kim Vocal 2) via ONNX Runtime** — real voice/instrumental
  separation with a hand-written STFT/ISTFT front-end (`dsp` module). Default for `clean`;
  ~4× faster than real-time on CPU.
- **Real low-RAM fallback engine: MDX-Net UVR 9482 (`mdx-lite`).** Replaces the former fake
  `demucs`/`mdx_q` placeholder (which had a nonexistent URL, a zero checksum, and fictional
  size/RAM/MIT metadata) with a real, checksum-verified ~29.7 MB community UVR model. It reuses the
  `MdxNet` engine path — only `dim_f` differs (2048 vs the HQ model's 3072). Quality is lower than
  HQ (older, lower-SDR model), as expected for a fallback. `CommunityDownloadOnly` (download-only,
  never bundled). CLI alias `fallback` still works; `mdx_q` is kept as an alias, `demucs` removed.
  The old `demucs.rs` engine stub was deleted.
- **Real Fast engine: DeepFilterNet 3 via ONNX Runtime** (behind the new `dfn` cargo feature, which
  implies `onnx`) — the only real-time engine (~180× real-time on CPU), a speech _enhancer_ that
  keeps speech and suppresses music + noise. Reuses only `deep_filter`'s pure-Rust DSP (`DFState`
  STFT/ERB/band-norm front-end, the exact upstream recipe) and runs the three DFN3 ONNX graphs
  (`enc`, `erb_dec`, `df_dec`) through ORT, because the bundled `tract` runtime can't load the model
  on current toolchains (a tract optimizer bug, `duplicate name /convt3/Conv.bias`). Weights are an
  Apache-2.0 (bundle-safe) 3-file ONNX bundle (~8 MB), downloaded + extracted from a commit-pinned
  tarball on first use. **CPU-preferred** — measured CoreML is slower for this small recurrent model,
  so DFN runs on CPU while MDX uses the GPU. Functionally validated (clean speech preserved within
  ~0.2 dB, pure music suppressed ~26 dB) but not yet bit-exact-verified against the upstream
  reference, and LSNR stage-gating is simplified (both stages applied); these are noted refinements.
  Without `dfn` the Fast engine remains a passthrough stub.
- **In-library resampling.** `AudioBuffer::resample(target_rate)` (rubato FFT resampler), wired into
  the pipeline as a sample-rate guard — a no-op when rates already match (the CLI case), correcting
  only library callers that hand the engine a buffer at the wrong rate.
- **Progress reporting.** `Pipeline::on_progress(cb)` and a `Progress` enum
  (`Extract` / `Separate { chunk, total }` / `Encode` / `Remux` / `Done`); the CLI renders a live
  progress line. MDX reports per-chunk progress via `Engine::separate_with_progress`.
- **Bounded-memory streaming.** For a long file processed by a streaming-capable engine (MDX) at the
  engine's sample rate, the pipeline now reads, separates, and writes the cleaned stem
  block-by-block (`WavBlockReader` / `WavBlockWriter`, `Engine::chunk_plan` + `separate_block`), so
  peak memory stays bounded by one ~32 MB block regardless of file length instead of holding the
  whole file (which for an hour of stereo audio was multiple GB). The streamed output is **byte-exact**
  to the whole-buffer path (each block is fed the surrounding STFT context); verified by an
  equivalence test. Short files and rate-mismatched inputs fall back to the whole-buffer path.
- **Concurrent audio re-encode.** On a video input the pipeline streams the cleaned speech into a
  piped FFmpeg AAC encoder as the engine produces it (`Engine::separate_into` + a threaded
  `PcmEncoder`), so the single-threaded encode runs _concurrently_ with separation instead of after
  it. A full 24-minute clip on the Fast engine drops from ~27 s to ~19 s end-to-end (~74× real-time),
  bit-identical to the sequential path (a streamed-vs-whole-buffer equivalence test guards this).
  Video is copied losslessly (`-c:v copy`); FFmpeg is also quieted for end users.
- **Chunked DeepFilterNet inference.** The Fast engine runs its three ONNX graphs over fixed ~40 s
  windows (with warmup context) instead of one whole-utterance batch, keeping intermediate tensors
  small so it holds ~180× real-time on long clips (a 24-min file no longer balloons to multi-GB
  tensors). Bit-identical to the un-chunked path.
- **Visible model downloads.** Weights still download automatically on first use, but now through a
  global progress observer (`registry::set_download_observer`) that the CLI renders as a live line
  (downloaded/total MB, %), so a first-run fetch no longer looks like a hang.
- **End-to-end ONNX test** (`tests/e2e_onnx.rs`, `#[ignore]`) driving `Pipeline::clean_file` through
  the whole real flow (FFmpeg extract → MDX inference → encode → write) and asserting progress events.
- **Automatic hardware acceleration.** A shared, tuned ONNX Runtime session (full graph
  optimization, machine-sized thread pool) auto-selects the best execution provider per platform —
  CoreML on macOS (~12–15× real-time on an M4, validated bit-close to CPU: <-59 dB residual),
  DirectML on Windows, and CUDA on Linux behind the opt-in `cuda` cargo feature. Registration is
  non-fatal (CPU fallback), and there are **no performance knobs for the user**; `SUKOON_CPU_ONLY=1`
  is the lone support escape hatch.
- Real end-to-end pipeline: FFmpeg extract (at engine rate) → WAV decode → `Engine::separate`
  → WAV encode → FFmpeg remux, with content-hash caching. WAV I/O on `AudioBuffer` (`audio`).
- Model registry with on-first-use download + SHA-256 verification (`Model::ensure_local`), and
  a `CommunityDownloadOnly` license class for runtime-only (never-bundled) weights.
- `Engine` trait with `MdxNet` (HQ + the `mdx-lite` low-RAM fallback) and `DeepFilterNet` (Fast),
  all behind the same interface.
- Browser extension skeleton: ad-aware `MutationObserver` bypass and Web Audio routing.
- Full documentation tree under `docs/`, `ARCHITECTURE.md`, and `LICENSING.md`.

### Changed

- **Engine lineup pivot.** Dropped BandIt Plus (planned DnR 3-stem BSRNN HQ engine) in favour of
  MDX-Net via ONNX Runtime. The real-time DeepFilterNet engine is now **wired up** via ONNX Runtime
  (see Added) rather than its pure-Rust `tract` runtime, which cannot load the model on current
  toolchains (a tract optimizer bug that reproduces even with DeepFilterNet's own pinned tract
  versions). See
  [design-considerations §6](./docs/design-considerations.md#6-quality-separation-now-a-real-time-path-later).
- Separation focus simplified to **remove music / keep voice**. `remove-all` and `keep-vocals`
  are the meaningful modes (identical for the 2-stem engine); `keep-percussion` and
  `preserve-effects` remain placeholders pending a multi-stem engine.

---

_No released versions yet. The first tag will be `v0.1.0-alpha`._
