//! Pure domain services: no IO, no repositories, just the agronomic math.
//! Ported and generalized from the `Non-Nobis-Solum-Py` prototype
//! (n.py/p.py/k.py availability and net-requirement formulas).

use super::entities::{AnnualClimatology, QualitativeBand};
use super::value_objects::IrrigationSystem;

/// Fraction of total soil N assumed to mineralize annually when nothing
/// is known about the site. The prototype's hardcoded 1.5% (`n.py`), and
/// the documented *average* of the admissible range — not its optimum.
/// [`mineralization_factor`] modulates it rather than replacing it.
pub const BASELINE_MINERALIZATION_FACTOR: f64 = 0.015;

/// Upper bound of the admissible annual mineralization rate (the workflow
/// reference gives `f` as 0.00–0.30).
///
/// AGRONOMIC_NOTE: this is a guard on the model's output, not a target it
/// is expected to reach. Temperature and water alone cannot carry a soil
/// from the 1.5% average to 30% — the top of that range belongs to fresh
/// labile organic inputs (green manure, recent residue incorporation,
/// manure), which this engine has no data about. A climate-only model
/// that reached 0.30 would be lying about how much it knows.
pub const MAX_MINERALIZATION_FACTOR: f64 = 0.30;

/// Temperature at which the mineralization response equals 1.0, i.e.
/// where [`BASELINE_MINERALIZATION_FACTOR`] applies unmodified.
///
/// AGRONOMIC_NOTE: 20 °C, not the 25 °C this used to assume. 25 °C is the
/// rough *optimum* for tropical mineralization, and anchoring an average
/// at an optimum makes every site on Earth read below average. 20 °C is a
/// mean growing-season temperature for which 1.5%/yr is a defensible
/// textbook figure. This is a calibration knob: move it if field
/// measurements say otherwise.
const MINERALIZATION_REFERENCE_TEMP_C: f64 = 20.0;

/// Q10 — how many times faster the microbial population mineralizes per
/// 10 °C of warming. 2.0 is the standard soil-biology value.
const MINERALIZATION_Q10: f64 = 2.0;

/// Floor of the water response. Even a soil in seasonal drought or under
/// standing water mineralizes during the spells when it is neither, so
/// the moisture term narrows the rate rather than switching it off.
const WATER_RESPONSE_FLOOR: f64 = 0.5;

/// Response lost per unit of aridity index above 1.0, i.e. how fast
/// waterlogging suppresses aerobic mineralization once rainfall exceeds
/// evaporative demand.
const WATERLOGGING_DECLINE_PER_INDEX: f64 = 0.25;

/// Mass of dry soil per hectare down to the arable depth, in kg/ha.
/// `bulk_density_kg_dm3` is DAP (apparent density, kg/dm3 == g/cm3).
pub fn soil_weight_kg_ha(bulk_density_kg_dm3: f64, arable_depth_m: f64) -> f64 {
    bulk_density_kg_dm3 * arable_depth_m * 10_000_000.0
}

/// Nutrient available in the soil, in kg/ha, from a concentration
/// already expressed in mg/kg.
pub fn availability_kg_ha(concentration_mg_kg: f64, soil_weight_kg_ha: f64) -> f64 {
    concentration_mg_kg * soil_weight_kg_ha / 1_000_000.0
}

/// Total nitrogen content of the soil, as a percent, estimated from
/// organic matter (MO): `N_total = MO / 20`.
pub fn nitrogen_total_percent(organic_matter_percent: f64) -> f64 {
    organic_matter_percent / 20.0
}

/// Nitrogen available to the crop this cycle, in kg/ha, from organic
/// matter mineralization: `N_ASIM = N_total * f * wha / 100`.
///
/// `mineralization_factor` is the fraction of total soil N assumed to
/// mineralize annually (e.g. 0.015 for 1.5%). Unlike other nutrients, N
/// has no soil-test-based availability path — it's derived entirely from
/// MO, matching the prototype (`n.py`) and the workflow reference.
pub fn nitrogen_available_kg_ha(organic_matter_percent: f64, mineralization_factor: f64, soil_weight_kg_ha: f64) -> f64 {
    nitrogen_total_percent(organic_matter_percent) / 100.0 * mineralization_factor * soil_weight_kg_ha
}

/// Total crop removal/uptake for the yield target, in kg/ha, given a
/// reference coefficient expressed per unit of yield (e.g. kg N per t_ha).
pub fn crop_removal_kg_ha(coefficient_kg_per_yield_unit: f64, yield_value: f64) -> f64 {
    coefficient_kg_per_yield_unit * yield_value
}

/// Net fertilizer requirement, in kg/ha: the gap between what the crop
/// needs and what the soil already supplies, inflated by the fraction of
/// applied nutrient the crop can actually use (`efficiency_fraction`,
/// e.g. 0.5 for 50%). Never negative.
pub fn net_requirement_kg_ha(demand_kg_ha: f64, availability_kg_ha: f64, efficiency_fraction: f64) -> f64 {
    let gap = demand_kg_ha - availability_kg_ha;
    (gap / efficiency_fraction).max(0.0)
}

/// Product dose, in kg of commercial product per ha, needed to deliver
/// `net_requirement_kg_ha` of a nutrient present at `nutrient_pct_in_source`
/// percent by weight.
///
/// `None` when the grade is not a usable divisor. Callers reach this
/// through `highest_rated`, which already drops non-positive and NaN
/// grades — but that coupling is invisible from here, and the identical
/// unguarded division in `net_requirement_kg_ha` printed `inf kg/ha Urea`
/// as a recommendation until session 9 caught it. The guard lives in the
/// function so a second call site cannot reintroduce it.
pub fn dose_kg_product_ha(net_requirement_kg_ha: f64, nutrient_pct_in_source: f64) -> Option<f64> {
    (nutrient_pct_in_source > 0.0).then(|| net_requirement_kg_ha / (nutrient_pct_in_source / 100.0))
}

/// Effective cation exchange capacity (CICE), cmolc/kg: the sum of acid
/// and base cations actually present, as opposed to `FieldContext`'s
/// `cec_cmolc_kg` (CIC at pH 7, a different standard measurement — see
/// the workflow reference for why the two are kept distinct).
pub fn cation_exchange_capacity_effective(h: f64, al: f64, k: f64, mg: f64, ca: f64) -> f64 {
    h + al + k + mg + ca
}

/// Current base saturation, as a percent of CICE held by K⁺/Mg²⁺/Ca²⁺.
pub fn base_saturation_pct(k: f64, mg: f64, ca: f64, cice: f64) -> f64 {
    if cice <= 0.0 {
        return 0.0;
    }
    (k + mg + ca) / cice * 100.0
}

/// Lime requirement, in t CaCO3-eq/ha, from exchangeable Al³⁺ toxicity.
/// `al_factor` is a literature constant (e.g. ~1.5 for tropical soils —
/// see `LimingRulesRepository`), not derived here.
pub fn lime_requirement_from_aluminum_t_ha(al_cmolc_kg: f64, al_factor: f64) -> f64 {
    (al_factor * al_cmolc_kg).max(0.0)
}

/// Lime requirement, in t CaCO3-eq/ha, to raise base saturation from its
/// current value to `target_base_saturation_pct`. Never negative — a soil
/// already at or above target needs no lime by this method.
pub fn lime_requirement_from_base_saturation_t_ha(cic_cmolc_kg: f64, current_base_saturation_pct: f64, target_base_saturation_pct: f64) -> f64 {
    (cic_cmolc_kg * (target_base_saturation_pct - current_base_saturation_pct) / 100.0).max(0.0)
}

/// Neutralizing value (EQ) of a liming material, as % CaCO3-equivalent,
/// from its CaO/MgO content.
pub fn neutralizing_value_pct(cao_pct: f64, mgo_pct: f64) -> f64 {
    cao_pct * 1.79 + mgo_pct * 2.48
}

/// PRNT (relative total neutralizing power): a material's neutralizing
/// value discounted by how much of it is fine enough to actually react.
pub fn prnt(neutralizing_value_pct: f64, granulometric_efficiency_pct: f64) -> f64 {
    neutralizing_value_pct * granulometric_efficiency_pct / 100.0
}

/// Product dose, in t of liming material per ha, needed to deliver
/// `caco3_eq_required_t_ha` given a material at `prnt_pct` PRNT.
///
/// `None` for a material with no neutralizing power — see
/// [`dose_kg_product_ha`] for why the guard is here and not at the call
/// site.
pub fn lime_material_dose_t_ha(caco3_eq_required_t_ha: f64, prnt_pct: f64) -> Option<f64> {
    (prnt_pct > 0.0).then(|| caco3_eq_required_t_ha / (prnt_pct / 100.0))
}

/// Aluminum saturation, as the percent of CICE held by exchangeable Al³⁺.
///
/// AGRONOMIC_NOTE: the figure crop Al tolerance is actually stated
/// against, and the one Tabla 12's acidity diagnosis keys on. A soil can
/// carry 2 cmolc/kg of Al harmlessly at high CICE and toxically at low
/// CICE, so the absolute Al reading on its own does not say whether roots
/// are being damaged.
pub fn aluminum_saturation_pct(al_cmolc_kg: f64, cice_cmolc_kg: f64) -> f64 {
    if cice_cmolc_kg <= 0.0 {
        return 0.0;
    }
    (al_cmolc_kg / cice_cmolc_kg * 100.0).clamp(0.0, 100.0)
}

/// The five cation balance ratios of Tabla 12's "balance de bases" block.
///
/// AGRONOMIC_NOTE: these diagnose *antagonism*, which the per-nutrient
/// critical levels cannot see. Ca, Mg and K compete for the same root
/// uptake sites, so a soil can hold an adequate absolute level of every
/// one of them and still starve the crop of potassium because calcium and
/// magnesium crowd it out. That is why a plan can report K as "medium"
/// against its own threshold while (Ca+Mg)/K says the crop will not get
/// it.
///
/// Every input must be in cmolc/kg — these are charge-equivalent ratios,
/// and computing them from mg/kg gives a different number with the same
/// shape, which is the worst kind of wrong.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CationRatios {
    pub ca_to_mg: Option<f64>,
    pub mg_to_k: Option<f64>,
    pub k_to_mg: Option<f64>,
    pub ca_to_k: Option<f64>,
    pub ca_plus_mg_to_k: Option<f64>,
}

pub fn cation_ratios(ca_cmolc_kg: f64, mg_cmolc_kg: f64, k_cmolc_kg: f64) -> CationRatios {
    // A cation the lab did not report arrives here as 0.0, and a ratio
    // over it is not "infinitely unbalanced", it is unknown.
    let ratio = |numerator: f64, denominator: f64| (denominator > 0.0).then(|| numerator / denominator);
    CationRatios {
        ca_to_mg: ratio(ca_cmolc_kg, mg_cmolc_kg),
        mg_to_k: ratio(mg_cmolc_kg, k_cmolc_kg),
        k_to_mg: ratio(k_cmolc_kg, mg_cmolc_kg),
        ca_to_k: ratio(ca_cmolc_kg, k_cmolc_kg),
        ca_plus_mg_to_k: ratio(ca_cmolc_kg + mg_cmolc_kg, k_cmolc_kg),
    }
}

/// The band of a qualitative interpretation table containing `value`, or
/// `None` where the table names none — see [`QualitativeBand`].
pub fn classify_band(bands: &[QualitativeBand], value: f64) -> Option<&QualitativeBand> {
    bands.iter().find(|band| band.contains(value))
}

// ---- Climate-modulated agronomy --------------------------------------------
// Everything below takes plain f64s already resolved by an adapter. No IO
// happens here; `AgroclimaticRepository` fetches, these functions decide.

/// Temperature response of mineralization, 1.0 at
/// [`MINERALIZATION_REFERENCE_TEMP_C`], doubling per 10 °C.
///
/// AGRONOMIC_NOTE: Q10 replaces the linear `T / 25` this used to be.
/// Microbial rates are exponential in temperature, and the linear form
/// got the direction right but the magnitude wrong at both ends — it
/// under-penalised cold soils and over-rewarded hot ones.
fn temperature_response(mean_temp_c: f64) -> f64 {
    MINERALIZATION_Q10.powf((mean_temp_c - MINERALIZATION_REFERENCE_TEMP_C) / 10.0)
}

/// Moisture response of mineralization against the aridity index
/// (annual precipitation / annual ET0), peaking at 1.0 where rainfall
/// just meets evaporative demand.
///
/// AGRONOMIC_NOTE: unimodal, unlike the monotonic `P / 800` this used to
/// be. Wetter is only better up to the point where the profile saturates;
/// past it, anoxia stops the aerobic decomposition that mineralizes N, so
/// a 3000 mm/yr site must not read as three times better than an 800 mm
/// one. Using P/ET0 rather than raw millimetres is what makes the same
/// rainfall mean "wet" in the highlands and "dry" in the lowlands.
fn water_response(aridity_index: f64) -> f64 {
    let response = if aridity_index <= 1.0 {
        aridity_index
    } else {
        1.0 - WATERLOGGING_DECLINE_PER_INDEX * (aridity_index - 1.0)
    };
    response.clamp(WATER_RESPONSE_FLOOR, 1.0)
}

/// Annual N mineralization factor for a site:
/// `f = 0.015 * temperature_response * water_response`, held inside the
/// admissible `0.0 ..= `[`MAX_MINERALIZATION_FACTOR`].
///
/// `None` means the climatology lacks what the model needs, and the
/// caller should fall back to [`BASELINE_MINERALIZATION_FACTOR`] — a
/// partial climatology is not obviously better than the documented
/// average. Water only limits under rainfed conditions: elsewhere the
/// grower supplies what the rain doesn't, so an irrigated lot needs
/// temperature alone.
pub fn mineralization_factor(climate: &AnnualClimatology, irrigation: &IrrigationSystem) -> Option<f64> {
    let temperature = temperature_response(climate.mean_temp_c?);
    let water = match irrigation {
        IrrigationSystem::Rainfed => {
            // A zero or absent ET0 would divide the aridity index into
            // infinity or NaN; neither is a soil moisture regime.
            let et0 = climate.annual_et0_mm().filter(|mm| *mm > 0.0)?;
            water_response(climate.annual_precip_mm()? / et0)
        }
        _ => 1.0,
    };

    Some((BASELINE_MINERALIZATION_FACTOR * temperature * water).clamp(0.0, MAX_MINERALIZATION_FACTOR))
}

/// Dimensionless radiation use efficiency index: how much of a reference
/// 18 MJ/m²/day of incoming solar radiation the site actually receives.
///
/// Informational only — nothing in the plan scales by it yet.
/// TODO(gap): consume this in a real yield-gap use case, where a
/// radiation-limited site should have its *yield target* questioned
/// rather than its fertilizer dose adjusted.
pub fn rue_index(solar_mj_m2_per_day: f64) -> f64 {
    (solar_mj_m2_per_day / 18.0).clamp(0.3, 1.0)
}

/// Reference evapotranspiration (mm/day) by the FAO-56 Hargreaves
/// equation: `ET0 = 0.0023 * Ra * (Tmean + 17.8) * sqrt(Tmax - Tmin)`.
///
/// `extraterrestrial_mj_m2_per_day` is Ra (top-of-atmosphere radiation),
/// converted to mm/day water equivalent by the standard 0.408 factor
/// (1 / 2.45 MJ per mm of vaporized water).
///
/// AGRONOMIC_NOTE: Hargreaves is FAO-56's sanctioned fallback for when
/// the full Penman-Monteith inputs aren't available. It needs only
/// temperature and Ra, and uses the diurnal range as a proxy for
/// cloudiness and humidity. Feed it a *same-period* Tmax/Tmin pair: using
/// an annual extreme spread instead of a within-month range inflates the
/// square-root term and overstates ET0.
pub fn reference_et0_hargreaves_mm_day(mean_temp_c: f64, max_temp_c: f64, min_temp_c: f64, extraterrestrial_mj_m2_per_day: f64) -> f64 {
    let range = (max_temp_c - min_temp_c).max(0.0);
    (0.0023 * (extraterrestrial_mj_m2_per_day * 0.408) * (mean_temp_c + 17.8) * range.sqrt()).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soil_weight_matches_reference_prototype() {
        // DAP 1.3 kg/dm3, 0.2 m arable depth -> 2,600,000 kg/ha.
        assert_eq!(soil_weight_kg_ha(1.3, 0.2), 2_600_000.0);
    }

    #[test]
    fn availability_scales_with_concentration() {
        let weight = soil_weight_kg_ha(1.3, 0.2);
        // 20 mg/kg over 2,600,000 kg/ha of soil -> 52 kg/ha.
        assert_eq!(availability_kg_ha(20.0, weight), 52.0);
    }

    #[test]
    fn net_requirement_is_never_negative() {
        assert_eq!(net_requirement_kg_ha(50.0, 200.0, 0.5), 0.0);
    }

    #[test]
    fn net_requirement_applies_efficiency() {
        // demand 150, availability 50 -> gap 100, at 50% efficiency -> 200.
        assert_eq!(net_requirement_kg_ha(150.0, 50.0, 0.5), 200.0);
    }

    #[test]
    fn dose_scales_by_grade() {
        // 100 kg N/ha net, urea at 46% N -> ~217.4 kg product/ha.
        assert!((dose_kg_product_ha(100.0, 46.0).expect("a real grade") - 217.391).abs() < 0.01);
        // A grade of zero would have divided to infinity and printed
        // `inf kg/ha Urea` as a recommendation.
        assert_eq!(dose_kg_product_ha(100.0, 0.0), None);
        assert_eq!(dose_kg_product_ha(100.0, f64::NAN), None);
    }

    #[test]
    fn nitrogen_available_matches_reference_prototype() {
        // MO 3.2%, 1.5% mineralization, soil weight 2,600,000 kg/ha
        // (LOT-001's numbers) -> 62.4 kg N/ha, matching n.py's n_asimilable.
        let weight = soil_weight_kg_ha(1.3, 0.2);
        assert!((nitrogen_available_kg_ha(3.2, 0.015, weight) - 62.4).abs() < 0.01);
    }

    #[test]
    fn cice_sums_all_cations() {
        // H 0.3 + Al 1.5 + K 0.18 + Mg 0.8 + Ca 3.0 -> 5.78 cmolc/kg.
        assert!((cation_exchange_capacity_effective(0.3, 1.5, 0.18, 0.8, 3.0) - 5.78).abs() < 1e-9);
    }

    #[test]
    fn base_saturation_is_fraction_of_cice() {
        // (0.18+0.8+3.0)/5.78 * 100 -> ~68.86%.
        assert!((base_saturation_pct(0.18, 0.8, 3.0, 5.78) - 68.858).abs() < 0.01);
    }

    #[test]
    fn base_saturation_is_zero_when_cice_is_zero() {
        assert_eq!(base_saturation_pct(0.0, 0.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn lime_from_aluminum_scales_by_factor() {
        // Al 1.5 cmolc/kg, Kamprath factor 1.5 -> 2.25 t CaCO3-eq/ha.
        assert!((lime_requirement_from_aluminum_t_ha(1.5, 1.5) - 2.25).abs() < 1e-9);
    }

    #[test]
    fn lime_from_base_saturation_is_never_negative() {
        // Already above target -> no lime needed by this method.
        assert_eq!(lime_requirement_from_base_saturation_t_ha(18.0, 90.0, 80.0), 0.0);
    }

    #[test]
    fn lime_from_base_saturation_scales_by_gap() {
        // CIC 18.0, current SB 68.858%, target 80% -> 18*(80-68.858)/100 ~ 2.006 t/ha.
        assert!((lime_requirement_from_base_saturation_t_ha(18.0, 68.858, 80.0) - 2.006).abs() < 0.01);
    }

    #[test]
    fn neutralizing_value_from_oxide_content() {
        // CaO 30%, MgO 18% -> 30*1.79 + 18*2.48 -> 98.34.
        assert!((neutralizing_value_pct(30.0, 18.0) - 98.34).abs() < 1e-9);
    }

    #[test]
    fn prnt_discounts_by_granulometric_efficiency() {
        // EQ 97.14%, EG 90% -> 87.426.
        assert!((prnt(97.14, 90.0) - 87.426).abs() < 1e-9);
    }

    #[test]
    fn lime_material_dose_scales_by_prnt() {
        // 2.25 t CaCO3-eq/ha needed, material at 87.426% PRNT -> ~2.574 t/ha.
        assert!((lime_material_dose_t_ha(2.25, 87.426).expect("a real PRNT") - 2.574).abs() < 0.01);
        assert_eq!(lime_material_dose_t_ha(2.25, 0.0), None, "a material that neutralizes nothing has no dose");
    }

    // ---- Climate-modulated agronomy ----

    /// Builds a rainfed climatology from annual totals, so the tests read
    /// in the units the agronomy is stated in rather than mm/day.
    fn climate(mean_temp_c: f64, annual_precip_mm: f64, annual_et0_mm: f64) -> AnnualClimatology {
        AnnualClimatology {
            mean_temp_c: Some(mean_temp_c),
            precip_mm_per_day: Some(annual_precip_mm / 365.0),
            et0_mm_per_day: Some(annual_et0_mm / 365.0),
            ..Default::default()
        }
    }

    #[test]
    fn mineralization_factor_matches_pasto_climatology() {
        // NASA POWER at LOT-001's coordinates (1.2136, -77.2811): T2M
        // 13.17 C, 1036.6 mm/yr rain against 1495 mm/yr ET0, rainfed.
        // temperature = 2^((13.17-20)/10) = 0.6229
        // aridity     = 1036.6/1495.0     = 0.6934 -> water = 0.6934
        // 0.015 * 0.6229 * 0.6934 -> ~0.006478.
        let f = mineralization_factor(&climate(13.17, 1036.6, 1495.0), &IrrigationSystem::Rainfed).expect("full climatology");
        assert!((f - 0.006478).abs() < 1e-5, "got {f}");
        // A cold, water-limited Andean lot must mineralize *less* than the
        // flat average the engine assumes when it knows nothing.
        assert!(f < BASELINE_MINERALIZATION_FACTOR);
    }

    #[test]
    fn mineralization_factor_is_the_baseline_at_reference_conditions() {
        // 20 C, rainfall exactly meeting evaporative demand: both
        // responses are 1.0, so the model reproduces the documented
        // average instead of drifting off it.
        let f = mineralization_factor(&climate(20.0, 1000.0, 1000.0), &IrrigationSystem::Rainfed).expect("full climatology");
        assert!((f - BASELINE_MINERALIZATION_FACTOR).abs() < 1e-12, "got {f}");
    }

    #[test]
    fn mineralization_factor_doubles_every_ten_degrees() {
        let cool = mineralization_factor(&climate(20.0, 1000.0, 1000.0), &IrrigationSystem::Rainfed).expect("climatology");
        let warm = mineralization_factor(&climate(30.0, 1000.0, 1000.0), &IrrigationSystem::Rainfed).expect("climatology");
        assert!((warm / cool - MINERALIZATION_Q10).abs() < 1e-12, "{warm} / {cool}");
    }

    #[test]
    fn waterlogging_suppresses_mineralization_instead_of_boosting_it() {
        // The bug this replaces: the old `P / 800` term rose without
        // bound, so a soaking site read as the *best* case. Past the point
        // where rain meets demand, anoxia has to pull the rate back down.
        let optimum = mineralization_factor(&climate(20.0, 1000.0, 1000.0), &IrrigationSystem::Rainfed).expect("climatology");
        let drowned = mineralization_factor(&climate(20.0, 3000.0, 1000.0), &IrrigationSystem::Rainfed).expect("climatology");
        assert!(drowned < optimum, "{drowned} should be below {optimum}");
        // ...but never to zero: the floor keeps it a narrowing, not a switch.
        assert!(drowned >= BASELINE_MINERALIZATION_FACTOR * WATER_RESPONSE_FLOOR);
    }

    #[test]
    fn mineralization_factor_ignores_rainfall_when_irrigated() {
        // Same temperature, same (irrelevant) drought: water pinned to 1.0.
        let dry = mineralization_factor(&climate(20.0, 100.0, 2000.0), &IrrigationSystem::Drip).expect("climatology");
        let wet = mineralization_factor(&climate(20.0, 3000.0, 800.0), &IrrigationSystem::Drip).expect("climatology");
        assert_eq!(dry, wet);
        assert!((dry - BASELINE_MINERALIZATION_FACTOR).abs() < 1e-12);
    }

    #[test]
    fn mineralization_factor_never_exceeds_the_admissible_maximum() {
        // No real grid cell reaches this; the clamp exists so a garbled
        // one cannot hand the plan a physically impossible rate.
        let absurd = mineralization_factor(&climate(100.0, 1000.0, 1000.0), &IrrigationSystem::Drip).expect("climatology");
        assert_eq!(absurd, MAX_MINERALIZATION_FACTOR);
    }

    #[test]
    fn a_climatology_missing_what_the_model_needs_yields_no_factor() {
        // Absent data must fall back to the baseline, never to a number
        // computed from a hole. An irrigated lot is the exception: it
        // genuinely needs nothing but temperature.
        let no_temp = AnnualClimatology { precip_mm_per_day: Some(3.0), et0_mm_per_day: Some(4.0), ..Default::default() };
        assert!(mineralization_factor(&no_temp, &IrrigationSystem::Rainfed).is_none());

        let no_water = AnnualClimatology { mean_temp_c: Some(20.0), ..Default::default() };
        assert!(mineralization_factor(&no_water, &IrrigationSystem::Rainfed).is_none());
        assert!(mineralization_factor(&no_water, &IrrigationSystem::Drip).is_some());

        let zero_et0 = climate(20.0, 1000.0, 0.0);
        assert!(mineralization_factor(&zero_et0, &IrrigationSystem::Rainfed).is_none());
    }

    #[test]
    fn rue_index_scales_and_clamps() {
        // Pasto's 14.5 MJ/m2/day against the 18 reference -> ~0.806.
        assert!((rue_index(14.5) - 0.80555).abs() < 1e-4);
        assert_eq!(rue_index(30.0), 1.0, "clamped at the bright end");
        assert_eq!(rue_index(1.0), 0.3, "clamped at the dark end");
    }

    #[test]
    fn hargreaves_et0_matches_pasto_climatology() {
        // Pasto monthly means: T 13.17, Tmax 20.9, Tmin 5.9, Ra 35.9
        // MJ/m2/day -> 0.0023 * 14.647 * 30.97 * sqrt(15) ~ 4.04 mm/day.
        let et0 = reference_et0_hargreaves_mm_day(13.17, 20.9, 5.9, 35.9);
        assert!((et0 - 4.04).abs() < 0.05, "got {et0}");
    }

    #[test]
    fn hargreaves_et0_survives_an_inverted_range() {
        // A garbled grid cell (Tmax < Tmin) must yield 0, not NaN.
        let et0 = reference_et0_hargreaves_mm_day(13.0, 5.0, 20.0, 35.9);
        assert_eq!(et0, 0.0);
    }
}
