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
// PLATFORM SCOPE (ADD-005). Windows is the verified platform. macOS code is
// written and has never been compiled. A green run of this verifier says nothing
// about macOS, so every output path — human and JSON — states that. Removing the
// banner is the one edit to this file that would make it lie.
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

// ── Platform scope (ADD-005) ────────────────────────────────────────────────
//
// Windows is the verified platform. macOS is written, never compiled, unknown.
// This is a budget decision — private repo, free Actions minutes exhausted, macOS
// runners bill at 10× — and it reverts once there is real Apple hardware.
//
// The banner prints on every run, twice, and in the JSON. It is not decoration: a
// reader who sees `PASS 30  FAIL 0  VERIFY OK` and nothing else will conclude the
// product is verified, and on macOS that conclusion is wrong by the width of an
// entire platform.
const VERIFIED_PLATFORM = 'win32';
const UNVERIFIED_PLATFORMS = ['darwin'];

/** The scope statement, as lines. Printed on every non-JSON run. */
function platformBanner() {
  const onVerified = process.platform === VERIFIED_PLATFORM;
  if (onVerified) {
    return [
      'PLATFORM SCOPE (ADD-005): this run covers Windows only.',
      '  macOS is written, NEVER COMPILED, and unverified. Nothing here tests it.',
      '  Checklist for real hardware: MACOS-UNVERIFIED.md',
    ];
  }
  const label = UNVERIFIED_PLATFORMS.includes(process.platform)
    ? `${process.platform} is an UNVERIFIED PLATFORM (ADD-005).`
    : `${process.platform} is not a supported platform (SPEC-V1 §8 ships macOS and Windows).`;
  return [
    `PLATFORM SCOPE (ADD-005): ${label}`,
    '  Windows is the verified platform; results here are a first look, not a gate.',
    '  Work through MACOS-UNVERIFIED.md against this log rather than trusting a pass.',
  ];
}

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** CRLF or LF, for splitting captured output on either platform. */
const NEWLINE = /\r?\n/;

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

/** Split captured output into lines, whatever the platform's line ending. */
function splitLines(text) {
  return String(text).split(NEWLINE);
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

/**
 * `tauri-build` reads `frontendDist`, so every cargo command that touches
 * src-tauri fails on a fresh clone until the frontend has been built once. That
 * failure is opaque, so build it here rather than let nine checks fail for one
 * missing directory.
 */
function ensureFrontendBuilt() {
  if (existsSync(join(ROOT, 'dist', 'index.html'))) return true;

  console.log('dist/ is missing — building the frontend first (tauri-build reads frontendDist)');
  const built = exec('pnpm build', { timeoutMs: 10 * 60_000 });
  if (built.ok && existsSync(join(ROOT, 'dist', 'index.html'))) {
    console.log('dist/ built.\n');
    return true;
  }

  console.error('\nCould not build the frontend, and no cargo check touching src-tauri can run.');
  console.error('Run these, then try again:\n');
  console.error('    pnpm install --frozen-lockfile');
  console.error('    pnpm build\n');
  if (built.detail) console.error(tail(built.detail, 12));
  return false;
}

// A Rust acceptance test lives in tests/acceptance and is addressed by target
// name, so it stays stable regardless of which crate ends up owning the code.
const acceptance = (target) => `cargo test -p keyring-acceptance --test ${target} -- --nocapture`;

/**
 * Run a `cargo test` command and require that it actually ran a test.
 *
 * `cargo test <target> <filter>` exits 0 when the filter matches nothing, and
 * `cargo test --test <name>` on a target that has been renamed or deleted is the
 * same shape of nothing. Either would read as a pass here.
 *
 * That is the one way this gate can be defeated without editing this file, so it
 * is checked rather than trusted: a missing test is a FAIL, not a quiet success.
 */
function cargoTest(command) {
  const r = exec(command);
  if (!r.ok) return r;

  const output = r.output ?? '';
  const ran = [...output.matchAll(/running (\d+) tests?/g)].reduce(
    (total, match) => total + Number(match[1]),
    0,
  );
  if (ran === 0) {
    return {
      ok: false,
      detail:
        `${command}\n` +
        'the command succeeded but ran zero tests — the target or the test name does ' +
        'not exist, so this row proves nothing. A missing test is a failure.',
    };
  }
  return r;
}

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
      { name: 'auto-lock triggers', run: 2, cargoTest: 'cargo test -p keyring --test lock_state' },
      { name: 'heap sentinel scan after lock', run: 2, cargoTest: 'cargo test -p keyring --test lock_zeroize' },
    ],
  },
  {
    id: 'AC10',
    title: 'Reveal does not bump revision or updated_at',
    checks: [
      {
        name: 'reveal leaves the item row untouched',
        run: 2,
        cargoTest:
          'cargo test -p keyring-store --test activity_and_vaults reveal_does_not_mutate_item_row',
      },
    ],
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
    checks: [
      { name: 'keyringbackup v1 round-trip', run: 2, cargoTest: 'cargo test -p keyring --test backup_roundtrip' },
    ],
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
    checks: [
      { name: 'theme validator rejects url()', run: 2, cargoTest: 'cargo test -p keyring --test theme_validator' },
    ],
  },
  {
    id: 'AC20',
    title: 'Search p95 under 16 ms at 5,000 items',
    checks: [
      {
        name: 'search benchmark',
        run: 2,
        // AC20 is a number, not a yes/no. The measured p95 is echoed on success
        // as well as failure, so a run that squeaks in at 15.9 ms is visibly
        // different from one that comes in at 2 ms. Release mode because a debug
        // Rust timing would be meaningless.
        fn: () => {
          const r = cargoTest('cargo test -p keyring --release --test search_p95 -- --nocapture');
          for (const line of splitLines(r.output ?? '')) {
            if (/search p95/i.test(line)) console.log(`           ${line.trim()}`);
          }
          return r;
        },
      },
    ],
  },
  {
    id: 'AC21',
    // Retitled by ADD-005, which overrides SPEC-V1 §8's parity requirement. The
    // original title was "Every item above passes on macOS and Windows" and its two
    // checks were a grep for both runner labels and a statement that always
    // returned ok. That is no longer the policy, and — more to the point — it was
    // never a test: the second check could not fail.
    //
    // These four are the ADD-005 verification clause, executable. They are
    // STRICTER than what they replace, not looser: each one can fail, and three of
    // them fail the moment someone quietly reinstates a claim of parity.
    title: 'Platform scope is stated honestly (ADD-005 supersedes §8 parity)',
    checks: [
      {
        name: 'CI runs the gate on Windows for every push',
        run: 1,
        fn: () => {
          const ci = readIfPresent('.github/workflows/ci.yml');
          if (!ci) return { ok: false, detail: '.github/workflows/ci.yml is missing' };
          if (!/runs-on:\s*windows-latest/.test(ci)) {
            return { ok: false, detail: 'no windows-latest job in ci.yml' };
          }
          if (!/verify:v1/.test(ci)) {
            return { ok: false, detail: 'ci.yml never invokes verify:v1' };
          }
          // The Windows gate must not itself be gated. If the only verify job sits
          // behind a tag or a dispatch input, nothing runs on an ordinary push and
          // the project has no gate at all.
          const verifyJob = ci.slice(ci.indexOf('  verify:'), ci.indexOf('  supply-chain:'));
          if (/refs\/tags|inputs\./.test(verifyJob)) {
            return {
              ok: false,
              detail: 'the Windows verify job is conditional — it must run on every push',
            };
          }
          return { ok: true, detail: 'verify-v1 runs on windows-latest, unconditionally, on push' };
        },
      },
      {
        name: 'macOS jobs are gated to tags or manual dispatch and labelled unverified',
        run: 1,
        fn: () => {
          const ci = readIfPresent('.github/workflows/ci.yml');
          if (!ci) return { ok: false, detail: '.github/workflows/ci.yml is missing' };
          if (!/macos-latest/.test(ci)) {
            return {
              ok: false,
              detail:
                'no macOS job at all. ADD-005 defers macOS, it does not delete it — ' +
                'a tag must still get the first compile.',
            };
          }
          if (!/startsWith\(github\.ref, 'refs\/tags\/v'\)/.test(ci)) {
            return {
              ok: false,
              detail:
                'macOS jobs are not gated to tags. Unconditional macOS runs bill at ' +
                '10x, which is the whole reason ADD-005 exists.',
            };
          }
          if (!/UNVERIFIED/.test(ci)) {
            return {
              ok: false,
              detail:
                'no macOS job name says UNVERIFIED. A green tick beside "macos-latest" ' +
                'reads as parity to everyone who did not read the addendum.',
            };
          }
          return {
            ok: true,
            detail: 'macOS runs on refs/tags/v* or workflow_dispatch, labelled UNVERIFIED',
          };
        },
      },
      {
        name: 'the macOS verification checklist exists and is tracked in git',
        run: 1,
        fn: () => {
          const doc = readIfPresent('MACOS-UNVERIFIED.md');
          if (!doc) return { ok: false, detail: 'MACOS-UNVERIFIED.md is missing' };
          const tracked = exec('git ls-files --error-unmatch MACOS-UNVERIFIED.md');
          if (!tracked.ok) {
            return {
              ok: false,
              detail:
                'MACOS-UNVERIFIED.md exists but is not tracked by git. A checklist that ' +
                'lives only on one machine is not a checklist.',
            };
          }
          // A file that says "macOS is unverified" and lists nothing is worse than no
          // file: it looks like the work was done.
          // Tolerant of prettier's table padding: it aligns cells, so the row id
          // is `| A1  |` rather than `| A1 |`. The first version of this check
          // matched the unpadded form and reported zero rows after a format pass.
          const rows = (doc.match(/^\|\s*[A-F]\d+\s*\|/gm) ?? []).length;
          if (rows < 10) {
            return {
              ok: false,
              detail: `only ${rows} checklist rows — this should enumerate every macOS path`,
            };
          }
          return { ok: true, detail: `MACOS-UNVERIFIED.md tracked, ${rows} executable checklist rows` };
        },
      },
      {
        name: 'no document still claims macOS parity',
        run: 1,
        fn: () => {
          const offenders = [];
          for (const file of ['CLAUDE.md', 'SECURITY.md', 'README.md']) {
            const text = readIfPresent(file);
            if (!text) continue;
            if (/Windows is not a port/i.test(text)) {
              offenders.push(`${file} still says "Windows is not a port"`);
            }
            if (/ships on both or ships on neither/i.test(text)) {
              offenders.push(`${file} still claims every feature ships on both platforms`);
            }
          }
          const claude = readIfPresent('CLAUDE.md');
          if (claude && !/never compiled/i.test(claude)) {
            offenders.push(
              'CLAUDE.md does not state that macOS has never been compiled — ' +
                'removing the parity claim is not the same as replacing it',
            );
          }
          return offenders.length === 0
            ? { ok: true, detail: 'CLAUDE.md and SECURITY.md state the actual position' }
            : { ok: false, detail: offenders.join('; ') };
        },
      },
    ],
  },
  {
    id: 'AC22',
    title: 'Toolchain clean: cargo test (debug and release), clippy pedantic, rustfmt, tsc, eslint, prettier, cargo-deny, cargo-audit, ts-rs no-diff',
    checks: [
      // Checked first: if the frozen suite has been altered, nothing below it is
      // evidence of anything.
      { name: 'frozen acceptance suite unchanged', run: 1, exec: 'node scripts/check-freeze.mjs' },
      { name: 'crypto path is one stable generation', run: 1, exec: 'node scripts/check-crypto-generation.mjs' },
      { name: 'cargo fmt', run: 1, exec: 'cargo fmt --all --check' },
      // Pedantic everywhere except the frozen acceptance crate. Frozen code can
      // never be updated to satisfy a lint added by a future clippy release, so
      // holding it to a lint set that grows over time guarantees an eventual
      // unfixable failure — and the only ways out would be editing a frozen test
      // or disabling the lint for real code. It still gets `-D warnings` below,
      // so correctness lints apply; only the evolving style tier is dropped.
      {
        name: 'cargo clippy pedantic (production code)',
        run: 1,
        exec: 'cargo clippy --workspace --exclude keyring-acceptance --all-targets --all-features -- -D warnings -W clippy::pedantic',
      },
      {
        name: 'cargo clippy (frozen acceptance suite)',
        run: 1,
        exec: 'cargo clippy -p keyring-acceptance --all-targets -- -D warnings',
      },
      { name: 'cargo test (debug)', run: 1, exec: 'cargo test --workspace --all-features' },
      { name: 'redaction test in release', run: 1, exec: 'cargo test -p keyring-crypto --release --test redaction' },
      { name: 'lock/zeroize test in release', run: 2, cargoTest: 'cargo test -p keyring --test lock_zeroize --release' },
      { name: 'cargo deny', run: 1, exec: 'cargo deny check' },
      { name: 'cargo audit', run: 1, exec: 'cargo audit --deny warnings' },
      { name: 'tsc', run: 1, exec: 'pnpm run --silent typecheck' },
      { name: 'eslint', run: 1, exec: 'pnpm run --silent lint' },
      { name: 'prettier', run: 1, exec: 'pnpm run --silent format:check' },
      { name: 'ts-rs generated types match', run: 2, exec: 'pnpm check:ts-rs' },
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
    let r;
    if (check.fn) r = check.fn();
    else if (check.cargoTest) r = cargoTest(check.cargoTest);
    else r = exec(check.exec);
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

  if (!ensureFrontendBuilt()) return 1;

  const started = Date.now();
  const results = [];

  if (!OPTS.json) {
    console.log('');
    console.log('Keyring — SPEC-V1 §11 acceptance verifier');
    console.log(`host ${process.platform}/${process.arch} · node ${process.versions.node}`);
    if (OPTS.run.length) console.log(`filter: run ${OPTS.run.join(',')}`);
    if (OPTS.only.length) console.log(`filter: ${OPTS.only.join(',')}`);
    console.log('');
    for (const line of platformBanner()) console.log(line);
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
        {
          platform: process.platform,
          verifiedPlatform: VERIFIED_PLATFORM,
          // A consumer reading only `pass`/`fail` would conclude the product is
          // verified. ADD-005 says no output may imply that, machine-readable
          // included.
          unverifiedPlatforms: UNVERIFIED_PLATFORMS,
          coversThisRun: process.platform === VERIFIED_PLATFORM ? 'the verified platform' : 'an UNVERIFIED platform',
          pass,
          fail,
          skip,
          elapsedMs: Date.now() - started,
          results,
        },
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
  // Repeated deliberately. `VERIFY OK` is the line that gets quoted in a commit
  // message or a status update, and on its own it reads as "the product is fine".
  // ADD-005 requires that it never read that way while a platform is unverified.
  for (const line of platformBanner()) console.log(line);
  console.log('');

  return fail === 0 ? 0 : 1;
}

process.exit(main());
