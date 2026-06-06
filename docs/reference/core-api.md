# Core API reference

The public surface of `sukoon-core`. Full docs: `cargo doc -p sukoon-core --open`.

## `Pipeline`

```rust
let pipeline = Pipeline::new(PipelineOptions {
    engine: EngineKind::Hq,           // MDX-Net — the working engine
    mode: SeparationMode::RemoveAll,
    use_cache: true,
})?;

pipeline.clean_file("in.mp4", "out.mp4")?;   // extract → separate → remux
pipeline.engine_id();                         // "mdx"
pipeline.realtime_capable();                  // false for Hq (MDX)
```

### Progress reporting

Attach a callback (builder-style) to observe pipeline stages live:

```rust
let pipeline = Pipeline::new(opts)?.on_progress(|p| match p {
    Progress::Extract                 => { /* extracting audio (FFmpeg) */ }
    Progress::Separate { chunk, total } => { /* chunk of total engine chunks done */ }
    Progress::Encode                  => { /* encoding speech WAV */ }
    Progress::Remux                   => { /* remuxing onto the container (FFmpeg) */ }
    Progress::Done                    => { /* finished */ }
});
```

`Progress::Separate { chunk, total }` is emitted per MDX inference chunk; engines that don't chunk
(DeepFilterNet) report fewer events. The CLI uses this to render a live status line. Engines stream
cleaned audio out incrementally as it's produced, so the pipeline runs the audio re-encode
**concurrently** with separation (the encode overlaps inference rather than adding to it); the video
stream is copied, never re-encoded.

## `EngineKind`

```rust
EngineKind::Fast | EngineKind::Hq | EngineKind::Fallback
EngineKind::from_id("hq")   // -> Some(EngineKind::Hq); also accepts "mdx", "mdx-net", "dfn", …
kind.id()                    // stable string id: "deepfilternet" | "mdx" | "mdx-lite"
kind.build()?                // -> Box<dyn Engine>
```

`Hq` (MDX-Net Kim Vocal 2) is the default engine; `Fallback` (`mdx-lite`, UVR 9482) is a real
low-RAM MDX model that reuses the same path; `Fast` (DeepFilterNet) is the real-time engine, live
behind the `dfn` feature (a passthrough stub without it). The `Fallback` id also accepts the
`fallback` / `mdx_q` aliases. See [engines.md](../architecture/engines.md).

## `Engine` (trait)

Implement to add a backend. See [engines.md](../architecture/engines.md).

```rust
pub trait Engine: Send + Sync {
    fn id(&self) -> &'static str;
    fn target_sample_rate(&self) -> u32;
    fn realtime_capable(&self) -> bool;
    fn separate(&self, input: &AudioBuffer) -> Result<Separation>;
    // Chunking engines (MDX) override this to report `on_chunk(done, total)`:
    fn separate_with_progress(&self, input: &AudioBuffer,
        on_chunk: &mut dyn FnMut(usize, usize)) -> Result<Separation>;
}
```

## `AudioBuffer`

```rust
AudioBuffer { channels: Vec<Vec<f32>>, sample_rate: u32 }
AudioBuffer::silent(channels, frames, sample_rate)
buf.channel_count(); buf.frame_count(); buf.duration_secs();
buf.resample(target_rate)?;   // rubato FFT resampler; no-op clone when rates already match
```

The pipeline calls `resample()` as a sample-rate guard before handing a buffer to the engine — a
no-op in the CLI path (FFmpeg already extracts at the engine rate), so library callers that pass a
buffer at the wrong rate are corrected transparently.

## `registry`

```rust
use sukoon_core::registry::{Model, License};
Model::all();            // every known model
Model::resolve("mdx")?;
model.ensure_local()?;         // auto-download (with progress) + SHA-256 verify on first use (onnx feature)
model.license.bundle_safe();   // false for CC-BY-SA-4.0 and community download-only weights
```

## `cache`

```rust
use sukoon_core::cache::{Cache, CacheKey};
let key = CacheKey::from_file(path, "deepfilternet", "remove-all")?;
let cache = Cache::default_location()?;
cache.contains(&key);
```

## Errors

`sukoon_core::Error` (`Ffmpeg` / `Engine` / `ModelUnavailable` / `Io` / `Other`) and
`Result<T> = Result<T, Error>`.
