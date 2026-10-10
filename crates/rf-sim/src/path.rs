//! A* over the navigation grid (DF uses A* with an admissible heuristic, wiki "Path").
//!
//! - Cost of a step = traffic cost of the entered tile (1/2/5/25).
//! - Heuristic = max(chebyshev_xy, |dz|) * min_cost to the nearest goal (admissible because
//!   one step moves at most one tile in x, y and z each).
//! - Node storage is an open-addressing hash table with generation stamps, so a search
//!   never touches memory proportional to the map size and needs no clearing.
//! - Ties are broken by (f, h, tile index), so results are deterministic.
//! - [perf] A node's fields share one 16-byte slot, heap entries remember their slot, and a
//!   table that grew for one huge search shrinks back after a run of small ones (so it stays
//!   in the CPU cache). Expansion order and tie-breaks are unchanged, so every path is
//!   exactly the one the earlier version found.

use crate::nav::{Nav, LM_INF, LM_K, MAX_NEIGHBORS};
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

pub const MAX_GOALS: usize = 16;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub struct Goals {
    pub n: usize,
    pub t: [u32; MAX_GOALS],
}

impl Default for Goals {
    fn default() -> Self {
        Goals { n: 0, t: [0; MAX_GOALS] }
    }
}

impl Goals {
    pub fn one(t: u32) -> Goals {
        let mut g = Goals::default();
        g.push(t);
        g
    }
    pub fn push(&mut self, t: u32) -> bool {
        if self.n >= MAX_GOALS || self.t[..self.n].contains(&t) {
            return false;
        }
        self.t[self.n] = t;
        self.n += 1;
        true
    }
    pub fn as_slice(&self) -> &[u32] {
        &self.t[..self.n]
    }
}

#[derive(Clone, Copy)]
struct Entry {
    f: u32,
    h: u32,
    idx: u32,
    /// [perf] Table slot of `idx` when pushed (checked on pop: a grow moves nodes).
    slot: u32,
}

// Heap order by (f, h, tile index) only; `slot` is just a lookup hint.
impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        (self.f, self.h, self.idx) == (o.f, o.h, o.idx)
    }
}
impl Eq for Entry {}
impl Ord for Entry {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.cmp(&self.f).then(o.h.cmp(&self.h)).then(o.idx.cmp(&self.idx))
    }
}
impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// [perf] Priority queue for A*: one min-heap of (h, tile) per f value, popped in increasing f.
///
/// Pops exactly the entry a single heap ordered by (f, h, tile) would pop: all keys are
/// distinct (a tile is only pushed again with a smaller g, hence a smaller f), and f never
/// drops below the last popped f because the heuristic is consistent (one step changes each
/// coordinate by at most 1 and costs at least `min_cost`). The per-f heaps are much smaller
/// than one big heap and compare a single integer.
#[derive(Default)]
struct Queue {
    base: u32,
    cur: usize,
    len: usize,
    used: usize,
    buckets: Vec<BinaryHeap<Reverse<(u64, u32)>>>,
}

impl Queue {
    fn clear(&mut self) {
        for b in &mut self.buckets[..self.used] {
            b.clear();
        }
        self.used = 0;
        self.cur = 0;
        self.len = 0;
    }

    #[inline]
    fn push(&mut self, e: Entry) {
        debug_assert!(e.f >= self.base + self.cur as u32, "A* f went down: inconsistent heuristic");
        let k = (e.f - self.base) as usize;
        if k >= self.buckets.len() {
            self.buckets.resize_with(k + 1, BinaryHeap::new);
        }
        self.used = self.used.max(k + 1);
        self.buckets[k].push(Reverse((((e.h as u64) << 32) | e.idx as u64, e.slot)));
        self.len += 1;
    }

    #[inline]
    fn pop(&mut self) -> Option<Entry> {
        if self.len == 0 {
            return None;
        }
        while self.buckets[self.cur].is_empty() {
            self.cur += 1;
        }
        let Reverse((key, slot)) = self.buckets[self.cur].pop()?;
        self.len -= 1;
        Some(Entry { f: self.base + self.cur as u32, h: (key >> 32) as u32, idx: key as u32, slot })
    }
}

/// [perf10] Landmark lower bound `max_j max(D_j(g) - D_j(n), D_j(n) - D_j(g) + off)` over the
/// landmarks known at both tiles (`i32::MIN` if none), with `off = cost(g) - cost(n)`.
///
/// Branch-free so the compiler vectorizes it (16 lanes); the value is exactly the one the
/// old per-landmark loop folded into the heuristic, so every A* search expands the same nodes.
#[inline(always)]
fn alt_bound(g: &[u16; LM_K], n: &[u16; LM_K], off: i32) -> i32 {
    let mut m = i32::MIN;
    for j in 0..LM_K {
        let (a, b) = (g[j] as i32, n[j] as i32);
        let v = (a - b).max(b - a + off);
        let ok = g[j] != LM_INF && n[j] != LM_INF;
        m = m.max(if ok { v } else { i32::MIN });
    }
    m
}

const NO_PARENT: u32 = u32::MAX >> 1;
/// [perf] Closed flag in the top bit of `Node::parent` (tile indices are below 2^31).
const CLOSED: u32 = 1 << 31;
const MIN_BITS: u32 = 12;

/// [perf] One hash table slot with all fields of a node (four slots per cache line).
#[derive(Clone, Copy, Default)]
struct Node {
    stamp: u32,
    key: u32,
    g: u32,
    /// Parent tile, plus `CLOSED`.
    parent: u32,
}

pub struct AStar {
    nodes: Vec<Node>,
    gen: u32,
    bits: u32,
    len: usize,
    /// [perf] Searches in a row that used less than 1/16 of a grown table.
    small_runs: u32,
    heap: Queue,
    pub expanded_total: u64,
    pub last_expanded: u32,
}

impl Default for AStar {
    fn default() -> Self {
        AStar::new()
    }
}

impl AStar {
    pub fn new() -> AStar {
        AStar {
            nodes: vec![Node::default(); 1usize << MIN_BITS],
            gen: 0,
            bits: MIN_BITS,
            len: 0,
            small_runs: 0,
            heap: Queue::default(),
            expanded_total: 0,
            last_expanded: 0,
        }
    }

    #[inline]
    fn hash(&self, k: u32) -> usize {
        ((k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - self.bits)) as usize
    }

    /// Slot of `k`, inserting it (g = u32::MAX) if missing.
    #[inline]
    fn slot(&mut self, k: u32) -> usize {
        if self.len * 2 >= self.nodes.len() {
            self.resize(self.bits + 1);
        }
        let mask = self.nodes.len() - 1;
        let mut s = self.hash(k);
        loop {
            let n = &mut self.nodes[s];
            if n.stamp != self.gen {
                *n = Node { stamp: self.gen, key: k, g: u32::MAX, parent: NO_PARENT };
                self.len += 1;
                return s;
            }
            if n.key == k {
                return s;
            }
            s = (s + 1) & mask;
        }
    }

    #[inline]
    fn find(&self, k: u32) -> Option<usize> {
        let mask = self.nodes.len() - 1;
        let mut s = self.hash(k);
        loop {
            let n = &self.nodes[s];
            if n.stamp != self.gen {
                return None;
            }
            if n.key == k {
                return Some(s);
            }
            s = (s + 1) & mask;
        }
    }

    fn resize(&mut self, bits: u32) {
        let old: Vec<Node> = self.nodes.iter().copied().filter(|n| n.stamp == self.gen).collect();
        self.bits = bits;
        self.nodes = vec![Node::default(); 1usize << bits];
        self.gen = 1;
        self.len = 0;
        let mask = self.nodes.len() - 1;
        for mut n in old {
            let mut s = self.hash(n.key);
            while self.nodes[s].stamp == self.gen {
                s = (s + 1) & mask;
            }
            n.stamp = self.gen;
            self.nodes[s] = n;
            self.len += 1;
        }
    }

    fn reset(&mut self) {
        // [perf] Back to the small table after a run of small searches.
        if self.bits > MIN_BITS && self.len * 16 < self.nodes.len() {
            self.small_runs += 1;
            if self.small_runs >= 64 {
                self.small_runs = 0;
                self.bits = MIN_BITS;
                self.nodes = vec![Node::default(); 1usize << MIN_BITS];
                self.gen = 0;
            }
        } else {
            self.small_runs = 0;
        }
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            self.nodes.iter_mut().for_each(|n| n.stamp = 0);
            self.gen = 1;
        }
        self.len = 0;
        self.heap.clear();
    }

    /// Find a cheapest path from `start` to any goal. Writes the tiles after `start` up to and
    /// including the goal into `out`. Returns false if no path within `max_nodes` expansions.
    pub fn search(&mut self, nav: &Nav, start: u32, goals: &Goals, max_nodes: u32, out: &mut Vec<u32>) -> bool {
        out.clear();
        if goals.n == 0 {
            return false;
        }
        self.last_expanded = 0;
        if goals.as_slice().contains(&start) {
            return true;
        }
        self.reset();
        let mut gp = [(0i32, 0i32, 0i32); MAX_GOALS];
        for (k, &t) in goals.as_slice().iter().enumerate() {
            let (x, y, z) = nav.xyz(t as usize);
            gp[k] = (x as i32, y as i32, z as i32);
        }
        let gp = &gp[..goals.n];
        let mc = nav.min_cost;
        // [perf9] Landmark (ALT) bounds per goal, see `nav::Landmarks`.
        let mut gd = [[LM_INF; LM_K]; MAX_GOALS];
        let mut gc = [0i32; MAX_GOALS];
        let mut alt = false;
        for (k, &t) in goals.as_slice().iter().enumerate() {
            if let Some(d) = nav.lm_dist(t as usize) {
                gd[k] = *d;
                gc[k] = nav.cost(t as usize) as i32;
                alt = true;
            }
        }
        let gd = &gd[..goals.n];
        let h = |i: u32| -> u32 {
            let (x, y, z) = nav.xyz(i as usize);
            let (x, y, z) = (x as i32, y as i32, z as i32);
            let dn = if alt { nav.lm_dist(i as usize) } else { None };
            let Some(dn) = dn else {
                let mut best = u32::MAX;
                for &(gx, gy, gz) in gp {
                    let d = ((gx - x).abs().max((gy - y).abs())).max((gz - z).abs()) as u32;
                    best = best.min(d);
                }
                return best * mc;
            };
            let cn = nav.cost(i as usize) as i32;
            let mut best = i32::MAX;
            for (k, &(gx, gy, gz)) in gp.iter().enumerate() {
                let hk = ((gx - x).abs().max((gy - y).abs())).max((gz - z).abs()) * mc as i32;
                best = best.min(hk.max(alt_bound(&gd[k], dn, gc[k] - cn))); // [perf10]
            }
            best as u32
        };
        let s0 = self.slot(start);
        self.nodes[s0].g = 0;
        let h0 = h(start);
        self.heap.base = h0;
        self.heap.push(Entry { f: h0, h: h0, idx: start, slot: s0 as u32 });
        let mut buf = [0u32; MAX_NEIGHBORS];
        let mut expanded = 0u32;
        while let Some(e) = self.heap.pop() {
            let hint = self.nodes[e.slot as usize];
            let s = if hint.stamp == self.gen && hint.key == e.idx {
                e.slot as usize
            } else {
                match self.find(e.idx) {
                    Some(s) => s,
                    None => continue,
                }
            };
            if self.nodes[s].parent & CLOSED != 0 {
                continue;
            }
            self.nodes[s].parent |= CLOSED;
            let g = self.nodes[s].g;
            if goals.as_slice().contains(&e.idx) {
                // Reconstruct.
                let mut cur = e.idx;
                while cur != start {
                    out.push(cur);
                    let cs = self.find(cur).unwrap();
                    cur = self.nodes[cs].parent & !CLOSED;
                }
                out.reverse();
                self.expanded_total += expanded as u64;
                self.last_expanded = expanded;
                return true;
            }
            expanded += 1;
            if expanded > max_nodes {
                break;
            }
            let n = nav.neighbors(e.idx as usize, &mut buf);
            for &q in &buf[..n] {
                let ng = g + nav.cost(q as usize);
                let qs = self.slot(q);
                let node = &mut self.nodes[qs];
                if node.parent & CLOSED != 0 || ng >= node.g {
                    continue;
                }
                node.g = ng;
                node.parent = e.idx;
                let hq = h(q);
                // [perf9] Pathmax: f never drops below the popped f (a no-op for a consistent
                // heuristic; keeps the bucket queue monotone if a table is ever off).
                self.heap.push(Entry { f: (ng + hq).max(e.f), h: hq, idx: q, slot: qs as u32 });
            }
        }
        self.expanded_total += expanded as u64;
        self.last_expanded = expanded;
        false
    }
}
