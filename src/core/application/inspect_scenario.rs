use super::scenario::FertilityScenario;
use crate::core::domain::{CriticalLevel, DomainError, FieldContext, Nutrient, RemovalReference, SoilTest, YieldTarget};
use crate::core::ports::{
    CriticalLevelsRepository, EfficiencyRulesRepository, FieldContextRepository, InspectScenarioPort,
    NutrientRemovalRepository, SoilTestRepository, YieldTargetRepository,
};

/// Per-nutrient provenance shown by `InspectScenario`: where each number
/// used in the plan actually comes from.
#[derive(Debug, Clone)]
pub struct NutrientProvenance {
    pub nutrient: Nutrient,
    pub removal_reference: Option<RemovalReference>,
    pub efficiency_range: Option<(f64, f64)>,
    pub critical_level: Option<CriticalLevel>,
}

/// Full picture of the data behind a scenario, without computing doses —
/// useful to answer "where do these numbers come from?" before trusting
/// a plan.
#[derive(Debug, Clone)]
pub struct ScenarioInspection {
    pub field_context: FieldContext,
    pub soil_tests: Vec<SoilTest>,
    pub yield_target: YieldTarget,
    pub provenance: Vec<NutrientProvenance>,
}

pub struct InspectScenario {
    soil_tests: Box<dyn SoilTestRepository>,
    field_context: Box<dyn FieldContextRepository>,
    yield_targets: Box<dyn YieldTargetRepository>,
    nutrient_removal: Box<dyn NutrientRemovalRepository>,
    efficiency_rules: Box<dyn EfficiencyRulesRepository>,
    critical_levels: Box<dyn CriticalLevelsRepository>,
}

impl InspectScenario {
    pub fn new(
        soil_tests: Box<dyn SoilTestRepository>,
        field_context: Box<dyn FieldContextRepository>,
        yield_targets: Box<dyn YieldTargetRepository>,
        nutrient_removal: Box<dyn NutrientRemovalRepository>,
        efficiency_rules: Box<dyn EfficiencyRulesRepository>,
        critical_levels: Box<dyn CriticalLevelsRepository>,
    ) -> Self {
        Self {
            soil_tests,
            field_context,
            yield_targets,
            nutrient_removal,
            efficiency_rules,
            critical_levels,
        }
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
            let removal_reference = self
                .nutrient_removal
                .describe_removal(&scenario.crop_id, &scenario.product, nutrient_id)
                .ok();
            let efficiency_range = self
                .efficiency_rules
                .get_efficiency_range(&field_context.texture, &field_context.irrigation_system, nutrient_id)
                .ok();
            let critical_level = self
                .critical_levels
                .get_critical_level(nutrient_id, &field_context.texture, &field_context.region)
                .ok();

            provenance.push(NutrientProvenance {
                nutrient,
                removal_reference,
                efficiency_range,
                critical_level,
            });
        }

        Ok(ScenarioInspection {
            field_context,
            soil_tests,
            yield_target,
            provenance,
        })
    }
}
