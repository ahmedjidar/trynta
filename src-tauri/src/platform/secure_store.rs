// SPDX-License-Identifier: AGPL-3.0-or-later
//! OS-backed storage for the biometric key wrap (SPEC-V1 §8).
//!
//! Keychain on macOS, DPAPI on Windows. This holds the *wrapped* MUK — never
//! the MUK itself, and never anything from a vault. The wrap is only useful to
//! someone who can also pass the biometric check, which is the whole point of
//! keeping the two in different places.

use thiserror::Error;

/// Why a secure-store operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum SecureStoreError {
    /// No secure store on this host.
    #[error("no platform secure store is available")]
    Unavailable,

    /// The entry exists but could not be decrypted — on Windows this is what a
    /// different user or machine looks like.
    #[error("the stored item could not be read")]
    Unreadable,

    /// The platform refused.
    #[error("the platform secure store failed")]
    Platform,
}

/// Store, load and delete small opaque blobs in the OS secure store.
pub trait SecureStore: Send + Sync {
    /// Store `value` under `key`, replacing any existing entry.
    ///
    /// # Errors
    ///
    /// [`SecureStoreError::Platform`] if the platform refuses.
    fn store(&self, key: &str, value: &[u8]) -> Result<(), SecureStoreError>;

    /// Load the entry for `key`, if present.
    ///
    /// # Errors
    ///
    /// [`SecureStoreError::Unreadable`] if the entry exists but cannot be
    /// decrypted, which the caller must treat as "fall back to the master
    /// password" rather than as a hard failure.
    fn load(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStoreError>;

    /// Delete the entry for `key`. Deleting a missing entry succeeds.
    ///
    /// # Errors
    ///
    /// [`SecureStoreError::Platform`] if the platform refuses.
    fn delete(&self, key: &str) -> Result<(), SecureStoreError>;
}
