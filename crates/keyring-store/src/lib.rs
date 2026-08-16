//! Keyring encrypted store — schema, two-phase migrations, item repository.
//!
//! Implemented in run 1 step 4. The crate exists now so the workspace, the
//! acceptance crate and CI are wired from the first push.
//!
//! Placement note: SPEC-V1 and CLAUDE.md §5 put storage under `src-tauri/src/store/`.
//! It lives here instead so the acceptance tests can exercise it without dragging
//! Tauri and a frontend build into every `cargo test` — the same argument ADD-002
//! Q10 accepted for `keyring-crypto`. Raised for confirmation at the run-1
//! checkpoint; moving it is a `git mv` plus one path in `tests/acceptance/Cargo.toml`.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
