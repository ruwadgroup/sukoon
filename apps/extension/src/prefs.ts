/**
 * Per-video on/off preference, keyed by an opaque, adapter-supplied media key (e.g. `yt:<id>`,
 * `web:<host><path>`) so it never leaks between videos or collides across platforms or tabs.
 *
 * - Per-video overrides live in `chrome.storage.local` under `v:<key>` (local: many videos, roomy
 *   with `unlimitedStorage`). A single `defaults` entry in `chrome.storage.sync` seeds an unset
 *   video and tracks the most recent choice; it's small and syncs across devices.
 */

export interface Prefs {
  enabled: boolean;
}

function normalize(v: unknown): Prefs {
  const o = (v ?? {}) as Partial<Prefs>;
  return { enabled: o.enabled !== false };
}

/** The storage key for a video's per-video override. */
export function videoKey(id: string): string {
  return `v:${id}`;
}

/** The default applied to a video the user hasn't configured. */
export async function getDefaults(): Promise<Prefs> {
  try {
    const { defaults } = await chrome.storage.sync.get("defaults");
    return normalize(defaults);
  } catch {
    return { enabled: true };
  }
}

/** Prefs for a specific video: its override if any, else the defaults. */
export async function getVideoPrefs(id: string): Promise<Prefs> {
  const key = videoKey(id);
  const [local, defaults] = await Promise.all([chrome.storage.local.get(key), getDefaults()]);
  return local[key] ? normalize(local[key]) : defaults;
}

/** Set a video's on/off, and seed `defaults` with the latest choice. */
export async function setVideoPrefs(id: string, partial: Partial<Prefs>): Promise<void> {
  const key = videoKey(id);
  const next: Prefs = { ...(await getVideoPrefs(id)), ...partial };
  await chrome.storage.local.set({ [key]: next });
  await chrome.storage.sync.set({ defaults: next });
}

/** Update the global default (used when no video is in context, e.g. the popup on a non-video page). */
export async function setDefaults(partial: Partial<Prefs>): Promise<void> {
  await chrome.storage.sync.set({ defaults: { ...(await getDefaults()), ...partial } });
}

/** Read a `v:<key>` storage-change value into Prefs. */
export function prefsFromChange(value: unknown): Prefs {
  return normalize(value);
}

/**
 * HQ Live setting — global (not per-video): whether to prefer the desktop companion's HQ engine.
 * Pairing with the desktop app is automatic (the background worker fetches the token from the
 * companion's `/pairing` endpoint), so this is a single `hqLive` flag in `chrome.storage.sync`.
 */
export interface HqSettings {
  enabled: boolean;
}

export const HQ_STORAGE_KEYS = ["hqLive"] as const;

export async function getHqSettings(): Promise<HqSettings> {
  try {
    const { hqLive } = await chrome.storage.sync.get("hqLive");
    return { enabled: hqLive === true };
  } catch {
    return { enabled: false };
  }
}

export async function setHqSettings(partial: Partial<HqSettings>): Promise<void> {
  if (partial.enabled !== undefined) await chrome.storage.sync.set({ hqLive: partial.enabled });
}

/**
 * Comfort-noise ("Add noise") setting — global. The bed's level always adapts to the content;
 * the mode only picks the color: `smart` matches the content's own noise-floor spectrum, the
 * classic colors force a shape. Stored under `noiseMode` in `chrome.storage.sync`.
 */
export type NoiseMode = "off" | "smart" | "white" | "pink" | "brown";

export const NOISE_STORAGE_KEY = "noiseMode";

const NOISE_MODES: readonly NoiseMode[] = ["off", "smart", "white", "pink", "brown"];

export async function getNoiseMode(): Promise<NoiseMode> {
  try {
    const { noiseMode } = await chrome.storage.sync.get(NOISE_STORAGE_KEY);
    return NOISE_MODES.includes(noiseMode as NoiseMode) ? (noiseMode as NoiseMode) : "off";
  } catch {
    return "off";
  }
}

export async function setNoiseMode(mode: NoiseMode): Promise<void> {
  await chrome.storage.sync.set({ [NOISE_STORAGE_KEY]: mode });
}
