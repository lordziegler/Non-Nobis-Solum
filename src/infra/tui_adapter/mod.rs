//! Terminal front-end. Same ports and use cases as `cli_adapter`, only the
//! presentation differs: a tiling workspace (module column · workspace ·
//! status column) with a modal statusline, keyboard-first.
//!
//! Hexagonal boundary notes:
//!
//! - Every calculation goes through the input ports (`FertilityCalculatorPort`,
//!   `ListCropsPort`) and every path comes from `bootstrap`.
//! - Domain types *are* imported, but only the ones those port signatures
//!   already hand back (`FertilityPlan`, `Crop`, …): rendering a port's
//!   return value requires naming its type. No domain service, constructor
//!   or agronomic rule is used here.
//! - TODO(gap): `InspectScenario` has no trait in `core::ports::input`, so
//!   the inspect screen calls its inherent `inspect()` method. Add
//!   `InspectScenarioPort` there and this call site becomes port-only like
//!   the other two.

pub mod i18n;
pub mod theme;
mod ui;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::core::application::{FertilityScenario, LotRegistration, LotSummary, ScenarioInspection, SoilTestEntry};
use crate::core::domain::{Crop, DomainError, FertilityPlan, YieldTarget};
use crate::core::ports::{FertilityCalculatorPort, ListCropsPort, ListLotsPort, RegisterLotPort};
use crate::infra::bootstrap::{self, App as Composition};

use i18n::{I18n, Language};
use theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Plan,
    Crops,
    Inspect,
    NewLot,
    NewSample,
    Settings,
}

/// Left navigation column: label id, mnemonic, target screen (`None` quits).
const MODULES: [(&str, char, Option<Screen>); 8] = [
    ("module_home", 'h', Some(Screen::Dashboard)),
    ("module_plan", 'f', Some(Screen::Plan)),
    ("module_crops", 'c', Some(Screen::Crops)),
    ("module_inspect", 'i', Some(Screen::Inspect)),
    ("module_new_lot", 'n', Some(Screen::NewLot)),
    ("module_new_sample", 's', Some(Screen::NewSample)),
    ("module_settings", ',', Some(Screen::Settings)),
    ("module_quit", 'q', None),
];

/// The "add lot" form, in `LotRegistration` field order. Label ids double
/// as the mapping to the registration struct — see `Form::registration`.
const NEW_LOT_FIELDS: [&str; 14] = [
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
    "form_crop",
    "form_yield_value",
    "form_yield_unit",
];

/// The "add sample" form: one lab result for an existing lot.
const NEW_SAMPLE_FIELDS: [&str; 7] = [
    "form_field_id",
    "form_nutrient",
    "form_value",
    "form_unit",
    "form_method",
    "form_depth_from",
    "form_depth_to",
];

const SETTINGS: [&str; 5] = [
    "settings_language",
    "settings_profile",
    "settings_data_root",
    "settings_reference_dir",
    "settings_curated_dir",
];

/// Micronutrients with reference data but no use-case wiring yet: shown
/// muted on the inspect screen instead of being silently dropped.
const UNPLANNED_MICRONUTRIENTS: [&str; 6] = ["Fe", "Mn", "Zn", "Cu", "B", "Mo"];

/// Unit a yield goal typed into the TUI is expressed in. Every row of
/// every shipped `nutrient_removal.csv` is `t_ha`, and the removal
/// repository rejects a mismatch with its own error, so this is a
/// constant rather than one more field to fill in.
/// ponytail: promote to a picker if a reference profile ever ships a crop
/// measured in something else.
const YIELD_UNIT: &str = "t_ha";

/// A data-entry form: a fixed list of labelled text fields plus a trailing
/// "save" row. Every value stays raw text all the way to `RegisterLot`,
/// which is the only thing allowed to decide whether it is valid.
pub struct Form {
    screen: Screen,
    fields: Vec<(&'static str, String)>,
    idx: usize,
    editing: bool,
}

impl Form {
    fn new(screen: Screen, labels: &[&'static str], prefill: &[(&str, String)]) -> Self {
        let fields = labels
            .iter()
            .map(|label| {
                let value = prefill
                    .iter()
                    .find(|(id, _)| id == label)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default();
                (*label, value)
            })
            .collect();
        Self { screen, fields, idx: 0, editing: false }
    }

    /// One past the last field: the row that submits.
    fn save_row(&self) -> usize {
        self.fields.len()
    }

    fn value(&self, label: &str) -> String {
        self.fields
            .iter()
            .find(|(id, _)| *id == label)
            .map(|(_, value)| value.clone())
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

pub struct Tui {
    cfg: Composition,
    i18n: I18n,
    theme: &'static Theme,
    screen: Screen,
    /// Which of the two navigable panes has focus (Tab toggles).
    focus_modules: bool,
    module_idx: usize,
    profiles: Vec<String>,
    lots: Vec<LotSummary>,
    lot_idx: usize,
    crops: Vec<Crop>,
    crop_idx: usize,
    /// Crop picked from the catalog, overriding the lot's curated crop.
    crop_override: Option<String>,
    /// Yield goal typed for the picked crop, as raw text. Only ever set
    /// for the currently selected (lot, crop) pair — both selections clear
    /// it — so a stale number can never leak into another scenario.
    yield_input: String,
    editing_yield: bool,
    filter: String,
    filtering: bool,
    plan: Option<FertilityPlan>,
    inspection: Option<ScenarioInspection>,
    /// The form currently on screen, if any. Rebuilt each time a form
    /// screen is opened, so a half-typed lot never survives a detour.
    form: Option<Form>,
    scroll: u16,
    setting_idx: usize,
    message: String,
    is_error: bool,
    help: bool,
    running: bool,
}

pub fn run(cfg: Composition) -> Result<(), DomainError> {
    // Queried before `ratatui::init()` takes the tty into raw mode and the
    // alternate screen — the OSC handshake needs the plain terminal.
    let theme = theme::detect();
    let mut tui = Tui::new(cfg, theme);

    let mut terminal = ratatui::init();
    let result = tui.event_loop(&mut terminal);
    ratatui::restore();
    result
}

impl Tui {
    fn new(cfg: Composition, theme: &'static Theme) -> Self {
        let mut tui = Self {
            profiles: cfg.profiles(),
            cfg,
            i18n: I18n::new(Language::English),
            theme,
            screen: Screen::Dashboard,
            focus_modules: true,
            module_idx: 0,
            lots: Vec::new(),
            lot_idx: 0,
            crops: Vec::new(),
            crop_idx: 0,
            crop_override: None,
            yield_input: String::new(),
            editing_yield: false,
            filter: String::new(),
            filtering: false,
            plan: None,
            inspection: None,
            form: None,
            scroll: 0,
            setting_idx: 0,
            message: String::new(),
            is_error: false,
            help: false,
            running: true,
        };
        tui.reload();
        if !tui.is_error {
            tui.info("msg_ready");
        }
        tui
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

    /// Reloads everything that depends on the selected profile. Called at
    /// startup and whenever the profile changes in Settings.
    fn reload(&mut self) {
        // Whatever went wrong before, it was about data that is being
        // re-read right now; callers check `is_error` afterwards to decide
        // whether the reload itself failed.
        self.is_error = false;
        self.plan = None;
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
        match bootstrap::build_list_lots(&self.cfg.layout()).list_lots() {
            Ok(lots) => self.lots = lots,
            Err(e) => {
                self.lots.clear();
                self.fail(e);
            }
        }
    }

    /// Drops both the picked crop and the yield goal typed for it. They
    /// are a pair: a yield goal only ever means something next to the crop
    /// it was typed for.
    fn clear_crop_choice(&mut self) {
        self.crop_override = None;
        self.yield_input.clear();
        self.editing_yield = false;
    }

    fn selected_lot(&self) -> Option<&LotSummary> {
        self.lots.get(self.lot_idx)
    }

    /// Whether `data/curated/yield_targets.csv` already has a goal for the
    /// selected lot and the given crop. Answered from the summaries already
    /// in memory — no IO, same rows the plan would consult.
    fn has_curated_yield_target(&self, crop_id: &str) -> bool {
        self.selected_lot().and_then(|lot| lot.target_for(crop_id)).is_some()
    }

    /// The typed yield goal, if it is a usable number. Anything else is
    /// treated as "not entered", so the plan falls back to the curated
    /// target and reports its own error if there is none.
    fn typed_yield_target(&self) -> Option<YieldTarget> {
        let value = self.yield_input.trim().parse::<f64>().ok()?;
        (value.is_finite() && value > 0.0).then(|| YieldTarget { value, unit: YIELD_UNIT.to_string() })
    }

    /// The crop the next plan would use: the one picked from the catalog,
    /// or the lot's curated one. `None` for a lot that has neither — a
    /// registered lot with no planning row yet.
    fn active_crop(&self) -> Option<String> {
        self.crop_override
            .clone()
            .or_else(|| self.selected_lot()?.default_crop().map(str::to_string))
    }

    fn scenario(&self) -> Option<FertilityScenario> {
        let lot = self.selected_lot()?;
        Some(FertilityScenario {
            sample_id: lot.field_id.clone(),
            field_id: lot.field_id.clone(),
            crop_id: self.active_crop()?,
            // ponytail: same placeholder the CLI defaults to; the real
            // harvested organ per crop is an open item in docs/HANDOFF.md.
            product: "grain".to_string(),
            // `None` falls back to the curated yield target, which is what
            // the lot row on screen shows. A typed goal is what unblocks
            // the 64 catalog crops that have no curated row at all.
            yield_override: self.typed_yield_target(),
        })
    }

    fn run_plan(&mut self) {
        self.screen = Screen::Plan;
        self.focus_modules = false;
        self.scroll = 0;
        let Some(scenario) = self.scenario() else {
            self.plan = None;
            return self.fail_missing_scenario();
        };
        // TODO(gap): no climate enrichment in the TUI. The fetch is
        // blocking with a 10 s timeout and this is a single-threaded
        // render loop, so wiring it here would freeze the UI on every
        // plan. Needs a background fetch (or a pre-warmed cache) first —
        // the CLI has it today.
        match bootstrap::build_calculate_fertility_plan(&self.cfg.layout(), None).and_then(|uc| uc.calculate(scenario)) {
            Ok(plan) => {
                self.plan = Some(plan);
                self.info("msg_plan_done");
            }
            // Includes the efficiency-rules texture gap: the port's own
            // message names the texture that has no rule, so it is shown
            // verbatim instead of a generic "could not plan".
            Err(e) => {
                self.plan = None;
                self.fail(e);
            }
        }
    }

    fn run_inspect(&mut self) {
        self.screen = Screen::Inspect;
        self.focus_modules = false;
        self.scroll = 0;
        let Some(scenario) = self.scenario() else {
            self.inspection = None;
            return self.fail_missing_scenario();
        };
        match bootstrap::build_inspect_scenario(&self.cfg.layout()).and_then(|uc| uc.inspect(&scenario)) {
            Ok(inspection) => {
                self.inspection = Some(inspection);
                self.info("msg_inspect_done");
            }
            Err(e) => {
                self.inspection = None;
                self.fail(e);
            }
        }
    }

    /// A scenario needs a lot *and* a crop; say which one is missing.
    fn fail_missing_scenario(&mut self) {
        let id = if self.selected_lot().is_none() { "err_no_lot" } else { "err_no_crop" };
        self.fail_key(id);
    }

    // ---- curating new data -----------------------------------------------

    fn open_form(&mut self, screen: Screen) {
        self.screen = screen;
        self.focus_modules = false;
        let form = match screen {
            Screen::NewSample => Form::new(
                screen,
                &NEW_SAMPLE_FIELDS,
                &[
                    ("form_field_id", self.selected_lot().map(|lot| lot.field_id.clone()).unwrap_or_default()),
                    ("form_depth_from", "0".to_string()),
                ],
            ),
            // Region is prefilled with the active profile: the reference
            // tables a plan reads are the profile's, so a lot curated from
            // here defaults to matching them.
            _ => Form::new(
                screen,
                &NEW_LOT_FIELDS,
                &[
                    ("form_region", self.cfg.profile.clone()),
                    ("form_yield_unit", YIELD_UNIT.to_string()),
                ],
            ),
        };
        self.form = Some(form);
    }

    /// Hands the typed text to `RegisterLot`, which is where it is parsed,
    /// validated and — only then — written. Every rejection lands in the
    /// status bar with the port's own message.
    fn submit_form(&mut self) {
        let Some(form) = &self.form else { return };
        let use_case = bootstrap::build_register_lot(&self.cfg.layout());
        let (outcome, message) = match form.screen {
            Screen::NewSample => (
                use_case.add_soil_tests(&form.value("form_field_id"), &[form.soil_test_entry()]),
                "msg_sample_saved",
            ),
            _ => (use_case.register_lot(&form.registration()), "msg_lot_saved"),
        };

        match outcome {
            Ok(()) => {
                self.form = None;
                self.screen = Screen::Dashboard;
                self.focus_modules = true;
                self.reload();
                if !self.is_error {
                    self.info(message);
                }
            }
            Err(e) => self.fail(e),
        }
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
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            // Raw mode swallows SIGINT, so Ctrl-C has to quit explicitly —
            // and no chorded key may fall through as plain text input.
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
        if self.form.as_ref().is_some_and(|form| form.editing) {
            return self.on_form_edit_key(code);
        }
        let in_settings = self.screen == Screen::Settings && !self.focus_modules;
        match code {
            KeyCode::Char('?') => self.help = true,
            KeyCode::Tab | KeyCode::BackTab => self.focus_modules = !self.focus_modules,
            KeyCode::Enter => self.activate(),
            KeyCode::Esc => self.back(),
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('/') if self.screen == Screen::Crops => {
                self.filtering = true;
                self.focus_modules = false;
            }
            KeyCode::Char('h') | KeyCode::Left if in_settings => self.change_setting(-1),
            KeyCode::Char('l') | KeyCode::Right if in_settings => self.change_setting(1),
            KeyCode::Char('q') => self.back(),
            KeyCode::Char(c) if self.focus_modules => self.mnemonic(c),
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

    /// Numeric entry for a yield goal. Only digits and one decimal
    /// separator get in, so the field can't hold something the plan would
    /// choke on later.
    fn on_yield_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.yield_input.clear();
                self.editing_yield = false;
            }
            KeyCode::Enter => match self.typed_yield_target() {
                Some(_) => {
                    self.editing_yield = false;
                    self.screen = Screen::Dashboard;
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

    /// Free text: any value the domain can parse is typed here verbatim,
    /// including a texture name or a negative coordinate.
    fn on_form_edit_key(&mut self, code: KeyCode) {
        let Some(form) = &mut self.form else { return };
        let Some((_, value)) = form.fields.get_mut(form.idx) else {
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

    fn mnemonic(&mut self, pressed: char) {
        if let Some((idx, module)) = MODULES.iter().enumerate().find(|(_, m)| m.1 == pressed) {
            self.module_idx = idx;
            self.open(module.2);
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.focus_modules {
            self.module_idx = step(self.module_idx, MODULES.len(), delta);
            return;
        }
        match self.screen {
            Screen::Dashboard => {
                let next = step(self.lot_idx, self.lots.len(), delta);
                if next != self.lot_idx {
                    // A different lot invalidates a crop picked for the
                    // previous one, and the yield goal typed alongside it.
                    self.clear_crop_choice();
                }
                self.lot_idx = next;
            }
            Screen::Crops => self.crop_idx = step(self.crop_idx, self.filtered_crops().len(), delta),
            Screen::Settings => self.setting_idx = step(self.setting_idx, SETTINGS.len(), delta),
            // One row past the last field: the save row.
            Screen::NewLot | Screen::NewSample => {
                if let Some(form) = &mut self.form {
                    form.idx = step(form.idx, form.fields.len() + 1, delta);
                }
            }
            // Plan and Inspect are read-only text: j/k scrolls them.
            Screen::Plan | Screen::Inspect => {
                self.scroll = if delta < 0 { self.scroll.saturating_sub(1) } else { self.scroll.saturating_add(1) }
            }
        }
    }

    fn activate(&mut self) {
        if self.focus_modules {
            return self.open(MODULES[self.module_idx].2);
        }
        match self.screen {
            Screen::Dashboard => self.run_plan(),
            Screen::Crops => {
                let Some(crop_id) = self.filtered_crops().get(self.crop_idx).map(|crop| crop.crop_id.clone()) else {
                    return;
                };
                self.clear_crop_choice();
                let curated = self.has_curated_yield_target(&crop_id);
                self.crop_override = Some(crop_id);
                if curated {
                    self.screen = Screen::Dashboard;
                    self.info("msg_crop_selected");
                } else {
                    // 64 of the 66 catalog crops land here: no curated
                    // yield goal exists, so ask for one instead of letting
                    // the plan fail on a missing row.
                    self.editing_yield = true;
                    self.info("msg_yield_needed");
                }
            }
            Screen::Settings => self.change_setting(1),
            Screen::NewLot | Screen::NewSample => match &mut self.form {
                Some(form) if form.idx == form.save_row() => self.submit_form(),
                Some(form) => form.editing = true,
                None => {}
            },
            Screen::Plan | Screen::Inspect => {}
        }
    }

    fn open(&mut self, target: Option<Screen>) {
        match target {
            None => self.running = false,
            Some(Screen::Plan) => self.run_plan(),
            Some(Screen::Inspect) => self.run_inspect(),
            Some(screen @ (Screen::NewLot | Screen::NewSample)) => self.open_form(screen),
            Some(screen) => {
                self.screen = screen;
                self.focus_modules = screen == Screen::Dashboard;
                self.scroll = 0;
            }
        }
    }

    fn back(&mut self) {
        if self.screen == Screen::Dashboard {
            self.running = false;
        } else {
            // Leaving a form discards it: a half-typed lot is not a draft.
            self.form = None;
            self.screen = Screen::Dashboard;
            self.focus_modules = true;
            self.module_idx = 0;
        }
    }

    fn change_setting(&mut self, delta: isize) {
        match SETTINGS[self.setting_idx] {
            "settings_language" => {
                self.i18n = I18n::new(self.i18n.language().toggled());
                self.info("msg_language_changed");
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
            // The remaining rows are read-only paths.
            _ => {}
        }
    }

    // ---- status bar ------------------------------------------------------

    fn info(&mut self, id: &str) {
        self.message = self.i18n.t(id).to_string();
        self.is_error = false;
    }

    fn fail(&mut self, error: DomainError) {
        self.message = error.to_string();
        self.is_error = true;
    }

    fn fail_key(&mut self, id: &str) {
        self.message = self.i18n.t(id).to_string();
        self.is_error = true;
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

    fn tui() -> Tui {
        Tui::new(bootstrap::build_app(), &theme::DARK_THEME)
    }

    fn press(tui: &mut Tui, code: KeyCode) {
        tui.on_key(KeyEvent::from(code));
    }

    /// A disposable copy of `data/`, so the write tests exercise the real
    /// adapters without touching the curated files under version control.
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
            Tui::new(cfg, &theme::DARK_THEME)
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

    /// Types a value into a form field. Navigation and submission still go
    /// through the real key handling — only the typing is shortcut.
    fn fill(tui: &mut Tui, label: &str, value: &str) {
        let form = tui.form.as_mut().expect("a form is on screen");
        let field = form
            .fields
            .iter_mut()
            .find(|(id, _)| *id == label)
            .unwrap_or_else(|| panic!("no field {label}"));
        field.1 = value.to_string();
    }

    fn save_form(tui: &mut Tui) {
        let save_row = tui.form.as_ref().expect("a form is on screen").save_row();
        for _ in 0..save_row {
            press(tui, KeyCode::Char('j'));
        }
        press(tui, KeyCode::Enter);
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

    /// The blocker this unlocks: 64 of the 66 catalog crops have no row in
    /// `yield_targets.csv`, so picking one used to guarantee a failed plan.
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
        assert!(!tui.editing_yield && tui.screen == Screen::Dashboard);

        let scenario = tui.scenario().expect("a lot is selected");
        assert_eq!(scenario.crop_id, "wheat");
        assert_eq!(scenario.yield_override.map(|target| target.value), Some(0.5));

        tui.run_plan();
        assert!(tui.plan.is_some(), "wheat should now plan: {}", tui.message);
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
        assert_eq!(tui.screen, Screen::Dashboard);
        assert!(tui.scenario().expect("a lot is selected").yield_override.is_none());
    }

    #[test]
    fn changing_lot_drops_the_crop_and_its_typed_yield_goal() {
        let mut tui = tui();
        tui.crop_override = Some("wheat".to_string());
        tui.yield_input = "6.5".to_string();
        tui.screen = Screen::Dashboard;
        tui.focus_modules = false;

        tui.move_selection(1);

        assert!(tui.crop_override.is_none() && tui.yield_input.is_empty());
    }

    /// The whole chain phases 1-3 exist for: curate a lot the shipped data
    /// never had, give it a sample, and plan it — with a texture and an
    /// irrigation system outside the curated efficiency grid, and a crop
    /// with no curated yield goal.
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
        tui.run_plan();
        assert!(tui.plan.is_none() && tui.is_error);

        tui.lot_idx = new_lot;
        tui.open(Some(Screen::NewSample));
        assert_eq!(
            tui.form.as_ref().expect("form").value("form_field_id"),
            "LOT-900",
            "the sample form must default to the selected lot"
        );
        fill(&mut tui, "form_nutrient", "P");
        fill(&mut tui, "form_value", "12");
        fill(&mut tui, "form_unit", "mg_per_kg");
        fill(&mut tui, "form_method", "Olsen");
        fill(&mut tui, "form_depth_to", "20");
        save_form(&mut tui);
        assert!(!tui.is_error, "the sample should have saved: {}", tui.message);

        tui.lot_idx = new_lot;
        tui.crop_override = Some("wheat".to_string());
        tui.yield_input = "6".to_string();
        tui.run_plan();
        assert!(tui.plan.is_some(), "silty_clay/gravity + wheat should plan: {}", tui.message);
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

    #[test]
    fn language_toggle_swaps_the_bundle_without_touching_the_data() {
        let mut tui = tui();
        let english = tui.i18n.t("module_settings").to_string();
        tui.screen = Screen::Settings;
        tui.focus_modules = false;
        tui.setting_idx = 0;
        tui.change_setting(1);
        assert_ne!(tui.i18n.t("module_settings"), english);
        assert_eq!(tui.i18n.language(), Language::Spanish);
    }
}
