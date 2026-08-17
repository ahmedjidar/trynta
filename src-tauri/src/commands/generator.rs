//! Generator commands (SPEC-V1 §6, §7.3).
//!
//! Two plaintext values cross IPC in this product. `item_reveal_field` is one; a
//! freshly generated password is the other, and it has to — showing the user the
//! password is the feature. What keeps it narrow is the same discipline: one value
//! per explicit action, and nothing the frontend is expected to keep.
//!
//! The **history** is deliberately not part of that. SPEC-V1 §6 gives it a `copy`
//! command and no reveal, so [`generator_history_list`] returns kinds, entropies
//! and timestamps while the values stay in Rust and go to the clipboard through
//! [`generator_history_copy`]. Rendering twenty old passwords into the webview to
//! draw a list nobody reads them from would be a lot of exposure for no benefit.
//!
//! History lives in `app_cache` under `muk.appcache`, so it needs an unlocked
//! vault. All three generate commands therefore require one: the generator sits
//! behind the lock screen in the UI, and a generator that silently forgets when
//! locked is worse than one that plainly needs unlocking.

// Tauri owns these signatures. `State<'_, T>` is an extractor and must be taken
// by value, and a command parameter has to be an owned deserializable type, so
// `&str` is not on offer. Both trip `needless_pass_by_value`; satisfying it would
// mean not using Tauri's extractors.
#![allow(clippy::needless_pass_by_value)]

use tauri::State;
use uuid::Uuid;

use crate::commands::dto::{
    GeneratedDto, HistoryEntryDto, PassphraseOptionsDto, PasswordOptionsDto,
};
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::generator::{self, Generated};
use crate::services::history::{GeneratedKind, History, HistoryEntry};
use keyring_store::AppCacheKey;

/// Generate a random password (SPEC-V1 §7.3).
///
/// # Errors
///
/// [`AppError::Locked`], or [`AppError::Crypto`] if the OS randomness source is
/// unavailable — for which there is no fallback.
#[tauri::command]
pub fn generator_password(
    state: State<'_, AppState>,
    options: PasswordOptionsDto,
) -> Result<GeneratedDto, AppError> {
    let generated = generator::password(options.into())?;
    finish(&state, generated, GeneratedKind::Password)
}

/// Generate a passphrase from the bundled EFF long wordlist (SPEC-V1 §7.3).
///
/// # Errors
///
/// [`AppError::FeatureUnavailable`] while the wordlist is not vendored — the
/// asset's licence is unconfirmed, and generating from a short list would report
/// 12.9 bits per word while delivering fewer. [`AppError::Locked`] or
/// [`AppError::Crypto`] otherwise.
#[tauri::command]
pub fn generator_passphrase(
    state: State<'_, AppState>,
    options: PassphraseOptionsDto,
) -> Result<GeneratedDto, AppError> {
    let words = generator::bundled_wordlist().ok_or(AppError::FeatureUnavailable)?;
    let generated = generator::passphrase(&options.into(), &words)?;
    finish(&state, generated, GeneratedKind::Passphrase)
}

/// Generate a numeric PIN (SPEC-V1 §7.3).
///
/// # Errors
///
/// As [`generator_password`].
#[tauri::command]
pub fn generator_pin(state: State<'_, AppState>, length: usize) -> Result<GeneratedDto, AppError> {
    let generated = generator::pin(length)?;
    finish(&state, generated, GeneratedKind::Pin)
}

/// Record a generated value in the history and project it for the wire.
///
/// The `Zeroizing` buffer is consumed here: one copy goes into the history that
/// is about to be sealed, one crosses IPC, and the original is wiped when it
/// drops at the end of this function.
fn finish(
    state: &State<'_, AppState>,
    generated: Generated,
    kind: GeneratedKind,
) -> Result<GeneratedDto, AppError> {
    let entry = HistoryEntry {
        id: Uuid::new_v4(),
        value: generated.value.to_string(),
        kind,
        entropy_bits: generated.entropy_bits,
        created_at: state.session.now_ms(),
    };
    let now = state.session.now_ms();

    state.session.with_session(|s| {
        let mut history = load(s)?;
        history.record(entry, now);
        save(s, &history)
    })?;

    Ok(GeneratedDto {
        value: generated.value.to_string(),
        entropy_bits: generated.entropy_bits,
    })
}

/// Read the history, pruning anything that has aged out.
///
/// Pruning on read as well as write is what makes the 7-day expiry true for a
/// user who stopped generating (SPEC-V1 §7.3).
fn load(session: &keyring_store::vault::Session<'_>) -> Result<History, AppError> {
    let stored = session.app_cache_get(AppCacheKey::GeneratorHistory)?;
    Ok(stored.map_or_else(History::new, |bytes| History::decode(&bytes)))
}

fn save(session: &keyring_store::vault::Session<'_>, history: &History) -> Result<(), AppError> {
    let encoded = history.encode().ok_or(AppError::Storage)?;
    session.app_cache_put(AppCacheKey::GeneratorHistory, &encoded)?;
    Ok(())
}

/// The retained history, newest first, **without values** (SPEC-V1 §6).
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::Storage`] or [`AppError::Crypto`].
#[tauri::command]
pub fn generator_history_list(
    state: State<'_, AppState>,
) -> Result<Vec<HistoryEntryDto>, AppError> {
    let now = state.session.now_ms();
    state.session.with_session(|s| {
        let mut history = load(s)?;
        history.prune(now);
        // Written back so an expired entry is gone from disk, not merely absent
        // from this answer.
        save(s, &history)?;
        Ok(history.entries.iter().map(HistoryEntryDto::from).collect())
    })
}

/// Copy one history entry to the clipboard, in Rust (CLAUDE.md §4.3).
///
/// Returns `()`. The value never enters the webview, exactly as
/// `item_copy_field` does not let a password through.
///
/// # Errors
///
/// [`AppError::NotFound`] if the entry has expired or been cleared,
/// [`AppError::Locked`], [`AppError::Clipboard`].
#[tauri::command]
pub fn generator_history_copy(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    let now = state.session.now_ms();
    let token = state.session.with_session(|s| {
        let mut history = load(s)?;
        history.prune(now);
        let value = history.value_of(id).ok_or(AppError::NotFound)?;
        state
            .session
            .platform()
            .clipboard
            .set_secret(value)
            .map_err(AppError::from)
    })?;

    state.session.note_clipboard_write(token);
    state.session.touch();
    Ok(())
}

/// Forget the whole history (SPEC-V1 §7.5, "clear generator history").
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::Storage`].
#[tauri::command]
pub fn generator_history_clear(state: State<'_, AppState>) -> Result<(), AppError> {
    state.session.with_session(|s| {
        // The row is deleted rather than overwritten with an empty history, so
        // nothing about how much was there survives.
        s.app_cache_clear(AppCacheKey::GeneratorHistory)
            .map_err(AppError::from)
    })
}
