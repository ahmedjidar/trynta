// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pinning key pages into RAM (CLAUDE.md §4.5).
//!
//! > `mlock`/`VirtualLock` the 32-byte keys on both platforms; log a warning if
//! > unavailable, never fail silently. **The Argon2 memory buffer is a documented
//! > exception** — it is far larger than the lockable working set and may be paged.
//!
//! ## What this buys, stated narrowly
//!
//! Locking a page tells the OS not to write it to the page file while it is resident.
//! It closes one specific hole: a machine that swaps or hibernates writing a live
//! session's keys to disk, where they outlive the process and survive a reboot.
//!
//! ## What it does not buy, stated just as plainly
//!
//! This is the part that is easy to overclaim, and the previous version of
//! `SECURITY.md` did overclaim it — it said keys were locked when nothing in the build
//! called `VirtualLock` at all. So:
//!
//! - **It pins an address, not a value.** `Key32` holds `Zeroizing<[u8; 32]>`; if that
//!   value is moved or cloned, the copy lives at an address nobody locked. We lock the
//!   buffers the live session owns, at the moment it adopts them, and that is the
//!   extent of it.
//! - **It cannot retroactively protect derivation.** The master password arrives as an
//!   unzeroizable JavaScript string and the MUK is derived through an Argon2 buffer far
//!   too large to lock. Both are documented exposures in `SECURITY.md`.
//! - **A hibernation image is not covered.** `VirtualLock` keeps a page out of the page
//!   file; it does not keep it out of `hiberfil.sys`, which is a full memory dump by
//!   design.
//! - **It is best-effort.** It can fail — most plausibly on the working-set quota — and
//!   when it does the app logs and carries on, because refusing to unlock a vault
//!   because a page could not be pinned would trade a real feature for a partial
//!   mitigation.
//!
//! Pages are locked and never unlocked. `VirtualUnlock` on a page that still holds a
//! key would undo the thing this exists for, and the process exiting releases every
//! lock it holds. The cost is bounded: a handful of 4 KiB pages for the life of a
//! session.

use std::fmt;

/// Why a lock attempt failed.
///
/// Deliberately does not carry the address or the length. An error that named where a
/// key lives would be a worse leak than the paging it is trying to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLockError {
    /// The platform refused the lock. Carries the OS error code, which is about the
    /// call and not about the contents.
    Refused(u32),
    /// This build has no implementation for the host platform.
    Unsupported,
}

impl fmt::Display for MemoryLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(code) => write!(
                f,
                "the operating system refused to lock the page (os error {code})"
            ),
            Self::Unsupported => f.write_str("this build cannot lock pages on this platform"),
        }
    }
}

impl std::error::Error for MemoryLockError {}

/// Pin the pages backing `bytes` into RAM.
///
/// An empty slice succeeds without calling anything — there is no page to pin, and
/// `VirtualLock` with a zero size is an error rather than a no-op.
///
/// # Errors
///
/// [`MemoryLockError::Refused`] with the OS error code, or
/// [`MemoryLockError::Unsupported`] on a platform this build does not implement.
pub fn lock_pages(bytes: &[u8]) -> Result<(), MemoryLockError> {
    if bytes.is_empty() {
        return Ok(());
    }
    imp::lock_pages(bytes)
}

/// Lock every key region a session holds, logging rather than failing.
///
/// Called from `SessionManager::adopt`, which is the one moment the keys are known to
/// be live and at a settled address. Returns how many regions were pinned, for the
/// test that asserts this is actually wired up rather than merely present.
pub fn lock_session_keys<F>(for_each_region: F) -> usize
where
    F: FnOnce(&mut dyn FnMut(&[u8])),
{
    let mut locked = 0usize;
    let mut failures = 0usize;
    for_each_region(&mut |region: &[u8]| match lock_pages(region) {
        Ok(()) => locked += 1,
        Err(e) => {
            failures += 1;
            // Never silently. §4.5 says warn, and the warning names the error and
            // nothing about the key.
            tracing::warn!(
                error = %e,
                "could not pin a key page into RAM; key material may reach the page file"
            );
        }
    });
    if failures == 0 && locked > 0 {
        tracing::debug!(regions = locked, "key pages pinned into RAM");
    }
    locked
}

#[cfg(windows)]
mod imp {
    use super::MemoryLockError;
    use windows::Win32::System::Memory::VirtualLock;

    pub fn lock_pages(bytes: &[u8]) -> Result<(), MemoryLockError> {
        // SAFETY: `VirtualLock` takes a pointer and a byte count describing a region
        // the caller must own. `bytes` is a live shared borrow, so the region is
        // mapped, readable and owned by this process for at least the duration of the
        // call, and the length is exactly that slice's length. `VirtualLock` reads no
        // memory and writes none — it only changes the pages' residency — so a shared
        // borrow is sufficient and no aliasing rule is engaged. The pointer cast is
        // `*const u8` to `*const c_void`, which changes no provenance.
        //
        // We never call `VirtualUnlock`. That is deliberate, not an oversight: see the
        // module note.
        let result = unsafe { VirtualLock(bytes.as_ptr().cast(), bytes.len()) };
        result.map_err(|e| {
            // `HRESULT::0` carries the Win32 code in its low word for these APIs.
            MemoryLockError::Refused(e.code().0.unsigned_abs())
        })
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::MemoryLockError;
    use std::ffi::{c_int, c_void};

    // Declared here rather than pulled from `libc`. CLAUDE.md §2 says to stop and ask
    // before adding a dependency that touches memory, and one POSIX function with a
    // stable, forty-year-old signature does not justify a crate. `mlock` is in libSystem,
    // which every macOS binary already links.
    //
    // Signature from `man 2 mlock`: `int mlock(const void *addr, size_t len);`
    //
    // UNVERIFIED: never compiled. See MACOS-UNVERIFIED.md row E4.
    //
    // SAFETY: the invariant an `extern` block carries is that the declaration matches
    // the symbol the linker resolves. `mlock` is POSIX and its signature has been
    // `int mlock(const void *, size_t)` since 4.4BSD; `c_void`, `usize` and `c_int` are
    // the Rust spellings of `void`, `size_t` and `int` on every macOS target. A mismatch
    // here would be a calling-convention error, which is why the types are spelled with
    // `std::ffi` aliases rather than concrete widths.
    unsafe extern "C" {
        fn mlock(addr: *const c_void, len: usize) -> c_int;
    }

    pub fn lock_pages(bytes: &[u8]) -> Result<(), MemoryLockError> {
        // SAFETY: `mlock` takes a pointer and a length describing a mapped region the
        // caller owns. `bytes` is a live shared borrow, so for the duration of the call
        // the region is mapped, readable, and owned by this process, and the length is
        // exactly that slice's length. `mlock` reads none of the region's bytes and
        // writes none — it changes only residency — so a shared borrow suffices and no
        // aliasing rule is engaged. The cast is `*const u8` to `*const c_void`, which
        // changes no provenance.
        //
        // We never call `munlock`, for the reason in the module note.
        let rc = unsafe { mlock(bytes.as_ptr().cast(), bytes.len()) };
        if rc == 0 {
            Ok(())
        } else {
            // `errno`, via the std wrapper. It describes the call, not the contents.
            let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
            Err(MemoryLockError::Refused(code.unsigned_abs()))
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::MemoryLockError;

    pub fn lock_pages(_bytes: &[u8]) -> Result<(), MemoryLockError> {
        Err(MemoryLockError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::{lock_pages, lock_session_keys};

    #[test]
    fn an_empty_region_is_a_no_op_rather_than_an_error() {
        // `VirtualLock` with a zero length fails. Callers iterate over key regions and
        // should not have to special-case a degenerate one.
        assert!(lock_pages(&[]).is_ok());
    }

    #[test]
    fn a_real_buffer_locks_on_this_platform() {
        // Windows is the verified platform, so on Windows this must actually succeed —
        // a build where `VirtualLock` silently never works is the bug this whole module
        // exists to fix, and it would otherwise look identical to a working one.
        let key = [0x5au8; 32];
        let result = lock_pages(&key);
        if cfg!(windows) {
            assert!(
                result.is_ok(),
                "VirtualLock refused a 32-byte buffer: {result:?}"
            );
        }
    }

    #[test]
    fn locking_the_same_page_twice_is_not_an_error() {
        // The MUK and the account keys can land on one page. Locking it twice has to be
        // fine, or which keys get pinned would depend on the allocator.
        let key = [0x11u8; 32];
        assert!(lock_pages(&key).is_ok() || !cfg!(windows));
        assert!(lock_pages(&key).is_ok() || !cfg!(windows));
    }

    #[test]
    fn every_region_offered_is_attempted_and_counted() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let locked = lock_session_keys(|visit| {
            visit(&a);
            visit(&b);
        });
        if cfg!(windows) {
            assert_eq!(locked, 2, "both key regions should have been pinned");
        }
    }

    #[test]
    fn a_failure_is_counted_but_does_not_panic() {
        // An empty region cannot fail, so this asserts the shape rather than a failure:
        // the helper must return a count and never propagate.
        assert_eq!(lock_session_keys(|visit| visit(&[])), 1);
    }
}
