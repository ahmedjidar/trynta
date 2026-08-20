# Third-party notices

Every asset bundled with Trynta, its source and its licence. ADD-001 requires that each bundled
icon traces to a documented source here; this file also covers the wordlist and the two-factor
directory, which carry their own terms.

Software dependencies are covered separately by `cargo deny check licenses` and the allow-list in
`deny.toml`; this file is for **data and assets** shipped inside the binary.

> Status: **three sections are live** — the Mozilla CA root store, compiled in since run 2, and the
> Manrope typeface and brand icons, vendored in run 3. Everything else is a placeholder for an asset that has not
> been vendored, and **an asset may not ship before its licence line is filled in.** ADD-001 and
> SPEC-V1 §7.4 both make that a precondition, not a follow-up. A section headed "run 3" with a
> blank licence field is not bundled; check the dependency or the assets directory rather than
> trusting the heading.

---

## Brand icons — run 3, live

Service logos are the trademarks of their respective owners and are used solely to identify those
services within the interface. Their presence implies no affiliation with or endorsement by those
companies. Icons are bundled with the application and **never fetched at runtime**, so using Trynta
does not disclose which services you have accounts with — the whole reason ADD-001 removed the
favicon layer.

### Sources

| Source               | Upstream                                              | Repository licence | Bundled from                                                          |
| -------------------- | ----------------------------------------------------- | ------------------ | --------------------------------------------------------------------- |
| **gilbarbara/logos** | https://github.com/gilbarbara/logos                   | CC0-1.0            | `logos/*.svg`, indexed by `logos.json`                                |
| **thesvg.org**       | https://thesvg.org — https://github.com/thesvg/thesvg | MIT (the codebase) | `public/icons/<slug>/<variant>.svg`, indexed by `src/data/icons.json` |

The two sets overlap on 908 brands. gilbarbara wins every overlap: its marks are hand-optimised and
drawn square, which is the shape an identity tile needs.

thesvg's MIT licence covers its codebase, not the marks. Each icon carries its own `license` field in
the manifest, and that value is what the map records per entry and what the table below aggregates.
Where a contributor recorded a repository licence (GPL, AGPL, MPL) rather than the mark's own terms,
that is the value as published upstream; it is reproduced faithfully rather than reinterpreted here.

### Licences of what actually ships

Aggregated from `src-tauri/assets/icon-map.tsv`, which is the per-icon record: every row carries
`source`, `variant`, `licence` and `brand_hex` for one match. That file is generated, committed, and
compiled into the binary, so the attribution for any single icon is one `grep` away and stays correct
when the sources change. Reproducing 3,778 rows here would be a copy that goes stale.

| Licence                    | Icons |
| -------------------------- | ----: |
| CC0-1.0                    | 3,162 |
| MIT                        |   335 |
| Fair use / nominative use  |    60 |
| Brand-use grant            |    55 |
| Apache-2.0                 |    53 |
| Trademark (identification) |    45 |
| GPL / AGPL                 |    19 |
| CC-BY-SA (2.5 / 3.0 / 4.0) |    18 |
| Custom, per-brand grant    |    14 |
| CC-BY (3.0 / 4.0)          |     7 |
| MPL-2.0                    |     3 |
| BSD-3-Clause               |     3 |
| Unlicense                  |     2 |
| LGPL-3.0                   |     1 |
| Public domain              |     1 |

Every copyleft and share-alike licence is broken out on its own row rather than folded into a
grouped one. An earlier version of this table carried `Other permissive (MPL, BSD, Unlicense)` and
a `Custom, per-brand grant` bucket that between them absorbed the single LGPL-3.0 mark and all
three MPL-2.0 marks — both are copyleft, neither is permissive, and a notices file that hides that
in a bucket labelled _permissive_ is worse than one that omits it. The rows sum to 3,778.

Two things worth being precise about rather than papering over:

- **Share-alike and copyleft marks are shipped as separate, unmodified-in-substance files.** The
  build strips metadata, editor cruft and comments and normalises the viewBox; it never redraws,
  recolours or merges a mark. Each file remains individually identifiable and individually licensed —
  the application does not relicense them. That is the standard position for bundled assets, and it
  is stated here as a position rather than as settled law: **confirm it before 1.0**, and drop the
  41 share-alike and copyleft entries (18 CC-BY-SA, 19 GPL/AGPL, 3 MPL-2.0, 1 LGPL-3.0) if the
  answer is unclear. `MANIFEST.md` carries this as an open item.
- **A trademark is not a licence.** Every mark here remains its owner's, whatever the file's licence
  says. Trynta uses them nominatively, to name a service the user already has an account with.

### Deliberately excluded

**AWS, Azure and Google Cloud architecture icon sets are excluded in full — 1,579 files.** The AWS
Architecture Icons are published under **CC BY-ND 2.0**. The ND term forbids distributing a modified
version, and this build pipeline modifies every icon it ships: SVGO optimisation and viewBox
normalisation are both derivative works. Shipping them unmodified to dodge that would mean a second
pipeline and a second set of rendering assumptions for a category of icon nobody stores a password
for. Azure and GCP architecture sets go with them: same shape of restriction, same reasoning, and
excluding a collection wholesale is auditable in a way a per-file judgement is not.

A further **95 files** were excluded because their recorded licence is ND, NC, `Unknown`, `TODO`, or
proprietary. The rule is in `scripts/build-icon-map.ts` as `FORBIDDEN_LICENCE`, and it fails closed:
an unrecognised licence string is excluded, not shipped.

### What ships, and what was cut

`pnpm icons:build` reports these numbers on every run; they are reproduced here as of the run that
produced the committed map.

|                                                         |                               |
| ------------------------------------------------------- | ----------------------------: |
| Brands with a usable mark in either source              |                         5,278 |
| **Shipped** (reachable from a domain, host or card row) | **3,778 brands, 3,952 files** |
| On disk                                                 |                       6.61 MB |
| Compressed                                              |                       3.03 MB |

Three cuts, each on a stated basis:

1. **1,500 brands with no domain in either manifest — not shipped, 2.70 MB.** `services::icons::resolve`
   looks up a host or a registrable domain and nothing else; there is no lookup by title. A brand with
   no domain row therefore has no route to the screen, so bundling it is weight that renders zero times.
2. **59 brands over a 16 KB per-file ceiling.** At 24–56px a mark needing more path data than that is
   an illustration, not an identity tile. Largest dropped: `lerna` 149 KB, `effector` 144 KB,
   `hugo` 137 KB, `composer` 122 KB, `memgraph` 104 KB.
3. **57 contested domains left unmapped.** More than one brand claimed the domain and none matched its
   label exactly. A wrong mapping puts another company's logo on someone's bank; a generated shape
   does not. Examples: `web.dev`, `trufflesuite.com`, `d3js.org`.

### Tiers, per ADD-001 rev 2

1. Bundled full-colour brand mark, resolved from the item's URL. Light and dark variants ship
   alongside the colour mark where a source provides them; a mark is never recoloured to fit a theme.
2. The user's own icon, processed entirely in Rust and stored encrypted in the item. No third-party
   asset, no notice required.
3. Locally generated geometric mark, seeded from the registrable domain. No third-party asset, no
   notice required.

## EFF long wordlist — run 3

Used by the passphrase generator (SPEC-V1 §7.3). 7,776 words, bundled, never fetched.

| Field   | Value                                                                                                                                 |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Source  | _to be filled in when the list is vendored_                                                                                           |
| Licence | _to be confirmed: the EFF publishes the list under CC BY 3.0 US — record the exact terms and attribution string here before shipping_ |
| Path    | `src-tauri/assets/eff_large_wordlist.txt`, one entry per line as `<5 dice digits>\t<word>`                                            |

Not yet vendored. `services::generator::passphrase` is implemented and refuses any list that is not
exactly 7,776 entries, so the feature fails closed rather than quietly generating weaker
passphrases; `pnpm check:wordlist` reports the file as absent today and validates it strictly
(count, distinctness, lowercase a–z) the moment it lands.

## Two-factor directory — run 3, live

Used by the "missing 2FA" check (SPEC-V1 §7.4) to know which services accept an authenticator
app. Bundled, versioned with the app, **never fetched**.

| Field   | Value                                                                          |
| ------- | ------------------------------------------------------------------------------ |
| Source  | Written for this product. `src-tauri/assets/twofactor-directory.tsv`           |
| Licence | Ours. No third-party data is redistributed, so there is nothing here to clear. |
| Size    | 188 registrable domains                                                        |

**Why it is not the obvious dataset.** The well-known public list — 2fa.directory, formerly
twofactorauth.org — is a community project whose terms are not unambiguously clear for
redistribution inside a commercial binary, and whose entries carry no per-entry provenance. §7.4
makes redistribution permission a precondition, not a preference. ADD-001 spent an entire addendum
refusing to guess at licences for brand icons; guessing here, for a much smaller prize, would undo
that reasoning. So the list was written for this product. Nothing in it is copied from that dataset.

**What an entry claims.** That the service accepts a time-based one-time code from a standard
authenticator app, on a normal consumer or developer account, without a paid upgrade. It does _not_
claim SMS, email codes, push approval, a hardware key, or a vendor's own app. Several of those are
stronger than TOTP — but they are not something Trynta can hold, and "add a one-time code" is only
actionable advice when a TOTP app is actually accepted. Apple, Steam, Netflix and most retail banks
are absent for exactly this reason.

**It is a floor, not a census.** A service that is missing counts as _not capable_, which is the
safe direction: the user is never nagged about something that cannot take a code, and the health
score's 2FA term simply covers fewer items. Accuracy was preferred to reach throughout — a wrong
entry produces a nag that can never be satisfied — so anything doubtful was left out.

**What changed when it shipped.** Before this, `two_factor_capable` was hardcoded to 0, which
triggered §7.4's redistribution branch for _every_ vault: the 20-point 2FA term vanished and the
other three weights silently became 43.75 / 31.25 / 25. The score was well defined, but it was not
the formula the breakdown displayed. `src-tauri/tests/report_two_factor.rs` now pins both paths —
listed services keep the 35/25/20/20 weights, unlisted ones still redistribute.

## Manrope — run 3, live

The interface typeface. The design's token layer names it first in `--font-sans`, and the layout's
fixed widths — the 96px field-label column, the 380px search pill, the 320px list column — are drawn
around its metrics. Without the file the stack falls through to the platform UI font, which renders
the same string about 8% narrower.

| Field   | Value                                                                                       |
| ------- | ------------------------------------------------------------------------------------------- |
| Source  | Manrope by Mikhail Sharanda — https://github.com/sharanda/manrope                           |
| Licence | SIL Open Font License 1.1 (redistribution permitted, including embedded in software)        |
| Path    | `public/fonts/manrope-latin.woff2`, `public/fonts/manrope-latin-ext.woff2` → `dist/fonts/`  |
| Faces   | One variable file per subset, weight axis 200–800, covering the whole 500/600/700/800 scale |

**Bundled, never fetched.** The `@font-face` rules in `src/theme/fonts.css` point at
`/fonts/…`, which is what the production CSP's `font-src 'self'` permits. A webfont pulled from a
CDN would be an outbound request on every launch — a fourth permitted request, which CLAUDE.md §4.7
does not allow, and the same class of leak ADD-001 exists to prevent, arriving through the typeface
instead of the icons.

Under the OFL the font may be embedded and redistributed; the Reserved Font Name clause means a
_modified_ copy may not keep the name Manrope. These files are unmodified subsets as published, so
the name stands. If they are ever re-subset or hinted differently, rename the family here and in the
token layer.

## Mozilla CA root store — run 2 onward

Compiled into the binary via `webpki-roots`, reached through `ureq` → `rustls`. It is the trust
anchor set for the two outbound requests the product makes (SPEC-V1 §7.4, §7.7).

| Field   | Value                                                        |
| ------- | ------------------------------------------------------------ |
| Source  | Mozilla CA Certificate Program, via the `webpki-roots` crate |
| Licence | CDLA-Permissive-2.0 (a data licence; no copyleft)            |

Listed here rather than left to `cargo deny` because it is **data compiled into the binary**, not
code, and because the trade-off is a security decision rather than a licensing one: bundling the
roots means they are updated by an app update rather than by the OS. ADD-002 Q13 accepted that
explicitly on the grounds that §7.7's updater makes it a real channel. The consequence to hold onto:
**a root store revocation reaches users at the speed of our release cadence.** If the updater ever
stops shipping, this becomes a reason to move to the platform verifier.

## Public Suffix List — run 3

To be compiled into the binary via the `psl` crate for registrable-domain (eTLD+1) matching. **Not a
dependency yet** — autofill and the §7.4 fix flow are the callers, and both are run 3.

| Field   | Value                                           |
| ------- | ----------------------------------------------- |
| Source  | Mozilla Public Suffix List, via the `psl` crate |
| Licence | MPL-2.0                                         |

Note: the list is a build-time snapshot. It goes stale, and a stale list is a correctness problem
for domain matching — bump the crate deliberately, on a schedule.

---

# Software dependencies — audited 2026-08-20

The sections above cover **data and assets**. This one covers **code**, because an
AGPL-3.0-or-later release makes dependency licence compatibility a distribution question rather
than a hygiene one. `cargo deny check licenses` enforces an allow-list on every build; this is the
audit behind that allow-list, plus the npm side, which `cargo deny` does not see.

## Rust — 604 crates in the resolved graph

Every licence in the tree, tallied from `cargo metadata --all-features`:

| Family                                         | Crates | AGPL-3.0 compatible                                                                                      |
| ---------------------------------------------- | -----: | -------------------------------------------------------------------------------------------------------- |
| MIT, and MIT-or-Apache dual                    |    496 | Yes                                                                                                      |
| Apache-2.0, incl. WITH LLVM-exception          |     71 | Yes — Apache-2.0 is compatible with GPLv3/AGPLv3 (not GPLv2)                                             |
| BSD-2-Clause, BSD-3-Clause, BSD-1-Clause, 0BSD |     12 | Yes                                                                                                      |
| ISC                                            |      6 | Yes                                                                                                      |
| Zlib                                           |     25 | Yes                                                                                                      |
| Unicode-3.0                                    |     19 | Yes                                                                                                      |
| Unlicense, CC0-1.0, MIT-0                      |     15 | Yes                                                                                                      |
| MPL-2.0                                        |      6 | Yes — MPL-2.0 §3.3 permits distribution under a Secondary Licence, which names the GPL family explicitly |
| CDLA-Permissive-2.0                            |      2 | Yes — a permissive **data** licence with no copyleft                                                     |

**No GPL-incompatible licence appears anywhere in the Rust dependency graph.** No crate lacks a
licence field, and none relies on a `license-file` this audit could not read.

The six MPL-2.0 crates are `cssparser`, `cssparser-macros`, `dtoa-short`, `selectors`,
`nucleo-matcher` and `option-ext`. MPL-2.0 is file-level copyleft: modifying one of those files
obliges publishing the modification. None of them is modified — they are consumed as published
crates.

The two CDLA-Permissive-2.0 entries are `webpki-roots` and `webpki-root-certs`, which are the
Mozilla CA root store repackaged as data. See the Mozilla CA root store section above.

**No Apache-2.0 dependency ships a `NOTICE` file.** All 425 were checked; Apache-2.0 §4(d) therefore
creates no propagation obligation here. See [`NOTICE`](NOTICE) for why one exists anyway.

## npm — 641 packages on disk, 6 of which ship

Only six npm packages reach the distributed bundle. They are the `dependencies` in
`package.json`; everything else is a `devDependency` used to build, lint or test, and none of it
is distributed.

| Shipped package         | Licence           |
| ----------------------- | ----------------- |
| `react`, `react-dom`    | MIT               |
| `@tanstack/react-query` | MIT               |
| `@tauri-apps/api`       | MIT OR Apache-2.0 |
| `zustand`               | MIT               |
| `lucide-react`          | ISC               |

All five licences are permissive and AGPL-compatible.

Five dev-only packages carry licences worth naming, none of which is distributed:

| Package             | Licence                 | Reached via                  | Note                                                                                                                                            |
| ------------------- | ----------------------- | ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `jszip`             | MIT OR GPL-3.0-or-later | `webdriverio`                | Dual; MIT applies. Compatible either way.                                                                                                       |
| `caniuse-lite`      | CC-BY-4.0               | `browserslist`               | Browser-support data used at build time.                                                                                                        |
| `spdx-exceptions`   | CC-BY-3.0               | `spdx-expression-parse`      | A data list. CC-BY-3.0 is not GPL-compatible for _software_; not distributed, so no conflict arises.                                            |
| `@promptbook/utils` | CC-BY-4.0               | `locate-app` → `@wdio/utils` | Transitive under the E2E runner.                                                                                                                |
| `css-value`         | none declared           | `webdriverio`                | **No licence field and no licence file.** Dev-only, so nothing is distributed under unclear terms — but it is unclear terms, and worth knowing. |

## Open licensing items

1. **Six bundled icons are CC-BY-SA 2.5 or 3.0, which are not GPL-compatible.** Creative Commons
   declared CC BY-SA **4.0** one-way compatible with GPLv3 in 2015; 2.5 and 3.0 were never covered
   by that declaration, and their share-alike term conflicts with the AGPL's. The six are:

   | Licence      | Domain         | Key        | Source |
   | ------------ | -------------- | ---------- | ------ |
   | CC-BY-SA-3.0 | `f-droid.org`  | `fdroid`   | thesvg |
   | CC-BY-SA-2.5 | `gentoo.org`   | `gentoo`   | thesvg |
   | CC-BY-SA-3.0 | `inkscape.org` | `inkscape` | thesvg |
   | CC-BY-SA-3.0 | `jenkins.io`   | `jenkins`  | thesvg |
   | CC-BY-SA-3.0 | `luanti.org`   | `luanti`   | thesvg |
   | CC-BY-SA-2.5 | `redmine.org`  | `redmine`  | thesvg |

   The position taken above — that each mark ships as a separate, individually licensed file and is
   not relicensed by the application — is the standard one for bundled assets, and it is still
   stated as a position rather than as settled law. **Resolve before 1.0.** The one-line fix, if the
   answer is to drop them, is to add `CC-BY-SA-2.5` and `CC-BY-SA-3.0` to `FORBIDDEN_LICENCE` in
   `scripts/build-icon-map.ts` and rerun `pnpm icons:build`; those six items fall back to a
   generated mark. The other 35 share-alike and copyleft marks — 12 CC-BY-SA-4.0, 19 GPL/AGPL,
   3 MPL-2.0, 1 LGPL-3.0 — are all compatible with AGPL-3.0 and are not affected.

2. **The installers carry no licence text.** `bundle.licenseFile`, `bundle.resources`,
   `bundle.copyright` and `bundle.publisher` are all unset in `src-tauri/tauri.conf.json`, so a
   built `.msi` or `.exe` ships without `LICENSE`, `NOTICE` or this file. Several bundled assets —
   CC-BY and CC-BY-SA marks, the SIL OFL typeface, MPL-2.0 components, and four per-brand written
   permissions — require their attribution to accompany the distribution, and AGPL-3.0 §4 requires
   the licence to accompany the conveyed work. **Fix before any public binary release.**

3. **The EFF wordlist is not vendored**, so the passphrase generator reports itself unavailable.
   See the wordlist section above.

## How to reproduce this audit

```bash
cargo deny check licenses                                  # the enforced allow-list
cargo metadata --format-version 1 --all-features            # every crate and its licence
pnpm icons:report                                           # per-icon licence tally
```

`src-tauri/assets/icon-map.tsv` is the per-icon record: one row per shipped mark, carrying its
source, variant and licence. Attribution for any single icon is one `grep` away.
