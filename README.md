# Non Nobis Solum

Fertilization planning from a soil analysis. Give it a lot, a crop and a
yield goal; it returns the net nutrient requirement, a product dose per
nutrient, a soil-status reading against critical levels, and a lime
requirement when the sample reports exchangeable aluminium.

The agronomy is data, not code: removal coefficients, use efficiencies,
critical levels, liming rules and fertilizer composition all live in
tables you can read and correct. Nitrogen is derived from organic matter
with a mineralization factor modulated by climate, fetched from NASA
POWER for lots that have coordinates.

## Install

```sh
cargo install --git https://github.com/lordziegler/Non-Nobis-Solum.git
```

That installs two commands into `~/.cargo/bin`:

| | |
|---|---|
| `nns` | the command-line interface — plan, inspect, list crops |
| `nns-tui` | the full terminal interface, and the only way to enter data |

To remove it: `cargo uninstall non_nobis_solum`.

### Desktop entry (optional)

To have the interface show up in application menus and launchers:

```sh
install -Dm644 nns.desktop ~/.local/share/applications/nns.desktop
```

The entry is `Terminal=true`, so the launcher has to know which terminal
emulator to open it with. In fuzzel that is the `terminal=` key in
`fuzzel.ini`; without it the entry is listed but starts nothing.

## Use

The first run creates the data catalog. Reference tables are seeded in
full; your own records start empty, so begin by registering a lot:

```sh
nns-tui
```

`n` opens the lot form, `s` adds a soil-test reading, `f` plans. Then, on
the command line:

```sh
nns crops                                    # what the profile supports
nns plan --lot LOT-001 --crop coffee         # the plan
nns inspect --lot LOT-001 --crop coffee      # the data behind it
```

`nns plan --help` lists the rest: `--yield-value` to plan for a goal that
isn't curated, `--profile` to switch reference catalogs, `--no-climate`
to skip the network and use baseline constants.

Note that `nns` only reads. Registering lots and entering lab results is
`nns-tui`'s job.

### Trying it without any data of your own

`--test` swaps the catalog for a disposable one under the temp directory,
seeded with two demonstration lots. Your own records are neither read nor
written:

```sh
nns --test plan --lot LOT-001 --crop corn
nns --test inspect --lot LOT-002 --crop coffee
```

On its own it reports whether this installation can plan at all, and
exits non-zero if it cannot:

```
$ nns --test
catalog  /tmp/non-nobis-solum-test

seeded     17/17 files written                          ok
profiles   andina_colombia, global                      ok
crops      66 in the catalog                            ok
doses      LOT-001/corn: 290 kg/ha Urea                 ok
liming     LOT-002/coffee: 2.25 t/ha                    ok

all checks passed
```

The network is out of scope there: NASA POWER being unreachable is not an
installation fault, and the engine is required to run without it.

## Where your data lives

```
~/.local/share/non-nobis-solum/       ($XDG_DATA_HOME is honoured)
├── reference/
│   ├── global/                       literature tables, portable
│   └── andina_colombia/              regional overrides
└── curated/                          your lots, analyses and yield goals
```

Everything is CSV, TOML and YAML on purpose: correcting a threshold you
disagree with is editing a file, not patching the program.

**A file is only ever written if it is absent — nothing is overwritten.**
A table you have edited survives every later run. The same rule means an
upgrade carrying a corrected table will not replace the copy you already
have; delete the file and run again to get the shipped one back.

Adding a reference profile takes no code change: copy
`reference/global/` to `reference/<name>/`, edit it, and pass
`--profile <name>`.

## Building from source

```sh
git clone https://github.com/lordziegler/Non-Nobis-Solum.git
cd Non-Nobis-Solum
cargo test
cargo run -- --data-dir data crops
```

`--data-dir` points the program at a catalog of your choosing and seeds
nothing, which is how to work against this repository's own `data/`
without touching your installed copy.

The layout is hexagonal: `src/core/` holds the domain and the use cases
and knows nothing about files or terminals, `src/infra/` holds every
adapter, and `src/infra/bootstrap.rs` is the only module that knows a
path. The dependency runs one way, `infra` to `core`, including in tests.

## Status

Working software, not a finished product. The engine computes and the
interfaces work, but several coefficients are still documented estimates
rather than measured values — nitrogen use efficiency above all — and
micronutrients have reference data with no use case reading it. Anything
resting on an assumption says so in the table it comes from.

## Licence

Copyright (C) 2026 Sebastian Ziegler

This program is free software: you can redistribute it and/or modify it
under the terms of the GNU General Public License as published by the
Free Software Foundation, either version 3 of the License, or (at your
option) any later version.

This program is distributed in the hope that it will be useful, but
WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General
Public License for more details.

Full text in [`LICENSE`](LICENSE) (GPL-3.0-or-later), and at
<https://www.gnu.org/licenses/>.
