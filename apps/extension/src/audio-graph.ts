/**
 * Web Audio routing for Sukoon's real-time engine.
 *
 * One engine: **DeepFilterNet**, run in the page's audio thread by the `dfn-processor` AudioWorklet —
 * real-time (~10 ms), in sync, on-device, no download. It taps the playing media element directly, so
 * it works on every stream (adaptive, muxed, ads) and never needs to read ahead or hold the video.
 *
 * The claimed media element routes: source → DFN worklet → loudness tail → destination. When removal
 * is off (or unavailable) the source routes straight to the destination (passthrough), so YouTube
 * audio is never lost. DFN's attenuation is capped (gentle) so its speech-enhancer objective is less
 * likely to thin melodic recitation.
 */

import { createLoudnessStage } from "./loudness.js";
import { debugEvent, registerDebugSnapshot } from "./debug.js";

/**
 * Lifecycle the UI reflects. `loading` is the engine warming up; `active` means music is being
 * reduced; `unavailable` means a failure (audio falls back to the original).
 */
export type GraphStatus = "off" | "loading" | "buffering" | "active" | "unavailable";

export type GraphStatusListener = (status: GraphStatus) => void;

type DeviceClass = "low" | "mid" | "high";

const WORKLET_URL = "worklet.js"; // registers the `dfn-processor` worklet
const WASM_URL = "dfn_wasm_bg.wasm";

/**
 * DeepFilterNet attenuation cap (dB). 0 = unlimited suppression (most aggressive, most likely to
 * thin melodic recitation); a finite value leaves some original through and is gentler on melody.
 */
const DFN_ATTEN_LIM_DB = 40;

/** A built processing chain: sources connect to `head`; it ends at the context destination. */
interface Chain {
  head: AudioNode;
  isReady(): boolean;
  setBypass(bypass: boolean): void;
  reset(): void;
  dispose(): void;
}

export class AudioGraph {
  private current: HTMLMediaElement | null = null;
  private context: AudioContext | null = null;
  private chain: Chain | null = null;
  private passthrough: GainNode | null = null;
  private device: { class: DeviceClass } | null = null;
  private deviceProbe: Promise<{ class: DeviceClass }> | null = null;
  private workletLoaded = false; // whether the worklet module is registered on `context`
  // Claimed elements must keep their source node so the chain can be re-routed.
  private readonly sources = new Map<HTMLMediaElement, MediaElementAudioSourceNode>();
  private readonly mediaCleanups = new Map<HTMLMediaElement, () => void>();
  private enabled = false;
  private ready = false;
  private preparing: { generation: number; promise: Promise<void> } | null = null;
  private buildGeneration = 0;
  private status: GraphStatus = "off";
  private gestureRetry: (() => void) | null = null;
  private readonly listeners = new Set<GraphStatusListener>();

  constructor() {
    registerDebugSnapshot("audioGraph", () => ({
      status: this.status,
      device: this.device,
      enabled: this.enabled,
      ready: this.ready,
      contextState: this.context?.state ?? "none",
      claimedSources: this.sources.size,
      current: this.current
        ? {
            paused: this.current.paused,
            currentTime: Math.round(this.current.currentTime * 1000) / 1000,
            readyState: this.current.readyState,
            src: this.current.currentSrc || this.current.src,
          }
        : null,
    }));
  }

  /** Track the current player element and wire it in if removal is enabled. */
  addMedia(media: HTMLMediaElement): void {
    this.current = media;
    this.watchMediaLifecycle(media);
    debugEvent("content", "media:selected", {
      currentTime: Math.round(media.currentTime * 1000) / 1000,
      paused: media.paused,
      src: media.currentSrc || media.src,
    });
    if (this.enabled) void this.start();
  }

  /** Subscribe to status changes. Fires immediately with the current status; returns an unsubscribe. */
  onStatus(listener: GraphStatusListener): () => void {
    this.listeners.add(listener);
    listener(this.status);
    return () => this.listeners.delete(listener);
  }

  /** Enable music removal. */
  enable(): void {
    this.enabled = true;
    debugEvent("graph", "enable", undefined, "info");
    this.chain?.setBypass(false);
    void this.start();
  }

  /** Disable music removal — keep the graph intact, just put the chain into passthrough (bypass). */
  disable(): void {
    this.enabled = false;
    debugEvent("graph", "disable", undefined, "info");
    this.disarmGestureRetry();
    this.chain?.setBypass(true);
    this.routeAllToPassthrough();
    this.emit("off");
  }

  /** Pre-load the engine without starting audio, so the first enable is instant. */
  async prewarm(): Promise<void> {
    if (this.ready && this.chain) return;
    this.ensureContext();
    try {
      await this.ensureChain();
    } catch {
      /* best-effort; the real attempt in start() reports failures */
    }
  }

  /** Tear down the graph and free the audio context. */
  detach(): void {
    debugEvent("graph", "detach", undefined, "info");
    this.invalidateBuild();
    this.chain?.dispose();
    this.chain = null;
    for (const source of this.sources.values()) {
      try {
        source.disconnect();
      } catch {
        /* ignore */
      }
    }
    this.sources.clear();
    for (const cleanup of this.mediaCleanups.values()) cleanup();
    this.mediaCleanups.clear();
    void this.context?.close();
    this.context = null;
    this.passthrough = null;
    this.ready = false;
    this.workletLoaded = false;
    this.emit("off");
  }

  /** Create the 48 kHz context expected by the worklet (matches YouTube's native rate). */
  private ensureContext(): void {
    if (this.context) return;
    this.context = new AudioContext({ sampleRate: 48_000 });
    this.workletLoaded = false; // a fresh context has no registered processors yet
  }

  /** A direct-to-speakers route for claimed media whenever the engine is off or unhealthy. */
  private ensurePassthrough(ctx: AudioContext): GainNode {
    if (this.passthrough) return this.passthrough;
    const node = ctx.createGain();
    node.gain.value = 1;
    node.connect(ctx.destination);
    this.passthrough = node;
    return node;
  }

  /** Load the worklet module once per AudioContext. */
  private async ensureWorkletModule(ctx: AudioContext): Promise<void> {
    if (this.workletLoaded) return;
    await addWorkletModule(ctx, WORKLET_URL);
    this.workletLoaded = true;
  }

  /** Probe device capability once, caching the result; used to tune the DFN priming cushion. */
  private async probeDevice(): Promise<{ class: DeviceClass }> {
    if (this.device) return this.device;
    if (this.deviceProbe) return this.deviceProbe;
    this.deviceProbe = detectDevice().then((d) => {
      this.device = d;
      debugEvent("graph", "device", d, "info");
      return d;
    });
    return this.deviceProbe;
  }

  /** Wire one element's audio into the current chain — once; keep the source node for re-routing. */
  private claim(media: HTMLMediaElement): void {
    if (!this.context) return;
    const fallback = this.ensurePassthrough(this.context);
    let source = this.sources.get(media);
    if (!source) {
      try {
        source = this.context.createMediaElementSource(media);
        this.sources.set(media, source);
      } catch (error) {
        debugEvent("graph", "media:claim-failed", { error }, "warn");
        return;
      }
    }
    this.routeSource(source, this.chain && this.enabled ? this.chain.head : fallback);
  }

  /** Bring the engine up and claim the current element. */
  private async start(): Promise<void> {
    if (!this.current) return;
    this.ensureContext();
    if (this.context && this.context.state === "suspended") {
      try {
        await this.context.resume();
      } catch {
        /* still blocked — handled below */
      }
    }
    if (!this.context || this.context.state !== "running") {
      this.armGestureRetry();
      debugEvent("graph", "context:blocked", { state: this.context?.state }, "warn");
      this.emit("off"); // nothing started yet; the element keeps playing natively
      return;
    }
    this.disarmGestureRetry();

    if (!this.ready) {
      this.routeAllToPassthrough();
      this.emit("loading");
    }
    try {
      await this.ensureChain();
    } catch (error) {
      debugEvent("graph", "engine:start-failed", { error }, "error");
      this.failOpen();
      return;
    }

    this.claim(this.current);
    if (this.chain) this.chain.setBypass(!this.enabled);
    if (this.enabled && this.chain) this.routeAllTo(this.chain.head);
    else this.routeAllToPassthrough();
    if (!this.enabled) this.emit("off");
    else this.emit(this.chain?.isReady() ? "active" : "buffering");
  }

  /** Flush latency buffers on player discontinuities so resume/seek never drains stale audio. */
  private watchMediaLifecycle(media: HTMLMediaElement): void {
    if (this.mediaCleanups.has(media)) return;
    const reset = (event: Event) => {
      this.chain?.reset();
      debugEvent("media", event.type, {
        currentTime: Math.round(media.currentTime * 1000) / 1000,
      });
    };
    const restart = () => {
      debugEvent("media", "playing", { currentTime: Math.round(media.currentTime * 1000) / 1000 });
      if (this.enabled) void this.start();
    };
    media.addEventListener("pause", reset);
    media.addEventListener("seeking", reset);
    media.addEventListener("seeked", reset);
    media.addEventListener("stalled", reset);
    media.addEventListener("emptied", reset);
    media.addEventListener("playing", restart);
    this.mediaCleanups.set(media, () => {
      media.removeEventListener("pause", reset);
      media.removeEventListener("seeking", reset);
      media.removeEventListener("seeked", reset);
      media.removeEventListener("stalled", reset);
      media.removeEventListener("emptied", reset);
      media.removeEventListener("playing", restart);
    });
  }

  /** Attach every claimed source to a single target; never leave a claimed media node disconnected. */
  private routeAllTo(target: AudioNode): void {
    for (const source of this.sources.values()) this.routeSource(source, target);
  }

  private routeAllToPassthrough(): void {
    if (!this.context || this.sources.size === 0) return;
    this.routeAllTo(this.ensurePassthrough(this.context));
  }

  private routeSource(source: MediaElementAudioSourceNode, target: AudioNode): void {
    try {
      source.disconnect();
    } catch {
      /* already disconnected */
    }
    try {
      source.connect(target);
    } catch (error) {
      debugEvent("graph", "media:route-failed", { error }, "warn");
    }
  }

  /** Arm a one-time retry on the user's next interaction — used when autoplay blocked the context. */
  private armGestureRetry(): void {
    if (this.gestureRetry) return;
    const retry = () => {
      this.disarmGestureRetry();
      if (this.enabled) void this.start();
    };
    this.gestureRetry = retry;
    document.addEventListener("pointerdown", retry, { capture: true, once: true });
    document.addEventListener("keydown", retry, { capture: true, once: true });
    this.current?.addEventListener("playing", retry, { once: true });
  }

  private disarmGestureRetry(): void {
    const retry = this.gestureRetry;
    if (!retry) return;
    this.gestureRetry = null;
    document.removeEventListener("pointerdown", retry, true);
    document.removeEventListener("keydown", retry, true);
    this.current?.removeEventListener("playing", retry);
  }

  /** Build the DeepFilterNet chain (once), connecting it to the destination. */
  private async ensureChain(): Promise<void> {
    if (this.ready && this.chain) return;
    if (this.preparing) return this.preparing.promise;
    const generation = ++this.buildGeneration;
    const promise = (async () => {
      if (!this.context) return;
      await this.probeDevice();
      if (!this.isBuildCurrent(generation)) return;
      const chain = await this.buildDfnChain(this.context);
      if (!this.isBuildCurrent(generation)) {
        chain.dispose();
        return;
      }
      chain.setBypass(!this.enabled);
      this.chain = chain;
      this.ready = true;
    })();
    this.preparing = { generation, promise };
    try {
      await promise;
    } finally {
      if (this.preparing?.generation === generation) this.preparing = null;
    }
  }

  /** Build the inline DeepFilterNet worklet — real-time, in sync. */
  private async buildDfnChain(ctx: AudioContext): Promise<Chain> {
    await this.ensureWorkletModule(ctx);
    const module = await compileWasm(runtimeUrl(WASM_URL));
    const primeMs = this.device?.class === "low" ? 240 : 120;
    const node = new AudioWorkletNode(ctx, "dfn-processor", {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      outputChannelCount: [2],
      processorOptions: { module, attenLimDb: DFN_ATTEN_LIM_DB, primeMs },
    });
    await waitForReady(node.port);
    const loudness = createLoudnessStage(ctx, node);
    loudness.output.connect(ctx.destination);
    return {
      head: node,
      isReady: () => true,
      setBypass: (bypass) => {
        node.port.postMessage({ type: "bypass", value: bypass });
        loudness.setActive(!bypass);
      },
      reset: () => node.port.postMessage({ type: "reset" }),
      dispose: () => {
        try {
          node.disconnect();
        } catch {
          /* ignore */
        }
        loudness.dispose();
      },
    };
  }

  private emit(status: GraphStatus): void {
    if (status === this.status) return;
    debugEvent("graph", "status", { from: this.status, to: status }, "info");
    this.status = status;
    for (const listener of this.listeners) listener(status);
  }

  /** On any engine/runtime failure, preserve YouTube audio by routing claimed media straight out. */
  private failOpen(): void {
    this.invalidateBuild();
    const failed = this.chain;
    this.chain = null;
    this.ready = false;
    failed?.setBypass(true);
    this.routeAllToPassthrough();
    failed?.dispose();
    this.emit("unavailable");
  }

  private invalidateBuild(): void {
    this.buildGeneration++;
    this.preparing = null;
  }

  private isBuildCurrent(generation: number): boolean {
    return generation === this.buildGeneration;
  }
}

/** Probe device capability: memory + cores + mobile hint → a coarse class (tunes the DFN cushion). */
async function detectDevice(): Promise<{ class: DeviceClass }> {
  const nav = navigator as Navigator & {
    deviceMemory?: number;
    userAgentData?: { mobile?: boolean };
  };
  const mem = nav.deviceMemory ?? 4;
  const cores = navigator.hardwareConcurrency ?? 4;
  const mobile = nav.userAgentData?.mobile ?? /Mobi|Android/i.test(navigator.userAgent);
  const cls: DeviceClass =
    mobile || mem <= 2 || cores <= 2 ? "low" : mem >= 8 && cores >= 8 ? "high" : "mid";
  return { class: cls };
}

/** Resolve a dist-relative asset to its extension URL (content-script context). */
function runtimeUrl(path: string): string {
  return chrome.runtime.getURL(path);
}

/** Load an AudioWorklet module by dist filename, falling back to a blob: URL if a CSP blocks it. */
async function addWorkletModule(ctx: AudioContext, file: string): Promise<void> {
  try {
    await ctx.audioWorklet.addModule(runtimeUrl(file));
  } catch {
    await ctx.audioWorklet.addModule(await blobUrl(runtimeUrl(file)));
  }
}

/** Resolve once the worklet posts `{type:"ready"}` (or errors). Times out rather than hanging. */
function waitForReady(port: MessagePort): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => resolve(), 8_000);
    port.onmessage = (ev: MessageEvent) => {
      const data = ev.data as { type?: string; message?: string };
      if (data.type === "ready") {
        clearTimeout(timeout);
        resolve();
      } else if (data.type === "error") {
        clearTimeout(timeout);
        reject(new Error(data.message ?? "worklet init failed"));
      }
    };
  });
}

/** Fetch a same-extension script and wrap it in a blob: URL (CSP-friendly for worklet loading). */
async function blobUrl(url: string): Promise<string> {
  const text = await (await fetch(url)).text();
  return URL.createObjectURL(new Blob([text], { type: "text/javascript" }));
}

/** Compile a WASM module, preferring streaming and falling back to a buffered compile. */
async function compileWasm(url: string): Promise<WebAssembly.Module> {
  try {
    return await WebAssembly.compileStreaming(fetch(url));
  } catch {
    const bytes = await (await fetch(url)).arrayBuffer();
    return WebAssembly.compile(bytes);
  }
}
