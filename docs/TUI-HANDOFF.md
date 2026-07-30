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
 NAV  non·nobis·solum · profile global · lot LOT-001 · corn      <- context bar
┌ Modules ─┐┏ Workspace ━━━━━━━━━━━━━━━━━┓┌ System status ─┐
│ Home   h ││  (the active screen)       ││ profile        │
│ …        │┃                            ┃│ lot / crop     │
└──────────┘┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└────────────────┘
 Home  j/k move · Tab pane · Enter open · ? help          ready  <- statusline
```

- Below 92 columns the status column is dropped so an 80x24 terminal keeps
  the module column and the workspace intact (`ui::NARROW`).
- The focused panel gets thick borders in the accent colour; the other one
  stays plain and muted.
- The statusline carries mode (`NAV`/`FILTER`/`HELP`), screen, keys, and the
  last message — green when informational, **red when it is an error**.
  Errors never leave the status bar and never panic.

## Screens

| Screen | Source | Notes |
|---|---|---|
| Dashboard | `bootstrap::App::lots()` | lot table (lot · crop · yield goal); `Enter` plans the selected lot |
| Crop catalog | `ListCropsPort::list_crops` | `/` filters over id, name, type and family; `Enter` picks a crop, overriding the lot's curated one |
| Fertility plan | `FertilityCalculatorPort::calculate` | per nutrient: demand, soil supply, efficiency, to-apply, balance bar, soil status, product and dose |
| Inspect | `InspectScenario::inspect` | field context, soil tests, per-nutrient provenance, then the muted micronutrient rows |
| Settings | — | language toggle, reference profile, and the three data paths (read-only) |

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
| `src/infra/tui_adapter/mod.rs` | 482 | `Tui` state, the event loop, every use-case call |
| `src/infra/tui_adapter/ui.rs` | 590 | all rendering; the only file that formats numbers |
| `src/infra/tui_adapter/i18n.rs` | 77 | bundle loading and `t()` |
| `src/infra/tui_adapter/theme.rs` | 79 | terminal query and the two palettes |
| `lang/{en,es}.toml` | 123 each | the string bundles |
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

Nav labels are length-constrained: the module column is 22 wide (20 inner),
which is why the Spanish module labels are short ("Fertilización", not "Plan
de fertilización") while the screen titles keep the long form.

### Theme

`terminal_colorsaurus::theme_mode()` runs **before** `ratatui::init()` — the
OSC 11 handshake needs the plain tty, not raw mode + alternate screen. Dark
or light picks between two palettes built from ANSI slots only (bright
slots on dark, normal slots on light); the accent is slot 4/12, as briefed.
No hex colours anywhere: panels, text and backgrounds inherit the terminal.
A terminal that doesn't answer falls back to the dark bundle.

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
  muted "not yet planned" rows, from the `UNPLANNED_MICRONUTRIENTS` const —
  hardcoded on purpose, so the row disappears the day a use case covers
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

- `ui::tests::every_screen_renders_at_both_densities` — draws all five
  screens plus the help overlay through `TestBackend` at 80x24 and 130x40.
  This is what catches layout panics and clipped panels.
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
  target), a `workspace()` arm, a `hint_*` string, and a `screen_title()`
  arm. Selection movement goes in `Tui::move_selection`.
- **A module that calls a new use case**: build it from
  `bootstrap::build_*(&self.cfg.layout())` inside a `Tui` method, match on
  the `Result`, and route the error through `self.fail(e)` so it lands in
  the status bar. Do not add file paths outside `bootstrap.rs`.
