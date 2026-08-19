// SPDX-License-Identifier: AGPL-3.0-or-later
//! Round-trip property tests.
//!
//! CLAUDE.md §8. The interesting cases are the boundaries: empty plaintexts,
//! exact multiples of the padding block, and plaintexts one byte either side of
//! one.

use keyring_crypto::{
    open, padding, seal, Aad, Envelope, Key32, Purpose, ENVELOPE_VERSION, PAD_BLOCK,
};
use proptest::prelude::*;

fn aad_for(revision: u64) -> Aad {
    Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose: Purpose::ItemMeta,
        subject_id: [0x11; 16],
        revision,
        key_id: [0x22; 16],
    }
}

// ── Padding ──────────────────────────────────────────────────────────────────

#[test]
fn padding_always_adds_at_least_one_byte() {
    for len in [0usize, 1, 255, 256, 257, 511, 512, 513] {
        let mut buf = vec![0xABu8; len];
        let before = buf.len();
        padding::pad(&mut buf);
        assert!(
            buf.len() > before,
            "no padding added for a {len}-byte input"
        );
        assert_eq!(buf.len() % PAD_BLOCK, 0, "not a whole number of blocks");
        assert!(buf.len() - before <= PAD_BLOCK);
    }
}

#[test]
fn an_exact_multiple_gets_a_whole_block_of_padding() {
    // The reason padding is never zero-length: without this, a plaintext ending
    // in 0x80 would be indistinguishable from a padded one.
    let mut buf = vec![0x80u8; PAD_BLOCK];
    padding::pad(&mut buf);
    assert_eq!(buf.len(), PAD_BLOCK * 2);
    padding::unpad(&mut buf).expect("unpad");
    assert_eq!(buf, vec![0x80u8; PAD_BLOCK]);
}

#[test]
fn unpad_rejects_structurally_invalid_input() {
    let mut empty = Vec::new();
    assert!(padding::unpad(&mut empty).is_err());

    let mut short = vec![0u8; PAD_BLOCK - 1];
    assert!(padding::unpad(&mut short).is_err(), "not a whole block");

    let mut no_marker = vec![0u8; PAD_BLOCK];
    assert!(
        padding::unpad(&mut no_marker).is_err(),
        "all zeroes, no marker"
    );

    let mut bad_tail = vec![0x41u8; PAD_BLOCK];
    assert!(
        padding::unpad(&mut bad_tail).is_err(),
        "no padding run at all"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn padding_round_trips(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let mut buf = data.clone();
        padding::pad(&mut buf);
        prop_assert_eq!(buf.len() % PAD_BLOCK, 0);
        padding::unpad(&mut buf).expect("unpad");
        prop_assert_eq!(buf, data);
    }

    #[test]
    fn seal_open_round_trips(
        data in proptest::collection::vec(any::<u8>(), 0..2048),
        key_bytes in any::<[u8; 32]>(),
        revision in any::<u64>(),
    ) {
        let key = Key32::from_bytes(key_bytes);
        let aad = aad_for(revision);
        let env = seal(&key, &aad, &data).expect("seal");
        let opened = open(&key, &aad, &env).expect("open");
        prop_assert_eq!(opened.as_slice(), data.as_slice());
    }

    #[test]
    fn envelope_serialisation_round_trips(
        data in proptest::collection::vec(any::<u8>(), 0..1024),
        key_bytes in any::<[u8; 32]>(),
    ) {
        let key = Key32::from_bytes(key_bytes);
        let aad = aad_for(1);
        let env = seal(&key, &aad, &data).expect("seal");
        let parsed = Envelope::from_bytes(&env.to_bytes()).expect("parse");
        prop_assert_eq!(parsed.envelope_version, env.envelope_version);
        prop_assert_eq!(parsed.key_id, env.key_id);
        prop_assert_eq!(parsed.nonce, env.nonce);
        prop_assert_eq!(parsed.ct, env.ct);
    }

    #[test]
    fn ciphertext_length_reveals_only_the_padded_block_count(
        a in proptest::collection::vec(any::<u8>(), 0..200),
        b in proptest::collection::vec(any::<u8>(), 0..200),
    ) {
        // Two plaintexts under 200 bytes both land in one 256-byte block, so a
        // short PIN and a long note are indistinguishable by length.
        let key = Key32::from_bytes([7u8; 32]);
        let aad = aad_for(1);
        let one = seal(&key, &aad, &a).expect("seal");
        let two = seal(&key, &aad, &b).expect("seal");
        prop_assert_eq!(one.ct.len(), two.ct.len());
    }
}
