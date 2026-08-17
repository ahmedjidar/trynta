//! Generator history (SPEC-V1 §7.3).
//!
//! > **History** — real secrets, so: ≤20 entries, auto-expire at 7 days,
//! > encrypted under `muk.appcache`, optionally wiped on lock, clearable in one
//! > action.
//!
//! "Real secrets" is the whole design constraint. A generator history is a list
//! of passwords the user may since have set on real accounts, so it is exactly as
//! sensitive as the vault it sits beside — and unlike an item, nothing in the
//! product needs it. That asymmetry is why the retention rules are tight and why
//! this type zeroizes.
//!
//! Two caps, and they do different jobs. The **count** cap bounds how much is at
//! risk at any moment. The **age** cap bounds how long a value the user has
//! already used stays recoverable. Neither substitutes for the other: twenty
//! entries generated in a minute all expire together, and one entry generated
//! eight days ago is gone even though the list is nearly empty.
//!
//! Pruning happens on **read as well as write**. A vault that is opened but never
//! generates anything would otherwise keep eight-day-old entries indefinitely,
//! and "auto-expire at 7 days" would be true only for people who kept using the
//! generator.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Entries retained (SPEC-V1 §7.3).
pub const MAX_ENTRIES: usize = 20;

/// How long an entry survives, in milliseconds (SPEC-V1 §7.3: 7 days).
pub const MAX_AGE_MS: i64 = 7 * 86_400_000;

/// What produced an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneratedKind {
    /// A random password.
    Password,
    /// An EFF-wordlist passphrase.
    Passphrase,
    /// A numeric PIN.
    Pin,
}

/// One generated value.
///
/// `Debug` is manual and redacting: this holds a password, and §4.6 applies to it
/// exactly as it applies to an item's.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct HistoryEntry {
    /// Stable id, so the UI can address an entry without holding its value.
    #[zeroize(skip)]
    pub id: Uuid,
    /// The generated value. The secret.
    pub value: String,
    /// What produced it.
    #[zeroize(skip)]
    pub kind: GeneratedKind,
    /// The entropy reported when it was generated.
    #[zeroize(skip)]
    pub entropy_bits: u32,
    /// When it was generated, Unix milliseconds.
    #[zeroize(skip)]
    pub created_at: i64,
}

impl std::fmt::Debug for HistoryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryEntry")
            .field("id", &self.id)
            .field("value", &"<redacted>")
            .field("kind", &self.kind)
            .field("entropy_bits", &self.entropy_bits)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// The stored history, newest first.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct History {
    /// Entries, newest first.
    pub entries: Vec<HistoryEntry>,
}

impl std::fmt::Debug for History {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The count is not a secret; the values are, and `HistoryEntry` redacts
        // its own. Rendered as a count anyway, because a list of twenty redacted
        // structs is noise.
        f.debug_struct("History")
            .field(
                "entries",
                &format_args!("{} <redacted>", self.entries.len()),
            )
            .finish()
    }
}

impl History {
    /// An empty history.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Decode a stored history, tolerating a payload this build cannot read.
    ///
    /// A history that will not decode is discarded rather than propagated as an
    /// error. It is a convenience cache of values the user has already used, and
    /// failing an unlock — or a generate — because of it would trade something
    /// that matters for something that does not.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Self {
        postcard::from_bytes(bytes).unwrap_or_default()
    }

    /// Encode for storage.
    ///
    /// # Errors
    ///
    /// Returns `None` if encoding fails, which cannot happen for this shape but
    /// is not worth an `unwrap` on a path that holds secrets.
    #[must_use]
    pub fn encode(&self) -> Option<zeroize::Zeroizing<Vec<u8>>> {
        postcard::to_stdvec(self).ok().map(zeroize::Zeroizing::new)
    }

    /// Add an entry, then apply both caps.
    ///
    /// Newest first, so the UI's natural order needs no sort and the truncation
    /// below drops the oldest.
    pub fn record(&mut self, entry: HistoryEntry, now_ms: i64) {
        self.entries.insert(0, entry);
        self.prune(now_ms);
    }

    /// Drop expired entries and anything past the count cap.
    ///
    /// Called on read as well as write: age-based expiry that only ran on write
    /// would not expire anything for a user who stopped generating.
    pub fn prune(&mut self, now_ms: i64) {
        let cutoff = now_ms.saturating_sub(MAX_AGE_MS);
        // `retain` drops in place, and `HistoryEntry` zeroizes on drop, so an
        // expired password is wiped rather than merely unlinked.
        self.entries.retain(|e| e.created_at > cutoff);
        self.entries.truncate(MAX_ENTRIES);
    }

    /// One entry's value by id, if it is still present.
    ///
    /// Returns a borrow rather than a clone so a caller that only needs to put it
    /// on the clipboard never makes a second copy.
    #[must_use]
    pub fn value_of(&self, id: Uuid) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.value.as_str())
    }

    /// How many entries are retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether anything is retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forget everything (SPEC-V1 §7.5, "clear generator history").
    pub fn clear(&mut self) {
        // Explicit zeroize before the Vec drops, so the capacity is wiped rather
        // than just the elements being released.
        self.entries.zeroize();
        self.entries.clear();
    }
}
