//! Types that cross the IPC boundary.
//!
//! CLAUDE.md §5: Rust types are the source of truth. These derive `ts-rs`, which
//! emits the TypeScript into `src/ipc/generated/` during `cargo test`, and CI
//! fails on any diff — so a Rust field added without regenerating is a build
//! failure rather than a runtime `undefined`.
//!
//! Every type here is a *projection*. None of them is a domain type re-exported
//! wholesale, because that is how a secret field ends up on the wire by
//! accident: `ItemSummaryDto` cannot carry a password because it has nowhere to
//! put one.

use keyring_store::{
    ActivityEvent, ActivityKind, CustomField, CustomFieldKind, IndexRow, ItemBody, ItemBodyMeta,
    ItemDraft, ItemKind, ItemMeta, SecretField, TotpAlgorithm, TotpConfig, VaultKind, VaultSummary,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::index::{ItemSource, ListQuery, QuickFilters, SortOrder};
use crate::services::{generator, history, report, totp};

/// Where the vault is (SPEC-V1 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum VaultStateDto {
    /// No vault file exists yet.
    Uninitialised,
    /// A vault exists and is closed.
    Locked,
    /// A derivation is in flight.
    Unlocking,
    /// Keys are in memory.
    Unlocked,
}

impl From<crate::session::VaultState> for VaultStateDto {
    fn from(s: crate::session::VaultState) -> Self {
        use crate::session::VaultState as V;
        match s {
            V::Uninitialised => Self::Uninitialised,
            V::Locked => Self::Locked,
            V::Unlocking => Self::Unlocking,
            V::Unlocked => Self::Unlocked,
        }
    }
}

/// Answer to `account_status` (SPEC-V1 §6).
///
/// No `last_sync`: sync is SPEC-V3, and §1 says not to scaffold out-of-scope
/// items.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    /// Lock state.
    pub state: VaultStateDto,
    /// Number of live vaults. Zero while locked.
    pub vault_count: usize,
    /// Number of live items. Zero while locked.
    pub item_count: usize,
    /// Whether biometric unlock is available on this device *and* enrolled.
    pub biometric_available: bool,
    /// What this platform's biometric is called, for the UI.
    pub biometric_label: String,
    /// Whether a master-password unlock is due (SPEC-V1 §5.1, 14 days).
    pub password_unlock_due: bool,
}

/// Item type, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum ItemKindDto {
    /// A login.
    Login,
    /// A secure note.
    SecureNote,
    /// A payment card.
    Card,
    /// An identity document.
    Identity,
}

impl From<ItemKind> for ItemKindDto {
    fn from(k: ItemKind) -> Self {
        match k {
            ItemKind::Login => Self::Login,
            ItemKind::SecureNote => Self::SecureNote,
            ItemKind::Card => Self::Card,
            ItemKind::Identity => Self::Identity,
        }
    }
}

impl From<ItemKindDto> for ItemKind {
    fn from(k: ItemKindDto) -> Self {
        match k {
            ItemKindDto::Login => Self::Login,
            ItemKindDto::SecureNote => Self::SecureNote,
            ItemKindDto::Card => Self::Card,
            ItemKindDto::Identity => Self::Identity,
        }
    }
}

/// A row in the item list.
///
/// SPEC-V1 §6: contains no secret field, ever. There is deliberately no
/// `password`, no `cvv` and no free-form value bag to smuggle one through.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct ItemSummaryDto {
    /// Item id.
    #[ts(type = "string")]
    pub id: Uuid,
    /// Owning vault.
    #[ts(type = "string")]
    pub vault_id: Uuid,
    /// Item type.
    pub kind: ItemKindDto,
    /// Title.
    pub title: String,
    /// Type-appropriate subtitle.
    pub subtitle: Option<String>,
    /// Whether a TOTP configuration exists. Never the secret itself.
    pub has_totp: bool,
    /// Favourite flag.
    pub is_favorite: bool,
    /// Always false in V1; sharing is SPEC-V2.
    pub is_shared: bool,
    /// Current revision.
    #[ts(type = "number")]
    pub revision: u64,
    /// Last modification, Unix milliseconds.
    #[ts(type = "number")]
    pub updated_at: i64,
}

impl From<&IndexRow> for ItemSummaryDto {
    fn from(row: &IndexRow) -> Self {
        Self {
            id: row.id,
            vault_id: row.vault_id,
            kind: row.kind.into(),
            title: row.title.clone(),
            subtitle: row.subtitle.clone(),
            has_totp: row.has_totp,
            is_favorite: row.favorite,
            is_shared: false,
            revision: row.revision,
            updated_at: row.updated_at,
        }
    }
}

/// Whether a secret field exists, without revealing it (SPEC-V1 §6).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct SecretPresence {
    /// Which field.
    pub field: SecretFieldDto,
    /// Whether it has a value. Never the value.
    pub present: bool,
}

/// A secret field, addressed by a closed enum rather than a string.
///
/// SPEC-V1 §4.1 is explicit: a bare string parameter invites a field-traversal
/// bug where a crafted value reaches something it shouldn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase", tag = "field", content = "index")]
pub enum SecretFieldDto {
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
    /// A hidden custom field, by index.
    Custom(u16),
}

impl From<SecretFieldDto> for SecretField {
    fn from(f: SecretFieldDto) -> Self {
        match f {
            SecretFieldDto::Password => Self::Password,
            SecretFieldDto::TotpSecret => Self::TotpSecret,
            SecretFieldDto::CardNumber => Self::CardNumber,
            SecretFieldDto::CardCvv => Self::CardCvv,
            SecretFieldDto::CardPin => Self::CardPin,
            SecretFieldDto::DocumentNumber => Self::DocumentNumber,
            SecretFieldDto::Custom(i) => Self::Custom(i),
        }
    }
}

/// A custom field as shown in item detail. Hidden values are blank.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldDto {
    /// Label.
    pub label: String,
    /// Value — empty when `hidden`, which is what makes this safe to list.
    pub value: String,
    /// Field kind.
    pub kind: CustomFieldKindDto,
}

/// What a custom field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum CustomFieldKindDto {
    /// Plain text.
    Text,
    /// Secret; the value is fetched separately.
    Hidden,
    /// A URL.
    Url,
    /// A date.
    Date,
}

impl From<CustomFieldKind> for CustomFieldKindDto {
    fn from(k: CustomFieldKind) -> Self {
        match k {
            CustomFieldKind::Text => Self::Text,
            CustomFieldKind::Hidden => Self::Hidden,
            CustomFieldKind::Url => Self::Url,
            CustomFieldKind::Date => Self::Date,
        }
    }
}

/// Item detail: everything except the secrets themselves (SPEC-V1 §6).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct ItemDetailDto {
    /// Item id.
    #[ts(type = "string")]
    pub id: Uuid,
    /// Owning vault.
    #[ts(type = "string")]
    pub vault_id: Uuid,
    /// Item type.
    pub kind: ItemKindDto,
    /// Title.
    pub title: String,
    /// Notes.
    pub notes: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Favourite flag.
    pub favorite: bool,
    /// Current revision.
    #[ts(type = "number")]
    pub revision: u64,
    /// Creation time, Unix milliseconds.
    #[ts(type = "number")]
    pub created_at: i64,
    /// Non-secret custom fields.
    pub custom_fields: Vec<CustomFieldDto>,
    /// Which secret fields exist, and only that.
    pub secrets: Vec<SecretPresence>,
    /// Non-secret type-specific fields, as label/value pairs so the frontend can
    /// render any item type without a per-type component in V1.
    pub fields: Vec<LabelledValue>,
}

/// A rendered non-secret field.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct LabelledValue {
    /// Field label.
    pub label: String,
    /// Field value.
    pub value: String,
}

/// Collect labelled non-secret values, skipping empties.
///
/// A field with no value is not rendered, so emitting it would make every item
/// detail a list of blanks.
struct FieldSink(Vec<LabelledValue>);

impl FieldSink {
    fn push(&mut self, label: &str, value: &str) {
        if !value.is_empty() {
            self.0.push(LabelledValue {
                label: label.to_owned(),
                value: value.to_owned(),
            });
        }
    }
}

fn present(field: SecretFieldDto) -> SecretPresence {
    SecretPresence {
        field,
        present: true,
    }
}

/// Project one item body into its visible fields and its secret *presence* list.
///
/// The two halves are produced together on purpose: which secrets exist is
/// decided by the same match that decides which labels to show, so a new item
/// type cannot add a field without someone deciding which half it belongs in.
fn project_body(body: &ItemBodyMeta) -> (Vec<LabelledValue>, Vec<SecretPresence>) {
    let mut fields = FieldSink(Vec::new());
    let mut secrets = Vec::new();

    match body {
        ItemBodyMeta::Login {
            username,
            urls,
            has_totp,
        } => {
            fields.push("Username", username);
            for url in urls {
                fields.push("Website", url);
            }
            secrets.push(present(SecretFieldDto::Password));
            if *has_totp {
                secrets.push(present(SecretFieldDto::TotpSecret));
            }
        }
        // The body of a secure note *is* the item's `notes`, which every type
        // carries, so there is nothing type-specific to project.
        ItemBodyMeta::SecureNote => {}
        ItemBodyMeta::Card {
            cardholder,
            expiry_month,
            expiry_year,
            billing_address,
            last4,
        } => {
            fields.push("Cardholder", cardholder);
            fields.push("Expires", &format!("{expiry_month:02}/{expiry_year}"));
            fields.push("Billing address", billing_address);
            if let Some(last4) = last4 {
                fields.push("Card number", &format!("···· {last4}"));
            }
            secrets.extend(
                [
                    SecretFieldDto::CardNumber,
                    SecretFieldDto::CardCvv,
                    SecretFieldDto::CardPin,
                ]
                .map(present),
            );
        }
        ItemBodyMeta::Identity {
            first_name,
            last_name,
            dob,
            document_type,
            issuing_country,
            expiry,
            address,
            phone,
            email,
        } => {
            fields.push("Name", format!("{first_name} {last_name}").trim());
            fields.push("Date of birth", dob);
            fields.push("Document type", document_type);
            fields.push("Issuing country", issuing_country);
            fields.push("Expires", expiry);
            fields.push("Address", address);
            fields.push("Phone", phone);
            fields.push("Email", email);
            secrets.push(present(SecretFieldDto::DocumentNumber));
        }
    }

    (fields.0, secrets)
}

impl ItemDetailDto {
    /// Project decrypted metadata for the wire, deciding which secrets exist
    /// without reading any of them.
    #[must_use]
    pub fn from_meta(meta: &ItemMeta) -> Self {
        let (fields, mut secrets) = project_body(&meta.body);

        // A hidden custom field is addressed by its index in the item's own
        // list, which is why the enum carries a `u16` rather than a label: a
        // label is user input, and user input naming a field to decrypt is the
        // field-traversal bug SPEC-V1 §4.1 rules out.
        for (index, field) in meta.custom_fields.iter().enumerate() {
            if field.kind == CustomFieldKind::Hidden {
                secrets.push(present(SecretFieldDto::Custom(
                    u16::try_from(index).unwrap_or(u16::MAX),
                )));
            }
        }

        Self {
            id: meta.id,
            vault_id: meta.vault_id,
            kind: meta.kind.into(),
            title: meta.title.clone(),
            notes: meta.notes.clone(),
            tags: meta.tags.clone(),
            favorite: meta.favorite,
            revision: meta.revision,
            created_at: meta.created_at,
            custom_fields: meta
                .custom_fields
                .iter()
                .map(|f| CustomFieldDto {
                    label: f.label.clone(),
                    value: f.value.clone(),
                    kind: f.kind.into(),
                })
                .collect(),
            secrets,
            fields,
        }
    }
}

/// A vault, on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct VaultSummaryDto {
    /// Vault id.
    #[ts(type = "string")]
    pub id: Uuid,
    /// Display name.
    pub name: String,
    /// A token *name* such as `vault.accent.3`, never a hex value.
    pub color_token: String,
    /// Personal or user-created.
    pub kind: VaultKindDto,
    /// Live item count, excluding soft-deleted rows.
    pub item_count: usize,
}

/// Whether a vault is the built-in personal one or user-created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum VaultKindDto {
    /// The default vault created with the account.
    Personal,
    /// A user-created vault.
    Custom,
}

impl From<VaultSummary> for VaultSummaryDto {
    fn from(v: VaultSummary) -> Self {
        Self {
            id: v.id,
            name: v.name,
            color_token: v.color_token,
            kind: match v.kind {
                VaultKind::Personal => VaultKindDto::Personal,
                VaultKind::Custom => VaultKindDto::Custom,
            },
            item_count: v.item_count,
        }
    }
}

/// Platform facts the UI needs, so no component hardcodes a modifier key.
///
/// SPEC-V1 §8: never a literal `⌘` in source.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    /// `windows` or `macos`.
    pub os: String,
    /// Target architecture.
    pub arch: String,
    /// The modifier key label for shortcuts: `Cmd` or `Ctrl`.
    pub modifier_key: String,
    /// What this platform's biometric is called, for the UI.
    pub biometric_label: String,
}

// ── Activity (SPEC-V1 §4.3) ─────────────────────────────────────────────────

/// What happened to an item, on the wire.
///
/// The two SPEC-V2 kinds and the SPEC-V3 one are present because the store's
/// on-disk discriminants reserve them; V1 never produces them, and the frontend
/// renders them as the generic "updated" string rather than pretending to know
/// about sharing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum ActivityKindDto {
    /// The item was created.
    Created,
    /// A non-secret field changed.
    Updated,
    /// The password changed.
    PasswordChanged,
    /// The item was filled into a form. SPEC-V3.
    Autofilled,
    /// A secret field was shown.
    Revealed,
    /// A secret field was copied.
    Copied,
    /// The item was shared. SPEC-V2.
    Shared,
    /// A share was revoked. SPEC-V2.
    ShareRevoked,
}

impl From<ActivityKind> for ActivityKindDto {
    fn from(k: ActivityKind) -> Self {
        match k {
            ActivityKind::Created => Self::Created,
            ActivityKind::Updated => Self::Updated,
            ActivityKind::PasswordChanged => Self::PasswordChanged,
            ActivityKind::Autofilled => Self::Autofilled,
            ActivityKind::Revealed => Self::Revealed,
            ActivityKind::Copied => Self::Copied,
            ActivityKind::Shared => Self::Shared,
            ActivityKind::ShareRevoked => Self::ShareRevoked,
        }
    }
}

/// One activity record, on the wire.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct ActivityEventDto {
    /// Event id.
    #[ts(type = "string")]
    pub id: Uuid,
    /// When it happened, Unix milliseconds.
    #[ts(type = "number")]
    pub at: i64,
    /// What happened.
    pub kind: ActivityKindDto,
}

impl From<ActivityEvent> for ActivityEventDto {
    fn from(e: ActivityEvent) -> Self {
        Self {
            id: e.id,
            at: e.at,
            kind: e.kind.into(),
        }
    }
}

// ── Item input (SPEC-V1 §4.1, §6) ───────────────────────────────────────────

/// A custom field on the way *in*, value included.
///
/// Separate from [`CustomFieldDto`], which is the outbound projection and blanks
/// hidden values. One type doing both jobs would mean the field that must be
/// empty on the way out is the same field that must be full on the way in, and
/// that is the kind of ambiguity a reviewer cannot check by reading a signature.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldInput {
    /// Label.
    pub label: String,
    /// Value. Routed to `secret_ct` when `kind` is `hidden`.
    pub value: String,
    /// Field kind.
    pub kind: CustomFieldKindDto,
}

impl From<CustomFieldKindDto> for CustomFieldKind {
    fn from(k: CustomFieldKindDto) -> Self {
        match k {
            CustomFieldKindDto::Text => Self::Text,
            CustomFieldKindDto::Hidden => Self::Hidden,
            CustomFieldKindDto::Url => Self::Url,
            CustomFieldKindDto::Date => Self::Date,
        }
    }
}

impl From<CustomFieldInput> for CustomField {
    fn from(f: CustomFieldInput) -> Self {
        Self {
            label: f.label,
            value: f.value,
            kind: f.kind.into(),
        }
    }
}

/// TOTP hash algorithm, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum TotpAlgorithmDto {
    /// SHA-1. The default every authenticator understands.
    Sha1,
    /// SHA-256.
    Sha256,
    /// SHA-512.
    Sha512,
}

impl From<TotpAlgorithmDto> for TotpAlgorithm {
    fn from(a: TotpAlgorithmDto) -> Self {
        match a {
            TotpAlgorithmDto::Sha1 => Self::Sha1,
            TotpAlgorithmDto::Sha256 => Self::Sha256,
            TotpAlgorithmDto::Sha512 => Self::Sha512,
        }
    }
}

impl From<TotpAlgorithm> for TotpAlgorithmDto {
    fn from(a: TotpAlgorithm) -> Self {
        match a {
            TotpAlgorithm::Sha1 => Self::Sha1,
            TotpAlgorithm::Sha256 => Self::Sha256,
            TotpAlgorithm::Sha512 => Self::Sha512,
        }
    }
}

/// A TOTP configuration on the way in.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct TotpConfigInput {
    /// Base32 shared secret. Routed to `secret_ct`.
    pub secret: String,
    /// Hash algorithm.
    pub algorithm: TotpAlgorithmDto,
    /// 6 or 8.
    pub digits: u8,
    /// Step, in seconds.
    pub period_seconds: u32,
    /// Issuer label.
    pub issuer: String,
    /// Account label.
    pub account: String,
}

impl From<TotpConfigInput> for TotpConfig {
    fn from(t: TotpConfigInput) -> Self {
        Self {
            secret: t.secret,
            algorithm: t.algorithm.into(),
            digits: t.digits,
            period_seconds: t.period_seconds,
            issuer: t.issuer,
            account: t.account,
        }
    }
}

/// The type-specific half of an item on the way in.
///
/// A discriminated union rather than optional-field soup (CLAUDE.md §7): a
/// `card` cannot arrive carrying a `username`, because there is nowhere to put
/// one.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
// `rename_all` renames the *variants*; `rename_all_fields` is what reaches the
// fields inside them. Without the second one a card arrives as `expiry_month`
// while every other type on the wire is camelCase — consistent between Rust and
// the generated TypeScript, but a trap for anyone hand-writing a call.
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum ItemBodyInput {
    /// A login.
    Login {
        /// Username.
        username: String,
        /// Password. Routed to `secret_ct`.
        password: String,
        /// Websites.
        urls: Vec<String>,
        /// Optional TOTP configuration.
        totp: Option<TotpConfigInput>,
    },
    /// A secure note; the body is the item's `notes`.
    SecureNote,
    /// A payment card.
    Card {
        /// Cardholder name.
        cardholder: String,
        /// Card number. Routed to `secret_ct`.
        number: String,
        /// Expiry month, 1–12.
        expiry_month: u8,
        /// Expiry year, four digits.
        expiry_year: u16,
        /// CVV. Routed to `secret_ct`.
        cvv: String,
        /// PIN. Routed to `secret_ct`.
        pin: String,
        /// Billing address.
        billing_address: String,
    },
    /// An identity document.
    Identity {
        /// Given name.
        first_name: String,
        /// Family name.
        last_name: String,
        /// Date of birth.
        dob: String,
        /// Document type.
        document_type: String,
        /// Document number. Routed to `secret_ct`.
        document_number: String,
        /// Issuing country.
        issuing_country: String,
        /// Expiry date.
        expiry: String,
        /// Address.
        address: String,
        /// Phone number.
        phone: String,
        /// Email address.
        email: String,
    },
}

impl From<ItemBodyInput> for ItemBody {
    fn from(b: ItemBodyInput) -> Self {
        match b {
            ItemBodyInput::Login {
                username,
                password,
                urls,
                totp,
            } => Self::Login {
                username,
                password,
                urls,
                totp: totp.map(Into::into),
            },
            ItemBodyInput::SecureNote => Self::SecureNote,
            ItemBodyInput::Card {
                cardholder,
                number,
                expiry_month,
                expiry_year,
                cvv,
                pin,
                billing_address,
            } => Self::Card {
                cardholder,
                number,
                expiry_month,
                expiry_year,
                cvv,
                pin,
                billing_address,
            },
            ItemBodyInput::Identity {
                first_name,
                last_name,
                dob,
                document_type,
                document_number,
                issuing_country,
                expiry,
                address,
                phone,
                email,
            } => Self::Identity {
                first_name,
                last_name,
                dob,
                document_type,
                document_number,
                issuing_country,
                expiry,
                address,
                phone,
                email,
            },
        }
    }
}

/// An item on the way in (SPEC-V1 §6, `item_upsert`).
///
/// `id` absent creates; `id` present updates. Secret values arrive in the clear
/// because the user typed them into a form — that is the one direction the
/// webview unavoidably sees plaintext, and it is the same exposure §2 already
/// documents for the master password. Nothing here travels back out.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct ItemDraftInput {
    /// Item id, when updating.
    #[ts(type = "string | null")]
    pub id: Option<Uuid>,
    /// Owning vault.
    #[ts(type = "string")]
    pub vault_id: Uuid,
    /// Title.
    pub title: String,
    /// Notes. For a secure note this is the body.
    pub notes: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Favourite flag.
    pub favorite: bool,
    /// Custom fields.
    pub custom_fields: Vec<CustomFieldInput>,
    /// The type-specific half.
    pub body: ItemBodyInput,
}

impl From<ItemDraftInput> for ItemDraft {
    fn from(d: ItemDraftInput) -> Self {
        Self {
            id: d.id,
            vault_id: d.vault_id,
            title: d.title,
            notes: d.notes,
            tags: d.tags,
            favorite: d.favorite,
            custom_fields: d.custom_fields.into_iter().map(Into::into).collect(),
            body: d.body.into(),
        }
    }
}

// ── List query (SPEC-V1 §7.1) ───────────────────────────────────────────────

/// Which items to consider, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase", tag = "source")]
pub enum ItemSourceDto {
    /// Every item in every vault.
    All,
    /// One vault.
    Vault {
        /// The vault to show.
        #[ts(type = "string")]
        id: Uuid,
    },
    /// One library category.
    Category {
        /// The item type to show.
        kind: ItemKindDto,
    },
    /// Favourites across every vault.
    Favorites,
}

impl From<ItemSourceDto> for ItemSource {
    fn from(s: ItemSourceDto) -> Self {
        match s {
            ItemSourceDto::All => Self::All,
            ItemSourceDto::Vault { id } => Self::Vault { id },
            ItemSourceDto::Category { kind } => Self::Category { kind: kind.into() },
            ItemSourceDto::Favorites => Self::Favorites,
        }
    }
}

/// Combinable quick filters, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct QuickFiltersDto {
    /// Only items whose password the last report flagged weak.
    pub weak: bool,
    /// Only items with a TOTP configuration.
    pub has_totp: bool,
    /// Only shared items. Always empty in V1 (SPEC-V2).
    pub shared: bool,
}

impl From<QuickFiltersDto> for QuickFilters {
    fn from(f: QuickFiltersDto) -> Self {
        Self {
            weak: f.weak,
            has_totp: f.has_totp,
            shared: f.shared,
        }
    }
}

/// Sort order, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum SortOrderDto {
    /// Most recently used first.
    RecentlyUsed,
    /// Most recently updated first.
    #[default]
    RecentlyUpdated,
    /// Title, case-insensitive.
    Alphabetical,
    /// Newest first.
    DateCreated,
}

impl From<SortOrderDto> for SortOrder {
    fn from(s: SortOrderDto) -> Self {
        match s {
            SortOrderDto::RecentlyUsed => Self::RecentlyUsed,
            SortOrderDto::RecentlyUpdated => Self::RecentlyUpdated,
            SortOrderDto::Alphabetical => Self::Alphabetical,
            SortOrderDto::DateCreated => Self::DateCreated,
        }
    }
}

/// A list request (SPEC-V1 §6, `items_list`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct ListQueryDto {
    /// Which items to consider.
    pub source: ItemSourceDto,
    /// Quick filters, combinable.
    pub filters: QuickFiltersDto,
    /// Sort order. Ignored while `search` is set, because relevance wins.
    pub sort: SortOrderDto,
    /// Fuzzy search text. Empty means no search.
    pub search: String,
}

impl From<ListQueryDto> for ListQuery {
    fn from(q: ListQueryDto) -> Self {
        Self {
            source: q.source.into(),
            filters: q.filters.into(),
            sort: q.sort.into(),
            search: q.search,
        }
    }
}

// ── Generator (SPEC-V1 §7.3) ────────────────────────────────────────────────

/// A generated secret and its honest entropy.
///
/// The value crosses IPC because the generator's entire purpose is to show the
/// user a password they can use. That makes this the second plaintext path out of
/// Rust after `item_reveal_field`, and it is narrow in the same way: one value,
/// on an explicit action, never persisted by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct GeneratedDto {
    /// The generated value.
    pub value: String,
    /// `floor(log2(|sample space|))`, by inclusion–exclusion. Never the inflated
    /// `length × log2(charset)` (SPEC-V1 §7.3).
    pub entropy_bits: u32,
}

/// Password generator options, on the wire.
///
/// Four class toggles, and they stay four booleans for the same reason
/// [`generator::Classes`] does: SPEC-V1 §7.3 defines exactly these switches and
/// the UI renders exactly these switches. A bitmask would satisfy the lint and
/// lose the one-to-one correspondence between the wire, the spec and the screen.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct PasswordOptionsDto {
    /// Length, clamped to 8–128 by Rust.
    pub length: usize,
    /// `A`–`Z`.
    pub uppercase: bool,
    /// `a`–`z`.
    pub lowercase: bool,
    /// `0`–`9`.
    pub digits: bool,
    /// The §7.3 symbol set.
    pub symbols: bool,
    /// Remove `l 1 I | 0 O o`.
    pub avoid_ambiguous: bool,
}

impl From<PasswordOptionsDto> for generator::PasswordOptions {
    fn from(o: PasswordOptionsDto) -> Self {
        Self {
            length: o.length,
            classes: generator::Classes {
                uppercase: o.uppercase,
                lowercase: o.lowercase,
                digits: o.digits,
                symbols: o.symbols,
            },
            avoid_ambiguous: o.avoid_ambiguous,
        }
    }
}

/// Passphrase generator options, on the wire.
///
/// `separator` and `capitalise` are presentation only. They add **zero** bits —
/// the attacker knows the scheme — and the entropy figure ignores them. A UI must
/// not imply otherwise (SPEC-V1 §7.3).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct PassphraseOptionsDto {
    /// Word count, clamped to 3–12 by Rust.
    pub words: usize,
    /// String placed between words. Adds no entropy.
    pub separator: String,
    /// Capitalise each word. Adds no entropy.
    pub capitalise: bool,
    /// Append one digit. Adds `log2(10)` bits.
    pub numeric_suffix: bool,
}

impl From<PassphraseOptionsDto> for generator::PassphraseOptions {
    fn from(o: PassphraseOptionsDto) -> Self {
        Self {
            words: o.words,
            separator: o.separator,
            capitalise: o.capitalise,
            numeric_suffix: o.numeric_suffix,
        }
    }
}

/// What produced a history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum GeneratedKindDto {
    /// A random password.
    Password,
    /// A passphrase.
    Passphrase,
    /// A numeric PIN.
    Pin,
}

impl From<history::GeneratedKind> for GeneratedKindDto {
    fn from(k: history::GeneratedKind) -> Self {
        match k {
            history::GeneratedKind::Password => Self::Password,
            history::GeneratedKind::Passphrase => Self::Passphrase,
            history::GeneratedKind::Pin => Self::Pin,
        }
    }
}

/// One generator-history entry, **without its value**.
///
/// SPEC-V1 §6 gives the history a `copy` command and no reveal, so the value never
/// crosses IPC: the user picks an entry by kind and time, and Rust puts it on the
/// clipboard. A list that carried the values would put twenty passwords into the
/// webview to render a list nobody is reading them from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntryDto {
    /// Entry id, for `generator_history_copy`.
    #[ts(type = "string")]
    pub id: Uuid,
    /// What produced it.
    pub kind: GeneratedKindDto,
    /// The entropy reported at generation.
    pub entropy_bits: u32,
    /// When it was generated, Unix milliseconds.
    #[ts(type = "number")]
    pub created_at: i64,
}

impl From<&history::HistoryEntry> for HistoryEntryDto {
    fn from(e: &history::HistoryEntry) -> Self {
        Self {
            id: e.id,
            kind: e.kind.into(),
            entropy_bits: e.entropy_bits,
            created_at: e.created_at,
        }
    }
}

// ── TOTP (SPEC-V1 §6, §7.2) ─────────────────────────────────────────────────

/// A live one-time code and its countdown.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct TotpCodeDto {
    /// The code, zero-padded to its digit count.
    pub code: String,
    /// Seconds until the step rolls over. Never zero.
    pub seconds_remaining: u32,
    /// The step length, so a countdown can be sized without a second call.
    pub period: u32,
}

impl From<totp::Code> for TotpCodeDto {
    fn from(c: totp::Code) -> Self {
        Self {
            code: c.value,
            seconds_remaining: c.seconds_remaining,
            period: c.period,
        }
    }
}

// ── The security report (SPEC-V1 §7.4) ──────────────────────────────────────

/// One weighted term of the health score.
///
/// The arithmetic crosses the boundary because §7.4 requires the breakdown to be
/// *"always visible — the user should see why, not just what."* A UI that has the
/// weights and fractions can render the reasoning; one that gets only a number has
/// to restate the formula and will eventually restate it wrongly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct HealthTermDto {
    /// Weight actually applied, after any redistribution.
    pub weight: f64,
    /// The fraction earned, `0.0..=1.0`.
    pub fraction: f64,
    /// `weight × fraction`, unrounded.
    pub points: f64,
}

impl From<report::Term> for HealthTermDto {
    fn from(t: report::Term) -> Self {
        Self {
            weight: t.weight,
            fraction: t.fraction,
            points: t.points,
        }
    }
}

/// The four terms behind a health score, in display order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct HealthBreakdownDto {
    /// Not-breached term.
    pub breached: HealthTermDto,
    /// Not-weak term.
    pub weak: HealthTermDto,
    /// Not-reused term.
    pub reused: HealthTermDto,
    /// Two-factor term. `weight` is `0` while no item is known to be capable.
    pub two_factor: HealthTermDto,
}

impl From<report::Breakdown> for HealthBreakdownDto {
    fn from(b: report::Breakdown) -> Self {
        Self {
            breached: b.breached.into(),
            weak: b.weak.into(),
            reused: b.reused.into(),
            two_factor: b.two_factor.into(),
        }
    }
}

/// Why an item appears in the risk list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum RiskKindDto {
    /// Found in a breach corpus.
    Breached,
    /// Under §7.4's crack-time threshold.
    Weak,
    /// Shared with at least one other item.
    Reused,
}

impl From<report::RiskKind> for RiskKindDto {
    fn from(k: report::RiskKind) -> Self {
        match k {
            report::RiskKind::Breached => Self::Breached,
            report::RiskKind::Weak => Self::Weak,
            report::RiskKind::Reused => Self::Reused,
        }
    }
}

/// One flagged item.
///
/// Carries the *shape* of the problem and nothing derived from the password beyond
/// the two figures §7.4 asks to be displayed. There is no field here a password
/// could be recovered from, and no field a password could be put in.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct RiskDto {
    /// Which item.
    #[ts(type = "string")]
    pub item_id: Uuid,
    /// Title, so the list renders without a second round trip.
    pub title: String,
    /// Subtitle, same reason.
    pub subtitle: Option<String>,
    /// Why it is flagged.
    pub kind: RiskKindDto,
    /// Appearances in a breach corpus, when `kind` is `breached`.
    pub breach_count: Option<u32>,
    /// Estimated offline crack time in seconds, when `kind` is `weak`.
    #[ts(type = "number | null")]
    pub crack_seconds: Option<u64>,
}

/// A set of items sharing one password (SPEC-V1 §7.4).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct ReuseGroupDto {
    /// Every item using it. Always two or more. The password itself is never here.
    #[ts(type = "string[]")]
    pub item_ids: Vec<Uuid>,
}

/// Answer to `security_report_run` (SPEC-V1 §6, §7.4).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct SecurityReportDto {
    /// 0–100, or `null` for "not enough data" — §7.4: *"not 0, not 100."*
    pub score: Option<u8>,
    /// The arithmetic. `null` exactly when `score` is.
    pub breakdown: Option<HealthBreakdownDto>,
    /// Logins with a password, the denominator of the first three terms.
    pub logins: u32,
    /// How many are breached.
    pub breached: u32,
    /// How many are weak.
    pub weak: u32,
    /// How many participate in a reuse group.
    pub reused: u32,
    /// How many are known to support a second factor.
    ///
    /// **`0` for now.** Knowing this needs the bundled 2FA directory, whose licence
    /// is still unverified in `THIRD-PARTY-NOTICES.md`, and §7.4 makes shipping one
    /// a precondition for the 2FA term carrying weight. While this is `0` the
    /// weight redistributes into the other three terms and no item is credited or
    /// penalised for a factor we cannot know about.
    pub two_factor_capable: u32,
    /// How many capable items have a TOTP configured.
    pub two_factor_enabled: u32,
    /// Items whose breach status is unknown.
    ///
    /// §7.4: *"Offline → 'not checked,' never 'safe.'"* Kept separate from
    /// `breached` so the UI can never present an unchecked item as clean.
    pub not_checked: u32,
    /// Every flagged item, breached first, then weak, then reused.
    pub risks: Vec<RiskDto>,
    /// Reuse groups, so the user sees what else is affected.
    pub reuse_groups: Vec<ReuseGroupDto>,
    /// When the breach cache was last refreshed, Unix milliseconds.
    #[ts(type = "number | null")]
    pub breach_checked_at: Option<i64>,
    /// Whether a refresh is allowed now (SPEC-V1 §7.4: at most once per 24 h).
    pub breach_refresh_available: bool,
}

/// Answer to `security_breach_check` (SPEC-V1 §6, §7.4).
///
/// Reports what the check *did*, not just whether it succeeded, so the UI can tell
/// the user the truth in all three cases: it ran, it was inside the 24-hour
/// interval, or it ran and some prefixes could not be reached. §7.4 requires
/// unreachable to read as "not checked" rather than "safe", and a result that
/// collapsed those cases into one boolean would make that impossible to say.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct BreachCheckDto {
    /// Whether anything was actually fetched. `false` inside the 24-hour interval.
    pub ran: bool,
    /// When HIBP was last reached, Unix milliseconds.
    #[ts(type = "number | null")]
    pub checked_at: Option<i64>,
    /// Earliest time another check may run, Unix milliseconds.
    #[ts(type = "number")]
    pub next_eligible_at: i64,
    /// Distinct password prefixes this check needed.
    pub prefixes_requested: u32,
    /// How many were fetched.
    pub prefixes_fetched: u32,
    /// How many could not be reached. Their items read as "not checked".
    pub prefixes_failed: u32,
    /// How many ranges the cache now holds.
    pub cached_prefixes: u32,
}

// ── Updates (SPEC-V1 §7.7) ──────────────────────────────────────────────────

/// The outcome of an update check.
///
/// A closed set rather than `Option<UpdateInfo>` alone, because §7.7 requires the
/// check to be *user-visible* and four of these five outcomes look identical
/// through an `Option`: "you are up to date", "checked an hour ago", "could not
/// reach the endpoint" and "you turned this off" are different things to tell
/// someone, and only one of them means everything is fine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum UpdateStatusDto {
    /// A newer build is available. `available` is populated.
    Available,
    /// This is the newest build the endpoint offers.
    UpToDate,
    /// Inside the 24-hour interval; no request was made.
    CheckedRecently,
    /// The endpoint could not be reached, or its answer was refused.
    ///
    /// Not an error. It also does not advance the clock, so the next launch tries
    /// again rather than waiting a day on a failure.
    CheckFailed,
    /// The user has turned update checks off.
    Disabled,
}

/// A release the endpoint is offering.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfoDto {
    /// The candidate version, strictly newer than the running one.
    pub version: String,
    /// Release notes from the manifest, if it carries any.
    ///
    /// Endpoint-controlled text. Render it as text — never as markup.
    pub notes: Option<String>,
    /// Publication date from the manifest, RFC 3339.
    pub published_at: Option<String>,
}

/// Answer to `update_check` (SPEC-V1 §6, §7.7).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckDto {
    /// What happened.
    pub status: UpdateStatusDto,
    /// The running version.
    pub current_version: String,
    /// The candidate. Populated exactly when `status` is `available`.
    pub available: Option<UpdateInfoDto>,
    /// When the endpoint was last reached, Unix milliseconds.
    #[ts(type = "number | null")]
    pub checked_at: Option<i64>,
    /// Earliest time an unattended check may run again, Unix milliseconds.
    #[ts(type = "number")]
    pub next_eligible_at: i64,
    /// Whether unattended checks are switched on (`app_state.update_checks_enabled`).
    ///
    /// Reported separately from `status` because the settings screen needs the
    /// toggle's position even when the answer to *this* call was `upToDate` or
    /// `checkedRecently`. `status == "disabled"` implies this is `false`; the
    /// converse does not hold.
    pub checks_enabled: bool,
}
