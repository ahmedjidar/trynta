# CLAUDE.md

Operating manual for AI agents working in this repository. Read this fully before your first
edit in any session. If an instruction here conflicts with a spec in `/specs`, the spec wins for
*what to build*; this file wins for *how to build it*.

---

## 1. What this is

**Trynta** — a desktop password manager for **macOS and Windows**, built as a single codebase
with full feature parity on both platforms from day one. Local-first, end-to-end encrypted.

Its differentiator is **true multi-owner credentials**: two people can co-own the same login
through a mutually confirmed invite. Both see it, both can use it, both can edit it, and neither
depends on the other staying online. Everything else in the product is table stakes; this is the
reason it exists. Treat sharing correctness and sharing cryptography as the highest-value code in
the repo.

**Current phase:** pre-1.0. No accounts, no subscriptions, no payment code, no license gating, no
telemetry. Do not add any of it. Do not add a "Pro" flag, a feature-flag stub for billing, or an
`isSubscribed` field "for later." When that layer arrives it gets its own spec.

---

## 2. Stack

| Layer | Choice | Notes |
|---|---|---|
| Shell | **Tauri v2** | Not Electron. Capability-scoped permissions. |
| Backend | **Rust** (stable, edition 2021+) | All crypto, all storage, all secret handling. |
| Frontend | **React + TypeScript**, Vite | TSX. Strict mode, no `any`. |
| Styling | **Tailwind CSS v4** + CSS custom properties | Chosen so design handoffs drop in with minimal translation, and so themes are runtime-swappable. |
| State | Zustand for app state; TanStack Query for anything async over IPC | No Redux. |
| Storage | SQLite via `rusqlite`, field-level encryption | The DB file is not the security boundary; the ciphertext is. |
| Tests | `cargo test` + `proptest` (Rust), Vitest (TS), **WebdriverIO + `@wdio/tauri-service`** (E2E) | Not Playwright — it drives browsers, not a Tauri binary hosting WKWebView. |

**Crypto crates are pinned to one stable RustCrypto generation.** Exactly one copy of `digest`,
`sha2`, `rand_core`, `aead`, and `curve25519-dalek` in the tree — enforced by `cargo deny` with
`multiple-versions = "deny"` on those crates. No release candidates anywhere in the crypto path.
The approved set and the full dependency list live in `ADD-002`; do not deviate from it without
asking.

Before adding **any** new dependency that touches secrets, memory, serialization, or the network:
stop and ask. Every new crate is new attack surface. `cargo deny check` and `cargo audit` must
pass in CI.

`opt-level = 3` in release, never `"s"` or `"z"`. Size-optimised Argon2 is slower per unit of
work, so KDF calibration compensates by choosing a smaller memory cost — optimising for binary
size silently weakens the KDF.

---

## 3. The single most important rule: you do not design the UI

Visual design for this product is produced separately in **Claude Design** and delivered into
`/handoffs`. That handoff is the source of truth for every visual decision.

**You must not invent:**
colour values · spacing scales · typography choices · border radii · shadows · layout composition ·
motion, easing, durations · icon style · empty-state illustration · anything else a designer decides.

**You are responsible for:**
component structure · accessibility semantics · keyboard behaviour · state management ·
performance · wiring the design tokens the handoff defines · faithful implementation.

**When a screen has no handoff yet:** build it functionally with unstyled or minimally structured
markup, wire it to real data, mark it `// UNSTYLED: awaiting handoff <screen-name>`, and move on.
Do not "make it look nice in the meantime." Placeholder styling is how a design system rots —
someone always forgets to replace it.

**Theming is architecture, not design.** Implement it now:

- Every colour, radius, spacing step, font stack, and duration is a CSS custom property in a
  single token layer. Zero hardcoded hex values, ever, anywhere in the app.
- Themes are data. A theme = a named set of token values, loadable at runtime without reload.
- Ship the structure for **dark and light** from the first commit, plus `system` following the OS.
  The *values* come from the handoff; you build the plumbing.
- Users must eventually be able to define their own theme. Design the token layer so that is a
  data change, not a code change.
- No gradients unless a handoff explicitly specifies one.

---

## 4. Non-negotiable security invariants

Violating any of these is a build-breaking bug, not a code review nit.

1. **Never invent cryptography.** No custom ciphers, no custom KDFs, no custom padding, no
   "clever" key derivation. Use the audited crates listed above, in their documented way. If a
   spec seems to require a novel construction, stop and ask.
2. **Master keys never leave Rust.** No key material — master key, vault key, item key, private
   key — is ever sent over IPC, written unencrypted to disk, or placed in a JS variable. Ever.
3. **Copy-to-clipboard happens in Rust.** When the user copies a password, the plaintext goes
   from the Rust decryption buffer to the OS clipboard directly. It never enters the webview.
4. **Reveal is the only plaintext path to the frontend.** It returns exactly one field, for one
   item, on explicit user action. The frontend must not persist it, cache it, put it in a store,
   or include it in any log/error/analytics path, and must clear it on blur, navigation, or lock.
5. **Zeroize everything.** All key and plaintext buffers use `Zeroizing`/`Secret`, pre-sized so
   growth can't orphan copies. `mlock`/`VirtualLock` the 32-byte keys on both platforms; log a
   warning if unavailable, never fail silently. **The Argon2 memory buffer is a documented
   exception** — it is far larger than the lockable working set and may be paged. Zeroization is
   best-effort by nature; never claim otherwise in the product or the specs.
6. **No secret ever reaches a log, a panic message, a `Debug` impl, or an error string.** Manually
   implement `Debug` as a redacting impl for every type holding secrets. Test this.
7. **Nothing phones home about vault contents.** There are exactly **two** permitted outbound
   requests in the entire product, and adding a third requires a spec change: (a) HIBP range
   queries, k-anonymous, 5 hex characters, `Add-Padding: true`; and (b) the signed update manifest
   check. Nothing else — `pnpm check:network` sanctions exactly those two call sites, `hibp.rs` and
   `updates.rs`, and fails the build on a third. Brand icons are bundled, never fetched
   (`ADD-001`). The app never
   probes a user's sites — not for favicons, not for `/.well-known/change-password`. No crash
   reporter that could capture memory. No analytics in pre-1.0.
8. **Autofill matches on registrable domain (eTLD+1) via the Public Suffix List.** Never substring
   or `contains()` matching. This is the difference between a password manager and a phishing
   accessory.
9. **Lock is real.** Locking wipes keys from memory, tears down decrypted caches, clears the
   clipboard if it holds a value we put there, and resets webview state. It is not a UI overlay.
10. **Fail closed.** Any error in a decrypt, verify, or authorize path denies the operation.
    Never fall back to plaintext, never skip a signature check on error, never continue "best
    effort."
11. **Strict CSP in `tauri.conf.json`.** No `unsafe-inline`, no `unsafe-eval`, no remote script
    origins. `withGlobalTauri: false`. DevTools disabled in release builds.
12. **Least-privilege capabilities.** Tauri v2 capability files grant the minimum permission set.
    No blanket `fs:default` or `shell:allow-execute`.

---

## 5. Architecture

Rust is a **Cargo workspace**, not one crate. `keyring-crypto` and `keyring-store` are separate
members — so their isolation is compiler-enforced rather than a convention, and their tests
compile in seconds. Tests that are slow to run are tests that get run less, which makes this a
security property rather than a convenience (ADD-003 §①).

```
crates/
  keyring-crypto/             kdf, aead, envelope, keys, hierarchy, manifest, backup format
                              — depends on NOTHING else in the workspace
  keyring-store/              sqlite schema, two-phase migrations, header, manifest maintenance,
                              item repository, app_state, backoff — depends only on keyring-crypto
tests/acceptance/  (FROZEN)   SPEC-V1 §11 acceptance suite. Never edited after commit; the hashes
                              in FREEZE.lock are checked by CI.
src/                          React + TypeScript
  app/                        shell, routing, providers
  features/<domain>/          vertical slices: items, generator, security, sharing, settings
  components/                 shared presentational components
  ipc/                        typed Tauri command bindings — the ONLY place invoke() appears
  theme/                      token definitions, theme loader, dark/light/system
  lib/                        pure helpers, no side effects
src-tauri/
  src/
    commands/                 #[tauri::command] surface only — thin, no logic
    platform/                 macos/ windows/ — biometrics, clipboard, secure storage
    services/                 generator, strength, totp, breach, icons, report
    error.rs                  redacting error types
scripts/                      verify-v1.mjs (FROZEN), check-freeze, check-tokens,
                              check-crypto-generation, check-bundle-size
specs/         (gitignored)   what to build — read these
addendums/     (gitignored)   amendments to specs — read these, they override
handoffs/                     design source of truth — read these before styling anything
```

**Layering rule:** `commands/` orchestrates and never contains business logic. `keyring-crypto`
depends on nothing else in the workspace; `keyring-store` depends only on `keyring-crypto`. Both
are asserted by `pnpm check:crypto-generation`, not merely intended. Frontend `features/` never
call `invoke()` directly — always through `src/ipc/`, so the whole IPC surface is typed in one
place and mockable in tests. An eslint rule enforces that `invoke` appears in exactly one file.

**The acceptance suite is frozen.** `tests/acceptance/` and `scripts/verify-v1.mjs` are never
edited, deleted, `#[ignore]`d or weakened after commit. `FREEZE.lock` records their hashes and CI
fails on any change. If a criterion is wrong or unimplementable as written, that is a spec
conversation — stop and raise it. Weakening an acceptance test to make a run pass is the worst
possible failure in this project, so the rule does not rely on anyone remembering it.

**Rust types are the source of truth across IPC.** TS types are generated with `ts-rs`
(dev-dependency, emits during `cargo test`) and committed. CI regenerates and fails on any diff.
Never hand-edit generated types.

**IPC discipline:** every command has a typed request and response. Item lists return decrypted
*metadata only* (title, subtitle, icon key, badges) — never secret fields. Secret fields are
fetched one at a time, on demand, by explicit command.

---

## 6. Platforms: Windows is verified, macOS is not

**Read this before you write a line of macOS code, and do not soften it anywhere.**

This section used to assert full parity — that neither platform was a port of the other and that
every feature shipped on both or on neither. That is no longer true, and repeating it would be a
lie in the operating manual. **ADD-005 supersedes it, and supersedes SPEC-V1 §8's parity
requirement.** The old wording is gone rather than qualified, deliberately: a reader skimming for
a parity guarantee should not find one to misread.

The actual position:

- **Windows is the verified platform.** CI runs the full gate on `windows-latest` for every
  push. That is the build that has to be green.
- **macOS compiled and passed the gate once, on 2026-08-17 (`c925f0f`), and nothing macOS has
  been compiled since.** ADD-005 moved it to tags-only that same afternoon and this repository
  has no tags. So there *is* one green macOS run in the Actions history — do not claim otherwise,
  it is checkable. But every macOS change made after it has **never compiled**, including the
  three platform files ADD-005 itself rewrote in the very commit that turned the compiler off.
  Treat macOS code written since 2026-08-17 exactly as this manual used to tell you to treat all
  of it: unknown, and unknown in ways it does not predict.
- macOS jobs run on tags and `workflow_dispatch` only, and are named `UNVERIFIED PLATFORM`.
- This is a budget decision, not a technical judgement: private repo, free Actions minutes
  exhausted, macOS runners bill at 10×. It reverts when there is real Apple hardware.

Calibrate your confidence accordingly. On 2026-08-17, macOS code that had been read, reviewed and
locally linted took **two CI round trips just to build** — three compile errors in `1423991`, three
clippy failures in `c925f0f`, plus a `crate-type` collision that produced two instances of
`keyring_store` in one test binary and a timing test that failed because the runner had three cores.
None of it was visible from a Windows machine. "It looks right" has already been wrong twice, and
that was with a compiler still checking. Nothing has checked since.

`cargo check --target aarch64-apple-darwin` **does not work from Windows** and this was measured,
not assumed: `keyring-crypto` cross-checks cleanly, `keyring-store` fails on `libsqlite3-sys`, and
`keyring` — the crate holding all the macOS code — fails on `objc2-exception-helper`, which
compiles Objective-C. There is no way to check macOS code from here without an Apple SDK.

### The standards get stricter, not looser

Unverified is not permission to be sloppy. Nothing will compile this code until there is hardware,
so it has to be right on the first read:

1. **Read the real API signatures before writing.** Fetch the docs, or better, read the vendored
   crate source under `~/.cargo/registry`. No guessing from memory, no plausible-looking calls.
   Prefer a crate's own constant over a hand-copied value — `AccessControlOptions::BIOMETRY_CURRENT_SET`
   over `1 << 3`.
2. **Every macOS path gets the same test coverage as its Windows counterpart**, even though the
   tests will not run. `tests/platform_macos.rs` mirrors `tests/platform_windows.rs` check for
   check, and its header carries the mapping table. Those tests are the first thing executed on
   real hardware.
3. **Mark every uncertainty `// UNVERIFIED: <what could be wrong>`** at the site, and add a
   *specific* row to `MACOS-UNVERIFIED.md` — a command to run or an observation to make, never
   "check that the Keychain works".
4. **Append to `MACOS-UNVERIFIED.md` in the same commit** as any macOS code you write.

The bar: when this runs on real hardware, the failures should be environment surprises — not code
that could have been right by reading the docs.

### The platform split itself is unchanged

| Concern | macOS (unverified) | Windows (verified) |
|---|---|---|
| Biometric unlock | Touch ID / LocalAuthentication | Windows Hello |
| Key-at-rest for biometric unlock | Keychain, Secure Enclave where available | DPAPI + Credential Manager, TPM where available |
| Modifier key | ⌘ | Ctrl |
| Window chrome | native traffic lights | native controls |
| Autostart / background | LaunchAgent | Registry Run key or Task Scheduler |

Anything platform-specific lives behind a trait in `platform/` with a `macos` and a `windows`
implementation. No `#[cfg]` scattered through business logic. `unsafe` is permitted only inside
`platform/`, only with a SAFETY comment, and `pnpm check:unsafe` enforces both — including in the
macOS files, which makes it the one check that covers macOS code from a Windows machine.

Never hardcode `⌘` in the UI. Keyboard hints resolve from a platform key-map.

---

## 7. Code conventions

**Rust**
- `#![forbid(unsafe_code)]` outside `platform/`, where any `unsafe` block carries a comment
  justifying it and describing the invariants it relies on.
- No `unwrap()` / `expect()` / `panic!()` in production paths. `thiserror` types, `Result`
  everywhere.
- Errors are redacting by construction: an error type that could carry a secret must not.
- `clippy::pedantic` clean, `rustfmt` clean.

**TypeScript**
- `strict: true`, `noUncheckedIndexedAccess: true`. No `any`. No non-null `!` assertions without a
  comment explaining why it's sound.
- Discriminated unions over optional-field soup, especially for item types.
- Follow the repository's **TSDoc discipline** convention: every exported symbol carries a TSDoc
  block, internal helpers stay bare, tags only when they add information the type signature
  doesn't already carry.
- No `dangerouslySetInnerHTML`. No `eval`. No dynamic script injection.
- Prefer composition over prop-drilling; colocate state with the feature that owns it.

**Naming**
- Rust: `snake_case` modules, `PascalCase` types. TS: `camelCase` values, `PascalCase` types and
  components. IPC command names: `domain_verb` (`item_reveal_field`, `share_invite_accept`).

---

## 8. Testing requirements

| Area | Requirement |
|---|---|
| `crypto/` | Known-answer tests against published vectors. Round-trip property tests via `proptest`. Tamper tests: every ciphertext bit-flip must fail authentication. |
| Sharing | Full multi-party integration tests: A invites B, B accepts, both edit, one revokes, keys rotate. Test the adversarial cases, not just the happy path. |
| Redaction | A test that asserts no secret-bearing type's `Debug`/`Display`/serialization emits plaintext. |
| Lock | A test that asserts key material is unreachable after lock. |
| Migrations | Every schema migration has a forward test with realistic fixture data. |
| E2E | Unlock → find item → copy → lock, on both platforms, in CI. |

Never commit real credentials as fixtures, not even fake-looking ones from a real site. Generated
test data only.

---

## 9. Working agreement

- **Read `/specs` and `/addendums` before starting.** Addendums amend specs and take precedence;
  read them second and let them override.
- **Ask before assuming** on anything touching crypto design, the sharing protocol, data schema
  changes, new dependencies, or anything that would change the threat model. A wrong guess in
  those areas is expensive to unwind later.
- **Small, reviewable commits.** Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`,
  `sec:`). One logical change per commit.
- **When you finish a unit of work**, state plainly: what you built, what you deliberately left
  out, what you assumed, and what you're least confident about. Do not report "done" on something
  partially wired.
- **Don't gold-plate.** Build what the spec says. If you think the spec is wrong, say so and wait —
  don't silently build the better version you have in mind.
- **Migrations are forever.** Any change to the on-disk format needs a version bump and a
  migration path. Users cannot lose vaults. There is no acceptable data-loss bug in this product.

---

## 10. Definition of done

A task is done when all of these are true:

- [ ] Works on macOS **and** Windows, verified — not assumed
- [ ] No hardcoded colours, spacing, or type; everything through the token layer
- [ ] Dark and light both render correctly
- [ ] No secret crosses IPC except through the one sanctioned reveal path
- [ ] Errors are handled and redacting; no `unwrap()` in production paths
- [ ] Keyboard-navigable; focus order sane; screen-reader labels present
- [ ] Tests written and passing; `clippy`, `rustfmt`, `tsc`, `eslint`, `cargo deny` all clean
- [ ] Nothing added that the spec didn't ask for