// SPDX-License-Identifier: AGPL-3.0-or-later
//! The only source of randomness in the product.
//!
//! SPEC-V1 §3.2: OS CSPRNG only. Never a seeded or userspace PRNG for anything
//! security-relevant. Routing every caller through this one function makes that
//! auditable by grep rather than by inspection.

use rand_core::{OsRng, RngCore};

use crate::error::CryptoError;

/// Fill `dest` with bytes from the operating system CSPRNG.
///
/// # Errors
///
/// [`CryptoError::Rng`] if the OS generator is unavailable. There is no fallback:
/// generating a key from a degraded source is worse than not generating one.
pub fn fill(dest: &mut [u8]) -> Result<(), CryptoError> {
    OsRng.try_fill_bytes(dest).map_err(|_| CryptoError::Rng)
}

/// A fresh array of random bytes from the OS CSPRNG.
///
/// # Errors
///
/// [`CryptoError::Rng`] if the OS generator is unavailable.
pub fn array<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut out = [0u8; N];
    fill(&mut out)?;
    Ok(out)
}
