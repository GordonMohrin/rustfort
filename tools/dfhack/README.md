# DFHack tools for Rustfort

| script | purpose |
|---|---|
| `rustfort-export.lua` | Exports the loaded fort into `export/<fort>/` (format: `docs/EXPORT_FORMAT.md`). Read-only. |
| `rustfort-verify.lua` | Spot-checks an export against the live game: sampled tiles, 3 citizens, counts, work orders, workshop job counts, farm crops, mods. Read-only. |

Both scripts are for DF 53.16 with DFHack 53.16-r2.

## Safety

The exporter is built to run while you, or your own agents, keep playing:

- It never pauses or unpauses the game and never writes to game memory.
- It enables no plugin and leaves no hook behind: the work runs in a coroutine, resumed by one-shot `dfhack.timeout(1, 'frames')` callbacks.
- Each slice is limited to `--budget` ms of Lua time (default 25). Lesson from BUG-415: a whole-map scan in one go froze DF for 9–28 s.
- It copies the block tile arrays with `dfhack.internal.memmove` *from* DF memory *into* buffers the script allocated itself with `df.new`. This makes the map pass about 10x faster than field access.
- DF's vectors change between frames. Units, items and buildings are therefore snapshotted by id and re-found with `df.*.find` in each slice.
- It refuses to run when a different fort than `--expect` (default `Tileshtulon`) is loaded, and while `export.lock` exists (another export is running; stale after 15 min, or use `--force`).

## Running the export

Run from Git Bash. `dfhack-run` starts the export and returns immediately; `manifest.json` is written last and is the done-marker.

```bash
HACK="/e/Program Files (x86)/Steam/steamapps/common/Dwarf Fortress/hack/dfhack-run.exe"
REPO="C:/Users/admin/claude gordons projects/rustfort"
"$HACK" lua -f "$REPO/tools/dfhack/rustfort-export.lua" "$REPO/export/tileshtulon"
# poll until done (progress.json is rewritten about once per second)
until [ -f "$REPO/export/tileshtulon/manifest.json" ] || [ -f "$REPO/export/tileshtulon/error.json" ]; do
  cat "$REPO/export/tileshtulon/progress.json"; echo; sleep 2
done
```

Options:

| option | effect |
|---|---|
| `--budget MS` | Lua time per frame (default 25). Lower keeps the game faster during the export; higher finishes sooner. |
| `--expect NAME` / `--any-fort` | Fort-name check. |
| `--force` | Ignore a stale `export.lock`. |
| `--skip-map` | Export only units, items, buildings and plants. Useful for quick re-tests. |
| `--zmin N --zmax N` | Export only these z-levels of the map (testing). |
| `--status` | Print `progress.json` and exit. |
| `--cancel` | Ask a running export to stop (it stops within about 1 s and writes `error.json`). |

Then load it in Rust:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo run -p rf-import --release --example inspect -- "$REPO/export/tileshtulon"   # incl. mods, work orders, workshop queues,
                                                                                  # farm plots, wells, drink/food stock, burrows, squads
RUSTFORT_EXPORT="$REPO/export/tileshtulon" cargo test -p rf-import    # includes the real-export test
```

## Measured on Trussroads (09.10.2026)

Trussroads is 192x192x150 tiles: 5.5 M tiles, 320 units (91 citizens), about 14 400 items, 1 177 buildings and 29 100 plants.

| | value |
|---|---|
| total export time | **14.1 s** wall, 11.4 s of it Lua inside DF, 364 frames |
| longest slice | **32 ms** (budget 25 ms; one block plus one row of overrun at most) |
| `dfhack-run` round trip during the export | avg 84 ms, max 149 ms (baseline about 60 ms) |
| game speed during the export | about 110 ticks/s (baseline about 170 ticks/s with fps cap 500); the game never stalled |
| output | about 44 MB map + 11 MB JSON |
| import (`rf-import`, release) | **0.02–0.04 s** |

## Round 2 (rev 2026-10-10a): orders, jobs, crops, plant raws, mods, gaits

The exporter now also writes `work_orders.jsonl`, `plant_raws.json`, `burrows.json`, `squads.json`, workshop/furnace job queues (`jobs`) and farm plot crops (`crops`) in `buildings.jsonl`, the mod list in `manifest.json`, and real gait speeds in `units.jsonl` (format: `docs/EXPORT_FORMAT.md`, "Additions for round 2"). It is still read-only and frame-sliced.

Where the data comes from in DF 53.16 / DFHack 53.16-r2:

| data | source |
|---|---|
| work orders | `df.global.world.manager_orders.all`; frequency `df.workquota_frequency_type`, conditions `item_conditions` with `df.logic_condition_type` and `df.job_item_flags1..3` |
| job queues | `bld.jobs` of Workshop/Furnace buildings, without `ConstructBuilding`/`DestroyBuilding` |
| crops | `building_farmplotst.plant_id[0..3]` (spring..winter) |
| plant raws | `df.global.world.raws.plants.all`: flags, `growdur`, `material_defs`, `growths`, `underground_depth_min/max` |
| mods | `df.global.world.object_loader.object_load_order_id` / `_numeric_version` / `_name` / `_displayed_version` |
| walk_speed | `caste.body_info.gait_info[WALK][default_gait_index[WALK]].full_speed` (FLY for fliers). `dfhack.units.computeMovementSpeed` returns 0 in 53.16-r2. |
| burrows | `df.global.plotinfo.burrows.list`, tiles from the block masks (`block.block_burrows`) |
| squads | `df.global.world.squads.all` with `entity_id == plotinfo.group_id`; members via `historical_figure.unit_id` |

Measured on Trussroads (10.10.2026, 00:10, year 274, game running):

| | value |
|---|---|
| total export time | **13.8 s** wall, 11.4 s Lua inside DF, 365 frames (the new phases add about 0.1 s) |
| longest slice | **32 ms** (budget 25 ms), unchanged from v0.1 |
| `dfhack-run` round trip during the export | avg 73 ms, max 112 ms |
| game speed during the export | about 90 ticks/s (1 245 ticks passed) |
| exported | 43 work orders, 36 queued workshop/furnace jobs in 11 of 29 workshops, 28 farm plots (21 planted), 225 plant raws, 25 mods (all vanilla), 2 burrows, 9 squads; 752 materials (+33 % for the plant materials) |
| import | 0.025 s, no warnings beyond the usual off-map units and exporter notes |
| `--skip-map` export | 2.2 s, 55 frames |

Verification (`spotcheck`, see below): a `--skip-map` export followed directly by `rustfort-verify.lua` (37 ticks apart): **orders 41/41**, **farm crops 28/28**, **mods 25/25**, **workshop job counts 28/29** (one mason finished a job in those 37 ticks), plant raws 225/225, units/citizens/buildings/plants identical, 375/375 tiles. The verify script counts jobs through a different path than the exporter (the global job list `world.jobs.list` + `dfhack.job.getHolder`). Against the full export (about 6 000 ticks later) the orders still matched 42/42 (amount_left had moved on for three), 2 of 28 farm plots had been replanted by then.

## Spot check against the live game

```bash
SP=/tmp/rf ; mkdir -p $SP
cargo run -p rf-import --release --example spotcheck -- pick    "$REPO/export/tileshtulon" $SP/samples.txt 25
"$HACK" lua -f "$REPO/tools/dfhack/rustfort-verify.lua" "$(cygpath -w $SP/samples.txt)" "$(cygpath -w $SP/result.txt)"
cargo run -p rf-import --release --example spotcheck -- compare "$REPO/export/tileshtulon" $SP/result.txt
```

The check samples 25 tiles for each tiletype material class and 3 citizens. Since round 2 the verify script also lists the live work orders, the job count of every workshop/furnace, the crops of every farm plot, the mod list and the number of plant raws, and `compare` checks them against the export (summary lines `orders:`, `workshop job counts:`, `farm crops:`, `mods:`). The game keeps running, so compare right after the export. On 09.10.2026 it gave:
- **375 of 375 tiles matched**, in all 15 classes: Stone, Soil, Mineral, Construction, Feature, Hfs, LavaStone, Magma, GrassLight, GrassDark, Tree, Root, Mushroom, Plant and Pool.
- The citizens' names and professions matched.
- Unit and citizen counts were identical. Items, buildings and plants were within ±10, because the game had been running for 13 more days.

The verify script uses plain field access, `getTileType`, `getPlantAtTile` and `df.construction.find`. It therefore checks the exporter's fast path (raw copies, bit decoding, palette, binary layout, importer). It does not check the material *rules*, because both scripts follow the same rules.

## Approximations (also written to `manifest.json` `notes`)

- `walk_speed` is the raw speed of the caste's default gait; gait attribute modifiers, carried weight, injuries and terrain are not applied. (Exports before rev 2026-10-10a always wrote 900.)
- `growdur` in `plant_raws.json` is GROWDUR × 100 ticks (wiki: GROWDUR is in hundreds of ticks).
- LAVA_STONE and MAGMA tiles (semi-molten rock) become `INORGANIC:OBSIDIAN`.
- POOL, BROOK and RIVER tiles get the geology layer material.
- FEATURE tiles take the init-feature material when available, else `RAW_ADAMANTINE` (local) or `SLADE` (global).
- For GRASS tiles, the grass event with the largest amount wins. 2–3 grass tiles per export have no event and get no material.
- Dead units (about 400 in `units.all`) and off-map items are not exported; the counts are in `notes`.
- Not exported yet:
  - jobs outside workshop/furnace queues (hauling, digging, planting), squad uniforms, orders and schedules, burrow tile lists, zones' detailed settings, stockpile item filters below category level
  - historical figures, unit wounds and body state, needs and thoughts
  - flows (miasma, smoke), the heavy aquifer flag, block hashes
- The tiletype special `SHODDY` (DF 53) has no `rf_core::TtSpecial` variant and is imported as `NONE`.
