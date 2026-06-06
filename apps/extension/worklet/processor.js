/* eslint-disable */
/* global wasm_bindgen, registerProcessor, AudioWorkletProcessor, sampleRate */
/**
 * Sukoon **Instant** DFN AudioWorklet processor — the real-time, in-sync engine.
 *
 * Runs the DeepFilterNet WASM engine (the global `wasm_bindgen` defined by the no-modules glue that
 * is concatenated ABOVE this file at build time — see scripts/bundle-worklet.mjs). The compiled
 * `WebAssembly.Module` is handed in via `processorOptions.module` (AudioWorklets can't fetch), and
 * `initSync` instantiates it synchronously inside the audio thread.
 *
 * Audio contract: the engine works on 48 kHz mono in fixed `hop` (480-sample) frames, while the
 * worklet is called in 128-sample render quanta. We down-mix to mono, accumulate into hop frames,
 * enhance each, and stream the cleaned samples back out — replicated across the output channels.
 * Steady-state added latency is ~one hop (~10 ms) plus the model's small lookahead. This is the
 * **only** engine that keeps YouTube audio and video in sync, because it runs frame-by-frame in the
 * audio thread with no round-trip and no multi-second analysis window.
 *
 * **Priming buffer (`primeMs`)**: a small cleaned-audio cushion held before draining. 0 drains the
 * instant the first frame is ready (lowest latency, but a scheduling/GC hiccup can momentarily starve
 * the queue). A small value (~200 ms) holds output until that much is queued, then drains, so a
 * transient stall eats into the cushion instead of clicking. On an underrun (queue emptied) we
 * re-prime. The cost is a constant added latency equal to the prime depth.
 *
 * **`attenLimDb`** caps how much music DFN may attenuate. 0 = unlimited suppression (most aggressive,
 * and the most likely to thin melodic recitation); a finite value leaves some original through, which
 * is gentler on melody — see packages/dfn-wasm/src/lib.rs.
 *
 * The host AudioContext is created at 48 kHz so `sampleRate` here is already 48000 (no resampling).
 */
class DfnProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.ready = false;
    this.bypass = false;
    this.hop = 480;
    this.acc = new Float32Array(this.hop);
    this.accLen = 0;
    this.outQueue = []; // queued cleaned frames (Float32Array(hop))
    this.outOffset = 0;

    const opts = options.processorOptions || {};
    // How much cleaned audio to queue before draining (the priming cushion). 0 = drain immediately.
    const primeMs = typeof opts.primeMs === "number" ? opts.primeMs : 0;
    try {
      wasm_bindgen.initSync({ module: opts.module });
      this.denoiser = new wasm_bindgen.DfnDenoiser(
        typeof opts.attenLimDb === "number" ? opts.attenLimDb : 40,
      );
      this.hop = this.denoiser.frame_length;
      this.acc = new Float32Array(this.hop);
      // Prime depth in whole hop-frames. primeMs=0 → 0 frames → drains as soon as one is ready.
      this.primeFrames = Math.ceil(((primeMs / 1000) * sampleRate) / this.hop);
      this.primed = this.primeFrames <= 0;
      this.ready = true;
      this.port.postMessage({ type: "ready", hop: this.hop, sampleRate });
    } catch (e) {
      this.port.postMessage({ type: "error", message: String((e && e.message) || e) });
    }

    this.port.onmessage = (ev) => {
      const d = ev.data || {};
      if (d.type === "bypass") this.bypass = !!d.value;
      else if (d.type === "reset") this.resetBuffers();
      else if (d.type === "atten" && this.denoiser && typeof d.value === "number") {
        try {
          this.denoiser.set_atten_lim(d.value);
        } catch {
          /* ignore */
        }
      }
    };
  }

  resetBuffers() {
    this.accLen = 0;
    this.outQueue = [];
    this.outOffset = 0;
    this.primed = this.primeFrames <= 0;
  }

  process(inputs, outputs) {
    const input = inputs[0];
    const output = outputs[0];
    if (!output || output.length === 0) return true;
    const n = output[0].length;

    // Passthrough when not ready, bypassed (ads/disabled), or no input: copy input straight to output.
    if (!this.ready || this.bypass || !input || input.length === 0) {
      for (let c = 0; c < output.length; c++) {
        const inC = input && (input[c] || input[0]);
        if (inC) output[c].set(inC);
        else output[c].fill(0);
      }
      return true;
    }

    const ch = input.length;
    for (let i = 0; i < n; i++) {
      // Down-mix this sample to mono.
      let m = 0;
      for (let c = 0; c < ch; c++) m += input[c][i];
      if (ch > 1) m /= ch;

      // Accumulate into a hop frame; enhance when full.
      this.acc[this.accLen++] = m;
      if (this.accLen === this.hop) {
        this.outQueue.push(this.denoiser.process_frame(this.acc));
        this.accLen = 0;
      }

      // Become primed once the cushion is filled. If processing has not produced a frame yet, pass
      // the current dry sample through instead of silence; once a media element is claimed, silence
      // here would remove the page's whole audio track during resume/re-prime gaps.
      if (!this.primed && this.outQueue.length >= this.primeFrames) this.primed = true;
      let out = m;
      if (this.primed && this.outQueue.length > 0) {
        out = this.outQueue[0][this.outOffset++];
        if (this.outOffset >= this.hop) {
          this.outQueue.shift();
          this.outOffset = 0;
        }
        if (this.outQueue.length === 0) this.primed = this.primeFrames <= 0;
      }
      for (let c = 0; c < output.length; c++) output[c][i] = out;
    }
    return true;
  }
}

registerProcessor("dfn-processor", DfnProcessor);
