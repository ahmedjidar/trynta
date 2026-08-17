//! Per-item activity — its own table, never the item payload (SPEC-V1 §4.3).
//!
//! Rev 1 of the spec put activity inside the item payload, and §4.3 records why
//! that was wrong. Every reveal would have been a payload rewrite: it would bump
//! `revision` — which is in the AAD — churn `updated_at`, turn "recently
//! updated" into "recently looked at", break the copy path on a read-only
//! database, and in V3 ship a full item update over sync every time somebody
//! copied a password.
//!
//! So an activity write touches exactly one table, and **never** the `items`
//! row. That property is what AC10 asserts, and [`record`] is deliberately the
//! only function here that writes.
//!
//! ## What is encrypted, and under what
//!
//! The payload is sealed under the owning vault's activity subkey — a vault-key
//! derivation rather than a MUK one, so in V2 an activity record travels with
//! the share it belongs to instead of being readable only by the account that
//! wrote it.
//!
//! `id`, `item_id` and `at` are plaintext columns, which §4.4 already documents
//! as leaked: an attacker with the file learns *that* an item was touched and
//! when, but not what happened to it.

use keyring_crypto::{derive_activity_subkey, open, seal, Aad, Envelope, Muk, Purpose};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::StoreError;
use crate::repository;

/// Events retained per item (SPEC-V1 §4.3).
pub const ACTIVITY_LIMIT: usize = 100;

/// What happened to an item (SPEC-V1 §4.3).
///
/// The discriminants are an on-disk format, so the two SPEC-V2 variants are
/// reserved here rather than appended later: V1 never writes them, and leaving
/// the space open now means sharing does not have to renumber a format that
/// already exists in users' vaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityKind {
    /// The item was created.
    Created,
    /// A non-secret field changed.
    Updated,
    /// The password changed. Distinct from [`ActivityKind::Updated`] because it
    /// is the one the security report and the user both care about.
    PasswordChanged,
    /// The item was filled into a form. SPEC-V3; never written in V1.
    Autofilled,
    /// A secret field was shown in the UI.
    Revealed,
    /// A secret field was copied to the clipboard.
    Copied,
    /// The item was shared. SPEC-V2; never written in V1.
    Shared,
    /// A share was revoked. SPEC-V2; never written in V1.
    ShareRevoked,
}

/// The encrypted half of an activity row.
///
/// Only the kind, deliberately. A reveal event that also recorded *which* field
/// was revealed would build, inside the vault, a log of which of a user's
/// secrets are the interesting ones — and §4.3 asks for a kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct ActivityPayload {
    kind: ActivityKind,
}

/// One decrypted activity record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityEvent {
    /// Event id.
    pub id: Uuid,
    /// The item it belongs to.
    pub item_id: Uuid,
    /// When it happened, Unix milliseconds.
    pub at: i64,
    /// What happened.
    pub kind: ActivityKind,
}

fn aad(item_id: Uuid, key_id: [u8; 16]) -> Aad {
    Aad {
        envelope_version: keyring_crypto::ENVELOPE_VERSION,
        purpose: Purpose::Activity,
        subject_id: *item_id.as_bytes(),
        // Activity rows are append-only and never revised, so the AAD revision
        // is fixed. It is the *item's* revision that moves, and binding an
        // activity row to it would make every event unreadable after the next
        // edit.
        revision: 0,
        key_id,
    }
}

/// Append one event for `item_id` and evict anything past [`ACTIVITY_LIMIT`].
///
/// Writes to `activity` and nothing else. In particular it does not touch the
/// `items` row, which is what makes a reveal free of `revision` and
/// `updated_at` churn (SPEC-V1 §4.3, AC10).
///
/// # Errors
///
/// [`StoreError::ItemNotFound`] if the item does not exist,
/// [`StoreError::Database`] or [`StoreError::Crypto`] otherwise.
pub(crate) fn record(
    conn: &Connection,
    muk: &Muk,
    item_id: Uuid,
    kind: ActivityKind,
    now: i64,
) -> Result<Uuid, StoreError> {
    let (vault_key, key_id) = repository::vault_key_of_item(conn, muk, item_id)?;
    let activity_key = derive_activity_subkey(&vault_key);

    let encoded =
        postcard::to_stdvec(&ActivityPayload { kind }).map_err(|_| StoreError::MalformedPayload)?;
    let payload_ct = seal(&activity_key, &aad(item_id, key_id), &encoded)?.to_bytes();

    let id = Uuid::new_v4();
    conn.execute(
        "INSERT INTO activity (id, item_id, at, payload_ct) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            id.as_bytes().as_slice(),
            item_id.as_bytes().as_slice(),
            now,
            payload_ct,
        ],
    )?;

    evict(conn, item_id)?;
    Ok(id)
}

/// Drop the oldest rows for `item_id` beyond the cap.
///
/// Eviction runs on write rather than on read: copies and reveals are the most
/// frequent events in the product, and a table that only trims when someone
/// happens to open the activity panel is a table that grows without bound.
fn evict(conn: &Connection, item_id: Uuid) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM activity WHERE item_id = ?1 AND id NOT IN (\
             SELECT id FROM activity WHERE item_id = ?1 ORDER BY at DESC, rowid DESC LIMIT ?2\
         )",
        rusqlite::params![
            item_id.as_bytes().as_slice(),
            i64::try_from(ACTIVITY_LIMIT).unwrap_or(i64::MAX),
        ],
    )?;
    Ok(())
}

/// The most recent events for `item_id`, newest first.
///
/// `limit` is clamped to [`ACTIVITY_LIMIT`]: a caller asking for more than the
/// table retains is asking for rows that do not exist, and letting an IPC
/// parameter size an allocation is how a list command becomes a memory
/// exhaustion bug.
///
/// # Errors
///
/// [`StoreError::ItemNotFound`] if the item does not exist,
/// [`StoreError::Database`] or [`StoreError::Crypto`] otherwise.
pub(crate) fn list(
    conn: &Connection,
    muk: &Muk,
    item_id: Uuid,
    limit: usize,
) -> Result<Vec<ActivityEvent>, StoreError> {
    let (vault_key, key_id) = repository::vault_key_of_item(conn, muk, item_id)?;
    let activity_key = derive_activity_subkey(&vault_key);
    let limit = i64::try_from(limit.min(ACTIVITY_LIMIT)).unwrap_or(0);

    let mut stmt = conn.prepare(
        "SELECT id, at, payload_ct FROM activity WHERE item_id = ?1 \
         ORDER BY at DESC, rowid DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![item_id.as_bytes().as_slice(), limit],
        |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        },
    )?;

    let mut out = Vec::new();
    for row in rows {
        let (id_bytes, at, payload_ct) = row?;
        let raw: [u8; 16] = id_bytes
            .as_slice()
            .try_into()
            .map_err(|_| StoreError::Database)?;

        let envelope = Envelope::from_bytes(&payload_ct)?;
        let opened = open(&activity_key, &aad(item_id, key_id), &envelope)?;
        let payload: ActivityPayload =
            postcard::from_bytes(&opened).map_err(|_| StoreError::MalformedPayload)?;

        out.push(ActivityEvent {
            id: Uuid::from_bytes(raw),
            item_id,
            at,
            kind: payload.kind,
        });
    }
    Ok(out)
}

/// Re-seal every activity row for `item_id` under a different vault's key.
///
/// Called when an item moves between vaults. The rows are sealed under the
/// owning vault's activity subkey (SPEC-V1 §4.3), so a move that did not do this
/// would leave a history that decrypts under a key the item no longer resolves
/// to — and the failure would surface on the next read rather than on the move.
///
/// The AAD is unchanged apart from `key_id`: `subject_id` is the item, which did
/// not move, and `revision` is fixed at 0 for activity.
pub(crate) fn rewrap(
    conn: &Connection,
    item_id: Uuid,
    from_key: &keyring_crypto::Key32,
    from_key_id: [u8; 16],
    to_key: &keyring_crypto::Key32,
    to_key_id: [u8; 16],
) -> Result<(), StoreError> {
    if from_key_id == to_key_id {
        return Ok(());
    }
    let from = derive_activity_subkey(from_key);
    let to = derive_activity_subkey(to_key);

    let mut stmt = conn.prepare("SELECT id, payload_ct FROM activity WHERE item_id = ?1")?;
    let rows: Vec<(Vec<u8>, Vec<u8>)> = stmt
        .query_map([item_id.as_bytes().as_slice()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    drop(stmt);

    for (id, payload_ct) in rows {
        let envelope = Envelope::from_bytes(&payload_ct)?;
        let opened = open(&from, &aad(item_id, from_key_id), &envelope)?;
        let resealed = seal(&to, &aad(item_id, to_key_id), &opened)?.to_bytes();
        conn.execute(
            "UPDATE activity SET payload_ct = ?1 WHERE id = ?2",
            rusqlite::params![resealed, id],
        )?;
    }
    Ok(())
}

/// Delete every activity row for `item_id`.
///
/// SPEC-V1 §7.5 offers "clear activity" as a privacy action, and a purge of an
/// item's rows is the same operation scoped to one item.
///
/// # Errors
///
/// [`StoreError::Database`] if the delete fails.
pub(crate) fn clear(conn: &Connection, item_id: Option<Uuid>) -> Result<usize, StoreError> {
    let deleted = match item_id {
        Some(id) => conn.execute(
            "DELETE FROM activity WHERE item_id = ?1",
            [id.as_bytes().as_slice()],
        )?,
        None => conn.execute("DELETE FROM activity", [])?,
    };
    Ok(deleted)
}
