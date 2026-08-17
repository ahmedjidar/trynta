//! Keyring encrypted store — schema, two-phase migrations, item repository.
//!
//! Depends on `keyring-crypto` and nothing else in the workspace: no Tauri, no
//! frontend build. That is a deliberate placement (ADD-003 §①) so the acceptance
//! tests can exercise storage in seconds rather than minutes — tests that are
//! slow to run are tests that get run less, which makes it a security property
//! rather than a convenience.
//!
//! ## What is encrypted, and under what
//!
//! ```text
//!   header            plaintext, authenticated by header_mac under muk.header
//!   app_state         plaintext, exhaustively enumerated (§4.5)
//!   vaults.meta_ct    under the vault key, so it travels with a V2 share
//!   items.meta_ct     under item.meta   — decrypted for every item at unlock
//!   items.secret_ct   under item.secret — decrypted one field at a time
//!   activity.payload  under the vault's activity subkey
//! ```
//!
//! The meta/secret split is the whole point (SPEC-V1 §3.4): unlocking must not
//! materialise every password in the process. [`repository`]'s `split_draft` is
//! where that is enforced.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod app_state;
pub mod backoff;
pub mod error;
pub mod header;
pub mod manifest;
pub mod model;
mod repository;
pub mod schema;
pub mod vault;

pub use app_state::AppStateKey;
pub use error::{StoreError, TamperKind, UnlockError};
pub use keyring_crypto::KdfParams;
pub use model::{
    CustomField, CustomFieldKind, IndexRow, ItemBody, ItemBodyMeta, ItemDraft, ItemKind, ItemMeta,
    ItemMetaPayload, ItemSecretPayload, ItemSummary, PasswordHistoryEntry, SecretField,
    TotpAlgorithm, TotpConfig, VaultKind, VaultMetaPayload, VaultSummary, PASSWORD_HISTORY_LIMIT,
};
pub use schema::{
    MigrationSet, PayloadCtx, PayloadMigration, Phase, SchemaMigration, CURRENT_PAYLOAD_VERSION,
    CURRENT_SCHEMA_VERSION, SNAPSHOT_RETENTION,
};
pub use vault::{Session, SessionKeys, VaultFile, PURGE_AFTER_DAYS};
