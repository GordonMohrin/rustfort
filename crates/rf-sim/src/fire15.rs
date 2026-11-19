//! [fire15] Temperature, fire and magma (DF v50; wiki Temperature, Fire, Magma, Magma-safe,
//! Fire-safe, Obsidian, Magma flow, Smoke, Steam, Wear, Material definition token).
//!
//! Spec: `docs/mechanics/fire15.md`. The model is cheap: there is no per-tile temperature
//! array. Only "active" things are simulated:
//! - hot items (`Fire15::hot`): items exposed to magma, fire or a heat emitter; each tick they
//!   exchange heat with their environment by DF's rule (difference / SPEC_HEAT, fractions kept)
//!   and melt, boil, ignite and wear (see `items`);
//! - burning tiles (`Fire15::cells`): grass, shrubs, saplings and tree tiles (see `tiles`);
//! - burning units (`Fire15::burning`): burn and melted-fat wounds per tissue layer (see `burn`).
//!
//! Everything except the obsidian edge cases and the magma-workshop check is gated on the
//! world's temperature setting (`settings.temperature`, DF `FEATURE_TEMPERATURE`; the player
//! can override it, see `fire15_api`). With temperature off, the legacy fluids behaviour
//! (10-tick magma kill, instant destruction of non-magma-safe items in magma) stays.
//!
//! Determinism: all state is in sorted vectors and serialized; passes run in tile / item id /
//! slot order; random numbers come from `Rng::for_unit(seed, key, tick, SALT)`.

pub(crate) mod burn;
pub mod heat;
pub(crate) mod items;
pub(crate) mod tiles;
#[cfg(test)]
mod tests;

use crate::fire15_api::FireReport;
use crate::{Kind, Sim};
use heat::*;
use rf_core::{liquid, tile_flags, MaterialHeat, Pos, NO_MAT};
use serde::{Deserialize, Serialize};

/// Fire pass period and phase (tile fires, burning units, smoke, emitters).
pub(crate) const FIRE_PERIOD: u64 = 10;
pub(crate) const FIRE_PHASE: u64 = 4;
/// Scans (items lying in magma, heat-emitting creatures, magma workshops) every 100 ticks.
pub(crate) const SCAN_PERIOD: u64 = 100;
pub(crate) const SCAN_PHASE: u64 = 29;
/// Smoke thoughts.
pub(crate) const SMOKE_PHASE: u64 = 83;
pub(crate) const SALT: u64 = 0xF15E_0015;

/// An item being heated or cooled (or burning).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct HotItem {
    pub id: i32,
    pub t: Temp,
    /// Wear points since the last wear level (0..806 400).
    pub wear_pts: u32,
    pub burning: bool,
    /// Ticks at equilibrium with an ordinary environment; dropped at 100.
    pub idle: u16,
    /// Was solid / below boiling when it entered (liquids do not "melt").
    pub solid: bool,
    pub below_boil: bool,
}

/// A burning tile.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FireCell {
    pub tile: u32,
    pub temp: i32,
    /// Ticks of burning left.
    pub fuel: i32,
    pub fuel0: i32,
    pub kind: u8,
    /// Tiletype before it caught fire.
    pub old_tt: u16,
    /// Traffic bits before (the burning tile is set to restricted).
    pub traffic: u8,
}

/// A unit on fire.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BurningUnit {
    pub slot: u32,
    /// Burning body parts (bit = part index; parts beyond 63 fold onto bit 63).
    pub parts: u64,
    pub since: u64,
    pub magma: bool,
}

/// Resolved tiletype ids for fire (absent ones are `u16::MAX`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct FireTiletypes {
    pub fire: u16,
    pub ashes: [u16; 3],
    pub soil_floor: u16,
}

impl Default for FireTiletypes {
    fn default() -> Self {
        FireTiletypes { fire: u16::MAX, ashes: [u16::MAX; 3], soil_floor: u16::MAX }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Fire15 {
    /// Player toggle; None = `World::settings.temperature`.
    pub enabled: Option<bool>,
    pub hot: Vec<HotItem>,
    pub cells: Vec<FireCell>,
    pub burning: Vec<BurningUnit>,
    /// Smoke: (tile, ticks left), sorted by tile.
    pub smoke: Vec<(u32, u16)>,
    /// Per slot: next tick a smoke thought may come.
    pub next_smoke: Vec<u64>,
    /// Heat sources for things on a tile, rebuilt every fire pass: (tile, temperature), sorted.
    pub burn_tiles: Vec<(u32, i32)>,
    /// Heat-emitting creatures (slot, fixed temperature), refreshed by the scan.
    pub emitters: Vec<(u32, i32)>,
    /// Magma workshops (building id, had magma at the last check), and the building count
    /// the list was built for.
    pub shops: Vec<(i32, bool)>,
    pub shops_n: usize,
    pub rep: FireReport,
    #[serde(skip)]
    pub ready: bool,
    #[serde(skip)]
    pub heat: Vec<MaterialHeat>,
    /// Per combat body: heat properties of each flat tissue layer.
    #[serde(skip)]
    pub bodies: Vec<Vec<MaterialHeat>>,
    /// Races with heat-emitting tissue (fixed temperature above ignition): (race, temp).
    #[serde(skip)]
    pub emitter_races: Vec<(String, i32)>,
    #[serde(skip)]
    pub tt: FireTiletypes,
    #[serde(skip)]
    pub scratch: Vec<u32>,
}

impl Sim {
    /// Temperature simulated at all (world setting, player override).
    #[inline]
    pub(crate) fn fire15_on(&self) -> bool {
        self.fire.enabled.unwrap_or(self.world.settings.temperature)
    }

    /// Build the caches (heat per palette entry, tiletypes, emitter races).
    pub(crate) fn fire15_init(&mut self) {
        self.fire.heat = self.world.materials.iter().map(heat_of).collect();
        self.fire.bodies.clear();
        let tt = &self.world.tiletypes;
        let id = |n: &str| tt.by_name(n).unwrap_or(u16::MAX);
        self.fire.tt = FireTiletypes {
            fire: id("Fire"),
            ashes: [id("Ashes1"), id("Ashes2"), id("Ashes3")],
            soil_floor: tt
                .find(rf_core::Shape::Floor, rf_core::TtMaterial::Soil, rf_core::TtSpecial::Normal, 1)
                .unwrap_or(u16::MAX),
        };
        let mut er = Vec::new();
        for cr in &self.world.creature_raws {
            let mut best = 0;
            for c in &cr.castes {
                for p in &c.body_parts {
                    for l in &p.layers {
                        if l.mat != NO_MAT {
                            let h = self.fire15_heat(l.mat);
                            if is_set(h.mat_fixed_temp) && h.mat_fixed_temp > GRASS_FIRE_START {
                                best = best.max(h.mat_fixed_temp);
                            }
                        }
                    }
                }
            }
            if best > 0 {
                er.push((cr.token.clone(), best));
            }
        }
        self.fire.emitter_races = er;
        self.fire.ready = true;
    }

    /// [saves11] After an exact load: caches rebuild lazily.
    pub(crate) fn fire15_after_load(&mut self) {
        self.fire.ready = false;
    }

    /// Heat properties of a palette material (fallback for unknown ones).
    pub(crate) fn fire15_heat(&self, mat: u16) -> MaterialHeat {
        if let Some(h) = self.fire.heat.get(mat as usize) {
            return *h;
        }
        match self.world.material(mat) {
            Some(m) => heat_of(m),
            None => MaterialHeat { spec_heat: NONE_TEMP, heatdam_point: NONE_TEMP, colddam_point: NONE_TEMP, ignite_point: NONE_TEMP, melting_point: NONE_TEMP, boiling_point: NONE_TEMP, mat_fixed_temp: NONE_TEMP },
        }
    }

    /// Ambient temperature of tile `i`: outside follows the weather API ([weather13]),
    /// underground is 10015 (w:Temperature).
    pub(crate) fn fire15_ambient(&self, i: usize) -> i32 {
        if self.world.tiles[i].flags & tile_flags::OUTSIDE != 0 {
            self.temperature_at(self.world.pos_of(i))
        } else {
            UNDERGROUND_TEMP
        }
    }

    /// Temperature a thing on tile `i` is exposed to (w:Temperature, w:Magma, w:Fire).
    pub(crate) fn fire15_env(&self, i: usize) -> i32 {
        let t = &self.world.tiles[i];
        let d = t.liquid & liquid::DEPTH_MASK;
        if d > 0 {
            if t.liquid & liquid::MAGMA != 0 {
                return MAGMA_TEMP;
            }
            return self.fire15_ambient(i).min(WATER_BOIL);
        }
        let mut e = self.fire15_ambient(i);
        let k = i as u32;
        if let Ok(c) = self.fire.cells.binary_search_by_key(&k, |c| c.tile) {
            e = e.max(self.fire.cells[c].temp);
        }
        if let Ok(c) = self.fire.burn_tiles.binary_search_by_key(&k, |c| c.0) {
            e = e.max(self.fire.burn_tiles[c].1);
        }
        e
    }

    /// Water (not magma) in tile `i`.
    #[inline]
    pub(crate) fn fire15_wet(&self, i: usize) -> bool {
        let t = &self.world.tiles[i];
        t.liquid & liquid::DEPTH_MASK > 0 && t.liquid & liquid::MAGMA == 0
    }

    #[inline]
    pub(crate) fn fire15_magma(&self, i: usize) -> bool {
        let t = &self.world.tiles[i];
        t.liquid & liquid::DEPTH_MASK > 0 && t.liquid & liquid::MAGMA != 0
    }

    /// Hook from `tick_inner` right after the liquids and carts.
    pub(crate) fn fire_tick(&mut self, now: u64) {
        if self.fluids_has_obsidian_cells() {
            self.fire15_obsidian(now); // temperature-independent fluid rule
        }
        if now % SCAN_PERIOD == SCAN_PHASE {
            self.fire15_magma_shops(now); // temperature-independent (w:Magma forge)
        }
        if !self.fire15_on() {
            return;
        }
        let t0 = std::time::Instant::now();
        if !self.fire.ready || self.fire.heat.len() != self.world.materials.len() {
            self.fire15_init();
        }
        if !self.fire.hot.is_empty() {
            self.fire15_hot_step(now);
        }
        if now % SCAN_PERIOD == SCAN_PHASE {
            self.fire15_scan(now);
        }
        if now % FIRE_PERIOD == FIRE_PHASE
            && !(self.fire.cells.is_empty() && self.fire.burning.is_empty() && self.fire.hot.is_empty() && self.fire.emitters.is_empty() && self.fire.smoke.is_empty() && self.fire.burn_tiles.is_empty())
        {
            self.fire15_pass(now);
        }
        if now % SCAN_PERIOD == SMOKE_PHASE && !self.fire.smoke.is_empty() {
            self.fire15_smoke_thoughts(now);
        }
        self.fire.rep.time_s += t0.elapsed().as_secs_f64();
    }

    /// The fire pass (every `FIRE_PERIOD` ticks while anything burns or is hot).
    fn fire15_pass(&mut self, now: u64) {
        self.fire.scratch.clear();
        self.fire15_cells_pass(now);
        self.fire15_items_pass(now);
        self.fire15_emitters_pass(now);
        self.fire15_units_pass(now);
        self.fire15_merge_ignitions(now);
        self.fire15_smoke_pass();
        self.fire15_rebuild_burn_tiles();
        let r = &mut self.fire.rep;
        r.cells_max = r.cells_max.max(self.fire.cells.len() as u32);
        r.hot_items_max = r.hot_items_max.max(self.fire.hot.len() as u32);
    }

    /// Every 100 ticks: items that lie in magma heat up; heat-emitting creatures.
    fn fire15_scan(&mut self, _now: u64) {
        if self.fluids_magma_volume() > 0 {
            self.fire15_scan_magma_items();
        }
        self.fire.emitters.clear();
        if !self.fire.emitter_races.is_empty() {
            for s in 0..self.ai.len() {
                if self.ai[s].kind == Kind::Dead {
                    continue;
                }
                let race = &self.world.units[self.ai[s].uidx].race;
                if let Some(&(_, t)) = self.fire.emitter_races.iter().find(|e| &e.0 == race) {
                    self.fire.emitters.push((s as u32, t));
                }
            }
        }
    }

    /// Heat sources for things on a tile: burning items, burning units, heat emitters
    /// (their tile and the 8 around, w:Creature token FIXED_TEMP "all adjacent tiles").
    fn fire15_rebuild_burn_tiles(&mut self) {
        let mut v = std::mem::take(&mut self.fire.burn_tiles);
        v.clear();
        for k in 0..self.fire.hot.len() {
            let h = self.fire.hot[k];
            if !h.burning {
                continue;
            }
            if let Some(&ix) = self.item_idx.get(&h.id) {
                if let Some(i) = self.world.idx(self.world.items[ix as usize].pos) {
                    v.push((i as u32, h.t.whole()));
                }
            }
        }
        for b in &self.fire.burning {
            v.push((self.ai[b.slot as usize].pos, if b.magma { MAGMA_TEMP } else { GRASS_FIRE_MAX }));
        }
        let (dx, dy) = (self.nav.dx as i32, self.nav.dy as i32);
        for &(s, t) in &self.fire.emitters {
            let i = self.ai[s as usize].pos as usize;
            let (x, y, z) = self.nav.xyz(i);
            for oy in -1..=1i32 {
                for ox in -1..=1i32 {
                    let (nx, ny) = (x as i32 + ox, y as i32 + oy);
                    if nx >= 0 && ny >= 0 && nx < dx && ny < dy {
                        v.push((self.nav.idx(nx as usize, ny as usize, z) as u32, t));
                    }
                }
            }
        }
        v.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        v.dedup_by_key(|e| e.0);
        self.fire.burn_tiles = v;
    }

    /// Smoke: every burning tile, item and unit refreshes smoke on its tile and the open tiles
    /// next to it and above (w:Smoke; ttl and radius ESTIMATE).
    fn fire15_smoke_pass(&mut self) {
        const TTL: u16 = 700;
        let mut sm = std::mem::take(&mut self.fire.smoke);
        for e in sm.iter_mut() {
            e.1 = e.1.saturating_sub(FIRE_PERIOD as u16);
        }
        sm.retain(|e| e.1 > 0);
        let n0 = sm.len();
        let add = |i: u32, v: &mut Vec<(u32, u16)>| v.push((i, TTL));
        let mut src: Vec<u32> = self.fire.cells.iter().map(|c| c.tile).collect();
        for h in &self.fire.hot {
            if h.burning {
                if let Some(&ix) = self.item_idx.get(&h.id) {
                    if let Some(i) = self.world.idx(self.world.items[ix as usize].pos) {
                        src.push(i as u32);
                    }
                }
            }
        }
        src.extend(self.fire.burning.iter().map(|b| self.ai[b.slot as usize].pos));
        let layer = self.nav.layer;
        for &i in &src {
            add(i, &mut sm);
            let (x, y, z) = self.nav.xyz(i as usize);
            for (ox, oy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i32 + ox, y as i32 + oy);
                if nx >= 0 && ny >= 0 && (nx as usize) < self.nav.dx && (ny as usize) < self.nav.dy {
                    let q = self.nav.idx(nx as usize, ny as usize, z);
                    if !self.nav.shape(self.world.tiles[q].tt).is_solid() {
                        add(q as u32, &mut sm);
                    }
                }
            }
            if z + 1 < self.nav.dz && !self.nav.shape(self.world.tiles[i as usize].tt).is_solid() {
                let q = i as usize + layer;
                if !self.nav.shape(self.world.tiles[q].tt).is_solid() && !self.nav.shape(self.world.tiles[q].tt).has_floor() {
                    add(q as u32, &mut sm);
                }
            }
        }
        if sm.len() != n0 || n0 > 0 {
            sm.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
            sm.dedup_by_key(|e| e.0);
        }
        self.fire.smoke = sm;
    }

    /// Units underground on a smoke tile: "Smoke" thought, at most every 2 days (w:Smoke; the
    /// miasma pattern of `lifestyle`).
    fn fire15_smoke_thoughts(&mut self, now: u64) {
        if self.fire.next_smoke.len() < self.ai.len() {
            self.fire.next_smoke.resize(self.ai.len(), 0);
        }
        for s in 0..self.ai.len() {
            if self.ai[s].kind != Kind::Worker || self.fire.next_smoke[s] > now {
                continue;
            }
            let i = self.ai[s].pos;
            if self.fire.smoke.binary_search_by_key(&i, |e| e.0).is_err() || self.world.tiles[i as usize].flags & tile_flags::OUTSIDE != 0 {
                continue;
            }
            self.fire.next_smoke[s] = now + 2 * rf_core::TICKS_PER_DAY as u64;
            self.fire.rep.smoke_thoughts += 1;
            self.feel(s, "Smoke", crate::social::Class::NegMild, 80, -1, 0, now);
        }
    }

    /// Obsidian casting edge cases (w:Obsidian): creatures in the cast tile die unless caged;
    /// non-magma-safe items are destroyed, magma-safe ones stay entombed.
    fn fire15_obsidian(&mut self, now: u64) {
        let mut cells = self.fluids_take_obsidian_cells();
        cells.sort_unstable();
        cells.dedup();
        for s in 0..self.ai.len() {
            if self.ai[s].kind == Kind::Dead || cells.binary_search(&self.ai[s].pos).is_err() {
                continue;
            }
            if self.world.units[self.ai[s].uidx].flags.caged {
                continue;
            }
            self.fire.rep.obsidian_kills += 1;
            self.combat_kill(s, now, "has been encased in obsidian");
        }
        let mut doomed = Vec::new();
        for (ix, it) in self.world.items.iter().enumerate() {
            if self.imeta[ix].dead || it.container >= 0 || it.holder >= 0 {
                continue;
            }
            let Some(i) = self.world.idx(it.pos) else { continue };
            if cells.binary_search(&(i as u32)).is_err() {
                continue;
            }
            if !magma_safe(&self.fire15_heat_any(it.mat)) {
                doomed.push(it.id);
            }
        }
        for id in doomed {
            self.destroy_item(id);
            self.fire.rep.items_melted += 1;
        }
        cells.clear();
        self.fluids_return_obsidian_cells(cells);
    }

    /// Heat of a material even before the cache is built (temperature off).
    pub(crate) fn fire15_heat_any(&self, mat: u16) -> MaterialHeat {
        if !self.fire.heat.is_empty() {
            return self.fire15_heat(mat);
        }
        match self.world.material(mat) {
            Some(m) => heat_of(m),
            None => self.fire15_heat(mat),
        }
    }

    /// Magma workshops lose their jobs the moment magma under them drops below 4/7
    /// (w:Magma forge). Runs with temperature off too.
    fn fire15_magma_shops(&mut self, _now: u64) {
        if self.fire.shops_n != self.world.buildings.len() || (self.ticks % rf_core::TICKS_PER_DAY as u64) < SCAN_PERIOD {
            let mut v: Vec<(i32, bool)> = Vec::new();
            for (bi, b) in self.world.buildings.iter().enumerate() {
                if crate::industry::is_magma_shop(&b.subtype) && b.built {
                    let old = self.fire.shops.iter().find(|e| e.0 == b.id).map_or_else(|| self.magma_shop_ok(bi), |e| e.1);
                    v.push((b.id, old));
                }
            }
            v.sort_unstable();
            self.fire.shops = v;
            self.fire.shops_n = self.world.buildings.len();
        }
        for k in 0..self.fire.shops.len() {
            let (bid, was) = self.fire.shops[k];
            let Some(bi) = self.world.buildings.iter().position(|b| b.id == bid) else { continue };
            let ok = self.magma_shop_ok(bi);
            if was && !ok {
                self.cancel_jobs_at(bid);
                self.fire.rep.magma_jobs_cancelled += 1;
            }
            self.fire.shops[k].1 = ok;
        }
    }

    /// Part of `state_hash`; mixes nothing while the module is idle.
    pub(crate) fn fire15_hash(&self, mix: &mut dyn FnMut(u64)) {
        let f = &self.fire;
        if f.hot.is_empty() && f.cells.is_empty() && f.burning.is_empty() && f.smoke.is_empty() && f.rep.is_zero() {
            return;
        }
        mix(0xF15E);
        for h in &f.hot {
            mix((h.id as u64) << 32 ^ h.t.0 as u64);
            mix((h.wear_pts as u64) << 1 | h.burning as u64);
        }
        for c in &f.cells {
            mix((c.tile as u64) << 32 ^ (c.temp as u64) << 16 ^ c.fuel as u64);
        }
        for b in &f.burning {
            mix((b.slot as u64) << 1 ^ b.parts.rotate_left(17) ^ b.magma as u64);
        }
        mix(f.smoke.len() as u64);
        let r = &f.rep;
        mix(r.tiles_ignited ^ r.tiles_burnt << 20 ^ r.items_ignited << 40);
        mix(r.items_burned_up ^ r.items_melted << 20 ^ r.items_boiled << 40);
        mix(r.units_ignited ^ r.units_burned_to_death << 20 ^ r.burn_wounds << 32);
        mix(r.extinguished ^ r.obsidian_kills << 32 ^ r.magma_jobs_cancelled << 48);
    }

    /// Position of a unit slot.
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn fire15_unit_pos(&self, s: usize) -> Pos {
        self.world.pos_of(self.ai[s].pos as usize)
    }
}
