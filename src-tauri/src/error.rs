//! Redacting error types for the application shell.
//!
//! CLAUDE.md §4.6: no secret ever reaches a log, a panic message, a `Debug` impl
//! or an error string. These types are redacting *by construction* — none of them
//! can carry a plaintext payload, because none of them has a field that could.

use std::fmt;

/// An error crossing the IPC boundary.
///
/// Deliberately carries no data beyond a discriminant and a static description.
/// An error type that *could* hold a secret eventually will.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
#[non_exhaustive]
pub enum AppError {
    /// The vault is locked; the operation requires an unlocked session.
    Locked,
    /// The operation is not valid in the current state.
    InvalidState,
    /// A storage operation failed. No detail crosses the boundary.
    Storage,
    /// A cryptographic operation failed. Fails closed; no detail, ever.
    Crypto,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Locked => "the vault is locked",
            Self::InvalidState => "the operation is not valid in the current state",
            Self::Storage => "a storage operation failed",
            Self::Crypto => "a cryptographic operation failed",
        };
        f.write_str(s)
    }
}

impl std::error::Error for AppError {}
