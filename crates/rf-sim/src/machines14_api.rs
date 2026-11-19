//! [machines14] Public API for the machines/mechanisms UI: the link target picker, link jobs,
//! pressure plate settings, device states and power networks.

use crate::linkage::{DevKind, LinkStats};
use crate::Sim;
use rf_core::{PlateInfo, Pos};

/// A building a lever or plate can be linked to.
#[derive(Clone, Debug)]
pub struct LinkTarget {
    pub bid: i32,
    pub kind: DevKind,
    pub label: &'static str,
    pub pos: Pos,
    /// DF "open" state (door open, bridge raised, gear disengaged, track stop disabled, ...).
    pub open: bool,
    pub already_linked: bool,
    pub queued: bool,
    /// Distance (Chebyshev, plus 4 per z level) to the trigger.
    pub dist: i32,
}

/// A queued link job.
#[derive(Clone, Copy, Debug)]
pub struct LinkJobInfo {
    pub trigger: i32,
    pub target: i32,
    /// 0/1: first mechanism to the target, 2/3: second mechanism to the trigger.
    pub phase: u8,
    pub worker: Option<i32>,
    /// Waiting (no mechanic, no mechanisms or no path) until this tick.
    pub retry_at: u64,
}

/// Pressure plate triggers and ranges (DF's plate dialog).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlateSettings {
    pub units: bool,
    pub water: bool,
    pub magma: bool,
    pub track: bool,
    pub citizens: bool,
    pub resets: bool,
    pub r: PlateInfo,
}

/// Live state of a pressure plate.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlateState {
    pub pressed: bool,
    /// Ticks until it sends "off" (Some while the condition has ended but the close is pending).
    pub closing_in: Option<u64>,
    pub occupants: usize,
}

/// State of a linked device.
#[derive(Clone, Copy, Debug)]
pub struct DeviceInfo {
    pub kind: DevKind,
    pub open: bool,
    /// A change is pending: (due tick, wanted open state).
    pub pending: Option<(u64, bool)>,
    pub jammed: bool,
}

/// One machine network.
#[derive(Clone, Debug)]
pub struct NetInfo {
    pub id: u32,
    pub produced: i32,
    pub used: i32,
    pub powered: bool,
    /// Building ids of its machines.
    pub machines: Vec<i32>,
    /// Port links between machines of the net: (building a, building b), a < b.
    pub links: Vec<(i32, i32)>,
}

impl Sim {
    /// Link a trigger to a target right away, without a mechanic or mechanisms (tests,
    /// scripts, the `--ui` demo).
    pub fn link_building_now(&mut self, trigger: i32, target: i32) -> Result<(), String> {
        self.link_now(trigger, target)
    }

    /// Every building `trigger` could be linked to, nearest first.
    pub fn linkable_targets(&self, trigger: i32) -> Vec<LinkTarget> {
        let Some(g) = self.world.building(trigger) else { return Vec::new() };
        let gp = g.center;
        let mut v: Vec<LinkTarget> = Vec::new();
        for b in &self.world.buildings {
            if b.id == trigger || !b.built {
                continue;
            }
            let Some(kind) = DevKind::of(b) else { continue };
            let open = self.device_open(b.id).map(|d| d.1).unwrap_or(false);
            let dist = (b.center.x - gp.x).abs().max((b.center.y - gp.y).abs()) as i32 + 4 * (b.center.z - gp.z).abs() as i32;
            v.push(LinkTarget {
                bid: b.id,
                kind,
                label: kind.noun(),
                pos: b.center,
                open,
                already_linked: g.links.contains(&b.id),
                queued: self.link.jobs.iter().any(|j| j.trigger == trigger && j.target == b.id),
                dist,
            });
        }
        v.sort_by_key(|t| (t.dist, t.bid));
        v
    }

    /// Queued link jobs.
    pub fn link_jobs(&self) -> Vec<LinkJobInfo> {
        self.link
            .jobs
            .iter()
            .map(|j| LinkJobInfo {
                trigger: j.trigger,
                target: j.target,
                phase: j.phase,
                worker: (j.claimed != crate::NONE).then(|| self.ai.get(j.claimed as usize).map(|a| a.id)).flatten(),
                retry_at: j.retry_at,
            })
            .collect()
    }

    /// Cancel a queued link job (an installed first mechanism stays in the target).
    pub fn cancel_link_job(&mut self, trigger: i32, target: i32) {
        if let Some(j) = self.link.jobs.iter().find(|j| j.trigger == trigger && j.target == target).copied() {
            if j.claimed != crate::NONE {
                let s = j.claimed as usize;
                self.cancel_task(s);
                self.ai[s].task = crate::Task::Idle;
            }
            if let Some(j) = self.link.jobs.iter().find(|j| j.trigger == trigger && j.target == target).copied() {
                for (n, &m) in j.mech.iter().enumerate() {
                    if m >= 0 && !(n == 0 && j.phase >= 2) {
                        self.unreserve(m);
                    }
                }
            }
        }
        self.link.jobs.retain(|j| !(j.trigger == trigger && j.target == target));
    }

    /// Mechanisms free for link jobs.
    pub fn free_mechanisms(&self) -> usize {
        self.link_free_mechs().len()
    }

    /// Settings of a pressure plate (None if `bid` is no plate).
    pub fn plate_settings(&self, bid: i32) -> Option<PlateSettings> {
        let b = self.world.building(bid)?;
        if !(b.kind == "Trap" && b.subtype == "PressurePlate") {
            return None;
        }
        let mut p = crate::linkage::Plate::default();
        crate::linkage::plate_settings_from(b, &mut p);
        Some(PlateSettings { units: p.units, water: p.water, magma: p.magma, track: p.track, citizens: p.citizens, resets: p.resets, r: p.r })
    }

    /// Change a plate's settings. DF: only before it is built; Err once built.
    pub fn set_plate_settings(&mut self, bid: i32, st: PlateSettings) -> Result<(), String> {
        let b = self.world.buildings.iter_mut().find(|b| b.id == bid).ok_or("no such building")?;
        if !(b.kind == "Trap" && b.subtype == "PressurePlate") {
            return Err("not a pressure plate".into());
        }
        if b.built {
            return Err("plate settings must be chosen before construction".into());
        }
        crate::linkage::write_plate_flags(b, &st);
        Ok(())
    }

    /// Live state of a pressure plate.
    pub fn plate_state(&self, bid: i32) -> Option<PlateState> {
        let p = self.link.plates.iter().find(|p| p.bid == bid)?;
        let closing_in = (p.on && p.false_since != u64::MAX).then(|| (p.false_since + crate::linkage::PLATE_CLOSE).saturating_sub(self.ticks));
        Some(PlateState { pressed: p.on, closing_in, occupants: p.occ.len() })
    }

    /// State of a linkable device (None if `bid` is none).
    pub fn device_state(&self, bid: i32) -> Option<DeviceInfo> {
        let (kind, open) = self.device_open(bid)?;
        let pending = self.link.timers.iter().find(|t| t.bid == bid).map(|t| (t.due, t.want));
        Some(DeviceInfo { kind, open, pending, jammed: self.link.jammed.binary_search(&bid).is_ok() })
    }

    /// Mechanism counters (signals, changes, plates, links, jams).
    pub fn link_stats(&self) -> LinkStats {
        self.link.stats
    }

    /// Machine networks with their machines and port links.
    pub fn power_networks(&self) -> Vec<NetInfo> {
        let ms = &self.power.machines;
        let mut v: Vec<NetInfo> = self
            .power
            .nets
            .iter()
            .enumerate()
            .map(|(i, n)| NetInfo { id: i as u32, produced: n.produced, used: n.used, powered: n.powered, machines: Vec::new(), links: Vec::new() })
            .collect();
        for (a, m) in ms.iter().enumerate() {
            if let Some(n) = v.get_mut(m.net as usize) {
                n.machines.push(m.bid);
                for (b, o) in ms.iter().enumerate() {
                    if b <= a || o.net != m.net {
                        continue;
                    }
                    let ab = m.ports.iter().any(|q| o.tiles.contains(q)) && o.ports.iter().any(|q| m.tiles.contains(q));
                    if ab {
                        n.links.push((m.bid.min(o.bid), m.bid.max(o.bid)));
                    }
                }
            }
        }
        v
    }

    /// Network index of a machine.
    pub fn machine_net(&self, bid: i32) -> Option<u32> {
        self.power.machines.iter().find(|m| m.bid == bid).map(|m| m.net)
    }

    /// Pump intake and output tiles of a screw pump (for the overlay arrow).
    pub fn pump_ends(&self, bid: i32) -> Option<(Pos, Pos)> {
        let m = self.power.machines.iter().find(|m| m.bid == bid)?;
        if m.intake == crate::NONE || m.output == crate::NONE {
            return None;
        }
        Some((self.world.pos_of(m.intake as usize), self.world.pos_of(m.output as usize)))
    }
}

impl Sim {
    /// Moving minecarts: (tile, direction (dx, dy), speed) for the route view arrows.
    pub fn cart_motion(&self) -> Vec<(Pos, (i8, i8), i32)> {
        use crate::minecarts::{CartState, DE, DN, DS, DW};
        self.carts
            .carts
            .iter()
            .filter(|c| c.state == CartState::Moving && c.item >= 0)
            .map(|c| {
                let d = match c.dir {
                    DN => (0, -1),
                    DS => (0, 1),
                    DE => (1, 0),
                    DW => (-1, 0),
                    _ => (0, 0),
                };
                (self.world.pos_of(c.tile as usize), d, c.v)
            })
            .collect()
    }
}
