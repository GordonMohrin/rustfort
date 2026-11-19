//! [fire15] Hot items: heat exchange, melting, boiling, ignition, burning wear (w:Temperature,
//! w:Fire, w:Wear, w:Magma, w:Material definition token).

use super::heat::*;
use super::HotItem;
use crate::Sim;

/// Liquid item kinds: they boil but do not "melt".
fn liquid_kind(kind: &str) -> bool {
    matches!(kind, "DRINK" | "LIQUID_MISC")
}

impl Sim {
    /// Put item `id` (and what it contains) into the hot set. `burning` sets it on fire;
    /// `at_least` raises its temperature (heat blast). Returns false if the item is gone.
    pub(crate) fn fire15_heat_item(&mut self, id: i32, burning: bool, at_least: i32) -> bool {
        let Some(&ix) = self.item_idx.get(&id) else { return false };
        let ix = ix as usize;
        if self.imeta[ix].dead {
            return false;
        }
        let (mat, pos) = (self.world.items[ix].mat, self.world.items[ix].pos);
        let h = self.fire15_heat(mat);
        let k = match self.fire.hot.binary_search_by_key(&id, |e| e.id) {
            Ok(k) => k,
            Err(k) => {
                let t0 = if is_set(h.mat_fixed_temp) {
                    h.mat_fixed_temp
                } else {
                    match self.world.idx(pos) {
                        Some(i) => self.fire15_ambient(i),
                        None => UNDERGROUND_TEMP,
                    }
                };
                let liquid = liquid_kind(&self.world.items[ix].kind);
                let e = HotItem {
                    id,
                    t: Temp::from_whole(t0),
                    wear_pts: 0,
                    burning: false,
                    idle: 0,
                    solid: !liquid && (!is_set(h.melting_point) || t0 < h.melting_point),
                    below_boil: !is_set(h.boiling_point) || t0 < h.boiling_point,
                };
                self.fire.hot.insert(k, e);
                // Contents follow their container's temperature (w:Temperature).
                if let Some(inner) = self.contents.get(&id) {
                    let inner = inner.clone();
                    for c in inner {
                        self.fire15_heat_item(c, false, 0);
                    }
                }
                self.fire.hot.binary_search_by_key(&id, |e| e.id).unwrap_or(k)
            }
        };
        let e = &mut self.fire.hot[k];
        if at_least > 0 && e.t.whole() < at_least && !is_set(h.mat_fixed_temp) {
            e.t = Temp::from_whole(at_least);
        }
        if burning && !e.burning && flammable(&h) {
            e.burning = true;
            self.fire.rep.items_ignited += 1;
        }
        true
    }

    /// One tick of every hot item (w:Temperature "Temperature transfer").
    pub(crate) fn fire15_hot_step(&mut self, _now: u64) {
        let mut gone = false;
        for k in 0..self.fire.hot.len() {
            let mut e = self.fire.hot[k];
            let Some(&ix) = self.item_idx.get(&e.id) else {
                self.fire.hot[k].idle = u16::MAX;
                gone = true;
                continue;
            };
            let ix = ix as usize;
            if self.imeta[ix].dead {
                self.fire.hot[k].idle = u16::MAX;
                gone = true;
                continue;
            }
            let it = &self.world.items[ix];
            let (mat, pos, container, artifact) = (it.mat, it.pos, it.container, it.flags.artifact || it.quality >= 6);
            let h = self.fire15_heat(mat);
            let tile = self.world.idx(pos);
            let mut env = match tile {
                Some(i) => self.fire15_env(i),
                None => UNDERGROUND_TEMP,
            };
            let ambient = match tile {
                Some(i) => self.fire15_ambient(i),
                None => UNDERGROUND_TEMP,
            };
            if container >= 0 {
                if let Ok(c) = self.fire.hot.binary_search_by_key(&container, |x| x.id) {
                    env = self.fire.hot[c].t.whole();
                }
            }
            let in_water = container < 0 && tile.map_or(false, |i| self.fire15_wet(i));
            if e.burning && in_water {
                // Water puts burning items out unless they are inside a bin (w:Fire Artifacts).
                e.burning = false;
                self.fire.rep.extinguished += 1;
            }
            if e.burning {
                env = env.max(h.ignite_point + BURN_EQUILIBRIUM);
            }
            if is_set(h.mat_fixed_temp) {
                e.t = Temp::from_whole(h.mat_fixed_temp);
            } else {
                e.t.step(env, h.spec_heat);
            }
            let t = e.t.whole();
            if is_set(h.boiling_point) && t >= h.boiling_point && e.below_boil {
                self.fire.hot[k].idle = u16::MAX;
                gone = true;
                self.fire.rep.items_boiled += 1;
                self.fire.rep.steam_puffs += 1;
                self.destroy_item(e.id);
                continue;
            }
            if is_set(h.melting_point) && t >= h.melting_point && e.solid {
                // MISSING: no "glob of molten <metal>" item (it vanishes soon anyway, w:Magma).
                self.fire.hot[k].idle = u16::MAX;
                gone = true;
                self.fire.rep.items_melted += 1;
                self.destroy_item(e.id);
                continue;
            }
            if !e.burning && !in_water && is_set(h.ignite_point) && t >= h.ignite_point && flammable(&h) {
                e.burning = true;
                self.fire.rep.items_ignited += 1;
            }
            // Wear (w:Wear): burning 10 points per tick; above HEATDAM / below COLDDAM
            // accelerated (ESTIMATE: 10 points per degree per tick).
            let mut pts: u64 = if e.burning { BURN_WEAR as u64 } else { 0 };
            if is_set(h.heatdam_point) && t > h.heatdam_point {
                pts += 10 * (t - h.heatdam_point) as u64;
            }
            if is_set(h.colddam_point) && t < h.colddam_point {
                pts += 10 * (h.colddam_point - t) as u64;
            }
            if pts > 0 && !artifact {
                let mut w = e.wear_pts as u64 + pts;
                let mut destroyed = false;
                while w >= WEAR_PER_LEVEL as u64 {
                    w -= WEAR_PER_LEVEL as u64;
                    let item = &mut self.world.items[ix];
                    item.wear += 1;
                    if item.wear > 3 {
                        destroyed = true;
                        break;
                    }
                }
                if destroyed {
                    self.fire.hot[k].idle = u16::MAX;
                    gone = true;
                    self.fire.rep.items_burned_up += 1;
                    self.destroy_item(e.id);
                    continue;
                }
                e.wear_pts = w as u32;
            }
            // Leave the set when cooled back to an ordinary environment.
            // Items whose temperature cannot change (fixed temperature such as nether-cap, or
            // SPEC_HEAT NONE) never reach the environment's value: they leave as well.
            let pinned = is_set(h.mat_fixed_temp) || !is_set(h.spec_heat);
            if !e.burning && pts == 0 && env <= ambient.max(UNDERGROUND_TEMP) + 1 && ((t - env).abs() <= 1 || pinned) {
                e.idle += 1;
                if e.idle >= 100 {
                    self.fire.hot[k].idle = u16::MAX;
                    gone = true;
                    continue;
                }
            } else {
                e.idle = 0;
            }
            self.fire.hot[k] = e;
        }
        if gone {
            self.fire.hot.retain(|e| e.idle != u16::MAX);
        }
    }

    /// Fire pass, items: loose items on burning tiles heat up; burning loose items set their
    /// tile on fire (chance 1/2 per pass, ESTIMATE).
    pub(crate) fn fire15_items_pass(&mut self, now: u64) {
        // The scan walks every item: with real fire every pass, with only heat emitters (fire
        // men, which can live on a map for years) once per scan period.
        let real_fire = !self.fire.cells.is_empty() || !self.fire.burning.is_empty() || self.fire.hot.iter().any(|e| e.burning);
        let emitters_only = !real_fire && !self.fire.burn_tiles.is_empty();
        if real_fire || (emitters_only && now % super::SCAN_PERIOD == super::FIRE_PHASE) {
            let mut add = Vec::new();
            for (ix, it) in self.world.items.iter().enumerate() {
                if self.imeta[ix].dead || it.container >= 0 {
                    continue;
                }
                let Some(i) = self.world.idx(it.pos) else { continue };
                let k = i as u32;
                let on_cell = self.fire.cells.binary_search_by_key(&k, |c| c.tile).is_ok();
                let on_src = self.fire.burn_tiles.binary_search_by_key(&k, |c| c.0).map_or(false, |c| self.fire.burn_tiles[c].1 >= GRASS_FIRE_START);
                if (on_cell || on_src) && self.fire.hot.binary_search_by_key(&it.id, |e| e.id).is_err() {
                    add.push(it.id);
                }
            }
            for id in add {
                self.fire15_heat_item(id, false, 0);
            }
        }
        for k in 0..self.fire.hot.len() {
            let e = self.fire.hot[k];
            if !e.burning {
                continue;
            }
            let Some(&ix) = self.item_idx.get(&e.id) else { continue };
            let it = &self.world.items[ix as usize];
            if it.container >= 0 {
                continue;
            }
            let Some(i) = self.world.idx(it.pos) else { continue };
            let mut r = crate::rng::Rng::for_unit(self.seed, i, now, super::SALT ^ 1);
            if r.chance(1, 2) {
                self.fire15_try_ignite_tile(i, e.t.whole());
            }
        }
    }

    /// Fluids hand-off: cells that just got magma (temperature on). Items there heat up
    /// gradually instead of vanishing (w:Magma, w:Fire); flammable tiles ignite; ice melts.
    pub(crate) fn fire15_magma_cells(&mut self, cells: &[u32], _now: u64) {
        if !self.fire.ready || self.fire.heat.len() != self.world.materials.len() {
            self.fire15_init();
        }
        let mut add = Vec::new();
        for (ix, it) in self.world.items.iter().enumerate() {
            if self.imeta[ix].dead || it.container >= 0 || it.holder >= 0 {
                continue;
            }
            let Some(i) = self.world.idx(it.pos) else { continue };
            if cells.binary_search(&(i as u32)).is_ok() && self.fire15_magma(i) {
                add.push(it.id);
            }
        }
        for id in add {
            self.fire15_heat_item(id, false, 0);
        }
        for &c in cells {
            let i = c as usize;
            if self.fire15_magma(i) {
                self.fire15_melt_ice_around(i);
            }
        }
    }

    /// Scan (every 100 ticks while the map has magma): loose items lying in magma.
    pub(crate) fn fire15_scan_magma_items(&mut self) {
        let mut add = Vec::new();
        for (ix, it) in self.world.items.iter().enumerate() {
            if self.imeta[ix].dead || it.container >= 0 || it.holder >= 0 {
                continue;
            }
            let Some(i) = self.world.idx(it.pos) else { continue };
            if self.fire15_magma(i) && self.fire.hot.binary_search_by_key(&it.id, |e| e.id).is_err() {
                add.push(it.id);
            }
        }
        for id in add {
            self.fire15_heat_item(id, false, 0);
        }
    }

    /// Temperature of a hot item (None = not tracked: ambient).
    pub(crate) fn fire15_item_temp(&self, id: i32) -> Option<i32> {
        self.fire.hot.binary_search_by_key(&id, |e| e.id).ok().map(|k| self.fire.hot[k].t.whole())
    }
}
