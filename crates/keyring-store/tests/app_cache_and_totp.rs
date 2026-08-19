// SPDX-License-Identifier: AGPL-3.0-or-later
//! The encrypted key/value table, and the TOTP configuration round trip.
//!
//! Two things, both about data surviving a write:
//!
//! **`app_cache` (SPEC-V1 §4.4).** Settings, generator history and the HIBP
//! prefix cache all need somewhere encrypted to live. §7.4 is explicit about why
//! the last one cannot be plaintext: *"a plaintext cache of your password hash
//! prefixes is a filter that massively narrows an offline attack."*
//!
//! **TOTP parameters.** The store used to keep only `TotpConfig::secret` and drop
//! the algorithm, digits, period, issuer and account. An item saved as SHA-256 at
//! 8 digits on a 60-second step came back as SHA-1 at 6 digits on 30 seconds and
//! generated codes that never worked, with nothing anywhere reporting a failure.
//! The round-trip test below is the one that would have caught it.

use keyring_store::{
    AppCacheKey, ItemBody, ItemDraft, KdfParams, SecretField, TotpAlgorithm, TotpConfig, VaultFile,
};
use rusqlite::Connection;

const MASTER: &str = "app-cache-test-master-5Tn8Qr";

fn vault() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    (dir, path)
}

// ── TOTP: the whole configuration survives, not just the seed ───────────────

/// Deliberately every field different from the defaults, so a dropped field shows
/// up as a wrong value rather than coincidentally matching.
fn non_default_totp() -> TotpConfig {
    TotpConfig {
        secret: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_owned(),
        algorithm: TotpAlgorithm::Sha256,
        digits: 8,
        period_seconds: 60,
        issuer: "Example Issuer".to_owned(),
        account: "alice+keyring@example.test".to_owned(),
    }
}

#[test]
fn a_sha256_eight_digit_sixty_second_totp_survives_save_and_load() {
    let (_guard, path) = vault();
    let id = {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault");

        session
            .item_upsert(&ItemDraft::new(
                vault_id,
                "a login with a non-default TOTP",
                ItemBody::Login {
                    username: "alice@example.test".to_owned(),
                    password: "fixture-password-Qb4".to_owned(),
                    urls: vec![],
                    totp: Some(non_default_totp()),
                },
            ))
            .expect("upsert")
    };

    // Reopened and re-unlocked, so this reads what is on disk rather than
    // anything still in memory from the write.
    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("re-unlock");

    let loaded = session
        .item_totp(id)
        .expect("read totp")
        .expect("the item has a TOTP configuration");

    assert_eq!(
        loaded,
        non_default_totp(),
        "the stored TOTP configuration does not match what was saved — a dropped \
         parameter here produces codes that look right and never work"
    );

    // Field by field, so a failure names the field rather than the struct.
    assert_eq!(loaded.algorithm, TotpAlgorithm::Sha256, "algorithm");
    assert_eq!(loaded.digits, 8, "digits");
    assert_eq!(loaded.period_seconds, 60, "period_seconds");
    assert_eq!(loaded.issuer, "Example Issuer", "issuer");
    assert_eq!(loaded.account, "alice+keyring@example.test", "account");
    assert_eq!(loaded.secret, non_default_totp().secret, "secret");
}

#[test]
fn the_totp_seed_is_still_reachable_as_a_secret_field() {
    // Splitting the config must not move the seed out from under
    // `SecretField::TotpSecret`, which is how reveal and copy address it.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(
            vault_id,
            "a login",
            ItemBody::Login {
                username: "alice@example.test".to_owned(),
                password: "fixture-password-Qb4".to_owned(),
                urls: vec![],
                totp: Some(non_default_totp()),
            },
        ))
        .expect("upsert");

    assert_eq!(
        &*session
            .item_secret(id, SecretField::TotpSecret)
            .expect("seed"),
        &non_default_totp().secret
    );
}

#[test]
fn an_item_without_a_totp_reports_none() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(vault_id, "a note", ItemBody::SecureNote))
        .expect("upsert");

    assert_eq!(session.item_totp(id).expect("read totp"), None);
}

#[test]
fn the_totp_seed_is_not_in_the_metadata_envelope() {
    // The parameters moved into secret_ct beside the seed. That must not have
    // pulled the seed into meta_ct, which is decrypted for every item at unlock.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");

    let id = session
        .item_upsert(&ItemDraft::new(
            vault_id,
            "a login",
            ItemBody::Login {
                username: "alice@example.test".to_owned(),
                password: "fixture-password-Qb4".to_owned(),
                urls: vec![],
                totp: Some(non_default_totp()),
            },
        ))
        .expect("upsert");

    let meta = session.item_meta(id).expect("meta");
    let rendered = format!("{meta:?}");
    assert!(
        !rendered.contains(&non_default_totp().secret),
        "the TOTP seed reached the metadata envelope: {rendered}"
    );
    assert!(
        !rendered.contains("fixture-password"),
        "the password reached the metadata envelope: {rendered}"
    );
}

// ── app_cache ───────────────────────────────────────────────────────────────

#[test]
fn a_namespace_round_trips_through_the_encrypted_table() {
    let (_guard, path) = vault();
    let payload = b"generator-history-fixture-bytes".to_vec();

    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        assert_eq!(
            session
                .app_cache_get(AppCacheKey::GeneratorHistory)
                .expect("read absent"),
            None,
            "an unwritten namespace reads as absent"
        );
        session
            .app_cache_put(AppCacheKey::GeneratorHistory, &payload)
            .expect("write");
    }

    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("re-unlock");
    assert_eq!(
        session
            .app_cache_get(AppCacheKey::GeneratorHistory)
            .expect("read")
            .map(|v| v.to_vec()),
        Some(payload)
    );
}

#[test]
fn the_stored_payload_is_not_plaintext_on_disk() {
    // The reason this table is encrypted at all (§7.4). Read with rusqlite
    // directly, so this is what an attacker with the file would see.
    let (_guard, path) = vault();
    let sentinel = b"APPCACHE-SENTINEL-Vd72Kq";

    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .app_cache_put(AppCacheKey::BreachCache, sentinel)
            .expect("write");
    }

    let conn = Connection::open(&path).expect("open");
    let stored: Vec<u8> = conn
        .query_row(
            "SELECT payload_ct FROM app_cache WHERE key = 'breach_cache'",
            [],
            |r| r.get(0),
        )
        .expect("row");

    assert!(
        !stored
            .windows(sentinel.len())
            .any(|w| w == sentinel.as_slice()),
        "the app_cache payload is on disk in the clear"
    );
}

#[test]
fn each_namespace_is_independent() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    for (index, key) in AppCacheKey::all().into_iter().enumerate() {
        session
            .app_cache_put(key, format!("payload-{index}").as_bytes())
            .expect("write");
    }
    for (index, key) in AppCacheKey::all().into_iter().enumerate() {
        assert_eq!(
            session
                .app_cache_get(key)
                .expect("read")
                .map(|v| String::from_utf8_lossy(&v).into_owned()),
            Some(format!("payload-{index}")),
            "{key:?} read back another namespace's payload"
        );
    }
}

#[test]
fn a_ciphertext_moved_between_namespaces_does_not_authenticate() {
    // Every row shares the same key, purpose and key_id, so without the
    // per-namespace `subject_id` in the AAD an attacker who can write the file
    // could swap the breach cache into the settings row and have it decrypt.
    let (_guard, path) = vault();
    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .app_cache_put(AppCacheKey::BreachCache, b"breach-bytes")
            .expect("write breach");
        session
            .app_cache_put(AppCacheKey::Settings, b"settings-bytes")
            .expect("write settings");
    }

    // The attacker's move: copy one row's ciphertext over another's.
    {
        let conn = Connection::open(&path).expect("open");
        let breach: Vec<u8> = conn
            .query_row(
                "SELECT payload_ct FROM app_cache WHERE key = 'breach_cache'",
                [],
                |r| r.get(0),
            )
            .expect("read breach");
        conn.execute(
            "UPDATE app_cache SET payload_ct = ?1 WHERE key = 'settings'",
            [breach],
        )
        .expect("swap");
    }

    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("re-unlock");
    assert!(
        session.app_cache_get(AppCacheKey::Settings).is_err(),
        "a ciphertext from another namespace authenticated as settings"
    );
    // The untouched row still opens: the swap is detected, not the whole table
    // condemned.
    assert_eq!(
        session
            .app_cache_get(AppCacheKey::BreachCache)
            .expect("read breach")
            .map(|v| v.to_vec()),
        Some(b"breach-bytes".to_vec())
    );
}

#[test]
fn a_tampered_row_is_an_error_and_not_an_absence() {
    // "Never written" and "tampered with" must not look the same to a caller: one
    // is normal, the other means the file has been modified.
    let (_guard, path) = vault();
    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .app_cache_put(AppCacheKey::Settings, b"settings-bytes")
            .expect("write");
    }
    {
        let conn = Connection::open(&path).expect("open");
        let mut stored: Vec<u8> = conn
            .query_row(
                "SELECT payload_ct FROM app_cache WHERE key = 'settings'",
                [],
                |r| r.get(0),
            )
            .expect("read");
        let last = stored.len() - 1;
        stored[last] ^= 0xff;
        conn.execute(
            "UPDATE app_cache SET payload_ct = ?1 WHERE key = 'settings'",
            [stored],
        )
        .expect("tamper");
    }

    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("re-unlock");
    assert!(
        session.app_cache_get(AppCacheKey::Settings).is_err(),
        "a bit-flipped payload decrypted"
    );
}

#[test]
fn writing_twice_replaces_rather_than_duplicating() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    session
        .app_cache_put(AppCacheKey::Settings, b"first")
        .expect("first");
    session
        .app_cache_put(AppCacheKey::Settings, b"second")
        .expect("second");

    assert_eq!(
        session
            .app_cache_get(AppCacheKey::Settings)
            .expect("read")
            .map(|v| v.to_vec()),
        Some(b"second".to_vec())
    );

    let conn = Connection::open(&path).expect("open");
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM app_cache", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rows, 1, "the table grew instead of being updated in place");
}

#[test]
fn clearing_removes_a_namespace() {
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    session
        .app_cache_put(AppCacheKey::GeneratorHistory, b"history")
        .expect("write");
    session
        .app_cache_clear(AppCacheKey::GeneratorHistory)
        .expect("clear");
    assert_eq!(
        session
            .app_cache_get(AppCacheKey::GeneratorHistory)
            .expect("read"),
        None
    );
    // Clearing an absent namespace is success, because callers clear defensively.
    session
        .app_cache_clear(AppCacheKey::GeneratorHistory)
        .expect("clear again");
}

#[test]
fn updated_at_is_recorded_for_the_breach_cadence() {
    // §7.4 needs "at most once per 24 h", which needs a last-written time.
    let (_guard, path) = vault();
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");

    assert_eq!(
        session
            .app_cache_updated_at(AppCacheKey::BreachCache)
            .expect("absent"),
        None
    );
    session
        .app_cache_put(AppCacheKey::BreachCache, b"cache")
        .expect("write");
    let at = session
        .app_cache_updated_at(AppCacheKey::BreachCache)
        .expect("read")
        .expect("written");
    assert!(at > 1_600_000_000_000, "implausible timestamp: {at}");
}

#[test]
fn app_cache_is_unreadable_without_the_master_password() {
    // The table is sealed under muk.appcache, so it is exactly as unreachable as
    // the rest of the vault before unlock. Asserted through the public API: there
    // is no way to reach a namespace without a Session.
    let (_guard, path) = vault();
    {
        let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .app_cache_put(AppCacheKey::Settings, b"settings-bytes")
            .expect("write");
    }

    let file = VaultFile::open(&path).expect("reopen");
    assert!(
        file.unlock("the-wrong-password").is_err(),
        "the wrong password must not produce a session"
    );
}
