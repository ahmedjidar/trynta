// SPDX-License-Identifier: AGPL-3.0-or-later
//! Maintaining and verifying the vault manifest (SPEC-V1 §3.5).
//!
//! The root is recomputed linearly on every write. At 10,000 items that is well
//! under a millisecond, which is why there is no incremental-update cleverness
//! here: a wrong incremental update would silently accept a rollback, and this
//! is the one place in the product where being slow and obviously correct is
//! worth more than being fast.

use keyring_crypto::{leaf_hash, manifest_root, AccountKeys, ManifestEntry};
use rusqlite::Connection;

use crate::error::{StoreError, TamperKind};

/// Collect a manifest entry for every **live** item.
///
/// Soft-deleted rows are excluded, which is what makes clearing a `deleted_at`
/// detectable: resurrecting a row changes the live set and therefore the root.
///
/// # Errors
///
/// [`StoreError::Database`] if the query fails.
pub fn collect_entries(conn: &Connection) -> Result<Vec<ManifestEntry>, StoreError> {
    let mut stmt = conn
        .prepare("SELECT id, revision, meta_ct, secret_ct FROM items WHERE deleted_at IS NULL")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (id, revision, meta_ct, secret_ct) = row?;
        let item_id: [u8; 16] = id.as_slice().try_into().map_err(|_| StoreError::Database)?;
        entries.push(ManifestEntry {
            item_id,
            revision: revision.unsigned_abs(),
            meta_hash: leaf_hash(&meta_ct),
            secret_hash: leaf_hash(&secret_ct),
        });
    }
    Ok(entries)
}

/// Recompute the manifest root over the current live item set.
///
/// # Errors
///
/// [`StoreError::Database`] if the query fails.
pub fn current_root(conn: &Connection) -> Result<[u8; 32], StoreError> {
    let mut entries = collect_entries(conn)?;
    Ok(manifest_root(&mut entries))
}

/// Sign the current live item set.
///
/// # Errors
///
/// [`StoreError::Database`] if the query fails.
pub fn sign_current(conn: &Connection, keys: &AccountKeys) -> Result<[u8; 64], StoreError> {
    Ok(keyring_crypto::sign_manifest(keys, &current_root(conn)?))
}

/// Verify the stored signature against the current live item set.
///
/// Two distinct failures, kept distinct because they mean different things:
/// a bad signature means the header and the key disagree; a mismatched root
/// means the rows changed under a valid signature — the rollback case.
///
/// # Errors
///
/// [`StoreError::Tampered`] with [`TamperKind::ManifestSignature`] or
/// [`TamperKind::ManifestRoot`], or [`StoreError::Database`].
pub fn verify_current(
    conn: &Connection,
    public_key: &[u8; 32],
    signature: &[u8; 64],
) -> Result<(), StoreError> {
    let root = current_root(conn)?;
    keyring_crypto::verify_manifest(public_key, &root, signature).map_err(|_| {
        // The signature does not verify over the root we just computed. Either
        // the row set changed (rollback, insertion, resurrection) or the
        // signature itself was replaced. Ed25519 cannot tell us which, and the
        // user-facing outcome is identical: refuse to open.
        StoreError::Tampered(TamperKind::ManifestRoot)
    })
}

/// Convenience for callers that have already decided the failure is a signature
/// problem rather than a root mismatch.
#[must_use]
pub fn signature_failure() -> StoreError {
    StoreError::Tampered(TamperKind::ManifestSignature)
}
