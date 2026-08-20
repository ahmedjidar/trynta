// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reading and writing the `header` row (SPEC-V1 §4.4).
//!
//! The header is the only row that is not encrypted, and the header MAC is what
//! makes the rest trustworthy: it binds the public keys, the KDF cost and the
//! manifest signature to the master password. Without it the manifest signature
//! is worthless, because an attacker who can rewrite a row can also rewrite
//! `pubkey_ed25519` and sign a manifest of their own (SPEC-V1 §3.5).

use keyring_crypto::{HeaderFields, KdfParams};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::StoreError;

/// The header row, as stored.
#[derive(Debug, Clone)]
pub struct Header {
    /// Pre-unlock DDL version.
    pub schema_version: u32,
    /// Post-unlock payload version.
    pub payload_version: u32,
    /// Crypto envelope format version.
    pub envelope_version: u16,
    /// Argon2id salt.
    pub account_salt: [u8; 32],
    /// Argon2id cost.
    pub kdf: KdfParams,
    /// `muk.verify` subkey.
    pub verifier: [u8; 32],
    /// Account X25519 public key.
    pub pubkey_x25519: [u8; 32],
    /// Account Ed25519 public key.
    pub pubkey_ed25519: [u8; 32],
    /// Account private key bundle, sealed under `muk.wrap`.
    pub privkeys_ct: Vec<u8>,
    /// Ed25519 signature over the manifest root.
    pub manifest_sig: [u8; 64],
    /// HMAC-SHA256 over the canonical header, under `muk.header`.
    pub header_mac: [u8; 32],
    /// Creation time, Unix milliseconds.
    pub created_at: i64,
}

/// `kdf_params` is stored as JSON for legibility when debugging a vault, but the
/// MAC covers the *parsed* integers — JSON is not canonical and a reformat must
/// not break authentication (ADD-002, ratified in ADD-003).
#[derive(Debug, Serialize, Deserialize)]
struct KdfParamsJson {
    m: u32,
    t: u32,
    p: u32,
}

impl Header {
    /// Borrow as the canonical field set the MAC is computed over.
    #[must_use]
    pub fn fields(&self) -> HeaderFields<'_> {
        HeaderFields {
            schema_version: self.schema_version,
            payload_version: self.payload_version,
            envelope_version: self.envelope_version,
            account_salt: &self.account_salt,
            kdf: self.kdf,
            verifier: &self.verifier,
            pubkey_x25519: &self.pubkey_x25519,
            pubkey_ed25519: &self.pubkey_ed25519,
            privkeys_ct: &self.privkeys_ct,
            manifest_sig: &self.manifest_sig,
            created_at: self.created_at,
        }
    }

    /// Read the header row.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotAVault`] if there is no header row,
    /// [`StoreError::Database`] on a query failure, [`StoreError::NotAVault`]
    /// if a fixed-width column is the wrong length.
    pub fn load(conn: &Connection) -> Result<Self, StoreError> {
        // A valid SQLite file that is not ours has no `header` table, and the
        // query below would fail with a generic database error. Someone who
        // opened the wrong file deserves to be told that, so check first.
        let has_header: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'header'",
            [],
            |r| r.get(0),
        )?;
        if has_header == 0 {
            return Err(StoreError::NotAVault);
        }

        let row = conn
            .query_row(
                "SELECT schema_version, payload_version, envelope_version, account_salt, \
                 kdf_params, verifier, pubkey_x25519, pubkey_ed25519, privkeys_ct, \
                 manifest_sig, header_mac, created_at FROM header WHERE id = 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Vec<u8>>(5)?,
                        r.get::<_, Vec<u8>>(6)?,
                        r.get::<_, Vec<u8>>(7)?,
                        r.get::<_, Vec<u8>>(8)?,
                        r.get::<_, Vec<u8>>(9)?,
                        r.get::<_, Vec<u8>>(10)?,
                        r.get::<_, i64>(11)?,
                    ))
                },
            )
            .optional()?
            .ok_or(StoreError::NotAVault)?;

        let kdf: KdfParamsJson = serde_json::from_str(&row.4).map_err(|_| StoreError::NotAVault)?;

        Ok(Self {
            schema_version: u32::try_from(row.0).map_err(|_| StoreError::NotAVault)?,
            payload_version: u32::try_from(row.1).map_err(|_| StoreError::NotAVault)?,
            envelope_version: u16::try_from(row.2).map_err(|_| StoreError::NotAVault)?,
            account_salt: fixed(&row.3)?,
            kdf: KdfParams {
                m_kib: kdf.m,
                t: kdf.t,
                p: kdf.p,
            },
            verifier: fixed(&row.5)?,
            pubkey_x25519: fixed(&row.6)?,
            pubkey_ed25519: fixed(&row.7)?,
            privkeys_ct: row.8,
            manifest_sig: fixed(&row.9)?,
            header_mac: fixed(&row.10)?,
            created_at: row.11,
        })
    }

    /// Insert the header row. Used once, at vault creation.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the insert fails.
    pub fn insert(&self, conn: &Connection) -> Result<(), StoreError> {
        conn.execute(
            "INSERT INTO header (id, schema_version, payload_version, envelope_version, \
             account_salt, kdf_params, verifier, pubkey_x25519, pubkey_ed25519, privkeys_ct, \
             manifest_sig, header_mac, created_at) \
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                i64::from(self.schema_version),
                i64::from(self.payload_version),
                i64::from(self.envelope_version),
                self.account_salt.as_slice(),
                self.kdf_json(),
                self.verifier.as_slice(),
                self.pubkey_x25519.as_slice(),
                self.pubkey_ed25519.as_slice(),
                self.privkeys_ct.as_slice(),
                self.manifest_sig.as_slice(),
                self.header_mac.as_slice(),
                self.created_at,
            ],
        )?;
        Ok(())
    }

    /// Persist the manifest signature and the recomputed MAC after a write.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the update fails.
    pub fn update_manifest(&self, conn: &Connection) -> Result<(), StoreError> {
        conn.execute(
            "UPDATE header SET manifest_sig = ?1, header_mac = ?2 WHERE id = 1",
            rusqlite::params![self.manifest_sig.as_slice(), self.header_mac.as_slice()],
        )?;
        Ok(())
    }

    /// Persist a bumped schema version and its recomputed MAC.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the update fails.
    pub fn update_schema_version(&self, conn: &Connection) -> Result<(), StoreError> {
        conn.execute(
            "UPDATE header SET schema_version = ?1, header_mac = ?2 WHERE id = 1",
            rusqlite::params![i64::from(self.schema_version), self.header_mac.as_slice()],
        )?;
        Ok(())
    }

    /// Persist a bumped payload version and its recomputed MAC.
    ///
    /// # Errors
    ///
    /// [`StoreError::Database`] if the update fails.
    pub fn update_payload_version(&self, conn: &Connection) -> Result<(), StoreError> {
        conn.execute(
            "UPDATE header SET payload_version = ?1, header_mac = ?2 WHERE id = 1",
            rusqlite::params![i64::from(self.payload_version), self.header_mac.as_slice()],
        )?;
        Ok(())
    }

    fn kdf_json(&self) -> String {
        // A three-integer object; serialization cannot fail, and falling back to
        // a literal keeps this off the error path without an unwrap.
        serde_json::to_string(&KdfParamsJson {
            m: self.kdf.m_kib,
            t: self.kdf.t,
            p: self.kdf.p,
        })
        .unwrap_or_else(|_| {
            format!(
                r#"{{"m":{},"t":{},"p":{}}}"#,
                self.kdf.m_kib, self.kdf.t, self.kdf.p
            )
        })
    }
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], StoreError> {
    bytes.try_into().map_err(|_| StoreError::NotAVault)
}
