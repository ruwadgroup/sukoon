/* eslint-disable */
/**
 * Minimal UTF-8 TextEncoder/TextDecoder for AudioWorkletGlobalScope, which (unlike Window/Worker)
 * provides neither. The wasm-bindgen glue instantiates both **unguarded at module-eval time**, so
 * without these the whole worklet fails to load with `AbortError: Unable to load a worklet's module`.
 * These are prepended ABOVE the glue at build time (scripts/bundle-worklet.mjs).
 *
 * Correctness only matters for the few short UTF-8 strings the engine round-trips (config keys, error
 * messages); our hot path passes Float32 buffers, no strings.
 */
if (typeof globalThis.TextDecoder === "undefined") {
  globalThis.TextDecoder = class {
    constructor() {}
    decode(input) {
      if (!input) return "";
      const b = input instanceof Uint8Array ? input : new Uint8Array(input.buffer || input);
      let out = "";
      let i = 0;
      while (i < b.length) {
        const c = b[i++];
        if (c < 0x80) {
          out += String.fromCharCode(c);
        } else if (c < 0xe0) {
          out += String.fromCharCode(((c & 0x1f) << 6) | (b[i++] & 0x3f));
        } else if (c < 0xf0) {
          out += String.fromCharCode(((c & 0x0f) << 12) | ((b[i++] & 0x3f) << 6) | (b[i++] & 0x3f));
        } else {
          let cp =
            ((c & 0x07) << 18) | ((b[i++] & 0x3f) << 12) | ((b[i++] & 0x3f) << 6) | (b[i++] & 0x3f);
          cp -= 0x10000;
          out += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
        }
      }
      return out;
    }
  };
}

if (typeof globalThis.TextEncoder === "undefined") {
  globalThis.TextEncoder = class {
    encode(str) {
      str = String(str);
      const bytes = [];
      for (let i = 0; i < str.length; i++) {
        let c = str.charCodeAt(i);
        if (c < 0x80) {
          bytes.push(c);
        } else if (c < 0x800) {
          bytes.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
        } else if (c >= 0xd800 && c <= 0xdbff) {
          const c2 = str.charCodeAt(++i);
          c = 0x10000 + ((c & 0x3ff) << 10) + (c2 & 0x3ff);
          bytes.push(
            0xf0 | (c >> 18),
            0x80 | ((c >> 12) & 0x3f),
            0x80 | ((c >> 6) & 0x3f),
            0x80 | (c & 0x3f),
          );
        } else {
          bytes.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
        }
      }
      return new Uint8Array(bytes);
    }
    encodeInto(str, view) {
      const enc = this.encode(str);
      const n = Math.min(enc.length, view.length);
      view.set(enc.subarray(0, n));
      return { read: str.length, written: n };
    }
  };
}
