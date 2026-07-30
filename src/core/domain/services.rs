//! Pure domain services: no IO, no repositories, just the agronomic math.
//! Ported and generalized from the `Non-Nobis-Solum-Py` prototype
//! (n.py/p.py/k.py availability and net-requirement formulas).

use super::entities::AnnualClimatology;
use super::value_objects::IrrigationSystem;

/// Fraction of total soil N assumed to mineralize annually, absent any
/// climate data. The prototype's hardcoded 1.5% (`n.py`) — now the
/// *baseline* that [`mineralization_factor`] modulates rather than the
/// only available value.
pub const BASELINE_MINERALIZATION_FACTOR: f64 = 0.015;

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
pub fn dose_kg_product_ha(net_requirement_kg_ha: f64, nutrient_pct_in_source: f64) -> f64 {
    net_requirement_kg_ha / (nutrient_pct_in_source / 100.0)
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
pub fn lime_material_dose_t_ha(caco3_eq_required_t_ha: f64, prnt_pct: f64) -> f64 {
    caco3_eq_required_t_ha / (prnt_pct / 100.0)
}

// ---- Climate-modulated agronomy --------------------------------------------
// Everything below takes plain f64s already resolved by an adapter. No IO
// happens here; `AgroclimaticRepository` fetches, these functions decide.

/// Annual N mineralization factor, modulating
/// [`BASELINE_MINERALIZATION_FACTOR`] by temperature and water:
/// `f = 0.015 * T_factor * W_factor`.
///
/// AGRONOMIC_NOTE: mineralization is microbial, so it tracks soil warmth
/// and moisture. The 25 °C reference is the rough optimum for tropical
/// mineralization; an Andean lot near 10 °C therefore yields ~0.4x the
/// tropical baseline, which is the agronomically expected direction.
/// Water only limits under rainfed conditions — under irrigation the
/// grower supplies what the rain doesn't, so `W_factor` is pinned at 1.0.
/// Both factors are clamped: this is a coarse correction, and letting an
/// extreme grid cell swing N availability by an order of magnitude would
/// be false precision.
pub fn mineralization_factor(mean_temp_c: f64, annual_precip_mm: f64, irrigation: &IrrigationSystem) -> f64 {
    let t_factor = (mean_temp_c / 25.0).clamp(0.5, 2.0);
    let w_factor = match irrigation {
        IrrigationSystem::Rainfed => (annual_precip_mm / 800.0).clamp(0.5, 1.5),
        _ => 1.0,
    };
    BASELINE_MINERALIZATION_FACTOR * t_factor * w_factor
}

/// Deltas to apply to the *upper* bound of a nutrient's efficiency range.
/// Always negative or zero: climate signals here can only narrow the
/// optimistic end of the range, never widen it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EfficiencyAdjustment {
    pub n_max_delta: f64,
    pub p_max_delta: f64,
    pub k_max_delta: f64,
}

/// How much a single unambiguous climate signal narrows an efficiency
/// range. One conservative step; these are not calibrated coefficients.
// ponytail: one flat magnitude for all three signals, and they don't
// compound. Split per-signal if field data ever justifies distinct
// weights.
const EFFICIENCY_PENALTY: f64 = 0.05;

/// Narrows nutrient use efficiency where the climatology shows an
/// unambiguous stress signal. Returns all-zero deltas when the relevant
/// variables are absent — missing data must never be read as a stressor.
pub fn efficiency_climate_adjustment(climate: &AnnualClimatology, irrigation: &IrrigationSystem) -> EfficiencyAdjustment {
    let mut adjustment = EfficiencyAdjustment::default();
    let rainfed = matches!(irrigation, IrrigationSystem::Rainfed);

    // AGRONOMIC_NOTE: above ~35 °C nitrification slows while ammonia
    // volatilization from surface-applied urea accelerates, so less of the
    // applied N ever reaches the crop. Keyed on the hottest month rather
    // than the annual mean: the loss happens during that window regardless
    // of how mild the rest of the year is.
    if climate.max_temp_c.is_some_and(|t| t > 35.0) {
        adjustment.n_max_delta -= EFFICIENCY_PENALTY;
    }

    // AGRONOMIC_NOTE: P moves to the root by diffusion through soil water.
    // Under a sustained water deficit the film thins, diffusion slows, and
    // fertilizer P stays put near the granule instead of reaching the
    // crop. Only meaningful when rainfall is the sole water supply — an
    // irrigated lot has no such deficit by definition.
    if rainfed {
        if let (Some(et0), Some(precip)) = (climate.annual_et0_mm(), climate.annual_precip_mm()) {
            if et0 > 1.5 * precip {
                adjustment.p_max_delta -= EFFICIENCY_PENALTY;
            }
        }
    }

    // AGRONOMIC_NOTE: K⁺ is a weakly held exchangeable cation. Under very
    // high rainfall it leaches below the root zone, and waterlogged
    // (anaerobic) soil impairs the root's ability to take up what remains.
    if rainfed && climate.annual_precip_mm().is_some_and(|p| p > 2000.0) {
        adjustment.k_max_delta -= EFFICIENCY_PENALTY;
    }

    adjustment
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
        assert!((dose_kg_product_ha(100.0, 46.0) - 217.391).abs() < 0.01);
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
        assert!((lime_material_dose_t_ha(2.25, 87.426) - 2.574).abs() < 0.01);
    }

    // ---- Climate-modulated agronomy ----

    #[test]
    fn mineralization_factor_matches_pasto_climatology() {
        // NASA POWER at LOT-001's coordinates (1.2136, -77.2811): T2M
        // 13.17 C, 2.84 mm/day -> 1036.6 mm/yr, rainfed.
        // T_factor = 13.17/25 = 0.5268; W_factor = 1036.6/800 = 1.29575.
        // 0.015 * 0.5268 * 1.29575 -> ~0.0102390.
        let f = mineralization_factor(13.17, 1036.6, &IrrigationSystem::Rainfed);
        assert!((f - 0.0102390).abs() < 1e-6, "got {f}");
        // A cold Andean lot must mineralize *less* than the flat baseline.
        assert!(f < BASELINE_MINERALIZATION_FACTOR);
    }

    #[test]
    fn mineralization_factor_ignores_rainfall_when_irrigated() {
        // Same temperature, same (irrelevant) drought: W_factor pinned to 1.0.
        let dry = mineralization_factor(25.0, 100.0, &IrrigationSystem::Drip);
        let wet = mineralization_factor(25.0, 3000.0, &IrrigationSystem::Drip);
        assert_eq!(dry, wet);
        // T_factor is exactly 1.0 at the 25 C reference, so f == baseline.
        assert!((dry - BASELINE_MINERALIZATION_FACTOR).abs() < 1e-12);
    }

    #[test]
    fn mineralization_factor_clamps_both_extremes() {
        // Arctic + desert, rainfed: floors at 0.5 * 0.5 of baseline.
        let floor = mineralization_factor(-30.0, 0.0, &IrrigationSystem::Rainfed);
        assert!((floor - BASELINE_MINERALIZATION_FACTOR * 0.25).abs() < 1e-12);
        // Hellish + soaking, rainfed: ceilings at 2.0 * 1.5 of baseline.
        let ceiling = mineralization_factor(80.0, 9000.0, &IrrigationSystem::Rainfed);
        assert!((ceiling - BASELINE_MINERALIZATION_FACTOR * 3.0).abs() < 1e-12);
    }

    #[test]
    fn no_climate_data_means_no_efficiency_adjustment() {
        // Absent data must never be read as a stress signal.
        let empty = AnnualClimatology::default();
        assert_eq!(
            efficiency_climate_adjustment(&empty, &IrrigationSystem::Rainfed),
            EfficiencyAdjustment::default()
        );
    }

    #[test]
    fn each_climate_stressor_narrows_only_its_own_nutrient() {
        // Heat only: 36 C in the hottest month -> N penalised, P/K untouched.
        let hot = AnnualClimatology { max_temp_c: Some(36.0), ..Default::default() };
        let adjustment = efficiency_climate_adjustment(&hot, &IrrigationSystem::Rainfed);
        assert_eq!(adjustment, EfficiencyAdjustment { n_max_delta: -0.05, p_max_delta: 0.0, k_max_delta: 0.0 });

        // Water deficit only: ET0 1500 mm/yr vs 600 mm/yr rain (ratio 2.5
        // > 1.5) -> P penalised. Precip is well under the 2000 mm
        // waterlogging threshold, so K stays clean.
        let dry = AnnualClimatology {
            precip_mm_per_day: Some(600.0 / 365.0),
            et0_mm_per_day: Some(1500.0 / 365.0),
            ..Default::default()
        };
        let adjustment = efficiency_climate_adjustment(&dry, &IrrigationSystem::Rainfed);
        assert_eq!(adjustment, EfficiencyAdjustment { n_max_delta: 0.0, p_max_delta: -0.05, k_max_delta: 0.0 });

        // Waterlogging only: 2500 mm/yr -> K penalised.
        let wet = AnnualClimatology { precip_mm_per_day: Some(2500.0 / 365.0), ..Default::default() };
        let adjustment = efficiency_climate_adjustment(&wet, &IrrigationSystem::Rainfed);
        assert_eq!(adjustment, EfficiencyAdjustment { n_max_delta: 0.0, p_max_delta: 0.0, k_max_delta: -0.05 });
    }

    #[test]
    fn water_rules_only_fire_under_rainfed() {
        // Same soaking-wet, high-demand climatology that penalises P and K
        // above must leave an irrigated lot alone — the deficit and the
        // leaching are both artefacts of relying on rainfall.
        let extreme = AnnualClimatology {
            precip_mm_per_day: Some(2500.0 / 365.0),
            et0_mm_per_day: Some(9000.0 / 365.0),
            ..Default::default()
        };
        assert_eq!(
            efficiency_climate_adjustment(&extreme, &IrrigationSystem::Drip),
            EfficiencyAdjustment::default()
        );
    }

    #[test]
    fn pasto_climatology_triggers_no_efficiency_penalty() {
        // The real LOT-001 grid cell: 22.59 C hottest month (< 35), ET0
        // 1495 mm vs 1037 mm rain (ratio 1.44 < 1.5), 1037 mm (< 2000).
        // Every signal lands below its threshold, so no rule fires.
        let pasto = AnnualClimatology {
            mean_temp_c: Some(13.17),
            max_temp_c: Some(22.59),
            min_temp_c: Some(4.42),
            precip_mm_per_day: Some(2.84),
            solar_mj_m2_per_day: Some(14.5),
            et0_mm_per_day: Some(4.096),
            ..Default::default()
        };
        assert_eq!(
            efficiency_climate_adjustment(&pasto, &IrrigationSystem::Rainfed),
            EfficiencyAdjustment::default()
        );
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
