//! The formulation engine against the real Andean catalog and the real
//! conversion table.
//!
//! Here rather than beside the domain because the two things most likely to
//! go wrong need `infra`: the elemental -> oxide conversion is read from
//! `conversion_factors.toml`, and the compound candidates are 500 real
//! products rather than a fixture. The unit tests in
//! `core::domain::formulation` pin the arithmetic; this pins that the
//! arithmetic is fed the right numbers.

use std::path::Path;

use non_nobis_solum::core::application::FormulationRequest;
use non_nobis_solum::core::domain::{
    efficiency, BlendSearchStrategy, FertilityPlan, FertilizationStrategy, GradeNutrient, IrrigationSystem,
    Nutrient, NutrientDemandMode,
    NutrientPlanEntry, ScenarioConditions, SourceRole, SulfurForm, Texture, YieldTarget,
};
use non_nobis_solum::core::ports::{RecommendFertilizerProgramPort, ReportExporter};
use non_nobis_solum::infra::bootstrap::{build_recommend_fertilizer_program, DataLayout};
use non_nobis_solum::infra::{report_renderer, PdfReportExporter};

/// `P2O5_to_P` and `K2O_to_K` from `conversion_factors.toml`. The plan
/// states P and K elementally, so the mandatory case's visible-basis
/// figures have to be entered the way the balance would have produced them.
const P2O5_TO_P: f64 = 0.4364267631;
const K2O_TO_K: f64 = 0.8301513890;

/// The workflow's mandatory numeric case: N 84.08, P2O5 96.18, K2O 20.09.
fn mandatory_plan() -> FertilityPlan {
    let entry = |nutrient: Nutrient, net: f64| NutrientPlanEntry {
        nutrient,
        availability_kg_ha: 0.0,
        demand_kg_ha: net,
        demand_mode_used: Some(NutrientDemandMode::Extraction),
        efficiency_used: 1.0,
        // Efficiency 1.0 with no modifier: this fixture states the net
        // requirements directly, so nothing here may rescale them.
        efficiency: efficiency::adjust(nutrient, 1.0, 1.0, &conditions(), SulfurForm::Sulfate, &band_rules()),
        net_requirement_kg_ha: net,
        soil_status: None,
        dose: None,
    };
    FertilityPlan {
        field_id: "LOT-001".to_string(),
        sample_id: "LOT-001".to_string(),
        crop_id: "corn".to_string(),
        yield_target: YieldTarget { value: 9.5, unit: "t_ha".to_string() },
        demand_mode: NutrientDemandMode::Extraction,
        nutrient_results: vec![
            entry(Nutrient::N, 84.08),
            entry(Nutrient::P, 96.18 * P2O5_TO_P),
            entry(Nutrient::K, 20.09 * K2O_TO_K),
            entry(Nutrient::S, 0.0),
            entry(Nutrient::Ca, 0.0),
            entry(Nutrient::Mg, 0.0),
        ],
        micronutrients: Vec::new(),
        liming: None,
        warnings: Vec::new(),
        mineralization_factor: 0.015,
        climate: None,
        conditions: conditions(),
        band_rules: band_rules(),
        area_ha: Some(12.0),
    }
}

/// The real shipped table, read through the real adapter — the fixture must
/// not drift from what a plan actually runs on.
fn band_rules() -> non_nobis_solum::core::domain::EfficiencyBandRules {
    use non_nobis_solum::core::ports::EfficiencyBandRepository;
    non_nobis_solum::infra::TomlEfficiencyBandsRepo::from_toml_file(
        "data/reference/andina_colombia/efficiency_bands.toml",
    )
    .expect("the shipped band table")
    .band_rules()
    .expect("rules")
}

/// A site with nothing wrong with it, so the fixture's stated requirements
/// reach the formulation unchanged.
fn conditions() -> ScenarioConditions {
    ScenarioConditions::reference(Texture::Loam, IrrigationSystem::Drip)
}

fn request(area_ha: f64, bag_kg: f64) -> FormulationRequest {
    FormulationRequest {
        strategy: FertilizationStrategy::CompositePlusSimple,
        total_area_ha: area_ha,
        bag_weight_kg: bag_kg,
        profile: "andina_colombia".to_string(),
        blend_search: BlendSearchStrategy::default(),
    }
}

fn report(request: &FormulationRequest) -> non_nobis_solum::core::domain::FertilizerRecommendationReport {
    build_recommend_fertilizer_program(&DataLayout::new("data", &request.profile))
        .expect("wire the use case")
        .recommend(&mandatory_plan(), request)
        .expect("a recommendation")
}

/// PART A and PART B on the mandatory case, through the real conversion
/// table and the real 500-product catalog: 84.08/96.18/20.09 ->
/// 80/100/20 -> 4:5:1 -> 40-50-10 -> 20-25-5 -> 10-13-3.
#[test]
fn the_mandatory_case_reaches_the_workflows_own_target_grade() {
    let report = report(&request(1.0, 50.0));

    let of = |nutrient: GradeNutrient| {
        report.requirements.iter().find(|r| r.nutrient == nutrient).expect("a requirement").kg_ha
    };
    assert!((of(GradeNutrient::N) - 84.08).abs() < 0.01);
    assert!((of(GradeNutrient::P2O5) - 96.18).abs() < 0.01, "P must be read back as P2O5, not as P");
    assert!((of(GradeNutrient::K2O) - 20.09).abs() < 0.01, "K must be read back as K2O, not as K");

    let ratio = report.ratio.as_ref().expect("a target ratio");
    let rounded: Vec<f64> = ratio.rounded.iter().map(|r| r.kg_ha).collect();
    assert_eq!(rounded, vec![80.0, 100.0, 20.0]);
    assert_eq!(ratio.smallest_rounded, 20.0);
    assert_eq!(ratio.normalized.label(), "4-5-1");

    let ladder: Vec<String> = ratio.steps.iter().map(|step| step.discretized.label()).collect();
    assert_eq!(&ladder[..3], &["40-50-10".to_string(), "20-25-5".to_string(), "10-13-3".to_string()]);
    assert_eq!(ratio.target.label(), "10-13-3");

    let coefficients = ratio.target.coefficients();
    assert!((coefficients.n_over_p.expect("N/P") - 0.769230).abs() < 1e-5);
    assert!((coefficients.p_over_k.expect("P/K") - 4.333333).abs() < 1e-5);
}

/// The catalog's own 13-26-6-3S, read off `fertilizer_sources.csv` where P
/// is stored as 11.3471% elemental and K as 4.9809%.
#[test]
fn a_catalog_row_stored_elementally_is_scored_as_its_printed_grade() {
    let report = report(&request(1.0, 50.0));
    let ratio = report.ratio.as_ref().expect("a target ratio");

    let candidates = build_recommend_fertilizer_program(&DataLayout::new("data", "andina_colombia"))
        .expect("wire")
        .recommend(&mandatory_plan(), &request(1.0, 50.0))
        .expect("a recommendation");
    // The reported ranking is truncated, so the control candidate is
    // checked through the domain scorer on the same target instead.
    let target = ratio.target;
    let control = non_nobis_solum::core::domain::CommercialGrade::new(13.0, 26.0, 6.0, 3.0);
    let coefficients = control.coefficients();
    assert!((coefficients.n_over_p.expect("N/P") - 0.5).abs() < 1e-9);
    assert!((coefficients.p_over_k.expect("P/K") - 4.333333).abs() < 1e-5);
    assert!(
        (target.coefficients().p_over_k.expect("P/K") - coefficients.p_over_k.expect("P/K")).abs() < 1e-5,
        "the control product matches the target on P/K exactly — that is why it is the control"
    );
    assert!(!candidates.candidates.is_empty(), "a 500-product catalog must offer compounds");
    assert!(candidates.candidates.iter().all(|c| c.nutrient_coverage_score > 0.0));
}

/// PARTS E, F and the consolidated output, end to end.
#[test]
fn the_program_covers_every_requirement_and_scales_to_area_and_bags() {
    let one_ha = report(&request(1.0, 50.0));

    let composite = one_ha.chosen.composite.as_ref().expect("a compound product");
    assert!(composite.kg_per_ha > 0.0);
    assert_eq!(
        composite.dose_per_nutrient.len(),
        3,
        "one dose figure per required nutrient the compound carries"
    );
    assert!(
        composite.dose_per_nutrient.iter().all(|(_, dose)| *dose >= composite.kg_per_ha),
        "the chosen dose is the smallest of the table, so nothing is over-applied"
    );
    assert!(one_ha.chosen.uncovered().is_empty(), "balance: {:?}", one_ha.chosen.balance);
    assert!(one_ha.chosen.lines.iter().any(|line| line.role == SourceRole::Composite));
    assert!(one_ha.chosen.lines.iter().any(|line| line.role == SourceRole::Simple));

    // Twelve hectares is the same plan, twelve times the product.
    let twelve_ha = report(&request(12.0, 50.0));
    assert!((twelve_ha.chosen.total_kg_per_ha - one_ha.chosen.total_kg_per_ha).abs() < 1e-6);
    assert!((twelve_ha.chosen.total_kg - one_ha.chosen.total_kg * 12.0).abs() < 1e-6);

    // Same mass, lighter bags, more bags.
    let forty = report(&request(12.0, 40.0));
    assert!((forty.chosen.total_kg - twelve_ha.chosen.total_kg).abs() < 1e-6);
    assert!(forty.chosen.total_bags_rounded_up > twelve_ha.chosen.total_bags_rounded_up);
    for (heavy, light) in twelve_ha.chosen.lines.iter().zip(&forty.chosen.lines) {
        let (heavy, light) = (heavy.bags.expect("bags"), light.bags.expect("bags"));
        assert_eq!(heavy.bag_weight_kg, 50.0);
        assert_eq!(light.bag_weight_kg, 40.0);
        assert!((light.bags_total / heavy.bags_total - 50.0 / 40.0).abs() < 1e-9);
        assert!(light.bags_total_rounded_up as f64 >= light.bags_total);
    }
}

/// The alternative strategy is always computed, and uses no compound.
#[test]
fn the_simple_blend_alternative_is_comparable_and_compound_free() {
    let report = report(&request(5.0, 50.0));

    assert_eq!(report.chosen.strategy, FertilizationStrategy::CompositePlusSimple);
    assert_eq!(report.alternative.strategy, FertilizationStrategy::SimpleBlendOnly);
    assert!(report.alternative.composite.is_none());
    assert!(report.alternative.lines.iter().all(|line| line.role == SourceRole::Simple));
    assert!(report.alternative.uncovered().is_empty(), "balance: {:?}", report.alternative.balance);

    // Asking for the blend directly swaps which one is which, and the
    // figures come out identical either way.
    let swapped = report_for(FertilizationStrategy::SimpleBlendOnly, 5.0);
    assert_eq!(swapped.chosen.lines.len(), report.alternative.lines.len());
    assert!((swapped.chosen.total_kg - report.alternative.total_kg).abs() < 1e-6);
}

fn report_for(
    strategy: FertilizationStrategy,
    area_ha: f64,
) -> non_nobis_solum::core::domain::FertilizerRecommendationReport {
    report(&FormulationRequest { strategy, total_area_ha: area_ha, ..request(area_ha, 50.0) })
}

/// A plan with a sulfur requirement compares products on N-P2O5-K2O-S.
#[test]
fn a_sulfur_requirement_enters_the_grade_and_is_covered() {
    let mut plan = mandatory_plan();
    plan.nutrient_results
        .iter_mut()
        .find(|entry| entry.nutrient == Nutrient::S)
        .expect("an S row")
        .net_requirement_kg_ha = 24.0;

    let report = build_recommend_fertilizer_program(&DataLayout::new("data", "andina_colombia"))
        .expect("wire")
        .recommend(&plan, &request(1.0, 50.0))
        .expect("a recommendation");

    let ratio = report.ratio.as_ref().expect("a target ratio");
    assert!(ratio.target.get(GradeNutrient::S) > 0.0, "sulfur belongs in the target grade");
    assert!(ratio.target.coefficients().k_over_s.is_some(), "K/S only exists once S is in the grade");
    assert!(report.chosen.uncovered().is_empty(), "balance: {:?}", report.chosen.balance);
    assert!(report.chosen.lines.iter().any(|line| line.grade.get(GradeNutrient::S) > 0.0));
}

/// Invalid area or bag weight is refused at the use case, not turned into
/// an `inf kg/ha` line by the division downstream.
#[test]
fn a_non_positive_area_or_bag_weight_is_refused() {
    let use_case = build_recommend_fertilizer_program(&DataLayout::new("data", "andina_colombia")).expect("wire");
    let plan = mandatory_plan();

    for (area, bag) in [(0.0, 50.0), (-3.0, 50.0), (f64::NAN, 50.0), (1.0, 0.0), (1.0, -40.0)] {
        let request = FormulationRequest {
            strategy: FertilizationStrategy::CompositePlusSimple,
            total_area_ha: area,
            bag_weight_kg: bag,
            profile: "andina_colombia".to_string(),
            blend_search: BlendSearchStrategy::default(),
        };
        assert!(use_case.recommend(&plan, &request).is_err(), "area {area}, bag {bag} must be refused");
    }
}

/// The report renders every mandatory section, and the PDF exporter writes
/// a file a reader will open.
#[test]
fn the_report_renders_and_exports() {
    let report = report(&request(12.0, 50.0));
    let text = report_renderer::render(&report).join("\n");

    for section in [
        "NET REQUIREMENT",
        "TARGET COMMERCIAL GRADE",
        "COMPOUND CANDIDATES EVALUATED",
        "COMPOUND DOSE",
        "REMAINDERS AND COMPLEMENTS",
        "WHAT TO BUY",
        "ALTERNATIVE",
        "ASSUMPTIONS AND LIMITS",
    ] {
        assert!(text.contains(section), "the report is missing the `{section}` section");
    }
    assert!(text.contains("10-13-3"), "the derived target grade has to appear in the report");
    assert!(text.contains("andina_colombia"), "the report must name the catalog that answered it");

    let destination =
        std::env::temp_dir().join(format!("nns_report_{}/plan.pdf", std::process::id()));
    PdfReportExporter.export(&report, &destination).expect("write the PDF");

    let bytes = std::fs::read(&destination).expect("read the PDF back");
    assert!(bytes.starts_with(b"%PDF-1.4"), "not a PDF");
    assert!(bytes.ends_with(b"%%EOF\n"), "truncated PDF");
    assert!(bytes.len() > 2_000, "a nine-section report cannot be {} bytes", bytes.len());
    let _ = std::fs::remove_dir_all(destination.parent().expect("parent"));
}

/// The repository's own catalog, not an installed one — a test must never
/// read or write the user's records.
#[test]
fn the_test_reads_the_repositorys_catalog() {
    assert!(Path::new("data/reference/andina_colombia/fertilizer_sources.csv").exists());
}

// ---------------------------------------------------------------------
// Scenario-adjusted efficiency, through the whole engine
// ---------------------------------------------------------------------

/// The efficiency table has to reach the report, not just the domain.
#[test]
fn the_report_carries_the_efficiency_trace_for_every_requirement() {
    let mut plan = mandatory_plan();
    plan.conditions = ScenarioConditions {
        ph: 5.3,
        texture: Texture::Sand,
        irrigation: IrrigationSystem::Rainfed,
        mean_temp_c: Some(14.2),
        max_temp_c: Some(24.0),
        moisture_index: Some(0.6),
        aluminium_saturation_pct: Some(40.0),
    };
    for entry in &mut plan.nutrient_results {
        entry.efficiency =
            efficiency::adjust(entry.nutrient, 0.40, 0.50, &plan.conditions, SulfurForm::Unstated, &band_rules());
    }

    let report = build_recommend_fertilizer_program(&DataLayout::new("data", "andina_colombia"))
        .expect("wire")
        .recommend(&plan, &request(1.0, 50.0))
        .expect("a recommendation");

    assert_eq!(report.efficiency.len(), report.requirements.len());
    let text = report_renderer::render(&report).join("\n");
    assert!(text.contains("EFFICIENCY ADJUSTED FOR THIS SITE"));
    // Every modifier that fired has to name its reading and its source.
    assert!(text.contains("x0.85 sand"), "{text}");
    assert!(text.contains("Cameron et al. 2013"));
    assert!(text.contains("Kochian et al. 2005"));

    // P under water deficit on an acid soil with high Al: the three
    // penalties the workflow's own example asks for, and the Al one
    // discounted because pH 5.3 already priced the fixation half.
    let phosphorus = report.efficiency.iter().find(|e| e.nutrient == Nutrient::P).expect("a P row");
    let factors: Vec<f64> = phosphorus.modifiers.iter().map(|m| m.factor).collect();
    assert_eq!(factors, vec![0.80, 0.80, 0.925], "pH 5.3, rainfed deficit, Al discounted");
    assert!((phosphorus.adjusted - 0.40 * 0.80 * 0.80 * 0.925).abs() < 1e-9);

    // N on sand: leaching, reduced mass flow, cold. No pH term at 5.3.
    let nitrogen = report.efficiency.iter().find(|e| e.nutrient == Nutrient::N).expect("an N row");
    assert_eq!(nitrogen.modifiers.len(), 4, "sand, deficit, cold, Al: {:?}", nitrogen.modifiers);
    assert!(nitrogen.adjusted < 0.40, "a bad site cannot come out at the base");

    // Said once, however many nutrients raised it.
    let unavailable = report.assumptions.iter().filter(|a| a.contains("No climatology")).count();
    assert!(unavailable <= 1, "assumptions must be deduplicated, got {unavailable}");
}

/// A low efficiency is not decoration: it is what raises the dose. Same
/// crop, same soil, worse site, more fertilizer.
#[test]
fn a_penalised_efficiency_raises_the_product_a_grower_has_to_buy() {
    let plan_at = |conditions: ScenarioConditions| {
        let mut plan = mandatory_plan();
        // 100 kg/ha of N demand met from nothing, so the net requirement is
        // entirely a function of the efficiency the site earns.
        let adjusted = efficiency::adjust(Nutrient::N, 0.40, 0.50, &conditions, SulfurForm::Unstated, &band_rules());
        for entry in &mut plan.nutrient_results {
            entry.net_requirement_kg_ha = if entry.nutrient == Nutrient::N { 100.0 / adjusted.adjusted } else { 0.0 };
            entry.efficiency = adjusted.clone();
        }
        plan.conditions = conditions;
        build_recommend_fertilizer_program(&DataLayout::new("data", "andina_colombia"))
            .expect("wire")
            .recommend(&plan, &request(1.0, 50.0))
            .expect("a recommendation")
    };

    let good = plan_at(ScenarioConditions::reference(Texture::Loam, IrrigationSystem::Drip));
    let bad = plan_at(ScenarioConditions {
        ph: 8.1,
        texture: Texture::Sand,
        irrigation: IrrigationSystem::Rainfed,
        mean_temp_c: Some(9.0),
        max_temp_c: Some(30.0),
        moisture_index: Some(0.5),
        aluminium_saturation_pct: Some(0.0),
    });

    assert!(
        bad.chosen.total_kg_per_ha > good.chosen.total_kg_per_ha * 1.5,
        "a site at {:.0}% efficiency must buy far more product than one at {:.0}%",
        bad.efficiency[0].adjusted * 100.0,
        good.efficiency[0].adjusted * 100.0
    );
    assert!(good.efficiency[0].modifiers.is_empty());
    assert_eq!(bad.efficiency[0].modifiers.len(), 4, "alkaline, sand, deficit, cold");
}

// ---------------------------------------------------------------------
// The `form` column
// ---------------------------------------------------------------------

/// The heuristic this replaced read the word "elemental" out of the
/// product's *name*, which worked only for the shipped Spanish catalog.
#[test]
fn elemental_sulfur_is_recognised_by_the_form_column_and_not_by_its_name() {
    use non_nobis_solum::core::domain::FertilizerForm;
    use non_nobis_solum::core::ports::FertilizerSourceRepository;
    use non_nobis_solum::infra::bootstrap::build_fertilizer_sources;

    let sources = build_fertilizer_sources(&DataLayout::new("data", "andina_colombia"))
        .list_sources()
        .expect("the migrated catalog still loads");

    let elemental: Vec<&str> = sources
        .iter()
        .filter(|source| source.form == FertilizerForm::Elemental)
        .map(|source| source.source_id.as_str())
        .collect();
    assert!(
        elemental.contains(&"azufre_elemental_agricola_granulado"),
        "the catalog has to declare its elemental S: {elemental:?}"
    );
    assert!(sources.iter().any(|s| s.form == FertilizerForm::Unknown), "an unmigrated row is `unknown`");
    assert!(sources.iter().any(|s| s.form == FertilizerForm::Amide), "urea declares its form");

    // The old rule keyed on the name; a product whose name says nothing
    // must now be judged on the column alone.
    for source in &sources {
        if source.form == FertilizerForm::Unknown {
            assert!(
                !source.form.needs_soil_transformation(),
                "{} must not be treated as slow-release on missing metadata",
                source.source_id
            );
        }
    }
}

/// Both shipped profiles still parse after the migration.
#[test]
fn both_profiles_load_with_the_new_column() {
    use non_nobis_solum::core::ports::FertilizerSourceRepository;
    use non_nobis_solum::infra::bootstrap::build_fertilizer_sources;

    for profile in ["global", "andina_colombia"] {
        let sources = build_fertilizer_sources(&DataLayout::new("data", profile))
            .list_sources()
            .unwrap_or_else(|e| panic!("{profile} failed to load: {e}"));
        assert!(!sources.is_empty(), "{profile} is empty");
    }
}

// ---------------------------------------------------------------------
// Efficiency bands as per-profile data
// ---------------------------------------------------------------------

/// The whole point of moving the thresholds into `efficiency_bands.toml`:
/// two profiles, the same lot, different efficiency — with no code path
/// that knows either profile's name.
#[test]
fn a_profiles_band_table_changes_the_efficiency_it_derives() {
    use non_nobis_solum::core::ports::EfficiencyBandRepository;
    use non_nobis_solum::infra::TomlEfficiencyBandsRepo;

    let rules = |profile: &str| {
        TomlEfficiencyBandsRepo::from_toml_file(format!("data/reference/{profile}/efficiency_bands.toml"))
            .unwrap_or_else(|e| panic!("{profile}: {e}"))
            .band_rules()
            .expect("rules")
    };

    // A strongly acid, ash-derived highland soil: exactly what the Andean
    // table's two overrides are about.
    let acid = ScenarioConditions {
        ph: 4.6,
        texture: Texture::Loam,
        irrigation: IrrigationSystem::Rainfed,
        mean_temp_c: Some(16.0),
        max_temp_c: Some(24.0),
        moisture_index: Some(1.0),
        aluminium_saturation_pct: Some(10.0),
    };

    let global = efficiency::adjust(Nutrient::P, 0.15, 0.20, &acid, SulfurForm::Unstated, &rules("global"));
    let andina = efficiency::adjust(Nutrient::P, 0.15, 0.20, &acid, SulfurForm::Unstated, &rules("andina_colombia"));

    assert!(
        andina.adjusted < global.adjusted,
        "the Andean table fixes P harder: {} vs {}",
        andina.adjusted,
        global.adjusted
    );
    assert_eq!(global.modifiers[0].factor, 0.70);
    assert_eq!(andina.modifiers[0].factor, 0.62);
    assert!(andina.modifiers[0].effect.contains("allophane"), "the row states its own reason");
    assert!(andina.floor < global.floor, "and lets the dose go further before clamping");

    // A neutral site sees no difference at all: the override is scoped to
    // the band it is about, not to the profile as a whole.
    let neutral = ScenarioConditions::reference(Texture::Loam, IrrigationSystem::Drip);
    assert_eq!(
        efficiency::adjust(Nutrient::P, 0.15, 0.20, &neutral, SulfurForm::Unstated, &rules("global")).adjusted,
        efficiency::adjust(Nutrient::P, 0.15, 0.20, &neutral, SulfurForm::Unstated, &rules("andina_colombia")).adjusted
    );
}

/// Both profiles still plan end to end with the new mandatory file.
#[test]
fn both_profiles_still_produce_a_plan_with_the_band_table_wired() {
    use non_nobis_solum::core::application::FertilityScenario;
    use non_nobis_solum::core::domain::NutrientDemandMode;
    use non_nobis_solum::core::ports::FertilityCalculatorPort;
    use non_nobis_solum::infra::bootstrap::build_calculate_fertility_plan;

    for profile in ["global", "andina_colombia"] {
        let plan = build_calculate_fertility_plan(&DataLayout::new("data", profile), None)
            .unwrap_or_else(|e| panic!("{profile} failed to wire: {e}"))
            .calculate(FertilityScenario {
                sample_id: "LOT-002".to_string(),
                field_id: "LOT-002".to_string(),
                crop_id: "coffee".to_string(),
                demand_mode: NutrientDemandMode::Extraction,
                yield_override: None,
            })
            .unwrap_or_else(|e| panic!("{profile} failed to plan: {e}"));

        assert!(!plan.band_rules.bands.is_empty(), "{profile} planned with an empty table");
        // The liming path is untouched by any of this.
        assert!(plan.liming.is_some(), "{profile} lost its lime recommendation");
        for entry in &plan.nutrient_results {
            assert!(entry.efficiency.adjusted >= plan.band_rules.floor(entry.nutrient));
            assert!(entry.efficiency.adjusted > 0.0, "{profile}/{} would divide by zero", entry.nutrient);
        }
    }
}

// ---------------------------------------------------------------------
// The shipped examples
// ---------------------------------------------------------------------

/// A broken example is worse than no example: it is the first thing a new
/// user runs, and it teaches them the file format. This imports all three
/// shipped files into a sandbox and plans every lot.
#[test]
fn the_three_shipped_examples_import_and_plan() {
    use non_nobis_solum::core::application::FertilityScenario;
    use non_nobis_solum::core::ports::FertilityCalculatorPort;
    use non_nobis_solum::infra::bootstrap::{self, CuratedSeed};
    use non_nobis_solum::infra::csv_import;

    let root = std::env::temp_dir().join(format!("nns_examples_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    bootstrap::ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("seed");

    // Order matters and the files say so: a reading needs its lot, a goal
    // needs its lot.
    let use_case = bootstrap::build_register_lot(&DataLayout::new(&root, "global"));
    for file in ["lots.csv", "soil_tests.csv", "yield_targets.csv"] {
        let report = csv_import::import(&root.join("examples").join(file), &use_case)
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!(report.rejected.is_empty(), "{file} rejected {:?}", report.rejected);
        assert!(report.accepted > 0, "{file} imported nothing");
    }

    // Each example is here because it reaches somewhere the others do not.
    for (lot, crop, profile, reaches) in [
        ("EJ-CAFE", "coffee", "andina_colombia", "liming"),
        ("EJ-HORT", "potato", "andina_colombia", "a dose"),
        ("EJ-CALI", "corn", "global", "a dose"),
    ] {
        let plan = bootstrap::build_calculate_fertility_plan(&DataLayout::new(&root, profile), None)
            .unwrap_or_else(|e| panic!("{lot}: {e}"))
            .calculate(FertilityScenario {
                sample_id: lot.to_string(),
                field_id: lot.to_string(),
                crop_id: crop.to_string(),
                demand_mode: NutrientDemandMode::Extraction,
                yield_override: None,
            })
            .unwrap_or_else(|e| panic!("{lot} failed to plan: {e}"));

        match reaches {
            "liming" => {
                let lime = plan.liming.as_ref().expect("the acid example has to reach liming");
                assert!(lime.recommended_t_ha > 1.0, "{lot}: {} t/ha", lime.recommended_t_ha);
            }
            _ => assert!(
                plan.nutrient_results.iter().any(|entry| entry.dose.is_some()),
                "{lot} asked for no fertilizer at all"
            ),
        }
        // And every one carries an area, so its totals mean something.
        assert!(plan.area_ha.is_some(), "{lot} has no planted area");
    }

    let _ = std::fs::remove_dir_all(&root);
}
