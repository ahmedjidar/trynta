#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// SPEC-V1 §9: installer 20 MB target, 25 MB hard failure, per platform.
//
// Warn at the target, fail at the ceiling. The budget is easy to hold now and
// expensive to reclaim later.

import { readdirSync, statSync } from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const TARGET_MB = Number(process.argv[2] ?? 20);
const CEILING_MB = Number(process.argv[3] ?? 25);
const INSTALLER_EXT = new Set(['.dmg', '.msi', '.exe']);

/**
 * Only what a user would actually download.
 *
 * The scan used to walk the whole of `target/`, which meant `target/debug/keyring.exe`
 * counted: a 38 MB unoptimised binary with debug info that is never shipped and never
 * could be. It failed the gate on any machine that had run `cargo test`, and passed in
 * CI only because CI has no debug build — a check that depends on which build you ran
 * last is not a check. Tauri writes every real artifact under a `bundle/` directory, so
 * that is the boundary.
 */
function findBundleDirs(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const e of entries) {
    if (!e.isDirectory()) continue;
    if (e.name === 'deps' || e.name === 'build' || e.name === 'incremental') continue;
    const p = join(dir, e.name);
    if (e.name === 'bundle') out.push(p);
    else findBundleDirs(p, out);
  }
  return out;
}

/** Total bytes of a directory tree — a macOS `.app` is a directory, not a file. */
function treeSize(dir) {
  let total = 0;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    total += e.isDirectory() ? treeSize(p) : statSync(p).size;
  }
  return total;
}

function findInstallers(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const e of entries) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      // `.app` is a bundle directory and is what ships on macOS, so it is measured
      // whole rather than descended into.
      if (extname(e.name) === '.app') out.push(p);
      else findInstallers(p, out);
    } else if (INSTALLER_EXT.has(extname(e.name))) {
      out.push(p);
    }
  }
  return out;
}

const installers = findBundleDirs(join(ROOT, 'target')).flatMap((d) => findInstallers(d));
if (installers.length === 0) {
  console.error(
    'check:bundle-size — no installer found under target/**/bundle/. Run `pnpm tauri build` first.',
  );
  process.exit(1);
}

let failed = false;
for (const file of installers) {
  const bytes = statSync(file).isDirectory() ? treeSize(file) : statSync(file).size;
  const mb = bytes / 1024 / 1024;
  const rel = file.slice(ROOT.length + 1);
  if (mb > CEILING_MB) {
    console.error(`FAIL  ${mb.toFixed(1)} MB  ${rel}  (ceiling ${CEILING_MB} MB)`);
    failed = true;
  } else if (mb > TARGET_MB) {
    console.warn(`WARN  ${mb.toFixed(1)} MB  ${rel}  (target ${TARGET_MB} MB)`);
  } else {
    console.log(`ok    ${mb.toFixed(1)} MB  ${rel}`);
  }
}

process.exit(failed ? 1 : 0);
