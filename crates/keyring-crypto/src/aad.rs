//! The canonical 43-byte associated-data encoding.
//!
//! SPEC-V1 §3.3: fixed order, fixed width, big-endian, no separators. Written
//! down once, here, and never deviated from — two implementations that disagree
//! about this cannot read each other's vaults, and the failure mode is "nobody
//! can open anything".
//!
//! ```text
//!   offset  size  field
//!        0     2  envelope_version : u16 big-endian
//!        2     1  purpose          : u8  discriminant
//!        3    16  subject_id       : item id, vault id, or 16 zero bytes
//!       19     8  revision         : u64 big-endian
//!       27    16  key_id           : the key that encrypted this envelope
//!   ─────────────
//!             43  bytes exactly
//! ```
//!
//! Binding `purpose` stops a `secret_ct` being served as a `meta_ct`. Binding
//! `subject_id` stops a ciphertext being moved between items. Binding `revision`
//! stops an *in-place* rollback — but not a whole-row restore, which is what the
//! signed manifest in [`crate::manifest`] is for.

/// Length of the canonical AAD encoding, in bytes.
pub const AAD_LEN: usize = 43;

/// What an envelope holds. The discriminant is part of the on-disk format and
/// must never be renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Purpose {
    /// An item's non-secret metadata (SPEC-V1 §3.4).
    ItemMeta = 1,
    /// An item's secret fields (SPEC-V1 §3.4).
    ItemSecret = 2,
    /// A vault's name, colour token and kind.
    VaultMeta = 3,
    /// One activity event.
    Activity = 4,
    /// HIBP prefix cache or generator history, under `muk.appcache`.
    AppCache = 5,
    /// A `.keyringbackup` payload.
    Backup = 6,
}

impl Purpose {
    /// The on-disk discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// A `subject_id` for envelopes that belong to no item or vault.
pub const NO_SUBJECT: [u8; 16] = [0u8; 16];

/// The associated data bound to one envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aad {
    /// Envelope format version. Must match the envelope's own field.
    pub envelope_version: u16,
    /// What this envelope holds.
    pub purpose: Purpose,
    /// The item or vault this envelope belongs to, or [`NO_SUBJECT`].
    pub subject_id: [u8; 16],
    /// The revision this ciphertext was written at.
    pub revision: u64,
    /// The key that encrypted it.
    pub key_id: [u8; 16],
}

impl Aad {
    /// Encode to the canonical 43-byte form.
    #[must_use]
    pub fn encode(&self) -> [u8; AAD_LEN] {
        let mut out = [0u8; AAD_LEN];
        out[0..2].copy_from_slice(&self.envelope_version.to_be_bytes());
        out[2] = self.purpose.as_u8();
        out[3..19].copy_from_slice(&self.subject_id);
        out[19..27].copy_from_slice(&self.revision.to_be_bytes());
        out[27..43].copy_from_slice(&self.key_id);
        out
    }
}
