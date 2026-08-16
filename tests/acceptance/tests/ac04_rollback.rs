//! SPEC-V1 §11: restore an item row from an earlier snapshot → unlock refuses
//! with `TamperDetected`.
//!
//! This is the attack the AAD alone does not stop. Every AAD field on the restored
//! row is correct, because the row is a genuine historical record of that item.
//! Only the signed manifest (§3.5) catches it.
//!
//! The tampering is done with `rusqlite` directly, deliberately bypassing our own
//! writer — a rollback test that goes through the code that maintains the manifest
//! proves nothing.
//!
//! FROZEN. See `tests/acceptance/API.md`.

use keyring_acceptance::{fixture_params, MASTER};
use rusqlite::Connection;
use store::{ItemBody, ItemDraft, SecretField, TamperKind, UnlockError, VaultFile};
use uuid::Uuid;

struct RawItemRow {
    item_key_ct: Vec<u8>,
    meta_ct: Vec<u8>,
    secret_ct: Vec<u8>,
    revision: i64,
    updated_at: i64,
}

fn read_raw(path: &std::path::Path, id: Uuid) -> RawItemRow {
    let conn = Connection::open(path).expect("attacker opens the file");
    conn.query_row(
        "SELECT item_key_ct, meta_ct, secret_ct, revision, updated_at FROM items WHERE id = ?1",
        [id.as_bytes().as_slice()],
        |r| {
            Ok(RawItemRow {
                item_key_ct: r.get(0)?,
                meta_ct: r.get(1)?,
                secret_ct: r.get(2)?,
                revision: r.get(3)?,
                updated_at: r.get(4)?,
            })
        },
    )
    .expect("read raw item row")
}

fn write_raw(path: &std::path::Path, id: Uuid, row: &RawItemRow) {
    let conn = Connection::open(path).expect("attacker opens the file");
    conn.execute(
        "UPDATE items SET item_key_ct = ?1, meta_ct = ?2, secret_ct = ?3, revision = ?4, \
         updated_at = ?5 WHERE id = ?6",
        rusqlite::params![
            row.item_key_ct,
            row.meta_ct,
            row.secret_ct,
            row.revision,
            row.updated_at,
            id.as_bytes().as_slice(),
        ],
    )
    .expect("attacker restores the earlier row");
}

#[test]
fn restoring_an_earlier_item_revision_is_detected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    let id;
    let old_row;
    {
        let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault_add");

        id = session
            .item_upsert(&ItemDraft::new(
                vault_id,
                "Rotated account",
                ItemBody::Login {
                    username: "user".to_owned(),
                    password: "breached-password-before-rotation".to_owned(),
                    urls: vec![],
                    totp: None,
                },
            ))
            .expect("upsert");

        old_row = read_raw(&path, id);

        // The user rotates the password after a breach notification.
        let mut rotated = ItemDraft::new(
            vault_id,
            "Rotated account",
            ItemBody::Login {
                username: "user".to_owned(),
                password: "fresh-password-after-rotation".to_owned(),
                urls: vec![],
                totp: None,
            },
        );
        rotated.id = Some(id);
        session.item_upsert(&rotated).expect("rotate");

        let now = session
            .item_secret(id, SecretField::Password)
            .expect("reveal");
        assert_eq!(&*now, "fresh-password-after-rotation");

        let new_row = read_raw(&path, id);
        assert!(
            new_row.revision > old_row.revision,
            "rotation must bump the revision, or there is nothing to roll back"
        );
    }

    // The attacker restores the pre-rotation row wholesale.
    write_raw(&path, id, &old_row);

    let file = VaultFile::open(&path).expect("reopen");
    match file.unlock(MASTER) {
        Err(UnlockError::TamperDetected(kind)) => {
            assert!(
                matches!(
                    kind,
                    TamperKind::ManifestRoot | TamperKind::ManifestSignature
                ),
                "rollback should be caught by the manifest, got {kind:?}"
            );
        }
        Ok(session) => {
            let pw = session.item_secret(id, SecretField::Password).ok();
            panic!(
                "unlock succeeded after a rollback — the rotated password was silently \
                 reverted (revealed: {pw:?})"
            );
        }
        Err(other) => panic!("expected TamperDetected, got {other:?}"),
    }
}

#[test]
fn a_deleted_item_cannot_be_resurrected_by_clearing_deleted_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    let id;
    {
        let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault_add");
        id = session
            .item_upsert(&ItemDraft::new(vault_id, "gone", ItemBody::SecureNote))
            .expect("upsert");
        session.item_delete(id).expect("delete");
    }

    let conn = Connection::open(&path).expect("attacker opens the file");
    conn.execute(
        "UPDATE items SET deleted_at = NULL WHERE id = ?1",
        [id.as_bytes().as_slice()],
    )
    .expect("attacker un-deletes the row");
    drop(conn);

    let file = VaultFile::open(&path).expect("reopen");
    match file.unlock(MASTER) {
        Err(UnlockError::TamperDetected(_)) => {}
        other => panic!(
            "resurrecting a soft-deleted row changes the live item set and must be \
             caught by the manifest, got {other:?}"
        ),
    }
}
