# rf-app: the `rustfort` game window

A macroquad window that plays like the Steam version of Dwarf Fortress: mouse-driven UI, the DF
font, keybindings read from your own DF `interface.txt`, z-levels with the levels below
visible through open space, and a fast sim loop.

```
rustfort.cmd                                                # repo root: build if needed, then the title screen
cargo run --release -p rf-app                               # the title screen (same as `play` without a world)
cargo run --release -p rf-app -- play --export export/tileshtulon
cargo run --release -p rf-app -- play --synthetic --size 192,192,153
cargo run --release -p rf-app -- bench --ticks 100000       # headless ticks/s + phase times
cargo run --release -p rf-app -- play --load saves/Trussroads-20261009-222118.rfsave
cargo run --release -p rf-app -- savebench                  # save + load time and size
```

`play` without `--export`, `--load`, `--synthetic`/`--size` or `--screenshot` opens the title
screen (round 6, below). With one of them the game starts directly, as before: the world comes
from `rf_import::find_export` (`--export DIR`, env `RUSTFORT_EXPORT`, or `export/tileshtulon`).
If there is none, or the import fails, you get `World::synthetic(96,96,40)` (the reason is shown
as the first announcement). The window title names the loaded world.

The DF install is `rf_gfx::df_dir()` (env `RUSTFORT_DF_DIR`, else the folder from the settings,
else the Steam default). From it the app reads, at runtime only:
- `data/art/title_background.png` and `df_logo.png` for the title screen;
- `data/art/curses_640x300.png`, the 8x12 CP437 font of the Steam UI;
- keybindings, from `%APPDATA%\Bay 12 Games\Dwarf Fortress\prefs\interface.txt`, then
  `<install>\prefs\interface.txt`, then `data\init\interface.txt`;
- Premium sprites and UI icons through rf-gfx. Wherever rf-gfx returns nothing, the app draws
  fallback tiles from material colors and glyphs.

## Title screen (round 6, `menu.rs`)

Like DF's title screen: DF's own background art and logo from the install (a plain gradient
and text logo without them), "RUSTFORT" below, and a column of buttons:

- **Continue** loads the newest save in `saves/` (autosaves count), else the newest complete
  export in `export/`, else the default export. The line under the button says what that is.
- **Load game**: two tabs, *Saves* (fort, game date, when saved, size; autosaves marked
  `[auto]`) and *DF exports* (every `export/*/manifest.json`: fort, year, when exported,
  folder). A click loads it.
- **Import from Dwarf Fortress** (`importer.rs`): see below.
- **New synthetic fort (dev)**: `World::synthetic` with `--size` (default 96x96x40).
- **Settings**: interface size (50-300 %), frame cap (used without vsync), vsync (next start),
  autosave interval (off / week / month / season / year) and how many autosaves to keep,
  population cap and invasions for new imports ("From the save", or a value: applied when a
  fresh export or synthetic fort starts, never to a loaded save), the DF folder (a text field:
  click, type or Ctrl+V, "Default"; it says whether DF and DFHack are there), and "Show them
  again" for the first-time hints. Back (or Esc) writes the file.
- **Quit**.

Loading runs in a thread (world, then the Premium graphics), with a progress bar on the title
screen; a failed load returns to the title with the reason.

The settings live in `saves/settings.json` (next to the saves; `$RUSTFORT_SAVE_DIR` moves both,
`$RUSTFORT_SETTINGS` names another file). A missing or broken file gives the defaults, unknown
fields are ignored. `--ui-scale` and `--gfps` win over the file; `RUSTFORT_VSYNC` wins over the
vsync setting; screenshot runs read the file but never write it.

In the game, Esc > **Save and return to title** saves (in the background, finished before the
switch) and goes back to the title screen.

### Import from Dwarf Fortress

1. **Check** (a thread, about 0.1 s): `hack/dfhack-run.exe lua -f tools/dfhack/rustfort-probe.lua`
   prints one JSON line (map loaded, fortress mode, fort and world names, year, citizens,
   paused). The probe only reads globals; it never pauses DF or writes to it. Only if DFHack does
   not answer, `tasklist` tells "DF is not running" from "DFHack does not answer" (the process
   list can take seconds on a busy machine).
2. The dialog shows what it found ("Found Trussroads (Tileshtulon) in the world Zilirróth. Year
   278, 88 citizens"), that the export only reads, and the target folder
   `export/<fort>-<yyyymmdd-hhmmss>/` (UTC; earlier exports are kept).
3. **Import and play** runs `dfhack-run lua -f tools/dfhack/rustfort-export.lua <dir> --any-fort`
   (the exporter returns at once and works in frame slices inside DF) and polls `progress.json`
   about every frame: a bar from the exporter's phase and `i/n` detail (phases weighted by their
   measured share, the map is about half), the phase name, the seconds so far. "Cancel export"
   writes the exporter's `cancel` file. `manifest.json` without `export.lock` = done,
   `error.json` = failed (shown), 90 s without a new `progress.json` = stalled.
4. The export is loaded like any other, saved once right away as an autosave (so Continue finds
   it), and the game starts.

Errors are spelled out with what to do: dfhack-run not found (check the DF folder in Settings,
install DFHack), the scripts not found (start from the repo with `rustfort.cmd`), DF not running,
DFHack not answering (dfhack-run crashes silently when nothing listens on its port), no fort
loaded. Retry checks again.

Measured on 10.10.2026 against Gordon's running fort (year 279, while other builds kept the
machine busy): check 0.3 s; export 64 s wall, 36 s of Lua inside DF, 997 frames, 68 MB; DF kept
running; Rustfort loaded the result and started at "3rd Granite, 279" with 87 citizens.

Paths: the scripts are found in `$RUSTFORT_TOOLS_DIR`, `tools/dfhack` in the working directory,
next to the exe's repo (`target/release/../../tools/dfhack`) or in the source tree; exports go to
`$RUSTFORT_EXPORT_ROOT` or `export/`.

## Autosave (round 6)

Every N game days (Settings; default 84 = one season, so it happens on the first day of each
season like DF's `AUTOSAVE:SEASONAL`; the in-game Settings page has the same switch and says
how many days are left). The save runs in the background like F5, into
`saves/<fort>-auto-<yyyymmdd-hhmmss>.rfsave`, and afterwards only the newest 3 autosaves of that
fort are kept (`autosave_keep`). Manual saves and other forts' autosaves are never deleted.
Screenshot runs do not autosave unless `--autosave-days N` is given.

## Help (round 6, `ui/help.rs`)

- **F1**, **?** or the `?` button at the right of the top bar opens the controls overview: View,
  Time, Game, Orders, Build and Screens with the keys of the bindings in use (from your DF
  `interface.txt`, so they match your Steam DF; `-` when a binding is not set), plus the mouse
  controls with your wheel bindings. Esc, F1, "Close" or a click outside close it.
- **First-time hints**: five bubbles, one at a time, pointing at what they explain (welcome and
  pause/speed, moving around, looking at things, the toolbar, the help and saving). "Next" /
  "Got it" moves on, "No more hints" stops them; both are remembered in the settings file.

## Controls

The defaults below are Steam DF's. Your own `interface.txt` overrides them. F1 in the game shows
the same overview with your actual keys.

| Action | Mouse | Keys |
|---|---|---|
| Pan | middle-drag | WASD or arrows (Shift = fast) |
| z-level up/down | wheel (direction as in your DF prefs; wheel down = up by default) | `q`/`e`, `<`/`>`, PageUp/PageDown; Shift+wheel = 10 levels |
| Zoom | Ctrl+wheel (anchored at the cursor) | `[` `]` |
| Pause / one tick | pause button | Space / `.` |
| Speed 1x / 2x / 5x / max | top bar buttons | `1` `2` `3` `4` (1x = 100 ticks/s, DF's FPS cap) |
| Select unit / tile | left click (click again to cycle through units on a tile) | |
| Back out | right click closes the topmost window: popup/box, big screen, sheet, drag box, tool, menu (see "Feel round") | Esc, which finally opens the menu |
| Dig menu | toolbar pickaxe | `m`, then `m` mine, `t` up/down stair, `r` ramp, `u` channel, `x` remove |
| Chop / gather / erase | toolbar | `l` / `g` / `x` |
| Smooth / engrave / carve | toolbar (after Gather): Smooth stone, Engrave, Carve track, Carve fortifications, Remove; drag a box (3D like digging) | `v`, then `m` smooth, `g` engrave, `t` track, `f` fortify |
| Traffic | toolbar (after Erase): High, Normal, Low, Restricted; the map shows DF's traffic colors while the menu is open | `Shift+t`, then `h` / `x` / `l` / `r` |
| Item designations | toolbar (after Traffic): Claim, Forbid, Dump, Remove dump, Melt, Remove melt, Hide, Unhide; drag a box over items | `i`, then `Shift+f` claim, `f` forbid, `p` / `Shift+p` dump, `m` / `Shift+m` melt, `h` / `Shift+h` hide |
| Forbid / dump one item | `F` and `D` toggles in the tile info item list and on the unit sheet's Items tab; "Forbid all" / "Claim all" for a tile | |
| Constructions | Build menu, category Constructions: Wall, Floor, Ramp, Up stair, Down stair, Up/down stair, Fortification, Track, Remove construction; drag a rectangle (one level) | `b` |
| Stockpile | toolbar crate, then pick a preset (Stone, Wood, Food, Furniture, All) | `p` |
| Creatures list | toolbar (first button): Citizens (sort by Name / Profession / Stress), Pets/livestock, Others (visitors, wild animals, hostiles) | `u` |
| Unit sheet tabs | Overview, Thoughts, Needs, Personality, Mood (only in a strange mood), Health, Relations, Skills, Labor, Items (wheel scrolls) | |
| Squads | toolbar (right group, red arrow) | `c` |
| Burrows | toolbar (middle group, after Zones) | `Shift+u` |
| Reports | toolbar (right group, `‼`) | `Shift+n` |
| Close a pop-up alert | "OK" / "Center on them" | Enter or Esc |
| Build | toolbar hammer: pick a category, then a building; a green/red ghost follows the mouse, click to place (farm plots: drag) | `b` |
| Zones | toolbar zone button: pick a type and drag, or click a zone to select it | `z` |
| Building sheet | left click a workshop, farm plot, stockpile or piece of furniture | |
| Doors and hatches | building sheet: Lock / Unlock, Forbid / Allow passage, Pet-passable, Internal | |
| Levers | building sheet: "Pull lever", "Link to..." then click a door, bridge, floodgate, hatch, cage, grate, bars or spikes | right click / Esc cancels linking |
| Trade | trade depot sheet "Trade" / "Move goods to depot", or "Trade" in the caravan box | |
| Nobles | toolbar (left group, crown) | `n` (DF's `D_NOBLES`) |
| Settings (population cap, invasions) | Esc menu "Settings" | |
| Work orders | toolbar (left group) | `o` |
| Labor: work details, kitchen, all labors | toolbar (left group) | `y` |
| Places: locations, zones, stockpiles, workshops, farm plots | toolbar (left group) | `P` (DF's `D_LOCATIONS`) |
| Objects: artifacts | toolbar (left group) | `O` (`D_ARTLIST`) |
| Justice | toolbar (left group, scales) | `j` (`D_JUSTICE`) |
| World: civilizations, caravans | toolbar (right group, globe) | `Y` (`D_WORLD`) |
| Pets and livestock | Creatures > Pets/livestock > "Animals: manage...", or a pen's "Pick animals..." | |
| Stockpile settings | stockpile sheet "Settings, links...", or Places > Stockpiles > Settings | |
| Minimap | `▓` button in the top bar (left of `?`) | |
| Save | Esc menu "Save game" | F5 (quicksave) |
| Load | Esc menu "Load game" (list of saves, newest first) | |
| Controls overview | `?` button (top bar, right) | F1 or `?` |
| Back to the title screen | Esc menu "Save and return to title" | |

To place designations, stockpiles, zones and farm plots, either press, drag and release, or click
once and click the second corner. Changing the z-level between the two corners makes a 3D box. While you place
it, the box size (`5x3x2`) follows the cursor.

The top bar shows pause/speed, the date (`15th Granite, 250`), the season, the **ticks/s
actually reached** (green = on target), FPS, the sim milliseconds per frame, population, and the
z-level (plus the elevation for imported forts). Hover the population for citizens, children,
the population cap, the next migrant wave and the expected caravan (`Sim::population_info`);
the Creatures screen shows the same line above the citizens. The bar at the right edge shows sky, surface
and underground; click it to jump. Announcements appear at the bottom left; click one that has a
position to jump there.

## Playing the fort

- **Build menu** (`b`): categories and buildings come from `Sim::build_options()` with DF's
  `BUILDING_ICON_*` icons. The ghost is the building's own sprite, tinted green when
  `Sim::can_place` accepts it and red (with the reason) when not; fixed footprints are centered
  on the cursor like DF. The tool stays active for placing more; right-click goes back to the
  list, a second right-click closes the menu. Unbuilt buildings are drawn translucent (workshops
  with DF's construction-stage sprites) with a dashed yellow frame and a hammer.
- **Constructions** (round 5, Build menu > Constructions, `ui/round5.rs`): Wall, Floor, Ramp,
  Up/Down/Up-down stair, Fortification, Track and Remove construction. Drag a rectangle on one
  level; the preview draws DF's `PLANNED_CONSTRUCTION_*` blueprint on every tile, green where it
  fits and red where not (a quick UI check: revealed, no building, floor for walls, floor or open
  space for floors; the sim decides), with "12 ok, 9 blocked" at the cursor. Releasing calls
  `Sim::build_construction`; the status says how many tiles were planned. Planned constructions
  (buildings of kind `Construction` that are not built yet) are drawn as DF's translucent
  blueprint, tinted with the material once one is assigned; tracks connect to their neighbors;
  removals get DF's remove-construction marker. The tooltip says "Wall (planned)".
- **Smooth / engrave / carve** (`v`): drag a box over levels like digging
  (`Sim::designate_detail`); designated tiles show DF's `DESIGNATION_SMOOTH` / `_ENGRAVE` /
  `_FORTIFY` / `_TRACK_*` markers (blue-tinted in marker mode). Carve designations have no tile
  flag in rf-core, so the UI remembers them for the overlay until the tile changes.
- **Traffic** (`Shift+t`): High (path cost 1), Normal (2), Low (5), Restricted (25) over a box
  (`Sim::set_traffic`). Like DF, the traffic colors (`DESIGNATION_TRAFFIC_*`: green chevrons
  high, orange low, red restricted) are only drawn while the traffic menu is open; the tooltip
  always says `[low traffic]` etc.
- **Item designations** (`i`): drag a box (`Sim::set_items_flag_in`). Forbidden items carry DF's
  padlock, dump-marked ones the bin (`DESIGNATION_ITEM_FORBIDDEN`, `_DUMP`, `_FORBIDDEN_DUMP`);
  hidden items are not drawn, except with their marker while the item menu is open. The tile
  info lists every item with `F` / `D` toggles (`Sim::set_item_flag`) and "Forbid all" / "Claim
  all"; forbidden lines are red. The unit sheet's Items tab has the same toggles.
- **Bridges and cages** (rf-gfx): a raised drawbridge is drawn as DF's `BLD_BRIDGE_*_RAISED_*`
  wall on its raising edge (the rest of its area shows the floor below), a retracted bridge not
  at all, a lowered one with its planks along the raise direction. Caught creatures are drawn
  inside their cage (the occupied cage sprite in the cage's material, the creature at 85 %, the
  bars faintly in front) on cage traps and on loose occupied cages.
- **Moods and treatment** (round 5): the overview shows "Fey mood: claimed Craftsdwarf's
  workshop, needs rough gems" (`Sim::mood_info`); the Mood tab (enabled only during a mood) has
  the mood in DF's words, the claimed workshop with "Go to workshop", the missing materials and
  the time left. The Health tab gets a Treatment block after the status, derived from DF's wound
  flags: Diagnosis / Cleaning (infection) / Suturing / Dressing / Setting (broken bone) /
  Immobilization required, what is done (diagnosed, sutured, ...), permanent damage, and the
  current activity when it is a rest or treatment job.
- **Zones** (`z`): DF only shows zones in zone mode, and so does Rustfort: the translucent
  `ACTIVITY_ZONES` borders plus the zone-type icon appear while the zone menu is open or a zone
  is selected.
- **Building sheet** (click a building): name, material, built state and "Remove". Workshops show
  their task queue (repeat `R`, suspend `S`, cancel `x`) and "+ Add job" with the jobs from
  `Sim::templates_for_workshop` (tick "Repeat" first for repeating jobs). Farm plots have one
  crop per season (`Sim::crop_options`, crops not plantable in that season are greyed out).
  Stockpiles show their categories as checkboxes, their containers and links, and "Settings,
  links..." for the full editor (round 8). Zones show their location; a pen has "Pick
  animals...", a meeting area without a location "Make location...".
- **Work orders** (`o`): every manager order with progress (left/total), frequency, conditions
  (e.g. `boulder (non economic, hard) ≥ 22; slab < 2`), workshop and status (√ active,
  · waiting). "+ New order" opens a window: pick a template, set the amount with -/+ (Shift =
  10) or the wheel, pick the frequency, "Create order". `x` removes an order.
- **Labor** (`y`, round 8: `ui/workdetails.rs`) has three tabs like DF Steam:
  - *Work details*: the details on the left with DF's `WORK_DETAIL_*` icons and "5 assigned" /
    "everybody" / "nobody". On the right the selected detail: its labors, the mode buttons
    (Everybody does this / Only selected do this / Nobody does this), "Assigned workers also do
    other jobs" / "only do this", "Edit labors" (your own details; DF's eleven built-in ones keep
    theirs), Delete. Below, every citizen with a toggle, profession, best skill of the detail's
    labors and their other details; click to assign (`Sim::set_work_detail_member`,
    `set_work_detail_mode`, `set_work_detail_labors`). "+ New work detail" asks for a name, then
    opens the labor editor (`Sim::create_work_detail`). An export without work details offers
    "Set up DF's default work details".
  - *Kitchen*: every cookable or brewable kind in stock (plants, fruit, seeds, drinks, meat,
    fish, eggs, cheese, milk, flour, fat) with DF's cook and brew toggles
    (`LABOR_KITCHEN_*_ALLOWED/RESTRICTED/CANNOT`, `Sim::set_kitchen`). Plump helmets start as
    "no cook" (Gordon's rule) when the save has no entry for them.
  - *Advanced: all labors*: the old grid, citizens x labors with DF's `unit_labor` names in tabs
    (Core, Farming & food, Wood & stone, Crafts & metal, Health & other). Cells show the related
    skill level; click a cell to toggle (`Sim::set_labor`), click a column header to toggle it
    for everyone shown, type in the filter box to filter by name or profession, wheel scrolls.
    The footer counts how many have each labor (red = nobody).
- **Stockpile settings** (round 8, `ui/stockpile_ed.rs`): DF Steam's editor. Presets (DF's
  custom-stockpile buttons: Everything, Organic, Inorganic, Empty cages, Empty animal traps,
  Prepared food, Usable/Unusable weapons and armor, Corpses, Nothing); Barrels / Bins /
  Wheelbarrows with -/+ (Shift = 10); "Give to..." / "Take from..." switch to the map, where you
  click another stockpile or a workshop (linkable buildings are outlined; links between piles
  are kept on both piles; right-click returns); "Takes from anywhere / links only". Below, three
  columns: the categories with DF's on/partial/off marks (click the mark to switch the whole
  category), the groups of the selected category (Meat, Fish, Plants, ...; stone is split into
  metal ores / economic / other / clay like DF Steam), and the leaves with checkboxes plus
  "Enable all" / "Disable all". Everything goes through `Sim::set_stockpile_settings`; leaves come
  from `Sim::stockpile_filter_options`, or a stand-in built from the save's raws while the sim
  has none. Filters are stored like the exporter writes them: `food/plants/PLANT:BARLEY:STRUCTURAL`,
  `food/meat/*` for a whole group, `finished_goods/type/!CROWN` for all but one. With a pile
  selected, its links are drawn on the map (green gives, orange takes).
- **Pets and livestock** (round 8, `ui/animals.rs`, `Sim::animals`): every tame animal with its
  sprite, name, creature and sex, training, owner, pen/pasture and state (pregnant, to
  slaughter, grazer, not on the map), tabs All / On the map / Pastured / For slaughter / Pets /
  Pregnant, and DF's buttons: slaughter (`Sim::set_slaughter`), war and hunting training
  (`Sim::request_training`), and the pen/pasture picker (Pen zones, `Sim::assign_pasture`).
- **Places** (`P`, round 8, `ui/places.rs`): *Locations* (temples, taverns, libraries,
  guildhalls, hospitals with their zone, deity, visitors switch and occupations with Assign /
  Replace: a citizen picker, `Sim::assign_occupation`), *Zones* (every zone; meeting areas get
  "Make location": kind, then for a temple the deity, from the gods the citizens worship with
  the number of worshippers: `Sim::create_location`), *Stockpiles* (with "Settings"),
  *Workshops* and *Farm plots*. Click a name to go there.
- **Justice** (`j`): open cases (crime, victim, year, witnesses, "Convict...": a citizen picker
  with the witnesses first, `Sim::convict`), closed cases with the convicted and the
  punishment, and the convicts. Names of dead or departed dwarves come from the export.
- **Objects** (`O`): every artifact with DF's item name ("Battle Axe"), material, where it is
  (carried by, in a building, a stockpile or a named room) and its maker (for artifacts made in
  Rustfort); click one to go there.
- **World** (`Y`): the civilizations with race, kind and relation (yours, at war, at peace, no
  contact) and whether they send caravans; *All entities*; *Caravans* (the caravan at the depot
  with "Trade", the expected caravan, the save's caravans, trade partners).
- **Minimap** (round 8, `ui/minimap.rs`): the `▓` button in the top bar shows the current
  level at the top right (left of an open sheet): terrain colors (walls darker, open space shows
  the level below dimmed, water, magma), buildings, citizens (yellow), animals (blue), hostiles
  (red) and the camera's view; click or drag on it to move there. Remembered in the settings.
- **Unit sheet** (click a creature): the DF Premium portrait at the top (see "Feel round"),
  then two rows of tabs like the Steam sheet.
  - *Overview*: hunger/thirst/tiredness bars, the DF stress category with its face and color
    (`Sim::stress_level`: Ecstatic, Happy, Content, Fine, Stressed, Very stressed, Breaking
    down), the latest thought, open wounds and the squad.
  - *Thoughts*: the remembered emotions newest first (`Sim::describe_emotion`), green for
    pleasant, red for unpleasant emotions (brighter when strong), with "today / 5 days ago".
    DF's placeholder memories (`ANYTHING`) are hidden, as in DF.
  - *Needs*: one line per need with a focus gauge and DF's wording (Badly distracted,
    Distracted, Unfocused, Not distracted, Untroubled, Level-headed, Unfettered), e.g.
    "Distracted by being away from friends"; strongest needs first.
  - *Personality*: facets as sentences for DF's non-average levels (0-9, 10-24, 25-39, 61-75,
    76-90, 91-100; the sentences paraphrase DF's), then beliefs (values) with their strength.
  - *Health*: status (bleeding, pain, infection, blood loss), blood bar, open wounds per body
    part (names from the creature raws) and healed scars.
  - *Relations*: family, worship, friends, kindred spirits, grudges, pets, acquaintances; click
    a name to open that unit. Figures that are not on the map show their id.
- **Squads** (`c`): squads with members, orders and uniforms. Orders: *Station* (click a tile),
  *Kill* (pick a hostile from the list or "Pick on map"), *Train*, *Cancel orders*
  (`Sim::order_squad`). "+ Assign" opens a citizen picker (best weapon skill first), "Remove"
  empties a position, "+ Create squad" makes a new one. Right-click or Esc during Station/Kill
  returns to the squads.
  [app9] Tabs below the orders: *Members*; *Equipment* (DF's per-position uniform screen: the
  positions with worn and missing pieces, and for the selected position one row per slot
  (body, head, legs, hands, feet, shield, weapon) with `<` `>` for the item (none, any, or a type
  from the save's armor/weapon raws) and a material button (any, metal, steel, iron, bronze,
  copper, leather, wood, bone); *Use squad uniform* clears the override, *Copy to all positions*;
  `Sim::set_squad_uniform`, `Sim::squad_equipment`); *Ammunition* (entries with amount
  -100/-10/+10/+100, material class, combat/training flags, remove, "+ bolts"/"+ arrows"/...
  from the ammo raws; rounds carried per member; `Sim::set_squad_ammo`); *Patrol and barracks*
  (route waypoints; *New route (pick on map)*: click waypoints, Enter finishes
  (`Sim::order_squad_patrol`), right-click removes the last, Esc cancels; *Patrol*, *Show route*
  (drawn on the map with numbered waypoints), *Clear route*; barracks zones with checkboxes,
  `Sim::set_squad_barracks`).
- **Burrows** (`Shift+u`): the burrow list with tile and unit counts; burrow tiles of the
  current z-level are drawn as a colored overlay (the selected burrow with its outline), like
  DF only while the menu is open. "+ New burrow" and drag a box (3D like digging) creates one
  (`Sim::create_burrow`). "Alert" turns the civilian alert on for that burrow
  (`Sim::set_civilian_alert`); the top bar then says "Civilian alert: <burrow>".
- **Reports** (`Shift+n`): every announcement of the session with its game date, newest first,
  filtered by All / Combat / Deaths / Jobs / Info, plus the combat log (`Sim::combat_log`).
  Click a line with `»` to jump there. [app9] *Combat reports* groups the combat lines per fight
  like DF: a line joins an open fight (last line at most 600 ticks ago) that shares a
  participant (citizens and other creatures by name, unnamed ones as "the goblin lasher"), else it
  starts a new one. The list shows "Urist McAxe and 2 more vs Snang", date, lines and deaths;
  the right side the fight's lines and *Go there*.
- **Hostiles**: hostile creatures the player can see (`Game::visible_hostiles`, see "Feel
  round") get a pulsing red frame with a `!` badge
  (not when caged); creatures with open wounds get a blood drop. The top bar counts the
  hostiles; click the counter to center on the next one. When new hostiles arrive, Rustfort
  pauses and shows a pop-up like DF ("A vile force of darkness has arrived!" for a goblin
  siege, "An ambush! Curse them!", "<Name> has come!" for megabeasts) with "Center on them".
  Hostiles arriving within a day of the first belong to the same wave. Wild animals only get an
  announcement. A siege already in progress when the fort is loaded only gets an announcement.
  [app9] When the sim spots a raid in the same frame, its line names the box ("Thief! Protect
  the hoard from skulking filth!", "Snatcher! Protect the children!"); whether a box opens,
  the game pauses or the camera recenters follows the announcement settings. Hidden raiders
  stay invisible until spotted.
- **Announcement boxes and settings** ([app9], `ui/round9.rs`): every sim line is matched to an
  announcement type (siege, ambush, thief, snatcher, abducted, megabeast, citizen/pet/enemy
  death, migrants, caravan, visitor, petition, mood, artifact, birth, animals, tantrum, justice,
  nobles, ghosts, cave-in, traps, destroyed buildings, job cancellations, other). Each type has
  DF's flags `BOX` (a box that pauses), `P`, `R` and `D_D` (shown in the announcement lines; the
  Reports keep every line). The defaults come from `%APPDATA%/Bay 12 Games/Dwarf
  Fortress/prefs/announcements.txt`, then the install's `prefs/` and `data/init/` files (the
  first DF token of the type that is found); Esc > Settings > *Announcements...* changes them
  and `saves/settings.json` keeps the changes. Boxes queue ("1 of 5"; from the fifth on, more of
  the same type fold into "... and N more"); Enter or Esc closes the front box, *Close all*
  every box, *Go there* centers on it.
- **Petitions** ([app9]): a pending visitor petition (`Sim::petitions`) opens a box with Accept /
  Deny / Petitions / Later (if the PETITION type has a box) and shows a blinking "Petitions (n)"
  badge at the top left. The petitions screen lists all petitions (pending first) with role,
  request, location, age and days left (28-day timeout) and answers with
  `Sim::answer_petition`; *Show visitor* selects the visitor. Temple and guildhall petitions of
  religions and guilds are listed too.
- **Doors, levers, bridges, traps** (round 4, `ui/devices.rs`): the building sheet shows the
  state from the building flags (`closed, locked`, `raised`, `pulled`, `armed`). Doors and
  hatches get Lock / Forbid passage / Pet-passable / Internal (`Sim::set_building_flag` with
  `LOCKED`, `FORBIDDEN`, `PET_PASSABLE`, `INTERNAL`). Levers and pressure plates get "Pull lever"
  (`Sim::pull_lever`) and "Link to..." (`Sim::link_building`; linkable targets on the current
  level are outlined, the one under the mouse in yellow; the hint shows the mechanisms in stock,
  DF needs 2 per link). The sheet lists the links in both directions (lever -> targets, target
  -> levers) with their state; click one to go there. On the map the selected device's links are
  outlined with a line to each (targets on other levels get a `z131` marker). Bridges show the
  raise direction, traps their contents (weapons, stone, a caught creature).
- **Trade** (round 4, `ui/trade.rs`): two columns like the Steam screen, the merchants' goods
  (`TradeStatus::goods`) and your goods in the depot (`fort_goods`), each with DF's own item
  description and value in ☼ (a bin counts with its contents). Click items to select; the
  merchants' valuation (`Sim::trade(offer, request, false)`) updates with the selection,
  "Offer" shows their reply, "Trade" commits. "Move goods to depot" lists tradeable goods
  (top-level items, not forbidden, owned, rotten, built or artifacts) by value with a search
  field; "Bring N to the depot" calls `Sim::move_to_depot`. When a caravan appears
  (`Sim::trade_status`), the log says "A caravan from X has arrived." (unless the sim announced
  it) and, if your DF `announcements.txt` gives `CARAVAN_ARRIVAL` the `BOX` (or `P`) flag, as
  yours does, the game pauses with a box offering Trade / Go to depot / OK.
- **Nobles** (round 4, `ui/nobles.rs`): every position of `World::nobles` with its holder and
  mandates. How each is filled follows the vanilla dwarf entity raws: manager, bookkeeper,
  broker, chief medical dwarf, militia commander, sheriff, captain of the guard and hammerer are
  appointed by the fort ("Appoint" / "Replace" opens a citizen picker sorted by the position's
  skill, `Sim::appoint`), the mayor is elected, militia captains are set in the squads screen,
  barons and the like are appointed by the monarch.
- **Settings** (Esc menu): population cap -10/-1/+1/+10 (`Sim::set_population_cap`, DF's
  d_init `POPULATION_CAP`) and the invasions switch (`Sim::set_invasions`); [app9]
  *Announcements...* opens the announcement settings.
- **Skills** ([app9]): the unit sheet's Skills tab lists DF's skill names with the level
  ("Legendary +2 Miner", "High Master Axe Dwarf"), an experience bar towards the next level
  (DF: 500 + 100 x level) and a tooltip with the token and "x / y experience to <level>"; the
  wheel scrolls. Tooltips of the toolbar, pause and speed buttons show the key and DF's binding
  name.
- **Liquids**: water and magma use Premium's `LIQUIDS` page, one sprite per depth 1-7 (1-3
  translucent, 4-7 opaque and darker) with the depth number, and `UNDERWATER_EDGE_*` foam on the
  dry tiles around a pool. Tiles are redrawn every frame, so flowing water moves as the sim
  changes it. The tooltip says e.g. "Water depth 4/7 (wading depth)" (plus salty, stagnant).
- **Stocks** in the top bar: drink units, food units and distinct foods, recounted every second
  (red below 2 per citizen). Food = prepared meals, meat, fish, cheese, eggs and plants that
  are edible raw or cooked according to the save's plant raws; rotten and dumped items do not
  count.

### Round 10: tasks, hauling, build keys, context menu, look lines, crash guard

- **Tasks** (DF `D_JOBLIST`, `t` in Gordon's prefs; the toolbar's Tasks button): every current
  job with its worker and place, in tabs All / Designations / Building / Workshops / Hauling /
  Medical / Military / Personal / Other with counts, plus "Only without worker". Sources:
  `Sim::jobs()` (dig, chop, gather), `Sim::construction_jobs()` (constructions, smoothing,
  engraving, dumping), unbuilt buildings, workshop queues (`Building::jobs`; the worker is the
  citizen whose job target is in the workshop) and `Sim::unit_status` for everything else
  (hauling, medical care, soldiers, eating/drinking/sleeping). Clicking a row closes the screen,
  centers on the place and selects the worker (or the building/tile). Workshop jobs have
  Suspend/Resume and "Do now" (moves the job to the front of the queue); designations,
  smoothing/engraving, planned constructions, planned buildings, workshop jobs and dump marks
  can be cancelled. Deconstructions and a creature's own activities cannot (no sim API yet).
  The list is rebuilt twice a second while the screen is open.
- **Hauling** (`D_HAULING`, Shift+h): the hauling routes of the import (`World::haul_routes`)
  with their stops (position with "Go"), departure conditions and linked stockpiles.
  Read-only: Rustfort does not simulate minecarts, and Gordon's fort has no routes.
- **Build hotkey chains** from `interface.txt`: `b`, then the category key
  (`HOTKEY_BUILDING_WORKSHOP` o, `_FURNITURE` f, `_CONSTRUCTION` n, `_FARMPLOT` p, `_WELL`
  Shift+l), then the building. Workshops have DF's sub-groups: `HOTKEY_BUILDING_FURNACE` (u),
  `_FARMING` (f: farmer, quern, millstone, still, kitchen, fishery, butcher) and
  `_WORKSHOPS_CLOTHING_LEATHER` (l: leather works, loom, clothier, dyer, tanner). So `b o t` is
  the mason's workshop, `b o f l` the still, `b o u l` the smelter, `b n l` a wall. A key bar
  under the top bar shows where the chain is and what each key does; menu buttons carry their
  key ("Still (l)"). Esc goes back one level (first from placing to the list), a clicked tab
  moves the chain there. Chair and door share `r` in DF: pressing it again cycles. Designations
  were already DF's chains (`m` then `m`/`u`/`t`/`r`/`x`, `v` then smooth/engrave ...).
- **Map context menu**: right-click with no tool active opens a menu for the tile under the
  mouse: select each creature, open each building or zone, cancel the dig designation, the
  smoothing/engraving designation or a planned construction, look at the tile. Esc, a second
  right-click or a click elsewhere closes it. A tile with nothing on it deselects as before.
- **Look lines** (tile sheet): "A rough floor of rock salt, never smoothed." / "A smoothed wall
  of granite." / "A constructed wall of ...", and for engraved tiles "The floor is engraved with
  a superior image of the fortress. *", the engraver with skill level and the value. DF does not
  export the picture's subject, so the subject is a generic stand-in (the sheet says so). The
  hover box adds "Masterful engraving ☼". Items are grouped in the sheet and the hover box
  ("6 x +iron bucket+"); a group's F/D toggles act on all its items.
- **Crash guard** (`crash.rs`, `simthread.rs`, `ui/round10.rs`): every panic is written with
  thread, place and backtrace to `saves/crash.log`. A panic in a sim tick (worker thread or
  inline) is caught: the worker hands the sim back with the message and stops ticking. A panic
  in the frame's input/drawing code is caught in `game::run`. Either way the crash dialog
  replaces the window's content: error, place, the last good state, and **Save last good
  state** (`saves/<fort>-crash-<time>.rfsave`), **Save it and continue from there** (saves,
  then loads it), **Save current state (may be broken)** (sim panics) or **Save now** / **Try
  to continue** (interface panics), **Return to title**, **Quit**. The last good state is an
  in-memory copy of the world taken when the game starts and then every 60 s of real time while
  the sim runs (a clone, about 60-270 ms on the real fort).
- **Title screen > Settings > Announcements...**: the same announcement table as in the game
  (round 9), saved in `saves/settings.json`.

### Feel round: right-click, seen hostiles, portraits (tag `[feel-ui]`)

- **Right-click closes the topmost window**, one per click, anywhere in the window, like DF
  Steam (`ui/rclick.rs`, run every frame before the interface; a right-click that closed
  something is consumed):
  1. modal popups, dialogs and boxes: help, hostile alert, caravan box, announcement boxes,
     map context menu, Esc menu (sub-page first), pickers and name fields, new work order,
     move goods, appoint, equipment position, add job, crop season; and the map pick modes
     that belong to a screen or sheet: lever linking, stockpile links (back to the settings),
     squad station/kill (back to the squads screen), patrol (removes the last waypoint, then
     back);
  2. big screens: orders, labor, squads, reports, trade, nobles, stockpile settings, animals,
     places, justice, objects, world, petitions, tasks, hauling; then the creatures list;
  3. the side panel: unit, building, zone or tile sheet;
  4. on the map: the pending drag box, then the tool (build/construct, zone and burrow tools
     go back to their menu; the build menu then goes one hotkey-chain level up), then the
     menu (dig, build, zone, stockpile, smooth, traffic, items, burrows);
  5. the patrol route display.

  Only when nothing is open does a right-click on the map open the context menu (round 10).
  Checked with `--script "4:rclick"` on `orders`, `neworder`, `ctx`, `place:Still`,
  `dig,drag`, `zone`, `zone:0`, `squads,station`, `squads,patrol-tool`, `squads,assign:1`,
  `pile`, `labor-screen`, `tasks`, `menu`, `help`, `spawn-hostile`, `bld:Workshop,addjob`,
  `bld:Lever,link`, `citizens,unit:<id>` and `unit:<id>,orders` (two clicks: screen, then sheet).
- **Seen hostiles only**: the top bar's "N hostiles" counter and its cycle-through click, the
  red markers (map, minimap), the squads' kill list and the arrival alert/auto-pause use
  `Game::visible_hostiles`: `Sim::hostiles()` (no unspotted raiders, `Sim::unit_hidden`) on a
  revealed tile and either reachable to a citizen (`Sim::reachable`, one citizen per walking
  component) or within 20 tiles of one (`alerts::SEEN_RANGE`). On Trussroads the four sealed
  demons at z 0 (elevation -100) no longer count; the four goblin pikemen at the gate do. A
  hostile that becomes visible later (e.g. a chamber is dug open) raises the alert then. The
  hover box still tags any non-hidden hostile "(hostile)".
- **Portraits** (`Gfx::portrait(world, unit, out) -> bool`, rf-gfx): DF Premium's
  `[LAYER_SET:PORTRAIT]` (and `CHILD:PORTRAIT`, `BABY:PORTRAIT`) evaluated with the map
  sprites' layer rules, now also with `CONDITION_BP` + `BP_APPEARANCE_MODIFIER_RANGE` (head
  broadness, eye shape, nose, ...; deterministic per unit, the export has no appearance data),
  `ITEM_QUALITY`, tissue density (eyebrows) and `TISSUE_SWAP:IF_MIN_CURLY`. Skin, hair, beard
  and eyebrow colors/lengths/shapes are the same stand-ins the map sprite uses; a race whose
  portraits know other colors (green goblins) gets one of those. Clothing and armor come from
  the worn items, recolored by their material (`USE_STANDARD_PALETTE_FROM_ITEM`; the default
  outfit gets a cloth color). The sheet draws DF's portrait background, the layers at a
  whole-number scale of 96 px, and DF's frame; creatures without a portrait (procedural
  demons, forgotten beasts) get their map sprite scaled up, and without that the glyph. The
  stress face and level sit under the job, like the Steam header. Cached per unit until race,
  caste, profession, age class or inventory change.
- rf-gfx fixes on the way: `CONDITION_RANDOM_PART_INDEX` is 1-based in DF (one in N units
  used to get no variant, e.g. no shirt), and beards never use the pony-tail shape (DF has no
  such beard layer).

## Saving

Saves are the whole `rf_core::World` as bincode, zstd-compressed (level 3, multithreaded), in
`saves/<fort>-<yyyymmdd-hhmmss>.rfsave` (UTC) relative to the working directory, or in
`$RUSTFORT_SAVE_DIR`. A small header (fort, game date, seed) lets the load menu list saves
without unpacking them. Saving clones the world and writes in a background thread, so play
goes on. Loading rebuilds the simulation with `Sim::new(world, seed)` (jobs in progress and
paths start fresh, like after an import); designations, buildings, stockpiles, zones, work
orders, labors and needs are all in the world and survive.

Measured on the real fort (192x192x150, 320 units, 14.4k items, 1177 buildings):
save 0.07 s, 1.4 MB on disk (45.5 MB raw bincode); load 0.11 s plus 0.05 s `Sim::new`.

## Speed model

Since round 4 the sim runs on its own thread (`simthread.rs`). The UI thread holds the `Sim`
while it handles input and builds the frame (about 1 ms), then hands it to the worker with a tick
allowance before the GPU flush and the vsync wait, and takes it back at the start of the next
frame. The `Sim` is moved, never shared, so there are no locks and the sim stays deterministic.
The worker runs `Sim::run_ticks` in chunks of about 0.5 ms and checks between chunks whether the
UI wants it back. At "max" the allowance is unlimited; at 1x/2x/5x a tick accumulator targets
100/200/500 ticks/s.

Measured on the real fort (round-3 export, 357 units, running siege), `--speed max`, game span
tick 60,000 to 260,000 (`RUSTFORT_MEASURE=60000,260000`), Ryzen 5 5600X with other builds
running:

| | ticks/s | frame CPU | FPS |
|---|---|---|---|
| sim in the frame (`--sim-inline`, the old model: 12 ms budget per frame) | 44,600-48,100 | 12.7-14.2 ms | 58-59 |
| sim thread | 58,300-62,800 (worker busy 93 %) | 0.6-0.7 ms | 60 |

`--sim-inline` (or `RUSTFORT_SIM_THREAD=0`) keeps the old model for comparisons.

Vsync is on in play mode and off for screenshots. `RUSTFORT_VSYNC=0` turns it off, and the app
then paces itself to `--gfps N` (default 60).

## Screenshot tests

```
rustfort play --screenshot target/shots/a.png --frames 20 --view 48,48,22,1 \
  --ui unit,citizens --script "3:key m; 5:move 600,300; 6:ldown; 8:wheel 1; 10:move 1000,560; 12:lup"
```

- `--view x,y,z,zoom` sets the camera.
- `--ui` opens states: `citizens`, `pets`, `others`, `sort:<name|profession|stress>`, `unit[:id]`, `skills`,
  `labor`, `items`, `thoughts`, `needs`, `personality`, `health`, `relations`, `dig`, `chop`,
  `stockpile`, `menu`, `loadmenu`, `tile`, `drag`, `build[:category]`, `place:<label|subtype|index>`
  (build tool with a ghost), `zone[:kind index]`, `orders`, `neworder[:template]`,
  `labor-screen`, `bld:<id|kind|subtype>` (select a building and center on it), `addjob`,
  `crop:<season>`, `squads[:id]`, `assign:<position>` (citizen picker), `killlist`, `station`
  (station tool), `burrows[:id]`, `burrow-tool`, `reports[:filter]`, and `spawn-hostile`
  (test hook: the 8 non-citizens nearest the camera turn hostile, so the arrival alert fires).
  Round 4: `trade`, `depot[:search]` (move-goods picker), `nobles`, `appoint[:TOKEN]`,
  `settings`, `link` (link tool for the building selected with `bld:`), `caravan-popup` (the
  arrival box without a caravan), `fake-caravan` (made-up merchant goods to check the trade
  screen layout while the sim has no caravan; trading still goes to the sim).
  Round 5: `construct:<Wall|Floor|Ramp|UpStair|DownStair|UpDownStair|Fortification|Track|Remove>`
  (Build > Constructions with that tool), `smooth[:Smooth|Engrave|CarveTrack|CarveFortification|Remove]`,
  `traffic[:High|Normal|Low|Restricted]`, `items-tool[:Claim|Forbid|Dump|...]`, `mood` (unit
  sheet Mood tab), and the test hooks `fake-constructions` (a row of planned constructions next
  to the camera, to check the blueprint look) and `fake-mood[:Fey|Secretive|...]` (the selected
  unit is in a mood).
- Screenshot runs print the average ticks/s and, with the sim thread, how busy the worker was;
  `RUSTFORT_MEASURE=a,b` prints the real time from sim tick a to b.
- `--mouse x,y` parks the cursor.
- Round 8: `workdetails[:index|name]`, `wd-edit` (labor editor), `wd-new:<name>` (name field;
  `--script "3:key enter"` creates it), `wd-defaults` (DF's default details), `kitchen`,
  `labor-grid`, `pile[:id]` (stockpile editor, default the largest pile), `pile-cat:<n|name>`,
  `pile-group:<n>`, `pile-link:give|take` (map link tool), `animals[:tab]`, `pasture-pick[:unit]`,
  `places[:tab]`, `make-location[:zone]`, `temple-pick`, `occupation-pick`, `justice[:tab]`,
  `convict[:crime]`, `objects`, `world[:tab]`, `minimap`, and the test hooks `fake-locations`
  (a tavern and a temple on the first meeting areas) and `fake-crimes` (two open and one closed
  case).
- Round 9: `petitions`, `announcements` (settings page), `squad-tab:<1|2|3>` (Equipment,
  Ammunition, Patrol and barracks; with `squads:<id>`), `eq-pos:<n>`, `patrol-tool` (a made-up
  3-point route around the camera), `fights` (Reports > Combat reports), and the test hooks
  `fake-petition[:performer|scholar|mercenary|slayer]` (a visitor arrives and the sim runs up to
  40 days until it petitions), `fake-raid[:thief|snatcher|ambush|abducted|death|mood]` (a sim-like
  line through the announcement path), `force-raid[:siege|ambush|thief|snatcher]`
  (`Sim::force_raid`) and `fake-fights` (two made-up fights).
- Round 10: `tasks[:tab]`, `hauling`, `bpath:<Workshops/Farming|Workshops/Furnaces|Furniture|...>`
  (build menu at a hotkey-chain level), `ctx` (context menu of the camera's tile), `engraved`
  (select an engraved tile of quality superior or better), `items-tile` (the tile with the most
  items on the level), and the test hooks `fake-route` (a 2-stop route between the first two
  stockpiles), `crash` (the crash dialog alone), `crash:sim` (a real panic on the sim thread:
  the units are removed behind the sim's back) and `crash:ui` (a real panic in the interface).
  `--menu-page announcements` opens the title screen's announcement page. Example:
  `--script "4:key b; 6:key o; 8:key f; 10:key l"` ends with the still being placed.
- Round 6: `--menu` shows the title screen even with `--screenshot` (the script can click on,
  and the frame count continues into the game); `--menu-page load|exports|import|settings`
  opens a page; `--menu-page import:go` starts the import as soon as DF answers (end-to-end
  test; it reads the live game, so run it on purpose only). `--shots-at 300,900` saves extra
  screenshots `<shot>-f300.png` on the way (e.g. the import's progress bar). `--ui help` opens
  the controls overview, `--ui hints[:n]` shows the first-time hints from hint n (hints are off
  in screenshot runs otherwise). `--autosave-days N` overrides the autosave interval.
  `RUSTFORT_DF_PROCESS=NoSuchDF.exe` and `DFHACK_PORT=5999` give the "DF is not running" and
  "DFHack does not answer" states of the import dialog without touching DF.
- `--script` replays input per frame: `move`, `click`, `ldown`, `lup`, `rclick`, `mdown`,
  `mup`, `wheel n`, `key k` (also `f5`, `backspace`, `tab`), `char c`, `mods shift|ctrl|none`.
  In screenshot runs a save started with F5 finishes within the frame.

With a script, the real mouse and keyboard are ignored, so runs are reproducible. Keep
screenshots under `target/`, never in git.

## Code map

- `main.rs`: CLI.
- `menu.rs` (round 6): title screen and its pages, loading thread, `app_main` (title screen and
  game in turn).
- `importer.rs` (round 6): DF/DFHack check, exporter start, `progress.json` polling, export list.
- `settings.rs` (round 6): `saves/settings.json`.
- `game.rs`: state, sim loop, camera, map input.
- `render.rs`: map, fallback tiles, overlays.
- `keys.rs`: DF `interface.txt` bindings and key repeat.
- `font.rs`: DF CP437 bitmap font.
- `script.rs`: scripted input.
- `bench.rs`: headless benchmark.
- `simthread.rs`: the sim worker thread (`SimHandle` derefs to `Sim` while the UI holds it).
- `save.rs`: save games (bincode + zstd), save list, `savebench`; (round 6) tagged autosaves and
  their rotation.
- `ui/help.rs` (round 6): controls overview and first-time hints.
- Feel round: `ui/rclick.rs` (right-click priority), `ui/alerts.rs` (`visible_hostiles`),
  `ui/panels.rs` (`draw_portrait`), rf-gfx `Gfx::portrait` / `creature::evaluate_portrait`.
- Round 10: `ui/round10.rs` (Tasks and Hauling screens, build hotkey tree `BUILD_KEYS` and the
  key bar, map context menu, look lines and item groups, crash snapshot and dialog, title-screen
  announcement page) and `crash.rs` (panic hook, `saves/crash.log`).
- Round 9: `ui/round9.rs` (announcement types and settings, box queue, raid boxes, petitions,
  squad equipment/ammo/patrol/barracks tabs, patrol tool and route overlay, combat reports per
  fight, DF skill names and experience bars).
- Round 8: `ui/round8.rs` (state, pickers, `--ui` states, Esc, stockpile link tool and map
  lines), `ui/workdetails.rs` (Labor tabs: work details, kitchen), `ui/stockpile_ed.rs`,
  `ui/animals.rs`, `ui/places.rs` (Places, Justice, Objects, World), `ui/minimap.rs`.
  `load.rs` fills work details, kitchen, locations, crimes and hauling routes from the round-6
  export's side files when the importer left them empty.
- `ui/`: `topbar` (plus stocks, hostile counter), `toolbar` (data-driven button tables plus
  Dig/Stockpile submenus), `build` (build and zone menus, placement ghost, zone overlay),
  `building` (building sheet), `orders` (work orders), `labor` (labor grid), `panels` (unit
  sheet, tile info, citizens), `psyche` (Thoughts/Needs/Personality/Health/Relations tabs and
  DF's wording), `military` (squads screen), `burrows` (burrow menu and overlay), `reports`
  (announcement history), `alerts` (hostile markers and the arrival pop-up), `devices` (doors,
  levers, bridges, traps, link tool and map highlight), `trade` (trade screen, move goods to
  depot), `nobles` (nobles screen), `round4` (caravan arrival box, population lines, settings
  page), `round5` (constructions, smooth/traffic/item tools and their markers, planned
  constructions, caged creatures, mood tab, treatment) and `overlay` (announcements, tooltips,
  Esc menu with save/load/settings).
- `load.rs` also fills squads, burrows, creature/weapon/armor raws and (round 4) entities,
  nobles, settings and historical figure names from the export's side files when the importer
  left them empty.
