# Handoff — Rust fertility plan engine

A hexagonal (ports & adapters) Rust CLI at the repo root (`Cargo.toml`,
`src/`, `data/`), package name `non_nobis_solum`, built on top of the
`Non-Nobis-Solum-Py` prototype, with a second front-end (`nns-tui`).
Nine sessions so far: (1) the initial Rust port, (2) Bertsch et al.'s
nutrient-removal/soil-standard tables, (3) a 500-product Colombian
fertilizer vademecum, (4) N from organic matter, (5) the TUI, (6) liming,
(7) the NASA POWER agroclimatic adapter, (8) the four blockers of
`docs/BLOCKERS-AND-ROADMAP.md` — including the first write path — and
(9) an audit pass that deleted dead code and closed one division by zero.

## Status

- `cargo build`: zero warnings (the old `dead_code` ones went away in
  session 5, when `core`/`infra` moved into a library).
- `cargo clippy --all-targets`: zero warnings as of session 9.
- `cargo test`: 81/81 as of session 9 (69/69 in session 8, 5/5 when this
  section was written) — pure domain math in
  `src/core/domain/services.rs`, pinned against the prototype's formulas
  with literal inputs (not tied to any reference-data row, so data changes
  never break these).
- CLI verified end-to-end every session:
  `cargo run -- crops --profile global`,
  `cargo run -- plan --lot LOT-001 --crop corn --profile andina_colombia`,
  `cargo run -- inspect --lot LOT-002 --crop coffee --profile andina_colombia`,
  plus (session 2) `pea`/`kikuyu_grass`/`broccoli` (new crops) and a
  Ca/Mg-replaced crop, plus (session 3) confirming `andina_colombia` now
  picks a real vademecum product ("Urea 46%") while `global` still picks
  its own untouched generic one ("Urea") — no cross-profile leakage.

## Layout

```
Cargo.toml
src/core/       domain + application + ports, no IO dependency
src/infra/      CSV/TOML/YAML adapters, clap CLI, bootstrap.rs (composition root)
data/reference/global/            crops.csv + conversion_factors.toml (shared by every
                                   profile) + its own nutrient_removal/critical_levels/
                                   fertilizer_sources.csv (generic/international default)
data/reference/andina_colombia/   nutrient_removal.csv, critical_levels.csv, and its own
                                   fertilizer_sources.csv (500-product Colombian vademecum,
                                   session 3 — overrides global's for this profile)
data/curated/   two example lots (LOT-001, LOT-002) with soil tests + field context;
                the only directory the app writes to (session 8, append-only)
docs/rust-architecture.md   full architecture write-up, data layout, and how to
                            add a new reference profile
assets/         literature source files these CSVs were transcribed from
                (conversion-factors.csv, fertilizantes_colombia.csv)
```

Full explanation of the architecture, data layout and how to add a new
reference profile is in `docs/rust-architecture.md` (kept in sync with
sessions 2 and 3).

## Known gaps

- **`docs/BLOCKERS-AND-ROADMAP.md`** audited what blocked real-world use
  (session 7) and records what session 8 fixed: all four blockers are
  closed, and the document now carries the list of numbers an agronomist
  still owes this repo. The gaps listed below are the narrower ones.
- Only N/P/K/S/Ca/Mg are planned; micronutrient (`Fe`/`Mn`/`Zn`/`Cu`/`B`/`Mo`)
  enum variants exist and now have real reference data (critical levels
  since session 2, fertilizer composition since session 3) but aren't
  wired into a use case yet.
- Ca/Mg removal coefficients started as illustrative estimates
  (`source=illustrative_estimate`); session 2 replaced 12 of 16 crops with
  real literature values. `coffee`, `cassava`, `bean`, `pasture` still
  illustrative (no confident crosswalk found — see checklist).
- `tui_adapter` added in session 5 (`nns-tui` binary); `InspectScenario`
  got its `InspectScenarioPort` in session 8.
- Several entities declared per the original spec
  (`SoilSample`, `NutrientDemand`, `Availability`, `DemandType`,
  `CropCatalogRepository::get_crop`, `FertilizerSourceRepository::get_source`)
  still aren't exercised by any use case. Harmless, not a bug — and no
  longer a warning, since they are `pub` in a library.
- `Nutrient` enum has no `Co` (cobalt) variant — the one vademecum product
  that needed it (`Sulfato de cobalto`) couldn't be represented at all.

## Session 2 — Tabla 10/11/12 (Bertsch et al.) folded into the reference data

Five source images (two nutrient-removal tables, one soil-interpretation
standard, and the conversion-factors popup already referenced from
`conversion_factors.toml`) were transcribed and slotted into the existing
`data/reference/` schema — no `core` code changes, no new columns.

- `data/reference/global/nutrient_removal.csv`: 50 new crops added (all the
  hortalizas/cereales/frutales/forrajes/industriales/flores from Tabla 10 and
  11 that weren't already in the file — peas, cucumber, broccoli, apple,
  grape, kikuyu grass, alfalfa, cotton, oil palm, rose, etc.). Each row's
  `source` names the literature citation(s) (e.g.
  `Bertsch_F_2003+IFA_1992_extraccion_tabla10`) instead of a generic tag, so
  provenance is traceable per row. `product` stays `"grain"` for all of them,
  matching the existing placeholder convention (see checklist).
- `data/reference/global/crops.csv`: the same 50 crops registered in the
  catalog (crop_id, name, crop_type, family).
- Ca/Mg `illustrative_estimate` rows **replaced** with real literature
  values for the 12 crops where Tabla 10/11 gave high-confidence overlap
  with existing coefficients (N/P/K matched closely): corn, wheat, rice,
  potato, sugarcane, soybean, sunflower, orange, onion, carrot, tomato,
  sorghum. `global` profile uses the table's *Extracción* column, matching
  how existing global N/P/K/S already tracked *Extracción*; `andina_colombia`
  uses *Absorción total*, matching how its N/P/K/S already tracked that
  column (confirmed by cross-checking exact N matches, e.g. global corn
  N=15 and andina corn N=24 both matched the source table exactly before
  any edit — strong evidence the whole dataset was originally built from
  this same literature, just without Ca/Mg at the time).
- `coffee`, `cassava`, `bean`, `pasture` Ca/Mg **left untouched** — no
  confident crosswalk (see checklist).
- `data/reference/{global,andina_colombia}/critical_levels.csv`: added Na,
  B, Mn, Cu, Zn, Fe rows (new nutrients, zero prior data) from Tabla 12's
  bajo/medio/alto tier boundaries, `texture` duplicated across
  `loam`/`clay_loam` same as every existing row (texture doesn't actually
  vary the numbers in this file today, existing or new).
- `data/reference/global/conversion_factors.toml`: existing oxide factors
  (P2O5, K2O, CaO, MgO) upgraded from 3-4 sig figs to full literature
  precision, plus new NH4↔N, NO3↔N, S↔SO4, S↔SO3 and Fe/Mn/Cu/B/Zn oxide
  conversions that weren't there before.
- `assets/conversion-factors.csv` (the original literature table the TOML
  was ported from): fixed — it was badly OCR'd (missing decimal points,
  e.g. `"NH4 a N" 777` instead of `0.7772102368`); replaced with clean
  values from the same source. Not read by any code, archival only.

## Session 3 — Colombian fertilizer vademecum (`assets/fertilizantes_colombia.csv`)

Integrated the 500-product Colombian commercial fertilizer catalog into
`data/reference/andina_colombia/fertilizer_sources.csv` (new file — this
profile previously had no fertilizer catalog of its own and silently used
`global`'s 6-product generic one).

- **Code change (anticipated by the architecture doc, not a new pattern):**
  `bootstrap.rs`'s `CsvFertilizerSourcesRepo` line now uses
  `layout.reference(...)` instead of `layout.shared_reference(...)`, so
  `andina_colombia` reads its own file and `global` is unaffected (same
  path as before, since `reference("global", ...)` == `shared_reference(...)`).
- Grades converted from the vademecum's commercial N-P2O5-K2O-S form to
  this project's elemental-P/elemental-K convention, using the same
  `P2O5_to_P`/`K2O_to_K` factors added to `conversion_factors.toml` in
  session 2. N and S were already elemental in both the vademecum and the
  existing schema, no conversion needed.
- 1382 (source, nutrient) rows across 494 products (500 in the vademecum,
  6 dropped — see checklist). `restrictions` carries the vademecum's
  `estado_colombia` + `fuente` columns verbatim, so a product's
  commercialization status and citation survive even though the schema has
  no dedicated column for them.
- 19 micronutrient (Zn/Mn/Cu/Fe/B/Mo/Ca) rows added for products the
  vademecum lists with all-zero N/P2O5/K2O/S grades (it only tracks those
  four columns — no assay data for micronutrient carriers at all). Filled
  in from atomic-weight stoichiometry for well-defined pure compounds
  (e.g. ZnSO4·7H2O, H3BO3) and from widely-cited standard commercial
  grades for chelates (Fe-EDDHA 6%, Zn/Mn/Cu-EDTA ~14%, noted per row as
  "varía por fabricante" since chelate assay isn't a fixed stoichiometric
  ratio).
- **6 products dropped, no fabricated numbers:** `Sulfato de cobalto`
  (`Nutrient` enum has no `Co` variant), `Mezcla de micronutrientes
  quelatados` (blend ratio unspecified), `Quelato de calcio-boro foliar`
  (same), and `Cal agrícola`/`Cal dolomita`/`Óxido de magnesio` (the
  vademecum gives no elemental Ca/Mg for these three — but equivalent
  products with real Ca/Mg % already exist in `global`'s generic catalog
  as `ag_lime`/`dolomitic_lime`, so nothing is actually lost).
- `docs/rust-architecture.md` updated: `fertilizer_sources.csv` is no
  longer described as always-shared; the per-profile-with-fallback
  pattern is now spelled out explicitly.

## Session 4 — N-from-MO (found via `Workflow_Planes_de_Fertilidad_de_Suelos.md`)

Cross-referencing the user's authoritative workflow doc against the code
surfaced a real correctness bug, not on the prior checklist: **N
availability was always 0.0 kg/ha.** `FieldContext.organic_matter_percent`
was read from CSV but never consumed anywhere — `N_total = MO/20` and
`N_ASIM = N_total * f * wha / 100` (the doc's formulas, and `n.py`'s
`n_asimilable` in the Python prototype, hardcoded `f=0.015`) were never
ported. `soil_tests.csv` correctly has no `N` row (N isn't measured
directly), so every plan silently treated N as fully deficient.

Fixed: `services::nitrogen_total_percent`/`nitrogen_available_kg_ha` added
(pinned against the prototype's exact numbers: MO 3.2% + LOT-001's soil
weight -> 62.4 kg N/ha, test `nitrogen_available_matches_reference_prototype`).
`CalculateFertilityPlan` special-cases `Nutrient::N` to use this instead of
the generic soil-test-based path (N has no test-based path at all — see
doc comment at the call site). Mineralization factor is a hardcoded 0.015
constant (`ANNUAL_MINERALIZATION_FACTOR`), matching the prototype;
`ponytail:`-flagged as the ceiling to raise if it ever needs to vary by
profile/texture. `cargo run -- plan --lot LOT-001 --crop corn --profile
andina_colombia` now shows N availability 62.4 kg/ha (was 0.0), net
requirement 290.5 kg/ha (was 400.0).

**Not yet resolved, flagged for the user:** the same workflow doc's
efficiency table gives S: 8-10%, but `efficiency_rules.yaml` has S at
90-100% for both profiles (with a comment claiming S/Ca/Mg are "treated as
amendments applied near their full theoretical value" — an undocumented
prior deviation from the spec, not obviously wrong, but contradicts the
doc's explicit number). Needs a decision before changing — an order-of-
magnitude change to S's efficiency changes its net requirement by ~10x.

Also not implemented, all confirmed against the same doc, no code changes
attempted this session: **Ley de Incrementos Decrecientes / Ley del
Máximo** (diminishing-returns and quadratic yield-response interpretation,
requested in the original spec too), **área de aplicación en perennes**
(`π(R²-r²)`, new scope, not in any prior checklist), and **encalamiento /
relaciones de nutrientes** (already tracked above as the Tabla 12 gap).

## Session 5 — `tui_adapter` (ratatui) and the `nns-tui` binary

**Full front-end handoff (screens, keys, gaps, how to extend): `docs/TUI-HANDOFF.md`.**

Second front-end over the same ports, following the "Estrato" direction in
`docs/Prototypes/`: context bar, fixed module column, workspace, status
column, modal statusline. Five screens — dashboard (lot selector), crop
catalog (filterable), fertility plan, scenario inspection, settings.

- **New crate layout:** `src/lib.rs` now holds `core`/`infra` so both
  binaries share them; `src/main.rs` (CLI, unchanged behaviour) and
  `src/tui_main.rs` (`nns-tui`) are thin. Side effect: the old `dead_code`
  warnings are gone — those items are `pub` in a library now.
- **`bootstrap::App` + `build_app()`**: data root plus the selected
  profile, with `layout()`, `reference_dir()`, `curated_dir()`,
  `profiles()` (reads `data/reference/`) and `lots()` (reads
  `data/curated/yield_targets.csv`). The TUI can switch profile at runtime,
  so use cases are rebuilt per action instead of wired once. No file path
  exists outside this file.
- **i18n**: `lang/en.toml` / `lang/es.toml`, embedded with `include_str!`,
  toggled in Settings, session-only. Every label goes through `I18n::t`;
  a missing id renders as the id. Test `bundles_parse_and_agree` fails if
  the two bundles drift apart.
- **Theme**: `terminal_colorsaurus` queries the background (OSC 11) *before*
  raw mode; dark/light selects between two ANSI-slot palettes (accent =
  slot 4/12 as briefed). No hex colours — chrome inherits the terminal.
- **Known gaps surfaced, not hidden**: micronutrients appear on the inspect
  screen as muted "not yet planned" rows; every `DomainError` (including
  the `efficiency_rules.yaml` texture gap) is shown verbatim in the status
  bar, red, never a panic.

New TODO(gap) markers in the code: no `ListLots` port (the lot selector is
fed from `bootstrap`), no `InspectScenarioPort`, and `ScenarioInspection`
carries critical levels but no classified `soil_status` (the inspect screen
borrows it from the plan when one has been calculated).

Verified by rendering every screen at 80x24 and 130x40 through
`TestBackend` (`every_screen_renders_at_both_densities`) and by driving the
real binary in a pty. `cargo test`: 16/16. `cargo build`: zero warnings.

## Session 6 — Liming (Tabla 12: "Requerimiento de Cal")

Scoped narrowly to the "Requerimiento de Cal" formulas from
`Workflow_Planes_de_Fertilidad_de_Suelos.md` (Al³⁺/CIC-based lime
requirement + PRNT/material dose) — matches the original spec's
"funciones básicas de encalamiento" bullet exactly. Left out: pH/EC/MO-
by-climate classification and base-cation ratios (the rest of Tabla 12) —
those are qualitative threshold tables that need their own literature
transcription, not a code gap. Still open, see checklist below.

- **`Nutrient::Al`/`Nutrient::H`** added (cmolc/kg, reuse
  `SoilTestRepository`/`soil_tests.csv` exactly like Ca/Mg/K). Not in
  `MACRONUTRIENTS` — acidity indicators, not fertilization targets. `H` is
  optional, defaults to 0 when absent (many labs report Al alone).
- **New entities**: `LimingMaterial` (CaO/MgO grades + granulometric
  efficiency — kept separate from `FertilizerSource` because neutralizing
  value isn't the same axis as elemental nutrient %), `LimingDose`,
  `LimingRecommendation`. `FertilityPlan.liming: Option<LimingRecommendation>`,
  `None` when the sample has no Al³⁺ test (the workflow's "si aplica").
- **New ports**: `LimingRulesRepository` (`liming_rules.toml` — `al_factor`
  1.5 per Kamprath's tropical-soils rule of thumb, `target_base_saturation_pct`
  80%, both `source=Kamprath_1970_tropical_soils_typical`, swappable
  per-profile), `LimingMaterialRepository` (`liming_materials.csv` — 3
  seed materials: ag lime, dolomitic lime, quicklime, all
  `source=illustrative_estimate`, same convention as the Ca/Mg removal
  placeholders — real assay data still needed before field use).
- **Domain services** (`services.rs`): CICE, current base saturation %,
  both lime-requirement formulas, EQ (neutralizing value), PRNT, material
  dose. `recommended_t_ha = max(al_based, base_saturation_based)` —
  `ponytail:`-flagged as a conservative stand-in for the real rule (Al
  method only above a crop-specific Al-saturation toxicity threshold).
- **Demo data**: `LOT-002` (`soil_tests.csv`) got an `Al` row (1.5
  cmolc/kg) so the flow is exercised end-to-end; `LOT-001` still has none,
  demonstrating the `None` path. `cargo run -- plan --lot LOT-002 --crop
  coffee --profile andina_colombia` now prints a liming line: base
  saturation 72.6% (target 80%), 2.25 t/ha by Al³⁺ vs. 1.33 t/ha by base
  saturation, recommended 2.25 t/ha, material Quicklime 1.43 t/ha.
- Tests: 17 new in `services.rs` (CICE, SB, both requirement formulas, EQ,
  PRNT, dose — hand-computed numbers). `cargo test`: 25/25 (shared with
  session 5's TUI tests, no interference — this session touched `core` and
  `bootstrap.rs`/`cli_adapter.rs` only).

**Found, not fixed (flagged in `docs/rust-architecture.md`'s "Known
simplifications" too):** the two curated lots' `region` column
(`field_context.csv`) is hardcoded `andina_colombia` regardless of
`--profile`. `CriticalLevelsRepository` already silently no-ops on the
resulting mismatch (`.ok()`); `LimingRulesRepository` does not
(hard-propagates, matching `EfficiencyRulesRepository`'s existing
convention), so `--lot LOT-002 --profile global` fails outright instead of
degrading quietly. Not a regression — this combination was never a
validated path — but worth fixing before curated data grows.

**Concurrency note:** this session ran while a second Claude Code session
was actively building the TUI (session 5, above) in the same working
tree, uncoordinated, no commits between us. No file-level collisions
happened (touched different files except `bootstrap.rs`/`src/infra/mod.rs`,
where both sessions only appended), but this file's history briefly lived
at the repo root as `HANDOFF.md` before session 5 relocated it here — if
you're reading this from git blame and the author/timing looks tangled,
that's why.

## Session 7 — `agroclimatic_adapter` (NASA POWER) and climate-modulated N

First adapter in the project that crosses a **network** rather than
reading a local file. Everything about it is shaped by one constraint:
the plan must run offline. The port returns `Err`, the use case turns
that into `None`, and the plan proceeds on baseline constants — a climate
failure can never fail a plan.

- **New port `AgroclimaticRepository`** (`ports/output.rs`): coordinates
  in, `AnnualClimatology` out. Deliberately names no provider, no time
  window and no parameter codes, so Open-Meteo or Agromonitoring drop in
  behind it untouched. Which variables a provider can supply is expressed
  by leaving fields `None`, not by widening the trait.
- **New entity `AnnualClimatology`**: eight `Option<f64>` annual figures
  plus `annual_precip_mm()`/`annual_et0_mm()`. All optional because a
  grid cell can be missing any of them.
- **`FieldContext` gained `latitude`/`longitude`** (`Option<f64>`,
  `#[serde(default)]`) — a lot without coordinates simply gets no climate
  enrichment, same path as an API outage.
- **`FertilityPlan` gained `mineralization_factor` + `climate`**, so the
  output can state which regime produced each number instead of leaving
  the reader to guess.
- **Domain services** (all pure, no IO): `mineralization_factor` (the
  `0.015 × T_factor × W_factor` formula), `efficiency_climate_adjustment`
  (returns `EfficiencyAdjustment`, three deltas that only ever *narrow*
  the optimistic end of an efficiency range), `rue_index`, and
  `reference_et0_hargreaves_mm_day`. Each rule carries an
  `// AGRONOMIC_NOTE:` explaining why the signal matters agronomically.
- **`ANNUAL_MINERALIZATION_FACTOR` retired**: the session-4 `ponytail:`
  ceiling is now `services::BASELINE_MINERALIZATION_FACTOR`, the fallback
  that climate modulates rather than the only value available.
- **`infra/agroclimatic_adapter/`**: `nasa_power.rs` (blocking `reqwest`,
  10 s timeout, `-999` sentinel filtered to `None`, HTTP status checked
  *before* decoding) and `cache.rs` (`CachedAgroclimaticRepo`, in-memory
  `HashMap` keyed on coordinates rounded to 2 dp; a poisoned mutex is a
  cache miss, never a panic; failures are not cached so a transient
  outage doesn't pin a lot to "no climate" for the session).
- **CLI**: `--no-climate` on `plan`. Output labels every climate-derived
  figure, e.g. `N mineralization factor: 0.0102  [climate-adjusted,
  T=13.2°C]` vs `0.0150  [baseline — no climate data]`, plus an
  informational `Solar yield potential: HIGH (RUE index 0.81)`.

**Three corrections to the session brief**, all verified against the live
API and the repo:

1. **`ET0_PENMAN` does not exist.** It is not an AG-community climatology
   parameter, and requesting it does not yield `-999` — POWER rejects the
   *entire request* with HTTP 422 (`"One of your parameters is incorrect:
   ET0_PENMAN."`), so every other variable is lost too. The only ET-ish
   parameters available are `EVLAND`/`EVPTRNS`, which are *actual*
   evapotranspiration in MJ/m²/day, not reference ET0 — and actual ET
   can't exceed the water supply, so the water-deficit rule keyed on
   `ET0 > 1.5 × precip` could never fire under rainfed conditions.
   Resolved by requesting `TOA_SW_DWN` instead and deriving ET0 with the
   **FAO-56 Hargreaves** equation, FAO's own sanctioned fallback when
   Penman-Monteith inputs are unavailable. Computed **per month and then
   averaged**: the equation's `(Tmax − Tmin)` term must be a within-period
   diurnal range, and POWER's `ANN` entries for `T2M_MAX`/`T2M_MIN` are
   annual *extremes* (22.59/4.42 for Pasto, vs ~20.9/5.9 monthly), which
   would inflate ET0 by ~20%.
2. **`field_context.csv` had no `latitude`/`longitude`** (nor `area_ha`
   nor `soil_weight_kg_ha`, also named in the brief). Added, with a
   quoted `coordinates_note` column marking both lots' Pasto coordinates
   as illustrative.
3. **`IrrigationType` is `IrrigationSystem`** here, and the `0.015`
   constant lived in `calculate_fertility_plan.rs`, not `services.rs`.
   `nitrogen_available_kg_ha` already took the factor as a parameter
   since session 4, so that deliverable needed no change.

Also deviated: the use case holds `Option<Box<dyn AgroclimaticRepository>>`
rather than the brief's `Option<&dyn ...>` — every other repository here
is a `Box<dyn>`, and a borrow would have forced a lifetime parameter onto
the struct for no gain.

`data/curated/field_context.csv` now carries `latitude,longitude,
coordinates_note`. Both lots point at Pasto, Nariño (1.2136, −77.2811),
**illustrative, not surveyed** — the note column says so per row.

Verified end-to-end:

- With network: N availability 42.6 kg/ha (was 62.4), net requirement
  325.3 kg/ha (was 290.5), factor 0.0102 — the cold Andean site
  mineralizes ~2/3 of the tropical baseline, the agronomically expected
  direction. LOT-002 (drip) gets 0.0079, confirming `W_factor` pins to
  1.0 under irrigation.
- `--no-climate`: baseline 0.0150, one stderr warning, exit 0.
- Network blocked (`HTTPS_PROXY` to a closed port): identical baseline
  plan, one warning, **exit 0**. Blackholed proxy: cuts at 10.2 s, still
  exit 0 with a full plan.
- `cargo test`: 45/45 (20 new — 11 in `services.rs`, 7 in `nasa_power.rs`
  parsing a trimmed real Pasto response, 3 in `cache.rs`).
  `cargo build`: zero warnings.

TODO(gap) added: **the TUI passes `None`** — the fetch is blocking with a
10 s timeout and the render loop is single-threaded, so wiring it in
would freeze the UI on every plan. Needs a background fetch or a
pre-warmed cache first. The CLI has climate today; the TUI does not.

**Concurrency note:** this session started while a third session was
mid-write on the liming feature (session 6) in the same uncommitted tree
— `cargo check` was failing and `CalculateFertilityPlan::new` was
half-wired. Work was paused until that settled rather than risk
interleaved edits to the five files both sessions needed; nothing was
lost, and the climate work then built on top of the finished liming
constructor.

## Session 8 — the four blockers of `BLOCKERS-AND-ROADMAP.md`

Worked the five phases of that document in order. Every blocker was
reproduced before the fix and re-run after. The tree was committed as-is
first (`Checkpoint: uncommitted work from sessions 2-7`) on branch
`session8-blockers`, so sessions 2-7 are separable from this work;
`target/` was untracked at the same time (`.gitignore` already listed it).

`cargo test`: 45 → **69**, `cargo build`: still zero warnings.

### Phase 1 — the efficiency grid answers for any lot

`efficiency_rules.yaml` covered 4 of 48 texture × irrigation
combinations, and `get_efficiency_range` hard-propagates, so a lot with a
`sandy_loam` texture or `sprinkler` irrigation had no plan at all.

`YamlEfficiencyRulesRepo` now mirrors `CsvCriticalLevelsRepo`: exact row
first, sentinel `("any", "any")` row second. Six sentinel rows per
profile, tagged
`source: documented_fallback_NOT_literature_envelope_of_covered_rows`,
each range being the envelope (lowest min, highest max) of that
nutrient's four curated rows. **No efficiency value was invented** — the
envelope is a mechanical derivation from data already in the file, its
midpoint equals the average of the covered combinations, and its width
states how little is known. The session-2 checklist item that called an
`"any"` fallback "scientifically dishonest" is answered by tagging the
provenance instead of laundering it: an exact row always wins, so real
per-class data can land one row at a time with no code change.

### Phase 2 — yield goal in the TUI

The Crops screen offered 66 crops and passed `yield_override: None`, so
the 64 without a row in `yield_targets.csv` errored on the first plan.
Picking a crop now checks the curated rows already in memory and, when
there is no goal for that (lot, crop), asks for one: digits and a single
separator only, refused unless it parses to a number greater than zero.
Changing lot or crop clears it, so a goal can never leak into a scenario
it wasn't typed for. No `core` changes.

### Phase 3 — the write path (first write in the project's history)

- **`CuratedDataWriter`** output port, exactly the three methods the
  roadmap specified. Append-only by design; editing an existing row is a
  read-modify-rename contract that nothing has asked for yet.
- **`CsvCuratedWriter`**: `OpenOptions::append` + `has_headers(false)` +
  `csv::Writer`, which quotes any field containing a comma, a quote or a
  newline. A test writes a region called `Nariño, Colombia` and reads the
  file back through the production readers — a previous session corrupted
  the curated CSVs by hand-formatting exactly this case. `field_context.csv`
  rows are written with an empty trailing `coordinates_note`: the app
  knows nothing about a coordinate's provenance, and a short row would
  make the whole file unreadable to `csv::Reader`.
- **`RegisterLot`** use case + `RegisterLotPort`. Every field arrives as
  raw text so that parsing and validation happen in exactly one place:
  texture and irrigation must parse, numbers must be finite and positive,
  pH and organic matter must be inside their definitional ranges,
  coordinates inside theirs, crop and yield goal are all-or-nothing,
  sample depths must not be inverted, a sample needs an existing lot, and
  a duplicate `field_id` is refused **before** anything is written (all
  curated readers stop at the first match, so a duplicate would shadow
  the original on every read).
- **`ListLots`** use case + `ListLotsPort`, reading `field_context.csv`
  rather than `yield_targets.csv`: a lot registered without a planning
  row is still a lot. This retired `bootstrap::App::lots`/`LotRow`, the
  last place a front-end read a curated file through the composition root.
- **TUI**: "New lot" and "New sample" forms (module column, mnemonics
  `n`/`s`), a labelled-field list with a save row; values stay text all
  the way to the use case, and a rejection leaves the form open showing
  the port's own message. `Texture::ALL`/`IrrigationSystem::ALL` were
  added to the domain so the form's accepted-values hint can't drift from
  `from_str`.
- Fixed on the way: `reload()` left a stale error flag set, which
  suppressed the success message of whatever triggered the reload.

### Phase 4 — `region` vs `--profile`

`field_context.csv` hardcodes `region=andina_colombia` for both lots
whatever `--profile` says; `LimingRulesRepository` propagated the
resulting `NotFound` (hard fail) and `CriticalLevelsRepository` swallowed
it (every soil status silently `-`).

A reference file already lives inside a profile directory, so its rows
are scoped to that profile whatever a lot claims. Both repositories now
accept the sentinel region `"any"` — a row naming a region still wins —
and the shipped `liming_rules.toml` / `critical_levels.csv` rows are
relabelled to it. **No threshold value changed, only the lookup key.**
`plan --lot LOT-002 --crop coffee --profile global` now produces a full
plan including the lime recommendation, and soil status resolves under
`--profile global`.

### Phase 5 — parity and polish

- **Both `unwrap()` calls gone.** `best_source_for` and
  `best_liming_material_for` share `highest_rated`, which drops
  non-positive *and* NaN ratings (`> 0.0` is false for NaN) before
  ordering with `total_cmp`. No `unwrap()` outside `#[cfg(test)]` remains.
- **`InspectScenarioPort`** added; the TUI and CLI stop calling an
  inherent method.
- **Climate in the TUI.** Not a wiring change on its own — the fetch
  blocks up to 10 s and the render loop is single-threaded — so:
  `CachedAgroclimaticRepo` now holds a `Send + Sync` provider and exposes
  `cached()`; a new `PrewarmedAgroclimaticRepo` is a non-blocking view
  over a shared `Arc` of it, reporting a miss as
  `ExternalServiceUnavailable` (which the use case already degrades on);
  `bootstrap::build_climate_cache` hands the TUI that `Arc`; the TUI
  starts one background fetch per lot per session and plans against the
  prewarmed view. The plan screen states which regime produced its N
  numbers, like the CLI already did. Tests construct the TUI with no
  cache, so the suite never opens a socket.
- **Not done, deliberately:** plan export and persisted TUI settings.
  Both are new IO surfaces that deserve their own port decision rather
  than a bolt-on at the end of a session.

### Verified

- `cargo build` zero warnings; `cargo test` 69/69.
- `plan --lot LOT-002 --crop corn --profile andina_colombia --yield-value 8
  --yield-unit t_ha --no-climate` → full plan, exit 0. **Without**
  `--yield-value` it still errors with `no yield target for
  field_id=LOT-002 crop_id=corn`, which is correct: the goal has to come
  from somewhere, and inventing one is exactly what this project must not
  do. `--yield-value` on the CLI and the Crops-screen prompt in the TUI
  are the two ways to supply it.
- `plan --lot LOT-002 --crop coffee --profile global --no-climate` → full
  plan with liming, exit 0 (was: `no liming rules for
  region=andina_colombia`).
- A lot with `silty_clay` / `gravity` (outside the curated grid) plans
  through both the CLI (`--data-dir` copy of `data/`) and the TUI write
  path (test `a_lot_curated_in_the_tui_can_be_sampled_and_planned`, which
  registers `LOT-900` as `silty_clay`/`gravity`, adds a P test and plans
  `wheat` on a typed yield goal).
- Climate still degrades: with `HTTPS_PROXY` pointed at a closed port,
  `plan` prints exactly one stderr line and exits 0 on baseline
  constants. With the network up, the same lot gives factor 0.0102 at
  T=13.2 °C — the session-7 numbers, unchanged.
- The TUI was **not** launched by hand (it takes over the tty); its own
  test suite renders all seven screens at 80x24 and 130x40.

## Session 9 — audit pass: one division by zero, dead code, clippy

No feature work. A read-through of all ~6,800 lines of `src/` asking two
questions per file: is this reachable, and is it right. The agronomy was
left alone on purpose — the formulas, the `AGRONOMIC_NOTE` blocks, the
clamps and the `ponytail:` calibration notes are the part where
"simplifying" means losing precision, not noise.

### The one correctness bug — an infinite fertilizer dose

`services::net_requirement_kg_ha` divides by the efficiency fraction and
had no guard against zero:

```rust
(demand_kg_ha - availability_kg_ha) / efficiency_fraction
```

Efficiency comes from `efficiency_rules.yaml`, i.e. from a file a human
edits. A row with `efficiency_min: 0.0` / `efficiency_max: 0.0` makes that
expression `inf`, `dose_kg_product_ha` then divides `inf` by the product
grade, and the CLI prints `inf kg/ha Urea` as an agronomic
recommendation — three layers away from the file that caused it, with no
error anywhere. An inverted range (`min > max`) is the same class of
garbage arriving quietly.

Fixed at the trust boundary rather than in the formula:
`YamlEfficiencyRulesRepo::from_yaml_str` now refuses to construct at all
unless every row satisfies `0 < min <= max <= 1`, and the
`DomainError::DataSource` message names the offending
texture/irrigation/nutrient and its numbers. That is the single choke
point both `CalculateFertilityPlan` and `InspectScenario` load through, so
one check covers both; putting the guard inside the formula would instead
have forced a decision (return 0? return `inf`?) that silently
under-fertilizes or keeps printing nonsense.

Two tests: `an_unusable_efficiency_range_is_refused_at_load` (zero and
inverted) and `every_shipped_profile_passes_the_check`, which loads
`global` and `andina_colombia` for real — a validation that rejected the
project's own reference data would be worse than none.

### Dead code removed

None of this had a single caller. It survived because everything in a
library crate is `pub`, so `dead_code` never fires on it — grep, not the
compiler, is what finds this class of rot:

- `SoilSample` and `NutrientDemand` (`entities.rs`); deleting
  `NutrientDemand` in turn orphaned `DemandType`, and `Availability` was
  never used either (`value_objects.rs`).
- `CropCatalogRepository::get_crop` and
  `FertilizerSourceRepository::get_source` — two **port methods**, each
  with a full CSV implementation behind it. Worth noting for the
  architecture: an unused method on a port is more expensive than an
  unused function, because every future adapter has to implement it.
- `NasaPowerRepo::with_base_url` and its `base_url` field. Its doc comment
  claimed it was "used by the tests to serve a canned response"; no test
  used it, and never had — the tests exercise `climatology_from` directly
  on a literal payload. A comment asserting a fact that is not true is
  worse than no comment, so both went.

### Simplifications

- `YamlEfficiencyRulesRepo::get_efficiency_range` used two closures and a
  `&dyn Fn` indirection to express "exact row first, sentinel second". Now
  one `row(texture, irrigation)` closure called twice.
- `bootstrap::build_agroclimatic_repo` no longer wraps NASA POWER in
  `CachedAgroclimaticRepo`. A CLI run fetches one climatology and exits, so
  that cache was never read once — as `cache.rs`'s own doc comment already
  said ("a single CLI run never hits it twice"). The TUI keeps its cache,
  which is the front-end that actually needs one.
- `csv_curated_writer`: dropped the `number()` wrapper (it was
  `to_string()`) and the `NO_COORDINATES_NOTE = ""` constant; the reason
  that column is written empty now sits as a comment where it is written.
- `csv_field_context_repo`: two `impl CsvFieldContextRepo` blocks merged.
- The five `cargo clippy` warnings (print literal, type complexity, two
  `cloned_ref_to_slice_refs`, `expect_fun_call`) all fixed.

### Faults found and left alone, with the reason

These are recorded rather than fixed — each one is either a judgement
call the next session should make, or a real gap that wanting a small
diff should not be allowed to paper over:

- **The division guard lives in the caller, not the function.**
  `services::dose_kg_product_ha` and `lime_material_dose_t_ha` divide the
  same way `net_requirement_kg_ha` did. Today they are safe *only* because
  `highest_rated` filters ratings to `> 0.0` before either is reached. That
  coupling is invisible from the functions themselves: a second call site
  that skips `highest_rated` reintroduces the infinite dose. Either move
  the guard in or say so in their doc comments.
- **Three climate fields are written and never read.**
  `AnnualClimatology::min_temp_c`, `humidity_pct` and `wind_ms` are filled
  by the POWER adapter and consumed by no rule — ET0 is derived from the
  raw monthly parameter map, not from these. They were kept because the
  struct is a provider-shaped DTO and dropping them would narrow what a
  future rule can reach for, but nothing today justifies them.
- **`rue_index` is printed but scales nothing.** The CLI reports "Solar
  yield potential: HIGH/MEDIUM/LOW" next to the plan. It is already marked
  `TODO(gap)` in `services.rs`, but the risk is presentational and worth
  restating: a figure printed beside a dose reads as an input to that dose.
  It is not.
- **`product: "grain"` is still hardcoded** in both front-ends
  (`ponytail:`-flagged in `tui_adapter/mod.rs`, a clap default on the CLI).
  Any crop whose removal rows are keyed to another harvested organ silently
  fails to match.
- **The inspect screen borrows `soil_status` from the plan.**
  `ScenarioInspection` carries critical levels but never classifies
  against them, so inspecting without planning first shows no status
  (`TODO(gap)` in `ui.rs`).

### Verified

- `cargo clippy --all-targets` zero warnings; `cargo test` 81/81.
- `plan --lot LOT-001 --crop corn --no-climate`, `inspect --lot LOT-001
  --crop corn` and `crops` all produce the same output as before the pass;
  `cargo build --bin nns-tui` builds.
- Test count went 77 → 81 across the working tree: +2 from this pass
  (the efficiency-range checks) and +2 from concurrent theme work, below.

### Note for whoever picks this up

This pass ran concurrently with the theme work that landed as
`2db5b41 Feat: four palettes and a theme selector, Imperator by default`.
For a while both sets of edits sat unstaged in the same checkout, which is
why the +2 tests above are not from this session; the file sets are
disjoint (`theme.rs`, `ui.rs`, `tui_adapter/mod.rs`, `lang/*.toml`,
`Cargo.toml`) and the two passes were committed separately.

An untracked `docs/Non-Nobis-Solum-Py/` also appeared during the session,
apparently the archived Python prototype being relocated. It was left
untracked and uncommitted on purpose — moving a prototype into `docs/` is
its own decision, not part of an audit pass.

## Checklist — data still to gather / implement

From session 7 (agroclimatic adapter):

- [ ] **Both curated lots share one illustrative coordinate pair.** Pasto
      (1.2136, −77.2811) stands in for LOT-001 and LOT-002 alike, so they
      necessarily resolve to the same POWER grid cell and the same
      climatology. Real per-lot coordinates are needed before any climate
      number here means anything about a specific field.
- [ ] **The three efficiency rules are uncalibrated.** One flat 0.05
      penalty per signal, thresholds (35 °C, ET0 > 1.5× precip, 2000 mm)
      taken as round agronomic rules of thumb rather than from a fitted
      dataset. They are deliberately conservative and don't compound, but
      no field data backs the magnitude. `ponytail:`-flagged in
      `services.rs`.
- [ ] **Hargreaves ET0 is a substitute for a real Penman-Monteith ET0.**
      It uses the diurnal temperature range as a proxy for humidity and
      cloudiness; POWER already returns real `RH2M` and `WS2M`, which are
      currently stored and unused. Open-Meteo exposes a genuine FAO-56
      `et0_fao_evapotranspiration` — the natural second adapter behind
      `AgroclimaticRepository`, and the first real test of whether that
      port is as swappable as intended.
- [ ] **`RUE_index` is computed and displayed but feeds nothing.** Per
      the brief it must not modify any dose this session. The right home
      is a yield-gap use case, where a radiation-limited site should have
      its *yield target* questioned rather than its fertilizer dose
      adjusted. TODO(gap) marked in `services.rs`.
- [ ] **A 30-year climatology is not a season.** Every figure here is a
      long-term mean, so it characterizes a site, not the year being
      planned. Planning against an actual forecast or the current
      season's observations is a different data product (and a different
      POWER endpoint) — `vigil` already does this for daily weather.

From session 2 (Tabla 10/11/12):

- [ ] **`product` is a placeholder, not the real harvestable organ.** Every
      row (existing and new) uses `"grain"` regardless of whether the crop's
      actual product is a fruit, root, leaf, forage, etc. Works today only
      because the CLI defaults `--product` to `"grain"`. Tabla 10/11 gives
      the real organ per crop (Fruto, Hojas, Raíz, Bulbo, forraje seco...) —
      not used yet. Fixing this is a real design decision (does `product`
      become crop-specific with no default, or does the CLI look up the
      right organ per crop?), not a data-entry fix.
- [ ] **`coffee`, `cassava`, `bean`, `pasture` Ca/Mg still `illustrative_estimate`.**
      Tabla 10/11 has `café`/`yuca` rows but the numbers don't line up with
      existing N/P/K/S at a plausible scale (café almendra's Extracción, e.g.,
      is ~7x today's N — likely a different yield-basis/unit), and `bean`/
      `pasture` have no direct match in the tables (which use named legumes
      like `arveja` and named grasses like `kikuyo` instead of a generic
      "bean"/"pasture"). Needs either a real per-crop match or an explicit
      decision to keep the estimate.
- [ ] **`andina_colombia` didn't get the 50 new crops, only `global` did.**
      Tabla 10/11 is general/international literature, which is why it went
      to `global`; if any of those new crops need Colombia-specific
      coefficients later, `andina_colombia/nutrient_removal.csv` needs its
      own rows (same crop_ids already exist in the catalog).
- [ ] **`critical_levels.csv` P/K/S/Ca/Mg thresholds not reconciled against
      Tabla 12.** Tabla 12 gives its own bajo/medio/alto boundaries for these
      (e.g. P: Bray II 10/20/40, Olsen 8/16/35 — two different *methods*,
      a dimension the current schema has no column for) that don't match
      today's numbers closely enough to blindly overwrite. Left untouched
      this session; only the never-before-covered nutrients (Na, B, Mn, Cu,
      Zn, Fe) were added.
- [x] **`critical_levels.csv` only has `loam`/`clay_loam` populated, for
      every nutrient, old and new.** Fixed in session 4: since the two
      populated rows were always identical per nutrient (confirming Tabla
      12 doesn't vary critical levels by texture at all), both profiles'
      `critical_levels.csv` now carry a single row per nutrient with the
      sentinel texture `"any"` instead of a duplicated `loam`/`clay_loam`
      pair. `CsvCriticalLevelsRepo::get_critical_level` matches an exact
      texture first, falls back to `"any"` otherwise — any of the 12
      `Texture` values now resolves a `soil_status` instead of silently
      landing on `None`. Tests: `falls_back_to_any_texture_when_no_exact_match_exists`,
      `unknown_nutrient_still_errors` in `csv_critical_levels_repo.rs`.
      Note: actually confirmed this was a *silent* gap, not a crash —
      `CalculateFertilityPlan`/`InspectScenario` both call
      `get_critical_level(...).ok()`, so an unmatched texture only dropped
      `soil_status` to `None`, it never propagated the `NotFound` up.
  - [x] **`efficiency_rules.yaml` had the identical shape of gap** — and
        unlike critical levels its N/P/K ranges genuinely differ between
        the two populated textures (N rainfed: 0.55–0.65 for `loam` vs.
        0.45–0.55 for `clay_loam`), so a silent `"any"` fallback would
        have been dishonest. Session 8 added one anyway, but **tagged**:
        the sentinel rows carry
        `source=documented_fallback_NOT_literature_envelope_of_covered_rows`
        and a header comment saying so, and their ranges are the envelope
        of the same file's curated rows rather than new numbers. The
        crash is gone (any texture × irrigation now plans); the data gap
        is not:
    - [ ] **Real per-texture-class efficiency data for the 44 uncovered
          combinations.** An exact row beats the sentinel, so these can
          be added one at a time with no code change. Until then every
          lot outside `{loam, clay_loam} × {rainfed, drip}` is planned on
          a derived envelope, not on literature.
- [x] **Lime requirement from Al³⁺/CIC + PRNT/material dose** — fixed in
      session 6, see above (`LimingRecommendation`, `LimingRulesRepository`,
      `LimingMaterialRepository`).
  - [ ] **Tabla 12's second half beyond lime requirement — still not
        implemented: no schema exists.** pH categories (muy ácido →
        alcalino), organic-matter thresholds by climate (frío/medio/cálido),
        electrical conductivity, base-cation ratios (Ca:Mg, Mg:K, K:Mg,
        Ca:K), and specific lime-material blends by name (cal
        hidratada/dolomita/abono Paz del Río) beyond the 3 generic
        materials session 6 seeded. These are qualitative
        threshold/classification tables, not formulas — need their own
        literature transcription (a new CSV, no new port shape) before
        they can be used. `cal_hidratada`/`abono_paz_del_rio` could be
        added as more rows to `liming_materials.csv` once real CaO/MgO/EG
        data is sourced — no code change needed for that part.
- [ ] **`CriticalLevel.classify()` only uses `low_threshold`/`medium_threshold`**
      (see doc-comment in `entities.rs`); `high_threshold` is reserved for a
      not-yet-built excess/toxicity check. The Na/B/Mn/Cu/Zn/Fe rows added in
      session 2 set `high_threshold` equal to `medium_threshold` (Tabla 12
      doesn't give a further ceiling above "alto") — harmless today, revisit
      once that check exists.

From session 3 (fertilizer vademecum):

- [ ] **~150 of the 500 rows are a combinatorial "NPK edáfico X-Y-Z grado
      agrícola" block** (ids ~348-500), explicitly marked in the source
      data itself as `no verificada en el registro ICA` / "catálogo
      genérico de mezcladoras" — these read like auto-generated filler
      (systematic near-sequential NPK ratios, no crop or brand tied to
      them) rather than real curated products, unlike ids 1-64 (named
      chemicals with real suppliers) or the crop-specific blends. They
      were integrated as-is per instruction, and the "unverified" caveat
      is preserved in `restrictions` — but worth deciding whether this
      block should stay, since it dominates the row count of an otherwise
      real product list.
- [ ] **Chelate/oxichloride percentages are typical commercial grades, not
      analyzed values** (Fe-EDDHA 6%, Zn/Mn/Cu-EDTA ~13-14%, Zn/Fe-DTPA
      ~10%, oxicloruro de cobre ~50%). Real products vary by manufacturer;
      flagged per-row in `restrictions` but worth sourcing real assay data
      before using this for a real purchase decision.
- [ ] **`Sulfato de cobalto` needs `Nutrient::Co`** added to the enum
      (`src/core/domain/nutrient.rs`) before it can be represented at all
      — currently the only vademecum product with zero possible
      representation for that reason alone.
- [ ] **`CalculateFertilityPlan`'s "highest % wins" source-picking logic
      now has ~500 more candidates to choose from per nutrient** (e.g. MAP
      soluble at 61% P2O5 ≈ 26.6% elemental P is now the highest-P source,
      above the previous DAP-based pick). Not a bug — just means plan
      output for `andina_colombia` will differ from before session 3 for
      any nutrient where a more concentrated product exists in the new
      catalog.
- [ ] **`global`'s fertilizer catalog was left untouched (6 products)** —
      only `andina_colombia` got the vademecum. If `global` should also
      grow beyond its current generic minimal set, that's separate work.

From session 9 (audit pass):

- [ ] **`dose_kg_product_ha` and `lime_material_dose_t_ha` divide without
      a guard of their own**, and are safe only because every current call
      site goes through `highest_rated`'s `> 0.0` filter first. Move the
      guard in, or state the precondition in their doc comments — the same
      shape of bug was live in `net_requirement_kg_ha` until session 9.
- [ ] **Decide what `AnnualClimatology::min_temp_c` / `humidity_pct` /
      `wind_ms` are for.** The POWER adapter fills all three; no rule reads
      any of them. Either a rule should consume them or they should go.

## One item worth knowing about

During session 1, file-write paths under an intended `non_nobis_solum/`
subdirectory silently landed at the repo root instead (tooling quirk, not
a data-loss issue — verified nothing pre-existing was overwritten). The
user decided to keep the Rust project at the repo root rather than move it
into its own subdirectory, so `Cargo.toml`/`src/`/`data/` now live
alongside `Non-Nobis-Solum-Py/`, `docs/`, `assets/`, etc.
