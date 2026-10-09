//! Run an imported fort headless and print a monthly stock / production report.
//!
//!   cargo run -p rf-sim --release --example fort -- [--export <dir>] [--ticks 806400] [--seed 1]
//!
//! Without `--export` the env var `RUSTFORT_EXPORT` or `export/tileshtulon` is used. Prints per
//! game month: citizens alive, drinks, prepared meals, raw food, seeds, brewable plants, empty
//! barrels/pots, open orders, queued workshop jobs, planted farm tiles and jobs done that month;
//! at the end deaths by cause and ticks/s.

use rf_sim::{AnnouncementKind, Sim};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(806_400);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let t0 = Instant::now();
    let (world, report) = rf_import::load_export(&dir).expect("load export");
    println!(
        "fort: {} ({}) from {}  |  {} units, {} citizens, {} items, {} buildings, {} work orders, {} plant raws  |  loaded in {:.2}s",
        world.meta.fort_name,
        world.meta.fort_name_native,
        dir.display(),
        world.units.len(),
        world.citizens().count(),
        world.items.len(),
        world.buildings.len(),
        world.work_orders.len(),
        world.plant_raws.len(),
        t0.elapsed().as_secs_f64()
    );
    let _ = report;
    let t1 = Instant::now();
    let mut sim = Sim::new(world, seed);
    if args.iter().any(|a| a == "--standing-orders") {
        sim.set_standing_orders(true);
    }
    if args.iter().any(|a| a == "--cook-brewables") {
        sim.set_cook_brewables(true);
    }
    if args.iter().any(|a| a == "--no-invasions") {
        sim.set_invasions(false);
    }
    // Military off duty from the start (like a player standing down after a siege).
    if args.iter().any(|a| a == "--off-duty") {
        let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
        for id in ids {
            sim.order_squad(id, rf_sim::SquadOrder::Cancel);
            sim.set_squad_schedule(id, "Off duty");
        }
    }
    let r0 = sim.production_report();
    println!(
        "sim init {:.2}s  |  {} workshops/furnaces, {} working wells, {} farm tiles ({} planted), {} work orders",
        t1.elapsed().as_secs_f64(),
        r0.workshops,
        r0.wells,
        r0.farm_tiles,
        r0.planted_tiles,
        sim.world.work_orders.len()
    );
    for n in sim.production_notes() {
        println!("note: {n}");
    }
    let citizens0 = sim.world.citizens().count();
    let mut deaths: BTreeMap<String, usize> = BTreeMap::new();
    let mut death_list: Vec<String> = Vec::new();
    let mut prev_done: BTreeMap<&'static str, u64> = BTreeMap::new();
    println!(
        "{:<22} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>7}  jobs this month",
        "date", "alive", "drinks", "meals", "rawfd", "seeds", "brewpl", "emptyc", "ordrs", "queue", "planted"
    );
    let start = Instant::now();
    let month = rf_core::TICKS_PER_MONTH as u64;
    let mut done = 0u64;
    let mut sim_secs = 0.0;
    let mut phases: Vec<(&'static str, f64)> = Vec::new();
    let standdown = args.iter().any(|a| a == "--standdown");
    let mut calm_since: Option<u64> = None;
    while done < ticks {
        let n = month.min(ticks - done);
        let t = Instant::now();
        sim.run_ticks(n as u32);
        if standdown {
            // Player emulation: one day after the last reachable hostile is gone, stand down.
            let cits: Vec<rf_core::Pos> = sim.world.citizens().map(|u| u.pos).collect();
            // Active threat = a reachable hostile within 30 tiles that is not just lurking.
            let threat = sim.hostiles().iter().any(|&id| {
                sim.unit_status(id).activity != "Lurk"
                    && sim.world.unit(id).map(|u| cits.iter().any(|&c| c.dist(u.pos) <= 30 && sim.reachable(u.pos, c))).unwrap_or(false)
            });
            // Re-alert like a player (or Gordon's guard script) when a new threat shows up.
            if threat && sim.world.squads.iter().all(|q| q.schedule.eq_ignore_ascii_case("Off duty")) {
                let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
                for id in ids {
                    sim.set_squad_schedule(id, "Ready");
                }
            }
            if threat {
                calm_since = None;
            } else {
                let since = *calm_since.get_or_insert(done);
                let on_duty = sim.world.squads.iter().any(|q| !q.schedule.eq_ignore_ascii_case("Off duty") || !q.orders.is_empty());
                if on_duty && done - since >= 1200 {
                    let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
                    for id in ids {
                        sim.order_squad(id, rf_sim::SquadOrder::Cancel);
                        sim.set_squad_schedule(id, "Off duty");
                    }
                    println!("stood down the military at tick {done}");
                }
            }
        }
        sim_secs += t.elapsed().as_secs_f64();
        done += n;
        for (k, (name, v)) in sim.stats().phase_times.iter().enumerate() {
            if phases.len() <= k {
                phases.push((*name, 0.0));
            }
            phases[k].1 += v;
        }
        for a in sim.drain_announcements() {
            if a.kind == AnnouncementKind::Death {
                let cause = a.text.split(" has ").nth(1).unwrap_or(&a.text).trim_end_matches('.').to_string();
                *deaths.entry(cause).or_default() += 1;
                death_list.push(format!("tick {}: {}", a.tick, a.text));
            }
        }
        let r = sim.production_report();
        let mut jobs = String::new();
        for (label, n) in &r.jobs_done {
            let before = prev_done.get(label).copied().unwrap_or(0);
            if *n > before {
                jobs.push_str(&format!("{label} {}, ", n - before));
            }
            prev_done.insert(label, *n);
        }
        let cal = sim.world.calendar;
        println!(
            "{:<22} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>4}/{:<3} {}",
            format!("{} {} {}", cal.day(), cal.month_name(), cal.year),
            sim.world.citizens().count(),
            r.drinks,
            r.meals,
            r.raw_food,
            r.seeds,
            r.brewable_plants,
            r.empty_containers,
            r.open_orders,
            r.queued_jobs,
            r.planted_tiles,
            r.farm_tiles,
            jobs.trim_end_matches(", ")
        );
    }
    let wall = start.elapsed().as_secs_f64();
    let r = sim.production_report();
    println!(
        "ran {done} ticks in {wall:.2}s (sim {sim_secs:.2}s) = {:.0} ticks/s",
        done as f64 / sim_secs.max(1e-9)
    );
    let st = sim.stats();
    println!(
        "phases (us/tick): {}  |  production searches {} ({:.2}s), reagent scans {}, paths {}",
        phases.iter().map(|(n, v)| format!("{n} {:.2}", v / done as f64 * 1e6)).collect::<Vec<_>>().join(", "),
        st.prod_searches,
        st.prod_search_secs,
        st.prod_item_scans,
        st.paths
    );
    let tk = done as f64;
    println!(
        "per tick: decisions {:.2}, steps {:.2}, job searches {:.3}, haul searches {:.3}, need searches {:.3}, items hauled {}",
        st.decisions as f64 / tk,
        st.steps as f64 / tk,
        st.job_searches as f64 / tk,
        st.haul_searches as f64 / tk,
        st.need_searches as f64 / tk,
        st.items_hauled
    );
    println!("citizens: {citizens0} at start, {} alive at end", sim.world.citizens().count());
    if deaths.is_empty() {
        println!("deaths: none");
    } else {
        println!("deaths by cause: {deaths:?}");
        for d in death_list.iter().take(10) {
            println!("  {d}");
        }
    }
    println!("jobs done in total: {:?}", r.jobs_done);
    if !r.unsupported.is_empty() {
        println!("work orders / workshop jobs not simulated yet: {:?}", r.unsupported.iter().map(|u| &u.0).collect::<Vec<_>>());
    }
    println!("state hash: {:016x}", sim.state_hash());
}
