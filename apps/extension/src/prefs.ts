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
