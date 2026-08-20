// SPDX-License-Identifier: AGPL-3.0-or-later
//! Clipboard (SPEC-V1 §8, CLAUDE.md §4.3).
//!
//! Two things make this a hand-written platform module rather than a plugin.
//!
//! **The plaintext must not enter the webview.** A copy goes from the Rust
//! decryption buffer to the OS clipboard directly. No available Tauri clipboard
//! plugin can do that — they all expose a JavaScript API, which is the opposite
//! of the requirement.
//!
//! **Windows Clipboard History quietly defeats auto-clear.** If history is on,
//! clearing the clipboard removes only the *current* entry; the password stays
//! in the Win+V list and syncs to Cloud Clipboard. SPEC-V1 §8 calls this out
//! explicitly, and it is invisible in testing unless you have history enabled.
//! The Windows implementation therefore sets `ExcludeClipboardContentFrom-
//! MonitorProcessing`, `CanIncludeInClipboardHistory` = 0 and
//! `CanUploadToCloudClipboard` = 0 alongside the text. macOS has the analogous
//! `org.nspasteboard.ConcealedType` convention.

use thiserror::Error;

/// Why a clipboard operation failed.
///
/// Carries no data: a clipboard error must never quote what we were trying to
/// copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ClipboardError {
    /// The clipboard could not be opened — usually another process holds it.
    #[error("the clipboard is unavailable")]
    Unavailable,

    /// The platform refused the write.
    #[error("the clipboard write failed")]
    WriteFailed,
}

/// Write secrets to, and clear them from, the system clipboard.
pub trait Clipboard: Send + Sync {
    /// Place `value` on the clipboard marked as sensitive.
    ///
    /// Returns an opaque ownership token identifying this write. The caller
    /// keeps it and hands it back to [`Clipboard::clear_if_ours`] so a clear
    /// cannot destroy something the user copied afterwards.
    ///
    /// # Errors
    ///
    /// [`ClipboardError::Unavailable`] if the clipboard cannot be opened,
    /// [`ClipboardError::WriteFailed`] if the platform refuses the write.
    fn set_secret(&self, value: &str) -> Result<u64, ClipboardError>;

    /// Clear the clipboard, but only if it still holds the write identified by
    /// `token`.
    ///
    /// Returns whether anything was cleared. A `false` return is the normal,
    /// correct outcome when the user has copied something else in the meantime
    /// — wiping their shopping list because a password timer expired would be
    /// its own kind of bug.
    ///
    /// # Errors
    ///
    /// [`ClipboardError::Unavailable`] if the clipboard cannot be opened.
    fn clear_if_ours(&self, token: u64) -> Result<bool, ClipboardError>;
}

/// Default clipboard auto-clear delay (SPEC-V1 §7.5: on by default, 30 s).
pub const DEFAULT_CLEAR_SECONDS: u64 = 30;
