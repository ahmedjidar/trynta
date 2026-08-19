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

use crate::commands::dto::TotpRejectionDto;
use crate::session::SessionError;

/// An error crossing the IPC boundary.
///
/// Generated into TypeScript with everything else that crosses IPC, so the
/// frontend matches on a closed set of discriminants rather than a hand-written
/// union that drifts the first time a variant is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
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
        ///
        /// Typed as a TS `number`, not `bigint`: Tauri's IPC is JSON, so this
        /// arrives as a double no matter what ts-rs infers from `u64`. A
        /// `bigint` annotation would typecheck and then fail at runtime.
        #[ts(type = "number")]
        retry_in_seconds: u64,
    },
    /// The vault file has been modified. Refuse, never partial-open.
    TamperDetected,
    /// The requested item does not exist.
    NotFound,
    /// The requested field does not exist on this item.
    NoSuchField,
    /// The reveal rate limit was reached (SPEC-V1 §6).
    ///
    /// Not a rejection: the caller re-authenticates and tries again. The reveal
    /// that hit the limit did not happen, and no plaintext was decrypted.
    ReauthRequired,
    /// The last remaining vault cannot be deleted.
    LastVaultRemaining,
    /// The clipboard could not be written or cleared.
    Clipboard,
    /// The application data directory could not be resolved or created.
    DataDirectory,
    /// A feature's bundled data is not present in this build.
    ///
    /// Today: the EFF wordlist, whose licence THIRD-PARTY-NOTICES.md still
    /// records as unconfirmed. Reported rather than worked around, because the
    /// alternative is generating passphrases from a short list while claiming the
    /// entropy of a complete one.
    FeatureUnavailable,
    /// A storage operation failed. No detail crosses the boundary.
    Storage,
    /// A cryptographic operation failed. Fails closed; no detail, ever.
    Crypto,
    /// A biometric operation failed or was declined.
    Biometric,
    /// An update could not be downloaded, verified or applied (SPEC-V1 §7.7).
    ///
    /// Deliberately one discriminant for every cause. A signature that did not
    /// verify, a truncated download and an unreachable endpoint are all the same
    /// message to the user — "the update did not install" — and distinguishing
    /// them in the UI would tell an attacker probing the channel which of their
    /// tampering attempts got furthest.
    UpdateFailed,
    /// The input did not validate.
    Invalid,
    /// A one-time-code setup was refused, and which rule it broke.
    ///
    /// Separate from [`Self::Invalid`] because the value being validated is a
    /// shared secret: the reason is a closed enum so the UI can be specific
    /// without the input ever reaching an error string (SPEC-V1 §4.1).
    TotpRejected {
        /// Which rule the input broke.
        reason: TotpRejectionDto,
    },
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
            Self::ReauthRequired => "confirm your master password to keep revealing secrets",
            Self::UpdateFailed => "the update could not be installed",
            Self::LastVaultRemaining => "the last vault cannot be deleted",
            Self::Clipboard => "the clipboard is unavailable",
            Self::DataDirectory => "Keyring could not find a place to store your vault",
            Self::FeatureUnavailable => "that feature is not available in this build",
            Self::Storage => "a storage operation failed",
            Self::Crypto => "a cryptographic operation failed",
            Self::Biometric => "biometric unlock is unavailable",
            Self::Invalid => "the input is not valid",
            Self::TotpRejected { .. } => "that one-time-code setup could not be read",
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
            StoreError::LastVault => Self::LastVaultRemaining,
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

impl From<crate::platform::ClipboardError> for AppError {
    fn from(_: crate::platform::ClipboardError) -> Self {
        Self::Clipboard
    }
}

impl From<crate::services::generator::GeneratorError> for AppError {
    fn from(e: crate::services::generator::GeneratorError) -> Self {
        use crate::services::generator::GeneratorError as G;
        match e {
            // No fallback for a degraded randomness source (SPEC-V1 §3.2).
            G::Rng | G::Exhausted => Self::Crypto,
            G::NoWordList => Self::FeatureUnavailable,
        }
    }
}

impl From<crate::platform::paths::PathError> for AppError {
    fn from(_: crate::platform::paths::PathError) -> Self {
        Self::DataDirectory
    }
}

impl From<keyring_crypto::CryptoError> for AppError {
    fn from(_: keyring_crypto::CryptoError) -> Self {
        Self::Crypto
    }
}
