# Handoff — `tui_adapter` (terminal front-end)

Second front-end over the same ports as the CLI, added in session 5. Binary
`nns-tui`, framework `ratatui` 0.30 + `crossterm` 0.29. Nothing under
`src/core/` was touched to build it.

Engine-side context (data layout, reference profiles, agronomic gaps) stays
in `docs/HANDOFF.md`; architecture in `docs/rust-architecture.md`. This file
only covers the TUI.

## Run it

```
cargo run --bin nns-tui          # `cargo run` alone is still the CLI
                                 # (default-run = "non_nobis_solum")
```

Run it from the repo root: the data root defaults to `data/`, relative,
exactly like the CLI's `--data-dir`. There is no CLI flag on `nns-tui` yet —
the profile is switched from the Settings screen instead.

## Layout on screen

The visual direction is **Estrato (1b)** from
`docs/Prototypes/Non nobis sollum TUI.zip` — a tiling workspace, not a form:

```
╭──────────────────────────────────────────────────────────╮
│ NAV │ ▸ Home │ non·nobis·solum │ global │ lot LOT-001     │  <- context bar
╰──────────────────────────────────────────────────────────╯
╭ MODULES ────╮╭ ACTIVE PROJECT ──────────╮╭ SYSTEM STATUS ╮
│▎⌂ Home    h ││  (the active screen)     ││ profile       │
│ ◈ …         ││                          ││ lot / crop    │
╰─────────────╯╰──────────────────────────╯╰───────────────╯
╭──────────────────────────────────────────────────────────╮
│ NAV │ home │ j/k move · Tab pane · … │ ● ready            │  <- statusline
╰──────────────────────────────────────────────────────────╯
```

- Both bars are framed boxes, so each costs 3 rows (`ui::BAR`).
- Below 92 columns the status column is dropped so an 80x24 terminal keeps
  the module column and the workspace intact (`ui::NARROW`).
- Every panel is rounded; the focused one is told apart by an **accent
  border and a lit title**, never by a heavier line — the mosaic must not
  shift as focus moves.
- The selected row of any list or table carries `▎` (`ui::MARKER`), the
  prototype's inset accent rule, over a reverse-video row.
- The dashboard opens with the prototype's box-drawing wordmark over a
  subtitle — `TOOLKIT · vX.Y.Z · $USER`, greeting whoever is running it
  (`ui::current_user`, falling back to `LOGNAME`, and dropping the segment
  when a session has neither). The banner needs 11 rows and
  `ui::banner_width` columns — the subtitle is *wider* than the art, so the
  art's own width is not what decides the fit. Below that it is dropped
  whole, never clipped.
- The statusline carries mode (`NAV`/`FILTER`/`HELP`), screen, keys, and the
  last message behind a dot — teal when informational, **red when it is an
  error**. Errors never leave the status bar and never panic.

## Screens

| Screen | Source | Notes |
|---|---|---|
| Dashboard | `bootstrap::App::lots()` | lot table (lot · crop · yield goal); `Enter` plans the selected lot |
| Crop catalog | `ListCropsPort::list_crops` | `/` filters over id, name, type and family; `Enter` picks a crop, overriding the lot's curated one |
| Fertility plan | `FertilityCalculatorPort::calculate` | per nutrient: demand, soil supply, efficiency, to-apply, balance bar, soil status, product and dose |
| Inspect | `InspectScenario::inspect` | field context, soil tests, per-nutrient provenance, then the muted micronutrient row |
| Settings | — | language, theme, reference profile, and the three data paths (read-only) |

## Keys

| Key | Action |
|---|---|
| `j`/`k`, `↑`/`↓` | move selection; on Plan/Inspect it scrolls |
| `Tab` | switch focus between the module column and the workspace |
| `Enter` | open module / plan the selected lot / pick crop / change setting |
| `Esc`, `q` | back to the dashboard; quits when already there |
| `Ctrl-C` | quit (raw mode swallows SIGINT, so it is handled explicitly) |
| `/` | filter, on the crop catalog only |
| `h`/`l`, `←`/`→` | change the selected value, on Settings only |
| `?` | help overlay for the current screen; any key closes it |
| `h f c i , q` | module mnemonics — **only while the module column has focus**, so they can't collide with `h`/`l` in Settings |

## The parts

| File | Lines | What it owns |
|---|---|---|
| `src/infra/tui_adapter/mod.rs` | 1315 | `Tui` state, the event loop, every use-case call |
| `src/infra/tui_adapter/ui.rs` | 1005 | all rendering; the only file that formats numbers |
| `src/infra/tui_adapter/i18n.rs` | 77 | bundle loading and `t()` |
| `src/infra/tui_adapter/theme.rs` | 241 | the four palettes and the cycle |
| `lang/{en,es}.toml` | 176 each | the string bundles |
| `src/tui_main.rs` | 8 | `tui_adapter::run(bootstrap::build_app())` |

`ui.rs` is a child module of `tui_adapter`, so it reads `Tui`'s private
fields directly — that is deliberate, it keeps the state struct free of
getters. It takes `&Tui` and never mutates.

### Crate layout change this brought

`src/lib.rs` now holds `core`/`infra`; `src/main.rs` (CLI) and
`src/tui_main.rs` are thin binaries over it. Two binaries cannot share
inline modules, hence the library. Side effect worth knowing: the old
`dead_code` warnings are gone, because those items are `pub` in a library
now — a clean `cargo build` no longer proves those entities are used.

### `bootstrap::App`

The TUI switches profile at runtime, and a `DataLayout` is bound to one
profile, so use cases are **rebuilt per action** instead of being wired once
at startup. `App` carries `data_root` + `profile` and exposes `layout()`,
`reference_dir()`, `curated_dir()`, `profiles()` (lists `data/reference/`)
and `lots()` (reads `data/curated/yield_targets.csv`). No file path exists
anywhere else in the TUI.

### i18n

Bundles are embedded with `include_str!`, so switching language is
session-only and touches no disk. Every label goes through `I18n::t(id)`;
an unknown id renders as the id itself — a missing string is visible, never
a panic and never a blank. `bundles_parse_and_agree` fails the build if the
two bundles drift apart, so **adding a key means adding it to both files**.

Nav labels are length-constrained: the module column is 26 wide (24 below
`NARROW`), and a row spends 4 of its inner columns on the glyph, the
mnemonic and their spaces. That is why the Spanish module labels are short
("Fertilización", not "Plan de fertilización") while the screen titles keep
the long form.

### Theme

Four palettes, cycled from the Settings screen. **Imperator is the
default** — the amber-on-obsidian identity from
`Imperator-dotfiles/assets/Imperator-palette.md`, the same one the
compositor, the bar and the editor use. Roles below carry that document's
own names.

| Theme | Character | Background |
|---|---|---|
| **Imperator** (default) | warm amber / obsidian | its own |
| Estrato | cold graphite / copper — the layout's own prototype | its own |
| Terminal dark | ANSI bright slots | the terminal's |
| Terminal light | ANSI normal slots | the terminal's |

`Theme::owns_background` is the distinction every style decision branches
on, and it is the only branch in the module:

- **Owns it** (Imperator, Estrato) — every colour is named, including the
  one behind the text, so the design reproduces exactly. It also paints
  over a configured transparency or wallpaper.
- **Does not** (Terminal *) — `bg`, `panel` and `fg` stay `Color::Reset`
  and the accents are ANSI slots, so the user's own colour scheme drives
  it end to end. Such a palette **may not name a highlight background**,
  because it does not know what is behind the text: `selected()` and
  `badge()` reverse instead. The cost is that a reversed row flattens its
  per-cell colours, which is why the two RGB themes lift a background and
  keep them.

`a_highlight_names_a_background_only_when_the_theme_owns_one` fails the
build if a new palette gets that backwards.

**Only `border` may be faint, and only structure may use it** — borders,
separators, the unfilled half of a bar. A `dim` role for secondary *text*
is exactly how panel titles, column headings and every label in the status
column once became unreadable on a terminal with a wallpaper. Text ranks by
hue and emphasis instead: `muted()` is the `label` colour for whatever
introduces a value, `strong()` is bold for the value itself, and
`accent()`/`title()`/`selected()` rank above both. Nothing fades.

The choice is **session-only**, like the language toggle: there is no
config file anywhere in this project, and adding one for a theme would be
the first. The default is the one that matters, and it is Imperator.

Adding a palette is a `Theme` constant plus an entry in `THEMES` — no
other file changes, and the name is a proper noun so it is not translated.

Dropping the old auto-detection took `terminal-colorsaurus` out of
`Cargo.toml`. Worth knowing because it also removed a constraint the rest
of this file used to carry: nothing has to run before `ratatui::init()`
any more.

## Boundary notes and gaps

Read these before "fixing" anything that looks odd.

- **Domain types are imported, domain logic is not.** `FertilityPlan`,
  `Crop`, `SoilStatus` etc. appear in the adapter because the port
  signatures return them and there is no DTO layer. No domain service,
  constructor or agronomic rule is called here. Strict isolation would mean
  adding DTOs in `core::application` — a real change, not a cleanup.
- **`InspectScenario` has no input port.** The inspect screen calls its
  inherent `inspect()`. Adding `InspectScenarioPort` to
  `src/core/ports/input.rs` makes that call site port-only like the other
  two. Marked `TODO(gap)` in `mod.rs`.
- **No `ListLots` use case.** The lot selector is fed from
  `bootstrap::App::lots()`, which parses the curated CSV in the composition
  root. Marked `TODO(gap)` there.
- **`ScenarioInspection` carries no classified `soil_status`** — only the
  critical levels. The inspect screen borrows the status from the plan when
  one has been calculated for the same scenario, and shows `—` otherwise
  (`ui::planned_status`).
- **Micronutrients** (Fe/Mn/Zn/Cu/B/Mo) are listed on the inspect screen as
  a single "not yet planned" row, from the `UNPLANNED_MICRONUTRIENTS` const
  — hardcoded on purpose, so the row disappears the day a use case covers
  them.
- **`product` is hardcoded to `"grain"`**, matching the CLI default and the
  open item in `docs/HANDOFF.md`. When the real harvested organ per crop is
  resolved, `Tui::scenario()` is the single place to change.
- **`efficiency_rules.yaml` only covers loam/clay_loam.** A lot with any
  other texture makes `plan` return `Err`; the TUI shows that error string
  verbatim in the status bar. Nothing to fix in the TUI — it is a data gap.

### Rough edges

- Scrolling on Plan/Inspect is unbounded upward: `j` past the last line
  keeps scrolling into blank space and needs the same number of `k` presses
  to come back. Clamping needs the rendered line count, which lives in
  `ui.rs`; the honest fix is to have `draw` report content height back into
  the state.
- `crop_override` is global, not per lot; it is cleared when the lot
  selection changes.
- No mouse support (optional in the brief, keyboard-only works).
- No yield-goal editing: the plan always uses the curated target. The
  prototype's stepper, scenario comparison (A/B) and application timeline
  are **not** implemented — they need domain support that does not exist.

## Verification

```
cargo build --all-targets     # zero warnings
cargo test                    # 16/16
cargo run --bin nns-tui
```

Tests specific to this adapter, all in-module:

- `ui::tests::every_screen_renders_at_both_densities` — draws every screen
  plus the help overlay through `TestBackend` at 80x24 and 130x40. This is
  what catches layout panics and clipped panels.
- `ui::tests::the_wordmark_shows_when_it_fits_and_is_dropped_when_it_does_not`
- `theme::tests::a_highlight_names_a_background_only_when_the_theme_owns_one`
- `theme::tests::cycling_visits_every_theme_and_wraps_both_ways`
- `tests::esc_leaves_a_screen_first_and_quits_only_from_the_dashboard`
- `tests::filter_narrows_the_catalog_and_typing_never_moves_the_selection`
- `tests::language_toggle_swaps_the_bundle_without_touching_the_data`
- `tests::step_saturates_at_both_ends`, `ui::tests::bar_is_proportional…`
- `i18n::tests::bundles_parse_and_agree`, `unknown_id_falls_back_to_the_id`

Beyond the tests, the real binary was driven inside a pty
(`script -qec "stty rows 40 cols 130; ./target/debug/nns-tui"` with keys
piped after a delay) and every screen was read back. Two things to know if
you repeat that: the terminal-colour query eats keystrokes that arrive
before it times out, so delay the input by a couple of seconds; and closing
the input pipe sends `Ctrl-D`, which is a real key event to the app.

## Adding things

- **A string**: add the id to `lang/en.toml` *and* `lang/es.toml`, then use
  `tui.i18n.t("id")`. Never a literal in `ui.rs`.
- **A screen**: a `Screen` variant, a row in `MODULES` (label id, mnemonic,
  target, glyph), a `workspace()` arm, a `hint_*` string, and a `screen_title()`
  arm. Selection movement goes in `Tui::move_selection`.
- **A module that calls a new use case**: build it from
  `bootstrap::build_*(&self.cfg.layout())` inside a `Tui` method, match on
  the `Result`, and route the error through `self.fail(e)` so it lands in
  the status bar. Do not add file paths outside `bootstrap.rs`.
