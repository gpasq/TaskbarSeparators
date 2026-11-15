// Renders assets/icon.svg into src-tauri/icons/icon.ico (PNG entries) and icon.png,
// using headless Microsoft Edge (already on every Windows 11 PC), so no extra tools.
// Run: node scripts/make-icon.mjs   (or `just icon`)
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const svg = readFileSync(join(root, "assets", "icon.svg"), "utf8");
const icons = join(root, "src-tauri", "icons");

const edge = [
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
].find(existsSync);
if (!edge) throw new Error("Microsoft Edge not found");

const tmp = mkdtempSync(join(tmpdir(), "ts-icon-"));
function render(size) {
  const html = join(tmp, `${size}.html`);
  const png = join(tmp, `${size}.png`);
  writeFileSync(
    html,
    `<!doctype html><style>html,body{margin:0;background:transparent;overflow:hidden}` +
      `svg{display:block;width:${size}px;height:${size}px}</style>${svg}`,
  );
  spawnSync(edge, [
    "--headless=new", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
    "--default-background-color=00000000", `--window-size=${size},${size}`,
    `--user-data-dir=${join(tmp, "profile")}`, `--screenshot=${png}`, pathToFileURL(html).href,
  ]);
  if (!existsSync(png)) throw new Error(`Edge did not render ${size}px`);
  return readFileSync(png);
}

const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const images = sizes.map(render);
const header = Buffer.alloc(6);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(images.length, 4);
let offset = 6 + 16 * images.length;
const entries = images.map((data, i) => {
  const e = Buffer.alloc(16);
  e[0] = e[1] = sizes[i] % 256;
  e.writeUInt16LE(1, 4);
  e.writeUInt16LE(32, 6);
  e.writeUInt32LE(data.length, 8);
  e.writeUInt32LE(offset, 12);
  offset += data.length;
  return e;
});
writeFileSync(join(icons, "icon.ico"), Buffer.concat([header, ...entries, ...images]));
writeFileSync(join(icons, "icon.png"), render(512));
rmSync(tmp, { recursive: true, force: true });
console.log(`Updated ${join(icons, "icon.ico")} and icon.png`);
