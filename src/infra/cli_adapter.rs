//! Command-line interface. Collects only what can't be inferred from the
//! reference catalog: which lot/sample, which crop, which yield goal (if
//! not already curated), and which reference profile to trust.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::core::application::FertilityScenario;
use crate::core::domain::{DomainError, SoilStatus, YieldTarget};
use crate::core::ports::{FertilityCalculatorPort, InspectScenarioPort, ListCropsPort};
use crate::infra::bootstrap::{self, DataLayout};

#[derive(Parser)]
#[command(name = "non_nobis_solum", version, about = "Fertilization plan engine driven by soil analysis and crop removal coefficients")]
pub struct Cli {
    /// Root of the data catalog (data/reference, data/curated).
    #[arg(long, global = true, default_value = "data")]
    pub data_dir: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Calculate a full fertility plan for a lot/crop/yield scenario.
    Plan(ScenarioArgs),
    /// List crops supported by the chosen reference profile.
    Crops(ProfileArgs),
    /// Show the reference data and provenance behind a scenario.
    Inspect(ScenarioArgs),
}

#[derive(Args)]
pub struct ScenarioArgs {
    /// Sample/lot identifier (used to look up both the soil test and the field context).
    #[arg(long)]
    pub lot: String,
    /// Crop identifier, as listed by `crops`.
    #[arg(long)]
    pub crop: String,
    /// Harvested product the removal coefficients apply to (e.g. "grain").
    #[arg(long, default_value = "grain")]
    pub product: String,
    /// Yield goal; when omitted, falls back to the curated yield target for this lot/crop.
    #[arg(long)]
    pub yield_value: Option<f64>,
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
pub struct ProfileArgs {
    #[arg(long, default_value = "global")]
    pub profile: String,
}

impl ScenarioArgs {
    fn into_scenario(self) -> FertilityScenario {
        FertilityScenario {
            sample_id: self.lot.clone(),
            field_id: self.lot,
            crop_id: self.crop,
            product: self.product,
            yield_override: self.yield_value.map(|value| YieldTarget { value, unit: self.yield_unit }),
        }
    }
}

pub fn run(cli: Cli) -> Result<(), DomainError> {
    match cli.command {
        Command::Plan(args) => {
            let layout = DataLayout::new(&cli.data_dir, &args.profile);
            let climate = if args.no_climate { None } else { bootstrap::build_agroclimatic_repo() };
            let scenario = args.into_scenario();
            let use_case = bootstrap::build_calculate_fertility_plan(&layout, climate)?;
            let plan = use_case.calculate(scenario)?;
            print_plan(&plan);
        }
        Command::Crops(args) => {
            let layout = DataLayout::new(&cli.data_dir, &args.profile);
            let use_case = bootstrap::build_list_supported_crops(&layout);
            for crop in use_case.list_crops()? {
                println!("{:<12} {:<20} {:<10} {}", crop.crop_id, crop.name, crop.crop_type, crop.family);
            }
        }
        Command::Inspect(args) => {
            let layout = DataLayout::new(&cli.data_dir, &args.profile);
            let scenario = args.into_scenario();
            let use_case = bootstrap::build_inspect_scenario(&layout)?;
            let inspection = use_case.inspect(&scenario)?;
            print_inspection(&inspection);
        }
    }
    Ok(())
}

fn print_plan(plan: &crate::core::domain::FertilityPlan) {
    println!(
        "Fertility plan — field {} / sample {} / crop {} / yield target {} {}",
        plan.field_id, plan.sample_id, plan.crop_id, plan.yield_target.value, plan.yield_target.unit
    );
    println!(
        "{:<4} {:>14} {:>10} {:>10} {:>10} {:>8}  Dose",
        "Nut", "Availability", "Demand", "Eff.", "Net req.", "Status"
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
        println!(
            "{:<4} {:>11.1} kg/ha {:>7.1} kg/ha {:>9.0}% {:>7.1} kg/ha {:>8}  {}",
            entry.nutrient,
            entry.availability_kg_ha,
            entry.demand_kg_ha,
            entry.efficiency_used * 100.0,
            entry.net_requirement_kg_ha,
            status,
            dose
        );
    }

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
            Some(dose) => println!("  Material: {:.2} t/ha {}", dose.t_product_per_ha, dose.source_name),
            None => println!("  Material: -"),
        }
    }
}

/// Reports which climate-derived values the plan actually used, and emits
/// the single stderr warning when it ran without any. Every figure here
/// is labelled `[climate-adjusted]` or `[baseline]` — the mineralization
/// factor alone can move N availability by 3x, so the reader must never
/// have to guess which regime produced a number.
fn print_climate(plan: &crate::core::domain::FertilityPlan) {
    let Some(climate) = &plan.climate else {
        eprintln!("[climate] NASA POWER unavailable — running without climate enrichment");
        println!("N mineralization factor: {:.4}  [baseline — no climate data]", plan.mineralization_factor);
        return;
    };

    match climate.mean_temp_c {
        Some(temp) => println!("N mineralization factor: {:.4}  [climate-adjusted, T={temp:.1}°C]", plan.mineralization_factor),
        // Reachable: the climatology arrived but without a usable T2M, so
        // the factor fell back to baseline even though climate is present.
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

fn print_inspection(inspection: &crate::core::application::ScenarioInspection) {
    println!(
        "Scenario data — field {} (texture {}, irrigation {}, region {})",
        inspection.field_context.field_id,
        inspection.field_context.texture,
        inspection.field_context.irrigation_system,
        inspection.field_context.region
    );
    println!("Yield target: {} {}", inspection.yield_target.value, inspection.yield_target.unit);
    println!("Soil tests on file:");
    for test in &inspection.soil_tests {
        println!("  {:<4} {:>8} {}", test.nutrient, test.value, test.unit);
    }
    println!("Reference provenance used for this scenario:");
    for p in &inspection.provenance {
        if let Some(removal) = &p.removal_reference {
            println!(
                "  {:<4} removal={} kg/unit  source={} region={} year={} dataset={}",
                p.nutrient, removal.removal_kg_per_unit, removal.source, removal.region, removal.year, removal.dataset_version
            );
        } else {
            println!("  {:<4} removal=<no data for this crop/product>", p.nutrient);
        }
        if let Some((min, max)) = p.efficiency_range {
            println!("       efficiency range: {:.0}%-{:.0}%", min * 100.0, max * 100.0);
        }
        if let Some(level) = &p.critical_level {
            println!(
                "       critical levels: low<{} medium<{} (excess ceiling {})  source={} year={}",
                level.low_threshold, level.medium_threshold, level.high_threshold, level.source, level.year
            );
        }
    }
}
