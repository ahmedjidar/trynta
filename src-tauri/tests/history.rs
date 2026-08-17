//! Generator history retention (SPEC-V1 §7.3).
//!
//! > **History** — real secrets, so: ≤20 entries, auto-expire at 7 days,
//! > encrypted under `muk.appcache`, optionally wiped on lock, clearable in one
//! > action.
//!
//! Both caps are tested because they fail differently and both fail silently. A
//! broken count cap grows the list forever and nothing looks wrong; a broken age
//! cap keeps a password the user rotated months ago recoverable from a file they
//! think is current. Neither shows up in the UI.
//!
//! Pruning on **read** gets its own test. Expiry that only ran on write would be
//! true for people who kept generating and false for everyone else, which is the
//! worse half of the population to be wrong about.

use keyring_lib::services::history::{
    GeneratedKind, History, HistoryEntry, MAX_AGE_MS, MAX_ENTRIES,
};
use uuid::Uuid;

/// A realistic epoch, not zero: a clock starting at the epoch is not one any user
/// has, and it hides sign and saturation mistakes.
const NOW: i64 = 1_700_000_000_000;

fn entry(value: &str, created_at: i64) -> HistoryEntry {
    HistoryEntry {
        id: Uuid::new_v4(),
        value: value.to_owned(),
        kind: GeneratedKind::Password,
        entropy_bits: 128,
        created_at,
    }
}

#[test]
fn the_newest_entry_is_first() {
    // The UI renders in list order, so "newest first" has to be a property of the
    // data rather than something a component sorts.
    let mut history = History::new();
    history.record(entry("first-fixture", NOW), NOW);
    history.record(entry("second-fixture", NOW + 1), NOW + 1);

    assert_eq!(history.len(), 2);
    assert_eq!(history.entries[0].value, "second-fixture");
    assert_eq!(history.entries[1].value, "first-fixture");
}

#[test]
fn the_count_cap_holds_at_exactly_twenty() {
    let mut history = History::new();
    for i in 0..MAX_ENTRIES + 15 {
        let at = NOW + i64::try_from(i).expect("small");
        history.record(entry(&format!("fixture-{i}"), at), at);
    }

    assert_eq!(history.len(), MAX_ENTRIES, "the count cap did not hold");
    // The oldest are the ones dropped, so the newest generation is still there.
    let newest = format!("fixture-{}", MAX_ENTRIES + 14);
    assert_eq!(history.entries[0].value, newest);
    assert!(
        !history.entries.iter().any(|e| e.value == "fixture-0"),
        "the oldest entry survived the cap"
    );
}

#[test]
fn an_entry_older_than_seven_days_expires() {
    let mut history = History::new();
    history.record(entry("stale-fixture", NOW - MAX_AGE_MS - 1), NOW);
    assert!(history.is_empty(), "an entry past the age cap was retained");
}

#[test]
fn an_entry_just_inside_seven_days_survives() {
    // The boundary in both directions, because an off-by-one here either keeps a
    // secret too long or throws away one the user still needs.
    let mut history = History::new();
    history.record(entry("fresh-fixture", NOW - MAX_AGE_MS + 1), NOW);
    assert_eq!(history.len(), 1);

    let mut history = History::new();
    history.record(entry("edge-fixture", NOW - MAX_AGE_MS), NOW);
    assert!(
        history.is_empty(),
        "an entry exactly at the cap should be gone: the cap is an age, not a grace period"
    );
}

#[test]
fn expiry_applies_on_read_not_only_on_write() {
    // A user who generates once and never again must still see the entry expire.
    let mut history = History::new();
    history.record(entry("fixture", NOW), NOW);
    assert_eq!(history.len(), 1);

    history.prune(NOW + MAX_AGE_MS + 1);
    assert!(
        history.is_empty(),
        "pruning on read did nothing, so the 7-day expiry only applies to people \
         who keep using the generator"
    );
}

#[test]
fn a_mixed_age_list_keeps_only_the_fresh_entries() {
    let mut history = History::new();
    // Interleaved so a naive implementation that stops at the first fresh entry
    // is caught.
    for (index, age) in [0, MAX_AGE_MS + 1, 5, MAX_AGE_MS + 2, 9]
        .into_iter()
        .enumerate()
    {
        history
            .entries
            .push(entry(&format!("fixture-{index}"), NOW - age));
    }
    history.prune(NOW);

    assert_eq!(history.len(), 3);
    for retained in &history.entries {
        assert!(retained.created_at > NOW - MAX_AGE_MS);
    }
}

#[test]
fn a_value_is_addressable_by_id_and_gone_after_clearing() {
    let mut history = History::new();
    let kept = entry("copy-me-fixture", NOW);
    let id = kept.id;
    history.record(kept, NOW);

    assert_eq!(history.value_of(id), Some("copy-me-fixture"));
    assert_eq!(history.value_of(Uuid::new_v4()), None);

    history.clear();
    assert!(history.is_empty());
    assert_eq!(
        history.value_of(id),
        None,
        "a cleared history still handed out a value"
    );
}

#[test]
fn a_round_trip_through_the_stored_encoding_preserves_everything() {
    // This is what goes into app_cache. A field lost here is a history entry the
    // user cannot copy, or an entropy figure that reads as zero.
    let mut history = History::new();
    for kind in [
        GeneratedKind::Password,
        GeneratedKind::Passphrase,
        GeneratedKind::Pin,
    ] {
        history.record(
            HistoryEntry {
                id: Uuid::new_v4(),
                value: format!("fixture-{kind:?}"),
                kind,
                entropy_bits: 77,
                created_at: NOW,
            },
            NOW,
        );
    }

    let encoded = history.encode().expect("encode");
    let decoded = History::decode(&encoded);
    assert_eq!(decoded, history, "the stored encoding lost something");
}

#[test]
fn an_undecodable_payload_reads_as_empty_rather_than_failing() {
    // The history is a convenience cache of values the user has already used.
    // Failing a generate — or an unlock — because of it would trade something
    // that matters for something that does not.
    let decoded = History::decode(b"not a postcard payload at all");
    assert!(decoded.is_empty());
    let decoded = History::decode(&[]);
    assert!(decoded.is_empty());
}

#[test]
fn debug_never_prints_a_stored_value() {
    // CLAUDE.md §4.6. These are passwords the user may have set on real accounts.
    let sentinel = "HISTORY-SENTINEL-Rk83Qw";
    let mut history = History::new();
    history.record(entry(sentinel, NOW), NOW);

    for rendered in [
        format!("{history:?}"),
        format!("{history:#?}"),
        format!("{:?}", history.entries[0]),
        format!("{:#?}", history.entries[0]),
    ] {
        assert!(
            !rendered.contains(sentinel),
            "a history rendering leaked a stored value: {rendered}"
        );
        assert!(
            rendered.contains("redacted"),
            "a rendering that says nothing was redacted looks identical to one that \
             never redacted: {rendered}"
        );
    }
}

#[test]
fn the_entry_count_is_not_a_secret_and_stays_visible() {
    // Useful in a log, and it discloses nothing the user has not already seen on
    // screen. Redacting it would make the Debug impl useless for its purpose.
    let mut history = History::new();
    for i in 0..3 {
        history.record(entry(&format!("fixture-{i}"), NOW), NOW);
    }
    assert!(format!("{history:?}").contains('3'));
}
