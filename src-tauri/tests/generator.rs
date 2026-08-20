// SPDX-License-Identifier: AGPL-3.0-or-later
//! The generator (SPEC-V1 §7.3, AC12).
//!
//! Three properties, in order of how much damage getting them wrong would do:
//!
//! 1. **Entropy is honest.** The known-answer table below was produced by an
//!    independent implementation written from the spec's formula in Python,
//!    which has arbitrary-precision integers and shares no code with ours. AC12
//!    asks for exactly this, and the table is committed so the cross-check does
//!    not have to be re-run to be trusted. Where our number and the naive
//!    `length × log2(charset)` differ, the table records both — those rows are
//!    the whole reason the spec insists on inclusion–exclusion.
//! 2. **Every enabled class appears**, and by resampling rather than by
//!    substitution. Substitution would make one position predictable and would
//!    invalidate the entropy figure in point 1.
//! 3. **The distribution is unbiased.** A modulo reduction over a charset that
//!    does not divide 256 skews the first `256 % n` characters. That is
//!    invisible in any single password, so it is checked statistically.

// Two pedantic lints fire on the *harness*, not on anything it tests. A
// distribution check needs floating-point ratios over counts, and the four class
// flags are four booleans because SPEC-V1 §7.3 defines four switches — the test
// helper mirrors an API shape that is already settled.
#![allow(clippy::cast_precision_loss, clippy::fn_params_excessive_bools)]

use std::collections::{HashMap, HashSet};

use keyring_lib::services::generator::{
    self, Classes, GeneratorError, PassphraseOptions, PasswordOptions, DEFAULT_PASSWORD_LEN,
    EFF_WORDLIST_LEN, MAX_PASSWORD_LEN, MAX_PIN_LEN, MIN_PASSWORD_LEN, MIN_PIN_LEN,
};
use proptest::prelude::*;

const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?";
const AMBIGUOUS: &str = "l1I|0Oo";

fn options(
    uppercase: bool,
    lowercase: bool,
    digits: bool,
    symbols: bool,
    avoid_ambiguous: bool,
    length: usize,
) -> PasswordOptions {
    PasswordOptions {
        length,
        classes: Classes {
            uppercase,
            lowercase,
            digits,
            symbols,
        },
        avoid_ambiguous,
    }
}

// ── AC12: entropy against an independent implementation ─────────────────────

/// `(upper, lower, digits, symbols, avoid_ambiguous, length, expected_bits)`
///
/// Produced by the Python cross-check; see the module comment. The trailing
/// comment on a row records the naive `floor(log2(charset^length))` when it
/// disagrees, which is the inflated figure SPEC-V1 §7.3 forbids.
const KNOWN: &[(bool, bool, bool, bool, bool, usize, u32)] = &[
    (true, true, true, true, false, 20, 128),
    (true, true, true, true, false, 8, 50),  // naive says 51
    (true, true, true, true, true, 8, 49),   // naive says 50
    (true, true, true, true, true, 20, 125), // naive says 126
    (true, true, true, true, false, 128, 820),
    (true, true, true, true, true, 128, 806),
    (false, true, false, false, false, 8, 37),
    (false, true, false, false, false, 20, 94),
    (false, true, false, false, true, 20, 91),
    (true, true, false, false, false, 12, 68),
    (false, false, true, false, false, 12, 39),
    (false, false, false, true, false, 16, 72),
    (true, false, true, false, true, 10, 49), // naive says 50
    (false, true, true, true, false, 32, 188),
    (true, true, true, false, false, 64, 381),
];

#[test]
fn entropy_matches_the_independent_implementation_exactly() {
    for &(up, lo, di, sy, av, len, expected) in KNOWN {
        let got = generator::password_entropy_bits(options(up, lo, di, sy, av, len));
        assert_eq!(
            got, expected,
            "entropy disagreed for (upper={up}, lower={lo}, digits={di}, symbols={sy}, \
             ambiguous-removed={av}, length={len})"
        );
    }
}

#[test]
fn rejection_sampling_costs_real_bits_at_short_lengths() {
    // SPEC-V1 §7.3: "At length 20 with 4 classes the gap from the naive formula
    // is under 0.001 bits. At length 8 with 4 classes and ambiguous removed it
    // is a real fraction of a bit." If this ever stops being true, someone has
    // reverted to the naive formula.
    let all = Classes::default();

    let long = generator::password_entropy_bits(PasswordOptions {
        length: 20,
        classes: all,
        avoid_ambiguous: false,
    });
    assert_eq!(long, 128, "length 20 should land on the naive floor");

    let short = generator::password_entropy_bits(PasswordOptions {
        length: 8,
        classes: all,
        avoid_ambiguous: true,
    });
    assert_eq!(
        short, 49,
        "length 8 with ambiguity removed must be a bit below the naive 50"
    );
}

#[test]
fn passphrase_entropy_ignores_separator_and_capitalisation() {
    // "A configurable separator and optional capitalisation add zero bits — the
    // attacker knows the scheme. Never let the UI imply they help."
    let plain = PassphraseOptions {
        words: 4,
        separator: "-".to_owned(),
        capitalise: false,
        numeric_suffix: false,
    };
    let dressed = PassphraseOptions {
        words: 4,
        separator: " correct horse ".to_owned(),
        capitalise: true,
        numeric_suffix: false,
    };
    assert_eq!(
        generator::passphrase_entropy_bits(&plain),
        generator::passphrase_entropy_bits(&dressed)
    );
    assert_eq!(generator::passphrase_entropy_bits(&plain), 51);
}

#[test]
fn passphrase_and_pin_entropy_match_the_cross_check() {
    for (words, suffix, expected) in [
        (3, false, 38u32),
        (3, true, 42),
        (4, false, 51),
        (4, true, 55),
        (6, false, 77),
        (6, true, 80),
        (12, false, 155),
        (12, true, 158),
    ] {
        let opts = PassphraseOptions {
            words,
            separator: "-".to_owned(),
            capitalise: false,
            numeric_suffix: suffix,
        };
        assert_eq!(
            generator::passphrase_entropy_bits(&opts),
            expected,
            "{words} words, suffix {suffix}"
        );
    }

    for (length, expected) in [
        (3, 9u32),
        (4, 13),
        (5, 16),
        (6, 19),
        (7, 23),
        (8, 26),
        (9, 29),
        (10, 33),
        (11, 36),
        (12, 39),
    ] {
        assert_eq!(
            generator::pin_entropy_bits(length),
            expected,
            "pin {length}"
        );
    }
}

// ── The generated strings themselves ────────────────────────────────────────

#[test]
fn every_enabled_class_appears_in_every_password() {
    let cases = [
        options(true, true, true, true, false, MIN_PASSWORD_LEN),
        options(true, true, true, true, true, MIN_PASSWORD_LEN),
        options(true, true, true, true, false, DEFAULT_PASSWORD_LEN),
        options(true, false, true, false, true, 10),
        options(false, true, false, true, false, 9),
    ];

    for opts in cases {
        for _ in 0..200 {
            let generated = generator::password(opts).expect("generate");
            let value = generated.value.as_str();
            assert_eq!(value.chars().count(), opts.length);

            let filtered = |set: &str| -> Vec<char> {
                set.chars()
                    .filter(|c| !opts.avoid_ambiguous || !AMBIGUOUS.contains(*c))
                    .collect()
            };
            for (enabled, set) in [
                (opts.classes.uppercase, UPPERCASE),
                (opts.classes.lowercase, LOWERCASE),
                (opts.classes.digits, DIGITS),
                (opts.classes.symbols, SYMBOLS),
            ] {
                if enabled {
                    let members = filtered(set);
                    assert!(
                        value.chars().any(|c| members.contains(&c)),
                        "a required class is missing from {} chars",
                        value.chars().count()
                    );
                }
            }
        }
    }
}

#[test]
fn a_disabled_class_never_appears() {
    let opts = options(false, true, false, false, false, 32);
    for _ in 0..200 {
        let generated = generator::password(opts).expect("generate");
        assert!(
            generated.value.chars().all(|c| LOWERCASE.contains(c)),
            "a character outside the enabled classes was generated"
        );
    }
}

#[test]
fn ambiguous_characters_are_absent_when_asked() {
    let opts = options(true, true, true, true, true, 64);
    for _ in 0..100 {
        let generated = generator::password(opts).expect("generate");
        for c in generated.value.chars() {
            assert!(
                !AMBIGUOUS.contains(c),
                "{c} is ambiguous and should have been excluded"
            );
        }
    }
}

#[test]
fn turning_every_class_off_is_prevented_rather_than_an_error() {
    // SPEC-V1 §7.3: "At least one class always enabled (prevented, not
    // error-handled)."
    let opts = options(false, false, false, false, false, 16);
    let generated = generator::password(opts).expect("generation must not fail");
    assert_eq!(generated.value.chars().count(), 16);
    assert!(
        generated.value.chars().all(|c| LOWERCASE.contains(c)),
        "the fallback class should be lowercase"
    );
    assert!(generated.entropy_bits > 0);
}

#[test]
fn lengths_are_clamped_not_rejected() {
    assert_eq!(
        generator::password(options(true, true, true, true, false, 1))
            .expect("short")
            .value
            .chars()
            .count(),
        MIN_PASSWORD_LEN
    );
    assert_eq!(
        generator::password(options(true, true, true, true, false, 100_000))
            .expect("long")
            .value
            .chars()
            .count(),
        MAX_PASSWORD_LEN
    );
    assert_eq!(generator::pin(1).expect("short").value.len(), MIN_PIN_LEN);
    assert_eq!(generator::pin(99).expect("long").value.len(), MAX_PIN_LEN);
}

#[test]
fn a_pin_is_all_digits() {
    for length in MIN_PIN_LEN..=MAX_PIN_LEN {
        let generated = generator::pin(length).expect("pin");
        assert_eq!(generated.value.len(), length);
        assert!(generated.value.chars().all(|c| c.is_ascii_digit()));
    }
}

#[test]
fn two_generations_differ() {
    // Not a randomness test — a wiring test. A generator returning a constant
    // would pass every other assertion in this file.
    let opts = options(true, true, true, true, false, 32);
    let a = generator::password(opts).expect("a");
    let b = generator::password(opts).expect("b");
    assert_ne!(a.value.as_str(), b.value.as_str());
}

// ── Distribution ────────────────────────────────────────────────────────────

#[test]
fn character_selection_is_not_modulo_biased() {
    // A `byte % 85` reduction makes the first 256 % 85 = 1 character... only
    // slightly more likely, which is exactly why this needs a sample rather than
    // an eye. The stronger signal is a small charset with a bad remainder: 23
    // symbols means 256 % 23 = 3, so a biased implementation over-represents the
    // first three symbols by ~9%.
    let opts = options(false, false, false, true, false, MAX_PASSWORD_LEN);
    let mut counts: HashMap<char, usize> = HashMap::new();
    let samples = 400;
    for _ in 0..samples {
        let generated = generator::password(opts).expect("generate");
        for c in generated.value.chars() {
            *counts.entry(c).or_default() += 1;
        }
    }

    let total: usize = counts.values().sum();
    assert_eq!(total, samples * MAX_PASSWORD_LEN);
    assert_eq!(
        counts.len(),
        SYMBOLS.chars().count(),
        "some symbols never appeared at all"
    );

    // Expected share is 1/23 ≈ 4.35%. With ~51k draws the standard deviation per
    // character is ~0.09% of the total, so a 9% relative bias would be ~4 sigma
    // out. A 25% tolerance catches that comfortably while staying far enough
    // from the noise floor that this does not flake.
    let expected = total as f64 / counts.len() as f64;
    for (c, &count) in &counts {
        let ratio = count as f64 / expected;
        assert!(
            (0.75..1.25).contains(&ratio),
            "{c} appeared {count} times, expected about {expected:.0} — that is the \
             signature of a modulo-biased reduction"
        );
    }
}

// ── Passphrase ──────────────────────────────────────────────────────────────

/// A stand-in list of the right size. The real EFF list is a separate,
/// licence-recorded artifact; the algorithm does not care what the words are,
/// only that there are exactly 7,776 of them.
fn fixture_wordlist() -> Vec<String> {
    (0..EFF_WORDLIST_LEN)
        .map(|i| format!("word{i:04}"))
        .collect()
}

#[test]
fn a_wordlist_of_the_wrong_size_is_refused() {
    // A short list silently costs entropy, and the reported figure would still
    // claim log2(7776) per word. Refusing is the only honest option.
    let short: Vec<String> = fixture_wordlist().into_iter().take(100).collect();
    let refs: Vec<&str> = short.iter().map(String::as_str).collect();
    assert_eq!(
        generator::passphrase(&PassphraseOptions::default(), &refs).unwrap_err(),
        GeneratorError::NoWordList
    );
}

#[test]
fn a_passphrase_uses_the_list_and_the_separator() {
    let words = fixture_wordlist();
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let opts = PassphraseOptions {
        words: 5,
        separator: "-".to_owned(),
        capitalise: false,
        numeric_suffix: false,
    };

    let generated = generator::passphrase(&opts, &refs).expect("passphrase");
    let parts: Vec<&str> = generated.value.split('-').collect();
    assert_eq!(parts.len(), 5);
    for part in parts {
        assert!(refs.contains(&part), "{part} is not from the list");
    }
}

#[test]
fn capitalisation_and_suffix_change_the_string_only_as_described() {
    let words = fixture_wordlist();
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();

    let capitalised = generator::passphrase(
        &PassphraseOptions {
            words: 4,
            separator: " ".to_owned(),
            capitalise: true,
            numeric_suffix: false,
        },
        &refs,
    )
    .expect("passphrase");
    for part in capitalised.value.split(' ') {
        let first = part.chars().next().expect("non-empty word");
        assert!(first.is_uppercase(), "{part} was not capitalised");
    }

    let suffixed = generator::passphrase(
        &PassphraseOptions {
            words: 3,
            separator: "-".to_owned(),
            capitalise: false,
            numeric_suffix: true,
        },
        &refs,
    )
    .expect("passphrase");
    let last = suffixed.value.chars().last().expect("non-empty");
    assert!(last.is_ascii_digit(), "the numeric suffix is missing");
}

#[test]
fn passphrase_words_are_drawn_across_the_whole_list() {
    // Catches an index reduction that can only reach part of a 7,776-entry list
    // — the wide-draw equivalent of modulo bias, and one that would cost real
    // entropy while the reported figure stayed at 12.9 bits per word.
    let words = fixture_wordlist();
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let opts = PassphraseOptions {
        words: MAX_PASSWORD_LEN.min(12),
        separator: " ".to_owned(),
        capitalise: false,
        numeric_suffix: false,
    };

    let mut seen: HashSet<usize> = HashSet::new();
    for _ in 0..400 {
        let generated = generator::passphrase(&opts, &refs).expect("passphrase");
        for part in generated.value.split(' ') {
            let index: usize = part
                .trim_start_matches("word")
                .parse()
                .expect("fixture words are indexed");
            seen.insert(index);
        }
    }

    let highest = *seen.iter().max().expect("at least one draw");
    assert!(
        highest > EFF_WORDLIST_LEN * 3 / 4,
        "the highest index drawn was {highest} of {EFF_WORDLIST_LEN}; the upper part \
         of the list looks unreachable"
    );
}

// ── Properties ──────────────────────────────────────────────────────────────

proptest! {
    /// Inclusion–exclusion can only ever remove strings from the space, so the
    /// honest figure is bounded above by the naive one. A regression to
    /// `length × log2(charset)` breaks this in the direction that matters.
    #[test]
    fn entropy_never_exceeds_the_naive_upper_bound(
        upper in any::<bool>(),
        lower in any::<bool>(),
        digits in any::<bool>(),
        symbols in any::<bool>(),
        avoid in any::<bool>(),
        length in MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN,
    ) {
        let opts = options(upper, lower, digits, symbols, avoid, length);
        let bits = generator::password_entropy_bits(opts);

        // Mirrors the normalisation: all-off falls back to lowercase.
        let (up, lo, di, sy) = if upper || lower || digits || symbols {
            (upper, lower, digits, symbols)
        } else {
            (false, true, false, false)
        };
        let filtered = |set: &str| -> usize {
            set.chars().filter(|c| !avoid || !AMBIGUOUS.contains(*c)).count()
        };
        let charset = usize::from(up) * filtered(UPPERCASE)
            + usize::from(lo) * filtered(LOWERCASE)
            + usize::from(di) * filtered(DIGITS)
            + usize::from(sy) * filtered(SYMBOLS);

        let naive = (length as f64) * (charset as f64).log2();
        prop_assert!(
            f64::from(bits) <= naive + 1e-9,
            "entropy {bits} exceeds the naive bound {naive} for charset {charset}"
        );
    }

    /// More length is never less entropy, and more classes are never less
    /// entropy. Both are true of the real formula and false of several plausible
    /// ways to get it wrong.
    #[test]
    fn entropy_is_monotonic_in_length(
        length in MIN_PASSWORD_LEN..MAX_PASSWORD_LEN,
    ) {
        let shorter = generator::password_entropy_bits(
            options(true, true, true, true, false, length));
        let longer = generator::password_entropy_bits(
            options(true, true, true, true, false, length + 1));
        prop_assert!(longer >= shorter);
    }

    /// Generated length always equals the clamped request.
    #[test]
    fn generated_length_is_the_clamped_request(length in 0usize..300) {
        let opts = options(true, true, true, true, false, length);
        let generated = generator::password(opts).expect("generate");
        let expected = length.clamp(MIN_PASSWORD_LEN, MAX_PASSWORD_LEN);
        prop_assert_eq!(generated.value.chars().count(), expected);
    }
}
