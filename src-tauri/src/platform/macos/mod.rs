// SPDX-License-Identifier: AGPL-3.0-or-later
//! macOS implementations of the platform traits.
//!
//! Every `unsafe` block in this subtree carries a comment naming the
//! precondition it relies on (CLAUDE.md §7). Nothing outside
//! `src-tauri/src/platform/` may use `unsafe` at all.

pub mod clipboard;
pub mod keychain;
pub mod touch_id;
