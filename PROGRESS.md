# Rustfort: progress

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
