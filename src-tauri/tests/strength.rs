//! The weak-password threshold (SPEC-V1 §7.4).
//!
//! > **Weak** — `zxcvbn`. Weak = offline crack time under 1 day at 10⁴ guesses/s.
//! > Show the estimate.
//!
//! The threshold is a **guess count**, not a `zxcvbn` score. Those track each
//! other loosely, and a score-based rule would drift from the spec's number
//! silently — so the tests below assert against `10⁴ × 86,400 = 8.64 × 10⁸`
//! directly, and assert that the meter's bands cannot disagree with the risk
//! list's verdict.
//!
//! The passwords here are deliberately terrible or deliberately synthetic. None is
//! a real credential and none came from a real site (CLAUDE.md §8).

use keyring_lib::services::strength::{
    self, Band, GUESSES_PER_SECOND, THRESHOLD_SECONDS, WEAK_AT_OR_BELOW_GUESSES,
};

#[test]
fn the_threshold_is_the_number_the_spec_gives() {
    // Guarded so a "tidying" edit cannot quietly move the line.
    assert_eq!(GUESSES_PER_SECOND, 10_000);
    assert_eq!(THRESHOLD_SECONDS, 86_400);
    assert_eq!(WEAK_AT_OR_BELOW_GUESSES, 864_000_000);
}

#[test]
fn obviously_guessable_passwords_are_weak() {
    for password in [
        "password",
        "123456",
        "qwerty",
        "letmein",
        "Password1",
        "abc123",
        "iloveyou",
        "trustno1",
    ] {
        let assessed = strength::assess(password, &[]);
        assert!(
            assessed.weak,
            "{password:?} was not flagged weak (guesses {}, {} s)",
            assessed.guesses, assessed.crack_seconds
        );
    }
}

#[test]
fn a_generated_twenty_character_password_is_not_weak() {
    // The shape our own generator produces at its default length.
    for password in [
        "xQ7#mK2$pR9!vT4&wZ6a",
        "3Bd)Yq8-Lm5^Nh2*Ws7c",
        "correct-horse-battery-staple-9",
    ] {
        let assessed = strength::assess(password, &[]);
        assert!(
            !assessed.weak,
            "{password:?} was flagged weak (guesses {})",
            assessed.guesses
        );
        assert!(assessed.crack_seconds > THRESHOLD_SECONDS);
    }
}

#[test]
fn an_empty_password_is_weak_rather_than_unmeasurable() {
    let assessed = strength::assess("", &[]);
    assert!(assessed.weak);
    assert_eq!(assessed.band, Band::VeryWeak);
    assert_eq!(assessed.guesses, 0);
}

#[test]
fn the_estimate_is_reported_and_not_only_the_verdict() {
    // §7.4: "Show the estimate." A user told "weak" learns nothing actionable; a
    // user told a time can decide.
    let assessed = strength::assess("password", &[]);
    assert_eq!(
        assessed.crack_seconds,
        assessed.guesses / GUESSES_PER_SECOND,
        "the reported time must be derived from the reported guesses, or the two \
         numbers on screen will disagree"
    );
}

#[test]
fn item_context_can_move_a_password_across_the_threshold() {
    // The clearest case, and a real one: a password that *is* the username. On its
    // own `alice@acme.test` measures 2.86e10 guesses and passes the threshold
    // comfortably. Told that the item's username is `alice@acme.test`, the
    // estimator puts it at 3 — because that is the first thing an attacker who can
    // see the item would type.
    //
    // This is why the report feeds an item's title, username and URLs in. Without
    // them the same password reads as safe.
    let context = ["Acme", "alice@acme.test", "https://acme.test"];
    let without = strength::assess("alice@acme.test", &[]);
    let with = strength::assess("alice@acme.test", &context);

    assert!(
        !without.weak,
        "expected this to look safe without context (guesses {})",
        without.guesses
    );
    assert!(
        with.weak,
        "context did not make a password identical to the username weak (guesses {})",
        with.guesses
    );
    assert!(with.guesses < without.guesses);
}

#[test]
fn context_never_makes_a_password_look_stronger() {
    // The general property. Context can only add ways to guess, so the estimate can
    // only fall — a case where it rose would mean the metadata was being scored as
    // if it were part of the secret.
    let context = ["Acme", "alice@acme.test", "https://acme.test"];
    for password in [
        "acme-alice-2026",
        "AcmeAcmeAcme",
        "alice@acme.test",
        "Acme12345",
        "acme.test-alice",
        "AcmeAlice2026!",
        "xQ7#mK2$pR9!vT4&wZ6a",
    ] {
        let without = strength::assess(password, &[]);
        let with = strength::assess(password, &context);
        assert!(
            with.guesses <= without.guesses,
            "context made {password:?} look stronger: {} then {}",
            without.guesses,
            with.guesses
        );
    }
}

#[test]
fn the_bands_cannot_disagree_with_the_weak_verdict() {
    // The meter has four segments and the report has a threshold. If a password
    // could be Fair or Strong while the report called it weak, the UI would
    // contradict itself on the same screen.
    for password in [
        "",
        "a",
        "password",
        "123456",
        "Password1",
        "hunter2hunter2",
        "xQ7#mK2$pR9!vT4&wZ6a",
        "3Bd)Yq8-Lm5^Nh2*Ws7c",
        "correct horse battery staple",
    ] {
        let assessed = strength::assess(password, &[]);
        if assessed.weak {
            assert!(
                matches!(assessed.band, Band::VeryWeak | Band::Weak),
                "{password:?} is weak but banded {:?}",
                assessed.band
            );
        } else {
            assert!(
                matches!(assessed.band, Band::Fair | Band::Strong),
                "{password:?} is not weak but banded {:?}",
                assessed.band
            );
        }
    }
}

#[test]
fn the_bands_are_ordered() {
    assert!(Band::VeryWeak < Band::Weak);
    assert!(Band::Weak < Band::Fair);
    assert!(Band::Fair < Band::Strong);
}

#[test]
fn a_longer_password_is_never_weaker() {
    // Not a law of the estimator in general, but it must hold for a password
    // extended with unrelated random-looking characters — otherwise the meter
    // would go backwards as a user types.
    let base = "xQ7#mK2$";
    let mut previous = 0u64;
    for extra in ["", "p", "pR", "pR9", "pR9!", "pR9!vT", "pR9!vT4&wZ6a"] {
        let assessed = strength::assess(&format!("{base}{extra}"), &[]);
        assert!(
            assessed.guesses >= previous,
            "adding {extra:?} lowered the estimate: {} then {}",
            previous,
            assessed.guesses
        );
        previous = assessed.guesses;
    }
}

#[test]
fn assessment_makes_no_network_request_and_is_deterministic() {
    // zxcvbn's dictionaries are compiled in. A strength estimator that consulted a
    // service would be a fourth outbound request, and §7 permits three. Determinism
    // is the observable proxy: the same input must give the same answer every time,
    // which a networked or randomised implementation would not.
    let first = strength::assess("Password1", &["Acme"]);
    for _ in 0..25 {
        assert_eq!(strength::assess("Password1", &["Acme"]), first);
    }
}

#[test]
fn a_very_strong_password_does_not_overflow_the_time_estimate() {
    // The crack time saturates rather than wrapping. A negative or tiny figure on
    // an extremely strong password would read as "weak" to a user.
    let assessed = strength::assess("9f3Qv#2Lm!8Xr$4Tz&7Bd)Yq-Nh^Ws*Kp+Jc=Rt~Vu", &[]);
    assert!(!assessed.weak);
    assert!(assessed.crack_seconds > THRESHOLD_SECONDS);
    assert_eq!(assessed.band, Band::Strong);
}
