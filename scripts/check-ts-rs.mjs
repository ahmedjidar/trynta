#!/usr/bin/env node
// ─────────────────────────────────────────────────────────────────────────────
// CLAUDE.md §5: Rust types are the source of truth across IPC. TS types are
// generated with `ts-rs` and committed, and CI fails on any diff.
//
// This is that check. It regenerates the bindings and fails if the working tree
// changed, which catches the two ways the two sides drift:
//
//   1. a Rust DTO changed and nobody regenerated
//   2. someone hand-edited a file under src/ipc/generated/
//
//   node scripts/check-ts-rs.mjs
//
// It lives here rather than in scripts/verify-v1.mjs because that file is frozen
// and records this criterion as deferred. The criterion is still enforced — by
// CI, on both platforms — it is just enforced from outside the frozen verifier.
// ─────────────────────────────────────────────────────────────────────────────

import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const GENERATED = 'src/ipc/generated';

function run(command) {
  return spawnSync(command, {
    cwd: ROOT,
    shell: true,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
    env: { ...process.env, CARGO_TERM_COLOR: 'never' },
  });
}

// The export happens in ts-rs's generated `#[test]` functions, so regenerating
// means running the lib tests. `--lib` only: the integration tests raise consent
// prompts and touch the clipboard, and neither belongs in a codegen check.
const generated = run('cargo test -p keyring --lib');
if (generated.status !== 0) {
  console.error('check:ts-rs — could not regenerate the bindings.\n');
  console.error(`${generated.stdout ?? ''}${generated.stderr ?? ''}`);
  process.exit(1);
}

if (!existsSync(join(ROOT, GENERATED))) {
  console.error(`check:ts-rs — ${GENERATED} does not exist after regeneration.`);
  console.error('Every IPC DTO needs #[ts(export, export_to = "../../src/ipc/generated/")].');
  process.exit(1);
}

const status = run(`git status --porcelain -- ${GENERATED}`);
if (status.status !== 0) {
  console.error('check:ts-rs — git status failed; is this a repository?');
  process.exit(1);
}

const dirty = (status.stdout ?? '').trim();
if (dirty === '') {
  console.log(`check:ts-rs — ${GENERATED} matches the Rust types`);
  process.exit(0);
}

console.error('\nThe committed TypeScript does not match the Rust types.\n');
for (const line of dirty.split(/\r?\n/)) console.error(`  ${line}`);

const diff = run(`git diff -- ${GENERATED}`);
if ((diff.stdout ?? '').trim() !== '') {
  console.error('');
  console.error(diff.stdout);
}

console.error(
  [
    'Rust is the source of truth (CLAUDE.md §5). Never hand-edit a generated file:',
    'change the Rust DTO, run the command below, and commit the result.',
    '',
    '    cargo test -p keyring --lib',
    '',
  ].join('\n'),
);

process.exit(1);
