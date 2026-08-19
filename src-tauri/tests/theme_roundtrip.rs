// SPDX-License-Identifier: AGPL-3.0-or-later
//! Everything this app exports as a theme must import again.
//!
//! `theme_export` writes out the token layer as it stands in the running document.
//! `theme_import` validates a document before applying it. Those two have to agree,
//! and for two releases they did not: the exporter emitted values the validator
//! refused, so the app's own output was the one file guaranteed to fail.
//!
//! Two ways, both from real tokens:
//!
//! | Token | Value | Why it was refused |
//! |---|---|---|
//! | `--font-sans` | `"Manrope", -apple-system, …` | `"` was not in the value alphabet |
//! | `--shadow-window` | `0 0 0 .5px …,\n    0 32px 90px …` | newline was not in the value alphabet |
//!
//! Neither was a security property. A font family with a space in its name has to be
//! quoted — that is CSS, not a choice — and a shadow with two layers is routinely
//! written across two lines. The validator was refusing well-formed CSS because the
//! alphabet had been drawn around the fixtures rather than around the token layer.
//!
//! The existing accept-side test did not catch it, and the reason is worth keeping in
//! mind: its fixture used `'Manrope'` and a single-line shadow. It was written to
//! what the validator accepted rather than to what the app emits, so it agreed with
//! the bug. This test cannot drift that way, because its fixture *is* `tokens.css` —
//! the file the exporter reads through the CSSOM. Change a token to something
//! unimportable and this goes red in the same commit.

use std::collections::BTreeMap;
use std::path::PathBuf;

use keyring_lib::services::theme::{self, ThemeMode};

/// The shipped token layer, which is what the exporter serialises.
fn tokens_css() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("src")
        .join("theme")
        .join("tokens.css");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Drop `/* … */`, so a token name inside a comment is not mistaken for a declaration.
fn without_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(open) = rest.find("/*") {
        out.push_str(&rest[..open]);
        // An unterminated comment runs to the end of the file; nothing after it is a
        // declaration, so stopping here is right rather than merely convenient.
        let Some(close) = rest[open + 2..].find("*/") else {
            return out;
        };
        rest = &rest[open + 2 + close + 2..];
    }
    out.push_str(rest);
    out
}

/// Every `--name: value` declaration in the sheet, in source order.
///
/// Deliberately naive — it is reading our own hand-written CSS, not untrusted input.
/// It only needs to find the declarations that are actually there, and a value in
/// this file never contains a `;`.
fn declarations(css: &str) -> Vec<(String, String)> {
    let stripped = without_comments(css);
    let mut found = Vec::new();
    for line in stripped.split(';') {
        let Some(start) = line.find("--") else {
            continue;
        };
        let decl = &line[start..];
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        // `var(--x)` inside a value also contains `--`; the split above lands on the
        // declaration because the scan starts from the line's first `--`, but a
        // fragment with no name before the colon is not one.
        if name.starts_with("--") && !name.contains(char::is_whitespace) && !value.is_empty() {
            found.push((name.to_owned(), value.to_owned()));
        }
    }
    found
}

/// Serialise like the exporter does: a JSON document with a `tokens` object.
fn document(mode: &str, tokens: &BTreeMap<String, String>) -> String {
    let body = serde_json::to_string(tokens).expect("tokens serialise");
    format!(r#"{{"id":"exported","name":"Exported","mode":"{mode}","tokens":{body}}}"#)
}

#[test]
fn the_token_layer_yields_declarations_to_check() {
    // If the extractor silently found nothing, every assertion below would pass
    // vacuously and this file would be worth less than no test at all.
    let decls = declarations(&tokens_css());
    assert!(
        decls.len() > 100,
        "expected the full token layer, extracted {} declarations",
        decls.len()
    );
    assert!(
        decls.iter().any(|(n, _)| n == "--font-sans"),
        "the quoted font stack should be among them"
    );
    assert!(
        decls.iter().any(|(_, v)| v.contains('\n')),
        "the multi-line shadows should be among them — they are the second half of \
         the bug this test exists for"
    );
}

#[test]
fn every_shipped_token_value_survives_import() {
    // One at a time, so a failure names the token instead of the whole sheet.
    for (name, value) in declarations(&tokens_css()) {
        let mut one = BTreeMap::new();
        one.insert(name.clone(), value.clone());
        let json = document("dark", &one);
        assert!(
            theme::validate(&json).is_ok(),
            "{name} cannot be re-imported: {:?}\n  value: {value:?}",
            theme::validate(&json).unwrap_err()
        );
    }
}

#[test]
fn the_whole_exported_document_round_trips() {
    // And as one document, which is what `theme_export` actually writes: the
    // per-token pass above would not catch a limit that only the full set breaches.
    let tokens: BTreeMap<String, String> = declarations(&tokens_css()).into_iter().collect();
    let json = document("dark", &tokens);

    let theme = theme::validate(&json).expect("our own export must import");
    assert_eq!(theme.mode, ThemeMode::Dark);
    assert_eq!(
        theme.tokens.len(),
        tokens.len(),
        "validation must not drop tokens"
    );
    for (name, value) in &tokens {
        assert_eq!(
            theme.tokens.get(name),
            Some(value),
            "{name} came back changed; an accepted value is stored verbatim"
        );
    }
}

#[test]
fn a_quoted_font_stack_is_accepted() {
    // The exact value from `tokens.css`, spelled the way CSS requires: a family whose
    // name has a space in it must be quoted, so refusing quotes refused valid CSS.
    let mut tokens = BTreeMap::new();
    tokens.insert(
        "--font-sans".to_owned(),
        r#""Manrope", -apple-system, BlinkMacSystemFont, system-ui, sans-serif"#.to_owned(),
    );
    tokens.insert(
        "--font-mono".to_owned(),
        r#"ui-monospace, "SF Mono", Menlo, monospace"#.to_owned(),
    );
    let theme = theme::validate(&document("dark", &tokens)).expect("quoted families are CSS");
    assert_eq!(theme.tokens.len(), 2);
}

#[test]
fn a_multi_line_value_is_accepted() {
    // Copied out of pretty-printed CSS, indentation and all. The value is identical
    // to the one-line spelling once whitespace is folded.
    let mut tokens = BTreeMap::new();
    tokens.insert(
        "--shadow-window".to_owned(),
        "0 0 0 .5px rgba(255, 255, 255, .07),\n                               0 32px 90px rgba(0, 0, 0, .75)".to_owned(),
    );
    theme::validate(&document("dark", &tokens)).expect("a two-layer shadow is one value");
}

#[test]
fn an_unclosed_quote_is_still_refused() {
    // Quotes are admitted so font stacks work, not as an escape hatch. Unbalanced,
    // they are how a value swallows what follows it.
    let mut tokens = BTreeMap::new();
    tokens.insert(
        "--font-sans".to_owned(),
        r#""Manrope, sans-serif"#.to_owned(),
    );
    theme::validate(&document("dark", &tokens)).expect_err("an unclosed quote must be refused");
}

#[test]
fn a_comment_cannot_hide_a_fetch_between_two_strings() {
    // The one that admitting quotes actually opened, and the reason comments are now
    // refused rather than stripped.
    //
    // `strip_comments` is not string-aware — it cannot be without a CSS tokeniser —
    // so it reads the middle of this as a comment and hands the rest of the validator
    // `""`: two balanced quotes and nothing to object to. The engine that applies the
    // value reads it the other way round, as the string `/*`, then a `url()`, then
    // the string `*/`, and fetches. Before quotes were admitted this was blocked only
    // because `"` was not in the alphabet, which is luck rather than a rule.
    for value in [
        r#""/*" url(https://attacker.example/beacon) "*/""#,
        r#""*/" url(https://attacker.example/beacon) "/*""#,
        "#000 /* url(https://attacker.example/beacon) */",
        "ur/**/l(https://attacker.example/beacon)",
    ] {
        let mut tokens = BTreeMap::new();
        tokens.insert("--surface-app".to_owned(), value.to_owned());
        let err = theme::validate(&document("dark", &tokens))
            .expect_err(&format!("{value:?} was accepted"));
        // And the refusal must not quote the value back, comment or not.
        let rendered = format!("{err} {err:?}");
        assert!(
            !rendered.contains("attacker.example"),
            "the error quoted the rejected value: {rendered}"
        );
    }
}

#[test]
fn quotes_do_not_smuggle_anything_past_the_other_rules() {
    // The alphabet gained `"`. Nothing else moved, and this is the assertion that
    // says so: every one of these is inside quotes, and every one is still refused.
    for value in [
        r#""url(https://a.example/x)""#,
        r#""a"; background: url(https://a.example/x)"#,
        r#""a" } body { background: url(https://a.example/x)"#,
        r#""\75 rl(https://a.example/x)""#,
        r#""a" <style>"#,
        r#""a" @import url(https://a.example/x)"#,
        r#""a"/**/url(https://a.example/x)"#,
    ] {
        let mut tokens = BTreeMap::new();
        tokens.insert("--surface-app".to_owned(), value.to_owned());
        assert!(
            theme::validate(&document("dark", &tokens)).is_err(),
            "{value:?} was accepted"
        );
    }
}
