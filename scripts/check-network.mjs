#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// CLAUDE.md §4.7 / ADD-001 / SPEC-V1 §11 AC14: exactly two outbound requests exist
// in this product, and adding a third is a spec change.
//
//   (a) HIBP range queries — k-anonymous, 5 hex characters, `Add-Padding: true`
//   (b) the signed update manifest check
//
// and nothing else — which is a statement about the absence of requests
// rather than a third one.
//
// ADD-001's verification list asks for this in so many words: *"Grep the codebase: no
// icon URL is constructed at runtime, ever."* This is that grep, run in CI.
//
// It is a static check and it does not replace the packet capture AC14 asks for — a
// capture proves what actually left the machine, and this proves nobody has written
// the code that would. They fail differently: a capture catches a dependency phoning
// home, this catches a well-meaning `<img src={faviconUrl}>` in a pull request.
//
// Comments are stripped before scanning, because the modules that hold the sanctioned
// requests document at length what they do and do not do, and a check that flagged its
// own rationale would push people to stop writing it down.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';
import { stripComments } from './lib/strip-comments.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * The only files permitted to reach the network, and what each is for.
 *
 * Anything not on this list that names an HTTP client is a third request.
 */
const SANCTIONED = new Map([
  [join('src-tauri', 'src', 'services', 'hibp.rs'), 'HIBP range queries (SPEC-V1 §7.4)'],
  [join('src-tauri', 'src', 'commands', 'updates.rs'), 'the signed update check (SPEC-V1 §7.7)'],
]);

const SCAN_DIRS = ['src', 'src-tauri/src', 'crates', 'e2e', 'scripts'];
const SCAN_EXT = new Set(['.rs', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.css', '.html']);
const SKIP_DIRS = new Set(['node_modules', 'dist', 'target', 'coverage', '.tsbuild', 'generated']);

/**
 * Hosts that must never appear in shipped source.
 *
 * Split across an array and joined so this file does not trip its own check — the
 * point of the list is that the *literal* must not exist in code, and writing it here
 * as one string would be the thing it forbids.
 */
const FORBIDDEN_HOSTS = [
  ['google', '.com/s2/favicons'], // what the HO-001 prototype used
  ['www.google', '.com/s2'],
  ['favicon', '.yandex.net'],
  ['icons.duckduckgo', '.com'],
  ['logo.clearbit', '.com'],
  ['gstatic', '.com'],
  ['fonts.googleapis', '.com'], // font-src 'self'; a webfont must be bundled
  ['fonts.gstatic', '.com'],
  ['cdn.jsdelivr', '.net'],
  ['unpkg', '.com'],
  ['cdnjs.cloudflare', '.com'],
].map((parts) => parts.join(''));

/**
 * Patterns that mean "this code can make a request", scoped by language.
 *
 * The scoping is not tidiness. `fetch` is a network call in TypeScript and an ordinary
 * method name in Rust — `RangeSource::fetch` is the trait the HIBP transport
 * implements, and flagging it would have taught everyone to ignore this check on its
 * first run, which is how a guard stops guarding.
 */
const RUST = new Set(['.rs']);
const WEB = new Set(['.ts', '.tsx', '.js', '.jsx', '.mjs', '.html']);

const CLIENTS = [
  { name: 'ureq client', re: /\bureq\s*::/, exts: RUST },
  { name: 'reqwest', re: /\breqwest\s*::/, exts: RUST },
  { name: 'hyper client', re: /\bhyper\s*::\s*Client/, exts: RUST },
  { name: 'std TcpStream', re: /\bTcpStream\s*::\s*connect/, exts: RUST },
  { name: 'fetch()', re: /(?<![\w.])fetch\s*\(/, exts: WEB },
  { name: 'XMLHttpRequest', re: /\bXMLHttpRequest\b/, exts: WEB },
  { name: 'EventSource', re: /\bnew\s+EventSource\b/, exts: WEB },
  { name: 'WebSocket', re: /\bnew\s+WebSocket\b/, exts: WEB },
  { name: 'navigator.sendBeacon', re: /\bsendBeacon\s*\(/, exts: WEB },
  { name: 'tauri http plugin', re: /tauri[-_]plugin[-_]http/, exts: new Set([...RUST, ...WEB]) },

  // ── ADD-001: no icon URL is ever constructed at runtime ────────────────────
  //
  // The clients above catch code that *makes* a request. These catch code that *hands a
  // URL to the platform* and lets it make the request — which is how the favicon leak
  // arrives in practice: not `fetch()`, but `<img src={`https://${domain}/favicon.ico`}>`.
  // A URL in an `<img>` is still a packet on the wire, and it still names the service.
  //
  // Web files only. A Rust string containing a URL is a fixture, an error message or the
  // theme validator's own rejection test; the two files that genuinely make requests are
  // on the sanctioned list, and the host list above applies to them regardless.
  //
  // The bundled tiles pass because they are root-relative — `/icons/<key>.svg`, resolved
  // against the app's own origin under `img-src 'self'`. `IconDto` carries a bundle key
  // and never a domain, so there is nothing in the webview to build a remote URL from
  // even by accident; these rules exist so that stays true.
  { name: 'absolute URL in web source', re: /\bhttps?:\/\//, exts: WEB, shipped: true },
  {
    name: 'protocol-relative URL in a src/href',
    re: /\b(?:src|href)\s*=\s*\{?\s*[`"']\s*\/\//,
    exts: WEB,
    shipped: true,
  },
  {
    name: 'favicon probe',
    re: /favicon\.(?:ico|png|svg|jpg)/i,
    exts: new Set([...RUST, ...WEB]),
  },
  {
    name: 'well-known probe',
    re: /\.well-known\/(?:change-password|security\.txt)/i,
    exts: new Set([...RUST, ...WEB]),
  },
];

/**
 * `@import url(...)` and remote `url()` — **stylesheets only**.
 *
 * Not applied to Rust or TypeScript, and the reason is a test that would otherwise
 * fail this check for doing its job: `theme_import`'s fixtures contain
 * `url(https://attacker.example)` precisely to assert the validator rejects it. A
 * remote URL inside a Rust string is not a stylesheet, and flagging it would mean the
 * only way to pass is to stop testing the attack.
 *
 * Real remote CSS is still caught, in CSS, and known hosts are caught everywhere by
 * `FORBIDDEN_HOSTS` regardless of language.
 */
const STYLESHEET = new Set(['.css', '.html']);
const CSS_REMOTE = [
  { name: 'remote @import', re: /@import\s+(?:url\()?['"]?https?:/i },
  { name: 'remote url()', re: /url\(\s*['"]?(?:https?:)?\/\//i },
];

function walk(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return out;
  }
  for (const name of entries) {
    if (SKIP_DIRS.has(name)) continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (SCAN_EXT.has(p.slice(p.lastIndexOf('.')))) out.push(p);
  }
  return out;
}

const findings = [];
let scanned = 0;
const sanctionedSeen = new Set();

for (const dir of SCAN_DIRS) {
  for (const file of walk(join(ROOT, ...dir.split('/')))) {
    const rel = relative(ROOT, file);
    // This script names the forbidden hosts by definition.
    if (rel === join('scripts', 'check-network.mjs')) continue;

    scanned += 1;
    const source = readFileSync(file, 'utf8');
    const code = stripComments(source).split(/\r?\n/);
    const original = source.split(/\r?\n/);
    const isSanctioned = SANCTIONED.has(rel);
    const ext = rel.slice(rel.lastIndexOf('.'));
    // A bare URL is a finding in code that ships and a help message in code that does
    // not: `check-e2e-ready` tells you where to download a WebDriver. Rules marked
    // `shipped` apply to the webview bundle only. The forbidden-host list is not one of
    // them — a CDN host is wrong in a build script too, because build scripts get copied.
    const isShipped = rel.startsWith(`src${sep}`) && !rel.startsWith(`src-tauri${sep}`);

    code.forEach((line, i) => {
      const at = `${rel}:${i + 1}`;
      const shown = (original[i] ?? '').trim().slice(0, 90);

      for (const host of FORBIDDEN_HOSTS) {
        if (line.includes(host)) {
          // No exemption, sanctioned or not. HIBP and the update endpoint are not
          // on this list; a CDN or favicon host in either of them is still a bug.
          findings.push(`${at}  remote host  ${host}   ${shown}`);
        }
      }

      if (STYLESHEET.has(ext)) {
        for (const { name, re } of CSS_REMOTE) {
          if (re.test(line)) findings.push(`${at}  ${name}   ${shown}`);
        }
      }

      if (isSanctioned) return;
      for (const { name, re, exts, shipped } of CLIENTS) {
        if (!exts.has(ext)) continue;
        if (shipped === true && !isShipped) continue;
        if (re.test(line)) findings.push(`${at}  ${name}   ${shown}`);
      }
    });

    if (isSanctioned) sanctionedSeen.add(rel);
  }
}

if (scanned === 0) {
  console.error('check:network — no files scanned. The walk is broken, not the code.');
  process.exit(1);
}

// A sanctioned file that has vanished means the list is stale, and a stale allow-list
// is how an exemption outlives the thing it was granted for.
for (const [file, purpose] of SANCTIONED) {
  if (!sanctionedSeen.has(file)) {
    findings.push(
      `${file}  missing  this file is on the sanctioned-request allow-list for ` +
        `${purpose} but was not found. Remove the exemption or restore the file.`,
    );
  }
}

console.log(
  `check:network — scanned ${scanned} files, ${SANCTIONED.size} sanctioned ` +
    `(HIBP range queries, the signed update check)`,
);

if (findings.length > 0) {
  console.error(`\n${findings.length} problem(s):\n`);
  for (const f of findings) console.error(`  ${f}`);
  console.error(
    '\nExactly two outbound requests exist in this product — an HIBP range ' +
      'query and the signed update manifest check (CLAUDE.md §4.7). Icons are ' +
      'bundled and never fetched (ADD-001). A third request is a spec change, ' +
      'not a patch.',
  );
  process.exit(1);
}

process.exit(0);
