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

use crate::commands::dto::{TotpAlgorithmDto, TotpCodeDto, TotpConfigInput, TotpRejectionDto};
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::base32;
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

/// Read a one-time-code setup the user pasted, as a URI or a bare secret.
///
/// Sites hand out two things and call both "the code": an `otpauth://totp/...`
/// URI behind the QR image, and — when there is no QR at all — a bare base32
/// string. Both arrive here, and **which one it is is detected rather than
/// asked**, because a user copying a value out of a support page does not know
/// or care which format their bank chose.
///
/// The parse is in Rust and not in the webview. Not for secrecy — the user just
/// pasted the value, so the webview has already seen it, exactly as it sees a
/// typed password (SPEC-V1 §4.1). It is here because this is the only place that
/// can guarantee the parameters reaching `secret_ct` are the ones the URI
/// carried. A TypeScript parser that quietly dropped `algorithm=SHA256` would
/// produce an item that stores SHA-1 and generates codes that never work, which
/// is precisely the bug ADD-004 §④ records having already been shipped once.
///
/// Everything is preserved: secret, algorithm, digits, period, issuer, account.
/// Nothing is defaulted silently except where the URI itself omits a parameter,
/// in which case RFC 6238's defaults apply — SHA-1, 6 digits, 30 seconds — which
/// is what an authenticator app would also assume.
///
/// # Errors
///
/// [`AppError::TotpRejected`] with the rule that failed. Never echoes the input:
/// the input is a shared secret.
#[tauri::command]
pub fn totp_parse(input: String) -> Result<TotpConfigInput, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::TotpRejected {
            reason: TotpRejectionDto::MissingSecret,
        });
    }

    let lower = trimmed.to_ascii_lowercase();

    // Counter-based codes get their own refusal. `parse_uri` would report these
    // as a generic bad URI, and "that is not a valid otpauth:// URI" is a
    // confusing thing to read about a URI that plainly is one.
    if lower.starts_with("otpauth://hotp/") {
        return Err(AppError::TotpRejected {
            reason: TotpRejectionDto::CounterBased,
        });
    }

    if lower.starts_with("otpauth://") {
        let config = totp::parse_uri(trimmed).map_err(|e| AppError::TotpRejected {
            reason: match e {
                totp::TotpError::Secret(base32::Base32Error::Empty) => {
                    TotpRejectionDto::EmptySecret
                }
                totp::TotpError::Secret(base32::Base32Error::Truncated) => {
                    TotpRejectionDto::TruncatedSecret
                }
                totp::TotpError::Secret(base32::Base32Error::InvalidCharacter { .. }) => {
                    TotpRejectionDto::NotBase32
                }
                totp::TotpError::Digits => TotpRejectionDto::UnsupportedDigits,
                totp::TotpError::Period => TotpRejectionDto::UnsupportedPeriod,
                totp::TotpError::Uri | totp::TotpError::Mac => TotpRejectionDto::NotOtpauthUri,
            },
        })?;
        return Ok(from_service(config));
    }

    // Anything else carrying a scheme is a URI we do not implement, and saying so
    // is more use than pretending it might be base32 and failing on the colon.
    if lower.contains("://") {
        return Err(AppError::TotpRejected {
            reason: TotpRejectionDto::NotOtpauthUri,
        });
    }

    // The manual-entry path. Validated by decoding it: a secret that is not
    // base32 cannot produce a code, and finding that out when the user pastes it
    // is much better than finding out every 30 seconds afterwards.
    base32::decode(trimmed).map_err(|e| AppError::TotpRejected {
        reason: match e {
            base32::Base32Error::Empty => TotpRejectionDto::EmptySecret,
            base32::Base32Error::Truncated => TotpRejectionDto::TruncatedSecret,
            base32::Base32Error::InvalidCharacter { .. } => TotpRejectionDto::NotBase32,
        },
    })?;

    Ok(from_service(TotpConfig {
        // Stored as the issuer wrote it, minus the spaces sites add for
        // readability — `base32::decode` ignores whitespace, so a secret that
        // round-trips with spaces in it would work, and then compare unequal
        // against the same secret pasted without them.
        secret: trimmed.split_whitespace().collect::<String>(),
        ..TotpConfig::default()
    }))
}

/// The wire form of a parsed configuration.
///
/// Carries the secret back to the caller, which is the same value it just sent:
/// this is a parse, not a decrypt, and nothing is read from the vault.
fn from_service(c: TotpConfig) -> TotpConfigInput {
    TotpConfigInput {
        secret: c.secret,
        algorithm: match c.algorithm {
            Algorithm::Sha1 => TotpAlgorithmDto::Sha1,
            Algorithm::Sha256 => TotpAlgorithmDto::Sha256,
            Algorithm::Sha512 => TotpAlgorithmDto::Sha512,
        },
        digits: c.digits,
        period_seconds: c.period_seconds,
        issuer: c.issuer,
        account: c.account,
    }
}

/// Attach, replace or remove an item's one-time-code setup.
///
/// Separate from `item_edit_meta` because a TOTP configuration is not metadata:
/// the seed belongs in `secret_ct`, and an edit path that carried it through the
/// metadata envelope would put a shared secret into the search index. Separate
/// from `item_upsert` because that rewrites the whole item and the detail view
/// does not hold the password — asking a user to retype their password in order
/// to add a one-time code would be absurd.
///
/// Pass `null` to remove one. Removing is not destructive to anything else: the
/// password and history are untouched.
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::NotFound`] if the item is absent or is not a
/// login, [`AppError::Storage`] or [`AppError::Crypto`].
#[tauri::command]
pub fn item_set_totp(
    state: State<'_, AppState>,
    id: Uuid,
    totp: Option<TotpConfigInput>,
) -> Result<bool, AppError> {
    let config = totp.map(|t| keyring_store::TotpConfig {
        secret: t.secret,
        algorithm: match t.algorithm {
            TotpAlgorithmDto::Sha1 => keyring_store::TotpAlgorithm::Sha1,
            TotpAlgorithmDto::Sha256 => keyring_store::TotpAlgorithm::Sha256,
            TotpAlgorithmDto::Sha512 => keyring_store::TotpAlgorithm::Sha512,
        },
        digits: t.digits,
        period_seconds: t.period_seconds,
        issuer: t.issuer,
        account: t.account,
    });

    let changed = state
        .session
        .with_session(|s| s.item_set_totp(id, config.as_ref()).map_err(AppError::from))?;
    state.session.touch();
    Ok(changed)
}
