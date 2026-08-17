//! Redacting error types for the store.
//!
//! CLAUDE.md §4.6: no secret ever reaches an error string. `StoreError` carries
//! only discriminants and non-secret identifiers — never a title, never a field
//! value, never a fragment of plaintext or ciphertext.
//!
//! `rusqlite::Error` is deliberately *not* wrapped with `#[from]` on a variant
//! that keeps it: a `SQLite` error can quote the offending SQL, and our SQL has
//! bound parameters carrying ciphertext. It is converted to a discriminant at
//! the boundary and the detail is dropped.

use std::time::Duration;

use thiserror::Error;

/// What kind of tampering was detected. Reported to the user as "this file has
/// been modified"; the distinction exists for logs and tests, not for recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TamperKind {
    /// The header MAC did not verify: the header was rewritten, or the password
    /// is wrong. We do not say which.
    HeaderMac,
    /// The manifest signature did not verify against the account public key.
    ManifestSignature,
    /// The recomputed manifest root does not match the signed one: a row was
    /// added, removed, rolled back, or resurrected.
    ManifestRoot,
}

impl std::fmt::Display for TamperKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::HeaderMac => "header authentication failed",
            Self::ManifestSignature => "manifest signature verification failed",
            Self::ManifestRoot => "the set of items does not match the signed manifest",
        })
    }
}

/// Anything that can go wrong in the store.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum StoreError {
    /// A database operation failed. The underlying detail is dropped rather than
    /// wrapped: `SQLite` errors can quote SQL, and our SQL binds ciphertext.
    #[error("a database operation failed")]
    Database,

    /// The file is not a Keyring vault, or its header row is missing.
    #[error("this file is not a Keyring vault")]
    NotAVault,

    /// The vault was written by a newer version of Keyring.
    #[error("this vault was written by a newer version of Keyring (schema {found}, this build supports {supported})")]
    UnsupportedSchema {
        /// Version found on disk.
        found: u32,
        /// Newest version this build understands.
        supported: u32,
    },

    /// A cryptographic operation failed. Fails closed; no detail crosses.
    #[error("a cryptographic operation failed")]
    Crypto,

    /// A stored payload did not decode. Only ever reachable after successful
    /// authentication, so this means a format bug rather than an attack.
    #[error("a stored payload could not be decoded")]
    MalformedPayload,

    /// The requested item does not exist, or is soft-deleted.
    #[error("no such item")]
    ItemNotFound,

    /// The requested vault does not exist.
    #[error("no such vault")]
    VaultNotFound,

    /// The requested field is not present on this item type.
    #[error("that field does not exist on this item")]
    NoSuchField,

    /// A migration was supplied with a version that is not strictly increasing,
    /// or two migrations share a version.
    #[error("migration set is invalid: version {version} in phase {phase} is out of order or duplicated")]
    InvalidMigrationSet {
        /// The offending version.
        version: u32,
        /// 1 for schema, 2 for payload.
        phase: u8,
    },

    /// The vault file itself has been modified.
    #[error("{0}")]
    Tampered(TamperKind),
}

impl From<rusqlite::Error> for StoreError {
    fn from(_: rusqlite::Error) -> Self {
        // Deliberately lossy. See the module comment.
        Self::Database
    }
}

impl From<keyring_crypto::CryptoError> for StoreError {
    fn from(_: keyring_crypto::CryptoError) -> Self {
        Self::Crypto
    }
}

/// Why an unlock did not produce a session.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum UnlockError {
    /// The password did not verify.
    ///
    /// Identical for every wrong password: the message must not encode *why*
    /// verification failed (SPEC-V1 §5.2).
    #[error("incorrect master password")]
    WrongPassword,

    /// Too many recent failures; the next attempt is refused until the delay
    /// elapses (SPEC-V1 §3.6).
    #[error("too many attempts, try again later")]
    Backoff {
        /// How long until the next attempt is accepted.
        retry_in: Duration,
    },

    /// The vault file has been modified. Refuse to unlock; never partial-open,
    /// never "repair" (SPEC-V1 §3.5).
    #[error("this vault file has been modified: {0}")]
    TamperDetected(TamperKind),

    /// Something else went wrong.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<keyring_crypto::CryptoError> for UnlockError {
    fn from(_: keyring_crypto::CryptoError) -> Self {
        Self::Store(StoreError::Crypto)
    }
}

impl From<rusqlite::Error> for UnlockError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Store(StoreError::Database)
    }
}
