#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// ADD-002 Q1 / ADD-003 §②: the crypto path resolves exactly one generation.
//
// `deny.toml` enforces single-version across the workspace, but it has to
// tolerate three `getrandom` versions because `tauri` pins 0.3 and `uuid` pins
// 0.4. That tolerance is workspace-wide, so on its own it would let a second
// `getrandom` — or a second `rand_core`, or an `argon2` release candidate —
// reach the *crypto* tree unnoticed, which is the only place it would matter.
//
// This walks `cargo metadata`'s resolve graph from `keyring-crypto`, following
// **normal** dependency edges only. Dev-dependencies are excluded deliberately:
// `proptest` legitimately pulls a second `rand_core` generation into the test
// binary, and that says nothing about what ships.
//
// Implemented here rather than as a `cargo test` because `Cargo.lock` alone
// cannot answer the question — lockfile v4 does not version-qualify dependency
// edges, so with two `rand_core` entries present a lockfile walk cannot tell
// which one the crypto path uses. `cargo metadata` resolves that exactly, and
// running it outside `cargo test` avoids any package-cache lock contention.

import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Two copies of any of these means two incompatible trait universes. */
const MUST_BE_SINGLE = [
  'getrandom',
  'digest',
  'sha2',
  'rand_core',
  'aead',
  'curve25519-dalek',
  'crypto-common',
  'block-buffer',
  'generic-array',
  'subtle',
  'zeroize',
  'universal-hash',
  'cipher',
];

/** Reaching any of these would break the crate's compiler-enforced isolation. */
const MUST_NOT_REACH = ['tauri', 'rusqlite', 'libsqlite3-sys', 'serde_json', 'keyring-store'];

// Semver: build metadata follows `+` and a pre-release tag follows `-` before
// it. Strip the build metadata first, or `wasi 0.11.1+wasi-snapshot-preview1`
// reads as a pre-release because "preview1" contains "pre".
const isPrerelease = (version) => version.split('+')[0].includes('-');

const meta = spawnSync('cargo metadata --format-version 1 --locked', {
  cwd: ROOT,
  shell: true,
  encoding: 'utf8',
  maxBuffer: 256 * 1024 * 1024,
});

if (meta.status !== 0) {
  console.error('cargo metadata failed:');
  console.error((meta.stderr ?? '').split(/\r?\n/).slice(-12).join('\n'));
  process.exit(1);
}

const { resolve: graph } = JSON.parse(meta.stdout);
const nodes = new Map(graph.nodes.map((n) => [n.id, n]));

const root = graph.nodes.find((n) => n.id.includes('keyring-crypto'));
if (!root) {
  console.error('keyring-crypto is not in the resolve graph — this check is not proving anything.');
  process.exit(1);
}

// Walk normal edges only. `deps[].dep_kinds[].kind` is null for a normal
// dependency, "dev" or "build" otherwise.
const seen = new Set();
const stack = [root.id];
while (stack.length > 0) {
  const id = stack.pop();
  if (seen.has(id)) continue;
  seen.add(id);

  const node = nodes.get(id);
  if (!node) continue;
  for (const dep of node.deps ?? []) {
    const isNormal = (dep.dep_kinds ?? []).some((k) => k.kind === null || k.kind === undefined);
    if (isNormal && !seen.has(dep.pkg)) stack.push(dep.pkg);
  }
}

// `registry+https://…#name@version`, or `path+file:///…#name@version`.
const parse = (id) => {
  const frag = id.slice(id.indexOf('#') + 1);
  const at = frag.lastIndexOf('@');
  return at === -1
    ? { name: frag, version: '' }
    : { name: frag.slice(0, at), version: frag.slice(at + 1) };
};

const path = [...seen].map(parse);
if (!path.some((p) => p.name === 'curve25519-dalek')) {
  console.error('the walk found no curve25519-dalek — the graph traversal is broken.');
  process.exit(1);
}

const versions = new Map();
for (const { name, version } of path) {
  if (!versions.has(name)) versions.set(name, new Set());
  versions.get(name).add(version);
}

const failures = [];

for (const name of MUST_BE_SINGLE) {
  const found = versions.get(name);
  if (found && found.size > 1) {
    failures.push(
      `${name} resolves to ${found.size} versions in the crypto path: ${[...found].sort().join(', ')}`,
    );
  }
}

for (const { name, version } of path) {
  if (isPrerelease(version)) {
    failures.push(`${name} ${version} is a pre-release, in the crypto path`);
  }
}

for (const name of MUST_NOT_REACH) {
  if (versions.has(name)) {
    failures.push(`keyring-crypto reaches ${name}, breaking its compiler-enforced isolation`);
  }
}

const single = MUST_BE_SINGLE.filter((n) => versions.has(n));
console.log(
  `check:crypto-generation — ${path.length} crates in the crypto path, ` +
    `${single.length} core crates checked, all single-version`,
);

if (failures.length > 0) {
  console.error('');
  for (const f of failures) console.error(`  ${f}`);
  console.error('\nSee ADD-002 Q1: one stable RustCrypto generation, no release candidates.');
  process.exit(1);
}

process.exit(0);
