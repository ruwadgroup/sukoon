// Stages the AudioWorklet bundle and the DeepFilterNet WASM into the extension dist/.
//
// `worklet.js` registers the `dfn-processor` (the real-time engine), with the wasm-bindgen glue
// concatenated in. The DFN WASM binary is copied verbatim so the content script can compile it and
// hand the module to the worklet (AudioWorklets can't fetch).
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const ext = resolve(here, "..");
const dist = resolve(ext, "dist");
const dfnPkg = resolve(ext, "../../packages/dfn-wasm/pkg");

const polyfills = readFileSync(resolve(ext, "worklet/polyfills.js"), "utf8");
const glue = readFileSync(resolve(dfnPkg, "dfn_wasm.js"), "utf8");
const dfn = readFileSync(resolve(ext, "worklet/processor.js"), "utf8");

mkdirSync(dist, { recursive: true });
writeFileSync(
  resolve(dist, "worklet.js"),
  `${polyfills}\n;/* --- wasm-bindgen glue (DeepFilterNet) --- */\n${glue}\n` +
    `;/* --- Sukoon DFN worklet --- */\n${dfn}`,
);

// The DFN model+runtime is embedded in this wasm; the content script compiles it and passes the
// module into the worklet. It must be web-accessible (see manifest web_accessible_resources).
copyFileSync(resolve(dfnPkg, "dfn_wasm_bg.wasm"), resolve(dist, "dfn_wasm_bg.wasm"));

console.log("[sukoon] staged worklet.js (dfn) and dfn_wasm_bg.wasm into dist/");
