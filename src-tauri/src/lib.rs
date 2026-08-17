//! Keyring application shell.
//!
//! This crate stays thin by construction: `commands/` orchestrates and never
//! holds business logic, which lives in `keyring-crypto` and `keyring-store`.
//! What does live here is the state that cannot: the lock/unlock state machine,
//! the auto-lock policy, and the platform layer.

// `forbid` cannot be relaxed per-module, and CLAUDE.md §7 requires exactly one
// place where `unsafe` is permitted. So the crate denies it and `platform`
// carries a scoped allow; `scripts/check-unsafe.mjs` fails the build if that
// allow ever appears anywhere else.
#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod autolock;
pub mod error;
pub mod index;
#[allow(unsafe_code)]
pub mod platform;
pub mod session;

/// Build and run the Tauri application.
///
/// # Panics
///
/// Panics only if the Tauri runtime cannot start, which is unrecoverable and
/// happens before any vault is opened or any key material exists.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("the Tauri runtime failed to start");
}
