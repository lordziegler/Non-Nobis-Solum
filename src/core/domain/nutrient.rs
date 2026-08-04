//! The closed set of nutrients, and the groupings the tables are written
//! in.
//!
//! One enum rather than strings so that a soil reading, a removal
//! coefficient and a fertilizer's composition cannot disagree about what
//! they are describing. Parsing is `FromStr`, so an unknown nutrient is
//! refused at the edge rather than carried inward as a typo.

use super::errors::DomainError;
use std::fmt;
use std::str::FromStr;

/// An enum rather than raw strings, so demand tables, soil tests and
/// fertilizer composition are all keyed by the same closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Nutrient {
    /// Nitrogen. The only macronutrient whose soil supply is *mineralized*
    /// out of organic matter rather than read directly off the exchange.
    N,
    /// Phosphorus. Reported and thresholded on the extractant used, and
    /// fixed by both acid and calcareous soils.
    P,
    /// Potassium. Exchangeable, and the mobile cation a coarse soil loses.
    K,
    /// Sulfur. The one macronutrient whose availability turns on the form
    /// it is applied in — sulfate is immediate, elemental is not.
    S,
    /// Calcium. A nutrient and a base: it appears in the balance and again
    /// in the base saturation a liming target is set against.
    Ca,
    /// Magnesium. Also a base, and the one a purely calcitic liming
    /// dilutes.
    Mg,
    /// Iron. Micronutrient; precipitates in calcareous soils, where a
    /// chelate is the only form that survives.
    Fe,
    /// Manganese. Micronutrient; availability falls sharply as pH rises.
    Mn,
    /// Zinc. Micronutrient, and the most commonly deficient one in the
    /// source literature's region.
    Zn,
    /// Copper. Micronutrient; held tightly by organic matter.
    Cu,
    /// Boron. Micronutrient with the narrowest margin between deficiency
    /// and toxicity of any element here.
    B,
    /// Molybdenum. Micronutrient needed in the smallest amount, and the
    /// only one more available as pH rises.
    Mo,
    /// Exchangeable aluminum (Al³⁺), cmolc/kg. Not a plant nutrient — a
    /// soil-acidity indicator, reused via the same soil-test pipeline as
    /// Ca/Mg/K because it's reported by the same lab panel in the same
    /// unit. Feeds `services::` liming calculations only; never appears
    /// in `MACRONUTRIENTS`.
    Al,
    /// Exchangeable hydrogen (H⁺), cmolc/kg. Same rationale as `Al`;
    /// optional in practice (many labs report Al alone), so callers treat
    /// a missing `H` test as 0.
    H,
}

impl Nutrient {
    /// The six planned by balance: demand at the yield goal, less what the
    /// soil supplies, divided by an efficiency. Excludes `Al` and `H`,
    /// which are acidity indicators rather than nutrients.
    pub const MACRONUTRIENTS: [Nutrient; 6] =
        [Nutrient::N, Nutrient::P, Nutrient::K, Nutrient::S, Nutrient::Ca, Nutrient::Mg];

    /// Planned on a different basis from the macronutrients, not as a
    /// lesser version of them.
    ///
    /// `AGRONOMIC_NOTE`: the source removal tables (Tabla 10/11) report no
    /// micronutrient coefficient for any crop, so there is no "what the
    /// harvest takes" figure to replace — and there would be little point
    /// if there were, since a crop's micronutrient offtake is grams per
    /// hectare against a soil reserve of kilograms. What matters is
    /// whether the soil holds enough for uptake to happen at all, so these
    /// are corrected against their critical level rather than balanced
    /// against a removal. See `CalculateFertilityPlan::correct_micronutrients`.
    pub const MICRONUTRIENTS: [Nutrient; 6] =
        [Nutrient::Fe, Nutrient::Mn, Nutrient::Zn, Nutrient::Cu, Nutrient::B, Nutrient::Mo];

    /// So a front-end can offer the closed set instead of asking for an id
    /// `from_str` might reject. Same role as `Texture::ALL`.
    pub const ALL: [Nutrient; 14] = [
        Nutrient::N,
        Nutrient::P,
        Nutrient::K,
        Nutrient::S,
        Nutrient::Ca,
        Nutrient::Mg,
        Nutrient::Fe,
        Nutrient::Mn,
        Nutrient::Zn,
        Nutrient::Cu,
        Nutrient::B,
        Nutrient::Mo,
        Nutrient::Al,
        Nutrient::H,
    ];

    /// The nutrient's chemical symbol — the identifier every reference
    /// table and CSV column keys on.
    ///
    /// # Returns
    /// A static symbol (`"N"`, `"P2O5"` is *not* one of these — this is
    /// always the element), which `from_str` accepts back unchanged.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Nutrient::N => "N",
            Nutrient::P => "P",
            Nutrient::K => "K",
            Nutrient::S => "S",
            Nutrient::Ca => "Ca",
            Nutrient::Mg => "Mg",
            Nutrient::Fe => "Fe",
            Nutrient::Mn => "Mn",
            Nutrient::Zn => "Zn",
            Nutrient::Cu => "Cu",
            Nutrient::B => "B",
            Nutrient::Mo => "Mo",
            Nutrient::Al => "Al",
            Nutrient::H => "H",
        }
    }
}

impl FromStr for Nutrient {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "N" => Ok(Nutrient::N),
            "P" => Ok(Nutrient::P),
            "K" => Ok(Nutrient::K),
            "S" => Ok(Nutrient::S),
            "Ca" => Ok(Nutrient::Ca),
            "Mg" => Ok(Nutrient::Mg),
            "Fe" => Ok(Nutrient::Fe),
            "Mn" => Ok(Nutrient::Mn),
            "Zn" => Ok(Nutrient::Zn),
            "Cu" => Ok(Nutrient::Cu),
            "B" => Ok(Nutrient::B),
            "Mo" => Ok(Nutrient::Mo),
            "Al" => Ok(Nutrient::Al),
            "H" => Ok(Nutrient::H),
            other => Err(DomainError::InvalidInput(format!("unknown nutrient id: {other}"))),
        }
    }
}

impl fmt::Display for Nutrient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A variant whose text form doesn't parse back would be unpickable.
    #[test]
    fn every_listed_nutrient_parses_back_from_its_own_text() {
        for nutrient in Nutrient::ALL {
            assert_eq!(Nutrient::from_str(nutrient.as_str()).ok(), Some(nutrient));
        }
        assert!(Nutrient::MACRONUTRIENTS.iter().all(|n| Nutrient::ALL.contains(n)));
    }
}
