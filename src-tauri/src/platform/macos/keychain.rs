//! Keychain-backed secure store (SPEC-V1 §8).
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

/// The macOS Keychain.
pub struct KeychainStore;

impl KeychainStore {
    /// A handle to the login keychain.
    #[must_use]
    pub const fn new() -> Self {
        Self
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
        let options = PasswordOptions::new_generic_password(SERVICE, key);
        set_generic_password_options(value, options).map_err(|_| SecureStoreError::Platform)
    }

    fn load(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStoreError> {
        match get_generic_password(SERVICE, key) {
            Ok(value) => Ok(Some(value)),
            // A missing item is not an error; anything else is unreadable rather
            // than fatal, so the caller falls back rather than failing hard.
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(_) => Err(SecureStoreError::Unreadable),
        }
    }

    fn delete(&self, key: &str) -> Result<(), SecureStoreError> {
        match delete_generic_password(SERVICE, key) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(_) => Err(SecureStoreError::Platform),
        }
    }
}

/// `errSecItemNotFound`.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
