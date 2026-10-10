# Rustfort fidelity audit (round 6)

This file compares the rules Rustfort implements with the DF v50 wiki (dwarffortresswiki.org)
and with what DF does in Gordon's fort. It was written by two audit agents:

- **Economy & society**: production, industry, storage, social, population, trade,
  construct, fluids.
- **Combat & military**: combat, ranged, military, defense, invasion, health.

Each row gets one of these classes:

| class | meaning |
|---|---|
| OK | matches the wiki, the raws or observed DF behavior |
| FIXED | was WRONG or MISSING before round 6, fixed in round 6 (with a test) |
| WRONG | contradicts the wiki |
| MISSING | documented DF behavior that is absent |
| ESTIMATE | the wiki is silent; our own number |

Sources: "w:Page" is the v50 wiki page, "raws" is the DF install's vanilla raws (read only),
"export" is Gordon's live fort read with read-only DFHack.

# Economy & society

## Summary of round-6 fixes

| fix | before | after | evidence |
|---|---|---|---|
| Brewing uses the whole plant stack | 1 plant unit per job, 5 drinks | the whole free stack, 5 drinks and 1 seed per unit ("Plump helmet [5]" -> "Dwarven wine [25]") | raws `BREW_DRINK_FROM_PLANT`: the barrel has `DOES_NOT_DETERMINE_PRODUCT_AMOUNT`, so the plant decides the amount; w:Brewing. Test `brewing_uses_the_whole_plant_stack_and_returns_its_seeds` |
| Fruit brewing returns seeds | no seed | 1 seed per fruit unit | raws `BREW_DRINK_FROM_PLANT_GROWTH` has a `SEEDS` product |
| Room value tiers | 1 / 100 / 250 / 500 / 1000 / 1500 / 2500, one tier too high (a room worth 100 was "ordinary", 2500 "royal") | 100 modest, 250 quarters, 500 decent, 1000 fine, 1500 great, 2500 grand, 10 000 royal | w:Room, w:Bedroom |
| Room value contents | furniture only | furniture + floor tiles + bordering walls + engravings | w:Room, w:Engraving. Test `room_value_counts_floor_walls_and_engravings` |
| Melt jobs | `ItemFlags::melt` was set but nothing melted | "Melt a metal object" at the smelter, 1 fuel, yields in tenths of a bar per smelter and metal | w:Melt item. Tests `melting_returns_tenths_of_bars_per_wiki`, `melt_yields_follow_the_wiki_table` |
| Carve fortification and track designations | lost on save/load | kept in `World::carve_designations` | test `carve_designations_survive_save_and_load` |
| Engraving constructed walls/floors | refused | allowed (constructions are already smooth) | w:Engraving. Test `wall_from_boulder_blocks_pathing` |
| Caravan stay | 16 days (19 200 ticks) after unloading, warning 3 days before | 30 000-40 000 ticks, warning 4 days before | w:Caravan |

**Room value evidence on the real fort** (`cargo run -p rf-sim --release --example rooms`):
the 83 owned bedrooms and the dining hall now score 119-503 by furniture and tiles alone
(furniture alone was 34-408). DF's own thoughts rate most of them "great" (BedroomQuality
severity 3), and read-only DFHack shows why: DF holds **16 233 engravings** in
`world.event.engravings`, 16-19 around each sampled bedroom (e.g. SZ A01: 16 engravings of
quality 0-4). The exporter does not export them yet, so Rustfort keeps the room level DF
reported as a floor (`min_tier`). Once the exporter writes the engravings into
`World::engravings`, the new room value counts them automatically.

## production.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Brew drink: inputs | 1 unrotten brewable plant **stack** + 1 empty barrel or large pot | same (raws) | FIXED |
| Brew drink: outputs | 5 drinks per plant unit into the container, 1 seed per plant unit | same (raws, w:Brewing) | FIXED |
| Brew from fruit | 5 drinks per fruit unit + seed | same (raws) | FIXED |
| Brewer skill and drink amount | no effect | "Regardless of skill, a brewer produces five units" (w:Brewing) | OK |
| Easy / fine / lavish meal | 2 / 3 / 4 ingredient stacks | same (w:Kitchen) | OK |
| Meal stack size | sum of ingredient stacks | same (w:Kitchen) | OK |
| Meal name | material of the last ingredient + biscuits/stew/roast | same (w:Kitchen) | OK |
| Meal quality from cook skill | meals are made at quality 0 | each ingredient gets a quality from the cook's skill (w:Kitchen) | MISSING |
| Cooking plants keeps no seeds | yes | yes (w:Kitchen) | OK |
| Brewables are not cooked | default off, switchable | DF cooks them unless the kitchen settings forbid it; Gordon's rule | ESTIMATE (player setting) |
| Seasonal crops | grow only in their raw seasons; destroyed when a season starts in which they cannot grow | same (w:Farming) | OK |
| Growth time | plant `GROWDUR` x 100 ticks | same (raws, w:Farming) | OK |
| Harvest yield | 1 + 50 % on skill-in-5/10/15/20/75 checks, planter skill at planting | same (w:Farming) | OK |
| Fertilizer (potash) | not simulated | raises yield (w:Farming) | MISSING |
| "Poor" underground soil -75 % | yes | not on the wiki | ESTIMATE |
| Seed cap | 200 seeds per species | 200 (w:Seed) | OK |
| Seeds from eating raw plants | yes | yes (w:Seed) | OK |
| Workshop queue | at most 10 jobs | 10 (w:Workshop) | OK |
| Work orders | checked daily, conditions on stock, `amount_left`, repeat per period | same model (w:Manager) | OK |
| Work times | 100-600 ticks, scaled by skill | not documented | ESTIMATE |
| Well | water of 3/7 or more directly below | (w:Well) | OK |

## industry.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Charcoal / ash | 1 log -> 1 bar | same (w:Wood furnace) | OK |
| Smelt ore | 1 ore + 1 fuel -> 4 bars per metal, by METAL_ORE probability | same (w:Ore, raws) | OK |
| Magma furnaces | no fuel | same (w:Smelter) | OK |
| Forge bars | MATERIAL_SIZE / 3, at least 1 | same (w:Metalsmith's forge) | OK |
| Gloves, boots | pairs (2 items) | same | OK |
| Bolts | 25 per bar | same (w:Metalsmith's forge) | OK |
| Melt a metal object | 1 designated item + 1 fuel -> tenths of bars: 0.3 x MATERIAL_SIZE (breastplate 2.7, greaves 1.8, leggings 1.5, pick 1.2, crossbow 0.9, helm 0.6, cap 0.3), furniture 1.0, blocks 0.5, goblet 0.2, 25 bolts 0.3 (single 0.1), 500 coins 1.1 (single 0.1) | same table (w:Melt item) | FIXED |
| Melt fractions | per smelter and metal, a bar at 10/10 | same (w:Melt item) | FIXED |
| Melt fractions in saves | lost on load (sim state) | kept by the smelter; lost when it is destroyed (w:Melt item) | MISSING |
| Melting crafts | 0.3 bar | not on the wiki | ESTIMATE |
| Raw green glass | 1 bag of sand + 1 fuel, bag kept | same (w:Glass furnace) | OK |
| Mill plants | 1 plant + 1 empty bag -> flour in the bag | same (w:Quern) | OK |
| Butcher yields | meat = size / 10 000, fat / 40 000, bones / 20 000 + skull + skin | per body part tissues (w:Butcher's shop) | ESTIMATE |
| Tan / leather / clothier | 1 hide -> 1 leather; 1 leather or cloth per item | same (w:Leather works, w:Clothier's shop) | OK |
| Crafts | 1 boulder / log / bone / bar -> 1 craft of a random kind | same (w:Craftsdwarf's workshop) | OK |
| Fuel reservation | not reserved across queued jobs | DF reserves at job start | ESTIMATE |

## storage.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Barrel / pot food | 60 meals, plants or cheeses; 30 meat or fish | same (w:Barrel) | OK |
| Barrel drink | one drink stack of any size | "only a single stack" (w:Barrel) | OK |
| Large pot | same capacity as a barrel | same (w:Barrel) | OK |
| Bins | for ammo, armor, bars/blocks, cloth, coins, finished goods, gems, leather, weapons | same (w:Bin) | OK |
| Bin volume | 60 000 cm³, item volumes calibrated on the real fort | not given on the wiki | ESTIMATE |
| Bags | seeds (100 per bag) and powder | seeds and flour go into bags (w:Seed) | ESTIMATE (count) |
| Stockpile container limits | `max_bins` / `max_barrels` empty containers brought | same (w:Stockpile) | OK |
| Rot protection | stockpile, carried, in a job; containers do not protect outside piles | same (w:Rot) | OK |
| Raw fish | rots everywhere | same (w:Rot) | OK |
| Never rot | seeds, drinks, flour, fat, milk | same (w:Rot) | OK |
| Plants wither, no miasma | plants wither | same (w:Rot) | OK |
| Rot timer | 28 days, rotted away after 28 more | not given on the wiki | ESTIMATE |
| Miasma | not simulated | rotting corpses and food make miasma, which spreads orthogonally (w:Rot) | MISSING |
| Wheelbarrows | not simulated (Gordon's piles use 0) | (w:Wheelbarrow) | MISSING |
| Trade depot protects food | not checked | yes (w:Rot) | MISSING |

## social.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Stress buckets | 50 000 / 25 000 / 10 000 / -10 000 / -25 000 / -50 000 | DFHack `stress_cutoffs` | OK |
| Long-term stress | +20 160 / -43 564 per year | w:Stress | OK |
| Emotion dividers | DF divider table, capped push into outer buckets | w:Emotion | OK |
| Breakdown kind by facet | anger -> tantrum, depression, anxiety -> oblivious | w:Stress | OK |
| Breakdown thresholds and duration | own numbers | not given | ESTIMATE |
| Thought strengths | 50-600 | not given | ESTIMATE |
| Need decay | `need_level` per 100 ticks, satisfied = 400 | fits w:Need ("every 3 months at level 10") | OK |
| Focus and skill | +-50 % effective skill | w:Need | OK |
| Room tiers | 0 meager, 100 modest, 250 quarters, 500 decent, 1 000 fine, 1 500 great, 2 500 grand, 10 000 royal | w:Room, w:Bedroom | FIXED |
| Room value | furniture + floors (rough 1x, smooth 4x, constructed 7x material value) + bordering walls (1x / 5x / 9x) + engravings (10 x material value x 1/2/3/4/5/12) | w:Room, w:Engraving | FIXED |
| Overlapping rooms | no penalty | -75 % for each room sharing floor (w:Room) | MISSING |
| Unenclosed rooms | no penalty | partial penalty on tile value (w:Room) | MISSING |
| Engravings of the live fort | not exported (16 233 in DF); the DF-reported room level is used as a floor | in `world.event.engravings` | MISSING (exporter) |
| Dining | chair next to a table in a dining zone -> DiningQuality; else LackTables / NoDining | w:Dining room | OK |
| Bedroom thoughts | own bedroom -> BedroomQuality; bed outside a room -> LackBedroom; floor -> SleptFloor... | w:Bedroom | OK |
| Temples, taverns, libraries | meeting zones recognized by name | DF locations | ESTIMATE |

## population.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Population caps | d_init POPULATION_CAP, STRICT_POPULATION_CAP (also stops births), BABY_CHILD_CAP | w:Population cap | OK |
| Wave timing | decided per season, arrive later in the season | once per season (w:Migrant) | OK |
| First two waves | not special | hardcoded, 1-10 dwarves, not in the first winter (w:Migrant) | MISSING (only matters for new forts) |
| Wave size | 2 + rand(min(28, 4 + wealth / 25 000)) | from created wealth (w:Migrant) | ESTIMATE |
| Wave chance | 3 in 4 per season | not given | ESTIMATE |
| Dangerousness | deaths do not deter migrants | many recent deaths deter migrants (w:Migrant) | MISSING |
| Pregnancy | married couples, 9 months, twins 1 in 50 | w:Pregnancy | ESTIMATE (rates) |
| Children | baby until 1, adult at 13 (observed, raws say 12) | raws `CHILD:12`; the export shows 12.x-year-olds as children | OK (observed) |
| Old age | MAXAGE 150-170 | raws | OK |
| Burial | corpse to a free coffin in a tomb zone | w:Burial | OK |
| Ghosts | unburied, not memorialized dead rise after 14 days (1 in 7 per day) | w:Ghost (timing not given) | ESTIMATE (timing) |
| Mayor | elected at 50 citizens, yearly | w:Mayor | OK |
| Mandates | 1 per noble, 1 in 3 per season, deadline 1 season | w:Mandate (rates not given) | ESTIMATE |

## trade.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Season by race | elves spring, humans summer, dwarves autumn | w:Caravan | OK |
| Stay at the depot | 30 000-40 000 ticks | w:Caravan | FIXED (was 16 days) |
| Leaving warning | 4 days before | w:Caravan | FIXED (was 3) |
| Wagons | need a 3-wide path from the edge to the depot, else pack animals; elves never use wagons | w:Caravan, w:Trade depot | OK |
| Wagons and traps or stairs | not checked separately | wagons cannot use stairs or cross traps (w:Caravan) | MISSING (check) |
| Stay extension per trade | +2 days, at most +8 | not given | ESTIMATE |
| Merchants stranded | give up after a month | insane after 25 days without leaving (w:Caravan) | ESTIMATE |
| Pack animal load | not limited | at most 100 items per pack animal (w:Caravan) | MISSING |
| Item value | base x material value x quality (1, 2, 3, 4, 5, 12, artifact 120); exported DF value when present | w:Value | OK |
| Appraisal | only changes displayed values | w:Broker | OK |
| Merchant acceptance | offer worth at least 110 % of the request | not given | ESTIMATE |
| Elves refuse wood | yes | yes (w:Elf) | OK |

## construct.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Construction materials | blocks, non-economic boulders, logs | blocks, boulders, logs, bars (w:Construction) | OK (bars MISSING) |
| Placement and support | walls etc. on floors or supported open space | w:Construction | OK |
| Construction keeps its item | `flags.construction`, given back on removal | export (DF 53) | OK |
| Smoothing | natural stone/mineral/obsidian/ice walls and floors, no soil, no constructions | w:Smoothing | OK |
| Engraving targets | smoothed natural stone and constructed walls/floors; removing a construction removes its engraving | constructed walls and floors can also be engraved (w:Engraving) | FIXED (was smoothed natural only) |
| Engraving value | 10 x material value x quality multiplier | w:Engraving | OK |
| Engraving quality | `min(5, (s + rand(0..=s)) / 3)` for skill s | "higher skill, higher quality" (w:Engraving) | ESTIMATE |
| Carve fortification | smoothed natural walls | w:Fortification | OK |
| Carve track | natural stone and constructed floors, directions from the drag | w:Track | OK |
| Carve designations survive saving | `World::carve_designations` | DF keeps designations in the save | FIXED |
| Traffic costs | high 1, normal 2, low 5, restricted 25 | DF defaults (w:Traffic) | OK |
| Dumping | dumped items lie forbidden in the dump | export | OK |
| Work times | construct 100-150, smooth 150, engrave 300, carve 150-250 ticks | not given | ESTIMATE |

## fluids.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Depth | 0-7 sevenths per tile, water or magma | w:Flow | OK |
| Falling | into the tile below if it is not a wall | w:Flow | OK |
| Spreading | 8 directions, also diagonally between walls, while 2/7 lower | w:Flow | OK |
| 1/7 flicker | not modelled | small random flow (w:Flow) | ESTIMATE |
| Pressure | water that falls onto full water rises up to one level below the source, orthogonally only | w:Pressure | OK |
| Magma pressure | none | none without pumps (w:Pressure) | OK |
| Evaporation | 1/7 water outside, 1/7 magma anywhere | w:Evaporation | OK |
| Mud | water leaving a floor leaves mud | w:Mud | OK |
| Aquifer | aquifer walls leak into open neighbours on their level | w:Aquifer | OK |
| Aquifer leak rate | light 1/10 per 100 ticks, heavy every 100 ticks | not given | ESTIMATE |
| Obsidian | water meeting magma -> obsidian wall | w:Obsidian | OK |
| Drowning | after `DROWN_TICKS` in 7/7 water | timing not given | ESTIMATE |
| Swimming skill | swimmers keep their heads above 4/7-6/7 water | w:Swimming | OK |
| Temperature, freezing, steam | not simulated (Gordon's world has temperature off) | w:Temperature | MISSING |
| Pumps, screw pumps | not simulated | w:Screw pump | MISSING |

## Known gaps worth doing next (economy & society)

1. Export `world.event.engravings` (16 233 in Gordon's fort) so room values match DF without
   the `min_tier` floor.
2. Meal quality from the cook's skill; fertilizer.
3. Miasma from rotting corpses and food.
4. Room overlap and unenclosed penalties.
5. Wagons and traps/stairs on the depot path; pack animal load limits.

# Combat & military

(Written by the combat audit agent.)
