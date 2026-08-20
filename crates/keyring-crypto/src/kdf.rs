// SPDX-License-Identifier: AGPL-3.0-or-later
//! Argon2id key derivation and bidirectional cost calibration.
//!
//! SPEC-V1 §3.2. Raw output via `hash_password_into` — no PHC strings, so there
//! is no password-hash parser anywhere in the tree.

use std::time::Duration;

use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::error::CryptoError;
use crate::keys::{Key32, Muk};

/// Argon2id cost parameters, stored per vault in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost in KiB.
    pub m_kib: u32,
    /// Time cost (passes).
    pub t: u32,
    /// Parallelism (lanes).
    pub p: u32,
}

impl KdfParams {
    /// SPEC-V1 §3.2 default: m = 65536 KiB, t = 3, p = 4.
    pub const DEFAULT: Self = Self {
        m_kib: 65_536,
        t: 3,
        p: 4,
    };

    /// Hard lower clamp on memory cost. Never go below this, for any reason.
    pub const MIN_M_KIB: u32 = 19_456;

    /// Hard upper clamp on memory cost.
    ///
    /// Exists so a 32-core desktop cannot calibrate its way to a vault its
    /// owner's laptop can barely open (SPEC-V1 §3.2).
    pub const MAX_M_KIB: u32 = 262_144;

    /// Minimum time cost.
    pub const MIN_T: u32 = 2;

    /// The cheapest spec-legal parameter set. Used by tests, which would
    /// otherwise spend most of their runtime in Argon2.
    #[must_use]
    pub const fn floor() -> Self {
        Self {
            m_kib: Self::MIN_M_KIB,
            t: Self::MIN_T,
            p: 1,
        }
    }

    /// Clamp memory and time into the permitted range.
    #[must_use]
    pub const fn clamped(self) -> Self {
        let m_kib = if self.m_kib < Self::MIN_M_KIB {
            Self::MIN_M_KIB
        } else if self.m_kib > Self::MAX_M_KIB {
            Self::MAX_M_KIB
        } else {
            self.m_kib
        };
        let t = if self.t < Self::MIN_T {
            Self::MIN_T
        } else {
            self.t
        };
        let p = if self.p == 0 { 1 } else { self.p };
        Self { m_kib, t, p }
    }

    /// Whether these parameters are within the permitted range.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.m_kib >= Self::MIN_M_KIB
            && self.m_kib <= Self::MAX_M_KIB
            && self.t >= Self::MIN_T
            && self.p >= 1
    }

    /// Whether a vault written with `self` should be upgraded to `current` on
    /// the next successful unlock (SPEC-V1 §3.2).
    #[must_use]
    pub const fn is_weaker_than(self, current: Self) -> bool {
        self.m_kib < current.m_kib || self.t < current.t
    }

    fn to_argon2(self) -> Result<Params, CryptoError> {
        Params::new(self.m_kib, self.t, self.p, Some(32)).map_err(|_| CryptoError::InvalidKdfParams)
    }
}

/// Calibration target: SPEC-V1 §3.2 aims for 700 ms and accepts 400–1200 ms.
pub const TARGET_MS: u64 = 700;
/// Lower bound of the acceptable calibration window.
pub const ACCEPT_MIN_MS: u64 = 400;
/// Upper bound of the acceptable calibration window.
pub const ACCEPT_MAX_MS: u64 = 1200;

/// Derive the Master Unlock Key from the master password and the account salt.
///
/// # Errors
///
/// [`CryptoError::InvalidKdfParams`] if `params` are outside the permitted range,
/// [`CryptoError::KeyDerivation`] if Argon2id itself fails.
pub fn derive_muk(password: &[u8], salt: &[u8; 32], params: KdfParams) -> Result<Muk, CryptoError> {
    if !params.is_valid() {
        return Err(CryptoError::InvalidKdfParams);
    }
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.to_argon2()?);
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password, salt, out.as_mut())
        .map_err(|_| CryptoError::KeyDerivation)?;
    Ok(Muk::from_key32(Key32::from_bytes(*out)))
}

/// Pick a parallelism lane count for this machine.
#[must_use]
pub fn lanes_for(cores: usize) -> u32 {
    u32::try_from(cores.clamp(1, 4)).unwrap_or(1)
}

/// Calibrate the memory cost so a real unlock lands near [`TARGET_MS`].
///
/// Bidirectional: a slow machine calibrates *down* toward
/// [`KdfParams::MIN_M_KIB`] just as a fast one calibrates up toward
/// [`KdfParams::MAX_M_KIB`]. Rev 1 only said "raise `m`", which leaves a low-end
/// machine with an unlock it cannot afford.
///
/// The result is always clamped and always valid, even when the machine is too
/// slow or too fast to land inside the acceptance window — the clamp wins.
#[must_use]
pub fn calibrate(cores: usize) -> KdfParams {
    calibrate_with(cores, |params| {
        let salt = [0x5a_u8; 32];
        let started = std::time::Instant::now();
        // A failure here means we cannot measure, so we keep the current estimate
        // and let the clamp decide. Never silently weaken the cost.
        let _ = derive_muk(b"calibration-probe", &salt, params);
        started.elapsed()
    })
}

/// [`calibrate`] with an injected measurement function, so the search itself is
/// testable without spending seconds in Argon2.
#[must_use]
pub fn calibrate_with<F>(cores: usize, mut measure: F) -> KdfParams
where
    F: FnMut(KdfParams) -> Duration,
{
    const MAX_STEPS: usize = 6;

    let mut params = KdfParams {
        m_kib: KdfParams::DEFAULT.m_kib,
        t: KdfParams::DEFAULT.t,
        p: lanes_for(cores),
    }
    .clamped();

    for _ in 0..MAX_STEPS {
        let elapsed_ms = u64::try_from(measure(params).as_millis()).unwrap_or(u64::MAX);
        if (ACCEPT_MIN_MS..=ACCEPT_MAX_MS).contains(&elapsed_ms) {
            return params;
        }

        // Argon2id cost is very close to linear in the memory parameter, so one
        // proportional step lands near the target and the loop converges fast.
        let scaled = if elapsed_ms == 0 {
            u64::from(KdfParams::MAX_M_KIB)
        } else {
            (u64::from(params.m_kib) * TARGET_MS) / elapsed_ms
        };
        let next = KdfParams {
            m_kib: u32::try_from(scaled).unwrap_or(KdfParams::MAX_M_KIB),
            ..params
        }
        .clamped();

        if next.m_kib == params.m_kib {
            // Converged, or pinned against a clamp. Either way, stop.
            return params;
        }
        params = next;
    }

    params
}
