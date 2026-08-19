// SPDX-License-Identifier: AGPL-3.0-or-later
//! Touch ID, via the Keychain's biometry access control (SPEC-V1 §5.1, §8).
//!
//! **UNVERIFIED PLATFORM (ADD-005).** Never compiled. Signatures were read out of
//! `security-framework 3.7.0` and `objc2-local-authentication 0.3.2` rather than
//! recalled; `MACOS-UNVERIFIED.md` lists what only hardware can settle — and for
//! this file that is most of it, because the security property is *when the
//! Keychain destroys the item*, which no compile can tell you.
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
use security_framework::passwords_options::{AccessControlOptions, PasswordOptions};

use crate::platform::biometric::{BiometricError, BiometricKind, Biometrics};

/// Keychain service name for every Trynta biometric item.
const SERVICE: &str = "dev.trynta.desktop.biometric";

/// `errSecUserCanceled` — the user dismissed the Touch ID prompt.
///
// UNVERIFIED: `-128` is Apple's documented value (it predates SecBase.h as
// `userCanceledErr`), but unlike `errSecItemNotFound` it is **not** defined in
// `security-framework-sys 2.17.0`, so there is no in-tree source to check it
// against. Getting it wrong is not silent-but-harmless: a cancelled prompt would
// be reported as `Invalidated`, and the UI would tell the user their enrolment is
// gone when they simply hit Cancel. See MACOS-UNVERIFIED.md item B4.
const ERR_SEC_USER_CANCELED: i32 = -128;

/// Touch ID biometrics.
pub struct TouchId {
    /// Keychain service name; overridable for tests, as in [`super::keychain`].
    service: &'static str,
}

impl TouchId {
    /// A handle to the platform's `LocalAuthentication` service.
    #[must_use]
    pub const fn new() -> Self {
        Self { service: SERVICE }
    }

    /// A handle scoped to a different Keychain service name, for tests.
    #[must_use]
    pub const fn with_service(service: &'static str) -> Self {
        Self { service }
    }

    /// Access control requiring the *current* biometric set, on this device
    /// only, and only when a passcode is set.
    ///
    /// `AccessControlOptions::BIOMETRY_CURRENT_SET` is the crate's own bitflag
    /// (`kSecAccessControlBiometryCurrentSet`, `1 << 3`) rather than a literal we
    /// maintain. `create_with_protection` takes `Option<ProtectionMode>` and
    /// `CFOptionFlags`, which is `usize`; the sibling
    /// `PasswordOptions::set_access_control_options` is deliberately *not* used
    /// because it builds its access control with `create_with_flags`, which
    /// defaults the protection class to `kSecAttrAccessibleWhenUnlocked` and would
    /// silently drop both `WhenPasscodeSet` and `ThisDeviceOnly`.
    fn access_control() -> Result<SecAccessControl, BiometricError> {
        SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
            AccessControlOptions::BIOMETRY_CURRENT_SET.bits(),
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
        // SAFETY: `LAContext::new` is generated as `pub unsafe fn new() ->
        // Retained<Self>`; it is `unsafe` only because objc2 marks every generated
        // `new`/`init` so, and `LAContext` is not a main-thread-only class, so
        // there is no thread precondition to violate. `canEvaluatePolicy_error`
        // takes only the policy — the bindings turn the `NSError**` out-parameter
        // into the `Result`, so there is no pointer for us to get wrong. Neither
        // call borrows beyond it and the context is owned by `Retained`.
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

        let mut options = PasswordOptions::new_generic_password(self.service, label);
        options.set_access_control(Self::access_control()?);
        set_generic_password_options(secret, options).map_err(|_| BiometricError::Platform)
    }

    fn unwrap_secret(&self, label: &str) -> Result<Vec<u8>, BiometricError> {
        // Reading the item is what raises the Touch ID prompt, because of the
        // access control attached at enrolment.
        get_generic_password(self.service, label).map_err(|e| {
            // Cancel is the user dismissing the prompt; anything else on a read of
            // an item we believe exists means the item is gone, which is what an
            // enrolment change looks like from here.
            if e.code() == ERR_SEC_USER_CANCELED {
                BiometricError::Cancelled
            } else {
                BiometricError::Invalidated
            }
        })
    }

    fn revoke(&self, label: &str) -> Result<(), BiometricError> {
        // Deleting an item that is not there is success, not failure: revoke is
        // called defensively before every enrol, and on a path where the item
        // has already been invalidated by an enrolment change.
        let _ = delete_generic_password(self.service, label);
        Ok(())
    }
}
