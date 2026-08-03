# Import templates

Three fillable sheets, one per shape the importer understands. It tells them
apart by their **header**, so keep the column names exactly as they are —
order does not matter and any extra column is ignored.

| File | Recognised by | What it does |
|---|---|---|
| `import-template-lots.csv` | a `texture` column | registers a lot, and its first yield goal if the crop columns are filled |
| `import-template-soil-tests.csv` | a `nutrient_id` column | adds lab readings to a lot that already exists |
| `import-template-yield-targets.csv` | a `yield_value` column | adds a goal for another crop on a lot that already exists |

Import them **in that order**: a reading needs its lot, a goal needs its lot.

```
nns import path/to/import-template-lots.csv
```

or press `i` in `nns-tui` and browse to the file. Every row is validated the
way a typed one is; a rejected row is reported with its line number and the
rest of the file still lands. Each template ships one filled row as an
example — replace it with your own before importing.

## Lots

`field_id` is yours to choose and is what every other file refers to.

| Column | Required | Notes |
|---|---|---|
| `field_id` | yes | must not already exist |
| `texture` | yes | `sand` `loamy_sand` `sandy_loam` `loam` `silt_loam` `silt` `sandy_clay_loam` `clay_loam` `silty_clay_loam` `sandy_clay` `silty_clay` `clay` |
| `irrigation_system` | yes | `rainfed` `gravity` `sprinkler` `drip` |
| `organic_matter_percent` | yes | percentage, 0–100 |
| `ph` | yes | 0–14 |
| `cec` | yes | cmolc/kg |
| `bulk_density_kg_dm3` | yes | kg/dm³ |
| `arable_depth_m` | yes | metres |
| `region` | yes | the reference profile that answers for this lot: `global` or `andina_colombia` |
| `latitude`, `longitude` | no | decimal degrees; both are what a climatology is fetched with |
| `altitude_m` | no | metres above sea level |
| `area_ha` | no | hectares; without it the plan is per hectare |
| `crop_id`, `yield_value`, `yield_unit` | no | the lot's first planning row, all three together. `yield_unit` is `t_ha` |

Only the textures and irrigation systems that the active profile's
`efficiency_rules.yaml` covers can be planned. A lot outside that grid is
registered fine and says so when a plan is asked for.

## Soil tests

One row per reading. `sample_id` is the `field_id` of the lot it belongs to.

| Column | Required | Notes |
|---|---|---|
| `sample_id` | yes | the lot must already exist |
| `nutrient_id` | yes | `P` `K` `S` `Ca` `Mg` `Fe` `Mn` `Zn` `Cu` `B` `Mo` `Al` `H` |
| `value` | yes | zero is a real reading — "below detection" |
| `unit` | yes | `mg_per_kg` or `cmolc_per_kg` |
| `method_id` | yes | the lab's extraction method, e.g. `Olsen`, `Bray_II`, `AcONH4_1N_pH7`, `KCl_1N`, `DTPA`, `hot_water` |
| `depth_from_cm`, `depth_to_cm` | yes | `depth_to_cm` must be deeper than `depth_from_cm` |

`N` is not a reading: nitrogen availability is derived from organic matter
and the climate, so a value for it would be accepted and then ignored.

Cations are usually reported in `cmolc_per_kg` and the rest in `mg_per_kg`;
either unit is accepted for the cations and converted where needed.

## Yield targets

A lot registered with `crop_id` already has one goal. This file is for the
others.

| Column | Required | Notes |
|---|---|---|
| `field_id` | yes | the lot must already exist |
| `crop_id` | yes | one of the ids `nns crops` lists |
| `yield_value` | yes | greater than zero |
| `yield_unit` | yes | `t_ha` |

A plan needs a goal for the (lot, crop) pair it is asked about. Without one
the app says so rather than guessing.
