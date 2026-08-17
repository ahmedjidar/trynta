//! Platform abstraction: biometrics, clipboard, secure storage.
//!
//! CLAUDE.md §6: anything platform-specific lives behind a trait here with a
//! `macos` and a `windows` implementation, and no `#[cfg]` is scattered through
//! business logic. There is exactly one `#[cfg]` fork in this file — the one
//! that picks an implementation — and callers above never see it.
//!
//! This is also the only module in the crate permitted to use `unsafe`. The
//! crate is `#![deny(unsafe_code)]` and this module carries a scoped
//! `#[allow]`, so `unsafe` outside here is a compile error and `scripts/
//! check-unsafe.mjs` fails the build if the allow spreads.

pub mod biometric;
pub mod clipboard;
pub mod paths;
pub mod secure_store;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

use std::sync::Arc;

pub use biometric::{BiometricError, BiometricKind, Biometrics};
pub use clipboard::{Clipboard, ClipboardError};
pub use secure_store::{SecureStore, SecureStoreError};

/// Everything the application needs from the operating system.
///
/// Bundled into one struct so a caller takes a single dependency and a test can
/// substitute all three at once.
pub struct Platform {
    /// Touch ID or Windows Hello.
    pub biometrics: Arc<dyn Biometrics>,
    /// The system clipboard, with the per-platform secrecy markers applied.
    pub clipboard: Arc<dyn Clipboard>,
    /// Keychain or DPAPI-backed storage for the biometric key wrap.
    pub secure_store: Arc<dyn SecureStore>,
}

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("os", &current_os())
            .finish_non_exhaustive()
    }
}

impl Platform {
    /// The real implementations for the host this build is running on.
    #[must_use]
    pub fn host() -> Self {
        #[cfg(windows)]
        {
            Self {
                biometrics: Arc::new(windows::hello::WindowsHello::new()),
                clipboard: Arc::new(windows::clipboard::WindowsClipboard::new()),
                secure_store: Arc::new(windows::dpapi::DpapiStore::new()),
            }
        }
        #[cfg(target_os = "macos")]
        {
            Self {
                biometrics: Arc::new(macos::touch_id::TouchId::new()),
                clipboard: Arc::new(macos::clipboard::MacClipboard::new()),
                secure_store: Arc::new(macos::keychain::KeychainStore::new()),
            }
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            // Keyring ships macOS and Windows only (SPEC-V1 §8). This exists so
            // the workspace still builds on a Linux CI runner for the
            // supply-chain job, and every call fails closed.
            Self {
                biometrics: Arc::new(unsupported::UnsupportedPlatform),
                clipboard: Arc::new(unsupported::UnsupportedPlatform),
                secure_store: Arc::new(unsupported::UnsupportedPlatform),
            }
        }
    }
}

/// Short identifier for the host OS, for diagnostics and the keyboard key-map.
#[must_use]
pub const fn current_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unsupported"
    }
}

/// The modifier key this platform labels shortcuts with.
///
/// SPEC-V1 §8: never a literal `⌘` in source; every keyboard hint resolves
/// through here.
#[must_use]
pub const fn modifier_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod unsupported {
    use super::{
        BiometricError, BiometricKind, Biometrics, Clipboard, ClipboardError, SecureStore,
        SecureStoreError,
    };

    /// Fails every call rather than pretending to work.
    pub struct UnsupportedPlatform;

    impl Biometrics for UnsupportedPlatform {
        fn kind(&self) -> BiometricKind {
            BiometricKind::None
        }
        fn is_available(&self) -> bool {
            false
        }
        fn enrol(&self, _label: &str, _secret: &[u8]) -> Result<(), BiometricError> {
            Err(BiometricError::Unavailable)
        }
        fn unwrap_secret(&self, _label: &str) -> Result<Vec<u8>, BiometricError> {
            Err(BiometricError::Unavailable)
        }
        fn revoke(&self, _label: &str) -> Result<(), BiometricError> {
            Err(BiometricError::Unavailable)
        }
    }

    impl Clipboard for UnsupportedPlatform {
        fn set_secret(&self, _value: &str) -> Result<u64, ClipboardError> {
            Err(ClipboardError::Unavailable)
        }
        fn clear_if_ours(&self, _token: u64) -> Result<bool, ClipboardError> {
            Err(ClipboardError::Unavailable)
        }
    }

    impl SecureStore for UnsupportedPlatform {
        fn store(&self, _key: &str, _value: &[u8]) -> Result<(), SecureStoreError> {
            Err(SecureStoreError::Unavailable)
        }
        fn load(&self, _key: &str) -> Result<Option<Vec<u8>>, SecureStoreError> {
            Err(SecureStoreError::Unavailable)
        }
        fn delete(&self, _key: &str) -> Result<(), SecureStoreError> {
            Err(SecureStoreError::Unavailable)
        }
    }
}
