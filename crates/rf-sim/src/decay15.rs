//! [decay15] Decay, rot, miasma, refuse, vermin and item wear (DF v50; spec:
//! docs/mechanics/decay15.md; wiki Miasma, Food, Corpse, Remains, Bone, Shell, Skin, Tanner,
//! Refuse, Standing orders, Vermin, Cat, Wear).
//!
//! - **Rot timers** (`Decay::rot`, item id -> days of exposure and stage). Food kinds (meat,
//!   fish, meals, cheese, plants) count only days they are exposed: a stockpile, the trade
//!   depot, a minecart, being carried, in a job or a building protect them; raw fish never.
//!   Corpses, body parts, hides and vermin remains rot "just over time" (w:Miasma). Only
//!   materials that ROT decay (raws `[ROTS]`): teeth, chitin, bone, shell... never do.
//! - **Stages**: food rots (12 days, ESTIMATE from the export) and rots away 28 days later;
//!   plants wither. Corpses: rotten at 14 days, skeleton 56 days later. Body parts rot to
//!   bones or a skull, hides and remains vanish, shelled raw fish leave a shell.
//! - **Miasma**: rotten items of GENERATES_MIASMA materials lying loose on Subterranean tiles
//!   are sources; it spreads 2 tiles orthogonally and up/down, not through closed doors or
//!   hatches (a door is open while a creature stands in it), never into 7/7 water, and
//!   lingers 2 days after the source is gone. `lifestyle::miasma_day` takes the set.
//! - **Refuse**: animal corpses, body parts, bones, skins, rotten food and tattered (XX)
//!   clothing become refuse (refuse hauling, refuse piles). Standing orders: gather refuse,
//!   gather outdoor refuse (off by default), dump per type.
//! - **Vermin**: species from the creature raws (VERMIN_GROUNDER/EATER/ROTTER); they spawn
//!   near food and refuse piles, at rotting items and in discovered caverns, live 14 days,
//!   and eaters eat one unit of food a day from containers they can get into
//!   (PENETRATEPOWER against the container material). Cats (HUNTS_VERMIN) and tame falcons
//!   (DIVE_HUNTS_VERMIN) kill vermin within 2 tiles and leave remains (cats bring them to
//!   their owner). Dogs do not hunt vermin.
//! - **Wear**: 806 400 points per level, destroyed after 4 levels, value 100/75/50/25 %
//!   (w:Wear). Clothing, armor, bones and shells on refuse piles wear out (ESTIMATE rates).
//!
//! Everything here runs serially in fixed (id/slot) order with RNG streams from `Rng::for_unit`
//! (salts 0xDEC0..), so the result is the same with any number of threads.

use crate::decay15_api::{CorpseStage, RefuseOrders, VerminInfo};
use crate::items::Cat;
use crate::rng::{splitmix, Rng};
use crate::{Kind, Sim};
use rf_core::{liquid, tile_flags, TICKS_PER_DAY};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

const DAY: u64 = TICKS_PER_DAY as u64;
/// `decay_day` runs at `now % DAY == DECAY_DAY_PHASE` (before the lifestyle miasma at 637).
pub(crate) const DECAY_DAY_PHASE: u64 = 531;
/// `decay_pass` (vermin eating, hunters) runs at `now % DECAY_PERIOD == DECAY_PHASE`.
pub(crate) const DECAY_PERIOD: u64 = 100;
pub(crate) const DECAY_PHASE: u64 = 91;

/// Food rots after this many days of exposure (ESTIMATE: export meals rotten at 12 days).
pub const FOOD_ROT_DAYS: u16 = 12;
/// Rotten food and fish rot away (are destroyed) this many days later (ESTIMATE).
pub const ROT_AWAY_DAYS: u16 = 28;
/// Corpse timers (ESTIMATE, export section 1; same values as `lifestyle::CORPSE_*`).
pub const CORPSE_ROT_DAYS: u16 = crate::lifestyle::CORPSE_ROT_DAYS;
pub const CORPSE_SKELETON_DAYS: u16 = crate::lifestyle::CORPSE_MIASMA_DAYS;
/// Body parts and hides: rotten after 14 days, gone 28 days later (ESTIMATE).
pub const PART_ROT_DAYS: u16 = 14;
pub const PART_GONE_DAYS: u16 = 28;
/// Vermin remains: rotten after 14 days, gone 14 days later (export: bat remains 17-31 days).
pub const REMAINS_ROT_DAYS: u16 = 14;
pub const REMAINS_GONE_DAYS: u16 = 14;
/// Miasma reach and how long a cloud lingers without a source (ESTIMATE).
pub const MIASMA_RANGE: u32 = 2;
pub const MIASMA_LINGER_DAYS: u8 = 2;
/// Wear points per level (w:Wear) and the value multiplier per level.
pub const WEAR_LEVEL_PTS: u32 = 806_400;
pub const WEAR_VALUE_PCT: [i32; 4] = [100, 75, 50, 25];
/// Refuse-pile wear per day: clothing/armor one level in 56 days, bones/shells in 336 (ESTIMATE).
pub const REFUSE_WEAR_CLOTHES_DAY: u32 = WEAR_LEVEL_PTS / 56;
pub const REFUSE_WEAR_BONES_DAY: u32 = WEAR_LEVEL_PTS / 336;
/// Vermin: live cap, life span, spawn chances per site and day (all ESTIMATE).
pub const MAX_VERMIN: usize = 24;
pub const VERMIN_LIFE_DAYS: u64 = 14;
const SPAWN_PILE_DEN: u32 = 40;
const SPAWN_ROT_DEN: u32 = 20;
const SPAWN_CAVERN_DEN: u32 = 10;
/// A hunter kills a vermin within this Chebyshev distance with chance 1/2 per pass (ESTIMATE).
const HUNT_RANGE: i32 = 2;

// Rot classes.
const C_FOOD: u8 = 1;
const C_FISH_RAW: u8 = 2;
const C_PLANT: u8 = 3;
const C_CORPSE: u8 = 4;
const C_PART: u8 = 5;
const C_HIDE: u8 = 6;
const C_REMAINS: u8 = 7;

// Vermin kinds.
const V_EATER: u8 = 0;
const V_ROTTER: u8 = 1;
const V_ROAMER: u8 = 2;

/// Rot state of one item.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub(crate) struct RotState {
    pub days: u16,
    /// 0 fresh, 1 rotten.
    pub stage: u8,
    pub class: u8,
}

/// A vermin creature (not a unit: no AI slot, no pathing).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Vermin {
    pub id: u32,
    /// Creature token.
    pub race: String,
    pub tile: u32,
    pub born: u64,
    pub next_meal: u64,
    pub kind: u8,
}

/// A vermin species from the creature raws (cache).
#[derive(Clone, Debug, Default)]
pub(crate) struct VSp {
    pub token: String,
    pub name: String,
    pub eater: bool,
    pub rotter: bool,
    pub hateable: bool,
    pub pen: u8,
    pub cavern: bool,
    pub surface: bool,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Decay {
    /// None = on (`Sim::set_decay_enabled`).
    pub enabled: Option<bool>,
    pub rot: BTreeMap<i32, RotState>,
    /// Items with id >= this have not been classified yet.
    pub scanned_to: i32,
    pub full_scan_done: bool,
    pub vermin: Vec<Vermin>,
    pub next_vermin_id: u32,
    /// Item id -> wear points below the next level (refuse-pile wear candidates).
    pub wear_pts: BTreeMap<i32, u32>,
    /// Miasma cloud: tile -> days left.
    pub cloud: BTreeMap<u32, u8>,
    pub orders: RefuseOrders,
    pub rep: crate::decay15_api::DecayReport,
    #[serde(skip)]
    pub depot_tiles: Vec<u32>,
    #[serde(skip)]
    pub door_tiles: Vec<u32>,
    #[serde(skip)]
    pub occupied: Vec<u32>,
    /// Sources of this day's miasma (sorted; saved: lifestyle reads them later the same day).
    pub miasma_src: Vec<u32>,
    /// Tiles with a rotten item (any, also outdoors): rotter vermin sites.
    #[serde(skip)]
    pub rot_tiles: Vec<u32>,
    #[serde(skip)]
    pub species: Vec<VSp>,
    #[serde(skip)]
    pub species_ready: bool,
    /// Per animal species index: hunts vermin (HUNTS_VERMIN), dive hunts (tame only), returns kills.
    #[serde(skip)]
    pub hunter_sp: Vec<(bool, bool, bool)>,
    #[serde(skip)]
    tr: Vec<(i32, u8)>,
    #[serde(skip)]
    drop: Vec<i32>,
    #[serde(skip)]
    bfs_set: HashSet<u32>,
    #[serde(skip)]
    bfs_a: Vec<u32>,
    #[serde(skip)]
    bfs_b: Vec<u32>,
    #[serde(skip)]
    reached: Vec<u32>,
}

/// Material segment (last part of the token) that never rots (raws, material templates).
fn seg_never_rots(seg: &str) -> bool {
    matches!(
        seg,
        "BONE" | "CARTILAGE" | "HAIR" | "WOOL" | "FEATHER" | "SCALE" | "NAIL" | "TOOTH" | "IVORY" | "HORN" | "HOOF" | "PEARL"
            | "SHELL" | "CHITIN" | "LEATHER" | "PARCHMENT" | "SILK" | "CLAW" | "TALON" | "SEED" | "MILL" | "DRINK" | "SOAP"
    )
}

/// Description words of body parts that do not rot.
fn desc_never_rots(d: &str) -> bool {
    ["tooth", "teeth", "chitin", "bone", "skull", "shell", "horn", "hoof", "nail", "hair", "wool", "feather", "scale", "claw", "talon", "ivory"]
        .iter()
        .any(|w| d.contains(w))
}

fn limits(class: u8, id: i32) -> (u16, u16) {
    match class {
        C_FOOD | C_PLANT => (FOOD_ROT_DAYS + (splitmix(id as u64) % 4) as u16, ROT_AWAY_DAYS),
        C_FISH_RAW => (FOOD_ROT_DAYS, ROT_AWAY_DAYS),
        C_CORPSE => (CORPSE_ROT_DAYS, CORPSE_SKELETON_DAYS),
        C_PART | C_HIDE => (PART_ROT_DAYS, PART_GONE_DAYS),
        C_REMAINS => (REMAINS_ROT_DAYS, REMAINS_GONE_DAYS),
        _ => (u16::MAX / 2, u16::MAX / 2),
    }
}

/// Rotten items of these classes give off miasma (GENERATES_MIASMA materials: flesh, organs,
/// meals, cheese; not skin, plants or vermin remains).
fn makes_miasma(class: u8) -> bool {
    matches!(class, C_FOOD | C_FISH_RAW | C_CORPSE | C_PART)
}

fn race_of(token: &str) -> Option<&str> {
    let mut it = token.split(':');
    (it.next() == Some("CREATURE")).then(|| it.next()).flatten()
}

/// Vermin penetration power (raws `PENETRATEPOWER`, vanilla creature files; default 2).
fn penetrate_power(token: &str) -> u8 {
    match token {
        "LIZARD" | "WAMBLER_FLUFFY" => 1,
        "RAT_DEMON" => 3,
        _ => 2,
    }
}

/// Raws flags for vermin and hunters that the exporter may not carry (vanilla creature files).
fn raw_vermin_flags(token: &str) -> (bool, bool) {
    // (VERMIN_EATER, VERMIN_ROTTER)
    match token {
        "RAT" | "HAMSTER" | "LIZARD" | "ROACH_LARGE" | "RAT_DEMON" | "WAMBLER_FLUFFY" | "LIZARD_RHINO_TWO_LEGGED" => (true, false),
        "FLY" | "FLY_ACORN" | "GNAT_BLOOD" | "CREEPY_CRAWLER" | "WORM_KNUCKLE" => (false, true),
        _ => (false, false),
    }
}

impl Sim {
    pub(crate) fn decay_on(&self) -> bool {
        self.decay.enabled.unwrap_or(true)
    }

    /// Part of `state_hash`; mixes nothing while the module is off.
    pub(crate) fn decay_hash(&self, mix: &mut dyn FnMut(u64)) {
        if !self.decay_on() {
            return;
        }
        let d = &self.decay;
        mix(d.rot.len() as u64);
        for (&id, st) in &d.rot {
            mix((id as u64) << 24 ^ (st.days as u64) << 8 ^ st.stage as u64);
        }
        mix(d.vermin.len() as u64);
        for v in &d.vermin {
            mix((v.id as u64) << 32 ^ v.tile as u64);
        }
        for (&id, &p) in &d.wear_pts {
            mix((id as u64) << 32 ^ p as u64);
        }
        mix(d.cloud.len() as u64);
        let r = &d.rep;
        for v in [r.rotted_food, r.withered, r.rotted_away, r.corpses_rotted, r.skeletons, r.parts_to_bones, r.to_refuse, r.vermin_spawned, r.vermin_ate, r.vermin_killed, r.items_worn_out] {
            mix(v);
        }
    }

    /// [saves11] Rebuild the caches after an exact load (they are also rebuilt lazily).
    pub(crate) fn decay_after_load(&mut self) {
        self.decay.species_ready = false;
        self.decay.hunter_sp.clear();
        self.decay.depot_tiles.clear();
        self.decay.door_tiles.clear();
        self.decay.occupied.clear();
        self.decay.rot_tiles.clear();
    }

    // ------------------------------------------------------------------ daily pass

    /// Daily decay step (hook in `tick_inner`).
    pub(crate) fn decay_day(&mut self, now: u64) {
        if !self.decay_on() {
            return;
        }
        self.decay_refresh_tiles();
        self.decay_scan();
        self.decay_rot_step(now);
        let day = now / DAY;
        if day % 7 == 0 {
            self.decay_clothing_scan();
        }
        self.decay_dump_orders();
        self.decay_refuse_wear(now);
        self.decay_miasma_build();
        self.vermin_day(now);
        self.hunter_patrol(now);
    }

    fn decay_refresh_tiles(&mut self) {
        let mut depot = std::mem::take(&mut self.decay.depot_tiles);
        let mut doors = std::mem::take(&mut self.decay.door_tiles);
        depot.clear();
        doors.clear();
        for b in &self.world.buildings {
            if !b.built {
                continue;
            }
            let door = matches!(b.kind.as_str(), "Door" | "Hatch");
            if b.kind != "TradeDepot" && !door {
                continue;
            }
            for y in b.y1..=b.y2 {
                for x in b.x1..=b.x2 {
                    if let Some(t) = self.world.idx(rf_core::Pos::new(x, y, b.z)) {
                        if door {
                            doors.push(t as u32);
                        } else {
                            depot.push(t as u32);
                        }
                    }
                }
            }
        }
        depot.sort_unstable();
        depot.dedup();
        doors.sort_unstable();
        doors.dedup();
        self.decay.depot_tiles = depot;
        self.decay.door_tiles = doors;
        let mut occ = std::mem::take(&mut self.decay.occupied);
        occ.clear();
        occ.extend(self.ai.iter().filter(|a| a.kind != Kind::Dead).map(|a| a.pos));
        occ.sort_unstable();
        occ.dedup();
        self.decay.occupied = occ;
    }

    /// Rot class of item `idx` (0 = does not rot).
    fn decay_class(&self, idx: usize) -> u8 {
        let it = &self.world.items[idx];
        let tok = self.world.material(it.mat).map(|m| m.token.as_str()).unwrap_or("");
        let seg = tok.rsplit(':').next().unwrap_or("");
        match it.kind.as_str() {
            "MEAT" | "FISH" | "FOOD" | "CHEESE" => C_FOOD,
            "FISH_RAW" => C_FISH_RAW,
            "PLANT" | "PLANT_GROWTH" => C_PLANT,
            "CORPSE" => {
                let d = it.desc.to_ascii_lowercase();
                if d.contains("skeleton") || desc_never_rots(&d) || seg_never_rots(seg) {
                    0
                } else {
                    C_CORPSE
                }
            }
            "CORPSEPIECE" => {
                let d = it.desc.to_ascii_lowercase();
                if d.contains("skeleton") || desc_never_rots(&d) || seg_never_rots(seg) {
                    0
                } else if seg == "SKIN" || d.contains("skin") || d.contains("hide") {
                    C_HIDE
                } else {
                    C_PART
                }
            }
            "REMAINS" => {
                // ESTIMATE (spec M2.5): remains rot when the species has hair (export: only
                // bat remains rotted; olm, cap hopper and cave swallow remains stayed fresh).
                match race_of(tok) {
                    Some(r) if self.decay_race_has(r, "HAIR") => C_REMAINS,
                    _ => 0,
                }
            }
            _ => 0,
        }
    }

    fn decay_mat_index(&self, token: &str) -> Option<u16> {
        self.world.materials.iter().position(|m| m.token == token).map(|i| i as u16)
    }

    fn decay_race_has(&self, race: &str, seg: &str) -> bool {
        let n = race.len();
        self.world.materials.iter().any(|m| {
            let t = m.token.as_str();
            t.len() == 9 + n + 1 + seg.len() && t.starts_with("CREATURE:") && &t[9..9 + n] == race && t.ends_with(seg) && t.as_bytes()[9 + n] == b':'
        })
    }

    fn decay_race_name(&self, race: &str) -> String {
        self.world
            .creature_raws
            .iter()
            .find(|c| c.token == race)
            .map(|c| c.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| race.to_ascii_lowercase().replace('_', " "))
    }

    /// Classify items not seen yet (a full scan the first time, then the new tail).
    fn decay_scan(&mut self) {
        let full = !self.decay.full_scan_done;
        let from = self.decay.scanned_to;
        let mut new_ids: Vec<usize> = Vec::new();
        if full {
            new_ids.extend(0..self.world.items.len());
        } else {
            let mut i = self.world.items.len();
            while i > 0 {
                i -= 1;
                if self.world.items[i].id < from {
                    break;
                }
                new_ids.push(i);
            }
            new_ids.reverse();
        }
        // Old saves: the storage rot timers move here.
        let mut old = std::mem::take(&mut self.store.rot);
        for idx in new_ids {
            if self.imeta[idx].dead {
                continue;
            }
            let class = self.decay_class(idx);
            let it = &self.world.items[idx];
            let id = it.id;
            if class != 0 && !self.decay.rot.contains_key(&id) {
                let (a, _) = limits(class, id);
                let mut st = RotState { days: 0, stage: 0, class };
                if it.flags.rotten {
                    st.stage = 1;
                    st.days = a;
                } else if let Some(d) = old.remove(&id) {
                    st.days = d.min(a.saturating_sub(1));
                } else if full && matches!(class, C_CORPSE | C_PART | C_HIDE | C_REMAINS) {
                    // ESTIMATE: imported corpses have an unknown age; spread them.
                    st.days = (splitmix(id as u64 ^ 0xDEC0) % a as u64) as u16;
                }
                self.decay.rot.insert(id, st);
            } else if class == 0 && it.kind == "CORPSEPIECE" && !self.decay.wear_pts.contains_key(&id) {
                // Bones, skulls, shells: refuse-pile wear candidates.
                self.decay.wear_pts.insert(id, 0);
            }
            self.decay_refuse_retag(idx);
        }
        old.clear();
        self.store.rot = old;
        self.decay.full_scan_done = true;
        self.decay.scanned_to = self.next_item_id;
    }

    /// Is this a corpse/part of a sapient (or named) creature (corpse pile) rather than refuse?
    fn decay_sapient_remains(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        if !it.name.is_empty() || it.desc.contains("'s ") {
            return true;
        }
        let tok = self.world.material(it.mat).map(|m| m.token.as_str()).unwrap_or("");
        let Some(race) = race_of(tok) else { return false };
        self.world
            .creature_raws
            .iter()
            .find(|c| c.token == race)
            .map(|c| c.castes.iter().any(|k| k.flags.iter().any(|f| f == "CAN_LEARN" || f == "CAN_SPEAK")))
            .unwrap_or(false)
    }

    /// Move an item to the refuse category where DF's refuse rules say so (w:Refuse).
    fn decay_refuse_retag(&mut self, idx: usize) {
        let cat = self.imeta[idx].cat;
        let it = &self.world.items[idx];
        let to_refuse = match it.kind.as_str() {
            "CORPSE" | "CORPSEPIECE" => cat == Cat::Corpses && !self.decay_sapient_remains(idx),
            _ => it.flags.rotten && cat == Cat::Food,
        };
        if to_refuse {
            self.set_item_cat(idx, Cat::Refuse);
            self.decay.rep.to_refuse += 1;
        }
    }

    /// Protected from food rot (w:Miasma): stockpile, trade depot, minecart, carried, in a job
    /// or building; containers alone do not protect; raw fish never.
    fn decay_rot_safe(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        if it.kind == "FISH_RAW" {
            return false;
        }
        if it.holder >= 0 || it.flags.in_job || it.flags.in_building || it.building >= 0 || self.imeta[idx].hauling || self.imeta[idx].reserved > 0 {
            return true;
        }
        let pos = if it.container >= 0 {
            match self.item_idx.get(&it.container) {
                Some(&ci) => {
                    let c = &self.world.items[ci as usize];
                    if c.holder >= 0 || c.building >= 0 || c.flags.in_building || crate::minecarts::is_minecart(&c.kind, &c.subtype) {
                        return true;
                    }
                    c.pos
                }
                None => it.pos,
            }
        } else {
            it.pos
        };
        match self.world.idx(pos) {
            Some(t) => self.pile_at.contains_key(&(t as u32)) || self.decay.depot_tiles.binary_search(&(t as u32)).is_ok(),
            None => true,
        }
    }

    /// Tile where a rotten item gives off miasma (loose, Subterranean, not under 7/7 water).
    fn decay_miasma_tile(&self, idx: usize) -> Option<u32> {
        let it = &self.world.items[idx];
        if !it.flags.on_ground || it.container >= 0 || it.holder >= 0 || it.flags.in_building || it.building >= 0 {
            return None;
        }
        let t = self.world.idx(it.pos)?;
        let tile = &self.world.tiles[t];
        if tile.flags & tile_flags::SUBTERRANEAN == 0 || tile.liquid & liquid::DEPTH_MASK >= 7 {
            return None;
        }
        Some(t as u32)
    }

    fn decay_rot_step(&mut self, now: u64) {
        let mut rot = std::mem::take(&mut self.decay.rot);
        let mut tr = std::mem::take(&mut self.decay.tr);
        let mut drop = std::mem::take(&mut self.decay.drop);
        let mut src = std::mem::take(&mut self.decay.miasma_src);
        let mut rt = std::mem::take(&mut self.decay.rot_tiles);
        tr.clear();
        drop.clear();
        src.clear();
        rt.clear();
        for (&id, st) in rot.iter_mut() {
            let Some(&i) = self.item_idx.get(&id) else {
                drop.push(id);
                continue;
            };
            let i = i as usize;
            if self.imeta[i].dead {
                drop.push(id);
                continue;
            }
            let exposed = match st.class {
                C_FOOD | C_PLANT | C_FISH_RAW => !self.decay_rot_safe(i),
                _ => true,
            };
            if exposed {
                st.days = st.days.saturating_add(1);
            }
            let (a, b) = limits(st.class, id);
            if st.stage == 0 && st.days >= a {
                st.stage = 1;
                tr.push((id, 0));
            } else if st.stage == 1 && st.days >= a.saturating_add(b) {
                tr.push((id, 1));
            }
            if st.stage == 1 {
                let it = &self.world.items[i];
                if it.flags.on_ground && it.container < 0 {
                    if let Some(t) = self.world.idx(it.pos) {
                        rt.push(t as u32);
                    }
                }
                if makes_miasma(st.class) {
                    if let Some(t) = self.decay_miasma_tile(i) {
                        src.push(t);
                    }
                }
            }
        }
        for &id in &drop {
            rot.remove(&id);
        }
        self.decay.rot = rot;
        src.sort_unstable();
        src.dedup();
        rt.sort_unstable();
        rt.dedup();
        self.decay.miasma_src = src;
        self.decay.rot_tiles = rt;
        let mut gone_food: Vec<i32> = Vec::new();
        for k in 0..tr.len() {
            let (id, t) = tr[k];
            let Some(st) = self.decay.rot.get(&id).copied() else { continue };
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            if t == 0 {
                self.decay_go_rotten(i, st.class);
                if matches!(st.class, C_FOOD | C_PLANT | C_FISH_RAW) {
                    gone_food.push(id);
                }
            } else {
                self.decay_go_final(i, id, st.class, now);
            }
        }
        if !gone_food.is_empty() {
            gone_food.sort_unstable();
            self.foods.retain(|x| gone_food.binary_search(x).is_err());
        }
        tr.clear();
        self.decay.tr = tr;
        self.decay.drop = drop;
    }

    fn decay_go_rotten(&mut self, i: usize, class: u8) {
        let it = &mut self.world.items[i];
        it.flags.rotten = true;
        match class {
            C_PLANT => {
                it.desc = format!("withered {}", it.desc);
                self.decay.rep.withered += 1;
            }
            C_FOOD | C_FISH_RAW => {
                it.desc = format!("rotten {}", it.desc);
                self.decay.rep.rotted_food += 1;
            }
            C_CORPSE => {
                it.desc = if it.desc.contains("'s corpse") { it.desc.replacen("'s corpse", "'s rotten corpse", 1) } else { format!("rotten {}", it.desc) };
                self.decay.rep.corpses_rotted += 1;
            }
            C_HIDE => {
                it.desc = format!("rotten {}", it.desc);
                self.decay.rep.hides_rotted += 1;
            }
            C_REMAINS => {
                it.desc = format!("rotten {}", it.desc);
                self.decay.rep.remains_rotted += 1;
            }
            _ => {
                it.desc = format!("rotten {}", it.desc);
            }
        }
        self.decay_refuse_retag(i);
    }

    fn decay_go_final(&mut self, i: usize, id: i32, class: u8, _now: u64) {
        if class == C_CORPSE {
            // Rotten corpse -> skeleton (final; undead can still raise it).
            let it = &mut self.world.items[i];
            it.flags.rotten = false;
            let d = it.desc.replace("rotten ", "");
            it.desc = if d.contains("mutilated corpse") {
                d.replacen("mutilated corpse", "partial skeleton", 1)
            } else if d.contains("corpse") {
                d.replacen("corpse", "skeleton", 1)
            } else {
                format!("{d} skeleton")
            };
            self.decay.rot.remove(&id);
            self.decay.rep.skeletons += 1;
            return;
        }
        if self.imeta[i].reserved > 0 {
            return; // a job or an undead claim holds it: retry tomorrow
        }
        let it = &self.world.items[i];
        let tok = self.world.material(it.mat).map(|m| m.token.clone()).unwrap_or_default();
        let race = race_of(&tok).map(|r| r.to_string());
        let tile = self.world.idx(it.pos);
        let d = it.desc.to_ascii_lowercase();
        let in_cont = it.container >= 0 || it.holder >= 0;
        match class {
            C_PART => {
                // Bones or a skull stay behind (w:Bone, w:Corpse).
                if let (Some(r), Some(t), false) = (race.as_deref(), tile, in_cont) {
                    if let Some(m) = self.decay_mat_index(&format!("CREATURE:{r}:BONE")) {
                        let name = self.decay_race_name(r);
                        let (desc, n) = if d.contains("head") {
                            (format!("{name} skull"), 1)
                        } else if d.contains("upper body") || d.contains("lower body") {
                            (format!("{name} bones"), 3)
                        } else if d.contains("arm") || d.contains("leg") || d.contains("tail") {
                            (format!("{name} bones"), 2)
                        } else {
                            (format!("{name} bone"), 1)
                        };
                        let nid = self.create_item("CORPSEPIECE", m, t, desc);
                        if let Some(&ni) = self.item_idx.get(&nid) {
                            self.world.items[ni as usize].stack = n;
                            self.set_item_cat(ni as usize, Cat::Refuse);
                        }
                        self.decay.wear_pts.insert(nid, 0);
                        self.decay.rep.parts_to_bones += 1;
                    }
                }
            }
            C_FISH_RAW => {
                // Shelled vermin fish leave a shell "if left to rot long enough" (w:Shell).
                if let (Some(r), Some(t), false) = (race.as_deref(), tile, in_cont) {
                    if let Some(m) = self.decay_mat_index(&format!("CREATURE:{r}:SHELL")) {
                        let name = self.decay_race_name(r);
                        let nid = self.create_item("CORPSEPIECE", m, t, format!("{name} shell"));
                        if let Some(&ni) = self.item_idx.get(&nid) {
                            self.set_item_cat(ni as usize, Cat::Refuse);
                        }
                        self.decay.wear_pts.insert(nid, 0);
                        self.decay.rep.shells_from_fish += 1;
                    }
                }
            }
            _ => {}
        }
        self.decay.rot.remove(&id);
        self.destroy_item(id);
        self.decay.rep.rotted_away += 1;
    }

    // ------------------------------------------------------------------ refuse

    /// Unworn tattered (XX) clothing and armor goes to the refuse pile (w:Stockpile).
    fn decay_clothing_scan(&mut self) {
        let mut v: Vec<usize> = Vec::new();
        for (i, it) in self.world.items.iter().enumerate() {
            if it.wear >= 3
                && matches!(it.kind.as_str(), "ARMOR" | "PANTS" | "SHOES" | "GLOVES" | "HELM" | "SHIELD")
                && it.holder < 0
                && it.container < 0
                && !self.imeta[i].dead
                && self.imeta[i].cat == Cat::Armor
            {
                v.push(i);
            }
        }
        for i in v {
            let id = self.world.items[i].id;
            self.set_item_cat(i, Cat::Refuse);
            self.decay.wear_pts.entry(id).or_insert(0);
            self.decay.rep.to_refuse += 1;
        }
    }

    /// Refuse may be hauled (standing orders: gather refuse, gather outdoor refuse).
    pub(crate) fn decay_haul_ok(&self, idx: usize) -> bool {
        if !self.decay_on() || self.imeta[idx].cat != Cat::Refuse {
            return true;
        }
        let o = &self.decay.orders;
        if !o.gather {
            return false;
        }
        if o.outdoor && !(o.ignore_outdoor_remains && self.world.items[idx].kind == "REMAINS") {
            return true;
        }
        match self.world.idx(self.world.items[idx].pos) {
            Some(t) => self.world.tiles[t].flags & tile_flags::OUTSIDE == 0,
            None => true,
        }
    }

    pub(crate) fn set_refuse_orders_impl(&mut self, o: RefuseOrders) {
        self.decay.orders = o;
        self.rebuild_haul();
    }

    /// Which dump order covers this refuse item.
    fn decay_dump_wanted(&self, idx: usize) -> bool {
        let o = &self.decay.orders;
        let it = &self.world.items[idx];
        let d = it.desc.to_ascii_lowercase();
        match it.kind.as_str() {
            "CORPSE" => o.dump_corpses,
            "CORPSEPIECE" => {
                if d.contains("skull") {
                    o.dump_skulls
                } else if d.contains("bone") {
                    o.dump_bones
                } else if d.contains("shell") {
                    o.dump_shells
                } else if d.contains("skin") || d.contains("hide") || d.contains("chitin") || d.contains("scale") {
                    o.dump_skins
                } else if d.contains("hair") || d.contains("wool") {
                    o.dump_hair
                } else {
                    o.dump_corpses
                }
            }
            _ => o.dump_other,
        }
    }

    fn decay_dump_orders(&mut self) {
        let o = self.decay.orders;
        if !(o.dump_corpses || o.dump_skulls || o.dump_bones || o.dump_shells || o.dump_skins || o.dump_hair || o.dump_other) {
            return;
        }
        if !self.world.buildings.iter().any(|b| b.kind == "Civzone" && b.subtype == "Dump") {
            return;
        }
        let mut v: Vec<i32> = Vec::new();
        for (i, it) in self.world.items.iter().enumerate() {
            if self.imeta[i].cat == Cat::Refuse && !it.flags.dump && !it.flags.forbid && it.holder < 0 && it.container < 0 && !self.imeta[i].dead && self.imeta[i].reserved == 0 && self.decay_dump_wanted(i) {
                v.push(it.id);
            }
        }
        for id in v {
            self.cx_set_item_flag(id, "dump", true);
            self.decay.rep.dump_marked += 1;
        }
    }

    // ------------------------------------------------------------------ wear

    /// Set an item's wear level and scale its value (w:Wear: 100/75/50/25 %).
    pub(crate) fn decay_apply_wear_level(&mut self, idx: usize, old: u8, new: u8) {
        let it = &mut self.world.items[idx];
        it.wear = new;
        let (o, n) = (old.min(3) as usize, new.min(3) as usize);
        if o != n {
            it.value = it.value * WEAR_VALUE_PCT[n] / WEAR_VALUE_PCT[o];
        }
    }

    /// Clothing, armor, bones and shells lying on refuse piles wear out (w:Wear, w:Bone).
    fn decay_refuse_wear(&mut self, _now: u64) {
        let mut pts = std::mem::take(&mut self.decay.wear_pts);
        let mut gone: Vec<i32> = Vec::new();
        let mut up: Vec<(usize, u8)> = Vec::new();
        pts.retain(|&id, p| {
            let Some(&i) = self.item_idx.get(&id) else { return false };
            let i = i as usize;
            if self.imeta[i].dead {
                return false;
            }
            let it = &self.world.items[i];
            if it.flags.artifact || it.quality >= 6 || it.holder >= 0 || it.container >= 0 {
                return true;
            }
            let on_refuse = self
                .world
                .idx(it.pos)
                .and_then(|t| self.pile_at.get(&(t as u32)))
                .map(|&(pi, _)| self.piles[pi as usize].cats & Cat::Refuse.bit() != 0)
                .unwrap_or(false);
            if !on_refuse {
                return true;
            }
            *p += if it.kind == "CORPSEPIECE" { REFUSE_WEAR_BONES_DAY } else { REFUSE_WEAR_CLOTHES_DAY };
            if *p >= WEAR_LEVEL_PTS {
                *p -= WEAR_LEVEL_PTS;
                if it.wear >= 3 {
                    gone.push(id);
                    return false;
                }
                up.push((i, it.wear));
            }
            true
        });
        self.decay.wear_pts = pts;
        for (i, w) in up {
            self.decay_apply_wear_level(i, w, w + 1);
            self.decay.rep.wear_levels_refuse += 1;
        }
        for id in gone {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            if self.imeta[i as usize].reserved > 0 {
                self.decay.wear_pts.insert(id, WEAR_LEVEL_PTS - 1);
                continue;
            }
            self.destroy_item(id);
            self.decay.rep.items_worn_out += 1;
        }
    }

    // ------------------------------------------------------------------ miasma

    fn decay_door_closed(&self, t: u32) -> bool {
        self.decay.door_tiles.binary_search(&t).is_ok() && self.decay.occupied.binary_search(&t).is_err()
    }

    /// Spread today's sources (BFS over orthogonal and vertical moves) and age the cloud.
    fn decay_miasma_build(&mut self) {
        let mut set = std::mem::take(&mut self.decay.bfs_set);
        let mut frontier = std::mem::take(&mut self.decay.bfs_a);
        let mut next = std::mem::take(&mut self.decay.bfs_b);
        let mut reached = std::mem::take(&mut self.decay.reached);
        set.clear();
        reached.clear();
        let mut nb = [0u32; crate::nav::MAX_NEIGHBORS];
        for k in 0..self.decay.miasma_src.len() {
            let src = self.decay.miasma_src[k];
            if !set.insert(src) {
                continue;
            }
            reached.push(src);
            frontier.clear();
            frontier.push(src);
            for _ in 0..MIASMA_RANGE {
                next.clear();
                for &t in &frontier {
                    let (x, y, z) = self.nav.xyz(t as usize);
                    let n = self.nav.neighbors(t as usize, &mut nb);
                    for &q in &nb[..n] {
                        let (qx, qy, qz) = self.nav.xyz(q as usize);
                        let ortho = qz == z && (qx.abs_diff(x) + qy.abs_diff(y)) == 1;
                        let vert = qx == x && qy == y && qz != z;
                        if !ortho && !vert {
                            continue;
                        }
                        let tile = &self.world.tiles[q as usize];
                        if tile.flags & tile_flags::OUTSIDE != 0 || tile.liquid & liquid::DEPTH_MASK >= 7 || self.decay_door_closed(q) {
                            continue;
                        }
                        if set.insert(q) {
                            reached.push(q);
                            next.push(q);
                        }
                    }
                }
                std::mem::swap(&mut frontier, &mut next);
            }
        }
        reached.sort_unstable();
        let cloud = &mut self.decay.cloud;
        cloud.retain(|_, d| {
            *d = d.saturating_sub(1);
            *d > 0
        });
        for &t in &reached {
            cloud.insert(t, MIASMA_LINGER_DAYS + 1);
        }
        let r = &mut self.decay.rep;
        r.miasma_sources_max = r.miasma_sources_max.max(self.decay.miasma_src.len());
        r.miasma_tiles_max = r.miasma_tiles_max.max(cloud.len());
        self.decay.bfs_set = set;
        self.decay.bfs_a = frontier;
        self.decay.bfs_b = next;
        self.decay.reached = reached;
    }

    /// The miasma set for `lifestyle::miasma_day` (sources, tiles).
    pub(crate) fn decay_miasma_set(&self) -> (usize, HashSet<u32>) {
        (self.decay.miasma_src.len(), self.decay.cloud.keys().copied().collect())
    }

    // ------------------------------------------------------------------ vermin

    fn vermin_species(&mut self) {
        if self.decay.species_ready {
            return;
        }
        self.decay.species_ready = true;
        let mut v: Vec<VSp> = Vec::new();
        for c in &self.world.creature_raws {
            let has = |f: &str| c.flags.iter().any(|x| x == f) || c.castes.iter().any(|k| k.flags.iter().any(|x| x == f));
            let (re, rr) = raw_vermin_flags(&c.token);
            let eater = has("VERMIN_EATER") || re;
            let rotter = has("VERMIN_ROTTER") || rr;
            let grounder = has("VERMIN_GROUNDER");
            if !(eater || rotter || grounder) {
                continue;
            }
            let cavern = c.flags.iter().any(|f| f.starts_with("BIOME_SUBTERRANEAN"));
            let surface = c.flags.iter().any(|f| f.starts_with("BIOME_") && !f.starts_with("BIOME_SUBTERRANEAN")) || !cavern;
            v.push(VSp { token: c.token.clone(), name: c.name.clone(), eater, rotter, hateable: has("VERMIN_HATEABLE"), pen: penetrate_power(&c.token), cavern, surface });
        }
        v.sort_by(|a, b| a.token.cmp(&b.token));
        self.decay.species = v;
    }

    /// Test hook: replace the vermin species pool.
    #[cfg(test)]
    pub(crate) fn set_vermin_species(&mut self, v: Vec<VSp>) {
        self.decay.species = v;
        self.decay.species_ready = true;
    }

    /// Pick a species: `want` 0 surface eater, 1 rotter, 2 cavern roamer, 3 any surface.
    fn vermin_pick(&self, want: u8, r: &mut Rng) -> Option<usize> {
        let ok = |s: &VSp| match want {
            0 => s.eater && s.surface,
            1 => s.rotter,
            2 => s.cavern,
            _ => s.surface && !s.rotter,
        };
        // At most 7 per list (sub-region lists, w:Creature token), in token order.
        let mut n = 0;
        let mut idx = [0usize; 7];
        for (k, s) in self.decay.species.iter().enumerate() {
            if ok(s) && n < 7 {
                idx[n] = k;
                n += 1;
            }
        }
        (n > 0).then(|| idx[r.below(n as u32) as usize])
    }

    fn vermin_spawn(&mut self, sp: usize, tile: u32, kind: u8, now: u64) {
        if self.decay.vermin.len() >= MAX_VERMIN {
            return;
        }
        let id = self.decay.next_vermin_id;
        self.decay.next_vermin_id += 1;
        let race = self.decay.species[sp].token.clone();
        self.decay.vermin.push(Vermin { id, race, tile, born: now, next_meal: now + DAY / 2, kind });
        self.decay.rep.vermin_spawned += 1;
    }

    /// Daily vermin spawning and leaving.
    fn vermin_day(&mut self, now: u64) {
        self.vermin_species();
        // Leave after their life span.
        let before = self.decay.vermin.len();
        self.decay.vermin.retain(|v| now < v.born + VERMIN_LIFE_DAYS * DAY);
        self.decay.rep.vermin_left += (before - self.decay.vermin.len()) as u64;
        if self.decay.species.is_empty() {
            return;
        }
        let mut r = Rng::for_unit(self.seed, 0, now, 0xDEC0);
        // (a) food and (b) refuse piles.
        for pi in 0..self.piles.len() {
            let cats = self.piles[pi].cats;
            let n = self.piles[pi].tiles.len();
            if n == 0 {
                continue;
            }
            let food = cats & Cat::Food.bit() != 0;
            let refuse = cats & Cat::Refuse.bit() != 0;
            if !(food || refuse) || !r.chance(1, SPAWN_PILE_DEN) {
                continue;
            }
            let want = if food { 0 } else { 3 };
            if let Some(sp) = self.vermin_pick(want, &mut r) {
                let t = self.piles[pi].tiles[r.below(n as u32) as usize];
                let kind = if self.decay.species[sp].eater { V_EATER } else { V_ROAMER };
                self.vermin_spawn(sp, t, kind, now);
            }
        }
        // (c) rotters at rotting items.
        for k in 0..self.decay.rot_tiles.len() {
            if !r.chance(1, SPAWN_ROT_DEN) {
                continue;
            }
            if let Some(sp) = self.vermin_pick(1, &mut r) {
                let t = self.decay.rot_tiles[k];
                self.vermin_spawn(sp, t, V_ROTTER, now);
            }
        }
        // (d) roamers in discovered caverns.
        for li in 0..self.caverns.layers.len() {
            let l = &self.caverns.layers[li];
            if l.magma || !l.discovered() || l.spawn.is_empty() || !r.chance(1, SPAWN_CAVERN_DEN) {
                continue;
            }
            let t = l.spawn[r.below(l.spawn.len() as u32) as usize];
            if let Some(sp) = self.vermin_pick(2, &mut r) {
                self.vermin_spawn(sp, t, V_ROAMER, now);
            }
        }
    }

    /// Protection of a container against vermin (ESTIMATE, spec M5.5): none 0; wood, plant,
    /// leather, cloth, bone 1; stone, ceramic, glass 2; metal 3.
    fn vermin_resist(&self, idx: usize) -> u8 {
        let it = &self.world.items[idx];
        if it.container < 0 {
            return 0;
        }
        let Some(&ci) = self.item_idx.get(&it.container) else { return 0 };
        let c = &self.world.items[ci as usize];
        match self.world.material(c.mat) {
            Some(m) if m.is_metal => 3,
            Some(m) if m.is_stone || m.is_glass || m.is_gem || m.token.contains("CLAY") || m.token.contains("CERAMIC") || m.token.contains("PORCELAIN") => 2,
            _ => 1,
        }
    }

    /// Vermin eating and hunters (every `DECAY_PERIOD` ticks).
    pub(crate) fn decay_pass(&mut self, now: u64) {
        if !self.decay_on() || self.decay.vermin.is_empty() {
            return;
        }
        self.vermin_species();
        self.vermin_eat(now);
        self.vermin_hunt(now);
    }

    fn vermin_sp(&self, race: &str) -> Option<&VSp> {
        self.decay.species.iter().find(|s| s.token == race)
    }

    fn vermin_eat(&mut self, now: u64) {
        for k in 0..self.decay.vermin.len() {
            let v = &self.decay.vermin[k];
            if v.kind != V_EATER || v.next_meal > now {
                continue;
            }
            let pen = self.vermin_sp(&v.race).map(|s| s.pen).unwrap_or(2);
            let (vx, vy, vz) = self.nav.xyz(v.tile as usize);
            let mut best: Option<i32> = None;
            for &fid in &self.foods {
                let Some(&i) = self.item_idx.get(&fid) else { continue };
                let i = i as usize;
                let it = &self.world.items[i];
                if self.imeta[i].dead || it.flags.rotten || it.holder >= 0 || self.imeta[i].reserved > 0 || it.flags.in_job {
                    continue;
                }
                let pos = if it.container >= 0 { self.item_idx.get(&it.container).map(|&c| self.world.items[c as usize].pos).unwrap_or(it.pos) } else { it.pos };
                if pos.z as usize != vz || (pos.x as i32 - vx as i32).abs() > 4 || (pos.y as i32 - vy as i32).abs() > 4 {
                    continue;
                }
                if self.vermin_resist(i) >= pen {
                    continue;
                }
                if best.map(|b| fid < b).unwrap_or(true) {
                    best = Some(fid);
                }
            }
            self.decay.vermin[k].next_meal = now + DAY;
            let Some(fid) = best else { continue };
            let i = self.item_idx[&fid] as usize;
            if self.world.items[i].stack > 1 {
                self.world.items[i].stack -= 1;
            } else {
                self.destroy_item(fid);
                self.foods.retain(|&x| x != fid);
            }
            self.decay.rep.vermin_ate += 1;
        }
    }

    fn hunter_flags(&mut self, sp: usize) -> (bool, bool, bool) {
        if self.decay.hunter_sp.len() != self.animals.species.len() {
            let mut v = Vec::with_capacity(self.animals.species.len());
            for s in &self.animals.species {
                let raw = self.world.creature_raws.iter().find(|c| c.token == s.token);
                let has = |f: &str| raw.map(|c| c.flags.iter().any(|x| x == f) || c.castes.iter().any(|k| k.flags.iter().any(|x| x == f))).unwrap_or(false);
                v.push((s.vermin || has("HUNTS_VERMIN"), has("DIVE_HUNTS_VERMIN"), has("RETURNS_VERMIN_KILLS_TO_OWNER") || s.vermin));
            }
            self.decay.hunter_sp = v;
        }
        self.decay.hunter_sp.get(sp).copied().unwrap_or((false, false, false))
    }

    fn vermin_hunt(&mut self, now: u64) {
        for s in 0..self.ai.len() {
            if self.decay.vermin.is_empty() {
                return;
            }
            if self.ai[s].kind != Kind::Animal || s >= self.animals.st.len() {
                continue;
            }
            let sp = self.animals.st[s].sp as usize;
            if sp >= self.animals.species.len() {
                continue;
            }
            let (hunts, dive, returns) = self.hunter_flags(sp);
            let uidx = self.ai[s].uidx;
            let tame = self.world.units[uidx].flags.tame;
            if !(hunts || dive && tame) || self.world.units[uidx].age < 1.0 {
                continue;
            }
            let (hx, hy, hz) = self.nav.xyz(self.ai[s].pos as usize);
            let mut r = Rng::for_unit(self.seed, s, now, 0xDEC2);
            let mut k = 0;
            while k < self.decay.vermin.len() {
                let (x, y, z) = self.nav.xyz(self.decay.vermin[k].tile as usize);
                if z != hz || (x as i32 - hx as i32).abs() > HUNT_RANGE || (y as i32 - hy as i32).abs() > HUNT_RANGE || !r.chance(1, 2) {
                    k += 1;
                    continue;
                }
                let v = self.decay.vermin.remove(k);
                self.decay.rep.vermin_killed += 1;
                self.animals.rep.vermin_killed += 1;
                let mut t = v.tile as usize;
                if returns {
                    let o = self.world.units[uidx].pet_owner;
                    if o > 0 {
                        if let Some(&os) = self.unit_slot.get(&o) {
                            if self.ai[os].kind == Kind::Worker {
                                t = self.ai[os].pos as usize;
                            }
                        }
                    }
                }
                let name = self.vermin_sp(&v.race).map(|s| s.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| v.race.to_ascii_lowercase());
                let m = self.decay_remains_mat(&v.race, &name);
                let id = self.create_item("REMAINS", m, t, format!("{name} remains"));
                self.decay.rep.remains_made += 1;
                let _ = id;
                break; // one kill per hunter and pass
            }
        }
    }

    /// Material `CREATURE:<race>:REMAINS` named "<name> remains" (as the animals module does).
    fn decay_remains_mat(&mut self, race: &str, name: &str) -> u16 {
        let tok = format!("CREATURE:{race}:REMAINS");
        if let Some(m) = self.decay_mat_index(&tok) {
            return m;
        }
        let m = self.ensure_material(&tok);
        self.world.materials[m as usize].name = format!("{name} remains");
        self.world.materials[m as usize].is_metal = false;
        m
    }

    /// Cats walk "randomly between places with food laying on the ground or in stockpiles"
    /// (w:Creature token HUNTS_VERMIN): an idle hunter goes to a vermin or a food pile tile
    /// with chance 1/4 a day (ESTIMATE).
    fn hunter_patrol(&mut self, now: u64) {
        let food_piles: Vec<usize> = (0..self.piles.len()).filter(|&p| self.piles[p].cats & Cat::Food.bit() != 0 && !self.piles[p].tiles.is_empty()).collect();
        if food_piles.is_empty() && self.decay.vermin.is_empty() {
            return;
        }
        for s in 0..self.ai.len() {
            if self.ai[s].kind != Kind::Animal || s >= self.animals.st.len() {
                continue;
            }
            let sp = self.animals.st[s].sp as usize;
            if sp >= self.animals.species.len() {
                continue;
            }
            let (hunts, dive, _) = self.hunter_flags(sp);
            let uidx = self.ai[s].uidx;
            let u = &self.world.units[uidx];
            if !(hunts || dive && u.flags.tame) || u.age < 1.0 || u.pasture > 0 || self.animals.st[s].held != crate::NONE {
                continue;
            }
            let a = &self.ai[s];
            if !matches!(a.task, crate::Task::Idle | crate::Task::Wander) || a.waiting {
                continue;
            }
            let mut r = Rng::for_unit(self.seed, s, now, 0xDEC3);
            if !r.chance(1, 4) {
                continue;
            }
            let here = self.nav.comp_root(a.pos as usize);
            let mut goal = None;
            if !self.decay.vermin.is_empty() {
                let v = &self.decay.vermin[r.below(self.decay.vermin.len() as u32) as usize];
                if self.nav.comp_root(v.tile as usize) == here {
                    goal = Some(v.tile);
                }
            }
            if goal.is_none() && !food_piles.is_empty() {
                let p = food_piles[r.below(food_piles.len() as u32) as usize];
                let tiles = &self.piles[p].tiles;
                let t = tiles[r.below(tiles.len() as u32) as usize];
                if self.nav.comp_root(t as usize) == here {
                    goal = Some(t);
                }
            }
            if let Some(t) = goal {
                self.ai[s].task = crate::Task::Wander;
                self.request_path(s, crate::path::Goals::one(t), now);
            }
        }
    }

    pub(crate) fn vermin_list_impl(&self) -> Vec<VerminInfo> {
        self.decay
            .vermin
            .iter()
            .map(|v| {
                let s = self.vermin_sp(&v.race);
                VerminInfo {
                    race: v.race.clone(),
                    name: s.map(|s| s.name.clone()).unwrap_or_default(),
                    pos: self.world.pos_of(v.tile as usize),
                    eater: v.kind == V_EATER,
                    rotter: v.kind == V_ROTTER,
                    hateable: s.map(|s| s.hateable).unwrap_or(false),
                }
            })
            .collect()
    }

    // ------------------------------------------------------------------ queries

    pub(crate) fn corpse_stage_impl(&self, item_id: i32) -> CorpseStage {
        let Some(&i) = self.item_idx.get(&item_id) else { return CorpseStage::NotRotting };
        let it = &self.world.items[i as usize];
        if it.kind != "CORPSE" {
            return CorpseStage::NotRotting;
        }
        if it.desc.contains("skeleton") {
            return CorpseStage::Skeleton;
        }
        match self.decay.rot.get(&item_id) {
            Some(st) if st.stage >= 1 => CorpseStage::Rotten,
            Some(_) => CorpseStage::Fresh,
            None if it.flags.rotten => CorpseStage::Rotten,
            None if self.decay_class(i as usize) == C_CORPSE => CorpseStage::Fresh,
            None => CorpseStage::NotRotting,
        }
    }

    pub(crate) fn rot_days_impl(&self, item_id: i32) -> Option<(u16, u16)> {
        let st = self.decay.rot.get(&item_id)?;
        let (a, b) = limits(st.class, item_id);
        Some((st.days, if st.stage == 0 { a } else { a.saturating_add(b) }))
    }
}

#[cfg(test)]
mod tests;
