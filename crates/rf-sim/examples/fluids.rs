//! [fluids] Liquid simulation demo and benchmark.
//!
//!   cargo run -p rf-sim --release --example fluids                 # flood the synthetic fort
//!   cargo run -p rf-sim --release --example fluids -- --magma      # ... with magma
//!   cargo run -p rf-sim --release --example fluids -- --export <dir> [--ticks 806400] [--compare]
//!
//! Synthetic mode: a 96x96x24 synthetic fort; an infinite source on the surface next to the
//! stair shaft pours water (or magma) down into the hall level. Prints a line per game day:
//! liquid tiles, flowing cells, volume, pressure/levelling moves, deaths, ticks/s.
//!
//! Export mode (the real fort): runs the imported fort with invasions off and prints the
//! flowing cells after one day and the water/magma volume drift over the run. `--compare`
//! runs the same ticks again with the fluid simulation off and prints both speeds.

use rf_core::{Pos, World};
use rf_sim::{AnnouncementKind, Sim};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let has = |name: &str| args.iter().any(|a| a == name);
    if has("--export") || std::env::var_os("RUSTFORT_EXPORT").is_some() && has("--real") {
        real_fort(get("--export").map(PathBuf::from), get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(806_400), has("--compare"));
    } else {
        synthetic(get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(12_000), has("--magma"));
    }
}

fn synthetic(ticks: u64, magma: bool) {
    let w = World::synthetic(96, 96, 24);
    let surface = (24i16 * 2 / 3).max(2);
    let (cx, cy) = (48i16, 48i16);
    let mut sim = Sim::new(w, 1);
    sim.set_invasions(false);
    // Baseline speed without any flowing liquid.
    let t0 = Instant::now();
    sim.run_ticks(2_000);
    let base = 2_000.0 / t0.elapsed().as_secs_f64();
    println!("synthetic fort 96x96x24, hall at z{} | baseline {:.0} ticks/s (no liquid)", surface - 4, base);
    let src = Pos::new(cx, cy - 1, surface);
    sim.add_liquid_source(src, magma);
    println!("{} source at {src:?} next to the stair shaft", if magma { "magma" } else { "water" });
    println!("{:>7} {:>7} {:>7} {:>8} {:>8} {:>8} {:>6} {:>9}", "tick", "liquid", "flowing", "volume", "pressure", "level", "dead", "ticks/s");
    let t1 = Instant::now();
    let mut last = Instant::now();
    let mut done = 0;
    while done < ticks {
        let step = 1_200.min(ticks - done);
        sim.run_ticks(step as u32);
        done += step;
        let r = sim.fluid_report();
        let dead = sim.world.units.iter().filter(|u| u.flags.dead).count();
        println!(
            "{:>7} {:>7} {:>7} {:>8} {:>8} {:>8} {:>6} {:>9.0}",
            sim.ticks(),
            r.liquid_tiles,
            r.flowing,
            r.water_volume + r.magma_volume,
            r.pressure_moves,
            r.level_moves,
            dead,
            step as f64 / last.elapsed().as_secs_f64()
        );
        last = Instant::now();
    }
    let r = sim.fluid_report();
    println!(
        "flood: {:.0} ticks/s over {ticks} ticks | cell updates {} | searches {} ({} tiles looked at) | evaporated {} | obsidian {} | drowned {} | burned {} | items melted {} | mud {}",
        ticks as f64 / t1.elapsed().as_secs_f64(),
        r.updates,
        r.searches,
        r.search_nodes,
        r.evaporated,
        r.obsidian,
        r.drowned,
        r.burned,
        r.items_melted,
        r.mud_tiles
    );
    for a in sim.drain_announcements().iter().filter(|a| a.kind == AnnouncementKind::Death) {
        println!("  {}", a.text);
    }
}

fn real_fort(explicit: Option<PathBuf>, ticks: u64, compare: bool) {
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let run = |fluids: bool| -> (f64, Option<(rf_sim::FluidReport, rf_sim::FluidReport, rf_sim::FluidReport)>, usize) {
        let (world, _) = rf_import::load_export(&dir).expect("load export");
        let mut sim = Sim::new(world, 1);
        sim.set_invasions(false);
        sim.set_fluids(fluids);
        let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
        for id in ids {
            sim.order_squad(id, rf_sim::SquadOrder::Cancel);
            sim.set_squad_schedule(id, "Off duty");
        }
        let r0 = sim.fluid_report();
        let t0 = Instant::now();
        sim.run_ticks(1_200);
        let r1 = sim.fluid_report();
        let mut done = 1_200;
        while done < ticks {
            let step = 100_800.min(ticks - done);
            sim.run_ticks(step as u32);
            done += step;
        }
        let tps = ticks as f64 / t0.elapsed().as_secs_f64();
        let alive = sim.world.citizens().filter(|u| !u.flags.dead).count();
        (tps, fluids.then(|| (r0, r1, sim.fluid_report())), alive)
    };
    let (tps, rep, alive) = run(true);
    let (r0, r1, r2) = rep.unwrap();
    println!("real fort from {} | {ticks} ticks, invasions off, military off duty", dir.display());
    println!(
        "start: {} liquid tiles ({} magma), water {} /7, magma {} /7, leaky aquifer tiles {}",
        r0.liquid_tiles, r0.magma_tiles, r0.water_volume, r0.magma_volume, r1.leaky_aquifer_tiles
    );
    println!("after 1 day: flowing cells {}, cell updates {}, pressure moves {}, level moves {}", r1.flowing, r1.updates, r1.pressure_moves, r1.level_moves);
    println!(
        "after {ticks} ticks: liquid tiles {} | water {} /7 (drift {:+}) | magma {} /7 (drift {:+}) | flowing {} | updates {} | aquifer +{} | evaporated {} | obsidian {} | drowned {} | burned {}",
        r2.liquid_tiles,
        r2.water_volume,
        r2.water_volume - r0.water_volume,
        r2.magma_volume,
        r2.magma_volume - r0.magma_volume,
        r2.flowing,
        r2.updates,
        r2.aquifer_added,
        r2.evaporated,
        r2.obsidian,
        r2.drowned,
        r2.burned
    );
    println!(
        "fluids on: {tps:.0} ticks/s, {alive} citizens alive | time in the fluid tick {:.3}s = {:.2} % of the run",
        r2.time_s,
        r2.time_s * tps / ticks as f64 * 100.0
    );
    if compare {
        let (tps_off, _, alive_off) = run(false);
        println!("fluids off: {tps_off:.0} ticks/s, {alive_off} citizens alive | fluids cost {:+.1} %", (tps_off / tps - 1.0) * 100.0);
    }
}
