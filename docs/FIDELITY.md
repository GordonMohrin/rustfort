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
| [validation] Craft quality from crafter skill | crafts are made at quality 0 | quality from the crafter's skill (w:Item quality); Gordon's DF crafts made in the r3-r6 windows average 2.4-2.9 with 25-46 masterworks (docs/VALIDATION.md) | MISSING (r8 validation) |
| Meal quality from cook skill | quality rolled from the cook's focus-adjusted Cooking skill (same roll as engravings), value x DF quality multiplier 1/2/3/4/5/12 (`lifestyle`) | each ingredient gets a quality from the cook's skill (w:Kitchen) | FIXED r8 (roll ESTIMATE; one roll per meal, not per ingredient) |
| Cooking plants keeps no seeds | yes | yes (w:Kitchen) | OK |
| Brewables are not cooked | default off, switchable | DF cooks them unless the kitchen settings forbid it; Gordon's rule | ESTIMATE (player setting) |
| Kitchen settings | `World::kitchen` / `Sim::set_kitchen`: excluded materials are never cooked (`no_cook`) or brewed (`no_brew`), matched by item type (PLANT, PLANT_GROWTH, other; empty = any) and material | DF kitchen screen (w:Kitchen) | FIXED r8 |
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
| Eggs | never rot (were perishable) | "Eggs do not appear to go rotten" (w:Nest box) | FIXED r8 |
| Plants wither, no miasma | plants wither | same (w:Rot) | OK |
| Rot timer | 28 days, rotted away after 28 more | not given on the wiki | ESTIMATE |
| Miasma | rotten corpses, body parts and food lying loose indoors (not under the sky) give off miasma that spreads orthogonally over open floor, 2 tiles; dwarves in it think "Miasma" (at most every 2 days); new corpses rot after 14 days and are skeletal 56 days later (`lifestyle`) | w:Rot, w:Miasma | FIXED r8 (range, timers ESTIMATE; real fort: about 40 Miasma thoughts a month, DF export 30-60) |
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
| Engravings of the live fort | exported to `engravings.json` (16 246 on 10.10.2026: pos, quality, floor/wall, tile material, engraver) and loaded into `World::engravings`, so room value counts them; the DF-reported level is still a floor (`min_tier`), see the note below | in `world.event.engravings` | FIXED r7 (exporter) |
| Dining | chair next to a table in a dining zone -> DiningQuality; else LackTables / NoDining | w:Dining room | OK |
| Bedroom thoughts | own bedroom -> BedroomQuality; bed outside a room -> LackBedroom; floor -> SleptFloor... | w:Bedroom | OK |
| Temples, taverns, libraries | meeting zones recognized by name | DF locations | ESTIMATE (r8: DF's locations, occupations, holders and values are now exported to `World::locations`) |
| Temples, taverns, libraries | `World::locations` (DF v50 locations); exports without them get locations from the named meeting halls (temple with the deity named in the zone, tavern, library, guildhall, hospital) and occupations from the citizens' professions (TAVERN_KEEPER, POET/BARD/DANCER, SCHOLAR...) (`locations`) | DF locations | FIXED r8 (name-based derivation is an ESTIMATE until the round-6 export brings DF's locations) |
| Dedicated temples | praying in a temple of the own deity (or an all-deities temple) adds "PrayedDedicatedTemple" | w:Temple | FIXED r8 |

## population.rs

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Population caps | d_init POPULATION_CAP, STRICT_POPULATION_CAP (also stops births), BABY_CHILD_CAP | w:Population cap | OK |
| Wave timing | decided per season, arrive later in the season | once per season (w:Migrant) | OK |
| First two waves | not special | hardcoded, 1-10 dwarves, not in the first winter (w:Migrant) | MISSING (only matters for new forts) |
| Wave size | 2 + rand(min(28, 4 + wealth / 25 000)) | from created wealth (w:Migrant) | ESTIMATE |
| Wave chance | 3 in 4 per season | not given | ESTIMATE |
| Dangerousness | deaths do not deter migrants | many recent deaths deter migrants (w:Migrant) | MISSING |
| Pregnancy | married couples, 9 months, twins 1 in 50 | w:Pregnancy | ESTIMATE (rates); r8 validation: 6 births (2-7) in 2.18 years vs DF's 2, and 4 vs 0 in 1.32 years, so about 3x too high |
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

## animals.rs (round 8)

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Who eats | only tame grazers (caste `GRAZER`); other tame animals need no food or drink | same (w:Grazer, w:Pasture) | OK |
| Grazer hunger | +1 per tick, death at 100 000 ("has starved to death") | same (w:Grazer) | OK |
| GRAZER value | 20000 x 100 x (max size / 10)^-0.75, clamped 150..3 000 000 (cow 521) | same formula and table (w:Grazer) | OK |
| Grass per bite | 1 % of a tile takes GRAZER / 100 hunger off; ten times the value needs a tenth of the grass | "A creature with ten times the grazer value needs one tenth the amount of grass" (w:Grazer) | OK |
| Eaten-bare tile | reverts to a soil floor (soil of the tile below) | "reverted to the base layer material" (w:Grazer) | OK |
| Grass regrowth | 1 % per 200 ticks, back to grass at 20 % | rate not given; we derive it from the rule of thumb "20000 = GRAZER x tiles" | ESTIMATE |
| No regrowth in mountain biomes | not modelled (biome not exported) | w:Grazer, w:Pasture | MISSING |
| Grazing start | at hunger 2 000 | not given | ESTIMATE |
| Pastures | Pen/Pasture zone; an Animal hauling dwarf leads the animal there; it strolls inside the zone | same (w:Pasture) | OK |
| Unpastured animals | wander anywhere (graze where they stand) | "gravitate to a meeting area" (w:Pasture) | MISSING |
| Fleeing from enemies | pastured animals do not flee | they flee (w:Pasture) | MISSING |
| Caged animal assigned to a pasture | uncaged and led there; the cage item stays marked occupied | uncaged and brought there (w:Pasture) | OK (cage bookkeeping: MISSING) |
| Mating | adult tame female + male of the race on adjacent tiles, idle | same (w:Breeding) | OK |
| Mating chance | 1/5 per 100 ticks of adjacency | not given | ESTIMATE |
| Not interested | 5 % of animals | 5 % (w:Breeding) | OK |
| Caps | no pregnancy at 50 of a race on the map or > 75 % children | same (w:Breeding) | OK |
| Gestation | 6 months | 6 months for non-sentients (w:Breeding) | OK |
| Litter | 1-3; `MULTIPLE_LITTER_RARE` 1 (1/500: 1-3) | LITTERSIZE default 1-3 (w:Creature token) | OK |
| Young | born tame, adult at 1 year (vanilla `CHILD:1`), size from 1/10 to adult | raws BODY_SIZE by age | ESTIMATE (size) |
| Eggs | adult `LAYS_EGGS` female claims a free nest box when ready, lays `CLUTCH_SIZE` eggs; next clutch 3 months later | same (w:Egg, w:Nest box, raws) | OK |
| Fertile eggs | a mating fertilizes for 6 months and 2 layings; fertile eggs left in the box hatch exactly 3 months after laying | same (w:Egg) | OK |
| Egg collection | food haulers take unforbidden eggs to food piles (they become food) | same (w:Nest box) | OK |
| Infertile eggs in a box | do not block the box | not documented | ESTIMATE |
| Milking | `MILKABLE` female, recharge 20 000 ticks (raws), milk into an empty bucket at the Farmer's workshop | same (w:Milking, raws) | OK |
| Shearing | sheep, llama, alpaca; again after 300 days; wool as a body part (`ShearCreature` order now runs) | same (w:Shearing) | OK |
| Animal walks to the workshop | the animal walks there itself while the dwarf fetches the bucket | the dwarf leads it (w:Milking) | ESTIMATE |
| Products | 1 milk, 1 wool, 1 cheese per milk, 1 yarn per wool, 2 fish per raw fish | not given | ESTIMATE |
| Work times | milk 200, shear 300, cheese 300, spin 300, clean fish 200, train 500, fish 600 ticks | not given | ESTIMATE |
| Slaughter | marked animal (not a pet) led to the butcher's shop by a butcher, killed, auto "Butcher a dead animal" job | same (w:Butcher's shop, w:Pet) | OK |
| Training levels | WildUntamed .. MasterfullyTrained, Domesticated | same names and order (w:Animal training) | OK |
| Who can be trained | taming needs PET/PET_EXOTIC, wild animals caged; war/hunting need TRAINABLE_* and adults | same (w:Animal training) | OK |
| Training knowledge | +10 points per training job per species | same (w:Animal training) | OK |
| Training success | 40 % + 4 %/skill level + knowledge/20 (max 20 %), cap 95 % | thresholds not given | ESTIMATE |
| Reverting | trained adults lose a level after 4 months without training; "reverted to a semi-wild state"; young trained once stay domesticated; bought/born tame never revert | same behaviour, timing not given (w:Animal training) | ESTIMATE (timing) |
| Training zone | not required | war/hunting training needs an animal training zone (or pasture) | MISSING |
| Knowledge to the civilization at caravan departure | not modelled | w:Animal training | MISSING |
| War animals | follow their dwarf; attack hostiles within 10 tiles | same (w:War animal) | OK (range ESTIMATE) |
| Hunting animals | follow their hunter and attack the prey | same (w:Hunting) | OK |
| Pet adoption | idle citizen within 3 tiles of a stray pet, 1/20 per day; cats (`ADOPTS_OWNER`) choose anybody; "after adopting a new pet" | same; chance depends on preferences (w:Pet) | ESTIMATE (chance) |
| Pets | follow the owner, cannot be slaughtered, "after interacting with a pet" 1/7 per day near the owner | same (w:Pet) | ESTIMATE (rate) |
| Available-for-adoption toggle | every stray tame pet is available | per-animal toggle (w:Pet) | MISSING |
| Hunting | labor Hunting, crossbow + bolts, wild butcherable prey, 1 bolt per shot, cancelled without bolts, corpse carried to the butcher | same (w:Hunting) | OK |
| Hunting rolls | hit 30 % + 4 %/crossbow + 2 %/ambusher level, 1 + size/100 000 hits kill, range 10 | combat model, sneaking | ESTIMATE |
| Hunting gear | reserved from the stockpile, not carried | the hunter equips crossbow, quiver, bolts | ESTIMATE |
| Prey flees | no | wild animals flee | MISSING |
| Fishing | labor Fishing, fishing zones preferred, else any water; pond turtles in non-natural water, cave fish/lobsters in caverns | same (w:Fishing) | OK |
| Overfishing | 40 catches per water body per season, refilled at season change | depletion and refill at season change (w:Fishing); amount not given; permanent depletion missing | ESTIMATE |
| Catch chance | 30 % + 5 %/Fishing level per 600-tick try | not given | ESTIMATE |
| Prepare raw fish | Fishery, labor Fish cleaning, auto job when raw fish exists | same (w:Fishery, auto fishery standing order) | OK |
| Cats and vermin | `HUNTS_VERMIN` cats kill a vermin 1/14 per day (rat remains, refuse) | vermin are real creatures in DF | ESTIMATE |
| Off-map animals in the export | not simulated; since exporter round 9 (and the importer for older exports) they are `flags.inactive` | observed 10.10.2026: the 80-82 tame animals at pos -30000 are animals of departed caravans (`flags1.forest`, not in `world.units.active`); DF does not simulate them either | FIXED r9 |
## locations.rs, justice.rs, lifestyle.rs (round 8, [society2])

Calibrated against the 17 281 thoughts of the 87 citizens in Gordon's round-5 export
(year 278-279): WatchPerform 908, Perform 95, Miasma 89, WearItem 73, IgnoredGuildhallPetition 44,
IgnoredTemplePetition 16, PrayedDedicatedTemple 5, PunishmentDelayed 7, and no Rain,
OldClothing or TatteredClothing thoughts.

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Tavern keeper | the TAVERN_KEEPER occupation serves drinks to idle patrons in the tavern (one per 100 ticks); occupation holders work at their location instead of their labors | w:Tavern | FIXED r8 (rate ESTIMATE) |
| Performances | performers (occupation or visiting) in a tavern or temple perform daily ("Perform", BeCreative); idle citizens there watch ("WatchPerform"; MakeMerry for music/dance, HearEloquence for poetry/story) | w:Performance | FIXED r8 (once a day ESTIMATE) |
| Libraries and scholars | idle citizens with unmet LearnSomething/ThinkAbstractly read written works kept in the library ("ReadBook", weekly); scholars ponder ("PonderTopic") and write on blank quires/scrolls ("WriteBook", 1 in 14 a day) | w:Library, w:Scholar | FIXED r8 (rates ESTIMATE) |
| Guildhalls | locations with a profession; guilds of 10+ members without a guildhall petition for one | w:Guildhall, w:Petition | FIXED r8 (guild grouping and 10 members ESTIMATE) |
| Temple petitions | religions with 10+ worshippers in the fort and no dedicated temple petition for one | w:Petition, w:Temple | FIXED r8 (10 ESTIMATE) |
| Ignored location petitions | unanswered after 28 days: members think "IgnoredTemplePetition" / "IgnoredGuildhallPetition"; accepted: "Accepted...Petition"; a new petition a year later | w:Petition; export shows both thoughts | FIXED r8 (timeout ESTIMATE) |
| Visitors | only with a tavern, library or temple and under VISITOR_CAP; performers and mercenaries/monster slayers to taverns, scholars to libraries, pilgrims to temples; walk in from the edge, stay 1-3 months (pilgrims 3-14 days), leave | w:Visitor | FIXED r8 (rates and stays ESTIMATE) |
| Residency / citizenship petitions | performers and scholars ask for residency, mercenaries and monster slayers for citizenship; `Sim::answer_petition`; unanswered = denied ("DenyResident"/"DenyCitizen") and the visitor leaves; residents keep performing / studying; new citizens join as workers | w:Petition | FIXED r8 |
| Crimes | tantrum toppling furniture = BuildingDestruction, tantrum fist fight = DisorderlyBehavior, visiting thieves (1 in 10 visitors) = Theft, unmet mandate = ProductionOrderViolation, broken export ban = ExportViolation (the noble names a random citizen); witnesses = citizens within 6 tiles | w:Justice, w:Crime | FIXED r8 (thief share, witness range ESTIMATE) |
| Investigation | only with a sheriff or captain of the guard: a witnessed crime is solved at the first witness interview (next day), unwitnessed ones by interrogation (1 in 56 a day); otherwise unsolved | w:Justice, w:Sheriff | FIXED r8 (rates ESTIMATE) |
| Conviction | the player convicts (`Sim::convict`); `Sim::set_auto_convict` lets the sheriff convict the accused (headless); convicting someone else is wrongful (no justice for the victim) | w:Justice | OK |
| Jail | Dungeon zone with a chain or built cage: the prisoner is restrained there, cannot work, is brought food and water, "Jailed" daily, "JailReleased" at the end | w:Jail | FIXED r8 (food/water brought instantly ESTIMATE) |
| Beatings | by the law enforcer when no restraint is free ("GotBeaten"/"GaveBeating", a bruise) | w:Justice | FIXED r8 |
| Hammerstrikes | with a hammerer and a hammer for crimes with strikes; each strike bruises, 1 in 15 breaks a bone; "GotHammered"/"GaveHammering"; no hammer = "NoHammer" and a beating | w:Hammerer | FIXED r8 (strike damage ESTIMATE) |
| Sentences | DisorderlyBehavior 7 days, BuildingDestruction 14, ProductionOrderViolation 14 or 5 strikes, Theft 28, AttemptedMurder 112 / 10 strikes, Murder 336 / 20 strikes | not given | ESTIMATE |
| Clothing wear | worn non-metal clothing gains a wear level every 336 days; past tattered it falls apart ("RottedClothing"); threadbare "OldClothing", tattered "TatteredClothing" (at most once a year) | w:Wear | FIXED r8 (timing ESTIMATE, fitted to the export's 91/7/1/1 % wear mix) |
| Clothing replacement | a dwarf (not soldiers) with worn (x) clothing fetches an unworn piece of the same subtype from a stockpile, puts it on ("WearItem" for quality items) and drops the old one | w:Clothing | FIXED r8 (threshold ESTIMATE) |
| Rain | weather on: rain on 1 day in 5 outside winter; "Rain" thoughts only with `Sim::set_rain_thoughts` (the export has none) | w:Weather | ESTIMATE |
| Fertilizer (potash) | not simulated | w:Farming | MISSING |
| Locations in the export | derived from zone names | the round-6 exporter writes DF's locations, occupations and crimes | MISSING (exporter) |

## Known gaps worth doing next (economy & society)

1. ~~Export `world.event.engravings`~~ FIXED r7: exported and counted. Bedrooms now score
   269-1 586 and the dining hall 2 826 (were 119-503). Under the `severity + 2` reading of DF's
   `BedroomQuality` thought our tiers are still about 2 below DF's (4/83 exact, 15/83 within one);
   read as `severity` = tier they fit (47/83 exact, 77/83 within one). Which reading is right
   decides whether `min_tier` can go.
   **r8 (exporter):** DF 53.16 keeps no value for personal rooms: `building_civzonest` has no
   value field, `dfhack.buildings.getRoomDescription` returns `""` for every v50 zone and piece of
   furniture, and the zone vmethods `getPersonalValue` / `getArchValue` return 0. DF's verdict on a
   bedroom exists only as the owner's quality thought (now also exported per zone as
   `room_thought_severity`). With the r6 export (15 more bedrooms, year 280) the `severity = tier`
   reading fits 7/14 exactly and 13/14 within one, `severity + 2` 1/14 and 2/14; together with r5,
   **54/97 exact and 90/97 within one tier**. So the thought severity is the wiki tier index
   (0 meager .. 7 royal), not tier - 2, and our room values are already in the right range; the
   remaining spread (mostly our value one tier below DF at 250-500) fits the missing overlap and
   enclosure rules. Locations do carry DF's value (`location_value`): Trussroads' temples range
   990-68 369 and are named by DF's difficulty thresholds (Temple 2 000, Temple Complex 10 000;
   guildhalls 2 000 / 10 000).
2. Meal quality from the cook's skill; fertilizer.
3. Miasma from rotting corpses and food.
4. Room overlap and unenclosed penalties.
5. Wagons and traps/stairs on the depot path; pack animal load limits.

# Combat, defense, health & moods

Modules: `combat.rs`, `military.rs`, `invasion.rs`, `ranged.rs`, `defense.rs`, `health.rs`, `moods.rs`, `caverns.rs` and the new `syndrome.rs`.

Wiki pages checked: Combat, Material science, Attack, Armor, Wound, Blood, Pain, Dodging, Military, Squad, Schedule, Siege, Ambush, Invasion, Crossbow, Ammunition, Trap, Cage trap, Weapon trap, Door, Bridge, Lever, Burrow, Health care, Hospital, Infection, Strange mood, Artifact, Insanity, Cavern, Forgotten beast and Cave-in.

## Round 6 fixes

| Fix | Wiki | Before | After | Evidence |
|---|---|---|---|---|
| Soldiers see lurking hostiles | Military: on-duty soldiers "will attack any hostiles they encounter" | A dormant hostile (no prey in its own sight) was skipped by the threat scan. Soldiers within 20 tiles ignored it until it woke up by itself 400-500 ticks later, and never at all if their z differed by 3-4. | Soldiers on duty spot it within 100 ticks; civilians still react only to active hostiles. | `combat::tests::soldiers_spot_a_lurking_hostile` fails without the fix (target stays `NONE`) and passes with it. Counter `CombatStats::lurkers_engaged`. |
| Wounded soldiers rest | Health care: injured dwarves "go (or are brought) to recuperate" | `combat_decide` always made a soldier on duty a `Soldier`, so soldiers with broken bones trained or stood guard until they healed (bones heal only at rest, so never). | With no able hostile within 3 tiles, a wounded soldier becomes a patient (hospital bed first, care by doctors) and returns to duty when healed. The threat scan no longer pulls a resting soldier out of bed. | `combat::tests::wounded_soldier_rests_instead_of_training`. On the real fort over 2 years, `soldiers_rested` = 2. |
| Weapon traps jam | Weapon trap: "a 50% chance that a kill will result in the victim getting stuck in the mechanism and cause the trap to jam"; "a mechanic will automatically attempt to clean it" | Traps never jammed. | 50 % of kills by the trap itself jam it: it does nothing until a mechanic does a **Clean Trap** job (no job while enemies are within 20 tiles). The state is kept as a `JAMMED` building flag, so it survives a rebuild or a save. | `defense::tests::weapon_trap_jams_on_kills_and_a_mechanic_cleans_it`. On the real fort over 2 years, `traps_jammed` = 1. |
| Forgotten beast special attacks | Forgotten beast: poisonous bites and stings, deadly dust and toxic gas, webs; syndromes "from mild pain to complete and instantaneous necrosis or paralysis" | Only claws and teeth. | `syndrome.rs` rolls one special attack per beast: none 30 %, venom 35 %, breath 25 %, webs 10 %.<br>Effects: paralysis, unconsciousness, bleeding, pain, necrosis, and respiratory failure (venom only).<br>The arrival text ends with DF's "Beware its ...!". | 4 tests in `syndrome::tests` (breath paralyzes at 6 tiles, a venom bite takes hold only on a wound, respiratory failure kills, the odds). On the real fort 2 of 5 beasts had stings. |

## Combat (wounds, materials, defense rolls)

| Item | Class | Notes |
|---|---|---|
| Attack momentum (Material science formula 3), weapon vs body-part attacks | OK | Unit test against the wiki's battle axe example. |
| Edged cut / blunt fracture thresholds over armor and tissue layers, quality factors | OK | Wiki formulas. Some minimum contact areas are estimates (A >= 1 blunt, A >= 2 edged on armor). |
| "Copper cannot cut iron": no cut when the armor's shear yield is at least 2x the weapon's | ESTIMATE | Wiki lore only, no formula. |
| Contact area = volume^(2/3) x 50 | ESTIMATE | |
| Hit, dodge, block and parry chances; attack intervals; momentum roll | ESTIMATE | DF uses opposed skill and attribute rolls; the wiki gives no numbers. |
| Dodging moves the defender to an adjacent tile | MISSING | Rustfort's dodge leaves the defender in place. |
| Wrestling moves (grab, joint lock, break, throw, choke) | MISSING | Unarmed attacks are body-part strikes only. |
| Stunned, winded, nauseous and dizzy states | MISSING | Combat page status effects. |
| Fatigue and exhaustion in long fights | MISSING | |
| Prone and crawling with broken legs | FIXED r8 | A unit with an unset broken bone in a weight-bearing (STANCE) limb, or a lost foot, cannot stand: it waits on the floor to be carried, or crawls 4x slower (ESTIMATE) when nobody has Recover wounded. No crutches yet. `health::tests::without_rescuers_the_patient_crawls`. |
| Fainting below 50 % blood, death at 0 blood, brain/heart/head death | OK | Wiki Blood / Wound. |
| Bleeding volumes, clotting, pain threshold 250 + WILLPOWER/10, healing rates | ESTIMATE | |
| Severing (all layers cut and the contact area is at least half the part) | ESTIMATE | |
| Fire, burns, temperature | MISSING | The save has temperature off. Dragonfire and fire breath are not modelled. |

## Military

| Item | Class | Notes |
|---|---|---|
| Duty from orders and schedule (off duty, staggered, constant, Ready) | OK | Staggered is split by position parity per month (ESTIMATE). |
| Kill orders: pursue the unit wherever it is reachable; done when it dies | OK | If the target is unreachable the soldier waits (DF soldiers stand idle the same way). |
| Soldiers engage hostiles in sight (20 tiles, 2 z-levels) | ESTIMATE + FIXED r6 | The range is an estimate; dormant hostiles are now included. |
| Patrol routes | FIXED r8 | `Squad::patrol` waypoints (`Sim::order_squad_patrol`): a `Patrol` order walks them in a loop, waiting 50 ticks at each (ESTIMATE); patrolling soldiers engage what they see. Since exporter round 9 the export carries the route's waypoints (Trussroads has no routes); patrols without waypoints still train. `barracks::tests::patrol_walks_its_route`. |
| Training by sparring and demonstrations (sparring injuries, teachers) | FIXED r8 (ESTIMATE rates) | Wiki Sparring/Barracks: soldiers train in their barracks; partners side by side spar (double weapon experience, +DODGING; a bruise in 1 of 2000 bouts), and a squadmate within 3 tiles who is 2+ levels better gives a demonstration (+2 experience, 1 in 10 training rounds). Without a barracks: individual drills as before. `barracks::tests::soldiers_train_and_spar_in_their_barracks` (sparring teaches more than drills). Lectures are not modelled. |
| Barracks: sleeping, training spot, equipment storage | FIXED r8 | `Squad::barracks` is read from the export (`squad.rooms`; your Wache has the Kaserne, the Schuetzen the archery range). Training spots in the barracks, beds in the barracks for soldiers on duty (ESTIMATE: DF's per-routine sleep mode is not exported), uniform pieces stored in the barracks taken first. `soldiers_on_duty_sleep_in_barracks_beds`. |
| Squad ammunition settings (ammo per squad, combat/training flags) | FIXED r8 | `Squad::ammo` (`Sim::set_squad_ammo`): ammo type, material or class (`WOOD`, `BONE`, `METAL`), amount (split per member, at most a quiver of 50) and combat/training use. Exported since round 9 (Trussroads: Schuetzen 200 iron bolts for combat, 120 wooden for training); without settings the old rule applies (metal for combat, else training ammo). `squad_ammo_settings_pick_ammo_by_use`. |
| Uniforms: pick up missing pieces, `KIND:SUBTYPE@MATERIAL` entries | OK + r8 | Per-position uniforms (`Sim::set_squad_uniform`) and an equipment report per squad (`Sim::squad_equipment`: worn, missing, ammo). "Replace clothing", "exact matches" and partial matches are not distinguished. `position_uniform_and_equipment_report`. |
| Wounded soldiers rest | FIXED r6 | |
| Soldiers leave a fight at the job-cancel thirst/hunger thresholds | ESTIMATE | Wiki: fights are "do-or-die". The rule exists so an unkillable foe cannot starve the whole squad. |
| **Civilian alert** | FIXED r7 | Wiki Burrow: citizens assigned to a burrow "will only accept jobs from within their assigned burrow(s)" and still work. Rustfort r7: inside the alert burrow civilians dig, build, work workshops and farm plots and haul, when the work tile and the needed items lie inside the burrow (one bitset test per candidate). Idle civilians mill about inside.<br>[r8] Drinks, meals and beds inside the burrow come first (outside ones only when there are none inside, ESTIMATE); doctors and Recover wounded treat patients inside the burrow with supplies from inside; levers inside the burrow are pulled; corpses inside are buried in coffins inside. `civilian_alert_prefers_drinks_and_beds_inside_the_burrow`.<br>Still ESTIMATE: depot, broker and trap-reload jobs pause under the alert. |
| Civilians flee hostiles within 12 tiles; cornered civilians fight back | ESTIMATE | |

## Invasions

| Item | Class | Notes |
|---|---|---|
| Sieges from a population of 80 | OK | Wiki Siege: "80 members". |
| Checked at season changes, chance 1/3 | ESTIMATE | |
| Siege size | FIXED r7 | Wiki Siege + Difficulty (Normal): a tier needs its population "and one of production or trade": population 20/50/80/110/140, created wealth 5k/25k/100k/200k/300k, exported wealth 500/2.5k/10k/20k/30k. Sieges from the Town tier: 10 regulars, City 40 (+5 monsters), Metropolis 120 (+40 monsters). Rustfort r7 uses these tiers.<br>ESTIMATE: created wealth = current item value; exported = goods sold this session. Monsters are not spawned yet. |
| Sieges vs ambushes | FIXED r8 | `raids.rs`: an invasion is a siege 1 time in 4 when the siege tier is met, otherwise an ambush (from the first tier on). Ambush squads arrive **hidden**: civilians do not flee, soldiers do not engage, `Sim::hostiles` does not list them, until a citizen spots them (observer vs ambusher skill within a few tiles, ESTIMATE) or they strike: "An ambush! Curse them!". Size ESTIMATE: 1-3 squads of 2-5. `raids::tests::ambush_is_hidden_until_spotted`. |
| Thieves and snatchers (kobold thieves, goblin snatchers) | FIXED r8 | Raws: kobolds `ITEM_THIEF`, goblins `BABYSNATCHER`. A hidden thief takes the most valuable small item it can reach and runs for the edge (the item is lost); spotted: "Thief! Protect the hoard from skulking filth!" and it flees. A snatcher carries a child off the map ("... has been abducted!", wording ESTIMATE); spotted: "Snatcher! Protect the children!"; killed or knocked out it drops the child. Visit odds ESTIMATE (thief 1/2, snatcher 1/3 per season, random day). 5 tests in `raids::tests`. |
| Goblins charge at once; humans sometimes camp at the map edge | FIXED r8 (ESTIMATE times) | Natural and forced sieges gather at their arrival point first (goblins 600-2400 ticks, other races 2-7 days), attack only what comes within 8 tiles, then charge. `Sim::spawn_siege` keeps the old immediate charge for tests. `siege_camps_then_charges_with_monsters`. |
| Monsters and mounts (trolls, beak dogs) in sieges | FIXED r8 | Wiki Siege: +5 monsters at City, +40 at Metropolis: trolls and beak dogs 1:1 (ESTIMATE) with the save's raws (troll 250 000 cm³, beak dog 150 000). Riding is not simulated (beak dogs walk beside). 25 % of a siege are bowmen (ESTIMATE). |
| Retreat when "sufficiently" beaten or waited out | ESTIMATE + r8 | Half dead or 2 months; [r8] a siege also breaks when its leader (best weapon skill) dies (wiki: killing the leader; ESTIMATE). Thieves and snatchers leave on their own, at the latest after a month. `a_dead_leader_breaks_the_siege`. |
| Invader equipment shares and skills | ESTIMATE | |

## Ranged combat

| Item | Class | Notes |
|---|---|---|
| Launcher + matching ammo, quivers, fire range about 20 tiles, misses fly on | OK | Wiki Marksdwarf. |
| Projectile momentum and armor resolution | OK | Wiki Material science. |
| Fortifications: below Accomplished, fire only from the adjacent tile | OK | Wiki Fortification. |
| Hit, dodge and block chances; rate of fire; ammo breakage | ESTIMATE | |
| Friendly fire (units standing in the line of fire) | MISSING | |
| Squad ammo settings | FIXED r8 | See Military. Hidden raiders cannot be targeted. |

## Defense (doors, bridges, levers, traps)

| Item | Class | Notes |
|---|---|---|
| Locked or lever-closed doors block everyone; building destroyers smash them | OK | |
| Pet-passable doors: pets path through | MISSING | Pets only wander through. |
| Bridges: raised wall edge, flinging, atom smasher, retracting | OK | |
| Mechanism delay 100 ticks; pulling a lever 10 ticks | ESTIMATE | |
| Linking needs mechanisms and a mechanic job | MISSING | Linking is instant. |
| Traps: hostiles and wild animals trigger them; citizens, pets, merchants and `TRAPAVOID` creatures do not | OK | |
| Unconscious creatures trigger traps, including your own dwarves | MISSING | Wiki Weapon trap. |
| Cage trap: the creature is caught; a mechanic re-arms it with a free cage | OK | |
| Occupied cages hauled to an animal stockpile (standing order) | MISSING | Wiki Cage trap. The cage stays on the trap tile; hauling belongs to the storage module. |
| Weapon trap: all weapons strike at once (trap components `HITS` times) | OK | |
| Weapon trap jamming and Clean Trap | FIXED r6 | |
| Weapon trap reset delay ("after an unknown period") | ESTIMATE | Rustfort: no delay. |
| Stone-fall trap: single use, re-armed with a stone | OK | |

## Health

| Item | Class | Notes |
|---|---|---|
| Diagnosis first, then suturing, bone setting, surgery and dressing with supplies | OK | Durations are estimates. |
| Hospital beds first, then the patient's own bed, then any free bed | OK | Wiki Health care. |
| **Recover wounded**: immobile or injured dwarves are carried to a hospital zone or the nearest free bed | FIXED r8 | Wiki Health care / labor RECOVER_WOUNDED: a patient who cannot walk (unconscious, or cannot stand) and needs rest is picked up by a dwarf with the labor and carried to a hospital bed, else its own, else any free bed; it no longer walks on a broken leg. Nobody is carried while an able hostile is within 3 tiles (ESTIMATE). A carrier that dies or passes out drops the patient. `health::tests::broken_leg_patient_is_carried_to_bed`. |
| Feeding bedridden patients (water, food) | OK | |
| Traction benches, casts, crutches | MISSING | |
| Hospital chests stocked with thread, cloth and splints | MISSING | Supplies are taken from anywhere. |
| Infections from uncleaned wounds; soap | ESTIMATE | Rates. |

## Strange moods and insanity

| Item | Class | Notes |
|---|---|---|
| Counter, 20 eligible dwarves, artifact cap `min(items/100.1, revealed tiles/2304)` | OK | |
| Profession lottery, workshop by highest moodable skill, materials, 50 000-tick insanity timer | OK | |
| Fell and macabre moods only for highly stressed dwarves | OK | The odds are estimates. |
| Artifact value, names | ESTIMATE | Names come from random syllables, not DF's language files. |
| Insanity kinds and odds | ESTIMATE | Melancholy dwarves stop eating; they do not seek water to drown in. |

## Caverns and forgotten beasts

| Item | Class | Notes |
|---|---|---|
| Cavern layers, discovery, breach, magma sea | ESTIMATE | Flood-fill detection; DF stores the layers directly. |
| Cave wildlife from the save's subterranean creature raws | OK | Group sizes and rates are estimates. |
| Forgotten beast arrival rate | ESTIMATE / too high (r8 validation) | Rustfort: 1/4 per season, one at a time, once a cavern is discovered (5 in 2 years on the real fort). Wiki: "exactly one beast is spawned per each underground region", attracted by wealth. [validation] Gordon's gamelog shows 0 forgotten beasts from autumn 275 to summer 280; Rustfort brings 1-5 per 2 years. |
| Beasts never leave; `TRAPAVOID`; building destroyers | OK | |
| Venom, breath, webs and syndromes | FIXED r6 (simplified) | No resistance or size dilution, no cure; the durations are estimates. |
| Fire breath, dragonfire, secretions, contact syndromes on touch | MISSING | No temperature. |
| **Cave-ins** | FIXED r7 | Wiki Cave-in: tiles not supported along the X, Y and Z axes fall (diagonals, bridges and ramps below give no support). Creatures directly underneath die; the dust knocks out and throws creatures nearby. Rustfort r7 (`caveins.rs`): after digging, channeling or deconstruction, a local flood fill (capped at 4096 tiles) looks for the map bottom or edge; unsupported chunks become open space, leave boulders and a floor below, kill creatures underneath, and the dust knocks out units within 2 tiles.<br>ESTIMATE: falling debris stops on floors; no dust throwing or dust thoughts. |

## [validation] Rules checked against Gordon's history (round 8)

Measured with `examples/validate.rs`, which replays real DF snapshots of the fort; numbers and
method in `docs/VALIDATION.md`.

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| XP per job | 30 for digging, crafting, cooking, smoothing, engraving; 20 for building | "Most civilian skills give 30 XP per use, plus 30 more if an item is produced that has quality levels"; stonecutting, smoothing, engraving and mining "only 10 XP per use" (w:Experience). DF's real mining gain (50 XP per dwarf-year) equals 782 dug tiles x 10 | WRONG |
| Skill level cap | the rating stops at 20 (`add_xp`) | "no upper limit" (w:Experience); Gordon's export has ratings up to 29 (96-137 skills above 20) | WRONG |
| Social and mental skill XP | none (conversations, teaching, reading give no XP) | DF: about 2 000 XP per dwarf-year (concentration, knowledge acquisition, teaching, organization, persuasion...) | MISSING |
| Sieges vs fort deaths | 0-4 citizens die in 2.18 years | DF: 14 in 2 sieges (r4 -> r5), 73 in 1 siege (r5 -> r6) | WRONG (see Siege size) |
| Tree growth | trees never grow | +65 trees in 2.18 years | MISSING |
| Building destroyers vs workshops | workshops are not attacked | 17 of 31 workshops gone after the r6 siege | MISSING |
| Caravans per year, strange moods, artifacts | 4 / 5 / 2-5 in 2.18 years | 4 / 5 / 5 | OK |
| Stockpile fill | 25 % of pile tiles hold items (r4 -> r5) | 37-39 % | ESTIMATE (bin volume) |

## Performance and determinism (round 6, real fort, 2 years with `--standdown`)

- **State hash:** stays the same across repeated runs (`9cfb23265fa36f6b`, twice).
- **Survival:** 88 -> 90 citizens, the same as master.
- **Speed:** about 59.6k ticks/s, against master's 61.4k measured back to back on the same busy machine (4 agents building at once). That is within the noise.
- **Combat cost:** the combat phase stays at about 0.11 us/tick.

# Export data (round 8)

What the round-8 exporter (`tools/dfhack/rustfort-export.lua`, rev `2026-10-10e`) brings into
`World` for the round-8 simulation modules, checked against the live game with
`rustfort-verify.lua` (docs/EXPORT_FORMAT.md "Additions for round 8").

| data | status | notes |
|---|---|---|
| Work details (name, labors, members, mode, icon) | NEW r8, OK | `World::work_details`, `Unit::work_details`; 12/12 match the live game. DF 53 has no per-detail "no other work" flag: `no_other_work` is always false, the per-unit switch is the unit extra `only_do_assigned_jobs` |
| Stockpile settings below category level | NEW r8, OK | `StockpileSettings::filters` with DF's full settings (`category/group/token`, `*`, `!token`), `use_links_only`, `give_to`, `take_from`; 110/110 sampled leaves and 43/43 piles' links match. `StockpileSettings::filter_allows` / `filter_option` give the semantics. ESTIMATE: the names and order of the "other material" lists follow DFHack's stockpiles plugin (lengths match DF) |
| Kitchen exclusions | NEW r8, OK | `World::kitchen`; Trussroads: 269 "no cook", 0 "no brew" |
| Locations, occupations, holders, values | NEW r8, OK | `World::locations` (23 on Trussroads), DF's `location_value`; tier names derived from DF's difficulty thresholds (DFHack's rule), not read from DF |
| Justice cases | NEW r8, OK | `World::crimes` (328 at the site, 60 sentenced); running punishments via `rf_import::load_extras`. `Jail N` keeps DF's `prison_time` unit (not confirmed: months?) |
| Animals: owner, training, pasture, slaughter, pregnancy | NEW r8, OK | `Unit::pet_owner/training/pasture/slaughter/pregnancy`; pasture from the Pen zones' `assigned_units` |
| Milk, grazing, shearing, eggs per caste | NEW r8, OK | caste flags (`MILKABLE`, `GRAZER`, `LAYS_EGGS`, pseudo-flag `SHEARABLE`) plus `rf_import::AnimalProducts` (milk material and frequency, grazer value, shearable layers, egg materials, clutch/litter size) |
| Hauling routes | NEW r8, synthetic only | `World::haul_routes`; Trussroads has none |
| DF's value of personal rooms | not available | DF 53.16 keeps none (see "Known gaps" item 1); the owner's quality thought is exported instead |
# Player screens (round 8, rf-app `[app8]`)

The screens follow the DF Steam (v50/53) layouts; the rules behind them live in the sim (`round8_api`). This is what the UI itself decides:

| Item | Class | Notes |
|---|---|---|
| Labor > Work details: list with DF's `WORK_DETAIL_*` icons, the modes Everybody / Only selected / Nobody (`LABOR_WORKER_ASSIGNED*`), per-dwarf toggles, "assigned workers only do this" | OK | Observed in Gordon's DF 53.16 via DFHack (read-only): the 11 built-in details (Miners ... Siege operators) with their icons, modes and labors, plus one custom detail. |
| Built-in details keep their labors | OK | DF's `no_modify` flag is set on the 11 defaults. Custom details get the labor editor and Delete. |
| Clicking a dwarf in an Everybody or Nobody detail | ESTIMATE | Switches to "Only selected" with everybody else, or with only that dwarf. DF's exact click behavior was not checked. |
| "Set up DF's default work details" for exports without them | ESTIMATE | Assigns the "only selected" details to dwarves who already have all of their labors. |
| Kitchen: cook/brew per item kind and material (`LABOR_KITCHEN_*`) | OK | Rows come from the fort's stock (DF lists the same kinds).<br>Cooking is allowed for edible plants, seeds, drinks, meat, fish, eggs, cheese, milk, flour and fat. Brewing is allowed for brewable plants and fruit (raws). |
| Plump helmets: no cook | ESTIMATE (player setting) | Gordon's rule. It is added at start only when the save has no kitchen entry for them. |
| Stockpile settings | OK | DF Steam layout and icons (`STOCKPILE_*`, `CUSTOM_STOCKPILE_*`): categories, groups and leaves with on/partial/off marks, presets, bins/barrels/wheelbarrows, give-to/take-from links picked on the map, and "take from links only".<br>Filter paths follow the exporter: `cat/group/token`, `cat/group/*` (whole group), `cat/group/!token` (all but that one). A category without filters accepts everything. |
| Leaf lists from DF (exporter round 9, `stockpile_options.json` -> `Sim::stockpile_filter_options`) | OK + ESTIMATE | Every leaf DF's vectors index; which inorganics, plants and item types a group shows follows DFHack's stockpiles plugin (ESTIMATE, see docs/EXPORT_FORMAT.md). |
| Leaf lists when the export has none (before round 9) | ESTIMATE | Built from the save's raws and materials (all creatures for meat, and so on).<br>Stone uses DF Steam's ore / economic / other / clay split. "Economic" means flux, gypsum, calcium carbonate and clay, plus a fixed list (coal, kaolinite, ...). |
| What the "Organic" and "Inorganic" presets cover | ESTIMATE | Organic: food, corpses, refuse, leather, cloth, wood, sheets.<br>Inorganic: stone, ammo, coins, bars/blocks, gems, finished goods, furniture, weapons, armor. |
| Pile-to-pile links are kept on both piles | OK | DF keeps `give_to` / `take_from` on both sides. Workshop links are kept on the pile only (rf-core has no workshop link list). |
| Pets and livestock: training, owner, pasture, pregnancy, slaughter, war/hunting training, pen picker | OK | DF's `PETS_LIVESTOCK_*` buttons. Training requests go to `Sim::request_training` ("" cancels). |
| Places: locations with occupations; make a location from a meeting area (a temple asks for the deity) | OK | The deities are those the citizens worship, most worshippers first (DF's dedication list).<br>Occupations per kind: tavern keeper and performer, temple performer, library scholar and scribe, hospital doctor (plus the exported diagnostician, surgeon and bone doctor).<br>New locations are named "Tavern 1 of Trussroads"; DF generates names (ESTIMATE). |
| Justice: open and closed cases, convicts, convict picker (witnesses first) | OK | No interrogation; the punishment is the sim's. |
| Objects: artifacts with DF's item name, material, place (named room) and maker | OK | The maker is only known for artifacts made in Rustfort (mood records); the export has no creator. |
| World: civilizations with their relation, caravans | OK | |
| Minimap | not in DF | DF Steam has no minimap; this is a Rustfort extra and off by default. |
## Round 8 combat check (real fort, `raids` example and 2-year bench)

- Forced raids on the r5 export (`cargo run -p rf-sim --release --example raids -- --kind ... --standdown`):
  - **Ambush** (8 goblins, hidden): spotted after about 800 ticks ("An ambush! Curse them!"); all 8 killed (one caged); 1 citizen lost.
  - **Thief** (kobold): spotted at the gate ("Thief! Protect the hoard from skulking filth!") and struck down by the Wache before it stole anything.
  - **Snatcher** (goblin): spotted ("Snatcher! Protect the children!"); it fled off the map without a child.
  - **Siege** with a camp, 20 goblins and 6 monsters: camped 2000 ticks, then charged. 2 trolls were caged, 3 beak dogs and a troll killed, the leader died and the rest retreated: 18 killed, 1 citizen lost.
- `--break-leg 4`: all 4 citizens with a broken lower leg were carried to beds (4 rescues).
- 2 years with `--standdown` and invasions on: 87 -> 91-93 citizens. 1 siege (camped), 2 ambushes, 2 thieves (2 items stolen unseen), 3 snatchers (no child lost); soldiers spar in the Kaserne.
- **Speed**, back to back with the round-7 build at low machine load: 53-61k ticks/s against 54k, the same within noise. The combat phase costs 0.17-0.19 us/tick. State hashes are equal with 1 and 6 threads, also with invasions on.
