// SPDX-License-Identifier: AGPL-3.0-or-later
//! The meta/secret split (SPEC-V1 §3.4).
//!
//! AC02 proves no secret reaches *disk*. This proves the stronger property the
//! split exists for: no secret reaches the **metadata envelope**, which is the
//! one decrypted for every item at unlock. A secret in the wrong half would
//! still be encrypted at rest and would still pass AC02 — and would still be
//! materialised in memory for the whole vault the moment it is opened.

use keyring_store::{
    CustomField, CustomFieldKind, ItemBody, ItemDraft, ItemMetaPayload, KdfParams, SecretField,
    TotpAlgorithm, TotpConfig, VaultFile,
};

const MASTER: &str = "split-test-master-8Kq2Xv";

/// Values that must never appear in a metadata payload.
mod secret {
    pub const PASSWORD: &str = "SPLITSECRETpasswordZZ";
    pub const TOTP: &str = "SPLITSECRETtotpZZ";
    pub const CARD: &str = "4111111111111111";
    pub const CVV: &str = "SPLITSECRETcvvZZ";
    pub const PIN: &str = "SPLITSECRETpinZZ";
    pub const DOC: &str = "SPLITSECRETdocZZ";
    pub const HIDDEN: &str = "SPLITSECREThiddenZZ";
}

fn all_secrets() -> Vec<&'static str> {
    vec![
        secret::PASSWORD,
        secret::TOTP,
        secret::CARD,
        secret::CVV,
        secret::PIN,
        secret::DOC,
        secret::HIDDEN,
    ]
}

/// Render a metadata payload and assert no secret is anywhere in it.
fn assert_no_secret_in_meta(meta: &ItemMetaPayload) {
    let rendered = format!("{meta:?}");
    for value in all_secrets() {
        assert!(
            !rendered.contains(value),
            "a secret reached the metadata envelope: {value}\n  in: {rendered}"
        );
    }
    // Postcard-encoded too, since that is what is actually sealed.
    let encoded = postcard::to_stdvec(meta).expect("encode");
    for value in all_secrets() {
        assert!(
            find(&encoded, value.as_bytes()).is_none(),
            "a secret reached the encoded metadata payload: {value}"
        );
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn seeded() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    (dir, path)
}

#[test]
fn no_secret_field_reaches_the_metadata_envelope() {
    let (_guard, path) = seeded();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let mut login = ItemDraft::new(
        vault_id,
        "a login",
        ItemBody::Login {
            username: "visible-username".to_owned(),
            password: secret::PASSWORD.to_owned(),
            urls: vec!["https://example.invalid".to_owned()],
            totp: Some(TotpConfig {
                secret: secret::TOTP.to_owned(),
                algorithm: TotpAlgorithm::Sha1,
                digits: 6,
                period_seconds: 30,
                issuer: "visible-issuer".to_owned(),
                account: "visible-account".to_owned(),
            }),
        },
    );
    login.custom_fields = vec![
        CustomField {
            label: "visible label".to_owned(),
            value: "visible value".to_owned(),
            kind: CustomFieldKind::Text,
        },
        CustomField {
            label: "hidden label".to_owned(),
            value: secret::HIDDEN.to_owned(),
            kind: CustomFieldKind::Hidden,
        },
    ];

    let card = ItemDraft::new(
        vault_id,
        "a card",
        ItemBody::Card {
            cardholder: "visible holder".to_owned(),
            number: secret::CARD.to_owned(),
            expiry_month: 7,
            expiry_year: 2031,
            cvv: secret::CVV.to_owned(),
            pin: secret::PIN.to_owned(),
            billing_address: "visible address".to_owned(),
        },
    );

    let identity = ItemDraft::new(
        vault_id,
        "an identity",
        ItemBody::Identity {
            first_name: "Visible".to_owned(),
            last_name: "Person".to_owned(),
            dob: "1990-01-01".to_owned(),
            document_type: "passport".to_owned(),
            document_number: secret::DOC.to_owned(),
            issuing_country: "ZZ".to_owned(),
            expiry: "2035-01-01".to_owned(),
            address: "visible address".to_owned(),
            phone: "+10000000000".to_owned(),
            email: "visible@example.invalid".to_owned(),
        },
    );

    for draft in [&login, &card, &identity] {
        let id = session.item_upsert(draft).expect("upsert");
        let meta = session.item_meta(id).expect("meta");

        // Round-trip the public metadata shape through the payload type.
        let payload = ItemMetaPayload {
            kind: meta.kind,
            title: meta.title.clone(),
            notes: meta.notes.clone(),
            tags: meta.tags.clone(),
            favorite: meta.favorite,
            created_at: meta.created_at,
            custom_fields: meta.custom_fields.clone(),
            body: meta.body.clone(),
            // The icon is bytes the user picked, never a secret — but it goes through
            // the same assertion as everything else in this envelope.
            custom_icon: session.item_custom_icon(id).expect("icon"),
        };
        assert_no_secret_in_meta(&payload);
    }
}

#[test]
fn the_item_summary_never_carries_a_secret() {
    let (_guard, path) = seeded();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    session
        .item_upsert(&ItemDraft::new(
            vault_id,
            "a card",
            ItemBody::Card {
                cardholder: "visible holder".to_owned(),
                number: secret::CARD.to_owned(),
                expiry_month: 1,
                expiry_year: 2030,
                cvv: secret::CVV.to_owned(),
                pin: secret::PIN.to_owned(),
                billing_address: String::new(),
            },
        ))
        .expect("upsert");

    let rendered = format!("{:?}", session.items_list().expect("list"));
    for value in all_secrets() {
        assert!(!rendered.contains(value), "items_list leaked {value}");
    }
}

#[test]
fn a_card_summary_shows_last_four_and_nothing_more() {
    // A deliberate, bounded disclosure: enough to identify the card in a list,
    // not enough to transact with. Asserted so a future change that widened it
    // would fail here rather than pass quietly.
    let (_guard, path) = seeded();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(
            vault_id,
            "a card",
            ItemBody::Card {
                cardholder: "Holder".to_owned(),
                number: secret::CARD.to_owned(),
                expiry_month: 1,
                expiry_year: 2030,
                cvv: secret::CVV.to_owned(),
                pin: secret::PIN.to_owned(),
                billing_address: String::new(),
            },
        ))
        .expect("upsert");

    let meta = session.item_meta(id).expect("meta");
    match meta.body {
        keyring_store::ItemBodyMeta::Card { last4, .. } => {
            assert_eq!(last4.as_deref(), Some("1111"));
            // The full number is 16 digits; only 4 are in metadata.
            assert!(!format!("{last4:?}").contains(secret::CARD));
        }
        other => panic!("expected a card body, got {other:?}"),
    }

    // And the full number is still retrievable through the secret path.
    let number = session
        .item_secret(id, SecretField::CardNumber)
        .expect("reveal");
    assert_eq!(&*number, secret::CARD);
}

#[test]
fn a_short_card_number_gets_no_last_four_at_all() {
    // Four digits of a four-digit "number" would be the whole value.
    let (_guard, path) = seeded();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(
            vault_id,
            "a short card",
            ItemBody::Card {
                cardholder: "Holder".to_owned(),
                number: "1234".to_owned(),
                expiry_month: 1,
                expiry_year: 2030,
                cvv: "999".to_owned(),
                pin: "0000".to_owned(),
                billing_address: String::new(),
            },
        ))
        .expect("upsert");

    match session.item_meta(id).expect("meta").body {
        keyring_store::ItemBodyMeta::Card { last4, .. } => assert_eq!(last4, None),
        other => panic!("expected a card body, got {other:?}"),
    }
}

#[test]
fn a_hidden_custom_field_keeps_its_label_but_loses_its_value() {
    let (_guard, path) = seeded();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let mut draft = ItemDraft::new(vault_id, "a note", ItemBody::SecureNote);
    draft.custom_fields = vec![CustomField {
        label: "recovery code".to_owned(),
        value: secret::HIDDEN.to_owned(),
        kind: CustomFieldKind::Hidden,
    }];
    let id = session.item_upsert(&draft).expect("upsert");

    let meta = session.item_meta(id).expect("meta");
    let field = &meta.custom_fields[0];
    assert_eq!(field.label, "recovery code", "the label stays visible");
    assert_eq!(field.value, "", "the value does not");
    assert_eq!(field.kind, CustomFieldKind::Hidden);

    let revealed = session
        .item_secret(id, SecretField::Custom(0))
        .expect("reveal");
    assert_eq!(&*revealed, secret::HIDDEN);
}

#[test]
fn asking_for_a_field_an_item_does_not_have_is_an_error_not_an_empty_string() {
    let (_guard, path) = seeded();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(vault_id, "a note", ItemBody::SecureNote))
        .expect("upsert");

    assert!(session.item_secret(id, SecretField::Password).is_err());
    assert!(session.item_secret(id, SecretField::CardNumber).is_err());
    assert!(session.item_secret(id, SecretField::Custom(7)).is_err());
}
