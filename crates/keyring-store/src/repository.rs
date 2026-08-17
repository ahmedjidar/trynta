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
    CustomField, CustomFieldKind, ItemBody, ItemBodyMeta, ItemDraft, ItemMeta, ItemMetaPayload,
    ItemSecretPayload, ItemSummary, PasswordHistoryEntry, SecretField, VaultKind, VaultMetaPayload,
    VaultSummary, PASSWORD_HISTORY_LIMIT,
};

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
fn vault_key(conn: &Connection, muk: &Muk, vault_id: Uuid) -> Result<Key32, StoreError> {
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
            secret.totp_secret = totp.as_ref().map(|t| t.secret.clone());
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
) -> Result<Uuid, StoreError> {
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
) -> Result<Uuid, StoreError> {
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
    Ok(id)
}

fn update_item(
    conn: &Connection,
    muk: &Muk,
    draft: &ItemDraft,
    id: Uuid,
    now: i64,
) -> Result<Uuid, StoreError> {
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
    let (meta_ct, secret_ct) =
        seal_payloads(&item_key, id, key_id, next.unsigned_abs(), &meta, &secret)?;

    conn.execute(
        "UPDATE items SET meta_ct = ?1, secret_ct = ?2, revision = ?3, updated_at = ?4 \
         WHERE id = ?5",
        rusqlite::params![meta_ct, secret_ct, next, now, uuid_bytes(id).as_slice()],
    )?;
    Ok(id)
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
    postcard::from_bytes(&opened).map_err(|_| StoreError::MalformedPayload)
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
    })
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
