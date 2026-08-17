//! Keyring application shell.
//!
//! This crate stays thin by construction: `commands/` orchestrates and never
//! holds business logic, which lives in `keyring-crypto` and `keyring-store`.
//! What does live here is the state that cannot: the lock/unlock state machine,
//! the auto-lock policy, the reveal rate limit, and the platform layer.

// `forbid` cannot be relaxed per-module, and CLAUDE.md §7 requires exactly one
// place where `unsafe` is permitted. So the crate denies it and `platform`
// carries a scoped allow; `scripts/check-unsafe.mjs` fails the build if that
// allow ever appears anywhere else.
#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod autolock;
pub mod commands;
pub mod error;
pub mod index;
#[allow(unsafe_code)]
pub mod platform;
pub mod reveal;
pub mod services;
pub mod session;

use std::sync::Arc;

use crate::autolock::SystemClock;
use crate::commands::AppState;
use crate::platform::Platform;
use crate::session::SessionManager;

/// Build and run the Tauri application.
///
/// # Panics
///
/// Panics only if the Tauri runtime cannot start, or if the platform cannot tell
/// us where to store data. Both happen before any vault is opened and before any
/// key material exists, and neither has a meaningful recovery: an app that cannot
/// find its own vault directory has nothing to show the user.
pub fn run() {
    let platform = Arc::new(Platform::host());
    let session = Arc::new(SessionManager::new(platform, Arc::new(SystemClock)));
    let vault_path = platform::paths::vault_path().expect("no application data directory");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new(session, vault_path))
        .invoke_handler(tauri::generate_handler![
            commands::account::account_status,
            commands::account::account_state,
            commands::account::account_exists,
            commands::account::account_create,
            commands::account::account_unlock,
            commands::account::account_reauth,
            commands::account::account_lock,
            commands::vaults::vaults_list,
            commands::vaults::vault_add,
            commands::vaults::vault_rename,
            commands::vaults::vault_set_color,
            commands::vaults::vault_delete,
            commands::items::items_list,
            commands::items::item_get,
            commands::items::item_reveal_field,
            commands::items::item_copy_field,
            commands::items::item_upsert,
            commands::items::item_delete,
            commands::items::item_restore,
            commands::items::item_toggle_favorite,
            commands::items::item_activity,
            commands::generator::generator_password,
            commands::generator::generator_passphrase,
            commands::generator::generator_pin,
            commands::generator::generator_history_list,
            commands::generator::generator_history_copy,
            commands::generator::generator_history_clear,
            commands::totp::totp_current,
            commands::app::app_platform_info,
        ])
        .run(tauri::generate_context!())
        .expect("the Tauri runtime failed to start");
}
