//! [machines14] Machine details on top of `power` (wiki v50: Power, Gear assembly, Windmill,
//! Water wheel, Screw pump, Roller, Machine component) and the construction recipes of machines
//! and mechanism devices.
//!
//! # DF rules implemented
//! - A **disengaged** gear assembly (toggled by a linked lever or plate) transmits nothing.
//! - **Windmills** connect through the 12 tiles around their 3x3 as well as below the centre.
//! - **Water wheels** connect at the tile beside their centre, across the axis (the ends and
//!   above the centre stay as before, ESTIMATE), and need at least **4/7** water.
//! - **Screw pumps** connect below too (pump stacks), keep running **49 ticks** after losing
//!   power, run **newest first**, need an open column above the intake (no floor, stairs or
//!   wall), make **obsidian** when pumping into the other liquid, and deliver clean water.
//! - **Rollers** longer than one tile connect only across their direction.
//! - **Floors** between two levels block vertical power transfer.

use crate::Sim;
use rf_core::{Building, Pos};

/// Minimum water depth under a water wheel (wiki Water wheel: 4/7).
pub const WHEEL_MIN_DEPTH: u8 = 4;
/// A pump runs this long after its network loses power (wiki Screw pump: 1 tick of power =
/// 50 ticks of pumping).
pub const PUMP_RUN_ON: u64 = 49;

fn has(flags: &[String], f: &str) -> bool {
    flags.iter().any(|x| x.eq_ignore_ascii_case(f))
}

pub(crate) fn gear_disengaged(b: &Building) -> bool {
    b.kind == "GearAssembly" && has(&b.flags, "DISENGAGED")
}

/// Extra state mixed into the power signature (gear engaged or not).
pub(crate) fn machine_sig_extra(h: u64, b: &Building) -> u64 {
    if gear_disengaged(b) {
        crate::rng::splitmix(h ^ 0xD15E)
    } else {
        h
    }
}

fn tile(sim: &Sim, p: Pos, dx: i16, dy: i16, dz: i16) -> Option<u32> {
    sim.world.idx(Pos::new(p.x + dx, p.y + dy, p.z + dz)).map(|t| t as u32)
}

/// The tiles beside a water wheel's centre, across its axis.
pub(crate) fn wheel_center_ports(sim: &Sim, b: &Building) -> Vec<u32> {
    let c = b.center;
    let ew = b.x2 > b.x1;
    let v = if ew { [tile(sim, c, 0, -1, 0), tile(sim, c, 0, 1, 0)] } else { [tile(sim, c, -1, 0, 0), tile(sim, c, 1, 0, 0)] };
    v.into_iter().flatten().collect()
}

/// The 12 tiles orthogonally around a windmill's 3x3 (same level).
pub(crate) fn windmill_ring(sim: &Sim, b: &Building) -> Vec<u32> {
    let mut v = Vec::with_capacity(12);
    for x in b.x1..=b.x2 {
        v.extend(sim.world.idx(Pos::new(x, b.y1 - 1, b.z)).map(|t| t as u32));
        v.extend(sim.world.idx(Pos::new(x, b.y2 + 1, b.z)).map(|t| t as u32));
    }
    for y in b.y1..=b.y2 {
        v.extend(sim.world.idx(Pos::new(b.x1 - 1, y, b.z)).map(|t| t as u32));
        v.extend(sim.world.idx(Pos::new(b.x2 + 1, y, b.z)).map(|t| t as u32));
    }
    v
}

/// Long rollers connect only across their long axis: drop the ports along it.
pub(crate) fn roller_ports_across(sim: &Sim, b: &Building, tiles: &[u32], ports: &mut Vec<u32>) {
    if tiles.len() <= 1 {
        return;
    }
    let ew = b.x2 > b.x1;
    ports.retain(|&q| {
        let p = sim.world.pos_of(q as usize);
        if ew {
            p.y != b.y1
        } else {
            p.x != b.x1
        }
    });
}

/// "Floor tiles will obstruct the transfer of power over z-levels": a vertical port needs the
/// upper of the two tiles to have no floor.
pub(crate) fn drop_floored_vertical(sim: &Sim, tiles: &[u32], ports: &mut Vec<u32>) {
    if tiles.is_empty() {
        return;
    }
    let z = sim.world.pos_of(tiles[0] as usize).z;
    let layer = sim.nav.layer as u32;
    ports.retain(|&q| {
        let pz = sim.world.pos_of(q as usize).z;
        if pz == z {
            return true;
        }
        let upper = if pz > z { q } else { q + layer };
        let sh = sim.nav.shape(sim.world.tiles[upper as usize].tt);
        !sh.has_floor() && !sh.is_solid()
    });
}

/// The tile above a pump's intake (at the pump's level) must be open.
pub(crate) fn intake_open(sim: &Sim, intake: u32) -> bool {
    let up = intake as usize + sim.nav.layer;
    if up >= sim.world.tiles.len() {
        return false;
    }
    let sh = sim.nav.shape(sim.world.tiles[up].tt);
    !(sh.has_floor() || sh.is_solid())
}

impl Sim {
    /// After a balance: pumps of powered networks may run until the next balance plus 49.
    pub(crate) fn pump_run_on(&mut self, nets: &[crate::power::PowerNet], now: u64) {
        for k in 0..self.power.machines.len() {
            let m = &self.power.machines[k];
            if m.kind != crate::power::MachineKind::ScrewPump || !nets.get(m.net as usize).map(|n| n.powered).unwrap_or(false) {
                continue;
            }
            let (bid, until) = (m.bid, now + 100 + PUMP_RUN_ON);
            match self.power.run_until.binary_search_by_key(&bid, |e| e.0) {
                Ok(i) => self.power.run_until[i].1 = until,
                Err(i) => self.power.run_until.insert(i, (bid, until)),
            }
        }
    }

    pub(crate) fn pump_running_on(&self, bid: i32, now: u64) -> bool {
        !self.power.run_until.is_empty() && self.power.run_until.binary_search_by_key(&bid, |e| e.0).map(|i| now <= self.power.run_until[i].1).unwrap_or(false)
    }

    /// Pumping water onto magma (or the reverse): the output tile turns into obsidian.
    pub(crate) fn fluids_obsidian_at(&mut self, i: usize) {
        self.fluids.obsidian_pub(&mut self.world.tiles, i);
    }
}

#[cfg(test)]
#[path = "machinery_tests.rs"]
mod tests;
