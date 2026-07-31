//! Rendering. Layout follows the "Estrato" direction from
//! `docs/Prototypes/`: context bar on top, fixed module column on the
//! left, workspace in the middle, status column on the right, modal
//! statusline at the bottom. Every label goes through `tui.i18n`.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

use super::i18n::Language;
use super::{Screen, Tui, MODULES, SETTINGS, UNPLANNED_MICRONUTRIENTS};
use crate::core::application::LotSummary;
use crate::core::domain::SoilStatus;

/// Below this width the status column is dropped rather than squeezed —
/// an 80x24 terminal keeps modules + workspace intact.
const NARROW: u16 = 92;

/// Context bar and statusline are framed boxes, like the prototype's, so
/// each costs a border row above and below its content.
const BAR: u16 = 3;

/// The accent rule down the left edge of a selected row — the terminal
/// equivalent of the prototype's `inset 2px 0 0 var(--acc)`.
const MARKER: &str = "▎";

/// The wordmark, straight from the prototype. 41 columns; the workspace
/// hides it rather than clip it when it doesn't fit.
const WORDMARK: [&str; 3] = [
    "╔╗╔╔═╗╔╗╔  ╔╗╔╔═╗╔╗ ╦╔═╗  ╔═╗╔═╗╦  ╦ ╦╔╦╗",
    "║║║║ ║║║║  ║║║║ ║╠╩╗║╚═╗  ╚═╗║ ║║  ║ ║║║║",
    "╝╚╝╚═╝╝╚╝  ╝╚╝╚═╝╚═╝╩╚═╝  ╚═╝╚═╝╩═╝╚═╝╩ ╩",
];
const WORDMARK_WIDTH: u16 = 41;
/// Three rows of wordmark, a blank, the subtitle, a blank.
const WORDMARK_HEIGHT: u16 = 6;

pub fn draw(frame: &mut Frame, tui: &Tui) {
    // Reset to the terminal's own background before anything else, so no
    // colour from a previous frame survives in the gaps between tiles.
    frame.render_widget(Block::new().style(Style::default().bg(tui.theme.bg)), frame.area());

    let [top, body, bottom] =
        Layout::vertical([Constraint::Length(BAR), Constraint::Min(0), Constraint::Length(BAR)]).areas(frame.area());

    context_bar(frame, top, tui);
    statusline(frame, bottom, tui);

    let columns = if body.width < NARROW {
        vec![Constraint::Length(24), Constraint::Min(0)]
    } else {
        vec![Constraint::Length(26), Constraint::Min(0), Constraint::Length(32)]
    };
    let panes = Layout::horizontal(columns).split(body);
    modules_pane(frame, panes[0], tui);
    workspace(frame, panes[1], tui);
    if let Some(area) = panes.get(2) {
        status_pane(frame, *area, tui);
    }

    if tui.picker.is_some() {
        picker_overlay(frame, tui);
    }
    if tui.help {
        help_overlay(frame, tui);
    }
}

// ---- chrome --------------------------------------------------------------

/// A tile. Rounded on every panel — the focused one is told apart by the
/// accent border and its lit title, not by a heavier line, which is what
/// keeps the mosaic from jumping as focus moves.
fn panel<'a>(title: String, focused: bool, tui: &Tui) -> Block<'a> {
    let (border, title_style) = if focused {
        (tui.theme.accent(), tui.theme.title())
    } else {
        (Style::default().fg(tui.theme.border), tui.theme.muted())
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .style(tui.theme.base())
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(title.to_uppercase(), title_style),
            Span::raw(" "),
        ]))
}

/// The framed box the two bars share: same rounded border, same fill.
fn bar_block<'a>(tui: &Tui) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(tui.theme.border))
        .style(tui.theme.base())
}

fn separator<'a>(tui: &Tui) -> Span<'a> {
    Span::styled("│", Style::default().fg(tui.theme.border))
}

fn mode_id(tui: &Tui) -> &'static str {
    if tui.help {
        "mode_help"
    } else if tui.filtering {
        "mode_filter"
    } else if tui.editing_yield {
        "mode_yield"
    } else if tui.form.as_ref().is_some_and(|form| form.editing) {
        "mode_edit"
    } else {
        "mode_nav"
    }
}

fn context_bar(frame: &mut Frame, area: Rect, tui: &Tui) {
    let block = bar_block(tui);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut spans = vec![
        Span::styled(format!(" {} ", tui.i18n.t(mode_id(tui))), tui.theme.badge(tui.theme.accent)),
        separator(tui),
        Span::styled(" ▸ ", tui.theme.accent()),
        Span::styled(format!("{} ", screen_title(tui)), tui.theme.title()),
        separator(tui),
        Span::styled(" non·nobis·solum ", tui.theme.strong()),
        separator(tui),
        Span::styled(format!(" {} ", tui.cfg.profile), tui.theme.strong()),
    ];
    if let Some(lot) = tui.lots.get(tui.lot_idx) {
        spans.push(separator(tui));
        spans.push(Span::styled(format!(" {} ", tui.i18n.t("st_lot")), tui.theme.muted()));
        spans.push(Span::styled(lot.field_id.clone(), tui.theme.strong()));
        if let Some(crop) = tui.active_crop() {
            spans.push(Span::styled(format!(" · {crop}"), tui.theme.ok()));
        }
        spans.push(Span::raw(" "));
    }

    let version = format!(" v{} ", env!("CARGO_PKG_VERSION"));
    let right = (version.chars().count() as u16 + 1).min(inner.width);
    let [left_area, right_area] = Layout::horizontal([Constraint::Min(0), Constraint::Length(right)]).areas(inner);
    frame.render_widget(Paragraph::new(Line::from(spans)), left_area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![separator(tui), Span::styled(version, tui.theme.muted())])),
        right_area,
    );
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
        Screen::NewLot | Screen::NewSample => "hint_form",
        Screen::Settings => "hint_settings",
    };
    statusline_with(frame, area, tui, hint);
}

fn statusline_with(frame: &mut Frame, area: Rect, tui: &Tui, hint: &str) {
    let block = bar_block(tui);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let left = Line::from(vec![
        Span::styled(format!(" {} ", tui.i18n.t(mode_id(tui))), tui.theme.badge(tui.theme.ok)),
        separator(tui),
        Span::styled(format!(" {} ", screen_title(tui).to_lowercase()), tui.theme.strong()),
        separator(tui),
        Span::styled(format!(" {} ", tui.i18n.t(hint)), tui.theme.muted()),
    ]);
    // The dot carries the severity, so the message itself stays readable
    // rather than being painted red end to end.
    let message = Line::from(vec![
        separator(tui),
        Span::styled(" ● ", if tui.is_error { tui.theme.error() } else { tui.theme.ok() }),
        Span::styled(
            format!("{} ", tui.message),
            if tui.is_error { tui.theme.error() } else { tui.theme.strong() },
        ),
    ]);

    let width = (tui.message.chars().count() as u16 + 5).min(inner.width);
    let [left_area, right_area] = Layout::horizontal([Constraint::Min(0), Constraint::Length(width)]).areas(inner);
    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(message), right_area);
}

fn modules_pane(frame: &mut Frame, area: Rect, tui: &Tui) {
    // Two borders and the selection marker; what is left is the row.
    let inner = area.width.saturating_sub(3) as usize;
    let items: Vec<ListItem> = MODULES
        .iter()
        .map(|(label, mnemonic, target, glyph)| {
            // The module whose screen is open keeps a lit glyph even when
            // the cursor has moved on, so "where am I" survives browsing.
            let current = *target == Some(tui.screen);
            let label = tui.i18n.t(label);
            // Glyph, its space, the mnemonic and its trailing space.
            let gap = inner.saturating_sub(label.chars().count() + 4);
            ListItem::new(Line::from(vec![
                Span::styled(format!("{glyph} "), if current { tui.theme.accent() } else { tui.theme.muted() }),
                Span::raw(label.to_string()),
                Span::raw(" ".repeat(gap)),
                Span::styled(format!("{mnemonic} "), tui.theme.muted()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(panel(tui.i18n.t("modules").to_string(), tui.focus_modules, tui))
        .highlight_symbol(MARKER)
        .highlight_style(if tui.focus_modules { tui.theme.selected() } else { tui.theme.accent() });
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(Some(tui.module_idx)));
}

fn status_pane(frame: &mut Frame, area: Rect, tui: &Tui) {
    let mut lines = vec![
        field(tui, "st_profile", tui.cfg.profile.clone()),
        field(tui, "st_crops", tui.crops.len().to_string()),
    ];
    if let Some(lot) = tui.lots.get(tui.lot_idx) {
        let crop = crop_of(tui, lot);
        lines.push(field(tui, "st_lot", lot.field_id.clone()));
        lines.push(field(tui, "st_yield", yield_of(tui, lot, &crop, true)));
        lines.push(field(tui, "st_crop", crop));
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
        Screen::NewLot | Screen::NewSample => form(frame, area, tui),
        Screen::Settings => settings(frame, area, tui),
    }
}

fn form(frame: &mut Frame, area: Rect, tui: &Tui) {
    let title = if tui.screen == Screen::NewSample { "form_new_sample_title" } else { "form_new_lot_title" };
    let block = panel(tui.i18n.t(title).to_string(), !tui.focus_modules, tui);
    let Some(form) = &tui.form else {
        return frame.render_widget(block, area);
    };

    let mut items: Vec<ListItem> = form
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let editing = form.editing && index == form.idx;
            let cursor = if editing { "█" } else { "" };
            // The marker is what tells a row you fill in from a row you
            // choose from: "▾" means Enter unfolds a list.
            let marker = if field.is_choice() { " ▾" } else { "" };
            let value = if field.is_choice() && field.value.is_empty() {
                tui.i18n.t("picker_none").to_string()
            } else {
                field.value.clone()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:<22}", tui.i18n.t(field.label)), tui.theme.muted()),
                Span::styled(format!("{value}{cursor}"), tui.theme.accent()),
                Span::styled(marker.to_string(), tui.theme.muted()),
            ]))
        })
        .collect();
    items.push(ListItem::new(Line::styled(
        format!(" [ {} ]", tui.i18n.t("form_save")),
        tui.theme.title(),
    )));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [list_area, hint_area] = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);

    let list = List::new(items).highlight_style(tui.theme.selected());
    frame.render_stateful_widget(list, list_area, &mut ListState::default().with_selected(Some(form.idx)));

    // A short closed set is worth spelling out under the form; a long one
    // (12 textures, 66 crops) only says how many there are and how to see
    // them.
    let hint = match form.fields.get(form.idx) {
        Some(field) if field.is_choice() && field.options.len() <= 5 => {
            field.options.iter().map(String::as_str).collect::<Vec<_>>().join(" · ")
        }
        Some(field) if field.is_choice() => format!("{} {}", field.options.len(), tui.i18n.t("form_pick_hint")),
        _ => tui.i18n.t("form_optional_hint").to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::styled(format!(" {hint}"), tui.theme.muted())).wrap(Wrap { trim: true }),
        hint_area,
    );
}

/// The wordmark, borrowed from the "Cuarzo" direction the prototype
/// explicitly keeps as Estrato's welcome screen. Shown only where it fits
/// whole — a clipped wordmark is worse than none.
fn wordmark(frame: &mut Frame, area: Rect, tui: &Tui) {
    let mut lines: Vec<Line> = WORDMARK.iter().map(|art| Line::styled(*art, tui.theme.title())).collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled(subtitle(tui), tui.theme.muted()));
    frame.render_widget(Paragraph::new(lines).centered(), area);
}

/// The line under the wordmark. Wider than the art itself, which is why
/// [`banner_width`] and not `WORDMARK_WIDTH` is what decides whether the
/// banner fits.
fn subtitle(tui: &Tui) -> String {
    let mut line = format!("{} · v{}", tui.i18n.t("app_subtitle").to_uppercase(), env!("CARGO_PKG_VERSION"));
    if let Some(user) = current_user() {
        line.push_str(&format!(" · {}", user.to_uppercase()));
    }
    line
}

/// Whoever is running this. `USER` is what a login shell sets and `LOGNAME`
/// is the POSIX spelling some environments set instead; a session with
/// neither (a bare service manager, a container) simply loses the segment
/// rather than greeting an empty name.
fn current_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .filter(|name| !name.trim().is_empty())
}

fn banner_width(tui: &Tui) -> u16 {
    WORDMARK_WIDTH.max(subtitle(tui).chars().count() as u16)
}

fn dashboard(frame: &mut Frame, area: Rect, tui: &Tui) {
    let block = panel(tui.i18n.t("dashboard_title").to_string(), !tui.focus_modules, tui);
    if tui.lots.is_empty() {
        return frame.render_widget(empty(tui, "no_lots").block(block), area);
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The banner gives way to the lot table the moment the table would be
    // squeezed: header plus four rows is the floor worth keeping.
    let banner = if inner.width >= banner_width(tui) && inner.height >= WORDMARK_HEIGHT + 5 {
        WORDMARK_HEIGHT
    } else {
        0
    };
    let [banner_area, table_area] =
        Layout::vertical([Constraint::Length(banner), Constraint::Min(0)]).areas(inner);
    if banner > 0 {
        wordmark(frame, banner_area, tui);
    }

    let rows: Vec<Row> = tui
        .lots
        .iter()
        .enumerate()
        .map(|(index, lot)| {
            let selected = index == tui.lot_idx;
            let crop = if selected {
                crop_of(tui, lot)
            } else {
                lot.default_crop().unwrap_or(tui.i18n.t("value_none")).to_string()
            };
            let goal = yield_of(tui, lot, &crop, selected);
            Row::new(vec![
                Cell::from(Span::styled(lot.field_id.clone(), tui.theme.accent())),
                Cell::from(crop),
                Cell::from(Span::styled(goal, tui.theme.ok())),
                Cell::from(Span::styled(
                    format!("{} · {}", lot.texture, lot.irrigation_system),
                    tui.theme.muted(),
                )),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Length(12), Constraint::Min(14), Constraint::Length(14), Constraint::Length(26)],
    )
    .header(header(tui, &["col_lot", "col_crop", "col_yield", "col_soil"]))
    .highlight_symbol(Span::styled(MARKER, tui.theme.accent()))
    .row_highlight_style(tui.theme.selected());
    frame.render_stateful_widget(table, table_area, &mut TableState::default().with_selected(Some(tui.lot_idx)));
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
                Cell::from(Span::styled(crop.crop_id.clone(), tui.theme.accent())),
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
    .highlight_symbol(Span::styled(MARKER, tui.theme.accent()))
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
                Cell::from(bar(tui, entry.net_requirement_kg_ha, entry.demand_kg_ha, 10)),
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
    ));

    let block = panel(title, !tui.focus_modules, tui);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [table_area, climate_area] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    frame.render_widget(table, table_area);
    frame.render_widget(Paragraph::new(climate_line(tui, plan)), climate_area);
}

/// Which regime produced the N numbers above. The mineralization factor
/// alone moves N availability by up to 3x between the climate-adjusted and
/// the baseline value, so the plan must never leave the reader guessing —
/// same rule the CLI output follows.
fn climate_line<'a>(tui: &Tui, plan: &crate::core::domain::FertilityPlan) -> Line<'a> {
    let label = match plan.climate.as_ref().and_then(|climate| climate.mean_temp_c) {
        Some(temp) => format!("{} ({} {temp:.1} °C)", tui.i18n.t("plan_climate_adjusted"), tui.i18n.t("plan_mean_temp")),
        None => tui.i18n.t("plan_climate_baseline").to_string(),
    };
    Line::from(vec![
        Span::styled(format!(" {} ", tui.i18n.t("plan_mineralization")), tui.theme.muted()),
        Span::styled(format!("{:.4} ", plan.mineralization_factor), tui.theme.accent()),
        Span::styled(label, tui.theme.muted()),
    ])
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
    // One row for all six — six copies of the same sentence was six rows of
    // a scrolling page saying one thing.
    lines.push(Line::raw(""));
    lines.push(Line::styled(tui.i18n.t("inspect_micronutrients").to_string(), tui.theme.title()));
    lines.push(Line::from(vec![
        Span::styled(format!("  {}  ", UNPLANNED_MICRONUTRIENTS.join(" · ")), tui.theme.warn()),
        Span::styled(tui.i18n.t("inspect_not_planned").to_string(), tui.theme.muted()),
    ]));

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

/// The unfolded option list for the selected form field. Sized to its
/// contents up to half the screen, so a 4-option list isn't a huge empty
/// box and the 66-crop list scrolls instead of overflowing.
fn picker_overlay(frame: &mut Frame, tui: &Tui) {
    let Some(picker) = &tui.picker else { return };
    let label = tui
        .form
        .as_ref()
        .and_then(|form| form.fields.get(picker.field_idx))
        .map(|field| tui.i18n.t(field.label))
        .unwrap_or_default();

    let items: Vec<ListItem> = picker
        .options
        .iter()
        .map(|option| {
            let shown = if option.is_empty() { tui.i18n.t("picker_none") } else { option.as_str() };
            ListItem::new(Line::raw(format!(" {shown}")))
        })
        .collect();

    let height = (items.len() as u16 + 2).min(frame.area().height / 2).max(3);
    let area = centered(frame.area(), 40, height);
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(
        List::new(items)
            .block(panel(label.to_string(), true, tui))
            .highlight_style(tui.theme.selected()),
        area,
        &mut ListState::default().with_selected(Some(picker.idx)),
    );
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
        Screen::NewLot | Screen::NewSample => {
            keys.insert(0, ("Enter", if tui.picker.is_some() { "help_pick" } else { "help_edit" }))
        }
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
        Screen::NewLot => "form_new_lot_title",
        Screen::NewSample => "form_new_sample_title",
        Screen::Settings => "settings_title",
    })
}

/// Column headings, the prototype's `table.t th`. Uppercase is what sets
/// them apart from the data — they used to be dimmed as well, which on a
/// terminal with a wallpaper simply deleted them.
fn header<'a>(tui: &Tui, ids: &[&str]) -> Row<'a> {
    Row::new(ids.iter().map(|id| Cell::from(tui.i18n.t(id).to_uppercase())).collect::<Vec<_>>())
        .style(tui.theme.muted())
        .bottom_margin(1)
}

/// `label   value` — the value is the one carrying the emphasis.
fn field<'a>(tui: &Tui, id: &str, value: String) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{:<18}", tui.i18n.t(id)), tui.theme.muted()),
        Span::styled(value, tui.theme.strong()),
    ])
}

fn empty<'a>(tui: &Tui, id: &str) -> Paragraph<'a> {
    Paragraph::new(Line::styled(format!(" {}", tui.i18n.t(id)), tui.theme.muted()))
}

/// The current value is a filled chip, the rest stay muted text. The `▸`
/// carries the same information in glyph form because the row highlight
/// patches over both colours — and the selected row is exactly the one
/// whose active value has to stay readable.
fn toggle(tui: &Tui, options: &[String], active: &str) -> Vec<Span<'static>> {
    options
        .iter()
        .map(|option| {
            if option == active {
                Span::styled(format!("▸{option} "), tui.theme.badge(tui.theme.accent))
            } else {
                Span::styled(format!(" {option} "), tui.theme.muted())
            }
        })
        .collect()
}

/// Filled in the accent, the remainder in the border colour — the prototype's
/// `████░░░░` reads as one bar only when the two halves differ in weight,
/// not just in glyph.
fn bar<'a>(tui: &Tui, value: f64, total: f64, width: usize) -> Line<'a> {
    let filled = if total > 0.0 {
        ((value / total) * width as f64).round().clamp(0.0, width as f64) as usize
    } else {
        0
    };
    Line::from(vec![
        Span::styled("█".repeat(filled), tui.theme.accent()),
        Span::styled("░".repeat(width - filled), Style::default().fg(tui.theme.border)),
    ])
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

fn crop_of(tui: &Tui, lot: &LotSummary) -> String {
    tui.crop_override
        .clone()
        .or_else(|| lot.default_crop().map(str::to_string))
        .unwrap_or_else(|| tui.i18n.t("value_none").to_string())
}

/// The goal shown for a lot: the curated one for the crop on that row,
/// or — on the selected row only — the goal typed in the crop catalog.
fn yield_of(tui: &Tui, lot: &LotSummary, crop: &str, selected: bool) -> String {
    match lot.target_for(crop) {
        Some(target) => format!("{} {}", target.value, target.unit),
        None if selected && !tui.yield_input.is_empty() => format!("{} {}", tui.yield_input, super::YIELD_UNIT),
        None => tui.i18n.t("value_none").to_string(),
    }
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
        let mut tui = Tui::new(bootstrap::build_app(), &theme::DARK_THEME, None);
        tui.run_plan();
        assert!(tui.plan.is_some(), "LOT-001/corn/global should plan: {}", tui.message);
        tui.run_inspect();
        assert!(tui.inspection.is_some(), "LOT-001 should inspect: {}", tui.message);

        for screen in [
            Screen::Dashboard,
            Screen::Plan,
            Screen::Crops,
            Screen::Inspect,
            Screen::NewLot,
            Screen::NewSample,
            Screen::Settings,
        ] {
            match screen {
                Screen::NewLot | Screen::NewSample => tui.open_form(screen),
                _ => tui.screen = screen,
            }
            // 80x24 drops the status column; 130x40 shows all three.
            for (width, height) in [(80, 24), (130, 40)] {
                let out = render(&tui, width, height);
                assert!(out.contains("MODULES"), "{screen:?} at {width}x{height} lost the module column");
            }
        }

        tui.screen = Screen::Inspect;
        // Tall enough for the whole page: the inspect screen scrolls, and
        // the micronutrient block sits at its very bottom, so a terminal
        // that fits everything is what proves the block is rendered rather
        // than dropped.
        let inspect = render(&tui, 130, 60);
        assert!(inspect.contains("Micronutrients"), "the unplanned-micronutrient rows must stay visible");

        tui.help = true;
        assert!(render(&tui, 80, 24).contains("KEYBINDINGS"), "the help overlay must fit an 80x24 terminal");
    }

    /// The banner is the one piece of chrome allowed to disappear, and it
    /// has to disappear whole — a half-drawn wordmark is worse than none.
    #[test]
    fn the_wordmark_shows_when_it_fits_and_is_dropped_when_it_does_not() {
        let tui = Tui::new(bootstrap::build_app(), &theme::DARK_THEME, None);
        assert!(render(&tui, 130, 40).contains(WORDMARK[0]), "a roomy terminal must show the wordmark");
        assert!(!render(&tui, 60, 40).contains(WORDMARK[0]), "too narrow: drop it, don't clip it");
        assert!(!render(&tui, 130, 14).contains(WORDMARK[0]), "too short: the lot table wins the space");
    }

    /// The longest list the form can unfold is the 66-crop catalog; it has
    /// to scroll inside the overlay rather than run off an 80x24 terminal.
    #[test]
    fn the_unfolded_option_list_renders_over_the_form() {
        let mut tui = Tui::new(bootstrap::build_app(), &theme::DARK_THEME, None);
        tui.open_form(Screen::NewLot);

        for label in ["form_irrigation", "form_crop"] {
            let form = tui.form.as_mut().expect("form");
            form.idx = form.fields.iter().position(|field| field.label == label).expect("field");
            tui.activate_form_row();
            assert!(tui.picker.is_some(), "{label} must unfold a list");
            for (width, height) in [(80, 24), (130, 40)] {
                let out = render(&tui, width, height);
                assert!(out.contains("MODULES"), "{label} at {width}x{height} lost the module column");
            }
            tui.picker = None;
        }

        // The list must actually paint its entries, not just an empty
        // frame. Texture is the case that proves it: with 12 options the
        // hint line under the form only shows a count, so a texture name
        // on screen can only have come from the overlay.
        let form = tui.form.as_mut().expect("form");
        form.idx = form.fields.iter().position(|field| field.label == "form_texture").expect("field");
        tui.activate_form_row();
        assert!(render(&tui, 80, 24).contains("silty_clay_loam"), "the option list must render its entries");
    }

    #[test]
    fn bar_is_proportional_and_never_overflows() {
        let tui = Tui::new(bootstrap::build_app(), &theme::DARK_THEME, None);
        // The two halves carry different styles, so the assertion is on the
        // glyphs the line ends up painting.
        let glyphs = |value, total| {
            bar(&tui, value, total, 4).spans.iter().map(|span| span.content.to_string()).collect::<String>()
        };
        assert_eq!(glyphs(0.0, 100.0), "░░░░");
        assert_eq!(glyphs(50.0, 100.0), "██░░");
        assert_eq!(glyphs(100.0, 100.0), "████");
        assert_eq!(glyphs(500.0, 100.0), "████", "over-100% must clamp, not panic");
        assert_eq!(glyphs(10.0, 0.0), "░░░░", "no demand must not divide by zero");
    }
}


