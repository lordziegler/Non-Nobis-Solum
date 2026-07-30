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
  `ListSupportedCrops`, `InspectScenario`, `ListLots` and `RegisterLot`.
  They depend only on the port traits, never on a concrete adapter.
- `src/infra` — concrete adapters (CSV/TOML/YAML readers, the `clap`
  CLI, the `ratatui` TUI, the NASA POWER HTTP client) and `bootstrap.rs`,
  the composition root: the only place that knows about file paths,
  network endpoints, and how adapters wire into use cases.

Dependency direction is one-way: `infra` depends on `core`, never the
reverse. `tui_adapter` was added next to `cli_adapter` without touching
`core` at all — same ports, same use cases, different frontend.

### Optional and remote adapters

`agroclimatic_adapter` is the one adapter that crosses a network instead
of reading a local file, and it's wired differently because of it:

- `CalculateFertilityPlan` takes it as `Option<Box<dyn
  AgroclimaticRepository>>`. Every other repository is mandatory — its
  absence is a bug — but a climate provider that's merely unreachable is
  an expected Tuesday.
- The use case collapses "switched off", "lot has no coordinates" and
  "provider is down" into a single `None`, and falls back to baseline
  constants. It never returns `Err` because climate failed.
- `DomainError::ExternalServiceUnavailable` exists to mark errors callers
  are expected to *degrade* on rather than propagate.
- The port names no provider, endpoint or parameter code, so a second
  provider is a new file in `agroclimatic_adapter/` plus one line in
  `bootstrap.rs`. A provider that can't supply some variable leaves that
  field `None` rather than widening the trait.

The general rule: an adapter that can fail for reasons the user can't fix
should be optional at the composition root and degrade in the use case,
not propagate.

A front-end that must not block gets a second wiring of the same adapter:
`CachedAgroclimaticRepo` is shared as an `Arc` (its provider is
`Send + Sync`), a background thread fills it, and the render loop reads it
through `PrewarmedAgroclimaticRepo`, whose cache miss is an
`ExternalServiceUnavailable` — the degradation path that already existed.
The CLI, which is allowed to wait, keeps the blocking wiring.

### The write path

`CuratedDataWriter` is the only output port that changes anything on
disk, and `RegisterLot` the only use case that holds one. Two rules keep
it boring:

- **Append-only, and the last row wins.** Creating a lot, a sample or a
  planning row is all the write access anything needs today. Editing an
  existing row is a read-modify-rename contract; it can be added when
  something asks. Until then the two curated readers that can see a
  repeated key collapse it themselves — `CsvSoilTestsRepo` keyed on
  (nutrient, depth), `CsvYieldTargetsRepo` on (field, crop) — because a
  correction can only arrive as a second row, and a reader that kept the
  first would let the app accept a corrected lab value and then silently
  plan on the stale one. A duplicate `field_id` is the exception: it is
  refused up front by `RegisterLot` rather than collapsed, since a second
  lot row means a mistake, not a revision.
- **Validation belongs to the use case, not the adapter and not the
  front-end.** `LotRegistration`/`SoilTestEntry` carry raw text for every
  field, including numbers, so a CLI, a TUI or an HTTP handler all reach
  the same parsing and the same range checks. The writer only ever sees
  domain types that already parsed.

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
cargo run -- plan --lot LOT-001 --crop corn --no-climate
```

New lots and samples are curated from the TUI (`nns-tui`, modules "New
lot" / "New sample"); the CLI has no write subcommand.

`plan` queries NASA POWER for the lot's coordinates unless `--no-climate`
is given. It never fails on that account: with no network, no
coordinates, or the flag set, it prints one stderr warning and falls back
to baseline constants. Output labels which regime produced each figure
(`[climate-adjusted, T=13.2°C]` vs `[baseline — no climate data]`).

`--lot` is used as both the sample ID (for `soil_tests.csv`) and the
field ID (for `field_context.csv`) — one lot, one composite sample, one
context row, matching the two sample rows shipped in
`data/curated/`. `--yield-value`/`--yield-unit` are optional; when
omitted, the plan falls back to `data/curated/yield_targets.csv`.

## The `"any"` sentinel convention

Three reference tables share one lookup shape: **exact match first,
sentinel row second**. A row keyed `"any"` means "this value does not
vary along that axis, as far as the data we have knows", and a row naming
a specific value always beats it — so real per-class data can be added
one row at a time without touching code.

- `critical_levels.csv`, `texture: any` — the literature behind the file
  (Olsen guidelines, Castro-Gomez 2009 Tabla 12) genuinely doesn't
  differentiate thresholds by texture.
- `critical_levels.csv` and `liming_rules.toml`, `region: any` — a
  reference file already lives inside a profile directory, so it answers
  for whatever region a lot's `field_context.csv` row claims. Without
  this, `--profile global` on a lot tagged `region=andina_colombia`
  dropped every `soil_status` to `None` and failed the liming lookup
  outright.
- `efficiency_rules.yaml`, `texture: any, irrigation: any` — **the one
  sentinel that is not a statement about the science.** Its N/P/K ranges
  do differ between the covered textures; the sentinel exists because 44
  of the 48 combinations have no data at all, and a lot outside the grid
  needs *a* plan more than it needs a fabricated coefficient. Those rows
  are tagged
  `source: documented_fallback_NOT_literature_envelope_of_covered_rows`
  and their ranges are the envelope of the same file's curated rows, not
  new numbers. Treat any plan that lands on them as provisional.

## Known simplifications
- Only the six macronutrients (N, P, K, S, Ca, Mg) are planned;
  micronutrient enum variants exist but aren't wired into a use case yet.
- `CalculateFertilityPlan` picks, per nutrient, the single fertilizer
  source with the highest composition percentage — no blending or cost
  optimization across sources.
- Ca/Mg removal coefficients in the shipped CSVs are illustrative
  estimates (tagged `source=illustrative_estimate`), not literature
  values like the N/P/K/S rows ported from the prototype — replace them
  with local data before using this for real recommendations.
- Climate enrichment uses a **30-year climatology**, not the season being
  planned: it characterizes a site, not a year. The three efficiency
  adjustments are uncalibrated round rules of thumb (one flat 0.05
  penalty each), and reference ET0 is derived by Hargreaves rather than
  Penman-Monteith because NASA POWER exposes no ET0 parameter at all —
  see `docs/HANDOFF.md`, session 7.
- The TUI's climate is best-effort *in time* as well as in availability:
  the first plan for a lot usually lands before the background fetch
  does, and runs on baseline constants. Asking again a moment later picks
  the climatology up. Both states are labelled on screen.
- Nothing is exported: no plan file, no plan history, and TUI settings
  (language, profile) still reset on exit.
- The write path only appends. A correction is a second row that
  supersedes the first on read (see "The write path"), so the values the
  app uses are always right, but the file grows and the superseded rows
  stay visible in it. Deleting a lot, or a sample, is not possible at all.

## Tests

```bash
cargo test
```

Domain service tests in `src/core/domain/services.rs` pin the agronomic
math (soil weight, availability, net requirement, dose) against known
values from the prototype.
