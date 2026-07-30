//! Pure domain services: no IO, no repositories, just the agronomic math.
//! Ported and generalized from the `Non-Nobis-Solum-Py` prototype
//! (n.py/p.py/k.py availability and net-requirement formulas).

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
}
