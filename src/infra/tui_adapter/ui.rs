//! Rendering: module column left, workspace centre, status column right,
//! and one bar along the bottom carrying all of the chrome. Every label
//! goes through `tui.i18n`.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

use super::i18n::Language;
use super::theme;
use super::{stage_index, Screen, Tui, BAG_WEIGHTS_KG, LOT_ACTIONS, SETTINGS, STAGES};
use crate::core::application::LotSummary;
use crate::core::domain::{FertilityPlan, FertilizationStrategy, Nutrient, NutrientDemandMode, SoilStatus};
use crate::infra::report_renderer;

/// Below this the status column is dropped rather than squeezed, so an
/// 80x24 terminal keeps modules + workspace intact.
const NARROW: u16 = 92;

/// One row, nothing else. The panels above close with their own border, so
/// a rule or a frame here only repeats a line the eye already has.
const BAR: u16 = 1;

/// The accent rule down the left edge of a selected row.
const MARKER: &str = "▎";

/// The stepper's numerals for a stage not reached yet, and the tick for
/// one that is done. Five stages, five glyphs.
const STAGE_NUMERALS: [&str; 5] = ["①", "②", "③", "④", "⑤"];
const STAGE_DONE: &str = "✓";

/// Blackletter art, rasterised from Pirata One onto a grid of 2x2 subcells
/// per character. Quadrant and shade glyphs let an edge land between full
/// and empty, so the letters carry a dithered fringe rather than a hard
/// silhouette — that is what keeps them legible at this size, and what
/// gives them depth once [`wordmark`] colours the fringe apart.
///
/// The whole name across one line, 116 columns. Only 8 rows deep, so it is
/// the cheapest tier vertically as well as the grandest — a wide terminal
/// that is short still gets the full name.
const WORDMARK_WIDE: [&str; 8] = [
    "▗▄▓▄▄▓▓▖                      ▗▄▓▄▄▓▓▖            ▗▄▒    ▓█▖               ▄▄▓▙           ▗▟▙▖",
    "░██▛▀▜██                      ░██▛▀██▓         ░▓██▘     ▀▀░             ▐██░▜█▓          ░██▌",
    " ▓█▌ ▐██  ░▄▄▓▓▙░▗▄▓▙▄▟▓▄      ██▌ ▐█▓  ░▄▄▓▓▙  ▐██▄▓▓▄ ▗▄▓▖  ░▄▄▓▙      ▐██░      ▄▄▄▓▓▖  ██▌▗▄▓▄ ▄▓▙ ▄▟▓▄▄▓▓▄▄▓▓▖",
    " ▓█▌ ▐██ ░██▌░██▌ ▐██░▐██░     ██▌ ▐█▓ ░██░░██▌ ▐██░▐██░ ▓█▌ ▐█▓▝▓▀▘     ▐██▄▄▄▄  ▓█▓ ▜██  ██▌ ▜█▓ ▐██ ░██▌░▓█▓░▜██",
    " ▓█▌ ▐██ ░██▌ ██▌ ▐██ ░██▌     ██▌ ▐█▓ ░██░ ██▌ ▐██ ░██░ ▓█▌ ▐██▄▄▓▄      ▀▀▀░██▌ ▓█▓ ▐██  ██▌ ▐█▓ ▐██  ██▌ ▓█▓ ▐██",
    " ▓█▌ ▐██ ░██▌ ██▌ ▐██ ░██▌     ██▌ ▐█▓ ░██░ ██▌ ▐██ ░██░ ▓█▌ ▝▀▀░▐██       ▄ ░██▌ ▓█▓ ▐██  ██▌ ▐█▓ ▐██░ ██▌ ▓█▓ ▐██",
    " ▓█▙ ▐██▖ ██▙▄██▌ ▐██▖░██▙     ██▙ ▓██▖░██▙▄██▌ ▐██▄▟██░ ▓█▙ ▒██▖▐██     ▝██▙░██▌ ▜█▓▄▟██  ██▙ ▐██▄▟██▄ ██▙ ▓██▖▐██▖",
    "▄▓▀▀▘▝▀▀▘ ▝▓▀▀▘   ▝▀▀▘ ▀▀▀    ▄▓▀▀ ▝▀▀▘ ▝▓▀▀░    ▀▓▀▀░   ▝▀▀░ ▝▓▀▀░       ▝▓▀▀░    ▀▀▀▀░   ▀▀▀  ▀▀▀░▀▀▘ ▀▀▀ ▝▀▀▘▝▀▀▘",
];

/// The same name broken over two lines for a body that has height to spare
/// but not width, 70 columns.
const WORDMARK_LOCKUP: [&str; 17] = [
    "    ▗▄▓▙▄▓▓▄                       ▄▟▓▄▄▓▓▖            ░▄▓░   ▐██░",
    "    ░██▓▀▜██░                      ▝██▛▀███          ▓██▛     ▝▀▀",
    " ▓█▓ ▐██░ ░▄▄▓▓▓▄ ▄▟█▄▄▓█▄      ██▌ ▓██  ░▄▄▓█▓▖ ░██▙▟▓▓▖ ▄▓▓▖  ▄▄▓█▙",
    " ▓█▓ ▐██░ ██▓░▜██░░██▓░▜██      ██▌ ▓██ ░██▌░▓██ ░██▌░██▓ ░██▌ ██▓▝▓▀▘",
    " ▓█▓ ▐██░ ██▓ ▐██░ ██▓ ▐██      ██▌ ▓██ ░██▌ ▓██ ░██▌ ▓██ ░██▌ ██▙▄▄▓▄",
    " ▓█▓ ▐██░ ██▓ ▐██░ ██▓ ▐██      ██▌ ▓██ ░██▌ ▓██ ░██▌ ▓██ ░██▌ ▝▀▀▀▜██",
    " ▓██▖▐██▄ ▓██▄▟██░ ██▙░▐██▄     ██▙ ▓██▖ ██▙▄▓█▓ ░██▙▄▓█▓ ░██▙ ▒██▖▐██",
    "▄▓▀▀▘▝▜▀▀ ▝▜▓▀▀░   ▀▓▀▘▝▓▀▀    ▟▓▀▀▘▝▓▀▘ ▝▓▓▀▀░   ▝▓▓▀▀░   ▀▛▀  ▜▓▀▀░",
    "",
    "                        ▄▄▓██▙            ▗▓██▖",
    "                       ▓██░▜█▓▒            ▐██▌",
    "          ▓██░      ░▄▄▄▓██▄  ▐██▌▗▄▓█▖░▄▓█▖ ▄▓█▙▄▓██▄▄▓██▖",
    "          ▓██▄▄▄▓▄  ▓██▌░▓██▌ ▐██▌ ▐██▓ ▐██▓ ▝███░▝███░▀███",
    "          ▝▀▀▀▀▓██░ ▓██▌ ▓██▌ ▐██▌ ▐██▓ ▐██▓  ███ ░███ ░███",
    "            ░  ▓██░ ▓██▌ ▓██▌ ▐██▌ ▐██▓ ▐██▓  ███ ░███ ░███",
    "          ▓██▙ ▓██░ ▐██▙░▓██▌ ▐██▌ ▐██▓░▒██▓░ ███░░███░░███░",
    "           ▜██▓▀▀░  ▝▜██▓▀▀░  ▝▓▓▀▘ ▀█▓▀▀▓█▛▀ ▜█▓▀ ▜█▓▀ ▜█▓▀",
];

/// The same hand stacked three deep for a narrow panel, 40 columns.
const WORDMARK_TALL: [&str; 23] = [
    "                ▒▓█▙▓██▄",
    "         ██▓ ▜██     ░▄    ▄  ░▄",
    "        ██▓ ▓██ ▗▓▓▓▓██▖░███▓██▓",
    "        ██▓ ▓██ ▐██░ ██▌ ▐█▓ ▐██",
    "        ██▓ ▓██ ▐██▌░██▌ ▓██ ▐██░",
    "        ▓█▙ ▓██░░██▒░██▌ ▓██░▐██▖",
    "       ▄▓▓▀▘▝▓▀▘ ▀▓▓▀▀░  ▝▓▀▘▝▓▀▀",
    "",
    "    ▗▄▓▙▄▓█▙             ▄▄▒    ▓█▓",
    "    ░███▀▜██▌         ░▓██▘     ▝▀▀",
    " ▓██ ▐██▌  ▄▄▄▓█▙  ▐██▄▓█▙▖ ▄▓▓░  ▄▄▓█▙",
    " ▓██ ▐██▌ ▓██░▒██▌ ▐██░▒██▌ ▐██▌ ██▌▝▓▀▘",
    " ▓██ ▐██▌ ▓██ ░██▌ ▐██░░██▌ ▐██▌ ██▙▄▄▓▄",
    " ▓██ ▐██▌ ▓██ ░██▌ ▐██░░██▌ ▐██░ ▝▀▀░▜██",
    " ▓██▖▐██▙ ▐██▄▄██▌ ▐██▄▄██▌ ▐██▙░▓██▖▟██",
    "▄▓▛▀▘░▜▀▀  ▀▓▀▀▀    ▀▓▀▀▀░  ▝▜▀▀  ▜▓▀▀░",
    "",
    "           ▗▄▓██▄        ░▓█▓",
    " ██▌▝▀▀   ░▄▄▄░ ▐██ ▄▄▖ ▄▄▖░▄▄▖▄▄▄░▄▄▖",
    " ▓█▙▄▄▄ ░██▀▜█▓ ▐██ ▜█▓ ▜█▌▝▓█▛▀██▛▜██░",
    " ▝▀▀░██░░██░▐██ ▐██ ▐█▓ ▓█▌ ▓█▌ ██▌░██░",
    " ▄▄▄ ██░░██░▐██ ▐██ ▐█▓ ▓█▌ ▓█▌ ██▌░██░",
    " ▝██▓▀▀  ▜█▓▓▀▀ ▝█▓░▝▓█▓▜█▓░▒█▓░▓█▓░▓█▒",
];

/// Monogram over the spelled-out name, 41 columns. The two N are one
/// rasterised glyph repeated: setting the string "N N S" in one pass lands
/// each N on a different subcell phase, and they come out visibly unalike.
const WORDMARK_MARK: [&str; 13] = [
    "▗▄▓█▙▄▄▓█▙     ▗▄▓█▙▄▄▓█▙        ░▄▄▓█▙",
    "▝▜███▀▓███▌    ▝▜███▀▓███▌     ▗███▀███▙▖",
    " ▐███  ▓██▓     ▐███  ▓██▓     ▐███ ▝▓▀▀",
    " ▐███  ▓██▓     ▐███  ▓██▓     ▐███",
    " ▐███  ▓██▓     ▐███  ▓██▓     ▐███░▄▄▓▙░",
    " ▐███  ▓██▓     ▐███  ▓██▓     ▝███▓▀███▙",
    " ▐███  ▓██▓     ▐███  ▓██▓       ░   ▓██▓",
    " ▐███  ▓██▓     ▐███  ▓██▓       ▄▖  ▓██▓",
    " ▐███  ▓██▓     ▐███  ▓██▓     ▓███▌ ▓██▓",
    " ▟███▓ ▜███▓    ▟███▓ ▜███▓     ▜███▓▓▓▀▘",
    "▒▀▀░    ▝▘     ▒▀▀░    ▝▘        ▝▀▀",
    "",
    "      N O N   N O B I S   S O L U M",
];

/// The mark cut down to block letters, 29 columns and 7 rows, with the
/// spelled name under it.
///
/// The rasterised monogram above is 41 columns — *wider* than the stacked
/// name it is supposed to fall back from, so it could only ever be the
/// short-panel tier and never the narrow one. A half-screen terminal got no
/// art at all, which is the opposite of what a monogram is for. Block
/// letters instead of blackletter because blackletter needs the subcell
/// detail to read, and there is no room for it here.
const WORDMARK_SMALL: [&str; 7] = [
    "     █▙  █  █▙  █  ▗▄▄▄▖",
    "     ██▖ █  ██▖ █  █▛▘",
    "     █▝█▖█  █▝█▖█  ▝▀▀▄▖",
    "     █ ▝██  █ ▝██     ▐█",
    "     █  ▜█  █  ▜█  ▝▀▀▀▘",
    "",
    "N O N   N O B I S   S O L U M",
];

/// Last resort, no art at all: 19 columns on one row.
///
/// Tight rather than letterspaced, and it wears the statusline's separator
/// instead of gaps. The spaced setting is 29 columns and [`WORDMARK_SMALL`]
/// already carries it — a floor that needed as much room as the tier above
/// it was not a floor, it was the same rung twice.
const WORDMARK_LINE: [&str; 1] = [
    "NON · NOBIS · SOLUM",
];

/// Grandest first, and each tier is checked on both axes: a body too narrow
/// for one line may still take the name broken over two, or stacked three
/// deep, or reduced to a monogram. Blackletter lives on its detail, so a
/// tier that doesn't fit gives way to the next rather than being scaled
/// down into mush.
///
/// Ordered by the room each needs, widest first — which is what makes the
/// ladder a ladder. `WORDMARK_MARK` sitting after `WORDMARK_TALL` while
/// being a column wider than it was the bug: no width ever reached it that
/// had not already been taken.
const WORDMARKS: [&[&str]; 6] = [
    &WORDMARK_WIDE,
    &WORDMARK_LOCKUP,
    &WORDMARK_TALL,
    &WORDMARK_MARK,
    &WORDMARK_SMALL,
    &WORDMARK_LINE,
];

pub fn draw(frame: &mut Frame, tui: &Tui) {
    // Reset first, so no colour from a previous frame survives in the gaps
    // between tiles.
    frame.render_widget(Block::new().style(Style::default().bg(tui.theme.bg)), frame.area());

    // Two bars of chrome, both one row: the context bar says *where you
    // are and on what*, the statusline says *what you can press and what
    // just happened*. The mode badge is on both on purpose — it is the one
    // thing the eye must find without hunting, wherever it happens to be.
    let [top, body, bottom] =
        Layout::vertical([Constraint::Length(BAR), Constraint::Min(0), Constraint::Length(BAR)])
            .areas(frame.area());

    context_bar(frame, top, tui);
    statusline(frame, bottom, tui);

    // Home is the launcher, not a tile in the mosaic: it takes the whole
    // body, which is what puts the wordmark on its grandest tier instead of
    // on the monogram a 26-column list next to it forced. The lot the menu
    // acts on is named on both bars and moved with `h`/`l`, so nothing here
    // depends on seeing the list.
    if tui.screen == Screen::Dashboard {
        launcher(frame, body, tui);
    } else {
        let columns = if body.width < NARROW {
            vec![Constraint::Length(24), Constraint::Min(0)]
        } else {
            vec![Constraint::Length(26), Constraint::Min(0), Constraint::Length(32)]
        };
        let panes = Layout::horizontal(columns).split(body);
        lots_pane(frame, panes[0], tui);
        workspace(frame, panes[1], tui);
        if let Some(area) = panes.get(2) {
            status_pane(frame, *area, tui);
        }
    }

    if tui.picker.is_some() {
        picker_overlay(frame, tui);
    }
    if tui.help {
        help_overlay(frame, tui);
    }
}

// ---- chrome --------------------------------------------------------------

/// Rounded on every panel; focus shows as an accent border and lit title,
/// not a heavier line, so the mosaic doesn't jump as focus moves.
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

/// No rule and no box: the panels above already close with a border, and a
/// second line under it only doubled up. The bar is one row of chrome.
fn bar_block<'a>(tui: &Tui) -> Block<'a> {
    Block::new().style(tui.theme.base())
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

/// Where you are and on what: the mode badge, the breadcrumb down to the
/// stage, and — pushed right — the scenario figures the workspace is
/// currently answering for. Everything here is either absent or true; a
/// chip whose number does not exist yet is not painted at all.
///
/// It carries nothing the statusline already carries except the mode
/// badge, which the prototype repeats on both bars deliberately.
fn context_bar(frame: &mut Frame, area: Rect, tui: &Tui) {
    let block = bar_block(tui);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut left = vec![
        Span::styled(format!(" {} ", tui.i18n.t(mode_id(tui))), tui.theme.badge(tui.theme.accent)),
        Span::styled(" ▸ ", tui.theme.accent()),
        Span::styled(tui.i18n.t(screen_label(tui.screen)).to_string(), tui.theme.strong()),
    ];
    if let Some(index) = stage_index(tui.screen) {
        left.push(Span::styled(" ▸ ", tui.theme.accent()));
        left.push(Span::styled(tui.i18n.t(STAGES[index].1).to_string(), tui.theme.title()));
    }
    left.push(Span::raw(" "));

    let mut right = Vec::new();
    let mut chip = |label: String, value: String, style: Style| {
        right.push(separator(tui));
        right.push(Span::styled(format!(" {label} "), tui.theme.muted()));
        right.push(Span::styled(format!("{value} "), style));
    };
    // Scenario figures belong to the flow that answers for them; on the
    // launcher or a form they would only be numbers with no question. What
    // Home does need is the *subject*: its menu acts on the selected lot,
    // and with no lot column beside it this bar is what names the lot.
    if stage_index(tui.screen).is_some() {
        if let Some(target) = tui.typed_yield_target().or_else(|| tui.curated_yield_target().cloned()) {
            chip(
                tui.i18n.t("plan_yield_target").to_string(),
                format!("{} {}", target.value, target.unit),
                tui.theme.strong(),
            );
        }
        chip(
            tui.i18n.t("target_method").to_string(),
            tui.i18n.t(demand_mode_id(tui.demand_mode)).to_string(),
            tui.theme.ok(),
        );
        if let Some(plan) = &tui.plan {
            if let Some(entry) = plan.nutrient_results.iter().find(|entry| entry.nutrient == Nutrient::N) {
                chip(
                    format!("{} N", tui.i18n.t("col_efficiency")),
                    format!("{:.0}%", entry.efficiency_used * 100.0),
                    tui.theme.accent(),
                );
            }
        }
    } else if tui.screen == Screen::Dashboard {
        if let Some(lot) = tui.lots.get(tui.lot_idx) {
            let crop = tui.active_crop().map(|crop| format!(" · {crop}")).unwrap_or_default();
            chip(tui.i18n.t("st_lot").to_string(), format!("{}{crop}", lot.field_id), tui.theme.strong());
        }
    }

    // Dropped whole rather than clipped, cheapest first: a half-written
    // chip reads as a number, and a wrong number is worse than none.
    while width_of(&left) + width_of(&right) > inner.width && right.len() >= 3 {
        right.drain(0..3);
    }
    let right_width = width_of(&right).min(inner.width);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(inner);
    frame.render_widget(Paragraph::new(Line::from(left)), left_area);
    frame.render_widget(Paragraph::new(Line::from(right)), right_area);
}

fn demand_mode_id(mode: NutrientDemandMode) -> &'static str {
    match mode {
        NutrientDemandMode::Extraction => "method_extraction",
        NutrientDemandMode::Absorption => "method_absorption",
    }
}

/// What the breadcrumb calls a screen. The five stages share one name —
/// they are one flow, and naming each of them there would repeat what the
/// stepper already says two rows down.
fn screen_label(screen: Screen) -> &'static str {
    match screen {
        Screen::Dashboard => "lots",
        Screen::NewLot => "form_new_lot_title",
        Screen::EditLot => "form_edit_lot_title",
        Screen::NewSample => "form_new_sample_title",
        Screen::Import => "form_import_title",
        Screen::Settings => "settings_title",
        _ => "module_plan",
    }
}

fn statusline(frame: &mut Frame, area: Rect, tui: &Tui) {
    if tui.editing_yield {
        return statusline_with(frame, area, tui, "hint_yield");
    }
    let hint = match tui.screen {
        Screen::Dashboard => "hint_dashboard",
        Screen::Soil => "hint_inspect",
        Screen::Crops => "hint_crops",
        Screen::Target => "hint_target",
        Screen::Sources => "hint_sources",
        Screen::Plan => "hint_plan",
        Screen::NewLot | Screen::EditLot | Screen::NewSample | Screen::Import => "hint_form",
        Screen::Settings => "hint_settings",
    };
    statusline_with(frame, area, tui, hint);
}

/// The one bar, carrying what the two used to: mode, screen, project,
/// profile, selection, keys, and the last message. It wears the old
/// context bar's palette — the accent badge and a lit screen title —
/// because that was the identity of the chrome; the statusline's green
/// badge now only ever means "ok", down in the message dot.
///
/// Too narrow to hold everything, segments are dropped whole and
/// lowest-priority first rather than clipped mid-word. Brand and version
/// go first: the dashboard wordmark and its subtitle already carry both.
fn statusline_with(frame: &mut Frame, area: Rect, tui: &Tui, hint: &str) {
    let block = bar_block(tui);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The dot carries the severity, so the message itself stays readable.
    let mut right = vec![
        separator(tui),
        Span::styled(" ● ", if tui.is_error { tui.theme.error() } else { tui.theme.ok() }),
        Span::styled(
            format!("{} ", tui.message),
            if tui.is_error { tui.theme.error() } else { tui.theme.strong() },
        ),
    ];
    // The version is decoration and goes before anything else does: it
    // only appears once the whole left run, keybindings included, has
    // already fitted beside it.
    let whole_left = width_of(&fitted_left(tui, hint, u16::MAX));
    if whole_left + width_of(&right) + VERSION_SEGMENT <= inner.width {
        right.push(separator(tui));
        right.push(Span::styled(format!(" v{} ", env!("CARGO_PKG_VERSION")), tui.theme.muted()));
    }

    let left = fitted_left(tui, hint, inner.width.saturating_sub(width_of(&right)));

    let right_width = width_of(&right).min(inner.width);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(inner);
    frame.render_widget(Paragraph::new(Line::from(left)), left_area);
    frame.render_widget(Paragraph::new(Line::from(right)), right_area);
}

/// `" v0.1.0 "` plus its separator.
const VERSION_SEGMENT: u16 = 10;

fn width_of(spans: &[Span]) -> u16 {
    spans.iter().map(|span| span.content.chars().count() as u16).sum()
}

/// Segments in display order, each with the order it is given up in.
/// Priority 0 stays whatever the width.
fn fitted_left<'a>(tui: &'a Tui, hint: &str, budget: u16) -> Vec<Span<'a>> {
    let mut segments: Vec<(u8, Vec<Span>)> = vec![
        (0, vec![Span::styled(format!(" {} ", tui.i18n.t(mode_id(tui))), tui.theme.badge(tui.theme.accent))]),
        (
            0,
            vec![
                separator(tui),
                Span::styled(" ▸ ", tui.theme.accent()),
                // The stage's own short name inside the flow: the context
                // bar above already spells out the module, and the long
                // title here was crowding the keybindings off the bar.
                Span::styled(format!("{} ", short_title(tui)), tui.theme.title()),
            ],
        ),
        (4, vec![separator(tui), Span::styled(" non·nobis·solum ", tui.theme.strong())]),
        (3, vec![separator(tui), Span::styled(format!(" {} ", tui.cfg.profile), tui.theme.strong())]),
    ];

    if let Some(lot) = tui.lots.get(tui.lot_idx) {
        let mut spans = vec![
            separator(tui),
            Span::styled(format!(" {} ", tui.i18n.t("st_lot")), tui.theme.muted()),
            Span::styled(lot.field_id.clone(), tui.theme.strong()),
        ];
        if let Some(crop) = tui.active_crop() {
            spans.push(Span::styled(format!(" · {crop}"), tui.theme.ok()));
        }
        spans.push(Span::raw(" "));
        segments.push((2, spans));
    }

    segments.push((1, vec![separator(tui), Span::styled(format!(" {} ", tui.i18n.t(hint)), tui.theme.muted())]));

    // Give up one whole segment at a time, worst first, until it fits.
    for victim in (1..=4).rev() {
        if segments.iter().map(|(_, spans)| width_of(spans)).sum::<u16>() <= budget {
            break;
        }
        segments.retain(|(priority, _)| *priority != victim);
    }
    segments.into_iter().flat_map(|(_, spans)| spans).collect()
}

/// The left column: the lots, and what can be done to the one under the
/// cursor.
///
/// It was a module menu — Home, Fertilization, New lot… — which is a list
/// of *places*. Lots are what this app is about, so they get the column and
/// the places became keys.
fn lots_pane(frame: &mut Frame, area: Rect, tui: &Tui) {
    // Two borders and the marker; what's left is the row.
    let inner = area.width.saturating_sub(3) as usize;
    let items: Vec<ListItem> = tui
        .lots
        .iter()
        .map(|lot| {
            let crop = lot.default_crop().unwrap_or("").to_string();
            let gap = inner.saturating_sub(lot.field_id.chars().count() + crop.chars().count() + 3);
            ListItem::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(lot.field_id.clone(), tui.theme.base()),
                Span::raw(" ".repeat(gap)),
                Span::styled(crop, tui.theme.muted()),
                Span::raw(" "),
            ]))
        })
        .collect();
    let empty = items.is_empty();
    let items = if empty {
        vec![ListItem::new(Line::styled(format!(" {}", tui.i18n.t("no_lots")), tui.theme.muted()))]
    } else {
        items
    };

    let block = panel(tui.i18n.t("lots").to_string(), tui.focus_modules, tui);
    let body = block.inner(area);
    frame.render_widget(block, area);

    // The same verbs the launcher's menu spells out, folded into one line
    // each so the actions stay visible on the screens that have no menu.
    // Home is not one of them: the column is not drawn there at all.
    let legend: Vec<Line> = LOT_ACTIONS
        .iter()
        .map(|(label, key, glyph)| {
            let label = tui.i18n.t(label);
            let gap = (body.width as usize).saturating_sub(label.chars().count() + 6);
            Line::from(vec![
                Span::styled(format!("{glyph} "), tui.theme.muted()),
                Span::styled(label.to_string(), tui.theme.muted()),
                Span::raw(" ".repeat(gap)),
                Span::styled(format!("{key} "), tui.theme.accent()),
            ])
        })
        .collect();
    let legend_height =
        if legend.is_empty() { 0 } else { (legend.len() as u16 + 1).min(body.height.saturating_sub(1)) };
    let [list_area, legend_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(legend_height)]).areas(body);

    let list = List::new(items)
        .highlight_symbol(MARKER)
        .highlight_style(if tui.focus_modules { tui.theme.selected() } else { tui.theme.accent() });
    frame.render_stateful_widget(
        list,
        list_area,
        &mut ListState::default().with_selected((!empty).then_some(tui.lot_idx)),
    );
    frame.render_widget(Paragraph::new(legend), legend_area);
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
    // What the plan actually settled on, once there is one. Every figure
    // is read off `FertilityPlan`; nothing here is derived a second time.
    if let Some(plan) = &tui.plan {
        lines.push(Line::raw(""));
        lines.push(Line::styled(tui.i18n.t("st_plan_ready").to_uppercase(), tui.theme.ok()));
        let total: f64 = plan.nutrient_results.iter().map(|entry| entry.net_requirement_kg_ha).sum();
        lines.push(field(tui, "st_total", format!("{total:.0} kg/ha")));
        for entry in &plan.nutrient_results {
            if let Some(dose) = &entry.dose {
                lines.push(Line::from(vec![
                    Span::styled(format!("{:<18}", dose.source_name), tui.theme.muted()),
                    Span::styled(format!("{:.0} kg", dose.kg_product_per_ha), tui.theme.strong()),
                ]));
            }
        }
        if let Some(liming) = &plan.liming {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<18}", tui.i18n.t("st_liming")), tui.theme.warn()),
                Span::styled(format!("{:.1} t/ha", liming.recommended_t_ha), tui.theme.strong()),
            ]));
            lines.extend(saturation_bar(
                tui,
                liming.current_base_saturation_pct,
                liming.target_base_saturation_pct,
            ));
        }
        if !plan.warnings.is_empty() {
            lines.push(field(tui, "st_warnings", plan.warnings.len().to_string()));
        }
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

/// Base saturation against the target the liming recommendation is aiming
/// at — the prototype's `saturación · meta` block, on the one saturation
/// figure the domain actually reports.
/// Two rows, because the label alone spends most of a 30-column pane: the
/// name, then the bar with both figures beside it.
fn saturation_bar<'a>(tui: &Tui, current: f64, goal: f64) -> Vec<Line<'a>> {
    let mut spans = vec![Span::raw("  ")];
    spans.extend(bar(tui, current, 100.0, 10).spans);
    spans.push(Span::styled(format!(" {current:.0}%"), tui.theme.strong()));
    spans.push(Span::styled(format!(" / {goal:.0}%"), tui.theme.muted()));
    vec![
        Line::styled(tui.i18n.t("st_base_saturation").to_string(), tui.theme.muted()),
        Line::from(spans),
    ]
}

// ---- screens -------------------------------------------------------------

/// The focal panel. Every stage of the fertilization flow opens with the
/// same stepper ribbon and then paints its own body underneath, so the
/// mosaic never changes shape as the flow advances.
fn workspace(frame: &mut Frame, area: Rect, tui: &Tui) {
    if stage_index(tui.screen).is_none() {
        return match tui.screen {
            Screen::NewLot | Screen::EditLot | Screen::NewSample | Screen::Import => form(frame, area, tui),
            Screen::Settings => settings(frame, area, tui),
            _ => launcher(frame, area, tui),
        };
    }

    let title = format!("{} · {}", tui.i18n.t("workspace"), screen_title(tui));
    let block = panel(title, !tui.focus_modules, tui);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Two rows: the ribbon folds onto a second one on a narrow terminal
    // rather than losing its last stages.
    let [ribbon, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(stepper(tui, inner.width)), ribbon);

    match tui.screen {
        Screen::Soil => soil(frame, body, tui),
        Screen::Crops => crops(frame, body, tui),
        Screen::Target => target(frame, body, tui),
        Screen::Sources => sources(frame, body, tui),
        _ => plan(frame, body, tui),
    }
}

/// `✓ Suelo › ✓ Cultivo › ③ Objetivo › ④ Fuentes › ⑤ Plan`.
///
/// Done is measured, never assumed: a stage reads as done when the thing
/// it produces actually exists in state, so backing out of a stage and
/// invalidating its result un-ticks it on the next frame.
/// Folded onto as many rows as `width` needs, **never mid-chip**: a
/// paragraph's own wrapping breaks on whitespace, and a filled pill has
/// spaces inside it, so it would split `✓ Fuentes` across two lines.
fn stepper(tui: &Tui, width: u16) -> Vec<Line<'static>> {
    let current = stage_index(tui.screen);
    let mut lines: Vec<Line> = Vec::new();
    let mut row: Vec<Span> = Vec::new();
    let mut used = 0usize;

    for (index, (screen, label)) in STAGES.iter().enumerate() {
        let done = stage_is_done(tui, *screen);
        let glyph = if done { STAGE_DONE } else { STAGE_NUMERALS[index] };
        let text = format!(" {glyph} {} ", tui.i18n.t(label));
        let chip = match (current == Some(index), done) {
            // The one you are on is a filled pill; nothing else fills, so
            // there is never a question about where the cursor is.
            (true, _) => Span::styled(text, tui.theme.badge(tui.theme.accent)),
            (false, true) => Span::styled(text, tui.theme.ok()),
            (false, false) => Span::styled(text, tui.theme.muted()),
        };
        let separator = Span::styled(" › ", Style::default().fg(tui.theme.border));
        let cost = chip.content.chars().count() + if row.is_empty() { 0 } else { 3 };
        if !row.is_empty() && used + cost > width as usize {
            lines.push(Line::from(std::mem::take(&mut row)));
            used = 0;
        }
        if !row.is_empty() {
            row.push(separator);
        }
        used += cost;
        row.push(chip);
    }
    lines.push(Line::from(row));
    lines
}

fn stage_is_done(tui: &Tui, stage: Screen) -> bool {
    match stage {
        Screen::Soil => tui.inspection.is_some(),
        Screen::Crops => tui.active_crop().is_some(),
        Screen::Target => tui.typed_yield_target().is_some() || tui.curated_yield_target().is_some(),
        // Both read one plan: there are no doses without a balance and no
        // balance without doses.
        _ => tui.plan.is_some(),
    }
}

fn form(frame: &mut Frame, area: Rect, tui: &Tui) {
    let title = match tui.screen {
        Screen::NewSample => "form_new_sample_title",
        Screen::EditLot => "form_edit_lot_title",
        Screen::Import => "form_import_title",
        _ => "form_new_lot_title",
    };
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
            // "▾" means Enter unfolds a list.
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
    // The import report stays on screen next to the field: a run that
    // rejected three rows is a file to go and fix, and a one-line status
    // bar cannot carry three line numbers.
    let report_height = if tui.import_report.is_empty() { 0 } else { (tui.import_report.len() as u16 + 2).min(12) };
    let [list_area, report_area, hint_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(report_height), Constraint::Length(2)])
            .areas(inner);

    let list = List::new(items).highlight_style(tui.theme.selected());
    frame.render_stateful_widget(list, list_area, &mut ListState::default().with_selected(Some(form.idx)));

    if report_height > 0 {
        let lines: Vec<Line> = std::iter::once(Line::raw(""))
            .chain(tui.import_report.iter().enumerate().map(|(index, line)| {
                // The first line is the tally, the rest are rejections.
                Line::styled(format!(" {line}"), if index == 0 { tui.theme.strong() } else { tui.theme.warn() })
            }))
            .collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), report_area);
    }

    // A short set is spelled out; a long one only says how many.
    let hint = match form.fields.get(form.idx) {
        Some(field) if field.is_choice() && field.options.len() <= 5 => {
            field.options.iter().map(String::as_str).collect::<Vec<_>>().join(" · ")
        }
        Some(field) if field.is_choice() => format!("{} {}", field.options.len(), tui.i18n.t("form_pick_hint")),
        _ if tui.screen == Screen::Import => tui.i18n.t("form_import_hint").to_string(),
        _ => tui.i18n.t("form_optional_hint").to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::styled(format!(" {hint}"), tui.theme.muted())).wrap(Wrap { trim: true }),
        hint_area,
    );
}

/// The launcher menu is one fixed-width block so every row centres on the
/// same axis — a ragged menu would zig-zag under a centred paragraph.
const MENU_WIDTH: usize = 48;

/// Marker, glyph and their spacing on the left, mnemonic on the right.
const MENU_CHROME: usize = 7;

/// Home is a launcher: wordmark, the menu, and one line of real numbers
/// about the data actually loaded. The lot table is the column to its
/// left; what the menu adds is the *names* of the keys that act on it.
fn launcher(frame: &mut Frame, area: Rect, tui: &Tui) {
    // Framed like every other screen, but untitled: the wordmark inside is
    // the title, and repeating it on the border would say it twice.
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(tui.theme.border))
        .style(tui.theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // A real menu: one row per verb, its key on the right, and a cursor on
    // the row `Enter` will run. The cursor lives in the workspace pane, so
    // `Tab` hands `j`/`k` to it and back to the lot list — the two-pane rule
    // the rest of the app already follows, rather than a second selection
    // competing for the same keys.
    let mut menu: Vec<Line> = LOT_ACTIONS
        .iter()
        .enumerate()
        .map(|(index, (label, key, glyph))| {
            menu_row(tui, glyph, tui.i18n.t(label), None, Some(*key), index == tui.menu_idx)
        })
        .collect();
    menu.push(Line::raw(""));
    menu.push(readiness(tui));

    // The banner takes what the menu leaves, and steps down a tier or
    // disappears if that isn't enough. Banner and menu are then one block,
    // centred on both axes: pinned to the top they left a third of the
    // panel empty under them, which reads as a screen still loading.
    let art = pick_wordmark(inner, menu.len() as u16);
    let banner = art.map_or(0, banner_height);
    let pad = inner.height.saturating_sub(banner + menu.len() as u16) / 2;
    let [_, banner_area, menu_area] =
        Layout::vertical([Constraint::Length(pad), Constraint::Length(banner), Constraint::Min(0)])
            .areas(inner);
    if let Some(art) = art {
        wordmark(frame, banner_area, art, tui);
    }
    frame.render_widget(Paragraph::new(menu).centered(), menu_area);
}

fn menu_row(
    tui: &Tui,
    glyph: &str,
    label: &str,
    badge: Option<&str>,
    key: Option<char>,
    selected: bool,
) -> Line<'static> {
    let (row, glyph_style, label_style) = if selected {
        (tui.theme.selected(), tui.theme.selected(), tui.theme.selected())
    } else {
        (tui.theme.base(), tui.theme.muted(), tui.theme.base())
    };
    let badge = badge.map(|id| tui.i18n.t(id).to_uppercase()).unwrap_or_default();
    let badge_width = if badge.is_empty() { 0 } else { badge.chars().count() + 2 };
    let gap = (MENU_WIDTH - MENU_CHROME).saturating_sub(label.chars().count() + badge_width);

    let mut spans = vec![
        Span::styled(if selected { MARKER } else { " " }, tui.theme.accent()),
        Span::styled(format!(" {glyph} "), glyph_style),
    ];
    spans.extend(mnemonic_in(tui, label, key, label_style, selected));
    spans.push(Span::styled(" ".repeat(gap), row));
    if !badge.is_empty() {
        let role = if selected { tui.theme.accent } else { tui.theme.label };
        spans.push(Span::styled(format!(" {badge} "), tui.theme.badge(role)));
    }
    spans.push(Span::styled(format!(" {} ", key.unwrap_or(' ')), tui.theme.muted()));
    Line::from(spans)
}

/// The module's key letter lit inside its own name — `**F**ertilización`,
/// the prototype's mnemonic treatment. Case-insensitive on the first
/// occurrence, and it degrades to a plain label when the key is not in the
/// word at all: Home's `h` is nowhere in "Inicio", and inventing one would
/// be worse than the key hint already at the end of the row.
///
/// A selected row keeps one style throughout: the highlight already owns
/// its background, and a second colour inside it reads as damage.
fn mnemonic_in(tui: &Tui, label: &str, key: Option<char>, base: Style, selected: bool) -> Vec<Span<'static>> {
    let plain = vec![Span::styled(label.to_string(), base)];
    if selected {
        return plain;
    }
    let Some(key) = key else { return plain };
    let Some((at, matched)) = label.char_indices().find(|(_, c)| c.eq_ignore_ascii_case(&key)) else {
        return plain;
    };
    vec![
        Span::styled(label[..at].to_string(), base),
        Span::styled(matched.to_string(), tui.theme.title()),
        Span::styled(label[at + matched.len_utf8()..].to_string(), base),
    ]
}

/// What is actually loaded, counted from the data on disk rather than
/// assumed: the crop catalog, the source catalog, and whether the climate
/// adapter was built at all.
fn readiness(tui: &Tui) -> Line<'static> {
    let dot = Span::styled("◷ ", tui.theme.muted());
    let sep = Span::styled(" · ", tui.theme.muted());
    let (sources_id, sources_style) = if !tui.sources.is_empty() {
        ("launch_sources_ready", tui.theme.ok())
    } else {
        ("launch_sources_missing", tui.theme.error())
    };
    let (climate_id, climate_style) = if tui.climate.is_some() {
        ("launch_climate_on", tui.theme.ok())
    } else {
        ("launch_climate_off", tui.theme.muted())
    };
    Line::from(vec![
        dot,
        Span::styled(format!("{} ", tui.i18n.t("launch_crops")), tui.theme.muted()),
        Span::styled(tui.crops.len().to_string(), tui.theme.strong()),
        Span::styled(format!(" {}", tui.i18n.t("launch_crops_unit")), tui.theme.muted()),
        sep.clone(),
        Span::styled(format!("{} ", tui.i18n.t("launch_sources")), tui.theme.muted()),
        Span::styled(tui.i18n.t(sources_id).to_string(), sources_style),
        sep,
        Span::styled(format!("{} ", tui.i18n.t("launch_climate")), tui.theme.muted()),
        Span::styled(tui.i18n.t(climate_id).to_string(), climate_style),
    ])
}

/// The partial-coverage glyphs, which the rasteriser only emits along an
/// edge. Colouring them apart from the solid core is what reads as depth.
const DITHER: [char; 3] = ['░', '▒', '▓'];

/// Art rows are padded to a common width before centring: the paragraph
/// centres every line on its own, so ragged rows would shear the letters
/// apart. The subtitle is left short on purpose — it centres under the art.
fn wordmark(frame: &mut Frame, area: Rect, art: &[&str], tui: &Tui) {
    let width = art_width(art) as usize;
    let mut lines: Vec<Line> = art.iter().map(|row| art_line(row, width, tui)).collect();
    lines.push(Line::raw(""));
    if let Some(line) = subtitle(tui, area.width) {
        lines.push(Line::styled(line, tui.theme.muted()));
    }
    frame.render_widget(Paragraph::new(lines).centered(), area);
}

/// One row split into runs of core and fringe. Spaces go with the core:
/// they carry no ink, so their style never shows.
fn art_line<'a>(row: &str, width: usize, tui: &Tui) -> Line<'a> {
    let style = |fringe: bool| if fringe { tui.theme.muted() } else { tui.theme.title() };
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut fringe = false;
    for ch in format!("{row:<width$}").chars() {
        if DITHER.contains(&ch) != fringe && !run.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut run), style(fringe)));
        }
        fringe = DITHER.contains(&ch);
        run.push(ch);
    }
    spans.push(Span::styled(run, style(fringe)));
    Line::from(spans)
}

/// Can outrun the art it sits under — the monogram is only 39 columns, and
/// the plain-line tier 29 — so it gives up a segment at a time and then
/// itself. It is decoration: the statusline carries the version too, and a
/// subtitle that vetoed the name would be decoration deciding the title.
///
/// That was the bug: a 60-column panel had room for `N O N   N O B I S
/// S O L U M` and showed nothing at all, because the 36-character subtitle
/// under it did not fit.
fn subtitle(tui: &Tui, width: u16) -> Option<String> {
    let name = tui.i18n.t("app_subtitle").to_uppercase();
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let mut candidates = vec![format!("{name} · {version}")];
    if let Some(user) = current_user() {
        candidates.insert(0, format!("{name} · {version} · {}", user.to_uppercase()));
    }
    candidates.push(version);
    candidates.into_iter().find(|line| line.chars().count() <= width as usize)
}

/// `USER` is what a login shell sets, `LOGNAME` the POSIX spelling some
/// environments set instead. Neither: the segment is dropped.
fn current_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .filter(|name| !name.trim().is_empty())
}

fn art_width(art: &[&str]) -> u16 {
    art.iter().map(|row| row.chars().count()).max().unwrap_or(0) as u16
}

/// Art rows, a blank, the subtitle, and a blank to keep it off the menu.
fn banner_height(art: &[&str]) -> u16 {
    art.len() as u16 + 3
}

/// The widest tier the panel can hold whole, `None` if even the plain line
/// would be clipped. `reserved` is the room the menu needs below it.
fn pick_wordmark(inner: Rect, reserved: u16) -> Option<&'static [&'static str]> {
    WORDMARKS.iter().copied().find(|art| {
        // A column of air each side: art flush against the panel border
        // reads as clipped even when it is whole.
        inner.width >= art_width(art) + 2 && inner.height >= banner_height(art) + reserved
    })
}

// ---- stage 1 · soil ------------------------------------------------------

/// What the analysis *says*, before anything is applied: the readings
/// against their interpretation tables, the base balance, the lab results
/// on file, and the provenance of every reference the plan will use.
///
/// `SoilQualityAssessment` is the whole point of this stage and nothing
/// else in the TUI showed it. Everything below the readings is the old
/// inspect screen, unchanged.
fn soil(frame: &mut Frame, area: Rect, tui: &Tui) {
    let Some(inspection) = &tui.inspection else {
        return frame.render_widget(empty(tui, "no_inspection"), area);
    };
    let context = &inspection.field_context;
    let quality = &inspection.soil_quality;

    let zone = match quality.climate_zone {
        Some(zone) => Span::styled(zone.to_string(), tui.theme.strong()),
        None => Span::styled(tui.i18n.t("soil_no_zone").to_string(), tui.theme.muted()),
    };
    // Whether the efficiency grid covers this lot at all. A texture it has
    // no rules for is what makes `calculate` return `Err`, so saying it
    // here is saying it one stage before the failure.
    let (rules_id, rules_style) = if inspection.provenance.iter().any(|entry| entry.efficiency_range.is_some()) {
        ("soil_rules_ok", tui.theme.ok())
    } else {
        ("soil_rules_missing", tui.theme.warn())
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", context.field_id), tui.theme.title()),
            Span::styled(
                format!("· {} · {} · {}", context.texture, context.irrigation_system, context.region),
                tui.theme.muted(),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("{:<16}", tui.i18n.t("soil_zone")), tui.theme.muted()),
            zone,
            Span::raw("  ·  "),
            Span::styled(tui.i18n.t(rules_id).to_string(), rules_style),
        ]),
    ];

    for (title, readings) in
        [("soil_properties", &quality.properties), ("soil_ratios", &quality.cation_ratios)]
    {
        if readings.is_empty() {
            continue;
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(tui.i18n.t(title).to_string(), tui.theme.title()));
        lines.push(reading_header(tui));
        for reading in readings.iter() {
            lines.push(reading_row(tui, reading));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(tui.i18n.t("inspect_soil_tests").to_string(), tui.theme.title()));
    for test in &inspection.soil_tests {
        // `to_string()` first: the Display impls write straight to the
        // formatter, so a bare `{:<4}` would not pad.
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
            Some(removal) => {
                // A dash where the source table prints one: the two bases
                // are shown apart because filling either from the other is
                // the transcription error the two-column schema removed.
                let basis = |value: Option<f64>| value.map_or_else(|| "-".to_string(), |v| format!("{v}"));
                lines.push(Line::styled(
                    format!(
                        "      {} extr {} / abs {} kg/{} · {} {} · {} {}",
                        tui.i18n.t("inspect_removal"),
                        basis(removal.extraction_kg_per_unit),
                        basis(removal.absorption_kg_per_unit),
                        removal.harvested_organ,
                        tui.i18n.t("inspect_source"),
                        removal.source,
                        tui.i18n.t("inspect_year"),
                        removal.year
                    ),
                    tui.theme.muted(),
                ))
            }
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
                // The unit belongs beside the thresholds, exactly as it
                // does on the soil-test rows a few lines above: a K figure
                // of 0.3 is adequate in cmolc/kg and destitute in mg/kg,
                // and the reader has no other way to tell which is meant.
                format!(
                    "      {} {} / {} / {} {} [{}] · {} {} ({})",
                    tui.i18n.t("inspect_critical"),
                    level.low_threshold,
                    level.medium_threshold,
                    level.high_threshold,
                    level.unit,
                    level.extraction_method,
                    tui.i18n.t("inspect_source"),
                    level.source,
                    level.year
                ),
                tui.theme.muted(),
            ));
        }
    }

    frame.render_widget(Paragraph::new(lines).scroll((tui.scroll, 0)), area);
}

/// Column headings for the readings, laid out by hand rather than as a
/// `Table`: the whole stage is one scrollable page, and a table widget
/// would not scroll with the provenance under it.
///
/// No source column: it was a fourth column of `Castro_Gomez_2009_tabla12_…`
/// strings clipped mid-token, and the provenance block at the bottom of this
/// same page already names every reference in full.
fn reading_header<'a>(tui: &Tui) -> Line<'a> {
    Line::styled(
        format!(
            "  {:<20}{:>8}  {}",
            tui.i18n.t("col_parameter").to_uppercase(),
            tui.i18n.t("col_value").to_uppercase(),
            tui.i18n.t("col_category").to_uppercase(),
        ),
        tui.theme.muted(),
    )
}

/// One reading against its table. A value the table names no band for is
/// reported as unclassified rather than rounded into the nearest one —
/// see [`crate::core::domain::QualitativeBand`].
///
/// ponytail: the prototype draws a filled scale here. Placing a value on
/// one needs the band bounds, which `ScenarioInspection` does not carry;
/// the source of the verdict is shown instead. Add the gauge the day the
/// inspection reports the bands it classified against.
fn reading_row<'a>(tui: &Tui, reading: &crate::core::domain::PropertyAssessment) -> Line<'a> {
    let (category, style) = match &reading.category {
        Some(category) => (category.clone(), tui.theme.strong()),
        None => (tui.i18n.t("value_unclassified").to_string(), tui.theme.muted()),
    };
    Line::from(vec![
        Span::styled(format!("  {:<20}", reading.property.replace('_', " ")), tui.theme.muted()),
        Span::styled(format!("{:>8}", format!("{:.2}", reading.value)), tui.theme.accent()),
        Span::styled(format!("  {category}"), style),
    ])
}

// ---- stage 2 · crop ------------------------------------------------------

fn crops(frame: &mut Frame, area: Rect, tui: &Tui) {
    let [filter_area, table_area, note_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0), Constraint::Length(2)]).areas(area);

    let matches = tui.filtered_crops();
    let cursor = if tui.filtering { "█" } else { "" };
    let count = Line::styled(
        format!("{} / {} {}", matches.len(), tui.crops.len(), tui.i18n.t("crops_matches")),
        if matches.is_empty() { tui.theme.error() } else { tui.theme.ok() },
    );
    let [typed_area, count_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(count.width() as u16 + 1)]).areas(filter_area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("/ ", tui.theme.accent()),
            Span::styled(format!("{}{cursor}", tui.filter), tui.theme.strong()),
        ])),
        typed_area,
    );
    frame.render_widget(Paragraph::new(count).right_aligned(), count_area);
    frame.render_widget(
        Paragraph::new(Line::styled(format!(" ⏎ {}", tui.i18n.t("crops_override")), tui.theme.muted()))
            .wrap(Wrap { trim: true }),
        note_area,
    );

    if matches.is_empty() {
        return frame.render_widget(empty(tui, "no_crops"), table_area);
    }
    let rows: Vec<Row> = matches
        .iter()
        .map(|crop| {
            Row::new(vec![
                Cell::from(highlighted(tui, &crop.crop_id, tui.theme.accent())),
                Cell::from(highlighted(tui, &crop.name, tui.theme.base())),
                Cell::from(highlighted(tui, &crop.crop_type, tui.theme.base())),
                Cell::from(highlighted(tui, &crop.family, tui.theme.muted())),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [Constraint::Length(12), Constraint::Min(12), Constraint::Length(10), Constraint::Length(12)],
    )
    .header(header(tui, &["col_crop_id", "col_name", "col_type", "col_family"]))
    .highlight_symbol(Span::styled(MARKER, tui.theme.accent()))
    .row_highlight_style(tui.theme.selected());
    frame.render_stateful_widget(table, table_area, &mut TableState::default().with_selected(Some(tui.crop_idx)));
}

/// The filtered substring lit inside the cell that matched it, so a hit on
/// `familia` is visibly a hit on familia and not a row the filter let
/// through for some other reason.
///
/// Matching is on the lowercased haystack, so the span offsets are taken
/// from that and sliced out of the original — same bytes, since
/// `to_lowercase` is only applied to compare.
fn highlighted<'a>(tui: &Tui, text: &str, base: Style) -> Line<'a> {
    let needle = tui.filter.to_lowercase();
    let haystack = text.to_lowercase();
    match if needle.is_empty() { None } else { haystack.find(&needle) } {
        // A char boundary is guaranteed: `find` returns one, and the two
        // strings only differ where a case fold changed a character's
        // length, which `char_indices` below would then reject.
        Some(at) if text.is_char_boundary(at) && text.is_char_boundary(at + needle.len()) => Line::from(vec![
            Span::styled(text[..at].to_string(), base),
            // Bold accent, not a filled badge: on the selected row the
            // highlight already owns the background, and a second fill
            // inside it reads as damage.
            Span::styled(text[at..at + needle.len()].to_string(), tui.theme.title()),
            Span::styled(text[at + needle.len()..].to_string(), base),
        ]),
        _ => Line::from(Span::styled(text.to_string(), base)),
    }
}

// ---- stage 3 · goal ------------------------------------------------------

/// The yield goal, edited in place — never in a modal. `h`/`l` nudge it by
/// [`super::YIELD_STEP`], `e` types it, `m` switches the basis the demand
/// is read on.
fn target(frame: &mut Frame, area: Rect, tui: &Tui) {
    let Some(crop) = tui.active_crop() else {
        return frame.render_widget(empty(tui, "target_no_crop"), area);
    };
    let curated = tui.curated_yield_target().cloned();
    let typed = tui.typed_yield_target();
    let cursor = if tui.editing_yield { "█" } else { "" };
    let shown = if tui.editing_yield {
        tui.yield_input.clone()
    } else {
        typed
            .as_ref()
            .or(curated.as_ref())
            .map(|target| format!("{}", target.value))
            .unwrap_or_default()
    };

    // The cursor is drawn, not implied: a control the user cannot see they
    // are on is a control they will not try to change. It also disappears
    // when the module column has focus, because there the keys that would
    // move it go somewhere else — a cursor that does not answer to the
    // keyboard is the same false promise in miniature.
    let on = |row: usize| if tui.target_idx == row && !tui.focus_modules { "› " } else { "  " };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{:<12}", tui.i18n.t("st_crop")), tui.theme.muted()),
            Span::styled(crop, tui.theme.strong()),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(on(super::TARGET_ROW_GOAL), tui.theme.accent()),
            Span::styled(format!("{shown}{cursor}"), tui.theme.title()),
            Span::styled(format!(" {}", super::YIELD_UNIT), tui.theme.muted()),
            Span::styled(
                format!(
                    "   ({})",
                    tui.i18n.t(if typed.is_some() { "target_typed" } else { "target_curated" })
                ),
                tui.theme.muted(),
            ),
        ]),
    ];

    // Anchored on the curated goal because that is the only reference this
    // domain gives: 0 to twice it, with the curated value marked. A crop
    // with no curated row gets no scale rather than an invented one.
    if let Some(curated) = &curated {
        let value = typed.as_ref().map_or(curated.value, |target| target.value);
        lines.push(Line::raw(""));
        lines.push(slider(tui, value, curated.value * 2.0, curated.value));
    }
    // The hint sits under whichever control the cursor is on, so it always
    // describes the thing about to change rather than the screen in general.
    lines.push(Line::raw(""));
    if tui.target_idx == super::TARGET_ROW_GOAL {
        lines.push(Line::styled(
            format!("  ◂ h · l ▸  ±{}   ·   {}", super::YIELD_STEP, tui.i18n.t("target_type_hint")),
            tui.theme.muted(),
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(on(super::TARGET_ROW_BASIS), tui.theme.accent()),
        Span::styled(tui.i18n.t("target_method").to_string(), tui.theme.title()),
    ]));
    for (mode, note) in [
        (NutrientDemandMode::Extraction, "method_extraction_note"),
        (NutrientDemandMode::Absorption, "method_absorption_note"),
    ] {
        let active = tui.demand_mode == mode;
        lines.push(Line::from(vec![
            Span::styled(if active { "    ● " } else { "    ○ " }, tui.theme.accent()),
            Span::styled(
                format!("{:<14}", tui.i18n.t(demand_mode_id(mode))),
                if active { tui.theme.strong() } else { tui.theme.muted() },
            ),
            Span::styled(tui.i18n.t(note).to_string(), tui.theme.muted()),
        ]));
    }
    if tui.target_idx == super::TARGET_ROW_BASIS {
        lines.push(Line::styled(format!("  ◂ h · l ▸  {}", tui.i18n.t("target_basis_hint")), tui.theme.muted()));
    }

    // The way out, as a row rather than as a hint nobody reads.
    let next = super::stage_index(Screen::Target)
        .and_then(|index| super::STAGES.get(index + 1))
        .map(|(_, label)| tui.i18n.t(label))
        .unwrap_or_default();
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(on(super::TARGET_ROW_CONTINUE), tui.theme.accent()),
        Span::styled(
            format!("[ {} {next} ]", tui.i18n.t("target_continue")),
            if tui.target_idx == super::TARGET_ROW_CONTINUE { tui.theme.title() } else { tui.theme.muted() },
        ),
    ]));
    if tui.target_idx == super::TARGET_ROW_CONTINUE {
        lines.push(Line::styled(format!("  ◂ h · l ▸  {}", tui.i18n.t("target_continue_hint")), tui.theme.muted()));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// `curado 8 ┈┈┈●┈┈┈ 16` — the value's place on a scale, with the mark it
/// departs from named at the left end.
fn slider<'a>(tui: &Tui, value: f64, max: f64, anchor: f64) -> Line<'a> {
    const TRACK: usize = 24;
    let at = if max > 0.0 { ((value / max) * TRACK as f64).round().clamp(0.0, TRACK as f64) as usize } else { 0 };
    Line::from(vec![
        Span::styled(format!("  {} {anchor} ", tui.i18n.t("target_curated")), tui.theme.muted()),
        Span::styled("┈".repeat(at), Style::default().fg(tui.theme.border)),
        Span::styled("●", tui.theme.accent()),
        Span::styled("┈".repeat(TRACK - at), Style::default().fg(tui.theme.border)),
        Span::styled(format!(" {max}"), tui.theme.muted()),
    ])
}

// ---- stage 4 · sources ---------------------------------------------------

/// Which product covers each nutrient, at what grade and what dose — the
/// product half of the same [`FertilityPlan`] whose balance stage ⑤ shows.
fn sources(frame: &mut Frame, area: Rect, tui: &Tui) {
    let Some(plan) = &tui.plan else {
        return frame.render_widget(empty(tui, "no_plan"), area);
    };

    let rows: Vec<Row> = plan
        .nutrient_results
        .iter()
        .map(|entry| {
            let none = tui.i18n.t("value_none").to_string();
            let (product, grade, dose) = match &entry.dose {
                Some(dose) => (
                    dose.source_name.clone(),
                    grade_of(tui, &dose.source_id),
                    format!("{:.0} kg/ha", dose.kg_product_per_ha),
                ),
                // A nutrient with nothing left to apply has no product for
                // the same reason it has no dose, and saying "no source
                // carries it" there would be a different, false claim —
                // that one is the note below the table.
                None => (none.clone(), String::new(), none),
            };
            Row::new(vec![
                Cell::from(Span::styled(entry.nutrient.to_string(), tui.theme.accent())),
                Cell::from(format!("{:.0}", entry.net_requirement_kg_ha)),
                Cell::from(Span::styled(product, tui.theme.base())),
                Cell::from(Span::styled(grade, tui.theme.muted())),
                Cell::from(Span::styled(dose, tui.theme.strong())),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(9),
            Constraint::Min(14),
            Constraint::Length(11),
            Constraint::Length(10),
        ],
    )
    .header(header(tui, &["col_nutrient", "col_net", "col_source", "col_grade", "col_dose_amount"]));

    let mut lines = vec![Line::raw(""), coverage_line(tui, plan)];
    // Only where it is actually true: something is needed and the catalog
    // has nothing that carries it.
    let uncovered: Vec<String> = plan
        .nutrient_results
        .iter()
        .filter(|entry| entry.dose.is_none() && entry.net_requirement_kg_ha > 0.0)
        .map(|entry| entry.nutrient.to_string())
        .collect();
    if !uncovered.is_empty() {
        lines.push(Line::styled(
            format!("{}: {}", uncovered.join(" · "), tui.i18n.t("sources_no_dose")),
            tui.theme.warn(),
        ));
    }
    if let Some(liming) = &plan.liming {
        let material = liming
            .material
            .as_ref()
            .map(|dose| format!("{} · {:.1} t/ha", dose.source_name, dose.t_product_per_ha))
            .unwrap_or_else(|| format!("{:.1} t/ha CaCO₃", liming.recommended_t_ha));
        lines.push(Line::from(vec![
            Span::styled(format!("{}  ", tui.i18n.t("st_liming")), tui.theme.warn()),
            Span::styled(material, tui.theme.strong()),
            Span::styled(
                format!(
                    "  ({} {:.0}% → {} {:.0}%)",
                    tui.i18n.t("st_base_saturation"),
                    liming.current_base_saturation_pct,
                    tui.i18n.t("st_saturation_goal"),
                    liming.target_base_saturation_pct
                ),
                tui.theme.muted(),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    lines.extend(recommendation_lines(tui));

    lines.push(Line::raw(""));
    lines.push(Line::styled(tui.i18n.t("sources_micro").to_string(), tui.theme.title()));
    if plan.micronutrients.is_empty() {
        lines.push(Line::styled(format!("  {}", tui.i18n.t("sources_micro_none")), tui.theme.muted()));
    }
    for micro in &plan.micronutrients {
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<4}", micro.nutrient.to_string()), tui.theme.accent()),
            Span::styled(format!("{:>8.2} {:<12}", micro.soil_value, micro.unit), tui.theme.base()),
            soil_status_span(tui, Some(micro.soil_status)),
            Span::styled(
                match &micro.dose {
                    Some(dose) => format!("  {} · {:.1} kg/ha", dose.source_name, dose.kg_product_per_ha),
                    None => String::new(),
                },
                tui.theme.strong(),
            ),
        ]));
    }

    // The table takes what it needs, the notes take the rest.
    let table_height = (plan.nutrient_results.len() as u16 + 2).min(area.height);
    let [table_area, notes] =
        Layout::vertical([Constraint::Length(table_height), Constraint::Min(0)]).areas(area);
    frame.render_widget(table, table_area);
    frame.render_widget(Paragraph::new(lines).scroll((tui.scroll, 0)), notes);
}

fn strategy_label(strategy: FertilizationStrategy) -> &'static str {
    match strategy {
        FertilizationStrategy::CompositePlusSimple => "strategy_composite",
        FertilizationStrategy::SimpleBlendOnly => "strategy_blend",
    }
}

/// The commercial half of stage ④: which bags, how many, and how the
/// balance above is met by them.
///
/// One product per line, exactly as the report and the CLI print it — the
/// figures are read off the same [`report_renderer`] the PDF uses, so the
/// screen and the exported page cannot disagree.
fn recommendation_lines<'a>(tui: &Tui) -> Vec<Line<'a>> {
    let Some(report) = &tui.recommendation else {
        return vec![Line::styled(tui.i18n.t("sources_no_program").to_string(), tui.theme.muted())];
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{}  ", tui.i18n.t("sources_program")), tui.theme.title()),
        Span::styled(
            format!(
                "{} · {:.1} ha · {:.0} kg/{}",
                tui.i18n.t(strategy_label(report.scenario.strategy)),
                report.scenario.total_area_ha,
                report.scenario.bag_weight_kg,
                tui.i18n.t("unit_bag")
            ),
            tui.theme.muted(),
        ),
    ])];
    for line in report_renderer::render_summary(report).into_iter().skip(3) {
        lines.push(Line::styled(line, tui.theme.base()));
    }
    lines
}

/// The composition behind a dose, read off the catalog the active profile
/// actually shipped rather than parsed out of the product's name.
fn grade_of(tui: &Tui, source_id: &str) -> String {
    tui.sources
        .iter()
        .find(|source| source.source_id == source_id)
        .map(|source| {
            source
                .composition_pct
                .iter()
                .map(|(nutrient, pct)| format!("{nutrient} {pct:.0}"))
                .collect::<Vec<_>>()
                .join("·")
        })
        .unwrap_or_default()
}

fn coverage_line<'a>(tui: &Tui, plan: &FertilityPlan) -> Line<'a> {
    let mut spans = vec![Span::styled(format!("{}  ", tui.i18n.t("sources_coverage")), tui.theme.muted())];
    for entry in &plan.nutrient_results {
        let covered = entry.dose.is_some();
        spans.push(Span::styled(
            format!("{}{} ", entry.nutrient, if covered { "✓" } else { "·" }),
            if covered { tui.theme.ok() } else { tui.theme.muted() },
        ));
    }
    Line::from(spans)
}

// ---- stage 5 · plan ------------------------------------------------------

/// The nutrient balance: what the crop asks for, what the soil already
/// gives, and what is left to apply. The product behind each figure lives
/// one stage back, on ④ — the prototype's plan table is six columns wide
/// and must not overflow the panel.
fn plan(frame: &mut Frame, area: Rect, tui: &Tui) {
    let Some(plan) = &tui.plan else {
        return frame.render_widget(empty(tui, "no_plan"), area);
    };

    let rows: Vec<Row> = plan
        .nutrient_results
        .iter()
        .map(|entry| {
            Row::new(vec![
                Cell::from(Span::styled(entry.nutrient.to_string(), tui.theme.accent())),
                Cell::from(format!("{:.0}", entry.demand_kg_ha)),
                Cell::from(format!("{:.0}", entry.availability_kg_ha)),
                Cell::from(soil_status_span(tui, entry.soil_status)),
                Cell::from(format!("{:.0}%", entry.efficiency_used * 100.0)),
                Cell::from(Span::styled(format!("{:.0}", entry.net_requirement_kg_ha), tui.theme.strong())),
                Cell::from(bar(tui, entry.net_requirement_kg_ha, entry.demand_kg_ha, 8)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Length(9),
            Constraint::Min(8),
        ],
    )
    .header(header(
        tui,
        &[
            "col_nutrient",
            "col_demand",
            "col_availability",
            "col_soil_status",
            "col_efficiency",
            "col_net",
            "col_balance",
        ],
    ));

    let mut notes = vec![climate_line(tui, plan)];
    for warning in &plan.warnings {
        notes.push(Line::styled(format!(" {}", warning_text(tui, warning)), tui.theme.warn()));
    }

    let table_height = (plan.nutrient_results.len() as u16 + 2).min(area.height);
    let [table_area, note_area] =
        Layout::vertical([Constraint::Length(table_height), Constraint::Min(0)]).areas(area);
    frame.render_widget(table, table_area);
    frame.render_widget(Paragraph::new(notes).wrap(Wrap { trim: true }).scroll((tui.scroll, 0)), note_area);
}

/// Warnings are the plan's own words about numbers it decided to show
/// anyway; the reader has to see them next to the table, not instead of it.
fn warning_text(tui: &Tui, warning: &crate::core::domain::PlanWarning) -> String {
    use crate::core::domain::PlanWarning;
    match warning {
        PlanWarning::FallbackToAbsorption { nutrient, net_requirement_kg_ha } => format!(
            "{nutrient}: {} → {} ({net_requirement_kg_ha:.0} kg/ha)",
            tui.i18n.t("method_extraction"),
            tui.i18n.t("method_absorption")
        ),
        PlanWarning::NoRemovalCoefficient { nutrient } => {
            format!("{nutrient}: {}", tui.i18n.t("inspect_no_removal"))
        }
    }
}

/// Which regime produced the N numbers above. The mineralization factor
/// alone moves N availability by up to 3x, so the reader must never have
/// to guess — the rule the CLI output follows too.
fn climate_line<'a>(tui: &Tui, plan: &FertilityPlan) -> Line<'a> {
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

fn settings(frame: &mut Frame, area: Rect, tui: &Tui) {
    // Two borders plus the label column below, so a value set knows how
    // much room it actually has.
    let value_width = area.width.saturating_sub(29) as usize;
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
                    toggle(tui, &names, &active, value_width)
                }
                "settings_theme" => {
                    let names: Vec<String> = theme::THEMES.iter().map(|t| t.name.to_string()).collect();
                    toggle(tui, &names, tui.theme.name, value_width)
                }
                "settings_profile" => toggle(tui, &tui.profiles, &tui.cfg.profile, value_width),
                "settings_strategy" => {
                    let names: Vec<String> = FertilizationStrategy::ALL
                        .iter()
                        .map(|strategy| tui.i18n.t(strategy_label(*strategy)).to_string())
                        .collect();
                    let active = tui.i18n.t(strategy_label(tui.formulation.strategy)).to_string();
                    toggle(tui, &names, &active, value_width)
                }
                "settings_area" => {
                    vec![Span::styled(format!("{:.1} ha", tui.formulation.total_area_ha), tui.theme.strong())]
                }
                "settings_bag" => {
                    let names: Vec<String> = BAG_WEIGHTS_KG.iter().map(|kg| format!("{kg:.0} kg")).collect();
                    toggle(tui, &names, &format!("{:.0} kg", tui.formulation.bag_weight_kg), value_width)
                }
                // Marked, because they sit in the same list as the rows
                // that do change: a cursor that lands somewhere `h`/`l`
                // does nothing has to say why.
                "settings_data_root" => read_only(tui, tui.cfg.data_root.display().to_string()),
                "settings_reference_dir" => read_only(tui, tui.cfg.reference_dir().display().to_string()),
                _ => read_only(tui, tui.cfg.curated_dir().display().to_string()),
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

/// A settings value the screen shows but cannot change.
fn read_only<'a>(tui: &Tui, value: String) -> Vec<Span<'a>> {
    vec![
        Span::styled(value, tui.theme.muted()),
        Span::styled(format!("  ({})", tui.i18n.t("settings_read_only")), tui.theme.muted()),
    ]
}

/// Sized to its contents up to half the screen, so a short list isn't a
/// huge empty box and a long one scrolls instead of overflowing.
fn picker_overlay(frame: &mut Frame, tui: &Tui) {
    let Some(picker) = &tui.picker else { return };
    let label = tui
        .form
        .as_ref()
        .and_then(|form| form.fields.get(picker.field_idx))
        .map(|field| tui.i18n.t(field.label))
        .unwrap_or_default();

    // Labels, not values: a value can be an absolute path, and six of those
    // in a narrow list are six identical rows.
    let items: Vec<ListItem> = picker
        .labels
        .iter()
        .map(|label| {
            let shown = if label.is_empty() { tui.i18n.t("picker_none") } else { label.as_str() };
            ListItem::new(Line::raw(format!(" {shown}")))
        })
        .collect();

    // The frame carries the directory when there is one, so "where am I"
    // never has to be inferred from the rows.
    let title = if picker.title.is_empty() { label.to_string() } else { picker.title.clone() };
    // Wide enough for a real path in the frame; the rows themselves are
    // basenames and need far less.
    let width = (title.chars().count() as u16 + 6).clamp(40, frame.area().width.saturating_sub(4)).max(40);
    let height = (items.len() as u16 + 2).min(frame.area().height.saturating_sub(4)).max(3);
    let area = centered(frame.area(), width, height);
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(
        List::new(items).block(panel(title, true, tui)).highlight_style(tui.theme.selected()),
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
        Screen::Crops => keys.insert(0, ("/", "help_filter")),
        Screen::Target => {
            keys.insert(0, ("h/l · ←/→", "help_step"));
            keys.insert(1, ("e", "help_edit_goal"));
            keys.insert(2, ("m", "help_method"));
        }
        Screen::Settings => keys.insert(0, ("h/l · ←/→", "help_change")),
        Screen::NewLot | Screen::EditLot | Screen::NewSample | Screen::Import => {
            keys.insert(0, ("Enter", if tui.picker.is_some() { "help_pick" } else { "help_edit" }))
        }
        _ => {}
    }
    // Every stage but the goal keeps h/l for the flow itself.
    if stage_index(tui.screen).is_some_and(|_| tui.screen != Screen::Target) {
        keys.insert(0, ("h/l · ←/→", "help_stage"));
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

/// What the statusline calls the current screen: a stage goes by its
/// stepper label, everything else by its own title.
fn short_title(tui: &Tui) -> &str {
    match stage_index(tui.screen) {
        Some(index) => tui.i18n.t(STAGES[index].1),
        None => screen_title(tui),
    }
}

fn screen_title(tui: &Tui) -> &str {
    tui.i18n.t(screen_title_id(tui.screen))
}

fn screen_title_id(screen: Screen) -> &'static str {
    match screen {
        Screen::Dashboard => "lots",
        Screen::Soil => "soil_title",
        Screen::Crops => "crops_title",
        Screen::Target => "target_title",
        Screen::Sources => "sources_title",
        Screen::Plan => "plan_title",
        Screen::NewLot => "form_new_lot_title",
        Screen::EditLot => "form_edit_lot_title",
        Screen::Import => "form_import_title",
        Screen::NewSample => "form_new_sample_title",
        Screen::Settings => "settings_title",
    }
}

/// Uppercase, not dimmed: dimming them deletes them on a terminal with a
/// wallpaper.
fn header<'a>(tui: &Tui, ids: &[&str]) -> Row<'a> {
    Row::new(ids.iter().map(|id| Cell::from(tui.i18n.t(id).to_uppercase())).collect::<Vec<_>>())
        .style(tui.theme.muted())
        .bottom_margin(1)
}

/// `label   value`, emphasis on the value.
fn field<'a>(tui: &Tui, id: &str, value: String) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{:<18}", tui.i18n.t(id)), tui.theme.muted()),
        Span::styled(value, tui.theme.strong()),
    ])
}

fn empty<'a>(tui: &Tui, id: &str) -> Paragraph<'a> {
    Paragraph::new(Line::styled(format!(" {}", tui.i18n.t(id)), tui.theme.muted()))
}

/// The current value as a filled chip among the others, with `▸` carrying
/// the same information in glyph form — the row highlight patches over
/// both colours, and the selected row is exactly the one whose active
/// value has to stay readable.
///
/// A set that does not fit collapses to the active value between cycle
/// arrows. Themes and reference profiles are both open-ended lists, and a
/// clipped one reads as a truncated word rather than as "there is more".
fn toggle(tui: &Tui, options: &[String], active: &str, width: usize) -> Vec<Span<'static>> {
    let spelled_out: usize = options.iter().map(|option| option.chars().count() + 2).sum();
    if spelled_out > width {
        return vec![
            Span::styled("◂", tui.theme.muted()),
            Span::styled(format!(" {active} "), tui.theme.badge(tui.theme.accent)),
            Span::styled("▸", tui.theme.muted()),
        ];
    }
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

/// The two halves differ in weight, not just in glyph.
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
    use super::super::i18n::I18n;
    use crate::infra::bootstrap;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Row-major, so anything fitting on one line stays contiguous.
    fn render(tui: &Tui, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|frame| draw(frame, tui)).expect("draw");
        terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect()
    }

    /// Every id the chrome asks for by `match` rather than from a table,
    /// in both bundles.
    ///
    /// `I18n::t` returns the id when there is no string for it, which is
    /// the right failure — but nothing was checking, and deleting the
    /// module menu left `Screen::Dashboard` asking for a `module_home` that
    /// no longer existed. The statusline read `▸ module_home` for a whole
    /// session before anyone noticed.
    #[test]
    fn no_screen_names_itself_with_a_string_that_does_not_exist() {
        for language in [Language::English, Language::Spanish] {
            let i18n = I18n::new(language);
            for screen in [
                Screen::Dashboard,
                Screen::Soil,
                Screen::Crops,
                Screen::Target,
                Screen::Sources,
                Screen::Plan,
                Screen::NewLot,
                Screen::EditLot,
                Screen::NewSample,
                Screen::Import,
                Screen::Settings,
            ] {
                for id in [screen_label(screen), screen_title_id(screen)] {
                    assert_ne!(i18n.t(id), id, "{screen:?} in {language:?} has no string for `{id}`");
                }
            }
            for (label, _, _) in LOT_ACTIONS {
                assert_ne!(i18n.t(label), label, "`{label}` in {language:?} has no string");
            }
            for (_, label) in STAGES {
                assert_ne!(i18n.t(label), label, "`{label}` in {language:?} has no string");
            }
        }
    }

    /// Any tier counts: which one `pick_wordmark` lands on is its own
    /// business, and a test that named one would break on a new size.
    fn shows_a_wordmark(out: &str) -> bool {
        WORDMARKS.iter().any(|art| out.contains(art[0]))
    }

    #[test]
    fn every_screen_renders_at_both_densities() {
        let mut tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);
        tui.open(Some(Screen::Plan));
        assert!(tui.plan.is_some(), "LOT-001/corn/global should plan: {}", tui.message);
        tui.open(Some(Screen::Soil));
        assert!(tui.inspection.is_some(), "LOT-001 should inspect: {}", tui.message);

        for screen in [
            Screen::Dashboard,
            Screen::Soil,
            Screen::Crops,
            Screen::Target,
            Screen::Sources,
            Screen::Plan,
            Screen::NewLot,
            Screen::NewSample,
            Screen::Settings,
        ] {
            match screen {
                Screen::NewLot | Screen::EditLot | Screen::NewSample | Screen::Import => tui.open_form(screen),
                _ => tui.screen = screen,
            }
            // 80x24 drops the status column; 130x40 shows all three. The
            // navigation is the lot column now, and Home is the exception:
            // it owns the whole body, so it carries its own way in.
            for (width, height) in [(80, 24), (130, 40)] {
                let out = render(&tui, width, height);
                let column = tui.i18n.t("lots").to_uppercase();
                if screen == Screen::Dashboard {
                    assert!(
                        out.contains(&tui.i18n.t("action_plan").to_string()),
                        "Home at {width}x{height} lost its menu"
                    );
                    assert!(!out.contains(&column), "and the launcher owns the whole body");
                } else {
                    assert!(out.contains(&column), "{screen:?} at {width}x{height} lost its navigation");
                }
            }
        }

        // Tall enough for the whole page: both stages scroll, and the
        // block each is checked for sits at its very bottom, so a terminal
        // that fits everything is what proves it is rendered rather than
        // dropped.
        tui.screen = Screen::Soil;
        let soil = render(&tui, 130, 60);
        assert!(soil.contains("Reference provenance"), "the provenance block must survive the move to stage 1");
        assert!(soil.contains(&tui.i18n.t("soil_properties").to_uppercase()) || soil.contains("Reading"));

        tui.screen = Screen::Sources;
        let sources = render(&tui, 130, 60);
        assert!(sources.contains("Micronutrients"), "the micronutrient rows must stay visible");
        assert!(sources.contains(tui.i18n.t("sources_coverage")), "and the coverage line with them");

        tui.screen = Screen::Plan;
        tui.help = true;
        assert!(render(&tui, 80, 24).contains("KEYBINDINGS"), "the help overlay must fit an 80x24 terminal");
    }

    /// The launcher answers "is this thing loaded?" before anything is
    /// curated, so a fresh install opens on the wordmark, the menu and a
    /// real count rather than on an empty table.
    #[test]
    fn the_launcher_stands_up_with_nothing_curated() {
        let mut tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);
        tui.lots.clear();
        tui.screen = Screen::Dashboard;

        let out = render(&tui, 130, 40);

        assert!(shows_a_wordmark(&out), "the launcher is where the title most belongs");
        assert!(out.contains(&tui.i18n.t("action_plan").to_string()), "and a way in");
        assert!(out.contains(&tui.crops.len().to_string()), "and the crop count must be the real one");
        assert!(
            out.contains(&tui.i18n.t("launch_sources_ready").to_string()),
            "shipped reference data means the sources report ready"
        );
    }

    /// Everything the bar carries is useful except the version, which the
    /// wordmark's subtitle repeats anyway — so it is the first thing to go
    /// when the terminal narrows, ahead of the keybindings.
    #[test]
    fn the_bar_gives_up_its_version_before_its_keybindings() {
        let mut tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);
        // A screen with no wordmark, so the only version on screen is the
        // bar's own.
        tui.screen = Screen::Crops;

        let roomy = render(&tui, 150, 24);
        assert!(roomy.contains(env!("CARGO_PKG_VERSION")), "a wide terminal has room for both");
        assert!(roomy.contains(tui.i18n.t("hint_crops")), "including the keys");

        let tight = render(&tui, 104, 24);
        assert!(tight.contains(tui.i18n.t("hint_crops")), "keys outlast decoration");
        assert!(!tight.contains(env!("CARGO_PKG_VERSION")), "the version goes first");
    }

    /// The user got stuck on stage ③: it is the only stage where `h`/`l`
    /// change a control instead of walking the stepper, so pressing `l` to
    /// move on nudged the yield goal instead. The exit is a row now.
    #[test]
    fn the_goal_stage_shows_the_way_out_and_walks_it_with_the_usual_keys() {
        let mut tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);
        tui.crop_override = Some("corn".to_string());
        tui.enter(Screen::Target);

        let screen = render(&tui, 100, 30);
        assert!(screen.contains(tui.i18n.t("target_continue")), "the exit has to be on screen:\n{screen}");
        assert!(screen.contains(tui.i18n.t("stage_sources")), "and name where it goes:\n{screen}");

        // j/k reaches it, and there `l` does what it does everywhere else.
        tui.move_selection(1);
        tui.move_selection(1);
        assert_eq!(tui.target_idx, super::super::TARGET_ROW_CONTINUE);
        let goal_before = tui.yield_input.clone();
        tui.change_target(1);
        assert_eq!(tui.screen, Screen::Sources, "l on the exit row advances instead of nudging a number");
        assert_eq!(tui.yield_input, goal_before, "and leaves the goal alone");
    }

    /// What the user actually sees when they press `i`: the browser open on
    /// the shipped examples, with names they can tell apart.
    #[test]
    fn the_import_screen_opens_on_the_examples_and_names_them() {
        let mut tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);
        tui.open_form(Screen::Import);
        tui.activate_form_row();

        let screen = render(&tui, 120, 30);
        for name in ["lots.csv", "soil_tests.csv", "yield_targets.csv"] {
            assert!(screen.contains(name), "the browser has to name {name}:\n{screen}");
        }
        // `panel` upper-cases its title, so match the way it is drawn.
        assert!(screen.to_lowercase().contains("examples"), "and say which folder it is showing:\n{screen}");
        assert!(screen.contains("../"), "and offer the way out of it");
        // The regression this replaced: six rows truncated to one prefix.
        assert!(
            !screen.contains("/home") || screen.matches("/home").count() <= 2,
            "rows are names, not absolute paths:\n{screen}"
        );
    }

    /// The banner may shrink a tier or vanish, but never gets clipped.
    #[test]
    fn the_wordmark_steps_down_a_tier_at_a_time_before_it_disappears() {
        let tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);

        // Every number here is a layout *budget*, measured against the real
        // render rather than reasoned about: the banner competes with the
        // menu below it for the whole body, so both move whenever either
        // does. A failure here is the test doing its job — re-measure the
        // ladder, don't just bump the number.
        assert!(render(&tui, 150, 40).contains(WORDMARK_WIDE[0]), "a roomy terminal gets the whole name across");
        // The one-liner is only 8 rows: a wide terminal that is short still
        // says the whole name.
        assert!(render(&tui, 150, 28).contains(WORDMARK_WIDE[0]), "height is not what the one-liner needs");

        // Too narrow for one line, tall enough to break it over two.
        let broken = render(&tui, 100, 36);
        assert!(broken.contains(WORDMARK_LOCKUP[0]), "the lockup takes the width the one-liner can't have");
        assert!(!broken.contains(WORDMARK_WIDE[0]), "and the one-liner stands down instead of clipping");

        // Narrower still, but tall enough to stack the name three deep
        // instead — which is the whole point of having tiers.
        let narrow = render(&tui, 60, 42);
        assert!(narrow.contains(WORDMARK_TALL[0]), "the stack takes the space the lockup can't use");
        assert!(!narrow.contains(WORDMARK_LOCKUP[0]), "and the lockup stands down in turn");

        // Same width, too short to stack: the monogram is the next rung,
        // which is what a small terminal is meant to land on.
        let squat = render(&tui, 60, 33);
        assert!(squat.contains(WORDMARK_MARK[0]), "the monogram fits where the stack cannot");
        assert!(!squat.contains(WORDMARK_TALL[0]), "and the stack stands down in turn");

        // Narrower than the monogram: the block-letter mark is the floor of
        // the art, and blackletter gives way to it rather than being scaled
        // into mush.
        let small = render(&tui, 40, 42);
        assert!(small.contains(WORDMARK_SMALL[0]), "a small panel is what a mark is for");
        assert!(!small.contains(WORDMARK_MARK[0]), "and it is reached by standing the monogram down");

        // Wide but short, which is the other way to be out of room.
        let squat_wide = render(&tui, 150, 25);
        assert!(squat_wide.contains(WORDMARK_SMALL[0]), "height runs out before width does here");

        // The floor: too short for the mark, still room to say the name.
        let floor = render(&tui, 150, 21);
        assert!(floor.contains(WORDMARK_LINE[0]), "the tight line is the floor");
        assert!(!floor.contains(WORDMARK_SMALL[0]), "and the mark stands down in turn");

        // Too short for even that: the menu is what Home is *for*, so it
        // takes the rows and the banner goes rather than being clipped.
        assert!(!shows_a_wordmark(&render(&tui, 150, 16)), "the menu wins the last rows, not the art");
    }

    /// The longest list must scroll inside the overlay rather than run off
    /// an 80x24 terminal.
    #[test]
    fn the_unfolded_option_list_renders_over_the_form() {
        let mut tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);
        tui.open_form(Screen::NewLot);

        for label in ["form_irrigation", "form_crop"] {
            let form = tui.form.as_mut().expect("form");
            form.idx = form.fields.iter().position(|field| field.label == label).expect("field");
            tui.activate_form_row();
            assert!(tui.picker.is_some(), "{label} must unfold a list");
            for (width, height) in [(80, 24), (130, 40)] {
                let out = render(&tui, width, height);
                assert!(
                    out.contains(&tui.i18n.t("lots").to_uppercase()),
                    "{label} at {width}x{height} lost the lot column"
                );
            }
            tui.picker = None;
        }

        // Texture proves the entries are painted, not just the frame: with
        // 12 options the hint line shows only a count, so a texture name on
        // screen can only have come from the overlay.
        let form = tui.form.as_mut().expect("form");
        form.idx = form.fields.iter().position(|field| field.label == "form_texture").expect("field");
        tui.activate_form_row();
        assert!(render(&tui, 80, 24).contains("silty_clay_loam"), "the option list must render its entries");
    }

    /// The filter highlight slices the original string at offsets found in
    /// its lowercased copy. That holds for every case fold that keeps a
    /// character's byte length — and must degrade to a plain label, never
    /// panic, for the ones that do not.
    #[test]
    fn the_filter_highlight_lights_the_match_and_never_splits_a_character() {
        let mut tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);
        let text = |line: Line| line.spans.iter().map(|span| span.content.to_string()).collect::<String>();

        assert_eq!(highlighted(&tui, "Coffee", tui.theme.base()).spans.len(), 1, "no filter, no split");

        tui.filter = "FF".to_string();
        let lit = highlighted(&tui, "Coffee", tui.theme.base());
        assert_eq!(lit.spans.len(), 3, "case-insensitive, and the match is its own span");
        assert_eq!(lit.spans[1].content, "ff");
        assert_eq!(text(lit), "Coffee", "the cell still reads the same");

        // Accented: `to_lowercase` keeps the byte length here, so the
        // offsets stay valid and the match is still lit.
        tui.filter = "café".to_string();
        assert_eq!(text(highlighted(&tui, "Café · arábica", tui.theme.base())), "Café · arábica");

        // And one whose fold does not keep it: the label survives whole.
        tui.filter = "i̇".to_string();
        let odd = highlighted(&tui, "İstanbul", tui.theme.base());
        assert_eq!(text(odd), "İstanbul", "an unslicable match falls back to a plain label");
    }

    #[test]
    fn bar_is_proportional_and_never_overflows() {
        let tui = Tui::new(bootstrap::build_app_from_repo_data(), crate::infra::tui_settings::TuiSettings::default(), None);
        // Different styles per half, so assert on the painted glyphs.
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





