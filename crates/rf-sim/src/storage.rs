//! Storage: bins, barrels, large pots and bags on stockpiles, and food rot.
//!
//! # DF rules implemented (wiki: Stockpile, Bin, Barrel, Large pot, Rot)
//! - **Bins** are used by the ammo, armor, bars/blocks, cloth, coins, finished goods, gems,
//!   leather and weapons stockpiles; a bin holds 60 000 cm³.
//! - **Barrels and large pots** are used by food stockpiles: 60 prepared meals, plants or
//!   cheeses, 30 pieces of meat or fish; a container with a drink holds only that drink stack.
//!   **Bags** hold seeds and powders (flour, sand) in food stockpiles.
//! - A pile brings empty containers to its free tiles up to its `max_bins` / `max_barrels`
//!   ("Store item in bin/barrel": the hauler puts the item *into* a container standing on the
//!   pile), so many items share one tile. Containers are hauled with their contents.
//! - Production and eating find items inside containers (the container must be free).
//! - **Rot** (wiki "Rot"): meat, fish, prepared meals, cheese, eggs rot and plants wither when
//!   they are not in a stockpile, not carried and not part of a job. Being in a barrel does
//!   *not* protect food outside a stockpile. Raw (uncleaned) fish rots everywhere. Seeds,
//!   drinks, flour, fat/tallow, milk and other liquids never rot.
//!
//! # Estimates (not on the wiki)
//! - Item volumes for bins are calibrated on Gordon's real fort (observed bins: 10-11 bars or
//!   blocks, ~48 gems, ~320 cloth, ~78 weapons, ~70 mixed clothing/armor pieces, 12 leathers,
//!   ~66 crafts per bin).
//! - A bag holds 100 seeds (observed up to 68 in the real fort) or one powder stack.
//! - Rotting takes 28 days (one month) of exposure, rotten food rots away after another 28.
//! - Existing containers on a pile are used even beyond the pile's maximum; the maximum only
//!   limits how many empty containers are brought.
//! - Wheelbarrows are not simulated yet (all of Gordon's piles have `max_wheelbarrows` 0).

use crate::items::Cat;
use crate::{manhattan, Sim, NONE};
use std::collections::HashMap;

pub(crate) const CL_NONE: u8 = 0;
pub(crate) const CL_BIN: u8 = 1;
/// Barrel or large pot.
pub(crate) const CL_BARREL: u8 = 2;
pub(crate) const CL_BAG: u8 = 3;

/// Bin capacity in cm³ (wiki "Bin").
pub const BIN_CAPACITY: i32 = 60_000;
/// Barrel/pot capacity in hundredths of a "plant" unit (wiki "Barrel": 60 plants, 30 meat).
pub const BARREL_CAPACITY: i32 = 6_000;
/// Bag capacity in seeds (estimate).
pub const BAG_CAPACITY: i32 = 100;
/// Days of exposure until food rots, and until rotten food rots away (estimate).
pub const ROT_DAYS: u16 = 28;
/// How often container lists are rebuilt (ticks).
pub const STORE_PERIOD: u64 = 400;

/// Container item kinds (never stored inside another container by haulers).
pub(crate) fn is_container_kind(kind: &str, subtype: &str) -> bool {
    matches!(kind, "BIN" | "BARREL" | "BAG" | "BOX" | "BUCKET" | "CAGE" | "QUIVER" | "FLASK" | "BACKPACK" | "ANIMALTRAP" | "GOBLET" | "WHEELBARROW")
        || (kind == "TOOL" && subtype == "ITEM_TOOL_LARGE_POT")
}

/// Which kind of storage container a container item is.
pub(crate) fn container_class(kind: &str, subtype: &str) -> u8 {
    match kind {
        "BIN" => CL_BIN,
        "BARREL" => CL_BARREL,
        "TOOL" if subtype == "ITEM_TOOL_LARGE_POT" => CL_BARREL,
        "BAG" => CL_BAG,
        _ => CL_NONE,
    }
}

/// Which container an item of this kind/category is stored in on a stockpile.
pub(crate) fn wanted_class(kind: &str, subtype: &str, cat: Cat) -> u8 {
    if is_container_kind(kind, subtype) {
        return CL_NONE;
    }
    use Cat::*;
    match cat {
        Ammo | Armor | BarsBlocks | Cloth | Coins | FinishedGoods | Gems | Leather | Weapons => CL_BIN,
        Food => match kind {
            "SEEDS" | "POWDER_MISC" => CL_BAG,
            "PLANT" | "PLANT_GROWTH" | "MEAT" | "FISH" | "FISH_RAW" | "FOOD" | "CHEESE" | "EGG" | "GLOB" | "LIQUID_MISC" => CL_BARREL,
            _ => CL_NONE,
        },
        _ => CL_NONE,
    }
}

/// Space an item takes in a container of class `cl` (see the capacity constants).
pub(crate) fn volume(kind: &str, stack: i32, cl: u8) -> i32 {
    let n = stack.max(1);
    match cl {
        CL_BIN => match kind {
            "BAR" | "BLOCKS" => 5_400,
            "ROUGH" | "SMALLGEM" | "GEM" => 1_200,
            "CLOTH" | "THREAD" => 180,
            "SKIN_TANNED" => 5_000,
            "WEAPON" | "TRAPCOMP" => 750,
            // DF bolt/arrow SIZE 150 per piece.
            "AMMO" => 150 * n,
            "ARMOR" => 2_000,
            "PANTS" => 1_200,
            "SHIELD" => 1_500,
            "HELM" => 600,
            "GLOVES" | "SHOES" => 400,
            "COIN" => 20 * n,
            _ => 900,
        },
        CL_BARREL => match kind {
            "MEAT" | "FISH" | "FISH_RAW" => 200 * n,
            "EGG" => n,
            "LIQUID_MISC" => 60 * n,
            _ => 100 * n,
        },
        CL_BAG => match kind {
            "SEEDS" => n,
            _ => BAG_CAPACITY,
        },
        _ => i32::MAX,
    }
}

fn capacity(cl: u8) -> i32 {
    match cl {
        CL_BIN => BIN_CAPACITY,
        CL_BARREL => BARREL_CAPACITY,
        CL_BAG => BAG_CAPACITY,
        _ => 0,
    }
}

/// Perishable item kinds (wiki "Rot"). [animals] Eggs do not rot (wiki "Nest box": "Eggs do
/// not appear to go rotten and can be collected and cooked after two or more seasons").
pub(crate) fn perishable(kind: &str) -> bool {
    matches!(kind, "MEAT" | "FISH" | "FISH_RAW" | "FOOD" | "CHEESE" | "PLANT" | "PLANT_GROWTH")
}

/// A storage container standing on a stockpile tile.
#[derive(Clone, Debug)]
pub(crate) struct Cont {
    pub id: i32,
    pub tile: u32,
    pub pile: u32,
    pub class: u8,
    pub used: i32,
    pub res: i32,
    /// Holds a drink stack (nothing else may go in).
    pub drink: bool,
    /// Holds anything (a drink may not go in).
    pub any: bool,
}

/// Per-pile container settings and counts.
#[derive(Clone, Debug, Default)]
pub(crate) struct PileStore {
    pub bid: i32,
    pub max_bins: i32,
    pub max_barrels: i32,
    pub max_wheelbarrows: i32,
    pub bins: i32,
    pub barrels: i32,
    pub bags: i32,
    /// Containers being brought (target this pile).
    pub bringing: i32,
}

#[derive(Default)]
pub(crate) struct Store {
    pub piles: Vec<PileStore>,
    pub conts: Vec<Cont>,
    pub cont_ix: HashMap<i32, u32>,
    /// Item -> (container id, volume) for hauls that end in a container.
    pub into: HashMap<i32, (i32, i32)>,
    /// Empty container -> target pile index.
    pub bring: HashMap<i32, u32>,
    /// Item id -> days exposed (rot timer).
    pub rot: HashMap<i32, u16>,
    /// Category bits that have container space somewhere.
    pub free_cats: u32,
    pub stored_into: u64,
    pub brought: u64,
    pub rotted: u64,
    pub rotted_away: u64,
    pub last_pass: u64,
}

/// Fill state of one stockpile (for reports).
#[derive(Clone, Debug, Default)]
pub struct PileReport {
    pub building_id: i32,
    pub categories: Vec<String>,
    pub tiles: usize,
    /// Tiles with at least one item (loose or a container).
    pub used_tiles: usize,
    /// Loose items lying on the pile's tiles (containers count as one).
    pub loose_items: usize,
    pub containers: usize,
    /// Items inside the containers.
    pub contained_items: usize,
    pub max_bins: i32,
    pub max_barrels: i32,
    pub max_wheelbarrows: i32,
}

/// Overall storage state.
#[derive(Clone, Debug, Default)]
pub struct StorageReport {
    pub piles: Vec<PileReport>,
    /// Loose items outside every stockpile that a stockpile would accept (waiting to be hauled).
    pub waiting_items: usize,
    /// All items lying on the ground outside stockpiles (incl. ones no pile accepts).
    pub outside_items: usize,
    pub stored_into_containers: u64,
    pub containers_brought: u64,
    pub rotted: u64,
    pub rotted_away: u64,
}

impl Sim {
    /// [industry] A stockpile was registered as pile `pi` from building `bi`.
    pub(crate) fn store_pile_added(&mut self, bi: usize) {
        let b = &self.world.buildings[bi];
        let s = b.stockpile.clone().unwrap_or_default();
        self.store.piles.push(PileStore {
            bid: b.id,
            max_bins: s.max_bins,
            max_barrels: s.max_barrels,
            max_wheelbarrows: s.max_wheelbarrows,
            ..Default::default()
        });
    }

    /// Classes of containers a pile uses (bit per class).
    fn pile_classes(&self, pi: usize) -> u8 {
        let cats = self.piles[pi].cats;
        let mut m = 0u8;
        let bin_cats = [Cat::Ammo, Cat::Armor, Cat::BarsBlocks, Cat::Cloth, Cat::Coins, Cat::FinishedGoods, Cat::Gems, Cat::Leather, Cat::Weapons];
        if bin_cats.iter().any(|c| cats & c.bit() != 0) {
            m |= 1 << CL_BIN;
        }
        if cats & Cat::Food.bit() != 0 {
            m |= 1 << CL_BARREL | 1 << CL_BAG;
        }
        m
    }

    /// Category a container standing on pile `pi` takes while empty.
    fn pile_cat_for(&self, pi: usize, cl: u8) -> Option<Cat> {
        let cats = self.piles[pi].cats;
        if cl == CL_BARREL || cl == CL_BAG {
            return (cats & Cat::Food.bit() != 0).then_some(Cat::Food);
        }
        [Cat::BarsBlocks, Cat::FinishedGoods, Cat::Gems, Cat::Cloth, Cat::Leather, Cat::Weapons, Cat::Armor, Cat::Ammo, Cat::Coins]
            .into_iter()
            .find(|c| cats & c.bit() != 0)
    }

    pub(crate) fn set_item_cat(&mut self, idx: usize, cat: Cat) {
        if self.imeta[idx].cat == cat {
            return;
        }
        let id = self.world.items[idx].id;
        let queued = self.haul_pos.contains_key(&id);
        if queued {
            self.haul_remove(id);
        }
        self.imeta[idx].cat = cat;
        if self.haulable(idx) && !self.is_stored(idx) && !self.imeta[idx].hauling {
            self.haul_add(id);
        }
    }

    /// Initial container scan (before the haul list is built).
    pub(crate) fn init_storage(&mut self) {
        self.rebuild_containers(true);
    }

    /// Rebuild the container lists from the items on pile tiles.
    fn rebuild_containers(&mut self, init: bool) {
        let mut st = std::mem::take(&mut self.store);
        st.conts.clear();
        st.cont_ix.clear();
        for p in &mut st.piles {
            p.bins = 0;
            p.barrels = 0;
            p.bags = 0;
            p.bringing = 0;
        }
        let mut retag: Vec<(usize, Cat)> = Vec::new();
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            let m = &self.imeta[idx];
            if m.dead || it.container >= 0 || it.holder >= 0 || !it.flags.on_ground || it.flags.in_building || it.building >= 0 {
                continue;
            }
            let cl = container_class(&it.kind, &it.subtype);
            if cl == CL_NONE {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            let Some(&(pi, _)) = self.pile_at.get(&(t as u32)) else { continue };
            let pi = pi as usize;
            if self.pile_classes(pi) & (1 << cl) == 0 {
                continue;
            }
            let (mut used, mut drink, mut any) = (0, false, false);
            if let Some(v) = self.contents.get(&it.id) {
                for c in v {
                    if let Some(&ci) = self.item_idx.get(c) {
                        let ci = ci as usize;
                        if self.imeta[ci].dead {
                            continue;
                        }
                        let ct = &self.world.items[ci];
                        any = true;
                        drink |= ct.kind == "DRINK";
                        used += volume(&ct.kind, ct.stack, cl);
                    }
                }
            }
            if drink {
                used = capacity(cl);
            }
            if !any {
                if let Some(cat) = self.pile_cat_for(pi, cl) {
                    if self.imeta[idx].cat != cat && self.piles[pi].cats & self.imeta[idx].cat.bit() == 0 {
                        retag.push((idx, cat));
                    }
                }
            }
            if let Some(ps) = st.piles.get_mut(pi) {
                match cl {
                    CL_BIN => ps.bins += 1,
                    CL_BARREL => ps.barrels += 1,
                    _ => ps.bags += 1,
                }
            }
            st.cont_ix.insert(it.id, st.conts.len() as u32);
            st.conts.push(Cont { id: it.id, tile: t as u32, pile: pi as u32, class: cl, used, res: 0, drink, any });
        }
        for (&item, &(cid, vol)) in &st.into {
            let _ = item;
            if let Some(&k) = st.cont_ix.get(&cid) {
                st.conts[k as usize].res += vol;
            }
        }
        // Containers that arrived at their target pile are no longer being brought.
        let bring: Vec<(i32, u32)> = st.bring.iter().map(|(&k, &v)| (k, v)).collect();
        for (cid, pi) in bring {
            if st.cont_ix.contains_key(&cid) || !self.item_idx.contains_key(&cid) {
                st.bring.remove(&cid);
            } else if let Some(ps) = st.piles.get_mut(pi as usize) {
                ps.bringing += 1;
            }
        }
        self.store = st;
        for (idx, cat) in retag {
            if init {
                self.imeta[idx].cat = cat;
            } else {
                self.set_item_cat(idx, cat);
            }
        }
        self.update_free_cats();
    }

    fn update_free_cats(&mut self) {
        let mut cats = 0u32;
        for c in &self.store.conts {
            if c.drink || c.used + c.res >= capacity(c.class) {
                continue;
            }
            let pc = self.piles[c.pile as usize].cats;
            cats |= match c.class {
                CL_BIN => pc & !(Cat::Food.bit()),
                _ => pc & Cat::Food.bit(),
            };
        }
        if cats & !self.store.free_cats != 0 {
            self.epoch_haul += 1;
        }
        self.store.free_cats = cats;
    }

    /// Periodic storage work: container lists, bringing empty containers, rot.
    pub(crate) fn storage_pass(&mut self, now: u64) {
        if now < self.store.last_pass + STORE_PERIOD && self.store.last_pass > 0 {
            return;
        }
        self.store.last_pass = now;
        self.rebuild_containers(false);
        self.bring_containers(now);
        if now % (rf_core::TICKS_PER_DAY as u64) < STORE_PERIOD {
            self.rot_pass();
        }
    }

    /// Send empty containers to piles below their maximum.
    fn bring_containers(&mut self, now: u64) {
        let np = self.store.piles.len().min(self.piles.len());
        // Piles that want a container class: (pile, class).
        let mut wants: Vec<(usize, u8)> = Vec::new();
        for pi in 0..np {
            let classes = self.pile_classes(pi);
            if classes == 0 || self.piles[pi].free == 0 {
                continue;
            }
            let ps = &self.store.piles[pi];
            if classes & (1 << CL_BIN) != 0 && ps.bins + ps.bringing < ps.max_bins {
                wants.push((pi, CL_BIN));
            }
            if classes & (1 << CL_BARREL) != 0 && ps.barrels + ps.bags + ps.bringing < ps.max_barrels {
                wants.push((pi, CL_BARREL));
                // Seeds and flour go into bags; bring one when the pile has none.
                if ps.bags == 0 {
                    wants.push((pi, CL_BAG));
                }
            }
        }
        if wants.is_empty() {
            return;
        }
        // Free empty containers not serving a pile, by class (one item scan).
        let mut cands: Vec<(u8, i32, usize, usize)> = Vec::new();
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            let cl = container_class(&it.kind, &it.subtype);
            if cl == CL_NONE || self.store.cont_ix.contains_key(&it.id) || self.store.bring.contains_key(&it.id) {
                continue;
            }
            if !self.haulable(idx) || self.imeta[idx].reserved > 0 || self.imeta[idx].hauling || self.imeta[idx].blocked_until > now || !self.container_empty_pub(it.id) {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            cands.push((cl, it.id, idx, t));
        }
        for (pi, cl) in wants {
            let Some(cat) = self.pile_cat_for(pi, cl) else { continue };
            let pt = self.piles[pi].tiles[0] as usize;
            let near = self.nav.xyz(pt);
            let c = self.nav.comp_root(pt);
            let mut best = (i32::MAX, usize::MAX);
            for (k, &(ccl, _, _, t)) in cands.iter().enumerate() {
                if ccl != cl {
                    continue;
                }
                let d = manhattan(near, self.nav.xyz(t));
                if d < best.0 && self.nav.comp_near(t) == c {
                    best = (d, k);
                }
            }
            if best.1 == usize::MAX {
                continue;
            }
            let (_, id, idx, _) = cands.swap_remove(best.1);
            self.store.bring.insert(id, pi as u32);
            self.store.piles[pi].bringing += 1;
            // Retag it so the haulers see it as belonging to this pile's category.
            let queued = self.haul_pos.contains_key(&id);
            if queued {
                self.haul_remove(id);
            }
            self.imeta[idx].cat = cat;
            self.haul_add(id);
            self.store.brought += 1;
        }
    }

    pub(crate) fn container_empty_pub(&self, id: i32) -> bool {
        match self.contents.get(&id) {
            None => true,
            Some(v) => v.iter().all(|c| self.item_idx.get(c).map(|&i| self.imeta[i as usize].dead).unwrap_or(true)),
        }
    }

    /// [industry] Haul destination for an item: a container with space on a pile, a free tile
    /// of the target pile for containers being brought, else a free pile tile.
    pub(crate) fn store_dest(&mut self, id: i32, cat: Cat, near: (usize, usize, usize), c: u32) -> Option<u32> {
        let idx = *self.item_idx.get(&id)? as usize;
        if let Some(&pi) = self.store.bring.get(&id) {
            let pile = &self.piles[pi as usize];
            let mut best = (i32::MAX, NONE);
            for (k, &t) in pile.tiles.iter().enumerate() {
                if pile.occ[k] != 0 || pile.res[k] {
                    continue;
                }
                let d = manhattan(near, self.nav.xyz(t as usize));
                if d < best.0 && self.nav.walkable(t as usize) && self.nav.comp_root(t as usize) == c {
                    best = (d, t);
                }
            }
            if best.1 != NONE {
                return Some(best.1);
            }
            return self.find_pile_tile(cat, near, c);
        }
        let it = &self.world.items[idx];
        let cl = wanted_class(&it.kind, &it.subtype, cat);
        if cl != CL_NONE && self.store.free_cats & cat.bit() != 0 {
            let vol = volume(&it.kind, it.stack, cl);
            let drink = it.kind == "DRINK";
            let mut best = (i32::MAX, NONE);
            for (k, ct) in self.store.conts.iter().enumerate() {
                if ct.class != cl || ct.drink || (drink && ct.any) || ct.used + ct.res + vol > capacity(cl) {
                    continue;
                }
                if self.piles[ct.pile as usize].cats & cat.bit() == 0 {
                    continue;
                }
                let d = manhattan(near, self.nav.xyz(ct.tile as usize));
                if d < best.0 && self.nav.comp_root(ct.tile as usize) == c {
                    best = (d, k as u32);
                }
            }
            if best.1 != NONE {
                let ct = &mut self.store.conts[best.1 as usize];
                ct.res += vol;
                self.store.into.insert(id, (ct.id, vol));
                return Some(ct.tile);
            }
        }
        self.find_pile_tile(cat, near, c)
    }

    /// [industry] The haul of `id` was assigned a container.
    pub(crate) fn store_assigned(&self, id: i32) -> bool {
        self.store.into.contains_key(&id)
    }

    /// [industry] Forget a container assignment (cancelled haul).
    pub(crate) fn store_release(&mut self, id: i32) {
        if let Some((cid, vol)) = self.store.into.remove(&id) {
            if let Some(&k) = self.store.cont_ix.get(&cid) {
                let ct = &mut self.store.conts[k as usize];
                ct.res = (ct.res - vol).max(0);
            }
        }
    }

    /// [industry] Arriving with a hauled item: put it into its container if that still stands
    /// at `tile`. Returns true if the item was stored.
    pub(crate) fn store_drop(&mut self, s: usize, id: i32, tile: u32) -> bool {
        let Some((cid, vol)) = self.store.into.remove(&id) else { return false };
        let k = self.store.cont_ix.get(&cid).copied();
        if let Some(k) = k {
            let ct = &mut self.store.conts[k as usize];
            ct.res = (ct.res - vol).max(0);
        }
        let Some(&ci) = self.item_idx.get(&cid) else { return false };
        let ci = ci as usize;
        let cpos = self.world.items[ci].pos;
        let c_ok = !self.imeta[ci].dead
            && self.world.items[ci].holder < 0
            && self.world.items[ci].container < 0
            && self.world.idx(cpos) == Some(tile as usize);
        let Some(&ii) = self.item_idx.get(&id) else { return false };
        let ii = ii as usize;
        if !c_ok || self.imeta[ii].dead {
            return false;
        }
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != id);
        self.ai[s].carrying = -1;
        let drink = self.world.items[ii].kind == "DRINK";
        let it = &mut self.world.items[ii];
        it.pos = cpos;
        it.container = cid;
        it.holder = -1;
        it.flags.on_ground = false;
        it.flags.in_inventory = false;
        it.flags.in_container = true;
        self.imeta[ii].hauling = false;
        self.contents.entry(cid).or_default().push(id);
        if let Some(k) = k {
            let ct = &mut self.store.conts[k as usize];
            ct.used += vol;
            ct.any = true;
            ct.drink |= drink;
        }
        // The container now carries the category of its contents.
        let cat = self.imeta[ii].cat;
        if self.imeta[ci].cat != cat && !self.store_cont_has_other(cid, id) {
            self.set_item_cat(ci, cat);
        }
        self.store.stored_into += 1;
        true
    }

    fn store_cont_has_other(&self, cid: i32, id: i32) -> bool {
        self.contents
            .get(&cid)
            .map(|v| v.iter().any(|&x| x != id && self.item_idx.get(&x).map(|&i| !self.imeta[i as usize].dead).unwrap_or(false)))
            .unwrap_or(false)
    }

    /// Is this item protected from rot (in a stockpile, carried, in a job or a building)?
    fn rot_safe(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        if it.kind == "FISH_RAW" {
            return false;
        }
        if it.holder >= 0 || it.flags.in_job || it.flags.in_building || it.building >= 0 || self.imeta[idx].hauling {
            return true;
        }
        let pos = if it.container >= 0 {
            match self.item_idx.get(&it.container) {
                Some(&ci) => {
                    let c = &self.world.items[ci as usize];
                    if c.holder >= 0 || c.building >= 0 || c.flags.in_building {
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
            Some(t) => self.pile_at.contains_key(&(t as u32)),
            None => true,
        }
    }

    /// Daily rot step (wiki "Rot").
    fn rot_pass(&mut self) {
        let mut rot_now: Vec<usize> = Vec::new();
        let mut gone: Vec<i32> = Vec::new();
        for idx in 0..self.world.items.len() {
            if self.imeta[idx].dead || !perishable(&self.world.items[idx].kind) {
                continue;
            }
            if self.rot_safe(idx) {
                continue;
            }
            let id = self.world.items[idx].id;
            let d = self.store.rot.entry(id).or_insert(0);
            *d += 1;
            if *d >= 2 * ROT_DAYS && self.world.items[idx].flags.rotten {
                gone.push(id);
            } else if *d >= ROT_DAYS && !self.world.items[idx].flags.rotten {
                rot_now.push(idx);
            }
        }
        if !rot_now.is_empty() {
            for &idx in &rot_now {
                self.world.items[idx].flags.rotten = true;
            }
            let ids: std::collections::HashSet<i32> = rot_now.iter().map(|&i| self.world.items[i].id).collect();
            self.foods.retain(|x| !ids.contains(x));
            self.store.rotted += rot_now.len() as u64;
        }
        for id in gone {
            if self.imeta[self.item_idx[&id] as usize].reserved > 0 {
                continue;
            }
            self.store.rot.remove(&id);
            self.destroy_item(id);
            self.store.rotted_away += 1;
        }
        let idx = &self.item_idx;
        self.store.rot.retain(|id, _| idx.contains_key(id));
    }

    // ------------------------------------------------------------------ public API

    /// Stockpile fill and container use (bins, barrels, pots, bags).
    pub fn storage_report(&self) -> StorageReport {
        let mut r = StorageReport {
            stored_into_containers: self.store.stored_into,
            containers_brought: self.store.brought,
            rotted: self.store.rotted,
            rotted_away: self.store.rotted_away,
            ..Default::default()
        };
        let mut per: Vec<PileReport> = (0..self.piles.len())
            .map(|pi| {
                let ps = self.store.piles.get(pi).cloned().unwrap_or_default();
                let b = self.world.building(ps.bid);
                PileReport {
                    building_id: ps.bid,
                    categories: b.and_then(|b| b.stockpile.as_ref()).map(|s| s.categories.clone()).unwrap_or_default(),
                    tiles: self.piles[pi].tiles.len(),
                    max_bins: ps.max_bins,
                    max_barrels: ps.max_barrels,
                    max_wheelbarrows: ps.max_wheelbarrows,
                    ..Default::default()
                }
            })
            .collect();
        let mut used: Vec<std::collections::HashSet<u32>> = vec![Default::default(); self.piles.len()];
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            let m = &self.imeta[idx];
            if m.dead || it.holder >= 0 || it.flags.in_building || it.building >= 0 || it.flags.construction {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            match self.pile_at.get(&(t as u32)) {
                Some(&(pi, _)) if self.piles[pi as usize].cats != 0 => {
                    let p = &mut per[pi as usize];
                    if it.container >= 0 {
                        if self.store.cont_ix.contains_key(&it.container) {
                            p.contained_items += 1;
                        }
                    } else if it.flags.on_ground {
                        used[pi as usize].insert(t as u32);
                        if self.store.cont_ix.contains_key(&it.id) {
                            p.containers += 1;
                        } else {
                            p.loose_items += 1;
                        }
                    }
                }
                _ => {
                    if it.container < 0 && it.flags.on_ground {
                        r.outside_items += 1;
                        if self.pile_cats & m.cat.bit() != 0 && self.haulable(idx) {
                            r.waiting_items += 1;
                        }
                    }
                }
            }
        }
        for (pi, p) in per.iter_mut().enumerate() {
            p.used_tiles = used[pi].len();
        }
        r.piles = per.into_iter().filter(|p| p.building_id != 0 && !p.categories.is_empty()).collect();
        r
    }
}
