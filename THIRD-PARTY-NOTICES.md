# Third-party notices

Every asset bundled with Keyring, its source and its licence. ADD-001 requires that each bundled
icon traces to a documented source here; this file also covers the wordlist and the two-factor
directory, which carry their own terms.

Software dependencies are covered separately by `cargo deny check licenses` and the allow-list in
`deny.toml`; this file is for **data and assets** shipped inside the binary.

> Status: **two sections are live** — the Mozilla CA root store, compiled in since run 2, and the
> Manrope typeface, vendored in run 3. Everything else is a placeholder for an asset that has not
> been vendored, and **an asset may not ship before its licence line is filled in.** ADD-001 and
> SPEC-V1 §7.4 both make that a precondition, not a follow-up. A section headed "run 3" with a
> blank licence field is not bundled; check the dependency or the assets directory rather than
> trusting the heading.

---

## Brand icons — run 3

Service logos are the trademarks of their respective owners and are used solely to identify those
services within the interface. Their presence implies no affiliation with or endorsement by those
companies. Icons are bundled with the application and never fetched at runtime, so using Keyring
does not disclose which services you have accounts with.

The icon map carries `source`, `variant`, `licence` and `brand_hex` per entry from the first
commit, so this table is generated from the map rather than maintained by hand, and removing a
single brand stays a one-file change.

| Key                  | Brand | Tier | Source | Variant | Licence |
| -------------------- | ----- | ---- | ------ | ------- | ------- |
| _(none bundled yet)_ |       |      |        |         |         |

Tiers, per ADD-001 rev 2:

1. Official full-colour mark from the brand kit, where the kit permits redistribution in software.
2. Simple Icons glyph rendered in that brand's documented hex. CC0; the trademarks depicted remain
   their owners'.
3. Locally generated monogram. No third-party asset, no notice required.

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

## Two-factor directory — run 3

Used by the "missing 2FA" check (SPEC-V1 §7.4) to know which services support second factors.
Bundled, versioned with the app, never fetched.

| Field   | Value                                                                                                |
| ------- | ---------------------------------------------------------------------------------------------------- |
| Source  | _to be filled in_                                                                                    |
| Licence | _**must be verified before shipping.** SPEC-V1 §7.4 makes redistribution permission a precondition._ |

**Not shipped, and not fetched to work around that.** `security_report_run` reports
`twoFactorCapable = 0` while no directory is bundled, which triggers §7.4's documented
redistribution — the 20-point 2FA term becomes 0 and the other three weights become
43.75 / 31.25 / 25. The score is therefore fully defined without the directory, and no item is
credited or penalised for a second factor we have no basis to claim exists. Guessing capability from
the domain was considered and rejected: it would flag real items on no evidence.
`src-tauri/tests/report_two_factor.rs` pins that path with the arithmetic written out, so shipping a
directory later cannot silently change every score without a failing test.

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
