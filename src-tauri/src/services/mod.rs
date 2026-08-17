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

pub mod base32;
pub mod exact;
pub mod generator;
pub mod report;
pub mod theme;
pub mod totp;
