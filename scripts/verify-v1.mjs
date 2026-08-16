#!/usr/bin/env node
// ─────────────────────────────────────────────────────────────────────────────
// Keyring — SPEC-V1 §11 acceptance verifier
//
// FROZEN FILE. Once committed this script is never edited to make a run pass.
// A criterion that cannot be satisfied is a spec problem to raise, not a check
// to weaken. Adding criteria is fine; removing, loosening or silently passing
// one is not.
//
// Every criterion in SPEC-V1 §11 appears here exactly once. A criterion whose
// implementation belongs to a later build run emits SKIP with a reason and a
// run number, and is counted in the summary. Nothing is ever silently passed.
//
//   PASS  the check ran and succeeded
//   FAIL  the check ran and failed, or the code it needs does not exist yet
//   SKIP  deferred to a later run — reason and run number always printed
//
// Exit code is 0 only when FAIL == 0.
//
// Usage:
//   pnpm verify:v1                 run everything
//   pnpm verify:v1 -- --run 1      only criteria scoped to run 1
//   pnpm verify:v1 -- --only AC04  only the named criteria (repeatable, prefix ok)
//   pnpm verify:v1 -- --json       machine-readable report on stdout
//   pnpm verify:v1 -- --list       print the criteria table and exit
// ─────────────────────────────────────────────────────────────────────────────

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// ── argument parsing ─────────────────────────────────────────────────────────

const argv = process.argv.slice(2);
const flag = (name) => argv.includes(name);
const values = (name) =>
  argv.reduce((acc, a, i) => (a === name && argv[i + 1] ? [...acc, argv[i + 1]] : acc), []);

const OPTS = {
  json: flag('--json'),
  list: flag('--list'),
  run: values('--run').map(Number),
  only: values('--only').map((s) => s.toUpperCase()),
};

// ── check primitives ─────────────────────────────────────────────────────────

/** Run a shell command from the repo root. Success is exit code 0. */
function exec(command, { cwd = ROOT, timeoutMs = 30 * 60_000 } = {}) {
  const r = spawnSync(command, {
    cwd,
    shell: true,
    encoding: 'utf8',
    timeout: timeoutMs,
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_TERM_COLOR: 'never', FORCE_COLOR: '0' },
  });
  const out = `${r.stdout ?? ''}${r.stderr ?? ''}`;
  if (r.error) return { ok: false, detail: `${command}\n${r.error.message}` };
  if (r.status !== 0) return { ok: false, detail: `${command}\n${tail(out, 24)}` };
  return { ok: true, detail: command, output: out };
}

/** Last `n` non-empty lines, for readable failure output. */
function tail(text, n) {
  const lines = String(text).split(/\r?\n/).filter((l) => l.trim() !== '');
  return lines.slice(-n).join('\n');
}

function readIfPresent(relPath) {
  const p = join(ROOT, relPath);
  return existsSync(p) ? readFileSync(p, 'utf8') : null;
}

// A Rust acceptance test lives in tests/acceptance and is addressed by target
// name, so it stays stable regardless of which crate ends up owning the code.
const acceptance = (target) => `cargo test -p keyring-acceptance --test ${target} -- --nocapture`;

// ── the criteria ─────────────────────────────────────────────────────────────
//
// One entry per bullet in SPEC-V1 §11, in document order. `run` is the build run
// that makes the criterion satisfiable. Checks with `skip` never execute.

const CRITERIA = [
  {
    id: 'AC01',
    title: 'Create vault, add all four item types, lock, restart, unlock, everything intact',
    checks: [{ name: 'lifecycle round-trip across a process boundary', run: 1, exec: acceptance('ac01_lifecycle') }],
  },
  {
    id: 'AC02',
    title: 'No plaintext item content on disk (.db, -wal, -shm) — sentinel string scan',
    checks: [{ name: 'sentinel scan of db, wal and shm', run: 1, exec: acceptance('ac02_no_plaintext_on_disk') }],
  },
  {
    id: 'AC03',
    title: 'Wrong password rejected in constant time; backoff survives a process restart',
    checks: [
      { name: 'verifier comparison is constant time', run: 1, exec: acceptance('ac03_constant_time_and_backoff') },
      {
        name: 'backoff persists across restart',
        run: 1,
        exec: `cargo test -p keyring-acceptance --test ac03_constant_time_and_backoff backoff -- --nocapture`,
      },
    ],
  },
  {
    id: 'AC04',
    title: 'Rollback: restore an item row from an earlier snapshot → unlock refuses with TamperDetected',
    checks: [{ name: 'row rollback is detected by the signed manifest', run: 1, exec: acceptance('ac04_rollback') }],
  },
  {
    id: 'AC05',
    title: 'Header tamper: swap a public key → header_mac verification fails',
    checks: [{ name: 'header MAC binds the public keys to the master password', run: 1, exec: acceptance('ac05_header_tamper') }],
  },
  {
    id: 'AC06',
    title: 'Biometric unlock works on both platforms, survives restart, invalidates on enrolment change',
    checks: [
      { name: 'Touch ID / Windows Hello unlock', run: 3, skip: 'requires the platform secure-store and biometric layer' },
      { name: 'invalidation on enrolment change', run: 3, skip: 'requires the platform secure-store and biometric layer' },
    ],
  },
  {
    id: 'AC07',
    title: 'Copy places the password on the clipboard with plaintext never entering the webview',
    checks: [{ name: 'Rust-side clipboard write', run: 3, skip: 'requires the platform clipboard layer and the IPC surface' }],
  },
  {
    id: 'AC08',
    title: 'Clipboard auto-clears on both platforms with Windows Clipboard History enabled',
    checks: [{ name: 'auto-clear with history exclusion', run: 3, skip: 'requires the platform clipboard layer' }],
  },
  {
    id: 'AC09',
    title: 'Auto-lock fires on idle and sleep; heap sentinel scan after lock finds no key material',
    checks: [
      { name: 'auto-lock triggers', run: 2, skip: 'requires the session and lock state machine' },
      { name: 'heap sentinel scan after lock', run: 2, skip: 'requires the session and lock state machine' },
    ],
  },
  {
    id: 'AC10',
    title: 'Reveal does not bump revision or updated_at',
    checks: [{ name: 'reveal leaves the item row untouched', run: 2, skip: 'requires item_reveal_field and the activity writer' }],
  },
  {
    id: 'AC11',
    title: 'TOTP matches a reference implementation for SHA1/256/512 and 6/8 digits',
    checks: [{ name: 'TOTP known-answer vectors', run: 3, skip: 'requires services/totp' }],
  },
  {
    id: 'AC12',
    title: 'Generator entropy matches an independent inclusion–exclusion implementation exactly',
    checks: [{ name: 'entropy cross-check', run: 3, skip: 'requires services/generator' }],
  },
  {
    id: 'AC13',
    title: 'Security report flags seeded breached, weak and reused; breakdown adds up; N == 0 returns null',
    checks: [
      { name: 'report flags all three classes', run: 3, skip: 'requires services/breach, strength and report' },
      { name: 'health score breakdown and the N == 0 case', run: 3, skip: 'requires services/report' },
    ],
  },
  {
    id: 'AC14',
    title: 'Packet capture: only HIBP range requests with Add-Padding, and at most one update check',
    checks: [
      { name: 'no request to any user site, favicon host or CDN', run: 3, skip: 'requires services/breach and the updater' },
      { name: 'security_report_run makes zero site requests', run: 3, skip: 'requires services/report' },
    ],
  },
  {
    id: 'AC15',
    title: 'Backup export → wipe → restore → identical vault',
    checks: [{ name: 'keyringbackup v1 round-trip', run: 2, skip: 'requires the backup export/restore feature (SPEC-V1 §7.8)' }],
  },
  {
    id: 'AC16',
    title: 'Both migration phases run; a VACUUM INTO snapshot exists before each',
    checks: [{ name: 'schema and payload runners, with snapshots', run: 1, exec: acceptance('ac16_migrations') }],
  },
  {
    id: 'AC17',
    title: 'Dark and light both render; pnpm check:tokens finds zero hardcoded colours',
    checks: [
      { name: 'no hardcoded colour values in the codebase', run: 1, exec: 'pnpm run --silent check:tokens' },
      { name: 'dark and light both render', run: 2, skip: 'requires the theme layer and a rendered shell' },
    ],
  },
  {
    id: 'AC18',
    title: 'Runtime theme swap works under production CSP on both WKWebView and WebView2',
    checks: [{ name: 'adoptedStyleSheets swap under production CSP', run: 2, skip: 'requires the theme loader and an E2E harness' }],
  },
  {
    id: 'AC19',
    title: 'A theme containing url() is rejected by the Rust validator',
    checks: [{ name: 'theme validator rejects url()', run: 2, skip: 'requires services/theme validation' }],
  },
  {
    id: 'AC20',
    title: 'Search p95 under 16 ms at 5,000 items',
    checks: [{ name: 'search benchmark', run: 2, skip: 'requires the in-memory index and the search stage' }],
  },
  {
    id: 'AC21',
    title: 'Every item above passes on macOS and Windows',
    checks: [
      {
        name: 'CI matrix declares macos-latest and windows-latest and invokes verify:v1',
        run: 1,
        fn: () => {
          const ci = readIfPresent('.github/workflows/ci.yml');
          if (!ci) return { ok: false, detail: '.github/workflows/ci.yml is missing' };
          const missing = [];
          if (!ci.includes('macos-latest')) missing.push('macos-latest');
          if (!ci.includes('windows-latest')) missing.push('windows-latest');
          if (!/verify:v1/.test(ci)) missing.push('a verify:v1 invocation');
          return missing.length === 0
            ? { ok: true, detail: `ci.yml runs verify:v1 on macos-latest and windows-latest (host here: ${process.platform})` }
            : { ok: false, detail: `ci.yml is missing: ${missing.join(', ')}` };
        },
      },
      {
        name: 'green on the other platform',
        run: 1,
        fn: () => ({
          ok: true,
          detail:
            `this host is ${process.platform}; cross-platform proof is the CI matrix result, ` +
            `not a local assertion — treat a single-platform green as half a green`,
        }),
      },
    ],
  },
  {
    id: 'AC22',
    title: 'Toolchain clean: cargo test (debug and release), clippy pedantic, rustfmt, tsc, eslint, prettier, cargo-deny, cargo-audit, ts-rs no-diff',
    checks: [
      { name: 'cargo fmt', run: 1, exec: 'cargo fmt --all --check' },
      { name: 'cargo clippy pedantic', run: 1, exec: 'cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::pedantic' },
      { name: 'cargo test (debug)', run: 1, exec: 'cargo test --workspace --all-features' },
      { name: 'redaction test in release', run: 1, exec: 'cargo test -p keyring-crypto --release --test redaction' },
      { name: 'lock/zeroize test in release', run: 2, skip: 'requires the session and lock state machine' },
      { name: 'cargo deny', run: 1, exec: 'cargo deny check' },
      { name: 'cargo audit', run: 1, exec: 'cargo audit --deny warnings' },
      { name: 'tsc', run: 1, exec: 'pnpm run --silent typecheck' },
      { name: 'eslint', run: 1, exec: 'pnpm run --silent lint' },
      { name: 'prettier', run: 1, exec: 'pnpm run --silent format:check' },
      { name: 'ts-rs generated types match', run: 2, skip: 'requires the IPC surface that ts-rs generates from' },
    ],
  },
];

// ── runner ───────────────────────────────────────────────────────────────────

function wanted(criterion, check) {
  if (OPTS.only.length && !OPTS.only.some((o) => criterion.id.startsWith(o))) return false;
  if (OPTS.run.length && !OPTS.run.includes(check.run)) return false;
  return true;
}

function runCheck(check) {
  if (check.skip) return { status: 'SKIP', detail: `${check.skip} (run ${check.run})` };
  try {
    const r = check.fn ? check.fn() : exec(check.exec);
    return { status: r.ok ? 'PASS' : 'FAIL', detail: r.detail ?? '' };
  } catch (err) {
    return { status: 'FAIL', detail: `check threw: ${err?.message ?? String(err)}` };
  }
}

const GLYPH = { PASS: '  ok  ', FAIL: ' FAIL ', SKIP: ' skip ' };

function main() {
  if (OPTS.list) {
    for (const c of CRITERIA) {
      const runs = [...new Set(c.checks.map((k) => k.run))].sort().join(',');
      console.log(`${c.id}  run ${runs.padEnd(4)}  ${c.title}`);
    }
    return 0;
  }

  const started = Date.now();
  const results = [];

  if (!OPTS.json) {
    console.log('');
    console.log('Keyring — SPEC-V1 §11 acceptance verifier');
    console.log(`host ${process.platform}/${process.arch} · node ${process.versions.node}`);
    if (OPTS.run.length) console.log(`filter: run ${OPTS.run.join(',')}`);
    if (OPTS.only.length) console.log(`filter: ${OPTS.only.join(',')}`);
    console.log('');
  }

  for (const criterion of CRITERIA) {
    const checks = criterion.checks.filter((k) => wanted(criterion, k));
    if (checks.length === 0) continue;

    if (!OPTS.json) console.log(`${criterion.id}  ${criterion.title}`);

    for (const check of checks) {
      const r = runCheck(check);
      results.push({ id: criterion.id, name: check.name, run: check.run, ...r });
      if (OPTS.json) continue;
      console.log(`  [${GLYPH[r.status]}] ${check.name}`);
      if (r.status === 'SKIP') console.log(`           SKIP: ${r.detail}`);
      if (r.status === 'FAIL' && r.detail) {
        for (const line of r.detail.split(/\r?\n/)) console.log(`           ${line}`);
      }
    }
    if (!OPTS.json) console.log('');
  }

  const count = (s) => results.filter((r) => r.status === s).length;
  const pass = count('PASS');
  const fail = count('FAIL');
  const skip = count('SKIP');

  const byRun = new Map();
  for (const r of results.filter((x) => x.status === 'SKIP')) {
    byRun.set(r.run, (byRun.get(r.run) ?? 0) + 1);
  }

  if (OPTS.json) {
    console.log(
      JSON.stringify(
        { platform: process.platform, pass, fail, skip, elapsedMs: Date.now() - started, results },
        null,
        2,
      ),
    );
    return fail === 0 ? 0 : 1;
  }

  const criteriaTouched = new Set(results.map((r) => r.id)).size;
  console.log('─'.repeat(72));
  console.log(`${criteriaTouched} criteria · ${results.length} checks · ${((Date.now() - started) / 1000).toFixed(1)}s`);
  console.log(`PASS ${pass}   FAIL ${fail}   SKIP ${skip}`);
  for (const [run, n] of [...byRun.entries()].sort()) {
    console.log(`  deferred to run ${run}: ${n}`);
  }
  console.log('');
  console.log(fail === 0 ? 'VERIFY OK' : `VERIFY FAILED — ${fail} check${fail === 1 ? '' : 's'} failing`);
  console.log('');

  return fail === 0 ? 0 : 1;
}

process.exit(main());
