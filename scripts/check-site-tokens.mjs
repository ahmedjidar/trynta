#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// The marketing site copies the app's light palette and scales into plain CSS,
// because it is served standalone and cannot import a file that also carries the
// whole dark theme, the row heights and the z-index scale. A copy drifts. This
// makes the drift a build failure instead of something somebody notices in a
// screenshot six months later.
//
// For each pair below, the value in `web/src/css/site.css` must equal the value
// of the named token in `src/theme/tokens.css` — read from the `[data-theme="light"]`
// block where there is one, and from `:root` otherwise.
//
// Four values are deliberately *different*, and they are listed separately: HO-003's
// `a11y.css` corrects them because the raw token fails WCAG AA in the use the page
// puts it to. Those are asserted to differ, so silently "fixing" one back to the raw
// token also fails.

import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const TOKENS = join(ROOT, 'src', 'theme', 'tokens.css');
const SITE = join(ROOT, 'web', 'src', 'css', 'site.css');

/** site variable -> app token. Must match exactly. */
const MIRRORED = {
  '--app': '--surface-app',
  '--panel': '--surface-panel',
  '--raised': '--surface-raised',
  '--hover': '--surface-hover',
  '--selected': '--surface-selected',
  '--ink': '--text-primary',
  '--ink-2': '--text-secondary',
  '--on-accent': '--text-on-accent',
  '--hairline': '--border-hairline',
  '--strong': '--border-strong',
  '--accent': '--accent',
  '--accent-hover': '--accent-hover',
  '--accent-subtle': '--accent-subtle',
  '--radius-xs': '--radius-xs',
  '--radius-sm': '--radius-sm',
  '--radius-md': '--radius-md',
  '--radius-lg': '--radius-lg',
  '--radius-xl': '--radius-xl',
  '--radius-2xl': '--radius-2xl',
  '--radius-window': '--radius-window',
  '--radius-full': '--radius-full',
  '--text-micro': '--text-micro',
  '--text-badge': '--text-badge',
  '--text-chip': '--text-chip',
  '--text-caption': '--text-caption',
  '--text-control': '--text-control',
  '--text-body': '--text-body',
  '--text-lead': '--text-heading',
  '--text-title': '--text-title',
  '--text-display': '--text-display',
  '--text-hero': '--text-metric',
  '--weight-regular': '--weight-regular',
  '--weight-medium': '--weight-medium',
  '--weight-bold': '--weight-bold',
  '--weight-heavy': '--weight-heavy',
  '--tracking-label': '--tracking-label',
  '--tracking-badge': '--tracking-badge',
  '--tracking-tight': '--tracking-tight',
  '--tracking-title': '--tracking-title',
  '--tracking-display': '--tracking-display',
  '--tracking-hero': '--tracking-metric',
  '--ease-standard': '--ease-standard',
  '--ease-spring': '--ease-spring',
  '--measure': '--measure-pane-wide',
  '--prose': '--measure-prose',
  '--window': '--window-width',
  '--row-header': '--row-toolbar',
};

/**
 * site variable -> the app token it deliberately departs from, and why.
 *
 * Asserted to *differ*. A raw token restored here would pass the mirror check
 * above and quietly reintroduce a contrast failure.
 */
const CORRECTED = {
  '--ink-3': ['--text-muted', 'a11y finding 2 — raw fails AA on --surface-app'],
  '--accent-text': ['--accent', 'a11y finding 5 — the fill accent is 4.34:1 as text'],
  '--warn': ['--status-warning', 'a11y finding 6 — raw is 3.68:1 on panel'],
  '--danger': ['--status-danger', 'a11y finding 7 — raw is 3.99:1 in its own fill'],
};

/** Durations are written in ms on the site and in s in the app. */
const DURATIONS = {
  '--duration-instant': '--duration-instant',
  '--duration-base': '--duration-base',
  '--duration-moderate': '--duration-moderate',
  '--duration-slow': '--duration-slow',
};

/** Read declarations from one block of a stylesheet. */
function declarations(css, selector) {
  const at = css.indexOf(selector);
  if (at === -1) return {};
  const open = css.indexOf('{', at);
  const close = css.indexOf('\n}', open);
  const body = css.slice(open + 1, close);
  const out = {};
  for (const m of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    out[m[1]] = m[2].replaceAll(/\s+/g, ' ').trim();
  }
  return out;
}

/**
 * Compare ignoring the ways CSS spells the same number.
 *
 * `0.5` and `.5` are one value, and so are `.10` and `.1` — the app writes the
 * first of each pair in places and the second in others. Trailing zeros are
 * trimmed only at the end of a decimal run, so `.07` survives intact; hex
 * colours contain no dot and are never touched.
 */
function same(a, b) {
  const norm = (v) =>
    v
      .toLowerCase()
      .replaceAll(/\s+/g, '')
      .replaceAll(/,/g, ' ')
      .replaceAll(/(^|[^\d])0\./g, '$1.')
      .replaceAll(/(\.\d*?)0+(?!\d)/g, '$1')
      .replaceAll(/\.(?!\d)/g, '');
  return norm(a) === norm(b);
}

/** `.26s` and `260ms` are the same duration. */
function sameDuration(app, site) {
  const ms = (v) => {
    const n = Number.parseFloat(v);
    return v.trim().endsWith('ms') ? n : n * 1000;
  };
  return Math.abs(ms(app) - ms(site)) < 0.001;
}

const tokensCss = readFileSync(TOKENS, 'utf8');
const siteCss = readFileSync(SITE, 'utf8');

const appRoot = declarations(tokensCss, ':root {');
const appLight = declarations(tokensCss, '[data-theme="light"] {');
const site = declarations(siteCss, ':root {');

const app = (name) => appLight[name] ?? appRoot[name];

const problems = [];
let checked = 0;

for (const [siteVar, appVar] of Object.entries(MIRRORED)) {
  const expected = app(appVar);
  const actual = site[siteVar];
  if (expected === undefined) {
    problems.push(`${appVar} is not defined in tokens.css — the map in this script is stale`);
    continue;
  }
  if (actual === undefined) {
    problems.push(`${siteVar} is missing from site.css`);
    continue;
  }
  checked += 1;
  if (!same(expected, actual)) {
    problems.push(`${siteVar} is "${actual}" but ${appVar} is "${expected}"`);
  }
}

for (const [siteVar, appVar] of Object.entries(DURATIONS)) {
  const expected = app(appVar);
  const actual = site[siteVar];
  if (expected === undefined || actual === undefined) {
    problems.push(`${siteVar} / ${appVar} — one of the pair is missing`);
    continue;
  }
  checked += 1;
  if (!sameDuration(expected, actual)) {
    problems.push(`${siteVar} is "${actual}" but ${appVar} is "${expected}"`);
  }
}

for (const [siteVar, [appVar, why]] of Object.entries(CORRECTED)) {
  const raw = app(appVar);
  const actual = site[siteVar];
  if (raw === undefined || actual === undefined) {
    problems.push(`${siteVar} / ${appVar} — one of the pair is missing`);
    continue;
  }
  checked += 1;
  if (same(raw, actual)) {
    problems.push(
      `${siteVar} has been set back to the raw ${appVar} ("${raw}"). That is the ` +
        `contrast failure the correction exists for — ${why}.`,
    );
  }
}

if (checked === 0) {
  console.error('check:site-tokens — nothing compared. The parser is broken, not the CSS.');
  process.exit(1);
}

console.log(
  `check:site-tokens — ${String(checked)} values compared against src/theme/tokens.css ` +
    `(${String(Object.keys(CORRECTED).length)} deliberate a11y corrections)`,
);

if (problems.length > 0) {
  console.error(`\n${String(problems.length)} problem(s):\n`);
  for (const p of problems) console.error(`  ${p}`);
  console.error(
    "\nThe site copies the app's light palette so it can be served standalone. " +
      'When one moves, move the other — or the two products stop looking like one.',
  );
  process.exit(1);
}

process.exit(0);
