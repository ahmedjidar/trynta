// SPDX-License-Identifier: AGPL-3.0-or-later
//! Search latency at 5,000 items (SPEC-V1 §9, AC20).
//!
//! AC20: *"Search p95 under 16 ms at 5,000 items."* §9 defines the measurement as
//! **keystroke → rendered** on a 5,000-item generated vault, so a sample here is
//! one keystroke: the query the index receives after the user types one more
//! character, not one whole search.
//!
//! 16 ms is one frame at 60 Hz. The point of the budget is that the list keeps up
//! with typing, so the number that matters is the tail, not the mean — one
//! keystroke in twenty stuttering is what a user actually notices.
//!
//! **The 16 ms budget is enforced in release only**, because `cargo test` without
//! `--release` builds `nucleo-matcher` and this crate at `opt-level = 0`, where a
//! fuzzy match over 5,000 haystacks is an order of magnitude slower than what
//! ships. Holding a debug build to the shipping budget would either fail a budget
//! the product meets or get the budget quietly relaxed to accommodate it.
//!
//! It does **not** skip in debug. A test that no-ops in one profile is a test that
//! reads as a pass while proving nothing, and `cargo test --workspace` runs this
//! in debug. So both profiles measure, both print, and both assert — debug against
//! a loose ceiling that still catches an accidental O(n²), release against §9's
//! actual number.
//!
//! What this does **not** measure: the IPC round trip and the React render. §9
//! scopes the budget to the whole path, so the query itself has to come in well
//! under 16 ms for the end-to-end number to fit. That is the honest limit of a
//! headless test, and it is recorded here rather than implied.

use std::time::{Duration, Instant};

use keyring_lib::index::{ItemSource, ListQuery, QuickFilters, SearchIndex, SortOrder};
use keyring_store::{IndexRow, ItemKind};
use uuid::Uuid;

/// SPEC-V1 §9's reference vault size.
const ITEMS: usize = 5_000;

/// SPEC-V1 §9's budget: one frame at 60 Hz. Enforced in release.
const BUDGET: Duration = Duration::from_millis(16);

/// Debug ceiling. Not a performance claim — a shape check. Debug is roughly an
/// order of magnitude slower, so anything past this is an algorithmic regression
/// rather than a missing optimiser.
const DEBUG_CEILING: Duration = Duration::from_millis(250);

/// The budget that applies to the profile this was built in.
fn budget() -> (Duration, &'static str) {
    if cfg!(debug_assertions) {
        (DEBUG_CEILING, "debug ceiling")
    } else {
        (BUDGET, "SPEC-V1 §9 budget")
    }
}

/// Words assembled into item titles, so matches are spread through the index
/// rather than clustered at one end of it.
const NOUNS: &[&str] = &[
    "account",
    "backup",
    "billing",
    "cloud",
    "console",
    "dashboard",
    "domain",
    "gateway",
    "identity",
    "invoice",
    "ledger",
    "mailbox",
    "network",
    "payroll",
    "portal",
    "registry",
    "storage",
    "support",
    "tenant",
    "vault",
];

const ORGS: &[&str] = &[
    "acme",
    "northwind",
    "contoso",
    "initech",
    "umbrella",
    "globex",
    "hooli",
    "soylent",
    "stark",
    "wayne",
];

const TLDS: &[&str] = &["test", "example", "invalid", "localhost"];

/// A deterministic generated vault.
///
/// No RNG: a benchmark whose input changes between runs cannot be compared
/// between runs, and a regression would be indistinguishable from a different
/// draw. The mixing below is a plain multiplicative hash over the row index.
fn generated_rows() -> Vec<IndexRow> {
    let vaults: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();

    (0..ITEMS)
        .map(|i| {
            let noun = NOUNS[(i * 31) % NOUNS.len()];
            let other = NOUNS[(i * 17 + 7) % NOUNS.len()];
            let org = ORGS[(i * 13) % ORGS.len()];
            let tld = TLDS[(i * 7) % TLDS.len()];

            let kind = match i % 4 {
                0 => ItemKind::Login,
                1 => ItemKind::SecureNote,
                2 => ItemKind::Card,
                _ => ItemKind::Identity,
            };

            IndexRow {
                id: Uuid::new_v4(),
                vault_id: vaults[i % vaults.len()],
                kind,
                title: format!("{org} {noun} {i}"),
                username: Some(format!("user{i}@{org}.{tld}")),
                urls: vec![format!("https://{noun}.{org}.{tld}/{other}")],
                tags: vec![noun.to_owned(), other.to_owned()],
                favorite: i % 23 == 0,
                has_totp: i % 5 == 0,
                revision: 1,
                created_at: 1_700_000_000_000 + i64::try_from(i).unwrap_or(0),
                updated_at: 1_700_000_000_000 + i64::try_from(ITEMS - i).unwrap_or(0),
                subtitle: Some(format!("user{i}@{org}.{tld}")),
                has_custom_icon: false,
            }
        })
        .collect()
}

fn query_for(search: &str) -> ListQuery {
    ListQuery {
        source: ItemSource::All,
        filters: QuickFilters::default(),
        sort: SortOrder::RecentlyUpdated,
        search: search.to_owned(),
    }
}

/// Every prefix of `word`, which is the sequence of queries typing it produces.
fn keystrokes(word: &str) -> Vec<String> {
    (1..=word.chars().count())
        .map(|n| word.chars().take(n).collect())
        .collect()
}

#[test]
fn search_p95_is_within_budget_at_five_thousand_items() {
    let rows = generated_rows();
    assert_eq!(rows.len(), ITEMS);

    let index = SearchIndex::build(rows);
    assert_eq!(index.len(), ITEMS);

    // Typing several realistic queries, one sample per keystroke. A word that
    // matches nothing is included on purpose: a no-match query still scores every
    // candidate, so it is the worst case rather than the cheapest.
    let words = [
        "acme portal",
        "northwind",
        "user4242",
        "storage",
        "contoso ledger",
        "zzzzzz",
        "identity.stark",
        "gateway",
    ];

    // Warm up so the first sample is not paying for lazily initialised matcher
    // state. Excluded from the measurement, and worth noting rather than hiding:
    // the first keystroke of a session really does pay this, and §9's budget is
    // about sustained typing.
    for word in &words {
        let _ = index.query(&query_for(word));
    }

    let mut samples: Vec<Duration> = Vec::new();
    let mut total_matches = 0usize;
    for word in &words {
        for prefix in keystrokes(word) {
            let query = query_for(&prefix);
            let started = Instant::now();
            let matched = index.query(&query);
            samples.push(started.elapsed());
            total_matches += matched.len();
        }
    }

    assert!(
        total_matches > 0,
        "every query matched nothing, so this measured an empty code path"
    );
    assert!(
        samples.len() >= 50,
        "only {} samples; a p95 over that few is noise",
        samples.len()
    );

    samples.sort_unstable();
    let p50 = percentile(&samples, 50.0);
    let p95 = percentile(&samples, 95.0);
    let worst = *samples.last().expect("at least one sample");

    let (limit, which) = budget();
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    // Printed on success as well as failure. AC20 is a number, and a run that
    // squeaks in at 15.9 ms should not look like one that came in at 2 ms. The
    // profile is printed too, so a debug measurement can never be mistaken for
    // the one the criterion is about.
    println!(
        "search p95 = {:.2} ms  (p50 {:.2} ms, max {:.2} ms, {} samples, {ITEMS} items, \
         {profile} build, limit {} ms as the {which})",
        millis(p95),
        millis(p50),
        millis(worst),
        samples.len(),
        limit.as_millis()
    );

    assert!(
        p95 <= limit,
        "search p95 is {:.2} ms, over the {} ms {which} — at 60 Hz that is one keystroke \
         in twenty dropping a frame",
        millis(p95),
        limit.as_millis()
    );
}

#[test]
fn building_the_index_at_five_thousand_items_is_not_the_bottleneck() {
    // The index is built once per unlock, and §9 budgets unlock at 1.2 s
    // *including* Argon2. If building the haystacks took a meaningful slice of
    // that, the search budget would be the wrong thing to worry about.
    let rows = generated_rows();
    let started = Instant::now();
    let index = SearchIndex::build(rows);
    let elapsed = started.elapsed();

    assert_eq!(index.len(), ITEMS);
    let ceiling = if cfg!(debug_assertions) {
        Duration::from_millis(2_000)
    } else {
        Duration::from_millis(250)
    };
    println!(
        "search p95 harness: index build = {:.2} ms (ceiling {} ms)",
        millis(elapsed),
        ceiling.as_millis()
    );
    assert!(
        elapsed < ceiling,
        "building the index took {:.2} ms, which is a real slice of the 1.2 s unlock budget",
        millis(elapsed)
    );
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

/// Nearest-rank percentile over a sorted slice.
fn percentile(sorted: &[Duration], pct: f64) -> Duration {
    assert!(!sorted.is_empty(), "no samples");
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let rank = ((pct / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}
