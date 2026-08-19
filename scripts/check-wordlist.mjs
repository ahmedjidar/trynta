#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// ─────────────────────────────────────────────────────────────────────────────
// Validate the vendored EFF long wordlist (SPEC-V1 §7.3).
//
// The passphrase generator's entropy figure claims log2(7776) ≈ 12.925 bits per
// word. That claim is only true if the list really has 7,776 distinct entries,
// so this checks the property the number depends on rather than trusting the
// file. A short or duplicate-bearing list silently costs entropy while the
// reported figure stays the same, which is the failure mode §7.3 is written
// against.
//
//   node scripts/check-wordlist.mjs
//
// Exits 0 and prints "absent" when the file is not vendored yet, so it can sit
// in CI before the asset arrives without turning the build red for a known gap.
// Once the file exists it is checked strictly, every time.
//
// Expected input: the EFF large wordlist as published, one entry per line,
// `<5 dice digits>\t<word>`. Licence and provenance go in
// THIRD-PARTY-NOTICES.md, which §7.4's sibling rule requires before shipping.
// ─────────────────────────────────────────────────────────────────────────────

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const LIST = join(ROOT, 'src-tauri', 'assets', 'eff_large_wordlist.txt');

/** SPEC-V1 §7.3: 6^5 entries. */
const EXPECTED = 7776;

if (!existsSync(LIST)) {
  console.log('check:wordlist — absent, so the passphrase generator refuses to run.');
  console.log(`  Drop the EFF large wordlist at: ${LIST}`);
  console.log('  Then record its source and licence in THIRD-PARTY-NOTICES.md.');
  process.exit(0);
}

const text = readFileSync(LIST, 'utf8');
const lines = text.split(/\r?\n/).filter((l) => l.trim() !== '');

const failures = [];
if (lines.length !== EXPECTED) {
  failures.push(`expected ${EXPECTED} entries, found ${lines.length}`);
}

const words = [];
for (const [index, line] of lines.entries()) {
  const parts = line.split('\t');
  const word = (parts.length === 2 ? parts[1] : parts[0]).trim();
  if (!/^[a-z]+$/.test(word)) {
    failures.push(`line ${index + 1}: ${JSON.stringify(word)} is not a lowercase a-z word`);
    if (failures.length > 12) break;
  }
  words.push(word);
}

const distinct = new Set(words);
if (distinct.size !== words.length) {
  failures.push(`${words.length - distinct.size} duplicate words — each duplicate costs entropy`);
}

if (failures.length > 0) {
  console.error('check:wordlist — the vendored list is not usable.\n');
  for (const f of failures.slice(0, 12)) console.error(`  ${f}`);
  console.error('');
  console.error('A list that is short, duplicated or non-alphabetic makes the generator report');
  console.error('more entropy than it delivers. Fix the asset; do not relax this check.');
  process.exit(1);
}

console.log(`check:wordlist — ${words.length} distinct lowercase entries, as §7.3 requires`);
process.exit(0);
