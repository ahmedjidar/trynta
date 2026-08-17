//! The IPC command surface (SPEC-V1 §6).
//!
//! CLAUDE.md §5: `commands/` orchestrates and never contains business logic.
//! Every function here is thin — it validates, calls into `keyring-store` or a
//! service, and maps the result. If a body starts growing a decision, that
//! decision belongs one layer down where it can be tested without Tauri.
//!
//! Three invariants this module exists to hold:
//!
//! - **`item_reveal_field` is the only plaintext path out** (CLAUDE.md §4.4).
//!   It returns exactly one field, for one item, on explicit user action.
//!   Nothing else here returns a secret, and `items_list` returns metadata only.
//! - **`item_copy_field` never lets the plaintext into the webview**
//!   (CLAUDE.md §4.3). It decrypts in Rust and hands the value straight to the
//!   OS clipboard; the command returns `()`.
//! - **Every command that needs keys goes through `SessionManager::with_session`**,
//!   which fails closed the instant the vault is locked. There is no path here
//!   that reads a cached decryption.
//!
//! Types crossing the boundary are generated into TypeScript by `ts-rs` during
//! `cargo test`, and CI fails on any diff, so the two sides cannot drift.

pub mod account;
pub mod app;
pub mod dto;
pub mod generator;
pub mod items;
pub mod security;
pub mod settings;
pub mod theme;
pub mod totp;
pub mod updates;
pub mod vaults;

use std::path::PathBuf;
use std::sync::Arc;

use crate::session::SessionManager;

/// Everything a command needs, as one Tauri-managed state object.
pub struct AppState {
    /// The lock/unlock state machine and everything it owns.
    pub session: Arc<SessionManager>,
    /// Where the vault file lives (SPEC-V1 §8).
    pub vault_path: PathBuf,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The path is not a secret, but it names the user's home directory, and
        // a Debug print is the kind of thing that ends up in a screenshot.
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

impl AppState {
    /// Wire the session to a vault location.
    #[must_use]
    pub fn new(session: Arc<SessionManager>, vault_path: PathBuf) -> Self {
        Self {
            session,
            vault_path,
        }
    }
}
