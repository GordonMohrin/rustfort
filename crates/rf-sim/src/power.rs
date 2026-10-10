//! [gaps10] Machines and power (round 10): screw pumps, water wheels, windmills, axles, gear
//! assemblies, rollers and millstones.
//!
//! # DF rules implemented (wiki v50: Power, Screw pump, Water wheel, Windmill, Gear assembly,
//! Axle, Millstone)
//! - **Power** per connected machine network: a water wheel makes net +90 when active (100 minus
//!   the 10 it needs to turn) or -10 when not; a windmill +40, +20 or (rarely) +0 depending on
//!   the wind. Axles use 1 per tile (horizontal and vertical), gear assemblies 5, screw pumps
//!   10, millstones 10, rollers 2. A network runs its machines when it makes at least what it
//!   uses.
//! - **Connections:** gear assemblies connect to any machine on the six orthogonal tiles
//!   (N, S, E, W, above, below); axles only along their axis (horizontal: beyond both ends,
//!   vertical: above and below), so a turn needs a gear assembly. Two machines connect when
//!   each reaches a tile of the other.
//! - **Water wheels** need flowing water below them; **windmills** need wind (outside).
//! - **Screw pumps** are 2x1, built with a direction ("pump from north" etc.); "the fluid is
//!   taken from one level below the tile next to the input" and "appears in the tile adjacent
//!   to the output side"; the intake needs at least 2/7 and a running pump "attempts to pump
//!   everything from the inlet tile to the outlet tile on every turn"; the output is never
//!   forced above the pump's level (no pressure). A pump runs from power (10) or by hand: a
//!   dwarf with the Operate pump labor works an `OperatePump` job at the pump.
//! - **Millstones** mill only while powered.
//!
//! # Estimates
//! - Wind: +40 on 15 of 20 days, +20 on 4, +0 on 1, for a windmill whose centre is outside.
//! - A water wheel is active while water (1/7 or more) under one of its tiles moved during the
//!   last 200 ticks (DF's "flowing"); still water does not turn it.
//! - Ports of other machines: water wheels at both ends of their long axis and above the
//!   centre; windmills below the centre; pumps, millstones and rollers on their four sides
//!   (pumps and millstones also above).
//! - Manual pumping: one job lasts `PUMP_TICKS` (then the repeat job is free again), 30 XP to
//!   Pump operating; a manual pump runs only while somebody works it.
//! - Gear assemblies are always engaged (disengaging by lever is not modelled); minecarts and
//!   rollers moving carts are not modelled (rollers only use power).

use crate::path::Goals;
use crate::rng::Rng;
use crate::{Sim, Task, NONE};
use rf_core::{tile_flags, Pos};

/// Power a water wheel makes and needs (wiki: net +90 when active, -10 when not).
pub const WHEEL_POWER: i32 = 100;
pub const WHEEL_USE: i32 = 10;
pub const AXLE_USE: i32 = 1;
pub const GEAR_USE: i32 = 5;
pub const PUMP_USE: i32 = 10;
pub const MILLSTONE_USE: i32 = 10;
pub const ROLLER_USE: i32 = 2;
/// Length of one manual "Operate pump" job (estimate).
pub const PUMP_TICKS: u64 = 600;
/// Water that moved this recently under a wheel counts as flowing (estimate).
const FLOW_FRESH: u64 = 200;
const POWER_PERIOD: u64 = 100;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)] // [saves11]
pub enum MachineKind {
    WaterWheel,
    Windmill,
    GearAssembly,
    AxleHorizontal,
    AxleVertical,
    ScrewPump,
    Millstone,
    Rollers,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)] // [saves11]
pub(crate) struct Machine {
    pub bid: i32,
    pub kind: MachineKind,
    pub tiles: Vec<u32>,
    pub ports: Vec<u32>,
    pub net: u32,
    /// Pumps: intake tile (one z below, next to the input end) and output tile.
    pub intake: u32,
    pub output: u32,
    /// Water wheels: tiles under the wheel.
    pub under: Vec<u32>,
}

/// One machine network.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct PowerNet {
    pub produced: i32,
    pub used: i32,
    pub powered: bool,
}

/// Manual pump work: the pump's building id, who works it, until when.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct PumpWork {
    pub bid: i32,
    pub claimed: u32,
    pub until: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct Power {
    pub sig: u64,
    pub checked: bool,
    pub machines: Vec<Machine>,
    pub nets: Vec<PowerNet>,
    /// Tiles under water wheels (sorted) and the tick water last moved there.
    pub under: Vec<u32>,
    pub flow_at: Vec<u64>,
    pub work: Vec<PumpWork>,
    pub stats: PowerStats,
}

/// Counters for reports and tests.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct PowerStats {
    pub machines: usize,
    pub networks: usize,
    /// Liquid moved by screw pumps, in sevenths.
    pub pumped: u64,
    pub manual_jobs: u64,
}

fn is_machine(b: &rf_core::Building) -> Option<MachineKind> {
    Some(match b.kind.as_str() {
        "WaterWheel" => MachineKind::WaterWheel,
        "Windmill" => MachineKind::Windmill,
        "GearAssembly" => MachineKind::GearAssembly,
        "AxleHorizontal" => MachineKind::AxleHorizontal,
        "AxleVertical" => MachineKind::AxleVertical,
        "ScrewPump" => MachineKind::ScrewPump,
        "Rollers" => MachineKind::Rollers,
        "Workshop" if b.subtype == "Millstone" => MachineKind::Millstone,
        _ => return None,
    })
}

fn find(p: &mut [u32], mut a: u32) -> u32 {
    while p[a as usize] != a {
        p[a as usize] = p[p[a as usize] as usize];
        a = p[a as usize];
    }
    a
}

impl Sim {
    // ------------------------------------------------------------------ public API

    /// Power networks: (produced, used, powered) per network, and the counters.
    pub fn power_report(&self) -> (Vec<PowerNet>, PowerStats) {
        (self.power.nets.clone(), self.power.stats)
    }

    /// Is this machine (building id) on a network with enough power?
    pub fn machine_powered(&self, bid: i32) -> bool {
        self.power.machines.iter().find(|m| m.bid == bid).map(|m| self.power.nets.get(m.net as usize).map(|n| n.powered).unwrap_or(false)).unwrap_or(false)
    }

    /// Queue (or cancel) the repeating "Operate pump" job at a screw pump (DF: the pump's
    /// "Operate pump" task); a dwarf with the Operate pump labor works it by hand.
    pub fn set_pump_manual(&mut self, bid: i32, on: bool) {
        let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == bid && b.kind == "ScrewPump") else { return };
        b.jobs.retain(|j| j.job != "OperatePump");
        if on {
            b.jobs.push(rf_core::BuildingJob { job: "OperatePump".into(), repeat: true, order_id: -1, ..Default::default() });
        }
        self.power.sig = 0; // re-read the jobs
    }

    // ------------------------------------------------------------------ build

    /// Signature of the machine buildings (rebuild when it changes).
    fn power_signature(&self) -> u64 {
        let mut h = 0xC0FFEEu64;
        for b in &self.world.buildings {
            if is_machine(b).is_some() {
                let jobs = b.jobs.iter().filter(|j| j.job == "OperatePump" && !j.suspended).count() as u64;
                h = crate::rng::splitmix(h ^ (b.id as u64) << 8 ^ (b.built as u64) << 40 ^ jobs << 41 ^ (b.stage as u64 & 0xff));
            }
        }
        h
    }

    fn building_tiles(&self, b: &rf_core::Building) -> Vec<u32> {
        let mut v = Vec::new();
        let w = (b.x2 - b.x1 + 1).max(1) as usize;
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                let k = (y - b.y1) as usize * w + (x - b.x1) as usize;
                if !b.extents.is_empty() && b.extents.get(k).copied().unwrap_or(0) == 0 {
                    continue;
                }
                if let Some(t) = self.world.idx(Pos::new(x, y, b.z)) {
                    v.push(t as u32);
                }
            }
        }
        v
    }

    fn tile_at(&self, p: Pos, dx: i16, dy: i16, dz: i16) -> Option<u32> {
        self.world.idx(Pos::new(p.x + dx, p.y + dy, p.z + dz)).map(|t| t as u32)
    }

    /// Rebuild machines, ports and networks from the buildings.
    pub(crate) fn power_rebuild(&mut self) {
        let mut ms: Vec<Machine> = Vec::new();
        for b in &self.world.buildings {
            let Some(kind) = is_machine(b) else { continue };
            if !b.built {
                continue;
            }
            let tiles = self.building_tiles(b);
            if tiles.is_empty() {
                continue;
            }
            let (lo, hi) = (Pos::new(b.x1, b.y1, b.z), Pos::new(b.x2, b.y2, b.z));
            let ew = b.x2 > b.x1;
            let mut ports = Vec::new();
            let mut push = |o: Option<u32>| {
                if let Some(t) = o {
                    ports.push(t);
                }
            };
            let center = b.center;
            let (mut intake, mut output, mut under) = (NONE, NONE, Vec::new());
            match kind {
                MachineKind::GearAssembly => {
                    for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                        push(self.tile_at(lo, dx, dy, dz));
                    }
                }
                MachineKind::AxleHorizontal => {
                    // Along the axis: the direction string (`NS`/`EW`) or the longer side.
                    let ns = b.direction.eq_ignore_ascii_case("NS") || b.direction.eq_ignore_ascii_case("N") || (!ew && b.y2 > b.y1);
                    if ns {
                        push(self.tile_at(lo, 0, -1, 0));
                        push(self.tile_at(hi, 0, 1, 0));
                    } else {
                        push(self.tile_at(lo, -1, 0, 0));
                        push(self.tile_at(hi, 1, 0, 0));
                    }
                }
                MachineKind::AxleVertical => {
                    push(self.tile_at(lo, 0, 0, 1));
                    push(self.tile_at(lo, 0, 0, -1));
                }
                MachineKind::WaterWheel => {
                    if ew {
                        push(self.tile_at(lo, -1, 0, 0));
                        push(self.tile_at(hi, 1, 0, 0));
                    } else {
                        push(self.tile_at(lo, 0, -1, 0));
                        push(self.tile_at(hi, 0, 1, 0));
                    }
                    push(self.tile_at(center, 0, 0, 1));
                    for &t in &tiles {
                        let p = self.world.pos_of(t as usize);
                        if let Some(u) = self.tile_at(p, 0, 0, -1) {
                            under.push(u);
                        }
                    }
                }
                MachineKind::Windmill => push(self.tile_at(center, 0, 0, -1)),
                MachineKind::ScrewPump | MachineKind::Millstone | MachineKind::Rollers => {
                    for &t in &tiles {
                        let p = self.world.pos_of(t as usize);
                        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                            push(self.tile_at(p, dx, dy, 0));
                        }
                        if kind != MachineKind::Rollers {
                            push(self.tile_at(p, 0, 0, 1));
                        }
                    }
                    if kind == MachineKind::ScrewPump {
                        // "Pump from north": the input end is north, the output south.
                        let d = b.direction.to_ascii_lowercase();
                        let (fx, fy): (i16, i16) = if d.contains("north") || d == "n" {
                            (0, -1)
                        } else if d.contains("south") || d == "s" {
                            (0, 1)
                        } else if d.contains("east") || d == "e" {
                            (1, 0)
                        } else if d.contains("west") || d == "w" {
                            (-1, 0)
                        } else if ew {
                            (-1, 0)
                        } else {
                            (0, -1)
                        };
                        let input = if fx < 0 || fy < 0 { lo } else { hi };
                        let out_end = if fx < 0 || fy < 0 { hi } else { lo };
                        intake = self.tile_at(input, fx, fy, -1).unwrap_or(NONE);
                        output = self.tile_at(out_end, -fx, -fy, 0).unwrap_or(NONE);
                    }
                }
            }
            ports.retain(|t| !tiles.contains(t));
            ms.push(Machine { bid: b.id, kind, tiles, ports, net: 0, intake, output, under });
        }
        // Tile -> machine, then mutual connections.
        let mut at: Vec<(u32, u32)> = ms.iter().enumerate().flat_map(|(i, m)| m.tiles.iter().map(move |&t| (t, i as u32))).collect();
        at.sort_unstable();
        let mut parent: Vec<u32> = (0..ms.len() as u32).collect();
        for a in 0..ms.len() {
            for &p in &ms[a].ports {
                let Ok(k) = at.binary_search_by_key(&p, |e| e.0) else { continue };
                let b = at[k].1 as usize;
                if b == a || !ms[b].ports.iter().any(|q| ms[a].tiles.contains(q)) {
                    continue;
                }
                let (ra, rb) = (find(&mut parent, a as u32), find(&mut parent, b as u32));
                if ra != rb {
                    parent[ra.max(rb) as usize] = ra.min(rb);
                }
            }
        }
        let mut roots: Vec<u32> = Vec::new();
        for i in 0..ms.len() {
            let r = find(&mut parent, i as u32);
            let n = match roots.iter().position(|&x| x == r) {
                Some(n) => n,
                None => {
                    roots.push(r);
                    roots.len() - 1
                }
            };
            ms[i].net = n as u32;
        }
        let mut under: Vec<u32> = ms.iter().flat_map(|m| m.under.iter().copied()).collect();
        under.sort_unstable();
        under.dedup();
        // Keep the flow memory of tiles that stay under a wheel.
        let old: Vec<(u32, u64)> = self.power.under.iter().copied().zip(self.power.flow_at.iter().copied()).collect();
        self.power.flow_at = under.iter().map(|t| old.iter().find(|e| e.0 == *t).map(|e| e.1).unwrap_or(0)).collect();
        self.power.under = under;
        // Manual pump work: keep claims of pumps that still have the job.
        let manual: Vec<i32> = self
            .world
            .buildings
            .iter()
            .filter(|b| b.kind == "ScrewPump" && b.built && b.jobs.iter().any(|j| j.job == "OperatePump" && !j.suspended))
            .map(|b| b.id)
            .collect();
        let mut work: Vec<PumpWork> = std::mem::take(&mut self.power.work).into_iter().filter(|w| manual.contains(&w.bid)).collect();
        for bid in manual {
            if !work.iter().any(|w| w.bid == bid) {
                work.push(PumpWork { bid, claimed: NONE, until: 0 });
            }
        }
        self.power.work = work;
        self.power.nets = vec![PowerNet::default(); roots.len()];
        self.power.stats.machines = ms.len();
        self.power.stats.networks = roots.len();
        self.power.machines = ms;
    }

    /// Recompute production and use of every network.
    fn power_balance(&mut self, now: u64) {
        let mut nets = vec![PowerNet::default(); self.power.nets.len()];
        let day = now / rf_core::TICKS_PER_DAY as u64;
        for m in &self.power.machines {
            let n = &mut nets[m.net as usize];
            match m.kind {
                MachineKind::WaterWheel => {
                    let flowing = m.under.iter().any(|&t| {
                        let k = self.power.under.binary_search(&t).ok();
                        let fresh = k.map(|k| self.power.flow_at[k] + FLOW_FRESH > now && self.power.flow_at[k] > 0).unwrap_or(false);
                        let (d, magma) = self.liquid_at(self.world.pos_of(t as usize));
                        fresh && d > 0 && !magma
                    });
                    n.used += WHEEL_USE;
                    if flowing {
                        n.produced += WHEEL_POWER;
                    }
                }
                MachineKind::Windmill => {
                    let c = self.world.buildings.iter().find(|b| b.id == m.bid).map(|b| b.center);
                    let outside = c.and_then(|c| self.world.tile(c)).map(|t| t.flags & tile_flags::OUTSIDE != 0).unwrap_or(false);
                    if outside {
                        let mut r = Rng::new(self.seed ^ crate::rng::splitmix(day ^ (m.bid as u64) << 20 ^ 0x3157));
                        n.produced += match r.below(20) {
                            0 => 0,
                            1..=4 => 20,
                            _ => 40,
                        };
                    }
                }
                MachineKind::GearAssembly => n.used += GEAR_USE,
                MachineKind::AxleHorizontal | MachineKind::AxleVertical => n.used += AXLE_USE * m.tiles.len() as i32,
                MachineKind::ScrewPump => n.used += PUMP_USE,
                MachineKind::Millstone => n.used += MILLSTONE_USE,
                MachineKind::Rollers => n.used += ROLLER_USE * m.tiles.len() as i32,
            }
        }
        for n in &mut nets {
            n.powered = n.produced > 0 && n.produced >= n.used;
        }
        self.power.nets = nets;
    }

    // ------------------------------------------------------------------ tick

    /// [gaps10] Every tick: rebuild when machines changed (checked every 100 ticks), balance the
    /// networks, run the pumps.
    pub(crate) fn power_tick(&mut self, now: u64) {
        if now % POWER_PERIOD == 41 || !self.power.checked {
            self.power.checked = true;
            let sig = self.power_signature();
            if sig != self.power.sig {
                self.power.sig = sig;
                self.power_rebuild();
            }
            if !self.power.machines.is_empty() {
                self.power_balance(now);
            }
        }
        if self.power.machines.is_empty() {
            return;
        }
        for k in 0..self.power.machines.len() {
            let m = &self.power.machines[k];
            if m.kind != MachineKind::ScrewPump || m.intake == NONE || m.output == NONE {
                continue;
            }
            let powered = self.power.nets.get(m.net as usize).map(|n| n.powered).unwrap_or(false);
            let manned = self.power.work.iter().any(|w| w.bid == m.bid && w.claimed != NONE && w.until > now);
            if powered || manned {
                let (a, b) = (m.intake, m.output);
                self.pump_step(a as usize, b as usize);
            }
        }
    }

    /// Move everything that fits from the intake to the output (intake at least 2/7).
    fn pump_step(&mut self, a: usize, b: usize) {
        let (pa, pb) = (self.world.pos_of(a), self.world.pos_of(b));
        let (da, ma) = self.liquid_at(pa);
        if da < 2 || !self.fluids.holds(&self.world.tiles, b) {
            return;
        }
        let (db, mb) = self.liquid_at(pb);
        if db > 0 && mb != ma {
            return; // water and magma do not mix in the pump (ESTIMATE)
        }
        let m = da.min(7 - db);
        if m == 0 {
            return;
        }
        self.set_liquid(pa, da - m, ma);
        self.set_liquid(pb, db + m, ma);
        self.power.stats.pumped += m as u64;
    }

    /// [fluids hook] Liquid moved in tile `t` this tick (only called for tiles under wheels).
    pub(crate) fn power_flow_seen(&mut self, t: u32, now: u64) {
        if let Ok(k) = self.power.under.binary_search(&t) {
            self.power.flow_at[k] = now;
        }
    }

    /// Millstones mill only while powered.
    pub(crate) fn power_shop_ok(&self, bid: i32, subtype: &str) -> bool {
        subtype != "Millstone" || self.machine_powered(bid)
    }

    // ------------------------------------------------------------------ manual pumping

    /// [decide hook] An idle dwarf with the Operate pump labor takes a free manual pump job.
    pub(crate) fn power_try_job(&mut self, s: usize, now: u64) -> bool {
        if self.power.work.is_empty() || self.ai[s].labors & crate::labor::OPERATE_PUMP == 0 {
            return false;
        }
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return false;
        }
        for k in 0..self.power.work.len() {
            let w = self.power.work[k];
            if w.claimed != NONE || self.machine_powered(w.bid) {
                continue;
            }
            let goals = self.pump_goals(w.bid);
            if !goals.as_slice().iter().any(|&t| self.nav.comp_root(t as usize) == c && !self.alert_blocks(s, t as usize)) {
                continue;
            }
            self.power.work[k].claimed = s as u32;
            self.power.work[k].until = 0;
            self.ai[s].task = Task::OperatePump(w.bid);
            self.request_path(s, goals, now);
            return true;
        }
        false
    }

    /// Where the operator stands: walkable tiles of the pump, else next to it.
    fn pump_goals(&self, bid: i32) -> Goals {
        let mut g = Goals::default();
        let Some(m) = self.power.machines.iter().find(|m| m.bid == bid) else { return g };
        for &t in &m.tiles {
            if self.nav.walkable(t as usize) {
                g.push(t);
            }
        }
        if g.n == 0 {
            for &t in &m.ports {
                if self.nav.walkable(t as usize) && self.world.pos_of(t as usize).z == self.world.pos_of(m.tiles[0] as usize).z {
                    g.push(t);
                }
            }
        }
        g
    }

    /// [arrive hook] At the pump: work it for `PUMP_TICKS`, then free the job again.
    pub(crate) fn power_arrive_pump(&mut self, s: usize, bid: i32, now: u64) {
        let Some(k) = self.power.work.iter().position(|w| w.bid == bid && w.claimed == s as u32) else {
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        };
        let goals = self.pump_goals(bid);
        if !goals.as_slice().contains(&self.ai[s].pos) {
            self.request_path(s, goals, now);
            return;
        }
        let until = self.power.work[k].until;
        if until == 0 {
            self.power.work[k].until = now + PUMP_TICKS;
            self.ai[s].wake = now + PUMP_TICKS;
            return;
        }
        if now < until {
            self.ai[s].wake = until;
            return;
        }
        self.power.work[k].claimed = NONE;
        self.power.work[k].until = 0;
        self.power.stats.manual_jobs += 1;
        self.add_xp(s, "OPERATE_PUMP", crate::skills::XP_USE);
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    /// [cancel hook] The operator gave up: the job waits for somebody else.
    pub(crate) fn power_cancel_pump(&mut self, s: usize, bid: i32) {
        if let Some(w) = self.power.work.iter_mut().find(|w| w.bid == bid && w.claimed == s as u32) {
            w.claimed = NONE;
            w.until = 0;
        }
    }

    /// State hash part: running pumps and manual work.
    pub(crate) fn power_hash(&self, mix: &mut impl FnMut(u64)) {
        mix(self.power.stats.pumped);
        for w in &self.power.work {
            mix(((w.bid as u64) << 32) ^ w.claimed as u64 ^ w.until << 8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{Building, World};

    fn bld(id: i32, kind: &str, sub: &str, x1: i16, y1: i16, x2: i16, y2: i16, z: i16, dir: &str) -> Building {
        Building {
            id,
            kind: kind.into(),
            subtype: sub.into(),
            x1,
            y1,
            x2,
            y2,
            z,
            center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, z),
            mat: 0,
            stage: -1,
            built: true,
            owner: -1,
            direction: dir.into(),
            ..Default::default()
        }
    }

    fn sim() -> (Sim, i16) {
        let mut w = World::synthetic(48, 48, 20);
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
        }
        let z = w.units[0].pos.z;
        let mut sim = Sim::new(w, 61);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        (sim, z)
    }

    #[test]
    fn axles_and_gears_connect_by_the_wiki_rules() {
        let (mut sim, z) = sim();
        // Gear at (10,10), E-W axle (11..13,10), gear (14,10), pump (14,11)-(14,12) N-S.
        sim.world.buildings.push(bld(9001, "GearAssembly", "", 10, 10, 10, 10, z, ""));
        sim.world.buildings.push(bld(9002, "AxleHorizontal", "", 11, 10, 13, 10, z, ""));
        sim.world.buildings.push(bld(9003, "GearAssembly", "", 14, 10, 14, 10, z, ""));
        sim.world.buildings.push(bld(9004, "ScrewPump", "", 14, 11, 14, 12, z, "FromNorth"));
        // An axle beside (not along) the first one does not connect: N-S axle at (12,11)-(12,12).
        sim.world.buildings.push(bld(9005, "AxleHorizontal", "", 12, 11, 12, 12, z, ""));
        sim.power_rebuild();
        let net = |sim: &Sim, bid: i32| sim.power.machines.iter().find(|m| m.bid == bid).unwrap().net;
        assert_eq!(net(&sim, 9001), net(&sim, 9002));
        assert_eq!(net(&sim, 9002), net(&sim, 9003));
        assert_eq!(net(&sim, 9003), net(&sim, 9004), "gear powers the pump beside it");
        assert_ne!(net(&sim, 9005), net(&sim, 9002), "axles connect only along their axis");
        // No producer: nothing runs; usage gear 5 + 5, axle 3, pump 10.
        sim.power_balance(0);
        let n = sim.power.nets[net(&sim, 9001) as usize];
        assert_eq!((n.produced, n.used, n.powered), (0, 23, false));
    }

    #[test]
    fn water_wheel_in_flowing_water_powers_a_pump() {
        let (mut sim, z) = sim();
        // Wheel (20..22, 30) over water at z-1 that keeps moving (a source next to a drain).
        let below = z - 1;
        for x in 19..=23 {
            sim.set_liquid(Pos::new(x, 30, below), 0, false);
        }
        sim.world.buildings.push(bld(9101, "WaterWheel", "", 20, 30, 22, 30, z, ""));
        sim.world.buildings.push(bld(9102, "GearAssembly", "", 23, 30, 23, 30, z, ""));
        sim.world.buildings.push(bld(9103, "ScrewPump", "", 24, 30, 25, 30, z, "FromWest"));
        sim.power_rebuild();
        sim.power.sig = sim.power_signature();
        sim.power_balance(sim.ticks);
        assert!(!sim.machine_powered(9103), "a dry wheel turns nothing");
        // Still water does not turn it either.
        sim.set_liquid(Pos::new(21, 30, below), 3, false);
        let t = sim.world.idx(Pos::new(21, 30, below)).unwrap() as u32;
        sim.power_balance(sim.ticks + 1_000);
        assert!(!sim.machine_powered(9103));
        // Flowing water: wheel +100 -10, gear -5, pump -10.
        let now = sim.ticks + 1_000;
        sim.power_flow_seen(t, now);
        sim.power_balance(now + 10);
        assert!(sim.machine_powered(9103), "{:?}", sim.power.nets);
        let n = sim.power.nets[0];
        assert_eq!((n.produced, n.used), (100, 25));
    }

    /// Synthetic map with two dug cisterns (sources) one level below the surface.
    fn surface_sim(seed: u64) -> (Sim, i16) {
        let mut w = World::synthetic(48, 48, 20);
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
        }
        let sz = (20i16 * 2 / 3).max(2);
        let floor = w.tiletypes.by_name("StoneFloor1").unwrap();
        for x in [19, 29] {
            let i = w.idx(Pos::new(x, 12, sz - 1)).unwrap();
            w.tiles[i].tt = floor;
        }
        let mut sim = Sim::new(w, seed);
        sim.set_invasions(false);
        sim.set_moods_enabled(false);
        sim.add_liquid_source(Pos::new(19, 12, sz - 1), false);
        sim.add_liquid_source(Pos::new(29, 12, sz - 1), false);
        (sim, sz)
    }

    #[test]
    fn wind_powered_pump_lifts_water_one_level() {
        let (mut sim, sz) = surface_sim(62);
        // Windmill (centre (20,11,sz+1), outside) -> gear (20,11,sz) -> pump (20..21,12) from west.
        sim.world.buildings.push(bld(9201, "ScrewPump", "", 20, 12, 21, 12, sz, "FromWest"));
        sim.world.buildings.push(bld(9202, "GearAssembly", "", 20, 11, 20, 11, sz, ""));
        sim.world.buildings.push(bld(9203, "Windmill", "", 19, 10, 21, 12, sz + 1, ""));
        sim.run_ticks(150);
        let m = sim.power.machines.iter().find(|m| m.bid == 9201).unwrap().clone();
        assert_eq!(sim.world.pos_of(m.intake as usize), Pos::new(19, 12, sz - 1), "one level below the tile next to the input");
        assert_eq!(sim.world.pos_of(m.output as usize), Pos::new(22, 12, sz), "the tile beyond the output end");
        assert!(sim.machine_powered(9201), "{:?}", sim.power.nets);
        let pumped = sim.power.stats.pumped;
        assert!(pumped >= 7, "the powered pump runs: {pumped}");
        let up: u32 = (16..30).flat_map(|x| (6..18).map(move |y| (x, y))).map(|(x, y)| sim.liquid_at(Pos::new(x, y, sz)).0 as u32).sum();
        assert!(up > 0, "water arrived on the pump's level");
    }

    #[test]
    fn a_dwarf_operates_a_pump_by_hand() {
        let (mut sim, sz) = surface_sim(63);
        sim.world.buildings.push(bld(9204, "ScrewPump", "", 30, 12, 31, 12, sz, "FromWest"));
        sim.run_ticks(150);
        assert!(!sim.machine_powered(9204), "no power");
        assert_eq!(sim.power.stats.pumped, 0, "an idle pump moves nothing");
        for a in &mut sim.ai {
            a.labors |= crate::labor::OPERATE_PUMP;
        }
        sim.set_pump_manual(9204, true);
        let mut worked = false;
        for _ in 0..60 {
            sim.run_ticks(100);
            worked |= sim.power.work.iter().any(|w| w.claimed != NONE && w.until > sim.ticks);
            if sim.power.stats.pumped > 0 {
                break;
            }
        }
        assert!(worked, "somebody operates the pump");
        assert!(sim.liquid_at(Pos::new(32, 12, sz)).0 > 0 || sim.power.stats.pumped > 0, "the hand pump delivers water");
        sim.set_pump_manual(9204, false);
        sim.run_ticks(200);
        assert!(sim.power.work.is_empty(), "the job is gone");
    }
}
