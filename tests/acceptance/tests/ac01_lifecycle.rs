//! SPEC-V1 §11: create vault, add all four item types, lock, restart, unlock,
//! everything intact.
//!
//! FROZEN. See `tests/acceptance/API.md`.
//!
//! "Restart" is simulated by dropping every handle to the database and opening a
//! fresh `VaultFile` from the path — the same thing a process restart does, minus
//! the process.

use keyring_acceptance::{fixture_params, four_item_drafts, sentinel, MASTER};
use store::{ItemBody, ItemKind, SecretField, VaultFile};

#[test]
fn four_item_types_survive_a_restart_intact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    // ── first session: create and populate ───────────────────────────────────
    let created_ids;
    {
        let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
        let session = file.unlock(MASTER).expect("unlock after create");

        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault_add");

        let drafts = four_item_drafts(vault_id);
        assert_eq!(drafts.len(), 4, "fixture must cover all four item types");

        created_ids = drafts
            .iter()
            .map(|d| session.item_upsert(d).expect("item_upsert"))
            .collect::<Vec<_>>();

        let listed = session.items_list().expect("items_list");
        assert_eq!(listed.len(), 4);

        // The list response must never carry a secret field. Belt and braces:
        // assert no secret sentinel appears anywhere in the rendered summaries.
        let rendered = format!("{listed:?}");
        for secret in [
            sentinel::PASSWORD,
            sentinel::CVV,
            sentinel::PIN,
            sentinel::DOCUMENT,
            sentinel::CARD_NUMBER,
        ] {
            assert!(
                !rendered.contains(secret),
                "items_list leaked a secret field: {secret}"
            );
        }
    } // every handle dropped — the "restart"

    // ── second session: reopen and verify ────────────────────────────────────
    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("unlock after restart");

    let vaults = session.vaults_list().expect("vaults_list");
    assert_eq!(vaults.len(), 1, "the vault survived the restart");

    let listed = session.items_list().expect("items_list after restart");
    assert_eq!(listed.len(), 4, "all four items survived the restart");

    let kinds: Vec<ItemKind> = listed.iter().map(|s| s.kind).collect();
    for expected in [
        ItemKind::Login,
        ItemKind::SecureNote,
        ItemKind::Card,
        ItemKind::Identity,
    ] {
        assert!(kinds.contains(&expected), "missing item kind {expected:?}");
    }

    for id in &created_ids {
        let meta = session.item_meta(*id).expect("item_meta after restart");
        assert!(meta.title.contains(sentinel::TITLE), "title survived");

        match meta.kind {
            ItemKind::Login => {
                assert_eq!(meta.notes, sentinel::NOTES);
                assert!(meta.favorite);
                assert_eq!(meta.tags, vec![sentinel::TAG.to_owned()]);
                let pw = session
                    .item_secret(*id, SecretField::Password)
                    .expect("reveal password");
                assert_eq!(&*pw, sentinel::PASSWORD, "password survived the restart");
                match &meta.body {
                    store::ItemBodyMeta::Login { username, urls, .. } => {
                        assert_eq!(username, sentinel::USERNAME);
                        assert_eq!(urls, &vec![sentinel::URL.to_owned()]);
                    }
                    other => panic!("login item has the wrong body meta: {other:?}"),
                }
            }
            ItemKind::SecureNote => {
                assert_eq!(meta.notes, sentinel::NOTES, "note body survived");
            }
            ItemKind::Card => {
                let number = session
                    .item_secret(*id, SecretField::CardNumber)
                    .expect("reveal card number");
                let cvv = session
                    .item_secret(*id, SecretField::CardCvv)
                    .expect("reveal cvv");
                let pin = session
                    .item_secret(*id, SecretField::CardPin)
                    .expect("reveal pin");
                assert_eq!(&*number, sentinel::CARD_NUMBER);
                assert_eq!(&*cvv, sentinel::CVV);
                assert_eq!(&*pin, sentinel::PIN);
            }
            ItemKind::Identity => {
                let doc = session
                    .item_secret(*id, SecretField::DocumentNumber)
                    .expect("reveal document number");
                assert_eq!(&*doc, sentinel::DOCUMENT);
            }
        }
    }
}

#[test]
fn a_deleted_item_is_soft_deleted_and_excluded_from_listings() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault_add");

    let id = session
        .item_upsert(&store::ItemDraft::new(
            vault_id,
            "to be deleted",
            ItemBody::SecureNote,
        ))
        .expect("upsert");

    assert_eq!(session.items_list().expect("list").len(), 1);
    session.item_delete(id).expect("delete");
    assert_eq!(
        session.items_list().expect("list after delete").len(),
        0,
        "soft-deleted items are excluded from listings"
    );
}
