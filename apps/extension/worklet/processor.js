/* eslint-disable */
/* global wasm_bindgen, registerProcessor, AudioWorkletProcessor, sampleRate, currentTime, NoiseEngine */
/**
 * Sukoon DFN AudioWorklet processor — the real-time engine, plus the **HQ Live** delay line.
 *
 * Runs the DeepFilterNet WASM engine (the global `wasm_bindgen` defined by the no-modules glue that
 * is concatenated ABOVE this file at build time — see scripts/bundle-worklet.mjs). The compiled
 * `WebAssembly.Module` is handed in via `processorOptions.module` (AudioWorklets can't fetch), and
 * `initSync` instantiates it synchronously inside the audio thread.
 *
 * Audio contract: the engine works on 48 kHz in fixed `hop` (480-sample) frames, while the worklet
 * is called in 128-sample render quanta. `processorOptions.channels` picks mono (down-mix, low-end
 * devices) or stereo (per-channel enhancement) DFN processing.
 *
 * ## Modes
 *
 * **"dfn" (Instant, default)** — accumulate input into hop frames, enhance each, stream the cleaned
 * samples straight back out. Steady-state added latency is ~one hop (~10 ms) plus the model's small
 * lookahead, so audio stays in sync with the live video.
 *
 * **"hq" (HQ Live)** — a `delaySamples` ring delay so a non-causal separator can work with real
 * lookahead. Input is (a) enhanced by DFN into a **bed ring** (the always-available fallback), and
 * (b) posted to the content script (`hq-input`), which streams it to the desktop companion's MDX
 * engine. Cleaned stereo comes back (`hq-audio`) and lands in an **hq ring** at the same stream
 * positions. Playout reads `delaySamples` behind the write head and blends hq-over-bed with a
 * ~10 ms smoothed gain, so a missing/late HQ block degrades to DFN quality instead of clicking or
 * going silent. The content script delays the video by the same amount and slaves it to the
 * `hq-pos` reports (the capture context-time of the audio now playing).
 *
 * `hold` freezes the delay line (both heads) across media pauses so buffered content neither drains
 * nor drifts; `reset` clears it (seeks/flushes). While the ring is filling the output is silence —
 * the content script shows this as "buffering" with the delayed video frozen.
 *
 * **Priming buffer (`primeMs`)** (dfn mode): a small cleaned-audio cushion held before draining so a
 * scheduling/GC hiccup eats into the cushion instead of clicking. On an underrun we re-prime.
 *
 * **`attenLimDb`** caps how much music DFN may attenuate. 0 = unlimited suppression (most aggressive,
 * and the most likely to thin melodic recitation); a finite value leaves some original through, which
 * is gentler on melody — see packages/dfn-wasm/src/lib.rs.
 *
 * The host AudioContext is created at 48 kHz so `sampleRate` here is already 48000 (no resampling).
 */

/** Interleaved input hops posted to the companion per message (~240 ms batches). */
const SEND_BATCH_HOPS = 24;
/** hq-pos report cadence in played hops (~120 ms). */
const POS_REPORT_HOPS = 12;
/** One-pole time constant for the bed↔hq blend gain (~10 ms at 48 kHz). */
const BLEND_COEF = Math.exp(-1 / (0.01 * 48000));
/** hq-diag cadence in process() calls (128-sample quanta → ~1 s at 48 kHz). */
const DIAG_CALLS = 375;

class DfnProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.ready = false;
    this.bypass = false;
    this.mode = "dfn";
    this.held = false;
    this.hop = 480;
    this.diagCountdown = DIAG_CALLS;
    // Comfort-noise bed over the enhanced output ("Add noise") — see worklet/noise.js.
    this.noise = new NoiseEngine(sampleRate);
    this.adShowing = false;

    const opts = options.processorOptions || {};
    this.channels = opts.channels === 2 ? 2 : 1;
    // How much cleaned audio to queue before draining in dfn mode (the priming cushion).
    const primeMs = typeof opts.primeMs === "number" ? opts.primeMs : 0;
    try {
      wasm_bindgen.initSync({ module: opts.module });
      this.denoiser = new wasm_bindgen.DfnDenoiser(
        typeof opts.attenLimDb === "number" ? opts.attenLimDb : 40,
        this.channels,
      );
      this.hop = this.denoiser.frame_length;
      // dfn-mode buffers: planar accumulator ([ch0 hop..., ch1 hop...]) and cleaned-frame queue.
      this.acc = new Float32Array(this.channels * this.hop);
      this.accLen = 0;
      this.outQueue = [];
      this.outOffset = 0;
      this.primeFrames = Math.ceil(((primeMs / 1000) * sampleRate) / this.hop);
      this.primed = this.primeFrames <= 0;
      this.hq = null; // allocated on first mode switch
      this.ready = true;
      this.port.postMessage({ type: "ready", hop: this.hop, sampleRate });
    } catch (e) {
      this.port.postMessage({ type: "error", message: String((e && e.message) || e) });
    }

    this.port.onmessage = (ev) => {
      const d = ev.data || {};
      if (d.type === "bypass") this.bypass = !!d.value;
      else if (d.type === "reset") this.resetBuffers();
      else if (d.type === "hold") this.held = !!d.value;
      else if (d.type === "noise") this.noise.setMode(d.value);
      else if (d.type === "ad") this.adShowing = !!d.value;
      else if (d.type === "mode") this.setMode(d.value, d.delaySamples);
      else if (d.type === "hq-audio" && this.hq) this.applyHqAudio(d.startSample, d.data);
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
    if (this.hq) this.resetHq();
  }

  setMode(mode, delaySamples) {
    if (mode === "hq") {
      const delayHops = Math.max(1, Math.ceil((delaySamples || 0) / this.hop));
      if (!this.hq || this.hq.delayHops !== delayHops) this.allocHq(delayHops);
      this.mode = "hq";
      this.resetBuffers();
    } else {
      this.mode = "dfn";
      this.resetBuffers();
    }
  }

  allocHq(delayHops) {
    const ringHops = delayHops + 8; // slack for blocks landing right at the write head
    const len = ringHops * this.hop;
    this.hq = {
      delayHops,
      ringHops,
      delaySamples: delayHops * this.hop,
      bedL: new Float32Array(len),
      bedR: new Float32Array(len),
      hqL: new Float32Array(len),
      hqR: new Float32Array(len),
      // Per-hop count of cleaned samples landed; a hop is HQ-valid once fully covered.
      hqFill: new Uint16Array(ringHops),
      writeCtx: new Float64Array(ringHops), // capture context-time per hop, for video slaving
      inAccL: new Float32Array(this.hop),
      inAccR: new Float32Array(this.hop),
      accLen: 0,
      writeHops: 0, // whole hops written since reset
      playedSamples: 0, // samples emitted from the delay line since reset
      blend: 0, // smoothed 0=bed, 1=hq
      sendBuf: new Float32Array(SEND_BATCH_HOPS * this.hop * 2),
      sendLen: 0, // interleaved samples in sendBuf
      sendStart: 0, // stream position (per-channel samples) of sendBuf[0]
      hqPrimed: false,
      posCountdown: POS_REPORT_HOPS,
    };
  }

  resetHq() {
    const h = this.hq;
    h.hqFill.fill(0);
    h.accLen = 0;
    h.writeHops = 0;
    h.playedSamples = 0;
    h.blend = 0;
    h.sendLen = 0;
    h.sendStart = 0;
    h.posCountdown = POS_REPORT_HOPS;
    if (h.hqPrimed) {
      h.hqPrimed = false;
      this.port.postMessage({ type: "hq-state", primed: false });
    }
  }

  /** Land cleaned interleaved stereo at absolute stream position `startSample`. */
  applyHqAudio(startSample, data) {
    const h = this.hq;
    const n = data.length >> 1;
    const writeSamples = h.writeHops * this.hop;
    for (let i = 0; i < n; i++) {
      const p = startSample + i;
      // Only positions not yet played and already captured can land (late/early samples drop).
      if (p < h.playedSamples || p >= writeSamples) continue;
      const hopIdx = Math.floor(p / this.hop);
      // Stale ring generation guard: the slot must still hold this stream position's hop.
      if (hopIdx < h.writeHops - h.ringHops) continue;
      const slot = hopIdx % h.ringHops;
      const off = p % this.hop;
      const base = slot * this.hop + off;
      h.hqL[base] = data[i * 2];
      h.hqR[base] = data[i * 2 + 1];
      h.hqFill[slot]++;
    }
  }

  process(inputs, outputs) {
    const input = inputs[0];
    const output = outputs[0];
    if (!output || output.length === 0) return true;
    const n = output[0].length;

    // A once-a-second state report while in HQ mode, so stalls are diagnosable from the page.
    if (this.mode === "hq" && this.hq && --this.diagCountdown <= 0) {
      this.diagCountdown = DIAG_CALLS;
      const h = this.hq;
      this.port.postMessage({
        type: "hq-diag",
        held: this.held,
        bypass: this.bypass,
        writeHops: h.writeHops,
        playedSamples: h.playedSamples,
        primed: h.hqPrimed,
        blend: Math.round(h.blend * 100) / 100,
      });
    }

    if (this.held) {
      for (let c = 0; c < output.length; c++) output[c].fill(0);
      return true;
    }

    // Passthrough when not ready, bypassed (ads/disabled), or no input: copy input straight to output.
    if (!this.ready || this.bypass || !input || input.length === 0) {
      for (let c = 0; c < output.length; c++) {
        const inC = input && (input[c] || input[0]);
        if (inC) output[c].set(inC);
        else output[c].fill(0);
      }
      return true;
    }

    if (this.mode === "hq") this.processHq(input, output, n);
    else this.processDfn(input, output, n);

    // The bed dresses only *processed* audio (never bypass/passthrough, which returned above).
    // During ads it freezes outright: no bed (ads aren't cleaned content to dress) and no floor
    // observation (a loud ad would drag the tracked floor - and thus the bed - up for the content
    // that follows).
    if (!this.adShowing) {
      this.noise.observeInput(input[0], input[1] || null, n);
      this.noise.addTo(output[0], output[1] || output[0], n);
    }
    return true;
  }

  /** Instant mode: enhance hop frames in-line, ~one hop of latency. */
  processDfn(input, output, n) {
    const ch = input.length;
    const stereo = this.channels === 2 && ch > 1;
    for (let i = 0; i < n; i++) {
      let m = 0;
      if (stereo) {
        this.acc[this.accLen] = input[0][i];
        this.acc[this.hop + this.accLen] = input[1][i];
        m = 0.5 * (input[0][i] + input[1][i]);
      } else {
        for (let c = 0; c < ch; c++) m += input[c][i];
        if (ch > 1) m /= ch;
        this.acc[this.accLen] = m;
      }
      this.accLen++;
      if (this.accLen === this.hop) {
        this.outQueue.push(this.denoiser.process_frame(this.acc));
        this.accLen = 0;
      }

      // Become primed once the cushion is filled. If processing has not produced a frame yet, pass
      // the current dry sample through instead of silence; once a media element is claimed, silence
      // here would remove the page's whole audio track during resume/re-prime gaps.
      if (!this.primed && this.outQueue.length >= this.primeFrames) this.primed = true;
      let outL = m;
      let outR = m;
      if (this.primed && this.outQueue.length > 0) {
        const frame = this.outQueue[0];
        outL = frame[this.outOffset];
        outR = stereo ? frame[this.hop + this.outOffset] : outL;
        this.outOffset++;
        if (this.outOffset >= this.hop) {
          this.outQueue.shift();
          this.outOffset = 0;
        }
        if (this.outQueue.length === 0) this.primed = this.primeFrames <= 0;
      }
      for (let c = 0; c < output.length; c++) output[c][i] = c === 1 ? outR : outL;
    }
  }

  /** HQ Live mode: write DFN bed + capture input into the delay line; play `delaySamples` behind. */
  processHq(input, output, n) {
    const h = this.hq;
    const hop = this.hop;
    const inL = input[0];
    const inR = input[1] || input[0];
    const stereo = this.channels === 2;

    for (let i = 0; i < n; i++) {
      // --- write side: accumulate one hop of raw input ---
      h.inAccL[h.accLen] = inL[i];
      h.inAccR[h.accLen] = inR[i];
      h.accLen++;
      if (h.accLen === hop) {
        // DFN bed for this hop (the fallback layer under HQ audio).
        if (stereo) {
          this.acc.set(h.inAccL, 0);
          this.acc.set(h.inAccR, hop);
        } else {
          for (let j = 0; j < hop; j++) this.acc[j] = 0.5 * (h.inAccL[j] + h.inAccR[j]);
        }
        const bed = this.denoiser.process_frame(this.acc);
        const slot = h.writeHops % h.ringHops;
        const base = slot * hop;
        for (let j = 0; j < hop; j++) {
          h.bedL[base + j] = bed[j];
          h.bedR[base + j] = stereo ? bed[hop + j] : bed[j];
        }
        h.hqFill[slot] = 0;
        h.hqL.fill(0, base, base + hop);
        h.hqR.fill(0, base, base + hop);
        h.writeCtx[slot] = currentTime;

        // Raw input (interleaved stereo) queued for the companion.
        for (let j = 0; j < hop; j++) {
          h.sendBuf[h.sendLen++] = h.inAccL[j];
          h.sendBuf[h.sendLen++] = h.inAccR[j];
        }
        h.writeHops++;
        h.accLen = 0;
        if (h.sendLen === h.sendBuf.length) this.flushSend();
      }

      // --- read side: play exactly delaySamples behind the write head ---
      const captured = h.writeHops * hop + h.accLen;
      const p = captured - h.delaySamples;
      if (p < 0 || p < h.playedSamples) {
        // Ring still filling (or freshly reset): buffering silence.
        for (let c = 0; c < output.length; c++) output[c][i] = 0;
        continue;
      }
      if (!h.hqPrimed) {
        h.hqPrimed = true;
        this.port.postMessage({ type: "hq-state", primed: true });
      }
      const hopIdx = Math.floor(p / hop);
      const slot = hopIdx % h.ringHops;
      const off = p % hop;
      const base = slot * hop + off;
      const target = h.hqFill[slot] >= hop ? 1 : 0;
      h.blend = target + (h.blend - target) * BLEND_COEF;
      const g = h.blend;
      const outL = h.bedL[base] * (1 - g) + h.hqL[base] * g;
      const outR = h.bedR[base] * (1 - g) + h.hqR[base] * g;
      for (let c = 0; c < output.length; c++) output[c][i] = c === 1 ? outR : outL;
      h.playedSamples = p + 1;

      if (off === 0 && --h.posCountdown <= 0) {
        h.posCountdown = POS_REPORT_HOPS;
        this.port.postMessage({ type: "hq-pos", ctxTime: h.writeCtx[slot] });
      }
    }
  }

  /** Post the queued raw-input batch to the content script (transferable, zero copy). */
  flushSend() {
    const h = this.hq;
    if (h.sendLen === 0) return;
    const data = h.sendBuf.slice(0, h.sendLen);
    this.port.postMessage({ type: "hq-input", startSample: h.sendStart, data }, [data.buffer]);
    h.sendStart += h.sendLen >> 1;
    h.sendLen = 0;
  }
}

registerProcessor("dfn-processor", DfnProcessor);
