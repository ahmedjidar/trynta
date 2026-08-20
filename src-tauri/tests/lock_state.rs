// SPDX-License-Identifier: AGPL-3.0-or-later
//! The lock/unlock state machine and auto-lock policy (SPEC-V1 §5, §11).
//!
//! Every trigger crossed with every setting, plus the properties that make lock
//! *real* rather than a UI overlay: the keys are gone, the state machine refuses
//! to hand out a session, and the clipboard entry we made is cleared while one
//! the user made is not.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use keyring_lib::autolock::{should_lock, AutoLockSetting, Clock, LockTrigger};
use keyring_lib::error::AppError;
use keyring_lib::platform::{
    BiometricError, BiometricKind, Biometrics, Clipboard, ClipboardError, Platform, SecureStore,
    SecureStoreError,
};
use keyring_lib::session::{SessionError, SessionManager, VaultState};
use keyring_store::{ItemBody, ItemDraft, KdfParams, SecretField, VaultFile};

const MASTER: &str = "lock-state-test-master-6Vn2Qd";

// ── A platform that records what it was asked to do ─────────────────────────

#[derive(Default)]
struct FakeClipboard {
    /// Every `(token, cleared)` pair, so a test can prove what happened rather
    /// than infer it.
    writes: Mutex<Vec<u64>>,
    cleared: Mutex<Vec<u64>>,
    /// Simulates the user copying something after us.
    hijacked: Mutex<bool>,
    next_token: Mutex<u64>,
}

impl Clipboard for FakeClipboard {
    fn set_secret(&self, _value: &str) -> Result<u64, ClipboardError> {
        let mut next = self.next_token.lock().expect("lock");
        *next += 1;
        self.writes.lock().expect("lock").push(*next);
        Ok(*next)
    }

    fn clear_if_ours(&self, token: u64) -> Result<bool, ClipboardError> {
        if *self.hijacked.lock().expect("lock") {
            return Ok(false);
        }
        self.cleared.lock().expect("lock").push(token);
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

/// A clock the test drives by hand, so a 60-minute timeout costs no seconds.
///
/// Starts at a realistic epoch rather than zero: a clock that begins at the
/// epoch is not a clock any user has, and tests that rely on it hide bugs that
/// only appear with real timestamps.
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
    clipboard: Arc<FakeClipboard>,
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
}

fn harness() -> Harness {
    let clock = Arc::new(FakeClock::default());
    let clipboard = Arc::new(FakeClipboard::default());
    let platform = Arc::new(Platform {
        biometrics: Arc::new(NoBiometrics),
        clipboard: clipboard.clone(),
        secure_store: Arc::new(NoStore),
    });
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    Harness {
        manager: SessionManager::new(platform, clock.clone()),
        clock,
        clipboard,
        _dir: dir,
        path,
    }
}

/// Create a vault, attach it, and unlock it.
fn unlocked(h: &Harness) -> Arc<VaultFile> {
    let file = Arc::new(VaultFile::create(&h.path, MASTER, KdfParams::floor()).expect("create"));
    h.manager.attach(file.clone());
    h.manager.begin_unlock().expect("begin");
    let keys = file.unlock(MASTER).expect("unlock").into_keys();
    h.manager.adopt(keys, true);
    file
}

// ── The policy table (SPEC-V1 §5.2) ─────────────────────────────────────────

#[test]
fn a_manual_request_locks_under_every_setting_including_never() {
    // `Never` is a statement about automatic locking. Refusing an explicit
    // request would be absurd.
    for setting in [
        AutoLockSetting::Immediately,
        AutoLockSetting::After(5),
        AutoLockSetting::OnSleep,
        AutoLockSetting::OnScreensaver,
        AutoLockSetting::Never,
    ] {
        assert!(
            should_lock(setting, LockTrigger::Manual, Duration::ZERO),
            "{setting:?} refused a manual lock"
        );
    }
}

#[test]
fn sleep_locks_everything_except_never() {
    for setting in [
        AutoLockSetting::Immediately,
        AutoLockSetting::After(60),
        AutoLockSetting::OnSleep,
        AutoLockSetting::OnScreensaver,
    ] {
        assert!(
            should_lock(setting, LockTrigger::Sleep, Duration::ZERO),
            "{setting:?} ignored sleep"
        );
    }
    assert!(!should_lock(
        AutoLockSetting::Never,
        LockTrigger::Sleep,
        Duration::ZERO
    ));
}

#[test]
fn a_screen_lock_is_honoured_by_everyone_but_never_and_sleep_only() {
    assert!(should_lock(
        AutoLockSetting::OnScreensaver,
        LockTrigger::ScreenLock,
        Duration::ZERO
    ));
    assert!(should_lock(
        AutoLockSetting::After(5),
        LockTrigger::ScreenLock,
        Duration::ZERO
    ));
    assert!(!should_lock(
        AutoLockSetting::OnSleep,
        LockTrigger::ScreenLock,
        Duration::ZERO
    ));
    assert!(!should_lock(
        AutoLockSetting::Never,
        LockTrigger::ScreenLock,
        Duration::ZERO
    ));
}

#[test]
fn the_idle_timer_fires_only_at_or_past_its_timeout() {
    let five = AutoLockSetting::After(5);
    assert!(!should_lock(
        five,
        LockTrigger::Idle,
        Duration::from_secs(299)
    ));
    assert!(should_lock(
        five,
        LockTrigger::Idle,
        Duration::from_secs(300)
    ));
    assert!(should_lock(
        five,
        LockTrigger::Idle,
        Duration::from_secs(301)
    ));

    // "Immediately" means any idle at all.
    assert!(should_lock(
        AutoLockSetting::Immediately,
        LockTrigger::Idle,
        Duration::ZERO
    ));

    // Event-only settings have no timer to fire.
    for setting in [
        AutoLockSetting::OnSleep,
        AutoLockSetting::OnScreensaver,
        AutoLockSetting::Never,
    ] {
        assert!(
            !should_lock(setting, LockTrigger::Idle, Duration::from_secs(86_400)),
            "{setting:?} locked on an idle timer it does not have"
        );
    }
}

#[test]
fn every_interval_the_settings_screen_offers_is_representable() {
    // SPEC-V1 §5.2 lists 1/5/15/30/60.
    for minutes in [1u32, 5, 15, 30, 60] {
        let setting = AutoLockSetting::After(minutes);
        let timeout = setting.idle_timeout().expect("an interval has a timeout");
        assert_eq!(timeout, Duration::from_secs(u64::from(minutes) * 60));
    }
    assert_eq!(AutoLockSetting::default(), AutoLockSetting::After(5));
}

// ── The state machine ───────────────────────────────────────────────────────

#[test]
fn the_states_follow_the_documented_transitions() {
    let h = harness();
    assert_eq!(h.manager.state(), VaultState::Uninitialised);

    let file = Arc::new(VaultFile::create(&h.path, MASTER, KdfParams::floor()).expect("create"));
    h.manager.attach(file.clone());
    assert_eq!(h.manager.state(), VaultState::Locked);

    h.manager.begin_unlock().expect("begin");
    assert_eq!(h.manager.state(), VaultState::Unlocking);

    // A second begin is invalid: two derivations in flight would race to adopt.
    assert!(matches!(
        h.manager.begin_unlock(),
        Err(SessionError::InvalidState(VaultState::Unlocking))
    ));

    let keys = file.unlock(MASTER).expect("unlock").into_keys();
    h.manager.adopt(keys, true);
    assert_eq!(h.manager.state(), VaultState::Unlocked);

    h.manager.lock();
    assert_eq!(h.manager.state(), VaultState::Locked);
}

#[test]
fn a_failed_unlock_returns_to_locked_rather_than_stranding_the_state() {
    let h = harness();
    let file = Arc::new(VaultFile::create(&h.path, MASTER, KdfParams::floor()).expect("create"));
    h.manager.attach(file.clone());

    h.manager.begin_unlock().expect("begin");
    assert!(file.unlock("wrong").is_err());
    h.manager.abort_unlock();
    assert_eq!(h.manager.state(), VaultState::Locked);

    // And a retry is possible, which it would not be from a stranded state.
    h.manager.begin_unlock().expect("retry");
}

#[test]
fn locking_during_a_derivation_is_honoured() {
    // A sleep signal arriving mid-Argon2 must not be dropped on the floor.
    let h = harness();
    let file = Arc::new(VaultFile::create(&h.path, MASTER, KdfParams::floor()).expect("create"));
    h.manager.attach(file);
    h.manager.begin_unlock().expect("begin");
    assert_eq!(h.manager.state(), VaultState::Unlocking);

    h.manager.lock();
    assert_eq!(h.manager.state(), VaultState::Locked);
}

#[test]
fn a_locked_vault_hands_out_no_session() {
    let h = harness();
    unlocked(&h);

    // Unlocked: the door opens.
    let title = h
        .manager
        .with_session(|s| {
            let vault = s.vault_add("Personal", "vault.accent.1")?;
            s.item_upsert(&ItemDraft::new(vault, "a note", ItemBody::SecureNote))?;
            Ok::<_, AppError>("ok")
        })
        .expect("unlocked session");
    assert_eq!(title, "ok");

    h.manager.lock();

    // Locked: it does not.
    let err = h
        .manager
        .with_session(|_| Ok::<_, AppError>(()))
        .expect_err("a locked vault must refuse");
    assert_eq!(err, AppError::Locked);
}

#[test]
fn an_error_inside_a_session_does_not_silently_lock_the_vault() {
    // The keys are taken out of the mutex for the duration of a call. If an
    // error path forgot to put them back, the next call would report a locked
    // vault and the user would be told to re-enter their master password
    // because a query failed.
    let h = harness();
    unlocked(&h);

    let err = h
        .manager
        .with_session(|_| Err::<(), AppError>(AppError::NoVault))
        .expect_err("propagates");
    assert_eq!(err, AppError::NoVault);

    assert_eq!(h.manager.state(), VaultState::Unlocked);
    h.manager
        .with_session(|s| s.vaults_list().map_err(AppError::from))
        .expect("the vault is still usable after an error");
}

// ── Lock is real ────────────────────────────────────────────────────────────

#[test]
fn locking_clears_a_clipboard_entry_we_made() {
    let h = harness();
    unlocked(&h);

    let token = h
        .manager
        .platform()
        .clipboard
        .set_secret("a password")
        .expect("copy");
    h.manager.note_clipboard_write(token);

    h.manager.lock();

    let cleared = h.clipboard.cleared.lock().expect("lock").clone();
    assert_eq!(
        cleared,
        vec![token],
        "lock must clear our own clipboard entry"
    );
}

#[test]
fn locking_does_not_clear_a_clipboard_entry_the_user_made() {
    let h = harness();
    unlocked(&h);

    let token = h
        .manager
        .platform()
        .clipboard
        .set_secret("a password")
        .expect("copy");
    h.manager.note_clipboard_write(token);

    // The user copies something of their own.
    *h.clipboard.hijacked.lock().expect("lock") = true;

    h.manager.lock();
    assert!(
        h.clipboard.cleared.lock().expect("lock").is_empty(),
        "wiping the user's clipboard because our timer expired is its own bug"
    );
}

#[test]
fn locking_twice_is_harmless() {
    let h = harness();
    unlocked(&h);
    h.manager.lock();
    h.manager.lock();
    assert_eq!(h.manager.state(), VaultState::Locked);
}

// ── Triggers driving the manager ────────────────────────────────────────────

#[test]
fn the_idle_trigger_locks_only_once_the_clock_has_moved_far_enough() {
    let h = harness();
    unlocked(&h);
    h.manager.set_setting(AutoLockSetting::After(5));
    h.manager.touch();

    h.clock.advance(Duration::from_secs(299));
    assert!(!h.manager.handle_trigger(LockTrigger::Idle));
    assert_eq!(h.manager.state(), VaultState::Unlocked);

    h.clock.advance(Duration::from_secs(1));
    assert!(h.manager.handle_trigger(LockTrigger::Idle));
    assert_eq!(h.manager.state(), VaultState::Locked);
}

#[test]
fn activity_resets_the_idle_timer() {
    let h = harness();
    unlocked(&h);
    h.manager.set_setting(AutoLockSetting::After(5));

    h.clock.advance(Duration::from_secs(299));
    h.manager.touch();
    h.clock.advance(Duration::from_secs(299));

    assert!(
        !h.manager.handle_trigger(LockTrigger::Idle),
        "touching the vault must restart the timer, not extend it"
    );
    assert_eq!(h.manager.state(), VaultState::Unlocked);
}

#[test]
fn using_a_session_counts_as_activity() {
    let h = harness();
    unlocked(&h);
    h.manager.set_setting(AutoLockSetting::After(5));

    h.clock.advance(Duration::from_secs(299));
    h.manager
        .with_session(|s| s.vaults_list().map_err(AppError::from))
        .expect("query");
    h.clock.advance(Duration::from_secs(299));

    assert!(!h.manager.handle_trigger(LockTrigger::Idle));
}

#[test]
fn sleep_locks_immediately_regardless_of_the_idle_timer() {
    let h = harness();
    unlocked(&h);
    h.manager.set_setting(AutoLockSetting::After(60));
    h.manager.touch();

    assert!(h.manager.handle_trigger(LockTrigger::Sleep));
    assert_eq!(h.manager.state(), VaultState::Locked);
}

#[test]
fn a_trigger_on_an_already_locked_vault_reports_no_lock() {
    let h = harness();
    unlocked(&h);
    h.manager.lock();
    assert!(
        !h.manager.handle_trigger(LockTrigger::Sleep),
        "reporting a lock that did not happen would make the idle timer look like it fired"
    );
}

#[test]
fn never_ignores_every_ambient_trigger_but_not_the_user() {
    let h = harness();
    unlocked(&h);
    h.manager.set_setting(AutoLockSetting::Never);
    h.clock.advance(Duration::from_secs(86_400));

    assert!(!h.manager.handle_trigger(LockTrigger::Idle));
    assert!(!h.manager.handle_trigger(LockTrigger::Sleep));
    assert!(!h.manager.handle_trigger(LockTrigger::ScreenLock));
    assert_eq!(h.manager.state(), VaultState::Unlocked);

    assert!(h.manager.handle_trigger(LockTrigger::Manual));
    assert_eq!(h.manager.state(), VaultState::Locked);
}

// ── Biometric re-auth window (SPEC-V1 §5.1) ─────────────────────────────────

#[test]
fn a_password_unlock_resets_the_reauth_clock_and_a_biometric_one_does_not() {
    use keyring_lib::platform::biometric::password_unlock_due;

    let h = harness();
    let file = Arc::new(VaultFile::create(&h.path, MASTER, KdfParams::floor()).expect("create"));
    h.manager.attach(file.clone());

    // Never unlocked by password: the password is due.
    assert!(password_unlock_due(
        h.clock.now_ms(),
        h.manager.last_password_unlock_ms()
    ));

    h.manager.begin_unlock().expect("begin");
    h.manager
        .adopt(file.unlock(MASTER).expect("unlock").into_keys(), true);
    let after_password = h.manager.last_password_unlock_ms();
    assert!(!password_unlock_due(h.clock.now_ms(), after_password));

    // Thirteen days later, still fine.
    h.clock.advance(Duration::from_secs(13 * 86_400));
    assert!(!password_unlock_due(h.clock.now_ms(), after_password));

    // A biometric unlock in the meantime must not push the deadline out.
    h.manager.lock();
    h.manager.begin_unlock().expect("begin");
    h.manager
        .adopt(file.unlock(MASTER).expect("unlock").into_keys(), false);
    assert_eq!(
        h.manager.last_password_unlock_ms(),
        after_password,
        "a biometric unlock must not reset the 14-day master-password clock"
    );

    // Day fifteen: due again.
    h.clock.advance(Duration::from_secs(2 * 86_400));
    assert!(password_unlock_due(h.clock.now_ms(), after_password));
}

// ── The secret path still works through the manager ─────────────────────────

#[test]
fn a_secret_is_reachable_while_unlocked_and_not_after() {
    let h = harness();
    unlocked(&h);

    let id = h
        .manager
        .with_session(|s| {
            let vault = s
                .vault_add("Personal", "vault.accent.1")
                .map_err(AppError::from)?;
            s.item_upsert(&ItemDraft::new(
                vault,
                "a login",
                ItemBody::Login {
                    username: "user".to_owned(),
                    password: "a-generated-fixture-password".to_owned(),
                    urls: vec![],
                    totp: None,
                },
            ))
            .map_err(AppError::from)
        })
        .expect("create");

    let revealed = h
        .manager
        .with_session(|s| {
            s.item_secret(id, SecretField::Password)
                .map_err(AppError::from)
        })
        .expect("reveal");
    assert_eq!(&*revealed, "a-generated-fixture-password");

    h.manager.lock();
    assert!(h
        .manager
        .with_session(|s| s
            .item_secret(id, SecretField::Password)
            .map_err(AppError::from))
        .is_err());
}
