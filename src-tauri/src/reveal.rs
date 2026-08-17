//! The reveal rate limit (SPEC-V1 §6).
//!
//! > **Reveal rate limit:** 20 reveals in any rolling 60 s, globally. Exceeding
//! > it does not reject — it requires biometric or master-password re-auth for
//! > the next reveal. Rejecting outright is user-hostile; re-auth is an actual
//! > control.
//!
//! The distinction matters, so it is worth stating what this defends against and
//! what it does not. It does **not** defend against a compromised webview: that
//! attacker is inside the trust boundary and §2 says so plainly. What it catches
//! is a script or a stuck UI walking the whole vault one field at a time — the
//! shape of an exfiltration attempt through the one sanctioned plaintext path —
//! and it answers by asking the human whether they meant it.
//!
//! Rolling rather than fixed-window on purpose. A fixed 60-second bucket lets an
//! attacker take 40 reveals across a boundary by waiting for the reset, which is
//! twice the budget the spec set.
//!
//! Counted **globally**, not per item: 20 reveals of one password is a person
//! fighting their own typing, while 20 reveals across 20 items is the thing this
//! exists to notice.

use std::collections::VecDeque;
use std::time::Duration;

/// The rolling window (SPEC-V1 §6).
pub const WINDOW: Duration = Duration::from_secs(60);

/// Reveals permitted inside [`WINDOW`] before re-auth is required.
pub const LIMIT: usize = 20;

/// Whether a reveal may proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Go ahead.
    Allowed,
    /// The window is full. The caller must re-authenticate before the next
    /// reveal, and the reveal it asked for did not happen.
    ReauthRequired,
}

/// A rolling-window counter over reveal timestamps.
///
/// Holds instants and nothing else — no item ids, no field names. A record of
/// *which* secrets a user looked at and when is exactly the inventory this
/// product exists to keep out of reach, and it has no business in a rate
/// limiter (the encrypted `activity` table is where a user-visible history
/// belongs, SPEC-V1 §4.3).
#[derive(Debug, Default)]
pub struct RevealLimiter {
    /// Unix milliseconds, oldest first.
    at: VecDeque<i64>,
    /// Set when the limit is hit, cleared by a successful re-auth.
    reauth_pending: bool,
}

impl RevealLimiter {
    /// An empty limiter.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            at: VecDeque::new(),
            reauth_pending: false,
        }
    }

    /// Decide whether a reveal at `now_ms` may proceed, recording it if so.
    ///
    /// Returns [`Gate::ReauthRequired`] without recording anything when the
    /// window is full: a refused attempt must not extend the window, or a caller
    /// that keeps trying would never be allowed through again.
    pub fn check(&mut self, now_ms: i64) -> Gate {
        self.prune(now_ms);
        if self.reauth_pending || self.at.len() >= LIMIT {
            self.reauth_pending = true;
            return Gate::ReauthRequired;
        }
        self.at.push_back(now_ms);
        Gate::Allowed
    }

    /// Whether the next reveal needs re-authentication.
    #[must_use]
    pub const fn reauth_pending(&self) -> bool {
        self.reauth_pending
    }

    /// Record a successful re-authentication.
    ///
    /// Clears the window as well as the flag. Leaving the timestamps in place
    /// would mean the very next reveal re-tripped the limit, which would turn
    /// re-auth into the rejection the spec ruled out.
    pub fn reauthenticated(&mut self) {
        self.at.clear();
        self.reauth_pending = false;
    }

    /// Forget everything. Called on lock.
    pub fn reset(&mut self) {
        self.at.clear();
        self.reauth_pending = false;
    }

    /// Drop timestamps that have fallen out of the window.
    fn prune(&mut self, now_ms: i64) {
        let cutoff = now_ms.saturating_sub(window_ms());
        while self.at.front().is_some_and(|&t| t <= cutoff) {
            self.at.pop_front();
        }
    }
}

/// [`WINDOW`] in milliseconds.
#[must_use]
pub const fn window_ms() -> i64 {
    // `as` rather than try_into: WINDOW is a compile-time constant of 60 s and
    // this is a const fn, where the fallible conversion is not available. The
    // value is 60_000.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    {
        WINDOW.as_millis() as i64
    }
}
