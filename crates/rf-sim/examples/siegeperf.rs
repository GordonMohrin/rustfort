//! [perf10] Speed during a big siege on the real fort.
//!
//!   cargo run -p rf-sim --release --example siegeperf -- [--export <dir>] [--n 0] [--ticks 20000]
//!       [--warm 600] [--seed 1] [--window 2000]
//!
//! Loads the export, runs `--warm` ticks, forces a goblin siege (`--n 0`: the size the fort's
//! invasion tier gives, `force_siege`; otherwise `spawn_siege("GOBLIN", n)`, both charge at
//! once), then measures `--ticks` ticks (or until the invaders are gone) in windows: ticks/s,
//! main-thread Mcycles per 1000 ticks (Windows; robust against other busy programs), hostiles
//! left, path searches and A* nodes per 1000 ticks, and the phase times. The last line has the
//! totals and the state hash (equal hashes = identical simulation).

use rf_sim::Sim;
use std::path::PathBuf;
use std::time::Instant;

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let n: usize = get("--n").and_then(|s| s.parse().ok()).unwrap_or(0);
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let warm: u64 = get("--warm").and_then(|s| s.parse().ok()).unwrap_or(600);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let window: u64 = get("--window").and_then(|s| s.parse().ok()).unwrap_or(2000).max(100);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let mut sim = Sim::new(world, seed);
    sim.set_invasions(false);
    sim.run_ticks(warm as u32);
    let arrived = if n == 0 { sim.force_siege() } else { sim.spawn_siege("GOBLIN", n) };
    let citizens0 = sim.world.citizens().count();
    println!("siege: {arrived} goblins arrived, {citizens0} citizens, {} units", sim.world.units.len());
    let (mut done, mut secs, mut cyc) = (0u64, 0.0f64, 0u64);
    let mut phases: Vec<(&'static str, f64)> = Vec::new();
    let s_start = sim.stats();
    while done < ticks {
        let s0 = sim.stats();
        let c0 = thread_cycles();
        let t = Instant::now();
        let mut k = 0;
        let mut wp: Vec<(&'static str, f64)> = Vec::new();
        while k < window && done < ticks {
            let m = (window - k).min(100).min(ticks - done);
            sim.run_ticks(m as u32);
            for (name, s) in sim.stats().phase_times {
                match wp.iter_mut().find(|(p, _)| *p == name) {
                    Some(e) => e.1 += s,
                    None => wp.push((name, s)),
                }
            }
            sim.drain_announcements();
            k += m;
            done += m;
        }
        let el = t.elapsed().as_secs_f64();
        let dc = thread_cycles() - c0;
        secs += el;
        cyc += dc;
        for (name, s) in &wp {
            match phases.iter_mut().find(|(p, _)| p == name) {
                Some(e) => e.1 += s,
                None => phases.push((name, *s)),
            }
        }
        let s1 = sim.stats();
        let kk = k as f64 / 1000.0;
        let top: Vec<String> = wp.iter().filter(|e| e.1 / k as f64 * 1e6 >= 1.0).map(|(n, s)| format!("{n} {:.1}", s / k as f64 * 1e6)).collect();
        println!(
            "  t+{done:>6}  {:>7.0} ticks/s  {:>6.1} Mcyc/1k  hostiles {:>3}  citizens {:>3}  paths/1k {:>6.1}  nodes/1k {:>8.0}  us/tick: {}",
            k as f64 / el.max(1e-9),
            dc as f64 / 1e6 / kk,
            sim.hostiles().len(),
            sim.world.citizens().count(),
            (s1.paths - s0.paths) as f64 / kk,
            (s1.path_nodes - s0.path_nodes) as f64 / kk,
            top.join("  ")
        );
        if sim.hostiles().is_empty() {
            break;
        }
    }
    let s1 = sim.stats();
    let kk = done as f64 / 1000.0;
    let line: Vec<String> = phases.iter().map(|(n, s)| format!("{n} {:.2}", s / done as f64 * 1e6)).collect();
    println!(
        "total {done} ticks: {:.0} ticks/s  {:.1} Mcyc/1k  paths/1k {:.1}  nodes/1k {:.0}  citizens {citizens0} -> {}  hash {:016x}",
        done as f64 / secs.max(1e-9),
        cyc as f64 / 1e6 / kk,
        (s1.paths - s_start.paths) as f64 / kk,
        (s1.path_nodes - s_start.path_nodes) as f64 / kk,
        sim.world.citizens().count(),
        sim.state_hash()
    );
    println!("  us/tick: {}", line.join("  "));
}
