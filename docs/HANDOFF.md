# Handoff — Rust fertility plan engine

A hexagonal (ports & adapters) Rust CLI at the repo root (`Cargo.toml`,
`src/`, `data/`), package name `non_nobis_solum`, built on top of the
`Non-Nobis-Solum-Py` prototype. Three sessions so far: (1) the initial
Rust port, (2) folding Bertsch et al.'s nutrient-removal/soil-standard
tables into the reference data, (3) integrating a 500-product Colombian
fertilizer vademecum.

## Status

- `cargo build`: compiles clean except expected `dead_code` warnings (see
  "Known gaps").
- `cargo test`: 5/5 passing — pure domain math in
  `src/core/domain/services.rs`, pinned against the prototype's formulas
  with literal inputs (not tied to any reference-data row, so data changes
  never break these).
- CLI verified end-to-end across all three sessions:
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
data/curated/   two example lots (LOT-001, LOT-002) with soil tests + field context
docs/rust-architecture.md   full architecture write-up, data layout, and how to
                            add a new reference profile
assets/         literature source files these CSVs were transcribed from
                (conversion-factors.csv, fertilizantes_colombia.csv)
```

Full explanation of the architecture, data layout and how to add a new
reference profile is in `docs/rust-architecture.md` (kept in sync with
sessions 2 and 3).

## Known gaps

- Only N/P/K/S/Ca/Mg are planned; micronutrient (`Fe`/`Mn`/`Zn`/`Cu`/`B`/`Mo`)
  enum variants exist and now have real reference data (critical levels
  since session 2, fertilizer composition since session 3) but aren't
  wired into a use case yet.
- Ca/Mg removal coefficients started as illustrative estimates
  (`source=illustrative_estimate`); session 2 replaced 12 of 16 crops with
  real literature values. `coffee`, `cassava`, `bean`, `pasture` still
  illustrative (no confident crosswalk found — see checklist).
- `tui_adapter` added in session 5 (`nns-tui` binary); `InspectScenario`
  still has no input port, so the TUI calls its inherent method.
- Several entities declared per the original spec
  (`SoilSample`, `NutrientDemand`, `Availability`, `DemandType`,
  `CropCatalogRepository::get_crop`, `FertilizerSourceRepository::get_source`)
  aren't exercised by the single demo use case yet — hence the
  `dead_code` warnings on `cargo build`. Harmless, not a bug.
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

## Checklist — data still to gather / implement

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
  - [ ] **`efficiency_rules.yaml` has the identical shape of gap but is
        NOT fixed** — only `loam`/`clay_loam` are populated there too, but
        unlike critical levels its N/P/K ranges genuinely differ between
        those two textures (e.g. N rainfed: 0.55–0.65 for `loam` vs.
        0.45–0.55 for `clay_loam`), so an `"any"` fallback would be
        scientifically dishonest — it needs real per-texture-class
        efficiency data, not a data-layer trick. Worth noting this one
        *does* crash: `CalculateFertilityPlan` calls
        `get_efficiency_range(...)?` (hard-propagates), so any lot with a
        texture outside `{loam, clay_loam}` still fails `plan` (though not
        `inspect`, which also uses `.ok()` there).
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

## One item worth knowing about

During session 1, file-write paths under an intended `non_nobis_solum/`
subdirectory silently landed at the repo root instead (tooling quirk, not
a data-loss issue — verified nothing pre-existing was overwritten). The
user decided to keep the Rust project at the repo root rather than move it
into its own subdirectory, so `Cargo.toml`/`src/`/`data/` now live
alongside `Non-Nobis-Solum-Py/`, `docs/`, `assets/`, etc.
