//! A laboratory is free to report an exchangeable cation in cmolc/kg or in
//! mg/kg, and the engine must reach the same recommendation either way.
//!
//! This lives here rather than beside the use case because proving it needs
//! the real reference tables and the real CSV adapters — `core` may not
//! reach into `infra`, not even from a test.

use std::path::{Path, PathBuf};

use non_nobis_solum::core::application::FertilityScenario;
use non_nobis_solum::core::domain::{DomainError, FertilityPlan, Nutrient, NutrientDemandMode};
use non_nobis_solum::core::ports::FertilityCalculatorPort;
use non_nobis_solum::infra::bootstrap::{build_calculate_fertility_plan, DataLayout};

/// LOT-002's shipped analysis, and the same readings restated in mg/kg.
/// Each conversion is the value times its `conversion_factors.toml` factor:
/// K x391, Ca x200, Mg x121, Al x89.94.
const CMOLC_SAMPLE: &str = "\
sample_id,nutrient_id,value,unit,method_id,depth_from_cm,depth_to_cm
LOT-002,P,9,mg_per_kg,Olsen,0,20
LOT-002,K,0.18,cmolc_per_kg,AcONH4_1N_pH7,0,20
LOT-002,S,10,mg_per_kg,turbidimetric,0,20
LOT-002,Ca,3.0,cmolc_per_kg,AcONH4_1N_pH7,0,20
LOT-002,Mg,0.8,cmolc_per_kg,AcONH4_1N_pH7,0,20
LOT-002,Al,1.5,cmolc_per_kg,KCl_1N,0,20
";

const MG_SAMPLE: &str = "\
sample_id,nutrient_id,value,unit,method_id,depth_from_cm,depth_to_cm
LOT-002,P,9,mg_per_kg,Olsen,0,20
LOT-002,K,70.38,mg_per_kg,AcONH4_1N_pH7,0,20
LOT-002,S,10,mg_per_kg,turbidimetric,0,20
LOT-002,Ca,600.0,mg_per_kg,AcONH4_1N_pH7,0,20
LOT-002,Mg,96.8,mg_per_kg,AcONH4_1N_pH7,0,20
LOT-002,Al,134.91,mg_per_kg,KCl_1N,0,20
";

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str, soil_tests: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nns_units_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_dir(Path::new("data"), &root.join("data"));
        std::fs::write(root.join("data/curated/soil_tests.csv"), soil_tests).expect("seed soil tests");
        Self { root }
    }

    fn plan_result(&self) -> Result<FertilityPlan, DomainError> {
        let layout = DataLayout::new(self.root.join("data"), "global");
        build_calculate_fertility_plan(&layout, None)
            .expect("wire the use case")
            .calculate(FertilityScenario {
                sample_id: "LOT-002".to_string(),
                field_id: "LOT-002".to_string(),
                crop_id: "coffee".to_string(),
                demand_mode: NutrientDemandMode::Extraction,
                yield_override: None,
            })
    }

    fn plan(&self) -> FertilityPlan {
        self.plan_result().expect("a plan")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("sandbox dir");
    for entry in std::fs::read_dir(from).expect("read data dir").flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy data file");
        }
    }
}

/// The regression this guards: every consumer used to read `SoilTest.value`
/// raw and assume the unit it wanted. A cation reported in mg/kg then read
/// as cmolc/kg inflates it ~200x, which reads as "high" against the
/// critical levels and pushes base saturation to ~100% — silently
/// cancelling the lime recommendation on exactly the acid soil that needs it.
#[test]
fn the_unit_the_lab_reported_in_does_not_change_the_plan() {
    let in_cmolc = Sandbox::new("cmolc", CMOLC_SAMPLE).plan();
    let in_mg = Sandbox::new("mg", MG_SAMPLE).plan();

    for (a, b) in in_cmolc.nutrient_results.iter().zip(&in_mg.nutrient_results) {
        assert_eq!(a.nutrient, b.nutrient);
        assert!(
            (a.availability_kg_ha - b.availability_kg_ha).abs() < 0.01,
            "{:?} availability: {} vs {}",
            a.nutrient,
            a.availability_kg_ha,
            b.availability_kg_ha
        );
        assert!(
            (a.net_requirement_kg_ha - b.net_requirement_kg_ha).abs() < 0.01,
            "{:?} net requirement: {} vs {}",
            a.nutrient,
            a.net_requirement_kg_ha,
            b.net_requirement_kg_ha
        );
        assert_eq!(a.soil_status, b.soil_status, "{:?} soil status", a.nutrient);
    }

    let lime_cmolc = in_cmolc.liming.expect("LOT-002 reports Al, so it gets a lime recommendation");
    let lime_mg = in_mg.liming.expect("the same sample in mg/kg still reports Al");
    assert!(
        (lime_cmolc.recommended_t_ha - lime_mg.recommended_t_ha).abs() < 0.01,
        "lime: {} vs {} t/ha",
        lime_cmolc.recommended_t_ha,
        lime_mg.recommended_t_ha
    );
    assert!(
        (lime_cmolc.current_base_saturation_pct - lime_mg.current_base_saturation_pct).abs() < 0.1,
        "base saturation: {}% vs {}%",
        lime_cmolc.current_base_saturation_pct,
        lime_mg.current_base_saturation_pct
    );
    // Not a tautology: an acid soil must still be *asking* for lime, or
    // the equality above would hold trivially at zero.
    assert!(lime_cmolc.recommended_t_ha > 0.0);
}

/// A unit no conversion factor covers stops the plan and names itself,
/// rather than being planned on at face value.
///
/// Refusing is the right answer here, not degrading: a dose is a number
/// somebody buys fertilizer with, and the engine cannot know whether an
/// unrecognised unit is off by a factor of 1 or of 200.
#[test]
fn a_reading_in_an_unconvertible_unit_refuses_the_plan_by_name() {
    let sandbox = Sandbox::new(
        "unknown",
        "sample_id,nutrient_id,value,unit,method_id,depth_from_cm,depth_to_cm\n\
         LOT-002,K,0.18,meq_per_100g,AcONH4_1N_pH7,0,20\n\
         LOT-002,Al,1.5,cmolc_per_kg,KCl_1N,0,20\n",
    );

    let error = sandbox.plan_result().expect_err("an unreadable unit cannot yield a dose");

    let message = error.to_string();
    assert!(message.contains("meq_per_100g"), "the offending unit must be named: {message}");
    assert!(message.contains(Nutrient::K.as_str()), "the offending nutrient must be named: {message}");
}
