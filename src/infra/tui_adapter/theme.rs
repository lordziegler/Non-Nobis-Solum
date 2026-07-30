//! Terminal-adaptive palette.
//!
//! Nothing here is a hex colour: chrome (panel borders, body text) keeps
//! `Style::default()` so it inherits whatever the user's terminal already
//! draws, and only semantic roles get an explicit colour — always from the
//! ANSI slots, which every palette tunes for its own background. The
//! accent is slot 4 (blue), as specified.

use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::{theme_mode, QueryOptions, ThemeMode};

pub struct Theme {
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
    pub muted: Color,
}

/// Bright ANSI slots: on a dark background the normal slots are too dim.
pub const DARK_THEME: Theme = Theme {
    accent: Color::Indexed(12),
    ok: Color::Indexed(10),
    warn: Color::Indexed(11),
    error: Color::Indexed(9),
    muted: Color::Indexed(8),
};

/// Normal ANSI slots: on a light background the bright ones wash out.
pub const LIGHT_THEME: Theme = Theme {
    accent: Color::Indexed(4),
    ok: Color::Indexed(2),
    warn: Color::Indexed(3),
    error: Color::Indexed(1),
    muted: Color::Indexed(8),
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
    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn title(&self) -> Style {
        Style::default().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn muted(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn error(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn ok(&self) -> Style {
        Style::default().fg(self.ok)
    }

    /// Selected row: reverse video keeps contrast in both themes without
    /// naming a background colour.
    pub fn selected(&self) -> Style {
        Style::default().fg(self.accent).add_modifier(Modifier::REVERSED)
    }

    pub fn warn(&self) -> Style {
        Style::default().fg(self.warn)
    }
}
