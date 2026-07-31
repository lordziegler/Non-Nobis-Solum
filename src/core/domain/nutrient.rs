use super::errors::DomainError;
use std::fmt;
use std::str::FromStr;

/// Plant nutrients tracked by the fertility engine. Using an enum instead
/// of raw strings keeps crop demand tables, soil tests and fertilizer
/// composition all keyed by the same closed set of values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Nutrient {
    N,
    P,
    K,
    S,
    Ca,
    Mg,
    Fe,
    Mn,
    Zn,
    Cu,
    B,
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
    pub const MACRONUTRIENTS: [Nutrient; 6] =
        [Nutrient::N, Nutrient::P, Nutrient::K, Nutrient::S, Nutrient::Ca, Nutrient::Mg];

    /// Every variant, so a front-end can offer the closed set instead of
    /// asking someone to type an id `from_str` might reject. Same role as
    /// `Texture::ALL` and `IrrigationSystem::ALL`.
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

    /// A front-end offers `ALL` as the only way to name a nutrient, so a
    /// variant whose text form doesn't parse back would be unpickable.
    #[test]
    fn every_listed_nutrient_parses_back_from_its_own_text() {
        for nutrient in Nutrient::ALL {
            assert_eq!(Nutrient::from_str(nutrient.as_str()).ok(), Some(nutrient));
        }
        assert!(Nutrient::MACRONUTRIENTS.iter().all(|n| Nutrient::ALL.contains(n)));
    }
}
