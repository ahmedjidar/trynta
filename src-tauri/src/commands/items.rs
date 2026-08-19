//! Item commands (SPEC-V1 §6, §7.1, §7.2).
//!
//! This is the module the security invariants are actually about, so it is worth
//! being explicit about which function does what:
//!
//! | command | returns | plaintext? |
//! |---|---|---|
//! | `items_list` | metadata only, from the in-memory index | no |
//! | `item_get` | metadata plus `{ present: bool }` per secret | no |
//! | `item_reveal_field` | **one** field, for **one** item | yes — the only path |
//! | `item_copy_field` | `()` | no — Rust writes the clipboard |
//! | `item_upsert` / `item_delete` / `item_restore` / `item_toggle_favorite` | ids and flags | no |
//! | `item_activity` | event kinds and timestamps | no |
//!
//! `item_reveal_field` is the sanctioned exception in CLAUDE.md §4.4, and
//! everything about its shape is there to keep it narrow: a closed `SecretField`
//! enum rather than a string, one field per call, no batching, and a rolling rate
//! limit that asks the human to re-authenticate rather than silently serving 500
//! reveals to a script (SPEC-V1 §6).
//!
//! `item_copy_field` is the reason CLAUDE.md §4.3 exists. The decrypted value
//! goes from the store's `Zeroizing<String>` to the OS clipboard inside Rust and
//! the command returns nothing. A version that returned the string "so the
//! frontend can copy it" would defeat the entire design.

// Tauri owns these signatures. `State<'_, T>` is an extractor and must be taken
// by value, and a command parameter has to be an owned deserializable type, so
// `&str` is not on offer. Both trip `needless_pass_by_value`; satisfying it would
// mean not using Tauri's extractors.
#![allow(clippy::needless_pass_by_value)]

use tauri::State;
use uuid::Uuid;

use crate::commands::dto::{
    ActivityEventDto, ItemDetailDto, ItemDraftInput, ItemSummaryDto, ListQueryDto, SecretFieldDto,
};
use crate::commands::AppState;
use crate::error::AppError;
use crate::reveal::Gate;

/// Longest accepted item title.
const MAX_TITLE: usize = 200;

/// Most activity events a single call will return.
///
/// The store clamps to its own retention cap as well; this bound exists so an
/// IPC parameter never sizes an allocation.
const MAX_ACTIVITY: usize = 100;

/// The item list, filtered, searched and sorted (SPEC-V1 §7.1).
///
/// Runs entirely against the in-memory index, which is built from `meta_ct` and
/// never touches `secret_ct` (SPEC-V1 §4.7). No secret field can appear in the
/// result because [`ItemSummaryDto`] has nowhere to put one.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is locked.
#[tauri::command]
pub fn items_list(
    state: State<'_, AppState>,
    query: ListQueryDto,
) -> Result<Vec<ItemSummaryDto>, AppError> {
    let query = query.into();
    let rows = state
        .session
        .with_index(|index| {
            index
                .query(&query)
                .into_iter()
                .map(ItemSummaryDto::from)
                .collect::<Vec<_>>()
        })
        .map_err(AppError::from)?;
    state.session.touch();
    Ok(rows)
}

/// One item's detail: everything except the secrets themselves.
///
/// Secret fields are reported as `{ field, present }`. Knowing that a card has a
/// PIN is what lets the UI render the row; knowing the PIN requires a separate,
/// explicit call.
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::NotFound`], [`AppError::Storage`] or
/// [`AppError::Crypto`].
#[tauri::command]
pub fn item_get(state: State<'_, AppState>, id: Uuid) -> Result<ItemDetailDto, AppError> {
    let meta = state
        .session
        .with_session(|s| s.item_meta(id).map_err(AppError::from))?;
    state.session.touch();
    Ok(ItemDetailDto::from_meta(&meta))
}

/// Reveal exactly one secret field — **the only plaintext path out**.
///
/// Rate limited to 20 in any rolling 60 seconds, globally. Over that, the reveal
/// does not happen and [`AppError::ReauthRequired`] comes back instead; the
/// frontend re-authenticates through `account_reauth` and tries again. The limit
/// is checked *before* the decryption, so a refused call never materialises the
/// value at all.
///
/// The frontend must not persist, cache or store what this returns, and must
/// clear it on blur, navigation or lock (CLAUDE.md §4.4). That is a frontend
/// obligation this signature cannot enforce; what it can do is make the value
/// expensive to get and cheap to drop, which it does.
///
/// # Errors
///
/// [`AppError::ReauthRequired`] when the rolling limit is exceeded,
/// [`AppError::Locked`], [`AppError::NotFound`], [`AppError::NoSuchField`],
/// [`AppError::Storage`] or [`AppError::Crypto`].
#[tauri::command]
pub fn item_reveal_field(
    state: State<'_, AppState>,
    id: Uuid,
    field: SecretFieldDto,
) -> Result<String, AppError> {
    // Two gates, and they are different rules.
    //
    // The rolling limit (§6) exists to notice a *walk* of the vault: twenty reveals
    // in a minute is not how anyone uses a password manager, so it asks once and
    // then lets the user carry on.
    //
    // `require_master_on_reveal` (§7.5) is the user saying they want each reveal
    // confirmed. It has to consume the confirmation, or one password entry would
    // authorise every reveal that followed it and the setting would be decoration —
    // which is what it was: the flag was stored, shown in settings, and never read.
    if crate::commands::settings::reveal_requires_master(&state)?
        && !state.session.take_fresh_reauth()
    {
        return Err(AppError::ReauthRequired);
    }

    if state.session.check_reveal() == Gate::ReauthRequired {
        return Err(AppError::ReauthRequired);
    }

    let value = state.session.with_session(|s| {
        s.item_reveal_field(id, field.into())
            .map_err(AppError::from)
    })?;
    state.session.touch();

    // `value` is `Zeroizing<String>`; this clone is the copy that crosses IPC and
    // the original is wiped when it drops at the end of this scope. There is no
    // way to hand a zeroizing buffer to serde, so the copy is unavoidable — what
    // is avoidable is keeping two live ones, and we do not.
    Ok(value.to_string())
}

/// Copy one secret field to the clipboard, in Rust (CLAUDE.md §4.3).
///
/// The plaintext never enters the webview. It goes from the store's decryption
/// buffer to the OS clipboard with the platform's secrecy markers applied — the
/// macOS concealed type, and on Windows the three clipboard-history exclusion
/// formats without which auto-clear is theatre (SPEC-V1 §8).
///
/// **Not** rate limited by the rolling reveal window, and that is still right: a
/// copy does not put the value on screen, the clipboard holds one value at a time,
/// so twenty copies leave the same single secret exposed as one. That limit exists
/// to notice a walk of the vault through the *readable* path.
///
/// It **is** gated by `require_master_on_reveal`, and that gap was a real hole. The
/// setting is about a secret leaving the vault on demand, and a copy is exactly
/// that — the value lands on the system clipboard, readable by anything. Gating the
/// reveal and not the copy left the control looking like a lock while the door next
/// to it stood open, which is worse than not having it: the user believes they are
/// protected.
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::NotFound`], [`AppError::NoSuchField`],
/// [`AppError::Clipboard`], [`AppError::Storage`] or [`AppError::Crypto`].
#[tauri::command]
pub fn item_copy_field(
    state: State<'_, AppState>,
    id: Uuid,
    field: SecretFieldDto,
) -> Result<(), AppError> {
    // The same gate as `item_reveal_field`, spending the same confirmation. Two
    // separate allowances would let a user confirm once and then both reveal *and*
    // copy, which is two secrets out of the vault for one password entry.
    if crate::commands::settings::reveal_requires_master(&state)?
        && !state.session.take_fresh_reauth()
    {
        return Err(AppError::ReauthRequired);
    }

    let value = state
        .session
        .with_session(|s| s.item_copy_field(id, field.into()).map_err(AppError::from))?;

    let token = state.session.platform().clipboard.set_secret(&value)?;
    // Remembering the token is what makes auto-clear safe: it lets a later clear
    // tell our own write apart from something the user copied since, and leave
    // theirs alone.
    state.session.note_clipboard_write(token);
    schedule_clipboard_clear(&state, token);
    state.session.touch();
    Ok(())
}

/// Start the clipboard auto-clear countdown for a write we just made.
///
/// **This did not exist.** `item_copy_field` wrote the secret, remembered the token so
/// a clear *could* tell its own write apart from the user's, and then nothing ever
/// called the clear. The setting was on by default, said "copied secrets are wiped
/// after 30 seconds", and wiped nothing — the value stayed on the system clipboard
/// until something else replaced it.
///
/// A thread rather than an async task: it needs one sleep and one call, the platform
/// clipboard is blocking anyway, and reaching for a Tokio timer would mean making
/// `tokio` a direct dependency for this (CLAUDE.md §2). The thread exits when it fires.
///
/// Superseded timers are harmless by construction — `clear_clipboard_token` compares
/// the token before doing anything, so an earlier copy's timer cannot wipe a later
/// copy's value.
pub(crate) fn schedule_clipboard_clear(state: &State<'_, AppState>, token: u64) {
    let Ok(settings) = crate::commands::settings::load(state) else {
        // Settings live in the vault. If they cannot be read the vault is locking or
        // locked, and lock clears the clipboard itself.
        return;
    };
    if !settings.clear_clipboard {
        return;
    }
    let seconds = settings.clipboard_seconds;
    let session = std::sync::Arc::clone(&state.session);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(u64::from(seconds)));
        session.clear_clipboard_token(token);
    });
}

/// Create or update an item.
///
/// # Errors
///
/// [`AppError::Invalid`] on an empty or over-long title, [`AppError::Locked`],
/// [`AppError::NotFound`] if the vault or item is missing, [`AppError::Storage`]
/// or [`AppError::Crypto`].
#[tauri::command]
pub fn item_upsert(state: State<'_, AppState>, draft: ItemDraftInput) -> Result<Uuid, AppError> {
    let title = draft.title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE {
        return Err(AppError::Invalid);
    }

    let mut draft: keyring_store::ItemDraft = draft.into();
    draft.title = draft.title.trim().to_owned();

    let id = state
        .session
        .with_session(|s| s.item_upsert(&draft).map_err(AppError::from))?;
    // Rebuilt rather than patched. A write changes title, username, urls, tags
    // and favourite all at once, and an index that drifts from the store is a
    // search that shows the user something that is no longer true.
    state.session.build_index()?;
    state.session.touch();
    Ok(id)
}

/// Soft-delete an item, behind two confirmations that are checked **here**.
///
/// Deleting is the one destructive thing a user can do to their own data from the
/// item pane, and an unlocked vault sitting on an unattended desk is the threat it
/// has to survive. So it takes two things that a passer-by does not have: the master
/// password, and the item's own title typed out.
///
/// Both are verified in Rust rather than in the webview. A confirmation the frontend
/// checks is a confirmation anyone who can call the command skips, which makes it a
/// speed bump rather than a gate — and this command is reachable from the IPC surface
/// whether or not a dialog was ever drawn.
///
/// The password goes through the store's own unlock path, so the comparison is the
/// same constant-time one the lock screen uses and a wrong attempt is seen by the
/// backoff counter. The resulting session is dropped immediately; proving presence
/// does not replace the keys already held.
///
/// The title is compared trimmed and case-insensitively. The point of typing it is
/// that the user has to look at *which* item they are about to destroy and name it —
/// which "GitHub" against "github" satisfies just as well, while a rejection on case
/// only teaches people to copy and paste, defeating the exercise entirely.
///
/// The delete is a soft delete, as it always was: the row keeps a `deleted_at` and
/// `item_restore` puts it back. That is not a reason to make this cheaper to call —
/// nothing in the UI surfaces deleted items yet, so from where the user stands this
/// is permanent.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is locked; [`AppError::WrongPassword`] if the
/// master password does not verify; [`AppError::Invalid`] if the typed title does not
/// match the item's; [`AppError::NotFound`], [`AppError::Storage`] or
/// [`AppError::Crypto`].
#[tauri::command]
pub fn item_delete(
    state: State<'_, AppState>,
    id: Uuid,
    master_password: String,
    confirm_title: String,
) -> Result<(), AppError> {
    if state.session.state() != crate::session::VaultState::Unlocked {
        return Err(AppError::Locked);
    }

    // Read the title first, so a delete against a missing id fails as NotFound
    // rather than burning a password attempt on an item that was never there.
    let meta = state
        .session
        .with_session(|s| s.item_meta(id).map_err(AppError::from))?;

    let file = state.session.file()?;
    match file.unlock(&master_password) {
        Ok(session) => {
            drop(session.into_keys());
            state.session.note_reauth();
        }
        Err(e) => return Err(e.into()),
    }

    if !title_confirms(&confirm_title, &meta.title) {
        return Err(AppError::Invalid);
    }

    state
        .session
        .with_session(|s| s.item_delete(id).map_err(AppError::from))?;
    state.session.build_index()?;
    state.session.touch();
    Ok(())
}

/// Whether the typed title confirms the item's.
///
/// Trimmed and case-insensitive. The point of typing the name is that the user has
/// to look at *which* item they are destroying and name it, which "GitHub" against
/// "github" satisfies as well as an exact match — while rejecting on case only
/// teaches people to copy and paste the name, which defeats the exercise.
///
/// Split out from [`item_delete`] because it is the part with edge cases, and the
/// part a future refactor could get subtly wrong without anything failing loudly.
fn title_confirms(typed: &str, actual: &str) -> bool {
    let typed = typed.trim();
    // An empty title cannot be confirmed by an empty box. Titles are trimmed and
    // rejected empty on write, so this is defence against a vault that predates
    // that rule rather than a reachable state — and getting it wrong would mean a
    // one-click delete on exactly the items whose name gives least warning.
    !typed.is_empty() && typed.eq_ignore_ascii_case(actual.trim())
}

/// Restore a soft-deleted item.
///
/// # Errors
///
/// As [`item_delete`].
#[tauri::command]
pub fn item_restore(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .session
        .with_session(|s| s.item_restore(id).map_err(AppError::from))?;
    state.session.build_index()?;
    state.session.touch();
    Ok(())
}

/// Apply non-secret edits to an item (SPEC-V1 §7.1).
///
/// The detail pane's edit mode. Distinct from `item_upsert` because upsert rebuilds
/// the secret envelope from the draft it is given: routing a title change through it
/// would mean the edit form had to hold the password, and an empty field would wipe
/// the stored one. This path reads the sealed secret and carries it across, so the
/// form never sees it and cannot lose it.
///
/// Returns whether anything changed. A no-op edit does not burn a revision — that
/// number is what the manifest uses to detect a rollback.
///
/// # Errors
///
/// [`AppError::Invalid`] if the title is present but blank or over the limit,
/// [`AppError::NotFound`] if the item does not exist, otherwise as [`item_delete`].
#[tauri::command]
pub fn item_edit_meta(
    state: State<'_, AppState>,
    id: Uuid,
    edits: crate::commands::dto::MetaEditsInput,
) -> Result<bool, AppError> {
    let mut edits = edits;
    if let Some(title) = &edits.title {
        let trimmed = title.trim();
        if trimmed.is_empty() || trimmed.chars().count() > MAX_TITLE {
            return Err(AppError::Invalid);
        }
        edits.title = Some(trimmed.to_owned());
    }

    let edits: keyring_store::MetaEdits = edits.into();
    let changed = state
        .session
        .with_session(|s| s.item_edit_meta(id, &edits).map_err(AppError::from))?;
    if changed {
        // Rebuilt rather than patched, for the same reason as `item_upsert`: title,
        // username and urls are all index inputs, and an index that drifts from the
        // store is a search that shows the user something no longer true.
        state.session.build_index()?;
        state.session.touch();
    }
    Ok(changed)
}

/// Flip an item's favourite flag and report the new value.
///
/// # Errors
///
/// As [`item_delete`].
#[tauri::command]
pub fn item_toggle_favorite(state: State<'_, AppState>, id: Uuid) -> Result<bool, AppError> {
    let now_favorite = state.session.with_session(|s| {
        let current = s.item_meta(id).map_err(AppError::from)?.favorite;
        s.item_set_favorite(id, !current).map_err(AppError::from)?;
        Ok::<bool, AppError>(!current)
    })?;
    state.session.build_index()?;
    state.session.touch();
    Ok(now_favorite)
}

/// Recent activity for one item, newest first (SPEC-V1 §4.3, §7.2).
///
/// Kinds and timestamps. Never which field was involved: a per-field log inside
/// the vault would be a map of which of a user's secrets are the interesting
/// ones.
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::NotFound`], [`AppError::Storage`] or
/// [`AppError::Crypto`].
#[tauri::command]
pub fn item_activity(
    state: State<'_, AppState>,
    id: Uuid,
    limit: usize,
) -> Result<Vec<ActivityEventDto>, AppError> {
    let limit = limit.min(MAX_ACTIVITY);
    let events = state
        .session
        .with_session(|s| s.item_activity(id, limit).map_err(AppError::from))?;
    Ok(events.into_iter().map(ActivityEventDto::from).collect())
}

#[cfg(test)]
mod tests {
    use super::title_confirms;

    #[test]
    fn the_exact_title_confirms() {
        assert!(title_confirms("Northline Bank", "Northline Bank"));
    }

    #[test]
    fn surrounding_whitespace_and_case_do_not_matter() {
        // Typing it back proves the user read which item they are on. Neither of
        // these is evidence they did not.
        assert!(title_confirms("  northline bank  ", "Northline Bank"));
        assert!(title_confirms("NORTHLINE BANK", "Northline Bank"));
        assert!(title_confirms("Northline Bank", "  Northline Bank "));
    }

    #[test]
    fn a_different_title_does_not_confirm() {
        assert!(!title_confirms("Northline", "Northline Bank"));
        assert!(!title_confirms("Northline Bank 2", "Northline Bank"));
        assert!(!title_confirms("NorthlineBank", "Northline Bank"));
    }

    #[test]
    fn an_empty_box_never_confirms() {
        // Including against an empty title, which would otherwise make the
        // confirmation a formality on precisely the items that need it most.
        assert!(!title_confirms("", "Northline Bank"));
        assert!(!title_confirms("   ", "Northline Bank"));
        assert!(!title_confirms("", ""));
        assert!(!title_confirms("  ", "  "));
    }
}
