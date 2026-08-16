# Third-party notices

Every asset bundled with Keyring, its source and its licence. ADD-001 requires that each bundled
icon traces to a documented source here; this file also covers the wordlist and the two-factor
directory, which carry their own terms.

Software dependencies are covered separately by `cargo deny check licenses` and the allow-list in
`deny.toml`; this file is for **data and assets** shipped inside the binary.

> Status: nothing below is bundled yet. Each section lands with the run that introduces the asset,
> and **an asset may not ship before its licence line is filled in.** ADD-001 and SPEC-V1 §7.4 both
> make that a precondition, not a follow-up.

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

## Two-factor directory — run 3

Used by the "missing 2FA" check (SPEC-V1 §7.4) to know which services support second factors.
Bundled, versioned with the app, never fetched.

| Field   | Value                                                                                                |
| ------- | ---------------------------------------------------------------------------------------------------- |
| Source  | _to be filled in_                                                                                    |
| Licence | _**must be verified before shipping.** SPEC-V1 §7.4 makes redistribution permission a precondition._ |

## Public Suffix List — run 1 onward

Compiled into the binary via the `psl` crate for registrable-domain (eTLD+1) matching.

| Field   | Value                                           |
| ------- | ----------------------------------------------- |
| Source  | Mozilla Public Suffix List, via the `psl` crate |
| Licence | MPL-2.0                                         |

Note: the list is a build-time snapshot. It goes stale, and a stale list is a correctness problem
for domain matching — bump the crate deliberately, on a schedule.
