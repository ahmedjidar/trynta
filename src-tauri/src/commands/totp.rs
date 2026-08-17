//! The live one-time code (SPEC-V1 §6, §7.2).
//!
//! One command, and the shape of it is the point: `totp_current` returns a
//! **code**, never the seed it came from. The seed is a secret field like any
//! other — reachable only through `item_reveal_field` on explicit user action —
//! while a 30-second code is not, because it is already on the user's screen in
//! their authenticator and expires on its own.
//!
//! The store hands back the full `TotpConfig`, seed included, so this module is on
//! a secret path: it decrypts, computes, and drops. Nothing here caches the config
//! and nothing returns it.
//!
//! Time comes from the session's clock rather than being read here, so an injected
//! clock in a test reaches this the same way it reaches auto-lock.

// Tauri owns these signatures. See commands/generator.rs.
#![allow(clippy::needless_pass_by_value)]

use tauri::State;
use uuid::Uuid;

use crate::commands::dto::TotpCodeDto;
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::totp::{self, Algorithm, TotpConfig};

/// Milliseconds per second, for turning the session clock into a TOTP counter.
const MS_PER_SECOND: i64 = 1_000;

/// Translate the store's stored configuration into the service's.
///
/// Two `TotpConfig` types exist on purpose. The store's is the persisted shape,
/// pinned by the frozen acceptance contract; the service's is what RFC 6238 needs.
/// Converting at the boundary keeps `keyring-store` free of the HMAC
/// implementation and keeps `services::totp` testable against the RFC's vectors
/// without a vault.
fn to_service(stored: &keyring_store::TotpConfig) -> TotpConfig {
    TotpConfig {
        secret: stored.secret.clone(),
        algorithm: match stored.algorithm {
            keyring_store::TotpAlgorithm::Sha1 => Algorithm::Sha1,
            keyring_store::TotpAlgorithm::Sha256 => Algorithm::Sha256,
            keyring_store::TotpAlgorithm::Sha512 => Algorithm::Sha512,
        },
        digits: stored.digits,
        period_seconds: stored.period_seconds,
        issuer: stored.issuer.clone(),
        account: stored.account.clone(),
    }
}

/// The current one-time code for an item, with its countdown.
///
/// Returns [`AppError::NotFound`] when the item has no TOTP configuration, which
/// includes the case where a seed was stored without its parameters — guessing
/// SHA-1/6/30 would hand back a plausible code that never works, and a missing
/// configuration is more useful to the user than a wrong number.
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::NotFound`] if the item or its configuration
/// is absent, [`AppError::Invalid`] if the stored configuration is out of range,
/// [`AppError::Storage`] or [`AppError::Crypto`].
#[tauri::command]
pub fn totp_current(state: State<'_, AppState>, id: Uuid) -> Result<TotpCodeDto, AppError> {
    let now_seconds = state.session.now_ms() / MS_PER_SECOND;
    let unix_seconds = u64::try_from(now_seconds).unwrap_or(0);

    let code = state.session.with_session(|s| {
        let stored = s.item_totp(id)?.ok_or(AppError::NotFound)?;
        // `stored` holds the seed and is dropped at the end of this closure. The
        // code is all that leaves.
        totp::code_at(&to_service(&stored), unix_seconds).map_err(|e| match e {
            totp::TotpError::Digits | totp::TotpError::Period | totp::TotpError::Uri => {
                AppError::Invalid
            }
            _ => AppError::Crypto,
        })
    })?;

    state.session.touch();
    Ok(code.into())
}
