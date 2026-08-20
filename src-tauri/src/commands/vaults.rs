// SPDX-License-Identifier: AGPL-3.0-or-later
//! Vault commands (SPEC-V1 §4.2, §6, §7.5).
//!
//! A "vault" here is a collection inside one account database, not the file. All
//! five commands are thin wrappers over `keyring-store`; the one decision they
//! make is refusing an empty name, because a nameless vault is unpickable in the
//! UI and the store has no opinion about display strings.

// Tauri owns these signatures. `State<'_, T>` is an extractor and must be taken
// by value, and a command parameter has to be an owned deserializable type, so
// `&str` is not on offer. Both trip `needless_pass_by_value`; satisfying it would
// mean not using Tauri's extractors.
#![allow(clippy::needless_pass_by_value)]

use tauri::State;
use uuid::Uuid;

use crate::commands::dto::VaultSummaryDto;
use crate::commands::AppState;
use crate::error::AppError;

/// Longest accepted vault name.
///
/// Bounded because the name is user input that lands in an encrypted payload and
/// is rendered in a list; an unbounded string is a layout bug and a needlessly
/// large envelope.
const MAX_NAME: usize = 60;

/// Longest accepted colour-token name.
const MAX_TOKEN: usize = 64;

/// Every live vault with its item count.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is locked, [`AppError::Storage`] or
/// [`AppError::Crypto`] on failure.
#[tauri::command]
pub fn vaults_list(state: State<'_, AppState>) -> Result<Vec<VaultSummaryDto>, AppError> {
    let vaults = state
        .session
        .with_session(|s| s.vaults_list().map_err(AppError::from))?;
    Ok(vaults.into_iter().map(VaultSummaryDto::from).collect())
}

/// Create a vault.
///
/// # Errors
///
/// [`AppError::Invalid`] on an empty or over-long name or token,
/// [`AppError::Locked`], [`AppError::Storage`] or [`AppError::Crypto`].
#[tauri::command]
pub fn vault_add(
    state: State<'_, AppState>,
    name: String,
    color_token: String,
) -> Result<Uuid, AppError> {
    let name = validated_name(&name)?;
    let token = validated_token(&color_token)?;
    state
        .session
        .with_session(|s| s.vault_add(name, token).map_err(AppError::from))
}

/// Rename a vault.
///
/// # Errors
///
/// [`AppError::Invalid`] on a bad name, [`AppError::NotFound`] if the vault does
/// not exist, [`AppError::Locked`], [`AppError::Storage`] or
/// [`AppError::Crypto`].
#[tauri::command]
pub fn vault_rename(state: State<'_, AppState>, id: Uuid, name: String) -> Result<(), AppError> {
    let name = validated_name(&name)?;
    state
        .session
        .with_session(|s| s.vault_rename(id, name).map_err(AppError::from))
}

/// Change a vault's colour.
///
/// Takes a token *name* such as `vault.accent.3`, never a colour value — no
/// hardcoded colour may exist anywhere outside the token layer (CLAUDE.md §3),
/// and a colour arriving over IPC would be exactly that.
///
/// # Errors
///
/// [`AppError::Invalid`] on a bad token, [`AppError::NotFound`],
/// [`AppError::Locked`], [`AppError::Storage`] or [`AppError::Crypto`].
#[tauri::command]
pub fn vault_set_color(
    state: State<'_, AppState>,
    id: Uuid,
    color_token: String,
) -> Result<(), AppError> {
    let token = validated_token(&color_token)?;
    state
        .session
        .with_session(|s| s.vault_set_color(id, token).map_err(AppError::from))
}

/// Delete a vault, moving its items into another or deleting them with it.
///
/// # Errors
///
/// [`AppError::NotFound`] if either vault is missing,
/// [`AppError::LastVaultRemaining`] if this is the only vault,
/// [`AppError::Locked`], [`AppError::Storage`] or [`AppError::Crypto`].
#[tauri::command]
pub fn vault_delete(
    state: State<'_, AppState>,
    id: Uuid,
    move_items_to: Option<Uuid>,
) -> Result<(), AppError> {
    state
        .session
        .with_session(|s| s.vault_delete(id, move_items_to).map_err(AppError::from))?;
    // The index is built from the item set, and both branches change it: a move
    // rewrites `vault_id`, a delete removes rows from the live set.
    state.session.build_index()?;
    Ok(())
}

/// A trimmed, length-checked vault name.
fn validated_name(name: &str) -> Result<&str, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME {
        return Err(AppError::Invalid);
    }
    Ok(trimmed)
}

/// A colour *token name*, checked to be one.
///
/// The grammar is deliberately narrow — dotted lowercase segments — so that a
/// colour value cannot pass as a token by accident. `#ff0000`, `rgb(…)` and
/// `url(…)` all fail it.
fn validated_token(token: &str) -> Result<&str, AppError> {
    let trimmed = token.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_TOKEN
        || !trimmed
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        return Err(AppError::Invalid);
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{validated_name, validated_token};

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(validated_name("  Work  ").expect("valid"), "Work");
        assert!(validated_name("   ").is_err());
        assert!(validated_name(&"a".repeat(61)).is_err());
    }

    #[test]
    fn a_colour_value_is_not_a_colour_token() {
        assert_eq!(
            validated_token("vault.accent.3").expect("valid"),
            "vault.accent.3"
        );
        for rejected in [
            "#ff0000",
            "rgb(255,0,0)",
            "url(https://attacker.example)",
            "vault..accent",
            "vault.accent.",
            "",
        ] {
            assert!(
                validated_token(rejected).is_err(),
                "{rejected} passed as a colour token"
            );
        }
    }
}
