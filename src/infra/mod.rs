//! Implementations of `core::ports`, plus the composition root that wires
//! them together. Everything touching a path, a CLI or a wire format.

pub mod agroclimatic_adapter;
pub mod bootstrap;
pub mod cli_adapter;
pub mod csv_critical_levels_repo;
pub mod csv_crop_catalog_repo;
pub mod csv_curated_writer;
pub mod csv_fertilizer_sources_repo;
pub mod csv_field_context_repo;
pub mod csv_import;
pub mod csv_liming_materials_repo;
pub mod csv_nutrient_removal_repo;
pub mod csv_soil_quality_thresholds_repo;
pub mod csv_soil_tests_repo;
pub mod csv_yield_targets_repo;
pub mod pdf_report_exporter;
pub mod report_export;
pub mod report_renderer;
pub mod shipped_data;
pub mod static_conversion_factors_repo;
pub mod static_liming_rules_repo;
pub mod toml_efficiency_bands_repo;
pub mod tui_adapter;
pub mod tui_settings;
pub mod yaml_efficiency_rules_repo;

pub use agroclimatic_adapter::{CachedAgroclimaticRepo, NasaPowerRepo, PrewarmedAgroclimaticRepo};
pub use csv_critical_levels_repo::CsvCriticalLevelsRepo;
pub use csv_crop_catalog_repo::CsvCropCatalogRepo;
pub use csv_curated_writer::CsvCuratedWriter;
pub use csv_fertilizer_sources_repo::CsvFertilizerSourcesRepo;
pub use csv_field_context_repo::CsvFieldContextRepo;
pub use csv_liming_materials_repo::CsvLimingMaterialsRepo;
pub use csv_nutrient_removal_repo::CsvNutrientRemovalRepo;
pub use csv_soil_quality_thresholds_repo::CsvSoilQualityThresholdsRepo;
pub use csv_soil_tests_repo::CsvSoilTestsRepo;
pub use csv_yield_targets_repo::CsvYieldTargetsRepo;
pub use pdf_report_exporter::PdfReportExporter;
pub use static_conversion_factors_repo::StaticConversionFactorsRepo;
pub use static_liming_rules_repo::StaticLimingRulesRepo;
pub use toml_efficiency_bands_repo::TomlEfficiencyBandsRepo;
pub use yaml_efficiency_rules_repo::YamlEfficiencyRulesRepo;
