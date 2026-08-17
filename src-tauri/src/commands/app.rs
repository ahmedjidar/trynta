//! Application-level commands (SPEC-V1 §6, §8).
//!
//! One command, and it exists so that no component ever hardcodes `⌘`. SPEC-V1
//! §8 is explicit: never a literal modifier glyph in source, resolve it through a
//! key-map. This is where that map's single fact comes from.
//!
//! It also replaces `tauri-plugin-os`, which would have cost a capability grant
//! and a dependency to answer the same question.

// Tauri owns these signatures. `State<'_, T>` is an extractor and must be taken
// by value, and a command parameter has to be an owned deserializable type, so
// `&str` is not on offer. Both trip `needless_pass_by_value`; satisfying it would
// mean not using Tauri's extractors.
#![allow(clippy::needless_pass_by_value)]

use tauri::State;

use crate::commands::dto::PlatformInfo;
use crate::commands::AppState;
use crate::error::AppError;
use crate::platform;

/// Platform facts the UI needs.
///
/// # Errors
///
/// Never fails.
#[tauri::command]
pub fn app_platform_info(state: State<'_, AppState>) -> Result<PlatformInfo, AppError> {
    Ok(PlatformInfo {
        os: platform::current_os().to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        modifier_key: platform::modifier_key().to_owned(),
        biometric_label: state
            .session
            .platform()
            .biometrics
            .kind()
            .label()
            .to_owned(),
    })
}
