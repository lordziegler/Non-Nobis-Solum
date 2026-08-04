//! UI string bundles. Every label the TUI paints goes through [`I18n::t`];
//! bundles are embedded at compile time, so switching language is a
//! session-only, IO-free operation.

use std::collections::HashMap;

const EN: &str = include_str!("../../../lang/en.toml");
const ES: &str = include_str!("../../../lang/es.toml");

/// The languages the TUI ships a bundle for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// `lang/en.toml`.
    English,
    /// `lang/es.toml`.
    Spanish,
}

impl Language {
    /// The other language. With two of them, this is the whole language
    /// switch.
    ///
    /// # Returns
    /// The language that is not this one.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Language::English => Language::Spanish,
            Language::Spanish => Language::English,
        }
    }

    /// Stable across releases: this is what lands in `settings.toml`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Spanish => "es",
        }
    }

    /// Anything unrecognised is English rather than an error — a settings
    /// file is a preference, and a bad one must not stop the TUI opening.
    #[must_use]
    pub fn from_code(code: &str) -> Self {
        match code.trim().to_lowercase().as_str() {
            "es" => Language::Spanish,
            _ => Language::English,
        }
    }

    fn source(self) -> &'static str {
        match self {
            Language::English => EN,
            Language::Spanish => ES,
        }
    }
}

/// A loaded string bundle, and the language it was loaded for.
pub struct I18n {
    language: Language,
    strings: HashMap<String, String>,
}

impl I18n {
    /// Loads the bundle compiled in for `language`.
    ///
    /// # Arguments
    /// * `language` — which bundle to read.
    ///
    /// # Returns
    /// The bundle. A malformed one degrades to raw ids on screen rather
    /// than taking the TUI down — it is a build-time bug, caught by test.
    #[must_use]
    pub fn new(language: Language) -> Self {
        // A malformed bundle is a build-time bug (see `bundles_parse`), so
        // it degrades to raw ids on screen instead of taking the TUI down.
        let strings = toml::from_str(language.source()).unwrap_or_default();
        Self { language, strings }
    }

    /// Which language this bundle is for.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }

    /// Unknown ids render as the id itself: a missing string is a visible
    /// bug, never a panic and never a blank label.
    pub fn t<'a>(&'a self, id: &'a str) -> &'a str {
        self.strings.get(id).map_or(id, String::as_str)
    }

    /// A word out of the reference data — a soil band, a measured property,
    /// a texture, a climate belt — in the reader's language.
    ///
    /// Not [`I18n::t`], and deliberately not held to the same standard.
    /// These ids come from CSV rows rather than from this crate, so the
    /// bundle cannot be exhaustive over them: a table shipping a band
    /// nobody has translated yet must still print something a reader
    /// recognises, not a raw key. The fallback is the id made readable,
    /// which is what the screen showed before any of this existed.
    ///
    /// # Arguments
    /// * `id` — the value as the data file spells it, e.g. `slightly_acid`.
    ///
    /// # Returns
    /// The translation if the bundle has one, otherwise `id` with its
    /// underscores opened out.
    #[must_use]
    pub fn term(&self, id: &str) -> String {
        self.strings.get(&format!("{TERM}{id}")).cloned().unwrap_or_else(|| id.replace('_', " "))
    }
}

/// What [`I18n::term`] prefixes a data value with to look it up. Also what
/// the parity test excuses, in both directions: the data has a language of
/// its own, and the bundle for *that* language would only be restating it.
pub const TERM: &str = "term_";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // `only_en` / `only_es` differ by the language code they are named for,
    // which is the whole point of the comparison.
    #[allow(clippy::similar_names)]
    fn bundles_parse_and_agree() {
        let en = I18n::new(Language::English);
        let es = I18n::new(Language::Spanish);
        assert!(!en.strings.is_empty(), "en.toml failed to parse");

        // Vocabulary out of the reference data is exempt in both
        // directions, because the data has a language of its own and
        // `term` falls back to it. A soil band is written in English, so
        // only `es` translates it; the `andina_colombia` product catalog
        // is written in Spanish, so only `en` does. A bundle carrying the
        // language its data is already in would be lines restating
        // themselves.
        //
        // Everything that is a label of this crate's own is not exempt:
        // that is what this test is for.
        let drifted = |from: &I18n, to: &I18n| {
            let mut keys: Vec<_> =
                from.strings.keys().filter(|k| !to.strings.contains_key(*k) && !k.starts_with(TERM)).cloned().collect();
            keys.sort();
            keys
        };
        let (only_en, only_es) = (drifted(&en, &es), drifted(&es, &en));
        assert!(only_en.is_empty() && only_es.is_empty(), "bundles drifted: en-only {only_en:?}, es-only {only_es:?}");
    }

    #[test]
    fn every_language_round_trips_through_its_settings_code() {
        for language in [Language::English, Language::Spanish] {
            assert_eq!(Language::from_code(language.code()), language);
        }
        // A settings file naming a language that no longer exists opens the
        // TUI in English instead of refusing to open it.
        assert_eq!(Language::from_code("kl"), Language::English);
        assert_eq!(Language::from_code(""), Language::English);
    }

    /// The two halves of [`I18n::term`]. The bundle cannot be exhaustive
    /// over vocabulary that lives in CSV rows, so a translated band has to
    /// read translated and an untranslated one still has to read as a
    /// word — a screen full of `very_strongly_acid` keys would be worse
    /// than the English it replaced.
    #[test]
    fn a_data_term_is_translated_where_it_can_be_and_readable_where_it_cannot() {
        let es = I18n::new(Language::Spanish);
        assert_eq!(es.term("slightly_acid"), "ligeramente ácido");
        assert_eq!(es.term("clay_loam"), "franco arcilloso");
        assert_eq!(es.term("a_band_no_table_ships_yet"), "a band no table ships yet");
        assert_eq!(I18n::new(Language::English).term("slightly_acid"), "slightly acid");
    }

    #[test]
    fn unknown_id_falls_back_to_the_id() {
        assert_eq!(I18n::new(Language::English).t("no_such_key"), "no_such_key");
    }
}
