// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Verify the generated brand rasters by decoding them back out, not by looking at them.
//
// This exists because the bug it guards against was invisible to inspection. Every
// frame of the previous `icon.ico` decoded to correct artwork with a clean alpha
// channel — and Windows still drew a black square on the desktop and in the taskbar,
// because all ten frames were PNG-compressed and the shell's legacy loader cannot read
// PNG inside an ICO below 256x256. The artwork was right and the container was wrong,
// which no amount of squinting at a dumped frame would have revealed.
//
// So the checks are about the container as much as the pixels:
//
//   1. Every size Windows asks for is present.
//   2. Sub-256 frames are DIB; 256 is PNG.
//   3. Each DIB's `biHeight` is twice its width — the XOR-plus-AND-mask convention.
//   4. Decoded pixels have a real alpha channel and clear corners at every size.
//   5. The mark is actually drawn — a frame of flat violet would pass 1 to 4.
//   6. The installer bitmaps are the exact dimensions NSIS and WiX require.
//
// Exit code is non-zero on any failure, so it can gate a build.

import { readFileSync, existsSync } from 'node:fs';
import { inflateSync } from 'node:zlib';

const ROOT = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const PNG_SIG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/** Sizes the Windows shell requests across its DPI scales. */
const REQUIRED_ICO = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];

const problems = [];
const note = (m) => problems.push(m);

// ── decoders ────────────────────────────────────────────────────────────────

function decodePng(buf) {
  let p = 8;
  let w = 0;
  let h = 0;
  let depth = 0;
  let colour = 0;
  const idat = [];
  while (p + 8 <= buf.length) {
    const len = buf.readUInt32BE(p);
    const type = buf.toString('ascii', p + 4, p + 8);
    const data = buf.subarray(p + 8, p + 8 + len);
    if (type === 'IHDR') {
      w = data.readUInt32BE(0);
      h = data.readUInt32BE(4);
      depth = data[8];
      colour = data[9];
    } else if (type === 'IDAT') idat.push(data);
    else if (type === 'IEND') break;
    p += 12 + len;
  }
  if (depth !== 8 || colour !== 6)
    return { w, h, bad: `depth ${depth} colour ${colour}, expected 8/6` };
  const raw = inflateSync(Buffer.concat(idat));
  const stride = w * 4;
  const px = Buffer.alloc(h * stride);
  let q = 0;
  for (let y = 0; y < h; y += 1) {
    const filter = raw[q];
    q += 1;
    const line = raw.subarray(q, q + stride);
    q += stride;
    const prev = y === 0 ? Buffer.alloc(stride) : px.subarray((y - 1) * stride, y * stride);
    const cur = px.subarray(y * stride, (y + 1) * stride);
    for (let x = 0; x < stride; x += 1) {
      const a = x >= 4 ? cur[x - 4] : 0;
      const b = prev[x];
      const c = x >= 4 ? prev[x - 4] : 0;
      let v = line[x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const pp = a + b - c;
        const pa = Math.abs(pp - a);
        const pb = Math.abs(pp - b);
        const pc = Math.abs(pp - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      cur[x] = v & 0xff;
    }
  }
  return { w, h, px };
}

function decodeDib(buf) {
  const headerSize = buf.readUInt32LE(0);
  const w = buf.readInt32LE(4);
  const hTotal = buf.readInt32LE(8);
  const bpp = buf.readUInt16LE(14);
  const h = Math.abs(hTotal) / 2;
  if (bpp !== 32) return { w, h, bpp, hTotal, bad: `${bpp}bpp, expected 32` };
  const px = Buffer.alloc(w * h * 4);
  const stride = w * 4;
  for (let row = 0; row < h; row += 1) {
    const y = h - 1 - row;
    for (let x = 0; x < w; x += 1) {
      const i = headerSize + row * stride + x * 4;
      const o = (y * w + x) * 4;
      px[o] = buf[i + 2];
      px[o + 1] = buf[i + 1];
      px[o + 2] = buf[i];
      px[o + 3] = buf[i + 3];
    }
  }
  return { w, h, bpp, hTotal, px };
}

/** Alpha and colour facts a correct frame must satisfy. */
function describe(px, w, h) {
  const at = (x, y) => {
    const o = (y * w + x) * 4;
    return [px[o], px[o + 1], px[o + 2], px[o + 3]];
  };
  const corners = [at(0, 0), at(w - 1, 0), at(0, h - 1), at(w - 1, h - 1)];
  let transparent = 0;
  let white = 0;
  let violet = 0;
  for (let o = 0; o < px.length; o += 4) {
    const a = px[o + 3];
    if (a < 8) {
      transparent += 1;
      continue;
    }
    if (px[o] > 235 && px[o + 1] > 235 && px[o + 2] > 235) white += 1;
    else if (px[o + 2] > px[o] && px[o + 2] > 120) violet += 1;
  }
  return {
    corners,
    openCorners: corners.filter((c) => c[3] < 8).length,
    transparent,
    white,
    violet,
    total: w * h,
  };
}

// ── 1-5: the ICO ────────────────────────────────────────────────────────────

function checkIco(path, required, label) {
  console.log(`\n${label}`);
  if (!existsSync(path)) {
    note(`${label}: missing`);
    return;
  }
  const b = readFileSync(path);
  if (b.readUInt16LE(0) !== 0 || b.readUInt16LE(2) !== 1) {
    note(`${label}: not an ICO`);
    return;
  }
  const count = b.readUInt16LE(4);
  const seen = [];
  for (let i = 0; i < count; i += 1) {
    const e = 6 + i * 16;
    const declared = b.readUInt8(e) || 256;
    const size = b.readUInt32LE(e + 8);
    const off = b.readUInt32LE(e + 12);
    const data = b.subarray(off, off + size);
    const isPng = data.subarray(0, 8).equals(PNG_SIG);
    seen.push(declared);

    // 2. Container per size.
    const wantPng = declared >= 256;
    if (isPng !== wantPng) {
      note(
        `${label} ${declared}px: stored as ${isPng ? 'PNG' : 'BMP'}, must be ${wantPng ? 'PNG' : 'BMP'} — ` +
          `the Windows shell's legacy loader cannot read PNG inside an ICO below 256px and falls back to black or a rescale`,
      );
    }

    const dec = isPng ? decodePng(data) : decodeDib(data);
    if (dec.bad) {
      note(`${label} ${declared}px: ${dec.bad}`);
      continue;
    }
    if (dec.w !== declared || dec.h !== declared) {
      note(`${label} ${declared}px: decodes to ${dec.w}x${dec.h}`);
      continue;
    }
    // 3. The stacked-height convention.
    if (!isPng && dec.hTotal !== declared * 2) {
      note(
        `${label} ${declared}px: biHeight is ${dec.hTotal}, must be ${declared * 2} (XOR + AND mask)`,
      );
    }

    const d = describe(dec.px, dec.w, dec.h);
    // 4. Real alpha, clear corners.
    if (d.openCorners !== 4) {
      note(
        `${label} ${declared}px: ${4 - d.openCorners} corner(s) opaque ${JSON.stringify(d.corners)} — ` +
          `artwork flattened onto a background`,
      );
    }
    if (d.transparent === 0) note(`${label} ${declared}px: no transparent pixel anywhere`);
    // 5. The mark is drawn, not a flat disc.
    const inkPct = (d.white / Math.max(1, d.total - d.transparent)) * 100;
    if (inkPct < 4)
      note(
        `${label} ${declared}px: only ${inkPct.toFixed(1)}% ink — the mark is missing or too faint`,
      );
    if (d.violet === 0) note(`${label} ${declared}px: no violet ground`);

    console.log(
      `  ${String(declared).padStart(3)}px ${(isPng ? 'PNG' : 'BMP').padEnd(3)} ` +
        `${String(size).padStart(7)}B  alpha ${((d.transparent / d.total) * 100).toFixed(1)}% clear, ` +
        `ink ${inkPct.toFixed(1)}% of disc`,
    );
  }
  // 1. Completeness.
  for (const want of required) {
    if (!seen.includes(want))
      note(`${label}: no ${want}px frame — the shell will rescale a neighbour`);
  }
  const extra = seen.filter((s) => !required.includes(s));
  if (extra.length) console.log(`  (also present: ${extra.join(', ')})`);
}

checkIco(`${ROOT}/src-tauri/icons/icon.ico`, REQUIRED_ICO, 'src-tauri/icons/icon.ico');
checkIco(`${ROOT}/public/favicon.ico`, [16, 32], 'public/favicon.ico');

// ── the ICNS and the loose PNGs ─────────────────────────────────────────────

console.log('\nsrc-tauri/icons/icon.icns');
{
  const b = readFileSync(`${ROOT}/src-tauri/icons/icon.icns`);
  if (b.toString('ascii', 0, 4) !== 'icns') note('icns: bad magic');
  if (b.readUInt32BE(4) !== b.length)
    note(`icns: declared length ${b.readUInt32BE(4)} != actual ${b.length}`);
  const want = { icp4: 16, icp5: 32, icp6: 64, ic07: 128, ic08: 256, ic09: 512, ic10: 1024 };
  let p = 8;
  const found = {};
  while (p + 8 <= b.length) {
    const type = b.toString('ascii', p, p + 4);
    const len = b.readUInt32BE(p + 4);
    const data = b.subarray(p + 8, p + len);
    if (!data.subarray(0, 8).equals(PNG_SIG)) note(`icns ${type}: not PNG`);
    else {
      const dec = decodePng(data);
      found[type] = dec.w;
      if (want[type] && dec.w !== want[type])
        note(`icns ${type}: ${dec.w}px, expected ${want[type]}`);
      const d = describe(dec.px, dec.w, dec.h);
      if (d.openCorners !== 4) note(`icns ${type}: corners opaque — flattened`);
      console.log(`  ${type} ${String(dec.w).padStart(4)}px ${String(len).padStart(7)}B  alpha ok`);
    }
    p += len;
  }
  for (const t of Object.keys(want)) if (!(t in found)) note(`icns: missing ${t}`);
}

console.log('\nPNGs listed in tauri.conf.json');
{
  const conf = JSON.parse(readFileSync(`${ROOT}/src-tauri/tauri.conf.json`, 'utf8'));
  for (const rel of conf.bundle.icon) {
    const p = `${ROOT}/src-tauri/${rel}`;
    if (!existsSync(p)) {
      note(`tauri.conf icon missing: ${rel}`);
      continue;
    }
    if (!rel.endsWith('.png')) continue;
    const dec = decodePng(readFileSync(p));
    if (dec.bad) {
      note(`${rel}: ${dec.bad}`);
      continue;
    }
    const d = describe(dec.px, dec.w, dec.h);
    if (d.openCorners !== 4) note(`${rel}: corners opaque — flattened onto a background`);
    console.log(
      `  ${rel.padEnd(24)} ${dec.w}x${dec.h}  alpha ${((d.transparent / d.total) * 100).toFixed(1)}% clear`,
    );
  }
}

// ── 6: the installer bitmaps ────────────────────────────────────────────────

console.log('\ninstaller branding bitmaps');
const BITMAPS = {
  'nsis-header.bmp': [150, 57],
  'nsis-sidebar.bmp': [164, 314],
  'wix-banner.bmp': [493, 58],
  'wix-dialog.bmp': [493, 312],
};
for (const [name, [w, h]] of Object.entries(BITMAPS)) {
  const p = `${ROOT}/src-tauri/installer/${name}`;
  if (!existsSync(p)) {
    note(`installer bitmap missing: ${name}`);
    continue;
  }
  const b = readFileSync(p);
  if (b.toString('ascii', 0, 2) !== 'BM') {
    note(`${name}: not a BMP`);
    continue;
  }
  const gw = b.readInt32LE(18);
  const gh = b.readInt32LE(22);
  const bpp = b.readUInt16LE(28);
  if (gw !== w || gh !== h)
    note(`${name}: ${gw}x${gh}, must be exactly ${w}x${h} — neither NSIS nor WiX scales`);
  if (bpp !== 24) note(`${name}: ${bpp}bpp, expected 24 (no alpha channel)`);
  console.log(`  ${name.padEnd(20)} ${gw}x${gh} ${bpp}bpp  ${String(b.length).padStart(7)}B`);
}

// ── 7: the wizard can be read ───────────────────────────────────────────────
//
// WixUI draws its own title and description as black text straight onto these two
// bitmaps, and there is no way to recolour that text short of replacing the whole
// WixUI theme. Both bitmaps were the app's dark surface end to end, which made every
// word of the MSI wizard invisible — so the regions the text lands in are checked
// rather than trusted to whoever next regenerates the art.
//
// The rectangles come from WixUI's own control geometry, converted at the 1.3324
// px-per-dialog-unit the bitmaps are stretched by: WelcomeDlg and ExitDlg write from
// X=135 DTU, and the interior dialogs write from X=15 to about X=305.

console.log('\nwizard text legibility');

/** WCAG relative luminance of an 8-bit channel triple. */
function luminance([r, g, b]) {
  const lin = (v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** Contrast of pure black text against a background colour. */
const againstBlack = (rgb) => (luminance(rgb) + 0.05) / 0.05;

/** WCAG AA for the large text a wizard title is; the body text clears it too. */
const MIN_CONTRAST = 4.5;

const TEXT_AREAS = {
  // name: [x0, y0, x1, y1] in bitmap pixels.
  'wix-dialog.bmp': ['welcome title and description', 180, 20, 493, 200],
  'wix-banner.bmp': ['interior title and description', 20, 6, 406, 50],
};

for (const [name, [what, x0, y0, x1, y1]] of Object.entries(TEXT_AREAS)) {
  const p = `${ROOT}/src-tauri/installer/${name}`;
  if (!existsSync(p)) continue;
  const b = readFileSync(p);
  const offset = b.readUInt32LE(10);
  const w = b.readInt32LE(18);
  const h = b.readInt32LE(22);
  const stride = Math.ceil((w * 3) / 4) * 4;

  let worst = Infinity;
  let at = null;
  for (let y = y0; y < Math.min(y1, h); y += 1) {
    for (let x = x0; x < Math.min(x1, w); x += 1) {
      // BMPs are bottom-up and BGR.
      const i = offset + (h - 1 - y) * stride + x * 3;
      const contrast = againstBlack([b[i + 2], b[i + 1], b[i]]);
      if (contrast < worst) {
        worst = contrast;
        at = [x, y];
      }
    }
  }
  if (worst < MIN_CONTRAST) {
    note(
      `${name}: the wizard's ${what} would sit on ${worst.toFixed(1)}:1 at ${at?.join(',')} — ` +
        `WixUI writes that text in black and cannot be told otherwise, so this region must ` +
        `stay at ${String(MIN_CONTRAST)}:1 or better`,
    );
  }
  console.log(`  ${name.padEnd(20)} ${what.padEnd(31)} worst ${worst.toFixed(1)}:1`);
}

// ── verdict ─────────────────────────────────────────────────────────────────
console.log('');
if (problems.length) {
  console.log(`check:brand-icons — ${problems.length} problem(s):\n`);
  for (const p of problems) console.log(`  - ${p}`);
  process.exit(1);
}
console.log('check:brand-icons — every frame, container and dimension verified by decoding');
