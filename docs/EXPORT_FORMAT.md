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
  "notes": ["units: skipped 409 dead ...", "..."]
}
```

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

`o.heavy_aquifer` is not exported yet.

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
- `walk_speed` is always 900, because DFHack 53.16 exposes no gait raws. This is listed in `notes`.
- Flags:
  - `citizen`, `resident`, `animal`, `tame`, `visitor`, `merchant` and `forest` come from `dfhack.units.is*`.
  - `hostile` = invader or undead, or dangerous and not tame.
  - `caged` = `flags1.caged`.
  - `inactive` = not `isActive`.
- Extra fields, ignored by serde: `profession_name` (as DF shows it) and `readable_name`.

**Items** are taken from `world.items.all`. Items that are `removed` or `garbage_collect` are skipped, and so are items without a map position (`dfhack.items.getPosition` returns nil), for example items held off-site.
- `kind` is `df.item_type[item:getType()]`.
- `subtype` is `item.subtype.id`, if the item has a subtype.
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
