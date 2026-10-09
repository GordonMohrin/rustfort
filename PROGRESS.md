# Rustfort: progress

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
