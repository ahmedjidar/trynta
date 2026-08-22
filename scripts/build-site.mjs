#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Assemble `web/dist` from `web/src` plus the repository's own facts.
//
// ## Why there is a build step at all
//
// Two things on the pages are not writable by hand without keeping a second copy
// of something that already exists:
//
//   * the version, which is `package.json`'s;
//   * the changelog, which is `CHANGELOG.md`'s.
//
// Both are read from the repository and substituted here, so `CHANGELOG.md` stays
// the single copy of the release notes — the site renders it and the GitHub
// release body is pasted from it. Nothing is fetched to do this, which matters
// more than it might: the page argues that Trynta bundles brand icons because
// requesting them would disclose which services you hold accounts with, and a
// build that phoned an API to render that sentence would be a smaller version of
// the same mistake.
//
// ## There is no star count
//
// The page links to the repository and does not say how many stars it has. Three
// ways to produce one were tried and all three are gone. A browser fetch hands
// every visitor's IP to a third party in order to draw a number, on a page whose
// argument is that Trynta bundles brand icons rather than do exactly that. A
// build-time fetch would have made `check:network` count three outbound call
// sites in a repository whose pages claim two. And a build variable leaves a slot
// that is empty whenever nobody remembers to set it, which is worse than no slot.
//
// The link says Source.
//
// ## Output
//
// `web/dist` is generated and disposable. Nothing in it is edited by hand and it
// is not committed.

import { cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(ROOT, 'web', 'src');
const OUT = join(ROOT, 'web', 'dist');

// ── the repository's own facts ──────────────────────────────────────────────

/** The shipped version, from the one file that already holds it. */
async function version() {
  const pkg = JSON.parse(await readFile(join(ROOT, 'package.json'), 'utf8'));
  // Pre-1.0 releases are tagged `-alpha`; `package.json` carries the bare number
  // because Cargo and npm both reject some pre-release spellings the other
  // accepts. The suffix lives in CHANGELOG.md, which is what the pages show.
  return pkg.version;
}

/**
 * Parse `CHANGELOG.md` into releases.
 *
 * Keep a Changelog is regular enough to read with a small parser and irregular
 * enough that a general Markdown library would be more code than this. What it
 * recognises: `## [version] — date` opens a release, `### Type` opens a group,
 * and `- text` is an entry. Everything else in a release — the prose paragraph
 * under the heading — is kept as a lede.
 */
function parseChangelog(md) {
  const releases = [];
  let release = null;
  let type = null;
  let entry = null;

  const flush = () => {
    if (entry && release) {
      release.entries.push({
        type: type ?? 'Note',
        text: entry.join(' ').replaceAll(/\s+/g, ' ').trim(),
      });
    }
    entry = null;
  };

  for (const raw of md.split(/\r?\n/)) {
    const line = raw.trimEnd();

    const head = /^## \[([^\]]+)\](?:\s*[—-]\s*(.+))?$/.exec(line);
    if (head) {
      flush();
      release = { version: head[1], date: (head[2] ?? '').trim(), lede: [], entries: [] };
      type = null;
      // `[unreleased]: https://…` link definitions at the foot are not releases,
      // and neither is an Unreleased section with nothing in it.
      releases.push(release);
      continue;
    }
    if (!release) continue;

    const group = /^### (.+)$/.exec(line);
    if (group) {
      flush();
      type = group[1].trim();
      continue;
    }

    const bullet = /^[-*] (.+)$/.exec(line);
    if (bullet) {
      flush();
      entry = [bullet[1]];
      continue;
    }

    // A continuation of the current bullet.
    if (entry && /^\s+\S/.test(raw)) {
      entry.push(line.trim());
      continue;
    }

    flush();
    if (line !== '' && !line.startsWith('[') && !line.startsWith('<!--')) {
      if (type === null) release.lede.push(line);
    }
  }
  flush();

  return releases
    .filter((r) => r.entries.length > 0)
    .map((r) => ({ ...r, lede: r.lede.join(' ').replaceAll(/\s+/g, ' ').trim() }));
}

// ── rendering ───────────────────────────────────────────────────────────────

const ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' };

/** Escape for HTML text, then re-admit the two inline marks the notes use. */
function inline(text) {
  const escaped = text.replaceAll(/[&<>"]/g, (c) => ESCAPES[c] ?? c);
  return escaped
    .replaceAll(/`([^`]+)`/g, '<span class="mono">$1</span>')
    .replaceAll(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replaceAll(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '<a href="$2">$1</a>');
}

/** The type label's colour: security is red, a removal amber, the rest quiet. */
function typeClass(type) {
  const t = type.toLowerCase();
  if (t === 'security') return ' is-security';
  if (t === 'removed') return ' is-removed';
  return '';
}

function renderEntries(release) {
  return release.entries
    .map(
      (e) => `        <div class="rel-entry">
          <div class="rel-type${typeClass(e.type)}">${inline(e.type)}</div>
          <div class="rel-text">${inline(e.text)}</div>
        </div>`,
    )
    .join('\n');
}

/**
 * How many of the newest release's entries the landing page shows.
 *
 * The first release has twenty-odd, which is honest and also longer than the
 * rest of the page put together. The changelog page shows all of them; this is a
 * sample with an explicit count of what was left out, so it reads correctly at
 * two entries and at fifteen.
 */
const LANDING_ENTRIES = 6;

/** The landing page's block: the newest release, trimmed. */
function renderLatest(releases) {
  const latest = releases[0];
  if (!latest) {
    return '      <p class="lede prose muted">No release yet.</p>';
  }
  const shown = { ...latest, entries: latest.entries.slice(0, LANDING_ENTRIES) };
  const hidden = latest.entries.length - shown.entries.length;
  const more =
    hidden <= 0
      ? ''
      : `
      <p class="caption mt-4">${String(hidden)} more in the ` +
        `<a href="changelog.html">full changelog</a>, including what this release cannot do.</p>`;
  return `      <div class="rel-head">
        <span class="rel-version nums">${inline(latest.version)}</span>
        <span class="badge badge-accent">Latest</span>
        <span class="spacer"></span>
        <span class="caption nums">${inline(latest.date)}</span>
      </div>
      <div class="rel-entries">
${renderEntries(shown)}
      </div>${more}`;
}

/**
 * The changelog page: one timeline node per release.
 *
 * Designed to read correctly at one entry or fifteen. Each entry carries its own
 * left border and the last one's is transparent, so the spine terminates at the
 * final node instead of trailing past it — which is what a single absolutely
 * positioned track would do at any content length.
 */
function renderReleases(releases) {
  if (releases.length === 0) {
    return '        <p class="lede prose muted">No release yet.</p>';
  }
  return releases
    .map((r, i) => {
      const isLatest = i === 0;
      const security = r.entries.some((e) => e.type.toLowerCase() === 'security');
      const node = isLatest ? 'is-done' : security ? 'is-security' : 'is-later';
      const badge = isLatest
        ? '<span class="badge badge-accent">Latest</span>'
        : security
          ? '<span class="badge badge-danger">Security</span>'
          : '';
      const anchor = `v${r.version.replaceAll('.', '-')}`;
      return `        <div class="tl-entry">
          <div class="tl-node ${node}"></div>
          <div class="rel-head">
            <h2 class="rel-version nums" id="${anchor}">${inline(r.version)}</h2>
            ${badge}
            <span class="spacer"></span>
            <span class="caption nums">${inline(r.date)}</span>
          </div>
          ${r.lede === '' ? '' : `<p class="prose rel-lede">${inline(r.lede)}</p>`}
          <div class="rel-entries">
${renderEntries(r)}
          </div>
        </div>`;
    })
    .join('\n');
}

/**
 * Replace the contents of every `<!--BUILD:NAME-->…<!--/BUILD:NAME-->` pair.
 *
 * Scans forward with an index rather than rewriting the whole string in a loop.
 * The first version looped `while (out.includes(open))` and kept the marker, so
 * the condition stayed true for ever — the build hung with no output at all,
 * which is a much worse failure than a wrong substitution.
 */
function fill(html, name, replacement) {
  const open = `<!--BUILD:${name}-->`;
  const close = `<!--/BUILD:${name}-->`;
  let out = '';
  let at = 0;
  for (;;) {
    const start = html.indexOf(open, at);
    if (start === -1) break;
    const end = html.indexOf(close, start);
    if (end === -1) break;
    out += html.slice(at, start + open.length) + replacement;
    at = end;
  }
  return out + html.slice(at);
}

// ── build ───────────────────────────────────────────────────────────────────

const [v, md] = await Promise.all([version(), readFile(join(ROOT, 'CHANGELOG.md'), 'utf8')]);

const releases = parseChangelog(md);
if (releases.length === 0) {
  console.warn(
    'build:site — CHANGELOG.md parsed to no releases; both changelog blocks will say so',
  );
}

await rm(OUT, { recursive: true, force: true });
await mkdir(OUT, { recursive: true });
await cp(join(SRC, 'assets'), join(OUT, 'assets'), { recursive: true });
await cp(join(SRC, 'css'), join(OUT, 'css'), { recursive: true });
await cp(join(SRC, 'js'), join(OUT, 'js'), { recursive: true });

// `docs.html` uses only BUILD:VERSION; the changelog markers simply find no
// match there, which is what the index-scanning `fill` above is built to do.
for (const page of ['index.html', 'changelog.html', 'docs.html']) {
  let html = await readFile(join(SRC, page), 'utf8');
  html = fill(html, 'VERSION', releases[0]?.version ?? v);
  html = fill(html, 'LATEST_RELEASE', `\n${renderLatest(releases)}\n      `);
  html = fill(html, 'RELEASES', `\n${renderReleases(releases)}\n      `);
  await writeFile(join(OUT, page), html);
}

console.log(
  `build:site — web/dist ready · version ${releases[0]?.version ?? v} · ` +
    `${String(releases.length)} release(s)`,
);
