# Fort export format (v1)

This is the contract between the DFHack exporter (`tools/dfhack/rustfort-export.lua`) and the Rust importer (`crates/rf-import`).

Rules:
- The format mirrors the types in `crates/rf-core`. JSON field names are the serde names of those structs. The importer can deserialize them directly, and fields it does not know are ignored.
- Missing optional fields fall back to their defaults.
- You may add fields. Document any you add here. Do not rename or remove fields.

Exports contain data from a real save and **never go into git**. The default location is `rustfort/export/<fort>/`, which is gitignored.

## Directory layout

```
export/<fort>/
  manifest.json      metadata, counts, timing
  tiletypes.json     DF tiletype enum (all entries)
  materials.json     material palette (only materials used by tiles/items/buildings)
  map/z000.bin ...   one binary file per z-level, z000 = bottom of the map
  units.jsonl        one JSON object per line (rf_core::Unit)
  items.jsonl        rf_core::Item
  buildings.jsonl    rf_core::Building
  plants.jsonl       rf_core::Plant
  creatures.json     optional: info about races present in units
```

## manifest.json

```json
{
  "format_version": 1,
  "df_version": "53.16", "dfhack_version": "53.16-r2",
  "world_name": "Zilirróth", "fort_name": "Trussroads", "fort_name_native": "Tileshtulon",
  "site_id": 123, "civ_id": 45, "group_id": 67,
  "region_x": 0, "region_y": 0, "region_z": 0,
  "dims": {"x": 192, "y": 192, "z": 153},
  "year": 250, "year_tick_start": 12345, "year_tick_end": 12400,
  "counts": {"units": 0, "items": 0, "buildings": 0, "plants": 0, "materials": 0, "tiletypes": 0},
  "export_seconds": 0.0
}
```

- `dims` are in tiles: `x = world.map.x_count`, `y = y_count`, `z = z_count`.
- The calendar comes from `df.global.cur_year` and `df.global.cur_year_tick`.
- The exporter does **not** pause the game. Ticks pass during the export, so the exporter records both the start and the end tick.

## tiletypes.json

An array of `rf_core::TiletypeInfo`, one entry for every DF tiletype id:

```json
{"id": 219, "name": "StoneWall", "shape": "WALL", "material": "STONE", "special": "NORMAL", "variant": 0, "direction": ""}
```

- `shape`, `material` and `special` are DF enum names: `df.tiletype_shape[attrs.shape]` and so on.
- `variant` is `0` for NONE and `1..4` for VAR_1..VAR_4.
- `direction` is `attrs.direction`, or `""` if there is none.

## materials.json

An array of `rf_core::MaterialInfo`. The array index is the palette index.

```json
{"mat_type": 0, "mat_index": 12, "token": "INORGANIC:GRANITE", "name": "granite",
 "rgb": [128,128,128], "is_stone": true, "is_gem": false, "is_metal": false, "is_ore": false,
 "is_soil": false, "is_wood": false, "is_glass": false, "tags": []}
```

- `token` is `dfhack.matinfo.decode(type, index):getToken()`.
- `name` is the solid state name.
- `rgb` is the solid state color, taken from the descriptor color and converted to 0..255.

## map/zNNN.bin

- Each file holds `dims.x * dims.y` records. Rows are in y order, and x runs fastest within each row.
- A record is **8 bytes, little endian**, exactly the layout of `rf_core::Tile`:

| bytes | field | meaning |
|---|---|---|
| 0–1 | `tt` u16 | DF tiletype id |
| 2–3 | `mat` u16 | palette index into materials.json, `0xFFFF` = none |
| 4–5 | `flags` u16 | `rf_core::tile_flags` bits: hidden, outside, subterranean, light, aquifer, pile, traffic(2 bits), smooth/engrave designation, marker, building occupancy, stagnant, salt |
| 6 | `liquid` u8 | bits 0–2 = depth (flow_size 0..7), bit 3 = magma |
| 7 | `desig` u8 | `rf_core::Dig`: 0 none, 1 dig/chop/gather, 2 up-down stair, 3 channel, 4 ramp, 5 down stair, 6 up stair |

How the material is resolved per tile, depending on the tiletype material class:

| Class | Material source |
|---|---|
| STONE / SOIL | geology layer material (biome region → geo_biome layer → `mat_index`) |
| MINERAL | the vein from the block's mineral events (tile bitmask) |
| FEATURE | the feature's material (adamantine, slade) |
| LAVA_STONE | the region's lava stone |
| CONSTRUCTION | `df.construction.find(pos)` |
| FROZEN_LIQUID | ice (water, solid) |
| GRASS_* | the grass plant from the block's grass events, or `NO_MAT` if unknown |
| TREE / PLANT / ROOT / MUSHROOM | the plant occupying the tile (wood for trees) |
| everything else | `0xFFFF` |

Approximations are fine where DF does not make the answer easy to get. Report them in `manifest.json` under `"notes"`.

## units.jsonl / items.jsonl / buildings.jsonl / plants.jsonl

There is one JSON object per line, matching `rf_core::Unit`, `Item`, `Building` and `Plant`.

**Units** are taken from `world.units.all`, skipping units that are gone or off the map unless they are citizens. `flags` is `rf_core::UnitFlags`. Enum-typed values use DF enum names:
- profession: `df.profession`
- skills: `df.job_skill`
- labors: `df.unit_labor`
- mood: `df.mood_type`

**Items** are taken from `world.items.all`. Notes:
- `kind` is `df.item_type[item:getType()]`.
- `subtype` is the subtype token, if the item has one.
- `mat` is a palette index.
- `pos` comes from `dfhack.items.getPosition`.
- `desc` is `dfhack.items.getReadableDescription` or `getDescription(item, 0)`.
- `value` is `dfhack.items.getValue`.
- `container`, `holder` and `building` come from the item's general references.

**Buildings** are taken from `world.buildings.all`. Notes:
- `kind` is `df.building_type[bld:getType()]`.
- `subtype` is the workshop, furnace, civzone or trap type name.
- `extents` is a row-major 0/1 array, or empty for a full rectangle.
- `stockpile.categories` lists the top-level stockpile setting categories that are enabled.

**Plants** are taken from `world.plants.all`:
- `token` is `world.raws.plants.all[plant.material].id`;
- `tree` is true for trees;
- `shrub` is true for shrubs that are not trees.
