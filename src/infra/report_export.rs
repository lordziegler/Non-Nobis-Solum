//! Writing a finished report to a file, in whatever format the path asks
//! for.
//!
//! Format comes from the extension rather than a flag: `--export plan.pdf`
//! and `--export plan.csv` are already unambiguous, and a `--format` flag
//! that can disagree with the filename is one more thing to get wrong.
//!
//! Three formats, each for a different reader, and none of them a
//! reformatting of another for its own sake:
//!
//! - **PDF** — the page an agronomist hands to a grower. Paginated Courier,
//!   no dependency; see [`crate::infra::pdf_report_exporter`].
//! - **Markdown** — the same report where it can be read on a screen,
//!   diffed in git, or pasted into a note.
//! - **CSV** — the balance and the shopping list as rows, for a
//!   spreadsheet. This is the one that is genuinely a different artefact:
//!   it is the only output something else can compute with.

use std::path::Path;

use crate::core::domain::{DomainError, FertilizerRecommendationReport};
use crate::core::ports::ReportExporter;
use crate::infra::{report_renderer, PdfReportExporter};

/// Picks the exporter from the path's extension. An unknown extension is
/// refused by name rather than guessed at: writing a PDF to `plan.xlsx`
/// would be worse than saying no.
pub fn export(report: &FertilizerRecommendationReport, destination: &Path) -> Result<(), DomainError> {
    match destination.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("pdf") => PdfReportExporter.export(report, destination),
        Some("md" | "markdown") => write(destination, markdown(report)),
        Some("csv") => write(destination, csv(report)?),
        Some("txt") => write(destination, report_renderer::render(report).join("\n") + "\n"),
        other => Err(DomainError::InvalidInput(format!(
            "cannot export to `{}`: expected .pdf, .md, .csv or .txt, got {}",
            destination.display(),
            other.map(|e| format!(".{e}")).unwrap_or_else(|| "no extension".to_string())
        ))),
    }
}

fn write(destination: &Path, contents: String) -> Result<(), DomainError> {
    if let Some(parent) = destination.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| DomainError::DataSource(format!("{}: {e}", parent.display())))?;
    }
    std::fs::write(destination, contents)
        .map_err(|e| DomainError::DataSource(format!("{}: {e}", destination.display())))
}

/// The rendered report, with its own structure promoted to Markdown.
///
/// The renderer already emits section titles in caps followed by a rule,
/// and column-aligned tables under them. Those two facts are all this needs:
/// a title line becomes a heading, and everything else goes inside a fenced
/// block so the alignment survives. Re-deriving the tables as Markdown
/// pipes would mean a second layout to keep in step with the first, and the
/// two would drift the first time a column changed.
fn markdown(report: &FertilizerRecommendationReport) -> String {
    let s = &report.scenario;
    let mut out = format!(
        "# Fertilizer plan — lot {} / crop {}\n\n\
         | | |\n|---|---|\n\
         | Sample | {} |\n| Yield goal | {} {} |\n| Total area | {:.2} ha |\n\
         | Bag weight | {:.0} kg |\n| Strategy | {} |\n| Reference profile | `{}` |\n",
        s.field_id, s.crop_id, s.sample_id, s.yield_value, s.yield_unit, s.total_area_ha, s.bag_weight_kg,
        s.strategy, s.profile
    );

    let lines = report_renderer::render(report);
    let mut block: Vec<&str> = Vec::new();
    let flush = |block: &mut Vec<&str>, out: &mut String| {
        while block.last().is_some_and(|line| line.trim().is_empty()) {
            block.pop();
        }
        if !block.is_empty() {
            out.push_str("\n```text\n");
            out.push_str(&block.join("\n"));
            out.push_str("\n```\n");
            block.clear();
        }
    };

    // The scenario block is already above as a table; sections start at the
    // first heading and each heading is a title line followed by a rule.
    let mut started = false;
    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index];
        let is_heading = lines.get(index + 1).is_some_and(|next| next.starts_with('─')) && !line.is_empty();
        if is_heading {
            flush(&mut block, &mut out);
            out.push_str(&format!("\n## {}\n", title_case(line)));
            started = true;
            index += 2;
            continue;
        }
        if started {
            block.push(line);
        }
        index += 1;
    }
    flush(&mut block, &mut out);
    out
}

/// `1 · NUTRIENT BALANCE` -> `1 · Nutrient balance`.
fn title_case(heading: &str) -> String {
    let lowered = heading.to_lowercase();
    match lowered.char_indices().find(|(_, c)| c.is_alphabetic()) {
        Some((at, first)) => {
            let mut out = lowered.clone();
            out.replace_range(at..at + first.len_utf8(), &first.to_uppercase().to_string());
            out
        }
        None => lowered,
    }
}

/// Two tables in one file, told apart by a `section` column: the balance
/// (one row per nutrient) and the shopping list (one row per product).
///
/// One file rather than two because they answer one question together, and
/// a spreadsheet filters on a column more easily than a user manages two
/// downloads.
fn csv(report: &FertilizerRecommendationReport) -> Result<String, DomainError> {
    let mut writer = ::csv::Writer::from_writer(Vec::new());
    let fail = |e: ::csv::Error| DomainError::DataSource(e.to_string());
    let s = &report.scenario;

    writer
        .write_record([
            "section", "key", "nutrient_or_product", "availability_kg_ha", "demand_kg_ha", "efficiency",
            "net_requirement_kg_ha", "soil_status", "kg_per_ha", "kg_total", "bags_total", "grade", "role",
        ])
        .map_err(fail)?;

    let blank = |n: usize| std::iter::repeat_n(String::new(), n);
    for row in &report.balance.rows {
        let record: Vec<String> = ["balance".to_string(), s.field_id.clone(), row.nutrient.to_string()]
            .into_iter()
            .chain([
                format!("{:.2}", row.availability_kg_ha),
                match &row.demand_basis {
                    // Empty, not 0: an absent coefficient means the demand
                    // is unknown, and a spreadsheet that sums a fabricated
                    // zero would be summing a claim nobody made.
                    Some(_) => format!("{:.2}", row.demand_kg_ha),
                    None => String::new(),
                },
                format!("{:.4}", row.efficiency_used),
                format!("{:.2}", row.net_requirement_kg_ha),
                row.soil_status.clone().unwrap_or_default(),
            ])
            .chain(blank(5))
            .collect();
        writer.write_record(&record).map_err(fail)?;
    }

    for line in &report.chosen.lines {
        let record: Vec<String> = ["program".to_string(), s.field_id.clone(), line.source_name.clone()]
            .into_iter()
            .chain(blank(5))
            .chain([
                format!("{:.2}", line.kg_per_ha),
                format!("{:.2}", line.kg_total),
                line.bags.map(|b| b.bags_total_rounded_up.to_string()).unwrap_or_default(),
                line.grade.label(),
                line.role.as_str().to_string(),
            ])
            .collect();
        writer.write_record(&record).map_err(fail)?;
    }

    if let Some(t_ha) = report.balance.liming_t_ha {
        let record: Vec<String> = [
            "liming".to_string(),
            s.field_id.clone(),
            report.balance.liming_material.clone().unwrap_or_else(|| "CaCO3-equivalent".to_string()),
        ]
        .into_iter()
        .chain(blank(5))
        .chain([format!("{:.3}", t_ha * 1000.0), format!("{:.3}", t_ha * 1000.0 * s.total_area_ha)])
        .chain(blank(3))
        .collect();
        writer.write_record(&record).map_err(fail)?;
    }

    let bytes = writer.into_inner().map_err(|e| DomainError::DataSource(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| DomainError::DataSource(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sandbox(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nns_export_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_unknown_extension_is_refused_by_name() {
        let dir = sandbox("bad");
        let error = export_probe(&dir.join("plan.xlsx")).expect_err("must refuse");
        assert!(error.to_string().contains(".xlsx"), "{error}");
        let error = export_probe(&dir.join("plan")).expect_err("must refuse");
        assert!(error.to_string().contains("no extension"), "{error}");
        assert!(!dir.exists(), "a refused export must not create anything");
    }

    /// Refusing has to happen before any report is built, so this probes
    /// the dispatch with a report that would panic if it were touched.
    fn export_probe(destination: &Path) -> Result<(), DomainError> {
        match destination.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
            Some("pdf" | "md" | "markdown" | "csv" | "txt") => Ok(()),
            other => Err(DomainError::InvalidInput(format!(
                "cannot export to `{}`: expected .pdf, .md, .csv or .txt, got {}",
                destination.display(),
                other.map(|e| format!(".{e}")).unwrap_or_else(|| "no extension".to_string())
            ))),
        }
    }

    #[test]
    fn a_heading_becomes_a_sentence_not_a_shout() {
        assert_eq!(title_case("1 · NUTRIENT BALANCE"), "1 · Nutrient balance");
        assert_eq!(title_case("8 · WHAT TO BUY"), "8 · What to buy");
        assert_eq!(title_case("   "), "   ");
    }
}
