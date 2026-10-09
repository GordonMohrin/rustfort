# Fort export format (v1)

This is the contract between the DFHack exporter (`tools/dfhack/rustfort-export.lua`) and the Rust importer (`crates/rf-import`).

Rules:
- The format mirrors the types in `crates/rf-core`. JSON field names are the serde names of those structs. The importer can deserialize them directly, and fields it does not know are ignored.
- Missing optional fields fall back to their defaults.
- You may add fields. Document any you add here. Do not rename or remove fields.
- All text is UTF-8. The exporter converts DF's CP437 strings with `dfhack.df2utf`. Tokens can contain spaces (e.g. `PLANT:CAVE MOSS:STRUCTURAL`).
- Lists are always JSON arrays, even when empty (`[]`). `stockpile` is `null` or an object.

Exports contain data from a real save and **never go into git**. The default location is `rustfort/export/<fort>/`, which is gitignored.

## Directory layout

```
export/<fort>/
  manifest.json      metadata, counts, timing, notes; written LAST = done-marker
  tiletypes.json     DF tiletype enum (all entries)
  materials.json     material palette (only materials used by tiles/items/buildings)
  map/z000.bin ...   one binary file per z-level, z000 = bottom of the map
  units.jsonl        one JSON object per line (rf_core::Unit)
  items.jsonl        rf_core::Item
  buildings.jsonl    rf_core::Building
  plants.jsonl       rf_core::Plant
  creatures.json     races present in units.jsonl
  work_orders.jsonl  rf_core::WorkOrder (round 2, optional)
  plant_raws.json    rf_core::PlantRaw array (round 2, optional)
  burrows.json       rf_core::Burrow array with tile lists (round 3; round 2 wrote tile counts only)
  squads.json        rf_core::Squad array (round 3; round 2 wrote names and members only)
  creature_raws.json rf_core::CreatureRaw array: expanded bodies, attacks (round 3, optional)
  weapon_raws.json   rf_core::WeaponRaw array: weapons, ammo, trap components (round 3, optional)
  armor_raws.json    rf_core::ArmorRaw array: armor, helms, gloves, shoes, pants, shields (round 3, optional)
  progress.json      exporter status, rewritten about once per second (not imported)
  error.json         only if the export aborted (not imported)
  export.lock        only while an export is running
```

## manifest.json

```json
{
  "format_version": 1, "exporter_rev": "2026-10-09c",
  "df_version": "53.16", "dfhack_version": "53.16-r2",
  "world_name": "Zilirróth", "world_name_english": "The Eternal Domain",
  "fort_name": "Trussroads", "fort_name_native": "Tileshtulon",
  "site_id": 90, "civ_id": 29, "group_id": 303,
  "region_x": 428, "region_y": 164, "region_z": 0,
  "dims": {"x": 192, "y": 192, "z": 150},
  "year": 273, "year_tick_start": 337929, "year_end": 273, "year_tick_end": 339499,
  "counts": {"units": 320, "citizens": 91, "items": 14355, "buildings": 1177, "plants": 29130,
             "materials": 379, "tiletypes": 697, "tiles": 5529600, "z_exported": 150},
  "export_seconds": 14.1, "lua_seconds": 11.4, "frames": 364, "max_slice_ms": 32, "budget_ms": 25,
  "map_complete": true,
  "mods": ["vanilla_text:5001", "...", "vanilla_procedural:5316"],
  "mods_detail": [{"id": "vanilla_text", "name": "Vanilla Text", "version": "50.01", "numeric_version": 5001}, "..."],
  "notes": ["units: skipped 409 dead ...", "..."]
}
```

(`mods`, `mods_detail` and the counts `work_orders`, `plant_raws`, `building_jobs`, `farm_plots`, `burrows`, `squads` exist since rev `2026-10-10a`; see "Additions for round 2".)

- `dims` are in tiles: `x = world.map.x_count`, `y = y_count`, `z = z_count`.
- The calendar comes from `df.global.cur_year` and `df.global.cur_year_tick`.
- The exporter does **not** pause the game. Ticks pass during the export, so it records the start (`year`, `year_tick_start`) and the end (`year_end`, `year_tick_end`). The map is exported first and the entities last, so **the importer sets `World.calendar` to the end tick**.
- `region_x/y/z` are `world.map.region_x/y/z`.
- `counts.citizens` counts units with `flags.citizen` and not dead. `tiles` is the number of tiles written, `z_exported` the number of z-files.
- `map_complete` is false when the export was made with `--skip-map`, `--zmin` or `--zmax`.
- `notes` lists everything the exporter approximated or skipped. The importer copies each note into `ImportReport::warnings` with the prefix `exporter:`.
- `lua_seconds` is the Lua time spent inside DF, `frames` the number of game frames used, `max_slice_ms` the longest single slice.

## tiletypes.json

An array with one entry for every DF tiletype id (dense, 0..N-1):

```json
{"id": 219, "name": "StoneWall", "shape": "WALL", "material": "STONE", "special": "NORMAL", "variant": 0, "direction": ""}
```

- `shape`, `material` and `special` are **DF enum names**: `df.tiletype_shape[attrs.shape]` and so on, e.g. `WORN_1` or `SHODDY`.
  - The importer maps them with `Shape::from_df`, `TtMaterial::from_df` and `TtSpecial::from_df`. It also accepts the serde names of the rf-core enums (e.g. `WORN1`, as written by `rf_import::write_export`).
  - A name rf-core does not know becomes `NONE`, with one warning. DF 53.16 has one such name: the special `SHODDY`.
- `variant` is `0` for NONE and `1..4` for VAR_1..VAR_4.
- `direction` is `attrs.direction`, or `""` if it has no letters (DF writes `--------`).

## materials.json

An array of `rf_core::MaterialInfo`. The array index is the palette index, so it fits in a `u16`. `0xFFFF` means none.

```json
{"mat_type": 0, "mat_index": 12, "token": "INORGANIC:GRANITE", "name": "granite",
 "rgb": [128,128,128], "is_stone": true, "is_gem": false, "is_metal": false, "is_ore": false,
 "is_soil": false, "is_wood": false, "is_glass": false, "tags": ["IGNEOUS_INTRUSIVE"]}
```

- `token` is `dfhack.matinfo.decode(type, index):getToken()`.
- `name` is the solid state name (`state_name.Solid`). For water that is `ice`.
- `rgb` is the solid state color, taken from the descriptor color and converted to 0..255.
- Each `is_*` flag comes from a DF flag:
  - `is_stone`, `is_gem`, `is_metal` and `is_glass` come from the material flags `IS_STONE`, `IS_GEM`, `IS_METAL` and `IS_GLASS`.
  - `is_wood` comes from the material flag `WOOD`, or is set when the token ends in `:WOOD`.
  - `is_ore` comes from the inorganic flag `METAL_ORE`.
  - `is_soil` comes from the inorganic flag `SOIL` or `SOIL_ANY`.
- `tags` holds:
  - `RC:<class>` for each reaction class, e.g. `RC:FLUX`.
  - `IS_SAND` for the inorganic flag `SOIL_SAND`.
  - `IS_CLAY` for soils whose token contains `CLAY`.
  - Any of these inorganic flags that are set: `DEEP_SPECIAL`, `AQUIFER`, `SEDIMENTARY`, `IGNEOUS_INTRUSIVE`, `IGNEOUS_EXTRUSIVE`, `METAMORPHIC`, `SOIL_OCEAN`, `SEDIMENTARY_OCEAN_SHALLOW`, `SEDIMENTARY_OCEAN_DEEP`, `DIVINE`, `WAFERS`, `GENERATED`.
- Mechanics (since rev `2026-10-10b`, see "Additions for round 3"): `density`, `shear_yield`, `shear_fracture`, `shear_strain_at_yield`, `impact_yield`, `impact_fracture`, `impact_strain_at_yield`, `max_edge`, `value`.

## map/zNNN.bin

- Each file holds `dims.x * dims.y` records. Rows are in y order, and x runs fastest within each row. The file size must be exactly `dims.x * dims.y * 8`.
- The importer replaces a missing or wrongly sized file with hidden void tiles (tiletype 0, no material, `HIDDEN`) and reports a warning.
- Map blocks that do not exist in DF are written the same way.
- A record is **8 bytes, little endian**, exactly the layout of `rf_core::Tile`:

| bytes | field | meaning |
|---|---|---|
| 0–1 | `tt` u16 | DF tiletype id |
| 2–3 | `mat` u16 | palette index into materials.json, `0xFFFF` = none |
| 4–5 | `flags` u16 | `rf_core::tile_flags`, see below |
| 6 | `liquid` u8 | bits 0–2 = depth (`flow_size` 0..7), bit 3 = magma (`liquid_type`) |
| 7 | `desig` u8 | `rf_core::Dig` = DF `designation.dig`: 0 none, 1 dig/chop/gather, 2 up-down stair, 3 channel, 4 ramp, 5 down stair, 6 up stair |

The flag bits come from the DF tile bitfields (`tile_designation` d, `tile_occupancy` o; bit numbers as in DF 53.16):

| `tile_flags` | bit | DF source |
|---|---|---|
| HIDDEN | 0 | d.hidden (bit 9) |
| OUTSIDE | 1 | d.outside (16) |
| SUBTERRANEAN | 2 | d.subterranean (15) |
| LIGHT | 3 | d.light (14) |
| AQUIFER | 4 | d.water_table (22) |
| PILE | 5 | d.pile (3) |
| TRAFFIC | 6–7 | d.traffic (24–25) |
| DESIG_SMOOTH | 8 | d.smooth == 1 (bits 7–8) |
| DESIG_ENGRAVE | 9 | d.smooth == 2 |
| MARKER | 10 | o.dig_marked (24) |
| OCC_BUILDING | 11 | o.building != 0 (0–2) |
| STAGNANT | 12 | d.water_stagnant (30) |
| SALT | 13 | d.water_salt (31) |
| *(heavy aquifer)* | 14 | o.heavy_aquifer (26), since rev `2026-10-10b` |

Bit 14 has no `rf_core::tile_flags` constant yet; `rf_import::TILE_FLAG_HEAVY_AQUIFER` names it. It is only set together with AQUIFER (bit 4). DF: a heavy aquifer refills dug tiles faster than a light one (wiki "Aquifer"). Bit 15 (`DIRTY`) is internal to Rustfort and never exported. Trussroads has 246 k aquifer tiles, all light (0 heavy).

How the material is resolved per tile, depending on the tiletype material class:

| Class | Material source |
|---|---|
| STONE / SOIL | geology layer: `getTileBiomeRgn` → `getRegionBiome().geo_index` → `geo_biomes[..].layers[d.geolayer_index].mat_index` |
| POOL / BROOK / RIVER | the geology layer, as for STONE |
| MINERAL | the vein from the block's `block_square_event_mineralst` (tile bitmask); the layer if there is none |
| FEATURE | `getLocalInitFeature` / `getGlobalInitFeature` material; falls back to `RAW_ADAMANTINE` (local) and `SLADE` (global) |
| LAVA_STONE / MAGMA | `INORGANIC:OBSIDIAN` (approximation; semi-molten rock and the region's lava stone) |
| CONSTRUCTION | `df.construction.find(pos)` |
| FROZEN_LIQUID | `WATER` (solid name: ice) |
| GRASS_* | `PLANT:<id>:STRUCTURAL` of the block's grass event with the largest `amount` on the tile |
| TREE / ROOT / MUSHROOM | `PLANT:<id>:WOOD` of `dfhack.maps.getPlantAtTile` (exact); falls back to the nearest tree trunk |
| PLANT (shrubs, saplings) | `PLANT:<id>:STRUCTURAL` of the plant at the tile |
| everything else (AIR, HFS, ASHES, FIRE, CAMPFIRE, DRIFTWOOD, UNDERWORLD_GATE, …) | `0xFFFF` |

Hidden tiles are exported completely and carry the `HIDDEN` flag. Renderers must not reveal them.

## units.jsonl / items.jsonl / buildings.jsonl / plants.jsonl

There is one JSON object per line, matching `rf_core::Unit`, `Item`, `Building` and `Plant`.

The importer skips bad lines and reports a warning for each; it never panics. Each object is re-found by id (`df.unit.find`, `df.item.find`, `df.building.find`) because DF's vectors change between frames.

**Units.** A unit from `world.units.all` is exported if it is not dead and is active, a citizen or a resident. Off-map citizens keep their DF position (`-30000`). The importer keeps them and reports how many there are. Enum-typed values use DF enum names:
- profession: `df.profession`
- skills: `df.job_skill`
- labors: `df.unit_labor`
- mood: `df.mood_type`

Field notes:
- `name` is the English `translateName(getVisibleName(u), true)`, or `getReadableName` for unnamed creatures. `name_native` is the dwarvish name.
- The needs come from `unit.counters2`: `hunger_timer`, `thirst_timer`, `sleepiness_timer`.
- `stress` is `status.current_soul.personality.stress`.
- `phys_attrs` holds `[["STRENGTH", 1250], …]` from `body.physical_attrs[i].value`. `ment_attrs` and `facets` come from `current_soul` (`mental_attrs`, `personality.traits`).
- `labors` lists every enabled labor from `status.labors[i]`.
- `inventory` lists item ids.
- `body_size` is `body.size_info.size_cur` in **cm³** since rev `2026-10-10b` (DF keeps volumes in units of 10 cm³, so the exporter multiplies by 10; extra `body_size_raw` = DF's value). Older exports wrote DF's raw value (a dwarf: about 6 000-8 000 instead of 60 000-80 000).
- `walk_speed` comes from the caste's default gait in the creature raws since exporter rev `2026-10-10a` (see "units.jsonl: walk_speed" below); older exports always wrote 900.
- Round 3 adds `needs`, `emotions`, `values`, `relationships`, `wounds`, `blood`, `blood_max`, `squad_position`, `invader_civ`: see "Additions for round 3".
- Flags:
  - `citizen`, `resident`, `animal`, `tame`, `visitor`, `merchant` and `forest` come from `dfhack.units.is*`.
  - `hostile` = invader or undead, or dangerous and not tame.
  - `caged` = `flags1.caged`.
  - `inactive` = not `isActive`.
- Extra fields, ignored by serde: `profession_name` (as DF shows it) and `readable_name`.

**Items** are taken from `world.items.all`. Items that are `removed` or `garbage_collect` are skipped, and so are items without a map position (`dfhack.items.getPosition` returns nil), for example items held off-site.
- `kind` is `df.item_type[item:getType()]`.
- `subtype` is `item.subtype.id`, if the item has a subtype. For `CORPSE`, `CORPSEPIECE` and `REMAINS` it is the creature token of `item.race` (e.g. `DWARF`, `GOBLIN`, `POND_TURTLE`; since rev `2026-10-10b`), with the caste token in the extra `caste`.
- `mat` is a palette index.
- `pos` comes from `dfhack.items.getPosition`. For contained or carried items it is the position of the holder.
- `quality` is `getQuality()`, or 6 for artifacts.
- `name` is the English artifact name (`IS_ARTIFACT` ref → `artifact_record.name`).
- `desc` is `dfhack.items.getReadableDescription`.
- `value` is `dfhack.items.getValue`.
- `container`, `holder` and `building` come from `dfhack.items.getContainer`, `getHolderUnit` and `getHolderBuilding`. `flags.in_container` is true when there is a container.

**Buildings** are taken from `world.buildings.all`, including civzones and stockpiles.
- `kind` is `df.building_type[bld:getType()]`.
- `subtype` is the workshop, furnace, trap, siege engine or construction type name, or `df.civzone_type[bld.type]` for zones (e.g. `Bedroom`, `Tomb`, `MeetingHall`). Custom workshops use the building_def `code`.
- `stage` is `getBuildStage()`; `built` is `stage >= getMaxBuildStage()`.
- `owner` is `bld.owner.id`, or the civzone's `assigned_unit_id`.
- `extents` is a row-major `0/1` array, but only when `bld.room` matches the building rectangle and is not full. Otherwise it is empty, which means the full rectangle. The importer drops an `extents` whose length does not match the area.
- `stockpile.categories` lists the enabled top-level `settings.flags` categories. `max_barrels`, `max_bins` and `max_wheelbarrows` come from `bld.storage`.
- `items` lists the ids from `bld.contained_items`.
- `jobs` (workshops/furnaces) and `crops` (farm plots): see "Additions for round 2".

**Plants** are taken from `world.plants.all`:
- `token` is `world.raws.plants.all[plant.material].id`;
- `tree` is true when the plant has `tree_info`, or when its raw has the `TREE` flag and it is not a sapling;
- `shrub` is true when the raw has no `TREE` flag;
- `sapling` is true for a TREE raw without `tree_info` whose tile shape is SAPLING;
- `dead` is `damage_flags.dead`.

## creatures.json

An array with one entry per race in units.jsonl:

```json
{"index": 572, "token": "DWARF", "name": "dwarf", "name_plural": "dwarves", "castes": [{"token": "FEMALE", "name": "dwarf"}]}
```


## Additions for round 2 (optional files, still format v1)

Written by exporter rev `2026-10-10a` and later. All of it is optional for the importer: an older export simply has no orders, jobs, crops, plant raws or mods (`rf-import` tests this).

| file | content |
|---|---|
| `work_orders.jsonl` | `rf_core::WorkOrder` per line, from `df.global.world.manager_orders.all`, with conditions |
| `plant_raws.json` | array of `rf_core::PlantRaw`, one per plant of `df.global.world.raws.plants.all` |
| `buildings.jsonl` | new fields `jobs` (`rf_core::BuildingJob` list) and `crops` (4 plant tokens for farm plots) |
| `manifest.json` | new fields `mods` (`"id:numeric_version"` in load order) and `mods_detail`; new `counts` keys |
| `burrows.json` | burrows: name, tile count, units (not in rf-core; `rf_import::load_extras`) |
| `squads.json` | the fort's squads: name and member unit ids (not in rf-core; `rf_import::load_extras`) |
| `units.jsonl` | `walk_speed` now comes from the creature raws (gaits) instead of always 900 |

Lines carry a few **extra fields** that rf-core does not have (listed below as "extras"). serde ignores them; they are there for verification and for later rounds.

### Jobs, orders and conditions: shared rules

`rf_core::BuildingJob` and `rf_core::WorkOrder` share these fields, filled the same way from a DF `job` or `manager_order`:

| field | source |
|---|---|
| `job` | `df.job_type[job_type]`, e.g. `CustomReaction`, `PrepareMeal`, `MakeBarrel`, `ConstructTable`. DF 53 has no `BrewDrink` job: brewing is `CustomReaction` with a reaction. |
| `reaction` | `reaction_name`, e.g. `BREW_DRINK_FROM_PLANT`, `BREW_DRINK_FROM_PLANT_GROWTH`; else `""` |
| `item_type` | the product item type: DF's `item_type` if it is set, **else the job type's product** `df.job_type.attrs[job].item` (DF leaves `item_type` at NONE for most jobs, e.g. `MakeGoblet` → `GOBLET`, `ConstructThrone` → `CHAIR`, `MakeCharcoal` → `BAR`). `""` if neither (e.g. `CustomReaction`, `MakeCrafts`). |
| `item_subtype` | `dfhack.items.getSubtypeDef(item_type, item_subtype).id`, e.g. `ITEM_WEAPON_PICK`, `ITEM_TOOL_LARGE_POT`; `""` if none |
| `mat` | `dfhack.matinfo.decode(mat_type, mat_index):getToken()`: `""` = any material, `INORGANIC` = any inorganic (stone/metal, `mat_type 0, index -1`), `INORGANIC:IRON`, `COAL`, `GLASS_GREEN`, `PLANT:X:DRINK`, ... |
| `mat_category` | names of the set bits of `material_category` (`df.job_material_category`, lowercase as in DF): `plant`, `wood`, `cloth`, `silk`, `leather`, `bone`, `shell`, `wood2`, `soap`, `tooth`, `horn`, `pearl`, `yarn`, `strand` |

### work_orders.jsonl

One `rf_core::WorkOrder` per line, in the order of the manager screen.

- `id`: the order id (`manager_order.id`), referenced by `BuildingJob::order_id`.
- `amount_left` / `amount_total`: as DF shows them ("1/2").
- `frequency`: `df.workquota_frequency_type`: `OneTime`, `Daily`, `Monthly`, `Seasonally`, `Yearly`.
- `workshop_id`: the workshop the order is restricted to, `-1` = any.
- `active` / `validated`: `status.active` / `status.validated`. DF re-checks the conditions of a validated order at its frequency; `active` means jobs are currently being dispatched.
- `conditions`: the item conditions (`item_conditions`), all must hold:
  - `compare`: `df.logic_condition_type`: `AtLeast`, `AtMost`, `GreaterThan`, `LessThan`, `Exactly`, `Not`.
  - `value`: `compare_val`. `item_type`, `item_subtype`, `mat`: as above (`""` = any).
  - `flags`: the set bits of `flags1`, `flags2`, `flags3` (`df.job_item_flags1..3`) **uppercased**, e.g. `UNROTTEN`, `EMPTY`, `NON_ECONOMIC`, `HARD`, `UNENGRAVED`, `SAND`, `BONE`, `BODY_PART`, `COOKABLE`, `MILLABLE`, `PROCESSABLE_TO_BARREL`. Plus these, when set:
    `REACTION_CLASS:<class>`, `HAS_MATERIAL_REACTION_PRODUCT:<product>` (DF's way to say "brewable": `HAS_MATERIAL_REACTION_PRODUCT:DRINK_MAT`), `METAL_ORE:<inorganic>`, `HAS_TOOL_USE:<df.tool_uses>`, `MIN_DIMENSION:<n>`, `REACTION_ID:<n>`, and `FLAGS4:0x..` / `FLAGS5:0x..` for the raw integer flag words if non-zero.
- Extras: `order_conditions` (`[{"order_id": n, "condition": "Activated"|"Completed"}]`, from `order_conditions`), `max_workshops`, `finished_year`, `finished_year_tick`.

```json
{"job":"ConstructSlab","reaction":"","item_type":"SLAB","item_subtype":"","mat":"INORGANIC","mat_category":[],"id":61,
 "amount_left":1,"amount_total":1,"frequency":"Daily","workshop_id":-1,"active":true,"validated":true,
 "conditions":[{"compare":"AtLeast","value":22,"item_type":"BOULDER","item_subtype":"","mat":"INORGANIC","flags":["NON_ECONOMIC","HARD"]},
               {"compare":"LessThan","value":2,"item_type":"SLAB","item_subtype":"","mat":"","flags":["UNENGRAVED"]}],
 "order_conditions":[],"max_workshops":0,"finished_year":-1,"finished_year_tick":-1}
```

`manager_orders.all` has no `find()` and is small (tens of orders), so the exporter writes it within one frame slice.

### buildings.jsonl: jobs and crops

- `jobs`: only for `Workshop` and `Furnace` buildings: the queue `bld.jobs` in order, **without** the building's own `ConstructBuilding` / `DestroyBuilding` job. (Other buildings' `bld.jobs` hold jobs DF generates itself, e.g. `StoreItemInStockpile` on stockpiles or `PlantSeeds` on farm plots; they are not exported.) Every other building has `"jobs":[]`.
  - `repeat` = `flags.repeat`, `suspended` = `flags.suspend`, `order_id` = `job.order_id` (`-1` = queued by hand).
  - Extras: `id` (job id), `worker` (unit id from `dfhack.job.getWorker`, `-1` = nobody), `working` (`flags.working`), `do_now` (`flags.do_now`).
- `crops`: only for `FarmPlot`: `building_farmplotst.plant_id[0..3]` as plant tokens (`raws.plants.all[id].id`) for **spring, summer, autumn, winter**; `""` = fallow. Every other building has `"crops":[]`. The importer pads or truncates a farm plot's list to 4 with a warning.
- Wells need no new field: `kind` is `"Well"`.

```json
{"id":10978,"kind":"Workshop","subtype":"Still", ..., "jobs":[{"job":"CustomReaction","reaction":"BREW_DRINK_FROM_PLANT","item_type":"","item_subtype":"","mat":"","mat_category":[],"repeat":false,"suspended":false,"order_id":-1,"id":761553,"worker":-1,"working":false,"do_now":false}],"crops":[]}
{"id":10609,"kind":"FarmPlot", ..., "jobs":[],"crops":["MUSHROOM_HELMET_PLUMP","GRASS_WHEAT_CAVE","GRASS_WHEAT_CAVE","MUSHROOM_HELMET_PLUMP"]}
```

### plant_raws.json

An array of `rf_core::PlantRaw`, one per entry of `df.global.world.raws.plants.all` (the save's raws, 225 plants in Tileshtulon), in raw order.

| field | source |
|---|---|
| `token`, `name`, `name_plural` | `plant.id`, `plant.name`, `plant.name_plural` |
| `growdur` | **ticks**: `[GROWDUR]` × 100. The wiki (Plant token) gives GROWDUR in hundreds of ticks, 1008 units per season (= 100 800 ticks). Plump helmet: GROWDUR 300 → 30 000 ticks (25 days). Extra `growdur_raw` keeps the raw value. |
| `spring` .. `winter` | plant flags `SPRING`, `SUMMER`, `AUTUMN`, `WINTER` (seasons it can be planted in; trees have none) |
| `underground` | any `BIOME_SUBTERRANEAN_*` flag (from `[BIOME:SUBTERRANEAN_WATER]` etc.): can grow in subterranean farm plots |
| `aboveground` | any other `BIOME_*` flag |
| `is_tree` | flag `TREE`; `is_shrub` = not `TREE` and not `GRASS` |
| `brewable` | flag `DRINK`, or a `material_defs` drink, or a structural/growth material with reaction product `DRINK_MAT`. Extra `brew_from`: `plant` (structural material: `BREW_DRINK_FROM_PLANT`), `growth` (fruit: `BREW_DRINK_FROM_PLANT_GROWTH`) or `""` |
| `edible_raw`, `edible_cooked` | material flags `EDIBLE_RAW` / `EDIBLE_COOKED` of the STRUCTURAL material **or of any growth** (extra `growths` lists them separately) |
| `millable` | has a `MILL` material def (`material_defs` mill; extra `mill` = its token, e.g. `PLANT:BARLEY:MILL`) |
| `threshable` | has a `THREAD` material def (processing at the farmer's workshop gives thread; extra `thread` = its token) |
| `has_seed` | flag `SEED` or a seed material def |
| `mat_structural`, `mat_drink`, `mat_seed` | palette indices of `material_defs` basic_mat / drink / seed. The exporter **adds these materials to the palette** (materials.json), so they always resolve; `65535` = the plant has none |

Extras: `growdur_raw`, `value`, `is_grass`, `biomes` (the `BIOME_*` flags without the prefix), `underground_depth` (`[min, max]` from `[UNDERGROUND_DEPTH]`; cavern layers 1..3, `[0,0]` = surface), `brew_from`, `mill`, `thread`, `extract_barrel`, `extract_vial`, `extract_still_vial` (material defs for "process plants to barrel/vial"), `seed_name`, `growths` (`[{"id":"FRUIT","name":"apple","mat":"PLANT:APPLE:FRUIT","edible_raw":true,"edible_cooked":true,"brewable":true}]`).

```json
{"token":"MUSHROOM_HELMET_PLUMP","name":"plump helmet","name_plural":"plump helmets","growdur":30000,
 "spring":true,"summer":true,"autumn":true,"winter":true,"underground":true,"aboveground":false,"is_tree":false,"is_shrub":true,
 "brewable":true,"edible_raw":true,"edible_cooked":true,"millable":false,"threshable":false,"has_seed":true,
 "mat_structural":1,"mat_drink":142,"mat_seed":200,
 "growdur_raw":300,"value":2,"is_grass":false,"biomes":["SUBTERRANEAN_WATER"],"underground_depth":[1,3],"brew_from":"plant", ...}
```

### manifest.json: mods and counts

- `mods`: the active mods of the loaded save in load order, as `"id:numeric_version"`, e.g. `"vanilla_text:5001"`, `"vanilla_creatures:5316"`. Source: `df.global.world.object_loader.object_load_order_id[i]` and `object_load_order_numeric_version[i]`, the object loader DF used to load this save's raws (DFHack's `script-manager` reads the same vectors). Tileshtulon runs the 25 vanilla mods, no third-party mods.
- `mods_detail`: the same list with display data: `[{"id":"vanilla_text","name":"Vanilla Text","version":"50.01","numeric_version":5001}, ...]` (`object_load_order_name`, `object_load_order_displayed_version`). `rf_import::Manifest::mods_detail` and `rf_import::load_extras` read it.
- New `counts` keys: `work_orders`, `plant_raws`, `building_jobs` (sum of the exported workshop/furnace queues), `farm_plots`, `burrows`, `squads`. The importer compares the first four with what it loaded.

### units.jsonl: walk_speed

`walk_speed` is the `full_speed` of the caste's **default gait** from the creature raws: `caste.body_info.gait_info[WALK][default_gait_index[WALK]]` (DFHack 53.16-r2 exposes it on `caste_raw.body_info`, not on `caste_raw.misc`). For castes with the `FLIER` flag and a fly gait, the FLY gait is used. Units: DF speed units, 900 for a dwarf (`[GAIT:WALK:Walk:900:...]`), lower = faster. Examples from Tileshtulon: dwarf, troglodyte, crundle 900, cat 549, dog 447, horse 375, chicken 2467, magma crab 6561.

Not applied: the attribute modifiers of the faster gaits (`STRENGTH`/`AGILITY`), `LAYERS_SLOW`, carried weight, injuries, terrain. `dfhack.units.computeMovementSpeed` returns 0 in 53.16-r2, so it is not used. If no gait is found the exporter writes 900 and counts it in `notes`.

### burrows.json and squads.json (round-2 layout)

Round 2 wrote a summary only. Since round 3 both files hold `rf_core::Burrow` / `rf_core::Squad` (see "Additions for round 3"). `rf-import` reads both layouts: a round-2 burrow (`tiles` is a number) becomes a `World::burrows` entry without tiles, with one warning. `rf_import::load_extras(dir)` still returns the summary (`BurrowInfo` / `SquadInfo`) for either layout.

```json
[{"id":1,"name":"Zuflucht","tiles":1716,"blocks":18,"units":[]}]
[{"id":72,"name":"Wache","members":[13957,13834],"member_hfs":[3898,3889],"positions":10}]
```

- Burrows come from `df.global.plotinfo.burrows.list`. `tiles` counts the set bits of the block masks (`block.block_burrows` links with the burrow's id), `blocks` = `#block_x`, `units` = the assigned unit ids.
- Squads are the `df.global.world.squads.all` entries whose `entity_id` is the fort's `plotinfo.group_id`. `name` = `alias` if set, else the translated name. `members` maps each filled position's historical figure (`positions[i].occupant`, in `member_hfs`) to its unit id (`-1` if the figure has no unit); `positions` counts filled and empty positions.


## Additions for round 3 (still format v1)

Written by exporter rev `2026-10-10b` and later. Everything is optional for the importer: an older export loads with empty lists (`rf-import` tests this). The new `World` fields are `creature_raws`, `weapon_raws`, `armor_raws`, `squads` and `burrows`; the new `Unit` and `MaterialInfo` fields come in through serde.

| file | content |
|---|---|
| `units.jsonl` | new fields `needs`, `emotions`, `values`, `relationships`, `wounds`, `blood`, `blood_max`, `squad_position`, `invader_civ`; `body_size` in cm³ |
| `materials.json` | mechanics: `density`, `shear_*`, `impact_*`, `max_edge`, `value`; the palette now also holds every tissue material of the exported creature raws |
| `items.jsonl` | corpses/remains: `subtype` = creature token |
| `map/zNNN.bin` | flag bit 14 = heavy aquifer |
| `creature_raws.json` | `rf_core::CreatureRaw` per race on the map, per race of a corpse on the map, per race of a world civilization |
| `weapon_raws.json` / `armor_raws.json` | `rf_core::WeaponRaw` / `rf_core::ArmorRaw` for every item definition of the save |
| `squads.json` | `rf_core::Squad`: members by position, current orders, uniform, schedule |
| `burrows.json` | `rf_core::Burrow` with the tile list |
| `manifest.json` | `stress_cutoffs`, `max_slice_by_phase`; new `counts` keys |

**Units of measure.** DF keeps volumes in units of 10 cm³ (raw `[BODY_SIZE:12:0:60000]` is 6 000 in memory, the whip's `[SIZE:100]` is 10). The exporter writes **cm³ as in the raws** everywhere: `Unit::body_size`, `CasteRaw::body_size`, `WeaponRaw::size`, `two_handed`, `minimum_size`. The wiki's combat formulas use these raw values.

### units.jsonl: psychology

From `unit.status.current_soul.personality` (units without a soul, e.g. most animals and demons, have empty lists):

| field | source |
|---|---|
| `needs` | `personality.needs`: `id` = `df.need_type` name (`DrinkAlcohol`, `PrayOrMeditate`, `BeWithFamily`, `CraftObject`, ...), `deity_id` (prayer needs, else -1), `focus_level` (positive = satisfied, negative = distracted), `need_level` (importance 1..10) |
| `emotions` | `personality.emotions`, in DF's order: `emotion` = `df.emotion_type` name (`SATISFACTION`, `ANNOYANCE`, `GRIEF`, `ANYTHING` for -1), `thought` = `df.unit_thought_type` name (`AteFineMeal`, `SawDeadBody`, `None` for -1), `subthought`, `strength`, `severity`, `year`, `year_tick`, `flags` = set `personality_mood_flag` bits uppercased (`VOCALIZED`, `REMEMBERED_SHORTTERM`, `FAILED_TO_OVERCOME`, ...). Extra `relative_strength`. About 40 % of the entries have emotion `ANYTHING` and thought `None` with a large `subthought`; DF keeps them, their meaning is unknown, they are exported as they are. |
| `values` | `personality.values`: `id` = `df.value_type` name (`LAW`, `PEACE`, `MERRIMENT`, ...), `strength` -50..50. Only the values that differ from the culture are stored (6 per citizen on average). |
| `stress` | unchanged (`personality.stress`). Extras `longterm_stress`, `current_focus`, `undistracted_focus`, `stress_category` (`dfhack.units.getStressCategory`: 0 = most stressed .. 6 = happiest). |

`manifest.json` `stress_cutoffs` = `dfhack.units.getStressCutoffs()`: `[50000, 25000, 10000, -10000, -25000, -50000, -100000]` in 53.16-r2. A unit's category is the number of cutoffs **strictly greater** than its stress (checked against `getStressCategory` for all citizens). `rf_import::Manifest::stress_category` computes it.

### units.jsonl: relationships

`rf_core::Relationship` (`kind`, `hist_fig_id`, `unit_id` = the unit if it is alive and on the map, else -1). Extras on every entry: `source` (`family`, `visual`, `unit`), `unit_id_any` (the figure's unit even if dead or away), `dead` (the figure has a death year).

- **Family and other links**: `historical_figure.histfig_links`, kind from `df.histfig_hf_link_type`: `Mother`, `Father`, `Spouse`, `Child`, `Lover`, `Deity` (worship; extra `strength` = link strength), `DeceasedSpouse`, `FormerSpouse`, `Master`, `Apprentice`, `FormerMaster`, `FormerApprentice`, `Companion`, `Prisoner`, `Imprisoner`, `PetOwner`.
- **Siblings**: the other children of the figure's mother and father (DF has no sibling link).
- **Friends and acquaintances**: `historical_figure.info.relationships.hf_visual`. DF 53 keeps the core values `love`, `trust`, `respect`, `loyalty`, `fear` and `meet_count` there (all extras). The `kind` follows the wiki ("Relationship", v50): `Friend` at love >= 50, `CloseFriend` >= 75, `KindredSpirit` at 100, else `LongTermAcquaintance` after 30 meetings, else `PassingAcquaintance`. `Grudge` = love < 0 is an **estimate**; Trussroads has no negative love at all (minimum 0). Extras also `first_year`, `last_meet_year`, `last_meet_tick`.
- **Pets**: `unit.relationship_ids.PetOwner` gives a `PetOwner` entry with `source: "unit"`.

Trussroads: 90 citizens, 3 669 relationships: 1 872 passing acquaintances, 1 191 kindred spirits, 223 friends, 108 close friends, 68 deities, 53 siblings, 42 children, 25 lovers, 15 spouses, 7 deceased spouses.

### units.jsonl: wounds, blood, squad, invaders

- `wounds`: one `rf_core::Wound` **per wound layer** (`unit.body.wounds[i].parts[j]`; a wound without parts gives one entry with `body_part: ""`).
  - `body_part` = the caste body part token (`body.body_plan.body_parts[part.body_part_id].token`), `layer` = its `layers[part.layer_idx].layer_name`.
  - `flags` = the set bits of the wound's `flags` (`DIAGNOSED`, `SUTURED`, `SEVERED_PART`, `INFECTION`, ...) and of the layer's `flags1`/`flags2` (`CUT`, `SMASHED`, `BROKEN`, `SCAR_CUT`, `EDGED_DAMAGE`, `ARTERY`, `MAJOR_ARTERY`, `GUTS_SPILLED`, `MOTOR_NERVE_SEVERED`, `NEEDS_SETTING`, `ENTIRE_SURFACE`, ...), uppercased, deduplicated, plus the aliases `SEVERED` (= `SEVERED_PART`), `INFECTED` (= `INFECTION`) and `BLEEDING` (layer `bleeding > 0`).
  - `pain`, `bleeding` = the layer's values; `age` = the wound's age in ticks.
  - `severity` = `surface_perc` × 100 (0..10 000), an **estimate** of "how much of the layer is damaged". The raw DF values are extras: `contact_area`, `surface_perc`, `strain`, `cur_penetration_perc`, `max_penetration_perc`, `swelling`, `impaired`, `numbness`, `paralysis`, `nausea`, `dizziness`, `part_id`, `layer_idx`, `global_layer_idx`, and per wound `wound_id`, `wound_pain`, `fever`, `syndrome_id`, `attacker_unit_id`, `attacker_hist_figure_id`.
- `blood` / `blood_max` = `body.blood_count` / `body.blood_max` (DF units; a dwarf has about 4 700-5 000).
- `squad_position` = `military.squad_position` (-1 = none). The real export checks that `Squad::members[squad_position]` is the unit.
- `invader_civ` = `civ_id` for hostile units (the `hostile` flag) whose civ is not the fort's; -1 otherwise (wildlife, undead, demons). The export at 10.10.2026 caught a goblin siege: 34 goblins of civ 39. Extras `invasion_id`, `population_id`, `last_attacker` (`relationship_ids.LastAttacker`).

### creature_raws.json

`rf_core::CreatureRaw` for each race that appears among the exported units, among the corpses/remains on the map (for butchery), or as the race of a `Civilization` entity of the world (visitors, traders, invaders even before they arrive). Trussroads: 67 creatures, 135 castes, 7 MB. Extras: `index` (the raw index), `source` (`["civ","item","unit"]`), `tissues`.

The bodies are DF's **fully expanded** `caste.body_info` (after `BODY_DETAIL_PLAN`, `TISSUE_LAYER` etc.), so Rustfort does not need to expand body plans for imported races:

| field | source |
|---|---|
| `castes[].token`, `name` | `caste_id`, `caste_name[0]` |
| `body_size` | the last `caste.body_size` entry × 10 = adult size in cm³ (dwarf 60 000, human 70 000, cat 5 000). Extra `body_size_by_age` = `[[age in days, cm³], ...]` (`body_size_day` / `body_size`; dwarf `[[0,3000],[504,15000],[4032,60000]]`) |
| `walk_speed` | the default gait as for units (see round 2) |
| `flags` | the set caste flags (`CAN_SPEAK`, `NOPAIN`, `NOBREATHE`, `FLIER`, `LARGE_PREDATOR`, `HAS_BLOOD`, `EQUIPS`, ...); creature `flags` = the set creature flags |
| `body_parts[]` | `body_info.body_parts` in DF's order: `token`, `name` (`name_singular[0]`), `category`, `parent` = `con_part_id` (index into the same list, -1 = root), `relsize`, `flags` = the set part flags (`UPPERBODY`, `LOWERBODY`, `HEAD`, `GRASP`, `STANCE`, `LIMB`, `INTERNAL`, `THOUGHT`, `NERVOUS`, `CIRCULATION`, `SKELETON`, `BREATHE`, `GUTS`, `THROAT`, `JOINT`, `SIGHT`, `HEAR`, `DIGIT`, `RIGHT`, `LEFT`, `HAS_TENDONS`, ...; DFHack 53.16-r2 has no `VITAL` part flag, vital parts are the ones with `THOUGHT`, `CIRCULATION` or `BREATHE`), extras `name_plural`, `number`, `fraction_total`, `fraction_fat`, `fraction_muscle` |
| `body_parts[].layers[]` | `layers`: `tissue` = `layer_name`, `material` = the token of `creature.tissue[tissue_id]` (`CREATURE:DWARF:SKIN`), `mat` = its **palette index** (the exporter adds every tissue material to materials.json), `thickness` = `part_fraction`, `flags` = layer flags (`CONNECTS`, `MAJOR_ARTERIES`) + tissue flags (`STRUCTURAL`, `FUNCTIONAL`, `SETTABLE`, `SPLINTABLE`, `ARTERIES`, `MUSCULAR`, `NERVOUS`, `THOUGHT`, `SCARS`, ...). Extras `tissue_id`, `healing_rate`, `vascular`, `pain_receptors`, `layer_depth`, `relative_thickness` |
| `attacks[]` | `body_info.attacks`: `name` (`PUNCH`), `verb` (2nd person), `body_part` = token of `body_part_idx[0]`, `contact_area` / `penetration` = `contact_perc` / `penetration_perc` (**percent**, as `ATTACK_CONTACT_PERC` / `ATTACK_PENETRATION_PERC`), `velocity_multiplier` (×1000), `prepare_ticks` / `recover_ticks` = `time_to_strike` / `time_to_recover`, `main` = `ATTACK_PRIORITY:MAIN`, `flags` = `WITH`, `CANLATCH`, `EDGE`, `INDEPENDENT_MULTIATTACK`, `BAD_MULTIATTACK` and `SPECIALATTACK_<type>`. Extras `verb_3rd`, `skill` (`GRASP_STRIKE`, `BITE`, ...), `perc: true`, `body_parts` (all tokens), `tissue_layers` |

Dwarf: 89 parts, 243 layers, 7 attacks (punch with each hand, kick with each foot, two scratches, bite).

### weapon_raws.json and armor_raws.json

`rf_core::WeaponRaw` for `itemdefs.weapons`, `ammo` and `trapcomps` (extra `kind` = `WEAPON`, `AMMO`, `TRAPCOMP`; 44 in Tileshtulon):

- `token` = `id`, `skill` = `df.job_skill[skill_melee]` (else the ranged skill), `size`, `two_handed`, `minimum_size` in cm³ as in the raws (battle axe 800 / 47 500 / 42 500), `ranged_ammo` (ammo: `ammo_class`).
- `attacks` = the item's `[ATTACK:...]`: `name` = `verb` = verb (2nd person), `contact_area`, `penetration` (**absolute**, as in the raw), `velocity_multiplier` (×1000), `prepare_ticks`, `recover_ticks`, `flags` = `EDGE` for edged attacks plus `INDEPENDENT_MULTIATTACK` / `BAD_MULTIATTACK`. Extras `noun` (`NO_SUB`, `flat`, `pommel`), `edged`, `perc: false`, `verb_3rd`.
- Extras: `skill_ranged`, `material_size`, `value`, `shoot_force`, `shoot_maxvel`, `hits` (trap components), `flags` (`CAN_STONE`, `HAS_EDGE_ATTACK`, `TRAINING`).

`rf_core::ArmorRaw` for `itemdefs.armor`, `helms`, `gloves`, `shoes`, `pants`, `shields` (`kind` = `ARMOR`, `HELM`, `GLOVES`, `SHOES`, `PANTS`, `SHIELD`; 62): `layer` = `props.layer` as `UNDER`, `OVER`, `ARMOR`, `COVER` (`""` for shields), `coverage` = `props.coverage`, `flags` = the set `props.flags` (`SOFT`, `HARD`, `METAL`, `BARRED`, `SCALED`, `LEATHER`, `SHAPED`, `STRUCTURAL_ELASTICITY_*`) and item flags (`METAL_ARMOR_LEVELS`, ...). Extras `armorlevel`, `ubstep`, `lbstep`, `upstep`, `layer_size`, `layer_permit`, `material_size`, `value`, `blockchance` (shields).

### materials.json: mechanics

For every palette material, from `dfhack.matinfo.decode(type, index).material`:

| field | source |
|---|---|
| `density` | `solid_density` (kg/m³; iron 7 850) |
| `shear_yield`, `shear_fracture`, `shear_strain_at_yield` | `strength.yield/fracture/strain_at_yield[SHEAR]` |
| `impact_yield`, `impact_fracture`, `impact_strain_at_yield` | the same for `IMPACT` |
| `max_edge` | `strength.max_edge` |
| `value` | `material_value` |

Extras: `strength` = all six strain types `{"BENDING":[yield,fracture,strain],"SHEAR":...,"TORSION":...,"IMPACT":...,"TENSILE":...,"COMPRESSIVE":...}`, `absorption`, `liquid_density`, `molar_mass`, `heat` (`spec_heat`, `heatdam_point`, `colddam_point`, `ignite_point`, `melting_point`, `boiling_point`, `mat_fixed_temp`). 2 079 of 2 087 palette materials have a density; the rest are pseudo-materials. Iron: density 7 850, shear 155 000 / 310 000, impact 542 500 / 1 085 000, max edge 10 000 (matches the wiki's metal table).

### squads.json

`rf_core::Squad` for each squad of the fort (`entity_id == plotinfo.group_id`):

| field | source |
|---|---|
| `id`, `alias` | `squad.id`, `squad.alias` (`Wache`) |
| `name` | the translated squad name (`""` for squads that only have an alias). Extra `display_name` = alias or name, `name_native` |
| `members` | the unit id of every position in order, `-1` = empty position or a figure without unit. Extra `member_hfs` = `positions[i].occupant` in the same order |
| `orders` | the **current** orders as DF-like strings: the explicit `squad.orders` if there are any, else the orders of the active routine for the current month (`schedule.routine[cur_routine_idx].month[cur_year_tick // 33600].orders`), as DF does. Strings: `Train`, `Station x,y,z` (`MOVE`, `pos`), `Kill <unit id>` (one per target of a `KILL_LIST`), `DefendBurrows <id,id>`, `Patrol <route id>`, other types as CamelCase type names (`RetrieveArtifact`, ...). Extras `explicit_orders`, `schedule_orders`, `schedule_month` (`{month, name, sleep_mode, uniform_mode, orders:[{order, min_count}]}`), `position_orders` (per position) |
| `uniform` | the distinct uniform entries over all positions, in order: `ITEM_TYPE[:SUBTYPE][@material]` with `@<token>` for a fixed material or `@<entity_material_category>` for a class: `ARMOR@Armor` (metal armor), `WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:IRON`. Extra `uniform_by_position` = per position `[{category, item, assigned:[item ids]}]` (`category` = `df.uniform_category`: body, head, pants, gloves, shoes, shield, weapon) |
| `schedule` | the name of the active alert routine (`plotinfo.alerts.routines[id == cur_routine_idx].name`): `Off duty`, `Staggered training`, `Constant training`, `Ready`. Extra `routine_id` |

Extras: `positions`, `leader_position`, `barracks` (building ids of `squad.rooms`). During the siege of 10.10.2026 all 9 squads were on `Ready` with `Station 82,97,128` (the archers `Station 94,97,128`); 4 hours of game time later Wache, Reserve 4 and Schuetzen were back on `Constant training` (`Train`), the rest `Off duty` (`[]`).

### burrows.json

`rf_core::Burrow`: `id`, `name`, `tiles` = every tile of the burrow as `{"x","y","z"}`, `units` = the assigned unit ids. Tiles come from the block masks: for each block of `dfhack.burrows.listBlocks(b)`, the `block_burrows` link with the burrow's id, `tile_bitmask.bits[ly]` bit `lx` = tile `(map_pos.x + lx, map_pos.y + ly)`. Extras `tile_count`, `blocks`, `flags`. The verify script checks samples (and their neighbours, for the x/y orientation) through `dfhack.burrows.isAssignedTile`.

### manifest.json: round-3 keys

- `stress_cutoffs` (see psychology), `max_slice_by_phase` (longest frame slice per exporter phase in ms).
- New `counts`: `creature_raws`, `weapon_raws`, `armor_raws`, `burrow_tiles`, `unit_needs`, `unit_emotions`, `unit_values`, `unit_relationships`, `unit_wounds`. The importer compares all of them (and `squads`, `burrows`) with what it loaded.
