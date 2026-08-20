# Contributing to Trynta

Thanks for looking. Before you write anything, two things worth knowing:

1. **Every pull request needs a signed [CLA](CLA.md).** It is one line added to a file, once, and
   [`CLA.md`](CLA.md) explains exactly what it grants and why. Read the "Why this exists" section
   before you sign — it grants the Maintainer the right to relicense contributions commercially,
   which is a real thing to agree to and not a formality.
2. **This is a security product that has never been audited.** The bar for changes that touch
   cryptography, storage or the platform layer is high, and "it works" is not the bar.

---

## Getting it running

```bash
# Prerequisites
#   Rust stable — see rust-toolchain.toml, rustup will pick it up
#   Node 20+ and pnpm
#   Windows: Visual Studio Build Tools with the "Desktop development with C++"
#            workload, plus the WebView2 runtime (preinstalled on Windows 11)
#   macOS:   Xcode Command Line Tools                              (UNVERIFIED — see below)

git clone <your fork>
cd trynta
pnpm install
pnpm tauri dev
```

The passphrase generator needs the EFF long wordlist, which is not vendored for licensing reasons.
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) says where to get it and where to put it.
Without it, that one feature reports itself unavailable — everything else works.

### Windows is the only platform anyone can verify

CI runs the full gate on `windows-latest` for every push. **The macOS code has never been
compiled by anyone.** If you are on a Mac, you cannot build this today, and a patch that "fixes"
macOS cannot be verified by us either. That is not a reason not to send it — it is a reason to be
explicit in the pull request about what you did and did not run.

If you write macOS code, [`MACOS-UNVERIFIED.md`](MACOS-UNVERIFIED.md) has to grow a row in the same
commit. Read its header first; it explains the format and why the standard is _stricter_ for code
nothing compiles.

## Running the tests

```bash
pnpm test                    # frontend unit tests (Vitest)
cargo test --workspace       # Rust unit, integration and property tests
pnpm lint && pnpm typecheck  # eslint + tsc
cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::pedantic
cargo fmt --all --check
```

### The acceptance gate

```bash
pnpm verify:v1
```

This runs the frozen SPEC-V1 acceptance suite plus the whole toolchain, and it is what CI runs. It
is the thing that has to be green.

**On a machine with 16 GB of RAM, run it in slices instead:**

```bash
node scripts/verify-v1.mjs --run 1
node scripts/verify-v1.mjs --run 2
node scripts/verify-v1.mjs --run 3
```

The gate chains release-profile compiles, and linking the workspace in release wants several
gigabytes. A single invocation has exhausted memory on 16 GB machines and taken the desktop with
it. Run one slice at a time, and never run a debug and a release build at once.

## The rules that are not negotiable

These are not style preferences. A change that breaks one of them will be sent back regardless of
how good the rest of it is.

**The acceptance suite is frozen.** `tests/acceptance/` and `scripts/verify-v1.mjs` are never
edited, deleted, `#[ignore]`d or weakened. `FREEZE.lock` records their hashes and CI fails on any
change. If a criterion is wrong or unimplementable as written, open an issue and say so — that is
a specification conversation, not a test edit. Weakening a frozen test to make a run pass is the
worst possible change to this repository.

**Never invent cryptography.** No custom ciphers, no custom KDFs, no custom padding, no clever key
derivation. Use the audited crates already in the tree, in their documented way.

**Key material stays in Rust.** No key — master, vault, item or private — is ever sent over IPC,
written unencrypted to disk, or placed in a JavaScript variable. Copy-to-clipboard happens in
Rust; the plaintext never enters the webview. There is exactly one sanctioned plaintext path to
the frontend and it returns one field, for one item, on explicit user action.

**No secret reaches a log, a panic message, a `Debug` impl or an error string.** Error types that
could carry a secret are written so that they cannot. There are tests that assert this.

**Fail closed.** Any error in a decrypt, verify or authorise path denies the operation. Never fall
back to plaintext, never skip a signature check on error, never continue best-effort.

**Two outbound requests exist in the whole product** — HIBP range queries and the signed update
manifest check. Nothing else. `pnpm check:network` sanctions exactly those two call sites and fails
the build on a third, which needs a specification change rather than a pull request.

**No hardcoded colours, spacing, radii or type.** Everything goes through the token layer in
`src/theme/`. `pnpm check:tokens` enforces it. Visual design is produced separately and delivered
into `handoffs/`; if a screen has no design, build it functionally, mark it
`// UNSTYLED: awaiting handoff <name>`, and leave it. Placeholder styling never gets replaced — it
gets shipped.

**New dependencies that touch secrets, memory, serialisation or the network need discussion
first.** Open an issue before you add one. `cargo deny check` and `cargo audit` must pass, and the
crypto crates are pinned to a single RustCrypto generation — `pnpm check:crypto-generation`
enforces that there is exactly one copy of `digest`, `sha2`, `rand_core`, `aead` and
`curve25519-dalek` in the tree.

**`unsafe` is permitted only inside `src-tauri/src/platform/`,** only with a `SAFETY:` comment
describing the invariants it relies on. `pnpm check:unsafe` enforces both.

**Anything you add to the tree that you did not write goes in
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md)** with its licence, its source URL and its
required attribution. Licences must be compatible with AGPL-3.0-or-later.

**Every new first-party file carries an SPDX header:**

```
// SPDX-License-Identifier: AGPL-3.0-or-later
```

## Style

- **Rust** — `clippy::pedantic` clean, `rustfmt` clean, `thiserror` types and `Result` everywhere.
  No `unwrap()`, `expect()` or `panic!()` in production paths; the one accepted exception is
  `.expect()` on a poisoned mutex, which can only happen if another thread already panicked.
- **TypeScript** — `strict`, `noUncheckedIndexedAccess`, no `any`, no non-null `!` without a
  comment saying why it is sound. Discriminated unions over optional-field soup.
- **Comments explain why, not what.** A comment that restates the line above it is noise; a
  comment that records why an obvious approach was rejected is the most valuable thing in the file.
  Every comment must be true — a comment that overclaims a security property is a bug.
- **Every exported TypeScript symbol carries a TSDoc block.** Internal helpers stay bare.
- **Conventional Commits** — `feat:`, `fix:`, `refactor:`, `chore:`, `sec:`, `docs:`, `test:`. One
  logical change per commit.

## Opening a pull request

- Say what you built, what you deliberately left out, what you assumed, and what you are least
  confident about. That last one is genuinely useful and nobody thinks less of you for it.
- Say what you ran. "Gate slice 1 and 2 pass, 3 not run" is a better pull request than silence.
- Tests for behaviour you added or changed. Bugs get a regression test that fails without the fix.
- Do not report "done" on something partially wired.

## Reporting bugs and vulnerabilities

Ordinary bugs: open an issue, with the platform, the build and what you expected.

**Vulnerabilities: do not open a public issue.** [`SECURITY.md`](SECURITY.md) has the private
channel and the scope.
