/**
 * Classic content-script shim.
 *
 * MV3 registers this plain script (no `import` syntax) so Chrome will inject it. It immediately
 * dynamic-imports the real ES-module content script, which is allowed to pull in shared chunks
 * (React). Both `content.js` and `assets/*` are listed in `web_accessible_resources`.
 */
import(chrome.runtime.getURL("content.js")).catch((error) => {
  console.error("[Sukoon] failed to load content script:", error);
});
