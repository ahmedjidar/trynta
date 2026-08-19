// SPDX-License-Identifier: AGPL-3.0-or-later
//! Which services accept an authenticator app (SPEC-V1 §7.4, "Missing 2FA").
//!
//! A compiled-in directory keyed by registrable domain, and a single question:
//! *could this item have a one-time code?* If it could and does not, that is worth
//! telling the user; if it could not, saying nothing is the only honest option.
//!
//! ## Why the list is ours
//!
//! §7.4 requires the directory's licence to permit redistribution and requires that
//! to be verified before shipping. The obvious source — 2fa.directory, formerly
//! twofactorauth.org — is a community dataset whose terms are not unambiguously
//! clear for redistribution inside a commercial binary. ADD-001 spent an entire
//! addendum refusing to guess at licences for brand icons; guessing here would
//! undo that reasoning for a smaller prize. So `assets/twofactor-directory.tsv` was
//! written for this product, and the header of that file says so.
//!
//! ## Why it counts against a smaller denominator than you might expect
//!
//! An entry means *a standard TOTP app is accepted*. Apple, Steam and most retail
//! banks are deliberately absent: their second factor is real, often stronger, and
//! cannot be stored in Trynta. Counting them as "capable" would produce a nag the
//! user can never satisfy and a health score that never reaches 100 for reasons
//! outside their control.
//!
//! Before this existed the report reported `two_factor_capable = 0` unconditionally,
//! which sounds conservative and is not: §7.4's health formula redistributes the 2FA
//! term's 20 points across the other three when nothing is capable, so *every* user
//! was silently scored on a different formula from the one the breakdown showed.

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::services::icons;

/// The directory, as shipped.
static DIRECTORY_SOURCE: &str = include_str!("../../assets/twofactor-directory.tsv");

/// Parsed once, on first use.
static DIRECTORY: OnceLock<HashSet<&'static str>> = OnceLock::new();

/// The set of registrable domains known to accept a TOTP app.
fn directory() -> &'static HashSet<&'static str> {
    DIRECTORY.get_or_init(|| {
        DIRECTORY_SOURCE
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    })
}

/// How many services the directory covers, for the diagnostics surface.
#[must_use]
pub fn size() -> usize {
    directory().len()
}

/// Whether a registrable domain is in the directory.
///
/// Takes an eTLD+1 only. Callers with a URL should go through [`capable_for`],
/// which reduces first — a substring or suffix match here would be the same class
/// of bug CLAUDE.md §4.8 bans for autofill, on the same data.
#[must_use]
pub fn is_capable(domain: &str) -> bool {
    directory().contains(domain.to_ascii_lowercase().as_str())
}

/// Whether any of an item's URLs names a service that accepts a TOTP app.
///
/// Reduces each URL to eTLD+1 through the Public Suffix List before looking it up,
/// so `https://github.com/settings/security` and `github.com` are the same service.
/// An item with no usable URL is not capable: there is nothing to look up, and
/// guessing from the title is how a password manager tells someone their local
/// router supports two-factor.
#[must_use]
pub fn capable_for(urls: &[String]) -> bool {
    urls.iter()
        .filter_map(|raw| icons::registrable_domain(raw))
        .any(|domain| is_capable(&domain))
}

#[cfg(test)]
mod tests {
    use super::{capable_for, is_capable, size};

    fn urls(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_directory_is_not_empty_and_parses() {
        assert!(size() > 150, "the directory shrank to {}", size());
    }

    #[test]
    fn a_known_service_is_capable_through_any_of_its_urls() {
        assert!(is_capable("github.com"));
        assert!(capable_for(&urls(&["https://github.com/login?next=/x"])));
        assert!(capable_for(&urls(&["https://www.github.com/"])));
        // A deep link on a subdomain still reduces to the same registrable domain.
        assert!(capable_for(&urls(&["https://gist.github.com/someone/abc"])));
    }

    #[test]
    fn an_unknown_service_is_not_capable() {
        assert!(!is_capable("example.test"));
        assert!(!capable_for(&urls(&[
            "https://intranet.example.test/login"
        ])));
    }

    #[test]
    fn an_item_with_no_usable_url_is_not_capable() {
        assert!(!capable_for(&[]));
        assert!(!capable_for(&urls(&["https://192.168.1.1"])));
        assert!(!capable_for(&urls(&["not a url at all"])));
    }

    /// The failure mode CLAUDE.md §4.8 bans for autofill, on the same data.
    #[test]
    fn matching_is_never_by_substring() {
        assert!(!is_capable("notgithub.com"));
        assert!(!is_capable("github.com.evil.test"));
        assert!(!capable_for(&urls(&["https://github.com.evil.test/login"])));
    }

    #[test]
    fn case_does_not_matter() {
        assert!(is_capable("GitHub.com"));
        assert!(capable_for(&urls(&["HTTPS://GITHUB.COM/"])));
    }

    #[test]
    fn every_entry_is_a_registrable_domain() {
        // A host like `aws.amazon.com` would never match, because lookups are always
        // done on an eTLD+1. An entry that cannot match is worse than absent: it
        // looks like coverage and provides none.
        for entry in super::directory() {
            let reduced = crate::services::icons::registrable_domain(entry);
            assert_eq!(
                reduced.as_deref(),
                Some(*entry),
                "{entry} is not its own registrable domain"
            );
        }
    }
}
