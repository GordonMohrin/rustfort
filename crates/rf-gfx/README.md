# rf-gfx: DF Premium sprites for Rustfort

rf-gfx reads the graphics raws and PNGs from the user's DF install at runtime (never from the repo) and answers "which sprites do I draw for this tile, unit, item or building". It does not render; rf-app uploads pages as textures and draws the returned `Sprite`s back to front.

```rust
let mut gfx = rf_gfx::Gfx::load_for_world(&rf_gfx::df_dir(), &world)?; // ~150 ms: vanilla + the save's mods
gfx.prepare(&world);                                    // creates palette variant pages (call again when the world changes)
for i in 0..gfx.pages.len() { let (w, h, rgba) = gfx.page_rgba(i as u16)?; /* upload */ }
gfx.terrain(&world, pos, &mut sprites);                 // allocation-free lookups
```

`Gfx::load(df_dir)` still loads vanilla only (or every installed mod with `RUSTFORT_GFX_MODS=1`).

Verify visually (paths relative to the repo; in a worktree use absolute export paths):

```
cargo run -p rf-gfx --release --example preview -- --out target/preview_hall.png --rect 18,22,34,20
cargo run -p rf-gfx --release --example preview -- --export export/<folder> --out target/fort.png --z 127 --rect 70,88,56,30
cargo run -p rf-gfx --release --example preview -- --export export/<folder> --z 134 --rect 120,40,32,24 --out target/canopy.png
```

Preview flags:

- `--mods a:25,b:1` overrides the world's mod list (`--mods none` = vanilla only). Without it the export's `manifest.json` `mods` are used.
- `--debug-missing` marks every tile, building, item and unit of the level that got no sprite (magenta X) and prints a histogram of the missing kinds; `--scan` does the histogram for all levels (about 25 ms for the real fort). Civzones are skipped (the UI draws zones).
- `--bench` times `terrain()` for 16 levels around `--z` and `item()`/`unit()` for everything (real fort: about 25 ns per tile, 0.5 ms for a 20k-tile frame).
- `--half` writes the PNG at half size.
- `--month 0..11` (0 = Granite) or `--tick <year tick>` overrides the calendar, to check the seasonal trees (the export's own tick comes from `manifest.json` `year_tick_end`).
- Items get their designation markers (forbidden, dump, melt, hidden) like in the app.

On the v0.1 export of the reference fort `--scan` found 4 sprite-less things left (3 procedural demons, 1 chicken), down from 17,525 (16,804 of them tree canopy tiles). Since round 6 it finds **0 on all 150 levels** of the latest export (year 277).

## Mods (`Gfx::load_with_mods`, `Gfx::load_for_world`)

- `World::meta.mods` (from the export's `manifest.json`, `"id:numeric_version"` in load order) picks exactly those mods from `data/installed_mods`, by `[ID]` and `[NUMERIC_VERSION]` in each folder's `info.txt` (several versions can be installed side by side, e.g. `perspective_walls (23)` and `(25)`). See `rf_raws::resolve_mod_list`.
  - Vanilla modules named in the list are loaded in list order; if the list names none, all vanilla modules load first.
  - A missing exact version falls back to the newest installed one (or a Steam workshop copy) and is reported in `Gfx::mod_warnings`; `Gfx::mods_loaded` lists what was loaded.
  - An empty list = vanilla only.
- Override rule (`rf_raws::GraphicsDb`): a mod's `[TILE_PAGE:X]` replaces the vanilla page X, and a mod's `[TILE_GRAPHICS]` for a name replaces *every* earlier definition of that name (repeats inside vanilla, like the 8 `BOULDER`s, are kept). This is what makes perspective_walls' `STONE_WALL_*` pieces and interface_tweaks' `BUILDING_ICON_*` win.
- Mods tested with (installed alongside vanilla on the development machine): perspective_walls (wall pages with a front face; the generic wall autotiler handles them), reduced_zfog (replaces the z-fog tile, which rf-app draws itself), interface_tweaks (building icons and interface bits; reach them via `ui_icon`/`building_icon`).

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
| tree trunk / branches / twigs / caps | per plant, see "Trees and plants" below |
| shrub / sapling | the plant's own `SHRUB` / `SAPLING` (generic ones as fallback), see below |
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

**Wall shadows**: floors next to walls (or unrevealed rock) get `WALL_SHADOW_STRAIGHT_<side>`, tapered `WALL_SHADOW_NEAR_<side>_OPEN_<diag>` where the wall run ends, and `WALL_SHADOW_AT_CORNER_<diag>` for lone diagonal walls (page `SHADOWS_WALL`). Tree trunks cast `TREE_BASE_SHADOW_*` instead.

## Trees and plants (`[PLANT_GRAPHICS:<plant>]`)

- The plant of a tile or item comes from its material token `PLANT:<ID>:<MAT>` (resolved per world material in `prepare`). Every tree tiletype is classified once per world tiletype (from DF's tiletype *name*, because a few `direction` attributes in DF's enum are wrong, e.g. `TreeCapWallThickS` says `N`).
- Plants without own `TREE_TILE`s (oak, chestnut, apricot, ...) use the standard pieces of the `TREES` page; trees with their own page (tower cap, saguaro, pine, ...) use their `TREE_TILE`s, gaps filled from the standard set.

| Tile | Sprites |
|---|---|
| trunk at ground level (tile below is no trunk) | floor (grass outside, dirt inside) + `TREE_BASE_TRUNK[_<pos>]`; neighbors get `TREE_BASE_SHADOW_<dir>` |
| trunk above ground | look-down + `TREE_TRUNK_PILLAR` / `TREE_TRUNK_<NSWE dirs>` (`TreeTrunkBranchN` -> `TREE_TRUNK_N`), thick trunks `TREE_TRUNK_THICK_<pos>`; `TREE_OVERLEAVES_TRUNK_<dirs>` where leaves touch it |
| directional branch (`TreeBranchNS`, ...) | look-down + `TREE_HEAVY_BRANCH_<dirs>` + `TREE_OVERLEAVES_HEAVY_BRANCH_<dirs>` |
| light branches (`TreeBranches`) | look-down + `TREE_BRANCH_<mask of solid tree neighbors>` + `TREE_TWIGS_FULL1..4` |
| twigs (the canopy; open tile, the renderer draws the level below first) | `TREE_TWIGS_<mask>` next to branches/trunk, else `TREE_TWIGS_FULL1..4`; dead trees `TREE_LEAFLESS_TWIGS_*` |
| mushroom caps | `TREE_CAP_WALL_<dirs>`, `TREE_CAP_WALL_THICK_<pos>`, `TREE_CAP_PILLAR`, `TREE_CAP_FLOOR_n`, `TREE_CAP_RAMP` (empty) |
| shrub / sapling | the plant's `SHRUB` (`SHRUB_DEAD`) / `SAPLING`, on grass outside or dirt inside (the ground under a plant is not exported) |
| farm plot | `FARMPLOT` + the plant's `CROP` for each planted seed (seeds inside the plot, `prepare` caches them per tile; ripe look, since growth is not exported), else `FARMPLOT_PLANTED` |
| items `PLANT` / `SEEDS` / `PLANT_GROWTH` | the plant's `PICKED` / `SEED` / `GROWTH_PICKED` (growth found via the plant raws' `GROWTH_ITEM ... LOCAL_PLANT_MAT:<mat>`) |

### Seasons (`World::calendar.year_tick`)

Trees follow their PLANT raws, like DF: `[GROWTH_TIMING]` says when a growth is on the tree and `[GROWTH_PRINT:..:fg:bg:bright:start:end]` its color over the year.

| growth | look |
|---|---|
| leaves (`[STANDARD_LEAVES]` in the plant graphics) | green print = the normal `TREES` twigs; yellow / orange / red prints = `TREES_LEAVES` `_AUTUMN:1/2/3` (oak, cherry, chestnut... turn yellow from mid-Limestone, orange in Sandstone, red in Timber); outside the timing (after tick 300 000, winter) the leaves have dropped: `TREE_LEAFLESS_TWIGS_*` and no over-leaves |
| blossoms (`[STANDARD_FLOWERS_1/2]`) | `TREES_FLOWERS` `_FLOWER:n` on top of the leaves while the growth is on (cherry, apricot in spring) |
| fruit (`[STANDARD_FRUIT_1..3]`) | `TREES_FRUIT` `_FRUIT:n` on top (apricots, cherries, bayberries in summer) |

- Flowers and fruit are standard-palette images, recolored (variant pages made by `prepare`) with the growth material's `STATE_COLOR` (e.g. cherry fruit `BURNT_UMBER`, blossoms `LAVENDER_BLUSH`), else the print color.
- Trees with their own seasonal pieces (`[TREE_TILE:TREE_TWIGS_FULL1_AUTUMN:1:TREE_MAPLE_LEAVES:4:4]`, maple) use them, gaps filled from the standard ones.
- Evergreens (bayberry, willow: one green print all year) stay green. Dead trees always use the leafless pieces.
- Per lookup this is a few comparisons on the plant's growth list (no allocation).

### Grass

- Grass floors use the grass plant's own `GRASS_1..4` (`[PLANT_GRAPHICS:CAVE MOSS] [GRASS_1:CAVERN_GRASS:0:1]`), by tiletype variant 1..4. Surface grasses look as before; cavern grasses (cave moss teal, floor fungi yellow, underlichen) now have their DF colors. The floors.png `GRASS_5*` set is the fallback.
- `[CUSTOM_EDGING:n]` grasses print their `CUSTOM_EDGE_<dir>` pieces onto neighboring tiles (sides, plus corners whose sides have none). Lower n wins; all of them win over regular grass, whose floors.png bleeding never covers them.
- `[CUSTOM_RAMP]` grass ramps: the plant's `GRASS_1` plus `OVERLAY_RAMP_WITH_WALL_<dirs>` (`RAMP_OVERLAY` page).

Not yet: `TREE_TRUNK_SLOPE_*` beyond the top piece, crop growth stages (`CROP_SPROUT`), grass blossoms (`[GROWTH:FLOWER] [GROWTH_1..4:GRASS_FLOWERS]`, none of the grasses on the reference map has them), dry/dead grass (the `GrassDry`/`GrassDead` tiletypes are drawn like living grass).

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
- Workshops of other sizes use the same `stage:tx:ty` scheme with the footprint read from the raws: 1x1 `WORKSHOP_QUERN`, `WORKSHOP_MILLSTONE` (`stage:ty`), 5x5 `WORKSHOP_SIEGE` and the trade depot (`TRADE_DEPOT`, building kind `TradeDepot`). Unbuilt workshops use their construction stage (`Building.stage`), built ones the highest stage.
- Stockpiles: `STOCKPILE_FLOOR` on each tile. Civzones: nothing (the UI draws zones).
- Traps: `CageTrap` -> `TRAP_CAGE`, `StoneFallTrap` -> `TRAP_STONE`, `WeaponTrap` -> `TRAP_WEAPON`, `Lever` -> `LEVER_SET`, `PressurePlate` -> `TRAP_PLATE_READY`.
- `Well` -> `BLD_WELL_WITH_ROPE`, `ArcheryTarget` / `Support` -> `BLD_ARCHERY_TARGET_*` / `BLD_SUPPORT_*` (stone/wood/metal), `Chain` -> `BLD_CHAIN_METAL`, `Slab` -> `ITEM_SLAB_BLANK`, roads `BLD_DIRT_ROAD` / `BLD_PAVED_ROAD`.
- Lowered bridges: `BLD_BRIDGE_<STONE|WOOD|METAL|GLASS>_<NS|WE>_<W|CENTER|E|N|S|1>`. The span direction is not exported; a bridge at least as long north-south as it is wide is drawn as a north-south span (estimate). Raised/retracting bridges are not drawn yet.
- Item mapping additions: `ROUGH` -> `ITEM_ROUGH_GEM`, `SMALLGEM` -> `ITEM_GEMS`, `ARMORSTAND`/`WEAPONRACK` -> `ITEM_*_<mat>_EMPTY`, crafts without stone versions (amulet, crown, scepter, ...) -> the metal version recolored, `TRACTION_BENCH` -> `ITEM_TRACTION_BENCH_<mat>_ROPE`, `GLOB` -> `ITEM_LIQUID` (estimate, Premium has no glob sprite).
- **Typed items** (`ARMOR_GRAPHICS`, `GLOVES_`, `HELM_`, `PANTS_`, `SHOES_`, `SHIELD_`, `FOOD_`, `TOY_`, `TRAPCOMP_`, `TOOL_`, `WEAPON_`, `AMMO_GRAPHICS`) are looked up by `Item.subtype` (e.g. `ITEM_WEAPON_PICK`), with the wood/metal/artifact variants, recolored by material.
- **Corpses** (`CORPSE`, `CORPSEPIECE`, `REMAINS`): the creature's `[CORPSE]` / `[REMAINS]` sprite. The export has no race for these items, so the race is taken from `Item.subtype` if that is a creature token (exporters may put it there), else from the longest creature name in `Item.desc` ("stray horse skeleton" -> `HORSE`, a heuristic). Layered creatures (dwarves) and unknown races get `ITEM_REMAINS`.

## Build menu (`building_ghost`, `building_size`, `building_icon`)

- `Gfx::building_size(kind, subtype) -> (w, h)`: DF's footprint (3x3 workshops/furnaces, 1x1 quern/millstone, 5x5 trade depot/siege workshop, 1x1 otherwise).
- `Gfx::building_ghost(kind, subtype) -> Vec<Sprite>`: the built look for placement previews, `dx`/`dy` relative to the top-left footprint tile (roof rows at `dy = -1`). Unrecolored; tint it yourself (e.g. alpha 150, red when the spot is invalid). Allocates, so cache it per selected building.
- `Gfx::building_icon(kind, subtype) -> Option<Sprite>`: `BUILDING_ICON_*` (8x12-px interface tiles combined into one sprite), e.g. `BUILDING_ICON_WORKSHOP_MASON`, `BUILDING_ICON_FURNACE_SMELTER`, `BUILDING_ICON_TRAP_CAGE`, `BUILDING_ICON_BED`, `BUILDING_ICON_TRADE_DEPOT`. Category icons via `ui_icon`: `BUILDING_ICON_WORKSHOPS`, `_WORKSHOPS_FURNACES`, `_WORKSHOPS_FARMING`, `_WORKSHOPS_CLOTHING`, `_FURNITURE`, `_DISPLAY_FURNITURE`, `_DOORS_HATCHES`, `_WALLS_FLOORS`, `_MILITARY`, `_TRAPS`, `_MACHINES_FLUIDS`, `_CAGES_RESTRAINT`. With interface_tweaks loaded, its versions win.

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

## Round 6 additions

- **Engravings** (`World::engravings`, indexed per tile by `prepare`; DF keeps the smooth tiletype): engraved walls use the `WALL_STONE_ENGRAVED` edge set with the same autotiling as smoothed walls; engraved floors draw `FLOOR_STONE_ENGRAVED_PALETTE` recolored by the stone (`FLOOR_STONE_ENGRAVED_NON_PALETTE` for stones without a palette row). Premium has one engraving image per family, so the quality does not change the look.
- **Item markers:** `Gfx::item_marker(&item)` returns `DESIGNATION_ITEM_FORBIDDEN` / `_DUMP` / `_FORBIDDEN_DUMP` / `_MELT` / `_FORBIDDEN_MELT` / `_HIDDEN`. rf-app draws the melt marks through it (`// [gfx6]` in `ui/round5.rs`).
- **Spider webs:** DF's webs are `THREAD` items described as "... web" (3,955 cave spider webs in the reference fort's caverns). They are drawn as `ITEM_WEB_HARMLESS:1..4` (variant by position), `ITEM_WEB_THICK` for giant cave spider webs.
- **Caste sprites:** `[CREATURE_CASTE_GRAPHICS:BIRD_CHICKEN:MALE]` and the like (rooster/hen, drake/duck, ram/ewe, deer, moose, ...) are picked by `Unit.caste` before the race-wide sprite.
- **Procedural creatures** (no graphics raws: forgotten beasts, demons such as `DEMON_2` "winged ghost", titans): DF Premium's procedural beast sprites (`[TILE_GRAPHICS_RECTANGLE:BEASTS:x:y:3:2:BEAST_<BODY>]`, now parsed into `named`).
  - The body is classified from the exported body parts: legs (`STANCE` limbs), hands (`GRASP`), wings, head and tail give `AMORPHOUS`, `SNAKE`, `WORM_SHORT`, `HUMANOID`, `BIPEDAL_DINOSAUR`, `FRONT_GRASP`, `QUADRUPED_BULKY`/`_SLINKY`, `INSECT`, `SPIDER` or `SCORPION`.
  - The sim's forgotten beasts (`FORGOTTEN_BEAST_n`, no raws) use their quadruped flag.
  - Winged ones get `_WINGS_<BAT|LACY|FEATHERED>_BACK/_FRONT` layers (bat wings for demons, lacy for bugs).
  - Forgotten beasts use the plain (stone-like) `BEAST_<BODY>`; other generated creatures get the `BEAST_ORGANIC_<BODY>` details (mouths, teeth, flesh) on top. They are not recolored, because a unit carries no material. They are bottom-aligned 3x2 sprites (centered by `unit_center_px`).
- **Planned furniture** (placed, `built == false`): the piece's sprite faded to alpha 130 (an estimate of DF's look).

## Known gaps / next steps

- rf-app still calls `Gfx::load`; switch it to `Gfx::load_for_world(&df, &world)` so the save's mods apply.
- Floors are drawn as-is (no material tint), as DF does for floors.png. The exporter does not export DF's engravings yet (the sim's engravings are drawn).
- No ramp shadows, no vision shadows, no traffic/blueprint overlays (the `DESIGNATION_TRAFFIC_*` icons exist for the UI).
- **Brooks, rivers, waterfalls:** the `BROOK`/`RIVER` bed pieces and the `FLOWS` `BROOK_TO_<dir>` / `RIVER_TO_<dir>` flow arrows exist, but the reference map has no brook or river tiles (only a murky pool), and the export carries no flow direction, so they are not mapped yet. Liquid depth is drawn by rf-app with the `UNDERWATER_1..7` sprites.
- **Blood and spatter:** the world data has no spatter/contaminants (DF `block.block_events` material spatter is not exported, and the sim keeps blood only as unit blood volume), so `blood_*.png`, `vomit.png` and `mud.png` are not drawn yet.
- `USE_STANDARD_PALETTE_FROM_ITEM` (clothing dyed by item material) is drawn uncolored.
- Custom workshops from mods (Crematorium, Armory) need the custom building token, which the export does not carry (`subtype` is `Custom`).
- The look cache is per unit id. A unit that changes profession or equipment keeps its old look until the cache is cleared; there is no public invalidation yet. Corpse sprites are cached per item id.
- Procedural creatures use the beast base sprite and wings only; DF also layers eyes, mandibles, horns, shells and material colors from the generated body.
