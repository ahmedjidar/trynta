// SPDX-License-Identifier: AGPL-3.0-or-later
//! First-run guided tour (SPEC-V1 §4.5, ADD-004 §⑦).
//!
//! Three thin commands over two `app_state` keys. All the policy is in
//! [`crate::services::tour`], which is pure; nothing here decides anything.
//!
//! ## Callable before unlock, and that is the point
//!
//! [`tour_state`] touches `app_state` only, so it answers with the vault locked
//! or absent — the same property that lets the lock screen render in the user's
//! theme. The card it governs is *on* the lock screen, so a command that needed
//! a session could never have been asked in time.
//!
//! ## What happens before there is a vault file
//!
//! On a genuinely fresh install there is no `.db` to write to, so
//! [`tour_mark_seen`] has nowhere to put the flag. It says so — it returns
//! `false` for "not persisted" rather than reporting a success that did not
//! happen. The frontend keeps the card down for the rest of the session from its
//! own state, and marks again once the vault exists, which is the moment the
//! user creates or unlocks one. The consequence is worth stating plainly: a user
//! who dismisses the card and quits **without creating a vault** sees it again
//! next launch. That is correct rather than a gap — nothing has been created, so
//! it is still their first run.

// Tauri owns these signatures; see the note in `items.rs`.
#![allow(clippy::needless_pass_by_value)]

use keyring_store::AppStateKey;
use tauri::State;

use crate::commands::dto::{TourKindDto, TourStateDto};
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::tour;

impl TourKindDto {
    /// The `app_state` key this tour's flag lives under.
    const fn key(self) -> AppStateKey {
        match self {
            Self::Unlock => AppStateKey::TourUnlockSeen,
            Self::App => AppStateKey::TourAppSeen,
        }
    }
}

/// Whether either tour should run (SPEC-V1 §4.5, ADD-004 §⑦).
///
/// Works locked, unlocked, and with no vault file at all. No vault means nothing
/// has been seen, so both tours are on — which is exactly the first-run case
/// this command exists for.
///
/// # Errors
///
/// [`AppError::Storage`] if `app_state` exists but cannot be read. A missing
/// vault file is not an error.
#[tauri::command]
pub fn tour_state(state: State<'_, AppState>) -> Result<TourStateDto, AppError> {
    let (unlock_seen, app_seen) = match state.session.file() {
        Ok(file) => (
            tour::seen_from(file.state_get(AppStateKey::TourUnlockSeen)?.as_deref()),
            tour::seen_from(file.state_get(AppStateKey::TourAppSeen)?.as_deref()),
        ),
        // No vault file. Nothing has been configured, so nothing has been seen.
        Err(_) => (false, false),
    };

    Ok(TourStateDto {
        show_unlock: tour::visible(unlock_seen, tour::DEV_REPLAY),
        show_app: tour::visible(app_seen, tour::DEV_REPLAY),
        replay: tour::DEV_REPLAY,
    })
}

/// Record that a tour has been seen (SPEC-V1 §4.5, ADD-004 §⑦).
///
/// Returns whether the flag was persisted. `false` means there is no vault file
/// yet and the write had nowhere to go; see the module note. It is deliberately
/// not an error: "there is nothing here to remember it in" is a normal first-run
/// state, and failing the call would make the frontend choose between showing an
/// error for a dismissed card and swallowing a rejection.
///
/// # Errors
///
/// [`AppError::Storage`] if a vault file exists and the write fails.
#[tauri::command]
pub fn tour_mark_seen(state: State<'_, AppState>, which: TourKindDto) -> Result<bool, AppError> {
    let Ok(file) = state.session.file() else {
        return Ok(false);
    };
    file.state_set(which.key(), tour::seen_to(true))?;
    Ok(true)
}

/// Clear both flags, so the tour runs again (SPEC-V1 §7.5).
///
/// The "replay the tour" action in settings. Both keys together rather than one
/// at a time: the two cards are one explanation split across the lock screen and
/// the app, and replaying half of it is not a state anyone asked for.
///
/// Requires a vault file, which the settings screen guarantees — it is only
/// reachable from an unlocked vault.
///
/// # Errors
///
/// [`AppError::NoVault`] if no vault file exists; [`AppError::Storage`] if the
/// clear fails.
#[tauri::command]
pub fn tour_reset(state: State<'_, AppState>) -> Result<(), AppError> {
    let file = state.session.file()?;
    file.state_clear(AppStateKey::TourUnlockSeen)?;
    file.state_clear(AppStateKey::TourAppSeen)?;
    Ok(())
}
