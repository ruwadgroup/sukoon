type DebugLevel = "debug" | "info" | "warn" | "error";

export interface DebugEvent {
  at: string;
  t: number;
  level: DebugLevel;
  scope: string;
  event: string;
  data?: unknown;
}

type SnapshotProvider = () => unknown;

interface DebugApi {
  events: DebugEvent[];
  verbose: boolean;
  clear(): void;
  enableVerbose(): void;
  disableVerbose(): void;
  snapshot(): Record<string, unknown>;
}

declare global {
  // Available from the DevTools context dropdown under "Sukoon — Music Remover Extension".
  // Example: __SUKOON_DEBUG__.snapshot()
  // eslint-disable-next-line no-var
  var __SUKOON_DEBUG__: DebugApi | undefined;
}

const PREFIX = "[Sukoon]";
const MAX_EVENTS = 250;

const events: DebugEvent[] = [];
const snapshots = new Map<string, SnapshotProvider>();
let verbose = false;

function serialize(data: unknown, seen = new WeakSet<object>()): unknown {
  if (data instanceof Error) return { name: data.name, message: data.message, stack: data.stack };
  if (data === null || typeof data !== "object") return data;
  if (seen.has(data)) return "[Circular]";
  seen.add(data);
  if (Array.isArray(data)) return data.map((item) => serialize(item, seen));
  return Object.fromEntries(
    Object.entries(data as Record<string, unknown>).map(([key, value]) => [
      key,
      serialize(value, seen),
    ]),
  );
}

function format(data: unknown): string {
  if (data === undefined || data === "") return "";
  try {
    return JSON.stringify(serialize(data));
  } catch {
    return String(data);
  }
}

function snapshot(): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [name, provider] of snapshots) {
    try {
      out[name] = provider();
    } catch (error) {
      out[name] = { error: serialize(error) };
    }
  }
  return out;
}

function installApi(): void {
  globalThis.__SUKOON_DEBUG__ = {
    get events() {
      return [...events];
    },
    get verbose() {
      return verbose;
    },
    clear: () => {
      events.length = 0;
    },
    enableVerbose: () => {
      verbose = true;
      console.info(`${PREFIX} verbose logging enabled`);
    },
    disableVerbose: () => {
      verbose = false;
      console.info(`${PREFIX} verbose logging disabled`);
    },
    snapshot,
  };
}

installApi();

export function registerDebugSnapshot(name: string, provider: SnapshotProvider): void {
  snapshots.set(name, provider);
}

export function debugEvent(
  scope: string,
  event: string,
  data?: unknown,
  level: DebugLevel = "debug",
): void {
  const item: DebugEvent = {
    at: new Date().toISOString(),
    t: Math.round(performance.now()),
    level,
    scope,
    event,
    data: serialize(data),
  };
  events.push(item);
  if (events.length > MAX_EVENTS) events.shift();

  const line = `${PREFIX} ${scope} ${event} ${format(item.data)}`.trim();
  if (level === "error") console.error(line);
  else if (level === "warn") console.warn(line);
  else if (level === "info" || verbose) console.info(line);
}
