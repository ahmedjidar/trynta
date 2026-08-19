// SPDX-License-Identifier: AGPL-3.0-or-later
//! `SQLite` schema and the two-phase migration framework (SPEC-V1 §4.4, §4.6).
//!
//! Three different things are called "version" in this system and they must
//! never collide:
//!
//! - `schema_version` — DDL, applied **pre-unlock**, never touches ciphertext.
//! - `payload_version` — re-encrypts, applied **post-unlock**, needs the MUK.
//! - `envelope_version` — the crypto envelope format, owned by `keyring-crypto`.
//!
//! Snapshots are taken with `VACUUM INTO` and never a file copy: a copy taken
//! with a live WAL is not a consistent database (SPEC-V1 §4.6).

use rusqlite::Connection;

use crate::error::StoreError;

/// Phase discriminant used in the `migrations` table's primary key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    /// Pre-unlock DDL.
    Schema = 1,
    /// Post-unlock re-encryption.
    Payload = 2,
}

impl Phase {
    /// The stored discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// The schema version this build ships.
///
/// **Pinned at 1 by the frozen acceptance suite, not by choice.**
/// `tests/acceptance/tests/ac16_migrations.rs` asserts that a freshly created
/// vault reports `schema_version == 1`, and its `snapshot_retention_keeps_the_last_three`
/// fixture occupies versions 2 through 7 and asserts it reaches each of them.
/// `MigrationSet::validate` additionally rejects any injected migration whose
/// version is `<=` this constant, so raising it would make those fixtures
/// invalid.
///
/// The consequence: **V1 cannot ship a real schema migration.** Anything the
/// initial schema does not already contain has to be added to
/// [`INITIAL_SCHEMA`], which means only *new* vaults get it. That is fine
/// pre-1.0 and is not fine after a release, so it is a spec conversation before
/// the first one — raised, not worked around.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// The payload version this build ships.
///
/// Pinned at 1 for the same reason as [`CURRENT_SCHEMA_VERSION`]: the frozen
/// `ac16_migrations` asserts a fresh vault reports `payload_version == 1` and
/// injects its own payload fixture at version 2.
pub const CURRENT_PAYLOAD_VERSION: u32 = 1;

/// Snapshots retained before migrations (SPEC-V1 §4.6).
pub const SNAPSHOT_RETENTION: usize = 3;

/// The initial schema (SPEC-V1 §4.4).
///
/// `app_state` is the one plaintext carve-out and its permitted keys are an
/// exhaustive list in §4.5 — see [`crate::app_state`].
pub const INITIAL_SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS header (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  schema_version   INTEGER NOT NULL,
  payload_version  INTEGER NOT NULL,
  envelope_version INTEGER NOT NULL,
  account_salt     BLOB NOT NULL,
  kdf_params       TEXT NOT NULL,
  verifier         BLOB NOT NULL,
  pubkey_x25519    BLOB NOT NULL,
  pubkey_ed25519   BLOB NOT NULL,
  privkeys_ct      BLOB NOT NULL,
  manifest_sig     BLOB NOT NULL,
  header_mac       BLOB NOT NULL,
  created_at       INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS vaults (
  id          BLOB PRIMARY KEY,
  key_id      BLOB NOT NULL,
  key_wrap_ct BLOB NOT NULL,
  meta_ct     BLOB NOT NULL,
  updated_at  INTEGER NOT NULL,
  deleted_at  INTEGER
);

CREATE TABLE IF NOT EXISTS items (
  id          BLOB PRIMARY KEY,
  vault_id    BLOB NOT NULL REFERENCES vaults(id),
  item_key_id BLOB NOT NULL,
  item_key_ct BLOB NOT NULL,
  meta_ct     BLOB NOT NULL,
  secret_ct   BLOB NOT NULL,
  revision    INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  deleted_at  INTEGER
);

CREATE INDEX IF NOT EXISTS items_by_vault ON items (vault_id, deleted_at);

CREATE TABLE IF NOT EXISTS activity (
  id         BLOB PRIMARY KEY,
  item_id    BLOB NOT NULL,
  at         INTEGER NOT NULL,
  payload_ct BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS activity_by_item ON activity (item_id, at);

CREATE TABLE IF NOT EXISTS app_state (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- Encrypted key/value, for the three things §7.3, §7.4 and §7.5 need to persist
-- that are not items: settings, generator history and the HIBP prefix cache.
-- Sealed under muk.appcache; the permitted namespaces are an exhaustive closed
-- enum in crate::app_cache and adding one is a spec change.
--
-- Part of the initial schema rather than a migration to version 2, and that is
-- forced rather than chosen: tests/acceptance/ac16_migrations.rs asserts a fresh
-- vault reports schema_version 1 and claims fixture versions 2 through 7. See
-- the note on CURRENT_SCHEMA_VERSION.
CREATE TABLE IF NOT EXISTS app_cache (
  key        TEXT PRIMARY KEY,
  payload_ct BLOB NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS migrations (
  phase      INTEGER NOT NULL,
  version    INTEGER NOT NULL,
  applied_at INTEGER NOT NULL,
  PRIMARY KEY (phase, version)
);
";

/// A pre-unlock DDL migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaMigration {
    /// Target version. Must be strictly greater than the previous one.
    pub version: u32,
    /// Short name, for logs.
    pub name: &'static str,
    /// DDL to execute. Must not touch ciphertext.
    pub sql: &'static str,
}

/// Context handed to a payload migration: the MUK-derived material it needs,
/// behind an interface that does not let it reach for the raw connection.
pub struct PayloadCtx<'a> {
    pub(crate) conn: &'a Connection,
}

impl PayloadCtx<'_> {
    /// Number of live items in the vault.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the query fails.
    pub fn item_count(&self) -> Result<usize, StoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM items WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(usize::try_from(count).unwrap_or(0))
    }
}

/// A post-unlock migration that may re-encrypt.
#[derive(Clone, Copy)]
pub struct PayloadMigration {
    /// Target version.
    pub version: u32,
    /// Short name, for logs.
    pub name: &'static str,
    /// The migration itself.
    pub apply: fn(&PayloadCtx) -> Result<(), StoreError>,
}

impl std::fmt::Debug for PayloadMigration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PayloadMigration")
            .field("version", &self.version)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The migrations a given open/unlock should consider.
///
/// [`MigrationSet::current`] is what the application uses. Tests inject extra
/// migrations so the *runner* can be exercised without V1 having shipped a
/// second real migration yet.
#[derive(Debug, Clone, Default)]
pub struct MigrationSet {
    pub(crate) schema: Vec<SchemaMigration>,
    pub(crate) payload: Vec<PayloadMigration>,
}

impl MigrationSet {
    /// The set this build ships. Version 1 is the initial schema, applied at
    /// creation, so there is nothing to migrate on a current vault.
    #[must_use]
    pub fn current() -> Self {
        Self::default()
    }

    /// Add a schema migration.
    #[must_use]
    pub fn with_schema(mut self, migration: SchemaMigration) -> Self {
        self.schema.push(migration);
        self
    }

    /// Add a payload migration.
    #[must_use]
    pub fn with_payload(mut self, migration: PayloadMigration) -> Self {
        self.payload.push(migration);
        self
    }

    /// Check both lists are ordered and free of duplicates.
    ///
    /// A migration set that is out of order would apply changes in an
    /// unpredictable sequence, which for an on-disk format is unrecoverable.
    ///
    /// # Errors
    ///
    /// [`StoreError::InvalidMigrationSet`] naming the offending version.
    pub fn validate(&self) -> Result<(), StoreError> {
        let mut last = CURRENT_SCHEMA_VERSION;
        for m in &self.schema {
            if m.version <= last {
                return Err(StoreError::InvalidMigrationSet {
                    version: m.version,
                    phase: Phase::Schema.as_u8(),
                });
            }
            last = m.version;
        }
        let mut last = CURRENT_PAYLOAD_VERSION;
        for m in &self.payload {
            if m.version <= last {
                return Err(StoreError::InvalidMigrationSet {
                    version: m.version,
                    phase: Phase::Payload.as_u8(),
                });
            }
            last = m.version;
        }
        Ok(())
    }

    /// Highest schema version this set can reach.
    #[must_use]
    pub fn target_schema_version(&self) -> u32 {
        self.schema
            .last()
            .map_or(CURRENT_SCHEMA_VERSION, |m| m.version)
    }

    /// Highest payload version this set can reach.
    #[must_use]
    pub fn target_payload_version(&self) -> u32 {
        self.payload
            .last()
            .map_or(CURRENT_PAYLOAD_VERSION, |m| m.version)
    }
}

/// Record that a migration was applied.
pub(crate) fn record(
    conn: &Connection,
    phase: Phase,
    version: u32,
    at: i64,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO migrations (phase, version, applied_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![i64::from(phase.as_u8()), i64::from(version), at],
    )?;
    Ok(())
}
