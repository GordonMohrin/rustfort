# Rustfort: progress

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
