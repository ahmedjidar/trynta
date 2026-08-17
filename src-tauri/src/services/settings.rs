//! The encrypted settings blob (SPEC-V1 §7.5, §4.4a).
//!
//! §7.5: *"Encrypted in the vault, except the §4.5 list."* So there are two
//! settings stores and the split is not arbitrary:
//!
//! | Store | Holds | Why |
//! |---|---|---|
//! | `app_state` (plaintext) | theme id and mode, biometric-enabled, backoff, window geometry, screen-capture flag, update-check flag and the two check timestamps | Every one has to be readable **before** unlock — the lock screen has to render in the user's theme, and the backoff has to gate the attempt |
//! | `app_cache.settings` (encrypted) | everything here | Preferences that reveal something about the user, and imported theme values, which are user data |
//!
//! §4.5's list is exhaustive and adding to it is a spec change, so the default for
//! a new preference is *this* file. Only move one up to `app_state` when it is
//! genuinely needed pre-unlock.
//!
//! ## Forward compatibility
//!
//! Every field carries `#[serde(default)]` and decode failure falls back to
//! defaults rather than erroring. A settings blob is a preference, not a
//! credential: losing one costs the user a re-toggle, and refusing to unlock a
//! vault because a preference did not parse would be absurd. That is the opposite
//! of the rule for item payloads, deliberately — nothing in here is recoverable
//! only from here.

use serde::{Deserialize, Serialize};

use crate::services::theme::{self, Theme};

/// Default clipboard clear delay in seconds (SPEC-V1 §7.5: *"default on, 30 s"*).
pub const DEFAULT_CLIPBOARD_SECONDS: u32 = 30;

/// Bounds on the clipboard timer.
///
/// Below a second the clear races the paste; past five minutes the feature is a
/// pretence. Clamped rather than rejected, because a settings value arriving out of
/// range means a stale or hand-edited blob, and the honest response is the nearest
/// sane behaviour.
pub const MIN_CLIPBOARD_SECONDS: u32 = 5;
/// See [`MIN_CLIPBOARD_SECONDS`].
pub const MAX_CLIPBOARD_SECONDS: u32 = 300;

/// Most imported themes kept at once.
///
/// A bound exists because the blob is decrypted at unlock and held in memory; it is
/// generous because there is no reason to stop someone collecting themes.
pub const MAX_IMPORTED_THEMES: usize = 32;

/// List row density (`--row-h` in the token layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Density {
    /// `--row-item`, 60px. The design's default.
    #[default]
    Comfortable,
    /// `--row-item-compact`, 40px.
    Compact,
}

/// Everything §7.5 keeps inside the vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Whether a copied secret is cleared from the clipboard automatically.
    pub clear_clipboard: bool,
    /// How long a copied secret stays on the clipboard.
    pub clipboard_seconds: u32,
    /// Whether the daily HIBP check may run (SPEC-V1 §7.4).
    pub watch_for_breaches: bool,
    /// Whether every reveal requires the master password, not just one past the
    /// rolling rate limit.
    pub require_master_on_reveal: bool,
    /// List row density.
    pub density: Density,
    /// Imported themes, stored as the **documents they were imported from**.
    ///
    /// Not `Vec<Theme>`, and that is the point. [`Theme`] deliberately has no
    /// `Deserialize` impl: `theme::validate` is the only way to construct one, so a
    /// validated theme cannot be forged by handing serde some JSON. Deriving
    /// `Deserialize` on it just to fit in this struct would throw that away for a
    /// storage convenience.
    ///
    /// Keeping the document also means every theme is **re-validated on load**, so
    /// tightening the grammar later protects themes that are already stored rather
    /// than only new imports.
    pub themes: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // §7.5 says clipboard clearing is on by default, and it is the one
            // default here that is a security property rather than a taste.
            clear_clipboard: true,
            clipboard_seconds: DEFAULT_CLIPBOARD_SECONDS,
            watch_for_breaches: true,
            // Off by default: the rolling 20-per-60s limit already asks for
            // re-auth, and demanding a password per reveal trains users to type
            // their master password constantly, which is its own risk.
            require_master_on_reveal: false,
            density: Density::Comfortable,
            themes: Vec::new(),
        }
    }
}

impl Settings {
    /// Decode a stored blob, falling back to defaults.
    ///
    /// Never fails. See the module note on forward compatibility.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Self {
        postcard::from_bytes(bytes).unwrap_or_default()
    }

    /// Encode for storage.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        postcard::to_stdvec(self).ok()
    }

    /// Clamp anything out of range and drop excess themes.
    ///
    /// Applied on read as well as on write: a blob written by a future build, or
    /// hand-edited, must not be able to put the app in a state its own UI cannot
    /// produce.
    pub fn normalise(&mut self) {
        self.clipboard_seconds = self
            .clipboard_seconds
            .clamp(MIN_CLIPBOARD_SECONDS, MAX_CLIPBOARD_SECONDS);
        self.themes.truncate(MAX_IMPORTED_THEMES);
    }

    /// Every stored theme that still passes validation.
    ///
    /// A document that no longer validates is **dropped from the result**, not
    /// returned and not an error. That happens when the grammar tightens, and the
    /// right behaviour is for the theme to disappear from the picker: the
    /// alternative is applying CSS the current build considers unsafe because an
    /// older one accepted it.
    #[must_use]
    pub fn valid_themes(&self) -> Vec<Theme> {
        self.themes
            .iter()
            .filter_map(|document| theme::validate(document).ok())
            .collect()
    }

    /// The imported theme with this id, if it is present and still valid.
    #[must_use]
    pub fn theme(&self, id: &str) -> Option<Theme> {
        self.valid_themes().into_iter().find(|t| t.id == id)
    }

    /// Insert or replace an imported theme by id.
    ///
    /// Takes the validated [`Theme`] to prove validation happened, and stores the
    /// `document` it came from. Replaces rather than appends, so re-importing a
    /// corrected file does what the user means instead of leaving two entries with
    /// the same name.
    ///
    /// # Errors
    ///
    /// [`ThemeLimit`] if the list is already at [`MAX_IMPORTED_THEMES`] and this
    /// would be a new entry. Replacing an existing one always succeeds, so a user at
    /// the limit can still fix a theme they already have.
    pub fn upsert_theme(&mut self, theme: &Theme, document: &str) -> Result<(), ThemeLimit> {
        let existing = self
            .themes
            .iter()
            .position(|d| theme::validate(d).is_ok_and(|t| t.id == theme.id));

        if let Some(index) = existing {
            document.clone_into(&mut self.themes[index]);
            return Ok(());
        }
        if self.themes.len() >= MAX_IMPORTED_THEMES {
            return Err(ThemeLimit);
        }
        self.themes.push(document.to_owned());
        Ok(())
    }

    /// Remove an imported theme. Removing an absent one is success.
    ///
    /// Returns whether anything was removed, so the caller can report honestly
    /// rather than claiming a deletion that did not happen.
    pub fn remove_theme(&mut self, id: &str) -> bool {
        let before = self.themes.len();
        self.themes
            .retain(|d| !theme::validate(d).is_ok_and(|t| t.id == id));
        before != self.themes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_spec() {
        let settings = Settings::default();
        assert!(
            settings.clear_clipboard,
            "SPEC-V1 §7.5: clipboard clearing is on by default"
        );
        assert_eq!(settings.clipboard_seconds, 30, "§7.5 names 30 seconds");
        assert!(settings.watch_for_breaches);
        assert_eq!(settings.density, Density::Comfortable);
        assert!(settings.themes.is_empty());
    }

    #[test]
    fn an_undecodable_blob_reads_as_defaults_rather_than_failing() {
        // A preference is not a credential. Refusing to open a vault because a
        // settings blob did not parse would be absurd, and there is nothing in
        // here that exists only in here.
        let settings = Settings::decode(b"not postcard at all");
        assert_eq!(settings, Settings::default());
        assert_eq!(Settings::decode(&[]), Settings::default());
    }

    #[test]
    fn the_blob_round_trips() {
        let settings = Settings {
            clear_clipboard: false,
            clipboard_seconds: 45,
            density: Density::Compact,
            ..Settings::default()
        };

        let encoded = settings.encode().expect("encode");
        assert_eq!(Settings::decode(&encoded), settings);
    }

    #[test]
    fn normalise_clamps_a_hand_edited_timer() {
        let mut settings = Settings {
            clipboard_seconds: 0,
            ..Settings::default()
        };
        settings.normalise();
        assert_eq!(settings.clipboard_seconds, MIN_CLIPBOARD_SECONDS);

        settings.clipboard_seconds = 99_999;
        settings.normalise();
        assert_eq!(settings.clipboard_seconds, MAX_CLIPBOARD_SECONDS);
    }

    /// A minimal valid theme document.
    fn document(id: &str, name: &str) -> String {
        format!(
            r##"{{"id":"{id}","name":"{name}","mode":"dark","tokens":{{"--accent":"#123456"}}}}"##
        )
    }

    fn validated(id: &str, name: &str) -> (Theme, String) {
        let document = document(id, name);
        let parsed = theme::validate(&document).expect("the fixture must validate");
        (parsed, document)
    }

    #[test]
    fn a_stored_theme_round_trips_through_validation() {
        let mut settings = Settings::default();
        let (parsed, document) = validated("midnight", "Midnight");
        settings.upsert_theme(&parsed, &document).expect("insert");

        let restored = Settings::decode(&settings.encode().expect("encode"));
        let themes = restored.valid_themes();
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].id, "midnight");
        assert_eq!(themes[0].name, "Midnight");
    }

    #[test]
    fn a_document_that_no_longer_validates_is_dropped_not_returned() {
        // Stands in for the grammar tightening under a theme stored by an older
        // build. It must vanish from the picker rather than be applied.
        let settings = Settings {
            themes: vec![
                document("good", "Good"),
                r#"{"id":"bad","name":"Bad","mode":"dark","tokens":{"--x":"url(https://a/b)"}}"#
                    .to_owned(),
            ],
            ..Settings::default()
        };
        let themes = settings.valid_themes();
        assert_eq!(themes.len(), 1, "the url() theme must not survive");
        assert_eq!(themes[0].id, "good");
    }

    #[test]
    fn reimporting_a_theme_replaces_it_rather_than_duplicating() {
        let mut settings = Settings::default();
        let (first, first_doc) = validated("midnight", "Midnight");
        settings.upsert_theme(&first, &first_doc).expect("first");

        let (second, second_doc) = validated("midnight", "Midnight fixed");
        settings.upsert_theme(&second, &second_doc).expect("second");

        assert_eq!(settings.themes.len(), 1);
        assert_eq!(settings.valid_themes()[0].name, "Midnight fixed");
    }

    #[test]
    fn the_theme_list_is_bounded_but_replacing_always_works() {
        let mut settings = Settings::default();
        for i in 0..MAX_IMPORTED_THEMES {
            let (parsed, doc) = validated(&format!("t{i}"), "T");
            settings
                .upsert_theme(&parsed, &doc)
                .expect("within the bound");
        }
        let (extra, extra_doc) = validated("one-too-many", "Extra");
        assert!(
            settings.upsert_theme(&extra, &extra_doc).is_err(),
            "a new theme past the bound is refused"
        );

        let (replacement, replacement_doc) = validated("t0", "Replaced");
        assert!(
            settings
                .upsert_theme(&replacement, &replacement_doc)
                .is_ok(),
            "but replacing an existing one is not a new entry and must still work"
        );
    }

    #[test]
    fn removing_reports_whether_it_did_anything() {
        let mut settings = Settings::default();
        let (parsed, document) = validated("midnight", "Midnight");
        settings.upsert_theme(&parsed, &document).expect("insert");
        assert!(settings.remove_theme("midnight"));
        assert!(
            !settings.remove_theme("midnight"),
            "removing an absent theme is success, but must not claim a deletion"
        );
    }
}

/// The imported-theme list is full.
///
/// Its own type rather than a `bool` so a caller cannot ignore it by accident, and
/// carrying no data because the only thing to say is which limit was hit and there
/// is exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no room for another imported theme")]
pub struct ThemeLimit;
