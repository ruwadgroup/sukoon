/**
 * **HQ Live** — desktop-grade separation on a live page, at the cost of a few seconds of delay.
 *
 * Orchestrates the three moving parts around the `dfn-processor` worklet's delay line:
 *
 * - the worklet (mode "hq"): buffers `delaySamples`, plays DFN-cleaned audio as the always-on bed,
 *   posts raw input batches (`hq-input`) and accepts cleaned overwrites (`hq-audio`);
 * - the [`CompanionClient`]: streams the raw input to the desktop app's resident MDX engine and
 *   returns true-separator audio for the same stream positions;
 * - the [`VideoDelay`]: shows the picture equally late, slaved to the worklet's `hq-pos` reports
 *   ("now playing audio captured at context time C") translated to media time via a rolling
 *   (contextTime → video.currentTime) correlation log.
 *
 * Degradation ladder: if the companion drops mid-play, the delay line keeps playing the DFN bed
 * (same quality as Instant mode, still in sync) while we retry the connection every few seconds —
 * the user never hears a gap or a 10-second jump. Only a video-pipeline failure (WebCodecs) tears
 * HQ Live down entirely, because without the delayed picture lip sync would be seconds off.
 *
 * Stream positions are per-epoch: every seek/flush bumps the epoch, resets the worklet's counters
 * to 0 and tells the server to restart. A reconnect mid-epoch instead offsets positions by `base`
 * (the server restarts at 0; the worklet doesn't), so the ring keeps its history.
 */

import { CompanionClient } from "./companion-client.js";
import { VideoDelay } from "./video-delay.js";
import { debugEvent, registerDebugSnapshot } from "./debug.js";

/** Extra safety on top of the server's latency estimate (48 kHz samples). */
const DELAY_MARGIN_SAMPLES = 48_000;
/** Upper bound on the total delay — beyond this the UX cost outweighs the quality. */
const MAX_DELAY_SAMPLES = 15 * 48_000;
/** (contextTime, mediaTime) correlation sampling period. */
const CORRELATION_MS = 250;
/** Correlation history depth (must exceed the delay). */
const CORRELATION_ENTRIES = 120;
const RECONNECT_MS = 10_000;

interface Correlation {
  ctx: number;
  media: number;
}

export interface HqLiveOptions {
  context: AudioContext;
  node: AudioWorkletNode;
  video: HTMLMediaElement;
  token: string;
  /** The delay line is filling (true) or playing (false). */
  onBuffering(buffering: boolean): void;
  /** Unrecoverable failure (video pipeline) — the owner must fall back to Instant mode. */
  onDown(reason: string): void;
}

export class HqLive {
  private client: CompanionClient | null = null;
  private videoDelay: VideoDelay;
  private epoch = 0;
  /** Server stream position = worklet position - base (reconnects restart the server at 0). */
  private base = 0;
  private awaitingZero = false; // drop stale in-flight hq-input after a flush until position 0
  private nextInputStart = 0;
  private readonly correlations: Correlation[] = [];
  private correlationTimer = 0;
  private reconnectTimer = 0;
  private disengaged = false;
  private lastDiag: unknown = null;

  private constructor(private readonly opts: HqLiveOptions) {
    this.videoDelay = new VideoDelay(opts.video as HTMLVideoElement, (reason) => {
      this.fail(`video delay: ${reason}`);
    });
    registerDebugSnapshot("hqLive", () => ({
      disengaged: this.disengaged,
      connected: this.client !== null,
      epoch: this.epoch,
      base: this.base,
      worklet: this.lastDiag,
    }));
  }

  /** Connect, size the delay from the server's estimate, and switch the worklet to HQ mode. */
  static async engage(opts: HqLiveOptions): Promise<HqLive> {
    const hq = new HqLive(opts);
    try {
      const rt = await hq.connect();
      const delaySamples = Math.min(
        Math.max(rt.latencySamples + DELAY_MARGIN_SAMPLES, rt.blockSamples + DELAY_MARGIN_SAMPLES),
        MAX_DELAY_SAMPLES,
      );
      await hq.videoDelay.start();
      hq.videoDelay.setBuffering(true);
      opts.onBuffering(true);
      opts.node.port.addEventListener("message", hq.onWorkletMessage);
      opts.node.port.postMessage({ type: "mode", value: "hq", delaySamples });
      // Engaging against a paused element must not fill the ring with silence.
      hq.hold(opts.video.paused && !opts.video.ended);
      hq.correlationTimer = window.setInterval(() => hq.sampleCorrelation(), CORRELATION_MS);
      debugEvent("hqLive", "engaged", { delaySamples, blockSamples: rt.blockSamples }, "info");
      return hq;
    } catch (err) {
      hq.disengage();
      throw err;
    }
  }

  /** Freeze (pause) or resume the whole delayed pipeline. */
  hold(value: boolean): void {
    this.opts.node.port.postMessage({ type: "hold", value });
    this.videoDelay.hold(value);
  }

  /** Discontinuity (seek/stall/new src): drop everything and refill under a new epoch. */
  flush(): void {
    this.epoch = (this.epoch + 1) & 0xffff;
    this.base = 0;
    this.awaitingZero = true;
    this.correlations.length = 0;
    this.opts.node.port.postMessage({ type: "reset" });
    this.videoDelay.flush();
    this.videoDelay.setBuffering(true);
    this.opts.onBuffering(true);
    const client = this.client;
    if (client) {
      client.flush(this.epoch).catch(() => {
        /* onDown handles a dead client */
      });
    }
  }

  /** Return the worklet to Instant mode and release everything. Safe to call twice. */
  disengage(): void {
    if (this.disengaged) return;
    this.disengaged = true;
    if (this.correlationTimer) clearInterval(this.correlationTimer);
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.opts.node.port.removeEventListener("message", this.onWorkletMessage);
    this.opts.node.port.postMessage({ type: "mode", value: "dfn" });
    this.opts.node.port.postMessage({ type: "hold", value: false });
    this.client?.close();
    this.client = null;
    this.videoDelay.dispose();
  }

  private async connect(): Promise<{ blockSamples: number; latencySamples: number }> {
    const client = await CompanionClient.connect(this.opts.token, {
      onCleanAudio: (epoch, startSample, data) => {
        if (epoch !== this.epoch) return;
        const msg = { type: "hq-audio", startSample: startSample + this.base, data };
        this.opts.node.port.postMessage(msg, [data.buffer]);
      },
      onDown: (reason) => this.onCompanionDown(reason),
    });
    const rt = await client.startRealtime("hq");
    this.client = client;
    return rt;
  }

  /** Companion gone: keep the delayed DFN bed playing and retry in the background. */
  private onCompanionDown(reason: string): void {
    if (this.disengaged) return;
    debugEvent("hqLive", "companion-down", { reason }, "warn");
    this.client = null;
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    if (this.disengaged || this.reconnectTimer) return;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = 0;
      void this.reconnect();
    }, RECONNECT_MS);
  }

  private async reconnect(): Promise<void> {
    if (this.disengaged || this.client) return;
    try {
      await this.connect();
      // The server restarted at position 0 mid-epoch: offset everything we send from here on.
      this.base = this.nextInputStart;
      debugEvent("hqLive", "reconnected", { base: this.base }, "info");
    } catch {
      this.scheduleReconnect();
    }
  }

  private readonly onWorkletMessage = (ev: MessageEvent): void => {
    const d = ev.data as {
      type?: string;
      startSample?: number;
      data?: Float32Array;
      ctxTime?: number;
      primed?: boolean;
    };
    if (d.type === "hq-input" && d.data && typeof d.startSample === "number") {
      // After a flush, batches posted before the worklet's reset can still be in flight; the fresh
      // stream always restarts at 0 (message ordering), so drop until we see it.
      if (this.awaitingZero) {
        if (d.startSample !== 0) return;
        this.awaitingZero = false;
      }
      this.nextInputStart = d.startSample + d.data.length / 2;
      this.client?.sendAudio(this.epoch, d.startSample - this.base, d.data);
    } else if (d.type === "hq-pos" && typeof d.ctxTime === "number") {
      const media = this.mediaTimeAt(d.ctxTime);
      if (media >= 0) this.videoDelay.setTargetMediaTime(media);
    } else if (d.type === "hq-state" && typeof d.primed === "boolean") {
      this.videoDelay.setBuffering(!d.primed);
      this.opts.onBuffering(!d.primed);
    } else if (d.type === "hq-diag") {
      this.lastDiag = d;
      debugEvent("hqLive", "diag", d);
    }
  };

  private sampleCorrelation(): void {
    const ctx = this.opts.context.currentTime;
    const media = this.opts.video.currentTime;
    // Discontinuity backstop: YouTube can swap videos (SPA navigation, MSE source resets) without
    // firing the events the graph flushes on. Media time then jumps while everything else keeps
    // flowing — the picture would freeze on the old video. If the observed media time strays far
    // from the extrapolated one, treat it as a seek we never heard about.
    const last = this.correlations[this.correlations.length - 1];
    if (last && !this.opts.video.paused) {
      const rate = this.opts.video.playbackRate || 1;
      const expected = last.media + (ctx - last.ctx) * rate;
      if (Math.abs(media - expected) > 3) {
        debugEvent("hqLive", "discontinuity", { expected, media }, "warn");
        this.flush();
        this.opts.node.port.postMessage({ type: "hold", value: false });
        return; // flush cleared the log; restart sampling next tick
      }
    }
    this.correlations.push({ ctx, media });
    if (this.correlations.length > CORRELATION_ENTRIES) this.correlations.shift();
  }

  /** Map an AudioContext capture time to the element's media time via the correlation log. */
  private mediaTimeAt(ctxTime: number): number {
    const log = this.correlations;
    if (log.length === 0) return -1;
    let before: Correlation | null = null;
    let after: Correlation | null = null;
    for (const entry of log) {
      if (entry.ctx <= ctxTime) before = entry;
      else {
        after = entry;
        break;
      }
    }
    if (before && after && after.ctx > before.ctx) {
      const t = (ctxTime - before.ctx) / (after.ctx - before.ctx);
      return before.media + t * (after.media - before.media);
    }
    return (before ?? after)!.media;
  }

  private fail(reason: string): void {
    if (this.disengaged) return;
    debugEvent("hqLive", "down", { reason }, "error");
    this.disengage();
    this.opts.onDown(reason);
  }
}
