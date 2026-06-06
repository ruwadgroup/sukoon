//! Decode, separate, and remux orchestration.

use std::path::Path;
use std::sync::Arc;

use crate::engine::{Engine, EngineKind};
use crate::{cache::Cache, ffmpeg, Result};

/// How aggressively to remove music. The string form is part of the cache key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeparationMode {
    /// Remove all music (the default, broadest position).
    #[default]
    RemoveAll,
    /// Keep percussion (duff/hand-drum), remove melodic instruments.
    KeepPercussion,
    /// Keep vocals, remove instruments only.
    KeepVocals,
    /// Keep ambient sound effects, remove only music.
    PreserveEffects,
}

impl SeparationMode {
    /// Stable string id (used in cache keys and CLIs).
    pub fn id(self) -> &'static str {
        match self {
            SeparationMode::RemoveAll => "remove-all",
            SeparationMode::KeepPercussion => "keep-percussion",
            SeparationMode::KeepVocals => "keep-vocals",
            SeparationMode::PreserveEffects => "preserve-effects",
        }
    }
}

/// Options for constructing a [`Pipeline`].
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    /// Which engine to run.
    pub engine: EngineKind,
    /// Separation mode.
    pub mode: SeparationMode,
    /// Whether to use the on-disk cache.
    pub use_cache: bool,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            engine: EngineKind::Fast,
            mode: SeparationMode::RemoveAll,
            use_cache: true,
        }
    }
}

/// A pipeline progress event, delivered to an optional [`Pipeline::on_progress`] callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Extracting the audio track from the input (FFmpeg).
    Extract,
    /// Running separation; `chunk` of `total` engine chunks done.
    Separate { chunk: usize, total: usize },
    /// Encoding the cleaned speech stem to WAV.
    Encode,
    /// Remuxing the clean audio onto the original container (FFmpeg).
    Remux,
    /// All done.
    Done,
}

/// A progress callback. Must be `Send + Sync` so a `Pipeline` stays shareable.
pub type ProgressFn = Box<dyn Fn(Progress) + Send + Sync>;

/// The processing pipeline. Construct once, reuse for many files (the model stays loaded).
pub struct Pipeline {
    engine: Arc<dyn Engine>,
    mode: SeparationMode,
    cache: Option<Cache>,
    progress: Option<ProgressFn>,
}

impl Pipeline {
    /// Build a pipeline, loading the requested engine.
    pub fn new(options: PipelineOptions) -> Result<Self> {
        let engine: Arc<dyn Engine> = Arc::from(options.engine.build()?);
        Ok(Self::from_engine(engine, options.mode, options.use_cache))
    }

    /// Build a pipeline reusing an already-loaded engine.
    pub fn from_engine(engine: Arc<dyn Engine>, mode: SeparationMode, use_cache: bool) -> Self {
        let cache = if use_cache {
            Cache::default_location().ok()
        } else {
            None
        };
        Self {
            engine,
            mode,
            cache,
            progress: None,
        }
    }

    /// Attach a progress callback (builder-style). Cheap and ignored when unset.
    pub fn on_progress(mut self, f: impl Fn(Progress) + Send + Sync + 'static) -> Self {
        self.progress = Some(Box::new(f));
        self
    }

    /// Emit a progress event if a callback is attached.
    fn report(&self, p: Progress) {
        if let Some(cb) = &self.progress {
            cb(p);
        }
    }

    /// The id of the loaded engine.
    pub fn engine_id(&self) -> &'static str {
        self.engine.id()
    }

    /// Whether the loaded engine can run in real time.
    pub fn realtime_capable(&self) -> bool {
        self.engine.realtime_capable()
    }

    /// Clean a media file and remux clean audio onto the original video when present.
    pub fn clean_file(&self, input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<()> {
        let input = input.as_ref();
        let output = output.as_ref();
        let target = self.engine.target_sample_rate();

        let tmp = tempdir()?;
        let extracted = tmp.join("in.wav");
        let cleaned = tmp.join("speech.wav");

        let key = self.cache.as_ref().and_then(|_| {
            crate::cache::CacheKey::from_file(input, self.engine.id(), self.mode.id()).ok()
        });
        let cache_hit = matches!((&self.cache, &key), (Some(cache), Some(k)) if cache.contains(k));
        let has_video = ffmpeg::has_video_stream(input)?;

        // Cache hit: reuse the stored speech stem. There's no separation to overlap the encode with,
        // so just encode the cached stem onto the original.
        if let (Some(cache), Some(k), true) = (&self.cache, &key, cache_hit) {
            tracing::info!(key = k.as_str(), "cache hit");
            std::fs::copy(cache.path_for(k), &cleaned)?;
            self.encode_from_wav(input, &cleaned, has_video, output)?;
            self.report(Progress::Done);
            return Ok(());
        }

        // Cache miss: extract the audio track at the engine's sample rate.
        self.report(Progress::Extract);
        ffmpeg::extract_audio(input, &extracted, target)?;

        // Bounded-memory streaming path (a streaming engine on a long file): separate block-by-block
        // to a temp WAV, then encode. Peak memory stays ~one block regardless of length.
        if let Some(plan) = self.engine.chunk_plan() {
            let reader = crate::audio::WavBlockReader::open(&extracted)?;
            if reader.sample_rate() == target && reader.total_frames() as usize > plan.block_frames
            {
                self.separate_streaming(reader, plan, &cleaned)?;
                self.store_cache(&cleaned, &key);
                self.encode_from_wav(input, &cleaned, has_video, output)?;
                self.report(Progress::Done);
                return Ok(());
            }
        }

        // Whole-buffer path (DeepFilterNet, and short clips). Guard the engine's sample-rate contract
        // (a no-op clone when FFmpeg already extracted at the engine rate).
        let decoded = crate::AudioBuffer::read_wav(&extracted)?;
        tracing::debug!(
            channels = decoded.channel_count(),
            frames = decoded.frame_count(),
            sample_rate = decoded.sample_rate,
            "decoded input"
        );
        let buffer = decoded.resample(target)?;
        let want_cache = matches!((&self.cache, &key), (Some(_), Some(_)));

        if has_video {
            // Overlap separation with the AAC encode: stream cleaned PCM straight into a piped
            // encoder as it's produced, so the (single-threaded) encode runs concurrently.
            self.separate_overlapped(
                &buffer,
                input,
                output,
                want_cache.then_some(cleaned.as_path()),
            )?;
        } else {
            // No video → no AAC encode to overlap (the cleaned audio is written as-is), so the
            // simple whole-buffer path is already optimal.
            let mut on_chunk =
                |chunk: usize, total: usize| self.report(Progress::Separate { chunk, total });
            let sep = self.engine.separate_with_progress(&buffer, &mut on_chunk)?;
            debug_assert_eq!(sep.speech.sample_rate, buffer.sample_rate);
            sep.speech.write_wav(&cleaned)?;
            self.report(Progress::Encode);
            std::fs::copy(&cleaned, output)?;
        }

        self.store_cache(&cleaned, &key);
        self.report(Progress::Done);
        Ok(())
    }

    /// Encode a finished speech WAV onto `output`: remux onto the source video (`-c:v copy`, no
    /// re-encode) when there's a video stream, else write the cleaned audio directly.
    fn encode_from_wav(
        &self,
        source: &Path,
        speech_wav: &Path,
        has_video: bool,
        output: &Path,
    ) -> Result<()> {
        if has_video {
            self.report(Progress::Remux);
            ffmpeg::remux_audio(source, speech_wav, output)
        } else {
            self.report(Progress::Encode);
            std::fs::copy(speech_wav, output)?;
            Ok(())
        }
    }

    /// Store the cleaned stem in the cache (best-effort; a write failure is logged, not fatal).
    fn store_cache(&self, cleaned: &Path, key: &Option<crate::cache::CacheKey>) {
        if let (Some(cache), Some(k)) = (&self.cache, key) {
            if let Err(e) = std::fs::copy(cleaned, cache.path_for(k)) {
                tracing::warn!(error = %e, "failed to write cache entry");
            }
        }
    }

    /// Whole-buffer separation that **overlaps** with the AAC encode for a video input.
    ///
    /// A piped [`ffmpeg::PcmEncoder`] is spawned on the first emitted window and fed cleaned PCM as
    /// the engine produces it, so FFmpeg's encode runs concurrently with separation instead of
    /// after it. The output is bit-identical to the sequential path — only the wall-clock changes.
    /// When caching, each window is tee'd into `cache_wav` as well.
    fn separate_overlapped(
        &self,
        buffer: &crate::AudioBuffer,
        video_source: &Path,
        output: &Path,
        cache_wav: Option<&Path>,
    ) -> Result<()> {
        let target_sr = self.engine.target_sample_rate();
        let mut encoder: Option<ffmpeg::PcmEncoder> = None;
        let mut cache_w: Option<crate::audio::WavBlockWriter> = None;

        self.report(Progress::Encode); // the encoder runs concurrently from here on
        let mut on_chunk =
            |chunk: usize, total: usize| self.report(Progress::Separate { chunk, total });
        let mut sink = |block: &[Vec<f32>], len: usize| -> Result<()> {
            let n_ch = block.len().max(1);
            let enc = match &mut encoder {
                Some(e) => e,
                None => {
                    encoder = Some(ffmpeg::PcmEncoder::spawn_remux(
                        video_source,
                        n_ch,
                        target_sr,
                        output,
                    )?);
                    encoder.as_mut().unwrap()
                }
            };
            enc.write_planar(block, len)?;
            if let Some(path) = cache_wav {
                let w = match &mut cache_w {
                    Some(w) => w,
                    None => {
                        cache_w =
                            Some(crate::audio::WavBlockWriter::create(path, n_ch, target_sr)?);
                        cache_w.as_mut().unwrap()
                    }
                };
                w.write_block(block, len)?;
            }
            Ok(())
        };
        self.engine
            .separate_into(buffer, &mut on_chunk, &mut sink)?;

        // Spawn an encoder even if the engine emitted nothing (degenerate empty input) so we still
        // write a valid output container.
        let encoder = match encoder {
            Some(e) => e,
            None => ffmpeg::PcmEncoder::spawn_remux(
                video_source,
                buffer.channel_count().max(1),
                target_sr,
                output,
            )?,
        };
        encoder.finish()?;
        if let Some(w) = cache_w {
            w.finalize()?;
        }
        Ok(())
    }

    /// Bounded-memory separation: process the input in blocks, writing speech as it's produced.
    ///
    /// Each block emits `plan.align`-aligned ranges and is fed the surrounding `plan.context`
    /// samples on both sides, so the output is byte-identical to the whole-buffer path while peak
    /// memory stays bounded by one block regardless of file length.
    fn separate_streaming(
        &self,
        mut reader: crate::audio::WavBlockReader,
        plan: crate::engine::ChunkPlan,
        output: &Path,
    ) -> Result<()> {
        let n = reader.total_frames();
        let sr = reader.sample_rate();
        let in_ch = reader.channel_count();
        let trim = plan.context;
        let align = plan.align;

        // The whole-buffer path pads the body up to a whole number of `align` blocks; mirror that.
        let total_emit = n.div_ceil(align as u64) * align as u64;
        let total_blocks = (total_emit / align as u64) as usize;

        let mut writer: Option<crate::audio::WavBlockWriter> = None;
        let mut emitted: u64 = 0;
        while emitted < total_emit {
            let emit = (plan.block_frames as u64).min(total_emit - emitted) as usize;

            // Window = signal[emitted - trim .. emitted + emit + trim], zero-filled past the ends.
            let win_len = emit + 2 * trim;
            let lo = emitted as i64 - trim as i64;
            let read_start = lo.max(0) as u64;
            let read_end = ((lo + win_len as i64).max(0) as u64).min(n);
            let read_len = read_end.saturating_sub(read_start) as usize;
            let data = reader.read_range(read_start, read_len)?;
            let offset = (read_start as i64 - lo) as usize;

            let mut planar = vec![vec![0.0f32; win_len]; in_ch];
            for (c, planar_c) in planar.iter_mut().enumerate() {
                if let Some(src) = data.channels.get(c) {
                    planar_c[offset..offset + src.len()].copy_from_slice(src);
                }
            }
            let window = crate::AudioBuffer {
                channels: planar,
                sample_rate: sr,
            };

            let speech = self.engine.separate_block(&window, trim, emit)?;

            let w = match &mut writer {
                Some(w) => w,
                None => {
                    writer = Some(crate::audio::WavBlockWriter::create(
                        output,
                        speech.channel_count(),
                        sr,
                    )?);
                    writer.as_mut().unwrap()
                }
            };
            // Crop the final block back to the true length (drop the body padding).
            let write_len = ((n - emitted).min(emit as u64)) as usize;
            w.write_block(&speech.channels, write_len)?;

            emitted += emit as u64;
            self.report(Progress::Separate {
                chunk: (emitted / align as u64) as usize,
                total: total_blocks,
            });
        }

        if let Some(w) = writer {
            w.finalize()?;
        }
        Ok(())
    }
}

fn tempdir() -> Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!("sukoon-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
