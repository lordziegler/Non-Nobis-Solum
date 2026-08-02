//! Composition root: the only place that knows about file paths.
//!
//! Adding a reference profile never touches `core` — create
//! `data/reference/<name>/` with the same files as `global` and pass
//! `--profile <name>`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::application::{
    CalculateFertilityPlan, InspectScenario, ListLots, ListSupportedCrops, RecommendFertilizerProgram, RegisterLot,
};
use crate::core::domain::DomainError;
use crate::core::ports::AgroclimaticRepository;
use crate::infra::{
    shipped_data, CachedAgroclimaticRepo, CsvCriticalLevelsRepo, CsvCropCatalogRepo, CsvCuratedWriter,
    CsvFertilizerSourcesRepo, CsvFieldContextRepo, CsvLimingMaterialsRepo, CsvNutrientRemovalRepo, CsvSoilTestsRepo,
    CsvSoilQualityThresholdsRepo, CsvYieldTargetsRepo, NasaPowerRepo, StaticConversionFactorsRepo,
    StaticLimingRulesRepo, TomlEfficiencyBandsRepo, YamlEfficiencyRulesRepo,
};

/// Paths for a chosen profile plus the curated directory.
/// `conversion_factors.toml` is shared across every profile: atomic
/// weights are universal science, not regional agronomy.
pub struct DataLayout {
    data_root: PathBuf,
    profile: String,
}

impl DataLayout {
    pub fn new(data_root: impl AsRef<Path>, profile: impl Into<String>) -> Self {
        Self { data_root: data_root.as_ref().to_path_buf(), profile: profile.into() }
    }

    fn reference(&self, filename: &str) -> PathBuf {
        self.data_root.join("reference").join(&self.profile).join(filename)
    }

    fn shared_reference(&self, filename: &str) -> PathBuf {
        self.data_root.join("reference").join("global").join(filename)
    }

    fn curated(&self, filename: &str) -> PathBuf {
        self.data_root.join("curated").join(filename)
    }
}

/// What a long-lived front-end holds instead of a wired use case. Use
/// cases are rebuilt from [`App::layout`] on every action, because
/// switching profile switches which reference files back them.
pub struct App {
    pub data_root: PathBuf,
    pub profile: String,
}

/// Default composition for the TUI: the same data root and `global`
/// profile the CLI defaults to.
pub fn build_app() -> App {
    App { data_root: default_data_root(), profile: "global".to_string() }
}

/// The repository's own `data/`, for tests that read the demonstration
/// lots shipped alongside the code.
///
/// Deliberately not [`build_app`]: that resolves to the user's real data
/// root now, so a test calling it would read — and, on the write path,
/// append to — somebody's actual records.
#[cfg(test)]
pub fn build_app_from_repo_data() -> App {
    App { data_root: PathBuf::from("data"), profile: "global".to_string() }
}

/// Where an installed copy keeps its catalog:
/// `$XDG_DATA_HOME/non-nobis-solum`, or `~/.local/share/non-nobis-solum`.
///
/// A relative default would resolve against the working directory, which
/// is the repository only while developing — installed to `~/.cargo/bin`
/// and run from anywhere else it would look for a `data/` that isn't
/// there.
pub fn default_data_root() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        // The XDG spec says a relative value is to be ignored.
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".local/share"));
    base.join("non-nobis-solum")
}

/// How much of the curated files [`ensure_data_root`] lays down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CuratedSeed {
    /// Column headers alone. The user's own dataset starts empty: the two
    /// demonstration lots are the repository's, not anybody's soil.
    HeadersOnly,
    /// The demonstration lots too — the `--test` catalog, which exists to
    /// be planned against.
    WithExamples,
}

/// A disposable catalog for `--test`, under the system temp directory
/// because nothing in it may ever be mistaken for the user's records.
pub fn test_data_root() -> PathBuf {
    std::env::temp_dir().join("non-nobis-solum-test")
}

/// Puts the shipped catalog under `root`.
///
/// The user's own records are written only when absent — nothing may ever
/// overwrite those. The literature and the examples are *refreshed*: they
/// are the release's, not the user's, and the settings screen already
/// calls the reference directory read-only.
///
/// That rule used to apply to the literature too, and it shipped a bug
/// with a long fuse: a catalog seeded before `nutrient_removal.csv` gained
/// `harvested_organ` kept the old nine-column header forever, so every
/// plan died on `missing field harvested_organ` with nothing on screen
/// explaining why. A schema the code no longer reads is not a preference
/// worth preserving.
///
/// An edited copy is not deleted, though: whatever differs from the
/// shipped file is moved aside to `<name>.bak` before the new one lands.
pub fn ensure_data_root(root: &Path, curated: CuratedSeed) -> Result<(), DomainError> {
    let write = |path: &Path, contents: &str| -> Result<(), DomainError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DomainError::DataSource(format!("{}: {e}", parent.display())))?;
        }
        std::fs::write(path, contents).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))
    };
    let seed = |relative: &str, contents: String| -> Result<(), DomainError> {
        let path = root.join(relative);
        if path.exists() {
            return Ok(());
        }
        write(&path, &contents)
    };
    // Rewritten only when it actually differs, so a start that changes
    // nothing touches no file and no `.bak` is ever written for nothing.
    let refresh = |relative: &str, contents: &str| -> Result<(), DomainError> {
        let path = root.join(relative);
        match std::fs::read_to_string(&path) {
            Ok(found) if found == contents => return Ok(()),
            Ok(_) => {
                let aside = path.with_extension(format!(
                    "{}.bak",
                    path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
                ));
                std::fs::rename(&path, &aside)
                    .map_err(|e| DomainError::DataSource(format!("{}: {e}", aside.display())))?;
            }
            Err(_) => {}
        }
        write(&path, contents)
    };

    for (relative, contents) in shipped_data::REFERENCE {
        refresh(relative, contents)?;
    }
    for (relative, contents) in shipped_data::EXAMPLES {
        refresh(relative, contents)?;
    }
    for (relative, contents) in shipped_data::CURATED {
        match curated {
            CuratedSeed::HeadersOnly => seed(relative, shipped_data::header_of(contents))?,
            CuratedSeed::WithExamples => seed(relative, contents.to_string())?,
        }
        reconcile_header(&root.join(relative), &shipped_data::header_of(contents))?;
    }
    Ok(())
}

/// Brings a curated file's header up to the schema the code writes.
///
/// The one hole the "never overwrite" rule above leaves. A catalog created
/// before a column existed keeps its old header forever, while
/// `CsvCuratedWriter` goes on appending rows shaped by today's struct:
/// `field_context.csv` gained `altitude_m` in session 13, so every lot
/// registered after that was one field wider than the header describing
/// it, and `csv` refuses the whole file — "found record with 14 fields, but
/// the previous record has 13". Nothing could read the lot list, which in
/// the TUI is every screen.
///
/// Runs on every start, before anything reads. A row is matched to the new
/// columns **by name** when its width says it was written under the old
/// header, and passed through untouched when its width says it was already
/// written under the new one — which is the shape the bug produced. Width
/// is what tells the two apart; name-mapping a row that is already correct
/// would shift its values by a column.
fn reconcile_header(path: &Path, shipped_header: &str) -> Result<(), DomainError> {
    let fail = |e: csv::Error| DomainError::DataSource(format!("{}: {e}", path.display()));
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(());
    };

    // `flexible` because the file this exists to repair is, by definition,
    // ragged — a strict reader cannot even open it to look.
    let mut reader = csv::ReaderBuilder::new().flexible(true).has_headers(false).from_reader(text.as_bytes());
    let rows: Vec<csv::StringRecord> = reader.records().collect::<Result<_, _>>().map_err(fail)?;
    let expected: Vec<&str> = shipped_header.trim().split(',').collect();
    let Some(current) = rows.first() else {
        return Ok(());
    };
    if current.iter().eq(expected.iter().copied()) {
        return Ok(());
    }

    let old: Vec<&str> = current.iter().collect();
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(&expected).map_err(fail)?;
    for row in rows.iter().skip(1) {
        let repaired: Vec<&str> = if row.len() == expected.len() {
            row.iter().collect()
        } else {
            expected
                .iter()
                .map(|column| {
                    old.iter().position(|had| had == column).and_then(|index| row.get(index)).unwrap_or("")
                })
                .collect()
        };
        writer.write_record(&repaired).map_err(fail)?;
    }
    let repaired = writer.into_inner().map_err(|e| DomainError::DataSource(e.to_string()))?;

    // Written beside and renamed: this is the user's only copy of their own
    // records, and a truncating write that fails halfway loses all of them.
    let temporary = path.with_extension("csv.repairing");
    std::fs::write(&temporary, repaired).map_err(|e| DomainError::DataSource(format!("{}: {e}", temporary.display())))?;
    std::fs::rename(&temporary, path).map_err(|e| DomainError::DataSource(format!("{}: {e}", path.display())))
}

impl App {
    pub fn layout(&self) -> DataLayout {
        DataLayout::new(&self.data_root, &self.profile)
    }

    pub fn reference_dir(&self) -> PathBuf {
        self.data_root.join("reference").join(&self.profile)
    }

    pub fn curated_dir(&self) -> PathBuf {
        self.data_root.join("curated")
    }

    /// Reference profiles available on disk, so the front-end never has to
    /// hardcode `global`/`andina_colombia`.
    pub fn profiles(&self) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(self.data_root.join("reference"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    }
}

/// For a one-shot CLI run: uncached, because a single run fetches one
/// climatology and exits.
///
/// `None` rather than an error if the HTTP client can't be built — at this
/// layer that is the API being down, and neither may stop a plan.
pub fn build_agroclimatic_repo() -> Option<Box<dyn AgroclimaticRepository>> {
    Some(Box::new(NasaPowerRepo::new().ok()?))
}

/// The same provider as a cache a front-end can share with a background
/// thread, read through [`crate::infra::PrewarmedAgroclimaticRepo`].
pub fn build_climate_cache() -> Option<Arc<CachedAgroclimaticRepo>> {
    Some(Arc::new(CachedAgroclimaticRepo::new(Box::new(NasaPowerRepo::new().ok()?))))
}

/// `agroclimatic` is `None` for an offline plan (`--no-climate`, or any
/// front-end that shouldn't block on a network call).
pub fn build_calculate_fertility_plan(
    layout: &DataLayout,
    agroclimatic: Option<Box<dyn AgroclimaticRepository>>,
) -> Result<CalculateFertilityPlan, DomainError> {
    Ok(CalculateFertilityPlan::new(
        Box::new(CsvSoilTestsRepo::new(layout.curated("soil_tests.csv"))),
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvYieldTargetsRepo::new(layout.curated("yield_targets.csv"))),
        Box::new(CsvNutrientRemovalRepo::new(layout.reference("nutrient_removal.csv"))),
        Box::new(StaticConversionFactorsRepo::from_toml_file(layout.shared_reference("conversion_factors.toml"))?),
        Box::new(YamlEfficiencyRulesRepo::from_yaml_file(layout.reference("efficiency_rules.yaml"))?),
        Box::new(TomlEfficiencyBandsRepo::from_toml_file(layout.reference("efficiency_bands.toml"))?),
        Box::new(CsvCriticalLevelsRepo::new(layout.reference("critical_levels.csv"))),
        Box::new(CsvFertilizerSourcesRepo::new(layout.reference("fertilizer_sources.csv"))),
        Box::new(StaticLimingRulesRepo::from_toml_file(layout.reference("liming_rules.toml"))?),
        Box::new(CsvLimingMaterialsRepo::new(layout.reference("liming_materials.csv"))),
        agroclimatic,
    ))
}

/// The product half of a plan. Two repositories only: the catalog to pick
/// from, and the conversion table that moves elemental P and K onto the
/// commercial P2O5/K2O basis the grades are stated in.
pub fn build_recommend_fertilizer_program(layout: &DataLayout) -> Result<RecommendFertilizerProgram, DomainError> {
    Ok(RecommendFertilizerProgram::new(
        Box::new(CsvFertilizerSourcesRepo::new(layout.reference("fertilizer_sources.csv"))),
        Box::new(StaticConversionFactorsRepo::from_toml_file(layout.shared_reference("conversion_factors.toml"))?),
    ))
}

/// The source catalog on its own, for callers that only want to report on
/// it rather than plan with it.
pub fn build_fertilizer_sources(layout: &DataLayout) -> CsvFertilizerSourcesRepo {
    CsvFertilizerSourcesRepo::new(layout.reference("fertilizer_sources.csv"))
}

/// The curated lots on their own, for a front-end that needs a whole
/// [`crate::core::domain::FieldContext`] rather than the summary
/// `ListLots` gives — prefilling an edit form is the case.
pub fn build_field_contexts(layout: &DataLayout) -> CsvFieldContextRepo {
    CsvFieldContextRepo::new(layout.curated("field_context.csv"))
}

/// Reads the curated lots, not the planning rows: a lot exists whether or
/// not a crop is planned on it.
pub fn build_list_lots(layout: &DataLayout) -> ListLots {
    ListLots::new(
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvYieldTargetsRepo::new(layout.curated("yield_targets.csv"))),
    )
}

/// The only use case wired to a writer. Curated data is
/// profile-independent, so the files are the same under any profile.
pub fn build_register_lot(layout: &DataLayout) -> RegisterLot {
    RegisterLot::new(
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvCuratedWriter::new(
            layout.curated("field_context.csv"),
            layout.curated("soil_tests.csv"),
            layout.curated("yield_targets.csv"),
        )),
    )
}

pub fn build_list_supported_crops(layout: &DataLayout) -> ListSupportedCrops {
    ListSupportedCrops::new(Box::new(CsvCropCatalogRepo::new(layout.shared_reference("crops.csv"))))
}

pub fn build_inspect_scenario(layout: &DataLayout) -> Result<InspectScenario, DomainError> {
    Ok(InspectScenario::new(
        Box::new(CsvSoilTestsRepo::new(layout.curated("soil_tests.csv"))),
        Box::new(CsvFieldContextRepo::new(layout.curated("field_context.csv"))),
        Box::new(CsvYieldTargetsRepo::new(layout.curated("yield_targets.csv"))),
        Box::new(CsvNutrientRemovalRepo::new(layout.reference("nutrient_removal.csv"))),
        Box::new(YamlEfficiencyRulesRepo::from_yaml_file(layout.reference("efficiency_rules.yaml"))?),
        Box::new(CsvCriticalLevelsRepo::new(layout.reference("critical_levels.csv"))),
        Box::new(StaticConversionFactorsRepo::from_toml_file(layout.shared_reference("conversion_factors.toml"))?),
        Box::new(CsvSoilQualityThresholdsRepo::new(layout.reference("soil_quality_thresholds.csv"))),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ports::FieldContextRepository;

    fn sandbox(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nns_seed_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn lines(root: &Path, relative: &str) -> usize {
        std::fs::read_to_string(root.join(relative)).expect("seeded file").lines().count()
    }

    /// The two seeds differ in exactly one place: whether the user's own
    /// records start with the repository's demonstration lots in them.
    #[test]
    fn only_the_test_catalog_arrives_with_lots_in_it() {
        let own = sandbox("own");
        let demo = sandbox("demo");

        ensure_data_root(&own, CuratedSeed::HeadersOnly).expect("seed");
        ensure_data_root(&demo, CuratedSeed::WithExamples).expect("seed");

        assert_eq!(lines(&own, "curated/field_context.csv"), 1, "a fresh install owns no lots");
        assert!(lines(&demo, "curated/field_context.csv") > 1, "--test has to have something to plan");
        // Reference data is the same either way: it is the literature.
        for (relative, _) in shipped_data::REFERENCE {
            assert_eq!(lines(&own, relative), lines(&demo, relative), "{relative} differs between seeds");
        }

        let _ = std::fs::remove_dir_all(&own);
        let _ = std::fs::remove_dir_all(&demo);
    }

    /// The bug this shipped with: a catalog seeded before `altitude_m`
    /// existed kept a 13-column header while the writer appended 14-field
    /// rows, and `csv` then refused the file outright — so the TUI could
    /// not list a single lot.
    #[test]
    fn a_catalog_whose_header_predates_a_column_is_repaired_on_start() {
        let root = sandbox("stale_header");
        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("seed");
        let path = root.join("curated/field_context.csv");

        // Exactly what was found on disk: the pre-`altitude_m` header, and
        // two lots the app registered afterwards at the new width.
        std::fs::write(
            &path,
            "field_id,sample_id,texture,irrigation_system,organic_matter_percent,ph,cec,bulk_density_kg_dm3,\
             arable_depth_m,region,latitude,longitude,coordinates_note\n\
             Test,Test,clay,rainfed,2.5,4.2,5.9,1.3,0.15,andina_colombia,,,,\n",
        )
        .expect("stale file");

        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("re-seed repairs");

        let repo = CsvFieldContextRepo::new(&path);
        let lots = repo.list_contexts().expect("the file has to be readable again");
        assert_eq!(lots.len(), 1);
        assert_eq!(lots[0].field_id, "Test");
        assert_eq!(lots[0].ph, 4.2, "a row already at the new width keeps its columns");
        assert_eq!(lots[0].altitude_m, None);
        // Idempotent: a second start must not rewrite an already-good file.
        let after = std::fs::read_to_string(&path).expect("read");
        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("third start");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), after);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The other direction: rows written *before* the column was added are
    /// one field short, and have to land in the right columns by name.
    #[test]
    fn rows_narrower_than_the_new_header_are_matched_by_column_name() {
        let root = sandbox("narrow_rows");
        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("seed");
        let path = root.join("curated/field_context.csv");

        std::fs::write(
            &path,
            "field_id,sample_id,texture,irrigation_system,organic_matter_percent,ph,cec,bulk_density_kg_dm3,\
             arable_depth_m,region,latitude,longitude,coordinates_note\n\
             LOT-9,LOT-9,loam,drip,3.0,6.1,11.0,1.25,0.2,global,1.5,-77.0,from a survey\n",
        )
        .expect("stale file");

        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("re-seed repairs");

        let lots = CsvFieldContextRepo::new(&path).list_contexts().expect("readable");
        assert_eq!(lots.len(), 1);
        // `coordinates_note` must not have been read as `altitude_m`.
        assert_eq!(lots[0].altitude_m, None);
        assert_eq!(lots[0].longitude, Some(-77.0));
        assert_eq!(lots[0].ph, 6.1);
        assert!(std::fs::read_to_string(&path).expect("read").contains("from a survey"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The rule the whole scheme rests on: the user's own records must
    /// survive every later run.
    #[test]
    fn seeding_twice_never_overwrites_the_user_records() {
        let root = sandbox("edit");
        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("seed");

        let mine = root.join("curated/soil_tests.csv");
        let row = std::fs::read_to_string(&mine).expect("read") + "S-1,N,12,mg_kg,kjeldahl,0,20\n";
        std::fs::write(&mine, &row).expect("edit");
        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("re-seed");

        assert_eq!(std::fs::read_to_string(&mine).expect("read"), row);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The bug a whole release lived with: a reference table seeded before
    /// a column existed was kept forever, and every plan then died on
    /// `missing field harvested_organ`. The literature is the release's.
    #[test]
    fn a_stale_reference_table_is_replaced_and_the_old_one_kept_aside() {
        let root = sandbox("stale_reference");
        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("seed");

        let table = root.join("reference/global/nutrient_removal.csv");
        std::fs::write(&table, "crop_id,product,yield_unit\ncorn,grain,t_ha\n").expect("age it");
        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("re-seed refreshes");

        let refreshed = std::fs::read_to_string(&table).expect("read");
        assert!(refreshed.contains("harvested_organ"), "the shipped schema has to win");
        assert!(
            std::fs::read_to_string(table.with_extension("csv.bak")).expect("backup").contains("crop_id,product"),
            "and the copy it replaced is kept, not deleted"
        );

        // Idempotent: an already-current table is not touched, so no start
        // ever writes a `.bak` over the one that holds the user's edit.
        let _ = std::fs::remove_file(table.with_extension("csv.bak"));
        ensure_data_root(&root, CuratedSeed::HeadersOnly).expect("third start");
        assert!(!table.with_extension("csv.bak").exists(), "nothing to back up when nothing differs");

        let _ = std::fs::remove_dir_all(&root);
    }
}
