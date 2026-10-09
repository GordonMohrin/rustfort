//! A* over the navigation grid (DF uses A* with an admissible heuristic, wiki "Path").
//!
//! - Cost of a step = traffic cost of the entered tile (1/2/5/25).
//! - Heuristic = max(chebyshev_xy, |dz|) * min_cost to the nearest goal (admissible because
//!   one step moves at most one tile in x, y and z each).
//! - Node storage is an open-addressing hash table with generation stamps, so a search
//!   never touches memory proportional to the map size and needs no clearing.
//! - Ties are broken by (f, h, tile index), so results are deterministic.

use crate::nav::{Nav, MAX_NEIGHBORS};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub const MAX_GOALS: usize = 16;

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct Entry {
    f: u32,
    h: u32,
    idx: u32,
}

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

const NO_PARENT: u32 = u32::MAX;

pub struct AStar {
    stamp: Vec<u32>,
    key: Vec<u32>,
    g: Vec<u32>,
    parent: Vec<u32>,
    closed: Vec<bool>,
    gen: u32,
    bits: u32,
    len: usize,
    heap: BinaryHeap<Entry>,
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
        let bits = 12;
        let cap = 1usize << bits;
        AStar {
            stamp: vec![0; cap],
            key: vec![0; cap],
            g: vec![0; cap],
            parent: vec![0; cap],
            closed: vec![false; cap],
            gen: 0,
            bits,
            len: 0,
            heap: BinaryHeap::with_capacity(1024),
            expanded_total: 0,
            last_expanded: 0,
        }
    }

    #[inline]
    fn hash(&self, k: u32) -> usize {
        ((k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - self.bits)) as usize
    }

    /// Slot of `k`, inserting it (g = u32::MAX) if missing. Returns (slot, was_new).
    #[inline]
    fn slot(&mut self, k: u32) -> (usize, bool) {
        if self.len * 2 >= self.stamp.len() {
            self.grow();
        }
        let mask = self.stamp.len() - 1;
        let mut s = self.hash(k);
        loop {
            if self.stamp[s] != self.gen {
                self.stamp[s] = self.gen;
                self.key[s] = k;
                self.g[s] = u32::MAX;
                self.parent[s] = NO_PARENT;
                self.closed[s] = false;
                self.len += 1;
                return (s, true);
            }
            if self.key[s] == k {
                return (s, false);
            }
            s = (s + 1) & mask;
        }
    }

    fn find(&self, k: u32) -> Option<usize> {
        let mask = self.stamp.len() - 1;
        let mut s = self.hash(k);
        loop {
            if self.stamp[s] != self.gen {
                return None;
            }
            if self.key[s] == k {
                return Some(s);
            }
            s = (s + 1) & mask;
        }
    }

    fn grow(&mut self) {
        let old: Vec<(u32, u32, u32, bool)> = (0..self.stamp.len())
            .filter(|&s| self.stamp[s] == self.gen)
            .map(|s| (self.key[s], self.g[s], self.parent[s], self.closed[s]))
            .collect();
        self.bits += 1;
        let cap = 1usize << self.bits;
        self.stamp = vec![0; cap];
        self.key = vec![0; cap];
        self.g = vec![0; cap];
        self.parent = vec![0; cap];
        self.closed = vec![false; cap];
        self.gen = 1;
        self.len = 0;
        let mask = cap - 1;
        for (k, g, p, c) in old {
            let mut s = self.hash(k);
            while self.stamp[s] == self.gen {
                s = (s + 1) & mask;
            }
            self.stamp[s] = self.gen;
            self.key[s] = k;
            self.g[s] = g;
            self.parent[s] = p;
            self.closed[s] = c;
            self.len += 1;
        }
    }

    fn reset(&mut self) {
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            self.stamp.iter_mut().for_each(|s| *s = 0);
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
        let h = |i: u32| -> u32 {
            let (x, y, z) = nav.xyz(i as usize);
            let (x, y, z) = (x as i32, y as i32, z as i32);
            let mut best = u32::MAX;
            for &(gx, gy, gz) in gp {
                let d = ((gx - x).abs().max((gy - y).abs())).max((gz - z).abs()) as u32;
                best = best.min(d);
            }
            best * mc
        };
        let (s0, _) = self.slot(start);
        self.g[s0] = 0;
        let h0 = h(start);
        self.heap.push(Entry { f: h0, h: h0, idx: start });
        let mut buf = [0u32; MAX_NEIGHBORS];
        let mut expanded = 0u32;
        while let Some(e) = self.heap.pop() {
            let s = match self.find(e.idx) {
                Some(s) => s,
                None => continue,
            };
            if self.closed[s] {
                continue;
            }
            self.closed[s] = true;
            let g = self.g[s];
            if goals.as_slice().contains(&e.idx) {
                // Reconstruct.
                let mut cur = e.idx;
                while cur != start {
                    out.push(cur);
                    let cs = self.find(cur).unwrap();
                    cur = self.parent[cs];
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
                let (qs, _) = self.slot(q);
                if self.closed[qs] || ng >= self.g[qs] {
                    continue;
                }
                self.g[qs] = ng;
                self.parent[qs] = e.idx;
                let hq = h(q);
                self.heap.push(Entry { f: ng + hq, h: hq, idx: q });
            }
        }
        self.expanded_total += expanded as u64;
        self.last_expanded = expanded;
        false
    }
}
