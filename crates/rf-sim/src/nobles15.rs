//! [nobles15] Nobles: room requirements by position, noble room thoughts, mandates (production
//! orders and export bans with deadlines and punishments), demands, meetings and complaints,
//! tombs, sleep pretension, elections thoughts, sheriff -> captain of the guard, manager
//! validation of work orders, bookkeeper precision and broker appraisal.
//!
//! Spec: docs/mechanics/nobles15.md; rules and their classes: docs/FIDELITY.md section
//! "[nobles15]". Sources: w:Noble (V50), w:Mandate, w:Demand, w:Bookkeeper, vanilla raws
//! `entity_default.txt` (numbers only) and the reference exports.
//!
//! Hooks (every one tagged `// [nobles15]` at its call site):
//! - `nobles15_pass` from `tick_inner` every `N15_PERIOD` ticks (phase `N15_PHASE`);
//! - `nobles15_item_made` from `production::push_item` (mandate fulfilment);
//! - `nobles15_items_traded` from `trade` (export bans), `nobles15_depot_hauled` (who hauled);
//! - `nobles15_sentence` / `nobles15_exempt` from `justice::convict_impl`;
//! - `nobles15_room_feel` from the social bedroom/dining thoughts;
//! - `nobles15_elected` / `nobles15_drop_holder` from `population::elect`.

use crate::rng::Rng;
use crate::social::Class;
use crate::{AnnouncementKind, Kind, Sim};
use rf_core::{Building, NoblePosition, Pos, TICKS_PER_DAY, TICKS_PER_MONTH, TICKS_PER_YEAR};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

pub(crate) const N15_PERIOD: u64 = 100;
pub(crate) const N15_PHASE: u64 = 23;
const DAY: u64 = TICKS_PER_DAY as u64;
const MONTH: u64 = TICKS_PER_MONTH as u64;
const YEAR: u64 = TICKS_PER_YEAR as u64;

/// Difficulty defaults (`settings.json` other `MANDATE_PERIOD=1000`, `DEMAND_PERIOD=500`).
pub const MANDATE_PERIOD_DEFAULT: u32 = 1000;
pub const DEMAND_PERIOD_DEFAULT: u32 = 500;
/// ESTIMATE: no new mandate or demand of the same noble within a month of the last one.
pub const MANDATE_COOLDOWN: u64 = MONTH;
/// OK (w:Mandate "about half a year"): export bans run out after six months.
pub const BAN_DURATION: u64 = 6 * MONTH;
/// OK (w:Demand): about half a year to fulfil, forgotten after about three years.
pub const DEMAND_DUE: u64 = 6 * MONTH;
pub const DEMAND_FORGET: u64 = 3 * YEAR;
/// ESTIMATE: an overdue demand angers the noble once a week.
pub const DEMAND_ANGER_EVERY: u64 = 7 * DAY;
/// ESTIMATE: a complaint per day comes with chance citizens / COMPLAINT_BASE (~2 a year at 150).
pub const COMPLAINT_BASE: u32 = 50_000;
/// ESTIMATE: above this many citizens the manager validates work orders in his office.
pub const MANAGER_OFFICE_POP: usize = 20;
/// ESTIMATE: ticks of office work per validated order, divided by 1 + ORGANIZATION / 5.
pub const VALIDATE_TICKS: u64 = 200;
/// OBSERVED: DF 53.16 gave the unmet reference mayor no `More*` furniture thoughts in a year.
pub const MORE_FURNITURE_THOUGHTS: bool = false;
/// ESTIMATE: culprit weight of a citizen with the mandated item's skill (others weigh 1).
pub const SKILLED_WEIGHT: u32 = 4;
/// ESTIMATE: punishment not started this long after the sentence -> `DelayedPunishment`.
pub const DELAY_THOUGHT_AFTER: u64 = 7 * DAY;
/// ESTIMATE: a sleep-pretension noble resents a better commoner bedroom at most weekly.
pub const PRETENSION_EVERY: u64 = 7 * DAY;
/// ESTIMATE: the bookkeeper refreshes his records weekly once the level is reached.
pub const RECORDS_EVERY: u64 = 7 * DAY;

/// Bedroom thought shift per holder unit (H1) from the positions alone (for the social
/// module's calibration of exported room thoughts, before the holder cache exists).
pub(crate) fn bedroom_shifts(nobles: &[NoblePosition]) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = Vec::new();
    for n in nobles.iter().filter(|n| n.holder >= 0) {
        let b = pos_req(n).bedroom;
        let sh = if b > 0 { (tier_of(b) - 1).max(0) } else { 0 };
        match v.iter_mut().find(|x| x.0 == n.holder) {
            Some(x) => x.1 = x.1.max(sh),
            None => v.push((n.holder, sh)),
        }
    }
    v
}

/// Position holders on the map (`present`) with their merged numbers and the index of their
/// highest-ranked position, in position order.
pub(crate) fn holders_of(nobles: &[NoblePosition], present: impl Fn(i32) -> bool) -> Vec<(i32, PosReq, usize)> {
    let mut out: Vec<(i32, PosReq, usize)> = Vec::new();
    for (ni, n) in nobles.iter().enumerate() {
        if n.holder < 0 || !present(n.holder) || n.flags.iter().any(|f| f == "HAS_BEEN_REPLACED") {
            continue;
        }
        let req = pos_req(n);
        match out.iter_mut().find(|x| x.0 == n.holder) {
            Some(x) => {
                if req.precedence > 0 && (x.1.precedence == 0 || req.precedence < x.1.precedence) {
                    x.2 = ni;
                }
                x.1.merge(&req);
            }
            None => out.push((n.holder, req, ni)),
        }
    }
    out
}

/// Does an export ban cover this item (by type/subtype; old texts by word)?
pub(crate) fn ban_hits(m: &MandateRt, it: &rf_core::Item) -> bool {
    if !m.export_ban {
        return false;
    }
    if !m.item_type.is_empty() {
        return m.item_type == it.kind && (m.item_subtype.is_empty() || m.item_subtype == it.subtype);
    }
    !m.word.is_empty() && (it.desc.to_ascii_lowercase().contains(&m.word) || it.subtype.to_ascii_lowercase().contains(&m.word))
}

/// Room tier thresholds (w:Room): 0 meager .. 7 royal.
const TIERS: [i32; 7] = [100, 250, 500, 1000, 1500, 2500, 10_000];

/// Tier index of a room value (0 meager .. 7 royal).
pub fn tier_of(value: i32) -> i32 {
    TIERS.iter().filter(|&&t| value >= t).count() as i32
}

/// Requirements, limits and flags of a position (raws `ENTITY:MOUNTAIN`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PosReq {
    pub office: i32,
    pub bedroom: i32,
    pub dining: i32,
    pub tomb: i32,
    pub boxes: i32,
    pub cabinets: i32,
    pub racks: i32,
    pub stands: i32,
    pub demand_max: i32,
    pub mandate_max: i32,
    pub precedence: i32,
    pub sleep_pretension: bool,
    pub punishment_exempt: bool,
    pub meets_workers: bool,
}

impl PosReq {
    /// Unit holding several positions: the maximum of each field (ESTIMATE); precedence = best.
    fn merge(&mut self, o: &PosReq) {
        self.office = self.office.max(o.office);
        self.bedroom = self.bedroom.max(o.bedroom);
        self.dining = self.dining.max(o.dining);
        self.tomb = self.tomb.max(o.tomb);
        self.boxes = self.boxes.max(o.boxes);
        self.cabinets = self.cabinets.max(o.cabinets);
        self.racks = self.racks.max(o.racks);
        self.stands = self.stands.max(o.stands);
        self.demand_max = self.demand_max.max(o.demand_max);
        self.mandate_max = self.mandate_max.max(o.mandate_max);
        self.precedence = if self.precedence == 0 { o.precedence } else if o.precedence == 0 { self.precedence } else { self.precedence.min(o.precedence) };
        self.sleep_pretension |= o.sleep_pretension;
        self.punishment_exempt |= o.punishment_exempt;
        self.meets_workers |= o.meets_workers;
    }
}

const SP: u8 = 1; // SLEEP_PRETENSION
const PE: u8 = 2; // PUNISHMENT_EXEMPTION
const MW: u8 = 4; // responsibility MEET_WORKERS

/// Vanilla dwarf positions: token, [office, bedroom, dining, tomb, boxes, cabinets, racks,
/// stands, DEMAND_MAX, MANDATE_MAX], PRECEDENCE, flags (numbers from the raws; no DF text).
const VANILLA: &[(&str, [i32; 10], i32, u8)] = &[
    ("MONARCH", [10000, 10000, 10000, 10000, 10, 5, 5, 5, 10, 5], 1, SP | PE),
    ("DUKE", [2500, 2500, 2500, 2500, 5, 3, 3, 3, 5, 3], 20, SP | PE),
    ("COUNT", [1500, 1500, 1500, 1500, 3, 2, 2, 2, 3, 2], 30, SP | PE),
    ("BARON", [500, 500, 500, 500, 2, 1, 1, 1, 2, 1], 40, SP | PE),
    ("GENERAL", [500, 250, 250, 1, 2, 1, 3, 3, 2, 0], 50, SP | PE),
    ("MAYOR", [500, 500, 500, 0, 2, 1, 1, 1, 2, 1], 60, MW),
    ("CHAMPION", [0, 0, 0, 0, 0, 0, 0, 0, 1, 0], 65, PE),
    ("DIPLOMAT", [1500, 1500, 1500, 0, 3, 2, 2, 2, 3, 0], 70, SP | PE),
    ("OUTPOST_LIAISON", [1500, 1500, 1500, 0, 3, 2, 2, 2, 3, 0], 80, SP | PE),
    ("LIEUTENANT", [100, 100, 100, 0, 1, 1, 2, 2, 1, 0], 100, 0),
    ("CAPTAIN_OF_THE_GUARD", [250, 250, 250, 0, 1, 1, 1, 1, 0, 0], 105, 0),
    ("DUNGEON_MASTER", [250, 250, 250, 0, 1, 1, 1, 1, 0, 0], 108, 0),
    ("EXPEDITION_LEADER", [0; 10], 110, MW),
    ("MILITIA_COMMANDER", [0; 10], 120, 0),
    ("SHERIFF", [100, 100, 100, 0, 1, 1, 1, 1, 0, 0], 130, 0),
    ("HAMMERER", [0; 10], 150, 0),
    ("MANAGER", [1, 0, 0, 0, 0, 0, 0, 0, 0, 0], 160, 0),
    ("CHIEF_MEDICAL_DWARF", [0; 10], 165, 0),
    ("BROKER", [0; 10], 170, 0),
    ("BOOKKEEPER", [1, 0, 0, 0, 0, 0, 0, 0, 0, 0], 180, 0),
    ("CAPTAIN", [1, 1, 1, 0, 1, 1, 1, 1, 0, 0], 190, 0),
    ("MILITIA_CAPTAIN", [0; 10], 200, 0),
    ("MESSENGER", [0; 10], 250, 0),
];

/// Gendered titles map to the raws token.
pub fn canon_token(t: &str) -> &str {
    match t {
        "BARONESS" => "BARON",
        "COUNTESS" => "COUNT",
        "DUCHESS" => "DUKE",
        "KING" | "QUEEN" | "EMPEROR" | "EMPRESS" => "MONARCH",
        "BARONESS_CONSORT" | "BARON_CONSORT" => "BARON",
        t => t,
    }
}

/// Numbers of a position: the export's if it has any, else the vanilla table by token.
pub fn pos_req(n: &NoblePosition) -> PosReq {
    let tok = canon_token(&n.token);
    let table = VANILLA.iter().find(|v| v.0 == tok);
    let exported = n.mandate_max != 0
        || n.demand_max != 0
        || n.required_office != 0
        || n.required_bedroom != 0
        || n.required_dining != 0
        || n.required_tomb != 0
        || n.required_boxes != 0
        || n.required_cabinets != 0
        || n.required_racks != 0
        || n.required_stands != 0
        || !n.flags.is_empty();
    let has = |f: &str| n.flags.iter().any(|x| x == f);
    let tflags = table.map(|v| v.3).unwrap_or(0);
    if exported {
        PosReq {
            office: n.required_office,
            bedroom: n.required_bedroom,
            dining: n.required_dining,
            tomb: n.required_tomb,
            boxes: n.required_boxes,
            cabinets: n.required_cabinets,
            racks: n.required_racks,
            stands: n.required_stands,
            demand_max: n.demand_max,
            mandate_max: n.mandate_max,
            precedence: if n.precedence > 0 { n.precedence } else { table.map(|v| v.2).unwrap_or(0) },
            // The export's flags are assignment flags (no SLEEP_PRETENSION etc.): add the raws'.
            sleep_pretension: has("SLEEP_PRETENSION") || tflags & SP != 0,
            punishment_exempt: has("PUNISHMENT_EXEMPTION") || tflags & PE != 0,
            meets_workers: n.responsibilities.iter().any(|r| r == "MEET_WORKERS") || tflags & MW != 0,
        }
    } else if let Some(&(_, v, prec, f)) = table {
        PosReq {
            office: v[0],
            bedroom: v[1],
            dining: v[2],
            tomb: v[3],
            boxes: v[4],
            cabinets: v[5],
            racks: v[6],
            stands: v[7],
            demand_max: v[8],
            mandate_max: v[9],
            precedence: prec,
            sleep_pretension: f & SP != 0,
            punishment_exempt: f & PE != 0,
            meets_workers: f & MW != 0,
        }
    } else {
        PosReq::default()
    }
}

/// A mandate item: DF item type, subtype, plural word, skills that make it.
pub(crate) const MANDATE_ITEMS: &[(&str, &str, &str, &[&str])] = &[
    ("GOBLET", "", "mugs", &["STONECRAFT", "WOODCRAFT", "METALCRAFT"]),
    ("FIGURINE", "", "figurines", &["STONECRAFT", "WOODCRAFT", "METALCRAFT", "BONECARVE"]),
    ("RING", "", "rings", &["STONECRAFT", "METALCRAFT", "BONECARVE"]),
    ("BRACELET", "", "bracelets", &["STONECRAFT", "METALCRAFT", "BONECARVE"]),
    ("CROWN", "", "crowns", &["STONECRAFT", "METALCRAFT"]),
    ("CHAIR", "", "chairs", &["CARPENTRY", "MASONRY", "FORGE_FURNITURE"]),
    ("TABLE", "", "tables", &["CARPENTRY", "MASONRY", "FORGE_FURNITURE"]),
    ("DOOR", "", "doors", &["CARPENTRY", "MASONRY", "FORGE_FURNITURE"]),
    ("CABINET", "", "cabinets", &["CARPENTRY", "MASONRY", "FORGE_FURNITURE"]),
    ("BOX", "", "chests", &["CARPENTRY", "MASONRY", "FORGE_FURNITURE"]),
    ("STATUE", "", "statues", &["MASONRY", "FORGE_FURNITURE"]),
    ("WEAPON", "ITEM_WEAPON_SPEAR", "spears", &["FORGE_WEAPON"]),
    ("WEAPON", "ITEM_WEAPON_AXE_BATTLE", "battle axes", &["FORGE_WEAPON"]),
    ("ARMOR", "ITEM_ARMOR_BREASTPLATE", "breastplates", &["FORGE_ARMOR"]),
    ("HELM", "ITEM_HELM_HELM", "helms", &["FORGE_ARMOR"]),
];

fn item_entry(kind: &str, sub: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static [&'static str])> {
    MANDATE_ITEMS.iter().find(|e| e.0 == kind && (e.1.is_empty() || e.1 == sub))
}

/// Furniture a noble may demand: building kind, plural word, room kinds (bit 0 bedroom,
/// 1 office, 2 dining) it fits in.
pub(crate) const DEMAND_FURNITURE: &[(&str, &str, u8)] = &[
    ("Cabinet", "cabinet", 0b011),
    ("Box", "chest", 0b011),
    ("Weaponrack", "weapon rack", 0b011),
    ("Armorstand", "armor stand", 0b011),
    ("Statue", "statue", 0b111),
    ("Table", "table", 0b111),
    ("Chair", "chair", 0b111),
    ("Door", "door", 0b111),
];

/// Room kinds of the requirement system.
pub const RK_BEDROOM: u8 = 0;
pub const RK_OFFICE: u8 = 1;
pub const RK_DINING: u8 = 2;
pub const RK_TOMB: u8 = 3;

pub(crate) fn zone_room_kind(b: &Building) -> Option<u8> {
    if b.kind != "Civzone" {
        return None;
    }
    Some(match b.subtype.as_str() {
        "Bedroom" => RK_BEDROOM,
        "Office" | "Study" => RK_OFFICE,
        "DiningHall" => RK_DINING,
        "Tomb" => RK_TOMB,
        _ => return None,
    })
}

pub fn room_kind_name(k: u8) -> &'static str {
    ["bedroom", "office", "dining room", "tomb"][k as usize & 3]
}

/// DF's tier names per room kind (w:Noble; low tomb names ESTIMATE, UI text only).
pub fn tier_name(kind: u8, tier: i32) -> &'static str {
    let t = tier.clamp(0, 7) as usize;
    match kind {
        RK_BEDROOM => ["Meager Quarters", "Modest Quarters", "Quarters", "Decent Quarters", "Fine Quarters", "Great Bedroom", "Grand Bedroom", "Royal Bedroom"][t],
        RK_OFFICE => ["Meager Office", "Modest Office", "Office", "Decent Office", "Splendid Office", "Throne Room", "Opulent Throne Room", "Royal Throne Room"][t],
        RK_DINING => ["Meager Dining Room", "Modest Dining Room", "Dining Room", "Decent Dining Room", "Fine Dining Room", "Great Dining Hall", "Grand Dining Hall", "Royal Dining Hall"][t],
        _ => ["Grave", "Servant's Burial Chamber", "Burial Chamber", "Tomb", "Fine Tomb", "Mausoleum", "Grand Mausoleum", "Royal Mausoleum"][t],
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct MandateRt {
    pub id: i32,
    /// Unit id of the issuing noble.
    pub noble: i32,
    pub position: String,
    pub export_ban: bool,
    pub item_type: String,
    pub item_subtype: String,
    /// Display only: matching ignores the material (w:Mandate).
    pub mat: String,
    /// Fallback for imported texts with an unknown item ("Ban export of <word>").
    pub word: String,
    pub amount_total: i32,
    pub amount_remaining: i32,
    pub issued: u64,
    pub deadline: u64,
    pub strikes: u8,
    pub jail_months: u8,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct DemandRt {
    pub id: i32,
    pub noble: i32,
    /// Building kind (`Cabinet`, `Box`, ...).
    pub building: String,
    /// Material index wanted, -1 = any.
    pub mat: i32,
    pub room: u8,
    pub issued: u64,
    pub due: u64,
    pub forget: u64,
    pub met: u64,
    pub next_thought: u64,
    pub text: String,
}

/// A mandate punishment waiting in justice.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct SentenceRt {
    pub crime: i32,
    pub noble: i32,
    pub strikes: u8,
    pub jail_months: u8,
    pub at: u64,
    pub delayed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub(crate) struct Bookkeeper {
    /// Target precision 1..=5.
    pub level: u8,
    /// Precision of the current records (0 = none yet).
    pub reached: u8,
    pub work: u32,
    pub records: Vec<(String, i32)>,
    pub recorded_at: u64,
}

impl Default for Bookkeeper {
    fn default() -> Self {
        Bookkeeper { level: 1, reached: 0, work: 0, records: Vec::new(), recorded_at: 0 }
    }
}

/// What the noble simulation did (UI, bench, tests).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct NoblesReport {
    pub mandates_issued: u64,
    pub mandates_met: u64,
    pub mandates_missed: u64,
    pub export_bans: u64,
    pub bans_broken: u64,
    pub bans_expired: u64,
    /// (tick, text) of each mandate sentence.
    pub sentences: Vec<(u64, String)>,
    pub no_punishment: u64,
    pub delayed_punishment: u64,
    pub demands_issued: u64,
    pub demands_met: u64,
    pub demands_angry: u64,
    pub demands_forgotten: u64,
    pub meetings: u64,
    pub complaints: u64,
    pub tomb_thoughts: u64,
    pub pretension: u64,
    pub noble_room_thoughts: u64,
    pub elected: u64,
    pub reelected: u64,
    pub orders_validated: u64,
    pub records_made: u64,
}

/// Cached per-holder data (rebuilt when the positions change; not saved).
#[derive(Clone, Debug, Default)]
pub(crate) struct HolderRt {
    pub unit: i32,
    pub slot: usize,
    pub req: PosReq,
    /// Index into `World::nobles` of the highest-ranked position (lowest precedence).
    pub ni: usize,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Nobles15 {
    /// Module off (tests, comparisons): the old population mandate code runs instead.
    pub off: bool,
    pub ready: bool,
    pub mandates: Vec<MandateRt>,
    pub next_id: i32,
    /// Last mandate/demand per noble unit (shared cooldown).
    pub last_issue: Vec<(i32, u64)>,
    pub demands: Vec<DemandRt>,
    pub sentences: Vec<SentenceRt>,
    /// Who brought an item to the depot (item id, unit id), while an export ban runs.
    pub depot: Vec<(i32, i32)>,
    pub bk: Bookkeeper,
    pub rep: NoblesReport,
    pub last_day: u64,
    /// Whole years of age per holder unit (birthday detection).
    pub ages: Vec<(i32, i32)>,
    pub last_pretension: Vec<(i32, u64)>,
    /// Manager office work towards the next validation (ticks).
    pub validate_work: u64,
    pub manager_warned: bool,
    /// Work orders created after the manager had to validate (ids, oldest first).
    pub pending: Vec<i32>,
    #[serde(skip)]
    pub holders: Vec<HolderRt>,
    /// Required room tier + 1 per unit slot: [bedroom, office, dining, tomb]; 0 = none.
    #[serde(skip)]
    pub req_tier: Vec<[u8; 4]>,
    #[serde(skip)]
    pub key: u64,
}

fn mix_str(mix: &mut dyn FnMut(u64), s: &str) {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    mix(h);
}

impl Sim {
    pub(crate) fn nobles15_hash(&self, mix: &mut dyn FnMut(u64)) {
        let n = &self.n15;
        if !n.ready {
            return;
        }
        mix(0x4E15_0000 ^ n.mandates.len() as u64);
        for m in &n.mandates {
            mix(m.id as u64);
            mix(m.noble as u64);
            mix(m.deadline);
            mix(m.amount_remaining as u64);
            mix(m.export_ban as u64 | (m.strikes as u64) << 8 | (m.jail_months as u64) << 16);
            mix_str(mix, &m.item_type);
            mix_str(mix, &m.item_subtype);
        }
        mix(n.demands.len() as u64);
        for d in &n.demands {
            mix(d.id as u64);
            mix(d.noble as u64);
            mix(d.due);
            mix(d.met);
            mix(d.room as u64);
            mix_str(mix, &d.building);
        }
        mix(n.sentences.len() as u64);
        for s in &n.sentences {
            mix(s.crime as u64 ^ (s.delayed as u64) << 40);
        }
        mix(n.bk.level as u64 | (n.bk.reached as u64) << 8 | (n.bk.work as u64) << 16);
        mix(n.pending.len() as u64 ^ n.validate_work << 16);
        let r = &n.rep;
        for v in [r.mandates_issued, r.mandates_met, r.mandates_missed, r.bans_broken, r.demands_issued, r.demands_met, r.meetings, r.complaints, r.tomb_thoughts, r.pretension, r.orders_validated] {
            mix(v);
        }
    }

    /// [saves11] Rebuild the caches after an exact load.
    pub(crate) fn nobles15_after_load(&mut self) {
        self.n15.key = 0;
        self.n15.holders.clear();
        self.n15.req_tier.clear();
        self.nobles15_refresh_holders();
    }

    fn n15_setting(&self, key: &str, default: u32) -> u32 {
        self.world
            .settings
            .other
            .iter()
            .find_map(|s| s.strip_prefix(key).and_then(|r| r.strip_prefix('=')).and_then(|v| v.trim().parse().ok()))
            .filter(|&v: &u32| v > 0)
            .unwrap_or(default)
    }

    /// Rebuild the holder cache when `World::nobles` or the unit slots changed.
    pub(crate) fn nobles15_refresh_holders(&mut self) {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ self.ai.len() as u64;
        for n in &self.world.nobles {
            h = (h ^ n.holder as u64 ^ (n.token.len() as u64) << 32 ^ (n.token.as_bytes().first().copied().unwrap_or(0) as u64) << 40).wrapping_mul(0x100_0000_01b3);
            h = (h ^ n.mandate_max as u64 ^ (n.required_bedroom as u64) << 20).wrapping_mul(0x100_0000_01b3);
        }
        if h == self.n15.key && self.n15.req_tier.len() == self.ai.len() {
            return;
        }
        self.n15.key = h;
        let holders: Vec<HolderRt> = holders_of(&self.world.nobles, |id| self.unit_slot.contains_key(&id))
            .into_iter()
            .map(|(unit, req, ni)| HolderRt { unit, slot: self.unit_slot[&unit], req, ni })
            .collect();
        let mut rt = vec![[0u8; 4]; self.ai.len()];
        for x in &holders {
            let r = &x.req;
            let enc = |v: i32| if v > 0 { (tier_of(v) + 1) as u8 } else { 0 };
            rt[x.slot] = [enc(r.bedroom), enc(r.office), enc(r.dining), enc(r.tomb)];
        }
        self.n15.holders = holders;
        self.n15.req_tier = rt;
    }

    fn n15_holder(&self, unit: i32) -> Option<&HolderRt> {
        self.n15.holders.iter().find(|h| h.unit == unit)
    }

    /// Slot of a living citizen holder able to act (not caged, not in a mood).
    fn n15_able(&self, unit: i32) -> Option<usize> {
        let &s = self.unit_slot.get(&unit)?;
        if !self.s2_citizen(s) {
            return None;
        }
        let u = &self.world.units[self.ai[s].uidx];
        if u.flags.caged || !u.mood.is_empty() || self.justice_is_prisoner(unit) {
            return None;
        }
        Some(s)
    }

    fn n15_slot_of(&self, unit: i32) -> Option<usize> {
        self.unit_slot.get(&unit).copied().filter(|&s| self.ai[s].kind == Kind::Worker)
    }

    fn n15_title(&self, unit: i32) -> String {
        let name = self.world.unit(unit).map(|u| u.name.clone()).unwrap_or_else(|| format!("unit #{unit}"));
        let pos = self.n15_holder(unit).and_then(|h| self.world.nobles.get(h.ni)).map(|n| if n.name.is_empty() { n.token.to_ascii_lowercase().replace('_', " ") } else { n.name.clone() }).unwrap_or_default();
        if pos.is_empty() {
            name
        } else {
            format!("{name}, {pos}")
        }
    }

    /// The module runs (not switched off, society on). Before its first pass the population
    /// module keeps the imported mandates for it.
    pub(crate) fn nobles15_active(&self) -> bool {
        !self.n15.off && !self.pop.disabled
    }

    /// [hook] `answer_petition_impl`: the MEET_WORKERS noble meets the petitioner.
    pub(crate) fn nobles15_petition_meeting(&mut self, now: u64) {
        if !self.nobles15_active() || !self.n15.ready {
            return;
        }
        let Some(unit) = self.n15.holders.iter().filter(|h| h.req.meets_workers).min_by_key(|h| (h.req.precedence, h.unit)).map(|h| h.unit) else { return };
        let Some(s) = self.n15_able(unit) else { return };
        self.nobles15_meeting(s, unit, now);
    }

    /// [hook] `add_work_order`: above `MANAGER_OFFICE_POP` citizens a new order waits for the
    /// manager (raws MANAGER description; threshold ESTIMATE).
    pub(crate) fn nobles15_order_created(&mut self, id: i32) {
        if !self.nobles15_active() || !self.nobles15_order_needs_validation() {
            return;
        }
        if let Some(o) = self.world.work_orders.iter_mut().find(|o| o.id == id) {
            o.validated = false;
            self.n15.pending.push(id);
        }
    }

    /// [hook] `run_orders`: the order waits for validation.
    pub(crate) fn nobles15_order_pending(&self, id: i32) -> bool {
        !self.n15.pending.is_empty() && self.n15.pending.contains(&id)
    }

    // ------------------------------------------------------------------ pass

    /// [hook] Every `N15_PERIOD` ticks from `tick_inner`.
    pub(crate) fn nobles15_pass(&mut self, now: u64) {
        if !self.nobles15_active() || self.social_is_off() {
            return;
        }
        if !self.n15.ready {
            if !self.pop.ready {
                return;
            }
            self.nobles15_init(now);
        }
        self.nobles15_refresh_holders();
        self.n15_mandates_tick(now);
        self.n15_manager_tick();
        let day = now / DAY;
        if day != self.n15.last_day {
            self.n15.last_day = day;
            self.nobles15_day(now);
        }
    }

    /// First pass: take over the mandates (exported details first, else the population module's).
    pub(crate) fn nobles15_init(&mut self, now: u64) {
        self.n15.ready = true;
        self.n15.last_day = now / DAY;
        self.nobles15_refresh_holders();
        let old = std::mem::take(&mut self.pop.mandates);
        let mut seen: Vec<(String, i32)> = Vec::new();
        for ni in 0..self.world.nobles.len() {
            let n = &self.world.nobles[ni];
            if n.holder < 0 {
                continue;
            }
            let (tok, holder) = (n.token.clone(), n.holder);
            for d in n.mandates_detail.clone() {
                if seen.iter().any(|(t, h)| *t == d.text && *h == holder) {
                    continue;
                }
                seen.push((d.text.clone(), holder));
                let ban = d.mode == "Export";
                if !ban && d.mode != "Make" {
                    continue; // Guild mandates: MISSING (no guild agreements)
                }
                let left = (d.timeout_limit as i64 - d.timeout_counter as i64).max(0) as u64 * 10;
                let id = self.n15_next_id();
                let word = if d.item_type.is_empty() { mandate_word(&d.text) } else { String::new() };
                self.n15.mandates.push(MandateRt {
                    id,
                    noble: holder,
                    position: tok.clone(),
                    export_ban: ban,
                    item_type: d.item_type.clone(),
                    item_subtype: d.item_subtype.clone(),
                    mat: d.mat.clone(),
                    word,
                    amount_total: d.amount_total.max(1),
                    amount_remaining: if ban { 0 } else { d.amount_remaining.max(0) },
                    issued: now,
                    deadline: now + if d.timeout_limit > 0 { left } else if ban { BAN_DURATION } else { 3 * MONTH },
                    strikes: d.punishment_hammerstrikes.clamp(0, 255) as u8,
                    jail_months: d.punishment_prison_time.clamp(0, 255) as u8,
                    text: d.text.clone(),
                });
            }
        }
        // Texts without details (older exports): the population module's parsed mandates.
        for (k, m) in old.into_iter().enumerate() {
            let holder = self.world.nobles.iter().find(|n| n.token == m.noble).map(|n| n.holder).unwrap_or(-1);
            if holder < 0 || seen.iter().any(|(t, h)| *t == m.text && *h == holder) {
                continue;
            }
            let ban = m.kind.starts_with("BAN:");
            let (it, sub, word) = if ban {
                let w = m.kind.trim_start_matches("BAN:").to_string();
                match MANDATE_ITEMS.iter().find(|e| e.2.starts_with(&w)) {
                    Some(e) => (e.0.to_string(), e.1.to_string(), String::new()),
                    None => (String::new(), String::new(), w),
                }
            } else {
                (m.kind.clone(), String::new(), String::new())
            };
            let mut r = Rng::for_unit(self.seed, k, now, 0x4E15_0010);
            let id = self.n15_next_id();
            self.n15.mandates.push(MandateRt {
                id,
                noble: holder,
                position: m.noble.clone(),
                export_ban: ban,
                item_type: it,
                item_subtype: sub,
                mat: String::new(),
                word,
                amount_total: m.count.max(1),
                amount_remaining: if ban { 0 } else { m.count.max(1) },
                issued: now,
                deadline: m.deadline.max(now),
                strikes: 1 + r.below(5) as u8,
                jail_months: 1 + r.below(4) as u8,
                text: m.text.clone(),
            });
        }
        self.n15_sync_texts();
    }

    fn n15_next_id(&mut self) -> i32 {
        self.n15.next_id += 1;
        self.n15.next_id
    }

    /// Keep `NoblePosition::mandates` (round-4 UI) in step with the live mandates.
    fn n15_sync_texts(&mut self) {
        for n in &mut self.world.nobles {
            if n.holder < 0 {
                n.mandates.clear();
                continue;
            }
            let (tok, holder) = (n.token.clone(), n.holder);
            n.mandates = self.n15.mandates.iter().filter(|m| m.noble == holder && m.position == tok).map(|m| m.text.clone()).collect();
        }
    }

    // ------------------------------------------------------------------ mandates

    fn n15_mandates_tick(&mut self, now: u64) {
        // Deadlines (production: missed; export bans: kept, they run out).
        let mut k = 0;
        let mut changed = false;
        while k < self.n15.mandates.len() {
            let m = &self.n15.mandates[k];
            let gone = !self.n15_alive(m.noble);
            if gone {
                self.n15.mandates.remove(k);
                changed = true;
                continue;
            }
            if !m.export_ban && m.amount_remaining <= 0 {
                let m = self.n15.mandates.remove(k);
                self.n15_mandate_met(&m, now);
                changed = true;
                continue;
            }
            if now < m.deadline {
                k += 1;
                continue;
            }
            let m = self.n15.mandates.remove(k);
            changed = true;
            if m.export_ban {
                self.n15.rep.bans_expired += 1;
            } else {
                self.n15_mandate_missed(&m, now);
            }
        }
        // New mandates.
        let period = self.n15_setting("MANDATE_PERIOD", MANDATE_PERIOD_DEFAULT);
        for h in 0..self.n15.holders.len() {
            let (unit, slot, max) = {
                let x = &self.n15.holders[h];
                (x.unit, x.slot, x.req.mandate_max)
            };
            if max <= 0 {
                continue;
            }
            let active = self.n15.mandates.iter().filter(|m| m.noble == unit).count() as i32;
            if active >= max || !self.n15_cooled(unit, now) {
                continue;
            }
            let mut r = Rng::for_unit(self.seed, slot, now, 0x4E15_0001);
            if !r.chance(1, period) {
                continue;
            }
            if self.n15_able(unit).is_none() {
                continue;
            }
            self.n15_issue_mandate(h, &mut r, now);
            changed = true;
        }
        if changed {
            self.n15_sync_texts();
        }
    }

    fn n15_alive(&self, unit: i32) -> bool {
        self.unit_slot.get(&unit).map(|&s| self.ai[s].kind != Kind::Dead && !self.world.units[self.ai[s].uidx].flags.dead).unwrap_or(false)
    }

    fn n15_cooled(&self, unit: i32, now: u64) -> bool {
        self.n15.last_issue.iter().find(|(u, _)| *u == unit).map(|&(_, t)| now >= t + MANDATE_COOLDOWN).unwrap_or(true)
    }

    fn n15_mark_issue(&mut self, unit: i32, now: u64) {
        match self.n15.last_issue.iter_mut().find(|(u, _)| *u == unit) {
            Some(e) => e.1 = now,
            None => self.n15.last_issue.push((unit, now)),
        }
    }

    fn n15_issue_mandate(&mut self, h: usize, r: &mut Rng, now: u64) {
        let (unit, ni) = (self.n15.holders[h].unit, self.n15.holders[h].ni);
        let position = self.world.nobles[ni].token.clone();
        // ESTIMATE: 2 in 3 production orders, 1 in 3 export bans.
        let ban = r.chance(1, 3);
        let e = &MANDATE_ITEMS[r.below(MANDATE_ITEMS.len() as u32) as usize];
        let amount = 1 + r.below(5) as i32;
        // OK (w:Mandate): production deadline 3-6 months (DF counter units of 10 ticks).
        let deadline = if ban { now + BAN_DURATION } else { now + 10 * (10_080 + r.below(10_081) as u64) };
        let text = if ban { format!("Ban export of {}", e.2) } else { format!("Make {amount} {}", e.2) };
        let id = self.n15_next_id();
        self.n15.mandates.push(MandateRt {
            id,
            noble: unit,
            position,
            export_ban: ban,
            item_type: e.0.into(),
            item_subtype: e.1.into(),
            mat: String::new(),
            word: String::new(),
            amount_total: if ban { 0 } else { amount },
            amount_remaining: if ban { 0 } else { amount },
            issued: now,
            deadline,
            strikes: 1 + r.below(5) as u8,
            jail_months: 1 + r.below(4) as u8,
            text,
        });
        self.n15_mark_issue(unit, now);
        self.n15.rep.mandates_issued += 1;
        self.pop.rep.mandates_issued += 1;
        let who = self.n15_title(unit);
        let what = if ban { "has imposed a ban on certain exports" } else { "has mandated the construction of certain goods" };
        if ban {
            self.n15.rep.export_bans += 1;
        }
        self.announce(now, format!("{who} {what}."), None, AnnouncementKind::Info);
    }

    fn n15_mandate_met(&mut self, m: &MandateRt, now: u64) {
        self.n15.rep.mandates_met += 1;
        self.pop.rep.mandates_met += 1;
        if let Some(s) = self.n15_slot_of(m.noble) {
            self.feel(s, "MandateDeadlineMet", Class::PosMild, 100, -1, 0, now);
        }
        let who = self.n15_title(m.noble);
        self.announce(now, format!("{who} is pleased: the mandate \"{}\" was fulfilled.", m.text), None, AnnouncementKind::Info);
    }

    /// A production mandate ran out: the noble is upset and names a culprit who is sentenced
    /// at once with the mandate's punishment (OBSERVED: auto-sentenced, no witnesses).
    fn n15_mandate_missed(&mut self, m: &MandateRt, now: u64) {
        self.n15.rep.mandates_missed += 1;
        self.pop.rep.mandates_missed += 1;
        let Some(ns) = self.n15_slot_of(m.noble) else { return };
        self.feel(ns, "MandateIgnored", Class::Neg, 200, -1, 0, now);
        let who = self.n15_title(m.noble);
        self.announce(now, format!("{who} is upset: the mandate \"{}\" was not fulfilled.", m.text), None, AnnouncementKind::Warning);
        self.n15_blame(m, ns, "ProductionOrderViolation", None, now);
    }

    /// Pick the culprit (weighted by the item's skill; exempt positions never) or use
    /// `culprit`, record the crime and sentence. No candidate -> `NoPunishment`.
    fn n15_blame(&mut self, m: &MandateRt, ns: usize, kind: &str, culprit: Option<usize>, now: u64) {
        let cs = match culprit {
            Some(c) => Some(c),
            None => self.n15_pick_culprit(m, ns, now),
        };
        let Some(cs) = cs else {
            self.n15.rep.no_punishment += 1;
            self.feel(ns, "NoPunishment", Class::Neg, 150, -1, 0, now);
            return;
        };
        let nid = self.ai[ns].id;
        let id = self.justice_crime(kind, cs, nid, true, now);
        if id < 0 {
            return;
        }
        if let Some(c) = self.world.crimes.iter_mut().find(|c| c.id == id) {
            c.witnesses.clear(); // OBSERVED: mandate crimes have no witnesses
        }
        self.n15.sentences.push(SentenceRt { crime: id, noble: m.noble, strikes: m.strikes, jail_months: m.jail_months, at: now, delayed: false });
        let cid = self.ai[cs].id;
        let name = self.world.units[self.ai[cs].uidx].name.clone();
        self.n15.rep.sentences.push((now, format!("{name}: hammerstrikes {}, jail {} months ({})", m.strikes, m.jail_months, m.text)));
        self.convict_impl(id, cid);
    }

    fn n15_pick_culprit(&self, m: &MandateRt, ns: usize, now: u64) -> Option<usize> {
        let skills: &[&str] = item_entry(&m.item_type, &m.item_subtype).map(|e| e.3).unwrap_or(&[]);
        let mut total = 0u32;
        let weight = |t: usize| -> u32 {
            if t == ns || !self.s2_citizen(t) {
                return 0;
            }
            let u = &self.world.units[self.ai[t].uidx];
            if u.profession == "CHILD" || u.profession == "BABY" {
                return 0;
            }
            if self.n15_holder(u.id).map(|h| h.req.punishment_exempt).unwrap_or(false) {
                return 0;
            }
            if skills.iter().any(|k| u.skill(k) > 0) {
                SKILLED_WEIGHT
            } else {
                1
            }
        };
        for t in 0..self.ai.len() {
            total += weight(t);
        }
        if total == 0 {
            return None;
        }
        let mut r = Rng::for_unit(self.seed, ns, now, 0x4E15_0002);
        let mut pick = r.below(total);
        for t in 0..self.ai.len() {
            let w = weight(t);
            if pick < w {
                return Some(t);
            }
            pick -= w;
        }
        None
    }

    /// [hook] `production::push_item`: an item came into being. Made by the fort (value not
    /// yet set, maker a citizen or nobody; caravan goods arrive with a value) -> fulfils
    /// matching production mandates (by type, not material: w:Mandate).
    pub(crate) fn nobles15_item_made(&mut self, it: &rf_core::Item) {
        if self.n15.mandates.is_empty() || it.value != 0 {
            return;
        }
        if it.holder >= 0 {
            let citizen = self.unit_slot.get(&it.holder).map(|&s| self.world.units[self.ai[s].uidx].flags.citizen).unwrap_or(false);
            if !citizen {
                return;
            }
        }
        for m in &mut self.n15.mandates {
            if m.export_ban || m.amount_remaining <= 0 || m.item_type != it.kind {
                continue;
            }
            if !m.item_subtype.is_empty() && m.item_subtype != it.subtype {
                continue;
            }
            m.amount_remaining -= it.stack.max(1);
            break;
        }
    }

    /// [hook] `trade::trade_stored`: `slot` brought `item` to the depot.
    pub(crate) fn nobles15_depot_hauled(&mut self, item: i32, slot: u32) {
        if self.n15.off || !self.n15.mandates.iter().any(|m| m.export_ban) || slot as usize >= self.ai.len() {
            return;
        }
        let Some(b) = self.world.item(item).map(|i| i.building) else { return };
        if b < 0 || !self.world.buildings.iter().any(|x| x.id == b && x.kind == "TradeDepot") {
            return;
        }
        let uid = self.ai[slot as usize].id;
        self.n15.depot.retain(|(i, _)| *i != item);
        self.n15.depot.push((item, uid));
        if self.n15.depot.len() > 4096 {
            self.n15.depot.drain(..1024);
        }
    }

    /// [hook] `trade::trade_impl` on commit: the fort sells `offer`. A banned item offered
    /// directly is a crime of the dwarf who hauled it; banned items inside a bin: nobody is
    /// punished, the noble is unhappy (w:Mandate). ESTIMATE: "leaves the map" = commit.
    pub(crate) fn nobles15_items_traded(&mut self, offer: &[i32], now: u64) {
        if !self.n15.mandates.iter().any(|m| m.export_ban) {
            return;
        }
        let mut k = 0;
        while k < self.n15.mandates.len() {
            if !self.n15.mandates[k].export_ban {
                k += 1;
                continue;
            }
            let mut direct: Option<i32> = None;
            let mut in_bin = false;
            {
                let m = &self.n15.mandates[k];
                for &id in offer {
                    let Some(it) = self.world.item(id) else { continue };
                    if ban_hits(m, it) {
                        direct = Some(id);
                        break;
                    }
                }
                if direct.is_none() {
                    in_bin = self.world.items.iter().enumerate().any(|(i, it)| it.container >= 0 && offer.contains(&it.container) && !self.imeta[i].dead && ban_hits(m, it));
                }
            }
            if direct.is_none() && !in_bin {
                k += 1;
                continue;
            }
            let m = self.n15.mandates.remove(k);
            self.n15.rep.bans_broken += 1;
            self.pop.rep.mandates_missed += 1;
            let Some(ns) = self.n15_slot_of(m.noble) else { continue };
            self.feel(ns, "MandateIgnored", Class::Neg, 200, -1, 0, now);
            let who = self.n15_title(m.noble);
            self.announce(now, format!("{who} is upset: the export ban \"{}\" was broken.", m.text), None, AnnouncementKind::Warning);
            let hauler = direct.and_then(|id| self.n15.depot.iter().find(|(i, _)| *i == id).map(|&(_, u)| u)).and_then(|u| self.n15_slot_of(u)).filter(|&s| self.s2_citizen(s) && s != ns);
            match hauler {
                Some(hs) => self.n15_blame(&m, ns, "ExportViolation", Some(hs), now),
                None => {
                    self.n15.rep.no_punishment += 1;
                    self.feel(ns, "NoPunishment", Class::Neg, 150, -1, 0, now);
                }
            }
        }
        self.n15_sync_texts();
    }

    /// [hook] `justice::convict_impl`: the mandate's own punishment as (jail days, strikes),
    /// like `justice::sentence`; a DF month is 28 days.
    pub(crate) fn nobles15_sentence(&self, crime: i32) -> Option<(i32, i32)> {
        self.n15.sentences.iter().find(|s| s.crime == crime).map(|s| (s.jail_months as i32 * 28, s.strikes as i32))
    }

    /// [hook] `justice::convict_impl`: PUNISHMENT_EXEMPTION ("Immune dwarves aren't punished
    /// for any crimes", w:Noble).
    pub(crate) fn nobles15_exempt(&self, unit: i32) -> bool {
        !self.n15.off && self.n15_holder(unit).map(|h| h.req.punishment_exempt).unwrap_or(false)
    }

    /// Daily: punishments that did not start (DelayedPunishment / PunishmentDelayed).
    fn n15_sentences_day(&mut self, now: u64) {
        let mut k = 0;
        while k < self.n15.sentences.len() {
            let sr = self.n15.sentences[k].clone();
            let status = self.world.crimes.iter().find(|c| c.id == sr.crime).map(|c| c.status.clone()).unwrap_or_default();
            if status != "Convicted" || now >= sr.at + 4 * DELAY_THOUGHT_AFTER {
                if status != "Convicted" && status != "Serving" {
                    self.n15.sentences.remove(k);
                    continue;
                }
                if status == "Serving" || now >= sr.at + 4 * DELAY_THOUGHT_AFTER {
                    if now >= sr.at + 4 * DELAY_THOUGHT_AFTER {
                        self.n15.sentences.remove(k);
                        continue;
                    }
                    k += 1;
                    continue;
                }
            }
            if !sr.delayed && now >= sr.at + DELAY_THOUGHT_AFTER {
                self.n15.sentences[k].delayed = true;
                self.n15.rep.delayed_punishment += 1;
                if let Some(s) = self.n15_slot_of(sr.noble) {
                    self.feel(s, "DelayedPunishment", Class::NegMild, 80, -1, 0, now);
                }
                let culprit = self.world.crimes.iter().find(|c| c.id == sr.crime).map(|c| c.convicted).unwrap_or(-1);
                if let Some(s) = self.n15_slot_of(culprit) {
                    self.feel(s, "PunishmentDelayed", Class::PosMild, 60, -1, 0, now);
                }
            }
            k += 1;
        }
    }

    /// [hook] `population::elect`: thoughts of the winner; a mayor voted out loses his
    /// mandates and demands (w:Mandate).
    pub(crate) fn nobles15_elected(&mut self, s: usize, old: i32, changed: bool, now: u64) {
        let uid = self.ai[s].id;
        let mayor = self.world.nobles.iter().any(|n| n.token == "MAYOR" && n.holder == uid);
        if self.n15.off || !mayor {
            if changed {
                self.feel(s, "BecomeNoble", Class::Pos, 200, -1, 0, now);
            }
            return;
        }
        if changed {
            self.n15.rep.elected += 1;
            self.feel(s, "Elected", Class::Pos, 200, -1, 0, now);
            if old >= 0 {
                self.nobles15_drop_holder(old);
            }
        } else {
            self.n15.rep.reelected += 1;
            self.feel(s, "Reelected", Class::PosMild, 120, -1, 0, now);
            let name = self.world.units[self.ai[s].uidx].name.clone();
            self.announce(now, format!("{name} has been re-elected mayor."), None, AnnouncementKind::Info);
        }
    }

    /// End all mandates and demands of `unit` (death, lost election).
    pub(crate) fn nobles15_drop_holder(&mut self, unit: i32) {
        let before = self.n15.mandates.len();
        self.n15.mandates.retain(|m| m.noble != unit);
        self.n15.demands.retain(|d| d.noble != unit);
        if before != self.n15.mandates.len() {
            self.n15_sync_texts();
        }
    }

    // ------------------------------------------------------------------ daily

    pub(crate) fn nobles15_day(&mut self, now: u64) {
        self.n15_sentences_day(now);
        self.n15_sheriff_replaced();
        self.n15_demands_day(now);
        self.n15_birthdays(now);
        self.n15_complaints(now);
        self.n15_pretension(now);
        self.n15_bookkeeper_day(now);
    }

    /// raws: SHERIFF `REPLACED_BY:CAPTAIN_OF_THE_GUARD`, which `REQUIRES_POPULATION:50`.
    /// ESTIMATE: the sheriff's holder carries over (justice keeps working).
    fn n15_sheriff_replaced(&mut self) {
        let (cit, _) = self.pop_counts();
        if cit < 50 || self.world.nobles.iter().any(|n| n.token == "CAPTAIN_OF_THE_GUARD") {
            return;
        }
        let Some(k) = self.world.nobles.iter().position(|n| n.token == "SHERIFF" && !n.flags.iter().any(|f| f == "HAS_BEEN_REPLACED")) else { return };
        let holder = self.world.nobles[k].holder;
        self.world.nobles[k].holder = -1;
        self.world.nobles[k].flags.push("HAS_BEEN_REPLACED".into());
        let entity_id = self.world.nobles[k].entity_id;
        let mut n = NoblePosition { token: "CAPTAIN_OF_THE_GUARD".into(), name: "captain of the guard".into(), holder, entity_id, ..Default::default() };
        let req = pos_req(&n);
        n.required_office = req.office;
        n.required_bedroom = req.bedroom;
        n.required_dining = req.dining;
        n.required_boxes = req.boxes;
        n.required_cabinets = req.cabinets;
        n.required_racks = req.racks;
        n.required_stands = req.stands;
        n.precedence = req.precedence;
        self.world.nobles.push(n);
        let now = self.ticks;
        self.announce(now, "The sheriff has been replaced by the captain of the guard.".into(), None, AnnouncementKind::Info);
    }

    // ------------------------------------------------------------------ rooms

    /// Value of a zone as the room system sees it: the social room value for bedrooms and
    /// dining halls (same numbers as the room thoughts), else furniture + floor + walls.
    pub(crate) fn nobles15_zone_value(&self, b: &Building) -> i32 {
        if let Some(v) = self.room_value(b.id) {
            return v;
        }
        let mut v = 0;
        for f in &self.world.buildings {
            if !f.built || f.kind == "Civzone" || !is_furniture(&f.kind) || !b.contains(f.center) {
                continue;
            }
            if f.items.is_empty() {
                v += 10;
            }
            for id in &f.items {
                if let Some(&i) = self.item_idx.get(id) {
                    v += self.world.items[i as usize].value.max(0);
                }
            }
        }
        let mut walls: Vec<usize> = Vec::new();
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                let p = Pos::new(x, y, b.z);
                if !b.contains(p) {
                    continue;
                }
                let Some(i) = self.world.idx(p) else { continue };
                v += self.room_tile_value(i);
                for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    let q = Pos::new(x + dx, y + dy, b.z);
                    if b.contains(q) {
                        continue;
                    }
                    if let Some(j) = self.world.idx(q) {
                        if matches!(self.world.shape(q), rf_core::Shape::Wall | rf_core::Shape::Fortification) {
                            walls.push(j);
                        }
                    }
                }
            }
        }
        walls.sort_unstable();
        walls.dedup();
        for j in walls {
            v += self.room_tile_value(j);
        }
        v
    }

    /// Best owned zone of a room kind: (zone id, value).
    pub(crate) fn nobles15_best_room(&self, unit: i32, kind: u8) -> Option<(i32, i32)> {
        let mut best: Option<(i32, i32)> = None;
        for b in &self.world.buildings {
            if b.owner != unit || zone_room_kind(b) != Some(kind) {
                continue;
            }
            let v = self.nobles15_zone_value(b);
            if best.map(|(id, bv)| (v, -b.id) > (bv, -id)).unwrap_or(true) {
                best = Some((b.id, v));
            }
        }
        best
    }

    /// Built furniture of `kind` inside the noble's bedroom and office zones (ESTIMATE: union).
    pub(crate) fn nobles15_furniture(&self, unit: i32, kind: &str) -> i32 {
        let zones: Vec<&Building> = self.world.buildings.iter().filter(|b| b.owner == unit && matches!(zone_room_kind(b), Some(RK_BEDROOM) | Some(RK_OFFICE))).collect();
        if zones.is_empty() {
            return 0;
        }
        self.world.buildings.iter().filter(|f| f.built && f.kind == kind && zones.iter().any(|z| z.contains(f.center))).count() as i32
    }

    /// [hook] Social bedroom/dining thoughts: a holder with a room requirement rates rooms
    /// against it (H1, OBSERVED): the tier is lowered by `max(0, required tier - 1)`.
    /// Returns true if the noble rule applied (the commoner thought is skipped).
    pub(crate) fn nobles15_room_feel(&mut self, s: usize, thought: &'static str, tier: i32, now: u64) -> bool {
        if self.n15.off {
            return false;
        }
        let k = match thought {
            "BedroomQuality" => 0,
            "OfficeQuality" => 1,
            "DiningQuality" => 2,
            "TombQuality" => 3,
            _ => return false,
        };
        let req = self.n15.req_tier.get(s).map(|r| r[k]).unwrap_or(0) as i32 - 1;
        if req <= 0 {
            return false; // no requirement or "any room": commoner rule
        }
        let shifted = tier - (req - 1).max(0);
        self.n15_feel_tier(s, thought, shifted, now);
        true
    }

    /// The commoner tier -> thought mapping, extended below meager for nobles.
    fn n15_feel_tier(&mut self, s: usize, thought: &'static str, t: i32, now: u64) {
        self.n15.rep.noble_room_thoughts += 1;
        match t {
            i32::MIN..=-1 => self.feel(s, thought, Class::Neg, 60 + 30 * (-t).min(5), -1, t - 2, now),
            0..=1 => self.feel(s, thought, Class::NegMild, 60, -1, t - 2, now),
            2 => {}
            3..=4 => self.feel(s, thought, Class::PosMild, 60 + 20 * t, -1, t - 2, now),
            5..=6 => self.feel(s, thought, Class::Pos, 60 + 20 * t, -1, t - 2, now),
            _ => self.feel(s, thought, Class::PosStrong, 200, -1, t - 2, now),
        }
    }

    /// Tier of a zone for a holder's thought (`OfficeQuality`, `TombQuality`).
    fn n15_room_thought(&mut self, s: usize, unit: i32, kind: u8, thought: &'static str, now: u64) -> bool {
        let Some((_, v)) = self.nobles15_best_room(unit, kind) else { return false };
        let tier = tier_of(v);
        if !self.nobles15_room_feel(s, thought, tier, now) {
            self.n15_feel_tier(s, thought, tier, now);
        }
        true
    }

    /// A meeting of a MEET_WORKERS holder: own office, else a dining room, else the bedroom.
    pub(crate) fn nobles15_meeting(&mut self, s: usize, unit: i32, now: u64) {
        self.n15.rep.meetings += 1;
        if self.n15_room_thought(s, unit, RK_OFFICE, "OfficeQuality", now) {
            return;
        }
        let dining = self.nobles15_best_room(unit, RK_DINING).is_some() || self.world.buildings.iter().any(|b| b.kind == "Civzone" && b.subtype == "DiningHall");
        if dining {
            self.feel(s, "MeetingInDiningRoom", Class::NegMild, 60, -1, 0, now);
        } else if self.nobles15_best_room(unit, RK_BEDROOM).is_some() {
            self.feel(s, "MeetingInBedroom", Class::NegMild, 60, -1, 0, now);
        } else {
            self.feel(s, "NoRooms", Class::Neg, 100, -1, 0, now);
        }
    }

    /// Daily: stressed citizens complain to the MEET_WORKERS noble (OBSERVED ~2 per year).
    fn n15_complaints(&mut self, now: u64) {
        let Some(h) = self.n15.holders.iter().filter(|h| h.req.meets_workers).min_by_key(|h| (h.req.precedence, h.unit)).map(|h| (h.unit, h.slot)) else { return };
        let (unit, _) = h;
        let Some(s) = self.n15_able(unit) else { return };
        let (cit, _) = self.pop_counts();
        if cit == 0 {
            return;
        }
        let mut r = Rng::for_unit(self.seed, s, now, 0x4E15_0003);
        if r.below(COMPLAINT_BASE) >= cit as u32 {
            return;
        }
        let stressed = (0..self.ai.len()).any(|t| t != s && self.s2_citizen(t) && self.world.units[self.ai[t].uidx].stress >= 25_000);
        if !stressed {
            return;
        }
        self.n15.rep.complaints += 1;
        self.feel(s, "ReceivedComplaint", Class::NegMild, 60, -1, 0, now);
        self.nobles15_meeting(s, unit, now);
    }

    /// Holders with a tomb requirement think about their tomb on their birthday.
    fn n15_birthdays(&mut self, now: u64) {
        for h in 0..self.n15.holders.len() {
            let (unit, slot, tomb) = (self.n15.holders[h].unit, self.n15.holders[h].slot, self.n15.holders[h].req.tomb);
            if tomb <= 0 || !self.s2_citizen(slot) {
                continue;
            }
            let age = self.world.units[self.ai[slot].uidx].age.floor() as i32;
            let last = match self.n15.ages.iter_mut().find(|(u, _)| *u == unit) {
                Some(e) => std::mem::replace(&mut e.1, age),
                None => {
                    self.n15.ages.push((unit, age));
                    age
                }
            };
            if age <= last {
                continue;
            }
            self.n15.rep.tomb_thoughts += 1;
            if !self.n15_room_thought(slot, unit, RK_TOMB, "TombQuality", now) {
                self.feel(slot, "TombLack", Class::Neg, 100, -1, 0, now);
            }
        }
    }

    /// SLEEP_PRETENSION: a commoner (lower rank) owns a better bedroom (ESTIMATE behaviour).
    fn n15_pretension(&mut self, now: u64) {
        if !self.n15.holders.iter().any(|h| h.req.sleep_pretension) {
            return;
        }
        let mut best_commoner = -1;
        for b in &self.world.buildings {
            if b.owner < 0 || zone_room_kind(b) != Some(RK_BEDROOM) || self.n15_holder(b.owner).is_some() {
                continue;
            }
            best_commoner = best_commoner.max(tier_of(self.nobles15_zone_value(b)));
        }
        if best_commoner < 0 {
            return;
        }
        for h in 0..self.n15.holders.len() {
            let (unit, slot, sp) = (self.n15.holders[h].unit, self.n15.holders[h].slot, self.n15.holders[h].req.sleep_pretension);
            if !sp || !self.s2_citizen(slot) {
                continue;
            }
            let own = self.nobles15_best_room(unit, RK_BEDROOM).map(|(_, v)| tier_of(v)).unwrap_or(-1);
            if own < 0 || own >= best_commoner {
                continue;
            }
            let last = self.n15.last_pretension.iter().find(|(u, _)| *u == unit).map(|&(_, t)| t);
            if last.map(|t| now < t + PRETENSION_EVERY).unwrap_or(false) {
                continue;
            }
            match self.n15.last_pretension.iter_mut().find(|(u, _)| *u == unit) {
                Some(e) => e.1 = now,
                None => self.n15.last_pretension.push((unit, now)),
            }
            self.n15.rep.pretension += 1;
            self.feel(slot, "RoomPretension", Class::Neg, 100, -1, 0, now);
        }
    }

    /// [build_api] Assign a bedroom/office/dining/tomb zone to `unit` (-1 = free).
    pub(crate) fn nobles15_assign_zone(&mut self, zone: i32, unit: i32) -> bool {
        let Some(k) = self.world.buildings.iter().position(|b| b.id == zone) else { return false };
        if zone_room_kind(&self.world.buildings[k]).is_none() {
            return false;
        }
        if unit >= 0 && !self.unit_slot.contains_key(&unit) {
            return false;
        }
        let old = self.world.buildings[k].owner;
        self.world.buildings[k].owner = unit;
        if self.world.buildings[k].subtype == "Bedroom" {
            for bed in &mut self.beds {
                if bed.owner == old && old >= 0 {
                    let p = self.world.pos_of(bed.tile as usize);
                    if self.world.buildings[k].contains(p) {
                        bed.owner = -1;
                    }
                }
            }
        }
        self.social_force_rebuild();
        true
    }

    // ------------------------------------------------------------------ demands

    /// Daily: issue, check, anger and forget demands (w:Demand).
    fn n15_demands_day(&mut self, now: u64) {
        // Check existing.
        let mut k = 0;
        while k < self.n15.demands.len() {
            let d = self.n15.demands[k].clone();
            if !self.n15_alive(d.noble) {
                self.n15.demands.remove(k);
                continue;
            }
            if d.met > 0 {
                if now >= d.met + DAY {
                    self.n15.demands.remove(k);
                    continue;
                }
                k += 1;
                continue;
            }
            if now >= d.forget {
                self.n15.demands.remove(k);
                self.n15.rep.demands_forgotten += 1;
                let who = self.n15_title(d.noble);
                self.announce(now, format!("{who} has forgotten a demand."), None, AnnouncementKind::Info);
                continue;
            }
            let slot = self.n15_slot_of(d.noble);
            if self.n15_demand_fulfilled(&d) {
                self.n15.demands[k].met = now;
                self.n15.rep.demands_met += 1;
                if let Some(s) = slot {
                    self.feel(s, "Demands", Class::PosMild, 100, -1, 1, now);
                }
                k += 1;
                continue;
            }
            if now > d.due && now >= d.next_thought {
                self.n15.demands[k].next_thought = now + DEMAND_ANGER_EVERY;
                self.n15.rep.demands_angry += 1;
                if let Some(s) = slot {
                    self.feel(s, "Demands", Class::Neg, 120, -1, -1, now);
                }
            }
            k += 1;
        }
        // New demands: 1 in DEMAND_PERIOD per day (ESTIMATE), only for owned room kinds.
        let period = self.n15_setting("DEMAND_PERIOD", DEMAND_PERIOD_DEFAULT);
        for h in 0..self.n15.holders.len() {
            let (unit, slot, max) = (self.n15.holders[h].unit, self.n15.holders[h].slot, self.n15.holders[h].req.demand_max);
            if max <= 0 {
                continue;
            }
            let open = self.n15.demands.iter().filter(|d| d.noble == unit && d.met == 0).count() as i32;
            if open >= max || !self.n15_cooled(unit, now) {
                continue;
            }
            let mut r = Rng::for_unit(self.seed, slot, now, 0x4E15_0004);
            if !r.chance(1, period) || self.n15_able(unit).is_none() {
                continue;
            }
            self.n15_issue_demand(unit, &mut r, now);
        }
    }

    fn n15_issue_demand(&mut self, unit: i32, r: &mut Rng, now: u64) -> bool {
        let rooms: Vec<u8> = [RK_BEDROOM, RK_OFFICE, RK_DINING].into_iter().filter(|&k| self.nobles15_best_room(unit, k).is_some()).collect();
        if rooms.is_empty() {
            return false;
        }
        let room = rooms[r.below(rooms.len() as u32) as usize];
        let fits: Vec<&(&str, &str, u8)> = DEMAND_FURNITURE.iter().filter(|f| f.2 & (1 << room) != 0).collect();
        let f = fits[r.below(fits.len() as u32) as usize];
        // ESTIMATE: material = the fort's most common material of that furniture (no preferences).
        let mut mats: Vec<(u16, u32)> = Vec::new();
        for b in &self.world.buildings {
            if b.kind == f.0 && b.built {
                match mats.iter_mut().find(|(m, _)| *m == b.mat) {
                    Some(e) => e.1 += 1,
                    None => mats.push((b.mat, 1)),
                }
            }
        }
        let mat = mats.iter().max_by_key(|(m, n)| (*n, u16::MAX - *m)).map(|&(m, _)| m as i32).unwrap_or(-1);
        let mname = if mat >= 0 { self.world.material(mat as u16).map(|m| m.name.clone()).unwrap_or_default() } else { String::new() };
        let text = if mname.is_empty() { format!("{} in {}", f.1, room_kind_name(room)) } else { format!("{mname} {} in {}", f.1, room_kind_name(room)) };
        let id = self.n15_next_id();
        self.n15.demands.push(DemandRt { id, noble: unit, building: f.0.into(), mat, room, issued: now, due: now + DEMAND_DUE, forget: now + DEMAND_FORGET, met: 0, next_thought: 0, text: text.clone() });
        self.n15_mark_issue(unit, now);
        self.n15.rep.demands_issued += 1;
        let who = self.n15_title(unit);
        self.announce(now, format!("{who} demands: {text}."), None, AnnouncementKind::Info);
        true
    }

    /// A built building of the kind (and material) inside one of the noble's rooms of that
    /// kind; doors on a wall tile next to the room count (w:Demand).
    fn n15_demand_fulfilled(&self, d: &DemandRt) -> bool {
        let zones: Vec<&Building> = self.world.buildings.iter().filter(|b| b.owner == d.noble && zone_room_kind(b) == Some(d.room)).collect();
        self.world.buildings.iter().any(|f| {
            if !f.built || f.kind != d.building || (d.mat >= 0 && f.mat as i32 != d.mat) {
                return false;
            }
            zones.iter().any(|z| {
                if z.contains(f.center) {
                    return true;
                }
                f.kind == "Door" && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| z.contains(Pos::new(f.center.x + dx, f.center.y + dy, f.center.z)))
            })
        })
    }

    // ------------------------------------------------------------------ manager, bookkeeper

    /// Work orders need validation by the manager above `MANAGER_OFFICE_POP` citizens.
    pub(crate) fn nobles15_order_needs_validation(&self) -> bool {
        !self.n15.off && self.pop_counts().0 > MANAGER_OFFICE_POP
    }

    /// Every pass: the manager, if he has an office, validates pending orders.
    fn n15_manager_tick(&mut self) {
        if self.n15.pending.is_empty() {
            return;
        }
        let orders = &self.world.work_orders;
        self.n15.pending.retain(|id| orders.iter().any(|o| o.id == *id));
        if self.n15.pending.is_empty() {
            self.n15.validate_work = 0;
            return;
        }
        let Some(ms) = self.s2_noble_slot("MANAGER") else { return };
        let uid = self.ai[ms].id;
        if self.nobles15_best_room(uid, RK_OFFICE).is_none() {
            if !self.n15.manager_warned {
                self.n15.manager_warned = true;
                let now = self.ticks;
                self.announce(now, "The manager needs an office to validate work orders.".into(), None, AnnouncementKind::Warning);
            }
            return;
        }
        self.n15.manager_warned = false;
        let org = self.world.units[self.ai[ms].uidx].skill("ORGANIZATION").max(0) as u64;
        self.n15.validate_work += N15_PERIOD;
        let need = (VALIDATE_TICKS / (1 + org / 5)).max(1);
        while self.n15.validate_work >= need {
            self.n15.validate_work -= need;
            if self.n15.pending.is_empty() {
                break;
            }
            let id = self.n15.pending.remove(0);
            if let Some(o) = self.world.work_orders.iter_mut().find(|o| o.id == id) {
                o.validated = true;
            }
            self.n15.rep.orders_validated += 1;
            self.skills_order_approved();
        }
    }

    /// Daily bookkeeping (resolved statistically, no walking: ESTIMATE). Level 1 works
    /// anywhere, higher levels need an office (w:Bookkeeper).
    fn n15_bookkeeper_day(&mut self, now: u64) {
        let Some(bs) = self.s2_noble_slot("BOOKKEEPER") else { return };
        let uid = self.ai[bs].id;
        let level = self.n15.bk.level.clamp(1, 5);
        let has_office = self.nobles15_best_room(uid, RK_OFFICE).is_some();
        let target = if has_office { level } else { 1 };
        let skill = self.world.units[self.ai[bs].uidx].skill("RECORD_KEEPING").max(0) as u32;
        // ESTIMATE: initial work level x 2 days at 10 units/day x (1 + skill/10).
        if self.n15.bk.reached != target {
            self.n15.bk.work += 10 + skill;
            self.add_xp(bs, "RECORD_KEEPING", 30);
            if self.n15.bk.work >= target as u32 * 20 {
                self.n15.bk.work = 0;
                self.n15.bk.reached = target;
                self.n15_take_records(now);
            }
        } else if now >= self.n15.bk.recorded_at + RECORDS_EVERY {
            self.add_xp(bs, "RECORD_KEEPING", 10);
            self.n15_take_records(now);
        }
    }

    fn n15_take_records(&mut self, now: u64) {
        let mut rec: Vec<(String, i32)> = Vec::new();
        for (i, it) in self.world.items.iter().enumerate() {
            if self.imeta[i].dead || it.flags.construction || it.holder >= 0 && !self.unit_slot.get(&it.holder).map(|&s| self.world.units[self.ai[s].uidx].flags.citizen).unwrap_or(false) {
                continue;
            }
            match rec.binary_search_by(|(k, _)| k.as_str().cmp(it.kind.as_str())) {
                Ok(p) => rec[p].1 += it.stack.max(1),
                Err(p) => rec.insert(p, (it.kind.clone(), it.stack.max(1))),
            }
        }
        self.n15.bk.records = rec;
        self.n15.bk.recorded_at = now;
        self.n15.rep.records_made += 1;
    }
}

fn is_furniture(kind: &str) -> bool {
    matches!(kind, "Bed" | "Table" | "Chair" | "Door" | "Cabinet" | "Box" | "Coffin" | "Statue" | "Armorstand" | "Weaponrack" | "Slab" | "Bookcase")
}

/// "Ban export of spears" -> "spear"; "Make 3 crowns" -> "crown".
fn mandate_word(text: &str) -> String {
    let t = text.to_ascii_lowercase();
    let rest = t.strip_prefix("ban export of ").map(|s| s.to_string()).unwrap_or_else(|| t.split_whitespace().skip(2).collect::<Vec<_>>().join(" "));
    let w = rest.split(" (").next().unwrap_or("").trim().to_string();
    w.strip_suffix('s').map(|s| s.to_string()).unwrap_or(w)
}

/// Displayed count at a bookkeeper precision level (ESTIMATE rounding).
pub fn shown_count(level: u8, n: i32) -> String {
    let round = |n: i32, m: i32| ((n + m / 2) / m) * m;
    match level {
        5 => n.to_string(),
        4 => round(n, 5).to_string(),
        3 => round(n, 10).to_string(),
        2 => format!("~{}", round(n, 50)),
        1 => match n {
            ..=9 => "<10".into(),
            10..=99 => "10+".into(),
            100..=999 => "100+".into(),
            _ => "1000+".into(),
        },
        _ => "?".into(),
    }
}
