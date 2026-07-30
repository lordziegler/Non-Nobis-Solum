# Blockers and roadmap — from prototype to usable tool

Audit of the tree as of session 7 (4,290 lines of Rust, `cargo test` 45/45
green). Every failure below was reproduced by running it, not inferred
from reading the code.

## Verdict

The calculation engine is complete: N from organic matter, availability,
crop demand, use efficiency, product dose, liming and climate enrichment
all work end to end. What does not exist is **everything upstream of the
calculation**. The application can only plan the two illustrative lots
shipped in `data/curated/`, with exactly the crop already hand-written in
`yield_targets.csv`. Of the 132 possible lot × crop combinations, **2
work**.

The four blockers below are chained: fixing the data-entry UI first would
ship a feature that fails the first time it is used on real data.

---

## Blocker 1 — no write path exists anywhere

```
grep -rn "File::create|fs::write|Writer::from_path|OpenOptions" src/   →  0 hits
```

All ten repositories in `core/ports/output.rs` are read-only without
exception. `data/curated/soil_tests.csv`, `field_context.csv` and
`yield_targets.csv` are maintained by opening them in a text editor.

This is not a missing screen in the TUI: the operation does not exist at
any layer — no port, no use case, no adapter.

## Blocker 2 — the TUI crop selector is broken in practice

The Crops screen offers all 66 catalog crops and stores the choice in
`crop_override` (`src/infra/tui_adapter/mod.rs:344-350`). But `scenario()`
builds the scenario with `yield_override: None`
(`src/infra/tui_adapter/mod.rs:187`), so the plan looks the yield goal up
in `yield_targets.csv` — which has two rows.

Reproduced:

```
plan --lot LOT-002 --crop corn    → error: no yield target for field_id=LOT-002 crop_id=corn
plan --lot LOT-001 --crop banana  → error: no yield target for field_id=LOT-001 crop_id=banana
```

**64 of the 66 crops the TUI offers produce an error when planned.** The
feature is visible to the user and does not work. The CLI already has the
escape hatch (`--yield-value` / `--yield-unit`); the TUI simply declined
to use it.

## Blocker 3 — the efficiency grid covers 8% of the possible cases

`efficiency_rules.yaml` (both profiles) has rows for 2 textures
(`loam`, `clay_loam`) × 2 irrigation systems (`rainfed`, `drip`). The
domain defines **12 textures × 4 irrigation systems = 48 combinations**.
Four are covered.

The texture half of this gap is documented in `rust-architecture.md`; the
irrigation half is not documented anywhere. A `loam` lot under `gravity`
or `sprinkler` fails just as hard. Unlike `critical_levels.csv`, this
repository has no `"any"` fallback — `YamlEfficiencyRulesRepo` propagates
with `?`, so both `plan` and `inspect` fail outright.

This is what makes Blocker 1 useless on its own: the moment a user creates
a real lot with a sandy-loam texture or sprinkler irrigation, there is no
plan.

## Blocker 4 — `region` and `--profile` are independent knobs that collide

`field_context.csv` hardcodes `region=andina_colombia` for both lots
regardless of the `--profile` flag. Reproduced:

```
plan --lot LOT-002 --crop coffee --profile global
  → error: not found: no liming rules for region=andina_colombia
```

`CriticalLevelsRepository` swallows the `NotFound` with `.ok()` (degrades
to `Status: -`), `LimingRulesRepository` does not. Perfectly reasonable
profile + lot combinations therefore hard-fail.

---

## Second-tier gaps

| Gap | State |
|---|---|
| **`product` is always `"grain"`** | All 66 rows of `nutrient_removal.csv` use `product=grain`, including apple, rose and boston fern. The TUI hardcodes it (`tui_adapter/mod.rs:185`); the CLI accepts `--product` but any other value fails. Real harvested organ per crop: outstanding (see `HANDOFF.md` checklist). |
| **`andina_colombia` has 16 of 66 crops** | Only `global` received the full catalog. Switching profile silently shrinks what can be planned. |
| **Micronutrients unwired** | `Nutrient` has the variants and `critical_levels.csv` has thresholds for B/Mn/Cu/Zn/Fe/Na, but `Nutrient::MACRONUTRIENTS` excludes them. The TUI renders them muted (`UNPLANNED_MICRONUTRIENTS`) — honest, but it is reference data with no consumer. |
| **Two missing input ports** | No `ListLots` (the TUI reads `yield_targets.csv` straight from the composition root, `bootstrap.rs:101`) and no `InspectScenarioPort` (the inspect screen calls the inherent method). Both already carry `TODO(gap)` markers. Architectural debt, not a functional gap. |
| **Climate is CLI-only** | The TUI passes `None`. The same lot yields a 0.015 mineralization factor from the TUI and 0.0102 from the CLI — **the two front-ends disagree on the numbers**. Needs a background fetch or a pre-warmed cache; the fetch blocks with a 10 s timeout and the render loop is single-threaded. |
| **Nothing persists** | No plan export (CSV, Markdown, PDF), no plan history, and TUI settings (language, profile) reset on exit. |
| **Two `unwrap()` outside tests** | `calculate_fertility_plan.rs:80` and `:100`, both `partial_cmp(..).unwrap()`. A `NaN` composition percentage in the fertilizer catalog panics. |

---

## Roadmap

Ordered so that no phase ships a feature the next phase is required to
make usable.

### Phase 1 — make the grid answer for any lot (data, not code)

Either populate `efficiency_rules.yaml` for the remaining texture ×
irrigation combinations, or give `YamlEfficiencyRulesRepo` the same
`"any"` fallback `CsvCriticalLevelsRepo` already implements (exact match
first, sentinel row second).

This is an agronomic decision, not a technical one: real per-class data,
or a documented fallback. Until one of the two exists, lot creation is a
trap.

### Phase 2 — yield override in the TUI (cheapest unblock)

Add a numeric input to the Crops screen so picking a crop that has no
curated yield target prompts for one, and pass it through
`FertilityScenario::yield_override` — the field already exists and the CLI
already uses it. Unblocks 64 crops today. No `core` changes.

### Phase 3 — the write path

1. New output port in `core/ports/output.rs`:
   ```rust
   pub trait CuratedDataWriter {
       fn save_field_context(&self, context: &FieldContext) -> Result<(), DomainError>;
       fn save_soil_tests(&self, tests: &[SoilTest]) -> Result<(), DomainError>;
       fn save_yield_target(&self, field_id: &str, crop_id: &str, target: &YieldTarget) -> Result<(), DomainError>;
   }
   ```
2. CSV adapters using `csv::Writer` in append mode. Rewrite-in-place (read,
   modify, write to a temp file, rename) is only needed once editing an
   existing row is supported — append covers creation.
3. New use case `RegisterLot` with validation at the trust boundary:
   texture and irrigation must parse, numeric values must be positive,
   the `field_id` must not already exist.
4. TUI: an "add lot" and an "add sample" form, plus the `ListLots` input
   port so the lot list stops being read from the composition root.

### Phase 4 — reconcile `region` with `--profile`

Either derive `region` from the active profile, or make
`LimingRulesRepository` degrade the way `CriticalLevelsRepository` does.
Worth doing before curated data grows past the two illustrative lots.

### Phase 5 — parity and polish

Climate in the TUI (needs the background fetch first), the
`InspectScenarioPort` trait, plan export, persisted TUI settings, and the
two `unwrap()` calls.

---

## Before phase 3

The repository has a single commit and everything from sessions 2 through
7 is unversioned. Commit before adding code that writes into
`data/curated/`.
