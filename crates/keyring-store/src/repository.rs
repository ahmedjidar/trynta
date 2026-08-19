//! Item and vault persistence: the meta/secret split in practice.
//!
//! [`split_draft`] is the function that enforces SPEC-V1 §3.4. Every secret
//! field is routed to `secret_ct` and every non-secret one to `meta_ct`, and
//! `tests/split.rs` asserts by sentinel scan that nothing crosses. If you are
//! adding a field to an item type, that function is where you decide which half
//! it belongs in — and getting it wrong means decrypting a secret for every item
//! at unlock.

use keyring_crypto::{
    derive_subkey, open, seal, Aad, Envelope, ItemSubkey, Key32, Muk, Purpose, Subkey,
    ENVELOPE_VERSION,
};
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::error::StoreError;
use crate::model::{
    CustomField, CustomFieldKind, IndexRow, ItemBody, ItemBodyMeta, ItemDraft, ItemMeta,
    ItemMetaPayload, ItemMetaPayloadPreIcon, ItemSecretPayload, ItemSummary, PasswordHistoryEntry,
    SecretField, StoredIcon, TotpConfig, TotpParams, VaultKind, VaultMetaPayload, VaultSummary,
    PASSWORD_HISTORY_LIMIT,
};

/// What an upsert did.
///
/// Crate-internal on purpose. The frozen acceptance contract
/// (`tests/acceptance/API.md`) pins `Session::item_upsert` to
/// `Result<Uuid, StoreError>`, so this never crosses that boundary — it exists
/// so [`crate::vault::Session::item_upsert`] can choose the right activity kind
/// without decrypting the secret half a second time to find out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UpsertOutcome {
    /// The item's id — freshly generated when `created`.
    pub(crate) id: Uuid,
    /// Whether this call created the item rather than updating one.
    pub(crate) created: bool,
    /// Whether the password field changed value.
    pub(crate) password_changed: bool,
}

fn uuid_bytes(id: Uuid) -> [u8; 16] {
    *id.as_bytes()
}

fn uuid_from(bytes: &[u8]) -> Result<Uuid, StoreError> {
    let raw: [u8; 16] = bytes.try_into().map_err(|_| StoreError::Database)?;
    Ok(Uuid::from_bytes(raw))
}

fn aad(purpose: Purpose, subject: [u8; 16], revision: u64, key_id: [u8; 16]) -> Aad {
    Aad {
        envelope_version: ENVELOPE_VERSION,
        purpose,
        subject_id: subject,
        revision,
        key_id,
    }
}

// ── Vaults ──────────────────────────────────────────────────────────────────

/// Number of live vaults.
pub(crate) fn vault_count(conn: &Connection) -> Result<i64, StoreError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM vaults WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

/// Create a vault: fresh key, wrapped under `muk.vault`; metadata sealed under
/// the vault key so it can travel with a V2 share (SPEC-V1 §4.2).
pub(crate) fn vault_insert(
    conn: &Connection,
    muk: &Muk,
    name: &str,
    color_token: &str,
    kind: VaultKind,
    now: i64,
) -> Result<Uuid, StoreError> {
    let id = Uuid::new_v4();
    let key_id = Uuid::new_v4();
    let vault_key = Key32::random()?;

    let wrap = derive_subkey(muk, Subkey::Vault);
    let wrap_aad = aad(Purpose::VaultMeta, uuid_bytes(id), 0, uuid_bytes(key_id));
    let key_wrap_ct = seal(&wrap, &wrap_aad, vault_key.expose())?.to_bytes();

    let payload = VaultMetaPayload {
        name: name.to_owned(),
        color_token: color_token.to_owned(),
        kind,
        created_at: now,
    };
    let encoded = postcard::to_stdvec(&payload).map_err(|_| StoreError::MalformedPayload)?;
    let meta_aad = aad(Purpose::VaultMeta, uuid_bytes(id), 0, uuid_bytes(key_id));
    let meta_ct = seal(&vault_key, &meta_aad, &encoded)?.to_bytes();

    conn.execute(
        "INSERT INTO vaults (id, key_id, key_wrap_ct, meta_ct, updated_at, deleted_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
        rusqlite::params![
            uuid_bytes(id).as_slice(),
            uuid_bytes(key_id).as_slice(),
            key_wrap_ct,
            meta_ct,
            now,
        ],
    )?;
    Ok(id)
}

/// Unwrap a vault key.
pub(crate) fn vault_key(conn: &Connection, muk: &Muk, vault_id: Uuid) -> Result<Key32, StoreError> {
    let (key_id, wrap_ct): (Vec<u8>, Vec<u8>) = conn
        .query_row(
            "SELECT key_id, key_wrap_ct FROM vaults WHERE id = ?1",
            [uuid_bytes(vault_id).as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(StoreError::VaultNotFound)?;

    let key_id: [u8; 16] = key_id
        .as_slice()
        .try_into()
        .map_err(|_| StoreError::Database)?;
    let wrap = derive_subkey(muk, Subkey::Vault);
    let envelope = Envelope::from_bytes(&wrap_ct)?;
    let opened = open(
        &wrap,
        &aad(Purpose::VaultMeta, uuid_bytes(vault_id), 0, key_id),
        &envelope,
    )?;
    let raw: [u8; 32] = opened
        .as_slice()
        .try_into()
        .map_err(|_| StoreError::MalformedPayload)?;
    Ok(Key32::from_bytes(raw))
}

/// List live vaults with their live item counts.
pub(crate) fn vaults_list(conn: &Connection, muk: &Muk) -> Result<Vec<VaultSummary>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, key_id, meta_ct FROM vaults WHERE deleted_at IS NULL ORDER BY updated_at",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (id_bytes, key_id, meta_ct) = row?;
        let id = uuid_from(&id_bytes)?;
        let key_id: [u8; 16] = key_id
            .as_slice()
            .try_into()
            .map_err(|_| StoreError::Database)?;

        let key = vault_key(conn, muk, id)?;
        let envelope = Envelope::from_bytes(&meta_ct)?;
        let opened = open(
            &key,
            &aad(Purpose::VaultMeta, uuid_bytes(id), 0, key_id),
            &envelope,
        )?;
        let payload: VaultMetaPayload =
            postcard::from_bytes(&opened).map_err(|_| StoreError::MalformedPayload)?;

        let item_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM items WHERE vault_id = ?1 AND deleted_at IS NULL",
            [id_bytes.as_slice()],
            |r| r.get(0),
        )?;

        out.push(VaultSummary {
            id,
            name: payload.name,
            color_token: payload.color_token,
            kind: payload.kind,
            item_count: usize::try_from(item_count).unwrap_or(0),
        });
    }
    Ok(out)
}

/// The vault key that owns `item_id`, and that vault's `key_id`.
///
/// Activity rows are sealed under a derivation of the *vault* key rather than a
/// MUK subkey (SPEC-V1 §4.3), so writing one needs the owning vault resolved
/// from the item.
pub(crate) fn vault_key_of_item(
    conn: &Connection,
    muk: &Muk,
    item_id: Uuid,
) -> Result<(Key32, [u8; 16]), StoreError> {
    let vault_bytes: Vec<u8> = conn
        .query_row(
            "SELECT vault_id FROM items WHERE id = ?1",
            [uuid_bytes(item_id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;
    vault_key_and_id(conn, muk, uuid_from(&vault_bytes)?)
}

/// A vault's unwrapped key together with its `key_id`.
///
/// Soft-deleted vaults are included on purpose: their rows survive the 30-day
/// purge window, and their items' envelopes still name them.
pub(crate) fn vault_key_and_id(
    conn: &Connection,
    muk: &Muk,
    vault_id: Uuid,
) -> Result<(Key32, [u8; 16]), StoreError> {
    let key_id: Vec<u8> = conn
        .query_row(
            "SELECT key_id FROM vaults WHERE id = ?1",
            [uuid_bytes(vault_id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::VaultNotFound)?;
    let key_id: [u8; 16] = key_id
        .as_slice()
        .try_into()
        .map_err(|_| StoreError::Database)?;

    Ok((vault_key(conn, muk, vault_id)?, key_id))
}

/// Read a vault's decrypted metadata, for an edit that rewrites it.
fn vault_meta(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
) -> Result<(VaultMetaPayload, Key32, [u8; 16]), StoreError> {
    let key_id: Vec<u8> = conn
        .query_row(
            "SELECT key_id FROM vaults WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::VaultNotFound)?;
    let key_id: [u8; 16] = key_id
        .as_slice()
        .try_into()
        .map_err(|_| StoreError::Database)?;

    let meta_ct: Vec<u8> = conn.query_row(
        "SELECT meta_ct FROM vaults WHERE id = ?1",
        [uuid_bytes(id).as_slice()],
        |r| r.get(0),
    )?;

    let key = vault_key(conn, muk, id)?;
    let envelope = Envelope::from_bytes(&meta_ct)?;
    let opened = open(
        &key,
        &aad(Purpose::VaultMeta, uuid_bytes(id), 0, key_id),
        &envelope,
    )?;
    let payload: VaultMetaPayload =
        postcard::from_bytes(&opened).map_err(|_| StoreError::MalformedPayload)?;
    Ok((payload, key, key_id))
}

/// Re-seal a vault's metadata after an edit.
///
/// The AAD revision stays 0 for the vault envelope's whole life — a vault has no
/// revision column, so there is nothing for it to track, and inventing one would
/// mean the manifest and the envelope disagreed about what a vault edit is.
fn vault_write_meta(
    conn: &Connection,
    id: Uuid,
    key: &Key32,
    key_id: [u8; 16],
    payload: &VaultMetaPayload,
    now: i64,
) -> Result<(), StoreError> {
    let encoded = postcard::to_stdvec(payload).map_err(|_| StoreError::MalformedPayload)?;
    let meta_ct = seal(
        key,
        &aad(Purpose::VaultMeta, uuid_bytes(id), 0, key_id),
        &encoded,
    )?
    .to_bytes();

    conn.execute(
        "UPDATE vaults SET meta_ct = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![meta_ct, now, uuid_bytes(id).as_slice()],
    )?;
    Ok(())
}

/// Rename a vault.
pub(crate) fn vault_rename(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
    name: &str,
    now: i64,
) -> Result<(), StoreError> {
    let (mut payload, key, key_id) = vault_meta(conn, muk, id)?;
    name.clone_into(&mut payload.name);
    vault_write_meta(conn, id, &key, key_id, &payload, now)
}

/// Change a vault's colour token.
pub(crate) fn vault_set_color(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
    color_token: &str,
    now: i64,
) -> Result<(), StoreError> {
    let (mut payload, key, key_id) = vault_meta(conn, muk, id)?;
    color_token.clone_into(&mut payload.color_token);
    vault_write_meta(conn, id, &key, key_id, &payload, now)
}

/// Move one item into another vault.
///
/// Two things are sealed under the *vault* key rather than the item key, and a
/// move has to carry both across:
///
/// - **the item key** (`item_key_ct`). The AAD is unchanged, because it names the
///   item and its own `key_id` and never the vault, so the item's two payload
///   envelopes stay valid and no secret is decrypted to move it.
/// - **every activity row** (SPEC-V1 §4.3 seals them under the vault's activity
///   subkey). Missing this leaves the history sealed under a key the item no
///   longer resolves to, and it does not fail at move time — it fails the next
///   time anyone opens the item, which is the worst possible moment to discover
///   it.
fn item_move(
    conn: &Connection,
    muk: &Muk,
    item_id: Uuid,
    to_vault: Uuid,
    now: i64,
) -> Result<(), StoreError> {
    let (from_key, from_key_id) = vault_key_of_item(conn, muk, item_id)?;
    let (to_key, to_key_id) = vault_key_and_id(conn, muk, to_vault)?;

    let (item_key, key_id) = item_key(conn, muk, item_id)?;
    let key_ct = seal(
        &to_key,
        &aad(Purpose::ItemMeta, uuid_bytes(item_id), 0, key_id),
        item_key.expose(),
    )?
    .to_bytes();

    conn.execute(
        "UPDATE items SET vault_id = ?1, item_key_ct = ?2, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![
            uuid_bytes(to_vault).as_slice(),
            key_ct,
            now,
            uuid_bytes(item_id).as_slice(),
        ],
    )?;

    crate::activity::rewrap(conn, item_id, &from_key, from_key_id, &to_key, to_key_id)
}

/// Soft-delete a vault, either moving its live items or soft-deleting them too.
///
/// Refuses to remove the last live vault. An account with no vault has nowhere
/// to put an item, and recovering from that state means editing the database by
/// hand — which is not a thing a user can do (CLAUDE.md §9: no acceptable
/// data-loss bug).
pub(crate) fn vault_delete(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
    move_items_to: Option<Uuid>,
    now: i64,
) -> Result<(), StoreError> {
    let live: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM vaults WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    if live.is_none() {
        return Err(StoreError::VaultNotFound);
    }
    if vault_count(conn)? <= 1 {
        return Err(StoreError::LastVault);
    }

    match move_items_to {
        Some(target) => {
            if target == id {
                return Err(StoreError::VaultNotFound);
            }
            let target_live: Option<i64> = conn
                .query_row(
                    "SELECT 1 FROM vaults WHERE id = ?1 AND deleted_at IS NULL",
                    [uuid_bytes(target).as_slice()],
                    |r| r.get(0),
                )
                .optional()?;
            if target_live.is_none() {
                return Err(StoreError::VaultNotFound);
            }

            let mut stmt =
                conn.prepare("SELECT id FROM items WHERE vault_id = ?1 AND deleted_at IS NULL")?;
            let ids: Vec<Vec<u8>> = stmt
                .query_map([uuid_bytes(id).as_slice()], |r| r.get::<_, Vec<u8>>(0))?
                .collect::<Result<_, _>>()?;
            drop(stmt);

            for raw in ids {
                item_move(conn, muk, uuid_from(&raw)?, target, now)?;
            }
        }
        None => {
            conn.execute(
                "UPDATE items SET deleted_at = ?1 WHERE vault_id = ?2 AND deleted_at IS NULL",
                rusqlite::params![now, uuid_bytes(id).as_slice()],
            )?;
        }
    }

    conn.execute(
        "UPDATE vaults SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now, uuid_bytes(id).as_slice()],
    )?;
    Ok(())
}

// ── Items ───────────────────────────────────────────────────────────────────

/// Route custom fields into their two halves.
///
/// A `Hidden` field keeps its label in the metadata envelope and loses its
/// value: omitting it from the UI is not the same as keeping it out of the
/// envelope that is decrypted for every item at unlock.
fn split_custom_fields(fields: &[CustomField], secret: &mut ItemSecretPayload) -> Vec<CustomField> {
    let mut meta = Vec::with_capacity(fields.len());
    for field in fields {
        if field.kind == CustomFieldKind::Hidden {
            secret.hidden_custom.push(field.value.clone());
            meta.push(CustomField {
                label: field.label.clone(),
                value: String::new(),
                kind: CustomFieldKind::Hidden,
            });
        } else {
            meta.push(field.clone());
        }
    }
    meta
}

/// Project a body into its non-secret half, moving every secret into `secret`.
fn split_body(body: &ItemBody, secret: &mut ItemSecretPayload) -> ItemBodyMeta {
    match body {
        ItemBody::Login {
            username,
            password,
            urls,
            totp,
        } => {
            secret.password = Some(password.clone());
            // Both halves of the TOTP config, not just the seed. Keeping only
            // the seed is what made an item saved as SHA-256/8-digit come back
            // as SHA-1/6-digit and generate codes that never worked.
            secret.totp_secret = totp.as_ref().map(|t| t.secret.clone());
            secret.totp_params = totp.as_ref().map(TotpParams::from_config);
            ItemBodyMeta::Login {
                username: username.clone(),
                urls: urls.clone(),
                has_totp: totp.is_some(),
            }
        }
        ItemBody::SecureNote => ItemBodyMeta::SecureNote,
        ItemBody::Card {
            cardholder,
            number,
            expiry_month,
            expiry_year,
            cvv,
            pin,
            billing_address,
        } => {
            secret.card_number = Some(number.clone());
            secret.card_cvv = Some(cvv.clone());
            secret.card_pin = Some(pin.clone());
            ItemBodyMeta::Card {
                cardholder: cardholder.clone(),
                expiry_month: *expiry_month,
                expiry_year: *expiry_year,
                billing_address: billing_address.clone(),
                last4: last4_of(number),
            }
        }
        ItemBody::Identity {
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
        } => {
            secret.document_number = Some(document_number.clone());
            ItemBodyMeta::Identity {
                first_name: first_name.clone(),
                last_name: last_name.clone(),
                dob: dob.clone(),
                document_type: document_type.clone(),
                issuing_country: issuing_country.clone(),
                expiry: expiry.clone(),
                address: address.clone(),
                phone: phone.clone(),
                email: email.clone(),
            }
        }
    }
}

/// Split a draft into its two payloads.
///
/// **This is the enforcement point for SPEC-V1 §3.4.** Everything the item list
/// needs goes left; everything that must be fetched one field at a time goes
/// right. A field placed in the wrong half is a field decrypted for every item
/// at unlock, so a `Hidden` custom field's value is blanked in the metadata half
/// rather than merely omitted from the UI.
fn split_draft(
    draft: &ItemDraft,
    created_at: i64,
    previous_secret: Option<&ItemSecretPayload>,
) -> (ItemMetaPayload, ItemSecretPayload) {
    let mut secret = ItemSecretPayload::default();
    let meta_customs = split_custom_fields(&draft.custom_fields, &mut secret);
    let body = split_body(&draft.body, &mut secret);

    // Password history: carry the old value forward when the password changed.
    if let Some(previous) = previous_secret {
        secret
            .password_history
            .clone_from(&previous.password_history);
        if let (Some(old), Some(new)) = (&previous.password, &secret.password) {
            if old != new {
                secret.password_history.insert(
                    0,
                    PasswordHistoryEntry {
                        value: old.clone(),
                        changed_at: created_at,
                    },
                );
                secret.password_history.truncate(PASSWORD_HISTORY_LIMIT);
            }
        }
    }

    let meta = ItemMetaPayload {
        kind: draft.body.kind(),
        title: draft.title.clone(),
        notes: draft.notes.clone(),
        tags: draft.tags.clone(),
        favorite: draft.favorite,
        created_at,
        custom_fields: meta_customs,
        body,
        // A draft never carries one. Attaching an icon is its own operation, so a save
        // from a form that has no icon field cannot silently erase one — the same
        // reason `item_edit_meta` exists rather than routing edits through `upsert`.
        custom_icon: None,
    };
    (meta, secret)
}

/// Last four characters of a card number, when there are more than four.
///
/// Returns `None` for a short or non-numeric value rather than exposing the
/// whole thing: a four-character "number" would put the entire value in the
/// metadata envelope.
fn last4_of(number: &str) -> Option<String> {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    (digits.len() > 4).then(|| digits[digits.len() - 4..].to_owned())
}

/// The item key for `id`, plus its `key_id`.
pub(crate) fn item_key(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
) -> Result<(Key32, [u8; 16]), StoreError> {
    let (vault_id, key_id, key_ct): (Vec<u8>, Vec<u8>, Vec<u8>) = conn
        .query_row(
            "SELECT vault_id, item_key_id, item_key_ct FROM items WHERE id = ?1",
            [uuid_bytes(id).as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let vault_id = uuid_from(&vault_id)?;
    let key_id: [u8; 16] = key_id
        .as_slice()
        .try_into()
        .map_err(|_| StoreError::Database)?;
    let vkey = vault_key(conn, muk, vault_id)?;

    let envelope = Envelope::from_bytes(&key_ct)?;
    let opened = open(
        &vkey,
        &aad(Purpose::ItemMeta, uuid_bytes(id), 0, key_id),
        &envelope,
    )?;
    let raw: [u8; 32] = opened
        .as_slice()
        .try_into()
        .map_err(|_| StoreError::MalformedPayload)?;
    Ok((Key32::from_bytes(raw), key_id))
}

/// Create or update an item.
pub(crate) fn item_upsert(
    conn: &Connection,
    muk: &Muk,
    draft: &ItemDraft,
    now: i64,
) -> Result<UpsertOutcome, StoreError> {
    // The vault must exist, or a foreign-key failure would surface as a generic
    // database error.
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM vaults WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(draft.vault_id).as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(StoreError::VaultNotFound);
    }

    match draft.id {
        None => insert_item(conn, muk, draft, now),
        Some(id) => update_item(conn, muk, draft, id, now),
    }
}

fn insert_item(
    conn: &Connection,
    muk: &Muk,
    draft: &ItemDraft,
    now: i64,
) -> Result<UpsertOutcome, StoreError> {
    let id = Uuid::new_v4();
    let key_id = Uuid::new_v4();
    let item_key = Key32::random()?;

    let vkey = vault_key(conn, muk, draft.vault_id)?;
    let key_ct = seal(
        &vkey,
        &aad(Purpose::ItemMeta, uuid_bytes(id), 0, uuid_bytes(key_id)),
        item_key.expose(),
    )?
    .to_bytes();

    let (meta, secret) = split_draft(draft, now, None);
    let (meta_ct, secret_ct) = seal_payloads(&item_key, id, uuid_bytes(key_id), 1, &meta, &secret)?;

    conn.execute(
        "INSERT INTO items (id, vault_id, item_key_id, item_key_ct, meta_ct, secret_ct, \
         revision, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, NULL)",
        rusqlite::params![
            uuid_bytes(id).as_slice(),
            uuid_bytes(draft.vault_id).as_slice(),
            uuid_bytes(key_id).as_slice(),
            key_ct,
            meta_ct,
            secret_ct,
            now,
        ],
    )?;
    Ok(UpsertOutcome {
        id,
        created: true,
        // A first password is not a change. Reporting it as one would put a
        // `PasswordChanged` event on every new login, which makes the one event
        // the security report actually cares about worthless.
        password_changed: false,
    })
}

fn update_item(
    conn: &Connection,
    muk: &Muk,
    draft: &ItemDraft,
    id: Uuid,
    now: i64,
) -> Result<UpsertOutcome, StoreError> {
    let revision: i64 = conn
        .query_row(
            "SELECT revision FROM items WHERE id = ?1",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;
    let next = revision.saturating_add(1);

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let previous = read_secret(conn, &item_key, id, key_id, revision.unsigned_abs())?;
    let (meta, secret) = split_draft(draft, now, Some(&previous));
    // Compared before the payloads are sealed and both copies dropped. The
    // comparison is on values that are already in memory for the split; it adds
    // no decryption of its own.
    let password_changed = match (&previous.password, &secret.password) {
        (Some(old), Some(new)) => old != new,
        _ => false,
    };
    let (meta_ct, secret_ct) =
        seal_payloads(&item_key, id, key_id, next.unsigned_abs(), &meta, &secret)?;

    conn.execute(
        "UPDATE items SET meta_ct = ?1, secret_ct = ?2, revision = ?3, updated_at = ?4 \
         WHERE id = ?5",
        rusqlite::params![meta_ct, secret_ct, next, now, uuid_bytes(id).as_slice()],
    )?;
    Ok(UpsertOutcome {
        id,
        created: false,
        password_changed,
    })
}

/// Metadata-only edits to an existing item (SPEC-V1 §7.1, the detail pane's edit mode).
///
/// Every field here lives in `meta_ct`. The secret envelope is read and re-sealed
/// **unchanged**, which is the whole point: `item_upsert` rebuilds the secret half
/// from the draft, so a metadata edit routed through it with an empty password
/// field would wipe the password. This path cannot, because it never constructs a
/// secret payload — it carries the previous one across verbatim.
///
/// Both envelopes bind the revision in their AAD, so changing metadata means
/// re-sealing the secret half too. That is a re-seal, not a re-write of its
/// contents.
#[derive(Debug, Clone, Default)]
pub struct MetaEdits {
    /// New title, or `None` to leave it.
    pub title: Option<String>,
    /// New notes.
    pub notes: Option<String>,
    /// New tags.
    pub tags: Option<Vec<String>>,
    /// New username, for a login. Ignored for any other kind.
    pub username: Option<String>,
    /// New URL list, for a login. Ignored for any other kind.
    pub urls: Option<Vec<String>>,
}

impl MetaEdits {
    /// Whether anything would change. Used to avoid burning a revision on a no-op.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.notes.is_none()
            && self.tags.is_none()
            && self.username.is_none()
            && self.urls.is_none()
    }
}

/// Apply [`MetaEdits`] to an item, leaving every secret field untouched.
///
/// Returns whether anything was written.
///
/// # Errors
///
/// [`StoreError::ItemNotFound`] if the item is absent or deleted,
/// [`StoreError::Database`] on a query failure, [`StoreError::Crypto`] if an
/// envelope fails to open or seal.
pub(crate) fn item_edit_meta(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
    edits: &MetaEdits,
    now: i64,
) -> Result<bool, StoreError> {
    if edits.is_empty() {
        return Ok(false);
    }

    let revision: i64 = conn
        .query_row(
            "SELECT revision FROM items WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let mut meta = read_meta_payload(conn, &item_key, id, key_id, revision.unsigned_abs())?;

    if let Some(title) = &edits.title {
        meta.title.clone_from(title);
    }
    if let Some(notes) = &edits.notes {
        meta.notes.clone_from(notes);
    }
    if let Some(tags) = &edits.tags {
        meta.tags.clone_from(tags);
    }
    if let ItemBodyMeta::Login { username, urls, .. } = &mut meta.body {
        if let Some(next) = &edits.username {
            username.clone_from(next);
        }
        if let Some(next) = &edits.urls {
            urls.clone_from(next);
        }
    }

    // Read and carry forward. Nothing in this function can construct a secret
    // payload, so nothing in it can lose one.
    let secret = read_secret(conn, &item_key, id, key_id, revision.unsigned_abs())?;
    let next = revision.saturating_add(1);
    let (meta_ct, secret_ct) =
        seal_payloads(&item_key, id, key_id, next.unsigned_abs(), &meta, &secret)?;

    conn.execute(
        "UPDATE items SET meta_ct = ?1, secret_ct = ?2, revision = ?3, updated_at = ?4          WHERE id = ?5",
        rusqlite::params![meta_ct, secret_ct, next, now, uuid_bytes(id).as_slice()],
    )?;
    Ok(true)
}

/// Set or clear an item's favourite flag.
///
/// `favorite` lives in `meta_ct`, and both envelopes bind the revision in their
/// AAD, so changing it means re-sealing the secret half too. A no-op toggle
/// returns early rather than burning a revision: `revision` is what the manifest
/// uses to detect a rollback, and churning it on a UI click makes the signal
/// noisier for no gain.
pub(crate) fn item_set_favorite(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
    favorite: bool,
    now: i64,
) -> Result<bool, StoreError> {
    let revision: i64 = conn
        .query_row(
            "SELECT revision FROM items WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let mut meta = read_meta_payload(conn, &item_key, id, key_id, revision.unsigned_abs())?;
    if meta.favorite == favorite {
        return Ok(false);
    }
    meta.favorite = favorite;

    let secret = read_secret(conn, &item_key, id, key_id, revision.unsigned_abs())?;
    let next = revision.saturating_add(1);
    let (meta_ct, secret_ct) =
        seal_payloads(&item_key, id, key_id, next.unsigned_abs(), &meta, &secret)?;

    conn.execute(
        "UPDATE items SET meta_ct = ?1, secret_ct = ?2, revision = ?3, updated_at = ?4 \
         WHERE id = ?5",
        rusqlite::params![meta_ct, secret_ct, next, now, uuid_bytes(id).as_slice()],
    )?;
    Ok(true)
}

/// Seal both halves under their own subkeys of the item key.
fn seal_payloads(
    item_key: &Key32,
    id: Uuid,
    key_id: [u8; 16],
    revision: u64,
    meta: &ItemMetaPayload,
    secret: &ItemSecretPayload,
) -> Result<(Vec<u8>, Vec<u8>), StoreError> {
    let meta_key = keyring_crypto::derive_item_subkey(item_key, ItemSubkey::Meta);
    let secret_key = keyring_crypto::derive_item_subkey(item_key, ItemSubkey::Secret);

    let meta_bytes = postcard::to_stdvec(meta).map_err(|_| StoreError::MalformedPayload)?;
    // The secret encoding is a plaintext buffer of real secrets, so it is wiped
    // as soon as it has been sealed rather than left for the allocator.
    let secret_bytes =
        Zeroizing::new(postcard::to_stdvec(secret).map_err(|_| StoreError::MalformedPayload)?);

    let meta_ct = seal(
        &meta_key,
        &aad(Purpose::ItemMeta, uuid_bytes(id), revision, key_id),
        &meta_bytes,
    )?
    .to_bytes();
    let secret_ct = seal(
        &secret_key,
        &aad(Purpose::ItemSecret, uuid_bytes(id), revision, key_id),
        &secret_bytes,
    )?
    .to_bytes();

    Ok((meta_ct, secret_ct))
}

fn read_meta_payload(
    conn: &Connection,
    item_key: &Key32,
    id: Uuid,
    key_id: [u8; 16],
    revision: u64,
) -> Result<ItemMetaPayload, StoreError> {
    let meta_ct: Vec<u8> = conn
        .query_row(
            "SELECT meta_ct FROM items WHERE id = ?1",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let key = keyring_crypto::derive_item_subkey(item_key, ItemSubkey::Meta);
    let envelope = Envelope::from_bytes(&meta_ct)?;
    let opened = open(
        &key,
        &aad(Purpose::ItemMeta, uuid_bytes(id), revision, key_id),
        &envelope,
    )?;
    decode_meta(&opened)
}

/// Decode an item metadata payload, accepting both shapes the format has had.
///
/// See [`ItemMetaPayloadPreIcon`] for why the second attempt exists and why it cannot
/// be reached by a payload that decodes correctly as the current shape.
fn decode_meta(opened: &[u8]) -> Result<ItemMetaPayload, StoreError> {
    if let Ok(current) = postcard::from_bytes::<ItemMetaPayload>(opened) {
        return Ok(current);
    }
    postcard::from_bytes::<ItemMetaPayloadPreIcon>(opened)
        .map(Into::into)
        .map_err(|_| StoreError::MalformedPayload)
}

fn read_secret(
    conn: &Connection,
    item_key: &Key32,
    id: Uuid,
    key_id: [u8; 16],
    revision: u64,
) -> Result<ItemSecretPayload, StoreError> {
    let secret_ct: Vec<u8> = conn
        .query_row(
            "SELECT secret_ct FROM items WHERE id = ?1",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let key = keyring_crypto::derive_item_subkey(item_key, ItemSubkey::Secret);
    let envelope = Envelope::from_bytes(&secret_ct)?;
    let opened = open(
        &key,
        &aad(Purpose::ItemSecret, uuid_bytes(id), revision, key_id),
        &envelope,
    )?;
    postcard::from_bytes(&opened).map_err(|_| StoreError::MalformedPayload)
}

/// List live items. Decrypts `meta_ct` only — never `secret_ct`.
pub(crate) fn items_list(conn: &Connection, muk: &Muk) -> Result<Vec<ItemSummary>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, vault_id, revision, updated_at FROM items WHERE deleted_at IS NULL \
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (id_bytes, vault_bytes, revision, updated_at) = row?;
        let id = uuid_from(&id_bytes)?;
        let (item_key, key_id) = item_key(conn, muk, id)?;
        let meta = read_meta_payload(conn, &item_key, id, key_id, revision.unsigned_abs())?;

        let (subtitle, has_totp) = match &meta.body {
            ItemBodyMeta::Login {
                username, has_totp, ..
            } => (Some(username.clone()), *has_totp),
            ItemBodyMeta::SecureNote => (None, false),
            ItemBodyMeta::Card {
                cardholder, last4, ..
            } => (
                Some(
                    last4
                        .as_ref()
                        .map_or_else(|| cardholder.clone(), |l| format!("{cardholder} ···· {l}")),
                ),
                false,
            ),
            ItemBodyMeta::Identity {
                first_name,
                last_name,
                ..
            } => (
                Some(format!("{first_name} {last_name}").trim().to_owned()),
                false,
            ),
        };

        out.push(ItemSummary {
            id,
            vault_id: uuid_from(&vault_bytes)?,
            kind: meta.kind,
            title: meta.title,
            subtitle,
            has_totp,
            is_favorite: meta.favorite,
            revision: revision.unsigned_abs(),
            updated_at,
        });
    }
    Ok(out)
}

/// Build the in-memory index (SPEC-V1 §4.7).
///
/// Decrypts every live item's `meta_ct` exactly once. `secret_ct` is never
/// touched: unlocking a vault must not materialise every password, which is the
/// whole reason for the split in §3.4.
pub(crate) fn index_rows(conn: &Connection, muk: &Muk) -> Result<Vec<IndexRow>, StoreError> {
    let mut stmt = conn
        .prepare("SELECT id, vault_id, revision, updated_at FROM items WHERE deleted_at IS NULL")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (id_bytes, vault_bytes, revision, updated_at) = row?;
        let id = uuid_from(&id_bytes)?;
        let (item_key, key_id) = item_key(conn, muk, id)?;
        let meta = read_meta_payload(conn, &item_key, id, key_id, revision.unsigned_abs())?;

        let (username, urls, has_totp) = match &meta.body {
            ItemBodyMeta::Login {
                username,
                urls,
                has_totp,
            } => (Some(username.clone()), urls.clone(), *has_totp),
            _ => (None, Vec::new(), false),
        };

        out.push(IndexRow {
            id,
            vault_id: uuid_from(&vault_bytes)?,
            kind: meta.kind,
            title: meta.title.clone(),
            username,
            urls,
            tags: meta.tags.clone(),
            favorite: meta.favorite,
            has_totp,
            revision: revision.unsigned_abs(),
            created_at: meta.created_at,
            updated_at,
            subtitle: subtitle_for(&meta.body),
            has_custom_icon: meta.custom_icon.is_some(),
        });
    }
    Ok(out)
}

/// The type-appropriate one-line subtitle for a row.
fn subtitle_for(body: &ItemBodyMeta) -> Option<String> {
    match body {
        ItemBodyMeta::Login { username, .. } => Some(username.clone()),
        ItemBodyMeta::SecureNote => None,
        ItemBodyMeta::Card {
            cardholder, last4, ..
        } => Some(
            last4
                .as_ref()
                .map_or_else(|| cardholder.clone(), |l| format!("{cardholder} ···· {l}")),
        ),
        ItemBodyMeta::Identity {
            first_name,
            last_name,
            ..
        } => Some(format!("{first_name} {last_name}").trim().to_owned()),
    }
}

/// One item's decrypted metadata.
pub(crate) fn item_meta(conn: &Connection, muk: &Muk, id: Uuid) -> Result<ItemMeta, StoreError> {
    let (vault_bytes, revision): (Vec<u8>, i64) = conn
        .query_row(
            "SELECT vault_id, revision FROM items WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let meta = read_meta_payload(conn, &item_key, id, key_id, revision.unsigned_abs())?;

    Ok(ItemMeta {
        id,
        vault_id: uuid_from(&vault_bytes)?,
        kind: meta.kind,
        title: meta.title,
        notes: meta.notes,
        tags: meta.tags,
        favorite: meta.favorite,
        revision: revision.unsigned_abs(),
        created_at: meta.created_at,
        custom_fields: meta.custom_fields,
        body: meta.body,
        has_custom_icon: meta.custom_icon.is_some(),
    })
}

/// Read the user's icon for one item, if it has one.
///
/// Decrypts that item's `meta_ct` and nothing else. Separate from [`item_get`] because
/// the icon is up to 64 KB and the detail pane asks for it once per visible row, not
/// once per open.
///
/// # Errors
///
/// [`StoreError::ItemNotFound`] if the item is absent or deleted,
/// [`StoreError::Database`] on a query failure, [`StoreError::Crypto`] if the envelope
/// fails to open.
pub(crate) fn item_custom_icon(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
) -> Result<Option<StoredIcon>, StoreError> {
    let revision: i64 = conn
        .query_row(
            "SELECT revision FROM items WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let meta = read_meta_payload(conn, &item_key, id, key_id, revision.unsigned_abs())?;
    Ok(meta.custom_icon)
}

/// Attach or remove an item's icon.
///
/// Modelled on [`item_set_favorite`]: the value lives in `meta_ct`, both envelopes bind
/// the revision in their AAD, so the secret half is read and re-sealed unchanged. A
/// no-op returns early rather than burning a revision — `revision` is what the manifest
/// uses to detect a rollback, and churning it makes that signal noisier for no gain.
///
/// # Errors
///
/// [`StoreError::ItemNotFound`] if the item is absent or deleted,
/// [`StoreError::Database`] on a query failure, [`StoreError::Crypto`] if an envelope
/// fails to open or seal.
pub(crate) fn item_set_custom_icon(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
    icon: Option<StoredIcon>,
    now: i64,
) -> Result<bool, StoreError> {
    let revision: i64 = conn
        .query_row(
            "SELECT revision FROM items WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let mut meta = read_meta_payload(conn, &item_key, id, key_id, revision.unsigned_abs())?;
    if meta.custom_icon == icon {
        return Ok(false);
    }
    meta.custom_icon = icon;

    let secret = read_secret(conn, &item_key, id, key_id, revision.unsigned_abs())?;
    let next = revision.saturating_add(1);
    let (meta_ct, secret_ct) =
        seal_payloads(&item_key, id, key_id, next.unsigned_abs(), &meta, &secret)?;

    conn.execute(
        "UPDATE items SET meta_ct = ?1, secret_ct = ?2, revision = ?3, updated_at = ?4          WHERE id = ?5",
        rusqlite::params![meta_ct, secret_ct, next, now, uuid_bytes(id).as_slice()],
    )?;
    Ok(true)
}

/// Attach, replace or remove an item's TOTP configuration.
///
/// Split across both envelopes, because a TOTP configuration is: the seed and its
/// parameters go into `secret_ct`, and `has_totp` — which the list and the search
/// index read, and which is not a secret — lives in `meta_ct`. Writing one without
/// the other is how an item ends up with a badge and no code, or a code the list
/// does not know about.
///
/// Only logins carry one. A card with a TOTP configuration is not a thing the
/// model can express, so this reports `ItemNotFound` rather than silently writing
/// a seed no reader would ever look for.
pub(crate) fn item_set_totp(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
    totp: Option<TotpConfig>,
    now: i64,
) -> Result<bool, StoreError> {
    let revision: i64 = conn
        .query_row(
            "SELECT revision FROM items WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let mut meta = read_meta_payload(conn, &item_key, id, key_id, revision.unsigned_abs())?;
    let mut secret = read_secret(conn, &item_key, id, key_id, revision.unsigned_abs())?;

    let ItemBodyMeta::Login { has_totp, .. } = &mut meta.body else {
        return Err(StoreError::ItemNotFound);
    };

    let (next_secret, next_params) = match &totp {
        Some(config) => (
            Some(config.secret.clone()),
            Some(TotpParams::from_config(config)),
        ),
        None => (None, None),
    };
    if secret.totp_secret == next_secret && secret.totp_params == next_params {
        return Ok(false);
    }

    *has_totp = totp.is_some();
    secret.totp_secret = next_secret;
    secret.totp_params = next_params;

    let next = revision.saturating_add(1);
    let (meta_ct, secret_ct) =
        seal_payloads(&item_key, id, key_id, next.unsigned_abs(), &meta, &secret)?;

    conn.execute(
        "UPDATE items SET meta_ct = ?1, secret_ct = ?2, revision = ?3, updated_at = ?4 \
         WHERE id = ?5",
        rusqlite::params![meta_ct, secret_ct, next, now, uuid_bytes(id).as_slice()],
    )?;
    Ok(true)
}

/// Decrypt exactly one secret field.
pub(crate) fn item_secret(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
    field: SecretField,
) -> Result<Zeroizing<String>, StoreError> {
    let revision: i64 = conn
        .query_row(
            "SELECT revision FROM items WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let secret = read_secret(conn, &item_key, id, key_id, revision.unsigned_abs())?;

    // Copy out one field; `secret` zeroizes on drop at the end of this scope.
    let value = secret.field(field).ok_or(StoreError::NoSuchField)?;
    Ok(Zeroizing::new(value.to_owned()))
}

/// The item's full TOTP configuration, seed included.
///
/// One decrypt of `secret_ct` yields both halves, which is the reason the
/// parameters live there rather than in `meta_ct`. Returns `None` when the item
/// has no TOTP, and `None` for the params too if only a seed was stored — which
/// can only be a payload written before [`TotpParams`] existed.
pub(crate) fn item_totp(
    conn: &Connection,
    muk: &Muk,
    id: Uuid,
) -> Result<Option<TotpConfig>, StoreError> {
    let revision: i64 = conn
        .query_row(
            "SELECT revision FROM items WHERE id = ?1 AND deleted_at IS NULL",
            [uuid_bytes(id).as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::ItemNotFound)?;

    let (item_key, key_id) = item_key(conn, muk, id)?;
    let secret = read_secret(conn, &item_key, id, key_id, revision.unsigned_abs())?;

    let Some(seed) = secret.totp_secret.clone() else {
        return Ok(None);
    };
    // A seed with no parameters would mean guessing SHA-1/6/30 and handing back
    // codes that may be wrong. `None` is the honest answer: the caller shows the
    // user that the configuration is incomplete rather than a plausible code.
    let Some(params) = secret.totp_params.clone() else {
        return Ok(None);
    };
    Ok(Some(params.into_config(seed)))
}

/// Soft-delete an item.
pub(crate) fn item_delete(conn: &Connection, id: Uuid, now: i64) -> Result<(), StoreError> {
    let changed = conn.execute(
        "UPDATE items SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
        rusqlite::params![now, uuid_bytes(id).as_slice()],
    )?;
    if changed == 0 {
        return Err(StoreError::ItemNotFound);
    }
    Ok(())
}

/// Restore a soft-deleted item.
pub(crate) fn item_restore(conn: &Connection, id: Uuid, _now: i64) -> Result<(), StoreError> {
    let changed = conn.execute(
        "UPDATE items SET deleted_at = NULL WHERE id = ?1 AND deleted_at IS NOT NULL",
        [uuid_bytes(id).as_slice()],
    )?;
    if changed == 0 {
        return Err(StoreError::ItemNotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{decode_meta, ItemMetaPayload};
    use crate::model::{ItemBodyMeta, ItemKind};
    use serde::Serialize;

    /// The exact shape the writer emitted before `custom_icon` existed.
    ///
    /// Declared here with `Serialize` rather than reusing
    /// `ItemMetaPayloadPreIcon` — that type deliberately cannot be written, and a
    /// test that shared it would be asserting against its own encoder rather than
    /// against the bytes a shipped build actually produced.
    #[derive(Serialize)]
    struct PreIconWriter {
        kind: ItemKind,
        title: String,
        notes: String,
        tags: Vec<String>,
        favorite: bool,
        created_at: i64,
        custom_fields: Vec<crate::model::CustomField>,
        body: ItemBodyMeta,
    }

    fn body() -> ItemBodyMeta {
        ItemBodyMeta::Login {
            username: "someone@example.test".to_owned(),
            urls: vec!["https://example.test/login".to_owned()],
            has_totp: false,
        }
    }

    #[test]
    fn a_payload_written_before_the_icon_field_still_decodes() {
        let old = PreIconWriter {
            kind: ItemKind::Login,
            title: "Example".to_owned(),
            notes: "note".to_owned(),
            tags: vec!["personal".to_owned()],
            favorite: true,
            created_at: 1_700_000_000_000,
            custom_fields: Vec::new(),
            body: body(),
        };
        let bytes = postcard::to_stdvec(&old).expect("encode");

        let decoded = decode_meta(&bytes).expect("a pre-icon payload must still open");
        assert_eq!(decoded.title, "Example");
        assert_eq!(decoded.tags, vec!["personal".to_owned()]);
        assert!(decoded.favorite);
        assert_eq!(decoded.created_at, 1_700_000_000_000);
        assert_eq!(decoded.body, body());
        assert!(
            decoded.custom_icon.is_none(),
            "a vault written before the field existed has no icon, not a corrupt one"
        );
    }

    #[test]
    fn a_current_payload_round_trips_unchanged() {
        let current = ItemMetaPayload {
            kind: ItemKind::Login,
            title: "Example".to_owned(),
            notes: String::new(),
            tags: Vec::new(),
            favorite: false,
            created_at: 1,
            custom_fields: Vec::new(),
            body: body(),
            custom_icon: None,
        };
        let bytes = postcard::to_stdvec(&current).expect("encode");
        assert_eq!(decode_meta(&bytes).expect("decode"), current);
    }

    /// The pre-icon shape is a strict prefix of the current one, so the fallback must
    /// never be what accepts a payload that the current shape already read correctly.
    #[test]
    fn an_attached_icon_survives_the_two_step_decode() {
        let with_icon = ItemMetaPayload {
            kind: ItemKind::Login,
            title: "Example".to_owned(),
            notes: String::new(),
            tags: Vec::new(),
            favorite: false,
            created_at: 1,
            custom_fields: Vec::new(),
            body: body(),
            custom_icon: Some(crate::model::StoredIcon {
                format: crate::model::IconFormat::Png,
                bytes: vec![1, 2, 3, 4],
            }),
        };
        let bytes = postcard::to_stdvec(&with_icon).expect("encode");
        let decoded = decode_meta(&bytes).expect("decode");
        assert_eq!(decoded, with_icon);
        assert!(
            decoded.custom_icon.is_some(),
            "the icon must not be dropped"
        );
    }

    #[test]
    fn genuine_rubbish_is_still_rejected() {
        assert!(decode_meta(&[]).is_err());
        assert!(decode_meta(&[0xff; 8]).is_err());
    }
}
