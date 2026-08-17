//! Auto-lock policy (SPEC-V1 §5.2).
//!
//! Pure decision logic, deliberately separated from the thing that holds keys:
//! whether an event should lock the vault is a question with a right answer that
//! can be enumerated, and enumerating it is cheaper than reasoning about it
//! inside a state machine that also owns a MUK.
//!
//! §5.2 lists the settings and the triggers but does not cross them, so the
//! table below fills that in. The principle it applies: **a stronger signal
//! satisfies a weaker preference.** Someone who asked to lock on screensaver
//! also wants to lock when the machine sleeps; someone who asked for a 5-minute
//! idle timer also wants a lock when they walk away and the screen locks. Only
//! `Never` means never.

use std::time::Duration;

/// What the user chose in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoLockSetting {
    /// Lock as soon as the app loses focus or the vault goes idle at all.
    Immediately,
    /// Lock after this many minutes of inactivity. §5.2 offers 1/5/15/30/60.
    After(u32),
    /// Lock only when the machine sleeps.
    OnSleep,
    /// Lock only when the screen locks or the screensaver starts.
    OnScreensaver,
    /// Never lock automatically. The user is choosing this trade-off knowingly.
    Never,
}

impl Default for AutoLockSetting {
    /// SPEC-V1 §5.2: five minutes.
    fn default() -> Self {
        Self::After(5)
    }
}

impl AutoLockSetting {
    /// The idle timeout, if this setting has one.
    #[must_use]
    pub const fn idle_timeout(self) -> Option<Duration> {
        match self {
            Self::Immediately => Some(Duration::ZERO),
            Self::After(minutes) => Some(Duration::from_secs(minutes as u64 * 60)),
            Self::OnSleep | Self::OnScreensaver | Self::Never => None,
        }
    }
}

/// Something that happened which might warrant locking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockTrigger {
    /// The idle timer elapsed.
    Idle,
    /// The machine is going to sleep.
    Sleep,
    /// The session was locked or the screensaver started.
    ScreenLock,
    /// The user asked, by menu, button, or `⌘L` / `Ctrl+L`.
    Manual,
}

/// Whether `trigger` should lock a vault configured with `setting`.
///
/// `idle` is how long the vault has been untouched; it is only consulted for
/// [`LockTrigger::Idle`].
///
/// | trigger \ setting | Immediately | After(n) | `OnSleep` | `OnScreensaver` | Never |
/// |---|---|---|---|---|---|
/// | Manual     | yes | yes | yes | yes | **yes** |
/// | Sleep      | yes | yes | yes | yes | no |
/// | `ScreenLock` | yes | yes | no  | yes | no |
/// | Idle       | yes | if elapsed | no | no | no |
///
/// `Manual` locks even under `Never`: `Never` is a statement about *automatic*
/// locking, and refusing an explicit request would be absurd.
#[must_use]
pub fn should_lock(setting: AutoLockSetting, trigger: LockTrigger, idle: Duration) -> bool {
    match trigger {
        // An explicit request always wins.
        LockTrigger::Manual => true,

        // Sleeping is the strongest ambient signal: the machine is leaving the
        // user's control entirely. Everything but `Never` locks.
        LockTrigger::Sleep => setting != AutoLockSetting::Never,

        // The screen locking means the user walked away. Honoured by everyone
        // except someone who specifically asked for sleep-only.
        LockTrigger::ScreenLock => {
            !matches!(setting, AutoLockSetting::Never | AutoLockSetting::OnSleep)
        }

        // The timer only means anything to a setting that has one.
        LockTrigger::Idle => setting
            .idle_timeout()
            .is_some_and(|timeout| idle >= timeout),
    }
}

/// A clock, so the state machine can be tested without sleeping.
///
/// Wall-clock time is an input to lock decisions, and a test that has to wait
/// five real minutes to check a five-minute timeout is a test nobody runs.
pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }
}
