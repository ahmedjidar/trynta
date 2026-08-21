// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The network rules, exercised against snippets rather than against this repository.
 *
 * This exists because of a specific failure: the favicon rule was a bare
 * `favicon\.(ico|png|svg|jpg)` and flagged the build scripts for *writing*
 * `${ROOT}/public/favicon.ico` to disk. Three findings, none of them a request, in
 * the check whose whole job is to find requests — and a guard that cries wolf is a
 * guard people learn to skip.
 *
 * Narrowing a security pattern is the kind of change that is easy to overshoot, and
 * "it still passes on our repo" is equally true of a pattern that matches nothing.
 * So both directions are pinned here: every shape a favicon URL is actually written
 * in must still be a finding, and the local paths that caused the false positives
 * must not be. The rest of the rule set is covered too, so a future edit to one
 * pattern cannot silently disable another.
 */

import { describe, expect, it } from 'vitest';

import { CLIENTS, FORBIDDEN_HOSTS, scanSource } from './network-rules.mjs';

/** Run one line through the rules as a shipped webview file would be. */
function scan(line, { ext = '.ts', isShipped = true, isSanctioned = false } = {}) {
  return scanSource({ code: [line], ext, isShipped, isSanctioned });
}

/** The rule names a line trips. */
function rules(line, opts) {
  return scan(line, opts).map((f) => f.rule);
}

describe('favicon probes', () => {
  // Every one of these is a request leaving the machine with the user's own
  // domain in the path, which is the leak ADD-001 exists to prevent.
  const PROBES = [
    ['a literal URL', `const u = 'https://example.test/favicon.ico';`],
    ['an interpolated host', 'const u = `https://${domain}/favicon.ico`;'],
    ['an img src', 'return <img src={`https://${domain}/favicon.ico`} alt="" />;'],
    ['a fetch', `await fetch('https://example.test/favicon.png');`],
    ['protocol-relative', `const u = '//cdn.example.test/favicon.ico';`],
    ['an origin joined onto the file', 'const u = `${origin}/favicon.ico`;'],
    ['concatenation onto a base', `const u = origin + '/favicon.svg';`],
    ['new URL against an origin', `const u = new URL('/favicon.ico', origin);`],
    ['an uppercase spelling', `const u = 'HTTPS://EXAMPLE.TEST/FAVICON.ICO';`],
    ['a jpg', 'const u = `https://${d}/favicon.jpg`;'],
  ];

  for (const [what, line] of PROBES) {
    it(`still fails on ${what}`, () => {
      expect(rules(line), line).toContain('favicon probe');
    });
  }

  it('still fails in Rust, where the absolute-URL rule does not apply', () => {
    // `absolute URL in web source` is web-only and shipped-only, so in Rust the
    // favicon rule is the only thing standing between a format string and a probe.
    const rust = [
      `let url = format!("https://{host}/favicon.ico");`,
      `let url = format!("{}/favicon.ico", origin);`,
    ];
    for (const line of rust) {
      expect(rules(line, { ext: '.rs', isShipped: false }), line).toContain('favicon probe');
    }
  });

  it('still fails inside a sanctioned file, because the host list has no exemption', () => {
    // A sanctioned file skips the client rules — it is allowed to make its one
    // request — but a known icon host in it is still a bug.
    const line = `let u = "https://www.google.com/s2/favicons?domain=x";`;
    expect(rules(line, { ext: '.rs', isShipped: false, isSanctioned: true })).toContain(
      'remote host',
    );
  });
});

describe('favicon paths that are not probes', () => {
  // The three lines that caused the false positives, verbatim from the build
  // scripts. None of them is a URL; all three write or read a local file.
  const LOCAL = [
    'writeFileSync(`${ROOT}/public/favicon.ico`, ico([16, 20, 24, 32, 48], frames));',
    'console.log(`\\n  public/favicon.ico`);',
    "checkIco(`${ROOT}/public/favicon.ico`, [16, 32], 'public/favicon.ico');",
  ];

  for (const line of LOCAL) {
    it(`does not flag ${line.slice(0, 44)}…`, () => {
      expect(rules(line, { ext: '.mjs', isShipped: false }), line).not.toContain('favicon probe');
    });
  }

  it('does not flag a root-relative reference in shipped source', () => {
    // Resolved against the app's own origin under `img-src 'self'`, so it cannot
    // leave the machine — the same reason the bundled tiles are allowed.
    expect(rules('<link rel="icon" href="/favicon.ico" />', { ext: '.html' })).not.toContain(
      'favicon probe',
    );
  });
});

describe('the rest of the rule set', () => {
  it('flags every HTTP client in the language it belongs to', () => {
    expect(rules('const r = await fetch(url);')).toContain('fetch()');
    expect(rules('new XMLHttpRequest();')).toContain('XMLHttpRequest');
    expect(rules('navigator.sendBeacon(url, body);')).toContain('navigator.sendBeacon');
    expect(rules('let r = ureq::get(url);', { ext: '.rs', isShipped: false })).toContain(
      'ureq client',
    );
    expect(rules('let r = reqwest::get(url);', { ext: '.rs', isShipped: false })).toContain(
      'reqwest',
    );
  });

  it('does not flag Rust methods that merely share a name with a web API', () => {
    // `RangeSource::fetch` is the trait the HIBP transport implements. Flagging it
    // would have taught everyone to ignore this check on its first run.
    expect(
      rules('fn fetch(&self, prefix: &str) -> Result<String, HibpError> {', {
        ext: '.rs',
        isShipped: false,
      }),
    ).toEqual([]);
  });

  it('flags every forbidden host, in any language', () => {
    for (const host of FORBIDDEN_HOSTS) {
      const line = `let s = "https://${host}/x";`;
      expect(rules(line, { ext: '.rs', isShipped: false }), host).toContain('remote host');
    }
  });

  it('flags a well-known probe', () => {
    expect(rules(`const u = base + '/.well-known/change-password';`)).toContain('well-known probe');
  });

  it('flags remote CSS, and only in a stylesheet', () => {
    const line = "@import url('https://fonts.example.test/x.css');";
    expect(rules(line, { ext: '.css', isShipped: true })).toContain('remote @import');
    // The theme validator's own rejection fixtures contain remote URLs in Rust
    // strings, precisely to assert they are refused. Flagging those would mean the
    // only way to pass is to stop testing the attack.
    expect(
      rules(`let bad = "url(https://attacker.example)";`, {
        ext: '.rs',
        isShipped: false,
      }),
    ).toEqual([]);
  });

  it('applies the shipped-only rules to the webview bundle alone', () => {
    const line = `console.log('download it from https://example.test/driver');`;
    expect(rules(line, { ext: '.mjs', isShipped: false })).toEqual([]);
    expect(rules(line, { ext: '.ts', isShipped: true })).toContain('absolute URL in web source');
  });

  it('reports the line a finding is on', () => {
    const found = scanSource({
      code: ['const a = 1;', 'const b = 2;', `await fetch('/x');`],
      ext: '.ts',
      isShipped: true,
      isSanctioned: false,
    });
    expect(found).toHaveLength(1);
    expect(found[0]?.line).toBe(3);
  });

  it('keeps every rule reachable — none is scoped to no extension', () => {
    // A rule with an empty `exts` set never runs and never fails, which is the
    // quietest way for a check to stop checking.
    for (const rule of CLIENTS) {
      expect(rule.exts.size, rule.name).toBeGreaterThan(0);
    }
  });
});
