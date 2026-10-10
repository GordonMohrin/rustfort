//! Run an imported fort headless and report strange moods, health care and caverns (round 5).
//!
//!   cargo run -p rf-sim --release --example moods -- [--export <dir>] [--ticks 806400]
//!       [--standdown] [--no-invasions] [--seed N] [--force-mood] [--force-beast] [--no-r5]
//!
//! Prints the detected cavern layers, then per game month citizens, patients, open care jobs and
//! ticks/s, and at the end every mood (who, type, skill, workshop, demands, outcome), the health
//! counters with recent treatment events, cave wildlife arrivals, forgotten beasts and deaths.
//! `--standdown` emulates the player standing the military down after a fight (like `bench`).

use rf_sim::{AnnouncementKind, Sim};
use std::collections::BTreeMap;
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
    println!("fort {} from {}: {} citizens, {} units", world.meta.fort_name, dir.display(), world.citizens().count(), world.units.len());
    let t0 = Instant::now();
    let mut sim = Sim::new(world, seed);
    println!("sim ready in {:.2}s", t0.elapsed().as_secs_f64());
    if flag("--no-invasions") {
        sim.set_invasions(false);
    }
    if flag("--no-r5") {
        // Baseline for speed comparisons: no moods, health care or caverns.
        sim.set_moods_enabled(false);
        sim.set_health_enabled(false);
        sim.set_caverns_enabled(false);
    }
    let tc = Instant::now();
    sim.redetect_caverns();
    println!("cavern detection: {:.0} ms", tc.elapsed().as_secs_f64() * 1000.0);
    for l in &sim.cavern_report().layers {
        println!(
            "  {:<10} z {:>3}..{:<3} {:>7} open tiles, {:>7} revealed, breached: {}",
            l.label(),
            l.z_min,
            l.z_max,
            l.tiles,
            l.revealed,
            l.breached
        );
    }
    let h0 = sim.health_report();
    println!("doctors (any medical labor): {}", h0.doctors);
    if flag("--force-mood") {
        let id = sim.world.citizens().map(|u| u.id).nth(3).unwrap();
        sim.force_mood(id, "");
        println!("forced a mood on {id}: {:?} demands {:?}", sim.mood_info(id), sim.mood_demands());
    }
    if flag("--force-beast") {
        println!("forced beast: {:?}", sim.spawn_forgotten_beast());
    }
    let standdown = flag("--standdown");
    let mut calm_since: Option<u64> = None;
    let mut deaths: BTreeMap<String, usize> = BTreeMap::new();
    let mut mood_lines: Vec<String> = Vec::new();
    let month = rf_core::TICKS_PER_MONTH as u64;
    let (mut done, mut sim_secs, mut month_secs, mut month_ticks) = (0u64, 0.0f64, 0.0f64, 0u64);
    println!("{:<22} {:>5} {:>8} {:>8} {:>6} {:>7} {:>6} {:>8}", "date", "cit", "patients", "diagn.", "infect", "artif.", "units", "ticks/s");
    while done < ticks {
        let n = 100u64.min(ticks - done);
        let t = Instant::now();
        sim.run_ticks(n as u32);
        let el = t.elapsed().as_secs_f64();
        sim_secs += el;
        month_secs += el;
        month_ticks += n;
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
            if a.kind == AnnouncementKind::Death {
                let cause = a.text.split(" has ").nth(1).unwrap_or(&a.text).trim_end_matches('.').to_string();
                *deaths.entry(cause).or_default() += 1;
            }
            let t = &a.text;
            if t.contains("mood") || t.contains("withdraws from society") || t.contains("possessed") || t.contains("fell and terrible") || t.contains("stalk and brood")
                || t.contains("has claimed a") || t.contains("mysterious construction") || t.contains(" has created ") || t.contains("melancholy")
                || t.contains("stark raving") || t.contains("berserk") || t.contains("catatonic") || t.contains("Forgotten Beast") || t.contains("murdered")
            {
                let c = sim.world.calendar;
                mood_lines.push(format!("{} {} {}: {t}", c.day(), c.month_name(), c.year));
            }
        }
        if done % month == 0 || done == ticks {
            let c = sim.world.calendar;
            let h = sim.health_report();
            let (arts, _) = sim.mood_counts();
            let units = sim.world.units.iter().filter(|u| !u.flags.dead && !u.flags.inactive).count();
            println!(
                "{:<22} {:>5} {:>8} {:>8} {:>6} {:>7} {:>6} {:>8.0}",
                format!("{} {} {}", c.day(), c.month_name(), c.year),
                sim.world.citizens().count(),
                h.patients_now,
                h.diagnoses,
                h.infections,
                arts,
                units,
                month_ticks as f64 / month_secs.max(1e-9)
            );
            month_secs = 0.0;
            month_ticks = 0;
        }
    }
    println!();
    println!("ran {done} ticks, sim {sim_secs:.1}s = {:.0} ticks/s", done as f64 / sim_secs.max(1e-9));
    println!("citizens at the end: {}  |  state hash {:016x}", sim.world.citizens().count(), sim.state_hash());
    println!("deaths by cause: {deaths:?}");
    println!("\n== strange moods ==");
    let (arts, insane) = sim.mood_counts();
    println!("artifacts created: {arts}, insane after failed moods: {insane}");
    for m in sim.mood_records() {
        println!(
            "  tick {:>7}: {} ({}) {} mood, skill {}, {}; demands {:?}{} -> {} (after {} ticks)",
            m.started,
            m.name,
            m.unit_id,
            m.kind,
            m.skill,
            m.workshop,
            m.demands,
            m.victim.as_ref().map(|v| format!(", victim {v}")).unwrap_or_default(),
            if m.outcome.is_empty() { "still in the mood" } else { &m.outcome },
            m.ended.unwrap_or(sim.ticks()) - m.started
        );
    }
    for l in mood_lines.iter().take(60) {
        println!("    {l}");
    }
    println!("\n== health care ==");
    let h = sim.health_report();
    println!(
        "diagnoses {}, sutures {}, dressings {}, bones set {}, surgeries {}, water given {}, meals given {}",
        h.diagnoses, h.sutures, h.dressings, h.bones_set, h.surgeries, h.waters_given, h.meals_given
    );
    println!("infections {}, cleared {}, deaths by infection {}; patients now {}, doctors {}", h.infections, h.infections_cured, h.infection_deaths, h.patients_now, h.doctors);
    for (t, e) in h.events.iter().rev().take(25).collect::<Vec<_>>().into_iter().rev() {
        println!("    tick {t:>7}: {e}");
    }
    println!("\n== caverns ==");
    let cr = sim.cavern_report();
    for l in &cr.layers {
        println!("  {:<10} z {}..{} tiles {} revealed {} breached {}", l.label(), l.z_min, l.z_max, l.tiles, l.revealed, l.breached);
    }
    println!("cave wildlife groups: {}", cr.wildlife.len());
    for (t, layer, name, n, hostile) in &cr.wildlife {
        println!("    tick {t:>7}: {n} {name} in the {layer}{}", if *hostile { " (hostile)" } else { "" });
    }
    println!("forgotten beasts: {}", cr.beasts.len());
    for b in &cr.beasts {
        println!(
            "    tick {:>7}: {} in the {} ({}, size {}): {}; {}",
            b.arrived,
            b.name,
            b.layer,
            b.material,
            b.body_size,
            b.description,
            b.killed.map(|k| format!("slain at tick {k}")).unwrap_or_else(|| "still alive".into())
        );
    }
}
