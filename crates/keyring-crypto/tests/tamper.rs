// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tamper tests.
//!
//! CLAUDE.md §8: every ciphertext bit-flip must fail authentication. Not a
//! sample — every bit. And every field bound as associated data must be
//! genuinely bound, checked one field at a time so a missing one cannot hide
//! behind another.

use keyring_crypto::{header_mac, leaf_hash, manifest_root, sign_manifest};
use keyring_crypto::{
    open, seal, verify_header_mac, verify_manifest, Aad, AccountKeys, CryptoError, Envelope,
    HeaderFields, KdfParams, Key32, ManifestEntry, Purpose, ENVELOPE_VERSION,
};

fn aad() -> Aad {
    Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose: Purpose::ItemSecret,
        subject_id: [0x11; 16],
        revision: 42,
        key_id: [0x22; 16],
    }
}

fn sealed() -> (Key32, Aad, Envelope) {
    let key = Key32::from_bytes([0x33; 32]);
    let a = aad();
    let env = seal(&key, &a, b"a generated fixture secret").expect("seal");
    (key, a, env)
}

#[test]
fn every_ciphertext_bit_flip_fails_authentication() {
    let (key, a, env) = sealed();
    assert!(
        open(&key, &a, &env).is_ok(),
        "the untampered envelope opens"
    );

    let mut flips = 0usize;
    for byte in 0..env.ct.len() {
        for bit in 0..8u32 {
            let mut tampered = env.clone();
            tampered.ct[byte] ^= 1 << bit;
            assert_eq!(
                open(&key, &a, &tampered),
                Err(CryptoError::Authentication),
                "flipping bit {bit} of ciphertext byte {byte} was not detected"
            );
            flips += 1;
        }
    }
    // A padded 26-byte plaintext is one 256-byte block plus a 16-byte tag.
    assert_eq!(flips, (256 + 16) * 8);
}

#[test]
fn every_nonce_bit_flip_fails_authentication() {
    let (key, a, env) = sealed();
    for byte in 0..env.nonce.len() {
        for bit in 0..8u32 {
            let mut tampered = env.clone();
            tampered.nonce[byte] ^= 1 << bit;
            assert_eq!(
                open(&key, &a, &tampered),
                Err(CryptoError::Authentication),
                "flipping bit {bit} of nonce byte {byte} was not detected"
            );
        }
    }
}

#[test]
fn every_key_bit_flip_fails_authentication() {
    let (_, a, env) = sealed();
    for byte in 0..32 {
        for bit in 0..8u32 {
            let mut raw = [0x33u8; 32];
            raw[byte] ^= 1 << bit;
            assert_eq!(
                open(&Key32::from_bytes(raw), &a, &env),
                Err(CryptoError::Authentication),
                "flipping bit {bit} of key byte {byte} was not detected"
            );
        }
    }
}

#[test]
fn changing_the_purpose_fails_authentication() {
    // Serving a secret_ct as a meta_ct would put a password in the item list.
    let (key, a, env) = sealed();
    let mut wrong = a;
    wrong.purpose = Purpose::ItemMeta;
    assert_eq!(open(&key, &wrong, &env), Err(CryptoError::Authentication));
}

#[test]
fn changing_the_subject_id_fails_authentication() {
    // Moving one item's ciphertext onto another item.
    let (key, a, env) = sealed();
    let mut wrong = a;
    wrong.subject_id[0] ^= 1;
    assert_eq!(open(&key, &wrong, &env), Err(CryptoError::Authentication));
}

#[test]
fn changing_the_revision_fails_authentication() {
    // An in-place rollback. A whole-row restore is the manifest's job.
    let (key, a, env) = sealed();
    let mut wrong = a;
    wrong.revision -= 1;
    assert_eq!(open(&key, &wrong, &env), Err(CryptoError::Authentication));
}

#[test]
fn changing_the_key_id_fails_authentication() {
    let (key, a, env) = sealed();
    let mut wrong = a;
    wrong.key_id[15] ^= 1;
    assert_eq!(open(&key, &wrong, &env), Err(CryptoError::Authentication));

    // And rewriting it in the envelope rather than the AAD is caught too.
    let mut tampered = env.clone();
    tampered.key_id[15] ^= 1;
    assert_eq!(open(&key, &a, &tampered), Err(CryptoError::Authentication));
}

#[test]
fn an_unknown_envelope_version_is_a_hard_error_not_a_best_effort_parse() {
    let (key, a, env) = sealed();
    let mut bytes = env.to_bytes();
    bytes[0] = 0x00;
    bytes[1] = 0x63; // version 99
    assert_eq!(
        Envelope::from_bytes(&bytes),
        Err(CryptoError::UnsupportedEnvelopeVersion {
            found: 99,
            supported: ENVELOPE_VERSION,
        })
    );

    let mut tampered = env;
    tampered.envelope_version = 99;
    assert_eq!(
        open(&key, &a, &tampered),
        Err(CryptoError::UnsupportedEnvelopeVersion {
            found: 99,
            supported: ENVELOPE_VERSION,
        })
    );
}

#[test]
fn a_truncated_envelope_is_rejected() {
    let (_, _, env) = sealed();
    let bytes = env.to_bytes();

    // 42-byte header + one 256-byte padded block + a 16-byte tag. Nothing shorter
    // can be an envelope we wrote, because padding is never zero-length.
    let minimum = 42 + 256 + 16;
    assert_eq!(
        bytes.len(),
        minimum,
        "a one-block payload is {minimum} bytes"
    );

    for cut in 0..minimum {
        assert_eq!(
            Envelope::from_bytes(&bytes[..cut]),
            Err(CryptoError::MalformedEnvelope),
            "a {cut}-byte input parsed as an envelope"
        );
    }
    assert!(Envelope::from_bytes(&bytes).is_ok());
}

#[test]
fn a_ciphertext_that_is_not_a_whole_number_of_blocks_is_rejected() {
    // Anything we wrote is header + n × 256 + tag. A length that cannot have come
    // from our own writer is rejected structurally, before any key is touched.
    let (_, _, env) = sealed();
    let mut bytes = env.to_bytes();
    bytes.push(0);
    assert_eq!(
        Envelope::from_bytes(&bytes),
        Err(CryptoError::MalformedEnvelope)
    );
}

#[test]
fn two_seals_of_the_same_plaintext_differ() {
    // Fresh nonce per encryption. Identical ciphertexts would leak equality of
    // passwords across items without decrypting anything.
    let key = Key32::from_bytes([0x33; 32]);
    let a = aad();
    let one = seal(&key, &a, b"same plaintext").expect("seal");
    let two = seal(&key, &a, b"same plaintext").expect("seal");
    assert_ne!(one.nonce, two.nonce);
    assert_ne!(one.ct, two.ct);
}

// ── Manifest and header ──────────────────────────────────────────────────────

#[test]
fn a_manifest_signature_over_a_different_root_does_not_verify() {
    let keys = AccountKeys::generate().expect("keys");
    let public = keys.public().ed25519;

    let mut entries = vec![ManifestEntry {
        item_id: [0x01; 16],
        revision: 1,
        meta_hash: leaf_hash(b"meta"),
        secret_hash: leaf_hash(b"secret"),
    }];
    let root = manifest_root(&mut entries);
    let sig = sign_manifest(&keys, &root);
    verify_manifest(&public, &root, &sig).expect("the honest manifest verifies");

    // The rollback: same item, earlier revision, genuinely-authenticated
    // ciphertexts. Only the manifest catches it.
    let mut rolled_back = vec![ManifestEntry {
        item_id: [0x01; 16],
        revision: 0,
        meta_hash: leaf_hash(b"meta"),
        secret_hash: leaf_hash(b"secret"),
    }];
    let rolled_root = manifest_root(&mut rolled_back);
    assert_eq!(
        verify_manifest(&public, &rolled_root, &sig),
        Err(CryptoError::BadSignature)
    );
}

#[test]
fn a_manifest_signed_by_an_attackers_key_does_not_verify_under_the_real_key() {
    let real = AccountKeys::generate().expect("keys");
    let attacker = AccountKeys::generate().expect("keys");

    let mut entries = vec![ManifestEntry {
        item_id: [0x01; 16],
        revision: 0,
        meta_hash: leaf_hash(b"old meta"),
        secret_hash: leaf_hash(b"old secret"),
    }];
    let root = manifest_root(&mut entries);
    let forged = sign_manifest(&attacker, &root);

    assert_eq!(
        verify_manifest(&real.public().ed25519, &root, &forged),
        Err(CryptoError::BadSignature)
    );
    // ...which is exactly why the header MAC must bind the public key. Verified
    // under the attacker's own key, the forgery is perfectly valid:
    verify_manifest(&attacker.public().ed25519, &root, &forged)
        .expect("forgery is self-consistent");
}

#[test]
fn swapping_the_public_key_fails_the_header_mac() {
    // The attack the previous test sets up: rewrite the row, re-sign with your
    // own key, rewrite pubkey_ed25519 to match. The MAC under muk.header is the
    // only thing standing in the way, because the attacker does not have the MUK.
    let mac_key = Key32::from_bytes([0xa5; 32]);
    let real = AccountKeys::generate().expect("keys");
    let attacker = AccountKeys::generate().expect("keys");

    let salt = [0x11u8; 32];
    let verifier = [0x22u8; 32];
    let privct = [0x55u8; 80];
    let sig = [0x66u8; 64];

    let real_pk = real.public();
    let honest = HeaderFields {
        schema_version: 1,
        payload_version: 1,
        envelope_version: ENVELOPE_VERSION,
        account_salt: &salt,
        kdf: KdfParams::DEFAULT,
        verifier: &verifier,
        pubkey_x25519: &real_pk.x25519,
        pubkey_ed25519: &real_pk.ed25519,
        privkeys_ct: &privct,
        manifest_sig: &sig,
        created_at: 1_700_000_000_000,
    };
    let stored = header_mac(&mac_key, &honest);
    verify_header_mac(&mac_key, &honest, &stored).expect("honest header verifies");

    let attacker_pk = attacker.public();
    let mut swapped = honest;
    swapped.pubkey_ed25519 = &attacker_pk.ed25519;
    assert_eq!(
        verify_header_mac(&mac_key, &swapped, &stored),
        Err(CryptoError::BadHeaderMac)
    );
}

#[test]
fn every_header_mac_bit_flip_is_rejected() {
    let mac_key = Key32::from_bytes([0xa5; 32]);
    let salt = [0x11u8; 32];
    let verifier = [0x22u8; 32];
    let pkx = [0x33u8; 32];
    let pke = [0x44u8; 32];
    let privct = [0x55u8; 80];
    let sig = [0x66u8; 64];

    let header = HeaderFields {
        schema_version: 1,
        payload_version: 1,
        envelope_version: ENVELOPE_VERSION,
        account_salt: &salt,
        kdf: KdfParams::DEFAULT,
        verifier: &verifier,
        pubkey_x25519: &pkx,
        pubkey_ed25519: &pke,
        privkeys_ct: &privct,
        manifest_sig: &sig,
        created_at: 1,
    };
    let stored = header_mac(&mac_key, &header);

    for byte in 0..stored.len() {
        for bit in 0..8u32 {
            let mut tampered = stored;
            tampered[byte] ^= 1 << bit;
            assert_eq!(
                verify_header_mac(&mac_key, &header, &tampered),
                Err(CryptoError::BadHeaderMac),
                "flipping bit {bit} of MAC byte {byte} was not detected"
            );
        }
    }
}

#[test]
fn a_malformed_public_key_fails_closed() {
    // Not a valid curve point. Must be an error, never an accepted signature.
    let mut entries = vec![ManifestEntry {
        item_id: [0x01; 16],
        revision: 1,
        meta_hash: leaf_hash(b"m"),
        secret_hash: leaf_hash(b"s"),
    }];
    let root = manifest_root(&mut entries);
    assert_eq!(
        verify_manifest(&[0xff; 32], &root, &[0u8; 64]),
        Err(CryptoError::BadSignature)
    );
}
