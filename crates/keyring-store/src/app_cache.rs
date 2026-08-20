// SPDX-License-Identifier: AGPL-3.0-or-later
//! `app_cache` — the encrypted key/value carve-out (SPEC-V1 §4.4).
//!
//! Three specified features need somewhere encrypted to live that is not an
//! item, a vault or an activity row: settings (§7.5, *"encrypted in the vault,
//! except the §4.5 list"*), generator history (§7.3, *"encrypted under
//! `muk.appcache`"*) and the HIBP prefix cache (§7.4, *"cache **inside the
//! encrypted store** under `muk.appcache`"*).
//!
//! That last one is the reason this table is not optional. §7.4 spells out the
//! consequence of getting it wrong: *"a plaintext cache of your password hash
//! prefixes is a filter that massively narrows an offline attack."*
//!
//! ## Not a dumping ground
//!
//! [`AppCacheKey`] is a closed enum and the read/write functions take nothing
//! else, exactly as [`crate::app_state`] does for the plaintext carve-out. The
//! namespace list is **exhaustive** and adding one is a spec change. Enforcing it
//! in the type system rather than documenting it means a fourth namespace cannot
//! arrive without a diff somebody has to approve.
//!
//! ## What binds a row to its namespace
//!
//! Every row is sealed under `muk.appcache` with `Purpose::AppCache` and the
//! reserved key id `…0002`. Those are the same for all three namespaces, so on
//! their own they would let an attacker who can write the file move the breach
//! cache's ciphertext into the settings row and have it authenticate. The AAD's
//! `subject_id` carries a distinct constant per namespace, which is what makes
//! that substitution fail — the same job `subject_id` does for an item.
//!
//! ## What this table is *not* covered by
//!
//! The vault manifest (§3.5) signs the live **item** set. It does not cover
//! `app_cache`, so an attacker who can write the file can roll a row back or
//! delete one. That is acceptable for all three namespaces and it is worth
//! saying why: rolling back settings re-enables a default, rolling back the
//! generator history loses history, and rolling back the breach cache causes a
//! re-query. None of them is an authorization decision. Do not put one here.

use keyring_crypto::{
    derive_subkey, open, reserved_key_id, seal, Aad, Envelope, Muk, Purpose, Subkey,
    ENVELOPE_VERSION,
};
use rusqlite::{Connection, OptionalExtension};
use zeroize::Zeroizing;

use crate::error::StoreError;

/// The complete set of namespaces permitted in `app_cache` (SPEC-V1 §4.4).
///
/// Adding a variant is a spec change. If you are here to add one: an item field
/// belongs in `secret_ct`, a pre-unlock flag belongs in `app_state` §4.5, and
/// anything load-bearing for authorization belongs in neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCacheKey {
    /// Encrypted application settings (SPEC-V1 §7.5), minus the §4.5 list.
    Settings,
    /// Generator history: ≤20 entries, 7-day expiry (SPEC-V1 §7.3).
    GeneratorHistory,
    /// HIBP range-query cache, keyed by 5-hex-character prefix (SPEC-V1 §7.4).
    BreachCache,
}

impl AppCacheKey {
    /// The stored column value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::GeneratorHistory => "generator_history",
            Self::BreachCache => "breach_cache",
        }
    }

    /// Every namespace, for tests that assert the list is exhaustive.
    #[must_use]
    pub const fn all() -> [Self; 3] {
        [Self::Settings, Self::GeneratorHistory, Self::BreachCache]
    }

    /// The AAD `subject_id` that binds a row to this namespace.
    ///
    /// Distinct per namespace so a ciphertext cannot be moved between rows and
    /// still authenticate. The values sit in the same reserved low range as the
    /// header key ids and must never be reused.
    const fn subject_id(self) -> [u8; 16] {
        let tag = match self {
            Self::Settings => 0x20,
            Self::GeneratorHistory => 0x21,
            Self::BreachCache => 0x22,
        };
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, tag]
    }
}

fn aad(key: AppCacheKey) -> Aad {
    Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose: Purpose::AppCache,
        subject_id: key.subject_id(),
        // These rows are overwritten in place rather than revised, and nothing
        // signs them, so there is no revision for the AAD to track. Binding one
        // would mean a counter that only this table knows about.
        revision: 0,
        key_id: reserved_key_id::MUK_APPCACHE,
    }
}

/// Read and decrypt one namespace, if it has been written.
///
/// # Errors
///
/// [`StoreError::Database`] if the read fails, [`StoreError::Crypto`] if the row
/// does not authenticate — which fails closed rather than returning `None`,
/// because "absent" and "tampered with" must not look the same to a caller.
pub(crate) fn get(
    conn: &Connection,
    muk: &Muk,
    key: AppCacheKey,
) -> Result<Option<Zeroizing<Vec<u8>>>, StoreError> {
    let payload_ct: Option<Vec<u8>> = conn
        .query_row(
            "SELECT payload_ct FROM app_cache WHERE key = ?1",
            [key.as_str()],
            |r| r.get(0),
        )
        .optional()?;
    let Some(payload_ct) = payload_ct else {
        return Ok(None);
    };

    let cache_key = derive_subkey(muk, Subkey::AppCache);
    let envelope = Envelope::from_bytes(&payload_ct)?;
    let opened = open(&cache_key, &aad(key), &envelope)?;
    Ok(Some(Zeroizing::new(opened.to_vec())))
}

/// Encrypt and store one namespace, replacing whatever was there.
///
/// # Errors
///
/// [`StoreError::Database`] or [`StoreError::Crypto`].
pub(crate) fn put(
    conn: &Connection,
    muk: &Muk,
    key: AppCacheKey,
    payload: &[u8],
    now: i64,
) -> Result<(), StoreError> {
    let cache_key = derive_subkey(muk, Subkey::AppCache);
    let payload_ct = seal(&cache_key, &aad(key), payload)?.to_bytes();

    conn.execute(
        "INSERT INTO app_cache (key, payload_ct, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET payload_ct = ?2, updated_at = ?3",
        rusqlite::params![key.as_str(), payload_ct, now],
    )?;
    Ok(())
}

/// Delete one namespace. Deleting one that is absent is success.
///
/// # Errors
///
/// [`StoreError::Database`] if the delete fails.
pub(crate) fn clear(conn: &Connection, key: AppCacheKey) -> Result<(), StoreError> {
    conn.execute("DELETE FROM app_cache WHERE key = ?1", [key.as_str()])?;
    Ok(())
}

/// When a namespace was last written, in Unix milliseconds.
///
/// Plaintext, and deliberately so: §7.4 needs the last breach-check time to
/// enforce its 24-hour cadence, and §4.5 already lists `last_breach_check_at` as
/// a permitted plaintext key. A timestamp here leaks the same fact.
///
/// # Errors
///
/// [`StoreError::Database`] if the read fails.
pub(crate) fn updated_at(conn: &Connection, key: AppCacheKey) -> Result<Option<i64>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT updated_at FROM app_cache WHERE key = ?1",
            [key.as_str()],
            |r| r.get(0),
        )
        .optional()?)
}
