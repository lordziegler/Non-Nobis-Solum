//! Palettes, and the list the Settings screen cycles through.
//!
//! Two kinds of theme live here and the difference matters more than the
//! colours do:
//!
//! - A theme that **owns its background** names every colour, including the
//!   one behind the text. It reproduces a design exactly and paints over
//!   whatever the terminal was showing — including a configured
//!   transparency or wallpaper.
//! - A theme that **does not** leaves `bg`, `panel` and `fg` as
//!   [`Color::Reset`] and takes its accents from ANSI slots, so the user's
//!   own terminal colour scheme drives it end to end.
//!
//! [`Theme::owns_background`] is what every style decision branches on: a
//! palette that does not know what is behind the text cannot name a
//! highlight background, so it reverses instead.
//!
//! One rule holds across all of them: **only [`Theme::border`] may be
//! faint, and only structure may use it.** Text ranks by hue and emphasis,
//! never by fading.

use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    /// Shown in Settings. Proper names, so they are not translated.
    pub name: &'static str,
    /// Painted over the whole frame before anything else.
    pub bg: Color,
    /// Inside a panel — one step off `bg`, which is what separates the
    /// tiles without needing a heavier border.
    pub panel: Color,
    /// **Structure only**: borders, separators, the unfilled half of a bar.
    /// Never a character a reader has to make out.
    pub border: Color,
    pub fg: Color,
    /// Everything that *introduces* a value rather than being one: field
    /// labels, column headings, key hints, mnemonics, paths, provenance.
    pub label: Color,
    /// Focus and nothing else — never decoration.
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
    pub sel_bg: Color,
    pub sel_fg: Color,
}

/// The Imperator palette — `Imperator-dotfiles/assets/Imperator-palette.md`,
/// the same amber-on-obsidian identity as the compositor, the bar and the
/// editor. Role names below are that document's own.
pub const IMPERATOR: Theme = Theme {
    name: "Imperator",
    bg: Color::Rgb(0x0E, 0x0C, 0x08),     // Deep Void
    panel: Color::Rgb(0x14, 0x10, 0x08),  // Dark Ember
    // The palette's Ember Border (#2A2010) disappears against Dark Ember at
    // this size, so this is its Hyprland inactive-border instead — the same
    // role, one step up in luminance.
    border: Color::Rgb(0x3A, 0x30, 0x18),
    fg: Color::Rgb(0xD4, 0xA8, 0x43),     // Amber Light
    label: Color::Rgb(0xB8, 0x86, 0x0B),  // Old Gold — secondary text
    accent: Color::Rgb(0xFF, 0xD7, 0x00), // Golden Signal
    ok: Color::Rgb(0x8D, 0xB8, 0x7A),     // Radar Green
    warn: Color::Rgb(0xC8, 0x96, 0x0C),   // Caution Amber
    error: Color::Rgb(0xFF, 0x6B, 0x2B),  // Plasma Red
    // Amber Dust, the palette's hover step, rather than its active step: a
    // selected row of text needs more separation than a window chrome does.
    sel_bg: Color::Rgb(0x24, 0x1C, 0x0C),
    sel_fg: Color::Rgb(0xFF, 0xD7, 0x00),
};

/// "Estrato" (1b) from `docs/Prototypes/` — the direction this TUI's layout
/// came from. Cold graphite against the copper focus; the deliberate
/// opposite of Imperator's warmth.
pub const ESTRATO: Theme = Theme {
    name: "Estrato",
    bg: Color::Rgb(0x0E, 0x12, 0x14),
    panel: Color::Rgb(0x13, 0x1A, 0x1D),
    border: Color::Rgb(0x26, 0x34, 0x3A),
    fg: Color::Rgb(0xCD, 0xD8, 0xD6),
    label: Color::Rgb(0x7F, 0x9A, 0xA8),
    accent: Color::Rgb(0xC8, 0x79, 0x4A),
    ok: Color::Rgb(0x5F, 0xA3, 0x92),
    warn: Color::Rgb(0xD0, 0xA2, 0x4A),
    error: Color::Rgb(0xD4, 0x67, 0x4F),
    sel_bg: Color::Rgb(0x1C, 0x2D, 0x32),
    sel_fg: Color::Rgb(0xEA, 0xF4, 0xF0),
};

/// No colours of its own: ANSI slots and `Reset`, so a configured
/// transparency, wallpaper or colour scheme comes through untouched. Bright
/// slots — on a dark background the normal ones are too dim.
pub const TERMINAL_DARK: Theme = Theme {
    name: "Terminal dark",
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
    sel_bg: Color::Reset,
    sel_fg: Color::Reset,
};

/// The same, on normal slots: the bright ones wash out on a light
/// background.
pub const TERMINAL_LIGHT: Theme = Theme {
    name: "Terminal light",
    bg: Color::Reset,
    panel: Color::Reset,
    border: Color::Indexed(8),
    fg: Color::Reset,
    label: Color::Indexed(4),
    accent: Color::Indexed(3),
    ok: Color::Indexed(6),
    warn: Color::Indexed(3),
    error: Color::Indexed(1),
    sel_bg: Color::Reset,
    sel_fg: Color::Reset,
};

/// What Settings cycles through. First entry is the default.
pub const THEMES: [&Theme; 4] = [&IMPERATOR, &ESTRATO, &TERMINAL_DARK, &TERMINAL_LIGHT];

pub fn default() -> &'static Theme {
    THEMES[0]
}

/// The theme after `current`, wrapping. Matching on the name keeps this
/// independent of how the constants are laid out in memory.
pub fn step(current: &Theme, delta: isize) -> &'static Theme {
    let index = THEMES.iter().position(|theme| theme.name == current.name).unwrap_or(0);
    let len = THEMES.len();
    THEMES[(index + if delta < 0 { len - 1 } else { 1 }) % len]
}

impl Theme {
    /// Whether this palette names the colour behind the text. `false` means
    /// the terminal owns it, and nothing here may assume what it is.
    pub fn owns_background(&self) -> bool {
        self.bg != Color::Reset
    }

    /// Body text inside a panel.
    pub fn base(&self) -> Style {
        Style::default().fg(self.fg).bg(self.panel)
    }

    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn title(&self) -> Style {
        Style::default().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    /// Secondary text. Ranked below the value it introduces by *hue*, never
    /// by fading — a faded glyph on an unknown background is a glyph nobody
    /// can read.
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

    /// Selected row: a lifted background where the palette owns one, which
    /// leaves the row's per-cell colours intact. Reverse video otherwise —
    /// the one highlight legible against a background this theme has
    /// deliberately not named.
    pub fn selected(&self) -> Style {
        if self.owns_background() {
            Style::default().fg(self.sel_fg).bg(self.sel_bg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.accent).add_modifier(Modifier::REVERSED)
        }
    }

    /// The filled block that opens the context bar and the statusline.
    pub fn badge(&self, role: Color) -> Style {
        if self.owns_background() {
            Style::default().fg(self.bg).bg(role).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(role).add_modifier(Modifier::REVERSED | Modifier::BOLD)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule the whole module exists to enforce: a palette that has not
    /// named its background must not name a highlight background either,
    /// and one that has must not fall back to reverse video.
    #[test]
    fn a_highlight_names_a_background_only_when_the_theme_owns_one() {
        for theme in THEMES {
            let selected = theme.selected();
            let badge = theme.badge(theme.accent);
            if theme.owns_background() {
                assert!(selected.bg.is_some() && badge.bg.is_some(), "{} must lift, not reverse", theme.name);
            } else {
                assert!(selected.bg.is_none() && badge.bg.is_none(), "{} must not name a background", theme.name);
                assert!(
                    selected.add_modifier.contains(Modifier::REVERSED),
                    "{} needs reverse video to stay legible",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn cycling_visits_every_theme_and_wraps_both_ways() {
        let mut theme = default();
        for _ in 0..THEMES.len() {
            theme = step(theme, 1);
        }
        assert_eq!(theme.name, default().name, "a full cycle must come back to the start");
        assert_eq!(step(default(), -1).name, THEMES[THEMES.len() - 1].name, "back from the first wraps to the last");
    }
}
