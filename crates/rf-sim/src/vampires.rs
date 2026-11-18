//! [vampire13] Vampires and werebeasts (DF v50; wiki Vampire, Werebeast, Night creature,
//! Justice).
//!
//! SKELETON from the round-13 contract: the state is persisted (part of the exact snapshot via
//! `Sim::vamp`), the hook runs, nothing happens yet. Owner [vampire13] fills it in: vampire
//! migrants with false identities (`rf_core::Unit::curse == "VAMPIRE"`, `true_name`; no
//! sleep/eat/drink, feeding on sleepers, drained corpses), unmasking through `justice`,
//! werebeast curse (`curse == "WEREBEAST"`, `curse_race`; full-moon transformation, bites
//! spread it) and werebeast attacks.

use crate::round13_api::{Threat, UnitCurse, VampireReport};
use crate::Sim;
use serde::{Deserialize, Serialize};

/// `vampires_pass` runs when `now % VAMP_PERIOD == VAMP_PHASE` (owner may change both).
pub(crate) const VAMP_PERIOD: u64 = 100;
pub(crate) const VAMP_PHASE: u64 = 83;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Vamp {
    /// Switched off by the player.
    pub vampires_off: bool,
    pub werebeasts_off: bool,
    pub rep: VampireReport,
}

impl Sim {
    /// Hook from `tick_inner` (every `VAMP_PERIOD` ticks).
    pub(crate) fn vampires_pass(&mut self, _now: u64) {}

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn vampires_hash(&self, _mix: &mut dyn FnMut(u64)) {}

    pub(crate) fn vampire_threats(&self, _out: &mut Vec<Threat>) {}

    pub(crate) fn vampire_unit_flags(&self, _unit_id: i32, _f: &mut UnitCurse) {}

    pub(crate) fn set_vampires_enabled_impl(&mut self, on: bool) {
        self.vamp.vampires_off = !on;
    }

    pub(crate) fn set_werebeasts_enabled_impl(&mut self, on: bool) {
        self.vamp.werebeasts_off = !on;
    }

    pub(crate) fn spawn_vampire_migrant_impl(&mut self) -> Option<i32> {
        None
    }

    pub(crate) fn spawn_werebeast_attack_impl(&mut self) -> Option<i32> {
        None
    }

    pub(crate) fn moon_phase_impl(&self) -> u8 {
        0
    }

    pub(crate) fn accuse_vampire_impl(&mut self, _unit_id: i32) -> bool {
        false
    }

    pub(crate) fn vampire_report_impl(&self) -> VampireReport {
        self.vamp.rep
    }
}
