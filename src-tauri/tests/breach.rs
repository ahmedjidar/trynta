//! The breach check's privacy rules (SPEC-V1 §7.4, §2, AC14).
//!
//! This is one of three outbound requests permitted in the entire product, and the
//! only one derived from the user's data. Every test here is about a rule whose
//! violation would be invisible in normal use:
//!
//! - **Five characters, never more.** The recording double asserts on exactly what
//!   it was handed, which is the complementary half of AC14's packet capture: the
//!   capture proves we did not call anyone we shouldn't, this proves that what we
//!   do send is a bucket and not a hash.
//! - **Padding is discarded.** `Add-Padding: true` makes HIBP inject entries with
//!   a count of 0. Counting one as a hit reports an unbreached password as
//!   breached *because* we asked for privacy — a bug that only appears once
//!   padding is switched on, which is to say in production.
//! - **Offline is not "safe".** §7.4: *"Offline → 'not checked,' never 'safe.'"*
//! - **The cache never reaches the network.** AC14 requires a report to make zero
//!   requests, so the report is handed a source with no transport rather than a
//!   transport it is trusted not to use.

use std::cell::RefCell;

use keyring_lib::services::breach::{
    self, BreachCache, BreachError, BreachStatus, CachedOnly, Prefix, RangeSource, MIN_INTERVAL_MS,
    PREFIX_LEN,
};

/// A published SHA-1, so the split is checked against a known value rather than
/// against itself. `password` hashes to 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8.
const KNOWN_PASSWORD: &str = "password";
const KNOWN_PREFIX: &str = "5BAA6";
const KNOWN_SUFFIX: &str = "1E4C9B93F3F0682250B6CF8331B7EE68FD8";

/// Records every prefix it is asked for, and answers from a script.
struct Recording {
    body: Result<String, BreachError>,
    seen: RefCell<Vec<String>>,
}

impl Recording {
    fn answering(body: &str) -> Self {
        Self {
            body: Ok(body.to_owned()),
            seen: RefCell::new(Vec::new()),
        }
    }

    fn offline() -> Self {
        Self {
            body: Err(BreachError::Unreachable),
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl RangeSource for Recording {
    fn fetch(&self, prefix: Prefix) -> Result<String, BreachError> {
        self.seen.borrow_mut().push(prefix.as_str().to_owned());
        self.body.clone()
    }
}

// ── k-anonymity ─────────────────────────────────────────────────────────────

#[test]
fn the_split_matches_the_known_sha1() {
    let (prefix, suffix) = breach::split(KNOWN_PASSWORD);
    assert_eq!(prefix.as_str(), KNOWN_PREFIX);
    assert_eq!(&*suffix, KNOWN_SUFFIX);
    assert_eq!(
        format!("{}{}", prefix.as_str(), &*suffix).len(),
        40,
        "prefix and suffix should reconstitute a full SHA-1"
    );
}

#[test]
fn only_five_characters_ever_reach_the_transport() {
    // The whole k-anonymity property in one assertion. If this ever fails, the
    // server can identify the password rather than a bucket of ~800 of them.
    let source = Recording::answering(&format!("{KNOWN_SUFFIX}:42\r\n"));
    let _ = breach::check_one(KNOWN_PASSWORD, &source);

    let seen = source.seen.borrow();
    assert_eq!(seen.len(), 1, "expected exactly one request");
    for prefix in seen.iter() {
        assert_eq!(
            prefix.len(),
            PREFIX_LEN,
            "the transport was handed {} characters, not {PREFIX_LEN}",
            prefix.len()
        );
        assert!(
            !KNOWN_SUFFIX.contains(prefix.as_str()) || prefix == KNOWN_PREFIX,
            "the transport was handed part of the suffix"
        );
    }
    assert_eq!(seen[0], KNOWN_PREFIX);
}

#[test]
fn the_suffix_never_appears_in_what_is_sent() {
    let source = Recording::answering("AAAA:1\r\n");
    let _ = breach::check_one(KNOWN_PASSWORD, &source);
    let sent = source.seen.borrow().join("|");
    assert!(
        !sent.contains(KNOWN_SUFFIX),
        "the suffix leaked into the request: {sent}"
    );
}

// ── Padding, which Add-Padding: true makes mandatory to handle ───────────────

#[test]
fn padding_entries_are_not_hits() {
    // HIBP's padding entries carry a count of 0. Treating one as a hit would
    // report an unbreached password as breached precisely because we asked for
    // the padding that protects us.
    let body = format!("{KNOWN_SUFFIX}:0\r\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:0\r\n");
    assert_eq!(
        breach::find_in_body(&body, KNOWN_SUFFIX).expect("valid body"),
        BreachStatus::NotBreached,
        "a padding entry matching our suffix was counted as a breach"
    );
}

#[test]
fn a_real_hit_among_padding_is_found() {
    let body = format!(
        "0000000000000000000000000000000000A:0\r\n\
         {KNOWN_SUFFIX}:3730471\r\n\
         BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB:0\r\n"
    );
    assert_eq!(
        breach::find_in_body(&body, KNOWN_SUFFIX).expect("valid body"),
        BreachStatus::Breached { count: 3_730_471 }
    );
}

#[test]
fn a_body_of_only_padding_means_absent_not_malformed() {
    let body = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:0\r\nBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB:0\r\n";
    assert_eq!(
        breach::find_in_body(body, KNOWN_SUFFIX).expect("valid body"),
        BreachStatus::NotBreached
    );
}

#[test]
fn suffix_matching_is_case_insensitive() {
    // HIBP returns uppercase; a proxy or a future change could not.
    let body = format!("{}:9\r\n", KNOWN_SUFFIX.to_lowercase());
    assert_eq!(
        breach::find_in_body(&body, KNOWN_SUFFIX).expect("valid body"),
        BreachStatus::Breached { count: 9 }
    );
}

#[test]
fn a_response_that_is_not_a_range_body_is_malformed() {
    for body in ["", "   ", "<html>503</html>", "not a range at all"] {
        assert_eq!(
            breach::find_in_body(body, KNOWN_SUFFIX).unwrap_err(),
            BreachError::Malformed,
            "{body:?} should not have parsed as a range body"
        );
    }
}

// ── Offline must never read as safe ─────────────────────────────────────────

#[test]
fn offline_is_not_checked_and_never_not_breached() {
    let source = Recording::offline();
    let status = breach::check_one(KNOWN_PASSWORD, &source);
    assert_eq!(
        status,
        BreachStatus::NotChecked,
        "an unreachable service reported a password as checked"
    );
    assert!(!status.is_breached());
}

#[test]
fn a_malformed_response_is_not_checked_rather_than_safe() {
    let source = Recording::answering("<html>maintenance</html>");
    assert_eq!(
        breach::check_one(KNOWN_PASSWORD, &source),
        BreachStatus::NotChecked
    );
}

#[test]
fn an_empty_password_is_not_checked_and_makes_no_request() {
    let source = Recording::answering("AAAA:1");
    assert_eq!(breach::check_one("", &source), BreachStatus::NotChecked);
    assert!(
        source.seen.borrow().is_empty(),
        "an empty password should not produce a request"
    );
}

#[test]
fn not_checked_does_not_count_toward_the_breached_tally() {
    // It would move the health score on no information, in whichever direction
    // the mistake went.
    assert!(!BreachStatus::NotChecked.is_breached());
    assert!(!BreachStatus::NotBreached.is_breached());
    assert!(BreachStatus::Breached { count: 1 }.is_breached());
}

// ── The cache ───────────────────────────────────────────────────────────────

#[test]
fn the_cache_round_trips_through_its_stored_encoding() {
    let (prefix, _) = breach::split(KNOWN_PASSWORD);
    let mut cache = BreachCache::default();
    cache.put(prefix, format!("{KNOWN_SUFFIX}:5\r\n"), 1_700_000_000_000);

    let encoded = cache.encode().expect("encode");
    let decoded = BreachCache::decode(&encoded);
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded.get(prefix), cache.get(prefix));
    assert_eq!(decoded.fetched_at, 1_700_000_000_000);
}

#[test]
fn an_undecodable_cache_reads_as_empty() {
    assert!(BreachCache::decode(b"not postcard").is_empty());
    assert!(BreachCache::decode(&[]).is_empty());
}

#[test]
fn the_twenty_four_hour_cadence_is_enforced() {
    // §7.4's exact wording: "at most once per 24 h, on the first unlock after the
    // interval elapses." Anything more frequent is a claim the product cannot
    // honour while locked.
    let last = 1_700_000_000_000;
    assert!(!BreachCache::may_refresh(last, last));
    assert!(!BreachCache::may_refresh(last, last + MIN_INTERVAL_MS - 1));
    assert!(BreachCache::may_refresh(last, last + MIN_INTERVAL_MS));
    assert!(BreachCache::may_refresh(last, last + MIN_INTERVAL_MS * 3));
    // A never-checked vault may check immediately.
    assert!(BreachCache::may_refresh(0, last));
}

#[test]
fn a_clock_that_moves_backwards_does_not_permit_a_refresh() {
    let last = 1_700_000_000_000;
    assert!(!BreachCache::may_refresh(last, last - MIN_INTERVAL_MS));
    assert!(!BreachCache::may_refresh(last, 0));
}

#[test]
fn a_cache_only_source_answers_without_a_transport() {
    // AC14: running a report must make zero requests. Guaranteed by handing the
    // report a source that has no transport, rather than one it is trusted not to
    // use.
    let (prefix, suffix) = breach::split(KNOWN_PASSWORD);
    let mut cache = BreachCache::default();
    cache.put(prefix, format!("{}:11\r\n", &*suffix), 1_700_000_000_000);

    let source = CachedOnly { cache: &cache };
    assert_eq!(
        breach::check_one(KNOWN_PASSWORD, &source),
        BreachStatus::Breached { count: 11 }
    );
}

#[test]
fn a_cache_miss_reads_as_not_checked() {
    let cache = BreachCache::default();
    let source = CachedOnly { cache: &cache };
    assert_eq!(
        breach::check_one(KNOWN_PASSWORD, &source),
        BreachStatus::NotChecked,
        "a cache miss must not read as safe"
    );
}

#[test]
fn an_error_never_quotes_a_password_or_a_hash() {
    for err in [BreachError::Unreachable, BreachError::Malformed] {
        let rendered = format!("{err} {err:?}");
        assert!(!rendered.contains(KNOWN_SUFFIX), "{rendered}");
        assert!(!rendered.contains(KNOWN_PREFIX), "{rendered}");
        assert!(!rendered.contains(KNOWN_PASSWORD), "{rendered}");
    }
}
