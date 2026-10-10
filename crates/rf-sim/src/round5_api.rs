//! Round-5 player API: constructions, smoothing/engraving/carving, traffic, item flags,
//! moods and health readouts.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies are minimal stubs; the construction and mood/health simulations replace them.

use crate::Sim;
use rf_core::{tile_flags, Pos};

/// DF "Build > Constructions" kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConstructionKind {
    Wall,
    Floor,
    Ramp,
    UpStair,
    DownStair,
    UpDownStair,
    Fortification,
    Track,
    /// Remove a construction (deconstruct).
    Remove,
}

/// Stone detailing designations (DF "Smooth / Engrave / Carve").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DetailKind {
    Smooth,
    Engrave,
    CarveFortification,
    CarveTrack,
    Remove,
}

/// Traffic designation levels (DF: high 1, normal 2, low 5, restricted 25 path cost).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Traffic {
    High,
    Normal,
    Low,
    Restricted,
}

/// A unit's strange mood / health summary for the UI.
#[derive(Clone, Debug, Default)]
pub struct MoodInfo {
    /// e.g. `Fey`, `Secretive`, `Possessed`, `Macabre`, `Fell`, "" if none.
    pub mood: String,
    /// Claimed workshop id, -1 if none.
    pub workshop: i32,
    /// Materials still needed, DF-style text ("rough gems", "bones", ...).
    pub needs: Vec<String>,
    /// Ticks until the mood fails into insanity, if known.
    pub ticks_left: Option<u64>,
}

impl Sim {
    /// Designate a construction over the inclusive box on `a.z` (dragged like DF).
    /// Returns the number of tiles designated. Materials are picked by the sim.
    pub fn build_construction(&mut self, _kind: ConstructionKind, _a: Pos, _b: Pos) -> usize {
        0
    }

    /// Smooth / engrave / carve designations over the inclusive box (may span z).
    pub fn designate_detail(&mut self, a: Pos, b: Pos, kind: DetailKind) -> usize {
        let mut n = 0;
        for z in a.z.min(b.z)..=a.z.max(b.z) {
            for y in a.y.min(b.y)..=a.y.max(b.y) {
                for x in a.x.min(b.x)..=a.x.max(b.x) {
                    if let Some(t) = self.world.tile_mut(Pos::new(x, y, z)) {
                        t.flags &= !(tile_flags::DESIG_SMOOTH | tile_flags::DESIG_ENGRAVE);
                        match kind {
                            DetailKind::Smooth => t.flags |= tile_flags::DESIG_SMOOTH,
                            DetailKind::Engrave => t.flags |= tile_flags::DESIG_ENGRAVE,
                            _ => {}
                        }
                        n += 1;
                    }
                }
            }
        }
        n
    }

    /// Traffic designation over the inclusive box (may span z).
    pub fn set_traffic(&mut self, a: Pos, b: Pos, level: Traffic) {
        let v: u16 = match level {
            Traffic::Normal => 0,
            Traffic::Low => 1,
            Traffic::High => 2,
            Traffic::Restricted => 3,
        };
        for z in a.z.min(b.z)..=a.z.max(b.z) {
            for y in a.y.min(b.y)..=a.y.max(b.y) {
                for x in a.x.min(b.x)..=a.x.max(b.x) {
                    if let Some(t) = self.world.tile_mut(Pos::new(x, y, z)) {
                        t.flags = (t.flags & !tile_flags::TRAFFIC_MASK) | (v << tile_flags::TRAFFIC_SHIFT);
                    }
                }
            }
        }
    }

    /// Set an item flag: `forbid`, `dump`, `melt`, `hidden` (DF item designations).
    pub fn set_item_flag(&mut self, item_id: i32, flag: &str, on: bool) {
        if let Some(it) = self.world.items.iter_mut().find(|i| i.id == item_id) {
            match flag {
                "forbid" => it.flags.forbid = on,
                "dump" => it.flags.dump = on,
                "hidden" => it.flags.hidden = on,
                _ => {}
            }
        }
    }

    /// Item designations over all items in the inclusive box (DF claim/forbid/dump/melt area).
    pub fn set_items_flag_in(&mut self, a: Pos, b: Pos, flag: &str, on: bool) -> usize {
        let ids: Vec<i32> = self
            .world
            .items
            .iter()
            .filter(|i| {
                let p = i.pos;
                p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y) && p.z >= a.z.min(b.z) && p.z <= a.z.max(b.z)
            })
            .map(|i| i.id)
            .collect();
        for id in &ids {
            self.set_item_flag(*id, flag, on);
        }
        ids.len()
    }

    pub fn mood_info(&self, unit_id: i32) -> MoodInfo {
        self.mood_info_impl(unit_id) // [moods]
    }
}
