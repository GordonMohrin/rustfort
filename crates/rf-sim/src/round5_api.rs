//! Round-5 player API: constructions, smoothing/engraving/carving, traffic, item flags,
//! moods and health readouts.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies are minimal stubs; the construction and mood/health simulations replace them.

use crate::Sim;
use rf_core::Pos;

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
    pub fn build_construction(&mut self, kind: ConstructionKind, a: Pos, b: Pos) -> usize {
        self.cx_build(kind, a, b) // [construct]
    }

    /// Smooth / engrave / carve designations over the inclusive box (may span z).
    pub fn designate_detail(&mut self, a: Pos, b: Pos, kind: DetailKind) -> usize {
        self.cx_detail(a, b, kind) // [construct]
    }

    /// Traffic designation over the inclusive box (may span z).
    pub fn set_traffic(&mut self, a: Pos, b: Pos, level: Traffic) {
        self.cx_set_traffic(a, b, level) // [construct]
    }

    /// Set an item flag: `forbid`, `claim` (= unforbid), `dump`, `melt`, `hidden` (DF item designations).
    pub fn set_item_flag(&mut self, item_id: i32, flag: &str, on: bool) {
        self.cx_set_item_flag(item_id, flag, on) // [construct]
    }

    /// Item designations over all items in the inclusive box (DF claim/forbid/dump/melt area).
    pub fn set_items_flag_in(&mut self, a: Pos, b: Pos, flag: &str, on: bool) -> usize {
        self.cx_set_items_flag_in(a, b, flag, on) // [construct]
    }

    pub fn mood_info(&self, unit_id: i32) -> MoodInfo {
        self.mood_info_impl(unit_id) // [moods]
    }
}
