//! MDX-Net — the **HQ** engine.
//!
//! A two-stream complex-spectrogram U-Net (the UVR "Kim Vocal 2" model) that does true
//! vocal/instrumental separation. We keep the vocal/speech stem and derive the music stem by
//! subtraction. Heavier than real-time on CPU (chunked ~6 s windows), so it backs file/batch — not
//! live playback. Weights are downloaded on first use and never bundled (see the registry license).
//!
//! The model has no STFT in its graph: it takes a packed complex spectrogram `[1, 4, dim_f, dim_t]`
//! (`4 = 2 channels × {real, imag}`) and returns the same shape for its target stem. The STFT/ISTFT
//! front-end lives in [`crate::dsp`] and must match the `torch.stft(center=True)` recipe the model
//! was trained with.

use crate::engine::{Engine, Separation};
use crate::registry::Model;
use crate::{AudioBuffer, Result};

#[cfg(feature = "onnx")]
use crate::registry::MdxParams;

/// An MDX-Net separation engine.
///
/// Drives both the **HQ** path (Kim Vocal 2, `dim_f=3072`) and the **low-RAM Fallback** path
/// (UVR 9482, `dim_f=2048`) — the only thing that differs is the registry id and its `MdxParams`,
/// so the whole STFT/demix pipeline is shared.
pub struct MdxNet {
    #[allow(dead_code)]
    model: Model,
    #[cfg(feature = "onnx")]
    params: MdxParams,
    #[cfg(feature = "onnx")]
    session: std::sync::Mutex<ort::session::Session>,
}

impl MdxNet {
    /// Stable id of the HQ model.
    pub const ID: &'static str = "mdx";
    /// Stable id of the low-RAM fallback model.
    pub const FALLBACK_ID: &'static str = "mdx-lite";

    /// Load the HQ model, downloading + verifying weights on first use.
    pub fn load() -> Result<Self> {
        Self::load_id(Self::ID)
    }

    /// Load a specific registry model by id (must be an MDX model — carry `MdxParams`).
    pub fn load_id(id: &'static str) -> Result<Self> {
        let model = Model::resolve(id)?;

        #[cfg(feature = "onnx")]
        {
            let params = model.mdx.ok_or_else(|| {
                crate::Error::Engine(format!("model `{id}` is missing MDX params"))
            })?;
            let path = model.ensure_local()?;
            let session = crate::engine::build_session(&path)?;
            return Ok(Self {
                model,
                params,
                session: std::sync::Mutex::new(session),
            });
        }

        #[cfg(not(feature = "onnx"))]
        Ok(Self { model })
    }
}

impl Engine for MdxNet {
    fn id(&self) -> &'static str {
        self.model.id
    }

    fn target_sample_rate(&self) -> u32 {
        #[cfg(feature = "onnx")]
        {
            self.params.sample_rate
        }
        #[cfg(not(feature = "onnx"))]
        {
            44_100
        }
    }

    fn realtime_capable(&self) -> bool {
        false
    }

    fn separate(&self, input: &AudioBuffer) -> Result<Separation> {
        #[cfg(feature = "onnx")]
        {
            self.demix(input, &mut |_, _| {})
        }

        #[cfg(not(feature = "onnx"))]
        {
            // Dry-mode passthrough until ONNX inference is compiled in.
            Ok(Separation {
                speech: input.clone(),
                music: None,
                effects: None,
            })
        }
    }

    #[cfg(feature = "onnx")]
    fn separate_with_progress(
        &self,
        input: &AudioBuffer,
        on_chunk: &mut dyn FnMut(usize, usize),
    ) -> Result<Separation> {
        self.demix(input, on_chunk)
    }

    #[cfg(feature = "onnx")]
    fn chunk_plan(&self) -> Option<crate::engine::ChunkPlan> {
        Some(self.plan())
    }

    #[cfg(feature = "onnx")]
    fn separate_block(
        &self,
        window: &AudioBuffer,
        context: usize,
        emit_len: usize,
    ) -> Result<AudioBuffer> {
        self.separate_block_impl(window, context, emit_len)
    }
}

#[cfg(feature = "onnx")]
impl MdxNet {
    /// Run the MDX overlap-add demix and split into vocal (kept) and music stems.
    ///
    /// `on_chunk(done, total)` is called once per inference chunk for progress reporting.
    fn demix(
        &self,
        input: &AudioBuffer,
        on_chunk: &mut dyn FnMut(usize, usize),
    ) -> Result<Separation> {
        let p = self.params;
        let mix = to_stereo(input);
        let n = mix[0].len();
        if n == 0 {
            return Ok(Separation {
                speech: input.clone(),
                music: None,
                effects: None,
            });
        }

        let trim = p.n_fft / 2;
        let chunk_size = p.hop * (p.dim_t - 1);
        let gen_size = chunk_size - 2 * trim;
        // Front-pad by `trim`, tail-pad so the body is a whole number of `gen_size` blocks, then
        // tail-pad another `trim` so the final chunk window stays in bounds.
        let body_pad = (gen_size - n % gen_size) % gen_size;
        let total = trim + n + body_pad + trim;
        let mut padded = [vec![0.0f32; total], vec![0.0f32; total]];
        for ch in 0..2 {
            padded[ch][trim..trim + n].copy_from_slice(&mix[ch]);
        }

        let n_blocks = (n + body_pad) / gen_size;
        let mut vocals = self.run_windows(&padded, n_blocks, on_chunk)?;

        // Crop back to the original length; derive the music stem as mix − vocals.
        let mut music = [vec![0.0f32; n], vec![0.0f32; n]];
        for ch in 0..2 {
            vocals[ch].truncate(n);
            for j in 0..n {
                music[ch][j] = mix[ch][j] - vocals[ch][j];
            }
        }

        // `output_is_vocals == false` would mean the model emits the instrumental; swap so `speech`
        // always carries the kept vocal/recitation stem.
        let (speech_ch, music_ch) = if p.output_is_vocals {
            (vocals, music)
        } else {
            (music, vocals)
        };

        Ok(Separation {
            speech: AudioBuffer {
                channels: speech_ch.to_vec(),
                sample_rate: p.sample_rate,
            },
            music: Some(AudioBuffer {
                channels: music_ch.to_vec(),
                sample_rate: p.sample_rate,
            }),
            effects: None,
        })
    }

    /// The overlap-add window loop, shared by [`demix`](Self::demix) (whole file) and
    /// [`separate_block`](Self::separate_block) (streaming). Runs `n_blocks` inference windows over
    /// `padded` — window `i` reads `padded[i*gen .. i*gen + chunk_size]` and emits the central
    /// `gen_size` samples — returning the two vocal channels (`n_blocks * gen_size` samples each).
    /// `padded` must hold at least `n_blocks * gen_size + 2 * trim` samples per channel.
    fn run_windows(
        &self,
        padded: &[Vec<f32>; 2],
        n_blocks: usize,
        on_chunk: &mut dyn FnMut(usize, usize),
    ) -> Result<[Vec<f32>; 2]> {
        use crate::dsp::Stft;
        use realfft::num_complex::Complex32;

        let p = self.params;
        let trim = p.n_fft / 2;
        let chunk_size = p.hop * (p.dim_t - 1);
        let gen_size = chunk_size - 2 * trim;

        let stft = Stft::new(p.n_fft, p.hop);
        let n_bins = stft.n_bins();
        debug_assert!(p.dim_f <= n_bins);

        let mut vocals = [
            Vec::<f32>::with_capacity(n_blocks * gen_size),
            Vec::<f32>::with_capacity(n_blocks * gen_size),
        ];

        let mut session = self
            .session
            .lock()
            .map_err(|_| crate::Error::Engine("mdx session lock poisoned".into()))?;

        let mut t_stft = std::time::Duration::ZERO;
        let mut t_infer = std::time::Duration::ZERO;
        let mut t_istft = std::time::Duration::ZERO;

        for i in 0..n_blocks {
            on_chunk(i, n_blocks);
            let start = i * gen_size;

            // Pack the two channels' spectrograms into [1, 4, dim_f, dim_t] (C-order).
            // Plane layout: [L_re, L_im, R_re, R_im].
            let t = std::time::Instant::now();
            let mut data = vec![0.0f32; 4 * p.dim_f * p.dim_t];
            for ch in 0..2 {
                let chunk = &padded[ch][start..start + chunk_size];
                let spec = stft.forward(chunk); // [dim_t][n_bins]
                let re_plane = ch * 2;
                let im_plane = ch * 2 + 1;
                for (t, frame) in spec.iter().enumerate() {
                    for f in 0..p.dim_f {
                        let c = frame[f];
                        data[(re_plane * p.dim_f + f) * p.dim_t + t] = c.re;
                        data[(im_plane * p.dim_f + f) * p.dim_t + t] = c.im;
                    }
                }
            }
            t_stft += t.elapsed();

            let t = std::time::Instant::now();
            let tensor = ort::value::Tensor::from_array((
                vec![1i64, 4, p.dim_f as i64, p.dim_t as i64],
                data,
            ))
            .map_err(|e| crate::Error::Engine(format!("build input tensor: {e}")))?;
            let outputs = session
                .run(ort::inputs!["input" => tensor])
                .map_err(|e| crate::Error::Engine(format!("mdx inference: {e}")))?;
            let (_shape, out) = outputs["output"]
                .try_extract_tensor::<f32>()
                .map_err(|e| crate::Error::Engine(format!("extract output: {e}")))?;
            // The inverse loop indexes `out` up to `4*dim_f*dim_t`; guard against a model whose
            // output is smaller than the registry's params claim (return an error, not a panic).
            let expected = 4 * p.dim_f * p.dim_t;
            if out.len() < expected {
                return Err(crate::Error::Engine(format!(
                    "mdx output too small: got {}, need {expected} (dim_f={}, dim_t={})",
                    out.len(),
                    p.dim_f,
                    p.dim_t
                )));
            }
            t_infer += t.elapsed();

            // Invert each channel's predicted spectrogram and keep the central `gen_size` samples.
            let t = std::time::Instant::now();
            for ch in 0..2 {
                let re_plane = ch * 2;
                let im_plane = ch * 2 + 1;
                let mut spec_out = vec![vec![Complex32::new(0.0, 0.0); n_bins]; p.dim_t];
                for (t, frame) in spec_out.iter_mut().enumerate() {
                    for f in 0..p.dim_f {
                        let re = out[(re_plane * p.dim_f + f) * p.dim_t + t];
                        let im = out[(im_plane * p.dim_f + f) * p.dim_t + t];
                        frame[f] = Complex32::new(re, im);
                    }
                }
                let wav = stft.inverse(&spec_out, chunk_size);
                vocals[ch].extend_from_slice(&wav[trim..trim + gen_size]);
            }
            t_istft += t.elapsed();
        }
        drop(session);
        tracing::debug!(
            stft_ms = t_stft.as_millis(),
            infer_ms = t_infer.as_millis(),
            istft_ms = t_istft.as_millis(),
            "mdx demix phase timings"
        );
        Ok(vocals)
    }

    /// Streaming counterpart of [`demix`](Self::demix): separate one pre-padded block window.
    fn separate_block_impl(
        &self,
        window: &AudioBuffer,
        context: usize,
        emit_len: usize,
    ) -> Result<AudioBuffer> {
        let p = self.params;
        let trim = p.n_fft / 2;
        let chunk_size = p.hop * (p.dim_t - 1);
        let gen_size = chunk_size - 2 * trim;
        debug_assert_eq!(context, trim, "streaming context must equal the STFT trim");
        debug_assert_eq!(
            emit_len % gen_size,
            0,
            "emit must be a whole number of blocks"
        );

        let mix = to_stereo(window); // already padded: signal[lo-trim .. lo+emit+trim]
        let n_blocks = emit_len / gen_size;
        let mut vocals = self.run_windows(&mix, n_blocks, &mut |_, _| {})?;

        // `vocals` is exactly `emit_len` samples. Build the speech stem (swap if the model emits
        // the instrumental), keeping the same 2-channel shape the whole-file path produces.
        let speech = if p.output_is_vocals {
            vocals.to_vec()
        } else {
            for ch in 0..2 {
                for j in 0..emit_len {
                    vocals[ch][j] = mix[ch][context + j] - vocals[ch][j];
                }
            }
            vocals.to_vec()
        };

        Ok(AudioBuffer {
            channels: speech,
            sample_rate: p.sample_rate,
        })
    }

    /// The block plan: STFT `trim` of context per side, `gen_size` emit granularity, and a block
    /// sized to a ~32 MB working set so memory stays bounded regardless of file length.
    fn plan(&self) -> crate::engine::ChunkPlan {
        let p = self.params;
        let trim = p.n_fft / 2;
        let gen_size = p.hop * (p.dim_t - 1) - 2 * trim;
        // ~32 MB / (2 ch * 4 bytes) ≈ 4M frames; round down to whole blocks.
        let blocks = (4_000_000 / gen_size).max(1);
        crate::engine::ChunkPlan {
            context: trim,
            align: gen_size,
            block_frames: blocks * gen_size,
        }
    }
}

/// Coerce any buffer to exactly two equal-length channel planes.
#[cfg(feature = "onnx")]
fn to_stereo(input: &AudioBuffer) -> [Vec<f32>; 2] {
    match input.channels.len() {
        0 => [Vec::new(), Vec::new()],
        1 => [input.channels[0].clone(), input.channels[0].clone()],
        _ => [input.channels[0].clone(), input.channels[1].clone()],
    }
}
