//! Reads `data/reference/<profile>/efficiency_bands.toml` — the modifiers
//! that move a nutrient's base efficiency to the conditions of a real site.
//!
//! No sentinel `region` column here, unlike `liming_rules.toml` and
//! `critical_levels.csv`. Those need one because a lot's declared region and
//! the active `--profile` are independent knobs that can disagree; every row
//! here is keyed on a *measured* site condition instead, and the file
//! already sits inside a profile directory. Per-profile bands are what the
//! directory gives for free.

use std::path::Path;
use std::str::FromStr;

use serde::Deserialize;

use crate::core::domain::{BandGroup, DomainError, EfficiencyBandRule, EfficiencyBandRules, Nutrient};
use crate::core::ports::EfficiencyBandRepository;

#[derive(Debug, Deserialize)]
struct BandRow {
    group: String,
    nutrient: String,
    class: Option<String>,
    min: Option<f64>,
    max: Option<f64>,
    factor: f64,
    effect: String,
    basis: String,
}

#[derive(Debug, Deserialize)]
struct FloorRow {
    nutrient: String,
    efficiency_floor: f64,
    // Parsed and never read. Keeping the field is what makes serde
    // *require* it: a floor row without its citation fails to load
    // instead of planning off an untraceable figure. See
    // `data/reference/README.md`.
    #[allow(dead_code)]
    source: String,
}

#[derive(Debug, Deserialize)]
struct EfficiencyBandsFile {
    acid_ph_already_priced: f64,
    #[serde(default)]
    band: Vec<BandRow>,
    #[serde(default)]
    floor: Vec<FloorRow>,
}

/// The profile's efficiency band table, loaded and validated once.
///
/// Held in memory rather than re-read because every nutrient of every plan
/// consults it, and because the validation it passes at load is what lets
/// the domain treat a band factor as already sane.
pub struct TomlEfficiencyBandsRepo {
    rules: EfficiencyBandRules,
}

impl TomlEfficiencyBandsRepo {
    /// Loads and *checks* the profile's band table at construction.
    ///
    /// # Errors
    /// `DataSource` when the file cannot be read, is not the expected TOML
    /// shape, or states a band whose factor is outside the range a modifier
    /// may take. The last one is refused here rather than allowed to
    /// produce an impossible efficiency in a plan later.
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;
        let file: EfficiencyBandsFile =
            toml::from_str(&text).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;

        let mut bands = Vec::with_capacity(file.band.len());
        for row in file.band {
            // A factor outside (0, 1] is refused at load rather than
            // multiplied into a dose: a zero drives the requirement to
            // infinity, and anything above 1 is a *bonus*, which no rule in
            // this project is allowed to be by accident.
            if !(row.factor > 0.0 && row.factor <= 1.0) {
                return Err(DomainError::DataSource(format!(
                    "{}: band {} / {} has factor {}, which must be in (0, 1]",
                    path.display(),
                    row.group,
                    row.nutrient,
                    row.factor
                )));
            }
            if let (Some(min), Some(max)) = (row.min, row.max) {
                if min >= max {
                    return Err(DomainError::DataSource(format!(
                        "{}: band {} / {} has min {min} >= max {max}",
                        path.display(),
                        row.group,
                        row.nutrient
                    )));
                }
            }
            bands.push(EfficiencyBandRule {
                group: BandGroup::from_str(&row.group)?,
                nutrient: Nutrient::from_str(&row.nutrient)?,
                class: row.class,
                min: row.min,
                max: row.max,
                factor: row.factor,
                effect: row.effect,
                basis: row.basis,
            });
        }

        let mut floors = Vec::with_capacity(file.floor.len());
        for row in file.floor {
            floors.push((Nutrient::from_str(&row.nutrient)?, row.efficiency_floor));
        }
        if floors.is_empty() {
            return Err(DomainError::DataSource(format!(
                "{}: no [[floor]] rows, so a stack of penalties would have nothing to stop it",
                path.display()
            )));
        }

        Ok(Self { rules: EfficiencyBandRules { bands, floors, acid_ph_already_priced: file.acid_ph_already_priced } })
    }
}

impl EfficiencyBandRepository for TomlEfficiencyBandsRepo {
    fn band_rules(&self) -> Result<EfficiencyBandRules, DomainError> {
        Ok(self.rules.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::Texture;

    fn shipped(profile: &str) -> EfficiencyBandRules {
        TomlEfficiencyBandsRepo::from_toml_file(format!("data/reference/{profile}/efficiency_bands.toml"))
            .expect("the shipped table has to load")
            .band_rules()
            .expect("rules")
    }

    /// Both profiles ship a table, and both parse into rules the domain can
    /// use. This is the test that fails when a band row is edited into
    /// something the enum does not know.
    #[test]
    fn both_profiles_ship_a_loadable_band_table() {
        for profile in ["global", "andina_colombia"] {
            let rules = shipped(profile);
            assert!(!rules.bands.is_empty(), "{profile} has no bands");
            assert!(rules.acid_ph_already_priced > 0.0, "{profile} has no double-count threshold");
            for nutrient in [Nutrient::N, Nutrient::P, Nutrient::K, Nutrient::S, Nutrient::Ca, Nutrient::Mg] {
                assert!(rules.floor(nutrient) > 0.0, "{profile} has no floor for {nutrient}");
            }
        }
    }

    /// A profile may differ from `global`, and the loader must carry the
    /// difference rather than the default.
    #[test]
    fn a_profile_can_state_its_own_thresholds() {
        let global = shipped("global");
        let andina = shipped("andina_colombia");

        // The Andean table lowers the phosphorus floor: its soils are
        // volcanic-ash derived and fix P far harder than the global figure
        // assumes. If that ever stops being true this test is the alarm.
        assert!(
            andina.floor(Nutrient::P) < global.floor(Nutrient::P),
            "andina P floor {} vs global {}",
            andina.floor(Nutrient::P),
            global.floor(Nutrient::P)
        );
    }

    #[test]
    fn a_band_with_an_impossible_factor_is_refused_at_load() {
        let dir = std::env::temp_dir().join(format!("nns_bands_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("bad.toml");

        let write = |body: &str| std::fs::write(&path, body).expect("write");
        let load = || TomlEfficiencyBandsRepo::from_toml_file(&path);

        let header = "acid_ph_already_priced = 5.5\n\n[[floor]]\nnutrient = \"N\"\nefficiency_floor = 0.15\nsource = \"x\"\n\n";
        let band = |factor: &str, min: &str, max: &str| {
            format!(
                "{header}[[band]]\ngroup = \"ph\"\nnutrient = \"N\"\n{min}{max}factor = {factor}\neffect = \"e\"\nbasis = \"b\"\n"
            )
        };

        write(&band("0.9", "", ""));
        assert!(load().is_ok(), "a well-formed row has to load");
        write(&band("0.0", "", ""));
        assert!(load().is_err(), "a zero factor would divide the dose by zero");
        write(&band("1.4", "", ""));
        assert!(load().is_err(), "a bonus must be deliberate, not a typo");
        write(&band("0.9", "min = 8.0\n", "max = 5.0\n"));
        assert!(load().is_err(), "an inverted interval can never fire");
        write("acid_ph_already_priced = 5.5\n");
        assert!(load().is_err(), "no floors means nothing bounds a stack of penalties");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The classification the domain does against a loaded table, rather
    /// than against a constant in a match arm.
    #[test]
    fn the_shipped_table_classifies_the_conditions_it_was_written_for() {
        assert_eq!(texture_class_of(Texture::Sand), "coarse");
        assert_eq!(texture_class_of(Texture::Loam), "medium");
        assert_eq!(texture_class_of(Texture::Clay), "fine");
    }

    fn texture_class_of(texture: Texture) -> &'static str {
        crate::core::domain::efficiency::texture_class(texture)
    }
}
