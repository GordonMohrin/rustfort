//! [perf] Scale benchmark on the real fort: ticks/s at high population.
//!
//!   cargo run -p rf-sim --release --example scale -- [--export <dir>] [--extra 0,100,200,400]
//!       [--warm 6000] [--ticks 33600] [--seed 1] [--threads N] [--par-threshold N] [--invasions]
//!       [--keep-military]
//!
//! For every N in `--extra` the export is loaded fresh and N extra citizens are added: clones
//! of the fort's own dwarves (labors and skills kept, so the labor mix stays realistic) with new
//! ids, no squad, no inventory and no relationships, standing where another citizen stands
//! (walkable fort tiles), with need counters spread out so they do not all get thirsty in the
//! same tick. Food and drink stacks are scaled by `citizens_after / citizens_before`, so the
//! bigger fort does not starve within the measured window. The military is stood down at the
//! start (like `bench --standdown` after a fight) and invasions are off unless `--invasions`.
//!
//! After `--warm` ticks, `--ticks` ticks are measured (default one game month). Printed per N:
//! ticks/s, phase times (us per tick), and counters per 1000 ticks (path searches, A* nodes,
//! path cache hits, decisions and job/haul/need/production searches, steps). `Mcyc/1k` is the
//! main thread's CPU cycles per 1000 ticks (Windows), which other busy programs disturb much
//! less than wall-clock ticks/s; at 4.4 GHz, 88 Mcyc/1k ticks are 50 000 ticks/s.

use rf_sim::Sim;
use std::path::PathBuf;
use std::time::Instant;

/// CPU cycles of the calling thread (Windows `QueryThreadCycleTime`), robust against other
/// programs competing for the CPU; 0 elsewhere.
#[cfg(windows)]
fn thread_cycles() -> u64 {
    extern "system" {
        fn GetCurrentThread() -> isize;
        fn QueryThreadCycleTime(thread: isize, cycles: *mut u64) -> i32;
    }
    let mut c = 0u64;
    // SAFETY: plain Win32 calls with a valid out pointer; the pseudo handle needs no closing.
    unsafe {
        QueryThreadCycleTime(GetCurrentThread(), &mut c);
    }
    c
}
#[cfg(not(windows))]
fn thread_cycles() -> u64 {
    0
}

fn splitmix(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Add `n` cloned citizens and scale food and drink stacks to the new population.
pub fn add_citizens(w: &mut rf_core::World, n: usize, seed: u64) {
    if n == 0 {
        return;
    }
    let cits: Vec<usize> = (0..w.units.len()).filter(|&i| w.units[i].is_worker()).collect();
    if cits.is_empty() {
        return;
    }
    let before = cits.len();
    let mut next_id = w.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
    let mut r = seed ^ 0x5CA1E;
    for k in 0..n {
        let src = cits[k % cits.len()];
        let mut u = w.units[src].clone();
        u.id = next_id;
        next_id += 1;
        u.hist_fig_id = -1;
        u.squad_id = -1;
        u.squad_position = -1;
        u.inventory.clear();
        u.relationships.clear();
        u.wounds.clear();
        u.blood = u.blood_max;
        u.mood.clear();
        u.name = format!("{} {}", u.name, k + 2);
        u.pos = w.units[cits[(k * 7 + 3) % cits.len()]].pos;
        u.thirst_timer = (splitmix(&mut r) % 20_000) as i32;
        u.hunger_timer = (splitmix(&mut r) % 40_000) as i32;
        u.sleepiness_timer = (splitmix(&mut r) % 50_000) as i32;
        w.units.push(u);
    }
    let factor = (before + n) as f64 / before as f64;
    for it in &mut w.items {
        let food = matches!(it.kind.as_str(), "DRINK" | "PLANT" | "MEAT" | "FISH" | "FOOD" | "CHEESE" | "EGG" | "PLANT_GROWTH");
        if food && it.stack > 0 {
            it.stack = ((it.stack as f64) * factor).ceil() as i32;
        }
    }
}

struct Row {
    extra: usize,
    citizens: usize,
    alive_end: usize,
    tps: f64,
    /// Main-thread Mcycles per 1000 ticks.
    mcyc: f64,
    phases: Vec<(&'static str, f64)>,
    per_k: Vec<(&'static str, f64)>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let extras: Vec<usize> = get("--extra").unwrap_or_else(|| "0,100,200,400".into()).split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let warm: u64 = get("--warm").and_then(|s| s.parse().ok()).unwrap_or(6_000);
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(33_600);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    if let Some(t) = get("--threads").and_then(|s| s.parse::<usize>().ok()) {
        rayon::ThreadPoolBuilder::new().num_threads(t).build_global().ok();
    }
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let mut rows = Vec::new();
    for &extra in &extras {
        let (mut world, _) = rf_import::load_export(&dir).expect("load export");
        add_citizens(&mut world, extra, seed);
        let citizens = world.citizens().count();
        let t_new = Instant::now();
        let mut sim = Sim::new(world, seed);
        let new_s = t_new.elapsed().as_secs_f64();
        sim.set_invasions(flag("--invasions"));
        if flag("--no-animals") {
            sim.set_animals_enabled(false); // [animals] for before/after comparisons
        }
        if let Some(t) = get("--par-threshold").and_then(|s| s.parse().ok()) {
            sim.set_parallel_threshold(t); // results must not depend on it (determinism check)
        }
        if !flag("--keep-military") {
            let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
            for id in ids {
                sim.order_squad(id, rf_sim::SquadOrder::Cancel);
                sim.set_squad_schedule(id, "Off duty");
            }
        }
        let tw = Instant::now();
        let mut left = warm;
        while left > 0 {
            let n = left.min(1000);
            sim.run_ticks(n as u32);
            left -= n;
        }
        let warm_s = tw.elapsed().as_secs_f64();
        let _ = rf_sim::prof_report(); // [perf11] profile only the measured ticks
        let s0 = sim.stats();
        let mut phases: Vec<(&'static str, f64)> = Vec::new();
        let mut done = 0u64;
        let mut secs = 0.0;
        let c0 = thread_cycles();
        while done < ticks {
            let n = (ticks - done).min(100);
            let t = Instant::now();
            sim.run_ticks(n as u32);
            secs += t.elapsed().as_secs_f64();
            done += n;
            for (name, s) in sim.stats().phase_times {
                match phases.iter_mut().find(|(p, _)| *p == name) {
                    Some(e) => e.1 += s,
                    None => phases.push((name, s)),
                }
            }
            sim.drain_announcements();
        }
        let s1 = sim.stats();
        let mcyc = (thread_cycles() - c0) as f64 / 1e6 / (done as f64 / 1000.0);
        let k = done as f64 / 1000.0;
        let per_k = vec![
            ("paths", (s1.paths - s0.paths) as f64 / k),
            ("nodes", (s1.path_nodes - s0.path_nodes) as f64 / k),
            ("cache", (s1.path_cache_hits - s0.path_cache_hits) as f64 / k),
            ("fail", (s1.path_failures - s0.path_failures) as f64 / k),
            ("decide", (s1.decisions - s0.decisions) as f64 / k),
            ("jobS", (s1.job_searches - s0.job_searches) as f64 / k),
            ("haulS", (s1.haul_searches - s0.haul_searches) as f64 / k),
            ("needS", (s1.need_searches - s0.need_searches) as f64 / k),
            ("prodS", (s1.prod_searches - s0.prod_searches) as f64 / k),
            ("prodScan", (s1.prod_item_scans - s0.prod_item_scans) as f64 / k),
            ("steps", (s1.steps - s0.steps) as f64 / k),
            ("hauled", (s1.items_hauled - s0.items_hauled) as f64 / k),
            ("jobs", (s1.jobs_done - s0.jobs_done) as f64 / k),
            ("navRb", (s1.nav_rebuilds - s0.nav_rebuilds) as f64 / k),
        ];
        let alive_end = sim.world.citizens().count();
        let tps = done as f64 / secs.max(1e-9);
        println!(
            "+{extra:<4} citizens {citizens:>4} -> {alive_end:>4}  units {:>4}  items {:>6}  |  {tps:>8.0} ticks/s  {mcyc:>6.1} Mcyc/1k ticks  (Sim::new {new_s:.2}s, warm-up {warm} ticks {:.0} ticks/s)  hash {:016x}",
            sim.world.units.len(),
            sim.world.items.len(),
            warm as f64 / warm_s.max(1e-9),
            sim.state_hash()
        );
        let line: Vec<String> = phases.iter().map(|(n, s)| format!("{n} {:.2}", s / done as f64 * 1e6)).collect();
        println!("      us/tick: {}", line.join("  "));
        let line: Vec<String> = per_k.iter().map(|(n, v)| format!("{n} {v:.1}")).collect();
        println!("      per 1k ticks: {}", line.join("  "));
        // [perf11] With `--features prof`: inclusive cycles per function.
        for (name, calls, cyc) in rf_sim::prof_report().iter().filter(|e| e.1 > 0) {
            println!("      prof {name:<16} {calls:>9} calls  {:>9.2} kcyc/call  {:>9.2} kcyc/tick", *cyc as f64 / *calls as f64 / 1e3, *cyc as f64 / done as f64 / 1e3);
        }
        rows.push(Row { extra, citizens, alive_end, tps, mcyc, phases, per_k });
    }
    println!();
    println!("{:>6} {:>8} {:>8} {:>10} {:>12}   {}", "extra", "citizens", "alive", "ticks/s", "Mcyc/1k", "top phases (us/tick)");
    for r in &rows {
        let mut ph = r.phases.clone();
        ph.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let ticks = ticks as f64;
        let top: Vec<String> = ph.iter().take(4).map(|(n, s)| format!("{n} {:.1}", s / ticks * 1e6)).collect();
        let paths = r.per_k.iter().find(|p| p.0 == "paths").map(|p| p.1).unwrap_or(0.0);
        println!("{:>6} {:>8} {:>8} {:>10.0} {:>12.1}   {}  | paths/1k {paths:.0}", r.extra, r.citizens, r.alive_end, r.tps, r.mcyc, top.join(", "));
    }
}
