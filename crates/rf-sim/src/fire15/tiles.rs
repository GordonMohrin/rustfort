//! [fire15] Tile fires: grass, shrubs, saplings and trees (w:Fire, w:Grass, w:Tree), ashes,
//! extinguishing by water and rain, magma melting natural ice (w:Ice).

use super::heat::*;
use super::{FireCell, FIRE_PERIOD, SALT};
use crate::rng::Rng;
use crate::round13_api::WeatherKind;
use crate::Sim;
use rf_core::{tile_flags, Shape, TtMaterial, TtSpecial, NO_MAT};

pub(crate) const K_GRASS: u8 = 0;
pub(crate) const K_SHRUB: u8 = 1;
pub(crate) const K_SAPLING: u8 = 2;
pub(crate) const K_TREE: u8 = 3;

/// Fuel per kind in ticks (ESTIMATE: the wiki gives no durations).
fn fuel_of(kind: u8) -> i32 {
    match kind {
        K_GRASS => 200,
        K_SHRUB => 300,
        K_SAPLING => 100,
        _ => 1200,
    }
}

impl Sim {
    /// What burns on tile `i` (None = nothing flammable). Ramps, constructions, water- or
    /// magma-covered tiles, roots and tiles already burning never count (w:Fire, w:Grass).
    pub(crate) fn fire15_fuel_kind(&self, i: usize) -> Option<u8> {
        let t = &self.world.tiles[i];
        if t.liquid & rf_core::liquid::DEPTH_MASK > 0 {
            return None;
        }
        let info = self.world.tiletypes.get(t.tt);
        let kind = match (info.shape, info.material) {
            (_, TtMaterial::Construction) => return None,
            (Shape::Floor, m) if m.is_grass() => K_GRASS,
            (Shape::Shrub, _) => K_SHRUB,
            (Shape::Sapling, _) => K_SAPLING,
            (Shape::Ramp, _) => return None,
            // Burnt (dead) tree tiles do not burn again (ESTIMATE; otherwise two dead tree
            // tiles would keep re-igniting each other forever).
            (_, TtMaterial::Tree | TtMaterial::Mushroom) if matches!(info.special, TtSpecial::Dead | TtSpecial::SmoothDead) || info.name.starts_with("TreeDead") => return None,
            (_, TtMaterial::Tree | TtMaterial::Mushroom) => K_TREE,
            _ => return None,
        };
        Some(kind)
    }

    /// Set tile `i` on fire if it is flammable at `src` U. Queued in `scratch` and merged at
    /// the end of the pass (or at once outside a pass).
    pub(crate) fn fire15_try_ignite_tile(&mut self, i: usize, src: i32) -> bool {
        let Some(_) = self.fire15_fuel_kind(i) else { return false };
        if self.fire.cells.binary_search_by_key(&(i as u32), |c| c.tile).is_ok() {
            return false;
        }
        let m = self.world.tiles[i].mat;
        if m != NO_MAT {
            let h = self.fire15_heat(m);
            // Dragonfire ignites organics even nether-cap (w:Dragonfire, w:Nether cap).
            let dragon = src >= DRAGONFIRE_TEMP && is_set(h.ignite_point);
            if !burns_at(&h, src) && !dragon && (is_set(h.ignite_point) || is_set(h.mat_fixed_temp)) {
                return false; // nether-cap: fixed 10000, never ignites from ordinary fire
            }
        }
        self.fire.scratch.push(i as u32);
        true
    }

    /// Merge the tiles queued in `scratch` into the burning cells.
    pub(crate) fn fire15_merge_ignitions(&mut self, _now: u64) {
        if self.fire.scratch.is_empty() {
            return;
        }
        let mut q = std::mem::take(&mut self.fire.scratch);
        q.sort_unstable();
        q.dedup();
        for &c in &q {
            let i = c as usize;
            if self.fire.cells.binary_search_by_key(&c, |x| x.tile).is_ok() {
                continue;
            }
            let Some(kind) = self.fire15_fuel_kind(i) else { continue };
            let old_tt = self.world.tiles[i].tt;
            let traffic = ((self.world.tiles[i].flags & tile_flags::TRAFFIC_MASK) >> tile_flags::TRAFFIC_SHIFT) as u8;
            let fuel = fuel_of(kind);
            let cell = FireCell { tile: c, temp: GRASS_FIRE_START, fuel, fuel0: fuel, kind, old_tt, traffic };
            let k = self.fire.cells.binary_search_by_key(&c, |x| x.tile).unwrap_err();
            self.fire.cells.insert(k, cell);
            self.fire.rep.tiles_ignited += 1;
            // Burning tiletype for the renderer (same shape class: no nav rebuild needed).
            let burning = self.fire15_burning_tt(old_tt, kind);
            if burning != u16::MAX {
                self.world.tiles[i].tt = burning;
                self.world.tiles[i].flags |= tile_flags::DIRTY;
                self.fire15_nav_refresh(i);
            }
            // Dwarves avoid fire (w:Fire "Dangers"): restricted traffic while it burns (ESTIMATE).
            self.fire15_set_traffic(i, 3);
        }
        q.clear();
        self.fire.scratch = q;
    }

    /// Burning tiletype for a tile: `Fire` floor, `BurningTree*` for tree tiles.
    fn fire15_burning_tt(&self, old: u16, kind: u8) -> u16 {
        if kind != K_TREE {
            return self.fire.tt.fire;
        }
        let name = &self.world.tiletypes.get(old).name;
        let n = if name.contains("Trunk") {
            "BurningTreeTrunk"
        } else if name.contains("Twigs") {
            "BurningTreeTwigs"
        } else if name.contains("Branch") {
            "BurningTreeBranches"
        } else if name.contains("CapWall") {
            "BurningTreeCapWall"
        } else if name.contains("CapRamp") {
            "BurningTreeCapRamp"
        } else if name.contains("CapFloor") {
            "BurningTreeCapFloor"
        } else {
            return u16::MAX;
        };
        self.world.tiletypes.by_name(n).unwrap_or(u16::MAX)
    }

    /// The tiletype a burnt tile becomes: ashes on soil (grass, shrubs), `TreeDead*` for trees
    /// (ESTIMATE: name substitution).
    fn fire15_burnt_tt(&self, c: &FireCell, i: usize, now: u64) -> u16 {
        if c.kind == K_TREE {
            let name = self.world.tiletypes.get(c.old_tt).name.clone();
            let dead = if name.contains("TreeDead") { name } else { name.replacen("Tree", "TreeDead", 1) };
            return self.world.tiletypes.by_name(&dead).unwrap_or(c.old_tt);
        }
        let mut r = Rng::for_unit(self.seed, i, now, SALT ^ 7);
        let a = self.fire.tt.ashes[r.below(3) as usize];
        if a != u16::MAX {
            return a;
        }
        if self.fire.tt.soil_floor != u16::MAX {
            self.fire.tt.soil_floor
        } else {
            c.old_tt
        }
    }

    /// Fire pass, tiles: heat up, burn fuel, spread on the level, burn out, go out in water or
    /// rain.
    pub(crate) fn fire15_cells_pass(&mut self, now: u64) {
        if self.fire.cells.is_empty() {
            return;
        }
        let layer = self.nav.layer;
        let (dx, dy, dz) = (self.nav.dx as i32, self.nav.dy as i32, self.nav.dz);
        let mut done: Vec<(usize, bool)> = Vec::new(); // (cell index, burnt)
        for k in 0..self.fire.cells.len() {
            let mut c = self.fire.cells[k];
            let i = c.tile as usize;
            let mut r = Rng::for_unit(self.seed, i, now, SALT ^ 2);
            // Water on the tile puts it out; rain or snow outside may (ESTIMATE 1/4 per pass).
            let wet = self.fire15_wet(i);
            let rain = !wet && self.world.tiles[i].flags & tile_flags::OUTSIDE != 0 && matches!(self.weather_at(self.world.pos_of(i)), WeatherKind::Rain | WeatherKind::Snow) && r.chance(1, 4);
            if wet || rain {
                self.fire.rep.extinguished += 1;
                done.push((k, c.fuel * 2 < c.fuel0));
                continue;
            }
            c.temp = (c.temp + 10 * FIRE_PERIOD as i32).min(GRASS_FIRE_MAX);
            c.fuel -= FIRE_PERIOD as i32;
            self.fire.cells[k] = c;
            if c.fuel <= 0 {
                done.push((k, true));
                continue;
            }
            // Spread to the 8 neighbours on the same level (chance 1/8 each, ESTIMATE); burning
            // tree tiles also carry fire up the tree (w:Fire).
            let (x, y, z) = self.nav.xyz(i);
            for oy in -1..=1i32 {
                for ox in -1..=1i32 {
                    if ox == 0 && oy == 0 {
                        continue;
                    }
                    let (nx, ny) = (x as i32 + ox, y as i32 + oy);
                    if nx < 0 || ny < 0 || nx >= dx || ny >= dy {
                        continue;
                    }
                    if !r.chance(1, 8) {
                        continue;
                    }
                    let n = self.nav.idx(nx as usize, ny as usize, z);
                    self.fire15_try_ignite_tile(n, c.temp);
                }
            }
            if c.kind == K_TREE && z + 1 < dz && r.chance(1, 4) {
                let n = i + layer;
                if self.world.tiletypes.get(self.world.tiles[n].tt).material == TtMaterial::Tree {
                    self.fire15_try_ignite_tile(n, c.temp);
                }
            }
        }
        if done.is_empty() {
            return;
        }
        let mut dead_trees: Vec<(i16, i16)> = Vec::new();
        let mut gone_plants: Vec<rf_core::Pos> = Vec::new();
        for &(k, burnt) in &done {
            let c = self.fire.cells[k];
            let i = c.tile as usize;
            let new_tt = if burnt {
                self.fire.rep.tiles_burnt += 1;
                self.fire15_burnt_tt(&c, i, now)
            } else {
                c.old_tt
            };
            self.world.tiles[i].tt = new_tt;
            self.world.tiles[i].flags |= tile_flags::DIRTY;
            if burnt {
                let p = self.world.pos_of(i);
                match c.kind {
                    K_TREE => dead_trees.push((p.x, p.y)),
                    K_SHRUB | K_SAPLING => gone_plants.push(p),
                    _ => {}
                }
                if c.kind != K_TREE {
                    // Burnt ground: soil material from below (the grazing rule's base layer).
                    let (_, _, z) = self.nav.xyz(i);
                    if z > 0 {
                        let m = self.world.tiles[i - layer].mat;
                        if self.world.material(m).map_or(false, |m| m.is_soil) {
                            self.world.tiles[i].mat = m;
                        }
                    }
                    self.fire15_drop_grass_record(i as u32);
                }
            }
            self.fire15_set_traffic(i, c.traffic);
            self.fire15_nav_refresh(i);
        }
        let mut ks: Vec<usize> = done.iter().map(|d| d.0).collect();
        ks.sort_unstable();
        let mut idx = 0;
        let mut kk = 0;
        self.fire.cells.retain(|_| {
            let drop = kk < ks.len() && ks[kk] == idx;
            if drop {
                kk += 1;
            }
            idx += 1;
            !drop
        });
        if !gone_plants.is_empty() {
            gone_plants.sort_unstable();
            self.world.plants.retain(|p| gone_plants.binary_search(&p.pos).is_err());
        }
        if !dead_trees.is_empty() {
            dead_trees.sort_unstable();
            dead_trees.dedup();
            for p in self.world.plants.iter_mut() {
                if p.tree && dead_trees.binary_search(&(p.pos.x, p.pos.y)).is_ok() {
                    p.dead = true;
                }
            }
        }
    }

    /// Forget the grazing record of a burnt tile (its tiletype is no grass any more).
    fn fire15_drop_grass_record(&mut self, t: u32) {
        let Some(g) = self.animals.grass_at.remove(&t) else { return };
        let g = g as usize;
        let last = self.animals.grass.len() - 1;
        self.animals.grass.swap_remove(g);
        if g < last {
            let moved = self.animals.grass[g].tile;
            self.animals.grass_at.insert(moved, g as u32);
        }
    }

    /// Set the traffic bits of one tile (the `cx_set_traffic` steps for a single tile).
    pub(crate) fn fire15_set_traffic(&mut self, i: usize, v: u8) {
        let t = &mut self.world.tiles[i];
        let nf = (t.flags & !tile_flags::TRAFFIC_MASK) | ((v as u16 & 3) << tile_flags::TRAFFIC_SHIFT);
        if nf == t.flags {
            return;
        }
        t.flags = nf;
        let f = self.nav.compute(&self.world.tiles, i);
        self.nav.flags[i] = f;
        self.nav.lm_cost_changed(i);
        if f & crate::nav::WALK != 0 {
            let c = crate::nav::TRAFFIC_COST[(f >> crate::nav::TRAFFIC_SHIFT) as usize];
            self.nav.min_cost = self.nav.min_cost.min(c);
        }
        self.path_cache.clear();
    }

    /// Walkability may have changed (shrub burnt to floor): rebuild nav around the tile.
    fn fire15_nav_refresh(&mut self, i: usize) {
        if self.nav.compute(&self.world.tiles, i) != self.nav.flags[i] {
            self.after_tile_change(i);
        }
    }

    /// Magma next to natural ice melts it: ice walls become open space with 7/7 water
    /// (w:Ice, w:Magma). Ice floors: MISSING (DF restores the depth they froze from).
    pub(crate) fn fire15_melt_ice_around(&mut self, i: usize) {
        let (x, y, z) = self.nav.xyz(i);
        let layer = self.nav.layer as i64;
        let mut ns: [i64; 6] = [-1; 6];
        if x > 0 {
            ns[0] = i as i64 - 1;
        }
        if x + 1 < self.nav.dx {
            ns[1] = i as i64 + 1;
        }
        if y > 0 {
            ns[2] = i as i64 - self.nav.dx as i64;
        }
        if y + 1 < self.nav.dy {
            ns[3] = i as i64 + self.nav.dx as i64;
        }
        if z > 0 {
            ns[4] = i as i64 - layer;
        }
        if z + 1 < self.nav.dz {
            ns[5] = i as i64 + layer;
        }
        for n in ns {
            if n < 0 {
                continue;
            }
            let n = n as usize;
            let info = self.world.tiletypes.get(self.world.tiles[n].tt);
            if info.material == TtMaterial::FrozenLiquid && info.shape == Shape::Wall {
                let p = self.world.pos_of(n);
                self.set_tile(n, Shape::Empty, TtMaterial::Air, 0, NO_MAT);
                self.after_tile_change(n);
                self.fluids_tile_changed(n);
                self.set_liquid(p, 7, false);
                self.fire.rep.ice_melted += 1;
            }
        }
    }
}
