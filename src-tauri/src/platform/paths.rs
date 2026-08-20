// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where the vault lives on each platform (SPEC-V1 §8).
//!
//! ```text
//!   macOS    ~/Library/Application Support/Trynta
//!   Windows  %APPDATA%\Trynta
//! ```
//!
//! Not Tauri's `app_data_dir()`, which keys off the bundle identifier and would
//! give `%APPDATA%\dev.trynta.desktop`. §8 names these two paths, and the path
//! a user's vault lives at is not a detail to leave to a helper's convention —
//! changing it later means either a migration or a lost vault.
//!
//! Resolved from the environment rather than from a `dirs`-style crate: two env
//! reads do not justify a dependency in a process that holds key material.

use std::path::PathBuf;

use thiserror::Error;

/// The vault filename inside the data directory.
pub const VAULT_FILE: &str = "vault.db";

/// The application directory name (SPEC-V1 §8).
const APP_DIR: &str = "Trynta";

/// Why a path could not be resolved.
///
/// Carries no path: an error string that names a home directory is a small leak
/// on a shared screen, and there is nothing the user can do with it that the
/// message does not already tell them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum PathError {
    /// The OS did not tell us where the user's data directory is.
    #[error("could not determine this user's application data directory")]
    NoDataDir,

    /// The directory could not be created.
    #[error("could not create the application data directory")]
    NotCreatable,
}

/// The directory Trynta stores data in, created if it does not exist.
///
/// # Errors
///
/// [`PathError::NoDataDir`] if the platform's base directory is unknown,
/// [`PathError::NotCreatable`] if it cannot be created.
pub fn data_dir() -> Result<PathBuf, PathError> {
    let dir = base_dir()?.join(APP_DIR);
    std::fs::create_dir_all(&dir).map_err(|_| PathError::NotCreatable)?;
    Ok(dir)
}

/// The vault file path.
///
/// # Errors
///
/// As [`data_dir`].
pub fn vault_path() -> Result<PathBuf, PathError> {
    Ok(data_dir()?.join(VAULT_FILE))
}

#[cfg(windows)]
fn base_dir() -> Result<PathBuf, PathError> {
    // %APPDATA% is the roaming directory, which is what §8 names. It is set for
    // every interactive session; its absence means the process is running
    // somewhere we cannot store a vault.
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(PathError::NoDataDir)
}

#[cfg(target_os = "macos")]
fn base_dir() -> Result<PathBuf, PathError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .map(|home| home.join("Library").join("Application Support"))
        .ok_or(PathError::NoDataDir)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn base_dir() -> Result<PathBuf, PathError> {
    // Trynta ships macOS and Windows only (SPEC-V1 §8). This exists so the
    // workspace still builds on a Linux CI runner, and it fails closed rather
    // than inventing a location.
    Err(PathError::NoDataDir)
}
