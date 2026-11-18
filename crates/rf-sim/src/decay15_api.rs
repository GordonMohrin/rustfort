//! [decay15] Player and UI API for rot, miasma, refuse standing orders, vermin and wear.
//!
//! The logic lives in `decay15.rs`; this file only holds the public types and thin `Sim`
//! methods (see docs/mechanics/decay15.md, section 9.1).

use crate::Sim;
use rf_core::Pos;
use serde::{Deserialize, Serialize};

/// Decay stage of a corpse (for [undead13] and the UI).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseStage {
    Fresh,
    Rotten,
    Skeleton,
    /// Not a corpse, or made of a material that does not rot (teeth, chitin...).
    NotRotting,
}

/// Counters of the decay module (fort statistics, bench).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DecayReport {
    /// Food items (meat, fish, meals, cheese) that rotted.
    pub rotted_food: u64,
    /// Plants and plant growths that withered.
    pub withered: u64,
    /// Rotten items that rotted away completely (destroyed).
    pub rotted_away: u64,
    pub corpses_rotted: u64,
    pub skeletons: u64,
    pub parts_to_bones: u64,
    pub shells_from_fish: u64,
    pub hides_rotted: u64,
    pub remains_rotted: u64,
    /// Items moved to the refuse category (bones, skins, animal corpses, rotten food, XX clothing).
    pub to_refuse: u64,
    /// Refuse items marked for dumping by the standing orders.
    pub dump_marked: u64,
    pub miasma_sources_max: usize,
    pub miasma_tiles_max: usize,
    pub vermin_spawned: u64,
    pub vermin_ate: u64,
    pub vermin_killed: u64,
    pub vermin_left: u64,
    pub remains_made: u64,
    pub wear_levels_refuse: u64,
    pub items_worn_out: u64,
    /// Live vermin right now.
    pub vermin_live: usize,
    /// Items with a rot timer right now.
    pub rot_tracked: usize,
}

/// One vermin creature on the map (map overlay).
#[derive(Clone, Debug, Default)]
pub struct VerminInfo {
    pub race: String,
    pub name: String,
    pub pos: Pos,
    pub eater: bool,
    pub rotter: bool,
    pub hateable: bool,
}

/// DF refuse standing orders (w:Standing orders). Defaults as in DF: gather refuse on,
/// outdoor refuse off, everything "save".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RefuseOrders {
    pub gather: bool,
    pub outdoor: bool,
    pub ignore_outdoor_remains: bool,
    pub dump_corpses: bool,
    pub dump_skulls: bool,
    pub dump_bones: bool,
    pub dump_shells: bool,
    pub dump_skins: bool,
    pub dump_hair: bool,
    pub dump_other: bool,
}

impl Default for RefuseOrders {
    fn default() -> Self {
        RefuseOrders {
            gather: true,
            outdoor: false,
            ignore_outdoor_remains: false,
            dump_corpses: false,
            dump_skulls: false,
            dump_bones: false,
            dump_shells: false,
            dump_skins: false,
            dump_hair: false,
            dump_other: false,
        }
    }
}

impl Sim {
    /// Switch the decay module on or off (on by default). Off restores the round-13 rules
    /// (storage rot pass, lifestyle miasma, the old cat roll) and the round-13 state hash.
    pub fn set_decay_enabled(&mut self, on: bool) {
        self.decay.enabled = Some(on);
    }

    pub fn decay_enabled(&self) -> bool {
        self.decay_on()
    }

    pub fn decay_report(&self) -> DecayReport {
        let mut r = self.decay.rep.clone();
        r.vermin_live = self.decay.vermin.len();
        r.rot_tracked = self.decay.rot.len();
        r
    }

    /// Decay stage of corpse `item_id`.
    pub fn corpse_stage(&self, item_id: i32) -> CorpseStage {
        self.corpse_stage_impl(item_id)
    }

    /// (days of exposure so far, days until it rots) for an item with a rot timer.
    pub fn rot_days(&self, item_id: i32) -> Option<(u16, u16)> {
        self.rot_days_impl(item_id)
    }

    /// Live vermin (map overlay).
    pub fn vermin(&self) -> Vec<VerminInfo> {
        self.vermin_list_impl()
    }

    pub fn refuse_orders(&self) -> RefuseOrders {
        self.decay.orders
    }

    /// Change the refuse standing orders (the haul list is rebuilt).
    pub fn set_refuse_orders(&mut self, o: RefuseOrders) {
        self.set_refuse_orders_impl(o);
    }

    /// Tiles with miasma right now (overlay).
    pub fn miasma_tiles(&self) -> Vec<Pos> {
        self.decay.cloud.keys().map(|&t| self.world.pos_of(t as usize)).collect()
    }
}
