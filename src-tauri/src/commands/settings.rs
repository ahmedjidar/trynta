//! Settings commands (SPEC-V1 §6, §7.5).
//!
//! §7.5: *"Encrypted in the vault, except the §4.5 list."* So a settings read spans two
//! stores and a write has to know which one a field lives in:
//!
//! | Store | Fields |
//! |---|---|
//! | `app_state`, plaintext, readable pre-unlock | biometric-enabled, screen-capture, update checks, the two check timestamps |
//! | `app_cache.settings`, encrypted | clipboard clearing and its interval, breach watching, reveal re-auth, list density |
//!
//! The DTO presents them as one object because that is what a settings screen is, and
//! `settings_set` takes a **patch** rather than a whole object: two screens editing
//! different rows must not overwrite each other's values, and a full-object write from a
//! stale UI silently reverts anything it did not know about.
//!
//! ## Two rows the design specifies that are not here
//!
//! `handoffs/README.md`: *"If the handoff specifies something that would break a security
//! invariant in CLAUDE.md §4 … the invariant wins. Flag it and stop."*
//!
//! - **"Share anonymous diagnostics — Crash reports only. Never vault contents."**
//!   CLAUDE.md §1 bans telemetry pre-1.0 and §4.7 bans a crash reporter outright, because
//!   one can capture memory. There is no field for it, so no UI can toggle it and no
//!   later commit can wire it up by accident.
//! - **Autofill and browser-extension toggles.** Autofill is V3 (§7.5), and §7.5 requires
//!   an honest "not available yet" state rather than *"a toggle that does nothing"*. The
//!   DTO carries `autofill_available: false` so the screen can say so without a switch.
//!
//! Both are recorded in `handoffs/MANIFEST.md` as HO-002 item 4.

// Tauri owns these signatures; see the note in `items.rs`.
#![allow(clippy::needless_pass_by_value)]

use keyring_store::{AppCacheKey, AppStateKey};
use tauri::{AppHandle, Manager as _, State};

use crate::commands::dto::{DensityDto, SettingsDto, SettingsPatch};
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::settings::{Density, Settings};
use crate::services::updater;

/// Read the encrypted blob. Requires an unlocked vault.
fn load(state: &State<'_, AppState>) -> Result<Settings, AppError> {
    state.session.with_session(|session| {
        let mut settings = session
            .app_cache_get(AppCacheKey::Settings)?
            .map_or_else(Settings::default, |bytes| Settings::decode(&bytes));
        settings.normalise();
        Ok(settings)
    })
}

/// Write the encrypted blob. Requires an unlocked vault.
fn save(state: &State<'_, AppState>, settings: &Settings) -> Result<(), AppError> {
    let encoded = settings.encode().ok_or(AppError::Storage)?;
    state.session.with_session(|session| {
        session.app_cache_put(AppCacheKey::Settings, &encoded)?;
        Ok::<(), AppError>(())
    })
}

/// Read a boolean from `app_state`, defaulting when absent or unparseable.
///
/// A hand-edited plaintext value must not put the app in a state its own UI cannot
/// produce, and it must not fail the read either — a settings screen that refuses to open
/// because one preference is corrupt is worse than one that shows a default.
fn state_flag(
    state: &State<'_, AppState>,
    key: AppStateKey,
    default: bool,
) -> Result<bool, AppError> {
    let Ok(file) = state.session.file() else {
        return Ok(default);
    };
    Ok(file
        .state_get(key)?
        .and_then(|raw| raw.trim().parse::<bool>().ok())
        .unwrap_or(default))
}

/// Everything the settings screen shows (SPEC-V1 §6).
///
/// Requires an unlocked vault: most of it is in the encrypted blob. The `app_state` half
/// would be readable while locked, but a settings screen that showed four of twelve rows
/// would be worse than one that is simply unavailable.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is locked; [`AppError::Storage`] on a read failure.
#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<SettingsDto, AppError> {
    let settings = load(&state)?;

    Ok(SettingsDto {
        clear_clipboard: settings.clear_clipboard,
        clipboard_seconds: settings.clipboard_seconds,
        watch_for_breaches: settings.watch_for_breaches,
        require_master_on_reveal: settings.require_master_on_reveal,
        density: match settings.density {
            Density::Comfortable => DensityDto::Comfortable,
            Density::Compact => DensityDto::Compact,
        },
        biometric_enabled: state_flag(&state, AppStateKey::BiometricEnabled, false)?,
        biometric_available: state.session.platform().biometrics.is_available(),
        content_protection: state_flag(&state, AppStateKey::ContentProtectionEnabled, false)?,
        update_checks_enabled: {
            let stored = state
                .session
                .file()
                .ok()
                .and_then(|f| f.state_get(AppStateKey::UpdateChecksEnabled).ok())
                .flatten();
            updater::checks_enabled_from(stored.as_deref())
        },
        // V3 (SPEC-V1 §7.5). Reported as a fact so the screen can state it plainly
        // instead of offering a switch that does nothing.
        autofill_available: false,
        imported_theme_count: u32::try_from(settings.valid_themes().len()).unwrap_or(u32::MAX),
    })
}

/// Apply a patch (SPEC-V1 §6).
///
/// Every field is optional and only the present ones are written, so two screens editing
/// different rows cannot overwrite each other. Returns the settings as they are *after*
/// the write, so the caller renders what was stored rather than what it asked for — the
/// two differ whenever a value is clamped.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is locked; [`AppError::Storage`] on a write failure;
/// [`AppError::Biometric`] if biometric unlock is switched on where no biometric is
/// available, because storing that flag would make the lock screen offer a button that
/// cannot work.
#[tauri::command]
pub fn settings_set(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<SettingsDto, AppError> {
    // ── The encrypted half ──
    let mut settings = load(&state)?;
    let mut touched = false;

    if let Some(value) = patch.clear_clipboard {
        settings.clear_clipboard = value;
        touched = true;
    }
    if let Some(value) = patch.clipboard_seconds {
        settings.clipboard_seconds = value;
        touched = true;
    }
    if let Some(value) = patch.watch_for_breaches {
        settings.watch_for_breaches = value;
        touched = true;
    }
    if let Some(value) = patch.require_master_on_reveal {
        settings.require_master_on_reveal = value;
        touched = true;
    }
    if let Some(value) = patch.density {
        settings.density = match value {
            DensityDto::Comfortable => Density::Comfortable,
            DensityDto::Compact => Density::Compact,
        };
        touched = true;
    }

    if touched {
        // Clamp before writing, not only on read: a value out of range must never reach
        // disk, or the next build's bounds decide what the user meant.
        settings.normalise();
        save(&state, &settings)?;
    }

    // ── The plaintext half ──
    if let Some(value) = patch.biometric_enabled {
        if value && !state.session.platform().biometrics.is_available() {
            // Fail closed. Storing this would make the lock screen offer a Touch ID or
            // Hello affordance on a machine with neither.
            return Err(AppError::Biometric);
        }
        state
            .session
            .file()?
            .state_set(AppStateKey::BiometricEnabled, &value.to_string())?;
    }
    if let Some(value) = patch.content_protection {
        state
            .session
            .file()?
            .state_set(AppStateKey::ContentProtectionEnabled, &value.to_string())?;
        // ADD-002 Q11: "Toggle it at runtime, not config-only."
        apply_content_protection(&app, value);
    }
    if let Some(value) = patch.update_checks_enabled {
        state.session.file()?.state_set(
            AppStateKey::UpdateChecksEnabled,
            updater::checks_enabled_to(value),
        )?;
    }

    settings_get(state)
}

/// Apply "hide from screen capture" to the live window (ADD-002 Q11).
///
/// Tauri's `set_content_protected` is the platform call: on Windows it is
/// `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`, which makes the compositor
/// hand a blank region to any capture path — including the ones a user cannot see,
/// like a screen recorder started by something else.
///
/// **This is applied, not merely stored.** The setting existed as a persisted flag
/// that nothing ever read, which is the worst possible shape for a security
/// control: the switch was on, the window was captured anyway, and the UI said it
/// was protected. A toggle that does nothing is worse than no toggle.
///
/// Failure is logged and swallowed rather than propagated. The window handle can
/// legitimately be gone — mid-shutdown, or on a platform build without the
/// affinity API — and refusing to save an otherwise valid settings change because
/// a decoration could not be applied would be the wrong trade. The UI reads the
/// stored value back, so it never claims success on its own.
///
/// **UNVERIFIED on macOS**: `set_content_protected` maps to `NSWindow`'s
/// `sharingType = .none` there, and no build of this has run on Apple hardware.
/// See MACOS-UNVERIFIED.md.
pub fn apply_content_protection(app: &AppHandle, enabled: bool) {
    let Some(window) = app.get_webview_window("main") else {
        tracing::debug!("no main window while applying content protection");
        return;
    };
    if let Err(error) = window.set_content_protected(enabled) {
        // The error carries a window-system message, never anything from the vault.
        tracing::warn!(%error, "could not change the window's capture protection");
    }
}

/// Whether capture protection was switched on, read before any unlock.
///
/// Opens the vault file if it is not already attached, because at startup nothing
/// has. Only `app_state` is touched, which is plaintext and exhaustively
/// enumerated (SPEC-V1 §4.5) — no key material is involved and no unlock happens.
///
/// Every failure means "off": a missing vault on first run, an unreadable file, a
/// value that does not parse. Failing closed here would mean a first-run window
/// that is invisible to the user's own screenshots with no way to have asked for
/// it, which is not a safer default, just a stranger one.
#[must_use]
pub fn content_protection_at_startup(state: &AppState) -> bool {
    if let Ok(file) = state.session.file() {
        return read_protection_flag(&file);
    }
    if !state.vault_path.exists() {
        return false;
    }
    let Ok(file) = keyring_store::VaultFile::open(&state.vault_path) else {
        return false;
    };
    let file = std::sync::Arc::new(file);
    state.session.attach(std::sync::Arc::clone(&file));
    read_protection_flag(&file)
}

/// The stored flag, defaulting to off.
fn read_protection_flag(file: &keyring_store::VaultFile) -> bool {
    file.state_get(AppStateKey::ContentProtectionEnabled)
        .ok()
        .flatten()
        .and_then(|raw| raw.trim().parse::<bool>().ok())
        .unwrap_or(false)
}

/// Whether the user asked for each reveal to be confirmed (SPEC-V1 §7.5).
///
/// Read from the encrypted settings blob on every reveal rather than cached. A
/// cache would be one more thing to invalidate when the setting changes, and a
/// stale `false` here is a security control that silently stopped working — the
/// failure this function exists to fix.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is not open, or a storage error.
pub fn reveal_requires_master(state: &State<'_, AppState>) -> Result<bool, AppError> {
    Ok(load(state)?.require_master_on_reveal)
}
