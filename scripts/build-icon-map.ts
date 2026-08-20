// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Brand icon bundle builder — ADD-001.
 *
 *     pnpm icons:build            rebuild the bundle and the map
 *     pnpm icons:build --report   report only, write nothing
 *
 * Reads the vendored sources under `handoffs/brand-icons/`, picks one square mark per
 * brand, optimises it, writes it to `public/icons/<key>.svg`, and emits the domain →
 * key map that Rust consults. **Adding a brand is a data change**: drop the SVG and its
 * manifest row into a source folder and run this. Removing a whole source folder is
 * also just a rerun — each one is discovered, and a missing folder is reported rather
 * than fatal.
 *
 * ## Why there is barely any name inference
 *
 * The obvious approach to "brand name → domain" is to guess: lowercase the title, strip
 * punctuation, append `.com`. That is how a password manager ends up showing Chase's
 * logo on someone's Chime account, and a wrong mark on a bank is worse than no mark at
 * all — it is an anti-signal, because it trains the user not to look.
 *
 * Both sources happen to carry a `url` field for nearly every brand: 1411 of 1411 in
 * gilbarbara, 5940 of 6511 in thesvg. So the mapping is *read*, not guessed —
 * `registrableDomain(entry.url)` through the real Public Suffix List. Titles and aliases
 * are used for exactly two things, neither of which can invent a domain:
 *
 *   1. **Collapsing the two sources into one key space.** `github`, `github-icon` and
 *      `GitHub` normalise to one entry, so a brand in both sources ships once.
 *   2. **Choosing between brands that claim the same domain.** Google publishes dozens
 *      of marks whose `url` is `google.com`; the one that wins is the brand whose slug
 *      equals the domain's own label, then the shortest slug, then gilbarbara over
 *      thesvg. When nothing matches the label the domain is left **unmapped** rather
 *      than assigned a guess.
 *
 * Anything inference cannot reach lives in {@link HOST_OVERRIDES} and
 * {@link DOMAIN_OVERRIDES} as explicit data, and the interesting cases are the ones
 * where eTLD+1 is the wrong granularity: `console.aws.amazon.com` must resolve to AWS,
 * not to Amazon, so host-suffix overrides are consulted before the domain map.
 *
 * ## What is deliberately excluded
 *
 * - **The `aws`, `azure` and `gcp` architecture collections.** thesvg licenses the AWS
 *   set CC BY-ND 2.0 — No Derivatives — which forbids the optimisation below, and cloud
 *   architecture glyphs are not brands a password manager has items for.
 * - **Any licence that forbids derivatives or commercial use, or that is absent.** ND,
 *   NC, `Unknown`, `Proprietary`, `TODO`, and thesvg's own "no express redistribution
 *   license supplied" marker. ADD-001 requires every bundled icon to trace to a
 *   documented licence.
 * - **`mono` and every wordmark variant.** A tile is a square; a wordmark is not, and a
 *   monochrome glyph is not the brand's mark.
 * - **Marks larger than {@link MAX_ICON_BYTES} after optimisation.** At the 32–56px the
 *   design draws, a mark needing more path data than that is an illustration. Where the
 *   other source has a smaller version of the same brand it is used instead, so this
 *   drops a file rather than a brand wherever it can.
 */

import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';
import { gzipSync } from 'node:zlib';
import psl from 'psl';
import { optimize } from 'svgo';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SOURCES = join(ROOT, 'handoffs', 'brand-icons');
const OUT_ICONS = join(ROOT, 'public', 'icons');
const OUT_MAP = join(ROOT, 'src-tauri', 'assets', 'icon-map.tsv');

/**
 * Ceiling for one optimised mark.
 *
 * 16 KB of path data is already far more than a 56px tile can show. Above it the file
 * is an illustration that happens to live in a logo repository.
 */
const MAX_ICON_BYTES = 16 * 1024;

/** thesvg collections that are cloud architecture glyph sets, not brands. */
const EXCLUDED_COLLECTIONS = new Set(['aws', 'azure', 'gcp']);

/**
 * Licences that do not grant what bundling needs: redistribution of an optimised copy
 * inside a commercial application.
 *
 * Matched against the manifest's free-text `license`, so it catches both SPDX ids and
 * thesvg's prose markers. Trademark and brand-use notices are **not** here: those are
 * about trademark rather than copyright, and ADD-001's stated position is that using a
 * company's mark to identify that company's own service inside a UI is nominative use.
 */
const FORBIDDEN_LICENCE =
  /\bND\b|NoDerivat|No[- ]?Derivat|\bNC\b|NonCommercial|non-commercial|\bUnknown\b|\bTODO\b|Proprietary|no express redistribution|CC[- ]?BY[- ]?SA[- ]?2\.5|CC[- ]?BY[- ]?SA[- ]?3\.0/i;

/*
 * The two share-alike versions in that pattern are excluded for a different reason from
 * everything else in it: licence *compatibility*, not fetchability or ambiguity.
 *
 * Trynta is AGPL-3.0-or-later. Creative Commons declared CC BY-SA **4.0** one-way
 * compatible with GPLv3 in 2015; 2.5 and 3.0 were never covered by that declaration, and
 * their share-alike term requires derivatives under CC BY-SA, which conflicts with the
 * AGPL. Six marks were affected — f-droid, gentoo, inkscape, jenkins, luanti, redmine —
 * and those domains now fall through to a generated shape.
 *
 * Matched by exact version rather than by the `CC-BY-SA` family on purpose. 4.0 *is*
 * compatible and its twelve marks still ship; a pattern that caught the whole family
 * would drop them for nothing. GPL, AGPL, LGPL and MPL marks are compatible with
 * AGPL-3.0 as well and are likewise untouched. The incompatibility is specific to two old
 * Creative Commons versions, not to copyleft in general.
 */

/**
 * Host suffixes that must not reduce to their registrable domain.
 *
 * Consulted before the domain map, longest suffix first. Every entry is a case where
 * eTLD+1 is the wrong granularity because one company runs distinct products on
 * subdomains of one domain.
 */
const HOST_OVERRIDES: Record<string, string> = {
  'aws.amazon.com': 'aws',
  'console.aws.amazon.com': 'aws',
  'signin.aws.amazon.com': 'aws',
  'portal.azure.com': 'microsoftazure',
  'console.cloud.google.com': 'googlecloud',
  'cloud.google.com': 'googlecloud',
  'mail.google.com': 'gmail',
  'drive.google.com': 'googledrive',
  'docs.google.com': 'googledocs',
  'photos.google.com': 'googlephotos',
  'play.google.com': 'googleplay',
  'meet.google.com': 'googlemeet',
  'calendar.google.com': 'googlecalendar',
  'analytics.google.com': 'googleanalytics',
  'teams.microsoft.com': 'microsoftteams',
  'outlook.office.com': 'microsoftoutlook',
  'outlook.live.com': 'microsoftoutlook',
  'onedrive.live.com': 'microsoftonedrive',
  'music.apple.com': 'applemusic',
  'icloud.com': 'icloud',
  'developer.apple.com': 'apple',
  'business.facebook.com': 'meta',
  'developers.facebook.com': 'meta',
};

/**
 * Registrable domains whose winning brand inference cannot get right, or that need a
 * second domain pointing at an existing brand.
 *
 * The right-hand side is a *source key* — a slug from either manifest — not a file
 * name. A key that no source provides is reported rather than written, so a stale
 * override shows up instead of producing a dangling map row.
 */
const DOMAIN_OVERRIDES: Record<string, string> = {
  // The obvious ones inference gets wrong because the slug does not match the label.
  'google.com': 'google',
  'youtube.com': 'youtube',
  'stripe.com': 'stripe',
  'amazon.com': 'amazon',
  'github.com': 'github',
  'gitlab.com': 'gitlab',
  'microsoft.com': 'microsoft',
  'live.com': 'microsoft',
  'office.com': 'microsoft-office',
  'npmjs.com': 'npm',
  'bitbucket.org': 'bitbucket',
  'expressjs.com': 'express',
  'sourceforge.net': 'sourceforge',
  'pypi.org': 'pypi',
  'medium.com': 'medium',
  'substack.com': 'substack',
  'wordpress.com': 'wordpress',
  'behance.net': 'behance',
  'dribbble.com': 'dribbble',
  'producthunt.com': 'producthunt',
  'web.dev': 'webdev',
  'd3js.org': 'd3',
  'emberjs.com': 'ember',
  'daily.dev': 'dailydev',
  'bunny.net': 'bunnynet',
  'trufflesuite.com': 'truffle',
  'blitzjs.com': 'blitz',
  'apple.com': 'apple',
  'facebook.com': 'facebook',
  'instagram.com': 'instagram',
  'whatsapp.com': 'whatsapp',
  'x.com': 'x',
  'twitter.com': 'x',
  'linkedin.com': 'linkedin',
  'netflix.com': 'netflix',
  'spotify.com': 'spotify',
  'paypal.com': 'paypal',
  'dropbox.com': 'dropbox',
  'slack.com': 'slack',
  'discord.com': 'discord',
  'reddit.com': 'reddit',
  'twitch.tv': 'twitch',
  'steampowered.com': 'steam',
  'ebay.com': 'ebay',
  'booking.com': 'bookingdotcom',
  'airbnb.com': 'airbnb',
  'uber.com': 'uber',
  'figma.com': 'figma',
  'notion.so': 'notion',
  'linear.app': 'linear',
  'atlassian.com': 'atlassian',
  'cloudflare.com': 'cloudflare',
  'digitalocean.com': 'digitalocean',
  'heroku.com': 'heroku',
  'vercel.com': 'vercel',
  'netlify.com': 'netlify',
  'openai.com': 'openai',
  'anthropic.com': 'anthropic',
  'claude.ai': 'anthropic',
  'adobe.com': 'adobe',
  'zoom.us': 'zoom',
  'nvidia.com': 'nvidia',
  'wise.com': 'wise',
  'revolut.com': 'revolut',
  'monzo.com': 'monzo',
  'coinbase.com': 'coinbase',
  'binance.com': 'binance',
  'protonmail.com': 'protonmail',
  'proton.me': 'proton',
  'bitwarden.com': 'bitwarden',
  '1password.com': '1password',
};

/**
 * Domains that host other people's things, and therefore say nothing about who owns
 * them.
 *
 * Both manifests contain brands whose `url` points at a Wikipedia article, a GitHub
 * repository or an app-store listing rather than at the brand's own site — 32 different
 * brands claim `wikipedia.org` alone. Inferring from those would put a random project's
 * mark on the tile for a user's Wikipedia account.
 *
 * They are unreachable by inference and can only be set by an override, which is where
 * every one that matters already is.
 */
const AGGREGATOR_DOMAINS = new Set([
  'wikipedia.org',
  'wikimedia.org',
  'github.com',
  'github.io',
  'gitlab.com',
  'sourceforge.net',
  'bitbucket.org',
  'npmjs.com',
  'pypi.org',
  'crates.io',
  'rubygems.org',
  'packagist.org',
  'nuget.org',
  'hex.pm',
  'maven.org',
  'apache.org',
  'gnu.org',
  'fsf.org',
  'readthedocs.io',
  'readthedocs.org',
  'gitbook.io',
  'medium.com',
  'substack.com',
  'notion.so',
  'notion.site',
  'apps.apple.com',
  'play.google.com',
  'chrome.google.com',
  'addons.mozilla.org',
  'microsoft.com',
  'docs.google.com',
  'drive.google.com',
  'sites.google.com',
  'netlify.app',
  'vercel.app',
  'herokuapp.com',
  'pages.dev',
  'web.app',
  'firebaseapp.com',
  'glitch.me',
  'repl.it',
  'replit.app',
  'surge.sh',
  'cloudfront.net',
  'amazonaws.com',
  'blogspot.com',
  'wordpress.com',
  'youtube.com',
  'x.com',
  'twitter.com',
  'facebook.com',
  'linkedin.com',
  'discord.gg',
  'discord.com',
  'slack.com',
  'figma.com',
  'behance.net',
  'dribbble.com',
  'producthunt.com',
  'crunchbase.com',
  'linktr.ee',
]);

/**
 * Multi-domain brands: additional registrable domains that resolve to a domain already
 * in the map. ADD-001's example is `amazon.co.uk` and `amazon.com` being one icon.
 */
const DOMAIN_ALIASES: Record<string, string> = {
  'amazon.co.uk': 'amazon.com',
  'amazon.de': 'amazon.com',
  'amazon.fr': 'amazon.com',
  'amazon.es': 'amazon.com',
  'amazon.it': 'amazon.com',
  'amazon.ca': 'amazon.com',
  'amazon.com.au': 'amazon.com',
  'amazon.co.jp': 'amazon.com',
  'amazon.in': 'amazon.com',
  'google.co.uk': 'google.com',
  'google.de': 'google.com',
  'google.fr': 'google.com',
  'google.ca': 'google.com',
  'google.com.au': 'google.com',
  'ebay.co.uk': 'ebay.com',
  'ebay.de': 'ebay.com',
  'paypal.me': 'paypal.com',
  'youtu.be': 'youtube.com',
  'github.io': 'github.com',
  'microsoftonline.com': 'microsoft.com',
  'apple.co.uk': 'apple.com',
};

/**
 * The `card:<brand>` namespace, per ADD-001.
 *
 * Card brands come from the card number, not a domain, so they cannot be reached by any
 * URL and need their own keys. The right-hand side is a source key, resolved the same
 * way an override is.
 */
const CARD_BRANDS: Record<string, string> = {
  visa: 'visa',
  mastercard: 'mastercard',
  amex: 'americanexpress',
  discover: 'discover',
  jcb: 'jcb',
  unionpay: 'unionpay',
  dinersclub: 'dinersclub',
  maestro: 'maestro',
};

// ── source loading ───────────────────────────────────────────────────────────

/** One candidate mark, before dedupe and before the size ceiling is applied. */
interface Candidate {
  /** Normalised key, shared across sources so duplicates collide. */
  key: string;
  /** The source manifest's own slug, for reporting and for override lookups. */
  slug: string;
  /** Display name, for reports. */
  title: string;
  /** Which folder it came from. */
  source: 'gilbarbara' | 'thesvg';
  /** `color` for a full-colour mark; `light`/`dark` for a theme-specific pair. */
  variant: 'color' | 'light' | 'dark';
  /** Absolute path on disk. */
  file: string;
  /** SPDX id or the manifest's free-text licence. */
  licence: string;
  /** Documented brand hex, when the source has one. */
  brandHex: string | null;
  /** Registrable domain from the manifest's own `url`, when it has one. */
  domain: string | null;
  /** Alternate names, used only for key collapsing. */
  aliases: string[];
}

/** Collapse a brand name or slug to one key space. */
function normaliseKey(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/\.svg$/, '')
    .replace(/-(icon|logo|mark|symbol|glyph|color|colour|original)$/, '')
    .replace(/[^a-z0-9]/g, '');
}

/** eTLD+1 of a URL or bare host, lowercased. `null` for anything without one. */
export function registrableDomain(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed === '') return null;

  let host: string;
  try {
    const parsed = new URL(
      /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`,
    );
    host = parsed.hostname;
  } catch {
    return null;
  }

  // An IP literal has no registrable domain.
  if (/^\d+\.\d+\.\d+\.\d+$/.test(host) || host.startsWith('[')) return null;

  const parsed = psl.parse(host.toLowerCase());
  if ('error' in parsed || parsed.domain === null) return null;
  return parsed.domain;
}

/** Read gilbarbara's index. Prefers the square `-icon` file over the logotype. */
function loadGilbarbara(): Candidate[] {
  const dir = join(SOURCES, 'gilbarbara-logos');
  const index = join(dir, 'logos.json');
  if (!existsSync(index)) return [];

  const entries = JSON.parse(readFileSync(index, 'utf8')) as {
    name: string;
    shortname: string;
    url: string;
    files: string[];
  }[];

  const out: Candidate[] = [];
  for (const entry of entries) {
    // `-icon` is the square mark; the bare file is usually a horizontal logotype, which
    // is the wrong shape for a tile.
    const square = entry.files.find((f) => f.endsWith('-icon.svg'));
    const plain = entry.files.find((f) => f === `${entry.shortname}.svg`) ?? entry.files[0];
    const file = square ?? plain;
    if (file === undefined) continue;
    const path = join(dir, 'logos', file);
    if (!existsSync(path)) continue;

    out.push({
      key: normaliseKey(entry.shortname),
      slug: entry.shortname,
      title: entry.name,
      source: 'gilbarbara',
      variant: 'color',
      file: path,
      // The repository is CC0 as a whole; per-brand trademarks remain their owners'.
      licence: 'CC0-1.0',
      brandHex: null,
      domain: registrableDomain(entry.url),
      aliases: [entry.name],
    });
  }
  return out;
}

/** Read thesvg's manifest. Takes `default`, plus `light`/`dark` where both exist. */
function loadTheSvg(): {
  candidates: Candidate[];
  excludedCollection: number;
  excludedLicence: number;
} {
  const dir = join(SOURCES, 'thesvg');
  const index = join(dir, 'src', 'data', 'icons.json');
  if (!existsSync(index)) return { candidates: [], excludedCollection: 0, excludedLicence: 0 };

  const entries = JSON.parse(readFileSync(index, 'utf8')) as {
    slug: string;
    title: string;
    aliases?: string[];
    hex?: string;
    categories?: string[];
    variants?: Record<string, string>;
    license?: string;
    url?: string;
    collection?: string;
  }[];

  const candidates: Candidate[] = [];
  let excludedCollection = 0;
  let excludedLicence = 0;

  for (const entry of entries) {
    if (entry.collection !== undefined && EXCLUDED_COLLECTIONS.has(entry.collection)) {
      excludedCollection += 1;
      continue;
    }
    const licence = entry.license ?? 'Unknown';
    if (FORBIDDEN_LICENCE.test(licence)) {
      excludedLicence += 1;
      continue;
    }

    const variants = entry.variants ?? {};
    const base = {
      key: normaliseKey(entry.slug),
      slug: entry.slug,
      title: entry.title,
      source: 'thesvg' as const,
      licence,
      brandHex: entry.hex === undefined ? null : `#${entry.hex.replace(/^#/, '')}`,
      domain: entry.url === undefined ? null : registrableDomain(entry.url),
      aliases: [entry.title, ...(entry.aliases ?? [])],
    };

    // A light/dark pair is only meaningful as a pair: one alone would leave one theme
    // rendering a mark drawn for the other. `mono` and every wordmark are ignored —
    // a wordmark is not square and a monochrome glyph is not the brand's mark.
    const hasPair = typeof variants.light === 'string' && typeof variants.dark === 'string';
    if (hasPair) {
      candidates.push({
        ...base,
        variant: 'light',
        file: join(dir, 'public', variants.light ?? ''),
      });
      candidates.push({ ...base, variant: 'dark', file: join(dir, 'public', variants.dark ?? '') });
    }

    const primary = variants.default ?? variants.color;
    if (typeof primary === 'string') {
      candidates.push({ ...base, variant: 'color', file: join(dir, 'public', primary) });
    }
  }

  return {
    candidates: candidates.filter((c) => existsSync(c.file)),
    excludedCollection,
    excludedLicence,
  };
}

// ── optimisation ─────────────────────────────────────────────────────────────

/**
 * Optimise and normalise to a common square viewBox.
 *
 * `removeViewBox` is off: the viewBox is the only thing that lets a mark drawn on a
 * 48-unit grid and one drawn on a 1024-unit grid render at the same size in the same
 * 32px tile. `removeDimensions` drops the fixed `width`/`height` so CSS decides.
 *
 * Never recolours. `preset-default` does not touch fill values, and no plugin here is
 * given permission to — ADD-001: *"Never recolour a tier-1 mark to fit a theme."*
 */
function optimiseSvg(source: string): string | null {
  let data: string;
  try {
    data = optimize(source, {
      multipass: true,
      plugins: [
        { name: 'preset-default', params: { overrides: { removeViewBox: false } } },
        'removeDimensions',
        { name: 'removeAttrs', params: { attrs: '(data-name|class)' } },
      ],
    }).data;
  } catch {
    return null;
  }

  // A mark with no viewBox cannot be scaled predictably. Rather than invent one, skip
  // it and report it — a guessed viewBox crops somebody's logo.
  if (!/viewBox\s*=/.test(data)) return null;
  // Belt for the sanitiser's rules: the bundle must never carry script or a remote ref.
  if (
    /<script|<foreignObject|xlink:href\s*=\s*["']https?:|href\s*=\s*["']https?:|url\(\s*["']?https?:/i.test(
      data,
    )
  ) {
    return null;
  }
  return data;
}

// ── build ────────────────────────────────────────────────────────────────────

/** One row of the emitted map. */
/**
 * The darkest surface a brand mark is ever drawn on, from `tokens.css`.
 *
 * Duplicated here as a literal rather than read from the token layer, and that is
 * the lesser evil: this script runs at build time with no CSS engine, and the
 * alternative is emitting no flag and letting the app guess at runtime. It is the
 * value of `--surface-raised` in the dark theme; if that token moves, this moves.
 */
const DARK_TILE_SURFACE: [number, number, number] = [0x10, 0x14, 0x20];

/** WCAG relative luminance. */
function luminance([r, g, b]: [number, number, number]): number {
  const channel = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** WCAG contrast ratio between two colours. */
function contrast(a: [number, number, number], b: [number, number, number]): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Every explicit colour in an SVG, as RGB triples. */
function inksOf(svg: string): [number, number, number][] {
  const out: [number, number, number][] = [];
  for (const m of svg.matchAll(/#([0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b/g)) {
    let hex = m[1];
    if (hex.length === 3) {
      hex = hex
        .split('')
        .map((c) => c + c)
        .join('');
    }
    out.push([
      Number.parseInt(hex.slice(0, 2), 16),
      Number.parseInt(hex.slice(2, 4), 16),
      Number.parseInt(hex.slice(4, 6), 16),
    ]);
  }
  // A mark with no declared colour inherits, and inherits dark on a dark surface.
  if (/fill="(black|currentColor)"/i.test(svg) || !/fill=|stroke=/i.test(svg)) {
    out.push([0, 0, 0]);
  }
  return out;
}

/**
 * Whether a mark needs a light chip behind it in the dark theme.
 *
 * A brand mark is drawn for the background its owner publishes it on, which is
 * usually white — so a black wordmark on a near-black tile is invisible, and 20% of
 * the shipped set is in that position. ADD-001 forbids recolouring the mark, and it
 * is right to: a recoloured brand mark is the wrong mark. What is left is changing
 * what sits *behind* it, and doing that only where it is needed, which is what this
 * flag is for. Marks with their own bright colour — Spotify, Netflix — get nothing,
 * because a white square behind them would be worse than the problem.
 *
 * The test is whether *any* ink in the mark reaches 3:1 against the dark tile. If
 * even one part of it does, the mark reads.
 */
function needsLightChip(svg: string): boolean {
  const inks = inksOf(svg);
  if (inks.length === 0) return false;
  return !inks.some((ink) => contrast(ink, DARK_TILE_SURFACE) >= 3);
}

interface MapRow {
  domain: string;
  key: string;
  source: string;
  variant: string;
  licence: string;
  brandHex: string;
  /** Whether the mark needs a light chip behind it in the dark theme. */
  darkInk: boolean;
}

function main(): void {
  const reportOnly = process.argv.includes('--report');

  const gilbarbara = loadGilbarbara();
  const svg = loadTheSvg();

  console.log('build-icon-map — sources');
  console.log(
    `  gilbarbara-logos  ${gilbarbara.length === 0 ? 'MISSING' : `${String(gilbarbara.length)} brands`}`,
  );
  console.log(
    `  thesvg            ${svg.candidates.length === 0 ? 'MISSING' : `${String(svg.candidates.length)} files`}` +
      `  (excluded: ${String(svg.excludedCollection)} architecture, ${String(svg.excludedLicence)} licence)`,
  );

  // ── one entry per brand, gilbarbara winning an overlap ──────────────────────
  const byKey = new Map<string, Candidate[]>();
  for (const c of [...gilbarbara, ...svg.candidates]) {
    const list = byKey.get(c.key);
    if (list === undefined) byKey.set(c.key, [c]);
    else list.push(c);
  }

  const overlap: string[] = [];
  for (const [key, list] of byKey) {
    const sources = new Set(list.map((c) => c.source));
    if (sources.size > 1) overlap.push(key);
  }

  // ── optimise and apply the ceiling ─────────────────────────────────────────
  //
  // The output directory is emptied here rather than at the end: a stale icon left from a
  // previous run with a different source set would ship, and nothing downstream would
  // notice, because `<img src>` resolving is a runtime concern.
  if (!reportOnly) {
    rmSync(OUT_ICONS, { recursive: true, force: true });
    mkdirSync(OUT_ICONS, { recursive: true });
  }

  /**
   * key → the optimised bytes, held rather than written.
   *
   * Nothing reaches disk until the map exists, because the map is what decides which
   * icons are *reachable*: `resolve()` looks up a host or a registrable domain and
   * nothing else — there is no lookup by title — so a brand with no domain row can never
   * appear in the product. Writing it would be installer weight that renders zero times.
   * The whole set is ~9 MB of text; holding it costs nothing next to being able to make
   * that decision correctly.
   */
  const written = new Map<
    string,
    { variants: Map<string, string>; chosen: Candidate; darkInk: boolean }
  >();
  const oversized: { key: string; bytes: number }[] = [];
  const unusable: string[] = [];

  for (const [key, list] of byKey) {
    // gilbarbara first: hand-optimised, and its square `-icon` files are drawn as marks
    // rather than reduced from a logotype.
    const ordered = [...list].sort((a, b) => {
      if (a.source !== b.source) return a.source === 'gilbarbara' ? -1 : 1;
      const rank = (v: string) => (v === 'color' ? 0 : 1);
      return rank(a.variant) - rank(b.variant);
    });

    let chosen: Candidate | null = null;
    const variants = new Map<string, string>();

    for (const candidate of ordered) {
      // Once a source has supplied the colour mark, only that same source's light/dark
      // pair may join it — mixing a gilbarbara colour mark with thesvg's dark variant
      // would put two different drawings of one brand in one tile.
      if (chosen !== null && candidate.source !== chosen.source) continue;
      if (variants.has(candidate.variant)) continue;

      const raw = readFileSync(candidate.file, 'utf8');
      const out = optimiseSvg(raw);
      if (out === null) {
        if (chosen === null) unusable.push(`${key} (${candidate.source})`);
        continue;
      }
      const size = Buffer.byteLength(out);
      if (size > MAX_ICON_BYTES) {
        if (chosen === null) oversized.push({ key, bytes: size });
        continue;
      }

      variants.set(candidate.variant, out);
      chosen ??= candidate;
    }

    // A light/dark pair with no colour mark is not usable: nothing to show while the
    // theme is resolving, and `Icon` has no third state.
    if (chosen !== null && !variants.has('color')) {
      variants.clear();
      chosen = null;
    }
    if (chosen !== null) {
      const colour = variants.get('color') ?? '';
      // Only the colour mark is tested. A themed pair is already the brand’s own
      // answer to this question, and second-guessing it would undo their work.
      const darkInk = !variants.has('light') && needsLightChip(colour);
      written.set(key, { variants, chosen, darkInk });
    }
  }

  // ── domain → key ───────────────────────────────────────────────────────────
  /** Every brand claiming a given domain, so a contest can be resolved. */
  const claims = new Map<string, Candidate[]>();
  for (const [key, entry] of written) {
    const domain = entry.chosen.domain;
    if (domain === null) continue;
    // A brand that lists a hosting or reference site as its home tells us nothing about
    // who owns that site. Only an override may map one.
    if (AGGREGATOR_DOMAINS.has(domain)) continue;
    void key;
    const list = claims.get(domain);
    if (list === undefined) claims.set(domain, [entry.chosen]);
    else list.push(entry.chosen);
  }

  const rows = new Map<string, MapRow>();
  const contested: string[] = [];
  const unresolvedContest: string[] = [];

  const rowFor = (domain: string, c: Candidate): MapRow => ({
    domain,
    key: c.key,
    source: c.source,
    variant: written.get(c.key)?.variants.has('light') === true ? 'color+theme' : 'color',
    licence: c.licence,
    brandHex: c.brandHex ?? '',
    darkInk: written.get(c.key)?.darkInk === true,
  });

  for (const [domain, list] of claims) {
    if (list.length === 1 && list[0] !== undefined) {
      rows.set(domain, rowFor(domain, list[0]));
      continue;
    }
    contested.push(`${domain} (${String(list.length)})`);

    // The domain's own label decides. `google.com` → label `google` → the brand whose
    // key is exactly `google`. No label match means no row: a guess here is the exact
    // failure mode this script is written to avoid.
    const label = normaliseKey(domain.split('.')[0] ?? '');
    const exact = list.filter((c) => c.key === label);
    const pick =
      exact.length > 0
        ? [...exact].sort((a, b) =>
            a.source === b.source ? 0 : a.source === 'gilbarbara' ? -1 : 1,
          )[0]
        : undefined;
    if (pick === undefined) {
      unresolvedContest.push(`${domain} → ${list.map((c) => c.key).join(', ')}`);
      continue;
    }
    rows.set(domain, rowFor(domain, pick));
  }

  // Overrides win over inference, and a stale one is reported rather than written.
  const staleOverrides: string[] = [];
  for (const [domain, wanted] of Object.entries(DOMAIN_OVERRIDES)) {
    const key = normaliseKey(wanted);
    const entry = written.get(key);
    if (entry === undefined) {
      staleOverrides.push(`${domain} → ${wanted} (no such icon)`);
      continue;
    }
    rows.set(domain, rowFor(domain, entry.chosen));
  }

  // Aliases copy an existing row, so a multi-domain brand ships one file.
  const staleAliases: string[] = [];
  for (const [alias, target] of Object.entries(DOMAIN_ALIASES)) {
    const row = rows.get(target);
    if (row === undefined) {
      staleAliases.push(`${alias} → ${target} (target unmapped)`);
      continue;
    }
    rows.set(alias, { ...row, domain: alias });
  }

  // Host overrides are a separate namespace: they are consulted before the reduction to
  // eTLD+1, which is the only way `console.aws.amazon.com` can avoid becoming Amazon.
  const hostRows = new Map<string, MapRow>();
  for (const [host, wanted] of Object.entries(HOST_OVERRIDES)) {
    const key = normaliseKey(wanted);
    const entry = written.get(key);
    if (entry === undefined) {
      staleOverrides.push(`${host} → ${wanted} (no such icon)`);
      continue;
    }
    hostRows.set(host, rowFor(host, entry.chosen));
  }

  const cardRows = new Map<string, MapRow>();
  const missingCards: string[] = [];
  for (const [brand, wanted] of Object.entries(CARD_BRANDS)) {
    const key = normaliseKey(wanted);
    const entry = written.get(key);
    if (entry === undefined) {
      missingCards.push(`${brand} → ${wanted}`);
      continue;
    }
    cardRows.set(`card:${brand}`, rowFor(`card:${brand}`, entry.chosen));
  }

  // ── emit ───────────────────────────────────────────────────────────────────
  if (!reportOnly) {
    const lines: string[] = [
      '# Generated by scripts/build-icon-map.ts — do not edit.',
      '# kind\tmatch\tkey\tsource\tvariant\tlicence\tbrand_hex\tdark_ink',
    ];
    const emit = (kind: string, map: Map<string, MapRow>) => {
      for (const key of [...map.keys()].sort()) {
        const r = map.get(key);
        if (r === undefined) continue;
        lines.push(
          `${kind}\t${key}\t${r.key}\t${r.source}\t${r.variant}\t${r.licence}\t${r.brandHex}\t${r.darkInk ? '1' : ''}`,
        );
      }
    };
    emit('host', hostRows);
    emit('domain', rows);
    emit('card', cardRows);
    mkdirSync(dirname(OUT_MAP), { recursive: true });
    writeFileSync(OUT_MAP, `${lines.join('\n')}\n`);
  }

  // ── write the reachable icons ──────────────────────────────────────────────
  //
  // `mappedKeys` is the reachability set: exactly the keys some host, domain or card row
  // points at. Everything else is a brand the resolver has no route to.
  const mappedKeys = new Set(
    [...rows.values(), ...hostRows.values(), ...cardRows.values()].map((r) => r.key),
  );
  const unmappedIcons = [...written.keys()].filter((k) => !mappedKeys.has(k));

  let bytes = 0;
  let gzBytes = 0;
  let files = 0;
  let unreachableBytes = 0;

  for (const [key, entry] of written) {
    const reachable = mappedKeys.has(key);
    for (const [variant, out] of entry.variants) {
      const size = Buffer.byteLength(out);
      if (!reachable) {
        unreachableBytes += size;
        continue;
      }
      bytes += size;
      gzBytes += gzipSync(Buffer.from(out), { level: 9 }).length;
      files += 1;
      if (reportOnly) continue;
      const name = variant === 'color' ? key : `${key}-${variant}`;
      writeFileSync(join(OUT_ICONS, `${name}.svg`), out);
    }
  }

  // ── the four reports ───────────────────────────────────────────────────────

  const mb = (x: number) => `${(x / 1048576).toFixed(2)} MB`;

  console.log('');
  console.log(`brands with a usable mark   ${String(written.size)}`);
  console.log(`  domain rows               ${String(rows.size)}`);
  console.log(`  host-override rows        ${String(hostRows.size)}`);
  console.log(`  card rows                 ${String(cardRows.size)}`);
  console.log('');
  console.log('── 1. bundled icons with no domain mapped ──────────────────────────');
  console.log(
    `  ${String(unmappedIcons.length)} of ${String(written.size)} brands carry no usable URL in either manifest`,
  );
  console.log(`  NOT SHIPPED — unreachable without a domain row, ${mb(unreachableBytes)} saved`);
  console.log(`  sample: ${unmappedIcons.slice(0, 14).join(', ')}`);
  console.log('');
  console.log('── 2. vault domains with no icon ───────────────────────────────────');
  const vault = vaultDomains();
  const missing = vault.filter((d) => !rows.has(d) && !hostRows.has(d));
  console.log(
    `  probe list of ${String(vault.length)} common services: ${String(missing.length)} unmapped`,
  );
  if (missing.length > 0) console.log(`  ${missing.join(', ')}`);
  console.log('');
  console.log('── 3. brands present in both sources ───────────────────────────────');
  console.log(
    `  ${String(overlap.length)} overlap; gilbarbara wins each (hand-optimised, square marks)`,
  );
  console.log(`  sample: ${overlap.slice(0, 14).join(', ')}`);
  console.log('');
  console.log('── 4. bundle size against the 20 MB installer budget ───────────────');
  console.log(
    `  shipped ${String(mappedKeys.size)} brands in ${String(files)} files, ` +
      `on disk ${mb(bytes)}, compressed ${mb(gzBytes)}`,
  );
  console.log(
    `  dropped over the ${String(MAX_ICON_BYTES / 1024)} KB ceiling: ${String(oversized.length)}`,
  );
  if (oversized.length > 0) {
    console.log(
      `    ${oversized
        .sort((a, b) => b.bytes - a.bytes)
        .slice(0, 10)
        .map((o) => `${o.key}=${String(Math.round(o.bytes / 1024))}k`)
        .join(' ')}`,
    );
  }
  console.log(`  unusable (no viewBox, or script/remote ref): ${String(unusable.length)}`);
  if (contested.length > 0) {
    console.log('');
    console.log(`contested domains (>1 brand claiming one domain): ${String(contested.length)}`);
    console.log(`  left unmapped rather than guessed: ${String(unresolvedContest.length)}`);
    if (unresolvedContest.length > 0)
      console.log(`  ${unresolvedContest.slice(0, 8).join('  |  ')}`);
  }
  if (staleOverrides.length > 0) {
    console.log('');
    console.log(
      `STALE OVERRIDES — pointing at icons no source provides: ${String(staleOverrides.length)}`,
    );
    for (const s of staleOverrides) console.log(`  ${s}`);
  }
  if (staleAliases.length > 0) {
    console.log(`STALE ALIASES: ${staleAliases.join(', ')}`);
  }
  if (missingCards.length > 0) {
    console.log(`CARD BRANDS WITH NO ICON: ${missingCards.join(', ')}`);
  }
  console.log('');
  console.log(reportOnly ? 'report only — nothing written' : `wrote ${OUT_ICONS} and ${OUT_MAP}`);
}

/**
 * A probe list for report 2.
 *
 * Not the user's actual vault — nothing here reads one, and a build script that opened a
 * vault would be a considerably worse idea than a missing icon. These are the services a
 * password manager is most likely to hold, so a gap in this list is a gap that matters.
 */
function vaultDomains(): string[] {
  return [
    'google.com',
    'youtube.com',
    'gmail.com',
    'microsoft.com',
    'live.com',
    'office.com',
    'apple.com',
    'icloud.com',
    'amazon.com',
    'amazon.co.uk',
    'ebay.com',
    'paypal.com',
    'netflix.com',
    'spotify.com',
    'disneyplus.com',
    'hulu.com',
    'twitch.tv',
    'facebook.com',
    'instagram.com',
    'whatsapp.com',
    'x.com',
    'twitter.com',
    'linkedin.com',
    'reddit.com',
    'discord.com',
    'slack.com',
    'zoom.us',
    'github.com',
    'gitlab.com',
    'bitbucket.org',
    'stripe.com',
    'cloudflare.com',
    'digitalocean.com',
    'heroku.com',
    'vercel.com',
    'netlify.com',
    'npmjs.com',
    'dropbox.com',
    'notion.so',
    'figma.com',
    'linear.app',
    'atlassian.com',
    'openai.com',
    'anthropic.com',
    'nvidia.com',
    'adobe.com',
    'steampowered.com',
    'airbnb.com',
    'booking.com',
    'uber.com',
    'wise.com',
    'revolut.com',
    'monzo.com',
    'coinbase.com',
    'binance.com',
    'protonmail.com',
    'proton.me',
    'bitwarden.com',
    '1password.com',
    'aws.amazon.com',
    'portal.azure.com',
    'cloud.google.com',
  ];
}

main();
