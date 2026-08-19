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
use tauri::Manager as _;

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
        // SPEC-V1 7.7. Registered unconditionally so `update_check` can report
        // "not configured" rather than the app failing to start: with no endpoint
        // in `tauri.conf.json`, `app.updater()` returns `EmptyEndpoints` and the
        // command maps that to `featureUnavailable`. Every JS-facing permission
        // this plugin offers is deliberately left out of `capabilities/`; the
        // frontend goes through our two commands, so the webview cannot start a
        // download or an install on its own.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::new(session, vault_path))
        // "Hide from screen capture" has to survive a restart, and the flag lives in
        // `app_state`, which is readable before unlock precisely so decisions like this
        // one can be made while the vault is still closed (SPEC-V1 §4.5). Reading it here
        // means the window is protected from the moment it appears rather than from the
        // moment the user unlocks — the setting says "hide the app", not "hide the vault".
        .setup(|app| {
            let state = app.state::<AppState>();
            let enabled = commands::settings::content_protection_at_startup(&state);
            if enabled {
                commands::settings::apply_content_protection(app.handle(), true);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::account::account_status,
            commands::account::account_state,
            commands::account::account_exists,
            commands::account::account_create,
            commands::account::account_unlock,
            commands::backup::backup_export,
            commands::backup::backup_preview,
            commands::backup::backup_restore,
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
            commands::icon::item_icon,
            commands::icon::item_set_icon,
            commands::icon::item_clear_icon,
            commands::items::item_delete,
            commands::items::item_edit_meta,
            commands::items::item_restore,
            commands::items::item_toggle_favorite,
            commands::items::item_activity,
            commands::generator::generator_password,
            commands::generator::generator_passphrase,
            commands::generator::generator_pin,
            commands::generator::password_strength,
            commands::generator::generator_history_list,
            commands::generator::generator_history_copy,
            commands::generator::generator_history_clear,
            commands::totp::totp_current,
            commands::totp::totp_parse,
            commands::totp::item_set_totp,
            commands::security::security_report_run,
            commands::security::security_breach_check,
            commands::app::app_platform_info,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::theme::theme_list,
            commands::theme::theme_set,
            commands::theme::theme_import,
            commands::theme::theme_import_file,
            commands::theme::theme_delete,
            commands::updates::update_check,
            commands::updates::update_install,
            commands::updates::update_checks_set_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("the Tauri runtime failed to start");
}
