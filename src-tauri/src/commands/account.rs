// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account-level commands: create, unlock, lock, status (SPEC-V1 §5, §6).
//!
//! `account_*` operates on the database and the identity; `vault_*` operates on
//! collections inside it. SPEC-V1 §6 calls the rename out explicitly because rev
//! 1 used one word for both.
//!
//! The master password arrives here as a `String` from the webview, and §2
//! documents that exposure rather than pretending it away: it is typed into an
//! `<input>` and exists as an unzeroizable JS string for that moment. What this
//! module can do — and does — is refuse to keep a second copy. The `String` is
//! moved into the store, which derives from it and drops it; nothing here logs
//! it, stores it, or returns it.

// Tauri owns these signatures. `State<'_, T>` is an extractor and must be taken
// by value, and a command parameter has to be an owned deserializable type, so
// `&str` is not on offer. Both trip `needless_pass_by_value`; satisfying it would
// mean not using Tauri's extractors.
#![allow(clippy::needless_pass_by_value)]

use tauri::State;

use crate::commands::dto::{AccountStatus, VaultStateDto};
use crate::commands::AppState;
use crate::error::AppError;
use crate::platform::biometric::password_unlock_due;
use crate::session::VaultState;

/// Where the vault is, and what the lock screen needs to know.
///
/// Callable while locked — that is the point. Item and vault counts are zero
/// until the vault is open, because counting them requires the keys.
///
/// # Errors
///
/// Never fails: a missing vault file is a state, not an error.
#[tauri::command]
pub fn account_status(state: State<'_, AppState>) -> Result<AccountStatus, AppError> {
    let session = &state.session;

    // The state machine starts at `Uninitialised` and only reaches `Locked` once a file
    // is attached, which `account_unlock` does lazily — nothing opens the vault at
    // startup. So the machine's own state cannot answer the question the lock screen
    // asks, which is "does this device have a vault?", and answering it wrong is not
    // cosmetic: it offers *create* to someone who needs *unlock*, and `account_create`
    // then refuses with `InvalidState` because the file is right there. Found by opening
    // the lock screen twice.
    let state_now = match session.state() {
        VaultState::Uninitialised if state.vault_path.exists() => VaultState::Locked,
        other => other,
    };

    let (vault_count, item_count) = if state_now == VaultState::Unlocked {
        let vaults = session
            .with_session(|s| s.vaults_list().map_err(AppError::from))
            .map_or(0, |vaults| vaults.len());
        let items = session
            .with_index(crate::index::SearchIndex::len)
            .unwrap_or(0);
        (vaults, items)
    } else {
        (0, 0)
    };

    let biometrics = &session.platform().biometrics;
    Ok(AccountStatus {
        state: state_now.into(),
        vault_count,
        item_count,
        biometric_available: biometrics.is_available(),
        biometric_label: biometrics.kind().label().to_owned(),
        password_unlock_due: password_unlock_due(
            session.now_ms(),
            session.last_password_unlock_ms(),
        ),
    })
}

/// Create the vault, calibrate the KDF, and leave it unlocked.
///
/// Calibration runs here rather than at first unlock because the chosen cost is
/// written into the header and every future unlock pays it. Targeting 700 ms on
/// *this* machine (SPEC-V1 §3.2) is only meaningful if it is measured on the
/// machine that will do the unlocking.
///
/// # Errors
///
/// [`AppError::InvalidState`] if a vault already exists, [`AppError::Storage`]
/// or [`AppError::Crypto`] if creation fails.
#[tauri::command]
pub fn account_create(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<AccountStatus, AppError> {
    if state.vault_path.exists() {
        return Err(AppError::InvalidState);
    }

    let params = keyring_crypto::calibrate(
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
    );
    let file = keyring_store::VaultFile::create(&state.vault_path, &master_password, params)?;
    let file = std::sync::Arc::new(file);
    state.session.attach(std::sync::Arc::clone(&file));

    // A brand-new vault has no items, so the index is empty — but it must exist,
    // or every list command would report the vault locked.
    let keys = file.unlock(&master_password)?.into_keys();
    state.session.adopt(keys, true);
    state.session.build_index()?;

    // The first vault is the Personal one (SPEC-V1 §4.2). Created here rather
    // than lazily, because an account with no vault has nowhere to put an item
    // and every write path would have to handle that case.
    state.session.with_session(|s| {
        s.vault_add("Personal", "vault.accent.1")
            .map_err(AppError::from)
    })?;

    account_status(state)
}

/// Unlock with the master password (SPEC-V1 §5).
///
/// # Errors
///
/// [`AppError::WrongPassword`], [`AppError::Backoff`],
/// [`AppError::TamperDetected`], [`AppError::NoVault`] if no vault file exists,
/// or [`AppError::InvalidState`] if the vault is not locked.
#[tauri::command]
pub fn account_unlock(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<AccountStatus, AppError> {
    unlock_with(&state, &master_password, true)
}

/// The unlock every path shares.
///
/// `by_password` is the whole reason this is a parameter rather than a constant: a
/// biometric unlock must **not** reset the 14-day master-password clock (§5.1), or
/// the rule would never fire for the people it exists for.
///
/// # Errors
///
/// As [`account_unlock`].
pub(crate) fn unlock_with(
    state: &State<'_, AppState>,
    master_password: &str,
    by_password: bool,
) -> Result<AccountStatus, AppError> {
    let file = open_or_attach(state)?;
    state.session.begin_unlock()?;

    let keys = match file.unlock(master_password) {
        Ok(session) => session.into_keys(),
        Err(e) => {
            // Back to Locked before returning, or a wrong password would leave
            // the state machine stuck in Unlocking and refuse every retry.
            state.session.abort_unlock();
            return Err(e.into());
        }
    };

    state.session.adopt(keys, by_password);
    if let Err(e) = state.session.build_index() {
        // The keys are adopted but the vault is not usable. Lock rather than
        // leave a half-open session: fail closed (CLAUDE.md §4.10).
        state.session.lock();
        return Err(e.into());
    }
    account_status(state.clone())
}

/// Re-authenticate an already-unlocked vault (SPEC-V1 §6, reveal rate limit).
///
/// Exceeding 20 reveals in a rolling 60 seconds does not reject the next one —
/// it asks the human whether they meant it. This is how they answer.
///
/// Password only for now. SPEC-V1 §6 also allows biometric re-auth, and that
/// arrives with the biometric enrolment path (AC06, deferred to run 3 by the
/// acceptance verifier itself). A biometric variant that could not prompt would
/// be a button that does nothing.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is not unlocked, [`AppError::WrongPassword`]
/// if the password does not verify.
#[tauri::command]
pub fn account_reauth(state: State<'_, AppState>, master_password: String) -> Result<(), AppError> {
    if state.session.state() != VaultState::Unlocked {
        return Err(AppError::Locked);
    }
    let file = state.session.file()?;

    // Verified through the store's own unlock path, so the comparison is the
    // same constant-time one the lock screen uses and the backoff counter sees
    // the attempt. The resulting session is dropped immediately: re-auth proves
    // presence, it does not replace the keys we already hold.
    match file.unlock(&master_password) {
        Ok(session) => {
            drop(session.into_keys());
            state.session.note_reauth();
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Lock the vault (SPEC-V1 §5.2).
///
/// Wipes the MUK, the vault and item keys, and the decrypted index; clears the
/// clipboard if it still holds a value we put there. Idempotent.
///
/// # Errors
///
/// Never fails. Locking an already-locked vault is a no-op.
#[tauri::command]
pub fn account_lock(state: State<'_, AppState>) -> Result<AccountStatus, AppError> {
    state.session.lock();
    account_status(state)
}

/// Whether a vault file exists at all, for the first-run screen.
///
/// # Errors
///
/// Never fails.
#[tauri::command]
pub fn account_exists(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(state.vault_path.exists())
}

/// Open the vault file and attach it if that has not happened yet.
pub(crate) fn open_or_attach(
    state: &State<'_, AppState>,
) -> Result<std::sync::Arc<keyring_store::VaultFile>, AppError> {
    if let Ok(file) = state.session.file() {
        return Ok(file);
    }
    if !state.vault_path.exists() {
        return Err(AppError::NoVault);
    }
    let file = std::sync::Arc::new(keyring_store::VaultFile::open(&state.vault_path)?);
    state.session.attach(std::sync::Arc::clone(&file));
    Ok(file)
}

/// The lock state alone, for a cheap poll from the shell.
///
/// # Errors
///
/// Never fails.
#[tauri::command]
pub fn account_state(state: State<'_, AppState>) -> Result<VaultStateDto, AppError> {
    Ok(state.session.state().into())
}
