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

use crate::core::application::{FertilityScenario, ScenarioInspection};
use crate::core::domain::{Crop, DomainError, FertilityPlan};
use crate::core::ports::{FertilityCalculatorPort, ListCropsPort};
use crate::infra::bootstrap::{self, App as Composition, LotRow};

use i18n::{I18n, Language};
use theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Plan,
    Crops,
    Inspect,
    Settings,
}

/// Left navigation column: label id, mnemonic, target screen (`None` quits).
const MODULES: [(&str, char, Option<Screen>); 6] = [
    ("module_home", 'h', Some(Screen::Dashboard)),
    ("module_plan", 'f', Some(Screen::Plan)),
    ("module_crops", 'c', Some(Screen::Crops)),
    ("module_inspect", 'i', Some(Screen::Inspect)),
    ("module_settings", ',', Some(Screen::Settings)),
    ("module_quit", 'q', None),
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

pub struct Tui {
    cfg: Composition,
    i18n: I18n,
    theme: &'static Theme,
    screen: Screen,
    /// Which of the two navigable panes has focus (Tab toggles).
    focus_modules: bool,
    module_idx: usize,
    profiles: Vec<String>,
    lots: Vec<LotRow>,
    lot_idx: usize,
    crops: Vec<Crop>,
    crop_idx: usize,
    /// Crop picked from the catalog, overriding the lot's curated crop.
    crop_override: Option<String>,
    filter: String,
    filtering: bool,
    plan: Option<FertilityPlan>,
    inspection: Option<ScenarioInspection>,
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
            filter: String::new(),
            filtering: false,
            plan: None,
            inspection: None,
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
        self.plan = None;
        self.inspection = None;
        self.crop_override = None;
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
        match self.cfg.lots() {
            Ok(lots) => self.lots = lots,
            Err(e) => {
                self.lots.clear();
                self.fail(e);
            }
        }
    }

    fn scenario(&self) -> Option<FertilityScenario> {
        let lot = self.lots.get(self.lot_idx)?;
        Some(FertilityScenario {
            sample_id: lot.field_id.clone(),
            field_id: lot.field_id.clone(),
            crop_id: self.crop_override.clone().unwrap_or_else(|| lot.crop_id.clone()),
            // ponytail: same placeholder the CLI defaults to; the real
            // harvested organ per crop is an open item in docs/HANDOFF.md.
            product: "grain".to_string(),
            // None on purpose: falls back to the curated yield target, which
            // is what the lot row on screen is showing.
            yield_override: None,
        })
    }

    fn run_plan(&mut self) {
        self.screen = Screen::Plan;
        self.focus_modules = false;
        self.scroll = 0;
        let Some(scenario) = self.scenario() else {
            self.plan = None;
            return self.fail_key("err_no_lot");
        };
        match bootstrap::build_calculate_fertility_plan(&self.cfg.layout()).and_then(|uc| uc.calculate(scenario)) {
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
            return self.fail_key("err_no_lot");
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
                    // A different lot invalidates a crop picked for the previous one.
                    self.crop_override = None;
                }
                self.lot_idx = next;
            }
            Screen::Crops => self.crop_idx = step(self.crop_idx, self.filtered_crops().len(), delta),
            Screen::Settings => self.setting_idx = step(self.setting_idx, SETTINGS.len(), delta),
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
                if let Some(crop) = self.filtered_crops().get(self.crop_idx) {
                    self.crop_override = Some(crop.crop_id.clone());
                    self.screen = Screen::Dashboard;
                    self.info("msg_crop_selected");
                }
            }
            Screen::Settings => self.change_setting(1),
            Screen::Plan | Screen::Inspect => {}
        }
    }

    fn open(&mut self, target: Option<Screen>) {
        match target {
            None => self.running = false,
            Some(Screen::Plan) => self.run_plan(),
            Some(Screen::Inspect) => self.run_inspect(),
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
