// SPDX-License-Identifier: AGPL-3.0-or-later
//! Unlock backoff (SPEC-V1 §3.6, ADD-003 §③).
//!
//! Three free attempts, then `min(5 × 2^(n−4), 900)` seconds. A successful
//! unlock **resets** the counter to zero rather than decrementing it.
//!
//! What this defends against, stated plainly because it changes how much
//! engineering it deserves: **a person at the keyboard, and nothing more.** An
//! attacker holding the file attacks it offline with their own code, where our
//! counter does not exist; the Argon2 parameters are the only defence there.
//! SPEC-V1 §2 is explicit that this control is not load-bearing, so there is
//! deliberately no tamper-detection around the counter — it lives in the
//! plaintext `app_state` table where the same attacker can simply reset it, and
//! that is fine.
//!
//! Biometric attempts use a separate counter and are not gated by this one. The
//! OS enforces biometric retry limits; we do not reimplement them.

use std::time::Duration;

/// Failures allowed before any delay is imposed.
pub const FREE_ATTEMPTS: i64 = 3;

/// Delay after the first non-free failure, in seconds.
pub const BASE_DELAY_SECS: i64 = 5;

/// Longest delay, in seconds (15 minutes).
pub const MAX_DELAY_SECS: i64 = 900;

/// Delay owed after `failures` consecutive failures.
///
/// ```text
///   failures  1..=3   none
///          4          5s
///          5          10s
///          6          20s
///          7          40s
///        ...
///         12          900s   (capped)
/// ```
#[must_use]
pub fn delay_after(failures: i64) -> Duration {
    if failures <= FREE_ATTEMPTS {
        return Duration::ZERO;
    }
    // Saturating rather than wrapping: a corrupt counter from the plaintext
    // table must clamp to the maximum, never wrap round to no delay at all.
    let steps = u32::try_from(failures - FREE_ATTEMPTS - 1).unwrap_or(u32::MAX);
    let secs = BASE_DELAY_SECS
        .checked_mul(2_i64.saturating_pow(steps.min(62)))
        .unwrap_or(MAX_DELAY_SECS)
        .clamp(0, MAX_DELAY_SECS);
    Duration::from_secs(secs.unsigned_abs())
}

/// Milliseconds until `until_ms`, or `None` if it has passed.
#[must_use]
pub fn remaining(now_ms: i64, until_ms: i64) -> Option<Duration> {
    let remaining_ms = until_ms.saturating_sub(now_ms);
    if remaining_ms <= 0 {
        None
    } else {
        Some(Duration::from_millis(remaining_ms.unsigned_abs()))
    }
}
