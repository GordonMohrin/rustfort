//! Round 7 evidence on the real fort: wealth tier / siege size, cave-in checks during a run,
//! and work done under a civilian alert.
//!
//!   cargo run -p rf-sim --release --example r7_check -- [--export <dir>] [--ticks 806400]

use rf_sim::Sim;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let ticks: u32 = get("--ticks").and_then(|t| t.parse().ok()).unwrap_or(806_400);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found");
        return;
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let burrows: Vec<(i32, String, usize)> = world.burrows.iter().map(|b| (b.id, b.name.clone(), b.tiles.len())).collect();
    let mut sim = Sim::new(world.clone(), 1);
    let pop = sim.world.citizens().count();
    println!(
        "start: {pop} citizens, created wealth {}, exported {}, tier {}, siege size {:?} (old rule: {})",
        sim.created_wealth(),
        sim.exported_wealth(),
        rf_sim::invasion::invasion_tier(pop, sim.created_wealth(), sim.exported_wealth()),
        sim.siege_size(),
        (10 + pop / 8).min(60)
    );
    let t = std::time::Instant::now();
    sim.run_ticks(ticks);
    let r = sim.cavein_report();
    println!(
        "after {ticks} ticks ({:.1}s): {} citizens, cave-in checks {} ({} nodes, {:.0}/check), collapses {}, tiles fallen {}, killed {}, siege size now {:?}",
        t.elapsed().as_secs_f64(),
        sim.world.citizens().count(),
        r.checks,
        r.nodes,
        r.nodes as f64 / r.checks.max(1) as f64,
        r.collapses,
        r.tiles_fallen,
        r.killed,
        sim.siege_size()
    );
    // Civilian alert on the largest burrow for one month.
    if let Some(b) = burrows.iter().max_by_key(|b| b.2) {
        let mut sim = Sim::new(world, 1);
        sim.set_invasions(false);
        sim.set_civilian_alert(Some(b.0));
        let s0 = sim.stats();
        sim.run_ticks(33_600);
        let s1 = sim.stats();
        println!(
            "alert on burrow {} '{}' ({} tiles), 1 month: jobs done {}, items hauled {}",
            b.0,
            b.1,
            b.2,
            s1.jobs_done - s0.jobs_done,
            s1.items_hauled - s0.items_hauled
        );
    } else {
        println!("the fort has no burrows");
    }
}
