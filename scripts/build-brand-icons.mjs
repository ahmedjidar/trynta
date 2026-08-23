// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Generate every brand raster from the mark's own geometry: the app icon set, the
// favicon, the Store tiles, and the installer branding bitmaps.
//
// Run with `pnpm icons:brand`. Committed rather than kept in a scratch directory
// because the assets it writes are committed, and an asset nobody can regenerate is an
// asset nobody can change.
//
// ## Why this rasterises rather than calling a converter
//
// Every frame is drawn from the mark's parameters at its exact final pixel size.
// Nothing is ever downscaled from a larger raster, which is the usual source of a
// mushy 16px icon: a 256px master resampled six times looks like six different
// degrees of blur. Two circular arcs and a disc do not need a rendering engine, and
// doing it here means no image toolchain to install and no process left running.
//
// ## The ICO container, which is where the bug was
//
// **Frames below 256px are BMP (DIB); 256 is PNG.** This is not a style choice.
// PNG-compressed frames inside an ICO are only reliably supported by Windows at
// 256x256. The shell paths that draw desktop Medium Icons and taskbar buttons go
// through the older loader, which expects a DIB — hand it PNG and it either drops the
// alpha, compositing the mark onto black, or fails the frame entirely and rescales a
// neighbour instead. Those are exactly the two symptoms that prompted this file: a
// black square at some sizes and blur at others, from one cause.
//
// The 32bpp DIB frames carry a real AND mask derived from the alpha channel as well as
// the alpha itself, so a legacy path that consults the mask gets the right answer
// rather than a rectangle.

import { deflateSync } from 'node:zlib';
import { writeFileSync, mkdirSync } from 'node:fs';

const ROOT = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');

// ── Colours ─────────────────────────────────────────────────────────────────
//
// The circle is the brand accent held at its hue and saturation and taken down in
// lightness: `--accent` is hsl(236, 78%, 64%) in light mode, this is hsl(243, 80%,
// 60%). Blue-violet, a step darker, still fully saturated so it holds against a light
// and a dark taskbar alike.
const VIOLET = [0x4f, 0x47, 0xeb];
/** White: the mark has to read at 16px on a saturated ground, and a tint costs contrast. */
const INK = [0xff, 0xff, 0xff];

// ── The mark, from public/trynta/brand/trynta-mark.svg ──────────────────────
const VIEW_W = 122;
const VIEW_H = 76;
const RING_R = 32.5;
const STROKE = 11;
const RINGS = [
  // Left ring, centre (38,38), open across the lower crossing at 45°.
  { cx: 38, cy: 38, gapFrom: 28.25, gapTo: 61.75 },
  // Right ring, centre (84,38), open across the upper crossing at 225°.
  { cx: 84, cy: 38, gapFrom: 208.25, gapTo: 241.75 },
];

/**
 * Optical sizing: how the mark is drawn at each icon size.
 *
 * The rings are a thin-stroke construction — 11 units against a 122-wide viewBox — so
 * at one fixed ratio the stroke lands on about one pixel at 16px, straddles the grid
 * and antialiases to grey. The small frames are therefore hinted: the mark grows and
 * its stroke thickens, trading fidelity to the construction for legibility at sizes
 * where the construction is invisible anyway. 96px and up carry the true proportions.
 */
function optical(size) {
  if (size <= 20) return { scale: 0.84, stroke: 15 };
  if (size <= 32) return { scale: 0.82, stroke: 13.5 };
  if (size <= 64) return { scale: 0.76, stroke: 12 };
  return { scale: 0.72, stroke: STROKE };
}

/** Subsamples per axis. 64 samples per pixel; 16 was visibly noisy on the small frames. */
const AA = 8;

/** Whether `deg` falls in a ring's gap, where the other ring passes over it. */
function inGap(deg, from, to) {
  const d = ((deg % 360) + 360) % 360;
  return d >= from && d <= to;
}

/** Whether an SVG-space point lies on the mark's stroke. */
function onMark(sx, sy, stroke) {
  const half = stroke / 2;
  for (const ring of RINGS) {
    const dx = sx - ring.cx;
    const dy = sy - ring.cy;
    if (Math.abs(Math.hypot(dx, dy) - RING_R) > half) continue;
    // atan2 on SVG coordinates, which run y-down — the frame the path data is written
    // in, so these angles are the ones in the SVG's own comment.
    if (!inGap((Math.atan2(dy, dx) * 180) / Math.PI, ring.gapFrom, ring.gapTo)) return true;
  }
  return false;
}

/**
 * Render one square RGBA frame at exactly `size` pixels.
 *
 * @param bleed 1 for a circle that fills the frame; less to inset it.
 */
function render(size, bleed = 1) {
  const px = Buffer.alloc(size * size * 4);
  const { scale, stroke } = optical(size);
  const centre = size / 2;
  const radius = (size / 2) * bleed;
  const markW = size * scale * bleed;
  const markH = (markW * VIEW_H) / VIEW_W;
  const x0 = (size - markW) / 2;
  const y0 = (size - markH) / 2;
  const k = markW / VIEW_W;
  const samples = AA * AA;

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      let disc = 0;
      let mark = 0;
      for (let sy = 0; sy < AA; sy += 1) {
        for (let sx = 0; sx < AA; sx += 1) {
          const px_ = x + (sx + 0.5) / AA;
          const py_ = y + (sy + 0.5) / AA;
          if (Math.hypot(px_ - centre, py_ - centre) > radius) continue;
          disc += 1;
          if (onMark((px_ - x0) / k, (py_ - y0) / k, stroke)) mark += 1;
        }
      }
      if (disc === 0) continue;
      const a = disc / samples;
      const m = mark / samples;
      const i = (y * size + x) * 4;
      // Composite premultiplied then divide back out: violet covers `a - m`, ink `m`.
      for (let c = 0; c < 3; c += 1) {
        px[i + c] = Math.round((VIOLET[c] * (a - m) + INK[c] * m) / a);
      }
      px[i + 3] = Math.round(a * 255);
    }
  }
  return { size, px };
}

/** Composite an RGBA frame over an opaque colour — for the installer bitmaps, which have no alpha. */
function flatten(width, height, draw, bg) {
  const px = Buffer.alloc(width * height * 4);
  for (let i = 0; i < px.length; i += 4) {
    px[i] = bg[0];
    px[i + 1] = bg[1];
    px[i + 2] = bg[2];
    px[i + 3] = 255;
  }
  draw(px, width, height);
  return px;
}

/** Blit an RGBA frame onto a canvas at (ox, oy), alpha-over. */
function blit(dst, dw, src, sw, ox, oy) {
  for (let y = 0; y < sw; y += 1) {
    for (let x = 0; x < sw; x += 1) {
      const s = (y * sw + x) * 4;
      const a = src[s + 3] / 255;
      if (a === 0) continue;
      const d = ((oy + y) * dw + ox + x) * 4;
      for (let c = 0; c < 3; c += 1) dst[d + c] = Math.round(src[s + c] * a + dst[d + c] * (1 - a));
      dst[d + 3] = 255;
    }
  }
}

// ── PNG ─────────────────────────────────────────────────────────────────────
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
  let c = -1;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}

function png({ size, px }, width = size, height = size) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.writeUInt8(8, 8); // bit depth
  ihdr.writeUInt8(6, 9); // RGBA
  const stride = width * 4;
  const raw = Buffer.alloc(height * (stride + 1));
  for (let y = 0; y < height; y += 1) {
    raw[y * (stride + 1)] = 0; // filter: none
    px.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ── BMP, in two shapes ──────────────────────────────────────────────────────

/**
 * A 32bpp DIB for an ICO frame: BITMAPINFOHEADER, bottom-up BGRA, then an AND mask.
 *
 * `biHeight` is **twice** the real height because the header describes the XOR image
 * and the AND mask stacked. Getting that wrong is the classic way to produce a frame
 * Windows renders as a rectangle.
 *
 * The mask is derived from the alpha rather than zeroed. A 32bpp icon should be drawn
 * from its alpha channel, but the legacy paths that read the mask are the same ones
 * that cannot read PNG — so they are exactly the paths that need it to be right.
 */
function dibFrame({ size, px }) {
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0); // biSize
  header.writeInt32LE(size, 4); // biWidth
  header.writeInt32LE(size * 2, 8); // biHeight: XOR + AND
  header.writeUInt16LE(1, 12); // biPlanes
  header.writeUInt16LE(32, 14); // biBitCount
  header.writeUInt32LE(0, 16); // biCompression = BI_RGB

  const xorStride = size * 4;
  const xor = Buffer.alloc(size * xorStride);
  // 1bpp mask rows are padded to a 4-byte boundary.
  const maskStride = Math.ceil(size / 32) * 4;
  const mask = Buffer.alloc(size * maskStride);

  for (let row = 0; row < size; row += 1) {
    const y = size - 1 - row; // DIBs are bottom-up
    for (let x = 0; x < size; x += 1) {
      const s = (y * size + x) * 4;
      const d = row * xorStride + x * 4;
      xor[d] = px[s + 2]; // B
      xor[d + 1] = px[s + 1]; // G
      xor[d + 2] = px[s]; // R
      xor[d + 3] = px[s + 3]; // A
      // Mask bit set = transparent. Threshold at half, which is what the legacy
      // renderer's 1-bit view of a soft edge can express.
      if (px[s + 3] < 128) {
        mask[row * maskStride + (x >> 3)] |= 0x80 >> (x & 7);
      }
    }
  }
  header.writeUInt32LE(xor.length + mask.length, 20); // biSizeImage
  return Buffer.concat([header, xor, mask]);
}

/** A plain 24bpp BMP file, for the installer bitmaps — NSIS and WiX both want no alpha. */
function bmpFile(width, height, px) {
  const rowStride = Math.ceil((width * 3) / 4) * 4;
  const pixels = Buffer.alloc(height * rowStride);
  for (let row = 0; row < height; row += 1) {
    const y = height - 1 - row; // bottom-up
    for (let x = 0; x < width; x += 1) {
      const s = (y * width + x) * 4;
      const d = row * rowStride + x * 3;
      pixels[d] = px[s + 2];
      pixels[d + 1] = px[s + 1];
      pixels[d + 2] = px[s];
    }
  }
  const header = Buffer.alloc(14);
  const info = Buffer.alloc(40);
  header.write('BM', 0, 'ascii');
  header.writeUInt32LE(14 + 40 + pixels.length, 2);
  header.writeUInt32LE(14 + 40, 10);
  info.writeUInt32LE(40, 0);
  info.writeInt32LE(width, 4);
  info.writeInt32LE(height, 8);
  info.writeUInt16LE(1, 12);
  info.writeUInt16LE(24, 14);
  info.writeUInt32LE(pixels.length, 20);
  return Buffer.concat([header, info, pixels]);
}

// ── ICO ─────────────────────────────────────────────────────────────────────
/**
 * Assemble an ICO. Frames below 256 go in as DIB; 256 goes in as PNG.
 *
 * See the module note: this split is the whole point of the file.
 */
function ico(sizes, frames) {
  const entries = [];
  const images = [];
  let offset = 6 + sizes.length * 16;
  for (const size of sizes) {
    const data = size >= 256 ? png(frames[size]) : dibFrame(frames[size]);
    const e = Buffer.alloc(16);
    e.writeUInt8(size >= 256 ? 0 : size, 0); // 0 means 256
    e.writeUInt8(size >= 256 ? 0 : size, 1);
    e.writeUInt16LE(1, 4); // colour planes
    e.writeUInt16LE(32, 6); // bits per pixel
    e.writeUInt32LE(data.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += data.length;
    entries.push(e);
    images.push(data);
  }
  const header = Buffer.alloc(6);
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(sizes.length, 4);
  return Buffer.concat([header, ...entries, ...images]);
}

// ── ICNS ────────────────────────────────────────────────────────────────────
//
// UNVERIFIED on macOS: written from the format description, and no Apple tool has read
// it. MACOS-UNVERIFIED.md carries the check.
function icns(entries, frames) {
  const chunks = [];
  for (const [type, size] of entries) {
    const data = png(frames[size]);
    const head = Buffer.alloc(8);
    head.write(type, 0, 4, 'ascii');
    head.writeUInt32BE(data.length + 8, 4);
    chunks.push(head, data);
  }
  const body = Buffer.concat(chunks);
  const head = Buffer.alloc(8);
  head.write('icns', 0, 4, 'ascii');
  head.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([head, body]);
}

// ── Render every size once ──────────────────────────────────────────────────
//
// The ICO set is the one Windows actually asks for across its DPI scales. A size the
// shell wants and cannot find is resampled from a neighbour, which is its own source
// of blur on top of the container problem.
const ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];
const ALL_SIZES = [...new Set([...ICO_SIZES, 512, 1024])].sort((a, b) => a - b);

const frames = {};
for (const size of ALL_SIZES) {
  frames[size] = render(size);
}

const iconsDir = `${ROOT}/src-tauri/icons`;
mkdirSync(iconsDir, { recursive: true });
const write = (name, buf) => {
  writeFileSync(`${iconsDir}/${name}`, buf);
  console.log(`  ${name.padEnd(26)} ${String(buf.length).padStart(7)} B`);
};

console.log('app icon set');
write('32x32.png', png(frames[32]));
write('64x64.png', png(frames[64]));
write('128x128.png', png(frames[128]));
write('128x128@2x.png', png(frames[256]));
write('icon.png', png(frames[512]));
write('source.png', png(frames[1024]));
write('icon.ico', ico(ICO_SIZES, frames));
write(
  'icon.icns',
  icns(
    [
      ['icp4', 16],
      ['icp5', 32],
      ['icp6', 64],
      ['ic07', 128],
      ['ic08', 256],
      ['ic09', 512],
      ['ic10', 1024],
    ],
    frames,
  ),
);

// ── Store tiles ─────────────────────────────────────────────────────────────
//
// Inset and transparent rather than full-bleed: a Start-menu tile paints its own
// ground, so a circle touching the tile edges reads as a crop. Not built by the
// current bundle targets, but leaving them stale while everything else changed would
// be worse.
console.log('\nStore tiles (inset, transparent)');
const TILES = {
  'Square30x30Logo.png': 30,
  'Square44x44Logo.png': 44,
  'Square71x71Logo.png': 71,
  'Square89x89Logo.png': 89,
  'Square107x107Logo.png': 107,
  'Square142x142Logo.png': 142,
  'Square150x150Logo.png': 150,
  'Square284x284Logo.png': 284,
  'Square310x310Logo.png': 310,
  'StoreLogo.png': 50,
};
for (const [name, size] of Object.entries(TILES)) {
  write(name, png(render(size, 0.66)));
}

// ── favicon ─────────────────────────────────────────────────────────────────
writeFileSync(`${ROOT}/public/favicon.ico`, ico([16, 20, 24, 32, 48], frames));
console.log(`\n  public/favicon.ico`);

// ── Installer branding ──────────────────────────────────────────────────────
//
// NSIS and WiX both want opaque BMPs at exact sizes; neither scales gracefully, which
// is why each is composed at its final dimensions rather than resized from one master.
//
// ## These have to be light where the wizard writes on them
//
// Every one of these was the app's dark surface end to end, and it made the MSI
// wizard unreadable: WixUI draws its own title and description as **black** text
// directly over the banner and the dialog bitmap, with no way to recolour it short
// of replacing the whole WixUI theme. Black on `#080a0f` is nothing at all.
//
// So each bitmap is composed around where its wizard puts text, and the geometry is
// not guesswork — WixUI positions controls in dialog units, and the bitmaps are
// stretched from 493px to 370 DTU, a factor of 1.3324:
//
//   * WelcomeDlg / ExitDlg put their title at X=135 DTU and their description at the
//     same, which is 180px into a 493px-wide dialog bitmap. Everything from there
//     rightwards is paper; the brand band gets the 164px to its left, with 16px of
//     margin before the first glyph.
//   * The interior dialogs put their title at X=15 DTU and description at X=25, both
//     running to about X=305 — 20px to 406px across the banner. So the banner is
//     paper with the mark in the right-hand margin, which is also where every
//     Windows installer has always put it.
//
// NSIS needs none of that: MUI2 draws its header text beside the header bitmap
// rather than on top of it, and nothing at all is drawn over the welcome sidebar.
// The sidebar therefore keeps the dark ground — it is the one panel in either wizard
// that can carry the brand at full strength, and it matches the MSI's left band.
const SURFACE = [0x08, 0x0a, 0x0f];
/** `--surface-panel` in the light theme: what the wizard's black text has to sit on. */
const PAPER = [0xff, 0xff, 0xff];
/** `--border-hairline` composited over paper, for the one rule that separates them. */
const HAIRLINE = [0xee, 0xee, 0xef];

/** Where the MSI's welcome text starts, in pixels of a 493px-wide dialog bitmap. */
const WIX_DIALOG_TEXT_X = 180;
/** Paper between the brand band and that first glyph. */
const WIX_DIALOG_MARGIN = 16;
/** The brand band's width, stated as the thing it must not reach. */
const WIX_DIALOG_BAND = WIX_DIALOG_TEXT_X - WIX_DIALOG_MARGIN;
/** Where the MSI's interior-dialog text ends. The mark goes to the right of it. */
const WIX_BANNER_TEXT_END = 406;

const brandDir = `${ROOT}/src-tauri/installer`;
mkdirSync(brandDir, { recursive: true });

/** Paint an axis-aligned rectangle of one opaque colour. */
function fill(dst, dw, colour, x0, y0, w, h) {
  for (let y = y0; y < y0 + h; y += 1) {
    for (let x = x0; x < x0 + w; x += 1) {
      const i = (y * dw + x) * 4;
      dst[i] = colour[0];
      dst[i + 1] = colour[1];
      dst[i + 2] = colour[2];
      dst[i + 3] = 255;
    }
  }
}

const branding = {
  // MUI_HEADERIMAGE_BITMAP — 150x57, beside the page title on every page after the
  // welcome. Paper, because the header it sits in is white and a dark tile in the
  // corner of a white strip reads as a rendering fault rather than as a logo.
  'nsis-header.bmp': () => {
    const edge = 38;
    const mark = render(edge);
    return bmpFile(
      150,
      57,
      flatten(
        150,
        57,
        (dst) => {
          blit(dst, 150, mark.px, edge, 16, Math.round((57 - edge) / 2));
          fill(dst, 150, HAIRLINE, 0, 56, 150, 1);
        },
        PAPER,
      ),
    );
  },

  // MUI_WELCOMEFINISHPAGE_BITMAP — 164x314, the welcome and finish sidebar. Nothing
  // is drawn over it, so this is the one panel that keeps the dark ground.
  'nsis-sidebar.bmp': () => {
    const edge = 96;
    const mark = render(edge);
    return bmpFile(
      164,
      314,
      flatten(
        164,
        314,
        (dst) => blit(dst, 164, mark.px, edge, Math.round((164 - edge) / 2), 54),
        SURFACE,
      ),
    );
  },

  // WiX banner — 493x58, above the title and description of every interior dialog.
  // Paper under the text, mark in the right-hand margin, hairline along the bottom
  // so the banner reads as a band rather than as an area that failed to paint.
  'wix-banner.bmp': () => {
    const edge = 40;
    const mark = render(edge);
    return bmpFile(
      493,
      58,
      flatten(
        493,
        58,
        (dst) => {
          const ox = WIX_BANNER_TEXT_END + Math.round((493 - WIX_BANNER_TEXT_END - edge) / 2);
          blit(dst, 493, mark.px, edge, ox, Math.round((58 - edge) / 2));
          fill(dst, 493, HAIRLINE, 0, 57, 493, 1);
        },
        PAPER,
      ),
    );
  },

  // WiX dialog — 493x312, the whole background of the welcome and completion pages.
  // A branded band down the left, paper from there so the black title and the black
  // description both have something to be read against.
  'wix-dialog.bmp': () => {
    const edge = 110;
    const mark = render(edge);
    return bmpFile(
      493,
      312,
      flatten(
        493,
        312,
        (dst) => {
          fill(dst, 493, SURFACE, 0, 0, WIX_DIALOG_BAND, 312);
          fill(dst, 493, HAIRLINE, WIX_DIALOG_BAND, 0, 1, 312);
          blit(
            dst,
            493,
            mark.px,
            edge,
            Math.round((WIX_DIALOG_BAND - edge) / 2),
            Math.round((312 - edge) / 2),
          );
        },
        PAPER,
      ),
    );
  },
};

console.log('\ninstaller branding');
for (const [name, make] of Object.entries(branding)) {
  const buf = make();
  writeFileSync(`${brandDir}/${name}`, buf);
  console.log(`  ${name.padEnd(26)} ${String(buf.length).padStart(7)} B`);
}

console.log('\ndone — verify with: node scripts/check-brand-icons.mjs');
