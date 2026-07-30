use super::nutrient::Nutrient;
use super::value_objects::{Depth, DemandType, IrrigationSystem, SoilStatus, Texture, YieldTarget};

/// A physical soil sample taken in the field. Identifies where and when
/// the laboratory analysis in [`SoilTest`] came from.
#[derive(Debug, Clone)]
pub struct SoilSample {
    pub sample_id: String,
    pub laboratory: String,
    /// ISO-8601 date (YYYY-MM-DD), kept as text: the domain never parses
    /// or arithmetics dates, it only displays them.
    pub sampled_on: String,
    pub depth: Depth,
}

/// One analytical result for a single nutrient from a lab report.
#[derive(Debug, Clone)]
pub struct SoilTest {
    pub sample_id: String,
    pub nutrient: Nutrient,
    pub value: f64,
    pub unit: String,
    pub method: String,
    pub layer: Depth,
}

/// A crop from the reference catalog.
#[derive(Debug, Clone)]
pub struct Crop {
    pub crop_id: String,
    pub name: String,
    pub crop_type: String,
    pub family: String,
}

/// Per-nutrient demand coefficient for a crop, as loaded from the
/// reference removal tables (see `NutrientRemovalRepository`).
#[derive(Debug, Clone)]
pub struct NutrientDemand {
    pub demand_type: DemandType,
    pub nutrient: Nutrient,
    pub base_coefficient_kg_per_yield_unit: f64,
}

/// A commercial fertilizer product: nutrient composition by percent
/// weight, physical density and any usage restrictions.
#[derive(Debug, Clone)]
pub struct FertilizerSource {
    pub source_id: String,
    pub name: String,
    pub composition_pct: Vec<(Nutrient, f64)>,
    pub density_kg_l: Option<f64>,
    pub restrictions: Vec<String>,
}

impl FertilizerSource {
    pub fn pct_of(&self, nutrient: Nutrient) -> Option<f64> {
        self.composition_pct
            .iter()
            .find(|(n, _)| *n == nutrient)
            .map(|(_, pct)| *pct)
    }
}

/// Physical and chemical context of a field/lot, independent of any
/// single soil test: texture, irrigation, bulk density, etc.
#[derive(Debug, Clone)]
pub struct FieldContext {
    pub field_id: String,
    pub sample_id: String,
    pub texture: Texture,
    pub irrigation_system: IrrigationSystem,
    pub organic_matter_percent: f64,
    pub ph: f64,
    pub cec_cmolc_kg: f64,
    pub bulk_density_kg_dm3: f64,
    pub arable_depth_m: f64,
    pub region: String,
    /// Decimal degrees, WGS84. Optional: a lot with no coordinates simply
    /// gets no climate enrichment, exactly as if the API were down.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// A long-term (30-year) climatology reduced to the annual figures the
/// domain actually consumes, as returned by an `AgroclimaticRepository`.
///
/// Every field is `Option` on purpose: a provider may not expose a
/// variable at all, or may return its missing-data sentinel for a given
/// grid cell. Each rule that reads this struct is responsible for doing
/// nothing when its input is absent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnualClimatology {
    pub mean_temp_c: Option<f64>,
    /// The *hottest month's* mean daily maximum, not the annual mean of
    /// the daily maxima — the heat-stress rule asks whether any month
    /// crosses a threshold, so the reduction has to be a max, not a mean.
    pub max_temp_c: Option<f64>,
    /// The coldest month's mean daily minimum, by the same logic.
    pub min_temp_c: Option<f64>,
    pub precip_mm_per_day: Option<f64>,
    pub solar_mj_m2_per_day: Option<f64>,
    pub humidity_pct: Option<f64>,
    pub wind_ms: Option<f64>,
    /// Reference evapotranspiration. See `NasaPowerRepo` for why this is
    /// derived rather than fetched.
    pub et0_mm_per_day: Option<f64>,
}

impl AnnualClimatology {
    pub fn annual_precip_mm(&self) -> Option<f64> {
        self.precip_mm_per_day.map(|v| v * 365.0)
    }

    pub fn annual_et0_mm(&self) -> Option<f64> {
        self.et0_mm_per_day.map(|v| v * 365.0)
    }
}

/// Thresholds used to classify a soil test value as low/medium/high.
#[derive(Debug, Clone)]
pub struct CriticalLevel {
    pub low_threshold: f64,
    pub medium_threshold: f64,
    pub high_threshold: f64,
    pub source: String,
    pub year: u16,
}

impl CriticalLevel {
    /// `high_threshold` marks an excess/toxicity ceiling and is kept for
    /// reporting; the low/medium/high split itself only needs the first
    /// two boundaries.
    pub fn classify(&self, value: f64) -> SoilStatus {
        if value < self.low_threshold {
            SoilStatus::Low
        } else if value < self.medium_threshold {
            SoilStatus::Medium
        } else {
            SoilStatus::High
        }
    }
}

/// Provenance of a removal coefficient: which dataset it came from, so
/// `InspectScenario` can show the user what science backs a number.
#[derive(Debug, Clone)]
pub struct RemovalReference {
    pub removal_kg_per_unit: f64,
    pub source: String,
    pub region: String,
    pub year: u16,
    pub dataset_version: String,
}

/// One fertilizer product dose recommended to cover a nutrient's net
/// requirement.
#[derive(Debug, Clone)]
pub struct FertilizerDose {
    pub source_id: String,
    pub source_name: String,
    pub kg_product_per_ha: f64,
}

/// Full result for a single nutrient within a [`FertilityPlan`].
#[derive(Debug, Clone)]
pub struct NutrientPlanEntry {
    pub nutrient: Nutrient,
    pub availability_kg_ha: f64,
    pub demand_kg_ha: f64,
    pub efficiency_used: f64,
    pub net_requirement_kg_ha: f64,
    pub soil_status: Option<SoilStatus>,
    pub dose: Option<FertilizerDose>,
}

/// A liming material: neutralizing value comes from its CaO/MgO content,
/// not from elemental Ca/Mg — kept separate from [`FertilizerSource`]
/// because mixing the two catalogs would misuse elemental-nutrient
/// percentages as neutralizing capacity.
#[derive(Debug, Clone)]
pub struct LimingMaterial {
    pub source_id: String,
    pub name: String,
    pub cao_pct: f64,
    pub mgo_pct: f64,
    /// Fraction of the material fine enough to actually react in-field
    /// (granulometric efficiency, "EG"), 0-100.
    pub granulometric_efficiency_pct: f64,
    pub restrictions: Vec<String>,
}

/// A liming material dose recommended to cover a [`LimingRecommendation`].
#[derive(Debug, Clone)]
pub struct LimingDose {
    pub source_id: String,
    pub source_name: String,
    pub t_product_per_ha: f64,
}

/// Lime requirement for a field, computed only when an Al³⁺ soil test
/// exists for the sample (the workflow's "encalamiento si aplica").
#[derive(Debug, Clone)]
pub struct LimingRecommendation {
    /// CaCO3-equivalent requirement from exchangeable Al³⁺ toxicity.
    pub al_based_t_ha: f64,
    /// CaCO3-equivalent requirement from raising base saturation to target.
    pub base_saturation_based_t_ha: f64,
    /// The larger of the two — the conservative pick (see `ponytail:` note
    /// at the call site for the real-world caveat this simplifies away).
    pub recommended_t_ha: f64,
    pub current_base_saturation_pct: f64,
    pub target_base_saturation_pct: f64,
    pub material: Option<LimingDose>,
}

/// Output of `CalculateFertilityPlan`: net nutrient requirements and
/// recommended fertilizer doses for a field/crop/yield scenario.
#[derive(Debug, Clone)]
pub struct FertilityPlan {
    pub field_id: String,
    pub sample_id: String,
    pub crop_id: String,
    pub yield_target: YieldTarget,
    pub nutrient_results: Vec<NutrientPlanEntry>,
    pub liming: Option<LimingRecommendation>,
    /// The mineralization factor actually used for N this run. Reported
    /// so the output can state whether it was climate-derived or the
    /// baseline constant — the two differ by up to 3x.
    pub mineralization_factor: f64,
    /// `None` means the plan ran without climate enrichment (no
    /// coordinates, provider unreachable, or explicitly disabled). Every
    /// climate-derived figure in the plan is baseline when this is `None`.
    pub climate: Option<AnnualClimatology>,
}
