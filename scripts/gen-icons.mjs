// Generates the application icons from the mascot pixel grid. No dependencies.
// Usage: node scripts/gen-icons.mjs
import { deflateSync, crc32 } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const GRID = [
  "..oooooooooo..",
  "..oooooooooo..",
  "..ooXooooXoo..",
  "..ooXooooXoo..",
  "oooooooooooooo",
  "oooooooooooooo",
  "..oooooooooo..",
  "..oooooooooo..",
  "..o.o....o.o..",
  "..o.o....o.o..",
];
const COLORS = { o: [0xd9, 0x77, 0x57, 0xff], X: [0x2b, 0x1b, 0x17, 0xff] };

function render(size) {
  const cell = Math.max(1, Math.floor(size / 16));
  const w = GRID[0].length * cell;
  const h = GRID.length * cell;
  const ox = Math.floor((size - w) / 2);
  const oy = Math.floor((size - h) / 2);
  const px = Buffer.alloc(size * size * 4);
  for (let gy = 0; gy < GRID.length; gy++) {
    for (let gx = 0; gx < GRID[gy].length; gx++) {
      const color = COLORS[GRID[gy][gx]];
      if (!color) continue;
      for (let y = 0; y < cell; y++) {
        for (let x = 0; x < cell; x++) {
          const i = ((oy + gy * cell + y) * size + ox + gx * cell + x) * 4;
          px.set(color, i);
        }
      }
    }
  }
  return px;
}

function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const out = Buffer.alloc(body.length + 8);
  out.writeUInt32BE(data.length, 0);
  body.copy(out, 4);
  out.writeUInt32BE(crc32(body) >>> 0, out.length - 4);
  return out;
}

function png(size) {
  const px = render(size);
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    px.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function ico(sizes) {
  const images = sizes.map((s) => ({ s, data: png(s) }));
  const header = Buffer.alloc(6 + 16 * images.length);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = header.length;
  images.forEach(({ s, data }, i) => {
    const e = 6 + 16 * i;
    header[e] = s >= 256 ? 0 : s;
    header[e + 1] = s >= 256 ? 0 : s;
    header.writeUInt16LE(1, e + 4);
    header.writeUInt16LE(32, e + 6);
    header.writeUInt32LE(data.length, e + 8);
    header.writeUInt32LE(offset, e + 12);
    offset += data.length;
  });
  return Buffer.concat([header, ...images.map((i) => i.data)]);
}

const dir = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons");
mkdirSync(dir, { recursive: true });
writeFileSync(join(dir, "32x32.png"), png(32));
writeFileSync(join(dir, "icon.png"), png(256));
writeFileSync(join(dir, "icon.ico"), ico([16, 32, 48, 256]));
console.log("icons written to", dir);
