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
| Prone and crawling with broken legs | MISSING | Units with a broken stance limb still walk; see Recover wounded below. |
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
| Patrol routes | MISSING | Treated as Train. |
| Training by sparring and demonstrations (sparring injuries, teachers) | MISSING | Training is flat experience every 100 ticks (ESTIMATE). |
| Barracks: sleeping, training spot, equipment storage | MISSING | |
| Squad ammunition settings (ammo per squad, combat/training flags) | MISSING | Not exported. Ammo is chosen by launcher class: bolts for crossbows, metal ammo first for combat. |
| Uniforms: pick up missing pieces, `KIND:SUBTYPE@MATERIAL` entries | OK | "Replace clothing", "exact matches" and partial matches are not distinguished. |
| Wounded soldiers rest | FIXED r6 | |
| Soldiers leave a fight at the job-cancel thirst/hunger thresholds | ESTIMATE | Wiki: fights are "do-or-die". The rule exists so an unkillable foe cannot starve the whole squad. |
| **Civilian alert** | WRONG | Wiki Burrow: the civilian alert is "missing" in v50.07+; citizens assigned to a burrow "will only accept jobs from within their assigned burrow(s)" and still work. Rustfort's alert makes civilians mill about in the burrow with no work at all.<br>Not fixed: it needs burrow filtering in the job, haul and production searches (economy modules). |
| Civilians flee hostiles within 12 tiles; cornered civilians fight back | ESTIMATE | |

## Invasions

| Item | Class | Notes |
|---|---|---|
| Sieges from a population of 80 | OK | Wiki Siege: "80 members". |
| Checked at season changes, chance 1/3 | ESTIMATE | |
| Siege size | WRONG | Wiki: set by fort wealth (from 10 regulars at Town to 120 regulars + 40 monsters at Metropolis). Rustfort: 10 + population/8, at most 60. |
| Sieges vs ambushes | MISSING | Wiki: 25 % of invasions are sieges on normal difficulty (50 % on hard); the rest are smaller ambushes. Rustfort only has sieges. |
| Thieves and snatchers (kobold thieves, goblin snatchers) | MISSING | |
| Goblins charge at once; humans sometimes camp at the map edge | OK for goblins / MISSING camps | Rustfort sieges are goblin sieges, which charge as on the wiki. |
| Monsters and mounts (trolls, beak dogs) in sieges | MISSING | 25 % of a siege are bowmen (ESTIMATE). |
| Retreat when "sufficiently" beaten or waited out | ESTIMATE | Half dead or 2 months. Leader death has no effect. |
| Invader equipment shares and skills | ESTIMATE | |

## Ranged combat

| Item | Class | Notes |
|---|---|---|
| Launcher + matching ammo, quivers, fire range about 20 tiles, misses fly on | OK | Wiki Marksdwarf. |
| Projectile momentum and armor resolution | OK | Wiki Material science. |
| Fortifications: below Accomplished, fire only from the adjacent tile | OK | Wiki Fortification. |
| Hit, dodge and block chances; rate of fire; ammo breakage | ESTIMATE | |
| Friendly fire (units standing in the line of fire) | MISSING | |
| Squad ammo settings | MISSING | See Military. |

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
| **Recover wounded**: immobile or injured dwarves are carried to a hospital zone or the nearest free bed | MISSING | Doctors treat unconscious patients where they fell. Patients with a broken leg walk to bed themselves (WRONG). Next item to do. |
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
| Forgotten beast arrival rate | ESTIMATE / likely too high | Rustfort: 1/4 per season, one at a time, once a cavern is discovered (5 in 2 years on the real fort). Wiki: "exactly one beast is spawned per each underground region", attracted by wealth. |
| Beasts never leave; `TRAPAVOID`; building destroyers | OK | |
| Venom, breath, webs and syndromes | FIXED r6 (simplified) | No resistance or size dilution, no cure; the durations are estimates. |
| Fire breath, dragonfire, secretions, contact syndromes on touch | MISSING | No temperature. |
| **Cave-ins** | MISSING | Wiki Cave-in: tiles not supported along the X, Y and Z axes fall (diagonals, bridges and ramps below give no support). Creatures directly underneath die; the dust knocks out and throws creatures nearby. Rustfort never checks support after digging, channeling or deconstruction. |

## Performance and determinism (round 6, real fort, 2 years with `--standdown`)

- **State hash:** stays the same across repeated runs (`9cfb23265fa36f6b`, twice).
- **Survival:** 88 -> 90 citizens, the same as master.
- **Speed:** about 59.6k ticks/s, against master's 61.4k measured back to back on the same busy machine (4 agents building at once). That is within the noise.
- **Combat cost:** the combat phase stays at about 0.11 us/tick.
