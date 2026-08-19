//! The lock/unlock state machine (SPEC-V1 §5).
//!
//! ```text
//!   Uninitialised ──create──▶ Locked ──unlock──▶ Unlocking ──ok──▶ Unlocked
//!                              ▲                     │                │
//!                              └─────────────────────┴────lock────────┘
//! ```
//!
//! `Locked` is reachable from anywhere, including from `Unlocking`, because a
//! sleep signal during an Argon2 derivation must not be ignored.
//!
//! **Lock is real** (CLAUDE.md §4.9). It is not a UI overlay and not a flag. It
//! drops the [`SessionKeys`], which zeroizes the MUK and the account keys; drops
//! the decrypted index; and clears the clipboard if it still holds a value we
//! put there. Everything that could hand out a secret goes through
//! [`SessionManager::with_session`], which returns [`SessionError::Locked`] the
//! instant the keys are gone — there is no path that reads a stale copy.

use std::sync::{Arc, Mutex, MutexGuard};

use keyring_store::{Session, SessionKeys, VaultFile};
use thiserror::Error;

use crate::autolock::{should_lock, AutoLockSetting, Clock, LockTrigger};
use crate::index::SearchIndex;
use crate::platform::Platform;
use crate::reveal::{Gate, RevealLimiter};

/// Where the vault is (SPEC-V1 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultState {
    /// No vault file exists yet.
    Uninitialised,
    /// A vault exists and is closed.
    Locked,
    /// A derivation is in flight.
    Unlocking,
    /// Keys are in memory.
    Unlocked,
}

/// Why a session operation failed.
///
/// Carries no data. In particular it never distinguishes "wrong password" here
/// — that comes from the store, which is the only place that knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum SessionError {
    /// The vault is locked and the operation needs keys.
    #[error("the vault is locked")]
    Locked,

    /// No vault file has been opened.
    #[error("no vault is open")]
    NoVault,

    /// The operation is not valid from the current state.
    #[error("that is not valid while the vault is {0:?}")]
    InvalidState(VaultState),
}

/// Everything the session owns, behind one lock.
///
/// One mutex rather than several: the fields are only ever meaningful together,
/// and a lock ordering bug in the code that holds the MUK is not a bug anyone
/// wants to debug.
struct Inner {
    state: VaultState,
    file: Option<Arc<VaultFile>>,
    keys: Option<SessionKeys>,
    /// The decrypted metadata index. Dropped on lock, which wipes it.
    index: Option<SearchIndex>,
    /// Ownership token for a clipboard write we made, so lock can clear it
    /// without destroying something the user copied since.
    clipboard_token: Option<u64>,
    /// When the vault was last touched, for the idle timer.
    last_activity_ms: i64,
    /// When a master-password unlock last succeeded, for the 14-day re-auth.
    /// `None` until one has.
    last_password_unlock_ms: Option<i64>,
    setting: AutoLockSetting,
    /// The rolling reveal window (SPEC-V1 §6). Lives here rather than beside the
    /// commands because it is session state: "lock is real" (CLAUDE.md §4.9)
    /// includes forgetting how many secrets were looked at before the lock.
    reveals: RevealLimiter,
}

/// Owns the unlocked state and everything that must be destroyed with it.
pub struct SessionManager {
    inner: Mutex<Inner>,
    platform: Arc<Platform>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // State is safe to print. Nothing else here is.
        f.debug_struct("SessionManager")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl SessionManager {
    /// A manager with no vault open.
    #[must_use]
    pub fn new(platform: Arc<Platform>, clock: Arc<dyn Clock>) -> Self {
        let now = clock.now_ms();
        Self {
            inner: Mutex::new(Inner {
                state: VaultState::Uninitialised,
                file: None,
                keys: None,
                index: None,
                clipboard_token: None,
                last_activity_ms: now,
                last_password_unlock_ms: None,
                setting: AutoLockSetting::default(),
                reveals: RevealLimiter::new(),
            }),
            platform,
            clock,
        }
    }

    fn lock_inner(&self) -> MutexGuard<'_, Inner> {
        // Recovering from poisoning rather than propagating: a panic elsewhere
        // must not make the vault permanently unlockable, and the invariant that
        // matters — keys are either present or absent — cannot be broken
        // half-way by a panic, because every mutation is a single assignment.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The current state.
    #[must_use]
    pub fn state(&self) -> VaultState {
        self.lock_inner().state
    }

    /// Adopt an opened vault file, moving to [`VaultState::Locked`].
    pub fn attach(&self, file: Arc<VaultFile>) {
        let mut inner = self.lock_inner();
        inner.file = Some(file);
        inner.state = VaultState::Locked;
    }

    /// The opened vault file, if any.
    ///
    /// # Errors
    ///
    /// [`SessionError::NoVault`] if nothing has been attached.
    pub fn file(&self) -> Result<Arc<VaultFile>, SessionError> {
        self.lock_inner().file.clone().ok_or(SessionError::NoVault)
    }

    /// The configured auto-lock setting.
    #[must_use]
    pub fn setting(&self) -> AutoLockSetting {
        self.lock_inner().setting
    }

    /// Change the auto-lock setting.
    pub fn set_setting(&self, setting: AutoLockSetting) {
        self.lock_inner().setting = setting;
    }

    /// Mark the vault as active, resetting the idle timer.
    pub fn touch(&self) {
        let now = self.clock.now_ms();
        self.lock_inner().last_activity_ms = now;
    }

    /// Run `f` with an unlocked session.
    ///
    /// The single door to key material. It rebuilds a transient
    /// [`Session`] from the stored keys for the duration of the call, so there
    /// is never a long-lived borrow to invalidate and never a copy of the keys
    /// outside the mutex.
    ///
    /// # Errors
    ///
    /// [`SessionError::Locked`] if the vault is not unlocked,
    /// [`SessionError::NoVault`] if nothing is attached. Anything `f` returns is
    /// passed through.
    pub fn with_session<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(&Session<'_>) -> Result<T, E>,
        E: From<SessionError>,
    {
        let mut inner = self.lock_inner();
        if inner.state != VaultState::Unlocked {
            return Err(SessionError::Locked.into());
        }
        let file = inner.file.clone().ok_or(SessionError::NoVault)?;
        let keys = inner.keys.take().ok_or(SessionError::Locked)?;

        let session = Session::resume(&file, keys);
        let result = f(&session);
        // Put the keys back whatever happened, so an error in `f` does not
        // silently lock the vault.
        inner.keys = Some(session.into_keys());
        inner.last_activity_ms = self.clock.now_ms();
        result
    }

    /// Record a successful unlock and take ownership of its keys.
    ///
    /// `by_password` distinguishes a master-password unlock from a biometric
    /// one, because only the former resets the 14-day re-auth clock (SPEC-V1
    /// §5.1).
    pub fn adopt(&self, keys: SessionKeys, by_password: bool) {
        let now = self.clock.now_ms();
        let mut inner = self.lock_inner();
        inner.keys = Some(keys);
        inner.state = VaultState::Unlocked;
        inner.last_activity_ms = now;
        if by_password {
            inner.last_password_unlock_ms = Some(now);
        }
    }

    /// Decrypt every item's metadata and build the search index (SPEC-V1 §4.7).
    ///
    /// Called once, immediately after [`SessionManager::adopt`]. Separate from
    /// `adopt` because it can fail and a failed index must not leave a
    /// half-unlocked vault: the keys are already adopted, so a caller that
    /// cannot build the index can still lock cleanly.
    ///
    /// # Errors
    ///
    /// Whatever the store reports while decrypting metadata.
    pub fn build_index(&self) -> Result<usize, SessionError> {
        let rows = self.with_session(|s| s.index_rows().map_err(|_| SessionError::Locked))?;
        let count = rows.len();
        self.lock_inner().index = Some(SearchIndex::build(rows));
        Ok(count)
    }

    /// Run `f` against the search index.
    ///
    /// # Errors
    ///
    /// [`SessionError::Locked`] if there is no index, which is the same thing as
    /// the vault being locked.
    pub fn with_index<T, F>(&self, f: F) -> Result<T, SessionError>
    where
        F: FnOnce(&SearchIndex) -> T,
    {
        let inner = self.lock_inner();
        inner.index.as_ref().map(f).ok_or(SessionError::Locked)
    }

    /// Mutate the search index in place.
    ///
    /// # Errors
    ///
    /// [`SessionError::Locked`] if there is no index.
    pub fn with_index_mut<T, F>(&self, f: F) -> Result<T, SessionError>
    where
        F: FnOnce(&mut SearchIndex) -> T,
    {
        let mut inner = self.lock_inner();
        inner.index.as_mut().map(f).ok_or(SessionError::Locked)
    }

    /// Enter [`VaultState::Unlocking`].
    ///
    /// # Errors
    ///
    /// [`SessionError::InvalidState`] unless the vault is currently locked.
    pub fn begin_unlock(&self) -> Result<(), SessionError> {
        let mut inner = self.lock_inner();
        if inner.state != VaultState::Locked {
            return Err(SessionError::InvalidState(inner.state));
        }
        inner.state = VaultState::Unlocking;
        Ok(())
    }

    /// Return to [`VaultState::Locked`] after a failed unlock.
    pub fn abort_unlock(&self) {
        let mut inner = self.lock_inner();
        if inner.state == VaultState::Unlocking {
            inner.state = VaultState::Locked;
        }
    }

    /// When a master-password unlock last succeeded, in Unix milliseconds, or
    /// `None` if one never has.
    #[must_use]
    pub fn last_password_unlock_ms(&self) -> Option<i64> {
        self.lock_inner().last_password_unlock_ms
    }

    /// The session's view of the clock.
    ///
    /// Exposed so a command measures time against the same source the auto-lock
    /// timer does. Two clocks in one process is how an injected clock in a test
    /// ends up half-effective.
    #[must_use]
    pub fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }

    /// Ask whether a reveal may proceed, counting it if so (SPEC-V1 §6).
    ///
    /// Returns [`Gate::ReauthRequired`] once 20 reveals have happened inside any
    /// rolling 60 seconds. The caller must not read the secret in that case.
    pub fn check_reveal(&self) -> Gate {
        let now = self.clock.now_ms();
        self.lock_inner().reveals.check(now)
    }

    /// Whether the next reveal needs re-authentication.
    #[must_use]
    pub fn reveal_reauth_pending(&self) -> bool {
        self.lock_inner().reveals.reauth_pending()
    }

    /// Record a successful re-authentication, reopening the reveal window.
    pub fn note_reauth(&self) {
        self.lock_inner().reveals.reauthenticated();
    }

    /// Consume a pending confirmation, for `require_master_on_reveal`.
    pub fn take_fresh_reauth(&self) -> bool {
        self.lock_inner().reveals.take_fresh_reauth()
    }

    /// Remember that we put `token` on the clipboard.
    pub fn note_clipboard_write(&self, token: u64) {
        self.lock_inner().clipboard_token = Some(token);
    }

    /// Clear the clipboard if it still holds our write.
    ///
    /// Called by the auto-clear timer and by [`SessionManager::lock`].
    pub fn clear_clipboard(&self) {
        let token = self.lock_inner().clipboard_token.take();
        if let Some(token) = token {
            // A failure here is not worth failing the caller over — the
            // clipboard being briefly unavailable is normal — but it is worth
            // recording, because a clipboard that never clears is a real leak.
            match self.platform.clipboard.clear_if_ours(token) {
                Ok(true) => tracing::debug!("cleared our clipboard entry"),
                Ok(false) => tracing::debug!("clipboard holds someone else's value; left alone"),
                Err(e) => tracing::warn!(error = %e, "could not clear the clipboard"),
            }
        }
    }

    /// Lock the vault.
    ///
    /// Idempotent, and callable from any state — a sleep signal arriving during
    /// a derivation must not be dropped.
    pub fn lock(&self) {
        // The clipboard first, and outside the inner lock, because clearing it
        // can block briefly on a contended OS clipboard and the keys should not
        // wait on that to be destroyed.
        self.clear_clipboard();

        let mut inner = self.lock_inner();
        // Dropping the keys is the lock. `SessionKeys` owns a `Zeroizing` MUK
        // and zeroizing dalek keys, so this wipes rather than merely releases.
        drop(inner.keys.take());
        // And the index with them: it holds every title, username and URL, which
        // is the account inventory even though none of it is a secret field.
        drop(inner.index.take());
        // The reveal window too. Carrying it across a lock would mean a user who
        // locked and came back was still one reveal from a re-auth prompt for a
        // session that no longer exists.
        inner.reveals.reset();
        inner.state = match inner.file {
            Some(_) => VaultState::Locked,
            None => VaultState::Uninitialised,
        };
        tracing::info!("vault locked");
    }

    /// Apply a lock trigger, locking if the policy says so.
    ///
    /// Returns whether the vault was locked.
    pub fn handle_trigger(&self, trigger: LockTrigger) -> bool {
        let (setting, idle) = {
            let inner = self.lock_inner();
            let idle_ms = self
                .clock
                .now_ms()
                .saturating_sub(inner.last_activity_ms)
                .max(0);
            (
                inner.setting,
                std::time::Duration::from_millis(idle_ms.unsigned_abs()),
            )
        };

        if !should_lock(setting, trigger, idle) {
            return false;
        }
        // Locking an already-locked vault is a no-op, but reporting it as a lock
        // would make the idle timer look like it fired when it did not.
        if self.state() != VaultState::Unlocked {
            return false;
        }
        self.lock();
        true
    }

    /// The platform services this session was built with.
    #[must_use]
    pub fn platform(&self) -> &Arc<Platform> {
        &self.platform
    }
}
