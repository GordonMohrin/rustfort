//! [perf9] Determinism under load: the same scenario with many rayon thread counts and
//! parallel-path thresholds, while busy threads compete for every core. Each run builds its
//! sim inside its own pool (so also on another OS thread, with other `std` hash keys), and the
//! state hash is compared at checkpoints, not only at the end. The real-fort version is
//! `examples/determinism.rs`.

use rf_core::{Pos, World};
use rf_sim::{gen, DesignationKind, Sim};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn hogs(n: usize) -> (Arc<AtomicBool>, Vec<std::thread::JoinHandle<()>>) {
    let stop = Arc::new(AtomicBool::new(false));
    let hs = (0..n)
        .map(|k| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut x = k as u64 + 1;
                while !stop.load(Ordering::Relaxed) {
                    for _ in 0..5_000 {
                        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    }
                    std::hint::black_box(x);
                }
            })
        })
        .collect();
    (stop, hs)
}

fn checkpoints(make: &(dyn Fn() -> Sim + Sync), threads: usize, par: usize, ticks: u32, every: u32) -> Vec<u64> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
    pool.install(|| {
        let mut sim = make();
        sim.set_parallel_threshold(par);
        let mut out = Vec::new();
        let mut done = 0;
        while done < ticks {
            sim.run_ticks(every);
            done += every;
            out.push(sim.state_hash());
            sim.drain_announcements();
        }
        out
    })
}

fn stress(make: &(dyn Fn() -> Sim + Sync), ticks: u32, every: u32) {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let (stop, hs) = hogs(cores);
    let base = checkpoints(make, 1, 1, ticks, every);
    let mut bad = Vec::new();
    for &(t, p) in &[(2, 1), (3, 2), (4, 1), (cores, 1), (cores, 8), (cores * 2, 1), (5, 1000)] {
        let h = checkpoints(make, t, p, ticks, every);
        if let Some(k) = (0..base.len()).find(|&k| base[k] != h[k]) {
            bad.push(format!("threads {t} par {p}: first divergence at tick {}", (k as u32 + 1) * every));
        }
    }
    stop.store(true, Ordering::Relaxed);
    for h in hs {
        let _ = h.join();
    }
    assert!(bad.is_empty(), "nondeterministic: {bad:?}");
}

#[test]
fn bench_world_is_deterministic_under_load() {
    // Many units with many simultaneous path searches (large parallel batches).
    stress(&|| Sim::new(gen::bench_world(64, 64, 30, 60, 3), 3), 4_000, 250);
}

#[test]
fn digging_fort_is_deterministic_under_load() {
    // Digging, chopping and hauling: the landmark tables are repaired while searches run.
    let make = || {
        let (dx, dy, dz) = (48u16, 48u16, 20u16);
        let surface_z = (dz as i16 * 2 / 3).max(2);
        let (cx, cy, hz) = (dx as i16 / 2, dy as i16 / 2, (surface_z - 4).max(0));
        let mut sim = Sim::new(World::synthetic(dx, dy, dz), 11);
        sim.designate(Pos::new(cx - 16, cy - 4, hz), Pos::new(cx - 7, cy - 1, hz), DesignationKind::Mine);
        sim.create_stockpile(Pos::new(cx + 1, cy + 1, hz), Pos::new(cx + 6, cy + 4, hz), &["stone", "wood"]);
        sim.designate(Pos::new(0, 0, hz + 5), Pos::new(dx as i16 - 1, dy as i16 - 1, hz + 5), DesignationKind::ChopTrees);
        sim
    };
    stress(&make, 12_000, 1_000);
}
