# rf-app: the `rustfort` game window

A macroquad window that plays like the Steam version of Dwarf Fortress: mouse-driven UI, the DF
font, keybindings read from your own DF `interface.txt`, z-levels with the levels below
visible through open space, and a fast sim loop.

```
cargo run --release -p rf-app -- play                       # your export if there is one, else a synthetic fort
cargo run --release -p rf-app -- play --export export/tileshtulon
cargo run --release -p rf-app -- play --synthetic --size 192,192,153
cargo run --release -p rf-app -- bench --ticks 100000       # headless ticks/s + phase times
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
| Citizens list | toolbar (first button) | `u` |

To place designations and stockpiles, either press, drag and release, or click once and click
the second corner. Changing the z-level between the two corners makes a 3D box. While you place
it, the box size (`5x3x2`) follows the cursor.

The top bar shows pause/speed, the date (`15th Granite, 250`), the season, the **ticks/s
actually reached** (green = on target), FPS, the sim milliseconds per frame, population, and the
z-level (plus the elevation for imported forts). The bar at the right edge shows sky, surface
and underground; click it to jump. Announcements appear at the bottom left; click one that has a
position to jump there.

## Speed model

The sim runs in the main loop, inside a per-frame budget of at most 12 ms (and at most 75 % of
a frame at `--gfps`), so rendering stays at 60 fps even at "max". At 1x/2x/5x a tick accumulator
targets 100/200/500 ticks/s. The batch size of `Sim::run_ticks` adapts so that each call takes
about 1 ms.

Vsync is on in play mode and off for screenshots. `RUSTFORT_VSYNC=0` turns it off, and the app
then paces itself to `--gfps N` (default 60).

## Screenshot tests

```
rustfort play --screenshot target/shots/a.png --frames 20 --view 48,48,22,1 \
  --ui unit,citizens --script "3:key m; 5:move 600,300; 6:ldown; 8:wheel 1; 10:move 1000,560; 12:lup"
```

- `--view x,y,z,zoom` sets the camera.
- `--ui` opens states: `citizens`, `unit[:id]`, `skills`, `labor`, `items`, `dig`, `chop`,
  `stockpile`, `menu`, `tile`, `drag`.
- `--mouse x,y` parks the cursor.
- `--script` replays input per frame: `move`, `click`, `ldown`, `lup`, `rclick`, `mdown`,
  `mup`, `wheel n`, `key k`, `char c`, `mods shift|ctrl|none`.

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
- `ui/`: `topbar`, `toolbar` (data-driven button tables plus Dig/Stockpile submenus), `panels`
  (unit sheet, tile info, citizens) and `overlay` (announcements, tooltips, Esc menu).
