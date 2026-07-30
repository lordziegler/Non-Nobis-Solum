use super::errors::DomainError;
use std::fmt;
use std::str::FromStr;

/// A depth range within a soil profile, in centimeters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Depth {
    pub from_cm: f64,
    pub to_cm: f64,
}

/// USDA soil texture classes. Drives phosphorus/potassium fixation
/// assumptions and nitrogen use efficiency in the domain services.
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
    /// Every class, so a front-end can offer the closed set instead of
    /// re-typing it and drifting from `from_str`.
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

/// Water source for the field. Drives nitrogen and potassium use efficiency.
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

/// A yield goal used to scale crop nutrient removal, e.g. 10.0 t_ha.
#[derive(Debug, Clone, PartialEq)]
pub struct YieldTarget {
    pub value: f64,
    pub unit: String,
}

/// Whether reference coefficients represent total plant absorption
/// (uptake by the whole plant) or harvest extraction (removed with the
/// harvested product only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DemandType {
    Absorption,
    Extraction,
}

impl FromStr for DemandType {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "absorption" => Ok(DemandType::Absorption),
            "extraction" => Ok(DemandType::Extraction),
            other => Err(DomainError::InvalidInput(format!("unknown demand type: {other}"))),
        }
    }
}

/// Nutrient available in the soil, already normalized to kg/ha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Availability {
    pub kg_ha: f64,
}

/// Qualitative interpretation of a soil test value against critical levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoilStatus {
    Low,
    Medium,
    High,
}
