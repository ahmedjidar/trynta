//! Biometric unlock, end to end (SPEC-V1 §5, §7.5; AC06).
//!
//! Every piece of this existed and none of it was connected. `platform::biometric`
//! has had `enrol` / `unwrap_secret` / `revoke` since run 2, Windows Hello has a
//! working implementation behind them, and the settings screen had a toggle that
//! wrote a boolean into `app_state`. Nothing ever enrolled anything, and no unlock
//! path ever asked the platform for a secret — so switching the toggle on changed a
//! flag, offered a promise on the lock screen, and did nothing.
//!
//! ## What is stored, and why it is the master password
//!
//! Enrolment wraps the **master password** and hands it to the platform's secure
//! store — DPAPI plus Credential Manager on Windows, released only after Hello
//! signs. Unlocking then runs the ordinary `VaultFile::unlock` with it.
//!
//! Storing a derived key instead was the alternative and is worse here: the vault's
//! KDF is the thing that makes a stolen file expensive, and a stored key skips it,
//! so a compromise of the secure store would hand over something strictly more
//! useful than the password. Storing the password means the attacker who defeats
//! Hello gets exactly what the user has — no more — and every existing check on the
//! unlock path still runs.
//!
//! ## Enrolment needs the password, so it asks for it
//!
//! There is no moment when the app is holding the master password and could enrol
//! silently: it is used once at unlock and dropped. So enabling biometric unlock
//! takes the password as an argument and verifies it before storing anything.
//! Verifying rather than trusting matters — enrolling an unverified string would
//! produce a biometric unlock that fails forever, with the biometric working fine.
//!
//! ## The 14-day rule still applies
//!
//! §5.1 requires a master-password unlock at least every 14 days.
//! `password_unlock_due` already implements it and `account_status` already reports
//! it; this path refuses when it comes due, rather than letting a fingerprint carry
//! the vault indefinitely.

// Tauri owns these signatures. See commands/generator.rs.
#![allow(clippy::needless_pass_by_value)]

use tauri::State;

use crate::commands::dto::AccountStatus;
use crate::commands::{account, AppState};
use crate::error::AppError;
use crate::platform::biometric::password_unlock_due;
use keyring_store::AppStateKey;

/// The label the wrapped secret is stored under.
///
/// One vault, one entry. A second vault would need a second label, which is a V2
/// problem and not one to guess at now.
const LABEL: &str = "trynta.master";

/// Whether biometric unlock is available *and* set up on this device.
///
/// Both halves matter and they fail differently: no hardware is a fact about the
/// machine, no enrolment is a fact about this vault. The lock screen needs the
/// conjunction, because either one alone means the button cannot work.
///
/// # Errors
///
/// [`AppError::Storage`] if `app_state` cannot be read.
#[tauri::command]
pub fn biometric_ready(state: State<'_, AppState>) -> Result<bool, AppError> {
    if !state.session.platform().biometrics.is_available() {
        return Ok(false);
    }
    let Ok(file) = state.session.file() else {
        return Ok(false);
    };
    Ok(file
        .state_get(AppStateKey::BiometricEnabled)?
        .and_then(|raw| raw.trim().parse::<bool>().ok())
        .unwrap_or(false))
}

/// Turn biometric unlock on, verifying the password before storing it.
///
/// # Errors
///
/// [`AppError::Biometric`] if no biometric is available or the platform refuses to
/// store the secret, [`AppError::WrongPassword`] if the password does not open the
/// vault, [`AppError::Storage`].
#[tauri::command]
pub fn biometric_enable(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<(), AppError> {
    let platform = state.session.platform();
    if !platform.biometrics.is_available() {
        return Err(AppError::Biometric);
    }

    // Verify by opening the vault. Enrolling a password that does not work would
    // produce a biometric unlock that fails every time, on a biometric that is fine.
    let file = state.session.file()?;
    file.unlock(&master_password)?;

    platform
        .biometrics
        .enrol(LABEL, master_password.as_bytes())
        .map_err(|_| AppError::Biometric)?;

    file.state_set(AppStateKey::BiometricEnabled, "true")?;
    Ok(())
}

/// Turn biometric unlock off and destroy the stored secret.
///
/// The flag and the secret are cleared together. Clearing only the flag would leave
/// the master password in the platform store with nothing in the product admitting
/// it was there.
///
/// # Errors
///
/// [`AppError::Storage`] if the flag cannot be written.
#[tauri::command]
pub fn biometric_disable(state: State<'_, AppState>) -> Result<(), AppError> {
    // Revoke first: if it fails, the flag stays on and the user can see the feature
    // is still enabled, rather than the reverse — a product that says "off" over a
    // secret that is still stored.
    state
        .session
        .platform()
        .biometrics
        .revoke(LABEL)
        .map_err(|_| AppError::Biometric)?;

    let file = state.session.file()?;
    file.state_set(AppStateKey::BiometricEnabled, "false")?;
    Ok(())
}

/// Unlock with the platform biometric.
///
/// Prompts through the platform — Windows Hello here — and unlocks with the password
/// it releases. Failure is deliberately indistinguishable between "the user
/// cancelled", "the finger did not match" and "the enrolment was invalidated": all
/// three mean *use your password*, and telling them apart tells an attacker which of
/// their attempts got furthest.
///
/// # Errors
///
/// [`AppError::Biometric`] if unavailable, not enrolled, or the prompt did not
/// succeed; [`AppError::InvalidState`] if a master-password unlock is due (§5.1);
/// [`AppError::WrongPassword`] if the released secret no longer opens the vault,
/// which happens when the master password was changed elsewhere.
#[tauri::command]
pub fn account_unlock_biometric(state: State<'_, AppState>) -> Result<AccountStatus, AppError> {
    if !biometric_ready(state.clone())? {
        return Err(AppError::Biometric);
    }

    // §5.1: a password is due every 14 days regardless of how good the biometric is.
    //
    // Read from the session rather than from `app_state`: §4.5’s key list is
    // exhaustive and has no entry for this, so the last password unlock lives in
    // memory. The practical consequence is that the clock restarts when the app
    // does, which is the conservative direction — it asks for the password sooner,
    // never later.
    if password_unlock_due(
        state.session.now_ms(),
        state.session.last_password_unlock_ms(),
    ) {
        return Err(AppError::InvalidState);
    }

    let secret = state
        .session
        .platform()
        .biometrics
        .unwrap_secret(LABEL)
        .map_err(|_| AppError::Biometric)?;

    // The released bytes are the master password. Held for exactly as long as the
    // unlock takes; `Zeroizing` wipes it when this scope ends, including on the error
    // path below.
    let password =
        zeroize::Zeroizing::new(String::from_utf8(secret).map_err(|_| AppError::Biometric)?);

    // Straight through the ordinary unlock, so every check on that path still runs.
    // `false` for `by_password`: this was a biometric unlock and must not reset the
    // 14-day clock, or the rule would never fire for anyone using this feature.
    account::unlock_with(&state, &password, false)
}
