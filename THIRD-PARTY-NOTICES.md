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
