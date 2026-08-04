//! The data model: the things a plan is written about and the things it
//! produces.
//!
//! Roughly three groups. What the user curates (`SoilTest`, `FieldContext`,
//! `LotYieldTarget`), what the reference tables state (`CriticalLevel`,
//! `RemovalReference`, `LimingMaterial`, `QualitativeBand`), and what a
//! calculation hands back (`FertilityPlan` and the entries, doses and
//! warnings it carries).
//!
//! Behaviour lives here only where it is a property of the datum itself —
//! `CriticalLevel::classify`, `QualitativeBand::contains`. Anything that
//! combines two of these belongs in `super::services`.

use super::efficiency::{AdjustedEfficiency, EfficiencyBandRules, ScenarioConditions};
use super::nutrient::Nutrient;
use super::value_objects::{
    ClimateZone, Depth, FertilizerForm, IrrigationSystem, NutrientDemandMode, SoilStatus, Texture, YieldTarget,
};

/// One analytical result for a single nutrient from a lab report.
#[derive(Debug, Clone)]
pub struct SoilTest {
    /// The lab report this reading belongs to. A lot's analyses are keyed
    /// on it, so re-sampling a field is a new `sample_id`, not an edit.
    pub sample_id: String,
    /// Which element was measured.
    pub nutrient: Nutrient,
    /// The measured figure, in [`Self::unit`] — never assume it is the unit
    /// a consumer wants, convert it.
    pub value: f64,
    /// The unit the lab reported in. Carried rather than normalised at the
    /// edge so the original report stays reconstructible; conversion is the
    /// use case's job.
    pub unit: String,
    /// The lab extraction that produced `value`. A lookup axis, not
    /// metadata: P thresholds genuinely differ between Bray II and Olsen.
    pub method: String,
    /// The soil layer sampled. Two readings of one nutrient at different
    /// depths are different facts, not a duplicate.
    pub layer: Depth,
}

/// One row of the crop catalog: the crops a plan can be written for.
#[derive(Debug, Clone)]
pub struct Crop {
    /// The key every other reference table joins on — the removal
    /// coefficients and the curated planning rows both name a crop by this.
    pub crop_id: String,
    /// The name shown to a grower, in the catalog's own language.
    pub name: String,
    /// Broad grouping (cereal, fruit, forage...). Descriptive; nothing in a
    /// plan branches on it.
    pub crop_type: String,
    /// Botanical family. Descriptive here, but the axis a rotation or a
    /// nitrogen-fixation rule would key on.
    pub family: String,
}

/// One purchasable product, as the catalog states it.
#[derive(Debug, Clone)]
pub struct FertilizerSource {
    /// Catalog key. Stable across runs, which is what lets a chosen product
    /// be traced back from a report to the row it came from.
    pub source_id: String,
    /// The product's commercial name.
    pub name: String,
    /// What fraction of the bag is each nutrient, as percent by mass, on
    /// the basis the catalog prints — which for P and K is the oxide, not
    /// the element. A nutrient the product does not carry is absent rather
    /// than zero.
    pub composition_pct: Vec<(Nutrient, f64)>,
    /// Only for liquids, and only where the catalog states it. Needed to
    /// turn a mass dose into the volume a grower actually measures out.
    pub density_kg_l: Option<f64>,
    /// The chemical form the product carries its nutrient in, where the
    /// catalog states one. Decides availability, not content — see
    /// [`FertilizerForm`]. `Unknown` for a row written before the column
    /// existed, which is every row of a catalog that has not been migrated.
    pub form: FertilizerForm,
    /// Free-text limits on buying or applying this product (regional
    /// commercialization, registration status). Not parsed: the formulation
    /// only counts them, to prefer a product that carries fewer.
    pub restrictions: Vec<String>,
}

impl FertilizerSource {
    /// This product's percentage of one nutrient.
    ///
    /// # Arguments
    /// * `nutrient` — the element to look up, on the basis the catalog
    ///   states its composition in.
    ///
    /// # Returns
    /// `Some(percent by mass)`, or `None` when the product does not carry
    /// that nutrient at all — which is deliberately distinct from carrying
    /// 0%.
    #[must_use]
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
    /// The lot's identifier, unique across the curated file: registering a
    /// second lot under one `field_id` is refused rather than merged.
    pub field_id: String,
    /// Which lab report describes this lot's soil. The join to
    /// [`SoilTest`].
    pub sample_id: String,
    /// Soil texture class. A lookup axis for both the efficiency rules and
    /// the critical levels — a sand and a clay lose a mobile nutrient at
    /// very different rates.
    pub texture: Texture,
    /// How the lot is watered. Decides whether the water-deficit and
    /// mineralization rules apply at all: an irrigated lot is not read
    /// against rainfall.
    pub irrigation_system: IrrigationSystem,
    /// Organic matter, percent by mass. The reserve nitrogen mineralizes
    /// out of, and interpreted against a thermal belt rather than a single
    /// scale — see [`Self::climate_zone`].
    pub organic_matter_percent: f64,
    /// Soil reaction. Moves phosphorus availability in both directions and
    /// is what a liming recommendation is ultimately about.
    pub ph: f64,
    /// Cation exchange capacity, cmolc/kg. The denominator of base
    /// saturation, so it is what a liming target is computed against.
    pub cec_cmolc_kg: f64,
    /// Bulk density, kg/dm³. Converts a concentration into a mass per
    /// hectare — with the depth below, it is what a hectare of topsoil
    /// weighs.
    pub bulk_density_kg_dm3: f64,
    /// Depth of the layer a plan is written for, in metres. The other half
    /// of the soil-weight calculation.
    pub arable_depth_m: f64,
    /// Which reference profile's regional rows apply. Falls back to the
    /// catalog's sentinel when the profile names no rows for it.
    pub region: String,
    /// Decimal degrees, WGS84. Optional: a lot with no coordinates simply
    /// gets no climate enrichment, exactly as if the API were down.
    pub latitude: Option<f64>,
    /// Decimal degrees, WGS84. Meaningful only together with
    /// [`Self::latitude`]; either one missing disables climate enrichment.
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
    /// The lot this goal is planned on.
    pub field_id: String,
    /// The crop it is planned for. A lot may carry one row per crop.
    pub crop_id: String,
    /// The goal itself, with the unit it is stated in — which must match
    /// the unit the removal coefficients are stated per.
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
    /// Annual mean air temperature. Drives mineralization, which roughly
    /// doubles per 10 °C, and is the fallback for the thermal belt when a
    /// lot has no altitude on file.
    pub mean_temp_c: Option<f64>,
    /// The *hottest month's* mean daily maximum, not the annual mean of
    /// the daily maxima — the heat-stress rule asks whether any month
    /// crosses a threshold, so the reduction has to be a max, not a mean.
    pub max_temp_c: Option<f64>,
    /// Mean daily precipitation. Read as an annual total against
    /// evapotranspiration to decide whether the site is water-limited.
    pub precip_mm_per_day: Option<f64>,
    /// Mean daily solar irradiance. Feeds the radiation-use-efficiency
    /// index and the Hargreaves evapotranspiration estimate.
    pub solar_mj_m2_per_day: Option<f64>,
    /// Reference evapotranspiration. See `NasaPowerRepo` for why this is
    /// derived rather than fetched.
    pub et0_mm_per_day: Option<f64>,
}

impl AnnualClimatology {
    /// Annual rainfall total, in mm.
    ///
    /// # Returns
    /// `None` when the provider gave no precipitation for this cell — the
    /// aridity rule then does nothing rather than assuming a dry year.
    #[must_use]
    pub fn annual_precip_mm(&self) -> Option<f64> {
        self.precip_mm_per_day.map(|v| v * 365.0)
    }

    /// Annual reference evapotranspiration total, in mm.
    ///
    /// # Returns
    /// `None` when [`Self::et0_mm_per_day`] could not be derived, which is
    /// the case for any partial year of source data.
    #[must_use]
    pub fn annual_et0_mm(&self) -> Option<f64> {
        self.et0_mm_per_day.map(|v| v * 365.0)
    }
}

/// The boundaries one nutrient's soil reading is classified against.
#[derive(Debug, Clone)]
pub struct CriticalLevel {
    /// Below this the soil is [`SoilStatus::Low`], in [`Self::unit`].
    pub low_threshold: f64,
    /// Below this — and at or above `low_threshold` — the soil is
    /// [`SoilStatus::Medium`]; at or above it, [`SoilStatus::High`].
    pub medium_threshold: f64,
    /// The excess/toxicity ceiling. Reported but not part of the
    /// three-way split — see [`Self::classify`].
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
    /// `AGRONOMIC_NOTE`: not decoration for P. Tabla 12 lists Bray II and
    /// Olsen side by side with different boundaries (20/40 vs 16/35 mg/kg)
    /// because the two reagents dissolve different fractions of soil P.
    /// Judging an Olsen reading against Bray II boundaries under-rates the
    /// soil and over-fertilizes; the reverse over-rates it and starves the
    /// crop.
    pub extraction_method: String,
    /// The publication these boundaries were taken from, so a number in a
    /// report can be traced to the literature that set it.
    pub source: String,
    /// Publication year of [`Self::source`].
    pub year: u16,
}

impl CriticalLevel {
    /// `value` must already be expressed in [`Self::unit`].
    ///
    /// `high_threshold` marks an excess/toxicity ceiling and is kept for
    /// reporting; the low/medium/high split itself only needs the first
    /// two boundaries.
    #[must_use]
    pub fn classify(&self, value: f64) -> SoilStatus {
        if value < self.low_threshold {
            SoilStatus::Low
        } else if value < self.medium_threshold {
            SoilStatus::Medium
        } else {
            SoilStatus::High
        }
    }

    /// The same three intervals [`Self::classify`] cuts on, as bands.
    ///
    /// Nutrient thresholds and the qualitative tables say the same kind of
    /// thing in two shapes, and anything that presents a reading against
    /// its scale needs the one shape. Converting here rather than at the
    /// point of use is what stops a second, drifting copy of the cut
    /// points: **whatever changes in `classify` has to change here**, and
    /// `the_bands_cut_where_classify_cuts` fails if it does not.
    ///
    /// `high_threshold` is not a boundary. Nothing is ever compared
    /// against it, so a band on it would be a interval no reading can be
    /// in.
    ///
    /// # Arguments
    /// * `property` — what the thresholds are for. A `CriticalLevel` does
    ///   not carry the nutrient it belongs to; whoever looked it up does.
    ///
    /// # Returns
    /// Three bands, low to high, in [`Self::unit`]. The outer two are open
    /// — the literature states no floor under *low* and no ceiling over
    /// *high*.
    #[must_use]
    pub fn bands(&self, property: &str) -> Vec<QualitativeBand> {
        let band = |category: &str, min, max| QualitativeBand {
            property: property.to_string(),
            category: category.to_string(),
            min_value: min,
            max_value: max,
            unit: self.unit.clone(),
            source: self.source.clone(),
            year: self.year,
        };
        vec![
            band("low", None, Some(self.low_threshold)),
            band("medium", Some(self.low_threshold), Some(self.medium_threshold)),
            band("high", Some(self.medium_threshold), None),
        ]
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
    /// Which soil property this band interprets (`ph`, `organic_matter`,
    /// `ca_mg`...). The lookup key.
    pub property: String,
    /// The name the source table gives this interval — the words that end
    /// up in front of the grower.
    pub category: String,
    /// Inclusive lower bound; `None` for the table's open-ended bottom
    /// band.
    pub min_value: Option<f64>,
    /// Exclusive upper bound; `None` for the table's open-ended top band.
    pub max_value: Option<f64>,
    /// The unit the bounds are stated in. The value tested against them has
    /// to be in this unit already.
    pub unit: String,
    /// The publication this band was taken from.
    pub source: String,
    /// Publication year of [`Self::source`].
    pub year: u16,
}

impl QualitativeBand {
    /// Half-open, `[min, max)`, so bands that share a boundary in the
    /// source table cover the line without overlapping at the seam.
    #[must_use]
    pub fn contains(&self, value: f64) -> bool {
        self.min_value.is_none_or(|min| value >= min) && self.max_value.is_none_or(|max| value < max)
    }
}

/// One soil property read against its interpretation table.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyAssessment {
    /// Which property was read, matching the interpretation table's own
    /// key.
    pub property: String,
    /// The figure that was classified, in [`Self::unit`].
    pub value: f64,
    /// The unit `value` is stated in.
    pub unit: String,
    /// `None` when the table names no band containing this value — see
    /// [`QualitativeBand`].
    pub category: Option<String>,
    /// The publication behind the band that matched. `None` whenever
    /// `category` is `None` — there is no source for a classification that
    /// did not happen.
    pub source: Option<String>,
    /// Every band the reading was judged against, in table order — not
    /// only the one it landed in.
    ///
    /// Carried because the verdict alone cannot be checked. `6.3` reading
    /// *slightly acid* says nothing about whether it sits at the edge of
    /// neutral or at the bottom of the band, and those are different soils
    /// to manage. Whoever classified is the only one who knows what the
    /// intervals were, so shipping them here is what keeps a reader — or a
    /// scale drawn on a screen — from reconstructing them out of a second
    /// copy of the table that can drift from this one.
    ///
    /// Empty when the property has no table at all, which is the same
    /// condition that leaves `category` as `None`.
    pub bands: Vec<QualitativeBand>,
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
    /// Every property that had both a reading and a table to read it
    /// against. A property with no covering band is still listed, with a
    /// `None` category, so the reader can see it was looked at.
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
    /// Kilograms of the nutrient the *harvest leaves the field with*, per
    /// unit of yield. The basis a replacement plan is normally written on.
    pub extraction_kg_per_unit: Option<f64>,
    /// Kilograms the whole crop *takes up*, per unit of yield — including
    /// what the residues return to the soil. Always at least as large as
    /// the extraction figure.
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
    /// The study these coefficients came from.
    pub source: String,
    /// Where the study was carried out. Descriptive: a coefficient measured
    /// elsewhere is still the best available number, but the reader is
    /// entitled to know it was.
    pub region: String,
    /// Publication year of [`Self::source`].
    pub year: u16,
    /// Which revision of the curated dataset this row came from, so a
    /// number in an old report can be matched to the table that produced
    /// it.
    pub dataset_version: String,
}

impl RemovalReference {
    /// The coefficient for one demand basis.
    ///
    /// # Arguments
    /// * `mode` — which basis to read, extraction or absorption.
    ///
    /// # Returns
    /// `None` when the source table printed a dash for that basis. Callers
    /// must not substitute the other column: an unmeasured basis is not the
    /// same number as the one that was measured.
    #[must_use]
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
        /// The nutrient whose basis was switched.
        nutrient: Nutrient,
        /// The dose that resulted, on the absorption basis.
        net_requirement_kg_ha: f64,
    },
    /// Neither column of the reference table has a coefficient for this
    /// crop and nutrient. Reported instead of failing the plan, and
    /// instead of a demand of zero, which would read as "the crop needs
    /// none" rather than "nobody measured it".
    NoRemovalCoefficient {
        /// The nutrient the reference table measured on neither basis.
        nutrient: Nutrient,
    },
}

/// One product and how much of it to apply, per hectare.
#[derive(Debug, Clone)]
pub struct FertilizerDose {
    /// The catalog row this dose is for.
    pub source_id: String,
    /// Its commercial name, carried so a report needs no second lookup.
    pub source_name: String,
    /// Kilograms of *product*, not of nutrient — already divided by the
    /// product's own concentration.
    pub kg_product_per_ha: f64,
}

/// One macronutrient's line of the balance: what the crop asks for, what
/// the soil already offers, and what is left to buy.
#[derive(Debug, Clone)]
pub struct NutrientPlanEntry {
    /// The element this line is about.
    pub nutrient: Nutrient,
    /// What the soil supplies over the season, in kg/ha, from the lab
    /// reading and the mass of soil a hectare of the arable layer holds.
    pub availability_kg_ha: f64,
    /// What the crop asks for at the yield goal, in kg/ha. Zero is a real
    /// demand of zero; an *unknown* demand is `demand_mode_used == None`.
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
    /// What actually has to be applied, in kg/ha of the *element*: the
    /// shortfall after availability, divided by the efficiency. Never
    /// negative — a soil with a surplus asks for nothing, it does not
    /// credit it back.
    pub net_requirement_kg_ha: f64,
    /// The reading classified against its critical level. `None` when the
    /// sample carries no reading for this nutrient, or no threshold table
    /// covers it.
    pub soil_status: Option<SoilStatus>,
    /// The single product that would cover this requirement. `None` when
    /// nothing is required, or when no catalog row carries the nutrient.
    /// The full multi-product program is a separate use case.
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
    /// The micronutrient this correction is about.
    pub nutrient: Nutrient,
    /// The lab reading, in [`Self::unit`] — already converted into the
    /// unit the threshold is stated in.
    pub soil_value: f64,
    /// The unit both `soil_value` and the threshold are stated in.
    pub unit: String,
    /// The boundary below which the soil counts as deficient, in
    /// [`Self::unit`]. The figure the correction is measured against.
    pub critical_low_threshold: f64,
    /// The reading classified. Only a `Low` soil produces a non-zero
    /// deficit.
    pub soil_status: SoilStatus,
    /// How much of the element the soil is short of, in kg/ha, before use
    /// efficiency. Zero for a soil at or above its critical level.
    pub deficit_kg_ha: f64,
    /// The recovery fraction the deficit was divided by to reach
    /// `net_requirement_kg_ha`.
    pub efficiency_used: f64,
    /// What to apply, in kg/ha of the element, after efficiency. Zero for a
    /// soil at or above its critical level.
    pub net_requirement_kg_ha: f64,
    /// The product that would supply it, where the catalog carries one.
    pub dose: Option<FertilizerDose>,
}

/// A liming material: neutralizing value comes from its CaO/MgO content,
/// not from elemental Ca/Mg — kept separate from [`FertilizerSource`]
/// because mixing the two catalogs would misuse elemental-nutrient
/// percentages as neutralizing capacity.
#[derive(Debug, Clone)]
pub struct LimingMaterial {
    /// Catalog key for this material.
    pub source_id: String,
    /// Its commercial name.
    pub name: String,
    /// Calcium oxide content, percent by mass. Half of what sets the
    /// material's neutralizing value — this is *not* elemental calcium.
    pub cao_pct: f64,
    /// Magnesium oxide content, percent by mass. The other half of the
    /// neutralizing value, and what makes a dolomitic material carry
    /// magnesium a calcitic one would dilute.
    pub mgo_pct: f64,
    /// Fraction of the material fine enough to actually react in-field
    /// (granulometric efficiency, "EG"), 0-100.
    pub granulometric_efficiency_pct: f64,
    /// The named combination this material belongs to, if any. Tabla 12
    /// prescribes one — hydrated lime, dolomite and Paz del Río slag in a
    /// 40/45/15 split of the `CaCO3` requirement.
    ///
    /// `AGRONOMIC_NOTE`: the table calls this "mejoramiento químico
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
    /// The publication this row was taken from.
    pub source: String,
    /// Free-text limits on sourcing or applying the material, counted the
    /// same way [`FertilizerSource::restrictions`] are.
    pub restrictions: Vec<String>,
}

/// One liming material and how much of it to apply, per hectare.
#[derive(Debug, Clone)]
pub struct LimingDose {
    /// The material catalog row this dose is for.
    pub source_id: String,
    /// Its commercial name.
    pub source_name: String,
    /// Tonnes of *material*, not of `CaCO3` equivalent — already divided by
    /// the material's own PRNT.
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
    /// Where the soil's base saturation stands now, as a percent of its
    /// effective cation exchange capacity.
    pub current_base_saturation_pct: f64,
    /// Where the profile's rules say it should be. The gap between the two
    /// is what `base_saturation_based_t_ha` is computed from.
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
    /// The combination's name in the source table.
    pub mixture_id: String,
    /// Each material and its dose. The shares behind these sum to the whole
    /// CaCO3-equivalent requirement, so applying all of them meets it once,
    /// not several times over.
    pub components: Vec<LimingDose>,
}

/// Output of `CalculateFertilityPlan`: net nutrient requirements and
/// recommended fertilizer doses for a field/crop/yield scenario.
#[derive(Debug, Clone)]
pub struct FertilityPlan {
    /// The lot this plan was written for.
    pub field_id: String,
    /// The lab report it was written from.
    pub sample_id: String,
    /// The crop it was written for.
    pub crop_id: String,
    /// The goal the demand was computed at, with its unit.
    pub yield_target: YieldTarget,
    /// Which basis the caller asked for. Individual entries may differ
    /// from it — see [`PlanWarning::FallbackToAbsorption`].
    pub demand_mode: NutrientDemandMode,
    /// One line per macronutrient, in the order the domain lists them.
    pub nutrient_results: Vec<NutrientPlanEntry>,
    /// Only the micronutrients this sample was actually tested for. A
    /// micronutrient with no lab reading is absent rather than assumed
    /// adequate — untested is not the same as sufficient.
    pub micronutrients: Vec<MicronutrientCorrection>,
    /// `None` when the sample carries no exchangeable aluminium reading, so
    /// there is nothing to base a lime requirement on.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// [`CriticalLevel::bands`] restates the cuts [`CriticalLevel::classify`]
    /// makes, and two statements of one rule drift. This is what stops
    /// them: a value classified `Low` has to fall in the band named `low`,
    /// at every boundary and on both sides of it.
    #[test]
    fn the_bands_cut_where_classify_cuts() {
        let level = CriticalLevel {
            low_threshold: 16.0,
            medium_threshold: 35.0,
            high_threshold: 35.0,
            unit: "mg_per_kg".to_string(),
            extraction_method: "Olsen".to_string(),
            source: "Castro_Gomez_2009".to_string(),
            year: 2009,
        };
        let bands = level.bands("P");

        // The boundaries themselves and a value either side of each: a
        // half-open interval is only ever wrong at its seams.
        for value in [0.0, 15.9, 16.0, 16.1, 34.9, 35.0, 35.1, 1000.0] {
            let expected = match level.classify(value) {
                SoilStatus::Low => "low",
                SoilStatus::Medium => "medium",
                SoilStatus::High => "high",
            };
            let drawn = bands.iter().find(|band| band.contains(value)).expect("every value is in some band");
            assert_eq!(drawn.category, expected, "{value} classifies `{expected}` but bands say `{}`", drawn.category);
        }
    }
}
