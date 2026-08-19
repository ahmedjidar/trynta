//! The self-describing envelope every ciphertext is stored in.
//!
//! SPEC-V1 §3.3:
//!
//! ```text
//!   offset  size  field
//!        0     2  envelope_version : u16 big-endian
//!        2    16  key_id
//!       18    24  nonce
//!       42     n  ciphertext (includes the 16-byte Poly1305 tag)
//! ```
//!
//! `envelope_version`, `key_id`, the purpose, the subject and the revision are
//! all bound as associated data (see [`crate::aad`]), so none of them can be
//! altered without failing authentication.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::aad::Aad;
use crate::error::CryptoError;
use crate::keys::Key32;
use crate::padding;
use crate::rng;

/// The envelope format version this build writes and understands.
pub const ENVELOPE_VERSION: u16 = 1;

/// Fixed-size prefix before the ciphertext.
const HEADER_LEN: usize = 2 + 16 + 24;
/// Poly1305 tag length.
const TAG_LEN: usize = 16;
/// `XChaCha20` nonce length.
pub const NONCE_LEN: usize = 24;

/// The shortest byte string that could be a Trynta envelope.
///
/// Not `HEADER_LEN + TAG_LEN`: every plaintext is padded to a whole
/// [`crate::PAD_BLOCK`] and padding is never zero-length, so the smallest
/// ciphertext we ever write is one full block plus the tag. Rejecting anything
/// shorter at parse time turns a confusing authentication failure into an honest
/// structural one.
const MIN_ENVELOPE_LEN: usize = HEADER_LEN + padding::PAD_BLOCK + TAG_LEN;

/// A sealed blob, as stored in a `*_ct` column.
#[derive(Clone, PartialEq, Eq)]
pub struct Envelope {
    /// Format version. Gates every parse.
    pub envelope_version: u16,
    /// The key that sealed this envelope.
    pub key_id: [u8; 16],
    /// Fresh 24-byte nonce from the OS CSPRNG. Never reused under one key.
    pub nonce: [u8; NONCE_LEN],
    /// Ciphertext with its authentication tag appended.
    pub ct: Vec<u8>,
}

impl core::fmt::Debug for Envelope {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Ciphertext is not a secret, but printing kilobytes of it into a log is
        // still how a vault ends up in a bug report.
        f.debug_struct("Envelope")
            .field("envelope_version", &self.envelope_version)
            .field("key_id", &hex_id(&self.key_id))
            .field("nonce", &"<24 bytes>")
            .field("ct", &format_args!("<{} bytes>", self.ct.len()))
            .finish()
    }
}

fn hex_id(id: &[u8; 16]) -> String {
    use core::fmt::Write as _;
    id.iter().fold(String::with_capacity(32), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

impl Envelope {
    /// Serialize to the on-disk byte layout.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.ct.len());
        out.extend_from_slice(&self.envelope_version.to_be_bytes());
        out.extend_from_slice(&self.key_id);
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.ct);
        out
    }

    /// Parse from the on-disk byte layout.
    ///
    /// # Errors
    ///
    /// [`CryptoError::MalformedEnvelope`] if the input is too short to be an
    /// envelope, or [`CryptoError::UnsupportedEnvelopeVersion`] if it was written
    /// by a newer build. Never a best-effort parse (SPEC-V1 §3.3).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() < MIN_ENVELOPE_LEN
            || (bytes.len() - HEADER_LEN - TAG_LEN) % padding::PAD_BLOCK != 0
        {
            return Err(CryptoError::MalformedEnvelope);
        }
        let mut version = [0u8; 2];
        version.copy_from_slice(&bytes[0..2]);
        let envelope_version = u16::from_be_bytes(version);
        if envelope_version != ENVELOPE_VERSION {
            return Err(CryptoError::UnsupportedEnvelopeVersion {
                found: envelope_version,
                supported: ENVELOPE_VERSION,
            });
        }
        let mut key_id = [0u8; 16];
        key_id.copy_from_slice(&bytes[2..18]);
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&bytes[18..HEADER_LEN]);
        Ok(Self {
            envelope_version,
            key_id,
            nonce,
            ct: bytes[HEADER_LEN..].to_vec(),
        })
    }
}

/// Pad, encrypt and wrap `plaintext` under `key`, bound to `aad`.
///
/// # Errors
///
/// [`CryptoError::Rng`] if the OS generator is unavailable,
/// [`CryptoError::Authentication`] if the AEAD refuses the input.
pub fn seal(key: &Key32, aad: &Aad, plaintext: &[u8]) -> Result<Envelope, CryptoError> {
    let mut padded = Zeroizing::new(plaintext.to_vec());
    padding::pad(&mut padded);

    let nonce_bytes: [u8; NONCE_LEN] = rng::array()?;
    let cipher = XChaCha20Poly1305::new(key.expose().into());
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce_bytes),
            Payload {
                msg: &padded,
                aad: &aad.encode(),
            },
        )
        .map_err(|_| CryptoError::Authentication)?;

    Ok(Envelope {
        envelope_version: aad.envelope_version,
        key_id: aad.key_id,
        nonce: nonce_bytes,
        ct,
    })
}

/// Unwrap, decrypt and unpad an envelope under `key`, bound to `aad`.
///
/// The returned buffer zeroizes on drop.
///
/// # Errors
///
/// [`CryptoError::UnsupportedEnvelopeVersion`] if the envelope predates or
/// postdates this build, [`CryptoError::Authentication`] if the key, the
/// ciphertext or any bound field is wrong, [`CryptoError::MalformedPadding`] if
/// the authenticated plaintext is not correctly padded.
pub fn open(
    key: &Key32,
    aad: &Aad,
    envelope: &Envelope,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if envelope.envelope_version != ENVELOPE_VERSION {
        return Err(CryptoError::UnsupportedEnvelopeVersion {
            found: envelope.envelope_version,
            supported: ENVELOPE_VERSION,
        });
    }
    // The AAD must describe this exact envelope. Both fields are also inside the
    // authenticated data, so a mismatch would fail below anyway — checking here
    // turns a confusing auth failure into an honest one.
    if envelope.envelope_version != aad.envelope_version || envelope.key_id != aad.key_id {
        return Err(CryptoError::Authentication);
    }

    let cipher = XChaCha20Poly1305::new(key.expose().into());
    let mut plaintext = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&envelope.nonce),
                Payload {
                    msg: &envelope.ct,
                    aad: &aad.encode(),
                },
            )
            .map_err(|_| CryptoError::Authentication)?,
    );

    padding::unpad(&mut plaintext)?;
    Ok(plaintext)
}
