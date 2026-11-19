//! [diplo13] Run an imported fort headless and report its diplomacy (round 13).
//!
//!   cargo run -p rf-sim --release --example diplomacy -- [--export <dir>] [--ticks 806400]
//!       [--seed 1] [--answer accept|refuse]
//!
//! Prints the meetings, demands (with answers), the land title, the diplomacy report and the
//! attitude/relation of every trading or warring civilization. `--answer` answers every open
//! demand at once (default: leave them to their deadline).

use rf_sim::Sim;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(806_400);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let answer = get("--answer").map(|a| a == "accept");
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (mut world, _) = rf_import::load_export(&dir).expect("load export");
    if world.settings.population_cap == 0 {
        if let Some(s) = std::fs::read_to_string(dir.join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()) {
            world.settings = s;
        }
    }
    let mut sim = Sim::new(world, seed);
    let start = Instant::now();
    let mut done = 0u64;
    while done < ticks {
        let n = 1000u64.min(ticks - done);
        sim.run_ticks(n as u32);
        done += n;
        if let Some(acc) = answer {
            for d in sim.diplomacy_demands().into_iter().filter(|d| d.answer.is_none()) {
                sim.answer_demand(d.id, acc);
            }
        }
    }
    println!("ran {done} ticks in {:.1}s", start.elapsed().as_secs_f64());
    println!("caravans:");
    for c in sim.trade_report().caravans {
        println!("  t{:<8} {:<8} {} merchants {} trades {} {}", c.arrived, c.race, c.civ, c.merchants, c.trades, c.note);
    }
    let (felled, quotas) = sim.tree_quota();
    println!("trees felled this year: {felled}, quotas {quotas:?}, tribute owed {:?}", sim.tribute_owed());
    println!("land title: {:?}", sim.land_title());
    println!("report: {:?}", sim.diplomacy_report());
    println!("meetings:");
    for m in sim.diplomacy_meetings() {
        println!("  #{:<3} t{:<8} {:?} {:<7} {}", m.id, m.tick, m.kind, m.state, m.text);
    }
    println!("demands:");
    for d in sim.diplomacy_demands() {
        println!("  #{:<3} {:?} amount {} answer {:?}: {}", d.id, d.kind, d.amount, d.answer, d.text);
    }
    println!("civilizations (trading or at war):");
    for e in sim.diplomacy_entities().into_iter().filter(|e| e.trades || e.relation == "War" || e.relation == "Own") {
        println!("  {:>6} {:<8} {:<6} att {:>4} trades {} war since {:?}  {}", e.id, e.race, e.relation, e.attitude, e.trades, e.at_war_since, e.name);
    }
}
