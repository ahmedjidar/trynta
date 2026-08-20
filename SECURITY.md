# Security

Trynta is pre-1.0 and not ready for real credentials. Please do not store anything you cannot
afford to lose while that remains true.

## macOS is unverified — read this before reporting or trusting anything on it

**The macOS build has never been compiled.** Not "lightly tested": no compiler has read it, no
test has run against it, and no one has launched it. Windows is the verified platform and the
only one with a green build. This is a budget decision (ADD-005) — private repo, exhausted free
Actions minutes, macOS runners at 10× — not a judgement that macOS matters less, and it reverts
once there is real Apple hardware.

What that means concretely, for anyone assessing this project:

- Every security claim below that depends on platform code — biometric unlock, the biometric key
  wrap at rest, clipboard concealment, key-page locking — is implemented **for Windows only**. The macOS half of each
  is written to the same standard and its behaviour is unknown. See "What is not verified" below
  for how much of the Windows half an automated test actually covers, which is less than all of it.
- Specifically unverified on macOS: that changing the enrolled fingerprint set destroys the
  Keychain item (`kSecAccessControlBiometryCurrentSet`), that the `org.nspasteboard.ConcealedType`
  marker actually lands on the pasteboard, and that Keychain access survives code-signing. Each is
  an enumerated row in [`MACOS-UNVERIFIED.md`](MACOS-UNVERIFIED.md).
- A macOS-only finding is **in scope and welcome**, and will not be treated as a duplicate of a
  known gap unless it is already a row in that file with the same failure described.

Two macOS build failures have already been found and fixed by CI after passing local review, which
is the honest measure of how much a careful read is worth here: not nothing, and not much.

## Reporting a vulnerability

Report privately. Do not open a public issue, and do not disclose before we have had a chance to
ship a fix.

- **Use GitHub's private vulnerability reporting** on this repository (Security → Report a
  vulnerability). That is the channel that is live today; it is private to the maintainers and it
  does not require an address to be published for scrapers to harvest.
- Include a proof of concept if you have one, and say which platform and build you saw it on.

A dedicated security address will be published here alongside the first tagged release. Until
then the reporting form above is the whole channel, and pointing at an address that does not
receive mail would be worse than saying so.

We will acknowledge within 3 working days and keep you updated as we work. If you would like
credit in the release notes, say so and tell us how you would like to be named.

We do not currently run a paid bounty programme.

## What is not verified

An honest list, because "we take security seriously" is not a security property. None of the
following has been confirmed by a third party or, in most cases, by an automated test.

| Area                                 | State                                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Third-party review**               | **None.** No audit, no penetration test, no external cryptographic review of any part of this codebase.                                                                                                                                                                                                                                 |
| **The whole macOS build**            | Never compiled. See the section above and [`MACOS-UNVERIFIED.md`](MACOS-UNVERIFIED.md).                                                                                                                                                                                                                                                 |
| **Biometric unlock (Windows Hello)** | Implemented and reachable, but the acceptance gate **skips** it — AC06 is marked `skip: requires the platform secure-store and biometric layer`. The signing path, the key wrap at rest, and invalidation on enrolment change have been exercised by hand, not by CI.                                                                   |
| **The updater's install path**       | The manifest signature check is tested. Downloading, verifying and _applying_ a real update to a real installation has never been done end to end.                                                                                                                                                                                      |
| **Memory locking**                   | Implemented for Windows and covered by tests — `VirtualLock` on the three 32-byte session keys, warning and continuing if the OS refuses. The macOS `mlock` counterpart is written and, like everything else on that platform, has never been compiled. What it does and does not buy is set out in `src-tauri/src/platform/memory.rs`. |
| **Clipboard history exclusion**      | The three Windows exclusion formats are set and tested, but only against the formats themselves — not against a machine with Cloud Clipboard actually syncing.                                                                                                                                                                          |
| **The share-alike bundled icons**    | Six bundled marks are CC-BY-SA 2.5/3.0, which are not GPL-compatible. Recorded in [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) as an open item. Not a vulnerability; a licensing one.                                                                                                                                             |

## Scope

In scope, and interesting to us in roughly this order:

1. **Anything that recovers plaintext without the master password.** Key hierarchy, envelope
   encryption, the KDF, the on-disk format.
2. **Anything that defeats the vault manifest.** Rolling an item back to an earlier revision,
   forging or bypassing `header_mac`, or resurrecting a deleted item without detection.
3. **Anything that gets a secret into the webview** other than the single sanctioned reveal path,
   or into a log, a panic message, an error string, a `Debug` impl or a crash artefact.
4. **Anything that makes the app talk to a network host it should not.** Exactly three outbound
   requests exist in the whole product: HIBP range queries, the signed update manifest check, and
   nothing else. A fourth is a vulnerability, not a feature.
5. **Clipboard, lock and auto-lock failures.** A secret that survives a lock, or a clipboard entry
   that outlives its timer or reaches Windows Clipboard History or Cloud Clipboard.
6. **Capability or CSP escapes** from the webview into the Tauri command surface.

## Out of scope

These are stated in the threat model as undefended, not overlooked:

- A compromised operating system, kernel-level malware, or a hardware keylogger.
- A webview compromised at the moment of unlock. The master password is typed into an `<input>`
  and exists as an unzeroizable JavaScript string for that moment. This is inherent to every
  webview-based password manager; CSP and capability hardening are what stand between us and it.
- **The Argon2 memory buffer being paged to disk.** It is far larger than the lockable working
  set on either platform, and it is a documented exception rather than an oversight: the three
  32-byte keys a live session holds — the MUK and the two account secrets — **are** pinned with
  `VirtualLock` at unlock, and a 64 MiB derivation buffer cannot be.
- **A hibernation image.** `VirtualLock` keeps a page out of the page file. It does not keep it out
  of `hiberfil.sys`, which is a complete memory dump by design.
- **Key material that has been moved or copied since unlock.** Locking pins an address, not a
  value. The buffers the session owns are pinned; a clone made later lives somewhere nobody locked.
  Zeroization is best-effort for the same family of reasons — allocators reuse blocks and `String`
  growth orphans copies.
- Screen capture by another process while the vault is unlocked, unless the user has enabled the
  opt-in screen-capture mitigation.
- A weak master password.
- Coercion of the user.
- Denial of service against the local application.
- Reports generated by a scanner with no demonstrated impact.

## Deliberate process aborts

`keyring-crypto` contains a small number of branches that call
`std::process::abort()` through one helper, `invariant_violated`, in
`crates/keyring-crypto/src/unreachable.rs`. Grep for that name to find every site.

They sit on library calls that return `Result` for a failure mode that cannot occur at the sizes
we use — HKDF-Expand rejects output longer than 255 × 32 bytes and we ask for 32; HMAC accepts a
key of any length and ours is 32. The branch exists because the type system cannot express "this
is dead", and something has to be in it.

Every alternative is worse. Returning a fallback value would hand back a _predictable key_ or a
MAC we never computed, and the caller would proceed as though it were real.
`unwrap`/`expect`/`panic!` are banned in production paths, and unwinding would let a
`catch_unwind` resume with half-zeroized state or carry a partially formatted secret in the panic
payload. Aborting gives no unwinding and nothing to catch, which is what "fail closed" means here.

If you find a way to _reach_ one of these, that is a vulnerability and we want to hear about it.

## On-disk format changed without a version bump — a pre-1.0 one-off

Two run-2 changes altered the on-disk format while `schema_version` and `payload_version` both
stayed at 1:

- `app_cache`, the encrypted key/value table, was added to the **initial schema** rather than as a
  schema migration to version 2.
- `items.secret_ct` gained the TOTP parameters (algorithm, digits, period, issuer, account), which
  changes its `postcard` encoding. Before this, the store kept only the shared secret and silently
  discarded the rest, so an item saved as SHA-256 at 8 digits came back as SHA-1 at 6 and generated
  codes that never worked.

Neither got a version bump, and that is forced rather than chosen. The frozen acceptance suite pins
both counters: `tests/acceptance/tests/ac16_migrations.rs` asserts that a freshly created vault
reports `schema_version == 1` **and** `payload_version == 1`, its fixtures occupy schema versions 2
through 7 and payload version 2, and `MigrationSet::validate` rejects any injected migration whose
version is not strictly greater than the current constant. Raising either constant breaks three
assertions in a file that is never edited to make a run pass.

**The consequence, stated plainly:** a vault created by an earlier run-2 build will not open on this
one. `app_cache` will be missing, and the secret payload will fail to decode. There is no migration
path and none can be written while the counters are pinned.

This is acceptable exactly once, and only because nothing has been released: every affected vault is
a developer's local file, and a migration could not have recovered the discarded TOTP parameters
anyway — it could only have re-shaped them around invented defaults. **After the first release this
would be an unacceptable data-loss bug**, so freeing version numbers for real migrations — by moving
the acceptance fixtures into a reserved high range and regenerating `FREEZE.lock` — has to happen
before then.

Accepted deliberately by the spec owner as a pre-1.0 one-off. Do not treat it as precedent.

## What we will not do

- Ask you to sign an NDA to report a bug.
- Argue about a takedown request for a bundled brand icon.
- Ship a "fix" that weakens an acceptance test rather than the bug.
