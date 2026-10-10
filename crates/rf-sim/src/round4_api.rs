//! Round-4 player API: doors/levers/bridges/traps, trade, nobles, population, fluids.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies are minimal stubs; the defense, population/trade and fluid simulations
//! replace them.

use crate::Sim;

/// What the depot/trade screen needs.
#[derive(Clone, Debug, Default)]
pub struct TradeStatus {
    /// Merchant civilization name and race.
    pub civ: String,
    pub race: String,
    /// Depot building id where trading happens.
    pub depot: i32,
    /// A broker is at the depot (DF: trading needs the broker present).
    pub broker_present: bool,
    /// Item ids the merchants offer.
    pub goods: Vec<i32>,
    /// Item ids of the fort currently in the depot (can be offered).
    pub fort_goods: Vec<i32>,
    /// Ticks until the caravan leaves.
    pub leaves_in: u64,
}

/// Result of a proposed trade.
#[derive(Clone, Debug, Default)]
pub struct TradeOffer {
    pub accepted: bool,
    /// Value the merchants see on each side (DF trade value incl. broker skill/markup).
    pub offer_value: i32,
    pub request_value: i32,
    /// DF-style merchant reply, e.g. "This is a fair trade." / "Are you mocking me?".
    pub reply: String,
}

/// Population outlook for the UI.
#[derive(Clone, Debug, Default)]
pub struct PopulationInfo {
    pub citizens: usize,
    pub children: usize,
    pub population_cap: i32,
    /// Ticks until the next migrant wave check (DF: seasonal), if any.
    pub next_wave_in: Option<u64>,
    pub caravan_expected: Option<String>,
}

impl Sim {
    /// Pull a lever (toggles all linked buildings: bridges, doors, floodgates, hatches).
    /// [defense] Like DF this queues a "Pull the Lever" job: an idle citizen who can reach the
    /// lever walks there and pulls it; linked buildings react after the mechanism delay.
    /// `pull_lever_now` pulls it at once.
    pub fn pull_lever(&mut self, building_id: i32) {
        self.defense_pull_lever(building_id);
    }

    /// Set or clear a building flag such as `LOCKED`, `FORBIDDEN`, `PET_PASSABLE` (doors, hatches).
    /// [defense] Door/hatch locks, floodgate state and plate settings take effect immediately.
    pub fn set_building_flag(&mut self, building_id: i32, flag: &str, on: bool) {
        self.defense_set_flag(building_id, flag, on);
    }

    /// Link a lever/pressure plate to a target building (DF: needs mechanisms).
    /// [defense] Targets: doors, hatches, floodgates, bridges, upright spikes. Instant (no
    /// mechanic job or mechanisms needed yet).
    pub fn link_building(&mut self, trigger_id: i32, target_id: i32) -> Result<(), String> {
        self.defense_link(trigger_id, target_id)
    }

    /// Current caravan at the depot, if any.
    pub fn trade_status(&self) -> Option<TradeStatus> {
        self.trade_status_impl() // [society]
    }

    /// Evaluate (and if `commit`, execute) a barter: fort items `offer` for merchant items `request`.
    pub fn trade(&mut self, offer: &[i32], request: &[i32], commit: bool) -> TradeOffer {
        self.trade_impl(offer, request, commit) // [society]
    }

    /// Mark fort items to be brought to the depot (DF "move goods to depot").
    pub fn move_to_depot(&mut self, item_ids: &[i32]) {
        self.move_to_depot_impl(item_ids) // [society]
    }

    /// Appoint a unit to a noble position (token from `world.nobles`).
    pub fn appoint(&mut self, token: &str, unit_id: i32) {
        self.appoint_impl(token, unit_id) // [society]
    }

    pub fn population_info(&self) -> PopulationInfo {
        self.population_info_impl() // [society]
    }

    pub fn set_population_cap(&mut self, cap: i32) {
        self.set_population_cap_impl(cap) // [society]
    }

    /// Number of tiles with liquid and how many are currently flowing (for the UI/bench).
    pub fn liquid_stats(&self) -> (usize, usize) {
        let r = self.fluid_report(); // [fluids]
        (r.liquid_tiles, r.flowing) // [fluids]
    }
}
