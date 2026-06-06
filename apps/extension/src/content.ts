import { AudioGraph } from "./audio-graph.js";
import { buttonStore } from "./button-store.js";
import { pickAdapter } from "./adapters/index.js";
import {
  getDefaults,
  getVideoPrefs,
  prefsFromChange,
  setDefaults,
  setVideoPrefs,
  videoKey,
  type Prefs,
} from "./prefs.js";
import { debugEvent, registerDebugSnapshot } from "./debug.js";

const adapter = pickAdapter(location.hostname);
const graph = new AudioGraph();

let currentKey: string | null = null;
let currentEnabled = false;

function extensionAlive(): boolean {
  try {
    return Boolean(chrome.runtime?.id);
  } catch {
    return false;
  }
}

const ORPHAN_FLAG = "sukoon-orphan-reload";
function healIfOrphaned(): boolean {
  if (extensionAlive()) {
    try {
      sessionStorage.removeItem(ORPHAN_FLAG);
    } catch {
      /* ignore */
    }
    return false;
  }
  try {
    if (!sessionStorage.getItem(ORPHAN_FLAG)) {
      sessionStorage.setItem(ORPHAN_FLAG, "1");
      location.reload();
    }
  } catch {
    location.reload();
  }
  return true;
}

registerDebugSnapshot("content", () => ({
  url: location.href,
  site: adapter.id,
  mediaKey: currentKey,
  enabled: currentEnabled,
  buttonStatus: buttonStore.get(),
  videoFound: adapter.pickVideo() !== null,
}));

graph.onStatus((status) => buttonStore.setStatus(status));

function applyPrefs(p: Prefs): void {
  currentEnabled = p.enabled;
  if (p.enabled) {
    void graph.prewarm();
    graph.enable();
  } else {
    graph.disable();
  }
}

let contextSeq = 0;
async function onVideoContext(): Promise<void> {
  if (healIfOrphaned()) return;
  const key = adapter.mediaKey();
  if (key === currentKey) return;
  currentKey = key;
  const seq = ++contextSeq;
  const prefs = key ? await getVideoPrefs(key) : await getDefaults();
  if (seq !== contextSeq) return;
  debugEvent("prefs", "applied", { mediaKey: key, enabled: prefs.enabled }, "info");
  applyPrefs(prefs);
}

buttonStore.setToggleHandler(() => {
  if (healIfOrphaned()) return;
  const turnOn = !currentEnabled;
  debugEvent("ui", "toggle", { turnOn, mediaKey: currentKey }, "info");
  currentEnabled = turnOn;
  if (turnOn) graph.enable();
  else graph.disable();
  if (currentKey) void setVideoPrefs(currentKey, { enabled: turnOn });
  else void setDefaults({ enabled: turnOn });
});

document.addEventListener(
  "playing",
  (e) => {
    if (adapter.isPlayerVideo(e.target)) {
      graph.addMedia(e.target);
      void onVideoContext();
      adapter.mountButton();
    }
  },
  true,
);

chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local" || !currentKey) return;
  const change = changes[videoKey(currentKey)];
  if (!change) return;
  const prefs = prefsFromChange(change.newValue);
  debugEvent("prefs", "remote-change", { mediaKey: currentKey, enabled: prefs.enabled }, "info");
  applyPrefs(prefs);
});

let scheduled = false;
const observer = new MutationObserver(() => {
  if (scheduled) return;
  scheduled = true;
  requestAnimationFrame(() => {
    scheduled = false;
    adapter.mountButton();
  });
});
observer.observe(document.documentElement, { childList: true, subtree: true });

let syncTries = 0;
function syncNow(): void {
  if (healIfOrphaned()) return;
  void onVideoContext();
  adapter.mountButton();
  const v = adapter.pickVideo();
  if (v) {
    syncTries = 0;
    graph.addMedia(v);
  } else if (syncTries++ < 10) {
    window.setTimeout(syncNow, 500);
  }
}

adapter.onNavigate(() => {
  syncTries = 0;
  syncNow();
});

adapter.mountButton();
debugEvent("content", "loaded", { url: location.href, site: adapter.id }, "info");
syncNow();
