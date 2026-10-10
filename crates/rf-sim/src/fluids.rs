//! [fluids] DF liquids: water and magma flow, pressure, evaporation, mud, aquifers, obsidian,
//! drowning and magma burns.
//!
//! # Rules (DF wiki v50: Water, Magma, Flow, Pressure, Evaporation, Mud, Aquifer, Obsidian,
//! Drowning, Swimming, Floodgate, Well)
//! - A tile holds 0..=7 sevenths of one liquid (`Tile::liquid`: depth bits + magma bit).
//! - **Falling:** liquid in a tile without a floor (open space, ramp top, down/up-down stairs)
//!   falls into the tile below if that tile is not a wall, as much as fits, one z per tick.
//! - **Spreading:** liquid spreads to the 8 neighbours on its level (DF liquids flow
//!   diagonally, also between two diagonal walls) while the neighbour is at least 2/7 lower,
//!   1/7 per neighbour per update. A difference of 1/7 does not move, so liquid comes to rest
//!   (DF's 1/7 "flicker" is not modelled).
//! - **Pressure (water only):** water that falls onto full (7/7) water searches through full
//!   water (down, the 4 orthogonal neighbours, up; never diagonally) for a tile that is not
//!   full and lies **below the level it fell from**, and moves 1/7 there. So a U-bend fills up
//!   to one z-level below the source, as in DF. Magma has no pressure.
//! - **Sources and sinks:** `RiverSource` tiles (and [`Sim::add_liquid_source`]) refill to 7/7
//!   every tick and push with pressure; river/brook tiles on the map edge and endless pits
//!   drain.
//! - **Water + magma:** where they meet, the tile turns into an obsidian wall
//!   (`LavaWall`, `INORGANIC:OBSIDIAN`); the water becomes steam (not simulated further).
//! - **Evaporation:** 1/7 water on an outside tile evaporates; 1/7 magma anywhere. Water that
//!   leaves a floor (evaporation or flow) leaves **mud** behind ([`Sim::has_mud`]).
//! - **Salt/stagnant** flags travel with the water into empty tiles and are cleared when a
//!   tile dries.
//! - **Aquifers:** an aquifer wall leaks water into liquid-holding neighbours on its own level
//!   (8 directions) that are not full. It does not leak up or down; the Trussroads export
//!   shows dry floors directly above and below aquifer walls, and a full well shaft beside
//!   them. Digging an aquifer tile clears its aquifer flag (no dug aquifer tile in the export
//!   has it).
//! - **Buildings:** closed doors, closed floodgates, windows and raised bridges block liquid
//!   like walls; closed hatches and lowered bridges block it like floors. A door, floodgate or
//!   hatch counts as open when its flags contain `OPEN` (floodgates/hatches also `ACTIVE`
//!   without `CLOSED`, i.e. opened by their lever). Retracting bridges vanish when raised.
//! - **Swimming and drowning:** water of 4/7 or more is deep (not walkable). A unit with the
//!   SWIMMING skill keeps its head above 4/7..6/7 water; a non-swimmer struggles there and
//!   drowns at half speed; in 7/7 water nobody breathes. Units in deep liquid swim towards
//!   shallower tiles and walk out. Creatures with `NOBREATHE`, and races found in deep water
//!   when the map loads (fish), never drown.
//! - **Magma:** a unit in magma burns to death unless its race is fire-immune (`FIREIMMUNE`,
//!   `FIREIMMUNE_SUPER`) or was found in magma on load; loose items that are not magma-safe
//!   melt or burn.
//!
//! # Speed
//! Only cells on the active worklist are updated: a tile whose liquid changed queues itself
//! and its liquid-holding neighbours for the next tick; a cell that cannot move anything is
//! dropped. Settled liquid (lakes, the magma sea) costs nothing. The worklist is sorted every
//! tick and all randomness comes from (seed, tile, tick), so the result is deterministic and
//! independent of the thread count. Pressure searches are capped at
//! [`PRESSURE_MAX_NODES`] and failures are cached for the tick.
//!
//! # Estimates (not on the wiki)
//! - Drowning after [`DROWN_TICKS`] ticks without air, burning to death after
//!   [`MAGMA_BURN_TICKS`] ticks in magma.
//! - Light aquifers add 1/7 to each open neighbour with chance 1/10 every 100 ticks, heavy
//!   ones every 100 ticks; evaporation chance 1/12 per 100 ticks (about a day).
//! - Raised (non-retracting) bridges block all their tiles, not only the raised edge.
//! - Magma-safe: stone, gems, glass and the metals iron, pig iron, steel, nickel, platinum and
//!   adamantine.
//! - No temperature: no freezing/thawing, no steam clouds, no fire spreading from magma.

use crate::rng::{splitmix, Rng};
use crate::{Kind, Sim};
use rf_core::{liquid, tile_flags, MaterialInfo, Pos, Shape, Tile, TtMaterial, TtSpecial, NO_MAT};
use std::collections::HashMap;

/// Ticks a unit survives without air (estimate).
pub const DROWN_TICKS: u32 = 100;
/// Ticks a non-fire-immune unit survives in magma (estimate).
pub const MAGMA_BURN_TICKS: u32 = 10;
/// Full-water tiles a single pressure search may visit.
pub const PRESSURE_MAX_NODES: usize = 4096;
/// Liquid tiles a single levelling search may visit.
pub const LEVEL_MAX_NODES: usize = 96;
/// Depth from which water must be swum (DF: 4/7 and deeper).
pub const DEEP: u8 = 4;
const AQ_PERIOD: u64 = 100;
const EVAP_PERIOD: u64 = 100;
const BLD_PERIOD: u64 = 16;
const UNIT_SCAN_PERIOD: u64 = 100;

// Tiletype classes.
const HOLD: u8 = 1;
const FLOORLESS: u8 = 2;
const FORT: u8 = 4;
const PIT: u8 = 8;
const RIVER: u8 = 16;
const SOURCE: u8 = 32;
const FLOOR_SHAPE: u8 = 64;

const DIRS8: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

#[derive(Clone, Default)]
struct Bits(Vec<u64>);

impl Bits {
    fn new(n: usize) -> Bits {
        Bits(vec![0; n / 64 + 1])
    }
    #[inline]
    fn get(&self, i: usize) -> bool {
        (self.0[i >> 6] >> (i & 63)) & 1 != 0
    }
    /// Set the bit; true if it was clear.
    #[inline]
    fn set(&mut self, i: usize) -> bool {
        let w = &mut self.0[i >> 6];
        let m = 1u64 << (i & 63);
        let was = *w & m != 0;
        *w |= m;
        !was
    }
    #[inline]
    fn clear(&mut self, i: usize) {
        self.0[i >> 6] &= !(1u64 << (i & 63));
    }
}

/// Counters for reports and the bench (all since the sim started).
#[derive(Clone, Debug, Default)]
pub struct FluidReport {
    /// Tiles holding any liquid, and how many of them hold magma.
    pub liquid_tiles: usize,
    pub magma_tiles: usize,
    /// Total volume in sevenths.
    pub water_volume: i64,
    pub magma_volume: i64,
    /// Cells on the active list right now ("flowing").
    pub flowing: usize,
    /// Cell updates done in total.
    pub updates: u64,
    pub pressure_moves: u64,
    /// 1/7 moves by the levelling search (slopes of 1/7 steps evening out).
    pub level_moves: u64,
    /// Pressure and levelling searches done, and the tiles they looked at.
    pub searches: u64,
    pub search_nodes: u64,
    pub obsidian: u64,
    pub evaporated: u64,
    pub aquifer_added: u64,
    pub drained: u64,
    pub source_added: u64,
    pub drowned: u64,
    pub burned: u64,
    pub items_melted: u64,
    pub leaky_aquifer_tiles: usize,
    pub mud_tiles: usize,
    /// Units currently in deep water or magma.
    pub submerged: usize,
    /// Wall-clock seconds spent in the fluid tick (not part of the simulated state).
    pub time_s: f64,
}

#[derive(Clone, Copy, Debug)]
struct Sub {
    slot: u32,
    air: u32,
    burn: u32,
    next_swim: u64,
    warned: bool,
}

/// Fluid state kept by the sim (`Sim::fluids`).
pub(crate) struct Fluids {
    pub(crate) on: bool,
    ready: bool,
    dx: usize,
    dy: usize,
    dz: usize,
    layer: usize,
    cls: Vec<u8>,
    next: Vec<u32>,
    queued: Bits,
    cur: Vec<u32>,
    moved: Bits,
    moved_list: Vec<u32>,
    /// Tiles whose tiletype changed (obsidian) this tick.
    reshaped: Vec<u32>,
    /// Tiles that got magma this tick (items there may melt).
    magma_new: Vec<u32>,
    deep_new: bool,
    bsolid: Bits,
    bfloor: Bits,
    bwatch: Vec<(usize, i32, u8)>,
    nbld: usize,
    seen: Bits,
    seen_list: Vec<u32>,
    bfs: Vec<u32>,
    fail: HashMap<u32, i16>,
    /// Levelling searches that failed this tick: tile -> depth they failed for.
    lfail: HashMap<u32, (u8, u8)>,
    fail_tick: u64,
    leaky: Vec<u32>,
    leaky_set: Bits,
    evap: Vec<u32>,
    evap_set: Bits,
    sources: Vec<(u32, bool)>,
    mud: Bits,
    /// Water tiles whose drinking access was registered.
    access: Bits,
    mud_count: usize,
    sub: Vec<Sub>,
    aquatic: Vec<String>,
    fireproof: Vec<String>,
    obsidian_tt: u16,
    obsidian_mat: u16,
    in_pass: bool,
    stats: FluidReport,
}

impl Default for Fluids {
    fn default() -> Self {
        Fluids {
            on: true,
            ready: false,
            dx: 0,
            dy: 0,
            dz: 0,
            layer: 0,
            cls: Vec::new(),
            next: Vec::new(),
            queued: Bits::default(),
            cur: Vec::new(),
            moved: Bits::default(),
            moved_list: Vec::new(),
            reshaped: Vec::new(),
            magma_new: Vec::new(),
            deep_new: false,
            bsolid: Bits::default(),
            bfloor: Bits::default(),
            bwatch: Vec::new(),
            nbld: usize::MAX,
            seen: Bits::default(),
            seen_list: Vec::new(),
            bfs: Vec::new(),
            fail: HashMap::new(),
            lfail: HashMap::new(),
            fail_tick: u64::MAX,
            leaky: Vec::new(),
            leaky_set: Bits::default(),
            evap: Vec::new(),
            evap_set: Bits::default(),
            sources: Vec::new(),
            mud: Bits::default(),
            access: Bits::default(),
            mud_count: 0,
            sub: Vec::new(),
            aquatic: Vec::new(),
            fireproof: Vec::new(),
            obsidian_tt: u16::MAX,
            obsidian_mat: NO_MAT,
            in_pass: false,
            stats: FluidReport::default(),
        }
    }
}

#[inline]
fn depth(t: &Tile) -> u8 {
    t.liquid & liquid::DEPTH_MASK
}

#[inline]
fn is_magma(t: &Tile) -> bool {
    t.liquid & liquid::MAGMA != 0
}

fn class_of(shape: Shape, mat: TtMaterial, special: TtSpecial) -> u8 {
    use Shape::*;
    let mut c = 0;
    if !matches!(shape, Wall | None) {
        c |= HOLD;
    }
    if matches!(shape, Empty | RampTop | StairDown | StairUpdown | EndlessPit | Twig) {
        c |= FLOORLESS;
    }
    if shape == Fortification {
        c |= FORT;
    }
    if shape == EndlessPit {
        c |= PIT;
    }
    if matches!(mat, TtMaterial::River | TtMaterial::Brook) {
        c |= RIVER;
    }
    if special == TtSpecial::RiverSource {
        c |= SOURCE;
    }
    if shape.has_floor() && !matches!(shape, StairDown | StairUpdown) {
        c |= FLOOR_SHAPE;
    }
    c
}

/// Magma-safe material (estimate, see module docs).
fn magma_safe(m: &MaterialInfo) -> bool {
    if m.is_metal {
        let t = m.token.trim_start_matches("INORGANIC:");
        return matches!(t, "IRON" | "PIG_IRON" | "STEEL" | "NICKEL" | "PLATINUM" | "ADAMANTINE");
    }
    m.is_stone || m.is_gem || m.is_glass
}

impl Fluids {
    #[inline]
    fn xyz(&self, i: usize) -> (usize, usize, usize) {
        let (i, dx, dy) = (i as u32, self.dx as u32, self.dy as u32);
        let row = i / dx;
        ((i - row * dx) as usize, (row % dy) as usize, (row / dy) as usize)
    }

    #[inline]
    fn class(&self, t: &Tile) -> u8 {
        self.cls.get(t.tt as usize).copied().unwrap_or(0)
    }

    #[inline]
    /// [gaps10] Can tile `i` hold liquid (for screw pump outputs)?
    pub(crate) fn holds(&self, tiles: &[Tile], i: usize) -> bool {
        self.hold(tiles, i)
    }

    fn hold(&self, tiles: &[Tile], i: usize) -> bool {
        self.class(&tiles[i]) & HOLD != 0 && !self.bsolid.get(i)
    }

    /// Liquid in `i` may fall into the tile below.
    #[inline]
    fn floorless(&self, tiles: &[Tile], i: usize) -> bool {
        self.class(&tiles[i]) & FLOORLESS != 0 && !self.bfloor.get(i)
    }

    /// Tile `b` can receive liquid from the tile above.
    #[inline]
    fn accepts_from_above(&self, tiles: &[Tile], b: usize) -> bool {
        self.hold(tiles, b) && self.class(&tiles[b]) & FORT == 0
    }

    fn on_edge(&self, i: usize) -> bool {
        let (x, y, _) = self.xyz(i);
        x == 0 || y == 0 || x + 1 == self.dx || y + 1 == self.dy
    }

    #[inline]
    fn enqueue(&mut self, i: usize) {
        if self.queued.set(i) {
            self.next.push(i as u32);
        }
    }

    /// Queue `i` and every liquid-holding neighbour that has liquid (same level, above, below).
    fn wake(&mut self, tiles: &[Tile], i: usize) {
        if depth(&tiles[i]) > 0 {
            self.enqueue(i);
        }
        let (x, y, z) = self.xyz(i);
        for &(ox, oy) in &DIRS8 {
            let (nx, ny) = (x as i32 + ox, y as i32 + oy);
            if nx < 0 || ny < 0 || nx >= self.dx as i32 || ny >= self.dy as i32 {
                continue;
            }
            let n = (i as i64 + oy as i64 * self.dx as i64 + ox as i64) as usize;
            if depth(&tiles[n]) > 0 {
                self.enqueue(n);
            }
        }
        if z + 1 < self.dz && depth(&tiles[i + self.layer]) > 0 {
            self.enqueue(i + self.layer);
        }
        if z > 0 && depth(&tiles[i - self.layer]) > 0 {
            self.enqueue(i - self.layer);
        }
    }

    /// Record a change of tile `i`: skip it for the rest of the tick and wake its area.
    fn touch(&mut self, tiles: &[Tile], i: usize) {
        if self.moved.set(i) {
            self.moved_list.push(i as u32);
        }
        self.wake(tiles, i);
    }

    /// Set the liquid of a tile, keeping counters, flags, mud and evaporation candidates.
    fn set(&mut self, tiles: &mut [Tile], i: usize, d: u8, magma: bool) {
        let t = &mut tiles[i];
        let (od, om) = (depth(t), is_magma(t));
        if od == d && (d == 0 || om == magma) {
            return;
        }
        if od > 0 {
            if om {
                self.stats.magma_volume -= od as i64;
                self.stats.magma_tiles -= 1;
            } else {
                self.stats.water_volume -= od as i64;
            }
            self.stats.liquid_tiles -= 1;
        }
        let keep = t.liquid & !(liquid::DEPTH_MASK | liquid::MAGMA);
        if d == 0 {
            t.liquid = keep;
            t.flags &= !(tile_flags::STAGNANT | tile_flags::SALT);
        } else {
            t.liquid = keep | d.min(7) | if magma { liquid::MAGMA } else { 0 };
            if magma {
                self.stats.magma_volume += d as i64;
                self.stats.magma_tiles += 1;
            } else {
                self.stats.water_volume += d as i64;
            }
            self.stats.liquid_tiles += 1;
        }
        let cls = self.class(&tiles[i]);
        if d == 0 && od > 0 && !om && cls & FLOOR_SHAPE != 0 && self.mud.set(i) {
            self.mud_count += 1;
        }
        if d == 1 && (magma || tiles[i].flags & tile_flags::OUTSIDE != 0) && self.evap_set.set(i) {
            self.evap.push(i as u32);
        }
        if magma && d > 0 && (od == 0 || !om) {
            self.magma_new.push(i as u32);
        }
        if (d >= DEEP || magma) && (od < DEEP || !om && magma) {
            self.deep_new = true;
        }
    }

    /// Move `m` sevenths from `a` to `b` (same liquid).
    fn transfer(&mut self, tiles: &mut [Tile], a: usize, b: usize, m: u8, magma: bool) {
        let (da, db) = (depth(&tiles[a]), depth(&tiles[b]));
        let carry = if db == 0 { tiles[a].flags & (tile_flags::STAGNANT | tile_flags::SALT) } else { tiles[a].flags & tile_flags::SALT };
        self.set(tiles, a, da - m, magma);
        self.set(tiles, b, db + m, magma);
        tiles[b].flags |= carry;
        self.touch(tiles, a);
        self.touch(tiles, b);
    }

    /// Water and magma meet in tile `i`: it becomes an obsidian wall.
    fn obsidian(&mut self, tiles: &mut [Tile], i: usize) {
        self.set(tiles, i, 0, false);
        if self.obsidian_tt != u16::MAX {
            tiles[i].tt = self.obsidian_tt;
            tiles[i].mat = self.obsidian_mat;
        }
        tiles[i].flags &= !(tile_flags::AQUIFER | 1 << 14);
        self.stats.obsidian += 1;
        self.reshaped.push(i as u32);
        self.touch(tiles, i);
    }

    /// Remove `m` sevenths from tile `i` (steam, drain).
    fn take(&mut self, tiles: &mut [Tile], i: usize, m: u8) {
        let t = tiles[i];
        let d = depth(&t).saturating_sub(m);
        self.set(tiles, i, d, is_magma(&t));
        self.touch(tiles, i);
    }

    /// One liquid update of tile `c`. True if anything moved.
    fn update(&mut self, tiles: &mut [Tile], c: usize, rng: &mut Rng, now: u64) -> bool {
        let t = tiles[c];
        let d = depth(&t);
        if d == 0 || !self.hold(tiles, c) {
            return false;
        }
        self.stats.updates += 1;
        let magma = is_magma(&t);
        let cls = self.class(&t);
        if cls & PIT != 0 || (cls & RIVER != 0 && self.on_edge(c)) {
            self.stats.drained += d as u64;
            self.take(tiles, c, d);
            return true;
        }
        let (x, y, z) = self.xyz(c);
        // Fall.
        if z > 0 && self.floorless(tiles, c) {
            let b = c - self.layer;
            if self.accepts_from_above(tiles, b) {
                let bt = tiles[b];
                let bd = depth(&bt);
                if bd > 0 && is_magma(&bt) != magma {
                    self.obsidian(tiles, b);
                    self.take(tiles, c, 1);
                    return true;
                }
                if bd < 7 {
                    self.transfer(tiles, c, b, d.min(7 - bd), magma);
                    return true;
                }
                if !magma && self.pressure(tiles, c, b, z, now) {
                    return true;
                }
            }
        }
        // Spread on the level.
        let mut d = d;
        let mut changed = false;
        let start = rng.below(8) as usize;
        for k in 0..8 {
            let (ox, oy) = DIRS8[(start + k) & 7];
            let (nx, ny) = (x as i32 + ox, y as i32 + oy);
            if nx < 0 || ny < 0 || nx >= self.dx as i32 || ny >= self.dy as i32 {
                continue;
            }
            let n = (c as i64 + oy as i64 * self.dx as i64 + ox as i64) as usize;
            if !self.hold(tiles, n) {
                continue;
            }
            let nt = tiles[n];
            let nd = depth(&nt);
            if nd > 0 && is_magma(&nt) != magma {
                self.obsidian(tiles, n);
                self.take(tiles, c, 1);
                d -= 1;
                changed = true;
            } else if nd + 1 < d {
                self.transfer(tiles, c, n, 1, magma);
                d -= 1;
                changed = true;
            }
            if d == 0 {
                break;
            }
        }
        // Levelling: on a slope of 1/7 steps (where the neighbour rule stops), connected liquid
        // on one level still evens out, as DF's flickering water does. A tile next to a 1/7
        // lower one searches the connected body on its level for a tile at least 2/7 lower
        // and pushes 1/7 there; a tile next to a 1/7 higher one pulls 1/7 from a tile at least
        // 2/7 higher. Every such move lowers the sum of squared depths, so the liquid comes to
        // rest. The searches are local ([`LEVEL_MAX_NODES`]).
        // Thin films (1/7, 2/7) are left to the neighbour rule.
        if !changed && d >= 2 {
            let (down, up) = self.step_neighbours(tiles, c, d, magma);
            if down && d >= 3 {
                if let Some(m) = self.level_search(tiles, c, d, magma, true, now) {
                    self.transfer(tiles, c, m, 1, magma);
                    self.stats.level_moves += 1;
                    return true;
                }
            }
            if up && d <= 5 {
                if let Some(m) = self.level_search(tiles, c, d, magma, false, now) {
                    self.transfer(tiles, m, c, 1, magma);
                    self.stats.level_moves += 1;
                    return true;
                }
            }
        }
        changed
    }

    /// Does `c` (depth `d`) have a neighbour of the same liquid exactly 1/7 lower / higher?
    fn step_neighbours(&self, tiles: &[Tile], c: usize, d: u8, magma: bool) -> (bool, bool) {
        let (x, y, _) = self.xyz(c);
        let (mut down, mut up) = (false, false);
        for &(ox, oy) in &DIRS8 {
            let (nx, ny) = (x as i32 + ox, y as i32 + oy);
            if nx < 0 || ny < 0 || nx >= self.dx as i32 || ny >= self.dy as i32 {
                continue;
            }
            let n = (c as i64 + oy as i64 * self.dx as i64 + ox as i64) as usize;
            let nt = tiles[n];
            let nd = depth(&nt);
            if nd > 0 && is_magma(&nt) != magma {
                continue;
            }
            if nd + 1 == d && self.hold(tiles, n) {
                down = true;
            } else if nd == d + 1 {
                up = true;
            }
        }
        (down, up)
    }

    /// Breadth-first search through the liquid connected to `c` on its level. `push`: find a
    /// tile holding at most `d - 2` (or nothing) that the body touches; else find a tile
    /// holding at least `d + 2`.
    fn level_search(&mut self, tiles: &[Tile], c: usize, d: u8, magma: bool, push: bool, now: u64) -> Option<usize> {
        if self.fail_tick != now {
            self.fail.clear();
            self.lfail.clear();
            self.fail_tick = now;
        }
        // Failure cache: push failed for depth <= fd (low byte), pull failed for depth >= fu.
        if let Some(&(fd, fu)) = self.lfail.get(&(c as u32)) {
            if (push && d <= fd) || (!push && d >= fu) {
                return None;
            }
        }
        if self.seen.0.is_empty() {
            self.seen = Bits::new(tiles.len());
        }
        let mut bfs = std::mem::take(&mut self.bfs);
        let mut seen = std::mem::take(&mut self.seen_list);
        bfs.clear();
        seen.clear();
        bfs.push(c as u32);
        self.seen.set(c);
        seen.push(c as u32);
        let mut head = 0;
        let mut found = None;
        let mut truncated = false;
        'search: while head < bfs.len() {
            let q = bfs[head] as usize;
            head += 1;
            let (x, y, _) = self.xyz(q);
            for &(ox, oy) in &DIRS8 {
                let (nx, ny) = (x as i32 + ox, y as i32 + oy);
                if nx < 0 || ny < 0 || nx >= self.dx as i32 || ny >= self.dy as i32 {
                    continue;
                }
                let n = (q as i64 + oy as i64 * self.dx as i64 + ox as i64) as usize;
                if self.seen.get(n) || !self.hold(tiles, n) {
                    continue;
                }
                self.seen.set(n);
                seen.push(n as u32);
                let nt = tiles[n];
                let nd = depth(&nt);
                if nd > 0 && is_magma(&nt) != magma {
                    continue;
                }
                let hit = if push { nd >= 1 && nd + 2 <= d } else { nd >= d + 2 };
                if hit {
                    found = Some(n);
                    break 'search;
                }
                if nd > 0 {
                    if bfs.len() >= LEVEL_MAX_NODES {
                        truncated = true;
                        break 'search;
                    }
                    bfs.push(n as u32);
                }
            }
        }
        for &q in &seen {
            self.seen.clear(q as usize);
        }
        self.stats.searches += 1;
        self.stats.search_nodes += seen.len() as u64;
        if found.is_none() && !truncated {
            for &q in &bfs {
                let e = self.lfail.entry(q).or_insert((0, u8::MAX));
                if push {
                    e.0 = e.0.max(d);
                } else {
                    e.1 = e.1.min(d);
                }
            }
        }
        self.bfs = bfs;
        self.seen_list = seen;
        found
    }

    /// Pressure: water in `src` (level `zsrc`) cannot fall into the full tile `start`. Search
    /// through full water for a non-full tile below `zsrc` and move 1/7 there.
    fn pressure(&mut self, tiles: &mut [Tile], src: usize, start: usize, zsrc: usize, now: u64) -> bool {
        if self.fail_tick != now {
            self.fail.clear();
            self.lfail.clear();
            self.fail_tick = now;
        }
        if let Some(&zl) = self.fail.get(&(start as u32)) {
            if zl as usize >= zsrc {
                return false;
            }
        }
        if self.seen.0.is_empty() {
            self.seen = Bits::new(tiles.len());
        }
        let mut bfs = std::mem::take(&mut self.bfs);
        let mut seen = std::mem::take(&mut self.seen_list);
        bfs.clear();
        seen.clear();
        bfs.push(start as u32);
        self.seen.set(start);
        seen.push(start as u32);
        let mut head = 0;
        let mut found = None;
        let mut truncated = false;
        let (dxi, layer) = (self.dx as i64, self.layer as i64);
        'search: while head < bfs.len() {
            let q = bfs[head] as usize;
            head += 1;
            let (x, y, z) = self.xyz(q);
            // DF order: down, the 4 orthogonal neighbours, up; never diagonally.
            for k in 0..6 {
                let n = match k {
                    0 if z > 0 && self.floorless(tiles, q) => q as i64 - layer,
                    1 if y > 0 => q as i64 - dxi,
                    2 if y + 1 < self.dy => q as i64 + dxi,
                    3 if x > 0 => q as i64 - 1,
                    4 if x + 1 < self.dx => q as i64 + 1,
                    5 if z + 1 < self.dz => q as i64 + layer,
                    _ => continue,
                } as usize;
                if self.seen.get(n) {
                    continue;
                }
                let ok = match k {
                    0 => self.accepts_from_above(tiles, n),
                    5 => self.floorless(tiles, n) && self.hold(tiles, n) && self.class(&tiles[n]) & FORT == 0,
                    _ => self.hold(tiles, n),
                };
                if !ok {
                    continue;
                }
                self.seen.set(n);
                seen.push(n as u32);
                let nt = tiles[n];
                let nd = depth(&nt);
                if nd > 0 && is_magma(&nt) {
                    continue;
                }
                if nd >= 7 {
                    if bfs.len() >= PRESSURE_MAX_NODES {
                        truncated = true;
                        break 'search;
                    }
                    bfs.push(n as u32);
                    continue;
                }
                if self.xyz(n).2 < zsrc {
                    found = Some(n);
                    break 'search;
                }
            }
        }
        for &q in &seen {
            self.seen.clear(q as usize);
        }
        self.stats.searches += 1;
        self.stats.search_nodes += seen.len() as u64;
        let res = match found {
            Some(o) => {
                self.transfer(tiles, src, o, 1, false);
                self.stats.pressure_moves += 1;
                true
            }
            None => {
                if !truncated {
                    for &q in &bfs {
                        self.fail.insert(q, zsrc as i16);
                    }
                }
                false
            }
        };
        self.bfs = bfs;
        self.seen_list = seen;
        res
    }

    /// A pressurized source tile: refill to 7/7, then flow normally or push with pressure.
    fn update_source(&mut self, tiles: &mut [Tile], c: usize, magma: bool, rng: &mut Rng, now: u64) {
        if !self.hold(tiles, c) {
            return;
        }
        let t = tiles[c];
        if depth(&t) > 0 && is_magma(&t) != magma {
            // The other liquid reached the source: obsidian plugs it.
            self.obsidian(tiles, c);
            return;
        }
        if depth(&t) < 7 {
            self.stats.source_added += (7 - depth(&t)) as u64;
            self.set(tiles, c, 7, magma);
            self.touch(tiles, c);
        }
        if self.update(tiles, c, rng, now) || magma {
            return;
        }
        // Full and nothing moved: push through the full water around it, up to one level
        // below the source.
        let z = self.xyz(c).2;
        let _ = self.pressure(tiles, c, c, z, now);
    }

    /// Building liquid state: 0 none, 1 blocks like a wall, 2 blocks like a floor.
    fn building_state(b: &rf_core::Building) -> u8 {
        if !b.built {
            return 0;
        }
        let has = |f: &str| b.flags.iter().any(|x| x == f);
        let opened = has("OPEN") || (has("ACTIVE") && !has("CLOSED"));
        match b.kind.as_str() {
            "Door" => (!has("OPEN")) as u8,
            "Floodgate" => (!opened) as u8,
            "WindowGlass" | "WindowGem" => 1,
            "Hatch" => {
                if opened {
                    0
                } else {
                    2
                }
            }
            "Bridge" => {
                if has("RAISED") {
                    if b.direction == "RETRACT" {
                        0
                    } else {
                        1
                    }
                } else {
                    2
                }
            }
            _ => 0,
        }
    }

    fn relevant_building(kind: &str) -> bool {
        matches!(kind, "Door" | "Floodgate" | "WindowGlass" | "WindowGem" | "Hatch" | "Bridge")
    }
}

impl Sim {
    // ------------------------------------------------------------------ init

    fn fluids_init(&mut self) {
        let w = &self.world;
        let n = w.tiles.len();
        let f = &mut self.fluids;
        f.ready = true;
        f.dx = w.dims.x as usize;
        f.dy = w.dims.y as usize;
        f.dz = w.dims.z as usize;
        f.layer = f.dx * f.dy;
        let max_tt = w.tiletypes.infos.iter().map(|i| i.id as usize).max().unwrap_or(0);
        f.cls = vec![0; max_tt + 1];
        for i in &w.tiletypes.infos {
            f.cls[i.id as usize] = class_of(i.shape, i.material, i.special);
        }
        f.queued = Bits::new(n);
        f.moved = Bits::new(n);
        f.bsolid = Bits::new(n);
        f.bfloor = Bits::new(n);
        f.leaky_set = Bits::new(n);
        f.evap_set = Bits::new(n);
        f.mud = Bits::new(n);
        f.access = Bits::new(n);
        if std::env::var_os("RUSTFORT_NO_FLUIDS").is_some() {
            f.on = false;
        }
        // Obsidian wall tiletype and material.
        f.obsidian_tt = w
            .tiletypes
            .by_name("LavaWall")
            .or_else(|| w.tiletypes.find(Shape::Wall, TtMaterial::LavaStone, TtSpecial::Normal, 0))
            .or_else(|| w.tiletypes.find(Shape::Wall, TtMaterial::Stone, TtSpecial::Normal, 0))
            .unwrap_or(u16::MAX);
        let obs = w.materials.iter().position(|m| m.token == "INORGANIC:OBSIDIAN");
        f.obsidian_mat = match obs {
            Some(m) => m as u16,
            None => {
                self.world.materials.push(MaterialInfo {
                    token: "INORGANIC:OBSIDIAN".into(),
                    name: "obsidian".into(),
                    rgb: [30, 30, 35],
                    is_stone: true,
                    mat_type: 0,
                    mat_index: -1,
                    density: 2650,
                    ..Default::default()
                });
                (self.world.materials.len() - 1) as u16
            }
        };
        let w = &self.world;
        let f = &mut self.fluids;
        // Liquids, sources, evaporation candidates, leaky aquifers.
        let mut st = FluidReport::default();
        for i in 0..n {
            let t = &w.tiles[i];
            let c = f.class(t);
            if c & SOURCE != 0 {
                f.sources.push((i as u32, false));
            }
            let d = depth(t);
            if d > 0 {
                st.liquid_tiles += 1;
                if is_magma(t) {
                    st.magma_tiles += 1;
                    st.magma_volume += d as i64;
                } else {
                    st.water_volume += d as i64;
                }
                if f.queued.set(i) {
                    f.next.push(i as u32);
                }
                if d == 1 && (is_magma(t) || t.flags & tile_flags::OUTSIDE != 0) && f.evap_set.set(i) {
                    f.evap.push(i as u32);
                }
            }
        }
        f.stats = st;
        for i in 0..n {
            if w.tiles[i].flags & tile_flags::AQUIFER != 0 && f.class(&w.tiles[i]) & HOLD == 0 && Self::fluids_leaks(f, &w.tiles, i) && f.leaky_set.set(i) {
                f.leaky.push(i as u32);
            }
        }
        // Races found in deep water / magma on load live there.
        for u in &w.units {
            // Citizens caught in a flood on load do not make their race aquatic.
            if u.flags.dead || u.is_worker() || w.citizens().any(|c| c.race == u.race) {
                continue;
            }
            if let Some(i) = w.idx(u.pos) {
                let t = &w.tiles[i];
                if is_magma(t) && depth(t) > 0 {
                    if !f.fireproof.contains(&u.race) {
                        f.fireproof.push(u.race.clone());
                    }
                } else if depth(t) >= DEEP && !f.aquatic.contains(&u.race) {
                    f.aquatic.push(u.race.clone());
                }
            }
        }
        for cr in &w.creature_raws {
            let flags = cr.flags.iter().chain(cr.castes.iter().flat_map(|c| c.flags.iter()));
            for fl in flags {
                if (fl == "FIREIMMUNE" || fl == "FIREIMMUNE_SUPER") && !f.fireproof.contains(&cr.token) {
                    f.fireproof.push(cr.token.clone());
                }
            }
        }
        self.fluids_buildings_check(true);
        self.fluids.deep_new = true;
    }

    /// Aquifer tile `i` has a liquid-holding neighbour on its level.
    fn fluids_leaks(f: &Fluids, tiles: &[Tile], i: usize) -> bool {
        let (x, y, _) = f.xyz(i);
        for &(ox, oy) in &DIRS8 {
            let (nx, ny) = (x as i32 + ox, y as i32 + oy);
            if nx < 0 || ny < 0 || nx >= f.dx as i32 || ny >= f.dy as i32 {
                continue;
            }
            let n = (i as i64 + oy as i64 * f.dx as i64 + ox as i64) as usize;
            if f.hold(tiles, n) {
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------ tick

    /// [fluids] One tick of liquid simulation (called from the tick loop).
    pub(crate) fn fluids_tick(&mut self, now: u64) {
        if !self.fluids.on {
            return;
        }
        if !self.fluids.ready {
            self.fluids_init();
            if !self.fluids.on {
                return;
            }
        }
        let periodic = now % BLD_PERIOD == 3 || now % AQ_PERIOD == 37 || now % EVAP_PERIOD == 71 || now % UNIT_SCAN_PERIOD == 13;
        let busy = !self.fluids.next.is_empty() || !self.fluids.sources.is_empty() || !self.fluids.sub.is_empty() || self.fluids.deep_new;
        if !periodic && !busy {
            // Settled: nothing to do this tick.
            return;
        }
        let t0 = std::time::Instant::now();
        if now % BLD_PERIOD == 3 {
            self.fluids_buildings_check(false);
        }
        if now % AQ_PERIOD == 37 && !self.fluids.leaky.is_empty() {
            self.fluids_aquifer(now);
        }
        if now % EVAP_PERIOD == 71 && !self.fluids.evap.is_empty() {
            self.fluids_evaporate(now);
        }
        let has_work = !self.fluids.next.is_empty() || !self.fluids.sources.is_empty();
        if has_work {
            self.fluids_pass(now);
        }
        if self.fluids.deep_new || now % UNIT_SCAN_PERIOD == 13 {
            self.fluids.deep_new = false;
            self.fluids_scan_units(now);
        }
        if !self.fluids.sub.is_empty() {
            self.fluids_units(now);
        }
        self.fluids.stats.time_s += t0.elapsed().as_secs_f64();
    }

    fn fluids_pass(&mut self, now: u64) {
        let seed = self.seed;
        let f = &mut self.fluids;
        let tiles = &mut self.world.tiles;
        std::mem::swap(&mut f.cur, &mut f.next);
        f.next.clear();
        f.cur.sort_unstable();
        let cur = std::mem::take(&mut f.cur);
        for &c in &cur {
            f.queued.clear(c as usize);
        }
        for k in 0..f.sources.len() {
            let (c, magma) = (f.sources[k].0 as usize, f.sources[k].1);
            let mut r = Rng::new(seed ^ splitmix(now ^ (c as u64) << 24 ^ 0x5157));
            f.update_source(tiles, c, magma, &mut r, now);
        }
        for &c in &cur {
            let c = c as usize;
            if f.moved.get(c) {
                f.enqueue(c);
                continue;
            }
            let mut r = Rng::new(seed ^ splitmix(now ^ (c as u64) << 24 ^ 0xF1));
            f.update(tiles, c, &mut r, now);
        }
        f.cur = cur;
        f.cur.clear();
        let moved = std::mem::take(&mut f.moved_list);
        if !self.power.under.is_empty() {
            // [gaps10] water wheels: water moving under a wheel is "flowing"
            for k in 0..self.power.under.len() {
                let t = self.power.under[k];
                if self.fluids.moved.get(t as usize) {
                    self.power_flow_seen(t, now);
                }
            }
        }
        let f = &mut self.fluids;
        for &c in &moved {
            f.moved.clear(c as usize);
        }
        // Navigation: liquid depth decides walkability of the tile itself; obsidian changes shape.
        let reshaped = std::mem::take(&mut self.fluids.reshaped);
        self.fluids.in_pass = true;
        for &c in &reshaped {
            self.after_tile_change(c as usize);
        }
        for &c in &moved {
            let c = c as usize;
            if self.nav.compute(&self.world.tiles, c) != self.nav.flags[c] {
                self.after_tile_change(c);
            } else if self.world.tiles[c].liquid & liquid::DEPTH_MASK > 0 && self.world.tiles[c].liquid & liquid::MAGMA == 0 && self.fluids.access.set(c) {
                self.add_water_access_around(c);
            }
        }
        self.fluids.in_pass = false;
        self.fluids.moved_list = moved;
        self.fluids.moved_list.clear();
        self.fluids.reshaped = reshaped;
        self.fluids.reshaped.clear();
        if !self.fluids.magma_new.is_empty() {
            self.fluids_melt_items(now);
        }
    }

    fn fluids_aquifer(&mut self, now: u64) {
        let seed = self.seed;
        let f = &mut self.fluids;
        let tiles = &mut self.world.tiles;
        let mut k = 0;
        while k < f.leaky.len() {
            let a = f.leaky[k] as usize;
            let t = tiles[a];
            if t.flags & tile_flags::AQUIFER == 0 || f.class(&t) & HOLD != 0 || !Self::fluids_leaks(f, tiles, a) {
                f.leaky_set.clear(a);
                f.leaky.swap_remove(k);
                continue;
            }
            k += 1;
            let heavy = t.flags & (1 << 14) != 0;
            let (x, y, _) = f.xyz(a);
            let mut r = Rng::new(seed ^ splitmix(now ^ (a as u64) << 24 ^ 0xA0));
            for &(ox, oy) in &DIRS8 {
                let (nx, ny) = (x as i32 + ox, y as i32 + oy);
                if nx < 0 || ny < 0 || nx >= f.dx as i32 || ny >= f.dy as i32 {
                    continue;
                }
                let n = (a as i64 + oy as i64 * f.dx as i64 + ox as i64) as usize;
                if !f.hold(tiles, n) {
                    continue;
                }
                let nt = tiles[n];
                let nd = depth(&nt);
                if nd >= 7 || (nd > 0 && is_magma(&nt)) {
                    continue;
                }
                if heavy || r.chance(1, 10) {
                    f.set(tiles, n, nd + 1, false);
                    f.touch(tiles, n);
                    f.stats.aquifer_added += 1;
                }
            }
        }
        // Order of `leaky` changed by swap_remove; keep it sorted for a stable iteration order.
        f.leaky.sort_unstable();
    }

    fn fluids_evaporate(&mut self, now: u64) {
        let seed = self.seed;
        let f = &mut self.fluids;
        let tiles = &mut self.world.tiles;
        let list = std::mem::take(&mut f.evap);
        let mut keep = Vec::with_capacity(list.len());
        for &c in &list {
            let i = c as usize;
            let t = tiles[i];
            let ok = depth(&t) == 1 && (is_magma(&t) || t.flags & tile_flags::OUTSIDE != 0);
            if !ok {
                f.evap_set.clear(i);
                continue;
            }
            let mut r = Rng::new(seed ^ splitmix(now ^ (i as u64) << 24 ^ 0xE7));
            if r.chance(1, 12) {
                f.evap_set.clear(i);
                f.set(tiles, i, 0, false);
                f.touch(tiles, i);
                f.stats.evaporated += 1;
            } else {
                keep.push(c);
            }
        }
        // `set` may have pushed new candidates during this pass.
        keep.append(&mut f.evap);
        f.evap = keep;
        let moved = std::mem::take(&mut f.moved_list);
        for &c in &moved {
            f.moved.clear(c as usize);
        }
        self.fluids.in_pass = true;
        for &c in &moved {
            let c = c as usize;
            if self.nav.compute(&self.world.tiles, c) != self.nav.flags[c] {
                self.after_tile_change(c);
            }
        }
        self.fluids.in_pass = false;
        self.fluids.moved_list = moved;
        self.fluids.moved_list.clear();
    }

    /// Re-read door/floodgate/hatch/bridge states; wake liquid next to the ones that changed.
    fn fluids_buildings_check(&mut self, force: bool) {
        let w = &self.world;
        let f = &mut self.fluids;
        let bl = &w.buildings;
        let stale = force || f.nbld != bl.len() || f.bwatch.iter().any(|&(bi, id, _)| bl.get(bi).map(|b| b.id) != Some(id));
        if stale {
            // Rebuild the masks from scratch and wake liquid where they differ.
            let n = w.tiles.len();
            f.nbld = bl.len();
            f.bwatch.clear();
            let old_s = std::mem::replace(&mut f.bsolid, Bits::new(n));
            let old_f = std::mem::replace(&mut f.bfloor, Bits::new(n));
            for (bi, b) in bl.iter().enumerate() {
                if Fluids::relevant_building(&b.kind) {
                    let st = Fluids::building_state(b);
                    f.bwatch.push((bi, b.id, st));
                    Self::fluids_mark_building(f, &w.tiles, b, st, w, false);
                }
            }
            if !force {
                for k in 0..old_s.0.len() {
                    let diff = (old_s.0[k] ^ f.bsolid.0[k]) | (old_f.0[k] ^ f.bfloor.0[k]);
                    if diff != 0 {
                        for bit in 0..64 {
                            if diff >> bit & 1 != 0 && k * 64 + bit < n {
                                f.wake(&w.tiles, k * 64 + bit);
                            }
                        }
                    }
                }
            }
            return;
        }
        for k in 0..f.bwatch.len() {
            let (bi, _, prev) = f.bwatch[k];
            let b = &bl[bi];
            let st = Fluids::building_state(b);
            if st != prev {
                f.bwatch[k].2 = st;
                Self::fluids_mark_building(f, &w.tiles, b, st, w, true);
            }
        }
    }

    fn fluids_mark_building(f: &mut Fluids, tiles: &[Tile], b: &rf_core::Building, st: u8, w: &rf_core::World, wake: bool) {
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                let p = Pos::new(x, y, b.z);
                if !b.contains(p) {
                    continue;
                }
                let Some(i) = w.idx(p) else { continue };
                let (was_s, was_f) = (f.bsolid.get(i), f.bfloor.get(i));
                f.bsolid.clear(i);
                f.bfloor.clear(i);
                match st {
                    1 => {
                        f.bsolid.set(i);
                    }
                    2 => {
                        f.bfloor.set(i);
                    }
                    _ => {}
                }
                if wake && (was_s != f.bsolid.get(i) || was_f != f.bfloor.get(i)) {
                    f.wake(tiles, i);
                }
            }
        }
    }

    /// [fluids] A tile was changed (dug, built, obsidian): wake liquid around it, update aquifers.
    pub(crate) fn fluids_tile_changed(&mut self, i: usize) {
        if !self.fluids.ready || self.fluids.in_pass {
            return;
        }
        let f = &mut self.fluids;
        let tiles = &mut self.world.tiles;
        let cls = f.class(&tiles[i]);
        if cls & HOLD != 0 {
            // DF: a dug aquifer tile is no aquifer any more; its neighbours leak into it.
            tiles[i].flags &= !(tile_flags::AQUIFER | 1 << 14);
        } else if depth(&tiles[i]) > 0 {
            // A wall was built over liquid: the liquid is gone.
            f.set(tiles, i, 0, false);
        }
        f.wake(tiles, i);
        let (x, y, _) = f.xyz(i);
        for &(ox, oy) in &DIRS8 {
            let (nx, ny) = (x as i32 + ox, y as i32 + oy);
            if nx < 0 || ny < 0 || nx >= f.dx as i32 || ny >= f.dy as i32 {
                continue;
            }
            let n = (i as i64 + oy as i64 * f.dx as i64 + ox as i64) as usize;
            if tiles[n].flags & tile_flags::AQUIFER != 0 && f.class(&tiles[n]) & HOLD == 0 && f.leaky_set.set(n) {
                f.leaky.push(n as u32);
            }
        }
    }

    // ------------------------------------------------------------------ units and items

    fn fluids_scan_units(&mut self, now: u64) {
        for s in 0..self.ai.len() {
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            let t = self.world.tiles[self.ai[s].pos as usize];
            let d = depth(&t);
            if d == 0 || (d < DEEP && !is_magma(&t)) {
                continue;
            }
            if self.fluids.sub.iter().any(|x| x.slot == s as u32) {
                continue;
            }
            let race = &self.world.units[self.ai[s].uidx].race;
            let immune = if is_magma(&t) { self.fluids.fireproof.contains(race) } else { self.fluids.aquatic.contains(race) || self.fluids_nobreathe(s) };
            if immune {
                continue;
            }
            self.fluids.sub.push(Sub { slot: s as u32, air: 0, burn: 0, next_swim: now + 1, warned: false });
        }
    }

    fn fluids_nobreathe(&self, s: usize) -> bool {
        let u = &self.world.units[self.ai[s].uidx];
        self.world.creature_raws.iter().find(|c| c.token == u.race).map_or(false, |cr| {
            cr.flags.iter().any(|f| f == "NOBREATHE")
                || cr.castes.iter().filter(|c| c.token == u.caste || u.caste.is_empty()).any(|c| c.flags.iter().any(|f| f == "NOBREATHE"))
        })
    }

    fn fluids_swim_skill(&self, s: usize) -> Option<i32> {
        self.world.units[self.ai[s].uidx].skills.iter().find(|k| k.id == "SWIMMING").map(|k| k.rating)
    }

    fn fluids_units(&mut self, now: u64) {
        let mut k = 0;
        while k < self.fluids.sub.len() {
            let mut e = self.fluids.sub[k];
            let s = e.slot as usize;
            if self.ai[s].kind == Kind::Dead {
                self.fluids.sub.swap_remove(k);
                continue;
            }
            let here = self.ai[s].pos as usize;
            let t = self.world.tiles[here];
            let d = depth(&t);
            let magma = is_magma(&t) && d > 0;
            if !magma && d < DEEP {
                self.fluids.sub.swap_remove(k);
                continue;
            }
            if !e.warned {
                // Flooded: drop what it is doing and get out.
                e.warned = true;
                self.cancel_task(s);
                self.ai[s].wake = now + 1;
            }
            let swim = self.fluids_swim_skill(s);
            if magma {
                e.burn += 1;
                if e.burn >= MAGMA_BURN_TICKS {
                    self.fluids.sub.swap_remove(k);
                    self.fluids.stats.burned += 1;
                    self.combat_kill(s, now, "has burned to death in magma");
                    continue;
                }
            } else {
                let breathes = d < 7 && swim.is_some();
                if breathes {
                    e.air = e.air.saturating_sub(5);
                } else if d < 7 {
                    // Struggling non-swimmer: half the air loss.
                    e.air += (now & 1) as u32;
                } else {
                    e.air += 1;
                }
                if e.air >= DROWN_TICKS {
                    self.fluids.sub.swap_remove(k);
                    self.fluids.stats.drowned += 1;
                    self.combat_kill(s, now, "has drowned");
                    continue;
                }
            }
            if now >= e.next_swim {
                let speed = self.ai[s].speed.max(100) as u64;
                // Swimming is slower than walking; non-swimmers struggle (estimate: 2x / 3x).
                e.next_swim = now + (speed * if swim.is_some() { 2 } else { 3 } / 100).max(2);
                if swim.is_some() {
                    let uidx = self.ai[s].uidx;
                    if let Some(sk) = self.world.units[uidx].skills.iter_mut().find(|k| k.id == "SWIMMING") {
                        sk.xp += 1;
                    }
                }
                self.fluids_swim(s, here, now);
            }
            self.fluids.sub[k] = e;
            k += 1;
        }
    }

    /// Move a unit in deep liquid one tile towards land or shallower liquid.
    fn fluids_swim(&mut self, s: usize, here: usize, now: u64) {
        let f = &self.fluids;
        let tiles = &self.world.tiles;
        let score = |i: usize| -> u32 {
            let t = &tiles[i];
            if self.nav.walkable(i) {
                0
            } else if is_magma(t) && depth(t) > 0 {
                100
            } else {
                1 + depth(t) as u32
            }
        };
        let cur = score(here);
        let (x, y, z) = f.xyz(here);
        let mut best = (cur, here);
        let start = (splitmix(now ^ here as u64) % 8) as usize;
        for k in 0..8 {
            let (ox, oy) = DIRS8[(start + k) & 7];
            let (nx, ny) = (x as i32 + ox, y as i32 + oy);
            if nx < 0 || ny < 0 || nx >= f.dx as i32 || ny >= f.dy as i32 {
                continue;
            }
            let n = (here as i64 + oy as i64 * f.dx as i64 + ox as i64) as usize;
            if !f.hold(tiles, n) || self.nav.shape(tiles[n].tt).is_solid() {
                continue;
            }
            let sc = score(n);
            if sc < best.0 {
                best = (sc, n);
            }
        }
        // Swim up through the surface.
        if z + 1 < f.dz && depth(&tiles[here]) >= 7 {
            let a = here + f.layer;
            if f.floorless(tiles, a) && f.hold(tiles, a) {
                let sc = score(a);
                if sc < best.0 {
                    best = (sc, a);
                }
            }
        }
        if best.1 == here {
            return;
        }
        let q = best.1;
        self.cancel_task(s);
        let p = self.world.pos_of(q);
        let ai = &mut self.ai[s];
        ai.pos = q as u32;
        ai.wake = now + 1;
        let (uidx, carrying) = (ai.uidx, ai.carrying);
        self.world.units[uidx].pos = p;
        if carrying >= 0 {
            self.set_item_pos(carrying, p);
        }
    }

    fn fluids_melt_items(&mut self, _now: u64) {
        let mut cells = std::mem::take(&mut self.fluids.magma_new);
        cells.sort_unstable();
        cells.dedup();
        let mut doomed = Vec::new();
        for it in &self.world.items {
            if !it.flags.on_ground || it.container >= 0 || it.holder >= 0 {
                continue;
            }
            let Some(i) = self.world.idx(it.pos) else { continue };
            if cells.binary_search(&(i as u32)).is_err() {
                continue;
            }
            let t = &self.world.tiles[i];
            if !(is_magma(t) && depth(t) > 0) {
                continue;
            }
            if self.world.material(it.mat).map_or(false, magma_safe) {
                continue;
            }
            doomed.push(it.id);
        }
        for id in doomed {
            if let Some(inner) = self.contents.get(&id).cloned() {
                for c in inner {
                    self.destroy_item(c);
                    self.fluids.stats.items_melted += 1;
                }
            }
            self.destroy_item(id);
            self.fluids.stats.items_melted += 1;
        }
        cells.clear();
        self.fluids.magma_new = cells;
    }

    /// [fluids] Mix fluid state into the state hash.
    pub(crate) fn fluids_hash(&self, mix: &mut impl FnMut(u64)) {
        let st = &self.fluids.stats;
        mix(st.water_volume as u64);
        mix(st.magma_volume as u64);
        mix(st.liquid_tiles as u64);
        mix(st.updates);
        mix(st.pressure_moves ^ st.obsidian << 20 ^ st.evaporated << 40);
        mix(self.fluids.next.len() as u64);
    }

    // ------------------------------------------------------------------ public API

    /// Set the liquid of a tile (0..=7; `magma` false = water) and wake the liquid around it.
    pub fn set_liquid(&mut self, p: Pos, depth_7: u8, magma: bool) {
        let Some(i) = self.world.idx(p) else { return };
        if !self.fluids.ready {
            self.fluids_init();
        }
        let f = &mut self.fluids;
        let tiles = &mut self.world.tiles;
        f.set(tiles, i, depth_7.min(7), magma);
        f.wake(tiles, i);
        f.enqueue(i);
        let reach = self.nav.compute(&self.world.tiles, i) != self.nav.flags[i];
        if reach {
            self.fluids.in_pass = true;
            self.after_tile_change(i);
            self.fluids.in_pass = false;
        } else if depth_7 > 0 && !magma && self.fluids.access.set(i) {
            self.add_water_access_around(i);
        }
        if depth_7 >= DEEP || magma {
            self.fluids.deep_new = true;
        }
    }

    /// Liquid depth (0..=7) and whether it is magma.
    pub fn liquid_at(&self, p: Pos) -> (u8, bool) {
        match self.world.tile(p) {
            Some(t) => (depth(t), is_magma(t) && depth(t) > 0),
            None => (0, false),
        }
    }

    /// Make a tile an infinite, pressurized source (like a DF river source), e.g. for tests.
    pub fn add_liquid_source(&mut self, p: Pos, magma: bool) {
        let Some(i) = self.world.idx(p) else { return };
        if !self.fluids.ready {
            self.fluids_init();
        }
        if !self.fluids.sources.iter().any(|s| s.0 == i as u32) {
            self.fluids.sources.push((i as u32, magma));
        }
        self.set_liquid(p, 7, magma);
    }

    /// Mud left by water on a floor (DF: allows farm plots on stone).
    pub fn has_mud(&self, p: Pos) -> bool {
        match self.world.idx(p) {
            Some(i) if self.fluids.ready => self.fluids.mud.get(i),
            _ => false,
        }
    }

    /// Turn the fluid simulation on or off (for benchmarks; on by default).
    pub fn set_fluids(&mut self, on: bool) {
        self.fluids.on = on;
    }

    /// Fluid counters and the current state.
    pub fn fluid_report(&self) -> FluidReport {
        let mut r = self.fluids.stats.clone();
        if !self.fluids.ready {
            for t in &self.world.tiles {
                let d = depth(t);
                if d > 0 {
                    r.liquid_tiles += 1;
                    if is_magma(t) {
                        r.magma_tiles += 1;
                        r.magma_volume += d as i64;
                    } else {
                        r.water_volume += d as i64;
                    }
                }
            }
        }
        r.flowing = self.fluids.next.len();
        r.leaky_aquifer_tiles = self.fluids.leaky.len();
        r.mud_tiles = self.fluids.mud_count;
        r.submerged = self.fluids.sub.len();
        r
    }
}
