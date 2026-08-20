// SPDX-License-Identifier: AGPL-3.0-or-later
//! DPAPI-backed secure store (SPEC-V1 §8).
//!
//! `CryptProtectData` encrypts under a key derived from the user's login
//! credentials, so a blob written here is undecryptable by another user account
//! and on another machine. That is exactly the property we want for the
//! biometric wrap: copying `%APPDATA%\Trynta` to another PC does not carry the
//! biometric shortcut with it.
//!
//! `CRYPTPROTECT_LOCAL_MACHINE` is deliberately **not** set. With it, any user
//! on the machine could decrypt the blob.

use std::path::PathBuf;

use windows::Win32::Foundation::LocalFree;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

use crate::platform::secure_store::{SecureStore, SecureStoreError};

/// DPAPI-encrypted blobs under the app data directory.
pub struct DpapiStore {
    root: PathBuf,
}

impl DpapiStore {
    /// A store rooted at `%APPDATA%\Trynta\secure`.
    #[must_use]
    pub fn new() -> Self {
        let root = std::env::var_os("APPDATA")
            .map_or_else(|| PathBuf::from("."), PathBuf::from)
            .join("Trynta")
            .join("secure");
        Self { root }
    }

    /// A store rooted at an explicit directory, for tests.
    #[must_use]
    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// Entry names are hex-encoded so an arbitrary key cannot escape the
    /// directory or collide with a path separator.
    fn path_for(&self, key: &str) -> PathBuf {
        use std::fmt::Write as _;
        let encoded = key.bytes().fold(String::new(), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        });
        self.root.join(format!("{encoded}.dpapi"))
    }
}

impl Default for DpapiStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Borrow a slice as a DPAPI blob descriptor.
fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
        // `pbData` is not written to by either DPAPI entry point we call, but
        // the binding types it as mutable, so the cast is needed.
        pbData: bytes.as_ptr().cast_mut(),
    }
}

/// Copy a DPAPI output blob into a `Vec` and free the OS allocation.
///
/// # Safety
///
/// `out` must be a blob DPAPI wrote, whose `pbData` is a `LocalAlloc` pointer of
/// `cbData` bytes that has not already been freed.
unsafe fn take_blob(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // SAFETY: guaranteed by this function's own contract — DPAPI wrote `out`,
    // so `pbData` points to `cbData` initialised bytes.
    let slice = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) };
    let owned = slice.to_vec();
    // SAFETY: DPAPI allocates output blobs with `LocalAlloc`, and the caller
    // owns them. Freeing exactly once, immediately after copying, is the
    // documented contract.
    unsafe {
        let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(out.pbData.cast())));
    }
    owned
}

impl SecureStore for DpapiStore {
    fn store(&self, key: &str, value: &[u8]) -> Result<(), SecureStoreError> {
        let input = blob(value);
        let mut output = CRYPT_INTEGER_BLOB::default();

        // SAFETY: `input` describes `value`, which outlives the call. `output`
        // is a valid out-pointer. Passing `None` for the optional entropy,
        // reserved and prompt-struct parameters is documented. No
        // CRYPTPROTECT_LOCAL_MACHINE, so the blob is user-scoped.
        unsafe {
            CryptProtectData(
                &raw const input,
                None,
                None,
                None,
                None,
                0, // no CRYPTPROTECT_LOCAL_MACHINE: user-scoped, deliberately
                &raw mut output,
            )
        }
        .map_err(|_| SecureStoreError::Platform)?;

        // SAFETY: `CryptProtectData` returned success, so `output` is a blob it
        // allocated and we own.
        let protected = unsafe { take_blob(output) };

        std::fs::create_dir_all(&self.root).map_err(|_| SecureStoreError::Platform)?;
        std::fs::write(self.path_for(key), protected).map_err(|_| SecureStoreError::Platform)
    }

    fn load(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStoreError> {
        let Ok(stored) = std::fs::read(self.path_for(key)) else {
            return Ok(None);
        };

        let input = blob(&stored);
        let mut output = CRYPT_INTEGER_BLOB::default();

        // SAFETY: as above. A failure here means the blob was written by a
        // different user or on a different machine, which is `Unreadable`
        // rather than a hard error — the caller falls back to the password.
        let ok = unsafe {
            CryptUnprotectData(
                &raw const input,
                None,
                None,
                None,
                None,
                0, // no CRYPTPROTECT_LOCAL_MACHINE: user-scoped, deliberately
                &raw mut output,
            )
        };
        ok.map_err(|_| SecureStoreError::Unreadable)?;

        // SAFETY: `CryptUnprotectData` returned success.
        Ok(Some(unsafe { take_blob(output) }))
    }

    fn delete(&self, key: &str) -> Result<(), SecureStoreError> {
        match std::fs::remove_file(self.path_for(key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SecureStoreError::Platform),
        }
    }
}
