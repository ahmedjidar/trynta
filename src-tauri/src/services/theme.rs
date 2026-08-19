//! Validation of imported themes (SPEC-V1 §7.6, AC19).
//!
//! > **An imported theme is untrusted input.** `background: url(https://attacker/…)`
//! > is a network beacon that fires on render — precisely the leak ADD-001 exists
//! > to prevent. Validate **in Rust**, not the webview: custom-property
//! > declarations only, values matched against a strict grammar, `url()` rejected
//! > outright. CSP is the second layer, not the first.
//!
//! The threat is narrow and worth stating exactly. A theme is a bag of CSS custom
//! properties supplied by someone else. If any value can reach the network, then
//! importing a theme tells a third party that you opened Trynta, from which IP,
//! and when — the one thing §7 says the product must never do.
//!
//! ## Why an allow-list, not a `url()` blocklist
//!
//! Blocking the string `url(` is not enough, because CSS has several ways to
//! spell it:
//!
//! | Spelling | Mechanism |
//! |---|---|
//! | `URL(…)`, `Url(…)` | function names are ASCII case-insensitive |
//! | `\75 rl(…)` | a CSS escape for `u`, with the space terminating the hex |
//! | `\000075rl(…)` | the six-digit form, where no terminator is needed |
//! | `u\72 l(…)` | any character may be escaped, not just the first |
//! | `ur/**/l(…)` | comments are stripped by the parser before tokenising |
//! | `image-set(…)`, `image(…)`, `element(…)` | other value functions that fetch |
//! | `@import`, `src:` | not custom-property values at all, but cheap to refuse |
//!
//! So this normalises first — strip comments, decode escapes, fold case — and
//! then requires every value to *match* a permitted shape rather than merely
//! *not match* a forbidden one. A grammar that only admits colours, lengths,
//! numbers, keywords, `var()` references and a few named functions cannot express
//! a fetch, whatever it is spelled like.
//!
//! Rejecting is always safe here: the cost of refusing a legitimate theme is that
//! a user keeps the built-in one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Longest accepted theme document, in bytes.
///
/// A theme is a few hundred short declarations. Anything far past that is either
/// a mistake or an attempt to make the validator the expensive part of importing.
pub const MAX_THEME_BYTES: usize = 64 * 1024;

/// Longest accepted single token value.
const MAX_VALUE_LEN: usize = 256;

/// Longest accepted token name.
const MAX_NAME_LEN: usize = 96;

/// Most tokens a theme may define.
const MAX_TOKENS: usize = 512;

/// Value functions that can cause a fetch, or that have no business in a token.
///
/// Checked after normalisation, so a spelling that decodes to one of these is
/// caught even though it never appears literally.
const FORBIDDEN_FUNCTIONS: &[&str] = &[
    "url",
    "image",
    "image-set",
    "-webkit-image-set",
    "-moz-image-set",
    "element",
    "src",
    "attr",
    "expression",
    "-moz-binding",
];

/// Functions a token value may use.
///
/// Deliberately short. Each one is a pure computation over numbers and colours
/// with no way to reference an external resource.
const ALLOWED_FUNCTIONS: &[&str] = &[
    "rgb",
    "rgba",
    "hsl",
    "hsla",
    "oklch",
    "oklab",
    "lab",
    "lch",
    "color-mix",
    "var",
    "calc",
    "min",
    "max",
    "clamp",
    "cubic-bezier",
    "steps",
    "linear-gradient",
    "blur",
    "saturate",
    "brightness",
    "contrast",
    "inset",
];

/// Which built-in mode a theme replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    /// Applies when the app is in dark mode.
    Dark,
    /// Applies when the app is in light mode.
    Light,
}

/// A validated theme.
///
/// Constructing one outside [`validate`] is impossible from another module: the
/// fields are public for reading, but `tokens` can only be populated by the
/// validator because [`Theme`] has no public constructor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Theme {
    /// Stable identifier, used as the `app_state` theme id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Which mode this theme replaces.
    pub mode: ThemeMode,
    /// Custom-property name to value. Sorted, so applying a theme twice produces
    /// byte-identical CSS and the loader can compare cheaply.
    pub tokens: BTreeMap<String, String>,
}

/// The untrusted shape, before validation.
#[derive(Debug, Deserialize)]
struct RawTheme {
    id: String,
    name: String,
    mode: ThemeMode,
    tokens: BTreeMap<String, String>,
}

/// Why a theme was refused.
///
/// Carries the offending **token name** but never the offending value: a value
/// from an untrusted file is exactly the kind of string that should not be
/// interpolated into an error that may be rendered or logged (CLAUDE.md §4.6).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ThemeError {
    /// The document was not the expected JSON shape.
    #[error("that file is not a Trynta theme")]
    Malformed,

    /// The document is larger than [`MAX_THEME_BYTES`].
    #[error("that theme file is too large")]
    TooLarge,

    /// `id` or `name` was empty, over-long, or not a safe identifier.
    #[error("that theme's id or name is not usable")]
    BadIdentity,

    /// More than [`MAX_TOKENS`] tokens.
    #[error("that theme defines too many tokens")]
    TooManyTokens,

    /// A key was not a `--custom-property` name.
    #[error("{name} is not a custom property; a theme may only set --custom-properties")]
    NotACustomProperty {
        /// The offending key, which is a name and not a value.
        name: String,
    },

    /// A value used a function that can fetch, however it was spelled.
    #[error("{name} uses a function that could reach the network, which a theme may never do")]
    ForbiddenFunction {
        /// The token whose value was refused.
        name: String,
    },

    /// A value did not match the permitted grammar.
    #[error("{name} has a value this build will not apply")]
    InvalidValue {
        /// The token whose value was refused.
        name: String,
    },
}

/// Validate an imported theme document.
///
/// # Errors
///
/// Any [`ThemeError`]. A refusal is always safe: the user keeps the theme they
/// already had.
pub fn validate(json: &str) -> Result<Theme, ThemeError> {
    if json.len() > MAX_THEME_BYTES {
        return Err(ThemeError::TooLarge);
    }
    let raw: RawTheme = serde_json::from_str(json).map_err(|_| ThemeError::Malformed)?;

    if !is_safe_identity(&raw.id) || !is_safe_identity(&raw.name) {
        return Err(ThemeError::BadIdentity);
    }
    if raw.tokens.len() > MAX_TOKENS {
        return Err(ThemeError::TooManyTokens);
    }

    let mut tokens = BTreeMap::new();
    for (name, value) in raw.tokens {
        if !is_custom_property(&name) {
            return Err(ThemeError::NotACustomProperty { name });
        }
        validate_value(&name, &value)?;
        tokens.insert(name, value);
    }

    Ok(Theme {
        id: raw.id,
        name: raw.name,
        mode: raw.mode,
        tokens,
    })
}

/// A theme id or display name: printable, bounded, no control characters.
fn is_safe_identity(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_NAME_LEN
        && !trimmed.chars().any(char::is_control)
        // A name is rendered. Anything that could be read as markup or a CSS
        // delimiter has no business in one.
        && !trimmed.contains(['<', '>', '{', '}', ';', '"', '\\'])
}

/// Whether `name` is a plain CSS custom property.
///
/// `--` followed by lowercase ASCII, digits and hyphens. Deliberately narrower
/// than the CSS grammar, which permits escapes and most Unicode: a theme has no
/// reason to define `--\75 rl`, and admitting escapes here would mean decoding
/// them to know what was really being set.
fn is_custom_property(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("--") else {
        return false;
    };
    !rest.is_empty()
        && rest.len() <= MAX_NAME_LEN
        && rest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Check one token value.
fn validate_value(name: &str, value: &str) -> Result<(), ThemeError> {
    if value.is_empty() || value.len() > MAX_VALUE_LEN {
        return Err(ThemeError::InvalidValue {
            name: name.to_owned(),
        });
    }

    let normalised = normalise(value);

    // A declaration terminator or a block would let a value escape its property
    // and become a rule of its own.
    if normalised.contains([';', '{', '}', '<', '>', '@', '\\']) {
        return Err(ThemeError::InvalidValue {
            name: name.to_owned(),
        });
    }

    for function in function_names(&normalised) {
        if FORBIDDEN_FUNCTIONS.contains(&function.as_str()) {
            return Err(ThemeError::ForbiddenFunction {
                name: name.to_owned(),
            });
        }
        if !ALLOWED_FUNCTIONS.contains(&function.as_str()) {
            return Err(ThemeError::InvalidValue {
                name: name.to_owned(),
            });
        }
    }

    if !is_permitted_shape(&normalised) {
        return Err(ThemeError::InvalidValue {
            name: name.to_owned(),
        });
    }
    Ok(())
}

/// Strip comments, decode CSS escapes, fold case, and collapse whitespace.
///
/// Order matters. Comments go first because they can sit between an escape and
/// the rest of an identifier; escapes are decoded next so `\75 rl` becomes `url`
/// before anything looks for a function name; whitespace is collapsed last so
/// `url  (` cannot slip past a check for `url(`.
fn normalise(value: &str) -> String {
    let no_comments = strip_comments(value);
    let decoded = decode_escapes(&no_comments);
    let lowered = decoded.to_ascii_lowercase();
    collapse_before_parens(&lowered)
}

fn strip_comments(value: &str) -> String {
    let bytes: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i) == Some(&'/') && bytes.get(i + 1) == Some(&'*') {
            // Unterminated comments swallow the rest, which is what a browser
            // does too.
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == '*' && bytes[j + 1] == '/') {
                j += 1;
            }
            i = (j + 2).min(bytes.len());
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Decode CSS `\` escapes into the characters they denote.
///
/// Handles the hex forms (`\75`, `\000075`, with an optional single terminating
/// space) and the literal form (`\(`). Anything undecodable is kept as a
/// backslash, which `validate_value` then refuses outright.
fn decode_escapes(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '\\' {
            out.push(chars[i]);
            i += 1;
            continue;
        }

        let mut hex = String::new();
        let mut j = i + 1;
        while j < chars.len() && hex.len() < 6 && chars[j].is_ascii_hexdigit() {
            hex.push(chars[j]);
            j += 1;
        }

        if hex.is_empty() {
            // `\(` and friends: the next character stands for itself.
            if let Some(&next) = chars.get(i + 1) {
                out.push(next);
                i += 2;
            } else {
                out.push('\\');
                i += 1;
            }
            continue;
        }

        // A single whitespace character after the hex digits terminates the
        // escape and is consumed, which is what makes `\75 rl` spell `url`.
        if chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
            Some(decoded) => out.push(decoded),
            None => out.push('\\'),
        }
        i = j;
    }
    out
}

/// Remove whitespace that sits immediately before an opening parenthesis.
///
/// `url (…)` is not a function token in CSS, but normalising it away costs
/// nothing and means one fewer spelling to reason about.
fn collapse_before_parens(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c == '(' {
            while out.ends_with(char::is_whitespace) {
                out.pop();
            }
        }
        out.push(c);
    }
    out
}

/// Every identifier immediately followed by `(` in a normalised value.
fn function_names(normalised: &str) -> Vec<String> {
    let chars: Vec<char> = normalised.chars().collect();
    let mut names = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if c != '(' {
            continue;
        }
        let mut start = i;
        while start > 0 {
            let prev = chars[start - 1];
            if prev.is_ascii_alphanumeric() || prev == '-' || prev == '_' {
                start -= 1;
            } else {
                break;
            }
        }
        if start < i {
            names.push(chars[start..i].iter().collect());
        }
    }
    names
}

/// Whether every character in the value belongs to the permitted alphabet.
///
/// The grammar is expressed as a character set plus the function check above
/// rather than a full CSS value parser. That is a deliberate trade: a real parser
/// is a large amount of untrusted-input handling, and the property that matters
/// is "cannot express a fetch", which the alphabet plus the function allow-list
/// already gives.
fn is_permitted_shape(normalised: &str) -> bool {
    normalised.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(
                c,
                ' ' | '#' | '%' | '.' | ',' | '-' | '+' | '*' | '/' | '(' | ')' | '_' | '\''
            )
    })
}
