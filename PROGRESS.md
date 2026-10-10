# Rustfort: progress

## v0.6.0 (2026-10-10): fidelity audit, title screen with live DF import, visual polish

**New since 0.5:**
- **Fidelity audit:** `docs/FIDELITY.md` checks every sim module against the DF v50 wiki (OK / WRONG / MISSING / ESTIMATE / FIXED). Fixed this round:
  - Brewing uses the whole plant stack, as in DF.
  - Room value tiers were one tier too high; room value now counts floor, walls and engravings.
  - Melt jobs at the smelter return bars per the wiki table.
  - Carve designations survive save/load; constructions can be engraved.
  - Caravans stay 30-40k ticks.
  - Soldiers notice lurking hostiles; wounded soldiers rest and get care.
  - Weapon traps jam (a mechanic cleans them); forgotten beasts get venom, breath and webs.
- **Title screen** with DF's own art:
  - Continue, Load game (saves and DF exports), **Import from Dwarf Fortress**, Settings, Quit.
  - Import runs a read-only probe plus the exporter in your running DF, without pausing it: about 1 min for your fort, then loading takes about 2 s.
  - Autosave each season (keeps the last 3), help overlay (F1 / ?) built from your DF keys, first-time hints, `rustfort.cmd` launcher, and a German "how to play" section in README.md.
- **Graphics:**
  - engravings, item markers (melt etc.)
  - seasonal trees (autumn colors, bare winter branches, blossoms, fruit)
  - per-plant grass (teal cave moss), webs, caste sprites
  - procedural beasts and demons with DF's BEAST_* sprites; unbuilt furniture faded
  - **0 missing sprites on all 150 levels** of your fort
- **Result:**
  - 2 years headless with `--standdown`: 88 -> 96 citizens, about 63k ticks/s.
  - 488 citizens: about 28-32k ticks/s (machine busy).

**Known gaps (see docs/FIDELITY.md for the full lists):**
- DF's existing engravings (16,233 in your fort) are not exported yet, so room value relies on DF's reported level.
- Civilian alert blocks all work, but DF allows work inside the burrow.
- No cave-ins, no recover-wounded carrying.
- Siege size does not yet depend on wealth.
- No meal quality from cook skill, no miasma, no temperature, pumps or minecarts, and no rivers or brooks (your map has none).

## v0.5.0 (2026-10-10): fast at high population, constructions, moods, health, caverns

**New since 0.4:**
- **Performance at high population** on your real map, with N cloned citizens (`examples/scale.rs`), behavior bit-identical (same state hashes):

  | citizens | before | after |
  |---|---|---|
  | 88 | 48k | 82k ticks/s |
  | 188 | 32k | 57k ticks/s |
  | 488 | 14k | 32-35k ticks/s |

  The gains come from a single-pass haul search, a dense item id table, cached walk components, a bucket-queue A* with packed nodes, and exact magic division.

  Real DF manages about 150-220 ticks/s on this fort. **Rustfort at 488 dwarves is still about 150x faster.**
- **Constructions:**
  - wall, floor, ramp, stairs, fortification and track from blocks, boulders or logs, with DF placement and support rules; deconstruction
  - smoothing, engraving (quality by skill), carving fortifications and tracks
  - traffic designations feeding A*; item forbid, claim and dump with garbage dump zones
- **Strange moods:** wiki trigger and lottery, fey/secretive/possessed/fell/macabre, workshop claim, material demands, artifacts (named, valuable, skill to Legendary), insanity on failure.
- **Health care:** diagnosis, suturing, bone setting, surgery and dressing by doctors with supplies; infections.
- **Caverns** are detected (3 layers plus the magma sea on your map), with seasonal cave wildlife and **forgotten beasts** arriving from breached caverns.
- **UI:**
  - Build > Constructions with blueprint preview
  - Smooth, Traffic and Items tools with DF overlays and markers; per-item forbid/dump
  - Mood tab and treatment status
  - raised bridges drawn raised; caged creatures drawn inside their cages
  - a playtest pass with about 10 visual fixes
- **Fix:** soldiers leave a fight when thirst or hunger reach the cancel threshold.
- **Result:** 2 years on the r4 fort with `--standdown`: 88 -> 90 citizens, 1 siege and 5 forgotten beasts beaten, about 70k ticks/s.

**Known gaps (next):**
- No melt jobs; carve-fortification/track designations are not saved; engravings are not drawn and not counted in room value yet.
- No cave-ins; no syndromes or breath attacks on beasts.
- No recover-wounded carrying, hospital stocking, or casts/traction.
- A* still dominates at low population (HPA* would change paths, so the state hash would change).
- Production census still does full scans every 100 ticks.

## v0.4.0 (2026-10-10): defenses, ranged combat, society, fluids

**New since 0.3:**
- **Round-4 export (year 277).** It adds:
  - building states: doors locked/open, bridges raised/lowered with direction, levers pulled, traps loaded with their contents
  - mechanism links in both directions (e.g. the Hoftor bridge to its lever)
  - 58 entities with war/peace/trade relations, nobles and mandates
  - DF settings (pop cap 90/120/120, temperature off)
  - 163 historical-figure names; caravans and diplomats
- **Defenses:**
  - locked doors that invaders path around (building destroyers break them), pet-passable doors
  - bridges that raise, lower and retract (crushing and flinging), levers as a 'Pull lever' job with DF's delay
  - cage, stone-fall and weapon traps (weapon traps use the combat damage model), upright spikes, pressure plates
  - in a natural 21-goblin siege on your real Fallenschnecke, all 21 were hit by traps (18 killed, 3 caged) and 86 of 88 citizens survived
- **Ranged combat:**
  - crossbows and bows with ammo by launcher, LOS through fortifications, projectile damage through the material model, ammo breaking or landing
  - marksdwarves back off and fall back to melee; archery training; bowgoblins in sieges
  - your Schuetzen fired 131 shots in the running siege
- **Society:**
  - migrant waves per DF caps; pregnancies, births and aging
  - burial in coffins and tombs, ghosts when a corpse stays unburied
  - caravans (humans, dwarves) with depot, broker and barter by DF item value
  - nobles, elections and mandates
- **Fluids:**
  - DF flow with active cells only (settled water costs nothing), pressure, evaporation, mud, aquifer leaks, obsidian
  - drowning, swimming, magma burns
  - the real fort stays stable over 2 years (0 flowing cells after a day)
- **UI:**
  - door lock/forbid, lever pull, links shown and drawn, bridge and trap details
  - trade screen (two columns, values, merchant replies), nobles screen, population info, settings (pop cap, invasions)
  - **the simulation runs on its own thread:** max speed about 60k ticks/s in play mode with a 0.7 ms frame CPU
- **Result:** 2 years headless on the r4 fort with `--standdown`: 88 citizens grew to 94 (births and migrants, cap 90/120), one siege beaten, at about 47k ticks/s.

**Known gaps (next):**
- No temperature (your world has it off), pumps, power or minecarts.
- Raised bridges are still drawn lowered; occupied cages are not hauled away.
- Wagons find no 3-wide path to your depot; no diplomats or liaisons.
- Moods and artifacts, engravings, hospitals/doctors, infections, sparring.
- Forgotten beasts as their own event; caverns as a source of creatures.

## v0.3.0 (2026-10-10): psychology, industry, combat

**New since 0.2:**
- **Round-3 export (year 275).** It caught a running goblin siege with 35 invaders.
  - Per unit: needs, emotions, values, relationships, wounds and blood.
  - Expanded creature bodies with tissue layers and attacks (67 creatures), weapon and armor raws, material mechanics.
  - 9 squads with orders and uniforms, burrows with tiles, corpse races.
- **Psychology (DF v50):**
  - About 240 DF thought types, emotions with the wiki's divider table, short- and long-term stress with DF's cutoffs.
  - Needs with focus and its effect on work speed.
  - Dining halls, bedrooms and room value; tantrums, depression and obliviousness, rarely insanity.
  - Over 2 years the real fort stays mostly ecstatic or happy, with one depression.
- **Storage and industry:**
  - Bins, barrels, pots and bags in stockpiles, so many items fit on one tile; rot by DF rules.
  - Wood furnace, smelter (ore table from the raws), forge (weapons, armor, tools, bolts), glass furnace.
  - Butcher, tanner, leatherworks, loom, clothier, quern and millstone, mechanics, and the mason and carpenter extras.
  - Almost all of your work orders run; only ShearCreature does not.
- **Combat:**
  - Wiki material-science formulas (momentum, shear and impact yield) over armor and tissue layers.
  - Wounds per body part, bleeding, pain, unconsciousness, severing; dodge, block and parry; DF-style combat log.
  - Squads: duty from orders and routine (off duty, staggered or constant training, ready), uniforms picked up, training, Kill and Station orders.
  - Civilians flee; civilian alert by burrow; wounded rest in hospital or bed.
  - Invasions by DF triggers (population of 80 or more), with retreat on losses.
- **UI:**
  - Unit sheet tabs Thoughts, Needs, Personality, Health and Relations; Creatures list with stress sorting.
  - Squads screen with routines, orders, assignment and uniforms; burrows tool with civilian alert; Reports screen with the combat log.
  - Hostile markers and a DF-style siege pop-up with auto-pause.
- **Results on the real fort (2 years headless, about 62k ticks/s):**
  - With your 9 squads the running siege is won with 4 citizens lost.
  - With `--standdown` (emulates a player, or your guard script, standing the military down after the fight and back to Ready when a threat shows up), **85 of 90 survive 2 years including 3 sieges**.
  - Without standing down, the 77 soldiers on *Ready* never work and the fort starves. DF behaves the same way: change the routine in the squads screen.

**Known gaps (next):**
- Ranged combat and ammo, traps, and locked or forbidden doors and raised bridges as barriers (your fort relies on them).
- A lone lurking goblin is never engaged. Doctors, infections and suturing are missing.
- Fluids and temperature, caverns and their creatures, migrants, caravans and trade, nobles and mandates, moods and artifacts, engravings.
- Your orders contain no brew order, so the booze runs out after about 5 months and the dwarves drink well water.

## v0.2.0 (2026-10-10): the fort survives and produces

**Works (new since 0.1):**
- **Round-2 export** of the live fort (year 274; read-only, no pause). It adds:
  - 43 work orders with conditions
  - workshop job queues and farm crops
  - 225 plant raws and the active mod list
  - real gait speeds, burrows and squads

  Spot-checked against the live DF.
- **Production simulation:**
  - **Workshops:** still (brew from plant or fruit into barrels or pots, seed returned), kitchen (easy, fine and lavish meals; brewables are never cooked, Gordon's rule), carpenter and mason furniture, barrels, bins, blocks, craftsdwarf pots.
  - **Farming** per the wiki: crops by season, plant growdur, harvest skill rolls, poor underground soil.
  - **Wells** as an emergency water source, as in DF.
  - **Work orders:** checked daily, with conditions, frequencies and workshop limits. Workshop queues with repeat.
  - **Construction:** placed buildings are hauled and built.
- **Result:** with your real orders, crops and stock, the fort survives **2 game years headless with 89/89 citizens alive**, at about 67-71k ticks/s.
  - Your orders contain no brew order (your scripts used to queue brews). After about 7 months the booze runs out and the dwarves drink from the well.
  - Add a *Brew drink* work order in the Work orders screen, or run `--standing-orders` in the sim example.
- **UI:**
  - Build menu with a ghost preview (green/red); zones in the DF style.
  - Building sheet with the workshop queue (add, repeat, suspend, cancel), farm crop picker per season, stockpile categories and remove.
  - Work orders screen (all 43 real orders, new order with amount and frequency); labor grid (89 citizens x labors, with skill levels).
  - Save/Load (bincode+zstd, 1.4 MB, about 0.1 s; F5 quicksave; Esc menu); Drink/Food readout in the top bar.
- **Graphics:**
  - Missing sprites dropped from 17,525 to 4 on the real fort.
  - Trees with trunks, branches, canopies and caps; per-plant shrubs, saplings and crops.
  - Corpses and remains, typed items, 1x1/5x5 workshops, traps, wells, bridges, wall shadows.
  - Exactly the save's active mods are loaded (your save currently uses vanilla only).

**Known gaps (next):**
- **Not simulated yet:**
  - no thoughts, stress or moods
  - no combat, military or sieges
  - no fluids or temperature
  - no bins or wheelbarrows: one item per stockpile tile
  - no butchery, fishing, milling, threshing or trade
  - furnaces and metal industry have no recipes yet
- Caverns are not detected (all underground soil counts as poor), and animals have no needs.
- **UI:** no material choice when building; no zone assignment (bedroom owners).

## v0.1.0 (2026-10-09): first playable prototype

**Works:**
- **Export:** a read-only DFHack exporter (`tools/dfhack/rustfort-export.lua`) dumps the live Trussroads/Tileshtulon fort in about 14 s without pausing DF:
  - 192x192x150 tiles
  - 320 units, 91 of them citizens
  - 14.4k items, 1177 buildings, 29k plants
  - 375 of 375 spot-checked tiles match DF
- **Import:** loads the export in 0.02-0.04 s.
- **Premium tileset** comes straight from the DF install:
  - palette recoloring by material
  - wall autotiling
  - layered dwarves wearing their real equipment
  - workshops, items and toolbar icons
- **Steam-style mouse UI:**
  - your own DF keybindings: wheel = z-level, Ctrl+wheel = zoom
  - Dig/Chop/Gather/Erase/Stockpile tools with drag rectangles across z-levels
  - unit sheet, tile info, citizens list, announcements
  - speeds 1x/2x/5x/max
- **Simulation:**
  - needs follow DF timers: thirst, hunger, sleep
  - exact gait speed
  - A* pathing with traffic costs plus O(1) reachability
  - mining, channels, stairs and ramps; felling and gathering
  - hauling to stockpiles
  - deterministic
- **Speed:** a full game year of the real fort runs headless in about 5 s (about 80,000 ticks/s, roughly 800x DF's 1x).

**Known gaps (next):**
- **No production yet.** There are no workshops, brewing, farming, cooking or wells, so the imported fort dies of dehydration after about 7 months once the drink stock is gone.
- No thoughts or stress, no combat, no military, no fluids or temperature, no saving, no build menu, no zones.
- Some items and buildings still fall back to glyphs (`?`, `&`). Tree canopies are missing.

## How to run

```
cargo build --release
target\release\rustfort.exe play                         # loads export/tileshtulon if present
target\release\rustfort.exe bench --ticks 403200          # one game year headless
```

Re-export your fort while DF is running (read-only, about 14 s; see tools/dfhack/README.md):

```
"E:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress\hack\dfhack-run.exe" lua -f "<repo>\tools\dfhack\rustfort-export.lua" "<repo>\export\tileshtulon"
```
