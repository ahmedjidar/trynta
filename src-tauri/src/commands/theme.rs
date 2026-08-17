//! Theme commands (SPEC-V1 §6, §7.6).
//!
//! Three things live here and the split between them is the whole design:
//!
//! - **The selection** — mode (`dark` / `light` / `system`) and an optional imported
//!   theme id — lives in `app_state`, in the clear, because §4.5 puts it there so
//!   the lock screen can render in the user's theme before there is a key.
//! - **Imported theme values** live in the encrypted settings blob, because they are
//!   user data. The consequence is deliberate and worth stating: **a custom theme
//!   applies after unlock, not before.** The lock screen always uses the built-in
//!   dark or light. Putting theme values in `app_state` would mean a plaintext file
//!   describing the user's taste, and §4.5's list is exhaustive anyway.
//! - **Validation** is [`crate::services::theme::validate`], in Rust, per §7.6:
//!   *"An imported theme is untrusted input… Validate in Rust, not the webview."*
//!   Nothing here re-implements any part of it and nothing here can bypass it.
//!
//! ## Deviation from §6's signature, stated plainly
//!
//! §6 lists `theme_set(id)`. This takes `theme_set(id, mode)` and writes both
//! `app_state` keys in one call. §4.5 defines `theme_id` and `theme_mode` as two
//! independent keys, so a single-argument command could only ever set one of them
//! and the UI would need a second command anyway. One call that leaves the pair
//! consistent beats two that can disagree.
//!
//! `theme_delete` is not in §6 either. A picker that can import and never remove is
//! a trap: the list is bounded at 32, and without a delete the only way out of a
//! full list is a file nobody can find.

// Tauri owns these signatures; see the note in `items.rs`.
#![allow(clippy::needless_pass_by_value)]

use keyring_store::{AppCacheKey, AppStateKey};
use tauri::State;

use crate::commands::dto::{ThemeCatalogDto, ThemeDto, ThemeModeDto};
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::settings::Settings;
use crate::services::theme;

/// Read the settings blob. Requires an unlocked vault.
fn load_settings(state: &State<'_, AppState>) -> Result<Settings, AppError> {
    state.session.with_session(|session| {
        let mut settings = session
            .app_cache_get(AppCacheKey::Settings)?
            .map_or_else(Settings::default, |bytes| Settings::decode(&bytes));
        settings.normalise();
        Ok(settings)
    })
}

/// Write the settings blob. Requires an unlocked vault.
fn save_settings(state: &State<'_, AppState>, settings: &Settings) -> Result<(), AppError> {
    let encoded = settings.encode().ok_or(AppError::Storage)?;
    state.session.with_session(|session| {
        session.app_cache_put(AppCacheKey::Settings, &encoded)?;
        Ok::<(), AppError>(())
    })
}

/// Every theme the user can pick, and which is active (SPEC-V1 §6).
///
/// Works locked **and** unlocked, and reports which it was: locked, the built-ins
/// and the stored selection are available but imported themes are not, because their
/// values are encrypted. A UI that knows this can show the picker in a disabled
/// state rather than showing an empty list that looks like the themes were lost.
///
/// # Errors
///
/// [`AppError::NoVault`] if no vault file exists yet — there is nowhere for a
/// selection to live before then. [`AppError::Storage`] if `app_state` is unreadable.
#[tauri::command]
pub fn theme_list(state: State<'_, AppState>) -> Result<ThemeCatalogDto, AppError> {
    let file = state.session.file()?;
    let mode = file
        .state_get(AppStateKey::ThemeMode)?
        .and_then(|raw| ThemeModeDto::parse(&raw))
        .unwrap_or_default();
    let active_id = file.state_get(AppStateKey::ThemeId)?;

    // Imported themes need the MUK. Locked is not an error here.
    let imported = match load_settings(&state) {
        Ok(settings) => settings.valid_themes().iter().map(ThemeDto::from).collect(),
        Err(AppError::Locked | AppError::NoVault) => Vec::new(),
        Err(other) => return Err(other),
    };

    Ok(ThemeCatalogDto {
        mode,
        active_id,
        imported,
        locked: state.session.state() != crate::session::VaultState::Unlocked,
    })
}

/// Set the mode and, optionally, the active imported theme (SPEC-V1 §6).
///
/// `id` of `None` means the built-in palette. An `id` that names no stored theme is
/// **refused** rather than stored: a dangling selection would render the built-in
/// theme while the settings screen claimed something else was active, and the user
/// would have no way to tell which was true.
///
/// Both `app_state` writes happen together, so the pair cannot end up half-applied.
///
/// # Errors
///
/// [`AppError::NotFound`] if `id` names no stored theme; [`AppError::Locked`] if an
/// `id` is given while the vault is locked, because verifying it needs the settings
/// blob; [`AppError::Storage`] on a write failure.
#[tauri::command]
pub fn theme_set(
    state: State<'_, AppState>,
    id: Option<String>,
    mode: ThemeModeDto,
) -> Result<(), AppError> {
    if let Some(wanted) = id.as_deref() {
        // Refusing an unknown id is the cheap half of keeping the selection and the
        // render in agreement. `load_settings` fails closed while locked, which is
        // correct: we cannot verify, so we do not store.
        if load_settings(&state)?.theme(wanted).is_none() {
            return Err(AppError::NotFound);
        }
    }

    let file = state.session.file()?;
    file.state_set(AppStateKey::ThemeMode, mode.as_str())?;
    match id.as_deref() {
        Some(wanted) => file.state_set(AppStateKey::ThemeId, wanted)?,
        None => file.state_clear(AppStateKey::ThemeId)?,
    }
    Ok(())
}

/// Validate and store an imported theme (SPEC-V1 §6, §7.6).
///
/// The document is validated **before** it is stored, and stored as the document
/// rather than as a parsed theme, so every load re-validates it against the current
/// grammar. Tightening the grammar therefore protects themes that are already on
/// disk, not just new imports.
///
/// Importing does not activate. That is `theme_set`, and keeping them separate means
/// a malformed-but-valid theme cannot change what the user is looking at as a side
/// effect of adding it to the list.
///
/// # Errors
///
/// [`AppError::Invalid`] if validation refuses the document — including every
/// spelling of `url()`, which is the specific attack §7.6 names.
/// [`AppError::LastVaultRemaining`] is reused for "no room": see the note on the
/// error mapping below. [`AppError::Locked`] if the vault is locked.
#[tauri::command]
pub fn theme_import(state: State<'_, AppState>, document: String) -> Result<ThemeDto, AppError> {
    // Fail closed on anything the validator does not positively admit. The error
    // deliberately does not carry the validator's message: it names the offending
    // token, which is attacker-supplied text, and CLAUDE.md §4.6 keeps that out of
    // anything renderable.
    let validated = theme::validate(&document).map_err(|_| AppError::Invalid)?;

    let mut settings = load_settings(&state)?;
    if settings.upsert_theme(&validated, &document).is_err() {
        // The list is full. There is no dedicated discriminant and inventing one
        // for a bound the UI already knows would grow the IPC error surface for a
        // case the UI can prevent; `Invalid` is the honest bucket — the request
        // cannot be satisfied as made.
        return Err(AppError::Invalid);
    }
    settings.normalise();
    save_settings(&state, &settings)?;

    Ok(ThemeDto::from(&validated))
}

/// Remove an imported theme.
///
/// If the removed theme was active, the selection is cleared in the same call so the
/// app cannot be left pointing at a theme that no longer exists.
///
/// # Errors
///
/// [`AppError::NotFound`] if no such theme is stored; [`AppError::Locked`] if the
/// vault is locked; [`AppError::Storage`] on a write failure.
#[tauri::command]
pub fn theme_delete(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let mut settings = load_settings(&state)?;
    if !settings.remove_theme(&id) {
        return Err(AppError::NotFound);
    }
    save_settings(&state, &settings)?;

    let file = state.session.file()?;
    if file.state_get(AppStateKey::ThemeId)?.as_deref() == Some(id.as_str()) {
        file.state_clear(AppStateKey::ThemeId)?;
    }
    Ok(())
}
