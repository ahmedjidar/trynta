//! A write that changes a list column must rebuild the index.
//!
//! `items_list` does not read the store. It reads the in-memory `SearchIndex` built
//! at unlock, which is the whole point of SPEC-V1 §4.7 — search runs against
//! decrypted metadata already in memory, so it is fast and never touches
//! `secret_ct`. `has_custom_icon` and `has_totp` are columns of that index.
//!
//! Which means a command can write correctly to the store and still change nothing
//! the user can see. That is what happened: `item_set_icon`, `item_clear_icon` and
//! `item_set_totp` each wrote, resealed and returned success, and the list went on
//! reporting the old icon and the old badge until the vault was locked and reopened.
//! From the outside it looked exactly like "changing my icon does nothing".
//!
//! It survived a round of testing because the test that was supposed to cover it
//! changed a URL through `item_edit_meta` — which *does* rebuild — and concluded the
//! reactivity worked. So this asserts the trap directly: the index goes stale after a
//! write and comes right only after a rebuild. If someone removes a `build_index()`
//! call from a command, the first half of each test still passes and they find this.

use std::sync::{Arc, Mutex};

use keyring_lib::error::AppError;
use keyring_lib::index::ListQuery;
use keyring_lib::platform::{
    BiometricError, BiometricKind, Biometrics, Clipboard, ClipboardError, Platform, SecureStore,
    SecureStoreError,
};
use keyring_lib::session::SessionManager;
use keyring_store::model::{IconFormat, StoredIcon};
use keyring_store::{ItemBody, ItemDraft, KdfParams, VaultFile};
use uuid::Uuid;

const MASTER: &str = "index-freshness-master-3Qp7Wz";

// ── Doubles ─────────────────────────────────────────────────────────────────

struct NoBiometrics;
impl Biometrics for NoBiometrics {
    fn kind(&self) -> BiometricKind {
        BiometricKind::None
    }
    fn is_available(&self) -> bool {
        false
    }
    fn enrol(&self, _label: &str, _secret: &[u8]) -> Result<(), BiometricError> {
        Err(BiometricError::Unavailable)
    }
    fn unwrap_secret(&self, _label: &str) -> Result<Vec<u8>, BiometricError> {
        Err(BiometricError::Unavailable)
    }
    fn revoke(&self, _label: &str) -> Result<(), BiometricError> {
        Ok(())
    }
}

#[derive(Default)]
struct NoClipboard(Mutex<u64>);
impl Clipboard for NoClipboard {
    fn set_secret(&self, _value: &str) -> Result<u64, ClipboardError> {
        let mut n = self.0.lock().expect("lock");
        *n += 1;
        Ok(*n)
    }
    fn clear_if_ours(&self, _token: u64) -> Result<bool, ClipboardError> {
        Ok(true)
    }
}

struct NoStore;
impl SecureStore for NoStore {
    fn store(&self, _key: &str, _value: &[u8]) -> Result<(), SecureStoreError> {
        Ok(())
    }
    fn load(&self, _key: &str) -> Result<Option<Vec<u8>>, SecureStoreError> {
        Ok(None)
    }
    fn delete(&self, _key: &str) -> Result<(), SecureStoreError> {
        Ok(())
    }
}

struct Harness {
    manager: SessionManager,
    _dir: tempfile::TempDir,
}

/// A vault with one login in it, unlocked, indexed.
fn harness() -> (Harness, Uuid) {
    let platform = Arc::new(Platform {
        biometrics: Arc::new(NoBiometrics),
        clipboard: Arc::new(NoClipboard::default()),
        secure_store: Arc::new(NoStore),
    });
    let dir = tempfile::tempdir().expect("tempdir");
    let file = Arc::new(
        VaultFile::create(&dir.path().join("vault.db"), MASTER, KdfParams::floor())
            .expect("create"),
    );
    let manager = SessionManager::new(platform, Arc::new(keyring_lib::autolock::SystemClock));
    manager.attach(file.clone());
    manager.begin_unlock().expect("begin");
    manager.adopt(file.unlock(MASTER).expect("unlock").into_keys(), true);

    // `with_session` flattens: the closure's own error is what comes back, so
    // everything inside maps to `AppError`, which is the type that can carry a
    // session failure as well as a store one.
    let id = manager
        .with_session(|s| {
            // A freshly created file has no vault rows — the first one is made by the
            // setup flow, not by `VaultFile::create`.
            let vault = match s.vaults_list().map_err(AppError::from)?.first() {
                Some(v) => v.id,
                None => s.vault_add("Personal", "vault-1").map_err(AppError::from)?,
            };
            s.item_upsert(&ItemDraft::new(
                vault,
                "Northline Bank",
                ItemBody::Login {
                    username: "ada@example.test".to_owned(),
                    password: String::new(),
                    urls: Vec::new(),
                    totp: None,
                },
            ))
            .map_err(AppError::from)
        })
        .expect("upsert");
    manager.build_index().expect("index");

    (Harness { manager, _dir: dir }, id)
}

/// Whether the index currently reports a custom icon for `id`.
fn index_says_custom(h: &Harness, id: Uuid) -> bool {
    h.manager
        .with_index(|index| {
            index
                .query(&ListQuery::default())
                .into_iter()
                .find(|row| row.id == id)
                .is_some_and(|row| row.has_custom_icon)
        })
        .expect("index")
}

/// A PNG header. The bytes do not matter here; the flag does.
fn icon() -> StoredIcon {
    StoredIcon {
        format: IconFormat::Png,
        bytes: vec![
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52,
        ],
    }
}

#[test]
fn setting_an_icon_is_invisible_to_the_list_until_the_index_is_rebuilt() {
    let (h, id) = harness();
    assert!(
        !index_says_custom(&h, id),
        "a fresh item has no custom icon"
    );

    h.manager
        .with_session(|s| {
            s.item_set_custom_icon(id, Some(icon()))
                .map_err(AppError::from)
        })
        .expect("set icon");

    // The store is right and the list is wrong. This is the bug, asserted rather
    // than described: without a rebuild, `items_list` keeps serving the old row.
    assert!(
        !index_says_custom(&h, id),
        "the index picked the icon up on its own — if this is now true, the index is \
         no longer a snapshot and the rebuild calls in commands/icon.rs can go"
    );

    h.manager.build_index().expect("rebuild");
    assert!(
        index_says_custom(&h, id),
        "after a rebuild the list must report the icon the store already has"
    );
}

#[test]
fn clearing_an_icon_is_invisible_to_the_list_until_the_index_is_rebuilt() {
    let (h, id) = harness();
    h.manager
        .with_session(|s| {
            s.item_set_custom_icon(id, Some(icon()))
                .map_err(AppError::from)
        })
        .expect("set icon");
    h.manager.build_index().expect("rebuild");
    assert!(index_says_custom(&h, id));

    h.manager
        .with_session(|s| s.item_set_custom_icon(id, None).map_err(AppError::from))
        .expect("clear icon");

    // The removal has the same shape, and it is the half the user hit second: the
    // tile stayed on the removed icon instead of falling back to the default mark.
    assert!(index_says_custom(&h, id), "still stale before the rebuild");
    h.manager.build_index().expect("rebuild");
    assert!(
        !index_says_custom(&h, id),
        "after a rebuild the list must report the icon as gone"
    );
}

#[test]
fn a_rebuild_keeps_every_other_column_intact() {
    // Rebuilding carries its own risk: it replaces the whole index, so a column the
    // rebuild path does not populate would silently empty out instead.
    let (h, id) = harness();
    h.manager
        .with_session(|s| {
            s.item_set_custom_icon(id, Some(icon()))
                .map_err(AppError::from)
        })
        .expect("set icon");
    h.manager.build_index().expect("rebuild");

    let row = h
        .manager
        .with_index(|index| {
            index
                .query(&ListQuery::default())
                .into_iter()
                .find(|r| r.id == id)
                .map(|r| (r.title.clone(), r.username.clone(), r.has_custom_icon))
        })
        .expect("index")
        .expect("the item is still listed");

    assert_eq!(row.0, "Northline Bank");
    assert_eq!(row.1.as_deref(), Some("ada@example.test"));
    assert!(row.2);
}
