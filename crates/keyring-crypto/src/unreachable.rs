//! The one way this crate stops the process.
//!
//! A few library calls return `Result` for a failure mode that cannot occur at
//! the sizes we use — HKDF-Expand rejects output longer than 255 × 32 bytes and
//! we ask for 32; HMAC accepts a key of any length and ours is 32. The type
//! system cannot express "this branch is dead", so the branch exists and
//! something has to be in it.
//!
//! Every option other than stopping is worse:
//!
//! - Returning a fixed fallback would hand back a *predictable key* or a MAC we
//!   did not compute, and the caller would carry on as if it were real.
//! - `unwrap`/`expect`/`panic!` are banned in production paths (CLAUDE.md §7),
//!   and unwinding would let a `catch_unwind` somewhere resume with half-zeroized
//!   state — or carry a partially formatted secret in the panic payload.
//!
//! So: abort. No unwinding, no message, nothing to catch. That is what invariant
//! #10 means by failing closed.
//!
//! Every site routes through [`invariant_violated`], so `grep invariant_violated`
//! finds all of them. `SECURITY.md` lists it.

/// Stop the process because a documented invariant did not hold.
///
/// `#[cold]` and `#[inline(never)]` keep it off the hot path and keep it a single
/// greppable symbol in the binary.
///
/// The `_invariant` argument is never read at runtime — it exists so each call
/// site names the invariant that makes it unreachable, in code rather than in a
/// comment that can drift.
#[cold]
#[inline(never)]
pub(crate) fn invariant_violated(_invariant: &'static str) -> ! {
    std::process::abort()
}
