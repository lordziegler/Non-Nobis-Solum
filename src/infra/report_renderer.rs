//! Renders a [`FertilizerRecommendationReport`] as plain text lines.
//!
//! One renderer, three readers: the CLI prints these lines, the TUI shows
//! the consolidated block of them, and the PDF exporter paginates them.
//! Keeping the layout here — rather than once per front-end — is what stops
//! the printed report and the screen disagreeing about a figure.

use crate::core::domain::{
    BlendLine, FertilizerProgram, FertilizerRecommendationReport, NutrientContribution, SourceRole,
};

/// Fits Courier 9pt inside A4 margins, which is what the PDF exporter
/// paginates against.
pub const WIDTH: usize = 96;

/// The whole report, in the order the workflow asks for it.
#[must_use]
pub fn render(report: &FertilizerRecommendationReport) -> Vec<String> {
    let mut out = Vec::new();
    scenario(report, &mut out);
    balance(report, &mut out);
    requirements(report, &mut out);
    efficiency(report, &mut out);
    target_ratio(report, &mut out);
    candidates(report, &mut out);
    compound(report, &mut out);
    remainders(report, &mut out);
    consolidated(report, &mut out);
    alternative(report, &mut out);
    assumptions(report, &mut out);
    out
}

/// The consolidated table alone, for a front-end with a panel to fill
/// rather than a page.
#[must_use]
pub fn render_summary(report: &FertilizerRecommendationReport) -> Vec<String> {
    let mut out = Vec::new();
    consolidated(report, &mut out);
    out
}

fn heading(title: &str, out: &mut Vec<String>) {
    out.push(String::new());
    out.push(title.to_uppercase());
    out.push("─".repeat(WIDTH.min(title.chars().count() + 8)));
}

// ---- 1 · scenario --------------------------------------------------------

fn scenario(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    let s = &report.scenario;
    out.push("FERTILIZER SOURCE RECOMMENDATION".to_string());
    out.push("═".repeat(WIDTH));
    out.push(format!("Lot {}   ·   sample {}   ·   crop {}", s.field_id, s.sample_id, s.crop_id));
    out.push(format!("Yield goal      {} {}", s.yield_value, s.yield_unit));
    out.push(format!("Total area      {:.2} ha", s.total_area_ha));
    out.push(format!("Bag weight      {:.0} kg", s.bag_weight_kg));
    out.push(format!("Strategy        {}", s.strategy));
    out.push(format!("Reference data  profile `{}`", s.profile));
}

// ---- 1 · the agronomic balance -------------------------------------------

/// Where the requirements come from. Without it an exported plan says what
/// to buy and not why, and a reader cannot tell a large dose on a poor soil
/// from a large dose for an inflated yield goal.
fn balance(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("1 · nutrient balance", out);
    let balance = &report.balance;
    out.push(format!(
        "  {:<6} {:>14} {:>12} {:>7} {:>7} {:>13} {:>9}",
        "nutr", "in the soil", "crop needs", "basis", "eff.", "net req.", "status"
    ));
    for row in &balance.rows {
        // A nutrient with no coefficient on either basis has an *unknown*
        // demand, not a demand of zero. Printing 0.0 there would read as
        // "this crop needs none of it".
        let (demand, basis) = match &row.demand_basis {
            Some(basis) => (format!("{:>9.1}", row.demand_kg_ha), &basis[..4]),
            None => ("  no data".to_string(), "-"),
        };
        out.push(format!(
            "  {:<6} {:>11.1} kg {:>12} {:>7} {:>6.0}% {:>10.1} kg {:>9}",
            row.nutrient.as_str(),
            row.availability_kg_ha,
            demand,
            basis,
            row.efficiency_used * 100.0,
            row.net_requirement_kg_ha,
            row.soil_status.as_deref().unwrap_or("-")
        ));
    }
    out.push(format!(
        "  N mineralization factor {:.4} [{}]",
        balance.mineralization_factor,
        if balance.climate_enriched { "climate-adjusted" } else { "baseline, no climate data" }
    ));

    if let Some(t_ha) = balance.liming_t_ha {
        out.push(String::new());
        out.push(format!("  Liming  {t_ha:.2} t/ha CaCO3-equivalent"));
        if let Some(material) = &balance.liming_material {
            out.push(format!("          {material}"));
        }
    }
    if !balance.micronutrients.is_empty() {
        out.push(String::new());
        out.push("  Micronutrients (corrected to the critical level, not to crop removal):".to_string());
        for (nutrient, reading, dose) in &balance.micronutrients {
            out.push(format!(
                "    {:<4} {:<18} {}",
                nutrient.as_str(),
                reading,
                dose.as_deref().unwrap_or("at or above its critical level")
            ));
        }
    }
    for warning in &balance.warnings {
        for (index, line) in wrap(warning, WIDTH - 6).into_iter().enumerate() {
            out.push(format!("  {} {line}", if index == 0 { "[!]" } else { "   " }));
        }
    }
}

// ---- 2 · net requirements ------------------------------------------------

fn requirements(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("2 · net requirement (NF)", out);
    if report.requirements.is_empty() {
        out.push("  This plan asks for no fertilizer at all.".to_string());
        return;
    }
    out.push(format!("  {:<8} {:>14} {:>18}", "Nutrient", "NF (kg/ha)", "elemental (kg/ha)"));
    for requirement in &report.requirements {
        let elemental = report
            .elemental_requirements
            .iter()
            .find(|(nutrient, _)| *nutrient == requirement.nutrient.elemental())
            .map_or(requirement.kg_ha, |(_, kg)| *kg);
        out.push(format!(
            "  {:<8} {:>14.2} {:>18.2}  ({})",
            requirement.nutrient.as_str(),
            requirement.kg_ha,
            elemental,
            requirement.nutrient.elemental()
        ));
    }
    out.push(
        "  The left column is the commercial basis every grade is stated in; the right is what the".to_string(),
    );
    out.push("  balance computed, which is elemental for P and K.".to_string());
}

// ---- 2 · efficiency adjusted for the site --------------------------------

/// Where the requirements above came from. The dose divides by this figure,
/// so a reader who disagrees with a modifier can see exactly which reading
/// triggered it and by how much — which is the whole point of the table
/// being multiplicative and banded rather than fitted.
fn efficiency(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("3 · efficiency adjusted for this site", out);
    if report.efficiency.is_empty() {
        out.push("  Nothing required, so no efficiency was computed.".to_string());
        return;
    }

    out.push(format!("  {:<8} {:>8} {:>10} {:>10}   {}", "nutrient", "base", "modifiers", "adjusted", "held at bound"));
    for adjusted in &report.efficiency {
        out.push(format!(
            "  {:<8} {:>7.0}% {:>10.3} {:>9.0}%   {}",
            adjusted.nutrient.as_str(),
            adjusted.base * 100.0,
            adjusted.retained_fraction(),
            adjusted.adjusted * 100.0,
            if adjusted.clamped { format!("yes, floor {:.0}%", adjusted.floor * 100.0) } else { String::new() }
        ));
    }

    out.push(String::new());
    for adjusted in &report.efficiency {
        if adjusted.modifiers.is_empty() {
            out.push(format!("  {:<6} nothing about this site moved it.", adjusted.nutrient.as_str()));
            continue;
        }
        out.push(format!("  {:<6} {}", adjusted.nutrient.as_str(), adjusted.summary()));
        for modifier in &adjusted.modifiers {
            let text = format!("x{:.2}  {} — {}", modifier.factor, modifier.condition, modifier.effect);
            for (index, line) in wrap(&text, WIDTH - 12).into_iter().enumerate() {
                out.push(format!("         {}{line}", if index == 0 { "" } else { "       " }));
            }
            // On its own line and never wrapped: a citation broken across a
            // line break is one nobody can look up, and looking it up is the
            // only reason it is printed.
            out.push(format!("                {}", modifier.basis));
        }
    }
}

// ---- 3 · the target ratio ------------------------------------------------

fn target_ratio(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("4 · target commercial grade", out);
    let Some(ratio) = &report.ratio else {
        out.push("  Nothing required, so no grade was derived.".to_string());
        return;
    };

    let listed = |items: &[(String, f64)]| {
        items.iter().map(|(name, value)| format!("{name} {value:.2}")).collect::<Vec<_>>().join("   ")
    };
    out.push(format!(
        "  NF as measured    {}",
        listed(&ratio.original.iter().map(|r| (r.nutrient.as_str().to_string(), r.kg_ha)).collect::<Vec<_>>())
    ));
    out.push(format!(
        "  rounded to 10s    {}",
        listed(&ratio.rounded.iter().map(|r| (r.nutrient.as_str().to_string(), r.kg_ha)).collect::<Vec<_>>())
    ));
    out.push(format!("  smallest positive {:.0}", ratio.smallest_rounded));
    out.push(format!("  normalized ratio  {}", ratio.normalized.label()));
    out.push(String::new());
    out.push(format!(
        "  {:<6} {:<14} {:>10} {:>9} {:>9} {:>9} {:>9}",
        "step", "grade", "rounding", "sum", "size", "catalog", "penalty"
    ));
    for step in &ratio.steps {
        out.push(format!(
            "  {:<6} {:<14} {:>10.3} {:>9.3} {:>9.3} {:>9.3} {:>9.3}{}",
            step.label,
            step.discretized.label(),
            step.rounding_distortion,
            step.sum_penalty,
            step.magnitude_penalty,
            step.catalog_distance,
            step.plausibility_penalty,
            if step.chosen { "  <- chosen" } else { "" }
        ));
    }
    let coefficients = ratio.target.coefficients();
    out.push(String::new());
    out.push(format!(
        "  Target grade {}   N/P {}   P/K {}{}",
        ratio.target.label(),
        optional(coefficients.n_over_p),
        optional(coefficients.p_over_k),
        coefficients.k_over_s.map(|v| format!("   K/S {v:.3}")).unwrap_or_default()
    ));
}

fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |v| format!("{v:.3}"))
}

// ---- 4 · candidates ------------------------------------------------------

fn candidates(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("5 · compound candidates evaluated", out);
    if report.candidates.is_empty() {
        out.push("  This catalog carries no compound product for these nutrients.".to_string());
        return;
    }
    out.push(format!(
        "  {:<3} {:<32} {:<14} {:>7} {:>7} {:>7} {:>7} {:>7}",
        "#", "product", "grade", "N/P", "r-dist", "g-dist", "cover", "score"
    ));
    for (index, candidate) in report.candidates.iter().enumerate() {
        out.push(format!(
            "  {:<3} {:<32} {:<14} {:>7} {:>7.3} {:>7.3} {:>6.0}% {:>7.3}",
            index + 1,
            truncate(&candidate.candidate_name, 32),
            candidate.candidate_grade.label(),
            optional(candidate.coefficients.n_over_p),
            candidate.ratio_distance,
            candidate.grade_distance,
            candidate.nutrient_coverage_score * 100.0,
            candidate.total_score
        ));
    }
    out.push(String::new());
    let verdict = |label: &str, score: &crate::core::domain::CompositeCandidateScore, out: &mut Vec<String>| {
        let text = format!("{label}: {} — {}", score.candidate_name, score.explanation);
        // Wrapped rather than truncated: the explanation is the audit trail,
        // and the page is 96 columns wide.
        for (index, line) in wrap(&text, WIDTH - 6).into_iter().enumerate() {
            out.push(format!("  {}{line}", if index == 0 { "" } else { "    " }));
        }
    };
    if let Some(winner) = report.candidates.first() {
        verdict("Selected", winner, out);
    }
    for runner_up in report.candidates.iter().skip(1) {
        verdict("Rejected", runner_up, out);
    }
    out.push("  Lower score is better. score = ratio distance + 0.5·grade distance + 2·(1 − coverage)".to_string());
    out.push("  + 0.5·sourcing penalty; ties break on the product id, so the ranking is reproducible.".to_string());
}

// ---- 5 · the compound dose -----------------------------------------------

fn compound(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("6 · compound dose", out);
    let Some(composite) = &report.chosen.composite else {
        out.push(format!(
            "  The `{}` strategy uses no compound product.",
            report.scenario.strategy
        ));
        return;
    };
    out.push(format!("  Product   {} ({})", composite.score.candidate_name, composite.score.candidate_grade.label()));
    out.push(String::new());
    out.push(format!("  {:<8} {:>16} {:>16}", "nutrient", "NF (kg/ha)", "dose it needs"));
    for (nutrient, dose) in &composite.dose_per_nutrient {
        let requirement =
            report.requirements.iter().find(|r| r.nutrient == *nutrient).map_or(0.0, |r| r.kg_ha);
        out.push(format!(
            "  {:<8} {:>16.2} {:>13.2} kg{}",
            nutrient.as_str(),
            requirement,
            dose,
            if *nutrient == composite.reference_nutrient { "  <- reference nutrient" } else { "" }
        ));
    }
    out.push(String::new());
    out.push(format!(
        "  Dose      {:.2} kg/ha — the smallest of the column above, so no nutrient is over-applied.",
        composite.kg_per_ha
    ));
    out.push(format!("  Supplies  {}", contributions(&composite.contributions)));
}

fn contributions(contributions: &[NutrientContribution]) -> String {
    contributions
        .iter()
        .map(|c| format!("{} {:.2}", c.nutrient.as_str(), c.kg_ha))
        .collect::<Vec<_>>()
        .join("   ")
}

// ---- 6 · remainders and complements --------------------------------------

fn remainders(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("7 · remainders and complements", out);
    let straights: Vec<&BlendLine> =
        report.chosen.lines.iter().filter(|line| line.role == SourceRole::Simple).collect();
    if straights.is_empty() {
        out.push("  Nothing was left over: no straight was needed.".to_string());
    }
    for line in straights {
        out.push(format!("  {:<34} {:>10.2} kg/ha   {}", truncate(&line.source_name, 34), line.kg_per_ha, line.grade.label()));
        out.push(format!("      supplies  {}", contributions(&line.contributions)));
        out.push(format!("      chosen because {}", line.rationale));
    }
    out.push(String::new());
    out.push(format!("  {:<8} {:>12} {:>12} {:>12} {:>10}", "nutrient", "required", "supplied", "still short", "coverage"));
    for entry in &report.chosen.balance {
        out.push(format!(
            "  {:<8} {:>12.2} {:>12.2} {:>12.2} {:>9.0}%",
            entry.nutrient.as_str(),
            entry.required_kg_ha,
            entry.supplied_kg_ha,
            entry.remaining_kg_ha,
            entry.coverage_pct()
        ));
    }
}

// ---- 7 · consolidated ----------------------------------------------------

fn consolidated(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("8 · what to buy", out);
    program_table(&report.chosen, report.scenario.total_area_ha, report.scenario.bag_weight_kg, out);
}

fn program_table(program: &FertilizerProgram, area_ha: f64, bag_kg: f64, out: &mut Vec<String>) {
    if program.lines.is_empty() {
        out.push("  No product was recommended.".to_string());
        return;
    }
    out.push(format!(
        "  {:<30} {:<10} {:<12} {:>9} {:>10} {:>8} {:>7}",
        "product", "type", "grade", "kg/ha", "kg total", "bags/ha", "bags"
    ));
    for line in &program.lines {
        let (bags_per_ha, bags_total) = match line.bags {
            Some(bags) => (format!("{:.2}", bags.bags_per_ha), bags.bags_total_rounded_up.to_string()),
            None => ("-".to_string(), "-".to_string()),
        };
        out.push(format!(
            "  {:<30} {:<10} {:<12} {:>9.1} {:>10.1} {:>8} {:>7}",
            truncate(&line.source_name, 30),
            line.role.as_str(),
            line.grade.label(),
            line.kg_per_ha,
            line.kg_total,
            bags_per_ha,
            bags_total
        ));
    }
    out.push(format!(
        "  {:<30} {:<10} {:<12} {:>9.1} {:>10.1} {:>8} {:>7}",
        "TOTAL", "", "", program.total_kg_per_ha, program.total_kg, "", program.total_bags_rounded_up
    ));
    out.push(format!(
        "  Bags are {bag_kg:.0} kg and the count is rounded up per product, over {area_ha:.2} ha."
    ));
    out.push(String::new());
    out.push("  Nutrients delivered per hectare, and how much of the requirement that is:".to_string());
    for entry in &program.balance {
        out.push(format!(
            "    {:<6} {:>9.2} kg/ha of {:>9.2} required   {:.0}%{}",
            entry.nutrient.as_str(),
            entry.supplied_kg_ha,
            entry.required_kg_ha,
            entry.coverage_pct(),
            if entry.remaining_kg_ha > 0.01 { "  <- NOT COVERED" } else { "" }
        ));
    }
}

// ---- 8 · the other strategy ----------------------------------------------

fn alternative(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading(&format!("9 · alternative — {}", report.alternative.strategy), out);
    program_table(&report.alternative, report.scenario.total_area_ha, report.scenario.bag_weight_kg, out);
    out.push(String::new());
    let delta = report.alternative.total_kg_per_ha - report.chosen.total_kg_per_ha;
    out.push(format!(
        "  Against the chosen {}: {:+.1} kg/ha of product ({} lines vs {}), {} bags vs {}.",
        report.chosen.strategy,
        delta,
        report.alternative.lines.len(),
        report.chosen.lines.len(),
        report.alternative.total_bags_rounded_up,
        report.chosen.total_bags_rounded_up
    ));
    let uncovered = report.alternative.uncovered();
    if !uncovered.is_empty() {
        let names: Vec<&str> = uncovered.iter().map(|entry| entry.nutrient.as_str()).collect();
        out.push(format!("  It leaves {} uncovered.", names.join(", ")));
    }
}

// ---- 9 · assumptions -----------------------------------------------------

fn assumptions(report: &FertilizerRecommendationReport, out: &mut Vec<String>) {
    heading("10 · assumptions and limits", out);
    for assumption in &report.assumptions {
        for (index, line) in wrap(assumption, WIDTH - 4).into_iter().enumerate() {
            out.push(format!("  {} {}", if index == 0 { "•" } else { " " }, line));
        }
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() > width { text.chars().take(width.saturating_sub(1)).collect::<String>() + "…" } else { text.to_string() }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// A one-line digest for a status bar or a log.
#[must_use]
pub fn one_line(report: &FertilizerRecommendationReport) -> String {
    let products: Vec<String> = report
        .chosen
        .lines
        .iter()
        .map(|line| format!("{} {:.0} kg/ha", truncate(&line.source_name, 22), line.kg_per_ha))
        .collect();
    if products.is_empty() { "no product recommended".to_string() } else { products.join(" + ") }
}

