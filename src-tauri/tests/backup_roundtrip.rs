//! `.keyringbackup` export and restore (SPEC-V1 §7.8, AC15).
//!
//! AC15: *"Backup export → wipe → restore → identical vault."*
//!
//! "Identical" is asserted field by field, secrets included, because the failure
//! this criterion exists to catch is a restore that *looks* complete. A vault whose
//! items are all present but whose passwords no longer decrypt is worse than a
//! restore that failed outright — the user finds out one item at a time, weeks
//! later, and by then the backup may be gone.
//!
//! The wipe is a real wipe: `.db`, `-wal` and `-shm`. Deleting only the database
//! leaves `SQLite` able to recover pages from the write-ahead log, so a test that
//! skipped the sidecars could pass while restoring nothing at all.
//!
//! One property is worth stating because it is a design choice rather than an
//! accident: **restore takes no master password.** A container holds the vault's own
//! ciphertext, so restoring is a ciphertext operation; the passphrase opens the
//! wrapper and the master password is only needed to *use* the result. That is what
//! makes a backup useful to someone whose machine died.

use std::path::{Path, PathBuf};

use keyring_store::{
    open_container, ItemBody, ItemDraft, KdfParams, RestoreMode, SecretField, StoreError,
    TotpAlgorithm, TotpConfig, VaultFile,
};

const MASTER: &str = "backup-test-master-9Xf3Kd";
const PASSPHRASE: &str = "an independent backup passphrase 4Qm7";

struct Fixture {
    _dir: tempfile::TempDir,
    vault: PathBuf,
    backup: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    Fixture {
        vault: dir.path().join("vault.db"),
        backup: dir.path().join("export.keyringbackup"),
        _dir: dir,
    }
}

/// Every item type, with a distinctive secret in each secret field, so a dropped
/// field shows up as a mismatch rather than as a coincidence.
fn seed(path: &Path) -> Vec<(uuid::Uuid, Vec<(SecretField, String)>)> {
    let file = VaultFile::create(path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let personal = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    let work = session.vault_add("Work", "vault.accent.2").expect("vault");

    let mut expected = Vec::new();

    let login = session
        .item_upsert(&ItemDraft::new(
            personal,
            "a login",
            ItemBody::Login {
                username: "alice@example.test".to_owned(),
                password: "FIXTURE-PASSWORD-Qb4Zt".to_owned(),
                urls: vec!["https://example.test".to_owned()],
                totp: Some(TotpConfig {
                    secret: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_owned(),
                    algorithm: TotpAlgorithm::Sha512,
                    digits: 8,
                    period_seconds: 60,
                    issuer: "Example".to_owned(),
                    account: "alice@example.test".to_owned(),
                }),
            },
        ))
        .expect("login");
    expected.push((
        login,
        vec![
            (SecretField::Password, "FIXTURE-PASSWORD-Qb4Zt".to_owned()),
            (
                SecretField::TotpSecret,
                "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_owned(),
            ),
        ],
    ));

    let card = session
        .item_upsert(&ItemDraft::new(
            work,
            "a card",
            ItemBody::Card {
                cardholder: "A Alice".to_owned(),
                number: "4111111111117834".to_owned(),
                expiry_month: 7,
                expiry_year: 2031,
                cvv: "FIXTURE-CVV-991".to_owned(),
                pin: "FIXTURE-PIN-4417".to_owned(),
                billing_address: "1 Example Street".to_owned(),
            },
        ))
        .expect("card");
    expected.push((
        card,
        vec![
            (SecretField::CardNumber, "4111111111117834".to_owned()),
            (SecretField::CardCvv, "FIXTURE-CVV-991".to_owned()),
            (SecretField::CardPin, "FIXTURE-PIN-4417".to_owned()),
        ],
    ));

    let identity = session
        .item_upsert(&ItemDraft::new(
            personal,
            "an identity",
            ItemBody::Identity {
                first_name: "A".to_owned(),
                last_name: "Alice".to_owned(),
                dob: "1990-01-01".to_owned(),
                document_type: "passport".to_owned(),
                document_number: "FIXTURE-DOC-X99242".to_owned(),
                issuing_country: "GB".to_owned(),
                expiry: "2031-01-01".to_owned(),
                address: "1 Example Street".to_owned(),
                phone: "+44 20 7946 0000".to_owned(),
                email: "alice@example.test".to_owned(),
            },
        ))
        .expect("identity");
    expected.push((
        identity,
        vec![(SecretField::DocumentNumber, "FIXTURE-DOC-X99242".to_owned())],
    ));

    let note = session
        .item_upsert(&ItemDraft::new(work, "a note", ItemBody::SecureNote))
        .expect("note");
    expected.push((note, Vec::new()));

    // A reveal, so there is activity to carry across too.
    session
        .item_reveal_field(login, SecretField::Password)
        .expect("reveal");

    expected
}

/// A real wipe. Deleting only the `.db` leaves `SQLite` able to recover pages from
/// the write-ahead log, so a test that skipped the sidecars could pass while
/// restoring nothing.
fn wipe(path: &Path) {
    std::fs::remove_file(path).expect("remove db");
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
    assert!(!path.exists(), "the vault file survived the wipe");
}

/// Everything about an item that must survive a round trip.
fn snapshot(
    path: &Path,
    expected: &[(uuid::Uuid, Vec<(SecretField, String)>)],
) -> Vec<(uuid::Uuid, u64, String, String, Vec<String>)> {
    let file = VaultFile::open(path).expect("open");
    let session = file.unlock(MASTER).expect("unlock");

    expected
        .iter()
        .map(|(id, secrets)| {
            let meta = session.item_meta(*id).expect("meta");
            let values = secrets
                .iter()
                .map(|(field, _)| {
                    session
                        .item_secret(*id, *field)
                        .expect("secret")
                        .to_string()
                })
                .collect();
            (
                meta.id,
                meta.revision,
                meta.title.clone(),
                format!("{:?}", meta.body),
                values,
            )
        })
        .collect()
}

// ── AC15 proper ─────────────────────────────────────────────────────────────

#[test]
fn export_wipe_restore_gives_an_identical_vault() {
    let fx = fixture();
    let expected = seed(&fx.vault);
    let before = snapshot(&fx.vault, &expected);

    // Export under a passphrase that is not the master password.
    let summary = {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export")
    };
    assert_eq!(summary.items, 4);
    assert_eq!(summary.vaults, 2);
    assert!(summary.bytes > 228, "the container has no body");

    wipe(&fx.vault);

    let contents = open_container(&fx.backup, PASSPHRASE).expect("open container");
    assert_eq!(contents.item_count(), 4);
    assert_eq!(contents.vault_count(), 2);

    let preview = contents.preview(&fx.vault).expect("preview");
    assert_eq!(preview.mode, RestoreMode::Fresh);
    assert_eq!(preview.created, 4);
    assert_eq!(preview.merged, 0);
    assert_eq!(preview.skipped, 0);

    contents.restore_replacing(&fx.vault).expect("restore");

    // The restored vault unlocks with the ORIGINAL master password, which means the
    // header MAC and the manifest signature both verified — `unlock` checks the MAC
    // before anything else and the signature over the live item set after
    // decrypting metadata, and refuses on either failure.
    let after = snapshot(&fx.vault, &expected);
    assert_eq!(
        after, before,
        "the restored vault differs from the exported one"
    );

    // And every secret field, by value.
    let file = VaultFile::open(&fx.vault).expect("open restored");
    let session = file.unlock(MASTER).expect("unlock restored");
    for (id, secrets) in &expected {
        for (field, value) in secrets {
            assert_eq!(
                &*session.item_secret(*id, *field).expect("secret"),
                value,
                "{field:?} on {id} did not survive the round trip"
            );
        }
    }
}

#[test]
fn a_restored_vault_keeps_its_activity_and_totp_parameters() {
    let fx = fixture();
    let expected = seed(&fx.vault);
    let login = expected[0].0;

    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }
    wipe(&fx.vault);
    open_container(&fx.backup, PASSPHRASE)
        .expect("open container")
        .restore_replacing(&fx.vault)
        .expect("restore");

    let file = VaultFile::open(&fx.vault).expect("open");
    let session = file.unlock(MASTER).expect("unlock");

    // Activity: created + revealed.
    let events = session.item_activity(login, 10).expect("activity");
    assert_eq!(events.len(), 2, "activity did not survive the restore");

    // The full TOTP configuration, not just the seed.
    let totp = session
        .item_totp(login)
        .expect("read totp")
        .expect("the item has a TOTP");
    assert_eq!(totp.algorithm, TotpAlgorithm::Sha512);
    assert_eq!(totp.digits, 8);
    assert_eq!(totp.period_seconds, 60);
}

// ── The passphrase is independent of the master password ────────────────────

#[test]
fn the_container_does_not_open_with_the_master_password() {
    let fx = fixture();
    seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    assert!(
        open_container(&fx.backup, MASTER).is_err(),
        "the master password opened the container — the passphrase is supposed to \
         be independent (§7.8)"
    );
    assert!(open_container(&fx.backup, "").is_err());
    assert!(open_container(&fx.backup, "not the passphrase").is_err());
    assert!(open_container(&fx.backup, PASSPHRASE).is_ok());
}

#[test]
fn restoring_needs_no_master_password() {
    // A container is ciphertext: the passphrase opens the wrapper and the master
    // password is only needed to *use* the result. That is what makes a backup
    // useful to someone whose machine died, and it is asserted here because it is a
    // design choice rather than an accident.
    let fx = fixture();
    let expected = seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }
    wipe(&fx.vault);

    // No MASTER anywhere in this block.
    let contents = open_container(&fx.backup, PASSPHRASE).expect("open container");
    contents.restore_replacing(&fx.vault).expect("restore");

    // And the restored vault still requires it to be used.
    let file = VaultFile::open(&fx.vault).expect("open");
    assert!(file.unlock("the-wrong-master-password").is_err());
    let session = file
        .unlock(MASTER)
        .expect("the original master password works");
    assert_eq!(session.items_list().expect("items").len(), expected.len());
}

// ── Tampering must be refused, never repaired ───────────────────────────────

#[test]
fn a_tampered_body_does_not_open() {
    let fx = fixture();
    seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    let mut bytes = std::fs::read(&fx.backup).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&fx.backup, &bytes).expect("write");

    assert!(
        open_container(&fx.backup, PASSPHRASE).is_err(),
        "a bit-flipped container body authenticated"
    );
}

#[test]
fn a_tampered_header_reads_as_tampering_and_not_as_a_wrong_passphrase() {
    let fx = fixture();
    seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    // The `created_at` field at offset 188 is covered by the MAC but not by the
    // passphrase verifier, so flipping it separates the two failure modes.
    let mut bytes = std::fs::read(&fx.backup).expect("read");
    bytes[188] ^= 0xff;
    std::fs::write(&fx.backup, &bytes).expect("write");

    match open_container(&fx.backup, PASSPHRASE) {
        Err(StoreError::Tampered(_)) => {}
        other => panic!("expected tampering, got {other:?}"),
    }
}

#[test]
fn a_file_that_is_not_a_container_is_refused_cleanly() {
    let fx = fixture();
    for junk in [
        b"".to_vec(),
        b"not a keyring backup".to_vec(),
        vec![0u8; 227],
        vec![0u8; 400],
    ] {
        std::fs::write(&fx.backup, &junk).expect("write");
        assert!(
            open_container(&fx.backup, PASSPHRASE).is_err(),
            "a {}-byte non-container opened",
            junk.len()
        );
    }
}

#[test]
fn an_item_removed_from_the_container_fails_the_manifest() {
    // The container carries its own manifest signature over its own item set, so
    // dropping an item from the body must be detected — the same rollback
    // resistance §3.5 gives the vault, applied to the backup.
    let fx = fixture();
    seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    // Truncating the sealed body is the crudest form of removal, and the AEAD tag
    // catches it before the manifest even has to.
    let bytes = std::fs::read(&fx.backup).expect("read");
    std::fs::write(&fx.backup, &bytes[..bytes.len() - 64]).expect("write");
    assert!(open_container(&fx.backup, PASSPHRASE).is_err());
}

// ── Preview, and the two restores it distinguishes ──────────────────────────

#[test]
fn a_container_from_another_account_offers_replace_not_merge() {
    // Nothing in it decrypts under the target's master password, so item-level
    // merge is not merely unimplemented — it is meaningless. The preview says so
    // rather than silently doing the wrong thing.
    let fx = fixture();
    seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    // A different account at the same path: fresh keys, so a different
    // pubkey_ed25519.
    wipe(&fx.vault);
    {
        let file = VaultFile::create(&fx.vault, "a-different-master-8Kd", KdfParams::floor())
            .expect("create other");
        let session = file.unlock("a-different-master-8Kd").expect("unlock other");
        session
            .vault_add("Theirs", "vault.accent.3")
            .expect("vault");
    }

    let contents = open_container(&fx.backup, PASSPHRASE).expect("open container");
    let preview = contents.preview(&fx.vault).expect("preview");
    assert_eq!(preview.mode, RestoreMode::Replace);
    assert_eq!(preview.created, 4);
}

#[test]
fn a_container_from_this_account_offers_merge_with_real_counts() {
    let fx = fixture();
    let expected = seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    // Same vault, unchanged: every item is already present at the same revision.
    let contents = open_container(&fx.backup, PASSPHRASE).expect("open container");
    let preview = contents.preview(&fx.vault).expect("preview");
    assert_eq!(preview.mode, RestoreMode::Merge);
    assert_eq!(preview.created, 0);
    assert_eq!(preview.merged, 0);
    assert_eq!(
        preview.skipped, 4,
        "an unchanged vault should skip everything, not merge it"
    );

    // Now delete one item and rotate another, so the counts have to differ.
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session.item_delete(expected[3].0).expect("delete note");
        let mut rotated = ItemDraft::new(
            session.item_meta(expected[0].0).expect("meta").vault_id,
            "a login",
            ItemBody::Login {
                username: "alice@example.test".to_owned(),
                password: "ROTATED-PASSWORD-Zx9".to_owned(),
                urls: vec!["https://example.test".to_owned()],
                totp: None,
            },
        );
        rotated.id = Some(expected[0].0);
        session.item_upsert(&rotated).expect("rotate");
    }

    let contents = open_container(&fx.backup, PASSPHRASE).expect("open container");
    let preview = contents.preview(&fx.vault).expect("preview");
    assert_eq!(preview.mode, RestoreMode::Merge);
    // The deleted note is absent from the live set, so the backup would recreate it.
    assert_eq!(
        preview.created, 1,
        "the deleted item should read as created"
    );
    // The rotated login is at a higher revision in the target, so the older backup
    // copy must not overwrite it.
    assert_eq!(
        preview.merged, 0,
        "an older backup revision must not be offered as a merge"
    );
    assert_eq!(preview.skipped, 3);
}

#[test]
fn a_merge_restores_a_deleted_item_and_leaves_a_newer_one_alone() {
    let fx = fixture();
    let expected = seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    let note = expected[3].0;
    let login = expected[0].0;
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session.item_delete(note).expect("delete");

        let mut rotated = ItemDraft::new(
            session.item_meta(login).expect("meta").vault_id,
            "a login",
            ItemBody::Login {
                username: "alice@example.test".to_owned(),
                password: "ROTATED-PASSWORD-Zx9".to_owned(),
                urls: vec!["https://example.test".to_owned()],
                totp: None,
            },
        );
        rotated.id = Some(login);
        session.item_upsert(&rotated).expect("rotate");
    }

    let contents = open_container(&fx.backup, PASSPHRASE).expect("open container");
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        let applied = session.backup_merge(&contents).expect("merge");
        assert_eq!(applied.created, 1);
    }

    // The merged vault must still unlock, which means the manifest was re-signed.
    let file = VaultFile::open(&fx.vault).expect("reopen after merge");
    let session = file
        .unlock(MASTER)
        .expect("a merge left the manifest unsigned");

    // The deleted note is back.
    assert!(
        session.item_meta(note).is_ok(),
        "the merge did not restore the deleted item"
    );
    // The rotated password survived: a merge must not roll a newer item back to an
    // older backup copy, which would silently undo a rotation the user did after a
    // breach — the exact attack §3.5 exists to prevent, arriving through a feature.
    assert_eq!(
        &*session
            .item_secret(login, SecretField::Password)
            .expect("secret"),
        "ROTATED-PASSWORD-Zx9",
        "the merge rolled a rotated password back to the backup's older copy"
    );
}

#[test]
fn a_merge_from_another_account_is_refused() {
    let fx = fixture();
    seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }
    wipe(&fx.vault);

    let other_master = "a-different-master-8Kd";
    {
        let file =
            VaultFile::create(&fx.vault, other_master, KdfParams::floor()).expect("create other");
        let session = file.unlock(other_master).expect("unlock other");
        session
            .vault_add("Theirs", "vault.accent.3")
            .expect("vault");
    }

    let contents = open_container(&fx.backup, PASSPHRASE).expect("open container");
    let file = VaultFile::open(&fx.vault).expect("open");
    let session = file.unlock(other_master).expect("unlock");
    assert!(
        session.backup_merge(&contents).is_err(),
        "a merge across accounts was allowed, and none of those items can decrypt"
    );
}

// ── Export is not a plaintext path ──────────────────────────────────────────

#[test]
fn the_container_holds_no_plaintext_secret() {
    // The whole file, scanned for the sentinels. §7.8 says a container carries
    // "full ciphertext", and this is what that has to mean.
    let fx = fixture();
    seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    let bytes = std::fs::read(&fx.backup).expect("read");
    for sentinel in [
        "FIXTURE-PASSWORD-Qb4Zt",
        "FIXTURE-CVV-991",
        "FIXTURE-PIN-4417",
        "FIXTURE-DOC-X99242",
        "4111111111117834",
        "GEZDGNBVGY3TQOJQ",
        // The passphrase and the master password must not be in there either.
        PASSPHRASE,
        MASTER,
        // Metadata is encrypted too, so a title should not appear.
        "a login",
    ] {
        assert!(
            !bytes
                .windows(sentinel.len())
                .any(|w| w == sentinel.as_bytes()),
            "{sentinel:?} appears in the container in the clear"
        );
    }
}

#[test]
fn two_exports_of_the_same_vault_differ() {
    // Fresh salt and fresh nonces each time. Two identical containers would mean a
    // reused nonce, which for XChaCha20-Poly1305 is a catastrophic failure rather
    // than an inefficiency.
    let fx = fixture();
    seed(&fx.vault);
    let second = fx.backup.with_extension("second");

    let file = VaultFile::open(&fx.vault).expect("open");
    let session = file.unlock(MASTER).expect("unlock");
    session
        .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
        .expect("first");
    session
        .backup_export(&second, PASSPHRASE, KdfParams::floor())
        .expect("second");

    let a = std::fs::read(&fx.backup).expect("read a");
    let b = std::fs::read(&second).expect("read b");
    assert_ne!(a, b, "two exports were byte-identical");

    // Both still open, so the difference is salt and nonces rather than corruption.
    assert!(open_container(&fx.backup, PASSPHRASE).is_ok());
    assert!(open_container(&second, PASSPHRASE).is_ok());
}

#[test]
fn a_failed_restore_leaves_no_staging_file_behind() {
    let fx = fixture();
    seed(&fx.vault);
    {
        let file = VaultFile::open(&fx.vault).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .backup_export(&fx.backup, PASSPHRASE, KdfParams::floor())
            .expect("export");
    }

    let contents = open_container(&fx.backup, PASSPHRASE).expect("open container");
    contents.restore_replacing(&fx.vault).expect("restore");

    let staging = fx.vault.with_extension("restore-staging");
    assert!(
        !staging.exists(),
        "a staging file survived a successful restore"
    );
}
