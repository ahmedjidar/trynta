# Acceptance test contract — FROZEN

Everything in `tests/acceptance/` is frozen once committed: never edited, deleted, `#[ignore]`d or
weakened to make a run pass. If a criterion in SPEC-V1 §11 turns out to be wrong or unimplementable
as written, that is a spec conversation, not a test edit.

These tests are written **before** the code they exercise, so this file pins the API surface they
depend on. Implementations must match it exactly.

---

## `keyring-crypto`

```rust
pub const ENVELOPE_VERSION: u16;
pub const AAD_LEN: usize;          // 43
pub const PAD_BLOCK: usize;        // 256

pub struct Key32;                  // Zeroizing<[u8; 32]>, redacting Debug + Display
impl Key32 {
    pub fn from_bytes(b: [u8; 32]) -> Self;
    pub fn random() -> Result<Self, CryptoError>;
    pub fn expose(&self) -> &[u8; 32];
}

pub struct Muk(Key32);
impl Muk {
    pub fn from_key32(k: Key32) -> Self;
    pub fn expose(&self) -> &[u8; 32];
}

pub struct KdfParams { pub m_kib: u32, pub t: u32, pub p: u32 }
impl KdfParams {
    pub const DEFAULT: KdfParams;      // m = 65536, t = 3, p = 4
    pub const MIN_M_KIB: u32;          // 19_456
    pub const MAX_M_KIB: u32;          // 262_144
    pub fn floor() -> KdfParams;       // m = MIN_M_KIB, t = 2, p = 1 — for tests
}

pub fn derive_muk(password: &[u8], salt: &[u8; 32], params: KdfParams) -> Result<Muk, CryptoError>;
pub fn verifier_from(muk: &Muk) -> [u8; 32];
pub fn verify_password(muk: &Muk, stored: &[u8; 32]) -> bool;   // constant time
```

## `keyring-store` (imported by the acceptance crate under the alias `store`)

```rust
pub struct VaultFile;
pub struct Session;

pub enum UnlockError {
    WrongPassword,
    Backoff { retry_in: std::time::Duration },
    TamperDetected(TamperKind),
    Store(StoreError),
}
pub enum TamperKind { HeaderMac, ManifestSignature, ManifestRoot }

impl VaultFile {
    pub fn create(path: &Path, master_password: &str, params: KdfParams) -> Result<Self, StoreError>;
    pub fn open(path: &Path) -> Result<Self, StoreError>;
    pub fn open_with(path: &Path, set: &MigrationSet) -> Result<Self, StoreError>;
    pub fn unlock(&self, master_password: &str) -> Result<Session, UnlockError>;
    pub fn unlock_with(&self, master_password: &str, set: &MigrationSet) -> Result<Session, UnlockError>;
    pub fn schema_version(&self) -> u32;
    pub fn snapshots(path: &Path) -> Result<Vec<PathBuf>, StoreError>;
}

impl Session {
    pub fn payload_version(&self) -> u32;
    pub fn vault_add(&self, name: &str, color_token: &str) -> Result<Uuid, StoreError>;
    pub fn vaults_list(&self) -> Result<Vec<VaultSummary>, StoreError>;
    pub fn item_upsert(&self, draft: &ItemDraft) -> Result<Uuid, StoreError>;
    pub fn items_list(&self) -> Result<Vec<ItemSummary>, StoreError>;
    pub fn item_meta(&self, id: Uuid) -> Result<ItemMeta, StoreError>;
    pub fn item_secret(&self, id: Uuid, field: SecretField) -> Result<Zeroizing<String>, StoreError>;
    pub fn item_delete(&self, id: Uuid) -> Result<(), StoreError>;
}

pub struct ItemDraft { pub id: Option<Uuid>, pub vault_id: Uuid, pub title: String,
                       pub notes: String, pub tags: Vec<String>, pub favorite: bool,
                       pub custom_fields: Vec<CustomField>, pub body: ItemBody }
impl ItemDraft { pub fn new(vault_id: Uuid, title: &str, body: ItemBody) -> Self; }

pub enum ItemBody {
    Login { username: String, password: String, urls: Vec<String>, totp: Option<TotpConfig> },
    SecureNote,
    Card { cardholder: String, number: String, expiry_month: u8, expiry_year: u16,
           cvv: String, pin: String, billing_address: String },
    Identity { first_name: String, last_name: String, dob: String, document_type: String,
               document_number: String, issuing_country: String, expiry: String,
               address: String, phone: String, email: String },
}

pub enum ItemKind { Login, SecureNote, Card, Identity }
pub enum SecretField { Password, TotpSecret, CardNumber, CardCvv, CardPin, DocumentNumber, Custom(u16) }
pub struct CustomField { pub label: String, pub value: String, pub kind: CustomFieldKind }
pub enum CustomFieldKind { Text, Hidden, Url, Date }

pub struct ItemSummary { pub id: Uuid, pub vault_id: Uuid, pub kind: ItemKind, pub title: String,
                         pub subtitle: Option<String>, pub has_totp: bool, pub is_favorite: bool,
                         pub revision: u64, pub updated_at: i64 }
pub struct ItemMeta { pub id: Uuid, pub vault_id: Uuid, pub kind: ItemKind, pub title: String,
                      pub notes: String, pub tags: Vec<String>, pub favorite: bool,
                      pub revision: u64, pub created_at: i64, pub body: ItemBodyMeta }

// Migration injection: the runner is real, the migrations passed in may be fixtures.
pub struct MigrationSet;
impl MigrationSet {
    pub fn current() -> Self;
    pub fn with_schema(self, m: SchemaMigration) -> Self;
    pub fn with_payload(self, m: PayloadMigration) -> Self;
}
pub struct SchemaMigration { pub version: u32, pub name: &'static str, pub sql: &'static str }
pub struct PayloadMigration { pub version: u32, pub name: &'static str,
                              pub apply: fn(&PayloadCtx) -> Result<(), StoreError> }
pub struct PayloadCtx<'a>;
impl PayloadCtx<'_> { pub fn item_count(&self) -> Result<usize, StoreError>; }

// Non-secret projection of an item body, returned by `item_meta`.
pub enum ItemBodyMeta {
    Login { username: String, urls: Vec<String>, has_totp: bool },
    SecureNote,
    Card { cardholder: String, expiry_month: u8, expiry_year: u16,
           billing_address: String, last4: Option<String> },
    Identity { first_name: String, last_name: String, dob: String, document_type: String,
               issuing_country: String, expiry: String, address: String,
               phone: String, email: String },
}

pub struct VaultSummary { pub id: Uuid, pub name: String, pub color_token: String,
                          pub kind: VaultKind, pub item_count: usize }
pub enum VaultKind { Personal, Custom }

pub struct TotpConfig { pub secret: String, pub algorithm: TotpAlgorithm, pub digits: u8,
                        pub period_seconds: u32, pub issuer: String, pub account: String }
pub enum TotpAlgorithm { Sha1, Sha256, Sha512 }
```

### Derived traits the tests rely on

- `ItemKind`, `TamperKind`, `CustomFieldKind`, `VaultKind`, `TotpAlgorithm`: `Debug + Clone + Copy + PartialEq + Eq`
- `ItemSummary`, `ItemMeta`, `ItemBodyMeta`, `VaultSummary`: `Debug + Clone`
- `StoreError`, `UnlockError`: `Debug + Display` (`std::error::Error`), **redacting** — no secret
  material may appear in either representation
- `UnlockError::WrongPassword`'s `Display` must be identical for every wrong password: the error
  must not encode *why* verification failed
- `ItemSummary`'s `Debug` must not contain any secret field, since AC01 asserts on it

Tampering tests open the SQLite file directly with `rusqlite`. That is deliberate: it is the
honest simulation of an attacker who can write the file, and it must not go through our own API.
