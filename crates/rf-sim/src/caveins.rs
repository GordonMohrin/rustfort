//! Cave-ins (round 7).
//!
//! # DF rules implemented (wiki: Cave-in)
//! - After a tile is dug, channeled, ramped or a construction is removed, the terrain around
//!   it is checked for support. Support runs "along the X, Y, and Z axes" only: diagonal
//!   neighbours never support. Walls, floors, stairs, ramps, fortifications, constructions,
//!   boulders/pebbles, tree tiles and support buildings connect to each other; up stairs,
//!   fortifications and supports also make "an invisible floor on the level above them". A ramp does
//!   not support the tile directly above it. Bridges are buildings, not tiles, so they never
//!   support.
//! - A connected chunk is supported when it reaches the bottom level of the map or a map
//!   edge. A chunk that reaches neither falls: its tiles become open space and it "crashes
//!   down", stopping on the first floor, stair, ramp or wall below. Where it lands on open
//!   space over a wall, it leaves a natural floor of the highest fallen terrain; natural stone
//!   walls leave boulders ("natural stone walls pile up").
//! - "Any creature caught directly underneath (on the same tile underneath) a cave-in is
//!   killed". Creatures standing on the falling chunk fall with it and take the fall damage of
//!   the levels fallen (same rule as falling off a retracted bridge). The dust knocks out
//!   creatures near the landing tiles. Items in the way are destroyed except boulders, rough
//!   gems, corpses and artifacts; items on the falling chunk land with it.
//! - DF announces "A section of the cavern has collapsed!".
//!
//! # Estimates
//! - The search is a depth-first flood fill from the changed tile's neighbours, downwards
//!   first, capped at [`SEARCH_CAP`] tiles: a chunk larger than that counts as supported
//!   (DF also bounds its check). Typical digs in solid rock reach the bottom in a few hundred
//!   steps.
//! - Fall stops on floors (DF: falls through floors and ramps, destroying them); one boulder
//!   per natural stone wall that fell, at most 8 per column; dust radius 2 tiles, unconscious
//!   for 60 ticks; no dust thoughts or flinging.

use crate::{AnnouncementKind, Kind, Sim};
use rf_core::{Pos, Shape, TtMaterial, NO_MAT};
use std::collections::HashSet;

/// Most tiles one support search visits before assuming support.
pub const SEARCH_CAP: usize = 4096;
const BOULDERS_PER_COLUMN: u32 = 8;
const DUST_RADIUS: i32 = 2;
const DUST_TICKS: u64 = 60;

/// Cave-in counters (see [`Sim::cavein_report`]).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct CaveInReport {
    /// Support checks after digging or deconstruction.
    pub checks: u64,
    /// Tiles visited by the support searches.
    pub nodes: u64,
    pub collapses: u32,
    pub tiles_fallen: u32,
    pub killed: u32,
    pub fell: u32,
    pub knocked_out: u32,
    pub boulders: u32,
    pub items_destroyed: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct CaveIns {
    pub rep: CaveInReport,
    /// Tiles of support buildings, rebuilt when the building count changes.
    supports: HashSet<u32>,
    known_buildings: usize,
    pub disabled: bool,
}

fn stone_like(m: TtMaterial) -> bool {
    matches!(m, TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::LavaStone | TtMaterial::Feature)
}

impl Sim {
    /// Cave-in statistics.
    pub fn cavein_report(&self) -> CaveInReport {
        self.cx.cave.rep.clone()
    }

    /// Switch cave-ins on or off (default on, like DF).
    pub fn set_caveins(&mut self, on: bool) {
        self.cx.cave.disabled = !on;
    }

    fn cv_shape(&self, i: usize) -> Shape {
        self.world.tiletypes.get(self.world.tiles[i].tt).shape
    }

    /// Does tile `i` take part in support (anything but open space)?
    fn cv_connects(&self, i: usize) -> bool {
        match self.cv_shape(i) {
            Shape::Empty | Shape::RampTop => {
                // Up stairs, fortifications and supports make "an invisible floor on the level
                // above them".
                let layer = self.nav.layer;
                self.cx.cave.supports.contains(&(i as u32))
                    || (i >= layer
                        && (matches!(self.cv_shape(i - layer), Shape::StairUp | Shape::StairUpdown | Shape::Fortification)
                            || self.cx.cave.supports.contains(&((i - layer) as u32))))
            }
            _ => true,
        }
    }

    fn cv_anchor(&self, i: usize) -> bool {
        let (x, y, z) = self.nav.xyz(i);
        z == 0 || x == 0 || y == 0 || x + 1 == self.nav.dx || y + 1 == self.nav.dy || self.cv_shape(i) == Shape::None
    }

    fn cv_refresh_supports(&mut self) {
        if self.world.buildings.len() == self.cx.cave.known_buildings {
            return;
        }
        self.cx.cave.known_buildings = self.world.buildings.len();
        let mut set = HashSet::new();
        for b in &self.world.buildings {
            if b.kind == "Support" {
                if let Some(t) = self.world.idx(Pos::new(b.x1, b.y1, b.z)) {
                    set.insert(t as u32);
                }
            }
        }
        self.cx.cave.supports = set;
    }

    /// Orthogonal neighbours of `i` that `i` is connected to (down last, so it is tried first).
    fn cv_neighbours(&self, i: usize, out: &mut [usize; 6]) -> usize {
        let (x, y, z) = self.nav.xyz(i);
        let layer = self.nav.layer;
        let mut n = 0;
        let mut push = |q: usize, out: &mut [usize; 6]| {
            out[n] = q;
            n += 1;
        };
        if x > 0 {
            push(i - 1, out);
        }
        if x + 1 < self.nav.dx {
            push(i + 1, out);
        }
        if y > 0 {
            push(i - self.nav.dx, out);
        }
        if y + 1 < self.nav.dy {
            push(i + self.nav.dx, out);
        }
        // A ramp does not hold up the tile above it.
        if z + 1 < self.nav.dz && self.cv_shape(i) != Shape::Ramp {
            push(i + layer, out);
        }
        if z > 0 && self.cv_shape(i - layer) != Shape::Ramp {
            push(i - layer, out);
        }
        n
    }

    /// Flood fill from `start`; true if supported (anchor reached, known-supported tile reached
    /// or search cap hit). `out` receives the visited tiles.
    fn cv_search(&mut self, start: usize, supported: &HashSet<u32>, seen: &mut HashSet<u32>, out: &mut Vec<u32>) -> bool {
        out.clear();
        seen.clear();
        let mut stack = vec![start as u32];
        seen.insert(start as u32);
        let mut nb = [0usize; 6];
        let mut ok = false;
        while let Some(i) = stack.pop() {
            out.push(i);
            if supported.contains(&i) || self.cv_anchor(i as usize) || out.len() >= SEARCH_CAP {
                ok = true;
                break;
            }
            let n = self.cv_neighbours(i as usize, &mut nb);
            for &q in &nb[..n] {
                if self.cv_connects(q) && seen.insert(q as u32) {
                    stack.push(q as u32);
                }
            }
        }
        self.cx.cave.rep.nodes += out.len() as u64;
        ok
    }

    /// [r7 hook] Tiles `changed` were dug out or deconstructed: let unsupported terrain fall.
    pub(crate) fn cavein_check(&mut self, changed: &[usize]) {
        if self.cx.cave.disabled {
            return;
        }
        self.cv_refresh_supports();
        self.cx.cave.rep.checks += 1;
        let mut seeds: Vec<usize> = Vec::with_capacity(changed.len() * 7);
        let mut nb = [0usize; 6];
        for &c in changed {
            if c == usize::MAX || c >= self.world.tiles.len() {
                continue;
            }
            seeds.push(c);
            // All six orthogonal neighbours (also the one above a ramp: it may just have lost
            // the wall below it).
            let n = self.cv_neighbours(c, &mut nb);
            seeds.extend_from_slice(&nb[..n]);
            let layer = self.nav.layer;
            if c + layer < self.world.tiles.len() {
                seeds.push(c + layer);
            }
        }
        let mut supported: HashSet<u32> = HashSet::new();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for &q in &seeds {
            if supported.contains(&(q as u32)) || !self.cv_connects(q) {
                continue;
            }
            if self.cv_search(q, &supported, &mut seen, &mut out) {
                supported.extend(out.iter().copied());
            } else {
                let chunk = std::mem::take(&mut out);
                self.cavein_collapse(&chunk);
            }
        }
    }

    /// Let an unsupported chunk fall.
    fn cavein_collapse(&mut self, chunk: &[u32]) {
        let now = self.ticks;
        let layer = self.nav.layer;
        self.cx.cave.rep.collapses += 1;
        self.cx.cave.rep.tiles_fallen += chunk.len() as u32;
        // Columns: (x, y) -> (lowest z, stone walls with their material, top terrain material).
        let mut cols: Vec<(usize, usize, usize, u32, u16, u16, TtMaterial)> = Vec::new();
        for &t in chunk {
            let (x, y, z) = self.nav.xyz(t as usize);
            let info = self.world.tiletypes.get(self.world.tiles[t as usize].tt);
            let (shape, material, mat) = (info.shape, info.material, self.world.tiles[t as usize].mat);
            let wall = matches!(shape, Shape::Wall | Shape::Fortification) && stone_like(material);
            match cols.iter_mut().find(|c| c.0 == x && c.1 == y) {
                Some(c) => {
                    c.2 = c.2.min(z);
                    if wall {
                        c.3 += 1;
                        c.4 = mat;
                    }
                    if matches!(material, TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::Soil | TtMaterial::LavaStone | TtMaterial::Feature) {
                        c.5 = mat;
                        c.6 = material;
                    }
                }
                None => {
                    let terr = matches!(material, TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::Soil | TtMaterial::LavaStone | TtMaterial::Feature);
                    cols.push((x, y, z, wall as u32, if wall { mat } else { NO_MAT }, if terr { mat } else { NO_MAT }, if terr { material } else { TtMaterial::Stone }));
                }
            }
        }
        cols.sort_unstable_by_key(|c| (c.1, c.0));
        // Riders: units standing on the chunk.
        let riders: Vec<(usize, usize)> =
            (0..self.ai.len()).filter(|&s| self.ai[s].kind != Kind::Dead && chunk.contains(&self.ai[s].pos)).map(|s| (s, self.nav.xyz(self.ai[s].pos as usize).2)).collect();
        // Items on the chunk.
        let chunk_set: HashSet<u32> = chunk.iter().copied().collect();
        let mut chunk_items: Vec<(i32, u32)> = Vec::new();
        for (i, it) in self.world.items.iter().enumerate() {
            if self.imeta[i].dead || !it.flags.on_ground || it.container >= 0 || it.holder >= 0 || it.building >= 0 {
                continue;
            }
            if let Some(t) = self.world.idx(it.pos) {
                if chunk_set.contains(&(t as u32)) {
                    chunk_items.push((it.id, t as u32));
                }
            }
        }
        // The chunk becomes open space.
        for &t in chunk {
            self.set_tile(t as usize, Shape::Empty, TtMaterial::Air, 0, NO_MAT);
            if let Some(&j) = self.job_at.get(&t) {
                self.remove_job(j);
            }
        }
        for &t in chunk {
            self.after_tile_change(t as usize);
        }
        // Where each column lands, and who is underneath.
        let mut landings: Vec<(usize, usize, u32)> = Vec::new(); // (x, y, landing tile)
        let mut under: Vec<u32> = Vec::new();
        for c in &cols {
            let (x, y) = (c.0, c.1);
            let mut z = c.2;
            let mut land = None;
            while z > 0 {
                z -= 1;
                let q = self.nav.idx(x, y, z);
                match self.cv_shape(q) {
                    Shape::Empty | Shape::RampTop => {
                        under.push(q as u32);
                        if z == 0 {
                            land = Some((q, true));
                        }
                    }
                    Shape::Wall | Shape::Fortification | Shape::None => {
                        land = Some((q + layer, true));
                        break;
                    }
                    _ => {
                        land = Some((q, false));
                        under.push(q as u32);
                        break;
                    }
                }
            }
            let Some((lt, open)) = land else { continue };
            if open && self.cv_shape(lt) == Shape::Empty && c.5 != NO_MAT {
                let class = crate::dig_class(c.6);
                self.set_tile(lt, Shape::Floor, class, 1, c.5);
                self.after_tile_change(lt);
            }
            if !under.contains(&(lt as u32)) {
                under.push(lt as u32);
            }
            landings.push((x, y, lt as u32));
            // Boulders of the natural stone walls that fell.
            let n = c.3.min(BOULDERS_PER_COLUMN);
            if n > 0 && c.4 != NO_MAT {
                let name = self.world.material(c.4).map(|m| m.name.clone()).unwrap_or_default();
                for _ in 0..n {
                    self.create_item("BOULDER", c.4, lt, format!("{name} boulder"));
                }
                self.cx.cave.rep.boulders += n;
            }
        }
        let land_of = |x: usize, y: usize, l: &[(usize, usize, u32)]| l.iter().find(|e| e.0 == x && e.1 == y).map(|e| e.2);
        self.combat_sync_units();
        // Creatures directly underneath are killed.
        let crushed: Vec<usize> = (0..self.ai.len()).filter(|&s| self.ai[s].kind != Kind::Dead && under.contains(&self.ai[s].pos)).collect();
        for s in crushed {
            if riders.iter().any(|r| r.0 == s) {
                continue;
            }
            self.combat_kill(s, now, "has been crushed by a cave-in");
            self.cx.cave.rep.killed += 1;
        }
        // Riders fall with the chunk.
        for &(s, z0) in &riders {
            let (x, y, _) = self.nav.xyz(self.ai[s].pos as usize);
            let Some(lt) = land_of(x, y, &landings) else { continue };
            let lz = self.nav.xyz(lt as usize).2;
            self.cancel_task(s);
            self.ai[s].pos = lt;
            let uidx = self.ai[s].uidx;
            self.world.units[uidx].pos = self.world.pos_of(lt as usize);
            self.cx.cave.rep.fell += 1;
            let levels = z0.saturating_sub(lz);
            if levels >= 10 {
                self.combat_kill(s, now, "has died from a fall");
                self.cx.cave.rep.killed += 1;
            } else if levels >= 2 && s < self.combat.f.len() {
                let f = &mut self.combat.f[s];
                f.uncon_until = f.uncon_until.max(now + 100 * levels as u64);
                self.ai[s].wake = now + 100 * levels as u64 + 1;
            } else {
                self.ai[s].wake = now + 1;
            }
        }
        // Items: in the way destroyed (except durable ones), on the chunk they land too.
        let durable = |sim: &Sim, i: usize| {
            let it = &sim.world.items[i];
            matches!(it.kind.as_str(), "BOULDER" | "ROUGH" | "CORPSE" | "CORPSEPIECE" | "REMAINS") || it.flags.artifact
        };
        let mut destroy: Vec<i32> = Vec::new();
        for (i, it) in self.world.items.iter().enumerate() {
            if self.imeta[i].dead || !it.flags.on_ground || it.container >= 0 || it.holder >= 0 || it.building >= 0 {
                continue;
            }
            if let Some(t) = self.world.idx(it.pos) {
                if under.contains(&(t as u32)) && !durable(self, i) && !it.desc.ends_with(" boulder") {
                    destroy.push(it.id);
                }
            }
        }
        for (id, t) in chunk_items {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let i = i as usize;
            if !durable(self, i) {
                destroy.push(id);
                continue;
            }
            let (x, y, _) = self.nav.xyz(t as usize);
            let Some(lt) = land_of(x, y, &landings) else {
                destroy.push(id);
                continue;
            };
            self.haul_remove(id);
            self.pile_occ_add(t, -1);
            self.world.items[i].pos = self.world.pos_of(lt as usize);
            self.pile_occ_add(lt, 1);
            self.haul_add(id);
            self.items_dirty = true;
        }
        for id in destroy {
            self.destroy_item(id);
            self.cx.cave.rep.items_destroyed += 1;
        }
        // Dust knocks out creatures near the landing tiles.
        for s in 0..self.ai.len().min(self.combat.f.len()) {
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            let (x, y, z) = self.nav.xyz(self.ai[s].pos as usize);
            let hit = landings.iter().any(|&(lx, ly, lt)| {
                let lz = self.nav.xyz(lt as usize).2;
                (x as i32 - lx as i32).abs() <= DUST_RADIUS && (y as i32 - ly as i32).abs() <= DUST_RADIUS && (z == lz || z == lz + 1)
            });
            if hit && self.combat.f[s].uncon_until < now + DUST_TICKS {
                self.combat.f[s].uncon_until = now + DUST_TICKS;
                self.ai[s].wake = self.ai[s].wake.max(now + DUST_TICKS + 1);
                self.cx.cave.rep.knocked_out += 1;
            }
        }
        let p = landings.first().map(|l| self.world.pos_of(l.2 as usize)).or_else(|| chunk.first().map(|&t| self.world.pos_of(t as usize)));
        self.announce(now, "A section of the cavern has collapsed!".into(), p, AnnouncementKind::Warning);
    }
}

#[cfg(test)]
impl Sim {
    pub(crate) fn dig_tile_for_test(&mut self, i: usize, d: rf_core::Dig) {
        let now = self.ticks;
        self.dig_tile(0, i, d, now);
    }

    pub(crate) fn place_unit_for_test(&mut self, s: usize, tile: usize) {
        self.cancel_task(s);
        self.ai[s].pos = tile as u32;
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].pos = self.world.pos_of(tile);
    }
}

#[cfg(test)]
mod tests {
    use crate::Sim;
    use rf_core::{Dig, Pos, Shape, World};

    fn shape(w: &World, p: Pos) -> Shape {
        w.tiletypes.get(w.tiles[w.idx(p).unwrap()].tt).shape
    }

    fn set(w: &mut World, p: Pos, name: &str) {
        let tt = w.tiletypes.by_name(name).unwrap();
        let i = w.idx(p).unwrap();
        w.tiles[i].tt = tt;
    }

    /// A stone pillar on a floor: digging away the pillar's base makes the column above fall.
    #[test]
    fn unsupported_chunk_collapses_and_supported_rock_does_not() {
        let mut w = World::synthetic(48, 48, 20);
        let surf = 20i16 * 2 / 3;
        // Open a 9x9 pit from z = surf down to surf - 6 (air), floor at surf - 7.
        let (cx, cy) = (8i16, 8i16);
        for z in (surf - 6)..=surf {
            for y in cy - 4..=cy + 4 {
                for x in cx - 4..=cx + 4 {
                    set(&mut w, Pos::new(x, y, z), "OpenSpace");
                }
            }
        }
        for y in cy - 4..=cy + 4 {
            for x in cx - 4..=cx + 4 {
                set(&mut w, Pos::new(x, y, surf - 7), "StoneFloor1");
            }
        }
        // A free-standing stone block of 3 levels in the pit hanging from a 1-tile stalk.
        for z in (surf - 3)..=(surf - 1) {
            set(&mut w, Pos::new(cx, cy, z), "StoneWall");
        }
        set(&mut w, Pos::new(cx, cy, surf - 4), "StoneWall");
        set(&mut w, Pos::new(cx, cy, surf - 5), "StoneWall");
        set(&mut w, Pos::new(cx, cy, surf - 6), "StoneWall");
        let mut sim = Sim::new(w, 1);
        // Digging a wall tile in the solid rock elsewhere: nothing falls.
        let rock = sim.world.idx(Pos::new(40, 40, surf - 5)).unwrap();
        sim.dig_tile_for_test(rock, Dig::Dig);
        assert_eq!(sim.cavein_report().collapses, 0, "solid rock is supported");
        // Dig the bottom of the stalk: the column above has no support any more.
        let base = sim.world.idx(Pos::new(cx, cy, surf - 6)).unwrap();
        sim.dig_tile_for_test(base, Dig::Channel);
        let rep = sim.cavein_report();
        assert_eq!(rep.collapses, 1, "{rep:?}");
        assert!(rep.tiles_fallen >= 5, "{rep:?}");
        for z in (surf - 5)..=(surf - 1) {
            assert_eq!(shape(&sim.world, Pos::new(cx, cy, z)), Shape::Empty, "z {z}");
        }
        assert!(rep.boulders >= 5, "{rep:?}");
        let boulders = sim.world.items.iter().filter(|i| i.kind == "BOULDER" && i.pos.x == cx && i.pos.y == cy).count();
        assert!(boulders >= 5, "boulders landed below ({boulders})");
    }

    /// A ceiling over a dug room stays (connected to the surrounding rock), and a creature
    /// under a collapsing slab is killed while one nearby is knocked out by the dust.
    #[test]
    fn creature_underneath_dies_and_dust_knocks_out() {
        let mut w = World::synthetic(48, 48, 20);
        let surf = 20i16 * 2 / 3;
        let z0 = surf - 6;
        // A room 7x7 on z0 (floor) with air at z0 + 1, and a floating 3x3 slab at z0 + 2
        // connected to the rock only through a wall ledge along y = 8.
        for z in z0..=surf {
            for y in 4..=12 {
                for x in 4..=12 {
                    set(&mut w, Pos::new(x, y, z), "OpenSpace");
                }
            }
        }
        for y in 4..=12 {
            for x in 4..=12 {
                set(&mut w, Pos::new(x, y, z0), "StoneFloor1");
            }
        }
        for y in 7..=9 {
            for x in 7..=9 {
                set(&mut w, Pos::new(x, y, z0 + 2), "StoneWall");
            }
        }
        // Bridge of wall from the slab to the room's east rock wall (x = 13).
        for x in 10..=12 {
            set(&mut w, Pos::new(x, 8, z0 + 2), "StoneWall");
        }
        let mut sim = Sim::new(w, 1);
        // Ceiling of rock elsewhere: a dug corridor is fine.
        assert_eq!(sim.cavein_report().collapses, 0);
        // Put two dwarves: one under the slab, one 2 tiles from it.
        let under = sim.world.idx(Pos::new(8, 8, z0)).unwrap();
        let near = sim.world.idx(Pos::new(11, 10, z0)).unwrap();
        sim.place_unit_for_test(0, under);
        sim.place_unit_for_test(1, near);
        // Dig the connecting tile next to the rock: the slab loses its support.
        let link = sim.world.idx(Pos::new(12, 8, z0 + 2)).unwrap();
        sim.dig_tile_for_test(link, Dig::Channel);
        let rep = sim.cavein_report();
        assert_eq!(rep.collapses, 1, "{rep:?}");
        assert!(rep.killed >= 1, "{rep:?}");
        assert!(rep.knocked_out >= 1, "{rep:?}");
        assert!(sim.drain_announcements().iter().any(|a| a.text.contains("collapsed")));
    }
}
