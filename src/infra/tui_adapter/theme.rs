//! Terminal-adaptive palette.
//!
//! The "Estrato" direction lives in the *structure* — rounded tiles, framed
//! bars, filled mode badges, the `▎` selection rule — not in a fixed set of
//! hex values. So no colour here is absolute: backgrounds are
//! [`Color::Reset`], which keeps whatever the terminal draws (including
//! transparency and a background image), and every semantic role is an ANSI
//! slot the user's own colour scheme defines.
//!
//! The role map reads copper as amber (yellow slot) and sage-teal as cyan,
//! which is as close as sixteen slots get to the prototype.
//!
//! One rule holds the whole thing together: **only [`Theme::border`] may be
//! faint, and only structure may use it.** Text ranks by hue and emphasis
//! (label vs value, bold, accent, reverse), never by fading, because this
//! palette does not know what it is being drawn on top of.

use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::{theme_mode, QueryOptions, ThemeMode};

pub struct Theme {
    /// Painted over the whole frame before anything else. `Reset` on both
    /// bundles: the gaps between tiles show the terminal, on purpose.
    pub bg: Color,
    /// Inside a panel. Also `Reset` — a terminal-driven palette has no
    /// second background to lift a tile with, and inventing one is what
    /// would paint over a configured transparency.
    pub panel: Color,
    /// **Structure only**: borders, separators, the unfilled half of a bar.
    /// Never a character a reader has to make out. Slot 8 is the one slot
    /// whose contrast against the background is anybody's guess — a
    /// terminal with a wallpaper can swallow it whole — so nothing that
    /// carries meaning is allowed to depend on it.
    pub border: Color,
    pub fg: Color,
    /// Everything that *introduces* a value rather than being one: field
    /// labels, column headings, key hints, mnemonics, paths, provenance.
    /// A colour rather than the body foreground, because a screen where
    /// label and value look identical reads as one grey block — and a
    /// colour rather than a dimmer grey, because faint is what made them
    /// unreadable in the first place.
    pub label: Color,
    /// Focus and nothing else — never decoration.
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
}

/// Bright ANSI slots: on a dark background the normal ones are too dim.
pub const DARK_THEME: Theme = Theme {
    bg: Color::Reset,
    panel: Color::Reset,
    border: Color::Indexed(8),
    fg: Color::Reset,
    label: Color::Indexed(12),
    accent: Color::Indexed(11),
    ok: Color::Indexed(14),
    // The normal slot, so "medium" soil status stays a step below the
    // accent instead of competing with it.
    warn: Color::Indexed(3),
    error: Color::Indexed(9),
};

/// Normal ANSI slots: on a light background the bright ones wash out.
pub const LIGHT_THEME: Theme = Theme {
    bg: Color::Reset,
    panel: Color::Reset,
    border: Color::Indexed(8),
    fg: Color::Reset,
    label: Color::Indexed(4),
    accent: Color::Indexed(3),
    ok: Color::Indexed(6),
    warn: Color::Indexed(3),
    error: Color::Indexed(1),
};

/// Asks the terminal for its background (OSC 11) and picks the bundle by
/// perceived lightness. Must run *before* the alternate screen is entered,
/// while the query/response handshake still owns the tty. Terminals that
/// don't answer fall back to the dark bundle.
pub fn detect() -> &'static Theme {
    match theme_mode(QueryOptions::default()) {
        Ok(ThemeMode::Light) => &LIGHT_THEME,
        _ => &DARK_THEME,
    }
}

impl Theme {
    /// Body text inside a panel: whatever the terminal already draws.
    pub fn base(&self) -> Style {
        Style::default().fg(self.fg).bg(self.panel)
    }

    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn title(&self) -> Style {
        Style::default().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    /// Secondary text: labels, hints, column headings, provenance. Ranked
    /// below the value it introduces by *hue*, never by fading — a faded
    /// glyph on an unknown background is a glyph nobody can read.
    pub fn muted(&self) -> Style {
        Style::default().fg(self.label)
    }

    /// The value a label introduces.
    pub fn strong(&self) -> Style {
        Style::default().fg(self.fg).add_modifier(Modifier::BOLD)
    }

    pub fn error(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn ok(&self) -> Style {
        Style::default().fg(self.ok)
    }

    pub fn warn(&self) -> Style {
        Style::default().fg(self.warn)
    }

    /// Selected row. Reverse video rather than a named background: it is
    /// the one highlight guaranteed to be legible against a background this
    /// palette deliberately does not know.
    pub fn selected(&self) -> Style {
        Style::default().fg(self.accent).add_modifier(Modifier::REVERSED)
    }

    /// The filled block that opens the context bar and the statusline.
    /// Reversing puts the role colour behind the terminal's own background
    /// colour, so the label stays readable in any scheme.
    pub fn badge(&self, role: Color) -> Style {
        Style::default().fg(role).add_modifier(Modifier::REVERSED | Modifier::BOLD)
    }
}
