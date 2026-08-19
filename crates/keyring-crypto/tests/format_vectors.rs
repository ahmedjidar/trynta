// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pinned vectors for Trynta's *own* on-disk encodings.
//!
//! SPEC-V1 §3.3 and §3.5 warn that two implementations disagreeing about these
//! bytes cannot read each other's vaults, and the failure mode is "nobody can
//! open anything". So the expected values below were produced by an independent
//! implementation of the documented layouts, not by running this code and
//! recording what it printed.
//!
//! The reference implementation, reproduced so anyone can re-derive the values:
//!
//! ```python
//! import hashlib, hmac, struct
//!
//! def leaf(ct):
//!     return hashlib.blake2b(b"keyring/v1/manifest/leaf" + ct, digest_size=32).digest()
//!
//! def root(entries):                       # entries: (item_id, revision, meta_ct, secret_ct)
//!     h = hashlib.blake2b(digest_size=32)
//!     h.update(b"keyring/v1/manifest")
//!     h.update(struct.pack(">Q", len(entries)))
//!     for item_id, rev, meta, secret in sorted(entries, key=lambda e: e[0]):
//!         h.update(item_id)
//!         h.update(struct.pack(">Q", rev))
//!         h.update(leaf(meta))
//!         h.update(leaf(secret))
//!     return h.digest()
//!
//! def header(schema, payload, env, salt, m, t, p, verifier, pkx, pke, privct, sig, created):
//!     out = bytearray(b"keyring/v1/header")
//!     out += struct.pack(">I", schema) + struct.pack(">I", payload) + struct.pack(">H", env)
//!     out += struct.pack(">I", m) + struct.pack(">I", t) + struct.pack(">I", p)
//!     for f in (salt, verifier, pkx, pke, privct, sig):
//!         out += struct.pack(">I", len(f)) + f
//!     out += struct.pack(">q", created)
//!     return bytes(out)
//! ```

use hex_literal::hex;
use keyring_crypto::{
    header_mac, leaf_hash, manifest_root, Aad, HeaderFields, KdfParams, Key32, ManifestEntry,
    Purpose, AAD_LEN,
};

// ── Canonical AAD, SPEC-V1 §3.3 ──────────────────────────────────────────────

#[test]
fn aad_encoding_is_exactly_43_bytes_in_the_documented_order() {
    let mut subject = [0u8; 16];
    subject.copy_from_slice(&hex!("101112131415161718191a1b1c1d1e1f"));
    let mut key_id = [0u8; 16];
    key_id.copy_from_slice(&hex!("a0a1a2a3a4a5a6a7a8a9aaabacadaeaf"));

    let aad = Aad {
        envelope_version: 1,
        purpose: Purpose::ItemSecret,
        subject_id: subject,
        revision: 0x0102_0304_0506_0708,
        key_id,
    };

    assert_eq!(AAD_LEN, 43);
    assert_eq!(
        aad.encode(),
        hex!(
            "0001"                              // envelope_version, u16 BE
            "02"                                // purpose = ItemSecret
            "101112131415161718191a1b1c1d1e1f"  // subject_id
            "0102030405060708"                  // revision, u64 BE
            "a0a1a2a3a4a5a6a7a8a9aaabacadaeaf"  // key_id
        )
    );
}

#[test]
fn purpose_discriminants_are_frozen() {
    // These numbers are on disk. Renumbering them silently reinterprets every
    // existing vault's envelopes.
    assert_eq!(Purpose::ItemMeta.as_u8(), 1);
    assert_eq!(Purpose::ItemSecret.as_u8(), 2);
    assert_eq!(Purpose::VaultMeta.as_u8(), 3);
    assert_eq!(Purpose::Activity.as_u8(), 4);
    assert_eq!(Purpose::AppCache.as_u8(), 5);
    assert_eq!(Purpose::Backup.as_u8(), 6);
}

// ── Manifest, SPEC-V1 §3.5 ───────────────────────────────────────────────────

#[test]
fn leaf_hash_matches_the_reference_implementation() {
    assert_eq!(
        leaf_hash(b"meta-one"),
        hex!("58d4da756e6190717351c521dc809a1512d50e1aae87538106dd4358fb8d0a73")
    );
    assert_eq!(
        leaf_hash(b""),
        hex!("4e10d4a8dbecdcd8fc4a892e6b18ce2bf2dc8c6f3f6a6df4445887cac9098356")
    );
}

fn reference_entries() -> Vec<ManifestEntry> {
    // Deliberately out of id order, to prove the root sorts.
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

#[test]
fn manifest_root_matches_the_reference_implementation() {
    let mut entries = reference_entries();
    assert_eq!(
        manifest_root(&mut entries),
        hex!("568e2a20761774495f14e154f08a62ff57145dd619a5199ad33eb1a6db1f0621")
    );
}

#[test]
fn manifest_root_is_independent_of_row_order() {
    let mut a = reference_entries();
    let mut b = reference_entries();
    b.reverse();
    assert_eq!(manifest_root(&mut a), manifest_root(&mut b));
}

#[test]
fn manifest_root_commits_to_the_entry_count() {
    // Without the count prefix, a vault of two items could in principle collide
    // with a differently-partitioned stream of the same bytes.
    let mut all = reference_entries();
    let mut fewer = reference_entries();
    fewer.pop();
    assert_ne!(manifest_root(&mut all), manifest_root(&mut fewer));
}

#[test]
fn manifest_root_changes_when_a_revision_changes() {
    let mut before = reference_entries();
    let mut after = reference_entries();
    after[0].revision += 1;
    assert_ne!(manifest_root(&mut before), manifest_root(&mut after));
}

#[test]
fn manifest_root_changes_when_a_ciphertext_changes() {
    let mut before = reference_entries();
    let mut after = reference_entries();
    after[1].secret_hash = leaf_hash(b"secret-one-rotated");
    assert_ne!(manifest_root(&mut before), manifest_root(&mut after));
}

// ── Header MAC, SPEC-V1 §3.5 ─────────────────────────────────────────────────

fn reference_header<'a>(
    salt: &'a [u8],
    verifier: &'a [u8],
    pkx: &'a [u8],
    pke: &'a [u8],
    privct: &'a [u8],
    sig: &'a [u8],
) -> HeaderFields<'a> {
    HeaderFields {
        schema_version: 1,
        payload_version: 1,
        envelope_version: 1,
        account_salt: salt,
        kdf: KdfParams {
            m_kib: 65_536,
            t: 3,
            p: 4,
        },
        verifier,
        pubkey_x25519: pkx,
        pubkey_ed25519: pke,
        privkeys_ct: privct,
        manifest_sig: sig,
        created_at: 1_700_000_000_000,
    }
}

#[test]
fn header_mac_matches_the_reference_implementation() {
    let salt = [0x11u8; 32];
    let verifier = [0x22u8; 32];
    let pkx = [0x33u8; 32];
    let pke = [0x44u8; 32];
    let privct = [0x55u8; 80];
    let sig = [0x66u8; 64];

    let header = reference_header(&salt, &verifier, &pkx, &pke, &privct, &sig);
    assert_eq!(header.canonical_bytes().len(), 343);

    let key = Key32::from_bytes([0xa5; 32]);
    assert_eq!(
        header_mac(&key, &header),
        hex!("267457eb4543607211c801d1c59376276dd18222b6515e5cc6cbc3c99013501b")
    );
}

#[test]
fn header_mac_covers_every_field_it_claims_to() {
    let salt = [0x11u8; 32];
    let verifier = [0x22u8; 32];
    let pkx = [0x33u8; 32];
    let pke = [0x44u8; 32];
    let privct = [0x55u8; 80];
    let sig = [0x66u8; 64];
    let key = Key32::from_bytes([0xa5; 32]);

    let base = reference_header(&salt, &verifier, &pkx, &pke, &privct, &sig);
    let baseline = header_mac(&key, &base);

    // Every mutation below is something an attacker with file write access would
    // try. All of them must change the MAC.
    let mut mutated = base;
    mutated.schema_version = 2;
    assert_ne!(header_mac(&key, &mutated), baseline, "schema_version");

    let mut mutated = base;
    mutated.payload_version = 2;
    assert_ne!(header_mac(&key, &mutated), baseline, "payload_version");

    let mut mutated = base;
    mutated.envelope_version = 2;
    assert_ne!(header_mac(&key, &mutated), baseline, "envelope_version");

    // A KDF downgrade makes every future offline attack cheaper. This is the
    // reason the MAC covers parsed integers rather than the stored JSON text.
    let mut mutated = base;
    mutated.kdf = KdfParams {
        m_kib: 19_456,
        t: 2,
        p: 1,
    };
    assert_ne!(header_mac(&key, &mutated), baseline, "kdf params");

    let other = [0x99u8; 32];
    let mut mutated = base;
    mutated.pubkey_ed25519 = &other;
    assert_ne!(header_mac(&key, &mutated), baseline, "pubkey_ed25519");

    let mut mutated = base;
    mutated.pubkey_x25519 = &other;
    assert_ne!(header_mac(&key, &mutated), baseline, "pubkey_x25519");

    let mut mutated = base;
    mutated.verifier = &other;
    assert_ne!(header_mac(&key, &mutated), baseline, "verifier");

    let mut mutated = base;
    mutated.account_salt = &other;
    assert_ne!(header_mac(&key, &mutated), baseline, "account_salt");

    let other_sig = [0x00u8; 64];
    let mut mutated = base;
    mutated.manifest_sig = &other_sig;
    assert_ne!(header_mac(&key, &mutated), baseline, "manifest_sig");

    let other_ct = [0x77u8; 80];
    let mut mutated = base;
    mutated.privkeys_ct = &other_ct;
    assert_ne!(header_mac(&key, &mutated), baseline, "privkeys_ct");

    let mut mutated = base;
    mutated.created_at = 0;
    assert_ne!(header_mac(&key, &mutated), baseline, "created_at");
}

#[test]
fn header_canonical_encoding_is_unambiguous_across_field_boundaries() {
    // Length prefixes exist so that moving a byte from one variable-length field
    // to the next cannot produce the same encoding.
    let key = Key32::from_bytes([0xa5; 32]);
    let salt = [0x11u8; 32];
    let sig = [0x66u8; 64];

    // Same 64 bytes of key material, split 33/31 in one header and 32/32 in the
    // other. Without length prefixes both would encode identically.
    let shifted_agreement = [0xaau8; 33];
    let shifted_signing = [0xaau8; 31];
    let even_agreement = [0xaau8; 32];
    let even_signing = [0xaau8; 32];
    let verifier = [0x22u8; 32];
    let privct = [0x55u8; 80];

    let shifted = reference_header(
        &salt,
        &verifier,
        &shifted_agreement,
        &shifted_signing,
        &privct,
        &sig,
    );
    let even = reference_header(
        &salt,
        &verifier,
        &even_agreement,
        &even_signing,
        &privct,
        &sig,
    );
    assert_ne!(header_mac(&key, &shifted), header_mac(&key, &even));
}
