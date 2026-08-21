// SPDX-License-Identifier: AGPL-3.0-or-later
//! First-run guided tour: what has been seen, and whether this build replays.
//!
//! Two flags, both in `app_state` (SPEC-V1 §4.5, ADD-004 §⑦), and one policy
//! decision that is compiled in rather than configured. Everything here is a
//! pure function over an `Option<&str>` and a `bool`, so the policy is testable
//! without a vault, a window or a build profile — which matters, because half of
//! it only holds in a profile the test suite never runs under.
//!
//! ## Why the profile and not an environment variable
//!
//! The requirement is that `pnpm tauri dev` replays the tour on every launch and
//! a release build shows it exactly once, ever. An environment variable would
//! satisfy that on the day it was written and then rot: `TRYNTA_TOUR=1` left in a
//! shell profile, a launcher, or a CI script turns a shipped binary into one that
//! nags every launch, and nothing in the artefact would say so. [`DEV_REPLAY`] is
//! `cfg!(debug_assertions)`, so the answer is a property of the binary. There is
//! no input that changes it and nothing to leave set.
//!
//! `debug_assertions` rather than `cfg!(debug)` — which does not exist — and
//! rather than `#[cfg(test)]`, which would make the flag true in unit tests and
//! false in the dev binary, i.e. exactly backwards.

/// Whether this build replays the tour on every launch.
///
/// True in `pnpm tauri dev` (a debug build), false in a bundled release.
///
/// [`visible`] takes the flag as an argument rather than reading this directly,
/// so both halves of the policy are tested. That is not a stylistic choice: a
/// test suite is itself a debug build, so a `visible` that consulted this
/// constant would have no way to exercise the release branch at all, and an
/// assertion *about* the constant is one clippy correctly rejects as having a
/// value known at compile time.
pub const DEV_REPLAY: bool = cfg!(debug_assertions);

/// Interpret a stored tour flag.
///
/// Absent means not seen. So does anything unparseable, and that direction is
/// deliberate: `app_state` is plaintext and attacker-writable by definition
/// (SPEC-V1 §4.5), so the question is which way a corrupt byte should fail. The
/// cost of guessing "not seen" is one explanatory card the user has already read.
/// The cost of guessing "seen" is that a first-run user never sees the sentence
/// telling them their master password cannot be recovered. Those are not
/// comparable, so it fails towards showing.
#[must_use]
pub fn seen_from(stored: Option<&str>) -> bool {
    stored
        .and_then(|raw| raw.trim().parse::<bool>().ok())
        .unwrap_or(false)
}

/// The stored form of a tour flag.
#[must_use]
pub const fn seen_to(seen: bool) -> &'static str {
    if seen {
        "true"
    } else {
        "false"
    }
}

/// Whether a tour should run.
///
/// The entire policy, in one line: a replaying build ignores the stored flag, and
/// every other build shows the tour until it has been seen.
#[must_use]
pub const fn visible(seen: bool, dev_replay: bool) -> bool {
    dev_replay || !seen
}

#[cfg(test)]
mod tests {
    use super::{seen_from, seen_to, visible};

    #[test]
    fn a_flag_round_trips() {
        assert!(seen_from(Some(seen_to(true))));
        assert!(!seen_from(Some(seen_to(false))));
    }

    #[test]
    fn anything_unreadable_means_not_seen() {
        // Every one of these must show the card rather than suppress it. The
        // last two are the realistic corruptions: a truncated write and a
        // hand-edited file.
        for stored in [
            None,
            Some(""),
            Some("  "),
            Some("yes"),
            Some("1"),
            Some("tru"),
        ] {
            assert!(
                !seen_from(stored),
                "{stored:?} must read as not-seen, so the card is shown rather than lost"
            );
        }
        // Surrounding whitespace is not corruption.
        assert!(seen_from(Some(" true ")), "a padded value is still true");
    }

    #[test]
    fn a_replaying_build_ignores_the_stored_flag() {
        assert!(visible(true, true), "dev replays even once seen");
        assert!(visible(false, true), "dev replays when never seen");
    }

    #[test]
    fn a_release_build_shows_it_exactly_once() {
        assert!(visible(false, false), "never seen means show");
        assert!(!visible(true, false), "seen once means never again");
    }
}
