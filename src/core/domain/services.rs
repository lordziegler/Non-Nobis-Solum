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
}
