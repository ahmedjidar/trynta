#!/usr/bin/env node
// Whether the E2E harness can run, and exactly what is missing if it cannot.
//
// The harness drives the real Tauri binary through `tauri-driver`, which on Windows
// proxies to `msedgedriver.exe`. Both are environment prerequisites rather than
// dependencies, so `pnpm install` cannot supply them and a missing one reports as a
// WebDriver session failure — which reads like a broken test rather than a missing tool.
//
// This turns that into a checklist with the exact commands, because AC17 and AC18 are
// unreachable without it and "the E2E suite is red" is not a useful thing to be told.

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Whether a command resolves on PATH. */
function onPath(command) {
  try {
    execFileSync(process.platform === 'win32' ? 'where' : 'which', [command], {
      stdio: 'ignore',
    });
    return true;
  } catch {
    return false;
  }
}

/** The installed WebView2 runtime version, or null. */
function webview2Version() {
  if (process.platform !== 'win32') return null;
  const key =
    'HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\' +
    '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  try {
    const out = execFileSync(
      'powershell',
      ['-NoProfile', '-Command', `(Get-ItemProperty '${key}').pv`],
      { encoding: 'utf8' },
    );
    return out.trim() || null;
  } catch {
    return null;
  }
}

const checks = [];

const binary = process.env.TRYNTA_E2E_BINARY ?? join(ROOT, 'target', 'debug', 'keyring.exe');
checks.push({
  name: 'app binary',
  ok: existsSync(binary),
  detail: binary,
  fix: 'cargo build   (or set TRYNTA_E2E_BINARY to a release build)',
});

checks.push({
  name: 'tauri-driver',
  ok: onPath('tauri-driver'),
  detail: 'proxies WebDriver to the Tauri webview',
  fix: 'cargo install tauri-driver --locked',
});

const runtime = webview2Version();
checks.push({
  name: 'WebView2 runtime',
  ok: runtime !== null,
  detail: runtime ?? 'not detected',
  fix: 'install the Evergreen WebView2 Runtime from Microsoft',
});

checks.push({
  name: 'msedgedriver',
  ok: onPath('msedgedriver'),
  detail:
    runtime === null
      ? 'must match the installed WebView2 runtime'
      : `must match WebView2 ${runtime}`,
  // Deliberately not automated. The driver is a signed binary whose version has to
  // match an evergreen runtime, so a build step that downloaded one would silently go
  // stale — and a mismatched driver fails at session creation, which looks like a
  // broken test suite rather than a stale tool.
  fix:
    runtime === null
      ? 'download Edge WebDriver and put msedgedriver.exe on PATH'
      : `download Edge WebDriver ${runtime} from ` +
        'https://developer.microsoft.com/microsoft-edge/tools/webdriver/ ' +
        'and put msedgedriver.exe on PATH',
});

const missing = checks.filter((c) => !c.ok);

console.log('check:e2e-ready — prerequisites for AC17 and AC18\n');
for (const check of checks) {
  console.log(`  ${check.ok ? '[ ok ]' : '[MISSING]'} ${check.name}  ${check.detail}`);
}

if (missing.length > 0) {
  console.log('\nTo fix:\n');
  for (const check of missing) console.log(`  ${check.name}:  ${check.fix}`);
  console.log(
    '\nAC17 (dark and light both render) and AC18 (adoptedStyleSheets under the\n' +
      'production CSP) cannot be verified without these. Both are unreachable in\n' +
      'happy-dom: it resolves no cascade and enforces no CSP, so a passing unit test\n' +
      'would prove nothing.',
  );
  process.exit(1);
}

console.log('\nAll prerequisites present. Run: pnpm e2e');
process.exit(0);
