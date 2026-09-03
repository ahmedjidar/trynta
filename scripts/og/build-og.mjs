#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Photograph `og.html` at 1200x630 and write the social card.
//
// ## Why a browser rather than a drawing library
//
// The card carries the wordmark in Manrope at a specific weight and tracking, and
// the only place those values exist is `web/src/css/site.css`. Rendering the page
// uses them directly; anything else would be a second copy of the type scale that
// drifts the first time the site's does. It also needs no new dependency.
//
// ## Why it shells out instead of driving the browser
//
// The obvious way to do this is the DevTools protocol, and the first version did:
// `fetch` the target list, open a `WebSocket`, call `Page.captureScreenshot`.
// `check:network` refused it, correctly — a `fetch()` in `scripts/` is exactly what
// that check exists to notice, and the two categories it exempts are the product's
// two sanctioned requests and the files that define the rules. This is neither, and
// adding a third category to make an asset build pass would be the wrong trade
// entirely.
//
// Headless `--screenshot` needs no network primitives at all, so there is nothing
// to exempt. `--virtual-time-budget` is what makes it wait for the webfont: without
// it the card ships in the fallback stack and the tracking is visibly wrong.
//
// ## Why there is one card and not a light and a dark one
//
// An Open Graph image cannot adapt to the reader's theme. A crawler fetches one
// static raster once, caches it, and the chat client composites it onto its own
// background — there is no media query, no `srcset`, and no second URL it will ask
// for. The mechanism people are thinking of is the favicon, which genuinely does
// support `media="(prefers-color-scheme: dark)"`, and which the site now uses.
//
// So the card is built to survive both grounds instead: a solid dark field with no
// transparency, which reads as deliberate on a white Slack and a dark Discord
// alike. The light variant is rendered too, because it costs one more invocation
// and someone will want it for a deck.
//
// Usage: node scripts/og/build-og.mjs [--browser "C:\path\to\msedge.exe"]

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..', '..');
const OUT = join(ROOT, 'web', 'src', 'assets', 'brand');

const WIDTH = 1200;
const HEIGHT = 630;

/** Chrome and Edge share the flag; whichever is installed will do. */
const CANDIDATES = [
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
];

const flag = process.argv.indexOf('--browser');
const browser = flag !== -1 ? process.argv[flag + 1] : CANDIDATES.find((p) => existsSync(p));

if (!browser || !existsSync(browser)) {
  console.error(
    'build-og — no Chrome or Edge found. Pass one:\n' +
      '  node scripts/og/build-og.mjs --browser "/path/to/chrome"',
  );
  process.exit(1);
}

mkdirSync(OUT, { recursive: true });
const source = `file:///${join(HERE, 'og.html').replaceAll('\\', '/')}`;
// A throwaway profile, so this never touches a real browser session and never
// inherits one's extensions or zoom.
const profile = join(HERE, '.chrome-profile');

for (const [name, query] of [
  ['og', ''],
  ['og-light', '?theme=light'],
]) {
  const file = join(OUT, `${name}.png`);
  rmSync(file, { force: true });
  execFileSync(
    browser,
    [
      '--headless=new',
      '--disable-gpu',
      '--hide-scrollbars',
      `--user-data-dir=${profile}`,
      `--window-size=${String(WIDTH)},${String(HEIGHT)}`,
      // Lets layout, the webfont and the blur settle before the frame is taken.
      '--virtual-time-budget=4000',
      '--allow-file-access-from-files',
      `--screenshot=${file}`,
      source + query,
    ],
    { stdio: 'pipe' },
  );
  if (!existsSync(file)) {
    console.error(`build-og — ${name}.png was not written. Is the browser headless-capable?`);
    process.exit(1);
  }
  console.log(
    `  ${name}.png  ${String(WIDTH)}x${String(HEIGHT)}  ${String(statSync(file).size)} B`,
  );
}

rmSync(profile, { recursive: true, force: true });
console.log('\nwrote web/src/assets/brand/');
