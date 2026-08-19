// SPDX-License-Identifier: AGPL-3.0-or-later
//! TOTP against RFC 6238's published vectors (SPEC-V1 §4.1, §7.2, AC11).
//!
//! AC11: *"TOTP matches a reference implementation for SHA1/256/512 and 6/8
//! digits."*
//!
//! The table below is RFC 6238 Appendix B, recomputed from RFC 4226 §5.3 by a
//! Python implementation using only `hmac` and `hashlib` — so the numbers are
//! confirmed twice over: they are what the RFC prints, and they are what an
//! implementation sharing no code with ours produces.
//!
//! What the three algorithms buy that SHA-1 alone would not: dynamic truncation
//! reads its offset from the **last** byte of the MAC, and the MAC lengths are
//! 20, 32 and 64 bytes. An implementation that hard-codes offset 19 passes every
//! SHA-1 vector and fails both of the others, which is precisely why the
//! criterion names all three.

use keyring_lib::services::totp::{self, Algorithm, TotpConfig, TotpError};

/// RFC 6238 seeds, base32-encoded. ASCII "12345678901234567890" repeated to the
/// length each hash wants.
const SEED_SHA1: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const SEED_SHA256: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA====";
const SEED_SHA512: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ\
                           GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNA=";

fn seed_for(algorithm: Algorithm) -> &'static str {
    match algorithm {
        Algorithm::Sha1 => SEED_SHA1,
        Algorithm::Sha256 => SEED_SHA256,
        Algorithm::Sha512 => SEED_SHA512,
    }
}

fn config(algorithm: Algorithm, digits: u8, period_seconds: u32) -> TotpConfig {
    TotpConfig {
        secret: seed_for(algorithm).to_owned(),
        algorithm,
        digits,
        period_seconds,
        issuer: String::new(),
        account: String::new(),
    }
}

/// RFC 6238 Appendix B: `(algorithm, unix seconds, 8-digit code)`.
const RFC6238: &[(Algorithm, u64, &str)] = &[
    (Algorithm::Sha1, 59, "94287082"),
    (Algorithm::Sha256, 59, "46119246"),
    (Algorithm::Sha512, 59, "90693936"),
    (Algorithm::Sha1, 1_111_111_109, "07081804"),
    (Algorithm::Sha256, 1_111_111_109, "68084774"),
    (Algorithm::Sha512, 1_111_111_109, "25091201"),
    (Algorithm::Sha1, 1_111_111_111, "14050471"),
    (Algorithm::Sha256, 1_111_111_111, "67062674"),
    (Algorithm::Sha512, 1_111_111_111, "99943326"),
    (Algorithm::Sha1, 1_234_567_890, "89005924"),
    (Algorithm::Sha256, 1_234_567_890, "91819424"),
    (Algorithm::Sha512, 1_234_567_890, "93441116"),
    (Algorithm::Sha1, 2_000_000_000, "69279037"),
    (Algorithm::Sha256, 2_000_000_000, "90698825"),
    (Algorithm::Sha512, 2_000_000_000, "38618901"),
    (Algorithm::Sha1, 20_000_000_000, "65353130"),
    (Algorithm::Sha256, 20_000_000_000, "77737706"),
    (Algorithm::Sha512, 20_000_000_000, "47863826"),
];

#[test]
fn rfc6238_eight_digit_vectors() {
    for &(algorithm, at, expected) in RFC6238 {
        let code = totp::code_at(&config(algorithm, 8, 30), at).expect("code");
        assert_eq!(
            code.value,
            expected,
            "{} at t={at} produced the wrong code",
            algorithm.name()
        );
    }
}

#[test]
fn six_digit_codes_are_the_low_six_of_the_eight_digit_ones() {
    // Truncation is modulo 10^digits over the same 31-bit word, so the 6-digit
    // code is the 8-digit code's last six characters. A separate code path for
    // 6 digits would break this, and it is the cheapest possible check that
    // there is no such path.
    for &(algorithm, at, expected_eight) in RFC6238 {
        if algorithm != Algorithm::Sha1 {
            continue;
        }
        let six = totp::code_at(&config(algorithm, 6, 30), at).expect("code");
        assert_eq!(six.value.len(), 6);
        assert_eq!(
            six.value,
            &expected_eight[2..],
            "6-digit code at t={at} disagrees with the 8-digit one"
        );
    }
}

#[test]
fn a_non_default_period_changes_the_counter() {
    // Confirmed against the same Python implementation. A period the code
    // ignored would return the 30-second answer here.
    for (at, expected) in [
        (59u64, "755224"),
        (1_111_111_109, "360094"),
        (1_234_567_890, "713351"),
        (2_000_000_000, "864010"),
        (20_000_000_000, "948864"),
    ] {
        let code = totp::code_at(&config(Algorithm::Sha1, 6, 60), at).expect("code");
        assert_eq!(code.value, expected, "period 60 at t={at}");
        assert_eq!(code.period, 60);
    }
}

#[test]
fn the_countdown_matches_the_step() {
    let cfg = config(Algorithm::Sha1, 6, 30);

    // At the very start of a step the whole step remains.
    assert_eq!(
        totp::code_at(&cfg, 0).expect("code").seconds_remaining,
        30,
        "a fresh code should report the full period"
    );
    assert_eq!(totp::code_at(&cfg, 1).expect("code").seconds_remaining, 29);
    assert_eq!(totp::code_at(&cfg, 29).expect("code").seconds_remaining, 1);
    // Rollover: the next code has a full period ahead of it. Reporting 0 would
    // render as expired for one tick while the code is in fact brand new.
    assert_eq!(totp::code_at(&cfg, 30).expect("code").seconds_remaining, 30);
}

#[test]
fn the_code_changes_at_the_step_boundary_and_not_before() {
    let cfg = config(Algorithm::Sha1, 6, 30);
    let first = totp::code_at(&cfg, 30).expect("code").value;
    let same_step = totp::code_at(&cfg, 59).expect("code").value;
    let next_step = totp::code_at(&cfg, 60).expect("code").value;

    assert_eq!(first, same_step, "the code changed inside a step");
    assert_ne!(first, next_step, "the code did not change at the boundary");
}

#[test]
fn only_six_and_eight_digits_are_accepted() {
    for digits in [0u8, 1, 4, 5, 7, 9, 10] {
        assert_eq!(
            totp::code_at(&config(Algorithm::Sha1, digits, 30), 59).unwrap_err(),
            TotpError::Digits,
            "{digits} digits should be refused"
        );
    }
}

#[test]
fn a_zero_period_is_refused_rather_than_dividing_by_zero() {
    assert_eq!(
        totp::code_at(&config(Algorithm::Sha1, 6, 0), 59).unwrap_err(),
        TotpError::Period
    );
}

#[test]
fn a_bad_secret_is_reported_without_quoting_it() {
    let mut cfg = config(Algorithm::Sha1, 6, 30);
    cfg.secret = "NOT-VALID-BASE32-18".to_owned();
    let err = totp::code_at(&cfg, 59).unwrap_err();
    let rendered = format!("{err} {err:?}");
    assert!(
        !rendered.contains("NOT") && !rendered.contains("BASE32"),
        "the error quoted the secret: {rendered}"
    );
}

// ── otpauth:// parsing (SPEC-V1 §4.1) ───────────────────────────────────────

#[test]
fn a_default_uri_parses() {
    let cfg = totp::parse_uri(&format!(
        "otpauth://totp/Example:alice@example.test?secret={SEED_SHA1}&issuer=Example"
    ))
    .expect("parse");

    assert_eq!(cfg.secret, SEED_SHA1);
    assert_eq!(cfg.algorithm, Algorithm::Sha1);
    assert_eq!(cfg.digits, 6);
    assert_eq!(cfg.period_seconds, 30);
    assert_eq!(cfg.issuer, "Example");
    assert_eq!(cfg.account, "alice@example.test");
}

#[test]
fn non_default_algorithms_are_honoured() {
    // SPEC-V1 §4.1 calls this out explicitly. Ignoring `algorithm=SHA256`
    // produces codes that look right and never work, and the user has no way to
    // tell which side is wrong.
    for (name, expected) in [
        ("SHA1", Algorithm::Sha1),
        ("SHA256", Algorithm::Sha256),
        ("SHA512", Algorithm::Sha512),
        ("sha256", Algorithm::Sha256),
    ] {
        let cfg = totp::parse_uri(&format!(
            "otpauth://totp/a?secret={SEED_SHA1}&algorithm={name}"
        ))
        .expect("parse");
        assert_eq!(cfg.algorithm, expected, "algorithm={name}");
    }
}

#[test]
fn digits_and_period_are_honoured_and_validated() {
    let cfg = totp::parse_uri(&format!(
        "otpauth://totp/a?secret={SEED_SHA1}&digits=8&period=60"
    ))
    .expect("parse");
    assert_eq!(cfg.digits, 8);
    assert_eq!(cfg.period_seconds, 60);

    assert_eq!(
        totp::parse_uri(&format!("otpauth://totp/a?secret={SEED_SHA1}&digits=7")).unwrap_err(),
        TotpError::Digits
    );
    assert_eq!(
        totp::parse_uri(&format!("otpauth://totp/a?secret={SEED_SHA1}&period=0")).unwrap_err(),
        TotpError::Period
    );
}

#[test]
fn a_percent_encoded_label_decodes() {
    let cfg = totp::parse_uri(&format!(
        "otpauth://totp/ACME%20Co%3Aalice%40example.test?secret={SEED_SHA1}"
    ))
    .expect("parse");
    assert_eq!(cfg.issuer, "ACME Co");
    assert_eq!(cfg.account, "alice@example.test");
}

#[test]
fn a_plus_in_a_label_stays_a_plus() {
    // `+` means space in form encoding and nothing in a path. An account like
    // alice+trynta@example.test is common and must survive intact.
    let cfg = totp::parse_uri(&format!(
        "otpauth://totp/alice+trynta%40example.test?secret={SEED_SHA1}"
    ))
    .expect("parse");
    assert_eq!(cfg.account, "alice+trynta@example.test");
}

#[test]
fn the_query_issuer_wins_over_the_label() {
    let cfg = totp::parse_uri(&format!(
        "otpauth://totp/Stale:alice?secret={SEED_SHA1}&issuer=Current"
    ))
    .expect("parse");
    assert_eq!(cfg.issuer, "Current");
    assert_eq!(cfg.account, "alice");
}

#[test]
fn a_parsed_uri_produces_the_rfc_code() {
    // End to end: the string an authenticator hands over, through the parser,
    // to a published answer.
    let cfg = totp::parse_uri(&format!(
        "otpauth://totp/RFC:6238?secret={SEED_SHA512}&algorithm=SHA512&digits=8&period=30"
    ))
    .expect("parse");
    assert_eq!(
        totp::code_at(&cfg, 59).expect("code").value,
        "90693936",
        "the parsed config did not reproduce the RFC vector"
    );
}

#[test]
fn hotp_and_junk_are_refused() {
    for bad in [
        "https://example.test/?secret=AAAA",
        "otpauth://hotp/a?secret=GEZDGNBVGY3TQOJQ&counter=1",
        "otpauth://totp/a",
        "otpauth://totp/a?issuer=NoSecret",
        "not a uri at all",
        "otpauth://",
    ] {
        assert!(
            totp::parse_uri(bad).is_err(),
            "{bad} should not have parsed"
        );
    }
}

#[test]
fn an_unparseable_secret_is_caught_at_paste_time() {
    // Better to reject while the user is looking at the field than thirty
    // seconds later when a code is wanted and nothing appears.
    assert!(matches!(
        totp::parse_uri("otpauth://totp/a?secret=1111!!!!"),
        Err(TotpError::Secret(_))
    ));
}
