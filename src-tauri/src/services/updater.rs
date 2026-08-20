// SPDX-License-Identifier: AGPL-3.0-or-later
//! When to check for an update, and what may be offered (SPEC-V1 §7.7).
//!
//! §7.7 is a deliberate carve-out from the no-network rule: *"A password manager
//! with no patch channel has 'hope users re-download the DMG' as its only response
//! to a dependency CVE."* The download, the signature check and the install are
//! Tauri's updater plugin — we do not hand-roll any of that, and CLAUDE.md §4.1
//! is why. What lives here is the part that is ours to get right: the cadence, and
//! a fail-closed guard on what may be presented to the user.
//!
//! The whole module is pure. The check runs at most once per 24 hours and only if
//! the user has not turned it off, and both of those are decisions that must be
//! testable without a network, a clock or a signing key.
//!
//! **What the endpoint learns.** §7.7: *"IP, version and platform — nothing else,
//! no identifier."* That is a property of the request, which the plugin builds from
//! the endpoint URL in `tauri.conf.json`. Nothing in this module contributes a
//! parameter to it, and nothing here may start to: no install id, no vault
//! fingerprint, no item count. If a future version of this file computes something
//! and puts it in a URL, that is the bug to look for.

/// Shortest gap between two checks (SPEC-V1 §7.7: *"at most once per 24 h"*).
pub const MIN_INTERVAL_MS: i64 = 86_400_000;

/// A `major.minor.patch` version.
///
/// Deliberately not a full semver implementation. This type exists only for the
/// guard in [`offerable`], where anything it cannot parse confidently must be
/// refused; a partial parse that silently ignores a pre-release suffix would turn
/// "refuse what you don't understand" into "accept it as equal".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// Major.
    pub major: u32,
    /// Minor.
    pub minor: u32,
    /// Patch.
    pub patch: u32,
}

impl Version {
    /// Parse exactly `major.minor.patch`.
    ///
    /// Returns `None` for anything else, pre-release and build metadata included.
    /// We do not ship either through this channel, and a version string we cannot
    /// read fully is one we must not compare.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Why a check did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skipped {
    /// The user turned update checks off.
    Disabled,
    /// Inside the 24-hour interval.
    TooSoon {
        /// When another check becomes permitted, Unix milliseconds.
        next_eligible_at: i64,
    },
}

/// Whether to check now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Go ahead.
    Check,
    /// Do nothing, for this reason.
    Skip(Skipped),
}

/// Decide whether a check may run (SPEC-V1 §7.7).
///
/// `Disabled` is checked before the interval, so a user who has switched updates
/// off is told that rather than being told to come back tomorrow.
///
/// A clock that has moved backwards does not unlock an early check: the comparison
/// is on elapsed time and a negative gap is not `>= MIN_INTERVAL_MS`. It also does
/// not lock the user out permanently, because `next_eligible_at` is computed from
/// the stored stamp and will pass once the clock catches up.
#[must_use]
pub const fn decide(enabled: bool, last_check_ms: i64, now_ms: i64) -> Decision {
    if !enabled {
        return Decision::Skip(Skipped::Disabled);
    }
    if now_ms.saturating_sub(last_check_ms) >= MIN_INTERVAL_MS {
        Decision::Check
    } else {
        Decision::Skip(Skipped::TooSoon {
            next_eligible_at: last_check_ms.saturating_add(MIN_INTERVAL_MS),
        })
    }
}

/// Whether a candidate release may be shown to the user.
///
/// Fails closed on every uncertainty. An unparseable current version, an
/// unparseable candidate, or a candidate that is not **strictly** newer all return
/// `false`.
///
/// The plugin already refuses to install anything whose signature does not verify,
/// and it does its own version comparison. This is the second lock on the same
/// door, and it is here because the failure it guards against is quiet: an endpoint
/// that starts serving an older build — through a rollback, a misconfiguration, or
/// a compromise of the manifest host with a stolen-but-old signed artefact — would
/// otherwise prompt every user to "update" to a version with a known
/// vulnerability, and the prompt would look completely normal.
#[must_use]
pub fn offerable(current: &str, candidate: &str) -> bool {
    match (Version::parse(current), Version::parse(candidate)) {
        (Some(current), Some(candidate)) => candidate > current,
        _ => false,
    }
}

/// Checks are on unless the user turns them off.
///
/// The alternative — off until opted in — reads as more conservative and is worse:
/// a password manager whose patch channel is dark by default ships a dependency CVE
/// to everyone who never found the setting, and SPEC-V1 §7.7 exists precisely
/// because that is not acceptable.
pub const ENABLED_BY_DEFAULT: bool = true;

/// Interpret the stored `app_state.update_checks_enabled` value (ADD-004).
///
/// Absent means [`ENABLED_BY_DEFAULT`]. So does anything unparseable: the failure
/// mode of a corrupted preference must not be a silently dark patch channel, and
/// this is the one place in the codebase where "fail closed" would mean *less*
/// safety rather than more.
#[must_use]
pub fn checks_enabled_from(stored: Option<&str>) -> bool {
    stored
        .and_then(|raw| raw.trim().parse::<bool>().ok())
        .unwrap_or(ENABLED_BY_DEFAULT)
}

/// The stored form of the toggle, for `app_state`.
#[must_use]
pub const fn checks_enabled_to(enabled: bool) -> &'static str {
    if enabled {
        "true"
    } else {
        "false"
    }
}

/// This build's version, from `Cargo.toml`.
#[must_use]
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cadence_is_twenty_four_hours() {
        assert_eq!(MIN_INTERVAL_MS, 24 * 60 * 60 * 1000);

        let last = 1_700_000_000_000;
        assert_eq!(
            decide(true, last, last + MIN_INTERVAL_MS - 1),
            Decision::Skip(Skipped::TooSoon {
                next_eligible_at: last + MIN_INTERVAL_MS
            })
        );
        assert_eq!(decide(true, last, last + MIN_INTERVAL_MS), Decision::Check);
    }

    #[test]
    fn a_first_run_may_check_immediately() {
        // No stored stamp reads as 0, and any real clock is far past the interval.
        assert_eq!(decide(true, 0, 1_700_000_000_000), Decision::Check);
    }

    #[test]
    fn disabled_beats_the_interval_in_both_directions() {
        assert_eq!(
            decide(false, 0, 1_700_000_000_000),
            Decision::Skip(Skipped::Disabled),
            "an eligible check is still not run when the user has said no"
        );
        assert_eq!(
            decide(false, 1_700_000_000_000, 1_700_000_000_001),
            Decision::Skip(Skipped::Disabled),
            "and 'you turned this off' is more useful than 'try tomorrow'"
        );
    }

    #[test]
    fn a_clock_that_moved_backwards_does_not_permit_an_early_check() {
        let last = 1_700_000_000_000;
        assert!(matches!(
            decide(true, last, last - MIN_INTERVAL_MS * 400),
            Decision::Skip(Skipped::TooSoon { .. })
        ));
    }

    #[test]
    fn only_a_strictly_newer_version_is_offerable() {
        assert!(offerable("0.1.0", "0.1.1"));
        assert!(offerable("0.1.0", "0.2.0"));
        assert!(offerable("0.9.9", "1.0.0"));
        assert!(offerable("1.2.3", "1.10.0"), "10 > 2, not '10' < '2'");

        assert!(!offerable("0.1.0", "0.1.0"), "equal is not newer");
        assert!(!offerable("0.2.0", "0.1.9"), "a downgrade is never offered");
        assert!(!offerable("1.0.0", "0.9.9"));
    }

    #[test]
    fn anything_unparseable_is_refused() {
        for candidate in [
            "1.2",
            "1.2.3.4",
            "v1.2.3",
            "1.2.3-rc.1",
            "1.2.3+build.5",
            "latest",
            "",
            "1.2.x",
            "-1.0.0",
        ] {
            assert!(
                !offerable("1.0.0", candidate),
                "{candidate:?} must be refused, not guessed at"
            );
        }
        assert!(
            !offerable("not-a-version", "9.9.9"),
            "an unreadable current version means no comparison is possible"
        );
    }

    #[test]
    fn the_toggle_round_trips_and_fails_open() {
        assert!(checks_enabled_from(Some(checks_enabled_to(true))));
        assert!(!checks_enabled_from(Some(checks_enabled_to(false))));
        assert!(
            !checks_enabled_from(Some(" false ")),
            "whitespace is trimmed"
        );

        assert!(
            checks_enabled_from(None),
            "no stored preference means checks are on"
        );
        for corrupt in ["", "0", "1", "no", "TRUE", "yes", "off", " "] {
            assert!(
                checks_enabled_from(Some(corrupt)),
                "{corrupt:?} is unparseable and must leave the patch channel open,                  not silently dark"
            );
        }
    }

    #[test]
    fn this_builds_own_version_parses() {
        assert!(
            Version::parse(current_version()).is_some(),
            "if our own version stops parsing, `offerable` refuses every update \
             and the patch channel silently dies"
        );
    }
}
