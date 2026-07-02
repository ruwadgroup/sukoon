import { debugEvent } from "./debug.js";

/**
 * Companion auto-pairing. Content scripts can't fetch the pairing token themselves (their requests
 * carry the page's origin, which the desktop app rightly refuses), so they ask this service worker:
 * its fetch carries the extension origin, which `GET /pairing` trusts. No user-visible pairing step.
 */
const PAIRING_URL = "http://127.0.0.1:8765/pairing";

chrome.runtime.onMessage.addListener((msg: { type?: string }, _sender, sendResponse) => {
  if (msg?.type !== "companion:pair") return;
  void (async () => {
    try {
      const res = await fetch(PAIRING_URL, { signal: AbortSignal.timeout(2_000) });
      if (!res.ok) throw new Error(`pairing responded ${res.status}`);
      const { token } = (await res.json()) as { token?: unknown };
      sendResponse({ token: typeof token === "string" && token !== "" ? token : null });
    } catch (error) {
      sendResponse({ token: null, error: String(error) });
    }
  })();
  return true; // async sendResponse
});

const PLATFORM_TABS = [
  "*://*.youtube.com/*",
  "*://*.facebook.com/*",
  "*://*.instagram.com/*",
  "*://*.x.com/*",
  "*://*.twitter.com/*",
];

chrome.runtime.onInstalled.addListener(async (details) => {
  const { defaults } = await chrome.storage.sync.get("defaults");
  if (defaults === undefined) {
    await chrome.storage.sync.set({ defaults: { enabled: true } });
  }

  if (details.reason === "install" || details.reason === "update") {
    try {
      const tabs = await chrome.tabs.query({ url: PLATFORM_TABS });
      for (const tab of tabs) {
        if (tab.id != null) chrome.tabs.reload(tab.id, { bypassCache: false });
      }
      debugEvent("background", "tabs:reloaded-on-update", { count: tabs.length }, "info");
    } catch (error) {
      debugEvent("background", "tabs:reload-failed", error, "warn");
    }
  }
});
