//! Trade (round 4): caravans, the trade depot, the broker and bartering.
//!
//! # DF rules implemented (wiki v50: Caravan, Trade depot, Broker, Trading, Value, Merchant)
//! - Trade partners are the civilizations in `World.entities` with `trades` (not at war); without
//!   exported entities the fort's own dwarven civilization trades. Elves come in spring, humans
//!   in summer, the dwarven caravan in autumn ("A caravan from <civ> has arrived.").
//! - The caravan enters at a map edge connected to the trade depot. Wagons need a path that is
//!   3 tiles wide all the way from the edge to the depot; without one, dwarves and humans come
//!   with pack animals only (elves never bring wagons).
//! - Arriving at the depot the merchants unload their goods ("The merchants have arrived and are
//!   unloading their goods."), stay a while, warn ("will be leaving soon"), and leave the map
//!   again ("have embarked on their journey"). Unsold goods go with them.
//! - Fort goods are brought to the depot by haulers (DF "Move goods to/from depot", labor
//!   HAUL_TRADE) and stay there until traded. Trading needs the broker at the depot: the
//!   appointed BROKER, else the expedition leader/mayor (who does all jobs early on), else the
//!   citizen with the best Appraisal.
//! - Item value = base value of the item type x material value x quality multiplier
//!   (1, 2, 3, 4, 5, 12, 120 for ordinary .. artifact), containers with their contents; the
//!   exported DF value is used where present. The broker's Appraisal skill only changes the
//!   displayed values, not the merchants' decision. Merchants want a profit; elves refuse wood.
//!
//! # Estimates (not on the wiki)
//! - Arrival 7-27 days into the season; stay 16 days after unloading (+2 days per trade, at
//!   most +8); leaving warning 3 days before; give up after a month if the depot is never
//!   reached. Merchants 2-4, pack animals 3-6 (2 draft animals per wagon), wagons 1-3.
//! - Merchants accept when the offer is worth at least 110 % of the request; replies by ratio.
//! - Displayed values deviate by up to (25 - 2 x Appraisal) % per item.
//! - Goods lists per race and the base values of item types are approximations.

use crate::path::Goals;
use crate::rng::{splitmix, Rng};
use crate::{labor, AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::{EntityInfo, Item, ItemFlags, Pos, Shape, Unit, UnitFlags, TICKS_PER_DAY, TICKS_PER_MONTH};

const DAY: u64 = TICKS_PER_DAY as u64;
const MONTH: u64 = TICKS_PER_MONTH as u64;
/// Merchants' required profit in percent (estimate).
pub const PROFIT_PCT: i64 = 10;
const STAY_TICKS: u64 = 16 * DAY;

pub(crate) struct Member {
    pub slot: u32,
    pub dest: u32,
    pub tries: u8,
    pub arrived: bool,
}

pub(crate) struct Caravan {
    pub civ_id: i32,
    pub civ: String,
    pub race: String,
    pub depot: i32,
    pub edge: u32,
    pub members: Vec<Member>,
    pub arrived: u64,
    pub unloaded: Option<u64>,
    pub leave_at: u64,
    pub warned: bool,
    pub leaving: bool,
    pub goods: Vec<i32>,
    pub wagons: u32,
    pub extended: u64,
    pub log: usize,
}

pub(crate) struct DepotMove {
    pub item: i32,
    pub claimed: u32,
    /// When the move was ordered (unclaimed moves are given up after a week).
    pub since: u64,
}

/// One caravan visit (for the UI, the bench and tests).
#[derive(Clone, Debug, Default)]
pub struct CaravanLog {
    pub civ: String,
    pub race: String,
    pub arrived: u64,
    pub unloaded: Option<u64>,
    pub left: Option<u64>,
    pub merchants: usize,
    pub animals: usize,
    pub wagons: u32,
    pub goods: usize,
    pub goods_value: i64,
    pub trades: usize,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct TradeReport {
    pub caravans: Vec<CaravanLog>,
    /// Accepted trades: (tick, offered value, requested value, reply).
    pub trades: Vec<(u64, i64, i64, String)>,
    pub items_moved_to_depot: usize,
}

#[derive(Default)]
pub(crate) struct Trade {
    pub caravan: Option<Caravan>,
    /// Scheduled arrival: (tick, entity).
    pub due: Option<(u64, EntityInfo)>,
    pub owned: Vec<u32>,
    pub pending: Vec<DepotMove>,
    pub broker_called: bool,
    pub rep: TradeReport,
    pub off: bool,
    /// Cached depot tile for haulers (NONE = no depot) and whether anyone has HAUL_TRADE.
    pub dest: u32,
    pub trade_labor: bool,
}

/// Base value of an item type (wiki "Item value"; approximate).
pub fn base_value(kind: &str) -> i64 {
    match kind {
        "BAR" => 5,
        "BLOCKS" => 3,
        "BOULDER" | "WOOD" => 1,
        "ROUGH" => 6,
        "SMALLGEM" => 10,
        "DRINK" | "MEAT" | "FISH" | "PLANT" | "PLANT_GROWTH" | "CHEESE" | "EGG" | "SEEDS" => 2,
        "FOOD" => 10,
        "CLOTH" => 7,
        "THREAD" => 6,
        "SKIN_TANNED" => 10,
        "WEAPON" | "ARMOR" | "HELM" | "SHIELD" | "PANTS" | "GLOVES" | "SHOES" | "ANVIL" => 10,
        "AMMO" => 1,
        "RING" | "AMULET" | "FIGURINE" | "BRACELET" | "EARRING" | "CROWN" | "SCEPTER" | "GOBLET" | "TOY" | "INSTRUMENT" | "TOTEM" => 10,
        "BARREL" | "BIN" | "BAG" | "BOX" | "TABLE" | "CHAIR" | "BED" | "DOOR" | "CABINET" | "STATUE" | "COFFIN" => 5,
        _ => 3,
    }
}

/// DF quality value multipliers (wiki "Quality").
pub fn quality_mult(q: u8) -> i64 {
    [1, 2, 3, 4, 5, 12, 120][(q as usize).min(6)]
}

/// Goods a caravan of `race` brings: (kind, subtype, material selector, min, max).
fn goods_list(race: &str) -> &'static [(&'static str, &'static str, &'static str, u32, u32)] {
    match race {
        "ELF" => &[
            ("CLOTH", "", "THREAD", 3, 8),
            ("PLANT", "", "PLANT", 4, 10),
            ("DRINK", "", "DRINK", 3, 6),
            ("FIGURINE", "", "WOOD", 2, 5),
            ("BRACELET", "", "WOOD", 1, 3),
            ("TOY", "", "WOOD", 1, 3),
            ("SEEDS", "", "SEED", 2, 6),
        ],
        "HUMAN" => &[
            ("CLOTH", "", "THREAD", 2, 6),
            ("SKIN_TANNED", "", "LEATHER", 2, 6),
            ("WOOD", "", "WOOD", 4, 10),
            ("MEAT", "", "MUSCLE", 3, 8),
            ("DRINK", "", "DRINK", 2, 5),
            ("WEAPON", "ITEM_WEAPON_SWORD_SHORT", "INORGANIC:BRONZE", 1, 3),
            ("WEAPON", "ITEM_WEAPON_SPEAR", "INORGANIC:IRON", 1, 3),
            ("ARMOR", "ITEM_ARMOR_MAIL_SHIRT", "INORGANIC:BRONZE", 0, 2),
            ("BAR", "", "INORGANIC:TIN", 1, 4),
            ("AMULET", "", "INORGANIC:SILVER", 1, 3),
        ],
        _ => &[
            ("BAR", "", "INORGANIC:IRON", 3, 8),
            ("BAR", "", "INORGANIC:STEEL", 1, 4),
            ("BAR", "", "INORGANIC:COPPER", 2, 6),
            ("BAR", "", "INORGANIC:SILVER", 1, 3),
            ("BAR", "", "INORGANIC:GOLD", 0, 2),
            ("SMALLGEM", "", "GEM", 2, 6),
            ("DRINK", "", "DRINK", 4, 10),
            ("WEAPON", "ITEM_WEAPON_AXE_BATTLE", "INORGANIC:STEEL", 1, 3),
            ("WEAPON", "ITEM_WEAPON_PICK", "INORGANIC:IRON", 1, 2),
            ("ARMOR", "ITEM_ARMOR_MAIL_SHIRT", "INORGANIC:IRON", 1, 3),
            ("HELM", "ITEM_HELM_HELM", "INORGANIC:IRON", 1, 2),
            ("CLOTH", "", "THREAD", 1, 4),
            ("RING", "", "INORGANIC:GOLD", 1, 3),
            ("FIGURINE", "", "GEM", 0, 2),
        ],
    }
}

fn season_race(season: i32) -> Option<&'static str> {
    match season {
        0 => Some("ELF"),
        1 => Some("HUMAN"),
        2 => Some("DWARF"),
        _ => None,
    }
}

impl Sim {
    // ------------------------------------------------------------------ public helpers

    /// Switch caravans off (or on again). Default on.
    pub fn set_caravans(&mut self, on: bool) {
        self.trade.off = !on;
    }

    pub fn trade_report(&self) -> TradeReport {
        self.trade.rep.clone()
    }

    /// Call the broker to the depot (DF "Request trader at depot"), or release him.
    pub fn request_broker(&mut self, on: bool) {
        self.trade.broker_called = on;
    }

    /// The unit that trades for the fort (see module docs).
    pub fn broker_unit(&self) -> Option<i32> {
        for tok in ["BROKER", "EXPEDITION_LEADER", "MAYOR"] {
            if let Some(n) = self.world.nobles.iter().find(|n| n.token == tok && n.holder >= 0) {
                if let Some(&s) = self.unit_slot.get(&n.holder) {
                    if self.ai[s].kind == Kind::Worker {
                        return Some(n.holder);
                    }
                }
            }
        }
        let mut best: Option<(i32, i32)> = None;
        for ai in &self.ai {
            if ai.kind != Kind::Worker {
                continue;
            }
            let u = &self.world.units[ai.uidx];
            if !u.flags.citizen || u.profession == "CHILD" || u.profession == "BABY" || u.squad_id >= 0 {
                continue;
            }
            let k = (u.skill("APPRAISAL") * 4 + u.skill("NEGOTIATION") + u.skill("JUDGING_INTENT"), -u.id);
            if best.map(|b| k > (b.0, -b.1)).unwrap_or(true) {
                best = Some((k.0, u.id));
            }
        }
        best.map(|b| b.1)
    }

    fn depot_building(&self, id: i32) -> Option<&rf_core::Building> {
        self.world.buildings.iter().find(|b| b.id == id && b.kind == "TradeDepot")
    }

    fn first_depot(&self) -> Option<i32> {
        self.world.buildings.iter().find(|b| b.kind == "TradeDepot" && b.built).map(|b| b.id)
    }

    fn depot_tiles(&self, id: i32) -> Vec<u32> {
        let Some(b) = self.depot_building(id) else { return Vec::new() };
        let mut out = Vec::new();
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                let p = Pos::new(x, y, b.z);
                if b.contains(p) {
                    if let Some(i) = self.world.idx(p) {
                        if self.nav.walkable(i) {
                            out.push(i as u32);
                        }
                    }
                }
            }
        }
        // Center first.
        if let Some(c) = self.world.idx(b.center) {
            if let Some(k) = out.iter().position(|&t| t == c as u32) {
                out.swap(0, k);
            }
        }
        out
    }

    fn in_depot(&self, id: i32, tile: u32) -> bool {
        let p = self.world.pos_of(tile as usize);
        self.depot_building(id).map(|b| b.contains(p)).unwrap_or(false)
    }

    pub(crate) fn broker_present(&self) -> bool {
        let (Some(c), Some(b)) = (self.trade.caravan.as_ref(), self.broker_unit()) else { return false };
        self.unit_slot.get(&b).map(|&s| self.in_depot(c.depot, self.ai[s].pos)).unwrap_or(false)
    }

    /// Trade value of an item including its contents (true value, as the merchants see it).
    pub fn item_trade_value(&self, id: i32) -> i64 {
        let Some(&i) = self.item_idx.get(&id) else { return 0 };
        let it = &self.world.items[i as usize];
        let mut v = if it.value > 0 {
            it.value as i64
        } else {
            let mv = self.world.material(it.mat).map(|m| m.value).filter(|&v| v > 0).unwrap_or(1) as i64;
            base_value(&it.kind) * mv * quality_mult(it.quality) * it.stack.max(1) as i64
        };
        if let Some(c) = self.contents.get(&id) {
            for &cid in c {
                if cid != id {
                    v += self.item_trade_value(cid);
                }
            }
        }
        v
    }

    /// Value as the broker appraises it (Appraisal only affects what is shown).
    fn shown_value(&self, id: i32, true_v: i64) -> i64 {
        let skill = self.broker_unit().and_then(|b| self.world.unit(b)).map(|u| u.skill("APPRAISAL")).unwrap_or(0);
        let err = (25 - 2 * skill).max(0) as i64;
        if err == 0 {
            return true_v;
        }
        let h = (splitmix(id as u64 ^ 0xA77) % (2 * err as u64 + 1)) as i64 - err;
        (true_v * (100 + h) / 100).max(0)
    }

    // ------------------------------------------------------------------ round-4 API bodies

    pub(crate) fn trade_status_impl(&self) -> Option<crate::TradeStatus> {
        let c = self.trade.caravan.as_ref()?;
        c.unloaded?;
        let goods: Vec<i32> = c.goods.iter().copied().filter(|id| self.item_idx.get(id).map(|&i| !self.imeta[i as usize].dead).unwrap_or(false)).collect();
        Some(crate::TradeStatus {
            civ: c.civ.clone(),
            race: c.race.clone(),
            depot: c.depot,
            broker_present: self.broker_present(),
            goods,
            fort_goods: self.fort_goods_in_depot(c.depot),
            leaves_in: c.leave_at.saturating_sub(self.ticks),
        })
    }

    /// Fort items inside the depot that can be offered.
    pub fn fort_goods_in_depot(&self, depot: i32) -> Vec<i32> {
        let merchant: &[i32] = self.trade.caravan.as_ref().map(|c| c.goods.as_slice()).unwrap_or(&[]);
        self.world
            .items
            .iter()
            .enumerate()
            .filter(|(i, it)| it.building == depot && !it.flags.in_building && !self.imeta[*i].dead && it.container < 0 && !merchant.contains(&it.id))
            .map(|(_, it)| it.id)
            .collect()
    }

    pub(crate) fn trade_impl(&mut self, offer: &[i32], request: &[i32], commit: bool) -> crate::TradeOffer {
        let now = self.ticks;
        let no = |reply: &str| crate::TradeOffer { reply: reply.into(), ..Default::default() };
        let Some(c) = self.trade.caravan.as_ref() else { return no("There is nobody to trade with.") };
        if c.unloaded.is_none() || c.leaving {
            return no("The merchants are not at the depot.");
        }
        let (depot, race, goods) = (c.depot, c.race.clone(), c.goods.clone());
        if !self.broker_present() {
            self.trade.broker_called = true;
            return no("Your broker must be at the depot to trade.");
        }
        let fort = self.fort_goods_in_depot(depot);
        if offer.iter().any(|id| !fort.contains(id)) {
            return no("You can only offer goods that are in the depot.");
        }
        if request.iter().any(|id| !goods.contains(id) || !self.item_idx.contains_key(id)) {
            return no("We do not have that.");
        }
        let ov: i64 = offer.iter().map(|&id| self.item_trade_value(id)).sum();
        let rv: i64 = request.iter().map(|&id| self.item_trade_value(id)).sum();
        let shown_o: i64 = offer.iter().map(|&id| self.shown_value(id, self.item_trade_value(id))).sum();
        let shown_r: i64 = request.iter().map(|&id| self.shown_value(id, self.item_trade_value(id))).sum();
        let mut out = crate::TradeOffer { accepted: false, offer_value: shown_o.min(i32::MAX as i64) as i32, request_value: shown_r.min(i32::MAX as i64) as i32, reply: String::new() };
        // Elves: no wood (wiki "Elf": they refuse items made of wood).
        if race == "ELF" {
            let wooden = offer.iter().any(|&id| {
                self.item_idx.get(&id).map(|&i| self.world.material(self.world.items[i as usize].mat).map(|m| m.is_wood).unwrap_or(false)).unwrap_or(false)
            });
            if wooden {
                out.reply = "You would trade in the flesh of trees? We will have nothing of this!".into();
                return out;
            }
        }
        if offer.is_empty() && request.is_empty() {
            out.reply = "What do you want?".into();
            return out;
        }
        if request.is_empty() {
            out.accepted = true;
            out.reply = "We gratefully accept your gift.".into();
        } else if ov * 100 >= rv * (100 + PROFIT_PCT) {
            out.accepted = true;
            out.reply = if ov >= rv * 2 { "This is a generous offer. We accept!".into() } else { "This is a fair trade.".into() };
        } else if ov * 2 < rv {
            out.reply = "Are you mocking me?".into();
        } else {
            out.reply = "That is not enough. Perhaps you could add something.".into();
        }
        if !(commit && out.accepted) {
            return out;
        }
        // Execute: fort goods go to the merchants, bought goods become the fort's.
        let sold: Vec<(String, String)> = offer.iter().filter_map(|id| self.world.item(*id).map(|i| (i.desc.clone(), i.subtype.clone()))).collect();
        self.check_export_bans(&sold, now);
        for &id in offer {
            self.destroy_with_contents(id);
            if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == depot) {
                b.items.retain(|&x| x != id);
            }
        }
        for &id in request {
            self.release_bought(id, depot);
        }
        if let Some(c) = self.trade.caravan.as_mut() {
            c.goods.retain(|id| !request.contains(id));
            if c.extended < 8 * DAY {
                c.leave_at += 2 * DAY;
                c.extended += 2 * DAY;
            }
            let log = c.log;
            self.trade.rep.caravans[log].trades += 1;
        }
        if let Some(b) = self.broker_unit() {
            if let Some(&s) = self.unit_slot.get(&b) {
                self.add_xp(s, "APPRAISAL", 30);
                self.add_xp(s, "NEGOTIATION", 30);
            }
        }
        self.trade.rep.trades.push((now, ov, rv, out.reply.clone()));
        out
    }

    fn destroy_with_contents(&mut self, id: i32) {
        if let Some(c) = self.contents.get(&id).cloned() {
            for cid in c {
                self.destroy_item(cid);
            }
        }
        self.destroy_item(id);
    }

    fn release_bought(&mut self, id: i32, depot: i32) {
        let Some(&i) = self.item_idx.get(&id) else { return };
        let i = i as usize;
        let it = &mut self.world.items[i];
        it.flags.forbid = false;
        it.flags.in_building = false;
        it.flags.on_ground = true;
        it.building = -1;
        let pos = it.pos;
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == depot) {
            b.items.retain(|&x| x != id);
        }
        if let Some(t) = self.world.idx(pos) {
            self.pile_occ_add(t as u32, 1);
        }
        self.haul_add(id);
        self.list_consumable(id);
    }

    pub(crate) fn move_to_depot_impl(&mut self, ids: &[i32]) {
        let Some(depot) = self.trade.caravan.as_ref().map(|c| c.depot).or_else(|| self.first_depot()) else { return };
        let _ = depot;
        for &id0 in ids {
            // Items in a bin or barrel travel with their container.
            let mut id = id0;
            for _ in 0..4 {
                match self.item_idx.get(&id).map(|&i| self.world.items[i as usize].container) {
                    Some(c) if c >= 0 => id = c,
                    _ => break,
                }
            }
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let it = &self.world.items[i as usize];
            let f = &it.flags;
            if self.imeta[i as usize].dead || f.forbid || f.in_building || f.construction || it.building >= 0 || it.holder >= 0 || !f.on_ground {
                continue;
            }
            if self.trade.pending.iter().any(|m| m.item == id) {
                continue;
            }
            self.trade.pending.push(DepotMove { item: id, claimed: NONE, since: self.ticks });
            self.trade_refresh_haul_cache();
        }
    }

    // ------------------------------------------------------------------ hauling hooks

    pub(crate) fn trade_haul_decide(&mut self, s: usize, now: u64) -> bool {
        if self.trade.pending.is_empty() || self.ai[s].labors & labor::ANY_HAUL == 0 {
            return false;
        }
        // Trade-goods hauling labor if anybody has it, else any hauler.
        if self.trade.trade_labor && !self.world.units[self.ai[s].uidx].has_labor("HAUL_TRADE") {
            return false;
        }
        let dest = self.trade.dest;
        if dest == NONE {
            return false;
        }
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 || self.nav.comp_near(dest as usize) != c {
            return false;
        }
        for k in 0..self.trade.pending.len() {
            let m = &self.trade.pending[k];
            if m.claimed != NONE {
                continue;
            }
            let id = m.item;
            let Some(&ii) = self.item_idx.get(&id) else { continue };
            let ii = ii as usize;
            let im = &self.imeta[ii];
            if im.dead || im.hauling || im.reserved > 0 || im.blocked_until > now {
                continue;
            }
            let Some(t) = self.world.idx(self.world.items[ii].pos) else { continue };
            if self.nav.comp_near(t) != c {
                continue;
            }
            self.set_hauling(id, true);
            self.haul_remove(id);
            self.trade.pending[k].claimed = s as u32;
            self.ai[s].task = Task::GoPickup(id, dest);
            let g = self.item_goals(id);
            self.request_path(s, g, now);
            return true;
        }
        false
    }

    /// Depot destination tile and "does anybody have the trade-hauling labor" (cached).
    fn trade_refresh_haul_cache(&mut self) {
        let depot = self.trade.caravan.as_ref().map(|c| c.depot).or_else(|| self.first_depot());
        self.trade.dest = depot.and_then(|d| self.depot_tiles(d).first().copied()).unwrap_or(NONE);
        self.trade.trade_labor = self.world.citizens().any(|c| c.has_labor("HAUL_TRADE"));
    }

    pub(crate) fn trade_stored(&mut self, id: i32, tile: u32) {
        let Some(k) = self.trade.pending.iter().position(|m| m.item == id) else { return };
        self.trade.pending.remove(k);
        let Some(depot) = self.world.buildings.iter().find(|b| b.kind == "TradeDepot" && b.contains(self.world.pos_of(tile as usize))).map(|b| b.id) else { return };
        let Some(&ii) = self.item_idx.get(&id) else { return };
        let ii = ii as usize;
        self.haul_remove(id);
        let loose = self.world.items[ii].flags.on_ground && self.world.items[ii].container < 0;
        {
            let it = &mut self.world.items[ii];
            it.flags.on_ground = false;
            it.building = depot;
        }
        if loose {
            self.pile_occ_add(tile, -1);
        }
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == depot) {
            b.items.push(id);
        }
        self.trade.rep.items_moved_to_depot += 1;
    }

    pub(crate) fn trade_broker_decide(&mut self, s: usize, now: u64) -> bool {
        let Some(c) = self.trade.caravan.as_ref() else { return false };
        if c.leaving {
            return false;
        }
        let depot = c.depot;
        if self.broker_unit() != Some(self.ai[s].id) {
            return false;
        }
        if self.in_depot(depot, self.ai[s].pos) {
            self.ai[s].wake = now + 100;
            return true;
        }
        let tiles = self.depot_tiles(depot);
        let pos = self.ai[s].pos as usize;
        let cc = self.nav.comp_near(pos);
        let Some(&t) = tiles.iter().find(|&&t| cc != 0 && self.nav.comp_root(t as usize) == cc) else { return false };
        self.ai[s].task = Task::Wander;
        self.request_path(s, Goals::one(t), now);
        true
    }

    // ------------------------------------------------------------------ caravans

    /// Season start: schedule this season's caravan.
    pub(crate) fn trade_season(&mut self, now: u64) {
        if self.trade.off || self.trade.caravan.is_some() || !self.has_civilization() {
            return;
        }
        let season = self.world.calendar.month() / 3;
        let Some(race) = season_race(season) else { return };
        let Some(e) = self.trade_partner(race) else { return };
        let mut r = Rng::for_unit(self.seed, 4, now, 0xCA7A);
        let delay = (7 + r.below(21)) as u64 * DAY;
        self.trade.due = Some((now + delay, e));
    }

    fn trade_partner(&self, race: &str) -> Option<EntityInfo> {
        if self.world.entities.is_empty() {
            if race != "DWARF" {
                return None;
            }
            return Some(EntityInfo {
                id: self.world.meta.civ_id,
                name: "the dwarven civilization".into(),
                race: "DWARF".into(),
                kind: "Civilization".into(),
                relation: "Own".into(),
                trades: true,
            });
        }
        let own = self.world.meta.civ_id;
        self.world
            .entities
            .iter()
            .filter(|e| e.race == race && (e.trades || (race == "DWARF" && e.id == own)) && e.relation != "War" && (e.kind.is_empty() || e.kind == "Civilization"))
            .min_by_key(|e| (e.id != own, e.id))
            .cloned()
    }

    /// Next expected caravan (civ name) for the UI.
    pub(crate) fn caravan_expected(&self) -> Option<String> {
        if let Some(c) = &self.trade.caravan {
            return Some(c.civ.clone());
        }
        if let Some((_, e)) = &self.trade.due {
            return Some(e.name.clone());
        }
        let season = self.world.calendar.month() / 3;
        for k in 1..=4 {
            if let Some(race) = season_race((season + k) % 4) {
                if let Some(e) = self.trade_partner(race) {
                    return Some(e.name);
                }
            }
        }
        None
    }

    /// Debug/UI: a caravan of `race` arrives now. Returns true if it could reach the depot.
    pub fn force_caravan(&mut self, race: &str) -> bool {
        if self.trade.caravan.is_some() {
            return false;
        }
        let e = self.trade_partner(race).unwrap_or(EntityInfo {
            id: -2,
            name: format!("the {} merchants", race.to_ascii_lowercase()),
            race: race.into(),
            kind: "Civilization".into(),
            relation: "Peace".into(),
            trades: true,
        });
        self.spawn_caravan(e)
    }

    /// [hook] From the population pass (every `POP_PERIOD` ticks).
    pub(crate) fn trade_tick(&mut self, now: u64) {
        if let Some((due, _)) = &self.trade.due {
            if now >= *due {
                let (_, e) = self.trade.due.take().unwrap();
                self.spawn_caravan(e);
            }
        }
        // Haulers that gave up on a depot move; moves nobody could take for a week are dropped.
        let mut k = 0;
        while k < self.trade.pending.len() {
            let m = &self.trade.pending[k];
            if m.claimed == NONE {
                let gone = self.item_idx.get(&m.item).map(|&i| self.imeta[i as usize].dead).unwrap_or(true);
                if gone || now >= m.since + 7 * DAY {
                    self.trade.pending.remove(k);
                    continue;
                }
                k += 1;
                continue;
            }
            let s = m.claimed as usize;
            let busy = matches!(self.ai[s].task, Task::GoPickup(id, _) | Task::GoStore(id, _) if id == m.item);
            if !busy {
                self.trade.pending[k].claimed = NONE;
            }
            k += 1;
        }
        if !self.trade.pending.is_empty() && now % DAY < crate::population::POP_PERIOD {
            self.trade_refresh_haul_cache();
        }
        let Some(c) = self.trade.caravan.as_ref() else { return };
        let (leaving, warned, leave_at, unloaded, arrived) = (c.leaving, c.warned, c.leave_at, c.unloaded, c.arrived);
        if !leaving {
            if unloaded.is_none() && now >= arrived + MONTH {
                self.caravan_leave(now);
            } else if !warned && unloaded.is_some() && now + 3 * DAY >= leave_at {
                let civ = c.civ.clone();
                self.trade.caravan.as_mut().unwrap().warned = true;
                self.announce(now, format!("The merchants from {civ} will be leaving soon."), None, AnnouncementKind::Info);
            } else if unloaded.is_some() && now >= leave_at {
                self.caravan_leave(now);
            }
        } else {
            let all_gone = c.members.iter().all(|m| self.ai[m.slot as usize].kind == Kind::Dead);
            if all_gone || now >= leave_at + MONTH {
                self.caravan_finish(now);
            }
        }
    }

    /// Is there a path at least 3 tiles wide from the depot to a map edge (wagon access)?
    pub fn wagon_access(&self, depot: i32) -> bool {
        let start = self.depot_tiles(depot);
        if start.is_empty() {
            return false;
        }
        let (dx, dy) = (self.nav.dx, self.nav.dy);
        let wide = |i: usize| -> bool {
            let (x, y, z) = self.nav.xyz(i);
            for oy in -1i32..=1 {
                for ox in -1i32..=1 {
                    let (nx, ny) = (x as i32 + ox, y as i32 + oy);
                    if nx < 0 || ny < 0 || nx >= dx as i32 || ny >= dy as i32 {
                        continue;
                    }
                    let q = self.nav.idx(nx as usize, ny as usize, z);
                    if !self.nav.walkable(q) {
                        return false;
                    }
                }
            }
            true
        };
        let mut seen = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        for &t in &start {
            seen.insert(t);
            queue.push_back(t);
        }
        let mut buf = [0u32; crate::nav::MAX_NEIGHBORS];
        while let Some(t) = queue.pop_front() {
            let (x, y, z) = self.nav.xyz(t as usize);
            if x == 0 || y == 0 || x == dx - 1 || y == dy - 1 {
                return true;
            }
            if seen.len() > 400_000 {
                return false;
            }
            let n = self.nav.neighbors(t as usize, &mut buf);
            for &q in &buf[..n] {
                let (_, _, qz) = self.nav.xyz(q as usize);
                if qz != z {
                    // Wagons take ramps, not stairs.
                    let a = self.world.shape(self.world.pos_of(t as usize));
                    let b = self.world.shape(self.world.pos_of(q as usize));
                    let ramp = |s: Shape| matches!(s, Shape::Ramp | Shape::RampTop);
                    if !ramp(a) && !ramp(b) {
                        continue;
                    }
                }
                if seen.contains(&q) || !wide(q as usize) {
                    continue;
                }
                seen.insert(q);
                queue.push_back(q);
            }
        }
        false
    }

    fn mat_for(&self, sel: &str, r: &mut Rng) -> Option<u16> {
        let m = &self.world.materials;
        let pick = |v: Vec<usize>, r: &mut Rng| if v.is_empty() { None } else { Some(v[r.below(v.len() as u32) as usize] as u16) };
        let found: Vec<usize> = match sel {
            "GEM" => (0..m.len()).filter(|&i| m[i].is_gem).collect(),
            "WOOD" => (0..m.len()).filter(|&i| m[i].is_wood).collect(),
            "DRINK" => (0..m.len()).filter(|&i| m[i].token.ends_with(":DRINK")).collect(),
            "THREAD" => (0..m.len()).filter(|&i| m[i].token.starts_with("PLANT:") && m[i].token.ends_with(":THREAD")).collect(),
            "PLANT" => (0..m.len()).filter(|&i| m[i].token.starts_with("PLANT:") && m[i].token.ends_with(":STRUCTURAL") && !m[i].is_wood).collect(),
            "SEED" => (0..m.len()).filter(|&i| m[i].token.ends_with(":SEED")).collect(),
            "LEATHER" => (0..m.len()).filter(|&i| m[i].token.ends_with(":LEATHER")).collect(),
            "MUSCLE" => (0..m.len()).filter(|&i| m[i].token.starts_with("CREATURE:") && m[i].token.ends_with(":MUSCLE")).collect(),
            tok => (0..m.len()).filter(|&i| m[i].token == tok).collect(),
        };
        let _ = &found;
        pick(found, r)
    }

    fn spawn_caravan(&mut self, e: EntityInfo) -> bool {
        let now = self.ticks;
        let race = if e.race.is_empty() { "DWARF".to_string() } else { e.race.clone() };
        let civ = if e.name.is_empty() { format!("the {} civilization", race.to_ascii_lowercase()) } else { e.name.clone() };
        let mut log = CaravanLog { civ: civ.clone(), race: race.clone(), arrived: now, ..Default::default() };
        let Some(depot) = self.first_depot() else {
            log.note = "no trade depot".into();
            log.left = Some(now);
            self.trade.rep.caravans.push(log);
            self.announce(now, format!("Merchants from {civ} have arrived, but there is no trade depot."), None, AnnouncementKind::Warning);
            return false;
        };
        let tiles = self.depot_tiles(depot);
        let comp = tiles.first().map(|&t| self.nav.comp_root(t as usize)).unwrap_or(0);
        let edges = if comp != 0 { self.society_edge_tiles(comp) } else { Vec::new() };
        if edges.is_empty() {
            log.note = "depot not reachable from the map edge".into();
            log.left = Some(now);
            self.trade.rep.caravans.push(log);
            self.announce(now, format!("The merchants from {civ} cannot reach the trade depot."), None, AnnouncementKind::Warning);
            return false;
        }
        self.combat_sync_units();
        let mut r = Rng::for_unit(self.seed, 5, now, 0xCA7B);
        let edge = edges[r.below(edges.len() as u32) as usize];
        let sp = self.nav.xyz(edge as usize);
        let mut near = edges.clone();
        near.sort_by_key(|&t| (crate::manhattan(sp, self.nav.xyz(t as usize)), t));
        let wagons = if race == "ELF" || !self.wagon_access(depot) { 0 } else { 1 + r.below(3) };
        let merchants = 2 + r.below(3) as usize;
        let animals = if wagons > 0 { 2 * wagons as usize } else { 3 + r.below(4) as usize };
        let pack: &[&str] = match race.as_str() {
            "ELF" => &["HORSE", "DONKEY"],
            "HUMAN" => &["HORSE", "MULE", "DONKEY"],
            _ => &["MULE", "DONKEY", "HORSE"],
        };
        let mut next_id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let first_slot = self.world.units.len();
        for k in 0..merchants + animals {
            let tile = near[k % near.len()];
            let p = self.world.pos_of(tile as usize);
            let animal = k >= merchants;
            let ur = if animal { pack[r.below(pack.len() as u32) as usize].to_string() } else { race.clone() };
            let raw = self.world.creature_raws.iter().find(|c| c.token == ur);
            let caste = raw.and_then(|c| c.castes.first());
            let sex = r.below(2) as i8;
            let unit = Unit {
                id: next_id,
                race: ur.clone(),
                caste: if sex == 0 { "FEMALE".into() } else { "MALE".into() },
                name: String::new(),
                profession: if animal { "STANDARD".into() } else { "MERCHANT".into() },
                custom_profession: if animal { String::new() } else { "merchant".into() },
                pos: p,
                flags: UnitFlags { animal, tame: animal, merchant: true, visitor: !animal, ..Default::default() },
                civ_id: e.id,
                hist_fig_id: -1,
                sex,
                age: 20.0 + r.below(60) as f32,
                squad_id: -1,
                squad_position: -1,
                body_size: caste.map(|c| c.body_size).filter(|&b| b > 0).unwrap_or(if animal { 300_000 } else { 60_000 }),
                walk_speed: caste.map(|c| c.walk_speed).filter(|&w| w > 0).unwrap_or(900),
                invader_civ: -1,
                ..Default::default()
            };
            self.world.units.push(unit);
            next_id += 1;
        }
        self.init_units();
        self.combat_sync_units();
        let mut members = Vec::new();
        for (k, uidx) in (first_slot..self.world.units.len()).enumerate() {
            let id = self.world.units[uidx].id;
            let s = self.unit_slot[&id];
            let dest = tiles[k % tiles.len()];
            members.push(Member { slot: s as u32, dest, tries: 0, arrived: false });
            match self.trade.owned.binary_search(&(s as u32)) {
                Ok(_) => {}
                Err(p) => self.trade.owned.insert(p, s as u32),
            }
            self.ai[s].wake = now + 1 + k as u64;
        }
        log.merchants = merchants;
        log.animals = animals;
        log.wagons = wagons;
        if wagons == 0 && race != "ELF" {
            log.note = "no wagon access (3-wide path) to the depot: pack animals only".into();
        }
        let li = self.trade.rep.caravans.len();
        self.trade.rep.caravans.push(log);
        self.trade.caravan = Some(Caravan {
            civ_id: e.id,
            civ: civ.clone(),
            race,
            depot,
            edge,
            members,
            arrived: now,
            unloaded: None,
            leave_at: u64::MAX,
            warned: false,
            leaving: false,
            goods: Vec::new(),
            wagons,
            extended: 0,
            log: li,
        });
        let p = self.world.pos_of(edge as usize);
        self.announce(now, format!("A caravan from {civ} has arrived."), Some(p), AnnouncementKind::Info);
        true
    }

    pub(crate) fn trade_owns(&self, s: usize) -> bool {
        !self.trade.owned.is_empty() && self.trade.owned.binary_search(&(s as u32)).is_ok()
    }

    /// [hook] Merchants and their animals: walk to the depot, wait, walk back off the map.
    pub(crate) fn trade_think(&mut self, s: usize, now: u64) {
        let Some(c) = self.trade.caravan.as_ref() else {
            self.trade_unit_leave(s);
            return;
        };
        let Some(mi) = c.members.iter().position(|m| m.slot == s as u32) else {
            self.trade_unit_leave(s);
            return;
        };
        let (leaving, edge, depot) = (c.leaving, c.edge, c.depot);
        let dest = if leaving { edge } else { c.members[mi].dest };
        if self.ai[s].waiting {
            return;
        }
        let ai = &self.ai[s];
        if ai.path_i < ai.path.len() && ai.path.last() == Some(&dest) {
            self.step(s, now);
            return;
        }
        let here = self.ai[s].pos;
        let p = self.world.pos_of(here as usize);
        let at_edge = p.x == 0 || p.y == 0 || p.x as usize == self.nav.dx - 1 || p.y as usize == self.nav.dy - 1;
        if leaving {
            if here == edge || at_edge {
                self.trade_unit_leave(s);
                return;
            }
        } else if here == dest || self.in_depot(depot, here) {
            let first = !self.trade.caravan.as_ref().unwrap().members[mi].arrived;
            self.trade.caravan.as_mut().unwrap().members[mi].arrived = true;
            if first && self.trade.caravan.as_ref().unwrap().unloaded.is_none() {
                self.unload_goods(now);
            }
            let mut r = Rng::for_unit(self.seed, s, now, 0x7AD);
            self.ai[s].path.clear();
            self.ai[s].path_i = 0;
            self.ai[s].wake = now + 200 + r.below(200) as u64;
            return;
        }
        let tries = {
            let m = &mut self.trade.caravan.as_mut().unwrap().members[mi];
            m.tries = m.tries.saturating_add(1);
            m.tries
        };
        if tries > 5 {
            // Could not path (blocked way): estimate, the merchant gets there anyway.
            if leaving {
                self.trade_unit_leave(s);
            } else {
                let dp = self.world.pos_of(dest as usize);
                self.ai[s].pos = dest;
                let uidx = self.ai[s].uidx;
                self.world.units[uidx].pos = dp;
                self.ai[s].wake = now + 1;
                self.trade.caravan.as_mut().unwrap().members[mi].tries = 0;
            }
            return;
        }
        self.request_path(s, Goals::one(dest), now);
        if !self.ai[s].waiting && self.ai[s].path.is_empty() {
            self.ai[s].wake = now + 10;
        }
    }

    fn unload_goods(&mut self, now: u64) {
        let (race, depot, civ, wagons) = {
            let c = self.trade.caravan.as_ref().unwrap();
            (c.race.clone(), c.depot, c.civ.clone(), c.wagons)
        };
        let tiles = self.depot_tiles(depot);
        let Some(&center) = tiles.first() else { return };
        let pos = self.world.pos_of(center as usize);
        let mut r = Rng::for_unit(self.seed, 6, now, 0x600D);
        let mut goods = Vec::new();
        let mut value = 0i64;
        let scale = if wagons > 0 { 1 + wagons } else { 1 };
        for &(kind, sub, sel, lo, hi) in goods_list(&race) {
            let n = (lo + r.below(hi - lo + 1)) * scale;
            for _ in 0..n {
                let Some(mat) = self.mat_for(sel, &mut r) else { break };
                let q = match r.below(100) {
                    0..=59 => 0,
                    60..=79 => 1,
                    80..=89 => 2,
                    90..=95 => 3,
                    96..=98 => 4,
                    _ => 5,
                };
                let stack = if matches!(kind, "DRINK" | "MEAT" | "PLANT" | "SEEDS") { 5 + r.below(11) as i32 } else { 1 };
                let mv = self.world.material(mat).map(|m| m.value).filter(|&v| v > 0).unwrap_or(1) as i64;
                let v = base_value(kind) * mv * quality_mult(q) * stack as i64;
                let mname = self.world.material(mat).map(|m| m.name.clone()).unwrap_or_default();
                let noun = if sub.is_empty() { kind.to_ascii_lowercase().replace('_', " ") } else { sub.trim_start_matches("ITEM_").splitn(2, '_').nth(1).unwrap_or(sub).to_ascii_lowercase().replace('_', " ") };
                let id = self.next_item_id;
                self.next_item_id += 1;
                self.push_item(Item {
                    id,
                    kind: kind.into(),
                    subtype: sub.into(),
                    mat,
                    pos,
                    stack,
                    quality: q,
                    container: -1,
                    holder: -1,
                    building: depot,
                    desc: format!("{mname} {noun}"),
                    value: v.min(i32::MAX as i64) as i32,
                    flags: ItemFlags { forbid: true, ..Default::default() },
                    ..Default::default()
                });
                if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == depot) {
                    b.items.push(id);
                }
                goods.push(id);
                value += v;
            }
        }
        let c = self.trade.caravan.as_mut().unwrap();
        c.unloaded = Some(now);
        c.leave_at = now + STAY_TICKS;
        let li = c.log;
        let n = goods.len();
        c.goods = goods;
        let l = &mut self.trade.rep.caravans[li];
        l.unloaded = Some(now);
        l.goods = n;
        l.goods_value = value;
        let _ = civ;
        self.announce(now, "The merchants have arrived and are unloading their goods.".into(), Some(pos), AnnouncementKind::Info);
    }

    fn caravan_leave(&mut self, now: u64) {
        let Some(c) = self.trade.caravan.as_mut() else { return };
        c.leaving = true;
        c.leave_at = now;
        let civ = c.civ.clone();
        let goods = std::mem::take(&mut c.goods);
        let depot = c.depot;
        let slots: Vec<u32> = c.members.iter().map(|m| m.slot).collect();
        for id in goods {
            self.destroy_item(id);
            if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == depot) {
                b.items.retain(|&x| x != id);
            }
        }
        for s in slots {
            let s = s as usize;
            if self.ai[s].kind != Kind::Dead {
                self.ai[s].path.clear();
                self.ai[s].path_i = 0;
                self.ai[s].wake = now + 1;
            }
        }
        if let Some(c) = self.trade.caravan.as_mut() {
            for m in &mut c.members {
                m.tries = 0;
            }
        }
        self.trade.broker_called = false;
        self.announce(now, format!("The merchants from {civ} have embarked on their journey."), None, AnnouncementKind::Info);
    }

    fn caravan_finish(&mut self, now: u64) {
        let Some(c) = self.trade.caravan.take() else { return };
        for m in &c.members {
            let s = m.slot as usize;
            if self.ai[s].kind != Kind::Dead {
                self.trade_unit_leave(s);
            }
        }
        self.trade.owned.retain(|s| c.members.iter().all(|m| m.slot != *s));
        self.trade.rep.caravans[c.log].left = Some(now);
    }

    fn trade_unit_leave(&mut self, s: usize) {
        let uidx = self.ai[s].uidx;
        let inv = std::mem::take(&mut self.world.units[uidx].inventory);
        for id in inv {
            self.destroy_item(id);
        }
        self.world.units[uidx].flags.inactive = true;
        self.cancel_task(s);
        self.ai[s].kind = Kind::Dead;
        self.ai[s].wake = u64::MAX;
        if let Ok(p) = self.trade.owned.binary_search(&(s as u32)) {
            self.trade.owned.remove(p);
        }
    }

    pub(crate) fn society_hash(&self, mix: &mut dyn FnMut(u64)) {
        mix(self.pop.preg.len() as u64);
        mix(self.pop.dead.len() as u64);
        mix(self.pop.bury.len() as u64);
        mix(self.pop.rep.waves.len() as u64 ^ (self.pop.rep.births.len() as u64) << 8);
        mix(self.trade.pending.len() as u64);
        if let Some(c) = &self.trade.caravan {
            mix(c.arrived ^ (c.civ_id as u64) << 48);
            mix(c.goods.len() as u64);
            mix(c.leave_at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{Building, World};

    const SZ: i16 = 13; // synthetic surface z for dz = 20

    fn world_with_depot() -> World {
        let mut w = World::synthetic(48, 48, 20);
        w.meta.source = "export:test".into();
        let c = Pos::new(32, 32, SZ);
        w.buildings.push(Building { id: 900, kind: "TradeDepot".into(), x1: 30, y1: 30, x2: 34, y2: 34, z: SZ, center: c, built: true, owner: -1, ..Default::default() });
        w.units[0].skills.push(rf_core::Skill { id: "APPRAISAL".into(), rating: 5, xp: 0 });
        for it in &mut w.items {
            if it.kind == "WOOD" {
                it.value = 500;
            }
        }
        w
    }

    fn run_until(sim: &mut Sim, max: u32, f: impl Fn(&Sim) -> bool) -> bool {
        for _ in 0..max / 10 {
            sim.run_ticks(10);
            if f(sim) {
                return true;
            }
        }
        false
    }

    #[test]
    fn caravan_arrives_unloads_trades_and_leaves() {
        let mut sim = Sim::new(world_with_depot(), 31);
        assert!(sim.wagon_access(900));
        assert!(sim.force_caravan("DWARF"));
        assert!(sim.announcements.iter().any(|a| a.text.starts_with("A caravan from ")));
        assert!(run_until(&mut sim, 6000, |s| s.trade_status().is_some()), "merchants reach the depot");
        let st = sim.trade_status().unwrap();
        assert_eq!(st.depot, 900);
        assert!(!st.goods.is_empty());
        assert!(st.leaves_in > 10 * DAY);
        let log = sim.trade_report().caravans[0].clone();
        assert!(log.wagons >= 1 && log.merchants >= 2);
        // Bring a log to the depot.
        let wood = sim.world.items.iter().find(|i| i.kind == "WOOD").unwrap().id;
        sim.move_to_depot(&[wood]);
        assert!(run_until(&mut sim, 6000, |s| s.trade_status().unwrap().fort_goods.contains(&wood)), "log hauled to the depot");
        // No broker yet: he is called.
        let drink = *st.goods.iter().find(|&&id| sim.world.item(id).map(|i| i.kind == "DRINK").unwrap_or(false)).expect("dwarves bring drink");
        let o = sim.trade(&[wood], &[drink], false);
        if !sim.broker_present() {
            assert!(!o.accepted);
            assert!(o.reply.contains("broker"));
            assert!(run_until(&mut sim, 6000, |s| s.broker_present()), "broker walks to the depot");
        }
        let mock = sim.trade(&[], &st.goods, false);
        assert!(!mock.accepted);
        let o = sim.trade(&[wood], &[drink], true);
        assert!(o.accepted, "{o:?}");
        assert!(o.offer_value > 0 && o.request_value > 0);
        let d = sim.world.item(drink).unwrap();
        assert!(!d.flags.forbid && d.building < 0, "bought drink belongs to the fort");
        assert!(sim.item_idx.get(&wood).map(|&i| sim.imeta[i as usize].dead).unwrap_or(true), "log went to the merchants");
        assert_eq!(sim.trade_report().trades.len(), 1);
        // They leave after their stay.
        sim.run_ticks(30 * DAY as u32);
        let log = sim.trade_report().caravans[0].clone();
        assert!(log.left.is_some(), "{log:?}");
        assert!(sim.trade_status().is_none());
        assert!(sim.world.units.iter().filter(|u| u.flags.merchant).all(|u| u.flags.inactive));
        assert!(sim.announcements.iter().any(|a| a.text.contains("embarked on their journey")));
    }

    #[test]
    fn elves_refuse_wood() {
        let mut sim = Sim::new(world_with_depot(), 32);
        assert!(sim.force_caravan("ELF"));
        assert_eq!(sim.trade_report().caravans[0].wagons, 0, "elves never bring wagons");
        assert!(run_until(&mut sim, 6000, |s| s.trade_status().is_some()));
        let wood = sim.world.items.iter().find(|i| i.kind == "WOOD").unwrap().id;
        sim.move_to_depot(&[wood]);
        sim.request_broker(true);
        assert!(run_until(&mut sim, 8000, |s| s.broker_present() && s.trade_status().unwrap().fort_goods.contains(&wood)));
        let goods = sim.trade_status().unwrap().goods;
        let o = sim.trade(&[wood], &goods[..1], true);
        assert!(!o.accepted);
        assert!(o.reply.contains("trees"));
    }

    #[test]
    fn narrow_path_means_no_wagons() {
        let mut w = world_with_depot();
        let wall = w.tiletypes.by_name("StoneWall").unwrap();
        for k in 27..=37 {
            for (x, y) in [(k, 27), (k, 37), (27, k), (37, k)] {
                if (x, y) == (32, 27) {
                    continue; // one-tile gap
                }
                let t = w.tile_mut(Pos::new(x, y, SZ)).unwrap();
                t.tt = wall;
                t.mat = 0;
            }
        }
        // Walls above so nobody climbs over (the level above is open air).
        let mut sim = Sim::new(w, 33);
        assert!(!sim.wagon_access(900));
        assert!(sim.force_caravan("DWARF"));
        let log = sim.trade_report().caravans[0].clone();
        assert_eq!(log.wagons, 0);
        assert!(log.animals >= 3, "pack animals instead: {log:?}");
        assert!(run_until(&mut sim, 8000, |s| s.trade_status().is_some()), "merchants still walk in");
    }

    #[test]
    fn seasonal_caravans_by_race() {
        let mut sim = Sim::new(world_with_depot(), 34);
        // Without exported entities only the fort's own dwarven civilization trades (autumn).
        sim.world.calendar.year_tick = 6 * TICKS_PER_MONTH;
        sim.trade_season(100);
        assert_eq!(sim.trade.due.as_ref().map(|d| d.1.race.clone()), Some("DWARF".into()));
        sim.trade.due = None;
        sim.world.calendar.year_tick = 0;
        sim.trade_season(200);
        assert!(sim.trade.due.is_none(), "no elven civ known");
        sim.world.entities.push(EntityInfo { id: 77, name: "the Forest of Leaves".into(), race: "ELF".into(), kind: "Civilization".into(), relation: "Peace".into(), trades: true });
        sim.world.entities.push(EntityInfo { id: 78, name: "the Hated Ones".into(), race: "HUMAN".into(), kind: "Civilization".into(), relation: "War".into(), trades: true });
        sim.trade_season(300);
        assert_eq!(sim.trade.due.as_ref().map(|d| d.1.name.clone()), Some("the Forest of Leaves".into()));
        sim.trade.due = None;
        sim.world.calendar.year_tick = 3 * TICKS_PER_MONTH;
        sim.trade_season(400);
        assert!(sim.trade.due.is_none(), "humans at war send no caravan");
        assert!(sim.population_info().caravan_expected.is_some());
    }

    #[test]
    fn item_value_formula() {
        let mut sim = Sim::new(world_with_depot(), 35);
        let t = sim.world.idx(sim.world.units[0].pos).unwrap();
        let id = sim.create_item("BAR", 0, t, "granite bar".into());
        let i = sim.item_idx[&id] as usize;
        sim.world.materials[0].value = 3;
        sim.world.items[i].quality = 5;
        assert_eq!(sim.item_trade_value(id), base_value("BAR") * 3 * 12);
        sim.world.items[i].value = 77;
        assert_eq!(sim.item_trade_value(id), 77, "exported DF value wins");
        assert_eq!(quality_mult(6), 120);
    }
}
