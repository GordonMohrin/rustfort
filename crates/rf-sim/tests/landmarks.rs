//! [perf9] The landmark (ALT) heuristic keeps A* optimal: same path costs as the plain
//! distance heuristic, also after digging changed the map (incremental table repair).

use rf_sim::gen;
use rf_sim::nav::Nav;
use rf_sim::path::{AStar, Goals};

fn lcg(s: &mut u64) -> u64 {
    *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *s >> 33
}

fn cost(nav: &Nav, path: &[u32]) -> u32 {
    path.iter().map(|&t| nav.cost(t as usize)).sum()
}

fn pairs(nav: &Nav, n: usize, seed: u64) -> Vec<(u32, u32)> {
    let walk: Vec<u32> = (0..nav.flags.len()).filter(|&i| nav.walkable(i)).map(|i| i as u32).collect();
    let mut s = seed;
    let mut out = Vec::new();
    while out.len() < n {
        let a = walk[lcg(&mut s) as usize % walk.len()];
        let b = walk[lcg(&mut s) as usize % walk.len()];
        if nav.comp_root(a as usize) == nav.comp_root(b as usize) {
            out.push((a, b));
        }
    }
    out
}

/// Optimal cost from `a` to `b` with the landmark heuristic on or off.
fn search(nav: &mut Nav, a: u32, b: u32, lm: bool, astar: &mut AStar) -> Option<(u32, u32)> {
    let was = nav.lm.on;
    nav.lm.on = lm && was;
    let mut out = Vec::new();
    let ok = astar.search(nav, a, &Goals::one(b), 2_000_000, &mut out);
    nav.lm.on = was;
    ok.then(|| (cost(nav, &out), astar.last_expanded))
}

#[test]
fn landmarks_keep_paths_optimal_and_expand_less() {
    let w = gen::bench_world(64, 64, 30, 10, 3);
    let mut nav = Nav::new(&w, &[]);
    assert!(nav.lm.on, "bench world gets landmarks");
    let mut a = AStar::new();
    let (mut n_lm, mut n_plain) = (0u64, 0u64);
    for (s, g) in pairs(&nav, 400, 7) {
        let x = search(&mut nav, s, g, true, &mut a).expect("path with landmarks");
        let y = search(&mut nav, s, g, false, &mut a).expect("path without landmarks");
        assert_eq!(x.0, y.0, "same optimal cost {s} -> {g}");
        n_lm += x.1 as u64;
        n_plain += y.1 as u64;
    }
    assert!(n_lm * 2 < n_plain, "landmarks should expand far fewer nodes: {n_lm} vs {n_plain}");
}

#[test]
fn landmarks_stay_admissible_after_digging() {
    let mut w = gen::bench_world(64, 64, 30, 10, 5);
    let mut nav = Nav::new(&w, &[]);
    let floor = w.tiletypes.by_name("StoneFloor1").expect("floor");
    let stair = w.tiletypes.by_name("StoneStairUD").expect("stairs");
    // Dig 600 solid tiles next to walkable ones: floors, and up/down stairs that connect levels.
    let mut s = 99u64;
    let mut dug = 0;
    let (dx, dy, dz) = (nav.dx, nav.dy, nav.dz);
    while dug < 600 {
        let x = 1 + lcg(&mut s) as usize % (dx - 2);
        let y = 1 + lcg(&mut s) as usize % (dy - 2);
        let z = 1 + lcg(&mut s) as usize % (dz - 2);
        let i = nav.idx(x, y, z);
        if !w.tiletypes.get(w.tiles[i].tt).shape.is_solid() {
            continue;
        }
        let next = [nav.idx(x - 1, y, z), nav.idx(x + 1, y, z), nav.idx(x, y - 1, z), nav.idx(x, y + 1, z), i - nav.layer, i + nav.layer];
        if !next.iter().any(|&q| nav.walkable(q)) {
            continue;
        }
        w.tiles[i].tt = if lcg(&mut s) % 3 == 0 { stair } else { floor };
        nav.update_around(&w.tiles, x, y, z);
        dug += 1;
    }
    nav.rebuild_components();
    assert!(nav.lm.on, "tables kept");
    let mut fresh = Nav::new(&w, &[]);
    let mut a = AStar::new();
    for (s, g) in pairs(&fresh, 400, 11) {
        let x = search(&mut nav, s, g, true, &mut a).expect("path on the repaired table");
        let y = search(&mut fresh, s, g, false, &mut a).expect("path on the fresh map");
        assert_eq!(x.0, y.0, "same optimal cost {s} -> {g} after digging");
    }
}
