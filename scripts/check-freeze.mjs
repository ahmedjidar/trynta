#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// ADD-003: the freeze rule must not depend on anyone remembering it.
//
// `tests/acceptance/` and `scripts/verify-v1.mjs` are frozen once committed:
// never edited, deleted, `#[ignore]`d or weakened to make a run pass. That was a
// rule in a file; this makes it a build failure.
//
//   node scripts/check-freeze.mjs           verify every hash (CI, verify:v1)
//   node scripts/check-freeze.mjs --write   regenerate FREEZE.lock
//
// `--write` is not an escape hatch. It exists for the one legitimate case: the
// spec owner has *deliberately* changed what the acceptance suite asserts, and
// the regenerated lock is reviewed as part of that change. Re-running it to make
// a red build go green is the single worst thing anyone can do in this
// repository — it silently deletes the evidence that the product is correct.

import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, posix, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const LOCK = join(ROOT, 'FREEZE.lock');

/** Everything under these paths is frozen. Directories are walked recursively. */
const FROZEN = ['tests/acceptance', 'scripts/verify-v1.mjs'];

const WRITE = process.argv.includes('--write');

function walk(target, out = []) {
  const abs = join(ROOT, target);
  if (!existsSync(abs)) return out;
  if (statSync(abs).isDirectory()) {
    for (const name of readdirSync(abs).sort()) {
      walk(posix.join(target, name), out);
    }
  } else {
    out.push(target);
  }
  return out;
}

/**
 * Hash content with line endings normalised.
 *
 * The repo checks out LF via .gitattributes, but a Windows editor or a stray
 * tool can still write CRLF. A frozen test whose meaning is identical must not
 * fail the build over an invisible byte — and a change that matters cannot hide
 * in one either.
 */
function hashFile(relPath) {
  const bytes = readFileSync(join(ROOT, relPath));
  const normalised = bytes.toString('utf8').replace(/\r\n/g, '\n');
  return createHash('sha256').update(normalised, 'utf8').digest('hex');
}

const files = FROZEN.flatMap((t) => walk(t))
  .map((p) => p.split(sep).join('/'))
  .sort();

if (files.length === 0) {
  console.error('check:freeze — found no frozen files at all. Refusing to pass.');
  process.exit(1);
}

const current = new Map(files.map((f) => [f, hashFile(f)]));

if (WRITE) {
  const header = [
    '# FREEZE.lock — SHA-256 of every frozen file.',
    '#',
    '# Frozen means: never edited, deleted, #[ignore]d or weakened to make a run',
    '# pass. If a criterion is wrong or unimplementable as written, that is a spec',
    '# conversation, not a test edit.',
    '#',
    '# Hashes are over content with CRLF normalised to LF, so a line-ending change',
    '# alone does not fail the build and a real change cannot hide in one.',
    '#',
    '# Regenerated only when the spec owner deliberately changes what the',
    '# acceptance suite asserts, and reviewed as part of that change:',
    '#     node scripts/check-freeze.mjs --write',
    '',
  ].join('\n');
  const body = files.map((f) => `${current.get(f)}  ${f}`).join('\n');
  writeFileSync(LOCK, `${header}${body}\n`, 'utf8');
  console.log(`check:freeze — wrote ${files.length} hashes to FREEZE.lock`);
  process.exit(0);
}

if (!existsSync(LOCK)) {
  console.error('check:freeze — FREEZE.lock is missing. Generate it with:');
  console.error('    node scripts/check-freeze.mjs --write');
  process.exit(1);
}

const recorded = new Map();
for (const line of readFileSync(LOCK, 'utf8').split(/\r?\n/)) {
  const trimmed = line.trim();
  if (trimmed === '' || trimmed.startsWith('#')) continue;
  const [hash, ...rest] = trimmed.split(/\s+/);
  recorded.set(rest.join(' '), hash);
}

const modified = [];
const added = [];
const removed = [];

for (const [file, hash] of current) {
  if (!recorded.has(file)) added.push(file);
  else if (recorded.get(file) !== hash) modified.push(file);
}
for (const file of recorded.keys()) {
  if (!current.has(file)) removed.push(file);
}

console.log(`check:freeze — ${current.size} frozen files verified against FREEZE.lock`);

if (modified.length === 0 && added.length === 0 && removed.length === 0) {
  process.exit(0);
}

console.error('\nA frozen file changed.\n');
for (const f of modified) console.error(`  MODIFIED  ${f}`);
for (const f of removed) console.error(`  DELETED   ${f}`);
for (const f of added) console.error(`  ADDED     ${f}`);

console.error(
  [
    '',
    'These files are the evidence that the product does what the spec says.',
    'Weakening one to make a run pass is the worst possible failure in this project.',
    '',
    'If this change is NOT deliberate: revert it.',
    '    git checkout -- tests/acceptance scripts/verify-v1.mjs',
    '',
    'If the spec owner HAS deliberately changed what the suite asserts, regenerate',
    'the lock and let it be reviewed alongside that change:',
    '    node scripts/check-freeze.mjs --write',
    '',
  ].join('\n'),
);

process.exit(1);
