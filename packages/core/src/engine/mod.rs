//! Separation engine abstraction.

mod deepfilternet;
mod mdx;

pub use deepfilternet::DeepFilterNet;
pub use mdx::MdxNet;

use crate::{AudioBuffer, Result};

/// Build an ONNX Runtime session with acceleration enabled.
#[cfg(feature = "onnx")]
pub(crate) fn build_session(path: &std::path::Path) -> Result<ort::session::Session> {
    build_session_accel(path, true)
}

/// Build a session, optionally registering the platform GPU accelerator.
#[cfg(feature = "onnx")]
pub(crate) fn build_session_accel(
    path: &std::path::Path,
    accelerate: bool,
) -> Result<ort::session::Session> {
    use ort::session::builder::GraphOptimizationLevel;

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let builder = ort::session::Session::builder()
        .map_err(|e| crate::Error::Engine(e.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| crate::Error::Engine(e.to_string()))?
        .with_intra_threads(threads)
        .map_err(|e| crate::Error::Engine(e.to_string()))?;

    let mut builder = if accelerate {
        with_acceleration(builder)?
    } else {
        builder
    };

    builder
        .commit_from_file(path)
        .map_err(|e| crate::Error::Engine(format!("load {}: {e}", path.display())))
}

/// Register the best available hardware accelerator for the current platform.
#[cfg(feature = "onnx")]
fn with_acceleration(
    builder: ort::session::builder::SessionBuilder,
) -> Result<ort::session::builder::SessionBuilder> {
    use ort::execution_providers::ExecutionProviderDispatch;

    let cpu_only = matches!(
        std::env::var("SUKOON_CPU_ONLY").as_deref(),
        Ok("1") | Ok("true") | Ok("on")
    );
    if cpu_only {
        tracing::debug!("execution provider: CPU (SUKOON_CPU_ONLY set)");
        return Ok(builder);
    }

    #[allow(unused_mut)]
    let mut eps: Vec<ExecutionProviderDispatch> = Vec::new();

    // NVIDIA CUDA, opt-in at build time.
    #[cfg(feature = "cuda")]
    {
        use ort::execution_providers::CUDAExecutionProvider;
        eps.push(CUDAExecutionProvider::default().build());
        tracing::info!("CUDA execution provider registered");
    }

    // Windows: DirectML over Direct3D 12.
    #[cfg(target_os = "windows")]
    {
        use ort::execution_providers::DirectMLExecutionProvider;
        eps.push(DirectMLExecutionProvider::default().build());
        tracing::info!("DirectML execution provider registered");
    }

    // Apple: CoreML with an on-disk compiled-model cache.
    #[cfg(target_os = "macos")]
    {
        use ort::execution_providers::coreml::ComputeUnits;
        use ort::execution_providers::CoreMLExecutionProvider;
        let cache_dir = coreml_cache_dir();
        let _ = std::fs::create_dir_all(&cache_dir);
        eps.push(
            CoreMLExecutionProvider::default()
                .with_compute_units(ComputeUnits::All)
                .with_model_cache_dir(cache_dir.to_string_lossy())
                .build(),
        );
        tracing::info!("CoreML execution provider registered");
    }

    if eps.is_empty() {
        // No accelerator compiled in for this platform (e.g. Linux without `--features cuda`).
        tracing::debug!("execution provider: CPU (no accelerator for this build)");
        return Ok(builder);
    }

    builder
        .with_execution_providers(eps)
        .map_err(|e| crate::Error::Engine(e.to_string()))
}

/// Directory where CoreML caches the compiled model.
#[cfg(all(feature = "onnx", target_os = "macos"))]
fn coreml_cache_dir() -> std::path::PathBuf {
    std::env::var_os("SUKOON_MODELS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("sukoon").join("models"))
        .join("coreml-cache")
}

/// A named audio stem produced by separation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stem {
    /// Speech / dialogue / vocals — what Sukoon keeps.
    Speech,
    /// Instrumental music — what Sukoon removes.
    Music,
    /// Non-music sound effects / ambience (only some engines produce this).
    Effects,
}

/// The result of running an engine over an [`AudioBuffer`].
///
/// Engines that do true source separation (BandIt) populate all stems they support. Speech
/// enhancement engines (DeepFilterNet) populate [`Stem::Speech`] and derive the rest.
#[derive(Debug, Clone)]
pub struct Separation {
    /// The enhanced/extracted speech stem — this is what gets remuxed.
    pub speech: AudioBuffer,
    /// The removed music stem, when the engine produces it (useful for debugging/preview).
    pub music: Option<AudioBuffer>,
    /// The effects stem, when the engine produces it.
    pub effects: Option<AudioBuffer>,
}

/// Which engine to run. Maps to a concrete [`Engine`] via [`EngineKind::build`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// DeepFilterNet — tiny, real-time speech enhancer. Default for live playback.
    Fast,
    /// MDX-Net (Kim Vocal 2) — true vocal/instrumental separation. Files/batch/cloud.
    Hq,
    /// Smaller UVR MDX model (9482) — a low-RAM fallback that reuses the MDX path.
    Fallback,
}

impl EngineKind {
    /// Stable string id used in CLIs, configs, and the registry.
    pub fn id(self) -> &'static str {
        match self {
            EngineKind::Fast => "deepfilternet",
            EngineKind::Hq => MdxNet::ID,
            EngineKind::Fallback => MdxNet::FALLBACK_ID,
        }
    }

    /// Parse from the stable id. Accepts a couple of friendly aliases.
    pub fn from_id(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fast" | "deepfilternet" | "dfn" => Some(EngineKind::Fast),
            "hq" | "mdx" | "mdx-net" | "kim-vocal-2" => Some(EngineKind::Hq),
            "fallback" | "mdx-lite" | "mdx_q" => Some(EngineKind::Fallback),
            _ => None,
        }
    }

    /// Instantiate the concrete engine for this kind.
    pub fn build(self) -> Result<Box<dyn Engine>> {
        Ok(match self {
            EngineKind::Fast => Box::new(DeepFilterNet::load()?),
            EngineKind::Hq => Box::new(MdxNet::load()?),
            EngineKind::Fallback => Box::new(MdxNet::load_id(MdxNet::FALLBACK_ID)?),
        })
    }
}

/// A separation backend.
///
/// Implementations are expected to be cheap to keep resident (the model stays loaded) and to
/// process an [`AudioBuffer`] into a [`Separation`]. An engine that chunks internally (like MDX)
/// reports per-chunk progress via [`separate_with_progress`](Engine::separate_with_progress).
pub trait Engine: Send + Sync {
    /// Stable engine id (matches [`EngineKind::id`]).
    fn id(&self) -> &'static str;

    /// The sample rate the model expects. The pipeline resamples to this before calling.
    fn target_sample_rate(&self) -> u32;

    /// Whether this engine is fast enough for real-time/live use.
    fn realtime_capable(&self) -> bool;

    /// Run separation over one buffer and return the stems.
    fn separate(&self, input: &AudioBuffer) -> Result<Separation>;

    /// Like [`separate`](Engine::separate) but reports progress.
    fn separate_with_progress(
        &self,
        input: &AudioBuffer,
        on_chunk: &mut dyn FnMut(usize, usize),
    ) -> Result<Separation> {
        let _ = on_chunk;
        self.separate(input)
    }

    /// Run separation and deliver the speech stem as planar `f32` blocks.
    fn separate_into(
        &self,
        input: &AudioBuffer,
        on_chunk: &mut dyn FnMut(usize, usize),
        sink: &mut dyn FnMut(&[Vec<f32>], usize) -> Result<()>,
    ) -> Result<()> {
        let sep = self.separate_with_progress(input, on_chunk)?;
        let len = sep.speech.frame_count();
        sink(&sep.speech.channels, len)
    }

    /// If this engine supports bounded-memory streaming, return its block plan.
    fn chunk_plan(&self) -> Option<ChunkPlan> {
        None
    }

    /// Separate one streaming block. Only called when [`chunk_plan`](Engine::chunk_plan) is `Some`.
    ///
    /// `window` includes `context` frames around the central `emit_len` frames to emit.
    fn separate_block(
        &self,
        window: &AudioBuffer,
        context: usize,
        emit_len: usize,
    ) -> Result<AudioBuffer> {
        // Only engines that return a `chunk_plan` are ever streamed, and they must override this.
        // Erroring (rather than returning the wrong length/region) makes a missing override loud.
        let _ = (window, context, emit_len);
        Err(crate::Error::Engine(
            "separate_block called on an engine that does not implement streaming".into(),
        ))
    }
}

/// How an engine wants a long input fed to it for bounded-memory streaming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPlan {
    /// Context frames the engine needs on each side of an emitted block (MDX: the STFT `trim`).
    pub context: usize,
    /// The emit granularity in frames — every block emits a whole multiple of this (MDX: `gen_size`).
    pub align: usize,
    /// Target frames to emit per block (a multiple of `align`), sized to a memory budget.
    pub block_frames: usize,
}
