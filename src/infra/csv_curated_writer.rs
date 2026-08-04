//! Appends rows to the curated CSVs. The only adapter that writes.
//!
//! Opened in append mode with `has_headers(false)`, so an existing header
//! is never duplicated and existing rows are never touched. Serialization
//! goes through `csv::Writer`, which quotes any field containing a comma,
//! a quote or a newline — hand-formatting these lines corrupts the file.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use crate::core::domain::{DomainError, FieldContext, SoilTest, YieldTarget};
use crate::core::ports::CuratedDataWriter;

/// Writes the three curated files: the lots, their analyses and their
/// planning rows.
///
/// The only adapter in the project that writes. Holds the three paths
/// because a lot's rows live in all of them, and deleting one has to reach
/// every file in the same call rather than leave a half-removed lot behind.
pub struct CsvCuratedWriter {
    field_context: PathBuf,
    soil_tests: PathBuf,
    yield_targets: PathBuf,
}

impl CsvCuratedWriter {
    /// Points the writer at the three curated files.
    ///
    /// # Arguments
    /// * `field_context` — the lots file.
    /// * `soil_tests` — the lab analyses file.
    /// * `yield_targets` — the planning rows file.
    ///
    /// # Returns
    /// A writer over those three paths. None is opened or created here.
    #[must_use]
    pub fn new(field_context: impl AsRef<Path>, soil_tests: impl AsRef<Path>, yield_targets: impl AsRef<Path>) -> Self {
        Self {
            field_context: field_context.as_ref().to_path_buf(),
            soil_tests: soil_tests.as_ref().to_path_buf(),
            yield_targets: yield_targets.as_ref().to_path_buf(),
        }
    }
}

/// Flushed once at the end, so a mid-record failure leaves no half line.
fn append(path: &Path, records: &[Vec<String>]) -> Result<(), DomainError> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;

    let mut writer = csv::WriterBuilder::new().has_headers(false).from_writer(file);
    for record in records {
        writer
            .write_record(record)
            .map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;
    }
    writer
        .flush()
        .map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))
}

/// An absent coordinate is an empty field, which `serde` reads back as
/// `None` — writing `0` there would place the lot in the Gulf of Guinea.
fn optional_number(value: Option<f64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// Rewrites `path`, keeping the header and every row `keep` accepts, and
/// substituting whatever `keep` returns for the rows it changes.
///
/// Read-modify-**rename**: the new file is built in memory, written beside
/// the original and renamed over it. `std::fs::rename` is atomic within a
/// filesystem, so an interrupted edit leaves the original intact. A
/// truncating write in place would leave a half-written file where the
/// user's only copy of their soil analyses used to be.
///
/// Reading is `flexible` for the same reason `bootstrap::reconcile_header`
/// is: a file that predates a column has to be *editable*, not just
/// readable, or the first edit after an upgrade would refuse to open it.
fn rewrite(
    path: &Path,
    mut keep: impl FnMut(&csv::StringRecord) -> Option<Vec<String>>,
) -> Result<usize, DomainError> {
    let fail = |e: csv::Error| DomainError::DataSource(format!("{}: {e}", path.display()));
    let text = std::fs::read_to_string(path).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;

    let mut reader = csv::ReaderBuilder::new().flexible(true).has_headers(false).from_reader(text.as_bytes());
    let rows: Vec<csv::StringRecord> = reader.records().collect::<Result<_, _>>().map_err(fail)?;
    let Some(header) = rows.first() else {
        return Ok(0);
    };

    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(header).map_err(fail)?;
    let mut changed = 0;
    for row in rows.iter().skip(1) {
        match keep(row) {
            Some(replacement) => {
                if replacement.iter().ne(row.iter()) {
                    changed += 1;
                }
                writer.write_record(&replacement).map_err(fail)?;
            }
            None => changed += 1,
        }
    }
    // Nothing matched, so nothing is written. Not an optimisation: a
    // refused edit or a delete of a lot that is not there has to leave the
    // file byte-for-byte as it was, and a rewrite would still normalize
    // quoting and churn the mtime for no change anyone asked for.
    if changed == 0 {
        return Ok(0);
    }
    let contents = writer.into_inner().map_err(|e| DomainError::DataSource(e.to_string()))?;

    let temporary = path.with_extension("csv.editing");
    std::fs::write(&temporary, contents)
        .map_err(|e| DomainError::DataSource(format!("{}: {e}", temporary.display())))?;
    std::fs::rename(&temporary, path).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))?;
    Ok(changed)
}

/// Column order of `field_context.csv`, shared by the append and the
/// replace so an edit can never write a row in a different shape from the
/// one it replaces.
fn field_context_record(context: &FieldContext) -> Vec<String> {
    vec![
        context.field_id.clone(),
        context.sample_id.clone(),
        context.texture.to_string(),
        context.irrigation_system.to_string(),
        context.organic_matter_percent.to_string(),
        context.ph.to_string(),
        context.cec_cmolc_kg.to_string(),
        context.bulk_density_kg_dm3.to_string(),
        context.arable_depth_m.to_string(),
        context.region.clone(),
        optional_number(context.latitude),
        optional_number(context.longitude),
        optional_number(context.altitude_m),
        optional_number(context.area_ha),
        // Trailing `coordinates_note`: free text explaining where the
        // coordinates came from. Empty for a lot registered through the app
        // — it knows nothing about their provenance — but still written,
        // because a short row makes the whole file unreadable.
        String::new(),
    ]
}

impl CuratedDataWriter for CsvCuratedWriter {
    fn save_field_context(&self, context: &FieldContext) -> Result<(), DomainError> {
        append(&self.field_context, &[field_context_record(context)])
    }

    fn replace_field_context(&self, context: &FieldContext) -> Result<(), DomainError> {
        let replacement = field_context_record(context);
        let mut found = false;
        rewrite(&self.field_context, |row| {
            if row.get(0) == Some(context.field_id.as_str()) {
                found = true;
                // The note column is the one thing the app did not write and
                // must not erase: whoever recorded where the coordinates came
                // from knew something the app does not.
                let mut kept = replacement.clone();
                if let Some(note) = row.get(14).filter(|note| !note.is_empty()) {
                    kept[14] = note.to_string();
                }
                return Some(kept);
            }
            Some(row.iter().map(String::from).collect())
        })?;

        // Checked after the rewrite rather than before: one pass over the
        // file, and an edit that matched nothing has changed nothing.
        if found { Ok(()) } else { Err(DomainError::NotFound(format!("no lot {} to edit", context.field_id))) }
    }

    fn delete_lot(&self, field_id: &str) -> Result<usize, DomainError> {
        let drop_matching = |path: &Path, column: usize| {
            rewrite(path, |row| if row.get(column) == Some(field_id) { None } else { Some(row.iter().map(String::from).collect()) })
        };
        // The lot last: if a later file fails, the lot is still there to
        // retry against rather than orphaning its analyses.
        let tests = drop_matching(&self.soil_tests, 0)?;
        let targets = drop_matching(&self.yield_targets, 0)?;
        let lots = drop_matching(&self.field_context, 0)?;
        Ok(tests + targets + lots)
    }

    /// Replaces the row for this (field, crop) if there is one, appends
    /// otherwise.
    ///
    /// It used to only append, because `CsvYieldTargetsRepo` collapses a
    /// repeated key to the last row — so a correction *read* right while
    /// the file grew forever. That is the documented "append-only, last row
    /// wins" contract, and it cost a user 57 912 copies of two rows: 1.1 MB
    /// of `Test,potato,30,t_ha`. Replacing in place is identical to every
    /// reader and cannot grow without bound. What it gives up is seeing the
    /// superseded value in the file, which nothing reads and no test
    /// asserted.
    fn save_yield_target(&self, field_id: &str, crop_id: &str, target: &YieldTarget) -> Result<(), DomainError> {
        let record = vec![field_id.to_string(), crop_id.to_string(), target.value.to_string(), target.unit.clone()];
        let mut replaced = false;
        rewrite(&self.yield_targets, |row| {
            if row.get(0) == Some(field_id) && row.get(1) == Some(crop_id) {
                replaced = true;
                return Some(record.clone());
            }
            Some(row.iter().map(String::from).collect())
        })?;
        if replaced { Ok(()) } else { append(&self.yield_targets, &[record]) }
    }

    /// Same rule as the yield goals, and for the same reason:
    /// `CsvSoilTestsRepo` collapses a repeated (sample, nutrient, depth) to
    /// the last row, so replacing in place reads identically and cannot
    /// grow without bound.
    fn save_soil_tests(&self, tests: &[SoilTest]) -> Result<(), DomainError> {
        let records: Vec<Vec<String>> = tests
            .iter()
            .map(|test| {
                vec![
                    test.sample_id.clone(),
                    test.nutrient.to_string(),
                    test.value.to_string(),
                    test.unit.clone(),
                    test.method.clone(),
                    test.layer.from_cm.to_string(),
                    test.layer.to_cm.to_string(),
                ]
            })
            .collect();

        let mut fresh: Vec<Vec<String>> = Vec::new();
        for record in records {
            let mut replaced = false;
            rewrite(&self.soil_tests, |row| {
                // The reader's key: same sample, same nutrient, same depth.
                let same = row.get(0) == Some(record[0].as_str())
                    && row.get(1) == Some(record[1].as_str())
                    && row.get(5) == Some(record[5].as_str())
                    && row.get(6) == Some(record[6].as_str());
                if same {
                    replaced = true;
                    return Some(record.clone());
                }
                Some(row.iter().map(String::from).collect())
            })?;
            if !replaced {
                fresh.push(record);
            }
        }
        append(&self.soil_tests, &fresh)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::{IrrigationSystem, Texture};
    use crate::core::ports::{FieldContextRepository, YieldTargetRepository};
    use crate::infra::{CsvFieldContextRepo, CsvSoilTestsRepo, CsvYieldTargetsRepo};

    /// A throwaway copy, so the tests write for real and read back through
    /// the production readers.
    struct Sandbox {
        dir: PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("nns_writer_{}_{name}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("sandbox dir");
            for file in ["field_context.csv", "soil_tests.csv", "yield_targets.csv"] {
                std::fs::copy(Path::new("data/curated").join(file), dir.join(file)).expect("seed curated file");
            }
            Self { dir }
        }

        fn writer(&self) -> CsvCuratedWriter {
            CsvCuratedWriter::new(
                self.dir.join("field_context.csv"),
                self.dir.join("soil_tests.csv"),
                self.dir.join("yield_targets.csv"),
            )
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn context(field_id: &str, region: &str) -> FieldContext {
        FieldContext {
            field_id: field_id.to_string(),
            sample_id: field_id.to_string(),
            texture: Texture::SandyLoam,
            irrigation_system: IrrigationSystem::Sprinkler,
            organic_matter_percent: 4.1,
            ph: 5.9,
            cec_cmolc_kg: 14.0,
            bulk_density_kg_dm3: 1.25,
            arable_depth_m: 0.25,
            region: region.to_string(),
            latitude: None,
            longitude: None,
            altitude_m: None,
            area_ha: Some(12.0),
        }
    }

    #[test]
    fn an_appended_lot_reads_back_and_leaves_the_existing_rows_alone() {
        let sandbox = Sandbox::new("roundtrip");
        sandbox.writer().save_field_context(&context("LOT-003", "global")).expect("write");

        let repo = CsvFieldContextRepo::new(sandbox.dir.join("field_context.csv"));
        let written = repo.get_context_by_field_id("LOT-003").expect("read back");
        assert_eq!(written.texture, Texture::SandyLoam);
        assert_eq!(written.irrigation_system, IrrigationSystem::Sprinkler);
        assert_eq!(written.arable_depth_m, 0.25);
        assert_eq!(written.latitude, None);
        // The shipped rows are still readable — a corrupted file would
        // fail here, not on the new row.
        assert_eq!(repo.get_context_by_field_id("LOT-001").expect("shipped row").texture, Texture::Loam);
        assert_eq!(repo.list_contexts().expect("list").len(), 3);
    }

    /// The area belongs to the lot, so it has to survive a round trip like
    /// every other field of it — and stay `None` when nobody stated one,
    /// rather than becoming a hectare nobody measured.
    /// The bug this closed cost a user 57 912 copies of two rows — 1.1 MB
    /// of `Test,potato,30,t_ha`. Correcting a goal a thousand times has to
    /// leave one row, not a thousand.
    #[test]
    fn correcting_a_goal_replaces_its_row_instead_of_growing_the_file() {
        let sandbox = Sandbox::new("no_growth");
        let path = sandbox.dir.join("yield_targets.csv");
        let before = std::fs::read_to_string(&path).expect("read").lines().count();

        for value in [10.0, 11.0, 12.0, 13.0] {
            sandbox
                .writer()
                .save_yield_target("LOT-001", "corn", &YieldTarget { value, unit: "t_ha".to_string() })
                .expect("write");
        }
        assert_eq!(
            std::fs::read_to_string(&path).expect("read").lines().count(),
            before,
            "four corrections to one goal are one row"
        );
        assert_eq!(
            CsvYieldTargetsRepo::new(&path).get_yield_target("LOT-001", "corn").expect("read").value,
            13.0,
            "and the last one still wins"
        );

        // A different crop is a different goal, so it does add a row.
        sandbox
            .writer()
            .save_yield_target("LOT-001", "wheat", &YieldTarget { value: 6.0, unit: "t_ha".to_string() })
            .expect("write");
        assert_eq!(std::fs::read_to_string(&path).expect("read").lines().count(), before + 1);
    }

    /// Same rule for a lab reading: correcting one is not adding one.
    #[test]
    fn correcting_a_reading_replaces_its_row() {
        use crate::core::domain::{Depth, Nutrient};
        use crate::core::ports::SoilTestRepository;

        let sandbox = Sandbox::new("reading_growth");
        let path = sandbox.dir.join("soil_tests.csv");
        let before = std::fs::read_to_string(&path).expect("read").lines().count();
        let reading = |value: f64, to_cm: f64| SoilTest {
            sample_id: "LOT-001".to_string(),
            nutrient: Nutrient::P,
            value,
            unit: "mg_per_kg".to_string(),
            method: "Olsen".to_string(),
            layer: Depth { from_cm: 0.0, to_cm },
        };

        for value in [2.0, 3.0, 4.0] {
            sandbox.writer().save_soil_tests(&[reading(value, 20.0)]).expect("write");
        }
        assert_eq!(std::fs::read_to_string(&path).expect("read").lines().count(), before);

        // A different depth is a different measurement, not a correction.
        sandbox.writer().save_soil_tests(&[reading(31.0, 40.0)]).expect("write");
        assert_eq!(std::fs::read_to_string(&path).expect("read").lines().count(), before + 1);

        let read = CsvSoilTestsRepo::new(&path).get_tests_by_sample_id("LOT-001").expect("read");
        let at = |to_cm: f64| {
            read.iter().find(|t| t.nutrient == Nutrient::P && t.layer.to_cm == to_cm).map(|t| t.value)
        };
        assert_eq!(at(20.0), Some(4.0));
        assert_eq!(at(40.0), Some(31.0));
    }

    #[test]
    fn a_lots_planted_area_is_written_and_read_back() {
        let sandbox = Sandbox::new("area");
        let repo = CsvFieldContextRepo::new(sandbox.dir.join("field_context.csv"));

        sandbox.writer().save_field_context(&context("LOT-006", "global")).expect("write");
        assert_eq!(repo.get_context_by_field_id("LOT-006").expect("read").area_ha, Some(12.0));

        let mut without = context("LOT-007", "global");
        without.area_ha = None;
        sandbox.writer().save_field_context(&without).expect("write");
        assert_eq!(
            repo.get_context_by_field_id("LOT-007").expect("read").area_ha,
            None,
            "no area on file must not become a fabricated hectare"
        );

        // And an edit can set one on a lot that had none.
        let mut corrected = without;
        corrected.area_ha = Some(4.5);
        sandbox.writer().replace_field_context(&corrected).expect("edit");
        assert_eq!(repo.get_context_by_field_id("LOT-007").expect("read").area_ha, Some(4.5));
        assert_eq!(repo.list_contexts().expect("list").len(), 4);
    }

    #[test]
    fn a_field_containing_a_comma_is_quoted_rather_than_splitting_the_row() {
        let sandbox = Sandbox::new("comma");
        // A region name with a comma is the realistic case: "Nariño, CO".
        sandbox
            .writer()
            .save_field_context(&context("LOT-004", "Nariño, Colombia"))
            .expect("write");

        let repo = CsvFieldContextRepo::new(sandbox.dir.join("field_context.csv"));
        assert_eq!(repo.get_context_by_field_id("LOT-004").expect("read back").region, "Nariño, Colombia");
        assert_eq!(repo.list_contexts().expect("list").len(), 3, "the row must not have split in two");
    }

    #[test]
    fn appended_soil_tests_read_back_as_a_sample() {
        use crate::core::domain::{Depth, Nutrient, SoilTest};
        use crate::core::ports::SoilTestRepository;

        let sandbox = Sandbox::new("sample");
        let test = SoilTest {
            sample_id: "LOT-003".to_string(),
            nutrient: Nutrient::P,
            value: 18.0,
            unit: "mg_per_kg".to_string(),
            method: "Olsen".to_string(),
            layer: Depth { from_cm: 0.0, to_cm: 20.0 },
        };
        sandbox.writer().save_soil_tests(&[test]).expect("write");

        let read = CsvSoilTestsRepo::new(sandbox.dir.join("soil_tests.csv"))
            .get_tests_by_sample_id("LOT-003")
            .expect("read back");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].nutrient, Nutrient::P);
        assert_eq!(read[0].layer.to_cm, 20.0);
    }

    /// If the reader kept the first row, a correction would be accepted and
    /// then silently ignored by every plan — the worst failure this has.
    #[test]
    fn a_corrected_lab_value_supersedes_the_one_it_replaces() {
        use crate::core::domain::{Depth, Nutrient, SoilTest};
        use crate::core::ports::SoilTestRepository;

        let sandbox = Sandbox::new("correction");
        let reading = |value: f64, to_cm: f64| SoilTest {
            sample_id: "LOT-001".to_string(),
            nutrient: Nutrient::P,
            value,
            unit: "mg_per_kg".to_string(),
            method: "Olsen".to_string(),
            layer: Depth { from_cm: 0.0, to_cm },
        };
        // LOT-001 ships with P = 18 at 0-20 cm. Correct it, then add a
        // genuinely different measurement at a deeper layer.
        sandbox.writer().save_soil_tests(&[reading(2.0, 20.0)]).expect("correction");
        sandbox.writer().save_soil_tests(&[reading(31.0, 40.0)]).expect("deeper layer");

        let read = CsvSoilTestsRepo::new(sandbox.dir.join("soil_tests.csv"))
            .get_tests_by_sample_id("LOT-001")
            .expect("read back");
        let at = |to_cm: f64| {
            read.iter()
                .find(|t| t.nutrient == Nutrient::P && t.layer.to_cm == to_cm)
                .map(|t| t.value)
        };
        assert_eq!(at(20.0), Some(2.0), "the later row for the same depth must win");
        assert_eq!(at(40.0), Some(31.0), "a different depth is a new measurement, not a correction");
        assert_eq!(read.iter().filter(|t| t.nutrient == Nutrient::P).count(), 2);
    }

    #[test]
    fn a_revised_yield_goal_supersedes_the_one_it_replaces() {
        let sandbox = Sandbox::new("revised_goal");
        let repo = CsvYieldTargetsRepo::new(sandbox.dir.join("yield_targets.csv"));
        // LOT-001/corn ships at 9.5 t_ha.
        sandbox
            .writer()
            .save_yield_target("LOT-001", "corn", &YieldTarget { value: 11.0, unit: "t_ha".to_string() })
            .expect("write");

        assert_eq!(repo.get_yield_target("LOT-001", "corn").expect("read back").value, 11.0);
        // And the lot picker must not offer the goal the planner won't use.
        let listed = repo.list_targets().expect("list");
        assert_eq!(listed.len(), 2, "the revision replaces the row, it doesn't add a lot");
        assert_eq!(listed.iter().find(|t| t.crop_id == "corn").expect("corn").target.value, 11.0);
    }

    /// The gap `replace_field_context` closed: a lot could be created and
    /// never corrected, because an append with the same id is refused.
    #[test]
    fn a_lot_can_be_edited_in_place_without_disturbing_the_others() {
        let sandbox = Sandbox::new("edit");
        let path = sandbox.dir.join("field_context.csv");
        let repo = CsvFieldContextRepo::new(&path);

        let mut corrected = context("LOT-001", "andina_colombia");
        corrected.ph = 5.1;
        corrected.texture = Texture::Clay;
        sandbox.writer().replace_field_context(&corrected).expect("edit");

        let read = repo.get_context_by_field_id("LOT-001").expect("read back");
        assert_eq!(read.ph, 5.1);
        assert_eq!(read.texture, Texture::Clay);
        // No row was added, and the other lot is untouched.
        assert_eq!(repo.list_contexts().expect("list").len(), 2);
        assert_eq!(repo.get_context_by_field_id("LOT-002").expect("other lot").texture, Texture::ClayLoam);

        // The file is still a valid CSV with its header intact.
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.starts_with("field_id,sample_id,texture"));
        assert_eq!(text.lines().count(), 3, "header plus two lots");
        assert!(!sandbox.dir.join("field_context.csv.editing").exists(), "the temp file has to be renamed away");
    }

    /// The app must not erase a note it did not write.
    #[test]
    fn an_edit_keeps_the_provenance_note_the_app_never_had() {
        let sandbox = Sandbox::new("note");
        let path = sandbox.dir.join("field_context.csv");
        let before = std::fs::read_to_string(&path).expect("read");
        assert!(before.contains("illustrative"), "the shipped rows carry a note");

        let mut corrected = context("LOT-001", "global");
        corrected.ph = 6.9;
        sandbox.writer().replace_field_context(&corrected).expect("edit");

        let after = std::fs::read_to_string(&path).expect("read");
        assert!(after.contains("illustrative"), "the note is somebody's knowledge, not ours to drop");
        assert_eq!(CsvFieldContextRepo::new(&path).get_context_by_field_id("LOT-001").expect("read").ph, 6.9);
    }

    #[test]
    fn editing_a_lot_that_does_not_exist_is_refused_rather_than_inserted() {
        let sandbox = Sandbox::new("missing");
        let path = sandbox.dir.join("field_context.csv");
        let before = std::fs::read_to_string(&path).expect("read");

        let error = sandbox.writer().replace_field_context(&context("LOT-404", "global")).expect_err("must refuse");
        assert!(matches!(error, DomainError::NotFound(_)), "{error}");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), before, "a refused edit changes nothing");
    }

    /// Deleting reaches all three files, because a lot without its analyses
    /// is worse than no lot at all.
    #[test]
    fn deleting_a_lot_takes_its_analyses_and_planning_rows_with_it() {
        use crate::core::ports::SoilTestRepository;

        let sandbox = Sandbox::new("delete");
        let removed = sandbox.writer().delete_lot("LOT-001").expect("delete");
        assert!(removed >= 3, "a lot, its sample rows and its planning row: {removed}");

        let contexts = CsvFieldContextRepo::new(sandbox.dir.join("field_context.csv"));
        assert!(contexts.get_context_by_field_id("LOT-001").is_err());
        // ...and only that lot.
        assert!(contexts.get_context_by_field_id("LOT-002").is_ok());
        assert_eq!(contexts.list_contexts().expect("list").len(), 1);

        assert!(CsvSoilTestsRepo::new(sandbox.dir.join("soil_tests.csv"))
            .get_tests_by_sample_id("LOT-001")
            .map_or(true, |tests| tests.is_empty()));
        assert!(!CsvSoilTestsRepo::new(sandbox.dir.join("soil_tests.csv"))
            .get_tests_by_sample_id("LOT-002")
            .expect("the other sample survives")
            .is_empty());
        assert!(CsvYieldTargetsRepo::new(sandbox.dir.join("yield_targets.csv"))
            .get_yield_target("LOT-001", "corn")
            .is_err());
    }

    /// A comma inside a field must survive a rewrite, or the edit that
    /// preserved the data would be the one that split the row in two.
    #[test]
    fn a_rewrite_preserves_quoting_and_never_leaves_a_partial_file() {
        let sandbox = Sandbox::new("quoting");
        let path = sandbox.dir.join("field_context.csv");
        sandbox.writer().save_field_context(&context("LOT-005", "Nariño, Colombia")).expect("append");

        let mut corrected = context("LOT-005", "Nariño, Colombia");
        corrected.ph = 7.2;
        sandbox.writer().replace_field_context(&corrected).expect("edit");

        let repo = CsvFieldContextRepo::new(&path);
        let read = repo.get_context_by_field_id("LOT-005").expect("read back");
        assert_eq!(read.region, "Nariño, Colombia");
        assert_eq!(read.ph, 7.2);
        assert_eq!(repo.list_contexts().expect("list").len(), 3, "the row must not have split in two");
    }

    #[test]
    fn a_yield_target_reads_back_through_its_own_repository() {
        let sandbox = Sandbox::new("target");
        sandbox
            .writer()
            .save_yield_target("LOT-003", "wheat", &YieldTarget { value: 6.5, unit: "t_ha".to_string() })
            .expect("write");

        let repo = CsvYieldTargetsRepo::new(sandbox.dir.join("yield_targets.csv"));
        let target = repo.get_yield_target("LOT-003", "wheat").expect("read back");
        assert_eq!((target.value, target.unit.as_str()), (6.5, "t_ha"));
        assert_eq!(repo.list_targets().expect("list").len(), 3);
    }
}
