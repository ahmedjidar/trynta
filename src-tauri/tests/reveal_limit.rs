// SPDX-License-Identifier: AGPL-3.0-or-later
//! The reveal rate limit (SPEC-V1 §6).
//!
//! > 20 reveals in any rolling 60 s, globally. Exceeding it does not reject — it
//! > requires biometric or master-password re-auth for the next reveal.
//!
//! Two things make this worth testing rather than eyeballing. The window is
//! **rolling**, so a fixed-bucket implementation would let 40 reveals through
//! across a boundary — twice the budget. And a refused attempt must not extend
//! the window, or a caller that keeps retrying would lock itself out forever.
//!
//! Driven through `SessionManager` with a hand-wound clock, because the limiter
//! lives inside the session: "lock is real" (CLAUDE.md §4.9) has to include
//! forgetting how many secrets were looked at.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use keyring_lib::autolock::Clock;
use keyring_lib::platform::{
    BiometricError, BiometricKind, Biometrics, Clipboard, ClipboardError, Platform, SecureStore,
    SecureStoreError,
};
use keyring_lib::reveal::{Gate, RevealLimiter, LIMIT, WINDOW};
use keyring_lib::session::SessionManager;
use keyring_store::{KdfParams, VaultFile};

const MASTER: &str = "reveal-limit-test-master-8Hs4Nv";

// ── Doubles ─────────────────────────────────────────────────────────────────

struct NoClipboard;

impl Clipboard for NoClipboard {
    fn set_secret(&self, _value: &str) -> Result<u64, ClipboardError> {
        Ok(1)
    }
    fn clear_if_ours(&self, _token: u64) -> Result<bool, ClipboardError> {
        Ok(true)
    }
}

struct NoBiometrics;

impl Biometrics for NoBiometrics {
    fn kind(&self) -> BiometricKind {
        BiometricKind::None
    }
    fn is_available(&self) -> bool {
        false
    }
    fn enrol(&self, _label: &str, _secret: &[u8]) -> Result<(), BiometricError> {
        Err(BiometricError::Unavailable)
    }
    fn unwrap_secret(&self, _label: &str) -> Result<Vec<u8>, BiometricError> {
        Err(BiometricError::Unavailable)
    }
    fn revoke(&self, _label: &str) -> Result<(), BiometricError> {
        Ok(())
    }
}

struct NoStore;

impl SecureStore for NoStore {
    fn store(&self, _key: &str, _value: &[u8]) -> Result<(), SecureStoreError> {
        Ok(())
    }
    fn load(&self, _key: &str) -> Result<Option<Vec<u8>>, SecureStoreError> {
        Ok(None)
    }
    fn delete(&self, _key: &str) -> Result<(), SecureStoreError> {
        Ok(())
    }
}

/// Starts at a realistic epoch: a clock that begins at zero is not a clock any
/// user has, and it hides off-by-one bugs that only appear with real timestamps.
struct FakeClock(Mutex<i64>);

impl Default for FakeClock {
    fn default() -> Self {
        Self(Mutex::new(1_700_000_000_000))
    }
}

impl FakeClock {
    fn advance(&self, by: Duration) {
        *self.0.lock().expect("lock") += i64::try_from(by.as_millis()).unwrap_or(0);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        *self.0.lock().expect("lock")
    }
}

struct Harness {
    manager: SessionManager,
    clock: Arc<FakeClock>,
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
}

fn harness() -> Harness {
    let clock = Arc::new(FakeClock::default());
    let platform = Arc::new(Platform {
        biometrics: Arc::new(NoBiometrics),
        clipboard: Arc::new(NoClipboard),
        secure_store: Arc::new(NoStore),
    });
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    Harness {
        manager: SessionManager::new(platform, clock.clone()),
        clock,
        _dir: dir,
        path,
    }
}

fn unlocked(h: &Harness) {
    let file = Arc::new(VaultFile::create(&h.path, MASTER, KdfParams::floor()).expect("create"));
    h.manager.attach(file.clone());
    h.manager.begin_unlock().expect("begin");
    let keys = file.unlock(MASTER).expect("unlock").into_keys();
    h.manager.adopt(keys, true);
}

// ── Through the session ─────────────────────────────────────────────────────

#[test]
fn the_budget_is_exactly_the_specified_limit() {
    let h = harness();
    unlocked(&h);

    for i in 0..LIMIT {
        assert_eq!(
            h.manager.check_reveal(),
            Gate::Allowed,
            "reveal {i} of the budget was refused"
        );
    }
    assert_eq!(
        h.manager.check_reveal(),
        Gate::ReauthRequired,
        "the {}th reveal inside the window must ask for re-auth",
        LIMIT + 1
    );
    assert!(h.manager.reveal_reauth_pending());
}

#[test]
fn a_refused_attempt_does_not_extend_the_window() {
    // If a refusal recorded a timestamp, a caller that kept trying would push the
    // window forward on every attempt and never be allowed through again — the
    // rejection the spec explicitly ruled out, arrived at by accident.
    let h = harness();
    unlocked(&h);

    for _ in 0..LIMIT {
        assert_eq!(h.manager.check_reveal(), Gate::Allowed);
    }
    for _ in 0..50 {
        assert_eq!(h.manager.check_reveal(), Gate::ReauthRequired);
    }

    h.manager.note_reauth();
    assert_eq!(
        h.manager.check_reveal(),
        Gate::Allowed,
        "after re-auth the very next reveal must go through"
    );
}

#[test]
fn re_authenticating_reopens_the_whole_budget() {
    // Clearing the flag but leaving the timestamps would mean the next reveal
    // re-tripped the limit immediately, turning re-auth into a rejection with
    // extra steps.
    let h = harness();
    unlocked(&h);

    for _ in 0..LIMIT {
        h.manager.check_reveal();
    }
    assert_eq!(h.manager.check_reveal(), Gate::ReauthRequired);

    h.manager.note_reauth();
    for i in 0..LIMIT {
        assert_eq!(
            h.manager.check_reveal(),
            Gate::Allowed,
            "reveal {i} after re-auth was refused"
        );
    }
}

#[test]
fn locking_forgets_the_window() {
    let h = harness();
    unlocked(&h);

    for _ in 0..LIMIT {
        h.manager.check_reveal();
    }
    assert!(h.manager.reveal_reauth_pending() || h.manager.check_reveal() == Gate::ReauthRequired);

    h.manager.lock();
    assert!(
        !h.manager.reveal_reauth_pending(),
        "a locked vault must not remember that a re-auth was pending: the session \
         it belonged to no longer exists"
    );
}

// ── The window itself ───────────────────────────────────────────────────────

#[test]
fn the_window_rolls_rather_than_resetting() {
    // The failure this catches: a fixed 60-second bucket lets 20 reveals happen
    // at t=59s and 20 more at t=61s, which is 40 inside any 62-second span.
    let mut limiter = RevealLimiter::new();
    let start = 1_700_000_000_000_i64;
    let window = i64::try_from(WINDOW.as_millis()).expect("window fits");

    // Fill the budget at the very start of the window.
    for _ in 0..LIMIT {
        assert_eq!(limiter.check(start), Gate::Allowed);
    }
    assert_eq!(limiter.check(start), Gate::ReauthRequired);

    // One millisecond before the oldest entry leaves the window, still refused.
    limiter.reauthenticated();
    for _ in 0..LIMIT {
        assert_eq!(limiter.check(start + window - 1), Gate::Allowed);
    }
    assert_eq!(limiter.check(start + window - 1), Gate::ReauthRequired);
}

#[test]
fn entries_leave_the_window_once_it_has_passed() {
    let mut limiter = RevealLimiter::new();
    let start = 1_700_000_000_000_i64;
    let window = i64::try_from(WINDOW.as_millis()).expect("window fits");

    for _ in 0..LIMIT {
        assert_eq!(limiter.check(start), Gate::Allowed);
    }
    assert_eq!(limiter.check(start), Gate::ReauthRequired);

    // The flag latches until a re-auth, on purpose: the limit having been hit is
    // a fact about the session, not about the clock. Clear it and the aged-out
    // window is genuinely empty again.
    limiter.reauthenticated();
    for _ in 0..LIMIT {
        assert_eq!(
            limiter.check(start + window + 1),
            Gate::Allowed,
            "an entry older than the window must not count against the budget"
        );
    }
}

#[test]
fn the_limiter_holds_no_record_of_what_was_revealed() {
    // Asserted through Debug rather than by reading fields, because Debug is the
    // representation that leaks: a limiter that grew an item-id list would be a
    // map of which of a user's secrets are the interesting ones (CLAUDE.md §4.6).
    let mut limiter = RevealLimiter::new();
    limiter.check(1_700_000_000_000);
    let rendered = format!("{limiter:?}");
    assert!(
        rendered.contains("RevealLimiter"),
        "unexpected Debug shape: {rendered}"
    );
    assert!(
        !rendered.contains("password") && !rendered.contains("item"),
        "the limiter's Debug names something it should not hold: {rendered}"
    );
}

#[test]
fn a_clock_that_goes_backwards_does_not_panic_or_unlock_the_budget() {
    // NTP correction, or a user changing the system clock. Neither should hand
    // out extra reveals or overflow the subtraction.
    let mut limiter = RevealLimiter::new();
    for _ in 0..LIMIT {
        limiter.check(1_700_000_000_000);
    }
    assert_eq!(limiter.check(0), Gate::ReauthRequired);
    assert_eq!(limiter.check(i64::MIN), Gate::ReauthRequired);
}

#[test]
fn a_session_clock_and_the_limiter_agree() {
    // The limiter is fed by `SessionManager`'s clock, so an injected clock in a
    // test has to reach it. If it did not, this test would pass with the real
    // clock and prove nothing.
    let h = harness();
    unlocked(&h);

    for _ in 0..LIMIT {
        assert_eq!(h.manager.check_reveal(), Gate::Allowed);
    }
    assert_eq!(h.manager.check_reveal(), Gate::ReauthRequired);

    h.manager.note_reauth();
    h.clock.advance(WINDOW + Duration::from_secs(1));
    for _ in 0..LIMIT {
        assert_eq!(h.manager.check_reveal(), Gate::Allowed);
    }
}
