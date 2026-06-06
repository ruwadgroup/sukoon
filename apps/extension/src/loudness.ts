/**
 * Output tail shared by every tier: a **fixed** makeup gain + a true peak limiter.
 *
 * We deliberately do NOT auto-level (chase a target RMS). Offline measurement on real cleaned audio
 * showed an RMS auto-leveler swings the gain ±4–9 dB on dynamic speech/music — audible "pumping" /
 * unstable volume — because it fights the content's natural dynamics. A fixed makeup keeps the
 * model's own (stable) dynamics intact; the limiter only catches peaks so hot content (separated
 * output can exceed 0 dBFS) never hard-clips at the destination.
 */

/**
 * Fixed makeup applied to cleaned audio. Removing music lowers perceived loudness a little; a gentle
 * lift compensates without inviting the limiter to work on normal-level speech. Unity-ish on purpose.
 */
const MAKEUP_GAIN = 1.2; // ~ +1.6 dB

export interface LoudnessStage {
  /** Connect THIS to the context destination — it's the end of the cleaned signal path. */
  output: AudioNode;
  /**
   * On/off. When off (removal bypassed) the tail is fully transparent — unity gain, pass-through
   * limiter — so the original audio is untouched. When on, the makeup + limiter apply.
   */
  setActive(active: boolean): void;
  /** Release nodes. */
  dispose(): void;
}

/**
 * Build the output tail. `input` is the cleaned-signal node; the returned `output` should be
 * connected to `ctx.destination` by the caller.
 */
export function createLoudnessStage(ctx: AudioContext, input: AudioNode): LoudnessStage {
  const makeup = ctx.createGain();
  makeup.gain.value = MAKEUP_GAIN;

  // True peak safety: high threshold + fast attack so it only acts on near-clip peaks, not as a
  // leveler. With a fixed (low) makeup it rarely engages on speech, so it doesn't itself pump.
  const limiter = ctx.createDynamicsCompressor();
  limiter.threshold.value = -1;
  limiter.knee.value = 0;
  limiter.ratio.value = 20;
  limiter.attack.value = 0.003;
  limiter.release.value = 0.1;

  input.connect(makeup);
  makeup.connect(limiter);

  let active = true;
  return {
    output: limiter,
    setActive: (next: boolean) => {
      if (next === active) return;
      active = next;
      makeup.gain.setTargetAtTime(next ? MAKEUP_GAIN : 1, ctx.currentTime, 0.05);
      limiter.ratio.value = next ? 20 : 1;
    },
    dispose: () => {
      try {
        input.disconnect(makeup);
        makeup.disconnect();
        limiter.disconnect();
      } catch {
        /* already torn down */
      }
    },
  };
}
