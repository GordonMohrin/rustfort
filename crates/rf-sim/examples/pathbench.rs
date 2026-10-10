//! [perf] A* benchmark on the real fort: fort-internal trips between the places dwarves go.
//!
//!   cargo run -p rf-sim --release --example pathbench -- [--export <dir>] [--n 3000] [--rounds 3]
//!
//! Start and goal tiles are drawn (deterministically) from where citizens stand and where
//! food, drink, beds and workshops are, keeping only reachable pairs. Prints the time per
//! search and a checksum over all paths: an A* change that keeps the paths exactly the same
//! keeps the checksum.

use rf_core::Pos;
use rf_sim::Sim;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let n: usize = get("--n").and_then(|s| s.parse().ok()).unwrap_or(3000);
    let rounds: usize = get("--rounds").and_then(|s| s.parse().ok()).unwrap_or(3);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let sim = Sim::new(world, 1);
    let mut spots: Vec<Pos> = sim.world.citizens().map(|u| u.pos).collect();
    for it in &sim.world.items {
        if matches!(it.kind.as_str(), "DRINK" | "PLANT" | "FOOD" | "MEAT" | "BOULDER" | "WOOD" | "BAR") && it.flags.on_ground {
            spots.push(it.pos);
        }
    }
    for b in &sim.world.buildings {
        if matches!(b.kind.as_str(), "Bed" | "Workshop" | "Furnace" | "Table" | "Chair") {
            spots.push(b.center);
        }
    }
    spots.retain(|&p| sim.is_walkable(p));
    spots.sort_by_key(|p| (p.z, p.y, p.x));
    spots.dedup();
    let mut seed = 0x1234_5678_u64;
    let mut rnd = |m: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) % m as u64) as usize
    };
    let mut pairs = Vec::new();
    let mut tries = 0;
    while pairs.len() < n && tries < n * 50 {
        tries += 1;
        let (a, b) = (spots[rnd(spots.len())], spots[rnd(spots.len())]);
        if a != b && sim.reachable(a, b) {
            pairs.push((a, b));
        }
    }
    println!("{} spots, {} reachable pairs", spots.len(), pairs.len());
    // The terrain grid (without the few buildings that block movement), searched directly so
    // the expanded node count is known.
    let nav = rf_sim::nav::Nav::new(&sim.world, &[]);
    let high = (0..nav.flags.len()).filter(|&i| nav.walkable(i) && nav.cost(i) == 1).count();
    println!("min traffic cost {}, {high} walkable high-traffic tiles", nav.min_cost);
    let idx = |p: Pos| sim.world.idx(p).unwrap() as u32;
    let mut astar = rf_sim::path::AStar::new();
    let mut out = Vec::new();
    let mut best = f64::MAX;
    let mut check = 0u64;
    let mut tiles = 0usize;
    let mut nodes = 0u64;
    for _ in 0..rounds {
        let mut h: u64 = 0xcbf29ce484222325;
        tiles = 0;
        nodes = 0;
        let t = Instant::now();
        for &(a, b) in &pairs {
            let ok = astar.search(&nav, idx(a), &rf_sim::path::Goals::one(idx(b)), 400_000, &mut out);
            nodes += astar.last_expanded as u64;
            if ok {
                tiles += out.len();
                for &q in &out {
                    h ^= q as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
            } else {
                h ^= 0xdead;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        best = best.min(t.elapsed().as_secs_f64());
        check = h;
    }
    if std::env::var_os("RF_NBENCH").is_some() {
        // Cost of the neighbour generation alone, on the tiles of all found paths.
        let mut all = Vec::new();
        for &(a, b) in &pairs {
            if astar.search(&nav, idx(a), &rf_sim::path::Goals::one(idx(b)), 400_000, &mut out) {
                all.extend_from_slice(&out);
            }
        }
        let mut buf = [0u32; rf_sim::nav::MAX_NEIGHBORS];
        let t = Instant::now();
        let mut acc = 0usize;
        for _ in 0..10 {
            for &q in &all {
                acc += nav.neighbors(q as usize, &mut buf);
            }
        }
        let dt = t.elapsed().as_secs_f64();
        println!("neighbors: {:.1} ns/call ({} calls, {:.1} neighbours each)", dt * 1e9 / (all.len() * 10) as f64, all.len() * 10, acc as f64 / (all.len() * 10) as f64);
        let t = Instant::now();
        let mut acc = 0u64;
        for _ in 0..10 {
            for &q in &all {
                let (x, y, z) = nav.xyz(q as usize);
                acc += (x + y + z) as u64;
            }
        }
        let dt = t.elapsed().as_secs_f64();
        println!("xyz: {:.2} ns/call ({acc})", dt * 1e9 / (all.len() * 10) as f64);
    }
    println!(
        "{} searches: best of {rounds} rounds {:.1} us/search, {:.0} nodes/search, {:.1} ns/node, avg path {:.1} tiles, path checksum {check:016x}",
        pairs.len(),
        best * 1e6 / pairs.len() as f64,
        nodes as f64 / pairs.len() as f64,
        best * 1e9 / nodes.max(1) as f64,
        tiles as f64 / pairs.len() as f64
    );
}
