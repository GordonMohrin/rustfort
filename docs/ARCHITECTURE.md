# Rustfort architecture and working rules

For the vision, see `VISION.md`. This file is the practical contract between the crates.

## Crates

| crate | responsibility | depends on |
|---|---|---|
| `rf-core` | data model (World, Tile, TiletypeTable, Unit, Item, Building, Plant, MaterialInfo, Calendar) plus `World::synthetic()` | serde |
| `rf-raws` | generic DF raw-file tokenizer/parser, object index (creatures, inorganics, plants, ...) plus graphics raws (TILE_PAGE, TILE_GRAPHICS, CREATURE_GRAPHICS, ...) | rf-core |
| `rf-gfx` | loads the Premium tileset PNGs from the DF install and answers "which sprite(s) for this tile, unit, item or building". Renderer-agnostic: returns page + rect + tint | rf-raws, rf-core |
| `rf-import` | reads `export/<fort>/` (see `docs/EXPORT_FORMAT.md`) into `rf_core::World` | rf-core |
| `rf-sim` | deterministic simulation: tick loop, pathfinding, unit AI, needs, jobs, designations | rf-core |
| `rf-app` | binary `rustfort`: macroquad window, Steam-style mouse UI, renderer, plus the headless `bench` subcommand | all |

There is also `tools/dfhack/rustfort-export.lua`, a read-only DFHack exporter that runs inside the user's live DF.

## Rules

1. **No DF data in git.** Raws, PNGs and exports are read at runtime from the user's install or the export folder.
   - DF install: `E:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress`. It is configurable through the env var `RUSTFORT_DF_DIR` or `rustfort.toml` (gitignored).
   - The user's mods in `data/installed_mods` count as part of their install.
2. **rf-core is the shared contract.** Only additive changes are allowed. If you need something there, add it in a backwards-compatible way and mention it in your report.
3. **Deterministic simulation.**
   - No `HashMap` iteration where the order affects results.
   - RNG is seeded per world and tick (`rand_pcg` or a small splitmix).
   - Parallel phases may only read; writes are applied in a fixed order.
   - [saves11] All sim state is saved: `Sim` and every module struct derive `serde::Serialize`/`Deserialize` (the save holds an exact snapshot, see `docs/SAVES.md`). A new field must be serializable, or `#[serde(skip)]` if it is a pure cache rebuilt in `Sim::after_exact_load`. Give module structs `#[serde(default)]`. A `&'static str` field is written as `crate::saves11::SStr` with `#[serde(with = "crate::saves11::interned")]`. Check with `cargo test -p rf-sim --release --test saves11`.
4. **Speed is a feature.** Keep hot loops allocation-free. Benchmark with `rustfort bench`.
5. **Clean-room.** The spec is the DF wiki plus observed behavior. Never decompile DF.
6. **Commits** are authored by `Claude Code <noreply@anthropic.com>` (set in the repo's local git config). Write short imperative English messages that end with the Co-Authored-By trailer.

## Coordinates

- `Pos { x, y, z }` uses local embark tiles. z = 0 is the **bottom** of the map, as in DF.
- Tile index = `(z * dy + y) * dx + x`.
