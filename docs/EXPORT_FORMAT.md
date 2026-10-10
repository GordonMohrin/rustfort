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
  entities.json      rf_core::EntityInfo array: own civ + site government, contacts, wars, trade partners (round 4, optional)
  nobles.json        rf_core::NoblePosition array with mandates (round 4, optional)
  settings.json      rf_core::GameSettings: population caps, invasions, temperature, ... (round 4, optional)
  hist_figs.json     rf_core::HistFigName array: every referenced historical figure (round 4, optional)
  caravans.json      rf_core::Caravan array: merchant caravans (round 4, optional)
  diplomats.json     diplomats / outpost liaisons (round 4, optional; rf_import::load_extras)
  engravings.json    rf_core::Engraving array: DF's existing engravings (round 5, optional)
  work_details.json  rf_core::WorkDetail array: v50 work details (round 8, optional)
  kitchen.json       rf_core::KitchenSetting array: cook/brew exclusions (round 8, optional)
  locations.json     rf_core::Location array: temples, taverns, libraries, guildhalls, hospitals (round 8, optional)
  crimes.json        rf_core::Crime array: justice cases at the site (round 8, optional)
  punishments.json   punishments DF is carrying out (round 8, optional; rf_import::load_extras)
  haul_routes.json   rf_core::HaulRoute array: hauling routes with stops (round 8, optional)
  stockpile_options.json  every stockpile leaf DF offers, per category/group -> World::stockpile_options (round 9, optional)
  invasions.json     every invasion DF sent (plotinfo.invasions.list; round 9, optional; rf_import::load_extras)
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


## Additions for round 4 (still format v1)

Written by exporter rev `2026-10-10c` and later. Everything is optional for the importer: an older export loads with empty lists, buildings without `flags`/`links`/`direction`, and `World::settings` = DF's defaults (`rf_import::default_settings()`: population cap 200, strict cap 220, baby cap 1000, invasions/temperature/weather/cave-ins on; `ImportReport::settings` is then false). The new `World` fields are `entities`, `nobles`, `settings`, `hist_figs` and `caravans` (`rf_core::Caravan`, added to rf-core in round 4); the new `Building` fields come in through serde.

| file | content |
|---|---|
| `buildings.jsonl` | new fields `flags`, `links`, `direction`; extras `link_targets`, `link_triggers`, `trap_items`, `caged_units` and per-type raw values |
| `entities.json` | `rf_core::EntityInfo` for the fort's civ and site government, every entity in their diplomacy lists, every civilization of the world, caravan/diplomat/trade-partner entities and the civs of units on the map |
| `nobles.json` | `rf_core::NoblePosition` for every position assignment of the fort's site government and its civilization, with mandates |
| `settings.json` | `rf_core::GameSettings` from `d_init` and the save's difficulty settings |
| `hist_figs.json` | `rf_core::HistFigName` for every referenced historical figure |
| `caravans.json` | `rf_core::Caravan` per entry of `plotinfo.caravans` |
| `diplomats.json` | diplomats and liaisons (`rf_import::DiplomatInfo`, via `load_extras`) |
| `manifest.json` | new `counts` keys (see below) |

### buildings.jsonl: flags, links, direction

`flags` are DF building state bits under Rustfort names, per building type (DF 53.16 field names in brackets). Unnamed set bits are written as `BIT<n>`.

| building | `flags` |
|---|---|
| `Door`, `Hatch` | `door_flags`: `LOCKED` (= `forbidden`, the door sheet's **Lock** button; Gordon's "forbidden doors" that steer invaders into the trap corridor), `CLOSED` or `OPEN` (`closed`), `OPERATED_BY_MECHANISMS`, `TAKEN_BY_INVADERS`, `USED_BY_INTRUDER`, `INTERNAL` (`unused_02`, DF 0.47's "internal"; never set in DF 53). **DF 53 has no pet-passable bit** on doors (`door_flags` has 6 named bits), so `PET_PASSABLE` is never exported. Extra `close_timer`. |
| `Bridge` | `gate_flags`: `RAISED` (else `LOWERED`), `RAISING`, `LOWERING`, `COLLAPSING`, `HAS_SUPPORT`. `direction` = `building_bridgest.direction`: `W` (Left, 0), `E` (Right, 1), `N` (Up, 2), `S` (Down, 3), `RETRACT` (-1). A raised drawbridge is a wall; a raised retracting bridge is open space (wiki "Bridge"). Extras `timer`, `material_amount`. |
| `Floodgate`, `GrateWall`, `GrateFloor`, `BarsVertical`, `BarsFloor` | `gate_flags`: `CLOSED` (else `OPEN`), `CLOSING`, `OPENING`, `COLLAPSING`, `HAS_SUPPORT` |
| `Weapon` (upright spikes) | `gate_flags`: `RETRACTED` (else `EXTENDED`), `RETRACTING`, `UNRETRACTING`; `LOADED` if it holds weapons |
| `Trap` / `Lever` | `ACTIVE` = pulled (`state` != 0; each pull toggles it and toggles all linked buildings) |
| `Trap` / `PressurePlate` | `PLATE_UNITS`, `PLATE_WATER`, `PLATE_MAGMA`, `PLATE_TRACK`, `PLATE_CITIZENS`, `PLATE_RESETS` (`plate_info.flags`); `ACTIVE` when ready; `direction` = the enabled triggers with their ranges, e.g. `units:50000-200000,water:1-7`; extra `plate` (all min/max) |
| `Trap` / `CageTrap`, `StoneFallTrap`, `WeaponTrap` | `LOADED`: an empty cage / a boulder / at least one weapon or trap component built in; `FULL`: the cage holds a creature; `RESETTING`: `ready_timeout` > 0; `ACTIVE` = armed = loaded and not resetting |
| `Trap` / `TrackStop` | `track_flags` uppercased; `direction` = dump direction (`N`, `SE`, ...); extra `friction` |
| all traps | extras `trap_state`, `ready_timeout`, `fill_timer` |
| `Well` | `well_flags`: `LOWERING`, `JUST_RAISED`; extra `bucket_z` |
| `TradeDepot` | `trade_flags`: `TRADER_REQUESTED`, `ANYONE_CAN_TRADE`; `ACCESSIBLE` (`accessible`, set by DF when wagons can reach it) |
| `Support` | `support_flags` uppercased |

`links` are mechanism links (DF: a mechanism item built into each end). The exporter reads the general refs of the items in `contained_items`: `BUILDING_TRIGGERTARGET` (the item sits in a lever/plate and names the building it operates) and `BUILDING_TRIGGER` (the item sits in the target and names the lever/plate), plus `linked_mechanisms` of levers/plates (each item is held by its target). So a lever lists its targets, a target its levers/plates (extras `link_targets` / `link_triggers` keep the two directions apart). The importer drops links to missing buildings, self links and duplicates and makes one-sided links symmetric, with a warning each. `rf_import::is_trigger(b)` = lever or pressure plate.

Trap contents: `items` (as before, `contained_items`) now also has the extra `trap_items` for traps, upright spikes and anything holding a cage: `[{"id", "kind", "subtype", "use_mode"}]` (`use_mode` 2 = built-in part, 0 = loaded item such as the cage or boulder). Creatures in cages (cage traps and built cages): extra `caged_units` from the cage's `CONTAINS_UNIT` refs.

```json
{"id":11808,"kind":"Trap","subtype":"Lever","x1":86,"y1":97,"z":128, ...,"flags":[],"links":[11814],"direction":"","link_targets":[11814],"link_triggers":[],"trap_state":0,"ready_timeout":0,"fill_timer":0}
{"id":11814,"kind":"Bridge","subtype":"","x1":103,"y1":100,"z":131, ...,"flags":["HAS_SUPPORT","LOWERED"],"links":[11808],"direction":"S","link_targets":[],"link_triggers":[11808],"timer":0,"material_amount":1}
{"id":11753,"kind":"Trap","subtype":"WeaponTrap", ...,"items":[926057,919630,1165898,921590],"flags":["ACTIVE","LOADED"],"links":[],"direction":"","trap_items":[{"id":926057,"kind":"WEAPON","subtype":"ITEM_WEAPON_AXE_BATTLE","use_mode":2}, ...]}
```

Trussroads (10.10.2026, year 277): 130 doors (127 closed, none locked at the time), 2 bridges each with a lever (the gate bridge "Hoftor" 11814 at (103,100,131), lever 11808 at (86,97,128), lowered; bridge 10972 at z121, lever 10973 pulled, raised), 79 loaded traps (25 weapon traps with 59 weapons/trap components, 23 cage traps, 31 stone-fall traps; 57 of them on z131 = the "Fallenschnecke"), 1 trade depot.

### entities.json

`rf_core::EntityInfo`:

| field | source |
|---|---|
| `id`, `name` | `historical_entity.id`, English `translateName(name, true)` (extra `name_native`) |
| `race` | creature token of `entity.race` (`""` for entities without race, e.g. religions) |
| `kind` | `df.historical_entity_type`: `Civilization`, `SiteGovernment`, `NomadicGroup`, `Religion`, `Guild`, ... |
| `relation` | `Own` for `plotinfo.civ_id` and `plotinfo.group_id`; else the entity's entry in the civ's `relations.diplomacy.state` (falling back to the site government's): `relation` 1 = `War`, 0 = `Peace`; `Unknown` if neither lists it (no contact) |
| `trades` | the entity sent a caravan to this site (history events of type `MERCHANT` with `site` = the fort) or has one in `plotinfo.caravans` now |

Extras: `parent` (the civ of a site government, from `entity_links` type PARENT), `reasons` (why it is listed: `own`, `diplomacy`, `civilization`, `caravan`, `diplomat`, `trade`, `units`), `diplomacy_flags` (e.g. `ALLIES`), `war_event_collection`, `caravans_sent`, `first_caravan_year`, `last_caravan_year` (from the merchant events), `units_on_map` (units with that `civ_id`), `hostility_level`, `siege_tier`, `deities` (historical figure ids).

Trussroads: 58 entities. Own: the Channels of Direction (dwarves, civ 29, 44 caravans since year 212) and the Urns of Rhyme (site government 303). War: 3 civilizations (goblins 31 and 39, the sieging civ 39; elves 37). Peace: humans 35 (`ALLIES`, 12 caravans since 264), kobolds 33, and 25 other site governments. The goblin *site governments* are listed at peace while their civilizations are at war; this is DF's data (invaders carry the civ id).

### nobles.json

`rf_core::NoblePosition` per `entity.positions.assignments` entry of the site government and then the civilization (so a position with several holders, like militia captains, has several entries):

| field | source |
|---|---|
| `token` | `entity_position.code`: `MAYOR`, `MANAGER`, `BROKER`, `BOOKKEEPER`, `CHIEF_MEDICAL_DWARF`, `MILITIA_COMMANDER`, `MILITIA_CAPTAIN`, `EXPEDITION_LEADER`, `BARON`, `MONARCH`, `OUTPOST_LIAISON`, ... |
| `name` | `name[0]`, or `name_female[0]` / `name_male[0]` by the holder's sex (`baroness`, `king`) |
| `holder` | unit id of the assignment's historical figure if alive and on the map, else -1 (vacant or away; extra `holder_hf` tells which) |
| `entity_id` | the site government or the civilization |
| `mandates` | text of every mandate the holder issued (`world.mandates.all`, matched by `mandate.unit`), only on the holder's first position that may issue mandates (`mandate_max` > 0). Text: `Make 1 spear`, `Make 3 crowns (gold)`, `Ban export of spears`, `Guild: ...` |

Extras: `holder_hf`, `holder_unit_any`, `holder_name`, `assignment_id`, `position_id`, `squad_id`, `responsibilities` (e.g. `TRADE`, `ACCOUNTING`, `MANAGE_PRODUCTION`, `HEALTH_MANAGEMENT`, `LAW_ENFORCEMENT`, `MEET_WORKERS`), `flags`, `precedence`, `number`, `mandate_max`, `demand_max`, `required_bedroom`, `required_office`, `required_dining`, `required_tomb`, `required_boxes`, `required_cabinets`, `required_racks`, `required_stands` (room value / furniture demands), and `mandates_detail` (`rf_import::MandateInfo`: `mode` = `Export`/`Make`/`Guild`, `item_type`, `item_subtype`, `mat`, `amount_total`, `amount_remaining`, `timeout_counter`, `timeout_limit`, `punishment_hammerstrikes`, `punishment_prison_time`). Mandates whose issuer holds no mandate position are listed in `notes`.

Trussroads: 21 fort assignments (mayor = expedition leader Amost Castlelancers, manager = dungeon master, bookkeeper, chief medical dwarf, 9 militia captains; **broker, hammerer, champion, sheriff, captain of the guard and militia commander vacant**) and 12 civ assignments (king, counts, barons, general, diplomat, outpost liaison; none on the map). One mandate: the mayor's `Ban export of spears`.

### settings.json

`rf_core::GameSettings`:

| field | source |
|---|---|
| `population_cap`, `strict_population_cap`, `baby_cap` | `d_init.dwarf.population_cap`, `strict_population_cap`, `baby_cap_absolute`: the values the running game uses (DF 50+ fills `d_init` from `prefs/d_init.txt` / the in-game settings) |
| `invasions` | difficulty `Enemies` (`plotinfo.main.custom_difficulty.difficulty_enemies`): `Off` = false; `Custom` = the `sieges` flag; `Normal`/`Hard` = true |
| `temperature`, `weather`, `cave_ins` | `d_init.feature.flags` (`d_init_flags4`) `TEMPERATURE`, `WEATHER`, `CAVEINS` |
| `other` | `KEY=value` strings: `DIFFICULTY_ENEMIES`, `DIFFICULTY_ECONOMY`, `VISITOR_CAP`, `BABY_CAP_PERCENT`, `SPECIFIC_SEED_CAP`, `FORTRESS_SEED_CAP`, `GRAZE_COEFFICIENT`, `CULL_DEAD_UNITS_AT`, `MAXIMUM_EMBARK_DIM`, every `FEATURE_<d_init flag>`, every field of `custom_difficulty` uppercased (arrays comma-separated, e.g. `ENEMY_POP_TRIGGER=20,50,80,110,140`; flags as `FLAG_SIEGES=false`; `INVASION_UNIT_CAP`, `MIN_RAIDS_BEFORE_SIEGE`, `SIEGE_FREQUENCY`, `MANDATE_PERIOD`, `FORT_DIG_SPEED`, ...) and the traffic costs |

Extras: `baby_cap_percent`, `visitor_cap`, `artifacts` (`FEATURE_ARTIFACTS`: strange moods), `difficulty_enemies`, `source`.

Trussroads: population cap **90**, strict cap 120, baby cap 120 (1000 %), **temperature off**, weather and cave-ins on, artifacts on, enemies `Normal` (siege population triggers 20/50/80/110/140, invasion unit cap 140), economy `Normal`.

### hist_figs.json

`rf_core::HistFigName` for every historical figure the export references: the units themselves (`Unit::hist_fig_id`), every `Relationship::hist_fig_id` (family, friends, deities), prayer needs (`Need::deity_id`), squad members, noble holders, diplomats, and the deities/worship of the fort's civ and site government.

| field | source |
|---|---|
| `name` | English `translateName(hf.name, true)` as DF shows it (`Kikrost Pillarworked`, deities like `Mondul the Dead`); the race name for unnamed figures. Extra `name_native` |
| `race` | creature token of `hf.race` |
| `alive` | `died_year == -1` |
| `unit_id` | the figure's unit if alive and on the map (as `Relationship::unit_id`), else -1; extra `unit_id_any` = `hf.unit_id` |

Extras: `sex`, `born_year`, `died_year`, `civ_id`, `deity` (`hf.flags.deity`), `profession`, `spheres` (deities: `info.metaphysical.spheres`, e.g. `DEATH`, `WEALTH`).

Trussroads: 163 figures (135 alive, 94 on the map, 28 dead; 7 worshipped deities).

### caravans.json and diplomats.json

`rf_core::Caravan` per `plotinfo.caravans` entry (DF keeps a leaving caravan in the list for a while):

| field | source |
|---|---|
| `entity_id`, `civ`, `race` | `caravan.entity` and that entity's English name and race |
| `state` | `df.caravan_state.T_trade_state`: `None`, `Approaching`, `AtDepot`, `Leaving`, `Stuck` |
| `leaves_in` | **ticks**: `time_remaining` × 10 (DFHack's `caravan` script counts 120 per day = 1 200 ticks) |
| `depot` | the trade depot building id (an `ACCESSIBLE` one first), -1 = none |
| `goods`, `animals` | `caravan.goods` (item ids), `caravan.animals` (unit ids of pack animals/wagons) |
| `merchants` | active units with `dfhack.units.isMerchant` and the caravan's civ |
| `import_value`, `export_value`, `mood`, `flags` | `import_value`, `export_value_total`, `mood`, the set `flags` bits |

Extras: `time_remaining`, `depot_notified`, `export_value_personal`, `offer_value`, `haggle_fail_count`, `civ_native`.

`diplomats.json` (read by `rf_import::load_extras` as `DiplomatInfo`): one entry per `plotinfo.dip_meeting_info` (`civ_id`, `diplomat_hf` = `diplomat_id`, `associate_hf`, their units, `flags`, `source: "meeting"`) and per active diplomat unit without a meeting (`source: "unit"`).

At the first probe of 10.10.2026 a dwarven caravan (civ 29) was `Leaving` with 560 goods and 2 animals, and the outpost liaison was on the map for a meeting; both had left before the export, so the Trussroads export has 0 caravans and 0 diplomats. The caravan code path is therefore only tested with synthetic data.

### manifest.json: round-4 keys

New `counts`: `entities`, `nobles`, `mandates`, `hist_figs`, `caravans`, `diplomats`, `building_links` (lever/plate → target links), `levers`, `locked_doors`, `traps`, `trade_depots`. The importer compares all of them except `diplomats` with what it loaded. New phases in `max_slice_by_phase`: `history` (the merchant-event scan, 1 000 events per check), `entities`, `nobles`, `settings`, `trade`, `hist_figs`.

## Additions for round 5 (still format v1)

Exporter revision `2026-10-10d`. The only new file is `engravings.json`; the importer loads it when present into `World::engravings`, so room values count DF's existing engravings.

### engravings.json

One `rf_core::Engraving` per entry of `world.event.engravings` (the DF v50 home of the engraving list; DF 53.16 had 16 246 in Trussroads on 10.10.2026):

| field | source |
|---|---|
| `pos` | `engraving.pos`: the engraved tile. Wall engravings sit on the wall tile itself. |
| `quality` | `engraving.quality` clamped to 0..5 (ordinary .. masterful) |
| `floor` | `engraving_flags.floor`: true for floors, false for walls |
| `mat` | palette index of the engraved tile's material, by the same rules as `map/zNNN.bin` (layer stone, vein mineral, construction, feature); added to the palette if missing |
| `engraver` | unit id of `engraving.artist` (a historical figure) when that unit is alive and on the map, else -1 |
| `skill` | `engraving.skill_rating` |

Extras (ignored by the importer): `artist_hf` (historical figure id), `sides` (which faces of a wall show the engraving: `north`, `south`, `east`, `west`, `northwest`, ...), `hidden`, `art_id`, `art_subid`, `masterpiece_event`.

The importer drops engravings outside the map and a second engraving on the same tile, clamps `quality` to 5, and replaces an unknown or out-of-range `mat` with the material of the tile. Each repair is a warning in `ImportReport::warnings`.

New `counts` key: `engravings` (compared with what was loaded). New phase in `max_slice_by_phase`: `engravings` (the budget is checked every 32 entries).

## Additions for round 8 (still format v1)

Exporter revision `2026-10-10e`. Everything is optional for the importer: an export without `work_details.json` loads with empty `World::work_details` / `kitchen` / `locations` / `crimes` / `haul_routes`, stockpiles without filters (= everything in their categories) and units with `pet_owner = pasture = -1` (`ImportReport::round8` is then false). The rf-import code lives in `crates/rf-import/src/round8.rs`.

| file | content |
|---|---|
| `units.jsonl` | new fields `pet_owner`, `training`, `pasture`, `slaughter`, `pregnancy`, `work_details` |
| `buildings.jsonl` | stockpiles: `filters`, `use_links_only`, `give_to`, `take_from`; civzones: room evaluation extras |
| `work_details.json` | `rf_core::WorkDetail` per v50 work detail |
| `kitchen.json` | `rf_core::KitchenSetting` per kitchen exclusion |
| `locations.json` | `rf_core::Location` per abstract building of the fort's site |
| `crimes.json` | `rf_core::Crime` per justice case at the fort's site |
| `punishments.json` | `rf_import::PunishmentInfo` per punishment in progress (via `load_extras`) |
| `haul_routes.json` | `rf_core::HaulRoute` per hauling route |
| `creature_raws.json` | caste extras for animal products; pseudo-flag `SHEARABLE` |
| `manifest.json` | new `counts` keys (see below) |

### units.jsonl: animals, pregnancy, work details

| field | source |
|---|---|
| `pet_owner` | `unit.relationship_ids.PetOwner` (a unit id), -1 = none |
| `training` | animals only: `df.animal_training_level[unit.training_level]` (`SemiWild`, `Trained`, `WellTrained`, `SkilfullyTrained`, `ExpertlyTrained`, `ExceptionallyTrained`, `MasterfullyTrained`, `Domesticated`, `WildUntamed`); `""` for non-animals |
| `pasture` | the Pen/pasture civzone whose `assigned_units` lists the unit (DF v50 keeps the pasture assignment there), else the unit's `BUILDING_CIVZONE_ASSIGNED` general ref, else -1 |
| `slaughter` | `unit.flags2.slaughter` |
| `pregnancy` | `unit.pregnancy_timer` in ticks (all units, citizens too); 0 = not pregnant |
| `work_details` | names of the work details whose `assigned_units` lists the unit, in DF's work detail order |

Extras: `gelded` (`flags3.gelded`), `only_do_assigned_jobs` (`flags4.only_do_assigned_jobs`, DF 53's per-unit "only do assigned work details" switch), `pregnancy_caste`, `pregnancy_spouse`.

The importer rebuilds `Unit::work_details` from the work details' member lists (warning when they differ), sets an owner that is not among the exported units to -1, and a pasture that is not a civzone to -1. `0` from `Unit::default()` (worlds made in code) becomes -1 without a warning.

### buildings.jsonl: stockpile filters and links

`stockpile` gains:

| field | source |
|---|---|
| `filters` | DF's full pile settings (`building_stockpilest.settings`) as leaf strings, see below |
| `use_links_only` | `stockpile_flag.use_links_only` ("take from links only") |
| `give_to` | `links.give_to_pile` + `links.give_to_workshop` (building ids) |
| `take_from` | `links.take_from_pile` + `links.take_from_workshop` |

Extras: `give_to_pile`, `take_from_pile`, `give_to_workshop`, `take_from_workshop` (the links split by kind), `stockpile_number`, `filters_complete`.

**Filter strings.** One string per leaf, `category/group/token`. Category and group names are DF's field names in `stockpile_settings` (the same names DFHack's `stockpiles` plugin uses in its settings files):

- `category/group/TOKEN` enables one leaf;
- `category/group/*` enables the whole group (written when all of DF's entries are set);
- when more than half of a group is set, the exporter writes `category/group/*` plus `category/group/!TOKEN` for each excluded leaf;
- `category/options/NAME` is a switch that is on;
- `category/none/-` marks an enabled category with nothing enabled in it (so it is never confused with "no filters");
- `misc/options/allow_organic`, `misc/options/allow_inorganic` are pile-wide (`settings.misc`); `misc/none/-` if both are off.

A category without any filter string accepts everything in it (exports before round 8, piles created without detailed settings). `rf_core::StockpileSettings::filter_allows(category, group, token)` and `filter_option(category, option)` implement these rules.

| category | groups (token kind) | options |
|---|---|---|
| `animals` | `enabled` (creature) | `empty_cages`, `empty_traps` |
| `food` | `meat`, `fish`, `unprepared_fish`, `egg`, `plants`, `drink_plant`, `drink_animal`, `cheese_plant`, `cheese_animal`, `seeds`, `leaves`, `powder_plant`, `powder_creature`, `glob`, `glob_paste`, `glob_pressed`, `liquid_plant`, `liquid_animal`, `liquid_misc` (all organic material) | `prepared_meals` |
| `furniture` | `type` (furniture type), `other_mats` (list A), `mats` (inorganic), `quality_core`, `quality_total` (quality) | |
| `corpses` | `corpses` (creature) | |
| `refuse` | `type` (item type), `corpses`, `body_parts`, `skulls`, `bones`, `hair`, `shells`, `teeth`, `horns` (creature) | `fresh_raw_hide`, `rotten_raw_hide` |
| `stone` | `mats` (inorganic), `ore` (inorganic, DF 53's separate `settings.ore` list; empty on Trussroads) | |
| `ammo` | `type` (itemdef ammo), `other_mats` (list D), `mats` (inorganic), `quality_core`, `quality_total` | |
| `coins` | `mats` (inorganic) | |
| `bars_blocks` | `bars_other_mats` (list E), `blocks_other_mats` (list F), `bars_mats`, `blocks_mats` (inorganic) | |
| `gems` | `rough_other_mats`, `cut_other_mats` (builtin material), `rough_mats`, `cut_mats` (inorganic) | |
| `finished_goods` | `type` (item type), `other_mats` (list B), `mats` (inorganic), `color` (color), `quality_core`, `quality_total` | `dyed`, `undyed` |
| `leather` | `mats` (organic Leather), `color` | `dyed`, `undyed` |
| `cloth` | `thread_silk`, `thread_plant`, `thread_yarn`, `thread_metal`, `cloth_silk`, `cloth_plant`, `cloth_yarn`, `cloth_metal` (organic Silk / PlantFiber / Yarn / MetalThread), `color` | `dyed`, `undyed` |
| `wood` | `mats` (plant) | |
| `weapons` | `weapon_type` (itemdef weapons), `trapcomp_type` (itemdef trapcomps), `other_mats` (list C), `mats` (inorganic), `quality_core`, `quality_total` | `usable`, `unusable` |
| `armor` | `body`, `head`, `feet`, `hands`, `legs`, `shield` (itemdef armor/helms/shoes/gloves/pants/shields), `other_mats` (list C), `mats` (inorganic), `color`, `quality_core`, `quality_total` | `usable`, `unusable`, `dyed`, `undyed` |
| `sheet` | `paper`, `parchment` (organic Paper / Parchment) | |

Token kinds (index `i` of DF's settings vector → token):

| kind | token | example |
|---|---|---|
| creature | `world.raws.creatures.all[i].creature_id` | `COW` |
| organic | material token of `mat_table.organic_types[cat][i]` / `organic_indexes[cat][i]` (cat = `df.organic_mat_category` of the group) | `CREATURE:COW:MUSCLE`, `PLANT:MUSHROOM_HELMET_PLUMP:DRINK` |
| inorganic | `INORGANIC:` + `inorganics.all[i].id` | `INORGANIC:HEMATITE` |
| plant | `PLANT:` + `plants.all[i].id` + `:WOOD` (the wood material token; the list covers every plant) | `PLANT:TOWER_CAP:WOOD` |
| item type / furniture type | `df.item_type[i]` / `df.furniture_type[i]` | `BARREL`, `BED` |
| itemdef | `world.raws.itemdefs.<list>[i].id` | `ITEM_WEAPON_PICK` |
| builtin material | material token of type `i` | `GLASS_GREEN` |
| color | `descriptors.colors[i].id` | `AMBER` |
| quality | `df.item_quality[i]` (7 entries) | `Masterful` |
| list A (furniture, 15) | `WOOD PLANT_CLOTH BONE TOOTH HORN PEARL SHELL LEATHER SILK AMBER CORAL GREEN_GLASS CLEAR_GLASS CRYSTAL_GLASS YARN` | |
| list B (finished goods, 16) | list A + `WAX` | |
| list C (weapons/armor, 10) | `WOOD PLANT_CLOTH BONE SHELL LEATHER SILK GREEN_GLASS CLEAR_GLASS CRYSTAL_GLASS YARN` | |
| list D (ammo, 2) | `WOOD BONE` | |
| list E (bars, 5) | `COAL POTASH ASH PEARLASH SOAP` | |
| list F (blocks, 4) | `GREEN_GLASS CLEAR_GLASS CRYSTAL_GLASS WOOD` | |

The lengths of lists A-F match DF 53.16's vectors (checked on Trussroads); their order follows DFHack's stockpiles plugin (ESTIMATE: not checked one by one in the UI). A list whose length differs from DF's vector gets `#<index>` tokens and a manifest note. An index without a resolvable token is written as `#<index>`.

The `pile_filters` phase scans the settings before the buildings phase. It yields every 1 024 entries and re-finds the pile by id after a yield; a vector that changed length in between ends that group early. Trussroads: 43 piles, 481 strings (most groups are complete `*`; "Farmebene Ernte" lists 114 single plants).

### buildings.jsonl: room evaluation of civzones

Every civzone gets these extras (read by `rf_import::load_extras` as `RoomEval`):

| extra | source |
|---|---|
| `room_value` | DF's own value where DF has one: the location's `abstract_building.contents.location_value` for the location's meeting-area zone; -1 for personal rooms (bedrooms, offices, dining halls, tombs, also a tavern's rented bedrooms) |
| `room_name` | `dfhack.buildings.getRoomDescription(zone, owner)` when non-empty, else the location tier name: temples and guildhalls by DF's difficulty thresholds (`TEMPLE_VALUE` 2 000 -> `Temple`, `TEMPLE_COMPLEX_VALUE` 10 000 -> `Temple Complex`, `GUILDHALL_VALUE` 2 000 / `GRAND_GUILDHALL_VALUE` 10 000; below: `Meeting Hall`; rule from DFHack's `caravan/pedestal.lua`), other locations their kind |
| `room_description` | `getRoomDescription` as returned |
| `location_id`, `location_kind`, `location_tier` | the location (`contents.building_ids` or the zone's `location_id`), its kind and `contents.location_tier` |
| `room_thought`, `room_thought_severity`, `room_thought_year`, `room_thought_tick` | the owner's newest `BedroomQuality` / `DiningQuality` / `OfficeQuality` / `TombQuality` emotion (by zone type): DF's verdict on the room; -1 if none |
| `room_collision` | `flags.room_collision` |

**What DF 53.16 offers (observed 10.10.2026):** DF keeps no value for personal rooms. `building_civzonest` has no value field, `dfhack.buildings.getRoomDescription` returns `""` for every v50 zone and every bed/table (with and without the owner), and the zone vmethods `getPersonalValue(unit)` / `getArchValue()` return 0 (calling them changed no flags). So DF's evaluation of bedrooms is only visible as the owners' quality thoughts, and the exact value-to-tier thresholds of personal rooms cannot be read from DF. For locations DF stores the value, and the tier names follow the difficulty thresholds above (Trussroads: 10 temples from 990 to 68 369, three of them "Temple Complex").

### work_details.json

`rf_core::WorkDetail` per `plotinfo.labor_info.work_details` entry:

| field | source |
|---|---|
| `name` | `work_detail.name` |
| `labors` | `df.unit_labor` names with `allowed_labors[l]` set |
| `assigned` | `assigned_units` (unit ids; the importer drops ids that are not exported units, e.g. the dead) |
| `mode` | `df.work_detail_mode[flags.mode]`: `Default`, `EverybodyDoesThis`, `NobodyDoesThis`, `OnlySelectedDoesThis` |
| `icon` | `icon` (DF's icon index as a string) |
| `no_other_work` | always false: DF 53 has no such per-detail flag; the per-unit switch is the unit extra `only_do_assigned_jobs` |

Extras: `index`, `no_modify`, `cannot_be_everybody`. Trussroads: the 12 details (Miners, Woodcutters, Hunters, Planters, Fisherdwarves, Plant gatherers, Stonecutters, Engravers, Haulers, Orderlies, Siege operators, Smelters).

### kitchen.json

`rf_core::KitchenSetting` per entry of `plotinfo.kitchen` (parallel vectors `item_types`, `item_subtypes`, `mat_types`, `mat_indices`, `exc_types`): `item_type` (DF name), `item_subtype` (subtype token), `mat` (material token), `no_cook` = `exc_types[i].Cook`, `no_brew` = `exc_types[i].Brew`. Extras `mat_type`, `mat_index`, `item_subtype_index`. The importer merges duplicate (type, subtype, material) entries. Trussroads: 269 entries, all "no cook" (84 seeds, 78 drinks, 34 globs (tallow), 32 plants, 27 plant growths, 14 misc liquids), none "no brew".

### locations.json

`rf_core::Location` per abstract building of `plotinfo.main.fortress_site.buildings`:

| field | source |
|---|---|
| `id` | `abstract_building.id` (= the civzones' `location_id`) |
| `kind` | `df.abstract_building_type`: `TEMPLE` -> `Temple`, `INN_TAVERN` -> `Tavern`, `LIBRARY` -> `Library`, `GUILDHALL` -> `Guildhall`, `HOSPITAL` -> `Hospital`, others in CamelCase |
| `name` | English `translateName(name, true)` |
| `deity` | historical figure of `deity_data.Deity` when `deity_type` = 0 (a deity), else -1 (extra `religion` = the religion entity when `deity_type` = 1) |
| `profession` | guildhalls: `df.profession[contents.profession]` |
| `zones` | `contents.building_ids` plus every civzone whose `location_id` is the location |
| `occupations` | `abstract_building.occupations`: `kind` = `df.occupation_type` (`TAVERN_KEEPER`, `PERFORMER`, `SCHOLAR`, `DOCTOR`, `DIAGNOSTICIAN`, `SURGEON`, `BONE_DOCTOR`, ...), `unit` = `occupation.unit_id` (-1 vacant) |
| `allow_visitors` | flag `VISITORS_ALLOWED` |

Extras: `type`, `name_native`, `religion`, `deity_type`, `flags` (`df.abstract_building_flags`), `non_citizens_allowed`, `members_only`, `exists`, `location_value`, `location_tier`, `tier_name`, `supplies` (desired/count of goblets, instruments, paper, splints, thread, cloth, crutches, powder, buckets, soap, copies), `occupations_detail` (with occupation and histfig ids). Deities of temples are added to `hist_figs.json`. The importer drops zones that are not exported buildings and makes holders that are not exported units vacant.

Trussroads (year 280): 23 locations: 10 temples (7 with a deity), 2 taverns ("The Confusing Pick" with 5 zones incl. 4 rented bedrooms, value 3 899), 3 libraries, 6 guildhalls (mason, farmer, miner, planter, craftsman, stoneworker), 2 hospitals; 15 occupations (tavern keeper, 2 performers, 2 scholars, 10 hospital posts).

### crimes.json and punishments.json

`rf_core::Crime` per `world.crimes.all` entry with `site` = the fort's site:

| field | source |
|---|---|
| `id`, `kind` | `crime.id`, `df.crime_type[crime.mode]` (`ProductionOrderViolation`, `ExportViolation`, `JobOrderViolation`, `Murder`, `DisorderlyBehavior`, `BuildingDestruction`, `Vandalism`, `Theft`, ...) |
| `criminal`, `victim` | unit ids (`crime.criminal`, `crime.victim`; often dead units, so not among the exported units) |
| `site_tick`, `year` | `event_time`, `event_year` |
| `convicted` | `crime.accused` if `flags.sentenced`, else -1 |
| `punishment` | if sentenced: `Hammerstrikes N`, `Jail N` (`punishment.prison_time` as DF stores it; unit not confirmed), `Beating`, `Exile`, `Death`, comma-separated |
| `witnesses` | distinct `witnesses[i].witness_id` (unit ids) |

Extras: `accused`, `criminal_hf`, `victim_hf`, `criminal_name`, `victim_name`, `flags` (`sentenced`, `discovered`, `needs_trial`), `hammerstrikes`, `prison_time`, `punishment_flags`, `discovered_year`, `discovered_tick`, `entity`, `item_id`, `incident_id`.

`punishments.json` (`rf_import::PunishmentInfo` via `load_extras`): `plotinfo.punishments`: `criminal`, `officer`, `beating`, `hammerstrikes`, `prison_counter` (remaining ticks), `time_to_assign`, `chain`, `victims`.

Trussroads: 328 crimes since year 215 (205 disorderly behavior, 60 production order violations = mandates, all 60 sentenced, 36 vandalism, 26 building destruction, 1 murder); 1-2 punishments running.

### haul_routes.json

`rf_core::HaulRoute` per `plotinfo.hauling.routes` entry: `id`, `name`, `vehicles` (`vehicle_ids`, minecart item ids), and `stops`: `name`, `pos`, `take_from` / `give_to` (stockpile ids of `stop.stockpiles` by link mode `take` / `give`), `conditions` as text: `<Push|Ride|Guide> <North|South|East|West>[ after N ticks][ when >= P% full | when <= P% full][ [flags]]` from `stop.conditions` (`mode`, `direction`, `timeout`, `load_percent`, `flags`). Extras: `vehicle_stops`, per stop `id`, `cart_id`, `time_waiting`, `links` (`building_id` + mode flags). Trussroads has no routes, so this path is tested with synthetic data only. The importer drops stop links to missing buildings and missing vehicles.

### creature_raws.json: animal products

Caste extras (read by `rf_import::load_extras` as `AnimalProducts` for castes that have any): `milkable` (`{mat, frequency}` from `extracts.milkable_mat/matidx` and `misc.milkable`, or `null`), `grazer` (`misc.grazer` when the caste has `GRAZER`, else -1; DF fills `misc.grazer` for every caste), `shearable` (`shearable_tissue_layer`: `length` and the body part tokens), `egg_materials` (material tokens), `clutch_size`, `litter_size`, `egg_size`, `pet_value`. `MILKABLE`, `GRAZER` and `LAYS_EGGS` were already in the caste `flags`; castes with a shearable layer now also carry the Rustfort pseudo-flag `SHEARABLE`.

### manifest.json: round-8 keys

New `counts`: `work_details`, `kitchen`, `locations`, `occupations`, `crimes`, `punishments`, `haul_routes`, `stockpile_filters` (all filter strings), `pets` (units with an owner), `pastured`, `slaughter`, `pregnant`. The importer compares all but `punishments` with what it loaded (before its repairs). New phases in `max_slice_by_phase`: `pile_filters`, `work_details`, `kitchen`, `locations`, `crimes`, `haul_routes`.

## Additions for round 9 (still format v1)

Exporter rev `2026-10-10f`. New files `stockpile_options.json` and `invasions.json`; new unit, squad and settings fields. Everything is optional for the importer (`rf_import::round9`).

### units.jsonl: units DF does not simulate, cages, chains

**Finding (live game, 10.10.2026):** the 80-82 tame animals at position -30000 in the r4-r6 exports are **animals of departed merchant caravans** (pack animals and unsold livestock). DF keeps them in `world.units.all` with `flags1.forest` ("part of a merchant caravan" in DFHack's `devel/export-dt-ini.lua`), `civ_id` = the fort's civ (the dwarven caravans), no owner, no general or specific refs, birth years 208-278, and `flags1.inactive` **unset**, so `dfhack.units.isActive` is true. But they are **not in `world.units.active`**: DF does not simulate them. None is in a built cage, a cage item, a chain or a pasture. Their number grows with every caravan (r1 54, r4 60, r5 80, r6/r7 82). Of the 82 off-map tame units in r7, 81 are such caravan animals and 1 is a water buffalo in a caravan's (foreign) cage item at the depot, whose unit position DF leaves at -30000.

| field | rule since round 9 |
|---|---|
| `flags.inactive` | `not isActive(u)` **or** the unit is not in `world.units.active` (DF does not simulate it). r7: the 81 caravan animals plus 7 units at the map edge that left the map (a human caravan, its horses, a goblin) |
| `pos` | a caged unit whose own position is off the map gets the position of its cage item (`CONTAINS_UNIT` ref of `items.other.CAGE`); everything else as before |

Extras: `chained` (`flags1.chained` or the unit is a chain building's `chained` unit), `chain_building`, `cage_item`, `cage_building` (the built cage holding the cage item, -1), `pos_source` (`unit`, `cage_item`, `offsite`), `in_active_list`.

The importer marks, for exports before rev `2026-10-10f`, every off-map unit that is not caged as inactive (warning `N off-map units marked inactive ...`; Gordon's r5 export: 80 units, all caravan animals). rf-sim already ignored units without a map position (`Kind::Dead`); with the flag set they also leave the animal lists, the population counts and the UI.

### squads.json: ammunition, per-position uniforms, patrol routes, supplies

New `rf_core::Squad` fields (they existed in rf-core since round 8, the exporter now fills them):

| field | source |
|---|---|
| `ammo` | `squad.ammo.ammunition` (`squad_ammo_spec`): `item` = `SUBTYPE[@material]`: the itemdef id (`ITEM_AMMO_BOLTS`), then `@` + the material token when `mattype >= 0` (`@INORGANIC:IRON`) or `@` + the upper-case `df.entity_material_category` name when only `material_class` is set (`@WOOD`); `amount` (total rounds); `combat` / `training` = `flags.use_combat` / `flags.use_training` |
| `position_uniforms` | per position the uniform entries of `positions[i].equipment.uniform` (all categories), same `ITEM_TYPE[:SUBTYPE][@material]` strings as `uniform`; an empty list = an empty position or no uniform |
| `patrol` | waypoints of the patrol route a `PATROL_ROUTE` order names (explicit squad orders, else this month's schedule): `plotinfo.waypoints.routes[id].points` -> `plotinfo.waypoints.points[pid].pos` |

Extras: `patrol_route`, `patrol_route_name`, `carry_food`, `carry_water` (`squad.supplies`), `ammo_items` (item ids DF assigned as squad ammo). Trussroads: squad "Schuetzen" keeps 200 iron bolts for combat and 120 wooden bolts for training; the fort has no waypoints or routes, so `patrol` is empty everywhere (checked: `#points` = `#routes` = 0).

### settings.json: wealth, invasion tiers, fort statistics

New `settings.other` entries (`KEY=value`) and the same values in the extras object `wealth`:

| key | source | Trussroads r7 |
|---|---|---|
| `WEALTH_CREATED` | `plotinfo.tasks.wealth.total` (= weapons + armor + furniture + other + architecture + displayed + held; DF's "created wealth") | 1 576 145 |
| `WEALTH_EXPORTED`, `WEALTH_IMPORTED`, `WEALTH_OFFERED` | `tasks.wealth.exported` / `imported` / `offered` (trade totals) | 216 264 / 253 175 / 0 |
| `WEALTH_WEAPONS`, `_ARMOR`, `_FURNITURE`, `_OTHER`, `_ARCHITECTURE`, `_DISPLAYED`, `_HELD` | the parts of `tasks.wealth` | 75 491, 133 531, 235 905, 486 272, 460 214, 184 430, 302 |
| `PROGRESS_POPULATION`, `PROGRESS_TRADE`, `PROGRESS_PRODUCTION` | `plotinfo.progress_*`: the tier index (0-5) DF reached in each of the `ENEMY_POP_TRIGGER` / `ENEMY_TRADE_TRIGGER` / `ENEMY_PROD_TRIGGER` lists | 3 / 5 / 5 |
| `FORTRESS_RANK` | `plotinfo.fortress_rank` | 3 |
| `FORTRESS_AGE`, `TREES_REMOVED`, `GAME_OVER`, `LOST_TO_SIEGE_CIV` | `plotinfo.*` | 2 743 766 ticks, 238, true, 39 |
| `FORT_TOTAL_DEATHS`, `FORT_TOTAL_INSANITIES`, `FORT_TOTAL_EXECUTIONS`, `FORT_NUM_ARTIFACTS`, `FORT_INVADERS_REPELLED`, `FORT_EXCAVATED_TILES` | `plotinfo.tasks.*` | 121, 13, 0, 46, 336, 23 410 |
| `INVASION_HISTORY` | `year:year_tick:size:siege|raid` per `plotinfo.invasions.list` entry | `274:85482:34:siege,275:292484:58:siege,277:37570:143:siege,278:384970:101:siege,280:145595:148:siege` |

`rf_import::wealth(&settings)` parses them into `WealthInfo` (`None` for older exports); `load_extras` returns it as `Extras::wealth`. `tasks.wealth` is DF's current statistic at export time, not a history: the r5 values (year 279) cannot be read back from the live game any more. Imported and exported wealth only grow, so r7's values are upper bounds for r5; `PROGRESS_TRADE` and `PROGRESS_PRODUCTION` were already at the top tier (5) before the r6 siege (inference: DF raised the tiers before sending the 148-unit siege of year 280).

### invasions.json

One object per `plotinfo.invasions.list` entry (`rf_import::InvasionInfo`, `load_invasions` / `Extras::invasions`): `id`, `civ_id`, `size` (the units DF planned), `year`, `year_tick` (`created_season_count`), `siege` / `active` / `layer_source` / `undead_source` (`flags`), `mission`, `active_size1`, `active_size2`, `duration_counter`, `total_death_cage_number`. Trussroads: 5 goblin sieges (civ 39) of 34, 58, 143, 101 and 148 units in the years 274, 275, 277, 278 and 280; the last one was still active at export time.

### stockpile_options.json

Every leaf DF's stockpile editor offers, in the `category/group/token` scheme of the stockpile filters (same token kinds and groups as the table under "buildings.jsonl: stockpile filters and links"):

```json
{"scheme":"category/group/token","misc":["allow_organic","allow_inorganic"],
 "categories":[{"category":"food","options":["prepared_meals"],
                "groups":[{"group":"meat","leaves":[["CREATURE:TOAD:MUSCLE","toad muscle"], ...]}, ...]}, ...]}
```

Leaves are `[token, label]`; labels are DF's names (creature `name[0]`, `matinfo:toString()` for organic materials, the inorganic's solid state name, plant, itemdef and color names, item and furniture types in lower case). The importer flattens the file into `World::stockpile_options` (`category/options/<name>`, `misc/options/<name>` for the switches) and `Sim::stockpile_filter_options(category)` serves it to the editor.

Which entries a group lists:

| group kind | entries |
|---|---|
| creature | every creature except `DOES_NOT_EXIST` ones and `EQUIPMENT_WAGON` |
| organic | the whole organic material table of the group (`mat_table.organic_types[cat]`), as DF's vectors |
| inorganic | by group (ESTIMATE, rules as DFHack's stockpiles plugin): `stone/mats` stones (`IS_STONE` without `NO_STONE_STOCKPILE`), `stone/ore` those with `metal_ore`, `gems/rough_mats` gems, `gems/cut_mats` gems and stones, `finished_goods/mats` metals, stones and gems, `furniture/mats` and `bars_blocks/blocks_mats` metals and stones, `ammo`, `coins`, `bars_blocks/bars_mats`, `weapons`, `armor` metals |
| plant (wood) | tree plants (`TREE` flag) |
| item type | `finished_goods/type`: the 26 finished-goods types (`CHAIN` ... `BOOK`); `refuse/type`: every item type except `NONE BAR SMALLGEM BLOCKS ROUGH BOULDER CORPSE CORPSEPIECE ROCK ORTHOPEDIC_CAST` (ESTIMATE, DFHack's rules) |
| furniture type, itemdef, color, quality, lists A-F | all entries |
| builtin (gems other mats) | the glass materials |

Trussroads r7: 29 735 leaves (12 232 meats, 1 056 creatures in each of the 10 creature groups, 2 046 globs, 705 leathers and parchments; stone 89 + 16 ores; 37 metals); the file is about 1.4 MB.

### manifest.json: round-9 keys

New `counts`: `stockpile_options` (leaves), `invasions`, `offsite_units` (inactive units off the map), `caged_units`, `chained_units`. New phase `pile_options` in `max_slice_by_phase`.
