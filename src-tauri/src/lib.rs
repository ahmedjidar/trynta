//! Keyring application shell.
//!
//! The IPC command surface (SPEC-V1 §6) lands in run 2. This crate stays thin by
//! construction: `commands/` orchestrates and never holds business logic, which
//! lives in `keyring-crypto` and `keyring-store`.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod error;

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
