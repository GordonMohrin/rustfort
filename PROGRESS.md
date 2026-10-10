# Rustfort: progress

## v0.11.0 (2026-10-10): calibration against Gordon's history, exact save continuation, faster sieges

- **Stress:**
  - "Hardened" dwarves feel nothing about corpses and witnessed deaths; DF's exports show exactly that (ANYTHING reactions), and the hardening grows with every horror.
  - The strongest grief memory keeps its own slot and is relived.
  - A calm year (r3 -> r4) now ends with 11 stressed dwarves (6-13 over seeds; DF 6), not 29.
  - A siege year comes out at mean stress -22k (DF -26k).
- **Masterpieces:** "X has created a masterpiece ...!" announcements (log only) and MakeMasterwork thoughts.
- **Migrants follow DF's rules:** none in winter, the wave holds until a siege ends, danger after many deaths turns migrants away, and waves shrink near the cap.
  - The r5 -> r6 fall now brings 5 migrants (DF 1), down from 26.
  - Consequence: a fort that loses many dwarves is no longer refilled.
- **Animal births** are counted and happen (r4 -> r5: 10 vs DF 12). Creatures sharing a cage breed.
- **Food under alert:** work orders and production inside the alert burrow keep running. r4 -> r5 drinks and meals are now within DF's range.
- **Exact continuation:** save, load and continue gives a **bit-identical** state hash with uninterrupted play (checked for N = M = 100, 20k and 403k ticks on several snapshots). The sim snapshot is stored compactly in the save, which is now about 3.4 MB.
- **Siege speed:**
  - shared flow fields per goal for charging raiders (A* nodes 13x fewer)
  - cheaper wound bookkeeping
  - a 133-goblin siege costs 198 instead of 615 Mcyc per 1k ticks, about 3x faster
  - validate r5 -> r6: median 64 citizens lost (DF 73), now "close"
- **Result:** 2 years headless with `--standdown` (simple player emulation) on r5: 87 -> 47-52 citizens through several DF-sized sieges without migrant refills. About 103-106k ticks/s, the same hash with 1 and N threads.
- **Known gaps:**
  - craft quality is still low (1.6-1.8 vs DF 2.6)
  - invaders do not climb yet (v50 climbing)
  - combat log lines are still formatted per blow
  - 487 citizens about 22k ticks/s (target 40k)
- **Build note:** `cargo build --workspace --examples` does NOT rebuild `rustfort.exe`; use `--bins` (or `--all-targets`).

## v0.10.0 (2026-10-10): update-proof saves, crash hunting, Gordon's playtest feedback, Maeander engravings

**Gordon's v0.9 feedback, implemented:**
- Right-click closes the topmost thing, like Steam DF: popup -> screen -> side panel -> submenu -> drag box/tool -> selection. On the bare map it opens a context menu.
- **The military screen is a side panel** (the map stays usable; clicking a member opens their sheet beside it). It has:
  - a Defense block with a burrow picker and an "Everyone into the burrow!" button, plus a banner with "Lift alert"
  - checkboxes to give one order or routine to several squads
  - per-member sprite previews with uniform slots (green = worn, orange = clothing only, red X = missing)
- **The hostile counter, markers and alerts count only hostiles you can see.** The sealed underworld demons no longer count.
- **Portraits** in the unit sheet, made from DF Premium's PORTRAIT layer sets with DF's frame.
- **Engravings in Gordon's chosen style C (Maeander)**:
  - Greek-key bands along each engraved region, seamless across tiles, with corner rosettes and lozenges inside
  - wall friezes; silver inlay at exceptional quality, gold at masterwork
  - Rustfort's own runtime art, no longer DF's dwarf-face picture
- Automated screenshot and test windows open on **virtual desktop 1** (`--desktop N` or `RUSTFORT_DESKTOP`); normal play stays on the active desktop.

**New since 0.9:**
- **Saves (format 2):** named-field MessagePack plus zstd, a CRC header, write-then-verify, and a `.bak` per save, so they tolerate added and removed fields.
  - v0.9 bincode saves are converted on load.
  - Module runtime state is saved in `World.sim_state`: petitions and visitors, raids and hidden raiders, moods, breakdowns, melt fractions, mud.
- **Crash hunting:** a chaos harness fires random API actions for years with invariant checks. It found and fixed 6 bugs at the root, among them 2 stack overflows under civilian alert, a station-on-a-wall freeze, dangling container contents and stale building item ids. 40 runs then finished with 0 panics.
  - Crash dialog and rescue saves; `saves/crash.log`.
- **Performance:**
  - siege pathing re-plans less, so the first siege phase uses 5x fewer A* nodes
  - wounds are mirrored once per blow (add_wound went from 190k to 335 cycles)
  - branch-free ALT bound
  - 2 years on r5 run at about 80-106k ticks/s
- **Gaps closed:**
  - uniform materials (exact and partial matches, best metal first)
  - mounted invaders that ride and dismount
  - thieves and snatchers calibrated to Gordon's history (0 at Trussroads)
  - pumps and power: screw pumps, water wheels, windmills, axles and gears
- **UI:** Tasks screen (suspend, do now, cancel), Hauling screen (read-only), DF hotkey chains for the build menu, look lines for engraved tiles, announcements on the title-screen settings page.
- **Result:** 2 years headless with `--standdown` on r5: 87 -> 79 citizens through several large sieges, about 106k ticks/s, the same hash with 1 and N threads.
## v0.9.0 (2026-10-10): calibrated against Gordon's real history

**Gordon's real DF fort has fallen.** At r7 (year 280) DF reports game_over, lost to goblin civ 39. Its five sieges brought 34, 58, 143, 101 and 148 units. In Rustfort the fort lives on from r5 (year 279, 87 citizens).

**New since 0.8:**
- **War calibration** from DF's own data (`plotinfo.invasions`, entity `siege_tier`, the save's difficulty settings):
  - the invasion tier is chosen by population, created wealth or exported wealth; your fort is at the Metropolis tier
  - siege strength grows per consecutive siege up to the unit cap 140 (the 5th siege brings 133 creatures, DF had 136 + 7 mounts)
  - master troops and iron armor at the observed rates, cave-crawler mounts, trolls, hidden ambush squads with sieges
  - building destroyers wreck workshops, doors and furniture
  - **forgotten beasts follow DF's irritation rules** (0 in 3.5 years, as in DF)
  - fixed: stuck leaving invaders; fleeing civilians can drink and eat again
  - validation: r5 -> r6 now median 26 citizens lost (DF 73, before 1); r4 -> r5 median 7 (DF 14)
- **Skills calibration:**
  - XP per job per the wiki, no level cap
  - military XP 15.8k per dwarf-year (DF 14.5k) with situational awareness, discipline and fighting at DF's shares
  - **social and mental XP** 1.9k (DF 2.0k)
  - item quality from skill (crafts mean 1.8, DF 2.6), meals included; births 3 (DF 2)
  - grief replays strong memories (8 stressed citizens, 12 with DF's real deaths replayed; DF 17)
  - **tree growth** (+70 trees, DF +65)
- **Performance:**
  - landmark (ALT) A* heuristic with exact path costs: A* nodes on r5 dropped 8-10x
  - **87 citizens about 70k ticks/s, 487 citizens about 33k ticks/s** (v0.8: 19k / 11k)
  - the root cause was construction fetches across about 10 z-levels; a determinism stress tool was added
- **Exporter r7:**
  - the off-map tame animals are departed caravan animals, now marked inactive
  - squad ammo, per-position uniforms and patrol waypoints
  - DF wealth counters and the invasion history
  - stockpile_options.json (29,735 leaves) feeds the stockpile editor
- **UI:**
  - DF announcement settings (box/pause/center/log per event, defaults from your announcements.txt) with a box queue
  - petitions badge and screen; raid alerts
  - squad Equipment/Ammunition/Patrol tabs (uniform editor, ammo, patrol routes drawn on the map, barracks)
  - combat reports grouped per fight; DF skill names with XP bars
- **Fix:** farm work could index a rebuilt farm list (crash) once destroyers wrecked buildings.
- **Result:** 2 years headless with `--standdown` (prompt stand-down and re-alert) on r5, seeds 1-3: 86-90 of 87 citizens alive, with 168-256 deaths that are mostly invaders, at about 64-83k ticks/s. With slower reactions (the fort example checks monthly) the fort falls, as Gordon's real one did.

**Known gaps:**
- Uniform materials are not filtered (type only).
- Petitions and raid state are not saved; invaders do not dig.
- Mounts do not carry riders; standalone thieves and snatchers are more frequent than in Gordon's history.

## v0.8.0 (2026-10-10): animals, locations/justice/lifestyle, raids, new screens, validation against real DF

**New since 0.7:**
- **Exporter round 6** (`export/tileshtulon-r6`, year 280). It adds:
  - work details (12), full stockpile filters and links, kitchen exclusions (269)
  - locations (23 temples, taverns, libraries...) with occupations, justice cases (328)
  - animal owner, training, pasture, slaughter and pregnancy; hauling routes
  - DF room evaluation thoughts. Finding: **room-thought severity is the wiki tier**, so 54 of 97 rooms match exactly and 90 of 97 are within one tier.

  **Note:** Gordon's real fort was overrun by a siege in year 280 (90 -> 15 citizens), so r6 is a dying fort. The default `export/tileshtulon` stays round 5 (year 279, 87 citizens).
- **Animals:**
  - grazers and pastures; breeding (6-month gestation, litters); egg-laying in nest boxes with hatching
  - milking, shearing (your ShearCreature order now runs), cheese and thread; slaughter at the butcher
  - war, hunting and taming training; hunting with crossbows, fishing; cats killing vermin; pet adoption
- **Locations and lifestyle:**
  - temples (deity match), taverns (keeper serves, performers), libraries (scholars)
  - visitors and petitions (`Sim::answer_petition`)
  - justice: crimes from tantrums, thieves and mandates; sheriff or captain investigates; jail, beating and hammerstrikes
  - clothing wear and replacement, meal quality from cook skill, kitchen exclusions respected, miasma
- **Raids and military:**
  - ambushes (3 of 4 invasions, hidden until spotted), kobold thieves, goblin snatchers
  - siege camps, trolls and beak dogs from the City tier, retreat when the leader dies
  - recover wounded (carried to bed; broken legs do not walk), barracks, sparring and demonstrations, patrols
  - per-position uniforms and squad ammo API; civilian alert prefers burrow resources
- **UI:**
  - v50 **Work details** screen (the old labor grid moved to "Advanced"), **stockpile editor** in DF Steam style (categories, groups, leaves, presets, containers, links), Kitchen screen
  - Animals screen, Places (locations, zones, piles, workshops, farms), Justice, Objects (293 artifacts), World (civs, caravans)
  - **minimap**; unspotted raiders stay hidden
- **Validation against Gordon's real history** (`examples/validate.rs`, `docs/VALIDATION.md`): Rustfort runs from a real snapshot to the date of the next one, for example r4 -> r5 (2.18 years), with several seeds.
  - **Matches DF:** caravans 4/4, moods and artifacts 5/5, siege count, item wealth within 5 %, stocks.
  - **Diverges:** sieges far too weak (DF sent at least 137 goblins and killed 14, later 73), forgotten beasts too frequent, social XP missing, military XP 0.5x, craft quality, pregnancy rate.
  - These calibration fixes are the plan for round 9.
  - DF's real speed on this fort was about **107 calendar ticks/s**; Rustfort is about 500x faster.
- **Result:** 2 years headless with `--standdown` on r5: 87 -> 92 citizens, about 55k ticks/s, the same state hash with 1 and N threads.

**Known gaps (next, round 9):**
- The calibrations above.
- Intermittent nondeterminism under heavy load reported by two agents.
- r5 has slower high-population scaling (hostiles' long A* searches).
- 80 tame animals are exported off-map (cages?).
- Invaders that are leaving can get stuck on traps; civilians fleeing a threat cannot drink.

## v0.7.0 (2026-10-10): DF engravings, civilian alert with work, cave-ins, wealth-based sieges

**New since 0.6:**
- **Exporter round 5:** all **16,246 engravings** of your fort are exported (position, quality, floor/wall, stone, engraver) and spot-checked against the live DF. The default export is now `export/tileshtulon` = round 5 (year 279, 87 citizens); older exports are kept as `-r1..-r4`.
  - With engravings counted, room values are 269-1,586 for bedrooms and 2,826 for the dining hall.
  - Open question: how DF's room-thought severity maps to tiers (see FIDELITY.md). The min_tier floor stays until then.
- **Civilian alert** as in v50: civilians in the alert burrow keep working on jobs inside it, and nothing outside.
- **Cave-ins:** unsupported chunks fall after a dig, channel or deconstruction; debris lands below; creatures underneath die and nearby ones are knocked out.
- **Siege size** by wealth tier (wiki), instead of 10 + population/8.
- **Result:** 2 years headless with `--standdown` on the new export: 87 -> 93 citizens, about 61k ticks/s.

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
