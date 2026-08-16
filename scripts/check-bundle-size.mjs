#!/usr/bin/env node
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
const INSTALLER_EXT = new Set(['.dmg', '.msi', '.exe', '.app']);

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
      if (e.name === 'deps' || e.name === 'build' || e.name === 'incremental') continue;
      findInstallers(p, out);
    } else if (INSTALLER_EXT.has(extname(e.name))) {
      out.push(p);
    }
  }
  return out;
}

const installers = findInstallers(join(ROOT, 'target'));
if (installers.length === 0) {
  console.error(
    'check:bundle-size — no installer found under target/. Run `pnpm tauri build` first.',
  );
  process.exit(1);
}

let failed = false;
for (const file of installers) {
  const mb = statSync(file).size / 1024 / 1024;
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
