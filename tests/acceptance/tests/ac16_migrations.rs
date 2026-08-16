//! SPEC-V1 §11: both migration phases run; a `VACUUM INTO` snapshot exists
//! before each.
//!
//! Schema migrations run pre-unlock (you read the header to know how to unlock);
//! payload migrations run post-unlock (they need the MUK). Two counters, two
//! runners, one snapshot before each phase that has pending work.
//!
//! The runner under test is the real one. The migrations fed to it are fixtures,
//! because V1 ships only the initial schema — there is no second real migration
//! to exercise a *forward* migration with yet.
//!
//! FROZEN. See `tests/acceptance/API.md`.

use std::sync::atomic::{AtomicUsize, Ordering};

use keyring_acceptance::{fixture_params, MASTER};
use store::{
    ItemBody, ItemDraft, MigrationSet, PayloadCtx, PayloadMigration, SchemaMigration, StoreError,
    VaultFile,
};

static PAYLOAD_RUNS: AtomicUsize = AtomicUsize::new(0);
static PAYLOAD_SAW_ITEMS: AtomicUsize = AtomicUsize::new(0);

fn fixture_payload_migration(ctx: &PayloadCtx) -> Result<(), StoreError> {
    PAYLOAD_RUNS.fetch_add(1, Ordering::SeqCst);
    PAYLOAD_SAW_ITEMS.store(ctx.item_count()?, Ordering::SeqCst);
    Ok(())
}

fn upgraded_set() -> MigrationSet {
    MigrationSet::current()
        .with_schema(SchemaMigration {
            version: 2,
            name: "fixture_add_column",
            sql: "ALTER TABLE items ADD COLUMN fixture_marker INTEGER NOT NULL DEFAULT 0;",
        })
        .with_payload(PayloadMigration {
            version: 2,
            name: "fixture_payload_touch",
            apply: fixture_payload_migration,
        })
}

#[test]
fn both_phases_run_with_a_snapshot_before_each() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    // ── a vault at the shipped versions ──────────────────────────────────────
    {
        let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        let vault_id = session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault_add");
        for i in 0..3 {
            session
                .item_upsert(&ItemDraft::new(
                    vault_id,
                    &format!("item {i}"),
                    ItemBody::SecureNote,
                ))
                .expect("upsert");
        }
        assert_eq!(
            session.payload_version(),
            1,
            "a fresh vault starts at payload v1"
        );
    }

    let baseline = VaultFile::open(&path).expect("open");
    assert_eq!(
        baseline.schema_version(),
        1,
        "a fresh vault starts at schema v1"
    );
    drop(baseline);

    let snapshots_before = VaultFile::snapshots(&path).expect("snapshots").len();

    // ── schema phase: pre-unlock ─────────────────────────────────────────────
    let set = upgraded_set();
    let file = VaultFile::open_with(&path, &set).expect("open_with runs the schema phase");
    assert_eq!(
        file.schema_version(),
        2,
        "the schema phase did not advance its counter"
    );

    let after_schema = VaultFile::snapshots(&path).expect("snapshots").len();
    assert!(
        after_schema > snapshots_before,
        "no VACUUM INTO snapshot was taken before the schema phase \
         ({snapshots_before} → {after_schema})"
    );
    assert_eq!(
        PAYLOAD_RUNS.load(Ordering::SeqCst),
        0,
        "a payload migration ran before unlock — it cannot have had the MUK"
    );

    // ── payload phase: post-unlock ───────────────────────────────────────────
    let session = file
        .unlock_with(MASTER, &set)
        .expect("unlock_with runs the payload phase");
    assert_eq!(
        session.payload_version(),
        2,
        "the payload phase did not advance its counter"
    );
    assert_eq!(
        PAYLOAD_RUNS.load(Ordering::SeqCst),
        1,
        "the payload migration ran {} times, expected exactly once",
        PAYLOAD_RUNS.load(Ordering::SeqCst)
    );
    assert_eq!(
        PAYLOAD_SAW_ITEMS.load(Ordering::SeqCst),
        3,
        "the payload migration could not see the vault's items"
    );

    let after_payload = VaultFile::snapshots(&path).expect("snapshots").len();
    assert!(
        after_payload > after_schema,
        "no VACUUM INTO snapshot was taken before the payload phase \
         ({after_schema} → {after_payload})"
    );

    // ── idempotence: neither phase re-runs ───────────────────────────────────
    drop(session);
    drop(file);
    let file = VaultFile::open_with(&path, &set).expect("reopen");
    let session = file.unlock_with(MASTER, &set).expect("re-unlock");
    assert_eq!(file.schema_version(), 2);
    assert_eq!(session.payload_version(), 2);
    assert_eq!(
        PAYLOAD_RUNS.load(Ordering::SeqCst),
        1,
        "an already-applied payload migration ran a second time"
    );
}

#[test]
fn snapshot_retention_keeps_the_last_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    {
        let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .vault_add("Personal", "vault.accent.1")
            .expect("vault_add");
    }

    // Six schema migrations, applied one version per open, so six snapshots are taken.
    const RETENTION_SQL: &str = "CREATE TABLE IF NOT EXISTS fixture_retention (x INTEGER);";
    for target in 2..=7u32 {
        let mut set = MigrationSet::current();
        for version in 2..=target {
            set = set.with_schema(SchemaMigration {
                version,
                name: "fixture_retention",
                sql: RETENTION_SQL,
            });
        }
        let file = VaultFile::open_with(&path, &set).expect("open_with");
        assert_eq!(file.schema_version(), target);
    }

    let snapshots = VaultFile::snapshots(&path).expect("snapshots");
    assert_eq!(
        snapshots.len(),
        3,
        "snapshot retention must keep exactly the last 3, found {}",
        snapshots.len()
    );
}
