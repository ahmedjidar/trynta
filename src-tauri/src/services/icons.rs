//! Item identity: bundled brand icons and the monogram fallback (ADD-001).
//!
//! ADD-001 is unambiguous and it is the strongest privacy rule in the product after
//! the no-network rule itself:
//!
//! > Every runtime favicon fetch — direct from the site, or through a third-party
//! > favicon service — announces to an observer that this user holds an account with
//! > that service. Do it for every item and you have transmitted, in the clear, a
//! > complete inventory of the user's accounts.
//!
//! **The HO-001 prototype fetches `google.com/s2/favicons`.** Its own README lists
//! that as a known deviation not to implement, and this module is what replaces it.
//! There is no URL construction anywhere in this file. Grep it: the only strings that
//! leave here are an icon *key* naming a file inside our own bundle, and two initials.
//!
//! ## Resolution
//!
//! 1. Take the item's URLs, parse each, reduce the host to its **registrable domain**
//!    (eTLD+1) through the Public Suffix List.
//! 2. Look that up in the bundled map. A hit gives a key; the frontend renders
//!    `dist/icons/<key>.svg` under `img-src 'self'`.
//! 3. A miss gives a monogram: one or two initials on one of the seven identity tones
//!    the token layer defines, chosen deterministically from the same domain string.
//!
//! Step 1 is eTLD+1 and not the raw host on purpose. `mail.google.com` and
//! `google.com` are one brand and must get one tile, or the list looks like two
//! unrelated accounts. It is the same reduction CLAUDE.md §4.8 requires for autofill
//! matching, and using `contains()` there *"is the difference between a password
//! manager and a phishing accessory"* — so it is worth doing once, correctly, in a
//! module both can share.
//!
//! ## The map is empty
//!
//! Nothing is bundled yet — `THIRD-PARTY-NOTICES.md` records the icon table as
//! `(none bundled yet)`, and ADD-001 stages sourcing as a content workstream with a
//! legal dependency that must not block engineering. So **every item renders a
//! monogram today**, which is the correct behaviour for an unmapped domain and is
//! exactly what the criterion asks for: an unknown domain renders a monogram with no
//! network activity. Adding a brand later is a data change to [`BUNDLED`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Number of identity tones the token layer defines (`--identity-1` … `-7`).
///
/// Each carries white text at ≥5.6:1 per the contrast report, so any tone is safe
/// for a monogram and the choice is purely about telling items apart.
pub const IDENTITY_TONES: u8 = 7;

/// Longest monogram. Two characters is what the design's tile fits.
const MAX_INITIALS: usize = 2;

/// Domain (eTLD+1) to bundled icon key.
///
/// **Deliberately empty.** See the module docs: coverage is a content workstream,
/// and an empty map means every item takes the monogram path, which is the path that
/// must be correct anyway. Populating this is a one-line-per-brand data change, and
/// removing a brand for a takedown is a one-line deletion — ADD-001 requires both.
static BUNDLED: &[(&str, &str)] = &[];

/// Card-brand icon keys, a separate namespace per ADD-001.
///
/// > Card brands — Visa, Mastercard, Amex — are derived from the card number, not a
/// > domain, and need their own `card:<brand>` namespace in the map.
///
/// Also empty, for the same reason.
static BUNDLED_CARDS: &[(&str, &str)] = &[];

/// How an item's tile should be drawn.
///
/// A closed enum rather than an optional key, because "bundled icon" and "monogram"
/// are different renderings, not one rendering with a missing field. A frontend
/// holding `Option<String>` has to decide what a `None` means; holding this, it
/// cannot get it wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Icon {
    /// A bundled SVG. The frontend loads `dist/icons/<key>.svg`.
    Bundled {
        /// Key into the bundled set. Never a URL, never anything user-supplied.
        key: String,
    },
    /// A locally generated monogram tile.
    Monogram {
        /// One or two uppercase initials.
        initials: String,
        /// Which `--identity-N` to fill with, `1..=IDENTITY_TONES`.
        tone: u8,
    },
}

/// The registrable domain (eTLD+1) of a URL, lowercased.
///
/// Returns `None` for anything that is not a parseable URL with a public-suffix
/// host — an IP address, a `localhost`, an internal hostname with no public suffix,
/// or free text someone typed into a URL field.
///
/// Exposed because §7.4's fix flow needs exactly this: it opens
/// `https://<etld+1>/.well-known/change-password` **in the browser** and must never
/// probe it from the app.
#[must_use]
pub fn registrable_domain(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // A bare `example.com` is not a URL, and users type bare domains constantly.
    // Parsing with a scheme prepended is a formatting convenience, not a network
    // operation — nothing here connects to anything.
    let parsed = url::Url::parse(trimmed)
        .or_else(|_| url::Url::parse(&format!("https://{trimmed}")))
        .ok()?;

    let host = parsed.host_str()?;
    // An IP literal has no registrable domain and `psl` would return nonsense for
    // one, so reject it before asking.
    if parsed
        .host()
        .is_some_and(|h| !matches!(h, url::Host::Domain(_)))
    {
        return None;
    }

    let suffix = psl::domain(host.as_bytes())?;
    let domain = std::str::from_utf8(suffix.as_bytes()).ok()?;
    Some(domain.to_ascii_lowercase())
}

/// Pick an identity tone from a string.
///
/// FNV-1a rather than `DefaultHasher`: `std`'s hasher is explicitly not stable across
/// releases, and a tile that changes colour when the toolchain changes would look
/// like data corruption to a user who has learned to recognise their items by colour.
/// This is not a security hash and does not need to be — it only has to be the same
/// tomorrow.
#[must_use]
pub fn tone_for(identity: &str) -> u8 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET;
    for byte in identity.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    u8::try_from(hash % u64::from(IDENTITY_TONES)).unwrap_or(0) + 1
}

/// One or two initials for a title.
///
/// Operates on `char`s, so a title starting with a multi-byte character produces a
/// whole character rather than a broken byte. Takes the first letter of each of the
/// first two words, falling back to the first two characters of a single word, and
/// `?` for a title with no usable characters — an item can be saved with an
/// all-emoji name and the tile still has to render something.
#[must_use]
pub fn initials_for(title: &str) -> String {
    let words: Vec<&str> = title
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '.')
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .collect();

    let mut out = String::new();
    for word in words.iter().take(MAX_INITIALS) {
        if let Some(first) = word.chars().find(|c| c.is_alphanumeric()) {
            out.extend(first.to_uppercase());
        }
    }

    if out.len() < 2 {
        if let Some(single) = words.first() {
            let letters: Vec<char> = single.chars().filter(|c| c.is_alphanumeric()).collect();
            if letters.len() >= 2 && out.chars().count() == 1 {
                out.clear();
                for c in letters.iter().take(MAX_INITIALS) {
                    out.extend(c.to_uppercase());
                }
            }
        }
    }

    if out.is_empty() {
        return "?".to_owned();
    }
    out
}

/// Resolve an item's tile from its URLs and title.
///
/// The first URL with a registrable domain decides the identity, so the tone is
/// stable when a user adds a second URL to an existing item. With no usable URL the
/// title is the identity string — which means renaming an item can change its tone,
/// and that is the right trade: the alternative is storing a tone, which would be a
/// new item field and a migration for a decoration.
#[must_use]
pub fn resolve(urls: &[String], title: &str) -> Icon {
    let domain = urls.iter().find_map(|u| registrable_domain(u));

    if let Some(domain) = &domain {
        if let Some(key) = lookup(domain) {
            return Icon::Bundled {
                key: key.to_owned(),
            };
        }
    }

    let identity = domain.as_deref().unwrap_or(title);
    Icon::Monogram {
        initials: initials_for(title),
        tone: tone_for(identity),
    }
}

/// Resolve a card tile from its brand, per ADD-001's `card:` namespace.
///
/// `brand` is derived from the card number by the caller, not from a domain.
#[must_use]
pub fn resolve_card(brand: &str, title: &str) -> Icon {
    let key = format!("card:{}", brand.to_ascii_lowercase());
    if let Some(found) = BUNDLED_CARDS
        .iter()
        .find_map(|(b, k)| (*b == key.as_str()).then_some(*k))
    {
        return Icon::Bundled {
            key: found.to_owned(),
        };
    }
    Icon::Monogram {
        initials: initials_for(title),
        tone: tone_for(&key),
    }
}

/// The bundled key for a registrable domain, if one is mapped.
#[must_use]
pub fn lookup(domain: &str) -> Option<&'static str> {
    BUNDLED
        .iter()
        .find_map(|(d, key)| (*d == domain).then_some(*key))
}

/// The whole map, for the notices-file generator ADD-001 asks for.
///
/// > The icon map carries `source`, `variant`, `licence` and `brand_hex` from day
/// > one, so this table is generated from the map rather than maintained by hand.
#[must_use]
pub fn bundled_map() -> BTreeMap<&'static str, &'static str> {
    BUNDLED.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_map_is_empty_and_that_is_deliberate() {
        // A guard against this test's own premise going stale rather than an
        // assertion that it must stay empty: when brands land, this fails and the
        // person adding them updates THIRD-PARTY-NOTICES.md in the same commit,
        // which is what ADD-001 requires.
        assert!(
            BUNDLED.is_empty() && BUNDLED_CARDS.is_empty(),
            "brands are bundled now — add their source, variant and licence to \
             THIRD-PARTY-NOTICES.md and update this test"
        );
    }

    #[test]
    fn a_subdomain_reduces_to_its_registrable_domain() {
        // The whole reason eTLD+1 is used rather than the host: one brand, one tile.
        for input in [
            "https://mail.google.com/mail/u/0",
            "https://google.com",
            "google.com",
            "HTTPS://MAIL.GOOGLE.COM/",
            "https://accounts.google.com:443/signin",
        ] {
            assert_eq!(
                registrable_domain(input).as_deref(),
                Some("google.com"),
                "{input}"
            );
        }
    }

    #[test]
    fn multi_part_suffixes_are_handled_by_the_public_suffix_list() {
        // The case a naive "last two labels" split gets wrong, and the reason this
        // needs a real PSL rather than string surgery.
        assert_eq!(
            registrable_domain("https://www.bbc.co.uk/news").as_deref(),
            Some("bbc.co.uk")
        );
        assert_eq!(
            registrable_domain("https://shop.example.com.au").as_deref(),
            Some("example.com.au")
        );
    }

    #[test]
    fn things_with_no_registrable_domain_are_refused() {
        for input in [
            "",
            "   ",
            "not a url at all",
            "https://192.168.1.1/admin",
            "http://[::1]:8080",
            "https://localhost:3000",
            "file:///etc/passwd",
        ] {
            assert_eq!(registrable_domain(input), None, "{input:?}");
        }
    }

    #[test]
    fn a_tone_is_stable_and_in_range() {
        for identity in ["google.com", "bbc.co.uk", "", "Acme Corp", "a"] {
            let tone = tone_for(identity);
            assert!(
                (1..=IDENTITY_TONES).contains(&tone),
                "{identity:?} gave tone {tone}, outside 1..={IDENTITY_TONES}"
            );
            assert_eq!(tone, tone_for(identity), "tone must be deterministic");
        }
    }

    #[test]
    fn tones_are_spread_across_the_ramp() {
        // A hash that mapped every real domain onto one tone would satisfy every
        // other test here and make the whole ramp pointless.
        let domains = [
            "google.com",
            "github.com",
            "amazon.com",
            "bbc.co.uk",
            "stripe.com",
            "figma.com",
            "cloudflare.com",
            "wikipedia.org",
            "reddit.com",
            "netflix.com",
            "spotify.com",
            "apple.com",
        ];
        let distinct: std::collections::BTreeSet<u8> =
            domains.iter().map(|d| tone_for(d)).collect();
        assert!(
            distinct.len() >= 4,
            "12 domains produced only {} distinct tones — the ramp is not being used",
            distinct.len()
        );
    }

    #[test]
    fn initials_come_from_word_boundaries() {
        assert_eq!(initials_for("Acme Corp"), "AC");
        assert_eq!(initials_for("github"), "GI");
        assert_eq!(initials_for("my-bank-login"), "MB");
        assert_eq!(initials_for("  spaced   out  "), "SO");
        assert_eq!(initials_for("X"), "X");
    }

    #[test]
    fn initials_never_split_a_character_or_come_back_empty() {
        // An item can be named anything, and a tile that renders half a UTF-8
        // sequence is a visible mess rather than a caught error.
        assert_eq!(initials_for(""), "?");
        assert_eq!(initials_for("   "), "?");
        assert_eq!(initials_for("🔐🔑"), "?");
        assert_eq!(initials_for("Ökonomie Bank"), "ÖB");
        // Every result is whole characters.
        for title in ["Ökonomie", "日本語", "ß"] {
            let out = initials_for(title);
            assert_eq!(out, String::from_utf8(out.clone().into_bytes()).unwrap());
        }
    }

    #[test]
    fn an_unmapped_domain_resolves_to_a_monogram() {
        // ADD-001's verification list: "An unknown domain renders a monogram with no
        // network activity."
        let icon = resolve(&["https://acme.test/login".to_owned()], "Acme Corp");
        match icon {
            Icon::Monogram { initials, tone } => {
                assert_eq!(initials, "AC");
                assert!((1..=IDENTITY_TONES).contains(&tone));
            }
            Icon::Bundled { key } => panic!("nothing is bundled, yet got {key}"),
        }
    }

    #[test]
    fn the_tone_survives_adding_a_second_url() {
        // A user editing an item to add another URL must not watch its tile change
        // colour. The first URL with a registrable domain decides.
        let one = resolve(&["https://acme.test".to_owned()], "Acme");
        let two = resolve(
            &[
                "https://acme.test".to_owned(),
                "https://acme.example".to_owned(),
            ],
            "Acme",
        );
        assert_eq!(one, two);
    }

    #[test]
    fn an_item_with_no_usable_url_falls_back_to_its_title() {
        let from_title = resolve(&[], "Acme Corp");
        let from_junk = resolve(&["not a url".to_owned()], "Acme Corp");
        assert_eq!(
            from_title, from_junk,
            "unusable URLs must not change the tile"
        );
        assert_eq!(from_title, resolve(&[String::new()], "Acme Corp"));
    }

    #[test]
    fn subdomains_of_one_brand_share_a_tile() {
        let mail = resolve(&["https://mail.google.com".to_owned()], "Gmail");
        let drive = resolve(&["https://drive.google.com".to_owned()], "Gmail");
        assert_eq!(mail, drive);
    }

    #[test]
    fn a_card_resolves_from_its_brand_not_a_domain() {
        let visa = resolve_card("VISA", "Everyday card");
        let amex = resolve_card("amex", "Everyday card");
        assert_ne!(visa, amex, "different brands must not share a tone");
        // Case-insensitive, so the caller's normalisation does not change the tile.
        assert_eq!(visa, resolve_card("visa", "Everyday card"));
    }
}
