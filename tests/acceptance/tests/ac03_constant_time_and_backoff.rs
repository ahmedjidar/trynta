//! SPEC-V1 §11: wrong password rejected in constant time; backoff survives a
//! process restart.
//!
//! Constant time is a property of the verifier comparison, not of the whole
//! unlock — unlock is dominated by Argon2 and timing it proves nothing. So the
//! comparison primitive is timed directly, against two input classes that a
//! short-circuiting `==` would separate cleanly: differing in the first byte
//! versus differing only in the last.
//!
//! FROZEN. See `tests/acceptance/API.md`.

use std::hint::black_box;
use std::time::{Duration, Instant};

use keyring_acceptance::{fixture_params, MASTER};
use keyring_crypto::{derive_muk, verifier_from, verify_password, KdfParams, Muk};
use store::{UnlockError, VaultFile};

const BATCHES: usize = 51;
const ITERATIONS: usize = 20_000;

fn muk_for(byte: u8) -> Muk {
    let salt = [byte; 32];
    derive_muk(b"timing-probe", &salt, KdfParams::floor()).expect("derive")
}

/// Median batch duration for comparing `stored` against a candidate that differs
/// from it at `differ_at`.
fn median_batch(stored: &[u8; 32], differ_at: usize) -> Duration {
    let mut candidate = *stored;
    candidate[differ_at] ^= 0xff;
    let probe = Muk::from_key32(keyring_crypto::Key32::from_bytes(candidate));

    let mut samples: Vec<Duration> = Vec::with_capacity(BATCHES);
    for _ in 0..BATCHES {
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            black_box(verify_password(black_box(&probe), black_box(stored)));
        }
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    samples[BATCHES / 2]
}

#[test]
fn verifier_comparison_is_constant_time() {
    let muk = muk_for(0x11);
    let stored = verifier_from(&muk);

    // Sanity: the primitive must actually reject both classes.
    let mut first_byte_wrong = stored;
    first_byte_wrong[0] ^= 0xff;
    let mut last_byte_wrong = stored;
    last_byte_wrong[31] ^= 0xff;
    assert!(!verify_password(
        &Muk::from_key32(keyring_crypto::Key32::from_bytes(first_byte_wrong)),
        &stored
    ));
    assert!(!verify_password(
        &Muk::from_key32(keyring_crypto::Key32::from_bytes(last_byte_wrong)),
        &stored
    ));
    assert!(verify_password(&muk, &stored), "the right key must verify");

    let early = median_batch(&stored, 0).as_secs_f64();
    let late = median_batch(&stored, 31).as_secs_f64();
    let spread = (early - late).abs() / early.max(late);

    assert!(
        spread < 0.25,
        "verifier comparison timing depends on how many leading bytes match \
         (first-byte-differs {early:.6}s vs last-byte-differs {late:.6}s, spread {spread:.3}) \
         — this is a short-circuiting comparison, not ConstantTimeEq"
    );
}

#[test]
fn wrong_password_is_rejected_and_reveals_nothing_about_why() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");

    // Two wrong passwords that differ from the real one in different ways must
    // produce the identical error value.
    let a = file.unlock("completely-different").unwrap_err();
    let b = file.unlock(&MASTER[..MASTER.len() - 1]).unwrap_err();

    assert!(matches!(a, UnlockError::WrongPassword));
    assert!(matches!(b, UnlockError::WrongPassword));
    assert_eq!(
        format!("{a}"),
        format!("{b}"),
        "the error message distinguishes two wrong passwords"
    );
}

#[test]
fn backoff_survives_a_process_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");

    {
        let file = VaultFile::create(&path, MASTER, fixture_params()).expect("create");
        let mut backed_off = false;
        for _ in 0..8 {
            match file.unlock("wrong-password") {
                Err(UnlockError::WrongPassword) => {}
                Err(UnlockError::Backoff { retry_in }) => {
                    assert!(
                        retry_in > Duration::ZERO,
                        "backoff with a zero delay is not backoff"
                    );
                    backed_off = true;
                    break;
                }
                other => panic!("unexpected unlock result: {other:?}"),
            }
        }
        assert!(
            backed_off,
            "repeated wrong passwords never triggered backoff"
        );
    } // every handle dropped — the "restart"

    let file = VaultFile::open(&path).expect("reopen");
    match file.unlock(MASTER) {
        Err(UnlockError::Backoff { retry_in }) => {
            assert!(
                retry_in > Duration::ZERO,
                "backoff state did not survive the restart"
            );
        }
        other => panic!(
            "backoff did not survive a restart — the correct password was accepted \
             or the counter reset: {other:?}"
        ),
    }
}
