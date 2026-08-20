// SPDX-License-Identifier: AGPL-3.0-or-later
//! Win32 clipboard with history and Cloud Clipboard exclusion.
//!
//! SPEC-V1 §8 singles this out: *"The Windows clipboard-history exclusion is
//! easy to miss and quietly defeats clipboard auto-clear. Test it explicitly."*
//!
//! With Clipboard History enabled (Win+V), `EmptyClipboard` removes only the
//! current entry. The password stays in the history list, and with Cloud
//! Clipboard on it has already left the machine. Auto-clear looks like it
//! works and does nothing.
//!
//! Three registered formats prevent that, and all three are needed:
//!
//! | Format | Effect |
//! |---|---|
//! | `ExcludeClipboardContentFromMonitorProcessing` | clipboard monitors skip it |
//! | `CanIncludeInClipboardHistory` = 0 | keeps it out of the Win+V list |
//! | `CanUploadToCloudClipboard` = 0 | keeps it off other devices |
//!
//! Each is a `DWORD` of zero in a `GlobalAlloc` block, handed to
//! `SetClipboardData` in the same open/close pair as the text.

use std::sync::atomic::{AtomicU64, Ordering};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

use crate::platform::clipboard::{Clipboard, ClipboardError};

/// Clipboard format names, as documented by Microsoft.
const EXCLUDE_FROM_MONITOR: &str = "ExcludeClipboardContentFromMonitorProcessing";
const CAN_INCLUDE_IN_HISTORY: &str = "CanIncludeInClipboardHistory";
const CAN_UPLOAD_TO_CLOUD: &str = "CanUploadToCloudClipboard";

/// The real Windows clipboard.
pub struct WindowsClipboard {
    /// Sequence number of our last write, so a clear can tell whether the
    /// clipboard still holds our value.
    last_write: AtomicU64,
}

impl WindowsClipboard {
    /// A handle to the system clipboard.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_write: AtomicU64::new(0),
        }
    }
}

impl Default for WindowsClipboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Null-terminated UTF-16, for the Win32 `W` APIs.
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Register a clipboard format by name, returning its id.
fn register(name: &str) -> u32 {
    let wide = wide(name);
    // SAFETY: `RegisterClipboardFormatW` reads a null-terminated wide string and
    // returns an atom. `wide` is null-terminated by construction above and
    // outlives the call. The function has no other preconditions and cannot
    // fail destructively — it returns 0 on failure, which we treat as "skip".
    unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) }
}

/// An RAII guard so the clipboard is always closed, including on an early
/// return or a panic. Leaving it open blocks every other process on the desktop.
struct ClipboardGuard;

/// Attempts before giving up on a contended clipboard.
const OPEN_ATTEMPTS: u32 = 10;
/// Pause between attempts.
const OPEN_RETRY: std::time::Duration = std::time::Duration::from_millis(10);

impl ClipboardGuard {
    fn open() -> Result<Self, ClipboardError> {
        // Only one process may hold the clipboard at a time, and `OpenClipboard`
        // fails outright rather than waiting. Any other application touching the
        // clipboard in the same instant makes a copy fail, which the user would
        // experience as Trynta randomly refusing to copy their password.
        // Microsoft's own guidance is to retry; a bounded ~100 ms is far below
        // the 30 ms copy budget's tolerance for the rare contended case and far
        // above the microseconds a normal acquisition takes.
        for attempt in 0..OPEN_ATTEMPTS {
            // SAFETY: `OpenClipboard` associates the clipboard with the current
            // task. It is the documented entry point, takes only an optional
            // owner window, and reports failure as an `Err` rather than by
            // exception. On success exactly one `CloseClipboard` follows, from
            // this guard's `Drop`.
            if unsafe { OpenClipboard(Some(HWND::default())) }.is_ok() {
                return Ok(Self);
            }
            if attempt + 1 < OPEN_ATTEMPTS {
                std::thread::sleep(OPEN_RETRY);
            }
        }
        Err(ClipboardError::Unavailable)
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: paired with the successful `OpenClipboard` that produced this
        // guard. Calling it exactly once per open is the documented contract.
        let _ = unsafe { CloseClipboard() };
    }
}

/// Copy `bytes` into a moveable global block and hand ownership to the clipboard
/// under `format`.
///
/// On success the clipboard owns the block and must not be freed by us.
fn set_global(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
    // SAFETY: `GlobalAlloc` with GMEM_MOVEABLE returns a handle to an
    // uninitialised block of the requested size, or an error. We write exactly
    // `bytes.len()` bytes into it below, within that allocation.
    let handle: HGLOBAL = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }
        .map_err(|_| ClipboardError::WriteFailed)?;

    // SAFETY: `GlobalLock` on a handle from `GlobalAlloc` returns a pointer to
    // the start of that block, valid until `GlobalUnlock`. We copy `bytes.len()`
    // bytes into a block allocated with exactly that size, so the write is in
    // bounds and the regions cannot overlap (one is a fresh allocation).
    unsafe {
        let ptr = GlobalLock(handle).cast::<u8>();
        if ptr.is_null() {
            let _ = GlobalFree(Some(handle));
            return Err(ClipboardError::WriteFailed);
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(handle);
    }

    // SAFETY: transfers ownership of `handle` to the clipboard, which frees it.
    // On failure ownership stays with us, so we free it ourselves — freeing a
    // handle the clipboard owns would be a double free, which is why the two
    // branches differ.
    if unsafe { SetClipboardData(format, Some(HANDLE(handle.0))) }.is_err() {
        // SAFETY: the clipboard rejected the handle, so we still own it and must
        // free it. Freeing one the clipboard accepted would be a double free,
        // which is why only this branch does.
        let _ = unsafe { GlobalFree(Some(handle)) };
        return Err(ClipboardError::WriteFailed);
    }
    Ok(())
}

/// Set one of the zero-valued `DWORD` marker formats.
fn set_marker(format: u32) -> Result<(), ClipboardError> {
    if format == 0 {
        // Registration failed. Better to fail the copy than to place a password
        // on a clipboard that will keep it in history.
        return Err(ClipboardError::WriteFailed);
    }
    set_global(format, &0u32.to_ne_bytes())
}

impl Clipboard for WindowsClipboard {
    fn set_secret(&self, value: &str) -> Result<u64, ClipboardError> {
        let text = wide(value);
        let bytes = {
            let mut out = Vec::with_capacity(text.len() * 2);
            for unit in &text {
                out.extend_from_slice(&unit.to_ne_bytes());
            }
            out
        };

        {
            let _guard = ClipboardGuard::open()?;
            // SAFETY: the clipboard is open and owned by this task, which is
            // `EmptyClipboard`'s documented precondition. It frees any handles
            // the previous owner placed.
            unsafe { EmptyClipboard() }.map_err(|_| ClipboardError::WriteFailed)?;

            // Markers first. If any of them fails we abandon the whole write
            // rather than leave a password on the clipboard unprotected — the
            // guard empties nothing on drop, so we clear explicitly.
            let markers = [
                register(EXCLUDE_FROM_MONITOR),
                register(CAN_INCLUDE_IN_HISTORY),
                register(CAN_UPLOAD_TO_CLOUD),
            ];
            for marker in markers {
                if let Err(e) = set_marker(marker) {
                    // SAFETY: clipboard still open and owned by us.
                    let _ = unsafe { EmptyClipboard() };
                    return Err(e);
                }
            }

            if let Err(e) = set_global(CF_UNICODETEXT.0.into(), &bytes) {
                // SAFETY: clipboard still open and owned by us.
                let _ = unsafe { EmptyClipboard() };
                return Err(e);
            }
        }

        // SAFETY: no arguments, no preconditions; returns a monotonically
        // increasing counter that changes on every clipboard update.
        let token = u64::from(unsafe { GetClipboardSequenceNumber() });
        self.last_write.store(token, Ordering::SeqCst);
        Ok(token)
    }

    fn clear_if_ours(&self, token: u64) -> Result<bool, ClipboardError> {
        // SAFETY: see above.
        let current = u64::from(unsafe { GetClipboardSequenceNumber() });
        if current != token {
            // Someone copied something else. Not ours to destroy.
            return Ok(false);
        }

        let _guard = ClipboardGuard::open()?;
        // SAFETY: the clipboard is open and owned by this task.
        unsafe { EmptyClipboard() }.map_err(|_| ClipboardError::WriteFailed)?;
        Ok(true)
    }
}

/// Whether a clipboard format is currently present.
///
/// Exists for the platform test: after a secret write, every exclusion marker
/// must be on the clipboard. "We called `SetClipboardData`" is not evidence that
/// the marker is there.
///
/// # Errors
///
/// Never; returns `false` for an unregisterable name.
#[must_use]
pub fn format_present(name: &str) -> bool {
    let format = register(name);
    if format == 0 {
        return false;
    }
    // SAFETY: takes a format id and returns a bool-ish result. No pointers, and
    // it does not require the clipboard to be open.
    unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

/// The three exclusion format names, for tests and diagnostics.
#[must_use]
pub const fn exclusion_formats() -> [&'static str; 3] {
    [
        EXCLUDE_FROM_MONITOR,
        CAN_INCLUDE_IN_HISTORY,
        CAN_UPLOAD_TO_CLOUD,
    ]
}

/// Read the clipboard's Unicode text, for verification only.
///
/// **Why a read exists in a password manager at all.** Auto-clear was written,
/// documented, defaulted to on — and never scheduled, so the value sat on the system
/// clipboard indefinitely. Nothing caught it because the only thing a test could
/// assert was that a timer had been asked to run. The way to know a clipboard is
/// empty is to read it, so this is the function that lets a test do that.
///
/// It is deliberately not on the [`Clipboard`](crate::platform::clipboard::Clipboard)
/// trait: nothing in the product reads the clipboard, and adding it to the trait
/// would make that capability available to every caller that holds one. It lives
/// here, in the platform module, where `pnpm check:unsafe` already looks.
///
/// Returns `None` when the clipboard holds no text at all.
#[must_use]
pub fn read_text() -> Option<String> {
    let _guard = ClipboardGuard::open().ok()?;

    // SAFETY: the clipboard is open for this thread, which is `GetClipboardData`'s
    // precondition. The returned handle is owned by the clipboard, not by us, so it
    // is read and never freed here.
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0.into()) }.ok()?;
    if handle.is_invalid() {
        return None;
    }

    // SAFETY: `handle` is a valid global memory handle from the clipboard. `GlobalLock`
    // returns a pointer valid until the matching `GlobalUnlock`, which happens below.
    let ptr = unsafe { GlobalLock(HGLOBAL(handle.0)) }.cast::<u16>();
    if ptr.is_null() {
        return None;
    }

    // SAFETY: the clipboard's CF_UNICODETEXT is documented NUL-terminated, so scanning
    // for the terminator stays inside the allocation.
    let mut len = 0usize;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
        // A clipboard entry longer than this is not something a test wrote, and an
        // unbounded scan on a malformed handle is how a read becomes a crash.
        if len > 1_000_000 {
            break;
        }
    }
    // SAFETY: `ptr` is valid for `len` u16 units, established by the scan above.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = String::from_utf16_lossy(slice);

    // SAFETY: matches the `GlobalLock` above.
    let _ = unsafe { GlobalUnlock(HGLOBAL(handle.0)) };
    Some(text)
}
