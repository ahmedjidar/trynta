// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The rules `check-network.mjs` enforces, and the pure function that applies them.
 *
 * Separated from the walker so the rules can be tested directly. A security check
 * whose patterns are only ever exercised by running it over this one repository is
 * a check nobody can safely tighten or loosen: the only evidence either way is
 * "it still passes", which is equally true of a pattern that matches nothing.
 *
 * `check-network.mjs` keeps the directory walk, the sanctioned-file allow-list and
 * the reporting. It also keeps executing unconditionally — this module is imported
 * by it, never the other way round, so there is no arrangement in which the check
 * quietly becomes a no-op.
 *
 * **This file is excluded from the scan**, exactly as the checker is, and for the
 * same reason: it names the forbidden hosts and spells out the shapes of the
 * requests it forbids. A file that defines a rule cannot also be judged by it.
 */

/**
 * Hosts that must never appear in shipped source.
 *
 * Split across an array and joined so this file does not trip its own check — the
 * point of the list is that the *literal* must not exist in code, and writing it
 * here as one string would be the thing it forbids.
 */
export const FORBIDDEN_HOSTS = [
  ['google', '.com/s2/favicons'], // what the HO-001 prototype used
  ['www.google', '.com/s2'],
  ['favicon', '.yandex.net'],
  ['icons.duckduckgo', '.com'],
  ['logo.clearbit', '.com'],
  ['gstatic', '.com'],
  ['fonts.googleapis', '.com'], // font-src 'self'; a webfont must be bundled
  ['fonts.gstatic', '.com'],
  ['cdn.jsdelivr', '.net'],
  ['unpkg', '.com'],
  ['cdnjs.cloudflare', '.com'],
].map((parts) => parts.join(''));

export const RUST = new Set(['.rs']);
export const WEB = new Set(['.ts', '.tsx', '.js', '.jsx', '.mjs', '.html']);
export const STYLESHEET = new Set(['.css', '.html']);

/** The image extensions a favicon probe would ask for. */
const ICON_EXT = String.raw`favicon\.(?:ico|png|svg|jpg)`;

/** Characters that end a URL inside a string literal. */
const URLISH = String.raw`[^\s'"\`)<>]*`;

/**
 * A favicon reference that is a **URL**, rather than a path.
 *
 * The distinction is the whole rule and it was not made at first: the original
 * pattern was a bare `favicon\.(ico|png|svg|jpg)`, which flagged this repository's
 * own build scripts for writing `${ROOT}/public/favicon.ico` to disk. Three
 * findings, none of them a request, in the check whose job is to find requests.
 * A guard that cries wolf is a guard people learn to skip.
 *
 * So a favicon path is only a finding when something on the line makes it remote.
 * There are five ways to write that, and each alternative below is one of them:
 *
 * 1. **A scheme and authority.** `https://example.test/favicon.ico`, including the
 *    interpolated form `https://${domain}/favicon.ico`. This is the shape ADD-001's
 *    verification list has in mind.
 * 2. **Protocol-relative**, inside a string or attribute: `"//cdn.example/favicon.ico"`.
 * 3. **An interpolation or format placeholder joined straight onto the filename**:
 *    `` `${origin}/favicon.ico` `` or `format!("{}/favicon.ico", host)`. No scheme
 *    appears on the line, so alternative 1 would miss it — and this is the form a
 *    leak actually takes, because the origin is a variable. It requires the
 *    placeholder to be *adjacent*: `${ROOT}/public/favicon.ico` has a path segment
 *    in between and is not a URL.
 * 4. **Concatenation onto a base**: `origin + '/favicon.ico'`.
 * 5. **`new URL('/favicon.ico', origin)`**, which is a request waiting for a fetch.
 *
 * What this deliberately no longer catches is a favicon path with nothing remote
 * about it. That is not a weakening: a bare relative path resolves against the
 * app's own origin under `img-src 'self'`, so it cannot leave the machine, and the
 * `FORBIDDEN_HOSTS` list catches the known icon services in any language and any
 * syntax regardless of this rule.
 */
export const FAVICON_URL = new RegExp(
  [
    String.raw`://${URLISH}${ICON_EXT}`,
    String.raw`['"\`]\s*//${URLISH}${ICON_EXT}`,
    String.raw`(?:\$\{[^{}]*\}|\{[A-Za-z_]\w*\}|\{\})\s*/\s*${ICON_EXT}`,
    String.raw`\+\s*['"\`]\s*/${ICON_EXT}`,
    String.raw`URL\s*\(\s*['"\`]\s*/?${ICON_EXT}`,
  ].join('|'),
  'i',
);

/**
 * Patterns that mean "this code can make a request", scoped by language.
 *
 * The scoping is not tidiness. `fetch` is a network call in TypeScript and an
 * ordinary method name in Rust — `RangeSource::fetch` is the trait the HIBP
 * transport implements, and flagging it would have taught everyone to ignore this
 * check on its first run, which is how a guard stops guarding.
 */
export const CLIENTS = [
  { name: 'ureq client', re: /\bureq\s*::/, exts: RUST },
  { name: 'reqwest', re: /\breqwest\s*::/, exts: RUST },
  { name: 'hyper client', re: /\bhyper\s*::\s*Client/, exts: RUST },
  { name: 'std TcpStream', re: /\bTcpStream\s*::\s*connect/, exts: RUST },
  { name: 'fetch()', re: /(?<![\w.])fetch\s*\(/, exts: WEB },
  { name: 'XMLHttpRequest', re: /\bXMLHttpRequest\b/, exts: WEB },
  { name: 'EventSource', re: /\bnew\s+EventSource\b/, exts: WEB },
  { name: 'WebSocket', re: /\bnew\s+WebSocket\b/, exts: WEB },
  { name: 'navigator.sendBeacon', re: /\bsendBeacon\s*\(/, exts: WEB },
  { name: 'tauri http plugin', re: /tauri[-_]plugin[-_]http/, exts: new Set([...RUST, ...WEB]) },

  // ── ADD-001: no icon URL is ever constructed at runtime ────────────────────
  //
  // The clients above catch code that *makes* a request. These catch code that
  // *hands a URL to the platform* and lets it make the request — which is how the
  // favicon leak arrives in practice: not `fetch()`, but
  // `<img src={`https://${domain}/favicon.ico`}>`. A URL in an `<img>` is still a
  // packet on the wire, and it still names the service.
  //
  // Web files only, for the two URL rules. A Rust string containing a URL is a
  // fixture, an error message or the theme validator's own rejection test; the two
  // files that genuinely make requests are on the sanctioned list, and the host
  // list applies to them regardless.
  //
  // The bundled tiles pass because they are root-relative — `/icons/<key>.svg`,
  // resolved against the app's own origin under `img-src 'self'`. `IconDto` carries
  // a bundle key and never a domain, so there is nothing in the webview to build a
  // remote URL from even by accident; these rules exist so that stays true.
  { name: 'absolute URL in web source', re: /\bhttps?:\/\//, exts: WEB, shipped: true },
  {
    name: 'protocol-relative URL in a src/href',
    re: /\b(?:src|href)\s*=\s*\{?\s*[`"']\s*\/\//,
    exts: WEB,
    shipped: true,
  },
  { name: 'favicon probe', re: FAVICON_URL, exts: new Set([...RUST, ...WEB]) },
  {
    name: 'well-known probe',
    re: /\.well-known\/(?:change-password|security\.txt)/i,
    exts: new Set([...RUST, ...WEB]),
  },
];

/**
 * `@import url(...)` and remote `url()` — **stylesheets only**.
 *
 * Not applied to Rust or TypeScript, and the reason is a test that would otherwise
 * fail this check for doing its job: `theme_import`'s fixtures contain
 * `url(https://attacker.example)` precisely to assert the validator rejects it. A
 * remote URL inside a Rust string is not a stylesheet, and flagging it would mean
 * the only way to pass is to stop testing the attack.
 *
 * Real remote CSS is still caught, in CSS, and known hosts are caught everywhere by
 * `FORBIDDEN_HOSTS` regardless of language.
 */
export const CSS_REMOTE = [
  { name: 'remote @import', re: /@import\s+(?:url\()?['"]?https?:/i },
  { name: 'remote url()', re: /url\(\s*['"]?(?:https?:)?\/\//i },
];

/**
 * Apply every rule to one file's lines.
 *
 * @param {object} file
 * @param {string[]} file.code - Lines with comments already stripped.
 * @param {string} file.ext - The file's extension, including the dot.
 * @param {boolean} file.isShipped - Whether this file reaches the webview bundle.
 * @param {boolean} file.isSanctioned - Whether it is on the allow-list.
 * @returns {{ line: number, rule: string, detail: string }[]} findings, 1-based.
 */
export function scanSource({ code, ext, isShipped, isSanctioned }) {
  const findings = [];

  code.forEach((line, i) => {
    const at = i + 1;

    for (const host of FORBIDDEN_HOSTS) {
      // No exemption, sanctioned or not. HIBP and the update endpoint are not on
      // this list; a CDN or favicon host in either of them is still a bug.
      if (line.includes(host)) findings.push({ line: at, rule: 'remote host', detail: host });
    }

    if (STYLESHEET.has(ext)) {
      for (const { name, re } of CSS_REMOTE) {
        if (re.test(line)) findings.push({ line: at, rule: name, detail: '' });
      }
    }

    if (isSanctioned) return;
    for (const { name, re, exts, shipped } of CLIENTS) {
      if (!exts.has(ext)) continue;
      if (shipped === true && !isShipped) continue;
      if (re.test(line)) findings.push({ line: at, rule: name, detail: '' });
    }
  });

  return findings;
}
