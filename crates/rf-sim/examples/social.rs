//! Run an imported fort headless and print the social report: stress distribution per month,
//! the most frequent thoughts, breakdowns, dining/prayer counts and a few sample thoughts.
//!
//!   cargo run -p rf-sim --release --example social -- [--export <dir>] [--ticks 806400] [--seed 1]
//!
//! Without `--export` the env var `RUSTFORT_EXPORT` or `export/tileshtulon` is used.
//! `--no-dining` removes all tables and `--no-beds` all beds before the run (comparison runs);
//! `--off` switches the social simulation off (speed A/B).

use rf_sim::{AnnouncementKind, Sim, StressLevel};
use std::path::PathBuf;
use std::time::Instant;

const LEVELS: [StressLevel; 7] = [
    StressLevel::Ecstatic,
    StressLevel::Happy,
    StressLevel::Content,
    StressLevel::Fine,
    StressLevel::Stressed,
    StressLevel::VeryStressed,
    StressLevel::Breaking,
];

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
    let (mut world, _) = rf_import::load_export(&dir).expect("load export");
    if args.iter().any(|a| a == "--no-dining") {
        world.buildings.retain(|b| b.kind != "Table");
    }
    if args.iter().any(|a| a == "--no-beds") {
        world.buildings.retain(|b| b.kind != "Bed");
    }
    let with_needs = world.citizens().filter(|u| !u.needs.is_empty()).count();
    let with_emotions = world.citizens().filter(|u| !u.emotions.is_empty()).count();
    let with_rel = world.citizens().filter(|u| !u.relationships.is_empty()).count();
    println!(
        "fort: {} from {}  |  {} citizens ({} with exported needs, {} with emotions, {} with relationships)",
        world.meta.fort_name,
        dir.display(),
        world.citizens().count(),
        with_needs,
        with_emotions,
        with_rel
    );
    let mut sim = Sim::new(world, seed);
    if args.iter().any(|a| a == "--off") {
        sim.set_social_enabled(false);
    }
    let r0 = sim.social_report();
    println!("dining seats: {}  |  owned bedrooms: {}", r0.seats, r0.bedrooms);
    let t0 = Instant::now();
    let mut done = 0u64;
    let mut breakdowns: Vec<String> = Vec::new();
    let mut deaths = 0;
    while done < ticks {
        let n = 1200u64.min(ticks - done);
        sim.run_ticks(n as u32);
        done += n;
        for a in sim.drain_announcements() {
            if a.kind == AnnouncementKind::Death {
                deaths += 1;
            }
            let t = &a.text;
            if t.contains("tantrum") || t.contains("depression") || t.contains("obliviously") || t.contains("berserk") || t.contains("melancholy") || t.contains("raving") {
                breakdowns.push(format!("{} {}: {}", sim.world.calendar.month_name(), sim.world.calendar.year, t));
            }
        }
    }
    let el = t0.elapsed().as_secs_f64();
    let rep = sim.social_report();
    println!("ran {done} ticks in {el:.2}s = {:.0} ticks/s, {} deaths", done as f64 / el, deaths);
    println!();
    print!("{:<18}", "month");
    for l in LEVELS {
        print!("{:>10}", l.label().split(' ').next().unwrap_or(""));
    }
    println!("{:>10}{:>10}{:>8}", "mean ST", "mean LT", "focus");
    for m in &rep.months {
        let c = rf_core::Calendar { year: m.year, year_tick: m.month * rf_core::TICKS_PER_MONTH };
        print!("{:<18}", format!("{} {}", c.month_name(), m.year));
        for v in m.counts {
            print!("{v:>10}");
        }
        println!("{:>10}{:>10}{:>7}%", m.mean_stress, m.mean_longterm, m.mean_focus);
    }
    println!();
    println!("top thoughts:");
    for (t, n) in rep.thoughts.iter().take(15) {
        println!("  {n:>7}  {t}");
    }
    println!();
    println!(
        "breakdowns: {} tantrums, {} depressions, {} oblivious, {} insane",
        rep.tantrums, rep.depressions, rep.oblivious, rep.insane
    );
    for b in breakdowns.iter().take(20) {
        println!("  {b}");
    }
    println!(
        "meals at a table: {}, without: {}  |  prayers: {}  |  conversations: {}",
        rep.meals_at_table, rep.meals_without_table, rep.prayers, rep.conversations
    );
    // A few citizens' recent thoughts.
    let ids: Vec<i32> = sim.world.citizens().map(|u| u.id).take(3).collect();
    for id in ids {
        let u = sim.world.unit(id).unwrap();
        println!();
        println!(
            "{} ({}): stress {} ({}), long-term {}, focus {}%",
            u.name,
            u.profession,
            u.stress,
            sim.stress_level(id).label(),
            sim.longterm_stress(id),
            sim.focus_percent(id)
        );
        for t in sim.recent_thoughts(id, 6) {
            println!("    {t}");
        }
    }
    println!("state hash: {:016x}", sim.state_hash());
}
