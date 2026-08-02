//! Commercial formulation: turning per-nutrient net requirements into
//! products a grower can actually buy.
//!
//! Everything here is pure. The one thing this module cannot do for
//! itself is unit conversion — the elemental/oxide factors live in
//! `conversion_factors.toml` behind `ConversionFactorsRepository`, so the
//! use case converts and hands this module figures that are already on the
//! **visible commercial basis** (N, P2O5, K2O, S).
//!
//! ## Why a second nutrient type
//!
//! The catalog stores P and K elementally (`Nutrient::P` = 20.0756 for
//! DAP), while every bag, every grade and every agronomist speaks
//! P2O5/K2O (DAP = 46% P2O5). The two differ by a factor of 2.29 and 1.20.
//! [`GradeNutrient`] exists so the compiler, not a comment, is what stops
//! an elemental figure being compared against a commercial grade.

use super::efficiency::AdjustedEfficiency;
use super::entities::FertilizerSource;
use super::value_objects::FertilizerForm;
use super::nutrient::Nutrient;

// ---------------------------------------------------------------------
// The visible commercial basis
// ---------------------------------------------------------------------

/// A nutrient as it is printed on a fertilizer bag.
///
/// Deliberately not [`Nutrient`]: `P` and `K` there are elemental.
///
/// # Ca and Mg here are not Ca and Mg in a liming material
///
/// The same two elements answer two different questions in this project and
/// live in two catalogs on purpose:
///
/// - **Here**, `CaO`/`MgO` is *nutrient supply* from a fertilizer source —
///   how much calcium or magnesium a crop gets from the bag. It competes
///   for space in a grade with N, P2O5 and K2O.
/// - In [`crate::core::domain::LimingMaterial`], `cao_pct`/`mgo_pct` is
///   *neutralizing value* — how much acidity a material corrects. A
///   material is chosen on PRNT, dosed in t/ha of CaCO3 equivalent, and
///   never enters this catalog.
///
/// A liming material must not be pushed into `fertilizer_sources.csv` to
/// make it visible to this heuristic. If a grower needs both, they get a
/// lime recommendation *and* a fertilizer program, which is what the plan
/// already prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GradeNutrient {
    N,
    P2O5,
    K2O,
    S,
    CaO,
    MgO,
}

impl GradeNutrient {
    /// Every nutrient the blend balances, covers and reports.
    pub const ALL: [GradeNutrient; 6] = [
        GradeNutrient::N,
        GradeNutrient::P2O5,
        GradeNutrient::K2O,
        GradeNutrient::S,
        GradeNutrient::CaO,
        GradeNutrient::MgO,
    ];

    /// The nutrients the *target commercial grade* is built from.
    ///
    /// AGRONOMIC_NOTE: narrower than [`Self::ALL`] on purpose. A compound
    /// product is specified, ordered and priced as N-P2O5-K2O(-S); no
    /// manufacturer sells against a six-term ratio, so folding Ca and Mg
    /// into the target grade would produce a specification nothing in any
    /// catalog can match, and the ratio search would chase it. Ca and Mg
    /// still enter everywhere it matters — coverage, cross-contributions,
    /// remainders, waste and the blend score — they simply do not distort
    /// the grade the compound is picked against.
    pub const GRADE_RATIO: [GradeNutrient; 4] =
        [GradeNutrient::N, GradeNutrient::P2O5, GradeNutrient::K2O, GradeNutrient::S];

    /// The element the catalog stores this as, and the unit conversion the
    /// use case must apply to move between the two.
    pub fn elemental(self) -> Nutrient {
        match self {
            GradeNutrient::N => Nutrient::N,
            GradeNutrient::P2O5 => Nutrient::P,
            GradeNutrient::K2O => Nutrient::K,
            GradeNutrient::S => Nutrient::S,
            GradeNutrient::CaO => Nutrient::Ca,
            GradeNutrient::MgO => Nutrient::Mg,
        }
    }

    /// `None` where the elemental and visible forms are the same substance
    /// — N and S need no conversion, P and K do. The strings are the keys
    /// `ConversionFactorsRepository` answers to.
    pub fn oxide_conversion(self) -> Option<(&'static str, &'static str)> {
        match self {
            GradeNutrient::P2O5 => Some(("P", "P2O5")),
            GradeNutrient::K2O => Some(("K", "K2O")),
            GradeNutrient::CaO => Some(("Ca", "CaO")),
            GradeNutrient::MgO => Some(("Mg", "MgO")),
            GradeNutrient::N | GradeNutrient::S => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GradeNutrient::N => "N",
            GradeNutrient::P2O5 => "P2O5",
            GradeNutrient::K2O => "K2O",
            GradeNutrient::S => "S",
            GradeNutrient::CaO => "CaO",
            GradeNutrient::MgO => "MgO",
        }
    }

    fn index(self) -> usize {
        match self {
            GradeNutrient::N => 0,
            GradeNutrient::P2O5 => 1,
            GradeNutrient::K2O => 2,
            GradeNutrient::S => 3,
            GradeNutrient::CaO => 4,
            GradeNutrient::MgO => 5,
        }
    }
}

impl std::fmt::Display for GradeNutrient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A percent-by-weight grade on the visible basis, or — with the same
/// shape and the same arithmetic — a bare ratio between nutrients.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CommercialGrade {
    values: [f64; 6],
}

impl CommercialGrade {
    pub fn new(n: f64, p2o5: f64, k2o: f64, s: f64) -> Self {
        Self { values: [n, p2o5, k2o, s, 0.0, 0.0] }
    }

    /// The full six-term grade, for a source that supplies Ca or Mg as a
    /// nutrient. See the note on [`GradeNutrient`] for why that is not the
    /// same thing as a liming material's CaO.
    pub fn with_bases(n: f64, p2o5: f64, k2o: f64, s: f64, cao: f64, mgo: f64) -> Self {
        Self { values: [n, p2o5, k2o, s, cao, mgo] }
    }

    pub fn from_pairs(pairs: impl IntoIterator<Item = (GradeNutrient, f64)>) -> Self {
        let mut grade = Self::default();
        for (nutrient, value) in pairs {
            grade.values[nutrient.index()] = value;
        }
        grade
    }

    pub fn get(&self, nutrient: GradeNutrient) -> f64 {
        self.values[nutrient.index()]
    }

    pub fn set(&mut self, nutrient: GradeNutrient, value: f64) {
        self.values[nutrient.index()] = value;
    }

    /// Total nutrient content, the figure that decides whether a grade is
    /// physically possible at all (nothing can exceed 100%).
    pub fn sum(&self) -> f64 {
        self.values.iter().sum()
    }

    pub fn carried(&self) -> Vec<GradeNutrient> {
        GradeNutrient::ALL.into_iter().filter(|n| self.get(*n) > 0.0).collect()
    }

    /// `13-26-6` / `13-26-6-3S`. The S term is only printed when present,
    /// which is how a bag prints it.
    pub fn label(&self) -> String {
        let head = format!(
            "{}-{}-{}",
            round_label(self.get(GradeNutrient::N)),
            round_label(self.get(GradeNutrient::P2O5)),
            round_label(self.get(GradeNutrient::K2O))
        );
        let mut label = match self.get(GradeNutrient::S) {
            s if s > 0.0 => format!("{head}-{}S", round_label(s)),
            _ => head,
        };
        for (nutrient, suffix) in [(GradeNutrient::CaO, "CaO"), (GradeNutrient::MgO, "MgO")] {
            if self.get(nutrient) > 0.0 {
                label.push_str(&format!("-{}{suffix}", round_label(self.get(nutrient))));
            }
        }
        label
    }

    /// The comparison coefficients of PART C: N/P, P/K and — only when
    /// sulfur is in play — K/S.
    pub fn coefficients(&self) -> RatioCoefficients {
        let ratio = |a: GradeNutrient, b: GradeNutrient| {
            let (a, b) = (self.get(a), self.get(b));
            (b > 0.0 && a > 0.0).then(|| a / b)
        };
        RatioCoefficients {
            n_over_p: ratio(GradeNutrient::N, GradeNutrient::P2O5),
            p_over_k: ratio(GradeNutrient::P2O5, GradeNutrient::K2O),
            k_over_s: ratio(GradeNutrient::K2O, GradeNutrient::S),
        }
    }

    fn scaled(&self, factor: f64) -> Self {
        Self { values: self.values.map(|v| v * factor) }
    }

    /// Rounded to whole numbers, the form a grade is actually printed in.
    /// A component that is positive but rounds to zero is held at 1: a
    /// nutrient the crop asked for must not vanish into the rounding.
    fn discretized(&self) -> Self {
        Self {
            values: self.values.map(|v| match v {
                v if v <= 0.0 => 0.0,
                v => v.round().max(1.0),
            }),
        }
    }
}

fn round_label(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
    }
}

/// PART C's comparison coefficients. `None` wherever the denominator is
/// absent — a grade with no sulfur has no K/S, and inventing one would
/// make sulfur-free products look wrong against a sulfur-free target.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RatioCoefficients {
    pub n_over_p: Option<f64>,
    pub p_over_k: Option<f64>,
    pub k_over_s: Option<f64>,
}

impl RatioCoefficients {
    /// Mean relative distance over the coefficients both sides define, in
    /// `[0, 1)`. Relative rather than absolute because N/P lives near 1
    /// while P/K can be 10 — an absolute distance would let one coefficient
    /// silently own the score.
    ///
    /// Zero comparable coefficients answers 0.0: nothing is known about the
    /// shapes, so the ratio term abstains and the grade distance and the
    /// coverage term decide. Documented rather than hidden because it is
    /// the one input to the score that can be vacuously perfect.
    pub fn distance_to(&self, other: &Self) -> f64 {
        let pairs = [
            (self.n_over_p, other.n_over_p),
            (self.p_over_k, other.p_over_k),
            (self.k_over_s, other.k_over_s),
        ];
        let comparable: Vec<f64> = pairs
            .iter()
            .filter_map(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => Some((a - b).abs() / a.max(*b).max(f64::EPSILON)),
                _ => None,
            })
            .collect();
        if comparable.is_empty() {
            return 0.0;
        }
        comparable.iter().sum::<f64>() / comparable.len() as f64
    }
}

// ---------------------------------------------------------------------
// PART A — the target ratio, built from the net requirements
// ---------------------------------------------------------------------

/// One nutrient's net requirement on the visible basis, in kg/ha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NutrientRequirement {
    pub nutrient: GradeNutrient,
    pub kg_ha: f64,
}

/// PART A's rounding.
///
/// AMBIGUITY, resolved and pinned by test: "round to the nearest multiple
/// of ten" sends anything under 5 kg/ha to zero, and the workflow's own
/// worked example rounds a P2O5 of 3.2 to **10**, not to 0. A positive
/// requirement that rounds away takes its nutrient out of the target ratio
/// entirely, which is a different plan, not a rounder one. So the floor for
/// any positive requirement is one step, 10.
pub fn round_to_nearest_ten(kg_ha: f64) -> f64 {
    if kg_ha <= 0.0 {
        return 0.0;
    }
    ((kg_ha / 10.0).round() * 10.0).max(10.0)
}

/// One rung of PART B's ladder, kept so the report can show the search
/// rather than only its answer.
#[derive(Debug, Clone)]
pub struct GradeScalingStep {
    /// `x10`, `/2`, `/4`... relative to the normalized ratio.
    pub label: String,
    /// Before rounding to whole numbers.
    pub continuous: CommercialGrade,
    /// What a bag would actually print.
    pub discretized: CommercialGrade,
    pub rounding_distortion: f64,
    pub sum_penalty: f64,
    pub magnitude_penalty: f64,
    pub catalog_distance: f64,
    pub plausibility_penalty: f64,
    pub chosen: bool,
}

/// The whole PART A + PART B derivation, start to finish.
#[derive(Debug, Clone)]
pub struct RatioConstruction {
    pub original: Vec<NutrientRequirement>,
    pub rounded: Vec<NutrientRequirement>,
    pub smallest_rounded: f64,
    /// The normalized ratio, e.g. 4:5:1.
    pub normalized: CommercialGrade,
    pub steps: Vec<GradeScalingStep>,
    /// The grade every candidate is then scored against.
    pub target: CommercialGrade,
}

/// Total nutrient content bounds a bagged compound plausibly carries.
///
/// AGRONOMIC_NOTE: below 20% the product is mostly filler and is sold as an
/// amendment rather than as a grade; above 65% no granulated NPK exists —
/// the densest common straights (urea 46, KCl 60) are single-nutrient, and
/// a compound has to carry its own binder and coating. 100% is the
/// physical wall: a grade summing above it cannot be manufactured at all.
const MIN_PLAUSIBLE_GRADE_SUM: f64 = 20.0;
const MAX_PLAUSIBLE_GRADE_SUM: f64 = 65.0;

/// Weights of the plausibility penalty. Each answers "how many times worse
/// is this than a unit of the others", and each term is separately
/// reported so a reader can see which one decided.
const OUT_OF_BAND_WEIGHT: f64 = 4.0;
const ROUNDING_WEIGHT: f64 = 1.0;
const MAGNITUDE_WEIGHT: f64 = 0.5;
const CATALOG_AFFINITY_WEIGHT: f64 = 1.0;

/// How many halvings PART B tries after the initial x10. Six takes a
/// x10 ratio down by 64, well past the point where any component is still
/// a grade; the loop also stops on its own once nothing is left to halve.
const MAX_HALVINGS: usize = 6;

/// PART A + PART B: net requirements in, one target commercial grade out.
///
/// `catalog` is what makes criterion 5 ("prefer ratios that actually exist")
/// operative — the affinity term is zero when it is empty, and the search
/// then rests on the other three.
///
/// `None` when nothing is required at all.
pub fn build_target_grade(
    requirements: &[NutrientRequirement],
    catalog: &[CompositeCandidate],
) -> Option<RatioConstruction> {
    // Ca and Mg are filtered out here and only here: they are balanced by
    // the blend like everything else, but a compound product is specified
    // as N-P2O5-K2O(-S) and the target grade has to be something a catalog
    // can actually match. See `GradeNutrient::GRADE_RATIO`.
    let original: Vec<NutrientRequirement> = requirements
        .iter()
        .copied()
        .filter(|r| r.kg_ha > 0.0 && GradeNutrient::GRADE_RATIO.contains(&r.nutrient))
        .collect();
    if original.is_empty() {
        return None;
    }

    let rounded: Vec<NutrientRequirement> = original
        .iter()
        .map(|r| NutrientRequirement { nutrient: r.nutrient, kg_ha: round_to_nearest_ten(r.kg_ha) })
        .collect();
    let smallest_rounded = rounded.iter().map(|r| r.kg_ha).fold(f64::INFINITY, f64::min);
    if !smallest_rounded.is_normal() || smallest_rounded < 0.0 {
        return None;
    }

    let normalized =
        CommercialGrade::from_pairs(rounded.iter().map(|r| (r.nutrient, r.kg_ha / smallest_rounded)));

    // x10 first, then successive halvings — the workflow's own ladder.
    let mut steps: Vec<GradeScalingStep> = Vec::new();
    for halvings in 0..=MAX_HALVINGS {
        let factor = 10.0 / 2_f64.powi(halvings as i32);
        let continuous = normalized.scaled(factor);
        let discretized = continuous.discretized();
        let label = if halvings == 0 { "x10".to_string() } else { format!("/{}", 1 << halvings) };
        steps.push(evaluate_step(label, continuous, discretized, catalog));
        // Every component already at its floor: halving again only repeats
        // the same discretized grade.
        if continuous.carried().iter().all(|n| continuous.get(*n) <= 1.0) {
            break;
        }
    }

    let best = steps
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.plausibility_penalty.total_cmp(&b.plausibility_penalty))
        .map(|(index, _)| index)?;
    steps[best].chosen = true;
    let target = steps[best].discretized;

    Some(RatioConstruction { original, rounded, smallest_rounded, normalized, steps, target })
}

/// PART B's plausibility heuristic, one rung at a time.
///
/// Four terms, all reported:
/// 1. **rounding distortion** — how much the printed grade had to move to
///    become whole numbers, relative to its own size (criterion 1).
/// 2. **out-of-band penalty** — distance outside the manufacturable total
///    nutrient content (criteria 2 and 3).
/// 3. **magnitude penalty** — a mild pull toward compact grades inside the
///    band, because commercial practice states 13-26-6 and never 26-52-12
///    (criterion 3).
/// 4. **catalog affinity** — the ratio distance to the nearest product that
///    actually exists in the active profile's catalog (criterion 5).
///
/// Criterion 4 (agronomically consistent proportions) is structural rather
/// than a term: every rung is the *same* ratio direction, so no rung can
/// distort the balance except through its own rounding, which term 1
/// prices.
fn evaluate_step(
    label: String,
    continuous: CommercialGrade,
    discretized: CommercialGrade,
    catalog: &[CompositeCandidate],
) -> GradeScalingStep {
    let magnitude = continuous.sum().max(f64::EPSILON);
    let rounding_distortion = GradeNutrient::ALL
        .iter()
        .map(|n| (continuous.get(*n) - discretized.get(*n)).abs())
        .sum::<f64>()
        / magnitude;

    let sum = discretized.sum();
    let outside = (MIN_PLAUSIBLE_GRADE_SUM - sum).max(sum - MAX_PLAUSIBLE_GRADE_SUM).max(0.0);
    let sum_penalty = OUT_OF_BAND_WEIGHT * outside / MAX_PLAUSIBLE_GRADE_SUM;
    let magnitude_penalty = MAGNITUDE_WEIGHT * sum / MAX_PLAUSIBLE_GRADE_SUM;

    let coefficients = discretized.coefficients();
    let catalog_distance = catalog
        .iter()
        .map(|candidate| coefficients.distance_to(&candidate.grade.coefficients()))
        .fold(f64::INFINITY, f64::min);
    let catalog_distance = if catalog_distance.is_finite() { catalog_distance } else { 0.0 };

    let plausibility_penalty = sum_penalty
        + ROUNDING_WEIGHT * rounding_distortion
        + magnitude_penalty
        + CATALOG_AFFINITY_WEIGHT * catalog_distance;

    GradeScalingStep {
        label,
        continuous,
        discretized,
        rounding_distortion,
        sum_penalty,
        magnitude_penalty,
        catalog_distance,
        plausibility_penalty,
        chosen: false,
    }
}

// ---------------------------------------------------------------------
// PART D — scoring the compound candidates
// ---------------------------------------------------------------------

/// A catalog product as the formulation heuristic sees it: its grade on the
/// visible basis, plus whatever the catalog says about sourcing it.
#[derive(Debug, Clone)]
pub struct CompositeCandidate {
    pub source_id: String,
    pub name: String,
    pub grade: CommercialGrade,
    /// Read off the catalog's `form` column, never inferred from the name.
    pub form: FertilizerForm,
    /// 0.0 (freely available) to 1.0 (tightly restricted) — see
    /// [`commercialization_penalty`].
    pub commercialization_penalty: f64,
}

impl CompositeCandidate {
    /// A product is *compound* when it carries two or more of N/P2O5/K2O.
    ///
    /// Sulfur, calcium and magnesium do not count toward it: single
    /// superphosphate (P + S), ammonium sulfate (N + S) and kieserite
    /// (Mg + S) are straights that happen to carry a secondary nutrient,
    /// and calling them compounds would empty the simple-blend strategy of
    /// most of its catalog.
    pub fn is_compound(&self) -> bool {
        [GradeNutrient::N, GradeNutrient::P2O5, GradeNutrient::K2O]
            .iter()
            .filter(|n| self.grade.get(**n) > 0.0)
            .count()
            >= 2
    }
}

/// How hard the catalog says this product is to actually buy.
///
/// AMBIGUITY, resolved: the `restrictions` column is free text and the only
/// sourcing metadata the schema has. The Andean catalog writes a consistent
/// prefix into it ("Amplia comercialización...", "Uso restringido...") and
/// this reads exactly those markers; a catalog that says nothing scores
/// 0.0, because absent metadata is not evidence of a problem. Keyword
/// matching is accent- and case-insensitive on the stem, so
/// "comercializacion" and "comercialización" both hit.
pub fn commercialization_penalty(restrictions: &[String]) -> f64 {
    let text = restrictions.join(" ").to_lowercase().replace(['á', 'é', 'í', 'ó', 'ú'], "?");
    // Ordered worst-first: a product that is both regional and restricted
    // is scored on the restriction.
    for (marker, penalty) in [
        ("uso restringido", 0.5),
        ("restringid", 0.5),
        ("uso especializado", 0.3),
        ("comercializaci?n regional", 0.2),
        ("comercializaci?n com?n", 0.1),
        ("amplia comercializaci?n", 0.0),
    ] {
        if text.contains(marker) {
            return penalty;
        }
    }
    0.0
}

/// PART D's weights.
///
/// Coverage dominates deliberately: a product that simply does not carry a
/// nutrient the crop needs is a worse answer than one whose proportions are
/// merely off, because the first cannot be fixed by adjusting the dose.
const RATIO_WEIGHT: f64 = 1.0;
const GRADE_WEIGHT: f64 = 0.5;
const COVERAGE_WEIGHT: f64 = 2.0;
const COMMERCIALIZATION_WEIGHT: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct CompositeCandidateScore {
    pub candidate_id: String,
    pub candidate_name: String,
    pub candidate_grade: CommercialGrade,
    pub coefficients: RatioCoefficients,
    /// Distance between the candidate's N/P, P/K, K/S and the target's.
    pub ratio_distance: f64,
    /// L1 distance between the two grade vectors, relative to the target's
    /// own size. Counts nutrients the candidate carries and the plan did
    /// not ask for: they are paid for and applied either way.
    pub grade_distance: f64,
    /// Share of the required nutrients this product carries at all, 0-1.
    pub nutrient_coverage_score: f64,
    pub commercialization_penalty: f64,
    pub total_score: f64,
    pub explanation: String,
}

/// Scores one compound candidate against the target grade. Lower is better.
pub fn score_candidate(
    target: &CommercialGrade,
    required: &[GradeNutrient],
    candidate: &CompositeCandidate,
) -> CompositeCandidateScore {
    let coefficients = candidate.grade.coefficients();
    let ratio_distance = target.coefficients().distance_to(&coefficients);

    // Distance on the terms a grade is specified on. A candidate's Ca or Mg
    // is not a deviation from an N-P2O5-K2O target — it is a bonus or a
    // waste depending on the plan, and the blend score is where that is
    // priced, once.
    let target_size = GradeNutrient::GRADE_RATIO.iter().map(|n| target.get(*n)).sum::<f64>().max(f64::EPSILON);
    let grade_distance = GradeNutrient::GRADE_RATIO
        .iter()
        .map(|n| (target.get(*n) - candidate.grade.get(*n)).abs())
        .sum::<f64>()
        / target_size;

    let carried = required.iter().filter(|n| candidate.grade.get(**n) > 0.0).count();
    let nutrient_coverage_score =
        if required.is_empty() { 1.0 } else { carried as f64 / required.len() as f64 };
    let missing: Vec<&str> =
        required.iter().filter(|n| candidate.grade.get(**n) <= 0.0).map(|n| n.as_str()).collect();

    let total_score = RATIO_WEIGHT * ratio_distance
        + GRADE_WEIGHT * grade_distance
        + COVERAGE_WEIGHT * (1.0 - nutrient_coverage_score)
        + COMMERCIALIZATION_WEIGHT * candidate.commercialization_penalty;

    let explanation = format!(
        "grade {} · ratio distance {:.3} · grade distance {:.3} · covers {}/{} required{} · sourcing penalty {:.2}",
        candidate.grade.label(),
        ratio_distance,
        grade_distance,
        carried,
        required.len(),
        if missing.is_empty() { String::new() } else { format!(" (missing {})", missing.join("+")) },
        candidate.commercialization_penalty
    );

    CompositeCandidateScore {
        candidate_id: candidate.source_id.clone(),
        candidate_name: candidate.name.clone(),
        candidate_grade: candidate.grade,
        coefficients,
        ratio_distance,
        grade_distance,
        nutrient_coverage_score,
        commercialization_penalty: candidate.commercialization_penalty,
        total_score,
        explanation,
    }
}

/// Every compound candidate, best first. Ties break on `source_id` so the
/// same catalog always produces the same ranking, whatever order it loaded
/// in.
pub fn rank_candidates(
    target: &CommercialGrade,
    required: &[GradeNutrient],
    catalog: &[CompositeCandidate],
) -> Vec<CompositeCandidateScore> {
    let mut scored: Vec<CompositeCandidateScore> = catalog
        .iter()
        .filter(|candidate| candidate.is_compound())
        .map(|candidate| score_candidate(target, required, candidate))
        .filter(|score| score.total_score.is_finite())
        .collect();
    scored.sort_by(|a, b| a.total_score.total_cmp(&b.total_score).then_with(|| a.candidate_id.cmp(&b.candidate_id)));
    scored
}

// ---------------------------------------------------------------------
// PARTS E, F, G — dosing, remainders and the blend
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FertilizationStrategy {
    /// One compound product carries the bulk, straights close the gap.
    #[default]
    CompositePlusSimple,
    /// No compound at all: a physical blend of straights.
    SimpleBlendOnly,
}

impl FertilizationStrategy {
    pub const ALL: [FertilizationStrategy; 2] =
        [FertilizationStrategy::CompositePlusSimple, FertilizationStrategy::SimpleBlendOnly];

    pub fn as_str(self) -> &'static str {
        match self {
            FertilizationStrategy::CompositePlusSimple => "composite_plus_simple",
            FertilizationStrategy::SimpleBlendOnly => "simple_blend_only",
        }
    }

    pub fn other(self) -> Self {
        match self {
            FertilizationStrategy::CompositePlusSimple => FertilizationStrategy::SimpleBlendOnly,
            FertilizationStrategy::SimpleBlendOnly => FertilizationStrategy::CompositePlusSimple,
        }
    }
}

impl std::str::FromStr for FertilizationStrategy {
    type Err = super::errors::DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().replace('-', "_").as_str() {
            "composite_plus_simple" | "composite" => Ok(FertilizationStrategy::CompositePlusSimple),
            "simple_blend_only" | "simple" | "blend" => Ok(FertilizationStrategy::SimpleBlendOnly),
            other => Err(super::errors::DomainError::InvalidInput(format!("unknown fertilization strategy: {other}"))),
        }
    }
}

impl std::fmt::Display for FertilizationStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRole {
    Composite,
    Simple,
}

impl SourceRole {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceRole::Composite => "compound",
            SourceRole::Simple => "straight",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NutrientContribution {
    pub nutrient: GradeNutrient,
    pub kg_ha: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NutrientRemainder {
    pub nutrient: GradeNutrient,
    pub required_kg_ha: f64,
    pub supplied_kg_ha: f64,
    /// `max(0, required - supplied)`: an over-application is reported by
    /// `coverage_pct`, never as a negative remainder.
    pub remaining_kg_ha: f64,
}

impl NutrientRemainder {
    pub fn coverage_pct(&self) -> f64 {
        if self.required_kg_ha <= 0.0 {
            return 100.0;
        }
        self.supplied_kg_ha / self.required_kg_ha * 100.0
    }
}

/// Bags of product, for a grower who buys by the bag and not by the kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BagBreakdown {
    pub bag_weight_kg: f64,
    pub bags_per_ha: f64,
    pub bags_total: f64,
    /// What to actually order — bags are not sold in fractions.
    pub bags_total_rounded_up: u64,
}

impl BagBreakdown {
    pub fn new(kg_per_ha: f64, kg_total: f64, bag_weight_kg: f64) -> Option<Self> {
        (bag_weight_kg > 0.0).then(|| Self {
            bag_weight_kg,
            bags_per_ha: kg_per_ha / bag_weight_kg,
            bags_total: kg_total / bag_weight_kg,
            bags_total_rounded_up: (kg_total / bag_weight_kg).ceil().max(0.0) as u64,
        })
    }
}

/// One product line of the final recommendation.
#[derive(Debug, Clone)]
pub struct BlendLine {
    pub source_id: String,
    pub source_name: String,
    pub role: SourceRole,
    pub grade: CommercialGrade,
    pub form: FertilizerForm,
    pub kg_per_ha: f64,
    pub kg_total: f64,
    pub bags: Option<BagBreakdown>,
    pub contributions: Vec<NutrientContribution>,
    /// Why this product, in one line — the audit trail the heuristic owes
    /// the reader.
    pub rationale: String,
}

/// One compound product and the mass of it this program applies.
#[derive(Debug, Clone, PartialEq)]
pub struct CompoundPart {
    pub source_id: String,
    pub source_name: String,
    pub grade: CommercialGrade,
    /// The nutrient whose 100% coverage sized this part.
    pub reference_nutrient: GradeNutrient,
    /// This part's share of the compound slot: 1.0 for a single product,
    /// two complementary fractions for a pair.
    pub share: f64,
    pub kg_per_ha: f64,
    pub rationale: String,
}

impl CompoundPart {
    fn candidate<'a>(&self, catalog: &'a [CompositeCandidate]) -> &'a CompositeCandidate {
        catalog.iter().find(|c| c.source_id == self.source_id).expect("the part came from this catalog")
    }
}

/// PART E's answer, with the whole dosing table rather than only the dose
/// that was chosen.
#[derive(Debug, Clone)]
pub struct CompositeRecommendation {
    pub score: CompositeCandidateScore,
    /// One entry for a single compound, two when the search found a pair
    /// that covers more of the plan than either alone.
    pub parts: Vec<CompoundPart>,
    /// kg/ha of the leading product that would cover 100% of each
    /// requirement on its own. The spread between them is the reason a
    /// single compound cannot balance a plan by itself.
    pub dose_per_nutrient: Vec<(GradeNutrient, f64)>,
    pub reference_nutrient: GradeNutrient,
    /// Total across every part.
    pub kg_per_ha: f64,
    pub contributions: Vec<NutrientContribution>,
    /// Why a pair beat the best single compound, when it did.
    pub pairing: String,
}

/// How many ranked compounds enter the pairing search, and how coarsely the
/// slot is partitioned between two of them.
///
/// AGRONOMIC_NOTE: the compound slot is where splitting actually pays,
/// because a compound is dosed on the *minimum* over its nutrients — a
/// non-linear function, so two grades can cover more of a plan together
/// than either does alone. Splitting a *straight* between two products
/// cannot do the same: for one nutrient the mass is linear in the share, so
/// the optimum is always an endpoint, which the single-pick search already
/// evaluates. That asymmetry is why the pairing lives here.
///
/// Bounded at 3 candidates and quarter shares: 3 singles + 3 pairs x 3
/// shares = 12 full program evaluations, each of which runs the straight
/// search once.
const PAIRED_COMPOUNDS: usize = 3;
const COMPOUND_SHARES: [f64; 3] = [0.25, 0.5, 0.75];

/// Picks the compound slot: the best single compound, or a pair of them
/// split by share, whichever leaves the plan better covered. `None` when
/// no compound earns the slot.
///
/// Scored on what is left *after* the compound rather than on the compound
/// itself — a grade that looks worse against the target can still be the
/// better buy if the straights it leaves behind are lighter and waste less.
///
/// A compound only earns the slot if it carries **at least two** of the
/// required nutrients. One is what a straight does, and a compound doing it
/// brings the rest of its grade along uninvited: a lot asking for N and S
/// alone was sold 988 kg/ha of DAP — 452 kg/ha of P2O5 on a soil already
/// testing high in P — because DAP ranked first among products none of
/// which fitted, and the slot had to be filled by something. It doesn't:
/// urea and ammonium sulfate covered that plan exactly, and now do.
fn choose_compound(
    ranked: &[CompositeCandidateScore],
    catalog: &[CompositeCandidate],
    required: &[NutrientRequirement],
    blend_search: BlendSearchStrategy,
) -> Option<CompositeRecommendation> {
    let carries_several = |candidate: &CompositeCandidate| {
        required.iter().filter(|r| candidate.grade.get(r.nutrient) > 0.0).count() >= 2
    };
    let qualified: Vec<&CompositeCandidateScore> = ranked
        .iter()
        .filter(|score| {
            catalog.iter().find(|c| c.source_id == score.candidate_id).is_some_and(&carries_several)
        })
        .collect();
    let leaders: Vec<&CompositeCandidate> = qualified
        .iter()
        .take(PAIRED_COMPOUNDS)
        .filter_map(|score| catalog.iter().find(|c| c.source_id == score.candidate_id))
        .collect();
    let best_score = (*qualified.first()?).clone();

    // Every option: each leader alone, then each pair at each share. A pair
    // is only tried when the search is allowed to split.
    let mut options: Vec<Vec<(&CompositeCandidate, f64)>> =
        leaders.iter().map(|candidate| vec![(*candidate, 1.0)]).collect();
    if blend_search == BlendSearchStrategy::SplitPairs {
        for (index, first) in leaders.iter().enumerate() {
            for second in leaders.iter().skip(index + 1) {
                for share in COMPOUND_SHARES {
                    options.push(vec![(*first, share), (*second, 1.0 - share)]);
                }
            }
        }
    }

    let mut best: Option<(Vec<CompoundPart>, BlendScore)> = None;
    for option in &options {
        let Some(parts) = dose_compound_parts(option, required) else {
            continue;
        };
        let mut left = required.to_vec();
        for part in &parts {
            for contribution in contributions_of(&part.grade, part.kg_per_ha) {
                if let Some(entry) = left.iter_mut().find(|r| r.nutrient == contribution.nutrient) {
                    entry.kg_ha = (entry.kg_ha - contribution.kg_ha).max(0.0);
                }
            }
        }
        // The compound is judged by the whole program it implies, straights
        // included — which is the only way a "worse" grade can win by
        // leaving an easier remainder.
        let (straights, _) = cover_with_straights(&left, catalog, 1.0, 50.0, blend_search);
        let score = program_score(&parts, &straights, required);
        let better = match &best {
            None => true,
            Some((_, best_score)) => match score.compare(best_score) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Equal => {
                    let ids = |parts: &[CompoundPart]| {
                        let mut ids: Vec<String> = parts.iter().map(|p| p.source_id.clone()).collect();
                        ids.sort();
                        ids
                    };
                    ids(&parts) < ids(&best.as_ref().expect("checked").0)
                }
                std::cmp::Ordering::Greater => false,
            },
        };
        if better {
            best = Some((parts, score));
        }
    }

    let (parts, _) = best?;
    let leader = &parts[0];
    let dose_per_nutrient = compound_dose_kg_ha(&leader.grade, required)?.per_nutrient;
    let kg_per_ha = parts.iter().map(|p| p.kg_per_ha).sum();
    let contributions = merged_contributions(&parts);
    let pairing = if parts.len() > 1 {
        format!(
            "The compound slot was split {:.0}/{:.0} between {} and {}: together they cover more of this plan              than either grade does alone, so the straights that follow are lighter.",
            parts[0].share * 100.0,
            parts[1].share * 100.0,
            parts[0].source_name,
            parts[1].source_name
        )
    } else {
        String::new()
    };

    Some(CompositeRecommendation {
        score: best_score,
        reference_nutrient: leader.reference_nutrient,
        parts,
        dose_per_nutrient,
        kg_per_ha,
        contributions,
        pairing,
    })
}

/// Sizes each product of a compound option. A part takes `share` of every
/// requirement, and is then dosed on the nutrient it satisfies first — the
/// same rule a single compound follows.
fn dose_compound_parts(
    option: &[(&CompositeCandidate, f64)],
    required: &[NutrientRequirement],
) -> Option<Vec<CompoundPart>> {
    let mut parts = Vec::new();
    for (candidate, share) in option {
        let scaled: Vec<NutrientRequirement> = required
            .iter()
            .map(|r| NutrientRequirement { nutrient: r.nutrient, kg_ha: r.kg_ha * share })
            .collect();
        let dose = compound_dose_kg_ha(&candidate.grade, &scaled)?;
        parts.push(CompoundPart {
            source_id: candidate.source_id.clone(),
            source_name: candidate.name.clone(),
            grade: candidate.grade,
            reference_nutrient: dose.reference_nutrient,
            share: *share,
            kg_per_ha: dose.kg_per_ha,
            rationale: match option.len() {
                1 => format!(
                    "closest compound to the target grade; dosed on {}, the first requirement it satisfies, so                      nothing is over-applied",
                    dose.reference_nutrient
                ),
                _ => format!(
                    "{:.0}% of the compound slot, dosed on {} within that share",
                    share * 100.0,
                    dose.reference_nutrient
                ),
            },
        });
    }
    (!parts.is_empty()).then_some(parts)
}

fn merged_contributions(parts: &[CompoundPart]) -> Vec<NutrientContribution> {
    GradeNutrient::ALL
        .into_iter()
        .filter_map(|nutrient| {
            let kg_ha: f64 =
                parts.iter().map(|part| part.kg_per_ha * part.grade.get(nutrient) / 100.0).sum();
            (kg_ha > 0.0).then_some(NutrientContribution { nutrient, kg_ha })
        })
        .collect()
}

/// The same [`BlendScore`] the straight search uses, over a whole program.
fn program_score(parts: &[CompoundPart], straights: &[BlendLine], required: &[NutrientRequirement]) -> BlendScore {
    let real = |kg: f64| if kg > NEGLIGIBLE_KG_HA { kg } else { 0.0 };
    let mut score = BlendScore {
        uncovered_kg: 0.0,
        over_supplied_kg: 0.0,
        sourced_mass_kg: 0.0,
        product_count: parts.len() + straights.len(),
    };
    for nutrient in GradeNutrient::ALL {
        let want = required.iter().find(|r| r.nutrient == nutrient).map_or(0.0, |r| r.kg_ha);
        let got: f64 = parts.iter().map(|p| p.kg_per_ha * p.grade.get(nutrient) / 100.0).sum::<f64>()
            + straights.iter().map(|l| l.kg_per_ha * l.grade.get(nutrient) / 100.0).sum::<f64>();
        score.uncovered_kg += real(want - got);
        score.over_supplied_kg += real(got - want);
    }
    score.sourced_mass_kg = parts.iter().map(|p| p.kg_per_ha).sum::<f64>()
        + straights.iter().map(|l| l.kg_per_ha).sum::<f64>();
    score
}

/// PART E's dose.
/// PART E's dose.
///
/// AMBIGUITY, resolved and pinned by test. The workflow states the dose as
/// the **largest** mass any covered nutrient demands, but its own worked
/// example does not use that figure, and PART F (remainders, complemented
/// with straights) only has work to do if the compound leaves something
/// uncovered — which the largest-mass rule guarantees it never does.
///
/// Resolved toward the smallest: the compound is dosed on the **first
/// nutrient it satisfies**, so it over-applies nothing, and the straights
/// close every remaining gap. On the workflow's own numbers the largest-mass
/// rule would apply 647 kg/ha of 13-26-6 to reach the nitrogen target,
/// delivering 168 kg/ha of P2O5 against a requirement of 96 — a 75% excess
/// of the least mobile, most easily fixed nutrient in the plan, paid for by
/// the grower. `dose_per_nutrient` reports every figure, including the
/// largest, so the decision is visible and reversible.
///
/// ponytail: one rule, no knob. If a grower deliberately wants to build
/// soil P, flipping `min` to `max` here is the whole change.
pub fn compound_dose_kg_ha(grade: &CommercialGrade, requirements: &[NutrientRequirement]) -> Option<CompoundDose> {
    let per_nutrient: Vec<(GradeNutrient, f64)> = requirements
        .iter()
        .filter(|r| r.kg_ha > 0.0 && grade.get(r.nutrient) > 0.0)
        .map(|r| (r.nutrient, r.kg_ha / (grade.get(r.nutrient) / 100.0)))
        .filter(|(_, dose)| dose.is_finite() && *dose > 0.0)
        .collect();

    let (reference_nutrient, kg_per_ha) = per_nutrient
        .iter()
        .copied()
        // Ties break on the nutrient's own order, so the same grade and the
        // same requirements always name the same reference nutrient.
        .min_by(|(an, a), (bn, b)| a.total_cmp(b).then_with(|| an.cmp(bn)))?;
    Some(CompoundDose { reference_nutrient, kg_per_ha, per_nutrient })
}

/// The dose, and the whole table it was chosen from.
#[derive(Debug, Clone)]
pub struct CompoundDose {
    pub reference_nutrient: GradeNutrient,
    pub kg_per_ha: f64,
    /// What each requirement alone would demand of this product.
    pub per_nutrient: Vec<(GradeNutrient, f64)>,
}

/// What `kg_per_ha` of a product delivers, on the visible basis.
pub fn contributions_of(grade: &CommercialGrade, kg_per_ha: f64) -> Vec<NutrientContribution> {
    grade
        .carried()
        .into_iter()
        .map(|nutrient| NutrientContribution { nutrient, kg_ha: kg_per_ha * grade.get(nutrient) / 100.0 })
        .collect()
}

/// PART F's straight-source heuristic, as a penalty (lower wins).
///
/// 1. **concentration** of the wanted nutrient — the dominant term, since a
///    dilute source means more product, more freight and more bags.
/// 2. **off-target load** — nutrients the product carries that nothing in
///    this plan still needs. A source that also supplies something still
///    outstanding is *not* penalized for it; that is the cross-contribution
///    the blend is supposed to exploit.
/// 3. **sourcing** — the same commercialization metadata the compound
///    scoring uses.
/// 4. ties break on `source_id`.
fn straight_shortlist_penalty(candidate: &CompositeCandidate, wanted: GradeNutrient) -> Option<f64> {
    let concentration = candidate.grade.get(wanted);
    (concentration > 0.0).then(|| sourcing_surcharge(candidate) / concentration)
}

/// Sourcing, priced as a multiplier on product mass: how much extra
/// product it is worth carrying to buy something the grower can actually
/// get. A "comercialización común" product (0.1) has to be 10% lighter to
/// win against a freely stocked one, a "uso especializado" one 30%, a
/// regulated one 50%.
///
/// AGRONOMIC_NOTE: the number that made this necessary was phosphoric acid
/// — 52% P2O5, a fertigation liquid — beating triple superphosphate at 46%
/// for a broadcast plan, because it is 12% lighter per unit of P2O5.
/// Concentration and mass alone keep picking specialty liquids for growers
/// who spread solids.
///
/// One preference, used twice: the shortlist ranks products on surcharged
/// mass per unit of nutrient, and [`CoverCost`] compares whole blends on
/// surcharged mass. The two cannot drift apart.
fn sourcing_surcharge(candidate: &CompositeCandidate) -> f64 {
    1.0 + candidate.commercialization_penalty
}

/// How many products per nutrient enter the search.
///
/// The shortlist is only a *filter* — which products are worth trying — and
/// the objective below is what actually chooses among them. Three is enough
/// for the shipped catalogs to reach past the obvious straight to the one
/// that also covers something else; the search grows as
/// `nutrients! x SHORTLIST^nutrients`, so 4 nutrients cost 24 x 81 = 1944
/// evaluations of four arithmetic steps each. Raise it if a catalog turns
/// out to hide a better third choice.
const SHORTLIST_PER_NUTRIENT: usize = 3;

/// The products worth trying for one nutrient, best concentration and
/// easiest sourcing first, ties broken on the id.
fn straight_shortlist(catalog: &[CompositeCandidate], wanted: GradeNutrient) -> Vec<&CompositeCandidate> {
    let mut ranked: Vec<(&CompositeCandidate, f64)> = catalog
        .iter()
        .filter(|candidate| !candidate.is_compound())
        .filter_map(|candidate| straight_shortlist_penalty(candidate, wanted).map(|penalty| (candidate, penalty)))
        .collect();
    ranked.sort_by(|(a, pa), (b, pb)| pa.total_cmp(pb).then_with(|| a.source_id.cmp(&b.source_id)));
    ranked.truncate(SHORTLIST_PER_NUTRIENT);
    ranked.into_iter().map(|(candidate, _)| candidate).collect()
}

/// What one candidate blend costs, compared lexicographically.
///
/// Three facts in priority order, and no weights to justify between them:
///
/// 1. **uncovered** — a requirement left short is a failure of the plan, not
///    an expense, so nothing else can buy it back.
/// 2. **wasted** — nutrient delivered past what the plan asked for, counted
///    over all four grade nutrients including ones it never asked for. This
///    is the term that used to be a hand-set `0.5 x off_target` applied at
///    pick time; it is now measured on the finished blend, where it is
///    actually true.
/// 3. **sourced mass** — freight and bags, with each product's mass
///    surcharged by how hard it is to buy (see [`sourcing_surcharge`]).
/// 4. **product count** — a blend with fewer bags is one fewer purchase and
///    one fewer pass over the field.
///
/// Count comes *last* on purpose. Put it any earlier and a split could
/// never win, since a split always adds a product: it would decide the
/// answer before waste and mass were consulted. Last, it means a split has
/// to pay for itself in waste or mass first, and only breaks ties in its
/// own disfavour.
#[derive(Debug, Clone, PartialEq)]
pub struct BlendScore {
    /// Requirement left short, summed over nutrients.
    pub uncovered_kg: f64,
    /// Nutrient delivered past what the plan asked for, over all six grade
    /// nutrients including ones it never asked for.
    pub over_supplied_kg: f64,
    /// Not the mass anyone weighs — that is on the blend lines. This is the
    /// comparison quantity: real mass surcharged by sourcing difficulty.
    pub sourced_mass_kg: f64,
    pub product_count: usize,
}

impl BlendScore {
    fn compare(&self, other: &Self) -> std::cmp::Ordering {
        self.uncovered_kg
            .total_cmp(&other.uncovered_kg)
            .then_with(|| self.over_supplied_kg.total_cmp(&other.over_supplied_kg))
            .then_with(|| self.sourced_mass_kg.total_cmp(&other.sourced_mass_kg))
            .then_with(|| self.product_count.cmp(&other.product_count))
    }
}

/// One product and the mass of it this blend uses.
type Dose<'a> = (&'a CompositeCandidate, f64);

/// Runs one candidate blend: cover each nutrient in `order` with the
/// product `picks` assigns to it, sizing every pick against what is still
/// outstanding at that point.
///
/// A product that comes up twice accumulates into one dose rather than a
/// second line — the same bag bought twice is one purchase, and printing it
/// as two products was a real defect of the old pass.
fn simulate_cover<'a>(
    order: &[GradeNutrient],
    assignment: &Assignment<'a>,
    start: &[NutrientRequirement],
) -> Vec<Dose<'a>> {
    let mut remaining: Vec<NutrientRequirement> = start.to_vec();
    let mut doses: Vec<Dose<'a>> = Vec::new();

    for nutrient in order {
        let Some((_, parts)) = assignment.iter().find(|(n, _)| n == nutrient) else {
            continue;
        };
        // Read once, before any part of the split is applied: both halves
        // size against the same outstanding figure, which is what makes
        // "70/30" mean 70% and 30% of one requirement.
        let short = remaining.iter().find(|r| r.nutrient == *nutrient).map_or(0.0, |r| r.kg_ha);
        if short <= NEGLIGIBLE_KG_HA {
            continue;
        }

        let mut applied: Vec<NutrientContribution> = Vec::new();
        for (candidate, share) in parts {
            let concentration = candidate.grade.get(*nutrient);
            if concentration <= 0.0 || *share <= 0.0 {
                continue;
            }
            let kg_per_ha = short * share / (concentration / 100.0);
            applied.extend(contributions_of(&candidate.grade, kg_per_ha));
            match doses.iter_mut().find(|(existing, _)| existing.source_id == candidate.source_id) {
                Some((_, kg)) => *kg += kg_per_ha,
                None => doses.push((candidate, kg_per_ha)),
            }
        }
        for contribution in applied {
            if let Some(entry) = remaining.iter_mut().find(|r| r.nutrient == contribution.nutrient) {
                entry.kg_ha = (entry.kg_ha - contribution.kg_ha).max(0.0);
            }
        }
    }
    doses
}

fn cover_cost(doses: &[Dose<'_>], start: &[NutrientRequirement]) -> BlendScore {
    // Everything here is quantized to the noise floor before it is compared.
    // The comparison is lexicographic, so without this a blend that misses a
    // requirement by 1.4e-14 kg — the residue of subtracting a dose from the
    // requirement that produced it — outranks one that wastes 21 real kg.
    let real = |kg: f64| if kg > NEGLIGIBLE_KG_HA { kg } else { 0.0 };

    let mut cost =
        BlendScore { uncovered_kg: 0.0, over_supplied_kg: 0.0, sourced_mass_kg: 0.0, product_count: doses.len() };
    for nutrient in GradeNutrient::ALL {
        let required = start.iter().find(|r| r.nutrient == nutrient).map_or(0.0, |r| r.kg_ha);
        let supplied: f64 =
            doses.iter().map(|(candidate, kg)| kg * candidate.grade.get(nutrient) / 100.0).sum();
        cost.uncovered_kg += real(required - supplied);
        cost.over_supplied_kg += real(supplied - required);
    }
    // Rounded to grams, so the mass tiebreak only fires on a difference a
    // scale could see and the id tiebreak below can still be reached.
    let sourced: f64 = doses.iter().map(|(candidate, kg)| kg * sourcing_surcharge(candidate)).sum();
    cost.sourced_mass_kg = (sourced * 1000.0).round() / 1000.0;
    cost
}

/// Every ordering of up to four nutrients. Recursive because four items is
/// 24 orderings and a hand-rolled Heap's algorithm would be more code than
/// the thing it replaces.
fn orderings(items: &[GradeNutrient]) -> Vec<Vec<GradeNutrient>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(index);
        for mut tail in orderings(&rest) {
            tail.insert(0, *item);
            out.push(tail);
        }
    }
    out
}

/// One product per nutrient, every way of choosing from the shortlists.
fn pick_combinations<'a>(
    shortlists: &[(GradeNutrient, Vec<&'a CompositeCandidate>)],
) -> Vec<Vec<(GradeNutrient, &'a CompositeCandidate)>> {
    let mut combinations = vec![Vec::new()];
    for (nutrient, options) in shortlists {
        combinations = combinations
            .iter()
            .flat_map(|prefix| {
                options.iter().map(move |candidate| {
                    let mut next = prefix.clone();
                    next.push((*nutrient, *candidate));
                    next
                })
            })
            .collect();
    }
    combinations
}

/// Which search covers the requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendSearchStrategy {
    /// One product per nutrient, covering it to exactly 100%. Exhaustive
    /// over orderings and shortlists. Kept as the baseline every split is
    /// measured against, and as the answer when no split earns its place.
    SinglePick,
    /// The baseline, then one pass that tries splitting each nutrient's
    /// requirement across two products.
    #[default]
    SplitPairs,
}

impl BlendSearchStrategy {
    pub const ALL: [BlendSearchStrategy; 2] = [BlendSearchStrategy::SplitPairs, BlendSearchStrategy::SinglePick];

    pub fn as_str(self) -> &'static str {
        match self {
            BlendSearchStrategy::SinglePick => "single_pick",
            BlendSearchStrategy::SplitPairs => "split_pairs",
        }
    }
}

impl std::str::FromStr for BlendSearchStrategy {
    type Err = super::errors::DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().replace('-', "_").as_str() {
            "single_pick" | "single" => Ok(BlendSearchStrategy::SinglePick),
            "split_pairs" | "split" => Ok(BlendSearchStrategy::SplitPairs),
            other => Err(super::errors::DomainError::InvalidInput(format!("unknown blend search: {other}"))),
        }
    }
}

impl std::fmt::Display for BlendSearchStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One product's share of one nutrient's requirement.
#[derive(Debug, Clone, PartialEq)]
pub struct BlendPartition {
    pub nutrient: GradeNutrient,
    pub source_id: String,
    pub source_name: String,
    /// 1.0 for a single pick; the two halves of a split sum to 1.0.
    pub share: f64,
}

/// A blend the search evaluated, and what it scored.
#[derive(Debug, Clone, PartialEq)]
pub struct BlendCandidate {
    pub strategy: BlendSearchStrategy,
    pub partitions: Vec<BlendPartition>,
    pub score: BlendScore,
    /// Why this beat the baseline, in one sentence. Empty when it *is* the
    /// baseline.
    pub improvement: String,
}

impl BlendCandidate {
    /// The splits only, for a report that wants to explain them.
    pub fn splits(&self) -> Vec<(GradeNutrient, Vec<&BlendPartition>)> {
        GradeNutrient::ALL
            .into_iter()
            .filter_map(|nutrient| {
                let parts: Vec<&BlendPartition> =
                    self.partitions.iter().filter(|p| p.nutrient == nutrient).collect();
                (parts.len() > 1).then_some((nutrient, parts))
            })
            .collect()
    }
}

/// Fractions of a requirement the first product of a pair may take.
///
/// 10% steps: finer than a grower can weigh out at field scale, and coarse
/// enough that the whole split pass stays inside a few hundred simulations.
/// The endpoints are excluded because 0% and 100% *are* the single pick,
/// which the baseline already evaluated exhaustively.
const PARTITION_STEPS: [f64; 9] = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];

/// What one nutrient's requirement is covered by: one product at 100%, or
/// two at complementary shares.
type Assignment<'a> = Vec<(GradeNutrient, Vec<(&'a CompositeCandidate, f64)>)>;

/// PARTS F and G: cover `remaining` with straights.
///
/// Two searches, in sequence, and the second only ever improves on the
/// first:
///
/// 1. **Baseline, exhaustive.** One product per nutrient covering it to
///    100%, over every ordering (<= 6! ) crossed with every choice from each
///    nutrient's three-product shortlist (<= 3^6). Optimal over that space.
/// 2. **Split pass.** For each nutrient in turn, try giving its requirement
///    to *two* shortlist products at each 10% partition, and keep the split
///    only if the whole blend scores strictly better. One pass, in a fixed
///    nutrient order.
///
/// # Where the computational frontier is
///
/// The split is a **local improvement over the exhaustive baseline**, not a
/// second exhaustive search. Folding partitions into the cartesian product
/// would take each nutrient's 3 options to 3 + 3 pairs x 9 steps = 30, and
/// 30^6 is 729 million simulations. The pass instead costs
/// nutrients x pairs x steps = 6 x 3 x 9 = 162. What it gives up is
/// *interaction between two splits at once*: a blend that only pays off if
/// N and K are both split is outside it. That is the stated ceiling.
///
/// ponytail: still not a linear program, and price is still not an input —
/// the catalog carries none.
fn search_blend<'a>(
    remaining: &[NutrientRequirement],
    catalog: &'a [CompositeCandidate],
    strategy: BlendSearchStrategy,
) -> Option<(Assignment<'a>, Vec<GradeNutrient>, Vec<GradeNutrient>, BlendCandidate)> {
    let shortlists: Vec<(GradeNutrient, Vec<&CompositeCandidate>)> = remaining
        .iter()
        .filter(|requirement| requirement.kg_ha > NEGLIGIBLE_KG_HA)
        .map(|requirement| (requirement.nutrient, straight_shortlist(catalog, requirement.nutrient)))
        // A nutrient nothing in the catalog carries drops out of the search
        // entirely rather than emptying it. The balance is recomputed from
        // the original requirements, so the shortfall still reaches the
        // report instead of disappearing.
        .filter(|(_, options)| !options.is_empty())
        .collect();
    if shortlists.is_empty() {
        return None;
    }

    let nutrients: Vec<GradeNutrient> = shortlists.iter().map(|(nutrient, _)| *nutrient).collect();
    // 4! = 24 orderings; 6! = 720, and crossed with 3^6 shortlist choices
    // that is 525k simulations for a plan that also needs Ca and Mg. Past
    // four the order is fixed to largest-requirement-first — the rule the
    // exhaustive search replaced, kept as the bound rather than as the
    // default.
    let orders = if nutrients.len() <= 4 {
        orderings(&nutrients)
    } else {
        let mut fixed = nutrients.clone();
        fixed.sort_by(|a, b| {
            let kg = |n: &GradeNutrient| remaining.iter().find(|r| r.nutrient == *n).map_or(0.0, |r| r.kg_ha);
            kg(b).total_cmp(&kg(a)).then_with(|| a.cmp(b))
        });
        vec![fixed]
    };

    // ---- 1. the exhaustive single-pick baseline ----
    let mut best: Option<(Vec<GradeNutrient>, Assignment, BlendScore)> = None;
    for order in &orders {
        for picks in &pick_combinations(&shortlists) {
            let assignment: Assignment =
                picks.iter().map(|(nutrient, candidate)| (*nutrient, vec![(*candidate, 1.0)])).collect();
            let score = score_assignment(order, &assignment, remaining);
            if is_better(&score, &assignment, best.as_ref().map(|(_, a, s)| (a, s))) {
                best = Some((order.clone(), assignment, score));
            }
        }
    }
    let (order, mut assignment, mut score) = best?;
    let baseline = score.clone();

    // ---- 2. the split pass ----
    let mut improvements: Vec<String> = Vec::new();
    if strategy == BlendSearchStrategy::SplitPairs {
        for (nutrient, options) in &shortlists {
            let Some(before) = try_splits(*nutrient, options, &order, &mut assignment, &mut score, remaining) else {
                continue;
            };
            improvements.push(before);
        }
    }

    let partitions: Vec<BlendPartition> = assignment
        .iter()
        .flat_map(|(nutrient, parts)| {
            parts.iter().map(move |(candidate, share)| BlendPartition {
                nutrient: *nutrient,
                source_id: candidate.source_id.clone(),
                source_name: candidate.name.clone(),
                share: *share,
            })
        })
        .collect();

    let improvement = if improvements.is_empty() {
        String::new()
    } else {
        format!(
            "{} Against the single-pick baseline: {:+.1} kg/ha of unwanted nutrient and {:+.1} kg/ha of product.",
            improvements.join(" "),
            score.over_supplied_kg - baseline.over_supplied_kg,
            score.sourced_mass_kg - baseline.sourced_mass_kg
        )
    };

    Some((
        assignment,
        order,
        nutrients,
        BlendCandidate {
            strategy: if improvements.is_empty() { BlendSearchStrategy::SinglePick } else { strategy },
            partitions,
            score,
            improvement,
        },
    ))
}

/// Tries every pair and every partition for one nutrient, keeping the best
/// strict improvement. Returns the sentence explaining it, or `None` when
/// the single pick stands.
fn try_splits<'a>(
    nutrient: GradeNutrient,
    options: &[&'a CompositeCandidate],
    order: &[GradeNutrient],
    assignment: &mut Assignment<'a>,
    score: &mut BlendScore,
    remaining: &[NutrientRequirement],
) -> Option<String> {
    let index = assignment.iter().position(|(n, _)| *n == nutrient)?;
    let original = assignment[index].1.clone();
    let mut winner: Option<(Vec<(&CompositeCandidate, f64)>, BlendScore)> = None;

    for (first_index, first) in options.iter().enumerate() {
        for second in options.iter().skip(first_index + 1) {
            for share in PARTITION_STEPS {
                let trial = vec![(*first, share), (*second, 1.0 - share)];
                assignment[index].1.clone_from(&trial);
                let candidate_score = score_assignment(order, assignment, remaining);
                // Strictly better, never merely equal: a split adds a
                // product to buy and a second pass to spread, so it has to
                // earn its place rather than tie its way in.
                let beats = match &winner {
                    Some((_, best)) => candidate_score.compare(best) == std::cmp::Ordering::Less,
                    None => candidate_score.compare(score) == std::cmp::Ordering::Less,
                };
                if beats {
                    winner = Some((trial, candidate_score));
                }
            }
        }
    }

    match winner {
        Some((trial, improved)) => {
            let explanation = format!(
                "{} was split {:.0}/{:.0} between {} and {}.",
                nutrient.as_str(),
                trial[0].1 * 100.0,
                trial[1].1 * 100.0,
                trial[0].0.name,
                trial[1].0.name,
            );
            assignment[index].1 = trial;
            *score = improved;
            Some(explanation)
        }
        None => {
            assignment[index].1 = original;
            None
        }
    }
}

fn is_better(score: &BlendScore, assignment: &Assignment, best: Option<(&Assignment, &BlendScore)>) -> bool {
    match best {
        None => true,
        Some((best_assignment, best_score)) => match score.compare(best_score) {
            std::cmp::Ordering::Less => true,
            // Ties break on the product ids, so a catalog that offers two
            // equally good blends always yields the same one whatever order
            // it loaded in.
            std::cmp::Ordering::Equal => assignment_ids(assignment) < assignment_ids(best_assignment),
            std::cmp::Ordering::Greater => false,
        },
    }
}

fn assignment_ids<'a>(assignment: &Assignment<'a>) -> Vec<&'a str> {
    let mut ids: Vec<&str> =
        assignment.iter().flat_map(|(_, parts)| parts.iter().map(|(c, _)| c.source_id.as_str())).collect();
    ids.sort_unstable();
    ids
}

fn score_assignment(order: &[GradeNutrient], assignment: &Assignment, remaining: &[NutrientRequirement]) -> BlendScore {
    let doses = simulate_cover(order, assignment, remaining);
    cover_cost(&doses, remaining)
}

fn cover_with_straights(
    remaining: &[NutrientRequirement],
    catalog: &[CompositeCandidate],
    total_area_ha: f64,
    bag_weight_kg: f64,
    strategy: BlendSearchStrategy,
) -> (Vec<BlendLine>, Option<BlendCandidate>) {
    let Some((assignment, order, nutrients, candidate)) = search_blend(remaining, catalog, strategy) else {
        return (Vec::new(), None);
    };
    // The winning *order* is half the answer: the same products in a
    // different sequence size against different outstanding figures and
    // make a different blend. Rebuilding the lines from the assignment's own
    // key order instead of this one silently shipped the losing simulation.
    let doses = simulate_cover(&order, &assignment, remaining);

    let lines = doses
        .into_iter()
        .map(|(product, kg_per_ha)| {
            let kg_total = kg_per_ha * total_area_ha;
            let covers: Vec<&str> = product
                .grade
                .carried()
                .into_iter()
                .filter(|nutrient| nutrients.contains(nutrient))
                .map(|nutrient| nutrient.as_str())
                .collect();
            let share = candidate
                .partitions
                .iter()
                .find(|p| p.source_id == product.source_id && p.share < 1.0)
                .map(|p| format!("{:.0}% of the {} requirement, ", p.share * 100.0, p.nutrient.as_str()))
                .unwrap_or_default();
            BlendLine {
                source_id: product.source_id.clone(),
                source_name: product.name.clone(),
                role: SourceRole::Simple,
                grade: product.grade,
                form: product.form,
                kg_per_ha,
                kg_total,
                bags: BagBreakdown::new(kg_per_ha, kg_total, bag_weight_kg),
                contributions: contributions_of(&product.grade, kg_per_ha),
                rationale: format!(
                    "{share}carries {} at the lowest waste and mass of any blend this catalog allows",
                    covers.join(" and ")
                ),
            }
        })
        .collect();
    (lines, Some(candidate))
}

/// One strategy's complete answer.
#[derive(Debug, Clone)]
pub struct FertilizerProgram {
    pub strategy: FertilizationStrategy,
    /// How the straights were searched, and what the search chose. `None`
    /// when no straight was needed at all.
    pub blend: Option<BlendCandidate>,
    /// `None` for [`FertilizationStrategy::SimpleBlendOnly`], and for a
    /// catalog with no usable compound.
    pub composite: Option<CompositeRecommendation>,
    pub lines: Vec<BlendLine>,
    pub balance: Vec<NutrientRemainder>,
    pub total_kg_per_ha: f64,
    pub total_kg: f64,
    pub total_bags_rounded_up: u64,
}

impl FertilizerProgram {
    /// Nutrients still short after every line, if any.
    pub fn uncovered(&self) -> Vec<&NutrientRemainder> {
        self.balance.iter().filter(|r| r.remaining_kg_ha > NEGLIGIBLE_KG_HA).collect()
    }
}

/// Below this a remainder is arithmetic residue, not agronomy: 10 g/ha is
/// four orders of magnitude under any dose anybody spreads. Without a floor
/// the greedy pass chases the rounding error of its own previous pick and
/// emits a second product line of 0.0 kg/ha — and one bag, since bags round
/// up.
const NEGLIGIBLE_KG_HA: f64 = 0.01;

/// Builds one program end to end. `requirements` are on the visible basis
/// and already net of soil supply and use efficiency — this module never
/// re-derives agronomy, it only chooses products.
#[allow(clippy::too_many_arguments)]
pub fn build_program(
    strategy: FertilizationStrategy,
    requirements: &[NutrientRequirement],
    target: Option<&CommercialGrade>,
    catalog: &[CompositeCandidate],
    total_area_ha: f64,
    bag_weight_kg: f64,
    blend_search: BlendSearchStrategy,
) -> FertilizerProgram {
    let required: Vec<NutrientRequirement> = requirements.iter().copied().filter(|r| r.kg_ha > 0.0).collect();
    let mut remaining = required.clone();
    let mut lines: Vec<BlendLine> = Vec::new();
    let mut composite = None;

    if strategy == FertilizationStrategy::CompositePlusSimple {
        let required_nutrients: Vec<GradeNutrient> = required.iter().map(|r| r.nutrient).collect();
        let ranked = match target {
            Some(target) => rank_candidates(target, &required_nutrients, catalog),
            None => Vec::new(),
        };
        composite = choose_compound(&ranked, catalog, &required, blend_search);
        if let Some(chosen) = &composite {
            for part in &chosen.parts {
                let candidate = part.candidate(catalog);
                let contributions = contributions_of(&candidate.grade, part.kg_per_ha);
                for contribution in &contributions {
                    if let Some(entry) = remaining.iter_mut().find(|r| r.nutrient == contribution.nutrient) {
                        entry.kg_ha = (entry.kg_ha - contribution.kg_ha).max(0.0);
                    }
                }
                let kg_total = part.kg_per_ha * total_area_ha;
                lines.push(BlendLine {
                    source_id: candidate.source_id.clone(),
                    source_name: candidate.name.clone(),
                    role: SourceRole::Composite,
                    grade: candidate.grade,
                    form: candidate.form,
                    kg_per_ha: part.kg_per_ha,
                    kg_total,
                    bags: BagBreakdown::new(part.kg_per_ha, kg_total, bag_weight_kg),
                    contributions: contributions.clone(),
                    rationale: part.rationale.clone(),
                });
            }
        }
    }

    let (straights, blend) = cover_with_straights(&remaining, catalog, total_area_ha, bag_weight_kg, blend_search);
    lines.extend(straights);

    let balance: Vec<NutrientRemainder> = required
        .iter()
        .map(|requirement| {
            let supplied: f64 = lines
                .iter()
                .flat_map(|line| &line.contributions)
                .filter(|c| c.nutrient == requirement.nutrient)
                .map(|c| c.kg_ha)
                .sum();
            NutrientRemainder {
                nutrient: requirement.nutrient,
                required_kg_ha: requirement.kg_ha,
                supplied_kg_ha: supplied,
                remaining_kg_ha: (requirement.kg_ha - supplied).max(0.0),
            }
        })
        .collect();

    let total_kg_per_ha = lines.iter().map(|line| line.kg_per_ha).sum();
    let total_kg = lines.iter().map(|line| line.kg_total).sum();
    // Summed per line, not derived from the total: bags are bought per
    // product, so each line rounds up on its own.
    let total_bags_rounded_up =
        lines.iter().filter_map(|line| line.bags).map(|bags| bags.bags_total_rounded_up).sum();

    FertilizerProgram { strategy, composite, lines, balance, total_kg_per_ha, total_kg, total_bags_rounded_up, blend }
}

/// Reads a catalog row onto the visible basis.
///
/// `oxide_factor` supplies the elemental -> oxide conversion the domain is
/// not allowed to hardcode; the use case reads it from
/// `conversion_factors.toml`. A factor the catalog cannot supply drops that
/// nutrient rather than passing an elemental figure off as a grade.
pub fn candidate_from_source(source: &FertilizerSource, oxide_factor: impl Fn(GradeNutrient) -> Option<f64>) -> CompositeCandidate {
    let grade = CommercialGrade::from_pairs(GradeNutrient::ALL.into_iter().filter_map(|nutrient| {
        let pct = source.pct_of(nutrient.elemental())?;
        let factor = match nutrient.oxide_conversion() {
            Some(_) => oxide_factor(nutrient)?,
            None => 1.0,
        };
        Some((nutrient, pct * factor))
    }));
    CompositeCandidate {
        source_id: source.source_id.clone(),
        name: source.name.clone(),
        grade,
        form: source.form,
        commercialization_penalty: commercialization_penalty(&source.restrictions),
    }
}

// ---------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------

/// What the plan was asked for, restated so a printed report stands on its
/// own away from the terminal that produced it.
#[derive(Debug, Clone)]
pub struct ScenarioSummary {
    pub field_id: String,
    pub sample_id: String,
    pub crop_id: String,
    pub yield_value: f64,
    pub yield_unit: String,
    pub total_area_ha: f64,
    pub bag_weight_kg: f64,
    pub strategy: FertilizationStrategy,
    pub profile: String,
}

/// One nutrient's whole balance, from what the crop asks for to what is
/// left after the products.
///
/// The half of the plan the formulation report used to drop. Without it an
/// exported file said what to buy but not why: a reader could not tell a
/// large dose on a poor soil from a large dose for an inflated yield goal.
#[derive(Debug, Clone, PartialEq)]
pub struct BalanceRow {
    pub nutrient: Nutrient,
    pub availability_kg_ha: f64,
    pub demand_kg_ha: f64,
    /// `None` when the reference table has no coefficient on either basis —
    /// the demand is unknown, not zero.
    pub demand_basis: Option<String>,
    pub efficiency_used: f64,
    pub net_requirement_kg_ha: f64,
    pub soil_status: Option<String>,
}

/// The agronomic half of the report: the balance, the lime, the
/// micronutrients and the warnings, all read off the plan.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FertilizationBalance {
    pub rows: Vec<BalanceRow>,
    /// Recommended CaCO3-equivalent, and the material if one was picked.
    pub liming_t_ha: Option<f64>,
    pub liming_material: Option<String>,
    /// Nutrient, reading with its unit, and the corrective dose if needed.
    pub micronutrients: Vec<(Nutrient, String, Option<String>)>,
    /// Anything the reader has to know to interpret the numbers, in the
    /// plan's own words.
    pub warnings: Vec<String>,
    pub mineralization_factor: f64,
    /// `false` means the plan ran on baseline constants.
    pub climate_enriched: bool,
}

/// Everything a front-end or an exporter needs, and nothing about how any
/// of it is rendered.
#[derive(Debug, Clone)]
pub struct FertilizerRecommendationReport {
    pub scenario: ScenarioSummary,
    /// Net requirements on the visible basis, as the whole heuristic saw
    /// them.
    pub requirements: Vec<NutrientRequirement>,
    /// The elemental figures they came from, so a reader can check the
    /// conversion rather than take it on faith.
    pub elemental_requirements: Vec<(Nutrient, f64)>,
    /// How the site's own conditions moved each nutrient's use efficiency,
    /// which is what turned a crop's demand into these requirements.
    pub efficiency: Vec<AdjustedEfficiency>,
    /// The agronomic balance the requirements came out of.
    pub balance: FertilizationBalance,
    /// `None` when nothing is required at all.
    pub ratio: Option<RatioConstruction>,
    /// The compound ranking, best first, truncated for reporting.
    pub candidates: Vec<CompositeCandidateScore>,
    pub chosen: FertilizerProgram,
    /// The other strategy, always computed, so the two are comparable
    /// without a second run.
    pub alternative: FertilizerProgram,
    /// Heuristics applied and limits hit, in the reader's language of
    /// consequence rather than of implementation.
    pub assumptions: Vec<String>,
}

/// How many ranked compounds a report carries. Enough to see why the
/// winner won; not the whole 500-product catalog.
pub const REPORTED_CANDIDATES: usize = 5;

#[cfg(test)]
mod tests {
    use super::*;

    /// The workflow's own two worked examples, PART A end to end.
    #[test]
    fn a_positive_requirement_never_rounds_out_of_the_ratio() {
        assert_eq!(round_to_nearest_ten(96.25), 100.0);
        assert_eq!(round_to_nearest_ten(63.3), 60.0);
        assert_eq!(round_to_nearest_ten(84.08), 80.0);
        assert_eq!(round_to_nearest_ten(96.18), 100.0);
        assert_eq!(round_to_nearest_ten(20.09), 20.0);
        // The one the plain rule would send to zero.
        assert_eq!(round_to_nearest_ten(3.2), 10.0);
        assert_eq!(round_to_nearest_ten(0.0), 0.0);
    }

    fn requirements(n: f64, p: f64, k: f64) -> Vec<NutrientRequirement> {
        vec![
            NutrientRequirement { nutrient: GradeNutrient::N, kg_ha: n },
            NutrientRequirement { nutrient: GradeNutrient::P2O5, kg_ha: p },
            NutrientRequirement { nutrient: GradeNutrient::K2O, kg_ha: k },
        ]
    }

    #[test]
    fn the_ratio_normalizes_on_the_smallest_positive_rounded_requirement() {
        let built = build_target_grade(&requirements(96.25, 3.2, 63.3), &[]).expect("a ratio");
        assert_eq!(built.smallest_rounded, 10.0);
        assert_eq!(built.normalized, CommercialGrade::new(10.0, 1.0, 6.0, 0.0));

        let built = build_target_grade(&requirements(84.08, 96.18, 20.09), &[]).expect("a ratio");
        assert_eq!(built.smallest_rounded, 20.0);
        assert_eq!(built.normalized, CommercialGrade::new(4.0, 5.0, 1.0, 0.0));
    }

    /// The control catalog of the mandatory case: one compound at the
    /// workflow's grade, plus the straights a plan needs to close gaps.
    fn control_catalog() -> Vec<CompositeCandidate> {
        let candidate = |id: &str, name: &str, grade: CommercialGrade| CompositeCandidate {
            source_id: id.to_string(),
            name: name.to_string(),
            grade,
            form: FertilizerForm::Unknown,
            commercialization_penalty: 0.0,
        };
        vec![
            candidate("npk_13_26_6", "NPK 13-26-6", CommercialGrade::new(13.0, 26.0, 6.0, 0.0)),
            candidate("npk_15_15_15", "NPK 15-15-15", CommercialGrade::new(15.0, 15.0, 15.0, 0.0)),
            candidate("urea", "Urea", CommercialGrade::new(46.0, 0.0, 0.0, 0.0)),
            candidate("kcl", "Muriate of potash", CommercialGrade::new(0.0, 0.0, 60.0, 0.0)),
            candidate("tsp", "Triple superphosphate", CommercialGrade::new(0.0, 46.0, 0.0, 0.0)),
            candidate("ammonium_sulfate", "Ammonium sulfate", CommercialGrade::new(21.0, 0.0, 0.0, 24.0)),
        ]
    }

    /// PART B, on the mandatory numeric case: x10, /2, /2, discretized.
    #[test]
    fn the_scaling_ladder_lands_on_a_plausible_commercial_grade() {
        let built = build_target_grade(&requirements(84.08, 96.18, 20.09), &control_catalog()).expect("a ratio");

        let ladder: Vec<String> = built.steps.iter().map(|step| step.discretized.label()).collect();
        assert_eq!(ladder[0], "40-50-10");
        assert_eq!(ladder[1], "20-25-5");
        assert_eq!(ladder[2], "10-13-3");
        assert_eq!(built.target, CommercialGrade::new(10.0, 13.0, 3.0, 0.0), "ladder was {ladder:?}");
        // 40-50-10 sums to 100%: no such bag can be manufactured.
        assert!(built.steps[0].sum_penalty > 0.0);
        assert!(built.steps.iter().filter(|step| step.chosen).count() == 1);
    }

    #[test]
    fn the_target_coefficients_are_the_worked_examples() {
        let target = CommercialGrade::new(10.0, 13.0, 3.0, 0.0);
        let coefficients = target.coefficients();
        assert!((coefficients.n_over_p.expect("N/P") - 0.769230).abs() < 1e-5);
        assert!((coefficients.p_over_k.expect("P/K") - 4.333333).abs() < 1e-5);
        assert_eq!(coefficients.k_over_s, None, "a sulfur-free grade has no K/S");

        let control = CommercialGrade::new(13.0, 26.0, 6.0, 0.0).coefficients();
        assert!((control.n_over_p.expect("N/P") - 0.5).abs() < 1e-9);
        assert!((control.p_over_k.expect("P/K") - 4.333333).abs() < 1e-5);
    }

    #[test]
    fn k_over_s_appears_only_once_sulfur_is_in_the_grade() {
        let with_sulfur = CommercialGrade::new(10.0, 20.0, 20.0, 5.0).coefficients();
        assert!((with_sulfur.k_over_s.expect("K/S") - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_product_missing_a_required_nutrient_scores_worse_than_a_lopsided_one() {
        let target = CommercialGrade::new(10.0, 13.0, 3.0, 0.0);
        let required = [GradeNutrient::N, GradeNutrient::P2O5, GradeNutrient::K2O];
        let catalog = control_catalog();

        let complete = score_candidate(&target, &required, &catalog[0]);
        let partial = score_candidate(
            &target,
            &required,
            &CompositeCandidate {
                source_id: "dap".to_string(),
                name: "DAP".to_string(),
                grade: CommercialGrade::new(18.0, 46.0, 0.0, 0.0),
                form: FertilizerForm::Unknown,
                commercialization_penalty: 0.0,
            },
        );

        assert_eq!(complete.nutrient_coverage_score, 1.0);
        assert!((partial.nutrient_coverage_score - 2.0 / 3.0).abs() < 1e-9);
        assert!(complete.total_score < partial.total_score);
        assert!(partial.explanation.contains("missing K2O"));
    }

    #[test]
    fn ranking_is_deterministic_and_puts_the_closest_compound_first() {
        let target = CommercialGrade::new(10.0, 13.0, 3.0, 0.0);
        let required = [GradeNutrient::N, GradeNutrient::P2O5, GradeNutrient::K2O];
        let ranked = rank_candidates(&target, &required, &control_catalog());

        assert_eq!(ranked.first().expect("a winner").candidate_id, "npk_13_26_6");
        // Straights are not candidates for the compound slot at all.
        assert!(ranked.iter().all(|score| score.candidate_id.starts_with("npk_")));
        let again = rank_candidates(&target, &required, &control_catalog());
        let ids: Vec<&str> = ranked.iter().map(|s| s.candidate_id.as_str()).collect();
        let ids_again: Vec<&str> = again.iter().map(|s| s.candidate_id.as_str()).collect();
        assert_eq!(ids, ids_again);
    }

    #[test]
    fn sourcing_metadata_breaks_a_tie_between_identical_grades() {
        assert_eq!(commercialization_penalty(&["Amplia comercialización en Colombia".to_string()]), 0.0);
        assert_eq!(commercialization_penalty(&["Comercialización común; fuente: Yara".to_string()]), 0.1);
        assert_eq!(commercialization_penalty(&["Comercialización regional".to_string()]), 0.2);
        assert_eq!(commercialization_penalty(&["Uso especializado (fertirriego)".to_string()]), 0.3);
        assert_eq!(commercialization_penalty(&["Uso restringido/regulado".to_string()]), 0.5);
        // No metadata is not evidence of a problem.
        assert_eq!(commercialization_penalty(&[]), 0.0);
    }

    /// PART E on the mandatory numbers, including the figure the workflow's
    /// own example quotes (P2O5 -> 369.92 kg/ha).
    #[test]
    fn the_compound_is_dosed_on_the_first_requirement_it_satisfies() {
        let grade = CommercialGrade::new(13.0, 26.0, 6.0, 0.0);
        let dose = compound_dose_kg_ha(&grade, &requirements(84.08, 96.18, 20.09)).expect("a dose");

        let of = |nutrient: GradeNutrient| dose.per_nutrient.iter().find(|(n, _)| *n == nutrient).expect("row").1;
        assert!((of(GradeNutrient::N) - 646.769).abs() < 0.01);
        assert!((of(GradeNutrient::P2O5) - 369.923).abs() < 0.01, "the workflow's own worked figure");
        assert!((of(GradeNutrient::K2O) - 334.833).abs() < 0.01);

        assert_eq!(dose.reference_nutrient, GradeNutrient::K2O);
        assert!((dose.kg_per_ha - 334.833).abs() < 0.01);
    }

    #[test]
    fn contributions_follow_the_grade_and_remainders_never_go_negative() {
        let grade = CommercialGrade::new(13.0, 26.0, 6.0, 0.0);
        let contributions = contributions_of(&grade, 334.8333);

        let of = |nutrient: GradeNutrient| {
            contributions.iter().find(|c| c.nutrient == nutrient).expect("contribution").kg_ha
        };
        assert!((of(GradeNutrient::N) - 43.528).abs() < 0.01);
        assert!((of(GradeNutrient::P2O5) - 87.056).abs() < 0.01);
        assert!((of(GradeNutrient::K2O) - 20.09).abs() < 0.01);

        let remainder = NutrientRemainder {
            nutrient: GradeNutrient::K2O,
            required_kg_ha: 20.09,
            supplied_kg_ha: 25.0,
            remaining_kg_ha: (20.09_f64 - 25.0).max(0.0),
        };
        assert_eq!(remainder.remaining_kg_ha, 0.0);
        assert!(remainder.coverage_pct() > 100.0);
    }

    /// The mandatory case, whole: compound, remainders, straights, balance.
    #[test]
    fn the_compound_plan_covers_every_requirement_with_straights() {
        let requirements = requirements(84.08, 96.18, 20.09);
        let catalog = control_catalog();
        let target = build_target_grade(&requirements, &catalog).expect("a ratio").target;

        let program = build_program(FertilizationStrategy::CompositePlusSimple, &requirements, Some(&target), &catalog, 1.0, 50.0, BlendSearchStrategy::default());

        let composite = program.composite.as_ref().expect("a compound");
        // The *ranking* still puts 13-26-6 first on grade closeness — that
        // is the shortlist. Which compound is actually applied is decided
        // by the program it implies, straights included.
        assert_eq!(composite.score.candidate_id, "npk_13_26_6");
        // It left requirements short, which is what the straights are for.
        assert!(program.lines.iter().any(|line| line.role == SourceRole::Simple));
        assert!(program.lines.iter().any(|line| line.role == SourceRole::Composite));
        assert!(program.uncovered().is_empty(), "balance: {:?}", program.balance);
        for entry in &program.balance {
            assert!(entry.coverage_pct() >= 99.9, "{entry:?}");
        }
    }

    /// A compound that carries one required nutrient is a straight with
    /// ballast attached, and the slot is better left empty.
    ///
    /// Measured on EJ-HORT/potato: the soil tested high in P, so the plan
    /// asked for N and S only. The slot was filled with 988 kg/ha of DAP —
    /// 452 kg/ha of P2O5 nobody asked for and a 1071 kg/ha program — over
    /// the 432 kg/ha of urea and ammonium sulfate that cover it exactly.
    #[test]
    fn a_compound_that_carries_one_required_nutrient_does_not_take_the_slot() {
        let requirements = vec![
            NutrientRequirement { nutrient: GradeNutrient::N, kg_ha: 177.82 },
            NutrientRequirement { nutrient: GradeNutrient::S, kg_ha: 20.0 },
        ];
        let mut catalog = control_catalog();
        catalog.push(CompositeCandidate {
            source_id: "dap".to_string(),
            name: "Diammonium phosphate (DAP)".to_string(),
            grade: CommercialGrade::new(18.0, 45.8, 0.0, 0.0),
            form: FertilizerForm::Unknown,
            commercialization_penalty: 0.0,
        });
        let target = build_target_grade(&requirements, &catalog).expect("a ratio").target;

        let program = build_program(FertilizationStrategy::CompositePlusSimple, &requirements, Some(&target), &catalog, 1.0, 50.0, BlendSearchStrategy::default());

        assert!(program.composite.is_none(), "no compound carries both N and S: {:?}", program.composite);
        assert!(program.uncovered().is_empty(), "balance: {:?}", program.balance);
        for line in &program.lines {
            assert_eq!(line.grade.get(GradeNutrient::P2O5), 0.0, "{} was bought for its P", line.source_name);
        }
        assert!(program.total_kg_per_ha < 500.0, "{} kg/ha is the DAP program again", program.total_kg_per_ha);
    }

    #[test]
    fn the_simple_blend_uses_no_compound_at_all() {
        let requirements = requirements(84.08, 96.18, 20.09);
        let catalog = control_catalog();

        let program =
            build_program(FertilizationStrategy::SimpleBlendOnly, &requirements, None, &catalog, 1.0, 50.0, BlendSearchStrategy::default());

        assert!(program.composite.is_none());
        assert!(program.lines.iter().all(|line| line.role == SourceRole::Simple));
        assert!(program.lines.iter().all(|line| !catalog
            .iter()
            .find(|c| c.source_id == line.source_id)
            .expect("in catalog")
            .is_compound()));
        assert!(program.uncovered().is_empty(), "balance: {:?}", program.balance);
    }

    /// A straight that carries a second wanted nutrient must reduce that
    /// requirement too, or the blend over-applies it.
    #[test]
    fn a_straights_cross_contribution_counts_against_the_other_requirement() {
        let requirements = vec![
            NutrientRequirement { nutrient: GradeNutrient::N, kg_ha: 42.0 },
            NutrientRequirement { nutrient: GradeNutrient::S, kg_ha: 48.0 },
        ];
        let program =
            build_program(FertilizationStrategy::SimpleBlendOnly, &requirements, None, &control_catalog(), 1.0, 50.0, BlendSearchStrategy::default());

        // 200 kg/ha of 21-0-0-24S covers both at once; a second N source
        // would mean the sulfur line's nitrogen went unaccounted.
        assert_eq!(program.lines.len(), 1);
        assert_eq!(program.lines[0].source_id, "ammonium_sulfate");
        assert!((program.lines[0].kg_per_ha - 200.0).abs() < 0.01);
        assert!(program.uncovered().is_empty());
    }

    /// The defect the search replaced. Working the largest requirement
    /// first and taking the most concentrated product for it commits to
    /// urea for the nitrogen, and then the only sulfur source in the
    /// catalog dumps 21 kg/ha of nitrogen nobody still needs. Covering the
    /// *smaller* requirement first costs less of both.
    #[test]
    fn the_search_beats_the_order_a_greedy_pass_would_have_committed_to() {
        let requirements = vec![
            NutrientRequirement { nutrient: GradeNutrient::N, kg_ha: 100.0 },
            NutrientRequirement { nutrient: GradeNutrient::S, kg_ha: 24.0 },
        ];
        let program =
            build_program(FertilizationStrategy::SimpleBlendOnly, &requirements, None, &control_catalog(), 1.0, 50.0, BlendSearchStrategy::default());

        let kg_of = |source_id: &str| {
            program.lines.iter().find(|line| line.source_id == source_id).map_or(0.0, |line| line.kg_per_ha)
        };
        // 100 kg of 21-0-0-24S covers the sulfur exactly and 21 of the 100
        // kg of N with it; urea covers the remaining 79.
        assert!((kg_of("ammonium_sulfate") - 100.0).abs() < 0.01);
        assert!((kg_of("urea") - 171.739).abs() < 0.01);
        // Largest-first would have spent 217.4 kg of urea and then 100 kg
        // of ammonium sulfate anyway: 317.4 kg of product and 21 kg/ha of
        // nitrogen over the requirement.
        assert!((program.total_kg_per_ha - 271.739).abs() < 0.01);
        for entry in &program.balance {
            assert!(entry.coverage_pct() - 100.0 < 0.01, "nothing is over-applied: {entry:?}");
        }
    }

    /// The other defect: a product that came up for two nutrients used to
    /// be printed as two lines of the same bag.
    /// PART 1, and the finding that came out of building it.
    ///
    /// Splitting is implemented for both slots — a nutrient's requirement
    /// across two straights, and the compound slot across two grades — and
    /// it is bounded, deterministic and never allowed to make a blend
    /// worse. What it does *not* do on this objective is fire often, and
    /// the reason is worth stating rather than discovering again:
    ///
    /// For one nutrient, the mass a product contributes is `req / conc`,
    /// **linear** in the share it is given; a compound part scaled by
    /// `share` is exactly `share x` its own full dose, so its contribution
    /// vector is linear too. A linear objective over a convex combination
    /// is optimised at an endpoint — the single pick. The only place an
    /// interior share can win is a *kink*: `max(supplied - required, 0)`
    /// crossing zero for some accompanying nutrient. The exhaustive
    /// ordering baseline already reaches most of those, because covering
    /// the accompanying nutrient first is the same thing by another route.
    ///
    /// So the contract this pins is the one that actually holds: the split
    /// search is never worse than the baseline, and when it does take a
    /// split it explains it.
    #[test]
    fn the_split_search_is_never_worse_than_the_single_pick_baseline() {
        let candidate = |id: &str, grade: CommercialGrade| CompositeCandidate {
            source_id: id.to_string(),
            name: id.to_string(),
            grade,
            form: FertilizerForm::Unknown,
            commercialization_penalty: 0.0,
        };
        let catalog = vec![
            candidate("kcl", CommercialGrade::new(0.0, 0.0, 60.0, 0.0)),
            candidate("sop", CommercialGrade::new(0.0, 0.0, 50.0, 18.0)),
            candidate("urea", CommercialGrade::new(46.0, 0.0, 0.0, 0.0)),
            candidate("tsp", CommercialGrade::new(0.0, 46.0, 0.0, 0.0)),
            candidate("npk_13_26_6", CommercialGrade::new(13.0, 26.0, 6.0, 0.0)),
            candidate("npk_15_15_15", CommercialGrade::new(15.0, 15.0, 15.0, 0.0)),
        ];

        // A spread of plans, so this is a property and not one example.
        let plans = [
            vec![(GradeNutrient::K2O, 100.0), (GradeNutrient::S, 20.0)],
            vec![(GradeNutrient::N, 84.08), (GradeNutrient::P2O5, 96.18), (GradeNutrient::K2O, 20.09)],
            vec![(GradeNutrient::N, 200.0), (GradeNutrient::P2O5, 30.0), (GradeNutrient::K2O, 150.0), (GradeNutrient::S, 40.0)],
            vec![(GradeNutrient::N, 60.0), (GradeNutrient::S, 55.0)],
        ];

        for plan in plans {
            let requirements: Vec<NutrientRequirement> =
                plan.iter().map(|(nutrient, kg_ha)| NutrientRequirement { nutrient: *nutrient, kg_ha: *kg_ha }).collect();
            for strategy in FertilizationStrategy::ALL {
                let target = build_target_grade(&requirements, &catalog).map(|r| r.target);
                let run = |search| {
                    build_program(strategy, &requirements, target.as_ref(), &catalog, 1.0, 50.0, search)
                };
                let single = run(BlendSearchStrategy::SinglePick);
                let split = run(BlendSearchStrategy::SplitPairs);

                // Never short where the baseline was not, never wasting
                // more, never heavier.
                assert!(
                    split.balance.iter().map(|b| b.remaining_kg_ha).sum::<f64>()
                        <= single.balance.iter().map(|b| b.remaining_kg_ha).sum::<f64>() + 1e-6,
                    "{strategy} left more uncovered on {plan:?}"
                );
                assert!(
                    split.total_kg_per_ha <= single.total_kg_per_ha + 1e-6,
                    "{strategy} got heavier on {plan:?}: {} vs {}",
                    split.total_kg_per_ha,
                    single.total_kg_per_ha
                );
                // And when it did split, it said so in words a grower can read.
                if let Some(blend) = &split.blend {
                    if !blend.splits().is_empty() {
                        assert!(blend.improvement.contains("split"), "{}", blend.improvement);
                        let shares: f64 =
                            blend.partitions.iter().filter(|p| p.nutrient == blend.splits()[0].0).map(|p| p.share).sum();
                        assert!((shares - 1.0).abs() < 1e-9, "the halves have to sum to the requirement");
                    }
                }
                if let Some(composite) = &split.composite {
                    let shares: f64 = composite.parts.iter().map(|p| p.share).sum();
                    assert!((shares - 1.0).abs() < 1e-9);
                    if composite.parts.len() > 1 {
                        assert!(composite.pairing.contains("split"));
                    }
                }
            }
        }
    }

    /// The compound slot is now decided by the program it implies, not by
    /// grade closeness alone: a "worse" grade wins when the straights it
    /// leaves behind are lighter.
    #[test]
    fn the_compound_is_chosen_on_the_whole_program_not_on_grade_closeness() {
        let requirements = requirements(84.08, 96.18, 20.09);
        let catalog = control_catalog();
        let target = build_target_grade(&requirements, &catalog).expect("a ratio").target;
        let program = build_program(
            FertilizationStrategy::CompositePlusSimple, &requirements, Some(&target), &catalog, 1.0, 50.0,
            BlendSearchStrategy::default(),
        );

        let composite = program.composite.as_ref().expect("a compound");
        // 13-26-6 is still the closest grade to the 10-13-3 target...
        assert_eq!(composite.score.candidate_id, "npk_13_26_6");
        // ...and 15-15-15 is what gets applied, because dosing it on K2O
        // (134 kg/ha) and topping up with straights totals less product
        // than 335 kg/ha of 13-26-6 plus its own top-up.
        assert_eq!(composite.parts[0].source_id, "npk_15_15_15");
        assert!((composite.kg_per_ha - 133.933).abs() < 0.01);
        assert!(program.uncovered().is_empty(), "balance: {:?}", program.balance);
        assert!(program.total_kg_per_ha < 440.0, "{}", program.total_kg_per_ha);
    }

    /// A split has to earn the extra product: on a plan where one product
    /// already lands the requirement exactly, the search must not split.
    #[test]
    fn a_split_that_buys_nothing_is_not_taken() {
        let requirements = requirements(84.08, 96.18, 20.09);
        let catalog = control_catalog();

        let single = build_program(
            FertilizationStrategy::SimpleBlendOnly, &requirements, None, &catalog, 1.0, 50.0,
            BlendSearchStrategy::SinglePick,
        );
        let split = build_program(
            FertilizationStrategy::SimpleBlendOnly, &requirements, None, &catalog, 1.0, 50.0,
            BlendSearchStrategy::SplitPairs,
        );

        let ids = |program: &FertilizerProgram| {
            let mut ids: Vec<String> = program.lines.iter().map(|l| l.source_id.clone()).collect();
            ids.sort();
            ids
        };
        assert_eq!(ids(&single), ids(&split), "nothing here is worth splitting");
        assert_eq!(split.blend.as_ref().expect("a blend").strategy, BlendSearchStrategy::SinglePick);
        assert!(split.blend.as_ref().expect("a blend").splits().is_empty());
    }

    #[test]
    fn a_product_that_covers_two_nutrients_is_one_line_not_two() {
        let requirements = vec![
            NutrientRequirement { nutrient: GradeNutrient::N, kg_ha: 100.0 },
            NutrientRequirement { nutrient: GradeNutrient::S, kg_ha: 100.0 },
        ];
        let only_ammonium_sulfate = vec![CompositeCandidate {
            source_id: "ammonium_sulfate".to_string(),
            name: "Ammonium sulfate".to_string(),
            grade: CommercialGrade::new(21.0, 0.0, 0.0, 24.0),
            form: FertilizerForm::Unknown,
            commercialization_penalty: 0.0,
        }];

        let program = build_program(FertilizationStrategy::SimpleBlendOnly, &requirements, None, &only_ammonium_sulfate, 1.0, 50.0, BlendSearchStrategy::default());

        assert_eq!(program.lines.len(), 1, "one product is one purchase: {:?}", program.lines);
        assert!((program.lines[0].kg_per_ha - 476.190).abs() < 0.01);
        assert!(program.uncovered().is_empty());
    }

    /// The answer must not depend on the order the catalog happened to
    /// load in, which is what the id tiebreak is for.
    #[test]
    fn a_reordered_catalog_reaches_the_same_blend() {
        let requirements = requirements(84.08, 96.18, 20.09);
        let forwards = control_catalog();
        let backwards: Vec<CompositeCandidate> = forwards.iter().rev().cloned().collect();
        let blend = |catalog: &[CompositeCandidate]| {
            let program =
                build_program(FertilizationStrategy::SimpleBlendOnly, &requirements, None, catalog, 1.0, 50.0, BlendSearchStrategy::default());
            let mut lines: Vec<(String, i64)> = program
                .lines
                .iter()
                .map(|line| (line.source_id.clone(), (line.kg_per_ha * 1000.0).round() as i64))
                .collect();
            lines.sort();
            lines
        };

        assert_eq!(blend(&forwards), blend(&backwards));
    }

    /// Ca and Mg are balanced by the blend but must not reach the target
    /// grade: no manufacturer sells against a six-term ratio.
    #[test]
    fn calcium_and_magnesium_are_covered_without_entering_the_target_grade() {
        let requirements = vec![
            NutrientRequirement { nutrient: GradeNutrient::N, kg_ha: 80.0 },
            NutrientRequirement { nutrient: GradeNutrient::P2O5, kg_ha: 100.0 },
            NutrientRequirement { nutrient: GradeNutrient::K2O, kg_ha: 20.0 },
            NutrientRequirement { nutrient: GradeNutrient::MgO, kg_ha: 30.0 },
        ];
        let mut catalog = control_catalog();
        catalog.push(CompositeCandidate {
            source_id: "kieserite".to_string(),
            name: "Kieserite".to_string(),
            grade: CommercialGrade::with_bases(0.0, 0.0, 0.0, 20.0, 0.0, 25.0),
            form: FertilizerForm::Sulfate,
            commercialization_penalty: 0.0,
        });

        let built = build_target_grade(&requirements, &catalog).expect("a ratio");
        assert_eq!(built.original.len(), 3, "Mg must not enter the ratio: {:?}", built.original);
        assert_eq!(built.target.get(GradeNutrient::MgO), 0.0);
        assert_eq!(built.target.label(), "10-13-3", "the NPK case is unchanged by Mg being required");

        // ...but the blend still has to cover it.
        let program = build_program(FertilizationStrategy::CompositePlusSimple, &requirements, Some(&built.target), &catalog, 1.0, 50.0, BlendSearchStrategy::default());
        assert!(program.lines.iter().any(|line| line.source_id == "kieserite"));
        assert!(program.uncovered().is_empty(), "balance: {:?}", program.balance);
        // And the sulfur kieserite brings along is on the books as waste,
        // not ignored: nothing asked for S here.
        let supplied_s: f64 = program
            .lines
            .iter()
            .flat_map(|line| &line.contributions)
            .filter(|c| c.nutrient == GradeNutrient::S)
            .map(|c| c.kg_ha)
            .sum();
        assert!(supplied_s > 0.0, "the cross-contribution has to be visible");
    }

    #[test]
    fn a_grade_carrying_bases_prints_them_the_way_a_bag_does() {
        let grade = CommercialGrade::with_bases(12.0, 4.0, 8.0, 3.0, 0.0, 2.0);
        assert_eq!(grade.label(), "12-4-8-3S-2MgO");
        assert_eq!(CommercialGrade::with_bases(0.0, 0.0, 0.0, 20.0, 0.0, 25.0).label(), "0-0-0-20S-25MgO");
        // A straight is a straight even when it carries two secondary
        // nutrients: only N/P2O5/K2O make a compound.
        assert!(!CompositeCandidate {
            source_id: "kieserite".to_string(),
            name: "Kieserite".to_string(),
            grade: CommercialGrade::with_bases(0.0, 0.0, 0.0, 20.0, 0.0, 25.0),
            form: FertilizerForm::Sulfate,
            commercialization_penalty: 0.0,
        }
        .is_compound());
    }

    #[test]
    fn a_requirement_no_product_carries_is_reported_rather_than_dropped() {
        let requirements = vec![NutrientRequirement { nutrient: GradeNutrient::S, kg_ha: 30.0 }];
        let catalog = vec![CompositeCandidate {
            source_id: "urea".to_string(),
            name: "Urea".to_string(),
            grade: CommercialGrade::new(46.0, 0.0, 0.0, 0.0),
            form: FertilizerForm::Unknown,
            commercialization_penalty: 0.0,
        }];

        let program =
            build_program(FertilizationStrategy::SimpleBlendOnly, &requirements, None, &catalog, 1.0, 50.0, BlendSearchStrategy::default());

        assert!(program.lines.is_empty());
        assert_eq!(program.uncovered().len(), 1);
        assert_eq!(program.uncovered()[0].remaining_kg_ha, 30.0);
    }

    #[test]
    fn area_and_bag_weight_scale_the_same_plan() {
        let requirements = requirements(84.08, 96.18, 20.09);
        let catalog = control_catalog();
        let target = build_target_grade(&requirements, &catalog).expect("a ratio").target;
        let plan = |area: f64, bag: f64| {
            build_program(FertilizationStrategy::CompositePlusSimple, &requirements, Some(&target), &catalog, area, bag, BlendSearchStrategy::default())
        };

        let one_ha = plan(1.0, 50.0);
        let twelve_ha = plan(12.0, 50.0);
        assert!((twelve_ha.total_kg - one_ha.total_kg * 12.0).abs() < 1e-6);
        assert!((twelve_ha.total_kg_per_ha - one_ha.total_kg_per_ha).abs() < 1e-9);

        let compound = |program: &FertilizerProgram| {
            program.lines.iter().find(|line| line.role == SourceRole::Composite).expect("compound").bags.expect("bags")
        };
        // Whatever the compound is, the bag arithmetic is the same: lighter
        // bags mean proportionally more of them, and a grower orders whole
        // ones.
        let fifty = compound(&twelve_ha);
        let forty = compound(&plan(12.0, 40.0));
        assert_eq!(fifty.bag_weight_kg, 50.0);
        assert_eq!(forty.bag_weight_kg, 40.0);
        assert!((fifty.bags_total * 50.0 - forty.bags_total * 40.0).abs() < 1e-6, "the same mass either way");
        assert!((forty.bags_total / fifty.bags_total - 50.0 / 40.0).abs() < 1e-9);
        assert_eq!(fifty.bags_total_rounded_up as f64, fifty.bags_total.ceil());
        assert_eq!(forty.bags_total_rounded_up as f64, forty.bags_total.ceil());
        assert!(forty.bags_total_rounded_up > fifty.bags_total_rounded_up);
    }

    #[test]
    fn a_sulfur_requirement_reaches_a_sulfur_bearing_product() {
        let requirements = vec![
            NutrientRequirement { nutrient: GradeNutrient::N, kg_ha: 100.0 },
            NutrientRequirement { nutrient: GradeNutrient::P2O5, kg_ha: 60.0 },
            NutrientRequirement { nutrient: GradeNutrient::K2O, kg_ha: 80.0 },
            NutrientRequirement { nutrient: GradeNutrient::S, kg_ha: 30.0 },
        ];
        let catalog = control_catalog();
        let built = build_target_grade(&requirements, &catalog).expect("a ratio");
        assert!(built.target.get(GradeNutrient::S) > 0.0, "sulfur belongs in the target grade");

        let program = build_program(FertilizationStrategy::CompositePlusSimple, &requirements, Some(&built.target), &catalog, 1.0, 50.0, BlendSearchStrategy::default());
        assert!(program.lines.iter().any(|line| line.grade.get(GradeNutrient::S) > 0.0));
        assert!(program.uncovered().is_empty(), "balance: {:?}", program.balance);
    }

    #[test]
    fn the_catalog_reader_moves_p_and_k_onto_the_visible_basis() {
        let source = FertilizerSource {
            source_id: "bulk_blend_13_26_6_3s".to_string(),
            name: "Bulk blend 13-26-6-3S".to_string(),
            composition_pct: vec![
                (Nutrient::N, 13.0),
                (Nutrient::P, 11.3471),
                (Nutrient::K, 4.9809),
                (Nutrient::S, 3.0),
            ],
            density_kg_l: None,
            form: FertilizerForm::Sulfate,
            restrictions: vec!["Comercialización regional".to_string()],
        };
        let candidate = candidate_from_source(&source, |nutrient| match nutrient {
            GradeNutrient::P2O5 => Some(2.291108362),
            GradeNutrient::K2O => Some(1.204593799),
            _ => None,
        });

        assert!((candidate.grade.get(GradeNutrient::P2O5) - 26.0).abs() < 0.01);
        assert!((candidate.grade.get(GradeNutrient::K2O) - 6.0).abs() < 0.01);
        assert_eq!(candidate.grade.label(), "13-26-6-3S");
        assert_eq!(candidate.commercialization_penalty, 0.2);
        assert!(candidate.is_compound());
    }
}
