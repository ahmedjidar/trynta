// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pure services: logic that needs neither Tauri nor the store.
//!
//! CLAUDE.md §5 puts `generator`, `strength`, `totp`, `breach`, `icons` and
//! `report` here. What they have in common is that they are testable without a
//! vault, a window or a network, and keeping them that way is the point — a
//! generator that needs an unlocked database to test is a generator whose
//! distribution nobody checks.
//!
//! Anything here that handles a secret returns it in a `Zeroizing` buffer and
//! carries no secret in its error type.
//!
//! **One module breaks the no-network rule, and it is the only one that may.**
//! [`hibp`] is the HIBP range transport. It is separate from [`breach`] so that
//! everything privacy-relevant — the SHA-1 split, which five characters may leave
//! the machine, how a response is parsed — stays in a module with no network code
//! in it, and so the pipe can be audited on its own. Nothing else here opens a
//! socket, and [`breach::CachedOnly`] exists so the security report is structurally
//! incapable of reaching one.

pub mod base32;
pub mod breach;
pub mod custom_icon;
pub mod exact;
pub mod generator;
pub mod hibp;
pub mod history;
pub mod icons;
pub mod report;
pub mod settings;
pub mod strength;
pub mod theme;
pub mod totp;
pub mod twofactor;
pub mod updater;
