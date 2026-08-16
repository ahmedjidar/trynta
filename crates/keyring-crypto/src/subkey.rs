//! HKDF-SHA256 subkey derivation with domain separation.
//!
//! SPEC-V1 §3.1. Info strings are literal, versioned, and defined once here.
//! Never reuse a derived key across purposes: a single key used for both an AEAD
//! and a MAC is how protocols get broken by people cleverer than us.

use hkdf::Hkdf;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::keys::{Key32, Muk};

/// Subkeys derived directly from the MUK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subkey {
    /// Unlock verifier. Compared in constant time, before any decryption.
    Verify,
    /// HMAC key over the canonical header.
    Header,
    /// Wraps the account private key bundle.
    Wrap,
    /// Wraps every vault key.
    Vault,
    /// Encrypts the HIBP prefix cache and the generator history.
    AppCache,
}

impl Subkey {
    /// The HKDF `info` string for this purpose.
    #[must_use]
    pub const fn info(self) -> &'static [u8] {
        match self {
            Self::Verify => b"keyring/v1/muk/verify",
            Self::Header => b"keyring/v1/muk/header",
            Self::Wrap => b"keyring/v1/muk/wrap",
            Self::Vault => b"keyring/v1/muk/vault",
            Self::AppCache => b"keyring/v1/muk/appcache",
        }
    }
}

/// Subkeys derived from an item key, one per envelope (SPEC-V1 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSubkey {
    /// Encrypts `meta_ct` — decrypted for every item at unlock.
    Meta,
    /// Encrypts `secret_ct` — decrypted one field at a time, on demand.
    Secret,
}

impl ItemSubkey {
    /// The HKDF `info` string for this purpose.
    #[must_use]
    pub const fn info(self) -> &'static [u8] {
        match self {
            Self::Meta => b"keyring/v1/item/meta",
            Self::Secret => b"keyring/v1/item/secret",
        }
    }
}

/// The HKDF `info` string for a vault's activity subkey.
pub const INFO_VAULT_ACTIVITY: &[u8] = b"keyring/v1/vault/activity";

/// Expand `ikm` into a 32-byte subkey under `info`.
///
/// HKDF-Extract runs with no salt: the input keying material is already a
/// uniformly random 32-byte key, so the extract step has nothing to do and the
/// separation comes entirely from `info`.
fn expand(ikm: &[u8; 32], info: &[u8]) -> Key32 {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut okm = Zeroizing::new([0u8; 32]);
    match hk.expand(info, okm.as_mut()) {
        Ok(()) => Key32::from_bytes(*okm),
        // Statically unreachable: HKDF-Expand rejects only output lengths above
        // 255 × 32 bytes, and this one is 32. There is no safe value to return
        // here — a fixed fallback would be a predictable key — so we stop the
        // process rather than continue with key material we did not derive.
        // `abort` and not `panic!`: no unwinding, no message, nothing to catch
        // (CLAUDE.md §4.10, fail closed).
        Err(_) => std::process::abort(),
    }
}

/// Derive a MUK subkey.
#[must_use]
pub fn derive_subkey(muk: &Muk, which: Subkey) -> Key32 {
    expand(muk.expose(), which.info())
}

/// Derive one of an item key's two envelope subkeys.
#[must_use]
pub fn derive_item_subkey(item_key: &Key32, which: ItemSubkey) -> Key32 {
    expand(item_key.expose(), which.info())
}

/// Derive a vault's activity subkey (SPEC-V1 §4.3).
#[must_use]
pub fn derive_activity_subkey(vault_key: &Key32) -> Key32 {
    expand(vault_key.expose(), INFO_VAULT_ACTIVITY)
}

/// The value stored in `header.verifier`.
#[must_use]
pub fn verifier_from(muk: &Muk) -> [u8; 32] {
    *derive_subkey(muk, Subkey::Verify).expose()
}

/// Check a candidate MUK against a stored verifier, in constant time.
///
/// The comparison must not short-circuit on the first differing byte, and the
/// return type must not encode *where* it differed. `tests/` times this against
/// two input classes that a short-circuiting comparison separates cleanly.
#[must_use]
pub fn verify_password(muk: &Muk, stored: &[u8; 32]) -> bool {
    let candidate = derive_subkey(muk, Subkey::Verify);
    candidate.expose().ct_eq(stored).into()
}
