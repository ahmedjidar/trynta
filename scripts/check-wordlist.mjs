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
// ## What actually protects the entropy figure, and what does not
//
// **Count and distinctness.** Those two, and nothing else. Entropy per word is
// log2(number of distinct entries the CSPRNG can choose between). It does not
// depend in any way on what characters those entries are spelled with.
//
// An earlier version of this file rejected the genuine EFF list because four of
// its entries are hyphenated — `drop-down`, `felt-tip`, `t-shirt`, `yo-yo` — and
// told the reader that "non-alphabetic" entries make the generator overstate its
// entropy. **That was simply false.** All 7,776 entries remain distinct with the
// hyphens present, so the figure is exactly log2(7776) either way. The check was
// rejecting a correct asset on a reason that did not exist, which is worse than
// not checking: it invites someone to edit a verified upstream file to satisfy a
// mistaken rule.
//
// The character class is still constrained, for a real but much smaller reason:
// a leading, trailing or doubled hyphen would be a sign the file had been
// mangled in transit or hand-edited, and those are shapes the published list
// does not contain. That is a corruption check, not an entropy one, and it is
// described as such below.
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
  // Lowercase a-z, with hyphens permitted between letters only. The published
  // list contains four such entries and no leading, trailing or doubled hyphen;
  // any of those would mean the file had been mangled rather than downloaded.
  if (!/^[a-z]+(?:-[a-z]+)*$/.test(word)) {
    failures.push(
      `line ${index + 1}: ${JSON.stringify(word)} is not lowercase a-z with internal hyphens`,
    );
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
  console.error('A list that is short or contains duplicates makes the generator report more');
  console.error('entropy than it delivers, because entropy per word is log2(distinct entries).');
  console.error('');
  console.error('A rejected *spelling* is a different matter and does not affect entropy at all —');
  console.error('it means the file does not look like the one EFF publishes, so check where it');
  console.error('came from rather than editing it. The published list is 7,776 lines of');
  console.error(
    '"<5 dice digits>TAB<word>", sha256 addd35536511597a02fa0a9ff1e5284677b8883b83e986e43f15a3db996b903e.',
  );
  process.exit(1);
}

const hyphenated = words.filter((w) => w.includes('-')).length;
console.log(
  `check:wordlist — ${words.length} distinct entries (${hyphenated} hyphenated), ` +
    `log2(${words.length}) = ${Math.log2(words.length).toFixed(3)} bits per word, as §7.3 requires`,
);
process.exit(0);
