use super::scenario::FertilityScenario;
use crate::core::domain::{
    efficiency, services, AnnualClimatology, DomainError, FertilityPlan, FertilizerDose, FieldContext, LimingDose,
    LimingMixture, LimingRecommendation, MicronutrientCorrection, Nutrient, NutrientDemandMode, NutrientPlanEntry,
    PlanWarning, ScenarioConditions, SoilTest, SulfurForm, YieldTarget,
};
use crate::core::ports::{
    AgroclimaticRepository, ConversionFactorsRepository, CriticalLevelsRepository, EfficiencyRulesRepository,
    EfficiencyBandRepository, FertilityCalculatorPort, FertilizerSourceRepository, FieldContextRepository,
    LimingMaterialRepository,
    LimingRulesRepository, NutrientRemovalRepository, SoilTestRepository, YieldTargetRepository,
};

/// The two units a soil test can be reported in. Every figure the plan
/// derives from a lab reading is compared against literature stated in one
/// specific unit, so the reading has to be moved to that unit first — see
/// [`CalculateFertilityPlan::value_in`].
const MG_PER_KG: &str = "mg_per_kg";
const CMOLC_PER_KG: &str = "cmolc_per_kg";

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
    efficiency_bands: Box<dyn EfficiencyBandRepository>,
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
        efficiency_bands: Box<dyn EfficiencyBandRepository>,
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
            efficiency_bands,
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

    /// A soil test's value expressed in `unit`, converting when the lab
    /// reported something else.
    ///
    /// Every consumer downstream compares the reading against literature
    /// fixed in one unit: availability against mg/kg, the liming formulas
    /// against cmolc/kg, a critical level against whatever unit its own
    /// row declares. Comparing across units doesn't fail loudly, it just
    /// answers wrong — a K reading of 97 mg/kg is 0.25 cmolc/kg, and
    /// judged against cmolc/kg thresholds it reads as "high" when the
    /// soil is merely adequate. One conversion point, no consumer left
    /// guessing what the lab meant.
    fn value_in(&self, test: &SoilTest, unit: &str) -> Result<f64, DomainError> {
        if test.unit == unit {
            return Ok(test.value);
        }
        self.conversion_factors.convert(&test.unit, unit, test.nutrient.as_str(), test.value)
    }

    /// Crop demand for one nutrient on each basis the reference table has,
    /// in kg/ha at this yield goal.
    ///
    /// `Ok(None)` is "the table has no row for this crop and nutrient at
    /// all", which is not an error: it must not fail the whole plan, and
    /// it must not read as a demand of zero either. Anything else wrong
    /// with the table still propagates.
    fn crop_demand(
        &self,
        crop_id: &str,
        nutrient_id: &str,
        yield_target: &YieldTarget,
    ) -> Result<Option<DemandByBasis>, DomainError> {
        let reference = match self.nutrient_removal.describe_removal(crop_id, nutrient_id) {
            Ok(reference) => reference,
            Err(DomainError::NotFound(_)) => return Ok(None),
            Err(other) => return Err(other),
        };

        // Checked before either coefficient is multiplied by the goal: a
        // coefficient stated per tonne applied to a goal in anything else
        // is wrong by whatever the units differ by, with nothing to show
        // for it in the output.
        if reference.yield_unit != yield_target.unit {
            return Err(DomainError::InvalidInput(format!(
                "yield_unit mismatch for crop_id={crop_id}: reference table states {}, scenario asks for {}",
                reference.yield_unit, yield_target.unit
            )));
        }

        let at_goal = |mode| {
            reference
                .coefficient(mode)
                .map(|coefficient| services::crop_removal_kg_ha(coefficient, yield_target.value))
        };
        Ok(Some(DemandByBasis {
            extraction: at_goal(NutrientDemandMode::Extraction),
            absorption: at_goal(NutrientDemandMode::Absorption),
        }))
    }

    /// Micronutrients, corrected against their critical level rather than
    /// balanced against a crop removal.
    ///
    /// AGRONOMIC_NOTE: this is a different question from the one the
    /// macronutrient path answers, not a cheaper version of it. There is
    /// no removal coefficient for any micronutrient in the source tables,
    /// and a "replace what the harvest took" plan would be beside the
    /// point anyway: a crop removes grams per hectare of zinc from a
    /// reserve measured in kilograms, so offtake is never what makes a
    /// soil deficient. What matters is whether the concentration in
    /// solution is high enough for uptake to happen at all. So the target
    /// is the critical level itself:
    ///
    /// ```text
    /// deficit_mg_kg = low_threshold - reading          (0 if already at it)
    /// deficit_kg_ha = deficit_mg_kg x soil_weight / 1e6
    /// net_kg_ha     = deficit_kg_ha / efficiency
    /// ```
    ///
    /// The same `availability_kg_ha` conversion the macronutrient path
    /// uses, run backwards: the mass that raises the measured
    /// concentration to the threshold across the arable layer.
    ///
    /// Two limits worth stating. This corrects to the *bottom* of the
    /// adequate band, not into the middle of it — the cheapest application
    /// that clears deficiency, which is the right default for elements
    /// whose toxic range starts not far above their deficient one. And it
    /// assumes the applied element stays plant-available, which is
    /// optimistic on high-pH soils where Fe, Mn and Zn precipitate; the
    /// efficiency range is the only place that is accounted for.
    fn correct_micronutrients(
        &self,
        soil_tests: &[SoilTest],
        field_context: &FieldContext,
        soil_weight: f64,
    ) -> Result<Vec<MicronutrientCorrection>, DomainError> {
        let mut corrections = Vec::new();
        for nutrient in Nutrient::MICRONUTRIENTS {
            let nutrient_id = nutrient.as_str();
            // Both are required, and neither is assumed: an untested
            // micronutrient is unknown, not adequate, and one with no
            // threshold in this profile cannot be judged at all.
            let Some(test) = soil_tests.iter().find(|t| t.nutrient == nutrient) else {
                continue;
            };
            let Ok(level) = self.critical_levels.get_critical_level(
                nutrient_id,
                &field_context.texture,
                &field_context.region,
                Some(&test.method),
            ) else {
                continue;
            };
            // A reading that cannot be moved into the threshold's unit is
            // dropped rather than compared at face value — the same rule
            // the macronutrient soil status follows.
            let Ok(soil_value) = self.value_in(test, &level.unit) else {
                continue;
            };

            let deficit_concentration = (level.low_threshold - soil_value).max(0.0);
            let deficit_kg_ha = services::availability_kg_ha(deficit_concentration, soil_weight);

            let (efficiency_min, efficiency_max) =
                self.efficiency_rules
                    .get_efficiency_range(&field_context.texture, &field_context.irrigation_system, nutrient_id)?;
            let efficiency_used = (efficiency_min + efficiency_max) / 2.0;
            let net_requirement_kg_ha = services::net_requirement_kg_ha(deficit_kg_ha, 0.0, efficiency_used);

            let dose = if net_requirement_kg_ha > 0.0 {
                self.best_source_for(nutrient, net_requirement_kg_ha)?
            } else {
                None
            };

            corrections.push(MicronutrientCorrection {
                nutrient,
                soil_value,
                unit: level.unit.clone(),
                critical_low_threshold: level.low_threshold,
                soil_status: level.classify(soil_value),
                deficit_kg_ha,
                efficiency_used,
                net_requirement_kg_ha,
                dose,
            });
        }
        Ok(corrections)
    }

    fn best_source_for(&self, nutrient: Nutrient, net_kg_ha: f64) -> Result<Option<FertilizerDose>, DomainError> {
        let sources = self.fertilizer_sources.list_sources()?;
        let best = highest_rated(sources.iter().filter_map(|s| s.pct_of(nutrient).map(|pct| (s, pct))));

        Ok(best.and_then(|(source, pct)| {
            Some(FertilizerDose {
                source_id: source.source_id.clone(),
                source_name: source.name.clone(),
                kg_product_per_ha: services::dose_kg_product_ha(net_kg_ha, pct)?,
            })
        }))
    }

    /// `t_ha` is a CaCO3-equivalent requirement; picks the material with
    /// the highest PRNT, same "highest wins" heuristic as `best_source_for`.
    fn best_liming_material_for(&self, t_ha: f64) -> Result<Option<LimingDose>, DomainError> {
        let materials = self.liming_materials.list_materials()?;
        let best = highest_rated(materials.iter().map(|m| (m, prnt_of(m))));

        Ok(best.and_then(|(material, prnt)| {
            Some(LimingDose {
                source_id: material.source_id.clone(),
                source_name: material.name.clone(),
                t_product_per_ha: services::lime_material_dose_t_ha(t_ha, prnt)?,
            })
        }))
    }

    /// The same requirement met by Tabla 12's prescribed combination
    /// instead of by one product: each material covers its own share of
    /// the CaCO3-equivalent total.
    ///
    /// `None` when the catalog declares no mixture at all, which is the
    /// normal case for a profile that ships only single materials.
    fn liming_mixture_for(&self, t_ha: f64) -> Result<Option<LimingMixture>, DomainError> {
        let materials = self.liming_materials.list_materials()?;
        // The first mixture named in the file wins. One is all Tabla 12
        // gives; a second would need a way to choose between them, which
        // nothing has asked for.
        let Some(mixture_id) = materials.iter().find_map(|m| m.mixture_id.clone()) else {
            return Ok(None);
        };

        let mut components = Vec::new();
        for material in materials.iter().filter(|m| m.mixture_id.as_deref() == Some(mixture_id.as_str())) {
            let Some(share_pct) = material.mixture_share_pct.filter(|s| *s > 0.0) else {
                continue;
            };
            let Some(t_product_per_ha) = services::lime_material_dose_t_ha(t_ha * share_pct / 100.0, prnt_of(material))
            else {
                continue;
            };
            components.push(LimingDose {
                source_id: material.source_id.clone(),
                source_name: material.name.clone(),
                t_product_per_ha,
            });
        }

        Ok((!components.is_empty()).then_some(LimingMixture { mixture_id, components }))
    }

    /// Exchangeable Al as a percent of CICE, the figure the efficiency
    /// modifiers read acidity through.
    ///
    /// `None` when the sample has no Al test — untested is not zero, and
    /// the efficiency module reports the absence rather than assuming the
    /// site is clean. Cations reported in mg/kg are converted first, for
    /// the same reason [`Self::calculate_liming`] does: feeding CICE a
    /// mg/kg number inflates it by two orders of magnitude, which here
    /// would read as *no* aluminium problem on exactly the soils that have
    /// one.
    fn aluminium_saturation_pct(&self, soil_tests: &[SoilTest]) -> Option<f64> {
        let al_test = soil_tests.iter().find(|t| t.nutrient == Nutrient::Al)?;
        let cmolc = |nutrient: Nutrient| match soil_tests.iter().find(|t| t.nutrient == nutrient) {
            Some(test) => self.value_in(test, CMOLC_PER_KG).ok(),
            None => Some(0.0),
        };

        let al = self.value_in(al_test, CMOLC_PER_KG).ok()?;
        let cice = services::cation_exchange_capacity_effective(
            cmolc(Nutrient::H)?,
            al,
            cmolc(Nutrient::K)?,
            cmolc(Nutrient::Mg)?,
            cmolc(Nutrient::Ca)?,
        );
        (cice > 0.0).then(|| al / cice * 100.0)
    }

    /// Lime requirement, computed only when the sample has an Al³⁺ test
    /// (the workflow's "encalamiento si aplica"). K⁺/Mg²⁺/Ca²⁺/H⁺ enter as
    /// cmolc/kg charge equivalents, not as the kg/ha availability used
    /// elsewhere in the plan: the liming formulas sum exchange sites, and
    /// only a charge unit can be summed that way.
    ///
    /// A cation the sample doesn't report counts as zero — many labs omit
    /// H⁺ — but one reported in another unit is *converted*, never taken at
    /// face value. Feeding CICE a mg/kg number inflates it by two orders of
    /// magnitude, which pushes base saturation to ~100% and silently
    /// cancels the lime recommendation on exactly the acid soils that need it.
    fn calculate_liming(&self, soil_tests: &[SoilTest], cic_cmolc_kg: f64, region: &str) -> Result<Option<LimingRecommendation>, DomainError> {
        let Some(al_test) = soil_tests.iter().find(|t| t.nutrient == Nutrient::Al) else {
            return Ok(None);
        };
        let cmolc_value_of = |nutrient: Nutrient| match soil_tests.iter().find(|t| t.nutrient == nutrient) {
            Some(test) => self.value_in(test, CMOLC_PER_KG),
            None => Ok(0.0),
        };

        let al = self.value_in(al_test, CMOLC_PER_KG)?;
        let h = cmolc_value_of(Nutrient::H)?;
        let k = cmolc_value_of(Nutrient::K)?;
        let mg = cmolc_value_of(Nutrient::Mg)?;
        let ca = cmolc_value_of(Nutrient::Ca)?;

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

        let (material, mixture) = if recommended_t_ha > 0.0 {
            (self.best_liming_material_for(recommended_t_ha)?, self.liming_mixture_for(recommended_t_ha)?)
        } else {
            (None, None)
        };

        Ok(Some(LimingRecommendation {
            al_based_t_ha,
            base_saturation_based_t_ha,
            recommended_t_ha,
            current_base_saturation_pct,
            target_base_saturation_pct,
            material,
            mixture,
        }))
    }
}

/// A material's relative total neutralizing power, the rating both the
/// single-material pick and the mixture split divide by.
fn prnt_of(material: &crate::core::domain::LimingMaterial) -> f64 {
    let eq = services::neutralizing_value_pct(material.cao_pct, material.mgo_pct);
    services::prnt(eq, material.granulometric_efficiency_pct)
}

/// One nutrient's demand on both bases the reference table carries, at a
/// given yield goal. Either may be `None` where the source table prints a
/// dash.
struct DemandByBasis {
    extraction: Option<f64>,
    absorption: Option<f64>,
}

impl DemandByBasis {
    fn on(&self, mode: NutrientDemandMode) -> Option<f64> {
        match mode {
            NutrientDemandMode::Extraction => self.extraction,
            NutrientDemandMode::Absorption => self.absorption,
        }
    }
}

/// The "highest number wins" pick shared by the fertilizer-source and
/// liming-material heuristics.
///
/// `> 0.0` is false for `NaN`, so a catalog row with an unparseable
/// composition drops out here instead of ordering the comparison — which
/// is what made the old `partial_cmp(..).unwrap()` a panic waiting for a
/// bad CSV row. `total_cmp` then needs no fallible comparison at all.
fn highest_rated<'a, T>(rated: impl Iterator<Item = (&'a T, f64)>) -> Option<(&'a T, f64)> {
    rated
        .filter(|(_, rating)| *rating > 0.0)
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
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
        let mineralization_factor = climate
            .as_ref()
            .and_then(|c| services::mineralization_factor(c, &field_context.irrigation_system))
            .unwrap_or(services::BASELINE_MINERALIZATION_FACTOR);

        // Everything about the site that moves nutrient use efficiency,
        // gathered once. Each source is optional on its own terms: no
        // climatology means no temperature or water modifier, no Al test
        // means no acidity modifier, and neither absence is read as "the
        // site is fine" — `AdjustedEfficiency::assumptions` says so.
        let conditions = ScenarioConditions {
            ph: field_context.ph,
            texture: field_context.texture,
            irrigation: field_context.irrigation_system,
            mean_temp_c: climate.as_ref().and_then(|c| c.mean_temp_c),
            max_temp_c: climate.as_ref().and_then(|c| c.max_temp_c),
            moisture_index: climate.as_ref().and_then(|c| {
                let (precip, et0) = (c.annual_precip_mm()?, c.annual_et0_mm()?);
                (et0 > 0.0).then(|| precip / et0)
            }),
            aluminium_saturation_pct: self.aluminium_saturation_pct(&soil_tests),
        };

        // Loaded once per plan: the table is per profile, not per nutrient.
        let band_rules = self.efficiency_bands.band_rules()?;

        let mut warnings = Vec::new();
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
                    Some(test) => services::availability_kg_ha(self.value_in(test, MG_PER_KG)?, soil_weight),
                    None => 0.0,
                }
            };

            let demand = self.crop_demand(&scenario.crop_id, nutrient_id, &yield_target)?;

            let (efficiency_min, efficiency_max) = self.efficiency_rules.get_efficiency_range(
                &field_context.texture,
                &field_context.irrigation_system,
                nutrient_id,
            )?;
            // The reference range's midpoint is the *base*; the site's own
            // conditions move it from there. `SulfurForm::Unstated` because
            // no product has been chosen yet — see `SulfurForm`.
            let efficiency = efficiency::adjust(
                nutrient,
                (efficiency_min + efficiency_max) / 2.0,
                efficiency_max,
                &conditions,
                SulfurForm::Unstated,
                &band_rules,
            );
            let efficiency_used = efficiency.adjusted;

            let net_of = |demand_kg_ha: f64| services::net_requirement_kg_ha(demand_kg_ha, availability_kg_ha, efficiency_used);

            let requested = demand.as_ref().and_then(|d| d.on(scenario.demand_mode));
            let mut demand_mode_used = requested.map(|_| scenario.demand_mode);
            let mut demand_kg_ha = requested.unwrap_or(0.0);
            let mut net_requirement_kg_ha = net_of(demand_kg_ha);

            // The fallback runs in one direction only. Absorption is
            // always the larger coefficient, so this can raise a
            // recommendation and never lower one — going the other way
            // would let a plan quietly under-fertilize. It fires both when
            // extraction says no fertilizer is needed and when the table
            // has no extraction figure at all, which are the same
            // situation from the reader's side: nothing to act on.
            if scenario.demand_mode == NutrientDemandMode::Extraction && net_requirement_kg_ha <= 0.0 {
                if let Some(absorption_demand) = demand.as_ref().and_then(|d| d.absorption) {
                    let absorption_net = net_of(absorption_demand);
                    if absorption_net > 0.0 {
                        demand_kg_ha = absorption_demand;
                        demand_mode_used = Some(NutrientDemandMode::Absorption);
                        net_requirement_kg_ha = absorption_net;
                        warnings.push(PlanWarning::FallbackToAbsorption {
                            nutrient,
                            net_requirement_kg_ha: absorption_net,
                        });
                    }
                }
            }

            if demand_mode_used.is_none() {
                warnings.push(PlanWarning::NoRemovalCoefficient { nutrient });
            }

            // Best-effort throughout: a nutrient with no thresholds, no
            // test, or a reading that can't be moved into the thresholds'
            // unit is reported as "unclassified" rather than failing the
            // plan. Saying nothing is honest; saying "high" off a
            // mismatched unit is not.
            let soil_status = test.and_then(|test| {
                let level = self
                    .critical_levels
                    .get_critical_level(nutrient_id, &field_context.texture, &field_context.region, Some(&test.method))
                    .ok()?;
                Some(level.classify(self.value_in(test, &level.unit).ok()?))
            });

            let dose = if net_requirement_kg_ha > 0.0 {
                self.best_source_for(nutrient, net_requirement_kg_ha)?
            } else {
                None
            };

            nutrient_results.push(NutrientPlanEntry {
                nutrient,
                availability_kg_ha,
                demand_kg_ha,
                demand_mode_used,
                efficiency_used,
                efficiency,
                net_requirement_kg_ha,
                soil_status,
                dose,
            });
        }

        let micronutrients = self.correct_micronutrients(&soil_tests, &field_context, soil_weight)?;
        let liming = self.calculate_liming(&soil_tests, field_context.cec_cmolc_kg, &field_context.region)?;

        Ok(FertilityPlan {
            field_id: scenario.field_id,
            sample_id: scenario.sample_id,
            crop_id: scenario.crop_id,
            yield_target,
            demand_mode: scenario.demand_mode,
            nutrient_results,
            micronutrients,
            liming,
            warnings,
            mineralization_factor,
            climate,
            conditions,
            band_rules,
            area_ha: field_context.area_ha,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_highest_rating_wins_and_a_nan_never_does() {
        let catalog = [("urea", 46.0), ("broken", f64::NAN), ("map", 26.6), ("filler", 0.0)];
        let rated = catalog.iter().map(|(name, rating)| (name, *rating));

        let best = highest_rated(rated).expect("a usable row exists");

        assert_eq!(*best.0, "urea");
        assert_eq!(best.1, 46.0);
    }

    #[test]
    fn a_catalog_with_nothing_usable_yields_no_dose() {
        let catalog = [("broken", f64::NAN), ("filler", 0.0), ("negative", -3.0)];

        assert!(highest_rated(catalog.iter().map(|(name, rating)| (name, *rating))).is_none());
    }
}
