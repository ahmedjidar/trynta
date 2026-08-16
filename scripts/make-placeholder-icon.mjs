#!/usr/bin/env node
// Emits a deliberately plain 1024×1024 source PNG for `pnpm tauri icon`.
//
// PLACEHOLDER: awaiting handoff app-icon. The application icon is a design
// decision and not ours to make (CLAUDE.md §3), but `tauri-build` cannot produce
// a Windows resource file without `src-tauri/icons/icon.ico`, so the build is
// blocked without *something*. This is that something: a flat neutral square
// with no mark, no letterform and no colour opinion, chosen to be obviously
// unfinished rather than plausibly final.
//
//   node scripts/make-placeholder-icon.mjs      → src-tauri/icons/source.png
//   pnpm tauri icon src-tauri/icons/source.png  → the full icon set

import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const OUT_DIR = join(ROOT, 'src-tauri', 'icons');
const SIZE = 1024;
const GREY = 0x80; // mid grey: no brand, no theme, no opinion

const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // colour type: RGBA
// 10..12: compression, filter, interlace — all zero

// One filter byte (0 = None) per scanline, then RGBA pixels.
const raw = Buffer.alloc(SIZE * (1 + SIZE * 4));
for (let y = 0; y < SIZE; y += 1) {
  const rowStart = y * (1 + SIZE * 4);
  raw[rowStart] = 0;
  for (let x = 0; x < SIZE; x += 1) {
    const p = rowStart + 1 + x * 4;
    raw[p] = GREY;
    raw[p + 1] = GREY;
    raw[p + 2] = GREY;
    raw[p + 3] = 0xff;
  }
}

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(raw, { level: 9 })),
  chunk('IEND', Buffer.alloc(0)),
]);

mkdirSync(OUT_DIR, { recursive: true });
const out = join(OUT_DIR, 'source.png');
writeFileSync(out, png);
console.log(`wrote ${out} (${SIZE}×${SIZE}, placeholder — replace when the icon handoff lands)`);
