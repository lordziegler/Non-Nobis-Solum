//! What the TUI remembers between sessions.
//!
//! One module rather than a port with one implementation: the values here
//! are a mix of presentation (language, theme) and planning defaults
//! (profile, strategy, bag weight), and a trait over them would have
//! to live somewhere. `core/ports` is the wrong home — nothing in the
//! domain knows what a theme is — and a trait inside `infra` with exactly
//! one impl is an interface for its own sake. What the settings *did* need
//! was to stop being read and written from inside the render loop, and
//! that is what this file is: one place, two functions, a stated fallback.
//!
//! Written to `$XDG_CONFIG_HOME/non-nobis-solum/settings.toml`, which is
//! deliberately **not** the data root: preferences are not records. A user
//! who deletes their config loses a theme; a user who deletes their data
//! root loses their soil analyses.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Everything the TUI restores on start.
///
/// Every field has a default, and the whole struct falls back to defaults
/// on a missing or corrupt file — see [`load`]. Adding a field is
/// backward-compatible: `#[serde(default)]` on the struct means an older
/// file simply leaves the new one at its default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiSettings {
    /// `en` or `es`, as `i18n::Language` writes it.
    pub language: String,
    /// The theme's own name, matched against `theme::THEMES`.
    pub theme: String,
    pub profile: String,
    /// `composite_plus_simple` or `simple_blend_only`.
    pub strategy: String,
    /// Bag weight is a preference — it is what the local trade sells, and
    /// it is the same for every lot. Planted **area** deliberately is not
    /// here: it is a fact about a field, so it lives on the lot in
    /// `field_context.csv`.
    pub bag_weight_kg: f64,
    /// The lot and crop the last session was working on. Empty means "none
    /// yet", and one that no longer exists is not restored — see
    /// `Tui::new`.
    pub lot: String,
    pub crop: String,
}

impl Default for TuiSettings {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            theme: String::new(),
            profile: "global".to_string(),
            strategy: "composite_plus_simple".to_string(),
            bag_weight_kg: 50.0,
            lot: String::new(),
            crop: String::new(),
        }
    }
}

impl TuiSettings {
    /// Refuses a stored value that would divide by zero downstream. A
    /// hand-edited file is a trust boundary like any other input: the
    /// bounds live here so a bad number becomes the default rather than an
    /// `inf kg/ha` recommendation.
    fn sanitized(mut self) -> Self {
        let defaults = Self::default();
        if !(self.bag_weight_kg.is_finite() && self.bag_weight_kg > 0.0) {
            self.bag_weight_kg = defaults.bag_weight_kg;
        }
        self
    }
}

/// `$XDG_CONFIG_HOME/non-nobis-solum/settings.toml`, or
/// `~/.config/non-nobis-solum/settings.toml`.
pub fn settings_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        // The XDG spec says a relative value is to be ignored.
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));
    base.join("non-nobis-solum").join("settings.toml")
}

/// The stored settings, and a note for the status bar when they could not
/// be read as written.
///
/// Never fails and never panics. A missing file is the normal first run
/// and says nothing; a corrupt one falls back to defaults and **says so**,
/// because silently reverting somebody's preferences looks like the app
/// forgetting rather than the file being broken.
pub fn load() -> (TuiSettings, Option<String>) {
    let path = settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return (TuiSettings::default(), None);
    };
    match toml::from_str::<TuiSettings>(&text) {
        Ok(settings) => (settings.sanitized(), None),
        Err(e) => (
            TuiSettings::default(),
            Some(format!("{} could not be read ({e}); using defaults", path.display())),
        ),
    }
}

/// Best effort, and the caller is expected to keep going: failing to store
/// a theme is not a reason to refuse to draw one.
pub fn save(settings: &TuiSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = toml::to_string_pretty(settings).map_err(|e| e.to_string())?;
    // Written beside and renamed: a truncating write interrupted halfway
    // leaves a file that parses as neither, and the next start would report
    // it as corrupt.
    let temporary = path.with_extension("toml.saving");
    std::fs::write(&temporary, text).map_err(|e| format!("{}: {e}", temporary.display()))?;
    std::fs::rename(&temporary, &path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A round trip through the real serializer, without touching the
    /// user's own config.
    #[test]
    fn settings_survive_a_round_trip_through_toml() {
        let settings = TuiSettings {
            language: "es".to_string(),
            theme: "Imperator".to_string(),
            profile: "andina_colombia".to_string(),
            strategy: "simple_blend_only".to_string(),
            bag_weight_kg: 40.0,
            lot: "LOT-002".to_string(),
            crop: "coffee".to_string(),
        };
        let text = toml::to_string_pretty(&settings).expect("serialize");
        assert_eq!(toml::from_str::<TuiSettings>(&text).expect("parse"), settings);
    }

    /// The three ways a file can let the user down, and what each does.
    #[test]
    fn a_broken_or_partial_file_falls_back_without_panicking() {
        // Corrupt: not TOML at all.
        assert!(toml::from_str::<TuiSettings>("{{{ not toml").is_err());

        // Partial: an older file that predates a field still loads, and the
        // new field takes its default rather than failing the whole read.
        let older = "language = \"es\"\nprofile = \"andina_colombia\"\n";
        let parsed: TuiSettings = toml::from_str(older).expect("a partial file has to load");
        assert_eq!(parsed.language, "es");
        assert_eq!(parsed.profile, "andina_colombia");
        assert_eq!(parsed.bag_weight_kg, TuiSettings::default().bag_weight_kg);

        // Hostile: a value that would divide by zero downstream.
        assert_eq!(TuiSettings { bag_weight_kg: -40.0, ..TuiSettings::default() }.sanitized().bag_weight_kg, 50.0);
        assert_eq!(TuiSettings { bag_weight_kg: f64::NAN, ..TuiSettings::default() }.sanitized().bag_weight_kg, 50.0);
    }

    #[test]
    fn the_config_path_is_not_the_data_root() {
        let path = settings_path();
        assert!(path.ends_with("non-nobis-solum/settings.toml"), "{}", path.display());
        assert_ne!(path.parent(), Some(crate::infra::bootstrap::default_data_root().as_path()));
    }
}
