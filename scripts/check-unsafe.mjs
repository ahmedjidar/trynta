#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// CLAUDE.md §7: `unsafe` is permitted in exactly one place —
// `src-tauri/src/platform/` — and every block carries a comment justifying it.
//
// The compiler already enforces most of this. `src-tauri/src/lib.rs` is
// `#![deny(unsafe_code)]` and `pub mod platform` carries the one scoped
// `#[allow(unsafe_code)]`, so an `unsafe` block in `commands/` or `services/` is a
// build error, not a review nit. `keyring-crypto` and `keyring-store` are
// `#![forbid(unsafe_code)]`, which cannot be relaxed at all.
//
// What the compiler does NOT catch is someone adding a *second*
// `#[allow(unsafe_code)]` somewhere else. That silently widens the one exception
// the whole rule depends on, and it looks like a one-line diff. This script exists
// for that case, and for the SAFETY-comment requirement, which no lint expresses.

import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The only directory permitted to contain `unsafe`. */
const PERMITTED = join('src-tauri', 'src', 'platform');

/** The only file permitted to carry `#[allow(unsafe_code)]`. */
const ALLOW_SITE = join('src-tauri', 'src', 'lib.rs');

/** Directories never worth walking. */
const SKIP = new Set(['target', 'node_modules', 'dist', '.git', 'gen']);

function rustFiles(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (SKIP.has(entry.name)) continue;
      rustFiles(join(dir, entry.name), out);
    } else if (entry.name.endsWith('.rs')) {
      out.push(join(dir, entry.name));
    }
  }
  return out;
}

const failures = [];
let unsafeBlocks = 0;
let filesScanned = 0;

for (const path of rustFiles(ROOT)) {
  const rel = relative(ROOT, path);
  const text = readFileSync(path, 'utf8');
  filesScanned += 1;

  // ── The scoped allow may exist in exactly one file ──
  if (text.includes('#[allow(unsafe_code)]') || text.includes('#![allow(unsafe_code)]')) {
    if (rel !== ALLOW_SITE) {
      failures.push(
        `${rel}: carries #[allow(unsafe_code)]. The crate-level deny in ` +
          `${ALLOW_SITE} is what confines unsafe to ${PERMITTED}${sep}; a second ` +
          `allow widens that exception invisibly. If this module genuinely needs ` +
          `unsafe, it belongs in platform/.`,
      );
    }
    continue;
  }

  const lines = text.split(/\r?\n/);
  const inPermitted = rel.startsWith(PERMITTED + sep);

  lines.forEach((line, index) => {
    // Match `unsafe` as a keyword, not as a substring of a word or a doc comment.
    // `unsafe(method(...))` is objc2's attribute syntax, not a block.
    const isBlock = /(^|[^\w"])unsafe\s*(\{|fn |impl |extern )/.test(line);
    const isComment = /^\s*(\/\/|\*|\/\*)/.test(line);
    if (!isBlock || isComment) return;

    unsafeBlocks += 1;

    if (!inPermitted) {
      failures.push(`${rel}:${index + 1}: unsafe outside ${PERMITTED}${sep} — ` + line.trim());
      return;
    }

    // ── Every unsafe site needs a justification above it (CLAUDE.md §7) ──
    // Look back for a SAFETY note. The window is 14 lines because the real
    // justifications in platform/ run long — the LAContext one names three separate
    // preconditions — and the word SAFETY sits on the *first* line of the block, so
    // a short window fails the most thoroughly documented sites and passes the
    // terse ones. That is exactly backwards.
    const WINDOW = 14;
    const window = lines.slice(Math.max(0, index - WINDOW), index).join('\n');
    if (!/SAFETY|# Safety/i.test(window)) {
      failures.push(
        `${rel}:${index + 1}: unsafe with no SAFETY comment in the ${WINDOW} lines ` +
          `above it. CLAUDE.md §7 requires a comment naming the invariant it relies ` +
          `on — ` +
          line.trim(),
      );
    }
  });
}

if (filesScanned === 0) {
  console.error('check:unsafe — no Rust files found. The walk is broken, not the code.');
  process.exit(1);
}

if (failures.length > 0) {
  console.error('check:unsafe — CLAUDE.md §7 violations:\n');
  for (const failure of failures) console.error(`  ${failure}`);
  console.error(
    `\n${failures.length} problem(s). unsafe is permitted only in ${PERMITTED}${sep}, ` +
      `only with a SAFETY comment, and the scoped allow lives only in ${ALLOW_SITE}.`,
  );
  process.exit(1);
}

console.log(
  `check:unsafe — ${unsafeBlocks} unsafe site(s), all in ${PERMITTED}${sep} ` +
    `with a SAFETY comment; ${filesScanned} Rust files scanned`,
);
