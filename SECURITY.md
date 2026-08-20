# Security

Trynta is pre-1.0 and not ready for real credentials. Please do not store anything you cannot
afford to lose while that remains true.

## macOS is unverified — read this before reporting or trusting anything on it

**The macOS build compiled and passed the gate once, on 2026-08-17, and nothing macOS has been
compiled since.** CI found and fixed three compile errors (`1423991`) and three clippy failures
(`c925f0f`) that morning, after which macOS ran green on every push. ADD-005 (`75e07df`, 15:54 the
same day) then moved macOS to tags and manual dispatch only, and this repository has no tags.

That boundary matters in both directions. It is a real green build, verifiable in the Actions
history — not a hope. And ADD-005 rewrote all three macOS platform files _in the same commit that
turned the compiler off_, so even the code that policy shipped has never been built. Everything
added since — the memory locking, the Keychain access-group fix, the icon pipeline, the rename — is
unbuilt too.

Windows is the verified platform and the only one with a currently green build. This is a budget
decision (ADD-005) — private repo, exhausted free Actions minutes, macOS runners at 10× — not a
judgement that macOS matters less, and it reverts once there is real Apple hardware.

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

The one macOS build that did happen is the honest measure of how much a careful read is worth here:
code that had been read, reviewed and locally linted still took two CI round trips just to compile.
Not nothing, and not much. Everything added since has had none of even that.

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

An honest list, because "we take security seriously" is not a security property. Each row names the
part that is **not** covered, and where something _is_ covered it says by what.

| Area                                  | State                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Third-party review**                | **None.** No audit, no penetration test, no external review of the cryptography, the storage format or the platform code. Nothing below has been looked at by anyone outside this project.                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| **The whole macOS build**             | Compiled and green once, on 2026-08-17 at `c925f0f`. Nothing macOS has been compiled since — ADD-005 moved it to tags-only that afternoon and there are no tags, and ADD-005 itself rewrote all three macOS platform files in that same commit. See the section above and [`MACOS-UNVERIFIED.md`](MACOS-UNVERIFIED.md).                                                                                                                                                                                                                                                                                                                                          |
| **Biometric unlock (Windows Hello)**  | The acceptance gate **skips** it: AC06 is `skip: requires the platform secure-store and biometric layer`. End-to-end tests do exist — `tests/platform_hello_enrolled.rs` covers enrol, sign, unwrap, signing-key stability across calls, and revocation making a wrap unopenable — but they are opt-in behind `TRYNTA_TEST_HELLO=1` because they raise a real consent prompt and block on a human answering it. **CI has never run them.** What CI does cover is narrower: that availability is reported honestly, that unwrapping with no enrolment is an invalidation rather than a panic, and that revoking a missing enrolment still clears any stored wrap. |
| **The updater's install path**        | Weaker than it sounds. `tests/updater.rs` asserts the _configuration_: an endpoint cannot be set without a public key to verify against, no `dangerous*` escape hatch is enabled, the webview cannot start a download or an install, and the running version matches what the bundle declares. **No test verifies a signature against a real manifest**, and no update has ever been downloaded, verified and applied to a real installation. The signature check is the plugin's; we have tested that we asked for it, not that it happens.                                                                                                                     |
| **Memory locking**                    | The Windows half is implemented and tested — `VirtualLock` on the three 32-byte session keys, called from `SessionManager::adopt`, warning and continuing if the OS refuses. The macOS `mlock` counterpart was added after the last macOS build and has never been compiled. Two limits apply on both platforms and are not defects to be fixed later: locking pins an **address, not a value**, so a key moved or cloned after unlock lives somewhere nobody locked; and it does nothing about `hiberfil.sys`, which is a complete memory dump by design. `src-tauri/src/platform/memory.rs` sets this out at length.                                           |
| **Clipboard history exclusion**       | The three Windows exclusion formats are asserted present on a real clipboard write by `platform_windows.rs::a_secret_copy_carries_every_history_exclusion_format`. What is untested is the thing that matters most: a machine with Cloud Clipboard **actually syncing**, where the formats have to be honoured rather than merely set.                                                                                                                                                                                                                                                                                                                           |
| **Anything under load, or over time** | `proptest` covers envelope round-trips, generator entropy and the report's score arithmetic. There is no fuzzing of the on-disk format or the theme validator, no long-running soak, and no concurrency stress on the session state machine beyond the threads a couple of tests happen to spawn.                                                                                                                                                                                                                                                                                                                                                                |

## Scope

In scope, and interesting to us in roughly this order:

1. **Anything that recovers plaintext without the master password.** Key hierarchy, envelope
   encryption, the KDF, the on-disk format.
2. **Anything that defeats the vault manifest.** Rolling an item back to an earlier revision,
   forging or bypassing `header_mac`, or resurrecting a deleted item without detection.
3. **Anything that gets a secret into the webview** other than the single sanctioned reveal path,
   or into a log, a panic message, an error string, a `Debug` impl or a crash artefact.
4. **Anything that makes the app talk to a network host it should not.** Exactly **two** outbound
   requests exist in the whole product: HIBP range queries and the signed update manifest check.
   `pnpm check:network` sanctions those two call sites and fails the build on any other. **A third
   is a vulnerability, not a feature.**
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

## What uninstalling does to your vault

Stated here because it is a data-loss decision, and because leaving it undocumented was
itself the problem: nothing told a user either way, and both silent answers are bad.
Deleting a vault without asking destroys data. Keeping one without saying so leaves an
encrypted copy of every password on a machine the user believes is clean.

**The uninstaller asks, and the default is to keep.**

| Choice             | What happens                                                                                                                                                                                                                                                                                                                                                     |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Keep** (default) | `%APPDATA%\Trynta` is left exactly as it is. The application is removed. Reinstall later and every item, vault, setting and one-time code is as you left it. The vault stays encrypted; nothing can read it without your master password.                                                                                                                        |
| **Delete**         | `%APPDATA%\Trynta` is removed recursively — every password, note, card and one-time code. **This cannot be undone.** Without a backup exported through Settings → Backup and restore, the contents are unrecoverable by anyone, including us: the vault is encrypted under a key derived from your master password and no copy of that key exists anywhere else. |

Choosing delete asks a second time. A single click should not be the whole distance to
permanent data loss.

Three details worth knowing:

- **An upgrade never asks and never deletes.** Installing a newer version over an older
  one replaces program files and leaves the vault alone. The question only appears on a
  real uninstall.
- **A silent or unattended uninstall keeps the vault.** `/S` and passive mode take the
  answer that loses nothing, because there is nobody to ask.
- **If deletion fails**, the uninstaller says so and names the folder rather than
  reporting a clean removal over a vault that is still there. The usual cause is a file
  still open.

Uninstalling never touches a backup file you exported yourself — those live wherever
you saved them and are encrypted under their own separate passphrase.

### A bug this replaced, recorded because it was wrong in an instructive direction

Tauri's NSIS template ships a "Delete the application data" checkbox that removes
`%APPDATA%\<bundle identifier>` — for us, `%APPDATA%\dev.trynta.desktop`. Trynta has
never written there. `platform::paths` builds its directory from the product name,
`%APPDATA%\Trynta`, because SPEC-V1 §8 specifies a human-readable path.

So the checkbox removed nothing. Nothing was ever destroyed by it — but a user who
ticked it to clean the machine kept their entire vault on disk having been told it was
gone, which for a password manager is the worse of the two ways to be wrong. The
checkbox's label is now specific about what it deletes, and the decision above is what
actually acts.

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
