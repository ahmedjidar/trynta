//! The vault health score (SPEC-V1 §7.4, AC13).
//!
//! AC13 asks for three things from the score itself: the breakdown adds up, the
//! `N == 0` case returns null rather than a number, and the weights redistribute
//! as written when nothing is 2FA-capable. All three are checked here, plus the
//! rounding rule — round once at the end — which is the one that silently makes
//! a visible breakdown disagree with a visible total.
//!
//! The breach, strength and 2FA-directory inputs are not exercised here; they
//! are inputs by design, which is what makes the score testable exhaustively
//! without a network.

use keyring_lib::services::report::{self, HealthInputs};
use proptest::prelude::*;
use uuid::Uuid;

fn inputs(
    logins: usize,
    breached: usize,
    weak: usize,
    reused: usize,
    capable: usize,
    enabled: usize,
) -> HealthInputs {
    HealthInputs {
        logins,
        breached,
        weak,
        reused,
        two_factor_capable: capable,
        two_factor_enabled: enabled,
    }
}

#[test]
fn an_empty_vault_has_no_score_rather_than_zero_or_a_hundred() {
    // "if N == 0: score = null -> 'not enough data' (not 0, not 100)". Both
    // numeric answers are lies, in opposite directions.
    let score = report::health(HealthInputs::default());
    assert_eq!(score.value(), None);

    // Still none once other counts are present but there are no logins — a
    // vault of secure notes has nothing to score.
    let score = report::health(inputs(0, 0, 0, 0, 3, 3));
    assert_eq!(score.value(), None);
}

#[test]
fn a_perfect_vault_scores_a_hundred() {
    let score = report::health(inputs(10, 0, 0, 0, 10, 10));
    assert_eq!(score.value(), Some(100));
    assert!(score.breakdown_adds_up());
}

#[test]
fn the_worst_possible_vault_scores_zero() {
    let score = report::health(inputs(10, 10, 10, 10, 10, 0));
    assert_eq!(score.value(), Some(0));
    assert!(score.breakdown_adds_up());
}

#[test]
fn the_weights_are_the_ones_the_spec_names() {
    // Each dimension failed alone should cost exactly its weight.
    let all_good = report::health(inputs(10, 0, 0, 0, 10, 10));
    assert_eq!(all_good.value(), Some(100));

    assert_eq!(
        report::health(inputs(10, 10, 0, 0, 10, 10)).value(),
        Some(65)
    ); // −35
    assert_eq!(
        report::health(inputs(10, 0, 10, 0, 10, 10)).value(),
        Some(75)
    ); // −25
    assert_eq!(
        report::health(inputs(10, 0, 0, 10, 10, 10)).value(),
        Some(80)
    ); // −20
    assert_eq!(report::health(inputs(10, 0, 0, 0, 10, 0)).value(), Some(80)); // −20
}

#[test]
fn the_two_factor_weight_redistributes_when_nothing_is_capable() {
    // "if 2fa_capable == 0: weights become 43.75 / 31.25 / 25 (written down so
    // nobody re-derives them)".
    let score = report::health(inputs(10, 0, 0, 0, 0, 0));
    assert_eq!(score.value(), Some(100), "a clean vault is still 100");
    let breakdown = score.breakdown().expect("a scored vault has a breakdown");
    assert!((breakdown.breached.weight - 43.75).abs() < 1e-9);
    assert!((breakdown.weak.weight - 31.25).abs() < 1e-9);
    assert!((breakdown.reused.weight - 25.0).abs() < 1e-9);
    assert!(
        (breakdown.two_factor.weight - 0.0).abs() < f64::EPSILON,
        "the 2FA term must carry no weight when nothing is capable"
    );

    // All breached, nothing capable: 100 − 43.75 → 56.25 → 56.
    assert_eq!(report::health(inputs(10, 10, 0, 0, 0, 0)).value(), Some(56));
}

#[test]
fn the_breakdown_adds_up_to_the_score() {
    // The reason for rounding once at the end: rounding each term first drifts
    // by up to two points, and a breakdown that does not sum to the number
    // beside it is exactly what "explainable" rules out.
    for logins in 1..=13usize {
        for breached in 0..=logins {
            for weak in 0..=logins {
                let score = report::health(inputs(logins, breached, weak, 0, logins, breached));
                assert!(
                    score.breakdown_adds_up(),
                    "breakdown did not sum to the score for \
                     logins={logins} breached={breached} weak={weak}"
                );
            }
        }
    }
}

#[test]
fn counts_above_the_denominator_do_not_overflow_the_score() {
    // Defensive: a caller that double-counts an item must not produce a negative
    // term or a score above 100. Every count is clamped to its denominator, so
    // "99 of 3 breached" is a zero, not a negative.
    let worst = report::health(inputs(3, 99, 99, 99, 3, 0));
    assert_eq!(worst.value(), Some(0));
    assert!(worst.breakdown_adds_up());

    // `enabled` clamps to `capable` the same way, so 99-of-3 with 2FA on is a
    // full 2FA term and nothing else: 20 points exactly.
    let only_2fa = report::health(inputs(3, 99, 99, 99, 3, 99));
    assert_eq!(only_2fa.value(), Some(20));
    assert!(only_2fa.breakdown_adds_up());
}

// ── Reuse grouping (SPEC-V1 §7.4) ───────────────────────────────────────────

#[test]
fn reuse_reports_groups_and_counts_participants() {
    // "reused = items participating in ANY reuse group (3 sharing one password
    // → 3, not 1)". Fixing one of three leaves two, so three is the number of
    // problems.
    let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();

    let groups = report::reuse_groups(&[
        (ids[0], "shared-fixture-one"),
        (ids[1], "shared-fixture-one"),
        (ids[2], "shared-fixture-one"),
        (ids[3], "shared-fixture-two"),
        (ids[4], "unique-fixture"),
    ]);

    assert_eq!(groups.len(), 1, "only one password is actually shared");
    assert_eq!(groups[0].items.len(), 3);
    assert_eq!(report::reused_item_count(&groups), 3);
}

#[test]
fn two_separate_reuse_groups_are_both_reported() {
    let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
    let groups = report::reuse_groups(&[
        (ids[0], "fixture-alpha"),
        (ids[1], "fixture-alpha"),
        (ids[2], "fixture-beta"),
        (ids[3], "fixture-beta"),
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(report::reused_item_count(&groups), 4);
}

#[test]
fn a_unique_password_is_not_a_group() {
    let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
    let groups = report::reuse_groups(&[
        (ids[0], "fixture-one"),
        (ids[1], "fixture-two"),
        (ids[2], "fixture-three"),
    ]);
    assert!(groups.is_empty());
    assert_eq!(report::reused_item_count(&groups), 0);
}

#[test]
fn blank_passwords_are_not_reuse() {
    // A vault full of empty placeholders is not a vault full of reuse, and
    // reporting it as one buries the cases that matter.
    let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
    let groups = report::reuse_groups(&[(ids[0], ""), (ids[1], ""), (ids[2], "")]);
    assert!(groups.is_empty());
}

#[test]
fn grouping_is_deterministic() {
    // A report that reorders itself between runs looks like it changed when it
    // did not.
    let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
    let pairs = [
        (ids[0], "fixture-alpha"),
        (ids[1], "fixture-alpha"),
        (ids[2], "fixture-alpha"),
        (ids[3], "fixture-beta"),
    ];
    let first = report::reuse_groups(&pairs);
    for _ in 0..20 {
        assert_eq!(report::reuse_groups(&pairs), first);
    }
}

#[test]
fn a_group_carries_ids_and_nothing_else() {
    // The grouping key is a password. It must not survive into the result, where
    // it would travel to the UI in a structure nobody thinks of as secret.
    let ids: Vec<Uuid> = (0..2).map(|_| Uuid::new_v4()).collect();
    let sentinel = "REUSE-SENTINEL-Xk92Qd";
    let groups = report::reuse_groups(&[(ids[0], sentinel), (ids[1], sentinel)]);
    let rendered = format!("{groups:?}");
    assert!(
        !rendered.contains(sentinel),
        "the reuse group rendered the shared password: {rendered}"
    );
}

proptest! {
    /// Whatever the counts, the score stays in range and the breakdown sums to
    /// it. These are the two properties the UI relies on unconditionally.
    #[test]
    fn the_score_is_always_in_range_and_always_adds_up(
        logins in 0usize..40,
        breached in 0usize..40,
        weak in 0usize..40,
        reused in 0usize..40,
        capable in 0usize..40,
        enabled in 0usize..40,
    ) {
        let score = report::health(inputs(logins, breached, weak, reused, capable, enabled));
        if logins == 0 {
            prop_assert_eq!(score.value(), None);
        } else {
            let value = score.value().expect("a vault with logins has a score");
            prop_assert!(value <= 100);
        }
        prop_assert!(score.breakdown_adds_up());
    }

    /// Fixing a problem never lowers the score.
    #[test]
    fn fewer_breached_items_never_score_worse(
        logins in 1usize..40,
        breached in 0usize..40,
    ) {
        let breached = breached.min(logins);
        let worse = report::health(inputs(logins, breached, 0, 0, logins, logins));
        let better = report::health(inputs(logins, breached.saturating_sub(1), 0, 0, logins, logins));
        prop_assert!(better.value() >= worse.value());
    }
}
