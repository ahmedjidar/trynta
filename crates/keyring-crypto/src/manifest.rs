//! Vault manifest and header authentication — rollback resistance.
//!
//! SPEC-V1 §3.5. The AAD binds a ciphertext to its item and revision, but it does
//! not stop an attacker who can write the file from restoring a genuine earlier
//! version of the whole row: every AAD field matches, because the row is a real
//! historical record. The manifest is what catches that.
//!
//! Two pieces, and both are load-bearing:
//!
//! - `manifest_sig` — Ed25519 over a BLAKE2b-256 root of every live item's
//!   `(id, revision, H(meta_ct), H(secret_ct))`.
//! - `header_mac` — HMAC-SHA256 under `muk.header` over the canonical header.
//!   Without it the signature is worthless: an attacker who can rewrite a row can
//!   rewrite `pubkey_ed25519` too, sign a manifest of their own with a key they
//!   control, and the signature verifies. The MAC is what binds the public keys
//!   to the master password.
//!
//! `header_mac` is verified immediately after key derivation, before anything
//! else is read. `manifest_sig` is verified at unlock, after `meta_ct` decryption.
//!
//! ## Domain separation
//!
//! SPEC-V1 §3.5 gives the manifest's fields but not the exact hash input, so it
//! is pinned here: every hash is prefixed with a distinct, versioned domain
//! string, and the root commits to the entry count before any entry. Without the
//! count, two different vaults could in principle produce the same byte stream.
//! This is specification of an underspecified encoding, not a novel construction.

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::CryptoError;
use crate::kdf::KdfParams;
use crate::keys::{AccountKeys, Key32};

type Blake2b256 = Blake2b<U32>;
type HmacSha256 = Hmac<Sha256>;

/// Domain prefix for the manifest root.
pub const DOMAIN_ROOT: &[u8] = b"keyring/v1/manifest";
/// Domain prefix for a per-ciphertext leaf hash.
pub const DOMAIN_LEAF: &[u8] = b"keyring/v1/manifest/leaf";
/// Domain prefix for the canonical header encoding.
pub const DOMAIN_HEADER: &[u8] = b"keyring/v1/header";

/// One live item's contribution to the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestEntry {
    /// The item's id.
    pub item_id: [u8; 16],
    /// The revision this row is at.
    pub revision: u64,
    /// [`leaf_hash`] of the item's `meta_ct`.
    pub meta_hash: [u8; 32],
    /// [`leaf_hash`] of the item's `secret_ct`.
    pub secret_hash: [u8; 32],
}

/// Hash one stored ciphertext for inclusion in the manifest.
#[must_use]
pub fn leaf_hash(ciphertext: &[u8]) -> [u8; 32] {
    let mut h = Blake2b256::new();
    h.update(DOMAIN_LEAF);
    h.update(ciphertext);
    h.finalize().into()
}

/// Compute the manifest root over every live item.
///
/// Entries are sorted by `item_id` so the root is independent of row order.
/// Soft-deleted items are *not* included — which is what makes clearing a
/// `deleted_at` detectable.
#[must_use]
pub fn manifest_root(entries: &mut [ManifestEntry]) -> [u8; 32] {
    root_with_domain(DOMAIN_ROOT, entries)
}

/// [`manifest_root`] under an explicit domain prefix.
///
/// Exists so the `.tryntabak` container can commit to the same entry shape
/// under its own domain, which stops a vault's `manifest_sig` being replayed
/// into a backup to vouch for a different set of items.
#[must_use]
pub fn root_with_domain(domain: &[u8], entries: &mut [ManifestEntry]) -> [u8; 32] {
    entries.sort_unstable_by(|a, b| a.item_id.cmp(&b.item_id));

    let mut h = Blake2b256::new();
    h.update(domain);
    h.update((entries.len() as u64).to_be_bytes());
    for e in entries.iter() {
        h.update(e.item_id);
        h.update(e.revision.to_be_bytes());
        h.update(e.meta_hash);
        h.update(e.secret_hash);
    }
    h.finalize().into()
}

/// HMAC-SHA256 over `message` under a 32-byte key.
///
/// One place, so both header formats compute their MAC identically and there is
/// a single site to audit.
#[must_use]
pub fn hmac_sha256(key: &Key32, message: &[u8]) -> [u8; 32] {
    // Unreachable: HMAC accepts a key of any length, and this one is 32 bytes.
    // Returning a MAC we did not compute would be worse than stopping. See
    // `crate::unreachable`.
    let Ok(mut mac) = HmacSha256::new_from_slice(key.expose()) else {
        crate::unreachable::invariant_violated("HMAC-SHA256 accepts a key of any length")
    };
    mac.update(message);
    mac.finalize().into_bytes().into()
}

/// Sign a manifest root with the account Ed25519 key.
#[must_use]
pub fn sign_manifest(keys: &AccountKeys, root: &[u8; 32]) -> [u8; 64] {
    keys.sign(root)
}

/// Verify a manifest signature against the account public key.
///
/// # Errors
///
/// [`CryptoError::BadSignature`] if the key is malformed or the signature does
/// not verify.
pub fn verify_manifest(
    public_key: &[u8; 32],
    root: &[u8; 32],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    crate::keys::verify_ed25519(public_key, root, signature)
}

/// The header fields covered by [`header_mac`].
///
/// `header_mac` itself is excluded, for the obvious reason. Everything else in
/// the header row is here — including `manifest_sig`, so the signature cannot be
/// swapped for one over a different item set.
#[derive(Debug, Clone, Copy)]
pub struct HeaderFields<'a> {
    /// Pre-unlock DDL version.
    pub schema_version: u32,
    /// Post-unlock payload version.
    pub payload_version: u32,
    /// Envelope format version.
    pub envelope_version: u16,
    /// 32-byte account salt.
    pub account_salt: &'a [u8],
    /// Argon2id cost. MAC'd as three parsed integers, not as the stored JSON
    /// text: JSON is not canonical, and it is the parsed values that decide how
    /// expensive an offline attack is.
    pub kdf: KdfParams,
    /// The `muk.verify` subkey as stored.
    pub verifier: &'a [u8],
    /// Account X25519 public key.
    pub pubkey_x25519: &'a [u8],
    /// Account Ed25519 public key.
    pub pubkey_ed25519: &'a [u8],
    /// Account private key bundle, sealed under `muk.wrap`.
    pub privkeys_ct: &'a [u8],
    /// Ed25519 signature over the manifest root.
    pub manifest_sig: &'a [u8],
    /// Vault creation time, Unix milliseconds.
    pub created_at: i64,
}

impl HeaderFields<'_> {
    /// The canonical byte encoding the header MAC is computed over.
    ///
    /// Every variable-length field is length-prefixed, so no two distinct
    /// headers can share an encoding.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(DOMAIN_HEADER);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.payload_version.to_be_bytes());
        out.extend_from_slice(&self.envelope_version.to_be_bytes());
        out.extend_from_slice(&self.kdf.m_kib.to_be_bytes());
        out.extend_from_slice(&self.kdf.t.to_be_bytes());
        out.extend_from_slice(&self.kdf.p.to_be_bytes());
        for field in [
            self.account_salt,
            self.verifier,
            self.pubkey_x25519,
            self.pubkey_ed25519,
            self.privkeys_ct,
            self.manifest_sig,
        ] {
            let len = u32::try_from(field.len()).unwrap_or(u32::MAX);
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(field);
        }
        out.extend_from_slice(&self.created_at.to_be_bytes());
        out
    }
}

/// Compute the header MAC under the `muk.header` subkey.
#[must_use]
pub fn header_mac(key: &Key32, header: &HeaderFields<'_>) -> [u8; 32] {
    hmac_sha256(key, &header.canonical_bytes())
}

/// Verify the header MAC in constant time.
///
/// # Errors
///
/// [`CryptoError::BadHeaderMac`] if it does not match. The caller must treat this
/// as tampering and refuse to unlock — never partial-open, never "repair"
/// (SPEC-V1 §3.5).
pub fn verify_header_mac(
    key: &Key32,
    header: &HeaderFields<'_>,
    stored: &[u8; 32],
) -> Result<(), CryptoError> {
    let computed = header_mac(key, header);
    if computed.ct_eq(stored).into() {
        Ok(())
    } else {
        Err(CryptoError::BadHeaderMac)
    }
}
