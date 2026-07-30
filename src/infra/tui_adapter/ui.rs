//! Rendering. Layout follows the "Estrato" direction from
//! `docs/Prototypes/`: context bar on top, fixed module column on the
//! left, workspace in the middle, status column on the right, modal
//! statusline at the bottom. Every label goes through `tui.i18n`.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

use super::i18n::Language;
use super::{Screen, Tui, MODULES, SETTINGS, UNPLANNED_MICRONUTRIENTS};
use crate::core::domain::SoilStatus;

/// Below this width the status column is dropped rather than squeezed —
/// an 80x24 terminal keeps modules + workspace intact.
const NARROW: u16 = 92;

pub fn draw(frame: &mut Frame, tui: &Tui) {
    let [top, body, bottom] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    context_bar(frame, top, tui);
    statusline(frame, bottom, tui);

    let columns = if body.width < NARROW {
        vec![Constraint::Length(20), Constraint::Min(0)]
    } else {
        vec![Constraint::Length(22), Constraint::Min(0), Constraint::Length(32)]
    };
    let panes = Layout::horizontal(columns).split(body);
    modules_pane(frame, panes[0], tui);
    workspace(frame, panes[1], tui);
    if let Some(area) = panes.get(2) {
        status_pane(frame, *area, tui);
    }

    if tui.help {
        help_overlay(frame, tui);
    }
}

// ---- chrome --------------------------------------------------------------

fn panel<'a>(title: String, focused: bool, tui: &Tui) -> Block<'a> {
    let style = if focused { tui.theme.title() } else { tui.theme.muted() };
    Block::bordered()
        .border_type(if focused { BorderType::Thick } else { BorderType::Plain })
        .border_style(if focused { tui.theme.accent() } else { tui.theme.muted() })
        .title(Line::from(format!(" {title} ")).style(style))
}

fn context_bar(frame: &mut Frame, area: Rect, tui: &Tui) {
    let mode = if tui.help {
        "mode_help"
    } else if tui.filtering {
        "mode_filter"
    } else if tui.editing_yield {
        "mode_yield"
    } else {
        "mode_nav"
    };
    let lot = tui.lots.get(tui.lot_idx);
    let mut spans = vec![
        Span::styled(format!(" {} ", tui.i18n.t(mode)), tui.theme.selected()),
        Span::raw(" non·nobis·solum "),
        Span::styled(format!("· {} ", tui.i18n.t("app_subtitle")), tui.theme.muted()),
        Span::styled(format!("· {} {} ", tui.i18n.t("st_profile"), tui.cfg.profile), tui.theme.accent()),
    ];
    if let Some(lot) = lot {
        spans.push(Span::raw(format!("· {} {} ", tui.i18n.t("st_lot"), lot.field_id)));
        spans.push(Span::raw(format!("· {} ", crop_of(tui, lot))));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn statusline(frame: &mut Frame, area: Rect, tui: &Tui) {
    if tui.editing_yield {
        return statusline_with(frame, area, tui, "hint_yield");
    }
    let hint = match tui.screen {
        Screen::Dashboard => "hint_dashboard",
        Screen::Plan => "hint_plan",
        Screen::Crops => "hint_crops",
        Screen::Inspect => "hint_inspect",
        Screen::Settings => "hint_settings",
    };
    statusline_with(frame, area, tui, hint);
}

fn statusline_with(frame: &mut Frame, area: Rect, tui: &Tui, hint: &str) {
    let left = Line::from(vec![
        Span::styled(format!(" {} ", screen_title(tui)), tui.theme.accent()),
        Span::styled(tui.i18n.t(hint).to_string(), tui.theme.muted()),
    ]);
    let message = Span::styled(
        format!("{} ", tui.message),
        if tui.is_error { tui.theme.error() } else { tui.theme.ok() },
    );

    let width = (tui.message.chars().count() + 1).min(area.width as usize) as u16;
    let [left_area, right_area] = Layout::horizontal([Constraint::Min(0), Constraint::Length(width)]).areas(area);
    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(Line::from(message)).right_aligned(), right_area);
}

fn modules_pane(frame: &mut Frame, area: Rect, tui: &Tui) {
    let inner = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = MODULES
        .iter()
        .map(|(label, mnemonic, _)| {
            let label = format!(" {}", tui.i18n.t(label));
            let key = format!("{mnemonic} ");
            let gap = inner.saturating_sub(label.chars().count() + key.chars().count());
            ListItem::new(Line::from(vec![
                Span::raw(label),
                Span::raw(" ".repeat(gap)),
                Span::styled(key, tui.theme.muted()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(panel(tui.i18n.t("modules").to_string(), tui.focus_modules, tui))
        .highlight_style(if tui.focus_modules { tui.theme.selected() } else { tui.theme.accent() });
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(Some(tui.module_idx)));
}

fn status_pane(frame: &mut Frame, area: Rect, tui: &Tui) {
    let mut lines = vec![
        field(tui, "st_profile", tui.cfg.profile.clone()),
        field(tui, "st_crops", tui.crops.len().to_string()),
    ];
    if let Some(lot) = tui.lots.get(tui.lot_idx) {
        lines.push(field(tui, "st_lot", lot.field_id.clone()));
        lines.push(field(tui, "col_crop", crop_of(tui, lot)));
        lines.push(field(tui, "col_yield", format!("{} {}", lot.yield_value, lot.yield_unit)));
    }
    if let Some(inspection) = &tui.inspection {
        let context = &inspection.field_context;
        lines.push(Line::raw(""));
        lines.push(field(tui, "st_texture", context.texture.to_string()));
        lines.push(field(tui, "st_irrigation", context.irrigation_system.to_string()));
        lines.push(field(tui, "st_ph", format!("{:.1}", context.ph)));
        lines.push(field(tui, "st_om", format!("{:.1} %", context.organic_matter_percent)));
        lines.push(field(tui, "st_cec", format!("{:.1}", context.cec_cmolc_kg)));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(format!("{}:", tui.i18n.t("st_reference")), tui.theme.muted()));
    lines.push(Line::styled(tui.cfg.reference_dir().display().to_string(), tui.theme.muted()));
    lines.push(Line::styled(format!("{}:", tui.i18n.t("st_curated")), tui.theme.muted()));
    lines.push(Line::styled(tui.cfg.curated_dir().display().to_string(), tui.theme.muted()));

    let block = panel(tui.i18n.t("system_status").to_string(), false, tui);
    frame.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), area);
}

// ---- screens -------------------------------------------------------------

fn workspace(frame: &mut Frame, area: Rect, tui: &Tui) {
    match tui.screen {
        Screen::Dashboard => dashboard(frame, area, tui),
        Screen::Crops => crops(frame, area, tui),
        Screen::Plan => plan(frame, area, tui),
        Screen::Inspect => inspect(frame, area, tui),
        Screen::Settings => settings(frame, area, tui),
    }
}

fn dashboard(frame: &mut Frame, area: Rect, tui: &Tui) {
    let block = panel(tui.i18n.t("dashboard_title").to_string(), !tui.focus_modules, tui);
    if tui.lots.is_empty() {
        return frame.render_widget(empty(tui, "no_lots").block(block), area);
    }

    let rows: Vec<Row> = tui
        .lots
        .iter()
        .enumerate()
        .map(|(index, lot)| {
            let crop = if index == tui.lot_idx { crop_of(tui, lot) } else { lot.crop_id.clone() };
            Row::new(vec![
                Cell::from(lot.field_id.clone()),
                Cell::from(crop),
                Cell::from(format!("{} {}", lot.yield_value, lot.yield_unit)),
            ])
        })
        .collect();

    let table = Table::new(rows, [Constraint::Length(12), Constraint::Min(14), Constraint::Length(14)])
        .header(header(tui, &["col_lot", "col_crop", "col_yield"]))
        .row_highlight_style(tui.theme.selected())
        .block(block);
    frame.render_stateful_widget(table, area, &mut TableState::default().with_selected(Some(tui.lot_idx)));
}

fn crops(frame: &mut Frame, area: Rect, tui: &Tui) {
    let block = panel(tui.i18n.t("crops_title").to_string(), !tui.focus_modules, tui);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [filter_area, yield_area, table_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    let cursor = if tui.filtering { "█" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("/{} ", tui.i18n.t("crops_filter")), tui.theme.muted()),
            Span::styled(format!("{}{cursor}", tui.filter), tui.theme.accent()),
        ])),
        filter_area,
    );
    frame.render_widget(Paragraph::new(yield_line(tui)), yield_area);

    let matches = tui.filtered_crops();
    if matches.is_empty() {
        return frame.render_widget(empty(tui, "no_crops"), table_area);
    }
    let rows: Vec<Row> = matches
        .iter()
        .map(|crop| {
            Row::new(vec![
                Cell::from(crop.crop_id.clone()),
                Cell::from(crop.name.clone()),
                Cell::from(crop.crop_type.clone()),
                Cell::from(Span::styled(crop.family.clone(), tui.theme.muted())),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [Constraint::Length(16), Constraint::Min(16), Constraint::Length(12), Constraint::Length(14)],
    )
    .header(header(tui, &["col_crop_id", "col_name", "col_type", "col_family"]))
    .row_highlight_style(tui.theme.selected());
    frame.render_stateful_widget(table, table_area, &mut TableState::default().with_selected(Some(tui.crop_idx)));
}

/// The yield-goal field under the crop filter. Empty until a crop is
/// picked, because a yield goal only means anything next to a crop.
fn yield_line<'a>(tui: &Tui) -> Line<'a> {
    let Some(crop_id) = &tui.crop_override else {
        return Line::raw("");
    };
    let cursor = if tui.editing_yield { "█" } else { "" };
    Line::from(vec![
        Span::styled(format!("{} {crop_id}: ", tui.i18n.t("crops_yield")), tui.theme.muted()),
        Span::styled(format!("{}{cursor}", tui.yield_input), tui.theme.accent()),
        Span::styled(format!(" {}", super::YIELD_UNIT), tui.theme.muted()),
    ])
}

fn plan(frame: &mut Frame, area: Rect, tui: &Tui) {
    let Some(plan) = &tui.plan else {
        let block = panel(tui.i18n.t("plan_title").to_string(), !tui.focus_modules, tui);
        return frame.render_widget(empty(tui, "no_plan").block(block), area);
    };

    let title = format!(
        "{} · {} · {} · {} {} {}",
        tui.i18n.t("plan_title"),
        plan.field_id,
        plan.crop_id,
        tui.i18n.t("plan_yield_target"),
        plan.yield_target.value,
        plan.yield_target.unit
    );
    let rows: Vec<Row> = plan
        .nutrient_results
        .iter()
        .map(|entry| {
            let dose = match &entry.dose {
                Some(dose) => format!("{} · {:.1} kg/ha", dose.source_name, dose.kg_product_per_ha),
                None => tui.i18n.t("value_none").to_string(),
            };
            Row::new(vec![
                Cell::from(Span::styled(entry.nutrient.to_string(), tui.theme.accent())),
                Cell::from(format!("{:.1}", entry.demand_kg_ha)),
                Cell::from(format!("{:.1}", entry.availability_kg_ha)),
                Cell::from(format!("{:.0}%", entry.efficiency_used * 100.0)),
                Cell::from(format!("{:.1}", entry.net_requirement_kg_ha)),
                Cell::from(Span::styled(
                    bar(entry.net_requirement_kg_ha, entry.demand_kg_ha, 10),
                    tui.theme.accent(),
                )),
                Cell::from(soil_status_span(tui, entry.soil_status)),
                Cell::from(dose),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(5),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(14),
        ],
    )
    .header(header(
        tui,
        &["col_nutrient", "col_demand", "col_availability", "col_efficiency", "col_net", "col_balance", "col_soil_status", "col_dose"],
    ))
    .block(panel(title, !tui.focus_modules, tui));
    frame.render_widget(table, area);
}

fn inspect(frame: &mut Frame, area: Rect, tui: &Tui) {
    let block = panel(tui.i18n.t("inspect_title").to_string(), !tui.focus_modules, tui);
    let Some(inspection) = &tui.inspection else {
        return frame.render_widget(empty(tui, "no_inspection").block(block), area);
    };

    let context = &inspection.field_context;
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", context.field_id), tui.theme.title()),
            Span::styled(
                format!(
                    "· {} · {} · {}",
                    context.texture, context.irrigation_system, context.region
                ),
                tui.theme.muted(),
            ),
        ]),
        field(
            tui,
            "plan_yield_target",
            format!("{} {}", inspection.yield_target.value, inspection.yield_target.unit),
        ),
        Line::raw(""),
        Line::styled(tui.i18n.t("inspect_soil_tests").to_string(), tui.theme.title()),
    ];
    for test in &inspection.soil_tests {
        // `to_string()` first: the domain's Display impls write straight to
        // the formatter, so a bare `{:<4}` on them would not pad.
        lines.push(Line::raw(format!(
            "  {:<4} {:>9}  {:<14} {}",
            test.nutrient.to_string(),
            test.value,
            test.unit,
            test.method
        )));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(tui.i18n.t("inspect_provenance").to_string(), tui.theme.title()));
    for entry in &inspection.provenance {
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<4}", entry.nutrient.to_string()), tui.theme.accent()),
            Span::raw(" "),
            soil_status_span(tui, planned_status(tui, &entry.nutrient.to_string())),
        ]));
        match &entry.removal_reference {
            Some(removal) => lines.push(Line::styled(
                format!(
                    "      {} {} kg/unit · {} {} · {} {}",
                    tui.i18n.t("inspect_removal"),
                    removal.removal_kg_per_unit,
                    tui.i18n.t("inspect_source"),
                    removal.source,
                    tui.i18n.t("inspect_year"),
                    removal.year
                ),
                tui.theme.muted(),
            )),
            None => lines.push(Line::styled(
                format!("      {}", tui.i18n.t("inspect_no_removal")),
                tui.theme.warn(),
            )),
        }
        if let Some((min, max)) = entry.efficiency_range {
            lines.push(Line::styled(
                format!("      {} {:.0}%-{:.0}%", tui.i18n.t("inspect_efficiency"), min * 100.0, max * 100.0),
                tui.theme.muted(),
            ));
        }
        if let Some(level) = &entry.critical_level {
            lines.push(Line::styled(
                format!(
                    "      {} {} / {} / {} · {} {} ({})",
                    tui.i18n.t("inspect_critical"),
                    level.low_threshold,
                    level.medium_threshold,
                    level.high_threshold,
                    tui.i18n.t("inspect_source"),
                    level.source,
                    level.year
                ),
                tui.theme.muted(),
            ));
        }
    }

    // Known gap: reference data exists for these, no use case consumes it.
    lines.push(Line::raw(""));
    lines.push(Line::styled(tui.i18n.t("inspect_micronutrients").to_string(), tui.theme.title()));
    for nutrient in UNPLANNED_MICRONUTRIENTS {
        lines.push(Line::styled(
            format!("  {:<4} {}", nutrient, tui.i18n.t("inspect_not_planned")),
            tui.theme.muted(),
        ));
    }

    frame.render_widget(Paragraph::new(lines).block(block).scroll((tui.scroll, 0)), area);
}

fn settings(frame: &mut Frame, area: Rect, tui: &Tui) {
    let items: Vec<ListItem> = SETTINGS
        .iter()
        .map(|id| {
            let label = format!(" {:<26}", tui.i18n.t(id));
            let value: Vec<Span> = match *id {
                "settings_language" => {
                    let names = [tui.i18n.t("lang_en").to_string(), tui.i18n.t("lang_es").to_string()];
                    let active = match tui.i18n.language() {
                        Language::English => names[0].clone(),
                        Language::Spanish => names[1].clone(),
                    };
                    toggle(tui, &names, &active)
                }
                "settings_profile" => toggle(tui, &tui.profiles, &tui.cfg.profile),
                "settings_data_root" => vec![Span::styled(tui.cfg.data_root.display().to_string(), tui.theme.muted())],
                "settings_reference_dir" => {
                    vec![Span::styled(tui.cfg.reference_dir().display().to_string(), tui.theme.muted())]
                }
                _ => vec![Span::styled(tui.cfg.curated_dir().display().to_string(), tui.theme.muted())],
            };
            let mut spans = vec![Span::raw(label)];
            spans.extend(value);
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(panel(tui.i18n.t("settings_title").to_string(), !tui.focus_modules, tui))
        .highlight_style(if tui.focus_modules { tui.theme.accent() } else { tui.theme.selected() });
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(Some(tui.setting_idx)));
}

fn help_overlay(frame: &mut Frame, tui: &Tui) {
    let mut keys = vec![
        ("j/k · ↑/↓", "help_move"),
        ("Enter", "help_confirm"),
        ("Tab", "help_panes"),
        ("Esc · q", "help_back"),
        ("?", "help_help"),
    ];
    match tui.screen {
        Screen::Crops => {
            keys.insert(0, ("/", "help_filter"));
            keys.insert(1, ("0-9 · .", "help_yield"));
        }
        Screen::Settings => keys.insert(0, ("h/l · ←/→", "help_change")),
        _ => {}
    }

    let mut lines: Vec<Line> = keys
        .iter()
        .map(|(key, label)| {
            Line::from(vec![
                Span::styled(format!(" {key:<12}"), tui.theme.accent()),
                Span::raw(tui.i18n.t(label).to_string()),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled(format!(" {}", tui.i18n.t("help_close")), tui.theme.muted()));

    let area = centered(frame.area(), 56, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(panel(tui.i18n.t("help_title").to_string(), true, tui)),
        area,
    );
}

// ---- small helpers -------------------------------------------------------

fn screen_title(tui: &Tui) -> &str {
    tui.i18n.t(match tui.screen {
        Screen::Dashboard => "module_home",
        Screen::Plan => "plan_title",
        Screen::Crops => "crops_title",
        Screen::Inspect => "inspect_title",
        Screen::Settings => "settings_title",
    })
}

fn header<'a>(tui: &Tui, ids: &[&str]) -> Row<'a> {
    Row::new(ids.iter().map(|id| Cell::from(tui.i18n.t(id).to_string())).collect::<Vec<_>>()).style(tui.theme.muted())
}

fn field<'a>(tui: &Tui, id: &str, value: String) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{:<18}", tui.i18n.t(id)), tui.theme.muted()),
        Span::raw(value),
    ])
}

fn empty<'a>(tui: &Tui, id: &str) -> Paragraph<'a> {
    Paragraph::new(Line::styled(format!(" {}", tui.i18n.t(id)), tui.theme.muted()))
}

/// `[active] other other` — the bracketed one is the current value.
fn toggle(tui: &Tui, options: &[String], active: &str) -> Vec<Span<'static>> {
    options
        .iter()
        .map(|option| {
            if option == active {
                Span::styled(format!("[{option}] "), tui.theme.accent())
            } else {
                Span::styled(format!("{option} "), tui.theme.muted())
            }
        })
        .collect()
}

fn bar(value: f64, total: f64, width: usize) -> String {
    let filled = if total > 0.0 {
        ((value / total) * width as f64).round().clamp(0.0, width as f64) as usize
    } else {
        0
    };
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn soil_status_span<'a>(tui: &Tui, status: Option<SoilStatus>) -> Span<'a> {
    let (id, style) = match status {
        Some(SoilStatus::Low) => ("soil_low", tui.theme.error()),
        Some(SoilStatus::Medium) => ("soil_medium", tui.theme.warn()),
        Some(SoilStatus::High) => ("soil_high", tui.theme.ok()),
        None => ("value_none", tui.theme.muted()),
    };
    Span::styled(tui.i18n.t(id).to_string(), style)
}

/// TODO(gap): `ScenarioInspection` carries critical levels but no
/// classified `soil_status`, so the inspect screen borrows it from the
/// plan when one has been calculated for the same scenario.
fn planned_status(tui: &Tui, nutrient: &str) -> Option<SoilStatus> {
    tui.plan
        .as_ref()?
        .nutrient_results
        .iter()
        .find(|entry| entry.nutrient.to_string() == nutrient)?
        .soil_status
}

fn crop_of(tui: &Tui, lot: &crate::infra::bootstrap::LotRow) -> String {
    tui.crop_override.clone().unwrap_or_else(|| lot.crop_id.clone())
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::bootstrap;
    use crate::infra::tui_adapter::theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Flattened buffer text; cells are row-major, so anything that fits on
    /// one line stays contiguous.
    fn render(tui: &Tui, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|frame| draw(frame, tui)).expect("draw");
        terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect()
    }

    #[test]
    fn every_screen_renders_at_both_densities() {
        let mut tui = Tui::new(bootstrap::build_app(), &theme::DARK_THEME);
        tui.run_plan();
        assert!(tui.plan.is_some(), "LOT-001/corn/global should plan: {}", tui.message);
        tui.run_inspect();
        assert!(tui.inspection.is_some(), "LOT-001 should inspect: {}", tui.message);

        for screen in [Screen::Dashboard, Screen::Plan, Screen::Crops, Screen::Inspect, Screen::Settings] {
            tui.screen = screen;
            // 80x24 drops the status column; 130x40 shows all three.
            for (width, height) in [(80, 24), (130, 40)] {
                let out = render(&tui, width, height);
                assert!(out.contains("Modules"), "{screen:?} at {width}x{height} lost the module column");
            }
        }

        tui.screen = Screen::Inspect;
        let inspect = render(&tui, 130, 40);
        assert!(inspect.contains("Micronutrients"), "the unplanned-micronutrient rows must stay visible");

        tui.help = true;
        assert!(render(&tui, 80, 24).contains("Keybindings"), "the help overlay must fit an 80x24 terminal");
    }

    #[test]
    fn bar_is_proportional_and_never_overflows() {
        assert_eq!(bar(0.0, 100.0, 4), "░░░░");
        assert_eq!(bar(50.0, 100.0, 4), "██░░");
        assert_eq!(bar(100.0, 100.0, 4), "████");
        assert_eq!(bar(500.0, 100.0, 4), "████", "over-100% must clamp, not panic");
        assert_eq!(bar(10.0, 0.0, 4), "░░░░", "no demand must not divide by zero");
    }
}
