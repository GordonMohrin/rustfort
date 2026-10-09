//! Headless benchmark: ticks/s against population on a large synthetic fort.
//!
//!   cargo run -p rf-sim --release --example bench -- [--units 50,100,200,400,800] [--ticks 2000]
//!       [--runs 3] [--warmup 500] [--size 192x192x60] [--threads N]

use rf_sim::{gen, Sim};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let pops: Vec<usize> = get("--units")
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![50, 100, 200, 400, 800]);
    let ticks: u32 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(2000);
    let runs: usize = get("--runs").and_then(|s| s.parse().ok()).unwrap_or(3);
    let warmup: u32 = get("--warmup").and_then(|s| s.parse().ok()).unwrap_or(500);
    let size = get("--size").unwrap_or_else(|| "192x192x60".into());
    let dims: Vec<u16> = size.split('x').filter_map(|x| x.parse().ok()).collect();
    let (dx, dy, dz) = (dims[0], dims[1], dims[2]);
    if let Some(n) = get("--threads").and_then(|s| s.parse::<usize>().ok()) {
        rayon::ThreadPoolBuilder::new().num_threads(n).build_global().ok();
    }
    println!(
        "rf-sim bench: map {dx}x{dy}x{dz}, {ticks} ticks x {runs} runs after {warmup} warm-up ticks, {} rayon threads",
        rayon::current_num_threads()
    );
    println!(
        "{:>6} {:>7} {:>10} {:>10} {:>8} {:>8} {:>8} {:>8} {:>9} {:>9} {:>7} {:>7}",
        "dwarfs", "units", "ticks/s", "us/tick", "think%", "paths%", "commit%", "items%", "paths/tk", "nodes/pth", "jobs", "hauled"
    );
    for &n in &pops {
        let t0 = Instant::now();
        let world = gen::bench_world(dx, dy, dz, n, 42);
        let total_units = world.units.len();
        let mut sim = Sim::new(world, 42);
        let build = t0.elapsed().as_secs_f64();
        sim.run_ticks(warmup);
        let mut rates = Vec::new();
        let mut phase_sum = [0.0f64; 4];
        let before = sim.stats();
        for _ in 0..runs {
            let t = Instant::now();
            sim.run_ticks(ticks);
            let dt = t.elapsed().as_secs_f64();
            rates.push(ticks as f64 / dt);
            for (k, (_, v)) in sim.stats().phase_times.iter().enumerate() {
                phase_sum[k] += v;
            }
        }
        let after = sim.stats();
        rates.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = rates[rates.len() / 2];
        let total: f64 = phase_sum.iter().sum::<f64>().max(1e-12);
        let tk = (ticks as usize * runs) as f64;
        let paths = (after.paths - before.paths) as f64;
        let nodes = (after.path_nodes - before.path_nodes) as f64;
        println!(
            "{:>6} {:>7} {:>10.0} {:>10.1} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>9.2} {:>9.0} {:>7} {:>7}",
            n,
            total_units,
            median,
            1e6 / median,
            100.0 * phase_sum[0] / total,
            100.0 * phase_sum[1] / total,
            100.0 * phase_sum[2] / total,
            100.0 * phase_sum[3] / total,
            paths / tk,
            if paths > 0.0 { nodes / paths } else { 0.0 },
            after.jobs_done,
            after.items_hauled,
        );
        eprintln!(
            "    (world build + nav init {build:.2}s; open jobs {}, dead {}, path failures {}, cache hits {}, nav rebuilds {}, items {})",
            after.open_jobs, after.dead_units, after.path_failures, after.path_cache_hits, after.nav_rebuilds, after.items
        );
    }
}
