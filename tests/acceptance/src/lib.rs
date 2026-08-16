//! Shared fixtures for the SPEC-V1 §11 acceptance tests.
//!
//! FROZEN. See `API.md`. Nothing here may be relaxed to make a run pass.

use store::{ItemBody, ItemDraft, KdfParams};
use uuid::Uuid;

/// Master password used by every acceptance fixture. Generated, never a real credential.
pub const MASTER: &str = "acceptance-fixture-master-4mLq7XvR2wZp";

/// Distinctive byte sequences seeded into item content so AC02 can assert none of
/// them ever reach the database file. Chosen so a false positive is impossible:
/// they occur nowhere else in the repository.
pub mod sentinel {
    pub const TITLE: &str = "SENTINEL7Q4XTITLEmarker";
    pub const USERNAME: &str = "SENTINEL7Q4XUSERmarker";
    pub const PASSWORD: &str = "SENTINEL7Q4XPASSmarker";
    pub const NOTES: &str = "SENTINEL7Q4XNOTESmarker";
    pub const URL: &str = "https://SENTINEL7Q4XURLmarker.example";
    pub const TAG: &str = "SENTINEL7Q4XTAGmarker";
    pub const CARD_NUMBER: &str = "SENTINEL7Q4XCARDmarker";
    pub const CVV: &str = "SENTINEL7Q4XCVVmarker";
    pub const PIN: &str = "SENTINEL7Q4XPINmarker";
    pub const DOCUMENT: &str = "SENTINEL7Q4XDOCmarker";
    pub const CARDHOLDER: &str = "SENTINEL7Q4XHOLDERmarker";
    pub const FIRST_NAME: &str = "SENTINEL7Q4XFIRSTmarker";

    /// Every sentinel, for the on-disk scan.
    #[must_use]
    pub fn all() -> Vec<&'static str> {
        vec![
            TITLE,
            USERNAME,
            PASSWORD,
            NOTES,
            URL,
            TAG,
            CARD_NUMBER,
            CVV,
            PIN,
            DOCUMENT,
            CARDHOLDER,
            FIRST_NAME,
        ]
    }
}

/// KDF cost used by acceptance fixtures: the spec floor, so tests stay fast.
/// Production calibrates; the floor is still a real, spec-legal parameter set.
#[must_use]
pub fn fixture_params() -> KdfParams {
    KdfParams::floor()
}

/// One draft of each of the four item types, all carrying sentinel content.
#[must_use]
pub fn four_item_drafts(vault_id: Uuid) -> Vec<ItemDraft> {
    let mut login = ItemDraft::new(
        vault_id,
        &format!("{} login", sentinel::TITLE),
        ItemBody::Login {
            username: sentinel::USERNAME.to_owned(),
            password: sentinel::PASSWORD.to_owned(),
            urls: vec![sentinel::URL.to_owned()],
            totp: None,
        },
    );
    login.notes = sentinel::NOTES.to_owned();
    login.tags = vec![sentinel::TAG.to_owned()];
    login.favorite = true;

    let mut note = ItemDraft::new(
        vault_id,
        &format!("{} note", sentinel::TITLE),
        ItemBody::SecureNote,
    );
    note.notes = sentinel::NOTES.to_owned();

    let card = ItemDraft::new(
        vault_id,
        &format!("{} card", sentinel::TITLE),
        ItemBody::Card {
            cardholder: sentinel::CARDHOLDER.to_owned(),
            number: sentinel::CARD_NUMBER.to_owned(),
            expiry_month: 7,
            expiry_year: 2031,
            cvv: sentinel::CVV.to_owned(),
            pin: sentinel::PIN.to_owned(),
            billing_address: "1 Generated Street".to_owned(),
        },
    );

    let identity = ItemDraft::new(
        vault_id,
        &format!("{} identity", sentinel::TITLE),
        ItemBody::Identity {
            first_name: sentinel::FIRST_NAME.to_owned(),
            last_name: "Fixture".to_owned(),
            dob: "1990-01-01".to_owned(),
            document_type: "passport".to_owned(),
            document_number: sentinel::DOCUMENT.to_owned(),
            issuing_country: "ZZ".to_owned(),
            expiry: "2035-01-01".to_owned(),
            address: "1 Generated Street".to_owned(),
            phone: "+10000000000".to_owned(),
            email: "fixture@example.invalid".to_owned(),
        },
    );

    vec![login, note, card, identity]
}

/// Byte-scan a file for a sentinel. Returns the offset of the first hit.
#[must_use]
pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
