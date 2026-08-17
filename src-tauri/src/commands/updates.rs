//! Update commands (SPEC-V1 §6, §7.7).
//!
//! §7.7 is the third and last permitted outbound request, and an explicit
//! carve-out rather than an oversight: *"A password manager with no patch channel
//! has 'hope users re-download the DMG' as its only response to a dependency
//! CVE."*
//!
//! **Nothing here verifies a signature by hand.** Downloading, verifying and
//! applying are `tauri-plugin-updater`'s job, and CLAUDE.md §4.1 is why — a
//! hand-rolled manifest verifier is exactly the sort of novel construction that
//! rule exists to prevent. The plugin refuses to install anything whose minisign
//! signature does not verify against the public key compiled into
//! `tauri.conf.json`, and there is no code path here that can skip that.
//!
//! What this module contributes is the part that is ours:
//!
//! - **The cadence.** [`crate::services::updater::decide`], driven by
//!   `app_state.last_update_check_at` — §4.5's pre-unlock carve-out, which is
//!   where §4.5 already puts this key. So the check works with the vault locked,
//!   as §7.7 requires.
//! - **A fail-closed guard on what may be offered.**
//!   [`crate::services::updater::offerable`] refuses anything that is not strictly
//!   newer than this build, so an endpoint serving an older signed artefact cannot
//!   prompt every user to "update" into a known vulnerability.
//! - **Nothing added to the request.** §7.7: the endpoint learns *"IP, version and
//!   platform — nothing else, no identifier."* The plugin builds the request from
//!   the configured URL; no code here contributes a parameter, and none may start
//!   to.
//!
//! **Not yet configured.** `tauri.conf.json` carries an empty `plugins.updater`,
//! because a real one needs a signing keypair whose private half belongs in the
//! release pipeline and not in this repository. Until an endpoint and public key
//! are set, every command here reports [`AppError::FeatureUnavailable`] — the same
//! honest-failure pattern as the missing wordlist, rather than a stub that appears
//! to work.

// Tauri owns these signatures; see the note in `items.rs`.
#![allow(clippy::needless_pass_by_value)]

use keyring_store::AppStateKey;
use tauri::{AppHandle, Emitter as _, State};
use tauri_plugin_updater::UpdaterExt as _;

use crate::commands::dto::{UpdateCheckDto, UpdateInfoDto, UpdateStatusDto};
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::updater::{self, Decision, Skipped};

/// Whether update checks are on.
///
/// **A constant, and that is a known gap.** §7.7 requires the check to be
/// disableable, and the toggle has nowhere to live yet: §4.5's `app_state` key list
/// is exhaustive and does not include one, and the encrypted settings blob cannot
/// be read before unlock — which is precisely when §7.7 wants the check to run.
/// Resolving that is a spec conversation, not something to decide here by adding a
/// key.
///
/// It is safe to leave as `true` today only because no endpoint is configured, so
/// no request can be made whether this is `true` or `false`. **It must not stay a
/// constant past the commit that configures an endpoint.**
const CHECKS_ENABLED: bool = true;

/// Progress event emitted while an update downloads.
const PROGRESS_EVENT: &str = "update://progress";

/// Ask the endpoint whether a newer build exists (SPEC-V1 §7.7).
///
/// Runs with the vault locked. Enforces §7.7's cadence itself — at most one request
/// per 24 hours — and reports *why* nothing happened when nothing did, so the UI
/// never has to guess between "you are up to date", "checked recently" and "could
/// not reach the endpoint". Being offline is a status, not an error.
///
/// A candidate that is not strictly newer than this build is reported as
/// `upToDate`. See [`updater::offerable`] for why that guard exists on top of the
/// plugin's own comparison.
///
/// # Errors
///
/// [`AppError::FeatureUnavailable`] until an endpoint and public key are
/// configured. [`AppError::Storage`] if the cadence stamp cannot be read or
/// written.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdateCheckDto, AppError> {
    let current = updater::current_version().to_owned();
    let now = state.session.now_ms();
    let last = last_check(&state)?;

    let next_eligible_at = last.saturating_add(updater::MIN_INTERVAL_MS);
    let mut dto = UpdateCheckDto {
        status: UpdateStatusDto::UpToDate,
        current_version: current.clone(),
        available: None,
        checked_at: (last > 0).then_some(last),
        next_eligible_at,
    };

    match updater::decide(CHECKS_ENABLED, last, now) {
        Decision::Skip(Skipped::Disabled) => {
            dto.status = UpdateStatusDto::Disabled;
            return Ok(dto);
        }
        Decision::Skip(Skipped::TooSoon { next_eligible_at }) => {
            dto.status = UpdateStatusDto::CheckedRecently;
            dto.next_eligible_at = next_eligible_at;
            return Ok(dto);
        }
        Decision::Check => {}
    }

    // `updater()` fails with `EmptyEndpoints` while `plugins.updater` has none,
    // which is the not-configured case rather than a runtime failure.
    let updater_handle = app.updater().map_err(|_| AppError::FeatureUnavailable)?;

    // Unreachable, malformed manifest, or a signature the plugin refused — all the
    // same to the caller: we do not know, and we did not install anything. The
    // clock is deliberately not stamped, so the next launch tries again rather than
    // waiting a day on a failure.
    let Ok(found) = updater_handle.check().await else {
        dto.status = UpdateStatusDto::CheckFailed;
        return Ok(dto);
    };

    stamp_check(&state, now)?;
    dto.checked_at = Some(now);
    dto.next_eligible_at = now.saturating_add(updater::MIN_INTERVAL_MS);

    if let Some(update) = found {
        if updater::offerable(&current, &update.version) {
            dto.status = UpdateStatusDto::Available;
            dto.available = Some(UpdateInfoDto {
                version: update.version.clone(),
                notes: update.body.clone(),
                published_at: update.date.map(|d| d.to_string()),
            });
        }
        // Otherwise: the endpoint offered something not strictly newer. Left as
        // `upToDate`, because from the user's point of view that is the truth, and
        // a prompt to "update" to an equal-or-older build is the failure mode this
        // guard exists for.
    }

    Ok(dto)
}

/// Download, verify and apply the pending update (SPEC-V1 §7.7).
///
/// Re-runs the check rather than installing a candidate discovered earlier. That is
/// deliberate: the manifest and its signature are fetched and verified again at the
/// moment of install, so a result cached from minutes ago cannot be what gets
/// applied, and there is no `Update` handle sitting in application state for
/// anything to tamper with.
///
/// Ignores the 24-hour cadence, because the user just asked for this. The cadence
/// governs *unattended* checks.
///
/// Does not require the vault to be unlocked, per §7.7.
///
/// Emits `update://progress` with `{ downloaded, total }` as bytes arrive, so the
/// UI can show real progress rather than a spinner. `total` is `null` when the
/// endpoint sends no `Content-Length`.
///
/// **One §7.7 requirement is not met: this is not resumable.** Tauri's updater
/// downloads to a temporary file and starts over if the transfer fails. Making it
/// resumable means replacing the plugin's downloader, which means owning the
/// signature-verification path — a trade CLAUDE.md §4.1 says not to make on our own
/// authority. Flagged rather than papered over.
///
/// # Errors
///
/// [`AppError::FeatureUnavailable`] until an endpoint and public key are
/// configured; [`AppError::NotFound`] if no update is available;
/// [`AppError::UpdateFailed`] if the download, the signature check or the install
/// failed — one discriminant for all three, on purpose.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), AppError> {
    let updater_handle = app.updater().map_err(|_| AppError::FeatureUnavailable)?;

    let update = updater_handle
        .check()
        .await
        .map_err(|_| AppError::UpdateFailed)?
        .ok_or(AppError::NotFound)?;

    let current = updater::current_version();
    if !updater::offerable(current, &update.version) {
        // The same guard as `update_check`, applied again at the point of no
        // return. A check that passed the guard and an install that skipped it
        // would make the guard advisory.
        return Err(AppError::NotFound);
    }

    let progress = app.clone();
    update
        .download_and_install(
            move |downloaded, total| {
                // A failed emit is not a reason to abort a verified install; the
                // user loses a progress bar, not the update.
                let _ = progress.emit(PROGRESS_EVENT, (downloaded, total));
            },
            || {},
        )
        .await
        .map_err(|_| AppError::UpdateFailed)?;

    Ok(())
}

/// The stored cadence stamp, or `0` if there is nothing to read.
///
/// A missing vault file is not an error here. §7.7 says the updater must not
/// require an unlocked vault, and by the same reasoning a first-run user with no
/// vault at all must still be able to receive a patch.
fn last_check(state: &State<'_, AppState>) -> Result<i64, AppError> {
    match state.session.file() {
        Ok(file) => Ok(file.state_get_i64(AppStateKey::LastUpdateCheckAt)?),
        Err(_) => Ok(0),
    }
}

/// Record that the endpoint was reached. Silently skipped if there is no vault.
fn stamp_check(state: &State<'_, AppState>, now_ms: i64) -> Result<(), AppError> {
    if let Ok(file) = state.session.file() {
        file.state_set_i64(AppStateKey::LastUpdateCheckAt, now_ms)?;
    }
    Ok(())
}
