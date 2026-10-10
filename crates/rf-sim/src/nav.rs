//! Navigation grid: per-tile movement flags, DF traffic costs and walk components.
//!
//! Movement rules (DF wiki "Path", "Ramp", "Traffic"):
//! - A unit stands on a tile that is not solid and either has a floor (floor, stairs, ramp,
//!   boulder, shrub, branch, ...) or is open space directly above a solid tile (top of a wall).
//! - 8 neighbours in the xy plane; diagonal moves between two walls are allowed, like in DF.
//! - Stairs: up from a STAIR_UP/UPDOWN tile to a STAIR_DOWN/UPDOWN tile directly above.
//! - Ramps: a ramp with an open tile above connects to the tops of the walls next to it, i.e.
//!   (x+dx, y+dy, z+1) when (x+dx, y+dy, z) is solid. The move is diagonal across z-levels.
//! - Water of depth >= 4 and any magma are not walkable for walkers.
//! - Traffic: entering a tile costs high 1, normal 2, low 5, restricted 25.
//!
//! Reachability uses connected components (union-find over component ids) so "can this unit
//! reach that job?" is O(1). Digging merges components incrementally. Removing walkable
//! tiles can only split components; that is not tracked eagerly: the grid is marked
//! `suspect`, and the next path failure inside one component triggers a full rebuild.

use rf_core::{tile_flags, Shape, Tile, World};

pub const WALK: u8 = 1;
pub const UP: u8 = 2;
pub const DOWN: u8 = 4;
pub const RAMP: u8 = 8;
pub const SOLID: u8 = 16;
pub const TRAFFIC_SHIFT: u8 = 6;

/// Path cost of entering a tile by its rf_core traffic value (0 normal, 1 low, 2 high, 3 restricted).
pub const TRAFFIC_COST: [u32; 4] = [2, 5, 1, 25];

pub const DIRS8: [(isize, isize); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

pub const MAX_NEIGHBORS: usize = 26;

pub struct Nav {
    pub dx: usize,
    pub dy: usize,
    pub dz: usize,
    pub layer: usize,
    pub flags: Vec<u8>,
    /// Component id per tile (0 = not walkable). Compare with [`Nav::comp_root`].
    pub comp: Vec<u32>,
    parent: Vec<u32>,
    size: Vec<u32>,
    /// Some walkable tiles/edges were removed since the last rebuild; components may be split.
    pub suspect: bool,
    /// Smallest traffic cost present on the map (admissible A* heuristic factor).
    pub min_cost: u32,
    shapes: Vec<Shape>,
    blocked: Vec<u64>,
    any_blocked: bool,
    pub rebuilds: u64,
    /// [perf] Bumped on every change of flags or components (cache key for derived data).
    pub version: u64,
    // [defense] Building overrides (bit sets, allocated on first use): lowered bridges and
    // hatches give a floor over open space; locked hatches block stairs/ramps through them.
    floor_force: Vec<u64>,
    novert: Vec<u64>,
    any_override: bool,
}

impl Nav {
    pub fn new(world: &World, blocked_tiles: &[usize]) -> Nav {
        let (dx, dy, dz) = (world.dims.x as usize, world.dims.y as usize, world.dims.z as usize);
        let n = dx * dy * dz;
        let max_tt = world.tiletypes.infos.iter().map(|i| i.id as usize).max().unwrap_or(0);
        let mut shapes = vec![Shape::None; max_tt + 1];
        for i in &world.tiletypes.infos {
            shapes[i.id as usize] = i.shape;
        }
        let mut blocked = vec![0u64; n / 64 + 1];
        for &b in blocked_tiles {
            if b < n {
                blocked[b / 64] |= 1 << (b % 64);
            }
        }
        let mut nav = Nav {
            dx,
            dy,
            dz,
            layer: dx * dy,
            flags: vec![0; n],
            comp: vec![0; n],
            parent: vec![0],
            size: vec![0],
            suspect: false,
            min_cost: 2,
            shapes,
            blocked,
            any_blocked: !blocked_tiles.is_empty(),
            rebuilds: 0,
            version: 0, // [perf]
            floor_force: Vec::new(), // [defense]
            novert: Vec::new(),      // [defense]
            any_override: false,     // [defense]
        };
        let tiles = &world.tiles;
        let mut min_traffic = 2;
        for i in 0..n {
            let f = nav.compute(tiles, i);
            if f & WALK != 0 {
                min_traffic = min_traffic.min(TRAFFIC_COST[(f >> TRAFFIC_SHIFT) as usize]);
            }
            nav.flags[i] = f;
        }
        nav.min_cost = min_traffic;
        nav.rebuild_components();
        nav
    }

    #[inline]
    pub fn shape(&self, tt: u16) -> Shape {
        self.shapes.get(tt as usize).copied().unwrap_or(Shape::None)
    }

    #[inline]
    pub fn xyz(&self, i: usize) -> (usize, usize, usize) {
        // u32 division is much cheaper than u64 on x86; maps are far below 2^32 tiles.
        let (i, dx, dy) = (i as u32, self.dx as u32, self.dy as u32);
        let row = i / dx;
        ((i - row * dx) as usize, (row % dy) as usize, (row / dy) as usize)
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dy + y) * self.dx + x
    }

    #[inline]
    pub fn walkable(&self, i: usize) -> bool {
        self.flags[i] & WALK != 0
    }

    #[inline]
    pub fn cost(&self, i: usize) -> u32 {
        TRAFFIC_COST[(self.flags[i] >> TRAFFIC_SHIFT) as usize]
    }

    fn is_blocked(&self, i: usize) -> bool {
        self.any_blocked && self.blocked[i / 64] & (1 << (i % 64)) != 0
    }

    pub fn set_blocked(&mut self, i: usize, on: bool) {
        if on {
            self.blocked[i / 64] |= 1 << (i % 64);
            self.any_blocked = true;
        } else {
            self.blocked[i / 64] &= !(1 << (i % 64));
        }
    }

    /// [defense] Tile is blocked by a building (closed floodgate, locked door, raised bridge, ...).
    pub fn is_blocked_tile(&self, i: usize) -> bool {
        self.is_blocked(i)
    }

    /// [defense] A building provides a floor on tile `i` (lowered bridge, hatch cover).
    pub fn set_floor_override(&mut self, i: usize, on: bool) {
        Self::set_bit(&mut self.floor_force, i, on, self.flags.len());
        self.any_override = true;
    }

    /// [defense] No vertical movement through tile `i` (locked hatch over stairs or a ramp).
    pub fn set_novert(&mut self, i: usize, on: bool) {
        Self::set_bit(&mut self.novert, i, on, self.flags.len());
        self.any_override = true;
    }

    fn set_bit(v: &mut Vec<u64>, i: usize, on: bool, n: usize) {
        if v.is_empty() {
            if !on {
                return;
            }
            *v = vec![0u64; n / 64 + 1];
        }
        if on {
            v[i / 64] |= 1 << (i % 64);
        } else {
            v[i / 64] &= !(1 << (i % 64));
        }
    }

    #[inline]
    fn bit(v: &[u64], i: usize) -> bool {
        !v.is_empty() && v[i / 64] & (1 << (i % 64)) != 0
    }

    /// Movement flags of tile `i` from the tile itself and the tiles directly below/above.
    pub fn compute(&self, tiles: &[Tile], i: usize) -> u8 {
        let t = &tiles[i];
        let traffic = ((t.flags & tile_flags::TRAFFIC_MASK) >> tile_flags::TRAFFIC_SHIFT) as u8;
        let mut f = traffic << TRAFFIC_SHIFT;
        let s = self.shape(t.tt);
        if s.is_solid() {
            return f | SOLID;
        }
        let z = i / self.layer;
        let below_solid = z > 0 && self.shape(tiles[i - self.layer].tt).is_solid();
        let forced = self.any_override && Self::bit(&self.floor_force, i); // [defense]
        let stand = forced || s.has_floor() || (s.is_open() && s != Shape::EndlessPit && below_solid);
        let depth = t.liquid & rf_core::liquid::DEPTH_MASK;
        let magma = t.liquid & rf_core::liquid::MAGMA != 0;
        let liquid_ok = depth < 4 && !(magma && depth > 0);
        if stand && liquid_ok && !self.is_blocked(i) {
            f |= WALK;
            let vert = !(self.any_override && Self::bit(&self.novert, i)); // [defense] locked hatch
            if s.allows_up() && vert {
                f |= UP;
            }
            if s.allows_down() && vert {
                f |= DOWN;
            }
            if s == Shape::Ramp && vert && z + 1 < self.dz && !self.shape(tiles[i + self.layer].tt).is_solid() {
                f |= RAMP;
            }
        }
        f
    }

    fn ramp_next_to(&self, x: usize, y: usize, z: usize) -> bool {
        for &(ddx, ddy) in &DIRS8 {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.dx as isize || ny >= self.dy as isize {
                continue;
            }
            if self.flags[self.idx(nx as usize, ny as usize, z)] & RAMP != 0 {
                return true;
            }
        }
        false
    }

    /// All tiles a walker can move to from `i` in one step. Returns the count written to `out`.
    #[inline]
    pub fn neighbors(&self, i: usize, out: &mut [u32; MAX_NEIGHBORS]) -> usize {
        let f = self.flags[i];
        let (x, y, z) = self.xyz(i);
        let has_above = z + 1 < self.dz;
        let below_solid = z > 0 && self.flags[i - self.layer] & SOLID != 0;
        let ramp = f & RAMP != 0 && has_above;
        let mut n = 0;
        for &(ddx, ddy) in &DIRS8 {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.dx as isize || ny >= self.dy as isize {
                continue;
            }
            let q = (i as isize + ddy * self.dx as isize + ddx) as usize;
            let fq = self.flags[q];
            if fq & WALK != 0 {
                out[n] = q as u32;
                n += 1;
            }
            // Up a ramp onto the top of the adjacent wall.
            if ramp && fq & SOLID != 0 && self.flags[q + self.layer] & WALK != 0 {
                out[n] = (q + self.layer) as u32;
                n += 1;
            }
            // Down onto an adjacent ramp, from the top of the wall it leans against.
            if below_solid && self.flags[q - self.layer] & RAMP != 0 {
                out[n] = (q - self.layer) as u32;
                n += 1;
            }
        }
        if f & UP != 0 && has_above && self.flags[i + self.layer] & DOWN != 0 {
            out[n] = (i + self.layer) as u32;
            n += 1;
        }
        if f & DOWN != 0 && z > 0 && self.flags[i - self.layer] & UP != 0 {
            out[n] = (i - self.layer) as u32;
            n += 1;
        }
        n
    }

    #[inline]
    pub fn root(&self, mut c: u32) -> u32 {
        while self.parent[c as usize] != c {
            c = self.parent[c as usize];
        }
        c
    }

    /// Component of tile `i` (0 = none).
    #[inline]
    pub fn comp_root(&self, i: usize) -> u32 {
        let c = self.comp[i];
        if c == 0 {
            0
        } else {
            self.root(c)
        }
    }

    /// Component of the tile, or of a walkable neighbour if the tile itself is not walkable
    /// (units standing in odd places, items on unwalkable tiles).
    pub fn comp_near(&self, i: usize) -> u32 {
        let c = self.comp_root(i);
        if c != 0 {
            return c;
        }
        let mut buf = [0u32; MAX_NEIGHBORS];
        let n = self.neighbors(i, &mut buf);
        for &q in &buf[..n] {
            let c = self.comp_root(q as usize);
            if c != 0 {
                return c;
            }
        }
        // Standing in mid air or in a wall: look straight down/up one level.
        let (_, _, z) = self.xyz(i);
        if z > 0 {
            let c = self.comp_root(i - self.layer);
            if c != 0 {
                return c;
            }
        }
        0
    }

    fn new_comp(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.size.push(1);
        id
    }

    fn union(&mut self, a: u32, b: u32) {
        let (mut ra, mut rb) = (self.root(a), self.root(b));
        if ra == rb {
            return;
        }
        if self.size[ra as usize] < self.size[rb as usize] {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb as usize] = ra;
        self.size[ra as usize] += self.size[rb as usize];
    }

    /// Full flood-fill relabelling of all walkable tiles.
    pub fn rebuild_components(&mut self) {
        self.version += 1; // [perf]
        self.comp.iter_mut().for_each(|c| *c = 0);
        self.parent.clear();
        self.parent.push(0);
        self.size.clear();
        self.size.push(0);
        let mut stack: Vec<u32> = Vec::new();
        let mut buf = [0u32; MAX_NEIGHBORS];
        for i in 0..self.flags.len() {
            if self.flags[i] & WALK == 0 || self.comp[i] != 0 {
                continue;
            }
            let id = self.parent.len() as u32;
            self.parent.push(id);
            self.comp[i] = id;
            stack.push(i as u32);
            let mut count = 0u32;
            while let Some(c) = stack.pop() {
                count += 1;
                let n = self.neighbors(c as usize, &mut buf);
                for &q in &buf[..n] {
                    if self.comp[q as usize] == 0 {
                        self.comp[q as usize] = id;
                        stack.push(q);
                    }
                }
            }
            self.size.push(count);
        }
        self.suspect = false;
        self.rebuilds += 1;
    }

    /// Recompute flags in the 3x3x3 box around a changed tile and merge components.
    /// Returns true if something walkable was lost (components may now be split).
    pub fn update_around(&mut self, tiles: &[Tile], x: usize, y: usize, z: usize) -> bool {
        self.version += 1; // [perf]
        let mut lost = false;
        let (x0, x1) = (x.saturating_sub(1), (x + 1).min(self.dx - 1));
        let (y0, y1) = (y.saturating_sub(1), (y + 1).min(self.dy - 1));
        let (z0, z1) = (z.saturating_sub(1), (z + 1).min(self.dz - 1));
        for zz in z0..=z1 {
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let q = self.idx(xx, yy, zz);
                    let old = self.flags[q];
                    let new = self.compute(tiles, q);
                    self.flags[q] = new;
                    let removed = old & !new;
                    if removed & (WALK | UP | DOWN | RAMP) != 0 || (new & SOLID != 0 && old & SOLID == 0) {
                        lost = true;
                    }
                    // A wall that disappears only removes edges if a ramp leans against it.
                    if removed & SOLID != 0 && self.ramp_next_to(xx, yy, zz) {
                        lost = true;
                    }
                    if new & WALK == 0 {
                        self.comp[q] = 0;
                    } else {
                        let c = TRAFFIC_COST[(new >> TRAFFIC_SHIFT) as usize];
                        self.min_cost = self.min_cost.min(c);
                    }
                }
            }
        }
        let mut buf = [0u32; MAX_NEIGHBORS];
        for zz in z0..=z1 {
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let q = self.idx(xx, yy, zz);
                    if self.flags[q] & WALK == 0 {
                        continue;
                    }
                    if self.comp[q] == 0 {
                        let id = self.new_comp();
                        self.comp[q] = id;
                    }
                    let n = self.neighbors(q, &mut buf);
                    for &o in &buf[..n] {
                        let co = self.comp[o as usize];
                        if co != 0 {
                            let cq = self.comp[q];
                            self.union(cq, co);
                        } else {
                            let cq = self.comp[q];
                            self.comp[o as usize] = cq;
                            let r = self.root(cq) as usize;
                            if let Some(s) = self.size.get_mut(r) {
                                *s += 1;
                            }
                        }
                    }
                }
            }
        }
        if lost {
            self.suspect = true;
        }
        lost
    }

    /// Flatten the union-find (keeps queries short). Cheap; call now and then.
    pub fn compress(&mut self) {
        for c in 1..self.parent.len() {
            let r = self.root(c as u32);
            self.parent[c] = r;
        }
    }

    pub fn component_count(&self) -> usize {
        (1..self.parent.len()).filter(|&c| self.parent[c] == c as u32).count()
    }
}
