//! Appends rows to the curated CSVs under `data/curated/`. The only
//! adapter in the project that writes.
//!
//! Append-only and header-aware: the file is opened in append mode and
//! written with `has_headers(false)`, so an existing header is never
//! duplicated and existing rows are never touched. Serialization goes
//! through `csv::Writer`, which quotes any field containing a comma, a
//! quote or a newline — hand-formatting these lines is exactly how a
//! previous session corrupted the curated files.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use crate::core::domain::{DomainError, FieldContext, SoilTest, YieldTarget};
use crate::core::ports::CuratedDataWriter;

/// Trailing column of `field_context.csv`: free text explaining where the
/// coordinates came from. Written empty for a lot registered through the
/// app — the app knows nothing about their provenance, and claiming
/// otherwise would be inventing metadata. It is still written, because a
/// short row would make the whole file unreadable to `csv::Reader`.
const NO_COORDINATES_NOTE: &str = "";

pub struct CsvCuratedWriter {
    field_context: PathBuf,
    soil_tests: PathBuf,
    yield_targets: PathBuf,
}

impl CsvCuratedWriter {
    pub fn new(field_context: impl AsRef<Path>, soil_tests: impl AsRef<Path>, yield_targets: impl AsRef<Path>) -> Self {
        Self {
            field_context: field_context.as_ref().to_path_buf(),
            soil_tests: soil_tests.as_ref().to_path_buf(),
            yield_targets: yield_targets.as_ref().to_path_buf(),
        }
    }
}

/// Appends whole records, or none of them: the writer is flushed once at
/// the end, so a mid-record failure can't leave half a line behind.
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

fn number(value: f64) -> String {
    value.to_string()
}

/// An absent coordinate is an empty field, which `serde` reads back as
/// `None` — writing `0` there would place the lot in the Gulf of Guinea.
fn optional_number(value: Option<f64>) -> String {
    value.map(number).unwrap_or_default()
}

impl CuratedDataWriter for CsvCuratedWriter {
    fn save_field_context(&self, context: &FieldContext) -> Result<(), DomainError> {
        // Column order is the header of data/curated/field_context.csv.
        let record = vec![
            context.field_id.clone(),
            context.sample_id.clone(),
            context.texture.to_string(),
            context.irrigation_system.to_string(),
            number(context.organic_matter_percent),
            number(context.ph),
            number(context.cec_cmolc_kg),
            number(context.bulk_density_kg_dm3),
            number(context.arable_depth_m),
            context.region.clone(),
            optional_number(context.latitude),
            optional_number(context.longitude),
            NO_COORDINATES_NOTE.to_string(),
        ];
        append(&self.field_context, &[record])
    }

    fn save_soil_tests(&self, tests: &[SoilTest]) -> Result<(), DomainError> {
        let records: Vec<Vec<String>> = tests
            .iter()
            .map(|test| {
                vec![
                    test.sample_id.clone(),
                    test.nutrient.to_string(),
                    number(test.value),
                    test.unit.clone(),
                    test.method.clone(),
                    number(test.layer.from_cm),
                    number(test.layer.to_cm),
                ]
            })
            .collect();
        append(&self.soil_tests, &records)
    }

    fn save_yield_target(&self, field_id: &str, crop_id: &str, target: &YieldTarget) -> Result<(), DomainError> {
        let record = vec![
            field_id.to_string(),
            crop_id.to_string(),
            number(target.value),
            target.unit.clone(),
        ];
        append(&self.yield_targets, &[record])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::{IrrigationSystem, Texture};
    use crate::core::ports::{FieldContextRepository, YieldTargetRepository};
    use crate::infra::{CsvFieldContextRepo, CsvSoilTestsRepo, CsvYieldTargetsRepo};

    /// A throwaway copy of the curated files, so the tests write for real
    /// and read back through the production readers.
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
