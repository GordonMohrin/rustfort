//! [nobles15] Public, UI-facing API of the noble simulation (`nobles15`): requirement status
//! per noble, mandates, demands, zone assignment, bookkeeper precision, broker appraisal.
//!
//! Everything here reads the sim or changes player-facing settings; it never advances time.

use crate::nobles15::{self, room_kind_name, tier_name, tier_of, RK_BEDROOM, RK_DINING, RK_OFFICE, RK_TOMB};
use crate::Sim;
pub use crate::nobles15::{shown_count, NoblesReport};

/// State of one requirement (colours as DF's nobles screen: red missing, yellow too low, green met).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReqState {
    /// The position has no such requirement.
    None,
    /// Required room missing (red).
    Missing,
    /// Present but its value is too low, or a furniture count is short (yellow).
    Low,
    /// Fulfilled (green).
    Met,
}

/// A room requirement of a noble.
#[derive(Clone, Debug)]
pub struct RoomReq {
    /// "bedroom", "office", "dining room", "tomb".
    pub kind: &'static str,
    /// Required value (1 = any room of that kind).
    pub required: i32,
    /// DF's name for the required tier ("Decent Office", ...).
    pub required_name: &'static str,
    /// Best assigned zone of that kind, its value and tier name.
    pub zone: i32,
    pub value: i32,
    pub tier_name: &'static str,
    pub state: ReqState,
}

/// A furniture requirement: built pieces in the noble's bedroom and office.
#[derive(Clone, Debug)]
pub struct FurnReq {
    /// "chests", "cabinets", "weapon racks", "armor stands".
    pub kind: &'static str,
    pub have: i32,
    pub need: i32,
}

/// A mandate as shown to the player.
#[derive(Clone, Debug)]
pub struct MandateView {
    pub id: i32,
    pub noble: i32,
    pub position: String,
    pub text: String,
    pub export_ban: bool,
    pub amount_total: i32,
    pub amount_remaining: i32,
    /// Ticks until the deadline (production) or until the ban ends.
    pub ticks_left: u64,
    pub strikes: u8,
    pub jail_months: u8,
}

/// A demand as shown to the player (w:Demand colours).
#[derive(Clone, Debug)]
pub struct DemandView {
    pub id: i32,
    pub noble: i32,
    pub text: String,
    /// "met" (green), "pending" (yellow), "running out" (orange, last month), "overdue" (red).
    pub status: &'static str,
}

/// Requirement status of one position holder.
#[derive(Clone, Debug)]
pub struct NobleReq {
    pub unit: i32,
    pub name: String,
    pub position: String,
    pub rooms: Vec<RoomReq>,
    pub furniture: Vec<FurnReq>,
    /// Worst state over rooms and furniture.
    pub state: ReqState,
    pub mandate_max: i32,
    pub demand_max: i32,
    pub punishment_exempt: bool,
    pub sleep_pretension: bool,
}

/// A bookkeeper-reported stock count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockCount {
    /// As shown at the reached precision ("?" before the first records).
    pub shown: String,
    /// The count in the records (not the live count).
    pub recorded: i32,
}

impl Sim {
    /// Holders on the map with any requirement or mandate/demand limit, highest rank first.
    pub fn noble_requirements(&self) -> Vec<NobleReq> {
        let mut out = Vec::new();
        let holders = nobles15::holders_of(&self.world.nobles, |id| self.unit_slot.contains_key(&id));
        for (unit, req, ni) in holders {
            let has_any = req.office > 0 || req.bedroom > 0 || req.dining > 0 || req.tomb > 0 || req.boxes > 0 || req.mandate_max > 0 || req.demand_max > 0;
            if !has_any {
                continue;
            }
            let mut rooms = Vec::new();
            for (k, need) in [(RK_OFFICE, req.office), (RK_BEDROOM, req.bedroom), (RK_DINING, req.dining), (RK_TOMB, req.tomb)] {
                if need <= 0 {
                    continue;
                }
                let best = self.nobles15_best_room(unit, k);
                let (zone, value) = best.unwrap_or((-1, 0));
                let state = match best {
                    None => ReqState::Missing,
                    Some((_, v)) if v >= need || need <= 1 => ReqState::Met, // 1 = any room
                    _ => ReqState::Low,
                };
                rooms.push(RoomReq {
                    kind: room_kind_name(k),
                    required: need,
                    required_name: if need <= 1 { tier_name(k, 0) } else { tier_name(k, tier_of(need)) },
                    zone,
                    value,
                    tier_name: if best.is_some() { tier_name(k, tier_of(value)) } else { "-" },
                    state,
                });
            }
            let mut furniture = Vec::new();
            for (kind, name, need) in [("Box", "chests", req.boxes), ("Cabinet", "cabinets", req.cabinets), ("Weaponrack", "weapon racks", req.racks), ("Armorstand", "armor stands", req.stands)] {
                if need > 0 {
                    furniture.push(FurnReq { kind: name, have: self.nobles15_furniture(unit, kind), need });
                }
            }
            let mut state = ReqState::None;
            for r in &rooms {
                state = worse(state, r.state);
            }
            for f in &furniture {
                state = worse(state, if f.have >= f.need { ReqState::Met } else { ReqState::Low });
            }
            let n = &self.world.nobles[ni];
            out.push(NobleReq {
                unit,
                name: self.world.unit(unit).map(|u| u.name.clone()).unwrap_or_default(),
                position: if n.name.is_empty() { n.token.to_ascii_lowercase().replace('_', " ") } else { n.name.clone() },
                rooms,
                furniture,
                state,
                mandate_max: req.mandate_max,
                demand_max: req.demand_max,
                punishment_exempt: req.punishment_exempt,
                sleep_pretension: req.sleep_pretension,
            });
        }
        out
    }

    /// Active mandates (production orders and export bans).
    pub fn mandates(&self) -> Vec<MandateView> {
        let now = self.ticks;
        self.n15
            .mandates
            .iter()
            .map(|m| MandateView {
                id: m.id,
                noble: m.noble,
                position: m.position.clone(),
                text: m.text.clone(),
                export_ban: m.export_ban,
                amount_total: m.amount_total,
                amount_remaining: m.amount_remaining,
                ticks_left: m.deadline.saturating_sub(now),
                strikes: m.strikes,
                jail_months: m.jail_months,
            })
            .collect()
    }

    /// Open and recently met demands.
    pub fn noble_demands(&self) -> Vec<DemandView> {
        let now = self.ticks;
        self.n15
            .demands
            .iter()
            .map(|d| DemandView {
                id: d.id,
                noble: d.noble,
                text: d.text.clone(),
                status: if d.met > 0 {
                    "met"
                } else if now > d.due {
                    "overdue"
                } else if d.due - now <= rf_core::TICKS_PER_MONTH as u64 {
                    "running out"
                } else {
                    "pending"
                },
            })
            .collect()
    }

    /// Fort goods in the trade depot that an export ban covers (purple in DF's trade window).
    pub fn banned_goods(&self) -> Vec<i32> {
        let bans: Vec<&nobles15::MandateRt> = self.n15.mandates.iter().filter(|m| m.export_ban).collect();
        if bans.is_empty() {
            return Vec::new();
        }
        let depots: Vec<i32> = self.world.buildings.iter().filter(|b| b.kind == "TradeDepot").map(|b| b.id).collect();
        self.world
            .items
            .iter()
            .enumerate()
            .filter(|(i, it)| !self.imeta[*i].dead && it.building >= 0 && depots.contains(&it.building) && bans.iter().any(|m| nobles15::ban_hits(m, it)))
            .map(|(_, it)| it.id)
            .collect()
    }

    /// Assign a bedroom, office, dining hall or tomb zone to a unit (`-1` = nobody). False if
    /// the zone is not such a zone or the unit is unknown.
    pub fn assign_zone(&mut self, zone_id: i32, unit_id: i32) -> bool {
        self.nobles15_assign_zone(zone_id, unit_id)
    }

    /// Bookkeeper precision: (target level 1..=5, reached level, 0 = no records yet).
    pub fn bookkeeper_level(&self) -> (u8, u8) {
        (self.n15.bk.level, self.n15.bk.reached)
    }

    /// Set the bookkeeper's target precision (1..=5, w:Bookkeeper).
    pub fn set_bookkeeper_level(&mut self, level: u8) {
        self.n15.bk.level = level.clamp(1, 5);
    }

    /// Stock count of a DF item type as the bookkeeper's records show it.
    pub fn stock_count(&self, kind: &str) -> StockCount {
        let reached = self.n15.bk.reached;
        if reached == 0 {
            return StockCount { shown: "?".into(), recorded: 0 };
        }
        let n = self.n15.bk.records.iter().find(|(k, _)| k == kind).map(|e| e.1).unwrap_or(0);
        StockCount { shown: shown_count(reached, n), recorded: n }
    }

    /// Value of an item as the broker appraises it; `None` without a broker who has any
    /// appraisal skill (w:Bookkeeper: item values depend on the broker's appraisal).
    pub fn appraised_value(&self, item_id: i32) -> Option<i64> {
        let b = self.broker_unit()?;
        if self.world.unit(b)?.skill("APPRAISAL") <= 0 {
            return None;
        }
        let it = self.world.item(item_id)?;
        Some(self.shown_value(item_id, it.value.max(0) as i64))
    }

    /// What the noble simulation did so far.
    pub fn nobles15_report(&self) -> NoblesReport {
        self.n15.rep.clone()
    }

    /// Switch the noble simulation off (the round-4 mandate code runs instead) or on.
    pub fn set_nobles15_enabled(&mut self, on: bool) {
        self.n15.off = !on;
    }
}

fn worse(a: ReqState, b: ReqState) -> ReqState {
    let rank = |s: ReqState| match s {
        ReqState::None => 0,
        ReqState::Met => 1,
        ReqState::Low => 2,
        ReqState::Missing => 3,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}
