# [fire15] Temperature, fire and magma: mechanics analysis and implementation spec (target v0.15)

Author role: mechanics analyst. This file is the implementer's only spec for the `[fire15]`
topic. Every rule gives: the DF rule (numbers, formulas, ticks, edge cases), its source, what
Rustfort has today, the gap, and the Rust design. Values not stated on the wiki or in the
raws are marked **ESTIMATE**; rules we know DF has but leave out are marked **MISSING**.

Sources (all read for this analysis; clean-room: wiki text, local raws, the Tileshtulon export):

- `w:Temperature`, `w:Fire`, `w:Magma`, `w:Magma-safe`, `w:Fire-safe`, `w:Obsidian`,
  `w:Obsidian farming`, `w:Magma flow`, `w:Semi-molten rock`, `w:Steam`, `w:Smoke`,
  `w:Magma mist`, `w:Wear`, `w:Water` (evaporation, freezing), `w:Ice`, `w:Dragonfire`,
  `w:Magma forge`, `w:Grass`, `w:Tree`, `w:Material definition token` (heat tokens),
  `w:Creature token` (FIREIMMUNE, FIREIMMUNE_SUPER, FIXED_TEMP, HOMEOTHERM), `w:Fire imp`,
  `w:Magma crab`, `w:Fire man`, `w:Glob`.
- Raws (read only): `vanilla_materials/objects/material_template_default.txt`
  (`FLAME_TEMPLATE` MAT_FIXED_TEMP 14000), `inorganic_*.txt`, `plant_standard.txt` (nether-cap
  MAT_FIXED_TEMP 10000), `creature_subterranean.txt` (fire man FLAME tissue MAT_FIXED_TEMP
  10800; fire imp MATERIAL_EMISSION HURL_FIREBALL / SPRAY_FIRE_JET), HOMEOTHERM counts in
  `vanilla_creatures` (198 x 10067, others 10040..10071).
- Export `export/tileshtulon` (exporter rev 2026-10-10d, DF 53.16): `materials.json` already
  carries `heat` for 2 101 palette entries (see section 2), `creature_raws.json` tissue layers
  carry their material palette index `mat`, `tiletypes.json` has the fire tiletypes
  (`Fire` 70, `BurningTreeTrunk` 71 .. `BurningTreeCapFloor` 76, `Ashes1..3` 238-240,
  `MagmaFlow` 260, `SemiMoltenRock` 259, `Frozen*`, `TreeDead*`), `settings.json` has
  `"temperature":false` (`FEATURE_TEMPERATURE=false`).

Conventions: temperatures in degrees Urist (U); `60001` = NONE (wiki Temperature: values are
u16, internally capped at 60000, 60001 means NONE). 1 200 ticks = 1 day (`rf_core::TICKS_PER_DAY`).
"Tile" = index `(z*dy+y)*dx+x`.

---------------------------------------------------------------------------------------------

## 0. Scope and boundaries with the parallel agents

| Area | Owner | fire15's part |
|---|---|---|
| Surface/ambient temperature, seasons, water freezing/thawing, snow, rain | `[weather13]` (`weather.rs`, `Sim::temperature_at`, `weather_at`, `snow_at`) | Only *reads* `temperature_at(p)` (outside tiles) and `weather_at(p)` (rain/snow put out fires). Ice melted **by magma** is fire15 (section 3.15). |
| Dragon breath, megabeast attacks | `[mega13]` | fire15 offers `fire15_api` calls (`heat_blast`, `ignite_unit`, `ignite_tile`) that mega13 can call; fire15 does not implement breath geometry. |
| Undead/ghosts | `[undead13]` | Burned-to-ash dwarves leave no corpse (wiki Fire, "Ghosts"); fire15 only removes the corpse item; ghosts stay theirs. |
| Liquids flow, pressure, evaporation | `[fluids]` (existing `fluids.rs`) | fire15 adds marked hooks in `fluids.rs` (magma contact events, obsidian edge cases). |
| Minecarts carrying magma | `[perf13]`/carts | not touched. |

All new logic lives in **new files**: `crates/rf-sim/src/fire15.rs` (+ `fire15/` submodules if
it grows: `fire15/heat.rs`, `fire15/burn.rs`, `fire15/tiles.rs`, `fire15/tests.rs`),
`crates/rf-sim/src/fire15_api.rs`, `crates/rf-app/src/ui/fire15.rs`. Every line added to a
shared file ends with `// [fire15]`.

---------------------------------------------------------------------------------------------

## 1. What Rustfort has today (round-13 contract commit c60e3a7)

| Feature | Where | Behaviour | Fidelity |
|---|---|---|---|
| Liquids, magma depth, magma sources | `fluids.rs` (`Fluids`, `update`, `fluids_tick` line ~958) | full flow model | OK |
| Obsidian | `fluids.rs::Fluids::obsidian` (~444), called from `update` (fall onto other liquid ~487, spread ~517) and `update_source` (~769) | tile becomes `LavaWall` + `INORGANIC:OBSIDIAN`, liquid cleared, aquifer flag cleared, the moving tile loses 1/7 | OK, edge cases missing (3.10) |
| Magma kills units | `fluids.rs::fluids_units` (~1285) | `MAGMA_BURN_TICKS = 10` ticks in any magma depth -> `combat_kill("has burned to death in magma")`; immune: races with `FIREIMMUNE`/`FIREIMMUNE_SUPER` or found in magma at load (`Fluids::fireproof`, ~919-931) | ESTIMATE; FIREIMMUNE semantics wrong (3.8) |
| Magma destroys items | `fluids.rs::fluids_melt_items` (~1410), fed by `Fluids::magma_new` (cells that just got magma, ~425) | loose non-magma-safe items on a new magma cell are destroyed at once (contents too) | instant (DF: heats up, 3.9) |
| Magma-safe test | `fluids.rs::magma_safe` (~300) | metal in {IRON, PIG_IRON, STEEL, NICKEL, PLATINUM, ADAMANTINE}, else any stone/gem/glass | wrong for e.g. granite (melts 11440 < 12000) |
| Magma workshops | `industry.rs::magma_shop_ok` (~474), `is_magma_shop` (~94) | need magma >= 4/7 below one of the 8 non-centre tiles; only stops taking jobs | DF cancels jobs at once (3.14) |
| Ambient temperature | `weather.rs::temperature_at_impl` | stub: climate temperature or 10 050 | owner weather13 |
| Item wear | `Item::wear` u8 (0..3, >3 destroyed); clothing wear in `lifestyle.rs` (~263) by days | no wear points | fire15 adds wear points for hot items (3.6) |
| Wounds | `combat.rs::WoundS` (~302), flags `wf::{CUT,BRUISED,BROKEN,SEVERED,TORN,BLEEDING,ARTERY}` (~82), `add_wound` (~2029), `combat_kill` (~2280) | mechanical wounds only | no burn/melt flags |
| Smoke thought caption | `social/captions.rs:69` `("Smoke", ...)` | caption exists, nothing emits it | MISSING |
| Miasma (model for smoke) | `lifestyle.rs` (`miasma`, `miasma_day`, thought every 2 days) | | pattern to copy |
| Grass state | `animals.rs` (`an_grass_amount` ~655, `Grass` records, `grass_at`) | grazing | fire15 must clear grass records when grass burns |
| Trees/shrubs | `World::plants` (`Plant { pos, token, tree, shrub, sapling, dead, grow_counter }`), tree tiles `TtMaterial::Tree/Mushroom/Root` | felling, growth | no burning |
| Temperature setting | `GameSettings::temperature` (rf-core `society.rs`); reference fort `false`; `World::synthetic()` default `false` | unused | gate for the whole module |
| FIDELITY rows | `docs/FIDELITY.md` lines 222-225, 398, 510 | "Temperature, freezing, steam: MISSING", "Fire, burns, temperature: MISSING" | to be replaced by the `[fire15]` section |

---------------------------------------------------------------------------------------------

## 2. Data the model needs

### 2.1 Material heat properties (rf-core, additive)

The exporter already writes them (`tools/dfhack/rustfort-export.lua` ~339, documented in
`docs/EXPORT_FORMAT.md` line 461: `heat` = `spec_heat, heatdam_point, colddam_point,
ignite_point, melting_point, boiling_point, mat_fixed_temp`), but `rf_core::MaterialInfo`
drops them on import (serde ignores the unknown key). **No exporter change is needed.**

Add to `crates/rf-core/src/entities.rs`:

```rust
/// [fire15] DF material heat properties (wiki Material definition token; 60001 = NONE).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialHeat {
    pub spec_heat: i32,
    pub heatdam_point: i32,
    pub colddam_point: i32,
    pub ignite_point: i32,
    pub melting_point: i32,
    pub boiling_point: i32,
    pub mat_fixed_temp: i32,
}
// in MaterialInfo:
    /// [fire15] Heat properties from the export (`heat`); all 0 = unknown (old export).
    pub heat: MaterialHeat, // [fire15]
```

`MaterialInfo` already has `#[serde(default)]`; the import test in `rf-import/src/lib.rs`
~1861 already feeds `"heat":{"melting_point":12768}`, so add an assertion there that it now
arrives. 0 everywhere = unknown -> fallback table (2.2).

Reference values from the export (for tests; ESTIMATE-free, they are DF's):

| Material | spec | heatdam | colddam | ignite | melt | boil | fixed |
|---|---|---|---|---|---|---|---|
| WATER | 4181 | NONE | NONE | NONE | 10000 | 10180 | NONE |
| INORGANIC:GRANITE | 800 | NONE | NONE | NONE | 11440 | 14000 | NONE |
| INORGANIC:BASALT | 800 | NONE | NONE | NONE | 12160 | 14000 | NONE |
| INORGANIC:OBSIDIAN | 800 | NONE | NONE | NONE | 13600 | 16000 | NONE |
| INORGANIC:IRON | 450 | NONE | NONE | NONE | 12768 | 15150 | NONE |
| INORGANIC:STEEL | 500 | | | | 12718 | 14968 | |
| INORGANIC:PIG_IRON | 500 | | | | 12106 | 13968 | |
| INORGANIC:COPPER | 385 | | | | 11952 | 14611 | |
| INORGANIC:BRONZE | 435 | | | | 11868 | 14140 | |
| INORGANIC:TIN | 210 | | | | 10417 | 14684 | |
| INORGANIC:ZINC | 390 | | | | 10755 | 11633 | |
| INORGANIC:LEAD | 130 | | | | 10589 | 13148 | |
| INORGANIC:SILVER | 230 | | | | 11731 | 13892 | |
| INORGANIC:COAL_BITUMINOUS | 409 | NONE | NONE | 11440 | NONE | 16708 | NONE |
| COAL:COKE | 409 | NONE | NONE | 11440 | NONE | NONE | NONE |
| GLASS_GREEN / GLASS_CLEAR | 700 | NONE | NONE | NONE | 13600 | 16000 | NONE |
| PLANT:TOWER_CAP:WOOD | 420 | 10250 | 9900 | 10508 | NONE | NONE | NONE |
| PLANT:NETHER_CAP:WOOD | 420 | 10250 | 9900 | 10508 | NONE | NONE | **10000** |
| PLANT:*:STRUCTURAL (grass, crops) | 3000 | 10250 | 9900 | 10508 | NONE | NONE | NONE |
| PLANT:*:SEED | 800 | 10500 | 9900 | 10400 | NONE | NONE | NONE |
| PLANT:*:DRINK | 2440 | NONE | NONE | 10800 | 9850 | 10150 | NONE |
| PLANT:*:THREAD, CREATURE:*:LEATHER | 420 | 10250 | 9900 | 10508 | NONE | NONE | NONE |
| CREATURE:*:TALLOW, CHEESE | 4181 | 10250 | 9900 | 10338 | 10078 | NONE | NONE |
| CREATURE:DWARF:SKIN, MUSCLE | 4181 | 10250 | 9900 | 10508 | NONE | NONE | NONE |
| CREATURE:DWARF:FAT | 4181 | 10250 | 9900 | 10338 | **10078** | NONE | NONE |
| CREATURE:DWARF:BONE | 1000 | 10250 | 9900 | 10508 | NONE | NONE | NONE |
| CREATURE:DWARF:HAIR | 420 | 10250 | 9900 | 10508 | NONE | NONE | NONE |
| CREATURE:MAGMA_CRAB:CHITIN/MUSCLE | 420/4181 | 13000 | 9900 | 13000 | NONE | NONE | NONE |
| CREATURE:MAGMA_CRAB:FAT | 4181 | 13000 | 9900 | 13000 | 13000 | NONE | NONE |

Not in the palette (hardcoded): **magma** = 12000 U (wiki Magma "Properties of magma",
Magma-safe "Game calculations"), not a palette entry: use a constant `MAGMA_TEMP = 12_000`.
Several metals (nickel, gold, bismuth) and lignite/charcoal/ice may be missing from a given
fort's palette; they only appear when the fort has such items/tiles, so the palette is
sufficient for everything that exists in the world.

### 2.2 Fallback when `heat` is unknown (exports before round 3, synthetic world)

`fn heat_of(&MaterialInfo) -> MaterialHeat` in `fire15/heat.rs`:
- known (any field != 0): return it;
- metal: by token from a compiled table (name the source "raws inorganic_metal.txt / wiki
  Magma-safe table"): IRON 450/12768/15150, STEEL 500/12718/14968, PIG_IRON 500/12106,
  NICKEL ESTIMATE 440/12619 (wiki Magma-safe), PLATINUM 130/13182, ADAMANTINE 7500/25000/50000,
  COPPER 385/11952, BRONZE 435/11868, SILVER 230/11731, GOLD ESTIMATE 129/11914, TIN 210/10417,
  ZINC 390/10755, LEAD 130/10589, BISMUTH ESTIMATE 122/10488; others ESTIMATE spec 450 melt 11800;
- `is_stone`/`is_gem`/`is_ore`/soil: spec 800, melt 11500, boil 14000 (STONE_TEMPLATE, ESTIMATE
  per stone), with the wiki Magma-safe stone list overriding melt (>12000) by token;
- `is_glass`: 700/13600/16000;
- `is_wood`: 420, heatdam 10250, colddam 9900, ignite 10508 (NETHER_CAP: fixed 10000);
- token `CREATURE:*`/`PLANT:*` otherwise: 4181, 10250, 9900, 10508 (organic template ESTIMATE);
- else NONE everywhere (inert).
Cache: `Fire15::heat: Vec<MaterialHeat>` indexed by palette, `#[serde(skip)]`, built in
`fire15_init` and in `after_exact_load`.

### 2.3 Creatures

- Tissue layer materials: `rf_core::military::TissueLayerRaw::mat` (palette index) for every
  layer of every body part -> per-caste table `BurnBody { parts: Vec<BurnPart> }` with
  `BurnPart { external: bool, layers: [(mat:u16, pain:i32, vascular:i32, flags:u8); <=8] }`.
  Built once per caste in `fire15_init` (cache, `#[serde(skip)]`).
- **Immunity is material-based** (wiki Creature token: FIREIMMUNE "does not, by itself, make
  the creature immune to the damaging effects of burning in fire ... would require
  adjustments to the creature's body materials"; Fire imp "only taking damage at 15000").
  A caste is *magma-proof* iff every external layer's material has `min(ignite, heatdam,
  melt, boil) > 12000` (or `mat_fixed_temp` set and inside (colddam, heatdam)); *fire-proof*
  iff the same holds at the fire temperature it meets (10708 grass fire, ignite+200 of a
  burning item). Magma crab: chitin 13000 > 12000 -> magma-proof (matches the wiki).
  `FIREIMMUNE`/`FIREIMMUNE_SUPER` keep only their documented meaning: immune to FIREBALL /
  FIREJET (and DRAGONFIRE for SUPER) **attacks**, and allowed to path through fire/magma.
- Body temperature: HOMEOTHERM (wiki Creature token; raws: 10067 for dwarves/humans, 10040-
  10071 others). Not exported. Read it at runtime from the installed raws via `rf-raws`
  (`creature` objects, caste token `HOMEOTHERM`) when available, else 10067 (ESTIMATE). It is
  only used as the starting temperature of a unit's outer layer (3.8).
- Fixed-temperature creatures (heat emitters): any caste whose tissue materials have
  `mat_fixed_temp != NONE` and > 10508 (fire man FLAME tissue: 10800 in
  `creature_subterranean.txt`; the FLAME_TEMPLATE default is 14000; wiki Temperature table
  "Creatures made of flame/fire 14000"). Derived from the palette, no exporter change.

### 2.4 Optional exporter additions (additive, read-only; only if cheap)

Document in `docs/EXPORT_FORMAT.md` under a `[fire15]` note if added:
- item `on_fire` (DF item flag `on_fire`) and `temperature` (`item.temperature.whole`) so a
  fort exported mid-fire continues; default false / 0 = ambient. Priority P3.
- caste `homeotherm` (`caste.misc` has no such field in all DFHack versions; prefer rf-raws).
Neither is needed for P1/P2.

---------------------------------------------------------------------------------------------

## 3. Mechanics

Each subsection: **DF rule** / **Source** / **Rustfort** / **Gap** / **Design** / **Tests** / **Perf**.

### 3.1 Temperature scale and constants

**DF rule.** U = F + 9968 = C*9/5 + 10000. Reference points: water freezes 10000, boils
10180; dwarf body 10067; underground 10015; tiles next to magma ("warm stone") 10075;
"lowest survivable" 9900 (colddam of fat); "highest survivable" 10078 (fat melts); organic
ignition 10508; wood/grass fire max 10708; fire-safe 11000; common stone melts ~11500;
coal fire max 11640; magma 12000 (= magma-safe threshold); flame creatures 14000; dragonfire
50000. Max 60000, NONE 60001.
**Source.** w:Temperature (scale, table), w:Magma, w:Fire-safe, w:Magma-safe.
**Rustfort.** `Sim::urist_to_celsius` exists (`[contract13]`). **Gap.** constants.
**Design.** `fire15/heat.rs`:
```rust
pub const NONE_TEMP: i32 = 60_001;
pub const UNDERGROUND_TEMP: i32 = 10_015;
pub const WARM_STONE_TEMP: i32 = 10_075;
pub const MAGMA_TEMP: i32 = 12_000;
pub const FIRE_SAFE_TEMP: i32 = 11_000;
pub const MAGMA_SAFE_TEMP: i32 = 12_000;
pub const GRASS_FIRE_START: i32 = 10_508;
pub const GRASS_FIRE_MAX: i32 = 10_708;
pub const BURN_ABOVE_IGNITE: i32 = 200;   // burning items: up to ignite + 200 (w:Fire)
pub const DRAGONFIRE_TEMP: i32 = 50_000;
```
**Tests.** `constants_match_wiki_table` (Celsius conversion of 10180 = 100 C, 12000 = 1111 C).

### 3.2 Environment temperature of a tile

**DF rule.** Each tile has a temperature; above ground it follows the biome/season (weather),
underground it is 10015 (w:Temperature table "Underground 10015"); tiles directly adjacent to
magma are heated to 10075 (w:Magma "Tiles directly adjacent to magma ... 10075"); magma is
12000; burning grass/environment fire starts at 10508 and quickly rises to 10708 (w:Fire
"Spread & Effect"). Fixed-temperature creatures alter "all adjacent tiles" (w:Creature token
FIXED_TEMP).
**Rustfort.** none (only `temperature_at` stub). **Gap.** whole model.
**Design.** No per-tile temperature array (memory and speed). A pure function:
```rust
/// [fire15] Temperature a thing on tile `i` is exposed to (environment), in U.
pub(crate) fn fire15_env_temp(&self, i: usize) -> i32 {
    // 1. liquid in the tile: magma -> MAGMA_TEMP; water -> min(ambient, 10180) (ESTIMATE: water never above boiling)
    // 2. burning tile (self.fire.cells, binary search) -> its cell temperature
    // 3. heat emitter (fixed-temp creature) on or next to the tile (self.fire.emit_tiles, sorted) -> that temperature
    // 4. ambient: OUTSIDE flag -> self.temperature_at(pos) ([weather13]), else UNDERGROUND_TEMP
}
```
Warm stone (10075) is not needed for heat exchange of items (DF items next to magma do not
burn: 10075 is far below every ignite point); it matters only for the dig warning (3.14).
Rule order: highest source wins (max of 1-3), else ambient. Ambient underground could later be
delegated to weather13 if its `temperature_at` starts handling underground tiles; keep the
OUTSIDE split in one place (`fire15_ambient(i)`).
**Tests.** `env_temp_underground_is_10015`, `env_temp_in_magma_is_12000`,
`env_temp_outside_follows_weather_api` (force `World::climate.temperature`).
**Perf.** O(log n) per call; only called for active objects.

### 3.3 Heat transfer (items, tile fuel, unit outer layer)

**DF rule.** "Every tick, an item that is exposed to a hotter/colder environment (or another
item) will adjust its temperature by the difference (in U) divided by the item's specific
heat. Fractions are retained, but not used in calculations." Worked example: lignite
(SPEC_HEAT 409) at 10015 in magma: +4.853 U in the first tick, reaches its ignition point
11440 after 517 ticks. Fixed temperature: `SPEC_HEAT:NONE` + `MAT_FIXED_TEMP` locks the
temperature; a container's fixed temperature affects its contents (water does not freeze in
a nether-cap bucket because it must go *below* 10000). Walls of a fixed-temperature inorganic
are not affected; boulders mined from it take that temperature.
**Source.** w:Temperature "Temperature transfer", "Material values".
**Rustfort.** none. **Gap.** whole model.
**Design.** Fixed point, DF-exact, integer only (deterministic):
```rust
/// whole in the high 16 bits, fraction in the low 16 (DF: temperature.whole / fraction)
#[derive(Clone, Copy, Serialize, Deserialize, Default)]
pub struct Temp(pub u32);
impl Temp {
    pub fn whole(self) -> i32 { (self.0 >> 16) as i32 }
    /// One DF tick of exchange with `env`. `spec` = SPEC_HEAT (> 0). Fractions accumulate,
    /// only the whole part enters the difference (w:Temperature).
    pub fn step(&mut self, env: i32, spec: i32) {
        let d = (env - self.whole()) as i64;
        let delta = (d << 16) / spec as i64; // truncates toward 0 (ESTIMATE: DF rounding)
        self.0 = (self.0 as i64 + delta).clamp(0, 60_000i64 << 16) as u32;
    }
}
```
Checked offline with this exact integer scheme: lignite 10015 -> 11440 in **517** ticks
(wiki: 517); tower-cap wood in magma reaches heatdam (10251) after 54 ticks and ignites
(10508) after 120 ticks; a wooden item lying on a grass fire (10708) ignites after 521 ticks;
copper in magma melts (11952) after 1 428 ticks. Materials with `spec_heat` NONE (60001) or 0
and `mat_fixed_temp` set never change; with `spec_heat` NONE and no fixed temp: never change
(DF "fixed at its initial value").
Contents of containers use the container's temperature as their environment (w:Temperature);
a container with a fixed temperature therefore pins its contents' environment.
**Tests.** `lignite_in_magma_ignites_after_517_ticks` (exact, wiki number),
`fixed_temp_material_never_changes` (nether-cap log in magma stays 10000, does not burn),
`container_contents_follow_container`.
**Perf.** Only objects in the active set (3.6/3.8) are stepped. Per tick and active object:
one `fire15_env_temp` + one integer division. A typical fort has 0 active objects.

### 3.4 Material thresholds and state changes of items

**DF rule.** (w:Material definition token, w:Temperature "Material values", w:Wear)
- `T > HEATDAM_POINT`: accelerated wear ("Items suffer accelerated wear when their
  temperature exceeds their base material's HEATDAM_POINT").
- `T < COLDDAM_POINT`: accelerated wear (cold damage), e.g. wood/cloth in a glacier depot.
- `T >= IGNITE_POINT`: the item catches fire (3.6).
- `T >= MELTING_POINT`: a solid melts (metal items become liquid; the wiki shows "glob of
  molten <metal>" which "will itself disappear after a short while", w:Magma minecart section);
  a frozen liquid thaws.
- `T >= BOILING_POINT`: boils away (booze boils, w:Fire "Dangers"; copper "boiling clouds",
  w:Smoke).
- Artifacts never wear (wear locked at 0) but still melt/boil (w:Wear, w:Fire "Artifacts").
- Melting and freezing points coincide (no supercooling).
**Rustfort.** instant destruction in magma only. **Gap.** threshold logic.
**Design.** `fn fire15_item_effects(&mut self, k: usize /*active idx*/, now)` after each
temperature step:
1. `boil` -> destroy item (+contents spill per `destroy_item`), stat `boiled`, smoke/steam
   puff (3.11/3.12).
2. `melt` (solid item, i.e. not `LIQUID_MISC`/`DRINK`) -> destroy item, stat `melted`.
   **ESTIMATE / MISSING:** no "glob of molten X" item is created (it vanishes "after a short
   while" anyway). Frozen items (ice boulders) thaw -> destroyed (w:Ice "melt into thin air";
   a thawed ice item in DF becomes a useless "water" item, bug 360 - not modelled).
3. `ignite` -> `burning = true`, temperature target becomes `ignite + BURN_ABOVE_IGNITE - 10`
   (ESTIMATE for "stay slightly below" ignite+200).
4. wear (3.6).
Liquids in containers (DRINK in barrels) use their own melt/boil: a barrel of dwarven ale
(boil 10150) next to a burning dwarf boils away while the barrel may survive.
**Tests.** `tin_item_in_magma_melts`, `booze_boils_in_a_hot_barrel`,
`artifact_does_not_wear_but_melts`.

### 3.5 Fire-safe and magma-safe

**DF rule.** Fire-safe: stable at 11000 (MELTING/BOILING/IGNITE/HEATDAM all > 11000 and
COLDDAM < 11000). Magma-safe: same at 12000. Constructions (walls, floors, ramps, stairs,
fortifications) of any material never melt or burn. Natural ice walls/floors melt. Closed
doors/floodgates/raised bridges of non-magma-safe material are safe while only acting as a
wall; opened (submerged) they melt; a raised drawbridge melts if magma flows over the area it
would occupy when lowered. Pumps with any non-magma-safe part are destroyed after prolonged
operation. Nether-cap (fixed 10000) survives magma but "will burn if used as a minecart";
the game "does not recognize it as a fire-safe building material". Coal/coke count as
fire-safe (they ignite only at 11440). For job item requirements only bars, blocks, stone,
logs and anvils can be fire-safe; others are rejected.
**Source.** w:Fire-safe, w:Magma-safe, w:Magma ("Properties of magma").
**Rustfort.** `fluids.rs::magma_safe` heuristic (wrong for granite, sandstone ok, etc.).
**Gap.** material-based test; buildings.
**Design.** In `fire15/heat.rs`:
```rust
pub fn stable_at(h: &MaterialHeat, t: i32) -> bool {
    let above = |p: i32| p == 0 || p > t; // 0 = unknown handled by fallback first
    above(h.melting_point) && above(h.boiling_point) && above(h.ignite_point)
        && above(h.heatdam_point) && (h.colddam_point >= NONE_TEMP || h.colddam_point < t)
}
pub fn fire_safe(h) -> bool { stable_at(h, 11_000) }
pub fn magma_safe(h) -> bool { stable_at(h, 12_000) }
```
(NONE = 60001 is > any t, so "above" holds.) Replace the body of `fluids.rs::magma_safe` by a
call through the heat cache: `crate::fire15::magma_safe_mat(&self.fire.heat, mat)` — it is a
free function taking `&MaterialInfo` today, so add a `heat_of(m)` fallback-aware variant and
call it from `fluids_melt_items` (`// [fire15]`). Keep the old token list only as the fallback
for unknown heat. **Hash impact:** only if a non-metal-listed stone/metal item touches magma
in the bench (expected: no change; verify).
Also offer `Sim::is_magma_safe(mat) / is_fire_safe(mat)` in `fire15_api.rs` for the UI and for
build menus (magma workshops must use magma-safe materials: `w:Magma forge` "non-magma-safe
materials will not be shown"). Building material filters are UI work in `ui/fire15.rs` or a
small marked hook in the build menu (P3).
**Tests.** `granite_is_not_magma_safe`, `basalt_obsidian_iron_steel_nickel_are_magma_safe`,
`copper_bronze_silver_are_fire_safe_not_magma_safe`, `bismuth_lead_tin_zinc_are_not_fire_safe`
(wiki list), `coal_counts_as_fire_safe`, `nether_cap_survives_magma_by_fixed_temp`.

### 3.6 Burning items and wear

**DF rule.**
- Burning items rapidly heat to ~200 U above their ignition point, slightly below because
  the surrounding air cools them (w:Fire).
- Wear: an item's damage level rises by one every 806 400 wear points (item -> xitemx ->
  XitemX -> XXitemXX -> destroyed; value 100/75/50/25 %) (w:Wear).
- Burning: **10 wear points every tick**; a burning item is "totally consumed after exactly
  322 560 game ticks" (= 4 x 806 400 / 10; 9 months 16.8 days). Applies when the material
  has an ignite point but no HEATDAM_POINT (w:Material definition token HEATDAM_POINT,
  w:Temperature "Burning items without a heat damage point ... burn for a very long time").
- Above HEATDAM_POINT wear is accelerated; the rate is not on the wiki (**ESTIMATE**:
  `+ 10 * (T - HEATDAM)` points per tick; wood burning at ~10698 vs heatdam 10250 -> 4 490
  points/tick -> destroyed in ~720 ticks (0.6 day) after ignition; in magma (12000) ~185 ticks).
- Below COLDDAM_POINT: accelerated wear (**ESTIMATE** same formula with `COLDDAM - T`).
- Artifacts: wear stays 0 (they burn forever, making smoke, until put out or melted).
  Artifact books burn forever; copies (non-artifacts) burn up.
- Water puts a burning item out unless it is inside a bin (artifact rule, w:Fire Artifacts;
  ESTIMATE: same for all items: an item submerged in water (depth >= 1) is extinguished and
  its temperature exchanges with the water).
- Burning items set fire to their tile (grass) and spread to adjacent flammables (w:Fire:
  logs, clothes, wooden furniture, buildings, lignite/bituminous coal, charcoal, coke).
**Rustfort.** `Item::wear` levels only; lifestyle clothing wear by days.
**Gap.** wear points, burning state.
**Design.** Active item set in `Fire15`:
```rust
#[derive(Clone, Copy, Serialize, Deserialize, Default)]
pub(crate) struct HotItem {
    pub id: i32,
    pub t: Temp,
    /// wear points accumulated since the last level (0..806_400)
    pub wear_pts: u32,
    pub burning: bool,
    /// ticks spent at equilibrium without effect; dropped from the set at 100 (ESTIMATE)
    pub idle: u16,
}
pub(crate) struct Fire15 { ..., pub hot: Vec<HotItem> /* sorted by id */, ... }
```
- Entry: an item enters `hot` when its environment differs from ambient enough to matter:
  magma contact, a burning tile/unit/item on its tile, a heat emitter adjacent, a heat blast.
  Entry temperature: ambient of its tile (items have no stored temperature) or the
  material's fixed temperature.
- Per tick (thermal step, only if `hot` non-empty): `t.step(env, spec)`, effects (3.4),
  wear: `wear_pts += 10 * burning + heat/cold damage`; while `wear_pts >= 806_400`:
  `wear_pts -= 806_400; item.wear += 1; if item.wear > 3 -> destroy_item` (stat `burned_up`).
  Artifacts (`flags.artifact` or `quality == 6`) skip wear.
- Exit: destroyed, or not burning and |T - env| < 1 and env == ambient for 100 ticks.
- Burning items are heat sources: their tile's env temp is at least their temperature for
  other objects on the same tile (needs `burn_tiles: Vec<(u32 tile, i32 temp)>` rebuilt each
  fire pass from burning items/units/cells, sorted).
- Item fuel tiles: a burning loose item on a grass tile ignites the grass (3.7).
- Built-in components of buildings (`Building::items`) are items too: they enter `hot` when the
  building's tile burns or gets magma; when a component is destroyed, the building is
  destroyed (3.9).
- Bins/barrels: contents use the container's temperature as env (3.3); when the container
  burns up, contents spill (existing `destroy_item` already spills, `[chaos10]`).
**Tests.** `burning_item_without_heatdam_lasts_322560_ticks` (use a test material with ignite
but heatdam NONE; run 322 559 ticks -> alive, 322 560 -> gone; use a test-only fast path or
a direct call to the per-tick function in a loop on a tiny world), `burning_wood_is_gone_in_
about_a_day` (ESTIMATE bound), `burning_artifact_never_disappears`, `water_puts_out_burning_
log`, `wear_levels_change_value_75_50_25` if value is recomputed (optional).
**Perf.** Hot items are few (tens). Stepping each tick is fine. 322 560 ticks of one burning
artifact = 322 560 cheap steps; acceptable.

### 3.7 Tile fires: grass, shrubs, trees, ashes

**DF rule.**
- Fire spreads to adjacent tiles containing flammable materials: grass, shrubs, logs,
  clothes, creatures not made of fire-safe materials, multi-tile trees, wooden furniture,
  buildings, roads, lignite/bituminous coal, charcoal, coke. Except dragonfire, fire does
  not burn rock or metal. **Constructions never burn** (w:Fire).
- **Fire does not spread across z-levels by itself.** Walls with trapped passages block it.
  Burning branches can carry fire uphill (trees). Grassy ramps/slopes do not catch fire
  (w:Fire, w:Grass "Grassy slopes will not catch on fire, thus preventing the fire from
  spreading across z-levels solely via grass").
- Grass ignited "slowly spreads to all connected grass, generating smoke"; grass becomes
  ashes ("not usable ashes"); anything flammable standing on burning grass ("A fire") "has
  a chance to catch fire each turn"; fire breath turns grass into ashes instantly (w:Grass).
- Environment fire: starts at 10508, quickly heats to 10708 (w:Fire).
- Spreading fire above ground can collapse trees ("Something outside has collapsed!") and
  hole the ceiling below (w:Fire) - **MISSING** (collapse not modelled; keep trees standing).
- A burned tree described as "dead" can rejuvenate after a few years from the trunk out
  (w:Tree) - **MISSING** (dead trees stay dead; `growth.rs` owns regrowth).
- Grass regrows on ashes over time (w:Grass erosion/regrowth; rate not stated, ESTIMATE:
  use the existing grass regrowth of `animals.rs`/`growth.rs` once the tile is soil again).
**Source.** w:Fire, w:Grass, w:Tree; tiletypes from the export.
**Rustfort.** none. **Gap.** everything.
**Design.** `fire15/tiles.rs`.
```rust
#[derive(Clone, Copy, Serialize, Deserialize)]
pub(crate) struct FireCell {
    pub tile: u32,
    pub temp: i16,      // offset from 10_000 (10508..10708 fits); or i32
    pub fuel: u16,      // ticks of burning left
    pub kind: u8,       // GRASS | SHRUB | TREE_TRUNK | TREE_BRANCH | TREE_TWIG | TREE_CAP | ITEMS_ONLY
    pub old_tt: u16,    // tiletype before burning (to pick the burnt result)
}
pub(crate) struct Fire15 { ..., pub cells: Vec<FireCell> /* sorted by tile */, ... }
```
- Fuel per kind (all **ESTIMATE**, the wiki gives no durations): grass 200 ticks, shrub 300,
  tree tile 1 200 (one day), sapling 100. Temperature: start 10508, +10 per tick up to 10708
  ("quickly heats up").
- Fire pass every `FIRE_PERIOD = 10` ticks (phase 4): for each cell in tile order:
  - `fuel -= 10`; when 0: grass -> `Ashes{1,2,3}` floor (variant from `Rng` seeded by
    (seed, tile, now)), keeping the soil material below (look up the tiletype by name in
    `TiletypeTable` at init; if absent fall back to the soil floor tiletype the
    `gather`/`set_tile(.., TtMaterial::Soil ..)` path uses, `lib.rs` ~2844); shrub -> remove
    the `Plant` record, tile -> grass-less soil/ashes; tree tile -> the matching `TreeDead*`
    tiletype (name substitution `Tree` -> `TreeDead`, `TreeCap` -> `TreeDeadCap`; ESTIMATE),
    set `Plant::dead = true` for the tree's plant record. Call `after_tile_change(i)` and
    drop the tile's `animals.grass_at` record (marked hook `self.an_grass_burned(i) // [fire15]`
    or direct removal inside fire15 since the fields are `pub(crate)`).
  - Spread: for each of the 8 same-z neighbours `n`, if `flammable_tile(n)` and not burning:
    ignite with chance **1/8 per pass** (ESTIMATE; front speed ~1 tile per 80 ticks, "slowly").
    Same-z only. Exceptions: a burning tree tile spreads also to tree tiles of the same tree
    at z+1 (branches/trunk above; "burning branches can spread fires uphill") and to grass at
    z-1 directly below a burning branch/twig (ESTIMATE: no, only uphill - keep it simple).
  - `flammable_tile(n)`: shape Floor (not Ramp) with `TtMaterial::is_grass()` (all four
    grass classes including `GrassDry`/`GrassDead`; ESTIMATE: the wiki does not exclude dry
    or dead grass); `Shape::Shrub`/`Sapling`; tree materials `Tree`/`Mushroom` (not `Root`); never a
    construction (`TtMaterial::Construction`), never water-covered (liquid depth >= 1 water),
    never magma. Ignition of a plant tile also requires its material's ignite point <= the
    source temperature (nether-cap trees: fixed 10000 -> never ignite from ordinary fire).
  - Tile ignition sources: magma entering a flammable tile (immediately, `MAGMA_TEMP >=
    ignite`), a burning item/unit on the tile (each fire pass, chance 1/2, ESTIMATE), a heat
    emitter on/next to it (env >= ignite -> ignite), `fire15_api::ignite_tile`.
  - While burning, tiletypes: grass floor -> `Fire` (id 70 in the export: FLOOR/FIRE) so the
    renderer shows it; tree tiles -> `BurningTreeTrunk/Branches/Twigs/CapWall/CapRamp/
    CapFloor` by shape. Walkability: `Fire` is a floor (walkable), burning trunk is a wall,
    same as before; call `after_tile_change` only when the shape class changes (it does not:
    floor->floor, wall->wall), so use a cheap path: update `tiles[i].tt` and mark the tile
    dirty for the renderer, no nav rebuild. (Check `nav.rs` derives walkability from shape
    only; if it reads material, call `after_tile_change`.)
- Units and items on a burning tile: see 3.6 (items: env = cell temp) and 3.8 (units: chance).
- Path avoidance ("dwarves see fire as a danger and try to avoid" w:Fire Dangers): set the
  burning tile's nav traffic cost to *restricted* while it burns and restore the player's
  traffic designation afterwards (`nav.flags` traffic bits, `nav::TRAFFIC_SHIFT`;
  store the old bits in `FireCell`). ESTIMATE (DF's exact avoidance is not documented).
  This does not change connectivity, so no component rebuild.
**Tests.** `grass_fire_spreads_on_one_level_only`, `grassy_ramp_does_not_burn`,
`construction_floor_never_burns`, `burnt_grass_becomes_ashes`, `tree_burns_to_dead_tree`,
`fire_does_not_cross_a_stone_floor_gap`, `fire_stops_at_water` (water-covered grass does not
ignite), `nether_cap_tree_does_not_ignite`.
**Perf.** `cells` sorted vec; spread pass O(cells * 8). A big surface wildfire might reach
10-50k cells; at FIRE_PERIOD 10 that is ~40k checks per 10 ticks: fine. Allocation-free: keep
a scratch `Vec<u32>` for new ignitions in `Fire15` (`#[serde(skip)]`), sort+dedup, merge.

### 3.8 Units: catching fire, burn wounds, death, extinguishing

**DF rule.**
- Creatures not made of fire-safe materials catch fire from adjacent fire (w:Fire). On
  burning grass "anything flammable ... has a chance to catch fire each turn" (w:Grass).
- Burning units "flash red and yellow", keep working, oblivious, and spread fire to others
  and their surroundings; they trail smoke (w:Fire Dangers).
- "A dwarf catching fire is not an automatic death sentence - dwarves have been observed
  catching fire, then extinguishing and surviving" (w:Fire).
- Body parts take heat damage per tissue layer: the wound layer effects include
  "burned/frostbite/melt/necrosis/blister/boil/freeze/condense" (w:Material science,
  layer_effect_fraction). Fat melts above 10078 ("Above this temperature, fat ... will start
  to melt (turning into grease), eventually killing them", w:Temperature); fat takes cold
  damage below 9900 ("lowest survivable").
- Magma crab spit "only burns areas it hits, generally melting off the fat of that body part
  ... they rapidly bleed until all the fat on the hit parts is melted away" (w:Magma crab):
  melted fat bleeds.
- Burned-to-death dwarves leave no remains (w:Fire "Ghosts").
- FIREIMMUNE: path through fire/magma, immune to FIREBALL/FIREJET attacks only; material
  points decide real immunity (2.3). FIXED_TEMP creatures are immune as long as the fixed
  temperature does not harm their materials.
- Magma contact: "nearly-instant immolation, followed by death if water is not close at hand"
  (w:Magma Dangers).
**Rustfort.** `fluids_units`: 10 ticks in magma -> dead, FIREIMMUNE races immune.
**Gap.** catching fire outside magma, burn wounds, extinguishing, material immunity.
**Design.** `fire15/burn.rs`.
```rust
#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct BurningUnit {
    pub slot: u32,
    /// burning body parts (bit = part index for the first 64 parts; parts beyond 64 are
    /// folded onto bit 63 — ESTIMATE, no vanilla body has > 64 external parts... check:
    /// HUMANOID has ~45 parts incl. teeth/organs; use only external parts)
    pub parts: u64,
    pub since: u64,
    /// in magma this pass (stronger burns)
    pub magma: bool,
}
```
1. **Ignition** (fire pass, period 10):
   - unit on a burning tile: chance **1/2 per pass** (= ~1/14 per tick; ESTIMATE for "a
     chance each turn") if the caste is not fire-proof at the cell temperature; ignites 1-3
     random external lower-body parts (ESTIMATE: stance parts first, `PartT::flags & STANCE`).
   - unit adjacent (same tile or 8-neighbour same z) to a burning unit: chance 1/10 per pass
     (ESTIMATE).
   - unit in magma (any depth, from `fluids.sub` and magma tiles): all external parts ignite
     at once (ESTIMATE for "nearly instant").
   - heat blast (dragonfire API): all external parts in the blast, unless FIREIMMUNE_SUPER or
     material-proof at 50000.
   - Ignition only if the outermost layer's material at that part has `ignite_point <=
     source temperature` or `heatdam_point < source temperature`; clothing: if the unit wears
     flammable clothing (inventory item with ignite point), the clothing item enters `hot`
     as burning (it wears out via 3.6) — ESTIMATE simplification: clothing burns but does not
     shield the part.
2. **Burn damage** (every fire pass, for each burning part): the outermost layer that is not
   yet fully burned gets `add_wound(.., flags = wf::BURN, sev += BURN_SEV)` with
   `BURN_SEV = 1 000` per pass outside magma (a layer is burned through in 100 ticks;
   ESTIMATE) and `5 000` in magma (ESTIMATE; a dwarf's skin+fat+muscle go in ~60 ticks).
   When the layer is FAT and the part's temperature (the source temperature) > the fat's
   melting point (10078), use `wf::MELTED` instead and make it bleed:
   `bleed = vascular * part_volume / 2000 * sev / 10_000` (the formula `add_wound` already
   uses for cuts, ESTIMATE). Pain from the layer's pain receptors (existing). Once all layers of
   a part are at 10 000: the part is charred - set `part_state |= 4` (function lost); for
   VITAL/THOUGHT/HEAD-with-brain or UPPERBODY-with-heart parts: death via `combat_kill(s,
   now, "has burned to death")` (ESTIMATE wording - DF's announcement text is not on the
   wiki; keep the existing "has burned to death in magma" for magma deaths).
3. **Burning goes out** when: the unit stands in water depth >= 1 (ESTIMATE; DF puts out
   burning items with water, w:Fire Artifacts) or rain/snow falls on an outside tile
   (`weather_at(p)` Rain/Snow: chance 1/2 per pass, ESTIMATE); or a burning part has no
   flammable layer left (all layers with an ignite point burned through: the fire ran out of
   fuel - this is how "extinguishing and surviving" happens when only hair/skin burned);
   or **ESTIMATE** a small per-pass chance 1/30 per part outside magma (a unit that moves
   off the fire). Units in magma do not go out.
4. **Spreading from burning units**: each fire pass a burning unit ignites its tile if
   flammable (chance 1/2, ESTIMATE) and heats the items it carries (they enter `hot` with env
   = ignite+200 of the burning tissue).
5. **AI**: burning units keep their task (DF: oblivious). Units do avoid burning tiles in
   pathing (3.7 traffic). Civilians do **not** flee to water (MISSING in DF too).
6. **Corpse**: when a burning unit dies, the corpse item (`combat_kill` creates it as the
   last item) enters `hot` as burning; corpses (heatdam 10250) burn up within ~1 day ->
   "no remains" emerges naturally. Do not delete it directly (keeps undead13 free to react).
7. **Health**: burn wounds go through the existing wounded list (`list_wounded`), so
   `health.rs` diagnoses/treats them like other wounds (DF: burns are treated like other
   wounds, suturing not needed; ESTIMATE: no suture/setting for BURN). Healing: existing
   wound healing by `age`.
8. **Fixed-temperature creatures** (fire men): never ignite themselves; they are heat
   emitters (3.2): the tile under them and the 8 neighbours get env = their fixed temp, so
   grass ignites and items heat up; they are fire-proof. Their death "burst of flame tends
   to set nearby creatures on fire" (w:Fire man) - ESTIMATE: on death, ignite the 3x3
   around with the same chances as a burning unit.
9. Wound flags: add to `combat.rs::wf` (`// [fire15]`): `pub const BURN: u16 = 128;
   pub const MELTED: u16 = 256;` and to `wf::NAMES` -> ("burned"), ("melted") so
   `wound_entry` mirrors them into `Unit::wounds`. Check `NAMES` users for array length.
**Rustfort hook changes (marked):**
- `fluids.rs::fluids_units` magma branch: `if self.fire15_on() { self.fire15_unit_in_magma(s,
  now); } else { e.burn += 1 ... existing }` — the legacy 10-tick kill stays when temperature
  is off.
- `fluids.rs::fluids_scan_units` immunity: `if self.fire15_on() { self.fire15_magma_proof(s)
  } else { fireproof.contains(race) }`; keep the "found in magma at load" immunity in both.
**Tests.** `dwarf_on_grass_fire_catches_fire_eventually` (seeded, bounded ticks),
`burn_wounds_go_skin_then_fat_then_muscle`, `melted_fat_bleeds`,
`dwarf_walking_into_water_is_extinguished`, `magma_crab_is_magma_proof_by_materials`,
`fireimmune_flag_alone_does_not_protect` (synthetic caste with FIREIMMUNE and dwarf tissues),
`fire_man_ignites_grass_around_it`, `burning_corpse_burns_up`,
`legacy_magma_kill_when_temperature_off` (10 ticks, unchanged).
**Perf.** burning units list tiny; fire pass checks units on burning tiles: one pass over
alive units (`ai`) per FIRE_PERIOD **only while `cells` or burning lists are non-empty**,
O(units * log cells).

### 3.9 Magma contact: items, buildings, mist

**DF rule.**
- Magma "will burn anything flammable it touches - however, any item set alight by this
  method will likely just be doused in magma until it is completely destroyed" (w:Fire).
- "Typical wood will disappear on contact with magma"; copper melts "very short while",
  zinc faster (w:Magma, minecart trick). With 3.3: wood ignites after 120 ticks.
- Buildings: constructions safe; non-construction buildings in contact with magma must be
  entirely magma-safe; non-magma-safe components "will eventually melt and the building will
  deconstruct"; closed doors/floodgates/raised bridges act as walls and are safe unless
  opened/submerged; pumps with non-safe parts are destroyed after prolonged operation
  (w:Magma, w:Magma-safe).
- Dropping large items into magma or cave-in debris creates **magma mist**, which "sets fire
  to all non-fire-resistant creatures that share a tile with it"; not created by falling
  magma (w:Magma mist). A single troll corpse makes a small cloud; mass dumping 3-4 z high.
- Minecarts/wheelbarrows of non-magma-safe material wear and disintegrate when hauling magma.
**Rustfort.** instant destruction of loose items on new magma cells (`fluids_melt_items`).
**Gap.** gradual heating; buildings; mist.
**Design.**
- Temperature on: `fluids_melt_items` hands its cells to fire15
  (`if self.fire15_on() { self.fire15_magma_cells(&cells, now); } else { legacy }`, marked).
  fire15 puts every loose item, container content and building component on those tiles
  into `hot` (env 12000). Additionally a periodic scan (`now % 100 == 29`, only if
  `fluids.stats.magma_volume > 0`) puts loose items lying in magma into `hot` (catches items
  dumped/dropped into existing magma). Items leave magma -> env falls back.
- Buildings: `OCC_BUILDING` tiles that get magma (from the same cells) -> look up the
  building (`world.buildings`, linear find by tile; few events) and decide:
  door/floodgate/hatch closed, or raised bridge with magma not on its footprint -> ignore;
  else components enter `hot`. When any component is destroyed (melted/burned up), the
  building is removed: call `self.remove_building(id)` (build_api.rs ~213, already cleans
  jobs, beds and items) then `destroy_item` on the destroyed component, announce
  "<building> has melted" (ESTIMATE wording, AnnouncementKind::Warning). Pattern:
  `defense.rs::destroy_building` (~1718).
- Magma mist: **P3, MISSING** for now. Design sketch: when an item of volume >= some
  threshold (corpse of size >= 30 000 cm3, ESTIMATE) falls into magma (the periodic scan sees
  a *new* item in magma), spawn `mist` cells (sorted vec of (tile, ttl)) in a 3x3x2 cloud
  above for 50 ticks (ESTIMATE); units on mist cells ignite like in 3.8.
**Tests.** `wooden_bed_in_magma_burns_up_after_ignition` (> 120 ticks, < 1 day),
`iron_door_survives_magma`, `opened_wooden_floodgate_burns`, `closed_wooden_floodgate_holds`
(ESTIMATE per wiki: holds indefinitely), `copper_item_melts_slowly_in_magma` (~1 428 ticks).

### 3.10 Obsidian formation

**DF rule.**
- Any tile containing both water and magma becomes **unmined obsidian** (a natural wall),
  generating abundant steam; both fluids disappear from that tile (w:Obsidian, w:Obsidian
  farming). Any amount of magma (even 1/7) gives a whole wall.
- The material is one of the inorganics with `[LAVA]`, chosen randomly **per biome** at world
  generation (w:Magma footnote) - normally obsidian.
- **Exception: magma flow tiles** (floor above semi-molten rock): no obsidian; water and
  magma vanish "in a puff of steam" (w:Obsidian, w:Magma flow, w:Semi-molten rock). Also
  "magma located above semi-molten rock will disappear when mixed with water" (w:Magma).
  A construction on a magma-flow tile *does* turn into obsidian when both meet (SMR page).
- Semi-molten rock itself never reacts (w:Semi-molten rock).
- Pond-zone dumping: water must free-fall at least one z-level onto magma, else only steam
  (w:Obsidian) - Rustfort has no pond zones: **MISSING / n.a.**
- Items in the tile: non-magma-safe objects that were in the magma are mostly destroyed;
  magma-safe items are entombed and recovered by mining; magma-unsafe items that had little
  exposure (e.g. were in the water) may survive; burning objects can stay on fire inside.
- **Creatures cast in obsidian die**, even without significant exposure; creatures in cages
  survive the casting (may still die of the heat).
- Formation does not change the light level.
**Rustfort.** `Fluids::obsidian` handles the wall; no unit/item handling, no magma-flow
exception, material fixed to `INORGANIC:OBSIDIAN`.
**Gap.** edge cases.
**Design.** In `fluids.rs` (marked `// [fire15]`):
- `Fluids` gets `pub obsidian_cells: Vec<u32>` (`#[serde(default)]`; cleared by fire15 every
  tick it reads them) pushed in `obsidian()`.
- In `obsidian()`: if the tile's tiletype material is `TtMaterial::Magma` (MagmaFlow floor)
  and the tile is not a construction: only clear the liquid (`self.set(tiles, i, 0, false)`),
  count `stats.steam += 1`, do not change `tt`. (The flow tile keeps refilling from below
  via sources if any.) This needs the tiletype material class: precompute a `MAGMA_FLOW`
  bit in `class_of` (new bit 128 in the u8 class, unused today: HOLD 1 .. FLOOR_SHAPE 64).
- fire15 (each tick when `obsidian_cells` non-empty, independent of the temperature setting,
  because it is a fluid rule): for every unit standing on such a tile: if not caged
  (`UnitFlags::caged`) -> `combat_kill(s, now, "has been encased in obsidian")` (ESTIMATE
  wording). Items on the tile: destroy those that are not magma-safe *and* were in magma
  (simplification: destroy non-magma-safe, ESTIMATE); keep magma-safe items (they stay at
  the tile position; mining the wall makes them reachable again - verify that the mining
  path does not delete items in a wall tile; `item_goals` already uses adjacent tiles).
- Obsidian material: keep `INORGANIC:OBSIDIAN` (OK for vanilla; the per-biome LAVA choice
  needs world data we do not export: MISSING, harmless).
**Tests.** existing fluids tests stay; add `water_on_magma_flow_makes_steam_not_obsidian`,
`dwarf_in_casting_tile_dies`, `caged_creature_survives_casting`,
`iron_bar_is_entombed_wood_is_destroyed`.
**Hash.** These run with temperature off too. They change the reference bench hash only if
obsidian forms on a magma flow or on a unit/item tile there; report it.

### 3.11 Steam

**DF rule.** Steam = boiled water; produced when water and magma mix (with obsidian), rain on
magma pools, magma boiling water that covers something magma-safe, very hot items/creatures
submerged in water. **Steam is visibly harmless** in the current version (w:Steam).
**Rustfort.** "water becomes steam (not simulated further)". **Gap.** cosmetic only.
**Design.** Counter `rep.steam_puffs`; optional short-lived cosmetic cells (same structure as
smoke, kind STEAM, ttl 100 ticks ESTIMATE) for the renderer. No gameplay effect. Hot items
(>10180) lying in water evaporate 1/7 water per 100 ticks (ESTIMATE) - P3.

### 3.12 Smoke

**DF rule.** Burning items release smoke into the surrounding area; smoke crosses z-levels up
or down through openings like miasma; inhaling smoke gives an unhappy thought but no other
harm; smoke takes "approximately 7 seconds" to dissipate after its source is gone; burning
artifacts make smoke forever; units do not path reliably through smoke (w:Smoke, w:Fire).
DF thought caption "after choking on smoke underground" (`social/captions.rs:69`).
**Rustfort.** none (caption only). **Gap.** smoke + thought.
**Design.** Copy the miasma pattern (`lifestyle.rs` `miasma`, `next_miasma`, thought at most
every 2 days, `feel(s, "Miasma", Class::NegMild, 80, -1, 0, now)`):
- `Fire15::smoke: Vec<(u32 tile, u16 ttl)>` sorted; every fire pass each burning cell/item/
  unit refreshes smoke on its tile and spreads one step to orthogonal open neighbours and
  the open tile above (`Shape` floorless/open) with ttl `SMOKE_TTL = 700` ticks (ESTIMATE:
  "7 seconds" at DF's 100 FPS cap), at most radius 2 from a source (as miasma).
- Thought pass (`now % 100 == 83`): units on a smoke tile that is **not OUTSIDE** ->
  `feel(s, "Smoke", Class::NegMild, 80, -1, 0, now)` at most every 2 days per unit (use a
  `next_smoke: Vec<u64>` per slot, `#[serde(default)]`).
- Pathing through smoke: MISSING (DF behaviour is a bug-like quirk).
**Tests.** `smoke_thought_underground_only`, `smoke_dissipates_after_source_ends`.

### 3.13 Water, rain and snow against fire

**DF rule.** Water extinguishes burning items not stored in bins (w:Fire Artifacts); fire
does not spread over water; an area enclosed by water is a firebreak (w:Fire). Rain/snow
effect on fire: not on the wiki (ESTIMATE).
**Design.** Fire pass: a burning cell whose tile has water depth >= 1 goes out (cell removed,
grass stays unburnt if fuel remains? ESTIMATE: becomes ashes if more than half the fuel is
gone, else back to its old grass tiletype). Burning items/units in water depth >= 1 go out.
Outside tiles where `weather_at(p)` is Rain or Snow (weather13 API): each pass chance 1/4 to
go out (ESTIMATE). If `weather_at` is still the stub (always Clear), no effect.
**Tests.** `rain_puts_out_outside_fire` (uses `force_weather`), `fire_does_not_cross_water`.

### 3.14 Magma workshops and warm stone

**DF rule.**
- Magma forge/smelter/glass furnace/kiln: need magma >= 4/7 directly below one of the 8
  non-centre tiles; "if the magma depth falls below 4/7, even for a moment, the forge will
  immediately cancel all of its jobs"; a moody dwarf who claimed it goes insane at once;
  magma is not consumed (w:Magma forge, w:Magma "Using magma").
- Warm stone: tiles next to magma are 10075; revealing a designated hidden tile that is warm
  cancels its dig designation with a "warm stone" warning (w:Magma), like damp stone.
**Rustfort.** `magma_shop_ok` gates new jobs only (FIDELITY line 102 notes the gap).
**Gap.** job cancellation; warm-stone cancel.
**Design.**
- `Fire15::magma_shops: Vec<(i32 building id, [u32; 8] below tiles)>` cache (`#[serde(skip)]`,
  rebuilt at init and when buildings change: compare `world.buildings.len()`/a building epoch
  if one exists). Check every tick in which `fluids` moved anything (`fluids.stats.updates`
  changed) — cheap: <= 8 tiles per shop. On a drop below 4/7: cancel the shop's jobs
  (`cancel_jobs_at(id)` used by `remove_building`) and log. Mood insanity hook: if
  `moods` exposes "claimed workshop" state, call its fail path (marked); else MISSING.
  This is temperature-independent (DF rule) -> runs always; **hash impact** only for forts with
  magma shops (reference fort has none: verified `buildings.jsonl` has no `Magma*` subtype).
- Warm stone: in `lib.rs::reveal` (~2644) add a marked call
  `self.fire15_revealed(q)` for each newly revealed tile: if a dig designation sits on it
  and any 6-neighbour (or 8+2?) has magma -> clear the designation and announce "warm stone"
  (ESTIMATE: 4 orthogonal + up/down, as DF's damp stone; check what Rustfort does for damp:
  currently nothing -> implement both in one hook only if cheap; damp belongs to fluids, so
  only warm here). P3.
**Tests.** `magma_forge_cancels_jobs_when_magma_drops`, `warm_stone_cancels_dig_on_reveal`.

### 3.15 Magma and ice

**DF rule.** "Running magma under unmined ice will melt it"; magma melts ice beyond the
"warm" wall only when magma moves in (later the reservoir may freeze again); very hot
creatures (magma crabs on a glacier, magma men) melt unmined ice around them; natural ice
walls/floors melt, ice constructions never do; ice walls thaw into 7/7 water; ice floors
yield the water they froze from (smoothed ice floor -> 2/7) (w:Ice, w:Magma).
**Owner.** Freezing/thawing by weather is `[weather13]`. Melting **by heat sources** is fire15.
**Design.** When magma arrives in a tile (fluids `magma_new` / fire15 magma cells), check the
6 neighbours: natural `TtMaterial::FrozenLiquid` wall -> set to open/empty + water 7/7
(`Sim::set_liquid(p, 7, false)` public API in fluids, ~1460); frozen floor -> floor of the
material below + water 1/7 (ESTIMATE: DF stores the original depth; we do not). Heat
emitters adjacent to natural ice do the same each fire pass. The resulting water meets the
magma -> obsidian by the normal fluid rule (this is DF's behaviour too). If weather13 adds a
`thaw_tile(i)` helper, call it instead (coordinate; until then local helper in fire15 with a
TODO).
**Tests.** `magma_next_to_ice_wall_melts_it` (synthetic, temperature on).

### 3.16 Cold damage (COLDDAM), frostbite

**DF rule.** Items below COLDDAM wear faster (wood/cloth in a glacier depot; walls do not
halt it, w:Wear). Creatures whose fat drops below 9900 take cold damage and die
(w:Temperature "lowest survivable"). HOMEOTHERM "provides minor protection".
**Owner.** Needs the ambient cold from weather13. **Design (P3):** a daily lazy pass
(`now % 1200 == 643`) only when the minimum outside temperature this day (`temperature_at`)
< 9900: loose items on OUTSIDE tiles with colddam > ambient get `+ 10 * (colddam - T) * 1200`
wear points (ESTIMATE, daily batch of the per-tick rule); units outside get frostbite wounds
(`wf::FROSTBITE = 512`) on extremities at 1/10 chance per day (ESTIMATE). Off in temperate
forts automatically. Mark the whole subsection ESTIMATE in FIDELITY.

### 3.17 Fire attacks (API for others): dragonfire, fireballs, fire jets, magma crab spit

**DF rule.** Dragonfire: cone ~20 tiles long, ~10 wide, up to 2 z-levels; up to 50000 U;
ignites organics including nether-cap; melts and boils away non-flammable stone, glass,
ceramics; no effect on water; affects buildings (not constructions) on its tiles; closed doors
immune; burned objects then burn at their normal temperature; shields block it > 99 %; only
dragons, cave dragons and some deep monsters are immune (FIREIMMUNE_SUPER) (w:Dragonfire).
Fire imps: HURL_FIREBALL / SPRAY_FIRE_JET (raws `MATERIAL_EMISSION`), ignite landscapes and
units; take damage only at 15000 (w:Fire imp). Magma crab: spits small globs of liquid basalt
that burn the hit part and melt its fat (w:Magma crab).
**Owners.** Dragonfire: `[mega13]`. Fire imps/magma crabs: cavern creatures (no round-13
owner) -> P3 here, after the API exists.
**Design.** `fire15_api.rs`:
```rust
impl Sim {
    /// [fire15] Set a flammable tile on fire (no-op if temperature is off or not flammable).
    pub fn ignite_tile(&mut self, p: Pos) -> bool;
    /// [fire15] Set body parts of a unit on fire (all external parts if `parts == 0`).
    pub fn ignite_unit(&mut self, unit_id: i32, parts: u64) -> bool;
    /// [fire15] Expose everything on tile `p` to `temp` for one tick (dragonfire = 50_000):
    /// items get T = max(T, temp) and run their thresholds (melt/boil/ignite), units ignite
    /// unless immune at that temperature or FIREIMMUNE_SUPER, grass turns to ashes at once.
    pub fn heat_blast(&mut self, p: Pos, temp: i32);
    pub fn is_burning_tile(&self, p: Pos) -> bool;
    pub fn is_burning_unit(&self, unit_id: i32) -> bool;
    pub fn item_temperature(&self, item_id: i32) -> Option<i32>;
    pub fn is_fire_safe(&self, mat: u16) -> bool;
    pub fn is_magma_safe(&self, mat: u16) -> bool;
    pub fn fire_report(&self) -> FireReport;
    pub fn set_temperature_enabled(&mut self, on: bool);   // player/test toggle
    pub fn temperature_enabled(&self) -> bool;
    pub fn fire_cells(&self, out: &mut Vec<(Pos, u8)>);   // renderer overlay
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FireReport {
    pub tiles_ignited: u64, pub tiles_burnt: u64, pub items_ignited: u64,
    pub items_burned_up: u64, pub items_melted: u64, pub items_boiled: u64,
    pub units_ignited: u64, pub units_burned_to_death: u64, pub burn_wounds: u64,
    pub extinguished: u64, pub steam_puffs: u64, pub smoke_thoughts: u64,
    pub obsidian_kills: u64, pub buildings_melted: u64, pub magma_jobs_cancelled: u64,
    pub hot_items_max: u32, pub cells_max: u32, pub time_s: f64,
}
```
mega13 calls `heat_blast` along its cone; fire15 does not depend on mega13.
Shield block for dragonfire is combat's/mega13's business. Tests: `heat_blast_melts_granite_
boulder_and_ignites_nether_cap`, `fireimmune_super_survives_heat_blast`.

### 3.18 Temperature setting off (the reference fort)

**DF rule.** With temperature calculations disabled, "effects such as water freezing,
melting, or evaporating, or creatures and items taking temperature-related damage will not
occur"; dwarves then refuse to walk on cast obsidian/former magma tiles (bug 8391)
(w:Temperature Bugs).
**Design.** `fn fire15_on(&self) -> bool { self.fire.enabled.unwrap_or(self.world.settings.temperature) }`.
When off: no thermal model, no ignition, no smoke; the legacy fluids behaviour (10-tick magma
kill, instant item destruction in magma) stays **unchanged** (ESTIMATE: DF itself would not
damage them at all, but we keep the existing behaviour so the reference bench hash and the
obsidian/magma tests stay as they are; document it). Temperature-independent fluid rules
(3.10 obsidian edge cases, 3.14 magma-shop cancel, 3.5 magma-safe fix used by the legacy
path) run in both modes.
Observation for the fluids owner (do **not** change in fire15): per the wiki, evaporation does
not happen with temperature off, yet `fluids_evaporate` runs in the reference fort. Changing
it would alter the bench hash; propose a separate validation against the r-series exports.
Bug 8391 is not reproduced (intentionally).

---------------------------------------------------------------------------------------------

## 4. Module design summary

### 4.1 Files

| File | Content |
|---|---|
| `crates/rf-core/src/entities.rs` | `MaterialHeat`, `MaterialInfo::heat` (`// [fire15]`) |
| `crates/rf-sim/src/fire15.rs` | `Fire15` state, `fire15_init`, `fire_tick`, `fire15_hash`, gating, after-load rebuild |
| `crates/rf-sim/src/fire15/heat.rs` | constants, `Temp`, `heat_of` fallback, `stable_at`, `fire_safe`, `magma_safe` |
| `crates/rf-sim/src/fire15/items.rs` | hot items, thresholds, wear, buildings |
| `crates/rf-sim/src/fire15/tiles.rs` | fire cells, spread, ashes, trees, smoke, steam |
| `crates/rf-sim/src/fire15/burn.rs` | burning units, burn wounds, magma contact, emitters |
| `crates/rf-sim/src/fire15/tests.rs` | unit tests (synthetic worlds with `settings.temperature = true`) |
| `crates/rf-sim/src/fire15_api.rs` | public API + `FireReport` |
| `crates/rf-sim/tests/fire15.rs` | integration tests incl. export-based ones (skip without `RUSTFORT_EXPORT`) |
| `crates/rf-sim/examples/fire15.rs` | demo: grass fire on a synthetic surface, magma dump, obsidian casting; prints `FireReport` |
| `crates/rf-app/src/ui/fire15.rs` | burning overlay (Fire tiletype already renders; units flash red/yellow; `!!name!!` in unit/item lists), temperature toggle in settings, item temperature in the item sheet |

### 4.2 State

```rust
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Fire15 {
    pub enabled: Option<bool>,           // player toggle; None = World::settings.temperature
    pub hot: Vec<HotItem>,               // sorted by item id
    pub cells: Vec<FireCell>,            // sorted by tile
    pub burning: Vec<BurningUnit>,       // sorted by slot
    pub smoke: Vec<(u32, u16)>,          // sorted by tile
    pub next_smoke: Vec<u64>,            // per slot
    pub rep: crate::fire15_api::FireReport,
    #[serde(skip)] pub ready: bool,
    #[serde(skip)] pub heat: Vec<MaterialHeat>,      // per palette index (fallback applied)
    #[serde(skip)] pub bodies: Vec<BurnBody>,        // per (race, caste) used on the map
    #[serde(skip)] pub caste_of: Vec<u16>,           // per slot -> bodies index
    #[serde(skip)] pub emitters: Vec<u32>,           // slots of fixed-temp creatures
    #[serde(skip)] pub magma_shops: Vec<(i32, [u32; 8])>,
    #[serde(skip)] pub tt: FireTiletypes,            // resolved tiletype ids (Fire, Ashes1-3, Burning*, TreeDead*)
    #[serde(skip)] pub scratch: Vec<u32>,
}
```
`Sim` gets `fire: fire15::Fire15, // [fire15]` (lib.rs struct + `Sim::new` init, marked).
Persistence: the whole `Sim` is the exact snapshot (`saves11.rs`); add
`("fire15", enc(&self.fire)), // [fire15]` to `exact_parts` (size listing) and
`v.push(("fire15", fnv(|mix| self.fire15_hash(mix)))); // [fire15]` to the part hashes in
`persist.rs` (~214), and `self.fire15_hash(&mut mix); // [fire15]` in `state_hash`
(lib.rs ~1308). `after_exact_load` (saves11.rs ~438, currently empty) gets
`self.fire15_after_load(); // [fire15]` which sets `ready = false` so caches rebuild lazily.
`fire15_hash` mixes nothing while all vectors are empty and the report is zero (keeps the
reference hash).

### 4.3 Tick hook

In `tick_inner` right after `self.carts_tick(now);` (inside the phase[7] liquids block, after
fluids so magma/obsidian events of this tick are seen):
```rust
self.fire_tick(now); // [fire15] temperature, fire, magma contact, obsidian casting
```
`fire_tick`:
1. `if !self.fluids.obsidian_cells.is_empty() { self.fire15_obsidian(now) }` (always).
2. magma shops check if fluids changed this tick (always, cheap, early-out with no shops).
3. `if !self.fire15_on() { return }` — **one branch per tick when off**.
4. lazy init (`ready`).
5. every tick, if `hot` non-empty: thermal step + effects (3.3/3.4/3.6).
6. `now % FIRE_PERIOD(10) == FIRE_PHASE(4)` and (`cells`/`burning`/`hot`/`emitters`
   non-empty): fire pass (3.7, 3.8, 3.12, 3.13, 3.15).
7. `now % 100 == 29` and magma on the map: scan items lying in magma (3.9).
8. `now % 100 == 83` and smoke non-empty: smoke thoughts.
9. `now % 1200 == 643`: cold pass (P3) if ambient < 9900.
10. `rep.time_s` accumulation like `fluids.stats.time_s`.
Order inside a pass: fixed (tiles ascending, items by id, units by slot); new ignitions are
collected into `scratch` and merged after the loop (no effect of iteration order inside a
pass beyond the sorted order). RNG: `Rng::for_unit(self.seed, key, now, SALT_FIRE15)` with
`key` = tile index or slot (`rng.rs::for_unit`); never thread-dependent. No rayon.

### 4.4 Perf budget

- Temperature off (reference bench): one load + compare per tick, plus the obsidian/shop
  checks which early-out on empty vectors -> expected cost < 1 ns/tick; the bench
  (`rustfort bench --ticks 806400 --standdown`) must stay within noise and keep its hash.
- Temperature on, no fire: init builds `heat` (2 101 entries) and bodies once; passes early
  out. The item-in-magma scan every 100 ticks is O(items) only when the map has magma
  (`fluids.stats.magma_volume > 0`): ~50k items -> ~50 us / 100 ticks = 0.5 us/tick. If that
  shows in profiles, replace with a hook in `set_item_pos`/`create_item` (marked) that tests
  the tile's liquid byte.
- Fire on: allocation-free passes (reuse `scratch`), sorted vectors with binary search.
- Measure: `cargo run --release -p rf-sim --example fire15` prints ticks/s for a 64x64
  grass fire and a 200-item magma dump.

### 4.5 Determinism and saves

- No HashMap iteration (only `item_idx.get`, `unit_slot.get` lookups).
- All state in `Fire15` serializable; caches `#[serde(skip)]` rebuilt from world + palette.
- `saveload` example must report IDENTICAL; add a test that saves mid-fire and continues
  (`fire_state_survives_exact_save` in `tests/fire15.rs`, pattern of `tests/saves11.rs`).

### 4.6 FIDELITY.md

New section at the end: `## Temperature, fire and magma ([fire15])` with one row per rule of
section 3 (OK / ESTIMATE / MISSING and the wiki page), and the old rows 225/398/510
superseded by a pointer (do not edit other sections; add a line in the new section stating
which old rows it supersedes).

---------------------------------------------------------------------------------------------

## 5. Tests to write (checklist)

Unit (`fire15/tests.rs`, synthetic world 32x32x4, `settings.temperature = true`):
1. `lignite_in_magma_ignites_after_517_ticks` (exact).
2. `fixed_temp_material_never_changes` (nether-cap).
3. `granite_is_not_magma_safe` / `iron_is` / wiki fire-safe list.
4. `burning_item_without_heatdam_lasts_322560_ticks` (exact 322 560).
5. `burning_artifact_never_disappears`, `artifact_melts_in_magma_if_meltable`.
6. `grass_fire_spreads_on_one_level_only`, `grassy_ramp_does_not_burn`,
   `construction_floor_never_burns`, `burnt_grass_becomes_ashes`, `tree_burns_to_dead_tree`.
7. `water_puts_out_burning_log`, `fire_does_not_cross_water`, `rain_puts_out_outside_fire`.
8. `dwarf_on_grass_fire_catches_fire_eventually`, `burn_wounds_go_skin_then_fat_then_muscle`,
   `melted_fat_bleeds`, `dwarf_walking_into_water_is_extinguished`.
9. `magma_crab_is_magma_proof_by_materials`, `fireimmune_flag_alone_does_not_protect`.
10. `fire_man_ignites_grass_around_it`.
11. `wooden_bed_in_magma_burns_up`, `iron_door_survives_magma`, `opened_wooden_floodgate_burns`.
12. `water_on_magma_flow_makes_steam_not_obsidian`, `dwarf_in_casting_tile_dies`,
    `caged_creature_survives_casting`, `iron_bar_is_entombed_wood_is_destroyed`.
13. `magma_forge_cancels_jobs_when_magma_drops`.
14. `smoke_thought_underground_only`.
15. `magma_next_to_ice_wall_melts_it`.
16. `legacy_magma_kill_when_temperature_off` (MAGMA_BURN_TICKS = 10, unchanged).
17. `temperature_off_changes_nothing` (run 10k ticks with a grass fire forced via API -> API
    returns false, hash equals a run without the call).
18. Determinism: a 20k-tick grass+magma scenario with RAYON 1 and N threads -> same hash
    (add to `tests/determinism_stress.rs` style or own test using `Sim::state_hash`).
Integration (`tests/fire15.rs`): export-based: load Tileshtulon, enable temperature via API,
ignite a surface grass tile near the wagon area, run 5 000 ticks: no panic, cells > 0 then 0,
report sane; save/load mid-fire identical.
rf-import: `material_heat_is_imported` (iron melting 12768).

---------------------------------------------------------------------------------------------

## 6. Prioritized implementation list

**P1 (core, needed for anything else)**
1. rf-core `MaterialHeat` + `MaterialInfo::heat`; rf-import test. (small)
2. `fire15/heat.rs`: constants, `Temp::step`, `heat_of` fallback, `fire_safe`/`magma_safe`;
   switch `fluids::magma_safe` to it. Test 1-3.
3. `Fire15` skeleton, Sim field, tick hook, hash/save registration, `fire15_api` toggle +
   report, gating on `settings.temperature`. Verify bench hash unchanged and saveload
   IDENTICAL.
4. Hot items: magma contact (`fluids_melt_items` hand-off), thresholds (melt/boil/ignite),
   wear points (10/tick burning, heatdam ESTIMATE), artifacts, containers. Tests 4, 5, 11.
5. Obsidian edge cases (magma flow -> steam; units cast die; caged survive; items) - runs in
   both modes; report hash effect. Test 12.

**P2 (fire)**
6. Tile fires: grass/shrub/tree cells, spread same-z, ramps/constructions excluded, ashes and
   dead trees, traffic avoidance, water/rain extinguishing. Tests 6, 7.
7. Burning units: ignition chances, burn/melt wounds (`wf::BURN`, `wf::MELTED`), bleeding fat,
   charred parts, death, extinguishing, burning corpses; material-based magma immunity in
   `fluids_scan_units`/`fluids_units` when temperature is on. Tests 8, 9, 16.
8. Heat emitters (fixed-temp creatures). Test 10.
9. Smoke + "Smoke" thought; steam counter. Test 14.
10. Magma workshop job cancellation (always on). Test 13.
11. `heat_blast` / `ignite_*` API for mega13. FIDELITY section, demo example, UI overlay and
    temperature toggle.

**P3 (extras)**
12. Magma melts natural ice (coordinate weather13). Test 15.
13. Buildings melting/burning (doors, floodgates, bridges, pumps) with component items.
14. Warm-stone dig cancellation on reveal.
15. Magma mist; fire imp fireballs/jets and magma crab spit through the API (needs a combat
    interaction hook; cavern creature attacks).
16. Cold damage / frostbite (needs weather13's seasonal ambient).
17. Exporter: item `on_fire`/`temperature` (only if forts with live fires matter).

---------------------------------------------------------------------------------------------

## 7. ESTIMATE / MISSING index (for the FIDELITY section)

ESTIMATE: heat-damage and cold-damage wear rate (10 x degrees over/under per tick); burning
item equilibrium ignite+190; fuel durations (grass 200, shrub 300, tree tile 1 200 ticks);
fire temperature ramp +10/tick; spread chance 1/8 per 10 ticks; unit ignition chances (1/2 per
10 ticks on fire, 1/10 next to a burning unit); burn severity per pass (1 000 / 5 000 in magma);
spontaneous extinguish 1/30; rain/snow extinguish 1/4 per pass; water depth >= 1 extinguishes;
units in magma ignite at once; smoke ttl 700 ticks / radius 2; underground ambient fixed 10015;
truncation toward zero in `Temp::step`; burnt tree -> `TreeDead*` tiletypes; dead and dry grass
burn; frozen floor melts to 1/7; casting destroys every non-magma-safe item; HOMEOTHERM 10067
when raws are unavailable; announcement wordings; legacy magma behaviour kept with temperature
off.
MISSING: tree collapse from fire; dead-tree rejuvenation; glob of molten metal items; per-biome
LAVA stone for obsidian; pond-zone free-fall rule; pathing quirks in smoke; magma mist (until
P3); fire attacks of cavern creatures (until P3); bug 8391.
