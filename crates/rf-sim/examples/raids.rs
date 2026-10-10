//! [military2] Raids on the real fort: a forced ambush, thief, snatcher or siege (with camp and
//! monsters); runs until the raiders are gone and reports the outcome.
//!
//!   cargo run -p rf-sim --release --example raids -- [--export <dir>] --kind ambush|thief|snatcher|siege
//!       [--n 0] [--monsters 5] [--ticks 40000] [--seed 1] [--standdown] [--log 25]
//!
//! `--standdown` emulates the player (or a guard script) like `rustfort bench`: all squads
//! go to Ready while a visible threat is near, and off duty one day after it is gone.

use rf_sim::{AnnouncementKind, RaidKind, Sim, SquadOrder};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let has = |name: &str| args.iter().any(|a| a == name);
    let kind = match get("--kind").as_deref().unwrap_or("ambush") {
        "thief" => RaidKind::Thief,
        "snatcher" => RaidKind::Snatcher,
        "siege" => RaidKind::Siege,
        _ => RaidKind::Ambush,
    };
    let n: usize = get("--n").and_then(|s| s.parse().ok()).unwrap_or(0);
    let monsters: usize = get("--monsters").and_then(|s| s.parse().ok()).unwrap_or(if kind == RaidKind::Siege { 5 } else { 0 });
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(40_000);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let nlog: usize = get("--log").and_then(|s| s.parse().ok()).unwrap_or(25);
    let standdown = has("--standdown");
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    println!("fort: {} from {} | {} citizens, {} squads", world.meta.fort_name, dir.display(), world.citizens().count(), world.squads.len());
    for q in &world.squads {
        let name = if q.alias.is_empty() { q.name.clone() } else { q.alias.clone() };
        println!("  squad {name:<10} {:>2} members, schedule {:?}, orders {:?}, barracks {:?}", q.members.iter().filter(|&&m| m >= 0).count(), q.schedule, q.orders, q.barracks);
    }
    let mut world = world;
    // --break-leg N: N citizens (not soldiers) get a broken lower leg (Recover wounded check).
    let legs: usize = get("--break-leg").and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut broken = Vec::new();
    for u in world.units.iter_mut().filter(|u| u.is_worker() && u.squad_id < 0 && u.race == "DWARF").take(legs) {
        u.wounds.push(rf_core::Wound { body_part: "LLL".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 6000, pain: 30, bleeding: 0, age: 0 });
        broken.push((u.id, u.name.clone(), u.pos));
    }
    let mut sim = Sim::new(world, seed);
    sim.set_invasions(has("--invasions")); // default: only the forced raid
    sim.run_ticks(10);
    let citizens0 = sim.world.citizens().count();
    let t0 = Instant::now();
    let none = get("--kind").as_deref() == Some("none");
    let arrived = if none { 0 } else { sim.force_raid(kind, n, monsters) };
    println!("forced {kind:?}: {arrived} creatures arrived (hidden: {})", sim.world.units.iter().filter(|u| sim.unit_hidden(u.id)).count());
    let mut done = 0u64;
    let mut calm_since: Option<u64> = None;
    let mut first_seen = None;
    let mut charged_at = None;
    let mut events: Vec<String> = Vec::new();
    while done < ticks {
        sim.run_ticks(100);
        done += 100;
        for a in sim.drain_announcements() {
            if matches!(a.kind, AnnouncementKind::Warning | AnnouncementKind::Death) || a.text.contains("retreat") || a.text.contains("over") {
                events.push(format!("tick +{done}: {}", a.text));
            }
        }
        if first_seen.is_none() && (sim.combat_stats().raids_spotted > 0 || kind == RaidKind::Siege) {
            first_seen = Some(done);
        }
        if charged_at.is_none() && sim.combat_log().iter().rev().take(50).any(|l| l.ends_with("charge!")) {
            charged_at = Some(done);
        }
        if standdown {
            let threat = !sim.hostiles().is_empty();
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
                if done - since >= rf_core::TICKS_PER_DAY as u64 && sim.world.squads.iter().any(|q| !q.schedule.eq_ignore_ascii_case("Off duty")) {
                    let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
                    for id in ids {
                        sim.order_squad(id, SquadOrder::Cancel);
                        sim.set_squad_schedule(id, "Off duty");
                    }
                }
            }
        }
        if sim.raids().is_empty() && !none {
            break;
        }
    }
    let el = t0.elapsed().as_secs_f64();
    let st = sim.combat_stats();
    println!("ran {done} ticks in {el:.2}s = {:.0} ticks/s", done as f64 / el.max(1e-9));
    for e in events.iter().take(40) {
        println!("  {e}");
    }
    println!(
        "spotted after: {} | charged after: {} | raids left: {:?}",
        first_seen.map(|t| format!("{t} ticks")).unwrap_or_else(|| "never".into()),
        charged_at.map(|t| format!("{t} ticks")).unwrap_or_else(|| "-".into()),
        sim.raids()
    );
    println!(
        "citizens {citizens0} -> {} | raiders killed {}, left the map {} | attacks {}, hits {} | stolen {}, lost {}, children abducted {} | monsters {}, leaders killed {} | rescues {}, crawlers {}",
        sim.world.citizens().count(),
        st.invaders_killed,
        st.invaders_left,
        st.attacks,
        st.hits,
        st.items_stolen,
        st.items_lost,
        st.children_abducted,
        st.monsters_spawned,
        st.leaders_killed,
        st.rescues,
        st.crawlers
    );
    println!(
        "raids: {} sieges ({} camped), {} ambushes, {} thieves, {} snatchers, {} spotted | sparring bouts {}, injuries {}, demonstrations {}, barracks nights {}, patrol points {}",
        st.sieges, st.siege_camps, st.ambushes, st.thieves, st.snatchers, st.raids_spotted, st.sparring, st.sparring_injuries, st.demonstrations, st.barracks_sleeps, st.patrol_points
    );
    for (id, name, p0) in &broken {
        let u = sim.world.unit(*id).unwrap();
        println!("  broken leg: {name} from {p0:?} -> {:?}, {}", u.pos, sim.unit_status(*id).activity);
    }
    let log = sim.combat_log();
    println!("--- combat log (last {nlog}) ---");
    for l in log.iter().rev().take(nlog).collect::<Vec<_>>().into_iter().rev() {
        println!("  {l}");
    }
}
