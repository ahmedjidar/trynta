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
//
// The rules themselves live in `lib/network-rules.mjs` so they can be tested against
// snippets rather than only against this repository — see `lib/network-rules.test.mjs`.
// This file keeps the walk, the allow-list and the reporting, and keeps running
// unconditionally, so there is no arrangement in which the check becomes a no-op.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';
import { stripComments } from './lib/strip-comments.mjs';
import { scanSource } from './lib/network-rules.mjs';

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

/**
 * The three files that define or test the rules, and are therefore not judged by
 * them.
 *
 * The first two name the forbidden hosts and spell out the shapes of the requests
 * they forbid; a file that defines a rule cannot also be judged by it. The third is
 * the rules' test suite, which has to *contain* a real favicon probe, a real
 * `fetch()` and a real CDN host in order to assert that each is still caught — the
 * fixtures are the point of the file.
 *
 * An explicit list rather than a directory exclusion, so adding a fourth is a
 * visible edit in this file rather than a file dropped into a folder. Each is a
 * hole in the scan and each is here on purpose; nothing else may join them without
 * the same justification.
 */
const SELF = new Set([
  join('scripts', 'check-network.mjs'),
  join('scripts', 'lib', 'network-rules.mjs'),
  join('scripts', 'lib', 'network-rules.test.mjs'),
]);

const SCAN_DIRS = ['src', 'src-tauri/src', 'crates', 'e2e', 'scripts'];
const SCAN_EXT = new Set(['.rs', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.css', '.html']);
const SKIP_DIRS = new Set(['node_modules', 'dist', 'target', 'coverage', '.tsbuild', 'generated']);

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
const selfSeen = new Set();

for (const dir of SCAN_DIRS) {
  for (const file of walk(join(ROOT, ...dir.split('/')))) {
    const rel = relative(ROOT, file);
    if (SELF.has(rel)) {
      selfSeen.add(rel);
      continue;
    }

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

    for (const { line, rule, detail } of scanSource({ code, ext, isShipped, isSanctioned })) {
      const shown = (original[line - 1] ?? '').trim().slice(0, 90);
      findings.push(`${rel}:${line}  ${rule}  ${detail}   ${shown}`);
    }

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

// The same reasoning applied to the self-exclusions. An exclusion for a file that
// no longer exists is a hole waiting for something to be renamed into it, and it is
// silent — the check keeps passing either way, which is exactly why it needs saying.
for (const file of SELF) {
  if (!selfSeen.has(file)) {
    findings.push(
      `${file}  missing  this file is excluded from the scan because it defines or ` +
        `tests the rules, but was not found. Remove the exclusion or restore the file.`,
    );
  }
}

console.log(
  `check:network — scanned ${scanned} files, ${SANCTIONED.size} sanctioned ` +
    `(HIBP range queries, the signed update check), ${SELF.size} self-excluded`,
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
