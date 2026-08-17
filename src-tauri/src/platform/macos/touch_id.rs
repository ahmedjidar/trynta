//! Touch ID, via the Keychain's biometry access control (SPEC-V1 §5.1, §8).
//!
//! macOS reaches the same guarantee as Windows by a different route, and the
//! difference is worth stating because it is why [`Biometrics::enrol`] returns
//! nothing to hold:
//!
//! - **Windows** derives a wrapping key from a Hello-gated TPM signature and we
//!   store the resulting ciphertext ourselves.
//! - **macOS** hands the secret to the Keychain under an access control of
//!   `kSecAccessControlBiometryCurrentSet`. The Secure Enclave holds the key,
//!   the Keychain holds the ciphertext, and reading the item is what prompts.
//!
//! `BiometryCurrentSet` rather than `BiometryAny` is the whole point: the item
//! is destroyed the moment the enrolled fingerprint set changes. That is
//! precisely the invalidation SPEC-V1 §5.1 says to rely on rather than
//! reimplement — adding a finger must not silently keep working.
//!
//! Paired with `AccessibleWhenPasscodeSetThisDeviceOnly`, the item also never
//! leaves this Mac and cannot exist at all on a machine with no passcode.

use objc2_local_authentication::{LAContext, LAPolicy};
use security_framework::access_control::{ProtectionMode, SecAccessControl};
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password_options,
};
use security_framework::passwords_options::PasswordOptions;

use crate::platform::biometric::{BiometricError, BiometricKind, Biometrics};

/// Keychain service name for every Keyring biometric item.
const SERVICE: &str = "app.keyring.desktop.biometric";

/// `kSecAccessControlBiometryCurrentSet` — invalidated when enrolment changes.
const BIOMETRY_CURRENT_SET: u32 = 1 << 3;

/// Touch ID biometrics.
pub struct TouchId;

impl TouchId {
    /// A handle to the platform's LocalAuthentication service.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Access control requiring the *current* biometric set, on this device
    /// only, and only when a passcode is set.
    fn access_control() -> Result<SecAccessControl, BiometricError> {
        SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
            BIOMETRY_CURRENT_SET.into(),
        )
        .map_err(|_| BiometricError::Platform)
    }
}

impl Default for TouchId {
    fn default() -> Self {
        Self::new()
    }
}

impl Biometrics for TouchId {
    fn kind(&self) -> BiometricKind {
        BiometricKind::TouchId
    }

    fn is_available(&self) -> bool {
        // SAFETY: `LAContext::new` allocates and initialises a context with no
        // preconditions; `canEvaluatePolicy_error` takes a policy and an
        // optional out-error pointer, and we pass none. Neither borrows beyond
        // the call, and the returned object is managed by `Retained`.
        unsafe {
            let context = LAContext::new();
            context.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
        }
        .is_ok()
    }

    fn enrol(&self, label: &str, secret: &[u8]) -> Result<(), BiometricError> {
        // Replacing rather than adding: an existing item under this label is a
        // stale wrap from a previous enrolment and must not survive.
        let _ = self.revoke(label);

        let mut options = PasswordOptions::new_generic_password(SERVICE, label);
        options.set_access_control(Self::access_control()?);
        set_generic_password_options(secret, options).map_err(|_| BiometricError::Platform)
    }

    fn unwrap_secret(&self, label: &str) -> Result<Vec<u8>, BiometricError> {
        // Reading the item is what raises the Touch ID prompt, because of the
        // access control attached at enrolment.
        get_generic_password(SERVICE, label).map_err(|e| {
            // `errSecUserCanceled` is the user dismissing the prompt; anything
            // else on a read of an existing item means the item is gone, which
            // is what an enrolment change looks like from here.
            const ERR_SEC_USER_CANCELED: i32 = -128;
            if e.code() == ERR_SEC_USER_CANCELED {
                BiometricError::Cancelled
            } else {
                BiometricError::Invalidated
            }
        })
    }

    fn revoke(&self, label: &str) -> Result<(), BiometricError> {
        match delete_generic_password(SERVICE, label) {
            Ok(()) => Ok(()),
            // Deleting an item that is not there is success, not failure.
            Err(_) => Ok(()),
        }
    }
}
