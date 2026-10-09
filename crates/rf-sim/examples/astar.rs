//! A* micro-benchmark on the bench world: random walkable pairs in the hall.
//!   cargo run -p rf-sim --release --example astar -- [--n 2000]

use rf_core::{Pos, Shape};
use rf_sim::nav::Nav;
use rf_sim::path::{AStar, Goals};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.iter().position(|a| a == "--n").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let w = rf_sim::gen::bench_world(192, 192, 60, 10, 1);
    let t = Instant::now();
    let nav = Nav::new(&w, &[]);
    println!("nav init {:.1} ms, {} components", t.elapsed().as_secs_f64() * 1e3, nav.component_count());
    let l = rf_sim::gen::bench_layout(192, 192, 60);
    let mut seed = 12345u64;
    let mut rnd = |m: i16| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) % m as u64) as i16
    };
    let (x1, y1, x2, y2) = l.hall;
    let mut pairs = Vec::new();
    while pairs.len() < n {
        let a = Pos::new(x1 + rnd(x2 - x1), y1 + rnd(y2 - y1), l.hall_z);
        let b = Pos::new(x1 + rnd(x2 - x1), y1 + rnd(y2 - y1), l.hall_z);
        if w.shape(a) == Shape::Floor && w.shape(b) == Shape::Floor {
            pairs.push((w.idx(a).unwrap() as u32, w.idx(b).unwrap() as u32));
        }
    }
    let mut a = AStar::new();
    let mut out = Vec::new();
    let t = Instant::now();
    let mut nodes = 0u64;
    let mut len = 0usize;
    for &(s, g) in &pairs {
        assert!(a.search(&nav, s, &Goals::one(g), 1_000_000, &mut out));
        nodes += a.last_expanded as u64;
        len += out.len();
    }
    let dt = t.elapsed().as_secs_f64();
    println!(
        "{} searches: {:.1} us/search, {:.0} nodes/search, {:.1} ns/node, avg path {:.1} tiles",
        n,
        dt * 1e6 / n as f64,
        nodes as f64 / n as f64,
        dt * 1e9 / nodes as f64,
        len as f64 / n as f64
    );
}
