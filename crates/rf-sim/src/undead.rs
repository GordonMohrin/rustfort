//! [undead13] Undead and necromancy (DF v50; wiki Undead, Reanimation, Necromancer, Husk,
//! Thrall, Evil weather).
//!
//! SKELETON from the round-13 contract: the state is persisted (part of the exact snapshot via
//! `Sim::undead`), the hook runs, nothing happens yet. Owner [undead13] fills it in:
//! corpses and body parts rising in reanimating regions (`World::climate.reanimating`), undead
//! combat traits (DF `NOT_LIVING`, `OPPOSED_TO_LIFE`: no pain, fear, fatigue, needs, bleeding;
//! destroyed by severing/smashing), necromancer sieges from towers, intelligent undead, husks
//! and thralls from evil weather (`Sim::evil_weather`). Mark undead units with
//! `rf_core::Unit::undead` / `curse` (NECROMANCER, HUSK, THRALL).

use crate::round13_api::{Threat, UndeadReport, UnitCurse};
use crate::Sim;
use serde::{Deserialize, Serialize};

/// `undead_pass` runs when `now % UNDEAD_PERIOD == UNDEAD_PHASE` (owner may change both).
pub(crate) const UNDEAD_PERIOD: u64 = 100;
pub(crate) const UNDEAD_PHASE: u64 = 59;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Undead {
    /// Player toggle; None = follow `World::climate.reanimating`.
    pub reanimation: Option<bool>,
    pub rep: UndeadReport,
}

impl Sim {
    /// Hook from `tick_inner` (every `UNDEAD_PERIOD` ticks).
    pub(crate) fn undead_pass(&mut self, _now: u64) {}

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn undead_hash(&self, _mix: &mut dyn FnMut(u64)) {}

    pub(crate) fn undead_threats(&self, _out: &mut Vec<Threat>) {}

    pub(crate) fn undead_unit_flags(&self, _unit_id: i32, _f: &mut UnitCurse) {}

    pub(crate) fn set_reanimation_enabled_impl(&mut self, on: bool) {
        self.undead.reanimation = Some(on);
    }

    pub(crate) fn reanimation_enabled_impl(&self) -> bool {
        self.undead.reanimation.unwrap_or(self.world.climate.reanimating)
    }

    pub(crate) fn spawn_necromancer_siege_impl(&mut self, _n: usize) -> usize {
        0
    }

    pub(crate) fn undead_report_impl(&self) -> UndeadReport {
        self.undead.rep
    }
}
