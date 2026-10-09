# rf-gfx: DF Premium sprites for Rustfort

rf-gfx reads the graphics raws and PNGs from the user's DF install at runtime (never from the repo) and answers "which sprites do I draw for this tile, unit, item or building". It does not render; rf-app uploads pages as textures and draws the returned `Sprite`s back to front.

```rust
let mut gfx = rf_gfx::Gfx::load(&rf_gfx::df_dir())?;   // ~120 ms: raws + page index
gfx.prepare(&world);                                    // creates palette variant pages (call again when the world changes)
for i in 0..gfx.pages.len() { let (w, h, rgba) = gfx.page_rgba(i as u16)?; /* upload */ }
gfx.terrain(&world, pos, &mut sprites);                 // allocation-free lookups
```

Verify visually:

```
cargo run -p rf-gfx --release --example preview -- --out target/preview_hall.png --rect 18,22,34,20
cargo run -p rf-gfx --release --example preview -- --out target/preview_surface.png --z 21 --rect 0,0,40,24
cargo run -p rf-gfx --release --example preview -- --export ../../export/tileshtulon --out target/fort.png --rect 70,95,48,36
```

## Pages and palettes

- Every `[TILE_PAGE]` becomes one page; the index follows load order. `PageInfo.tile_w/h` vary: most pages are 32x32, the interface pages are 8x12.
- DF Premium recolors many images with palettes. Row 0 of a palette PNG holds the colors the image uses, and row N holds the replacement colors.
  - The standard palette (`[PALETTE:DEFAULT]`, `vanilla_descriptors_graphics/images/palettes.png`, 137 rows x 18 colors) is indexed by descriptor color: `[PALETTE_COLOR:GRAY:50]`.
  - Creature layer sets have their own palettes (`LS_PALETTE_FILE`), for example dwarf skin, hair and clothes.
- A world material maps to a palette row through its descriptor color. `MaterialInfo.rgb` is the descriptor color's RGB, so we take the nearest `[COLOR]` and then its `PALETTE_COLOR` row.
- `Gfx::prepare(world)` bakes the needed recolorings into **variant pages**, which are appended to `pages`. `page_rgba` applies the swap with an exact color match.
  - If `prepare` was not called, sprites fall back to the base page with a multiply tint (an approximation).
- Palette-based terrain pages (checked by pixel colors): wall_stone (and smoothed, engraved, worn 1–3), wall_soil, wall_rock_blocks, wall_wooden, the metal walls, terrain_boulders, floor_pebbles, floor_stone_blocks, wooden/metal floors, stairs (row "PALETTE_"), and the fortifications.
- **Not** palette-based: floors.png (grass, stone and dirt floors are drawn as-is, as in DF) and the ramps. Those are never tinted.

## Terrain (`Gfx::terrain`)

Lookups are driven by the `TiletypeInfo` attributes (shape, material, special, variant), never by tiletype ids.

| Tile | Sprites (back to front) |
|---|---|
| hidden (`HIDDEN` flag) | `HIDDEN_ROCK_1..5`, picked by position hash |
| open space / ramp top | the tile one level below, darkened (DF's look-down) |
| floor: grass | `GRASS_5`; tiletype variant 1..4 → `5`, `5B`, `5C`, `5D` |
| floor: soil | `DIRT_FLOOR_5[B/C/D]`; sand soil → `SAND_FLOOR_*`; furrowed → `FURROWED_SOIL_1..4` |
| floor: stone/mineral/lava/feature | `STONE_FLOOR_5[B/C/D]`; smooth → `SMOOTH_FLOOR` |
| floor: construction | wood → `WOOD_FLOOR`, metal → `METAL_FLOOR` (both palette), glass → `GLASS_CLEAR_FLOOR`, otherwise the stone block floor (palette) |
| floor: ice / ashes / driftwood | `ROUGH_ICE_FLOOR` / `SMOOTH_ICE_FLOOR`, `FLOOR_ASHES`, dirt plus `DRIFTWOOD` |
| floor edge bleeding | the 3x3 sets `X_1..X_9` (5 = the tile itself) of a higher-priority neighbor floor (grass > dirt > stone) are drawn onto lower floors. The piece for neighbor offset (dx,dy) is `X_k` with `k = (1-dy)*3 + (1-dx) + 1` |
| pebbles / boulder | floor plus `PEBBLES_FLOOR_5` / one of 8 `BOULDER` (palette by material) |
| wall | see below |
| stairs | `{GRASS,DIRT,STONE,SAND}_STAIR_{UP,DOWN,UPDOWN}`; constructed → `PALETTE_STAIR_*` recolored |
| ramp | `{STONE,SOIL,GRASS,SAND_BEIGE}_RAMP_WITH_WALL_<dirs>`, see below; fallback `*_RAMP_OTHER` |
| tree trunk / branches | grass floor plus `TREE_TRUNK_PILLAR` (or `TREE_BASE_TRUNK`); `TREE_BRANCH_NSWE` |
| shrub / sapling | floor plus `SHRUB` / `SAPLING` |
| fortification | floor plus `FORTIFICATION` (palette) |
| liquids | `WATER` / `MAGMA_1`, alpha by depth 1..7 |
| designations | `DESIGNATION_DIG_STANDARD`, `_CHOP` (tree), `_GATHER` (shrub), `_DIG_CHANNEL`, `_DIG_RAMP`, `_DIG_STAIR_{UP,DOWN,UPDOWN}`; the smooth/engrave flags → `DESIGNATION_SMOOTH` / `_ENGRAVE` |

**Walls** (autotiling inferred from the wall images).

- The wall pages hold *edge pieces*. Each piece name carries the set of sides that face non-wall tiles:
  - `STONE_WALL_N`: rim along the top edge
  - `..._N_S_W_E_1..4`: isolated pillar, 4 variants
  - `..._SE`: inner-corner blob for a wall whose only open neighbor is the diagonal SE
- The names are parsed generically (the tokens after `_WALL_`, with a trailing number as the variant), so every wall family uses the same code:
  - `WALL_STONE`, `WALL_WORN{1,2,3}_STONE`, `WALL_STONE_SMOOTHED`, `WALL_STONE_ENGRAVED`
  - `WALL_SOIL`, `WALL_SAND`
  - `WALL_ROCK_BLOCKS`, `WALL_WOODEN`, `WALL_REINFORCED_METAL`
  - `WALL_ICE[_SMOOTHED]`, `WALL_ROOT`, `WALL_ORE_VEIN`, `WALL_GEM`
- Per wall tile, back to front:
  1. the dark rock mass (`HIDDEN_ROCK_n`)
  2. the piece for the mask of open orthogonal sides (variant by position hash)
  3. one corner piece for each open diagonal whose two adjacent sides are closed
- Neighbors that are walls, hidden, or off the map count as closed.
- Mineral tiles get the ore vein (or gem) overlay on top. The family is chosen by material class and special: smooth → smoothed, WORN_n → worn, construction → blocks/wood/metal.

**Ramps** use a wall-neighbor mask: orthogonal walls, plus diagonal walls only where both adjacent orthogonals are open. The mask is looked up exactly, then without its diagonals, then falls back to `_RAMP_OTHER`.

## Units (`Gfx::unit`)

- `[CREATURE_GRAPHICS]` objects with the same id are merged, because DF splits them across sprite, portrait and corpse files. Portrait layer sets are skipped.
- **Simple creatures** (cats, dogs, most animals): the `[DEFAULT:PAGE:x:y]` sprite, or `[CHILD:...:DEFAULT]` for children. `LARGE_IMAGE` sprites are bottom-aligned (`dy`).
- **Layered creatures** (dwarves; humans, elves and goblins use the same code): the `LAYER_SET` matching the state is used (DEFAULT, CHILD or BABY).
  - Layers outside groups are drawn when all their conditions hold. Inside a `[LAYER_GROUP]` the first matching layer wins.
  - Supported conditions:
    - `CONDITION_CASTE`, `CONDITION_PROFESSION_CATEGORY` (DF profession → MINER, WOODWORKER, STONEWORKER, FARMER, ... mapping in `creature.rs`)
    - `CONDITION_ITEM_WORN` (BY_CATEGORY / BY_TOKEN) and `SHUT_OFF_IF_ITEM_PRESENT`
    - `CONDITION_TISSUE_LAYER` (SKIN, HAIR, CHIN_WHISKERS) with `TISSUE_MAY_HAVE_COLOR`, `TISSUE_MIN/MAX_LENGTH`, `TISSUE_MAY_HAVE_SHAPING` and `TISSUE_NOT_SHAPED`
    - `CONDITION_RANDOM_PART_INDEX` and `CONDITION_MATERIAL_FLAG` (NOT_ARTIFACT and WOVEN_ITEM count as true)
    - Syndrome and ghost conditions are false.
  - Palettes: `USE_PALETTE:<LS_PALETTE>:row` is baked into variant pages by `prepare`. `USE_STANDARD_PALETTE_FROM_ITEM` is currently drawn as-is.
- The export has no skin, hair or beard data yet, so these are picked deterministically from the unit id (gray hair from age 120).
- Worn items come from items held by the unit (`holder` or `inventory`): ARMOR, PANTS, SHOES, HELM, GLOVES, WEAPON, SHIELD. Without any, a default outfit is used: shirt, trousers, shoes, plus a pick for miners or an axe for woodcutters.
- Looks are cached per unit id in `prepare`, so units without `prepare` get only their DEFAULT sprite.
- **Renderer note:** multi-tile unit sprites (2-tile-wide weapons, large creatures) must be centered on the tile. Add `sprite.unit_center_px(tile_w)` to x. It is 0 for 1x1 sprites.

## Items (`Gfx::item`) and buildings (`Gfx::building`)

- Items and furniture use `ITEM_<KIND>_<WOOD|STONE|METAL|GLASS>`, then `ITEM_<KIND>`, then `ITEM_<KIND>S`, recolored by the item's material. Examples:
  - `BOULDER` → `ITEM_BOULDER`, `WOOD` → `ITEM_WOOD`, `BAR` → `ITEM_BARS`, `BLOCKS` → `ITEM_BLOCKS`
  - `DRINK` → `ITEM_BARREL_WOOD_LIQUID`
  - `BARREL`, `BIN`, `BUCKET`, `CAGE` → wood or metal versions
  - `PLANT` → `ITEM_PLANT` (generic, not per plant yet)
- Building kinds `Bed`, `Table`, `Chair`, `Door`, `Box`, `Cabinet`, `Coffin`, `Armorstand`, `Weaponrack`, ... use the same item sprites.
- Workshops and furnaces: the `WORKSHOP_*` / `FURNACE_*` tile graphics carry `stage:tx:ty` (stage 0..3, where 3 = built; ty 0 is the row *above* the 3x3 footprint).
  - The base is recolored by the building material. `<NAME>_OVERLAY` is drawn on top unchanged.
  - Subtypes mapped: Carpenters, Masons, Craftsdwarfs, Still, Kitchen, Butchers, Tanners, Fishery, Farmers, Jewelers, Mechanics, MetalsmithsForge, MagmaForge, Loom, Leatherworks, Clothiers, Dyers, Bowyers, Ashery, Custom, Smelter, MagmaSmelter, WoodFurnace, Kiln, MagmaKiln, GlassFurnace, MagmaGlassFurnace.
  - For the top row of the footprint, an extra sprite with `dy = -1` is pushed.
- Stockpiles: `STOCKPILE_FLOOR` on each tile. Civzones and farm plots: nothing yet (the UI draws zones).

## UI icons (`Gfx::ui_icon(name)`)

- Any `TILE_GRAPHICS` name works. Graphics made of parts (`[TILE_GRAPHICS:PAGE:x:y:NAME:col:row]`) are combined into one sprite with `tw`/`th` set, in the page's tile size; interface pages use 8x12 px tiles.
- Main toolbar and time controls:
  - `BUTTON_DIG`, `BUTTON_DIG_DIG_ACTIVE` / `_INACTIVE`, `BUTTON_DIG_STAIRS_*`, `BUTTON_DIG_RAMP_*`, `BUTTON_DIG_CHANNEL_*`, `BUTTON_DIG_REMOVE_STAIRS_RAMPS_*`
  - `BUTTON_DES_CHOP_*`, `BUTTON_DES_GATHER_*`, `BUTTON_DES_SMOOTH_*`, `BUTTON_DES_SMOOTH_ENGRAVE_*`, `BUTTON_DES_SMOOTH_TRACK_*`, `BUTTON_DES_SMOOTH_FORTIFY_*`, `BUTTON_DES_ERASE`
  - `BUTTON_DES_TRAFFIC_*`, `BUTTON_DES_FORBID_*`, `BUTTON_DES_DUMP_*`, `BUTTON_DES_MELT_*`, `BUTTON_DES_HIDE_*`, `BUTTON_DES_CLAIM_*`
  - `BUTTON_BUILDING_ACTIVE` / `_INACTIVE`, `BUTTON_ZONE_ACTIVE` / `_INACTIVE`, `BUTTON_BURROW_ACTIVE` / `_INACTIVE`, `BUTTON_HAULING_ACTIVE` / `_INACTIVE`
  - `BUTTON_PAUSE_ACTIVE` / `_INACTIVE`, `BUTTON_PLAY_ACTIVE` / `_INACTIVE`, `BUTTON_SETTINGS`
- Map markers (32x32): `DESIGNATION_*` (see above), `DESIGNATION_TRAFFIC_HIGH` / `_LOW` / `_RESTRICTED`, `DESIGNATION_ITEM_FORBIDDEN` / `_DUMP` / `_MELT` / `_HIDDEN`.
- Zones: `ZONE_*` (for example `ZONE_BEDROOM`, `ZONE_MEETING`, plus border pieces `ZONE_ACTIVE_N_S`...).
- Any other name in `data/vanilla/vanilla_interface/graphics/*.txt` also works.

## Known gaps / next steps

- Mods: `RUSTFORT_GFX_MODS=1` loads `data/installed_mods` (newest version per mod id). The save's actual mod list (for example perspective_walls) is not read yet.
- Floors are drawn as-is (no material tint), as DF does for floors.png. Engraved walls and floors need the engraving data from the export.
- No wall shadows on floors (`WALL_SHADOW_*`), no `RAMP_OVERLAY`, no tree canopies or leaves, no plant-specific item sprites, no brooks or rivers, no spatter.
- `USE_STANDARD_PALETTE_FROM_ITEM` (clothing dyed by item material) is drawn uncolored.
- The look cache is per unit id. A unit that changes profession or equipment keeps its old look until the cache is cleared; there is no public invalidation yet.
- Workshops with other footprints (1x1 and 5x5 pages: quern, millstone, siege workshop, ...) are not mapped yet.
