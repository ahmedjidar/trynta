//! Security-report commands (SPEC-V1 §6, §7.4).
//!
//! This module reads every login's password, which makes it the second-largest
//! concentration of plaintext in the app after unlock itself. Three things keep
//! that contained:
//!
//! - **Every password stays in a `Zeroizing` buffer owned by this function.**
//!   `ItemUnderReview` borrows rather than copies, so a report over 500 items
//!   makes no second copy of any password, and the whole set is wiped when the
//!   command returns.
//! - **Nothing derived from a password crosses IPC** except the two figures §7.4
//!   asks to display: a breach count and a crack-time estimate. Reuse is reported
//!   as *groups of item ids*, never as the shared value or a hash of it.
//! - **The report cannot make a network request.** It is handed a
//!   [`CachedOnly`] source, which has no transport at all. AC14 requires zero
//!   requests from a report, and this makes that structural rather than a promise
//!   someone has to keep.

// Tauri owns these signatures; see the note in `items.rs`.
#![allow(clippy::needless_pass_by_value)]

use std::collections::{BTreeMap, BTreeSet};

use keyring_store::{AppCacheKey, AppStateKey, IndexRow, ItemKind, SecretField, StoreError};
use tauri::State;
use zeroize::Zeroizing;

use crate::commands::dto::{BreachCheckDto, ReuseGroupDto, RiskDto, SecurityReportDto};
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::breach::{self, BreachCache, CachedOnly};
use crate::services::hibp::HibpClient;
use crate::services::report::{self, HealthScore, ItemUnderReview};

/// Narrow a `usize` for the wire.
///
/// Saturating rather than wrapping: a count that arrives as `u32::MAX` is
/// obviously wrong, where a wrapped one looks plausible.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Run the security report (SPEC-V1 §7.4).
///
/// Makes **no network requests**. It scores against whatever HIBP ranges are
/// already in the encrypted `app_cache`, and reports anything it has no range for
/// as `notChecked` rather than as clean — §7.4: *"Offline → 'not checked,' never
/// 'safe.'"* Refreshing that cache is `security_breach_check`, a separate
/// user-initiated command, so opening this screen can never be the thing that
/// talks to the network.
///
/// `twoFactorCapable` is `0` for now, which redistributes §7.4's 20-point 2FA
/// weight into 43.75 / 31.25 / 25 across the other three terms. See
/// [`report::assess_all`] for why that is the honest placeholder.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is locked; [`AppError::Store`] if an item
/// fails to decrypt.
#[tauri::command]
pub fn security_report_run(state: State<'_, AppState>) -> Result<SecurityReportDto, AppError> {
    let now = state.session.now_ms();

    let dto = state.session.with_session(|session| {
        let cache = session
            .app_cache_get(AppCacheKey::BreachCache)?
            .map_or_else(BreachCache::default, |bytes| BreachCache::decode(&bytes));

        // Passwords live here and nowhere else for the duration of the call.
        let mut held: Vec<(IndexRow, Zeroizing<String>)> = Vec::new();
        for row in session.index_rows()? {
            if row.kind != ItemKind::Login {
                continue;
            }
            match session.item_secret(row.id, SecretField::Password) {
                // A login with a blank password is not weak, not reused and not
                // breached — it is unfinished. Counting it would dilute every
                // denominator with items the user has nothing to fix on.
                Ok(password) if password.is_empty() => {}
                Ok(password) => held.push((row, password)),
                Err(StoreError::NoSuchField) => {}
                Err(other) => return Err(AppError::from(other)),
            }
        }

        let under_review: Vec<ItemUnderReview<'_>> = held
            .iter()
            .map(|(row, password)| ItemUnderReview {
                id: row.id,
                title: &row.title,
                subtitle: row.subtitle.as_deref().unwrap_or(""),
                password: password.as_str(),
                has_totp: row.has_totp,
            })
            .collect();

        let assessment = report::assess_all(&under_review, &CachedOnly { cache: &cache });

        // Titles for the risk list. Already non-secret — `items_list` returns
        // them — so re-sending them saves the UI a round trip per risk.
        let labels: BTreeMap<_, _> = held
            .iter()
            .map(|(row, _)| (row.id, (row.title.clone(), row.subtitle.clone())))
            .collect();

        let risks = assessment
            .risks
            .iter()
            .map(|risk| {
                let label = labels.get(&risk.item_id);
                RiskDto {
                    item_id: risk.item_id,
                    title: label.map(|l| l.0.clone()).unwrap_or_default(),
                    subtitle: label.and_then(|l| l.1.clone()),
                    kind: risk.kind.into(),
                    breach_count: risk.breach_count,
                    crack_seconds: risk.crack_seconds,
                }
            })
            .collect();

        let (score, breakdown) = match assessment.score {
            HealthScore::NotEnoughData => (None, None),
            HealthScore::Scored { score, breakdown } => (Some(score), Some(breakdown.into())),
        };

        let checked_at = (assessment.inputs.logins > 0 && cache.fetched_at > 0)
            .then_some(cache.fetched_at);

        Ok(SecurityReportDto {
            score,
            breakdown,
            logins: count(assessment.inputs.logins),
            breached: count(assessment.inputs.breached),
            weak: count(assessment.inputs.weak),
            reused: count(assessment.inputs.reused),
            two_factor_capable: count(assessment.inputs.two_factor_capable),
            two_factor_enabled: count(assessment.inputs.two_factor_enabled),
            not_checked: count(assessment.not_checked),
            risks,
            reuse_groups: assessment
                .groups
                .iter()
                .map(|group| ReuseGroupDto {
                    item_ids: group.items.clone(),
                })
                .collect(),
            breach_checked_at: checked_at,
            breach_refresh_available: BreachCache::may_refresh(cache.fetched_at, now),
        })
    })?;

    state.session.touch();
    Ok(dto)
}

/// Refresh the HIBP range cache (SPEC-V1 §7.4).
///
/// This is the *only* command that talks to HIBP, and one of exactly three
/// outbound requests in the product (CLAUDE.md §4.7). Everything about what leaves
/// the machine is in [`crate::services::hibp`].
///
/// **Cadence.** §7.4 is explicit that a daily background check is impossible while
/// locked, and prescribes the honest alternative: *at most once per 24 h, on the
/// first unlock after the interval elapses.* So the frontend calls this once after
/// unlock and this function decides whether anything happens. Inside the interval
/// it returns `ran: false` and makes no request. There is no `force` parameter,
/// because a button that bypasses the cadence is the cadence not existing.
///
/// **The cache is rebuilt, not appended to.** Only the prefixes of passwords the
/// vault currently holds survive a refresh. §7.4 calls a plaintext cache of these
/// prefixes *"a filter that massively narrows an offline attack"* — ours is
/// encrypted, but the same logic says not to keep prefixes for passwords the user
/// has already changed or deleted. A prefix whose request fails keeps its previous
/// body rather than being dropped, so one failed request cannot cost a usable
/// answer.
///
/// **It runs long.** One request per distinct password, sequentially. It is the
/// only command on the surface that can take tens of seconds, and it must not be
/// on a path the user is waiting behind.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is locked; [`AppError::Store`] if an item
/// fails to decrypt or the cache cannot be written. A failed *request* is not an
/// error — it is a count in the result, because one unreachable prefix must not
/// fail a check over hundreds of items.
#[tauri::command]
pub fn security_breach_check(state: State<'_, AppState>) -> Result<BreachCheckDto, AppError> {
    let now = state.session.now_ms();
    let last = state
        .session
        .file()?
        .state_get_i64(AppStateKey::LastBreachCheckAt)?;
    let attempting = BreachCache::may_refresh(last, now);

    // ── Phase A, under the session lock: what needs fetching ──
    //
    // Deliberately short. The network loop below must not hold the lock that
    // auto-lock and every other command need.
    let (previous, wanted) = state.session.with_session(|session| {
        let previous = session
            .app_cache_get(AppCacheKey::BreachCache)?
            .map_or_else(BreachCache::default, |bytes| BreachCache::decode(&bytes));

        let mut wanted = BTreeSet::new();
        if attempting {
            for row in session.index_rows()? {
                if row.kind != ItemKind::Login {
                    continue;
                }
                match session.item_secret(row.id, SecretField::Password) {
                    Ok(password) if password.is_empty() => {}
                    // The password goes no further than this line: `split` keeps
                    // the suffix in a `Zeroizing` buffer that is dropped here, and
                    // only the five characters §7.4 permits are carried out.
                    Ok(password) => {
                        wanted.insert(breach::split(&password).0);
                    }
                    Err(StoreError::NoSuchField) => {}
                    Err(other) => return Err(AppError::from(other)),
                }
            }
        }
        Ok((previous, wanted))
    })?;

    // ── Phase B, no lock held: the requests ──
    //
    // The decisions about what survives a refresh are in `breach::refresh`, where
    // they can be tested against a source that fails on demand.
    let refreshed = if wanted.is_empty() {
        breach::Refreshed {
            cache: previous.clone(),
            fetched: 0,
            failed: 0,
        }
    } else {
        breach::refresh(&previous, &wanted, &HibpClient::new(), now)
    };

    let ran = refreshed.fetched > 0;
    let checked_at = refreshed.cache.fetched_at;

    // ── Phase C, under the lock again: persist ──
    if attempting {
        state.session.with_session(|session| {
            let encoded = refreshed.cache.encode().ok_or(AppError::Storage)?;
            session.app_cache_put(AppCacheKey::BreachCache, &encoded)?;
            Ok::<(), AppError>(())
        })?;
        if ran {
            state
                .session
                .file()?
                .state_set_i64(AppStateKey::LastBreachCheckAt, now)?;
        }
    }

    state.session.touch();
    Ok(BreachCheckDto {
        ran,
        checked_at: (checked_at > 0).then_some(checked_at),
        next_eligible_at: checked_at.saturating_add(breach::MIN_INTERVAL_MS),
        prefixes_requested: count(wanted.len()),
        prefixes_fetched: count(refreshed.fetched),
        prefixes_failed: count(refreshed.failed),
        cached_prefixes: count(refreshed.cache.len()),
    })
}
