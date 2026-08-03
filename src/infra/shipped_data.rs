//! The data catalog, carried inside the binary.
//!
//! `cargo install` copies binaries and nothing else, so a program that
//! reads its reference tables off disk has to put them there itself. Same
//! device the two language files already use (`tui_adapter::i18n`), just
//! applied to the rest of `data/`.
//!
//! Written out on first run rather than read from memory: reading the
//! embedded copy directly would mean every CSV, TOML and YAML adapter
//! grows a from-string constructor beside the from-path one it already
//! has. Writing the files instead leaves all of them untouched — and puts
//! the reference tables somewhere the user can open, which is what this
//! project treats them as.

/// Path relative to the data root, and the file's contents.
type ShippedFile = (&'static str, &'static str);

/// The literature catalog. Seeded whole.
pub const REFERENCE: [ShippedFile; 18] = [
    ("reference/global/soil_quality_thresholds.csv", include_str!("../../data/reference/global/soil_quality_thresholds.csv")),
    ("reference/andina_colombia/soil_quality_thresholds.csv", include_str!("../../data/reference/andina_colombia/soil_quality_thresholds.csv")),
    ("reference/global/conversion_factors.toml", include_str!("../../data/reference/global/conversion_factors.toml")),
    ("reference/global/critical_levels.csv", include_str!("../../data/reference/global/critical_levels.csv")),
    ("reference/global/crops.csv", include_str!("../../data/reference/global/crops.csv")),
    ("reference/global/efficiency_rules.yaml", include_str!("../../data/reference/global/efficiency_rules.yaml")),
    ("reference/global/efficiency_bands.toml", include_str!("../../data/reference/global/efficiency_bands.toml")),
    ("reference/andina_colombia/efficiency_bands.toml", include_str!("../../data/reference/andina_colombia/efficiency_bands.toml")),
    ("reference/global/fertilizer_sources.csv", include_str!("../../data/reference/global/fertilizer_sources.csv")),
    ("reference/global/liming_materials.csv", include_str!("../../data/reference/global/liming_materials.csv")),
    ("reference/global/liming_rules.toml", include_str!("../../data/reference/global/liming_rules.toml")),
    ("reference/global/nutrient_removal.csv", include_str!("../../data/reference/global/nutrient_removal.csv")),
    ("reference/andina_colombia/critical_levels.csv", include_str!("../../data/reference/andina_colombia/critical_levels.csv")),
    ("reference/andina_colombia/efficiency_rules.yaml", include_str!("../../data/reference/andina_colombia/efficiency_rules.yaml")),
    ("reference/andina_colombia/fertilizer_sources.csv", include_str!("../../data/reference/andina_colombia/fertilizer_sources.csv")),
    ("reference/andina_colombia/liming_materials.csv", include_str!("../../data/reference/andina_colombia/liming_materials.csv")),
    ("reference/andina_colombia/liming_rules.toml", include_str!("../../data/reference/andina_colombia/liming_rules.toml")),
    ("reference/andina_colombia/nutrient_removal.csv", include_str!("../../data/reference/andina_colombia/nutrient_removal.csv")),
];

/// Three lots that exercise three different halves of the engine, shipped
/// as **importable files** rather than as curated rows: they land in
/// `examples/` where the TUI's file picker finds them, and nothing enters
/// anyone's records until they choose to import them.
///
/// Each id names the condition it demonstrates, not a place or a crop —
/// that is the only reason each of them is here:
///
/// - `EX-ACID` — an acid ash-derived coffee lot (pH 4.9, Al 2.6 cmolc/kg).
///   Reaches liming and the Andean profile's phosphorus overrides.
/// - `EX-SANDY` — a sandy irrigated horticulture lot with low CEC. Reaches
///   the coarse-texture leaching modifiers for N, K and S.
/// - `EX-CALCAREOUS` — a calcareous lot at pH 7.9. Reaches the high-pH bands
///   for ammonia volatilization and Ca-phosphate fixation.
///
/// Their coordinates are illustrative, like the curated lots' own: they
/// place each example in a region whose reference tables answer for it,
/// and they are not surveyed positions. The soil figures are typical of
/// the condition each one is named for, not readings from a lab report.
pub const EXAMPLES: [ShippedFile; 3] = [
    ("examples/lots.csv", include_str!("../../data/examples/lots.csv")),
    ("examples/soil_tests.csv", include_str!("../../data/examples/soil_tests.csv")),
    ("examples/yield_targets.csv", include_str!("../../data/examples/yield_targets.csv")),
];

/// The user's own records. Seeded with the column header alone — the two
/// demonstration lots are the repository's, not anybody's soil.
///
/// The header is taken from the shipped file's first line instead of
/// being written out again here, so a column added to one is never
/// forgotten in the other.
pub const CURATED: [ShippedFile; 3] = [
    ("curated/field_context.csv", include_str!("../../data/curated/field_context.csv")),
    ("curated/soil_tests.csv", include_str!("../../data/curated/soil_tests.csv")),
    ("curated/yield_targets.csv", include_str!("../../data/curated/yield_targets.csv")),
];

/// What [`CURATED`] actually seeds: the header row, newline included.
pub fn header_of(contents: &str) -> String {
    let header = contents.lines().next().unwrap_or_default();
    format!("{header}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A header that came back empty would seed a file `csv` can't parse.
    #[test]
    fn every_curated_file_ships_a_usable_header() {
        for (path, contents) in CURATED {
            let header = header_of(contents);
            assert!(header.contains(','), "{path} header has no columns: {header:?}");
            assert!(header.ends_with('\n'), "{path} header is not a line: {header:?}");
        }
    }

    /// `include_str!` proves the files exist at compile time; this proves
    /// none of them is empty and no path is listed twice.
    #[test]
    fn every_shipped_file_has_content_and_a_distinct_path() {
        let mut paths: Vec<&str> = REFERENCE.iter().chain(&CURATED).map(|(path, _)| *path).collect();
        let listed = paths.len();
        paths.sort_unstable();
        paths.dedup();

        assert_eq!(paths.len(), listed, "a path is listed twice");
        for (path, contents) in REFERENCE.iter().chain(&CURATED) {
            assert!(!contents.trim().is_empty(), "{path} ships empty");
        }
    }
}
