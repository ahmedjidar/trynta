// SPDX-License-Identifier: AGPL-3.0-or-later
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

use std::fmt;

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
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomField {
    /// Label shown in the UI.
    pub label: String,
    /// The value. Empty in the metadata envelope when `kind == Hidden`.
    pub value: String,
    /// Whether this field is a secret.
    pub kind: CustomFieldKind,
}

/// TOTP configuration (SPEC-V1 §4.1).
///
/// The input shape, pinned by `tests/acceptance/API.md`. On disk it is split:
/// the seed goes to [`ItemSecretPayload::totp_secret`] and everything else to
/// [`ItemSecretPayload::totp_params`], because only one of the six fields is a
/// secret and conflating them is how the other five got dropped.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A TOTP configuration minus its seed (SPEC-V1 §4.1).
///
/// These five fields are **not** secrets: the algorithm, digit count and period
/// are public protocol parameters, and the issuer and account are labels the item
/// already shows. They live beside the seed in `secret_ct` rather than in
/// `meta_ct` for two reasons: producing a code needs the seed anyway, so one
/// decrypt is enough; and parameters stored apart from the seed they describe can
/// drift out of agreement with it.
///
/// This type exists because the first version of the store kept only
/// `TotpConfig::secret` and silently discarded the rest, so an item saved as
/// SHA-256/8-digit came back as SHA-1/6-digit and generated codes that never
/// worked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotpParams {
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

impl TotpParams {
    /// Split a full config into its secret and non-secret halves.
    #[must_use]
    pub fn from_config(config: &TotpConfig) -> Self {
        Self {
            algorithm: config.algorithm,
            digits: config.digits,
            period_seconds: config.period_seconds,
            issuer: config.issuer.clone(),
            account: config.account.clone(),
        }
    }

    /// Rejoin these parameters with a seed.
    #[must_use]
    pub fn into_config(self, secret: String) -> TotpConfig {
        TotpConfig {
            secret,
            algorithm: self.algorithm,
            digits: self.digits,
            period_seconds: self.period_seconds,
            issuer: self.issuer,
            account: self.account,
        }
    }
}

// ── Redacting Debug impls (CLAUDE.md §4.6) ──────────────────────────────────
//
// Every type below holds at least one plaintext secret, and a derived `Debug`
// prints all of them. §4.6 requires a manual redacting impl for exactly this
// reason: an error string, a `dbg!` left in a branch, or a `{:?}` in a log
// message is the one place a secret escapes without anyone deciding to send it.
// `tests/redaction.rs` asserts each of these, in debug and in release.
//
// These are field-precise rather than blanket. A `Debug` that prints nothing is
// useless for the debugging it exists for, so the non-secret fields stay
// visible and only the secrets are replaced.

/// Placeholder written in place of a secret. The redaction test greps for it.
const REDACTED: &str = "<redacted>";

impl fmt::Debug for CustomField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only a `Hidden` field's value is a secret. A text, url or date value is
        // shown to the user in the clear, and blanket-redacting it would make
        // `ItemMetaPayload`'s Debug useless for no gain.
        let value: &dyn fmt::Debug = if self.kind == CustomFieldKind::Hidden {
            &REDACTED
        } else {
            &self.value
        };
        f.debug_struct("CustomField")
            .field("label", &self.label)
            .field("value", value)
            .field("kind", &self.kind)
            .finish()
    }
}

impl fmt::Debug for TotpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TotpConfig")
            .field("secret", &REDACTED)
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("period_seconds", &self.period_seconds)
            .field("issuer", &self.issuer)
            .field("account", &self.account)
            .finish()
    }
}

impl fmt::Debug for PasswordHistoryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasswordHistoryEntry")
            .field("value", &REDACTED)
            .field("changed_at", &self.changed_at)
            .finish()
    }
}

impl fmt::Debug for ItemBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Login {
                username,
                urls,
                totp,
                password: _,
            } => f
                .debug_struct("ItemBody::Login")
                .field("username", username)
                .field("password", &REDACTED)
                .field("urls", urls)
                .field("totp", totp)
                .finish(),
            Self::SecureNote => f.write_str("ItemBody::SecureNote"),
            Self::Card {
                cardholder,
                expiry_month,
                expiry_year,
                billing_address,
                number: _,
                cvv: _,
                pin: _,
            } => f
                .debug_struct("ItemBody::Card")
                .field("cardholder", &cardholder)
                .field("number", &REDACTED)
                .field("expiry_month", expiry_month)
                .field("expiry_year", expiry_year)
                .field("cvv", &REDACTED)
                .field("pin", &REDACTED)
                .field("billing_address", billing_address)
                .finish(),
            Self::Identity {
                first_name,
                last_name,
                dob,
                document_type,
                issuing_country,
                expiry,
                address,
                phone,
                email,
                document_number: _,
            } => f
                .debug_struct("ItemBody::Identity")
                .field("first_name", first_name)
                .field("last_name", last_name)
                .field("dob", dob)
                .field("document_type", document_type)
                .field("document_number", &REDACTED)
                .field("issuing_country", issuing_country)
                .field("expiry", expiry)
                .field("address", address)
                .field("phone", phone)
                .field("email", email)
                .finish(),
        }
    }
}

/// One historical password (SPEC-V1 §4.1: last 5 retained).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
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
#[derive(Clone, PartialEq, Eq)]
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

/// The encoding of a user-supplied icon, as stored.
///
/// A closed enum rather than a MIME string: the frontend builds a `data:` URI from it
/// and a free-form string there would be a way to inject a content type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconFormat {
    /// Sanitised SVG, UTF-8.
    Svg,
    /// Lossless WebP.
    Webp,
    /// PNG, the fallback when WebP encoding is unavailable.
    Png,
}

impl IconFormat {
    /// The media type for a `data:` URI.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Webp => "image/webp",
            Self::Png => "image/png",
        }
    }
}

/// A user-supplied icon, processed and ready to render.
///
/// Stored inside `meta_ct` like any other non-secret field, so it is encrypted at rest
/// and travels with a backup. It is **not** in the search index: see
/// [`ItemMeta::has_custom_icon`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredIcon {
    /// How `bytes` is encoded.
    pub format: IconFormat,
    /// The processed image. Never the file the user picked — see `services::custom_icon`.
    pub bytes: Vec<u8>,
}

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
    /// The user's own icon, if they attached one (ADD-001 tier 2).
    ///
    /// **Appended last, and that is load-bearing.** `postcard` is not self-describing,
    /// so field order is the format: a field added anywhere else would shift every byte
    /// after it and no existing payload would decode.
    pub custom_icon: Option<StoredIcon>,
}

/// [`ItemMetaPayload`] as it stood before ADD-001 added the tier-2 icon field.
///
/// `postcard` is not self-describing, so a payload written before `custom_icon` existed
/// simply *ends* where the current struct expects to read the `Option` discriminant, and
/// `deserialize_option` reports `DeserializeUnexpectedEnd` rather than `None`. Appending
/// the field last was the least disruptive position available; it was never a compatible
/// one. This is where the compatibility actually lives.
///
/// Reads are tolerant and writes never are: `repository::read_meta_payload` tries the
/// current shape first and falls back to this one, while every write emits the current
/// shape. A vault therefore upgrades itself item by item as items are edited, and nothing
/// is rewritten eagerly — the fallback costs one failed parse on a pre-icon row and
/// nothing at all on a current one.
///
/// The discrimination is unambiguous in both directions. A current payload carries the
/// trailing discriminant and decodes as itself. A pre-icon payload cannot decode as the
/// current shape, because the buffer is exhausted at exactly that byte. No input silently
/// selects the wrong one.
///
/// **This struct never gains a field.** It is a record of a format that already shipped;
/// the next format change appends to [`ItemMetaPayload`] and adds a sibling of this.
#[derive(Deserialize)]
pub(crate) struct ItemMetaPayloadPreIcon {
    kind: ItemKind,
    title: String,
    notes: String,
    tags: Vec<String>,
    favorite: bool,
    created_at: i64,
    custom_fields: Vec<CustomField>,
    body: ItemBodyMeta,
}

impl From<ItemMetaPayloadPreIcon> for ItemMetaPayload {
    fn from(old: ItemMetaPayloadPreIcon) -> Self {
        Self {
            kind: old.kind,
            title: old.title,
            notes: old.notes,
            tags: old.tags,
            favorite: old.favorite,
            created_at: old.created_at,
            custom_fields: old.custom_fields,
            body: old.body,
            custom_icon: None,
        }
    }
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
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct ItemSecretPayload {
    /// `login.password`.
    pub password: Option<String>,
    /// `login.totp.secret` — the base32 seed, and the only secret half of a TOTP
    /// configuration.
    pub totp_secret: Option<String>,
    /// The rest of the TOTP configuration.
    ///
    /// `zeroize(skip)` because none of it is a secret: they are public protocol
    /// parameters and two display labels. Zeroizing them would cost work and
    /// protect nothing, and marking them skipped documents the judgement instead
    /// of leaving a reader to wonder.
    #[zeroize(skip)]
    pub totp_params: Option<TotpParams>,
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

impl fmt::Debug for ItemSecretPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Presence rather than values. Which secrets an item *has* already
        // crosses IPC as `SecretPresence`, so it is not a disclosure; the values
        // are the entire point of the type and none of them appears here.
        let present = |set: bool| if set { "<redacted>" } else { "None" };
        f.debug_struct("ItemSecretPayload")
            .field("password", &present(self.password.is_some()))
            .field("totp_secret", &present(self.totp_secret.is_some()))
            .field("totp_params", &self.totp_params)
            .field("card_number", &present(self.card_number.is_some()))
            .field("card_cvv", &present(self.card_cvv.is_some()))
            .field("card_pin", &present(self.card_pin.is_some()))
            .field("document_number", &present(self.document_number.is_some()))
            .field(
                "hidden_custom",
                &format_args!("{} <redacted>", self.hidden_custom.len()),
            )
            .field(
                "password_history",
                &format_args!("{} <redacted>", self.password_history.len()),
            )
            .finish()
    }
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
    /// Whether the user attached an icon.
    ///
    /// A flag, not the bytes. The index is built by decrypting every item's `meta_ct`
    /// at unlock and is held for the life of the session — carrying up to 64 KB per
    /// item here would put a 10,000-item vault hundreds of megabytes over §9's memory
    /// budget for a decoration. The bytes are read on demand by id.
    pub has_custom_icon: bool,
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
    /// Whether the user attached an icon (ADD-001). A flag, never the bytes.
    pub has_custom_icon: bool,
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
