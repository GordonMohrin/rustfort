# rf-app: the `rustfort` game window

A macroquad window that plays like the Steam version of Dwarf Fortress: mouse-driven UI, the DF
font, keybindings read from your own DF `interface.txt`, z-levels with the levels below
visible through open space, and a fast sim loop.

```
cargo run --release -p rf-app -- play                       # your export if there is one, else a synthetic fort
cargo run --release -p rf-app -- play --export export/tileshtulon
cargo run --release -p rf-app -- play --synthetic --size 192,192,153
cargo run --release -p rf-app -- bench --ticks 100000       # headless ticks/s + phase times
cargo run --release -p rf-app -- play --load saves/Trussroads-20261009-222118.rfsave
cargo run --release -p rf-app -- savebench                  # save + load time and size
```

The world comes from `rf_import::find_export` (`--export DIR`, env `RUSTFORT_EXPORT`, or
`export/tileshtulon`). If there is none, or the import fails, you get `World::synthetic(96,96,40)`
(the reason is shown as the first announcement). The window title names the loaded world.

The DF install is `rf_gfx::df_dir()` (env `RUSTFORT_DF_DIR`). From it the app reads, at runtime only:
- `data/art/curses_640x300.png`, the 8x12 CP437 font of the Steam UI;
- keybindings, from `%APPDATA%\Bay 12 Games\Dwarf Fortress\prefs\interface.txt`, then
  `<install>\prefs\interface.txt`, then `data\init\interface.txt`;
- Premium sprites and UI icons through rf-gfx. Wherever rf-gfx returns nothing, the app draws
  fallback tiles from material colors and glyphs.

## Controls

The defaults below are Steam DF's. Your own `interface.txt` overrides them.

| Action | Mouse | Keys |
|---|---|---|
| Pan | middle-drag | WASD or arrows (Shift = fast) |
| z-level up/down | wheel (direction as in your DF prefs; wheel down = up by default) | `q`/`e`, `<`/`>`, PageUp/PageDown; Shift+wheel = 10 levels |
| Zoom | Ctrl+wheel (anchored at the cursor) | `[` `]` |
| Pause / one tick | pause button | Space / `.` |
| Speed 1x / 2x / 5x / max | top bar buttons | `1` `2` `3` `4` (1x = 100 ticks/s, DF's FPS cap) |
| Select unit / tile | left click (click again to cycle through units on a tile) | |
| Back out | right click: pending box, then tool, then selection | Esc, which finally opens the menu |
| Dig menu | toolbar pickaxe | `m`, then `m` mine, `t` up/down stair, `r` ramp, `u` channel, `x` remove |
| Chop / gather / erase | toolbar | `l` / `g` / `x` |
| Stockpile | toolbar crate, then pick a preset (Stone, Wood, Food, Furniture, All) | `p` |
| Creatures list | toolbar (first button): Citizens (sort by Name / Profession / Stress), Pets/livestock, Others (visitors, wild animals, hostiles) | `u` |
| Unit sheet tabs | Overview, Thoughts, Needs, Personality, Health, Relations, Skills, Labor, Items (wheel scrolls) | |
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
| Labor | toolbar (left group) | `y` |
| Save | Esc menu "Save game" | F5 (quicksave) |
| Load | Esc menu "Load game" (list of saves, newest first) | |

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
- **Zones** (`z`): DF only shows zones in zone mode, and so does Rustfort: the translucent
  `ACTIVITY_ZONES` borders plus the zone-type icon appear while the zone menu is open or a zone
  is selected.
- **Building sheet** (click a building): name, material, built state and "Remove". Workshops show
  their task queue (repeat `R`, suspend `S`, cancel `x`) and "+ Add job" with the jobs from
  `Sim::templates_for_workshop` (tick "Repeat" first for repeating jobs). Farm plots have one
  crop per season (`Sim::crop_options`, crops not plantable in that season are greyed out).
  Stockpiles show their categories as checkboxes.
- **Work orders** (`o`): every manager order with progress (left/total), frequency, conditions
  (e.g. `boulder (non economic, hard) ≥ 22; slab < 2`), workshop and status (√ active,
  · waiting). "+ New order" opens a window: pick a template, set the amount with -/+ (Shift =
  10) or the wheel, pick the frequency, "Create order". `x` removes an order.
- **Labor** (`y`): citizens x labors with DF's `unit_labor` names in tabs (Core, Farming & food,
  Wood & stone, Crafts & metal, Health & other). Cells show the related skill level; click a
  cell to toggle (`Sim::set_labor`), click a column header to toggle it for everyone shown, type
  in the filter box to filter by name or profession, wheel scrolls. The footer counts how many
  have each labor (red = nobody).
- **Unit sheet** (click a creature): two rows of tabs like the Steam sheet.
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
- **Burrows** (`Shift+u`): the burrow list with tile and unit counts; burrow tiles of the
  current z-level are drawn as a colored overlay (the selected burrow with its outline), like
  DF only while the menu is open. "+ New burrow" and drag a box (3D like digging) creates one
  (`Sim::create_burrow`). "Alert" turns the civilian alert on for that burrow
  (`Sim::set_civilian_alert`); the top bar then says "Civilian alert: <burrow>".
- **Reports** (`Shift+n`): every announcement of the session with its game date, newest first,
  filtered by All / Combat / Deaths / Jobs / Info, plus the combat log (`Sim::combat_log`).
  Click a line with `»` to jump there.
- **Hostiles**: hostile creatures (`Sim::hostiles`) get a pulsing red frame with a `!` badge
  (not when caged); creatures with open wounds get a blood drop. The top bar counts the
  hostiles; click the counter to center on the next one. When new hostiles arrive, Rustfort
  pauses and shows a pop-up like DF ("A vile force of darkness has arrived!" for a goblin
  siege, "An ambush! Curse them!", "<Name> has come!" for megabeasts) with "Center on them".
  Hostiles arriving within a day of the first belong to the same wave. Wild animals only get an
  announcement. A siege already in progress when the fort is loaded only gets an announcement.
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
  d_init `POPULATION_CAP`) and the invasions switch (`Sim::set_invasions`).
- **Liquids**: water and magma use Premium's `LIQUIDS` page, one sprite per depth 1-7 (1-3
  translucent, 4-7 opaque and darker) with the depth number, and `UNDERWATER_EDGE_*` foam on the
  dry tiles around a pool. Tiles are redrawn every frame, so flowing water moves as the sim
  changes it. The tooltip says e.g. "Water depth 4/7 (wading depth)" (plus salty, stagnant).
- **Stocks** in the top bar: drink units, food units and distinct foods, recounted every second
  (red below 2 per citizen). Food = prepared meals, meat, fish, cheese, eggs and plants that
  are edible raw or cooked according to the save's plant raws; rotten and dumped items do not
  count.

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
- Screenshot runs print the average ticks/s and, with the sim thread, how busy the worker was;
  `RUSTFORT_MEASURE=a,b` prints the real time from sim tick a to b.
- `--mouse x,y` parks the cursor.
- `--script` replays input per frame: `move`, `click`, `ldown`, `lup`, `rclick`, `mdown`,
  `mup`, `wheel n`, `key k` (also `f5`, `backspace`, `tab`), `char c`, `mods shift|ctrl|none`.
  In screenshot runs a save started with F5 finishes within the frame.

With a script, the real mouse and keyboard are ignored, so runs are reproducible. Keep
screenshots under `target/`, never in git.

## Code map

- `main.rs`: CLI.
- `game.rs`: state, sim loop, camera, map input.
- `render.rs`: map, fallback tiles, overlays.
- `keys.rs`: DF `interface.txt` bindings and key repeat.
- `font.rs`: DF CP437 bitmap font.
- `script.rs`: scripted input.
- `bench.rs`: headless benchmark.
- `simthread.rs`: the sim worker thread (`SimHandle` derefs to `Sim` while the UI holds it).
- `save.rs`: save games (bincode + zstd), save list, `savebench`.
- `ui/`: `topbar` (plus stocks, hostile counter), `toolbar` (data-driven button tables plus
  Dig/Stockpile submenus), `build` (build and zone menus, placement ghost, zone overlay),
  `building` (building sheet), `orders` (work orders), `labor` (labor grid), `panels` (unit
  sheet, tile info, citizens), `psyche` (Thoughts/Needs/Personality/Health/Relations tabs and
  DF's wording), `military` (squads screen), `burrows` (burrow menu and overlay), `reports`
  (announcement history), `alerts` (hostile markers and the arrival pop-up), `devices` (doors,
  levers, bridges, traps, link tool and map highlight), `trade` (trade screen, move goods to
  depot), `nobles` (nobles screen), `round4` (caravan arrival box, population lines, settings
  page) and `overlay` (announcements, tooltips, Esc menu with save/load/settings).
- `load.rs` also fills squads, burrows, creature/weapon/armor raws and (round 4) entities,
  nobles, settings and historical figure names from the export's side files when the importer
  left them empty.
