/**
 * A synchronized **video delay line** for HQ Live mode.
 *
 * HQ Live gives the audio separator seconds of real lookahead by playing audio `D` seconds behind
 * the media element. Lip sync then requires the *picture* to run `D` late too — this module does
 * that without touching the element's playback: it captures the element's frames as they render
 * (`requestVideoFrameCallback` + `VideoFrame`), passes them through a hardware
 * `VideoEncoder` → chunk ring → `VideoDecoder` (raw frames for ~10 s would exhaust GPU frame pools;
 * re-encoded they are a few MB), and draws the delayed frames onto a canvas overlaid on the
 * (visually hidden) element. Everything stays local to the page: no MSE tap, no network, no
 * service-worker lifecycle.
 *
 * Timing is slaved to the audio: the owner feeds `setTargetMediaTime(m)` (derived from the delay
 * line's "now playing audio captured at media time m" reports) and the renderer presents the newest
 * buffered frame at or before `m`, extrapolating between updates at the playback rate. Keying on
 * media time (frames carry `metadata.mediaTime`) makes pauses freeze naturally and rules out
 * cumulative drift.
 *
 * Resolution changes (YouTube quality switches) re-configure the encoder mid-stream; chunks are
 * version-tagged so the decoder flushes and re-configures exactly at the boundary. Any capture or
 * codec failure calls `onError` so the owner can fall back to the instant engine — never a black
 * player.
 */

import { debugEvent } from "./debug.js";

/** Feed the decoder this far ahead of the presentation target (seconds). */
const DECODE_LOOKAHEAD_S = 0.3;
/** Force a keyframe at least this often (frames), bounding post-flush refill cost. */
const KEYFRAME_INTERVAL = 60;
/** Hard cap on buffered encoded video; exceeding it means playout stalled — reset instead of OOM. */
const MAX_QUEUE_S = 40;
/** Hidden tabs stop `requestVideoFrameCallback`; a 1 Hz timer keeps a coarse frame supply. */
const HIDDEN_CAPTURE_MS = 1000;

interface QueuedChunk {
  chunk: EncodedVideoChunk;
  version: number;
  tsSec: number;
}

export class VideoDelay {
  private readonly video: HTMLVideoElement;
  private canvas: HTMLCanvasElement | null = null;
  private ctx2d: CanvasRenderingContext2D | null = null;
  private spinner: HTMLElement | null = null;
  private resize: ResizeObserver | null = null;

  private encoder: VideoEncoder | null = null;
  private decoder: VideoDecoder | null = null;
  private codec: string | null = null;
  private version = 0; // bumped on reconfigure/flush; tags chunks for the decoder side
  private decoderVersion = -1;
  private decoderBusy = false; // a flush/reconfigure is in flight
  private encWidth = 0;
  private encHeight = 0;
  private framesSinceKey = KEYFRAME_INTERVAL; // first encode is a keyframe

  private queue: QueuedChunk[] = [];
  private decoded: VideoFrame[] = [];
  private lastEncodedTs = -1;

  private targetMedia = -1;
  private targetStamp = 0; // performance.now() ms of the last setTargetMediaTime
  private heldFlag = false;

  private rvfcHandle = 0;
  private rafHandle = 0;
  private hiddenTimer = 0;
  private disposed = false;

  constructor(
    video: HTMLVideoElement,
    private readonly onError: (reason: string) => void,
  ) {
    this.video = video;
  }

  /** Begin capture and mount the overlay. Throws if WebCodecs/capture is unavailable. */
  async start(): Promise<void> {
    if (typeof VideoEncoder === "undefined" || typeof VideoDecoder === "undefined") {
      throw new Error("WebCodecs unavailable");
    }
    if (typeof this.video.requestVideoFrameCallback !== "function") {
      throw new Error("requestVideoFrameCallback unavailable");
    }
    if (!this.video.videoWidth || !this.video.videoHeight) {
      throw new Error("video has no frames yet");
    }
    this.codec = await pickCodec(this.video.videoWidth, this.video.videoHeight);
    if (!this.codec) throw new Error("no supported video codec");
    this.mountOverlay();
    this.drawPoster();
    await this.configurePipelines(this.video.videoWidth, this.video.videoHeight);
    this.captureLoop();
    this.renderLoop();
    document.addEventListener("visibilitychange", this.onVisibility);
    this.onVisibility();
  }

  /** Audio-derived presentation target: show the frame at or just before media time `m`. */
  setTargetMediaTime(m: number): void {
    this.targetMedia = m;
    this.targetStamp = performance.now();
  }

  /** Freeze presentation across media pauses (capture pauses by itself — no new frames render). */
  hold(value: boolean): void {
    this.heldFlag = value;
  }

  /** Toggle the "enhancing" spinner shown while the delay line refills. */
  setBuffering(value: boolean): void {
    if (this.spinner) this.spinner.style.display = value ? "flex" : "none";
  }

  /** Drop all buffered video (seeks/flushes); the canvas freezes on a live poster frame. */
  flush(): void {
    this.queue = [];
    for (const f of this.decoded.splice(0)) f.close();
    this.version++;
    this.framesSinceKey = KEYFRAME_INTERVAL;
    this.lastEncodedTs = -1;
    this.targetMedia = -1;
    this.drawPoster();
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    document.removeEventListener("visibilitychange", this.onVisibility);
    if (this.rvfcHandle) this.video.cancelVideoFrameCallback(this.rvfcHandle);
    if (this.rafHandle) cancelAnimationFrame(this.rafHandle);
    if (this.hiddenTimer) clearInterval(this.hiddenTimer);
    for (const f of this.decoded.splice(0)) f.close();
    this.queue = [];
    try {
      this.encoder?.close();
    } catch {
      /* ignore */
    }
    try {
      this.decoder?.close();
    } catch {
      /* ignore */
    }
    this.resize?.disconnect();
    this.canvas?.remove();
    this.spinner?.remove();
    this.video.style.removeProperty("visibility");
  }

  // --- capture side ---

  private captureLoop(): void {
    if (this.disposed) return;
    this.rvfcHandle = this.video.requestVideoFrameCallback((_now, meta) => {
      this.captureFrame(meta.mediaTime);
      this.captureLoop();
    });
  }

  private readonly onVisibility = (): void => {
    if (this.disposed) return;
    if (document.visibilityState === "hidden") {
      // rVFC stops firing in hidden tabs; audio keeps the tab exempt from intensive throttling, so
      // a ~1 Hz timer keeps sparse frames flowing and the picture recovers quickly on refocus.
      if (!this.hiddenTimer) {
        this.hiddenTimer = window.setInterval(() => {
          if (!this.video.paused) this.captureFrame(this.video.currentTime);
        }, HIDDEN_CAPTURE_MS);
      }
    } else if (this.hiddenTimer) {
      clearInterval(this.hiddenTimer);
      this.hiddenTimer = 0;
    }
  };

  private captureFrame(mediaTime: number): void {
    if (!this.encoder || this.encoder.state !== "configured") return;
    const w = this.video.videoWidth;
    const h = this.video.videoHeight;
    if (!w || !h) return;
    const ts = Math.round(mediaTime * 1e6);
    if (ts <= this.lastEncodedTs) return;
    if (w !== this.encWidth || h !== this.encHeight) {
      void this.configurePipelines(w, h);
      return; // this frame is dropped; the next one lands in the reconfigured encoder
    }
    const first = this.queue[0];
    const last = this.queue[this.queue.length - 1];
    if (first && last && last.tsSec - first.tsSec > MAX_QUEUE_S) {
      debugEvent("videoDelay", "queue-overflow", { len: this.queue.length }, "warn");
      this.flush();
    }
    let frame: VideoFrame;
    try {
      frame = new VideoFrame(this.video, { timestamp: ts });
    } catch (err) {
      this.fail(`frame capture failed: ${String(err)}`);
      return;
    }
    try {
      const keyFrame = this.framesSinceKey >= KEYFRAME_INTERVAL;
      this.encoder.encode(frame, { keyFrame });
      this.framesSinceKey = keyFrame ? 1 : this.framesSinceKey + 1;
      this.lastEncodedTs = ts;
    } catch (err) {
      this.fail(`encode failed: ${String(err)}`);
    } finally {
      frame.close();
    }
  }

  private async configurePipelines(width: number, height: number): Promise<void> {
    this.version++;
    this.framesSinceKey = KEYFRAME_INTERVAL;
    this.encWidth = width;
    this.encHeight = height;
    const config: VideoEncoderConfig = {
      codec: this.codec!,
      width,
      height,
      bitrate: Math.min(10_000_000, Math.max(1_500_000, Math.round(width * height * 2.1))),
      latencyMode: "realtime",
      ...(this.codec!.startsWith("avc1") ? { avc: { format: "annexb" as const } } : {}),
    };
    try {
      if (!this.encoder) {
        this.encoder = new VideoEncoder({
          output: (chunk) => this.onChunk(chunk),
          error: (e) => this.fail(`encoder: ${e.message}`),
        });
      }
      this.encoder.configure(config);
    } catch (err) {
      this.fail(`encoder configure failed: ${String(err)}`);
    }
  }

  private onChunk(chunk: EncodedVideoChunk): void {
    this.queue.push({ chunk, version: this.version, tsSec: chunk.timestamp / 1e6 });
  }

  // --- presentation side ---

  private renderLoop(): void {
    if (this.disposed) return;
    this.rafHandle = requestAnimationFrame(() => {
      this.present();
      this.renderLoop();
    });
  }

  private effectiveTarget(): number {
    if (this.targetMedia < 0) return -1;
    if (this.heldFlag) return this.targetMedia;
    const rate = this.video.playbackRate || 1;
    const age = Math.min(1, (performance.now() - this.targetStamp) / 1000);
    return this.targetMedia + age * rate;
  }

  private present(): void {
    const target = this.effectiveTarget();
    if (target < 0) return;
    void this.feedDecoder(target);

    let pick = -1;
    for (let i = 0; i < this.decoded.length; i++) {
      if (this.decoded[i]!.timestamp / 1e6 <= target) pick = i;
    }
    if (pick < 0) return;
    const frame = this.decoded[pick]!;
    for (const old of this.decoded.splice(0, pick)) old.close();
    this.drawFrame(frame);
  }

  private async feedDecoder(target: number): Promise<void> {
    if (this.decoderBusy) return;
    for (;;) {
      const next = this.queue[0];
      if (!next || next.tsSec > target + DECODE_LOOKAHEAD_S) break;
      if (!this.decoder || next.version !== this.decoderVersion) {
        // Version boundary: drain the old stream, then re-configure. Chunks after a boundary
        // always start with a keyframe (configurePipelines/flush reset the keyframe counter).
        if (next.chunk.type !== "key") {
          this.queue.shift();
          continue;
        }
        this.decoderBusy = true;
        try {
          if (this.decoder && this.decoder.state === "configured") await this.decoder.flush();
          if (!this.decoder || this.decoder.state === "closed") {
            this.decoder = new VideoDecoder({
              output: (frame) => this.onDecoded(frame),
              error: (e) => this.fail(`decoder: ${e.message}`),
            });
          }
          this.decoder.configure({ codec: this.codec!, optimizeForLatency: true });
          this.decoderVersion = next.version;
        } catch (err) {
          this.fail(`decoder configure failed: ${String(err)}`);
          return;
        } finally {
          this.decoderBusy = false;
        }
      }
      if (this.decoder.decodeQueueSize > 8) break;
      this.queue.shift();
      try {
        this.decoder.decode(next.chunk);
      } catch (err) {
        this.fail(`decode failed: ${String(err)}`);
        return;
      }
    }
  }

  private onDecoded(frame: VideoFrame): void {
    this.decoded.push(frame);
    while (this.decoded.length > 8) this.decoded.shift()!.close();
  }

  // --- overlay ---

  private mountOverlay(): void {
    const parent = this.video.parentElement;
    if (!parent) throw new Error("video has no parent to overlay");
    const canvas = document.createElement("canvas");
    canvas.setAttribute("data-sukoon", "video-delay");
    canvas.style.cssText =
      "position:absolute;pointer-events:none;z-index:10;background:transparent;";
    // The spinner overlay covers the same box as the canvas (the parent may not be the video's
    // containing block, so centering against it lands off-screen on some players), dimming the
    // frozen poster frame behind it so the refill state reads as deliberate.
    const spinner = document.createElement("div");
    spinner.setAttribute("data-sukoon", "video-delay-spinner");
    spinner.style.cssText =
      "position:absolute;z-index:11;pointer-events:none;display:none;" +
      "flex-direction:column;align-items:center;justify-content:center;gap:12px;" +
      "background:rgba(0,0,0,.6);";
    const ring = document.createElement("div");
    ring.style.cssText =
      "width:42px;height:42px;border:3px solid rgba(255,255,255,.25);" +
      "border-top-color:rgba(255,255,255,.9);border-radius:50%;" +
      "animation:sukoon-spin 0.9s linear infinite;";
    const label = document.createElement("div");
    label.textContent = "Removing Music (HQ)";
    label.style.cssText =
      "color:rgba(255,255,255,.9);font:500 14px/1 system-ui,sans-serif;" +
      "text-shadow:0 1px 2px rgba(0,0,0,.6);";
    spinner.append(ring, label);
    ensureSpinKeyframes();
    parent.appendChild(canvas);
    parent.appendChild(spinner);
    this.canvas = canvas;
    this.spinner = spinner;
    this.ctx2d = canvas.getContext("2d");
    const sync = () => {
      for (const el of [canvas, spinner]) {
        el.style.left = `${this.video.offsetLeft}px`;
        el.style.top = `${this.video.offsetTop}px`;
        el.style.width = `${this.video.offsetWidth}px`;
        el.style.height = `${this.video.offsetHeight}px`;
      }
    };
    sync();
    this.resize = new ResizeObserver(sync);
    this.resize.observe(this.video);
    this.video.style.setProperty("visibility", "hidden", "important");
  }

  /** Freeze the current live frame onto the canvas (engage/seek), so refills never show black. */
  private drawPoster(): void {
    if (!this.canvas || !this.ctx2d) return;
    const w = this.video.videoWidth;
    const h = this.video.videoHeight;
    if (!w || !h) return;
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    try {
      this.ctx2d.drawImage(this.video, 0, 0, w, h);
    } catch {
      /* a tainted/broken frame just leaves the previous canvas contents */
    }
  }

  private drawFrame(frame: VideoFrame): void {
    if (!this.canvas || !this.ctx2d) return;
    const w = frame.displayWidth;
    const h = frame.displayHeight;
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.ctx2d.drawImage(frame, 0, 0, w, h);
  }

  private fail(reason: string): void {
    if (this.disposed) return;
    debugEvent("videoDelay", "failed", { reason }, "error");
    this.onError(reason);
  }
}

/** Pick the first hardware-supportable codec for the given dimensions. */
async function pickCodec(width: number, height: number): Promise<string | null> {
  const candidates = ["avc1.640033", "vp09.00.51.08", "vp8"];
  for (const codec of candidates) {
    try {
      const { supported } = await VideoEncoder.isConfigSupported({
        codec,
        width,
        height,
        bitrate: 5_000_000,
        latencyMode: "realtime",
        ...(codec.startsWith("avc1") ? { avc: { format: "annexb" as const } } : {}),
      });
      if (supported) return codec;
    } catch {
      /* try the next codec */
    }
  }
  return null;
}

let spinInjected = false;
function ensureSpinKeyframes(): void {
  if (spinInjected) return;
  spinInjected = true;
  const style = document.createElement("style");
  style.textContent = "@keyframes sukoon-spin{to{transform:rotate(360deg)}}";
  document.head.appendChild(style);
}
