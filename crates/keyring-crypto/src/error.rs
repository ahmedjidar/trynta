//! Redacting error type.
//!
//! CLAUDE.md §4.6 and §4.10: no secret ever reaches an error string, and any
//! error in a decrypt, verify or authorize path denies the operation. This type
//! is redacting *by construction* — it is `Copy`, every variant is either a bare
//! discriminant or carries a format version number, and there is no variant that
//! could hold a key, a plaintext or a ciphertext. An error type that *can* carry
//! a secret eventually will.

use thiserror::Error;

/// Every way a cryptographic operation can fail.
///
/// Deliberately coarse. Distinguishing "wrong key" from "corrupt ciphertext" is
/// information an attacker wants and a user cannot act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum CryptoError {
    /// Argon2id failed, or was asked for parameters it cannot satisfy.
    #[error("key derivation failed")]
    KeyDerivation,

    /// KDF cost parameters outside the range SPEC-V1 §3.2 permits.
    #[error("kdf parameters are outside the permitted range")]
    InvalidKdfParams,

    /// AEAD authentication failed: wrong key, tampered ciphertext, wrong
    /// associated data, or a replayed nonce. One error for all of them.
    #[error("authentication failed")]
    Authentication,

    /// A stored envelope is too short or structurally invalid.
    #[error("malformed envelope")]
    MalformedEnvelope,

    /// The envelope was written by a format version this build does not know.
    /// Never a best-effort parse (SPEC-V1 §3.3).
    #[error("this vault was written by a newer version of Keyring (envelope format {found}, this build supports {supported})")]
    UnsupportedEnvelopeVersion {
        /// The version found on disk.
        found: u16,
        /// The version this build understands.
        supported: u16,
    },

    /// Padding did not follow ISO/IEC 7816-4 after a successful decryption,
    /// which means the plaintext is not what we wrote.
    #[error("malformed padding")]
    MalformedPadding,

    /// The manifest signature did not verify against the account public key.
    #[error("manifest signature verification failed")]
    BadSignature,

    /// The header MAC did not verify. The header has been modified, or the
    /// password is wrong — and we do not say which.
    #[error("header authentication failed")]
    BadHeaderMac,

    /// A key or signature had the wrong length for its type.
    #[error("invalid key or signature length")]
    InvalidLength,

    /// The operating system's random number generator failed. Fail closed; there
    /// is no fallback source of randomness in this product.
    #[error("the operating system random number generator failed")]
    Rng,
}
