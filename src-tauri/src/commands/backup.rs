// SPDX-License-Identifier: AGPL-3.0-or-later
//! Backup export and restore (SPEC-V1 §7.8).
//!
//! `keyring-store` already owns the format, the manifest signature and the
//! transactional restore. These commands are the thin orchestration on top: ask the
//! user for a path, hand the store the passphrase, report what happened.
//!
//! ## The passphrase is not the master password
//!
//! §7.8 gives a backup its own passphrase, derived with its own salt and the KDF
//! parameters recorded in the container header. That is deliberate: a backup travels
//! to places the vault does not — a USB stick, a cloud drive, another machine — and a
//! container that opened with the master password would make every copy of it as
//! valuable as the vault itself. `backup_roundtrip.rs` asserts the master password
//! does **not** open a container.
//!
//! ## Why a file dialog rather than a path from the webview
//!
//! The webview never chooses a filesystem path from nothing: it asks for a dialog,
//! the user picks, and Rust does the I/O. `capabilities/default.json` grants exactly
//! `dialog:allow-save` and `dialog:allow-open` and **no `fs:` permission at all**, so
//! the webview cannot read or write a file even knowing its name.
//!
//! ## Preview and apply open the container twice, on purpose
//!
//! §7.8 wants a preview before applying. The obvious implementation holds the opened
//! container in the session between the two steps — but an opened `BackupContents`
//! holds the *decrypted* body of every item, and holding that across user think-time
//! is a much bigger exposure than the second Argon2 derivation costs. So the preview
//! returns the path it opened, the apply step re-opens it, and nothing decrypted
//! outlives either call. A path is not a secret, and without an `fs:` permission the
//! webview can do nothing with one.

// Tauri owns these signatures; see the note in `items.rs`.
#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;

use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::commands::dto::{BackupPreviewDto, BackupSummaryDto, RestoreModeDto};
use crate::commands::AppState;
use crate::error::AppError;

/// §7.8's floor for a backup passphrase.
///
/// Short enough to type, long enough that the container's own KDF is doing
/// meaningful work rather than covering for a four-character secret.
const MIN_PASSPHRASE: usize = 12;

/// Suggested filename. Timestamped so two exports in a row do not collide.
fn suggested_name(now_ms: i64) -> String {
    format!("trynta-backup-{}.tryntabak", now_ms / 1000)
}

/// Export an encrypted backup under its own passphrase (SPEC-V1 §7.8).
///
/// Requires an unlocked vault: the export re-seals every item under a key derived
/// from the backup passphrase, which means opening them first.
///
/// Returns `None` when the user cancels the dialog, which is not an error.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is locked, [`AppError::Invalid`] if the
/// passphrase is under [`MIN_PASSPHRASE`], [`AppError::Storage`] or
/// [`AppError::Crypto`] if the container cannot be written.
#[tauri::command]
pub fn backup_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<Option<BackupSummaryDto>, AppError> {
    if passphrase.chars().count() < MIN_PASSPHRASE {
        return Err(AppError::Invalid);
    }

    let Some(chosen) = app
        .dialog()
        .file()
        .set_title("Save an encrypted backup")
        .set_file_name(suggested_name(state.session.now_ms()))
        .add_filter("Trynta backup", &["tryntabak"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let Ok(path) = chosen.into_path() else {
        return Err(AppError::Storage);
    };

    // The container is derived with the same calibrated parameters the vault uses. A
    // backup cheaper to attack than the vault it came from would be the weakest link.
    let params = state.session.file()?.kdf_params();

    let summary = state.session.with_session(|s| {
        s.backup_export(&path, &passphrase, params)
            .map_err(AppError::from)
    })?;

    Ok(Some(BackupSummaryDto {
        vaults: u32::try_from(summary.vaults).unwrap_or(u32::MAX),
        items: u32::try_from(summary.items).unwrap_or(u32::MAX),
        bytes: summary.bytes,
    }))
}

/// Open a container and report what a restore would do, without doing it (§7.8).
///
/// Opening authenticates the passphrase, the header MAC **and** the manifest
/// signature — all three — so a preview that returns at all describes something
/// trustworthy. Nothing is written.
///
/// Returns `None` when the user cancels the dialog.
///
/// # Errors
///
/// [`AppError::WrongPassword`] if the passphrase does not open the container,
/// [`AppError::TamperDetected`] if a MAC or a signature fails, [`AppError::Storage`]
/// if the file cannot be read.
#[tauri::command]
pub fn backup_preview(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<Option<BackupPreviewDto>, AppError> {
    let Some(chosen) = app
        .dialog()
        .file()
        .set_title("Choose a backup to restore")
        .add_filter("Trynta backup", &["tryntabak"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let Ok(path) = chosen.into_path() else {
        return Err(AppError::Storage);
    };

    let contents = keyring_store::open_container(&path, &passphrase)?;
    let preview = contents.preview(&state.vault_path)?;

    Ok(Some(BackupPreviewDto {
        mode: preview.mode.into(),
        created: u32::try_from(preview.created).unwrap_or(u32::MAX),
        merged: u32::try_from(preview.merged).unwrap_or(u32::MAX),
        skipped: u32::try_from(preview.skipped).unwrap_or(u32::MAX),
        created_at: preview.created_at,
        path: path.to_string_lossy().into_owned(),
    }))
}

/// Apply a restore (SPEC-V1 §7.8).
///
/// Never partially applies: a merge runs in one transaction, and a fresh or replacing
/// restore is built at a temporary path and moved into place.
///
/// `path` comes from a previous [`backup_preview`] and is re-opened here rather than
/// trusted: the passphrase has to verify again, so a path the webview altered opens
/// nothing.
///
/// # Errors
///
/// [`AppError::WrongPassword`], [`AppError::TamperDetected`], [`AppError::Invalid`] if
/// the restore would replace a **different account's** vault and `allow_replace` is
/// false, [`AppError::Storage`] or [`AppError::Crypto`] on a write failure.
#[tauri::command]
pub fn backup_restore(
    state: State<'_, AppState>,
    path: String,
    passphrase: String,
    allow_replace: bool,
) -> Result<BackupPreviewDto, AppError> {
    let path = PathBuf::from(path);
    let contents = keyring_store::open_container(&path, &passphrase)?;
    let preview = contents.preview(&state.vault_path)?;

    let applied = match preview.mode {
        keyring_store::RestoreMode::Merge => {
            let merged = state
                .session
                .with_session(|s| s.backup_merge(&contents).map_err(AppError::from))?;
            state.session.build_index()?;
            state.session.touch();
            merged
        }
        keyring_store::RestoreMode::Fresh | keyring_store::RestoreMode::Replace => {
            // Both write a whole vault file. `Replace` destroys an existing vault that
            // belongs to a different account, so the caller has to have said so.
            if preview.mode == keyring_store::RestoreMode::Replace && !allow_replace {
                return Err(AppError::Invalid);
            }
            // Lock first. The session's keys belong to the file about to be replaced,
            // and carrying them across the swap would leave it pointing at a vault
            // that no longer exists — fail closed (§4.10) and make the user unlock
            // the restored vault, which is also the only way to prove it opens.
            state.session.lock();
            contents.restore_replacing(&state.vault_path)?;
            preview
        }
    };

    Ok(BackupPreviewDto {
        mode: applied.mode.into(),
        created: u32::try_from(applied.created).unwrap_or(u32::MAX),
        merged: u32::try_from(applied.merged).unwrap_or(u32::MAX),
        skipped: u32::try_from(applied.skipped).unwrap_or(u32::MAX),
        created_at: applied.created_at,
        path: String::new(),
    })
}

impl From<keyring_store::RestoreMode> for RestoreModeDto {
    fn from(mode: keyring_store::RestoreMode) -> Self {
        match mode {
            keyring_store::RestoreMode::Fresh => Self::Fresh,
            keyring_store::RestoreMode::Merge => Self::Merge,
            keyring_store::RestoreMode::Replace => Self::Replace,
        }
    }
}
