//! [diplo13] Diplomacy (DF v50; wiki Diplomacy, Outpost liaison, Diplomat, Land holder,
//! Barony, Tree cutting, Tribute, War).
//!
//! SKELETON from the round-13 contract: the state is persisted (part of the exact snapshot via
//! `Sim::diplo`), the hook runs, nothing happens yet. Owner [diplo13] fills it in: liaison
//! meetings with each caravan (requests, elevation to barony/county/duchy, land holder),
//! diplomats of other civs (elf tree quotas, human tribute/peace),
//! `rf_core::EntityInfo::attitude` shifting by trade value, killings and refused demands,
//! declarations of war and peace (`EntityInfo::relation`; `trade` already filters
//! `relation == "War"`).

use crate::round13_api::{Demand, DiplomacyEntity, DiplomacyReport, Meeting};
use crate::Sim;
use serde::{Deserialize, Serialize};

/// `diplomacy_day` runs when `now % TICKS_PER_DAY == DIPLO_DAY_PHASE` (owner may change it).
pub(crate) const DIPLO_DAY_PHASE: u64 = 857;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Diplo {
    /// Switched off by the player.
    pub off: bool,
    pub meetings: Vec<Meeting>,
    pub demands: Vec<Demand>,
    pub next_id: i32,
    /// Land title index: 0 none (outpost), 1 barony, 2 county, 3 duchy.
    pub title: u8,
    /// Liaison requests per entity: (entity id, requests).
    pub requests: Vec<(i32, Vec<String>)>,
    /// Year each current war started: (entity id, year).
    pub wars: Vec<(i32, i32)>,
    pub rep: DiplomacyReport,
}

impl Sim {
    /// Hook from `tick_inner` (once a day).
    pub(crate) fn diplomacy_day(&mut self, _now: u64) {}

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn diplomacy_hash(&self, _mix: &mut dyn FnMut(u64)) {}

    pub(crate) fn diplomacy_entities_impl(&self) -> Vec<DiplomacyEntity> {
        self.world
            .entities
            .iter()
            .filter(|e| e.kind == "Civilization" || e.kind.is_empty())
            .map(|e| DiplomacyEntity {
                id: e.id,
                name: e.name.clone(),
                race: e.race.clone(),
                relation: e.relation.clone(),
                attitude: e.attitude,
                trades: e.trades,
                at_war_since: self.diplo.wars.iter().find(|w| w.0 == e.id).map(|w| w.1),
            })
            .collect()
    }

    pub(crate) fn diplomacy_meetings_impl(&self) -> Vec<Meeting> {
        self.diplo.meetings.clone()
    }

    pub(crate) fn diplomacy_demands_impl(&self) -> Vec<Demand> {
        self.diplo.demands.clone()
    }

    pub(crate) fn answer_demand_impl(&mut self, demand: i32, accept: bool) -> bool {
        match self.diplo.demands.iter_mut().find(|d| d.id == demand && d.answer.is_none()) {
            Some(d) => {
                d.answer = Some(accept);
                true
            }
            None => false,
        }
    }

    pub(crate) fn set_liaison_requests_impl(&mut self, entity: i32, requests: &[&str]) {
        let v: Vec<String> = requests.iter().map(|r| r.to_string()).collect();
        match self.diplo.requests.iter_mut().find(|r| r.0 == entity) {
            Some(r) => r.1 = v,
            None => self.diplo.requests.push((entity, v)),
        }
    }

    pub(crate) fn land_title_impl(&self) -> &'static str {
        ["", "Barony", "County", "Duchy"][(self.diplo.title as usize).min(3)]
    }

    pub(crate) fn set_diplomacy_enabled_impl(&mut self, on: bool) {
        self.diplo.off = !on;
    }

    pub(crate) fn diplomacy_report_impl(&self) -> DiplomacyReport {
        self.diplo.rep
    }
}
