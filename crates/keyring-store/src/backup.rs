//! `.tryntabak` export and restore (SPEC-V1 §7.8, AC15).
//!
//! Run 1 froze the 228-byte header (ADD-003 §④) and deliberately left the body to
//! run 2. This is the body, and the design decision that shapes everything else is
//! this:
//!
//! **A backup carries the vault's own ciphertext, unchanged.** The container is
//! sealed under a key derived from an independent passphrase, but inside it the
//! items are still encrypted under the item keys, wrapped by vault keys, wrapped
//! by the *master* password's MUK. Nothing is re-keyed.
//!
//! Three consequences follow, and all three are desirable:
//!
//! 1. **Restore reproduces a bit-identical vault**, which is what AC15 asks for.
//!    The header travels with the rows, so its `manifest_sig` is exactly right for
//!    the item set it describes.
//! 2. **Restore needs no master password.** It is a ciphertext operation: the
//!    passphrase opens the container, and the master password is only needed to
//!    *use* the restored vault afterwards. A user who has forgotten their master
//!    password can still move their backup to a new machine and then remember it.
//! 3. **A backup from a different account cannot be merged item by item**, because
//!    nothing in it decrypts under the target's MUK. That is not a limitation to
//!    work around; it is what "an independent passphrase protects the container,
//!    not the contents" means. [`RestoreMode`] reports which of the two restores
//!    is on offer rather than silently doing the wrong one.
//!
//! ## Layout
//!
//! ```text
//!        0   228   the frozen header (keyring-crypto::backup)
//!      228     4   u32 BE  sealed body length
//!      232     n   one XChaCha20-Poly1305 envelope over the postcard body
//! ```
//!
//! **One envelope over the whole body**, not one per row. A per-row format would
//! need its own defence against reordering, duplication and truncation; a single
//! AEAD tag over everything gets all three for free, and a 10,000-item vault is a
//! few megabytes.
//!
//! ## What a backup does *not* carry
//!
//! `app_state` and `app_cache` are excluded, deliberately. `app_state` is local
//! plaintext preferences — theme, window geometry, unlock backoff — and none of it
//! should travel to another machine. `app_cache` holds the generator history,
//! which is a list of real passwords the user did not ask to archive, and a breach
//! cache that is derived and worthless elsewhere. A backup is the user's
//! credentials, not their session.

use std::path::Path;

use keyring_crypto::{
    backup_leaf_hash, backup_manifest_root, backup_verifier_from, derive_backup_muk,
    derive_backup_subkey, open as open_envelope, reserved_key_id, seal, verify_backup_header_mac,
    verify_backup_passphrase, verify_ed25519, Aad, BackupHeader, BackupSubkey, Envelope, KdfParams,
    ManifestEntry, Purpose, BACKUP_HEADER_LEN, BACKUP_VERSION, ENVELOPE_VERSION, NO_SUBJECT,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::StoreError;
use crate::header::Header;
use crate::schema::INITIAL_SCHEMA;
use crate::vault::{Session, VaultFile};

/// Largest container this build will read, in bytes.
///
/// A bound so a hostile file cannot ask us to allocate arbitrarily before any
/// authentication has happened. 256 MiB is far past a 10,000-item vault.
pub const MAX_CONTAINER_BYTES: u64 = 256 * 1024 * 1024;

/// The vault header, as carried in a backup.
///
/// A separate type from [`Header`] on purpose. This is an on-disk format that must
/// be readable in five years, so it is written down explicitly rather than
/// inheriting whatever shape the live struct happens to have after future
/// refactors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HeaderSnapshot {
    schema_version: u32,
    payload_version: u32,
    envelope_version: u16,
    account_salt: [u8; 32],
    kdf_m_kib: u32,
    kdf_t: u32,
    kdf_p: u32,
    verifier: [u8; 32],
    pubkey_x25519: [u8; 32],
    pubkey_ed25519: [u8; 32],
    privkeys_ct: Vec<u8>,
    /// 64 bytes. Carried as a byte string because serde implements its array
    /// impls only up to 32, and a fixed-width field in a format that must outlive
    /// this build is better explicit than clever.
    manifest_sig: Vec<u8>,
    header_mac: [u8; 32],
    created_at: i64,
}

impl HeaderSnapshot {
    fn of(header: &Header) -> Self {
        Self {
            schema_version: header.schema_version,
            payload_version: header.payload_version,
            envelope_version: header.envelope_version,
            account_salt: header.account_salt,
            kdf_m_kib: header.kdf.m_kib,
            kdf_t: header.kdf.t,
            kdf_p: header.kdf.p,
            verifier: header.verifier,
            pubkey_x25519: header.pubkey_x25519,
            pubkey_ed25519: header.pubkey_ed25519,
            privkeys_ct: header.privkeys_ct.clone(),
            manifest_sig: header.manifest_sig.to_vec(),
            header_mac: header.header_mac,
            created_at: header.created_at,
        }
    }

    /// The signature as a fixed array, or zeroes if the stored length is wrong —
    /// which then fails signature verification, as it should.
    fn manifest_sig_array(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        if self.manifest_sig.len() == 64 {
            out.copy_from_slice(&self.manifest_sig);
        }
        out
    }

    fn to_header(&self) -> Header {
        Header {
            schema_version: self.schema_version,
            payload_version: self.payload_version,
            envelope_version: self.envelope_version,
            account_salt: self.account_salt,
            kdf: KdfParams {
                m_kib: self.kdf_m_kib,
                t: self.kdf_t,
                p: self.kdf_p,
            },
            verifier: self.verifier,
            pubkey_x25519: self.pubkey_x25519,
            pubkey_ed25519: self.pubkey_ed25519,
            privkeys_ct: self.privkeys_ct.clone(),
            manifest_sig: self.manifest_sig_array(),
            header_mac: self.header_mac,
            created_at: self.created_at,
        }
    }
}

/// One `vaults` row, verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct VaultRow {
    id: [u8; 16],
    key_id: [u8; 16],
    key_wrap_ct: Vec<u8>,
    meta_ct: Vec<u8>,
    updated_at: i64,
    deleted_at: Option<i64>,
}

/// One `items` row, verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ItemRow {
    id: [u8; 16],
    vault_id: [u8; 16],
    item_key_id: [u8; 16],
    item_key_ct: Vec<u8>,
    meta_ct: Vec<u8>,
    secret_ct: Vec<u8>,
    revision: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
}

/// One `activity` row, verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ActivityRow {
    id: [u8; 16],
    item_id: [u8; 16],
    at: i64,
    payload_ct: Vec<u8>,
}

/// The sealed half of a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Body {
    header: HeaderSnapshot,
    vaults: Vec<VaultRow>,
    items: Vec<ItemRow>,
    activity: Vec<ActivityRow>,
}

/// What an export produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupSummary {
    /// Vault rows written, soft-deleted included.
    pub vaults: usize,
    /// Item rows written, soft-deleted included.
    pub items: usize,
    /// Bytes on disk.
    pub bytes: u64,
}

/// Which restore a container can perform against a given target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    /// No vault exists at the target. Everything in the container is created.
    Fresh,
    /// A vault exists and its account key matches the container's, so items can be
    /// compared and merged one by one.
    Merge,
    /// A vault exists but belongs to a **different account**, so nothing in the
    /// container decrypts under its master password. The only coherent restore
    /// replaces it, and doing so destroys what is there.
    Replace,
}

/// What a restore would do, shown before anything is written (SPEC-V1 §7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestorePreview {
    /// Which restore is on offer.
    pub mode: RestoreMode,
    /// Items the target does not have.
    pub created: usize,
    /// Items the target has at a lower revision.
    pub merged: usize,
    /// Items the target already has at the same or a higher revision.
    pub skipped: usize,
    /// When the container was written, Unix milliseconds.
    pub created_at: i64,
}

/// A container that has been opened and fully authenticated.
///
/// Holding one means the passphrase verified, the header MAC verified, and the
/// manifest signature verified against the public key the container carries.
/// Nothing here is trustworthy before all three, which is why there is no way to
/// construct one except through [`open_container`].
pub struct BackupContents {
    body: Body,
    created_at: i64,
}

impl std::fmt::Debug for BackupContents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Counts only. Everything inside is ciphertext, so a derived `Debug` would
        // not leak a plaintext secret — but it would dump megabytes of it into a
        // log, and the header snapshot carries the sealed account key bundle and
        // both salts. None of that belongs in a formatter (CLAUDE.md §4.6).
        f.debug_struct("BackupContents")
            .field("vaults", &self.body.vaults.len())
            .field("items", &self.body.items.len())
            .field("activity", &self.body.activity.len())
            .field("created_at", &self.created_at)
            .finish_non_exhaustive()
    }
}

fn body_aad() -> Aad {
    Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose: Purpose::Backup,
        subject_id: NO_SUBJECT,
        revision: 0,
        key_id: reserved_key_id::BACKUP_WRAP,
    }
}

/// Manifest entries for the live items in a body, in the shape §3.5 defines.
fn manifest_entries(items: &[ItemRow]) -> Vec<ManifestEntry> {
    items
        .iter()
        // Soft-deleted items are excluded from the root, exactly as the vault's own
        // manifest excludes them — which is what makes a cleared `deleted_at`
        // detectable (§3.5).
        .filter(|row| row.deleted_at.is_none())
        .map(|row| ManifestEntry {
            item_id: row.id,
            revision: row.revision.unsigned_abs(),
            meta_hash: backup_leaf_hash(&row.meta_ct),
            secret_hash: backup_leaf_hash(&row.secret_ct),
        })
        .collect()
}

fn read_body(conn: &Connection, header: &Header) -> Result<Body, StoreError> {
    let mut vaults = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, key_id, key_wrap_ct, meta_ct, updated_at, deleted_at FROM vaults",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })?;
        for row in rows {
            let (id, key_id, key_wrap_ct, meta_ct, updated_at, deleted_at) = row?;
            vaults.push(VaultRow {
                id: fixed16(&id)?,
                key_id: fixed16(&key_id)?,
                key_wrap_ct,
                meta_ct,
                updated_at,
                deleted_at,
            });
        }
    }

    let mut items = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, vault_id, item_key_id, item_key_ct, meta_ct, secret_ct, revision, \
             updated_at, deleted_at FROM items",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, Vec<u8>>(4)?,
                r.get::<_, Vec<u8>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, Option<i64>>(8)?,
            ))
        })?;
        for row in rows {
            let (
                id,
                vault_id,
                item_key_id,
                item_key_ct,
                meta_ct,
                secret_ct,
                revision,
                updated_at,
                deleted_at,
            ) = row?;
            items.push(ItemRow {
                id: fixed16(&id)?,
                vault_id: fixed16(&vault_id)?,
                item_key_id: fixed16(&item_key_id)?,
                item_key_ct,
                meta_ct,
                secret_ct,
                revision,
                updated_at,
                deleted_at,
            });
        }
    }

    let mut activity = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, item_id, at, payload_ct FROM activity")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        })?;
        for row in rows {
            let (id, item_id, at, payload_ct) = row?;
            activity.push(ActivityRow {
                id: fixed16(&id)?,
                item_id: fixed16(&item_id)?,
                at,
                payload_ct,
            });
        }
    }

    Ok(Body {
        header: HeaderSnapshot::of(header),
        vaults,
        items,
        activity,
    })
}

fn fixed16(bytes: &[u8]) -> Result<[u8; 16], StoreError> {
    bytes.try_into().map_err(|_| StoreError::Database)
}

impl Session<'_> {
    /// Write a `.tryntabak` container (SPEC-V1 §7.8).
    ///
    /// `passphrase` is **independent of the master password** and gets its own
    /// salt and cost. §7.8 requires that, and the reason is longevity: a backup
    /// often outlives the machine it came from, so binding it to today's KDF
    /// parameters would freeze a 2026 cost into a file opened in 2031.
    ///
    /// The account key signs the container's manifest, which is why this needs a
    /// session while restoring does not.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the vault cannot be read or the file cannot be
    /// written, [`StoreError::Crypto`] on a derivation or sealing failure.
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned, which can only happen if another thread
    /// panicked while holding it.
    pub fn backup_export(
        &self,
        path: &Path,
        passphrase: &str,
        params: KdfParams,
    ) -> Result<BackupSummary, StoreError> {
        let body = {
            let conn = self.file_conn();
            let header = self.file_header();
            read_body(&conn, &header)?
        };

        let mut entries = manifest_entries(&body.items);
        let root = backup_manifest_root(&mut entries);
        let manifest_sig = self.sign_backup_root(&root);

        let account_salt = keyring_crypto::rng::array::<32>()?;
        let backup_muk = derive_backup_muk(passphrase.as_bytes(), &account_salt, params)?;
        let wrap = derive_backup_subkey(&backup_muk, BackupSubkey::Wrap);
        let header_key = derive_backup_subkey(&backup_muk, BackupSubkey::Header);

        let encoded = postcard::to_stdvec(&body).map_err(|_| StoreError::MalformedPayload)?;
        let sealed = seal(&wrap, &body_aad(), &encoded)?.to_bytes();

        let container_header = BackupHeader {
            backup_version: BACKUP_VERSION,
            envelope_version: ENVELOPE_VERSION,
            account_salt,
            kdf: params,
            verifier: backup_verifier_from(&backup_muk),
            pubkey_ed25519: body.header.pubkey_ed25519,
            manifest_sig,
            created_at: now_ms(),
        };

        let mut out = Vec::with_capacity(BACKUP_HEADER_LEN + 4 + sealed.len());
        out.extend_from_slice(&container_header.to_bytes(&header_key));
        let sealed_len = u32::try_from(sealed.len()).map_err(|_| StoreError::Database)?;
        out.extend_from_slice(&sealed_len.to_be_bytes());
        out.extend_from_slice(&sealed);

        std::fs::write(path, &out).map_err(|_| StoreError::Database)?;

        Ok(BackupSummary {
            vaults: body.vaults.len(),
            items: body.items.len(),
            bytes: out.len() as u64,
        })
    }

    /// Merge an authenticated container into this vault (SPEC-V1 §7.8).
    ///
    /// Only valid when the container came from **this account** — the preview
    /// reports [`RestoreMode::Merge`] in that case. Item rows are ciphertext under
    /// the same MUK, so they can be inserted verbatim; what needs the keys is
    /// re-signing the manifest afterwards, because merging changes the live item
    /// set.
    ///
    /// Transactional. Either every row lands or none does (§7.8: *"never partially
    /// applies"*).
    ///
    /// # Errors
    ///
    /// [`StoreError::Tampered`] if the container belongs to another account,
    /// [`StoreError::Database`] or [`StoreError::Crypto`] otherwise.
    ///
    /// # Panics
    ///
    /// If an internal lock is poisoned.
    pub fn backup_merge(&self, contents: &BackupContents) -> Result<RestorePreview, StoreError> {
        let preview = {
            let header = self.file_header();
            contents.preview_against(Some(&header), &self.file_conn())
        };
        if preview.mode != RestoreMode::Merge {
            return Err(StoreError::Tampered(
                crate::error::TamperKind::ManifestSignature,
            ));
        }

        {
            let conn = self.file_conn();
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let applied = apply_merge(&conn, &contents.body);
            match applied {
                Ok(()) => conn.execute_batch("COMMIT")?,
                Err(e) => {
                    // Roll back before returning, so a failed merge leaves the
                    // vault exactly as it was.
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
        }

        // The live item set changed, so the manifest and the header MAC no longer
        // describe it. Missing this would leave a vault that refuses to unlock.
        self.reseal_after_merge()?;
        Ok(preview)
    }
}

fn apply_merge(conn: &Connection, body: &Body) -> Result<(), StoreError> {
    for vault in &body.vaults {
        conn.execute(
            "INSERT INTO vaults (id, key_id, key_wrap_ct, meta_ct, updated_at, deleted_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![
                vault.id.as_slice(),
                vault.key_id.as_slice(),
                vault.key_wrap_ct,
                vault.meta_ct,
                vault.updated_at,
                vault.deleted_at,
            ],
        )?;
    }

    for item in &body.items {
        // Two ways a backup row wins, and one way it must not.
        //
        //   * a strictly higher revision — an ordinary merge
        //   * the target's copy is soft-deleted and the backup's is not, provided
        //     the backup is not older. That is how a restore recovers a deletion,
        //     which is the most common reason to run one.
        //
        // It must never overwrite a *live* row with an older revision. That would
        // silently undo a rotation the user made after a breach — precisely the
        // attack §3.5's manifest exists to catch, arriving through a feature rather
        // than an attacker, with the manifest obligingly re-signed over the result.
        //
        // Equal revisions on a live row are skipped rather than rewritten: the
        // ciphertext is identical anyway, and a needless write churns `updated_at`,
        // which is what the item list sorts by.
        conn.execute(
            "INSERT INTO items (id, vault_id, item_key_id, item_key_ct, meta_ct, secret_ct, \
             revision, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(id) DO UPDATE SET \
               vault_id = ?2, item_key_id = ?3, item_key_ct = ?4, meta_ct = ?5, \
               secret_ct = ?6, revision = ?7, updated_at = ?8, deleted_at = ?9 \
             WHERE excluded.revision > items.revision \
                OR (items.deleted_at IS NOT NULL \
                    AND excluded.deleted_at IS NULL \
                    AND excluded.revision >= items.revision)",
            rusqlite::params![
                item.id.as_slice(),
                item.vault_id.as_slice(),
                item.item_key_id.as_slice(),
                item.item_key_ct,
                item.meta_ct,
                item.secret_ct,
                item.revision,
                item.updated_at,
                item.deleted_at,
            ],
        )?;
    }

    for event in &body.activity {
        conn.execute(
            "INSERT INTO activity (id, item_id, at, payload_ct) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![
                event.id.as_slice(),
                event.item_id.as_slice(),
                event.at,
                event.payload_ct,
            ],
        )?;
    }
    Ok(())
}

/// Open and fully authenticate a container.
///
/// The order is the same discipline `VaultFile::unlock` uses, and for the same
/// reason: authenticate before trusting anything derived from the input.
///
/// 1. bounds-check the file before allocating
/// 2. parse the header for **structure only**
/// 3. derive the container's MUK from its own salt and cost
/// 4. verify the header MAC — a tampered header is refused, never repaired
/// 5. verify the passphrase, constant-time
/// 6. open the body envelope
/// 7. recompute the manifest root and verify the signature
///
/// Needs no master password: a container is ciphertext, and the passphrase only
/// opens the wrapper.
///
/// # Errors
///
/// [`StoreError::NotAVault`] if this is not a container, [`StoreError::Crypto`] if
/// the passphrase is wrong or the body does not authenticate,
/// [`StoreError::Tampered`] if the header MAC or the manifest signature fails,
/// [`StoreError::Database`] if the file cannot be read.
pub fn open_container(path: &Path, passphrase: &str) -> Result<BackupContents, StoreError> {
    let size = std::fs::metadata(path)
        .map_err(|_| StoreError::Database)?
        .len();
    if size > MAX_CONTAINER_BYTES {
        return Err(StoreError::NotAVault);
    }
    let bytes = std::fs::read(path).map_err(|_| StoreError::Database)?;
    if bytes.len() < BACKUP_HEADER_LEN + 4 {
        return Err(StoreError::NotAVault);
    }

    let (header, stored_mac) = BackupHeader::parse(&bytes).map_err(|_| StoreError::NotAVault)?;

    let backup_muk = derive_backup_muk(passphrase.as_bytes(), &header.account_salt, header.kdf)?;
    let header_key = derive_backup_subkey(&backup_muk, BackupSubkey::Header);

    // The MAC covers the verifier, so checking it first means a tampered verifier
    // reads as tampering rather than as a wrong passphrase.
    if verify_backup_header_mac(&header_key, &header, &stored_mac).is_err() {
        // Unless the passphrase is simply wrong, which is overwhelmingly the
        // likely explanation and must not be reported as an attack.
        if !verify_backup_passphrase(&backup_muk, &header.verifier) {
            return Err(StoreError::Crypto);
        }
        return Err(StoreError::Tampered(crate::error::TamperKind::HeaderMac));
    }
    if !verify_backup_passphrase(&backup_muk, &header.verifier) {
        return Err(StoreError::Crypto);
    }

    let len_at = BACKUP_HEADER_LEN;
    let sealed_len = u32::from_be_bytes([
        bytes[len_at],
        bytes[len_at + 1],
        bytes[len_at + 2],
        bytes[len_at + 3],
    ]) as usize;
    let start = len_at + 4;
    let end = start
        .checked_add(sealed_len)
        .filter(|end| *end <= bytes.len())
        .ok_or(StoreError::NotAVault)?;

    let wrap = derive_backup_subkey(&backup_muk, BackupSubkey::Wrap);
    let envelope = Envelope::from_bytes(&bytes[start..end])?;
    let opened = open_envelope(&wrap, &body_aad(), &envelope)?;
    let body: Body = postcard::from_bytes(&opened).map_err(|_| StoreError::MalformedPayload)?;

    // The signature is over the container's own item set under the backup domain,
    // so a vault's `manifest_sig` cannot be replayed in to vouch for a different
    // set of items.
    let mut entries = manifest_entries(&body.items);
    let root = backup_manifest_root(&mut entries);
    verify_ed25519(&body.header.pubkey_ed25519, &root, &header.manifest_sig)
        .map_err(|_| StoreError::Tampered(crate::error::TamperKind::ManifestRoot))?;

    Ok(BackupContents {
        body,
        created_at: header.created_at,
    })
}

impl BackupContents {
    /// When the container was written, Unix milliseconds.
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// Live items the container holds.
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.body
            .items
            .iter()
            .filter(|i| i.deleted_at.is_none())
            .count()
    }

    /// Vaults the container holds, soft-deleted excluded.
    #[must_use]
    pub fn vault_count(&self) -> usize {
        self.body
            .vaults
            .iter()
            .filter(|v| v.deleted_at.is_none())
            .count()
    }

    /// What restoring into `target` would do, without writing anything.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if an existing target cannot be read.
    pub fn preview(&self, target: &Path) -> Result<RestorePreview, StoreError> {
        if !target.exists() {
            return Ok(RestorePreview {
                mode: RestoreMode::Fresh,
                created: self.item_count(),
                merged: 0,
                skipped: 0,
                created_at: self.created_at,
            });
        }
        let file = VaultFile::open(target)?;
        let header = file.header_snapshot();
        let conn = file.conn_handle();
        Ok(self.preview_against(Some(&header), &conn))
    }

    /// Infallible: a row that cannot be read is treated as absent, which reports
    /// "created" and is the conservative answer for a preview. Failing here would
    /// mean a corrupt row in the *target* prevented a restore, which is exactly
    /// when a restore is most wanted.
    fn preview_against(&self, target_header: Option<&Header>, conn: &Connection) -> RestorePreview {
        let Some(header) = target_header else {
            return RestorePreview {
                mode: RestoreMode::Fresh,
                created: self.item_count(),
                merged: 0,
                skipped: 0,
                created_at: self.created_at,
            };
        };

        // The account key is plaintext in both headers, so this comparison needs no
        // password. Different keys mean a different account, which means nothing in
        // the container decrypts under the target's master password.
        if header.pubkey_ed25519 != self.body.header.pubkey_ed25519 {
            return RestorePreview {
                mode: RestoreMode::Replace,
                created: self.item_count(),
                merged: 0,
                skipped: 0,
                created_at: self.created_at,
            };
        }

        let mut created = 0;
        let mut merged = 0;
        let mut skipped = 0;
        for item in self.body.items.iter().filter(|i| i.deleted_at.is_none()) {
            let existing: Option<(i64, Option<i64>)> = conn
                .query_row(
                    "SELECT revision, deleted_at FROM items WHERE id = ?1",
                    [item.id.as_slice()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok();

            match existing {
                // Never seen here.
                None => created += 1,
                // Soft-deleted here. From the user's point of view the item is
                // gone, and bringing it back is the most common reason to run a
                // restore at all — counting it as "skipped" would make restore
                // useless for recovering a deletion. It only returns if the backup
                // is not *older* than what was deleted, so recovery never doubles
                // as a rollback.
                Some((current, Some(_))) if item.revision >= current => created += 1,
                // Live here, and the backup is newer.
                Some((current, None)) if item.revision > current => merged += 1,
                Some(_) => skipped += 1,
            }
        }

        RestorePreview {
            mode: RestoreMode::Merge,
            created,
            merged,
            skipped,
            created_at: self.created_at,
        }
    }

    /// Write the container's contents as a complete vault at `target`.
    ///
    /// This is the AC15 path: export → wipe → restore → identical vault. It needs
    /// no master password, because the header travels with the rows and its
    /// `manifest_sig` is already correct for the item set it describes.
    ///
    /// **Destroys whatever is at `target`.** The caller must have shown the user a
    /// [`RestorePreview`] reporting [`RestoreMode::Fresh`] or
    /// [`RestoreMode::Replace`] first.
    ///
    /// Transactional within the new file: it is built at a temporary path and moved
    /// into place, so an interrupted restore never leaves a half-written vault
    /// where the old one was (§7.8: *"never partially applies"*).
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the file cannot be written.
    pub fn restore_replacing(&self, target: &Path) -> Result<(), StoreError> {
        let staging = target.with_extension("restore-staging");
        // A stale staging file from an interrupted attempt must not be appended to.
        let _ = std::fs::remove_file(&staging);

        {
            let conn = Connection::open(&staging).map_err(|_| StoreError::Database)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "FULL")?;
            conn.execute_batch(INITIAL_SCHEMA)?;

            conn.execute_batch("BEGIN IMMEDIATE")?;
            let written = write_all(&conn, &self.body);
            match written {
                Ok(()) => conn.execute_batch("COMMIT")?,
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    drop(conn);
                    let _ = std::fs::remove_file(&staging);
                    return Err(e);
                }
            }
        }

        // The WAL and shm of the *old* vault describe a database that is about to
        // stop existing. Leaving them would let SQLite recover pages from it over
        // the restored file.
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = target.as_os_str().to_owned();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(std::path::PathBuf::from(sidecar));
        }
        std::fs::rename(&staging, target).map_err(|_| StoreError::Database)?;
        for suffix in ["-wal", "-shm"] {
            let mut from = staging.as_os_str().to_owned();
            from.push(suffix);
            let _ = std::fs::remove_file(std::path::PathBuf::from(from));
        }
        Ok(())
    }
}

fn write_all(conn: &Connection, body: &Body) -> Result<(), StoreError> {
    body.header.to_header().insert(conn)?;
    crate::schema::record(
        conn,
        crate::schema::Phase::Schema,
        body.header.schema_version,
        body.header.created_at,
    )?;
    crate::schema::record(
        conn,
        crate::schema::Phase::Payload,
        body.header.payload_version,
        body.header.created_at,
    )?;

    for vault in &body.vaults {
        conn.execute(
            "INSERT INTO vaults (id, key_id, key_wrap_ct, meta_ct, updated_at, deleted_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                vault.id.as_slice(),
                vault.key_id.as_slice(),
                vault.key_wrap_ct,
                vault.meta_ct,
                vault.updated_at,
                vault.deleted_at,
            ],
        )?;
    }
    for item in &body.items {
        conn.execute(
            "INSERT INTO items (id, vault_id, item_key_id, item_key_ct, meta_ct, secret_ct, \
             revision, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                item.id.as_slice(),
                item.vault_id.as_slice(),
                item.item_key_id.as_slice(),
                item.item_key_ct,
                item.meta_ct,
                item.secret_ct,
                item.revision,
                item.updated_at,
                item.deleted_at,
            ],
        )?;
    }
    for event in &body.activity {
        conn.execute(
            "INSERT INTO activity (id, item_id, at, payload_ct) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                event.id.as_slice(),
                event.item_id.as_slice(),
                event.at,
                event.payload_ct,
            ],
        )?;
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
