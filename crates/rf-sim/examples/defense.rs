//! Fort defense on the real fort: run the export headless (with the bench's stand-down player
//! emulation) and report the sieges: deaths per side, invaders caught or killed by traps,
//! lever pulls, crushed and flung units, plus a monthly line.
//!
//!   cargo run -p rf-sim --release --example defense -- [--export <dir>] [--ticks 806400]
//!       [--seed 1] [--no-invasions] [--no-standdown] [--hostiles] [--log 20]
//!
//! `--hostiles` lists the remaining hostiles every month (debugging stuck sieges).

use rf_sim::{AnnouncementKind, MechKind, Sim, SquadOrder};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let has = |name: &str| args.iter().any(|a| a == name);
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(806_400);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let nlog: usize = get("--log").and_then(|s| s.parse().ok()).unwrap_or(20);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let flagged = world.buildings.iter().filter(|b| !b.flags.is_empty()).count();
    let linked = world.buildings.iter().filter(|b| !b.links.is_empty()).count();
    let mut sim = Sim::new(world, seed);
    if has("--no-invasions") {
        sim.set_invasions(false);
    }
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for (_, k) in sim.mechanisms() {
        *kinds.entry(format!("{k:?}")).or_default() += 1;
    }
    println!("fort: {} from {} | {} buildings with state flags, {} with links | mechanisms: {kinds:?}", sim.world.meta.fort_name, dir.display(), flagged, linked);
    for (id, k) in sim.mechanisms() {
        let b = sim.world.building(id).unwrap();
        let odd_door = k == MechKind::Door && b.flags.iter().any(|f| f != "CLOSED" && f != "OPEN");
        if matches!(k, MechKind::Lever | MechKind::Bridge | MechKind::PressurePlate) || odd_door {
            println!("  {k:?} {id} at ({},{},{})-({},{}) flags {:?} links {:?} dir {:?}", b.x1, b.y1, b.z, b.x2, b.y2, b.flags, b.links, b.direction);
        }
    }
    let standdown = !has("--no-standdown");
    // `--force`: a goblin siege arrives right away (size from the population, like DF).
    if has("--force") {
        sim.run_ticks(10);
        let n = sim.force_siege();
        println!("forced siege: {n} goblins");
    }
    let citizens0 = sim.world.citizens().count();
    let hostile0 = sim.hostiles().len();
    let mut deaths: BTreeMap<String, usize> = BTreeMap::new();
    let mut warnings: BTreeMap<String, usize> = BTreeMap::new();
    let (mut fort_deaths, mut calm_since, mut standdowns) = (0usize, None::<u64>, 0usize);
    let start = Instant::now();
    let mut done = 0u64;
    let month = rf_core::TICKS_PER_MONTH as u64;
    while done < ticks {
        let n = 100.min(ticks - done);
        sim.run_ticks(n as u32);
        done += n;
        if standdown {
            // Same player emulation as `rustfort bench --standdown`.
            let cits: Vec<rf_core::Pos> = sim.world.citizens().map(|u| u.pos).collect();
            let threat = sim.hostiles().iter().any(|&id| {
                sim.unit_status(id).activity != "Lurk"
                    && sim.world.unit(id).map(|u| cits.iter().any(|&c| c.dist(u.pos) <= 30 && sim.reachable(u.pos, c))).unwrap_or(false)
            });
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
                if on_duty && done - since >= rf_core::TICKS_PER_DAY as u64 {
                    let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
                    for id in ids {
                        sim.order_squad(id, SquadOrder::Cancel);
                        sim.set_squad_schedule(id, "Off duty");
                    }
                    standdowns += 1;
                }
            }
        }
        for a in sim.drain_announcements() {
            if a.kind == AnnouncementKind::Death {
                let cause = a.text.splitn(3, ' ').nth(2).unwrap_or("").to_string();
                *deaths.entry(cause).or_default() += 1;
            } else if a.kind == AnnouncementKind::Warning {
                let key = if a.text.contains(" has been caught in ") { "<creature> has been caught in a cage trap!".to_string() } else { a.text.clone() };
                *warnings.entry(key).or_default() += 1;
            }
        }
        if done % month == 0 || done == ticks {
            let st = sim.defense_stats();
            let cs = sim.combat_stats();
            let alive = sim.world.citizens().count();
            fort_deaths = citizens0.saturating_sub(alive);
            println!(
                "{:>4} {:<9} {:>3}: alive {alive:>2} | hostiles {:>3} | sieges {} spawned {} killed {} left {} | caught {} trap kills {} trap attacks {} stones {} | levers {} crushed {} flung {}",
                sim.world.calendar.year,
                sim.world.calendar.month_name(),
                sim.world.calendar.day(),
                sim.hostiles().len(),
                cs.sieges,
                cs.invaders_spawned,
                cs.invaders_killed,
                cs.invaders_left,
                st.caught,
                st.trap_kills,
                st.trap_attacks,
                st.stones_dropped,
                st.levers_pulled,
                st.crushed,
                st.flung
            );
            if has("--hostiles") {
                for id in sim.hostiles().iter().take(12) {
                    let u = sim.world.unit(*id).unwrap();
                    println!("    hostile {id} {} at {:?}: {}", u.race, u.pos, sim.unit_status(*id).activity);
                }
            }
        }
    }
    let el = start.elapsed().as_secs_f64();
    let st = sim.defense_stats();
    let cs = sim.combat_stats();
    println!("ran {done} ticks in {el:.2}s = {:.0} ticks/s; military stood down {standdowns} time(s)", done as f64 / el);
    println!(
        "citizens: {citizens0} at start, {} alive ({fort_deaths} lost) | hostiles at start {hostile0}, sieges {}, invaders spawned {}, killed {}, caught {}, left {}",
        sim.world.citizens().count(),
        cs.sieges,
        cs.invaders_spawned,
        cs.invaders_killed,
        st.caught,
        cs.invaders_left
    );
    println!("defense: {st:?}");
    println!("deaths by cause: {deaths:?}");
    println!("warnings: {warnings:?}");
    let log = sim.combat_log();
    let traps: Vec<&String> = log.iter().filter(|l| l.contains("trap") || l.contains("bridge") || l.contains("lever")).collect();
    println!("--- trap/bridge/lever log ({} lines, last {nlog}) ---", traps.len());
    for l in traps.iter().skip(traps.len().saturating_sub(nlog)) {
        println!("  {l}");
    }
}
