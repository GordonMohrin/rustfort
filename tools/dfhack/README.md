# DFHack tools for Rustfort

| script | purpose |
|---|---|
| `rustfort-export.lua` | Exports the loaded fort into `export/<fort>/` (format: `docs/EXPORT_FORMAT.md`). Read-only. |
| `rustfort-verify.lua` | Spot-checks an export against the live game: sampled tiles, 3 citizens, counts. Read-only. |

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
cargo run -p rf-import --release --example inspect -- "$REPO/export/tileshtulon"
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

## Spot check against the live game

```bash
SP=/tmp/rf ; mkdir -p $SP
cargo run -p rf-import --release --example spotcheck -- pick    "$REPO/export/tileshtulon" $SP/samples.txt 25
"$HACK" lua -f "$REPO/tools/dfhack/rustfort-verify.lua" "$(cygpath -w $SP/samples.txt)" "$(cygpath -w $SP/result.txt)"
cargo run -p rf-import --release --example spotcheck -- compare "$REPO/export/tileshtulon" $SP/result.txt
```

The check samples 25 tiles for each tiletype material class and 3 citizens. On 09.10.2026 it gave:
- **375 of 375 tiles matched**, in all 15 classes: Stone, Soil, Mineral, Construction, Feature, Hfs, LavaStone, Magma, GrassLight, GrassDark, Tree, Root, Mushroom, Plant and Pool.
- The citizens' names and professions matched.
- Unit and citizen counts were identical. Items, buildings and plants were within ±10, because the game had been running for 13 more days.

The verify script uses plain field access, `getTileType`, `getPlantAtTile` and `df.construction.find`. It therefore checks the exporter's fast path (raw copies, bit decoding, palette, binary layout, importer). It does not check the material *rules*, because both scripts follow the same rules.

## Approximations (also written to `manifest.json` `notes`)

- `walk_speed` is always 900, because DFHack 53.16 exposes no gait raws on `caste_raw`.
- LAVA_STONE and MAGMA tiles (semi-molten rock) become `INORGANIC:OBSIDIAN`.
- POOL, BROOK and RIVER tiles get the geology layer material.
- FEATURE tiles take the init-feature material when available, else `RAW_ADAMANTINE` (local) or `SLADE` (global).
- For GRASS tiles, the grass event with the largest amount wins. 2–3 grass tiles per export have no event and get no material.
- Dead units (about 400 in `units.all`) and off-map items are not exported; the counts are in `notes`.
- Not exported yet:
  - jobs, squads and uniforms, burrows, zones' detailed settings, stockpile item filters below category level
  - historical figures, unit wounds and body state, needs and thoughts
  - flows (miasma, smoke), the heavy aquifer flag, block hashes
- The tiletype special `SHODDY` (DF 53) has no `rf_core::TtSpecial` variant and is imported as `NONE`.
