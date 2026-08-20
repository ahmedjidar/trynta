// SPDX-License-Identifier: AGPL-3.0-or-later
//! Metadata-only edits (SPEC-V1 §7.1, the detail pane's edit mode).
//!
//! One property, and it is the reason this path exists rather than reusing
//! `item_upsert`: **editing a title cannot lose a password.**
//!
//! `item_upsert` rebuilds the secret envelope from the draft it is handed, so a
//! detail-pane edit routed through it would have to carry the password — which
//! means putting the plaintext in the edit form, a second plaintext path out of
//! Rust that §4.4 does not permit. `item_edit_meta` reads the sealed secret and
//! carries it across instead. These tests hold that line: they assert the secret
//! reads back byte-identical after an edit, that the revision advances so the
//! manifest still sees the write, and that a no-op edit burns nothing.

use keyring_store::{
    ItemBody, ItemDraft, KdfParams, MetaEdits, SecretField, StoreError, VaultFile,
};
use rusqlite::Connection;
use uuid::Uuid;

const MASTER: &str = "edit-meta-test-master-4Rk9Zt";
const PASSWORD: &str = "generated-fixture-Pw-8812-not-a-real-credential";

fn vault() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    (dir, path)
}

fn login(vault_id: Uuid) -> ItemDraft {
    ItemDraft::new(
        vault_id,
        "Original title",
        ItemBody::Login {
            username: "before@example.test".to_owned(),
            password: PASSWORD.to_owned(),
            urls: vec!["https://before.example.test".to_owned()],
            totp: None,
        },
    )
}

fn revision(path: &std::path::Path, id: Uuid) -> i64 {
    let conn = Connection::open(path).expect("open");
    conn.query_row(
        "SELECT revision FROM items WHERE id = ?1",
        [id.as_bytes().as_slice()],
        |r| r.get(0),
    )
    .expect("row")
}

#[test]
fn editing_metadata_leaves_the_password_intact() {
    let (_dir, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session.item_upsert(&login(vault_id)).expect("insert");

    let edits = MetaEdits {
        title: Some("Edited title".to_owned()),
        username: Some("after@example.test".to_owned()),
        urls: Some(vec!["https://after.example.test".to_owned()]),
        notes: Some("Edited notes".to_owned()),
        tags: None,
    };
    assert!(
        session.item_edit_meta(id, &edits).expect("edit"),
        "edit reported no change"
    );

    // The whole point.
    let secret = session
        .item_reveal_field(id, SecretField::Password)
        .expect("reveal");
    assert_eq!(
        secret.as_str(),
        PASSWORD,
        "a metadata edit rewrote or wiped the password"
    );

    // And the metadata actually changed.
    let meta = session.item_meta(id).expect("meta");
    assert_eq!(meta.title, "Edited title");
    assert_eq!(meta.notes, "Edited notes");
}

#[test]
fn editing_metadata_advances_the_revision() {
    let (_dir, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session.item_upsert(&login(vault_id)).expect("insert");
    let before = revision(&path, id);

    session
        .item_edit_meta(
            id,
            &MetaEdits {
                title: Some("Second title".to_owned()),
                ..MetaEdits::default()
            },
        )
        .expect("edit");

    // Both envelopes bind the revision in their AAD, so a write that did not advance it
    // would leave the secret sealed under an AAD that no longer matches — unopenable.
    assert_eq!(revision(&path, id), before + 1);

    // Still readable under the new revision, which is what proves the re-seal was correct
    // rather than merely successful.
    let secret = session
        .item_reveal_field(id, SecretField::Password)
        .expect("reveal after edit");
    assert_eq!(secret.as_str(), PASSWORD);
}

#[test]
fn an_empty_edit_writes_nothing() {
    let (_dir, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let id = session.item_upsert(&login(vault_id)).expect("insert");
    let before = revision(&path, id);

    // `revision` is what the manifest uses to detect a rollback. Churning it on a UI
    // click that changed nothing makes that signal noisier for no gain.
    assert!(!session
        .item_edit_meta(id, &MetaEdits::default())
        .expect("no-op edit"));
    assert_eq!(revision(&path, id), before);
}

#[test]
fn editing_a_missing_item_fails_closed() {
    let (_dir, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    let outcome = session.item_edit_meta(
        Uuid::new_v4(),
        &MetaEdits {
            title: Some("Nothing to edit".to_owned()),
            ..MetaEdits::default()
        },
    );
    assert!(matches!(outcome, Err(StoreError::ItemNotFound)));
}
