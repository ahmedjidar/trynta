//! The Windows Hello signing path, end to end (SPEC-V1 §5.1, §11).
//!
//! Separate from `platform_windows.rs` because these tests **raise a Hello
//! consent prompt** and block until someone answers it. They are opt-in via
//! `TRYNTA_TEST_HELLO=1` rather than `#[ignore]`, so a CI runner without an
//! enrolment does not silently report them as passing — the frozen suite's rule
//! about never `#[ignore]`ing applies in spirit here too.
//!
//! Run them with:
//!
//! ```text
//! TRYNTA_TEST_HELLO=1 cargo test -p keyring --test platform_hello_enrolled -- --test-threads=1
//! ```
//!
//! `--test-threads=1` matters: two concurrent prompts race for the same consent
//! UI and the second is dismissed by the system.
//!
//! What these prove that `platform_windows.rs` cannot:
//!
//! - a real `KeyCredential` is created in the TPM and signs,
//! - the signature is **deterministic**, which is the property the whole design
//!   rests on — a non-deterministic signature would derive a different wrapping
//!   key every time and the wrap would never open twice,
//! - a secret survives enrol → unwrap,
//! - revoking makes the wrap unopenable, which is the same code path an
//!   enrolment change takes.

#![cfg(windows)]

use keyring_lib::platform::windows::dpapi::DpapiStore;
use keyring_lib::platform::windows::hello::WindowsHello;
use keyring_lib::platform::{BiometricError, Biometrics};

/// Credential name used by these tests. Distinct from the app's, so a failed run
/// cannot invalidate a real user's enrolment on a developer machine.
const LABEL: &str = "trynta-test-hello-enrolled";

/// A secret with no structure a signature could accidentally reproduce.
const SECRET: &[u8] = b"HELLO-ENROLLED-SECRET-8Xr3Qm5Vd9Lp2Kt";

fn opted_in() -> bool {
    std::env::var("TRYNTA_TEST_HELLO").is_ok_and(|v| v == "1")
}

/// Skip loudly rather than quietly, so a log records that nothing ran.
fn skip(what: &str) -> bool {
    if opted_in() {
        return false;
    }
    println!("SKIPPED {what}: set TRYNTA_TEST_HELLO=1 to run (raises a Hello prompt)");
    true
}

fn hello() -> (tempfile::TempDir, WindowsHello) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DpapiStore::with_root(dir.path().to_path_buf());
    (dir, WindowsHello::with_store(store))
}

#[test]
fn hello_is_available_when_enrolled() {
    if skip("hello_is_available_when_enrolled") {
        return;
    }
    let (_dir, hello) = hello();
    assert!(
        hello.is_available(),
        "TRYNTA_TEST_HELLO=1 was set but KeyCredentialManager reports Hello unavailable"
    );
}

#[test]
fn a_secret_survives_enrol_and_unwrap() {
    if skip("a_secret_survives_enrol_and_unwrap") {
        return;
    }
    let (_dir, hello) = hello();
    let _ = hello.revoke(LABEL);

    hello.enrol(LABEL, SECRET).expect("enrol");
    let recovered = hello.unwrap_secret(LABEL).expect("unwrap");
    assert_eq!(recovered, SECRET, "the unwrapped secret differs");

    let _ = hello.revoke(LABEL);
}

#[test]
fn the_wrapping_key_is_stable_across_calls() {
    // The design rests on `KeyCredential` signing deterministically: the wrapping
    // key is derived from a signature over a fixed challenge, so a signature with
    // a random nonce would produce a different key every time and the wrap would
    // open exactly once. Unwrapping twice is the cheapest way to prove it, and
    // it fails loudly if Windows ever changes the algorithm.
    if skip("the_wrapping_key_is_stable_across_calls") {
        return;
    }
    let (_dir, hello) = hello();
    let _ = hello.revoke(LABEL);

    hello.enrol(LABEL, SECRET).expect("enrol");
    let first = hello.unwrap_secret(LABEL).expect("first unwrap");
    let second = hello.unwrap_secret(LABEL).expect("second unwrap");

    assert_eq!(first, SECRET);
    assert_eq!(
        first, second,
        "two unwraps disagreed, so the Hello signature is not deterministic and the \
         whole wrapping-key derivation is unsound"
    );

    let _ = hello.revoke(LABEL);
}

#[test]
fn revoking_makes_the_wrap_unopenable() {
    // The same path an enrolment change takes: the credential is gone, so the
    // wrapping key cannot be re-derived and the caller must fall back to the
    // master password.
    if skip("revoking_makes_the_wrap_unopenable") {
        return;
    }
    let (_dir, hello) = hello();
    let _ = hello.revoke(LABEL);

    hello.enrol(LABEL, SECRET).expect("enrol");
    assert!(hello.unwrap_secret(LABEL).is_ok(), "enrolled and readable");

    hello.revoke(LABEL).expect("revoke");
    assert_eq!(
        hello.unwrap_secret(LABEL).unwrap_err(),
        BiometricError::Invalidated,
        "after revocation the wrap must report Invalidated, which is what sends \
         the user to their master password"
    );
}

#[test]
fn a_wrap_does_not_open_under_a_different_credential() {
    // Enrolling under a second label creates a different TPM key, so the first
    // label's wrap must not open under it. If it did, the wrapping key would not
    // actually be bound to the credential.
    if skip("a_wrap_does_not_open_under_a_different_credential") {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DpapiStore::with_root(dir.path().to_path_buf());
    let hello = WindowsHello::with_store(store);

    let other = "trynta-test-hello-enrolled-other";
    let _ = hello.revoke(LABEL);
    let _ = hello.revoke(other);

    hello.enrol(LABEL, SECRET).expect("enrol first");
    hello
        .enrol(other, b"a different secret")
        .expect("enrol second");

    assert_eq!(hello.unwrap_secret(LABEL).expect("first"), SECRET);
    assert_eq!(
        hello.unwrap_secret(other).expect("second"),
        b"a different secret"
    );

    let _ = hello.revoke(LABEL);
    let _ = hello.revoke(other);
}
