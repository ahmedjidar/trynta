//! Real macOS platform behaviour (SPEC-V1 §8, §11).
//!
//! **These have never run.** ADD-005 makes Windows the verified platform; this
//! file is written so that the first thing anyone does on real Apple hardware is
//! execute it. It deliberately mirrors `platform_windows.rs` check for check, so a
//! gap between the two platforms shows up as a missing test rather than as an
//! absence nobody notices:
//!
//! | Windows | macOS |
//! |---|---|
//! | `a_secret_copy_carries_every_history_exclusion_format` | `a_secret_copy_carries_every_concealed_type` |
//! | `clearing_removes_our_own_write` | `clearing_removes_our_own_write` |
//! | `clearing_does_not_touch_a_write_that_is_not_ours` | `clearing_does_not_touch_a_write_that_is_not_ours` |
//! | `dpapi_round_trips_a_secret` | `the_keychain_round_trips_a_secret` |
//! | `a_dpapi_blob_is_not_plaintext_on_disk` | `a_keychain_item_is_not_plaintext_in_the_app_support_directory` |
//! | `a_corrupted_dpapi_blob_reads_as_unreadable_not_as_success` | *(no counterpart — see below)* |
//! | `hello_availability_is_reported_honestly` | `touch_id_availability_is_reported_honestly` |
//! | `unwrapping_without_an_enrolment_is_invalidated_not_a_panic` | `unwrapping_without_an_enrolment_is_invalidated_not_a_panic` |
//! | `revoking_a_missing_enrolment_removes_any_stored_wrap` | `revoking_a_missing_enrolment_is_success` |
//!
//! **The one deliberate asymmetry.** Windows has
//! `a_corrupted_dpapi_blob_reads_as_unreadable_not_as_success` because we own the
//! file the blob lives in and can flip a bit in it. The Keychain is an opaque
//! system database; there is no supported way to corrupt one item, and doing it by
//! writing into `login.keychain-db` would be testing `SQLite`, not us. The
//! equivalent macOS property — the item disappearing when enrolment changes — is
//! enforced by the OS and is checklist item B1 in `MACOS-UNVERIFIED.md`, because
//! it needs a human to add a fingerprint.
//!
//! `#![cfg(target_os = "macos")]` rather than `#[ignore]`, so on Windows these do
//! not exist instead of silently "passing".

#![cfg(target_os = "macos")]

use keyring_lib::platform::clipboard::Clipboard;
use keyring_lib::platform::macos::clipboard::{concealed_types, type_present, MacClipboard};
use keyring_lib::platform::macos::keychain::KeychainStore;
use keyring_lib::platform::macos::touch_id::TouchId;
use keyring_lib::platform::secure_store::SecureStore;
use keyring_lib::platform::{BiometricError, BiometricKind, Biometrics};

/// A value that is obviously ours if it turns up somewhere it should not.
const SENTINEL: &str = "TRYNTA-PLATFORM-TEST-4Kq7Zx";

/// Keychain service names used only by this file.
///
/// The Keychain has no temporary-directory equivalent, so isolation is by service
/// name. Anything left behind after a failure is findable with
/// `security find-generic-password -s dev.trynta.desktop.test`.
const TEST_SERVICE: &str = "dev.trynta.desktop.test";
const TEST_BIOMETRIC_SERVICE: &str = "dev.trynta.desktop.test.biometric";

/// The clipboard is one global OS resource, so these tests cannot run in
/// parallel with each other — a second test's write invalidates the first's
/// assertions. Serialising them here rather than with `--test-threads=1` keeps
/// the rest of the suite parallel.
///
/// A panic in one test poisons the lock; recovering the inner guard means the
/// remaining tests still report their own result instead of all failing at once.
static CLIPBOARD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn clipboard_guard() -> std::sync::MutexGuard<'static, ()> {
    CLIPBOARD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ── Clipboard ───────────────────────────────────────────────────────────────

#[test]
fn a_secret_copy_carries_every_concealed_type() {
    let _serial = clipboard_guard();
    let clipboard = MacClipboard::new();
    let token = clipboard.set_secret(SENTINEL).expect("set_secret");
    assert!(token > 0, "a write must produce an ownership token");

    for pasteboard_type in concealed_types() {
        assert!(
            type_present(pasteboard_type),
            "{pasteboard_type} is not on the pasteboard — a clipboard manager is \
             then free to record this password permanently, and Universal Clipboard \
             would already have carried it to another device"
        );
    }

    // Leave nothing behind.
    let _ = clipboard.clear_if_ours(token);
}

#[test]
fn a_secret_copy_is_readable_as_text() {
    // The other half of the concealed-type test: it is possible to declare all the
    // markers correctly and still fail to write the password, because
    // `setString:forType:` returns a bool that is easy to ignore. A copy that
    // marks itself concealed and puts nothing on the pasteboard would pass every
    // other test in this file.
    let _serial = clipboard_guard();
    let clipboard = MacClipboard::new();
    let token = clipboard.set_secret(SENTINEL).expect("set_secret");

    assert!(
        type_present("public.utf8-plain-text"),
        "NSPasteboardTypeString is public.utf8-plain-text and must be present"
    );

    let _ = clipboard.clear_if_ours(token);
}

#[test]
fn clearing_removes_our_own_write() {
    let _serial = clipboard_guard();
    let clipboard = MacClipboard::new();
    let token = clipboard.set_secret(SENTINEL).expect("set_secret");
    assert!(
        clipboard.clear_if_ours(token).expect("clear"),
        "a clear of our own write must report that it cleared something"
    );
    for pasteboard_type in concealed_types() {
        assert!(
            !type_present(pasteboard_type),
            "{pasteboard_type} survived the clear"
        );
    }
}

#[test]
fn clearing_does_not_touch_a_write_that_is_not_ours() {
    let _serial = clipboard_guard();
    // Auto-clear firing after the user copied something else must not wipe their
    // clipboard. This is the reason `clear_if_ours` takes a token at all.
    let clipboard = MacClipboard::new();
    let stale = clipboard.set_secret(SENTINEL).expect("first write");

    // A second write moves the change count on, standing in for the user copying
    // something of their own.
    let current = clipboard
        .set_secret("something the user copied")
        .expect("second write");
    assert_ne!(stale, current, "each write gets a distinct token");

    assert!(
        !clipboard
            .clear_if_ours(stale)
            .expect("clear with a stale token"),
        "a stale token must not clear the pasteboard"
    );

    let _ = clipboard.clear_if_ours(current);
}

// ── Keychain ────────────────────────────────────────────────────────────────

#[test]
fn the_keychain_round_trips_a_secret() {
    let store = KeychainStore::with_service(TEST_SERVICE);
    let _ = store.delete("wrap");

    assert_eq!(store.load("absent").expect("load absent"), None);

    store.store("wrap", SENTINEL.as_bytes()).expect("store");
    assert_eq!(
        store.load("wrap").expect("load").as_deref(),
        Some(SENTINEL.as_bytes())
    );

    store.delete("wrap").expect("delete");
    assert_eq!(store.load("wrap").expect("load after delete"), None);
    store
        .delete("wrap")
        .expect("deleting a missing entry is not an error");
}

#[test]
fn storing_twice_replaces_rather_than_failing() {
    // `SecItemAdd` on an existing item returns errSecDuplicateItem. `store` deletes
    // first for exactly that reason, and this is the test that would catch the
    // delete being dropped — on Windows the same call is a plain file overwrite, so
    // there is no Windows counterpart to lose.
    let store = KeychainStore::with_service(TEST_SERVICE);
    let _ = store.delete("replace");

    store.store("replace", b"first").expect("first store");
    store.store("replace", b"second").expect("second store");
    assert_eq!(
        store.load("replace").expect("load").as_deref(),
        Some(&b"second"[..]),
        "the second write must win, not fail as a duplicate"
    );

    store.delete("replace").expect("cleanup");
}

#[test]
fn a_keychain_item_is_not_plaintext_in_the_app_support_directory() {
    // The macOS counterpart to `a_dpapi_blob_is_not_plaintext_on_disk`. It cannot
    // assert the same thing — the Keychain database is not ours and is not in our
    // data directory — so it asserts the property that actually matters for us:
    // storing a secret writes nothing of our own to disk in the clear.
    let store = KeychainStore::with_service(TEST_SERVICE);
    let _ = store.delete("wrap");
    store.store("wrap", SENTINEL.as_bytes()).expect("store");

    let root = keyring_lib::platform::paths::data_dir().expect("data dir");
    let mut scanned = 0;
    if root.exists() {
        for entry in walk(&root) {
            let bytes = std::fs::read(&entry).unwrap_or_default();
            scanned += 1;
            assert!(
                !bytes
                    .windows(SENTINEL.len())
                    .any(|w| w == SENTINEL.as_bytes()),
                "{} contains the plaintext secret",
                entry.display()
            );
        }
    }
    println!("app-support-files-scanned={scanned}");

    store.delete("wrap").expect("cleanup");
}

/// Every file under `root`, recursively. Small helper; the tree is tiny.
fn walk(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

// ── Touch ID ────────────────────────────────────────────────────────────────

#[test]
fn touch_id_availability_is_reported_honestly() {
    let touch_id = TouchId::new();
    assert_eq!(touch_id.kind(), BiometricKind::TouchId);

    // Record what the machine actually offers rather than asserting a value, the
    // same way the Windows Hello test does. A log line saying "no biometric device
    // on this runner" is evidence; a test that passes either way without saying
    // which is not.
    //
    // On macOS there is only one probe to make — `LAContext
    // canEvaluatePolicy:DeviceOwnerAuthenticationWithBiometrics` — which is the
    // reason this test is shorter than its Windows twin rather than less thorough.
    // It returns false for all of: no Touch ID hardware, hardware present with
    // nothing enrolled, and biometry locked out after too many failures. Those are
    // distinguishable only from the `NSError` domain code, which is checklist item
    // B2 in MACOS-UNVERIFIED.md.
    let available = touch_id.is_available();
    println!("touch-id-available={available}");

    // CI runners have no Secure Enclave biometry, so an assertion either way would
    // be wrong somewhere. What must hold is that the answer is stable — a probe
    // that flips between calls means the LAContext is being reused across a state
    // change it cannot see.
    assert_eq!(
        available,
        touch_id.is_available(),
        "availability must not change between two consecutive probes"
    );
}

#[test]
fn unwrapping_without_an_enrolment_is_invalidated_not_a_panic() {
    // The fallback path: no stored item means the master password is required.
    // Reachable without a prompt, because the Keychain lookup misses before
    // biometry is consulted.
    let touch_id = TouchId::with_service(TEST_BIOMETRIC_SERVICE);

    let err = touch_id
        .unwrap_secret("trynta-test-no-such-enrolment")
        .expect_err("there is no enrolment");
    assert_eq!(
        err,
        BiometricError::Invalidated,
        "a missing item must send the caller to the master password"
    );
}

#[test]
fn revoking_a_missing_enrolment_is_success() {
    // `revoke` is called defensively before every `enrol`, and on a path where the
    // OS has already destroyed the item because enrolment changed. Both must be
    // success, or re-enrolling after adding a fingerprint fails.
    let touch_id = TouchId::with_service(TEST_BIOMETRIC_SERVICE);
    touch_id
        .revoke("trynta-test-never-existed")
        .expect("revoking a missing enrolment is success");
    touch_id
        .revoke("trynta-test-never-existed")
        .expect("and it stays success when repeated");
}

// UNVERIFIED: there is no test here for `enrol` followed by `unwrap_secret`. It
// cannot be one — reading the item raises a Touch ID prompt and blocks on a
// finger, so it would hang CI rather than fail it. That round trip, and the
// invalidation that is the entire reason for `BIOMETRY_CURRENT_SET`, are manual
// checklist items B1 and B3 in MACOS-UNVERIFIED.md.
