// SPDX-License-Identifier: AGPL-3.0-or-later
//! `app_state` — the pre-unlock plaintext carve-out (SPEC-V1 §4.5).
//!
//! Theme, biometric-enabled, unlock backoff and the first-run tour flags have to
//! be readable *before* the vault is unlocked, so they live here in the clear.
//! **Nothing else does.**
//!
//! §4.5 calls the permitted list exhaustive and says adding a key requires a
//! spec change. That is a security boundary, not a convenience bucket, so it is
//! enforced rather than documented: [`AppStateKey`] is a closed enum and the
//! read/write functions take nothing else. A new key means editing this file,
//! which means the diff shows up in review.
//!
//! Everything here is visible to an attacker holding the file. Nothing written
//! here may be secret, and nothing here may be trusted for an authorization
//! decision — the backoff counter in particular is explicitly *not* load-bearing
//! (SPEC-V1 §2).

use rusqlite::{Connection, OptionalExtension};

use crate::error::StoreError;

/// The complete set of keys permitted in `app_state` (SPEC-V1 §4.5).
///
/// Adding a variant is a spec change. If you are here to add one, check §4.5
/// first — most things belong in the encrypted settings instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppStateKey {
    /// Selected theme id.
    ThemeId,
    /// `dark` | `light` | `system`.
    ThemeMode,
    /// Whether biometric unlock is enrolled.
    BiometricEnabled,
    /// Consecutive failed master-password attempts.
    BackoffFailures,
    /// Unix milliseconds before which no attempt is accepted.
    BackoffUntil,
    /// Consecutive failed biometric attempts.
    ///
    /// Separate from the password counter, and not gated by it (ADD-003 §③):
    /// a user who fat-fingers their master password four times must not lose
    /// Touch ID for five minutes. That punishes the legitimate user and does
    /// nothing to an attacker, who does not have their finger.
    BiometricFailures,
    /// Serialized window geometry.
    WindowGeometry,
    /// Whether the screen-capture mitigation is on.
    ContentProtectionEnabled,
    /// Unix milliseconds of the last HIBP check.
    LastBreachCheckAt,
    /// Unix milliseconds of the last update-manifest check.
    LastUpdateCheckAt,
    /// Whether unattended update checks are permitted (SPEC-V1 §7.7, ADD-004).
    ///
    /// App state rather than vault state, and readable before unlock, because
    /// §7.7 checks on launch. Putting it in the encrypted settings would mean the
    /// only moment the preference is legible is a moment after the one it governs,
    /// so a locked app would either check against the user's wishes or never check
    /// at all.
    ///
    /// It is a preference, not an authorization decision, so §4.5's rule that
    /// nothing here may be load-bearing still holds: an attacker who flips it can
    /// suppress an update check, which is a nuisance, and cannot cause an unsigned
    /// artefact to be installed — that is the signature's job, not this flag's.
    UpdateChecksEnabled,
    /// Whether the pre-unlock master-password explanation has been shown
    /// (SPEC-V1 §4.5, ADD-004 §⑦).
    ///
    /// Here rather than in the encrypted settings for the same reason as the
    /// theme: the card it governs renders on the **lock screen**, before there
    /// is a key to read anything with. A flag stored inside the vault could
    /// only be consulted after the moment it decides.
    ///
    /// Not load-bearing. An attacker who sets it suppresses one explanatory
    /// card; an attacker who clears it causes one to be shown again. Neither is
    /// an authorization decision, and neither reveals anything a `stat` of the
    /// file did not already reveal — that this device has run Trynta.
    TourUnlockSeen,
    /// Whether the in-app card sequence has been completed or skipped
    /// (SPEC-V1 §4.5, ADD-004 §⑦).
    ///
    /// This one *could* have lived in the encrypted settings — it is only ever
    /// read after unlock. It is here anyway, next to its pair, because two
    /// halves of one feature stored under two different threat models is how
    /// you end up with a tour that has been half-seen: the settings blob is
    /// carried by a backup restore (§7.8) and `app_state` deliberately is not,
    /// so splitting them would make "restore a backup" replay one card and not
    /// the other four.
    TourAppSeen,
}

impl AppStateKey {
    /// The stored column value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThemeId => "theme_id",
            Self::ThemeMode => "theme_mode",
            Self::BiometricEnabled => "biometric_enabled",
            Self::BackoffFailures => "backoff_failures",
            Self::BackoffUntil => "backoff_until",
            Self::BiometricFailures => "biometric_failures",
            Self::WindowGeometry => "window_geometry",
            Self::ContentProtectionEnabled => "content_protection_enabled",
            Self::LastBreachCheckAt => "last_breach_check_at",
            Self::LastUpdateCheckAt => "last_update_check_at",
            Self::UpdateChecksEnabled => "update_checks_enabled",
            Self::TourUnlockSeen => "tour_unlock_seen",
            Self::TourAppSeen => "tour_app_seen",
        }
    }

    /// Every permitted key. Used by the test that pins the list to §4.5.
    #[must_use]
    pub const fn all() -> [Self; 13] {
        [
            Self::ThemeId,
            Self::ThemeMode,
            Self::BiometricEnabled,
            Self::BackoffFailures,
            Self::BackoffUntil,
            Self::BiometricFailures,
            Self::WindowGeometry,
            Self::ContentProtectionEnabled,
            Self::LastBreachCheckAt,
            Self::LastUpdateCheckAt,
            Self::UpdateChecksEnabled,
            Self::TourUnlockSeen,
            Self::TourAppSeen,
        ]
    }
}

/// Read a value, if set.
///
/// # Errors
///
/// [`StoreError::Database`] if the query fails.
pub fn get(conn: &Connection, key: AppStateKey) -> Result<Option<String>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            [key.as_str()],
            |r| r.get::<_, String>(0),
        )
        .optional()?)
}

/// Read an integer value, defaulting to 0 when unset or unparseable.
///
/// Unparseable defaults rather than errors because this table is attacker-
/// writable: a corrupt counter must not brick the unlock path. It is not
/// load-bearing, so treating garbage as zero is correct (SPEC-V1 §2).
///
/// # Errors
///
/// [`StoreError::Database`] if the query fails.
pub fn get_i64(conn: &Connection, key: AppStateKey) -> Result<i64, StoreError> {
    Ok(get(conn, key)?
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0))
}

/// Write a value.
///
/// # Errors
///
/// [`StoreError::Database`] if the write fails.
pub fn set(conn: &Connection, key: AppStateKey, value: &str) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO app_state (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key.as_str(), value],
    )?;
    Ok(())
}

/// Write an integer value.
///
/// # Errors
///
/// [`StoreError::Database`] if the write fails.
pub fn set_i64(conn: &Connection, key: AppStateKey, value: i64) -> Result<(), StoreError> {
    set(conn, key, &value.to_string())
}

/// Remove a value.
///
/// # Errors
///
/// [`StoreError::Database`] if the delete fails.
pub fn clear(conn: &Connection, key: AppStateKey) -> Result<(), StoreError> {
    conn.execute("DELETE FROM app_state WHERE key = ?1", [key.as_str()])?;
    Ok(())
}
