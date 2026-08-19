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

/// Characters that would let a value stop being a value.
///
/// `;` ends the declaration, `{}` open a rule, `<>` start markup, `@` starts an
/// at-rule, and a surviving `\` is an escape [`decode_escapes`] could not resolve.
const STRUCTURAL: &[char] = &[';', '{', '}', '<', '>', '@', '\\'];

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
    #[error("{name} uses {found}(), which could reach the network — a theme may never do that")]
    ForbiddenFunction {
        /// The token whose value was refused.
        name: String,
        /// The function name, already matched against [`FORBIDDEN_FUNCTIONS`].
        found: String,
    },

    /// A value called a function that is not on the allow-list.
    #[error("{name} uses {found}(), which is not one of the functions a theme may call")]
    UnknownFunction {
        /// The token whose value was refused.
        name: String,
        /// The function name, parsed out of the value.
        found: String,
    },

    /// A value contained a character the grammar does not admit.
    ///
    /// Carries the offending **character**, never the value. One character is enough
    /// to find and fix the line, and is not a quotation of untrusted input.
    #[error("{name} contains {found:?}, which a theme value may not")]
    ForbiddenCharacter {
        /// The token whose value was refused.
        name: String,
        /// The single character that failed.
        found: char,
    },

    /// A value contained a CSS comment sequence.
    ///
    /// Refused rather than stripped. [`strip_comments`] is not string-aware, so
    /// `"/*" url(x) "*/"` normalises here to `""` — balanced quotes, nothing left to
    /// object to — while the engine that applies it reads the quotes as literal
    /// strings and the `url()` between them as a fetch. Stripping cannot close that
    /// gap without a full CSS tokeniser; refusing has no such gap, and a token value
    /// has no legitimate use for a comment.
    #[error("{name} contains {found}, and a theme value may not carry a comment")]
    CommentSequence {
        /// The token whose value was refused.
        name: String,
        /// Which sequence: `/*` or `*/`.
        found: &'static str,
    },

    /// A value used double quotes that do not pair up.
    #[error("{name} has an unclosed double quote")]
    UnbalancedQuotes {
        /// The token whose value was refused.
        name: String,
    },

    /// A value was empty, or longer than [`MAX_VALUE_LEN`].
    #[error("{name} is empty or longer than {MAX_VALUE_LEN} characters")]
    ValueLength {
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
        return Err(ThemeError::ValueLength {
            name: name.to_owned(),
        });
    }

    // On the **raw** value, before any transformation: the whole point is that what
    // this validator sees and what the browser applies must not be able to differ.
    if let Some(found) = ["/*", "*/"].into_iter().find(|seq| value.contains(seq)) {
        return Err(ThemeError::CommentSequence {
            name: name.to_owned(),
            found,
        });
    }

    let normalised = normalise(value);

    // A declaration terminator or a block would let a value escape its property
    // and become a rule of its own. A `\` that reached here is an escape
    // `decode_escapes` could not resolve, which is not something to guess at.
    if let Some(found) = normalised.chars().find(|c| STRUCTURAL.contains(c)) {
        return Err(ThemeError::ForbiddenCharacter {
            name: name.to_owned(),
            found,
        });
    }

    for function in function_names(&normalised) {
        if FORBIDDEN_FUNCTIONS.contains(&function.as_str()) {
            return Err(ThemeError::ForbiddenFunction {
                name: name.to_owned(),
                found: function,
            });
        }
        if !ALLOWED_FUNCTIONS.contains(&function.as_str()) {
            return Err(ThemeError::UnknownFunction {
                name: name.to_owned(),
                found: function,
            });
        }
    }

    // Quotes are permitted so a font stack can name a family that has a space in it —
    // `"sf mono", monospace` — which is what the token layer actually holds and so
    // what our own export emits. Balanced pairs only: an unclosed quote is how a value
    // swallows whatever follows it.
    if normalised.matches('"').count() % 2 != 0 {
        return Err(ThemeError::UnbalancedQuotes {
            name: name.to_owned(),
        });
    }

    if let Some(found) = normalised.chars().find(|c| !is_permitted_char(*c)) {
        return Err(ThemeError::ForbiddenCharacter {
            name: name.to_owned(),
            found,
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
///
/// **Every** run of whitespace folds to a single space, newlines included. A shadow
/// lifted out of pretty-printed CSS arrives with embedded newlines and indentation,
/// and it is the same value once folded — refusing it taught nobody anything. This
/// subsumes the old collapse-before-parens rule rather than sitting beside it.
fn normalise(value: &str) -> String {
    let no_comments = strip_comments(value);
    let decoded = decode_escapes(&no_comments);
    let lowered = decoded.to_ascii_lowercase();
    collapse_whitespace(&lowered)
}

/// Remove `/* … */`.
///
/// Unreachable in practice: [`validate_value`] refuses a value carrying either
/// sequence before it gets here. Kept anyway, so that the function-name scan below
/// still sees through `ur/**/l(` if that check is ever moved or removed — the
/// spelling this was written for, and the one AC19 names.
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

/// Fold every run of whitespace to one space, and drop it entirely before a `(`.
///
/// `url (…)` is not a function token in CSS, but normalising the gap away costs
/// nothing and means one fewer spelling to reason about.
fn collapse_whitespace(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending = false;
    for c in value.chars() {
        if c.is_whitespace() {
            pending = true;
            continue;
        }
        if pending && !out.is_empty() && c != '(' {
            out.push(' ');
        }
        pending = false;
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

/// Whether one character belongs to the permitted alphabet.
///
/// The grammar is expressed as a character set plus the function check above
/// rather than a full CSS value parser. That is a deliberate trade: a real parser
/// is a large amount of untrusted-input handling, and the property that matters
/// is "cannot express a fetch", which the alphabet plus the function allow-list
/// already gives.
///
/// `"` is in the set so a font stack can name a family with a space in it — which is
/// what the token layer holds, and therefore what this app's own export emits. It is
/// safe here only because of what runs before it in [`validate_value`]: comments are
/// refused outright, escapes are decoded and a surviving `\` refused, `;` and the
/// block characters are refused wherever they appear, and the quotes themselves must
/// balance. A quote cannot conceal any of those, because none of them survive to
/// reach this point.
fn is_permitted_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            ' ' | '#' | '%' | '.' | ',' | '-' | '+' | '*' | '/' | '(' | ')' | '_' | '\'' | '"'
        )
}
