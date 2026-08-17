//! Biometric unlock (SPEC-V1 §5.1).
//!
//! The shape is the same on both platforms and it matters that it is:
//!
//! 1. The OS holds a device-bound key that only a successful biometric check
//!    releases — Secure Enclave or TPM where the hardware provides one.
//! 2. That key wraps a copy of the MUK. We store the *wrapped* copy; the OS
//!    stores the key.
//! 3. An enrolment change invalidates the OS key, so the wrap becomes
//!    undecryptable and the user falls back to the master password.
//!
//! Step 3 is the one that is easy to get wrong by reimplementing it. SPEC-V1
//! §5.1 is explicit: *rely on the OS invalidating the item, don't reimplement
//! it.* Both platforms give us that for free — macOS through
//! `kSecAccessControlBiometryCurrentSet`, Windows because a `KeyCredential` is
//! destroyed when Hello credentials change. Our job is to notice the failure and
//! fall back, not to track enrolment ourselves.
//!
//! Biometric unlock is **persistent** across app and OS restart (ADD-003 §③
//! confirmed option (b)): forcing a long master password several times a day
//! pushes people toward short master passwords, which loses more than it gains.

use thiserror::Error;

/// What the host offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiometricKind {
    /// macOS Touch ID.
    TouchId,
    /// Windows Hello.
    WindowsHello,
    /// No biometric hardware, or none enrolled.
    None,
}

impl BiometricKind {
    /// A label for the UI. Never a literal platform name in a component.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TouchId => "Touch ID",
            Self::WindowsHello => "Windows Hello",
            Self::None => "biometric unlock",
        }
    }
}

/// Why a biometric operation did not produce a secret.
///
/// Redacting by construction: no variant can carry key material, and the
/// platform's own error text is dropped at the boundary rather than propagated,
/// because it can name file paths and credential identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum BiometricError {
    /// No biometric hardware, or nothing enrolled.
    #[error("biometric unlock is not available on this device")]
    Unavailable,

    /// The user dismissed the prompt or failed the check.
    #[error("biometric verification was cancelled")]
    Cancelled,

    /// The stored wrap is gone or no longer decryptable — almost always because
    /// enrolment changed. The caller must fall back to the master password.
    #[error("biometric enrolment changed; the master password is required")]
    Invalidated,

    /// The platform API failed for a reason we deliberately do not relay.
    #[error("the platform biometric service failed")]
    Platform,
}

/// Wrap and unwrap a secret behind a biometric check.
///
/// Implementations must never log, persist or return the plaintext secret
/// anywhere but the return value of [`Biometrics::unwrap_secret`].
pub trait Biometrics: Send + Sync {
    /// What this host offers.
    fn kind(&self) -> BiometricKind;

    /// Whether a biometric unlock could succeed right now.
    ///
    /// Cheap and non-prompting: safe to call to decide whether to render the
    /// affordance at all.
    fn is_available(&self) -> bool;

    /// Protect `secret` under `label` behind a biometric check.
    ///
    /// Prompts. The platform owns where the protected form lives, because the
    /// two platforms differ in a way a shared "return the wrapped bytes" shape
    /// would obscure: Windows derives a key from a Hello-gated TPM signature and
    /// stores the ciphertext under DPAPI, while macOS hands the secret to the
    /// Keychain under a biometry access-control. Both give the same guarantee;
    /// only one of them has bytes for us to hold.
    ///
    /// # Errors
    ///
    /// [`BiometricError::Unavailable`] if there is no biometric,
    /// [`BiometricError::Cancelled`] if the user declines,
    /// [`BiometricError::Platform`] otherwise.
    fn enrol(&self, label: &str, secret: &[u8]) -> Result<(), BiometricError>;

    /// Retrieve a secret previously stored by [`Biometrics::enrol`].
    ///
    /// Prompts.
    ///
    /// # Errors
    ///
    /// [`BiometricError::Invalidated`] when the OS key is gone, which is the
    /// enrolment-changed path and must send the caller to the master password.
    fn unwrap_secret(&self, label: &str) -> Result<Vec<u8>, BiometricError>;

    /// Destroy the device-bound key for `label`.
    ///
    /// # Errors
    ///
    /// [`BiometricError::Platform`] if the platform refuses.
    fn revoke(&self, label: &str) -> Result<(), BiometricError>;
}

/// How long a biometric unlock stays valid without a master-password unlock.
///
/// SPEC-V1 §5.1: the master password is required again after 14 days regardless
/// of biometric success, so a device that is never re-authenticated does not
/// drift indefinitely from the thing that actually protects the vault.
pub const REAUTH_INTERVAL_DAYS: i64 = 14;

/// Milliseconds in [`REAUTH_INTERVAL_DAYS`].
#[must_use]
pub const fn reauth_interval_ms() -> i64 {
    REAUTH_INTERVAL_DAYS * 86_400_000
}

/// Whether a master-password unlock is due, given when one last happened.
///
/// `None` means "never", and takes an `Option` rather than a zero sentinel
/// deliberately: zero is a real instant, and a sentinel that collides with a
/// valid value is a bug waiting for the one caller whose clock starts there.
#[must_use]
pub const fn password_unlock_due(now_ms: i64, last_password_unlock_ms: Option<i64>) -> bool {
    match last_password_unlock_ms {
        None => true,
        Some(last) => now_ms.saturating_sub(last) >= reauth_interval_ms(),
    }
}
