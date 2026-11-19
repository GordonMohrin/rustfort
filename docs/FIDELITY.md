# Rustfort fidelity audit (round 6)

This file compares the rules Rustfort implements with the DF v50 wiki (dwarffortresswiki.org)
and with what DF does in the reference fort (Trussroads / Tileshtulon, the DF fort Rustfort
is developed with). It was written by two audit agents:

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
"export" is the reference fort, read live in DF with read-only DFHack.

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
| [validation] Craft quality from crafter skill | [skills9] every workshop product with quality levels (crafts, furniture, weapons, armor, clothing, containers, tools, cloth, ammo) rolls its quality from the maker's focus-adjusted skill: a floor of skill/4, then up to 3 more levels at 55 % each; masterwork at most 1 in 3 (`skills::quality_roll`); value x DF quality multiplier | quality from the crafter's skill; "past a certain high skill level a dwarf will almost always make at least exceptional quality items", masterworks capped at 1/3 (w:Item quality); the reference fort's DF crafts average 2.4-2.9 with 25-46 masterworks, its skill 1-3 leatherworkers made 39/34/12/15 % ordinary..superior | FIXED r9 (roll ESTIMATE; r4 -> r5 craft mean 1.8 vs DF 2.6, because unskilled dwarves with the craft labor take most craft jobs in the sim). [gaps10] Investigated read-only in the reference fort in DF (year 280): all 10 surviving workshops have the default profile (skill 0-3000, no assigned workers), no citizen has `only_do_assigned_jobs`, so neither workshop profiles nor specialization explain it. Its 1 234 crafts came from 93 makers; the top 9 (final skill 3-25) made 49 % of them. Wiki: quality also depends on the maker's **attributes and preferences** (liked materials make better goods), which are not modelled. [metal12] Now the wiki's quality-point formula (w:Skill): `rand(0..10) + 2 x rand(0..5 level / 2)` + the skill's attributes (primary / 100, others / 250; crafters Agility + Creativity, Spatial and Kinesthetic sense; smiths and masons Strength, Agility, Endurance + Kinesthetic) x focus, skill penalties (dehydrated, starving, very drowsy: x0.5 each), +10 per item / material preference, thresholds 22 / 30 / 35 / 45 / 55 (`skills::quality_points_roll`). Result (3 seeds, VALIDATION round 12): crafts unchanged, r4 -> r5 1.8 (before 1.9; DF 2.6), r3 -> r4 1.7 (before 1.7; DF 2.9); other goods moved toward DF (r3 -> r4 all goods 2.9 -> 2.6, DF 1.9; masterworks 183 -> 123, DF 70). Preferences (+10 + 10 points, about two quality levels for a matching crafter) are the remaining lever and are **not exported** (DFHack `unit.status.current_soul.preferences`). OPEN (exporter) |
| Meal quality from cook skill | quality rolled from the cook's focus-adjusted Cooking skill (same roll as engravings), value x DF quality multiplier 1/2/3/4/5/12 (`lifestyle`) | each ingredient gets a quality from the cook's skill (w:Kitchen) | FIXED r8 (roll ESTIMATE; one roll per meal, not per ingredient) |
| Cooking plants keeps no seeds | yes | yes (w:Kitchen) | OK |
| Brewables are not cooked | default off, switchable | DF cooks them unless the kitchen settings forbid it; a player rule of the reference fort | ESTIMATE (player setting) |
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
| [calib11-econ] Work orders under a civilian alert | the order's jobs go to matching workshops inside the alert burrow (if it has one), and jobs waiting outside no longer use up the order's amount | burrowed dwarves "will only accept jobs from within their assigned burrow(s)" (w:Burrow); before, jobs queued at unreachable workshops held the whole amount and the burrow's still or kitchen got none | FIXED |
| [calib11-econ] Fallback standing orders (`--standing-orders`, not DF) | brew below 18 drinks per citizen, cook below 8 meals per citizen (today's citizens, re-checked daily); the daily amount grows with the shortfall (1 job per 15 missing drinks / 6 missing meals, base .. 4x base) | emulates the reference player's food-watch script ("as stocks run low, raise the production targets instead of switching production off"); targets from DF's real stocks per citizen (drinks 14.6 / 18.4 / 35, meals 8.8 / 7.4 / 8.8 at r4 / r5 / r6) | ESTIMATE (player emulation) |
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
| [metal12] Coke | 1 bituminous coal boulder -> 9 coke, 1 lignite -> 5 coke, + fuel (smelter; the `BITUMINOUS_COAL_TO_COKE` job in the reference fort's smelter queue now runs) | raws `reaction_smelter.txt` (w:Coke) | FIXED |
| [metal12] Alloys | bronze, brass, electrum, billon (bars 1 + 1 -> 2, ore 1 + 1 -> 8), fine / trifle / lay pewter, nickel silver, black bronze, sterling silver, rose gold, bismuth bronze with the raws' reagent counts; METAL_ORE reagents accept any ore with that METAL_ORE (tetrahedrite = copper and silver ore); `industry::reactions::REACTIONS`, checked against the install's raws by `reaction_table_matches_raws` | raws (w:Alloy) | FIXED (adamantine wafers missing) |
| [metal12] Pig iron, steel | pig iron = 1 iron bar + 1 flux boulder (RC:FLUX) + 1 coal bar (+ fuel); steel = iron + pig iron + flux + coal -> 2 steel (+ fuel); the coal reagent is needed even in a magma smelter | raws (w:Steel, w:Pig iron, w:Flux) | FIXED |
| [metal12] Magma smelter / forge / glass furnace / kiln | the plain workshop's jobs without fuel, only while a tile directly below one of the 8 non-center tiles holds magma 4/7+ | "at least one tile directly beneath one of its eight non-center tiles contains magma at a depth of 4/7 or greater" (w:Magma forge) | FIXED (DF cancels jobs in progress when the magma drops; the sim only stops taking jobs) |
| [metal12] Kiln | clay boulders (FIRED_MAT: clay, silty / sandy clay, clay loam -> earthenware value 3, fire clay -> stoneware 4, kaolinite -> porcelain 10) -> jug, brick, statue, large pot, craft (Pottery); pearlash from potash (+ fuel) | raws `reaction_other.txt` (w:Kiln) | FIXED (no glazes, no plaster, no "Collect clay" zone yet) |
| [metal12] Glass furnace | raw clear glass = bag of sand + pearlash, raw crystal glass = rock crystal + pearlash (+ fuel) | w:Glass furnace | FIXED (glass furniture/vials/windows not yet) |
| [metal12] Ashery | make lye: ash + empty bucket -> lye in the bucket; potash from lye (bucket emptied) or from ash (no fuel) | w:Ashery, w:Lye, w:Potash | FIXED (milk of lime missing) |
| [metal12] Tallow, soap | kitchen "Render fat" 1 fat (RC:FAT) -> 1 tallow of the creature; soap maker 1 lye + 1 tallow -> 1 soap bar (doctors use soap, `health.rs`) | raws `RENDER_FAT`, `MAKE_SOAP_FROM_TALLOW` | FIXED (soap from oil, screw press missing) |
| [metal12] Splints, crutches | carpenter from 1 log, forge from 1 bar; quality items (the reference fort's standing orders `ConstructSplint` / `ConstructCrutch` now run) | w:Splint, w:Crutch | FIXED |
| [metal12] Dyes | not implemented (milled dye powder exists, no dyer's shop jobs) | w:Dyer's shop | MISSING |

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
| Wheelbarrows | not simulated (the reference fort's piles use 0) | (w:Wheelbarrow) | MISSING |
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
| Wave timing | [calib11-econ] a date 1-27 days into spring, summer and autumn; what comes is decided on that date | "The date ... is determined at the start of that season, but the number of immigrants ... when the migrant wave arrives" (w:Migrant); no winter waves: the reference install's gamelog (8 forts, ~100 years) never has more than 3 migration messages per year | OK (wiki + observed) |
| Sieges | [calib11-econ] a wave waits while a siege is seen on the map and is lost when its season ends | waves can be "blocked by a siege" (w:Migrant); gamelog: no migration message in spring 279, right after a siege arrived | OK (wiki + observed) |
| First two waves | not special | hardcoded, 1-10 dwarves, not in the first winter (w:Migrant) | MISSING (only matters for new forts) |
| Wave size | [calib11-econ] 1 + rand(max), max = min(4 + wealth / 25 000 (<= 28), free places under the cap / 3 (>= 1), local-population limit) | from created wealth; 1000 local units: at most 10, -1 at 1300 ... 2900, none from 3000 (w:Migrant); the reference fort's waves near the cap were 1-3 (22 waves, counted by the player's script), up to 10 with ~40 free places | ESTIMATE (wealth part, cap shrink observed) |
| Wave chance | 3 in 4 per season, else "The fortress attracted no migrants this season." | not given (gamelog: 22 waves in 36 seasons for Trussroads) | ESTIMATE |
| Dangerousness | [calib11-econ] citizen deaths + insanities per year, this year x1, last x1/2 ... 5 years ago x1/32; known to the civilization when its own caravan leaves and at each year start; from 10 a wave comes 1 in 3 ("despite the danger", else "Migrants were too wary to make the journey this season."), from 50 likewise ("brave this terrifying place" / "cursed death-trap") | sum and weights, thresholds 10 / 50 and the messages: w:Migrant; the effect size is not given (forts 6-7 of the reference install: 15 danger arrivals, 44 "too wary"); report by the own caravan: gamelog spring 277 (11 deaths, two plain waves before the dwarven caravan left) | OK (rule) / ESTIMATE (refusal rate, report timing) |
| Pregnancy | married couples, 9 months, twins 1 in 50; [skills9] conception 1 in 36 per couple and month (was 1 in 12) | w:Pregnancy | ESTIMATE (rates); r9: 3 births (2-4) in 2.18 years vs DF's 2 (r8: 6) |
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
| Temperature, freezing, steam | not simulated (the reference world has temperature off) | w:Temperature | MISSING |
| Pumps, screw pumps, power | [gaps10] `power.rs`: screw pumps take liquid from one level below the tile next to the input (at least 2/7) and move all that fits into the tile beyond the output end every tick, never above the pump's level; powered (10) or worked by hand (`OperatePump` job, labor `OPERATE_PUMP`, 600 ticks per job). Power networks: water wheel net +90 when water under it flows (-10 when not), windmill +40/+20/+0 with wind (outside), axles -1 per tile, gear assemblies -5, pumps and millstones -10, rollers -2; gears connect on all 6 sides, axles only along their axis. Millstones mill only while powered. Tests `power::tests` (4) | w:Screw pump, w:Power, w:Water wheel, w:Windmill, w:Gear assembly, w:Axle | FIXED r10 (wind odds, wheel ports, flow freshness, manual job length ESTIMATE; gear disengaging and minecarts MISSING; the reference fort has no machines) |

## minecarts.rs (round 12, `[carts12]`)

Hauling routes, minecart physics, track stops, rollers, wheelbarrows and carried weight. Tests `minecarts::tests` (11), demo `examples/minecarts.rs` on Trussroads (a guided ore cart from z 123 to the smelters' ore pile at z 126, and a pushed cart on a 24-tile track carved beside "Steinlager Sued A").

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| Routes | stops (tile, linked piles take/give, conditions), minecart items as vehicles; a dwarf first brings the cart to stop 1 | w:Hauling | OK |
| Loading | dwarves with the item's HAUL_* labor carry items one at a time from `take_from` piles into the cart, up to 3 at once per cart | w:Hauling | OK |
| Stop item filter | not modelled: a stop loads what the route's later `give_to` piles accept | per-stop item settings (w:Hauling) | ESTIMATE |
| Unloading | dwarves carry items from the cart into free tiles of the `give_to` piles | w:Hauling | OK |
| Capacity | 500 000 cm³; boulder 100 000, log 50 000, bar/block 6 000, meal 1 000, cloth 200 (5 stones, 10 logs, 83 bars, 500 meals, 2 500 cloth) | w:Minecart | OK |
| Other item volumes | gems 1 200, other items 5 000 | not given | ESTIMATE |
| Departure | `Leave when N% full` / `full` / `empty`, `Wait N days`; Push, Ride, Guide with Push/Haul Vehicles (`HANDLE_VEHICLES`); "full" also when waiting items no longer fit | w:Hauling | OK (no-fill default ESTIMATE) |
| Push | the cart goes to the middle of the next tile with 19 990; direction from the condition or along the track to the next stop | w:Minecart | OK |
| Ride | same push, the dwarf rides along and gets off at the next stop | w:Minecart | OK |
| Guide | the cart moves with the walking dwarf, ignores physics (also ramps) | w:Minecart | OK (follows the walking path, not the track: ESTIMATE) |
| Speed units | 100 000 = one tile per tick; tiles 100 000 long, ramps 141 420 | w:Minecart | OK |
| Friction | track 10, floor 200, track stop highest 50 000 / high 10 000 / medium 500 / low 50 / lowest 10, ramp up 4 910, ramp down -4 890 | w:Minecart | OK |
| Corners | turn, -1 000 speed; derail above 50 000 unless a wall blocks the straight way | w:Minecart | OK |
| Track ends | the cart leaves the track straight on, until floor friction stops it, a wall stops it or it falls | w:Minecart | OK (falling carts stop where they land: ESTIMATE) |
| Dumping | entering a track stop with a dump direction drops the contents onto that tile; items fall to the first floor | w:Track stop | OK |
| Rollers | powered (2 per tile) rollers give a slower cart their speed (lowest 10 000 .. highest 50 000) in their direction | w:Roller | OK |
| Collisions, checkpoint effect, fluids in carts, levers/pressure plates on track stops | not modelled | w:Minecart | MISSING |
| Stalled carts | a cart that stops away from a stop is carried by a dwarf to the stop it was heading for | dwarves retrieve carts | ESTIMATE |
| Cart weight | minecarts are 40 000 cm³ of material (oak 28Γ, iron 314Γ); weight only matters where a dwarf moves the cart (guiding, carrying) | w:Minecart | OK |
| Wheelbarrows | piles with `max_wheelbarrows` > 0 take free wheelbarrows; items of 75Γ or more hauled to such a pile go by wheelbarrow at full speed, the wheelbarrow stays at the pile; only used to carry items to its pile | w:Wheelbarrow | OK (wheelbarrows are not carried to the pile first: ESTIMATE; the reference fort's piles all have 0) |
| Carried weight | a dwarf hauling (by hand) an item, carrying or guiding a cart above 75Γ walks `1 + (w - 75) / 300` times slower, at most 4x (granite boulder 1.6x) | slowdown exists, formula not given (w:Weight) | ESTIMATE (construction and workshop fetches are not slowed yet) |

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
| Caged creatures | [calib11-econ] a caged female sharing her cage with a male of her race conceives (1 in 16 per month), gives birth in the cage ("Troglodyte has given birth to a girl."); 50-per-race cap | w:Breeding: caged males can breed, caged females give birth in cages; the reference fort: all of DF's animal births r3 -> r6 (23) were troglodytes held together in cage traps | OK (observed) / ESTIMATE (rate) |
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

Calibrated against the 17 281 thoughts of the 87 citizens in the reference fort's round-5 export
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
| Patrol routes | FIXED r8 | `Squad::patrol` waypoints (`Sim::order_squad_patrol`): a `Patrol` order walks them in a loop, waiting 50 ticks at each (ESTIMATE); patrolling soldiers engage what they see. The export only names the route id, so imported patrols without waypoints still train. `barracks::tests::patrol_walks_its_route`. |
| Training by sparring and demonstrations (sparring injuries, teachers) | FIXED r8, recalibrated r9 (ESTIMATE rates) | Wiki Sparring/Barracks: soldiers train in their barracks; partners side by side spar (a bruise in 1 of 2000 bouts), and a squadmate within 3 tiles who is 2+ levels better gives a demonstration (1 in 10 training rounds). [skills9] Sparring needs Competent Fighting on both sides; the experience amounts are in `skills.rs` (see the validation table). Without a barracks: individual drills. `barracks::tests::soldiers_train_and_spar_in_their_barracks` (sparring teaches more than drills). Lectures are not modelled. |
| Barracks: sleeping, training spot, equipment storage | FIXED r8 | `Squad::barracks` is read from the export (`squad.rooms`; in the reference fort the squad "Wache" has the barracks "Kaserne", the squad "Schuetzen" the archery range). Training spots in the barracks, beds in the barracks for soldiers on duty (ESTIMATE: DF's per-routine sleep mode is not exported), uniform pieces stored in the barracks taken first. `soldiers_on_duty_sleep_in_barracks_beds`. |
| Squad ammunition settings (ammo per squad, combat/training flags) | FIXED r8 | `Squad::ammo` (`Sim::set_squad_ammo`): ammo type, material or class (`WOOD`, `BONE`, `METAL`), amount (split per member, at most a quiver of 50) and combat/training use. Exported since round 9 (Trussroads: Schuetzen 200 iron bolts for combat, 120 wooden for training); without settings the old rule applies (metal for combat, else training ammo). `squad_ammo_settings_pick_ammo_by_use`. |
| Uniforms: pick up missing pieces, `KIND:SUBTYPE@MATERIAL` entries | OK + r8, r10 | Per-position uniforms (`Sim::set_squad_uniform`) and an equipment report per squad (`Sim::squad_equipment`: worn, missing, ammo). [gaps10] The material part is matched: a token (`@INORGANIC:STEEL`), a plain name (`@steel`) or a class (`@Armor`/`@Metal`, `@Leather`, `@Wood`, `@Bone`, `@Shell`, `@Cloth`...); without a material the best item is taken (wiki Uniform: "steel rather than iron rather than copper"), then the nearest. "Partial matches okay" (DF default) substitutes the same item type in another material (the best one) when no exact match exists; "Exact matches only" (`Sim::set_squad_uniform_exact`, session setting, not saved yet) goes without. "Replace clothing" is not modelled. `uniform_materials_exact_and_partial_matches`. |
| Wounded soldiers rest | FIXED r6 | |
| Soldiers leave a fight at the job-cancel thirst/hunger thresholds | ESTIMATE | Wiki: fights are "do-or-die". The rule exists so an unkillable foe cannot starve the whole squad. |
| **Civilian alert** | FIXED r7 | Wiki Burrow: citizens assigned to a burrow "will only accept jobs from within their assigned burrow(s)" and still work. Rustfort r7: inside the alert burrow civilians dig, build, work workshops and farm plots and haul, when the work tile and the needed items lie inside the burrow (one bitset test per candidate). Idle civilians mill about inside.<br>[r8] Drinks, meals and beds inside the burrow come first (outside ones only when there are none inside, ESTIMATE); doctors and Recover wounded treat patients inside the burrow with supplies from inside; levers inside the burrow are pulled; corpses inside are buried in coffins inside. `civilian_alert_prefers_drinks_and_beds_inside_the_burrow`.<br>Still ESTIMATE: depot, broker and trap-reload jobs pause under the alert. |
| Civilians flee hostiles within 12 tiles; cornered civilians fight back | ESTIMATE | [r9] FIXED: a fleeing civilian who needs to drink or eat goes for it when the threat is more than 2 tiles away, and for 2 days only a hostile within 2 tiles makes it flee again (ESTIMATE; DF dwarves cancel jobs on seeing a hostile but drink once it is out of reach; r8 civilians kept fleeing until they died of thirst). `warcal::tests::fleeing_civilian_breaks_off_to_drink`. |

## Invasions

| Item | Class | Notes |
|---|---|---|
| Sieges from a population of 80 | OK | Wiki Siege: "80 members". |
| Checked at season changes, chance 1/3 | ESTIMATE | |
| Siege size | FIXED r9 (`warcal.rs`) | Wiki Difficulty: the tier is the population tier **or** production (created wealth) **or** trade (exported wealth); triggers from the save (`ENEMY_POP/PROD/TRADE_TRIGGER`, Normal 20/50/80/110/140, 5k-300k, 500-30k). r7 needed population **and** wealth, so the reference fort's 87 citizens stayed at "10 regulars".<br>Strength grows per siege of a civilization: `INVASION_DIFFICULTY_FIRST/SECOND/THIRD/SCALING` (Normal 50/300/1000/+500); the count is DF's `entity.siege_tier` from `entities.json` (2 at r4, 4 at r5, 5 at r6 = the gamelog's sieges). Cap `INVASION_UNIT_CAP` 140 without mounts.<br>ESTIMATE: 15 strength per creature (the 5th siege = 133, DF's was 136 goblins + 7 mounts), at least 10, capped by the wiki tier forces (Town 10, City 45, Metropolis 160 -> 140); masters strength/160 % (r6: 12 %), weapon skill masters 10-14, regulars 3-9; r6 gear (iron armor: torso 31 %, cap 54 %, legs 31 %, shield 34 %; weapons iron 47 % / silver 53 %, pikes, long swords, mauls...).<br>Created wealth: `World::fort_history` (seeded with the imported items' value), each made item adds base x material x quality at creation; exported wealth persists there too. |
| Sieges vs ambushes | FIXED r8, r9 | [r9] Wiki Difficulty: `SIEGE_FREQUENCY` 25 % siege "otherwise raid", after `MIN_RAIDS_BEFORE_SIEGE` 2 / `MIN_RAIDS_BETWEEN_SIEGES` 3 raids of that civilization (counted in `World::fort_history`); 1-5 ambush squads of 3-6 arrive with each siege (gamelog: 1, 5, 5, 1, 4 "An ambush!" lines right after Trussroads' 5 sieges) and count as raids. Invasion chance per season 2/3 (ESTIMATE; DF: a siege every 1.25-2 years). [r8] `raids.rs`: an invasion is a siege 1 time in 4 when the siege tier is met, otherwise an ambush (from the first tier on). Ambush squads arrive **hidden**: civilians do not flee, soldiers do not engage, `Sim::hostiles` does not list them, until a citizen spots them (observer vs ambusher skill within a few tiles, ESTIMATE) or they strike: "An ambush! Curse them!". Size ESTIMATE: 1-3 squads of 2-5. `raids::tests::ambush_is_hidden_until_spotted`. |
| Thieves and snatchers (kobold thieves, goblin snatchers) | FIXED r8, calibrated r10 | Raws: kobolds `ITEM_THIEF`, goblins `BABYSNATCHER`. A hidden thief takes the most valuable small item it can reach and runs for the edge (the item is lost); spotted: "Thief! Protect the hoard from skulking filth!" and it flees. A snatcher carries a child off the map ("... has been abducted!", wording ESTIMATE); spotted: "Snatcher! Protect the children!"; killed or knocked out it drops the child. Visit odds ESTIMATE (thief 1/2, snatcher 1/3 per season, random day). 5 tests in `raids::tests`.<br>[gaps10] Entity raws: `ITEM_THIEF` is on the kobolds' `SKULKING` entity, `BABYSNATCHER` on the goblins' `EVIL` entity. Thieves now need a kobold civilization **at war** (the reference fort's neighbour Chulgis is at peace); snatchers come only from the invading goblin civilization before its first siege of the fort (stage rule, ESTIMATE). The reference install's gamelog: Trussroads (68 springs, 16 ambushes, 5 sieges) had 0 thieves and 0 snatchers, now matched; earlier forts in the same install had 28-45 "Snatcher!" alarms each, mostly before or between their first sieges. Test in `raids::tests`. |
| Goblins charge at once; humans sometimes camp at the map edge | FIXED r8 (ESTIMATE times) | Natural and forced sieges gather at their arrival point first (goblins 600-2400 ticks, other races 2-7 days), attack only what comes within 8 tiles, then charge. `Sim::spawn_siege` keeps the old immediate charge for tests. `siege_camps_then_charges_with_monsters`. |
| Monsters and mounts (trolls, beak dogs) in sieges | FIXED r8, r9 | Wiki Siege: +5 monsters at City, +40 at Metropolis: trolls and beak dogs 1:1 (ESTIMATE) with the save's raws (troll 250 000 cm³, beak dog 150 000). [r9] ESTIMATE: one monster per 20 creatures (DF's r6 siege had none); mounts from strength 1 000 on (DF's "mounted" troops from the 3rd siege), one per 20 regulars, of a `MOUNT_EXOTIC` cave creature in the raws (the reference fort's goblins rode 7 voracious cave crawlers, `BUILDINGDESTROYER:2`), else beak dogs. [gaps10] Riding: one regular mounts each mount; mount and rider share a tile and move at the mount's speed, the rider fights from it, the mount is passive while ridden (ESTIMATE); a dead or unconscious mount throws its rider (who walks on), a dead, unconscious or caged rider frees the mount (it fights on its own); a leaving rider takes its mount. `mounted_invaders_ride_and_dismount`, `a_leaving_rider_takes_its_mount`. 25 % of a siege are bowmen (ESTIMATE). |
| Retreat when "sufficiently" beaten or waited out | ESTIMATE + r8 | Half dead or 2 months; [r8] a siege also breaks when its leader (best weapon skill) dies (wiki: killing the leader; ESTIMATE). Thieves and snatchers leave on their own, at the latest after a month. `a_dead_leader_breaks_the_siege`.<br>[r9] FIXED: a raider still on the map 3 days after it started leaving (stuck in the trap corridor, paralyzed, crippled) gives up: it fights on if it can walk and reach somebody, slips away if it can walk but reach nobody, or stays as a plain helpless hostile; in every case it leaves its group, so the invasion ends (r8: the threat never cleared). `warcal::tests::stuck_leavers_leave_their_group`. |
| Invader equipment shares and skills | ESTIMATE | |
| [siege12] Climbing invaders (v50) | FIXED r12 (`climbing.rs`, `nav.rs` `ClimbGraph`/`ClimbNet`) | Wiki Climbing: rough natural walls, constructed walls and fortifications (block walls too), ice and trees can be climbed; **smoothed or engraved natural walls cannot** (smoothed pillars can), carved fortifications cannot; a floor sticking out over the wall top (or a raised drawbridge) stops climbers, a one-tile overhanging wall is climbed from one level below, a two-tile one is safe. Needs a grasping part (invaders only; animals, fliers and `CANNOT_CLIMB` do not). Wiki Jumping: one-tile gaps are jumped. Losing hold: unskilled climbers on walls (not trees) in the air; Climber skill lowers it. Climbing is a gait (speed independent of skill).<br>Pathing: a climber walks when it can (DF: climbing is used when a creature is stuck, wiki on dwarves) and otherwise paths over the climbing graph (A*, shared climbing flows); its targets are found in its *climbing group* (walk components joined by climbing moves), built on demand when a climber finds nobody to walk to, saved with the sim. Sim-generated invaders get Climber 1-3 at the rates of the reference fort's r6 goblins (79 % / 8 % / 13 %).<br>ESTIMATE: climbing step 6x the walking time and path cost; losing hold 20 % x (15 - skill) / 15 per step in the air; fall damage as for retracting bridges (2+ levels knock out, 10+ kill); 1 Climber XP per step. `climbing::tests` (rough, constructed, smoothed, overhanging floor, one-tile overhanging wall, move symmetry). |
| [siege12] Wild animals and fort units climbing | MISSING | Only invaders climb; DF dwarves climb when stuck or chased, climbing animals (cats, cave spiders: `STANCE_CLIMBER`) are not exported. |

## Ranged combat

| Item | Class | Notes |
|---|---|---|
| Launcher + matching ammo, quivers, fire range about 20 tiles, misses fly on | OK | Wiki Marksdwarf. |
| Projectile momentum and armor resolution | OK | Wiki Material science. |
| Fortifications: below Accomplished, fire only from the adjacent tile | OK | Wiki Fortification. |
| Hit, dodge and block chances; rate of fire; ammo breakage | ESTIMATE | |
| Friendly fire (units standing in the line of fire) | MISSING | |
| Squad ammo settings | FIXED r8 | See Military. Hidden raiders cannot be targeted. |

## Siege engines (round 12, `[siege12]` `siegeengines.rs`)

| Item | Class | Notes |
|---|---|---|
| Ballista, catapult, bolt thrower from parts (siege workshop, wood, Siege engineering) | OK | Wiki Siege engine: 3 ballista / 3 catapult parts (3x3), bolt thrower parts + mechanism + chain (1x1). Recipes "Construct ballista/catapult/bolt thrower parts", "Make wooden ballista arrow" (5x5 siege workshop). Engines stand on floors or wall tops and do not block movement. |
| Bolt thrower needs a bin | MISSING | Bins are containers in the storage code; the bin is left out of the construction. |
| Facing: catapults and ballistae fixed, bolt throwers turn | OK | `Sim::set_siege_engine_facing` (before building for ballista/catapult); bolt throwers turn toward their target. |
| One siege operator per engine (Siege operating labor), fetch ammo, load, fire | OK | Catapult: stone boulders (no clay), ballista: ballista arrows, bolt thrower: a whole stack of bolts ("a large amount of ammo internally"). `Task::SiegeOperate`. |
| Load time by skill | ESTIMATE | `base / (1 + skill^2 / 5)`, catapult 4 800 ticks unskilled (wiki: "a whole month ... to load a catapult"), ballista 1 200, bolt thrower 600 per stack; Legendary 104 / 26 / 13. |
| Field of fire 19 degrees, 4 z-levels, ranges, catapult minimum 30 | OK | Ranges: ballista 150, catapult 100, bolt thrower 60. Fire at will: fires while a visible hostile is in the field of fire (line of fire through fortifications; catapults fire over creatures). |
| Accuracy: aim point 60 tiles out, unskilled drift up to 10 tiles, three 0..14 rolls under the skill = straight | OK | Wiki Siege operator table (Novice 18.7 % ... Accomplished 96.3 %, Master 99.2 %, Legendary 100 %), `accuracy_matches_wiki_table`. Fire at will flies along the facing (DF), an aimed shot (`Sim::aim_siege_engine`) through the aimed tile. |
| Ballista arrows hit every unit in every tile the head passes (friendly fire), 5-6 goblin-sized bodies | OK | Through the combat module (momentum, armor, tissue layers). Momentum ESTIMATE 1 500 (edged). `ballista_arrow_hits_a_dwarf_in_its_path_too`. |
| Catapult stone lands and bursts into shrapnel | ESTIMATE | Lands at the triggering hostile's (or the aimed tile's) distance, 30..100; 600 blunt on the landing tile, shrapnel 80 blunt to each neighbour with 50 %. |
| Bolt thrower bolts | ESTIMATE | One bolt per `40 * 10 / (10 + skill)` ticks while a target is in the field, crossbow momentum x1.4, hits the first creature in line. |
| Ammo used up on landing; experience 20 per load, 30 per shot | ESTIMATE | Wiki: "destroyed upon landing, unless it falls a z-level" (the z-level exception is not modelled). |
| Ballista arrows destroying buildings, metal arrowheads | MISSING | |

## Defense (doors, bridges, levers, traps)

| Item | Class | Notes |
|---|---|---|
| Locked or lever-closed doors block everyone; building destroyers smash them | OK | |
| Pet-passable doors: pets path through | MISSING | Pets only wander through. |
| Bridges: raised wall edge, flinging, atom smasher, retracting | OK | |
| Mechanism delay 100 ticks; pulling a lever 10 ticks | ESTIMATE | |
| Linking needs mechanisms and a mechanic job | MISSING | Linking is instant. |
| Traps: hostiles and wild animals trigger them; citizens, pets, merchants and `TRAPAVOID` creatures do not | OK | |
| Unconscious creatures trigger traps, including the fort's own dwarves | MISSING | Wiki Weapon trap. |
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
| Forgotten beast arrival rate | FIXED r9 (ESTIMATE parts) | Wiki: "exactly one beast is spawned per each underground region", attracted by wealth and noise; Difficulty: wealth / `FORGOTTEN_WEALTH_DIV` (40) is irritation, none at or below `FORGOTTEN_IRRITATE_MIN` (5 000), full chance `FORGOTTEN_SENS` (10 000) above. Rustfort r9: at most one beast per cavern layer (`World::fort_history.beast_layers`), chance per season 1/4 x the irritation share (ESTIMATE); cavern noise is not counted. Layers already discovered when a fort is imported count as spent (ESTIMATE: the reference install's gamelog has 0 beasts in Trussroads' whole life since its founding, with 3 caverns open). r8: 1/4 per season with no limit (1-5 per 2 years). |
| Beasts never leave; `TRAPAVOID`; building destroyers | OK | |
| Venom, breath, webs and syndromes | FIXED r6 (simplified) | No resistance or size dilution, no cure; the durations are estimates. |
| Fire breath, dragonfire, secretions, contact syndromes on touch | MISSING | No temperature. |
| **Cave-ins** | FIXED r7 | Wiki Cave-in: tiles not supported along the X, Y and Z axes fall (diagonals, bridges and ramps below give no support). Creatures directly underneath die; the dust knocks out and throws creatures nearby. Rustfort r7 (`caveins.rs`): after digging, channeling or deconstruction, a local flood fill (capped at 4096 tiles) looks for the map bottom or edge; unsupported chunks become open space, leave boulders and a floor below, kill creatures underneath, and the dust knocks out units within 2 tiles.<br>ESTIMATE: falling debris stops on floors; no dust throwing or dust thoughts. |

## [validation] Rules checked against the reference fort's history (round 8)

Measured with `examples/validate.rs`, which replays real DF snapshots of the fort; numbers and
method in `docs/VALIDATION.md`.

| rule | Rustfort | DF (source) | class |
|---|---|---|---|
| XP per job | [skills9] 30 per use (planting, harvesting, building, brewing, cooking...), +30 for a product with quality levels, 10 for mining, smoothing, carving, engraving and stone blocks, 100 Organization per order approval (`skills.rs`) | "Most civilian skills give 30 XP per use, plus 30 more if an item is produced that has quality levels"; stonecutting, smoothing, engraving and mining "only 10 XP per use"; 100 per approved work order (w:Experience). DF's real mining gain (50 XP per dwarf-year) equals 782 dug tiles x 10 | FIXED r9 (work XP 0.68 of DF: the digging and building between the exports was player-driven) |
| Skill level cap | [skills9] no cap: level n -> n+1 needs 500 + 100 n without limit | "no upper limit" (w:Experience); the reference export has ratings up to 150 (Growing) and 126 (Situational awareness) | FIXED r9 |
| Military training XP | [skills9] individual drills train one skill at a time; sparring (both partners Competent in Fighting) trains Situational awareness, Fighting, weapon, Discipline, Wrestling, Armor, Dodging, Shield and the unarmed strikes; watching a bout trains Situational awareness; demonstrations give the student the skill plus Student and Concentration, the teacher Teacher (`skills_train_round`) | w:Training, w:Sparring, w:Situational awareness ("trained just by witnessing a fight (which includes sparring)"); amounts not given | FIXED r9 (amounts ESTIMATE, fitted: r4 -> r5 military XP 15.8k vs DF 14.5k per dwarf-year; SA 6.4k / 6.1k, Fighting 2.8k / 2.6k, axe 1.9k / 1.8k, Discipline 1.8k / 1.7k) |
| Social and mental skill XP | [skills9] conversations train one conversation skill (Consoler for a stressed partner, Pacifier / Intimidator in an argument, otherwise weighted like the reference fort), demonstrations Student / Concentration / Teacher, reading Student / Concentration, poetry performers Speaking, the manager Organization | DF: about 2 000 XP per dwarf-year (concentration, knowledge acquisition, teaching, organization, persuasion...) | FIXED r9 (ESTIMATE amounts; 1 870 vs 1 977 per dwarf-year) |
| Stress after deaths | [skills9] strong bad memories (deaths of relations, witnessed deaths, dead bodies) come back: each day 1 in 6 the strongest of the last two years is relived; [calib11-social] at full strength (half in the second year), from its own memory slot that survives the turnover of the thought log | DF v50 keeps strong memories in 8 short- and 8 long-term slots and relives the strongest each year with an additional stress change (w:Memory (thought)); DF's fort went from 6 to 17 stressed citizens while losing 14 | ESTIMATE rate (r3 -> r4 11 stressed vs DF 6, r4 -> r5 10 vs 17, VALIDATION.md round 11) |
| Hardened to death | [calib11-social] hardening 0-100 (DF `combat_hardened`): from the export (a dwarf that already felt ANYTHING about a corpse or a witnessed death = 100), +3 per horror thought; it is the chance to feel nothing about corpses and deaths seen; hardened dwarves grieve with SADNESS | DF: "felt nothing after seeing ..."; all dead-body and witnessed-death emotions in the reference fort's r3-r5 exports are ANYTHING, all 70 grief emotions in r5 SADNESS | OK (step ESTIMATE) |
| Masterpiece announcements | [calib11-social] every masterwork product or meal: "<name> has created a masterpiece <item>!" (own announcement type, log only) and the maker's MakeMasterwork thought | DF MASTERPIECE_CRAFTED / COOKED_MASTERPIECE (A_D:D_D, no box); each masterwork is a historical event (w:Item quality) | OK (engravings and constructions do not announce yet) |
| Sieges vs fort deaths | r9: r5 -> r6 median 26 of 87 citizens lost (0-42 over 6 seeds), r4 -> r5 median 7 (2-15); r8: 1 and 2 | DF: 14 in 2 sieges (r4 -> r5), 73 in 1 siege (r5 -> r6) | ESTIMATE (closer; see docs/VALIDATION.md) |
| Tree growth | [skills9] `growth.rs`: monthly a sapling matures into a trunk (1 in 330) or dies (1 in 240), a tree seeds a sapling on free soil/grass within 2 tiles (1 in 750) | +65 trees in 2.18 years; of 1 110 saplings 87 matured, 123 died, 104 sprouted | FIXED r9 (rates fitted to the reference fort; +70 trees in the sim, one-tile trunks without crowns) |
| Building destroyers vs workshops | r9: level-2 destroyers (trolls, cave crawler mounts, forgotten beasts) seek buildings within 10 tiles unless a foe is within 3: furniture, workshops, furnaces, depots, doors, floodgates, racks, pumps...; level 1 only statues, windows, targets, slabs, wooden doors/hatches (wiki Building destroyer). Smashing takes 100 ticks (200 for workshops, ESTIMATE) | 17 of 31 workshops gone after the r6 siege | FIXED r9 (invaders deconstructing, `FLAG_NO_DECONSTRUCTING_INVADERS`, still MISSING) |
| Caravans per year, strange moods, artifacts | 4 / 5 / 2-5 in 2.18 years | 4 / 5 / 5 | OK |
| Stockpile fill | 25 % of pile tiles hold items (r4 -> r5) | 37-39 % | ESTIMATE (bin volume) |

## Performance and determinism (round 6, real fort, 2 years with `--standdown`)

- **State hash:** stays the same across repeated runs (`9cfb23265fa36f6b`, twice).
- **Survival:** 88 -> 90 citizens, the same as master.
- **Speed:** about 59.6k ticks/s, against master's 61.4k measured back to back on the same busy machine (4 agents building at once). That is within the noise.
- **Combat cost:** the combat phase stays at about 0.11 us/tick.

## Pathfinding and determinism (round 9, `[perf9]`)

| Item | Class | Notes |
|---|---|---|
| A* path cost (traffic 1/2/5/25 per entered tile) | OK | Unchanged rule (w:Path, w:Traffic). New heuristic: landmarks (ALT, `nav::Landmarks`), 16 per walk component of at least 1 500 tiles, picked by farthest-point selection. Paths stay **optimal** (tests `landmarks_keep_paths_optimal_and_expand_less`, `landmarks_stay_admissible_after_digging`). |
| Which of several equally cheap paths a unit takes | ESTIMATE (changed) | The tie-break is still (f, h, tile), but h is larger now, so among equally cheap paths another one can win. The state hash differs from round 8 for that reason only. DF's own choice among equal paths is unknown. `RF_NO_LANDMARKS=1` switches back to the round-8 heuristic for A/B runs. |
| New stairs, dug tunnels, opened doors, cheaper traffic | OK | The landmark table is repaired at once (decrease-only relaxation from the changed 3x3x3 box, `Nav::lm_relax_box`; traffic: `Nav::lm_cost_changed`), so new shortcuts are used right away, as in DF. Removed moves (walls, locked doors, raised bridges) keep the old values, which stay valid lower bounds. |
| Search effort on the r5 fort | measured | 87 citizens: A* nodes per 1 000 ticks 315k -> 37-50k (6-8x fewer), main-thread cycles per 1 000 ticks 252-354 -> 85 Mcyc. The big searches were construction fetches across 10 z-levels (stockpile at z121, sites at z130-132), not hostiles. |
| Determinism with 1..24 threads and any parallel threshold | OK | Not reproduced: 16 + 12 configurations on r5 and the r6 siege (`examples/determinism.rs`, 12 busy threads), 6 rounds of all `*deterministic*` tests run concurrently, and the new `tests/determinism_stress.rs` all gave equal hashes at every checkpoint. Checked and ruled out: the parallel A* phase is a pure function per search (thread-local tables never affect results), no time-based decisions (all `Instant`s only feed statistics), no `HashMap` iteration whose order changes results, no global state. Hardened: A* never lets f drop below the popped f (pathmax), so an inconsistent heuristic can no longer lose queue entries. |

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
| Labor > Work details: list with DF's `WORK_DETAIL_*` icons, the modes Everybody / Only selected / Nobody (`LABOR_WORKER_ASSIGNED*`), per-dwarf toggles, "assigned workers only do this" | OK | Observed in the reference fort in DF 53.16 via DFHack (read-only): the 11 built-in details (Miners ... Siege operators) with their icons, modes and labors, plus one custom detail. |
| Built-in details keep their labors | OK | DF's `no_modify` flag is set on the 11 defaults. Custom details get the labor editor and Delete. |
| Clicking a dwarf in an Everybody or Nobody detail | ESTIMATE | Switches to "Only selected" with everybody else, or with only that dwarf. DF's exact click behavior was not checked. |
| "Set up DF's default work details" for exports without them | ESTIMATE | Assigns the "only selected" details to dwarves who already have all of their labors. |
| Kitchen: cook/brew per item kind and material (`LABOR_KITCHEN_*`) | OK | Rows come from the fort's stock (DF lists the same kinds).<br>Cooking is allowed for edible plants, seeds, drinks, meat, fish, eggs, cheese, milk, flour and fat. Brewing is allowed for brewable plants and fruit (raws). |
| Plump helmets: no cook | ESTIMATE (player setting) | A player rule of the reference fort. It is added at start only when the save has no kitchen entry for them. |
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
## Player screens (round 9, rf-app `[app9]`)

| Item | Class | Notes |
|---|---|---|
| Announcement flags per event: `BOX` (box that pauses), `P`, `R`, `D_D` | OK | DF's `announcements.txt` (its header documents the flags). Read-only from `%APPDATA%/Bay 12 Games/Dwarf Fortress/prefs/announcements.txt`, then the install's `prefs/` and `data/init/`. The reference install's prefs differ from DF's defaults: `BOX` on CITIZEN_DEATH, MIGRANT_ARRIVAL, AMBUSH_THIEF, AMBUSH_SNATCHER, CARAVAN_ARRIVAL, CITIZEN_TANTRUM, BERSERK_CITIZEN, MOOD_BUILDING_CLAIMED (`BOX:R`), ARTIFACT_BEGUN; Rustfort follows the player's file. |
| Which sim line belongs to which DF token | ESTIMATE | The sim sends text and a kind, no token: lines are matched by DF's wording (e.g. "Thief! Protect the hoard..." -> AMBUSH_THIEF, "has been abducted" -> CITIZEN_SNATCHED). Deaths are split by the dead unit: citizen -> CITIZEN_DEATH, tame -> PET_DEATH, others (invaders, wildlife) no box. |
| Siege, ambush, hostile-wildlife boxes | ESTIMATE | No DF token was identified for "A vile force of darkness has arrived!" and "An ambush! Curse them!" (AMBUSH_AMBUSHER is not certain): Rustfort keeps the box + pause of earlier rounds; wildlife only gets a line. |
| Petition box | ESTIMATE (Rustfort) | DF v50 shows petitions as an alert and in the petitions window; Rustfort also opens a box with Accept / Deny (switch it off in the settings). |
| Box queue | OK / ESTIMATE | One box at a time like DF; folding the fifth and later boxes of one type into "... and N more" is Rustfort's (a siege can kill dozens). |
| Raid boxes name the raid | OK | The sim's spotted lines are DF's wording (wiki: Ambush, Thief, Snatcher). Hidden raiders stay invisible until spotted (round 8). |
| Petitions screen | OK | Residency (performers, scholars), citizenship (mercenaries, monster slayers), temple/guildhall petitions; 28-day timeout is the sim's ESTIMATE. |
| Squad uniform per position: slots, item type, material | OK | DF's uniform screen assigns per position. [gaps10] The material (`@INORGANIC:STEEL`, `@METAL`, `@LEATHER`, ...) filters, with DF's exact/partial match rule (see the military table). |
| Squad ammunition: item, amount, combat/training | OK | DF squad ammo screen (`squad_ammo_spec`). New entries get 100 rounds (ESTIMATE; DF proposes the squad's need). |
| Patrol routes picked on the map; barracks per squad | OK | DF v50 patrol routes and barracks zones; the sim walks the route in a loop (round 8). |
| Combat reports per fight | ESTIMATE | DF keeps per-unit combat reports of an encounter. Rustfort groups lines that share a participant (by the names the sim writes) and are at most 600 ticks apart; the sim's log has no ticks or unit ids, so lines are stamped when the UI polls them. |
| Skill names and levels | OK | DF wiki "Skill": profession-style names (Miner, Wood Cutter, Axe Dwarf, Observer), levels Dabbling ... Legendary +5, next level at 500 + 100 x level experience. |

## Round 8 combat check (real fort, `raids` example and 2-year bench)

- Forced raids on the r5 export (`cargo run -p rf-sim --release --example raids -- --kind ... --standdown`):
  - **Ambush** (8 goblins, hidden): spotted after about 800 ticks ("An ambush! Curse them!"); all 8 killed (one caged); 1 citizen lost.
  - **Thief** (kobold): spotted at the gate ("Thief! Protect the hoard from skulking filth!") and struck down by the Wache before it stole anything.
  - **Snatcher** (goblin): spotted ("Snatcher! Protect the children!"); it fled off the map without a child.
  - **Siege** with a camp, 20 goblins and 6 monsters: camped 2000 ticks, then charged. 2 trolls were caged, 3 beak dogs and a troll killed, the leader died and the rest retreated: 18 killed, 1 citizen lost.
- `--break-leg 4`: all 4 citizens with a broken lower leg were carried to beds (4 rescues).
- 2 years with `--standdown` and invasions on: 87 -> 91-93 citizens. 1 siege (camped), 2 ambushes, 2 thieves (2 items stolen unseen), 3 snatchers (no child lost); soldiers spar in the Kaserne.
- **Speed**, back to back with the round-7 build at low machine load: 53-61k ticks/s against 54k, the same within noise. The combat phase costs 0.17-0.19 us/tick. State hashes are equal with 1 and 6 threads, also with invasions on.

## Round 13 contract (`[contract13]`)

Seams only: no rule is simulated yet; each module owner adds its own section below.

| Item | Class | Notes |
|---|---|---|
| Season label "Early/Mid/Late <season>" (`Sim::season_label`) | OK | wiki Calendar: each season has three months (early, mid, late). |
| Degrees Urist to Celsius (`Sim::urist_to_celsius`) | OK | wiki Temperature: U = F + 9968 (10 000 U = water freezes). |
| Region climate `World::climate` (biome, temperature, swing, rainfall, savagery, evil/good, reanimating, evil weather) | MISSING | Not in the export yet: all fields default to unknown; `weather` assumes 10 050 U (ESTIMATE) until filled. wiki Weather, Surroundings. |
| Unit curse markers `Unit::undead`, `curse`, `curse_race`, `true_name` | MISSING | Fields only (wiki Undead, Vampire, Werebeast, Necromancer); set by `undead` / `vampires`. |
| Entity attitude `EntityInfo::attitude` | MISSING | Field only (wiki Diplomacy); set by `diplomacy`. |
| Hauling route stop item filter `HaulStop::filter` | MISSING | Stored and editable through the API; minecarts do not apply it yet (wiki Hauling route: stop conditions and desired items). |
| Moon phase (`Sim::moon_phase`) | MISSING | Stub returns 0; wiki Werebeast (full moon). |

## Round 13 weather and climate (`[weather13]`)

Module `crates/rf-sim/src/weather.rs`; API in `round13_api.rs` (`weather_summary`, `weather_at`, `temperature_at`, `snow_at`, `evil_weather`, `set_weather_enabled`, `force_weather`, `force_evil_weather`, `weather_report`).

| Item | Class | Notes |
|---|---|---|
| Surface temperature = region mean + seasonal swing (coldest mid winter, warmest mid summer) | OK / ESTIMATE | wiki Temperature, Weather. The curve shape (per-month share of the swing) is an ESTIMATE. |
| Unknown climate (export has none yet) | ESTIMATE | Mean 10 020 U (52 F), swing 30 U: outdoor water freezes in early winter and thaws in early spring. Known mean without swing: 30 U. |
| Underground keeps the mean, magma 12 000 U | OK / ESTIMATE | wiki Temperature, Magma. No heat sources, no per-tile temperature diffusion (MISSING). |
| Daily rain roll (1 day in 5 outside winter, 2-8 hours) | OK (Rustfort round 8) | Moved from `lifestyle::weather_day` into `weather_precip_day`, same RNG stream and draws, so the "Rain" thoughts are unchanged. Known `climate.rainfall` scales the chance to rainfall/250 (ESTIMATE). |
| Snow instead of rain when the surface is below 10 000 U; winter snow roll | OK / ESTIMATE | wiki Weather, Snow. Winter snow: 1 day in 5 (own stream, ESTIMATE). Snow gives no Rain thought. |
| Snow cover lies until the temperature rises above freezing | OK / ESTIMATE | wiki Snow. Kept in weather state (no Tile field); no depth, no tracks. |
| Clouds and fog | ESTIMATE | Overcast 3 hours before precipitation and on 1 day in 4; morning fog on 1 day in 12 (1 in 4 in wet biomes / rainfall >= 66) for 2-6 hours. Fog has no gameplay effect yet (vision MISSING). |
| Outdoor water freezes below 10 000 U; ice thaws above it | OK / ESTIMATE | wiki Ice. Only the topmost water tile of an outdoor column freezes (ESTIMATE); 4/7+ becomes an ice wall, shallower an ice floor (ESTIMATE). River sources do not freeze. Thaw restores depth and salt/stagnant flags, plus any water that ran onto an ice floor meanwhile; magma on the ice keeps the tile. Ice dug out into an ice floor melts back to the old floor without water; ice built over stays. Items frozen into an ice wall are not moved (MISSING). |
| Creatures in water that freezes die | OK | wiki Ice ("encased in ice"). Only in deep (wall) ice in Rustfort, only at the moment of the frost (swimmers usually climb out of small pits within a few ticks); counted in `frozen_deaths`. |
| Rain refills outdoor murky pools | OK / ESTIMATE | wiki Murky pool. +1/7 per pool tile with chance 1/4 every 100 rain ticks (ESTIMATE). |
| Evil weather: rain over the whole surface or a drifting cloud | OK / ESTIMATE | wiki Evil weather. 1 day in 8 in evil regions with `climate.evil_weather`, 2-6 hours; cloud radius 4-8, drifts 1 tile per 100 ticks (ESTIMATE). Effect class BLOOD/HUSK/THRALL/SYNDROME from the name; the effects themselves belong to `[undead13]`. |
| Indoors / underground | OK | `weather_at` is Clear under a roof; snow only on outdoor dry floors. |
| Sim change on the reference fort | expected | With the default climate the fort's outdoor water now freezes in winter and thaws in spring, which changes the bench state hash (b98debc359a168d5 -> f64c6ebe3055a8e6). Checked on Tileshtulon: frost in Early Winter, snow cover in winter, thaw in Early Spring, save/load across the frost IDENTICAL. |
## Undead and necromancy (`[undead13]`)

Module `crates/rf-sim/src/undead.rs` (tests `undead_tests.rs`). Off unless the region reanimates (`World::climate.reanimating`, not yet filled by the importer), the player/test toggle `Sim::set_reanimation_enabled` is on, a necromancer siege is enabled or spawned, or evil weather with a husk/thrall effect is active. The reference fort is in none of these, so its bench hash is unchanged and `undead_hash` mixes nothing.

| Item | Class | Notes |
|---|---|---|
| Corpses rise in reanimating regions | OK / ESTIMATE | wiki Surroundings (evil, "reanimating"), Undead. Loose corpses rise as hostile undead of their race; corpses in a building (coffin), container, inventory or construction stay down. Chance 1 in 8 per 100-tick pass per corpse is an ESTIMATE (DF raises corpses within days). Corpses that were not tracked when their unit died (reanimation was off) find their unit by the corpse name; imported corpses take their race from the `CREATURE:RACE:...` material. |
| Severed body parts rise | ESTIMATE | wiki Undead: severed limbs and heads animate on their own. Rustfort raises a part as a small undead of the race (1/10 body size); DF gives it the part's own body. |
| Destroyed undead can rise again | OK | wiki Reanimation: the remains of a destroyed undead reanimate again in an evil area. |
| Undead traits `NOT_LIVING`, `NOPAIN`, `NOSTUN`, `NOFEAR`, `NO_EAT/DRINK/SLEEP`, `PARALYZEIMMUNE`, `OPPOSED_TO_LIFE` | OK / ESTIMATE | wiki Undead, Creature token. Undead, husks and thralls never pass out (`knock_out`); undead and husks do not bleed and ignore brain/heart wounds; nerve paralysis is ignored. `NOBREATHE`: undead and husks do not drown or suffocate. As `Kind::Other` hostiles they have no needs and do not flee. A unit turned husk stops bleeding from its open wounds and wakes at once. Hostile to all living (opposed to life) through the hostile role. |
| How an undead is destroyed | ESTIMATE | DF: an undead dies when its head/upper body is destroyed or severed. Rustfort: head severed or knocked off, or total wound severity on upper/lower body >= 40 000 (4 fully damaged layers). |
| Necromancers raise the dead | ESTIMATE | wiki Necromancer (`RAISE_DEAD`-style interaction). A living necromancer on the map raises the nearest risable corpse within 12 tiles (same z +/-1), chance 1 in 2 per pass; slain necromancers raise no more. |
| Necromancer sieges | OK / ESTIMATE | wiki Necromancer, Siege: a necromancer leads an army of undead; uses the regular siege machinery (`raids::spawn_raid_ex`: edge, camp, charge, leader). Announcement "The dead walk. Hide while you still can!" (DF wording). Natural sieges only if the world has a necromancer tower entity (kind or name names a necromancer/tower; the export has no site types, so detection is an ESTIMATE) or `Sim::set_necromancer_sieges(true)`; chance 1 in 4 per season once the fort meets the siege tier, size half the regular siege (10..=60) are ESTIMATES. Every 10th undead is flagged intelligent (ESTIMATE). Undead do not retreat when the siege breaks; they stay as a horde (wiki: undead never flee). |
| Husks and thralls from evil weather | OK / ESTIMATE | wiki Evil weather, Husk, Thrall: units outside under a cloud/rain with effect `HUSK` or `THRALL` (`Sim::evil_weather`, owned by weather) turn hostile; they leave the fort and their squads. Exposure chance 1 in 2 per 100-tick pass is an ESTIMATE (DF: on contact, after a syndrome delay). Husks are `NOT_LIVING` (no bleeding); thralls only `NOSTUN/NOPAIN`. |
| Intelligent undead speech, mummies, ghosts, undead from necromancer experiments | MISSING | |
| Threats list (`Sim::threats`) | OK | `NecromancerSiege` per active siege, `UndeadHorde` for all other undead/husk/thrall hostiles. |
| Moon phase (`Sim::moon_phase`) | ESTIMATE | Implemented by `[vampire13]` (see its section below): 28-day cycle per month, full moon on the 15th. |

## Vampires and werebeasts (`[vampire13]`)

Module `crates/rf-sim/src/vampires.rs` (tests in `vampires_tests.rs`); one hook line in `combat.rs` (`pursue`, after an attack). Wiki pages: Vampire, Werebeast, Night creature, Justice, Calendar.

| Rule | Class | Notes |
|---|---|---|
| Vampires arrive hidden in migrant waves; never children, babies, starting dwarves or caravans | OK | wiki Vampire. Only adult dwarf migrants of a wave can be picked. Visitors/diplomats as vampires: MISSING. |
| Chance per migrant wave | ESTIMATE | 1 in 12 (wiki: no number; "many forts see one within the first few years", DF lets the difficulty settings change it). Player toggle `set_vampires_enabled`. |
| False identity: assumed name in `Unit::name`, real name in `Unit::true_name`; the true name returns when unmasked | OK | wiki Vampire (a revealed vampire's corpse bears its original name). The cover identity's memorial/coffin split: MISSING. |
| No eating, drinking or sleeping; no aging; sterile | OK | wiki Vampire. Need timers are held at zero every 100 ticks, apparent age frozen, pregnancy cleared. Alcohol withdrawal (a DF bug) and the "can't remember the last time" blood thought: MISSING. |
| More and higher skills | ESTIMATE | wiki Vampire ("4-5+ social skills", many Novice+ skills). Existing skills +2..5, 4-6 social skills at 2..7. |
| Feeds on sleeping citizens, usually killing them; survivors remember nothing | OK / ESTIMATE | wiki Vampire. A drained victim dies with no blood left ("has been drained of blood"). Interval 28-55 days, kill chance 2/3, a survivor loses 30 % of its blood (all ESTIMATE). Feeding on unconscious creatures and animals: MISSING. |
| Victim choice and approach | ESTIMATE | The least observed sleeper in the vampire's walk area, then the nearest. The walk is not simulated: the vampire appears beside the sleeper. |
| Witnesses unmask the vampire | OK / ESTIMATE | wiki Vampire ("a witness to an attack uncovers them permanently"). Each awake citizen within 6 tiles on the same level sees it with chance 1/2 (ESTIMATE). Unmasked: true name, announcement, the crime stands accused. |
| A death without a witness is an unsolved Murder; interrogation may name the vampire (suspect) | OK | wiki Justice ("a corpse with no suspects"), through the existing `justice` investigation. A witnessed feeding that does not kill is an AttemptedMurder (ESTIMATE). |
| Accusing a unit (`accuse_vampire`) | ESTIMATE | True and unmasks only for a real vampire; open crimes of the vampire become accusations. DF's convict/interrogate screens and false accusations by vampires: MISSING. |
| Convicted vampires get a normal sentence | OK | wiki Vampire; uses the `justice` sentences unchanged. A jailed vampire does not feed. |
| Moon phase 0..=27, full moon = 14 | ESTIMATE | One 28-day cycle per DF month (12 x 28 days), so the full moon is on the 15th of every month. wiki Werebeast/Calendar give no exact phase table. |
| Werebeast curse: turns into its were-form at the full moon, back the next day | OK / ESTIMATE | wiki Werebeast. Duration one day (ESTIMATE). In were-form the unit drops its gear and is run by combat as a hostile; afterwards it returns to work and fighters drop it as a target. |
| Were-form race and body | ESTIMATE | DF generates were-creatures per world; Rustfort uses `WERE<animal>` from 10 animal bases. Were-forms keep their body; attackers get a big humanoid body (80 000 cm3), strength 2250, toughness/endurance 2000, walk speed 700. |
| Werebeasts from the wilds attack at a full moon and leave afterwards | ESTIMATE | Chance 1 in 36 per full moon (about one every 3 years), only while invasions are on and the fort has at least 20 citizens (first invasion tier, wiki Difficulty; ESTIMATE). Player toggle `set_werebeasts_enabled`. |
| Hidden werebeast visitors | ESTIMATE | A new visitor carries the curse with chance 1 in 60 and turns at the next full moon. |
| A bite passes the curse on; the victim turns at the next full moon | OK / ESTIMATE | wiki Werebeast. Every wounding melee attack of a were-form on a non-animal counts as a bite (ESTIMATE); the curse holds if the victim survives to the next 100-tick pass. |
| Threats panel and unit markers | OK | `threats()`: unmasked free vampires, transformed werebeasts. `unit_curse()`: vampire, suspected, unmasked, werebeast, transformed. |
| Reference fort (r5, 2-year bench) | ESTIMATE | Default on. All rolls use their own seeded streams, so other systems' RNG is untouched; the hash changes only if a vampire or werebeast actually appears. |
## Megabeasts, semi-megabeasts and titans (`[mega13]`)

Module `crates/rf-sim/src/megabeasts.rs` (tests in `megabeasts/tests.rs`). wiki pages: Megabeast, Semi-megabeast, Titan, Dragon, Hydra, Roc, Bronze colossus, Building destroyer, Difficulty, Fire.

| Item | Class | Notes |
|---|---|---|
| Only living megabeasts of the world come, each at most once | OK | wiki Megabeast. Historical figures of megabeast races in the export form the pool (dead ones, those that came and those already on the map, e.g. caged, are excluded); historical figures of `TITAN` races come as titans with their own name. |
| Pool when the world list is not exported | ESTIMATE | Races from the save's creature raws flagged `MEGABEAST` / `SEMIMEGABEAST` (vanilla table dragon, hydra, roc, bronze colossus, giant, ettin, cyclops, minotaur if no raws), plus generated titans. |
| Arrival trigger: third enemy tier of population, created and exported wealth together | OK | wiki Megabeast / Difficulty: Normal 80 citizens and 100 000 created and 10 000 exported wealth, from the save's `ENEMY_*_TRIGGER`; titans ignore exported wealth (then only a titan can come). Off with `FLAG_MEGABEASTS=true`, with invasions off or by `set_megabeasts_enabled(false)`. |
| `MEGABEAST_INTERVAL` | ESTIMATE | Counted in seasons since the last attack (or the import); chance 1/2 per eligible season; one megabeast at a time. Kind odds: megabeast 35 %, semi-megabeast 40 %, titan 25 %. |
| Arrival at a map edge connected to the fort, announcement "The <race> <name> has come! ..." | OK | wiki Megabeast. Megabeasts hunt the fort until slain (no retreat). |
| Sieges and megabeasts: no double counting | OK / ESTIMATE | A megabeast is not a siege or raid: `World::fort_history.sieges/raids` are untouched and its marker civ (`MEGA_CIV`) is never taken as an invading civilization (`raids.rs`). No megabeast arrives while a siege is on the map (ESTIMATE). `threats()` lists megabeasts, titans, forgotten beasts and revealed sieges/ambushes separately. |
| Titans: generated per world like forgotten beasts | ESTIMATE | wiki Titan: random body (quadruped or humanoid), size 2-8 M cm³, half with a shell of stone/gem/metal; a third breathe fire, the rest roll a forgotten-beast power (`syndrome::roll_power`). Names from own syllables (DF uses its language files). |
| Building destroyer | OK | Vanilla megabeasts/semi-megabeasts except the roc are `BUILDINGDESTROYER:2` (`defense::DESTROYERS` / raws); titans level 2 via `mega_destroyer_level`. Wrecks count in `MegabeastReport::buildings_destroyed`. |
| Dragonfire | ESTIMATE | wiki Dragon: breathes fire. No temperature/fire model yet: target 2-10 tiles away in line of fire, the target and creatures next to it catch fire for 200-400 ticks, one burn wound (sev 2 500, pain 200) every 50 ticks; death at `3 + size/100 000` burns (dwarves 3). One breath per 300-600 ticks. `FIREIMMUNE` creatures and breathers are not hurt. No burning items, grass or buildings (MISSING). |
| Body sizes and attributes without raws | OK / ESTIMATE | Vanilla adult `BODY_SIZE`s: dragon 25 M, hydra 8 M, roc (`BIRD_ROC`) 20 M, bronze colossus 20 M (bronze shell), giant 9 M, ettin/cyclops 8 M, minotaur 0.22 M cm³ (OK); hydra and bronze colossus `NOPAIN` (OK); natural skills 6, strength/toughness 1 500-2 500. |
| Slaying: announcement with slayer, fort history, thoughts | OK / ESTIMATE | "The <race> <name> has been slain by <slayer>!"; kept in `Sim::megabeast_history()`. Thoughts `SlewMegabeast` (slayer, strong positive) and `MegabeastSlain` (other citizens, mild) are ESTIMATE (DF: killed a megabeast thought, plus fame for the slayer, MISSING). |
| Caught in a cage trap | OK | wiki Megabeast (caged megabeasts can be trained). Recorded as caught (`MegabeastInfo::caught`), not slain: no slaying announcement or thoughts, no longer a threat. |
| Megabeasts leaving after some time, flying (roc, dragon), multiple hydra heads | MISSING | They walk and fight to the death. |
## Diplomacy (`[diplo13]`)

Module `crates/rf-sim/src/diplomacy.rs`; hooks in `trade.rs` (caravan arrival, committed trade, goods unloading) and `lib.rs` (`kill`, `fell_tree`). Runs once a day (`DIPLO_DAY_PHASE`); switch off with `set_diplomacy_enabled(false)`.

| Rule | Class | Notes |
|---|---|---|
| Outpost liaison travels with the fort's own civilization's caravan and meets the host noble a few days after arrival | OK | wiki Outpost liaison. Delay 1-3 days is ESTIMATE. |
| Envoys (liaison, diplomats) still come when the caravan finds no usable trade depot and leaves at once | ESTIMATE | wiki Outpost liaison, Trade depot; the envoy travels with the caravan but its meeting does not need the depot. |
| Host = land holder, else mayor, else expedition leader; nobody to receive the envoy -> meeting "Missed", small attitude loss | OK / ESTIMATE | wiki Diplomat, Noble. The penalty (-5) is ESTIMATE. |
| Liaison asks for requests; accepting makes a one-year trade agreement, requested goods come in double numbers | ESTIMATE | wiki Outpost liaison (requests raise the chance/amount of goods). DF has a per-item request screen with priorities; Rustfort matches request words against caravan goods kinds/subtypes and doubles their count. |
| Elevation to barony / county / duchy at the liaison meeting, in order, when population AND created wealth AND exported wealth meet `LAND_HOLDER_POP/PROD/TRADE_TRIGGER` | OK | wiki Noble (land holder), Barony; difficulty settings read from the save, vanilla defaults 20/100 000/10 000 etc. otherwise (the defaults are ESTIMATE where the save carries none). |
| Land holder appointed (baron -> count -> duke, same unit promoted) | ESTIMATE | wiki Land holder: DF lets the player choose / the monarch appoints; Rustfort promotes the existing holder or appoints the host. No arrival of an outside noble. |
| Diplomats of other civilizations only come once the fort has a land holder | OK | wiki Diplomat. |
| Elven tree quota per year (`TREE_FELL_COUNT`, `_SAVAGE` when savagery >= 66), every felled tree counts; exceeding it angers the elves, more each week | OK / ESTIMATE | wiki Diplomat, Tree. Quota is set even if refused (refusing costs attitude). Weekly extra penalty is ESTIMATE. Savagery threshold depends on `World::climate` (0 = unknown until weather13's importer fills it). |
| Human diplomats demand tribute when their attitude is negative; paid as gifts to their caravan before the deadline | ESTIMATE | wiki Diplomat (tribute demands). Amount formula (500 + 50 x displeasure + exports/100) is ESTIMATE. |
| Attitude -100..100: + profitable trades and gifts, - merchants/visitors (-10) and envoys (-40) killed by the fort (struck down by a fort dwarf, or crushed by a drawbridge; deaths to goblins, cave-ins or wounds bled out later are not held against the fort), refused demands, broken quotas, unpaid tribute; seasonal drift to a baseline | ESTIMATE | DF tracks this inside the entity; the numbers are ESTIMATE. |
| War declaration at attitude <= -50: relation War, caravans stop (`trade_partner`), their merchants on the map leave at once, envoys/demands voided | OK / ESTIMATE | wiki Diplomacy, War. Threshold ESTIMATE. |
| War parties of a civilization that declared war here (1 in 3 per season, 3-7 units, ambush raid) | ESTIMATE | wiki Ambush, Siege. Goblin sieges stay with `raids`. |
| Peace offer: non-goblin civ at war for at least a year with attitude > -30 sends an envoy in its caravan season; accepting restores trade | ESTIMATE | wiki Diplomacy (peace). |
| Unanswered demands: tree quota imposed, tribute counts as refused, requests and peace offers lapse after 14 days | ESTIMATE | |
| Elevation announcement wording, meeting screen dialogue | MISSING | Short own wording; DF's meeting conversation not reproduced. |
## Minecarts round 13 (`[perf13]`)

| Rule | Class | Notes |
|---|---|---|
| Per-stop item filter (`HaulStop::filter`, `set_stop_filter`) | OK / ESTIMATE | wiki Hauling route ("desired items" of a stop). Uses the stockpile filter syntax: category must match, `*` takes a group, `!X` excludes; type-like groups match kind/subtype, `quality_*` the quality, others the material token or DF's built-in lists (WOOD, COAL, glass, ash). Item colors are ignored (ESTIMATE). A stop without a filter keeps the round-12 rule (load what a later stop's piles take). |
| Guided carts follow the track | OK / ESTIMATE | wiki Minecart (guiding): the guide walks the track tiles to the next stop (corners turn, junctions go straight). Falls back to the walking path when no track leads there or a track tile is not walkable (ESTIMATE). |
| Cart hits cart | ESTIMATE | wiki Minecart (collisions happen; no formula published). 1-D elastic collision by load weight when a cart enters a tile with another cart on the ground and is faster along the line of travel; the hitter stays on its tile; a hit idle/stopped cart starts rolling, unless dwarves are working on it (load, unload, fetch): then the hitter just stops. |
| Cart hits creature | ESTIMATE | wiki Minecart (fast carts knock creatures down and hurt them). From 25 000 speed: creatures on the entered tile are unconscious for momentum/50 ticks (10..1 000); the cart loses momentum as if hitting 60Γ. Injuries are MISSING. |
| Checkpoint effect, cart-on-cart derailing | MISSING | |

Speed work in round 13 (`[perf13]`, no rule change, the state hash stays the same):

| Change | Class | Notes |
|---|---|---|
| Cavern detection (`detect_caverns`) on a bitset | OK | One linear pass marks open cavern tiles (tiletype shape looked up per tiletype, not per tile); the flood fill clears bits instead of a `Vec<bool>`. Same regions in the same order; about 4.5x faster (450 -> 90 Mcyc per season on r5). |
| Item scans prefiltered by production class | OK | Container scans (`rebuild_containers`, `bring_containers`) and the reaction census only compare kind strings for items whose class can match; the food census asks the brew/cook tests only for food classes. |
| Profile counters | OK | `--features prof`: production pass and per-tick pass sub-steps; coarse counters use the thread's own cycles on Windows (rdtsc counts wall time, which a busy machine inflates). |
## Round 13 interface (`[ui13]`)

Interface only: it shows what the round-13 modules report through `round13_api.rs` and sends the
player's answers back. The rules themselves are the module owners' sections.

| Item | Class | Notes |
|---|---|---|
| Weather and temperature in the top bar | ESTIMATE | DF v50 shows the weather and temperature on the top bar (wiki Weather, Temperature). Rustfort writes degrees Celsius from degrees Urist (U = F + 9968); DF's own wording of the temperature line is not copied. |
| Rain, snow, fog, evil rain/cloud drawn over outside tiles | ESTIMATE | wiki Weather, Evil weather: precipitation falls only on tiles open to the sky; evil clouds drift over a region. The look (streaks, flakes, veils) is Rustfort's; DF Premium draws its own weather sprites, which are not used. |
| Snow cover on the ground | ESTIMATE | wiki Weather: snow settles on the surface in cold seasons. Drawn as a white veil where `Sim::snow_at` is true. |
| Moon phase names (28-day cycle, 14 = full) | ESTIMATE | wiki Werebeast: transformation at the full moon. The eight phase names are common English, not DF text. |
| Diplomacy screen: civilizations, relation, attitude, wars | ESTIMATE | DF v50 has no single diplomacy screen; the wiki Diplomacy and Civilization pages describe relations, wars and envoys. The layout is Rustfort's; data from `Sim::diplomacy_entities`. |
| Demands: tree quota, tribute, peace, liaison requests; Accept / Refuse | OK | wiki Diplomat (elven tree quota, tribute, peace talks), Outpost liaison (requests for next year's caravan). The answer goes to `Sim::answer_demand`; consequences are the diplomacy module's. |
| Liaison request categories | ESTIMATE | wiki Outpost liaison: the requests list item categories. The 18 categories shown are a short list, not DF's full request screen with per-item quantities. |
| Meetings with envoys (diplomat, liaison, elevation, land holder) | OK | wiki Diplomat, Outpost liaison, Nobles (barony/county/duchy elevation). Shown as a log. |
| Threats screen (megabeasts, titans, forgotten beasts, sieges, undead, necromancers, werebeasts, vampires) | ESTIMATE | DF has no threats screen; it announces arrivals (wiki Megabeast, Forgotten beast, Siege, Necromancer, Werebeast, Vampire). Rustfort lists them with Go buttons. |
| World event toggles (weather, reanimation, megabeasts, vampires, werebeasts, diplomats) | ESTIMATE | DF sets these per world (worldgen) or in d_init; Rustfort lets them change in game. |
| Unit sheet curse markers; accusing a suspected vampire | ESTIMATE | wiki Vampire (drained corpses, interrogation and conviction unmask them; the true name shows after), Undead, Necromancer, Husk, Thrall, Werebeast. DF has no "suspected" marker; Rustfort shows the module's suspicion and the accusation goes to `Sim::accuse_vampire` instead of DF's interrogation in the justice screen. |
| Alerts: demand box (pauses), Demands / Threats badges, status lines | ESTIMATE | DF pauses and shows a box when a diplomat or liaison wants to meet (wiki Diplomat); the badges are Rustfort's. |
## Machines and mechanisms (round 14, `[machines14]`, `linkage.rs`, `machinery.rs`)

Spec: `docs/mechanics/machines14.md`. Wiki pages (v50): Lever, Pressure plate, Machine component, Mechanism, Door, Hatch cover, Floodgate, Bridge, Grate, Bars, Gear assembly, Power, Water wheel, Windmill, Screw pump, Roller, Minecart, Cage, Restraint, Support.

| Rule | Class | Notes |
|---|---|---|
| Lever sends its new state; plates send "on" at once and "off" 99 ticks after the condition ends (no repeat "on") | OK | w:Lever "On/Off states", w:Pressure plate "Operation". |
| "On" opens doors, hatches, floodgates, grates, bars; raises/retracts bridges; retracts spikes; disables track stops | OK | Fixes the inverted doors/hatches/spikes of round 4 (F1, F2). Linked doors and hatches are shut until "on". |
| Gear assemblies toggle on every signal; a disengaged gear transmits nothing; gears do not block movement | OK | w:Gear assembly. Two signals in one tick = two toggles = no change (ESTIMATE). |
| Reaction times: doors, hatches, gears, cages, chains, supports 0; spikes 40; bridges, floodgates, grates, bars 100; track stop off 0 / back on 40 | OK | w:Lever "Activation of the devices", plate timing table. |
| Refractory: delayed devices ignore signals until the change took effect | OK | w:Lever. Spikes treated like the other delayed devices (ESTIMATE); track stops not refractory (ESTIMATE). |
| "On" and "off" in the same tick toggle the device | OK | w:Lever. |
| Plate-before-device one-tick build-order offset | MISSING | Every device acts as if built after the plate (ESTIMATE, matches levers). |
| Plate creature trigger: weight sum in [min, max], citizens/tame only with the setting, merchants, diplomats and TRAPAVOID never, unconscious always | OK | Weight = body size; worn equipment not added (ESTIMATE). Visitors do not trigger (ESTIMATE). Webbed creatures: MISSING (no web state). |
| Plate water / magma depth range, minecart gross weight range on track | OK | Cart weight = cart + load + rider body size / 1000 (ESTIMATE). Fast carts trigger in every tile they enter. |
| One-use plates fire once, deconstruct and destroy their mechanisms | OK | Plates without exported trigger flags count as creature plates that reset (ESTIMATE, old exports). Default range 10 000-2 000 000 (ESTIMATE: DF's dialog defaults are undocumented). |
| Plates re-scanned every 100 ticks for creatures that arrived without walking | ESTIMATE | Safety net for flung, fallen and spawned creatures. |
| Link job: a Mechanic carries 2 mechanisms (oldest first), installs the 1st in the target, the 2nd in the trigger | OK | w:Lever "Linking". `LINK_WORK` 100 ticks per end, scaled by Mechanics skill (ESTIMATE); XP per end (ESTIMATE). Waiting jobs whose trigger or target is gone are dropped. Stand tile: on doors/hatches (locked ones cannot be linked), else the target or a neighbour. No signal on completion. |
| Linkable: doors, hatches, floodgates, bridges, spikes, grates, bars, gears, track stops, cages, chains, supports | OK | Cages release their occupants and come apart, chains release, supports come down (cave-in check) and free their tile. Grates and bars take their nav state from their `OPEN` flag (imported open grates are passable, closed floor grates/bars are floors, player-built ones block once built, removed ones free their tiles). The chained creature's chain item and lever ownership: MISSING. |
| Items jam doors and floodgates; floodgates close by themselves once clear (checked every 10 ticks, ESTIMATE), doors need a new signal | OK | w:Door, w:Floodgate. An "on" to a jammed floodgate cancels the pending close. Creatures holding a door open: MISSING. |
| Closing doors/floodgates and lowering bridges destroy the liquid on their tiles | OK | w:Door, w:Bridge. Raising bridges do not move items yet (MISSING). |
| A creature of size 1 200 000+ stops a bridge; lowering onto it deconstructs the bridge | OK | w:Bridge. |
| Track stops are `Trap`/`TrackStop` for the cart physics, with the exported friction | OK | Fix of a round-12 bug (stops never braked). |
| Carts stopped by closed doors, raised bridges; lowered bridges as crossings | MISSING | Needs `cart_next` (owner of the cart physics). |
| Power ports: pumps also below (stacks), windmill 12-tile ring, wheel beside its centre, long rollers across only, floors block vertical links | OK | w:Power, w:Screw pump, w:Windmill, w:Water wheel, w:Roller. Wheel ends and above the centre kept (ESTIMATE). Gears on stairs: MISSING. |
| Water wheel needs 4/7 flowing water | OK | Flow = water moved there in the last 200 ticks (ESTIMATE). Persistent edge-drain flow mark and rivers: MISSING. |
| Screw pump: 49-tick run-on, newest first, obsidian into the other liquid, pumped water clean, open intake column | OK | Run-on counted from the last powered balance (balances every 100 ticks, ESTIMATE), capped at 49 ticks after an off-cadence power loss (gear disengaged). Output tile blocking, pressurised output, operator on the input tile: MISSING. |
| Hanging machines collapse when their support is removed or a gear disengages | MISSING | |
| Windmill wind per fort (not per day) | MISSING | Waits for `[weather13]` climate. |
| Millstone job cancel on power loss, lever owner and repeat, frozen machines | MISSING | |
| Machines and devices built from the UI (`new_cons` recipes) and the missing build options (lever, plate, bridge, floodgate, hatch, grates, bars, support, chain) | OK | Recipes from the wiki infoboxes (`machinery::mech_cons`). ESTIMATE: screw pumps need no enormous corkscrew, bridges at most 4 items, track stops 1 material + 1 mechanism. Built cages and placement rules (floodgate on unflooded floor, bridge perimeter, orthogonal support): MISSING. |
| Exporter: gear disengaged, roller speed/direction, pump direction, machine power, flowing-water tiles | MISSING | Spec section 11; DFHack field names to be checked first. |
| UI: link target picker (filters, badges, hover outline, mechanism count), link jobs, plate settings and live state, pending/jammed device states, power network overlay (colour per net, port links, +P/-U, disengaged gears, pump arrows, "no flow" wheels), route view polish (stop friction, rollers, track plates, cart-blocking devices, cart speed arrows), lever and plate sprites | OK | `ui/machines14.rs`; `--ui m14-link,m14-plate,m14-power,m14-routes`. |
## DF audio (`[audio14]`)

Music, ambience and sound effects read at runtime from the player's DF install (`data/sound`,
`data/vanilla/vanilla_music/objects/{music,sound}_standard.txt`, active mods, `init.txt`); no
DF file is copied. Spec: `docs/mechanics/audio14.md`. Sources: w:Audio, w:Soundtrack,
w:Settings#Audio, the raws, DF's init files and release notes. Audio is presentation only: the
sim's cue buffer is off by default, never saved or hashed and uses no RNG (bench hash unchanged).

| Item | Class | Notes |
|---|---|---|
| MUSIC raws: FILE, CARD, EVENT, CONTEXT, FREQUENCY; SOUND raws: FILE, ANNOUNCEMENT, SAVAGE_AREA | OK | w:Audio; parsed at runtime (vanilla + the world's active mods; later objects with the same id replace earlier ones: ESTIMATE). Text outside brackets is a comment. |
| Mod `SOUND_FILE` / `MUSIC_FILE` identifiers | OK | w:Audio "Identifiers" (50.09); they win over the folder naming rules. |
| File id -> file on disk | ESTIMATE | DF hard-codes it; Rustfort derives it from folder and file names (`tracks/<dir>`, `_Full`, `_<n>` cards, `tracks/adv/<n>_<Title>`, `sounds/<name>`), never from the executable. All vanilla ids resolve on 53.17. |
| Contexts ANY, MAIN, SPRING..WINTER | OK | w:Audio. Title screen plays ANY only (ESTIMATE simplification of "seasons also hold in menus"). |
| FIRST_YEAR / SECOND_YEAR_PLUS | OK rule, ESTIMATE data | Fort age from `Meta::founded` (new exporter field `founded_year/tick`, unverified DFHack field names, -1 when absent). Unknown founding: older than a year; synthetic forts count from sim tick 0. |
| CAVERNS_OPENED | ESTIMATE | Any non-magma cavern layer with revealed tiles. |
| Events SIEGE, MEGABEAST_ATTACK, FORGOTTEN_BEAST_ATTACK | OK | From the announcements (`round9::classify`); necromancer sieges by wording (ESTIMATE). |
| Events JUST_EMBARKED, FIRST_CAVERN_OPENED, FIRST_GHOST, FORT_LEVEL, LOST_FORT | ESTIMATE | Transitions of `Sim::audio_facts` seen by the app; the first snapshot after a load is the baseline, so loading never fires them. LOST_FORT = living citizens drop to 0 (Rustfort has no abandon). |
| DEATH_SPIRAL | ESTIMATE | 5 citizen deaths within 2 months, then a season's cooldown. |
| TAVERN_MUSIC_PRESENT | ESTIMATE | A music performance in a tavern or temple (the only places the sim performs) with >= 5 idle listeners, at most once a season. |
| TAVERN_DANCE_PRESENT | MISSING | The sim does not tell dance from music. |
| Event songs override the current song, priority order, 1.5 s fade | ESTIMATE | w:Audio says event songs override; the order LOST_FORT > SIEGE > FB = MEGABEAST > DEATH_SPIRAL > FIRST_CAVERN > FIRST_GHOST > FORT_LEVEL > TAVERN > JUST_EMBARKED is a guess. |
| FREQUENCY weights 10 / 5 / 2 | OK | w:Audio ("half as often", "1/5 as often"). No immediate repeat (ESTIMATE). |
| Song gap uniform in avg x [0.75, 1.25] | OK at 240 s, ESTIMATE elsewhere | init comment: 240 gives 180-300 s. |
| Cards: all in order the first time, later 1-3 random from a shuffled deck, then the song | ESTIMATE | 50.0x release notes + w:Audio. Neutral `cards/Card_N` as interludes with p = 1/4 (ESTIMATE). |
| Title music `TITLE_MUSIC` (default DWARF_FORTRESS), classic `song_title`/`song_game` fallback | OK | init + folder. |
| Volumes 0..255 linear, `starting-volume.txt` (song 0.85), `SOUND:NO` mutes | OK values, ESTIMATE curve | init files and the adventure table; until the player changes a volume or the Mute box in Rustfort, DF's own values are used (a DF install with master 0 starts silent; `SOUND:NO` starts muted and the Mute box can unmute). |
| SOUND stingers from announcements, 2 s de-duplication, 8 per frame | OK where the sim announces; ESTIMATE limits | Class -> DF token table in `audio/sfx.rs`. Lone thieves/snatchers have no sound (raws bind only the supported variants). |
| FEATURE_DISCOVERY (cavern break), STRUCK_DEEP_METAL (adamantine) | ESTIMATE | Not announced by the sim; detected by the app (new cavern layer; first dug adamantine tile per session). |
| `sounds/alert.ogg` for hostile alerts without their own sound | ESTIMATE | No raw object binds it. |
| DEMON_ATTACK, WEDDING, supported-thief AMBUSH | MISSING | No sim source. |
| SAVAGE_AREA howls and giant steps every 45-150 s | ESTIMATE, dormant | Needs `World::climate.savagery` (not exported yet, [contract13]). |
| Ambience beds (`ambience/`): biome, neutral winds, evil/good/terrifying, blizzard, thunderstorm, cavern, magma, river, siege, combat, tavern, depot, workshop | ESTIMATE | w:Audio prose, release notes and 53.17 FMOD parameter names; every threshold is a guess. Silent layers pause and resume where they left off (50.0x notes: OK). Ducked to 0.6 under music (ESTIMATE). |
| Mining sounds (53.17: positional, muffled, per tile type) | MISSING as files | They exist only in FMOD banks; Rustfort plays `audio/actions/dig.ogg` positionally (z gain 1/0.45/0.2, 10-tile hearing distance from `spatialization.txt`, low-pass one z away, at most 3 mining voices at once): ESTIMATE. |
| UI button sounds (53.17) | MISSING | FMOD only; not substituted yet. |
| Walla (crowd), adventure-folder fallback ambience and one-shots | MISSING | Optional in the spec; not done. |
| Extra combat hits (`audio/combat/melee`) | Rustfort extension | Off by default (Settings > Audio). |
| Device policy | Rustfort | Only `play` without `--screenshot`/`--script`/`--shots-at`, `--mute`, `RUSTFORT_MUTE=1`, Mute or master 0 opens a device; bench, savebench and tests never do. |
## Temperature, fire and magma (`[fire15]`)

Code: `crates/rf-sim/src/fire15.rs` (+ `fire15/heat.rs`, `items.rs`, `tiles.rs`, `burn.rs`), public API `fire15_api.rs`, overlay `rf-app/src/ui/fire15.rs`; spec `docs/mechanics/fire15.md`. This section supersedes the rows "Temperature, freezing, steam" (fluids table), "Fire, burns, temperature" (combat table) and the temperature part of "Fire breath, dragonfire ..." (military table). Everything except the obsidian edge cases, the magma-safe test and the magma-workshop check only runs when temperature is on (`settings.temperature`, DF `FEATURE_TEMPERATURE`, or the player override `Sim::set_temperature_enabled`); the reference fort has it off.

| Rule | Class | Notes |
|---|---|---|
| Material heat points (SPEC_HEAT, IGNITE, MELTING, BOILING, HEATDAM, COLDDAM, MAT_FIXED_TEMP) | OK | From the export (`materials.json` `heat`, now kept in `MaterialInfo::heat`). Fallback for older exports/synthetic worlds: raws templates and the vanilla inorganic MELTING/BOILING/SPEC values (w:Material definition token). |
| Heat transfer: per tick (env - T) / SPEC_HEAT, fractions kept | OK | w:Temperature. Fixed point 16.16, truncation toward zero (ESTIMATE); reproduces the wiki's lignite example exactly (517 ticks to 11440 in magma). Only "hot" items are stepped. |
| Environment: magma 12000, underground 10015, outside = weather API, fire 10508 -> 10708 | OK | w:Magma, w:Temperature, w:Fire; water never above 10180 (ESTIMATE). Outside temperature comes from `Sim::temperature_at` ([weather13]). |
| Fixed-temperature materials (nether-cap 10000) | OK | Never change, never ignite from ordinary fire; survive magma (w:Magma-safe). |
| Containers pin their contents' environment | OK | w:Temperature. |
| Fire-safe = stable at 11000, magma-safe = stable at 12000 | FIXED r15 | w:Fire-safe, w:Magma-safe. `fluids::magma_safe` used "any stone"; granite (11440) etc. now melt. Also used by the legacy (temperature off) magma path. |
| Items melt, boil, ignite at their points | OK | w:Material definition token. Liquids (drinks) only boil. MISSING: "glob of molten metal" items (they vanish anyway, w:Magma). |
| Burning item: equilibrium ignite + 190 | ESTIMATE | w:Fire "about 200 above, slightly below". |
| Burning wear 10 points/tick, 806 400 per level: gone after exactly 322 560 ticks | OK | w:Wear, w:Material definition token (materials without HEATDAM). |
| Heat/cold damage wear 10 points per degree above HEATDAM / below COLDDAM per tick | ESTIMATE | Rate not on the wiki; wood burns up in ~0.6 day, in magma in ~1/6 day. |
| Artifacts never wear (burn forever), but melt/boil | OK | w:Wear, w:Fire. |
| Water puts out burning items (not in bins), units and tiles | OK / ESTIMATE | w:Fire "Artifacts", firebreaks; depth >= 1 (ESTIMATE). |
| Grass, shrub, sapling and tree tiles burn; spread to the 8 neighbours on the same level only; trees carry fire upward | OK | w:Fire, w:Grass. Spread chance 1/8 per 10 ticks, fuel 200/300/100/1200 ticks (ESTIMATE). |
| Ramps, constructions, roots, water-covered tiles never burn | OK | w:Fire, w:Grass ("grassy slopes will not catch on fire"). Dry and dead grass burn (ESTIMATE). |
| Burnt grass -> ashes, burnt trees -> `TreeDead*`, burnt shrubs removed | OK / ESTIMATE | w:Grass, w:Tree; tiletype by name (ESTIMATE). MISSING: tree collapse, dead-tree rejuvenation. |
| Dwarves avoid fire | ESTIMATE | Burning tiles get restricted traffic while they burn (w:Fire "Dangers"; DF's avoidance is not documented). |
| Rain/snow put out outside fires | ESTIMATE | 1/4 per 10 ticks; inactive until [weather13]'s `weather_at` reports rain. |
| Units catch fire on burning ground (1/2 per 10 ticks, 1-3 stance parts) and next to burning units (1/10) | ESTIMATE | w:Grass "a chance each turn", w:Fire. |
| Burn wounds layer by layer from the outside; fat above its melting point (10078) melts and bleeds; charred head/upper body kills | OK / ESTIMATE | w:Temperature, w:Magma crab, w:Material science. Severity 1000 per 10 ticks (5000 in magma), bleeding by the cut formula, per-part self-extinguish 1/30 (ESTIMATE). Wound flags BURNED / MELTED. |
| Immunity is material-based; FIREIMMUNE alone does not protect | OK | w:Creature token FIREIMMUNE; magma crab chitin 13000 is magma-proof. Races found in magma at load stay immune. |
| Units in magma: immediate immolation, death by burns | OK / ESTIMATE | w:Magma "nearly-instant immolation". Temperature off keeps the legacy 10-tick kill. |
| Burned-to-death units: the corpse keeps burning (no remains) | OK | w:Fire "Ghosts". |
| Heat emitters (fire man FLAME 10800) ignite the 3x3 around them | OK | w:Fire man, w:Creature token FIXED_TEMP. Death burst: MISSING. |
| Smoke: from every fire, ttl 700 ticks, "Smoke" thought underground at most every 2 days | ESTIMATE | w:Smoke. MISSING: pathing quirks in smoke. |
| Steam | OK (cosmetic) | w:Steam: harmless. Counted (`fire_report().steam_puffs`, `fluid_report().steam`). |
| Obsidian: water on a magma flow tile only makes steam | OK | w:Obsidian, w:Magma flow. Runs with temperature off too. |
| Obsidian casting kills creatures (caged ones survive), destroys non-magma-safe items, entombs magma-safe ones | OK / ESTIMATE | w:Obsidian. Every non-magma-safe item in the tile is destroyed (ESTIMATE). MISSING: per-biome LAVA stone, pond-zone free-fall rule. |
| Magma melts natural ice walls (7/7 water) | OK | w:Ice, w:Magma. Ice floors: MISSING. |
| Magma workshops cancel their jobs when magma drops below 4/7 | OK | w:Magma forge. Checked every 100 ticks (ESTIMATE for "even for a moment"); temperature-independent. Moody claimers going insane: MISSING. |
| Dragonfire / heat blast | OK (API) | `Sim::heat_blast(p, temp)` for [mega13]: items heat to `temp`, grass burns out, units ignite unless FIREIMMUNE_SUPER or immune at that temperature (w:Dragonfire). Cone geometry is [mega13]'s. |
| Buildings melting/burning (doors, floodgates, bridges, pumps), magma mist, warm-stone dig cancel, cold damage/frostbite, fire imp and magma crab attacks | MISSING | Spec P3. |
| Temperature off | ESTIMATE | DF would do no temperature damage at all; Rustfort keeps the legacy magma behaviour (10-tick kill, instant destruction of non-magma-safe items). Evaporation with temperature off is a [fluids] question (see the spec). |
| Burnt (dead) tree tiles do not catch fire again | ESTIMATE | Verifier fix: `TreeDead*` tiles keep material TREE, so two of them re-ignited each other forever. DF dead trees can stand for years (w:Tree); whether they burn again is not on the wiki. |
| A unit catches fire / takes damage when its outer layer burns, heat-damages **or melts** at the source temperature | OK | Verifier fix: a creature with a metal (e.g. bronze, melting 11868) shell was neither magma-proof nor ignitable and lived in magma forever. |
| Fixed-temperature (nether-cap) and SPEC_HEAT-NONE items leave the hot set | OK | Verifier fix: they never reach the ambient value, so they used to stay in the per-tick hot set forever after any magma or fire contact (perf leak, hash churn). |
| Dragonfire ignites nether-cap (items and tree tiles) | OK / ESTIMATE | Verifier fix (w:Dragonfire "ignites organics including nether-cap"). A heat blast of >= 50000 U ignores the fixed-temperature protection (ESTIMATE: threshold); the item then burns at its fixed temperature. |
| Magma reaches flesh under a magma-safe shell | ESTIMATE | Verifier fix: a creature whose outer layer is magma-safe but whose inner layers are not was neither magma-proof nor ever ignited, so it lived in magma forever. In magma the first harmed layer burns (heat soaks through; DF's per-layer heat flow is not documented). |
| Item scan for heat emitters | ESTIMATE | Perf: with only heat emitters (fire men) and no real fire, loose items near them are picked up every 100 ticks instead of every 10. |
## [prefs15] Preferences, item quality and item value

Spec: [docs/mechanics/prefs15.md](mechanics/prefs15.md). Code: `rf-sim/src/prefs.rs`,
`item_value.rs`, `mood_prefs.rs`, `prefs15_api.rs`; UI `rf-app/src/ui/prefs15.rs`. A/B switch
`Sim::set_prefs_enabled(false)` restores the old preference-free rules (item values stay v50).

| Rule | Source | Class | Notes |
|---|---|---|---|
| Unit preferences from the export (`units.jsonl: preferences`, round 15) | w:Preferences | OK | `rf_core::Unit::preferences`; exporter reads `soul.preferences` read-only. |
| Derived preferences when the export has none: one stone, metal, gem, alcohol and hated vermin always; wood 1/5, glass 1/7, leather 1/3, ..., color 1/2, shape 1/5, item groups 1/3 (clothing 1/7), domestic / wild creature 1/2, tree 1/50, plant 1/10, forms 1/2, foods (meat 1/2, cheese 1/10, plant 1/7, powder 1/5, seed 1/7) | w:Preferences | ESTIMATE | Wiki chances; seeded by unit id + facets (not the sim seed). Stone/metal/gem/wood drawn over the world counts of the vanilla raws (86/26/127/trees) so a liked material is often "not found here"; facets only seed the RNG (wiki ties none). Fish, extracts, growths, pearl not drawn. Only dwarves. |
| Quality roll dice `rand(0..N)` inclusive | w:Skill (item quality formula) | OK | Was exclusive (F2): every roll 1-3 points low. |
| +10 points per matching item preference and per matching material preference | w:Skill | OK | Workshop products (`skills_job_done`). Item match: DF item type + subtype ("" = any); material: exact palette index. |
| Engraving quality | w:Engraving | ESTIMATE | Keeps the old estimate roll, no preference bonus (the wiki does not name engravings among the formula's skills). |
| Item value `floor(base x mat x {1, 1.1, 1.2, 4/3, 1.5, 2, 20}) + {0, 3, 6, 10, 15, 30, 300}`, x stack | w:Item value, w:Item quality/Table | OK | Was the pre-v50 x1..x120 table (F3). Used for new products, caravan goods, created wealth, trade fallback, artifacts. Changes created wealth and therefore siege/migrant triggers. |
| Base values (log/boulder 3, bar/block/tanned hide/shoes/gloves 5, thread/rough 6, cloth 7, cut gem/traction bench 20, window/statue 25, mechanism 30, cauldron/minecart/wheelbarrow/stepladder 50, rest 10) | w:Item value | OK | Checked against the reference export (spec 4.1). Millstone 25: ESTIMATE. |
| Engravings and room tiles keep the old multipliers | w:Engraving, w:Zone | OK | `trade::quality_mult`. |
| Prepared meal value `(1 x mult + bonus) + 2` per unit | w:Item value | ESTIMATE | Rustfort tracks no ingredient list. |
| Decoration value: base 20 (cut gem) / 10 at the decoration's quality | w:Decoration | OK | Only mood artifacts carry decorations so far. |
| Artifact value `floor(base x mat x 20) + 300` + each decoration at artifact quality | w:Item value, export | OK | Measured on the export's artifacts. |
| Admiring furniture: no quality gate; pleasure from the value in the dwarf's eyes, `value x (1 + matches)` (item type, material) | w:Preferences, w:Item quality | OK / ESTIMATE | Severity = perceived value as in the export. Chance 1/7 per daily think near furniture, 1 day cooldown: ESTIMATE (calibration target 2.5 per citizen per 28 days). Caption tiers 50/250/1000: ESTIMATE. Image shapes, decorations, maker race, dye: MISSING. Sculpture garden ("arranged"): MISSING. |
| AdmireArt need met by admiring | w:Personality need | OK | Engravings near the dwarf: MISSING. |
| Satisfied at work only with a liked material, item or creature; never with CHEER_PROPENSITY <= 24 | w:Thought, w:Personality facet | OK | Hooks: workshop products, digging/felling/gathering (tile material), engraving. Once a day per dwarf: ESTIMATE. Animal jobs, construction, cooking: MISSING. |
| GoodDrink only for a liked booze; GoodMeal if liked or worth >= 20; level by value 5/25/100/200/400; EatGoodMeal likewise; SameFood/SameBooze never for liked food | w:Thought, w:Personality need, w:Kitchen | OK / ESTIMATE | A meal counts as its main material (no ingredients). Liked food gets at least level 1. "4 in a row" kept at Rustfort's 3: ESTIMATE. |
| AcquireObject met by storing a trade good in a stockpile or bin; AcquiredItem thought if the need was unmet | w:Personality need | ESTIMATE | Depot hauling: MISSING. Thought only when the need was below 0: ESTIMATE. |
| BeExtravagant met by putting on a worn item of quality >= 1 | w:Personality need | OK | |
| Mood skill names `BONECARVE`, `CUTGEM`, `ENCRUSTGEM` | w:Strange mood, export | OK | Were misspelt (F6): bone carvers and jewelers never got their own mood. |
| Mood artifact type from the wiki list of the skill; a liked item type on the list always wins | w:Strange mood | OK | Picked at mood start (DF: at the end, unless forced): ESTIMATE-equivalent. "Any weapon/trap component" resolve to one subtype; † entries weigh 0.1. Perfect gems: MISSING. |
| Bone carvers use shells iff they like a shell | w:Strange mood | OK | Was a 1/3 estimate. |
| Metalworkers demand a liked metal the fort has smelted (not melted) | w:Strange mood | OK | Adamantine/divine metal, cloth and glass classes from preferences: MISSING. |
| Decorations never repeat the base type; miners, engravers, masons and stonecrafters never ask for blocks | w:Strange mood | OK | Glass only if produced, macabre bones, jeweler single gem: MISSING. |
| Rooms in the owner's eyes, liked caged creatures, pets, noble mandates/demands from preferences, art images, ArtDefacement, item decorations | w:Preferences, w:Mandate, w:Demand, w:Pet | MISSING | Spec P4/P5; `prefs15_api::unit_likes_item` is ready for owners. |
| Creature preference pools read the exporter's DFHack creature flags (`HAS_ANY_VERMIN_HATEABLE`, `HAS_ANY_COMMON_DOMESTIC`, `HAS_ANY_DEMON`) as well as raw tokens; demons and (without prefstring data) speaking races are never liked | w:Preferences, w:Hateable | OK / ESTIMATE | Verifier fix: the exports carry only `HAS_ANY_*`, so before it no dwarf hated vermin and dogs/cats counted as wild. Excluding `INTELLIGENT_SPEAKS` when the export has no `HAS_PREFSTRING`: ESTIMATE. |
