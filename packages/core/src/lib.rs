//! # sukoon-core
//!
//! The shared audio engine behind Sukoon's file tools (CLI, desktop, mobile).
//!
//! Sukoon removes background **music** from media while preserving **speech**. This crate owns the
//! whole pipeline — decode, separate, remux — and exposes it through one [`Pipeline`] type and one
//! [`Engine`] trait. **Platform shells must not reimplement separation; they call into here.**
//!
//! ```no_run
//! use sukoon_core::{Pipeline, PipelineOptions, EngineKind};
//!
//! let pipeline = Pipeline::new(PipelineOptions {
//!     engine: EngineKind::Fast,
//!     ..Default::default()
//! })?;
//!
//! // Clean a file: extract audio, drop the music stem, remux losslessly.
//! pipeline.clean_file("input.mp4", "output.mp4")?;
//! # Ok::<(), sukoon_core::Error>(())
//! ```
//!
//! ## Layout
//!
//! - [`engine`] — the [`Engine`] trait and its implementations (DeepFilterNet, MDX-Net, Demucs).
//! - [`pipeline`] — orchestration: decode → separate → remux, with chunking and caching.
//! - [`audio`] — WAV decode/encode between [`AudioBuffer`] and disk.
//! - [`ffmpeg`] — thin wrapper around the FFmpeg binary for audio extraction and remuxing.
//! - [`registry`] — the model registry: URLs, checksums, and **licenses** per model.
//! - [`cache`] — content-hash keyed cache of cleaned stems.

pub mod audio;
pub mod cache;
#[cfg(feature = "onnx")]
pub mod dsp;
pub mod engine;
pub mod ffmpeg;
pub mod pipeline;
pub mod registry;

pub use engine::{Engine, EngineKind, Separation, Stem};
pub use pipeline::{Pipeline, PipelineOptions, Progress, SeparationMode};

/// Audio represented as planar f32 samples, one `Vec` per channel.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    /// Planar samples: `channels[c][frame]`.
    pub channels: Vec<Vec<f32>>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
}

impl AudioBuffer {
    /// Create a silent buffer of `frames` length with the given shape.
    pub fn silent(channels: usize, frames: usize, sample_rate: u32) -> Self {
        Self {
            channels: vec![vec![0.0; frames]; channels],
            sample_rate,
        }
    }

    /// Number of channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Number of frames per channel (0 if no channels).
    pub fn frame_count(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    /// Duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.frame_count() as f64 / f64::from(self.sample_rate)
    }
}

/// The crate-wide error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ffmpeg error: {0}")]
    Ffmpeg(String),

    #[error("engine error: {0}")]
    Engine(String),

    #[error("model not available: {0}")]
    ModelUnavailable(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, Error>;
