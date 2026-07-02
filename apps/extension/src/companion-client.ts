/**
 * WebSocket client for the desktop companion's realtime protocol — see
 * docs/research/companion-realtime-protocol.md.
 *
 * The extension streams noisy 48 kHz stereo PCM (interleaved f32) to the desktop app, which runs
 * the resident MDX (HQ) engine in a streaming session and pushes cleaned PCM back at the same
 * stream positions. Control is JSON text frames; audio is 16-byte-header binary frames. The client
 * connects from the content script, so pairing is by token (the page origin is the site's, not the
 * extension's).
 */

import { debugEvent } from "./debug.js";

export const COMPANION_PORT = 8765;

/**
 * Fetch the pairing token via the background service worker (whose request carries the extension
 * origin the companion's `/pairing` endpoint trusts). Returns null when the desktop app isn't
 * running - the caller stays on the instant engine and retries later.
 */
export async function requestPairingToken(): Promise<string | null> {
  try {
    const reply = (await chrome.runtime.sendMessage({ type: "companion:pair" })) as {
      token?: string | null;
    } | null;
    return reply?.token ?? null;
  } catch {
    return null;
  }
}

const KIND_NOISY = 1;
const KIND_CLEAN = 2;
const HEADER_BYTES = 16;
const CONTROL_TIMEOUT_MS = 10_000;

export interface RealtimeStart {
  /** Emit granularity in 48 kHz samples (one engine block). */
  blockSamples: number;
  /** Server's estimated end-to-end processing latency in 48 kHz samples. */
  latencySamples: number;
}

export interface CompanionCallbacks {
  /** Cleaned interleaved stereo landed for `[startSample, startSample + data.length/2)`. */
  onCleanAudio(epoch: number, startSample: number, data: Float32Array): void;
  /** The session or connection is gone (overload, socket close, protocol error). */
  onDown(reason: string): void;
}

interface Pending {
  type: string;
  resolve(msg: Record<string, unknown>): void;
  reject(err: Error): void;
  timer: number;
}

export class CompanionClient {
  private readonly pending: Pending[] = [];
  private down = false;

  private constructor(
    private readonly ws: WebSocket,
    private readonly callbacks: CompanionCallbacks,
  ) {
    ws.onmessage = (ev) => this.onMessage(ev);
    ws.onclose = () => this.fail("connection closed");
    ws.onerror = () => this.fail("connection error");
  }

  /** Connect and complete the `hello` → `ready` handshake; rejects if realtime is unsupported. */
  static async connect(token: string, callbacks: CompanionCallbacks): Promise<CompanionClient> {
    const ws = await openSocket(
      `ws://127.0.0.1:${COMPANION_PORT}/?token=${encodeURIComponent(token)}`,
    );
    const client = new CompanionClient(ws, callbacks);
    try {
      const ready = await client.request({ type: "hello" }, "ready");
      if (ready.realtime !== true) throw new Error("companion has no realtime support");
      return client;
    } catch (err) {
      client.close();
      throw err;
    }
  }

  /** Start the realtime session on the given engine. */
  async startRealtime(engine: string): Promise<RealtimeStart> {
    const started = await this.request({ type: "rt_start", engine }, "rt_started");
    const blockSamples = Number(started.blockSamples);
    const latencySamples = Number(started.latencySamples);
    if (!Number.isFinite(blockSamples) || !Number.isFinite(latencySamples)) {
      throw new Error("malformed rt_started");
    }
    return { blockSamples, latencySamples };
  }

  /** Reset the server's stream state; audio sent after this carries `epoch` from position 0. */
  async flush(epoch: number): Promise<void> {
    await this.request({ type: "rt_flush", epoch }, "rt_flushed");
  }

  /** Stream one batch of noisy interleaved stereo at `startSample`. */
  sendAudio(epoch: number, startSample: number, data: Float32Array): void {
    if (this.ws.readyState !== WebSocket.OPEN) return;
    const buf = new ArrayBuffer(HEADER_BYTES + data.byteLength);
    const view = new DataView(buf);
    view.setUint8(0, KIND_NOISY);
    view.setUint8(1, 2);
    view.setUint16(2, epoch, true);
    view.setUint32(4, 0, true);
    view.setBigUint64(8, BigInt(startSample), true);
    new Float32Array(buf, HEADER_BYTES).set(data);
    this.ws.send(buf);
  }

  close(): void {
    this.down = true;
    try {
      this.ws.close();
    } catch {
      /* ignore */
    }
  }

  private request(
    msg: Record<string, unknown>,
    replyType: string,
  ): Promise<Record<string, unknown>> {
    return new Promise((resolve, reject) => {
      if (this.ws.readyState !== WebSocket.OPEN) {
        reject(new Error("companion socket not open"));
        return;
      }
      const timer = window.setTimeout(() => {
        const idx = this.pending.findIndex((p) => p.timer === timer);
        if (idx >= 0) this.pending.splice(idx, 1);
        reject(new Error(`companion ${replyType} timed out`));
      }, CONTROL_TIMEOUT_MS);
      this.pending.push({ type: replyType, resolve, reject, timer });
      this.ws.send(JSON.stringify(msg));
    });
  }

  private onMessage(ev: MessageEvent): void {
    if (typeof ev.data === "string") {
      let msg: Record<string, unknown>;
      try {
        msg = JSON.parse(ev.data) as Record<string, unknown>;
      } catch {
        return;
      }
      const type = msg.type;
      if (type === "rt_overloaded") {
        this.fail("companion overloaded (inference behind realtime)");
        return;
      }
      const idx = this.pending.findIndex((p) => p.type === type);
      if (idx >= 0) {
        const [p] = this.pending.splice(idx, 1);
        if (p) {
          clearTimeout(p.timer);
          p.resolve(msg);
        }
      } else if (type === "error") {
        // An unpaired error settles the oldest pending request, if any.
        const p = this.pending.shift();
        if (p) {
          clearTimeout(p.timer);
          p.reject(new Error(String(msg.error ?? "companion error")));
        } else {
          debugEvent("companion", "error", { error: msg.error }, "warn");
        }
      }
      return;
    }

    const data = ev.data as ArrayBuffer;
    if (!(data instanceof ArrayBuffer) || data.byteLength < HEADER_BYTES) return;
    const view = new DataView(data);
    if (view.getUint8(0) !== KIND_CLEAN || view.getUint8(1) !== 2) return;
    const epoch = view.getUint16(2, true);
    const startSample = Number(view.getBigUint64(8, true));
    this.callbacks.onCleanAudio(epoch, startSample, new Float32Array(data, HEADER_BYTES));
  }

  private fail(reason: string): void {
    if (this.down) return;
    this.down = true;
    for (const p of this.pending.splice(0)) {
      clearTimeout(p.timer);
      p.reject(new Error(reason));
    }
    try {
      this.ws.close();
    } catch {
      /* ignore */
    }
    this.callbacks.onDown(reason);
  }
}

function openSocket(url: string): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    let ws: WebSocket;
    try {
      ws = new WebSocket(url);
    } catch (err) {
      reject(err instanceof Error ? err : new Error(String(err)));
      return;
    }
    ws.binaryType = "arraybuffer";
    const timer = window.setTimeout(() => {
      ws.close();
      reject(new Error("companion connect timed out"));
    }, 4_000);
    ws.onopen = () => {
      clearTimeout(timer);
      resolve(ws);
    };
    ws.onerror = () => {
      clearTimeout(timer);
      reject(new Error("companion not reachable"));
    };
  });
}
