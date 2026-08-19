//! `.tryntabak` v1 — **format only**.
//!
//! ADD-003 §④: the byte layout is frozen here, in run 1, while the format work is
//! fresh. Export and restore logic land in run 2 and must not appear in this
//! file. Nothing here reads or writes a file.
//!
//! A backup is a self-contained container. It carries its **own** Argon2id salt
//! and cost, derived from an **independent** user-supplied passphrase — not the
//! vault's master password (SPEC-V1 §7.8). That is deliberate: a backup often
//! outlives the machine it came from, so binding it to the vault's KDF
//! parameters would freeze a 2026 cost into a file opened in 2031.
//!
//! ## Header layout — 228 bytes, fixed width, big-endian
//!
//! ```text
//!   offset  size  field
//!        0     8  magic             "KEYRINGB"
//!        8     2  backup_version    u16   (currently 1)
//!       10     2  envelope_version  u16   the envelope format inside the body
//!       12     4  reserved          u32   must be 0
//!       16    32  account_salt      this backup's own Argon2id salt
//!       48     4  kdf.m_kib         u32
//!       52     4  kdf.t             u32
//!       56     4  kdf.p             u32
//!       60    32  verifier          backup.verify subkey, constant-time compared
//!       92    32  pubkey_ed25519    the account key that signed the manifest
//!      124    64  manifest_sig      Ed25519 over the backup manifest root
//!      188     8  created_at        i64, Unix milliseconds
//!      196    32  header_mac        HMAC-SHA256 over bytes 0..196
//!   ─────────────
//!            228
//! ```
//!
//! Every field is fixed width, so unlike the vault header there is no need for
//! length prefixes to make the encoding unambiguous. The magic at offset 0 is
//! the domain separator: a vault header MAC is computed over a byte string
//! starting `keyring/v1/header`, so no input can be read as both.
//!
//! The keys come from the backup passphrase, through the same HKDF construction
//! as the vault but under distinct info strings, so a backup key can never
//! collide with a vault key.

use subtle::ConstantTimeEq;

use crate::error::CryptoError;
use crate::kdf::KdfParams;
use crate::keys::{Key32, Muk};
use crate::manifest::ManifestEntry;

/// Magic bytes at the start of every `.tryntabak` file.
pub const MAGIC: [u8; 8] = *b"KEYRINGB";

/// The backup container format this build writes and understands.
pub const BACKUP_VERSION: u16 = 1;

/// Bytes covered by [`BackupHeader::mac_input`] — everything but the MAC itself.
pub const HEADER_PREFIX_LEN: usize = 196;

/// Total serialized header length.
pub const HEADER_LEN: usize = HEADER_PREFIX_LEN + 32;

/// Domain prefix for the backup manifest root.
///
/// Distinct from the vault's, so a vault's `manifest_sig` cannot be replayed
/// into a backup container to vouch for a different set of items. Leaf hashes
/// share the vault's domain, because a leaf is a content hash of one ciphertext
/// and reusing it lets an exporter hash each ciphertext once.
pub const DOMAIN_BACKUP_MANIFEST: &[u8] = b"keyring/v1/backup/manifest";

/// Subkeys derived from a backup's own MUK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupSubkey {
    /// Passphrase verifier for the container.
    Verify,
    /// HMAC key over the canonical header.
    Header,
    /// Wraps the per-item keys inside the container body.
    Wrap,
}

impl BackupSubkey {
    /// The HKDF `info` string for this purpose.
    #[must_use]
    pub const fn info(self) -> &'static [u8] {
        match self {
            Self::Verify => b"keyring/v1/backup/verify",
            Self::Header => b"keyring/v1/backup/header",
            Self::Wrap => b"keyring/v1/backup/wrap",
        }
    }
}

/// Derive a backup subkey from the backup's own MUK.
#[must_use]
pub fn derive_backup_subkey(muk: &Muk, which: BackupSubkey) -> Key32 {
    crate::subkey::expand_for(muk.expose(), which.info())
}

/// The value stored in the header's `verifier` field.
#[must_use]
pub fn backup_verifier_from(muk: &Muk) -> [u8; 32] {
    *derive_backup_subkey(muk, BackupSubkey::Verify).expose()
}

/// Check a candidate backup MUK against a stored verifier, in constant time.
#[must_use]
pub fn verify_backup_passphrase(muk: &Muk, stored: &[u8; 32]) -> bool {
    derive_backup_subkey(muk, BackupSubkey::Verify)
        .expose()
        .ct_eq(stored)
        .into()
}

/// Manifest root over the items a backup contains.
///
/// Same shape as [`crate::manifest_root`] — sorted by id, count-prefixed — under
/// the backup domain.
#[must_use]
pub fn backup_manifest_root(entries: &mut [ManifestEntry]) -> [u8; 32] {
    crate::manifest::root_with_domain(DOMAIN_BACKUP_MANIFEST, entries)
}

/// Hash one ciphertext for inclusion in a backup manifest.
///
/// Deliberately identical to [`crate::leaf_hash`] — a leaf is a content hash of
/// one ciphertext, and sharing [`crate::manifest::DOMAIN_LEAF`] lets an exporter
/// hash each ciphertext once for both the vault and the backup manifest.
#[must_use]
pub fn backup_leaf_hash(ciphertext: &[u8]) -> [u8; 32] {
    crate::manifest::leaf_hash(ciphertext)
}

/// A parsed `.tryntabak` v1 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupHeader {
    /// Container format version.
    pub backup_version: u16,
    /// Envelope format used by the ciphertexts in the body.
    pub envelope_version: u16,
    /// This backup's own Argon2id salt.
    pub account_salt: [u8; 32],
    /// This backup's own Argon2id cost.
    pub kdf: KdfParams,
    /// Passphrase verifier.
    pub verifier: [u8; 32],
    /// Public key that signed `manifest_sig`.
    pub pubkey_ed25519: [u8; 32],
    /// Ed25519 signature over [`backup_manifest_root`].
    pub manifest_sig: [u8; 64],
    /// Creation time, Unix milliseconds.
    pub created_at: i64,
}

impl BackupHeader {
    /// The exact bytes the header MAC is computed over: offsets 0..196.
    #[must_use]
    pub fn mac_input(&self) -> [u8; HEADER_PREFIX_LEN] {
        let mut out = [0u8; HEADER_PREFIX_LEN];
        out[0..8].copy_from_slice(&MAGIC);
        out[8..10].copy_from_slice(&self.backup_version.to_be_bytes());
        out[10..12].copy_from_slice(&self.envelope_version.to_be_bytes());
        // 12..16 stays zero: reserved.
        out[16..48].copy_from_slice(&self.account_salt);
        out[48..52].copy_from_slice(&self.kdf.m_kib.to_be_bytes());
        out[52..56].copy_from_slice(&self.kdf.t.to_be_bytes());
        out[56..60].copy_from_slice(&self.kdf.p.to_be_bytes());
        out[60..92].copy_from_slice(&self.verifier);
        out[92..124].copy_from_slice(&self.pubkey_ed25519);
        out[124..188].copy_from_slice(&self.manifest_sig);
        out[188..196].copy_from_slice(&self.created_at.to_be_bytes());
        out
    }

    /// Serialize the full 228-byte header, MAC included.
    #[must_use]
    pub fn to_bytes(&self, header_key: &Key32) -> [u8; HEADER_LEN] {
        let prefix = self.mac_input();
        let mac = crate::manifest::hmac_sha256(header_key, &prefix);
        let mut out = [0u8; HEADER_LEN];
        out[..HEADER_PREFIX_LEN].copy_from_slice(&prefix);
        out[HEADER_PREFIX_LEN..].copy_from_slice(&mac);
        out
    }

    /// Parse a header without authenticating it.
    ///
    /// Structure only. Nothing derived from this is trustworthy until
    /// [`verify_backup_header_mac`] has passed, which is why parsing and
    /// verifying are separate calls that read differently at a review.
    ///
    /// # Errors
    ///
    /// [`CryptoError::MalformedEnvelope`] if the input is the wrong length, the
    /// magic does not match, or `reserved` is non-zero.
    /// [`CryptoError::UnsupportedEnvelopeVersion`] if it was written by a newer
    /// build.
    pub fn parse(bytes: &[u8]) -> Result<(Self, [u8; 32]), CryptoError> {
        if bytes.len() < HEADER_LEN || bytes[0..8] != MAGIC {
            return Err(CryptoError::MalformedEnvelope);
        }
        let be16 = |at: usize| u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        let be32 = |at: usize| {
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };

        let backup_version = be16(8);
        if backup_version != BACKUP_VERSION {
            return Err(CryptoError::UnsupportedEnvelopeVersion {
                found: backup_version,
                supported: BACKUP_VERSION,
            });
        }
        if be32(12) != 0 {
            return Err(CryptoError::MalformedEnvelope);
        }

        let mut account_salt = [0u8; 32];
        account_salt.copy_from_slice(&bytes[16..48]);
        let mut verifier = [0u8; 32];
        verifier.copy_from_slice(&bytes[60..92]);
        let mut pubkey_ed25519 = [0u8; 32];
        pubkey_ed25519.copy_from_slice(&bytes[92..124]);
        let mut manifest_sig = [0u8; 64];
        manifest_sig.copy_from_slice(&bytes[124..188]);
        let mut created = [0u8; 8];
        created.copy_from_slice(&bytes[188..196]);
        let mut mac = [0u8; 32];
        mac.copy_from_slice(&bytes[HEADER_PREFIX_LEN..HEADER_LEN]);

        Ok((
            Self {
                backup_version,
                envelope_version: be16(10),
                account_salt,
                kdf: KdfParams {
                    m_kib: be32(48),
                    t: be32(52),
                    p: be32(56),
                },
                verifier,
                pubkey_ed25519,
                manifest_sig,
                created_at: i64::from_be_bytes(created),
            },
            mac,
        ))
    }
}

/// Verify a backup header MAC in constant time.
///
/// # Errors
///
/// [`CryptoError::BadHeaderMac`] on mismatch. As with the vault header, the
/// caller must refuse to proceed — never partial-restore, never "repair".
pub fn verify_backup_header_mac(
    header_key: &Key32,
    header: &BackupHeader,
    stored: &[u8; 32],
) -> Result<(), CryptoError> {
    let computed = crate::manifest::hmac_sha256(header_key, &header.mac_input());
    if computed.ct_eq(stored).into() {
        Ok(())
    } else {
        Err(CryptoError::BadHeaderMac)
    }
}

/// Derive a backup's MUK from its passphrase and its own salt.
///
/// # Errors
///
/// Propagates [`crate::derive_muk`].
pub fn derive_backup_muk(
    passphrase: &[u8],
    salt: &[u8; 32],
    params: KdfParams,
) -> Result<Muk, CryptoError> {
    crate::derive_muk(passphrase, salt, params)
}
