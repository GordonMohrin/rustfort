//! [machines14] Mechanism links: signal routing from levers and pressure plates to the linked
//! devices, the per-device reaction times, pressure plates as sensors, and the mechanic's
//! linking job (DF v50; wiki Lever, Pressure plate, Machine component, Mechanism, Floodgate,
//! Bridge, Grate, Bars, Gear assembly, Minecart "Track stop", Cage, Restraint, Support).
//!
//! # DF rules implemented
//! - A lever sends its **new state** ("on" or "off") to every target; a pressure plate sends
//!   "on" the moment its condition holds and "off" once it has been false for 99 ticks (a
//!   retrigger inside that window cancels the "off" and sends nothing).
//! - "On" **opens** doors, hatches, floodgates, grates and bars, **raises** (or retracts)
//!   bridges, **retracts** upright spikes, **disables** track stops; "off" does the reverse.
//!   Gear assemblies **toggle** on every signal. Cages, chains and supports react once to "on".
//! - Reaction times: doors, hatches, gears, cages, chains, supports and the track stop's
//!   switch-off at once; spikes 40 ticks, the track stop's switch-on 40; bridges, floodgates,
//!   grates and bars 100. Delayed devices ignore further signals until the change has taken
//!   effect (refractory). "On" and "off" in the same tick toggle the device.
//! - A signal asking for the state the device is in does nothing.
//! - Plates: creature weight sum in `[unit_min, unit_max]`, citizens (and tame animals) only
//!   with the citizens setting, merchants and diplomats never, TRAPAVOID never, unconscious
//!   creatures always; water / magma depth in range; minecart gross weight in range on track.
//!   One-use plates (no "resets") fire once and deconstruct, destroying their mechanisms.
//! - Linking needs 2 mechanisms and a mechanic: the first is installed in the target, the
//!   second in the trigger.
//! - Items on a floodgate or door keep it from closing; a floodgate closes by itself once the
//!   tile is clear, a door needs a new signal. Closing destroys the liquid on the tile.
//! - A creature of size 1 200 000 or more on a bridge stops it from raising; lowering onto one
//!   deconstructs the bridge.
//!
//! # Estimates
//! - `LINK_WORK` (100 ticks per end), `JAM_RECHECK` (10), the plate re-scan (every 100 ticks),
//!   default ranges of plates without exported settings, creature weight = body size,
//!   visitors do not trigger plates, gear toggles twice in one tick = no change.

use crate::path::Goals;
use crate::{Kind, Sim, Task, NONE};
use rf_core::{Building, PlateInfo, Pos};
use serde::{Deserialize, Serialize};

/// Bridges, floodgates, grates and bars (wiki Lever "Activation of the devices").
pub const DELAY_GATE: u64 = 100;
/// Upright spikes (wiki Pressure plate timing table: 40 / 139).
pub const DELAY_SPIKES: u64 = 40;
/// A disabled track stop comes back on 40 ticks after "off" (wiki Minecart).
pub const DELAY_TRACKSTOP_ON: u64 = 40;
/// A plate sends "off" after its condition has been false this long (wiki Pressure plate).
pub const PLATE_CLOSE: u64 = 99;
/// Jammed floodgates look again this often (ESTIMATE).
pub const JAM_RECHECK: u64 = 10;
/// Work per installed mechanism of a link job (ESTIMATE).
pub const LINK_WORK: u64 = 100;
/// A creature this big stops a bridge (wiki Bridge).
pub const BRIDGE_STOP_SIZE: i32 = 1_200_000;
/// Default creature range of plates (DF's selectable range, wiki Pressure plate).
pub const PLATE_UNIT_MIN: i32 = 10_000;
pub const PLATE_UNIT_MAX: i32 = 2_000_000;

/// What a linked building does with signals.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum DevKind {
    Door,
    Hatch,
    Floodgate,
    Bridge,
    GrateWall,
    GrateFloor,
    BarsVertical,
    BarsFloor,
    Spikes,
    Gear,
    TrackStop,
    Cage,
    Chain,
    Support,
}

impl DevKind {
    pub fn of(b: &Building) -> Option<DevKind> {
        Some(match b.kind.as_str() {
            "Door" => DevKind::Door,
            "Hatch" => DevKind::Hatch,
            "Floodgate" => DevKind::Floodgate,
            "Bridge" => DevKind::Bridge,
            "GrateWall" => DevKind::GrateWall,
            "GrateFloor" => DevKind::GrateFloor,
            "BarsVertical" => DevKind::BarsVertical,
            "BarsFloor" => DevKind::BarsFloor,
            "Weapon" => DevKind::Spikes,
            "GearAssembly" => DevKind::Gear,
            "Cage" => DevKind::Cage,
            "Chain" => DevKind::Chain,
            "Support" => DevKind::Support,
            _ if is_track_stop(b) => DevKind::TrackStop,
            _ => return None,
        })
    }

    /// Ticks from receiving a signal to the change (`open` = the wanted state).
    pub fn delay(self, open: bool) -> u64 {
        match self {
            DevKind::Floodgate | DevKind::Bridge | DevKind::GrateWall | DevKind::GrateFloor | DevKind::BarsVertical | DevKind::BarsFloor => DELAY_GATE,
            DevKind::Spikes => DELAY_SPIKES,
            DevKind::TrackStop if !open => DELAY_TRACKSTOP_ON,
            _ => 0,
        }
    }

    /// Ignores signals while a change is pending.
    pub fn refractory(self) -> bool {
        !matches!(self, DevKind::TrackStop) && self.delay(true) > 0
    }

    pub fn noun(self) -> &'static str {
        match self {
            DevKind::Door => "door",
            DevKind::Hatch => "hatch",
            DevKind::Floodgate => "floodgate",
            DevKind::Bridge => "bridge",
            DevKind::GrateWall => "wall grate",
            DevKind::GrateFloor => "floor grate",
            DevKind::BarsVertical => "vertical bars",
            DevKind::BarsFloor => "floor bars",
            DevKind::Spikes => "upright spikes",
            DevKind::Gear => "gear assembly",
            DevKind::TrackStop => "track stop",
            DevKind::Cage => "cage",
            DevKind::Chain => "chain",
            DevKind::Support => "support",
        }
    }

    fn one_shot(self) -> bool {
        matches!(self, DevKind::Cage | DevKind::Chain | DevKind::Support)
    }
}

/// Exported and player-built track stops are `Trap`/`TrackStop`; tests use kind `TrackStop`.
pub fn is_track_stop(b: &Building) -> bool {
    b.kind == "TrackStop" || (b.kind == "Trap" && b.subtype == "TrackStop")
}

/// A track stop switched off by its trigger.
pub fn stop_disabled(b: &Building) -> bool {
    has(&b.flags, "DISABLED")
}

/// Friction of a track stop: the exported value, else the subtype label.
pub fn stop_friction_of(b: &Building) -> i32 {
    if b.friction > 0 {
        b.friction
    } else {
        crate::minecarts::stop_friction(&b.subtype)
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

/// A pending change of a delayed device.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(default)]
pub(crate) struct Timer {
    pub due: u64,
    pub bid: i32,
    pub want: bool,
}

/// One pressure plate.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Plate {
    pub bid: i32,
    pub tile: u32,
    pub units: bool,
    pub water: bool,
    pub magma: bool,
    pub track: bool,
    pub citizens: bool,
    pub resets: bool,
    pub r: PlateInfo,
    /// Last signal sent.
    pub on: bool,
    /// Tick the condition became false while on (u64::MAX while it holds).
    pub false_since: u64,
    /// Unit slots seen on the plate (pruned when they leave).
    pub occ: Vec<u32>,
    /// Gross weight of a cart that entered the tile this tick (0 = none).
    pub cart_w: i32,
    /// Weight of the last cart that entered (a cart resting on the plate keeps pressing it).
    pub last_w: i32,
}

/// A queued "Link a building to a trigger" job.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(default)]
pub(crate) struct LinkJob {
    pub trigger: i32,
    pub target: i32,
    pub claimed: u32,
    /// Reserved mechanism item ids (-1 = none yet).
    pub mech: [i32; 2],
    /// 0 fetch the first mechanism, 1 install it at the target, 2 fetch the second,
    /// 3 install it at the trigger.
    pub phase: u8,
    pub until: u64,
    pub retry_at: u64,
}

impl Default for LinkJob {
    fn default() -> Self {
        LinkJob { trigger: -1, target: -1, claimed: NONE, mech: [-1, -1], phase: 0, until: 0, retry_at: 0 }
    }
}

/// Counters for reports and tests.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub struct LinkStats {
    pub signals: u64,
    pub changes: u64,
    pub ignored: u64,
    pub toggled_same_tick: u64,
    pub plates_on: u64,
    pub plates_off: u64,
    pub plates_spent: u64,
    pub links_made: u64,
    pub jammed: u64,
    pub one_shots: u64,
    pub bridges_stopped: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Linkage {
    /// Signals sent this tick: (target building id, on).
    pub inbox: Vec<(i32, bool)>,
    /// Pending changes, sorted by (due, bid).
    pub timers: Vec<Timer>,
    pub plates: Vec<Plate>,
    /// Building count the plate list was built for.
    pub nb: usize,
    pub jobs: Vec<LinkJob>,
    /// Jammed doors and floodgates (building ids, sorted).
    pub jammed: Vec<i32>,
    pub stats: LinkStats,
    #[serde(skip)]
    scratch: Vec<(i32, bool)>,
}

impl Sim {
    // ------------------------------------------------------------------ sending

    /// Send `on`/`off` from a lever or plate to all its targets (they react in `linkage_tick`).
    pub(crate) fn link_send(&mut self, target: i32, on: bool) {
        self.link.inbox.push((target, on));
        self.link.stats.signals += 1;
    }

    /// Current DF state of a linked device: (kind, open). See [`DevKind`] for "open".
    pub(crate) fn device_open(&self, bid: i32) -> Option<(DevKind, bool)> {
        let b = self.world.building(bid)?;
        let k = DevKind::of(b)?;
        let open = match k {
            DevKind::Door | DevKind::Hatch | DevKind::Spikes | DevKind::Bridge | DevKind::Floodgate => {
                let on = self.def_on(bid)?;
                if matches!(k, DevKind::Bridge | DevKind::Floodgate) {
                    on
                } else {
                    !on
                }
            }
            DevKind::Gear => has(&b.flags, "DISENGAGED"),
            DevKind::TrackStop => stop_disabled(b),
            DevKind::GrateWall | DevKind::GrateFloor | DevKind::BarsVertical | DevKind::BarsFloor => has(&b.flags, "OPEN"),
            DevKind::Cage | DevKind::Chain | DevKind::Support => false,
        };
        Some((k, open))
    }

    // ------------------------------------------------------------------ tick

    /// Every tick right after `defense_tick`: plates, signals, pending changes, jams.
    pub(crate) fn linkage_tick(&mut self, now: u64) {
        if self.world.buildings.len() != self.link.nb || now % 100 == 59 {
            self.link_sync_plates();
        }
        if now % 100 == 61 && !self.link.plates.is_empty() {
            self.link_rescan_plates();
        }
        if !self.link.plates.is_empty() {
            self.link_poll_plates(now);
        }
        if !self.link.inbox.is_empty() {
            self.link_receive(now);
        }
        while let Some(&t) = self.link.timers.first() {
            if t.due > now {
                break;
            }
            self.link.timers.remove(0);
            if let Some((k, open)) = self.device_open(t.bid) {
                if open != t.want {
                    self.link_apply(t.bid, k, t.want, now);
                }
            }
        }
        if now % 100 == 63 && !self.link.jobs.is_empty() {
            self.link_prune_jobs(now);
        }
        if !self.link.jammed.is_empty() && now % JAM_RECHECK == 0 {
            let jam = self.link.jammed.clone();
            for bid in jam {
                if let Some((DevKind::Floodgate, true)) = self.device_open(bid) {
                    self.link_apply(bid, DevKind::Floodgate, false, now);
                } else if !matches!(self.device_open(bid), Some((DevKind::Door, true))) {
                    self.link.jammed.retain(|&x| x != bid);
                }
            }
        }
    }

    /// Drop waiting jobs whose trigger or target is gone; free jobs whose mechanic was pulled
    /// away without the cancel hook (task changed, died elsewhere).
    fn link_prune_jobs(&mut self, now: u64) {
        let mut k = 0;
        while k < self.link.jobs.len() {
            let j = self.link.jobs[k];
            if j.claimed != NONE {
                let s = j.claimed as usize;
                let lost = s >= self.ai.len() || self.ai[s].kind == Kind::Dead || self.ai[s].task != Task::Link(j.trigger, j.target);
                if lost {
                    for (n, &m) in j.mech.iter().enumerate() {
                        if m >= 0 && !(n == 0 && j.phase >= 2) {
                            self.unreserve(m);
                        }
                    }
                    let jj = &mut self.link.jobs[k];
                    jj.claimed = NONE;
                    jj.until = 0;
                    if jj.phase < 2 {
                        jj.mech = [-1, -1];
                        jj.phase = 0;
                    } else {
                        jj.mech[1] = -1;
                    }
                    jj.retry_at = now;
                }
                k += 1;
                continue;
            }
            if self.world.building(j.trigger).is_none() || self.world.building(j.target).is_none() {
                self.link.jobs.remove(k);
                continue;
            }
            k += 1;
        }
    }

    fn link_receive(&mut self, now: u64) {
        std::mem::swap(&mut self.link.inbox, &mut self.link.scratch);
        let mut sig = std::mem::take(&mut self.link.scratch);
        sig.sort_unstable();
        let mut i = 0;
        while i < sig.len() {
            let bid = sig[i].0;
            let (mut got_on, mut got_off, mut n) = (false, false, 0u32);
            while i < sig.len() && sig[i].0 == bid {
                if sig[i].1 {
                    got_on = true;
                } else {
                    got_off = true;
                }
                n += 1;
                i += 1;
            }
            self.link_signal(bid, got_on, got_off, n, now);
        }
        sig.clear();
        self.link.scratch = sig;
    }

    /// One device received signals this tick.
    fn link_signal(&mut self, bid: i32, got_on: bool, got_off: bool, n: u32, now: u64) {
        let Some((k, open)) = self.device_open(bid) else { return };
        if k == DevKind::Gear {
            // Every signal toggles a gear assembly.
            if n % 2 == 1 {
                self.link_apply(bid, k, !open, now);
            }
            return;
        }
        let pending = self.link.timers.iter().position(|t| t.bid == bid);
        if let Some(p) = pending {
            if k.refractory() {
                self.link.stats.ignored += 1;
                return;
            }
            self.link.timers.remove(p);
        }
        let want = if got_on && got_off {
            self.link.stats.toggled_same_tick += 1;
            !open
        } else {
            got_on
        };
        if want && k == DevKind::Floodgate {
            // Asked to stay open: a pending jam must not close it once the tile clears.
            if let Ok(p) = self.link.jammed.binary_search(&bid) {
                self.link.jammed.remove(p);
            }
        }
        let jammed_door = k == DevKind::Door && self.link.jammed.binary_search(&bid).is_ok();
        if want == open && !jammed_door {
            return;
        }
        if k.one_shot() && !want {
            return;
        }
        let d = k.delay(want);
        if d == 0 {
            self.link_apply(bid, k, want, now);
        } else {
            let t = Timer { due: now + d, bid, want };
            let at = self.link.timers.partition_point(|x| *x < t);
            self.link.timers.insert(at, t);
        }
    }

    /// Make device `bid` open (`open` = true) or closed now.
    pub(crate) fn link_apply(&mut self, bid: i32, k: DevKind, open: bool, now: u64) {
        let Some(bi) = self.world.buildings.iter().position(|b| b.id == bid) else { return };
        let tiles = self.link_tiles(bi);
        match k {
            DevKind::Door | DevKind::Hatch | DevKind::Floodgate => {
                if !open && k != DevKind::Hatch && self.link_tile_jammed(&tiles) {
                    if let Err(p) = self.link.jammed.binary_search(&bid) {
                        self.link.jammed.insert(p, bid);
                        self.link.stats.jammed += 1;
                    }
                    if k == DevKind::Floodgate {
                        // Ensure it is open (it may have been asked to close while jammed).
                        if self.def_on(bid) == Some(false) {
                            self.defense_apply(bid, true, now);
                        }
                    }
                    return;
                }
                if let Ok(p) = self.link.jammed.binary_search(&bid) {
                    self.link.jammed.remove(p);
                }
                let on = if k == DevKind::Floodgate { open } else { !open };
                self.defense_apply(bid, on, now);
                if !open && k != DevKind::Hatch {
                    for &t in &tiles {
                        let p = self.world.pos_of(t as usize);
                        if self.liquid_at(p).0 > 0 {
                            self.set_liquid(p, 0, false);
                        }
                    }
                }
            }
            DevKind::Spikes => self.defense_apply(bid, !open, now),
            DevKind::Bridge => {
                let huge = self.link_huge_on(&tiles);
                if huge {
                    self.link.stats.bridges_stopped += 1;
                    if !open {
                        // Lowering onto a huge creature: the bridge comes apart.
                        self.remove_building(bid);
                        self.defense_scan();
                    }
                    return;
                }
                self.defense_apply(bid, open, now);
                if !open {
                    for &t in &tiles {
                        let p = self.world.pos_of(t as usize);
                        if self.liquid_at(p).0 > 0 {
                            self.set_liquid(p, 0, false);
                        }
                    }
                }
            }
            DevKind::GrateWall | DevKind::BarsVertical | DevKind::GrateFloor | DevKind::BarsFloor => {
                set_flag(&mut self.world.buildings[bi].flags, "OPEN", open);
                set_flag(&mut self.world.buildings[bi].flags, "CLOSED", !open);
                let wall = matches!(k, DevKind::GrateWall | DevKind::BarsVertical);
                for &t in &tiles {
                    let i = t as usize;
                    if wall {
                        self.nav.set_blocked(i, !open);
                    } else {
                        self.nav.set_floor_override(i, !open);
                    }
                    self.after_tile_change(i);
                }
                self.link_settle_nav();
            }
            DevKind::Gear => {
                set_flag(&mut self.world.buildings[bi].flags, "DISENGAGED", open);
                self.power.sig = 0;
                self.power.checked = false;
            }
            DevKind::TrackStop => {
                set_flag(&mut self.world.buildings[bi].flags, "DISABLED", open);
                self.carts_bld_rebuild();
            }
            DevKind::Cage | DevKind::Chain | DevKind::Support => {
                if open {
                    self.link_one_shot(bid, k, &tiles, now);
                }
            }
        }
        self.link.stats.changes += 1;
    }

    fn link_settle_nav(&mut self) {
        if self.nav.suspect {
            self.nav.rebuild_components();
            self.path_cache.clear();
            self.epoch_jobs += 1;
            self.epoch_haul += 1;
        }
    }

    pub(crate) fn link_tiles(&self, bi: usize) -> Vec<u32> {
        let b = &self.world.buildings[bi];
        let mut v = Vec::new();
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                let p = Pos::new(x, y, b.z);
                if b.contains(p) {
                    if let Some(i) = self.world.idx(p) {
                        v.push(i as u32);
                    }
                }
            }
        }
        v
    }

    /// A loose item (or, for doors, a creature) on one of the tiles.
    fn link_tile_jammed(&self, tiles: &[u32]) -> bool {
        for (i, it) in self.world.items.iter().enumerate() {
            if self.imeta[i].dead || !it.flags.on_ground || it.building >= 0 || it.holder >= 0 || it.container >= 0 {
                continue;
            }
            if let Some(t) = self.world.idx(it.pos) {
                if tiles.contains(&(t as u32)) {
                    return true;
                }
            }
        }
        false
    }

    fn link_huge_on(&self, tiles: &[u32]) -> bool {
        self.ai.iter().any(|a| {
            a.kind != Kind::Dead && tiles.contains(&a.pos) && {
                let u = &self.world.units[a.uidx];
                !u.flags.caged && u.body_size >= BRIDGE_STOP_SIZE
            }
        })
    }

    /// Cages release their occupants, chains their creature, supports come down.
    fn link_one_shot(&mut self, bid: i32, k: DevKind, tiles: &[u32], now: u64) {
        self.link.stats.one_shots += 1;
        if matches!(k, DevKind::Cage | DevKind::Chain) {
            for s in 0..self.ai.len() {
                let uidx = self.ai[s].uidx;
                let u = &self.world.units[uidx];
                if u.flags.dead || !tiles.contains(&self.ai[s].pos) {
                    continue;
                }
                if k == DevKind::Cage && !u.flags.caged {
                    continue;
                }
                if u.flags.caged {
                    self.world.units[uidx].flags.caged = false;
                    let kind = if self.world.units[uidx].flags.citizen { Kind::Worker } else if self.world.units[uidx].flags.animal || self.world.units[uidx].flags.tame { Kind::Animal } else { Kind::Other };
                    self.ai[s].kind = kind;
                    self.ai[s].wake = now + 1;
                    if self.world.units[uidx].flags.hostile {
                        let id = self.ai[s].id;
                        self.make_hostile(id);
                    }
                }
            }
        }
        self.remove_building(bid);
        self.defense_scan();
        if k == DevKind::Support {
            let mut changed: Vec<usize> = tiles.iter().map(|&t| t as usize).collect();
            for &t in tiles {
                if (t as usize) + self.nav.layer < self.world.tiles.len() {
                    changed.push(t as usize + self.nav.layer);
                }
            }
            self.cavein_check(&changed);
        }
        // The links to it are gone; the triggers keep their mechanisms.
        for m in self.def.mechs.iter_mut() {
            m.targets.retain(|&x| x != bid);
        }
        for b in self.world.buildings.iter_mut() {
            b.links.retain(|&x| x != bid);
        }
    }

    // ------------------------------------------------------------------ pressure plates

    /// Bring the plate list in line with the buildings (keeps the state of known plates).
    fn link_sync_plates(&mut self) {
        self.link.nb = self.world.buildings.len();
        let old = std::mem::take(&mut self.link.plates);
        let mut v: Vec<Plate> = Vec::new();
        for b in &self.world.buildings {
            if !(b.kind == "Trap" && b.subtype == "PressurePlate") || !b.built {
                continue;
            }
            let Some(t) = self.world.idx(Pos::new(b.x1, b.y1, b.z)) else { continue };
            if let Ok(k) = old.binary_search_by_key(&b.id, |p| p.bid) {
                let mut p = old[k].clone();
                Self::plate_settings_from(b, &mut p);
                v.push(p);
                continue;
            }
            let mut p = Plate { bid: b.id, tile: t as u32, false_since: u64::MAX, ..Default::default() };
            Self::plate_settings_from(b, &mut p);
            v.push(p);
        }
        v.sort_by_key(|p| p.bid);
        self.link.plates = v;
    }

    fn plate_settings_from(b: &Building, p: &mut Plate) {
        plate_settings_from(b, p)
    }
}

/// Triggers and ranges from the building (exported or set by the player).
pub(crate) fn plate_settings_from(b: &Building, p: &mut Plate) {
    {
        let f = &b.flags;
        let any = has(f, "PLATE_UNITS") || has(f, "PLATE_WATER") || has(f, "PLATE_MAGMA") || has(f, "PLATE_TRACK");
        p.units = !any || has(f, "PLATE_UNITS");
        p.water = has(f, "PLATE_WATER");
        p.magma = has(f, "PLATE_MAGMA");
        p.track = has(f, "PLATE_TRACK");
        p.citizens = has(f, "PLATE_CITIZENS") || has(f, "CITIZENS");
        p.resets = !any || has(f, "PLATE_RESETS");
        let mut r = b.plate.unwrap_or_default();
        if b.plate.is_none() {
            r = PlateInfo { unit_min: PLATE_UNIT_MIN, unit_max: PLATE_UNIT_MAX, water_min: 1, water_max: 7, magma_min: 1, magma_max: 7, track_min: 1, track_max: 0 };
        }
        p.r = r;
    }
}

/// Write plate settings into the building flags and ranges (unbuilt plates).
pub(crate) fn write_plate_flags(b: &mut Building, st: &crate::machines14_api::PlateSettings) {
    for (f, on) in [("PLATE_UNITS", st.units), ("PLATE_WATER", st.water), ("PLATE_MAGMA", st.magma), ("PLATE_TRACK", st.track), ("PLATE_CITIZENS", st.citizens), ("PLATE_RESETS", st.resets)] {
        set_flag(&mut b.flags, f, on);
    }
    set_flag(&mut b.flags, "CITIZENS", st.citizens);
    b.plate = Some(st.r);
}

impl Sim {

    /// [trap_entered hook] Unit `s` stepped onto plate tile `tile`.
    pub(crate) fn link_plate_enter(&mut self, s: u32, tile: u32) {
        for p in self.link.plates.iter_mut() {
            if p.tile == tile && !p.occ.contains(&s) {
                p.occ.push(s);
            }
        }
    }

    /// [minecarts hook] A cart of gross weight `w` (Γ) entered tile `t`.
    pub(crate) fn link_plate_cart(&mut self, t: u32, w: i32) {
        for p in self.link.plates.iter_mut() {
            if p.tile == t && p.track {
                p.cart_w = p.cart_w.max(w.max(1));
                p.last_w = w.max(1);
            }
        }
    }

    /// Safety net: creatures that got onto a plate without walking (flung, fell, spawned).
    fn link_rescan_plates(&mut self) {
        for s in 0..self.ai.len() {
            let a = &self.ai[s];
            if a.kind == Kind::Dead || !self.def.trap_tile(a.pos) {
                continue;
            }
            let pos = a.pos;
            for p in self.link.plates.iter_mut() {
                if p.tile == pos && !p.occ.contains(&(s as u32)) {
                    p.occ.push(s as u32);
                }
            }
        }
    }

    /// Does unit slot `s` press a plate? (`citizens`: the plate's citizens setting.)
    fn plate_counts(&self, s: usize, citizens: bool, now: u64) -> bool {
        let a = &self.ai[s];
        if a.kind == Kind::Dead {
            return false;
        }
        let u = &self.world.units[a.uidx];
        let fl = &u.flags;
        if fl.dead || fl.caged || fl.inactive {
            return false;
        }
        if self.combat.f.get(s).map(|f| f.uncon_until > now).unwrap_or(false) {
            return true; // unconscious creatures always trigger
        }
        if fl.merchant || fl.visitor || self.caverns_trapavoid(s) || self.link_trapavoid(&u.race) {
            return false;
        }
        let own = a.kind == Kind::Worker || fl.citizen || (fl.tame && !fl.hostile);
        !own || citizens
    }

    fn link_trapavoid(&self, race: &str) -> bool {
        // Races are few; the raws list is scanned only for units standing on plates.
        self.world
            .creature_raws
            .iter()
            .find(|c| c.token == race)
            .map(|c| c.flags.iter().chain(c.castes.iter().flat_map(|k| k.flags.iter())).any(|f| f == "TRAPAVOID"))
            .unwrap_or(race == "KOBOLD")
    }

    fn link_poll_plates(&mut self, now: u64) {
        let carts_here = !self.carts.carts.is_empty();
        for k in 0..self.link.plates.len() {
            let p = &self.link.plates[k];
            if p.occ.is_empty() && !p.on && !p.water && !p.magma && !(p.track && (p.cart_w > 0 || carts_here)) {
                continue;
            }
            let (tile, r, citizens) = (p.tile, p.r, p.citizens);
            let mut cond = false;
            if p.units && !p.occ.is_empty() {
                let mut occ = std::mem::take(&mut self.link.plates[k].occ);
                occ.retain(|&s| (s as usize) < self.ai.len() && self.ai[s as usize].pos == tile && self.ai[s as usize].kind != Kind::Dead);
                let mut sum: i64 = 0;
                let mut any = false;
                for &s in &occ {
                    if self.plate_counts(s as usize, citizens, now) {
                        any = true;
                        sum += self.world.units[self.ai[s as usize].uidx].body_size.max(1) as i64;
                    }
                }
                cond |= any && sum >= r.unit_min as i64 && sum <= r.unit_max as i64;
                self.link.plates[k].occ = occ;
            } else if !self.link.plates[k].occ.is_empty() {
                let ai = &self.ai;
                self.link.plates[k].occ.retain(|&s| (s as usize) < ai.len() && ai[s as usize].pos == tile);
            }
            let p = &self.link.plates[k];
            if p.water || p.magma {
                let (d, magma) = self.liquid_at(self.world.pos_of(tile as usize));
                let d = d as i32;
                if p.water && (!magma || d == 0) && d >= r.water_min && d <= r.water_max {
                    cond = true;
                }
                if p.magma && (magma || d == 0) && d >= r.magma_min && d <= r.magma_max {
                    cond = true;
                }
            }
            if p.track {
                let mut w = p.cart_w;
                if carts_here {
                    for c in &self.carts.carts {
                        if c.tile == tile && c.item >= 0 {
                            w = w.max(p.last_w.max(1));
                        }
                    }
                }
                let max = if r.track_max <= 0 { i32::MAX } else { r.track_max };
                if w > 0 && w >= r.track_min.max(1) && w <= max && self.track_bits(tile as usize) != 0 {
                    cond = true;
                }
                self.link.plates[k].cart_w = 0;
            }
            let p = &mut self.link.plates[k];
            let bid = p.bid;
            if cond {
                p.false_since = u64::MAX;
                if !p.on {
                    p.on = true;
                    let resets = p.resets;
                    self.link.stats.plates_on += 1;
                    self.def.stats.plates_triggered += 1;
                    self.plate_fire(bid, true);
                    if !resets {
                        self.plate_spend(bid);
                        return; // the plate list changed; continue next tick
                    }
                }
            } else if p.on {
                if p.false_since == u64::MAX {
                    p.false_since = now;
                }
                if now - p.false_since >= PLATE_CLOSE {
                    p.on = false;
                    p.false_since = u64::MAX;
                    self.link.stats.plates_off += 1;
                    self.plate_fire(bid, false);
                }
            }
        }
    }

    fn plate_fire(&mut self, bid: i32, on: bool) {
        let targets = match self.def.by_id.get(&bid) {
            Some(&mi) => {
                self.def.mechs[mi].on = on;
                self.def.mechs[mi].targets.clone()
            }
            None => self.world.building(bid).map(|b| b.links.clone()).unwrap_or_default(),
        };
        for t in targets {
            self.link_send(t, on);
        }
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == bid) {
            set_flag(&mut b.flags, "ACTIVE", !on); // DF: ACTIVE = ready
            set_flag(&mut b.flags, "PRESSED", on);
        }
    }

    /// A one-use plate deconstructs and destroys its mechanisms.
    fn plate_spend(&mut self, bid: i32) {
        self.link.stats.plates_spent += 1;
        let mechs: Vec<i32> = self
            .world
            .building(bid)
            .map(|b| b.items.iter().copied().filter(|id| self.item_idx.get(id).map(|&i| self.world.items[i as usize].kind == "TRAPPARTS").unwrap_or(false)).collect())
            .unwrap_or_default();
        for id in mechs {
            if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == bid) {
                b.items.retain(|&x| x != id);
            }
            self.destroy_item(id);
        }
        self.remove_building(bid);
        self.defense_scan();
        self.link_sync_plates();
    }

    // ------------------------------------------------------------------ links

    /// Can `trigger` be linked to `target`? Err with the reason.
    pub(crate) fn link_check(&self, trigger: i32, target: i32) -> Result<DevKind, String> {
        let g = self.world.building(trigger).ok_or("no such lever or pressure plate")?;
        if !(g.kind == "Trap" && (g.subtype == "Lever" || g.subtype == "PressurePlate")) {
            return Err("only levers and pressure plates can be linked".into());
        }
        if !g.built {
            return Err("the trigger is not built yet".into());
        }
        let t = self.world.building(target).ok_or("no such target")?;
        let k = DevKind::of(t).ok_or("this building cannot be linked to a mechanism")?;
        if !t.built {
            return Err("the target is not built yet".into());
        }
        if trigger == target {
            return Err("a trigger cannot be linked to itself".into());
        }
        if g.links.contains(&target) {
            return Err("already linked".into());
        }
        if self.link.jobs.iter().any(|j| j.trigger == trigger && j.target == target) {
            return Err("a link job is already queued".into());
        }
        if has(&t.flags, "FORBIDDEN") && !matches!(k, DevKind::Door | DevKind::Hatch) {
            return Err("the target is forbidden".into());
        }
        Ok(k)
    }

    /// Link right away (no mechanic, no mechanisms): tests, scripts, the link job's last step.
    pub(crate) fn link_now(&mut self, trigger: i32, target: i32) -> Result<(), String> {
        let k = self.link_check(trigger, target)?;
        for b in self.world.buildings.iter_mut() {
            if b.id == trigger && !b.links.contains(&target) {
                b.links.push(target);
            }
            if b.id == target && !b.links.contains(&trigger) {
                b.links.push(trigger);
            }
        }
        if self.def.by_id.get(&trigger).is_none() || self.def.by_id.get(&target).is_none() && matches!(k, DevKind::Door | DevKind::Hatch | DevKind::Floodgate | DevKind::Bridge | DevKind::Spikes) {
            self.defense_scan();
        }
        if let Some(&mi) = self.def.by_id.get(&trigger) {
            if !self.def.mechs[mi].targets.contains(&target) {
                self.def.mechs[mi].targets.push(target);
            }
        }
        // A linked door or hatch is shut: dwarves no longer open it, only the mechanism does.
        if matches!(k, DevKind::Door | DevKind::Hatch) && self.def_on(target) == Some(false) {
            let now = self.ticks;
            self.defense_apply(target, true, now);
        }
        self.link.stats.links_made += 1;
        Ok(())
    }

    /// Queue a link job (a mechanic installs 2 mechanisms).
    pub(crate) fn link_queue(&mut self, trigger: i32, target: i32) -> Result<(), String> {
        self.link_check(trigger, target)?;
        self.link.jobs.push(LinkJob { trigger, target, ..Default::default() });
        Ok(())
    }

    /// Free mechanisms (on the ground, not reserved, not forbidden).
    pub(crate) fn link_free_mechs(&self) -> Vec<(i32, u32)> {
        let mut v = Vec::new();
        for (i, it) in self.world.items.iter().enumerate() {
            if it.kind != "TRAPPARTS" {
                continue;
            }
            let m = &self.imeta[i];
            let f = &it.flags;
            if m.dead || m.reserved > 0 || m.hauling || it.holder >= 0 || it.building >= 0 || it.container >= 0 || !f.on_ground || f.forbid || f.in_job || f.construction {
                continue;
            }
            if let Some(t) = self.world.idx(it.pos) {
                v.push((it.id, t as u32));
            }
        }
        v
    }

    /// Where the mechanic stands to install at building `bid`.
    pub(crate) fn link_goals(&self, bid: i32) -> Goals {
        let mut g = Goals::default();
        let Some(bi) = self.world.buildings.iter().position(|b| b.id == bid) else { return g };
        let b = &self.world.buildings[bi];
        let own_only = matches!(b.kind.as_str(), "Door" | "Hatch");
        let tiles = self.link_tiles(bi);
        let tiles: Vec<u32> = if b.kind == "Bridge" { self.world.idx(b.center).map(|c| vec![c as u32]).unwrap_or(tiles) } else { tiles };
        for &t in &tiles {
            if self.nav.walkable(t as usize) {
                g.push(t);
            }
        }
        if g.n > 0 || own_only {
            return g;
        }
        for &t in &tiles {
            let (x, y, z) = self.nav.xyz(t as usize);
            for &(dx, dy) in &crate::nav::DIRS8 {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                    continue;
                }
                let q = self.nav.idx(nx as usize, ny as usize, z) as u32;
                if self.nav.walkable(q as usize) && !tiles.contains(&q) && !g.as_slice().contains(&q) {
                    g.push(q);
                }
            }
        }
        g
    }

    /// [decide hook] An idle mechanic takes a waiting link job (needs 2 reachable mechanisms).
    pub(crate) fn linkage_try_job(&mut self, s: usize, now: u64) -> bool {
        if self.link.jobs.is_empty() || self.ai[s].labors & crate::labor::MECHANIC == 0 {
            return false;
        }
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return false;
        }
        for k in 0..self.link.jobs.len() {
            let j = self.link.jobs[k];
            if j.claimed != NONE || j.retry_at > now {
                continue;
            }
            let reach = |sim: &Sim, bid: i32| sim.link_goals(bid).as_slice().iter().any(|&t| sim.nav.comp_root(t as usize) == c);
            if (j.phase < 2 && !reach(self, j.target)) || !reach(self, j.trigger) {
                self.link.jobs[k].retry_at = now + 500;
                continue;
            }
            let need = if j.phase < 2 { 2 } else { 1 };
            let mut free: Vec<(i32, u32)> = self.link_free_mechs().into_iter().filter(|&(_, t)| self.nav.comp_near(t as usize) == c).collect();
            if free.len() < need {
                self.link.jobs[k].retry_at = now + 500;
                continue;
            }
            free.sort_by_key(|x| x.0); // oldest first (item id order)
            let mut mech = j.mech;
            if j.phase < 2 {
                mech = [free[0].0, free[1].0];
            } else {
                mech[1] = free[0].0;
            }
            for &m in &mech {
                if m >= 0 && j.mech[0] != m {
                    self.reserve(m);
                }
            }
            let jj = &mut self.link.jobs[k];
            jj.mech = mech;
            jj.claimed = s as u32;
            jj.until = 0;
            jj.phase = if jj.phase < 2 { 0 } else { 2 };
            let first = if jj.phase == 0 { mech[0] } else { mech[1] };
            self.ai[s].task = Task::Link(j.trigger, j.target);
            let g = self.item_goals(first);
            self.request_path(s, g, now);
            return true;
        }
        false
    }

    /// [arrive hook] Fetch, carry and install the mechanisms.
    pub(crate) fn linkage_arrive(&mut self, s: usize, trigger: i32, target: i32, now: u64) {
        let Some(k) = self.link.jobs.iter().position(|j| j.trigger == trigger && j.target == target && j.claimed == s as u32) else {
            self.cancel_task(s);
            self.decide(s, now);
            return;
        };
        let j = self.link.jobs[k];
        let (item, at) = if j.phase < 2 { (j.mech[0], target) } else { (j.mech[1], trigger) };
        if self.world.building(at).is_none() {
            // An end is gone: the job is cancelled.
            self.cancel_task(s);
            self.link.jobs.retain(|x| !(x.trigger == trigger && x.target == target));
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
            let g = self.link_goals(at);
            self.request_path(s, g, now);
            return;
        }
        let goals = self.link_goals(at);
        if !goals.as_slice().contains(&self.ai[s].pos) {
            if goals.n == 0 {
                self.cancel_task(s);
                self.decide(s, now);
                return;
            }
            self.request_path(s, goals, now);
            return;
        }
        if j.until == 0 {
            let skill = self.world.units[self.ai[s].uidx].skills.iter().find(|x| x.id == "MECHANICS").map(|x| x.rating).unwrap_or(0);
            let work = (LINK_WORK * 20 / (20 + skill.clamp(0, 20) as u64)).max(10);
            self.link.jobs[k].until = now + work;
            self.ai[s].wake = now + work;
            return;
        }
        if now < j.until {
            self.ai[s].wake = j.until;
            return;
        }
        // Install the mechanism.
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != item);
        self.ai[s].carrying = -1;
        self.unreserve(item);
        self.haul_remove(item);
        let p = self.world.building(at).map(|b| b.center).unwrap_or_default();
        if let Some(&ii) = self.item_idx.get(&item) {
            let it = &mut self.world.items[ii as usize];
            it.holder = -1;
            it.pos = p;
            it.building = at;
            it.flags.in_inventory = false;
            it.flags.on_ground = false;
            it.flags.in_building = true;
            self.imeta[ii as usize].hauling = false;
        }
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == at) {
            b.items.push(item);
        }
        self.add_xp(s, "MECHANICS", crate::skills::XP_USE);
        if j.phase < 2 {
            let jj = &mut self.link.jobs[k];
            jj.phase = 2;
            jj.until = 0;
            let g = self.item_goals(j.mech[1]);
            if self.item_idx.contains_key(&j.mech[1]) {
                self.request_path(s, g, now);
            } else {
                self.cancel_task(s);
                self.decide(s, now);
            }
            return;
        }
        self.link.jobs.remove(k);
        let _ = self.link_now(trigger, target);
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    /// [cancel hook] Drop what is carried; the job waits for somebody else.
    pub(crate) fn linkage_cancel(&mut self, s: usize, trigger: i32, target: i32) {
        let now = self.ticks;
        let Some(k) = self.link.jobs.iter().position(|j| j.trigger == trigger && j.target == target && j.claimed == s as u32) else { return };
        let j = self.link.jobs[k];
        let carried = self.ai[s].carrying;
        if carried >= 0 && j.mech.contains(&carried) {
            let here = self.ai[s].pos;
            self.drop_item(s, carried, here);
        }
        for (n, &m) in j.mech.iter().enumerate() {
            if m >= 0 && !(n == 0 && j.phase >= 2) {
                self.unreserve(m);
            }
        }
        let jj = &mut self.link.jobs[k];
        jj.claimed = NONE;
        jj.until = 0;
        if jj.phase < 2 {
            jj.mech = [-1, -1];
            jj.phase = 0;
        } else {
            jj.mech[1] = -1;
            jj.phase = 2;
        }
        jj.retry_at = now + 200;
    }

    // ------------------------------------------------------------------ saves, hash

    pub(crate) fn link_hash(&self, mix: &mut dyn FnMut(u64)) {
        for t in &self.link.timers {
            mix(t.due ^ (t.bid as u64) << 40 ^ (t.want as u64) << 63);
        }
        for p in &self.link.plates {
            mix((p.bid as u64) << 8 ^ p.on as u64 ^ p.false_since.wrapping_mul(31));
        }
        for j in &self.link.jobs {
            mix((j.trigger as u64) << 32 ^ j.target as u64 ^ (j.phase as u64) << 60 ^ (j.claimed as u64) << 20);
        }
        for &b in &self.link.jammed {
            mix(b as u64 ^ 0x7A44);
        }
    }

    /// Rebuild caches after an exact load (nothing beyond the scratch buffer today).
    pub(crate) fn link_after_load(&mut self) {
        self.link.scratch.clear();
    }
}

#[cfg(test)]
#[path = "linkage_tests.rs"]
mod tests;
