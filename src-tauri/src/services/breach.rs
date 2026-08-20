// SPDX-License-Identifier: AGPL-3.0-or-later
//! The breach check: HIBP range queries, k-anonymous (SPEC-V1 §7.4, §2, AC14).
//!
//! This is one of exactly **three** outbound requests permitted in the whole
//! product, and the only one that is a function of the user's data. §7 makes
//! adding a fourth a spec change. So the rules here are narrow and each one is
//! load-bearing:
//!
//! | Rule | Why |
//! |---|---|
//! | Send **5 hex characters** of the SHA-1, never more | k-anonymity: the server sees a bucket of ~800 hashes, not yours |
//! | `Add-Padding: true` is **mandatory** | without it the response *length* reveals which prefix you asked for, which undoes the k-anonymity |
//! | Discard entries with count 0 | those are the padding. Counting them as hits would report an unbreached password as breached |
//! | Cache **inside the encrypted store** | §7.4: *"a plaintext cache of your password hash prefixes is a filter that massively narrows an offline attack"* |
//! | Offline is **"not checked"**, never "safe" | a silent downgrade from unknown to safe is the one failure mode that actively misleads |
//!
//! SHA-1 is used because HIBP's API is defined in terms of it. That is interop
//! with someone else's protocol, not a security choice of ours — the hash never
//! authenticates anything and never protects anything.
//!
//! ## Why the transport is a trait
//!
//! [`RangeSource`] exists so every rule above is testable without a network. AC14
//! asks for a packet capture proving zero requests to user sites; a test double
//! that records exactly what it was asked for proves the complementary half — that
//! the only thing we ever hand a transport is five hex characters.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use zeroize::Zeroizing;

/// Hex characters of the hash sent to the server (SPEC-V1 §7.4).
pub const PREFIX_LEN: usize = 5;

/// Hex characters of a full SHA-1.
const HASH_LEN: usize = 40;

/// How often a check may run (SPEC-V1 §7.4: at most once per 24 h).
pub const MIN_INTERVAL_MS: i64 = 86_400_000;

/// Why a range query did not produce an answer.
///
/// No variant carries a password, a hash, or a suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BreachError {
    /// The request did not complete. Offline, DNS, TLS, timeout — all the same
    /// from here, and all mean "not checked".
    #[error("the breach service could not be reached")]
    Unreachable,

    /// The service answered with something that is not a range response.
    #[error("the breach service returned an unexpected response")]
    Malformed,
}

/// The five hex characters of a SHA-1 that may leave the device.
///
/// A newtype rather than a `String` so the type system carries the k-anonymity
/// rule: [`RangeSource`] cannot be handed anything longer, because there is no
/// way to construct one that is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Prefix([u8; PREFIX_LEN]);

impl Prefix {
    /// The prefix as an uppercase hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // Every byte came from `HEX_DIGITS`, so this is ASCII by construction.
        std::str::from_utf8(&self.0).unwrap_or("00000")
    }
}

impl std::fmt::Display for Prefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a range response comes from.
///
/// Implementations must send `Add-Padding: true` and must not attach any
/// identifier — no cookie, no user agent naming the user, no query parameter
/// beyond the prefix itself. The endpoint learns an IP and a bucket; that is the
/// documented cost in §2 and it must not grow.
pub trait RangeSource {
    /// Fetch the range for `prefix`.
    ///
    /// # Errors
    ///
    /// [`BreachError::Unreachable`] if the request did not complete,
    /// [`BreachError::Malformed`] if the response is not a range body.
    fn fetch(&self, prefix: Prefix) -> Result<String, BreachError>;
}

/// What is known about one password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BreachStatus {
    /// Found in a breach corpus, with the number of appearances.
    Breached {
        /// How many times it appears. Always ≥ 1; a 0 is padding, not a hit.
        count: u32,
    },
    /// Checked and absent.
    NotBreached,
    /// Not checked — offline, rate-limited, or disabled.
    ///
    /// **Never** collapse this into [`BreachStatus::NotBreached`]. §7.4:
    /// *"Offline → 'not checked,' never 'safe.'"* Telling someone a password is
    /// fine when nobody looked is worse than telling them nothing.
    NotChecked,
}

impl BreachStatus {
    /// Whether this counts toward the report's `breached` tally.
    ///
    /// `NotChecked` does not. An unchecked password is not evidence of anything,
    /// and counting it either way would move the health score on no information.
    #[must_use]
    pub const fn is_breached(self) -> bool {
        matches!(self, Self::Breached { .. })
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

/// SHA-1 of `password`, uppercase hex.
///
/// `Zeroizing`, because a password's hash is a password-equivalent for the purpose
/// of an offline attack on a leaked cache.
fn hash_hex(password: &str) -> Zeroizing<Vec<u8>> {
    let digest = Sha1::digest(password.as_bytes());
    let mut hex = Zeroizing::new(Vec::with_capacity(HASH_LEN));
    for byte in digest {
        hex.push(HEX_DIGITS[usize::from(byte >> 4)]);
        hex.push(HEX_DIGITS[usize::from(byte & 0x0F)]);
    }
    hex
}

/// Split a password into the prefix that may be sent and the suffix that may not.
///
/// The suffix stays in `Zeroizing` and never crosses a transport boundary — that
/// division is the entire k-anonymity property, so it happens once, here.
#[must_use]
pub fn split(password: &str) -> (Prefix, Zeroizing<String>) {
    let hex = hash_hex(password);
    let mut prefix = [0u8; PREFIX_LEN];
    prefix.copy_from_slice(&hex[..PREFIX_LEN]);
    let suffix = Zeroizing::new(String::from_utf8_lossy(&hex[PREFIX_LEN..]).into_owned());
    (Prefix(prefix), suffix)
}

/// Find `suffix` in a range response body.
///
/// The body is `SUFFIX:COUNT` per line. Entries with a count of 0 are the padding
/// `Add-Padding: true` asks for and are discarded — treating one as a hit would
/// report a password as breached because we asked for privacy.
///
/// # Errors
///
/// [`BreachError::Malformed`] if no line parses at all, which means the response
/// was not a range body. A body of only padding is valid and means "absent".
pub fn find_in_body(body: &str, suffix: &str) -> Result<BreachStatus, BreachError> {
    let mut parsed_any = false;

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((candidate, count)) = line.split_once(':') else {
            continue;
        };
        let Ok(count) = count.trim().parse::<u32>() else {
            continue;
        };
        parsed_any = true;

        // Padding. Not a hit, and not evidence of anything.
        if count == 0 {
            continue;
        }
        if candidate.trim().eq_ignore_ascii_case(suffix) {
            return Ok(BreachStatus::Breached { count });
        }
    }

    if parsed_any {
        Ok(BreachStatus::NotBreached)
    } else {
        Err(BreachError::Malformed)
    }
}

/// Check one password against a range source.
///
/// A transport failure is [`BreachStatus::NotChecked`], not an error: one
/// unreachable prefix must not fail a report over hundreds of items.
pub fn check_one(password: &str, source: &dyn RangeSource) -> BreachStatus {
    if password.is_empty() {
        return BreachStatus::NotChecked;
    }
    let (prefix, suffix) = split(password);
    match source.fetch(prefix) {
        Ok(body) => find_in_body(&body, &suffix).unwrap_or(BreachStatus::NotChecked),
        Err(_) => BreachStatus::NotChecked,
    }
}

/// Cached range responses, keyed by prefix (SPEC-V1 §7.4).
///
/// Lives in the encrypted `app_cache`, never on disk in the clear. The keys are
/// the prefixes of the user's own password hashes: §7.4 calls a plaintext copy of
/// exactly this "a filter that massively narrows an offline attack", and it is
/// right — knowing the 20-bit bucket of every password in a vault removes most of
/// the search space before an attacker starts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BreachCache {
    /// Prefix (as hex) to response body.
    pub ranges: BTreeMap<String, String>,
    /// When this cache was last refreshed, Unix milliseconds.
    pub fetched_at: i64,
}

impl BreachCache {
    /// Decode a stored cache, discarding one this build cannot read.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Self {
        postcard::from_bytes(bytes).unwrap_or_default()
    }

    /// Encode for storage.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        postcard::to_stdvec(self).ok()
    }

    /// Whether a check may run, given the last one's time (SPEC-V1 §7.4).
    ///
    /// §7.4 is explicit that a "daily background check" is impossible while
    /// locked, and prescribes the exact honest behaviour: *at most once per 24 h,
    /// on the first unlock after the interval elapses.*
    #[must_use]
    pub const fn may_refresh(last_check_ms: i64, now_ms: i64) -> bool {
        now_ms.saturating_sub(last_check_ms) >= MIN_INTERVAL_MS
    }

    /// A cached body for `prefix`, if present.
    #[must_use]
    pub fn get(&self, prefix: Prefix) -> Option<&str> {
        self.ranges.get(prefix.as_str()).map(String::as_str)
    }

    /// Record a fetched body.
    pub fn put(&mut self, prefix: Prefix, body: String, now_ms: i64) {
        self.ranges.insert(prefix.as_str().to_owned(), body);
        self.fetched_at = now_ms;
    }

    /// How many ranges are cached.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Whether anything is cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

/// A source that answers only from a cache and never reaches the network.
///
/// This is what `security_report_run()` uses. AC14 requires that running a report
/// makes **zero** requests, and the way to guarantee that is to hand the report a
/// source that has no transport at all rather than to remember not to call one.
pub struct CachedOnly<'a> {
    /// The cache to answer from.
    pub cache: &'a BreachCache,
}

impl RangeSource for CachedOnly<'_> {
    fn fetch(&self, prefix: Prefix) -> Result<String, BreachError> {
        self.cache
            .get(prefix)
            .map(ToOwned::to_owned)
            // Not an error the caller should report: a missing range means this
            // password has not been checked, which is a status, not a failure.
            .ok_or(BreachError::Unreachable)
    }
}

/// The outcome of a cache refresh.
#[derive(Debug, Clone)]
pub struct Refreshed {
    /// The cache to store. Rebuilt, not merged — see [`refresh`].
    pub cache: BreachCache,
    /// How many prefixes were fetched successfully.
    pub fetched: usize,
    /// How many could not be reached. Their items read as "not checked".
    pub failed: usize,
}

/// Rebuild the range cache for exactly the prefixes still in use.
///
/// Two decisions live here rather than in the command, because both are the kind
/// of thing that is easy to get subtly wrong and impossible to notice afterwards:
///
/// **The cache is rebuilt, not appended to.** Only `wanted` survives. SPEC-V1 §7.4
/// calls a plaintext cache of these prefixes *"a filter that massively narrows an
/// offline attack"*; ours is encrypted, but the same reasoning says not to keep the
/// prefix of a password the user changed six months ago. An append-only cache grows
/// into a record of every password the vault has ever held.
///
/// **A failed request keeps its previous body.** Dropping it would turn one
/// unreachable prefix into a downgrade from "breached" to "not checked", which is
/// the one direction §7.4 says never to move in silently.
///
/// `fetched_at` advances only if something was actually fetched. A refresh that
/// reached nothing must not start the 24-hour clock — being offline once would
/// otherwise cost the user a day.
#[must_use]
pub fn refresh(
    previous: &BreachCache,
    wanted: &BTreeSet<Prefix>,
    source: &dyn RangeSource,
    now_ms: i64,
) -> Refreshed {
    let mut cache = BreachCache::default();
    let mut fetched = 0;
    let mut failed = 0;

    for prefix in wanted {
        if let Ok(body) = source.fetch(*prefix) {
            cache.put(*prefix, body, now_ms);
            fetched += 1;
        } else {
            failed += 1;
            if let Some(stale) = previous.get(*prefix) {
                cache.put(*prefix, stale.to_owned(), now_ms);
            }
        }
    }

    cache.fetched_at = if fetched > 0 {
        now_ms
    } else {
        previous.fetched_at
    };

    Refreshed {
        cache,
        fetched,
        failed,
    }
}
