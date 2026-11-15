// Generates src-tauri/icons/icon.ico and icon.png (the app's own icon) with no dependencies.
// Run: node scripts/make-app-icon.mjs
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const out = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons");
mkdirSync(out, { recursive: true });

// Signed-distance helpers in a 0..1 coordinate space.
const roundRect = (x, y, cx, cy, hw, hh, r) => {
  const dx = Math.max(Math.abs(x - cx) - hw + r, 0);
  const dy = Math.max(Math.abs(y - cy) - hh + r, 0);
  return Math.hypot(dx, dy) - r + Math.min(Math.max(Math.abs(x - cx) - hw + r, Math.abs(y - cy) - hh + r), 0);
};

function shade(x, y) {
  // Background tile: blue→violet gradient.
  if (roundRect(x, y, 0.5, 0.5, 0.46, 0.46, 0.2) > 0) return null;
  const t = (x + y) / 2;
  let c = [0x3b + (0x7c - 0x3b) * t, 0x82 + (0x3a - 0x82) * t, 0xf6 + (0xed - 0xf6) * t, 255];
  // Taskbar strip.
  if (roundRect(x, y, 0.5, 0.62, 0.36, 0.13, 0.05) <= 0) c = [0x14, 0x18, 0x2b, 235];
  // App squares on either side of the separator.
  for (const cx of [0.27, 0.41, 0.69]) {
    if (roundRect(x, y, cx, 0.62, 0.05, 0.05, 0.015) <= 0) c = [0xe8, 0xec, 0xff, 255];
  }
  // The separator itself, standing a little taller than the strip.
  if (roundRect(x, y, 0.55, 0.55, 0.022, 0.26, 0.02) <= 0) c = [0xff, 0xc8, 0x3d, 255];
  return c;
}

function render(size) {
  const ss = 4;
  const px = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) {
      let acc = [0, 0, 0, 0];
      for (let sy = 0; sy < ss; sy++)
        for (let sx = 0; sx < ss; sx++) {
          const c = shade((x + (sx + 0.5) / ss) / size, (y + (sy + 0.5) / ss) / size);
          if (c) {
            const a = c[3] / 255;
            acc = [acc[0] + c[0] * a, acc[1] + c[1] * a, acc[2] + c[2] * a, acc[3] + a];
          }
        }
      const i = (y * size + x) * 4;
      const a = acc[3] / (ss * ss);
      px[i] = acc[3] ? acc[0] / acc[3] : 0;
      px[i + 1] = acc[3] ? acc[1] / acc[3] : 0;
      px[i + 2] = acc[3] ? acc[2] / acc[3] : 0;
      px[i + 3] = Math.round(a * 255);
    }
  return px;
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
};
function png(size) {
  const px = render(size);
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) px.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const sizes = [16, 24, 32, 48, 64, 128, 256];
const images = sizes.map(png);
const header = Buffer.alloc(6);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(sizes.length, 4);
let offset = 6 + 16 * sizes.length;
const entries = sizes.map((s, i) => {
  const e = Buffer.alloc(16);
  e[0] = s % 256;
  e[1] = s % 256;
  e.writeUInt16LE(1, 4);
  e.writeUInt16LE(32, 6);
  e.writeUInt32LE(images[i].length, 8);
  e.writeUInt32LE(offset, 12);
  offset += images[i].length;
  return e;
});
writeFileSync(join(out, "icon.ico"), Buffer.concat([header, ...entries, ...images]));
writeFileSync(join(out, "icon.png"), png(512));
console.log("wrote", join(out, "icon.ico"), "and icon.png");
