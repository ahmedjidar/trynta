//! Redacting error types for the application shell.
//!
//! CLAUDE.md §4.6: no secret ever reaches a log, a panic message, a `Debug` impl
//! or an error string. This type is redacting *by construction* — it is `Copy`,
//! every variant is a bare discriminant, and there is no variant that could hold
//! a title, a field value or a fragment of plaintext. An error type that *can*
//! carry a secret eventually will.
//!
//! It is also the single error type crossing IPC, so the frontend sees a closed
//! set of discriminants rather than a string it might render verbatim.

use std::fmt;

use keyring_store::{StoreError, UnlockError};

use crate::session::SessionError;

/// An error crossing the IPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
#[non_exhaustive]
pub enum AppError {
    /// The vault is locked; the operation requires an unlocked session.
    Locked,
    /// No vault has been created or opened yet.
    NoVault,
    /// The operation is not valid in the current state.
    InvalidState,
    /// The master password did not verify.
    ///
    /// Identical for every wrong password: the message must not encode *why*
    /// verification failed (SPEC-V1 §5.2).
    WrongPassword,
    /// Too many recent attempts. Carries the wait so the UI can show it —
    /// a duration is not a secret, and hiding it would only make the app look
    /// broken.
    Backoff {
        /// Seconds until the next attempt is accepted.
        retry_in_seconds: u64,
    },
    /// The vault file has been modified. Refuse, never partial-open.
    TamperDetected,
    /// The requested item does not exist.
    NotFound,
    /// The requested field does not exist on this item.
    NoSuchField,
    /// A storage operation failed. No detail crosses the boundary.
    Storage,
    /// A cryptographic operation failed. Fails closed; no detail, ever.
    Crypto,
    /// A biometric operation failed or was declined.
    Biometric,
    /// The input did not validate.
    Invalid,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Locked => "the vault is locked",
            Self::NoVault => "no vault is open",
            Self::InvalidState => "the operation is not valid in the current state",
            Self::WrongPassword => "incorrect master password",
            Self::Backoff { .. } => "too many attempts, try again later",
            Self::TamperDetected => "this vault file has been modified",
            Self::NotFound => "not found",
            Self::NoSuchField => "that field does not exist on this item",
            Self::Storage => "a storage operation failed",
            Self::Crypto => "a cryptographic operation failed",
            Self::Biometric => "biometric unlock is unavailable",
            Self::Invalid => "the input is not valid",
        };
        f.write_str(s)
    }
}

impl std::error::Error for AppError {}

impl From<SessionError> for AppError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::Locked => Self::Locked,
            SessionError::NoVault => Self::NoVault,
            SessionError::InvalidState(_) => Self::InvalidState,
        }
    }
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::ItemNotFound | StoreError::VaultNotFound => Self::NotFound,
            StoreError::NoSuchField => Self::NoSuchField,
            StoreError::Crypto => Self::Crypto,
            StoreError::Tampered(_) => Self::TamperDetected,
            // Everything else is a storage problem the user cannot act on, and
            // the distinctions are useful in a log, not in a dialog.
            _ => Self::Storage,
        }
    }
}

impl From<UnlockError> for AppError {
    fn from(e: UnlockError) -> Self {
        match e {
            UnlockError::WrongPassword => Self::WrongPassword,
            UnlockError::Backoff { retry_in } => Self::Backoff {
                retry_in_seconds: retry_in.as_secs(),
            },
            UnlockError::TamperDetected(_) => Self::TamperDetected,
            UnlockError::Store(inner) => inner.into(),
            // `UnlockError` is #[non_exhaustive]: a variant added later must
            // land somewhere safe rather than fail to compile into silence.
            _ => Self::Storage,
        }
    }
}

impl From<crate::platform::BiometricError> for AppError {
    fn from(_: crate::platform::BiometricError) -> Self {
        Self::Biometric
    }
}
