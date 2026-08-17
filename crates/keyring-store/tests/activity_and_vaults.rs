//! Activity records and vault mutations (SPEC-V1 §4.2, §4.3, §7.5).
//!
//! The property worth protecting here is the one §4.3 exists for: an activity
//! write touches the `activity` table and **nothing else**. AC10 asserts it at
//! the acceptance level for a reveal; these assert it for every event kind, and
//! for the paths AC10 does not reach — copy, edit, favourite.
//!
//! The vault tests exist because `vault_delete` is the one operation in the
//! store that rewrites another row's key material. Moving an item re-wraps its
//! item key under a different vault key, and a re-wrap that silently produced an
//! unopenable envelope would not surface until the user next opened the item.

use keyring_store::{
    ActivityKind, ItemBody, ItemDraft, KdfParams, SecretField, StoreError, VaultFile,
    ACTIVITY_LIMIT,
};
use rusqlite::Connection;
use uuid::Uuid;

const MASTER: &str = "activity-test-master-7Kd2Wq";

fn vault() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    (dir, path)
}

fn login(vault_id: Uuid, title: &str, password: &str) -> ItemDraft {
    ItemDraft::new(
        vault_id,
        title,
        ItemBody::Login {
            username: "user@example.test".to_owned(),
            password: password.to_owned(),
            urls: vec!["https://example.test".to_owned()],
            totp: None,
        },
    )
}

/// Everything about an item row that a read must not change.
fn row_state(path: &std::path::Path, id: Uuid) -> (i64, i64, Vec<u8>, Vec<u8>) {
    let conn = Connection::open(path).expect("open");
    conn.query_row(
        "SELECT revision, updated_at, meta_ct, secret_ct FROM items WHERE id = ?1",
        [id.as_bytes().as_slice()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .expect("row")
}

fn activity_row_count(path: &std::path::Path, id: Uuid) -> i64 {
    let conn = Connection::open(path).expect("open");
    conn.query_row(
        "SELECT COUNT(*) FROM activity WHERE item_id = ?1",
        [id.as_bytes().as_slice()],
        |r| r.get(0),
    )
    .expect("count")
}

// ── Activity is written, and only to its own table ──────────────────────────

#[test]
fn a_reveal_records_activity_and_leaves_the_item_row_untouched() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session
        .item_upsert(&login(vault_id, "a login", "fixture-password-Qb4"))
        .expect("upsert");

    let before = row_state(&path, id);
    let created_events = activity_row_count(&path, id);

    let value = session
        .item_reveal_field(id, SecretField::Password)
        .expect("reveal");
    assert_eq!(&*value, "fixture-password-Qb4");

    assert_eq!(
        row_state(&path, id),
        before,
        "a reveal mutated the item row — §4.3 exists precisely to stop this, and \
         AC10 asserts it at the acceptance level"
    );
    assert_eq!(
        activity_row_count(&path, id),
        created_events + 1,
        "a reveal must leave exactly one activity row behind"
    );

    let events = session.item_activity(id, 10).expect("activity");
    assert_eq!(events[0].kind, ActivityKind::Revealed);
    assert_eq!(events[0].item_id, id);
}

#[test]
fn a_copy_records_a_copied_event_and_leaves_the_item_row_untouched() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session
        .item_upsert(&login(vault_id, "a login", "fixture-password-Qb4"))
        .expect("upsert");

    let before = row_state(&path, id);
    let value = session
        .item_copy_field(id, SecretField::Password)
        .expect("copy");
    assert_eq!(&*value, "fixture-password-Qb4");

    assert_eq!(row_state(&path, id), before, "a copy mutated the item row");
    assert_eq!(
        session.item_activity(id, 10).expect("activity")[0].kind,
        ActivityKind::Copied
    );
}

#[test]
fn item_secret_stays_the_silent_read() {
    // `item_secret` is the path backup and the security report take. Neither is
    // a user looking at a password, so neither should appear in the user's
    // activity — a report over 500 items would otherwise bury every real event.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session
        .item_upsert(&login(vault_id, "a login", "fixture-password-Qb4"))
        .expect("upsert");

    let before = activity_row_count(&path, id);
    for _ in 0..5 {
        session
            .item_secret(id, SecretField::Password)
            .expect("silent read");
    }
    assert_eq!(
        activity_row_count(&path, id),
        before,
        "item_secret recorded activity; only reveal and copy should"
    );
}

#[test]
fn creating_editing_and_rotating_record_distinct_kinds() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&login(vault_id, "a login", "first-password-Qb4"))
        .expect("create");

    // A metadata-only edit.
    let mut renamed = login(vault_id, "a renamed login", "first-password-Qb4");
    renamed.id = Some(id);
    session.item_upsert(&renamed).expect("rename");

    // A rotation.
    let mut rotated = login(vault_id, "a renamed login", "second-password-Zx9");
    rotated.id = Some(id);
    session.item_upsert(&rotated).expect("rotate");

    let kinds: Vec<ActivityKind> = session
        .item_activity(id, 10)
        .expect("activity")
        .iter()
        .map(|e| e.kind)
        .collect();

    // Newest first.
    assert_eq!(
        kinds,
        vec![
            ActivityKind::PasswordChanged,
            ActivityKind::Updated,
            ActivityKind::Created
        ],
        "a rotation must be distinguishable from an edit: it is the event the \
         security report and the user both care about"
    );
}

#[test]
fn activity_is_capped_per_item() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session
        .item_upsert(&login(vault_id, "a login", "fixture-password-Qb4"))
        .expect("upsert");

    // One create event plus enough reveals to overrun the cap.
    for _ in 0..ACTIVITY_LIMIT + 20 {
        session
            .item_reveal_field(id, SecretField::Password)
            .expect("reveal");
    }

    assert_eq!(
        activity_row_count(&path, id),
        i64::try_from(ACTIVITY_LIMIT).expect("limit fits"),
        "eviction runs on write, so the table must stay bounded without anyone \
         opening the activity panel"
    );
    assert_eq!(
        session.item_activity(id, 1_000).expect("activity").len(),
        ACTIVITY_LIMIT,
        "a caller asking for more than the cap must not get more than the cap"
    );
}

#[test]
fn activity_survives_a_lock_and_reopen() {
    let (_guard, path) = vault();
    let id = {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault");
        let id = session
            .item_upsert(&login(vault_id, "a login", "fixture-password-Qb4"))
            .expect("upsert");
        session
            .item_reveal_field(id, SecretField::Password)
            .expect("reveal");
        id
    };

    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("re-unlock");
    let events = session.item_activity(id, 10).expect("activity");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, ActivityKind::Revealed);
    assert_eq!(events[1].kind, ActivityKind::Created);
}

#[test]
fn clearing_activity_removes_it() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let first = session
        .item_upsert(&login(vault_id, "first", "fixture-password-Qb4"))
        .expect("upsert");
    let second = session
        .item_upsert(&login(vault_id, "second", "fixture-password-Zx9"))
        .expect("upsert");

    assert_eq!(session.activity_clear(Some(first)).expect("clear one"), 1);
    assert!(session
        .item_activity(first, 10)
        .expect("activity")
        .is_empty());
    assert_eq!(
        session.item_activity(second, 10).expect("activity").len(),
        1
    );

    assert_eq!(session.activity_clear(None).expect("clear all"), 1);
    assert!(session
        .item_activity(second, 10)
        .expect("activity")
        .is_empty());
}

// ── Favourites ──────────────────────────────────────────────────────────────

#[test]
fn favouriting_bumps_the_revision_and_keeps_the_secret_readable() {
    // Both envelopes bind the revision in their AAD, so a favourite toggle has
    // to re-seal the secret half too. If it did not, the item's password would
    // become undecryptable the moment someone starred it.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session
        .item_upsert(&login(vault_id, "a login", "fixture-password-Qb4"))
        .expect("upsert");

    let (revision_before, _, _, _) = row_state(&path, id);
    assert!(session.item_set_favorite(id, true).expect("favourite"));

    let (revision_after, _, _, _) = row_state(&path, id);
    assert_eq!(revision_after, revision_before + 1);
    assert!(session.item_meta(id).expect("meta").favorite);
    assert_eq!(
        &*session
            .item_secret(id, SecretField::Password)
            .expect("secret after favourite"),
        "fixture-password-Qb4"
    );
}

#[test]
fn favouriting_twice_does_not_burn_a_revision() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session
        .item_upsert(&login(vault_id, "a login", "fixture-password-Qb4"))
        .expect("upsert");

    assert!(session.item_set_favorite(id, true).expect("first"));
    let (revision, updated_at, _, _) = row_state(&path, id);

    assert!(
        !session.item_set_favorite(id, true).expect("second"),
        "a no-op toggle must report that nothing changed"
    );
    let (revision_after, updated_after, _, _) = row_state(&path, id);
    assert_eq!(
        (revision, updated_at),
        (revision_after, updated_after),
        "revision is what the manifest uses to detect a rollback; churning it on \
         a no-op click makes that signal noisier for nothing"
    );
}

// ── Vault edits ─────────────────────────────────────────────────────────────

#[test]
fn renaming_and_recolouring_a_vault_keeps_everything_else() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    session
        .item_upsert(&login(vault_id, "a login", "fixture-password-Qb4"))
        .expect("upsert");

    session.vault_rename(vault_id, "Work").expect("rename");
    session
        .vault_set_color(vault_id, "vault.accent.4")
        .expect("recolour");

    let listed = session.vaults_list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Work");
    assert_eq!(listed[0].color_token, "vault.accent.4");
    assert_eq!(listed[0].item_count, 1, "an edit must not orphan the items");
}

#[test]
fn a_renamed_vault_still_opens_after_a_reopen() {
    let (_guard, path) = vault();
    let vault_id = {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault");
        session.vault_rename(vault_id, "Renamed").expect("rename");
        vault_id
    };

    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("re-unlock");
    let listed = session.vaults_list().expect("list");
    assert_eq!(listed[0].id, vault_id);
    assert_eq!(listed[0].name, "Renamed");
}

#[test]
fn deleting_a_vault_moves_its_items_and_they_still_decrypt() {
    // The re-wrap is the risky part: an item key sealed under the wrong vault
    // key produces an envelope that opens nowhere, and nothing would notice
    // until the user next opened the item.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let personal = session.vault_add("Personal", "vault.accent.1").expect("v1");
    let work = session.vault_add("Work", "vault.accent.2").expect("v2");

    let id = session
        .item_upsert(&login(work, "a login", "fixture-password-Qb4"))
        .expect("upsert");
    session
        .item_reveal_field(id, SecretField::Password)
        .expect("reveal before the move");

    session.vault_delete(work, Some(personal)).expect("delete");

    let meta = session.item_meta(id).expect("meta after move");
    assert_eq!(meta.vault_id, personal, "the item did not move");
    assert_eq!(
        &*session
            .item_secret(id, SecretField::Password)
            .expect("secret after move"),
        "fixture-password-Qb4",
        "the item key was re-wrapped under the wrong vault key"
    );
    assert_eq!(
        session.item_activity(id, 10).expect("activity").len(),
        2,
        "moving an item must not lose its history"
    );

    let listed = session.vaults_list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, personal);
    assert_eq!(listed[0].item_count, 1);
}

#[test]
fn a_vault_delete_leaves_the_file_unlockable() {
    // Both branches change the live item set, so both have to re-sign the
    // manifest. A missed reseal is not a small bug: the vault refuses to open.
    let (_guard, path) = vault();
    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let personal = session.vault_add("Personal", "vault.accent.1").expect("v1");
        let work = session.vault_add("Work", "vault.accent.2").expect("v2");
        session
            .item_upsert(&login(work, "moved", "fixture-password-Qb4"))
            .expect("upsert");
        session.vault_delete(work, Some(personal)).expect("move");

        let archive = session.vault_add("Archive", "vault.accent.3").expect("v3");
        session
            .item_upsert(&login(archive, "dropped", "fixture-password-Zx9"))
            .expect("upsert");
        session.vault_delete(archive, None).expect("drop");
    }

    let file = VaultFile::open(&path).expect("reopen");
    let session = file
        .unlock(MASTER)
        .expect("a vault delete left the manifest unsigned");
    assert_eq!(session.vaults_list().expect("list").len(), 1);
    assert_eq!(
        session.items_list().expect("items").len(),
        1,
        "the dropped vault's items should be soft-deleted, not listed"
    );
}

#[test]
fn deleting_without_a_target_soft_deletes_the_items_rather_than_purging_them() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let personal = session.vault_add("Personal", "vault.accent.1").expect("v1");
    let work = session.vault_add("Work", "vault.accent.2").expect("v2");
    let id = session
        .item_upsert(&login(work, "a login", "fixture-password-Qb4"))
        .expect("upsert");

    session.vault_delete(work, None).expect("delete");

    assert!(session.items_list().expect("items").is_empty());

    // Soft, not hard: the row is still there for the 30-day purge window, which
    // is what makes an accidental delete recoverable.
    let conn = Connection::open(&path).expect("open");
    let deleted_at: Option<i64> = conn
        .query_row(
            "SELECT deleted_at FROM items WHERE id = ?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .expect("row");
    assert!(
        deleted_at.is_some(),
        "the item was purged rather than soft-deleted"
    );
    assert_eq!(session.vaults_list().expect("list")[0].id, personal);
}

#[test]
fn the_last_vault_cannot_be_deleted() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let only = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    assert_eq!(
        session.vault_delete(only, None).unwrap_err(),
        StoreError::LastVault,
        "an account with no vault has nowhere to put an item and no way back"
    );
    assert_eq!(session.vaults_list().expect("list").len(), 1);
}

#[test]
fn deleting_a_vault_into_itself_is_refused() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    session.vault_add("Personal", "vault.accent.1").expect("v1");
    let work = session.vault_add("Work", "vault.accent.2").expect("v2");

    assert_eq!(
        session.vault_delete(work, Some(work)).unwrap_err(),
        StoreError::VaultNotFound
    );
    assert_eq!(session.vaults_list().expect("list").len(), 2);
}

#[test]
fn activity_for_a_missing_item_is_not_found() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    assert_eq!(
        session.item_activity(Uuid::new_v4(), 10).unwrap_err(),
        StoreError::ItemNotFound
    );
}
