//! Tabla 12 end to end against the curated lots: the qualitative
//! interpretation, the base balance, and the combined liming mixture.
//!
//! Here rather than beside the use cases because every assertion is about
//! a real reference table read through a real CSV adapter — `core` may not
//! reach into `infra`, not even from a test.

// Asserting an exact `0.0`: the requirement is not "close to nothing", it is
// the nutrient never entering the plan at all. An epsilon would let a real
// dose through.
#![allow(clippy::float_cmp)]

use non_nobis_solum::core::application::FertilityScenario;
use non_nobis_solum::core::domain::{FertilityPlan, NutrientDemandMode};
use non_nobis_solum::core::ports::{FertilityCalculatorPort, InspectScenarioPort};
use non_nobis_solum::infra::bootstrap::{build_calculate_fertility_plan, build_inspect_scenario, DataLayout};

fn scenario(lot: &str, crop: &str) -> FertilityScenario {
    FertilityScenario {
        sample_id: lot.to_string(),
        field_id: lot.to_string(),
        crop_id: crop.to_string(),
        demand_mode: NutrientDemandMode::Extraction,
        yield_override: None,
    }
}

fn layout() -> DataLayout {
    DataLayout::new("data", "global")
}

fn plan(lot: &str, crop: &str) -> FertilityPlan {
    build_calculate_fertility_plan(&layout(), None)
        .expect("wire the use case")
        .calculate(scenario(lot, crop))
        .expect("a plan")
}

/// The whole point of splitting the requirement: Tabla 12's note says
/// 1 t/ha of `CaCO3` equals 0.36 t hydrated lime + 0.48 t dolomite + 0.17 t
/// Paz del Río slag. If the oxide equivalents in `liming_materials.csv` or
/// the share arithmetic drift, these ratios stop reproducing the table.
#[test]
fn the_combined_liming_mixture_reproduces_the_source_table() {
    let plan = plan("LOT-002", "coffee");
    let liming = plan.liming.expect("LOT-002 reports Al3+, so it gets a recommendation");
    let mixture = liming.mixture.expect("the shipped catalog declares a mixture");
    let required = liming.recommended_t_ha;
    assert!(required > 0.0, "an acid soil must actually be asking for lime");

    let per_tonne: Vec<(String, f64)> = mixture
        .components
        .iter()
        .map(|c| (c.source_id.clone(), c.t_product_per_ha / required))
        .collect();

    for (source_id, expected) in [("hydrated_lime", 0.36), ("dolomitic_lime", 0.48), ("paz_del_rio", 0.17)] {
        let (_, actual) = per_tonne
            .iter()
            .find(|(id, _)| id == source_id)
            .unwrap_or_else(|| panic!("{source_id} missing from the mixture"));
        assert!(
            (actual - expected).abs() < 0.01,
            "{source_id}: {actual:.3} t per t CaCO3, table says {expected}"
        );
    }

    // The shares have to add up to the requirement, or the combination
    // silently under- or over-limes relative to the single material.
    let delivered: f64 = per_tonne.iter().map(|(_, share)| share).sum();
    assert!((delivered - 1.01).abs() < 0.05, "the three components total {delivered:.3} t per t CaCO3");
}

/// Micronutrients are corrected against their critical level, and only
/// where the sample actually reports one. Untested is not adequate.
#[test]
fn micronutrients_are_corrected_only_where_the_lab_measured_them() {
    let plan = plan("LOT-001", "corn");

    let zinc = plan
        .micronutrients
        .iter()
        .find(|m| m.nutrient.as_str() == "Zn")
        .expect("LOT-001 reports Zn");
    // 1.4 mg/kg against a 3.0 threshold over 2,600,000 kg/ha of soil:
    // a 1.6 mg/kg shortfall is 4.16 kg/ha, at 15% efficiency -> 27.7.
    assert!((zinc.deficit_kg_ha - 4.16).abs() < 0.01, "deficit {}", zinc.deficit_kg_ha);
    assert!((zinc.net_requirement_kg_ha - 27.73).abs() < 0.1, "net {}", zinc.net_requirement_kg_ha);

    let manganese = plan.micronutrients.iter().find(|m| m.nutrient.as_str() == "Mn").expect("Mn");
    assert_eq!(manganese.net_requirement_kg_ha, 0.0, "12 mg/kg is above the 5 mg/kg threshold");

    assert!(
        plan.micronutrients.iter().all(|m| m.nutrient.as_str() != "Cu"),
        "an untested micronutrient must be absent, not reported as adequate"
    );
}

/// Extraction is the default basis; a nutrient it asks nothing for is
/// retried on absorption and the plan says so.
#[test]
fn a_nutrient_extraction_asks_nothing_for_falls_back_to_absorption() {
    let plan = plan("LOT-001", "corn");

    let sulphur = plan
        .nutrient_results
        .iter()
        .find(|e| e.nutrient.as_str() == "S")
        .expect("S is planned");
    // Tabla 10 gives maize 1 kg S/t extracted against 4 absorbed. At 9.5
    // t/ha the extraction figure sits under the soil's own supply and the
    // absorption one does not.
    assert_eq!(sulphur.demand_mode_used, Some(NutrientDemandMode::Absorption));
    assert!(sulphur.net_requirement_kg_ha > 0.0);
    assert!(
        plan.warnings.iter().any(|w| format!("{w:?}").contains("FallbackToAbsorption")),
        "the switch of basis has to be stated, not silent: {:?}",
        plan.warnings
    );
}

/// The qualitative half: pH class, the thermal belt organic matter is read
/// against, and the base balance that per-nutrient thresholds cannot see.
#[test]
fn the_curated_lot_is_interpreted_against_tabla_12() {
    let inspection = build_inspect_scenario(&layout())
        .expect("wire the use case")
        .inspect(&scenario("LOT-002", "coffee"))
        .expect("an inspection");

    let category = |property: &str| {
        inspection
            .soil_quality
            .properties
            .iter()
            .chain(&inspection.soil_quality.cation_ratios)
            .find(|a| a.property == property)
            .unwrap_or_else(|| panic!("{property} not assessed"))
            .category
            .clone()
    };

    // LOT-002: pH 5.8, 2.5% organic matter at 2527 m, Ca 3.0 / Mg 0.8 / K 0.18.
    assert_eq!(category("ph").as_deref(), Some("moderately_acid"));
    // 2.5% is "sufficient" in the lowlands and "very low" this high up —
    // the reason the belt is a lookup key and not a footnote.
    assert_eq!(category("organic_matter").as_deref(), Some("very_low"));
    assert_eq!(category("ca_to_mg").as_deref(), Some("ideal"), "3.0/0.8 = 3.75");
    assert_eq!(category("mg_to_k").as_deref(), Some("magnesium_deficient"), "0.8/0.18 = 4.44 < 6");
    // Al 1.5 of a CICE of 5.48 is 27%, which Tabla 12 calls a medium
    // acidity problem — consistent with the lime this lot is prescribed.
    assert_eq!(category("aluminum_saturation").as_deref(), Some("acidity_medium"));
}
