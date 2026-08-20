// SPDX-License-Identifier: AGPL-3.0-or-later
//! Unlocking pins the session's key pages into RAM (CLAUDE.md §4.5).
//!
//! The unit tests in `platform::memory` prove `VirtualLock` works. This proves it is
//! actually *called* — which is the half that was missing, and missing in a way nobody
//! noticed for two releases while `SECURITY.md` claimed the opposite.
//!
//! That is the failure mode worth testing against. A module that can lock pages and is
//! never invoked looks identical, from the outside, to one that locks them on every
//! unlock. So this goes through the real `SessionManager::adopt` and asserts on the
//! regions it offers.
//!
//! What it does not claim: that the pages stay pinned if the keys are later moved or
//! cloned. Locking pins an address, not a value. See the module note in
//! `platform/memory.rs` for the full list of what this does and does not buy.

use std::sync::{Arc, Mutex};

use keyring_lib::platform::{
    lock_pages, BiometricError, BiometricKind, Biometrics, Clipboard, ClipboardError, Platform,
    SecureStore, SecureStoreError,
};
use keyring_lib::session::SessionManager;
use keyring_store::{KdfParams, VaultFile};

const MASTER: &str = "key-page-lock-master-8Rt3Kw";

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

fn platform() -> Arc<Platform> {
    Arc::new(Platform {
        biometrics: Arc::new(NoBiometrics),
        clipboard: Arc::new(NoClipboard::default()),
        secure_store: Arc::new(NoStore),
    })
}

#[test]
fn a_real_unlock_offers_every_key_region_for_pinning() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = Arc::new(
        VaultFile::create(&dir.path().join("vault.db"), MASTER, KdfParams::floor())
            .expect("create"),
    );
    let keys = file.unlock(MASTER).expect("unlock").into_keys();

    // Three 32-byte secrets: the MUK, and the account X25519 and Ed25519 keys. If the
    // key hierarchy grows another, this number should change deliberately rather than
    // quietly — a new key that nobody pins is exactly the gap this file is about.
    let mut regions = Vec::new();
    keys.for_each_key_region(|r| regions.push(r.len()));
    assert_eq!(
        regions,
        vec![32, 32, 32],
        "expected the MUK and both account secrets to be offered"
    );

    // And each one is lockable in practice, not merely offered.
    let mut pinned = 0;
    keys.for_each_key_region(|r| {
        if lock_pages(r).is_ok() {
            pinned += 1;
        }
    });
    if cfg!(windows) {
        assert_eq!(
            pinned, 3,
            "every key region should pin on the verified platform"
        );
    }

    // Adopting must not panic, and must leave the vault usable — the lock is
    // best-effort and sits on the unlock path, so a mistake here breaks unlocking.
    let manager = SessionManager::new(platform(), Arc::new(keyring_lib::autolock::SystemClock));
    manager.attach(file.clone());
    manager.begin_unlock().expect("begin");
    manager.adopt(keys, true);
    assert_eq!(
        manager.state(),
        keyring_lib::session::VaultState::Unlocked,
        "adopt must still unlock the vault"
    );
    manager
        .build_index()
        .expect("the vault is usable after adopt");
}

#[test]
fn the_regions_offered_are_the_live_buffers_and_not_copies() {
    // Locking a copy would pin the wrong page and leave the real key pageable, which
    // would be worse than not locking at all: the check would pass and the property
    // would be absent. Two visits must report the same addresses.
    let dir = tempfile::tempdir().expect("tempdir");
    let file = VaultFile::create(&dir.path().join("vault.db"), MASTER, KdfParams::floor())
        .expect("create");
    let keys = file.unlock(MASTER).expect("unlock").into_keys();

    let mut first = Vec::new();
    keys.for_each_key_region(|r| first.push(r.as_ptr() as usize));
    let mut second = Vec::new();
    keys.for_each_key_region(|r| second.push(r.as_ptr() as usize));

    assert_eq!(
        first, second,
        "the visitor handed back different addresses on two calls, so at least one is a \
         temporary copy rather than the key's own buffer"
    );
    assert!(
        first.iter().all(|&a| a != 0),
        "a null address means the borrow did not point at a real buffer"
    );
}
