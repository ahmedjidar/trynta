//! Keyring cryptographic core.
//!
//! This crate is deliberately isolated: it depends on nothing else in the
//! workspace, has no knowledge of Tauri, `SQLite` or the item model, and its tests
//! compile in seconds. That is enforced by the absence of those dependencies in
//! its `Cargo.toml`, not by convention.
//!
//! The key hierarchy (SPEC-V1 §3.1):
//!
//! ```text
//!   master password + account salt
//!             │  Argon2id
//!             ▼
//!            MUK ── never persisted, never leaves Rust
//!             │  HKDF-SHA256, domain-separated
//!             ├─ muk.verify    unlock verifier, constant-time compare
//!             ├─ muk.header    HMAC over the canonical header
//!             ├─ muk.wrap      wraps the account private key bundle
//!             ├─ muk.vault     wraps every vault key
//!             └─ muk.appcache  HIBP prefix cache, generator history
//!
//!   vault key (per vault) ─┬─ encrypts vault metadata
//!                          ├─ derives the activity subkey
//!                          └─ wraps every item key
//!
//!   item key (per item) ───┬─ item.meta   → meta_ct
//!                          └─ item.secret → secret_ct
//! ```
//!
//! Nothing here invents cryptography. Argon2id, HKDF-SHA256,
//! XChaCha20-Poly1305, Ed25519, X25519, `BLAKE2b` and HMAC-SHA256, each used the
//! documented way, from one stable `RustCrypto` generation.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod aad;
pub mod backup;
pub mod envelope;
pub mod error;
pub mod kdf;
pub mod keys;
pub mod manifest;
pub mod padding;
pub mod rng;
pub mod subkey;
mod unreachable;

pub use aad::{Aad, Purpose, AAD_LEN, NO_SUBJECT};
pub use backup::{
    backup_manifest_root, derive_backup_muk, verify_backup_header_mac, BackupHeader, BackupSubkey,
    BACKUP_VERSION,
};
pub use envelope::{open, seal, Envelope, ENVELOPE_VERSION, NONCE_LEN};
pub use error::CryptoError;
pub use kdf::{calibrate, calibrate_with, derive_muk, KdfParams};
pub use keys::{AccountKeys, AccountPublicKeys, Key32, Muk, ACCOUNT_KEYS_LEN};
pub use manifest::{
    header_mac, leaf_hash, manifest_root, sign_manifest, verify_header_mac, verify_manifest,
    HeaderFields, ManifestEntry,
};
pub use padding::PAD_BLOCK;
pub use subkey::{
    derive_activity_subkey, derive_item_subkey, derive_subkey, verifier_from, verify_password,
    ItemSubkey, Subkey,
};

/// Reserved `key_id` values for header-level keys (SPEC-V1 §3.3).
///
/// Every other key gets a random UUID v4 stored next to its wrapped form.
pub mod reserved_key_id {
    /// `muk.wrap`, which seals the account private key bundle.
    pub const MUK_WRAP: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    /// `muk.appcache`, which seals the HIBP cache and generator history.
    pub const MUK_APPCACHE: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2];
    /// `muk.header`, which keys the header MAC.
    pub const MUK_HEADER: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3];
}
