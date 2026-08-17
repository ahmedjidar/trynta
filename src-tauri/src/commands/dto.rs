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
