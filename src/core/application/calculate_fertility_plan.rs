use super::scenario::FertilityScenario;
use crate::core::domain::{services, DomainError, FertilityPlan, FertilizerDose, Nutrient, NutrientPlanEntry};
use crate::core::ports::{
    ConversionFactorsRepository, CriticalLevelsRepository, EfficiencyRulesRepository, FertilizerSourceRepository,
    FertilityCalculatorPort, FieldContextRepository, NutrientRemovalRepository, SoilTestRepository,
    YieldTargetRepository,
};

/// Fraction of total soil N assumed to mineralize annually. Matches the
/// prototype's hardcoded 1.5% (`n.py`); not yet profile/texture-specific.
// ponytail: single global constant, promote to a per-profile/texture value
// (like efficiency_rules.yaml) if calibration ever needs it to vary.
const ANNUAL_MINERALIZATION_FACTOR: f64 = 0.015;

/// The main use case: turns a scenario (field + crop + yield goal) into a
/// full fertility plan by combining curated scenario data with the
/// reference-data catalog.
pub struct CalculateFertilityPlan {
    soil_tests: Box<dyn SoilTestRepository>,
    field_context: Box<dyn FieldContextRepository>,
    yield_targets: Box<dyn YieldTargetRepository>,
    nutrient_removal: Box<dyn NutrientRemovalRepository>,
    conversion_factors: Box<dyn ConversionFactorsRepository>,
    efficiency_rules: Box<dyn EfficiencyRulesRepository>,
    critical_levels: Box<dyn CriticalLevelsRepository>,
    fertilizer_sources: Box<dyn FertilizerSourceRepository>,
}

impl CalculateFertilityPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        soil_tests: Box<dyn SoilTestRepository>,
        field_context: Box<dyn FieldContextRepository>,
        yield_targets: Box<dyn YieldTargetRepository>,
        nutrient_removal: Box<dyn NutrientRemovalRepository>,
        conversion_factors: Box<dyn ConversionFactorsRepository>,
        efficiency_rules: Box<dyn EfficiencyRulesRepository>,
        critical_levels: Box<dyn CriticalLevelsRepository>,
        fertilizer_sources: Box<dyn FertilizerSourceRepository>,
    ) -> Self {
        Self {
            soil_tests,
            field_context,
            yield_targets,
            nutrient_removal,
            conversion_factors,
            efficiency_rules,
            critical_levels,
            fertilizer_sources,
        }
    }

    fn best_source_for(&self, nutrient: Nutrient, net_kg_ha: f64) -> Result<Option<FertilizerDose>, DomainError> {
        let sources = self.fertilizer_sources.list_sources()?;
        let best = sources
            .iter()
            .filter_map(|s| s.pct_of(nutrient).map(|pct| (s, pct)))
            .filter(|(_, pct)| *pct > 0.0)
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap());

        Ok(best.map(|(source, pct)| FertilizerDose {
            source_id: source.source_id.clone(),
            source_name: source.name.clone(),
            kg_product_per_ha: services::dose_kg_product_ha(net_kg_ha, pct),
        }))
    }
}

impl FertilityCalculatorPort for CalculateFertilityPlan {
    fn calculate(&self, scenario: FertilityScenario) -> Result<FertilityPlan, DomainError> {
        let field_context = self.field_context.get_context_by_field_id(&scenario.field_id)?;
        let soil_tests = self.soil_tests.get_tests_by_sample_id(&scenario.sample_id)?;
        let yield_target = match scenario.yield_override {
            Some(yt) => yt,
            None => self.yield_targets.get_yield_target(&scenario.field_id, &scenario.crop_id)?,
        };

        let soil_weight = services::soil_weight_kg_ha(field_context.bulk_density_kg_dm3, field_context.arable_depth_m);

        let mut nutrient_results = Vec::with_capacity(Nutrient::MACRONUTRIENTS.len());
        for nutrient in Nutrient::MACRONUTRIENTS {
            let nutrient_id = nutrient.as_str();
            let test = soil_tests.iter().find(|t| t.nutrient == nutrient);

            let availability_kg_ha = if nutrient == Nutrient::N {
                // N has no soil-test-based path: it's derived from organic
                // matter, never measured as a raw ppm value (see workflow
                // reference and the prototype's n.py).
                services::nitrogen_available_kg_ha(
                    field_context.organic_matter_percent,
                    ANNUAL_MINERALIZATION_FACTOR,
                    soil_weight,
                )
            } else {
                match test {
                    Some(test) => {
                        let value_mg_kg = if test.unit == "mg_per_kg" {
                            test.value
                        } else {
                            self.conversion_factors.convert(&test.unit, "mg_per_kg", nutrient_id, test.value)?
                        };
                        services::availability_kg_ha(value_mg_kg, soil_weight)
                    }
                    None => 0.0,
                }
            };

            let demand_kg_ha = self.nutrient_removal.get_removal(
                &scenario.crop_id,
                &scenario.product,
                nutrient_id,
                yield_target.value,
                &yield_target.unit,
            )?;

            let (efficiency_min, efficiency_max) = self.efficiency_rules.get_efficiency_range(
                &field_context.texture,
                &field_context.irrigation_system,
                nutrient_id,
            )?;
            let efficiency_used = (efficiency_min + efficiency_max) / 2.0;

            let net_requirement_kg_ha = services::net_requirement_kg_ha(demand_kg_ha, availability_kg_ha, efficiency_used);

            let soil_status = self
                .critical_levels
                .get_critical_level(nutrient_id, &field_context.texture, &field_context.region)
                .ok()
                .zip(test)
                .map(|(level, test)| level.classify(test.value));

            let dose = if net_requirement_kg_ha > 0.0 {
                self.best_source_for(nutrient, net_requirement_kg_ha)?
            } else {
                None
            };

            nutrient_results.push(NutrientPlanEntry {
                nutrient,
                availability_kg_ha,
                demand_kg_ha,
                efficiency_used,
                net_requirement_kg_ha,
                soil_status,
                dose,
            });
        }

        Ok(FertilityPlan {
            field_id: scenario.field_id,
            sample_id: scenario.sample_id,
            crop_id: scenario.crop_id,
            yield_target,
            nutrient_results,
        })
    }
}
