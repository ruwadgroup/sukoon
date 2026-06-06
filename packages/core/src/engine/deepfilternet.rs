//! DeepFilterNet Fast engine.

use crate::engine::{Engine, Separation};
use crate::registry::Model;
use crate::{AudioBuffer, Result};

/// The DeepFilterNet Fast engine.
pub struct DeepFilterNet {
    #[allow(dead_code)]
    model: Model,
    #[cfg(feature = "dfn")]
    nets: std::sync::Mutex<DfnNets>,
}

/// The three DFN3 ONNX graphs (encoder, ERB-gain decoder, deep-filter decoder).
#[cfg(feature = "dfn")]
struct DfnNets {
    enc: ort::session::Session,
    erb_dec: ort::session::Session,
    df_dec: ort::session::Session,
}

impl DeepFilterNet {
    /// Stable id.
    pub const ID: &'static str = "deepfilternet";

    /// Load the model, downloading + extracting the ONNX bundle on first use.
    pub fn load() -> Result<Self> {
        let model = Model::resolve(Self::ID)?;

        #[cfg(feature = "dfn")]
        {
            let dir = model.ensure_extracted(&["enc.onnx", "erb_dec.onnx", "df_dec.onnx"])?;
            // DFN is small and recurrent, so CPU sessions are faster than GPU here.
            let nets = DfnNets {
                enc: crate::engine::build_session_accel(&dir.join("enc.onnx"), false)?,
                erb_dec: crate::engine::build_session_accel(&dir.join("erb_dec.onnx"), false)?,
                df_dec: crate::engine::build_session_accel(&dir.join("df_dec.onnx"), false)?,
            };
            return Ok(Self {
                model,
                nets: std::sync::Mutex::new(nets),
            });
        }

        #[cfg(not(feature = "dfn"))]
        Ok(Self { model })
    }
}

impl Engine for DeepFilterNet {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn target_sample_rate(&self) -> u32 {
        48_000
    }

    fn realtime_capable(&self) -> bool {
        true
    }

    fn separate(&self, input: &AudioBuffer) -> Result<Separation> {
        #[cfg(feature = "dfn")]
        {
            self.enhance(input)
        }

        #[cfg(not(feature = "dfn"))]
        {
            // Passthrough keeps shells and tests usable without bundled DFN weights.
            Ok(Separation {
                speech: input.clone(),
                music: None,
                effects: None,
            })
        }
    }

    /// Stream enhanced speech one window at a time.
    fn separate_into(
        &self,
        input: &AudioBuffer,
        on_chunk: &mut dyn FnMut(usize, usize),
        sink: &mut dyn FnMut(&[Vec<f32>], usize) -> Result<()>,
    ) -> Result<()> {
        #[cfg(feature = "dfn")]
        {
            self.enhance_into(input, on_chunk, sink)
        }

        #[cfg(not(feature = "dfn"))]
        {
            let _ = on_chunk;
            sink(&input.channels, input.frame_count())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Real inference (dfn feature)
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "dfn")]
mod dfn {
    use super::*;
    use df::DFState;
    use ort::value::Tensor;
    use realfft::num_complex::Complex32;

    // DFN3 constants (from the model's config.ini / upstream recipe).
    const SR: usize = 48_000;
    const FFT: usize = 960;
    const HOP: usize = 480;
    const FREQS: usize = FFT / 2 + 1; // 481
    const NB_ERB: usize = 32;
    const MIN_NB_ERB: usize = 2;
    const NB_DF: usize = 96;
    const DF_ORDER: usize = 5;
    // Lookaheads from config.ini.
    const CONV_LOOKAHEAD: usize = 2;
    const DF_LOOKAHEAD: usize = 2;
    // EMA normalization decay for a 480/48000 hop with tau=1s.
    const ALPHA: f32 = 0.99005;

    impl DeepFilterNet {
        /// Enhance the whole input and return the speech stem.
        pub(super) fn enhance(&self, input: &AudioBuffer) -> Result<Separation> {
            let n_ch = input.channel_count().max(1);
            let mut channels: Vec<Vec<f32>> = vec![Vec::new(); n_ch];
            let mut noop = |_: usize, _: usize| {};
            self.enhance_into(input, &mut noop, &mut |block, len| {
                for (dst, src) in channels.iter_mut().zip(block) {
                    dst.extend_from_slice(&src[..len]);
                }
                Ok(())
            })?;
            let sample_rate = if input.frame_count() == 0 {
                input.sample_rate
            } else {
                SR as u32
            };
            Ok(Separation {
                speech: AudioBuffer {
                    channels,
                    sample_rate,
                },
                music: None, // a speech enhancer derives no separate music stem
                effects: None,
            })
        }

        /// Enhance the input, emitting clean speech window by window.
        pub(super) fn enhance_into(
            &self,
            input: &AudioBuffer,
            on_chunk: &mut dyn FnMut(usize, usize),
            sink: &mut dyn FnMut(&[Vec<f32>], usize) -> Result<()>,
        ) -> Result<()> {
            let n_ch = input.channel_count();
            let n_in = input.frame_count();
            if n_in == 0 || n_ch == 0 {
                sink(&input.channels, n_in)?;
                return Ok(());
            }

            // DFN is a single-channel speech model.
            let mut mono = vec![0.0f32; n_in];
            for ch in &input.channels {
                for (m, &s) in mono.iter_mut().zip(ch) {
                    *m += s;
                }
            }
            let inv = 1.0 / n_ch as f32;
            for m in &mut mono {
                *m *= inv;
            }

            // Frame into `HOP` blocks (pad the final partial frame).
            let n_frames = n_in.div_ceil(HOP);
            mono.resize(n_frames * HOP, 0.0);

            let mut state = DFState::new(SR, FFT, HOP, NB_ERB, MIN_NB_ERB);
            state.init_norm_states(NB_DF);

            // Norm EMAs are sequential, so analysis must stay in order.
            let mut specs: Vec<Vec<Complex32>> = Vec::with_capacity(n_frames); // noisy spectra [t][FREQS]
            let mut feat_erb = vec![0.0f32; n_frames * NB_ERB]; // [S,32]
            let mut feat_spec = vec![0.0f32; 2 * n_frames * NB_DF]; // [2,S,96] (plane 0 re, 1 im)
            let mut erb_tmp = vec![0.0f32; NB_ERB];
            let mut cplx_tmp = vec![Complex32::default(); NB_DF];

            for t in 0..n_frames {
                let mut spec = vec![Complex32::default(); FREQS];
                state.analysis(&mono[t * HOP..t * HOP + HOP], &mut spec);
                state.feat_erb(&spec, ALPHA, &mut erb_tmp);
                state.feat_cplx(&spec[..NB_DF], ALPHA, &mut cplx_tmp);

                feat_erb[t * NB_ERB..(t + 1) * NB_ERB].copy_from_slice(&erb_tmp);
                for f in 0..NB_DF {
                    feat_spec[t * NB_DF + f] = cplx_tmp[f].re; // plane 0
                    feat_spec[(n_frames + t) * NB_DF + f] = cplx_tmp[f].im; // plane 1
                }
                specs.push(spec);
            }

            // Fixed windows bound ONNX intermediates; warmup context hides cold recurrent state.
            const NET_CHUNK: usize = 4096; // ~40 s of emitted frames per window
            const NET_WARMUP: usize = 384; // ~3.8 s of warmup context per side

            let mut nets = self
                .nets
                .lock()
                .map_err(|_| crate::Error::Engine("dfn session lock poisoned".into()))?;

            let mut frame_out = vec![0.0f32; HOP];
            let window_count = n_frames.div_ceil(NET_CHUNK);

            let mut cs = 0usize;
            let mut window_index = 0usize;
            while cs < n_frames {
                on_chunk(window_index, window_count);
                let ce = (cs + NET_CHUNK).min(n_frames);
                let ws = cs.saturating_sub(NET_WARMUP);
                let we = (ce + NET_WARMUP).min(n_frames);
                let clen = we - ws;

                // Apply the encoder's feature lookahead.
                let mut erb_in = vec![0.0f32; clen * NB_ERB];
                let mut spec_in = vec![0.0f32; 2 * clen * NB_DF];
                for r in 0..clen {
                    let src = ws + r + CONV_LOOKAHEAD;
                    if src >= n_frames {
                        continue; // tail sees zeros
                    }
                    erb_in[r * NB_ERB..(r + 1) * NB_ERB]
                        .copy_from_slice(&feat_erb[src * NB_ERB..(src + 1) * NB_ERB]);
                    for f in 0..NB_DF {
                        spec_in[r * NB_DF + f] = feat_spec[src * NB_DF + f];
                        spec_in[(clen + r) * NB_DF + f] = feat_spec[(n_frames + src) * NB_DF + f];
                    }
                }

                let cl = clen as i64;
                let erb_t = Tensor::from_array((vec![1, 1, cl, NB_ERB as i64], erb_in))
                    .map_err(|e| eng(format!("feat_erb tensor: {e}")))?;
                let spec_t = Tensor::from_array((vec![1, 2, cl, NB_DF as i64], spec_in))
                    .map_err(|e| eng(format!("feat_spec tensor: {e}")))?;

                let enc = nets
                    .enc
                    .run(ort::inputs!["feat_erb" => erb_t, "feat_spec" => spec_t])
                    .map_err(|e| eng(format!("enc run: {e}")))?;
                let e0 = owned(&enc, "e0")?;
                let e1 = owned(&enc, "e1")?;
                let e2 = owned(&enc, "e2")?;
                let e3 = owned(&enc, "e3")?;
                let emb = owned(&enc, "emb")?;
                let c0 = owned(&enc, "c0")?;
                drop(enc);

                let erb_dec_out = nets
                    .erb_dec
                    .run(ort::inputs![
                        "emb" => to_tensor(&emb)?,
                        "e3" => to_tensor(&e3)?,
                        "e2" => to_tensor(&e2)?,
                        "e1" => to_tensor(&e1)?,
                        "e0" => to_tensor(&e0)?,
                    ])
                    .map_err(|e| eng(format!("erb_dec run: {e}")))?;
                let gains = owned(&erb_dec_out, "m")?; // [1,1,clen,32]
                drop(erb_dec_out);

                let df_dec_out = nets
                    .df_dec
                    .run(ort::inputs!["emb" => to_tensor(&emb)?, "c0" => to_tensor(&c0)?])
                    .map_err(|e| eng(format!("df_dec run: {e}")))?;
                let coefs = owned(&df_dec_out, "coefs")?; // [1,clen,96,10]
                drop(df_dec_out);

                // Synthesize central frames in order so OLA state stays continuous.
                let mut chunk_out = vec![0.0f32; (ce - cs) * HOP];
                for t in cs..ce {
                    let local = t - ws;
                    let mut spec = specs[t].clone();
                    // Stage 1: ERB band gains over the full spectrum.
                    let g = &gains.data[local * NB_ERB..(local + 1) * NB_ERB];
                    state.apply_mask(&mut spec, g);
                    // Stage 2: deep filter over the noisy window `spec[t-2 .. t+2]` (df_lookahead=2).
                    for f in 0..NB_DF {
                        let mut acc = Complex32::default();
                        for k in 0..DF_ORDER {
                            let rel = t as isize + DF_LOOKAHEAD as isize + k as isize
                                - (DF_ORDER as isize - 1);
                            if rel < 0 || rel as usize >= n_frames {
                                continue;
                            }
                            let ti = rel as usize;
                            let base = (local * NB_DF + f) * (DF_ORDER * 2) + k * 2;
                            let c = Complex32::new(coefs.data[base], coefs.data[base + 1]);
                            acc += specs[ti][f] * c;
                        }
                        spec[f] = acc;
                    }
                    state.synthesis(&mut spec, &mut frame_out);
                    let off = (t - cs) * HOP;
                    chunk_out[off..off + HOP].copy_from_slice(&frame_out);
                }

                let emit_len = (ce * HOP).min(n_in) - cs * HOP;
                let block: Vec<Vec<f32>> = std::iter::repeat_with(|| chunk_out.clone())
                    .take(n_ch)
                    .collect();
                sink(&block, emit_len)?;

                cs = ce;
                window_index += 1;
            }
            drop(nets);
            Ok(())
        }
    }

    fn eng(s: String) -> crate::Error {
        crate::Error::Engine(s)
    }

    /// An owned ONNX tensor: its shape and flattened f32 data.
    struct Owned {
        shape: Vec<i64>,
        data: Vec<f32>,
    }

    /// Extract a named output into an owned tensor.
    fn owned(outputs: &ort::session::SessionOutputs, name: &str) -> Result<Owned> {
        let (shape, data) = outputs[name]
            .try_extract_tensor::<f32>()
            .map_err(|e| eng(format!("extract `{name}`: {e}")))?;
        Ok(Owned {
            shape: shape.to_vec(),
            data: data.to_vec(),
        })
    }

    /// Rebuild an ORT tensor from an owned one (clones the data — ORT takes ownership).
    fn to_tensor(o: &Owned) -> Result<Tensor<f32>> {
        Tensor::from_array((o.shape.clone(), o.data.clone()))
            .map_err(|e| eng(format!("rebuild tensor: {e}")))
    }
}
