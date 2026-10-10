//! Ranged combat on the real fort: Gordon's "Schuetzen" (marksdwarves) and the goblin bowmen
//! of the running siege. Runs until the hostiles are gone (or `--ticks`), then reports shots,
//! hits, kills, ammo used, the squad's skills and ammo, and DF-style projectile log lines.
//!
//!   cargo run -p rf-sim --release --example ranged -- [--export <dir>] [--ticks 67200]
//!       [--seed 1] [--log 25] [--squad Schuetzen] [--train] [--full] [--no-invasions]
//!
//! `--train` sets the squad to training (archery practice at the range) once the hostiles are
//! gone; `--full` keeps running after the siege.

use rf_core::{ArmorRaw, Burrow, CreatureRaw, Squad, WeaponRaw, World};
use rf_sim::{AnnouncementKind, Sim, SquadOrder};
use std::path::{Path, PathBuf};
use std::time::Instant;

macro_rules! read {
    ($dir:expr, $name:expr, $t:ty) => {
        std::fs::read_to_string($dir.join($name)).ok().and_then(|s| serde_json::from_str::<$t>(&s).ok()).unwrap_or_default()
    };
}

fn fill_round3(w: &mut World, dir: &Path) {
    if w.creature_raws.is_empty() {
        w.creature_raws = read!(dir, "creature_raws.json", Vec<CreatureRaw>);
    }
    if w.weapon_raws.is_empty() {
        w.weapon_raws = read!(dir, "weapon_raws.json", Vec<WeaponRaw>);
    }
    if w.armor_raws.is_empty() {
        w.armor_raws = read!(dir, "armor_raws.json", Vec<ArmorRaw>);
    }
    if w.squads.is_empty() {
        w.squads = read!(dir, "squads.json", Vec<Squad>);
    }
    if w.burrows.is_empty() {
        w.burrows = read!(dir, "burrows.json", Vec<Burrow>);
    }
}

fn squad_report(sim: &mut Sim, members: &[i32], label: &str) {
    println!("{label}:");
    for &id in members {
        let ammo = sim.unit_ammo(id);
        let Some(u) = sim.world.unit(id) else { continue };
        let sk = |k: &str| u.skills.iter().find(|s| s.id == k).map(|s| format!("{}+{}xp", s.rating, s.xp)).unwrap_or_else(|| "-".into());
        let weapon = u.inventory.iter().filter_map(|i| sim.world.item(*i)).find(|i| i.kind == "WEAPON").map(|i| i.desc.clone()).unwrap_or_else(|| "(no weapon)".into());
        let quiver = u.inventory.iter().filter_map(|i| sim.world.item(*i)).any(|i| i.kind == "QUIVER");
        println!(
            "  {:<24} {:<6} at {:>3},{:>3},{:>3}  CROSSBOW {:<8} ARCHER {:<8} HAMMER {:<7} {weapon}, quiver {}, {ammo} bolts, {} wounds",
            u.name,
            if u.flags.dead { "DEAD" } else { "alive" },
            u.pos.x,
            u.pos.y,
            u.pos.z,
            sk("CROSSBOW"),
            sk("RANGED_COMBAT"),
            sk("HAMMER"),
            if quiver { "yes" } else { "no" },
            u.wounds.len()
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let has = |name: &str| args.iter().any(|a| a == name);
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(67_200);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let nlog: usize = get("--log").and_then(|s| s.parse().ok()).unwrap_or(25);
    let squad_name = get("--squad").unwrap_or_else(|| "Schuetzen".into());
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let t0 = Instant::now();
    let (mut world, _) = rf_import::load_export(&dir).expect("load export");
    fill_round3(&mut world, &dir);
    let ammo_items = world.items.iter().filter(|i| i.kind == "AMMO").map(|i| i.stack.max(0) as i64).sum::<i64>();
    let bowmen = world.units.iter().filter(|u| u.flags.hostile && !u.flags.dead && !u.flags.inactive && u.profession == "BOWMAN").count();
    println!(
        "fort {} from {} | {} units, {} citizens | {} rounds of ammo in the fort | {} hostile bowmen on the map | loaded in {:.2}s",
        world.meta.fort_name,
        dir.display(),
        world.units.len(),
        world.citizens().count(),
        ammo_items,
        bowmen,
        t0.elapsed().as_secs_f64()
    );
    let squad = world.squads.iter().find(|q| q.alias == squad_name || q.name.contains(&squad_name)).cloned();
    let members: Vec<i32> = squad.as_ref().map(|q| q.members.iter().copied().filter(|&m| m >= 0).collect()).unwrap_or_default();
    if let Some(q) = &squad {
        println!("squad {:?}: {} members, orders {:?}, schedule {:?}", q.alias, members.len(), q.orders, q.schedule);
    }
    let mut sim = Sim::new(world, seed);
    if has("--no-invasions") {
        sim.set_invasions(false);
    }
    squad_report(&mut sim, &members, "at start");
    let citizens0 = sim.world.citizens().count();
    let t1 = Instant::now();
    let mut done = 0u64;
    let mut end_tick = None;
    let mut deaths: Vec<String> = Vec::new();
    let mut trained = false;
    while done < ticks {
        sim.run_ticks(100);
        done += 100;
        for a in sim.drain_announcements() {
            if a.kind == AnnouncementKind::Death {
                deaths.push(format!("tick {}: {}", a.tick, a.text));
            }
        }
        if end_tick.is_none() && sim.hostiles().iter().all(|id| sim.unit_status(*id).activity == "Lurk") {
            end_tick = Some(done);
            if has("--train") && !trained {
                if let Some(q) = &squad {
                    sim.order_squad(q.id, SquadOrder::Train);
                    trained = true;
                    println!("tick {done}: hostiles gone or lurking, {} set to Train", q.alias);
                }
            }
            if !has("--full") && !has("--train") {
                break;
            }
        }
        if done % 10_000 == 0 {
            let rs = sim.ranged_stats();
            eprintln!("  {done} ticks: {} hostiles, {} citizens, {} shots ({} by the fort), {} hits, {} kills", sim.hostiles().len(), sim.world.citizens().count(), rs.shots, rs.fort_shots, rs.hits, rs.kills);
        }
    }
    let el = t1.elapsed().as_secs_f64();
    let rs = sim.ranged_stats();
    let st = sim.combat_stats();
    println!("ran {done} ticks in {el:.2}s = {:.0} ticks/s", done as f64 / el.max(1e-9));
    match end_tick {
        Some(t) => println!("active hostiles gone after {t} ticks ({:.1} days)", t as f64 / 1200.0),
        None => println!("hostiles still active after {done} ticks: {}", sim.hostiles().len()),
    }
    println!("citizens {citizens0} -> {} | melee: attacks {}, hits {}, kills (all) {}", sim.world.citizens().count(), st.attacks, st.hits, st.kills);
    println!(
        "ranged: {} shots ({} by the fort, {} by hostiles), {} hits, {} missed, {} dodged, {} blocked, {} deflected by armor, {} killing shots ({} by the fort)",
        rs.shots, rs.fort_shots, rs.hostile_shots, rs.hits, rs.misses, rs.dodged, rs.blocked, rs.deflected, rs.kills, rs.fort_kills
    );
    println!(
        "ammo: {} rounds fired, {} broke, {} landed; {} quivers and {} ammo stacks fetched; {} melee fallbacks ({} out of ammo); practice: {} shots, {} on target",
        rs.ammo_used, rs.ammo_broken, rs.ammo_dropped, rs.quivers_fetched, rs.ammo_fetched, rs.melee_fallbacks, rs.out_of_ammo, rs.practice_shots, rs.practice_hits
    );
    squad_report(&mut sim, &members, "at end");
    let names: Vec<String> = members.iter().filter_map(|id| sim.world.unit(*id)).map(|u| u.name.clone()).collect();
    let log = sim.combat_log();
    let flying: Vec<&String> = log.iter().filter(|l| l.contains("flying")).collect();
    println!("projectile log lines: {} (of {} in the log); sample:", flying.len(), log.len());
    let step = (flying.len() / nlog.max(1)).max(1);
    for l in flying.iter().step_by(step).take(nlog) {
        println!("  {l}");
    }
    let squad_lines: Vec<&String> = log.iter().filter(|l| names.iter().any(|n| !n.is_empty() && l.starts_with(n.as_str())) && l.contains("crossbow")).collect();
    println!("{squad_name} melee lines with the crossbow: {}", squad_lines.len());
    for l in squad_lines.iter().take(5) {
        println!("  {l}");
    }
    println!("deaths ({}):", deaths.len());
    for d in deaths.iter().take(15) {
        println!("  {d}");
    }
    println!("state hash: {:016x}", sim.state_hash());
}
