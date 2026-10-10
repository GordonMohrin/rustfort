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
    pub fn pull_lever(&mut self, building_id: i32) {
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == building_id) {
            if let Some(i) = b.flags.iter().position(|f| f == "ACTIVE") {
                b.flags.remove(i);
            } else {
                b.flags.push("ACTIVE".into());
            }
        }
    }

    /// Set or clear a building flag such as `LOCKED`, `FORBIDDEN`, `PET_PASSABLE` (doors, hatches).
    pub fn set_building_flag(&mut self, building_id: i32, flag: &str, on: bool) {
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == building_id) {
            b.flags.retain(|f| f != flag);
            if on {
                b.flags.push(flag.to_string());
            }
        }
    }

    /// Link a lever/pressure plate to a target building (DF: needs mechanisms).
    pub fn link_building(&mut self, trigger_id: i32, target_id: i32) -> Result<(), String> {
        let ok = self.world.buildings.iter().any(|b| b.id == target_id);
        if !ok {
            return Err("no such target".into());
        }
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == trigger_id) {
            if !b.links.contains(&target_id) {
                b.links.push(target_id);
            }
            return Ok(());
        }
        Err("no such lever".into())
    }

    /// Current caravan at the depot, if any.
    pub fn trade_status(&self) -> Option<TradeStatus> {
        None
    }

    /// Evaluate (and if `commit`, execute) a barter: fort items `offer` for merchant items `request`.
    pub fn trade(&mut self, _offer: &[i32], _request: &[i32], _commit: bool) -> TradeOffer {
        TradeOffer { reply: "There is nobody to trade with.".into(), ..Default::default() }
    }

    /// Mark fort items to be brought to the depot (DF "move goods to depot").
    pub fn move_to_depot(&mut self, _item_ids: &[i32]) {}

    /// Appoint a unit to a noble position (token from `world.nobles`).
    pub fn appoint(&mut self, token: &str, unit_id: i32) {
        if let Some(n) = self.world.nobles.iter_mut().find(|n| n.token == token) {
            n.holder = unit_id;
        }
    }

    pub fn population_info(&self) -> PopulationInfo {
        PopulationInfo {
            citizens: self.world.citizens().count(),
            children: self.world.citizens().filter(|u| u.profession == "CHILD" || u.profession == "BABY").count(),
            population_cap: self.world.settings.population_cap,
            ..Default::default()
        }
    }

    pub fn set_population_cap(&mut self, cap: i32) {
        self.world.settings.population_cap = cap;
    }

    /// Number of tiles with liquid and how many are currently flowing (for the UI/bench).
    pub fn liquid_stats(&self) -> (usize, usize) {
        let r = self.fluid_report(); // [fluids]
        (r.liquid_tiles, r.flowing) // [fluids]
    }
}
