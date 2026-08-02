use super::errors::DomainError;
use std::fmt;
use std::str::FromStr;

/// A depth range within a soil profile, in centimeters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Depth {
    pub from_cm: f64,
    pub to_cm: f64,
}

/// USDA soil texture classes. Drives P/K fixation assumptions and
/// nitrogen use efficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Texture {
    Sand,
    LoamySand,
    SandyLoam,
    Loam,
    SiltLoam,
    Silt,
    SandyClayLoam,
    ClayLoam,
    SiltyClayLoam,
    SandyClay,
    SiltyClay,
    Clay,
}

impl Texture {
    /// So a front-end can offer the closed set instead of drifting from
    /// `from_str`.
    pub const ALL: [Texture; 12] = [
        Texture::Sand,
        Texture::LoamySand,
        Texture::SandyLoam,
        Texture::Loam,
        Texture::SiltLoam,
        Texture::Silt,
        Texture::SandyClayLoam,
        Texture::ClayLoam,
        Texture::SiltyClayLoam,
        Texture::SandyClay,
        Texture::SiltyClay,
        Texture::Clay,
    ];
}

impl FromStr for Texture {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "sand" => Ok(Texture::Sand),
            "loamy_sand" => Ok(Texture::LoamySand),
            "sandy_loam" => Ok(Texture::SandyLoam),
            "loam" => Ok(Texture::Loam),
            "silt_loam" => Ok(Texture::SiltLoam),
            "silt" => Ok(Texture::Silt),
            "sandy_clay_loam" => Ok(Texture::SandyClayLoam),
            "clay_loam" => Ok(Texture::ClayLoam),
            "silty_clay_loam" => Ok(Texture::SiltyClayLoam),
            "sandy_clay" => Ok(Texture::SandyClay),
            "silty_clay" => Ok(Texture::SiltyClay),
            "clay" => Ok(Texture::Clay),
            other => Err(DomainError::InvalidInput(format!("unknown texture: {other}"))),
        }
    }
}

impl fmt::Display for Texture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Texture::Sand => "sand",
            Texture::LoamySand => "loamy_sand",
            Texture::SandyLoam => "sandy_loam",
            Texture::Loam => "loam",
            Texture::SiltLoam => "silt_loam",
            Texture::Silt => "silt",
            Texture::SandyClayLoam => "sandy_clay_loam",
            Texture::ClayLoam => "clay_loam",
            Texture::SiltyClayLoam => "silty_clay_loam",
            Texture::SandyClay => "sandy_clay",
            Texture::SiltyClay => "silty_clay",
            Texture::Clay => "clay",
        };
        f.write_str(s)
    }
}

/// Drives nitrogen and potassium use efficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrrigationSystem {
    Rainfed,
    Gravity,
    Sprinkler,
    Drip,
}

impl IrrigationSystem {
    pub const ALL: [IrrigationSystem; 4] = [
        IrrigationSystem::Rainfed,
        IrrigationSystem::Gravity,
        IrrigationSystem::Sprinkler,
        IrrigationSystem::Drip,
    ];
}

impl FromStr for IrrigationSystem {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "rainfed" => Ok(IrrigationSystem::Rainfed),
            "gravity" => Ok(IrrigationSystem::Gravity),
            "sprinkler" => Ok(IrrigationSystem::Sprinkler),
            "drip" => Ok(IrrigationSystem::Drip),
            other => Err(DomainError::InvalidInput(format!("unknown irrigation system: {other}"))),
        }
    }
}

impl fmt::Display for IrrigationSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IrrigationSystem::Rainfed => "rainfed",
            IrrigationSystem::Gravity => "gravity",
            IrrigationSystem::Sprinkler => "sprinkler",
            IrrigationSystem::Drip => "drip",
        };
        f.write_str(s)
    }
}

/// The thermal belt a lot sits in, which is the axis Tabla 4 keys its
/// organic-matter categories to.
///
/// AGRONOMIC_NOTE: the same 3% organic matter is *high* in the lowlands
/// and *very low* above 2000 m. Cold slows the mineralization that
/// consumes soil organic matter, so a highland soil accumulates several
/// times more of it at equilibrium — reading one figure against the other
/// belt's thresholds inverts the diagnosis outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClimateZone {
    /// Below 1000 m, mean temperature above 24 °C.
    Warm,
    /// 1000-2000 m, 18-24 °C.
    Temperate,
    /// Above 2000 m, below 18 °C.
    Cold,
}

impl ClimateZone {
    /// Tabla 4's primary key. Preferred over temperature because the
    /// grower knows it without a network call and it does not drift.
    pub fn from_altitude_m(altitude_m: f64) -> Self {
        if altitude_m < 1000.0 {
            ClimateZone::Warm
        } else if altitude_m <= 2000.0 {
            ClimateZone::Temperate
        } else {
            ClimateZone::Cold
        }
    }

    /// Tabla 4's second key, given as equivalent to the first. Used when a
    /// lot has no altitude on file but a climatology was fetched.
    pub fn from_mean_temp_c(mean_temp_c: f64) -> Self {
        if mean_temp_c > 24.0 {
            ClimateZone::Warm
        } else if mean_temp_c >= 18.0 {
            ClimateZone::Temperate
        } else {
            ClimateZone::Cold
        }
    }
}

impl fmt::Display for ClimateZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ClimateZone::Warm => "warm",
            ClimateZone::Temperate => "temperate",
            ClimateZone::Cold => "cold",
        })
    }
}

/// A yield goal used to scale crop nutrient removal, e.g. 10.0 t_ha.
#[derive(Debug, Clone, PartialEq)]
pub struct YieldTarget {
    pub value: f64,
    pub unit: String,
}

/// The chemical or commercial form a fertilizer carries its nutrient in.
///
/// AGRONOMIC_NOTE: form decides *when* a nutrient becomes available, which
/// is a different question from how much of it a product contains. Two
/// products at 24% S are not interchangeable if one is sulfate — taken up
/// on contact — and the other elemental, which soil bacteria have to
/// oxidize first over weeks to months. The same distinction runs through
/// nitrogen (nitrate is immediate and leachable, amide has to hydrolyse
/// and can volatilize) and through the micronutrients (a chelate survives
/// a calcareous soil that precipitates an inorganic salt).
///
/// This replaced reading the product's *name* for the word "elemental",
/// which worked only for the shipped Spanish catalog and only for sulfur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FertilizerForm {
    /// SO4(2-): immediately plant-available.
    Sulfate,
    /// S(0), Fe(0)...: needs microbial oxidation before uptake.
    Elemental,
    /// NO3(-): immediately available, and the most leachable N form.
    Nitrate,
    /// NH4(+): held on the exchange, volatile above neutral pH.
    Ammonium,
    /// CO(NH2)2: hydrolyses to ammonium first, so it carries urea's
    /// volatilization risk.
    Amide,
    Chloride,
    Oxide,
    Carbonate,
    /// An organic ligand keeps the cation in solution where an inorganic
    /// salt would precipitate.
    Chelate,
    /// Genuinely more than one form in one product — a blend of urea and
    /// ammonium sulfate, say. Distinct from `Unknown`: somebody looked.
    Mixed,
    /// Nobody has stated it. The default for a catalog row that predates
    /// the column, and the only value that means "do not reason about
    /// form here".
    #[default]
    Unknown,
}

impl FertilizerForm {
    pub const ALL: [FertilizerForm; 11] = [
        FertilizerForm::Sulfate,
        FertilizerForm::Elemental,
        FertilizerForm::Nitrate,
        FertilizerForm::Ammonium,
        FertilizerForm::Amide,
        FertilizerForm::Chloride,
        FertilizerForm::Oxide,
        FertilizerForm::Carbonate,
        FertilizerForm::Chelate,
        FertilizerForm::Mixed,
        FertilizerForm::Unknown,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FertilizerForm::Sulfate => "sulfate",
            FertilizerForm::Elemental => "elemental",
            FertilizerForm::Nitrate => "nitrate",
            FertilizerForm::Ammonium => "ammonium",
            FertilizerForm::Amide => "amide",
            FertilizerForm::Chloride => "chloride",
            FertilizerForm::Oxide => "oxide",
            FertilizerForm::Carbonate => "carbonate",
            FertilizerForm::Chelate => "chelate",
            FertilizerForm::Mixed => "mixed",
            FertilizerForm::Unknown => "unknown",
        }
    }

    /// Whether this form has to be transformed in the soil before the crop
    /// can take the nutrient up at all.
    ///
    /// `Unknown` answers `false`: the safe reading of missing metadata is
    /// the common case (a soluble salt), and the efficiency path reports
    /// the assumption rather than quietly penalising every row of an
    /// un-migrated catalog.
    pub fn needs_soil_transformation(self) -> bool {
        matches!(self, FertilizerForm::Elemental)
    }
}

impl FromStr for FertilizerForm {
    type Err = DomainError;

    /// An empty cell is `Unknown` — that is the migration path for a
    /// catalog written before the column existed. A cell with something
    /// unrecognised in it is an error, not an `Unknown`: a typo has to be
    /// visible, or the column is decoration.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let text = s.trim().to_lowercase();
        if text.is_empty() {
            return Ok(FertilizerForm::Unknown);
        }
        FertilizerForm::ALL
            .into_iter()
            .find(|form| form.as_str() == text)
            .ok_or_else(|| DomainError::InvalidInput(format!("unknown fertilizer form: {s}")))
    }
}

impl fmt::Display for FertilizerForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Qualitative interpretation of a soil test value against critical levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoilStatus {
    Low,
    Medium,
    High,
}

/// Which of the two coefficients the source tables give per crop is used
/// to size the crop's demand.
///
/// AGRONOMIC_NOTE: these answer different questions and neither is a
/// refinement of the other. *Extraction* is what physically leaves the
/// field in the harvested organ, so it sizes a maintenance plan — replace
/// what the harvest took. *Absorption* is total uptake by the whole plant
/// over the cycle, most of which returns to the soil in residues; it is
/// the right basis when the residue does not stay (burned, baled, grazed
/// off) or when the goal is to build fertility rather than hold it.
/// Absorption is always the larger of the two, so planning on it when the
/// residue does stay over-fertilizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NutrientDemandMode {
    Extraction,
    Absorption,
}

impl NutrientDemandMode {
    pub const ALL: [NutrientDemandMode; 2] = [NutrientDemandMode::Extraction, NutrientDemandMode::Absorption];
}

impl FromStr for NutrientDemandMode {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "extraction" => Ok(NutrientDemandMode::Extraction),
            "absorption" => Ok(NutrientDemandMode::Absorption),
            other => Err(DomainError::InvalidInput(format!("unknown demand mode: {other}"))),
        }
    }
}

impl fmt::Display for NutrientDemandMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NutrientDemandMode::Extraction => "extraction",
            NutrientDemandMode::Absorption => "absorption",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A variant whose `Display` doesn't parse back would be offered by the
    /// TUI and then refused on save.
    #[test]
    fn every_listed_texture_and_irrigation_system_parses_back_from_its_own_text() {
        for texture in Texture::ALL {
            assert_eq!(Texture::from_str(&texture.to_string()).expect("texture"), texture);
        }
        for irrigation in IrrigationSystem::ALL {
            assert_eq!(IrrigationSystem::from_str(&irrigation.to_string()).expect("irrigation"), irrigation);
        }
        for mode in NutrientDemandMode::ALL {
            assert_eq!(NutrientDemandMode::from_str(&mode.to_string()).expect("demand mode"), mode);
        }
        for form in FertilizerForm::ALL {
            assert_eq!(FertilizerForm::from_str(&form.to_string()).expect("form"), form);
        }
    }

    /// The migration contract: a catalog written before the column exists
    /// reads as `Unknown`, and a typo is refused rather than silently
    /// becoming one.
    #[test]
    fn a_missing_form_is_unknown_and_a_misspelt_one_is_an_error() {
        assert_eq!(FertilizerForm::from_str("").expect("empty"), FertilizerForm::Unknown);
        assert_eq!(FertilizerForm::from_str("   ").expect("blank"), FertilizerForm::Unknown);
        assert_eq!(FertilizerForm::from_str("  SULFATE ").expect("case"), FertilizerForm::Sulfate);
        assert!(FertilizerForm::from_str("sulphate").is_err(), "a near miss must not pass as Unknown");
        assert!(FertilizerForm::from_str("elementall").is_err());

        assert!(FertilizerForm::Elemental.needs_soil_transformation());
        assert!(!FertilizerForm::Sulfate.needs_soil_transformation());
        // Missing metadata is read as the common case, and reported.
        assert!(!FertilizerForm::Unknown.needs_soil_transformation());
    }
}
