# Design decisions, for readers of the code

<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

Comments throughout this codebase cite `SPEC-V1 §7.6`, `ADD-002 Q13`, `ADD-005` and similar. Those
documents are **not in this repository** — they are working documents that also carry product
strategy and unreleased plans, and they are deliberately gitignored.

That left a hole: a reader hits 468 references to `SPEC-V1` and roughly 130 more to the addendums,
and cannot follow any of them. This file closes it. It records **the decisions the code depends
on** — enough that a citation in a comment resolves to something you can read — without
reproducing the planning material around them.

Where a decision is already documented in depth elsewhere in the repository, this file points
there rather than repeating it.

---

## The document set, and what each one is

| Document    | What it is                                                                                           | Where its substance lives in this repo                                                                                                                                                                                                            |
| ----------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **SPEC-V1** | The V1 product and security specification. Numbered sections; §11 is the acceptance criteria.        | The acceptance criteria are executable: [`tests/acceptance/`](../tests/acceptance) and [`scripts/verify-v1.mjs`](../scripts/verify-v1.mjs), both frozen. Security invariants: [`SECURITY.md`](../SECURITY.md) and [`CLAUDE.md`](../CLAUDE.md) §4. |
| **SPEC-V2** | Sharing and multi-owner credentials. **Not implemented.**                                            | Nothing. No code implements it; the X25519 keypair is generated and reserved for it.                                                                                                                                                              |
| **SPEC-V3** | Sync relay. **Not implemented.**                                                                     | Nothing.                                                                                                                                                                                                                                          |
| **ADD-001** | Brand icons: bundled only, never fetched.                                                            | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md) and [`scripts/build-icon-map.ts`](../scripts/build-icon-map.ts), whose header carries the full rationale. Summarised below.                                                                 |
| **ADD-002** | Scaffolding review: dependency and crate-selection decisions.                                        | [`deny.toml`](../deny.toml), `Cargo.toml` comments, [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md). Summarised below.                                                                                                                      |
| **ADD-003** | Run-1 checkpoint decisions: crate layout, backoff, backup format.                                    | Summarised below.                                                                                                                                                                                                                                 |
| **ADD-004** | Run-2 checkpoint decisions: `app_cache`, key-id reservations, backup body layout, migration pinning. | Summarised below. **This one had no tracked explanation before.**                                                                                                                                                                                 |
| **ADD-005** | Platform policy: Windows verified, macOS unverified.                                                 | [`SECURITY.md`](../SECURITY.md), [`MACOS-UNVERIFIED.md`](../MACOS-UNVERIFIED.md), [`CLAUDE.md`](../CLAUDE.md) §6. Summarised below.                                                                                                               |

**Addendums override the specs.** Where ADD-005 and SPEC-V1 §8 disagree about platform parity,
ADD-005 wins — see below.

---

## ADD-001 — brand icons are bundled, never fetched

**The decision.** Identity tiles use real service logos, and every logo ships inside the
application. The app never requests an icon, a favicon, or anything else from a site the user has
an account with.

**Why it is a security decision and not a visual one.** Fetching a favicon for each item tells
whoever serves it — and every network hop — exactly which services you hold credentials for, your
IP, and when you opened your password manager. That is the entire contents of your vault's index,
leaked as a side effect of drawing a list. It is the single most damaging thing a password manager
can do casually, and plenty do it.

**Three tiers, in resolution order:**

1. A bundled brand mark, resolved from the item's URL by registrable domain or exact host.
2. The user's own uploaded icon, processed entirely in Rust and stored encrypted inside the item.
3. A locally generated geometric mark, seeded from the domain. No network, no third-party asset.

**Consequences visible in the code.** `services::icons::resolve` looks up a host or a registrable
domain and nothing else — there is no lookup by title, which is why a brand with no domain in the
source manifests is not shipped at all. The map is generated at build time into
`src-tauri/assets/icon-map.tsv` and compiled in.

Full sourcing, per-icon licensing and the trademark position:
[`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md).

## ADD-002 — dependency and scaffolding decisions

**One RustCrypto generation.** Exactly one copy of `digest`, `sha2`, `rand_core`, `aead` and
`curve25519-dalek` in the tree, enforced by [`deny.toml`](../deny.toml) with
`multiple-versions = "deny"` on those crates and by `pnpm check:crypto-generation`. Two versions of
a crypto crate means two implementations, one of which nobody audited.

**No release candidates in the crypto path.** Ever.

**`opt-level = 3` in release, never `"s"` or `"z"`.** Size-optimised Argon2 is slower per unit of
work, and the KDF calibrator compensates by choosing a _smaller_ memory cost — so optimising the
binary for size silently weakens the key derivation. This is the kind of coupling that is invisible
unless someone writes it down.

**Q13 — `serde_json` and `webpki-roots`, both accepted with stated costs.** `serde_json` leaves
unzeroized scratch buffers when serialising item payloads; accepted because the alternative was a
hand-rolled format. `webpki-roots` pins the Mozilla CA store into the binary rather than using the
OS trust store, so roots update with the app rather than with Windows Update; accepted, and the
licence half of that decision is the `CDLA-Permissive-2.0` entry in `deny.toml`.

## ADD-003 — run-1 decisions

**`crates/keyring-store` is a separate workspace member**, not a module of the Tauri crate. The
isolation between crypto, storage and application is compiler-enforced rather than conventional,
and the two library crates compile and test in seconds. Tests that are slow to run are tests that
get run less, which makes this a security property rather than a convenience.

**`getrandom` at two major versions is accepted.** It is not in the list of crypto crates pinned to
one generation, and both versions defer to the same OS entropy source.

**The backup format bytes were frozen in run 1 and implemented in run 2.** Fixing the header layout
before writing the writer means the format is a specification the implementation satisfies, rather
than a description of whatever the implementation happened to emit.

**Backoff constants** for failed unlock attempts persist across process restarts — an attacker who
kills the process does not get a fresh allowance.

## ADD-004 — run-2 decisions

Referenced by the code in eleven places and, until this file, explained nowhere a reader could
reach.

**① The `app_cache` table (§4.4a).** A separate encrypted table for derived, regenerable state —
the settings blob, the cached security report — keyed by a small enum. Distinct from `app_state`,
which is deliberately _plaintext_ so the lock screen can render in the user's chosen theme before
any key exists. The split is the point: anything that would reveal something about the user goes in
`app_cache` and is encrypted; only the mode, the theme id and a few flags live in the clear.

**② Key-id reservations (§3.3).** Key identifiers are allocated from a reserved numbering scheme so
that a future key rotation, or a sharing-era key class, cannot collide with an existing one. Ranges
are reserved now because renumbering after data exists is a migration nobody wants.

**③ The backup body layout (§7.8).** The `.tryntabak` body is a versioned, length-prefixed
structure with its own manifest and Ed25519 signature, encrypted under a passphrase independent of
the master password. An unknown version is a hard error, never a best-effort parse — see
`crates/keyring-crypto/src/backup.rs` and its round-trip and tamper tests.

**④ Migration version pinning — an accepted pre-1.0 one-off.** The on-disk format changed without a
version bump, once, before anyone had real data. This is recorded in [`SECURITY.md`](../SECURITY.md)
under "On-disk format changed without a version bump" rather than quietly forgotten. It does not
set a precedent: migrations are forever, and every later change to the on-disk format needs a
version bump and a forward migration with a test.

**⑤ `update_checks_enabled` lives in `app_state`.** In the clear, with the theme, because the
updater has to know whether it may run before the vault is unlocked. It reveals nothing about vault
contents.

## ADD-005 — Windows is verified, macOS is not

**This supersedes SPEC-V1 §8's platform-parity requirement.** The specification originally required
that every feature ship on both platforms or neither. That is no longer true, and the addendum
replaces it rather than qualifying it.

**The position.** CI runs the full acceptance gate on `windows-latest` for every push. macOS jobs
run only on tags and manual dispatch, and are labelled `UNVERIFIED PLATFORM`. The macOS code has
compiled and passed the gate once, on 2026-08-17 at `c925f0f`, and has not been compiled since — ADD-005 moved it to tags-only that afternoon and there are no tags.

**Why.** Budget: a private repository with exhausted free Actions minutes, and macOS runners
billing at ten times the rate. It is not a judgement that macOS matters less, and it reverts when
there is Apple hardware.

**The standards get stricter, not looser.** Because nothing compiles the macOS code, it has to be
right on the first read: real API signatures checked against vendored crate source rather than
recalled, every macOS path carrying the same test coverage as its Windows counterpart even though
the tests do not run, and every uncertainty marked `// UNVERIFIED:` at the site with a matching row
in [`MACOS-UNVERIFIED.md`](../MACOS-UNVERIFIED.md).

**Measured, not assumed:** `cargo check --target aarch64-apple-darwin` does not work from Windows.
`keyring-crypto` cross-checks cleanly; `keyring-store` fails on `libsqlite3-sys`; the `keyring`
crate fails on `objc2-exception-helper`, which compiles Objective-C. There is no way to check macOS
code without an Apple SDK.

---

## What a reader still cannot see

Being straight about the limits of this file: it covers the decisions the **code** depends on. The
specification documents also contain product strategy, threat-model working notes, unreleased
plans and the V2 sharing protocol design. None of that is needed to read the code, and none of it
is reproduced here.

If a comment cites a section and you cannot work out what it means from this file, that is a gap
worth an issue.
