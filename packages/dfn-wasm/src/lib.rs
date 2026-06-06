//! Sukoon's in-page **Fast engine**, compiled to WebAssembly.
//!
//! This is a thin `wasm-bindgen` wrapper over DeepFilterNet's [`DfTract`] — the same pure-Rust
//! `tract` streaming runtime + DSP front-end that powers the upstream LADSPA plugin and web demo.
//! It runs the DFN3 model **frame by frame** with the recurrent state held inside `DfTract`, so the
//! latency is one hop (`hop_size` samples @ 48 kHz ≈ 10 ms) plus the model's small lookahead — i.e.
//! real-time, unlike the desktop's offline windowed path.
//!
//! The DFN3 model is embedded at compile time (`default-model`), so the resulting `.wasm` needs no
//! network and no separate model download: install the extension and it works offline.
//!
//! Audio contract: 48 kHz, mono, processed in blocks of exactly [`DfnDenoiser::frame_length`]
//! samples. Feed a frame to [`DfnDenoiser::process_frame`]; get back the enhanced frame.

// The `deep_filter` package's library is named `df`.
use df::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::prelude::*;
use wasm_bindgen::prelude::*;

/// A live DeepFilterNet denoiser. Construct once, then call [`process_frame`](Self::process_frame)
/// on consecutive `frame_length`-sample blocks of 48 kHz mono audio.
#[wasm_bindgen]
pub struct DfnDenoiser {
    inner: DfTract,
    /// Scratch input/output arrays reused across frames to avoid per-frame allocation.
    out: Array2<f32>,
}

#[wasm_bindgen]
impl DfnDenoiser {
    /// Build the engine from the embedded DFN3 model.
    ///
    /// `atten_lim_db` caps how much the model may attenuate (0 = no limit, the upstream default).
    /// A finite value (e.g. 100) leaves a little of the original through, which can sound more
    /// natural; 0 is maximum suppression.
    ///
    /// We enable DeepFilterNet's **post-filter** (`post_filter_beta = 0.02`, the upstream default).
    /// It's off in `default_with_ch`, but it's a perceptually-motivated over-attenuation of the
    /// noisy bins that markedly reduces the residual "musical noise"/warble of real-time DFN — the
    /// single cheapest lever toward the cleaner offline-desktop quality. Cost is negligible per frame.
    #[wasm_bindgen(constructor)]
    pub fn new(atten_lim_db: f32) -> Result<DfnDenoiser, JsError> {
        console_error_panic_hook::set_once();
        let r_params = RuntimeParams::default_with_ch(1)
            .with_atten_lim(atten_lim_db)
            .with_post_filter(0.02);
        let df_params = DfParams::default(); // embedded DeepFilterNet3_onnx.tar.gz
        let inner = DfTract::new(df_params, &r_params)
            .map_err(|e| JsError::new(&format!("DfTract init failed: {e}")))?;
        let hop = inner.hop_size;
        Ok(DfnDenoiser {
            inner,
            out: Array2::zeros((1, hop)),
        })
    }

    /// The exact block size the engine consumes/produces per call (the model's hop size, 480 @ 48k).
    #[wasm_bindgen(getter)]
    pub fn frame_length(&self) -> usize {
        self.inner.hop_size
    }

    /// The engine's working sample rate (always 48 kHz for DFN3).
    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> u32 {
        48_000
    }

    /// Enhance one frame of exactly [`frame_length`](Self::frame_length) mono samples, returning the
    /// cleaned frame (speech kept, music/noise suppressed). Maintains recurrent state across calls.
    pub fn process_frame(&mut self, input: &[f32]) -> Result<Vec<f32>, JsError> {
        let hop = self.inner.hop_size;
        if input.len() != hop {
            return Err(JsError::new(&format!(
                "process_frame expected {hop} samples, got {}",
                input.len()
            )));
        }
        let noisy = ArrayView2::from_shape((1, hop), input)
            .map_err(|e| JsError::new(&format!("input reshape failed: {e}")))?;
        self.inner
            .process(noisy, self.out.view_mut())
            .map_err(|e| JsError::new(&format!("process failed: {e}")))?;
        Ok(self.out.as_slice().unwrap().to_vec())
    }

    /// Adjust the attenuation limit (dB) live; 0 disables the limit (max suppression).
    pub fn set_atten_lim(&mut self, lim_db: f32) {
        self.inner.set_atten_lim(lim_db);
    }
}
