# Rustfort validation against Gordon's real fort (round 8)

This file measures how close Rustfort's simulation is to **real DF**, using Gordon's own
history. We have real DF snapshots of the same fort (Tileshtulon / Trussroads) at different
dates. Rustfort starts from an earlier snapshot, runs for exactly the in-game time until a
later snapshot, and the same census is applied to the simulated end state and to the real one.

- Tool: `crates/rf-sim/examples/validate.rs` (+ `validate_support/gamelog.rs`), read-only.
- Real events come from DF's `gamelog.txt` (read only), real speed from the export timestamps
  and a few read-only `dfhack-run` reads of the calendar. Nothing in DF was changed or paused.
- Verdicts: **in range** = the real value lies within the seeds' min..max; **close** = the
  median is within 25 % (or 2) of the real value; otherwise **LOW** / **HIGH** (median below or
  above). `sim/DF` is median / real.

## Method

### Snapshots

| export | DF date (calendar at export end) | written | citizens | notes |
|---|---|---|---|---|
| `tileshtulon-r1` | 273, tick 339 444 (Opal) | 09.10. 23:35:14 | 91 | oldest format |
| `tileshtulon-r2` | 274, tick 188 204 (Galena) | 10.10. 00:10:14 | 89 | |
| `tileshtulon-r3` | 275, tick 294 557 (Timber) | 01:02:24 | 90 | 35 goblins of a running siege; **no settings.json** |
| `tileshtulon-r4` | 277, tick 22 298 (Granite) | 02:09:11 | 88 | no engravings exported yet |
| `tileshtulon` (r5) | 279, tick 94 209 (Felsite) | 05:13:09 | 87 | 16 246 engravings |
| `tileshtulon-r6` | 280, tick 150 232 (Malachite) | 06:21 | **15** | taken during the siege that broke the fort |

Intervals used: **r4 -> r5** (878 311 ticks = 2.18 years, the main comparison), r3 -> r4
(534 141 ticks = 1.32 years) and r5 -> r6 (459 223 ticks = 1.14 years).

### What is compared

One function (`measure`) is applied to the real end export and to the simulated end world,
always relative to the start export:

- **Population:** citizens at the end, children, citizens lost (start citizens not alive at
  the end), new citizens split into births (younger than the interval) and migrants,
  hostiles on the map, animals.
- **Stress** of the citizens at the end, in DF's 7 buckets (cut-offs -50k/-25k/-10k/10k/25k/50k).
- **Skills:** experience gained per dwarf and year by the dwarves present at both ends, using
  DF's level table (level n -> n+1 needs 500 + 100 n, w:Experience), grouped into work,
  military and social skills, and per skill.
- **Items:** stock (stack units) per group at the end, and **new items**: DF item ids only grow,
  so items with an id above the start export's highest id were made (or split off a stack)
  during the interval and still exist. The same holds in Rustfort (`next_item_id`).
  Quality of the new durable goods (all, and crafts alone).
- **Wealth:** sum of item values (DF's exported value; for items Rustfort made, the trade
  formula of `trade.rs` without contents).
- **Buildings** by kind, **tile changes** against the start map (dug walls, new and removed
  constructions, smoothed tiles, engravings, trees), **stockpile fill** (share of stockpile
  tiles holding an item) and loose items outside piles.
- **Events** (from the sim's reports, and for DF from `gamelog.txt`): sieges, ambushes,
  migrant waves, births, deaths, caravans, strange moods, artifacts, tantrums, forgotten beasts.

### Player emulation

Gordon and his scripts gave orders between the snapshots (work orders from his food watch,
digging, smoothing, new zones, uniforms). Rustfort only has what the start export contains.
The runs therefore use:

- `--standing-orders`: the sim's fallback orders (brew when drinks are low, cook, barrels,
  pots), the closest equivalent of Gordon's food watch. **Without it, Rustfort brews nothing**
  from r4: the export has 14 active orders (furniture, crafts, ammo, barrels...) and 20 queued
  jobs (11 meals, 2 brews), but no repeating brew or cook order, so after 2.18 years the fort has
  **0 drinks and 20 meals left** (DF real: 1 600 drinks, 645 meals) and lives from the wells.
- military stand-down like `rustfort bench --standdown` (squads Ready while a reachable hostile
  is near, then off duty after a day of calm). For r4 -> r5 `--restore-schedules` returns the
  squads to the routine of the start export instead (3 squads on constant training at r4),
  which is what Gordon's fort did. For r3 -> r4 the export was taken mid-siege (all 9 squads
  Ready with Station orders), so restoring that "routine" keeps 70 soldiers stationed forever;
  in 5 of 6 seeds they starved (88 starved in one seed). r3 -> r4 is therefore run with the
  plain stand-down.
- invasions on; 6 seeds (1-6), 3 in parallel.
- r3 has no `settings.json`; the importer's defaults (no population cap) flooded the fort with
  69 migrants, so the tool copies the caps of the later export (90/120/120) when the start
  export has none.

Run it:

```
cargo run -p rf-sim --release --example validate -- --from export/tileshtulon-r4 --to export/tileshtulon \
    --seeds 1,2,3,4,5,6 --standing-orders --restore-schedules \
    --gamelog "E:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress\gamelog.txt" \
    --gl-from 11816811 --gl-to 12027766 [--md table.md]
```

About 30-80 s for 6 seeds (each sim runs at 20-55k ticks/s depending on machine load).

### Finding the snapshots in `gamelog.txt`

The gamelog (1 GB, 12.2 M lines, last "Loading Fortress" at line 10 999 799, so no reload in
the windows) has no dates; the only calendar markers are the season lines. Counting them back
from the live date (year 280, Hematite, read with `dfhack-run`) gives spring 277 at line
11 816 811, spring 279 at 12 019 138, summer 279 at 12 027 767. Anchors:

- r3: the export holds the 35 goblins of the siege announced at 11 637 327 and both Axe Lords
  found dead at 11 639 760 / 11 640 471, so it lies between them (window from 11 637 500).
- r4: 22 % into spring 277; the next siege arrives at 11 828 198 and r4 has no goblins of it.
- r5: 93 % into spring 279, between the last counted event (12 020 281) and summer.
- r6: the end of the log at the time of the export (12 166 369).

The window's real deaths match the exports exactly: the 14 citizens of r4 missing in r5 are the
14 "has been found dead" lines (plus one migrant who arrived and died of thirst).

## DF's real speed on this fort

| measurement | calendar ticks | wall time | ticks/s |
|---|---|---|---|
| r1 -> r2 (export end to export end) | 251 960 | 2 099 s | **120** |
| r2 -> r3 | 509 553 | 3 130 s | **163** |
| r3 -> r4 | 534 141 | 4 007 s | **133** |
| r4 -> r5 | 878 311 | 11 039 s | **80** |
| r1 -> r5 overall | 2 173 965 | 20 275 s | **107** |
| during the exports (exporter running) | 399-2 054 | 14-34 s | 12-106 |
| live, 10.10. ~06:10, year 280, 393 active units (72 s, 7 reads) | 7 701 | 72 s | **107** |

These are lower bounds (pauses count as wall time). The live read also shows
`world.frame_counter` advancing only **2 077 frames in 72 s (29 frames/s)** while the calendar
advanced 7 701 ticks, about **3.7 calendar ticks per simulated frame**, and DF's own
`calculated_fps` read 4-60 (cap 100). So on this fort DF simulates about **30-60 frames/s**,
and a time-stretching tool (consistent with DFHack's `timestream`) makes the calendar run at
roughly **100-160 ticks/s**. A later read while the game was paused moved 6 ticks/s with frames
equal to ticks.

Rustfort on the same fort: about 54-56k ticks/s per sim with 3 sims running side by side on a
quiet machine (first r4 -> r5 run, r3 -> r4 run), about 61k for one sim in round 7
(PROGRESS.md). That is **about 500x DF's calendar speed and about 2 000x DF's real frame
rate**.

## Results r4 -> r5 (2.18 years, 6 seeds, standing orders, restored squad routines)

| metric | DF r4 | DF r5 / change | sim min | sim median | sim max | verdict |
|---|---|---|---|---|---|---|
| citizens at the end | 88 | 87 | 89 | 95 | 96 | close (1.09) |
| citizens lost | | **14** (+1 migrant died) | 1 | 2 | 4 | **LOW (0.14)** |
| deaths in gamelog: sieges / thirst | | 14 / 1 | | | | |
| migrants arrived (alive) / waves | | 11 / 5 | 1 / 1 | 3 / 2 | 4 / 2 | LOW (pop stays at the cap of 90) |
| births (alive at the end) | | 2 | 2 | 6 | 7 | in range, median 3x |
| children at the end | 10 | 8 | 8 | 12 | 13 | in range |
| sieges / ambush groups | | 2 / 6 | 0 / 0 | 3 / 0 | 4 / 0 | in range / LOW |
| invaders spawned | | (r6 siege: 137 goblins) | 0 | 30 | 40 | |
| hostiles on the map at the end | 6 | 16 | 11 | 23 | 25 | in range |
| caravans | | 4 | 4 | 4 | 4 | **in range** |
| strange moods / artifacts | | 5 / 5 | 5 / 2 | 5 / 4 | 6 / 5 | **in range** |
| forgotten beasts | | **0** | 1 | 3 | 5 | **HIGH** |
| tantrums | | 1 | 0 | 2 | 3 | in range |
| stressed or worse at the end | 6 | **17** | 0 | 0 | 1 | **LOW** |
| mean stress | -42 328 | -25 720 | -43 065 | -38 342 | -37 676 | LOW (too happy) |
| work XP per dwarf-year | | 1 892 | 494 | 916 | 1 041 | **LOW (0.48)** |
| military XP per dwarf-year | | 14 545 | 6 757 | 6 924 | 7 186 | LOW (0.48); 383 with plain stand-down |
| social XP per dwarf-year | | 1 977 | 0 | 0 | 0 | **LOW (0)** |
| new drinks (units) | | 1 096 | 805 | 846 | 952 | close (0.77); **0 without standing orders** |
| new meals (units) | | 263 | 42 | 223 | 244 | close |
| new crafts | | 290 (and 161 older crafts left the fort, likely traded) | 345 | 370 | 373 | HIGH (1.28) |
| craft quality: mean / masterworks | | 2.6 / 25 | 0 / 0 | 0.0 / 0 | 0.0 / 0 | **LOW (all ordinary)** |
| new furniture | | 119 (60 beds, 25 coffins) | 66 | 67 | 68 | LOW (0.56) |
| new containers | | 119 (105 barrels) | 79 | 79 | 79 | LOW (75 of them leather bags) |
| new clothing / cloth+thread | | 1 543 / 506 | 0 / 2 | 0 / 2 | 1 / 2 | LOW (no orders in r4) |
| new metal armor / weapons / ammo | | 318 / 235 / 907 | 0 / 2 / 217 | 34 / 26 / 625 | 52 / 42 / 828 | LOW / LOW / LOW (0.69) |
| new plants / seeds / flour | | 584 / 61 / 3 | 1 054 / 80 / 91 | 1 225 / 163 / 91 | 1 322 / 185 / 92 | HIGH (2.1 / 2.7 / 30) |
| new refuse (corpses, parts) | | 545 | 89 | 132 | 158 | LOW (fewer deaths) |
| drinks / meals in stock | 1 286 / 774 | 1 600 / 645 | 892 / 201 | 915 / 353 | 952 / 365 | LOW (0.57 / 0.55) |
| plants / boulders / logs in stock | 1 052 / 882 / 155 | 1 312 / 878 / 77 | | 1 926 / 447 / 150 | | HIGH / LOW / HIGH |
| items total | 16 218 | 17 390 | 16 603 | 16 863 | 16 952 | close |
| item wealth | 3 028k | 3 215k | 3 281k | 3 334k | 3 474k | close (1.04) |
| new items without a stored value | | 545 | 1 369 | 1 551 | 1 630 | HIGH (count 0 in `fort_wealth`) |
| buildings (total / furniture / zones) | 1 669 / 1 237 / 213 | 1 822 / 1 350 / 248 | | 1 669 / 1 237 / 213 | | player-driven |
| tiles dug / smoothed / deconstructed | | 782 / 896 / 23 | | 0 / 0 / 0 | | player-driven |
| trees | | **+65** | | 0 | | no tree growth |
| stockpile fill | 39.2 % | 37.4 % | 24.2 % | 24.6 % | 25.4 % | LOW (0.66) |

Selected skills (XP per persisting dwarf and year, DF / sim median): mining 50 / 21,
masonry 391 / 117, stonecraft 186 / 57, growing 644 / 324, brewing 92 / 46, cooking 93 / 15,
carpentry 69 / 1; axe 1 816 / 2 992, dodging 336 / 1 021, armor 399 / 1 019, shield 190 / 730,
melee combat 2 648 / 1 031, wrestling 606 / 64, **situational awareness 6 116 / 0,
discipline 1 711 / 0**, concentration 417 / 0, knowledge acquisition 417 / 0, teaching 366 / 0,
organization 306 / 0.

Cross-check of the XP model with the wiki: w:Experience gives 10 XP per mined tile. DF dug 782
wall tiles in the window; 782 x 10 / (74 dwarves x 2.18 years) = 48 XP per dwarf-year, and the
measured real mining gain is 50. Rustfort gives 30 per tile.

## Results r3 -> r4 (1.32 years, plain stand-down)

| metric | DF r3 | DF r4 / change | sim min | sim median | sim max | verdict |
|---|---|---|---|---|---|---|
| citizens | 90 | 88 | 89 | 92 | 94 | close |
| citizens lost | | 2 (the running siege) | 1 | 6 | 7 | in range (3x) |
| new sieges | | 0 | 0 | 3 | 4 | in range |
| migrants / births | | 0 / 0 | 1 / 1 | 4 / 4 | 6 / 4 | HIGH / HIGH |
| caravans | | 2 | 1 | 1 | 1 | close |
| moods / artifacts | | 5 / 5 | 3 / 3 | 4 / 4 | 4 / 4 | close |
| forgotten beasts | | 0 | 1 | 2 | 2 | HIGH |
| tantrums | | 4 | 1 | 1 | 3 | LOW |
| stressed or worse / mean stress | 2 / -42 628 | 6 / -42 328 | 0 / -38 171 | 4 / -35 353 | 4 / -31 450 | close |
| work / military / social XP per dwarf-year | | 2 183 / 27 097 / 1 811 | | 1 024 / 836 / 0 | | LOW (0.47 / 0.03 / 0) |
| new drinks / meals | | 709 / 440 | 379 / 0 | 432 / 0 | 475 / 0 | LOW (0.61 / 0) |
| new crafts / craft quality | | 346 / 2.9 | 234 / 0.0 | 239 / 0.0 | 245 / 0.1 | LOW (0.69) / LOW |
| new furniture | | 326 | 65 | 67 | 68 | LOW (0.21) |
| new bars | | 51 | 7 | 8 | 8 | LOW (0.16) |
| meals in stock | 630 | 774 | 51 | 88 | 112 | LOW (0.11) |
| tiles dug / smoothed | | 1 333 / 1 245 | | 0 / 0 | | player-driven |
| item wealth | 2 957k | 3 028k | 3 132k | 3 318k | 3 490k | close (1.10) |
| stockpile fill | 43.6 % | 39.2 % | 33.5 % | 33.6 % | 33.8 % | close |

## Results r5 -> r6 (1.14 years): the fall of the fort

In real DF a third siege arrived in summer 280 ("elite mounted goblin pikemen"). The r6 export
holds **137 goblins** (40 swordsmen, 21 lashers, 19 hammermen, 32 master or elite troops,
6 crossbowmen) plus 7 voracious cave crawlers, and the gamelog lists **90 dwarves found dead**
within a few thousand lines, six mayors elected one after another while they died, and 17 of 31
workshops gone (likely wrecked by building destroyers). 15 citizens were left.

| metric | DF r5 | DF r6 / change | sim min | sim median | sim max | verdict |
|---|---|---|---|---|---|---|
| citizens | 87 | **15** | 90 | 91 | 93 | **HIGH (6x)** |
| citizens lost | | 73 | 0 | 3 | 3 | LOW (0.04) |
| sieges | | 1 (+4 ambush groups) | 1 | 2 | 3 | in range |
| invaders | | >= 137 in one siege | 10 | 20 | 30 (all sieges) | **LOW** |
| hostiles on the map | 16 | 147 | 15 | 24 | 35 | LOW |
| workshops | 31 | 14 | 31 | 31 | 31 | HIGH (no building destroyers wrecking them) |
| caravans / moods / artifacts | | 2 / 3 / 3 | 3 / 1 / 1 | 3 / 3 / 3 | 3 / 4 / 3 | close / in range / in range |

Rustfort's siege rule (`invasion.rs`, r7) needs the population **and** one of created or
exported wealth for a tier. At 87 citizens that is tier 3 ("10 regulars"), whatever the wealth.
DF sent at least 137 goblins to the same fort with the same population and 3.2 M item wealth,
which fits tier 5 (Metropolis, 120 regulars + 40 monsters on the wiki) reached by wealth alone.

**DF's own numbers (exporter round 9, `export/tileshtulon-r7`, live game after the fall):** `plotinfo.progress_population` = 3, `progress_trade` = 5, `progress_production` = 5 (tier indices into `ENEMY_POP/TRADE/PROD_TRIGGER`), created wealth (`tasks.wealth.total`) 1 576 145, exported 216 264, imported 253 175. DF's invasion list holds 5 goblin sieges: 34 (year 274), 58 (275), 143 (277), 101 (278) and 148 (280) units, so DF chose the size by the trade/production tier 5 while the population tier was 3. These values are in `settings.other` (`WEALTH_CREATED=`, `PROGRESS_TRADE=`, `INVASION_HISTORY=`, ...; `rf_import::wealth`, `invasions.json`). The r5 export has none of them; DF keeps only current values, so r5's cannot be recovered. Imported and exported wealth only grow, so r7's are upper bounds for r5.

## What matches DF

- **Caravans** (4 in 2.18 years, 2 in 1.32 years), **strange moods and artifacts**, tantrums.
- **Siege count** (2-3 per two years) and the population staying near the cap of 90.
- **Total item count and item wealth** within 5-10 %, ammo stock, bars, blocks, gems, furniture
  stock, stockpile space.
- **Drink and meal production** when the fallback standing orders stand in for Gordon's food
  watch (0.77 and 0.85 of DF on r4 -> r5).
- **Overall item quality** (mean 1.3-2.2 on both sides), though for different items (below).

## What diverges, and why

| # | divergence (DF vs Rustfort) | likely cause | module / FIDELITY row |
|---|---|---|---|
| 1 | Sieges kill 14 of 88 (r4 -> r5) and 73 of 87 (r5 -> r6) in DF, 0-4 in Rustfort | siege size: 10 regulars per siege at 87 citizens vs >= 137 goblins in DF; no ambush groups, no mounts (beak dogs) or monsters, invader skills | `invasion.rs` siege tier rule (WRONG by observation), "Sieges vs ambushes" MISSING, "Monsters and mounts" MISSING |
| 2 | Forgotten beasts: 0 in 3.5 years of DF history, 1-5 per 2 years in Rustfort | arrival 1/4 per season once a cavern is known | `caverns.rs`, "Forgotten beast arrival rate" (now confirmed too high) |
| 3 | Social skills gain about 2 000 XP per dwarf-year in DF, 0 in Rustfort | conversations, teaching/learning at demonstrations, organization/record keeping give no XP | `social.rs`, `military.rs` (MISSING) |
| 4 | Military XP: DF 14.5k per dwarf-year, mostly situational awareness (6.1k), melee combat (2.6k), discipline (1.7k), axe (1.8k); Rustfort trains weapon, dodging, armor and shield (2.6-3.8x DF) and never awareness or discipline | flat 3/1/1/1 XP every 100 ticks of training; no sparring, demonstrations or individual drills | `military.rs` training (ESTIMATE) |
| 5 | Work XP about half of DF; per job Rustfort gives 30 for everything | wiki: 30 per use plus 30 for an item with quality (crafts, furniture: 60), mining, smoothing, engraving and stonecutting only 10 | `lib.rs` `add_xp` callers (WRONG) |
| 6 | Skill levels above 20: DF has 96-137 skills over 20 (up to 29); Rustfort stops levelling at 20 | `add_xp` caps the rating at 20; wiki: no upper limit | `lib.rs` `add_xp` (WRONG) |
| 7 | Crafts are always "ordinary" in Rustfort; DF crafts average 2.4-2.9 (25-46 masterworks) | crafts take no quality from the crafter's skill | `production.rs` / `industry.rs` (MISSING, like meal quality) |
| 8 | Stress: DF's fort gets more stressed over the window (6 -> 17 stressed); Rustfort's gets happier | fewer deaths (no grief), thought strengths estimated | `social.rs` thought strengths (ESTIMATE) |
| 9 | Births: 6 alive per 2.18 years in Rustfort, 2 in DF (0 in 1.32 years) | pregnancy rates | `population.rs` "Pregnancy" (ESTIMATE, too high) |
| 10 | Rustfort makes 30x the flour and 15x the bags, twice the plants and seeds of DF, but no barrels and few meals without help | the r4 orders MillPlants / ConstructBag run every day with plenty of input; farms keep harvesting; raw-plant eating makes seeds | `production.rs` (order conditions OK; consumption balance ESTIMATE) |
| 11 | Items Rustfort makes have no stored value (1 551 new items), so `fort_wealth()` (created wealth for siege tiers) ignores all new production | `push_item` leaves `value` 0 | `population.rs` `fort_wealth` / `invasion.rs` (WRONG) |
| 12 | Trees grow in DF (+65 in 2.18 years), never in Rustfort | no sapling growth | MISSING |
| 13 | Building destroyers wrecked 17 of 31 workshops in DF's last siege | invaders do not attack workshops | `combat.rs` / `defense.rs` (MISSING) |
| 14 | Stockpiles hold items on 37-39 % of their tiles in DF, 25 % in Rustfort (r4 -> r5) | containers are filled more densely (bin volume ESTIMATE), fewer new items to store | `storage.rs` "Bin volume" (ESTIMATE) |
| 15 | Clothing, cloth, beds, barrels, coffins, digging, smoothing, new zones | Gordon's orders and designations after the start export | player-driven, not a sim error |

## Prioritized calibration fixes for round 9

1. **Siege size and lethality** (`invasion.rs`): pick the tier by population **or** created
   wealth **or** exported wealth (the observed 137-goblin siege at 87 citizens), count the value
   of items the sim makes in `fort_wealth` (give made items a value at creation), add ambush
   groups that arrive with sieges, beak-dog mounts and siege monsters, and building destroyers
   that attack workshops. Re-run `validate` r5 -> r6: the fort should be in real danger.
2. **Experience per job** (`lib.rs` and the `add_xp` callers): 10 XP for mining, smoothing,
   engraving, carving and stonecutting; 30 per use plus 30 when the product has a quality
   level (furniture, crafts, weapons, armor). Remove the level cap of 20.
3. **Military training** (`military.rs`): sparring and demonstrations that train situational
   awareness, discipline, melee combat and the weapon, with teacher/student XP; less flat
   dodge/armor/shield XP. Target: 14-27k military XP per dwarf-year with Gordon's routine.
4. **Social and mental skills** (`social.rs`): XP for conversation, comedy, flattery,
   consoling, pacifying and so on from the social interactions the sim already counts, plus
   concentration, knowledge acquisition and teaching from demonstrations and reading, and
   organization/record keeping for the manager and bookkeeper. Target: about 2 000 XP per
   dwarf-year.
5. **Item quality from skill** for crafts (and meals, already a known gap): DF's crafts
   average 2.4-2.9.
6. **Forgotten beast rate** (`caverns.rs`): far lower; 0 in 3.5 real years with three caverns
   open.
7. **Pregnancy rate** (`population.rs`): about a third of today's (2 births in 2.18 years).
8. **Stress after deaths and sieges**: grief and fear thoughts strong enough that a fort
   losing 14 dwarves ends with about 17 stressed citizens, as DF did.
9. **Tree growth** (saplings to trees) and a milling/bag balance check.
10. For the tool: a player emulation for Gordon's routine digging and building is out of
    scope; compare those only as "player-driven".

## UI behavior checked against Gordon's DF setup (round 9, rf-app)

The validation runs are headless, so the UI does not change any number above. What the game window now takes from Gordon's real DF setup (read-only):

| What | Gordon's DF | Rustfort round 9 |
|---|---|---|
| Boxes that pause (`prefs/announcements.txt`) | `BOX` on citizen deaths, migrants, thieves, snatchers, caravans, tantrums, berserkers, mood workshop claims (also `R`), artifacts begun | the same types open a box and pause; enemy deaths do not (DF has no box for them) |
| Petitions | answered in DF's petitions window | petitions screen + box with Accept / Deny; unanswered ones are denied after 28 days (sim ESTIMATE) |
| Squad uniforms (`squads.json`, `uniform_by_position`) | Wache: armor, helm, greaves, gloves, shoes, shield, battle axe | shown per position with the missing pieces (on r5: shields missing for most soldiers); editable per position |

During the r5 -> r6 siege (73 deaths in DF), a box per citizen death would be many boxes: the queue folds the fifth and later boxes of one type, and *Close all* clears them.

## Limitations

- One real history and a few seeds. Sieges dominate the deaths, so seeds vary a lot.
- Gordon's scripts changed orders between the exports; production of goods without a standing
  equivalent (clothing, beds, barrels, armor) cannot be compared.
- "New items" also counts stacks split off older stacks (food, drink, ammo) and misses goods
  that were made and then eaten, traded or destroyed; compare durable goods only.
- The r3 export lacks settings, squads' routines and engravings; r4 lacks engravings.
- XP is compared for dwarves present at both ends; DF's dead or arriving dwarves are not in it.
- `pop.animals` counts the animals of departed caravans that DF keeps off the map (r4: 60, r5: 80, r6: 82; DF does not simulate them). Since exporter round 9 they import as `flags.inactive` (older exports are repaired by the importer); on the map r4 had about 117 and r5 about 109 animals. `validate` should filter `!flags.inactive` for this row.

## Appendix: full table r4 -> r5 (as printed by the tool)

Run: `--seeds 1,2,3,4,5,6 --standing-orders --restore-schedules`, gamelog lines 11 816 811 to
12 027 766. `start.*` rows describe the start export; `ev.*` real values come from the gamelog;
`simjob.*` / `simmade.*` are the sim's own job and product counters (no DF equivalent);
`xpskill.*` is XP per persisting dwarf and year; `newkind.*` counts new items by DF item kind.

| metric | DF start | DF end / change | sim min | sim median | sim max | verdict | sim/DF |
|---|---|---|---|---|---|---|---|
| start.active_work_orders | 14 | 14 | - | - | - | - | - |
| start.dig_designations | 24 | 24 | - | - | - | - | - |
| start.queued_workshop_jobs | 20 | 20 | - | - | - | - | - |
| pop.animals | 177 | 189 | 196 | 205 | 211 | close | 1.08 |
| pop.births_alive | 0 | 2 | 2 | 6 | 7 | in range | 3.00 |
| pop.children | 10 | 8 | 8 | 12 | 13 | in range | 1.50 |
| pop.citizens | 88 | 87 | 89 | 95 | 96 | close | 1.09 |
| pop.citizens_lost | 0 | 14 | 1 | 2 | 4 | LOW | 0.14 |
| pop.hostiles_on_map | 6 | 16 | 11 | 23 | 25 | in range | 1.44 |
| pop.migrants_arrived | 0 | 11 | 1 | 3 | 4 | LOW | 0.27 |
| ev.ambushes | - | 6 | 0 | 0 | 0 | LOW | 0.00 |
| ev.animal_births | - | 12 | 0 | 0 | 0 | LOW | 0.00 |
| ev.artifacts | - | 5 | 2 | 4 | 5 | in range | 0.80 |
| ev.beasts | - | 0 | 1 | 3 | 5 | HIGH | - |
| ev.caravans | - | 4 | 4 | 4 | 4 | in range | 1.00 |
| ev.citizen_deaths | - | 15 | 1 | 3 | 4 | LOW | 0.20 |
| ev.death_announcements | - | - | 4 | 19 | 31 | - | - |
| ev.depressions | - | - | 3 | 4 | 6 | - | - |
| ev.dwarf_births | - | 2 | 2 | 6 | 7 | in range | 3.00 |
| ev.insane | - | 0 | 0 | 0 | 0 | in range | - |
| ev.invaders | - | - | 0 | 30 | 40 | - | - |
| ev.masterpieces | - | 85 | 0 | 0 | 0 | LOW | 0.00 |
| ev.migrant_waves | - | 5 | 1 | 2 | 2 | LOW | 0.40 |
| ev.migrants | - | - | 2 | 3 | 4 | - | - |
| ev.moods | - | 5 | 5 | 5 | 6 | in range | 1.00 |
| ev.sieges | - | 2 | 0 | 3 | 4 | in range | 1.50 |
| ev.tantrums | - | 1 | 0 | 2 | 3 | in range | 2.00 |
| ev.trades | - | - | 0 | 0 | 0 | - | - |
| stress.breaking | 2 | 5 | 0 | 0 | 1 | LOW | 0.00 |
| stress.content | 26 | 13 | 31 | 36 | 39 | HIGH | 2.77 |
| stress.ecstatic | 30 | 19 | 25 | 29 | 33 | HIGH | 1.53 |
| stress.fine | 5 | 14 | 0 | 1 | 2 | LOW | 0.07 |
| stress.happy | 21 | 24 | 21 | 30 | 33 | in range | 1.25 |
| stress.mean | -42328 | -25720 | -43065 | -38342 | -37676 | LOW | 1.49 |
| stress.stressed | 3 | 10 | 0 | 0 | 0 | LOW | 0.00 |
| stress.stressed_or_worse | 6 | 17 | 0 | 0 | 1 | LOW | 0.00 |
| stress.very_stressed | 1 | 2 | 0 | 0 | 1 | close | 0.00 |
| xp.military_levels_gained | 0 | 1018 | 557 | 576 | 592 | LOW | 0.57 |
| xp.military_per_dwarf_year | 0 | 14545 | 6757 | 6924 | 7186 | LOW | 0.48 |
| xp.persisting_dwarves | 88 | 74 | 84 | 86 | 87 | close | 1.16 |
| xp.skills_above_20 | 137 | 96 | 129 | 133 | 137 | HIGH | 1.39 |
| xp.social_levels_gained | 0 | 293 | 0 | 0 | 0 | LOW | 0.00 |
| xp.social_per_dwarf_year | 0 | 1977 | 0 | 0 | 0 | LOW | 0.00 |
| xp.work_levels_gained | 0 | 264 | 61 | 102 | 124 | LOW | 0.39 |
| xp.work_per_dwarf_year | 0 | 1892 | 494 | 916 | 1041 | LOW | 0.48 |
| stock.ammo | 1697 | 2360 | 1664 | 1928 | 2026 | close | 0.82 |
| stock.armor_metal | 688 | 1006 | 688 | 722 | 740 | LOW | 0.72 |
| stock.bar | 185 | 176 | 162 | 169 | 171 | close | 0.96 |
| stock.block | 183 | 181 | 182 | 183 | 183 | close | 1.01 |
| stock.boulder | 882 | 878 | 442 | 447 | 468 | LOW | 0.51 |
| stock.cloth_leather | 5364 | 3647 | 5277 | 5285 | 5289 | HIGH | 1.45 |
| stock.clothing | 1310 | 2772 | 1310 | 1310 | 1311 | LOW | 0.47 |
| stock.container | 481 | 600 | 560 | 560 | 560 | close | 0.93 |
| stock.craft | 1089 | 1218 | 1434 | 1459 | 1462 | close | 1.20 |
| stock.drink | 1286 | 1600 | 892 | 915 | 952 | LOW | 0.57 |
| stock.furniture | 1633 | 1749 | 1699 | 1700 | 1701 | close | 0.97 |
| stock.gem | 432 | 441 | 438 | 442 | 443 | in range | 1.00 |
| stock.items_total | 16218 | 17390 | 16603 | 16863 | 16952 | close | 0.97 |
| stock.log | 155 | 77 | 149 | 150 | 151 | HIGH | 1.95 |
| stock.meal | 774 | 645 | 201 | 353 | 365 | LOW | 0.55 |
| stock.mechanism | 306 | 296 | 306 | 306 | 306 | close | 1.03 |
| stock.medical | 22 | 42 | 21 | 22 | 22 | LOW | 0.52 |
| stock.other | 4 | 4 | 4 | 4 | 4 | in range | 1.00 |
| stock.other_food | 1416 | 1562 | 1499 | 1499 | 1500 | close | 0.96 |
| stock.plant | 1052 | 1312 | 1753 | 1926 | 1985 | HIGH | 1.47 |
| stock.refuse | 674 | 933 | 747 | 790 | 816 | close | 0.85 |
| stock.seed | 430 | 397 | 406 | 490 | 511 | close | 1.23 |
| stock.tool | 172 | 190 | 172 | 172 | 172 | close | 0.91 |
| stock.weapon | 451 | 684 | 453 | 477 | 493 | LOW | 0.70 |
| new.ammo | - | 907 | 217 | 625 | 828 | LOW | 0.69 |
| new.armor_metal | - | 318 | 0 | 34 | 52 | LOW | 0.11 |
| new.bar | - | 5 | 0 | 0 | 0 | LOW | 0.00 |
| new.block | - | 16 | 0 | 0 | 0 | LOW | 0.00 |
| new.boulder | - | 92 | 0 | 0 | 0 | LOW | 0.00 |
| new.cloth_leather | - | 506 | 2 | 2 | 2 | LOW | 0.00 |
| new.clothing | - | 1543 | 0 | 0 | 1 | LOW | 0.00 |
| new.container | - | 119 | 79 | 79 | 79 | LOW | 0.66 |
| new.craft | - | 290 | 345 | 370 | 373 | HIGH | 1.28 |
| new.drink | - | 1096 | 805 | 846 | 952 | close | 0.77 |
| new.furniture | - | 119 | 66 | 67 | 68 | LOW | 0.56 |
| new.gem | - | 12 | 11 | 12 | 12 | in range | 1.00 |
| new.log | - | 56 | 0 | 0 | 0 | LOW | 0.00 |
| new.meal | - | 263 | 42 | 223 | 244 | close | 0.85 |
| new.medical | - | 20 | 0 | 0 | 0 | LOW | 0.00 |
| new.other_food | - | 165 | 95 | 95 | 96 | LOW | 0.58 |
| new.plant | - | 584 | 1054 | 1225 | 1322 | HIGH | 2.10 |
| new.refuse | - | 545 | 89 | 132 | 158 | LOW | 0.24 |
| new.seed | - | 61 | 80 | 163 | 185 | HIGH | 2.67 |
| new.tool | - | 18 | 0 | 0 | 0 | LOW | 0.00 |
| new.weapon | - | 235 | 2 | 26 | 42 | LOW | 0.11 |
| quality.craft_new_masterwork | - | 25 | 0 | 0 | 0 | LOW | 0.00 |
| quality.craft_new_mean | - | 2.6 | 0 | 0.0 | 0.0 | LOW | 0.01 |
| quality.new_durable_items | - | 3277 | 739 | 916 | 1113 | LOW | 0.28 |
| quality.new_exceptional_or_better | - | 359 | 200 | 256 | 422 | in range | 0.71 |
| quality.new_masterwork | - | 76 | 74 | 90 | 134 | in range | 1.18 |
| quality.new_mean | - | 1.3 | 1.3 | 1.4 | 1.8 | close | 1.11 |
| wealth.items | 3028k | 3215k | 3281k | 3334k | 3474k | close | 1.04 |
| wealth.new_items | 0 | 238k | 275k | 323k | 475k | HIGH | 1.35 |
| wealth.new_items_without_stored_value | 0 | 545 | 1369 | 1551 | 1630 | HIGH | 2.85 |
| bld.furniture | 1237 | 1350 | 1235 | 1237 | 1237 | close | 0.92 |
| bld.other | 65 | 71 | 65 | 65 | 65 | close | 0.92 |
| bld.stockpiles | 41 | 42 | 41 | 41 | 41 | close | 0.98 |
| bld.total | 1669 | 1822 | 1667 | 1669 | 1669 | close | 0.92 |
| bld.traps | 81 | 80 | 81 | 81 | 81 | close | 1.01 |
| bld.workshops | 32 | 31 | 32 | 32 | 32 | close | 1.03 |
| bld.zones | 213 | 248 | 213 | 213 | 213 | close | 0.86 |
| tile.constructed | 0 | 0 | 0 | 0 | 0 | in range | - |
| tile.deconstructed | 0 | 23 | 0 | 0 | 0 | LOW | 0.00 |
| tile.dug | 0 | 782 | 0 | 0 | 0 | LOW | 0.00 |
| tile.engravings_added | 0 | - | 0 | 0 | 0 | - | - |
| tile.smoothed | 0 | 896 | 0 | 0 | 0 | LOW | 0.00 |
| tile.trees_lost | 0 | -65 | 0 | 0 | 0 | HIGH | -0.00 |
| pile.fill_pct | 39.2 | 37.4 | 24.2 | 24.6 | 25.4 | LOW | 0.66 |
| pile.loose_items_outside | 6673 | 6773 | 6877 | 7033 | 7237 | close | 1.04 |
| pile.tiles | 2993 | 3002 | 2993 | 2993 | 2993 | close | 1.00 |
| newkind.AMMO | - | 907 | 217 | 625 | 828 | LOW | 0.69 |
| newkind.AMULET | - | 24 | 44 | 51 | 62 | HIGH | 2.12 |
| newkind.ARMOR | - | 280 | 0 | 8 | 26 | LOW | 0.03 |
| newkind.BAG | - | 5 | 74 | 75 | 75 | HIGH | 15.00 |
| newkind.BARREL | - | 105 | 4 | 4 | 5 | LOW | 0.04 |
| newkind.BED | - | 60 | 0 | 0 | 0 | LOW | 0.00 |
| newkind.BOULDER | - | 92 | 0 | 0 | 0 | LOW | 0.00 |
| newkind.BRACELET | - | 38 | 36 | 54 | 60 | in range | 1.42 |
| newkind.CABINET | - | 0 | 20 | 20 | 21 | HIGH | - |
| newkind.CHAIR | - | 9 | 23 | 23 | 24 | HIGH | 2.56 |
| newkind.CLOTH | - | 162 | 1 | 1 | 1 | LOW | 0.01 |
| newkind.COFFIN | - | 25 | 0 | 0 | 1 | LOW | 0.00 |
| newkind.CORPSE | - | 171 | 4 | 19 | 31 | LOW | 0.11 |
| newkind.CORPSEPIECE | - | 298 | 85 | 113 | 127 | LOW | 0.38 |
| newkind.CROWN | - | 34 | 45 | 58 | 61 | HIGH | 1.71 |
| newkind.DRINK | - | 1096 | 805 | 846 | 952 | close | 0.77 |
| newkind.EARRING | - | 39 | 40 | 55 | 58 | HIGH | 1.41 |
| newkind.FIGURINE | - | 35 | 46 | 50 | 58 | HIGH | 1.43 |
| newkind.FOOD | - | 263 | 42 | 223 | 244 | close | 0.85 |
| newkind.GLOB | - | 153 | 4 | 4 | 4 | LOW | 0.03 |
| newkind.GLOVES | - | 452 | 0 | 0 | 0 | LOW | 0.00 |
| newkind.GOBLET | - | 45 | 0 | 0 | 0 | LOW | 0.00 |
| newkind.HELM | - | 107 | 0 | 12 | 17 | LOW | 0.11 |
| newkind.PANTS | - | 283 | 0 | 10 | 11 | LOW | 0.04 |
| newkind.PLANT | - | 535 | 1054 | 1225 | 1322 | HIGH | 2.29 |
| newkind.PLANT_GROWTH | - | 49 | 0 | 0 | 0 | LOW | 0.00 |
| newkind.POWDER_MISC | - | 3 | 91 | 91 | 92 | HIGH | 30.33 |
| newkind.QUIVER | - | 31 | 0 | 4 | 7 | LOW | 0.13 |
| newkind.REMAINS | - | 76 | 0 | 0 | 0 | LOW | 0.00 |
| newkind.RING | - | 44 | 42 | 49 | 60 | in range | 1.11 |
| newkind.SCEPTER | - | 29 | 51 | 60 | 66 | HIGH | 2.07 |
| newkind.SEEDS | - | 61 | 80 | 163 | 185 | HIGH | 2.67 |
| newkind.SHIELD | - | 43 | 0 | 0 | 1 | LOW | 0.00 |
| newkind.SHOES | - | 739 | 0 | 0 | 0 | LOW | 0.00 |
| newkind.SKIN_TANNED | - | 20 | 0 | 0 | 0 | LOW | 0.00 |
| newkind.THREAD | - | 324 | 1 | 1 | 1 | LOW | 0.00 |
| newkind.WEAPON | - | 161 | 1 | 22 | 35 | LOW | 0.14 |
| newkind.WOOD | - | 56 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.APPRAISAL | 0 | 33.2 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.ARMOR | 0 | 399 | 999 | 1019 | 1049 | HIGH | 2.55 |
| xpskill.AXE | 0 | 1816 | 2952 | 2992 | 3070 | HIGH | 1.65 |
| xpskill.BITE | 0 | 121 | 1.0 | 1.3 | 3.4 | LOW | 0.01 |
| xpskill.BONECARVE | 0 | 27.0 | 0.2 | 0.2 | 0.2 | LOW | 0.01 |
| xpskill.BREWING | 0 | 91.7 | 39.5 | 45.6 | 48.7 | LOW | 0.50 |
| xpskill.CARPENTRY | 0 | 69.4 | 0.7 | 0.8 | 0.9 | LOW | 0.01 |
| xpskill.COMEDY | 0 | 41.5 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.CONCENTRATION | 0 | 417 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.CONSOLE | 0 | 47.5 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.CONVERSATION | 0 | 27.6 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.COOK | 0 | 93.3 | 9.1 | 14.7 | 15.5 | LOW | 0.16 |
| xpskill.CROSSBOW | 0 | 71.6 | 11.4 | 26.2 | 55.3 | LOW | 0.37 |
| xpskill.CRUTCH_WALK | 0 | 70.5 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.CUT_STONE | 0 | 41.3 | 0 | 0 | 106 | in range | 0.00 |
| xpskill.DISCIPLINE | 0 | 1711 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.DODGING | 0 | 336 | 1000 | 1021 | 1049 | HIGH | 3.04 |
| xpskill.ENGRAVE_STONE | 0 | 16.3 | 0 | 106 | 108 | in range | 6.49 |
| xpskill.FORGE_ARMOR | 0 | 0 | 0 | 0 | 107 | in range | - |
| xpskill.FORGE_FURNITURE | 0 | 0 | 0 | 0 | 106 | in range | - |
| xpskill.FORGE_WEAPON | 0 | 0 | 0 | 106 | 108 | in range | - |
| xpskill.GRASP_STRIKE | 0 | 331 | 6.3 | 12.1 | 27.3 | LOW | 0.04 |
| xpskill.HERBALISM | 0 | 46.1 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.INTIMIDATION | 0 | 33.6 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.JUDGING_INTENT | 0 | 37.2 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.KNOWLEDGE_ACQUISITION | 0 | 417 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.MASONRY | 0 | 391 | 10.0 | 117 | 224 | LOW | 0.30 |
| xpskill.MELEE_COMBAT | 0 | 2648 | 1007 | 1031 | 1073 | LOW | 0.39 |
| xpskill.METALCRAFT | 0 | 0 | 0 | 0.2 | 106 | in range | - |
| xpskill.MINING | 0 | 50.1 | 13.0 | 21.1 | 141 | in range | 0.42 |
| xpskill.NEGOTIATION | 0 | 43.3 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.ORGANIZATION | 0 | 306 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.PACIFY | 0 | 50.4 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.PERSUASION | 0 | 76.0 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.PLANT | 0 | 644 | 299 | 324 | 349 | LOW | 0.50 |
| xpskill.RANGED_COMBAT | 0 | 65.3 | 5.4 | 12.8 | 27.2 | LOW | 0.20 |
| xpskill.SHIELD | 0 | 190 | 721 | 730 | 747 | HIGH | 3.84 |
| xpskill.SITUATIONAL_AWARENESS | 0 | 6116 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.SPEAKING | 0 | 69.3 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.STANCE_STRIKE | 0 | 135 | 4.4 | 6.3 | 13.7 | LOW | 0.05 |
| xpskill.STONECRAFT | 0 | 186 | 50.7 | 56.5 | 163 | LOW | 0.30 |
| xpskill.TEACHING | 0 | 366 | 0 | 0 | 0 | LOW | 0.00 |
| xpskill.WEAVING | 0 | 78.6 | 0.3 | 0.3 | 0.3 | LOW | 0.00 |
| xpskill.WRESTLING | 0 | 606 | 45.3 | 64.2 | 85.3 | LOW | 0.11 |
| simjob.Brew drink from plant | - | - | 271 | 306 | 318 | - | - |
| simjob.Construct rock cabinet | - | - | 20 | 20 | 20 | - | - |
| simjob.Construct rock throne | - | - | 22 | 23 | 23 | - | - |
| simjob.Harvest plants | - | - | 1469 | 1526 | 1626 | - | - |
| simjob.Make leather bag | - | - | 74 | 75 | 75 | - | - |
| simjob.Make rock crafts | - | - | 342 | 367 | 369 | - | - |
| simjob.Mill plants | - | - | 91 | 91 | 92 | - | - |
| simjob.Plant seeds | - | - | 1481 | 1541 | 1656 | - | - |
| simjob.Prepare easy meal | - | - | 61 | 96 | 99 | - | - |
| simmade.AMULET | - | - | 44 | 50 | 62 | - | - |
| simmade.BAG | - | - | 74 | 75 | 75 | - | - |
| simmade.BRACELET | - | - | 35 | 54 | 60 | - | - |
| simmade.CORPSEPIECE | - | - | 85 | 85 | 85 | - | - |
| simmade.CROWN | - | - | 45 | 58 | 61 | - | - |
| simmade.EARRING | - | - | 40 | 55 | 57 | - | - |
| simmade.FIGURINE | - | - | 46 | 50 | 58 | - | - |
| simmade.POWDER_MISC | - | - | 91 | 91 | 92 | - | - |
| simmade.RING | - | - | 42 | 49 | 60 | - | - |
| simmade.SCEPTER | - | - | 51 | 60 | 66 | - | - |
| perf.ticks_per_s | - | - | 20867 | 27000 | 28075 | - | - |
