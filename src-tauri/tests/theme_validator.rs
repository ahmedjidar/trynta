//! The imported-theme validator (SPEC-V1 §7.6, AC19).
//!
//! AC19: *"A theme containing `url()` is rejected by the Rust validator."*
//!
//! The criterion names one spelling; the test covers the ones an attacker would
//! actually reach for. CSS lets `url` be written at least six ways that all parse
//! to the same function token, and a validator that only greps for `url(` catches
//! exactly one of them:
//!
//! | Spelling | Why it works in CSS |
//! |---|---|
//! | `URL(…)` | function names are ASCII case-insensitive |
//! | `\75 rl(…)` | `\75` is `u`; the space terminates the hex and is consumed |
//! | `\000075rl(…)` | six hex digits need no terminator |
//! | `u\72 l(…)` | any character may be escaped, not only the first |
//! | `ur/**/l(…)` | comments are stripped before tokenising |
//! | `url (…)` | not a function token, but free to normalise away |
//!
//! Why this matters more than it looks: a theme value that can fetch turns
//! "import a theme" into a beacon that reports your IP and the fact that you
//! opened your password manager, every render. §7 permits exactly three outbound
//! requests in the product and this is not one of them, which is why §7.6 puts the
//! check in Rust and calls CSP "the second layer, not the first".

use keyring_lib::services::theme::{self, ThemeError, ThemeMode, MAX_THEME_BYTES};

/// A theme document with one token value under test.
fn theme_with(value: &str) -> String {
    // The value is JSON-escaped so a backslash in a CSS escape survives into the
    // string the validator actually sees.
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"id":"fixture","name":"Fixture","mode":"dark","tokens":{{"--surface-app":"{escaped}"}}}}"#
    )
}

// ── AC19 proper: url(), however it is spelled ───────────────────────────────

#[test]
fn a_theme_containing_url_is_rejected() {
    let rejected = theme::validate(&theme_with("url(https://attacker.example/beacon.png)"))
        .expect_err("url() must be refused");
    assert!(
        matches!(rejected, ThemeError::ForbiddenFunction { .. }),
        "expected ForbiddenFunction, got {rejected:?}"
    );
}

#[test]
fn every_spelling_of_url_is_rejected() {
    let spellings = [
        // Case folding.
        "URL(https://a.example/x)",
        "Url(https://a.example/x)",
        "uRl(https://a.example/x)",
        // CSS hex escapes, with and without the terminating space.
        r"\75 rl(https://a.example/x)",
        r"\000075rl(https://a.example/x)",
        r"\75rl(https://a.example/x)",
        // An escape anywhere in the identifier, not just the first character.
        r"u\72 l(https://a.example/x)",
        r"ur\6c (https://a.example/x)",
        // Comments, which the CSS parser strips before it tokenises.
        "ur/**/l(https://a.example/x)",
        "u/**/rl(https://a.example/x)",
        "/*x*/url(https://a.example/x)",
        // Whitespace before the parenthesis.
        "url (https://a.example/x)",
        "url\t(https://a.example/x)",
        // Wrapped in something otherwise legitimate.
        "linear-gradient(url(https://a.example/x), #000)",
        // Unterminated comment, which swallows the rest in a real parser too.
        "url(https://a.example/x) /* unterminated",
    ];

    for spelling in spellings {
        let result = theme::validate(&theme_with(spelling));
        assert!(
            result.is_err(),
            "this spelling was accepted and would fetch on render: {spelling:?}"
        );
    }
}

#[test]
fn the_other_fetching_functions_are_rejected_too() {
    // url() is the one AC19 names, but it is not the only value function that
    // reaches the network. Blocking one and admitting the rest would satisfy the
    // criterion and miss the point of it.
    for spelling in [
        "image(https://a.example/x)",
        "image-set(https://a.example/x 1x)",
        "-webkit-image-set(https://a.example/x 1x)",
        "element(#other)",
        "attr(data-x)",
        "expression(alert(1))",
        "-moz-binding(https://a.example/x)",
    ] {
        assert!(
            theme::validate(&theme_with(spelling)).is_err(),
            "{spelling:?} was accepted"
        );
    }
}

#[test]
fn a_value_cannot_escape_its_declaration() {
    // A value that can close its own declaration can write a rule of its own, and
    // a rule can carry a background image even if the value grammar cannot.
    for spelling in [
        "#000; background: url(https://a.example/x)",
        "#000 } body { background: url(https://a.example/x)",
        "#000 <style>",
        "#000 @import url(https://a.example/x)",
    ] {
        assert!(
            theme::validate(&theme_with(spelling)).is_err(),
            "{spelling:?} was accepted"
        );
    }
}

// ── The accept side: a valid theme must actually work ───────────────────────

#[test]
fn a_custom_property_only_theme_is_accepted() {
    // Shaped like the real token layer: colours, alpha colours, lengths,
    // unitless numbers, durations, easing curves, var() references, calc(), a
    // quoted font stack and a multi-part shadow.
    let json = r##"{
      "id": "midnight",
      "name": "Midnight",
      "mode": "dark",
      "tokens": {
        "--surface-app": "#080A0F",
        "--surface-chrome": "rgba(11, 14, 20, .74)",
        "--text-secondary": "#FFFFFF80",
        "--accent": "oklch(72% 0.12 275)",
        "--radius-lg": "12px",
        "--space-4": "16px",
        "--weight-bold": "700",
        "--duration-slow": "0.26s",
        "--stagger-meter": "60ms",
        "--ease-spring": "cubic-bezier(.32, .72, 0, 1)",
        "--strength-2": "var(--status-warning)",
        "--measure-pane": "calc(100% - 32px)",
        "--font-sans": "'Manrope', system-ui, sans-serif",
        "--shadow-window": "0 0 0 .5px rgba(255, 255, 255, .07), 0 32px 90px rgba(0, 0, 0, .75)",
        "--blur-vibrancy": "saturate(180%) blur(28px)"
      }
    }"##;

    let theme = theme::validate(json).expect("a legitimate theme must be accepted");
    assert_eq!(theme.id, "midnight");
    assert_eq!(theme.name, "Midnight");
    assert_eq!(theme.mode, ThemeMode::Dark);
    assert_eq!(theme.tokens.len(), 15);
    assert_eq!(
        theme.tokens.get("--surface-app").map(String::as_str),
        Some("#080A0F"),
        "an accepted value must survive validation unmodified"
    );
}

#[test]
fn a_light_theme_is_accepted() {
    let json =
        r##"{"id":"noon","name":"Noon","mode":"light","tokens":{"--surface-app":"#F6F7FA"}}"##;
    let theme = theme::validate(json).expect("light themes are themes too");
    assert_eq!(theme.mode, ThemeMode::Light);
}

// ── Structure ───────────────────────────────────────────────────────────────

#[test]
fn only_custom_properties_may_be_set() {
    // §7.6: "custom-property declarations only". A theme that could set `content`
    // or `background` would not need a url() to be dangerous.
    for name in [
        "background",
        "content",
        "-webkit-mask",
        "--Surface-App",
        "-surface",
        "--",
        "--surface app",
        "--surface;app",
        r"--\75 rl",
    ] {
        let json = format!(
            r##"{{"id":"f","name":"F","mode":"dark","tokens":{{"{}":"#000"}}}}"##,
            name.replace('\\', "\\\\")
        );
        let err = theme::validate(&json).expect_err("{name} was accepted as a property");
        assert!(
            matches!(err, ThemeError::NotACustomProperty { .. }),
            "{name:?} gave {err:?}, expected NotACustomProperty"
        );
    }
}

#[test]
fn a_malformed_document_is_refused_cleanly() {
    for json in [
        "",
        "null",
        "[]",
        "{}",
        r#"{"id":"f","name":"F","mode":"dark"}"#,
        r#"{"id":"f","name":"F","mode":"sideways","tokens":{}}"#,
        r#"{"id":"","name":"F","mode":"dark","tokens":{}}"#,
        r#"{"id":"f","name":"  ","mode":"dark","tokens":{}}"#,
        "not json at all",
    ] {
        assert!(
            theme::validate(json).is_err(),
            "{json:?} should not have validated"
        );
    }
}

#[test]
fn an_identity_that_could_be_read_as_markup_is_refused() {
    // The name is rendered in a theme picker.
    for name in ["<script>", "F</style>", "F{", "F;", "F\\"] {
        let json = format!(
            r#"{{"id":"f","name":"{}","mode":"dark","tokens":{{}}}}"#,
            name.replace('\\', "\\\\")
        );
        assert_eq!(
            theme::validate(&json).unwrap_err(),
            ThemeError::BadIdentity,
            "{name:?} was accepted as a display name"
        );
    }
}

#[test]
fn an_oversized_document_is_refused_before_parsing() {
    // Bounded so the validator cannot be made the expensive part of an import.
    let padding = "x".repeat(MAX_THEME_BYTES);
    let json = format!(r#"{{"id":"f","name":"{padding}","mode":"dark","tokens":{{}}}}"#);
    assert_eq!(theme::validate(&json).unwrap_err(), ThemeError::TooLarge);
}

#[test]
fn an_error_never_quotes_the_rejected_value() {
    // A theme file is untrusted input, and its values are the part an attacker
    // controls. An error that interpolated one would put attacker text into a
    // string destined for a toast or a log (CLAUDE.md §4.6).
    let sentinel = "https://attacker.example/THEME-SENTINEL-Qk4";
    let err =
        theme::validate(&theme_with(&format!("url({sentinel})"))).expect_err("must be refused");
    let rendered = format!("{err} {err:?}");
    assert!(
        !rendered.contains("attacker.example") && !rendered.contains("THEME-SENTINEL"),
        "the error quoted the rejected value: {rendered}"
    );
    assert!(
        rendered.contains("--surface-app"),
        "the error should name the token so the user can find it: {rendered}"
    );
}
