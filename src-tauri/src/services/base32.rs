//! RFC 4648 base32 decoding, for TOTP shared secrets.
//!
//! Every authenticator on earth hands out its shared secret as base32, so
//! decoding it is a prerequisite for [`crate::services::totp`]. Hand-written
//! rather than pulled in as a dependency: it is thirty lines of table lookup
//! against a published alphabet, it runs on secret material, and CLAUDE.md §2
//! asks for a reason before every crate that touches secrets. "It saves thirty
//! lines" is not one.
//!
//! Two decisions worth stating:
//!
//! - **Padding is optional and whitespace is ignored.** Real `otpauth://` URIs
//!   in the wild arrive unpadded, and users paste secrets with spaces in them
//!   because that is how issuers print them. Refusing either would be technically
//!   defensible and practically useless.
//! - **Case is folded, but nothing else is guessed.** A character outside the
//!   alphabet is an error rather than a skip. Silently dropping an unexpected
//!   byte would turn a typo into a valid-looking secret that generates wrong
//!   codes forever.
//!
//! Output is `Zeroizing`: the decoded bytes are the TOTP seed, which is a secret
//! of exactly the same weight as the password beside it.

use zeroize::Zeroizing;

/// Why a base32 string did not decode.
///
/// Carries the offending character's *position*, never the character and never
/// any part of the input — the input is a shared secret (CLAUDE.md §4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Base32Error {
    /// A character outside the RFC 4648 alphabet at this 0-based position.
    #[error("the shared secret contains a character that is not valid base32")]
    InvalidCharacter {
        /// Where it was, counting only non-whitespace characters.
        position: usize,
    },

    /// The input decoded to nothing.
    #[error("the shared secret is empty")]
    Empty,

    /// The bit length is not a whole number of bytes plus valid padding.
    #[error("the shared secret is truncated")]
    Truncated,
}

/// Map one character to its 5-bit value.
fn value_of(c: char) -> Option<u8> {
    match c {
        'A'..='Z' => Some(c as u8 - b'A'),
        'a'..='z' => Some(c as u8 - b'a'),
        '2'..='7' => Some(c as u8 - b'2' + 26),
        _ => None,
    }
}

/// Decode RFC 4648 base32 into bytes.
///
/// # Errors
///
/// [`Base32Error::InvalidCharacter`] for anything outside the alphabet,
/// [`Base32Error::Empty`] if nothing decodes, [`Base32Error::Truncated`] if the
/// remaining bits cannot form a byte.
pub fn decode(input: &str) -> Result<Zeroizing<Vec<u8>>, Base32Error> {
    // Pre-sized from the input length so growth cannot orphan a copy of a
    // partially decoded secret (CLAUDE.md §4.5).
    let mut out = Zeroizing::new(Vec::with_capacity(input.len() * 5 / 8 + 1));

    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    let mut position = 0usize;
    let mut saw_padding = false;

    for c in input.chars() {
        if c.is_whitespace() || c == '-' {
            continue;
        }
        if c == '=' {
            saw_padding = true;
            continue;
        }
        if saw_padding {
            // Data after padding means the string was assembled wrong, and a
            // decoder that accepts it will disagree with the issuer's.
            return Err(Base32Error::InvalidCharacter { position });
        }

        let value = value_of(c).ok_or(Base32Error::InvalidCharacter { position })?;
        position += 1;

        buffer = (buffer << 5) | u32::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            let byte = u8::try_from((buffer >> bits) & 0xFF).unwrap_or(0);
            out.push(byte);
        }
    }

    if out.is_empty() {
        return Err(Base32Error::Empty);
    }
    // Leftover bits must be zero padding. Anything else means characters were
    // lost in transit.
    if bits > 0 && (buffer & ((1 << bits) - 1)) != 0 {
        return Err(Base32Error::Truncated);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{decode, Base32Error};

    /// RFC 4648 §10 test vectors.
    #[test]
    fn rfc4648_vectors() {
        for (encoded, expected) in [
            ("MY======", "f"),
            ("MZXQ====", "fo"),
            ("MZXW6===", "foo"),
            ("MZXW6YQ=", "foob"),
            ("MZXW6YTB", "fooba"),
            ("MZXW6YTBOI======", "foobar"),
        ] {
            let got = decode(encoded).expect(encoded);
            assert_eq!(
                got.as_slice(),
                expected.as_bytes(),
                "RFC 4648 vector {encoded}"
            );
        }
    }

    #[test]
    fn padding_is_optional_and_case_is_folded() {
        let padded = decode("MZXW6YTBOI======").expect("padded");
        let bare = decode("MZXW6YTBOI").expect("unpadded");
        let lower = decode("mzxw6ytboi").expect("lowercase");
        assert_eq!(padded.as_slice(), bare.as_slice());
        assert_eq!(padded.as_slice(), lower.as_slice());
    }

    #[test]
    fn separators_people_actually_paste_are_tolerated() {
        let spaced = decode("MZXW 6YTB OI").expect("spaces");
        let dashed = decode("MZXW-6YTB-OI").expect("dashes");
        assert_eq!(spaced.as_slice(), b"foobar");
        assert_eq!(dashed.as_slice(), b"foobar");
    }

    #[test]
    fn a_character_outside_the_alphabet_is_an_error_not_a_skip() {
        // '1', '8', '9' and '0' are deliberately absent from base32 because they
        // look like letters. Dropping them would turn a typo into a plausible
        // secret that generates wrong codes forever.
        for bad in ["MZXW6YTB1I", "MZXW6YTB0I", "MZXW6YTB!I", "MZXW6YTB8I"] {
            assert!(
                matches!(decode(bad), Err(Base32Error::InvalidCharacter { .. })),
                "{bad} should not decode"
            );
        }
    }

    #[test]
    fn an_error_never_quotes_the_secret() {
        let err = decode("MZXW6YTB1I").unwrap_err();
        let rendered = format!("{err} {err:?}");
        assert!(
            !rendered.contains("MZXW") && !rendered.contains('1'),
            "the error rendered part of the secret: {rendered}"
        );
    }

    #[test]
    fn empty_and_truncated_inputs_are_refused() {
        assert_eq!(decode("").unwrap_err(), Base32Error::Empty);
        assert_eq!(decode("=====").unwrap_err(), Base32Error::Empty);
        assert_eq!(decode("A").unwrap_err(), Base32Error::Empty);
        // 'MZXW6YTBOJ' has a non-zero remainder in its trailing bits.
        assert_eq!(decode("MZXW6YTBOJ").unwrap_err(), Base32Error::Truncated);
    }

    #[test]
    fn data_after_padding_is_refused() {
        assert!(matches!(
            decode("MZXW====6YTB"),
            Err(Base32Error::InvalidCharacter { .. })
        ));
    }
}
