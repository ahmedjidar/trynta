// SPDX-License-Identifier: AGPL-3.0-or-later
//! ISO/IEC 7816-4 padding to a 256-byte boundary.
//!
//! SPEC-V1 §3.3 and §4.4: pad the plaintext before encryption so a ciphertext
//! length does not distinguish a short PIN from a long note.
//!
//! **Always at least one byte.** An exact multiple of the block size still gets a
//! full block of padding, otherwise a plaintext ending in `0x80` would be
//! ambiguous with a padded one and unpadding would be a guess.

use crate::error::CryptoError;

/// Padding block size, in bytes.
pub const PAD_BLOCK: usize = 256;

/// The ISO 7816-4 marker byte.
const MARKER: u8 = 0x80;

/// Pad `buf` in place to the next [`PAD_BLOCK`] boundary.
///
/// Appends `0x80` and then `0x00` bytes. Adds between 1 and [`PAD_BLOCK`] bytes,
/// never zero.
pub fn pad(buf: &mut Vec<u8>) {
    let pad_len = PAD_BLOCK - (buf.len() % PAD_BLOCK);
    buf.reserve_exact(pad_len);
    buf.push(MARKER);
    buf.resize(buf.len() + pad_len - 1, 0);
}

/// Strip ISO 7816-4 padding from `buf` in place.
///
/// Only ever called on a plaintext that has already authenticated, so this is a
/// structural check rather than a security boundary — but it still fails closed.
///
/// # Errors
///
/// [`CryptoError::MalformedPadding`] if `buf` is empty, is not a whole number of
/// blocks, or does not end in a valid marker-and-zeroes run.
pub fn unpad(buf: &mut Vec<u8>) -> Result<(), CryptoError> {
    if buf.is_empty() || buf.len() % PAD_BLOCK != 0 {
        return Err(CryptoError::MalformedPadding);
    }

    // Walk back over the zero run, then expect the marker. The run can be at most
    // PAD_BLOCK - 1 bytes long.
    let mut i = buf.len();
    let limit = buf.len().saturating_sub(PAD_BLOCK);
    while i > limit {
        i -= 1;
        match buf.get(i) {
            Some(0x00) => {}
            Some(&MARKER) => {
                buf.truncate(i);
                return Ok(());
            }
            _ => return Err(CryptoError::MalformedPadding),
        }
    }

    Err(CryptoError::MalformedPadding)
}
