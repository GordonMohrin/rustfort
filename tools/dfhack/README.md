# DFHack tools for Rustfort

| script | purpose |
|---|---|
| `rustfort-export.lua` | Exports the loaded fort into `export/<fort>/` (format: `docs/EXPORT_FORMAT.md`). Read-only. |
| `rustfort-verify.lua` | Spot-checks an export against the live game: sampled tiles, 3 citizens, counts, work orders, workshop job counts, farm crops, mods; since round 4 also door/bridge/lever/trap states and links, noble positions, entity relations, settings; since round 8 work details, kitchen, locations, crimes, animals, pregnancies, pile links and sampled filter leaves; since round 9 DF's active-unit list and caged-unit positions, squad ammo, wealth, invasions and stockpile option group sizes. Read-only. |
| `rustfort-probe.lua` | (round 6) Prints one JSON line about the loaded game (map loaded, fortress mode, fort and world names, year, citizens, paused, DFHack version) for Rustfort's "Import from Dwarf Fortress" dialog. Read-only, takes about 0.1 s. |

The scripts are for DF 53.16 with DFHack 53.16-r2. Rustfort's title screen runs the probe and the exporter itself ("Import from Dwarf Fortress", see `crates/rf-app/README.md`); the commands below are for running them by hand.

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
cargo run -p rf-import --release --example inspect -- "$REPO/export/tileshtulon"   # incl. mods, work orders, workshop queues, needs/thoughts, wounds, raws,
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

## Round 3 (rev 2026-10-10b): psychology, bodies, raws, military

The exporter now also writes (format: `docs/EXPORT_FORMAT.md`, "Additions for round 3"):

- `units.jsonl`: `needs`, `emotions`, `values`, `relationships` (family links, siblings, friends/acquaintances with love/trust/meet counts), `wounds` (one entry per wound layer), `blood`/`blood_max`, `squad_position`, `invader_civ`; `body_size` is now in cm³.
- `creature_raws.json`: fully expanded bodies (parts, tissue layers with palette materials, flags), attacks and caste flags for the races on the map, the corpses on the map and the world's civilizations.
- `weapon_raws.json`, `armor_raws.json`: all item definitions (weapons, ammo, trap components; armor, helms, gloves, shoes, pants, shields).
- `materials.json`: density, shear/impact yield/fracture/strain, max edge, value, heat points.
- `squads.json` (members by position, current orders, uniform, alert routine) and `burrows.json` (tile lists) as `rf_core::Squad` / `rf_core::Burrow`.
- corpse/remains items carry the creature token in `subtype`; map flag bit 14 = heavy aquifer.

Where the data comes from in DF 53.16 / DFHack 53.16-r2:

| data | source |
|---|---|
| needs, emotions, values, stress | `unit.status.current_soul.personality.needs/emotions/values`; `dfhack.units.getStressCategory`, `getStressCutoffs` |
| family | `historical_figure.histfig_links` (`df.histfig_hf_link_type`); siblings through the parents' `CHILD` links |
| friends, acquaintances | `historical_figure.info.relationships.hf_visual[i]`: `core.love/trust/respect/loyalty/fear`, `meet_count` (DF 53 has no rank: `rank` is -1 everywhere) |
| wounds | `unit.body.wounds[i]` (`flags`, `pain`, `age`, attacker) and `.parts[j]` (`body_part_id`, `layer_idx`, `flags1/2`, `bleeding`, `pain`, `surface_perc`, ...) |
| bodies | `caste.body_info.body_parts[i]` (`token`, `con_part_id`, `relsize`, `flags`, `layers[k]` with `tissue_id` → `creature.tissue`), `body_info.attacks` |
| item raws | `world.raws.itemdefs.weapons/ammo/trapcomps/armor/helms/gloves/shoes/pants/shields` |
| mechanics | `material.solid_density`, `material.strength.yield/fracture/strain_at_yield[df.strain_type]`, `max_edge`, `material_value`, `material.heat` |
| squads | `squad.positions[i].occupant` / `.equipment.uniform[category]`, `squad.orders`, `squad.schedule.routine[cur_routine_idx].month[m].orders`, `plotinfo.alerts.routines` |
| burrow tiles | `dfhack.burrows.listBlocks` + the block's `block_burrows` link → `tile_bitmask.bits[y]` bit x |
| heavy aquifer | `tile_occupancy.heavy_aquifer` = occupancy bit 26 (found by setting single bits on an own `df.tile_occupancy` object) |

DF keeps volumes in units of 10 cm³ (the dwarf's `[BODY_SIZE:12:0:60000]` is 6000 in memory, the whip's `[SIZE:100]` is 10, checked against `raw_strings`). The export writes cm³ as in the raws.

Measured on Trussroads (10.10.2026, year 275, during and after a goblin siege; Gordon's game kept running and was paused and unpaused by him in between):

| | value |
|---|---|
| total export time | **34.4 s** wall, **13.5 s Lua** inside DF, 431 frames (round 2: 11.4 s Lua). The wall time is longer because DF ran at a lower frame rate at the time (the machine was busy with 5 parallel Rust builds). |
| longest slice | **32 ms** (budget 25 ms) in every phase (`max_slice_by_phase` in manifest.json). A first run had one 47 ms slice; the materials phase now checks the budget every 8 instead of 32 materials, units after every unit. |
| `dfhack-run` round trip during the export | avg 150 ms, max 329 ms; the baseline right afterwards was avg 365 ms (machine under load), so no slowdown can be attributed to the export |
| `--skip-map` export | 5.3 s wall |
| exported | 357 units (90 citizens): 3 066 needs, 8 784 emotions, 823 values, 3 669 relationships, 261 wound layers, 34 invaders (goblins, civ 39); 67 creature raws (135 castes, dwarf 89 parts / 243 layers / 7 attacks), 44 weapon and 62 armor raws, 9 squads, 2 burrows with 1 771 tiles, 2 087 materials (2 079 with mechanics; +1 335 tissue materials) |
| output | 44 MB map + 23 MB JSON (creature_raws.json 7 MB, units.jsonl 3.6 MB) |
| import (`rf-import`, release) | 0.06-0.15 s, no count mismatches |

Verification (`spotcheck`, see below), a `--skip-map` export followed directly by `rustfort-verify.lua` (270 ticks apart): **unit psychology fields 90/90** (needs, emotions, values, family and visual relationship counts, wound layers, blood, squad/position, civ of 10 units incl. a troglodyte and 3 demons), **squads 9/9** (positions, members, routine), **burrow tiles 32/32** (16 burrow tiles plus 16 neighbours, through `isAssignedTile`), **burrows 2/2** (tile count via `isAssignedBlockTile`), **corpse tokens 8/8**, **creature bodies 5/5** (DWARF, GOBLIN, HUMAN, ELF, CAT: castes/parts/layers/attacks/adult size), **material mechanics 5/5**, item raw counts 1/1. Against the full export: tiles 375/375, **aquifer flags 20/20** (10 aquifer tiles + 10 random). About 10 800 ticks after the full export the squads had all left the `Ready` alert and wounds had healed, which `compare` shows as differences.

## Round 4 (rev 2026-10-10c): building states, links, society, settings

The exporter now also writes (format: `docs/EXPORT_FORMAT.md`, "Additions for round 4"):

- `buildings.jsonl`: `flags` (doors `LOCKED`/`OPEN`/`CLOSED`, bridges `RAISED`/`LOWERED`, levers `ACTIVE` = pulled, traps `LOADED`/`ACTIVE`/`RESETTING`/`FULL`, floodgates and grates `OPEN`, pressure plate triggers, depot `ACCESSIBLE`), `links` (mechanism links in both directions), `direction` (bridge raise direction, plate trigger ranges, track-stop dump direction), extras `trap_items` and `caged_units`.
- `entities.json` (own civ and site government, contacts with relation `Own`/`Peace`/`War`, trade partners), `nobles.json` (positions, holders, mandates), `settings.json` (population caps, invasions, temperature, weather, cave-ins, all difficulty values), `hist_figs.json` (names of every referenced historical figure), `caravans.json`, `diplomats.json`.

Where the data comes from in DF 53.16 / DFHack 53.16-r2:

| data | source |
|---|---|
| door lock | `building_doorst.door_flags.forbidden` (= the door sheet's Lock; DF 53 has no pet-passable bit, `unused_02` was "internal") |
| bridges, floodgates, grates, spikes | `gate_flags` (`raised`/`raising`/`lowering` for bridges, `closed` for floodgates/grates, `retracted` for spikes), `building_bridgest.direction` (Left/Right/Up/Down/Retracting) |
| levers, traps | `building_trapst.trap_type`, `state` (lever pulled), `ready_timeout`, `plate_info`, `track_stop_info`, `contained_items` (cage/boulder/weapons) |
| links | general refs `BUILDING_TRIGGERTARGET` / `BUILDING_TRIGGER` of the mechanisms in `contained_items`, and `linked_mechanisms` |
| relations | `historical_entity.relations.diplomacy.state[i]` (`group_id`, `relation` 0 = peace, 1 = war, `flags`) of the civ and the site government |
| trade partners | history events of type `MERCHANT` at the fort's site (31 857 events scanned, 1 000 per budget check), `plotinfo.caravans` |
| nobles | `entity.positions.own` / `.assignments` (`histfig`, `squad_id`), `world.mandates.all` |
| settings | `d_init.dwarf` (caps), `d_init.feature.flags` (`d_init_flags4`), `plotinfo.main.custom_difficulty` (`difficultyst`) |
| caravans, diplomats | `plotinfo.caravans` (`trade_state`, `time_remaining`, `goods`, `animals`), `plotinfo.dip_meeting_info` |

Measured on Trussroads (10.10.2026, year 277, game running; one of the two runs overlapped with a Rust build on the same machine):

| | value |
|---|---|
| total export time | 21.8-27.9 s wall, 14.1-14.9 s Lua inside DF, 452-476 frames |
| longest slice | 32 ms in every phase without parallel load (the round-4 phases `history`, `entities`, `nobles`, `settings`, `trade`, `hist_figs` stay at 31-32 ms); 47 ms in the map/plants phases while `cargo` was compiling |
| exported (round 4) | 130 doors, 2 bridges with 2 levers (2 links), 81 traps (79 loaded: 25 weapon traps with 59 weapons, 23 cage traps, 31 stone-fall traps; 57 on z131), 58 entities (3 civs at war), 33 noble assignments with 1 mandate, 163 historical figures, settings (population cap 90, temperature off) |
| import (`rf-import`, release) | 0.06 s, no count mismatches |

Verification right after the export (`spotcheck`, the verify script reads other paths where DF has one: the raw `door_flags.whole` / `gate_flags.whole` bits, the lever's `linked_mechanisms` → `getHolderBuilding`, `dfhack.units.getNoblePositions` per citizen): **tiles 375/375, doors 128/130** (2 doors had opened/closed by then), **bridges 2/2, levers with links 2/2, traps loaded 79/79, noble positions 16/16, entity relations 32/32**, settings 1/1, caravan and mandate counts 1/1 each, plus all round-2/3 checks (orders 43/43, farm crops 56/56, squads 9/9, burrows, creature bodies, mechanics).

## Round 8 (rev 2026-10-10e): work details, stockpile filters, kitchen, locations, justice, animals

The exporter now also writes (format: `docs/EXPORT_FORMAT.md`, "Additions for round 8"):

- `units.jsonl`: `pet_owner`, `training`, `pasture`, `slaughter`, `pregnancy`, `work_details`.
- `buildings.jsonl`: per stockpile the full DF settings as `filters` leaf strings (`category/group/token`, `*` for complete groups, `!token` exclusions), `use_links_only`, `give_to`, `take_from`; per civzone DF's room evaluation (`room_value`, `room_name`, the owner's latest room quality thought).
- `work_details.json`, `kitchen.json`, `locations.json` (with occupations), `crimes.json`, `punishments.json`, `haul_routes.json`.
- `creature_raws.json`: milk, grazing, shearing and egg facts per caste; pseudo-flag `SHEARABLE`.

Where the data comes from in DF 53.16 / DFHack 53.16-r2:

| data | source |
|---|---|
| work details | `plotinfo.labor_info.work_details`: `name`, `flags.mode` (`df.work_detail_mode`), `assigned_units`, `allowed_labors[df.unit_labor]`, `icon` |
| stockpile filters | `building_stockpilest.settings` (`stockpile_settings`: per category vectors of `char` indexed by raw/organic-table index, 7-entry quality arrays, option booleans, `misc.allow_organic/inorganic`); organic tokens via `world.raws.mat_table.organic_types/organic_indexes[df.organic_mat_category]` |
| pile links | `building_stockpilest.links` (`give_to_pile`, `take_from_pile`, `give_to_workshop`, `take_from_workshop`), `stockpile_flag.use_links_only` |
| kitchen | `plotinfo.kitchen` (`item_types`, `item_subtypes`, `mat_types`, `mat_indices`, `exc_types` with bits `Cook`, `Brew`) |
| locations | `plotinfo.main.fortress_site.buildings` (`abstract_building`: type, name, flags, `deity_type`/`deity_data`, `contents.profession/location_value/location_tier/building_ids/desired_*/count_*`, `occupations`); civzone `location_id` |
| crimes | `world.crimes.all` with `site` = the fort (`mode` = `df.crime_type`, `criminal`, `accused`, `victim`, `flags.sentenced`, `punishment.hammerstrikes/prison_time/flags`, `witnesses[i].witness_id`); running punishments `plotinfo.punishments` |
| animals | `unit.relationship_ids.PetOwner`, `unit.training_level` (`df.animal_training_level`), Pen civzones' `assigned_units`, `unit.flags2.slaughter`, `unit.pregnancy_timer` |
| animal products | `caste.extracts.milkable_mat/matidx`, `caste.misc.milkable/grazer/clutch_size_*/litter_size_*/egg_size/petvalue`, `caste.shearable_tissue_layer`, `caste.extracts.egg_material_*` |
| hauling routes | `plotinfo.hauling.routes` (stops with `conditions`, `stockpiles` links, `vehicle_ids`) |
| room evaluation | `dfhack.buildings.getRoomDescription(zone, owner)` (returns `""` for v50 zones in DFHack 53.16-r2), location values, owners' `BedroomQuality`/`DiningQuality`/`OfficeQuality`/`TombQuality` emotions |

Measured on Trussroads (10.10.2026, year 280, during a large siege: 153 hostiles on the map, 66 citizens killed this year, 15 citizens left at export time):

| | value |
|---|---|
| total export time | **14.0 s** wall, 13.1 s Lua inside DF, 421 frames (`--skip-map`: 4.5-4.9 s) |
| longest slice | **32 ms** (budget 25 ms) in every phase, including the new `pile_filters` (43 piles) and `locations` |
| exported (round 8) | 12 work details, 43 stockpiles with 481 filter strings (11 linked), 269 kitchen exclusions (all "no cook"), 23 locations with 15 occupations, 328 crimes (60 sentenced), 1 punishment running, 0 hauling routes, 25 animals marked for slaughter, 4 pregnancies, 34 castes with milk/grazing/shearing/egg data |
| import (`rf-import`, release) | 0.22-0.38 s warm (first load from a cold disk cache 1.7 s), no count mismatches |

Verification right after a `--skip-map` export (spot check against the live game, see below): **work details 12/12** (mode, members, labor count), **kitchen 1/1** (entries, no cook, no brew), **locations 23/23** (zones via the civzones' `location_id`, filled occupations via `world.occupations.all`), **crimes 1/1** (count at the site, sentenced), **stockpile links 43/43**, **sampled filter leaves 110/110** (the verify script resolves the first set, the first unset and the middle entry of 10 groups per pile itself and `compare` asks `StockpileSettings::filter_allows`), **tame animals 2/2**, **pregnant units 4/4**; against the full export also **tiles 375/375** and **aquifer flags 20/20**, plus all older checks (orders 54/54, doors 136/136, traps 79/79, squads 9/9, ...).

## Round 9 (rev 2026-10-10f): off-map animals, squad ammo, wealth and invasions, stockpile options

The exporter now also writes (format: `docs/EXPORT_FORMAT.md`, "Additions for round 9"):

- `units.jsonl`: `flags.inactive` for every unit DF does not simulate (not in `world.units.active`); caged units inside a cage item take the cage's position; extras `chained`, `chain_building`, `cage_item`, `cage_building`, `pos_source`, `in_active_list`.
- `squads.json`: `ammo`, `position_uniforms`, `patrol` (route waypoints), extras `carry_food`, `carry_water`, `ammo_items`.
- `settings.json`: `WEALTH_CREATED/EXPORTED/IMPORTED/...`, `PROGRESS_POPULATION/TRADE/PRODUCTION`, `FORTRESS_RANK`, fort statistics and `INVASION_HISTORY` in `other`, plus the extras object `wealth`.
- `invasions.json` (DF's invasion list) and `stockpile_options.json` (every stockpile leaf DF offers, about 30 000).

**The off-map tame animals** (80 of 87 in r5): read-only probes of the live game showed that they are the pack animals and unsold livestock of **departed merchant caravans**. DF keeps them in `world.units.all` with `flags1.forest`, the fort's civ (dwarven caravans), position -30000 and `flags1.inactive` unset, but not in `world.units.active`, so DF does not simulate them. None of them is in a built cage, a cage item, a chain or a pasture. The exporter therefore marks them inactive instead of inventing a position; the importer does the same for older exports. The one real exception was a water buffalo inside a caravan's cage item at the depot (unit position -30000), which now gets the cage's position.

Where the data comes from in DF 53.16 / DFHack 53.16-r2:

| data | source |
|---|---|
| simulated units | `df.global.world.units.active` (ids), `flags1.forest` = "part of a merchant caravan" (DFHack `devel/export-dt-ini.lua`) |
| cages, chains | `items.other.CAGE` with `CONTAINS_UNIT` general refs, `dfhack.items.getHolderBuilding`; `buildings.other.CHAIN` `.chained` |
| squad ammo | `squad.ammo.ammunition` (`item_type`, `item_subtype`, `mattype`/`matindex`, `material_class` = `df.entity_material_category`, `amount`, `flags.use_combat/use_training`), `squad.ammo.ammo_items`, `squad.supplies` |
| per-position uniforms | `squad.positions[i].equipment.uniform[category]` |
| patrol routes | squad orders of type `PATROL_ROUTE` (`route_id`) -> `plotinfo.waypoints.routes` (`points` = point ids) -> `plotinfo.waypoints.points[].pos` |
| wealth | `plotinfo.tasks.wealth` (`total` = created wealth, `exported`, `imported`, `offered`, parts) |
| invasion tiers, fort stats | `plotinfo.progress_population/progress_trade/progress_production`, `fortress_rank`, `fortress_age`, `game_over`, `lost_to_siege_civ`; `plotinfo.tasks.total_deaths`, `invaders_repelled`, `excavated_tiles`, `num_artifacts` |
| invasions | `plotinfo.invasions.list` (`civ_id`, `size`, `created_year`, `created_season_count`, `flags.siege/active`, `active_size1/2`) |
| stockpile options | the raws and `world.raws.mat_table.organic_types` per group; inclusion rules as DFHack's stockpiles plugin (estimate) |

Measured on Trussroads (10.10.2026, year 280 tick 152 130). **The fort has fallen:** `plotinfo.game_over` is true, `lost_to_siege_civ` = 39 (the goblins), 0 citizens; the game clock stood still during all exports. The export `export/tileshtulon-r7` documents that end state.

| | value |
|---|---|
| total export time | **16.8 s** wall, 510 frames (`--skip-map` test export: 12.0 s) |
| longest slice | 31-47 ms in all phases but one 187 ms slice in `creature_raws` (an unchanged phase; 4 other agents were compiling Rust on the same 6-core machine, and the skip-map run had 47 ms there) |
| exported (round 9) | 311 units (0 citizens), 88 inactive (81 caravan animals off the map, 7 units at the map edge that had left), 60 caged units (1 placed at its cage item), 0 chained; 9 squads (Schuetzen: 200 iron bolts for combat, 120 wooden bolts for training), no patrol routes in the fort; created wealth 1 576 145, exported 216 264, imported 253 175, tiers population 3 / trade 5 / production 5; 5 invasions (34, 58, 143, 101, 148 goblins); 29 735 stockpile option leaves (1.4 MB) |
| import (`rf-import`, release) | no count mismatches |

Verification right after the full export (spot check against the live game): **tiles 375/375**, **aquifer flags 20/20**, and the new checks **units simulated by DF (inactive flag and position) 311/311** (1 unit not exported: dead or gone), **squad ammo 9/9**, **wealth 1/1**, **invasions 1/1**, **stockpile option group sizes 6/6** (meat, leather, corpses, coins, wood, weapon types, counted by the verify script from the raws on its own); plus all older checks (orders 65/65, doors 136/136, traps 79/79, squads 9/9, work details 12/12, locations 23/23, sampled filter leaves 110/110, ...).

## Spot check against the live game

```bash
SP=/tmp/rf ; mkdir -p $SP
cargo run -p rf-import --release --example spotcheck -- pick    "$REPO/export/tileshtulon" $SP/samples.txt 25
"$HACK" lua -f "$REPO/tools/dfhack/rustfort-verify.lua" "$(cygpath -w $SP/samples.txt)" "$(cygpath -w $SP/result.txt)"
cargo run -p rf-import --release --example spotcheck -- compare "$REPO/export/tileshtulon" $SP/result.txt
```

The check samples 25 tiles for each tiletype material class and 3 citizens. Since round 3 it also samples the psychology of 12 units (`psy`), 20 aquifer flags (`aq`), burrow tiles and their neighbours (`btile`) and 8 corpses (`corpse`), and the verify script lists squads, burrows, item raw counts, 5 creature bodies (`craw`) and 5 material mechanics (`mech`). Since round 2 the verify script also lists the live work orders, the job count of every workshop/furnace, the crops of every farm plot, the mod list and the number of plant raws, and `compare` checks them against the export (summary lines `orders:`, `workshop job counts:`, `farm crops:`, `mods:`). The game keeps running, so compare right after the export. On 09.10.2026 it gave:
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
- Round 4: `leaves_in` = `time_remaining` × 10 ticks (DFHack's `caravan` script: 120 per day); `trades` = the entity sent a caravan to this site or has one now (DF has no "trade partner" flag); `invasions` from the difficulty setting `Enemies` (Off/Normal/Hard/Custom + `sieges` flag); `LOCKED` = `door_flags.forbidden`.
- Round 3 estimates: relationship kinds from `love`/`meet_count` (wiki thresholds; `Grudge` = love < 0 is a guess, Trussroads has none), wound `severity` = `surface_perc` × 100. Emotions with emotion `ANYTHING` and thought `None` (about 40 %) are exported as DF stores them.
- Not exported yet:
  - jobs outside workshop/furnace queues (hauling, digging, planting), zones' detailed settings (pen/pond/archery/tomb options), squad food/water supplies beyond the carry settings (stockpile item filters are exported since round 8; squad ammunition and patrol routes since round 9)
  - DF's value of personal rooms (DF 53.16 keeps none; see "Room evaluation" in docs/EXPORT_FORMAT.md)
  - historical figures beyond the relationship list, syndromes, preferences (likes/dislikes), dreams, mannerisms
  - flows (miasma, smoke), block hashes
- The tiletype special `SHODDY` (DF 53) has no `rf_core::TtSpecial` variant and is imported as `NONE`.
