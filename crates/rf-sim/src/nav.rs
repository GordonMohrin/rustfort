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
use std::cmp::Reverse; // [perf9]
use std::collections::BinaryHeap; // [perf9]

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

/// [perf] Division by a fixed divisor `d` (1..2^16) as `(n * m) >> s`, exact for n < 2^31
/// (Granlund-Montgomery: s = 31 + ceil(log2 d), m = ceil(2^s / d), so n * m < 2^63).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub struct Magic {
    m: u64,
    s: u32,
}

impl Magic {
    pub fn new(d: u32) -> Magic {
        let d = d.max(1);
        let l = 32 - (d - 1).leading_zeros(); // ceil(log2 d), 0 for d = 1
        let s = 31 + l;
        let m = ((1u64 << s) + d as u64 - 1) / d as u64;
        Magic { m, s }
    }
    #[inline]
    pub fn div(self, n: u32) -> u32 {
        ((n as u64 * self.m) >> self.s) as u32
    }
}

#[derive(serde::Serialize, serde::Deserialize)] // [saves11] exact snapshot (big tables as byte columns)
#[serde(default = "Nav::empty")] // [saves11]
pub struct Nav {
    pub dx: usize,
    pub dy: usize,
    pub dz: usize,
    pub layer: usize,
    #[serde(with = "crate::saves11::le_u8")] // [saves11]
    pub flags: Vec<u8>,
    /// Component id per tile (0 = not walkable). Compare with [`Nav::comp_root`].
    #[serde(with = "crate::saves11::le_u32")] // [saves11]
    pub comp: Vec<u32>,
    #[serde(with = "crate::saves11::le_u32")] // [saves11]
    parent: Vec<u32>,
    #[serde(with = "crate::saves11::le_u32")] // [saves11]
    size: Vec<u32>,
    /// Some walkable tiles/edges were removed since the last rebuild; components may be split.
    pub suspect: bool,
    /// Smallest traffic cost present on the map (admissible A* heuristic factor).
    pub min_cost: u32,
    shapes: Vec<Shape>,
    #[serde(with = "crate::saves11::le_u64")] // [saves11]
    blocked: Vec<u64>,
    any_blocked: bool,
    pub rebuilds: u64,
    /// [perf] Exact division by dx and dy (for `xyz`).
    mdx: Magic,
    mdy: Magic,
    /// [perf] Bumped on every change of flags or components (cache key for derived data).
    pub version: u64,
    // [defense] Building overrides (bit sets, allocated on first use): lowered bridges and
    // hatches give a floor over open space; locked hatches block stairs/ramps through them.
    #[serde(with = "crate::saves11::le_u64")] // [saves11]
    floor_force: Vec<u64>,
    #[serde(with = "crate::saves11::le_u64")] // [saves11]
    novert: Vec<u64>,
    any_override: bool,
    /// [perf9] Landmark distances for the A* heuristic (see [`Landmarks`]).
    pub lm: Landmarks,
}

/// [perf9] Landmarks per walk component: number of slots per tile.
pub const LM_K: usize = 16;
/// [perf9] No landmark value for this tile and slot.
pub const LM_INF: u16 = u16::MAX;
/// [perf9] Components with fewer walkable tiles get no landmarks (plain heuristic).
const LM_MIN_COMP: usize = 1500;
/// [perf9] Relaxations allowed in one incremental update before the tables are given up.
const LM_RELAX_CAP: u64 = 4_000_000;

/// [perf9] ALT heuristic data ("A*, Landmarks and Triangle inequality", Goldberg & Harrelson).
///
/// Every component with at least `LM_MIN_COMP` tiles gets `LM_K` landmark tiles picked by
/// farthest-point selection; slot `j` of a tile holds its path cost from landmark `j` of its
/// component. A* then uses, besides the distance bound, `D(g) - D(n)` and
/// `D(n) - D(g) + cost(g) - cost(n)` (moves are symmetric and a step costs the entered tile, so
/// the reverse distance is the forward one shifted by the two end costs).
///
/// Both bounds stay admissible and consistent as long as the table is a *feasible potential*:
/// `D(x) <= D(y) + cost(x)` for every move y -> x. Removing moves (walls, locked doors) keeps
/// that; new moves (digging, opened doors, cheaper traffic) are repaired right away by a
/// decrease-only relaxation from the changed tiles (`relax_box`). So paths stay optimal. They can
/// differ from the old heuristic's paths among equally cheap ones (other tie-breaks).
#[derive(Default)]
pub struct Landmarks {
    pub on: bool,
    /// Tile -> compact index (u32::MAX = none).
    id: Vec<u32>,
    /// Compact index -> tile.
    tile: Vec<u32>,
    d: Vec<[u16; LM_K]>,
    /// Landmark tiles (all components).
    pub marks: Vec<u32>,
    inq: Vec<bool>,
    queue: std::collections::VecDeque<u32>,
    /// Relaxations done by incremental updates (stats).
    pub relaxed: u64,
    /// [saves11] Map width (tile index -> row above, for the snapshot's predictor).
    dx: u32,
}

/// [saves11] Snapshot form of [`Landmarks`]: `id` and `inq` are rebuilt from `tile` and
/// `queue`, the distances are predicted from the tiles left and above (see
/// `saves11::lm_encode`).
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct LmSnap {
    on: bool,
    dx: u32,
    n: u64,
    #[serde(with = "crate::saves11::le_u32")]
    tile: Vec<u32>,
    #[serde(with = "crate::saves11::le_u8")]
    d: Vec<u8>,
    marks: Vec<u32>,
    queue: std::collections::VecDeque<u32>,
    relaxed: u64,
}

// [saves11] Landmarks are saved through `LmSnap`.
impl serde::Serialize for Landmarks {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let d = crate::saves11::lm_encode(self.dx as usize, &self.id, &self.tile, &self.d);
        let snap = LmSnap { on: self.on, dx: self.dx, n: self.id.len() as u64, tile: self.tile.clone(), d, marks: self.marks.clone(), queue: self.queue.clone(), relaxed: self.relaxed };
        snap.serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for Landmarks {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = LmSnap::deserialize(d)?;
        if s.n > 1 << 31 || s.tile.iter().any(|&t| t as u64 >= s.n) || s.queue.iter().any(|&k| k as usize >= s.tile.len()) {
            return Err(serde::de::Error::custom("landmark table out of range"));
        }
        let mut lm = Landmarks { on: s.on, dx: s.dx, id: vec![u32::MAX; s.n as usize], tile: s.tile, marks: s.marks, queue: s.queue, relaxed: s.relaxed, ..Default::default() };
        for (k, &t) in lm.tile.iter().enumerate() {
            lm.id[t as usize] = k as u32;
        }
        lm.inq = vec![false; lm.tile.len()];
        for &k in &lm.queue {
            lm.inq[k as usize] = true;
        }
        lm.d = crate::saves11::lm_decode(lm.dx as usize, &lm.id, &lm.tile, &s.d).ok_or_else(|| serde::de::Error::custom("landmark distances"))?;
        Ok(lm)
    }
}

impl Nav {
    /// [saves11] Grid of an empty world (defaults of a snapshot's missing fields).
    pub(crate) fn empty() -> Nav {
        Nav::new(&World::default(), &[])
    }

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
            mdx: Magic::new(dx as u32), // [perf]
            mdy: Magic::new(dy as u32), // [perf]
            version: 0, // [perf]
            floor_force: Vec::new(), // [defense]
            novert: Vec::new(),      // [defense]
            any_override: false,     // [defense]
            lm: Landmarks::default(), // [perf9]
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
        nav.build_landmarks(); // [perf9]
        nav
    }

    // ------------------------------------------------------------------ [perf9] landmarks

    /// [perf9] Landmark distances of walkable tile `i` (None: no table, or not walkable).
    #[inline]
    pub fn lm_dist(&self, i: usize) -> Option<&[u16; LM_K]> {
        if !self.lm.on || self.flags[i] & WALK == 0 {
            return None;
        }
        let k = *self.lm.id.get(i)?;
        self.lm.d.get(k as usize)
    }

    /// Path costs from compact tile `src` to every tile of its component (u32::MAX = not reached).
    fn lm_dijkstra(&self, src: u32, dist: &mut [u32], heap: &mut BinaryHeap<Reverse<(u32, u32)>>) {
        dist.iter_mut().for_each(|d| *d = u32::MAX);
        heap.clear();
        dist[src as usize] = 0;
        heap.push(Reverse((0, src)));
        let mut buf = [0u32; MAX_NEIGHBORS];
        while let Some(Reverse((d, c))) = heap.pop() {
            if d > dist[c as usize] {
                continue;
            }
            let t = self.lm.tile[c as usize] as usize;
            let n = self.neighbors(t, &mut buf);
            for &q in &buf[..n] {
                let qc = self.lm.id[q as usize];
                if qc == u32::MAX {
                    continue;
                }
                let nd = d + self.cost(q as usize);
                if nd < dist[qc as usize] {
                    dist[qc as usize] = nd;
                    heap.push(Reverse((nd, qc)));
                }
            }
        }
    }

    /// Farthest-point landmarks of one component (compact indices in increasing order) and
    /// their distance columns (one per landmark, over all compact indices).
    fn lm_component(&self, members: &[u32]) -> (Vec<u32>, Vec<Vec<u16>>) {
        let n = self.lm.tile.len();
        let mut dist = vec![u32::MAX; n];
        let mut heap = BinaryHeap::new();
        // Member with the largest finite value (ties: the lowest index).
        let far = |vals: &[u32]| -> u32 {
            let mut best = (0u32, members[0]);
            for &m in members {
                let v = vals[m as usize];
                if v != u32::MAX && v > best.0 {
                    best = (v, m);
                }
            }
            best.1
        };
        self.lm_dijkstra(members[0], &mut dist, &mut heap);
        let mut pick = far(&dist);
        let mut mind = vec![u32::MAX; n];
        let mut marks = Vec::new();
        let mut cols = Vec::new();
        for _ in 0..LM_K {
            marks.push(pick);
            self.lm_dijkstra(pick, &mut dist, &mut heap);
            let mut col = vec![LM_INF; n];
            for &m in members {
                let d = dist[m as usize];
                if d < LM_INF as u32 {
                    col[m as usize] = d as u16;
                }
                mind[m as usize] = mind[m as usize].min(d);
            }
            cols.push(col);
            pick = far(&mind);
        }
        (marks, cols)
    }

    /// [perf9] (Re)build the landmark tables from scratch: farthest-point landmarks in every
    /// component of at least `LM_MIN_COMP` tiles, one Dijkstra each (components in parallel;
    /// the result does not depend on the thread count).
    pub fn build_landmarks(&mut self) {
        use rayon::prelude::*;
        let n = self.flags.len();
        let mut lm = Landmarks { id: vec![u32::MAX; n], dx: self.dx as u32, ..Default::default() }; // [saves11] dx
        let mut by_comp: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
        for i in 0..n {
            if self.flags[i] & WALK != 0 {
                let c = lm.tile.len() as u32;
                lm.id[i] = c;
                lm.tile.push(i as u32);
                by_comp.entry(self.comp_root(i)).or_default().push(c);
            }
        }
        lm.d = vec![[LM_INF; LM_K]; lm.tile.len()];
        lm.inq = vec![false; lm.tile.len()];
        let comps: Vec<Vec<u32>> = by_comp.into_values().filter(|v| v.len() >= LM_MIN_COMP).collect();
        let old = std::mem::replace(&mut self.lm, lm);
        let relaxed = old.relaxed;
        let res: Vec<(Vec<u32>, Vec<Vec<u16>>)> = comps.par_iter().map(|m| self.lm_component(m)).collect();
        for (k, (marks, cols)) in res.into_iter().enumerate() {
            for &m in &marks {
                self.lm.marks.push(self.lm.tile[m as usize]);
            }
            for &c in &comps[k] {
                for (j, col) in cols.iter().enumerate() {
                    self.lm.d[c as usize][j] = col[c as usize];
                }
            }
        }
        self.lm.relaxed = relaxed;
        // RF_NO_LANDMARKS: debug switch for A/B runs (old heuristic, old paths).
        self.lm.on = !comps.is_empty() && std::env::var_os("RF_NO_LANDMARKS").is_none();
    }

    /// Compact index of walkable tile `i`, adding one (no values yet) if it has none.
    fn lm_ensure(&mut self, i: usize) -> u32 {
        let k = self.lm.id[i];
        if k != u32::MAX {
            return k;
        }
        let k = self.lm.tile.len() as u32;
        self.lm.id[i] = k;
        self.lm.tile.push(i as u32);
        self.lm.d.push([LM_INF; LM_K]);
        self.lm.inq.push(false);
        k
    }

    /// `D(to) = min(D(to), D(from) + c)` per slot; true if something got smaller.
    #[inline]
    fn lm_relax(&mut self, from: u32, to: u32, c: u32) -> bool {
        let a = self.lm.d[from as usize];
        let b = &mut self.lm.d[to as usize];
        let mut changed = false;
        for j in 0..LM_K {
            if a[j] == LM_INF {
                continue;
            }
            let v = a[j] as u32 + c;
            if v < b[j] as u32 {
                b[j] = v as u16; // v < LM_INF here
                changed = true;
            }
        }
        changed
    }

    fn lm_push(&mut self, k: u32) {
        if !self.lm.inq[k as usize] {
            self.lm.inq[k as usize] = true;
            self.lm.queue.push_back(k);
        }
    }

    /// Relax every move into and out of walkable tile `i` (after its moves or cost changed).
    fn lm_touch(&mut self, i: usize) {
        if self.flags[i] & WALK == 0 {
            return;
        }
        let k = self.lm_ensure(i);
        let ci = self.cost(i);
        let mut buf = [0u32; MAX_NEIGHBORS];
        let n = self.neighbors(i, &mut buf);
        for &q in &buf[..n] {
            let qk = self.lm_ensure(q as usize);
            if self.lm_relax(qk, k, ci) {
                self.lm_push(k);
            }
            let cq = self.cost(q as usize);
            if self.lm_relax(k, qk, cq) {
                self.lm_push(qk);
            }
        }
    }

    /// Decrease-only propagation until the table is a feasible potential again.
    fn lm_propagate(&mut self) {
        let mut buf = [0u32; MAX_NEIGHBORS];
        let mut work = 0u64;
        while let Some(k) = self.lm.queue.pop_front() {
            self.lm.inq[k as usize] = false;
            let t = self.lm.tile[k as usize] as usize;
            if self.flags[t] & WALK == 0 {
                continue;
            }
            let n = self.neighbors(t, &mut buf);
            work += n as u64;
            for &q in &buf[..n] {
                let qk = self.lm_ensure(q as usize);
                let cq = self.cost(q as usize);
                if self.lm_relax(k, qk, cq) {
                    self.lm_push(qk);
                }
            }
            if work > LM_RELAX_CAP {
                // Give up (deterministically): plain heuristic from now on.
                self.lm.on = false;
                while let Some(k) = self.lm.queue.pop_front() {
                    self.lm.inq[k as usize] = false;
                }
                break;
            }
        }
        self.lm.relaxed += work;
    }

    /// [perf9] The traffic cost of tile `i` was changed directly in `flags`.
    pub fn lm_cost_changed(&mut self, i: usize) {
        if self.lm.on {
            self.lm_touch(i);
            self.lm_propagate();
        }
    }

    /// [perf9] Repair the landmark table after the flags in a box changed (`gained`: tiles
    /// that became walkable; their old values are dropped so they are relearned exactly).
    fn lm_relax_box(&mut self, x0: usize, x1: usize, y0: usize, y1: usize, z0: usize, z1: usize, gained: &[u32]) {
        for &g in gained {
            let k = self.lm_ensure(g as usize);
            self.lm.d[k as usize] = [LM_INF; LM_K];
        }
        for zz in z0..=z1 {
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let q = self.idx(xx, yy, zz);
                    self.lm_touch(q);
                }
            }
        }
        self.lm_propagate();
    }

    #[inline]
    pub fn shape(&self, tt: u16) -> Shape {
        self.shapes.get(tt as usize).copied().unwrap_or(Shape::None)
    }

    #[inline]
    pub fn xyz(&self, i: usize) -> (usize, usize, usize) {
        // [perf] Divisions by dx and dy as multiply + shift (exact for all i < 2^31, see
        // `Magic`); a hardware division costs several times as much and A* calls this for
        // every neighbour it looks at.
        let (i, dx, dy) = (i as u32, self.dx as u32, self.dy as u32);
        let row = self.mdx.div(i);
        let z = self.mdy.div(row);
        ((i - row * dx) as usize, (row - z * dy) as usize, z as usize)
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
        let mut gained = [0u32; 27]; // [perf9] tiles that became walkable
        let (mut n_gained, mut any_change) = (0usize, false); // [perf9]
        for zz in z0..=z1 {
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let q = self.idx(xx, yy, zz);
                    let old = self.flags[q];
                    let new = self.compute(tiles, q);
                    self.flags[q] = new;
                    if old != new {
                        any_change = true; // [perf9]
                        if old & WALK == 0 && new & WALK != 0 {
                            gained[n_gained] = q as u32;
                            n_gained += 1;
                        }
                    }
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
        if any_change && self.lm.on {
            self.lm_relax_box(x0, x1, y0, y1, z0, z1, &gained[..n_gained]); // [perf9]
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

#[cfg(test)]
mod tests {
    use super::Magic;

    #[test]
    fn magic_division_is_exact() {
        for d in [1u32, 2, 3, 5, 7, 16, 48, 96, 100, 144, 191, 192, 193, 255, 256, 1000, 36864, 65535] {
            let m = Magic::new(d);
            let mut n = 0u32;
            while n < (1u32 << 31) - 1 {
                assert_eq!(m.div(n), n / d, "{n} / {d}");
                for k in 1..4 {
                    let a = n.saturating_add(d * k - 1).min((1 << 31) - 1);
                    assert_eq!(m.div(a), a / d, "{a} / {d}");
                }
                n = n.saturating_add(n / 7 + d).min((1 << 31) - 1);
            }
            for n in 0..200_000u32 {
                assert_eq!(m.div(n), n / d);
            }
            let top = (1u32 << 31) - 1;
            assert_eq!(m.div(top), top / d);
        }
    }
}
