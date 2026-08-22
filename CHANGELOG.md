# Changelog

All notable changes to Trynta are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Pre-1.0 means the on-disk
format may still change; when it does, a migration ships with it and no vault is left behind.

**This file is the only copy of the release notes.** `web/` generates its changelog page from it,
and the GitHub release body is pasted from it, so the three cannot drift apart.

<!-- Entry types, in the order they are listed under a release:
     Security · Added · Changed · Fixed · Removed · Note
     One sentence, occasionally two; the second explains a consequence, not an
     implementation. If it is not worth a sentence, it is not worth a line. -->

## [Unreleased]

Nothing yet.

## [0.1.0-alpha] — 2026-08-22

The first public build. It is a working local password manager and it is not finished: read the
notes at the bottom before you install it.

### Added

- Logins, secure notes, cards and identities in an encrypted local vault, with multiple vaults that
  can be renamed and recoloured.
- Unlock with a master password derived by Argon2id, with exponential backoff that survives a
  restart. There is no wipe-after-N-failures — it is too easily turned into a denial of service by
  somebody who has your file.
- Unlock with Windows Hello, off by default. Your master password is still required every fourteen
  days, so losing the biometric never means losing the vault.
- Copying a password decrypts it in Rust and writes the system clipboard directly. The value never
  enters the webview that draws the interface, and the clipboard is cleared after thirty seconds —
  only if what is on it is still what Trynta put there.
- Revealing a secret is the one path a plaintext takes to the interface: one field, one item, on an
  explicit action, rate-limited to twenty a minute.
- A password and passphrase generator built on the operating system's random source with rejection
  sampling, reporting the entropy it actually produced rather than a bar with a colour. The
  passphrase wordlist is the EFF long list, vendored and verified by hash.
- One-time codes to RFC 6238 — SHA-1, SHA-256 and SHA-512, six and eight digits — from an
  `otpauth://` link or a bare setup key.
- A security report that scores every stored password and groups the ones used twice, with an
  optional breach check that sends the first five characters of a SHA-1 hash to Have I Been Pwned
  and matches locally.
- Encrypted backup and restore under a passphrase chosen for the file, independent of the master
  password.
- Fuzzy search over an index built in memory at unlock, measured under 16 ms at the 95th percentile
  on a vault of five thousand items.
- Light, dark and follow-the-system themes, starting on light, plus themes you can write yourself
  as JSON. A theme is validated in Rust before anything is applied: it cannot fetch, and it cannot
  contain `url()`.
- A signed update check, which you can switch off.
- Optional exclusion from screenshots and screen sharing, off by default.
- A first-run tour: one card on the lock screen about what a master password is, and four inside
  the app. It runs once.

### Security

- 3,772 brand marks ship inside the application rather than being fetched. A favicon request per
  entry would announce, in the clear, every service you hold an account with.
- The whole product makes exactly two outbound requests: the breach check above and the signed
  update manifest. A build-time check fails on a third, and the frozen acceptance suite asserts
  that opening the security report makes none at all.
- The three 32-byte keys a live session holds are pinned with `VirtualLock`, keeping them out of
  the page file, and every key buffer is zeroed when it drops.

### Note

- **Never audited.** No third party has reviewed the cryptography, the storage format or the
  platform code. Don't put credentials you actually rely on in this yet.
- **Windows only.** The macOS target compiled and passed the full gate once, on 17 August 2026, and
  nothing macOS has been compiled since. There is no macOS build.
- **Sharing is not built.** Multi-owner credentials are the reason this project exists and they do
  not exist. An X25519 keypair is generated and reserved for them; no key agreement happens
  anywhere.
- **The installers are unsigned.** Windows SmartScreen will warn and will list the publisher as
  unknown. That warning is accurate — compare the SHA-256 on the release page before running
  anything.

[unreleased]: https://github.com/ahmedjidar/trynta/compare/v0.1.0-alpha...HEAD
[0.1.0-alpha]: https://github.com/ahmedjidar/trynta/releases/tag/v0.1.0-alpha
