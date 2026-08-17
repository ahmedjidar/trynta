//! The in-memory search index (SPEC-V1 §4.5, §4.7, §7.1).
//!
//! Built once at unlock by decrypting every item's `meta_ct`, held for the life
//! of the session, destroyed on lock. It contains **only** non-secret fields —
//! that is what makes it safe to hold, and `IndexRow`'s own contract enforces
//! it.
//!
//! Rust-side rather than webview-side (ADD-002 Q8). The budget is 16 ms p95 at
//! 5,000 items *including* the IPC round trip, so the query itself has to be
//! well inside that; `benches/search.rs` measures it rather than assuming.
//!
//! Matching is `nucleo-matcher`, the fzf-grade matcher extracted from Helix.
//! Hand-rolling a fuzzy matcher that stays inside the budget is a bad use of
//! time and a good source of subtle ranking bugs.

use keyring_store::{IndexRow, ItemKind};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use uuid::Uuid;

/// Which items to consider (SPEC-V1 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "source")]
pub enum ItemSource {
    /// Every item in every vault.
    #[default]
    All,
    /// One vault.
    Vault {
        /// The vault to show.
        id: Uuid,
    },
    /// One library category.
    Category {
        /// The item type to show.
        kind: ItemKind,
    },
    /// Favourites across every vault.
    Favorites,
}

/// Combinable quick filters (SPEC-V1 §7.1).
///
/// `shared` is present and always empty in V1 — the data model must not make
/// SPEC-V2 impossible, and a filter that silently disappeared and reappeared
/// would be worse than one that returns nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickFilters {
    /// Only items whose password is weak.
    pub weak: bool,
    /// Only items with a TOTP configuration.
    pub has_totp: bool,
    /// Only shared items. Always empty in V1 (SPEC-V2).
    pub shared: bool,
}

/// Sort order (SPEC-V1 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    /// Most recently used first.
    RecentlyUsed,
    /// Most recently updated first.
    #[default]
    RecentlyUpdated,
    /// Title, case-insensitive.
    Alphabetical,
    /// Newest first.
    DateCreated,
}

/// A list request.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    /// Which items to consider.
    pub source: ItemSource,
    /// Quick filters, combinable.
    pub filters: QuickFilters,
    /// Sort order. Ignored when `search` is set, because relevance wins.
    pub sort: SortOrder,
    /// Fuzzy search text. Empty means no search.
    pub search: String,
}

/// The searchable projection of an unlocked vault.
///
/// Dropping it is what "destroyed on lock" means; `IndexRow` wipes its strings
/// on drop, so releasing the index wipes the account inventory with it.
pub struct SearchIndex {
    rows: Vec<IndexRow>,
    /// Pre-joined haystack per row, so a keystroke does not rebuild strings for
    /// 5,000 items. Same lifetime and same wiping rules as the rows.
    haystacks: Vec<String>,
    /// Item ids whose password the security report flagged weak, for the `weak`
    /// quick filter. Empty until a report has run.
    weak: Vec<Uuid>,
}

impl std::fmt::Debug for SearchIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Titles and URLs are not secrets, but they are the account inventory.
        f.debug_struct("SearchIndex")
            .field("items", &self.rows.len())
            .finish_non_exhaustive()
    }
}

impl Drop for SearchIndex {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        for haystack in &mut self.haystacks {
            haystack.zeroize();
        }
        // `rows` wipe themselves via `IndexRow::drop`.
    }
}

impl SearchIndex {
    /// Build an index from decrypted metadata.
    #[must_use]
    pub fn build(rows: Vec<IndexRow>) -> Self {
        let haystacks = rows.iter().map(haystack_for).collect();
        Self {
            rows,
            haystacks,
            weak: Vec::new(),
        }
    }

    /// How many items the index holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Record which items the security report considers weak.
    pub fn set_weak(&mut self, weak: Vec<Uuid>) {
        self.weak = weak;
    }

    /// One row by id.
    #[must_use]
    pub fn row(&self, id: Uuid) -> Option<&IndexRow> {
        self.rows.iter().find(|r| r.id == id)
    }

    /// Every row, unfiltered.
    #[must_use]
    pub fn rows(&self) -> &[IndexRow] {
        &self.rows
    }

    /// Run a query and return the matching rows in order.
    ///
    /// Search beats sort: when the user is typing, relevance is the ordering
    /// they asked for, and re-sorting fuzzy results alphabetically would throw
    /// away the only signal that makes the search useful.
    #[must_use]
    pub fn query(&self, query: &ListQuery) -> Vec<&IndexRow> {
        let candidates: Vec<usize> = (0..self.rows.len())
            .filter(|&i| self.passes(&self.rows[i], query))
            .collect();

        if query.search.trim().is_empty() {
            let mut rows: Vec<&IndexRow> = candidates.iter().map(|&i| &self.rows[i]).collect();
            sort_rows(&mut rows, query.sort);
            return rows;
        }

        // A fresh matcher per query: `Matcher` carries reusable scratch buffers
        // and is not `Sync`, and constructing one is cheap relative to scoring
        // thousands of rows.
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(
            query.search.trim(),
            CaseMatching::Ignore,
            Normalization::Smart,
        );

        let mut scored: Vec<(u32, usize)> = candidates
            .into_iter()
            .filter_map(|i| {
                let mut buf = Vec::new();
                let haystack = nucleo_matcher::Utf32Str::new(&self.haystacks[i], &mut buf);
                pattern
                    .score(haystack, &mut matcher)
                    .map(|score| (score, i))
            })
            .collect();

        // Highest score first; ties broken by recency so the order is stable and
        // useful rather than arbitrary.
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| self.rows[b.1].updated_at.cmp(&self.rows[a.1].updated_at))
        });
        scored.into_iter().map(|(_, i)| &self.rows[i]).collect()
    }

    fn passes(&self, row: &IndexRow, query: &ListQuery) -> bool {
        let source_ok = match query.source {
            ItemSource::All => true,
            ItemSource::Vault { id } => row.vault_id == id,
            ItemSource::Category { kind } => row.kind == kind,
            ItemSource::Favorites => row.favorite,
        };
        if !source_ok {
            return false;
        }
        if query.filters.has_totp && !row.has_totp {
            return false;
        }
        if query.filters.weak && !self.weak.contains(&row.id) {
            return false;
        }
        // Sharing is SPEC-V2. The filter exists so the UI does not have to
        // appear and disappear between releases; it matches nothing in V1.
        if query.filters.shared {
            return false;
        }
        true
    }
}

/// Everything a search should match against, joined once at build time.
///
/// SPEC-V1 §7.1: title, username, url, tag.
fn haystack_for(row: &IndexRow) -> String {
    let mut out = String::with_capacity(64);
    out.push_str(&row.title);
    if let Some(username) = &row.username {
        out.push(' ');
        out.push_str(username);
    }
    for url in &row.urls {
        out.push(' ');
        out.push_str(url);
    }
    for tag in &row.tags {
        out.push(' ');
        out.push_str(tag);
    }
    out
}

fn sort_rows(rows: &mut [&IndexRow], order: SortOrder) {
    match order {
        // `last_used_at` is a run-3 concern (activity drives it); until then the
        // honest fallback is update time rather than a field that is always zero.
        SortOrder::RecentlyUsed | SortOrder::RecentlyUpdated => {
            rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        }
        SortOrder::Alphabetical => {
            rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        SortOrder::DateCreated => rows.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
    }
}
