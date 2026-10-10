//! [animals] Run an imported fort headless and report its animals (round 8).
//!
//!   cargo run -p rf-sim --release --example animals -- [--export <dir>] [--ticks 806400]
//!       [--standdown] [--seed 1] [--place-offmap] [--no-animals] [--no-invasions] [--off-duty]
//!
//! Prints per game season: citizens, tame animals on the map (top races), pregnancies, births,
//! clutches/eggs (laid, hatched), milkings, shearings, cheese, thread, slaughters, hunts (kills),
//! fish caught/cleaned, starved grazers, hungry grazers, bare grass tiles and ticks/s.
//!
//! `--place-offmap` is a what-if scenario, not the real fort: the export lists tame animals
//! without a map position (pos -30000, not simulated); this option puts them into the fort's
//! first Pen/Pasture zone and assigns them to it, to see grazing, breeding and starvation in a
//! pasture of that size.

use rf_sim::Sim;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(806_400);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (mut world, _) = rf_import::load_export(&dir).expect("load export");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    if world.settings.population_cap == 0 {
        if let Some(s) = read("settings.json").and_then(|s| serde_json::from_str(&s).ok()) {
            world.settings = s;
        }
    }
    let mut placed: Vec<(i32, i32)> = Vec::new();
    if flag("--place-offmap") {
        let pen = world.buildings.iter().find(|b| b.kind == "Civzone" && (b.subtype == "Pen" || b.subtype == "Pasture")).cloned();
        if let Some(pen) = pen {
            let mut tiles = Vec::new();
            for y in pen.y1..=pen.y2 {
                for x in pen.x1..=pen.x2 {
                    let p = rf_core::Pos::new(x, y, pen.z);
                    if pen.contains(p) && world.shape(p).has_floor() {
                        tiles.push(p);
                    }
                }
            }
            if !tiles.is_empty() {
                let mut k = 0;
                for u in world.units.iter_mut() {
                    if u.flags.animal && u.flags.tame && !u.flags.dead && u.pos.z < 0 {
                        u.pos = tiles[k % tiles.len()];
                        k += 1;
                        placed.push((u.id, pen.id));
                    }
                }
                let grass = tiles.iter().filter(|&&p| world.tt(p).material.is_grass()).count();
                println!("what-if: placed {} off-map tame animals into pen {} '{}' ({} floor tiles, {} with grass)", placed.len(), pen.id, pen.name, tiles.len(), grass);
            }
        }
    }
    println!("fort {} from {}: {} citizens, {} units", world.meta.fort_name, dir.display(), world.citizens().count(), world.units.len());
    let mut sim = Sim::new(world, seed);
    if flag("--no-animals") {
        sim.set_animals_enabled(false);
    }
    if flag("--no-invasions") {
        sim.set_invasions(false);
    }
    if flag("--off-duty") {
        // Like `examples/scale.rs`: all squads off duty at the start.
        let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
        for id in ids {
            sim.order_squad(id, rf_sim::SquadOrder::Cancel);
            sim.set_squad_schedule(id, "Off duty");
        }
    }
    let mut deaths: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for &(id, pen) in &placed {
        sim.assign_pasture(id, pen);
    }
    let r0 = sim.animal_report();
    println!(
        "start: {} tame animals on the map ({} grazers, {} off the map, not simulated); races: {}",
        r0.tame,
        r0.grazers,
        r0.off_map,
        r0.by_race.iter().map(|(r, n)| format!("{r} {n}")).collect::<Vec<_>>().join(", ")
    );
    let standdown = flag("--standdown");
    let mut calm_since: Option<u64> = None;
    let season = 3 * rf_core::TICKS_PER_MONTH as u64;
    let mut done = 0u64;
    let mut sim_secs = 0.0;
    let mut chunk_secs = 0.0;
    let mut chunk_ticks = 0u64;
    println!(
        "{:<20} {:>4} {:>5} {:>4} {:>5} {:>8} {:>5} {:>5} {:>6} {:>5} {:>6} {:>8} {:>8} {:>5} {:>5} {:>4} {:>8}",
        "date", "cit", "tame", "preg", "birth", "eggs l/h", "milk", "wool", "chs/th", "slau", "hunt", "fish c/c", "pets/ver", "starv", "hungr", "bare", "ticks/s"
    );
    while done < ticks {
        let n = 100u64.min(ticks - done);
        let t = Instant::now();
        sim.run_ticks(n as u32);
        let el = t.elapsed().as_secs_f64();
        sim_secs += el;
        chunk_secs += el;
        chunk_ticks += n;
        done += n;
        if standdown {
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
                if on_duty && done - since >= 1200 {
                    let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
                    for id in ids {
                        sim.order_squad(id, rf_sim::SquadOrder::Cancel);
                        sim.set_squad_schedule(id, "Off duty");
                    }
                }
            }
        }
        for a in sim.drain_announcements() {
            if a.kind == rf_sim::AnnouncementKind::Death {
                let cause = a.text.split(" has ").nth(1).unwrap_or(&a.text).trim_end_matches('.').to_string();
                let who = if a.pos.is_some() && sim.world.units.iter().any(|u| u.flags.citizen && a.text.starts_with(&u.name) && !u.name.is_empty()) { "citizen" } else { "other" };
                *deaths.entry(format!("{who}: {cause}")).or_default() += 1;
            }
        }
        if done % season == 0 || done == ticks {
            let c = sim.world.calendar;
            let r = sim.animal_report();
            let tps = chunk_ticks as f64 / chunk_secs.max(1e-9);
            println!(
                "{:<20} {:>4} {:>5} {:>4} {:>5} {:>8} {:>5} {:>5} {:>6} {:>5} {:>6} {:>8} {:>8} {:>5} {:>5} {:>4} {:>8.0}",
                format!("{} {} {}", c.day(), c.month_name(), c.year),
                sim.world.citizens().count(),
                r.tame,
                r.pregnant,
                r.births,
                format!("{}/{}", r.eggs_laid, r.eggs_hatched),
                r.milkings,
                r.shearings,
                format!("{}/{}", r.cheese, r.thread),
                r.slaughtered,
                format!("{}({})", r.hunts, r.hunt_kills),
                format!("{}/{}", r.fish_caught, r.fish_cleaned),
                format!("{}/{}", r.adoptions, r.vermin_killed),
                r.starved,
                r.hungry_grazers,
                r.bare_tiles,
                tps
            );
            chunk_secs = 0.0;
            chunk_ticks = 0;
        }
    }
    let r = sim.animal_report();
    println!("\nend: {} citizens, {} tame animals on the map", sim.world.citizens().count(), r.tame);
    println!("  by race: {}", r.by_race.iter().map(|(r, n)| format!("{r} {n}")).collect::<Vec<_>>().join(", "));
    println!(
        "  pregnancies {}, egg-layer matings {}, births {}, clutches {} ({} eggs laid, {} hatched), milkings {}, shearings {}, cheese {}, thread {}",
        r.pregnancies, r.fertilizations, r.births, r.clutches, r.eggs_laid, r.eggs_hatched, r.milkings, r.shearings, r.cheese, r.thread
    );
    println!(
        "  slaughtered {}, led to pasture {}, starved {}, grass eaten {} %-tiles, bare tiles {}",
        r.slaughtered, r.led_to_pasture, r.starved, r.grass_eaten, r.bare_tiles
    );
    println!(
        "  training sessions {}, tamed {}, war {}, hunting {}, reverted {}, war attacks {}",
        r.training_sessions, r.tamed, r.war_trained, r.hunt_trained, r.reverted, r.war_attacks
    );
    println!(
        "  hunts {} ({} kills, {} shots), fishing trips {} ({} caught, {} cleaned), adoptions {}, pet thoughts {}, vermin killed {}",
        r.hunts, r.hunt_kills, r.shots, r.fishing_trips, r.fish_caught, r.fish_cleaned, r.adoptions, r.pet_thoughts, r.vermin_killed
    );
    println!("  hunt targets now {}, crossbow and bolts free: {}", r.hunt_targets, r.hunt_gear);
    println!("  deaths: {}", deaths.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
    println!("  {} ticks in {:.1} s = {:.0} ticks/s; animal passes and animal AI: {:.2} s ({:.1} %)", done, sim_secs, done as f64 / sim_secs.max(1e-9), r.cpu_secs, 100.0 * r.cpu_secs / sim_secs.max(1e-9));
}
