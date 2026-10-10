# Rustfort architecture and working rules

For the vision, see `VISION.md`. This file is the practical contract between the crates.

## Crates

| crate | responsibility | depends on |
|---|---|---|
| `rf-core` | data model (World, Tile, TiletypeTable, Unit, Item, Building, Plant, MaterialInfo, Calendar) plus `World::synthetic()` | serde |
| `rf-raws` | generic DF raw-file tokenizer/parser, object index (creatures, inorganics, plants, ...) plus graphics raws (TILE_PAGE, TILE_GRAPHICS, CREATURE_GRAPHICS, ...); finds the DF install (`rf_raws::df_dir()`, `df_dir_status()`) | rf-core |
| `rf-gfx` | loads the Premium tileset PNGs from the DF install and answers "which sprite(s) for this tile, unit, item or building". Renderer-agnostic: returns page + rect + tint (`rf_gfx::df_dir()` forwards to rf-raws) | rf-raws, rf-core |
| `rf-import` | reads `export/<fort>/` (see `docs/EXPORT_FORMAT.md`) into `rf_core::World` | rf-core |
| `rf-sim` | deterministic simulation: tick loop, pathfinding, unit AI, needs, jobs, designations | rf-core |
| `rf-app` | binary `rustfort`: macroquad window, Steam-style mouse UI, renderer, title screen and DF import, saves, plus the headless `bench` / `savebench` subcommands | all |
| `rf-save-v09` | the frozen v0.9 save types, used only to convert old (format 1) saves. **Never edit it.** | serde |

There is also `tools/dfhack/rustfort-export.lua`, a read-only DFHack exporter that runs inside the user's live DF.

## Rules

1. **No DF data in git.** Raws, PNGs and exports are read at runtime from the user's install or the export folder. Never commit DF files, fort exports, saves or screenshots.
   - DF install lookup (`rf_raws::df_dir()`): the env var `RUSTFORT_DF_DIR`, then the folder set in the title screen's Settings (`saves/settings.json`), then the Steam install (registry `SteamPath` / `InstallPath` and every library in `steamapps/libraryfolders.vdf`; on Linux `~/.steam/steam`, `~/.local/share/Steam`, the Flatpak path), then common default paths. A folder counts when it contains `data/vanilla`. `rf_raws::df_dir_status()` also returns where it was found (`RUSTFORT_DF_DIR`, `settings`, `Steam`, `default path` or `not found`).
   - Fort export lookup (the default fort of the app, `bench` and the examples): `--export DIR`, else env `RUSTFORT_EXPORT`, else the most recently modified complete export under `./export` (a folder with `manifest.json`), else none (the built-in synthetic fort).
   - The user's mods in `data/installed_mods` count as part of their install.
2. **rf-core is the shared contract.** Only additive changes are allowed. If you need something there, add it in a backwards-compatible way and mention it in the pull request.
3. **Deterministic simulation.**
   - No `HashMap` iteration where the order affects results.
   - RNG is a small splitmix64 (`rf-sim` `rng.rs`), seeded per world, unit (slot) and tick, so results do not depend on thread scheduling.
   - Parallel phases may only read; writes are applied in a fixed order.
   - [saves11] All sim state is saved: `Sim` and every module struct derive `serde::Serialize`/`Deserialize` (the save holds an exact snapshot, see `docs/SAVES.md`). A new field must be serializable, or `#[serde(skip)]` if it is a pure cache rebuilt in `Sim::after_exact_load`. Give module structs `#[serde(default)]`. A `&'static str` field is written as `crate::saves11::SStr` with `#[serde(with = "crate::saves11::interned")]`. Check with `cargo test -p rf-sim --release --test saves11`.
4. **Speed is a feature.** Keep hot loops allocation-free. Benchmark with `rustfort bench`.
5. **Clean-room.** The spec is the DF wiki plus observed behavior. Never decompile DF.
6. **Commits:** short imperative English messages; see `CONTRIBUTING.md`.

## Coordinates

- `Pos { x, y, z }` uses local embark tiles. z = 0 is the **bottom** of the map, as in DF.
- Tile index = `(z * dy + y) * dx + x`.
