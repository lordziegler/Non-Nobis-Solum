//! Nutrient use efficiency, adjusted for the conditions of a real site.
//!
//! `efficiency_rules.yaml` gives a **base** range per nutrient: what a crop
//! recovers of what is applied, under conditions nobody in the file states.
//! A sandy rainfed lot at pH 5.1 with 35% aluminium saturation does not
//! recover 40% of its nitrogen, and dividing the requirement by a figure
//! that says it does under-fertilizes exactly the sites that need the most.
//!
//! ```text
//! EF = clamp(EF_base x M_pH x M_texture x M_water x M_temperature
//!                    x M_acidity x M_source,  EF_floor, EF_ceiling)
//! ```
//!
//! Every `M` is a multiplier from a band table below, every band cites the
//! literature it comes from, and every one that fires is carried on the
//! result so the report can print it. Nothing here is fitted, learned or
//! scored: it is a lookup a person can check against a book.
//!
//! # What this replaced
//!
//! A flat additive `-0.05` on the top of the range for each of three
//! climate stressors, which the project's own documentation called
//! "uncalibrated round rules of thumb". It priced heat, drought and
//! waterlogging and nothing else — not pH, not texture, not aluminium —
//! and it moved the range end rather than the figure the dose divides by.
//!
//! # Bibliography
//!
//! Every rule group below names one of these at its band table.
//!
//! - **Havlin, J.L., Tisdale, S.L., Nelson, W.L. & Beaton, J.D. (2014).
//!   *Soil Fertility and Fertilizers: An Introduction to Nutrient
//!   Management*, 8th ed. Pearson.** The pH/availability relationship for
//!   P (Al/Fe phosphate precipitation below ~5.5, Ca phosphate above
//!   ~7.5), ammonia volatilization from urea and ammonium sources at high
//!   pH, sulfate adsorption on Fe/Al oxides in acid soils, and the
//!   texture-driven leaching contrasts. A copy is in
//!   `docs/Literature/papers/`.
//! - **Cameron, K.C., Di, H.J. & Moir, J.L. (2013). Nitrogen losses from
//!   the soil/plant system: a review. *Annals of Applied Biology*
//!   162:145-173.** Nitrate leaching on coarse-textured soils and
//!   denitrification losses on poorly aerated fine-textured or waterlogged
//!   soils, with the magnitudes behind the N texture and water bands.
//! - **Barber, S.A. (1995). *Soil Nutrient Bioavailability: A Mechanistic
//!   Approach*, 2nd ed. Wiley.** Phosphorus reaches the root almost
//!   entirely by diffusion through soil water, so its supply falls with
//!   water content — the basis of the water-deficit penalty on P.
//! - **Germida, J.J. & Janzen, H.H. (1993). Factors affecting the
//!   oxidation of elemental sulfur in soils. *Fertilizer Research*
//!   35:101-114.** Elemental S must be microbially oxidized to sulfate
//!   before a crop can use it, and the rate depends on temperature,
//!   moisture and particle size — the basis of the sulfur source-form
//!   table.
//! - **Kochian, L.V., Piñeros, M.A. & Hoekenga, O.A. (2005). The
//!   physiology, genetics and molecular biology of plant aluminum
//!   resistance and toxicity. *Plant and Soil* 274:175-195.** Exchangeable
//!   Al inhibits root elongation, which cuts the soil volume a crop can
//!   explore — the part of the aluminium penalty that is *not* already
//!   priced by acid pH.
//! - **Ladha, J.K., Pathak, H., Krupnik, T.J., Six, J. & van Kessel, C.
//!   (2005). Efficiency of fertilizer nitrogen in cereal production:
//!   retrospects and prospects. *Advances in Agronomy* 87:85-156.** The
//!   observed envelope of nitrogen recovery efficiency, behind the N floor
//!   and ceiling.
//! - **Syers, J.K., Johnston, A.E. & Curtin, D. (2008). *Efficiency of
//!   soil and fertilizer phosphorus use.* FAO Fertilizer and Plant
//!   Nutrition Bulletin 18.** First-season phosphorus recovery, behind the
//!   P floor and ceiling.
//! - **Dobermann, A., Witt, C., Dawe, D. et al. (2002). Site-specific
//!   nutrient management for intensive rice cropping systems in Asia.
//!   *Field Crops Research* 74:37-66.** The general case for adjusting
//!   nutrient recommendations to measured site conditions rather than to a
//!   regional blanket figure, which is what this module is.

use super::errors::DomainError;
use super::nutrient::Nutrient;
use super::value_objects::{IrrigationSystem, Texture};

/// Everything about a site that moves nutrient use efficiency.
///
/// Every field but `ph`, `texture` and `irrigation` is optional: a lot with
/// no climatology and no aluminium test still gets a plan, it just gets one
/// with fewer modifiers, and the report says which were unavailable rather
/// than assuming the site is fine.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioConditions {
    pub ph: f64,
    pub texture: Texture,
    pub irrigation: IrrigationSystem,
    /// Annual mean, °C.
    pub mean_temp_c: Option<f64>,
    /// Hottest month's mean daily maximum, °C — the window ammonia
    /// volatilization actually happens in.
    pub max_temp_c: Option<f64>,
    /// Annual rainfall divided by annual reference evapotranspiration.
    /// Below 1 the site is water-limited, well above it water is in excess.
    pub moisture_index: Option<f64>,
    /// Exchangeable Al as a percent of effective CEC (CICE).
    pub aluminium_saturation_pct: Option<f64>,
}

impl ScenarioConditions {
    /// The conditions of a site nothing is known to be wrong with: used as
    /// the reference point the band tables are written against, and by the
    /// tests that check a neutral site collects no penalties.
    pub fn reference(texture: Texture, irrigation: IrrigationSystem) -> Self {
        Self {
            ph: 6.5,
            texture,
            irrigation,
            mean_temp_c: Some(20.0),
            max_temp_c: Some(28.0),
            moisture_index: Some(1.0),
            aluminium_saturation_pct: Some(0.0),
        }
    }
}

/// Which sulfur carrier the plan will actually apply.
///
/// AGRONOMIC_NOTE: this is the one modifier that depends on the product
/// rather than the site, and it creates an ordering problem — efficiency
/// sizes the requirement, and the requirement is what picks the product.
/// The balance therefore runs on [`SulfurForm::Unstated`], which is treated
/// as sulfate and **reported as an assumption**; the formulation report
/// flags it afterwards if the blend it chose is elemental. Feeding the
/// chosen product back into the balance would be a fixed point, not a
/// calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SulfurForm {
    /// Immediately plant-available.
    Sulfate,
    /// Needs microbial oxidation first.
    Elemental,
    /// Nothing chosen yet, or a catalog with no form metadata.
    #[default]
    Unstated,
}

impl SulfurForm {
    pub fn as_str(self) -> &'static str {
        match self {
            SulfurForm::Sulfate => "sulfate",
            SulfurForm::Elemental => "elemental",
            SulfurForm::Unstated => "unstated",
        }
    }
}

impl std::fmt::Display for SulfurForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One condition that moved the efficiency, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct EfficiencyModifier {
    /// The multiplier applied. Always <= 1.0 today — every rule here is a
    /// penalty — but the clamp below is written for both directions so a
    /// future favourable rule needs no new plumbing.
    pub factor: f64,
    /// The measured condition, in the reader's units: "pH 5.3", "sandy
    /// loam", "mean 14.2 °C".
    pub condition: String,
    /// What it does to the nutrient, in one clause. From the rule row, so
    /// a profile that adds a band writes its own wording.
    pub effect: String,
    /// Short citation, as the rule row states it.
    pub basis: String,
}

/// One nutrient's efficiency, start to finish.
#[derive(Debug, Clone, PartialEq)]
pub struct AdjustedEfficiency {
    pub nutrient: Nutrient,
    /// Midpoint of the range `efficiency_rules.yaml` gives for this
    /// nutrient and this texture/irrigation class.
    pub base: f64,
    pub modifiers: Vec<EfficiencyModifier>,
    /// The figure the dose actually divides by.
    pub adjusted: f64,
    pub floor: f64,
    pub ceiling: f64,
    /// True when the product of the modifiers fell outside the bounds and
    /// the clamp caught it — worth saying, because it means the site is at
    /// the edge of what the literature has measured.
    pub clamped: bool,
    /// Assumptions the reader has to know the figure rests on.
    pub assumptions: Vec<String>,
}

impl AdjustedEfficiency {
    /// The product of every modifier, i.e. how much of the base survived.
    pub fn retained_fraction(&self) -> f64 {
        self.modifiers.iter().map(|m| m.factor).product()
    }

    /// "45% base, x0.85 sandy loam, x0.90 rainfed water deficit -> 34%".
    pub fn summary(&self) -> String {
        let mut text = format!("{:.0}% base", self.base * 100.0);
        for modifier in &self.modifiers {
            text.push_str(&format!(", x{:.2} {}", modifier.factor, modifier.condition));
        }
        text.push_str(&format!(" -> {:.0}%", self.adjusted * 100.0));
        if self.clamped {
            text.push_str(" (held at the bound)");
        }
        text
    }
}

// ---------------------------------------------------------------------
// The rules, as data
// ---------------------------------------------------------------------

/// Which site variable a band reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BandGroup {
    Ph,
    Texture,
    Water,
    TemperatureMean,
    TemperatureMax,
    Acidity,
    SourceForm,
}

impl BandGroup {
    pub const ALL: [BandGroup; 7] = [
        BandGroup::Ph,
        BandGroup::Texture,
        BandGroup::Water,
        BandGroup::TemperatureMean,
        BandGroup::TemperatureMax,
        BandGroup::Acidity,
        BandGroup::SourceForm,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BandGroup::Ph => "ph",
            BandGroup::Texture => "texture",
            BandGroup::Water => "water",
            BandGroup::TemperatureMean => "temperature_mean",
            BandGroup::TemperatureMax => "temperature_max",
            BandGroup::Acidity => "acidity",
            BandGroup::SourceForm => "source_form",
        }
    }
}

impl std::str::FromStr for BandGroup {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BandGroup::ALL
            .into_iter()
            .find(|group| group.as_str() == s.trim().to_lowercase())
            .ok_or_else(|| DomainError::InvalidInput(format!("unknown efficiency band group: {s}")))
    }
}

/// One row of `efficiency_bands.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct EfficiencyBandRule {
    pub group: BandGroup,
    pub nutrient: Nutrient,
    /// Categorical filter — a texture class, `rainfed`/`any`, a product
    /// form. `None` matches anything.
    pub class: Option<String>,
    /// Half-open `[min, max)` on the group's driving variable, the same
    /// convention `soil_quality_thresholds.csv` uses. Both `None` marks the
    /// row that fires when the variable is *unknown*.
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub factor: f64,
    pub effect: String,
    pub basis: String,
}

impl EfficiencyBandRule {
    fn is_unknown_value_row(&self) -> bool {
        self.min.is_none() && self.max.is_none()
    }

    fn covers(&self, value: f64) -> bool {
        self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value < max)
    }

    fn matches_class(&self, class: Option<&str>) -> bool {
        match (&self.class, class) {
            (None, _) => true,
            (Some(wanted), Some(actual)) => wanted == actual || wanted == "any",
            (Some(wanted), None) => wanted == "any",
        }
    }
}

/// The whole per-profile table: the bands, the floors, and the one
/// threshold that is logic rather than a band.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EfficiencyBandRules {
    pub bands: Vec<EfficiencyBandRule>,
    pub floors: Vec<(Nutrient, f64)>,
    /// Below this pH the `ph` group already charged phosphorus for Al/Fe
    /// phosphate fixation, so `acidity` applies half its penalty.
    pub acid_ph_already_priced: f64,
}

impl EfficiencyBandRules {
    /// The first row of `group` for `nutrient` whose class matches and whose
    /// interval contains `value`.
    ///
    /// `value` of `None` means the site has no reading for this group's
    /// variable, and then only a row with no interval at all can fire.
    /// Anything else would let a missing measurement be read as a condition.
    fn find(
        &self,
        group: BandGroup,
        nutrient: Nutrient,
        class: Option<&str>,
        value: Option<f64>,
    ) -> Option<&EfficiencyBandRule> {
        self.bands
            .iter()
            .filter(|rule| rule.group == group && rule.nutrient == nutrient && rule.matches_class(class))
            .find(|rule| match value {
                Some(value) => !rule.is_unknown_value_row() && rule.covers(value),
                None => rule.is_unknown_value_row(),
            })
    }

    fn modifier(
        &self,
        group: BandGroup,
        nutrient: Nutrient,
        class: Option<&str>,
        value: Option<f64>,
        condition: String,
    ) -> Option<EfficiencyModifier> {
        let rule = self.find(group, nutrient, class, value)?;
        Some(EfficiencyModifier {
            factor: rule.factor,
            condition,
            effect: rule.effect.clone(),
            basis: rule.basis.clone(),
        })
    }

    /// The lowest efficiency the model will claim for a nutrient. A nutrient
    /// with no floor row falls back to the most pessimistic one in the
    /// table, which is a bound, not a number about that nutrient.
    pub fn floor(&self, nutrient: Nutrient) -> f64 {
        self.floors
            .iter()
            .find(|(n, _)| *n == nutrient)
            .map(|(_, floor)| *floor)
            .unwrap_or_else(|| self.floors.iter().map(|(_, f)| *f).fold(f64::INFINITY, f64::min).min(0.05))
    }
}

/// The three classes the loss processes actually distinguish. USDA's twelve
/// classes are more resolution than any of the cited magnitudes support.
pub fn texture_class(texture: Texture) -> &'static str {
    match texture {
        Texture::Sand | Texture::LoamySand | Texture::SandyLoam => "coarse",
        Texture::Loam | Texture::SiltLoam | Texture::Silt | Texture::SandyClayLoam => "medium",
        Texture::ClayLoam | Texture::SiltyClayLoam | Texture::SandyClay | Texture::SiltyClay | Texture::Clay => "fine",
    }
}

/// `EF = clamp(base x M_pH x M_texture x M_water x M_temp x M_acidity x
/// M_source, floor, ceiling)`.
///
/// `base` is the midpoint of the range `efficiency_rules.yaml` gives, and
/// `ceiling` its top: conditions can only take efficiency away from what
/// the reference table says is achievable for the class, never add to it.
/// The ceiling is unreachable while every band is a penalty, and is kept
/// because it stops being unreachable the day a profile ships a favourable
/// one.
///
/// Every modifier comes from `rules`, which the application layer loaded
/// from the active profile. Nothing here reads a file and nothing here
/// carries a threshold.
pub fn adjust(
    nutrient: Nutrient,
    base: f64,
    ceiling: f64,
    conditions: &ScenarioConditions,
    sulfur_form: SulfurForm,
    rules: &EfficiencyBandRules,
) -> AdjustedEfficiency {
    let mut modifiers: Vec<EfficiencyModifier> = Vec::new();

    if let Some(modifier) = rules.modifier(
        BandGroup::Ph,
        nutrient,
        None,
        Some(conditions.ph),
        format!("pH {:.1}", conditions.ph),
    ) {
        modifiers.push(modifier);
    }

    if let Some(modifier) = rules.modifier(
        BandGroup::Texture,
        nutrient,
        Some(texture_class(conditions.texture)),
        // Texture is categorical: it has no interval, so it is matched on
        // class alone and its rows carry no min/max.
        None,
        conditions.texture.to_string(),
    ) {
        modifiers.push(modifier);
    }

    if let Some(index) = conditions.moisture_index {
        let class = if conditions.irrigation == IrrigationSystem::Rainfed { "rainfed" } else { "any" };
        if let Some(modifier) = rules.modifier(
            BandGroup::Water,
            nutrient,
            Some(class),
            Some(index),
            format!("{class}, rain/ET0 {index:.2}"),
        ) {
            modifiers.push(modifier);
        }
    }

    // Temperature is charged once: a hit on the hottest month stops the
    // annual mean from also firing. A site reported as both freezing on
    // average and scorching in one month pays the volatilization penalty
    // and not also the cold one — the inputs contradict each other, but the
    // rule has to be stated rather than left to whichever row is written
    // first.
    let hot = conditions.max_temp_c.and_then(|max| {
        rules.modifier(
            BandGroup::TemperatureMax,
            nutrient,
            None,
            Some(max),
            format!("hottest month {max:.1} °C"),
        )
    });
    match hot {
        Some(modifier) => modifiers.push(modifier),
        None => {
            if let Some(mean) = conditions.mean_temp_c {
                if let Some(modifier) = rules.modifier(
                    BandGroup::TemperatureMean,
                    nutrient,
                    None,
                    Some(mean),
                    format!("mean {mean:.1} °C"),
                ) {
                    modifiers.push(modifier);
                }
            }
        }
    }

    if let Some(saturation) = conditions.aluminium_saturation_pct {
        if let Some(mut modifier) = rules.modifier(
            BandGroup::Acidity,
            nutrient,
            None,
            Some(saturation),
            format!("Al saturation {saturation:.0}%"),
        ) {
            // The double-count seam. Two mechanisms sit on the same soil:
            // acid pH precipitates phosphate (chemistry, already priced by
            // the `ph` group below `acid_ph_already_priced`), and Al
            // inhibits root elongation (physiology, a function of Al
            // saturation that no pH band prices). Where the first has
            // fired, only the second is left, so the penalty is halved —
            // the penalty, not the factor.
            if conditions.ph < rules.acid_ph_already_priced && nutrient == Nutrient::P {
                modifier.factor = 1.0 - (1.0 - modifier.factor) / 2.0;
                modifier.effect =
                    "root elongation inhibited by exchangeable Al (the fixation half is already charged to pH)"
                        .to_string();
            }
            modifiers.push(modifier);
        }
    }

    if sulfur_form != SulfurForm::Unstated {
        let condition = match conditions.mean_temp_c {
            Some(temp) => format!("{sulfur_form} S at mean {temp:.1} °C"),
            None => format!("{sulfur_form} S, site temperature unknown"),
        };
        if let Some(modifier) = rules.modifier(
            BandGroup::SourceForm,
            nutrient,
            Some(sulfur_form.as_str()),
            conditions.mean_temp_c,
            condition,
        ) {
            modifiers.push(modifier);
        }
    }

    let floor = rules.floor(nutrient);
    let ceiling = ceiling.max(floor);
    let raw = base * modifiers.iter().map(|m| m.factor).product::<f64>();
    let adjusted = raw.clamp(floor, ceiling);

    let mut assumptions = Vec::new();
    if nutrient == Nutrient::S && sulfur_form == SulfurForm::Unstated {
        assumptions.push(
            "Sulfur is assumed to be applied as sulfate, which is immediately available. An elemental S product \
             is worth 10-40% less depending on site temperature, and the product recommendation flags it if one \
             is chosen."
                .to_string(),
        );
    }
    if conditions.mean_temp_c.is_none() {
        assumptions.push(
            "No climatology for this lot, so no temperature or water modifier could be applied. The figure is \
             not a statement that the site has neither problem."
                .to_string(),
        );
    }
    if conditions.aluminium_saturation_pct.is_none() {
        assumptions.push(
            "The sample reports no exchangeable Al, so no acidity modifier could be applied. Untested is not the \
             same as absent."
                .to_string(),
        );
    }

    AdjustedEfficiency {
        nutrient,
        base,
        modifiers,
        adjusted,
        floor,
        ceiling,
        clamped: (raw - adjusted).abs() > f64::EPSILON,
        assumptions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(
        group: BandGroup,
        nutrient: Nutrient,
        class: Option<&str>,
        min: Option<f64>,
        max: Option<f64>,
        factor: f64,
    ) -> EfficiencyBandRule {
        EfficiencyBandRule {
            group,
            nutrient,
            class: class.map(String::from),
            min,
            max,
            factor,
            effect: "effect".to_string(),
            basis: "basis".to_string(),
        }
    }

    /// A fixture with the same *shape* as the shipped table. The domain
    /// tests exercise lookup and composition; whether `global`'s own
    /// numbers parse is `toml_efficiency_bands_repo`'s job, and whether
    /// they reach a plan is the integration test's.
    fn rules() -> EfficiencyBandRules {
        use BandGroup::*;
        use Nutrient::*;
        EfficiencyBandRules {
            acid_ph_already_priced: 5.5,
            floors: vec![(N, 0.15), (P, 0.05), (K, 0.25), (S, 0.05), (Ca, 0.25), (Mg, 0.25)],
            bands: vec![
                band(Ph, P, None, None, Some(5.0), 0.70),
                band(Ph, P, None, Some(5.0), Some(5.5), 0.80),
                band(Ph, P, None, Some(5.5), Some(6.0), 0.92),
                band(Ph, P, None, Some(7.0), Some(7.5), 0.90),
                band(Ph, P, None, Some(7.5), None, 0.75),
                band(Ph, N, None, None, Some(5.0), 0.95),
                band(Ph, N, None, Some(7.0), Some(7.5), 0.95),
                band(Ph, N, None, Some(7.5), None, 0.88),
                band(Ph, S, None, None, Some(5.5), 0.90),
                band(Texture, N, Some("coarse"), None, None, 0.85),
                band(Texture, N, Some("fine"), None, None, 0.92),
                band(Texture, S, Some("coarse"), None, None, 0.85),
                band(Texture, K, Some("coarse"), None, None, 0.90),
                band(Texture, Ca, Some("coarse"), None, None, 0.90),
                band(Texture, Mg, Some("coarse"), None, None, 0.90),
                band(Water, P, Some("rainfed"), None, Some(0.75), 0.80),
                band(Water, N, Some("rainfed"), None, Some(0.75), 0.95),
                band(Water, K, Some("rainfed"), None, Some(0.75), 0.95),
                band(Water, N, Some("any"), Some(1.5), None, 0.85),
                band(Water, S, Some("any"), Some(1.5), None, 0.88),
                band(Water, K, Some("any"), Some(1.5), None, 0.92),
                band(TemperatureMax, N, None, Some(35.0), None, 0.90),
                band(TemperatureMean, N, None, None, Some(10.0), 0.85),
                band(TemperatureMean, N, None, Some(10.0), Some(15.0), 0.92),
                band(TemperatureMean, P, None, None, Some(10.0), 0.90),
                band(Acidity, P, None, Some(30.0), None, 0.85),
                band(Acidity, P, None, Some(15.0), Some(30.0), 0.92),
                band(Acidity, N, None, Some(30.0), None, 0.92),
                band(Acidity, N, None, Some(15.0), Some(30.0), 0.97),
                band(Acidity, K, None, Some(30.0), None, 0.92),
                band(SourceForm, S, Some("elemental"), None, Some(15.0), 0.60),
                band(SourceForm, S, Some("elemental"), Some(15.0), Some(20.0), 0.75),
                band(SourceForm, S, Some("elemental"), Some(20.0), None, 0.90),
                band(SourceForm, S, Some("elemental"), None, None, 0.75),
            ],
        }
    }

    fn neutral() -> ScenarioConditions {
        ScenarioConditions::reference(Texture::Loam, IrrigationSystem::Drip)
    }

    fn factor_of(nutrient: Nutrient, conditions: &ScenarioConditions, group: BandGroup) -> Option<f64> {
        adjust(nutrient, 0.40, 0.50, conditions, SulfurForm::Unstated, &rules())
            .modifiers
            .iter()
            .find(|m| {
                let table = rules();
                table.bands.iter().any(|rule| {
                    rule.group == group && rule.nutrient == nutrient && (rule.factor - m.factor).abs() < 1e-12
                })
            })
            .map(|m| m.factor)
    }

    /// The reference site collects nothing. If this ever fails, some band
    /// row has crept over the neutral point and every plan is quietly
    /// penalised.
    #[test]
    fn a_site_with_nothing_wrong_with_it_collects_no_penalty() {
        for nutrient in [Nutrient::N, Nutrient::P, Nutrient::K, Nutrient::S] {
            let result = adjust(nutrient, 0.40, 0.50, &neutral(), SulfurForm::Sulfate, &rules());
            assert!(result.modifiers.is_empty(), "{nutrient} picked up {:?}", result.modifiers);
            assert_eq!(result.adjusted, 0.40);
            assert!(!result.clamped);
        }
    }

    /// An empty table is not a table of 1.0s: it produces no modifiers, and
    /// the floor still has to come from somewhere.
    #[test]
    fn an_empty_table_leaves_the_base_alone_and_still_bounds_it() {
        let empty = EfficiencyBandRules { bands: Vec::new(), floors: vec![(Nutrient::N, 0.15)], acid_ph_already_priced: 5.5 };
        let hostile = ScenarioConditions { ph: 4.0, texture: Texture::Sand, ..neutral() };
        let result = adjust(Nutrient::N, 0.40, 0.50, &hostile, SulfurForm::Unstated, &empty);
        assert!(result.modifiers.is_empty());
        assert_eq!(result.adjusted, 0.40);
        assert_eq!(result.floor, 0.15);
    }

    #[test]
    fn ph_moves_phosphorus_in_both_directions_and_leaves_potassium_alone() {
        let at = |ph: f64| {
            let conditions = ScenarioConditions { ph, ..neutral() };
            factor_of(Nutrient::P, &conditions, BandGroup::Ph)
        };
        assert_eq!(at(4.5), Some(0.70));
        assert_eq!(at(5.2), Some(0.80));
        assert_eq!(at(5.8), Some(0.92));
        assert_eq!(at(6.5), None, "6.0-7.0 is the band of maximum availability");
        assert_eq!(at(7.2), Some(0.90));
        assert_eq!(at(8.0), Some(0.75), "calcareous soils fix P as Ca phosphates");

        let n_at = |ph: f64| factor_of(Nutrient::N, &ScenarioConditions { ph, ..neutral() }, BandGroup::Ph);
        assert_eq!(n_at(6.5), None);
        assert_eq!(n_at(8.0), Some(0.88));
        assert_eq!(n_at(4.5), Some(0.95));
        // Potassium never: what hurts K on acid soils is Al on the exchange
        // sites, and that is priced exactly once, under acidity.
        for ph in [4.0, 5.5, 6.5, 8.5] {
            assert_eq!(factor_of(Nutrient::K, &ScenarioConditions { ph, ..neutral() }, BandGroup::Ph), None);
        }
    }

    #[test]
    fn texture_penalises_the_mobile_nutrients_on_sand_and_nitrogen_on_heavy_clay() {
        let on = |texture, nutrient| {
            factor_of(nutrient, &ScenarioConditions { texture, ..neutral() }, BandGroup::Texture)
        };
        assert_eq!(on(Texture::Sand, Nutrient::N), Some(0.85));
        assert_eq!(on(Texture::Sand, Nutrient::S), Some(0.85));
        assert_eq!(on(Texture::Sand, Nutrient::K), Some(0.90));
        assert_eq!(on(Texture::Sand, Nutrient::P), None, "P fixation is pH chemistry, priced once under pH");
        // Ca and Mg ride with K: the same exchange sites, the same source.
        assert_eq!(on(Texture::Sand, Nutrient::Ca), Some(0.90));
        assert_eq!(on(Texture::Sand, Nutrient::Mg), Some(0.90));

        assert_eq!(on(Texture::Clay, Nutrient::N), Some(0.92));
        assert_eq!(on(Texture::Clay, Nutrient::K), None);
        for nutrient in [Nutrient::N, Nutrient::P, Nutrient::K, Nutrient::S] {
            assert_eq!(on(Texture::Loam, nutrient), None, "loam is the reference the ranges were written for");
        }
    }

    #[test]
    fn a_water_deficit_hits_phosphorus_hardest_and_only_when_rainfed() {
        let dry = |irrigation| {
            let conditions =
                ScenarioConditions { irrigation, moisture_index: Some(0.4), ..neutral() };
            factor_of(Nutrient::P, &conditions, BandGroup::Water)
        };
        assert_eq!(dry(IrrigationSystem::Rainfed), Some(0.80));
        assert_eq!(dry(IrrigationSystem::Drip), None, "an irrigated lot has no water deficit");

        // Excess water is a site fact, so it fires irrigated or not.
        for irrigation in [IrrigationSystem::Rainfed, IrrigationSystem::Drip] {
            let wet = ScenarioConditions { irrigation, moisture_index: Some(2.5), ..neutral() };
            assert_eq!(factor_of(Nutrient::N, &wet, BandGroup::Water), Some(0.85));
            assert_eq!(factor_of(Nutrient::S, &wet, BandGroup::Water), Some(0.88));
            assert_eq!(factor_of(Nutrient::P, &wet, BandGroup::Water), None);
        }
        // No climatology, no modifier — and no assumption that it is fine.
        let unknown = ScenarioConditions { moisture_index: None, ..neutral() };
        assert_eq!(factor_of(Nutrient::N, &unknown, BandGroup::Water), None);
    }

    #[test]
    fn cold_slows_nitrogen_and_phosphorus_and_heat_volatilises_nitrogen() {
        let cold = ScenarioConditions { mean_temp_c: Some(8.0), ..neutral() };
        assert_eq!(factor_of(Nutrient::N, &cold, BandGroup::TemperatureMean), Some(0.85));
        assert_eq!(factor_of(Nutrient::P, &cold, BandGroup::TemperatureMean), Some(0.90));
        assert_eq!(factor_of(Nutrient::K, &cold, BandGroup::TemperatureMean), None);
        // Sulfur is absent on purpose: temperature reaches S through
        // elemental oxidation, priced once as a source property.
        assert_eq!(factor_of(Nutrient::S, &cold, BandGroup::TemperatureMean), None);

        let cool = ScenarioConditions { mean_temp_c: Some(14.2), ..neutral() };
        assert_eq!(factor_of(Nutrient::N, &cool, BandGroup::TemperatureMean), Some(0.92));

        let hot = ScenarioConditions { max_temp_c: Some(36.0), ..neutral() };
        assert_eq!(factor_of(Nutrient::N, &hot, BandGroup::TemperatureMax), Some(0.90));
    }

    /// Temperature is charged once. A site both freezing on average and
    /// scorching in one month pays the volatilization penalty and not also
    /// the cold one.
    #[test]
    fn a_hot_month_stops_the_cold_annual_mean_from_also_firing() {
        let contradictory = ScenarioConditions { mean_temp_c: Some(8.0), max_temp_c: Some(36.0), ..neutral() };
        let result = adjust(Nutrient::N, 0.40, 0.50, &contradictory, SulfurForm::Unstated, &rules());
        let temperature: Vec<f64> = result
            .modifiers
            .iter()
            .filter(|m| m.condition.contains("°C"))
            .map(|m| m.factor)
            .collect();
        assert_eq!(temperature, vec![0.90], "one temperature term, not two: {:?}", result.modifiers);
    }

    #[test]
    fn elemental_sulfur_is_penalised_by_cold_and_sulfate_never_is() {
        let at = |temp, form| {
            let conditions = ScenarioConditions { mean_temp_c: temp, ..neutral() };
            adjust(Nutrient::S, 0.20, 0.30, &conditions, form, &rules())
                .modifiers
                .into_iter()
                .find(|m| m.condition.contains(" S"))
                .map(|m| m.factor)
        };
        assert_eq!(at(Some(12.0), SulfurForm::Elemental), Some(0.60));
        assert_eq!(at(Some(18.0), SulfurForm::Elemental), Some(0.75));
        assert_eq!(at(Some(25.0), SulfurForm::Elemental), Some(0.90));
        // Unknown temperature falls to the row with no interval — the
        // conservative middle band, not the best case.
        assert_eq!(at(None, SulfurForm::Elemental), Some(0.75));

        assert_eq!(at(Some(12.0), SulfurForm::Sulfate), None);
        assert_eq!(at(Some(12.0), SulfurForm::Unstated), None);
    }

    /// The double-counting seam, which is the whole reason this test exists.
    #[test]
    fn aluminium_is_not_charged_twice_when_the_ph_band_already_priced_it() {
        let acid = ScenarioConditions { ph: 5.0, aluminium_saturation_pct: Some(45.0), ..neutral() };
        let limed = ScenarioConditions { ph: 6.2, aluminium_saturation_pct: Some(45.0), ..neutral() };
        let al = |conditions: &ScenarioConditions, nutrient| {
            adjust(nutrient, 0.15, 0.20, conditions, SulfurForm::Unstated, &rules())
                .modifiers
                .into_iter()
                .find(|m| m.condition.starts_with("Al saturation"))
                .map(|m| m.factor)
        };

        // pH 6.2 is out of the acid band, so nothing has priced the Al yet.
        assert_eq!(al(&limed, Nutrient::P), Some(0.85));
        // pH 5.0 already charged P for Al/Fe phosphate fixation; only the
        // root-growth half is left, so the penalty is halved: 0.15 -> 0.075.
        assert_eq!(al(&acid, Nutrient::P), Some(0.925));
        // Nitrogen's acid pH band is about nitrification, not about Al, so
        // there is nothing to discount.
        assert_eq!(al(&acid, Nutrient::N), Some(0.92));
        assert_eq!(al(&limed, Nutrient::N), Some(0.92));

        // The combined P figure stays above a naive double charge.
        let combined = adjust(Nutrient::P, 0.15, 0.20, &acid, SulfurForm::Unstated, &rules());
        assert!((combined.retained_fraction() - 0.80 * 0.925).abs() < 1e-9);
        assert!(combined.retained_fraction() > 0.80 * 0.85);
    }

    /// The threshold is a datum: a profile that sets it to 0 charges both
    /// mechanisms in full.
    #[test]
    fn the_double_count_discount_is_a_profile_setting() {
        let acid = ScenarioConditions { ph: 5.0, aluminium_saturation_pct: Some(45.0), ..neutral() };
        let mut no_discount = rules();
        no_discount.acid_ph_already_priced = 0.0;

        let discounted = adjust(Nutrient::P, 0.15, 0.20, &acid, SulfurForm::Unstated, &rules());
        let full = adjust(Nutrient::P, 0.15, 0.20, &acid, SulfurForm::Unstated, &no_discount);
        assert!(full.retained_fraction() < discounted.retained_fraction());
        assert!((full.retained_fraction() - 0.80 * 0.85).abs() < 1e-9);
    }

    #[test]
    fn a_sample_with_no_aluminium_test_gets_no_modifier_and_says_so() {
        let untested = ScenarioConditions { aluminium_saturation_pct: None, ..neutral() };
        let result = adjust(Nutrient::P, 0.15, 0.20, &untested, SulfurForm::Sulfate, &rules());
        assert!(result.modifiers.iter().all(|m| !m.condition.starts_with("Al saturation")));
        assert!(result.assumptions.iter().any(|a| a.contains("no exchangeable Al")));
    }

    #[test]
    fn modifiers_compound_multiplicatively_across_conditions() {
        let hard = ScenarioConditions {
            ph: 5.3,
            texture: Texture::Sand,
            irrigation: IrrigationSystem::Rainfed,
            mean_temp_c: Some(14.2),
            max_temp_c: Some(24.0),
            moisture_index: Some(0.6),
            aluminium_saturation_pct: Some(5.0),
        };

        let nitrogen = adjust(Nutrient::N, 0.40, 0.50, &hard, SulfurForm::Unstated, &rules());
        // sand 0.85 x rainfed deficit 0.95 x mean 14.2 C 0.92; pH 5.3 is
        // above the 5.0 nitrification band, so no pH term.
        assert_eq!(nitrogen.modifiers.len(), 3);
        assert!((nitrogen.adjusted - 0.40 * 0.85 * 0.95 * 0.92).abs() < 1e-9);
        assert!(nitrogen.summary().contains("x0.85 sand"));

        let phosphorus = adjust(Nutrient::P, 0.15, 0.20, &hard, SulfurForm::Unstated, &rules());
        // pH 5.3 0.80 x water deficit 0.80; no texture term, no Al term at
        // 5% saturation, no cold term above 10 C.
        assert_eq!(phosphorus.modifiers.len(), 2);
        assert!((phosphorus.adjusted - 0.15 * 0.80 * 0.80).abs() < 1e-9);
    }

    #[test]
    fn the_floor_stops_a_stack_of_penalties_from_driving_the_dose_to_infinity() {
        let worst = ScenarioConditions {
            ph: 4.2,
            texture: Texture::Sand,
            irrigation: IrrigationSystem::Rainfed,
            mean_temp_c: Some(8.0),
            max_temp_c: Some(36.0),
            moisture_index: Some(0.3),
            aluminium_saturation_pct: Some(60.0),
        };

        // pH 4.2 0.70 x deficit 0.80 x cold 0.90 x Al 0.925 (halved) = 0.466.
        let phosphorus = adjust(Nutrient::P, 0.15, 0.20, &worst, SulfurForm::Unstated, &rules());
        assert!((phosphorus.retained_fraction() - 0.4662).abs() < 1e-4);
        // Worth pinning: even the worst site does *not* reach the floor from
        // the middle of the P range. A floor that clamped routinely would be
        // doing the modifiers' job for them.
        assert!(!phosphorus.clamped);

        // From the bottom of the same range it does, and says so.
        let pessimistic = adjust(Nutrient::P, 0.10, 0.20, &worst, SulfurForm::Unstated, &rules());
        assert_eq!(pessimistic.adjusted, rules().floor(Nutrient::P));
        assert!(pessimistic.clamped);

        let nitrogen = adjust(Nutrient::N, 0.40, 0.50, &worst, SulfurForm::Unstated, &rules());
        assert!(nitrogen.adjusted >= rules().floor(Nutrient::N));
    }

    /// A profile that lowers a floor changes what the clamp allows, with no
    /// code change at all. This is the point of the whole file.
    #[test]
    fn a_profile_that_lowers_a_floor_lets_the_dose_go_further() {
        let worst = ScenarioConditions {
            ph: 4.2,
            texture: Texture::Sand,
            irrigation: IrrigationSystem::Rainfed,
            mean_temp_c: Some(8.0),
            max_temp_c: Some(30.0),
            moisture_index: Some(0.3),
            aluminium_saturation_pct: Some(60.0),
        };
        let mut andean = rules();
        andean.floors = vec![(Nutrient::P, 0.03)];
        // ...and the harder fixation band an ash soil justifies.
        for rule in andean.bands.iter_mut() {
            if rule.group == BandGroup::Ph && rule.nutrient == Nutrient::P && rule.max == Some(5.0) {
                rule.factor = 0.62;
            }
        }

        let global = adjust(Nutrient::P, 0.10, 0.20, &worst, SulfurForm::Unstated, &rules());
        let regional = adjust(Nutrient::P, 0.10, 0.20, &worst, SulfurForm::Unstated, &andean);
        assert!(regional.adjusted < global.adjusted, "{} vs {}", regional.adjusted, global.adjusted);
        assert_eq!(regional.floor, 0.03);
    }

    #[test]
    fn the_ceiling_holds_even_if_a_band_ever_turns_favourable() {
        let mut result = adjust(Nutrient::N, 0.60, 0.50, &neutral(), SulfurForm::Sulfate, &rules());
        assert_eq!(result.adjusted, 0.50, "a base above its own range top is still capped");
        assert!(result.clamped);
        result = adjust(Nutrient::N, 0.40, 0.01, &neutral(), SulfurForm::Sulfate, &rules());
        assert_eq!(result.adjusted, rules().floor(Nutrient::N));
    }

    #[test]
    fn an_unstated_sulfur_form_is_treated_as_sulfate_and_reported() {
        let result = adjust(Nutrient::S, 0.20, 0.30, &neutral(), SulfurForm::Unstated, &rules());
        assert!(result.modifiers.is_empty());
        assert!(result.assumptions.iter().any(|a| a.contains("assumed to be applied as sulfate")));
    }

    #[test]
    fn calcium_and_magnesium_only_carry_the_rule_the_table_states_for_them() {
        let sandy = ScenarioConditions { ph: 4.5, texture: Texture::Sand, ..neutral() };
        for nutrient in [Nutrient::Ca, Nutrient::Mg] {
            let result = adjust(nutrient, 0.50, 0.60, &sandy, SulfurForm::Unstated, &rules());
            // Exactly one: the exchange-capacity row. No pH row, because
            // the table states none and none was invented.
            assert_eq!(result.modifiers.len(), 1, "{nutrient}: {:?}", result.modifiers);
            assert!((result.adjusted - 0.50 * 0.90).abs() < 1e-9);
        }
    }

    #[test]
    fn a_missing_floor_falls_back_to_the_most_pessimistic_one_in_the_table() {
        let table = rules();
        assert_eq!(table.floor(Nutrient::Zn), 0.05, "an unlisted nutrient gets the table's own bound");
    }
}
