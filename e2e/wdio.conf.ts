/**
 * E2E harness — WebdriverIO against the real Tauri binary (SPEC-V1 §11, AC17, AC18).
 *
 * ## Why this exists and why it cannot be jsdom
 *
 * Two acceptance criteria are unreachable any other way:
 *
 * - **AC17**: *"Dark and light both render."* A unit test can assert that
 *   `applyTheme('light')` sets an attribute. Only a real engine resolves
 *   `var(--surface-panel)` through the cascade and reports a computed colour, which is
 *   the thing the criterion is about.
 * - **AC18**: *"Runtime theme swap works under production CSP on both WKWebView and
 *   WebView2."* happy-dom enforces no CSP at all, so a passing unit test proves
 *   nothing. The whole question is whether `adoptedStyleSheets` survives
 *   `style-src 'self'` — and the only way to know is to run under it.
 *
 * ADD-005 makes Windows the verified platform, so this runs against **WebView2**. The
 * WKWebView half of AC18 stays unverified and `MACOS-UNVERIFIED.md` carries it.
 *
 * ## Not Playwright
 *
 * CLAUDE.md §2 names WebdriverIO with `@wdio/tauri-service` and gives the reason:
 * Playwright drives browsers, not a Tauri binary hosting a webview. There is no
 * browser process here to attach to.
 *
 * ## Prerequisites
 *
 * - `cargo install tauri-driver`
 * - `msedgedriver.exe` on PATH, matching the installed WebView2 runtime. Windows ships
 *   WebView2 evergreen, so the driver has to be refreshed when the runtime updates —
 *   a mismatch reports as a session-creation failure, not as a test failure.
 * - a built binary: `pnpm tauri build --debug`, or `cargo build` for the debug exe.
 */

import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * The binary under test.
 *
 * Debug by default: a release build takes minutes and nothing here measures
 * performance. `KEYRING_E2E_BINARY` overrides it, which is what CI uses after
 * `tauri build`.
 */
const BINARY = process.env.KEYRING_E2E_BINARY ?? join(ROOT, 'target', 'debug', 'keyring.exe');

if (!existsSync(BINARY)) {
  throw new Error(
    `E2E binary not found at ${BINARY}. Run \`cargo build\` first, or set ` +
      'KEYRING_E2E_BINARY. The harness drives the real app, not a dev server, ' +
      'because AC18 is about the production CSP.',
  );
}

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

  services: ['tauri'],
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
