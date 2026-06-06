import { debugEvent } from "./debug.js";

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
