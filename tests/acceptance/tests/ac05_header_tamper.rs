//! SPEC-V1 §11: swap a public key in the header → `header_mac` verification fails.
//!
//! Without this, the manifest signature is worthless: an attacker who can rewrite
//! an item row can also rewrite `pubkey_ed25519`, sign a manifest of their own
//! choosing with a key they control, and the signature still checks out. The
//! header MAC under `muk.header` is what binds the public keys to the master
//! password, and it must be verified immediately after key derivation — before
//! anything else is read.
//!
//! FROZEN. See `tests/acceptance/API.md`.

use keyring_acceptance::{fixture_params, MASTER};
use rusqlite::Connection;
use store::{ItemBody, ItemDraft, TamperKind, UnlockError, VaultFile};

fn tamper(path: &std::path::Path, sql: &str, value: Vec<u8>) {
    let conn = Connection::open(path).expect("attacker opens the file");
    conn.execute(sql, rusqlite::params![value])
        .expect("attacker rewrites the header");
}

fn seeded_vault(path: &std::path::Path) {
    let file = VaultFile::create(path, MASTER, fixture_params()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault_add");
    session
        .item_upsert(&ItemDraft::new(vault_id, "an item", ItemBody::SecureNote))
        .expect("upsert");
}

fn expect_header_mac_failure(path: &std::path::Path, what: &str) {
    let file = VaultFile::open(path).expect("reopen");
    match file.unlock(MASTER) {
        Err(UnlockError::TamperDetected(TamperKind::HeaderMac)) => {}
        other => panic!("{what} was not caught by the header MAC: {other:?}"),
    }
}

#[test]
fn swapping_the_ed25519_public_key_fails_the_header_mac() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    seeded_vault(&path);

    tamper(
        &path,
        "UPDATE header SET pubkey_ed25519 = ?1 WHERE id = 1",
        vec![0x42; 32],
    );
    expect_header_mac_failure(&path, "an ed25519 public key swap");
}

#[test]
fn swapping_the_x25519_public_key_fails_the_header_mac() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    seeded_vault(&path);

    tamper(
        &path,
        "UPDATE header SET pubkey_x25519 = ?1 WHERE id = 1",
        vec![0x42; 32],
    );
    expect_header_mac_failure(&path, "an x25519 public key swap");
}

#[test]
fn downgrading_the_kdf_parameters_fails_the_header_mac() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    seeded_vault(&path);

    // An attacker who can weaken the stored cost makes every future offline
    // attack cheaper. The MAC covers the parsed parameters, so this must fail.
    let conn = Connection::open(&path).expect("attacker opens the file");
    conn.execute(
        r#"UPDATE header SET kdf_params = '{"m":19456,"t":1,"p":1}' WHERE id = 1"#,
        [],
    )
    .expect("attacker weakens the kdf params");
    drop(conn);

    expect_header_mac_failure(&path, "a kdf parameter downgrade");
}

#[test]
fn replacing_the_manifest_signature_fails_the_header_mac() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    seeded_vault(&path);

    tamper(
        &path,
        "UPDATE header SET manifest_sig = ?1 WHERE id = 1",
        vec![0x00; 64],
    );
    expect_header_mac_failure(&path, "a manifest signature replacement");
}
