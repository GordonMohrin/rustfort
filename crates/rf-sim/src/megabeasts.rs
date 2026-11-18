//! [mega13] Megabeasts, semi-megabeasts and titans (DF v50; wiki Megabeast, Semi-megabeast,
//! Titan, Dragon, Hydra, Roc, Bronze colossus, Building destroyer).
//!
//! SKELETON from the round-13 contract: the state is persisted (part of the exact snapshot via
//! `Sim::mega`), the hook runs, nothing happens yet. Owner [mega13] fills it in: surface
//! attacks triggered by fort wealth/population, generated titans (like forgotten beasts in
//! `caverns`), semi-megabeasts, dragon fire, building destroyers, fortress history and
//! thoughts on slaying. Also lists forgotten beasts and sieges in `Sim::threats`.

use crate::round13_api::{MegabeastInfo, MegabeastReport, Threat, UnitCurse};
use crate::Sim;
use serde::{Deserialize, Serialize};

/// `megabeasts_day` runs when `now % TICKS_PER_DAY == MEGA_DAY_PHASE` (owner may change it).
pub(crate) const MEGA_DAY_PHASE: u64 = 733;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Mega {
    /// Switched off by the player.
    pub off: bool,
    /// Every megabeast that came (alive and slain).
    pub beasts: Vec<MegabeastInfo>,
    pub rep: MegabeastReport,
}

impl Sim {
    /// Hook from `tick_inner` (once a day).
    pub(crate) fn megabeasts_day(&mut self, _now: u64) {}

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn megabeasts_hash(&self, _mix: &mut dyn FnMut(u64)) {}

    pub(crate) fn megabeast_threats(&self, _out: &mut Vec<Threat>) {}

    pub(crate) fn megabeast_unit_flags(&self, _unit_id: i32, _f: &mut UnitCurse) {}

    pub(crate) fn megabeasts_impl(&self) -> Vec<MegabeastInfo> {
        self.mega.beasts.clone()
    }

    pub(crate) fn spawn_megabeast_impl(&mut self, _race: &str) -> Option<i32> {
        None
    }

    pub(crate) fn set_megabeasts_enabled_impl(&mut self, on: bool) {
        self.mega.off = !on;
    }

    pub(crate) fn megabeast_report_impl(&self) -> MegabeastReport {
        self.mega.rep
    }
}
