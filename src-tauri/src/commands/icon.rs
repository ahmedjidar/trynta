//! Custom item icons (ADD-001 tier 2).
//!
//! Three commands, and the shape of them is the point:
//!
//! | command | returns | who reads the file |
//! |---|---|---|
//! | `item_icon` | a `data:` URI for one item, or `null` | — |
//! | `item_set_icon` | the processed size | **Rust** |
//! | `item_clear_icon` | `()` | — |
//!
//! `item_set_icon` opens the file dialog, reads the bytes, decodes, resizes and
//! re-encodes, all inside Rust. The webview never receives the file the user chose and
//! never learns its path. That is not ceremony: a decoder is a parser, an image the user
//! found on the internet is untrusted input, and the webview is the one process in this
//! product that also holds revealed secrets.
//!
//! `item_icon` returns a `data:` URI rather than raw bytes because the production CSP is
//! `img-src 'self' data:` — so a data URI renders in an `<img>`, and an `<img>` cannot
//! execute script even if the bytes were something other than what the sanitiser
//! believes. Two independent reasons the same value is safe to render.

// Tauri owns these signatures. `State<'_, T>` is an extractor and must be taken by
// value, and a command parameter has to be an owned deserializable type.
#![allow(clippy::needless_pass_by_value)]

use tauri::State;
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use crate::commands::dto::IconUploadDto;
use crate::commands::AppState;
use crate::error::AppError;
use crate::services::custom_icon::{self, MAX_UPLOAD_BYTES};

/// The user's icon for one item, as a `data:` URI, or `None` if it has none.
///
/// Decrypts that item's metadata envelope and nothing else. The search index carries
/// only a flag, so the list asks for this once per row that actually has one — which in
/// a normal vault is none of them.
///
/// # Errors
///
/// [`AppError::Locked`] if the vault is not open, [`AppError::NotFound`] if the item is
/// gone, [`AppError::Storage`] on a read failure.
#[tauri::command]
pub fn item_icon(state: State<'_, AppState>, id: Uuid) -> Result<Option<String>, AppError> {
    let stored = state
        .session
        .with_session(|s| s.item_custom_icon(id).map_err(AppError::from))?;

    Ok(stored.map(|icon| {
        // Base64 rather than a percent-encoded SVG: one encoding for all three formats
        // is one thing to get right, and the frontend does not branch.
        let encoded = base64(&icon.bytes);
        format!("data:{};base64,{encoded}", icon.format.media_type())
    }))
}

/// Ask the user for an image and attach it to an item.
///
/// Returns `None` when the dialog is cancelled, which is not an error.
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::NotFound`], [`AppError::Storage`] if the file
/// cannot be read, or [`AppError::Invalid`] if the image is refused — too large, an
/// unsupported format, or an SVG carrying something the allowlist rejects.
#[tauri::command]
pub fn item_set_icon(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<Option<IconUploadDto>, AppError> {
    let Some(chosen) = app
        .dialog()
        .file()
        .set_title("Choose an icon")
        .add_filter("Image", &["svg", "png", "jpg", "jpeg", "webp", "ico"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let Ok(path) = chosen.into_path() else {
        return Err(AppError::Storage);
    };

    // Refuse on the file's length before reading it into memory, so a multi-gigabyte
    // "icon" costs a stat rather than an allocation.
    let length = std::fs::metadata(&path)
        .map_err(|_| AppError::Storage)?
        .len();
    if length > MAX_UPLOAD_BYTES as u64 {
        return Err(AppError::Invalid);
    }

    let raw = std::fs::read(&path).map_err(|_| AppError::Storage)?;
    // The rejection reason is deliberately not carried into the error: every variant of
    // `IconError` describes the file rather than its contents, but mapping them all to
    // one `Invalid` keeps even the shape of the file out of the IPC surface. The UI
    // shows the accepted formats and the size limit, which is what a user can act on.
    let processed = custom_icon::process(&raw).map_err(|_| AppError::Invalid)?;
    let bytes = processed.bytes;

    state.session.with_session(|s| {
        s.item_set_custom_icon(id, Some(processed.icon.clone()))
            .map_err(AppError::from)
    })?;

    // Rebuild the index, or the change is invisible.
    //
    // `items_list` reads the in-memory index built at unlock, not the store, and
    // `has_custom_icon` is one of its columns — it is what makes `IconDto` resolve to
    // `custom`. Writing the icon without rebuilding left the list reporting the *old*
    // icon until the vault was locked and reopened, which is exactly what "it did not
    // reflect instantly" looked like. Every other write path already does this;
    // these two were simply missed.
    state.session.build_index()?;
    state.session.touch();

    Ok(Some(IconUploadDto {
        bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
    }))
}

/// Remove an item's icon, so it falls back to the bundled mark or a generated one.
///
/// # Errors
///
/// [`AppError::Locked`], [`AppError::NotFound`], or [`AppError::Storage`].
#[tauri::command]
pub fn item_clear_icon(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .session
        .with_session(|s| s.item_set_custom_icon(id, None).map_err(AppError::from))?;

    // See `item_set_icon`: the index carries `has_custom_icon`, so without this the
    // list keeps drawing the removed icon until the vault is reopened.
    state.session.build_index()?;
    state.session.touch();
    Ok(())
}

/// Standard base64, no line breaks.
///
/// Hand-written rather than a dependency: it is fourteen lines, it runs on icon bytes
/// that are already public, and a new crate in the tree costs more than that
/// (CLAUDE.md §2).
fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        // RFC 4648 §10. Covers all three padding cases, which is where a hand-written
        // encoder goes wrong.
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected, "{input}");
        }
    }

    #[test]
    fn base64_handles_high_bytes() {
        assert_eq!(base64(&[0xFF, 0xFE, 0xFD]), "//79");
        assert_eq!(base64(&[0x00, 0x00, 0x00]), "AAAA");
    }
}
