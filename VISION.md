# Rustfort: vision

Rustfort is an independent re-implementation of Dwarf Fortress fortress mode in Rust. It aims
to stay fast with hundreds of dwarves, to behave like the original, and to continue a fort
that was started in real Dwarf Fortress.

This document describes what the project is for and how it is meant to work. For what exists
today, see [PROGRESS.md](PROGRESS.md). For how close each rule is to DF, see
[docs/FIDELITY.md](docs/FIDELITY.md).

## 1. The problem

Dwarf Fortress slows down sharply as a fort grows. Past about 100 dwarves, many forts drop far
below the 100 ticks per second of normal speed, and a game year takes hours of real time.

The cost comes from **units in total**, not from dwarves alone. One fort measured during
planning had 5 citizens but 229 active units. 98 of them were demons in the sealed underworld
and 24 were animals in the caverns, and all of them were simulated every tick. With 12,889
items, DF reached about 220 ticks per second. Its main thread was at its limit, while the
process used only about 1.4 CPU cores.

## 2. Goals

| goal | how it is measured | status (v0.12) |
|---|---|---|
| **Fast** | An imported fort with 200 dwarves and 500 other units runs headless at **1,000 ticks/s or more**, and at 500 or more with graphics. | Reached by far: the 87-citizen reference fort runs at about 100,000 ticks/s headless, and 487 citizens at about 38,000. |
| **Faithful** | The raws of the user's installation load 1:1 (vanilla: 981 creatures, 265 inorganics, 225 plants, 163 reactions). The time base, need timers and gaits are exact. The rest behaves like DF as described by the wiki and as observed. | Rules are checked one by one in [docs/FIDELITY.md](docs/FIDELITY.md) and against a real fort's history in [docs/VALIDATION.md](docs/VALIDATION.md). |
| **Continue your own fort** | A fort from a running DF is imported and plays on in Rustfort. An import report shows what was taken over, regenerated or dropped. | Import from the title screen in about a minute, read-only. |

## 3. Why DF slows down, and Rustfort's answer

| cause (wiki and profiling) | share | Rustfort's answer | expected gain |
|---|---|---|---|
| Unit turns | over 60 % of the time | A unit only thinks when it has something to do (a "think wheel"). Expensive checks are staggered over ticks, and the decision phase runs in parallel. Unreachable or sleeping creatures (underworld, caverns) are set dormant. | 5-20x |
| Line of sight between units | O(n²) within 26 tiles, "slowest part by a wide margin" | Spatial hashing, recomputed only on movement, in parallel | 10-50x in crowds |
| Temperature and items | turning temperature off roughly doubles DF's FPS | Temperature only where there are active heat sources. Items are indexed by category, material and place instead of long lists. | about 10x |
| Pathfinding | under 10 % of unit time, except for trapped units | Connected components answer "reachable?" in O(1). Landmark heuristics and shared flow fields cover common goals. | 5-30x |
| Fluids | named in v50 profiling | Settled liquid costs nothing; only active cells are computed. | 3-10x |
| Job search | every dwarf searches everything | Central matching with spatial indexes | 3-10x |
| **One core** | the main thread is the limit | Use all cores and stay deterministic | multiplier |

The per-cause factors were estimates when the project started, so measuring was planned from
the first milestone on. The measured results are in [PROGRESS.md](PROGRESS.md).

## 4. Principles

1. **Clean-room.** The specification is the Dwarf Fortress Wiki plus behavior observed in the
   running game, read-only through DFHack. DF is never decompiled or disassembled.
2. **DF data only at runtime.** Raws, graphics and fonts are read from the user's own DF
   installation when Rustfort runs. This is the way OpenMW uses the Morrowind data files. None of
   them is ever copied into the repository, and neither are exports or saves of real forts.
3. **A DF-compatible data model**, so that a real fort can be imported: raw ids, tiletypes,
   material indices, unit fields (attributes, skills, personality, needs, wounds) and
   historical figure ids follow DF. The behavior may be implemented differently and faster, but
   the data must match.
4. **Deterministic.** The same seed and the same commands give the same result, including with
   parallel execution. That makes replays, hash checks and reproducible bug reports possible. The
   state hash must not depend on the number of threads.
5. **Headless first.** The simulation is a library without a screen. The game window, the
   benchmark and the test tools (validation, crash hunting, save/load checks) are all clients
   of the same `Sim` API. The original plan also had a localhost-only HTTP API for scripted
   players. That is not built yet.
6. **A read-only exporter.** The DFHack exporter only reads. It never pauses DF, never writes
   to its memory and never installs hooks, so it can run while the player keeps playing.
7. **Measure first, then judge.** A benchmark harness (ticks/s against population) existed from
   the first milestone on, and a real fort is the reference benchmark.

## 5. Fidelity tiers

| tier | meaning | contents |
|---|---|---|
| **A: exact** | like DF, number for number | time base (1,200 ticks per day, 403,200 per year); hunger, thirst and sleep; gaits (900 = 9 ticks per tile); raws (materials, creatures, bodies, items); designations and tile semantics; labors, skills and experience; stockpiles and hauling; workshop outputs; quality levels; liquid depths 0-7 |
| **B: same behavior** | the statistics match, not every tick | thoughts, stress and needs; combat through tissue layers; migrant and caravan rhythm; trade values; strange moods and artifacts; sieges; forgotten beasts; temperature; liquid flow |
| **C: simplified** | plays similarly | noble politics, justice, religion, performances, weather, wildlife migration, hospital details |
| **D: not planned** | | adventure mode, full world history and Legends, necromancy chains, a modding interface |

An important finding: almost all workshops and furnaces (carpenter, mason, smelter, still,
kitchen, ...) are **hard-coded in DF and are not in the raws**. The raws define only two
workshops. Rustfort rebuilds these workshops and their jobs from the wiki.

## 6. Importing a fort

**The route:** a DFHack script reads the running DF and writes an export folder
([docs/EXPORT_FORMAT.md](docs/EXPORT_FORMAT.md)), and `rf-import` loads that folder into
Rustfort. Reading DF's `.sav` files directly is not realistic. They are an undocumented dump of
DF's internal structures, compressed in thousands of zlib chunks (one save: 22 MB packed,
233 MB unpacked, 11,672 chunks), and the layout changes with every DF version.

**How an export runs** ([tools/dfhack/README.md](tools/dfhack/README.md)):

- The work runs in short frame slices (25 ms of Lua per frame by default), so DF keeps running
  and never freezes. An early whole-map scan in one go had frozen DF for 9-28 s.
- Units, items and buildings are snapshotted by id and found again in each slice, because DF's
  lists change between frames.
- `manifest.json` (versions, date, counts, notes) is written last and marks the export as
  complete. The rest is JSON and JSON-lines files plus one binary file per z-level.
- The raws come **from the save itself**, not just from `data/vanilla`. A save contains
  generated materials (divine metals, forgotten beast materials, syndromes), and material
  indices are taken over exactly as DF has them.

| category | contents |
|---|---|
| **Taken over** | all DF ids and next-id counters; the map with materials, veins, liquids, constructions, engravings and revealed state; units with body, wounds, skills, attributes, personality, needs, stress, memories and relationships; items with containers, decorations and artifacts; buildings, rooms and ownership; zones; stockpile settings; burrows; nobles, squads and uniforms; the calendar; the world in a slim form (civilizations, historical figures the fort refers to) |
| **Regenerated** | jobs in progress, paths, reachability data, caches, short-term emotions |
| **Not imported** | deep world history, announcements, combat logs, view state (camera, open screens) |

**Checking an import:**

1. Counts per kind are compared with the exporter's manifest when loading.
2. Spot checks against the live game (`rustfort-verify.lua`) compare sampled tiles, citizens,
   orders, buildings and more, field by field.
3. Every reference must resolve. What cannot be resolved is repaired and listed as a warning
   in the import report.

**One direction only:** DF to Rustfort. Writing a fort back into a DF save is not realistic,
since a single mistake would silently damage the save. The DF save stays as it was, and the
export can be repeated at any time: play on in DF, then import the newest state.

## 7. Architecture

```
 DF + DFHack ──rustfort-export.lua (read-only)──▶ export/<fort>/ ──▶ rf-import ─┐
 DF install: data/vanilla, Premium graphics ──▶ rf-raws, rf-gfx                │
                                                                              ▼
                                        rf-core World (shared data model)
                                                                              │
                 rf-sim: deterministic tick (parallel read phases, ordered writes)
                                                                              │
          rf-app: game window (sim on its own thread), headless bench, saves ◀┘
```

- Tiles, items and jobs live in their own compact stores and indexes.
- Random numbers are derived from (seed, slot, tick), so the order of threads does not matter
  and there is no generator state to save.
- Drawing never blocks the simulation: the window hands the sim to a worker thread between
  frames.

The crate responsibilities and working rules are in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## 8. Milestones

Each milestone has a checkpoint that shows it is done.

| # | contents | checkpoint |
|---|---|---|
| **M0** foundation | workspace, raw parser (templates, creature variations, COPY_TAGS_FROM, castes), benchmark harness, stress test with 200/400/800 units on a 4x4 embark | the raw statistics match the install (981/265/225/163); the stress test shows the ticks/s curve |
| **M1** the map | DFHack exporter (map, buildings, stockpiles) and a viewer | you scroll through your own fort in Rustfort, and the tile counts per material match DF |
| **M2** life | import of units; movement, pathfinding, line of sight; eating, drinking, sleeping | the fort runs one game year headless at 1,000+ ticks/s without mass deaths; the need curves match real DF |
| **M3** work | designations, labors, hauling, stockpiles, the core workshops, farming, brewing, cooking, work orders | **playable from here:** dig, build and produce; the fort sustains itself |
| **M4** society | thoughts, stress, personality, migrants, nobles and mandates, burial and ghosts, caravans and trade | mood and stress develop plausibly; the first caravan arrives |
| **M5** combat | body and tissue combat from the raws, wounds, medicine, squads, sieges, forgotten beasts | a goblin siege with 50 attackers runs at full speed |
| **M6** physics | fluids, temperature, magma, caverns, aquifers | stress tests: a flooded cavern and a magma forge |
| **M7** polish | strange moods and artifacts, the Premium tileset from the user's install, UI parity | it feels like DF |

**The measure of progress across all milestones is import coverage:** the share of the units,
items, buildings and jobs of a real fort that Rustfort actually simulates. It is one number,
and it rises with each milestone.

By v0.12 the milestones M0-M5 and most of M7 are done. Of M6, fluids, magma, caverns and
aquifers are in, and temperature is not. See [PROGRESS.md](PROGRESS.md).

Expectations from the start:

- 100 % parity will never be reached, since DF contains two decades of development.
- A fort becomes playable with gaps from M3 on, and should feel like DF from M5 on.

## 9. Risks

1. **Breadth is harder than speed.** The biggest risks are needs, moods, combat and materials.
2. **Import gaps.** What Rustfort does not know yet is lost on import. The import report makes
   that visible.
3. **DF updates change the data structures.** The exporter was developed against DF 53.16 and
   DFHack 53.16-r2, and other versions may need fixes.
4. **Determinism with parallelism** needs discipline from day one: state hash checks with one
   and many threads, and exact save/load continuation tests.
5. **Staggered execution shifts timings slightly** compared with DF. This is documented and
   bounded by comparing against real DF.
6. **Legal:** Rustfort is an unofficial fan project. The repository contains no DF files, and
   the project uses its own name.
