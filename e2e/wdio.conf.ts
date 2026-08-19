/**
 * E2E harness — WebdriverIO against the real Tauri binary (SPEC-V1 §11, AC17b, AC18).
 *
 * ## Why this exists and why it cannot be jsdom
 *
 * Two acceptance criteria are unreachable any other way:
 *
 * - **AC17b**: *"Dark and light both render."* A unit test can assert that
 *   `applyTheme('light')` sets an attribute. Only a real engine resolves
 *   `var(--surface-panel)` through the cascade and reports a computed colour, which is
 *   what the criterion is actually about.
 * - **AC18**: *"Runtime theme swap works under production CSP."* happy-dom enforces no
 *   CSP, so a passing unit test proves nothing. The whole question is whether
 *   `adoptedStyleSheets` survives `style-src 'self'`, and the only way to know is to run
 *   under it.
 *
 * ADD-005 makes Windows the verified platform, so this runs against **WebView2**. The
 * WKWebView half of AC18 stays unverified and `MACOS-UNVERIFIED.md` carries it.
 *
 * ## Not Playwright
 *
 * CLAUDE.md §2 names WebdriverIO with `@wdio/tauri-service` and gives the reason:
 * Playwright drives browsers, not a Tauri binary hosting a webview. There is no browser
 * process here to attach to.
 */

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * The binary under test.
 *
 * Debug by default: a release build takes minutes and nothing here measures performance.
 * `TRYNTA_E2E_BINARY` overrides it, which is what CI uses after `tauri build`.
 */
const BINARY = process.env.TRYNTA_E2E_BINARY ?? join(ROOT, 'target', 'debug', 'keyring.exe');

/** Whether a command resolves on PATH. */
function onPath(command: string): boolean {
  try {
    execFileSync(process.platform === 'win32' ? 'where' : 'which', [command], {
      stdio: 'ignore',
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * Fail on a missing prerequisite with a sentence, not a stack trace.
 *
 * Without this, a missing `msedgedriver.exe` surfaces as a WebDriver session-creation
 * error several frames deep inside the service — which reads as "the E2E suite is broken"
 * rather than "one tool is not installed". AC17b and AC18 are gated on this harness, so
 * this message is the only thing that tells anyone which of the two it is.
 */
function requirePrerequisites(): void {
  const problems: string[] = [];

  if (!existsSync(BINARY)) {
    problems.push(
      [
        `app binary missing at ${BINARY}`,
        '  Fix: cargo build   (or set TRYNTA_E2E_BINARY to a release build)',
      ].join('\n'),
    );
  }

  if (!onPath('tauri-driver')) {
    problems.push(
      ['tauri-driver missing', '  Fix: cargo install tauri-driver --locked'].join('\n'),
    );
  }

  if (!onPath('msedgedriver')) {
    problems.push(
      [
        'msedgedriver missing — run `pnpm e2e:setup`',
        '  Fix: pnpm e2e:setup prints the exact version to match this machine',
        '       WebView2 runtime, and the download URL.',
      ].join('\n'),
    );
  }

  if (problems.length > 0) {
    throw new Error(
      [
        '',
        'E2E prerequisites are not met, so AC17b and AC18 cannot run.',
        '',
        ...problems,
        '',
        'Neither criterion is reachable in happy-dom: it resolves no cascade and',
        'enforces no CSP, so a passing unit test would prove nothing.',
        '',
      ].join('\n'),
    );
  }
}

requirePrerequisites();

export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: [join(ROOT, 'e2e', 'specs', '**', '*.e2e.ts')],
  maxInstances: 1,

  capabilities: [
    {
      // The service translates this into a tauri-driver session.
      'tauri:options': { application: BINARY },
      browserName: 'wry',
    } as WebdriverIO.Capabilities,
  ],

  // `external` means tauri-driver, which is the prerequisite `pnpm e2e:setup` checks
  // for. The service's default is an *embedded* WebDriver that requires
  // `tauri-plugin-wdio-webdriver` registered inside the app — a test-only plugin
  // compiled into the shipping binary, which is not a trade this product makes for two
  // acceptance criteria. Without this line the run fails in `onPrepare` waiting for a
  // port nothing is listening on.
  services: [['tauri', { driverProvider: 'external' }]],
  framework: 'mocha',
  reporters: ['spec'],

  // A cold Rust binary opening a webview is slow on first launch, and a timeout here
  // reads as a failing assertion rather than as a slow start.
  waitforTimeout: 15_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 2,

  mochaOpts: {
    ui: 'bdd',
    timeout: 120_000,
  },

  logLevel: 'warn',
};
