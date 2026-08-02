use super::efficiency::{AdjustedEfficiency, EfficiencyBandRules, ScenarioConditions};
use super::nutrient::Nutrient;
use super::value_objects::{
    ClimateZone, Depth, FertilizerForm, IrrigationSystem, NutrientDemandMode, SoilStatus, Texture, YieldTarget,
};

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

#[derive(Debug, Clone)]
pub struct Crop {
    pub crop_id: String,
    pub name: String,
    pub crop_type: String,
    pub family: String,
}

#[derive(Debug, Clone)]
pub struct FertilizerSource {
    pub source_id: String,
    pub name: String,
    pub composition_pct: Vec<(Nutrient, f64)>,
    pub density_kg_l: Option<f64>,
    /// The chemical form the product carries its nutrient in, where the
    /// catalog states one. Decides availability, not content — see
    /// [`FertilizerForm`]. `Unknown` for a row written before the column
    /// existed, which is every row of a catalog that has not been migrated.
    pub form: FertilizerForm,
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

/// Context of a field/lot, independent of any single soil test.
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
    /// Metres above sea level. Optional, and its own field rather than a
    /// third coordinate: it is the key Tabla 4 interprets organic matter
    /// against, and unlike latitude and longitude it is useful with no
    /// network at all. A lot without one falls back to the fetched mean
    /// temperature, and without that its organic matter stays
    /// uninterpreted.
    pub altitude_m: Option<f64>,
    /// Planted hectares. Optional, because a lot can be planned per
    /// hectare and many are: without it the report gives kg/ha and no
    /// totals, which is honest rather than a total computed from a
    /// fabricated area.
    ///
    /// Lives here and not in a preference because it is a fact about *this
    /// field*. Two lots have different areas, so one stored globally would
    /// compute lot B's totals from lot A's hectares.
    pub area_ha: Option<f64>,
}

impl FieldContext {
    /// The thermal belt to read this lot's organic matter against.
    /// Altitude first because it is the grower's own datum; the
    /// climatology is the fallback, and `None` is honest when there is
    /// neither.
    pub fn climate_zone(&self, mean_temp_c: Option<f64>) -> Option<ClimateZone> {
        self.altitude_m
            .map(ClimateZone::from_altitude_m)
            .or_else(|| mean_temp_c.map(ClimateZone::from_mean_temp_c))
    }
}

/// One curated planning row: which crop is planned on which lot, and the
/// yield goal to plan it for (`data/curated/yield_targets.csv`).
#[derive(Debug, Clone)]
pub struct LotYieldTarget {
    pub field_id: String,
    pub crop_id: String,
    pub target: YieldTarget,
}

/// A long-term (30-year) climatology reduced to the annual figures the
/// domain consumes.
///
/// Every field is `Option` on purpose — a provider may not expose a
/// variable, or may return its missing-data sentinel for a grid cell. Each
/// rule reading this must do nothing when its input is absent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnualClimatology {
    pub mean_temp_c: Option<f64>,
    /// The *hottest month's* mean daily maximum, not the annual mean of
    /// the daily maxima — the heat-stress rule asks whether any month
    /// crosses a threshold, so the reduction has to be a max, not a mean.
    pub max_temp_c: Option<f64>,
    pub precip_mm_per_day: Option<f64>,
    pub solar_mj_m2_per_day: Option<f64>,
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

#[derive(Debug, Clone)]
pub struct CriticalLevel {
    pub low_threshold: f64,
    pub medium_threshold: f64,
    pub high_threshold: f64,
    /// The unit the three thresholds are stated in. Not decoration: the
    /// literature reports P and S in mg/kg and the exchangeable cations in
    /// cmolc/kg, so the number a threshold is compared against has to be
    /// moved into this unit first. Leaving it implicit is what let a
    /// cmolc/kg reading be judged against mg/kg thresholds.
    pub unit: String,
    /// The lab extraction the thresholds were established for, or `any`
    /// where the literature gives one set for all of them.
    ///
    /// AGRONOMIC_NOTE: not decoration for P. Tabla 12 lists Bray II and
    /// Olsen side by side with different boundaries (20/40 vs 16/35 mg/kg)
    /// because the two reagents dissolve different fractions of soil P.
    /// Judging an Olsen reading against Bray II boundaries under-rates the
    /// soil and over-fertilizes; the reverse over-rates it and starves the
    /// crop.
    pub extraction_method: String,
    pub source: String,
    pub year: u16,
}

impl CriticalLevel {
    /// `value` must already be expressed in [`Self::unit`].
    ///
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

/// One named band of a qualitative interpretation table: "pH between 5.6
/// and 6.1 is moderately acid", "organic matter above 10% is high in the
/// cold belt", "a Ca:Mg between 3 and 5 is ideal".
///
/// `min_value`/`max_value` are `Option` because the outermost band of
/// every such table is open-ended, and because some tables name only part
/// of the number line: Tabla 12's base-balance block gives Ca:Mg an
/// "ideal" band of 3-5 and a "magnesium deficient" one above 10, and says
/// nothing at all about 5-10. A value landing in that gap is genuinely
/// unclassified, and must not be rounded into whichever band is nearest.
#[derive(Debug, Clone, PartialEq)]
pub struct QualitativeBand {
    pub property: String,
    pub category: String,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub unit: String,
    pub source: String,
    pub year: u16,
}

impl QualitativeBand {
    /// Half-open, `[min, max)`, so bands that share a boundary in the
    /// source table cover the line without overlapping at the seam.
    pub fn contains(&self, value: f64) -> bool {
        self.min_value.is_none_or(|min| value >= min) && self.max_value.is_none_or(|max| value < max)
    }
}

/// One soil property read against its interpretation table.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyAssessment {
    pub property: String,
    pub value: f64,
    pub unit: String,
    /// `None` when the table names no band containing this value — see
    /// [`QualitativeBand`].
    pub category: Option<String>,
    pub source: Option<String>,
}

/// The qualitative half of a soil analysis: what the numbers mean, as
/// opposed to what to apply. Nothing here feeds a dose.
#[derive(Debug, Clone, Default)]
pub struct SoilQualityAssessment {
    /// Which set of organic-matter thresholds was used, and how it was
    /// determined. `None` means the lot has neither an altitude nor a
    /// climatology, so organic matter is left uninterpreted rather than
    /// judged against an arbitrary belt.
    pub climate_zone: Option<ClimateZone>,
    pub properties: Vec<PropertyAssessment>,
    /// Ca:Mg, Mg:K, K:Mg, Ca:K and (Ca+Mg)/K, from Tabla 12's
    /// "balance de bases" block.
    pub cation_ratios: Vec<PropertyAssessment>,
}

/// Provenance of a removal coefficient: which dataset it came from, so
/// `InspectScenario` can show the user what science backs a number.
///
/// Both coefficients are `Option` because the source tables print a dash
/// wherever a study measured one basis and not the other. An absent one is
/// not a zero, and must never be filled in from the other column: that is
/// what produced the mixed rows (N/P/K on the extraction basis, Ca/Mg/S on
/// the absorption basis) this schema replaced.
#[derive(Debug, Clone)]
pub struct RemovalReference {
    pub extraction_kg_per_unit: Option<f64>,
    pub absorption_kg_per_unit: Option<f64>,
    /// What a tonne of yield actually is for this crop (grain, fruit, dry
    /// forage, green bean...). Descriptive only, never a lookup key — but
    /// the coefficient cannot be read without it: coffee is stated per
    /// tonne of green bean, not cherry.
    pub harvested_organ: String,
    /// The unit the coefficients are stated per (e.g. `t_ha`). Checked
    /// against the scenario's yield goal by the use case: a coefficient in
    /// kg per tonne applied to a goal in anything else is off by whatever
    /// the two units differ by, silently.
    pub yield_unit: String,
    pub source: String,
    pub region: String,
    pub year: u16,
    pub dataset_version: String,
}

impl RemovalReference {
    pub fn coefficient(&self, mode: NutrientDemandMode) -> Option<f64> {
        match mode {
            NutrientDemandMode::Extraction => self.extraction_kg_per_unit,
            NutrientDemandMode::Absorption => self.absorption_kg_per_unit,
        }
    }
}

/// Something the reader has to know to interpret the plan, which is not
/// severe enough to refuse one. Carried on the plan rather than printed by
/// the use case so every front-end decides how to show it.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanWarning {
    /// The extraction basis asked for no fertilizer and the absorption
    /// basis does; the dose shown is the absorption one. The distinction
    /// matters to the reader: it means the recommendation covers uptake
    /// the crop will mostly return to the soil in its residues.
    FallbackToAbsorption {
        nutrient: Nutrient,
        net_requirement_kg_ha: f64,
    },
    /// Neither column of the reference table has a coefficient for this
    /// crop and nutrient. Reported instead of failing the plan, and
    /// instead of a demand of zero, which would read as "the crop needs
    /// none" rather than "nobody measured it".
    NoRemovalCoefficient { nutrient: Nutrient },
}

#[derive(Debug, Clone)]
pub struct FertilizerDose {
    pub source_id: String,
    pub source_name: String,
    pub kg_product_per_ha: f64,
}

#[derive(Debug, Clone)]
pub struct NutrientPlanEntry {
    pub nutrient: Nutrient,
    pub availability_kg_ha: f64,
    pub demand_kg_ha: f64,
    /// Which coefficient produced `demand_kg_ha`. `None` means the
    /// reference table has neither, so the demand is unknown rather than
    /// zero — a distinction the front-end must not flatten.
    pub demand_mode_used: Option<NutrientDemandMode>,
    /// The figure the dose divided by, i.e. `efficiency.adjusted`. Kept as
    /// its own field because it is what almost every reader wants, and the
    /// trace beside it is what only the report wants.
    pub efficiency_used: f64,
    /// Where `efficiency_used` came from: the reference range's midpoint
    /// and every site condition that moved it. See
    /// [`crate::core::domain::efficiency`].
    pub efficiency: AdjustedEfficiency,
    pub net_requirement_kg_ha: f64,
    pub soil_status: Option<SoilStatus>,
    pub dose: Option<FertilizerDose>,
}

/// A micronutrient judged against its critical level, and the corrective
/// application that would bring a deficient soil up to it.
///
/// Deliberately not a [`NutrientPlanEntry`]: there is no crop demand here
/// and no `demand_kg_ha` that would mean anything. Reusing that struct
/// would have put a fabricated number in a field readers already trust to
/// mean "what the harvest takes".
#[derive(Debug, Clone)]
pub struct MicronutrientCorrection {
    pub nutrient: Nutrient,
    /// The lab reading, in [`Self::unit`] — already converted into the
    /// unit the threshold is stated in.
    pub soil_value: f64,
    pub unit: String,
    pub critical_low_threshold: f64,
    pub soil_status: SoilStatus,
    /// How much of the element the soil is short of, in kg/ha, before use
    /// efficiency. Zero for a soil at or above its critical level.
    pub deficit_kg_ha: f64,
    pub efficiency_used: f64,
    pub net_requirement_kg_ha: f64,
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
    /// The named combination this material belongs to, if any. Tabla 12
    /// prescribes one — hydrated lime, dolomite and Paz del Río slag in a
    /// 40/45/15 split of the CaCO3 requirement.
    ///
    /// AGRONOMIC_NOTE: the table calls this "mejoramiento químico
    /// integral" and the split is the point of it, not an averaging
    /// convenience. Hydrated lime reacts fast and corrects acidity within
    /// the season; dolomite is slower and carries the magnesium a
    /// straight calcitic liming would dilute; the slag adds phosphorus.
    /// Meeting the whole requirement with the single highest-PRNT
    /// material corrects the pH and leaves the imbalance.
    pub mixture_id: Option<String>,
    /// This material's share of the CaCO3-equivalent requirement, as a
    /// percent of it. The shares of one `mixture_id` sum to 100.
    pub mixture_share_pct: Option<f64>,
    pub source: String,
    pub restrictions: Vec<String>,
}

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
    /// The single highest-PRNT material that covers the whole requirement.
    pub material: Option<LimingDose>,
    /// Tabla 12's combined alternative: the same requirement split across
    /// the materials of one `mixture_id`. Offered alongside the single
    /// material rather than instead of it — the combination corrects the
    /// base balance as well as the pH, but it needs three products a
    /// grower may not be able to source.
    pub mixture: Option<LimingMixture>,
}

/// A liming requirement met by a named combination of materials.
#[derive(Debug, Clone)]
pub struct LimingMixture {
    pub mixture_id: String,
    pub components: Vec<LimingDose>,
}

/// Output of `CalculateFertilityPlan`: net nutrient requirements and
/// recommended fertilizer doses for a field/crop/yield scenario.
#[derive(Debug, Clone)]
pub struct FertilityPlan {
    pub field_id: String,
    pub sample_id: String,
    pub crop_id: String,
    pub yield_target: YieldTarget,
    /// Which basis the caller asked for. Individual entries may differ
    /// from it — see [`PlanWarning::FallbackToAbsorption`].
    pub demand_mode: NutrientDemandMode,
    pub nutrient_results: Vec<NutrientPlanEntry>,
    /// Only the micronutrients this sample was actually tested for. A
    /// micronutrient with no lab reading is absent rather than assumed
    /// adequate — untested is not the same as sufficient.
    pub micronutrients: Vec<MicronutrientCorrection>,
    pub liming: Option<LimingRecommendation>,
    /// Anything the reader needs to interpret the numbers above. Empty is
    /// the normal case; never a reason to refuse a plan.
    pub warnings: Vec<PlanWarning>,
    /// The mineralization factor actually used for N this run. Reported
    /// so the output can state whether it was climate-derived or the
    /// baseline constant: N availability is directly proportional to it,
    /// and a cold or water-limited site lands well under half the
    /// baseline while a hot humid one runs above it.
    pub mineralization_factor: f64,
    /// `None` means the plan ran without climate enrichment (no
    /// coordinates, provider unreachable, or explicitly disabled). Every
    /// climate-derived figure in the plan is baseline when this is `None`.
    pub climate: Option<AnnualClimatology>,
    /// The site conditions every `NutrientPlanEntry::efficiency` was
    /// derived from, kept once rather than per entry: they are a property
    /// of the lot, and a reader checking one modifier wants to see the
    /// reading that triggered it.
    pub conditions: ScenarioConditions,
    /// The band table this plan's efficiencies were derived from, carried
    /// so a downstream consumer can re-run one — the formulation does, to
    /// price an elemental sulfur product it only chose afterwards — without
    /// reaching for a repository the domain must not know about.
    pub band_rules: EfficiencyBandRules,
    /// The lot's planted area, straight off `field_context.csv`. Carried so
    /// a caller sizing totals does not have to re-open the context the plan
    /// already read — and `None` stays `None`, so "no area on file" never
    /// becomes a fabricated hectare.
    pub area_ha: Option<f64>,
}
