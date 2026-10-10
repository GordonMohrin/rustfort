//! [society2] Lifestyle (DF v50): clothing wear, meal quality, kitchen settings, miasma, rain.
//!
//! # DF rules implemented (wiki v50: Clothing, Wear, Meal, Kitchen, Quality, Miasma, Rot,
//! Weather, Rain)
//! - **Wear** (w:Wear, w:Clothing): clothing worn by a citizen (armor, pants, shoes, gloves,
//!   helms of cloth, leather, bone ...; metal does not wear) ages: worn (x), threadbare (X),
//!   tattered (XX), then it falls apart ("RottedClothing"). Threadbare clothing gives
//!   "OldClothing", tattered "TatteredClothing". A civilian whose clothing is worn fetches a new
//!   (unworn) replacement of the same kind (same item subtype) from a stockpile, puts it on
//!   ("WearItem" for a quality item) and drops the old piece.
//! - **Meal quality** (w:Kitchen, w:Quality): each prepared meal gets a quality from the cook's
//!   (focus-adjusted) Cooking skill; its value is the ingredient values times DF's quality
//!   multiplier (1, 2, 3, 4, 5, 12). Quality drives the "GoodMeal" thought (see `social`).
//! - **Kitchen settings** (`World::kitchen`, `Sim::set_kitchen`): a material excluded from
//!   cooking is never put into a meal, one excluded from brewing never brewed (DF's kitchen
//!   screen). Matches item type (PLANT, PLANT_GROWTH, MEAT, FISH, ...; empty = any) and material.
//! - **Miasma** (w:Miasma, w:Rot): rotten corpses, body parts and food lying loose indoors give
//!   off miasma, which spreads orthogonally over open floor; a dwarf walking through it thinks
//!   "Miasma" ("after retching on a miasma"). Nothing outdoors (under the sky) makes miasma.
//! - **Rain** (w:Weather): with weather on it rains on some days; with `Sim::set_rain_thoughts`
//!   citizens outdoors in the rain think "Rain" (off by default: none of the 17 281 thoughts in
//!   the reference fort's export is a Rain thought although its world has weather on).
//!
//! # Estimates (not on the wiki)
//! - One wear level per 336 days of wearing; clothing thoughts at most once a year; dwarves
//!   look for replacements once an item is worn (x) and try again weekly when none is free.
//!   Calibrated on the reference fort: 91 % of the worn clothing is unworn, 7 % worn, 2 % threadbare
//!   or tattered, no OldClothing/TatteredClothing thoughts but about 25 WearItem a month.
//! - Corpses of creatures that die in the sim rot after 14 days and stop giving off miasma 56
//!   days later (skeletal); miasma reaches 2 tiles from its source; at most one Miasma thought
//!   per dwarf in 2 days (the export has 30-60 Miasma thoughts a month).
//! - Rain on 1 day in 5 outside winter, for 2-8 hours.

use crate::path::Goals;
use crate::rng::Rng;
use crate::social::Class;
use crate::{needs, Kind, Sim, Task};
use rf_core::{tile_flags, TICKS_PER_DAY};
use std::collections::{BTreeMap, HashSet};

const DAY: u64 = TICKS_PER_DAY as u64;
/// Days of wearing per wear level (estimate).
pub const WEAR_DAYS: u16 = 336;
/// Days until a new corpse rots, and how long it gives off miasma (estimates).
pub const CORPSE_ROT_DAYS: u16 = 14;
pub const CORPSE_MIASMA_DAYS: u16 = 56;
/// Miasma reach in tiles (estimate).
pub const MIASMA_RANGE: u32 = 2;

/// Lifestyle statistics for reports and the benchmark.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct LifestyleReport {
    /// Wear levels gained by worn clothing.
    pub wear_steps: u64,
    pub old_clothing: u64,
    pub tattered_clothing: u64,
    pub rotted_off: u64,
    pub replacements: u64,
    /// Meals cooked per quality level 0..=5 (ordinary .. masterwork).
    pub meal_quality: [u64; 6],
    pub miasma_events: u64,
    pub miasma_sources_max: usize,
    pub miasma_tiles_max: usize,
    pub rain_days: u64,
    pub rain_thoughts: u64,
    /// Ingredients skipped because of the kitchen settings (census counts).
    pub kitchen_rules: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct Life {
    ready: bool,
    /// Worn clothing item id -> days worn at the current wear level.
    wear: BTreeMap<i32, u16>,
    /// Per slot: next tick a clothing thought / replacement search may happen.
    next_thought: Vec<u64>,
    next_try: Vec<u64>,
    /// Per slot: clothing item the dwarf walks to (-1 none) and the item it replaces.
    errand: Vec<(i32, i32)>,
    next_miasma: Vec<u64>,
    /// Free replacement clothing (item ids), refreshed daily.
    spares: Vec<i32>,
    miasma: HashSet<u32>,
    /// Age in days of corpses created in this run.
    corpse_age: BTreeMap<i32, u16>,
    first_new_item: i32,
    rain_start: u64,
    rain_until: u64,
    rain_seen: Vec<u64>,
    /// "Rain" thoughts on (`Sim::set_rain_thoughts`; off by default, see module docs).
    pub rain_thoughts: bool,
    /// Kitchen exclusions per material: bits 0-3 no-cook for PLANT / PLANT_GROWTH / other /
    /// any type, bits 4-7 no-brew.
    kitchen: Vec<u8>,
    kitchen_key: usize,
    pub rep: LifestyleReport,
}

/// Clothing kinds that wear out.
pub(crate) fn is_clothing(kind: &str) -> bool {
    matches!(kind, "ARMOR" | "PANTS" | "SHOES" | "GLOVES" | "HELM")
}

fn kitchen_group(kind: &str) -> u8 {
    match kind {
        "PLANT" => 0,
        "PLANT_GROWTH" => 1,
        "" => 3,
        _ => 2,
    }
}

impl Sim {
    fn life_ensure(&mut self) {
        let n = self.ai.len();
        if !self.life.ready {
            self.life.ready = true;
            self.life.first_new_item = self.next_item_id;
        }
        if self.life.next_thought.len() < n {
            self.life.next_thought.resize(n, 0);
            self.life.next_try.resize(n, 0);
            self.life.errand.resize(n, (-1, -1));
            self.life.next_miasma.resize(n, 0);
            self.life.rain_seen.resize(n, 0);
        }
    }

    // ------------------------------------------------------------------ kitchen

    fn kitchen_rebuild(&mut self) {
        let mut v = vec![0u8; self.world.materials.len()];
        for k in &self.world.kitchen {
            let Some(m) = self.world.materials.iter().position(|m| m.token == k.mat) else { continue };
            let g = kitchen_group(&k.item_type);
            if k.no_cook {
                v[m] |= 1 << g;
            }
            if k.no_brew {
                v[m] |= 1 << (4 + g);
            }
        }
        self.life.kitchen = v;
        self.life.kitchen_key = self.world.kitchen.len();
        self.life.rep.kitchen_rules = self.world.kitchen.len();
    }

    /// [hook] Is item `idx` excluded from cooking (`cook`) or brewing by the kitchen settings?
    #[inline]
    pub(crate) fn kitchen_excluded(&self, idx: usize, cook: bool) -> bool {
        if self.world.kitchen.is_empty() {
            return false;
        }
        let it = &self.world.items[idx];
        let bits = self.life.kitchen.get(it.mat as usize).copied().unwrap_or(0);
        if bits == 0 {
            return false;
        }
        let g = kitchen_group(&it.kind);
        let mask = if cook { (1u8 << g) | (1 << 3) } else { (1u8 << (4 + g)) | (1 << 7) };
        bits & mask != 0
    }

    /// Re-index the kitchen settings after a change (called by `Sim::set_kitchen`).
    pub(crate) fn kitchen_changed(&mut self) {
        self.kitchen_rebuild();
        self.mark_prod_dirty();
    }

    // ------------------------------------------------------------------ meals

    /// [hook] From `prod_complete`: meals with ids >= `first` were just cooked by slot `s`.
    pub(crate) fn lifestyle_meal_cooked(&mut self, s: usize, first: i32, now: u64) {
        if self.locs.off {
            return;
        }
        let rating = self.social_rating(s, self.world.units[self.ai[s].uidx].skill("COOK"));
        let mut r = Rng::for_unit(self.seed, s, now, 0xC00C);
        let mut masterworks: Vec<i32> = Vec::new(); // [calib11-social]
        for id in first..self.next_item_id {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            if self.world.items[i].kind != "FOOD" {
                continue;
            }
            let q = crate::skills::quality_roll(rating, &mut r).min(5); // [skills9] same roll as crafts (DF meals average ~1.3)
            let it = &mut self.world.items[i];
            it.quality = q;
            // Ingredient value 2 per unit (estimate for unrecorded values) x quality multiplier.
            let base = if it.value > 0 { it.value } else { 2 * it.stack.max(1) };
            it.value = base * crate::trade::quality_mult(q) as i32;
            self.life.rep.meal_quality[q as usize] += 1;
            if q == 5 {
                masterworks.push(id); // [calib11-social]
            }
        }
        for id in masterworks {
            self.skills_masterpiece(s, id, now); // [calib11-social] "has created a masterpiece <meal>!"
        }
    }

    // ------------------------------------------------------------------ periodic

    /// From the society2 pass.
    pub(crate) fn lifestyle_tick(&mut self, now: u64, phase: u64) {
        self.life_ensure();
        if self.life.kitchen_key != self.world.kitchen.len() || (self.life.kitchen.is_empty() && !self.world.kitchen.is_empty()) {
            self.kitchen_rebuild();
        }
        if phase == 0 {
            self.refresh_spares();
            self.weather_day(now);
        }
        if phase == 6 {
            self.miasma_day();
        }
        // Daily wear for the citizens of this wheel slot.
        let wheel = crate::locations::S2_PERIOD;
        let per_day = DAY / wheel;
        for s in 0..self.ai.len() {
            if s as u64 % per_day == phase && self.s2_citizen(s) {
                self.wear_day(s, now);
            }
        }
        // Miasma and rain, every pass.
        let check_miasma = !self.life.miasma.is_empty();
        let raining = now < self.life.rain_until;
        if !check_miasma && !raining {
            return;
        }
        for s in 0..self.ai.len() {
            if self.ai[s].kind != Kind::Worker || !self.s2_citizen(s) {
                continue;
            }
            let t = self.ai[s].pos;
            if check_miasma && self.life.miasma.contains(&t) && now >= self.life.next_miasma[s] {
                self.life.next_miasma[s] = now + 2 * DAY;
                self.life.rep.miasma_events += 1;
                self.feel(s, "Miasma", Class::NegMild, 80, -1, 0, now);
            }
            if raining && self.life.rain_thoughts && now >= self.life.rain_start && self.world.tiles[t as usize].flags & tile_flags::OUTSIDE != 0 && self.life.rain_seen[s] < self.life.rain_start {
                self.life.rain_seen[s] = now;
                self.life.rep.rain_thoughts += 1;
                self.feel(s, "Rain", Class::NegMild, 50, -1, 0, now);
            }
        }
    }

    fn wear_day(&mut self, s: usize, now: u64) {
        let uidx = self.ai[s].uidx;
        let inv = self.world.units[uidx].inventory.clone();
        let mut worst = 0u8;
        for id in inv {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            let it = &self.world.items[i];
            if !is_clothing(&it.kind) || self.imeta[i].dead || self.world.material(it.mat).map(|m| m.is_metal || m.is_stone || m.is_gem).unwrap_or(false) {
                continue;
            }
            let start = (crate::rng::splitmix(id as u64) % WEAR_DAYS as u64) as u16;
            let d = self.life.wear.entry(id).or_insert(start);
            *d += 1;
            if *d >= WEAR_DAYS {
                *d = 0;
                self.life.rep.wear_steps += 1;
                let it = &mut self.world.items[i];
                it.wear += 1;
                if it.wear > 3 {
                    // Falls apart.
                    self.world.units[uidx].inventory.retain(|&x| x != id);
                    self.life.wear.remove(&id);
                    self.destroy_item(id);
                    self.life.rep.rotted_off += 1;
                    self.feel(s, "RottedClothing", Class::Neg, 120, -1, 0, now);
                    continue;
                }
            }
            worst = worst.max(self.world.items[i].wear);
        }
        if worst >= 2 && now >= self.life.next_thought[s] {
            self.life.next_thought[s] = now + 336 * DAY;
            if worst >= 3 {
                self.life.rep.tattered_clothing += 1;
                self.feel(s, "TatteredClothing", Class::Neg, 100, -1, 0, now);
            } else {
                self.life.rep.old_clothing += 1;
                self.feel(s, "OldClothing", Class::NegMild, 60, -1, 0, now);
            }
        }
    }

    fn refresh_spares(&mut self) {
        let mut v = Vec::new();
        for (i, it) in self.world.items.iter().enumerate() {
            if !is_clothing(&it.kind) || it.wear >= 1 || it.holder >= 0 || self.imeta[i].dead {
                continue;
            }
            let f = &it.flags;
            if f.forbid || f.in_job || f.in_building || f.owned || f.in_inventory || f.construction || it.building >= 0 || self.imeta[i].reserved > 0 {
                continue;
            }
            // Only from stockpiles (loose on a pile tile or in a bin there).
            let pos = it.pos;
            let on_pile = self.world.idx(pos).map(|t| self.pile_at.contains_key(&(t as u32))).unwrap_or(false);
            if on_pile {
                v.push(it.id);
            }
        }
        self.life.spares = v;
    }

    /// [hook] Start of `decide`: replace threadbare clothing. True if the dwarf is on the way.
    pub(crate) fn lifestyle_decide(&mut self, s: usize, now: u64) -> bool {
        if self.locs.off || s >= self.life.next_try.len() {
            return false;
        }
        let (target, old) = self.life.errand[s];
        if target >= 0 {
            self.life.errand[s] = (-1, -1);
            if let Some(&i) = self.item_idx.get(&target) {
                self.imeta[i as usize].reserved = 0;
            }
            if self.item_reachable_here(s, target) && self.item_idx.get(&target).map(|&i| self.item_free(i as usize, now)).unwrap_or(false) {
                let here = self.ai[s].pos;
                self.s2_drop_inv(s, old, here);
                self.s2_take_item(s, target);
                self.life.wear.remove(&old);
                self.life.rep.replacements += 1;
                let q = self.item_idx.get(&target).map(|&i| self.world.items[i as usize].quality).unwrap_or(0);
                if q >= 1 {
                    self.feel(s, "WearItem", Class::PosMild, 50 + 10 * q as i32, -1, q as i32, now);
                }
            }
            self.ai[s].wake = now + 1;
            return false;
        }
        if now < self.life.next_try[s] || !self.s2_citizen(s) || self.social_no_work(s) || self.world.units[self.ai[s].uidx].squad_id >= 0 {
            return false;
        }
        let ai = &self.ai[s];
        if ai.thirst >= needs::THIRST_CONSIDER || ai.hunger >= needs::HUNGER_CONSIDER || ai.sleep >= needs::SLEEP_CONSIDER {
            return false;
        }
        self.life.next_try[s] = now + 7 * DAY;
        let uidx = self.ai[s].uidx;
        let worn = self.world.units[uidx].inventory.iter().copied().find(|id| {
            self.item_idx.get(id).map(|&i| {
                let it = &self.world.items[i as usize];
                is_clothing(&it.kind) && it.wear >= 1
            })
            .unwrap_or(false)
        });
        let Some(old) = worn else { return false };
        let (kind, sub) = {
            let it = &self.world.items[self.item_idx[&old] as usize];
            (it.kind.clone(), it.subtype.clone())
        };
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        let up = self.nav.xyz(pos);
        let mut best: Option<(i32, i32, u32)> = None;
        for &id in &self.life.spares {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            let it = &self.world.items[i];
            if it.kind != kind || it.subtype != sub || !self.item_free(i, now) {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if !self.nav.walkable(t) || self.nav.comp_root(t) != c {
                continue;
            }
            let d = crate::manhattan(up, self.nav.xyz(t));
            if best.map(|b| (d, id) < (b.0, b.1)).unwrap_or(true) {
                best = Some((d, id, t as u32));
            }
        }
        let Some((_, id, t)) = best else { return false };
        self.imeta[self.item_idx[&id] as usize].reserved = 1;
        self.life.spares.retain(|&x| x != id);
        self.life.errand[s] = (id, old);
        self.ai[s].task = Task::Wander;
        self.request_path(s, Goals::one(t), now);
        true
    }

    /// Take item `id` (loose or in a container) into the inventory of slot `s`.
    pub(crate) fn s2_take_item(&mut self, s: usize, id: i32) {
        let Some(&i) = self.item_idx.get(&id) else { return };
        let idx = i as usize;
        let uid = self.ai[s].id;
        let here = self.world.pos_of(self.ai[s].pos as usize);
        let it = &mut self.world.items[idx];
        let (cont, pos, loose) = (it.container, it.pos, it.container < 0 && it.flags.on_ground);
        it.container = -1;
        it.flags.in_container = false;
        it.flags.on_ground = false;
        it.flags.in_inventory = true;
        it.holder = uid;
        it.pos = here;
        if cont >= 0 {
            if let Some(v) = self.contents.get_mut(&cont) {
                v.retain(|&x| x != id);
            }
        } else if loose {
            if let Some(t) = self.world.idx(pos) {
                self.pile_occ_add(t as u32, -1);
            }
        }
        self.haul_remove(id);
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.push(id);
    }

    /// Drop inventory item `id` of slot `s` on tile `tile` (it is hauled away later).
    pub(crate) fn s2_drop_inv(&mut self, s: usize, id: i32, tile: u32) {
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != id);
        let Some(&i) = self.item_idx.get(&id) else { return };
        let idx = i as usize;
        let p = self.world.pos_of(tile as usize);
        let it = &mut self.world.items[idx];
        it.pos = p;
        it.flags.on_ground = true;
        it.flags.in_inventory = false;
        it.holder = -1;
        self.pile_occ_add(tile, 1);
        if self.haulable(idx) && !self.is_stored(idx) {
            self.haul_add(id);
        }
    }

    // ------------------------------------------------------------------ miasma

    fn miasma_day(&mut self) {
        let first = self.life.first_new_item;
        let mut sources: Vec<u32> = Vec::new();
        let mut ages = std::mem::take(&mut self.life.corpse_age);
        for (i, it) in self.world.items.iter().enumerate() {
            if self.imeta[i].dead {
                continue;
            }
            let corpse = matches!(it.kind.as_str(), "CORPSE" | "CORPSEPIECE");
            let food = matches!(it.kind.as_str(), "REMAINS" | "MEAT" | "FISH" | "FISH_RAW" | "FOOD" | "CHEESE" | "EGG");
            if !corpse && !food {
                continue;
            }
            let loose = it.flags.on_ground && it.container < 0 && it.holder < 0 && !it.flags.in_building && it.building < 0;
            let mut rotting = it.flags.rotten;
            if corpse && it.id >= first && !it.flags.rotten {
                let d = desc_skeletal(&it.desc);
                let age = ages.entry(it.id).or_insert(0);
                *age = age.saturating_add(1);
                rotting = !d && *age >= CORPSE_ROT_DAYS && *age < CORPSE_ROT_DAYS + CORPSE_MIASMA_DAYS;
            }
            if !rotting || !loose {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.world.tiles[t].flags & tile_flags::OUTSIDE != 0 {
                continue;
            }
            sources.push(t as u32);
        }
        let idx = &self.item_idx;
        ages.retain(|id, _| idx.contains_key(id));
        self.life.corpse_age = ages;
        sources.sort_unstable();
        sources.dedup();
        let mut set: HashSet<u32> = HashSet::new();
        let mut frontier: Vec<u32> = Vec::new();
        let mut next: Vec<u32> = Vec::new();
        for &src in &sources {
            if !set.insert(src) {
                continue;
            }
            frontier.clear();
            frontier.push(src);
            for _ in 0..MIASMA_RANGE {
                next.clear();
                for &t in &frontier {
                    let (x, y, z) = self.nav.xyz(t as usize);
                    for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        if nx < 0 || ny < 0 || nx as usize >= self.nav.dx || ny as usize >= self.nav.dy {
                            continue;
                        }
                        let q = self.nav.idx(nx as usize, ny as usize, z);
                        if !self.nav.walkable(q) || self.world.tiles[q].flags & tile_flags::OUTSIDE != 0 {
                            continue;
                        }
                        if set.insert(q as u32) {
                            next.push(q as u32);
                        }
                    }
                }
                std::mem::swap(&mut frontier, &mut next);
            }
        }
        self.life.rep.miasma_sources_max = self.life.rep.miasma_sources_max.max(sources.len());
        self.life.rep.miasma_tiles_max = self.life.rep.miasma_tiles_max.max(set.len());
        self.life.miasma = set;
    }

    // ------------------------------------------------------------------ weather

    fn weather_day(&mut self, now: u64) {
        if !self.world.settings.weather || self.world.calendar.month() / 3 == 3 {
            return;
        }
        let mut r = Rng::for_unit(self.seed, 1, now, 0x4A1);
        if r.chance(1, 5) {
            let start = now + r.below(DAY as u32) as u64;
            self.life.rain_start = start;
            self.life.rain_until = start + (2 + r.below(7) as u64) * DAY / 24;
            self.life.rep.rain_days += 1;
        }
    }

    /// "Rain" thoughts for citizens caught outdoors in the rain (default off: the reference
    /// fort shows none in 17 000 exported thoughts although weather is on).
    pub fn set_rain_thoughts(&mut self, on: bool) {
        self.life.rain_thoughts = on;
    }

    pub(crate) fn lifestyle_report_impl(&self) -> LifestyleReport {
        self.life.rep.clone()
    }

    pub(crate) fn lifestyle_miasma_at(&self, tile: u32) -> bool {
        self.life.miasma.contains(&tile)
    }
}

fn desc_skeletal(desc: &str) -> bool {
    let d = desc.to_ascii_lowercase();
    d.contains("skeleton") || d.contains("bone") || d.contains("skull")
}

#[cfg(test)]
mod tests;
