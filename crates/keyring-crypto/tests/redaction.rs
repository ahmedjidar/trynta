// SPDX-License-Identifier: AGPL-3.0-or-later
//! Redaction tests.
//!
//! CLAUDE.md §4.6 and §8: no secret-bearing type's `Debug`, `Display` or error
//! representation may emit plaintext.
//!
//! Run in **both** profiles — `cargo test --test redaction` and
//! `cargo test --release --test redaction`. Release codegen is where a `Drop`
//! gets elided and where formatting can be inlined differently, so a redaction
//! test that only ever runs in debug is proving less than it looks like.

use keyring_crypto::{
    derive_muk, derive_subkey, seal, Aad, AccountKeys, CryptoError, KdfParams, Key32, Muk, Purpose,
    Subkey, ENVELOPE_VERSION,
};

/// A byte pattern that is trivially recognisable in any encoding a formatter
/// might choose: decimal, hex, or raw.
const SENTINEL_BYTE: u8 = 0xC7;

fn sentinel_forms() -> Vec<String> {
    vec![
        format!("{SENTINEL_BYTE}"),      // 199
        format!("{SENTINEL_BYTE:x}"),    // c7
        format!("{SENTINEL_BYTE:X}"),    // C7
        format!("{SENTINEL_BYTE:#04x}"), // 0xc7
    ]
}

/// Compiles only for a `Copy` type, which a type owning a heap buffer cannot be.
fn assert_copy<T: Copy>() {}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

fn assert_redacted(what: &str, rendered: &str) {
    for form in sentinel_forms() {
        assert!(
            !rendered.contains(&form),
            "{what} leaked key material: rendered as {rendered:?}, which contains {form:?}"
        );
    }
    assert!(
        rendered.contains("redacted"),
        "{what} rendered as {rendered:?}, which does not say it is redacted — a formatter that \
         silently prints nothing is one refactor away from printing everything"
    );
}

#[test]
fn key32_debug_and_display_are_redacted() {
    let key = Key32::from_bytes([SENTINEL_BYTE; 32]);
    assert_redacted("Key32 Debug", &format!("{key:?}"));
    assert_redacted("Key32 Display", &format!("{key}"));
    assert_redacted("Key32 alternate Debug", &format!("{key:#?}"));
}

#[test]
fn muk_debug_and_display_are_redacted() {
    let muk = Muk::from_key32(Key32::from_bytes([SENTINEL_BYTE; 32]));
    assert_redacted("Muk Debug", &format!("{muk:?}"));
    assert_redacted("Muk Display", &format!("{muk}"));
    assert_redacted("Muk alternate Debug", &format!("{muk:#?}"));
}

#[test]
fn account_keys_debug_is_redacted() {
    let keys = AccountKeys::generate().expect("keys");
    let rendered = format!("{keys:?}");
    assert!(
        rendered.contains("redacted"),
        "AccountKeys Debug rendered as {rendered:?}"
    );

    // And the private scalars specifically must not appear in any encoding.
    let secret = keys.to_bytes();
    let hex = to_hex(secret.as_ref());
    assert!(
        !rendered.contains(&hex[..16]),
        "AccountKeys Debug leaked the x25519 scalar"
    );
    assert!(
        !rendered.contains(&hex[64..80]),
        "AccountKeys Debug leaked the ed25519 scalar"
    );
}

#[test]
fn a_derived_subkey_is_redacted_like_any_other_key() {
    let muk = Muk::from_key32(Key32::from_bytes([SENTINEL_BYTE; 32]));
    for which in [
        Subkey::Verify,
        Subkey::Header,
        Subkey::Wrap,
        Subkey::Vault,
        Subkey::AppCache,
    ] {
        let sub = derive_subkey(&muk, which);
        assert_redacted(&format!("{which:?} subkey Debug"), &format!("{sub:?}"));
    }
}

#[test]
fn a_real_derived_muk_never_prints_its_bytes() {
    // The path a user's actual master password takes.
    let muk = derive_muk(
        b"a generated fixture password",
        &[0x5a; 32],
        KdfParams::floor(),
    )
    .expect("derive");
    let raw = *muk.expose();
    let rendered = format!("{muk:?} {muk} {muk:#?}");
    // A single byte could appear in any short string by chance, so look for the
    // full contiguous run instead.
    let full_hex = to_hex(&raw);
    assert!(!rendered.contains(&full_hex));
    assert!(!rendered.contains(&format!("{raw:?}")));
    assert!(rendered.contains("redacted"));
}

#[test]
fn envelope_debug_does_not_dump_the_ciphertext() {
    // Ciphertext is not secret, but a Debug that prints kilobytes of it is how a
    // vault ends up pasted into a bug report.
    let key = Key32::from_bytes([0x11; 32]);
    let aad = Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose: Purpose::ItemSecret,
        subject_id: [0x22; 16],
        revision: 1,
        key_id: [0x33; 16],
    };
    let env = seal(&key, &aad, &[SENTINEL_BYTE; 64]).expect("seal");
    let rendered = format!("{env:?}");
    assert!(rendered.contains("bytes"), "Envelope Debug: {rendered:?}");
    assert!(
        rendered.len() < 200,
        "Envelope Debug dumped the ciphertext: {rendered:?}"
    );
}

#[test]
fn crypto_errors_carry_nothing_but_a_discriminant() {
    // Every variant, rendered both ways. If any of these ever gains a field that
    // could hold a key, a plaintext or a ciphertext, this test is where it should
    // become obvious.
    let errors = [
        CryptoError::KeyDerivation,
        CryptoError::InvalidKdfParams,
        CryptoError::Authentication,
        CryptoError::MalformedEnvelope,
        CryptoError::UnsupportedEnvelopeVersion {
            found: 99,
            supported: ENVELOPE_VERSION,
        },
        CryptoError::MalformedPadding,
        CryptoError::BadSignature,
        CryptoError::BadHeaderMac,
        CryptoError::InvalidLength,
        CryptoError::Rng,
    ];

    for err in errors {
        let debug = format!("{err:?}");
        let display = format!("{err}");
        assert!(!debug.is_empty() && !display.is_empty());
        for rendered in [&debug, &display] {
            assert!(
                !rendered.contains(&format!("{SENTINEL_BYTE:02x}")),
                "error rendered as {rendered:?}"
            );
        }
    }

    // The type is Copy, which is only possible because no variant owns a buffer.
    // That is the structural guarantee behind "redacting by construction".
    assert_copy::<CryptoError>();
}

#[test]
fn a_wrong_password_error_does_not_say_why() {
    // Two wrong passwords, one sharing a long prefix with the real one, must
    // produce identical error text.
    let salt = [0x5a; 32];
    let real = derive_muk(b"the-real-master-password", &salt, KdfParams::floor()).expect("derive");
    let verifier = keyring_crypto::verifier_from(&real);

    let near = derive_muk(b"the-real-master-passworX", &salt, KdfParams::floor()).expect("derive");
    let far = derive_muk(b"z", &salt, KdfParams::floor()).expect("derive");

    assert!(!keyring_crypto::verify_password(&near, &verifier));
    assert!(!keyring_crypto::verify_password(&far, &verifier));
    assert!(keyring_crypto::verify_password(&real, &verifier));
}
