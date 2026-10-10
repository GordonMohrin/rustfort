//! [perf] Faster versions of hot searches that keep the exact results of the originals.
//!
//! Everything here is a drop-in replacement: the same unit gets the same item, path or job as
//! before, so `state_hash` does not change. See `examples/scale.rs` for the benchmark.

use crate::items::Cat;
use crate::Sim;
use std::collections::HashMap;

/// [perf11] Cycle counters for hot functions: compiled in only with the `prof` feature
/// (`cargo run --release -p rf-sim --features prof --example siegeperf`), otherwise every call
/// is an empty inline function. Counts are inclusive (a nested counter is also in its caller).
#[allow(dead_code)] // the names and the table size are only read with the `prof` feature
pub(crate) mod prof {
    pub const N: usize = 33;
    pub const NAMES: [&str; N] = [
        "think worker", "think other", "combat_think", "fighter_think", "soldier_think", "civilian_think", "find_target", "attack",
        "strike", "threat_scan", "request_path", "decide", "sync_wounds", "solve_paths", "commit_paths", "pursue",
        "ranged_pursue", "war_wreck_think", "destroyer_think", "raid_think", "try_haul", "haul_scan", "leave_think", "patient_think",
        "uname", "armor_over", "combat_kill", "wielded", "flow_path", "strike log", "haul no item", "haul no dest",
        "haul skipped",
    ];
    pub const HAUL_NO_ITEM: usize = 30;
    pub const HAUL_NO_DEST: usize = 31;
    pub const HAUL_SKIPPED: usize = 32;
    pub const UNAME: usize = 24;
    pub const ARMOR_OVER: usize = 25;
    pub const COMBAT_KILL: usize = 26;
    pub const WIELDED: usize = 27;
    pub const FLOW_PATH: usize = 28;
    pub const STRIKE_LOG: usize = 29;
    pub const THINK_WORKER: usize = 0;
    pub const THINK_OTHER: usize = 1;
    pub const COMBAT_THINK: usize = 2;
    pub const FIGHTER: usize = 3;
    pub const SOLDIER: usize = 4;
    pub const CIVILIAN: usize = 5;
    pub const FIND_TARGET: usize = 6;
    pub const ATTACK: usize = 7;
    pub const STRIKE: usize = 8;
    pub const THREAT_SCAN: usize = 9;
    pub const REQUEST_PATH: usize = 10;
    pub const DECIDE: usize = 11;
    pub const SYNC_WOUNDS: usize = 12;
    pub const SOLVE_PATHS: usize = 13;
    pub const COMMIT_PATHS: usize = 14;
    pub const PURSUE: usize = 15;
    pub const RANGED_PURSUE: usize = 16;
    pub const WRECK: usize = 17;
    pub const DESTROYER: usize = 18;
    pub const RAID_THINK: usize = 19;
    pub const TRY_HAUL: usize = 20;
    pub const HAUL_SCAN: usize = 21;
    pub const LEAVE: usize = 22;
    pub const PATIENT: usize = 23;

    #[cfg(feature = "prof")]
    mod imp {
        use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
        #[allow(clippy::declare_interior_mutable_const)]
        const Z: AtomicU64 = AtomicU64::new(0);
        static CYC: [AtomicU64; super::N] = [Z; super::N];
        static CNT: [AtomicU64; super::N] = [Z; super::N];
        #[inline(always)]
        pub fn t() -> u64 {
            // SAFETY: rdtsc has no preconditions on x86_64.
            unsafe { core::arch::x86_64::_rdtsc() }
        }
        #[inline(always)]
        pub fn add(k: usize, t0: u64) {
            CYC[k].fetch_add(t().wrapping_sub(t0), Relaxed);
            CNT[k].fetch_add(1, Relaxed);
        }
        pub fn report() -> Vec<(&'static str, u64, u64)> {
            (0..super::N).map(|k| (super::NAMES[k], CNT[k].swap(0, Relaxed), CYC[k].swap(0, Relaxed))).collect()
        }
    }
    #[cfg(not(feature = "prof"))]
    mod imp {
        #[inline(always)]
        pub fn t() -> u64 {
            0
        }
        #[inline(always)]
        pub fn add(_k: usize, _t0: u64) {}
        pub fn report() -> Vec<(&'static str, u64, u64)> {
            Vec::new()
        }
    }
    pub use imp::{add, report, t};

    /// Counts the cycles until it is dropped (`let _p = prof::Guard::new(prof::STRIKE);`).
    pub struct Guard(#[allow(dead_code)] usize, #[allow(dead_code)] u64);
    impl Guard {
        #[inline(always)]
        pub fn new(k: usize) -> Guard {
            Guard(k, t())
        }
    }
    #[cfg(feature = "prof")]
    impl Drop for Guard {
        #[inline(always)]
        fn drop(&mut self) {
            add(self.0, self.1);
        }
    }
}

const NO_IX: u32 = u32::MAX;
/// Ids from 0 up to this bound live in the dense table, others (never seen in DF) in a map.
const DENSE_MAX: i32 = 1 << 26;

/// [perf] Item id -> index into `world.items`: a dense table instead of a `HashMap`.
///
/// Same interface as the `HashMap<i32, u32>` it replaces (`get`, `insert`, `contains_key`,
/// `clear`, `[&id]`), so all modules keep working unchanged. Lookups are one array read, and
/// the item compaction updates only the entries that moved instead of rebuilding a hash map
/// of all 16 000 items whenever one item is used up.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct IdMap {
    v: Vec<u32>,
    other: HashMap<i32, u32>,
}

impl IdMap {
    #[inline]
    pub(crate) fn get(&self, id: &i32) -> Option<&u32> {
        let id = *id;
        if (0..DENSE_MAX).contains(&id) {
            match self.v.get(id as usize) {
                Some(x) if *x != NO_IX => Some(x),
                _ => None,
            }
        } else {
            self.other.get(&id)
        }
    }

    #[inline]
    pub(crate) fn contains_key(&self, id: &i32) -> bool {
        self.get(id).is_some()
    }

    pub(crate) fn insert(&mut self, id: i32, ix: u32) -> Option<u32> {
        if (0..DENSE_MAX).contains(&id) {
            let i = id as usize;
            if i >= self.v.len() {
                self.v.resize((i + 1).max(self.v.len() * 3 / 2), NO_IX);
            }
            let old = std::mem::replace(&mut self.v[i], ix);
            (old != NO_IX).then_some(old)
        } else {
            self.other.insert(id, ix)
        }
    }

    pub(crate) fn remove(&mut self, id: &i32) -> Option<u32> {
        let id = *id;
        if (0..DENSE_MAX).contains(&id) {
            let old = std::mem::replace(self.v.get_mut(id as usize)?, NO_IX);
            (old != NO_IX).then_some(old)
        } else {
            self.other.remove(&id)
        }
    }

    pub(crate) fn clear(&mut self) {
        self.v.iter_mut().for_each(|x| *x = NO_IX);
        self.other.clear();
    }
}

impl std::ops::Index<&i32> for IdMap {
    type Output = u32;
    #[inline]
    fn index(&self, id: &i32) -> &u32 {
        self.get(id).expect("no item with this id")
    }
}

/// [perf11] Haul categories that just failed in a walk component (no waiting item there, or no
/// destination for the nearest one), skipped by every hauler there for a while.
///
/// At 487 citizens about 2 300 haul searches ran per 1000 ticks for 16-29 hauls; each scanned
/// the whole waiting list (29k cycles) to find the same nothing. An entry dies after
/// `HAUL_FAIL_TTL` ticks, when an item is added to the haul list (`epoch_haul`) or when the
/// navigation grid changes (`Nav::version`). Freed pile space, released reservations and
/// expired path blocks are only seen after the TTL, so a haul can start up to that much later
/// (DF also queues stockpile hauls periodically, not every tick).
#[derive(Clone, Default)]
pub(crate) struct HaulFail {
    v: Vec<(u32, u32, u64, u64, u64)>, // (component, category mask, until, epoch_haul, nav version)
}

pub(crate) const HAUL_FAIL_TTL: u64 = 200;

impl HaulFail {
    /// Categories to skip in component `c` right now.
    pub(crate) fn mask(&mut self, c: u32, now: u64, epoch: u64, ver: u64) -> u32 {
        self.v.retain(|e| e.2 > now && e.3 == epoch && e.4 == ver);
        self.v.iter().find(|e| e.0 == c).map(|e| e.1).unwrap_or(0)
    }

    /// Remember that the categories in `mask` failed in component `c`.
    pub(crate) fn add(&mut self, c: u32, mask: u32, now: u64, epoch: u64, ver: u64) {
        if mask == 0 {
            return;
        }
        match self.v.iter_mut().find(|e| e.0 == c && e.3 == epoch && e.4 == ver) {
            Some(e) => e.1 |= mask,
            None => self.v.push((c, mask, now + HAUL_FAIL_TTL, epoch, ver)),
        }
    }
}

/// [perf] Per `haul` entry: the item's category and its cached walk component.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)] // [saves11]
pub(crate) struct HaulCache {
    pub cat: u8,
    tile: u32,
    ver: u64,
    comp: u32,
}

impl HaulCache {
    pub(crate) fn new(cat: Cat) -> HaulCache {
        HaulCache { cat: cat as u8, tile: u32::MAX, ver: u64::MAX, comp: 0 }
    }
}

/// Best haul candidate of one category: (Manhattan distance, item id, item tile).
pub(crate) type HaulBest = (i32, i32, usize);
pub(crate) const NO_HAUL: HaulBest = (i32::MAX, i32::MAX, 0);

impl Sim {
    /// [perf] One pass over the haul list that finds the nearest item of every wanted category
    /// (same filters and tie-break by item id as the old per-category re-scan).
    ///
    /// `try_haul` used to scan the whole list, try the winner's destination and, when its
    /// category had none, drop the category and scan again (on average 2.7 scans per call at
    /// 500 citizens, nearly all failing). `store_dest` has no side effects when it returns
    /// None, so trying the categories in the order of their best (distance, id) gives exactly
    /// the same item as the repeated scans.
    pub(crate) fn haul_best_per_cat(&mut self, want: u32, up: (usize, usize, usize), c: u32, now: u64) -> [HaulBest; 17] {
        let _p = prof::Guard::new(prof::HAUL_SCAN); // [perf11]
        let mut best = [NO_HAUL; 17];
        let (ux, uy, uz) = (up.0 as i32, up.1 as i32, up.2 as i32);
        let ver = self.nav.version;
        for (k, &id) in self.haul.iter().enumerate() {
            // The cache array is read sequentially; only wanted items touch item data.
            let e = &mut self.haul_cat[k];
            if want & (1u32 << e.cat) == 0 {
                continue;
            }
            let idx = self.haul_ix[k] as usize;
            let m = &self.imeta[idx];
            if want & m.cat.bit() == 0 || m.reserved > 0 || m.blocked_until > now {
                continue;
            }
            // Distance straight from the position (no tile index -> x/y/z divisions);
            // equal to manhattan(up, xyz(tile)) for every in-bounds position.
            let p = self.world.items[idx].pos;
            if !self.world.in_bounds(p) {
                continue;
            }
            let d = (p.x as i32 - ux).abs() + (p.y as i32 - uy).abs() + (p.z as i32 - uz).abs();
            let b = &mut best[m.cat as usize];
            if (d, id) >= (b.0, b.1) {
                continue;
            }
            let t = self.world.idx_unchecked(p);
            // comp_near of an item lying on an unwalkable tile looks at all its neighbours; the
            // answer only changes with the item's tile or the navigation grid (`Nav::version`).
            if e.tile != t as u32 || e.ver != ver {
                e.tile = t as u32;
                e.ver = ver;
                e.comp = self.nav.comp_near(t);
            }
            if e.comp != c {
                continue;
            }
            *b = (d, id, t);
        }
        best
    }

    /// [perf] Drop used-up items from `world.items` (keeping the order) and fix every index.
    ///
    /// Same result as the old version (stable compaction, then all id -> index lookups and the
    /// drink, food, haul and container lists rebuilt), but only items behind the first dead
    /// one are touched, and the id table is patched in place.
    pub(crate) fn compact_items_fast(&mut self) {
        self.items_dirty = false;
        let n = self.world.items.len();
        let Some(first) = (0..n).find(|&r| self.imeta[r].dead) else { return };
        let mut gone: Vec<i32> = Vec::new(); // [chaos10]
        for r in first..n {
            if self.imeta[r].dead {
                let id = self.world.items[r].id;
                // Only forget the id if it still points here (the newest item with an id wins).
                if self.item_idx.get(&id) == Some(&(r as u32)) {
                    self.item_idx.remove(&id);
                    gone.push(id);
                }
            }
        }
        // [chaos10] Buildings (planted seeds, consumed trap ammo) and units (eaten food) must
        // not keep the ids of items that no longer exist: the UI and saves would carry them.
        if !gone.is_empty() {
            gone.sort_unstable();
            let dead = |id: &i32| gone.binary_search(id).is_ok();
            for b in &mut self.world.buildings {
                if b.items.iter().any(dead) {
                    b.items.retain(|id| !dead(id));
                }
            }
            for u in &mut self.world.units {
                if u.inventory.iter().any(dead) {
                    u.inventory.retain(|id| !dead(id));
                }
            }
        }
        let mut w = first;
        for r in first..n {
            if !self.imeta[r].dead {
                if w != r {
                    self.world.items.swap(w, r);
                    self.imeta.swap(w, r);
                }
                let id = self.world.items[w].id;
                if self.item_idx.get(&id) == Some(&(r as u32)) {
                    self.item_idx.insert(id, w as u32);
                }
                w += 1;
            }
        }
        self.world.items.truncate(w);
        self.imeta.truncate(w);
        let idx = &self.item_idx;
        self.drinks.retain(|id| idx.contains_key(id));
        self.foods.retain(|id| idx.contains_key(id));
        let mut k = 0;
        let cache = &mut self.haul_cat;
        self.haul.retain(|id| {
            let keep = idx.contains_key(id);
            if !keep {
                cache[k].cat = u8::MAX;
            }
            k += 1;
            keep
        });
        cache.retain(|e| e.cat != u8::MAX);
        self.haul_pos.clear();
        self.haul_ix.clear();
        for (p, &id) in self.haul.iter().enumerate() {
            self.haul_pos.insert(id, p as u32);
            self.haul_ix.push(idx[&id]);
        }
        self.contents.retain(|cid, _| idx.contains_key(cid));
        for v in self.contents.values_mut() {
            v.retain(|id| idx.contains_key(id));
        }
    }

    /// [perf] Category in `want` whose best candidate has the smallest (distance, id).
    pub(crate) fn haul_pick(best: &[HaulBest; 17], want: u32) -> Option<Cat> {
        let mut pick: Option<(i32, i32, Cat)> = None;
        for cat in Cat::ALL {
            let b = best[cat as usize];
            if want & cat.bit() == 0 || b.0 == i32::MAX {
                continue;
            }
            if pick.map(|p| (b.0, b.1) < (p.0, p.1)).unwrap_or(true) {
                pick = Some((b.0, b.1, cat));
            }
        }
        pick.map(|p| p.2)
    }
}
