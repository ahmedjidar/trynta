// SPDX-License-Identifier: AGPL-3.0-or-later
//! The first-run tour's two flags, at the level they actually live (SPEC-V1 §4.5,
//! ADD-004 §⑦).
//!
//! `commands/tour.rs` is three lines of orchestration over `app_state` and
//! `services::tour`, so there is nothing in it to test that is not tested here or
//! in that module's own unit tests. What this file covers is the half that only
//! exists once a file is involved:
//!
//! - the flag is still set after the process that wrote it is gone;
//! - it is readable with **no master password in hand**, which is the entire
//!   reason §4.5 exists and the reason the pre-unlock card can be decided at all;
//! - the two flags are independent, so dismissing one card does not silently
//!   count as having seen the other four;
//! - a corrupt value shows the card rather than losing it.
//!
//! The replay policy itself is a pure function and is tested in
//! `services::tour`, both halves — this suite is a debug build, so the release
//! branch has no other way to be exercised.

use keyring_lib::services::tour;
use keyring_store::{AppStateKey, KdfParams, VaultFile};

const MASTER: &str = "tour-state-master-8Rk2Vp";

/// A created vault file, and the temp dir keeping it alive.
fn vault() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    drop(file);
    (dir, path)
}

/// What `tour_state` would report for one flag, given the profile.
fn shows(path: &std::path::Path, key: AppStateKey, dev_replay: bool) -> bool {
    let file = VaultFile::open(path).expect("open");
    let stored = file.state_get(key).expect("read");
    tour::visible(tour::seen_from(stored.as_deref()), dev_replay)
}

#[test]
fn a_dismissed_tour_stays_dismissed_across_a_restart() {
    let (_guard, path) = vault();

    // First launch: nothing stored, so a release build shows both.
    assert!(shows(&path, AppStateKey::TourUnlockSeen, false));
    assert!(shows(&path, AppStateKey::TourAppSeen, false));

    {
        let file = VaultFile::open(&path).expect("open");
        file.state_set(AppStateKey::TourUnlockSeen, tour::seen_to(true))
            .expect("write");
        file.state_set(AppStateKey::TourAppSeen, tour::seen_to(true))
            .expect("write");
        // Everything this process held is gone — the file is the only thing left.
        drop(file);
    }

    assert!(
        !shows(&path, AppStateKey::TourUnlockSeen, false),
        "the unlock card came back after a restart"
    );
    assert!(
        !shows(&path, AppStateKey::TourAppSeen, false),
        "the sequence came back after a restart"
    );
}

#[test]
fn the_same_stored_flag_replays_in_dev_and_does_not_in_release() {
    // The requirement, end to end against a real file: one flag, two profiles,
    // opposite answers. `visible` is what makes them differ and it is the only
    // thing that may.
    let (_guard, path) = vault();
    let file = VaultFile::open(&path).expect("open");
    file.state_set(AppStateKey::TourAppSeen, tour::seen_to(true))
        .expect("write");
    drop(file);

    assert!(
        shows(&path, AppStateKey::TourAppSeen, true),
        "pnpm tauri dev must replay a tour that has already been seen"
    );
    assert!(
        !shows(&path, AppStateKey::TourAppSeen, false),
        "a release build must show it exactly once, ever"
    );
}

#[test]
fn the_flag_is_readable_with_the_vault_locked() {
    // §4.5's whole purpose. `VaultFile::open` performs no derivation and holds no
    // key; if this ever needs `unlock`, the pre-unlock card cannot be decided in
    // time and the feature is broken by construction.
    let (_guard, path) = vault();
    let file = VaultFile::open(&path).expect("open");
    file.state_set(AppStateKey::TourUnlockSeen, tour::seen_to(true))
        .expect("write");
    drop(file);

    let locked = VaultFile::open(&path).expect("reopen, still locked");
    let stored = locked.state_get(AppStateKey::TourUnlockSeen).expect("read");
    assert!(tour::seen_from(stored.as_deref()));
}

#[test]
fn dismissing_one_card_does_not_mark_the_other_tour_seen() {
    // The two are one explanation split across a boundary, not one flag. Marking
    // the lock-screen card must leave the four-card sequence untouched.
    let (_guard, path) = vault();
    let file = VaultFile::open(&path).expect("open");
    file.state_set(AppStateKey::TourUnlockSeen, tour::seen_to(true))
        .expect("write");
    drop(file);

    assert!(!shows(&path, AppStateKey::TourUnlockSeen, false));
    assert!(
        shows(&path, AppStateKey::TourAppSeen, false),
        "the in-app sequence was marked seen by a lock-screen dismissal"
    );
}

#[test]
fn a_reset_brings_both_back() {
    // What `tour_reset` does, at the level it does it. Both keys, together: half
    // a replayed explanation is not a state anyone asked for.
    let (_guard, path) = vault();
    let file = VaultFile::open(&path).expect("open");
    for key in [AppStateKey::TourUnlockSeen, AppStateKey::TourAppSeen] {
        file.state_set(key, tour::seen_to(true)).expect("write");
    }
    for key in [AppStateKey::TourUnlockSeen, AppStateKey::TourAppSeen] {
        file.state_clear(key).expect("clear");
    }
    drop(file);

    assert!(shows(&path, AppStateKey::TourUnlockSeen, false));
    assert!(shows(&path, AppStateKey::TourAppSeen, false));
}

#[test]
fn a_hand_edited_flag_shows_the_card_rather_than_losing_it() {
    // `app_state` is plaintext and writable by anyone holding the file. The
    // failure this refuses to have is a first-run user who never sees the
    // sentence saying their master password cannot be recovered.
    let (_guard, path) = vault();
    let file = VaultFile::open(&path).expect("open");
    // Through the normal accessor, because `state_set` takes an arbitrary string:
    // the enum constrains which *key* may be written, never the value, so this is
    // a state the store can genuinely be in.
    file.state_set(AppStateKey::TourUnlockSeen, "🙂")
        .expect("write");
    drop(file);

    assert!(
        shows(&path, AppStateKey::TourUnlockSeen, false),
        "a corrupt flag suppressed the card instead of showing it"
    );
}
