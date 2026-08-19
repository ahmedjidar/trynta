// SPDX-License-Identifier: AGPL-3.0-or-later
//! No secret-bearing store type may print a secret (CLAUDE.md §4.6, §8).
//!
//! `keyring-crypto` has had this test since run 1, but it can only see its own
//! types. The store's domain model holds the actual passwords, card numbers and
//! TOTP seeds, and every one of those types derived `Debug` — so `{:?}` on an
//! `ItemDraft` printed the password in the clear, and `ItemSecretPayload` printed
//! all of them.
//!
//! That is not a theoretical leak. `Debug` output is what ends up in a `dbg!`
//! left in a branch, a `tracing` field, an `expect` message, or a panic payload;
//! §4.6 requires a manual redacting impl precisely because the derive is the
//! default and the default is wrong here.
//!
//! Run in both profiles. Release is where formatting can inline differently and
//! where a `Drop` can be elided, so a redaction that holds in debug is not
//! evidence about what ships:
//!
//! ```text
//! cargo test -p keyring-store --test redaction
//! cargo test -p keyring-store --test redaction --release
//! ```

use keyring_store::{
    CustomField, CustomFieldKind, ItemBody, ItemDraft, ItemSecretPayload, PasswordHistoryEntry,
    TotpAlgorithm, TotpConfig, TotpParams,
};
use uuid::Uuid;

/// Values that must never appear in any rendering. Distinctive enough that a
/// substring match cannot be a coincidence.
mod sentinel {
    pub const PASSWORD: &str = "SENTINEL-PASSWORD-7Kq2Vx";
    pub const TOTP_SEED: &str = "SENTINELSEED234567AAAAAA";
    pub const CARD_NUMBER: &str = "4111111111117834";
    pub const CVV: &str = "SENTINEL-CVV-991";
    pub const PIN: &str = "SENTINEL-PIN-4417";
    pub const DOCUMENT: &str = "SENTINEL-DOC-X99242";
    pub const HIDDEN_CUSTOM: &str = "SENTINEL-HIDDEN-Zm81";
    pub const OLD_PASSWORD: &str = "SENTINEL-HISTORY-Qp37";
}

const ALL_SENTINELS: &[&str] = &[
    sentinel::PASSWORD,
    sentinel::TOTP_SEED,
    sentinel::CARD_NUMBER,
    sentinel::CVV,
    sentinel::PIN,
    sentinel::DOCUMENT,
    sentinel::HIDDEN_CUSTOM,
    sentinel::OLD_PASSWORD,
];

/// Assert a rendering leaks nothing and says it redacted something.
///
/// Both halves matter. "Contains no secret" alone is satisfied by an empty
/// string, and a formatter that silently prints nothing is how a redaction gets
/// removed later without anyone noticing.
fn assert_redacted(what: &str, rendered: &str) {
    for secret in ALL_SENTINELS {
        assert!(
            !rendered.contains(secret),
            "{what} leaked a secret: {rendered}"
        );
    }
    assert!(
        rendered.contains("redacted"),
        "{what} rendered as {rendered:?} without saying anything was redacted — a \
         formatter that prints nothing looks identical to one that was never written"
    );
}

/// Every rendering a formatter offers, since they are separate code paths.
fn renderings<T: std::fmt::Debug>(value: &T) -> Vec<(String, String)> {
    vec![
        ("Debug".to_owned(), format!("{value:?}")),
        ("alternate Debug".to_owned(), format!("{value:#?}")),
    ]
}

fn totp() -> TotpConfig {
    TotpConfig {
        secret: sentinel::TOTP_SEED.to_owned(),
        algorithm: TotpAlgorithm::Sha256,
        digits: 8,
        period_seconds: 60,
        issuer: "Example".to_owned(),
        account: "alice@example.test".to_owned(),
    }
}

fn login_body() -> ItemBody {
    ItemBody::Login {
        username: "alice@example.test".to_owned(),
        password: sentinel::PASSWORD.to_owned(),
        urls: vec!["https://example.test".to_owned()],
        totp: Some(totp()),
    }
}

fn card_body() -> ItemBody {
    ItemBody::Card {
        cardholder: "A Alice".to_owned(),
        number: sentinel::CARD_NUMBER.to_owned(),
        expiry_month: 7,
        expiry_year: 2031,
        cvv: sentinel::CVV.to_owned(),
        pin: sentinel::PIN.to_owned(),
        billing_address: "1 Example Street".to_owned(),
    }
}

fn identity_body() -> ItemBody {
    ItemBody::Identity {
        first_name: "A".to_owned(),
        last_name: "Alice".to_owned(),
        dob: "1990-01-01".to_owned(),
        document_type: "passport".to_owned(),
        document_number: sentinel::DOCUMENT.to_owned(),
        issuing_country: "GB".to_owned(),
        expiry: "2031-01-01".to_owned(),
        address: "1 Example Street".to_owned(),
        phone: "+44 20 7946 0000".to_owned(),
        email: "alice@example.test".to_owned(),
    }
}

fn secret_payload() -> ItemSecretPayload {
    ItemSecretPayload {
        password: Some(sentinel::PASSWORD.to_owned()),
        totp_secret: Some(sentinel::TOTP_SEED.to_owned()),
        totp_params: Some(TotpParams::from_config(&totp())),
        card_number: Some(sentinel::CARD_NUMBER.to_owned()),
        card_cvv: Some(sentinel::CVV.to_owned()),
        card_pin: Some(sentinel::PIN.to_owned()),
        document_number: Some(sentinel::DOCUMENT.to_owned()),
        hidden_custom: vec![sentinel::HIDDEN_CUSTOM.to_owned()],
        password_history: vec![PasswordHistoryEntry {
            value: sentinel::OLD_PASSWORD.to_owned(),
            changed_at: 1_700_000_000_000,
        }],
    }
}

// ── The types that hold secrets ─────────────────────────────────────────────

#[test]
fn totp_config_debug_is_redacted() {
    for (what, rendered) in renderings(&totp()) {
        assert_redacted(&format!("TotpConfig {what}"), &rendered);
    }
    // The non-secret half stays visible: a redaction that hides the algorithm
    // makes the type useless for the debugging Debug exists for, and the
    // algorithm is a public protocol parameter.
    let rendered = format!("{:?}", totp());
    assert!(rendered.contains("Sha256"), "{rendered}");
    assert!(rendered.contains('8'), "{rendered}");
}

#[test]
fn password_history_debug_is_redacted() {
    let entry = PasswordHistoryEntry {
        value: sentinel::OLD_PASSWORD.to_owned(),
        changed_at: 1_700_000_000_000,
    };
    for (what, rendered) in renderings(&entry) {
        assert_redacted(&format!("PasswordHistoryEntry {what}"), &rendered);
    }
}

#[test]
fn every_item_body_variant_is_redacted() {
    for (name, body) in [
        ("Login", login_body()),
        ("Card", card_body()),
        ("Identity", identity_body()),
    ] {
        for (what, rendered) in renderings(&body) {
            assert_redacted(&format!("ItemBody::{name} {what}"), &rendered);
        }
    }
}

#[test]
fn a_secure_note_body_has_nothing_to_redact() {
    // The one variant that holds no secret. Its Debug says so rather than
    // claiming a redaction it did not perform.
    let rendered = format!("{:?}", ItemBody::SecureNote);
    assert!(rendered.contains("SecureNote"), "{rendered}");
    for secret in ALL_SENTINELS {
        assert!(!rendered.contains(secret));
    }
}

#[test]
fn item_secret_payload_debug_is_redacted() {
    for (what, rendered) in renderings(&secret_payload()) {
        assert_redacted(&format!("ItemSecretPayload {what}"), &rendered);
    }
}

#[test]
fn item_secret_payload_debug_reports_presence_not_values() {
    // Which secrets an item *has* already crosses IPC as `SecretPresence`, so
    // showing presence is not a disclosure — and it is the only thing about this
    // type that is useful in a log.
    let rendered = format!("{:?}", secret_payload());
    assert!(rendered.contains("password"), "{rendered}");

    let empty = ItemSecretPayload::default();
    let rendered = format!("{empty:?}");
    assert!(
        rendered.contains("None"),
        "an absent field should read as None, not as redacted: {rendered}"
    );
}

#[test]
fn a_hidden_custom_field_is_redacted_and_a_visible_one_is_not() {
    let hidden = CustomField {
        label: "Recovery code".to_owned(),
        value: sentinel::HIDDEN_CUSTOM.to_owned(),
        kind: CustomFieldKind::Hidden,
    };
    for (what, rendered) in renderings(&hidden) {
        assert_redacted(&format!("hidden CustomField {what}"), &rendered);
    }
    assert!(
        format!("{hidden:?}").contains("Recovery code"),
        "the label is not a secret and should stay visible"
    );

    // A text, url or date value is shown to the user in the clear. Blanket
    // redaction here would make ItemMetaPayload's Debug useless for no gain.
    let visible = CustomField {
        label: "Support line".to_owned(),
        value: "+44 20 7946 0000".to_owned(),
        kind: CustomFieldKind::Text,
    };
    let rendered = format!("{visible:?}");
    assert!(
        rendered.contains("+44 20 7946 0000"),
        "a non-hidden value should not be redacted: {rendered}"
    );
}

#[test]
fn item_draft_debug_is_redacted_through_its_members() {
    // `ItemDraft` still derives Debug. It is safe because every member that holds
    // a secret redacts itself — and this test is what stops that being an
    // accident: adding a `String` secret directly to ItemDraft would break it.
    let mut draft = ItemDraft::new(Uuid::new_v4(), "Example", login_body());
    draft.custom_fields = vec![CustomField {
        label: "Recovery code".to_owned(),
        value: sentinel::HIDDEN_CUSTOM.to_owned(),
        kind: CustomFieldKind::Hidden,
    }];

    for (what, rendered) in renderings(&draft) {
        assert_redacted(&format!("ItemDraft {what}"), &rendered);
    }
}

#[test]
fn a_draft_carrying_a_card_is_redacted() {
    let draft = ItemDraft::new(Uuid::new_v4(), "A card", card_body());
    for (what, rendered) in renderings(&draft) {
        assert_redacted(&format!("ItemDraft with card {what}"), &rendered);
    }
}

#[test]
fn totp_params_alone_carry_no_secret() {
    // The split exists so the five non-secret fields can be handled without the
    // seed. If TotpParams ever gained the seed back, this fails.
    let params = TotpParams::from_config(&totp());
    let rendered = format!("{params:?}");
    for secret in ALL_SENTINELS {
        assert!(
            !rendered.contains(secret),
            "TotpParams leaked {secret}: {rendered}"
        );
    }
    assert!(rendered.contains("Sha256"), "{rendered}");
}
