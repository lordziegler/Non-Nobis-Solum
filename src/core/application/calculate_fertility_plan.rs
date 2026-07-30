use super::scenario::FertilityScenario;
use crate::core::domain::{
    services, AnnualClimatology, DomainError, FertilityPlan, FertilizerDose, FieldContext, LimingDose,
    LimingRecommendation, Nutrient, NutrientPlanEntry, SoilTest,
};
use crate::core::ports::{
    AgroclimaticRepository, ConversionFactorsRepository, CriticalLevelsRepository, EfficiencyRulesRepository,
    FertilityCalculatorPort, FertilizerSourceRepository, FieldContextRepository, LimingMaterialRepository,
    LimingRulesRepository, NutrientRemovalRepository, SoilTestRepository, YieldTargetRepository,
};

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
    liming_rules: Box<dyn LimingRulesRepository>,
    liming_materials: Box<dyn LimingMaterialRepository>,
    /// The one optional dependency: every other repository reads a local
    /// file and its absence is a bug, but this one crosses a network and
    /// the plan is required to run without it.
    agroclimatic: Option<Box<dyn AgroclimaticRepository>>,
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
        liming_rules: Box<dyn LimingRulesRepository>,
        liming_materials: Box<dyn LimingMaterialRepository>,
        agroclimatic: Option<Box<dyn AgroclimaticRepository>>,
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
            liming_rules,
            liming_materials,
            agroclimatic,
        }
    }

    /// Best-effort climatology for a lot. Returns `None` — never an error
    /// — when climate is switched off, the lot has no coordinates, or the
    /// provider is unreachable. Collapsing all three into one `None` is
    /// deliberate: from the plan's perspective they are the same
    /// situation, and the caller reports it with a single warning.
    fn resolve_climate(&self, field_context: &FieldContext) -> Option<AnnualClimatology> {
        let repo = self.agroclimatic.as_ref()?;
        let latitude = field_context.latitude?;
        let longitude = field_context.longitude?;
        repo.fetch_climatology(latitude, longitude).ok()
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

    /// `t_ha` is a CaCO3-equivalent requirement; picks the material with
    /// the highest PRNT, same "highest wins" heuristic as `best_source_for`.
    fn best_liming_material_for(&self, t_ha: f64) -> Result<Option<LimingDose>, DomainError> {
        let materials = self.liming_materials.list_materials()?;
        let best = materials
            .iter()
            .map(|m| {
                let eq = services::neutralizing_value_pct(m.cao_pct, m.mgo_pct);
                (m, services::prnt(eq, m.granulometric_efficiency_pct))
            })
            .filter(|(_, prnt)| *prnt > 0.0)
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap());

        Ok(best.map(|(material, prnt)| LimingDose {
            source_id: material.source_id.clone(),
            source_name: material.name.clone(),
            t_product_per_ha: services::lime_material_dose_t_ha(t_ha, prnt),
        }))
    }

    /// Lime requirement, computed only when the sample has an Al³⁺ test
    /// (the workflow's "encalamiento si aplica"). K⁺/Mg²⁺/Ca²⁺/H⁺ are read
    /// as their raw cmolc/kg lab values, not the kg/ha availability used
    /// elsewhere in the plan — the liming formulas operate directly in
    /// cmolc/kg (assumes these tests are reported in that unit, same as
    /// every soil test currently in `data/curated/`).
    fn calculate_liming(&self, soil_tests: &[SoilTest], cic_cmolc_kg: f64, region: &str) -> Result<Option<LimingRecommendation>, DomainError> {
        let Some(al_test) = soil_tests.iter().find(|t| t.nutrient == Nutrient::Al) else {
            return Ok(None);
        };
        let cmolc_value_of = |nutrient: Nutrient| soil_tests.iter().find(|t| t.nutrient == nutrient).map(|t| t.value).unwrap_or(0.0);

        let al = al_test.value;
        let h = cmolc_value_of(Nutrient::H);
        let k = cmolc_value_of(Nutrient::K);
        let mg = cmolc_value_of(Nutrient::Mg);
        let ca = cmolc_value_of(Nutrient::Ca);

        let cice = services::cation_exchange_capacity_effective(h, al, k, mg, ca);
        let current_base_saturation_pct = services::base_saturation_pct(k, mg, ca, cice);

        let al_factor = self.liming_rules.al_factor(region)?;
        let target_base_saturation_pct = self.liming_rules.target_base_saturation_pct(region)?;

        let al_based_t_ha = services::lime_requirement_from_aluminum_t_ha(al, al_factor);
        let base_saturation_based_t_ha =
            services::lime_requirement_from_base_saturation_t_ha(cic_cmolc_kg, current_base_saturation_pct, target_base_saturation_pct);
        // ponytail: "take the max of both methods" is a conservative stand-in
        // for the real rule (use the Al method only once Al saturation
        // crosses a crop-specific toxicity threshold). Revisit once
        // per-crop Al tolerance data exists.
        let recommended_t_ha = al_based_t_ha.max(base_saturation_based_t_ha);

        let material = if recommended_t_ha > 0.0 { self.best_liming_material_for(recommended_t_ha)? } else { None };

        Ok(Some(LimingRecommendation {
            al_based_t_ha,
            base_saturation_based_t_ha,
            recommended_t_ha,
            current_base_saturation_pct,
            target_base_saturation_pct,
            material,
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

        let climate = self.resolve_climate(&field_context);

        // Both climate-derived quantities fall back to "as if there were
        // no climate at all": the baseline constant and a zero delta.
        // Note the mineralization factor needs *both* temperature and
        // precipitation before it will replace the baseline, even under
        // irrigation where precipitation is then discarded — a partial
        // climatology is not obviously better than the calibrated default.
        let mineralization_factor = climate
            .as_ref()
            .and_then(|c| Some(services::mineralization_factor(c.mean_temp_c?, c.annual_precip_mm()?, &field_context.irrigation_system)))
            .unwrap_or(services::BASELINE_MINERALIZATION_FACTOR);

        let efficiency_adjustment = climate
            .as_ref()
            .map(|c| services::efficiency_climate_adjustment(c, &field_context.irrigation_system))
            .unwrap_or_default();

        let mut nutrient_results = Vec::with_capacity(Nutrient::MACRONUTRIENTS.len());
        for nutrient in Nutrient::MACRONUTRIENTS {
            let nutrient_id = nutrient.as_str();
            let test = soil_tests.iter().find(|t| t.nutrient == nutrient);

            let availability_kg_ha = if nutrient == Nutrient::N {
                // N has no soil-test-based path: it's derived from organic
                // matter, never measured as a raw ppm value (see workflow
                // reference and the prototype's n.py).
                services::nitrogen_available_kg_ha(field_context.organic_matter_percent, mineralization_factor, soil_weight)
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
            // Climate can only pull the optimistic end of the range down,
            // and never below the pessimistic end — a stress signal must
            // narrow the range, not invert it.
            let efficiency_max = match nutrient {
                Nutrient::N => efficiency_max + efficiency_adjustment.n_max_delta,
                Nutrient::P => efficiency_max + efficiency_adjustment.p_max_delta,
                Nutrient::K => efficiency_max + efficiency_adjustment.k_max_delta,
                _ => efficiency_max,
            }
            .max(efficiency_min);
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

        let liming = self.calculate_liming(&soil_tests, field_context.cec_cmolc_kg, &field_context.region)?;

        Ok(FertilityPlan {
            field_id: scenario.field_id,
            sample_id: scenario.sample_id,
            crop_id: scenario.crop_id,
            yield_target,
            nutrient_results,
            liming,
            mineralization_factor,
            climate,
        })
    }
}
