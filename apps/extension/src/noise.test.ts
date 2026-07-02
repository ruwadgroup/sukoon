// Unit tests for the worklet comfort-noise engine (worklet/noise.js). The file is a plain script
// (the worklet bundle is concatenated, no module system), so load it via `new Function`.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const src = readFileSync(resolve(__dirname, "../worklet/noise.js"), "utf8");
// eslint-disable-next-line @typescript-eslint/no-implied-eval
const NoiseEngine = new Function(`${src}; return NoiseEngine;`)() as new (sampleRate: number) => {
  setMode(mode: string): void;
  observeInput(l: Float32Array, r: Float32Array | null, n: number): void;
  addTo(l: Float32Array, r: Float32Array, n: number): void;
};

const SR = 48_000;
const BLOCK = 128;

/** Drive the engine for `seconds` with the given input generator; returns collected output. */
function run(
  engine: InstanceType<typeof NoiseEngine>,
  seconds: number,
  inputSample: () => number,
): Float32Array {
  const blocks = Math.round((seconds * SR) / BLOCK);
  const out = new Float32Array(blocks * BLOCK);
  const inL = new Float32Array(BLOCK);
  const outL = new Float32Array(BLOCK);
  const outR = new Float32Array(BLOCK);
  for (let b = 0; b < blocks; b++) {
    for (let i = 0; i < BLOCK; i++) inL[i] = inputSample();
    outL.fill(0);
    outR.fill(0);
    engine.observeInput(inL, null, BLOCK);
    engine.addTo(outL, outR, BLOCK);
    out.set(outL, b * BLOCK);
  }
  return out;
}

function rmsDb(x: Float32Array, from = 0): number {
  let sum = 0;
  let n = 0;
  for (let i = from; i < x.length; i++) {
    sum += x[i]! * x[i]!;
    n++;
  }
  return 10 * Math.log10(sum / Math.max(1, n) + 1e-20);
}

/** A steady input at `db` dBFS-ish (uniform noise scaled to that RMS). */
function noisyInput(db: number): () => number {
  const amp = Math.sqrt(10 ** (db / 10)) / 0.577; // uniform [-1,1] has RMS ~0.577
  return () => (Math.random() * 2 - 1) * amp;
}

describe("NoiseEngine", () => {
  it("adds nothing when off", () => {
    const e = new NoiseEngine(SR);
    const out = run(e, 2, noisyInput(-50));
    expect(rmsDb(out)).toBeLessThan(-90);
  });

  it("levels the bed ~8 dB under the input's noise floor", () => {
    const e = new NoiseEngine(SR);
    e.setMode("smart");
    run(e, 10, noisyInput(-55)); // converge (sliding-min window is ~6 s)
    const out = run(e, 4, noisyInput(-55));
    const level = rmsDb(out, out.length / 2);
    expect(level).toBeGreaterThan(-74);
    expect(level).toBeLessThan(-57);
  });

  it("never exceeds the level cap on loud content", () => {
    const e = new NoiseEngine(SR);
    e.setMode("white");
    run(e, 10, noisyInput(-10));
    const out = run(e, 4, noisyInput(-10));
    expect(rmsDb(out, out.length / 2)).toBeLessThan(-45);
  });

  it("gates off on silent input", () => {
    const e = new NoiseEngine(SR);
    e.setMode("smart");
    run(e, 6, noisyInput(-50)); // active
    const silent = run(e, 4, () => 0);
    const tail = rmsDb(silent, Math.round(silent.length * 0.75));
    expect(tail).toBeLessThan(-80);
  });

  it("pink is darker than white (more low-band energy share)", () => {
    const tilt = (mode: string): number => {
      const e = new NoiseEngine(SR);
      e.setMode(mode);
      run(e, 8, noisyInput(-50));
      const out = run(e, 6, noisyInput(-50));
      // One-pole split at ~1 kHz: compare low vs high energy.
      const k = 1 - Math.exp((-2 * Math.PI * 1000) / SR);
      let lp = 0;
      let lowE = 0;
      let highE = 0;
      for (let i = out.length / 2; i < out.length; i++) {
        lp += k * (out[i]! - lp);
        lowE += lp * lp;
        highE += (out[i]! - lp) ** 2;
      }
      return lowE / (highE + 1e-12);
    };
    expect(tilt("pink")).toBeGreaterThan(tilt("white") * 2);
  });
});
