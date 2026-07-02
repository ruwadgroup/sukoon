/* eslint-disable */
/**
 * Sukoon's comfort-noise engine ("Add noise").
 *
 * Separation leaves unnaturally *dead* spots where music used to be; a low bed of noise makes the
 * result read as a real recording again (the same trick as film room tone and codec comfort
 * noise). The engine is deliberately knob-free — everything adapts:
 *
 * - **Level is always automatic.** A minimum-statistics tracker follows the input's noise floor
 *   (the minimum short-term power over a ~6 s sliding window, so continuous speech/music can never
 *   inflate it), and the bed sits **8 dB below** that floor — room tone dresses the gaps, it never
 *   competes with the content's own noise — clamped to [-72, -48] dBFS. When the input goes truly
 *   silent (paused player, dead stream) the bed fades out entirely — noise must never outlive the
 *   content it's dressing.
 * - **"smart" color matches the content.** The floor is tracked in three bands (one-pole
 *   crossovers at ~400 Hz and ~3 kHz), and white noise is re-shaped through the same crossovers
 *   with per-band gains that reproduce the measured spectrum — the video's own room tone,
 *   approximated. "white"/"pink"/"brown" force a classic color instead, still auto-leveled.
 * - **Self-normalizing generators.** Band gains divide by the *measured* energy of the generated
 *   noise through the same filters, so color/filter choices can't change loudness.
 *
 * All smoothing is one-pole; per-sample cost is a handful of multiplies. Stereo channels use
 * independent generators (decorrelated noise reads as natural width, not a mono hiss).
 *
 * The worklet feeds raw input via `observeInput` (pre-enhancement — the floor must be measured on
 * the *original* signal) and mixes via `addTo` (post-enhancement). Modes: "off" | "smart" |
 * "white" | "pink" | "brown".
 */

const NOISE_FLOOR_MIN = 10 ** (-72 / 10); // power units (1.0 = 0 dBFS sine RMS^2-ish)
const NOISE_FLOOR_MAX = 10 ** (-48 / 10);
/** The bed sits this far below the measured floor (power factor; -8 dB). */
const BED_OFFSET = 10 ** (-8 / 10);
/** Minimum-statistics window: minima of `MIN_SUBWINDOWS` sub-windows of `SUBWINDOW_S` seconds. */
const SUBWINDOW_S = 0.5;
const MIN_SUBWINDOWS = 12;
/** Input below this (broadband, smoothed) counts as silence and gates the bed off. */
const SILENCE_GATE = 10 ** (-70 / 10);
/** How long the input must stay silent before the bed fades out (seconds). */
const SILENCE_HOLD_S = 1.0;

class NoiseChannel {
  constructor() {
    // Pink: Paul Kellet's economy filter state. Brown: leaky integrator state.
    this.b0 = 0;
    this.b1 = 0;
    this.b2 = 0;
    this.brown = 0;
    // Band-split state for shaping (smart mode) and self-measurement.
    this.lp1 = 0;
    this.lp2 = 0;
  }

  white() {
    return Math.random() * 2 - 1;
  }

  pink() {
    const w = this.white();
    this.b0 = 0.99765 * this.b0 + w * 0.099046;
    this.b1 = 0.963 * this.b1 + w * 0.2965164;
    this.b2 = 0.57 * this.b2 + w * 1.0526913;
    return (this.b0 + this.b1 + this.b2 + w * 0.1848) * 0.2;
  }

  brownNoise() {
    this.brown = 0.998 * this.brown + this.white() * 0.05;
    return this.brown * 3;
  }
}

class NoiseEngine {
  constructor(sampleRate) {
    this.sampleRate = sampleRate;
    this.mode = "off";
    this.ch = [new NoiseChannel(), new NoiseChannel()];

    // One-pole crossover coefficients (~400 Hz, ~3 kHz).
    this.k1 = 1 - Math.exp((-2 * Math.PI * 400) / sampleRate);
    this.k2 = 1 - Math.exp((-2 * Math.PI * 3000) / sampleRate);
    // Input band-split state (mono sum) + short-term band powers.
    this.inLp1 = 0;
    this.inLp2 = 0;
    this.pow = [0, 0, 0]; // smoothed band powers of the input (~50 ms)
    this.floor = [NOISE_FLOOR_MIN, NOISE_FLOOR_MIN, NOISE_FLOOR_MIN]; // per-band noise floor
    // Sliding-minimum machinery: the running minimum of the current sub-window, and a ring of the
    // last MIN_SUBWINDOWS sub-window minima. The floor is the minimum over the whole ring, so it
    // tracks the quietest recent moment and is immune to continuous loud program material.
    this.winMin = [Infinity, Infinity, Infinity];
    this.minRing = Array.from({ length: MIN_SUBWINDOWS }, () => [Infinity, Infinity, Infinity]);
    this.ringIdx = 0;
    this.winSamples = 0;
    this.noisePow = [1e-3, 1e-3, 1e-3]; // measured band powers of the *generated* noise
    this.gain = [0, 0, 0]; // smoothed per-band synthesis gains
    this.master = 0; // smoothed 0..1 fade (mode switches, silence gate)
    this.silentSec = SILENCE_HOLD_S; // start gated until real input is observed

    const at = (tau) => 1 - Math.exp(-1 / (tau * sampleRate));
    this.aPow = at(0.05); // band power smoothing
    this.aFloor = at(1.5); // floor glide toward the sliding minimum
    this.aGain = at(0.1); // gain glide
    this.aMaster = at(0.08); // master fade
  }

  setMode(mode) {
    this.mode =
      mode === "smart" || mode === "white" || mode === "pink" || mode === "brown" ? mode : "off";
  }

  /** Track the noise floor of the raw input (mono sum of up to two channels), pre-enhancement. */
  observeInput(inL, inR, n) {
    let silent = true;
    for (let i = 0; i < n; i++) {
      const x = inR ? 0.5 * (inL[i] + inR[i]) : inL[i];
      this.inLp1 += this.k1 * (x - this.inLp1);
      this.inLp2 += this.k2 * (x - this.inLp2);
      const low = this.inLp1;
      const mid = this.inLp2 - this.inLp1;
      const high = x - this.inLp2;
      this.pow[0] += this.aPow * (low * low - this.pow[0]);
      this.pow[1] += this.aPow * (mid * mid - this.pow[1]);
      this.pow[2] += this.aPow * (high * high - this.pow[2]);
    }
    // Advance the sliding minimum, then glide the floor toward it (per-sample coefficient scaled
    // by n; valid for these small taus).
    for (let b = 0; b < 3; b++) {
      if (this.pow[b] < this.winMin[b]) this.winMin[b] = this.pow[b];
    }
    this.winSamples += n;
    if (this.winSamples >= SUBWINDOW_S * this.sampleRate) {
      this.minRing[this.ringIdx] = this.winMin;
      this.ringIdx = (this.ringIdx + 1) % MIN_SUBWINDOWS;
      this.winMin = [Infinity, Infinity, Infinity];
      this.winSamples = 0;
    }
    const aFloor = Math.min(1, this.aFloor * n);
    for (let b = 0; b < 3; b++) {
      let m = this.winMin[b];
      for (const w of this.minRing) if (w[b] < m) m = w[b];
      if (!Number.isFinite(m)) m = NOISE_FLOOR_MIN;
      this.floor[b] += aFloor * (m - this.floor[b]);
      if (this.floor[b] < NOISE_FLOOR_MIN) this.floor[b] = NOISE_FLOOR_MIN;
    }
    const broadband = this.pow[0] + this.pow[1] + this.pow[2];
    if (broadband > SILENCE_GATE) silent = false;
    this.silentSec = silent ? this.silentSec + n / this.sampleRate : 0;
  }

  /** Generate one source sample of the selected color for channel `c`. */
  source(c) {
    const ch = this.ch[c];
    if (this.mode === "pink") return ch.pink();
    if (this.mode === "brown") return ch.brownNoise();
    return ch.white(); // "white" and "smart" both start from white
  }

  /**
   * Mix the bed into the (post-enhancement) output. `outR` may be the same array as `outL` for
   * mono. Call once per render quantum, after `observeInput`.
   */
  addTo(outL, outR, n) {
    const active = this.mode !== "off" && this.silentSec < SILENCE_HOLD_S;
    const targetMaster = active ? 1 : 0;
    if (!active && this.master < 1e-4) {
      this.master = 0;
      return;
    }

    // Per-band synthesis targets. "smart" reproduces the measured spectrum; fixed colors get the
    // broadband floor split evenly across bands *of that color's own spectrum* (gain equal across
    // bands = the color's natural tilt preserved, total power = floor - 8 dB).
    const total = Math.min(
      Math.max((this.floor[0] + this.floor[1] + this.floor[2]) * BED_OFFSET, NOISE_FLOOR_MIN),
      NOISE_FLOOR_MAX,
    );
    const noiseTotal = this.noisePow[0] + this.noisePow[1] + this.noisePow[2] + 1e-12;
    for (let b = 0; b < 3; b++) {
      let target;
      if (this.mode === "smart") {
        const share = this.floor[b] / (this.floor[0] + this.floor[1] + this.floor[2] + 1e-12);
        target = Math.sqrt((total * share) / (this.noisePow[b] + 1e-12));
      } else {
        target = Math.sqrt(total / noiseTotal);
      }
      if (target > 8) target = 8; // never amplify generator noise absurdly
      this.gain[b] += Math.min(1, this.aGain * n) * (target - this.gain[b]);
    }

    const stereo = outR !== outL;
    for (let i = 0; i < n; i++) {
      this.master += this.aMaster * (targetMaster - this.master);
      for (let c = 0; c < (stereo ? 2 : 1); c++) {
        const ch = this.ch[c];
        const s = this.source(c);
        ch.lp1 += this.k1 * (s - ch.lp1);
        ch.lp2 += this.k2 * (s - ch.lp2);
        const low = ch.lp1;
        const mid = ch.lp2 - ch.lp1;
        const high = s - ch.lp2;
        // Measure the generator through the same crossovers (channel 0 is representative).
        if (c === 0) {
          this.noisePow[0] += this.aPow * (low * low - this.noisePow[0]);
          this.noisePow[1] += this.aPow * (mid * mid - this.noisePow[1]);
          this.noisePow[2] += this.aPow * (high * high - this.noisePow[2]);
        }
        const bed = (low * this.gain[0] + mid * this.gain[1] + high * this.gain[2]) * this.master;
        if (c === 0) outL[i] += bed;
        else outR[i] += bed;
      }
    }
  }
}

// Test hook: the worklet bundle concatenates this file (no module system), while unit tests load
// it with `new Function(src + "; return NoiseEngine;")`.
/* c8 ignore next */
if (typeof globalThis !== "undefined") globalThis.SukoonNoiseEngine = NoiseEngine;
