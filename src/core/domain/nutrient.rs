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
}

impl Nutrient {
    pub const MACRONUTRIENTS: [Nutrient; 6] =
        [Nutrient::N, Nutrient::P, Nutrient::K, Nutrient::S, Nutrient::Ca, Nutrient::Mg];

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
            other => Err(DomainError::InvalidInput(format!("unknown nutrient id: {other}"))),
        }
    }
}

impl fmt::Display for Nutrient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
