//! Turns a computed [`FertilityPlan`] into products to buy.
//!
//! A second use case rather than a wider
//! [`CalculateFertilityPlan`](crate::core::application::CalculateFertilityPlan): the
//! balance ("how much N does this crop still need") and the formulation
//! ("which bag, how many, and how many bags") are different questions with
//! different inputs, and the first has been answered and tested for
//! thirteen sessions. Nothing in the existing plan path changes.

use crate::core::domain::formulation::{
    build_program, build_target_grade, candidate_from_source, rank_candidates, CompositeCandidate,
    BlendSearchStrategy, FertilizationStrategy, FertilizerRecommendationReport, GradeNutrient,
    NutrientRequirement, ScenarioSummary,
    REPORTED_CANDIDATES,
};
use crate::core::domain::{
    efficiency, AdjustedEfficiency, BalanceRow, DomainError, FertilityPlan, FertilizationBalance, FertilizerProgram,
    Nutrient, PlanWarning, SoilStatus, SulfurForm,
};
use crate::core::ports::{ConversionFactorsRepository, FertilizerSourceRepository, RecommendFertilizerProgramPort};

/// The knobs a formulation needs and the balance does not.
#[derive(Debug, Clone)]
pub struct FormulationRequest {
    /// Whether compound products may be used, or the program must be built
    /// from straights alone.
    pub strategy: FertilizationStrategy,
    /// Hectares the recommendation is bought for. 1.0 reports per-hectare
    /// figures unchanged.
    pub total_area_ha: f64,
    /// 40, 50, or whatever the local trade sells.
    pub bag_weight_kg: f64,
    /// Reported, never used to decide anything — the report has to say
    /// which catalog answered it.
    pub profile: String,
    /// Whether a nutrient's requirement may be split across two products.
    /// `SinglePick` reproduces the pre-split behaviour exactly, and is the
    /// baseline the split pass is measured against.
    pub blend_search: BlendSearchStrategy,
}

impl Default for FormulationRequest {
    fn default() -> Self {
        Self {
            strategy: FertilizationStrategy::CompositePlusSimple,
            total_area_ha: 1.0,
            bag_weight_kg: 50.0,
            profile: String::new(),
            blend_search: BlendSearchStrategy::default(),
        }
    }
}

/// The product half of a plan: turns nutrient kilograms into bags.
pub struct RecommendFertilizerProgram {
    fertilizer_sources: Box<dyn FertilizerSourceRepository>,
    conversion_factors: Box<dyn ConversionFactorsRepository>,
}

impl RecommendFertilizerProgram {
    /// # Arguments
    /// * `fertilizer_sources` — the catalog to pick products from.
    /// * `conversion_factors` — moves elemental P and K onto the oxide
    ///   basis the commercial grades are stated in.
    ///
    /// # Returns
    /// The use case, ready to formulate.
    #[must_use]
    pub fn new(
        fertilizer_sources: Box<dyn FertilizerSourceRepository>,
        conversion_factors: Box<dyn ConversionFactorsRepository>,
    ) -> Self {
        Self { fertilizer_sources, conversion_factors }
    }

    /// The elemental -> oxide factor for each grade nutrient, read once
    /// from `conversion_factors.toml`.
    ///
    /// The domain must not hardcode 2.291 and 1.205: they are unit science
    /// and they live in a reference table like every other constant. A
    /// factor the table cannot supply comes back `None`, and
    /// [`candidate_from_source`] then drops that nutrient rather than
    /// passing an elemental percentage off as part of a commercial grade.
    fn oxide_factor(&self, nutrient: GradeNutrient) -> Option<f64> {
        let (from, to) = nutrient.oxide_conversion()?;
        self.conversion_factors.convert(from, to, from, 1.0).ok()
    }

    /// Net requirements moved onto the visible commercial basis.
    ///
    /// The plan states P and K elementally, because that is what the soil
    /// tests, the removal coefficients and the critical levels are in. A
    /// grade is not: 96.18 kg/ha of P2O5 is 41.98 kg/ha of P, and comparing
    /// the wrong one against a bag understates phosphorus by a factor of
    /// 2.29.
    fn requirements_on_visible_basis(&self, plan: &FertilityPlan) -> Vec<NutrientRequirement> {
        GradeNutrient::ALL
            .into_iter()
            .filter_map(|nutrient| {
                let entry = plan
                    .nutrient_results
                    .iter()
                    .find(|entry| entry.nutrient == nutrient.elemental())
                    .filter(|entry| entry.net_requirement_kg_ha > 0.0)?;
                let kg_ha = match nutrient.oxide_conversion() {
                    Some(_) => entry.net_requirement_kg_ha * self.oxide_factor(nutrient)?,
                    None => entry.net_requirement_kg_ha,
                };
                Some(NutrientRequirement { nutrient, kg_ha })
            })
            .collect()
    }

    fn catalog(&self) -> Result<Vec<CompositeCandidate>, DomainError> {
        Ok(self
            .fertilizer_sources
            .list_sources()?
            .iter()
            .map(|source| candidate_from_source(source, |nutrient| self.oxide_factor(nutrient)))
            .collect())
    }
}

impl RecommendFertilizerProgramPort for RecommendFertilizerProgram {
    fn recommend(
        &self,
        plan: &FertilityPlan,
        request: &FormulationRequest,
    ) -> Result<FertilizerRecommendationReport, DomainError> {
        // The trust boundary: both divide, and a zero, a negative or a NaN
        // reaching the domain is an `inf kg/ha` recommendation.
        let positive = |value: f64| value.is_finite() && value > 0.0;
        if !positive(request.total_area_ha) {
            return Err(DomainError::InvalidInput(format!("total area must be positive, got {}", request.total_area_ha)));
        }
        if !positive(request.bag_weight_kg) {
            return Err(DomainError::InvalidInput(format!("bag weight must be positive, got {}", request.bag_weight_kg)));
        }

        let requirements = self.requirements_on_visible_basis(plan);
        let catalog = self.catalog()?;
        let ratio = build_target_grade(&requirements, &catalog);
        let required_nutrients: Vec<GradeNutrient> = requirements.iter().map(|r| r.nutrient).collect();

        let mut candidates = match &ratio {
            Some(ratio) => rank_candidates(&ratio.target, &required_nutrients, &catalog),
            None => Vec::new(),
        };
        candidates.truncate(REPORTED_CANDIDATES);

        let target = ratio.as_ref().map(|ratio| ratio.target);
        let program = |strategy| {
            build_program(
                strategy,
                &requirements,
                target.as_ref(),
                &catalog,
                request.total_area_ha,
                request.bag_weight_kg,
                request.blend_search,
            )
        };
        let chosen = program(request.strategy);
        let alternative = program(request.strategy.other());

        let elemental_requirements = plan
            .nutrient_results
            .iter()
            .filter(|entry| entry.net_requirement_kg_ha > 0.0)
            .map(|entry| (entry.nutrient, entry.net_requirement_kg_ha))
            .collect();
        let efficiency: Vec<AdjustedEfficiency> = plan
            .nutrient_results
            .iter()
            .filter(|entry| entry.net_requirement_kg_ha > 0.0)
            .map(|entry| entry.efficiency.clone())
            .collect();

        let balance = balance_of(plan);
        let mut assumptions = assumptions(&chosen, &catalog, plan);
        assumptions.extend(efficiency.iter().flat_map(|adjusted| adjusted.assumptions.clone()));
        if let Some(warning) = elemental_sulfur_warning(&chosen, plan) {
            assumptions.push(warning);
        }
        // "No climatology for this lot" is one fact about the lot, and every
        // nutrient reports it. Deduplicated in order, so the reader sees
        // each once and still sees them in the order they were raised.
        let mut seen = std::collections::HashSet::new();
        assumptions.retain(|assumption| seen.insert(assumption.clone()));

        Ok(FertilizerRecommendationReport {
            scenario: ScenarioSummary {
                field_id: plan.field_id.clone(),
                sample_id: plan.sample_id.clone(),
                crop_id: plan.crop_id.clone(),
                yield_value: plan.yield_target.value,
                yield_unit: plan.yield_target.unit.clone(),
                total_area_ha: request.total_area_ha,
                bag_weight_kg: request.bag_weight_kg,
                strategy: request.strategy,
                profile: request.profile.clone(),
            },
            requirements,
            elemental_requirements,
            efficiency,
            balance,
            ratio,
            candidates,
            chosen,
            alternative,
            assumptions,
        })
    }
}

/// The plan's own figures, restated as report data.
///
/// A copy rather than a reference to the plan: the report is what gets
/// exported, and an exported file has to stand on its own away from the
/// process that produced it.
fn balance_of(plan: &FertilityPlan) -> FertilizationBalance {
    FertilizationBalance {
        rows: plan
            .nutrient_results
            .iter()
            .map(|entry| BalanceRow {
                nutrient: entry.nutrient,
                availability_kg_ha: entry.availability_kg_ha,
                demand_kg_ha: entry.demand_kg_ha,
                demand_basis: entry.demand_mode_used.map(|mode| mode.to_string()),
                efficiency_used: entry.efficiency_used,
                net_requirement_kg_ha: entry.net_requirement_kg_ha,
                soil_status: entry.soil_status.map(|status| {
                    match status {
                        SoilStatus::Low => "low",
                        SoilStatus::Medium => "medium",
                        SoilStatus::High => "high",
                    }
                    .to_string()
                }),
            })
            .collect(),
        liming_t_ha: plan.liming.as_ref().map(|l| l.recommended_t_ha),
        liming_material: plan
            .liming
            .as_ref()
            .and_then(|l| l.material.as_ref())
            .map(|dose| format!("{} · {:.2} t/ha", dose.source_name, dose.t_product_per_ha)),
        micronutrients: plan
            .micronutrients
            .iter()
            .map(|micro| {
                (
                    micro.nutrient,
                    format!("{:.2} {}", micro.soil_value, micro.unit),
                    micro.dose.as_ref().map(|d| format!("{} · {:.1} kg/ha", d.source_name, d.kg_product_per_ha)),
                )
            })
            .collect(),
        warnings: plan
            .warnings
            .iter()
            .map(|warning| match warning {
                PlanWarning::FallbackToAbsorption { nutrient, net_requirement_kg_ha } => format!(
                    "{nutrient}: extraction asked for no fertilizer; the {net_requirement_kg_ha:.1} kg/ha shown is \
                     on the absorption basis (total plant uptake, most of which returns in the residues)."
                ),
                PlanWarning::NoRemovalCoefficient { nutrient } => format!(
                    "{nutrient}: the reference table has no coefficient for this crop on either basis — its demand \
                     is unknown, not zero, and no dose was computed."
                ),
            })
            .collect(),
        mineralization_factor: plan.mineralization_factor,
        climate_enriched: plan.climate.is_some(),
    }
}

/// Closes the loop the balance could not: efficiency is what sizes the
/// sulfur requirement, but the sulfur *product* is only chosen afterwards,
/// so the balance ran on the sulfate assumption. If the blend turned out to
/// be elemental S, say so here with the correction the site's temperature
/// implies.
///
/// Reads the catalog's `form` column. This used to match the word
/// "elemental" in the product *name*, which worked only for the shipped
/// Spanish catalog and only for sulfur; a row whose form is `unknown` now
/// gets no warning, which is honest rather than a guess at its chemistry.
fn elemental_sulfur_warning(program: &FertilizerProgram, plan: &FertilityPlan) -> Option<String> {
    let sulfur = plan
        .nutrient_results
        .iter()
        .find(|entry| entry.nutrient == Nutrient::S && entry.net_requirement_kg_ha > 0.0)?;
    let line = program
        .lines
        .iter()
        .find(|line| line.grade.get(GradeNutrient::S) > 0.0 && line.form.needs_soil_transformation())?;

    let sulfate_efficiency = sulfur.efficiency.adjusted;
    let elemental = efficiency::adjust(
        Nutrient::S,
        sulfur.efficiency.base,
        sulfur.efficiency.ceiling,
        &plan.conditions,
        SulfurForm::Elemental,
        &plan.band_rules,
    );
    Some(format!(
        "The sulfur requirement was sized assuming a sulfate source ({:.0}% efficiency), and the blend uses {}. \
         Elemental S has to be microbially oxidized first, which at this site is worth about {:.0}% — so its \
         {:.0} kg/ha is closer to a {:.0} kg/ha equivalent, or split the sulfur across a sulfate carrier.",
        sulfate_efficiency * 100.0,
        line.source_name,
        elemental.adjusted * 100.0,
        line.kg_per_ha,
        line.kg_per_ha * sulfate_efficiency / elemental.adjusted.max(f64::EPSILON),
    ))
}

/// What the reader has to know to act on the numbers above them.
///
/// Written as consequences, not as implementation notes: "phosphorus is
/// short by 12 kg/ha" is actionable, "the greedy pass terminated" is not.
fn assumptions(
    program: &crate::core::domain::formulation::FertilizerProgram,
    catalog: &[CompositeCandidate],
    plan: &FertilityPlan,
) -> Vec<String> {
    let mut assumptions = vec![
        "The target ratio rounds each requirement to the nearest 10 kg/ha, with a floor of 10 so a small but real \
         requirement keeps its place in the ratio."
            .to_string(),
        "The compound is dosed on the first requirement it satisfies, so it never over-applies; the straights cover \
         what is left. The dose that each nutrient alone would demand is listed with the recommendation."
            .to_string(),
        "The straights are chosen by enumerating every order the requirements can be covered in, against the three \
         best products for each, and keeping the blend that leaves nothing short, wastes the least nutrient and \
         weighs the least — in that order of priority. Cost is not among them: the catalog carries no prices."
            .to_string(),
        "Each straight covers its nutrient to exactly 100%, so a blend that would be lighter by splitting one \
         requirement across two products is outside the search."
            .to_string(),
        "Ca and Mg are outside the commercial grade heuristic. They are planned by liming and by corrective \
         application, both of which are reported separately."
            .to_string(),
    ];

    if !catalog.iter().any(super::super::domain::formulation::CompositeCandidate::is_compound) {
        assumptions.push(
            "This profile's catalog carries no compound product at all, so the compound strategy falls back to \
             straights."
                .to_string(),
        );
    }

    for remainder in program.uncovered() {
        assumptions.push(format!(
            "{} is short by {:.1} kg/ha: nothing in this catalog carries it, or what does was already exhausted.",
            remainder.nutrient, remainder.remaining_kg_ha
        ));
    }

    for entry in &plan.nutrient_results {
        if entry.net_requirement_kg_ha > 0.0 && entry.nutrient == Nutrient::S {
            assumptions.push(
                "Sulfur entered the grade heuristic because this plan needs it; a plan without a sulfur requirement \
                 compares products on N-P2O5-K2O alone."
                    .to_string(),
            );
        }
    }

    assumptions
}
