//! Lurking hostiles on the real fort (round 6 combat audit): lists every hostile at the start
//! with its distance to the nearest soldier on duty and whether they share a walk component,
//! then runs `--ticks` (default 20 000) and reports which hostiles were engaged or killed.
//!
//!   cargo run -p rf-sim --release --example lurkers -- [--export <dir>] [--ticks 20000]

use rf_sim::Sim;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let mut sim = Sim::new(world, 1);
    sim.set_invasions(false);
    let soldiers: Vec<i32> = sim.world.squads.iter().flat_map(|q| q.members.iter().copied()).filter(|&m| m >= 0).collect();
    let report = |sim: &Sim, label: &str| {
        let hs = sim.hostiles();
        println!("{label}: {} hostiles", hs.len());
        for id in hs {
            let u = sim.world.units.iter().find(|u| u.id == id).unwrap();
            let mut best = (i32::MAX, false);
            for &m in &soldiers {
                let Some(d) = sim.world.units.iter().find(|x| x.id == m && !x.flags.dead) else { continue };
                let dist = (d.pos.x as i32 - u.pos.x as i32).abs().max((d.pos.y as i32 - u.pos.y as i32).abs()) + 3 * (d.pos.z as i32 - u.pos.z as i32).abs();
                if dist < best.0 {
                    best = (dist, sim.reachable(d.pos, u.pos));
                }
            }
            println!("  {:>6} {:<20} at {:?} wounds {:>2}  nearest soldier {:>4} (reachable {})", id, u.race, (u.pos.x, u.pos.y, u.pos.z), u.wounds.len(), best.0, best.1);
        }
    };
    report(&sim, "start");
    let st0 = sim.combat_stats();
    sim.run_ticks(ticks as u32);
    report(&sim, &format!("after {ticks} ticks"));
    let st = sim.combat_stats();
    println!("attacks {} -> {}, kills {} -> {}", st0.attacks, st.attacks, st0.kills, st.kills);
}
