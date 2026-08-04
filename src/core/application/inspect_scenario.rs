//! Reading a lot without planning one.
//!
//! The interpretation half on its own: the readings classified against the
//! critical levels, the qualitative soil-quality assessment, and where each
//! number came from. Nothing here computes a dose, which is why a front-end
//! can show a lot before anyone has asked for a recommendation.

use super::scenario::FertilityScenario;
use crate::core::domain::{
    services, CriticalLevel, DomainError, FieldContext, Nutrient, PropertyAssessment, RemovalReference,
    SoilQualityAssessment, SoilTest, YieldTarget,
};
use crate::core::ports::{
    ConversionFactorsRepository, CriticalLevelsRepository, EfficiencyRulesRepository, FieldContextRepository,
    InspectScenarioPort, NutrientRemovalRepository, SoilQualityThresholdsRepository, SoilTestRepository,
    YieldTargetRepository,
};

/// Charge-equivalent unit the cation ratios and the acidity diagnosis are
/// both stated in. Ratios of mg/kg values have the same shape and
/// different numbers, which is the worst kind of wrong.
const CMOLC_PER_KG: &str = "cmolc_per_kg";

/// Where each number used in the plan comes from.
#[derive(Debug, Clone)]
pub struct NutrientProvenance {
    /// The element these sources are for.
    pub nutrient: Nutrient,
    /// The demand coefficients and the study behind them. `None` when the
    /// table has no row for this crop and nutrient.
    pub removal_reference: Option<RemovalReference>,
    /// The base `(min, max)` recovery fractions before site conditions.
    /// `None` when the profile states no row for this nutrient.
    pub efficiency_range: Option<(f64, f64)>,
    /// The thresholds the reading is classified against. `None` when no
    /// row covers this nutrient and extraction method.
    pub critical_level: Option<CriticalLevel>,
}

/// The data behind a scenario, without computing doses.
#[derive(Debug, Clone)]
pub struct ScenarioInspection {
    /// The lot as curated.
    pub field_context: FieldContext,
    /// Every reading of the sample, as the lab reported it.
    pub soil_tests: Vec<SoilTest>,
    /// The goal the inspection was read against — the override if one was
    /// given, otherwise the curated row.
    pub yield_target: YieldTarget,
    /// Which table each number would come from, so a reader can check the
    /// science behind a figure before trusting the plan built on it.
    pub provenance: Vec<NutrientProvenance>,
    /// What the analysis *means*: pH class, organic matter against its
    /// thermal belt, salinity, CEC, the acidity diagnosis and the cation
    /// balance. None of it feeds a dose.
    pub soil_quality: SoilQualityAssessment,
}

/// The read-only use case: interprets a lot without planning a dose.
pub struct InspectScenario {
    soil_tests: Box<dyn SoilTestRepository>,
    field_context: Box<dyn FieldContextRepository>,
    yield_targets: Box<dyn YieldTargetRepository>,
    nutrient_removal: Box<dyn NutrientRemovalRepository>,
    efficiency_rules: Box<dyn EfficiencyRulesRepository>,
    critical_levels: Box<dyn CriticalLevelsRepository>,
    conversion_factors: Box<dyn ConversionFactorsRepository>,
    soil_quality: Box<dyn SoilQualityThresholdsRepository>,
}

impl InspectScenario {
    /// # Arguments
    /// One boxed repository per reference or curated table the inspection
    /// reads, in the order the fields are declared.
    ///
    /// # Returns
    /// The use case, ready to inspect.
    // Wide because the inspection genuinely reads this many tables; a
    // parameter struct would only move the same list one file over.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        soil_tests: Box<dyn SoilTestRepository>,
        field_context: Box<dyn FieldContextRepository>,
        yield_targets: Box<dyn YieldTargetRepository>,
        nutrient_removal: Box<dyn NutrientRemovalRepository>,
        efficiency_rules: Box<dyn EfficiencyRulesRepository>,
        critical_levels: Box<dyn CriticalLevelsRepository>,
        conversion_factors: Box<dyn ConversionFactorsRepository>,
        soil_quality: Box<dyn SoilQualityThresholdsRepository>,
    ) -> Self {
        Self {
            soil_tests,
            field_context,
            yield_targets,
            nutrient_removal,
            efficiency_rules,
            critical_levels,
            conversion_factors,
            soil_quality,
        }
    }

    /// One reading judged against its interpretation table. A property
    /// whose table is missing comes back with `category: None` rather than
    /// failing: nothing here is required to plan.
    fn assess(&self, property: &str, zone: &str, value: f64, unit: &str) -> PropertyAssessment {
        let bands = self.soil_quality.bands(property, zone).unwrap_or_default();
        let band = services::classify_band(&bands, value).cloned();
        PropertyAssessment {
            property: property.to_string(),
            value,
            unit: unit.to_string(),
            category: band.as_ref().map(|b| b.category.clone()),
            source: band.map(|b| format!("{} ({})", b.source, b.year)),
            bands,
        }
    }

    /// The whole qualitative reading of a soil analysis.
    ///
    /// Every step degrades rather than fails. An unconvertible cation
    /// leaves the ratios that need it out; a lot with no altitude and no
    /// climatology leaves organic matter uninterpreted. Saying nothing is
    /// honest; classifying off a unit nobody checked is not.
    fn assess_soil_quality(&self, context: &FieldContext, tests: &[SoilTest]) -> SoilQualityAssessment {
        let zone = context.climate_zone(None);
        let zone_key = zone.map_or_else(|| "any".to_string(), |z| z.to_string());

        let mut properties = vec![
            self.assess("ph", &zone_key, context.ph, "ph"),
            self.assess("cec", &zone_key, context.cec_cmolc_kg, CMOLC_PER_KG),
        ];
        // Only offered when a belt is known: the same 3% is high in the
        // lowlands and very low above 2000 m, so the unkeyed answer would
        // be a coin flip dressed as a diagnosis.
        if zone.is_some() {
            properties.push(self.assess("organic_matter", &zone_key, context.organic_matter_percent, "percent"));
        }

        let cmolc = |nutrient: Nutrient| -> Option<f64> {
            let test = tests.iter().find(|t| t.nutrient == nutrient)?;
            if test.unit == CMOLC_PER_KG {
                return Some(test.value);
            }
            self.conversion_factors
                .convert(&test.unit, CMOLC_PER_KG, nutrient.as_str(), test.value)
                .ok()
        };
        let (ca, mg, k) = (cmolc(Nutrient::Ca), cmolc(Nutrient::Mg), cmolc(Nutrient::K));

        if let (Some(ca), Some(mg), Some(k), Some(al)) = (ca, mg, k, cmolc(Nutrient::Al)) {
            let h = cmolc(Nutrient::H).unwrap_or(0.0);
            let cice = services::cation_exchange_capacity_effective(h, al, k, mg, ca);
            properties.push(self.assess("exchangeable_aluminum", &zone_key, al, CMOLC_PER_KG));
            properties.push(self.assess(
                "aluminum_saturation",
                &zone_key,
                services::aluminum_saturation_pct(al, cice),
                "percent",
            ));
            properties.push(self.assess("sum_of_bases", &zone_key, ca + mg + k, CMOLC_PER_KG));
        }

        let mut cation_ratios = Vec::new();
        if let (Some(ca), Some(mg), Some(k)) = (ca, mg, k) {
            let ratios = services::cation_ratios(ca, mg, k);
            for (property, value) in [
                ("ca_to_mg", ratios.ca_to_mg),
                ("mg_to_k", ratios.mg_to_k),
                ("k_to_mg", ratios.k_to_mg),
                ("ca_to_k", ratios.ca_to_k),
                ("ca_plus_mg_to_k", ratios.ca_plus_mg_to_k),
            ] {
                if let Some(value) = value {
                    cation_ratios.push(self.assess(property, &zone_key, value, "ratio"));
                }
            }
        }

        SoilQualityAssessment { climate_zone: zone, properties, cation_ratios }
    }
}

impl InspectScenarioPort for InspectScenario {
    fn inspect(&self, scenario: &FertilityScenario) -> Result<ScenarioInspection, DomainError> {
        let field_context = self.field_context.get_context_by_field_id(&scenario.field_id)?;
        let soil_tests = self.soil_tests.get_tests_by_sample_id(&scenario.sample_id)?;
        let yield_target = match scenario.yield_override.clone() {
            Some(yt) => yt,
            None => self.yield_targets.get_yield_target(&scenario.field_id, &scenario.crop_id)?,
        };

        let mut provenance = Vec::with_capacity(Nutrient::MACRONUTRIENTS.len());
        for nutrient in Nutrient::MACRONUTRIENTS {
            let nutrient_id = nutrient.as_str();
            let removal_reference = self.nutrient_removal.describe_removal(&scenario.crop_id, nutrient_id).ok();
            let efficiency_range = self
                .efficiency_rules
                .get_efficiency_range(&field_context.texture, &field_context.irrigation_system, nutrient_id)
                .ok();
            // Keyed on the method this sample's own reading came from, so
            // the thresholds shown are the ones the plan actually judged
            // it against rather than an arbitrary pick between Bray II and
            // Olsen.
            let method = soil_tests.iter().find(|t| t.nutrient == nutrient).map(|t| t.method.as_str());
            let critical_level = self
                .critical_levels
                .get_critical_level(nutrient_id, &field_context.texture, &field_context.region, method)
                .ok();

            provenance.push(NutrientProvenance {
                nutrient,
                removal_reference,
                efficiency_range,
                critical_level,
            });
        }

        let soil_quality = self.assess_soil_quality(&field_context, &soil_tests);

        Ok(ScenarioInspection {
            field_context,
            soil_tests,
            yield_target,
            provenance,
            soil_quality,
        })
    }
}
