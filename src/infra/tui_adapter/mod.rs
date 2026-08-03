//! Terminal front-end: same ports and use cases as `cli_adapter`, only the
//! presentation differs. Keyboard-first tiling workspace.
//!
//! Boundary: every operation goes through an input port and every path
//! comes from `bootstrap`. Domain types are imported only where naming a
//! port's return value requires it — no service or agronomic rule.

pub mod i18n;
pub mod theme;
mod ui;

use std::cell::Cell;
use std::collections::HashSet;
use std::sync::Arc;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::core::application::{
    FertilityScenario, FormulationRequest, LotRegistration, LotSummary, ScenarioInspection, SoilTestEntry,
};
use crate::core::domain::{
    BlendSearchStrategy, Crop, DomainError, FertilityPlan, FertilizerRecommendationReport, FertilizerSource,
    IrrigationSystem, Nutrient, NutrientDemandMode, SoilTest, Texture, YieldTarget,
};
use crate::core::ports::{
    AgroclimaticRepository, FertilityCalculatorPort, FertilizerSourceRepository, FieldContextRepository,
    InspectScenarioPort, ListCropsPort, ListLotsPort, RecommendFertilizerProgramPort, RegisterLotPort,
    SoilTestRepository,
};
use crate::infra::bootstrap::{self, App as Composition};
use crate::infra::{csv_import, tui_settings, CachedAgroclimaticRepo, PrewarmedAgroclimaticRepo};

use i18n::{I18n, Language};
use theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    /// Stage ①: the soil analysis behind the scenario.
    Soil,
    /// Stage ②: the crop catalog.
    Crops,
    /// Stage ③: the yield goal and the demand basis.
    Target,
    /// Stage ④: the products and doses the plan settled on.
    Sources,
    /// Stage ⑤: the nutrient balance.
    Plan,
    NewLot,
    /// The same form as `NewLot`, prefilled from the selected lot and
    /// submitted through `edit_lot` instead of `register_lot`.
    EditLot,
    NewSample,
    /// The whole lab panel as one editable table: a row per nutrient, any
    /// number of them filled, written in a single pass.
    SampleBatch,
    /// Bulk entry: point at a CSV, get every row validated the way a typed
    /// one is.
    Import,
    Settings,
}

/// The fertilization module is a **directed flow**, not five screens: the
/// stepper ribbon paints these in order and `h`/`l` walk them. Stage ④ and
/// ⑤ read the two halves of one [`FertilityPlan`] — the doses and the
/// balance — which is why entering either calculates.
pub const STAGES: [(Screen, &str); 5] = [
    (Screen::Soil, "stage_soil"),
    (Screen::Crops, "stage_crop"),
    (Screen::Target, "stage_target"),
    (Screen::Sources, "stage_sources"),
    (Screen::Plan, "stage_plan"),
];

pub fn stage_index(screen: Screen) -> Option<usize> {
    STAGES.iter().position(|(stage, _)| *stage == screen)
}

/// Which module a screen belongs to. Every stage belongs to the
/// fertilization module, whose menu row targets the first of them — so the
/// module column stays lit on stage ⑤ as much as on stage ①.
pub fn module_screen(screen: Screen) -> Screen {
    match stage_index(screen) {
        Some(_) => STAGES[0].0,
        None => screen,
    }
}

/// What the left column offers *for the lot under the cursor*, plus the
/// two that are about the app rather than a lot.
///
/// The column used to be a module menu — Home, Fertilization, New lot… —
/// which is a list of places rather than a list of things. Lots are the
/// things this app is about, so they get the column, and what used to be a
/// row is now a key that acts on the selection.
///
/// Label id, key, glyph. Every one of them is reachable three ways: by its
/// key from either pane, from the launcher menu with the cursor on it, and
/// — for the first — with `Enter` on the lot list.
pub const LOT_ACTIONS: [(&str, char, &str); 10] = [
    ("action_plan", '⏎', "◈"),
    ("action_new", 'n', "+"),
    ("action_copy", 'c', "⧉"),
    ("action_edit", 'e', "✎"),
    ("action_sample", 'a', "⌕"),
    ("action_batch", 'b', "▤"),
    ("action_delete", 'x', "␡"),
    ("action_import", 'i', "⇥"),
    ("action_settings", ',', "⚙"),
    ("action_quit", 'q', "⏻"),
];

/// What one `h`/`l` press moves the yield goal by, in the goal's own unit.
const YIELD_STEP: f64 = 0.1;

/// Stage ③ has two things to set — the goal and the demand basis — and
/// both are picked the way everything else in this app is picked: `j`/`k`
/// moves between them, `h`/`l` changes the one under the cursor.
///
/// It used to have one reachable control and one that only *looked*
/// reachable: the basis was drawn as radio buttons, which promise "move
/// here and choose", and could in fact only be changed with an `m` nobody
/// could see. `j`/`k` nudged the number instead. This is the row index that
/// closed that gap.
/// Three rows, not two: the third is the way out.
///
/// Stage ③ is the only one where `h`/`l` are taken by a control instead of
/// walking the stepper, so the habit built on stages ①, ④ and ⑤ breaks
/// exactly here — press `l` to move on and the yield goal changes instead.
/// `Enter` and `]` always advanced, but a screen that needs a hint read to
/// escape is a screen people get stuck on. So the exit is a row you can
/// land on, the same way a form's `[ Save ]` row is.
pub const TARGET_ROWS: usize = 3;
pub const TARGET_ROW_GOAL: usize = 0;
pub const TARGET_ROW_BASIS: usize = 1;
pub const TARGET_ROW_CONTINUE: usize = 2;

/// In `LotRegistration` field order; label ids map to that struct.
const NEW_LOT_FIELDS: [&str; 16] = [
    "form_field_id",
    "form_texture",
    "form_irrigation",
    "form_om",
    "form_ph",
    "form_cec",
    "form_bulk_density",
    "form_arable_depth",
    "form_region",
    "form_latitude",
    "form_longitude",
    "form_altitude",
    "form_area",
    "form_crop",
    "form_yield_value",
    "form_yield_unit",
];

/// One field: which file. Everything else about an import is read off the
/// file's own header.
const IMPORT_FIELDS: [&str; 1] = ["form_import_file"];

/// Offered at the end of a list that cannot be closed: the file browser,
/// where the right file may be anywhere on disk, and the lab methods,
/// where the right extraction may be one nobody here has seen. Picking it
/// turns the field into a text box.
///
/// A NUL is what makes it a sentinel and not a value: no path and no
/// method can collide with it.
const TYPE_IT_IN: &str = "\u{0}type-it-in";

/// Where the browser opens: the shipped examples if they are there, then
/// the data root, then home. Somewhere with something in it beats an empty
/// directory that is technically correct.
fn import_start_dir(data_root: &std::path::Path) -> std::path::PathBuf {
    [data_root.join("examples"), data_root.to_path_buf()]
        .into_iter()
        .find(|dir| dir.is_dir())
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

/// One directory as picker rows: the parent, the subdirectories, the CSVs.
///
/// Directories are offered because a picker that cannot leave the folder it
/// opened in is not a file picker, it is a shortlist. A row's *label* is the
/// basename — the whole point of the browser — while its *value* is the
/// absolute path the field receives.
fn browse_dir(dir: &std::path::Path) -> (Vec<String>, Vec<String>) {
    let mut values = Vec::new();
    let mut labels = Vec::new();

    if let Some(parent) = dir.parent() {
        values.push(parent.display().to_string());
        labels.push("../".to_string());
    }

    let mut dirs: Vec<(String, String)> = Vec::new();
    let mut files: Vec<(String, String)> = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        // Hidden entries are noise in a folder anyone would pick a lab
        // report out of.
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            dirs.push((path.display().to_string(), format!("{name}/")));
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv")) {
            files.push((path.display().to_string(), name));
        }
    }
    dirs.sort_by(|a, b| a.1.cmp(&b.1));
    files.sort_by(|a, b| a.1.cmp(&b.1));

    for (value, label) in dirs.into_iter().chain(files) {
        values.push(value);
        labels.push(label);
    }
    (values, labels)
}

/// One lab result for an existing lot.
const NEW_SAMPLE_FIELDS: [&str; 7] = [
    "form_field_id",
    "form_nutrient",
    "form_value",
    "form_unit",
    "form_method",
    "form_depth_from",
    "form_depth_to",
];

/// The whole lab panel at once: one row per nutrient a lab can report, one
/// column per thing a reading needs beyond the nutrient naming itself.
///
/// `form_method` is a column and not a panel-wide field because the domain
/// requires a method on every reading *and* it changes between nutrients —
/// Bray for P, ammonium acetate for K — so one value for the table would be
/// a claim about provenance nobody made. `depth_from` is not a column: the
/// single-reading form has always defaulted it to 0, and a lab panel is one
/// layer.
const BATCH_COLUMNS: [&str; 5] = ["form_nutrient", "form_value", "form_unit", "form_method", "form_depth_to"];

/// Column 0 is the nutrient, which is what a row *is* — it names the row
/// and is never edited, so the cursor starts on the first cell that is.
const BATCH_FIRST_CELL: usize = 1;
const BATCH_COL_VALUE: usize = 1;
const BATCH_COL_UNIT: usize = 2;
const BATCH_COL_METHOD: usize = 3;

/// Where a fresh row starts: the unit most lab reports use and the depth
/// the single-reading form has always suggested. Both are one keypress to
/// change and neither is written unless the row carries a value.
const BATCH_DEFAULT_DEPTH_CM: &str = "20";

/// A whole lab panel as an editable table, held apart from [`Form`]
/// because a form is a list of fields and this is a grid of cells: the same
/// four columns repeated down every nutrient.
pub struct SampleBatch {
    /// Fixed at open time from the selection, so the table cannot drift
    /// onto another lot half way through being filled.
    field_id: String,
    /// One row per nutrient, in [`BATCH_COLUMNS`] order.
    rows: Vec<[String; BATCH_COLUMNS.len()]>,
    row: usize,
    col: usize,
    editing: bool,
}

impl SampleBatch {
    /// `existing` prefills a row from the lot's own last reading, so
    /// reopening the panel shows what is already on file instead of asking
    /// the cursor to walk a list that starts nowhere near it.
    ///
    /// Only a reading at the surface (`from_cm == 0`) is shown: that is the
    /// only depth this table can write back, since `depth_from` is not one
    /// of its columns. A deeper layer is left for the single-reading form.
    fn new(field_id: String, existing: &[SoilTest]) -> Self {
        let rows = soil_test_nutrients()
            .into_iter()
            .map(|nutrient| match existing.iter().find(|t| t.layer.from_cm == 0.0 && t.nutrient.to_string() == nutrient) {
                Some(test) => {
                    [nutrient, test.value.to_string(), test.unit.clone(), test.method.clone(), test.layer.to_cm.to_string()]
                }
                None => {
                    let unit = customary_unit(&nutrient).to_string();
                    [nutrient, String::new(), unit, String::new(), BATCH_DEFAULT_DEPTH_CM.to_string()]
                }
            })
            .collect();
        Self { field_id, rows, row: 0, col: BATCH_FIRST_CELL, editing: false }
    }

    /// Only the rows somebody actually typed a number into. An untouched
    /// row is not an error and not a zero reading — it is a nutrient this
    /// panel did not report.
    fn entries(&self) -> Vec<SoilTestEntry> {
        self.rows
            .iter()
            .filter(|row| row[BATCH_COL_VALUE].trim().parse::<f64>().is_ok())
            .map(|row| SoilTestEntry {
                nutrient_id: row[0].clone(),
                value: row[1].clone(),
                unit: row[2].clone(),
                method: row[3].clone(),
                depth_from_cm: "0".to_string(),
                depth_to_cm: row[4].clone(),
            })
            .collect()
    }

    fn cell(&mut self) -> Option<&mut String> {
        self.rows.get_mut(self.row)?.get_mut(self.col)
    }

    /// The two cells with a list behind them cycle it, rather than opening
    /// an overlay over a table the overlay would cover.
    ///
    /// An empty method cell lands on the extraction that nutrient is
    /// customarily read with — the first `l` offers it, which is one press
    /// for the common case and still an act rather than a default.
    fn cycle(&mut self, delta: isize) {
        let Some(row) = self.rows.get(self.row) else { return };
        // The method list is this row's nutrient's own; the first entry is
        // what that nutrient is customarily read with, which is what an
        // empty cell lands on.
        let options: &[&str] = match self.col {
            BATCH_COL_UNIT => &SOIL_TEST_UNITS,
            BATCH_COL_METHOD => methods_for(&row[0]),
            _ => return,
        };
        let next = match options.iter().position(|option| *option == row[self.col]) {
            Some(current) => options[(current + if delta < 0 { options.len() - 1 } else { 1 }) % options.len()],
            None => options[0],
        }
        .to_string();
        if let Some(cell) = self.cell() {
            *cell = next;
        }
    }


    /// Wraps within the editable columns, never onto the nutrient.
    fn move_cell(&mut self, delta: isize) {
        let last = BATCH_COLUMNS.len() - 1;
        self.col = match delta < 0 {
            true if self.col <= BATCH_FIRST_CELL => last,
            true => self.col - 1,
            false if self.col >= last => BATCH_FIRST_CELL,
            false => self.col + 1,
        };
    }

    /// The one cell `Enter` commits the table from.
    fn on_last_cell(&self) -> bool {
        self.row + 1 == self.rows.len() && self.col + 1 == BATCH_COLUMNS.len()
    }
}

const SETTINGS: [&str; 8] = [
    "settings_language",
    "settings_theme",
    "settings_profile",
    "settings_strategy",
    "settings_bag",
    "settings_data_root",
    "settings_reference_dir",
    "settings_curated_dir",
];

/// The bag weights the local trade actually sells. `h`/`l` cycles them;
/// ponytail: a free-entry field when somebody buys in something else.
const BAG_WEIGHTS_KG: [f64; 3] = [25.0, 40.0, 50.0];

/// Every shipped `nutrient_removal.csv` row is `t_ha` and the repository
/// rejects a mismatch, so this is a constant, not another field to fill.
/// ponytail: promote to a picker if a profile ships another unit.
const YIELD_UNIT: &str = "t_ha";

/// `mg_per_kg` is consumed directly; anything else needs a conversion in
/// `conversion_factors.toml`, and `cmolc_per_kg` is the only other one the
/// curated data and the liming math use.
/// ponytail: extend when a lab report arrives in something else.
const SOIL_TEST_UNITS: [&str; 2] = ["mg_per_kg", "cmolc_per_kg"];

/// The region every shipped reference row answers to, whatever profile is
/// active. Mirrors the `"any"` sentinel the reference adapters look for.
const REGION_ANY: &str = "any";

/// The extractions a lab reports **this nutrient** with, first one first.
/// Ids short enough to live in a CSV cell, taken from a Colombian NTC
/// panel and from the methods the shipped reference tables classify
/// against.
///
/// Per nutrient and not one flat list, because a flat list offers Olsen
/// for calcium: a phosphorus extraction on a cation is not a choice, it is
/// a wrong answer waiting in a menu. The first entry is what a panel
/// customarily uses — offered, never filled in, since a method is a fact
/// about somebody's lab report and not something this app may assume.
///
/// Still a **list, not a closed set**: the last row hands the field back
/// for typing, because a lab is free to report an extraction nobody here
/// has seen. Nothing breaks when it does — `same_method` compares
/// alphanumerics only, and a method no critical level names falls back to
/// the row that names none.
fn methods_for(nutrient: &str) -> &'static [&'static str] {
    match nutrient {
        "P" => &["Bray_II", "Olsen", "Mehlich"],        // NTC 5350
        "K" | "Ca" | "Mg" => &["AcONH4_1N_pH7"],        // NTC 5268 / 5349
        "Al" | "H" => &["KCl_1N"],                      // NTC 5263
        "S" => &["turbidimetric", "Ca_H2PO4_2_0.008M"], // NTC 5402
        "B" => &["hot_water"],                          // NTC 5404
        // No nutrient picked yet: no list to narrow, and offering the
        // micronutrient one would be a guess with a menu around it.
        "" => &[],
        _ => &["DTPA", "Mehlich"],                      // Fe, Mn, Cu, Zn · NTC 5526
    }
}

/// The unit a lab reports a nutrient in.
///
/// Everything read off the exchange complex arrives as a charge — that is
/// what `cmolcarga/kg` on a lab report means, and what the liming formulas
/// and the cation critical levels are both stated in. The bases are the
/// reason for this; Al and H come with them because they are read off the
/// same complex, in the same column of the same report, and
/// `conversion_factors.toml` carries a factor for exactly these five.
///
/// A suggestion, not a constraint: `h`/`l` switches it, and a lab that
/// reports a cation in mg/kg is converted rather than refused.
fn customary_unit(nutrient: &str) -> &'static str {
    match nutrient {
        "K" | "Ca" | "Mg" | "Al" | "H" => "cmolc_per_kg",
        _ => "mg_per_kg",
    }
}

/// That nutrient's methods plus the row that starts typing.
fn lab_method_options(nutrient: &str) -> Vec<String> {
    methods_for(nutrient).iter().map(|m| m.to_string()).chain([TYPE_IT_IN.to_string()]).collect()
}

/// Every nutrient except N: N availability is derived from organic matter,
/// so offering it would accept a value the plan then ignores.
fn soil_test_nutrients() -> Vec<String> {
    Nutrient::ALL
        .iter()
        .filter(|nutrient| **nutrient != Nutrient::N)
        .map(Nutrient::to_string)
        .collect()
}

/// Empty `options` means free text, reserved for the three values no
/// closed list can express: an id being invented, a lab reading, a
/// coordinate. Everything that must match an enum, a catalog entry or a
/// reference key is picked, so a plan can't fail on a preventable typo.
pub struct FormField {
    label: &'static str,
    value: String,
    options: Vec<String>,
}

impl FormField {
    fn is_choice(&self) -> bool {
        !self.options.is_empty()
    }
}

/// Labelled fields plus a trailing "save" row. Values stay raw text all
/// the way to `RegisterLot`, the only thing allowed to judge them — a
/// picked value is validated exactly like a typed one.
pub struct Form {
    screen: Screen,
    fields: Vec<FormField>,
    idx: usize,
    editing: bool,
}

impl Form {
    fn new(
        screen: Screen,
        labels: &[&'static str],
        prefill: &[(&str, String)],
        options: &[(&str, Vec<String>)],
    ) -> Self {
        let fields = labels
            .iter()
            .map(|label| FormField {
                label,
                value: prefill
                    .iter()
                    .find(|(id, _)| id == label)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default(),
                options: options
                    .iter()
                    .find(|(id, _)| id == label)
                    .map(|(_, values)| values.clone())
                    .unwrap_or_default(),
            })
            .collect();
        Self { screen, fields, idx: 0, editing: false }
    }

    /// One past the last field: the row that submits.
    fn save_row(&self) -> usize {
        self.fields.len()
    }

    fn set(&mut self, label: &str, value: String) {
        if let Some(field) = self.fields.iter_mut().find(|field| field.label == label) {
            field.value = value;
        }
    }

    /// Replaces a field's list, dropping a value the new one does not
    /// carry: the methods depend on the nutrient, and a method left over
    /// from the last one is an answer to a question nobody asked.
    fn set_options(&mut self, label: &str, options: Vec<String>) {
        if let Some(field) = self.fields.iter_mut().find(|field| field.label == label) {
            if !options.contains(&field.value) {
                field.value.clear();
            }
            field.options = options;
        }
    }

    fn value(&self, label: &str) -> String {
        self.fields
            .iter()
            .find(|field| field.label == label)
            .map(|field| field.value.clone())
            .unwrap_or_default()
    }

    fn registration(&self) -> LotRegistration {
        LotRegistration {
            field_id: self.value("form_field_id"),
            texture: self.value("form_texture"),
            irrigation_system: self.value("form_irrigation"),
            organic_matter_percent: self.value("form_om"),
            ph: self.value("form_ph"),
            cec_cmolc_kg: self.value("form_cec"),
            bulk_density_kg_dm3: self.value("form_bulk_density"),
            arable_depth_m: self.value("form_arable_depth"),
            region: self.value("form_region"),
            latitude: self.value("form_latitude"),
            longitude: self.value("form_longitude"),
            altitude_m: self.value("form_altitude"),
            area_ha: self.value("form_area"),
            crop_id: self.value("form_crop"),
            yield_value: self.value("form_yield_value"),
            yield_unit: self.value("form_yield_unit"),
        }
    }

    fn soil_test_entry(&self) -> SoilTestEntry {
        SoilTestEntry {
            nutrient_id: self.value("form_nutrient"),
            value: self.value("form_value"),
            unit: self.value("form_unit"),
            method: self.value("form_method"),
            depth_from_cm: self.value("form_depth_from"),
            depth_to_cm: self.value("form_depth_to"),
        }
    }
}

/// Owns a copy of the options rather than borrowing the form, so Esc
/// leaves the field untouched.
pub struct Picker {
    field_idx: usize,
    /// What gets written to the field when a row is chosen.
    options: Vec<String>,
    /// What the row shows. Separate from `options` because a value can be
    /// an absolute path and a list 40 columns wide will truncate six of
    /// them to the same prefix — which is a list of six identical rows.
    labels: Vec<String>,
    idx: usize,
    /// Shown in the overlay's frame. The browser puts the current
    /// directory here, so "which folder am I in" is never a guess.
    title: String,
}

/// What a plan is a function of. Stages ④ and ⑤ are two halves of one
/// result and both enter through [`Tui::calculate`], so walking from one to
/// the other used to spend a whole recalculation — climate lookup included
/// — to arrive at the numbers already on screen.
///
/// `climate_ready` is part of the question, not of the answer: a plan
/// computed before the prefetch landed ran on baseline constants, and
/// reusing it after the climatology arrives would pin the session to
/// numbers the app knows are superseded.
#[derive(PartialEq)]
struct PlanKey {
    lot: String,
    crop: String,
    yield_goal: Option<f64>,
    demand_mode: NutrientDemandMode,
    climate_ready: bool,
}

pub struct Tui {
    cfg: Composition,
    /// Fetched off the render loop. `None` disables climate entirely.
    climate: Option<Arc<CachedAgroclimaticRepo>>,
    /// So scrolling doesn't fire one request per keystroke.
    climate_requested: HashSet<String>,
    i18n: I18n,
    theme: &'static Theme,
    screen: Screen,
    /// Which of the two navigable panes has focus (Tab toggles). The left
    /// one is the lot list, so its cursor is `lot_idx`.
    focus_modules: bool,
    profiles: Vec<String>,
    lots: Vec<LotSummary>,
    lot_idx: usize,
    /// The launcher menu's cursor. It is a menu, with a marker on the row
    /// under it and `Enter` running that row — the treatment Home has had
    /// since the first prototype. It only moves while the workspace has
    /// focus, so `j`/`k` still walk the lots when the lots are lit.
    pub(super) menu_idx: usize,
    crops: Vec<Crop>,
    crop_idx: usize,
    /// Fertilizer sources the active profile actually carries. Read on
    /// reload so the launcher reports the data on disk, not an assumption,
    /// and so stage ④ can name the grade behind a dose.
    sources: Vec<FertilizerSource>,
    /// Which basis the plan is asked for, switched on stage ③. The CLI's
    /// `--demand-mode` by another route.
    demand_mode: NutrientDemandMode,
    /// Crop picked from the catalog, overriding the lot's curated crop.
    crop_override: Option<String>,
    /// Raw text, only ever set for the selected (lot, crop) pair — both
    /// selections clear it, so it can't leak into another scenario.
    yield_input: String,
    editing_yield: bool,
    /// Which of stage ③'s two controls the cursor is on.
    target_idx: usize,
    /// Readings saved without leaving the sample form. A lab panel is one
    /// visit, not eight.
    saved_readings: usize,
    /// What the last import did, line by line. Rejections name their line
    /// number, so the pane is enough to go and fix the file.
    import_report: Vec<String>,
    filter: String,
    filtering: bool,
    /// Set by the first delete keypress, cleared by anything else. The
    /// only irreversible action in the TUI, so it takes two presses and
    /// names the lot in between.
    pending_delete: Option<String>,
    plan: Option<FertilityPlan>,
    /// What question the plan beside it answers. `None` whenever `plan` is,
    /// which is what makes "the same key" enough to reuse the result.
    plan_key: Option<PlanKey>,
    /// The product half of the same plan: which bags, how many. Computed
    /// beside the plan and kept beside it, so stage ④ never recalculates.
    recommendation: Option<FertilizerRecommendationReport>,
    /// Strategy, area and bag weight, all switched on the Settings screen.
    /// The CLI's `--strategy`/`--area-ha`/`--bag-kg` by another route.
    formulation: FormulationRequest,
    inspection: Option<ScenarioInspection>,
    /// Rebuilt each time a form opens, so a half-typed lot never survives
    /// a detour.
    form: Option<Form>,
    /// The lab panel being filled in, if the batch screen is open.
    batch: Option<SampleBatch>,
    /// The option list currently unfolded over that form, if any.
    picker: Option<Picker>,
    scroll: u16,
    /// What the last frame actually painted on the scrolling screens, and
    /// how much of it fitted. Written by `ui::draw` — the one thing it is
    /// allowed to write — because the line count is a property of the
    /// rendering, and `j` had no way to know where the page ended.
    content_height: Cell<u16>,
    viewport_height: Cell<u16>,
    setting_idx: usize,
    message: String,
    is_error: bool,
    /// A third severity, and only that: a warning is information the reader
    /// can act on, not a failure, so it must not read as one — and must not
    /// make `is_error` lie to everything that checks it.
    is_warning: bool,
    help: bool,
    running: bool,
}

pub fn run(cfg: Composition) -> Result<(), DomainError> {
    // Preferences are read here, not in `Tui::new`: a constructor that
    // reaches for the user's config file cannot be exercised by a test
    // without depending on whatever the developer happens to have stored.
    let (settings, problem) = tui_settings::load();
    let mut tui = Tui::new(cfg, settings, bootstrap::build_climate_cache());
    if let Some(problem) = problem {
        tui.message = problem;
        tui.is_error = true;
    }

    let mut terminal = ratatui::init();
    let result = tui.event_loop(&mut terminal);
    ratatui::restore();
    // On the way out, so the lot and crop the session ended on are the ones
    // it opens with. One write per session rather than one per keystroke,
    // and it catches every exit path — `q`, Esc from the launcher, Ctrl-C.
    tui.persist_settings();
    result
}

impl Tui {
    fn new(
        mut cfg: Composition,
        settings: tui_settings::TuiSettings,
        climate: Option<Arc<CachedAgroclimaticRepo>>,
    ) -> Self {
        // Applied before the first `reload()`, because the stored profile
        // decides which reference files that read opens.
        let profiles = cfg.profiles();
        if profiles.contains(&settings.profile) {
            cfg.profile = settings.profile.clone();
        }

        let mut tui = Self {
            profiles,
            cfg,
            climate,
            climate_requested: HashSet::new(),
            i18n: I18n::new(Language::from_code(&settings.language)),
            theme: theme::by_name(&settings.theme),
            screen: Screen::Dashboard,
            // Home draws no lot column, so its cursor is the launcher menu.
            focus_modules: false,
            lots: Vec::new(),
            lot_idx: 0,
            menu_idx: 0,
            crops: Vec::new(),
            crop_idx: 0,
            sources: Vec::new(),
            demand_mode: NutrientDemandMode::Extraction,
            crop_override: None,
            yield_input: String::new(),
            editing_yield: false,
            target_idx: TARGET_ROW_GOAL,
            saved_readings: 0,
            import_report: Vec::new(),
            filter: String::new(),
            filtering: false,
            pending_delete: None,
            plan: None,
            plan_key: None,
            recommendation: None,
            formulation: FormulationRequest {
                // A stored value that no longer parses is a preference, not
                // a record: it falls back rather than refusing to open.
                strategy: settings.strategy.parse().unwrap_or_default(),
                // Not a setting: on every shipped catalog both searches
                // reach the same blend, so a row for it would promise an
                // effect it cannot deliver. `--blend-search` keeps it
                // reachable for the diagnosis it is actually for.
                blend_search: BlendSearchStrategy::default(),
                // Replaced per plan from the lot being planned — see
                // `selected_lot_area`. Never restored from a preference.
                total_area_ha: 1.0,
                    bag_weight_kg: settings.bag_weight_kg,
                profile: String::new(),
            },
            inspection: None,
            form: None,
            batch: None,
            picker: None,
            scroll: 0,
            content_height: Cell::new(0),
            viewport_height: Cell::new(0),
            setting_idx: 0,
            message: String::new(),
            is_error: false,
            is_warning: false,
            help: false,
            running: true,
        };
        tui.reload();
        // After the reload, because that is what fills the lot list. A lot
        // or crop that is no longer there is simply not restored — a stale
        // preference must never select something that does not exist.
        if let Some(index) = tui.lots.iter().position(|lot| lot.field_id == settings.lot) {
            tui.lot_idx = index;
        }
        // ...and only if the restored lot carries a goal for it. The typed
        // goal is deliberately not stored — it belongs to one session's
        // scenario, not to a preference — so restoring a crop without a
        // curated goal would open the session on a pair that cannot plan.
        if !settings.crop.is_empty()
            && tui.crops.iter().any(|crop| crop.crop_id == settings.crop)
            && tui.has_curated_yield_target(&settings.crop)
        {
            tui.crop_override = Some(settings.crop.clone());
        }
        if !tui.is_error {
            tui.info("msg_ready");
        }
        tui
    }

    /// Called after every settings change. Best effort by design: failing
    /// to store a theme must not stop the TUI drawing one, so a write
    /// error lands in the status bar and nothing else happens.
    fn persist_settings(&mut self) {
        let settings = tui_settings::TuiSettings {
            language: self.i18n.language().code().to_string(),
            theme: self.theme.name.to_string(),
            profile: self.cfg.profile.clone(),
            strategy: self.formulation.strategy.to_string(),
            bag_weight_kg: self.formulation.bag_weight_kg,
            // Where the user was, so the next session opens on the lot they
            // were working rather than on whichever one sorts first.
            lot: self.selected_lot().map(|lot| lot.field_id.clone()).unwrap_or_default(),
            crop: self.active_crop().unwrap_or_default(),
        };
        if let Err(problem) = tui_settings::save(&settings) {
            self.message = problem;
            self.is_error = true;
        }
    }

    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<(), DomainError> {
        while self.running {
            terminal.draw(|frame| ui::draw(frame, self)).map_err(io_error)?;
            if let Event::Key(key) = event::read().map_err(io_error)? {
                if key.kind == KeyEventKind::Press {
                    self.on_key(key);
                }
            }
        }
        Ok(())
    }

    // ---- data ------------------------------------------------------------

    /// Everything that depends on the selected profile.
    fn reload(&mut self) {
        // Whatever failed before concerned data being re-read now; callers
        // check `is_error` afterwards for the reload's own failure.
        self.is_error = false;
        self.plan = None;
        self.plan_key = None;
        self.inspection = None;
        self.clear_crop_choice();
        self.crop_idx = 0;
        self.lot_idx = 0;
        self.scroll = 0;

        match bootstrap::build_list_supported_crops(&self.cfg.layout()).list_crops() {
            Ok(crops) => self.crops = crops,
            Err(e) => {
                self.crops.clear();
                self.fail(e);
            }
        }
        self.sources = bootstrap::build_fertilizer_sources(&self.cfg.layout())
            .list_sources()
            .unwrap_or_default();
        match bootstrap::build_list_lots(&self.cfg.layout()).list_lots() {
            Ok(lots) => self.lots = lots,
            Err(e) => {
                self.lots.clear();
                self.fail(e);
            }
        }
        self.prefetch_climate();
    }

    /// Fetches the selected lot's climatology on a background thread.
    ///
    /// Nothing waits for it: the thread fills the shared cache and exits. A
    /// plan asked for before it lands runs on baseline constants and says
    /// so, which is what keeps a 10 s timeout out of the render loop.
    fn prefetch_climate(&mut self) {
        let Some(cache) = self.climate.clone() else { return };
        let Some(lot) = self.selected_lot() else { return };
        let (Some(latitude), Some(longitude)) = (lot.latitude, lot.longitude) else { return };
        if !self.climate_requested.insert(lot.field_id.clone()) {
            return;
        }
        std::thread::spawn(move || {
            // Dropped on purpose: an unreachable provider is expected, and
            // the plan's own labelling reports it.
            let _ = cache.fetch_climatology(latitude, longitude);
        });
    }

    /// A pair: a yield goal only means something next to its crop.
    fn clear_crop_choice(&mut self) {
        self.crop_override = None;
        self.yield_input.clear();
        self.editing_yield = false;
    }

    fn selected_lot(&self) -> Option<&LotSummary> {
        self.lots.get(self.lot_idx)
    }

    /// Answered from the summaries in memory — no IO, same rows the plan
    /// would consult.
    fn has_curated_yield_target(&self, crop_id: &str) -> bool {
        self.selected_lot().and_then(|lot| lot.target_for(crop_id)).is_some()
    }

    /// The curated goal for the crop in force, if the lot carries one.
    /// Stage ③ shows it as the baseline the typed goal departs from.
    fn curated_yield_target(&self) -> Option<&YieldTarget> {
        let crop = self.active_crop()?;
        self.selected_lot()?.target_for(&crop)
    }

    /// Anything unusable counts as "not entered", so the plan falls back to
    /// the curated target and reports its own error if there is none.
    fn typed_yield_target(&self) -> Option<YieldTarget> {
        let value = self.yield_input.trim().parse::<f64>().ok()?;
        (value.is_finite() && value > 0.0).then(|| YieldTarget { value, unit: YIELD_UNIT.to_string() })
    }

    /// Picked from the catalog, or the lot's curated one. `None` for a lot
    /// registered without a planning row.
    fn active_crop(&self) -> Option<String> {
        self.crop_override
            .clone()
            .or_else(|| self.selected_lot()?.default_crop().map(str::to_string))
    }

    fn scenario(&self) -> Option<FertilityScenario> {
        let lot = self.selected_lot()?;
        let crop_id = self.active_crop()?;
        // A goal is as required as the crop. With none typed the use case
        // reads the curated one, and for most (lot, crop) pairs there is
        // none — which used to surface as the repository's own words,
        // `no yield target for field_id=… crop_id=…`, on stage ④ or ⑤.
        // The reader can only fix it on stage ③, so that is what the app
        // has to say, one screen before the failure.
        if self.typed_yield_target().is_none() && !self.has_curated_yield_target(&crop_id) {
            return None;
        }
        Some(FertilityScenario {
            sample_id: lot.field_id.clone(),
            field_id: lot.field_id.clone(),
            crop_id,
            // Extraction is the maintenance basis and the default, same as
            // the CLI; stage ③ switches it. A nutrient that needs nothing
            // on extraction is retried on absorption by the use case
            // itself, and the plan screen shows the warning.
            demand_mode: self.demand_mode,
            // `None` falls back to the curated yield target, which is what
            // the lot row on screen shows. A typed goal is what unblocks
            // the 64 catalog crops that have no curated row at all.
            yield_override: self.typed_yield_target(),
        })
    }

    /// The plan itself, with no opinion about which screen shows it:
    /// stages ④ and ⑤ read the two halves of the same result, and stage ③
    /// recalculates in place when the basis changes.
    fn calculate(&mut self) {
        let Some(scenario) = self.scenario() else {
            self.plan = None;
            self.plan_key = None;
            return self.fail_missing_scenario();
        };
        // Walking ④ → ⑤ asks the same question twice; answer it once.
        let key = PlanKey {
            lot: scenario.field_id.clone(),
            crop: scenario.crop_id.clone(),
            yield_goal: scenario.yield_override.as_ref().map(|target| target.value),
            demand_mode: scenario.demand_mode,
            climate_ready: self.climate_is_warm(),
        };
        if self.plan.is_some() && self.plan_key.as_ref() == Some(&key) {
            return self.info("msg_plan_done");
        }
        // The prefetched climatology, never the network: a miss falls back
        // to baseline constants, as `--no-climate` does on the CLI.
        let climate = self
            .climate
            .clone()
            .map(|cache| Box::new(PrewarmedAgroclimaticRepo::new(cache)) as Box<dyn AgroclimaticRepository>);
        match bootstrap::build_calculate_fertility_plan(&self.cfg.layout(), climate).and_then(|uc| uc.calculate(scenario)) {
            Ok(plan) => {
                self.recommendation = self.recommend(&plan);
                self.plan = Some(plan);
                self.plan_key = Some(key);
                self.info("msg_plan_done");
            }
            // Shown verbatim: the port's message names what was missing.
            Err(e) => {
                self.plan = None;
                self.plan_key = None;
                self.recommendation = None;
                self.fail(e);
            }
        }
    }

    /// Whether the selected lot's climatology is already in the shared
    /// cache. Reads memory only — the whole point of the prewarmed repo.
    fn climate_is_warm(&self) -> bool {
        let Some(cache) = &self.climate else { return false };
        let Some(lot) = self.selected_lot() else { return false };
        let (Some(latitude), Some(longitude)) = (lot.latitude, lot.longitude) else { return false };
        cache.cached(latitude, longitude).is_some()
    }

    /// Enter on stage ⑤ means "run it again", which a cached answer would
    /// quietly refuse. Dropping the key is what makes it a real re-run.
    fn recalculate(&mut self) {
        self.plan_key = None;
        self.calculate();
    }

    /// Best-effort, exactly like climate: a catalog that cannot answer
    /// costs the reader the product block, never the balance they asked
    /// for. Stage ④ says so where the block would have been.
    fn recommend(&self, plan: &FertilityPlan) -> Option<FertilizerRecommendationReport> {
        let request = FormulationRequest {
            profile: self.cfg.profile.clone(),
            // Read off the lot being planned, never off a preference: two
            // lots have different areas, and one stored globally would
            // compute this lot's totals from the last one's hectares. A lot
            // with no area on file is planned per hectare.
            total_area_ha: self.selected_lot_area().unwrap_or(1.0),
            ..self.formulation.clone()
        };
        bootstrap::build_recommend_fertilizer_program(&self.cfg.layout())
            .and_then(|use_case| use_case.recommend(plan, &request))
            .ok()
    }

    fn inspect(&mut self) {
        let Some(scenario) = self.scenario() else {
            self.inspection = None;
            return self.fail_missing_scenario();
        };
        match bootstrap::build_inspect_scenario(&self.cfg.layout()).and_then(|uc| uc.inspect(&scenario)) {
            Ok(inspection) => {
                self.inspection = Some(inspection);
                self.info("msg_inspect_done");
                self.warn_about_the_panel();
            }
            Err(e) => {
                self.inspection = None;
                self.fail(e);
            }
        }
    }

    /// What stage ① can see about the lab panel that stages ④ and ⑤ would
    /// otherwise be the first to mention — by which point the reader has
    /// walked past the screen where the gap could still be filled.
    ///
    /// Informative, never blocking: a panel without Al still plans, it just
    /// plans without a liming recommendation, and saying so here is saying
    /// it two stages earlier than the plan does.
    fn warn_about_the_panel(&mut self) {
        let Some(inspection) = &self.inspection else { return };
        if !inspection.soil_tests.iter().any(|test| test.nutrient == Nutrient::Al) {
            return self.warn("warn_no_al_test");
        }
        // Zero is a real reading — "below detection" — and the plan treats
        // it as one, so it is worth naming rather than worth refusing.
        let zeroed: Vec<String> = inspection
            .soil_tests
            .iter()
            .filter(|test| test.value == 0.0)
            .map(|test| test.nutrient.to_string())
            .collect();
        if !zeroed.is_empty() {
            let label = self.i18n.t("warn_zero_nutrient").to_string();
            self.message = format!("{label} {}", zeroed.join(" · "));
            self.is_error = false;
            self.is_warning = true;
        }
    }

    /// A scenario needs a lot, a crop *and* a goal; say which is missing.
    fn fail_missing_scenario(&mut self) {
        let id = if self.selected_lot().is_none() {
            "err_no_lot"
        } else if self.active_crop().is_none() {
            "err_no_crop"
        } else {
            "err_no_yield_goal"
        };
        self.fail_key(id);
    }

    // ---- curating new data -----------------------------------------------

    fn open_form(&mut self, screen: Screen) {
        self.screen = screen;
        self.focus_modules = false;
        self.picker = None;
        let form = match screen {
            // No options: the file field is free text, so an absolute path
            // can always be typed. `Enter` on it opens the browser instead
            // of the text cursor — see `activate_form_row`.
            Screen::Import => Form::new(screen, &IMPORT_FIELDS, &[], &[]),
            Screen::NewSample => Form::new(
                screen,
                &NEW_SAMPLE_FIELDS,
                &[
                    ("form_field_id", self.selected_lot().map(|lot| lot.field_id.clone()).unwrap_or_default()),
                    ("form_depth_from", "0".to_string()),
                ],
                &[
                    // `RegisterLot` refuses a lot that doesn't exist, so the
                    // lot is picked, never spelled out.
                    ("form_field_id", self.lots.iter().map(|lot| lot.field_id.clone()).collect()),
                    ("form_nutrient", soil_test_nutrients()),
                    ("form_unit", SOIL_TEST_UNITS.iter().map(|unit| unit.to_string()).collect()),
                    // Rebuilt the moment a nutrient is picked — see
                    // `commit_picker`. Until then the form has no nutrient
                    // to narrow them by.
                    ("form_method", lab_method_options("")),
                ],
            ),
            // Region prefilled with the active profile, whose reference
            // tables a plan reads. On an edit every field is prefilled from
            // the lot on disk instead, so a blank one means "cleared" and
            // not "unchanged" — the form is the whole row either way.
            _ => Form::new(
                screen,
                &NEW_LOT_FIELDS,
                &match screen {
                    Screen::EditLot => self.lot_prefill(),
                    _ => vec![
                        ("form_region", self.cfg.profile.clone()),
                        ("form_yield_unit", YIELD_UNIT.to_string()),
                    ],
                },
                &[
                    ("form_texture", Texture::ALL.iter().map(Texture::to_string).collect()),
                    ("form_irrigation", IrrigationSystem::ALL.iter().map(IrrigationSystem::to_string).collect()),
                    ("form_region", self.region_options()),
                    ("form_crop", self.crop_options()),
                    ("form_yield_unit", vec![YIELD_UNIT.to_string()]),
                ],
            ),
        };
        self.form = Some(form);
    }

    /// The planted area of the lot being planned, when it has one on file.
    fn selected_lot_area(&self) -> Option<f64> {
        let field_id = self.selected_lot().map(|lot| lot.field_id.clone())?;
        bootstrap::build_field_contexts(&self.cfg.layout())
            .get_context_by_field_id(&field_id)
            .ok()
            .and_then(|context| context.area_ha)
    }

    /// Every field of the selected lot as text, for the edit form.
    ///
    /// Empty when no lot is selected or the read fails, which leaves the
    /// form blank — and `edit_lot` then refuses it for a missing id rather
    /// than writing a half-empty row.
    fn lot_prefill(&self) -> Vec<(&'static str, String)> {
        let Some(field_id) = self.selected_lot().map(|lot| lot.field_id.clone()) else {
            return Vec::new();
        };
        let Ok(context) = bootstrap::build_field_contexts(&self.cfg.layout()).get_context_by_field_id(&field_id)
        else {
            return Vec::new();
        };
        let optional = |value: Option<f64>| value.map(|v| v.to_string()).unwrap_or_default();
        vec![
            ("form_field_id", context.field_id),
            ("form_texture", context.texture.to_string()),
            ("form_irrigation", context.irrigation_system.to_string()),
            ("form_om", context.organic_matter_percent.to_string()),
            ("form_ph", context.ph.to_string()),
            ("form_cec", context.cec_cmolc_kg.to_string()),
            ("form_bulk_density", context.bulk_density_kg_dm3.to_string()),
            ("form_arable_depth", context.arable_depth_m.to_string()),
            ("form_region", context.region),
            ("form_latitude", optional(context.latitude)),
            ("form_longitude", optional(context.longitude)),
            ("form_altitude", optional(context.altitude_m)),
            ("form_area", optional(context.area_ha)),
            // The planning row is a separate append; blank means "leave the
            // yield goal alone", which is why it is not prefilled.
            ("form_yield_unit", YIELD_UNIT.to_string()),
        ]
    }

    /// Removes the selected lot, its analyses and its planning rows.
    /// First press arms, second press commits.
    fn delete_selected_lot(&mut self) {
        let Some(field_id) = self.selected_lot().map(|lot| lot.field_id.clone()) else {
            return self.fail_key("err_no_lot");
        };
        if self.pending_delete.as_deref() != Some(field_id.as_str()) {
            self.pending_delete = Some(field_id.clone());
            self.message = format!("{} {field_id}", self.i18n.t("msg_confirm_delete"));
            self.is_error = true;
            return;
        }
        self.pending_delete = None;
        match bootstrap::build_register_lot(&self.cfg.layout()).delete_lot(&field_id) {
            Ok(rows) => {
                self.plan = None;
                self.recommendation = None;
                self.reload();
                if !self.is_error {
                    self.message = format!("{} {field_id} ({rows})", self.i18n.t("msg_lot_deleted"));
                    self.is_error = false;
                }
            }
            Err(e) => self.fail(e),
        }
    }

    /// The profiles on disk plus the `"any"` sentinel. A region no
    /// reference file knows makes `plan` fail outright.
    fn region_options(&self) -> Vec<String> {
        let mut regions = vec![REGION_ANY.to_string()];
        regions.extend(self.profiles.iter().cloned());
        regions
    }

    /// Empty first entry: a lot may be registered with no planning row.
    fn crop_options(&self) -> Vec<String> {
        std::iter::once(String::new())
            .chain(self.crops.iter().map(|crop| crop.crop_id.clone()))
            .collect()
    }

    /// Submit, unfold the option list, or start typing.
    fn activate_form_row(&mut self) {
        let Some(form) = &self.form else { return };
        if form.idx == form.save_row() {
            return self.submit_form();
        }
        let Some(field) = form.fields.get(form.idx) else { return };
        if form.screen == Screen::Import {
            // The file field is text *and* browsable: typing reaches any
            // path, Enter opens the folder the current value points at.
            let current = std::path::PathBuf::from(field.value.trim());
            let dir = if current.is_dir() {
                current
            } else {
                current.parent().filter(|p| p.is_dir()).map(|p| p.to_path_buf()).unwrap_or_else(|| {
                    import_start_dir(&self.cfg.data_root)
                })
            };
            return self.open_browser(&dir);
        }
        if field.is_choice() {
            // Opens on the field's current value, so re-opening never moves
            // the selection.
            let options = field.options.clone();
            // The sentinel is the one row whose label is not its value.
            let labels = options
                .iter()
                .map(|option| match option.as_str() {
                    TYPE_IT_IN => self.i18n.t("picker_type_value").to_string(),
                    value => value.to_string(),
                })
                .collect();
            let idx = options.iter().position(|option| *option == field.value).unwrap_or(0);
            self.picker = Some(Picker { field_idx: form.idx, options, labels, idx, title: String::new() });
        } else if let Some(form) = &mut self.form {
            form.editing = true;
        }
    }

    /// Lists one directory as picker rows.
    fn open_browser(&mut self, dir: &std::path::Path) {
        let Some(form) = &self.form else { return };
        let (mut options, mut labels) = browse_dir(dir);
        options.push(TYPE_IT_IN.to_string());
        labels.push(self.i18n.t("picker_type_path").to_string());
        self.picker = Some(Picker {
            field_idx: form.idx,
            options,
            labels,
            idx: 0,
            title: dir.display().to_string(),
        });
    }

    /// A miss means the form changed underneath the picker; the field is
    /// then left alone.
    fn commit_picker(&mut self) {
        let Some(picker) = self.picker.take() else { return };
        let Some(chosen) = picker.options.get(picker.idx).cloned() else { return };

        // A directory is a place to look, not an answer: choosing one
        // re-opens the browser there instead of putting it in the field.
        if std::path::Path::new(&chosen).is_dir() {
            self.picker = Some(picker);
            return self.open_browser(&std::path::PathBuf::from(chosen));
        }
        // The sentinel is not a value either — it is a request to stop
        // picking and start typing.
        let typing = chosen == TYPE_IT_IN;
        if let Some(form) = self.form.as_mut() {
            if let Some(field) = form.fields.get_mut(picker.field_idx) {
                field.value = if typing { String::new() } else { chosen };
            }
            form.editing = typing;
            // Picking the nutrient answers two of the rows under it. The
            // methods are that nutrient's own, so choosing Ca can never
            // offer Olsen; the unit is answered for anything read off the
            // exchange complex, but only while it is empty — after a saved
            // reading the form carries the last unit forward, and that is
            // the user's own answer, not a suggestion.
            if form.screen == Screen::NewSample
                && form.fields.get(picker.field_idx).is_some_and(|field| field.label == "form_nutrient")
            {
                let nutrient = form.value("form_nutrient");
                form.set_options("form_method", lab_method_options(&nutrient));
                if form.value("form_unit").is_empty() {
                    form.set("form_unit", customary_unit(&nutrient).to_string());
                }
            }
        }
    }

    /// `RegisterLot` parses, validates and only then writes. Rejections
    /// land in the status bar with the port's own message.
    fn submit_form(&mut self) {
        let Some(form) = &self.form else { return };
        let use_case = bootstrap::build_register_lot(&self.cfg.layout());
        if form.screen == Screen::Import {
            return self.run_import();
        }
        let sample = form.screen == Screen::NewSample;
        let (outcome, message) = match form.screen {
            Screen::NewSample => (
                use_case.add_soil_tests(&form.value("form_field_id"), &[form.soil_test_entry()]),
                "msg_sample_saved",
            ),
            Screen::EditLot => (use_case.edit_lot(&form.registration()), "msg_lot_edited"),
            _ => (use_case.register_lot(&form.registration()), "msg_lot_saved"),
        };

        match outcome {
            Err(e) => self.fail(e),
            // A lab report is one table with eight rows, not eight visits
            // through the menu. Saving a reading keeps the form open with
            // everything that does not change between rows — the lot, the
            // depths, and the last unit and method — so the next reading is
            // a nutrient and a number.
            Ok(()) if sample => {
                self.saved_readings += 1;
                let kept = self.keep_between_readings();
                self.reload();
                self.open_form(Screen::NewSample);
                if let Some(form) = &mut self.form {
                    for (label, value) in kept {
                        form.set(label, value);
                    }
                    form.idx = NEW_SAMPLE_FIELDS.iter().position(|f| *f == "form_nutrient").unwrap_or(0);
                }
                if !self.is_error {
                    self.message = format!("{} ({})", self.i18n.t(message), self.saved_readings);
                    self.is_error = false;
                }
            }
            Ok(()) => {
                self.form = None;
                self.saved_readings = 0;
                self.enter(Screen::Dashboard);
                self.reload();
                if !self.is_error {
                    self.info(message);
                }
            }
        }
    }

    /// The lab panel as a table, on the lot under the cursor.
    fn open_batch(&mut self) {
        let Some(lot) = self.selected_lot() else {
            return self.fail_key("err_no_lot");
        };
        let field_id = lot.field_id.clone();
        // A fresh lot has no `soil_tests.csv` row at all, which is not an
        // error to surface — it is the same "nothing yet" an empty table
        // already means.
        let existing = bootstrap::build_soil_tests(&self.cfg.layout())
            .get_tests_by_sample_id(&field_id)
            .unwrap_or_default();
        self.batch = Some(SampleBatch::new(field_id, &existing));
        self.enter(Screen::SampleBatch);
        self.info("msg_batch_open");
    }

    /// One call, not one per row: `add_soil_tests` already takes a slice
    /// and validates the whole set before writing any of it, so a panel
    /// with one bad row leaves nothing half-written.
    fn commit_batch(&mut self) {
        let Some(batch) = &self.batch else { return };
        let entries = batch.entries();
        let field_id = batch.field_id.clone();
        // An empty table is not a rejection to explain, it is a table
        // nobody filled — but committing it silently would look like a save.
        if entries.is_empty() {
            return self.fail_key("err_no_readings");
        }
        let saved = entries.len();
        match bootstrap::build_register_lot(&self.cfg.layout()).add_soil_tests(&field_id, &entries) {
            Ok(()) => {
                self.batch = None;
                self.enter(Screen::Dashboard);
                self.reload();
                if !self.is_error {
                    self.message = format!("{} ({saved})", self.i18n.t("msg_sample_saved"));
                    self.is_warning = false;
                }
            }
            // The table stays on screen with everything typed in it: a
            // refused panel is a panel to fix, not one to retype.
            Err(e) => self.fail(e),
        }
    }

    /// Reads the chosen file through the same importer the CLI uses.
    ///
    /// The form stays open and the report stays on screen: an import that
    /// rejected three rows is a file to go and fix, and closing the screen
    /// would take the line numbers with it.
    fn run_import(&mut self) {
        let Some(form) = &self.form else { return };
        let path = form.value("form_import_file");
        if path.trim().is_empty() {
            return self.fail_key("err_no_file");
        }

        self.import_report.clear();
        match csv_import::import(
            std::path::Path::new(path.trim()),
            &bootstrap::build_register_lot(&self.cfg.layout()),
        ) {
            Ok(report) => {
                self.import_report.push(report.summary());
                for (line, why) in &report.rejected {
                    self.import_report.push(format!("{} {line}: {why}", self.i18n.t("import_line")));
                }
                // New lots and samples change what every other screen shows.
                self.reload();
                if !self.is_error {
                    self.message = report.summary();
                    self.is_error = report.accepted == 0;
                }
            }
            Err(e) => {
                self.import_report.push(e.to_string());
                self.fail(e);
            }
        }
    }

    /// What carries over from one reading of a lab panel to the next.
    ///
    /// The lot and the depths are constant across a whole report. Unit and
    /// method are not — P is mg/kg by Olsen, K is cmolc/kg by ammonium
    /// acetate — but they repeat in runs, so carrying the last one forward
    /// is right more often than clearing it, and it is one keystroke to
    /// change when it is not.
    fn keep_between_readings(&self) -> Vec<(&'static str, String)> {
        let Some(form) = &self.form else { return Vec::new() };
        ["form_field_id", "form_unit", "form_method", "form_depth_from", "form_depth_to"]
            .into_iter()
            .map(|label| (label, form.value(label)))
            .collect()
    }

    fn filtered_crops(&self) -> Vec<&Crop> {
        let needle = self.filter.to_lowercase();
        self.crops
            .iter()
            .filter(|crop| {
                needle.is_empty()
                    || crop.crop_id.to_lowercase().contains(&needle)
                    || crop.name.to_lowercase().contains(&needle)
                    || crop.crop_type.to_lowercase().contains(&needle)
                    || crop.family.to_lowercase().contains(&needle)
            })
            .collect()
    }

    // ---- input -----------------------------------------------------------

    fn on_key(&mut self, key: KeyEvent) {
        // Any key that is not the delete key disarms a pending one, so an
        // armed deletion can never survive a detour and fire on a lot the
        // user has since moved away from.
        if !matches!(key.code, KeyCode::Char('x')) {
            self.pending_delete = None;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            // Raw mode swallows SIGINT, so Ctrl-C quits explicitly — and no
            // chord may fall through as text input.
            if key.code == KeyCode::Char('c') {
                self.running = false;
            }
            return;
        }
        let code = key.code;
        if self.help {
            self.help = false;
            return;
        }
        if self.filtering {
            return self.on_filter_key(code);
        }
        if self.editing_yield {
            return self.on_yield_key(code);
        }
        // The picker sits on top of the form and eats keys first.
        if self.picker.is_some() {
            return self.on_picker_key(code);
        }
        if self.form.as_ref().is_some_and(|form| form.editing) {
            return self.on_form_edit_key(code);
        }
        // The table owns its screen's keys, `Tab` included — see
        // `on_batch_key`.
        if self.batch.is_some() && self.screen == Screen::SampleBatch {
            return self.on_batch_key(code);
        }
        // `h`/`l` belong to the workspace three different ways, and the
        // module column keeps them as mnemonics — the same rule that has
        // always kept Settings' `h` from colliding with Home's.
        let in_workspace = !self.focus_modules;
        let in_settings = self.screen == Screen::Settings && in_workspace;
        let in_target = self.screen == Screen::Target && in_workspace;
        match code {
            KeyCode::Char('?') => self.help = true,
            // Home has one pane, so there is nothing to hand the cursor to:
            // a `Tab` there would light a lot column that is not drawn.
            KeyCode::Tab | KeyCode::BackTab if self.screen != Screen::Dashboard => {
                self.focus_modules = !self.focus_modules
            }
            // `s` saves what is being filled in, from anywhere on the
            // screen filling it. The save row and the last cell still
            // work; this is for people who would rather not walk to them.
            KeyCode::Char('s') if self.form.is_some() => self.submit_form(),
            KeyCode::Enter => self.activate(),
            KeyCode::Esc => self.back(),
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('/') if self.screen == Screen::Crops => {
                self.filtering = true;
                self.focus_modules = false;
            }
            // The launcher's menu acts on the selected lot, and Home is the
            // one screen with no column to move that selection in. `j`/`k`
            // belong to the menu, so the other axis takes the lot.
            KeyCode::Char('h') | KeyCode::Left if self.screen == Screen::Dashboard => self.select_lot(-1),
            KeyCode::Char('l') | KeyCode::Right if self.screen == Screen::Dashboard => self.select_lot(1),
            KeyCode::Char('h') | KeyCode::Left if in_settings => self.change_setting(-1),
            KeyCode::Char('l') | KeyCode::Right if in_settings => self.change_setting(1),
            KeyCode::Char('h') | KeyCode::Left if in_target => self.change_target(-1),
            KeyCode::Char('l') | KeyCode::Right if in_target => self.change_target(1),
            // A digit is how anyone types a number: it starts the edit and
            // is its own first character. `e` still works and is still in
            // the hint, but it is no longer the only door — and it had
            // become a door that could open somewhere else entirely, since
            // `e` is also the module column's mnemonic for "Edit lot".
            KeyCode::Char(c) if in_target && self.target_idx == TARGET_ROW_GOAL && (c.is_ascii_digit() || c == '.') => {
                self.yield_input.clear();
                self.editing_yield = true;
                self.on_yield_key(KeyCode::Char(c));
            }
            KeyCode::Char('e') if in_target && self.target_idx == TARGET_ROW_GOAL => self.editing_yield = true,
            KeyCode::Char('m') if in_target => self.toggle_demand_mode(),
            // Only from the lot list, where the lot being deleted is the one
            // under the cursor and on screen.
            KeyCode::Char('x') if self.screen == Screen::Dashboard || self.focus_modules => {
                self.delete_selected_lot()
            }
            KeyCode::Char('h') | KeyCode::Left if in_workspace => self.go_to_stage(-1),
            KeyCode::Char('l') | KeyCode::Right if in_workspace => self.go_to_stage(1),
            // `h`/`l` walk the stepper only on the stages that do not need
            // them for a control, which was three of five — so the ribbon
            // promised a walk it could not deliver on stage ③. These two
            // are never intercepted and work from anywhere in the flow.
            KeyCode::Char('[') if in_workspace => self.go_to_stage(-1),
            KeyCode::Char(']') if in_workspace => self.go_to_stage(1),
            KeyCode::Char('q') => self.back(),
            // The lot actions work from either pane: they are about the
            // selection, not about which pane is lit. `x` is the exception
            // and stays on the lot list, where the lot it deletes is on
            // screen under the cursor.
            KeyCode::Char(c @ ('n' | 'c' | 'e' | 'a' | 'b' | 'i' | ',')) if self.focus_modules || self.screen == Screen::Dashboard => {
                self.lot_action(c)
            }
            _ => {}
        }
    }

    fn on_filter_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.filter.clear();
                self.filtering = false;
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
            }
            KeyCode::Char(c) => self.filter.push(c),
            _ => {}
        }
        self.crop_idx = 0;
    }

    /// The table's own keys.
    ///
    /// `Tab` walks the cells of a row instead of the panes — the same trade
    /// stage ③ makes with `h`/`l`, and for the same reason: the screen has
    /// one control and it is the point of the screen. `Esc` is still the
    /// way out, so nothing is trapped.
    fn on_batch_key(&mut self, code: KeyCode) {
        let Some(batch) = &self.batch else { return };
        let (editing, on_last_cell) = (batch.editing, batch.on_last_cell());
        if editing {
            return self.on_batch_edit_key(code);
        }
        match code {
            // Nothing is written until the table is committed, so leaving
            // is leaving: a half-filled panel is not a draft, exactly as a
            // half-typed form is not one.
            KeyCode::Esc | KeyCode::Char('q') => {
                self.batch = None;
                self.enter(Screen::Dashboard);
            }
            KeyCode::Char('?') => self.help = true,
            // The table is written from the last cell, or from wherever
            // the cursor happens to be.
            KeyCode::Char('s') => self.commit_batch(),
            KeyCode::Enter if on_last_cell => self.commit_batch(),
            _ => {
                let Some(batch) = &mut self.batch else { return };
                match code {
                    KeyCode::Char('j') | KeyCode::Down => batch.row = step(batch.row, batch.rows.len(), 1),
                    KeyCode::Char('k') | KeyCode::Up => batch.row = step(batch.row, batch.rows.len(), -1),
                    KeyCode::Tab => batch.move_cell(1),
                    KeyCode::BackTab => batch.move_cell(-1),
                    // One rule, no exceptions: `h`/`l` walks whatever list
                    // the cell has behind it, `e` types. Both cells with a
                    // list also accept a value it does not carry, which is
                    // the point of `e` on a lab method.
                    KeyCode::Char('h') | KeyCode::Left => batch.cycle(-1),
                    KeyCode::Char('l') | KeyCode::Right => batch.cycle(1),
                    KeyCode::Enter | KeyCode::Char('e') => batch.editing = true,
                    _ => {}
                }
            }
        }
    }

    /// A lab reading, a method name or a depth: all free text, all typed
    /// into the cell under the cursor.
    fn on_batch_edit_key(&mut self, code: KeyCode) {
        let Some(batch) = &mut self.batch else { return };
        let Some(cell) = batch.cell() else {
            batch.editing = false;
            return;
        };
        match code {
            KeyCode::Enter | KeyCode::Esc => batch.editing = false,
            KeyCode::Backspace => {
                cell.pop();
            }
            KeyCode::Char(c) => cell.push(c),
            _ => {}
        }
    }

    /// Only digits and one separator get in.
    fn on_yield_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.yield_input.clear();
                self.editing_yield = false;
            }
            // Stays on stage ③: the goal was typed there, and the reader
            // has the stepper in front of them to move on with.
            KeyCode::Enter => match self.typed_yield_target() {
                Some(_) => {
                    self.editing_yield = false;
                    self.enter(Screen::Target);
                    self.info("msg_yield_set");
                }
                None => self.fail_key("err_bad_yield"),
            },
            KeyCode::Backspace => {
                self.yield_input.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => self.yield_input.push(c),
            KeyCode::Char('.') if !self.yield_input.contains('.') => self.yield_input.push('.'),
            _ => {}
        }
    }

    /// Same navigation keys as every other list in the app.
    fn on_picker_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.picker = None,
            KeyCode::Enter => self.commit_picker(),
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(picker) = &mut self.picker {
                    picker.idx = step(picker.idx, picker.options.len(), 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(picker) = &mut self.picker {
                    picker.idx = step(picker.idx, picker.options.len(), -1);
                }
            }
            _ => {}
        }
    }

    /// Free text: a new lot id, a lab reading, a coordinate.
    fn on_form_edit_key(&mut self, code: KeyCode) {
        let Some(form) = &mut self.form else { return };
        let Some(value) = form.fields.get_mut(form.idx).map(|field| &mut field.value) else {
            form.editing = false;
            return;
        };
        match code {
            KeyCode::Enter | KeyCode::Esc => form.editing = false,
            KeyCode::Backspace => {
                value.pop();
            }
            KeyCode::Char(c) => value.push(c),
            _ => {}
        }
    }

    /// Moving the lot cursor, wherever it is moved from.
    fn select_lot(&mut self, delta: isize) {
        let next = step(self.lot_idx, self.lots.len(), delta);
        if next != self.lot_idx {
            // A different lot invalidates the crop picked for the previous
            // one and its typed goal.
            self.clear_crop_choice();
            self.lot_idx = next;
            self.prefetch_climate();
        }
    }

    /// What a key does to the lot under the cursor.
    ///
    /// These were menu rows until the left column became the lot list. A
    /// list of places became a list of things, and what used to be a row is
    /// now a verb applied to the selection.
    fn lot_action(&mut self, pressed: char) {
        match pressed {
            '⏎' => self.open(Some(Screen::Soil)),
            'q' => self.running = false,
            'n' => self.open_form(Screen::NewLot),
            'c' => self.copy_selected_lot(),
            'e' => self.open_form(Screen::EditLot),
            // A reading belongs to a lot. The form prefills the selected
            // one and picks from the rest, but with nothing curated there
            // is nothing to prefill *or* pick from — and the field falls
            // back to free text, which is a lot id nobody can typo-check.
            'a' if self.selected_lot().is_none() => self.fail_key("err_no_lot"),
            'a' => self.open_form(Screen::NewSample),
            'b' => self.open_batch(),
            'x' => self.delete_selected_lot(),
            'i' => self.open_form(Screen::Import),
            ',' => self.enter(Screen::Settings),
            _ => {}
        }
    }

    /// A copy is the New lot form prefilled from the selection with the id
    /// cleared, because an id is the one thing a copy cannot share. No new
    /// port method: a duplicate is a registration whose values happen to
    /// come from a neighbour.
    fn copy_selected_lot(&mut self) {
        if self.selected_lot().is_none() {
            return self.fail_key("err_no_lot");
        }
        let prefill = self.lot_prefill();
        self.open_form(Screen::NewLot);
        if let Some(form) = &mut self.form {
            for (label, value) in prefill {
                // Everything but the id, which has to be new.
                if label != "form_field_id" {
                    form.set(label, value);
                }
            }
            form.idx = 0;
        }
        self.info("msg_copy_lot");
    }

    fn move_selection(&mut self, delta: isize) {
        if self.focus_modules {
            return self.select_lot(delta);
        }
        match self.screen {
            // The launcher menu, which is what the workspace *is* on Home.
            Screen::Dashboard => self.menu_idx = step(self.menu_idx, LOT_ACTIONS.len(), delta),
            Screen::Crops => self.crop_idx = step(self.crop_idx, self.filtered_crops().len(), delta),
            Screen::Settings => self.setting_idx = step(self.setting_idx, SETTINGS.len(), delta),
            // One row past the last field is the save row.
            Screen::NewLot | Screen::EditLot | Screen::NewSample | Screen::Import => {
                if let Some(form) = &mut self.form {
                    form.idx = step(form.idx, form.fields.len() + 1, delta);
                }
            }
            Screen::Target => self.target_idx = step(self.target_idx, TARGET_ROWS, delta),
            // Reached only if the table screen is open without a table;
            // `on_batch_key` owns j/k the rest of the time.
            Screen::SampleBatch => {
                if let Some(batch) = &mut self.batch {
                    batch.row = step(batch.row, batch.rows.len(), delta);
                }
            }
            // Read-only text: j/k scrolls.
            Screen::Soil | Screen::Plan | Screen::Sources => self.scroll_by(delta),
        }
    }

    /// The page ends where the last frame said it ended.
    ///
    /// Scrolling was unbounded downward: `j` past the last line kept
    /// walking into blank space and needed as many `k` to come back. The
    /// line count is a property of the rendering, so it comes back from
    /// `ui::draw` — the one thing that file writes.
    fn scroll_by(&mut self, delta: isize) {
        let last = self.content_height.get().saturating_sub(self.viewport_height.get());
        self.scroll = match delta < 0 {
            true => self.scroll.saturating_sub(1),
            false => self.scroll.saturating_add(1).min(last),
        };
    }

    fn activate(&mut self) {
        // Enter on a lot opens the flow *on that lot*, at stage ①. It used
        // to jump straight to the finished plan, which skipped the ribbon
        // the screen spends two rows drawing.
        if self.focus_modules {
            return self.open(Some(Screen::Soil));
        }
        match self.screen {
            // Enter runs the menu row under the cursor, which is the whole
            // point of drawing a cursor on it.
            Screen::Dashboard => self.lot_action(LOT_ACTIONS[self.menu_idx].1),
            Screen::Crops => {
                let Some(crop_id) = self.filtered_crops().get(self.crop_idx).map(|crop| crop.crop_id.clone()) else {
                    return;
                };
                self.clear_crop_choice();
                let curated = self.has_curated_yield_target(&crop_id);
                self.crop_override = Some(crop_id);
                // Picking a crop is what stage ② is for, so it advances —
                // to the goal, which is the next thing the plan needs.
                self.enter(Screen::Target);
                if curated {
                    self.info("msg_crop_selected");
                } else {
                    // Most catalog crops land here with no curated goal, so
                    // ask rather than let the plan fail on a missing row.
                    self.editing_yield = true;
                    self.info("msg_yield_needed");
                }
            }
            Screen::Settings => self.change_setting(1),
            Screen::NewLot | Screen::EditLot | Screen::NewSample | Screen::Import => self.activate_form_row(),
            // The table owns Enter; see `on_batch_key`.
            Screen::SampleBatch => {}
            // Enter is "next stage" along the flow; on the last one it is
            // "run it again", which is what a changed input wants — and
            // what the cached plan has to stand aside for.
            Screen::Soil | Screen::Target | Screen::Sources => self.go_to_stage(1),
            Screen::Plan => self.recalculate(),
        }
    }

    /// Land on a screen with the cursor in the workspace, which is where
    /// the point of every screen is — the launcher's menu on Home, the
    /// stage body everywhere else. `Tab` is what hands it to the lot column
    /// on the screens that draw one.
    fn enter(&mut self, screen: Screen) {
        self.screen = screen;
        self.focus_modules = false;
        self.scroll = 0;
        // Stage ③ always opens on the goal: it is the thing the flow came
        // here for, and the basis has a working default.
        if screen == Screen::Target {
            self.target_idx = TARGET_ROW_GOAL;
        }
    }

    fn open(&mut self, target: Option<Screen>) {
        match target {
            None => self.running = false,
            // Both halves of one plan, so both stages compute it.
            Some(screen @ (Screen::Plan | Screen::Sources)) => {
                self.enter(screen);
                self.calculate();
            }
            Some(screen @ Screen::Soil) => {
                self.enter(screen);
                self.inspect();
            }
            Some(screen @ (Screen::NewLot | Screen::EditLot | Screen::NewSample | Screen::Import)) => self.open_form(screen),
            Some(screen) => self.enter(screen),
        }
    }

    /// One step along the fertilization flow. A screen outside it has no
    /// stage to move from, so `h`/`l` do nothing there.
    fn go_to_stage(&mut self, delta: isize) {
        let Some(current) = stage_index(self.screen) else { return };
        let next = step(current, STAGES.len(), delta);
        if next != current {
            self.open(Some(STAGES[next].0));
        }
    }

    /// `h`/`l` on stage ③ changes whichever control the cursor is on, the
    /// same rule the Settings screen follows.
    fn change_target(&mut self, delta: isize) {
        match self.target_idx {
            TARGET_ROW_BASIS => self.toggle_demand_mode(),
            // On the exit row `h`/`l` walk the stepper, so the instinct
            // every other stage teaches works here too.
            TARGET_ROW_CONTINUE => self.go_to_stage(delta),
            _ => self.step_yield(delta),
        }
    }

    /// ±[`YIELD_STEP`] on the goal actually in force — the typed one, or
    /// the curated one it departs from — so the first press nudges the
    /// number the plan would have used rather than starting from zero.
    fn step_yield(&mut self, delta: isize) {
        if self.active_crop().is_none() {
            return self.fail_key("target_no_crop");
        }
        let current = self
            .typed_yield_target()
            .map(|target| target.value)
            .or_else(|| self.curated_yield_target().map(|target| target.value))
            .unwrap_or(0.0);
        let next = (current + delta as f64 * YIELD_STEP).max(YIELD_STEP);
        self.yield_input = format!("{next:.1}");
        self.info("msg_yield_set");
    }

    /// Extraction ⇄ absorption. A plan already on screen is recomputed, so
    /// the toggle never leaves numbers behind that answer the other
    /// question.
    fn toggle_demand_mode(&mut self) {
        self.demand_mode = match self.demand_mode {
            NutrientDemandMode::Extraction => NutrientDemandMode::Absorption,
            NutrientDemandMode::Absorption => NutrientDemandMode::Extraction,
        };
        if self.plan.is_some() {
            self.calculate();
        }
        if !self.is_error {
            self.info("msg_mode_changed");
        }
    }

    fn back(&mut self) {
        if self.screen == Screen::Dashboard {
            self.running = false;
        } else {
            // A half-typed lot is not a draft, and neither is a half-filled
            // lab panel.
            self.form = None;
            self.batch = None;
            self.picker = None;
            self.enter(Screen::Dashboard);
        }
    }

    fn change_setting(&mut self, delta: isize) {
        match SETTINGS[self.setting_idx] {
            "settings_language" => {
                self.i18n = I18n::new(self.i18n.language().toggled());
                self.info("msg_language_changed");
            }
            "settings_theme" => {
                self.theme = theme::step(self.theme, delta);
                self.info("msg_theme_changed");
            }
            "settings_profile" if !self.profiles.is_empty() => {
                let current = self.profiles.iter().position(|p| *p == self.cfg.profile).unwrap_or(0);
                let len = self.profiles.len();
                let next = (current + if delta < 0 { len - 1 } else { 1 }) % len;
                self.cfg.profile = self.profiles[next].clone();
                self.reload();
                if !self.is_error {
                    self.info("msg_profile_changed");
                }
            }
            // The three formulation knobs. Each re-runs the product half
            // only — the balance did not change, so recomputing it would
            // just spend a climate lookup to reach the same numbers.
            "settings_strategy" => {
                self.formulation.strategy = self.formulation.strategy.other();
                self.recalculate_recommendation();
            }
            "settings_bag" => {
                let current =
                    BAG_WEIGHTS_KG.iter().position(|kg| *kg == self.formulation.bag_weight_kg).unwrap_or(0);
                let len = BAG_WEIGHTS_KG.len();
                let next = (current + if delta < 0 { len - 1 } else { 1 }) % len;
                self.formulation.bag_weight_kg = BAG_WEIGHTS_KG[next];
                self.recalculate_recommendation();
            }
            // The rest are read-only paths.
            _ => {}
        }
        self.persist_settings();
    }

    fn recalculate_recommendation(&mut self) {
        let Some(plan) = self.plan.take() else { return };
        self.recommendation = self.recommend(&plan);
        self.plan = Some(plan);
    }

    // ---- status bar ------------------------------------------------------

    fn info(&mut self, id: &str) {
        self.message = self.i18n.t(id).to_string();
        self.is_error = false;
        self.is_warning = false;
    }

    /// Something the reader should know and can still act on. Not an error:
    /// nothing was refused and nothing is blocked.
    fn warn(&mut self, id: &str) {
        self.message = self.i18n.t(id).to_string();
        self.is_error = false;
        self.is_warning = true;
    }

    fn fail(&mut self, error: DomainError) {
        self.message = error.to_string();
        self.is_error = true;
        self.is_warning = false;
    }

    fn fail_key(&mut self, id: &str) {
        self.message = self.i18n.t(id).to_string();
        self.is_error = true;
        self.is_warning = false;
    }
}

/// Saturating one-step move; no wraparound, so holding j/k parks on the edge.
fn step(index: usize, len: usize, delta: isize) -> usize {
    match len {
        0 => 0,
        _ if delta < 0 => index.saturating_sub(1),
        len => (index + 1).min(len - 1),
    }
}

fn io_error(error: std::io::Error) -> DomainError {
    DomainError::DataSource(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `None` for the cache: the tests never open a socket.
    fn tui() -> Tui {
        Tui::new(bootstrap::build_app_from_repo_data(), tui_settings::TuiSettings::default(), None)
    }

    fn press(tui: &mut Tui, code: KeyCode) {
        tui.on_key(KeyEvent::from(code));
    }

    /// Stage ③ draws two controls and used to let you reach one. The basis
    /// was radio buttons — which promise "move here and choose" — changed
    /// only by an `m` nothing on screen mentioned, while `j`/`k` nudged the
    /// number instead of moving between them.
    #[test]
    fn both_of_the_goal_stages_controls_are_reachable_with_the_usual_keys() {
        let mut tui = tui();
        tui.enter(Screen::Target);
        assert!(!tui.focus_modules, "entering a stage puts the cursor in the workspace");
        assert_eq!(tui.target_idx, TARGET_ROW_GOAL, "the stage opens on what the flow came for");

        // j/k moves between the controls rather than changing one of them.
        press(&mut tui, KeyCode::Char('j'));
        assert_eq!(tui.target_idx, TARGET_ROW_BASIS);
        press(&mut tui, KeyCode::Char('k'));
        assert_eq!(tui.target_idx, TARGET_ROW_GOAL);

        // h/l changes whichever control is under the cursor — the Settings
        // rule, applied here too.
        let before = tui.yield_input.clone();
        press(&mut tui, KeyCode::Char('l'));
        assert_ne!(tui.yield_input, before, "on the goal row, h/l moves the goal");

        let basis = tui.demand_mode;
        press(&mut tui, KeyCode::Char('j'));
        press(&mut tui, KeyCode::Char('l'));
        assert_ne!(tui.demand_mode, basis, "on the basis row, h/l switches the basis");
        // ...and no longer touches the number it is not pointing at.
        let goal = tui.yield_input.clone();
        press(&mut tui, KeyCode::Char('h'));
        assert_eq!(tui.yield_input, goal);
    }

    /// Typing a goal must not need a letter key nobody can see — and `e`
    /// had become worse than invisible, since it is also the module
    /// column's mnemonic for "Edit lot" and would jump there instead.
    #[test]
    fn a_digit_starts_typing_the_yield_goal() {
        let mut tui = tui();
        tui.enter(Screen::Target);

        press(&mut tui, KeyCode::Char('2'));
        assert!(tui.editing_yield, "a digit opens the editor");
        assert_eq!(tui.yield_input, "2", "and is its own first character");
        press(&mut tui, KeyCode::Char('4'));
        press(&mut tui, KeyCode::Char('.'));
        press(&mut tui, KeyCode::Char('5'));
        assert_eq!(tui.yield_input, "24.5");
        press(&mut tui, KeyCode::Enter);
        assert!(!tui.editing_yield);
        assert_eq!(tui.typed_yield_target().expect("a goal").value, 24.5);

        // `e` still works, and starts from whatever is already there.
        press(&mut tui, KeyCode::Char('e'));
        assert!(tui.editing_yield);

        // On the basis row a digit is not a goal, so it must not hijack it.
        let mut other = Tui::new(bootstrap::build_app_from_repo_data(), tui_settings::TuiSettings::default(), None);
        other.enter(Screen::Target);
        press(&mut other, KeyCode::Char('j'));
        press(&mut other, KeyCode::Char('7'));
        assert!(!other.editing_yield, "the cursor is not on the goal");
    }

    /// The left column was a menu of *places*. It is the lot list now, and
    /// what used to be a row is a verb applied to the selection.
    #[test]
    fn the_lot_column_selects_lots_and_its_keys_act_on_the_selection() {
        let mut tui = tui();
        tui.focus_modules = true;
        assert!(tui.lots.len() >= 2, "the repository ships two demo lots");

        // j/k moves the lot cursor, wherever the focus is.
        let first = tui.selected_lot().expect("a lot").field_id.clone();
        press(&mut tui, KeyCode::Char('j'));
        assert_ne!(tui.selected_lot().expect("a lot").field_id, first, "the column selects lots");
        press(&mut tui, KeyCode::Char('k'));
        assert_eq!(tui.selected_lot().expect("a lot").field_id, first);

        // Enter enters the flow on that lot rather than jumping to the end.
        press(&mut tui, KeyCode::Enter);
        assert_eq!(tui.screen, STAGES[0].0);
        press(&mut tui, KeyCode::Esc);

        // Every verb reaches its screen from the column.
        for (key, screen) in [
            ('n', Screen::NewLot),
            ('e', Screen::EditLot),
            ('a', Screen::NewSample),
            ('i', Screen::Import),
            (',', Screen::Settings),
        ] {
            tui.focus_modules = true;
            tui.screen = Screen::Dashboard;
            press(&mut tui, KeyCode::Char(key));
            assert_eq!(tui.screen, screen, "`{key}` has to reach {screen:?}");
        }
    }

    /// Home's menu is a menu: `Tab` lights it, `j`/`k` walk it and `Enter`
    /// runs the row under the cursor. It had degraded to a list of labels
    /// with no cursor, which is a legend — the keys worked, but the rows
    /// did not, and a row you can see a marker on is a row you expect to
    /// be able to land on.
    #[test]
    fn the_launcher_menu_is_walked_with_the_cursor_and_run_with_enter() {
        let mut tui = tui();
        tui.enter(Screen::Dashboard);
        // Home has one pane and the menu is it: the cursor lands there.
        assert!(!tui.focus_modules);

        let lot = tui.selected_lot().expect("a lot").field_id.clone();
        press(&mut tui, KeyCode::Char('j'));
        assert_eq!(tui.menu_idx, 1, "the cursor walks the menu, not the lots");
        assert_eq!(tui.selected_lot().expect("a lot").field_id, lot, "and the lot under it does not move");

        // The other axis is what moves the lot the menu acts on, since
        // there is no column to move it in.
        press(&mut tui, KeyCode::Char('l'));
        assert_ne!(tui.selected_lot().expect("a lot").field_id, lot, "h/l picks the subject");
        press(&mut tui, KeyCode::Char('h'));
        assert_eq!(tui.selected_lot().expect("a lot").field_id, lot);
        assert_eq!(tui.menu_idx, 1, "and leave the menu cursor where it was");

        // Row 1 is New lot, and Enter is what runs it.
        press(&mut tui, KeyCode::Enter);
        assert_eq!(tui.screen, Screen::NewLot);
        press(&mut tui, KeyCode::Esc);

        // Every row runs from the menu, the first one included.
        for (index, (_, key, _)) in LOT_ACTIONS.iter().enumerate() {
            if *key == 'q' || *key == 'x' {
                continue; // one ends the session, the other asks twice.
            }
            tui.screen = Screen::Dashboard;
            tui.focus_modules = false;
            tui.menu_idx = index;
            press(&mut tui, KeyCode::Enter);
            assert_ne!(tui.screen, Screen::Dashboard, "row {index} (`{key}`) did nothing");
            press(&mut tui, KeyCode::Esc);
        }

        // The last row quits, which is what the old menu's `Quit` row did.
        tui.screen = Screen::Dashboard;
        tui.focus_modules = false;
        tui.menu_idx = LOT_ACTIONS.len() - 1;
        press(&mut tui, KeyCode::Enter);
        assert!(!tui.running);
    }

    /// A copy is the New lot form wearing the neighbour's values, with the
    /// one field a copy cannot share left empty.
    #[test]
    fn copying_a_lot_prefills_everything_but_the_id() {
        let mut tui = tui();
        tui.focus_modules = true;
        tui.screen = Screen::Dashboard;
        let source = tui.selected_lot().expect("a lot").field_id.clone();

        press(&mut tui, KeyCode::Char('c'));
        assert_eq!(tui.screen, Screen::NewLot);
        let form = tui.form.as_ref().expect("form");
        assert_eq!(form.value("form_field_id"), "", "an id is the one thing a copy cannot share");
        assert_ne!(form.value("form_texture"), "", "and everything else came across");
        assert_ne!(form.value("form_ph"), "");
        assert_eq!(form.idx, 0, "with the cursor on the field that needs typing");

        // The source lot is untouched — a copy is a new registration.
        assert_eq!(tui.lots.iter().filter(|lot| lot.field_id == source).count(), 1);
    }

    /// The picker showed six absolute paths in a 40-column list, which is
    /// six rows reading `/home/ziegler/.local/share/non-nobis-` and nothing
    /// else. Rows are basenames now, and the directory is on the frame.
    #[test]
    fn the_file_browser_shows_names_and_can_leave_the_folder_it_opened_in() {
        let dir = std::env::temp_dir().join(format!("nns_browse_{}", std::process::id()));
        let nested = dir.join("panels");
        std::fs::create_dir_all(&nested).expect("dirs");
        std::fs::write(dir.join("lots.csv"), "field_id\n").expect("write");
        std::fs::write(dir.join("notes.txt"), "ignored").expect("write");
        std::fs::write(nested.join("panel.csv"), "sample_id\n").expect("write");

        let (values, labels) = browse_dir(&dir);
        // A name, not a path: that is the whole fix.
        assert!(labels.contains(&"lots.csv".to_string()), "{labels:?}");
        assert!(labels.contains(&"panels/".to_string()), "a subdirectory is offered, so the browser can descend");
        assert_eq!(labels[0], "../", "and it can leave the folder it opened in");
        assert!(!labels.iter().any(|l| l.contains("notes")), "only CSVs are candidates");
        // The value is still the absolute path the field receives.
        assert!(values.iter().any(|v| v.ends_with("lots.csv") && v.starts_with('/')), "{values:?}");
        assert_eq!(values.len(), labels.len());

        // Descending is a real listing, not the same one.
        let (_, deeper) = browse_dir(&nested);
        assert!(deeper.contains(&"panel.csv".to_string()), "{deeper:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every path in is reachable: browse to it, or type it.
    #[test]
    fn the_import_field_accepts_a_typed_absolute_path() {
        let mut tui = tui();
        tui.open_form(Screen::Import);
        let field = &tui.form.as_ref().expect("form").fields[0];
        assert!(!field.is_choice(), "the file field is text, so a path can always be typed");

        // Enter opens the browser rather than the text cursor...
        tui.activate_form_row();
        let picker = tui.picker.as_ref().expect("a browser");
        assert!(!picker.title.is_empty(), "the frame names the directory");
        assert_eq!(picker.options.last().map(String::as_str), Some(TYPE_IT_IN));

        // ...and the last row hands the field back for typing.
        let last = tui.picker.as_ref().expect("picker").options.len() - 1;
        tui.picker.as_mut().expect("picker").idx = last;
        tui.commit_picker();
        assert!(tui.picker.is_none());
        assert!(tui.form.as_ref().expect("form").editing, "picking it starts a text entry");

        for c in "/tmp/x.csv".chars() {
            tui.on_form_edit_key(KeyCode::Char(c));
        }
        assert_eq!(tui.form.as_ref().expect("form").value("form_import_file"), "/tmp/x.csv");
    }

    /// The stepper ribbon is drawn on every stage, so it has to be walkable
    /// from every stage — `h`/`l` are taken by a control on stage ③.
    #[test]
    fn the_stepper_can_be_walked_from_the_stage_that_needs_h_and_l() {
        let mut tui = tui();
        tui.enter(Screen::Target);

        press(&mut tui, KeyCode::Char('['));
        assert_eq!(tui.screen, Screen::Crops, "[ steps back where h cannot");
        press(&mut tui, KeyCode::Char(']'));
        assert_eq!(tui.screen, Screen::Target, "] steps forward again");

        // And they are the workspace's keys, not the module column's.
        tui.focus_modules = true;
        let screen = tui.screen;
        press(&mut tui, KeyCode::Char('['));
        assert_eq!(tui.screen, screen);
    }

    /// A disposable copy of `data/`, so the write tests use the real
    /// adapters without touching the tracked curated files.
    struct Sandbox {
        root: std::path::PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("nns_tui_{}_{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            copy_dir(std::path::Path::new("data"), &root.join("data"));
            Self { root }
        }

        fn tui(&self) -> Tui {
            let cfg = Composition { data_root: self.root.join("data"), profile: "global".to_string() };
            Tui::new(cfg, tui_settings::TuiSettings::default(), None)
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).expect("sandbox dir");
        for entry in std::fs::read_dir(from).expect("read data dir").flatten() {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).expect("copy data file");
            }
        }
    }

    /// Navigation and submission still go through real key handling.
    fn fill(tui: &mut Tui, label: &str, value: &str) {
        let form = tui.form.as_mut().expect("a form is on screen");
        let field = form
            .fields
            .iter_mut()
            .find(|field| field.label == label)
            .unwrap_or_else(|| panic!("no field {label}"));
        assert!(
            field.options.is_empty() || field.options.iter().any(|option| option == value),
            "{label} is a closed set and {value:?} is not one of its options: {:?}",
            field.options
        );
        field.value = value.to_string();
    }

    /// Chooses a value off a field's own list, which is the only way the
    /// app itself fills a closed set — and the only way a field that
    /// depends on another one (the methods on the nutrient) sees it.
    fn pick(tui: &mut Tui, label: &str, value: &str) {
        go_to_field(tui, label);
        press(tui, KeyCode::Enter);
        let picker = tui.picker.as_mut().expect("a list unfolds");
        picker.idx = picker
            .options
            .iter()
            .position(|option| option == value)
            .unwrap_or_else(|| panic!("{label} does not offer {value:?}: {:?}", picker.options));
        tui.commit_picker();
    }

    /// Moves to a field by label, the way a user would.
    fn go_to_field(tui: &mut Tui, label: &str) {
        let form = tui.form.as_ref().expect("a form is on screen");
        let target = form.fields.iter().position(|field| field.label == label).expect("field exists");
        while tui.form.as_ref().expect("form").idx != target {
            let idx = tui.form.as_ref().expect("form").idx;
            press(tui, if idx < target { KeyCode::Char('j') } else { KeyCode::Char('k') });
        }
    }

    fn save_form(tui: &mut Tui) {
        let save_row = tui.form.as_ref().expect("a form is on screen").save_row();
        for _ in 0..save_row {
            press(tui, KeyCode::Char('j'));
        }
        press(tui, KeyCode::Enter);
    }

    /// Moves the cell cursor and types into it through real key handling.
    /// Backspaces out whatever the cell already carries first — a reopened
    /// panel prefills from the lot's last reading, and `e` edits in place
    /// rather than clearing, exactly like every other field in the app.
    fn set_cell(tui: &mut Tui, nutrient: &str, column: usize, value: &str) {
        let existing_len = {
            let batch = tui.batch.as_mut().expect("a table is on screen");
            batch.row = batch.rows.iter().position(|row| row[0] == nutrient).expect("a row per nutrient");
            batch.col = column;
            batch.rows[batch.row][column].chars().count()
        };
        press(tui, KeyCode::Char('e'));
        for _ in 0..existing_len {
            press(tui, KeyCode::Backspace);
        }
        for c in value.chars() {
            press(tui, KeyCode::Char(c));
        }
        press(tui, KeyCode::Enter);
    }

    /// `cmolcarga/kg` is one column of a lab report, spanning every reading
    /// off the exchange complex. Picking the nutrient is enough to know
    /// which unit its value arrives in.
    #[test]
    fn choosing_a_base_answers_the_unit_it_is_reported_in() {
        let mut tui = tui();
        tui.open(Some(Screen::NewSample));
        assert_eq!(tui.form.as_ref().expect("form").value("form_unit"), "", "nothing is assumed before the nutrient");

        pick(&mut tui, "form_nutrient", "Ca");
        assert_eq!(tui.form.as_ref().expect("form").value("form_unit"), "cmolc_per_kg");

        // The suggestion fills an empty row; it never overrules an answer
        // already there — including the one carried from the last reading.
        pick(&mut tui, "form_nutrient", "P");
        assert_eq!(
            tui.form.as_ref().expect("form").value("form_unit"),
            "cmolc_per_kg",
            "a unit already on the form is the user's, not a default to revise"
        );
    }

    /// A lab report names its extraction, and a picker is how every closed
    /// set in this app is filled — except this one is not closed: a lab is
    /// free to report an extraction nobody here has seen.
    #[test]
    fn the_method_list_offers_what_that_nutrient_is_read_with_and_nothing_else() {
        let mut tui = tui();
        tui.open(Some(Screen::NewSample));

        let methods = |tui: &mut Tui| {
            go_to_field(tui, "form_method");
            press(tui, KeyCode::Enter);
            let options = tui.picker.as_ref().expect("the method list").options.clone();
            tui.picker = None;
            options
        };

        // A phosphorus extraction on a cation is not a choice, it is a
        // wrong answer waiting in a menu.
        pick(&mut tui, "form_nutrient", "Ca");
        let calcium = methods(&mut tui);
        assert_eq!(calcium, vec!["AcONH4_1N_pH7".to_string(), TYPE_IT_IN.to_string()]);
        for wrong in ["Olsen", "Mehlich", "Bray_II", "hot_water"] {
            assert!(!calcium.contains(&wrong.to_string()), "{wrong} is not how calcium is read: {calcium:?}");
        }

        // Phosphorus is the nutrient the method actually reclassifies, and
        // it gets all three of its own.
        pick(&mut tui, "form_nutrient", "P");
        let phosphorus = methods(&mut tui);
        for expected in ["Bray_II", "Olsen", "Mehlich"] {
            assert!(phosphorus.contains(&expected.to_string()), "{phosphorus:?}");
        }
        // ...and the method the last nutrient answered for is gone with it.
        assert_eq!(tui.form.as_ref().expect("form").value("form_method"), "");

        // Every list keeps the row that hands the field back for typing: a
        // lab is free to report an extraction nobody here has seen.
        assert_eq!(phosphorus.last().map(String::as_str), Some(TYPE_IT_IN));
        go_to_field(&mut tui, "form_method");
        press(&mut tui, KeyCode::Enter);
        let last = tui.picker.as_ref().expect("picker").options.len() - 1;
        tui.picker.as_mut().expect("picker").idx = last;
        tui.commit_picker();
        assert!(tui.form.as_ref().expect("form").editing, "the sentinel starts a text entry");
        for c in "NTC_5350".chars() {
            press(&mut tui, KeyCode::Char(c));
        }
        assert_eq!(tui.form.as_ref().expect("form").value("form_method"), "NTC_5350");
    }

    /// In the table the same list is walked with `h`/`l`, and an empty
    /// method cell offers the extraction that nutrient is customarily read
    /// with — one press for the common case, still an act.
    #[test]
    fn an_empty_method_cell_offers_the_extraction_that_nutrient_is_read_with() {
        let mut tui = tui();
        // Built directly rather than through `b`: this is about the default
        // an empty cell offers, not about what a lot's own history prefills
        // — see `a_reopened_panel_shows_what_the_lot_already_carries`.
        tui.batch = Some(SampleBatch::new("TEST".to_string(), &[]));
        tui.screen = Screen::SampleBatch;

        for (nutrient, expected) in [("P", "Bray_II"), ("Ca", "AcONH4_1N_pH7"), ("Al", "KCl_1N"), ("B", "hot_water")] {
            {
                let batch = tui.batch.as_mut().expect("a table");
                batch.row = batch.rows.iter().position(|row| row[0] == nutrient).expect("a row");
                batch.col = BATCH_COL_METHOD;
                assert_eq!(batch.rows[batch.row][BATCH_COL_METHOD], "", "the cell starts empty");
            }
            press(&mut tui, KeyCode::Char('l'));
            let batch = tui.batch.as_ref().expect("a table");
            assert_eq!(batch.rows[batch.row][BATCH_COL_METHOD], expected, "{nutrient}");
        }

        // From there it is a list like any other...
        let method_of = |tui: &Tui, nutrient: &str| {
            let batch = tui.batch.as_ref().expect("a table");
            batch.rows.iter().find(|row| row[0] == nutrient).expect("a row")[BATCH_COL_METHOD].clone()
        };
        {
            let batch = tui.batch.as_mut().expect("a table");
            batch.row = batch.rows.iter().position(|row| row[0] == "P").expect("a row");
        }
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(method_of(&tui, "P"), "Olsen", "h/l walks on through that nutrient's own list");
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(method_of(&tui, "P"), "Mehlich");
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(method_of(&tui, "P"), "Bray_II", "and comes back round");
        // Boron has one extraction, so there is nowhere to walk to.
        {
            let batch = tui.batch.as_mut().expect("a table");
            batch.row = batch.rows.iter().position(|row| row[0] == "B").expect("a row");
        }
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(method_of(&tui, "B"), "hot_water", "a one-entry list stays put rather than emptying");

        // ...and `e` still types one the list has never heard of.
        set_cell(&mut tui, "Zn", BATCH_COL_METHOD, "NTC_5526");
        assert_eq!(method_of(&tui, "Zn"), "NTC_5526");
    }

    /// One key writes whatever is being filled in, wherever the cursor
    /// happens to be on it. The save row and the last cell still work —
    /// this is the key for not walking to them.
    #[test]
    fn s_saves_what_is_on_screen_from_anywhere_on_it() {
        let sandbox = Sandbox::new("save_key");
        let mut tui = sandbox.tui();
        let curated = sandbox.root.join("data/curated/soil_tests.csv");
        let written = || std::fs::read_to_string(&curated).expect("curated file");

        // A form, from its first field rather than from the save row.
        tui.screen = Screen::Dashboard;
        press(&mut tui, KeyCode::Char('a'));
        assert_eq!(tui.screen, Screen::NewSample, "`a` is the reading key now");
        pick(&mut tui, "form_nutrient", "P");
        fill(&mut tui, "form_value", "21");
        fill(&mut tui, "form_method", "Olsen");
        fill(&mut tui, "form_depth_to", "20");
        go_to_field(&mut tui, "form_field_id");
        press(&mut tui, KeyCode::Char('s'));
        assert!(!tui.is_error, "the reading should have saved: {}", tui.message);

        // And the table, from a cell that is not the last one.
        press(&mut tui, KeyCode::Esc);
        press(&mut tui, KeyCode::Char('b'));
        set_cell(&mut tui, "Zn", BATCH_COL_VALUE, "1.8");
        set_cell(&mut tui, "Zn", BATCH_COL_METHOD, "DTPA");
        {
            let batch = tui.batch.as_mut().expect("a table");
            batch.row = 0;
            batch.col = BATCH_COL_VALUE;
            assert!(!batch.on_last_cell(), "the point is that this is not where Enter commits");
        }
        press(&mut tui, KeyCode::Char('s'));
        assert!(!tui.is_error, "the panel should have saved: {}", tui.message);
        assert_eq!(tui.screen, Screen::Dashboard, "a committed table is done");

        // Both writes are on disk. The curated writer supersedes a reading
        // of the same nutrient rather than appending a second one, so this
        // is what the rows say and not how many there are.
        let file = written();
        assert!(file.contains(",P,21,mg_per_kg,Olsen,"), "the form's reading:\n{file}");
        assert!(file.contains(",Zn,1.8,mg_per_kg,DTPA,"), "the table's reading:\n{file}");
    }

    /// A reading belongs to a lot. With nothing curated the sample form's
    /// lot field has nothing to prefill or pick from and degrades to free
    /// text, which is a lot id nobody can typo-check.
    #[test]
    fn a_reading_needs_a_lot_to_belong_to() {
        let mut tui = tui();
        tui.screen = Screen::Dashboard;
        tui.lots.clear();

        for key in ['a', 'b'] {
            press(&mut tui, KeyCode::Char(key));
            assert_eq!(tui.screen, Screen::Dashboard, "`{key}` opened a reading with no lot to attach it to");
            assert!(tui.is_error, "and said nothing about why");
        }

        // With a lot under the cursor both are open doors again.
        tui.reload();
        assert!(!tui.lots.is_empty(), "the repository ships demo lots");
        press(&mut tui, KeyCode::Char('b'));
        assert_eq!(tui.screen, Screen::SampleBatch);
    }

    /// A lab panel is one visit: fill the rows the report carries, leave
    /// the rest, write once.
    ///
    /// A fresh lot, not one of the fixture's — those already carry a full
    /// panel, which is exactly the case [`a_reopened_panel_shows_what_the_lot_already_carries`]
    /// covers.
    #[test]
    fn the_batch_table_writes_only_the_rows_that_carry_a_reading() {
        let sandbox = Sandbox::new("batch");
        let mut tui = sandbox.tui();
        tui.open(Some(Screen::NewLot));
        fill(&mut tui, "form_field_id", "LOT-BLANK");
        fill(&mut tui, "form_texture", "loam");
        fill(&mut tui, "form_irrigation", "rainfed");
        fill(&mut tui, "form_om", "3.2");
        fill(&mut tui, "form_ph", "5.4");
        fill(&mut tui, "form_cec", "12");
        fill(&mut tui, "form_bulk_density", "1.3");
        fill(&mut tui, "form_arable_depth", "0.2");
        save_form(&mut tui);
        assert!(!tui.is_error, "the lot should have saved: {}", tui.message);
        tui.lot_idx = tui.lots.iter().position(|lot| lot.field_id == "LOT-BLANK").expect("new lot listed");
        tui.screen = Screen::Dashboard;

        let lot = tui.selected_lot().expect("a lot").field_id.clone();
        let curated = sandbox.root.join("data/curated/soil_tests.csv");
        let rows_before = std::fs::read_to_string(&curated)
            .expect("curated file")
            .lines()
            .filter(|line| line.starts_with(&format!("{lot},")))
            .count();

        press(&mut tui, KeyCode::Char('b'));
        assert_eq!(tui.screen, Screen::SampleBatch);
        let rows = &tui.batch.as_ref().expect("a table").rows;
        assert_eq!(rows.len(), soil_test_nutrients().len(), "one row per nutrient a lab reports");
        assert!(rows.iter().all(|row| row[0] != "N"), "N is derived from organic matter, never measured");

        // A reading off the exchange complex opens in the charge unit its
        // report states it in; everything else in mg/kg.
        let unit_of = |tui: &Tui, nutrient: &str| {
            let batch = tui.batch.as_ref().expect("a table");
            batch.rows.iter().find(|row| row[0] == nutrient).expect("a row")[BATCH_COL_UNIT].clone()
        };
        for base in ["K", "Ca", "Mg", "Al"] {
            assert_eq!(unit_of(&tui, base), "cmolc_per_kg", "{base} is read as a charge");
        }
        for other in ["P", "S", "Zn"] {
            assert_eq!(unit_of(&tui, other), "mg_per_kg", "{other} is not");
        }

        // Two rows of twelve, and the unit still walks with h/l.
        set_cell(&mut tui, "P", BATCH_COL_VALUE, "14");
        set_cell(&mut tui, "P", BATCH_COL_METHOD, "Olsen");
        set_cell(&mut tui, "K", BATCH_COL_VALUE, "0.4");
        set_cell(&mut tui, "K", BATCH_COL_METHOD, "NH4OAc");
        {
            let batch = tui.batch.as_mut().expect("a table");
            batch.col = BATCH_COL_UNIT;
        }
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(unit_of(&tui, "K"), "mg_per_kg", "h/l overrides the suggestion");
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(unit_of(&tui, "K"), "cmolc_per_kg", "and comes back round");
        assert_eq!(tui.batch.as_ref().expect("a table").entries().len(), 2, "an untouched row is not a reading");

        // Enter commits from the last cell, and only from there.
        {
            let batch = tui.batch.as_mut().expect("a table");
            batch.row = batch.rows.len() - 1;
            batch.col = BATCH_COLUMNS.len() - 1;
        }
        press(&mut tui, KeyCode::Enter);
        assert!(!tui.is_error, "the panel should have saved: {}", tui.message);
        assert_eq!(tui.screen, Screen::Dashboard, "a committed table is done");

        // Only the two filled rows land; the ten empty ones add nothing.
        let written = std::fs::read_to_string(sandbox.root.join("data/curated/soil_tests.csv")).expect("curated file");
        let ours: Vec<&str> = written.lines().filter(|line| line.starts_with(&format!("{lot},"))).collect();
        assert_eq!(ours.len(), rows_before + 2, "an untouched row must not become a reading:\n{written}");
        assert!(ours.iter().any(|line| line.contains(",P,14,mg_per_kg,Olsen,")), "{ours:?}");
        assert!(
            ours.iter().any(|line| line.contains(",K,0.4,cmolc_per_kg,NH4OAc,")),
            "the unit on the cell has to survive the write: {ours:?}"
        );
    }

    /// Reopening the panel on a lot that already has a reading has to show
    /// it — not a blank row next to a cursor with nothing to land on. This
    /// was the actual bug behind "the method selector doesn't work": the
    /// panel always opened empty, so changing an already-answered
    /// nutrient's method looked broken because there was nothing on screen
    /// to say what it currently was.
    #[test]
    fn a_reopened_panel_shows_what_the_lot_already_carries() {
        let sandbox = Sandbox::new("batch_prefill");
        let mut tui = sandbox.tui();
        tui.screen = Screen::Dashboard;
        // The fixture's LOT-001 already carries a phosphorus reading.
        let field_id = tui.selected_lot().expect("a lot").field_id.clone();
        assert_eq!(field_id, "LOT-001");

        press(&mut tui, KeyCode::Char('b'));
        let row = |tui: &Tui, nutrient: &str| {
            tui.batch.as_ref().expect("a table").rows.iter().find(|row| row[0] == nutrient).expect("a row").clone()
        };

        assert_eq!(row(&tui, "P"), ["P", "18", "mg_per_kg", "Olsen", "20"], "the lot's own reading, not a blank one");
        assert_eq!(row(&tui, "Al")[BATCH_COL_VALUE], "", "a nutrient the lot has never reported stays blank");

        // The prefilled method still walks like any other: `e` replaces it
        // rather than appending to it.
        set_cell(&mut tui, "P", BATCH_COL_METHOD, "Mehlich");
        assert_eq!(row(&tui, "P")[BATCH_COL_METHOD], "Mehlich");
    }

    /// Esc is the way out of the table, and nothing typed into it is
    /// written on the way.
    #[test]
    fn esc_leaves_the_batch_table_without_writing_anything() {
        let sandbox = Sandbox::new("batch_esc");
        let mut tui = sandbox.tui();
        tui.screen = Screen::Dashboard;
        let before = std::fs::read_to_string(sandbox.root.join("data/curated/soil_tests.csv")).expect("curated file");

        press(&mut tui, KeyCode::Char('b'));
        set_cell(&mut tui, "P", BATCH_COL_VALUE, "99");
        press(&mut tui, KeyCode::Esc);

        assert_eq!(tui.screen, Screen::Dashboard);
        assert!(tui.batch.is_none(), "a half-filled panel is not a draft");
        let after = std::fs::read_to_string(sandbox.root.join("data/curated/soil_tests.csv")).expect("curated file");
        assert_eq!(before, after, "nothing is written until the table is committed");
    }

    /// Stages ④ and ⑤ are two halves of one result, and walking from one
    /// to the other used to pay for the whole calculation twice.
    #[test]
    fn walking_the_last_two_stages_reuses_the_plan_instead_of_recomputing_it() {
        let mut tui = tui();
        tui.open(Some(Screen::Sources));
        assert!(tui.plan.is_some(), "LOT-001/corn should plan: {}", tui.message);

        // A sentinel only a recalculation can erase.
        tui.plan.as_mut().expect("a plan").mineralization_factor = -1.0;
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(tui.screen, Screen::Plan);
        assert_eq!(
            tui.plan.as_ref().expect("a plan").mineralization_factor,
            -1.0,
            "stage 5 asked the question stage 4 had already answered"
        );

        // Enter on the last stage still means "run it again".
        press(&mut tui, KeyCode::Enter);
        assert_ne!(tui.plan.as_ref().expect("a plan").mineralization_factor, -1.0);

        // And a different question is a different plan.
        tui.plan.as_mut().expect("a plan").mineralization_factor = -1.0;
        tui.crop_override = Some("wheat".to_string());
        tui.yield_input = "6".to_string();
        tui.open(Some(Screen::Plan));
        assert_ne!(
            tui.plan.as_ref().expect("a plan").mineralization_factor,
            -1.0,
            "a changed crop must not be answered from the last crop's plan"
        );
    }

    /// A gap in the panel has to be named on stage ① — where it can still
    /// be filled — and not on stage ④, two stages after the reader walked
    /// past it. Informative, never blocking.
    #[test]
    fn stage_one_says_what_the_lab_panel_is_missing() {
        let sandbox = Sandbox::new("panel_gaps");
        let mut tui = sandbox.tui();

        tui.open(Some(Screen::NewLot));
        fill(&mut tui, "form_field_id", "LOT-901");
        fill(&mut tui, "form_texture", "loam");
        fill(&mut tui, "form_irrigation", "rainfed");
        fill(&mut tui, "form_om", "3.2");
        fill(&mut tui, "form_ph", "5.4");
        fill(&mut tui, "form_cec", "12");
        fill(&mut tui, "form_bulk_density", "1.3");
        fill(&mut tui, "form_arable_depth", "0.2");
        save_form(&mut tui);
        assert!(!tui.is_error, "the lot should have saved: {}", tui.message);

        let new_lot = tui.lots.iter().position(|lot| lot.field_id == "LOT-901").expect("new lot listed");
        let plan_this_lot = |tui: &mut Tui| {
            tui.lot_idx = new_lot;
            tui.crop_override = Some("wheat".to_string());
            tui.yield_input = "6".to_string();
        };

        // One reading, and it is not Al: the plan will run without a liming
        // recommendation and stage ① is where that can still be fixed.
        plan_this_lot(&mut tui);
        tui.open(Some(Screen::SampleBatch));
        tui.batch = Some(SampleBatch::new("LOT-901".to_string(), &[]));
        set_cell(&mut tui, "P", BATCH_COL_VALUE, "12");
        set_cell(&mut tui, "P", 3, "Olsen");
        tui.commit_batch();
        assert!(!tui.is_error, "the panel should have saved: {}", tui.message);

        plan_this_lot(&mut tui);
        tui.open(Some(Screen::Soil));
        assert!(tui.inspection.is_some(), "LOT-901 should inspect: {}", tui.message);
        assert!(tui.is_warning && !tui.is_error, "a missing reading is information, not a refusal");
        assert_eq!(tui.message, tui.i18n.t("warn_no_al_test"));

        // With Al on file but read as zero, the warning is the other one —
        // and it names the nutrient it is about.
        tui.open(Some(Screen::SampleBatch));
        tui.batch = Some(SampleBatch::new("LOT-901".to_string(), &[]));
        set_cell(&mut tui, "Al", BATCH_COL_VALUE, "0");
        set_cell(&mut tui, "Al", 3, "KCl");
        tui.commit_batch();
        assert!(!tui.is_error, "{}", tui.message);

        plan_this_lot(&mut tui);
        tui.open(Some(Screen::Soil));
        assert!(tui.is_warning && !tui.is_error, "{}", tui.message);
        assert!(tui.message.starts_with(tui.i18n.t("warn_zero_nutrient")), "{}", tui.message);
        assert!(tui.message.contains("Al"), "the warning has to name the reading: {}", tui.message);
    }

    #[test]
    fn step_saturates_at_both_ends() {
        assert_eq!(step(0, 3, -1), 0);
        assert_eq!(step(2, 3, 1), 2);
        assert_eq!(step(0, 0, 1), 0, "an empty list must not index out of bounds");
        assert_eq!(step(1, 3, 1), 2);
    }

    #[test]
    fn esc_leaves_a_screen_first_and_quits_only_from_the_dashboard() {
        let mut tui = tui();
        tui.open(Some(Screen::Crops));
        press(&mut tui, KeyCode::Esc);
        assert_eq!(tui.screen, Screen::Dashboard);
        assert!(tui.running);
        press(&mut tui, KeyCode::Esc);
        assert!(!tui.running);
    }

    #[test]
    fn filter_narrows_the_catalog_and_typing_never_moves_the_selection() {
        let mut tui = tui();
        tui.open(Some(Screen::Crops));
        press(&mut tui, KeyCode::Char('/'));
        for c in "corn".chars() {
            press(&mut tui, KeyCode::Char(c));
        }
        let matches = tui.filtered_crops();
        assert!(!matches.is_empty() && matches.len() < tui.crops.len());
        assert!(matches.iter().all(|c| c.crop_id.contains("corn") || c.name.to_lowercase().contains("corn")));
        assert_eq!(tui.crop_idx, 0);
    }

    /// Most catalog crops have no row in `yield_targets.csv`.
    #[test]
    fn a_crop_without_a_curated_yield_goal_asks_for_one_and_then_plans() {
        let mut tui = tui();
        tui.open(Some(Screen::Crops));
        press(&mut tui, KeyCode::Char('/'));
        for c in "wheat".chars() {
            press(&mut tui, KeyCode::Char(c));
        }
        press(&mut tui, KeyCode::Enter); // leave the filter
        press(&mut tui, KeyCode::Enter); // pick the crop

        assert_eq!(tui.crop_override.as_deref(), Some("wheat"));
        assert!(tui.editing_yield, "a crop with no curated goal must prompt for one");

        press(&mut tui, KeyCode::Enter); // empty input
        assert!(tui.is_error && tui.editing_yield, "an empty yield goal must be refused");
        for c in "0abc".chars() {
            press(&mut tui, KeyCode::Char(c));
        }
        assert_eq!(tui.yield_input, "0", "only digits and one separator may be typed");
        press(&mut tui, KeyCode::Enter);
        assert!(tui.is_error && tui.editing_yield, "a zero yield goal must be refused");

        for c in ".5".chars() {
            press(&mut tui, KeyCode::Char(c));
        }
        press(&mut tui, KeyCode::Enter);
        assert!(!tui.editing_yield && tui.screen == Screen::Target);

        let scenario = tui.scenario().expect("a lot is selected");
        assert_eq!(scenario.crop_id, "wheat");
        assert_eq!(scenario.yield_override.map(|target| target.value), Some(0.5));

        tui.open(Some(Screen::Plan));
        assert!(tui.plan.is_some(), "wheat should now plan: {}", tui.message);
    }

    /// The screenshot bug: pick a crop this lot has no goal for, decline
    /// to type one, walk on — and stage ④ answered with the repository's
    /// own words, `no yield target for field_id=… crop_id=…`, on a screen
    /// where nothing can be done about it.
    #[test]
    fn a_crop_with_no_goal_is_refused_in_the_apps_own_words_not_the_repositorys() {
        let mut tui = tui();
        tui.open(Some(Screen::Crops));
        press(&mut tui, KeyCode::Char('/'));
        for c in "wheat".chars() {
            press(&mut tui, KeyCode::Char(c));
        }
        press(&mut tui, KeyCode::Enter); // leave the filter
        press(&mut tui, KeyCode::Enter); // pick the crop
        assert!(tui.editing_yield, "a crop with no curated goal asks for one");

        // Declining is allowed — and used to poison every stage after it.
        press(&mut tui, KeyCode::Esc);
        assert!(!tui.editing_yield);
        assert!(tui.scenario().is_none(), "a pair with no goal is not a scenario");

        for stage in [Screen::Sources, Screen::Plan, Screen::Soil] {
            tui.open(Some(stage));
            assert!(tui.is_error, "{stage:?} planned without a goal");
            assert_eq!(
                tui.message,
                tui.i18n.t("err_no_yield_goal"),
                "{stage:?} answered with the repository's words instead of the app's"
            );
            assert!(!tui.message.contains("field_id"), "and leaked a column name at the reader");
        }

        // Typing one is all it ever needed.
        tui.yield_input = "5".to_string();
        tui.open(Some(Screen::Plan));
        assert!(tui.plan.is_some(), "{}", tui.message);
    }

    /// The pair the last session ended on is restored; a pair that cannot
    /// plan is not. The typed goal is deliberately not stored, so a crop
    /// without a curated one would come back unusable.
    #[test]
    fn a_stored_crop_is_only_restored_onto_a_lot_that_has_a_goal_for_it() {
        let stored = |lot: &str, crop: &str| tui_settings::TuiSettings {
            lot: lot.to_string(),
            crop: crop.to_string(),
            ..tui_settings::TuiSettings::default()
        };

        let restored = Tui::new(bootstrap::build_app_from_repo_data(), stored("LOT-001", "corn"), None);
        assert_eq!(restored.crop_override.as_deref(), Some("corn"), "LOT-001/corn is curated");
        assert!(restored.scenario().is_some());

        // LOT-002 is a coffee lot: corn on it is a pair with no goal.
        let impossible = Tui::new(bootstrap::build_app_from_repo_data(), stored("LOT-002", "corn"), None);
        assert!(impossible.crop_override.is_none(), "a pair that cannot plan must not be restored");
        assert!(impossible.scenario().is_some(), "and the lot's own crop still plans");
    }

    #[test]
    fn a_crop_with_a_curated_yield_goal_is_left_alone() {
        let mut tui = tui();
        tui.open(Some(Screen::Crops));
        press(&mut tui, KeyCode::Char('/'));
        for c in "corn".chars() {
            press(&mut tui, KeyCode::Char(c));
        }
        press(&mut tui, KeyCode::Enter);
        press(&mut tui, KeyCode::Enter);

        // LOT-001/corn is curated, so nothing is asked and nothing overrides.
        assert!(!tui.editing_yield);
        assert_eq!(tui.screen, Screen::Target, "picking a crop advances to the goal stage");
        assert!(tui.scenario().expect("a lot is selected").yield_override.is_none());
    }

    /// The stepper is a directed flow, and Enter on a lot enters it at the
    /// first stage rather than at the finished plan.
    #[test]
    fn the_stages_walk_in_order_and_stop_at_both_ends() {
        let mut tui = tui();
        tui.focus_modules = true;
        tui.activate();
        assert_eq!(tui.screen, STAGES[0].0, "Enter on a lot opens the flow at stage 1");
        assert!(!tui.focus_modules, "and hands focus to the workspace");

        press(&mut tui, KeyCode::Char('h'));
        assert_eq!(tui.screen, STAGES[0].0, "stage 1 has nowhere to go back to");
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(tui.screen, Screen::Crops);
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(tui.screen, Screen::Target);

        // The one exception, and the reason Enter walks the flow too: on
        // stage 3 `h`/`l` belong to the value being edited.
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(tui.screen, Screen::Target, "the goal stage keeps h/l for itself");
        press(&mut tui, KeyCode::Enter);
        assert_eq!(tui.screen, Screen::Sources);
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(tui.screen, Screen::Plan);
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(tui.screen, Screen::Plan, "the last stage stays put");
        press(&mut tui, KeyCode::Char('h'));
        assert_eq!(tui.screen, Screen::Sources, "and h walks back");
        // Esc leaves the flow; there is no Home mnemonic to press any
        // more, because the column is the lot list rather than a menu.
        press(&mut tui, KeyCode::Esc);
        assert_eq!(tui.screen, Screen::Dashboard);
        assert!(!tui.focus_modules, "and Home's cursor is its own menu, the only pane it draws");
    }

    /// Stage 3 edits the goal in place: a nudge starts from whatever the
    /// plan would otherwise have used, and the basis toggle is the CLI's
    /// `--demand-mode` by another route.
    #[test]
    fn the_goal_stage_nudges_the_curated_value_and_switches_basis() {
        let mut tui = tui();
        tui.open(Some(Screen::Target));
        let curated = tui.curated_yield_target().expect("LOT-001/corn is curated").value;

        // One decimal, the step's own resolution: comparing raw f64 sums
        // here would be asserting on binary rounding, not on the nudge.
        let goal = |tui: &Tui| format!("{:.1}", tui.typed_yield_target().expect("a goal").value);
        press(&mut tui, KeyCode::Char('l'));
        assert_eq!(goal(&tui), format!("{:.1}", curated + YIELD_STEP));
        press(&mut tui, KeyCode::Char('h'));
        assert_eq!(goal(&tui), format!("{curated:.1}"));

        assert_eq!(tui.scenario().expect("a scenario").demand_mode, NutrientDemandMode::Extraction);
        press(&mut tui, KeyCode::Char('m'));
        assert_eq!(tui.scenario().expect("a scenario").demand_mode, NutrientDemandMode::Absorption);
    }

    #[test]
    fn changing_lot_drops_the_crop_and_its_typed_yield_goal() {
        let mut tui = tui();
        tui.crop_override = Some("wheat".to_string());
        tui.yield_input = "6.5".to_string();
        tui.screen = Screen::Dashboard;
        // The lot list, not the launcher menu: on Home the workspace pane
        // is the menu, and its cursor moves between verbs, not between lots.
        tui.focus_modules = true;

        tui.move_selection(1);

        assert!(tui.crop_override.is_none() && tui.yield_input.is_empty());
    }

    /// Curate a lot, sample it, plan it — with a texture and irrigation
    /// system outside the curated efficiency grid, and an uncurated crop.
    #[test]
    fn a_lot_curated_in_the_tui_can_be_sampled_and_planned() {
        let sandbox = Sandbox::new("register");
        let mut tui = sandbox.tui();
        let lots_before = tui.lots.len();

        tui.open(Some(Screen::NewLot));
        fill(&mut tui, "form_field_id", "LOT-900");
        fill(&mut tui, "form_texture", "silty_clay");
        fill(&mut tui, "form_irrigation", "gravity");
        fill(&mut tui, "form_om", "3.8");
        fill(&mut tui, "form_ph", "5.5");
        fill(&mut tui, "form_cec", "15");
        fill(&mut tui, "form_bulk_density", "1.2");
        fill(&mut tui, "form_arable_depth", "0.2");
        save_form(&mut tui);

        assert!(!tui.is_error, "the lot should have saved: {}", tui.message);
        assert_eq!(tui.lots.len(), lots_before + 1, "the new lot must show up in the picker");
        let new_lot = tui.lots.iter().position(|lot| lot.field_id == "LOT-900").expect("new lot listed");
        assert!(tui.lots[new_lot].curated_targets.is_empty(), "a lot with no planning row is still a lot");

        // No soil sample yet: the plan must say so rather than invent one.
        tui.lot_idx = new_lot;
        tui.crop_override = Some("wheat".to_string());
        tui.yield_input = "6".to_string();
        tui.open(Some(Screen::Plan));
        assert!(tui.plan.is_none() && tui.is_error);

        tui.lot_idx = new_lot;
        tui.open(Some(Screen::NewSample));
        assert_eq!(
            tui.form.as_ref().expect("form").value("form_field_id"),
            "LOT-900",
            "the sample form must default to the selected lot"
        );
        pick(&mut tui, "form_nutrient", "P");
        fill(&mut tui, "form_value", "12");
        fill(&mut tui, "form_unit", "mg_per_kg");
        fill(&mut tui, "form_method", "Olsen");
        fill(&mut tui, "form_depth_to", "20");
        save_form(&mut tui);
        assert!(!tui.is_error, "the sample should have saved: {}", tui.message);

        tui.lot_idx = new_lot;
        tui.crop_override = Some("wheat".to_string());
        tui.yield_input = "6".to_string();
        tui.open(Some(Screen::Plan));
        assert!(tui.plan.is_some(), "silty_clay/gravity + wheat should plan: {}", tui.message);
    }

    /// Only identifiers, lab readings and coordinates are still typed.
    #[test]
    fn closed_set_fields_are_picked_and_open_ended_ones_are_typed() {
        let mut tui = tui();
        tui.open(Some(Screen::NewLot));
        let kinds: Vec<(&str, bool)> = tui
            .form
            .as_ref()
            .expect("form")
            .fields
            .iter()
            .map(|field| (field.label, field.is_choice()))
            .collect();

        for label in ["form_texture", "form_irrigation", "form_region", "form_crop", "form_yield_unit"] {
            assert!(kinds.contains(&(label, true)), "{label} must be a list");
        }
        for label in ["form_field_id", "form_om", "form_ph", "form_latitude", "form_longitude"] {
            assert!(kinds.contains(&(label, false)), "{label} must stay free text");
        }

        // A lab panel never reports N: it is derived from organic matter,
        // so offering it would accept a value the plan then ignores.
        tui.open(Some(Screen::NewSample));
        let nutrients = tui.form.as_ref().expect("form").fields.iter().find(|f| f.label == "form_nutrient");
        let nutrients = &nutrients.expect("nutrient field").options;
        assert!(nutrients.contains(&"P".to_string()) && nutrients.contains(&"Al".to_string()));
        assert!(!nutrients.contains(&"N".to_string()));
    }

    #[test]
    fn the_option_list_fills_the_field_on_enter_and_leaves_it_alone_on_esc() {
        let mut tui = tui();
        tui.open(Some(Screen::NewLot));
        go_to_field(&mut tui, "form_texture");

        press(&mut tui, KeyCode::Enter); // unfold
        assert!(tui.picker.is_some(), "a closed-set field must open a list, not an edit cursor");
        press(&mut tui, KeyCode::Char('j'));
        press(&mut tui, KeyCode::Char('j'));
        press(&mut tui, KeyCode::Enter);

        assert!(tui.picker.is_none());
        // Third entry of `Texture::ALL`, reached with two moves from the top.
        assert_eq!(tui.form.as_ref().expect("form").value("form_texture"), Texture::ALL[2].to_string());

        // Re-opening starts on the current value, and Esc discards.
        press(&mut tui, KeyCode::Enter);
        assert_eq!(tui.picker.as_ref().expect("picker").idx, 2, "the list must open on the current value");
        press(&mut tui, KeyCode::Char('j'));
        press(&mut tui, KeyCode::Esc);
        assert!(tui.picker.is_none());
        assert_eq!(tui.form.as_ref().expect("form").value("form_texture"), Texture::ALL[2].to_string());

        // A free-text field still types.
        go_to_field(&mut tui, "form_ph");
        press(&mut tui, KeyCode::Enter);
        assert!(tui.picker.is_none() && tui.form.as_ref().expect("form").editing);
    }

    #[test]
    fn an_invalid_lot_keeps_the_form_open_and_shows_the_reason() {
        let sandbox = Sandbox::new("invalid");
        let mut tui = sandbox.tui();

        tui.open(Some(Screen::NewLot));
        fill(&mut tui, "form_field_id", "LOT-001"); // already curated
        fill(&mut tui, "form_texture", "loam");
        fill(&mut tui, "form_irrigation", "rainfed");
        fill(&mut tui, "form_om", "3.2");
        fill(&mut tui, "form_ph", "6.3");
        fill(&mut tui, "form_cec", "12");
        fill(&mut tui, "form_bulk_density", "1.3");
        fill(&mut tui, "form_arable_depth", "0.2");
        save_form(&mut tui);

        assert!(tui.is_error && tui.message.contains("LOT-001"), "{}", tui.message);
        assert_eq!(tui.screen, Screen::NewLot, "a refused form stays open with the typed values");
        assert_eq!(tui.form.as_ref().expect("form").value("form_field_id"), "LOT-001");
    }

    /// A plan sees a climatology only once something else has cached it.
    #[test]
    fn a_plan_uses_a_prewarmed_climatology_and_runs_on_baseline_without_one() {
        use crate::core::domain::{services, AnnualClimatology};

        struct FakeProvider;
        impl AgroclimaticRepository for FakeProvider {
            fn fetch_climatology(&self, _latitude: f64, _longitude: f64) -> Result<AnnualClimatology, DomainError> {
                // ET0 as well as rain: the mineralization model reads the
                // aridity index, not raw millimetres, so a rainfed lot
                // needs both halves of the water balance. `NasaPowerRepo`
                // derives ET0 for every real grid cell.
                Ok(AnnualClimatology {
                    mean_temp_c: Some(13.2),
                    precip_mm_per_day: Some(3.0),
                    et0_mm_per_day: Some(4.1),
                    ..Default::default()
                })
            }
        }

        let mut tui = tui();
        let lot = tui.lots.first().expect("a curated lot").clone();

        // Cold: no cache at all, so the plan is baseline and says so.
        tui.open(Some(Screen::Plan));
        let cold = tui.plan.as_ref().expect("a plan without climate");
        assert!(cold.climate.is_none());
        assert_eq!(cold.mineralization_factor, services::BASELINE_MINERALIZATION_FACTOR);

        // Warm: whatever the background thread would have done, done here
        // synchronously so the test never races it.
        let cache = Arc::new(CachedAgroclimaticRepo::new(Box::new(FakeProvider)));
        cache
            .fetch_climatology(lot.latitude.expect("lat"), lot.longitude.expect("lon"))
            .expect("prewarm");
        tui.climate = Some(cache);

        tui.open(Some(Screen::Plan));
        let warm = tui.plan.as_ref().expect("a plan with climate");
        assert_eq!(warm.climate.as_ref().and_then(|c| c.mean_temp_c), Some(13.2));
        assert_ne!(
            warm.mineralization_factor,
            services::BASELINE_MINERALIZATION_FACTOR,
            "a climatology must actually move the factor"
        );
    }

    #[test]
    fn language_toggle_swaps_the_bundle_without_touching_the_data() {
        let mut tui = tui();
        let english = tui.i18n.t("action_settings").to_string();
        tui.screen = Screen::Settings;
        tui.focus_modules = false;
        tui.setting_idx = 0;
        tui.change_setting(1);
        assert_ne!(tui.i18n.t("action_settings"), english);
        assert_eq!(tui.i18n.language(), Language::Spanish);
    }
}
