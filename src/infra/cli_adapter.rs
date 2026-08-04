//! Collects only what the reference catalog can't infer: which lot, which
//! crop, which yield goal if not already curated, which profile.

use std::path::{Path, PathBuf};

use clap::{Args, CommandFactory, Parser, Subcommand};

use crate::core::application::{FertilityScenario, FormulationRequest};
use crate::core::domain::{DomainError, NutrientDemandMode, PlanWarning, SoilStatus, YieldTarget};
use crate::core::ports::{
    FertilityCalculatorPort, InspectScenarioPort, ListCropsPort, RecommendFertilizerProgramPort,
};
use crate::infra::bootstrap::{self, App, CuratedSeed, DataLayout};
use crate::infra::{csv_import, report_export, report_renderer, shipped_data};

#[derive(Parser)]
#[command(name = "nns", version, about = "Fertilization plan engine driven by soil analysis and crop removal coefficients")]
/// The command line, as clap parses it.
pub struct Cli {
    /// Root of the data catalog. Defaults to $XDG_DATA_HOME/non-nobis-solum
    /// (~/.local/share/non-nobis-solum), created and seeded on first run.
    /// Naming one explicitly takes it as it is and seeds nothing — which
    /// is how to run against this repository's own `data/`.
    #[arg(long, global = true, conflicts_with = "test")]
    pub data_dir: Option<PathBuf>,

    /// Work against the shipped demonstration lots in a disposable
    /// catalog under the temp directory, leaving your own records
    /// untouched. On its own, with no subcommand, reports whether this
    /// installation can plan at all.
    #[arg(long, global = true)]
    pub test: bool,

    /// Optional so `--test` alone is a complete command.
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
/// The subcommands `nns` accepts.
pub enum Command {
    /// Calculate a full fertility plan for a lot/crop/yield scenario.
    Plan(PlanArgs),
    /// List crops supported by the chosen reference profile.
    Crops(ProfileArgs),
    /// Show the reference data and provenance behind a scenario.
    Inspect(ScenarioArgs),
    /// Load lots, soil analyses or yield goals from a CSV.
    ///
    /// The columns are the curated files' own, so a spreadsheet export
    /// round-trips and a lab panel goes in as the table it arrived as. What
    /// the file is gets read off its header — `texture` means lots,
    /// `nutrient_id` means soil tests, `yield_value` means planning rows.
    /// Every row is validated exactly as the forms validate one, and a bad
    /// row is reported by line number without stopping the good ones.
    Import(ImportArgs),
}

#[derive(Args)]
/// Arguments of `import`.
pub struct ImportArgs {
    /// The CSV to read.
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
}

#[derive(Args)]
/// The lot, crop and goal every scenario-shaped subcommand needs.
pub struct ScenarioArgs {
    /// Sample/lot identifier (used to look up both the soil test and the field context).
    #[arg(long)]
    pub lot: String,
    /// Crop identifier, as listed by `crops`.
    #[arg(long)]
    pub crop: String,
    /// Which reference coefficient sizes the crop's demand.
    /// `extraction` (default) plans to replace what the harvest removes;
    /// `absorption` plans for total plant uptake, which is the right basis
    /// when residues leave the field. On `extraction`, a nutrient that
    /// needs no fertilizer is retried on `absorption` and reported as
    /// such — see the warnings printed under the plan.
    #[arg(long, default_value = "extraction")]
    pub demand_mode: String,
    /// Yield goal; when omitted, falls back to the curated yield target for this lot/crop.
    #[arg(long)]
    pub yield_value: Option<f64>,
    /// The unit `yield_value` is stated in. Must match the unit the crop's
    /// removal coefficients are stated per.
    #[arg(long, default_value = "t_ha")]
    pub yield_unit: String,
    /// Reference profile to trust for removal coefficients, efficiencies, etc.
    #[arg(long, default_value = "global")]
    pub profile: String,
    /// Skip the agroclimatic API and plan from the baseline constants.
    /// Only affects `plan`; `inspect` never queries climate.
    #[arg(long)]
    pub no_climate: bool,
}

#[derive(Args)]
/// The lone `--profile` flag, for subcommands that need nothing else.
pub struct ProfileArgs {
    /// Reference profile to read the tables from.
    #[arg(long, default_value = "global")]
    pub profile: String,
}

/// `plan` is the one subcommand that also picks products, so the
/// formulation knobs hang off it alone; `inspect` keeps the bare scenario.
#[derive(Args)]
pub struct PlanArgs {
    /// The lot, crop and goal to plan for.
    #[command(flatten)]
    pub scenario: ScenarioArgs,
    /// How to cover the requirement. `composite_plus_simple` (default)
    /// leads with the closest compound product and closes the gaps with
    /// straights; `simple_blend_only` uses no compound at all and reports
    /// a physical blend. The other one is computed either way, for
    /// comparison.
    #[arg(long, default_value = "composite_plus_simple")]
    pub strategy: String,
    /// Hectares to buy for, overriding the lot's own `area_ha`. Same shape
    /// as `--yield-value` over the curated yield goal: the curated figure
    /// is the lot's, this is for one run. A lot with neither is planned per
    /// hectare, and the per-hectare figures never change either way.
    #[arg(long)]
    pub area_ha: Option<f64>,
    /// What one bag weighs locally, in kg.
    #[arg(long, default_value_t = 50.0)]
    pub bag_kg: f64,
    /// Print the whole derivation — the target grade, every scaling step,
    /// the ranked candidates and why each was taken or left.
    #[arg(long)]
    pub explain: bool,
    /// Which blend search runs. A **diagnostic**, not a recommendation
    /// knob: on every shipped catalog both answers reach the same blend,
    /// because the objective is linear in the share a product takes and the
    /// exhaustive ordering baseline already reaches its breakpoints. It is
    /// here so that claim stays checkable — `split_pairs` (default) may
    /// give a nutrient to two products, `single_pick` is the
    /// one-product-per-nutrient baseline it is measured against. Deliberately
    /// absent from the TUI settings, where a control that cannot change an
    /// answer is worse than no control.
    #[arg(long, default_value = "split_pairs")]
    pub blend_search: String,
    /// Write the full report — balance included — to this path. The format
    /// comes from the extension: `.pdf`, `.md`, `.csv` or `.txt`.
    #[arg(long, value_name = "FILE")]
    pub export: Option<PathBuf>,
}

impl ScenarioArgs {
    fn into_scenario(self) -> Result<FertilityScenario, DomainError> {
        Ok(FertilityScenario {
            sample_id: self.lot.clone(),
            field_id: self.lot,
            crop_id: self.crop,
            demand_mode: self.demand_mode.parse()?,
            yield_override: self.yield_value.map(|value| YieldTarget { value, unit: self.yield_unit }),
        })
    }
}

/// Runs one CLI invocation: the single entry point `main` delegates to.
///
/// # Errors
/// Whatever the selected subcommand's use case returns — `NotFound` for a
/// lot, crop or goal that is not curated, `InvalidInput` for an argument
/// that does not parse or a reading in an unconvertible unit, `DataSource`
/// for a curated or reference file that cannot be read or written.
pub fn run(cli: Cli) -> Result<(), DomainError> {
    // Seeding is part of running a root we chose; a root the user named is
    // theirs, and filling it with our files uninvited would be a surprise.
    let data_dir = if cli.test {
        let root = bootstrap::test_data_root();
        bootstrap::ensure_data_root(&root, CuratedSeed::WithExamples)?;
        root
    } else if let Some(dir) = cli.data_dir { dir } else {
        let root = bootstrap::default_data_root();
        bootstrap::ensure_data_root(&root, CuratedSeed::HeadersOnly)?;
        root
    };

    let Some(command) = cli.command else {
        if cli.test {
            return self_check(&data_dir);
        }
        // clap would have said this itself if the subcommand were still
        // mandatory; it stopped being so for `--test`, so say it here.
        let _ = Cli::command().print_help();
        return Err(DomainError::InvalidInput("a subcommand is required, or --test on its own".to_string()));
    };

    match command {
        Command::Plan(args) => {
            let layout = DataLayout::new(&data_dir, &args.scenario.profile);
            let profile = args.scenario.profile.clone();
            let climate = if args.scenario.no_climate { None } else { bootstrap::build_agroclimatic_repo() };
            let scenario = args.scenario.into_scenario()?;
            let plan = bootstrap::build_calculate_fertility_plan(&layout, climate)?.calculate(scenario)?;
            print_plan(&plan);

            let request = FormulationRequest {
                strategy: args.strategy.parse()?,
                total_area_ha: args.area_ha.or(plan.area_ha).unwrap_or(1.0),
                bag_weight_kg: args.bag_kg,
                profile,
                blend_search: args.blend_search.parse()?,
            };
            let report = bootstrap::build_recommend_fertilizer_program(&layout)?.recommend(&plan, &request)?;
            // Short by default, everything on `--explain`: the derivation
            // is nine sections long and most runs want the shopping list.
            let lines = if args.explain {
                report_renderer::render(&report)
            } else {
                report_renderer::render_summary(&report)
            };
            for line in lines {
                println!("{line}");
            }
            if let Some(path) = &args.export {
                report_export::export(&report, path)?;
                println!("\nReport written to {}", path.display());
            }
        }
        Command::Crops(args) => {
            let layout = DataLayout::new(&data_dir, &args.profile);
            let use_case = bootstrap::build_list_supported_crops(&layout);
            for crop in use_case.list_crops()? {
                println!("{:<12} {:<20} {:<10} {}", crop.crop_id, crop.name, crop.crop_type, crop.family);
            }
        }
        Command::Import(args) => {
            let layout = DataLayout::new(&data_dir, "global");
            let report = csv_import::import(&args.file, &bootstrap::build_register_lot(&layout))?;
            println!("{}", report.summary());
            for (line, why) in &report.rejected {
                // Line number first: the point of the report is to be able
                // to open the file and go straight there.
                println!("  line {line}: {why}");
            }
            if report.accepted == 0 && !report.rejected.is_empty() {
                return Err(DomainError::InvalidInput("nothing was imported".to_string()));
            }
        }
        Command::Inspect(args) => {
            let layout = DataLayout::new(&data_dir, &args.profile);
            let scenario = args.into_scenario()?;
            let use_case = bootstrap::build_inspect_scenario(&layout)?;
            let inspection = use_case.inspect(&scenario)?;
            print_inspection(&inspection);
        }
    }
    Ok(())
}

/// What `--test` answers on its own: can this installation plan at all?
///
/// Every line reports the figure it found rather than a bare "ok" — a
/// check that hides what it looked at is no easier to trust than no check
/// — and a failure exits non-zero so a script can gate on it.
///
/// The network is deliberately out of scope. NASA POWER being down is not
/// an installation fault, and the engine is required to run without it.
fn self_check(data_dir: &Path) -> Result<(), DomainError> {
    println!("catalog  {}\n", data_dir.display());
    let mut failures = 0;

    // Seeding has already run by the time we get here, so this line does
    // not prove the files pre-existed — it proves they could be written.
    // A read-only home or a full disk is what fails it.
    let shipped: Vec<&str> =
        shipped_data::REFERENCE.iter().chain(&shipped_data::CURATED).map(|(path, _)| *path).collect();
    let present = shipped.iter().filter(|path| data_dir.join(path).exists()).count();
    failures += report("seeded", &format!("{present}/{} files written", shipped.len()), present == shipped.len());

    let app = App { data_root: data_dir.to_path_buf(), profile: "global".to_string() };
    let profiles = app.profiles();
    // Building the use case is what parses every reference table, so a
    // profile that wires is a profile whose TOML, YAML and CSV all read.
    let broken: Vec<&String> = profiles
        .iter()
        .filter(|profile| bootstrap::build_calculate_fertility_plan(&DataLayout::new(data_dir, *profile), None).is_err())
        .collect();
    let listed = if broken.is_empty() { profiles.join(", ") } else { format!("{broken:?} failed to load") };
    failures += report("profiles", &listed, broken.is_empty() && !profiles.is_empty());

    let crops = bootstrap::build_list_supported_crops(&DataLayout::new(data_dir, "global")).list_crops();
    match &crops {
        Ok(crops) => failures += report("crops", &format!("{} in the catalog", crops.len()), !crops.is_empty()),
        Err(e) => failures += report("crops", &e.to_string(), false),
    }

    // The two demonstration lots exercise different halves of the engine:
    // LOT-001 needs nitrogen and so reaches the dose path, LOT-002 reports
    // Al³⁺ and so reaches liming. Neither alone would cover both.
    match plan_demo(data_dir, "LOT-001", "corn") {
        Ok(plan) => {
            let dose = plan.nutrient_results.iter().find_map(|entry| entry.dose.as_ref());
            let detail = match dose {
                Some(dose) => format!("LOT-001/corn: {:.0} kg/ha {}", dose.kg_product_per_ha, dose.source_name),
                None => "LOT-001/corn asked for no fertilizer at all".to_string(),
            };
            failures += report("doses", &detail, dose.is_some());
        }
        Err(e) => failures += report("doses", &e.to_string(), false),
    }

    match plan_demo(data_dir, "LOT-002", "coffee") {
        Ok(plan) => {
            let detail = match &plan.liming {
                Some(liming) => format!("LOT-002/coffee: {:.2} t/ha", liming.recommended_t_ha),
                None => "LOT-002 reports Al³⁺ but got no recommendation".to_string(),
            };
            failures += report("liming", &detail, plan.liming.is_some());
        }
        Err(e) => failures += report("liming", &e.to_string(), false),
    }

    println!();
    if failures > 0 {
        return Err(DomainError::DataSource(format!("{failures} check(s) failed")));
    }
    println!("all checks passed");
    Ok(())
}

/// Offline on purpose — see [`self_check`].
fn plan_demo(data_dir: &Path, lot: &str, crop: &str) -> Result<crate::core::domain::FertilityPlan, DomainError> {
    bootstrap::build_calculate_fertility_plan(&DataLayout::new(data_dir, "global"), None)?.calculate(FertilityScenario {
        sample_id: lot.to_string(),
        field_id: lot.to_string(),
        crop_id: crop.to_string(),
        demand_mode: NutrientDemandMode::Extraction,
        yield_override: None,
    })
}

/// Returns 1 when the check failed, so the caller can just sum.
fn report(name: &str, detail: &str, ok: bool) -> u32 {
    println!("{name:<10} {detail:<44} {}", if ok { "ok" } else { "FAILED" });
    u32::from(!ok)
}

fn print_plan(plan: &crate::core::domain::FertilityPlan) {
    println!(
        "Fertility plan — field {} / sample {} / crop {} / yield target {} {} / demand basis {}",
        plan.field_id, plan.sample_id, plan.crop_id, plan.yield_target.value, plan.yield_target.unit, plan.demand_mode
    );
    println!(
        "{:<4} {:>14} {:>10} {:>5} {:>10} {:>10} {:>8}  Dose",
        "Nut", "Availability", "Demand", "Basis", "Eff.", "Net req.", "Status"
    );
    for entry in &plan.nutrient_results {
        let status = match entry.soil_status {
            Some(SoilStatus::Low) => "low",
            Some(SoilStatus::Medium) => "medium",
            Some(SoilStatus::High) => "high",
            None => "-",
        };
        let dose = match &entry.dose {
            Some(d) => format!("{:.1} kg/ha {}", d.kg_product_per_ha, d.source_name),
            None => "-".to_string(),
        };
        // A nutrient with no coefficient on either basis has an unknown
        // demand, not a demand of zero. Printing "0.0" there would read as
        // "this crop needs none of it".
        let (demand, basis) = match entry.demand_mode_used {
            Some(NutrientDemandMode::Extraction) => (format!("{:>7.1} kg/ha", entry.demand_kg_ha), "extr"),
            Some(NutrientDemandMode::Absorption) => (format!("{:>7.1} kg/ha", entry.demand_kg_ha), "abs"),
            None => ("  no data".to_string(), "-"),
        };
        println!(
            "{:<4} {:>11.1} kg/ha {:>13} {:>5} {:>9.0}% {:>7.1} kg/ha {:>8}  {}",
            entry.nutrient,
            entry.availability_kg_ha,
            demand,
            basis,
            entry.efficiency_used * 100.0,
            entry.net_requirement_kg_ha,
            status,
            dose
        );
    }

    // A separate block, because these are corrected to their critical
    // level rather than balanced against a crop removal — printing them in
    // the table above would imply a "Demand" that does not exist.
    if !plan.micronutrients.is_empty() {
        println!("Micronutrients (corrected to the critical level, not to crop removal):");
        for entry in &plan.micronutrients {
            // "No dose" has two causes and they are opposite news: the
            // soil is fine, or the soil is short and this profile's
            // catalog sells nothing that carries the element.
            let dose = match (&entry.dose, entry.net_requirement_kg_ha > 0.0) {
                (Some(d), _) => format!("{:.1} kg/ha {}", d.kg_product_per_ha, d.source_name),
                (None, false) => "- (at or above its critical level)".to_string(),
                (None, true) => "- NO SOURCE: this profile's catalog carries no product with it".to_string(),
            };
            println!(
                "  {:<4} {:>8.2} {:<12} target {:<6} {:>7.2} kg/ha net  {}",
                entry.nutrient, entry.soil_value, entry.unit, entry.critical_low_threshold, entry.net_requirement_kg_ha, dose
            );
        }
    }

    print_warnings(&plan.warnings);
    print_climate(plan);

    if let Some(liming) = &plan.liming {
        println!(
            "Liming — base saturation {:.1}% (target {:.0}%); requirement: {:.2} t/ha by Al3+, {:.2} t/ha by base saturation -> recommended {:.2} t/ha",
            liming.current_base_saturation_pct,
            liming.target_base_saturation_pct,
            liming.al_based_t_ha,
            liming.base_saturation_based_t_ha,
            liming.recommended_t_ha
        );
        match &liming.material {
            Some(dose) => println!("  Single material: {:.2} t/ha {}", dose.t_product_per_ha, dose.source_name),
            None => println!("  Single material: -"),
        }
        // Offered beside the single material, not instead of it: the
        // combination also corrects the base balance, but it asks the
        // grower to source three products.
        if let Some(mixture) = &liming.mixture {
            println!("  Or, combined ({}):", mixture.mixture_id);
            for component in &mixture.components {
                println!("    {:.2} t/ha {}", component.t_product_per_ha, component.source_name);
            }
        }
    }
}

/// Warnings are not errors and never suppress the plan, but a dose
/// computed on a basis the caller did not ask for has to say so where the
/// dose is read, not in a log.
fn print_warnings(warnings: &[PlanWarning]) {
    for warning in warnings {
        match warning {
            PlanWarning::FallbackToAbsorption { nutrient, net_requirement_kg_ha } => println!(
                "[!] {nutrient}: extraction asked for no fertilizer; the {net_requirement_kg_ha:.1} kg/ha above is on the \
                 ABSORPTION basis (total plant uptake, most of which returns in the residues)."
            ),
            PlanWarning::NoRemovalCoefficient { nutrient } => println!(
                "[!] {nutrient}: the reference table has no coefficient for this crop on either basis — its demand is \
                 unknown, not zero, and no dose was computed."
            ),
        }
    }
}

/// Every figure is labelled `[climate-adjusted]` or `[baseline]`: the
/// mineralization factor alone moves N availability by up to 3x, so the
/// reader must never have to guess which regime produced a number.
fn print_climate(plan: &crate::core::domain::FertilityPlan) {
    let Some(climate) = &plan.climate else {
        eprintln!("[climate] NASA POWER unavailable — running without climate enrichment");
        println!("N mineralization factor: {:.4}  [baseline — no climate data]", plan.mineralization_factor);
        return;
    };

    match climate.mean_temp_c {
        Some(temp) => println!("N mineralization factor: {:.4}  [climate-adjusted, T={temp:.1}°C]", plan.mineralization_factor),
        // Reachable: climatology arrived without a usable T2M, so the
        // factor fell back to baseline even though climate is present.
        None => println!("N mineralization factor: {:.4}  [baseline — climatology has no mean temperature]", plan.mineralization_factor),
    }

    if let Some(solar) = climate.solar_mj_m2_per_day {
        let index = crate::core::domain::services::rue_index(solar);
        let label = if index >= 0.8 {
            "HIGH"
        } else if index >= 0.5 {
            "MEDIUM"
        } else {
            "LOW"
        };
        // Informational only — no dose above was scaled by this.
        println!("Solar yield potential: {label} (RUE index {index:.2}, {solar:.1} MJ/m²/day)");
    }
}

/// The qualitative reading of the analysis: Tabla 12's interpretation
/// columns and its base-balance block. None of it scales a dose, and the
/// header says so — a figure printed next to a recommendation reads as an
/// input to it otherwise.
fn print_soil_quality(assessment: &crate::core::domain::SoilQualityAssessment) {
    let row = |a: &crate::core::domain::PropertyAssessment| {
        // "unclassified" is a real answer here, not a failure: Tabla 12's
        // base-balance block names some stretches of the number line and
        // leaves others unnamed, and a value in a gap must not be rounded
        // into the nearest named band.
        println!(
            "  {:<24} {:>8.2} {:<14} {}",
            a.property,
            a.value,
            a.unit,
            a.category.as_deref().unwrap_or("unclassified — the table names no band here")
        );
    };

    let belt = match assessment.climate_zone {
        Some(zone) => format!("{zone} belt"),
        None => "no altitude or climatology on file, so organic matter is not interpreted".to_string(),
    };
    println!("Soil interpretation (Tabla 12 / Tabla 4 — diagnostic only, nothing below scales a dose) — {belt}:");
    for assessment in &assessment.properties {
        row(assessment);
    }
    if !assessment.cation_ratios.is_empty() {
        // Antagonism, which the per-nutrient critical levels cannot see: a
        // soil can be adequate in K and still starve the crop of it
        // because Ca and Mg crowd the uptake sites.
        println!("Base balance (cation antagonism):");
        for ratio in &assessment.cation_ratios {
            row(ratio);
        }
    }
}

fn print_inspection(inspection: &crate::core::application::ScenarioInspection) {
    println!(
        "Scenario data — field {} (texture {}, irrigation {}, region {})",
        inspection.field_context.field_id,
        inspection.field_context.texture,
        inspection.field_context.irrigation_system,
        inspection.field_context.region
    );
    println!("Yield target: {} {}", inspection.yield_target.value, inspection.yield_target.unit);
    print_soil_quality(&inspection.soil_quality);
    println!("Soil tests on file:");
    for test in &inspection.soil_tests {
        println!("  {:<4} {:>8} {}", test.nutrient, test.value, test.unit);
    }
    println!("Reference provenance used for this scenario:");
    for p in &inspection.provenance {
        if let Some(removal) = &p.removal_reference {
            // A dash where the source table has none: the two bases are
            // reported separately because filling one from the other is
            // precisely the transcription error this schema removed.
            let basis = |value: Option<f64>| value.map_or_else(|| "-".to_string(), |v| format!("{v}"));
            println!(
                "  {:<4} extraction={} absorption={} kg/{} of {}  source={} region={} year={} dataset={}",
                p.nutrient,
                basis(removal.extraction_kg_per_unit),
                basis(removal.absorption_kg_per_unit),
                removal.yield_unit,
                removal.harvested_organ,
                removal.source,
                removal.region,
                removal.year,
                removal.dataset_version
            );
        } else {
            println!("  {:<4} <no coefficient for this crop on either basis>", p.nutrient);
        }
        if let Some((min, max)) = p.efficiency_range {
            println!("       efficiency range: {:.0}%-{:.0}%", min * 100.0, max * 100.0);
        }
        if let Some(level) = &p.critical_level {
            // The unit belongs beside every threshold: without it a K
            // figure of 0.3 could be cmolc/kg or mg/kg, which differ by
            // three orders of magnitude and classify the same soil
            // oppositely.
            println!(
                "       critical levels ({}): low<{} medium<{} (excess ceiling {})  method={} source={} year={}",
                level.unit,
                level.low_threshold,
                level.medium_threshold,
                level.high_threshold,
                level.extraction_method,
                level.source,
                level.year
            );
        }
    }
}
