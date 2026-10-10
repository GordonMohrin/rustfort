//! [chaos10] Crash hunting: random but valid player actions on the real fort (or synthetic
//! worlds) over many game years and many seeds, with invariant checks after every step and
//! save/load round trips. Every panic is reported with the seed and the last actions.
//!
//!   cargo run -p rf-sim --release --example chaos -- [--export <dir> | --synthetic | --bench]
//!       [--seeds 1-24] [--years 2] [--step 1200] [--save-every 120] [--jobs 4] [--peaceful]
//!
//! Without `--export` the env var `RUSTFORT_EXPORT` or `export/tileshtulon` is used. Each seed
//! runs on its own thread (`--jobs` at a time); the sim seed and the chaos seed are both the
//! seed number, so `--seeds 7` reproduces seed 7 exactly.

#[path = "chaos_support/mod.rs"]
mod chaos_support;

use chaos_support::Chaos;
use rf_core::World;
use rf_sim::{gen, Sim};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn parse_seeds(s: &str) -> Vec<u64> {
    let mut v = Vec::new();
    for part in s.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            let (a, b): (u64, u64) = (a.trim().parse().unwrap(), b.trim().parse().unwrap());
            v.extend(a..=b);
        } else if !part.trim().is_empty() {
            v.push(part.trim().parse().unwrap());
        }
    }
    v
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let has = |name: &str| args.iter().any(|a| a == name);
    let seeds = parse_seeds(&get("--seeds").unwrap_or_else(|| "1-8".into()));
    let years: f64 = get("--years").and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let step: u32 = get("--step").and_then(|s| s.parse().ok()).unwrap_or(1200);
    let save_every: u64 = get("--save-every").and_then(|s| s.parse().ok()).unwrap_or(120);
    let jobs: usize = get("--jobs").and_then(|s| s.parse().ok()).unwrap_or(4).max(1);
    let peaceful = has("--peaceful");
    let no_invasions = has("--no-invasions");
    let slow_tps: f64 = get("--slow").and_then(|s| s.parse().ok()).unwrap_or(5000.0);
    let synthetic = has("--synthetic");
    let bench = has("--bench");
    let ticks = (years * rf_core::TICKS_PER_YEAR as f64) as u64;
    let steps = ticks / step as u64;

    let world: Arc<World> = Arc::new(if synthetic {
        World::synthetic(64, 64, 24)
    } else if bench {
        gen::bench_world(64, 64, 30, 60, 3)
    } else {
        let explicit = get("--export").map(PathBuf::from);
        let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
            eprintln!("no export found (use --export <dir>, RUSTFORT_EXPORT, --synthetic or --bench)");
            std::process::exit(1);
        };
        let (w, _) = rf_import::load_export(&dir).expect("load export");
        println!("fort {} from {}: {} units, {} items, {} buildings", w.meta.fort_name, dir.display(), w.units.len(), w.items.len(), w.buildings.len());
        w
    });
    println!(
        "chaos: {} seeds, {years} years ({steps} steps of {step} ticks), save/load every {save_every} steps, {jobs} at a time{}",
        seeds.len(),
        if peaceful { ", peaceful" } else { "" }
    );

    let next = Arc::new(AtomicUsize::new(0));
    let results: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let seeds = Arc::new(seeds);
    let t0 = Instant::now();
    let mut hs = Vec::new();
    for _ in 0..jobs.min(seeds.len()) {
        let (next, results, seeds, world) = (next.clone(), results.clone(), seeds.clone(), world.clone());
        hs.push(std::thread::spawn(move || loop {
            let k = next.fetch_add(1, Ordering::SeqCst);
            let Some(&seed) = seeds.get(k) else { break };
            let w = (*world).clone();
            let t = Instant::now();
            // Single-threaded rayon per seed, so the seeds use the cores instead.
            let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
            let mut c = Chaos::new(seed);
            c.violent = !peaceful;
            c.slow_tps = slow_tps;
            c.no_invasions = no_invasions;
            // `c` stays readable after a panic (its action log is the repro).
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pool.install(|| {
                    let sim = c.run(Sim::new(w, seed), steps, step, save_every);
                    (c.out.clone(), sim.world.citizens().count(), sim.stats().dead_units)
                })
            }));
            let secs = t.elapsed().as_secs_f64();
            let line = match r {
                Ok((out, cits, dead)) => {
                    let mut s = format!(
                        "seed {seed:>3}: OK    {} steps, {} actions, {} saves, citizens {} -> {cits}, dead units {dead}, {:.0} ticks/s",
                        out.steps,
                        out.actions,
                        out.saves,
                        out.citizens_start,
                        out.ticks as f64 / out.sim_secs.max(1e-9)
                    );
                    let _ = secs;
                    let viol: Vec<String> = ["force_siege", "force_raid", "spawn_siege", "make_hostile", "kill_unit", "spawn_forgotten_beast", "guard_ready"]
                        .iter()
                        .filter_map(|k| out.per_action.get(k).map(|n| format!("{k} {n}")))
                        .collect();
                    if !viol.is_empty() {
                        s.push_str(&format!("\n           violent actions: {}", viol.join(", ")));
                    }
                    if !out.citizen_deaths.is_empty() {
                        let mut d: Vec<(&String, &u64)> = out.citizen_deaths.iter().collect();
                        d.sort_by(|a, b| b.1.cmp(a.1));
                        let top: Vec<String> = d.iter().take(5).map(|(c, n)| format!("{n} {c}")).collect();
                        s.push_str(&format!("
           citizen deaths: {}", top.join(", ")));
                    }
                    for v in out.slow.iter().take(8) {
                        s.push_str(&format!("
           slow: {v}"));
                    }
                    if !out.violations.is_empty() {
                        s.push_str(&format!("\n           {} NEW INVARIANT VIOLATIONS:", out.violations.len()));
                        for v in out.violations.iter().take(12) {
                            s.push_str(&format!("\n             {v}"));
                        }
                    }
                    s
                }
                Err(e) => {
                    let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                    let mut s = format!("seed {seed:>3}: PANIC after {secs:.0}s: {msg}\n           last actions:");
                    for l in c.log.iter().rev().take(15).rev() {
                        s.push_str(&format!("\n             {l}"));
                    }
                    s
                }
            };
            println!("{line}");
            results.lock().unwrap().push((seed, line));
        }));
    }
    for h in hs {
        let _ = h.join();
    }
    let res = results.lock().unwrap();
    let panics = res.iter().filter(|(_, l)| l.contains("PANIC")).count();
    let viol = res.iter().filter(|(_, l)| l.contains("VIOLATIONS")).count();
    println!("done in {:.0}s: {} seeds, {panics} panicked, {viol} with new invariant violations", t0.elapsed().as_secs_f64(), res.len());
    if panics > 0 {
        std::process::exit(2);
    }
}
