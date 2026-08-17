//! Keychain-backed secure store (SPEC-V1 §8).
//!
//! **UNVERIFIED PLATFORM (ADD-005).** Never compiled. Every signature below was
//! read out of `security-framework 3.7.0`'s source rather than recalled — see
//! `MACOS-UNVERIFIED.md` for what still has to be checked on hardware.
//!
//! The counterpart to DPAPI on Windows. Items are `ThisDeviceOnly`, so copying
//! `~/Library/Application Support/Keyring` to another Mac does not carry
//! anything stored here with it.
//!
//! This is *not* where the biometric wrap lives — that goes through
//! [`crate::platform::macos::touch_id`], which attaches a biometry access
//! control the plain store deliberately does not. This store is for values that
//! need to survive without a biometric prompt.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password_options,
};
use security_framework::passwords_options::PasswordOptions;

use crate::platform::secure_store::{SecureStore, SecureStoreError};

/// Keychain service name for non-biometric Keyring items.
const SERVICE: &str = "app.keyring.desktop";

/// `errSecItemNotFound`.
///
/// Verified against `security_framework_sys::base::errSecItemNotFound`, which is
/// `-25300`. Written out rather than imported because `security-framework` does
/// not re-export the `-sys` crate and pulling `security-framework-sys` in as a
/// direct dependency for one integer is not worth the extra surface.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// The macOS Keychain.
pub struct KeychainStore {
    /// Keychain service name. Overridable so tests do not write into the real
    /// login-keychain entries — the macOS counterpart to `DpapiStore::with_root`.
    service: &'static str,
}

impl KeychainStore {
    /// A handle to the login keychain.
    #[must_use]
    pub const fn new() -> Self {
        Self { service: SERVICE }
    }

    /// A handle scoped to a different service name, for tests.
    ///
    /// The Keychain is one global store with no equivalent of a temporary
    /// directory, so isolating a test means using a service name the real app
    /// never writes to. Taking `&'static str` keeps that to compile-time
    /// constants rather than something assembled at runtime.
    #[must_use]
    pub const fn with_service(service: &'static str) -> Self {
        Self { service }
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecureStore for KeychainStore {
    fn store(&self, key: &str, value: &[u8]) -> Result<(), SecureStoreError> {
        // Replace rather than add: `SecItemAdd` on an existing item is an error,
        // and the caller's intent is always "this is the current value".
        let _ = self.delete(key);
        let options = PasswordOptions::new_generic_password(self.service, key);
        // `set_generic_password_options(password, options)` — password first. Easy
        // to transpose and it would compile either way only if both were `&[u8]`,
        // which they are not, so a transposition is a type error rather than a
        // silent bug.
        set_generic_password_options(value, options).map_err(|_| SecureStoreError::Platform)
    }

    fn load(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStoreError> {
        match get_generic_password(self.service, key) {
            Ok(value) => Ok(Some(value)),
            // A missing item is not an error; anything else is unreadable rather
            // than fatal, so the caller falls back rather than failing hard.
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(_) => Err(SecureStoreError::Unreadable),
        }
    }

    fn delete(&self, key: &str) -> Result<(), SecureStoreError> {
        match delete_generic_password(self.service, key) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(_) => Err(SecureStoreError::Platform),
        }
    }
}
