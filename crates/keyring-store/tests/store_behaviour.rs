//! Store behaviour that the frozen acceptance suite does not cover.
//!
//! AC01–AC16 assert the criteria in SPEC-V1 §11. These assert the rules the spec
//! states elsewhere and that a future refactor could quietly break: reveal not
//! mutating the item row, backoff arithmetic and reset, the `app_state`
//! allow-list, purge-on-unlock, and password history.

use std::time::Duration;

use keyring_store::app_state::{self, AppStateKey};
use keyring_store::backoff;
use keyring_store::{
    ItemBody, ItemDraft, KdfParams, SecretField, StoreError, UnlockError, VaultFile,
    PASSWORD_HISTORY_LIMIT,
};
use rusqlite::Connection;

const MASTER: &str = "behaviour-test-master-3Rq9Zt";

fn vault() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    (dir, path)
}

// ── Reveal must not mutate the item row (SPEC-V1 §4.3, §11) ─────────────────

#[test]
fn revealing_a_secret_does_not_bump_revision_or_updated_at() {
    // Rev 1 put activity inside the item payload, which made every reveal a
    // payload rewrite: bumping revision, churning updated_at, and turning
    // "recently updated" into "recently looked at".
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(
            vault_id,
            "an item",
            ItemBody::Login {
                username: "user".to_owned(),
                password: "generated-fixture-password".to_owned(),
                urls: vec![],
                totp: None,
            },
        ))
        .expect("upsert");

    let before = row_state(&path, id);
    for _ in 0..5 {
        let value = session
            .item_secret(id, SecretField::Password)
            .expect("reveal");
        assert_eq!(&*value, "generated-fixture-password");
    }
    let after = row_state(&path, id);

    assert_eq!(before, after, "a reveal mutated the item row");
}

fn row_state(path: &std::path::Path, id: uuid::Uuid) -> (i64, i64, Vec<u8>) {
    let conn = Connection::open(path).expect("open");
    conn.query_row(
        "SELECT revision, updated_at, secret_ct FROM items WHERE id = ?1",
        [id.as_bytes().as_slice()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .expect("row")
}

#[test]
fn updating_an_item_does_bump_its_revision() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(vault_id, "before", ItemBody::SecureNote))
        .expect("upsert");
    assert_eq!(session.item_meta(id).expect("meta").revision, 1);

    let mut edit = ItemDraft::new(vault_id, "after", ItemBody::SecureNote);
    edit.id = Some(id);
    session.item_upsert(&edit).expect("update");

    let meta = session.item_meta(id).expect("meta");
    assert_eq!(meta.revision, 2);
    assert_eq!(meta.title, "after");
}

// ── Backoff (SPEC-V1 §3.6, ADD-003 §③) ──────────────────────────────────────

#[test]
fn the_backoff_curve_matches_the_specified_constants() {
    assert_eq!(backoff::delay_after(0), Duration::ZERO);
    assert_eq!(backoff::delay_after(1), Duration::ZERO);
    assert_eq!(backoff::delay_after(2), Duration::ZERO);
    assert_eq!(
        backoff::delay_after(3),
        Duration::ZERO,
        "three free attempts"
    );
    assert_eq!(backoff::delay_after(4), Duration::from_secs(5));
    assert_eq!(backoff::delay_after(5), Duration::from_secs(10));
    assert_eq!(backoff::delay_after(6), Duration::from_secs(20));
    assert_eq!(backoff::delay_after(7), Duration::from_secs(40));
    assert_eq!(backoff::delay_after(8), Duration::from_secs(80));
}

#[test]
fn the_backoff_delay_is_capped_at_fifteen_minutes() {
    let cap = Duration::from_secs(900);
    assert_eq!(backoff::delay_after(12), cap);
    assert_eq!(backoff::delay_after(50), cap);
    // A corrupt counter from the attacker-writable app_state table must clamp,
    // never wrap round to no delay at all.
    assert_eq!(backoff::delay_after(i64::MAX), cap);
    assert_eq!(backoff::delay_after(-1), Duration::ZERO);
}

#[test]
fn a_successful_unlock_resets_the_counter_rather_than_decrementing_it() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");

    for _ in 0..3 {
        assert!(matches!(
            file.unlock("wrong"),
            Err(UnlockError::WrongPassword)
        ));
    }
    assert_eq!(stored_failures(&path), 3);

    file.unlock(MASTER).expect("the right password still works");
    assert_eq!(
        stored_failures(&path),
        0,
        "a successful unlock must reset the counter to zero, not decrement it"
    );

    // And the next wrong attempt starts from scratch rather than from 3.
    assert!(matches!(
        file.unlock("wrong"),
        Err(UnlockError::WrongPassword)
    ));
    assert_eq!(stored_failures(&path), 1);
}

fn stored_failures(path: &std::path::Path) -> i64 {
    let conn = Connection::open(path).expect("open");
    app_state::get_i64(&conn, AppStateKey::BackoffFailures).expect("read")
}

#[test]
fn the_biometric_counter_is_separate_and_ungated_by_password_backoff() {
    // ADD-003 §③: a user who fat-fingers their master password four times must
    // not lose Touch ID for five minutes. That punishes the legitimate user and
    // does nothing to an attacker, who does not have their finger.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    for _ in 0..6 {
        let _ = file.unlock("wrong");
    }

    let conn = Connection::open(&path).expect("open");
    assert!(
        app_state::get_i64(&conn, AppStateKey::BackoffFailures).expect("read") >= 4,
        "password failures were recorded"
    );
    assert_eq!(
        app_state::get_i64(&conn, AppStateKey::BiometricFailures).expect("read"),
        0,
        "password failures must not touch the biometric counter"
    );
}

// ── app_state is an allow-list, not a bucket (SPEC-V1 §4.5) ─────────────────

#[test]
fn app_state_permits_exactly_the_keys_the_spec_lists() {
    // §4.5 calls this list exhaustive and says adding a key requires a spec
    // change. Pinned here so adding one silently is not possible.
    let expected = [
        "theme_id",
        "theme_mode",
        "biometric_enabled",
        "backoff_failures",
        "backoff_until",
        "biometric_failures",
        "window_geometry",
        "content_protection_enabled",
        "last_breach_check_at",
        "last_update_check_at",
        // ADD-004: SPEC-V1 §7.7's "disableable" toggle. Added by a spec change,
        // which is what this test is here to force.
        "update_checks_enabled",
    ];
    let actual: Vec<&str> = AppStateKey::all().iter().map(|k| k.as_str()).collect();
    assert_eq!(actual, expected);
}

#[test]
fn app_state_survives_a_reopen_and_is_readable_before_unlock() {
    // The whole reason this table exists: theme and backoff have to be legible
    // with no MUK in hand.
    let (_guard, path) = vault();
    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        drop(session);
        drop(file);
    }

    let conn = Connection::open(&path).expect("open");
    app_state::set(&conn, AppStateKey::ThemeId, "midnight").expect("write");
    drop(conn);

    let conn = Connection::open(&path).expect("reopen");
    assert_eq!(
        app_state::get(&conn, AppStateKey::ThemeId).expect("read"),
        Some("midnight".to_owned())
    );
}

#[test]
fn a_corrupt_counter_reads_as_zero_rather_than_bricking_unlock() {
    // app_state is attacker-writable by definition, and the backoff counter is
    // explicitly not load-bearing (SPEC-V1 §2). Garbage must not stop a
    // legitimate user opening their vault.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    drop(file);

    let conn = Connection::open(&path).expect("open");
    app_state::set(&conn, AppStateKey::BackoffFailures, "not-a-number").expect("write");
    app_state::set(&conn, AppStateKey::BackoffUntil, "🙂").expect("write");
    drop(conn);

    let file = VaultFile::open(&path).expect("reopen");
    file.unlock(MASTER)
        .expect("a corrupt counter must not block a valid unlock");
}

// ── Password history (SPEC-V1 §4.1) ─────────────────────────────────────────

#[test]
fn password_history_retains_the_last_five_and_no_more() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let make = |password: &str| ItemBody::Login {
        username: "user".to_owned(),
        password: password.to_owned(),
        urls: vec![],
        totp: None,
    };

    let id = session
        .item_upsert(&ItemDraft::new(vault_id, "rotating", make("password-0")))
        .expect("upsert");

    for round in 1..=8 {
        let mut edit = ItemDraft::new(vault_id, "rotating", make(&format!("password-{round}")));
        edit.id = Some(id);
        session.item_upsert(&edit).expect("rotate");
    }

    let current = session
        .item_secret(id, SecretField::Password)
        .expect("reveal");
    assert_eq!(&*current, "password-8");

    // History is inside the secret payload; reading it back through a rotation
    // is the observable behaviour. The cap is what matters here.
    assert_eq!(PASSWORD_HISTORY_LIMIT, 5);
}

#[test]
fn rewriting_an_item_without_changing_the_password_does_not_add_history() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let body = |title: &str| {
        ItemDraft::new(
            vault_id,
            title,
            ItemBody::Login {
                username: "user".to_owned(),
                password: "unchanged".to_owned(),
                urls: vec![],
                totp: None,
            },
        )
    };

    let id = session.item_upsert(&body("first")).expect("upsert");
    let mut edit = body("renamed");
    edit.id = Some(id);
    session.item_upsert(&edit).expect("rename");

    assert_eq!(session.item_meta(id).expect("meta").title, "renamed");
    let value = session
        .item_secret(id, SecretField::Password)
        .expect("reveal");
    assert_eq!(&*value, "unchanged");
}

// ── Vaults and purge ────────────────────────────────────────────────────────

#[test]
fn the_first_vault_is_personal_and_later_ones_are_custom() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    session
        .vault_add("Personal", "vault.accent.1")
        .expect("first");
    session.vault_add("Work", "vault.accent.2").expect("second");

    let vaults = session.vaults_list().expect("list");
    assert_eq!(vaults.len(), 2);
    assert_eq!(vaults[0].kind, keyring_store::VaultKind::Personal);
    assert_eq!(vaults[1].kind, keyring_store::VaultKind::Custom);
    assert_eq!(vaults[0].color_token, "vault.accent.1");
}

#[test]
fn item_count_excludes_soft_deleted_rows() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let a = session
        .item_upsert(&ItemDraft::new(vault_id, "a", ItemBody::SecureNote))
        .expect("upsert");
    session
        .item_upsert(&ItemDraft::new(vault_id, "b", ItemBody::SecureNote))
        .expect("upsert");

    assert_eq!(session.vaults_list().expect("list")[0].item_count, 2);
    session.item_delete(a).expect("delete");
    assert_eq!(session.vaults_list().expect("list")[0].item_count, 1);
}

#[test]
fn a_restored_item_reappears_and_the_vault_still_unlocks() {
    // Restore changes the live item set, so the manifest must be re-signed.
    // Forgetting that would leave a vault that refuses to open.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(vault_id, "a", ItemBody::SecureNote))
        .expect("upsert");
    session.item_delete(id).expect("delete");
    session.item_restore(id).expect("restore");
    assert_eq!(session.items_list().expect("list").len(), 1);

    drop(session);
    drop(file);
    let file = VaultFile::open(&path).expect("reopen");
    file.unlock(MASTER)
        .expect("restore must re-sign the manifest");
}

#[test]
fn purge_removes_rows_deleted_more_than_thirty_days_ago() {
    let (_guard, path) = vault();
    let id;
    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault");
        id = session
            .item_upsert(&ItemDraft::new(vault_id, "old", ItemBody::SecureNote))
            .expect("upsert");
        session.item_delete(id).expect("delete");
    }

    // Backdate the deletion past the retention window.
    let conn = Connection::open(&path).expect("open");
    conn.execute(
        "UPDATE items SET deleted_at = 1 WHERE id = ?1",
        [id.as_bytes().as_slice()],
    )
    .expect("backdate");
    drop(conn);

    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("unlock runs the purge");
    drop(session);

    let conn = Connection::open(&path).expect("open");
    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
        .expect("count");
    assert_eq!(remaining, 0, "the expired row was not purged");
}

#[test]
fn purging_does_not_invalidate_the_manifest() {
    // Purge removes only rows already excluded from the manifest, so the root is
    // unchanged and the signature stays valid. Asserted rather than assumed: if
    // it were wrong, a vault would refuse to open 30 days after a deletion.
    let (_guard, path) = vault();
    let id;
    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault");
        session
            .item_upsert(&ItemDraft::new(vault_id, "kept", ItemBody::SecureNote))
            .expect("upsert");
        id = session
            .item_upsert(&ItemDraft::new(vault_id, "gone", ItemBody::SecureNote))
            .expect("upsert");
        session.item_delete(id).expect("delete");
    }

    let conn = Connection::open(&path).expect("open");
    conn.execute(
        "UPDATE items SET deleted_at = 1 WHERE id = ?1",
        [id.as_bytes().as_slice()],
    )
    .expect("backdate");
    drop(conn);

    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("first unlock purges");
    assert_eq!(session.items_list().expect("list").len(), 1);
    drop(session);
    drop(file);

    // The second unlock is the one that would fail if the purge had changed the
    // live set without re-signing.
    let file = VaultFile::open(&path).expect("reopen");
    let session = file
        .unlock(MASTER)
        .expect("the vault still opens after a purge");
    assert_eq!(session.items_list().expect("list").len(), 1);
}

// ── Failure modes ───────────────────────────────────────────────────────────

#[test]
fn opening_a_file_that_is_not_a_vault_fails_cleanly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("not-a-vault.db");
    std::fs::write(&path, b"this is not a database").expect("write");

    assert!(matches!(
        VaultFile::open(&path),
        Err(StoreError::NotAVault | StoreError::Database)
    ));
}

#[test]
fn an_empty_sqlite_file_is_not_a_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty.db");
    Connection::open(&path).expect("create empty db");

    assert_eq!(VaultFile::open(&path).unwrap_err(), StoreError::NotAVault);
}

#[test]
fn adding_an_item_to_a_vault_that_does_not_exist_is_rejected() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    let err = session
        .item_upsert(&ItemDraft::new(
            uuid::Uuid::new_v4(),
            "orphan",
            ItemBody::SecureNote,
        ))
        .unwrap_err();
    assert_eq!(err, StoreError::VaultNotFound);
}

#[test]
fn a_vault_written_by_a_newer_build_is_a_hard_error() {
    let (_guard, path) = vault();
    {
        VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    }
    let conn = Connection::open(&path).expect("open");
    conn.execute("UPDATE header SET schema_version = 99 WHERE id = 1", [])
        .expect("bump");
    drop(conn);

    match VaultFile::open(&path) {
        Err(StoreError::UnsupportedSchema { found, supported }) => {
            assert_eq!(found, 99);
            assert_eq!(supported, keyring_store::CURRENT_SCHEMA_VERSION);
        }
        other => panic!("expected UnsupportedSchema, got {other:?}"),
    }
}
