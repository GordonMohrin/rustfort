//! Fort defense (round 4): doors, hatches, floodgates, bridges, levers, pressure plates,
//! traps, upright spikes and building destroyers.
//!
//! # DF rules implemented (wiki v50: Door, Hatch, Floodgate, Bridge, Lever, Mechanism,
//! Pressure plate, Trap, Cage trap, Stone-fall trap, Weapon trap, Upright spike,
//! Building destroyer, Atom smasher)
//! - **Doors** are passable for everybody while unlocked (a closed door opens for whoever walks
//!   through). A **locked/forbidden** door (`FORBIDDEN`/`LOCKED`, DF `door_flags.forbidden`)
//!   and a door **closed by a linked lever** block *every* creature: citizens path around it,
//!   and so do invaders and wild animals, which is why steering invaders with locked doors
//!   into a trap corridor works. Only building destroyers get through, by smashing the door.
//!   `PET_PASSABLE` lets tame animals through a locked door (DF: "used with forbidden").
//! - **Hatches** give a floor over their tile; locked or lever-closed hatches block movement
//!   through them (stairs/ramps below), but can be walked over.
//! - **Floodgates** block movement (and fluids) while closed; a linked lever opens them.
//! - **Bridges**: a lowered bridge is a floor (also over a moat or chasm). A raised drawbridge
//!   stands as a wall on its raising edge (the row/column in its direction; the whole bridge
//!   for a one-tile-wide one); the rest of its area is whatever is underneath. Raising flings
//!   whoever stands on it off the bridge; **lowering atom-smashes** every creature and item on
//!   its area (no corpse). A retracting bridge simply leaves a gap: whoever stands on it falls.
//! - **Levers** and **pressure plates** send an on/off signal to all linked buildings: "on"
//!   closes and locks doors/hatches, raises (retracts) bridges, opens floodgates and extends
//!   upright spikes; "off" does the reverse. Pulling a lever is a labor-free job ("Pull the
//!   Lever"): an idle citizen who can reach it walks there and pulls it.
//! - **Traps** are triggered by hostile creatures and wild animals, never by citizens, tame
//!   animals, merchants or visitors, and never by `TRAPAVOID` creatures (kobolds, ...).
//!   - Cage trap (needs an empty cage loaded): the creature is caught in the cage and out of
//!     the fight; the trap must be re-armed with a new cage. Re-arming cage and stone-fall
//!     traps is a Mechanic job: an idle mechanic fetches a free cage (stone) and loads it
//!     (not while enemies are within 20 tiles of the trap).
//!   - Stone-fall trap (needs a stone loaded): the stone falls on the creature and stays on the
//!     ground; single use.
//!   - Weapon trap: every weapon/trap component inside attacks (trap components `HITS` times,
//!     vanilla raws), resolved by the combat module (dodge, armor, layered wounds); reusable.
//!     Half of the kills by the trap itself jam it (wiki: "a 50% chance that a kill will result in
//!     the victim getting stuck in the mechanism"); a jammed trap does nothing until a mechanic
//!     cleans it ("Clean Trap", not while enemies are within 20 tiles).
//!   - Pressure plate: sends "on" when triggered and "off" when it resets.
//! - **Upright spikes** (building `Weapon`) attack everybody on their tile when extended.
//! - **Building destroyers** (`BUILDINGDESTROYER` in the vanilla raws: trolls, ogres, giants,
//!   dragons, cave crocodiles, ...) that cannot reach anybody walk to the nearest locked door
//!   or hatch (level 2 also floodgates) bordering their area and smash it. Raised bridges and
//!   walls are immune.
//!
//! # Estimates (not on the wiki)
//! - Mechanism reaction delay 100 ticks (lever/plate -> linked building), plate reset 100
//!   ticks, pulling a lever 10 ticks.
//! - Trap attacks: as if wielded by a 60 000 cm³ creature with STRENGTH 1250 and skill 10.
//! - Falling stone: a blunt 1500 cm³ stone of the boulder's material, contact area 400,
//!   velocity x2.
//! - Smashing a door takes 300 ticks (level 1) or 150 ticks (level 2).
//! - Flung units are stunned 30 ticks; falls of 2+ z-levels knock out (100 ticks per level),
//!   10+ levels kill.
//!
//! # Gaps
//! - Caught creatures stay in their cage on the trap tile (the occupied cage is not hauled to
//!   an animal stockpile). Weapon traps are never reloaded (they need no ammunition). Linking
//!   needs no mechanisms or mechanic job (instant).
//! - Pets do not path through pet-passable locked doors (they only wander through them).
//! - Lever-linked cage traps, gear assemblies, grates and bars are not operated.

use crate::combat::{AtkT, Role, WeapT, SIGHT};
use crate::path::Goals;
use crate::rng::Rng; // [audit-combat]
use crate::{AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::{Pos, Skill, Unit, UnitFlags};
use std::collections::HashMap;

/// Mechanism reaction delay (lever or pressure plate -> linked building), estimate.
pub const LINK_DELAY: u64 = 100;
/// Ticks until a pressure plate resets and sends "off" (estimate).
pub const PLATE_RESET: u64 = 100;
/// Ticks a dwarf needs to pull a lever (estimate).
pub const PULL_TICKS: u64 = 10;
const TRAP_SKILL: i32 = 10;
const TRAP_SIZE: i32 = 60_000;
const TRAP_STRENGTH: i32 = 1250;
/// Marks the hidden unit that "wields" trap weapons (kept across save/load).
const TRAP_MARK: &str = "trap mechanism";
const STONE_TOKEN: &str = "RF_FALLING_STONE";

/// `BUILDINGDESTROYER` levels of the vanilla creature raws (DF 53, data/vanilla).
const DESTROYERS: &[(&str, u8)] = &[
    ("VORACIOUS_CAVE_CRAWLER", 2),
    ("BLIND_CAVE_OGRE", 2),
    ("CAVE_DRAGON", 2),
    ("ELEMENTMAN_GABBRO", 2),
    ("ELEMENTMAN_AMETHYST", 2),
    ("TROLL", 2),
    ("OGRE", 2),
    ("DRAGON", 2),
    ("COLOSSUS_BRONZE", 2),
    ("GIANT", 2),
    ("CYCLOPS", 2),
    ("ETTIN", 2),
    ("MINOTAUR", 2),
    ("YETI", 2),
    ("SASQUATCH", 2),
    ("HYDRA", 2),
    ("CROCODILE_CAVE", 1),
    ("TOAD_GIANT_CAVE", 1),
    ("OLM_GIANT", 1),
    ("SPIDER_CAVE_GIANT", 1),
    ("ELEMENTMAN_FIRE", 2),
    ("ELEMENTMAN_MAGMA", 2),
    ("ELEMENTMAN_IRON", 2),
    ("ELEMENTMAN_MUD", 2),
];

/// Vanilla trap components (item_trapcomp.txt): token, name, verb, contact, penetration, size, hits.
const TRAPCOMPS: &[(&str, &str, &str, i32, i32, i32, u32)] = &[
    ("ITEM_TRAPCOMP_GIANTAXEBLADE", "axe blade", "slash", 100_000, 10_000, 1600, 1),
    ("ITEM_TRAPCOMP_ENORMOUSCORKSCREW", "corkscrew", "drill", 100, 10_000, 1600, 1),
    ("ITEM_TRAPCOMP_SPIKEDBALL", "ball", "strike", 10, 200, 1000, 3),
    ("ITEM_TRAPCOMP_LARGESERRATEDDISC", "disc", "saw", 100_000, 10_000, 1000, 3),
    ("ITEM_TRAPCOMP_MENACINGSPIKE", "spike", "stab", 10, 6000, 1600, 1),
];

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub enum MechKind {
    Door,
    Hatch,
    Floodgate,
    Bridge,
    Lever,
    PressurePlate,
    CageTrap,
    StoneFallTrap,
    WeaponTrap,
    UprightSpikes,
}

impl MechKind {
    fn of(kind: &str, subtype: &str) -> Option<MechKind> {
        Some(match (kind, subtype) {
            ("Door", _) => MechKind::Door,
            ("Hatch", _) => MechKind::Hatch,
            ("Floodgate", _) => MechKind::Floodgate,
            ("Bridge", _) => MechKind::Bridge,
            ("Weapon", _) => MechKind::UprightSpikes,
            ("Trap", "Lever") => MechKind::Lever,
            ("Trap", "PressurePlate") => MechKind::PressurePlate,
            ("Trap", "CageTrap") => MechKind::CageTrap,
            ("Trap", "StoneFallTrap") => MechKind::StoneFallTrap,
            ("Trap", "WeaponTrap") => MechKind::WeaponTrap,
            _ => return None,
        })
    }
    fn is_trigger(self) -> bool {
        matches!(self, MechKind::Lever | MechKind::PressurePlate)
    }
    fn is_trap(self) -> bool {
        matches!(self, MechKind::PressurePlate | MechKind::CageTrap | MechKind::StoneFallTrap | MechKind::WeaponTrap)
    }
    fn linkable(self) -> bool {
        matches!(self, MechKind::Door | MechKind::Hatch | MechKind::Floodgate | MechKind::Bridge | MechKind::UprightSpikes)
    }
    fn noun(self) -> &'static str {
        match self {
            MechKind::Door => "door",
            MechKind::Hatch => "hatch",
            MechKind::Floodgate => "floodgate",
            MechKind::Bridge => "bridge",
            MechKind::Lever => "lever",
            MechKind::PressurePlate => "pressure plate",
            MechKind::CageTrap => "cage trap",
            MechKind::StoneFallTrap => "stone-fall trap",
            MechKind::WeaponTrap => "weapon trap",
            MechKind::UprightSpikes => "upright spikes",
        }
    }
}

/// Wanted navigation state of one building tile: (blocked, floor provided, no vertical moves).
type NavState = (bool, bool, bool);

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)] // [saves11]
pub(crate) struct Mech {
    pub id: i32,
    pub kind: MechKind,
    pub tiles: Vec<u32>,
    /// Bridges: tiles that stand as a wall while raised.
    pub edge: Vec<u32>,
    /// Doors/hatches: locked by the player (DF "forbid passage").
    pub forbid: bool,
    pub pet_passable: bool,
    /// Signal state: lever pulled, door/hatch closed by mechanism, bridge raised/retracted,
    /// floodgate open, spikes extended, plate pressed.
    pub on: bool,
    pub retract: bool,
    pub plate_citizens: bool,
    pub plate_until: u64,
    /// Levers and plates: linked target building ids.
    pub targets: Vec<i32>,
    /// [audit-combat] Weapon traps: jammed by a victim stuck in the mechanism (wiki: 50 % per
    /// kill) until a mechanic cleans it ("Clean Trap").
    pub jammed: bool,
    applied: Vec<NavState>,
}

/// A queued "Pull the Lever" job.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct LeverJob {
    pub lever: i32,
    pub claimed: u32,
    pub retry_at: u64,
    pub working_until: u64,
}

/// A trap waiting to be re-armed by a mechanic (DF jobs "Load Cage Trap", "Load Stone Trap").
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct ReloadJob {
    pub trap: i32,
    pub claimed: u32,
    pub item: i32,
    pub retry_at: u64,
}

/// Counters for reports and tests.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct DefenseStats {
    /// Creatures caught in cage traps.
    pub caught: u64,
    pub stones_dropped: u64,
    pub weapon_trap_triggers: u64,
    /// Single trap weapon attacks (weapon traps and upright spikes, incl. falling stones).
    pub trap_attacks: u64,
    /// Creatures killed by traps outright (weapons, stones, spikes).
    pub trap_kills: u64,
    /// Creatures hit by trap weapons at least once, and how many of them are dead by now
    /// (bled out or finished off later).
    pub trap_victims: u64,
    pub trap_victims_dead: u64,
    /// Creatures that set off a trap but were not affected (empty trap, dodged, ...).
    pub plates_triggered: u64,
    pub levers_pulled: u64,
    pub signals: u64,
    /// Cage and stone-fall traps re-armed by mechanics.
    pub traps_reloaded: u64,
    pub crushed: u64,
    pub items_crushed: u64,
    pub flung: u64,
    pub fell: u64,
    pub buildings_destroyed: u64,
}

#[derive(serde::Serialize, serde::Deserialize)] // [saves11]
pub(crate) struct Defense {
    pub mechs: Vec<Mech>,
    pub by_id: HashMap<i32, usize>,
    /// Bit set of tiles with a trap or pressure plate (checked on every step).
    bits: Vec<u64>,
    pub any_traps: bool,
    trap_at: HashMap<u32, Vec<u32>>,
    door_at: HashMap<u32, u32>,
    /// (unit slot, tile) entered a trap tile this tick.
    pub pending: Vec<(u32, u32)>,
    /// Delayed signals: (due tick, target building id, on).
    signals: Vec<(u64, i32, bool)>,
    pub lever_jobs: Vec<LeverJob>,
    pub reload_jobs: Vec<ReloadJob>,
    /// Hidden unit slot that wields trap weapons, `usize::MAX` until needed.
    pub trap_unit: usize,
    /// Weapon the trap unit attacks with right now (item index, combat weapon template).
    pub trap_weapon: Option<(usize, u16)>,
    /// Per race: (TRAPAVOID, BUILDINGDESTROYER level).
    races: HashMap<String, (bool, u8)>,
    /// Building destroyers at work: (slot, building id, done at).
    smash: Vec<(u32, i32, u64)>,
    /// Cage item id -> caught unit id.
    occupied: HashMap<i32, i32>,
    /// Trap attacks so far (gives each attack its own random stream).
    attack_seq: u64,
    /// Units hit by a trap weapon (ids, for the report).
    trap_victims: Vec<i32>,
    /// Mechanism kinds switched off (`RF_DEFENSE_OFF`).
    skip: Vec<String>,
    nbuildings: usize,
    pub stats: DefenseStats,
    /// [war9] Buildings destroyers may wreck: (building id, center tile, z, x1, y1, x2, y2,
    /// minimum destroyer level), rebuilt when the building count changes.
    wreck_list: Vec<(i32, u32, i16, i16, i16, i16, i16, u8)>,
    wreck_n: usize,
}

impl Default for Defense {
    fn default() -> Self {
        Defense {
            mechs: Vec::new(),
            by_id: HashMap::new(),
            bits: Vec::new(),
            any_traps: false,
            trap_at: HashMap::new(),
            door_at: HashMap::new(),
            pending: Vec::new(),
            signals: Vec::new(),
            lever_jobs: Vec::new(),
            reload_jobs: Vec::new(),
            trap_unit: usize::MAX,
            trap_weapon: None,
            races: HashMap::new(),
            smash: Vec::new(),
            occupied: HashMap::new(),
            attack_seq: 0,
            trap_victims: Vec::new(),
            skip: Vec::new(),
            nbuildings: usize::MAX,
            stats: DefenseStats::default(),
            wreck_list: Vec::new(),
            wreck_n: usize::MAX,
        }
    }
}

impl Defense {
    /// Called on every step (hot): does tile `q` hold a trap or pressure plate?
    #[inline]
    pub fn trap_tile(&self, q: u32) -> bool {
        self.any_traps && self.bits[q as usize / 64] & (1 << (q % 64)) != 0
    }
}

fn has(flags: &[String], f: &str) -> bool {
    flags.iter().any(|x| x.eq_ignore_ascii_case(f))
}

fn set_flag(flags: &mut Vec<String>, f: &str, on: bool) {
    flags.retain(|x| !x.eq_ignore_ascii_case(f));
    if on {
        flags.push(f.to_string());
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn verb3(v: &str) -> String {
    if v.ends_with("ch") || v.ends_with("sh") || v.ends_with('s') || v.ends_with('x') {
        format!("{v}es")
    } else {
        format!("{v}s")
    }
}

fn trapcomp_hits(token: &str) -> u32 {
    TRAPCOMPS.iter().find(|t| t.0 == token).map(|t| t.6).unwrap_or(1)
}

impl Sim {
    // ------------------------------------------------------------------ init and scanning

    pub(crate) fn init_defense(&mut self) {
        // Debug/comparison knob: `RF_DEFENSE_OFF=all` (or e.g. `CageTrap,WeaponTrap`) runs the
        // fort without these mechanisms, i.e. like v0.3.
        if let Ok(v) = std::env::var("RF_DEFENSE_OFF") {
            self.def.skip = v.split(',').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect();
        }
        self.defense_scan();
        // Caught creatures of the export sit in the cage of the trap they stand on.
        for mi in 0..self.def.mechs.len() {
            if self.def.mechs[mi].kind != MechKind::CageTrap {
                continue;
            }
            let tile = self.def.mechs[mi].tiles[0];
            let p = self.world.pos_of(tile as usize);
            let Some(caught) = self.world.units.iter().find(|u| u.flags.caged && !u.flags.dead && u.pos == p).map(|u| u.id) else { continue };
            if let Some(cage) = self.trap_items(mi, &["CAGE"]).first().map(|&i| self.world.items[i].id) {
                self.def.occupied.insert(cage, caught);
            }
        }
    }

    /// Register new mechanisms, forget removed ones and apply their states to the nav grid.
    pub(crate) fn defense_scan(&mut self) {
        self.def.nbuildings = self.world.buildings.len();
        let flagged = self.world.buildings.iter().any(|b| !b.flags.is_empty());
        let mut seen = vec![false; self.def.mechs.len()];
        let mut fresh: Vec<Mech> = Vec::new();
        for b in &self.world.buildings {
            let Some(kind) = MechKind::of(&b.kind, &b.subtype) else { continue };
            if self.def.skip.iter().any(|k| k == "all" || *k == format!("{kind:?}")) {
                continue;
            }
            if !b.built {
                continue;
            }
            if let Some(&m) = self.def.by_id.get(&b.id) {
                seen[m] = true;
                continue;
            }
            let mut tiles = Vec::new();
            for y in b.y1..=b.y2 {
                for x in b.x1..=b.x2 {
                    let p = Pos::new(x, y, b.z);
                    if b.contains(p) {
                        if let Some(i) = self.world.idx(p) {
                            tiles.push(i as u32);
                        }
                    }
                }
            }
            if tiles.is_empty() {
                continue;
            }
            let f = &b.flags;
            let dir = b.direction.to_ascii_uppercase();
            let retract = dir.starts_with("RETRACT") || b.subtype.to_ascii_uppercase().contains("RETRACT");
            let edge: Vec<u32> = if kind != MechKind::Bridge || retract {
                Vec::new()
            } else {
                let (wx, wy) = (b.x2 - b.x1 + 1, b.y2 - b.y1 + 1);
                tiles
                    .iter()
                    .copied()
                    .filter(|&t| {
                        let p = self.world.pos_of(t as usize);
                        match dir.as_str() {
                            "N" | "UP" | "NORTH" if wy > 1 => p.y == b.y1,
                            "S" | "DOWN" | "SOUTH" if wy > 1 => p.y == b.y2,
                            "W" | "LEFT" | "WEST" if wx > 1 => p.x == b.x1,
                            "E" | "RIGHT" | "EAST" if wx > 1 => p.x == b.x2,
                            _ => true,
                        }
                    })
                    .collect()
            };
            let linked = !b.links.is_empty();
            let on = match kind {
                // A lever-linked door that is closed is locked by the mechanism.
                MechKind::Door | MechKind::Hatch => {
                    (has(f, "CLOSED") && (has(f, "OPERATED_BY_MECHANISMS") || linked)) || (linked && has(f, "LOCKED"))
                }
                MechKind::Bridge => has(f, "RAISED") || has(f, "CLOSED") || has(f, "RETRACTED"),
                // Floodgates are closed unless the export says otherwise.
                MechKind::Floodgate => has(f, "OPEN") || (flagged && !f.is_empty() && !has(f, "CLOSED")),
                MechKind::Lever | MechKind::UprightSpikes => has(f, "ACTIVE") || has(f, "ON") || has(f, "PULLED") || has(f, "EXTENDED"),
                _ => false,
            };
            let forbid = matches!(kind, MechKind::Door | MechKind::Hatch)
                && (has(f, "FORBIDDEN") || has(f, "FORBID") || (has(f, "LOCKED") && !linked));
            let applied = vec![if kind == MechKind::Floodgate { (true, false, false) } else { (false, false, false) }; tiles.len()];
            fresh.push(Mech {
                id: b.id,
                kind,
                tiles,
                edge,
                forbid,
                pet_passable: has(f, "PET_PASSABLE"),
                on,
                retract,
                plate_citizens: has(f, "CITIZENS"),
                plate_until: 0,
                targets: if kind.is_trigger() { b.links.clone() } else { Vec::new() },
                jammed: has(f, "JAMMED"),
                applied,
            });
        }
        // Removed buildings: give their tiles back.
        let mut keep = Vec::new();
        let old = std::mem::take(&mut self.def.mechs);
        let mut gone = Vec::new();
        for (k, m) in old.into_iter().enumerate() {
            if seen[k] {
                keep.push(m);
            } else {
                gone.push(m);
            }
        }
        self.def.mechs = keep;
        let first_new = self.def.mechs.len();
        self.def.mechs.extend(fresh);
        self.reindex_mechs();
        let mut changed = false;
        for m in gone {
            for (k, &t) in m.tiles.iter().enumerate() {
                if m.applied[k] != (false, false, false) {
                    self.set_tile_nav(t, (false, false, false));
                    changed = true;
                }
            }
        }
        for mi in first_new..self.def.mechs.len() {
            changed |= self.apply_mech_nav(mi);
        }
        if changed {
            self.settle_nav();
        }
    }

    fn reindex_mechs(&mut self) {
        let d = &mut self.def;
        d.by_id.clear();
        d.trap_at.clear();
        d.door_at.clear();
        if d.bits.is_empty() {
            d.bits = vec![0u64; self.world.tiles.len() / 64 + 1];
        } else {
            d.bits.iter_mut().for_each(|b| *b = 0);
        }
        d.any_traps = false;
        for (mi, m) in d.mechs.iter().enumerate() {
            d.by_id.insert(m.id, mi);
            if m.kind.is_trap() {
                for &t in &m.tiles {
                    d.bits[t as usize / 64] |= 1 << (t % 64);
                    d.trap_at.entry(t).or_default().push(mi as u32);
                    d.any_traps = true;
                }
            }
            if matches!(m.kind, MechKind::Door | MechKind::Hatch) {
                for &t in &m.tiles {
                    d.door_at.insert(t, mi as u32);
                }
            }
        }
    }

    fn mech_state(&self, mi: usize, t: u32) -> NavState {
        let m = &self.def.mechs[mi];
        match m.kind {
            MechKind::Door => (m.forbid || m.on, false, false),
            MechKind::Hatch => (false, true, m.forbid || m.on),
            MechKind::Floodgate => (!m.on, false, false),
            MechKind::Bridge => {
                if !m.on {
                    (false, true, false)
                } else if m.retract {
                    (false, false, false)
                } else {
                    (m.edge.contains(&t), false, false)
                }
            }
            _ => (false, false, false),
        }
    }

    fn set_tile_nav(&mut self, t: u32, s: NavState) {
        let i = t as usize;
        self.nav.set_blocked(i, s.0);
        self.nav.set_floor_override(i, s.1);
        self.nav.set_novert(i, s.2);
        self.after_tile_change(i);
    }

    /// Bring the nav grid in line with a mechanism's state. True if something changed.
    fn apply_mech_nav(&mut self, mi: usize) -> bool {
        let mut changed = false;
        for k in 0..self.def.mechs[mi].tiles.len() {
            let t = self.def.mechs[mi].tiles[k];
            let want = self.mech_state(mi, t);
            if want != self.def.mechs[mi].applied[k] {
                self.def.mechs[mi].applied[k] = want;
                self.set_tile_nav(t, want);
                changed = true;
            }
        }
        changed
    }

    /// After blocking changes: relabel walk components right away (rare event; keeps "can
    /// reach" queries exact instead of failing expensive searches first).
    fn settle_nav(&mut self) {
        if self.nav.suspect {
            self.nav.rebuild_components();
            self.path_cache.clear();
            self.epoch_jobs += 1;
            self.epoch_haul += 1;
        }
    }

    // ------------------------------------------------------------------ tick

    pub(crate) fn defense_tick(&mut self, now: u64) {
        if self.world.buildings.len() != self.def.nbuildings || now % rf_core::TICKS_PER_DAY as u64 == 7 {
            self.defense_scan();
        }
        if !self.def.pending.is_empty() {
            let pending = std::mem::take(&mut self.def.pending);
            for &(s, t) in &pending {
                self.trap_entered(s as usize, t, now);
            }
        }
        if !self.def.signals.is_empty() && self.def.signals.iter().any(|x| x.0 <= now) {
            let sig = std::mem::take(&mut self.def.signals);
            let (due, later): (Vec<_>, Vec<_>) = sig.into_iter().partition(|x| x.0 <= now);
            self.def.signals = later;
            for (_, target, on) in due {
                self.apply_signal(target, on, now);
            }
        }
        if now % 600 == 300 {
            self.post_reload_jobs();
        }
        if now % 10 == 0 {
            for mi in 0..self.def.mechs.len() {
                let m = &self.def.mechs[mi];
                if m.kind == MechKind::PressurePlate && m.on && now >= m.plate_until {
                    self.def.mechs[mi].on = false;
                    self.send_signals(mi, false, now);
                }
            }
        }
    }

    fn send_signals(&mut self, mi: usize, on: bool, now: u64) {
        let targets = self.def.mechs[mi].targets.clone();
        for t in targets {
            self.def.signals.push((now + LINK_DELAY, t, on));
        }
        let id = self.def.mechs[mi].id;
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == id) {
            set_flag(&mut b.flags, "ACTIVE", on);
        }
    }

    /// A linked building receives an on/off signal.
    fn apply_signal(&mut self, target: i32, on: bool, now: u64) {
        let Some(&mi) = self.def.by_id.get(&target) else { return };
        self.def.stats.signals += 1;
        let kind = self.def.mechs[mi].kind;
        if !kind.linkable() || self.def.mechs[mi].on == on {
            return;
        }
        let m = &self.def.mechs[mi];
        let (retract, tiles) = (m.retract, m.tiles.clone());
        if kind == MechKind::Bridge && on && !retract {
            self.fling_from(&tiles, now);
        }
        if kind == MechKind::Bridge && !on && !retract {
            self.crush(&tiles, now);
        }
        self.def.mechs[mi].on = on;
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == target) {
            match kind {
                MechKind::Door | MechKind::Hatch => {
                    set_flag(&mut b.flags, "CLOSED", on);
                    set_flag(&mut b.flags, "OPERATED_BY_MECHANISMS", true);
                }
                MechKind::Bridge => {
                    set_flag(&mut b.flags, "CLOSED", false);
                    set_flag(&mut b.flags, "RAISED", on);
                }
                MechKind::Floodgate => {
                    set_flag(&mut b.flags, "OPEN", on);
                    set_flag(&mut b.flags, "CLOSED", !on);
                }
                _ => set_flag(&mut b.flags, "ACTIVE", on),
            }
        }
        if self.apply_mech_nav(mi) {
            self.settle_nav();
        }
        if kind == MechKind::Bridge && on && retract {
            self.fall_from(&tiles, now);
        }
        if kind == MechKind::UprightSpikes && on {
            self.spikes_extend(mi, now);
        }
    }

    fn units_on(&self, tiles: &[u32]) -> Vec<usize> {
        (0..self.ai.len()).filter(|&s| self.ai[s].kind != Kind::Dead && tiles.contains(&self.ai[s].pos)).collect()
    }

    /// Move unit `s` to tile `q` right away (flung, fell, caught), dropping its plans.
    fn displace(&mut self, s: usize, q: u32) {
        self.cancel_task(s);
        let p = self.world.pos_of(q as usize);
        self.ai[s].pos = q;
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].pos = p;
    }

    fn fling_from(&mut self, tiles: &[u32], now: u64) {
        for s in self.units_on(tiles) {
            let from = self.ai[s].pos as usize;
            let (x, y, z) = self.nav.xyz(from);
            let mut dest = None;
            'r: for r in 1..=4isize {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs().max(dy.abs()) != r {
                            continue;
                        }
                        let (nx, ny) = (x as isize + dx, y as isize + dy);
                        if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                            continue;
                        }
                        let q = self.nav.idx(nx as usize, ny as usize, z) as u32;
                        if !tiles.contains(&q) && self.nav.walkable(q as usize) {
                            dest = Some(q);
                            break 'r;
                        }
                    }
                }
            }
            let Some(q) = dest else { continue };
            let name = self.uname(s);
            self.displace(s, q);
            self.combat_sync_units();
            let f = &mut self.combat.f[s];
            f.uncon_until = f.uncon_until.max(now + 30);
            self.ai[s].wake = now + 31;
            self.def.stats.flung += 1;
            self.combat.log_line(format!("{} is thrown off the rising bridge!", cap(&name)));
        }
    }

    /// DF "atom smasher": a lowering drawbridge obliterates everything on its area.
    fn crush(&mut self, tiles: &[u32], now: u64) {
        for s in self.units_on(tiles) {
            let name = self.uname(s);
            let tile = self.ai[s].pos;
            self.combat_sync_units();
            self.combat_kill(s, now, "has been crushed by a drawbridge");
            self.def.stats.crushed += 1;
            let p = self.world.pos_of(tile as usize);
            self.announce(now, format!("{} has been crushed by a drawbridge!", cap(&name)), Some(p), AnnouncementKind::Death);
        }
        for i in 0..self.world.items.len() {
            let it = &self.world.items[i];
            if self.imeta[i].dead || !it.flags.on_ground || it.building >= 0 || it.holder >= 0 || it.container >= 0 {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if tiles.contains(&(t as u32)) {
                let id = it.id;
                self.destroy_item(id);
                self.def.stats.items_crushed += 1;
            }
        }
    }

    /// Units over a retracted bridge drop to the next floor below.
    fn fall_from(&mut self, tiles: &[u32], now: u64) {
        for s in self.units_on(tiles) {
            let from = self.ai[s].pos as usize;
            if self.nav.walkable(from) {
                continue;
            }
            let (x, y, z) = self.nav.xyz(from);
            let mut land = None;
            for zz in (0..z).rev() {
                let q = self.nav.idx(x, y, zz);
                if self.nav.walkable(q) {
                    land = Some((q as u32, z - zz));
                    break;
                }
                if self.nav.flags[q] & crate::nav::SOLID != 0 {
                    break;
                }
            }
            let Some((q, levels)) = land else { continue };
            self.displace(s, q);
            self.def.stats.fell += 1;
            self.combat_sync_units();
            if levels >= 10 {
                self.combat_kill(s, now, "has died from a fall");
            } else if levels >= 2 {
                let f = &mut self.combat.f[s];
                f.uncon_until = f.uncon_until.max(now + 100 * levels as u64);
                self.ai[s].wake = now + 100 * levels as u64 + 1;
            } else {
                self.ai[s].wake = now + 1;
            }
        }
    }

    // ------------------------------------------------------------------ traps

    fn race_info(&mut self, race: &str) -> (bool, u8) {
        if let Some(&r) = self.def.races.get(race) {
            return r;
        }
        let raw = self.world.creature_raws.iter().find(|c| c.token == race);
        let flags = raw.map(|c| c.flags.iter().chain(c.castes.iter().flat_map(|k| k.flags.iter())).cloned().collect::<Vec<_>>()).unwrap_or_default();
        let trapavoid = flags.iter().any(|f| f == "TRAPAVOID") || (raw.is_none() && race == "KOBOLD");
        let mut lvl = DESTROYERS.iter().find(|d| d.0 == race).map(|d| d.1).unwrap_or(0);
        for f in &flags {
            if let Some(rest) = f.strip_prefix("BUILDINGDESTROYER") {
                lvl = lvl.max(rest.trim_start_matches([':', '_']).parse().unwrap_or(1));
            }
        }
        self.def.races.insert(race.to_string(), (trapavoid, lvl));
        (trapavoid, lvl)
    }

    /// Hostiles and wild animals set off traps; citizens, pets, merchants and visitors do not.
    fn sets_off_traps(&mut self, s: usize) -> bool {
        let ai = &self.ai[s];
        if matches!(ai.kind, Kind::Dead | Kind::Worker) {
            return false;
        }
        let u = &self.world.units[ai.uidx];
        let fl = &u.flags;
        if fl.citizen || fl.tame || fl.merchant || fl.visitor || fl.caged || fl.dead || fl.inactive {
            return false;
        }
        let hostile = fl.hostile || self.combat.f.get(s).map(|f| f.role == Role::Hostile).unwrap_or(false);
        if !(hostile || fl.animal) {
            return false;
        }
        let race = u.race.clone();
        !self.race_info(&race).0
    }

    /// Item indices of building `mi` of the given kinds (cages not occupied).
    fn trap_items(&self, mi: usize, kinds: &[&str]) -> Vec<usize> {
        let id = self.def.mechs[mi].id;
        let Some(b) = self.world.buildings.iter().find(|b| b.id == id) else { return Vec::new() };
        b.items
            .iter()
            .filter_map(|iid| self.item_idx.get(iid).map(|&i| i as usize))
            .filter(|&i| !self.imeta[i].dead && kinds.contains(&self.world.items[i].kind.as_str()) && !self.def.occupied.contains_key(&self.world.items[i].id))
            .collect()
    }

    fn trap_entered(&mut self, s: usize, tile: u32, now: u64) {
        if s >= self.ai.len() || self.ai[s].kind == Kind::Dead {
            return;
        }
        let Some(list) = self.def.trap_at.get(&tile).cloned() else { return };
        let trig = self.sets_off_traps(s);
        let citizen = self.ai[s].kind == Kind::Worker;
        for mi in list {
            let mi = mi as usize;
            if self.ai[s].kind == Kind::Dead {
                break;
            }
            match self.def.mechs[mi].kind {
                MechKind::PressurePlate => {
                    if trig || (citizen && self.def.mechs[mi].plate_citizens) {
                        self.def.stats.plates_triggered += 1;
                        let was = self.def.mechs[mi].on;
                        self.def.mechs[mi].on = true;
                        self.def.mechs[mi].plate_until = now + PLATE_RESET;
                        if !was {
                            self.send_signals(mi, true, now);
                        }
                    }
                }
                MechKind::CageTrap if trig => {
                    if let Some(&cage) = self.trap_items(mi, &["CAGE"]).first() {
                        self.cage_unit(s, mi, cage, tile, now);
                    }
                }
                MechKind::StoneFallTrap if trig => {
                    if let Some(&stone) = self.trap_items(mi, &["BOULDER", "ROCK"]).first() {
                        self.drop_stone(s, mi, stone, tile, now);
                    }
                }
                MechKind::WeaponTrap if trig => {
                    if self.def.mechs[mi].jammed {
                        continue; // [audit-combat] a jammed trap does nothing until cleaned
                    }
                    self.def.stats.weapon_trap_triggers += 1;
                    let weapons = self.trap_items(mi, &["WEAPON", "TRAPCOMP"]);
                    let mut struck = false;
                    for wi in weapons {
                        let hits = trapcomp_hits(&self.world.items[wi].subtype);
                        for _ in 0..hits {
                            if self.ai[s].kind == Kind::Dead {
                                break;
                            }
                            struck |= self.trap_attack(s, wi, None, "the weapon trap", tile, now);
                        }
                    }
                    // [audit-combat] Wiki Weapon trap: "a 50% chance that a kill will result in the
                    // victim getting stuck in the mechanism and cause the trap to jam" (only
                    // kills by the trap itself, not later bleeding out).
                    if struck && self.ai[s].kind == Kind::Dead {
                        let mut r = Rng::for_unit(self.seed, s, now, 0x7A11);
                        if r.chance(1, 2) {
                            self.jam_trap(mi, tile, now);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn cage_unit(&mut self, s: usize, mi: usize, cage: usize, tile: u32, now: u64) {
        let name = self.uname(s);
        self.combat_sync_units();
        self.carry_unlink(s, now); // [gaps10] a caged rider/snatcher lets go of its mount/child
        self.displace(s, tile);
        let uidx = self.ai[s].uidx;
        let uid = self.world.units[uidx].id;
        self.world.units[uidx].flags.caged = true;
        let ai = &mut self.ai[s];
        ai.kind = Kind::Dead;
        ai.wake = u64::MAX;
        let f = &mut self.combat.f[s];
        f.role = Role::None;
        f.target = NONE;
        if let Ok(p) = self.combat.hostiles.binary_search(&(s as u32)) {
            self.combat.hostiles.remove(p);
        }
        let cage_id = self.world.items[cage].id;
        self.def.occupied.insert(cage_id, uid);
        let race = self.world.units[uidx].race.to_ascii_lowercase();
        let it = &mut self.world.items[cage];
        if !it.desc.contains('(') {
            it.desc = format!("{} ({})", it.desc, race.replace('_', " "));
        }
        self.def.stats.caught += 1;
        let p = self.world.pos_of(tile as usize);
        let what = self.def.mechs[mi].kind.noun();
        self.announce(now, format!("{} has been caught in a {what}!", cap(&name)), Some(p), AnnouncementKind::Warning);
        self.combat.log_line(format!("{} has been caught in a {what}!", cap(&name)));
    }

    fn drop_stone(&mut self, s: usize, mi: usize, stone: usize, tile: u32, now: u64) {
        self.def.stats.stones_dropped += 1;
        let tmpl = self.stone_template();
        self.trap_attack(s, stone, Some(tmpl), "the stone-fall trap", tile, now);
        // The stone stays on the ground (to be hauled and loaded again).
        let id = self.def.mechs[mi].id;
        let sid = self.world.items[stone].id;
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == id) {
            b.items.retain(|&x| x != sid);
        }
        let p = self.world.pos_of(tile as usize);
        let it = &mut self.world.items[stone];
        it.building = -1;
        it.pos = p;
        it.flags.in_building = false;
        it.flags.on_ground = true;
        self.pile_occ_add(tile, 1);
        if !self.is_stored(stone) {
            self.haul_add(sid);
        }
    }

    fn spikes_extend(&mut self, mi: usize, now: u64) {
        let tiles = self.def.mechs[mi].tiles.clone();
        let weapons = self.trap_items(mi, &["WEAPON", "TRAPCOMP"]);
        for s in self.units_on(&tiles) {
            for &wi in &weapons {
                if self.ai[s].kind == Kind::Dead {
                    break;
                }
                let tile = self.ai[s].pos;
                self.trap_attack(s, wi, None, "the upright spike", tile, now);
            }
        }
    }

    /// Synthetic combat weapon for a falling stone (estimate, see the module docs).
    fn stone_template(&mut self) -> u16 {
        if let Some(&w) = self.combat.weap_of.get(STONE_TOKEN) {
            return w;
        }
        let atk = AtkT {
            verb2: "strike".into(),
            verb3: "strikes".into(),
            part: -1,
            contact: 400.0,
            pen: 0.0,
            vel: 2.0,
            prep: 3,
            rec: 3,
            edge: false,
            weight: 1,
            skill: String::new(),
        };
        let id = self.combat.weapons.len() as u16;
        self.combat.weapons.push(WeapT { name: "stone".into(), skill: String::new(), size: 1500.0, attacks: vec![atk] });
        self.combat.weap_of.insert(STONE_TOKEN.to_string(), id);
        id
    }

    /// Combat weapon template of a trap weapon item (vanilla trap components if the export has
    /// no raws for them).
    fn trap_weapon_template(&mut self, wi: usize) -> Option<u16> {
        let token = self.world.items[wi].subtype.clone();
        if let Some(w) = self.weapon_t(&token) {
            return Some(w);
        }
        let &(tok, name, verb, contact, pen, size, _) = TRAPCOMPS.iter().find(|t| t.0 == token)?;
        let atk = AtkT {
            verb3: verb3(verb),
            verb2: verb.into(),
            part: -1,
            contact: contact as f32,
            pen: pen as f32,
            vel: 1.0,
            prep: 4,
            rec: 4,
            edge: true,
            weight: 1,
            skill: String::new(),
        };
        let id = self.combat.weapons.len() as u16;
        self.combat.weapons.push(WeapT { name: name.into(), skill: String::new(), size: size as f32, attacks: vec![atk] });
        self.combat.weap_of.insert(tok.to_string(), id);
        Some(id)
    }

    /// The hidden unit that "wields" trap weapons for the combat module (created on demand).
    fn trap_slot(&mut self) -> usize {
        if self.def.trap_unit != usize::MAX {
            return self.def.trap_unit;
        }
        let existing = self.world.units.iter().find(|u| u.custom_profession == TRAP_MARK && u.flags.inactive).map(|u| u.id);
        let id = match existing {
            Some(id) => id,
            None => {
                let id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
                let caste = self
                    .world
                    .creature_raws
                    .iter()
                    .find(|c| c.token == "DWARF")
                    .and_then(|c| c.castes.first())
                    .map(|c| c.token.clone())
                    .unwrap_or_else(|| "MALE".into());
                self.world.units.push(Unit {
                    id,
                    race: "DWARF".into(),
                    caste,
                    name: String::new(),
                    profession: "STANDARD".into(),
                    custom_profession: TRAP_MARK.into(),
                    flags: UnitFlags { inactive: true, ..Default::default() },
                    civ_id: -1,
                    hist_fig_id: -1,
                    sex: -1,
                    body_size: TRAP_SIZE,
                    walk_speed: 900,
                    squad_id: -1,
                    squad_position: -1,
                    invader_civ: -1,
                    phys_attrs: vec![("STRENGTH".into(), TRAP_STRENGTH), ("AGILITY".into(), 1000)],
                    ..Default::default()
                });
                id
            }
        };
        self.init_units();
        self.combat_sync_units();
        let s = self.unit_slot[&id];
        self.def.trap_unit = s;
        s
    }

    /// One attack of a trap weapon on unit `d`, resolved by the combat module. True if `d` died.
    fn trap_attack(&mut self, d: usize, wi: usize, tmpl: Option<u16>, label: &str, tile: u32, now: u64) -> bool {
        let tmpl = match tmpl.or_else(|| self.trap_weapon_template(wi)) {
            Some(t) => t,
            None => return false,
        };
        let ts = self.trap_slot();
        self.combat_sync_units();
        let skill = {
            let w = &self.combat.weapons[tmpl as usize];
            let s = w.attacks.first().map(|a| a.skill.clone()).unwrap_or_default();
            if s.is_empty() {
                "WRESTLING".to_string()
            } else {
                s
            }
        };
        let p = self.world.pos_of(tile as usize);
        let uidx = self.ai[ts].uidx;
        {
            let u = &mut self.world.units[uidx];
            u.name = label.into();
            u.pos = p;
            u.skills = vec![Skill { id: skill, rating: TRAP_SKILL, xp: 0 }];
        }
        self.ai[ts].pos = tile;
        self.def.trap_weapon = Some((wi, tmpl));
        // The combat rolls derive from (seed, attacker slot, tick); several trap weapons strike in
        // the same tick, so each attack gets its own seed (deterministic).
        self.def.attack_seq += 1;
        let seed = self.seed;
        self.seed = seed ^ self.def.attack_seq.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let hits = self.combat.stats.hits;
        let died = self.attack(ts, d, now);
        self.seed = seed;
        self.def.trap_weapon = None;
        let vid = self.world.units[self.ai[d].uidx].id;
        if self.combat.stats.hits > hits && !self.def.trap_victims.contains(&vid) {
            self.def.trap_victims.push(vid);
        }
        self.world.units[uidx].name = String::new();
        self.def.stats.trap_attacks += 1;
        if died {
            self.def.stats.trap_kills += 1;
        }
        died
    }

    // ------------------------------------------------------------------ levers

    /// Toggle a lever now and send its signal (linked buildings react after the delay).
    pub(crate) fn lever_pulled(&mut self, lever: i32, now: u64) {
        let Some(&mi) = self.def.by_id.get(&lever) else { return };
        if self.def.mechs[mi].kind != MechKind::Lever {
            return;
        }
        let on = !self.def.mechs[mi].on;
        self.def.mechs[mi].on = on;
        self.def.stats.levers_pulled += 1;
        self.send_signals(mi, on, now);
    }

    fn building_goals(&self, lever: i32) -> Goals {
        let mut g = Goals::default();
        let Some(&mi) = self.def.by_id.get(&lever) else { return g };
        let t = self.def.mechs[mi].tiles[0] as usize;
        if self.nav.walkable(t) {
            g.push(t as u32);
        }
        let (x, y, z) = self.nav.xyz(t);
        for &(dx, dy) in &crate::nav::DIRS8 {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let q = self.nav.idx(nx as usize, ny as usize, z);
            if self.nav.walkable(q) {
                g.push(q as u32);
            }
        }
        g
    }

    /// [audit-combat] A weapon trap jams; a mechanic gets a "Clean Trap" job (item -1).
    fn jam_trap(&mut self, mi: usize, tile: u32, now: u64) {
        let id = self.def.mechs[mi].id;
        self.def.mechs[mi].jammed = true;
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == id) {
            set_flag(&mut b.flags, "JAMMED", true);
        }
        self.combat.stats.traps_jammed += 1;
        if !self.def.reload_jobs.iter().any(|j| j.trap == id) {
            self.def.reload_jobs.push(ReloadJob { trap: id, claimed: NONE, item: -1, retry_at: 0 });
        }
        let p = self.world.pos_of(tile as usize);
        self.announce(now, "A weapon trap has jammed.".into(), Some(p), AnnouncementKind::Info);
    }

    /// Cage and stone-fall traps without ammunition get a re-arm job.
    fn post_reload_jobs(&mut self) {
        for mi in 0..self.def.mechs.len() {
            let kind = self.def.mechs[mi].kind;
            let id = self.def.mechs[mi].id;
            if !matches!(kind, MechKind::CageTrap | MechKind::StoneFallTrap) || self.def.reload_jobs.iter().any(|j| j.trap == id) {
                continue;
            }
            if self.trap_loaded(id) == Some(false) {
                self.def.reload_jobs.push(ReloadJob { trap: id, claimed: NONE, item: -1, retry_at: 0 });
            }
        }
    }

    /// Nearest free cage (or stone) in walk component `c`.
    fn reload_item(&self, kind: MechKind, c: u32, from: (usize, usize, usize)) -> Option<i32> {
        let want: &[&str] = if kind == MechKind::CageTrap { &["CAGE"] } else { &["BOULDER"] };
        let mut best = (i32::MAX, -1);
        for (i, it) in self.world.items.iter().enumerate() {
            if !want.contains(&it.kind.as_str()) {
                continue;
            }
            let m = &self.imeta[i];
            let f = &it.flags;
            if m.dead || m.reserved > 0 || m.hauling || it.holder >= 0 || it.building >= 0 || it.container >= 0 || !f.on_ground || f.forbid || f.in_job || f.construction {
                continue;
            }
            if self.def.occupied.contains_key(&it.id) {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.nav.comp_near(t) != c {
                continue;
            }
            let d = crate::manhattan(from, self.nav.xyz(t));
            if (d, it.id) < best {
                best = (d, it.id);
            }
        }
        (best.1 >= 0).then_some(best.1)
    }

    /// A conscious hostile within `r` tiles (and 3 z-levels) of tile `t`.
    fn hostile_near(&self, t: usize, r: i32, now: u64) -> bool {
        let (x, y, z) = self.nav.xyz(t);
        self.combat.hostiles.iter().any(|&h| {
            let h = h as usize;
            if self.ai[h].kind == Kind::Dead || !self.can_fight(h, now) {
                return false;
            }
            let (hx, hy, hz) = self.nav.xyz(self.ai[h].pos as usize);
            (hx as i32 - x as i32).abs().max((hy as i32 - y as i32).abs()) <= r && (hz as i32 - z as i32).abs() <= 3
        })
    }

    /// A mechanic takes a waiting re-arm job.
    fn try_reload_job(&mut self, s: usize, c: u32, now: u64) -> bool {
        if self.ai[s].labors & crate::labor::MECHANIC == 0 {
            return false;
        }
        for k in 0..self.def.reload_jobs.len() {
            let j = self.def.reload_jobs[k];
            if j.claimed != NONE || j.retry_at > now {
                continue;
            }
            let Some(&mi) = self.def.by_id.get(&j.trap) else { continue };
            let goals = self.building_goals(j.trap);
            if !goals.as_slice().iter().any(|&t| self.nav.comp_root(t as usize) == c) {
                continue;
            }
            // DF dwarves drop jobs near hostiles ("Interrupted by ..."): no re-arming while
            // enemies are close to the trap.
            if self.hostile_near(self.def.mechs[mi].tiles[0] as usize, SIGHT, now) {
                self.def.reload_jobs[k].retry_at = now + 200;
                continue;
            }
            if self.def.mechs[mi].kind == MechKind::WeaponTrap {
                // [audit-combat] "Clean Trap": walk to the jammed trap, nothing to fetch.
                self.def.reload_jobs[k].claimed = s as u32;
                self.def.reload_jobs[k].item = -1;
                self.ai[s].task = Task::LoadTrap(j.trap, -1);
                let g = self.building_goals(j.trap);
                self.request_path(s, g, now);
                return true;
            }
            let from = self.nav.xyz(self.ai[s].pos as usize);
            let Some(item) = self.reload_item(self.def.mechs[mi].kind, c, from) else {
                self.def.reload_jobs[k].retry_at = now + rf_core::TICKS_PER_DAY as u64;
                continue;
            };
            self.reserve(item);
            self.def.reload_jobs[k].claimed = s as u32;
            self.def.reload_jobs[k].item = item;
            self.ai[s].task = Task::LoadTrap(j.trap, item);
            let g = self.item_goals(item);
            self.request_path(s, g, now);
            return true;
        }
        false
    }

    /// [arrive hook] Re-arming: pick up the cage/stone, carry it to the trap, load it.
    pub(crate) fn defense_arrive_reload(&mut self, s: usize, trap: i32, item: i32, now: u64) {
        let Some(k) = self.def.reload_jobs.iter().position(|j| j.trap == trap && j.claimed == s as u32) else {
            self.cancel_task(s);
            self.decide(s, now);
            return;
        };
        if item < 0 {
            // [audit-combat] Clean a jammed weapon trap.
            if !self.building_goals(trap).as_slice().contains(&self.ai[s].pos) {
                let g = self.building_goals(trap);
                self.request_path(s, g, now);
                return;
            }
            if let Some(&mi) = self.def.by_id.get(&trap) {
                self.def.mechs[mi].jammed = false;
            }
            if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == trap) {
                set_flag(&mut b.flags, "JAMMED", false);
            }
            self.def.reload_jobs.remove(k);
            self.def.stats.traps_reloaded += 1;
            self.add_xp(s, "MECHANICS", 30);
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        }
        if self.ai[s].carrying != item {
            if !self.item_reachable_here(s, item) {
                self.cancel_task(s);
                self.decide(s, now);
                return;
            }
            self.pickup(s, item);
            let g = self.building_goals(trap);
            self.request_path(s, g, now);
            return;
        }
        let Some(&mi) = self.def.by_id.get(&trap) else {
            self.cancel_task(s);
            self.decide(s, now);
            return;
        };
        let tile = self.def.mechs[mi].tiles[0];
        if !self.building_goals(trap).as_slice().contains(&self.ai[s].pos) {
            let g = self.building_goals(trap);
            self.request_path(s, g, now);
            return;
        }
        // Load it.
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != item);
        self.ai[s].carrying = -1;
        self.unreserve(item);
        self.haul_remove(item);
        let p = self.world.pos_of(tile as usize);
        if let Some(&ii) = self.item_idx.get(&item) {
            let it = &mut self.world.items[ii as usize];
            it.holder = -1;
            it.pos = p;
            it.building = trap;
            it.flags.in_inventory = false;
            it.flags.on_ground = false;
            it.flags.in_building = true;
            self.imeta[ii as usize].hauling = false;
        }
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == trap) {
            b.items.push(item);
            set_flag(&mut b.flags, "LOADED", true);
        }
        self.def.reload_jobs.remove(k);
        self.def.stats.traps_reloaded += 1;
        self.add_xp(s, "MECHANICS", 30);
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    /// [cancel hook] The mechanic gave up: drop what it carries, the job waits.
    pub(crate) fn defense_cancel_reload(&mut self, s: usize, trap: i32, item: i32) {
        let now = self.ticks;
        if item >= 0 && self.ai[s].carrying == item { // [audit-combat] item -1: Clean Trap
            let here = self.ai[s].pos;
            self.drop_item(s, item, here);
        }
        self.unreserve(item);
        for j in &mut self.def.reload_jobs {
            if j.trap == trap && j.claimed == s as u32 {
                j.claimed = NONE;
                j.item = -1;
                j.retry_at = now + 200;
            }
        }
    }

    /// DF job caption of a re-arm job.
    pub(crate) fn defense_reload_caption(&self, trap: i32) -> &'static str {
        match self.def.by_id.get(&trap).map(|&m| self.def.mechs[m].kind) {
            Some(MechKind::StoneFallTrap) => "Load Stone Trap",
            Some(MechKind::WeaponTrap) => "Clean Trap", // [audit-combat]
            _ => "Load Cage Trap",
        }
    }

    /// [decide hook] An idle citizen takes a waiting "Pull the Lever" job it can reach, an
    /// idle mechanic a trap to re-arm.
    pub(crate) fn defense_try_job(&mut self, s: usize, now: u64) -> bool {
        if self.def.lever_jobs.is_empty() && self.def.reload_jobs.is_empty() {
            return false;
        }
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return false;
        }
        let restricted = self.alert_restricted(s); // [military2] civilian alert: levers inside only
        if !restricted && !self.def.reload_jobs.is_empty() && self.try_reload_job(s, c, now) {
            return true;
        }
        for k in 0..self.def.lever_jobs.len() {
            let j = self.def.lever_jobs[k];
            if j.claimed != NONE || j.retry_at > now {
                continue;
            }
            let goals = self.building_goals(j.lever);
            if !goals.as_slice().iter().any(|&t| self.nav.comp_root(t as usize) == c && !self.alert_blocks(s, t as usize)) {
                continue;
            }
            self.def.lever_jobs[k].claimed = s as u32;
            self.ai[s].task = Task::PullLever(j.lever);
            self.request_path(s, goals, now);
            return true;
        }
        false
    }

    /// [arrive hook] At the lever: pull it (takes `PULL_TICKS`).
    pub(crate) fn defense_arrive_lever(&mut self, s: usize, lever: i32, now: u64) {
        let Some(k) = self.def.lever_jobs.iter().position(|j| j.lever == lever && j.claimed == s as u32) else {
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        };
        let goals = self.building_goals(lever);
        if !goals.as_slice().contains(&self.ai[s].pos) {
            self.request_path(s, goals, now);
            return;
        }
        let until = self.def.lever_jobs[k].working_until;
        if until == 0 {
            self.def.lever_jobs[k].working_until = now + PULL_TICKS;
            self.ai[s].wake = now + PULL_TICKS;
            return;
        }
        if now < until {
            self.ai[s].wake = until;
            return;
        }
        self.def.lever_jobs.remove(k);
        self.lever_pulled(lever, now);
        let name = self.world.units[self.ai[s].uidx].name.clone();
        self.combat.log_line(format!("{name} pulls the lever."));
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    /// [cancel hook] The puller gave up: the job waits for somebody else.
    pub(crate) fn defense_cancel_lever(&mut self, s: usize, lever: i32) {
        let now = self.ticks;
        for j in &mut self.def.lever_jobs {
            if j.lever == lever && j.claimed == s as u32 {
                j.claimed = NONE;
                j.working_until = 0;
                j.retry_at = now + 50;
            }
        }
    }

    /// [random_step hook] Tame animals pass locked doors that are pet-passable.
    pub(crate) fn defense_pet_door(&self, s: usize, q: u32) -> bool {
        if self.def.door_at.is_empty() || self.ai[s].kind != Kind::Animal {
            return false;
        }
        let Some(&mi) = self.def.door_at.get(&q) else { return false };
        let m = &self.def.mechs[mi as usize];
        let u = &self.world.units[self.ai[s].uidx];
        m.kind == MechKind::Door && m.forbid && !m.on && m.pet_passable && u.flags.tame && !u.flags.hostile
    }

    // ------------------------------------------------------------------ building destroyers

    fn mech_blocking(&self, mi: usize) -> bool {
        let m = &self.def.mechs[mi];
        match m.kind {
            MechKind::Door | MechKind::Hatch => m.forbid || m.on,
            MechKind::Floodgate => !m.on,
            _ => false,
        }
    }

    /// [fighter_think hook] A building destroyer that cannot reach anybody smashes the nearest
    /// locked door (hatch, floodgate) on its side. True if it took over the unit's turn.
    pub(crate) fn defense_destroyer_think(&mut self, s: usize, now: u64, invader: bool) -> bool {
        if self.def.mechs.is_empty() {
            return false;
        }
        let race = self.world.units[self.ai[s].uidx].race.clone();
        let lvl = self.race_info(&race).1;
        if lvl == 0 {
            return false;
        }
        if let Some(k) = self.def.smash.iter().position(|x| x.0 == s as u32) {
            let (_, bid, until) = self.def.smash[k];
            if !self.def.by_id.contains_key(&bid) {
                self.def.smash.remove(k);
            } else if now >= until {
                self.def.smash.remove(k);
                self.destroy_building(s, bid, now);
                self.ai[s].wake = now + 1;
                return true;
            } else {
                self.ai[s].wake = until;
                return true;
            }
        }
        if self.find_target(s, if invader { None } else { Some(SIGHT) }) != NONE {
            return false;
        }
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        let up = self.nav.xyz(pos);
        let mut best: Option<(i32, usize, Goals)> = None;
        for mi in 0..self.def.mechs.len() {
            let kind = self.def.mechs[mi].kind;
            let breakable = matches!(kind, MechKind::Door | MechKind::Hatch) || (kind == MechKind::Floodgate && lvl >= 2);
            if !breakable || !self.mech_blocking(mi) {
                continue;
            }
            for &t in &self.def.mechs[mi].tiles {
                let tp = self.nav.xyz(t as usize);
                let d = crate::manhattan(up, tp);
                if best.as_ref().map(|b| d >= b.0).unwrap_or(false) || d > 400 {
                    continue;
                }
                let mut g = Goals::default();
                for &(dx, dy) in &crate::nav::DIRS8 {
                    let (nx, ny) = (tp.0 as isize + dx, tp.1 as isize + dy);
                    if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                        continue;
                    }
                    let q = self.nav.idx(nx as usize, ny as usize, tp.2);
                    if self.nav.walkable(q) && self.nav.comp_root(q) == c {
                        g.push(q as u32);
                    }
                }
                if g.n > 0 {
                    best = Some((d, mi, g));
                }
            }
        }
        let Some((_, mi, goals)) = best else { return false };
        if goals.as_slice().contains(&self.ai[s].pos) {
            let id = self.def.mechs[mi].id;
            let until = now + if lvl >= 2 { 150 } else { 300 };
            self.def.smash.push((s as u32, id, until));
            self.ai[s].path.clear();
            self.ai[s].path_i = 0;
            self.ai[s].wake = until;
            let name = self.uname(s);
            let noun = self.def.mechs[mi].kind.noun();
            self.combat.log_line(format!("{} batters the {noun}!", cap(&name)));
            return true;
        }
        let ai = &self.ai[s];
        if ai.path_i < ai.path.len() && goals.as_slice().contains(ai.path.last().unwrap()) {
            self.step(s, now);
        } else if self.combat.f[s].repath_at > now && ai.path.is_empty() {
            self.ai[s].wake = self.combat.f[s].repath_at;
        } else {
            self.combat.f[s].repath_at = now + 20;
            self.request_path(s, goals, now);
        }
        true
    }

    /// [war9] Building kinds a destroyer of level 1 / 2 wrecks (wiki Building destroyer).
    /// Level 1: statues, windows, archery targets, slabs, wooden doors and hatches; level 2 also
    /// furniture, workshops, furnaces, depots, doors, floodgates, containers, racks, siege
    /// engines, pumps, grates, bars and machine parts.
    fn war_wreck_level(&self, b: &rf_core::Building) -> u8 {
        if !b.built {
            return 0;
        }
        let wood = self.world.material(b.mat).map(|m| m.is_wood).unwrap_or(false);
        match b.kind.as_str() {
            "Statue" | "WindowGlass" | "WindowGem" | "ArcheryTarget" | "Slab" => 1,
            "Door" | "Hatch" if wood => 1,
            "Door" | "Hatch" | "Floodgate" | "Chair" | "Bed" | "Table" | "Coffin" | "Workshop" | "Furnace" | "TradeDepot" | "Box" | "Weaponrack" | "Armorstand" | "Cabinet" | "SiegeEngine" | "ScrewPump" | "GrateWall" | "GrateFloor" | "BarsVertical" | "BarsFloor" | "GearAssembly" | "AxleHorizontal" | "AxleVertical" | "WaterWheel" | "Windmill" => 2,
            "FarmPlot" if b.crops.iter().any(|c| !c.is_empty()) => 2,
            _ => 0,
        }
    }

    fn war_wreck_refresh(&mut self) {
        if self.def.wreck_n == self.world.buildings.len() {
            return;
        }
        self.def.wreck_n = self.world.buildings.len();
        let mut list = Vec::new();
        for b in &self.world.buildings {
            let lvl = self.war_wreck_level(b);
            if lvl == 0 {
                continue;
            }
            let Some(t) = self.world.idx(b.center) else { continue };
            list.push((b.id, t as u32, b.z, b.x1, b.y1, b.x2, b.y2, lvl));
        }
        self.def.wreck_list = list;
    }

    fn war_wreck_goals(&self, c: u32, z: i16, x1: i16, y1: i16, x2: i16, y2: i16) -> Goals {
        let mut g = Goals::default();
        for y in (y1 - 1)..=(y2 + 1) {
            for x in (x1 - 1)..=(x2 + 1) {
                if let Some(q) = self.world.idx(Pos::new(x, y, z)) {
                    if self.nav.walkable(q) && self.nav.comp_root(q) == c {
                        g.push(q as u32);
                    }
                }
            }
        }
        g
    }

    /// [war9] [fighter_think hook] Building destroyers "actively seek out buildings and destroy
    /// them" within about 10 tiles (wiki), unless a foe is at hand. True if it took the turn.
    pub(crate) fn war_wreck_think(&mut self, s: usize, now: u64) -> bool {
        let f = &self.combat.f[s];
        if f.wreck < 0 && now < f.wreck_at {
            return false;
        }
        if f.role != Role::Hostile || f.hidden {
            return false;
        }
        let race = self.world.units[self.ai[s].uidx].race.clone();
        let lvl = self.race_info(&race).1;
        if lvl == 0 {
            self.combat.f[s].wreck_at = u64::MAX;
            return false;
        }
        let t = self.combat.f[s].target;
        if self.alive(t) && self.upos(s).dist(self.upos(t as usize)) <= 3 {
            self.combat.f[s].wreck = -1;
            self.combat.f[s].wreck_at = now + 50;
            return false; // a foe at hand comes first
        }
        let here = self.ai[s].pos;
        let c = self.nav.comp_near(here as usize);
        if c == 0 {
            return false;
        }
        let bid = self.combat.f[s].wreck;
        if bid >= 0 {
            let Some(e) = self.def.wreck_list.iter().find(|e| e.0 == bid).copied() else {
                self.combat.f[s].wreck = -1;
                return false;
            };
            if !self.world.buildings.iter().any(|b| b.id == bid) {
                self.combat.f[s].wreck = -1;
                return false;
            }
            let goals = self.war_wreck_goals(c, e.2, e.3, e.4, e.5, e.6);
            if goals.n == 0 {
                self.combat.f[s].wreck = -1;
                self.combat.f[s].wreck_at = now + 300;
                return false;
            }
            if goals.as_slice().contains(&here) {
                let until = self.combat.f[s].wreck_until;
                if until == 0 {
                    // Smashing takes a while (estimate: 100 ticks, 200 for workshops/furnaces).
                    let big = e.3 != e.5 || e.4 != e.6;
                    let done = now + if big { 200 } else { 100 };
                    self.combat.f[s].wreck_until = done;
                    self.ai[s].path.clear();
                    self.ai[s].path_i = 0;
                    self.ai[s].wake = done;
                    return true;
                }
                if now < until {
                    self.ai[s].wake = until;
                    return true;
                }
                self.combat.f[s].wreck = -1;
                self.combat.f[s].wreck_until = 0;
                self.combat.f[s].wreck_at = now + 20;
                self.war_wreck_destroy(s, bid, now);
                self.ai[s].wake = now + 1;
                return true;
            }
            let ai = &self.ai[s];
            if ai.path_i < ai.path.len() && goals.as_slice().contains(ai.path.last().unwrap()) {
                self.step(s, now);
            } else if self.combat.f[s].repath_at > now + 20 {
                // The path failed: forget this building for a while.
                self.combat.f[s].wreck = -1;
                self.combat.f[s].wreck_at = now + 300;
                return false;
            } else {
                self.combat.f[s].repath_at = now + 20;
                self.request_path(s, goals, now);
            }
            return true;
        }
        // Look for a building within 10 tiles on this level.
        let mut r = Rng::for_unit(self.seed, s, now, 0x3EC);
        self.combat.f[s].wreck_at = now + 50 + r.below(50) as u64;
        self.war_wreck_refresh();
        let up = self.upos(s);
        let mut best: Option<(i32, i32)> = None;
        for e in &self.def.wreck_list {
            if e.7 > lvl || e.2 != up.z {
                continue;
            }
            let dx = if up.x < e.3 { e.3 - up.x } else if up.x > e.5 { up.x - e.5 } else { 0 };
            let dy = if up.y < e.4 { e.4 - up.y } else if up.y > e.6 { up.y - e.6 } else { 0 };
            let d = dx.max(dy) as i32;
            if d > 10 || best.map(|b| (d, e.0) >= (b.0, b.1)).unwrap_or(false) {
                continue;
            }
            best = Some((d, e.0));
        }
        let Some((_, bid)) = best else { return false };
        let e = *self.def.wreck_list.iter().find(|e| e.0 == bid).unwrap();
        let goals = self.war_wreck_goals(c, e.2, e.3, e.4, e.5, e.6);
        if goals.n == 0 {
            return false;
        }
        self.combat.f[s].wreck = bid;
        self.combat.f[s].wreck_until = 0;
        if goals.as_slice().contains(&here) {
            self.ai[s].wake = now + 1;
        } else {
            self.combat.f[s].repath_at = now + 20;
            self.request_path(s, goals, now);
        }
        true
    }

    fn war_wreck_destroy(&mut self, s: usize, bid: i32, now: u64) {
        if self.def.by_id.contains_key(&bid) {
            self.destroy_building(s, bid, now); // doors, hatches, floodgates (mechanisms)
        } else {
            let Some(b) = self.world.buildings.iter().find(|b| b.id == bid) else { return };
            let (p, noun) = (b.center, war_noun(b));
            let name = self.uname(s);
            self.remove_building(bid);
            self.combat.log_line(format!("{} has destroyed the {noun}!", cap(&name)));
            self.announce(now, format!("{} has destroyed a {noun}!", cap(&name)), Some(p), AnnouncementKind::Warning);
        }
        self.combat.stats.buildings_wrecked += 1;
        self.def.wreck_n = usize::MAX;
    }

    fn destroy_building(&mut self, s: usize, id: i32, now: u64) {
        let Some(&mi) = self.def.by_id.get(&id) else { return };
        let noun = self.def.mechs[mi].kind.noun();
        let p = self.world.pos_of(self.def.mechs[mi].tiles[0] as usize);
        let name = self.uname(s);
        self.remove_building(id);
        self.defense_scan();
        self.def.stats.buildings_destroyed += 1;
        self.combat.log_line(format!("{} has destroyed the {noun}!", cap(&name)));
        self.announce(now, format!("{} has destroyed a {noun}!", cap(&name)), Some(p), AnnouncementKind::Warning);
    }

    /// Walkable outside map-edge tiles of the largest outside area, for invaders arriving at a
    /// sealed fortress (DF: they come anyway and lay siege outside).
    pub(crate) fn defense_sealed_edges(&self) -> Vec<u32> {
        let (dx, dy, dz) = (self.nav.dx, self.nav.dy, self.nav.dz);
        let mut by_comp: Vec<(u32, Vec<u32>)> = Vec::new();
        for z in 0..dz {
            for y in 0..dy {
                for x in 0..dx {
                    if !(x == 0 || y == 0 || x == dx - 1 || y == dy - 1) {
                        continue;
                    }
                    let i = self.nav.idx(x, y, z);
                    if !self.nav.walkable(i) || self.world.tiles[i].flags & rf_core::tile_flags::OUTSIDE == 0 {
                        continue;
                    }
                    let c = self.nav.comp_root(i);
                    match by_comp.iter_mut().find(|e| e.0 == c) {
                        Some(e) => e.1.push(i as u32),
                        None => by_comp.push((c, vec![i as u32])),
                    }
                }
            }
        }
        by_comp.into_iter().max_by_key(|e| (e.1.len(), std::cmp::Reverse(e.0))).map(|e| e.1).unwrap_or_default()
    }

    // ------------------------------------------------------------------ player API (round4_api)

    pub(crate) fn defense_pull_lever(&mut self, id: i32) {
        let lever = self.def.by_id.get(&id).map(|&mi| self.def.mechs[mi].kind == MechKind::Lever).unwrap_or(false);
        if lever {
            self.def.lever_jobs.push(LeverJob { lever: id, claimed: NONE, retry_at: 0, working_until: 0 });
        }
    }

    pub(crate) fn defense_set_flag(&mut self, id: i32, flag: &str, on: bool) {
        let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == id) else { return };
        set_flag(&mut b.flags, flag, on);
        let Some(&mi) = self.def.by_id.get(&id) else { return };
        let f = flag.to_ascii_uppercase();
        let m = &mut self.def.mechs[mi];
        match f.as_str() {
            "FORBIDDEN" | "FORBID" | "LOCKED" if matches!(m.kind, MechKind::Door | MechKind::Hatch) => m.forbid = on,
            "PET_PASSABLE" => m.pet_passable = on,
            "CITIZENS" => m.plate_citizens = on,
            "OPEN" if m.kind == MechKind::Floodgate => m.on = on,
            "CLOSED" if m.kind == MechKind::Floodgate => m.on = !on,
            _ => {}
        }
        if self.apply_mech_nav(mi) {
            self.settle_nav();
        }
    }

    pub(crate) fn defense_link(&mut self, trigger: i32, target: i32) -> Result<(), String> {
        let tk = self.def.by_id.get(&target).map(|&m| self.def.mechs[m].kind);
        let gk = self.def.by_id.get(&trigger).map(|&m| self.def.mechs[m].kind);
        match (gk, tk) {
            (None, _) => Err("no such lever or pressure plate".into()),
            (Some(g), _) if !g.is_trigger() => Err("only levers and pressure plates can be linked".into()),
            (_, None) => Err("no such target".into()),
            (_, Some(t)) if !t.linkable() => Err("this building cannot be linked to a mechanism".into()),
            _ => {
                let mi = self.def.by_id[&trigger];
                if !self.def.mechs[mi].targets.contains(&target) {
                    self.def.mechs[mi].targets.push(target);
                }
                for b in self.world.buildings.iter_mut() {
                    if b.id == trigger && !b.links.contains(&target) {
                        b.links.push(target);
                    }
                    if b.id == target && !b.links.contains(&trigger) {
                        b.links.push(trigger);
                    }
                }
                Ok(())
            }
        }
    }

    // ------------------------------------------------------------------ public API

    /// Pull a lever right now, without a dwarf walking there (tests, scripts). Linked buildings
    /// react after the mechanism delay ([`LINK_DELAY`]).
    pub fn pull_lever_now(&mut self, building_id: i32) {
        let now = self.ticks;
        self.lever_pulled(building_id, now);
    }

    /// Queued "Pull the Lever" jobs for this lever (claimed or waiting).
    pub fn lever_jobs_pending(&self, building_id: i32) -> usize {
        self.def.lever_jobs.iter().filter(|j| j.lever == building_id).count()
    }

    /// Fort defense counters (traps, bridges, levers, destroyed buildings).
    pub fn defense_stats(&self) -> DefenseStats {
        let mut st = self.def.stats.clone();
        st.trap_victims = self.def.trap_victims.len() as u64;
        st.trap_victims_dead = self.def.trap_victims.iter().filter(|&&id| self.world.unit(id).map(|u| u.flags.dead).unwrap_or(false)).count() as u64;
        st
    }

    /// Building ids of the mechanisms the simulation runs, with their kind.
    pub fn mechanisms(&self) -> Vec<(i32, MechKind)> {
        self.def.mechs.iter().map(|m| (m.id, m.kind)).collect()
    }

    /// Is the trap loaded (cage, stone; weapon traps: any weapon)? None if not a trap.
    pub fn trap_loaded(&self, building_id: i32) -> Option<bool> {
        let &mi = self.def.by_id.get(&building_id)?;
        let kinds: &[&str] = match self.def.mechs[mi].kind {
            MechKind::CageTrap => &["CAGE"],
            MechKind::StoneFallTrap => &["BOULDER", "ROCK"],
            MechKind::WeaponTrap | MechKind::UprightSpikes => &["WEAPON", "TRAPCOMP"],
            _ => return None,
        };
        Some(!self.trap_items(mi, kinds).is_empty())
    }
}


/// [war9] Noun of a building for announcements ("carpenters workshop", "bed").
fn war_noun(b: &rf_core::Building) -> String {
    let base = if b.subtype.is_empty() || b.subtype == "None" { b.kind.clone() } else { format!("{}{}", b.subtype, b.kind) };
    let mut out = String::new();
    for (i, ch) in base.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{Building, Item, ItemFlags, World};

    const DZ: u16 = 20;

    fn hall_z() -> i16 {
        ((DZ as i16 * 2 / 3).max(2) - 4).max(0)
    }

    fn building(id: i32, kind: &str, subtype: &str, x1: i16, y1: i16, x2: i16, y2: i16, z: i16) -> Building {
        Building {
            id,
            kind: kind.into(),
            subtype: subtype.into(),
            x1,
            y1,
            x2,
            y2,
            z,
            center: Pos::new(x1, y1, z),
            stage: 1,
            built: true,
            owner: -1,
            ..Default::default()
        }
    }

    fn item(w: &mut World, id: i32, kind: &str, subtype: &str, mat: &str, pos: Pos, bld: i32) {
        if !w.materials.iter().any(|m| m.token == mat) {
            w.materials.push(rf_core::MaterialInfo { token: mat.into(), name: "iron".into(), is_metal: true, mat_type: 0, mat_index: -1, ..Default::default() });
        }
        let m = w.materials.iter().position(|m| m.token == mat).unwrap_or(0) as u16;
        w.items.push(Item {
            id,
            kind: kind.into(),
            subtype: subtype.into(),
            mat: m,
            pos,
            stack: 1,
            container: -1,
            holder: -1,
            building: bld,
            desc: format!("{} {}", mat.rsplit(':').next().unwrap_or("").to_ascii_lowercase(), kind.to_ascii_lowercase()),
            flags: ItemFlags { in_building: true, ..Default::default() },
            ..Default::default()
        });
        if let Some(b) = w.buildings.iter_mut().find(|b| b.id == bld) {
            b.items.push(id);
        }
    }

    /// The synthetic hall split by a wall at x = cx + 1 with a single gap at (cx + 1, cy + 2).
    fn walled_hall() -> (World, Pos) {
        let mut w = World::synthetic(48, 48, DZ);
        let (cx, cy, hz) = (24i16, 24i16, hall_z());
        let wall = w.tiletypes.by_name("StoneWall").unwrap();
        for y in cy - 4..=cy + 4 {
            if y == cy + 2 {
                continue;
            }
            let t = w.tile_mut(Pos::new(cx + 1, y, hz)).unwrap();
            t.tt = wall;
        }
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
        }
        place(&mut w, true);
        (w, Pos::new(cx + 1, cy + 2, hz))
    }

    /// Dwarves (and the cat) into the east (x 27..29) or west (x 19..21) part of the hall.
    fn place(w: &mut World, east: bool) {
        let hz = hall_z();
        let x0 = if east { 27 } else { 19 };
        for (k, u) in w.units.iter_mut().enumerate() {
            u.pos = Pos::new(x0 + (k % 3) as i16, 20 + (k / 3) as i16, hz);
        }
    }

    fn quiet(w: World) -> Sim {
        let mut sim = Sim::new(w, 7);
        sim.set_invasions(false);
        sim
    }

    #[test]
    fn locked_door_blocks_everyone_and_unlocks_again() {
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(500, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
        let mut sim = quiet(w);
        let (a, b) = (gap.offset(-3, -1, 0), gap.offset(3, -1, 0));
        assert!(sim.find_path(a, b).is_some(), "an unlocked door is passable");
        sim.set_building_flag(500, "FORBIDDEN", true);
        assert!(!sim.is_walkable(gap));
        assert!(sim.find_path(a, b).is_none(), "a locked door blocks pathing");
        assert!(!sim.reachable(a, b), "components are split at once");
        sim.set_building_flag(500, "FORBIDDEN", false);
        assert!(sim.find_path(a, b).is_some());
        assert!(sim.reachable(a, b));
    }

    #[test]
    fn invaders_path_around_locked_doors_into_the_trap() {
        // Two ways through the wall: a door at the gap and a second gap with a cage trap.
        let (mut w, gap) = walled_hall();
        let gap2 = gap.offset(0, -4, 0);
        w.tile_mut(gap2).unwrap().tt = w.tiletypes.by_name("StoneFloor1").unwrap();
        w.buildings.push(building(500, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
        w.buildings.push(building(501, "Trap", "CageTrap", gap2.x, gap2.y, gap2.x, gap2.y, gap2.z));
        item(&mut w, 9001, "CAGE", "", "PLANT:OAK:WOOD", gap2, 501);
        item(&mut w, 9002, "TRAPPARTS", "", "INORGANIC:GRANITE", gap2, 501);
        // Dwarves on the east side, a hostile (the cat) on the west side next to the door.
        for u in &mut w.units {
            if u.race == "CAT" {
                u.pos = gap.offset(-2, 0, 0);
            }
        }
        let mut sim = quiet(w);
        sim.set_building_flag(500, "FORBIDDEN", true);
        sim.make_hostile(100);
        let mut caught = false;
        for _ in 0..40 {
            sim.run_ticks(50);
            if sim.world.unit(100).unwrap().flags.caged {
                caught = true;
                break;
            }
        }
        assert!(caught, "the hostile walked around the locked door into the cage trap: {:?}", sim.defense_stats());
        assert_eq!(sim.world.unit(100).unwrap().pos, gap2);
        assert_eq!(sim.trap_loaded(501), Some(false), "the cage is used up");
        assert!(sim.hostiles().is_empty());
    }

    fn goblin_at(sim: &mut Sim, pos: Pos) -> i32 {
        let id = sim.world.units.iter().map(|u| u.id).max().unwrap() + 1;
        sim.world.units.push(Unit {
            id,
            race: "GOBLIN".into(),
            caste: "MALE".into(),
            profession: "AXEMAN".into(),
            custom_profession: "axeman".into(),
            pos,
            flags: UnitFlags { hostile: true, ..Default::default() },
            civ_id: 77,
            hist_fig_id: -1,
            squad_id: -1,
            squad_position: -1,
            sex: 1,
            body_size: 60_000,
            walk_speed: 900,
            invader_civ: 77,
            phys_attrs: vec![("STRENGTH".into(), 1000), ("AGILITY".into(), 1000)],
            ..Default::default()
        });
        sim.run_ticks(1);
        sim.make_hostile(id);
        id
    }

    #[test]
    fn weapon_trap_wounds_a_goblin() {
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(510, "Trap", "WeaponTrap", gap.x, gap.y, gap.x, gap.y, gap.z));
        for k in 0..3 {
            item(&mut w, 9100 + k, "WEAPON", "ITEM_WEAPON_AXE_BATTLE", "INORGANIC:IRON", gap, 510);
        }
        item(&mut w, 9110, "TRAPCOMP", "ITEM_TRAPCOMP_MENACINGSPIKE", "INORGANIC:IRON", gap, 510);
        let mut sim = quiet(w);
        let g = goblin_at(&mut sim, gap.offset(-1, 0, 0));
        let s = sim.unit_slot[&g];
        let now = sim.ticks();
        let t = sim.world.idx(gap).unwrap() as u32;
        sim.move_to(s, t, now);
        sim.run_ticks(2);
        let st = sim.defense_stats();
        assert_eq!(st.weapon_trap_triggers, 1);
        let dead = sim.world.unit(g).unwrap().flags.dead;
        assert!(st.trap_attacks == 4 || (dead && st.trap_attacks >= 1), "three axes and one spike (HITS 1): {st:?}");
        for l in sim.combat_log() {
            println!("  {l}");
        }
        let u = sim.world.unit(g).unwrap();
        assert!(u.flags.dead || !u.wounds.is_empty(), "the goblin is wounded: {:?}", sim.combat_log());
        assert!(sim.combat_log().iter().any(|l| l.starts_with("The weapon trap")), "{:?}", sim.combat_log());
        // Citizens walk over their own traps safely.
        let dw = sim.unit_slot[&1];
        let now = sim.ticks();
        sim.move_to(dw, t, now);
        sim.run_ticks(2);
        assert_eq!(sim.defense_stats().weapon_trap_triggers, 1);
    }

    #[test]
    fn weapon_trap_jams_on_kills_and_a_mechanic_cleans_it() {
        // [audit-combat] Wiki Weapon trap: 50 % of kills jam the trap; a mechanic cleans it.
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(512, "Trap", "WeaponTrap", gap.x, gap.y, gap.x, gap.y, gap.z));
        for k in 0..10 {
            item(&mut w, 9200 + k, "WEAPON", "ITEM_WEAPON_AXE_BATTLE", "INORGANIC:IRON", gap, 512);
        }
        w.units[0].labors.push("MECHANIC".into());
        let mut sim = quiet(w);
        let mi = sim.def.by_id[&512];
        let t = sim.world.idx(gap).unwrap() as u32;
        let mut kills = 0;
        for _ in 0..16 {
            let g = goblin_at(&mut sim, gap.offset(-1, 0, 0));
            let s = sim.unit_slot[&g];
            let now = sim.ticks();
            sim.move_to(s, t, now);
            sim.run_ticks(2);
            if sim.world.unit(g).unwrap().flags.dead {
                kills += 1;
            } else {
                let now = sim.ticks();
                sim.combat_kill(s, now, "has been struck down");
            }
            if sim.def.mechs[mi].jammed {
                break;
            }
        }
        assert!(sim.def.mechs[mi].jammed, "{kills} trap kills, none jammed");
        assert!(sim.combat_stats().traps_jammed == 1);
        assert!(sim.world.building(512).unwrap().flags.iter().any(|f| f == "JAMMED"));
        // A jammed trap does nothing.
        let attacks = sim.defense_stats().trap_attacks;
        let g = goblin_at(&mut sim, gap.offset(-1, 0, 0));
        let s = sim.unit_slot[&g];
        let now = sim.ticks();
        sim.move_to(s, t, now);
        sim.run_ticks(2);
        assert_eq!(sim.defense_stats().trap_attacks, attacks);
        let now = sim.ticks();
        sim.combat_kill(s, now, "has been struck down");
        // The mechanic cleans it ("Clean Trap").
        sim.run_ticks(3000);
        assert!(!sim.def.mechs[mi].jammed, "{:?}", sim.unit_status(1));
        assert!(!sim.world.building(512).unwrap().flags.iter().any(|f| f == "JAMMED"));
    }

    #[test]
    fn stone_fall_trap_drops_its_stone_once() {
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(520, "Trap", "StoneFallTrap", gap.x, gap.y, gap.x, gap.y, gap.z));
        item(&mut w, 9200, "BOULDER", "", "INORGANIC:GRANITE", gap, 520);
        let mut sim = quiet(w);
        let g = goblin_at(&mut sim, gap.offset(-1, 0, 0));
        let s = sim.unit_slot[&g];
        let t = sim.world.idx(gap).unwrap() as u32;
        let now = sim.ticks();
        sim.move_to(s, t, now);
        sim.run_ticks(2);
        assert_eq!(sim.defense_stats().stones_dropped, 1);
        assert_eq!(sim.trap_loaded(520), Some(false));
        let stone = sim.world.item(9200).unwrap();
        assert!(stone.flags.on_ground && stone.building < 0 && stone.pos == gap);
        let now = sim.ticks();
        sim.move_to(s, t, now);
        sim.run_ticks(2);
        assert_eq!(sim.defense_stats().stones_dropped, 1, "single use until reloaded");
    }

    #[test]
    fn mechanic_rearms_a_sprung_cage_trap() {
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(540, "Trap", "CageTrap", gap.x, gap.y, gap.x, gap.y, gap.z));
        item(&mut w, 9400, "CAGE", "", "PLANT:OAK:WOOD", gap, 540);
        // A spare cage on the floor and a mechanic.
        let spare = gap.offset(3, -4, 0);
        item(&mut w, 9401, "CAGE", "", "PLANT:OAK:WOOD", spare, -1);
        let it = w.items.last_mut().unwrap();
        it.flags = ItemFlags { on_ground: true, ..Default::default() };
        w.units[0].labors.push("MECHANIC".into());
        let mut sim = quiet(w);
        let g = goblin_at(&mut sim, gap.offset(-1, 0, 0));
        let s = sim.unit_slot[&g];
        let t = sim.world.idx(gap).unwrap() as u32;
        let now = sim.ticks();
        sim.move_to(s, t, now);
        sim.run_ticks(2);
        assert_eq!(sim.defense_stats().caught, 1);
        assert_eq!(sim.trap_loaded(540), Some(false));
        sim.run_ticks(3000);
        assert_eq!(sim.defense_stats().traps_reloaded, 1, "{:?}", sim.unit_status(1));
        assert_eq!(sim.trap_loaded(540), Some(true));
        let cage = sim.world.item(9401).unwrap();
        assert!(cage.flags.in_building && cage.building == 540 && cage.holder < 0);
    }

    #[test]
    fn trapavoid_creatures_ignore_traps() {
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(530, "Trap", "CageTrap", gap.x, gap.y, gap.x, gap.y, gap.z));
        item(&mut w, 9300, "CAGE", "", "PLANT:OAK:WOOD", gap, 530);
        let mut sim = quiet(w);
        let g = goblin_at(&mut sim, gap.offset(-1, 0, 0));
        let s = sim.unit_slot[&g];
        let ui = sim.ai[s].uidx;
        sim.world.units[ui].race = "KOBOLD".into();
        let t = sim.world.idx(gap).unwrap() as u32;
        let now = sim.ticks();
        sim.move_to(s, t, now);
        sim.run_ticks(2);
        assert_eq!(sim.defense_stats().caught, 0);
        assert!(!sim.world.unit(g).unwrap().flags.caged);
    }

    /// A 3x3 drawbridge raising north over solid floor, with a lever.
    fn bridge_world() -> (Sim, Pos, Pos) {
        let mut w = World::synthetic(48, 48, DZ);
        let hz = hall_z();
        let (bx, by) = (26i16, 22i16);
        let mut br = building(600, "Bridge", "", bx, by, bx + 2, by + 2, hz);
        br.direction = "N".into();
        w.buildings.push(br);
        w.buildings.push(building(601, "Trap", "Lever", 20, 26, 20, 26, hz));
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
        }
        place(&mut w, false);
        let mut sim = quiet(w);
        sim.link_building(601, 600).unwrap();
        (sim, Pos::new(bx, by, hz), Pos::new(bx + 1, by + 2, hz))
    }

    #[test]
    fn raised_bridge_blocks_and_lowering_crushes() {
        let (mut sim, edge, inner) = bridge_world();
        assert!(sim.is_walkable(edge) && sim.is_walkable(inner));
        // A dwarf standing on the raising edge gets flung off.
        let dw = sim.unit_slot[&1];
        let t = sim.world.idx(edge.offset(1, 0, 0)).unwrap() as u32;
        let now = sim.ticks();
        sim.move_to(dw, t, now);
        sim.pull_lever_now(601);
        sim.run_ticks(LINK_DELAY as u32 - 5);
        assert!(sim.is_walkable(edge), "the mechanism has a delay");
        sim.run_ticks(10);
        assert!(!sim.is_walkable(edge), "the raised edge is a wall");
        assert!(!sim.is_walkable(edge.offset(2, 0, 0)));
        assert!(sim.is_walkable(inner), "the rest of the area is the floor beneath");
        let d = sim.world.unit(1).unwrap();
        assert!(!d.flags.dead && d.pos.y != edge.y, "flung off the bridge: {:?}", d.pos);
        assert_eq!(sim.defense_stats().flung, 1);
        // A goblin and some junk under the raised bridge are atom-smashed when it comes down.
        let g = goblin_at(&mut sim, inner);
        let gs = sim.unit_slot[&g];
        sim.combat.f[gs].uncon_until = u64::MAX / 2; // stays put
        let ti = sim.world.idx(inner).unwrap();
        let junk = sim.create_item("BOULDER", 0, ti, "granite boulder".into());
        let items0 = sim.world.items.iter().filter(|i| i.kind == "CORPSE").count();
        sim.pull_lever_now(601);
        sim.run_ticks(LINK_DELAY as u32 + 5);
        assert!(sim.is_walkable(edge), "lowered again");
        let gu = sim.world.unit(g).unwrap();
        assert!(gu.flags.dead, "crushed");
        assert!(sim.world.item(junk).is_none() || sim.imeta[sim.item_idx[&junk] as usize].dead, "items obliterated");
        assert_eq!(sim.world.items.iter().filter(|i| i.kind == "CORPSE").count(), items0, "no corpse left");
        assert_eq!(sim.defense_stats().crushed, 1);
    }

    #[test]
    fn bridge_over_a_moat_is_a_floor_until_retracted() {
        let mut w = World::synthetic(48, 48, DZ);
        let hz = hall_z();
        let open = w.tiletypes.by_name("OpenSpace").unwrap();
        // A moat across the whole hall at x = 27 (two levels deep).
        for y in 0..48 {
            for z in [hz, hz - 1] {
                let t = w.tile_mut(Pos::new(27, y, z)).unwrap();
                t.tt = if z == hz { open } else { open };
            }
        }
        let mut br = building(700, "Bridge", "", 27, 24, 27, 24, hz);
        br.direction = "Retracting".into();
        w.buildings.push(br);
        w.buildings.push(building(701, "Trap", "Lever", 22, 22, 22, 22, hz));
        place(&mut w, false);
        let mut sim = quiet(w);
        sim.link_building(701, 700).unwrap();
        let (a, b, on) = (Pos::new(24, 24, hz), Pos::new(30, 24, hz), Pos::new(27, 24, hz));
        assert!(sim.is_walkable(on), "the bridge is a floor over the moat");
        assert!(sim.find_path(a, b).is_some());
        sim.pull_lever_now(701);
        sim.run_ticks(LINK_DELAY as u32 + 2);
        assert!(!sim.is_walkable(on), "retracted: a gap");
        assert!(sim.find_path(a, b).is_none());
    }

    #[test]
    fn lever_job_is_done_by_a_dwarf_and_locks_the_door() {
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(800, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
        w.buildings.push(building(801, "Trap", "Lever", gap.x - 4, gap.y - 3, gap.x - 4, gap.y - 3, gap.z));
        place(&mut w, false);
        let mut sim = quiet(w);
        sim.link_building(801, 800).unwrap();
        assert!(sim.link_building(800, 801).is_err(), "a door is no trigger");
        let (a, b) = (gap.offset(-3, -1, 0), gap.offset(3, -1, 0));
        sim.pull_lever(801);
        assert_eq!(sim.lever_jobs_pending(801), 1);
        let mut pulled_at = None;
        for t in 0..300 {
            sim.run_ticks(10);
            if pulled_at.is_none() && sim.defense_stats().levers_pulled == 1 {
                pulled_at = Some(t);
            }
            if !sim.is_walkable(gap) {
                break;
            }
        }
        assert!(pulled_at.is_some(), "somebody pulled the lever");
        assert_eq!(sim.lever_jobs_pending(801), 0);
        assert!(!sim.is_walkable(gap), "the linked door closed and locked");
        assert!(sim.find_path(a, b).is_none());
        assert!(sim.world.building(800).unwrap().flags.iter().any(|f| f == "CLOSED"));
        sim.pull_lever_now(801);
        sim.run_ticks(LINK_DELAY as u32 + 2);
        assert!(sim.find_path(a, b).is_some(), "pulled again: open");
    }

    #[test]
    fn pressure_plate_triggers_linked_door() {
        let (mut w, gap) = walled_hall();
        let plate = gap.offset(-3, 0, 0);
        w.buildings.push(building(900, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
        w.buildings.push(building(901, "Trap", "PressurePlate", plate.x, plate.y, plate.x, plate.y, plate.z));
        let mut sim = quiet(w);
        sim.link_building(901, 900).unwrap();
        // Citizens do not set it off.
        let t = sim.world.idx(plate).unwrap() as u32;
        let dw = sim.unit_slot[&1];
        let now = sim.ticks();
        sim.move_to(dw, t, now);
        sim.run_ticks(LINK_DELAY as u32 + 5);
        assert!(sim.is_walkable(gap));
        let g = goblin_at(&mut sim, plate.offset(-1, 0, 0));
        let s = sim.unit_slot[&g];
        let now = sim.ticks();
        sim.move_to(s, t, now);
        sim.run_ticks(LINK_DELAY as u32 + 5);
        assert_eq!(sim.defense_stats().plates_triggered, 1);
        assert!(!sim.is_walkable(gap), "plate on: door locked");
        // After the plate resets the door opens again.
        let away = sim.world.idx(plate.offset(-1, -1, 0)).unwrap() as u32;
        let now = sim.ticks();
        sim.move_to(s, away, now);
        sim.combat.f[s].uncon_until = u64::MAX / 2; // stays off the plate
        sim.run_ticks((PLATE_RESET + LINK_DELAY) as u32 + 20);
        assert!(sim.is_walkable(gap), "plate off: door open");
    }

    #[test]
    fn troll_smashes_a_locked_door() {
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(1000, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
        let mut sim = quiet(w);
        sim.set_building_flag(1000, "FORBIDDEN", true);
        let g = goblin_at(&mut sim, gap.offset(-4, -2, 0));
        let s = sim.unit_slot[&g];
        let ui = sim.ai[s].uidx;
        sim.world.units[ui].race = "TROLL".into();
        let mut gone = false;
        for _ in 0..60 {
            sim.run_ticks(50);
            if sim.world.building(1000).is_none() {
                gone = true;
                break;
            }
        }
        assert!(gone, "the troll destroyed the door: {:?} {:?}", sim.defense_stats(), sim.combat_log());
        assert!(sim.is_walkable(gap));
        assert_eq!(sim.defense_stats().buildings_destroyed, 1);
    }

    #[test]
    fn locked_hatch_blocks_the_stairs_below() {
        let mut w = World::synthetic(48, 48, DZ);
        let hz = hall_z();
        // The stair shaft runs from the hall up to the surface at (24, 24).
        w.buildings.push(building(1100, "Hatch", "", 24, 24, 24, 24, hz + 1));
        let mut sim = quiet(w);
        let (hall, up) = (Pos::new(22, 24, hz), Pos::new(24, 24, hz + 2));
        assert!(sim.find_path(hall, up).is_some());
        sim.set_building_flag(1100, "FORBIDDEN", true);
        assert!(sim.find_path(hall, up).is_none(), "a locked hatch closes the shaft");
        assert!(sim.is_walkable(Pos::new(24, 24, hz + 1)), "but it can be walked on");
        sim.set_building_flag(1100, "FORBIDDEN", false);
        assert!(sim.find_path(hall, up).is_some());
    }

    #[test]
    fn floodgate_opens_with_its_lever() {
        let (mut w, gap) = walled_hall();
        w.buildings.push(building(1200, "Floodgate", "", gap.x, gap.y, gap.x, gap.y, gap.z));
        w.buildings.push(building(1201, "Trap", "Lever", 22, 24, 22, 24, gap.z));
        let mut sim = quiet(w);
        sim.link_building(1201, 1200).unwrap();
        let (a, b) = (gap.offset(-3, -1, 0), gap.offset(3, -1, 0));
        assert!(sim.find_path(a, b).is_none(), "floodgates start closed");
        sim.pull_lever_now(1201);
        sim.run_ticks(LINK_DELAY as u32 + 2);
        assert!(sim.find_path(a, b).is_some(), "open");
        assert!(sim.world.building(1200).unwrap().flags.iter().any(|f| f == "OPEN"));
    }

    #[test]
    fn defense_is_deterministic_across_threads() {
        let run = |par: usize| {
            let (mut w, gap) = walled_hall();
            w.buildings.push(building(510, "Trap", "WeaponTrap", gap.x, gap.y, gap.x, gap.y, gap.z));
            for k in 0..3 {
                item(&mut w, 9100 + k, "TRAPCOMP", "ITEM_TRAPCOMP_GIANTAXEBLADE", "INORGANIC:IRON", gap, 510);
            }
            let mut sim = quiet(w);
            sim.set_parallel_threshold(par);
            for k in 0..4 {
                goblin_at(&mut sim, gap.offset(-3, k - 2, 0));
            }
            sim.run_ticks(3000);
            println!("{:?}", sim.defense_stats());
            (sim.state_hash(), sim.defense_stats().trap_attacks, sim.combat_log().len())
        };
        let a = run(1);
        let b = run(1_000_000);
        assert_eq!(a, b);
        assert!(a.1 > 0, "the goblins ran through the trap");
    }
}
