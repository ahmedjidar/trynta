<div align="center">

# Trynta

**A password manager built for credentials that belong to more than one person.**

macOS · Windows · local-first · end-to-end encrypted

</div>

---

## Why

Most password managers treat sharing as an afterthought bolted onto a single-user product. You get
a read-only copy, or a link that expires, or a "team vault" that requires everyone to be on the
same paid plan under the same admin.

That model breaks the moment real people use it. Two founders share a Stripe account. Four
moderators rotate a Discord login. A family splits a Netflix subscription. Somebody changes the
password and everyone else is locked out until it gets pasted into a group chat — which is exactly
where credentials go to die.

Trynta makes co-ownership a first-class primitive. Two users confirm an invite in both
directions, and from then on they genuinely co-own the credential. Both see it. Both can use it.
Both can change it. Neither one is a guest in the other's vault, and neither depends on the other
being online.

## Principles

**Local-first.** Your vault lives on your machine and works with the network unplugged. Sync is a
convenience layer, never a dependency.

**Zero-knowledge.** Keys are derived from your master password on your device. Anything that ever
leaves the device is ciphertext we cannot read, by construction rather than by policy.

**No dark patterns.** No engagement mechanics, no upsell interstitials, no telemetry on what's in
your vault. Breach checks use k-anonymity: we learn a five-character hash prefix and nothing else.

**Fast enough to disappear.** Unlock, find, copy, gone. A password manager you notice is a
password manager you route around.

**Native, not a web page in a frame.** Platform biometrics, platform secure storage, platform
clipboard semantics — hand-written per OS rather than papered over by a cross-platform shim.

**Right now, that means Windows.** The macOS half is written and has never been compiled, so its
state is *unknown* rather than "coming soon". Windows is the platform with a green build. See
[`MACOS-UNVERIFIED.md`](MACOS-UNVERIFIED.md) for exactly what is unverified and how it gets
verified.

## Status

Pre-1.0, in active development. Not ready for real credentials yet.

No accounts. No subscriptions. No payment code anywhere in this repository.

## Stack

Tauri v2 · Rust · React · TypeScript · SQLite

Everything that touches a secret lives in Rust. The webview renders; it does not hold keys.

## Cryptography

| Purpose | Primitive |
|---|---|
| Key derivation from master password | Argon2id |
| Subkey derivation | HKDF-SHA256 |
| Symmetric encryption | XChaCha20-Poly1305 |
| Key agreement for sharing | X25519 |
| Signatures | Ed25519 |
| Randomness | OS CSPRNG only |

Items are encrypted individually under per-item keys, wrapped by per-vault keys, wrapped by keys
derived from your master password. Sharing works by wrapping an item key to a recipient's public
key — no plaintext ever transits, and no server is ever in a position to read anything.

Standard primitives, used the boring way. No novel constructions.

## Building

```bash
# Prerequisites: Rust (stable), Node 20+, pnpm
# macOS: Xcode Command Line Tools
# Windows: Visual Studio Build Tools with the C++ workload, WebView2 runtime

pnpm install
pnpm tauri dev          # run in development
pnpm tauri build        # produce a release bundle
```

```bash
pnpm test               # frontend unit tests
cargo test --manifest-path src-tauri/Cargo.toml
pnpm lint && pnpm typecheck
```

## Brand assets

Service logos in this application are the trademarks of their respective owners and are used
solely to identify those services within the interface. Their presence implies no affiliation with
or endorsement by those companies. Logos are bundled with the application and are never fetched at
runtime, so using Trynta does not disclose which services you have accounts with.

## Security

Found a vulnerability? Please report it privately rather than opening a public issue.
`security@` — see `SECURITY.md`.

## License

TBD before first public release.
