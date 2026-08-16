//! KDF calibration.
//!
//! SPEC-V1 §3.2: bidirectional, target 700 ms, accept 400–1200 ms, memory clamped
//! to [19456, 262144] KiB. The clamp is the point — a 32-core desktop must not
//! write a vault its owner's laptop can barely open, and a slow laptop must be
//! able to calibrate *down*, which rev 1's "raise `m`" did not allow.
//!
//! The search is exercised with an injected clock, so these tests cost
//! microseconds rather than tens of seconds of real Argon2.

use std::cell::Cell;
use std::time::Duration;

use keyring_crypto::kdf::{lanes_for, ACCEPT_MAX_MS, ACCEPT_MIN_MS};
use keyring_crypto::{calibrate_with, derive_muk, KdfParams};

/// A machine whose Argon2 cost is linear in `m`, at `ns_per_kib` nanoseconds per KiB.
fn machine(ns_per_kib: u64) -> impl FnMut(KdfParams) -> Duration {
    move |p: KdfParams| Duration::from_nanos(u64::from(p.m_kib) * ns_per_kib)
}

fn in_window(d: Duration) -> bool {
    let ms = u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
    (ACCEPT_MIN_MS..=ACCEPT_MAX_MS).contains(&ms)
}

#[test]
fn a_fast_machine_calibrates_up() {
    // 65536 KiB costs ~66 ms here, well under the window.
    let mut m = machine(1_000);
    let params = calibrate_with(8, &mut m);
    assert!(params.m_kib > KdfParams::DEFAULT.m_kib, "did not raise m");
    assert!(params.is_valid());
    assert!(in_window(m(params)) || params.m_kib == KdfParams::MAX_M_KIB);
}

#[test]
fn a_slow_machine_calibrates_down() {
    // 65536 KiB costs ~3.3 s here. Rev 1 only said "raise m", which would have
    // left this machine with an unlock it cannot afford.
    let mut m = machine(50_000);
    let params = calibrate_with(2, &mut m);
    assert!(params.m_kib < KdfParams::DEFAULT.m_kib, "did not lower m");
    assert!(params.is_valid());
    assert!(in_window(m(params)) || params.m_kib == KdfParams::MIN_M_KIB);
}

#[test]
fn an_absurdly_fast_machine_stops_at_the_upper_clamp() {
    let params = calibrate_with(32, machine(1));
    assert_eq!(params.m_kib, KdfParams::MAX_M_KIB);
    assert!(params.is_valid());
}

#[test]
fn an_absurdly_slow_machine_stops_at_the_lower_clamp_and_never_below() {
    let params = calibrate_with(1, machine(10_000_000));
    assert_eq!(params.m_kib, KdfParams::MIN_M_KIB);
    assert!(
        params.m_kib >= KdfParams::MIN_M_KIB,
        "never below the floor"
    );
    assert!(params.is_valid());
}

#[test]
fn a_machine_already_in_the_window_is_left_alone() {
    // 65536 KiB × 10 µs = ~655 ms, inside 400–1200.
    let params = calibrate_with(4, machine(10_000));
    assert_eq!(params.m_kib, KdfParams::DEFAULT.m_kib);
}

#[test]
fn calibration_always_terminates_and_measures_a_bounded_number_of_times() {
    // A pathological machine whose timing bears no relation to `m` must not spin.
    let calls = Cell::new(0usize);
    let params = calibrate_with(4, |_| {
        calls.set(calls.get() + 1);
        Duration::from_millis(if calls.get() % 2 == 0 { 5 } else { 5_000 })
    });
    assert!(
        calls.get() <= 6,
        "calibration measured {} times",
        calls.get()
    );
    assert!(params.is_valid());
}

#[test]
fn a_zero_measurement_does_not_divide_by_zero() {
    let params = calibrate_with(4, |_| Duration::ZERO);
    assert_eq!(params.m_kib, KdfParams::MAX_M_KIB);
    assert!(params.is_valid());
}

#[test]
fn parallelism_is_capped_at_four_lanes_and_never_zero() {
    assert_eq!(lanes_for(0), 1);
    assert_eq!(lanes_for(1), 1);
    assert_eq!(lanes_for(4), 4);
    assert_eq!(lanes_for(32), 4);
    assert_eq!(lanes_for(usize::MAX), 4);
}

// ── The clamp is enforced at derivation, not just at calibration ─────────────

#[test]
fn derivation_refuses_parameters_below_the_floor() {
    let salt = [0x5a; 32];
    let too_little_memory = KdfParams {
        m_kib: KdfParams::MIN_M_KIB - 1,
        t: 3,
        p: 1,
    };
    assert!(derive_muk(b"pw", &salt, too_little_memory).is_err());

    let too_few_passes = KdfParams {
        m_kib: KdfParams::MIN_M_KIB,
        t: 1,
        p: 1,
    };
    assert!(derive_muk(b"pw", &salt, too_few_passes).is_err());

    let too_much_memory = KdfParams {
        m_kib: KdfParams::MAX_M_KIB + 1,
        t: 3,
        p: 1,
    };
    assert!(derive_muk(b"pw", &salt, too_much_memory).is_err());
}

#[test]
fn clamping_is_idempotent_and_always_lands_in_range() {
    for m_kib in [0, 1, 19_455, 19_456, 65_536, 262_144, 262_145, u32::MAX] {
        for t in [0u32, 1, 2, 3] {
            for p in [0u32, 1, 4] {
                let clamped = KdfParams { m_kib, t, p }.clamped();
                assert!(
                    clamped.is_valid(),
                    "{m_kib}/{t}/{p} clamped to an invalid set"
                );
                assert_eq!(clamped.clamped(), clamped, "clamping is not idempotent");
            }
        }
    }
}

#[test]
fn a_weaker_stored_cost_is_detected_for_upgrade_on_next_unlock() {
    // SPEC-V1 §3.2: below the current default on unlock → re-derive and re-wrap
    // as a payload-phase migration.
    assert!(KdfParams::floor().is_weaker_than(KdfParams::DEFAULT));
    assert!(!KdfParams::DEFAULT.is_weaker_than(KdfParams::DEFAULT));
    assert!(!KdfParams {
        m_kib: KdfParams::MAX_M_KIB,
        t: 3,
        p: 4
    }
    .is_weaker_than(KdfParams::DEFAULT));
}

// ── One real derivation, to prove the injected clock is not the only path ────

#[test]
fn a_real_derivation_at_the_floor_produces_a_stable_key() {
    let salt = [0x5a; 32];
    let a = derive_muk(b"generated fixture", &salt, KdfParams::floor()).expect("derive");
    let b = derive_muk(b"generated fixture", &salt, KdfParams::floor()).expect("derive");
    assert_eq!(a.expose(), b.expose(), "derivation is not deterministic");

    let different_salt =
        derive_muk(b"generated fixture", &[0x5b; 32], KdfParams::floor()).expect("derive");
    assert_ne!(a.expose(), different_salt.expose(), "salt is not bound");

    let different_cost = derive_muk(
        b"generated fixture",
        &salt,
        KdfParams {
            m_kib: KdfParams::MIN_M_KIB,
            t: 3,
            p: 1,
        },
    )
    .expect("derive");
    assert_ne!(a.expose(), different_cost.expose(), "cost is not bound");
}
