//! The item and vault domain model (SPEC-V1 §4.1, §4.2).
//!
//! The split that matters is [`ItemMetaPayload`] vs [`ItemSecretPayload`]: they
//! are separate structs because they are separate ciphertexts under separate
//! keys, and keeping them separate in *types* is what stops a secret drifting
//! into the metadata envelope during a refactor. A field in the wrong struct is
//! a field decrypted for every item at unlock.
//!
//! Ids are UUID v4, never v7: `id` is a plaintext primary key, and v7 embeds a
//! millisecond timestamp that would leak the creation time §4.4 deliberately
//! encrypts.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// The four item types (SPEC-V1 §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    /// Username, password, URLs, optional TOTP.
    Login,
    /// Body lives in `notes`.
    SecureNote,
    /// Payment card.
    Card,
    /// Identity document.
    Identity,
}

/// A secret field, addressed by a typed enum rather than a string.
///
/// SPEC-V1 §4.1 is explicit about this: a bare string parameter invites a
/// field-traversal bug where a crafted value reaches something it shouldn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretField {
    /// `login.password`
    Password,
    /// `login.totp.secret`
    TotpSecret,
    /// `card.number`
    CardNumber,
    /// `card.cvv`
    CardCvv,
    /// `card.pin`
    CardPin,
    /// `identity.document_number`
    DocumentNumber,
    /// A custom field with `kind == hidden`, by index.
    Custom(u16),
}

/// What a custom field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomFieldKind {
    /// Plain text; lives in the metadata envelope.
    Text,
    /// Secret; lives in the secret envelope.
    Hidden,
    /// A URL; metadata.
    Url,
    /// A date; metadata.
    Date,
}

/// A user-defined field on an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomField {
    /// Label shown in the UI.
    pub label: String,
    /// The value. Empty in the metadata envelope when `kind == Hidden`.
    pub value: String,
    /// Whether this field is a secret.
    pub kind: CustomFieldKind,
}

/// TOTP configuration (SPEC-V1 §4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotpConfig {
    /// Base32 shared secret. A secret field: never in the metadata envelope.
    pub secret: String,
    /// Hash algorithm.
    pub algorithm: TotpAlgorithm,
    /// 6 or 8.
    pub digits: u8,
    /// Step length in seconds.
    pub period_seconds: u32,
    /// Issuer label.
    pub issuer: String,
    /// Account label.
    pub account: String,
}

/// TOTP hash algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TotpAlgorithm {
    /// The default, and what most services use.
    Sha1,
    /// SHA-256.
    Sha256,
    /// SHA-512.
    Sha512,
}

/// One historical password (SPEC-V1 §4.1: last 5 retained).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub struct PasswordHistoryEntry {
    /// The previous password.
    pub value: String,
    /// When it was replaced, Unix milliseconds.
    #[zeroize(skip)]
    pub changed_at: i64,
}

/// Number of past passwords retained per item.
pub const PASSWORD_HISTORY_LIMIT: usize = 5;

/// The body of an item as supplied by a caller: metadata and secrets together.
///
/// This is the only place the two halves coexist, and only in memory, on the way
/// in. [`crate::repository`] splits it immediately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemBody {
    /// A login.
    Login {
        /// Username. Metadata.
        username: String,
        /// Password. Secret.
        password: String,
        /// Associated URLs. Metadata.
        urls: Vec<String>,
        /// Optional TOTP configuration; its secret is a secret field.
        totp: Option<TotpConfig>,
    },
    /// A secure note. The body is the item's `notes`.
    SecureNote,
    /// A payment card.
    Card {
        /// Cardholder name. Metadata.
        cardholder: String,
        /// Card number. Secret.
        number: String,
        /// Expiry month, 1–12. Metadata.
        expiry_month: u8,
        /// Expiry year. Metadata.
        expiry_year: u16,
        /// Security code. Secret.
        cvv: String,
        /// Card PIN. Secret.
        pin: String,
        /// Billing address. Metadata.
        billing_address: String,
    },
    /// An identity document.
    Identity {
        /// Given name. Metadata.
        first_name: String,
        /// Family name. Metadata.
        last_name: String,
        /// Date of birth. Metadata.
        dob: String,
        /// Document type. Metadata.
        document_type: String,
        /// Document number. Secret.
        document_number: String,
        /// Issuing country. Metadata.
        issuing_country: String,
        /// Expiry date. Metadata.
        expiry: String,
        /// Address. Metadata.
        address: String,
        /// Phone. Metadata.
        phone: String,
        /// Email. Metadata.
        email: String,
    },
}

impl ItemBody {
    /// Which of the four types this is.
    #[must_use]
    pub fn kind(&self) -> ItemKind {
        match self {
            Self::Login { .. } => ItemKind::Login,
            Self::SecureNote => ItemKind::SecureNote,
            Self::Card { .. } => ItemKind::Card,
            Self::Identity { .. } => ItemKind::Identity,
        }
    }
}

/// An item as supplied to [`crate::Session::item_upsert`].
///
/// Carries plaintext secrets, so it is short-lived by construction: the caller
/// builds it, hands it over, and the repository splits and seals it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDraft {
    /// `None` creates a new item; `Some` updates and bumps its revision.
    pub id: Option<Uuid>,
    /// Which vault it belongs to.
    pub vault_id: Uuid,
    /// Title. Metadata — but still encrypted; §4.4 keeps it out of plaintext.
    pub title: String,
    /// Free-form notes. Metadata.
    pub notes: String,
    /// Tags. Metadata.
    pub tags: Vec<String>,
    /// Favourite flag. Metadata.
    pub favorite: bool,
    /// Custom fields; `Hidden` ones are routed to the secret envelope.
    pub custom_fields: Vec<CustomField>,
    /// Type-specific fields.
    pub body: ItemBody,
}

impl ItemDraft {
    /// A draft for a new item with no notes, tags or custom fields.
    #[must_use]
    pub fn new(vault_id: Uuid, title: &str, body: ItemBody) -> Self {
        Self {
            id: None,
            vault_id,
            title: title.to_owned(),
            notes: String::new(),
            tags: Vec::new(),
            favorite: false,
            custom_fields: Vec::new(),
            body,
        }
    }
}

// ── What actually goes into each envelope ───────────────────────────────────

/// The non-secret half of an item: everything decrypted at unlock.
///
/// Adding a secret-bearing field to this struct would decrypt it for every item
/// at unlock and place it in the in-memory index. `tests/split.rs` asserts no
/// secret sentinel survives a round trip through this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemMetaPayload {
    /// Item type.
    pub kind: ItemKind,
    /// Title.
    pub title: String,
    /// Notes.
    pub notes: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Favourite flag.
    pub favorite: bool,
    /// Creation time, Unix milliseconds. Encrypted, which is why ids are v4.
    pub created_at: i64,
    /// Non-secret custom fields, with `Hidden` values blanked.
    pub custom_fields: Vec<CustomField>,
    /// Type-specific non-secret fields.
    pub body: ItemBodyMeta,
}

/// The non-secret projection of an [`ItemBody`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemBodyMeta {
    /// A login, without its password or TOTP secret.
    Login {
        /// Username.
        username: String,
        /// Associated URLs.
        urls: Vec<String>,
        /// Whether a TOTP config exists — never the secret itself.
        has_totp: bool,
    },
    /// A secure note.
    SecureNote,
    /// A card, without its number, CVV or PIN.
    Card {
        /// Cardholder name.
        cardholder: String,
        /// Expiry month.
        expiry_month: u8,
        /// Expiry year.
        expiry_year: u16,
        /// Billing address.
        billing_address: String,
        /// Last four digits, when the number is long enough to have them.
        ///
        /// A deliberate, bounded disclosure: it is what makes a card list
        /// usable, and four digits of a PAN is the industry norm for display.
        /// It is *not* enough to transact with. If you would rather it were not
        /// in the metadata envelope, that is a spec change, not a code change.
        last4: Option<String>,
    },
    /// An identity, without its document number.
    Identity {
        /// Given name.
        first_name: String,
        /// Family name.
        last_name: String,
        /// Date of birth.
        dob: String,
        /// Document type.
        document_type: String,
        /// Issuing country.
        issuing_country: String,
        /// Expiry date.
        expiry: String,
        /// Address.
        address: String,
        /// Phone.
        phone: String,
        /// Email.
        email: String,
    },
}

impl ItemBodyMeta {
    /// Which of the four types this is.
    #[must_use]
    pub fn kind(&self) -> ItemKind {
        match self {
            Self::Login { .. } => ItemKind::Login,
            Self::SecureNote => ItemKind::SecureNote,
            Self::Card { .. } => ItemKind::Card,
            Self::Identity { .. } => ItemKind::Identity,
        }
    }
}

/// The secret half of an item: decrypted one field at a time, on demand.
///
/// `ZeroizeOnDrop`, and every field is a `String` that is wiped rather than
/// freed. Deserializing this allocates the plaintext, so callers must keep it in
/// scope for as short a time as possible.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct ItemSecretPayload {
    /// `login.password`.
    pub password: Option<String>,
    /// `login.totp.secret`.
    pub totp_secret: Option<String>,
    /// `card.number`.
    pub card_number: Option<String>,
    /// `card.cvv`.
    pub card_cvv: Option<String>,
    /// `card.pin`.
    pub card_pin: Option<String>,
    /// `identity.document_number`.
    pub document_number: Option<String>,
    /// Values of the `Hidden` custom fields, in the order they appear in
    /// [`ItemMetaPayload::custom_fields`].
    pub hidden_custom: Vec<String>,
    /// Previous passwords, newest first, capped at [`PASSWORD_HISTORY_LIMIT`].
    pub password_history: Vec<PasswordHistoryEntry>,
}

impl ItemSecretPayload {
    /// Read one field by its typed address.
    #[must_use]
    pub fn field(&self, which: SecretField) -> Option<&str> {
        match which {
            SecretField::Password => self.password.as_deref(),
            SecretField::TotpSecret => self.totp_secret.as_deref(),
            SecretField::CardNumber => self.card_number.as_deref(),
            SecretField::CardCvv => self.card_cvv.as_deref(),
            SecretField::CardPin => self.card_pin.as_deref(),
            SecretField::DocumentNumber => self.document_number.as_deref(),
            SecretField::Custom(index) => {
                self.hidden_custom.get(index as usize).map(String::as_str)
            }
        }
    }
}

/// A vault's own metadata, encrypted under the vault key so it can travel with a
/// V2 share (SPEC-V1 §4.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultMetaPayload {
    /// Display name.
    pub name: String,
    /// A token *name* such as `vault.accent.3`, never a hex value.
    pub color_token: String,
    /// Personal or user-created.
    pub kind: VaultKind,
    /// Creation time, Unix milliseconds.
    pub created_at: i64,
}

/// Whether a vault is the built-in personal one or user-created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultKind {
    /// The default vault created with the account.
    Personal,
    /// A user-created vault.
    Custom,
}

/// A vault as listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultSummary {
    /// Vault id.
    pub id: Uuid,
    /// Display name.
    pub name: String,
    /// Colour token name.
    pub color_token: String,
    /// Personal or custom.
    pub kind: VaultKind,
    /// Live item count, excluding soft-deleted rows (SPEC-V1 §4.2).
    pub item_count: usize,
}

/// An item as listed. Contains no secret field, ever (SPEC-V1 §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSummary {
    /// Item id.
    pub id: Uuid,
    /// Owning vault.
    pub vault_id: Uuid,
    /// Item type.
    pub kind: ItemKind,
    /// Title.
    pub title: String,
    /// Type-appropriate subtitle: username, cardholder, or name.
    pub subtitle: Option<String>,
    /// Whether a TOTP config exists.
    pub has_totp: bool,
    /// Favourite flag.
    pub is_favorite: bool,
    /// Current revision.
    pub revision: u64,
    /// Last modification, Unix milliseconds.
    pub updated_at: i64,
}

/// An item's decrypted metadata, as returned by [`crate::Session::item_meta`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemMeta {
    /// Item id.
    pub id: Uuid,
    /// Owning vault.
    pub vault_id: Uuid,
    /// Item type.
    pub kind: ItemKind,
    /// Title.
    pub title: String,
    /// Notes.
    pub notes: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Favourite flag.
    pub favorite: bool,
    /// Current revision.
    pub revision: u64,
    /// Creation time, Unix milliseconds.
    pub created_at: i64,
    /// Non-secret custom fields.
    pub custom_fields: Vec<CustomField>,
    /// Type-specific non-secret fields.
    pub body: ItemBodyMeta,
}

/// One row of the in-memory search index (SPEC-V1 §4.7).
///
/// Contains **only** non-secret fields. That is the whole contract: the index is
/// built by decrypting every item's `meta_ct` once at unlock, and if a secret
/// were ever added here it would be decrypted for every item and held for the
/// life of the session.
///
/// The strings are wiped on drop. They are not secrets, but a list of every
/// site a user has an account with is exactly the inventory the vault exists to
/// protect, and leaving it in freed memory after lock would undo §4.7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    /// Item id.
    pub id: Uuid,
    /// Owning vault.
    pub vault_id: Uuid,
    /// Item type.
    pub kind: ItemKind,
    /// Title.
    pub title: String,
    /// Username, for a login.
    pub username: Option<String>,
    /// Associated URLs.
    pub urls: Vec<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Favourite flag.
    pub favorite: bool,
    /// Whether a TOTP config exists.
    pub has_totp: bool,
    /// Current revision.
    pub revision: u64,
    /// Creation time, Unix milliseconds.
    pub created_at: i64,
    /// Last modification, Unix milliseconds.
    pub updated_at: i64,
    /// Type-appropriate subtitle for rendering a row.
    pub subtitle: Option<String>,
}

impl Drop for IndexRow {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        self.title.zeroize();
        if let Some(username) = self.username.as_mut() {
            username.zeroize();
        }
        for url in &mut self.urls {
            url.zeroize();
        }
        for tag in &mut self.tags {
            tag.zeroize();
        }
        if let Some(subtitle) = self.subtitle.as_mut() {
            subtitle.zeroize();
        }
    }
}
