# Reference tables

The scientific literature, encoded as tables. A user curates their own lots,
analyses and yield goals under `data/curated/`; nothing here is theirs to
edit, and the app treats this directory as read-only.

Every row carries the study it came from in a `source` column and, where the
study states one, a `year`. **That citation is deliberately not shown on
screen** — printed beside the figures it is longer than they are, and a
terminal clips it mid-token, so it identified nothing and cost the data its
room. This file is where it lives instead.

## Profiles

A profile is one directory of tables. `--profile` picks it, and the choice is
which body of literature to trust for a given region:

| Profile | What it is |
|---|---|
| `global` | The default. Broad international coefficients. |
| `andina_colombia` | The Colombian Andean tables, where they differ from the global ones. |

`conversion_factors.toml` is shared by every profile: unit and oxide
conversions are chemistry, not a regional judgement.

## Tables

| File | What it decides | Carries a source |
|---|---|---|
| `nutrient_removal.csv` | How much of a nutrient a tonne of yield asks for, on the extraction and absorption bases. | yes |
| `critical_levels.csv` | The low/medium/high boundaries a soil reading is classified against. | yes |
| `soil_quality_thresholds.csv` | The qualitative interpretation bands: pH classes, organic matter by thermal belt, the acidity diagnosis, the cation balance. | yes |
| `liming_materials.csv` | Neutralizing value and granulometric efficiency per liming material. | yes |
| `liming_rules.toml` | The aluminium factor and the target base saturation, per region. | yes |
| `efficiency_rules.yaml` | The base recovery range per nutrient, texture and irrigation system. | yes |
| `efficiency_bands.toml` | What a real site's conditions do to that base range. | per band |
| `fertilizer_sources.csv` | The purchasable product catalog: composition, form, sourcing restrictions. | no — a catalog, not a finding |
| `crops.csv` | The crops a plan can be written for. | no |
| `conversion_factors.toml` | Unit and elemental/oxide conversions. | no |

## Where the citations point

### Soil interpretation — `critical_levels.csv`, `soil_quality_thresholds.csv`, `liming_materials.csv`

All of it is Castro & Gómez (2009), by table:

- `Castro_Gomez_2009_tabla12` — general standards for interpreting a soil
  analysis. Phosphorus appears twice, under Bray II and under Olsen, with
  different boundaries: the reagents dissolve different fractions of soil P,
  so the extraction method is a lookup axis and not metadata.
- `Castro_Gomez_2009_tabla12_balance_de_bases` — the Ca:Mg, Mg:K, K:Mg, Ca:K
  and (Ca+Mg):K bands.
- `Castro_Gomez_2009_tabla12_diagnostico_acidez` — the acidity diagnosis.
- `Castro_Gomez_2009_tabla12_nota` — the combined liming mixture (hydrated
  lime / dolomite / Paz del Río slag, 40/45/15 of the CaCO₃ requirement).
- `Tabla4_MOS_horizonte_A_altitud_temperatura` — organic matter categories by
  altitude and temperature. The same 3% is *high* in the lowlands and *very
  low* above 2000 m, which is why this table is keyed on a thermal belt.

Scans of the source tables are under `docs/datasets/`.

### Crop demand — `nutrient_removal.csv`

360 rows drawn from Tabla 10 (extraction and absorption in vegetables,
cereals and fruit) and Tabla 11 (forage grasses and legumes, industrial
crops, flowers and ornamentals). Named by the study behind each row; a `+`
means the row combines them. Most-cited first:

| Source | Rows |
|---|---|
| `Bertsch_F_2003` (alone or combined) | 158 |
| `IFA_1992` (alone or combined) | 54 |
| `Guerrero_2001` | 41 |
| `INPOFOS_2003` / `INPOFOS_1999` | 32 |
| `IPNI` (`Norte_Latinoamerica_2007`, `NorthCentral_USA_2007`) | 43 |
| `Gomez_2006a` (alone or combined) | 18 |
| `Ciampitti_Garcia_2007` | 12 |
| `IPI_2007` | 12 |
| `Melgar_DiazZorita_1997` | 10 |
| `Torrez_Chinchilla_2007` | 6 |
| `Cenicafe_2008` | 6 |
| `Munevar_2001` | 6 |
| `Posada_2008` | 6 |
| `Garcia_et_al_2006` | 6 |
| `Halevy_Bazelet_1992` | 5 |

**`illustrative_estimate` (12 rows) is not a citation.** Those rows are
placeholders where no measured coefficient was available, and a plan built on
one is only as good as the guess. They are the first thing to replace.

### Efficiency — `efficiency_rules.yaml`, `efficiency_bands.toml`

The bibliography for the band model is in the module that consumes it, beside
the rule each entry justifies: see the module documentation of
`src/core/domain/efficiency.rs` (Havlin et al. 2014; Cameron, Di & Moir 2013;
Barber 1995; Germida & Janzen 1993; Kochian, Piñeros & Hoekenga 2005; Ladha
et al. 2005; Syers, Johnston & Curtin 2008; Dobermann et al. 2002). Each band
row also carries its own `basis` naming which of them it comes from.

## Editing these files

Seeding refreshes this directory on every start: it is the release's, not the
user's. An edited copy is not deleted — whatever differs from the shipped
file is moved aside to `<name>.bak` before the new one lands. To carry a
change forward, add it to the shipped table rather than to an installed copy.

Adding a row means adding its `source`. A figure nobody can trace is worse
than a missing one: a missing coefficient is reported as a warning on the
plan, a wrong one silently changes what somebody applies to a field.
