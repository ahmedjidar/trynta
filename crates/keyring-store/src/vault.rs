//! [`VaultFile`] and [`Session`]: opening, unlocking, and the repository.
//!
//! The order of operations in [`VaultFile::unlock_with`] is the security-
//! critical part of this file and follows SPEC-V1 §3.5 exactly:
//!
//! 1. backoff check — before any expensive work
//! 2. Argon2id → MUK
//! 3. **header MAC** — immediately after derivation, before anything else is read
//! 4. verifier — constant-time, tells us "wrong password" rather than "tampered"
//! 5. unwrap the account keys
//! 6. manifest signature over the live item set
//! 7. payload migrations, purge, and only then a usable session
//!
//! Step 3 before step 4 is deliberate: the MAC covers the verifier, so checking
//! it first means a tampered verifier is reported as tampering rather than as a
//! wrong password.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use keyring_crypto::{
    derive_muk, reserved_key_id, verify_header_mac, verify_password, Aad, AccountKeys, KdfParams,
    Key32, Muk, Purpose, ENVELOPE_VERSION, NO_SUBJECT,
};
use rusqlite::Connection;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::activity::{self, ActivityEvent, ActivityKind};
use crate::app_cache::{self, AppCacheKey};
use crate::app_state::{self, AppStateKey};
use crate::backoff;
use crate::error::{StoreError, TamperKind, UnlockError};
use crate::header::Header;
use crate::manifest;
use crate::model::{
    IndexRow, ItemDraft, ItemMeta, ItemSummary, SecretField, StoredIcon, TotpConfig, VaultKind,
    VaultSummary,
};
use crate::repository::{self, MetaEdits};
use crate::schema::{
    self, MigrationSet, PayloadCtx, Phase, CURRENT_PAYLOAD_VERSION, CURRENT_SCHEMA_VERSION,
    INITIAL_SCHEMA, SNAPSHOT_RETENTION,
};

/// Days after which a soft-deleted row is purged (SPEC-V1 §4.4).
pub const PURGE_AFTER_DAYS: i64 = 30;

const MS_PER_DAY: i64 = 86_400_000;

/// Current wall-clock time in Unix milliseconds.
///
/// Clamped at 0 rather than allowed to go negative: a machine with a clock set
/// before 1970 must not produce timestamps that sort before every real one.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The counter embedded in a snapshot filename, if this path is one.
///
/// Names look like `vault.db.snapshot.0003.schema`.
fn snapshot_index(path: &Path, prefix: &str) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix(prefix)?;
    let digits = rest.split('.').next()?;
    digits.parse().ok()
}

/// An open vault file that has not been unlocked.
///
/// Holds a connection and the parsed header. No key material.
pub struct VaultFile {
    conn: Mutex<Connection>,
    path: PathBuf,
    header: Mutex<Header>,
}

impl std::fmt::Debug for VaultFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultFile")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

fn open_connection(path: &Path) -> Result<Connection, StoreError> {
    let conn = Connection::open(path)?;
    // WAL for crash resilience; FULL synchronous because losing a vault write is
    // not an acceptable trade for speed. foreign_keys so an item cannot outlive
    // its vault.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

impl VaultFile {
    /// Create a new vault at `path` and initialise its header.
    ///
    /// Generates the account key bundle even though only its Ed25519 half is
    /// used in V1 — retrofitting identity keys onto existing vaults later is a
    /// migration nobody wants to write (SPEC-V1 §1).
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the file cannot be created or written,
    /// [`StoreError::Crypto`] if key generation or derivation fails.
    pub fn create(
        path: &Path,
        master_password: &str,
        params: KdfParams,
    ) -> Result<Self, StoreError> {
        let conn = open_connection(path)?;
        conn.execute_batch(INITIAL_SCHEMA)?;

        let account_salt = keyring_crypto::rng::array::<32>()?;
        let muk = derive_muk(master_password.as_bytes(), &account_salt, params)?;
        let keys = AccountKeys::generate()?;
        let public = keys.public();

        // Seal the private bundle under muk.wrap.
        let wrap = keyring_crypto::derive_subkey(&muk, keyring_crypto::Subkey::Wrap);
        let aad = Aad {
            envelope_version: ENVELOPE_VERSION,
            purpose: Purpose::AppCache,
            subject_id: NO_SUBJECT,
            revision: 0,
            key_id: reserved_key_id::MUK_WRAP,
        };
        let privkeys_ct = keyring_crypto::seal(&wrap, &aad, keys.to_bytes().as_ref())?.to_bytes();

        let created_at = now_ms();
        let mut header = Header {
            schema_version: CURRENT_SCHEMA_VERSION,
            payload_version: CURRENT_PAYLOAD_VERSION,
            envelope_version: ENVELOPE_VERSION,
            account_salt,
            kdf: params,
            verifier: keyring_crypto::verifier_from(&muk),
            pubkey_x25519: public.x25519,
            pubkey_ed25519: public.ed25519,
            privkeys_ct,
            // An empty vault's manifest root is the root over zero entries, so
            // it is signed like any other: a vault with no items still has an
            // authenticated item set, and adding a row without the key is
            // detectable from the very first write.
            manifest_sig: keyring_crypto::sign_manifest(&keys, &manifest::current_root(&conn)?),
            header_mac: [0u8; 32],
            created_at,
        };

        let header_key = keyring_crypto::derive_subkey(&muk, keyring_crypto::Subkey::Header);
        header.header_mac = keyring_crypto::header_mac(&header_key, &header.fields());
        header.insert(&conn)?;
        schema::record(&conn, Phase::Schema, CURRENT_SCHEMA_VERSION, created_at)?;
        schema::record(&conn, Phase::Payload, CURRENT_PAYLOAD_VERSION, created_at)?;

        Ok(Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
            header: Mutex::new(header),
        })
    }

    /// Open an existing vault, running any pending schema migrations.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotAVault`], [`StoreError::UnsupportedSchema`], or
    /// [`StoreError::Database`].
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::open_with(path, &MigrationSet::current())
    }

    /// [`VaultFile::open`] with an explicit migration set.
    ///
    /// # Errors
    ///
    /// As [`VaultFile::open`], plus [`StoreError::InvalidMigrationSet`].
    pub fn open_with(path: &Path, set: &MigrationSet) -> Result<Self, StoreError> {
        set.validate()?;
        let conn = open_connection(path)?;
        let header = Header::load(&conn)?;

        let target = set.target_schema_version();
        if header.schema_version > target {
            return Err(StoreError::UnsupportedSchema {
                found: header.schema_version,
                supported: target,
            });
        }

        let file = Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
            header: Mutex::new(header),
        };
        file.run_schema_migrations(set)?;
        Ok(file)
    }

    /// The vault's current schema version.
    ///
    /// # Panics
    ///
    /// If the internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.header.lock().expect("header lock").schema_version
    }

    /// The KDF parameters this vault's header records.
    ///
    /// Needed by the backup export: a container derived with weaker parameters than
    /// the vault it came from would be the weakest link, so §7.8's export reuses the
    /// calibration rather than picking its own.
    ///
    /// # Panics
    ///
    /// If the internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    #[must_use]
    pub fn kdf_params(&self) -> KdfParams {
        self.header.lock().expect("header lock").kdf
    }

    /// Read one `app_state` value (SPEC-V1 §4.5).
    ///
    /// On `VaultFile` rather than on `Session`, because §4.5's entire reason for
    /// existing is that these are readable **before** unlock — the theme has to
    /// render the unlock screen and the backoff counter has to gate it.
    ///
    /// [`AppStateKey`] is a closed enum, so this cannot be used to read anything
    /// §4.5 does not permit. Nothing here is secret and nothing here may be
    /// trusted for an authorization decision.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`].
    ///
    /// # Panics
    ///
    /// If the internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn state_get(&self, key: AppStateKey) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().expect("connection lock");
        app_state::get(&conn, key)
    }

    /// Read one `app_state` value as a timestamp or counter, absent as `0`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`].
    ///
    /// # Panics
    ///
    /// If the internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn state_get_i64(&self, key: AppStateKey) -> Result<i64, StoreError> {
        let conn = self.conn.lock().expect("connection lock");
        app_state::get_i64(&conn, key)
    }

    /// Write one `app_state` value.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`].
    ///
    /// # Panics
    ///
    /// If the internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn state_set(&self, key: AppStateKey, value: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("connection lock");
        app_state::set(&conn, key, value)
    }

    /// Write one `app_state` timestamp or counter.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`].
    ///
    /// # Panics
    ///
    /// If the internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn state_set_i64(&self, key: AppStateKey, value: i64) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("connection lock");
        app_state::set_i64(&conn, key, value)
    }

    /// Delete one `app_state` value. Deleting an absent one is success.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`].
    ///
    /// # Panics
    ///
    /// If the internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn state_clear(&self, key: AppStateKey) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("connection lock");
        app_state::clear(&conn, key)
    }

    /// A copy of the parsed header, for the backup module's account comparison.
    pub(crate) fn header_snapshot(&self) -> Header {
        self.header.lock().expect("header lock").clone()
    }

    /// The connection, for the backup module's revision comparison.
    pub(crate) fn conn_handle(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("connection lock")
    }

    /// Snapshot files retained beside `path`, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the directory cannot be read.
    pub fn snapshots(path: &Path) -> Result<Vec<PathBuf>, StoreError> {
        let dir = path.parent().unwrap_or(Path::new("."));
        let stem = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("vault.db");
        let prefix = format!("{stem}.snapshot.");

        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(Vec::new());
        };
        let mut found: Vec<(u64, PathBuf)> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter_map(|p| snapshot_index(&p, &prefix).map(|index| (index, p)))
            .collect();
        // Sorted by the parsed counter, not lexically: the counter is monotonic,
        // so this is chronological order and stays correct past four digits.
        found.sort_by_key(|(index, _)| *index);
        Ok(found.into_iter().map(|(_, path)| path).collect())
    }

    /// Take a `VACUUM INTO` snapshot and prune to [`SNAPSHOT_RETENTION`].
    ///
    /// Never a file copy: a copy taken with a live WAL is not a consistent
    /// database (SPEC-V1 §4.6).
    fn snapshot(&self, conn: &Connection, label: &str) -> Result<(), StoreError> {
        let stem = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("vault.db");
        let prefix = format!("{stem}.snapshot.");

        // A monotonic counter rather than a timestamp, because two migrations
        // inside the same millisecond would collide and `VACUUM INTO` refuses to
        // overwrite an existing file.
        //
        // Taken from the highest index still on disk rather than from the count
        // of files: retention prunes the oldest, so a count would fall back and
        // reuse a name that already exists.
        let next = Self::snapshots(&self.path)?
            .iter()
            .filter_map(|p| snapshot_index(p, &prefix))
            .max()
            .unwrap_or(0)
            + 1;
        let target = self
            .path
            .with_file_name(format!("{stem}.snapshot.{next:04}.{label}"));

        // `VACUUM INTO` takes a path literal, and a Windows path contains
        // backslashes, so bind it as a parameter rather than formatting it in.
        conn.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])?;

        let mut all = Self::snapshots(&self.path)?;
        while all.len() > SNAPSHOT_RETENTION {
            let oldest = all.remove(0);
            let _ = std::fs::remove_file(oldest);
        }
        Ok(())
    }

    fn run_schema_migrations(&self, set: &MigrationSet) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("connection lock");
        let mut header = self.header.lock().expect("header lock");

        let pending: Vec<_> = set
            .schema
            .iter()
            .filter(|m| m.version > header.schema_version)
            .collect();
        if pending.is_empty() {
            return Ok(());
        }

        self.snapshot(&conn, "schema")?;

        for migration in pending {
            conn.execute_batch(migration.sql)?;
            schema::record(&conn, Phase::Schema, migration.version, now_ms())?;
            header.schema_version = migration.version;
            tracing::info!(
                version = migration.version,
                name = migration.name,
                "applied schema migration"
            );
        }

        // The schema version is inside the MAC, so it has to be recomputed — but
        // that needs the MUK, which we do not have before unlock. Persist the
        // version now and let `unlock` re-MAC once it can. Until then the header
        // MAC is stale, and a stale MAC is reported as tampering, so this window
        // must be closed inside the same unlock that opens it. See
        // `reseal_header_after_schema_migration` below.
        conn.execute(
            "UPDATE header SET schema_version = ?1 WHERE id = 1",
            [i64::from(header.schema_version)],
        )?;
        Ok(())
    }

    /// Unlock with the master password.
    ///
    /// # Errors
    ///
    /// [`UnlockError::WrongPassword`], [`UnlockError::Backoff`],
    /// [`UnlockError::TamperDetected`], or [`UnlockError::Store`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn unlock(&self, master_password: &str) -> Result<Session<'_>, UnlockError> {
        self.unlock_with(master_password, &MigrationSet::current())
    }

    /// [`VaultFile::unlock`] with an explicit migration set.
    ///
    /// # Errors
    ///
    /// As [`VaultFile::unlock`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn unlock_with(
        &self,
        master_password: &str,
        set: &MigrationSet,
    ) -> Result<Session<'_>, UnlockError> {
        set.validate()?;
        let conn = self.conn.lock().expect("connection lock");

        // ── 1. backoff, before any expensive work ───────────────────────────
        let failures = app_state::get_i64(&conn, AppStateKey::BackoffFailures)?;
        let until = app_state::get_i64(&conn, AppStateKey::BackoffUntil)?;
        if let Some(retry_in) = backoff::remaining(now_ms(), until) {
            return Err(UnlockError::Backoff { retry_in });
        }

        let mut header = self.header.lock().expect("header lock");

        // ── 2. derive ───────────────────────────────────────────────────────
        // The stored cost decides how expensive an offline attack is, so an
        // attacker who can weaken it makes every future attack cheaper. We only
        // ever *write* parameters inside the §3.2 clamp, so a stored set outside
        // it did not come from us: that is tampering, and it must be reported as
        // tampering rather than as a generic key-derivation failure. This has to
        // be checked here, before derivation, because derivation with invalid
        // parameters fails first and would mask the cause.
        if !header.kdf.is_valid() {
            return Err(UnlockError::TamperDetected(TamperKind::HeaderMac));
        }
        let muk = derive_muk(master_password.as_bytes(), &header.account_salt, header.kdf)?;

        // ── 3. header MAC, before anything else is read ─────────────────────
        // The MAC covers the verifier, so checking it first means a tampered
        // verifier reads as tampering rather than as a wrong password. A stale
        // MAC after a schema migration is the one benign cause, so that case is
        // recognised explicitly rather than reported as an attack.
        let header_key = keyring_crypto::derive_subkey(&muk, keyring_crypto::Subkey::Header);
        let stored_mac = header.header_mac;
        if verify_header_mac(&header_key, &header.fields(), &stored_mac).is_err() {
            // Was the password simply wrong? If the verifier does not match
            // either, that is overwhelmingly the likely explanation, and saying
            // "this file has been modified" to someone with a typo is worse than
            // useless.
            if !verify_password(&muk, &header.verifier) {
                Self::record_failure(&conn, failures)?;
                return Err(UnlockError::WrongPassword);
            }
            if !Self::header_mac_is_stale_from_migration(&header_key, &header) {
                return Err(UnlockError::TamperDetected(TamperKind::HeaderMac));
            }
        }

        // ── 4. verifier ─────────────────────────────────────────────────────
        if !verify_password(&muk, &header.verifier) {
            Self::record_failure(&conn, failures)?;
            return Err(UnlockError::WrongPassword);
        }

        // ── 5. account keys ─────────────────────────────────────────────────
        let wrap = keyring_crypto::derive_subkey(&muk, keyring_crypto::Subkey::Wrap);
        let aad = Aad {
            envelope_version: ENVELOPE_VERSION,
            purpose: Purpose::AppCache,
            subject_id: NO_SUBJECT,
            revision: 0,
            key_id: reserved_key_id::MUK_WRAP,
        };
        let envelope = keyring_crypto::Envelope::from_bytes(&header.privkeys_ct)?;
        let bundle = keyring_crypto::open(&wrap, &aad, &envelope)?;
        let account_keys = AccountKeys::from_bytes(&bundle)?;

        if account_keys.public().ed25519 != header.pubkey_ed25519 {
            // The sealed private key and the stored public key disagree. The MAC
            // covers both, so this is not reachable by tampering — it is a
            // corrupt or half-written header.
            return Err(UnlockError::TamperDetected(TamperKind::HeaderMac));
        }

        // ── 6. manifest ─────────────────────────────────────────────────────
        manifest::verify_current(&conn, &header.pubkey_ed25519, &header.manifest_sig).map_err(
            |e| match e {
                StoreError::Tampered(kind) => UnlockError::TamperDetected(kind),
                other => UnlockError::Store(other),
            },
        )?;

        // ── success: reset the counter, not decrement it (ADD-003 §③) ───────
        app_state::set_i64(&conn, AppStateKey::BackoffFailures, 0)?;
        app_state::clear(&conn, AppStateKey::BackoffUntil)?;

        // Re-seal the header if a schema migration left the MAC stale.
        if header.schema_version != CURRENT_SCHEMA_VERSION || {
            let recomputed = keyring_crypto::header_mac(&header_key, &header.fields());
            recomputed != stored_mac
        } {
            header.header_mac = keyring_crypto::header_mac(&header_key, &header.fields());
            header.update_schema_version(&conn)?;
        }

        drop(header);
        drop(conn);

        let session = Session {
            file: self,
            keys: SessionKeys { muk, account_keys },
        };

        session.run_payload_migrations(set)?;
        session.purge_expired()?;
        Ok(session)
    }

    /// Whether the stored MAC matches the header with its pre-migration schema
    /// version — the one benign reason a MAC can be stale.
    fn header_mac_is_stale_from_migration(header_key: &Key32, header: &Header) -> bool {
        // Try every version below the current one. In practice this is a handful
        // of iterations, and it is only reached when a MAC has already failed.
        for previous in 1..header.schema_version {
            let mut candidate = header.clone();
            candidate.schema_version = previous;
            let mac = keyring_crypto::header_mac(header_key, &candidate.fields());
            if mac == header.header_mac {
                return true;
            }
        }
        false
    }

    fn record_failure(conn: &Connection, previous: i64) -> Result<(), StoreError> {
        let failures = previous.saturating_add(1);
        app_state::set_i64(conn, AppStateKey::BackoffFailures, failures)?;
        let delay = backoff::delay_after(failures);
        if delay > std::time::Duration::ZERO {
            let until =
                now_ms().saturating_add(i64::try_from(delay.as_millis()).unwrap_or(i64::MAX));
            app_state::set_i64(conn, AppStateKey::BackoffUntil, until)?;
        }
        Ok(())
    }
}

/// The key material an unlocked vault holds, separated from the borrow.
///
/// A [`Session`] borrows its [`VaultFile`], which is right for a scoped
/// operation and wrong for an application that has to hold an unlocked vault
/// across many IPC calls — that would need a self-referential struct. So the
/// keys are their own owned type: the application stores these next to an
/// `Arc<VaultFile>` and rebuilds a transient `Session` per call with
/// [`Session::resume`].
///
/// Dropping this is what "lock" means at the storage layer. `Muk` owns a
/// `Zeroizing<[u8; 32]>` and the dalek keys zeroize on drop, so there is nothing
/// further to wipe by hand — and nothing that can be forgotten.
pub struct SessionKeys {
    muk: Muk,
    account_keys: AccountKeys,
}

impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionKeys(<redacted>)")
    }
}

/// An unlocked vault. Holds the MUK and the account keys for its lifetime.
pub struct Session<'a> {
    pub(crate) file: &'a VaultFile,
    pub(crate) keys: SessionKeys,
}

impl std::fmt::Debug for Session<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never render the MUK or the account keys, not even redacted-by-proxy.
        f.write_str("Session { <unlocked> }")
    }
}

impl<'a> Session<'a> {
    /// Take ownership of the key material, consuming the session.
    ///
    /// The caller becomes responsible for the keys' lifetime, and dropping them
    /// is what locks the vault.
    #[must_use]
    pub fn into_keys(self) -> SessionKeys {
        self.keys
    }

    /// Rebuild a session from a file and previously extracted keys.
    ///
    /// Deliberately cheap and infallible: it does no verification, because the
    /// keys can only have come from a successful [`VaultFile::unlock`] on this
    /// same vault. Handing it keys from a *different* vault would fail at the
    /// first decrypt, which is the correct outcome and not something to
    /// re-check on every call.
    #[must_use]
    pub fn resume(file: &'a VaultFile, keys: SessionKeys) -> Self {
        Self { file, keys }
    }
}

impl Session<'_> {
    /// The vault's current payload version.
    ///
    /// # Panics
    ///
    /// If the internal lock is poisoned.
    #[must_use]
    pub fn payload_version(&self) -> u32 {
        self.file
            .header
            .lock()
            .expect("header lock")
            .payload_version
    }

    fn run_payload_migrations(&self, set: &MigrationSet) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let mut header = self.file.header.lock().expect("header lock");

        let pending: Vec<_> = set
            .payload
            .iter()
            .filter(|m| m.version > header.payload_version)
            .collect();
        if pending.is_empty() {
            return Ok(());
        }

        self.file.snapshot(&conn, "payload")?;

        for migration in pending {
            let ctx = PayloadCtx { conn: &conn };
            (migration.apply)(&ctx)?;
            schema::record(&conn, Phase::Payload, migration.version, now_ms())?;
            header.payload_version = migration.version;
            tracing::info!(
                version = migration.version,
                name = migration.name,
                "applied payload migration"
            );
        }

        // A payload migration may have rewritten ciphertext, so both the
        // manifest and the MAC have to be recomputed.
        header.manifest_sig = manifest::sign_current(&conn, &self.keys.account_keys)?;
        let header_key =
            keyring_crypto::derive_subkey(&self.keys.muk, keyring_crypto::Subkey::Header);
        header.header_mac = keyring_crypto::header_mac(&header_key, &header.fields());
        header.update_payload_version(&conn)?;
        header.update_manifest(&conn)?;
        Ok(())
    }

    /// Purge soft-deleted rows older than [`PURGE_AFTER_DAYS`].
    ///
    /// Runs on unlock, never on a timer (SPEC-V1 §4.4).
    fn purge_expired(&self) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let cutoff = now_ms().saturating_sub(PURGE_AFTER_DAYS * MS_PER_DAY);

        let purged = conn.execute(
            "DELETE FROM items WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
            [cutoff],
        )?;
        conn.execute(
            "DELETE FROM activity WHERE item_id NOT IN (SELECT id FROM items)",
            [],
        )?;
        conn.execute(
            "DELETE FROM vaults WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
            [cutoff],
        )?;

        if purged > 0 {
            // Purging removes only rows that were already excluded from the
            // manifest, so the root is unchanged and the signature stays valid.
            // Asserted by `tests/purge.rs` rather than assumed.
            tracing::info!(purged, "purged expired soft-deleted rows");
        }
        Ok(())
    }

    // ── Vaults ──────────────────────────────────────────────────────────────

    /// Create a vault.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn vault_add(&self, name: &str, color_token: &str) -> Result<Uuid, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let kind = if repository::vault_count(&conn)? == 0 {
            VaultKind::Personal
        } else {
            VaultKind::Custom
        };
        repository::vault_insert(&conn, &self.keys.muk, name, color_token, kind, now_ms())
    }

    /// List live vaults with their item counts.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn vaults_list(&self) -> Result<Vec<VaultSummary>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::vaults_list(&conn, &self.keys.muk)
    }

    /// Rename a vault.
    ///
    /// # Errors
    ///
    /// [`StoreError::VaultNotFound`], [`StoreError::Database`], or
    /// [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn vault_rename(&self, id: Uuid, name: &str) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::vault_rename(&conn, &self.keys.muk, id, name, now_ms())
    }

    /// Change a vault's colour token.
    ///
    /// The token is a *name* such as `vault.accent.3`, never a colour value
    /// (SPEC-V1 §4.2) — the store does not validate that, because the set of
    /// valid token names belongs to the theme layer, not to storage.
    ///
    /// # Errors
    ///
    /// [`StoreError::VaultNotFound`], [`StoreError::Database`], or
    /// [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn vault_set_color(&self, id: Uuid, color_token: &str) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::vault_set_color(&conn, &self.keys.muk, id, color_token, now_ms())
    }

    /// Soft-delete a vault, moving its items or soft-deleting them with it.
    ///
    /// # Errors
    ///
    /// [`StoreError::VaultNotFound`] if either vault is missing,
    /// [`StoreError::LastVault`] if this is the only live vault,
    /// [`StoreError::Database`] or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn vault_delete(&self, id: Uuid, move_items_to: Option<Uuid>) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::vault_delete(&conn, &self.keys.muk, id, move_items_to, now_ms())?;
        // Soft-deleting items removes them from the manifest root, and moving
        // them rewrites `item_key_ct`, so either branch changes what the
        // signature has to cover.
        self.reseal(&conn)
    }

    // ── Items ───────────────────────────────────────────────────────────────

    /// Create or update an item, then re-sign the manifest.
    ///
    /// # Errors
    ///
    /// [`StoreError::VaultNotFound`], [`StoreError::ItemNotFound`],
    /// [`StoreError::Database`], or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_upsert(&self, draft: &ItemDraft) -> Result<Uuid, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let now = now_ms();
        let outcome = repository::item_upsert(&conn, &self.keys.muk, draft, now)?;
        self.reseal(&conn)?;

        // Recorded after the reseal so a write that fails to re-sign leaves no
        // activity claiming it succeeded. `PasswordChanged` rather than
        // `Updated` when the password moved: it is the event the security
        // report and the user both care about, and collapsing the two would
        // lose it.
        let kind = if outcome.created {
            ActivityKind::Created
        } else if outcome.password_changed {
            ActivityKind::PasswordChanged
        } else {
            ActivityKind::Updated
        };
        activity::record(&conn, &self.keys.muk, outcome.id, kind, now)?;
        Ok(outcome.id)
    }

    /// Apply metadata-only edits, leaving every secret field untouched.
    ///
    /// `item_upsert` rebuilds the secret half from the draft it is given, so a detail-pane
    /// edit of a title or username routed through it would need the password in hand — and
    /// putting the password in the edit form is a second plaintext path out of Rust, which
    /// §4.4 does not permit. This carries the sealed secret across instead, so the form
    /// never sees it.
    ///
    /// Returns whether anything changed.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread panicked
    /// while holding it.
    pub fn item_edit_meta(&self, id: Uuid, edits: &MetaEdits) -> Result<bool, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let changed = repository::item_edit_meta(&conn, &self.keys.muk, id, edits, now_ms())?;
        if changed {
            self.reseal(&conn)?;
        }
        Ok(changed)
    }

    /// Set or clear an item's favourite flag.
    ///
    /// Returns whether anything changed; a no-op toggle does not burn a revision.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread panicked
    /// while holding it.
    pub fn item_set_favorite(&self, id: Uuid, favorite: bool) -> Result<bool, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let changed = repository::item_set_favorite(&conn, &self.keys.muk, id, favorite, now_ms())?;
        if changed {
            self.reseal(&conn)?;
        }
        Ok(changed)
    }

    /// Attach, replace or remove an item's TOTP configuration (SPEC-V1 §4.1).
    ///
    /// Returns whether anything changed; writing the same configuration twice does
    /// not burn a revision. Only a login can hold one.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`] if the item is absent or is not a login,
    /// [`StoreError::Database`], or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_set_totp(&self, id: Uuid, totp: Option<&TotpConfig>) -> Result<bool, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let changed = repository::item_set_totp(&conn, &self.keys.muk, id, totp, now_ms())?;
        if changed {
            self.reseal(&conn)?;
        }
        Ok(changed)
    }

    /// The user's own icon for one item, if it has one (ADD-001).
    ///
    /// Reads and decrypts that item's `meta_ct` alone. The search index carries only a
    /// flag, so this is how the bytes are obtained.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread panicked
    /// while holding it.
    pub fn item_custom_icon(&self, id: Uuid) -> Result<Option<StoredIcon>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::item_custom_icon(&conn, &self.keys.muk, id)
    }

    /// Attach an icon to an item, or remove the one it has.
    ///
    /// Returns whether anything changed; setting the same bytes twice does not burn a
    /// revision. The caller is responsible for having processed the image first — this
    /// stores exactly what it is given.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread panicked
    /// while holding it.
    pub fn item_set_custom_icon(
        &self,
        id: Uuid,
        icon: Option<StoredIcon>,
    ) -> Result<bool, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let changed = repository::item_set_custom_icon(&conn, &self.keys.muk, id, icon, now_ms())?;
        if changed {
            self.reseal(&conn)?;
        }
        Ok(changed)
    }

    /// List live items, metadata only. Never contains a secret field.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn items_list(&self) -> Result<Vec<ItemSummary>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::items_list(&conn, &self.keys.muk)
    }

    /// Build the in-memory search index (SPEC-V1 §4.7).
    ///
    /// Called once at unlock. Decrypts every live item's `meta_ct` and no
    /// `secret_ct` at all.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn index_rows(&self) -> Result<Vec<IndexRow>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::index_rows(&conn, &self.keys.muk)
    }

    /// Read one item's decrypted metadata.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or
    /// [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_meta(&self, id: Uuid) -> Result<ItemMeta, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::item_meta(&conn, &self.keys.muk, id)
    }

    /// Decrypt exactly one secret field.
    ///
    /// This is the only path to a secret value in the store. It opens the item's
    /// `secret_ct`, copies out one field, and drops the rest. It does **not**
    /// touch the item row: a reveal must not bump `revision` or `updated_at`
    /// (SPEC-V1 §4.3).
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::NoSuchField`],
    /// [`StoreError::Database`], or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_secret(
        &self,
        id: Uuid,
        field: SecretField,
    ) -> Result<Zeroizing<String>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::item_secret(&conn, &self.keys.muk, id, field)
    }

    /// The item's full TOTP configuration, seed included.
    ///
    /// Returns `None` when the item has no TOTP, or when a stored seed has no
    /// parameters beside it — guessing SHA-1/6/30 would hand back codes that may
    /// be wrong, and a missing configuration is more useful to the user than a
    /// plausible wrong number.
    ///
    /// This is a secret path: the returned config carries the seed. Callers must
    /// compute a code and drop it, never return it over IPC.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or
    /// [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_totp(&self, id: Uuid) -> Result<Option<TotpConfig>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::item_totp(&conn, &self.keys.muk, id)
    }

    /// Decrypt one secret field and record that it was shown to the user.
    ///
    /// The activity write is the whole difference from [`Session::item_secret`],
    /// and it is deliberately in the store rather than in the caller: AC10
    /// asserts that a reveal leaves `revision` and `updated_at` untouched, and
    /// that property is only meaningful if the activity write lives next to the
    /// read it accompanies, where a test can see both.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::NoSuchField`],
    /// [`StoreError::Database`], or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_reveal_field(
        &self,
        id: Uuid,
        field: SecretField,
    ) -> Result<Zeroizing<String>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let value = repository::item_secret(&conn, &self.keys.muk, id, field)?;
        activity::record(&conn, &self.keys.muk, id, ActivityKind::Revealed, now_ms())?;
        Ok(value)
    }

    /// Decrypt one secret field for the clipboard and record the copy.
    ///
    /// The value never enters the webview (CLAUDE.md §4.3); this returns it to
    /// the Rust caller that hands it to the OS clipboard.
    ///
    /// # Errors
    ///
    /// As [`Session::item_reveal_field`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_copy_field(
        &self,
        id: Uuid,
        field: SecretField,
    ) -> Result<Zeroizing<String>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        let value = repository::item_secret(&conn, &self.keys.muk, id, field)?;
        activity::record(&conn, &self.keys.muk, id, ActivityKind::Copied, now_ms())?;
        Ok(value)
    }

    /// The most recent activity for one item, newest first.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or
    /// [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_activity(&self, id: Uuid, limit: usize) -> Result<Vec<ActivityEvent>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        activity::list(&conn, &self.keys.muk, id, limit)
    }

    // ── Encrypted key/value (SPEC-V1 §4.4) ──────────────────────────────────

    /// Read one `app_cache` namespace, decrypted.
    ///
    /// Returns `None` only when the row is absent. A row that does not
    /// authenticate is an error, not a `None`: "never written" and "tampered
    /// with" must not look the same to a caller.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn app_cache_get(
        &self,
        key: AppCacheKey,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        app_cache::get(&conn, &self.keys.muk, key)
    }

    /// Encrypt and store one `app_cache` namespace.
    ///
    /// The payload is opaque bytes here on purpose. The store's job is to seal
    /// and persist them; deciding what a settings blob or a generator history
    /// looks like belongs to the layer that owns those types, and teaching this
    /// crate about them would drag the whole services layer down into storage.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] or [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn app_cache_put(&self, key: AppCacheKey, payload: &[u8]) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        app_cache::put(&conn, &self.keys.muk, key, payload, now_ms())
    }

    /// Delete one `app_cache` namespace. Deleting an absent one is success.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the delete fails.
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn app_cache_clear(&self, key: AppCacheKey) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        app_cache::clear(&conn, key)
    }

    /// When one `app_cache` namespace was last written, Unix milliseconds.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the read fails.
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn app_cache_updated_at(&self, key: AppCacheKey) -> Result<Option<i64>, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        app_cache::updated_at(&conn, key)
    }

    /// Delete activity for one item, or for every item when `id` is `None`.
    ///
    /// Returns how many rows went. SPEC-V1 §7.5 offers this under Privacy & data.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the delete fails.
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn activity_clear(&self, id: Option<Uuid>) -> Result<usize, StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        activity::clear(&conn, id)
    }

    /// Soft-delete an item, then re-sign the manifest.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or
    /// [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_delete(&self, id: Uuid) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::item_delete(&conn, id, now_ms())?;
        self.reseal(&conn)
    }

    /// Restore a soft-deleted item, then re-sign the manifest.
    ///
    /// # Errors
    ///
    /// [`StoreError::ItemNotFound`], [`StoreError::Database`], or
    /// [`StoreError::Crypto`].
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn item_restore(&self, id: Uuid) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        repository::item_restore(&conn, id, now_ms())?;
        self.reseal(&conn)
    }

    // ── Access for the backup module (SPEC-V1 §7.8) ─────────────────────────
    //
    // Backup export reads raw rows and signs the container's manifest with the
    // account key, neither of which any other caller needs. These are
    // `pub(crate)` rather than public so the capability stays inside the crate.

    /// The connection, for a caller that reads rows directly.
    pub(crate) fn file_conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.file.conn.lock().expect("connection lock")
    }

    /// A copy of the parsed header.
    pub(crate) fn file_header(&self) -> Header {
        self.file.header.lock().expect("header lock").clone()
    }

    /// Sign a backup manifest root with the account key.
    ///
    /// The account key stays inside the session; only the signature leaves.
    pub(crate) fn sign_backup_root(&self, root: &[u8; 32]) -> [u8; 64] {
        self.keys.account_keys.sign(root)
    }

    /// Re-sign the vault manifest after a merge changed the live item set.
    pub(crate) fn reseal_after_merge(&self) -> Result<(), StoreError> {
        let conn = self.file.conn.lock().expect("connection lock");
        self.reseal(&conn)
    }

    /// Recompute and persist the manifest signature and header MAC.
    ///
    /// Every mutation of the live item set goes through here. Missing one would
    /// leave a vault that refuses to unlock, which is why the write methods are
    /// deliberately few and all of them end with this call.
    fn reseal(&self, conn: &Connection) -> Result<(), StoreError> {
        let mut header = self.file.header.lock().expect("header lock");
        header.manifest_sig = manifest::sign_current(conn, &self.keys.account_keys)?;
        let header_key =
            keyring_crypto::derive_subkey(&self.keys.muk, keyring_crypto::Subkey::Header);
        header.header_mac = keyring_crypto::header_mac(&header_key, &header.fields());
        header.update_manifest(conn)
    }
}
