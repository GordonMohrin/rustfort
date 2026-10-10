//! [perf9] Determinism stress test on a real export: the same scenario with many rayon thread
//! counts and parallel thresholds, while other threads load every CPU core.
//!
//!   cargo run -p rf-sim --release --example determinism -- [--export <dir>] [--ticks 20000]
//!       [--every 500] [--threads 1,2,3,4,6,12] [--par 1,8,1000] [--load 12] [--extra 0]
//!       [--invasions] [--repeat 1] [--seed 1]
//!
//! Every configuration runs in its own rayon pool (`install`), so the sim is also created on a
//! different OS thread each time (different `std` hash keys). State hashes are compared at
//! every checkpoint (`--every` ticks); the first divergence is printed with the units whose
//! position differs. Exit code 1 on any divergence.

use rf_sim::Sim;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[path = "scale.rs"]
#[allow(dead_code)]
mod scale;

struct Run {
    label: String,
    hashes: Vec<(u64, u64)>,
    units: Vec<(i32, rf_core::Pos, bool)>,
    tps: f64,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let list = |name: &str, def: &str| -> Vec<usize> { get(name).unwrap_or_else(|| def.into()).split(',').filter_map(|s| s.trim().parse().ok()).collect() };
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let every: u64 = get("--every").and_then(|s| s.parse().ok()).unwrap_or(500).max(1);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let extra: usize = get("--extra").and_then(|s| s.parse().ok()).unwrap_or(0);
    let repeat: usize = get("--repeat").and_then(|s| s.parse().ok()).unwrap_or(1);
    let threads = list("--threads", "1,2,3,4,6,12");
    let pars = list("--par", "1,8,1000");
    let load: usize = get("--load").and_then(|s| s.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(2);
    };
    let (mut world, _) = rf_import::load_export(&dir).expect("load export");
    scale::add_citizens(&mut world, extra, seed);
    let invasions = flag("--invasions");

    // Background load: busy threads that compete with the rayon workers for every core.
    let stop = Arc::new(AtomicBool::new(false));
    let mut hogs = Vec::new();
    for k in 0..load {
        let stop = stop.clone();
        hogs.push(std::thread::spawn(move || {
            let mut x = k as u64 + 1;
            while !stop.load(Ordering::Relaxed) {
                for _ in 0..10_000 {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                }
                std::hint::black_box(x);
                if x % 97 == 0 {
                    std::thread::yield_now();
                }
            }
        }));
    }

    let mut runs: Vec<Run> = Vec::new();
    for rep in 0..repeat {
        for &t in &threads {
            for &par in &pars {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(t).build().unwrap();
                let w = world.clone();
                let t0 = std::time::Instant::now();
                let (hashes, units) = pool.install(|| {
                    let mut sim = Sim::new(w, seed);
                    sim.set_invasions(invasions);
                    sim.set_parallel_threshold(par);
                    let mut hashes = Vec::new();
                    let mut done = 0;
                    while done < ticks {
                        let n = every.min(ticks - done);
                        sim.run_ticks(n as u32);
                        done += n;
                        hashes.push((sim.ticks(), sim.state_hash()));
                        sim.drain_announcements();
                    }
                    let units = sim.world.units.iter().map(|u| (u.id, u.pos, u.flags.dead)).collect::<Vec<_>>();
                    (hashes, units)
                });
                let tps = ticks as f64 / t0.elapsed().as_secs_f64().max(1e-9);
                let label = format!("rep {rep} threads {t:>2} par {par:>4}");
                println!("{label}: final {:016x}  {tps:>7.0} ticks/s", hashes.last().map(|h| h.1).unwrap_or(0));
                runs.push(Run { label, hashes, units, tps });
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
    for h in hogs {
        let _ = h.join();
    }

    let base = &runs[0];
    let mut bad = 0;
    for r in &runs[1..] {
        if let Some(k) = (0..base.hashes.len()).find(|&k| base.hashes[k] != r.hashes[k]) {
            bad += 1;
            println!("DIVERGED: {} vs {} first at tick {} ({:016x} vs {:016x})", r.label, base.label, r.hashes[k].0, base.hashes[k].1, r.hashes[k].1);
            let diffs: Vec<String> = base
                .units
                .iter()
                .zip(&r.units)
                .filter(|(a, b)| a != b)
                .take(8)
                .map(|(a, b)| format!("unit {} {:?}{} vs {:?}{}", a.0, a.1, if a.2 { " dead" } else { "" }, b.1, if b.2 { " dead" } else { "" }))
                .collect();
            println!("   units differing at the end: {}", diffs.join("; "));
        }
    }
    let mean = runs.iter().map(|r| r.tps).sum::<f64>() / runs.len() as f64;
    println!("{} runs, {bad} diverged, {} checkpoints each, mean {mean:.0} ticks/s under load {load}", runs.len(), base.hashes.len());
    if bad > 0 {
        std::process::exit(1);
    }
}
