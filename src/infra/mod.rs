//! Adapters: concrete implementations of `core::ports`, plus the
//! composition root that wires them together. Everything that touches a
//! file path, a CLI framework or a serialization format lives here.

pub mod bootstrap;
pub mod cli_adapter;
pub mod csv_critical_levels_repo;
pub mod csv_crop_catalog_repo;
pub mod csv_fertilizer_sources_repo;
pub mod csv_field_context_repo;
pub mod csv_nutrient_removal_repo;
pub mod csv_soil_tests_repo;
pub mod csv_yield_targets_repo;
pub mod static_conversion_factors_repo;
pub mod yaml_efficiency_rules_repo;

pub use csv_critical_levels_repo::CsvCriticalLevelsRepo;
pub use csv_crop_catalog_repo::CsvCropCatalogRepo;
pub use csv_fertilizer_sources_repo::CsvFertilizerSourcesRepo;
pub use csv_field_context_repo::CsvFieldContextRepo;
pub use csv_nutrient_removal_repo::CsvNutrientRemovalRepo;
pub use csv_soil_tests_repo::CsvSoilTestsRepo;
pub use csv_yield_targets_repo::CsvYieldTargetsRepo;
pub use static_conversion_factors_repo::StaticConversionFactorsRepo;
pub use yaml_efficiency_rules_repo::YamlEfficiencyRulesRepo;
