<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

# Draft release notes — v0.1.0

Paste the block below the rule into the GitHub release body for `v0.1.0`. It is the same
copy as [`CHANGELOG.md`](../CHANGELOG.md), which is where `web/` also gets its changelog from, so
the three stay in step. **If you edit one, edit `CHANGELOG.md` and regenerate the other two** —
`pnpm build:site` for the site, copy-paste for the release.

Before publishing, check:

- [ ] The tag is `v0.1.0` on the commit CI built the artefacts from.
- [ ] Both artefacts are attached: the `.msi` and the NSIS `.exe`.
- [ ] The SHA-256 of each below still matches the artefacts you are attaching.
- [ ] "Set as a pre-release" is ticked. This is alpha and the page says so.
- [ ] The site is rebuilt and redeployed, so its changelog shows this release.

---

Trynta is a local-first, end-to-end encrypted password manager for Windows. This is the first
public build.

**Read this before you install it.** Trynta is pre-1.0 and has never been security-audited. No
third party has reviewed the cryptography, the storage format or the platform code. Windows is the
only verified platform. Multi-owner sharing — the reason the project exists — is not built. Don't
put credentials you actually rely on in it yet.

## What it does

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

## What leaves your machine

Two requests, and nothing else:

- The first five characters of a password's SHA-1 hash, to the Have I Been Pwned range API, with
  padding requested — only if you turn breach checking on.
- A signed update manifest — only if you leave update checks on.

3,772 brand marks ship inside the application rather than being fetched, because a favicon request
per entry would announce, in the clear, every service you hold an account with. There is no
sign-up, no server, no telemetry and no analytics.

## What it does not do

- **Sharing.** Multi-owner credentials do not exist yet. An X25519 keypair is generated and
  reserved for them; no key agreement happens anywhere.
- **Sync.** Nothing is synchronised between devices.
- **Autofill or a browser extension.** When it arrives, matching will be on the registrable domain
  through the Public Suffix List, never a substring.
- **Import from another manager.**
- **macOS.** The target compiled and passed the full gate once, on 17 August 2026, and nothing
  macOS has been compiled since. There is no macOS build here.

## Installing

Windows 10 build 1809 or later, x64. The WebView2 runtime is required and is present on current
Windows 11; the installer will fetch it if it is missing.

Two artefacts: an `.msi` and an NSIS `.exe`. Either is fine.

**Both are unsigned**, so SmartScreen shows a blue dialog reading `Windows protected your PC` with
the publisher listed as `Unknown publisher`. Getting past it takes `More info → Run anyway`. That
warning is accurate and you should not wave it away on our say-so — compare the SHA-256 below
against the file you downloaded first.

```
9884ECF57F9DB666503A85982C51A65B07697326185E2B1DCE693B7A581BDA60  Trynta_0.1.0_x64_en-US.msi
26CCCCAF955B7EDFE99128171D1569E4DD380C6110BE93348EB8672147E6B06E  Trynta_0.1.0_x64-setup.exe
```

## Reporting a problem

Security issues go to the private advisory form, not a public issue:
<https://github.com/ahmedjidar/trynta/security/advisories/new>. Everything else is welcome as an
issue.

Licensed under AGPL-3.0-or-later. The source is the whole product; there is no closed component.
