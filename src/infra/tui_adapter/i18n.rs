//! UI string bundles. Every label the TUI paints goes through [`I18n::t`];
//! bundles are embedded at compile time, so switching language is a
//! session-only, IO-free operation.

use std::collections::HashMap;

const EN: &str = include_str!("../../../lang/en.toml");
const ES: &str = include_str!("../../../lang/es.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Spanish,
}

impl Language {
    pub fn toggled(self) -> Self {
        match self {
            Language::English => Language::Spanish,
            Language::Spanish => Language::English,
        }
    }

    fn source(self) -> &'static str {
        match self {
            Language::English => EN,
            Language::Spanish => ES,
        }
    }
}

pub struct I18n {
    language: Language,
    strings: HashMap<String, String>,
}

impl I18n {
    pub fn new(language: Language) -> Self {
        // A malformed bundle is a build-time bug (see `bundles_parse`), so
        // it degrades to raw ids on screen instead of taking the TUI down.
        let strings = toml::from_str(language.source()).unwrap_or_default();
        Self { language, strings }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// Unknown ids render as the id itself: a missing string is a visible
    /// bug, never a panic and never a blank label.
    pub fn t<'a>(&'a self, id: &'a str) -> &'a str {
        self.strings.get(id).map(String::as_str).unwrap_or(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_parse_and_agree() {
        let en = I18n::new(Language::English);
        let es = I18n::new(Language::Spanish);
        assert!(!en.strings.is_empty(), "en.toml failed to parse");

        let mut only_en: Vec<_> = en.strings.keys().filter(|k| !es.strings.contains_key(*k)).collect();
        let mut only_es: Vec<_> = es.strings.keys().filter(|k| !en.strings.contains_key(*k)).collect();
        only_en.sort();
        only_es.sort();
        assert!(only_en.is_empty() && only_es.is_empty(), "bundles drifted: en-only {only_en:?}, es-only {only_es:?}");
    }

    #[test]
    fn unknown_id_falls_back_to_the_id() {
        assert_eq!(I18n::new(Language::English).t("no_such_key"), "no_such_key");
    }
}
