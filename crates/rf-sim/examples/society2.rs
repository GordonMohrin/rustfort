//! [society2] Run an imported fort headless and report the round-8 society modules:
//! locations, visitors and petitions, crimes and punishments, clothing, meals, miasma.
//!
//!   cargo run -p rf-sim --release --example society2 -- [--export <dir>] [--ticks 806400]
//!       [--standdown] [--player] [--seed 1] [--off]
//!
//! `--player` emulates a player who uses the round-8 features: appoints a sheriff and a
//! hammerer if vacant, gives the taverns a tavern keeper and a performer and the libraries a
//! scholar (idle citizens), lets the sheriff convict accused criminals and accepts every
//! residency petition (citizenship petitions are left to expire). Without it the fort runs as
//! exported (DF: no law enforcer, nobody answers petitions).

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
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    println!("fort {} from {}: {} citizens, {} locations exported", world.meta.fort_name, dir.display(), world.citizens().count(), world.locations.len());
    let mut sim = Sim::new(world, seed);
    if flag("--off") {
        sim.set_society2_enabled(false);
    }
    sim.society2_ensure();
    for l in &sim.world.locations {
        println!("  location {:>2} {:<9} {:<40} deity {:>3} zones {:?}", l.id, l.kind, l.name, l.deity, l.zones);
    }
    let player = flag("--player");
    if player {
        let cits: Vec<i32> = sim.world.citizens().filter(|u| u.squad_id < 0 && u.profession != "CHILD" && u.profession != "BABY").map(|u| u.id).collect();
        let mut pick = cits.into_iter().rev();
        for tok in ["SHERIFF", "HAMMERER"] {
            if sim.world.nobles.iter().all(|n| n.token != tok || n.holder < 0) {
                if let Some(u) = pick.next() {
                    sim.appoint(tok, u);
                }
            }
        }
        let locs: Vec<(i32, String)> = sim.world.locations.iter().map(|l| (l.id, l.kind.clone())).collect();
        for (id, kind) in locs {
            match kind.as_str() {
                "Tavern" => {
                    if let Some(u) = pick.next() {
                        sim.assign_occupation(id, "TAVERN_KEEPER", u);
                    }
                    if let Some(u) = pick.next() {
                        sim.assign_occupation(id, "PERFORMER", u);
                    }
                }
                "Library" => {
                    if let Some(u) = pick.next() {
                        sim.assign_occupation(id, "SCHOLAR", u);
                    }
                }
                _ => {}
            }
        }
        sim.set_auto_convict(true);
    }
    let standdown = flag("--standdown");
    let mut calm_since: Option<u64> = None;
    let chunk = 1200u64 * 28;
    let mut done = 0u64;
    let mut secs = 0.0;
    let mut deaths: std::collections::BTreeMap<String, usize> = Default::default();
    let t_all = Instant::now();
    println!("{:<22} {:>5} {:>6} {:>6} {:>6} {:>6} {:>7} {:>8}", "date", "cit", "visit", "crimes", "jail", "miasma", "repl", "ticks/s");
    let mut month_secs = 0.0;
    let mut month_ticks = 0u64;
    while done < ticks {
        // 100-tick steps like `rustfort bench` (standdown reacts within 100 ticks).
        let n = 100u64.min(ticks - done);
        let t = Instant::now();
        sim.run_ticks(n as u32);
        let el = t.elapsed().as_secs_f64();
        secs += el;
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
            if a.kind == rf_sim::AnnouncementKind::Death {
                let cause = a.text.split(" has ").nth(1).unwrap_or(&a.text).trim_end_matches('.').to_string();
                *deaths.entry(cause).or_default() += 1;
            }
        }
        if player {
            let pending: Vec<(i32, String)> = sim.petitions().iter().filter(|p| p.status == "Pending").map(|p| (p.id, p.kind.clone())).collect();
            for (id, kind) in pending {
                if kind == "Residency" {
                    sim.answer_petition(id, true);
                }
            }
        }
        if done % chunk != 0 && done != ticks {
            continue;
        }
        let (n, el) = (month_ticks, month_secs.max(1e-9));
        month_ticks = 0;
        month_secs = 0.0;
        let c = sim.world.calendar;
        let jr = sim.justice_report();
        let lr = sim.lifestyle_report();
        println!(
            "{:<22} {:>5} {:>6} {:>6} {:>6} {:>6} {:>7} {:>8.0}",
            format!("{} {} {}", c.day(), c.month_name(), c.year),
            sim.world.citizens().count(),
            sim.visitors().len(),
            sim.world.crimes.len(),
            jr.jailed,
            lr.miasma_events,
            lr.replacements,
            n as f64 / el
        );
        if flag("--food") {
            let pr = sim.production_report();
            let ids: Vec<i32> = sim.world.citizens().map(|u| u.id).collect();
            let hungry = ids.iter().filter(|&&id| sim.unit_needs(id).map(|n| n.1 >= 45_000).unwrap_or(false)).count();
            let thirsty = ids.iter().filter(|&&id| sim.unit_needs(id).map(|n| n.0 >= 22_000).unwrap_or(false)).count();
            let mut acts: std::collections::BTreeMap<String, usize> = Default::default();
            for &id in &ids {
                *acts.entry(sim.unit_status(id).activity).or_default() += 1;
            }
            println!("    meals {} raw {} drinks {} | hungry {} thirsty {} | {:?}", pr.meals, pr.raw_food, pr.drinks, hungry, thirsty, acts);
            let hs: Vec<String> = sim
                .hostiles()
                .iter()
                .take(6)
                .map(|&id| {
                    let u = sim.world.unit(id).unwrap();
                    format!("{} {} {:?} {}", u.race, u.profession, u.pos, sim.unit_status(id).activity)
                })
                .collect();
            let sq: Vec<String> = sim.world.squads.iter().map(|q| format!("{}:{}:{}", q.id, q.schedule, q.orders.len())).collect();
            println!("    hostiles {} {:?} | squads {:?}", sim.hostiles().len(), hs, sq);
        }
    }
    let total = t_all.elapsed().as_secs_f64();
    println!("deaths by cause: {:?}", deaths);
    println!("\n{} ticks in {:.1} s sim time ({:.0} ticks/s), {:.1} s wall", done, secs, done as f64 / secs, total);
    let lr = sim.location_report();
    println!("\n== locations (derived from zones: {})", lr.derived_locations);
    for (id, kind, name, days) in &lr.usage {
        println!("  {:>2} {:<9} {:<40} {:>6} citizen-days", id, kind, name, days);
    }
    println!(
        "  drinks served {} | performances {} (audience {}) | dedicated prayers {} | books read {} written {} | scholar-days {}",
        lr.drinks_served, lr.performances, lr.audience, lr.dedicated_prayers, lr.books_read, lr.books_written, lr.scholar_days
    );
    println!("== visitors: {} arrived, {} left, {} petitions ({} accepted, {} denied/expired)", lr.visitors.len(), lr.visitors_left, lr.petitions, lr.accepted, lr.denied);
    let mut by_role: std::collections::BTreeMap<String, usize> = Default::default();
    for (_, role, _) in &lr.visitors {
        *by_role.entry(role.clone()).or_default() += 1;
    }
    println!("  by role: {:?}", by_role);
    for p in sim.petitions().iter().take(12) {
        println!("  petition {:>2} {:<11} {:<14} {:<28} {}", p.id, p.kind, p.role, p.name, p.status);
    }
    let jr = sim.justice_report();
    println!("== justice: crimes {:?}", jr.crimes);
    println!(
        "  identified {} | convicted {} (wrongful {}) | jailed {} ({} jail-days, {} released) | beatings {} | hammerstrikes {} (broken bones {}) | jail->beating {} | thefts {} (value {}) | unsolved for lack of a law enforcer {}",
        jr.identified, jr.convicted, jr.wrongful, jr.jailed, jr.jail_days, jr.released, jr.beatings, jr.hammerstrikes, jr.broken_bones, jr.no_jail, jr.thefts, jr.stolen_value, jr.no_enforcer
    );
    for c in sim.world.crimes.iter().take(12) {
        println!("  crime {:>2} {:<26} criminal {:>6} victim {:>6} witnesses {:>2} {:<9} {}", c.id, c.kind, c.criminal, c.victim, c.witnesses.len(), c.status, c.punishment);
    }
    let lr = sim.lifestyle_report();
    println!("== lifestyle");
    println!(
        "  clothing: {} wear steps, {} replaced, {} fell apart, thoughts OldClothing {} TatteredClothing {}",
        lr.wear_steps, lr.replacements, lr.rotted_off, lr.old_clothing, lr.tattered_clothing
    );
    println!("  meals cooked by quality (ordinary .. masterwork): {:?}", lr.meal_quality);
    println!("  miasma: {} events, max {} sources / {} tiles", lr.miasma_events, lr.miasma_sources_max, lr.miasma_tiles_max);
    println!("  rain: {} rainy days, {} Rain thoughts | kitchen rules {}", lr.rain_days, lr.rain_thoughts, lr.kitchen_rules);
    println!("citizens at the end: {}, state hash {:016x}", sim.world.citizens().count(), sim.state_hash());
}
