// Generates the macOS DMG background (a soft, on-brand teal gradient) with no
// external image tooling — pure Node PNG encoding. Run: node scripts/gen-dmg-background.mjs
// Output: src-tauri/icons/dmg-background.png (660x400, matches the DMG window size).
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const W = 660;
const H = 400;
const TOP = [0xee, 0xf5, 0xf4]; // very light teal-white (top)
const BOT = [0xd6, 0xe9, 0xe7]; // light teal (bottom)

// Raw image: one filter byte (0 = none) + RGB triples per row.
const raw = Buffer.alloc(H * (1 + W * 3));
for (let y = 0; y < H; y++) {
  const t = y / (H - 1);
  const r = Math.round(TOP[0] + (BOT[0] - TOP[0]) * t);
  const g = Math.round(TOP[1] + (BOT[1] - TOP[1]) * t);
  const b = Math.round(TOP[2] + (BOT[2] - TOP[2]) * t);
  const off = y * (1 + W * 3);
  raw[off] = 0;
  for (let x = 0; x < W; x++) {
    const p = off + 1 + x * 3;
    raw[p] = r;
    raw[p + 1] = g;
    raw[p + 2] = b;
  }
}

const crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = crcTable[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
};

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 2; // color type 2 = RGB
const png = Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const out = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../src-tauri/icons/dmg-background.png",
);
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, png);
console.log(`wrote ${out} (${png.length} bytes, ${W}x${H})`);
