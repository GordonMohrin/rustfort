//! [diplo13] Diplomacy (DF v50; wiki Diplomacy, Outpost liaison, Diplomat, Noble / Land holder,
//! Barony, Tree, Tribute, War).
//!
//! # Rules (see docs/FIDELITY.md, section `[diplo13]`)
//! - **Outpost liaison**: travels with the caravan of the fort's own civilization and meets the
//!   noble who receives diplomats (land holder, else mayor, else expedition leader). The meeting
//!   raises a `Request` demand: accepting it makes a trade agreement, and the requested goods
//!   (`set_liaison_requests`) come in double numbers with that civilization's caravans for a year.
//! - **Elevation**: at the liaison meeting the fort rises one land-holder level (barony, county,
//!   duchy) if it meets the save's `LAND_HOLDER_POP_TRIGGER` / `_PROD_TRIGGER` /
//!   `_TRADE_TRIGGER` for that level (population, created wealth, exported wealth). The noble
//!   receiving the liaison becomes the land holder (a `BARON`/`COUNT`/`DUKE` position).
//! - **Diplomats** of other civilizations come with their caravans once the fort has a land
//!   holder (wiki Diplomat). Elves impose a yearly tree-felling quota (`TREE_FELL_COUNT`, or
//!   `TREE_FELL_COUNT_SAVAGE` in savage lands); every tree felled counts (`fell_tree` hook).
//!   Humans that dislike the fort demand tribute, paid as gifts to their caravan.
//! - **Attitude** (`EntityInfo::attitude`, -100..=100) rises with profitable trades and gifts,
//!   falls when their people are killed on the map, demands are refused, quotas are exceeded or
//!   tribute is not paid. It drifts back towards its baseline each season.
//! - **War**: a civilization whose attitude falls to `WAR_ATTITUDE` declares war: `relation`
//!   becomes `War`, which stops its caravans (`trade::trade_partner`) and sends war parties of
//!   its race each season (raids of `RaidKind::Ambush`; goblin sieges stay in `raids`).
//! - **Peace**: a non-goblin civilization at war for at least a year whose anger has cooled
//!   sends an envoy with a peace offer in its caravan season.
//!
//! Hooks (single tagged lines): `trade.rs` caravan arrival, trade commit and goods unloading;
//! `lib.rs` `kill` and `fell_tree`.

use crate::raids::RaidKind;
use crate::rng::Rng;
use crate::round13_api::{Demand, DemandKind, DiplomacyEntity, DiplomacyReport, Meeting, MeetingKind};
use crate::{AnnouncementKind, Sim};
use rf_core::{TICKS_PER_DAY, TICKS_PER_MONTH, TICKS_PER_YEAR};
use serde::{Deserialize, Serialize};

/// `diplomacy_day` runs when `now % TICKS_PER_DAY == DIPLO_DAY_PHASE`.
pub(crate) const DIPLO_DAY_PHASE: u64 = 857;

const DAY: u64 = TICKS_PER_DAY as u64;
/// A civilization at peace declares war at this attitude (estimate).
pub const WAR_ATTITUDE: i32 = -50;
/// Peace is offered once attitude has recovered above this (estimate).
pub const PEACE_ATTITUDE: i32 = -30;
/// Days a demand stays open before it is resolved by default.
const DEMAND_DAYS: u64 = 14;
/// Default quotas (vanilla d_init / difficulty `TREE_FELL_COUNT`, `_SAVAGE`).
const TREE_FELL_DEFAULT: i64 = 25;
const TREE_FELL_SAVAGE_DEFAULT: i64 = 5;
/// Created-wealth / exported-wealth / population defaults of the first three land-holder levels
/// (vanilla difficulty settings, used when the save carries none).
const LAND_POP: [i64; 3] = [20, 20, 20];
const LAND_PROD: [i64; 3] = [100_000, 200_000, 300_000];
const LAND_TRADE: [i64; 3] = [10_000, 20_000, 30_000];
const LAND_TOKENS: [&str; 3] = ["BARON", "COUNT", "DUKE"];

/// A meeting scheduled for later (envoys arrive with or shortly after the caravan).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Visit {
    pub at: u64,
    pub entity: i32,
    pub kind: MeetingKind,
    pub envoy: i32,
    /// Peace envoy of a civilization at war.
    pub peace: bool,
}

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
    // ---- [diplo13] own state
    /// Baselines set (first day the module ran).
    pub init: bool,
    pub visits: Vec<Visit>,
    /// Trade agreements from accepted liaison requests: (entity, valid until tick).
    pub agreements: Vec<(i32, u64)>,
    /// Tree quotas of this year: (entity, allowed, already over the limit).
    pub quotas: Vec<(i32, i64, bool)>,
    /// Trees felled this game year.
    pub felled: i64,
    /// Accepted tribute still owed: (entity, value owed, due tick).
    pub tribute: Vec<(i32, i64, u64)>,
    /// Deaths of other civilizations' people since the last day: (entity, unit, envoy).
    pub deaths: Vec<(i32, i32, bool)>,
    /// Wars declared by this module (they send war parties): entity ids.
    pub declared: Vec<i32>,
    /// `trades` flag of each civilization before its war: (entity, trades).
    pub war_trades: Vec<(i32, bool)>,
    pub last_year: i32,
    pub last_season: i32,
    /// Units of land holders appointed here, for the UI: (title level, unit).
    pub holders: Vec<(u8, i32)>,
    /// Attacker slot of the combat blow that is killing a unit right now (set by `strike`,
    /// taken by `diplomacy_unit_died` in the same call chain; never outlives it).
    #[serde(skip)]
    pub killer: Option<usize>,
}

fn clamp_att(a: i32) -> i32 {
    a.clamp(-100, 100)
}

impl Sim {
    // ------------------------------------------------------------------ helpers

    fn diplo_own_civ(&self) -> i32 {
        self.world.meta.civ_id
    }

    fn diplo_entity_idx(&self, id: i32) -> Option<usize> {
        self.world.entities.iter().position(|e| e.id == id)
    }

    fn diplo_entity_name(&self, id: i32) -> String {
        match self.diplo_entity_idx(id) {
            Some(i) if !self.world.entities[i].name.is_empty() => self.world.entities[i].name.clone(),
            Some(i) => format!("the {} civilization", self.world.entities[i].race.to_ascii_lowercase()),
            None => "the civilization".into(),
        }
    }

    fn diplo_race(&self, id: i32) -> String {
        self.diplo_entity_idx(id).map(|i| self.world.entities[i].race.clone()).unwrap_or_else(|| if id == self.diplo_own_civ() { "DWARF".into() } else { String::new() })
    }

    /// Shift the attitude of an entity (no-op for unknown entities).
    pub(crate) fn diplo_shift(&mut self, id: i32, d: i32) {
        if let Some(i) = self.diplo_entity_idx(id) {
            let e = &mut self.world.entities[i];
            e.attitude = clamp_att(e.attitude + d);
        }
    }

    fn diplo_setting(&self, key: &str, idx: usize, default: i64) -> i64 {
        self.war_setting(key, idx, default)
    }

    fn diplo_unit_alive(&self, id: i32) -> bool {
        id >= 0 && self.unit_slot.get(&id).map(|&s| self.ai[s].kind != crate::Kind::Dead && !self.world.units[self.ai[s].uidx].flags.dead).unwrap_or(false)
    }

    /// The fort's own government entity (the mayor's / expedition leader's), -1 if unknown.
    fn diplo_fort_entity(&self) -> i32 {
        self.world.nobles.iter().find(|n| matches!(n.token.as_str(), "MAYOR" | "EXPEDITION_LEADER")).map(|n| n.entity_id).unwrap_or(-1)
    }

    /// The noble who receives diplomats: land holder, else mayor, else expedition leader.
    pub(crate) fn diplo_host(&self) -> i32 {
        let fe = self.diplo_fort_entity();
        for toks in [&LAND_TOKENS[..], &["MAYOR"][..], &["EXPEDITION_LEADER"][..]] {
            for n in self.world.nobles.iter().rev() {
                if toks.contains(&n.token.as_str()) && (n.entity_id == fe || n.token == "MAYOR" || n.token == "EXPEDITION_LEADER") && self.diplo_unit_alive(n.holder) {
                    return n.holder;
                }
            }
        }
        -1
    }

    fn diplo_unit_name(&self, id: i32) -> String {
        self.unit_slot.get(&id).map(|&s| self.world.units[self.ai[s].uidx].name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "the envoy".into())
    }

    fn diplo_new_id(&mut self) -> i32 {
        self.diplo.next_id += 1;
        self.diplo.next_id
    }

    fn diplo_add_demand(&mut self, kind: DemandKind, entity: i32, text: String, amount: i64, due: u64) -> i32 {
        let id = self.diplo_new_id();
        self.diplo.demands.push(Demand { id, kind, entity_id: entity, text, amount, due, answer: None });
        self.diplo.rep.demands += 1;
        // Keep the list bounded: drop the oldest answered demands.
        if self.diplo.demands.len() > 200 {
            if let Some(k) = self.diplo.demands.iter().position(|d| d.answer.is_some()) {
                self.diplo.demands.remove(k);
            }
        }
        id
    }

    fn diplo_push_meeting(&mut self, m: Meeting) {
        self.diplo.meetings.push(m);
        self.diplo.rep.meetings += 1;
        if self.diplo.meetings.len() > 200 {
            self.diplo.meetings.remove(0);
        }
    }

    /// Land-holder level the fort qualifies for now (0 = none). Levels must be reached in
    /// order; each needs population AND created wealth AND exported wealth (estimate).
    pub(crate) fn diplo_land_level(&self) -> u8 {
        let pop = self.pop_counts().0 as i64;
        let (cw, ew) = (self.created_wealth(), self.exported_wealth());
        let mut lvl = 0u8;
        for k in 0..3 {
            let p = self.diplo_setting("LAND_HOLDER_POP_TRIGGER", k, LAND_POP[k]);
            let c = self.diplo_setting("LAND_HOLDER_PROD_TRIGGER", k, LAND_PROD[k]);
            let t = self.diplo_setting("LAND_HOLDER_TRADE_TRIGGER", k, LAND_TRADE[k]);
            if pop >= p && cw >= c && ew >= t {
                lvl = k as u8 + 1;
            } else {
                break;
            }
        }
        lvl
    }

    fn diplo_tree_limit(&self) -> i64 {
        if self.world.climate.savagery >= 66 {
            self.diplo_setting("TREE_FELL_COUNT_SAVAGE", 0, TREE_FELL_SAVAGE_DEFAULT)
        } else {
            self.diplo_setting("TREE_FELL_COUNT", 0, TREE_FELL_DEFAULT)
        }
    }

    // ------------------------------------------------------------------ hooks

    /// [hook] `trade::spawn_caravan`: a caravan of `entity` reached the map.
    pub(crate) fn diplomacy_caravan_arrived(&mut self, entity: i32, now: u64) {
        if self.diplo.off {
            return;
        }
        let envoy = self
            .trade
            .caravan
            .as_ref()
            .filter(|c| c.civ_id == entity) // a caravan that found no depot was never set up
            .and_then(|c| c.members.first())
            .map(|m| self.world.units[self.ai[m.slot as usize].uidx].id)
            .unwrap_or(-1);
        let own = entity == self.diplo_own_civ() || self.diplo_entity_idx(entity).map(|i| self.world.entities[i].relation == "Own").unwrap_or(false);
        let mut r = Rng::for_unit(self.seed, 13, now, 0xD1F0);
        if own {
            // The liaison travels with the caravan and asks for the meeting within a few days.
            let at = now + (1 + r.below(3)) as u64 * DAY;
            self.diplo.visits.push(Visit { at, entity, kind: MeetingKind::Liaison, envoy, peace: false });
        } else if self.diplo.title >= 1 && self.diplo_entity_idx(entity).is_some() {
            // Wiki Diplomat: other civilizations send diplomats once there is a land holder.
            let at = now + r.below(8) as u64 * DAY;
            self.diplo.visits.push(Visit { at, entity, kind: MeetingKind::Diplomat, envoy, peace: false });
        }
    }

    /// [hook] `trade::trade_impl` after a committed trade: offered / requested true values.
    pub(crate) fn diplomacy_traded(&mut self, offered: i64, requested: i64) {
        if self.diplo.off {
            return;
        }
        let Some(civ) = self.trade.caravan.as_ref().map(|c| c.civ_id) else { return };
        let profit = offered - requested;
        let d = if requested == 0 { (offered / 500).clamp(1, 10) } else { (profit / 1000).clamp(0, 5) };
        self.diplo_shift(civ, d as i32);
        // Gifts pay owed tribute.
        if requested == 0 {
            if let Some(t) = self.diplo.tribute.iter_mut().find(|t| t.0 == civ) {
                t.1 -= offered;
            }
            let now = self.ticks;
            if self.diplo.tribute.iter().any(|t| t.0 == civ && t.1 <= 0) {
                self.diplo.tribute.retain(|t| !(t.0 == civ && t.1 <= 0));
                let name = self.diplo_entity_name(civ);
                self.diplo_shift(civ, 10);
                self.announce(now, format!("The tribute to {name} has been paid."), None, AnnouncementKind::Info);
            }
        }
    }

    /// [hook] `trade::unload_goods`: number multiplier for a goods line of the caravan.
    pub(crate) fn diplomacy_request_mult(&self, kind: &str, sub: &str) -> u32 {
        if self.diplo.requests.is_empty() || self.diplo.agreements.is_empty() {
            return 1;
        }
        let Some(civ) = self.trade.caravan.as_ref().map(|c| c.civ_id) else { return 1 };
        let now = self.ticks;
        if !self.diplo.agreements.iter().any(|a| a.0 == civ && now <= a.1) {
            return 1;
        }
        let Some(req) = self.diplo.requests.iter().find(|r| r.0 == civ) else { return 1 };
        let hit = req.1.iter().any(|q| {
            let q = q.trim();
            !q.is_empty() && (q.eq_ignore_ascii_case(kind) || (!sub.is_empty() && sub.to_ascii_uppercase().contains(&q.to_ascii_uppercase())))
        });
        if hit {
            2
        } else {
            1
        }
    }

    /// [hook] `Sim::kill`: a unit died. Merchants, envoys and visitors of other civilizations
    /// killed on the map anger their civilization (natural deaths do not).
    pub(crate) fn diplomacy_unit_died(&mut self, s: usize, why: &str) {
        let killer = self.diplo.killer.take();
        if self.diplo.off || !self.diplo.init {
            return;
        }
        let u = &self.world.units[self.ai[s].uidx];
        if u.flags.hostile || u.flags.citizen || u.flags.animal || u.civ_id < 0 || !(u.flags.merchant || u.flags.visitor) {
            return;
        }
        // Only deaths the fort caused count: struck down by a fort dwarf, or crushed by the
        // fort's drawbridge. Goblins ambushing a caravan or a cave-in do not anger the civ.
        let by_fort = match killer {
            // Merchants and visitors run as workers too: only citizens count as the fort.
            Some(k) => k != s && k < self.ai.len() && { let f = &self.world.units[self.ai[k].uidx].flags; f.citizen && !f.hostile },
            None => why.contains("drawbridge"),
        };
        if !by_fort {
            return;
        }
        let (civ, id) = (u.civ_id, u.id);
        let envoy = self.diplo.visits.iter().any(|v| v.envoy == id) || self.diplo.meetings.iter().rev().take(8).any(|m| m.envoy == id && m.state != "Done");
        self.diplo.deaths.push((civ, id, envoy));
    }

    /// [hook] `Sim::fell_tree`.
    pub(crate) fn diplomacy_tree_felled(&mut self) {
        self.diplo.felled += 1;
    }

    // ------------------------------------------------------------------ daily pass

    fn diplo_init(&mut self) {
        self.diplo.init = true;
        self.diplo.last_year = self.world.calendar.year;
        self.diplo.last_season = self.world.calendar.month() / 3;
        let own = self.diplo_own_civ();
        let year = self.world.calendar.year;
        for e in &mut self.world.entities {
            if e.attitude == 0 {
                e.attitude = if e.relation == "War" {
                    -60
                } else if e.relation == "Own" || e.id == own {
                    50
                } else if e.relation == "Peace" && e.trades {
                    20
                } else {
                    0
                };
            }
            if e.relation == "War" && (e.kind.is_empty() || e.kind == "Civilization") && !self.diplo.wars.iter().any(|w| w.0 == e.id) {
                self.diplo.wars.push((e.id, year));
            }
        }
        // An imported fort that already has a land holder keeps its title.
        let fe = self.diplo_fort_entity();
        for (k, t) in LAND_TOKENS.iter().enumerate() {
            if self.world.nobles.iter().any(|n| n.token == *t && n.entity_id == fe && fe >= 0) {
                self.diplo.title = self.diplo.title.max(k as u8 + 1);
            }
        }
    }

    /// Baseline attitude an entity drifts back to.
    fn diplo_baseline(&self, i: usize) -> i32 {
        let e = &self.world.entities[i];
        if e.relation == "War" {
            0
        } else if e.relation == "Own" || e.id == self.diplo_own_civ() {
            50
        } else if e.relation == "Peace" && e.trades {
            20
        } else {
            0
        }
    }

    /// Hook from `tick_inner` (once a day).
    pub(crate) fn diplomacy_day(&mut self, now: u64) {
        if self.diplo.off || !self.has_civilization() {
            return;
        }
        if !self.diplo.init {
            self.diplo_init();
        }
        // Year start: quotas end, the felled-tree count restarts.
        let year = self.world.calendar.year;
        if year != self.diplo.last_year {
            self.diplo.last_year = year;
            self.diplo.felled = 0;
            self.diplo.quotas.clear();
        }
        // Season start: attitudes drift, war parties, peace envoys.
        let season = self.world.calendar.month() / 3;
        if season != self.diplo.last_season {
            self.diplo.last_season = season;
            self.diplo_season(now, season);
        }
        // Envoys arriving.
        let mut k = 0;
        while k < self.diplo.visits.len() {
            if self.diplo.visits[k].at <= now {
                let v = self.diplo.visits.remove(k);
                self.diplo_meet(v, now);
            } else {
                k += 1;
            }
        }
        // Killed people of other civilizations.
        if !self.diplo.deaths.is_empty() {
            let deaths = std::mem::take(&mut self.diplo.deaths);
            for (civ, _, envoy) in deaths {
                self.diplo_shift(civ, if envoy { -40 } else { -10 });
                if envoy {
                    let name = self.diplo_entity_name(civ);
                    self.announce(now, format!("The death of their envoy has outraged {name}."), None, AnnouncementKind::Warning);
                }
            }
        }
        // Tree quotas.
        let felled = self.diplo.felled;
        for q in 0..self.diplo.quotas.len() {
            let (civ, limit, over) = self.diplo.quotas[q];
            if felled > limit && !over {
                self.diplo.quotas[q].2 = true;
                self.diplo_shift(civ, -20);
                let name = self.diplo_entity_name(civ);
                self.announce(now, format!("{} is angered: more trees have been felled than agreed.", cap(&name)), None, AnnouncementKind::Warning);
            } else if over && felled > limit && now % (7 * DAY) < DAY {
                // Every further week of felling over the limit angers them more.
                self.diplo_shift(civ, -2);
            }
        }
        // Open demands past their deadline get their default answer.
        for k in 0..self.diplo.demands.len() {
            let d = &self.diplo.demands[k];
            if d.answer.is_none() && d.due > 0 && now >= d.due {
                let (id, kind) = (d.id, d.kind);
                match kind {
                    // The elves set their quota whether the fort agrees or not (wiki Diplomat).
                    DemandKind::TreeQuota => {
                        self.diplo_answer(id, true, now, false);
                    }
                    DemandKind::Tribute => {
                        self.diplo_answer(id, false, now, false);
                    }
                    // Requests and peace offers simply lapse.
                    DemandKind::Request | DemandKind::Peace => {
                        self.diplo.demands[k].answer = Some(false);
                    }
                }
            }
        }
        // Unpaid tribute.
        let mut k = 0;
        while k < self.diplo.tribute.len() {
            let (civ, owed, due) = self.diplo.tribute[k];
            if now >= due && owed > 0 {
                self.diplo.tribute.remove(k);
                self.diplo_shift(civ, -25);
                let name = self.diplo_entity_name(civ);
                self.announce(now, format!("The tribute demanded by {name} was not paid."), None, AnnouncementKind::Warning);
            } else {
                k += 1;
            }
        }
        // Declarations of war.
        self.diplo_check_war(now);
    }

    fn diplo_check_war(&mut self, now: u64) {
        let own = self.diplo_own_civ();
        for i in 0..self.world.entities.len() {
            let e = &self.world.entities[i];
            if e.relation != "Peace" || e.id == own || e.attitude > WAR_ATTITUDE || !(e.kind.is_empty() || e.kind == "Civilization") {
                continue;
            }
            let id = e.id;
            self.diplo_declare_war(id, now);
        }
    }

    /// `entity` declares war on the fort.
    pub(crate) fn diplo_declare_war(&mut self, entity: i32, now: u64) {
        let Some(i) = self.diplo_entity_idx(entity) else { return };
        let year = self.world.calendar.year;
        let e = &mut self.world.entities[i];
        if e.relation == "War" {
            return;
        }
        let trades = e.trades;
        e.relation = "War".into();
        e.trades = false;
        self.diplo.war_trades.retain(|w| w.0 != entity);
        self.diplo.war_trades.push((entity, trades));
        self.diplo.wars.retain(|w| w.0 != entity);
        self.diplo.wars.push((entity, year));
        if !self.diplo.declared.contains(&entity) {
            self.diplo.declared.push(entity);
        }
        // Their envoys and open demands are void.
        self.diplo.visits.retain(|v| v.entity != entity);
        self.diplo.tribute.retain(|t| t.0 != entity);
        self.diplo.quotas.retain(|q| q.0 != entity);
        for d in &mut self.diplo.demands {
            if d.entity_id == entity && d.answer.is_none() {
                d.answer = Some(false);
            }
        }
        // A due caravan of theirs does not come.
        if self.trade.due.as_ref().map(|d| d.1.id == entity).unwrap_or(false) {
            self.trade.due = None;
        }
        // Their merchants on the map pack up and leave at once.
        if self.trade.caravan.as_ref().map(|c| c.civ_id == entity && !c.leaving).unwrap_or(false) {
            self.caravan_leave(now);
        }
        self.diplo.rep.wars_declared += 1;
        let name = self.diplo_entity_name(entity);
        self.announce(now, format!("{} has declared war on {}!", cap(&name), self.diplo_fort_name()), None, AnnouncementKind::Warning);
    }

    fn diplo_make_peace(&mut self, entity: i32, now: u64) {
        let Some(i) = self.diplo_entity_idx(entity) else { return };
        let trades = self.diplo.war_trades.iter().find(|w| w.0 == entity).map(|w| w.1);
        let e = &mut self.world.entities[i];
        e.relation = "Peace".into();
        if let Some(t) = trades {
            e.trades = t;
        }
        e.attitude = e.attitude.max(0);
        self.diplo.war_trades.retain(|w| w.0 != entity);
        self.diplo.wars.retain(|w| w.0 != entity);
        self.diplo.declared.retain(|&d| d != entity);
        self.diplo.rep.peace_made += 1;
        let name = self.diplo_entity_name(entity);
        self.announce(now, format!("{} and {} have made peace.", self.diplo_fort_name(), name), None, AnnouncementKind::Info);
    }

    fn diplo_fort_name(&self) -> String {
        if self.world.meta.fort_name.is_empty() {
            "the fortress".into()
        } else {
            self.world.meta.fort_name.clone()
        }
    }

    fn diplo_season(&mut self, now: u64, season: i32) {
        // Attitudes drift a little towards their baseline.
        for i in 0..self.world.entities.len() {
            let b = self.diplo_baseline(i);
            let e = &mut self.world.entities[i];
            let d = b - e.attitude;
            e.attitude += d.signum() * d.abs().min(3);
        }
        let year = self.world.calendar.year;
        let mut r = Rng::for_unit(self.seed, 13, now, 0xD1F1);
        for w in 0..self.diplo.wars.len() {
            let (civ, since) = self.diplo.wars[w];
            let Some(i) = self.diplo_entity_idx(civ) else { continue };
            let (race, attitude) = (self.world.entities[i].race.clone(), self.world.entities[i].attitude);
            // War parties of civilizations that declared war here (goblin sieges: `raids`).
            if self.diplo.declared.contains(&civ) && race != "GOBLIN" && self.invasions_enabled() && r.chance(1, 3) {
                let n = 3 + r.below(5) as usize;
                if !race.is_empty() && self.spawn_raid(RaidKind::Ambush, &race, n, 0, 0) > 0 {
                    let name = self.diplo_entity_name(civ);
                    self.announce(now, format!("A war party of {name} has been sighted."), None, AnnouncementKind::Warning);
                }
            }
            // Peace envoys come in the civilization's caravan season.
            let their = match race.as_str() {
                "ELF" => 0,
                "HUMAN" => 1,
                "DWARF" => 2,
                _ => -1,
            };
            if their == season && year > since && attitude > PEACE_ATTITUDE && r.chance(1, 2) && !self.diplo.visits.iter().any(|v| v.entity == civ) {
                let at = now + (3 + r.below(25)) as u64 * DAY;
                self.diplo.visits.push(Visit { at, entity: civ, kind: MeetingKind::Diplomat, envoy: -1, peace: true });
            }
        }
    }

    /// An envoy meets the fort's host.
    fn diplo_meet(&mut self, v: Visit, now: u64) {
        let host = self.diplo_host();
        let name = self.diplo_entity_name(v.entity);
        let at_war = self.diplo_entity_idx(v.entity).map(|i| self.world.entities[i].relation == "War").unwrap_or(false);
        if at_war && !v.peace {
            return;
        }
        let id = self.diplo_new_id();
        let mut m = Meeting { id, kind: v.kind, entity_id: v.entity, envoy: v.envoy, host, state: "Done".into(), tick: now, text: String::new(), demands: Vec::new() };
        if host < 0 {
            // Wiki Diplomat: with nobody to receive them, envoys leave unhappy.
            m.state = "Missed".into();
            m.text = format!("The envoy of {name} found nobody to receive them and left.");
            self.diplo_shift(v.entity, -5);
            self.diplo_push_meeting(m);
            return;
        }
        let host_name = self.diplo_unit_name(host);
        let due = now + DEMAND_DAYS * DAY;
        let race = self.diplo_race(v.entity);
        match v.kind {
            MeetingKind::Liaison => {
                m.text = format!("The outpost liaison of {name} met with {host_name}.");
                let d = self.diplo_add_demand(DemandKind::Request, v.entity, format!("The liaison of {name} asks what {} wants from next year's caravan.", self.diplo_fort_name()), 0, due);
                m.demands.push(d);
                self.announce(now, m.text.clone(), None, AnnouncementKind::Info);
                self.diplo_push_meeting(m);
                self.diplo_elevate(v.entity, host, now);
                return;
            }
            _ if v.peace => {
                m.text = format!("An envoy of {name} met with {host_name} to offer peace.");
                let d = self.diplo_add_demand(DemandKind::Peace, v.entity, format!("{} offers peace.", cap(&name)), 0, due);
                m.demands.push(d);
            }
            _ if race == "ELF" => {
                let limit = self.diplo_tree_limit();
                m.text = format!("The diplomat of {name} met with {host_name}.");
                let d = self.diplo_add_demand(DemandKind::TreeQuota, v.entity, format!("{} asks that no more than {limit} trees be felled this year.", cap(&name)), limit, due);
                m.demands.push(d);
            }
            _ => {
                m.text = format!("The diplomat of {name} met with {host_name}.");
                let att = self.diplo_entity_idx(v.entity).map(|i| self.world.entities[i].attitude).unwrap_or(0);
                if att < 0 {
                    // Estimate: tribute grows with their displeasure and the fort's exports.
                    let amount = (500 + (-att as i64) * 50 + self.exported_wealth() / 100).min(50_000);
                    let d = self.diplo_add_demand(DemandKind::Tribute, v.entity, format!("{} demands tribute worth {amount}\u{263c}.", cap(&name)), amount, due);
                    m.demands.push(d);
                }
            }
        }
        self.announce(now, m.text.clone(), None, AnnouncementKind::Info);
        self.diplo_push_meeting(m);
    }

    /// Liaison meeting: rise one land-holder level if the fort qualifies.
    fn diplo_elevate(&mut self, entity: i32, host: i32, now: u64) {
        if self.diplo.title >= 3 || self.diplo_land_level() <= self.diplo.title {
            return;
        }
        self.diplo.title += 1;
        let lvl = self.diplo.title as usize;
        let title = ["", "barony", "county", "duchy"][lvl];
        let fort = self.diplo_fort_name();
        let id = self.diplo_new_id();
        let text = format!("{fort} has been made a {title}.");
        self.diplo_push_meeting(Meeting { id, kind: MeetingKind::Elevation, entity_id: entity, envoy: -1, host, state: "Done".into(), tick: now, text: text.clone(), demands: Vec::new() });
        self.diplo.rep.elevations += 1;
        self.announce(now, text, None, AnnouncementKind::Info);
        // The land holder: the previous one is promoted, else the host is appointed (estimate).
        let fe = self.diplo_fort_entity();
        let tok = LAND_TOKENS[lvl - 1];
        let pos = self.world.nobles.iter().position(|n| LAND_TOKENS.contains(&n.token.as_str()) && n.entity_id == fe);
        let holder = match pos {
            Some(p) if self.diplo_unit_alive(self.world.nobles[p].holder) => self.world.nobles[p].holder,
            _ => host,
        };
        match pos {
            Some(p) => {
                let n = &mut self.world.nobles[p];
                n.token = tok.into();
                n.name = tok.to_ascii_lowercase();
                n.holder = holder;
            }
            None => self.world.nobles.push(rf_core::NoblePosition { token: tok.into(), name: tok.to_ascii_lowercase(), holder, entity_id: fe, mandates: Vec::new(), ..Default::default() }),
        }
        self.diplo.holders.push((lvl as u8, holder));
        let hn = self.diplo_unit_name(holder);
        let text = format!("{hn} is now the {} of {fort}.", tok.to_ascii_lowercase());
        let id = self.diplo_new_id();
        self.diplo_push_meeting(Meeting { id, kind: MeetingKind::LandHolder, entity_id: entity, envoy: -1, host: holder, state: "Done".into(), tick: now, text: text.clone(), demands: Vec::new() });
        self.announce(now, text, None, AnnouncementKind::Info);
    }

    /// Apply an answer. `player`: answered by the player (counts in the report).
    fn diplo_answer(&mut self, demand: i32, accept: bool, now: u64, player: bool) -> bool {
        let Some(k) = self.diplo.demands.iter().position(|d| d.id == demand && d.answer.is_none()) else { return false };
        self.diplo.demands[k].answer = Some(accept);
        let (kind, civ, amount) = (self.diplo.demands[k].kind, self.diplo.demands[k].entity_id, self.diplo.demands[k].amount);
        if accept {
            self.diplo.rep.accepted += 1;
        } else {
            self.diplo.rep.refused += 1;
        }
        let _ = player; // reserved: the UI may show who answered
        let year_end = now + (TICKS_PER_YEAR - self.world.calendar.year_tick).max(0) as u64;
        match (kind, accept) {
            (DemandKind::Request, true) => {
                self.diplo.agreements.retain(|a| a.0 != civ);
                self.diplo.agreements.push((civ, now + TICKS_PER_YEAR as u64 + TICKS_PER_MONTH as u64));
                self.diplo_shift(civ, 2);
            }
            (DemandKind::Request, false) => {}
            (DemandKind::TreeQuota, true) => {
                self.diplo.quotas.retain(|q| q.0 != civ);
                // Trees felled earlier this year count against it (wiki: yearly allowance).
                self.diplo.quotas.push((civ, amount, false));
                self.diplo_shift(civ, 5);
            }
            (DemandKind::TreeQuota, false) => {
                self.diplo.quotas.retain(|q| q.0 != civ);
                self.diplo.quotas.push((civ, amount, false));
                self.diplo_shift(civ, -25);
            }
            (DemandKind::Tribute, true) => {
                self.diplo.tribute.retain(|t| t.0 != civ);
                self.diplo.tribute.push((civ, amount, year_end.max(now + 3 * TICKS_PER_MONTH as u64)));
                self.diplo_shift(civ, 10);
            }
            (DemandKind::Tribute, false) => {
                self.diplo_shift(civ, -30);
            }
            (DemandKind::Peace, true) => {
                self.diplo_make_peace(civ, now);
            }
            (DemandKind::Peace, false) => {
                self.diplo_shift(civ, -15);
            }
        }
        self.diplo_check_war(now);
        true
    }

    /// Part of `state_hash`; mixes nothing while the module never ran.
    pub(crate) fn diplomacy_hash(&self, mix: &mut dyn FnMut(u64)) {
        let d = &self.diplo;
        if !d.init {
            return;
        }
        mix(d.title as u64);
        mix(d.next_id as u64);
        mix(d.felled as u64);
        mix(d.meetings.len() as u64 ^ (d.demands.len() as u64) << 16 ^ (d.visits.len() as u64) << 32);
        mix(d.rep.meetings ^ d.rep.demands << 8 ^ d.rep.accepted << 16 ^ d.rep.refused << 24 ^ d.rep.wars_declared << 32 ^ d.rep.peace_made << 40 ^ d.rep.elevations << 48);
        for e in &self.world.entities {
            mix((e.id as u32 as u64) << 32 | (e.attitude as u32 as u64) ^ ((e.relation == "War") as u64) << 31);
        }
    }

    // ------------------------------------------------------------------ round-13 API

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
                at_war_since: if e.relation == "War" { self.diplo.wars.iter().find(|w| w.0 == e.id).map(|w| w.1) } else { None },
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
        let now = self.ticks;
        self.diplo_answer(demand, accept, now, true)
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

    /// [diplo13] Trees felled this year and the quotas in force: (entity, allowed).
    pub fn tree_quota(&self) -> (i64, Vec<(i32, i64)>) {
        (self.diplo.felled, self.diplo.quotas.iter().map(|q| (q.0, q.1)).collect())
    }

    /// [diplo13] Tribute still owed: (entity, value, due tick).
    pub fn tribute_owed(&self) -> Vec<(i32, i64, u64)> {
        self.diplo.tribute.clone()
    }

    /// [diplo13] Debug/UI: an envoy of `entity` arrives now (`peace`: with a peace offer).
    /// Returns the meeting id, or None if nobody could meet them.
    pub fn force_envoy(&mut self, entity: i32, peace: bool) -> Option<i32> {
        if !self.diplo.init {
            self.diplo_init();
        }
        let now = self.ticks;
        let own = entity == self.diplo_own_civ();
        let kind = if own { MeetingKind::Liaison } else { MeetingKind::Diplomat };
        self.diplo_meet(Visit { at: now, entity, kind, envoy: -1, peace }, now);
        self.diplo.meetings.last().filter(|m| m.tick == now && m.entity_id == entity && m.state == "Done").map(|m| m.id)
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{EntityInfo, NoblePosition, World};

    fn civ(id: i32, race: &str, rel: &str, trades: bool) -> EntityInfo {
        EntityInfo { id, name: format!("the {} folk {id}", race.to_ascii_lowercase()), race: race.into(), kind: "Civilization".into(), relation: rel.into(), trades, ..Default::default() }
    }

    fn sim() -> Sim {
        let mut w = World::synthetic(32, 32, 20);
        w.meta.source = "export:test".into();
        w.meta.civ_id = 10;
        w.meta.fort_name = "Testfort".into();
        w.entities = vec![civ(10, "DWARF", "Own", true), civ(11, "ELF", "Peace", true), civ(12, "HUMAN", "Peace", true), civ(13, "GOBLIN", "War", false), civ(14, "HUMAN", "War", false)];
        let leader = w.units.iter().find(|u| u.flags.citizen).map(|u| u.id).unwrap_or(w.units[0].id);
        w.nobles.push(NoblePosition { token: "EXPEDITION_LEADER".into(), name: "expedition leader".into(), holder: leader, entity_id: 500, mandates: Vec::new(), ..Default::default() });
        let mut sim = Sim::new(w, 77);
        sim.diplomacy_day(0);
        sim
    }

    #[test]
    fn baselines_and_wars_are_set_once() {
        let s = sim();
        let e = s.diplomacy_entities();
        assert_eq!(e.iter().find(|e| e.id == 10).unwrap().attitude, 50);
        assert_eq!(e.iter().find(|e| e.id == 11).unwrap().attitude, 20);
        assert_eq!(e.iter().find(|e| e.id == 13).unwrap().attitude, -60);
        assert!(e.iter().find(|e| e.id == 13).unwrap().at_war_since.is_some());
        assert_eq!(s.land_title(), "");
    }

    #[test]
    fn liaison_meeting_request_and_elevation() {
        let mut s = sim();
        // Without wealth: no elevation.
        let m = s.force_envoy(10, false).expect("liaison met");
        let meet = s.diplomacy_meetings().into_iter().find(|x| x.id == m).unwrap();
        assert_eq!(meet.kind, MeetingKind::Liaison);
        assert_eq!(s.land_title(), "");
        let d = meet.demands[0];
        s.set_liaison_requests(10, &["BAR", "pick"]);
        assert!(s.answer_demand(d, true));
        assert!(!s.answer_demand(d, true), "only once");
        assert_eq!(s.diplomacy_report().accepted, 1);
        // The agreement doubles requested goods of their caravan only.
        assert_eq!(s.diplomacy_request_mult("BAR", ""), 1, "no caravan on the map");
        // With the triggers met the fort becomes a barony, then a county.
        s.world.settings.other = vec!["LAND_HOLDER_POP_TRIGGER=1,1,1".into(), "LAND_HOLDER_PROD_TRIGGER=0,0,0".into(), "LAND_HOLDER_TRADE_TRIGGER=0,0,1000000000".into()];
        s.force_envoy(10, false);
        assert_eq!(s.land_title(), "Barony");
        assert!(s.world.nobles.iter().any(|n| n.token == "BARON" && n.holder >= 0));
        s.force_envoy(10, false);
        assert_eq!(s.land_title(), "County");
        assert!(s.world.nobles.iter().any(|n| n.token == "COUNT") && !s.world.nobles.iter().any(|n| n.token == "BARON" && n.entity_id == 500));
        s.force_envoy(10, false);
        assert_eq!(s.land_title(), "County", "duchy needs exported wealth");
        assert_eq!(s.diplomacy_report().elevations, 2);
        assert!(s.diplomacy_meetings().iter().any(|m| m.kind == MeetingKind::LandHolder));
    }

    #[test]
    fn elven_tree_quota_counts_felled_trees() {
        let mut s = sim();
        s.force_envoy(11, false).unwrap();
        let d = s.diplomacy_demands().into_iter().find(|d| d.kind == DemandKind::TreeQuota).expect("quota");
        assert_eq!(d.amount, 25);
        assert!(s.answer_demand(d.id, true));
        let a0 = s.diplomacy_entities().iter().find(|e| e.id == 11).unwrap().attitude;
        for _ in 0..26 {
            s.diplomacy_tree_felled();
        }
        s.diplomacy_day(DAY);
        let a1 = s.diplomacy_entities().iter().find(|e| e.id == 11).unwrap().attitude;
        assert_eq!(a1, a0 - 20);
        assert!(s.announcements.iter().any(|a| a.text.contains("more trees have been felled")));
        assert_eq!(s.tree_quota().0, 26);
    }

    #[test]
    fn refused_demands_lead_to_war_and_war_stops_caravans() {
        let mut s = sim();
        s.world.entities.iter_mut().find(|e| e.id == 12).unwrap().attitude = -10;
        s.force_envoy(12, false).unwrap();
        let d = s.diplomacy_demands().into_iter().find(|d| d.kind == DemandKind::Tribute).expect("tribute");
        assert!(d.amount > 0);
        assert!(s.answer_demand(d.id, false));
        // -10 - 30 = -40: not yet war; a killed merchant tips it over.
        assert_eq!(s.diplomacy_entities().iter().find(|e| e.id == 12).unwrap().relation, "Peace");
        s.diplo.deaths.push((12, 999, false));
        s.diplomacy_day(2 * DAY);
        let e = s.diplomacy_entities().into_iter().find(|e| e.id == 12).unwrap();
        assert_eq!(e.relation, "War");
        assert!(!e.trades);
        assert!(e.at_war_since.is_some());
        assert_eq!(s.diplomacy_report().wars_declared, 1);
        s.world.calendar.year_tick = 3 * TICKS_PER_MONTH; // summer: human caravans
        s.trade_season(3 * DAY);
        assert!(s.trade.due.as_ref().map(|d| d.1.id != 12).unwrap_or(true), "no caravan from a civ at war");
    }

    #[test]
    fn peace_offer_restores_trade() {
        let mut s = sim();
        s.diplo_declare_war(12, 10);
        assert!(!s.world.entities.iter().find(|e| e.id == 12).unwrap().trades);
        s.force_envoy(12, true).unwrap();
        let d = s.diplomacy_demands().into_iter().find(|d| d.kind == DemandKind::Peace).expect("peace offer");
        assert!(s.answer_demand(d.id, true));
        let e = s.world.entities.iter().find(|e| e.id == 12).unwrap();
        assert_eq!(e.relation, "Peace");
        assert!(e.trades);
        assert_eq!(s.diplomacy_report().peace_made, 1);
        assert!(s.diplomacy_entities().iter().find(|e| e.id == 12).unwrap().at_war_since.is_none());
    }

    #[test]
    fn unanswered_demands_get_defaults_and_trade_raises_attitude() {
        let mut s = sim();
        s.force_envoy(11, false).unwrap();
        s.diplomacy_day(DEMAND_DAYS * DAY + 1);
        let d = s.diplomacy_demands().into_iter().find(|d| d.kind == DemandKind::TreeQuota).unwrap();
        assert_eq!(d.answer, Some(true), "the elves impose their quota");
        assert_eq!(s.tree_quota().1, vec![(11, 25)]);
        // No host: the envoy leaves unhappy.
        s.world.nobles.clear();
        assert!(s.force_envoy(12, false).is_none());
        assert_eq!(s.diplomacy_meetings().last().unwrap().state, "Missed");
    }

    #[test]
    fn only_merchants_killed_by_the_fort_count() {
        let mut s = sim();
        let slots: Vec<usize> = (0..s.ai.len()).filter(|&k| s.ai[k].kind == crate::Kind::Worker).collect();
        assert!(slots.len() >= 3);
        let (victim, dwarf) = (slots[0], slots[1]);
        let u = &mut s.world.units[s.ai[victim].uidx];
        u.flags.citizen = false;
        u.flags.merchant = true;
        u.civ_id = 12;
        // Killed by something that is not the fort (no attacker recorded): ignored.
        s.diplomacy_unit_died(victim, "has been struck down");
        assert!(s.diplo.deaths.is_empty());
        // Struck down by another merchant (not a citizen): ignored.
        let other = s.ai[slots[2]].uidx;
        s.world.units[other].flags.citizen = false;
        s.diplo.killer = Some(slots[2]);
        s.diplomacy_unit_died(victim, "has been struck down");
        assert!(s.diplo.deaths.is_empty());
        // Struck down by a fort dwarf: counts.
        s.diplo.killer = Some(dwarf);
        s.diplomacy_unit_died(victim, "has been struck down");
        assert_eq!(s.diplo.deaths.len(), 1);
        assert!(s.diplo.killer.is_none());
        // The drawbridge is the fort's doing too.
        s.diplomacy_unit_died(victim, "has been crushed by a drawbridge");
        assert_eq!(s.diplo.deaths.len(), 2);
    }

    #[test]
    fn disabled_module_does_nothing() {
        let mut w = World::synthetic(32, 32, 20);
        w.meta.source = "export:test".into();
        let mut s = Sim::new(w, 5);
        s.set_diplomacy_enabled(false);
        s.diplomacy_day(0);
        let mut n = 0;
        s.diplomacy_hash(&mut |_| n += 1);
        assert_eq!(n, 0);
    }
}
