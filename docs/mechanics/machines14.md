# Machines, mechanisms and minecart devices: DF rules and the Rustfort design (`[machines14]`, v0.14)

This is the implementer's spec for round 14 topic **machines14**: power networks, levers,
pressure plates, mechanism links, linked devices (doors, hatches, floodgates, bridges, grates,
bars, supports, cages, chains, spikes, gear assemblies, track stops), the minecart-device
interactions, and the UI for linking, plate settings, the power overlay and the route view.

Sources (clean-room): the DF v50 wiki pages, fetched as wikitext on 2026-11-18 (`w:Page`), the
vanilla raws of the local install (read only), and the current Rustfort code. Every number
below is from the wiki unless it says **ESTIMATE**. "Rustfort today" names the file and function.

Classes: **OK** (already right), **WRONG** (Rustfort contradicts the wiki), **MISSING**,
**ESTIMATE** (wiki silent, our number).

Out of scope: undead, vampires, megabeasts, diplomacy, weather (wind and ice come
from `[weather13]` once it exists), the round-13 UI, minecart collisions and stop item filters
(`[perf13]`), and fire, magma melting and temperature (`[fire15]`). Where a rule here depends on
one of those, it gets a hook or a note and nothing more.

---

## 0. Summary of findings (ordered by impact)

| # | finding | where | class |
|---|---|---|---|
| F1 | **Linked doors and hatches are inverted.** DF: an "on" signal **opens** a linked door or hatch and "off" closes and seals it. Rustfort: "on" closes and locks. On imported forts the first pull of a lever linked to a closed door does nothing (the door's `on` is already true), and from then on the door is always the opposite of DF's. | `defense.rs` `mech_state`, `apply_signal`, `defense_scan` (`on` for Door/Hatch) | WRONG |
| F2 | **Upright spikes are inverted.** DF: "on" **retracts** them, "off" extends them, each after 40 ticks. Rustfort: "on" extends them, after 100 ticks. | `defense.rs` `apply_signal`, `spikes_extend` | WRONG |
| F3 | **One delay for every device.** Rustfort delays every signal by `LINK_DELAY` = 100. DF: doors, hatches, gear assemblies, cages, chains and supports react at once; spikes after 40; bridges, floodgates, grates and bars after 100; a track stop switches off at once and back on after 40. Doors and hatches currently lag 100 ticks, which is enough for invaders to run through. | `defense.rs` `send_signals` | WRONG |
| F4 | **No refractory period, and no same-tick rule.** Delayed devices ignore signals while a change is pending. A device that gets "on" and "off" in the same tick toggles. | `defense.rs` `apply_signal` | MISSING |
| F5 | **Pressure plates are edge-triggered.** Rustfort sends "off" 100 ticks after the last step onto the plate, even while a goblin is still standing on it. DF sends "on" as soon as the condition holds and "off" 99 ticks after it stops holding. Weight ranges, water, magma and minecart triggers, one-use plates, and "unconscious or webbed creatures always trigger" are all missing. | `defense.rs` `trap_entered`, `defense_tick` | WRONG / MISSING |
| F6 | **Gear assemblies cannot be disengaged.** DF: any signal toggles a gear. A disengaged gear transmits nothing, and machines hanging from it collapse. Gear assemblies also count as movement blockers in Rustfort, but DF says they "do not block unit movement". | `power.rs` (doc "always engaged"), `lib.rs` `blocks_movement`, `ranged.rs:203` | MISSING / WRONG |
| F7 | **Linking is instant and free.** DF: a Mechanic job uses 2 mechanisms (oldest first): one is carried to the target and installed while standing on it, then the other is carried to the trigger. | `defense.rs` `defense_link` | MISSING |
| F8 | **Grates, bars, supports, cages, chains and track stops cannot be linked** (the UI offers them, but the sim refuses with "cannot be linked"). | `defense.rs` `MechKind::linkable`, `ui/devices.rs` `linkable` | MISSING |
| F9 | **Machines and devices built from the UI never get built.** `production.rs` `new_cons` returns `None` for ScrewPump, WaterWheel, Windmill, GearAssembly, axles, Rollers and Trap/TrackStop. The round-12 demo forces `built = true`. Levers, pressure plates, bridges, floodgates, hatches, grates, bars, supports, cages and chains are not in the build menu at all. | `production.rs` `new_cons`, `build_api.rs` `OPTIONS` | MISSING |
| F10 | **Track stops are never seen by the cart physics.** `minecarts.rs` `carts_bld_rebuild` looks for `kind == "TrackStop"`, but the exporter and the build menu both produce `kind "Trap"`, `subtype "TrackStop"`. So real and player-built track stops neither brake nor dump. Only the unit tests (which use kind `TrackStop`) see them. The exporter's `friction` value is also dropped (no rf-core field). | `minecarts.rs:775,786`, `build_api.rs:452`, exporter `building_state` | WRONG (bug) |
| F11 | **Port rules disagree with the wiki.** Screw pumps must also connect **below** (pump stacks; the pump above has no port on the pump below, so the mutual-port test fails). Windmills must also connect to the **12 tiles around** their 3x3. A water wheel's power port is the tile beside its **center**, across the axis (the ends are ESTIMATE). Rollers longer than 1 tile connect only **across** their direction. Vertical links need open space between the levels (no floor). | `power.rs` `power_rebuild` | WRONG |
| F12 | **Water wheels need 4/7 water**, and the water must be "flowing" in DF's sense (it flows from deeper to shallower water, or the tile carries the flag for draining off the map edge or into an aquifer). Rustfort accepts any depth of at least 1/7. | `power.rs` `power_balance` | WRONG |
| F13 | **Screw pump details:** a pump keeps running 49 ticks after it loses power; pumps run newest first; pumping into the other liquid makes obsidian (Rustfort refuses); pumped water comes out clean (salt and stagnant flags cleared); the intake column must be open (no floor, stairs or wall above the source); the output tile is impassable and the operator stands on the input tile. | `power.rs` `power_tick`, `pump_step`, `pump_goals` | MISSING / WRONG |
| F14 | **Bridges:** a creature of size >= 1 200 000 on the bridge stops it from operating, and lowering onto such a creature deconstructs the bridge. Fluids on the area are destroyed. Items are flung or dropped too. Lowered bridges act as track crossings for carts; raised bridges are walls for carts. | `defense.rs` `fling_from`, `crush`; `minecarts.rs` `cart_next` | MISSING |
| F15 | **Floodgates and doors jam:** an item on the tile stops them from closing. A floodgate closes by itself once the item is gone; a door needs a new signal. Closing destroys any liquid on the tile. | `defense.rs` | MISSING |
| F16 | **Minecarts ignore buildings.** Closed doors and floodgates and raised bridges do not stop carts; lowered bridges are not a straight track. Plates set to "track" never fire. Linked track stops cannot be switched off. | `minecarts.rs` `cart_next`, `cart_enter`, `friction` | MISSING |

The reference fort has levers, linked doors and bridges, and traps, but no machines
(FIDELITY `fluids.rs` row). F1 to F5 can change the 2-year bench hash if a plate fires during
the siege. Expect a hash change and document it. F6 and F9 to F13 change nothing on the
reference fort.

---

## 1. Signals: what levers and pressure plates send

Sources: w:Lever ("Activation of the devices", "On/Off vs Open/Close", "On/Off states"),
w:Pressure plate ("Operation", "Build order & delay"), w:Machine component ("On/off,
open/close").

### 1.1 The DF rules

1. **Levers have a state** (on/off). Pulling it flips the state and sends the **new state**
   to every linked building. It does not send "toggle". The sprites are `LEVER_PULLED` and
   `LEVER_SET` in `graphics_buildings`. On the wiki, "lever to the left" is on in Premium.
2. A **pressure plate** sends **"on" (open)** the moment its condition holds, and **"off"
   (close)** 99 ticks after the condition stops holding. If the condition comes back before
   the close has gone out, the close is cancelled and **no second "on" is sent**. A plate
   never repeats "on" while it is held down.
3. **Gear assemblies are the only exception:** every signal, on or off, **toggles** them.
4. A signal that asks for the state the device is already in **does nothing**.
5. **Refractory devices** (the ones with a delay) "will not respond to subsequent signals
   until the first has taken effect". Example from the wiki: pull a floodgate lever on and then
   off quickly, and the gate opens and then stays open, out of phase with the lever.
6. **Same tick:** "In the unusual case that a device receives both an on and an off trigger in
   exactly the same tick, the device will change states."
7. **Order within a tick:** "any levers will send their signals before any furniture receives
   those signals". With plates, build order adds one tick (devices built before the plate
   react one tick later). That one-tick difference is **ignored here** (ESTIMATE: every device
   behaves as if it were built after the plate, which matches levers).
8. A lever with nothing linked can still be pulled. The puller ends up standing at the lever
   ("Urist's lever").
9. **Links:** one lever can have any number of targets, and one target any number of
   levers/plates. Links cannot be removed except by deconstructing either end. The same pair
   cannot be linked twice. Forbidden buildings cannot be linked.

### 1.2 How each device reacts (delay = ticks from receiving the signal to the change)

| device (rf `kind`/`subtype`) | delay on | delay off | "on" | "off" | refractory | source |
|---|---|---|---|---|---|---|
| Door | 0 | 0 | opens | closes and seals (linked doors never open for walkers) | no | w:Lever, w:Door |
| Hatch | 0 | 0 | opens (things on it fall if there is open space or a ramp below; not over stairs) | closes (a floor) | no | w:Lever, w:Hatch cover |
| Floodgate | 100 | 100 | opens | closes; destroys liquid on its tile; **will not close while an item lies on it, and closes by itself once the item is gone** | yes | w:Lever, w:Floodgate |
| Bridge (raising or retracting) | 100 | 100 | raises (the edge row becomes a wall, everything on it is flung) or retracts (everything drops) | lowers or extends (atom-smashes creatures and items on the whole area) | yes | w:Lever, w:Bridge |
| GrateWall / GrateFloor | 100 | 100 | opens (the building "just disappears": no wall or floor; liquids pass in both states) | closes | yes | w:Lever, w:Grate |
| BarsVertical / BarsFloor | 100 | 100 | opens (as grates) | closes | yes | w:Lever, w:Bars |
| Weapon (upright spear/spike) | 40 | 40 | **retracts** | **extends** (attacks whoever is on the tile, as today) | yes (ESTIMATE: treated like the other delayed devices) | w:Lever, w:Pressure plate timing table (40 / 139 = 99 + 40) |
| GearAssembly | 0 | 0 | toggles engaged/disengaged | toggles | no | w:Lever, w:Gear assembly |
| Trap/TrackStop | 0 (switches off) | 40 (switches back on) | **disables** it (no friction, no dumping; carts on it act as on the track below) | enables it | ESTIMATE: no | w:Lever, w:Minecart "More on Track stop" ("switch On = track stop Off") |
| Cage (built) | 0 | 0 | one-shot: releases all occupants; the cage and its mechanism deconstruct (dropped next to it); the lever keeps its mechanism | nothing | no | w:Lever, w:Cage |
| Chain (restraint) | 0 | 0 | one-shot: releases the creature; the chain goes into its inventory; the mechanism is dropped | nothing | no | w:Lever, w:Restraint |
| Support | 0 | 0 | one-shot: deconstructs (material and mechanism dropped) and may cause a cave-in | nothing | no | w:Lever, w:Support |

For pressure plates, the wiki's timing table checks out against the 99-tick close: doors
open at 0 and close at 99 (100 if built later). Bridges, floodgates, grates and vertical bars
open at 100 (101) and close at 199 (200). Spikes go 40 / 139.

### 1.3 Rustfort today

`defense.rs`: `send_signals` queues `(now + LINK_DELAY, target, on)` for every target.
`apply_signal` sets `mech.on = on` unless it already equals `on`. Doors, hatches and spikes
are inverted (F1, F2), there is one delay for all (F3), no refractory period and no same-tick
toggle (F4). Linkable targets are only Door, Hatch, Floodgate, Bridge and UprightSpikes (F8).
`lever_pulled` toggles and sends. `defense_link` links instantly (F7).

### 1.4 Rust design

**New module `crates/rf-sim/src/linkage.rs`** (owner `[machines14]`). It owns signal routing
and the state machine of every linked device. `defense.rs` keeps the nav/fluid effects
(fling, crush, fall, spikes attack) and calls into linkage through tagged one-line hooks.

```rust
/// What a target does with signals (one per linked building).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum DevKind { Door, Hatch, Floodgate, Bridge, GrateWall, GrateFloor, BarsVertical,
                   BarsFloor, Spikes, Gear, TrackStop, Cage, Chain, Support }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub(crate) struct Device {
    pub bid: i32,
    pub kind: DevKind,
    /// DF's "open" state: door/hatch/floodgate/grate/bars open, bridge raised/retracted,
    /// spikes retracted, gear disengaged, track stop disabled. Off = DF's resting state.
    pub open: bool,
    /// Pending change (refractory devices): (due tick, wanted `open`), due = u64::MAX if none.
    pub due: u64,
    pub want: bool,
    /// Floodgate/door jammed by an item: retry while true (floodgate) / wait for a signal (door).
    pub jammed: bool,
    pub spent: bool,                // one-shot devices after firing
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Linkage {
    pub devs: Vec<Device>,          // sorted by bid (binary search; no HashMap)
    /// Signals sent this tick: (target bid, on). Drained in `linkage_tick` (sorted, deduped).
    pub inbox: Vec<(i32, bool)>,
    /// Devices with a pending change, sorted by (due, bid).
    pub timers: Vec<(u64, i32)>,
    pub links: Vec<(i32, i32)>,     // (trigger, target), sorted; mirror of Building::links
    pub jobs: Vec<LinkJob>,         // §3
    pub plates: Vec<Plate>,         // §4
    pub sig: u64,                   // building signature (rebuild when it changes)
    pub stats: LinkStats,
    #[serde(skip)] scratch: Vec<(i32, bool)>,   // reused buffer, no per-tick allocation
}
```

**Sending:** `Sim::link_send(trigger_bid, on)` pushes `(target, on)` into `inbox` for every
target in `links` (binary-search range on `trigger`). `lever_pulled` (defense) calls it
instead of `send_signals`: `// [machines14]`. Plates call it from §4.

**Receiving** in `linkage_tick(now)`, every tick:

1. Swap `inbox` into `scratch`, then sort and dedup it by `(bid, on)`. For each `bid`, `got_on`
   and `got_off` decide: both set means **toggle** (F4 same-tick rule). One set means
   `want = on`. Gear: toggle per signal received (both in one tick = two toggles = no change,
   ESTIMATE; the wiki only says "toggles on every signal").
2. If `want == open` and nothing is pending, do nothing (rule 4).
3. Delay 0: apply now. Otherwise, if `due != u64::MAX` the device is refractory and **ignores**
   the signal. Else set `due = now + delay`, `want`, and insert into `timers`.
4. Pop `timers` with `due <= now` in (due, bid) order and apply them.

**Applying** calls `defense`'s effect functions through new `pub(crate)` entry points (tagged
in defense.rs):

- Door/Hatch: `def.mechs[mi].on` keeps its meaning **"blocked by mechanism"**. Set
  `on = !open` for linked doors and hatches, which fixes F1 without touching `mech_state`.
  Linked doors and hatches start closed (`open = false`). Imported ones take
  `open = flags OPEN && linked`.
- Bridge: `on = open` (unchanged meaning). Fling, crush and fall as today, plus §6.
- Floodgate: `on = open`. On close: if an item lies on the tile, set `jammed = true` and stay
  open. Re-check jammed floodgates every 10 ticks (ESTIMATE) and close them once the tile is
  clear, destroying the liquid there (`set_liquid(p, 0, _)`).
- Spikes: `on` (extended) `= !open`, then `spikes_extend` when it becomes extended (F2).
- Grates and bars: new `MechKind` variants in defense are not needed. linkage keeps a nav
  override per tile through `nav.set_blocked` (wall grate or vertical bars closed = blocked,
  open = free) and `set_floor_override` (floor grate or floor bars closed = floor, open = none),
  plus `fluids_buildings_check(true)` is **not** needed (liquids pass both states). Replace the
  static `blocks_movement` entries `GrateWall` and `BarsVertical` (lib.rs:752) with
  linkage-driven state for linked ones: `// [machines14]`.
- Gear: flip `Device.open` (= disengaged), add/remove `DISENGAGED` in `Building::flags`, and
  set `self.power.sig = 0; self.power.checked = false` so `power_tick` rebuilds next tick (§5.4).
- TrackStop: flip `open` (= disabled) and set `self.carts.bld_sig = 0` so the cart table is
  rebuilt (§7).
- Cage/Chain/Support: one-shot effects (§8), then `spent = true`. The trigger keeps the link
  but the target is gone.

`Building::flags` stays the shared state for the renderer and fluids: `OPEN`/`CLOSED`,
`RAISED`, `EXTENDED`, `DISENGAGED`, `ACTIVE`, set exactly as fluids `building_state` reads them.

**Phase:** in `tick_inner` right after `self.defense_tick(now);` add
`self.linkage_tick(now); // [machines14]`. It runs after fluids and carts, so plates see this
tick's liquid and cart positions, and after defense, so trap enters have been processed.
Fast path: `if self.link.devs.is_empty() && self.link.inbox.is_empty() &&
self.link.plates.is_empty() { return }`, plus the building-signature rebuild every 100 ticks
(phase 59, ESTIMATE) or when `world.buildings.len()` changes.

**Constants (`linkage.rs`):**
`DELAY_GATE = 100` (bridge, floodgate, grate, bars), `DELAY_SPIKES = 40`,
`DELAY_TRACKSTOP_ON = 40`, `PLATE_CLOSE = 99`, `JAM_RECHECK = 10` (ESTIMATE),
`PULL_TICKS = 10` (ESTIMATE, keep defense's).

Remove `defense::LINK_DELAY` and `PLATE_RESET` from use (keep the consts for API stability, mark
them deprecated in a comment).

---

## 2. Pulling a lever

Sources: w:Lever.

- "Pull lever" posts a job for "a dwarf with the lever operation labor". **ESTIMATE:** in v50
  that labor is on for everybody by default, so it is treated as labor-free. The job is high
  priority. A lever can be **assigned to one dwarf**,
  and then only that dwarf pulls it.
- The wiki gives no duration. Rustfort uses `PULL_TICKS` = 10 (ESTIMATE, keep it).
- Rustfort today: `defense_try_job` and `defense_arrive_lever` give the job to any idle
  citizen who can reach the lever. **OK**, except that lever assignment is **MISSING**.

Design (P3): add `Linkage::lever_owner: Vec<(i32 bid, i32 unit_id)>` and check it in
`defense_try_job` (tagged hook `if !self.link_may_pull(s, lever) { continue }`). Add a
"Repeat" flag (v50 lever jobs can repeat, w:Lever "levers are set on repeat"): re-post the job
after it is done while the flag is set.

---

## 3. Linking job (mechanisms)

Sources: w:Lever ("Linking"), w:Mechanism, w:Floodgate ("Placement"), w:Pressure plate
("Construction"), w:Bridge ("A raised bridge cannot be linked ... from the inside").

### 3.1 The DF rules

- Each link needs **2 mechanisms** and a dwarf with the **Mechanic** labor. The **first**
  mechanism goes into the target ("the device end"), the second into the trigger.
- Mechanisms are picked automatically, **oldest first** (item id order, ESTIMATE). Forbidden
  mechanisms are skipped.
- Job flow: the mechanic carries mechanism 1 to the target, "works for a while", then carries
  mechanism 2 to the trigger and completes the link.
- The mechanic must be able to **stand on** the target. A locked door cannot be linked (the
  job is suspended, "cannot find path"). A raised bridge needs its **center tile** reachable
  while lowered.
- "Cannot link" if the pair is already linked or a link is queued, or if the target is
  forbidden.
- Mechanisms stay inside the buildings (`Building::items`). Deconstructing an end frees its
  mechanism. One-shot targets (cage, chain, support) drop theirs. One-use plates **destroy**
  their mechanisms.
- Mechanism quality does not change timing.
- First link to a plate: the mechanic finishing the first install does not cause an "open",
  but a "close" is sent (w:Pressure plate "Unusual triggers"). Gears linked to a plate toggle
  once on linking. **ESTIMATE: not modelled** (rare and confusing).

### 3.2 Rustfort today

`defense_link` / `round4_api::link_building`: instant, no items, no labor (**MISSING**). The UI
already says "a mechanic installs 2 mechanisms" (`ui/devices.rs` `link_click`), and the sheet
shows "Mechanisms in stock: N (a link needs 2)".

### 3.3 Rust design

```rust
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(default)]
pub(crate) struct LinkJob {
    pub trigger: i32, pub target: i32,
    pub claimed: u32,          // unit slot or NONE
    pub mech: [i32; 2],        // reserved mechanism item ids (-1 = not yet)
    pub phase: u8,             // 0 fetch #1, 1 install at target, 2 fetch #2, 3 install at trigger
    pub until: u64,            // install timer
    pub retry_at: u64,
}
```

- `Sim::link_building` (round4_api, signature unchanged) now **queues** a `LinkJob` and returns
  `Ok(())` after validating (both exist and are built, trigger is Lever/PressurePlate, target
  is linkable (§1.2 table), not already linked or queued, target not forbidden). Body:
  `self.link_queue(trigger, target) // [machines14]`. Keep instant linking for tests and
  scripts as `Sim::link_building_now` in `machines14_api.rs`.
- **Task:** append `Task::Link(u32)` (job index) at the **end** of `enum Task` in lib.rs
  (`// [machines14]`, appended so earlier variants keep their positions in saves). Add hooks in
  the decide / arrive / cancel / caption matches the way `PullLever` does it: `linkage_try_job`
  (labor `labor::MECHANIC`, after `defense_try_job`), `linkage_arrive`, `linkage_cancel`,
  `linkage_caption` ("Link a building to a trigger").
- **Item handling:** reuse the hauling helpers that `defense` reload jobs use
  (`try_reload_job` / `defense_arrive_reload` pattern): reserve the mechanism (oldest free
  TRAPPARTS item by id, not forbidden, reachable: same nav component), carry it, put it into
  `Building::items` of the target (`item.building = target`).
- **Work time:** `LINK_WORK = 100` ticks per end (ESTIMATE), scaled by Mechanics skill with the
  same helper workshop jobs use. XP: `crate::skills::XP_USE` to `MECHANICS` per end (ESTIMATE).
- **Stand tile:** the target's own tiles when walkable (door, hatch, floodgate (only when
  open), bridge center when lowered), else any walkable neighbour (gear, grate, bars, support,
  cage, chain). If none of the goals is reachable, `retry_at = now + 500` (ESTIMATE) and the
  sheet says "cannot reach".
- **Completion:** push the pair into `link.links` and both `Building::links` (as
  `defense_link` does today), update `def.mechs[..].targets`, and create the `Device` if it is
  new. **No signal** on completion (the device keeps its current state).
- **Cancel:** drop the carried mechanism where the mechanic stands and unreserve it.

API (`machines14_api.rs`): `link_jobs(&self) -> Vec<LinkJobInfo { trigger, target, phase,
worker: Option<i32> }>`, `cancel_link_job(trigger, target)`, `free_mechanisms(&self) -> usize`.

---

## 4. Pressure plates

Source: w:Pressure plate (all of it), w:Minecart ("Capacity and weights").

### 4.1 The DF rules

- **Triggers** (any combination; the plate needs **one mechanism per enabled trigger type**,
  1 to 4, built by a Mechanic):
  - **Creatures:** the **sum of the weights** of the qualifying creatures on the tile lies in
    `[unit_min, unit_max]`. The selectable range is 10 000 to 2 000 000 (the menu shows
    values divided by 10). Weight = creature size, and equipment adds to it. Vermin never
    count. **Citizens** (including tame animals) count only with the **citizens** setting;
    non-citizens always count. **Merchants, their pack animals and diplomats never count**,
    nor do **TRAPAVOID** creatures or dead ones. **Unconscious or webbed creatures always
    count**, whatever the settings (also your own dwarves, also TRAPAVOID). Flyers and
    swimmers count in every tile they move through. Creatures falling *through* a hanging
    plate do not count, but creatures landing alive on it do. Invaders do not trigger plates
    that a diplomat of their civ has seen (**ESTIMATE: not modelled**).
  - **Water** / **magma:** "CLOSE < min <= OPEN <= max < CLOSE": the plate is "on" while
    `min <= depth <= max` (depth 0..7). Presence or absence can both be selected (min 0 is
    allowed).
  - **Track (minecarts):** only on tiles with track. Gross cart weight (cart + load + rider)
    in Γ in `[track_min, track_max]`. The range is 1 to 2000 in steps of 50; the default is
    min 1, max "any". An empty cart weighs density/25 Γ (40 000 cm³ x density); platinum is
    the heaviest at 856 Γ.
- **Timing:** "on" at once. "Off" when the condition has been false for **99 consecutive
  ticks**. Retriggering inside that window cancels the "off" and sends no new "on".
- **One-use** plates (no `PLATE_RESETS` / "resets" setting): send "on" once, then
  **deconstruct, destroying their mechanisms**.
- Not targeted by building destroyers.

### 4.2 Rustfort today

`defense.rs`: a plate is a `MechKind::PressurePlate`. `trap_entered` (called from the
`move_to` hook via `def.pending`) sets `on`, `plate_until = now + PLATE_RESET` (100), and
sends "on" through the 100-tick delay. `defense_tick` sends "off" every 10 ticks once
`plate_until` has passed, even if somebody still stands there (**WRONG**, F5). Only
`sets_off_traps` creatures (hostiles, wild animals) or citizens with `CITIZENS` count. No
weights, liquids, carts or one-use plates (**MISSING**). The exporter already exports
everything needed: flags `PLATE_UNITS`, `PLATE_WATER`, `PLATE_MAGMA`, `PLATE_TRACK`,
`PLATE_CITIZENS`, `PLATE_RESETS`, `ACTIVE` (ready), and the extra
`plate: {unit_min, unit_max, water_min, ..., track_max}` (EXPORT_FORMAT §round 4), but rf-core
has no field for `plate`, so serde drops it.

### 4.3 Rust design

**rf-core (additive):** in `Building`:

```rust
/// [machines14] Pressure plate ranges (export extra `plate`); None = not a plate / unknown.
#[serde(default)] pub plate: Option<PlateInfo>,
/// [machines14] Track stop friction (export extra `friction`, DF track_stop_info.friction); 0 = unknown.
#[serde(default)] pub friction: i32,
/// [machines14] Roller speed (export extra `speed`, planned); 0 = unknown.
#[serde(default)] pub speed: i32,
```

```rust
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PlateInfo { pub unit_min: i32, pub unit_max: i32, pub water_min: i32, pub water_max: i32,
                       pub magma_min: i32, pub magma_max: i32, pub track_min: i32, pub track_max: i32 }
```

The key names match the existing export, so real forts load their ranges without exporter
changes. The plate's flags stay in `Building::flags` (`PLATE_*`).

**linkage.rs:**

```rust
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Plate {
    pub bid: i32, pub tile: u32,
    pub units: bool, pub water: bool, pub magma: bool, pub track: bool,
    pub citizens: bool, pub resets: bool,
    pub r: PlateInfo,
    pub on: bool,              // last signal sent
    pub false_since: u64,      // tick the condition became false (u64::MAX while true)
    pub occ: Vec<u32>,         // unit slots seen entering (pruned when they leave)
    pub cart_w: i32,           // gross weight of a cart that touched the tile this tick (0 = none)
}
```

- **Defaults** for plates the player builds: units on, `unit_min = 10_000`,
  `unit_max = 2_000_000`, citizens off, resets on (ESTIMATE: DF's dialog defaults are not
  documented); water and magma `[1, 7]`, track `[1, 2000]` when enabled.
- **Entering:** `trap_entered` keeps calling defense for the other traps. For
  `MechKind::PressurePlate` it calls `self.link_plate_enter(s, tile)` (pushes `s` into `occ` if
  not present): `// [machines14]`.
- **Carts:** a hook in `minecarts.rs` `cart_set_tile` (one tagged line) calls
  `link_plate_cart(t, gross_weight)` when `def.trap_tile(t)`. Gross weight = cart item weight
  + `load_weight(cart)` + rider body weight, in Γ (minecarts already computes the first two).
  Carts that move several tiles in one tick pass through `cart_enter` for each tile, so fast
  carts still trigger.
- **Polling (every tick, only plates with `occ` non-empty, `on`, a liquid trigger or
  `cart_w > 0`):** the condition is `units_ok || water_ok || magma_ok || track_ok`, where:
  - `units_ok`: sum the weight of every slot in `occ` that still has `ai.pos == tile`, is
    alive, is not a merchant, pack animal or diplomat, is not TRAPAVOID
    (`race_info().0`/`caverns_trapavoid`), and is either not a citizen/tame or the plate has
    `citizens`. **Unconscious** (`combat.f[s].uncon_until > now`) or webbed units always
    count. Remove slots that have left from `occ`. Weight =
    `Unit::body_size + carried/worn item weight` (ESTIMATE: worn weight via `item_weight`; the
    wiki says equipment adds but gives no formula). In range = `unit_min <= sum <= unit_max`.
  - `water_ok` / `magma_ok`: `liquid_at(tile)` depth and kind in range.
  - `track_ok`: `cart_w` in `[track_min, track_max]` (track_max 0 = "any"). Reset `cart_w = 0`
    after each poll. A cart resting on the plate keeps setting it, because `cart_set_tile` is
    not called for a resting cart, so stopped carts on the tile have to be checked separately:
    `carts.carts` with `tile == plate.tile` and state AtStop/Stalled.
- **Edges:** condition true and `!on`: `on = true`, `link_send(bid, true)`, set `ACTIVE`, and
  if `!resets` deconstruct the plate (remove the building, destroy its own TRAPPARTS items;
  targets keep theirs). Condition true and `on`: `false_since = MAX`. Condition false and
  `on`: if `false_since == MAX` set it to `now`; if `now - false_since >= PLATE_CLOSE (99)` then
  `on = false`, `link_send(bid, false)`, clear `ACTIVE`.
- **Safety net:** every 100 ticks (phase 61, ESTIMATE), rebuild `occ` for every plate from
  scratch by scanning `ai` once (O(units)), so creatures displaced without `move_to` (flung,
  fallen, caught, spawned) are seen too.
- **Stats:** `plates_on`, `plates_off`, `plates_spent`.

The wiki's plate-before-device one-tick offset is not modelled (ESTIMATE, §1.1 rule 7).

---

## 5. Power networks

Sources: w:Power, w:Gear assembly, w:Axle, w:Water wheel, w:Windmill, w:Screw pump,
w:Millstone, w:Roller, w:Machine component.

### 5.1 Numbers (Rustfort: OK)

| machine | power | Rustfort `power.rs` |
|---|---|---|
| water wheel | +100 when it has flow, and always -10 (net +90 / -10) | `WHEEL_POWER`, `WHEEL_USE` OK |
| windmill | +40, +20 or (rarely) 0; uses 0 | see 5.3 |
| axle (horizontal or vertical) | -1 per tile | OK |
| gear assembly | -5 | OK |
| screw pump | -10 (even with nothing to pump) | OK |
| millstone | -10 | OK |
| roller | -2 per tile (whatever its speed) | OK |
| network runs | when total produced >= total needed (one shortfall stops **all** machines on it) | `powered = produced > 0 && produced >= used` OK |

### 5.2 Connections (ports): DF rules vs `power_rebuild`

| machine | DF rule | Rustfort today | class |
|---|---|---|---|
| Gear assembly | all 6 orthogonal tiles. Built on stairs: no vertical transfer. | 6 ports | OK (stairs: MISSING) |
| Horizontal axle | only along its axis, past both ends; axles in a line connect directly | OK | OK |
| Vertical axle | above and below | OK | OK |
| Any vertical link | "Floor tiles will obstruct the transfer of power over z-levels": the upper tile must have no floor (open space, the top of a ramp, or a hanging machine). | not checked | MISSING |
| Screw pump | **either** tile, all orthogonal directions **including above and below** ("a properly aligned pump stack only needs one power input"); adjacent pumps connect | 4 sides + above, **no below** | WRONG (pump stacks never connect) |
| Millstone | sides and above (it cannot be built powered from below: the builder must stand on it) | 4 sides + above | OK |
| Roller, 1 tile | 4 cardinal directions, same level, never from above | 4 sides | OK |
| Roller, longer | **across** its direction only ("along" is unreliable and depends on build order: treat as no) | 4 sides of every tile | WRONG |
| Windmill | "transmitted either horizontally through the surrounding twelve tiles, or downward through its center tile" | only below the center | WRONG |
| Water wheel | the tile beside its **center**, across the axis ("Build the water wheel with its central tile orthogonally next to a gear assembly, a horizontal axle, a screw pump, or the central tile of a pre-existing water wheel"); parallel wheels connect center to center. The long-axis ends and above the center are ESTIMATE. | both long-axis ends + above center | WRONG (center-side ports missing) |
| Disengaged gear | transmits nothing | not modelled | MISSING |

Mutual connection (each machine reaches a tile of the other) stays the rule; it is the
simplest reading of the wiki and avoids one-sided links.

### 5.3 Producers

- **Water wheel:** "produces power as long as it has flowing water with a minimum depth of
  **4/7** under at least one of its three tiles" (Rustfort: `d > 0`, **WRONG**). "Flowing"
  (w:Water wheel "Flowing Water") means either (1) **gradient flow**, water moving from deeper
  into shallower water (7/7 water moving through is not flowing), or (2) the tile is marked as
  flowing because water drained **off the map edge or into an aquifer** from it. That mark
  **persists** even after the drain is blocked, and even through draining and refilling. It
  never works with magma or waterfalls. A wheel turning in still water costs 10 and makes
  nothing. Flow direction: "a N-S wheel over water flowing straight east or west" does
  nothing (**ESTIMATE: not modelled**; most DF flow is diagonal).
  - Rustfort today: `power_flow_seen(t)` from fluids whenever the liquid in a tile under a
    wheel moved, fresh for `FLOW_FRESH` = 200 ticks (ESTIMATE).
  - Design: keep `power.under` / `flow_at`, but (a) require depth >= 4, (b) let the fluids
    hook pass `gradient: bool` (source depth > destination depth) and only count gradient moves
    as fresh flow, and (c) add `power.flow_mark: Vec<bool>` parallel to `power.under`, set
    persistently when fluids reports an **edge drain or aquifer sink** from that tile
    (new tagged call in fluids where it deletes water at the map edge / into aquifer walls:
    `self.power_flow_mark(t) // [machines14]`). Wheel active =
    `depth >= 4 && !magma && (flow_mark || fresh gradient flow within FLOW_FRESH)`.
  - Rivers and brooks in imported forts: DF marks them as flowing. **Exporter (planned):** for
    tiles with liquid, export the designation bit that means DF's flowing quality (candidate:
    `liquid_static == false`; **ESTIMATE, check it on a river tile with DFHack first**) as a
    sparse list `flowing_tiles` in a new optional file. Until then, mark every tile of the
    geology layers `RIVER` and `BROOK` (EXPORT_FORMAT line 167) as flowing at load
    (ESTIMATE).
- **Windmill:** needs its **center tile outside**. 40, 20 or (rarely) 0. w:Machine component
  says the wind "is constant across a map and time", while w:Windmill says it depends on the
  map's latitude and sometimes on height. Rustfort today: a new random roll per windmill per
  **day** (15/20 days 40, 4/20 20, 1/20 0) (**ESTIMATE, contradicts the wiki**). Design: a
  per-fort wind level from `World::climate` once `[weather13]` provides it. Until then, one
  roll per fort from the sim seed: 40 (75 %), 20 (20 %), 0 (5 %) (ESTIMATE). Plus a start-up
  delay of 50 ticks after building ("give it a few ticks (fifty or so)", ESTIMATE). This
  changes no bench hash (no windmills in the reference fort).
- **Frozen:** components on natural ice floors freeze, and a frozen component stops its whole
  network ("frozen elsewhere"). MISSING. Hook: `machine_frozen(t)` asks
  `temperature_at(p) <= 10_000` and the tile material ice (owner `[weather13]`); default false.
  Do not implement the bugs.

### 5.4 Gear assemblies: engage and disengage (F6)

- Any signal toggles the gear (§1.2). A **disengaged** gear transmits nothing. **ESTIMATE:** it
  also uses no power, since it has no network. The wiki is silent; DF's sheet shows it as its
  own network.
- **Support:** "a disconnected gear assembly can't support anything". A machine **hanging**
  from it (a windmill right on top of it, a water wheel whose only support is that gear)
  collapses, though "not *immediately*"; it happens on the next machine stability check (when
  any machine is deconstructed). Components on solid ground ("stable foundation") never
  collapse.
- Rustfort today: `power.rs` says "Gear assemblies are always engaged"; `ui/round12.rs:845`
  already tells the player "a lever can disengage it".
- Design: (a) `is_machine` stays; in `power_rebuild` (tagged lines), a gear whose building has
  the `DISENGAGED` flag gets **no ports**. (b) `power_signature` mixes in the `DISENGAGED` flag
  (tagged). (c) linkage sets `power.sig = 0` on toggle, so the next `power_tick` rebuilds.
  (d) **Stability (P3):** `machinery.rs` `machine_stable(m)`: a machine is supported if any of
  its tiles has a floor or solid shape below it at its own z (`Shape::has_floor` of the tile it
  stands on, or a wall below), or if it is connected through ports to a supported, engaged
  machine. Run it when a gear is disengaged and when a machine is removed (DF's trigger), and
  deconstruct unsupported machines with `remove_building`, dropping their materials. ESTIMATE:
  run it at once, not "eventually".

### 5.5 Screw pumps (F13)

| DF rule (w:Screw pump) | Rustfort today | class |
|---|---|---|
| 2x1. Intake = one level below the tile next to the input end; output = the tile beyond the output end | `power_rebuild` intake/output | OK |
| Intake needs at least 2/7 ("more than 1/7") | `pump_step` `da < 2` | OK |
| Moves everything that fits from intake to output every tick | `pump_step` | OK |
| **The space above the source cannot be a floor, stairway or wall**; stairs in the intake block it; grates, floor bars and fortifications at the intake level are fine | not checked | MISSING |
| The output never goes above the pump's level; a full output stops the pump | `pump_step` min(7 - db) | OK |
| **Pumped liquid is pressurized to the pump's z (magma too)**: a U-turn delivers it at the other end | fluids pressure only for falling water | MISSING (fluids hook; P3) |
| **Pumping water onto magma (or the reverse) turns the output tile into obsidian** | refuses ("ESTIMATE") | WRONG |
| Pumping salt or stagnant water gives clean water | flags not touched | MISSING |
| **A pump keeps running 49 ticks after losing power** (1 tick of power = 50 ticks of pumping) | stops at once | MISSING |
| **Pumps run newest first** (reverse build order) | machine list order | MISSING (ESTIMATE: newest = highest building id) |
| Output tile **blocks liquid and creatures**; the input tile does not | output tile walkable | MISSING |
| The operator stands **on the input tile**; the builder too | `pump_goals`: any walkable pump tile, else beside it | WRONG (minor) |
| Operators make no power for the network; a manned pump only runs itself | manned pump runs itself | OK |
| A powered pump transfers power | ports | OK |
| Intake tile blocked (cave-in, obsidian, tree): the pump runs but moves nothing | depth 0 | OK |
| Manual job length | `PUMP_TICKS` 600 | ESTIMATE |

Design (in `machinery.rs`, tagged calls from `power_tick`):

- `Power::run_until: Vec<u64>` parallel to `machines` (`#[serde(default)]`). When the network is
  powered, `run_until = now + 49`. A pump runs while `powered || now <= run_until || manned`.
  `power_balance` only runs every 100 ticks, so record the tick it last saw the net powered
  and keep running for 49 ticks after the first unpowered balance. ESTIMATE: the drop is seen
  at the next balance (at most 100 ticks late). The exact way is to rebalance when a gear
  toggles or a machine changes.
- Order: iterate pumps by **descending `bid`** (precompute `pump_order: Vec<u32>` in
  `power_rebuild`; no per-tick sort).
- `pump_step`: if the output has the other liquid, call fluids' obsidian routine
  (`fluids_make_obsidian(b)`, an existing function or a small tagged pub(crate) wrapper around
  the code fluids uses where water meets magma) and pump nothing more. Clear the `SALT` and
  `STAGNANT` tile flags on the output when water is pumped.
- Intake check in `power_rebuild`: the tile at pump z above the intake (`intake + layer`) must
  have shape `Empty`, `RampTop` or `None`, else `intake = NONE`.
- Output-tile blocking: `nav.set_blocked(output_pump_tile, true)` plus a fluids building mask
  (`relevant_building("ScrewPump")`, wall-like on the output-end tile only). The fluids part
  is a tagged line in `fluids.rs` `building_state` / `fluids_mark_building`.
- `pump_goals`: input-end tile only (fall back to the old goals when it is not walkable).

### 5.6 Millstones

"If the power to the millstone is interrupted, any jobs must be re-queued when power is
returned" (w:Millstone). Rustfort: `power_shop_ok` refuses new jobs without power (OK).
Whether a job in progress is cancelled when power drops: **MISSING**. P3: in `power_balance`,
when a net goes unpowered, cancel the active job of its millstones (`production` cancel
helper) and keep the queued job (not "re-queued", ESTIMATE).

### 5.7 Rollers (power side)

- Need power 2 per tile. Unpowered rollers are ignored by carts (OK).
- Only affect carts **on track** ("if placed on a floor or ramp without any tracks, they are
  ignored"). Rustfort does not check track bits in `cart_physics`: **MISSING**, one tagged
  condition `self.track_bits(t0 as usize) != 0`.
- Guided carts are unaffected: OK (guided carts are not in the physics).
- A cart moving against the roller is sent back at the roller's speed, "unless it was moving
  extremely fast". The roller changes the cart's speed by at most 100 000 per tick
  (w:Minecart: "speed increment of 100000 allows to reverse carts from the full 'highest'
  (50000) speed"). Rustfort reverses any speed. ESTIMATE fix:
  `along' = min(value, along + 100_000)`.
- A cart crossing **perpendicular** gains the roller speed sideways and derails diagonally
  unless a wall constrains it. MISSING; this is cart physics (`[perf13]`), so list it only.

---

## 6. Bridges (details beyond §1)

Source: w:Bridge, w:Dwarven atom smasher.

| rule | Rustfort `defense.rs` | class |
|---|---|---|
| Response 100 ticks after the signal, all sizes | `LINK_DELAY` 100 | OK |
| Raising: the edge row/column becomes a wall one z high; a one-wide bridge is a wall over its whole area | `edge` | OK |
| Raising flings everything on it (creatures **and items**) | creatures only, items stay | MISSING (items) |
| Retracting: everything on it drops (semi-random velocity) | `fall_from` creatures; items not dropped | MISSING (items) |
| Lowering destroys creatures and items on the area (no corpse) | `crush` | OK |
| Lowering destroys **fluids** on the area; "any water displaced by a raising bridge is destroyed" | not done | MISSING |
| **A creature of size >= 1 200 000 on the bridge stops it** (raising or retracting); lowering onto one **deconstructs the bridge**; caged creatures do not count | not done | MISSING |
| Raised bridges are immune to building destroyers | OK | OK |
| Lowered bridges count as track (straight crossing) for carts; raised bridges are walls for carts | not done | MISSING (§7) |
| Operating a bridge with a cart on it flings or crushes the cart | not done | MISSING (P3) |

Design: in linkage's bridge apply path, before calling defense:
`if self.units_on(tiles).any(|s| body_size(s) >= 1_200_000 && !caged)` then lowering means
`remove_building` (the bridge deconstructs, materials dropped) and raising or retracting means
nothing happens (the device stays as it is, `due` cleared). Use `world.units[..].body_size`
(current size). Fluids: `set_liquid(p, 0, _)` on every tile when lowering and on the raised
edge when raising. Items: reuse `crush` for lowering. Raising moves loose items on non-edge
tiles to the fling destination tile (ESTIMATE: same ring search as units).

---

## 7. Minecarts and devices (no collisions here)

Sources: w:Minecart ("Track Stops", "More on Track stop", "Roller", "Switching", "Bridges"),
w:Lever (track stop), w:Pressure plate (track).

| rule | Rustfort `minecarts.rs` | class |
|---|---|---|
| Track stops are `Trap/TrackStop` buildings | looks for kind `TrackStop` | **WRONG (F10)** |
| Track stop friction from the building (export `friction`: 10, 50, 500, 10 000, 50 000) | `stop_friction(subtype)`: subtype is `TrackStop` for exported stops, so always 50 000 | WRONG |
| A linked track stop switched "on" = disabled: no friction, no dumping, the tile acts as the track below; back on 40 ticks after "off" | not modelled | MISSING |
| Plates with the track trigger see the cart's gross weight | not modelled | MISSING (§4) |
| Closed doors, floodgates, raised bridges, closed wall grates and vertical bars stop carts like walls | carts ignore buildings | MISSING |
| A lowered (or extended retracting) bridge is a track crossing in all four directions, and a floor over a gap | ignored (the cart falls) | MISSING |
| Hatch (closed) over a gap is a floor for carts | ignored | MISSING (ESTIMATE: as floor without track) |

Design (all reads; physics stays with minecarts and `[perf13]`):

- **Track stop recognition (fix F10):** add `pub(crate) fn is_track_stop(b: &Building) -> bool
  { b.kind == "TrackStop" || (b.kind == "Trap" && b.subtype == "TrackStop") }` in
  `linkage.rs`, and use it in `carts_bld_rebuild` (two tagged lines in minecarts.rs). Friction:
  `if b.friction > 0 { b.friction } else { stop_friction(&b.subtype) }` (tagged). Dump direction
  is already in `direction`. **Coordinate with `[perf13]`** (they edit minecarts.rs): the
  change is two `// [machines14]` lines. Mention it in the commit message.
- **Disabled stops:** in `carts_bld_rebuild`, skip stops whose `Device.open` is true (disabled).
  linkage sets `carts.bld_sig = 0` on change, and `carts_bld_rebuild` already re-runs on
  signature change. Make sure the signature mixes in the `ACTIVE`/`DISABLED` flag: add the
  flag `DISABLED` to the building and fold `has_flag(DISABLED)` into the hash (tagged).
- **Cart blocking:** `linkage_cart_tile(q) -> CartTile { Free, Wall, Crossing }`, a lookup in a
  bitset `link.cart_wall` / `link.cart_cross` (`Vec<u64>` over tiles, rebuilt when a device
  changes; `#[serde(skip)]`, rebuilt in `after_exact_load` and on load). In `cart_next`, one
  tagged line before the shape checks: `Wall` returns `Err(NONE)` (the cart stops as at a
  wall); `Crossing` returns `Ok(q)` and is treated as having track bits `DN|DS|DE|DW` (a tagged
  line in `track_bits` callers: `cart_enter` uses bits). Cost: one bit test per tile entered
  (fast path when no devices exist).

---

## 8. One-shot devices: cages, chains, supports

- **Cage** (built `Cage` building linked to a trigger; on "on"): every unit inside
  (`flags.caged` units on the cage tile and units referenced by the cage item) is released
  (`caged = false`, `Kind` restored as in `animals.rs:2671`, placed on the cage tile or the
  nearest walkable neighbour). The cage building is removed. The cage item and the target's
  mechanism lie on the tile. Hostiles released this way act at once (combat picks them up).
- **Chain** (restraint): the chained creature is released. The chain item goes to its
  inventory (ESTIMATE: as a worn item, or dropped if not supported), the mechanism is dropped,
  the building is removed.
- **Support:** the building is removed (material and mechanism dropped), then
  `cavein_check(&[support_tile, tile_above])`. The cave-in module already treats supports as
  support (`cv_refresh_supports`). Check that the refresh sees the removal.
- All three: `Device.spent = true`, and the links to them are dropped from `link.links`
  (the triggers keep their mechanisms, which in DF stay inside the lever: leave the
  lever's item list alone).

---

## 9. Doors and floodgates: jamming and liquids (F15)

- "Any item on the same tile as a door will cause the door to be propped open, which makes it
  impossible to lock and will allow fluids in. Such items will also jam mechanically linked
  doors; in order for the door to be shut, the item must be cleared out and the lever or
  pressure plate must be triggered again." Floodgates "refuse to close" with any item on
  them and "grind shut by themselves" once it is gone.
- "A door destroys any fluid on its tile when it closes." The floodgate likewise destroys the
  liquid on its tile.
- A creature on the tile also keeps a door from closing (ESTIMATE: as with items, "a dwarf or
  other creature passing through will hold" it, w:Machine component).
- Design: in linkage, closing a Door/Floodgate checks loose items on the tile through the item
  position index used by `crush` (or a linear scan bounded to that tile through
  `items_at(tile)` if available) and units on the tile. Blocked: Door stays open with
  `jammed = true`, and only the next "off" retries (the device stays open, so a repeated "off"
  is not a no-op). Floodgate: `jammed = true`, retried every `JAM_RECHECK` ticks. On closing:
  `set_liquid(p, 0, magma)` on the tile.

---

## 10. Building machines and devices (F9)

Source: the infobox of each wiki page. Rustfort today: `build_api.rs` places them, and
`production.rs` `new_cons` has no recipe, so they stay unbuilt forever.

| building (kind/subtype) | items | labor (Rustfort bit) | notes |
|---|---|---|---|
| Trap/Lever | 1 mechanism | MECHANIC | |
| Trap/PressurePlate | 1 mechanism per enabled trigger type (1-4) | MECHANIC | the settings are chosen before placing |
| GearAssembly | 1 mechanism | MECHANIC | |
| AxleHorizontal | logs: 1 for 1-3 tiles, 2 for 4-7, 3 for 8-10 | CARPENTER (wiki: "Axles require the carpenter labor"; infobox says Mechanics: ESTIMATE carpenter) | |
| AxleVertical | 1 log | CARPENTER | |
| Rollers | `len / 4 + 1` mechanisms + 1 rope or chain | MECHANIC | |
| Trap/TrackStop | 1 building material (log, bar or block) (w:Minecart) + 1 mechanism (ESTIMATE, the current label) | MECHANIC | dump direction and friction chosen before placing |
| ScrewPump | 1 block + 1 enormous corkscrew (`TRAPCOMP:ITEM_TRAPCOMP_ENORMOUSCORKSCREW`) + 1 pipe section (`PIPE_SECTION`) | by the block's material: MASON / CARPENTER / FORGE_FURNITURE (ESTIMATE: MASON or CARPENTER) | the builder stands on the input tile |
| WaterWheel | 3 logs (or wooden blocks) | CARPENTER | |
| Windmill | 4 logs (or wooden blocks) | CARPENTER | |
| Workshop/Millstone | 1 millstone item (`MILLSTONE`) + 1 mechanism | MILLER? (ESTIMATE: MECHANIC) | |
| Bridge | `tiles / 4 + 1` of stone, logs or blocks (mixed allowed) | by the core material: MASON / CARPENTER | max 31x31; no more than 1/7 water on its tiles |
| Floodgate | 1 floodgate item | MECHANIC (w:Floodgate infobox "Mechanics") | |
| Hatch | 1 hatch cover | HAUL_FURNITURE (as doors) | |
| GrateWall / GrateFloor | 1 grate | HAUL_FURNITURE (ESTIMATE) | |
| BarsVertical / BarsFloor | 1 metal bar | HAUL_FURNITURE (w:Bars: "a dwarf with the furniture hauling labor") | |
| Support | 1 stone, log, block or bar | MASON / CARPENTER / FORGE_FURNITURE by material | |
| Cage (built) | 1 cage | HAUL_FURNITURE (ESTIMATE) | |
| Chain | 1 chain or rope | HAUL_FURNITURE (ESTIMATE) | |

Design: one tagged fallback in `new_cons`: `k => match furniture_item_for(k) { Some(..) =>
..., None => return self.mech_cons(b) // [machines14] }`, with `machinery.rs`
`fn mech_cons(&self, b) -> Option<(Vec<(u8, &'static str)>, u64)>` built from the table. It
needs pc classes for corkscrew, pipe section, floodgate, hatch cover, grate, bar, millstone
and cage. Check `production.rs` `pc` for existing ones (`LOG`, `BLOCKS`, `MECHANISM`,
`CHAIN`, `FURNITURE` with a subtype string). Use `(pc::FURNITURE, "FLOODGATE")`-style entries
where the item kind is enough, as siege engines do. Add the missing options to
`build_api.rs` in a new `MECH14_OPTIONS` list chained after `APP12_OPTIONS` (same pattern as
`app12_extra`, tagged): Lever, Pressure plate, Bridge (dragged, with a raise direction
N/E/S/W or Retract), Floodgate, Hatch, Wall grate, Floor grate, Vertical bars, Floor bars,
Support, Cage, Chain. Placement rules: floodgates need flat unflooded floor; bridges need one
walkable perimeter tile and at most 1/7 water; floor grates and hatches need orthogonal support.

---

## 11. Data needed from rf-core and the export

| need | where it comes from | change |
|---|---|---|
| plate ranges | export extra `plate` (exists) | rf-core `Building::plate: Option<PlateInfo>` (additive, `#[serde(default)]`) |
| plate flags | export flags `PLATE_*` (exist) | none |
| track stop friction | export extra `friction` (exists) | rf-core `Building::friction: i32` |
| track stop on/off | DF `track_stop_info.track_flags` (exported uppercased into flags). **Exporter:** add `DISABLED` when the stop is switched off by its trigger (field name to check in df-structures: `track_stop_info.track_flags.use_dump` / friction 0, ESTIMATE) | additive flag |
| gear disengaged | **Exporter (new):** for `BT.GearAssembly` add flag `DISENGAGED` from `bld.gear_flags.disengaged` (df-structures `building_gear_assemblyst`) | additive flag; EXPORT_FORMAT row |
| roller direction and speed | **Exporter (new):** `BT.Rollers`: `direction` = N/S/E/W from `bld.direction` (`screw_pump_direction` enum), extra `speed` = `bld.speed` (10000..50000) | rf-core `Building::speed: i32` |
| pump direction, manual | **Exporter (new):** `BT.ScrewPump`: `direction` = `FromNorth`/... from `bld.direction`, flag `PUMP_MANUALLY` from `bld.pump_manually` | additive |
| wheel and axle axis | **Exporter (new):** `BT.WaterWheel`, `BT.AxleHorizontal`: `direction` = `NS` if `bld.is_vertical` else `EW` | additive |
| machine power, for validation | **Exporter (new):** extras `power_produced`, `power_needed`, flags `MACHINE_ACTIVE`, `FROZEN` from `df.machine.find(bld.machine.machine_id)` (`cur_power`, `min_power`, `flags.active`, `flags.frozen`) | read-only extras, ignored by serde unless added |
| flowing water | **Exporter (optional, later):** sparse `flowing_tiles` (§5.3), mapping to be checked first | new optional file |
| lever owner | DF `building_trapst` assigned unit (field to check) | later |

All the exporter field names above are **to be checked against DFHack's df-structures
(`df.buildings.xml`) before use**. Every access stays inside `pcall`, read-only, in the
existing `building_state` frame slice. Document each one in EXPORT_FORMAT §round 4 (new rows
tagged `[machines14]`).

---

## 12. State, saves, hash

- `Sim` gets `link: linkage::Linkage` (`#[serde(default)]` on the field; tagged in the struct,
  in `Sim::new`, and in the `from_exact` default path if fields are listed there).
- `saves11.rs` `exact_parts`: `("link", enc(&self.link)), // [machines14]`.
- `persist.rs`: `v.push(("link", fnv(|mix| self.link_hash(mix)))); // [machines14]`, and the
  state hash in lib.rs mixes `self.link_hash(&mut mix)` (tagged, next to `power_hash`).
  `link_hash` mixes device `(bid, open, due)`, plate `(bid, on, false_since)` and job
  `(trigger, target, phase, claimed)`.
- `Power` gets `run_until`, `pump_order`, `flow_mark` (all `#[serde(default)]`). `pump_order`
  is a cache: `#[serde(skip)]`, rebuilt in `power_rebuild` (run it in `after_exact_load`, which
  is `saves11.rs:438`, an empty fn today: add a tagged `self.link_after_load();` that also
  rebuilds the `cart_wall`/`cart_cross` bitsets).
- `Task::Link(u32)` is appended at the end of `enum Task`.
- Test: `cargo test -p rf-sim --release --test saves11`, and the `saveload` example must stay
  IDENTICAL with a lever pulled and a plate held mid-delay (add such a step to the test, see
  §14).

---

## 13. UI (`crates/rf-app/src/ui/machines14.rs`, API `crates/rf-sim/src/machines14_api.rs`)

API (all `pub fn` on `Sim`, `[machines14]`):

- `linkable_targets(&self, trigger: i32) -> Vec<LinkTarget { bid, kind_label, pos, state,
  already_linked, queued, reachable }>`: sorted by distance to the trigger, then id.
- `link_building` (existing): now queues the job. `link_building_now` does it instantly
  (tests, `--ui` scripts).
- `link_jobs`, `cancel_link_job`, `free_mechanisms`.
- `plate_settings(bid) -> Option<PlateSettings { units, water, magma, track, citizens, resets,
  r: PlateInfo }>` and `set_plate_settings(bid, PlateSettings)`. DF only allows this before
  construction; Rustfort allows it while unbuilt and refuses with Err once built (DF:
  "settings ... must be set before construction"). Plates the player places get the settings
  from the build dialog.
- `device_state(bid) -> Option<DeviceInfo { open, pending_until, jammed, kind }>`.
- `power_networks(&self) -> Vec<NetInfo { id, produced, used, powered, machines: Vec<i32> }>`
  and `machine_net(bid) -> Option<u32>` (the existing `power_report` stays).
- `set_lever_owner(bid, unit)` and `set_lever_repeat(bid, on)` (P3).

UI pieces:

1. **Lever / plate target picker** (replaces the one-click `Tool::Link` flow, which stays as a
   shortcut): a panel listing `linkable_targets` with a kind filter (doors, bridges,
   floodgates, hatches, grates and bars, gears, track stops, cages, chains, supports), state,
   distance, z, and "linked" / "queued" / "unreachable" badges. Hover outlines the target on
   the map (reuse `devices::outline_tiles`); click queues the link; multi-select with Shift.
   The footer shows "Mechanisms: N free, M needed for the selection" in red when short.
2. **Plate settings panel** (sheet section for unbuilt plates, read-only once built): four
   trigger checkboxes; creature min/max as two sliders over the DF scale (10 000..2 000 000)
   with an example creature name per step (from the raws: the first creature whose adult
   `BODY_SIZE` matches, as DF does; read through `world.creature_raws`); a citizens checkbox;
   water and magma min/max 0..7; track min/max 1..2000 in steps of 50 (max "any"); a resets
   checkbox ("one-use plates destroy their mechanisms"). The live state shows "pressed / ready
   / closing in N ticks".
3. **Device sheet additions:** pending change ("raising in 37 ticks"), jammed (with the reason:
   item on tile), and the one-shot warning for cages, chains and supports.
4. **Power overlay polish** (`round12::draw_map_overlay` stays; new drawing in machines14.rs,
   called right after it, tagged in render.rs): one color per network (hash of the net id
   over a fixed 8-color palette), port links drawn as short lines between connected machines
   (from `power_networks`), a label at each network's first machine with "+P / -U" (green when
   powered), disengaged gears drawn with an X, pumps with an arrow from intake to output,
   water wheels marked "no flow" when inactive. Keep the topbar hint.
5. **Minecart route polish** (draws only, no route logic): moving carts get a velocity arrow
   whose length is the speed, track stops a friction label (H, M, L ...) and their dump arrow,
   powered rollers an arrow (green when powered, grey when not), track plates a "T" marker
   with their weight range, devices that block carts (closed doors and the like) a red
   outline while the route view is open.
6. **Sprites:** lever `LEVER_PULLED` when `ACTIVE`, else `LEVER_SET`. Plates
   `TRAP_PLATE_PRESSED*` when on, `TRAP_PLATE_READY*` otherwise, with the `_WATER`, `_MAGMA`
   and `_MINECART` variants by enabled trigger (all in `vanilla_buildings_graphics`). These are
   tagged lines in `rf-gfx/src/terrain.rs` around line 465 and line 1465.
7. `--ui` keys for screenshots: `m14-link` (picker open on the first lever), `m14-plate`,
   `m14-power`. Screenshots go under `target/` only.

The app tests stay headless: `rustfort play --screenshot target/m14_*.png --frames N --ui
...`, muted.

---

## 14. Tests (rf-sim unless noted; synthetic worlds, as in `power::tests` and `defense` tests)

Signals and devices (`linkage::tests`):

1. `lever_on_opens_linked_door_at_once`: a linked door is closed (blocked); pull the lever
   (`pull_lever_now`); the door is passable **in the same tick** after `linkage_tick`; pull
   again and it is blocked.
2. `imported_closed_linked_door_opens_on_first_pull`: building flags `CLOSED`,
   `OPERATED_BY_MECHANISMS`, lever not `ACTIVE`; one pull opens it (regression test for F1).
3. `bridge_raises_after_100_and_ignores_signals_meanwhile`: on at t, off at t+10; raised at
   t+100, then stays raised (refractory), and lowers only after a new off.
4. `same_tick_on_and_off_toggles`: two levers linked to one floodgate, pulled in the same
   tick in opposite states; the gate changes state once.
5. `spikes_retract_on_and_extend_off_after_40`.
6. `gear_toggles_on_every_signal_and_cuts_power`: windmill -> gear (linked) -> pump; the pump is
   powered; signal on means unpowered after the next power tick; signal off (also a toggle)
   means powered again.
7. `track_stop_disabled_by_lever_lets_cart_roll_through`: a cart pushed onto a highest-friction
   stop stops there; after the lever is on, it passes; back on 40 ticks after off.
8. `cage_lever_releases_occupant_and_deconstructs` and `support_lever_causes_cavein` (uses
   `caveins` test helpers).
9. `floodgate_jams_on_item_and_closes_when_cleared`, `door_closing_destroys_water`.
10. `bridge_does_not_lower_onto_huge_creature` (size 1 300 000 means the bridge deconstructs).

Plates (`linkage::plate_tests`):

11. `plate_holds_while_occupied_and_closes_99_after`: a goblin stands 300 ticks; exactly one
    "on", "off" at leave + 99.
12. `plate_weight_range`: a dwarf (60 000) does not trigger a 100 000..2 000 000 plate (with
    citizens on); a troll does.
13. `citizens_flag_and_merchants`: citizens only with the flag; merchants never; an
    unconscious citizen always.
14. `water_plate_range`: depths 0..7 against min 2 max 5, using `set_liquid`.
15. `minecart_plate_weight`: an empty oak cart (28 Γ) against a track range 50..2000 does not
    trigger; a loaded one does; a fast cart (v 60 000) passing still triggers.
16. `one_use_plate_deconstructs_and_destroys_mechanisms`.

Linking job:

17. `mechanic_links_with_two_mechanisms`: 2 TRAPPARTS in the world, a mechanic, link queued;
    after the run both are inside the buildings, the link exists, and no signal was sent.
18. `link_waits_without_mechanisms` and `locked_door_cannot_be_linked` (job suspended).

Power:

19. `pump_stack_connects_vertically`: two pumps stacked with open space between them; one
    network.
20. `windmill_powers_through_side_ring` and `water_wheel_center_side_port`.
21. `long_roller_connects_only_across`.
22. `wheel_needs_four_sevenths`: 3/7 flowing makes nothing, 4/7 makes 100.
23. `pump_runs_49_ticks_after_power_loss`, `pumps_run_newest_first`,
    `pump_water_onto_magma_makes_obsidian`, `pumped_salt_water_is_clean`.
24. `floor_between_levels_blocks_vertical_power`.
25. `ui_built_machines_get_constructed`: place a gear assembly with a mechanism in stock and a
    mechanic; `built` becomes true without forcing it (F9).
26. minecarts: `exported_trap_trackstop_brakes` (kind `Trap`, subtype `TrackStop`,
    `friction: 500`) (F10).

Saves and determinism: extend `tests/saves11.rs` (or a new `tests/machines14.rs`) with a fort
that has a lever pulled with a bridge pending, a plate held with its close timer running, and a
link job half done. Save at that moment, load, continue 500 ticks; the hash must equal the
uninterrupted run. Run `determinism_stress` with plates and pumps (1 thread vs N).

---

## 15. Performance notes

- `linkage_tick` fast path: three `is_empty()` checks when the fort has no linked devices or
  plates. The reference fort has levers and links, so `devs` is non-empty but `inbox`,
  `timers` and the active-plate list are empty almost always. The common per-tick cost is
  three length checks.
- Plates: only plates with occupants, liquid triggers or carts are polled. Liquid checks are
  O(1). Occupant sums go over `occ` (usually 0-3 slots). The 100-tick resync is one O(units)
  pass over `ai` with a bitset test per unit (`def.trap_tile`).
- Signals: `inbox` and `scratch` are reused `Vec`s (clear, never reallocate); `timers` is a
  sorted Vec (few entries). No HashMap anywhere in linkage (`devs` sorted by bid, binary
  search). `defense` keeps its own HashMaps; linkage never iterates them.
- Power: gear toggles trigger at most one `power_rebuild` per tick (O(M log M) with M machines;
  M < 200 in big forts). Pump order is precomputed. The `run_until` check is O(1) per pump.
- Carts: one bit test per tile entered for the device masks. The masks are rebuilt only on
  device changes.
- Bench before and after: `target/release/rustfort.exe bench --ticks 806400 --standdown`;
  expect less than 1 % (no new per-unit work on the hot path besides the existing
  `trap_tile` test in `move_to`).

---

## 16. Prioritized implementation list

**P1: correctness of what forts already use (levers, doors, bridges, plates)**

1. `linkage.rs` skeleton: `Linkage`, `Device`, sending and receiving, per-kind delays,
   refractory, same-tick toggle, phase after `defense_tick`, save, hash and after-load wiring.
   Route `lever_pulled` and the plate edges through it. **Fixes F1-F4.** Tests 1-5.
2. Pressure plates as level-triggered sensors with 99-tick close, weight, citizens, merchants,
   unconscious, water and magma, and one-use. rf-core `PlateInfo`. **Fixes F5.** Tests 11-14,
   16.
3. Track stop recognition fix (`is_track_stop`, export `friction`) (F10, two tagged lines in
   minecarts.rs, coordinate with `[perf13]`). Test 26.

**P2: the machine features the topic asks for**

4. Gear assembly toggle (power ports, signature, `DISENGAGED`), and gears no longer blocking
   movement (F6). Test 6.
5. Linking job with mechanisms (`Task::Link`, Mechanic labor) (F7). Tests 17-18.
6. More linkable targets: grates, bars, track stops (disable/enable), cages, chains, supports
   (F8). Tests 7-8.
7. Port fixes: pump below, windmill ring, wheel center side, long rollers across only,
   floors block vertical power (F11). Tests 19-21, 24.
8. Water wheel 4/7, gradient flow and persistent edge-drain mark (F12). Test 22.
9. Construction of machines and devices plus the missing build options (F9). Test 25.
10. UI: target picker, plate settings, device state, lever and plate sprites (§13.1-3, 6).
11. Minecart-device interactions: plates on track, doors and raised bridges as walls, lowered
    bridges as crossings, rollers only on track (F16). Test 15.

**P3: details**

12. Screw pump details: 49-tick run-on, newest first, obsidian, clean water, intake column,
    output tile blocking, operator tile (F13). Test 23.
13. Bridge details: huge creatures, fluids, items flung or dropped (F14). Jamming and liquid
    destruction for doors and floodgates (F15). Tests 9-10.
14. Power overlay and route view polish (§13.4-5); `--ui` screenshot keys.
15. Machine stability (hanging machines collapse when their support is removed or
    disengaged); windmill per-fort wind; millstone job cancel on power loss; lever owner and
    repeat; frozen hook for `[weather13]`.
16. Exporter additions (§11) with EXPORT_FORMAT rows. Later: the flowing-water designation
    after checking it in DF.

---

## 17. FIDELITY.md section to add at the end (draft rows)

`## Machines and mechanisms (round 14, [machines14], linkage.rs)`, with rows for:
signal semantics (on/off, gear toggle), per-device delays (0/40/100), refractory, same tick,
plate triggers and 99-tick close, one-use plates, link job (2 mechanisms, Mechanic,
`LINK_WORK` ESTIMATE), lever pull (`PULL_TICKS` ESTIMATE), power ports (pump below, windmill
ring, wheel center), wheel 4/7 and flow, pump run-on 49, pump order, obsidian, track stop
toggle, bridge size limit 1 200 000, jamming, cart-device interactions. Mark every ESTIMATE
from this file and leave the ones not implemented as MISSING.
