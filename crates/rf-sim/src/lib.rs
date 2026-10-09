//! rf-sim: deterministic fortress simulation (tick loop, pathfinding, units, jobs, needs).
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies below are minimal stubs to be replaced by the real simulation.

use rf_core::{Dig, Pos, World};

/// Player designations that can be placed with the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DesignationKind {
    Mine,
    Channel,
    UpDownStair,
    UpStair,
    DownStair,
    Ramp,
    ChopTrees,
    GatherPlants,
    /// Erase designations in the area.
    Remove,
}

/// What a unit is doing right now, for the UI (DF shows this as the unit's "job").
#[derive(Clone, Debug, Default)]
pub struct UnitStatus {
    /// Short DF-style activity text, e.g. `Mine`, `Drink`, `Sleep`, `No Job`, `Store Item in Stockpile`.
    pub activity: String,
    /// Target position of the current job, if any.
    pub job_pos: Option<Pos>,
    /// 0..=100 need levels for the UI (100 = urgent).
    pub hunger: u8,
    pub thirst: u8,
    pub tiredness: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnouncementKind {
    Info,
    Job,
    Warning,
    Death,
}

#[derive(Clone, Debug)]
pub struct Announcement {
    /// Absolute sim tick.
    pub tick: u64,
    pub text: String,
    pub pos: Option<Pos>,
    pub kind: AnnouncementKind,
}

/// Counters for the UI and the benchmark.
#[derive(Clone, Debug, Default)]
pub struct SimStats {
    pub ticks: u64,
    pub open_jobs: usize,
    pub active_units: usize,
    /// Wall-clock seconds spent per phase in the last `run_ticks` call (name, seconds).
    pub phase_times: Vec<(&'static str, f64)>,
}

pub struct Sim {
    pub world: World,
    pub seed: u64,
    ticks: u64,
    announcements: Vec<Announcement>,
}

impl Sim {
    pub fn new(world: World, seed: u64) -> Sim {
        Sim { world, seed, ticks: 0, announcements: Vec::new() }
    }

    /// Advance the simulation by exactly one DF tick.
    pub fn tick(&mut self) {
        self.ticks += 1;
        self.world.calendar.advance(1);
    }

    pub fn run_ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.tick();
        }
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Designate the inclusive box between `a` and `b` (any corner order, may span z-levels).
    pub fn designate(&mut self, a: Pos, b: Pos, kind: DesignationKind) {
        let (lo, hi) = (
            Pos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            Pos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        );
        let d = match kind {
            DesignationKind::Mine | DesignationKind::ChopTrees | DesignationKind::GatherPlants => Dig::Dig,
            DesignationKind::Channel => Dig::Channel,
            DesignationKind::UpDownStair => Dig::UpDownStair,
            DesignationKind::UpStair => Dig::UpStair,
            DesignationKind::DownStair => Dig::DownStair,
            DesignationKind::Ramp => Dig::Ramp,
            DesignationKind::Remove => Dig::None,
        };
        for z in lo.z..=hi.z {
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    if let Some(t) = self.world.tile_mut(Pos::new(x, y, z)) {
                        t.desig = d as u8;
                    }
                }
            }
        }
    }

    /// Create a stockpile over the inclusive box (single z-level `a.z`) accepting `categories`
    /// (DF names, see `rf_core::StockpileSettings`). Returns the new building id.
    pub fn create_stockpile(&mut self, a: Pos, b: Pos, categories: &[&str]) -> i32 {
        let id = self.world.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        let (x1, x2, y1, y2) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        self.world.buildings.push(rf_core::Building {
            id,
            kind: "Stockpile".into(),
            x1,
            y1,
            x2,
            y2,
            z: a.z,
            center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, a.z),
            built: true,
            owner: -1,
            stockpile: Some(rf_core::StockpileSettings {
                categories: categories.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }),
            ..Default::default()
        });
        id
    }

    pub fn unit_status(&self, _unit_id: i32) -> UnitStatus {
        UnitStatus { activity: "No Job".into(), ..Default::default() }
    }

    /// Take all announcements produced since the last call.
    pub fn drain_announcements(&mut self) -> Vec<Announcement> {
        std::mem::take(&mut self.announcements)
    }

    pub fn stats(&self) -> SimStats {
        SimStats { ticks: self.ticks, active_units: self.world.units.len(), ..Default::default() }
    }

    /// Deterministic hash of the world state (for replay/determinism tests).
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        mix(self.ticks);
        for u in &self.world.units {
            mix(u.id as u64);
            mix(((u.pos.x as u64) << 32) | ((u.pos.y as u64) << 16) | u.pos.z as u64);
        }
        for t in &self.world.tiles {
            mix(((t.tt as u64) << 16) | t.desig as u64);
        }
        h
    }
}
