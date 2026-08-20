// SPDX-License-Identifier: AGPL-3.0-or-later
//! Key material types.
//!
//! Every type here holds secret bytes and every one of them has a hand-written
//! redacting `Debug`. The derived `Debug` is the single most common way a key
//! ends up in a log line, and `#[derive(Debug)]` on a struct with a `[u8; 32]`
//! field is one keystroke away at all times. `tests/redaction.rs` asserts none of
//! these ever print their contents, in debug *and* release.

use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use x25519_dalek::{PublicKey as XPublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::error::CryptoError;
use crate::rng;

/// A 32-byte symmetric key. Zeroized on drop.
#[derive(Clone)]
pub struct Key32(Zeroizing<[u8; 32]>);

impl Key32 {
    /// Wrap existing bytes as a key.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Generate a fresh key from the OS CSPRNG.
    ///
    /// # Errors
    ///
    /// [`CryptoError::Rng`] if the OS generator is unavailable.
    pub fn random() -> Result<Self, CryptoError> {
        Ok(Self(Zeroizing::new(rng::array::<32>()?)))
    }

    /// Borrow the raw bytes.
    ///
    /// Named `expose` rather than `as_bytes` so that every use site reads as a
    /// deliberate act at review time.
    #[must_use]
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Key32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key32(<redacted>)")
    }
}

impl fmt::Display for Key32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// The Master Unlock Key: Argon2id output over the master password and the
/// account salt. Never persisted, never leaves Rust (CLAUDE.md §4.2).
#[derive(Clone)]
pub struct Muk(Key32);

impl Muk {
    /// Wrap a derived key as the MUK.
    #[must_use]
    pub fn from_key32(key: Key32) -> Self {
        Self(key)
    }

    /// Borrow the raw bytes.
    #[must_use]
    pub fn expose(&self) -> &[u8; 32] {
        self.0.expose()
    }

    /// Borrow as a [`Key32`], for subkey derivation.
    #[must_use]
    pub fn as_key(&self) -> &Key32 {
        &self.0
    }
}

impl fmt::Debug for Muk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Muk(<redacted>)")
    }
}

impl fmt::Display for Muk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// The public halves of the account key bundle. Not secret: stored in the
/// header in the clear, and bound to the master password by the header MAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountPublicKeys {
    /// X25519 public key, for key agreement (consumed by SPEC-V2).
    pub x25519: [u8; 32],
    /// Ed25519 verifying key, for the vault manifest signature.
    pub ed25519: [u8; 32],
}

/// The account private key bundle: X25519 for key agreement, Ed25519 for
/// signatures.
///
/// Generated at vault creation even though only the Ed25519 half is consumed in
/// V1 (by the manifest). Retrofitting identity keys onto existing vaults later
/// is a migration nobody wants to write (SPEC-V1 §1).
pub struct AccountKeys {
    x25519: StaticSecret,
    ed25519: SigningKey,
}

/// Serialized length of an [`AccountKeys`] bundle: two 32-byte scalars.
pub const ACCOUNT_KEYS_LEN: usize = 64;

impl AccountKeys {
    /// Generate a fresh bundle from the OS CSPRNG.
    ///
    /// # Errors
    ///
    /// [`CryptoError::Rng`] if the OS generator is unavailable.
    pub fn generate() -> Result<Self, CryptoError> {
        Ok(Self {
            x25519: StaticSecret::from(rng::array::<32>()?),
            ed25519: SigningKey::from_bytes(&rng::array::<32>()?),
        })
    }

    /// The public halves, for storage in the header.
    #[must_use]
    pub fn public(&self) -> AccountPublicKeys {
        AccountPublicKeys {
            x25519: XPublicKey::from(&self.x25519).to_bytes(),
            ed25519: self.ed25519.verifying_key().to_bytes(),
        }
    }

    /// Serialize for sealing under `muk.wrap`. The buffer zeroizes on drop.
    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<[u8; ACCOUNT_KEYS_LEN]> {
        let mut out = Zeroizing::new([0u8; ACCOUNT_KEYS_LEN]);
        out[..32].copy_from_slice(&self.x25519.to_bytes());
        out[32..].copy_from_slice(&self.ed25519.to_bytes());
        out
    }

    /// Reconstruct from the sealed form.
    ///
    /// # Errors
    ///
    /// [`CryptoError::InvalidLength`] if `bytes` is not [`ACCOUNT_KEYS_LEN`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != ACCOUNT_KEYS_LEN {
            return Err(CryptoError::InvalidLength);
        }
        let mut x = [0u8; 32];
        let mut e = [0u8; 32];
        x.copy_from_slice(&bytes[..32]);
        e.copy_from_slice(&bytes[32..]);
        let keys = Self {
            x25519: StaticSecret::from(x),
            ed25519: SigningKey::from_bytes(&e),
        };
        // The scratch copies are ours to clear; the constructed keys own theirs.
        zeroize::Zeroize::zeroize(&mut x);
        zeroize::Zeroize::zeroize(&mut e);
        Ok(keys)
    }

    /// Sign a message with the account Ed25519 key.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.ed25519.sign(message).to_bytes()
    }
}

impl fmt::Debug for AccountKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccountKeys(<redacted>)")
    }
}

/// Verify an Ed25519 signature against a raw public key.
///
/// # Errors
///
/// [`CryptoError::BadSignature`] if the key is not a valid point or the
/// signature does not verify. Fails closed on both.
pub fn verify_ed25519(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    let vk = VerifyingKey::from_bytes(public_key).map_err(|_| CryptoError::BadSignature)?;
    vk.verify(message, &Signature::from_bytes(signature))
        .map_err(|_| CryptoError::BadSignature)
}
