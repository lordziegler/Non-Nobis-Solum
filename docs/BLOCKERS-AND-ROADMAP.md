# Blockers and roadmap — from prototype to usable tool

Audit of the tree as of session 7, **resolved in session 8**. Every
failure below was reproduced by running it before the fix and after; the
commands are in `docs/HANDOFF.md`, session 8.

## Verdict (session 7, kept for the record)

The calculation engine is complete: N from organic matter, availability,
crop demand, use efficiency, product dose, liming and climate enrichment
all work end to end. What does not exist is **everything upstream of the
calculation**. The application can only plan the two illustrative lots
shipped in `data/curated/`, with exactly the crop already hand-written in
`yield_targets.csv`. Of the 132 possible lot × crop combinations, **2
work**.

## Status after session 8

| Blocker | State |
|---|---|
| **1 — no write path exists anywhere** | **Fixed.** `CuratedDataWriter` output port, `CsvCuratedWriter` adapter (append-only, `csv::Writer` quoting), `RegisterLot` use case with validation, `RegisterLotPort`, and two TUI forms. |
| **2 — the TUI crop selector is broken in practice** | **Fixed.** Picking a crop with no curated yield goal prompts for one and passes it through `FertilityScenario::yield_override`. The CLI's `--yield-value` was and is the equivalent escape hatch. |
| **3 — the efficiency grid covers 8% of the possible cases** | **Fixed by fallback, not by data.** `YamlEfficiencyRulesRepo` falls back to an `("any", "any")` sentinel row. The 44 uncovered combinations still have **no real per-class data** — see "What an agronomist still owes this repo". |
| **4 — `region` and `--profile` are independent knobs that collide** | **Fixed.** Both `LimingRulesRepository` and `CriticalLevelsRepository` accept the sentinel region `"any"`, and the shipped reference rows use it. Soil status now resolves under `--profile global` too. |

### Second-tier gaps

| Gap | State |
|---|---|
| **`product` is always `"grain"`** | **Open.** All 66 rows of `nutrient_removal.csv` use `product=grain`. Real harvested organ per crop: outstanding (see `HANDOFF.md` checklist). |
| **`andina_colombia` has 16 of 66 crops** | **Open.** Only `global` received the full catalog. |
| **Micronutrients unwired** | **Open.** Reference data exists, `Nutrient::MACRONUTRIENTS` excludes them, the TUI renders them muted. |
| **Two missing input ports** | **Fixed.** `ListLotsPort` (with a `ListLots` use case reading field contexts, not planning rows) and `InspectScenarioPort` both exist; `bootstrap::App::lots` is gone. |
| **Climate is CLI-only** | **Fixed.** The TUI prefetches a climatology on a background thread and plans against a non-blocking `PrewarmedAgroclimaticRepo`. A plan asked for before the fetch lands runs on baseline constants and labels itself as such. |
| **Nothing persists** | **Open** for plan export (CSV/Markdown/PDF), plan history and TUI settings (language, profile still reset on exit). Curated *data* now persists. |
| **Two `unwrap()` outside tests** | **Fixed.** Both call sites share `highest_rated`, which drops NaN before ordering with `total_cmp`. |

---

## What an agronomist still owes this repo

Nothing below was invented to make the code run. These are the places
where the code deliberately answers with a documented fallback instead of
a number nobody has measured.

1. **The 44 uncovered texture × irrigation efficiency combinations.**
   `efficiency_rules.yaml` (both profiles) now ends with six sentinel
   rows tagged
   `source: documented_fallback_NOT_literature_envelope_of_covered_rows`.
   Each range is the envelope (lowest min, highest max) of that
   nutrient's four curated rows — a mechanical derivation, not a
   measurement. An exact row always wins over the sentinel, so real
   per-class data can be added one row at a time with no code change.
2. **S efficiency contradicts the workflow reference** (8-10% there,
   85-100% here). Unresolved since session 4; an order-of-magnitude
   decision that changes S doses ~10x.
3. **Ca/Mg removal for `coffee`, `cassava`, `bean`, `pasture`** are still
   `illustrative_estimate`.
4. **`liming_materials.csv`** is three seed materials, all
   `illustrative_estimate` — real CaO/MgO/EG assays needed.
5. **Both curated lots share one illustrative coordinate pair** (Pasto),
   so their climatologies are identical by construction.
6. **The three climate efficiency rules are uncalibrated** (one flat 0.05
   penalty each, round thresholds).
7. **`product` per crop** (fruit, root, leaf, forage…) — a design
   decision plus a data-entry job, not one or the other.

---

## Roadmap — all five phases done

### Phase 1 — make the grid answer for any lot ✔

Sentinel fallback in `YamlEfficiencyRulesRepo` + six tagged rows per
profile. Chose the fallback over fabricating 44 × 6 values.

### Phase 2 — yield override in the TUI ✔

Numeric entry on the Crops screen, refused unless it is a number greater
than zero, cleared whenever the lot or crop selection changes. No `core`
changes. Unblocks the 64 catalog crops with no curated goal.

### Phase 3 — the write path ✔

Port, adapter, use case and two TUI forms, exactly as specified. Input
validation lives in `RegisterLot` and nowhere else: texture and
irrigation must parse, numbers must be finite and positive, pH and
organic matter must be inside their definitional ranges, coordinates
inside theirs, crop and yield goal are all-or-nothing, sample depths must
not be inverted, and a duplicate `field_id` is refused before anything is
written. Appending goes through `csv::Writer`, which quotes commas.

### Phase 4 — reconcile `region` with `--profile` ✔

A reference file already lives inside a profile directory, so its rows
answer for whatever region a lot claims: sentinel `"any"`, exact match
still wins. No threshold value changed, only the lookup key.

### Phase 5 — parity and polish ✔ (partly)

Done: climate in the TUI, `InspectScenarioPort`, both `unwrap()` calls.
**Not done:** plan export and persisted TUI settings — neither is a
blocker and both are new IO surfaces that deserve their own port
decision rather than being bolted on at the end of a session.

---

## Next, in rough order of value

1. **Real per-texture efficiency data** — the one fallback in the tree
   that is load-bearing for every uncovered lot.
2. **`product` per crop** — the last hardcoded agronomic assumption.
3. **Plan export** (a `PlanExporter` output port, Markdown first).
4. **Editing curated rows**, not just appending: read-modify-rename, a
   different contract from `CuratedDataWriter`.
5. **Persisted TUI settings** (language, profile).
6. **Micronutrients** into a use case, since the data is already there.
