//! Command-line interface. Collects only what can't be inferred from the
//! reference catalog: which lot/sample, which crop, which yield goal (if
//! not already curated), and which reference profile to trust.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::core::application::FertilityScenario;
use crate::core::domain::{DomainError, SoilStatus, YieldTarget};
use crate::core::ports::{FertilityCalculatorPort, ListCropsPort};
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
            let scenario = args.into_scenario();
            let use_case = bootstrap::build_calculate_fertility_plan(&layout)?;
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
        "{:<4} {:>14} {:>10} {:>10} {:>10} {:>8}  {}",
        "Nut", "Availability", "Demand", "Eff.", "Net req.", "Status", "Dose"
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
