//! Run an imported fort headless and report population and trade (round 4).
//!
//!   cargo run -p rf-sim --release --example society -- [--export <dir>] [--ticks 806400]
//!       [--standdown] [--no-invasions] [--trade] [--pop-cap 75 --strict-cap 100 --baby-cap 266]
//!
//! Prints per game month: citizens, children, pregnancies, ghosts, unburied dead, caravan state
//! and ticks/s; at the end migrant waves, births, deaths by cause, burials, ghosts, caravans,
//! trades, elections and mandates. `--trade` emulates a player: when merchants are at the depot
//! it moves up to 30 crafts to the depot, calls the broker and trades them for goods worth less.

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
    // Round-4 files, in case the importer does not read them yet.
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    if world.entities.is_empty() {
        if let Some(e) = read("entities.json").and_then(|s| serde_json::from_str(&s).ok()) {
            world.entities = e;
        }
    }
    if world.nobles.is_empty() {
        if let Some(n) = read("nobles.json").and_then(|s| serde_json::from_str(&s).ok()) {
            world.nobles = n;
        }
    }
    if world.settings.population_cap == 0 {
        if let Some(s) = read("settings.json").and_then(|s| serde_json::from_str(&s).ok()) {
            world.settings = s;
        }
    }
    if let Some(c) = get("--pop-cap").and_then(|s| s.parse().ok()) {
        world.settings.population_cap = c;
    }
    if let Some(c) = get("--strict-cap").and_then(|s| s.parse().ok()) {
        world.settings.strict_population_cap = c;
    }
    if let Some(c) = get("--baby-cap").and_then(|s| s.parse().ok()) {
        world.settings.baby_cap = c;
    }
    println!(
        "fort {} from {}: {} citizens, {} entities, {} noble positions, settings {:?}",
        world.meta.fort_name,
        dir.display(),
        world.citizens().count(),
        world.entities.len(),
        world.nobles.len(),
        (world.settings.population_cap, world.settings.strict_population_cap, world.settings.baby_cap)
    );
    let mut sim = Sim::new(world, seed);
    if flag("--no-society") {
        sim.set_society_enabled(false);
    }
    if flag("--no-invasions") {
        sim.set_invasions(false);
    }
    println!("population caps in use (pop, strict, baby, baby %): {:?}", sim.population_caps());
    let r0 = sim.population_report();
    println!("free coffins in tombs: {}  |  broker: {:?}  |  next caravan: {:?}", r0.free_coffins, sim.broker_unit(), sim.population_info().caravan_expected);
    let standdown = flag("--standdown");
    let script_trade = flag("--trade");
    let mut calm_since: Option<u64> = None;
    let mut deaths: BTreeMap<String, usize> = BTreeMap::new();
    let mut traded_for: Option<u64> = None;
    let mut moved_for: Option<u64> = None;
    let mut trade_log: Vec<String> = Vec::new();
    let mut curve: Vec<(String, usize, f64)> = Vec::new();
    let chunk = 100u64;
    let month = rf_core::TICKS_PER_MONTH as u64;
    let mut done = 0u64;
    let mut sim_secs = 0.0;
    let mut month_secs = 0.0;
    let mut month_ticks = 0u64;
    let t_all = Instant::now();
    println!("{:<22} {:>5} {:>5} {:>5} {:>6} {:>6} {:>8}  {:<28} {:>8}", "date", "cit", "kids", "preg", "ghosts", "unbur", "units", "caravan", "ticks/s");
    while done < ticks {
        let n = chunk.min(ticks - done);
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
        if script_trade {
            if let Some(st) = sim.trade_status() {
                let key = sim.trade_report().caravans.last().map(|c| c.arrived).unwrap_or(0);
                if moved_for != Some(key) {
                    moved_for = Some(key);
                    let crafts: Vec<i32> = sim
                        .world
                        .items
                        .iter()
                        .filter(|i| {
                            matches!(i.kind.as_str(), "AMULET" | "FIGURINE" | "RING" | "BRACELET" | "CROWN" | "EARRING" | "GOBLET" | "SCEPTER" | "TOY" | "TOTEM")
                                && i.flags.on_ground
                                && !i.flags.forbid
                                && i.building < 0
                        })
                        .map(|i| i.id)
                        .take(30)
                        .collect();
                    sim.move_to_depot(&crafts);
                    sim.request_broker(true);
                    trade_log.push(format!("tick {}: {} at the depot, {} goods; moving {} crafts, calling the broker", sim.ticks(), st.civ, st.goods.len(), crafts.len()));
                }
                if traded_for != Some(key) && st.broker_present && !st.fort_goods.is_empty() && st.leaves_in < 8 * 1200 {
                    traded_for = Some(key);
                    let offer = st.fort_goods.clone();
                    let ov: i64 = offer.iter().map(|&id| sim.item_trade_value(id)).sum();
                    let mut goods: Vec<(i64, i32)> = st.goods.iter().map(|&id| (sim.item_trade_value(id), id)).collect();
                    goods.sort_by_key(|g| std::cmp::Reverse(*g));
                    let mut want = Vec::new();
                    let mut rv = 0i64;
                    for (v, id) in goods {
                        if (rv + v) * 115 <= ov * 100 {
                            rv += v;
                            want.push(id);
                        }
                    }
                    let o = sim.trade(&offer, &want, true);
                    trade_log.push(format!(
                        "tick {}: offered {} items (shown {}, true {ov}) for {} items (shown {}, true {rv}): {} -> {}",
                        sim.ticks(),
                        offer.len(),
                        o.offer_value,
                        want.len(),
                        o.request_value,
                        o.reply,
                        if o.accepted { "accepted" } else { "refused" }
                    ));
                }
            }
        }
        for a in sim.drain_announcements() {
            if a.kind == AnnouncementKind::Death {
                let cause = a.text.split(" has ").nth(1).unwrap_or(&a.text).trim_end_matches('.').to_string();
                *deaths.entry(cause).or_default() += 1;
            }
        }
        if done % month == 0 || done == ticks {
            let c = sim.world.calendar;
            let r = sim.population_report();
            let info = sim.population_info();
            let car = match sim.trade_status() {
                Some(s) => format!("{} ({} goods)", s.race.to_ascii_lowercase(), s.goods.len()),
                None => info.caravan_expected.clone().map(|e| format!("next: {e}")).unwrap_or_default(),
            };
            let units = sim.world.units.iter().filter(|u| !u.flags.dead && !u.flags.inactive).count();
            let tps = month_ticks as f64 / month_secs.max(1e-9);
            let date = format!("{} {} {}", c.day(), c.month_name(), c.year);
            println!(
                "{:<22} {:>5} {:>5} {:>5} {:>6} {:>6} {:>8}  {:<28} {:>8.0}",
                date, info.citizens, info.children, r.pregnant_now, r.ghosts_active, r.unburied, units, car.chars().take(28).collect::<String>(), tps
            );
            curve.push((date, info.citizens, tps));
            month_secs = 0.0;
            month_ticks = 0;
        }
    }
    let r = sim.population_report();
    let tr = sim.trade_report();
    println!();
    println!("ran {done} ticks in {:.1}s (sim {sim_secs:.1}s) = {:.0} ticks/s", t_all.elapsed().as_secs_f64(), done as f64 / sim_secs.max(1e-9));
    println!("migrant waves: {:?}", r.waves);
    println!("pregnancies started: {}, births: {} {:?}", r.pregnancies_started, r.births.len(), r.births.iter().map(|b| b.0).collect::<Vec<_>>());
    println!("grown up: {:?}", r.grown_up);
    println!("deaths by cause: {deaths:?}  (old age: {})", r.old_age_deaths.len());
    println!("burials: {} {:?}", r.burials.len(), r.burials.iter().take(12).collect::<Vec<_>>());
    println!("ghosts risen: {} {:?}; laid to rest: {}; active: {}; unburied now: {}; free coffins: {}", r.ghosts_risen.len(), r.ghosts_risen.iter().take(8).collect::<Vec<_>>(), r.ghosts_laid.len(), r.ghosts_active, r.unburied, r.free_coffins);
    println!("elections: {:?}", r.elections);
    println!("mandates: issued {}, met {}, missed {}, active {:?}", r.mandates_issued, r.mandates_met, r.mandates_missed, r.mandates_active);
    for c in &tr.caravans {
        println!(
            "caravan {} ({}): arrived {} unloaded {:?} left {:?}; {} merchants, {} animals, {} wagons, {} goods worth {}, {} trades {}",
            c.civ, c.race, c.arrived, c.unloaded, c.left, c.merchants, c.animals, c.wagons, c.goods, c.goods_value, c.trades, c.note
        );
    }
    println!("items moved to the depot: {}", tr.items_moved_to_depot);
    for l in &trade_log {
        println!("  {l}");
    }
    println!("population / speed curve (month: citizens, ticks/s):");
    for (d, c, t) in &curve {
        println!("  {d:<22} {c:>4} {t:>8.0}");
    }
}
