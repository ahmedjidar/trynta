//! Real Windows platform behaviour (SPEC-V1 §8, §11).
//!
//! These run against the actual OS, not a mock. `#![cfg(windows)]` rather than
//! `#[ignore]`, so on macOS they do not exist instead of silently "passing".
//!
//! The clipboard tests are the ones SPEC-V1 §8 demands be explicit: *"The
//! Windows clipboard-history exclusion is easy to miss and quietly defeats
//! clipboard auto-clear. Test it explicitly."* They assert the exclusion formats
//! are genuinely present on the clipboard afterwards — calling
//! `SetClipboardData` is not evidence that the marker landed.
//!
//! Windows Hello enrolment cannot be asserted headlessly: it raises a consent
//! prompt and waits for a finger. What *is* asserted is availability detection
//! and the invalidation path, both of which are reachable without a prompt, and
//! `hello_availability_is_reported_honestly` prints what this machine actually
//! has so a CI log records it rather than a claim.

#![cfg(windows)]

use keyring_lib::platform::clipboard::Clipboard;
use keyring_lib::platform::secure_store::SecureStore;
use keyring_lib::platform::windows::clipboard::{
    exclusion_formats, format_present, WindowsClipboard,
};
use keyring_lib::platform::windows::dpapi::DpapiStore;
use keyring_lib::platform::windows::hello::WindowsHello;
use keyring_lib::platform::{BiometricError, BiometricKind, Biometrics};

/// A value that is obviously ours if it turns up somewhere it should not.
const SENTINEL: &str = "KEYRING-PLATFORM-TEST-4Kq7Zx";

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
fn a_secret_copy_carries_every_history_exclusion_format() {
    let _serial = clipboard_guard();
    let clipboard = WindowsClipboard::new();
    let token = clipboard.set_secret(SENTINEL).expect("set_secret");
    assert!(token > 0, "a write must produce an ownership token");

    for format in exclusion_formats() {
        assert!(
            format_present(format),
            "{format} is not on the clipboard — with Clipboard History enabled this \
             password would stay in the Win+V list after auto-clear, and with Cloud \
             Clipboard on it would already have left the machine"
        );
    }

    // Leave nothing behind.
    let _ = clipboard.clear_if_ours(token);
}

#[test]
fn clearing_removes_our_own_write() {
    let _serial = clipboard_guard();
    let clipboard = WindowsClipboard::new();
    let token = clipboard.set_secret(SENTINEL).expect("set_secret");
    assert!(
        clipboard.clear_if_ours(token).expect("clear"),
        "a clear of our own write must report that it cleared something"
    );
    for format in exclusion_formats() {
        assert!(!format_present(format), "{format} survived the clear");
    }
}

#[test]
fn clearing_does_not_touch_a_write_that_is_not_ours() {
    let _serial = clipboard_guard();
    // Auto-clear firing after the user copied something else must not wipe their
    // clipboard. This is the reason `clear_if_ours` takes a token at all.
    let clipboard = WindowsClipboard::new();
    let stale = clipboard.set_secret(SENTINEL).expect("first write");

    // A second write moves the sequence number on, standing in for the user
    // copying something of their own.
    let current = clipboard
        .set_secret("something the user copied")
        .expect("second write");
    assert_ne!(stale, current, "each write gets a distinct token");

    assert!(
        !clipboard
            .clear_if_ours(stale)
            .expect("clear with a stale token"),
        "a stale token must not clear the clipboard"
    );

    let _ = clipboard.clear_if_ours(current);
}

// ── DPAPI ───────────────────────────────────────────────────────────────────

#[test]
fn dpapi_round_trips_a_secret() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DpapiStore::with_root(dir.path().to_path_buf());

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
fn a_dpapi_blob_is_not_plaintext_on_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DpapiStore::with_root(dir.path().to_path_buf());
    store.store("wrap", SENTINEL.as_bytes()).expect("store");

    let mut found = 0;
    for entry in std::fs::read_dir(dir.path()).expect("read_dir") {
        let bytes = std::fs::read(entry.expect("entry").path()).expect("read");
        found += 1;
        assert!(
            !bytes
                .windows(SENTINEL.len())
                .any(|w| w == SENTINEL.as_bytes()),
            "the DPAPI blob contains the plaintext"
        );
    }
    assert_eq!(found, 1, "exactly one blob should have been written");
}

#[test]
fn a_corrupted_dpapi_blob_reads_as_unreadable_not_as_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DpapiStore::with_root(dir.path().to_path_buf());
    store.store("wrap", SENTINEL.as_bytes()).expect("store");

    let path = std::fs::read_dir(dir.path())
        .expect("read_dir")
        .next()
        .expect("one entry")
        .expect("entry")
        .path();
    let mut bytes = std::fs::read(&path).expect("read");
    bytes[20] ^= 0xff;
    std::fs::write(&path, bytes).expect("write");

    assert!(
        store.load("wrap").is_err(),
        "a tampered blob must not decrypt"
    );
}

// ── Windows Hello ───────────────────────────────────────────────────────────

#[test]
fn hello_availability_is_reported_honestly() {
    use keyring_lib::platform::windows::winrt::block_on;
    use windows::Security::Credentials::KeyCredentialManager;
    use windows::Security::Credentials::UI::{
        UserConsentVerifier, UserConsentVerifierAvailability,
    };

    let hello = WindowsHello::new();
    assert_eq!(hello.kind(), BiometricKind::WindowsHello);

    // Record what the machine actually offers rather than asserting a value.
    // A log line saying "no biometric device on this runner" is evidence; a
    // test that passes either way without saying which is not.
    //
    // Both APIs are probed because they fail for different reasons and the
    // distinction decides whether biometric unlock is even possible here:
    //
    //   KeyCredentialManager  false           -> no TPM-backed key credential.
    //                                            Also false for a *local*
    //                                            Windows account even with
    //                                            Hello configured.
    //   UserConsentVerifier   DeviceNotPresent -> no fingerprint reader or IR
    //                                            camera at all.
    //                         NotConfiguredForUser -> hardware present,
    //                                            nothing enrolled.
    let key_credential = KeyCredentialManager::IsSupportedAsync()
        .and_then(|op| block_on(&op))
        .unwrap_or(false);
    println!("windows-hello-available={key_credential}");
    println!("key-credential-manager-supported={key_credential}");

    let availability = UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| block_on(&op));
    let described = match availability {
        Ok(UserConsentVerifierAvailability::Available) => "Available",
        Ok(UserConsentVerifierAvailability::DeviceNotPresent) => "DeviceNotPresent",
        Ok(UserConsentVerifierAvailability::NotConfiguredForUser) => "NotConfiguredForUser",
        Ok(UserConsentVerifierAvailability::DisabledByPolicy) => "DisabledByPolicy",
        Ok(UserConsentVerifierAvailability::DeviceBusy) => "DeviceBusy",
        Ok(_) => "Unknown",
        Err(_) => "QueryFailed",
    };
    println!("user-consent-verifier={described}");

    assert_eq!(
        hello.is_available(),
        key_credential,
        "is_available must report what KeyCredentialManager actually says"
    );
}

#[test]
fn unwrapping_without_an_enrolment_is_invalidated_not_a_panic() {
    // The fallback path: no stored wrap means the master password is required.
    // Reachable without a prompt, because it fails before Hello is consulted.
    let dir = tempfile::tempdir().expect("tempdir");
    let hello = WindowsHello::with_store(DpapiStore::with_root(dir.path().to_path_buf()));

    let err = hello
        .unwrap_secret("keyring-test-no-such-enrolment")
        .expect_err("there is no enrolment");
    assert_eq!(
        err,
        BiometricError::Invalidated,
        "a missing wrap must send the caller to the master password"
    );
}

#[test]
fn revoking_a_missing_enrolment_removes_any_stored_wrap() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DpapiStore::with_root(dir.path().to_path_buf());
    store
        .store("keyring-test-revoke", b"a stale wrap")
        .expect("store");

    let hello = WindowsHello::with_store(DpapiStore::with_root(dir.path().to_path_buf()));
    // `revoke` deletes our blob first, so even when the platform half fails —
    // which it does on a machine with no Hello credential — the wrap is gone.
    let _ = hello.revoke("keyring-test-revoke");

    let store = DpapiStore::with_root(dir.path().to_path_buf());
    assert_eq!(
        store.load("keyring-test-revoke").expect("load"),
        None,
        "revoke must remove the stored wrap even if the platform call fails"
    );
}
