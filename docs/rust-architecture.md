# Non Nobis Solum — fertility plan engine

A CLI fertilization-plan engine driven by soil analysis, crop nutrient
removal coefficients, use-efficiency rules and fertilizer sources. Ported
from the `Non-Nobis-Solum-Py` prototype into a hexagonal Rust core so
reference agronomic data — the part nobody should re-type by hand — lives
in versioned, swappable data files instead of scattered constants.

## Architecture

Strict ports & adapters:

- `src/core/domain` — entities and pure functions. No IO, no `csv`/`toml`/
  `serde_yaml`/`clap` dependency. `services.rs` holds the actual
  agronomic math (availability, crop removal, net requirement, product
  dose), ported from the prototype's `n.py`/`p.py`/`k.py` formulas.
- `src/core/ports` — traits only. `output.rs` is what the domain needs
  from the outside world (repositories); `input.rs` is what the outside
  world can ask the domain to do (use cases).
- `src/core/application` — the use cases: `CalculateFertilityPlan`,
  `ListSupportedCrops`, `InspectScenario`. They depend only on the port
  traits, never on a concrete adapter.
- `src/infra` — concrete adapters (CSV/TOML/YAML readers, the `clap`
  CLI) and `bootstrap.rs`, the composition root: the only place that
  knows about file paths and wires adapters into use cases.

Dependency direction is one-way: `infra` depends on `core`, never the
reverse. A `tui_adapter` can be added later next to `cli_adapter` without
touching `core` at all — same ports, same use cases, different frontend.

## Data layout

```
data/
  reference/            # scientific literature, encoded as tables, versioned in Git.
    global/              # conversion_factors.toml and crops.csv are shared across
      crops.csv           #   every profile (universal, not regional); global's own
      conversion_factors.toml   fertilizer_sources.csv is the small generic/international
      fertilizer_sources.csv   default used by any profile that doesn't ship its own.
      nutrient_removal.csv
      efficiency_rules.yaml
      critical_levels.csv
      liming_rules.toml
      liming_materials.csv
    andina_colombia/      # a second profile: local removal coefficients, critical
      nutrient_removal.csv       levels, AND its own regional commercial fertilizer
      efficiency_rules.yaml      catalog (500+ products from a Colombian vademecum) —
      critical_levels.csv        overrides global's generic one for this profile.
      fertilizer_sources.csv
      liming_rules.toml
      liming_materials.csv
  curated/               # scenario-specific data: one row per real sample/field/plan.
    soil_tests.csv
    field_context.csv
    yield_targets.csv
```

The end user never fills in removal coefficients, conversion factors,
efficiency ranges or critical levels — they pick a **profile**
(`--profile global` or `--profile andina_colombia`) and the reference
adapters load the matching tables. Only `data/curated/*` is meant to grow
with real field data.

`crops.csv` and `conversion_factors.toml` are always read from `global`
(`DataLayout::shared_reference`), regardless of profile — taxonomy and unit
science don't vary regionally. `nutrient_removal.csv`, `critical_levels.csv`
and `fertilizer_sources.csv` are read from the *active* profile's own folder
(`DataLayout::reference`); `global`'s copies double as the fallback default
for any profile that doesn't ship its own (there's no automatic fallback in
code — a profile missing one of these three files simply errors).

### Adding a new reference profile

1. Create `data/reference/<new_profile>/` with `nutrient_removal.csv`,
   `efficiency_rules.yaml`, `critical_levels.csv`, `liming_rules.toml` and
   `liming_materials.csv`, same columns as the existing profiles.
2. If the new profile also needs its own crop catalog, add `crops.csv`
   there too and point `bootstrap.rs` at `layout.reference(...)` instead
   of `layout.shared_reference(...)` for that file — currently shared
   from `global` since crop taxonomy doesn't usually vary by agronomic
   region. `fertilizer_sources.csv` already follows this per-profile
   pattern (`andina_colombia` has its own regional catalog; any profile
   without one needs to add it, there's no automatic fallback to
   `global`'s).
3. Run with `--profile <new_profile>`. No `core` code changes.

## Liming

`CalculateFertilityPlan` computes a lime recommendation
(`FertilityPlan.liming: Option<LimingRecommendation>`) only when the
sample has an Al³⁺ soil test (`Nutrient::Al`, cmolc/kg) — the workflow
reference's "encalamiento si aplica". `Nutrient::Al`/`Nutrient::H` reuse
`SoilTestRepository`/`soil_tests.csv` exactly like Ca/Mg/K (same lab
panel, same unit); they're deliberately excluded from
`Nutrient::MACRONUTRIENTS` since they're acidity indicators, not
fertilization targets.

Two independent CaCO3-equivalent requirements are computed — from
exchangeable Al³⁺ toxicity, and from raising base saturation (computed
from CICE, the sum of soil-test cations, *not* `FieldContext.cec_cmolc_kg`
which is a different standard measurement — see the doc comment on
`services::cation_exchange_capacity_effective`) to
`LimingRulesRepository::target_base_saturation_pct` — and the larger of
the two is recommended. `LimingMaterialRepository` (`liming_materials.csv`)
is a separate catalog from `FertilizerSourceRepository`
(`fertilizer_sources.csv`): liming materials are graded by CaO/MgO
(neutralizing value) rather than elemental Ca/Mg (nutrient supply), so
reusing one catalog for both would misrepresent the numbers.

## Usage

```bash
cargo run -- crops --profile global
cargo run -- plan --lot LOT-001 --crop corn --profile andina_colombia
cargo run -- plan --lot LOT-001 --crop corn --yield-value 10 --yield-unit t_ha
cargo run -- inspect --lot LOT-002 --crop coffee --profile andina_colombia
```

`--lot` is used as both the sample ID (for `soil_tests.csv`) and the
field ID (for `field_context.csv`) — one lot, one composite sample, one
context row, matching the two sample rows shipped in
`data/curated/`. `--yield-value`/`--yield-unit` are optional; when
omitted, the plan falls back to `data/curated/yield_targets.csv`.

## Known simplifications

- `data/curated/field_context.csv`'s `region` column is `andina_colombia`
  for both shipped lots regardless of the `--profile` flag chosen — a
  pre-existing mismatch (region and profile are independent knobs).
  `CriticalLevelsRepository` swallows the resulting `NotFound` with `.ok()`
  (silently drops `soil_status` to `None`, reproducible today via
  `cargo run -- plan --lot LOT-001 --crop corn --yield-value 10 --yield-unit t_ha`,
  which defaults to `--profile global` and shows `Status: -` on every row).
  `LimingRulesRepository` does **not** swallow it (`?`, hard-propagates,
  same as `EfficiencyRulesRepository`) — recommended for a lookup the
  liming math can't meaningfully proceed without — so `--lot LOT-002`
  (the only curated lot with an Al³⁺ test) under `--profile global`
  fails outright instead of degrading quietly. Not a regression: this
  exact combination was never a validated/documented path. Worth fixing
  before curated data grows past two illustrative lots.
- `critical_levels.csv` rows use the sentinel texture `"any"` instead of
  one row per USDA texture class — the literature behind this file
  (Olsen guidelines, Castro-Gomez 2009 Tabla 12) doesn't differentiate
  low/medium/high thresholds by texture, so a single `"any"` row covers
  every texture until real per-texture data exists.
  `CsvCriticalLevelsRepo::get_critical_level` matches an exact texture
  first and falls back to `"any"`; `efficiency_rules.yaml` has no such
  fallback (its N/P/K ranges *do* differ meaningfully between `loam` and
  `clay_loam`), so a lot with any texture other than those two still
  fails `plan`/`inspect` — real per-texture-class efficiency data is
  needed before that file can grow past two rows.
- Only the six macronutrients (N, P, K, S, Ca, Mg) are planned;
  micronutrient enum variants exist but aren't wired into a use case yet.
- `CalculateFertilityPlan` picks, per nutrient, the single fertilizer
  source with the highest composition percentage — no blending or cost
  optimization across sources.
- Ca/Mg removal coefficients in the shipped CSVs are illustrative
  estimates (tagged `source=illustrative_estimate`), not literature
  values like the N/P/K/S rows ported from the prototype — replace them
  with local data before using this for real recommendations.
- `infra::tui_adapter` doesn't exist yet — the ports and use cases are
  already frontend-agnostic, so it's a pure addition when needed.

## Tests

```bash
cargo test
```

Domain service tests in `src/core/domain/services.rs` pin the agronomic
math (soil weight, availability, net requirement, dose) against known
values from the prototype.
