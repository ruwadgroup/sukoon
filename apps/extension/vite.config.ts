import { resolve } from "node:path";
import { defineConfig } from "vite";

// Builds the MV3 bundle: content script, service worker, and the React popup.
//
// React forces a shared vendor chunk, and a *classic* content script (the kind the manifest
// registers) cannot `import` it. So the manifest registers `content-loader.js` (a tiny classic
// script in `public/`) which dynamic-imports the real ES-module `content.js`; that module and its
// chunks are exposed via `web_accessible_resources`. Everything in `public/` (manifest, loader,
// icons) is copied to `dist/` verbatim.
export default defineConfig({
  base: "./",
  publicDir: "public",
  // JSX is transformed by esbuild using the `jsx`/`jsxImportSource` settings from tsconfig.json
  // (automatic runtime) — no @vitejs/plugin-react needed; the extension build has no Fast Refresh.
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        content: resolve(__dirname, "src/content.ts"),
        background: resolve(__dirname, "src/background.ts"),
        popup: resolve(__dirname, "popup.html"),
      },
      output: {
        entryFileNames: "[name].js",
        chunkFileNames: "assets/[name]-[hash].js",
        assetFileNames: "assets/[name]-[hash][extname]",
        format: "es",
      },
    },
  },
});
