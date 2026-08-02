//! Bulk entry: a curated CSV in, validated rows out.
//!
//! Nobody types twenty lots into a form, and nobody types a lab panel into
//! one either — the panel arrives as a table and should leave as a table.
//! The columns here are **the curated files' own columns**, so a
//! spreadsheet export of `soil_tests.csv` round-trips, and so does anything
//! a lab or an agronomist assembles with the same headers.
//!
//! Every row still goes through `RegisterLot`, which is the only thing
//! allowed to judge a value. This module parses a file into the same raw
//! text a form produces and hands it over; it validates nothing itself, so
//! there is no second set of bounds to drift from the first.

use std::path::Path;

use crate::core::application::{LotRegistration, SoilTestEntry};
use crate::core::domain::DomainError;
use crate::core::ports::RegisterLotPort;

/// What a file turned out to be. Detected from the header rather than
/// asked for with a flag: the three curated shapes have disjoint columns,
/// and a mode flag is one more thing to get wrong about a file that already
/// says what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Lots,
    SoilTests,
    YieldTargets,
}

impl ImportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportKind::Lots => "lots",
            ImportKind::SoilTests => "soil tests",
            ImportKind::YieldTargets => "yield targets",
        }
    }

    /// The column that gives each shape away.
    ///
    /// Order matters: a lot file may carry `yield_value` for its first
    /// planning row, so `texture` has to be asked about before it.
    fn detect(header: &csv::StringRecord) -> Option<Self> {
        let has = |name: &str| header.iter().any(|column| column.trim() == name);
        match () {
            _ if has("nutrient_id") => Some(ImportKind::SoilTests),
            _ if has("texture") => Some(ImportKind::Lots),
            _ if has("yield_value") => Some(ImportKind::YieldTargets),
            _ => None,
        }
    }
}

/// What an import did, so the caller can say it out loud. A run that
/// changed nothing has to be as visible as one that changed everything.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportReport {
    pub kind: ImportKind,
    pub accepted: usize,
    /// Row number (as the file counts them, header = 1) and why.
    pub rejected: Vec<(usize, String)>,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        let mut text = format!("{} {} accepted", self.accepted, self.kind.as_str());
        if !self.rejected.is_empty() {
            text.push_str(&format!(", {} rejected", self.rejected.len()));
        }
        text
    }
}

/// Reads `path` and applies every row through `use_case`.
///
/// **Row-by-row, and a bad row does not stop the good ones.** A lab panel
/// with one impossible pH is nineteen readings worth having and one to fix,
/// and refusing the file would make the user hunt for the offending line by
/// bisection. Every rejection is reported with its line number and the use
/// case's own message.
///
/// ponytail: not a transaction. A file that fails halfway leaves the
/// accepted rows written, which is what "append-only, last row wins" makes
/// safe — re-importing a corrected file supersedes rather than duplicates,
/// except for lots, where a duplicate id is refused by name.
pub fn import(path: &Path, use_case: &dyn RegisterLotPort) -> Result<ImportReport, DomainError> {
    // Importing the app's own output into itself is never what anyone
    // means, and it is one keystroke away: the file browser lists
    // `curated/` because that is where a user's own exports live. Every row
    // would be re-applied, and on the append-only writer this used to
    // double the file each time.
    if path.parent().is_some_and(|dir| dir.file_name().is_some_and(|name| name == "curated")) {
        return Err(DomainError::InvalidInput(format!(
            "{} is the app's own curated data — importing it into itself would re-apply every row. Point at the \
             file you want to load instead.",
            path.display()
        )));
    }
    let mut reader = csv::Reader::from_path(path)
        .map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;
    let header = reader.headers().map_err(|e| DomainError::DataSource(e.to_string()))?.clone();
    let kind = ImportKind::detect(&header).ok_or_else(|| {
        DomainError::InvalidInput(format!(
            "{}: cannot tell what this file is. A lot file has a `texture` column, a soil test file has \
             `nutrient_id`, a planning file has `yield_value`.",
            path.display()
        ))
    })?;

    let column = |row: &csv::StringRecord, name: &str| -> String {
        header
            .iter()
            .position(|c| c.trim() == name)
            .and_then(|index| row.get(index))
            .unwrap_or_default()
            .trim()
            .to_string()
    };

    let mut report = ImportReport { kind, accepted: 0, rejected: Vec::new() };
    for (offset, row) in reader.records().enumerate() {
        // Header is line 1, so the first data row is line 2 — the number a
        // text editor shows.
        let line = offset + 2;
        let row = match row {
            Ok(row) => row,
            Err(e) => {
                report.rejected.push((line, e.to_string()));
                continue;
            }
        };

        let outcome = match kind {
            ImportKind::SoilTests => use_case.add_soil_tests(
                &column(&row, "sample_id"),
                &[SoilTestEntry {
                    nutrient_id: column(&row, "nutrient_id"),
                    value: column(&row, "value"),
                    unit: column(&row, "unit"),
                    method: column(&row, "method_id"),
                    depth_from_cm: column(&row, "depth_from_cm"),
                    depth_to_cm: column(&row, "depth_to_cm"),
                }],
            ),
            ImportKind::Lots => use_case.register_lot(&LotRegistration {
                field_id: column(&row, "field_id"),
                texture: column(&row, "texture"),
                irrigation_system: column(&row, "irrigation_system"),
                organic_matter_percent: column(&row, "organic_matter_percent"),
                ph: column(&row, "ph"),
                cec_cmolc_kg: column(&row, "cec"),
                bulk_density_kg_dm3: column(&row, "bulk_density_kg_dm3"),
                arable_depth_m: column(&row, "arable_depth_m"),
                region: column(&row, "region"),
                latitude: column(&row, "latitude"),
                longitude: column(&row, "longitude"),
                altitude_m: column(&row, "altitude_m"),
                area_ha: column(&row, "area_ha"),
                // A lot file may carry the first planning row with it, the
                // way the New lot form does.
                crop_id: column(&row, "crop_id"),
                yield_value: column(&row, "yield_value"),
                yield_unit: column(&row, "yield_unit"),
            }),
            ImportKind::YieldTargets => use_case.set_yield_target(
                &column(&row, "field_id"),
                &column(&row, "crop_id"),
                &column(&row, "yield_value"),
                &column(&row, "yield_unit"),
            ),
        };

        match outcome {
            Ok(()) => report.accepted += 1,
            Err(e) => report.rejected.push((line, e.to_string())),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::core::ports::{FieldContextRepository, SoilTestRepository, YieldTargetRepository};
    use crate::infra::bootstrap::{self, CuratedSeed, DataLayout};
    use crate::infra::{CsvFieldContextRepo, CsvSoilTestsRepo, CsvYieldTargetsRepo};

    /// A disposable catalog, so an import test never touches real records.
    struct Sandbox {
        root: PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("nns_import_{}_{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            bootstrap::ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("seed");
            Self { root }
        }

        fn run(&self, contents: &str) -> Result<ImportReport, DomainError> {
            let path = self.root.join("in.csv");
            std::fs::write(&path, contents).expect("write");
            import(&path, &bootstrap::build_register_lot(&DataLayout::new(&self.root, "global")))
        }

        fn curated(&self, file: &str) -> PathBuf {
            self.root.join("curated").join(file)
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const LOT: &str = "field_id,texture,irrigation_system,organic_matter_percent,ph,cec,\
                       bulk_density_kg_dm3,arable_depth_m,region,area_ha,crop_id,yield_value,yield_unit\n\
                       FINCA-A,loam,drip,3.8,5.4,16,1.25,0.25,global,8.5,corn,9,t_ha\n";

    /// A file says what it is; the user does not have to.
    #[test]
    fn each_curated_shape_is_recognised_from_its_own_header() {
        let sandbox = Sandbox::new("detect");
        assert_eq!(sandbox.run(LOT).expect("lots").kind, ImportKind::Lots);

        let tests = "sample_id,nutrient_id,value,unit,method_id,depth_from_cm,depth_to_cm\n\
                     FINCA-A,P,14,mg_per_kg,Olsen,0,25\n";
        assert_eq!(sandbox.run(tests).expect("tests").kind, ImportKind::SoilTests);

        let goals = "field_id,crop_id,yield_value,yield_unit\nFINCA-A,coffee,1.8,t_ha\n";
        assert_eq!(sandbox.run(goals).expect("goals").kind, ImportKind::YieldTargets);

        // A lot file carrying its first planning row is still a lot file:
        // `texture` is asked about before `yield_value`.
        assert!(LOT.contains("yield_value"));
    }

    /// The browser lists `curated/`, so re-importing the app's own output
    /// is one keystroke away — and it is never what anyone means.
    #[test]
    fn the_apps_own_curated_data_cannot_be_imported_into_itself() {
        let sandbox = Sandbox::new("self");
        sandbox.run(LOT).expect("a lot to have something to re-import");

        let curated = sandbox.root.join("curated").join("yield_targets.csv");
        let before = std::fs::read_to_string(&curated).expect("read");
        let error = import(&curated, &bootstrap::build_register_lot(&DataLayout::new(&sandbox.root, "global")))
            .expect_err("must refuse");
        assert!(error.to_string().contains("into itself"), "{error}");
        assert_eq!(std::fs::read_to_string(&curated).expect("read"), before, "and change nothing");
    }

    #[test]
    fn a_file_that_is_none_of_the_three_is_refused_by_name() {
        let sandbox = Sandbox::new("unknown");
        let error = sandbox.run("a,b,c\n1,2,3\n").expect_err("must refuse");
        assert!(error.to_string().contains("nutrient_id"), "the message has to name the shapes: {error}");
    }

    /// One impossible reading in a lab panel is nineteen worth keeping and
    /// one to fix — not a file to refuse.
    #[test]
    fn a_bad_row_is_reported_by_line_without_stopping_the_good_ones() {
        let sandbox = Sandbox::new("partial");
        sandbox.run(LOT).expect("the lot has to exist first");

        let panel = "sample_id,nutrient_id,value,unit,method_id,depth_from_cm,depth_to_cm\n\
                     FINCA-A,P,14,mg_per_kg,Olsen,0,25\n\
                     FINCA-A,Xx,5,mg_per_kg,Olsen,0,25\n\
                     FINCA-A,K,0.31,cmolc_per_kg,AcONH4_1N_pH7,0,25\n";
        let report = sandbox.run(panel).expect("import");

        assert_eq!(report.accepted, 2);
        assert_eq!(report.rejected.len(), 1);
        // Line 3 as a text editor counts, header included.
        assert_eq!(report.rejected[0].0, 3);
        assert!(report.rejected[0].1.contains("Xx"), "{}", report.rejected[0].1);
        assert_eq!(report.summary(), "2 soil tests accepted, 1 rejected");

        let read = CsvSoilTestsRepo::new(sandbox.curated("soil_tests.csv"))
            .get_tests_by_sample_id("FINCA-A")
            .expect("read back");
        assert_eq!(read.len(), 2, "the good rows are on disk");
    }

    /// The import runs through the same use case as the forms, so it is
    /// held to the same bounds — no second set to drift.
    #[test]
    fn imported_rows_are_validated_exactly_as_typed_ones_are() {
        let sandbox = Sandbox::new("validated");
        let bad_ph = LOT.replace(",5.4,", ",99,");
        let report = sandbox.run(&bad_ph).expect("import");
        assert_eq!(report.accepted, 0);
        assert!(report.rejected[0].1.contains("ph"), "{}", report.rejected[0].1);
        assert!(CsvFieldContextRepo::new(sandbox.curated("field_context.csv"))
            .get_context_by_field_id("FINCA-A")
            .is_err());
    }

    /// The gap the planning-row import surfaced: nothing could curate a
    /// goal for a second crop on an existing lot.
    #[test]
    fn a_second_crops_goal_can_be_curated_for_a_lot_that_already_exists() {
        let sandbox = Sandbox::new("goals");
        sandbox.run(LOT).expect("lot");

        let report = sandbox
            .run("field_id,crop_id,yield_value,yield_unit\n\
                  FINCA-A,coffee,1.8,t_ha\n\
                  FINCA-A,potato,28,t_ha\n\
                  NOPE,corn,9,t_ha\n")
            .expect("import");
        assert_eq!(report.accepted, 2);
        assert_eq!(report.rejected.len(), 1, "a goal for a lot that does not exist is refused");

        let repo = CsvYieldTargetsRepo::new(sandbox.curated("yield_targets.csv"));
        assert_eq!(repo.get_yield_target("FINCA-A", "coffee").expect("coffee").value, 1.8);
        assert_eq!(repo.get_yield_target("FINCA-A", "potato").expect("potato").value, 28.0);
        // ...and the one the lot file wrote is still there.
        assert_eq!(repo.get_yield_target("FINCA-A", "corn").expect("corn").value, 9.0);
    }

    /// A whole lot arrives in one row, planning included.
    #[test]
    fn a_lot_file_writes_the_lot_and_its_first_planning_row() {
        let sandbox = Sandbox::new("lot");
        assert_eq!(sandbox.run(LOT).expect("import").accepted, 1);

        let lot = CsvFieldContextRepo::new(sandbox.curated("field_context.csv"))
            .get_context_by_field_id("FINCA-A")
            .expect("read back");
        assert_eq!(lot.ph, 5.4);
        assert_eq!(lot.area_ha, Some(8.5), "the area comes in with the lot, not from a preference");
        assert_eq!(
            CsvYieldTargetsRepo::new(sandbox.curated("yield_targets.csv"))
                .get_yield_target("FINCA-A", "corn")
                .expect("goal")
                .value,
            9.0
        );
    }
}
