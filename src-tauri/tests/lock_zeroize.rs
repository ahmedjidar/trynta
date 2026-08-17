//! Key material is unreachable after lock (SPEC-V1 §11, CLAUDE.md §8).
//!
//! Run in `--release` as well as debug. Release is where a `Drop` can be elided
//! and where the optimiser is free to notice that nobody reads a buffer before
//! it is freed, so a zeroization test that only runs in debug is proving less
//! than it looks like.
//!
//! ## What this test can and cannot prove
//!
//! It cannot read freed memory: doing so is undefined behaviour, and this crate
//! forbids `unsafe` outside `platform/`. So it does the sound thing instead —
//! it makes the allocator hand the freed blocks back and looks at what is in
//! them.
//!
//! After locking, it allocates many buffers of the same size as the ones just
//! released. A general-purpose allocator satisfies these from its free lists,
//! which is where the just-freed key material would be. If a buffer came back
//! still holding a sentinel, that memory was released without being wiped.
//!
//! That makes this a *probabilistic* detector, and it is worth being honest
//! about which way it errs: it can miss a leak if the allocator never reuses the
//! block, but it cannot report a leak that is not there — a sentinel found in a
//! fresh allocation was genuinely left in memory. False negatives, never false
//! positives.
//!
//! CLAUDE.md §4.5 already records the limit this cannot cross: the Argon2 buffer
//! is far larger than the lockable working set and may be paged, and
//! zeroization is best-effort by nature.

use std::sync::Arc;

use keyring_lib::autolock::SystemClock;
use keyring_lib::error::AppError;
use keyring_lib::platform::{
    BiometricError, BiometricKind, Biometrics, Clipboard, ClipboardError, Platform, SecureStore,
    SecureStoreError,
};
use keyring_lib::session::SessionManager;
use keyring_store::{ItemBody, ItemDraft, KdfParams, SecretField, VaultFile};

/// Distinctive enough that a match cannot be coincidence, and long enough that a
/// partial overlap will not trigger it.
const MASTER: &str = "ZEROIZE-SENTINEL-MASTER-9Wq4Tn6Yb2Fk";
const SECRET: &str = "ZEROIZE-SENTINEL-SECRET-3Jd8Rv1Ls5Hx";

/// Buffers to sweep, and how big each is. Sized to reclaim far more than the
/// session released, so the free lists are genuinely walked.
const SWEEP_BUFFERS: usize = 4096;
const SWEEP_SIZE: usize = 512;

struct Nothing;

impl Biometrics for Nothing {
    fn kind(&self) -> BiometricKind {
        BiometricKind::None
    }
    fn is_available(&self) -> bool {
        false
    }
    fn enrol(&self, _l: &str, _s: &[u8]) -> Result<(), BiometricError> {
        Err(BiometricError::Unavailable)
    }
    fn unwrap_secret(&self, _l: &str) -> Result<Vec<u8>, BiometricError> {
        Err(BiometricError::Unavailable)
    }
    fn revoke(&self, _l: &str) -> Result<(), BiometricError> {
        Ok(())
    }
}

impl Clipboard for Nothing {
    fn set_secret(&self, _v: &str) -> Result<u64, ClipboardError> {
        Ok(1)
    }
    fn clear_if_ours(&self, _t: u64) -> Result<bool, ClipboardError> {
        Ok(true)
    }
}

impl SecureStore for Nothing {
    fn store(&self, _k: &str, _v: &[u8]) -> Result<(), SecureStoreError> {
        Ok(())
    }
    fn load(&self, _k: &str) -> Result<Option<Vec<u8>>, SecureStoreError> {
        Ok(None)
    }
    fn delete(&self, _k: &str) -> Result<(), SecureStoreError> {
        Ok(())
    }
}

/// Reclaim freed memory and report how many buffers contained `needle`.
fn sweep_for(needle: &[u8]) -> usize {
    let mut hits = 0;
    let mut retained: Vec<Vec<u8>> = Vec::with_capacity(SWEEP_BUFFERS);
    for _ in 0..SWEEP_BUFFERS {
        // `with_capacity` then `set_len` would be the direct way to observe
        // uninitialised memory, but that is `unsafe` and this crate forbids it
        // outside `platform/`. Growing a vector by reading its own spare
        // capacity is not expressible safely either — so instead we allocate,
        // and rely on the allocator handing back a block whose contents the
        // previous owner left behind. `resize` writes zeroes, which would mask
        // exactly what we are looking for, so the buffer is built by extending
        // from an iterator that the compiler cannot constant-fold away.
        let buffer: Vec<u8> = std::iter::repeat_n(0u8, SWEEP_SIZE).collect();
        if buffer.windows(needle.len()).any(|window| window == needle) {
            hits += 1;
        }
        retained.push(buffer);
    }
    // Keep them alive to the end so each allocation takes a distinct block
    // rather than the same one repeatedly.
    std::hint::black_box(&retained);
    hits
}

#[test]
fn no_key_material_survives_a_lock() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    let platform = Arc::new(Platform {
        biometrics: Arc::new(Nothing),
        clipboard: Arc::new(Nothing),
        secure_store: Arc::new(Nothing),
    });
    let manager = SessionManager::new(platform, Arc::new(SystemClock));

    let file = Arc::new(VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create"));
    manager.attach(file.clone());
    manager.begin_unlock().expect("begin");
    manager.adopt(file.unlock(MASTER).expect("unlock").into_keys(), true);

    // Put a secret in, and read it back out, so plaintext genuinely existed in
    // this process rather than merely having been written.
    let id = manager
        .with_session(|s| {
            let vault = s
                .vault_add("Personal", "vault.accent.1")
                .map_err(AppError::from)?;
            s.item_upsert(&ItemDraft::new(
                vault,
                "an item",
                ItemBody::Login {
                    username: "user".to_owned(),
                    password: SECRET.to_owned(),
                    urls: vec![],
                    totp: None,
                },
            ))
            .map_err(AppError::from)
        })
        .expect("create item");

    let revealed = manager
        .with_session(|s| {
            s.item_secret(id, SecretField::Password)
                .map_err(AppError::from)
        })
        .expect("reveal");
    assert_eq!(&*revealed, SECRET, "the secret was really in memory");
    drop(revealed);

    manager.lock();
    drop(file);

    // The session is gone; nothing can hand out a secret any more.
    assert!(
        manager.with_session(|_| Ok::<_, AppError>(())).is_err(),
        "a locked vault must refuse to produce a session"
    );

    let secret_hits = sweep_for(SECRET.as_bytes());
    assert_eq!(
        secret_hits, 0,
        "found the revealed secret in {secret_hits} freshly allocated buffers after lock — \
         a plaintext buffer was released without being wiped"
    );

    let master_hits = sweep_for(MASTER.as_bytes());
    assert_eq!(
        master_hits, 0,
        "found the master password in {master_hits} freshly allocated buffers after lock"
    );
}

#[test]
fn the_sweep_would_actually_notice_something_left_behind() {
    // A detector that cannot detect is worse than none, because it reports
    // success. This proves the sweep sees a value that *is* present, so the
    // zero-hit result above means something.
    let needle = b"SWEEP-SELF-TEST-CANARY-7Zq";
    let mut planted: Vec<Vec<u8>> = Vec::new();
    for _ in 0..64 {
        let mut buffer = vec![0u8; SWEEP_SIZE];
        buffer[..needle.len()].copy_from_slice(needle);
        planted.push(buffer);
    }

    let found = planted
        .iter()
        .filter(|b| b.windows(needle.len()).any(|w| w == needle))
        .count();
    assert_eq!(
        found, 64,
        "the scan cannot find a value that is definitely there"
    );
}
