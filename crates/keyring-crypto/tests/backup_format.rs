// SPDX-License-Identifier: AGPL-3.0-or-later
//! Frozen vectors for the `.tryntabak` v1 header (ADD-003 §④).
//!
//! Format only — export and restore are run 2. Freezing the bytes now means run
//! 2 implements against a fixed target instead of inventing one under deadline.
//!
//! As with the envelope, the expected values were produced by an independent
//! implementation of the documented layout, not by printing what this code does.
//! Reference implementation, reproduced so anyone can re-derive them:
//!
//! ```python
//! import hashlib, hmac, struct
//!
//! def prefix(backup_version, envelope_version, salt, m, t, p,
//!            verifier, pubkey_ed25519, manifest_sig, created_at):
//!     out  = bytearray(b"KEYRINGB")
//!     out += struct.pack(">H", backup_version)
//!     out += struct.pack(">H", envelope_version)
//!     out += struct.pack(">I", 0)                       # reserved
//!     out += salt                                       # 32
//!     out += struct.pack(">I", m) + struct.pack(">I", t) + struct.pack(">I", p)
//!     out += verifier                                   # 32
//!     out += pubkey_ed25519                             # 32
//!     out += manifest_sig                               # 64
//!     out += struct.pack(">q", created_at)
//!     return bytes(out)                                 # 196 bytes
//!
//! mac = hmac.new(key, prefix(...), hashlib.sha256).digest()   # header is 228
//! ```

use hex_literal::hex;
use keyring_crypto::backup::{
    backup_manifest_root, backup_verifier_from, derive_backup_subkey, verify_backup_header_mac,
    verify_backup_passphrase, BackupHeader, BackupSubkey, BACKUP_VERSION, DOMAIN_BACKUP_MANIFEST,
    HEADER_LEN, HEADER_PREFIX_LEN, MAGIC,
};
use keyring_crypto::{
    derive_muk, leaf_hash, manifest_root, CryptoError, KdfParams, Key32, ManifestEntry, Muk,
};

fn reference_header() -> BackupHeader {
    BackupHeader {
        backup_version: BACKUP_VERSION,
        envelope_version: 1,
        account_salt: [0xB1; 32],
        kdf: KdfParams {
            m_kib: 65_536,
            t: 3,
            p: 4,
        },
        verifier: [0xB2; 32],
        pubkey_ed25519: [0xB3; 32],
        manifest_sig: [0xB4; 64],
        created_at: 1_700_000_000_000,
    }
}

fn header_key() -> Key32 {
    Key32::from_bytes([0xB5; 32])
}

// ── Layout ───────────────────────────────────────────────────────────────────

#[test]
fn the_header_is_228_bytes_with_a_196_byte_mac_input() {
    assert_eq!(HEADER_PREFIX_LEN, 196);
    assert_eq!(HEADER_LEN, 228);
    assert_eq!(MAGIC, *b"KEYRINGB");

    let header = reference_header();
    assert_eq!(header.mac_input().len(), HEADER_PREFIX_LEN);
    assert_eq!(header.to_bytes(&header_key()).len(), HEADER_LEN);
}

#[test]
fn the_header_prefix_matches_the_reference_implementation() {
    let prefix = reference_header().mac_input();

    // Field-by-field, at the documented offsets.
    assert_eq!(&prefix[0..8], b"KEYRINGB");
    assert_eq!(&prefix[8..10], &[0x00, 0x01]); // backup_version = 1
    assert_eq!(&prefix[10..12], &[0x00, 0x01]); // envelope_version = 1
    assert_eq!(&prefix[12..16], &[0, 0, 0, 0]); // reserved
    assert_eq!(&prefix[16..48], &[0xB1; 32]); // account_salt
    assert_eq!(&prefix[48..52], &65_536u32.to_be_bytes()); // kdf.m
    assert_eq!(&prefix[52..56], &3u32.to_be_bytes()); // kdf.t
    assert_eq!(&prefix[56..60], &4u32.to_be_bytes()); // kdf.p
    assert_eq!(&prefix[60..92], &[0xB2; 32]); // verifier
    assert_eq!(&prefix[92..124], &[0xB3; 32]); // pubkey_ed25519
    assert_eq!(&prefix[124..188], &[0xB4; 64]); // manifest_sig
    assert_eq!(&prefix[188..196], &1_700_000_000_000i64.to_be_bytes());

    // And the whole prefix, against the independent implementation.
    assert_eq!(
        &prefix[..32],
        hex!("4b455952494e47420001000100000000b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1")
    );
    assert_eq!(
        sha256(&prefix),
        hex!("290fb40da1f2e36e9a20477658ae5c62eb54e8aa1039445638e6c6bd88b50c8d")
    );
}

#[test]
fn the_header_mac_matches_the_reference_implementation() {
    let header = reference_header();
    let bytes = header.to_bytes(&header_key());
    assert_eq!(
        &bytes[HEADER_PREFIX_LEN..],
        hex!("22e6ec41ceba99a3fcf25c74d1c43b1593c03f791d15d839ca482b69620e9541")
    );
    let (parsed, mac) = BackupHeader::parse(&bytes).expect("parse");
    verify_backup_header_mac(&header_key(), &parsed, &mac).expect("verify");
}

#[test]
fn a_backup_manifest_root_matches_the_reference_implementation() {
    let mut entries = reference_entries();
    assert_eq!(
        backup_manifest_root(&mut entries),
        hex!("5a86c3cb08d54666a1f7a85575b369929530546cec37a545b82eb036dca51b8f")
    );
}

fn reference_entries() -> Vec<ManifestEntry> {
    let mut id_two = [0u8; 16];
    id_two.copy_from_slice(&hex!("202122232425262728292a2b2c2d2e2f"));
    let mut id_one = [0u8; 16];
    id_one.copy_from_slice(&hex!("101112131415161718191a1b1c1d1e1f"));

    vec![
        ManifestEntry {
            item_id: id_two,
            revision: 7,
            meta_hash: leaf_hash(b"meta-two"),
            secret_hash: leaf_hash(b"secret-two"),
        },
        ManifestEntry {
            item_id: id_one,
            revision: 1,
            meta_hash: leaf_hash(b"meta-one"),
            secret_hash: leaf_hash(b"secret-one"),
        },
        ManifestEntry {
            item_id: [0xff; 16],
            revision: 99,
            meta_hash: leaf_hash(b""),
            secret_hash: leaf_hash(b""),
        },
    ]
}

// ── Domain separation ────────────────────────────────────────────────────────

#[test]
fn a_backup_root_is_not_a_vault_root_over_the_same_items() {
    // Otherwise a vault's manifest_sig could be replayed into a backup container
    // to vouch for a different set of items, or the reverse.
    let mut a = reference_entries();
    let mut b = reference_entries();
    assert_ne!(backup_manifest_root(&mut a), manifest_root(&mut b));
}

#[test]
fn backup_subkey_info_strings_are_the_documented_literals() {
    assert_eq!(BackupSubkey::Verify.info(), b"keyring/v1/backup/verify");
    assert_eq!(BackupSubkey::Header.info(), b"keyring/v1/backup/header");
    assert_eq!(BackupSubkey::Wrap.info(), b"keyring/v1/backup/wrap");
    assert_eq!(DOMAIN_BACKUP_MANIFEST, b"keyring/v1/backup/manifest");
}

#[test]
fn backup_subkeys_match_the_reference_implementation() {
    let muk = Muk::from_key32(Key32::from_bytes([0xC1; 32]));
    assert_eq!(
        derive_backup_subkey(&muk, BackupSubkey::Verify).expose(),
        &hex!("9dabd27f764926f655fc5b038477a3be1acb3332a9fdcf528e6a50865069531d")
    );
    assert_eq!(
        derive_backup_subkey(&muk, BackupSubkey::Header).expose(),
        &hex!("7d59cefbdfef3ff7f865b0a9f176978ecd1500bb86c9ee8787a0576f3572311f")
    );
    assert_eq!(
        derive_backup_subkey(&muk, BackupSubkey::Wrap).expose(),
        &hex!("aa50266e5160432c2cf5b52251bb2885e97c2fc6d8fa235e40c87cc3689880c3")
    );
}

#[test]
fn a_backup_subkey_never_collides_with_a_vault_subkey() {
    use keyring_crypto::{derive_subkey, Subkey};
    let muk = Muk::from_key32(Key32::from_bytes([0xC1; 32]));

    let vault_keys = [
        Subkey::Verify,
        Subkey::Header,
        Subkey::Wrap,
        Subkey::Vault,
        Subkey::AppCache,
    ]
    .map(|s| *derive_subkey(&muk, s).expose());

    for which in [
        BackupSubkey::Verify,
        BackupSubkey::Header,
        BackupSubkey::Wrap,
    ] {
        let backup = *derive_backup_subkey(&muk, which).expose();
        assert!(
            !vault_keys.contains(&backup),
            "{which:?} collides with a vault subkey derived from the same MUK"
        );
    }
}

#[test]
fn a_backup_has_its_own_salt_and_cost_independent_of_the_vault() {
    // SPEC-V1 §7.8: an independent passphrase. A backup outlives the machine it
    // came from, so it must not inherit the vault's 2026 KDF cost.
    let vault_muk = derive_muk(
        b"the vault master password",
        &[0x01; 32],
        KdfParams::floor(),
    )
    .expect("vault muk");
    let backup_muk = keyring_crypto::derive_backup_muk(
        b"a different backup passphrase",
        &[0x02; 32],
        KdfParams::floor(),
    )
    .expect("backup muk");
    assert_ne!(vault_muk.expose(), backup_muk.expose());
}

// ── Verifier ─────────────────────────────────────────────────────────────────

#[test]
fn the_backup_verifier_accepts_only_the_right_passphrase() {
    let salt = [0x77; 32];
    let right = keyring_crypto::derive_backup_muk(b"correct passphrase", &salt, KdfParams::floor())
        .expect("derive");
    let stored = backup_verifier_from(&right);
    assert!(verify_backup_passphrase(&right, &stored));

    let wrong = keyring_crypto::derive_backup_muk(b"wrong passphrase", &salt, KdfParams::floor())
        .expect("derive");
    assert!(!verify_backup_passphrase(&wrong, &stored));
}

// ── Parsing fails closed ─────────────────────────────────────────────────────

#[test]
fn a_header_without_the_magic_is_rejected() {
    let mut bytes = reference_header().to_bytes(&header_key());
    bytes[0] ^= 0xff;
    assert_eq!(
        BackupHeader::parse(&bytes).unwrap_err(),
        CryptoError::MalformedEnvelope
    );
}

#[test]
fn a_truncated_header_is_rejected() {
    let bytes = reference_header().to_bytes(&header_key());
    for cut in 0..HEADER_LEN {
        assert_eq!(
            BackupHeader::parse(&bytes[..cut]).unwrap_err(),
            CryptoError::MalformedEnvelope,
            "a {cut}-byte input parsed as a backup header"
        );
    }
    assert!(BackupHeader::parse(&bytes).is_ok());
}

#[test]
fn an_unknown_backup_version_is_a_hard_error() {
    let mut bytes = reference_header().to_bytes(&header_key());
    bytes[8] = 0x00;
    bytes[9] = 0x63; // version 99
    assert_eq!(
        BackupHeader::parse(&bytes).unwrap_err(),
        CryptoError::UnsupportedEnvelopeVersion {
            found: 99,
            supported: BACKUP_VERSION,
        }
    );
}

#[test]
fn a_non_zero_reserved_field_is_rejected() {
    // Reserved means reserved: a future version may use it, and a build that
    // ignored it would silently misread whatever lands there.
    let mut bytes = reference_header().to_bytes(&header_key());
    bytes[15] = 0x01;
    assert_eq!(
        BackupHeader::parse(&bytes).unwrap_err(),
        CryptoError::MalformedEnvelope
    );
}

#[test]
fn every_header_field_is_covered_by_the_mac() {
    let key = header_key();
    let base = reference_header();
    let baseline = base.to_bytes(&key);
    let (_, stored) = BackupHeader::parse(&baseline).expect("parse");

    let mutations: Vec<(&str, BackupHeader)> = vec![
        (
            "envelope_version",
            BackupHeader {
                envelope_version: 2,
                ..base.clone()
            },
        ),
        (
            "account_salt",
            BackupHeader {
                account_salt: [0x00; 32],
                ..base.clone()
            },
        ),
        (
            "kdf downgrade",
            BackupHeader {
                kdf: KdfParams {
                    m_kib: 19_456,
                    t: 2,
                    p: 1,
                },
                ..base.clone()
            },
        ),
        (
            "verifier",
            BackupHeader {
                verifier: [0x00; 32],
                ..base.clone()
            },
        ),
        (
            "pubkey_ed25519",
            BackupHeader {
                pubkey_ed25519: [0x00; 32],
                ..base.clone()
            },
        ),
        (
            "manifest_sig",
            BackupHeader {
                manifest_sig: [0x00; 64],
                ..base.clone()
            },
        ),
        (
            "created_at",
            BackupHeader {
                created_at: 0,
                ..base.clone()
            },
        ),
    ];

    for (what, mutated) in mutations {
        assert_eq!(
            verify_backup_header_mac(&key, &mutated, &stored),
            Err(CryptoError::BadHeaderMac),
            "{what} is not covered by the backup header MAC"
        );
    }
}

#[test]
fn every_backup_header_mac_bit_flip_is_rejected() {
    let key = header_key();
    let header = reference_header();
    let bytes = header.to_bytes(&key);
    let (parsed, stored) = BackupHeader::parse(&bytes).expect("parse");

    for byte in 0..stored.len() {
        for bit in 0..8u32 {
            let mut tampered = stored;
            tampered[byte] ^= 1 << bit;
            assert_eq!(
                verify_backup_header_mac(&key, &parsed, &tampered),
                Err(CryptoError::BadHeaderMac),
                "flipping bit {bit} of MAC byte {byte} was not detected"
            );
        }
    }
}

#[test]
fn a_header_round_trips_through_serialisation() {
    let header = reference_header();
    let bytes = header.to_bytes(&header_key());
    let (parsed, _) = BackupHeader::parse(&bytes).expect("parse");
    assert_eq!(parsed, header);
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}
