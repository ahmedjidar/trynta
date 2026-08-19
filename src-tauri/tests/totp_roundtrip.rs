//! A pasted `otpauth://` URI survives save → lock → reopen and still authenticates.
//!
//! ADD-004 §④ records a shipped bug: `secret_ct` once held only the seed, so an item
//! saved as SHA-256 at 8 digits came back as SHA-1 at 6 and generated codes that
//! never worked. Nothing about that failure is visible from the parser's own tests —
//! `parse_uri` was always right — so the test that would have caught it has to cross
//! the same three boundaries the bug did: parse, persist, reopen.
//!
//! The verification is against an **independent implementation of RFC 6238 written
//! in this file**, not against `services::totp`. A round-trip that compares the
//! product to itself proves the two ends agree and says nothing about whether either
//! is correct; the point here is that the code Keyring displays is the code the
//! user's service is expecting. The reference below is deliberately naive — build
//! the counter, HMAC it, truncate — so it can be read against the RFC line by line.
//!
//! The timestamp is fixed. A test whose expected value depends on when it runs is a
//! test that fails at midnight for reasons nobody can reproduce.

use hmac::{Hmac, Mac};
use keyring_lib::commands::dto::{TotpAlgorithmDto, TotpConfigInput};
use keyring_lib::services::totp::{self, Algorithm, TotpConfig};
use keyring_store::model::{TotpAlgorithm, TotpConfig as StoredTotp};
use keyring_store::{ItemBody, ItemDraft, KdfParams, VaultFile};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

const MASTER: &str = "totp-roundtrip-master-4Bn7Rx";

/// RFC 6238's own test seed, in base32: 20 bytes of "12345678901234567890".
const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

/// A fixed instant, so the expected code never depends on the wall clock.
const AT: u64 = 1_700_000_000;

// ── the independent reference ───────────────────────────────────────────────

/// RFC 6238 §4, implemented from the document rather than from `services::totp`.
fn reference_code(secret_base32: &str, alg: &str, digits: u32, period: u64, at: u64) -> String {
    let key = decode_base32(secret_base32);
    let counter = (at / period).to_be_bytes();

    let digest: Vec<u8> = match alg {
        "SHA1" => {
            let mut m = <Hmac<Sha1> as Mac>::new_from_slice(&key).expect("key");
            m.update(&counter);
            m.finalize().into_bytes().to_vec()
        }
        "SHA256" => {
            let mut m = <Hmac<Sha256> as Mac>::new_from_slice(&key).expect("key");
            m.update(&counter);
            m.finalize().into_bytes().to_vec()
        }
        "SHA512" => {
            let mut m = <Hmac<Sha512> as Mac>::new_from_slice(&key).expect("key");
            m.update(&counter);
            m.finalize().into_bytes().to_vec()
        }
        other => panic!("unsupported algorithm in the reference: {other}"),
    };

    // Dynamic truncation: the low nibble of the last byte picks a 4-byte window.
    let offset = usize::from(digest[digest.len() - 1] & 0x0f);
    let binary = ((u32::from(digest[offset]) & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);

    let modulus = 10u32.pow(digits);
    let width = usize::try_from(digits).expect("digit count fits");
    format!("{:0width$}", binary % modulus, width = width)
}

/// RFC 4648 base32, decoded independently of `services::base32`.
fn decode_base32(input: &str) -> Vec<u8> {
    let mut bits = 0u32;
    let mut nbits = 0u32;
    let mut out = Vec::new();
    for c in input.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32,
            '2'..='7' => c as u32 - '2' as u32 + 26,
            other => panic!("not base32: {other}"),
        };
        bits = (bits << 5) | v;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push(u8::try_from((bits >> nbits) & 0xff).expect("byte"));
        }
    }
    out
}

// ── conversions across the two TotpConfig types ─────────────────────────────

fn to_stored(input: &TotpConfigInput) -> StoredTotp {
    StoredTotp {
        secret: input.secret.clone(),
        algorithm: match input.algorithm {
            TotpAlgorithmDto::Sha1 => TotpAlgorithm::Sha1,
            TotpAlgorithmDto::Sha256 => TotpAlgorithm::Sha256,
            TotpAlgorithmDto::Sha512 => TotpAlgorithm::Sha512,
        },
        digits: input.digits,
        period_seconds: input.period_seconds,
        issuer: input.issuer.clone(),
        account: input.account.clone(),
    }
}

fn to_service(stored: &StoredTotp) -> TotpConfig {
    TotpConfig {
        secret: stored.secret.clone(),
        algorithm: match stored.algorithm {
            TotpAlgorithm::Sha1 => Algorithm::Sha1,
            TotpAlgorithm::Sha256 => Algorithm::Sha256,
            TotpAlgorithm::Sha512 => Algorithm::Sha512,
        },
        digits: stored.digits,
        period_seconds: stored.period_seconds,
        issuer: stored.issuer.clone(),
        account: stored.account.clone(),
    }
}

// ── the round trip ──────────────────────────────────────────────────────────

#[test]
fn a_sha256_8_digit_60_second_uri_round_trips_and_matches_a_reference() {
    let uri = format!(
        "otpauth://totp/Northline%20Bank:alice%40example.test?secret={SEED}&algorithm=SHA256&digits=8&period=60&issuer=Northline%20Bank"
    );

    // 1. Parse, exactly as `totp_parse` does.
    let parsed = totp::parse_uri(&uri).expect("the URI parses");
    assert_eq!(parsed.algorithm, Algorithm::Sha256);
    assert_eq!(parsed.digits, 8);
    assert_eq!(parsed.period_seconds, 60);
    assert_eq!(parsed.issuer, "Northline Bank");
    assert_eq!(parsed.account, "alice@example.test");

    let input = TotpConfigInput {
        secret: parsed.secret.clone(),
        algorithm: TotpAlgorithmDto::Sha256,
        digits: parsed.digits,
        period_seconds: parsed.period_seconds,
        issuer: parsed.issuer.clone(),
        account: parsed.account.clone(),
    };

    // 2. Save it into a real vault, then close the vault.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    let id = {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault");
        session
            .item_upsert(&ItemDraft::new(
                vault,
                "Northline Bank",
                ItemBody::Login {
                    username: "alice@example.test".to_owned(),
                    password: "FIXTURE-PASSWORD-Qw9Zt".to_owned(),
                    urls: vec!["https://northline.example".to_owned()],
                    totp: Some(to_stored(&input)),
                },
            ))
            .expect("item")
        // `session` and `file` drop here: the vault is closed, not merely idle.
    };

    // 3. Reopen from the password and read it back.
    let file = VaultFile::open(&path).expect("open");
    let session = file.unlock(MASTER).expect("unlock again");
    let loaded = session
        .item_totp(id)
        .expect("read")
        .expect("the item has a configuration");

    // Every parameter, not just the seed. This is the assertion ADD-004 §④ needed.
    assert_eq!(loaded.secret, parsed.secret, "the seed changed");
    assert_eq!(
        loaded.algorithm,
        TotpAlgorithm::Sha256,
        "algorithm was lost"
    );
    assert_eq!(loaded.digits, 8, "digit count was lost");
    assert_eq!(loaded.period_seconds, 60, "period was lost");
    assert_eq!(loaded.issuer, "Northline Bank", "issuer label was lost");
    assert_eq!(
        loaded.account, "alice@example.test",
        "account label was lost"
    );

    // 4. The code Keyring would display, computed from what came out of the vault.
    let shown = totp::code_at(&to_service(&loaded), AT).expect("code");

    // 5. The code the service is expecting, computed independently.
    let expected = reference_code(SEED, "SHA256", 8, 60, AT);

    assert_eq!(
        shown.value.len(),
        8,
        "an 8-digit configuration produced {} digits",
        shown.value.len()
    );
    assert_eq!(
        shown.value, expected,
        "the stored configuration produced a code the reference does not agree with"
    );
}

#[test]
fn the_rfc_defaults_round_trip_too_and_still_match_the_reference() {
    // SHA-1, 6 digits, 30 seconds, all omitted from the URI — so this also covers
    // "an absent parameter means the RFC default", not "absent means zero".
    let uri = format!("otpauth://totp/alice@example.test?secret={SEED}");
    let parsed = totp::parse_uri(&uri).expect("parses");
    assert_eq!(parsed.algorithm, Algorithm::Sha1);
    assert_eq!(parsed.digits, 6);
    assert_eq!(parsed.period_seconds, 30);

    let shown = totp::code_at(&parsed, AT).expect("code");
    assert_eq!(shown.value, reference_code(SEED, "SHA1", 6, 30, AT));
}

#[test]
fn a_bare_base32_secret_agrees_with_the_reference() {
    // The manual-entry path: no URI, just the string the site printed.
    let config = TotpConfig {
        secret: SEED.to_owned(),
        ..TotpConfig::default()
    };
    let shown = totp::code_at(&config, AT).expect("code");
    assert_eq!(shown.value, reference_code(SEED, "SHA1", 6, 30, AT));
}

#[test]
fn sha512_at_eight_digits_agrees_with_the_reference() {
    let config = TotpConfig {
        secret: SEED.to_owned(),
        algorithm: Algorithm::Sha512,
        digits: 8,
        ..TotpConfig::default()
    };
    let shown = totp::code_at(&config, AT).expect("code");
    assert_eq!(shown.value, reference_code(SEED, "SHA512", 8, 30, AT));
}

#[test]
fn the_countdown_follows_the_configured_period() {
    let config = TotpConfig {
        secret: SEED.to_owned(),
        period_seconds: 60,
        ..TotpConfig::default()
    };
    // A real step boundary, not a round-looking number: 1_700_000_000 is 20 seconds
    // into a 60-second step, which is exactly the sort of thing that makes a
    // countdown assertion pass for the wrong reason.
    const STEP_START: u64 = 1_699_999_980;
    assert_eq!(STEP_START % 60, 0, "the fixture is not on a step boundary");

    let start = totp::code_at(&config, STEP_START).expect("code");
    assert_eq!(start.seconds_remaining, 60);

    let mid = totp::code_at(&config, STEP_START + 25).expect("code");
    assert_eq!(mid.seconds_remaining, 35);
    assert_eq!(mid.value, start.value, "the code changed mid-step");

    let last = totp::code_at(&config, STEP_START + 59).expect("code");
    assert_eq!(
        last.seconds_remaining, 1,
        "the countdown never reaches zero"
    );
    assert_eq!(
        last.value, start.value,
        "the code changed at the end of its step"
    );

    let next = totp::code_at(&config, STEP_START + 60).expect("code");
    assert_ne!(next.value, start.value, "the code did not roll over");
    assert_eq!(next.seconds_remaining, 60);
}
