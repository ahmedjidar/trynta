//! SPEC-V1 §11: no plaintext item content on disk. Seed known sentinel strings,
//! assert none appear in the `.db`, `-wal` or `-shm` files.
//!
//! Ids, timestamps, salt, kdf params and public keys *are* plaintext by design
//! (§4.4) and are not sentinels. Creation timestamps are not leaked, which is why
//! ids are UUIDv4 — asserted here too.
//!
//! FROZEN. See `tests/acceptance/API.md`.

use std::fs;
use std::path::Path;

use keyring_acceptance::{find_bytes, fixture_params, four_item_drafts, sentinel, MASTER};
use store::VaultFile;

fn sidecars(path: &Path) -> Vec<std::path::PathBuf> {
    let base = path.as_os_str().to_string_lossy().into_owned();
    vec![
        path.to_path_buf(),
        std::path::PathBuf::from(format!("{base}-wal")),
        std::path::PathBuf::from(format!("{base}-shm")),
    ]
}

/// Scan every existing file for every sentinel. Returns human-readable hits.
fn scan(path: &Path, phase: &str) -> Vec<String> {
    let mut hits = Vec::new();
    let mut scanned = 0usize;
    for file in sidecars(path) {
        let Ok(bytes) = fs::read(&file) else { continue };
        scanned += 1;
        for needle in sentinel::all() {
            if let Some(at) = find_bytes(&bytes, needle.as_bytes()) {
                hits.push(format!(
                    "{phase}: {} contains {needle:?} at byte {at}",
                    file.display()
                ));
            }
        }
    }
    assert!(
        scanned > 0,
        "{phase}: scanned no files at all — the scan is not proving anything"
    );
    hits
}

#[test]
fn no_sentinel_reaches_the_database_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    let hits_live = {
        let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add(sentinel::TITLE, "vault.accent.1")
            .expect("vault_add");

        for draft in four_item_drafts(vault_id) {
            session.item_upsert(&draft).expect("item_upsert");
        }

        // Scan with the connection still open, so any live -wal and -shm are covered.
        scan(&path, "while open")
    };

    // Scan again after every handle is dropped and SQLite has checkpointed.
    let hits_closed = scan(&path, "after close");

    let all: Vec<String> = hits_live.into_iter().chain(hits_closed).collect();
    assert!(
        all.is_empty(),
        "plaintext item content reached disk:\n{}",
        all.join("\n")
    );
}

#[test]
fn item_ids_are_uuid_v4_so_creation_time_is_not_leaked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault_id = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault_add");

    let mut ids = vec![vault_id];
    for draft in four_item_drafts(vault_id) {
        ids.push(session.item_upsert(&draft).expect("item_upsert"));
    }

    for id in ids {
        assert_eq!(
            id.get_version_num(),
            4,
            "id {id} is not UUIDv4 — a v7 id in a plaintext column leaks the creation time \
             that §4.4 deliberately encrypts"
        );
    }
}
