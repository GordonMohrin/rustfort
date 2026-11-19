//! [prefs15] A/B of the preference rules on a real fort: the same world runs with preferences on
//! and off (`Sim::set_prefs_enabled`; item values are v50 in both), with the bench's player
//! emulation (squads stand down a day after hostiles leave). Prints citizens, created wealth,
//! sieges, mean stress, made-item quality and the preference counters.
//!
//!   cargo run -p rf-sim --release --example prefs15_ab -- [--export <dir>] [--ticks 806400] [--seed 1]

use rf_sim::Sim;
use std::path::PathBuf;

fn standdown(sim: &mut Sim, calm: &mut Option<u64>, done: u64) {
    let cit: Vec<rf_core::Pos> = sim.world.citizens().map(|u| u.pos).collect();
    let threat = sim.hostiles().iter().any(|&id| {
        sim.unit_status(id).activity != "Lurk" && sim.world.unit(id).map(|u| cit.iter().any(|&c| c.dist(u.pos) <= 30 && sim.reachable(u.pos, c))).unwrap_or(false)
    });
    if threat && sim.world.squads.iter().all(|q| q.schedule.eq_ignore_ascii_case("Off duty")) {
        let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
        for id in ids {
            sim.set_squad_schedule(id, "Ready");
        }
    }
    if threat {
        *calm = None;
    } else {
        let since = *calm.get_or_insert(done);
        let on_duty = sim.world.squads.iter().any(|q| !q.schedule.eq_ignore_ascii_case("Off duty") || !q.orders.is_empty());
        if on_duty && done - since >= rf_core::TICKS_PER_DAY as u64 {
            let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
            for id in ids {
                sim.order_squad(id, rf_sim::SquadOrder::Cancel);
                sim.set_squad_schedule(id, "Off duty");
            }
        }
    }
}

fn report(name: &str, sim: &Sim) {
    let cits: Vec<&rf_core::Unit> = sim.world.citizens().collect();
    let stress = cits.iter().map(|u| u.stress as i64).sum::<i64>() / cits.len().max(1) as i64;
    let made = sim.skills_report().made;
    let n: u64 = made.iter().sum();
    let mean = made.iter().enumerate().map(|(q, &k)| q as u64 * k).sum::<u64>() as f64 / n.max(1) as f64;
    println!(
        "{name}: citizens {} | sieges {} | created wealth {} | mean stress {} | made {:?} mean quality {:.2}",
        cits.len(),
        sim.world.fort_history.sieges.len(),
        sim.created_wealth(),
        stress,
        made,
        mean
    );
    println!("{name}: {:?}", sim.prefs_report());
}

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
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let mut on = Sim::new(world.clone(), seed);
    let mut off = Sim::new(world, seed);
    off.set_prefs_enabled(false);
    let (mut c_on, mut c_off) = (None, None);
    let mut done = 0u64;
    while done < ticks {
        let n = 100u64.min(ticks - done);
        on.run_ticks(n as u32);
        off.run_ticks(n as u32);
        done += n;
        standdown(&mut on, &mut c_on, done);
        standdown(&mut off, &mut c_off, done);
        on.drain_announcements();
        off.drain_announcements();
    }
    report("prefs on ", &on);
    report("prefs off", &off);
}
