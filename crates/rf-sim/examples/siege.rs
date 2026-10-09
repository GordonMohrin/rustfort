//! Siege on the real fort: load an export (round 3 for real squads, bodies and raws), optionally
//! force an extra goblin siege, run until the invaders are gone and report the outcome.
//!
//!   cargo run -p rf-sim --release --example siege -- [--export <dir>] [--ticks 100800] [--force]
//!       [--seed 1] [--log 30] [--no-invasions]
//!
//! Round-3 raws (creature_raws.json, weapon_raws.json, armor_raws.json, squads.json,
//! burrows.json) are read here if the importer did not fill them into the World yet.

use rf_core::{ArmorRaw, Burrow, CreatureRaw, Squad, WeaponRaw, World};
use rf_sim::{AnnouncementKind, Sim};
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let has = |name: &str| args.iter().any(|a| a == name);
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(100_800);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let nlog: usize = get("--log").and_then(|s| s.parse().ok()).unwrap_or(30);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let t0 = Instant::now();
    let (mut world, _) = rf_import::load_export(&dir).expect("load export");
    fill_round3(&mut world, &dir);
    let hostile0 = world.units.iter().filter(|u| u.flags.hostile && !u.flags.dead && !u.flags.caged && !u.flags.inactive).count();
    let wounded0 = world.units.iter().filter(|u| !u.flags.dead && !u.wounds.is_empty()).count();
    println!(
        "fort: {} from {} | {} units, {} citizens, {} squads ({} soldiers), {} creature raws, {} weapon raws, {} armor raws | {} hostiles on the map, {} units with wounds | loaded in {:.2}s",
        world.meta.fort_name,
        dir.display(),
        world.units.len(),
        world.citizens().count(),
        world.squads.len(),
        world.squads.iter().map(|q| q.members.iter().filter(|&&m| m >= 0).count()).sum::<usize>(),
        world.creature_raws.len(),
        world.weapon_raws.len(),
        world.armor_raws.len(),
        hostile0,
        wounded0,
        t0.elapsed().as_secs_f64()
    );
    for q in &world.squads {
        println!("  squad {:<10} {:>2} members, orders {:?}, schedule {:?}", if q.alias.is_empty() { q.name.clone() } else { q.alias.clone() }, q.members.iter().filter(|&&m| m >= 0).count(), q.orders, q.schedule);
    }
    let mut sim = Sim::new(world, seed);
    if has("--no-invasions") {
        sim.set_invasions(false);
    }
    let citizens0 = sim.world.citizens().count();
    if has("--force") {
        let n = sim.force_siege();
        println!("forced siege: {n} goblins arrived");
    }
    let hostiles_start = sim.hostiles().len();
    println!("hostiles at start: {hostiles_start}");
    let t1 = Instant::now();
    let mut done = 0u64;
    let mut end_tick = None;
    let mut deaths: Vec<String> = Vec::new();
    let mut combat_secs = 0.0;
    let mut combat_ticks = 0u64;
    while done < ticks {
        let fighting = !sim.hostiles().is_empty();
        let tc = Instant::now();
        sim.run_ticks(100);
        if fighting {
            combat_secs += tc.elapsed().as_secs_f64();
            combat_ticks += 100;
        }
        done += 100;
        for a in sim.drain_announcements() {
            if a.kind == AnnouncementKind::Death {
                deaths.push(format!("tick {}: {}", a.tick, a.text));
            } else if a.kind == AnnouncementKind::Warning && a.text.contains("vile") {
                println!("tick {}: {}", a.tick, a.text);
            }
        }
        if end_tick.is_none() && sim.hostiles().is_empty() {
            end_tick = Some(done);
            if !has("--full") {
                break;
            }
        }
        if done % 10_000 == 0 {
            let st = sim.combat_stats();
            eprintln!("  {done} ticks: {} hostiles left, {} citizens alive, {} attacks so far", sim.hostiles().len(), sim.world.citizens().count(), st.attacks);
        }
    }
    let el = t1.elapsed().as_secs_f64();
    let st = sim.combat_stats();
    let citizens = sim.world.citizens().count();
    let invaders_dead = sim.world.units.iter().filter(|u| u.flags.dead && (u.invader_civ >= 0 || u.flags.hostile)).count();
    let wounded = sim.world.citizens().filter(|u| u.wounds.iter().any(|w| !w.flags.iter().any(|f| f.starts_with("SCAR")))).count();
    println!(
        "ran {done} ticks in {el:.2}s = {:.0} ticks/s overall, {:.0} ticks/s while hostiles were on the map ({} ticks)",
        done as f64 / el.max(1e-9),
        combat_ticks as f64 / combat_secs.max(1e-9),
        combat_ticks
    );
    let left: Vec<String> = sim.hostiles().iter().filter_map(|id| sim.world.units.iter().find(|u| u.id == *id)).map(|u| format!("{} at z{}", u.race, u.pos.z)).collect();
    println!("hostiles left: {left:?}");
    match end_tick {
        Some(t) => println!("siege over after {t} ticks ({:.1} game days)", t as f64 / 1200.0),
        None => println!("siege still going after {done} ticks: {} hostiles left", sim.hostiles().len()),
    }
    println!(
        "citizens {citizens0} -> {citizens} ({} died) | invaders: {} killed, {} left the map | citizens with fresh wounds: {wounded}",
        citizens0.saturating_sub(citizens),
        st.invaders_killed.max(invaders_dead as u64),
        st.invaders_left
    );
    println!(
        "attacks {}, hits {}, misses {}, dodged {}, blocked {}, parried {}, deflected by armor {}, parts severed {}, knocked out {}, bled out {}",
        st.attacks, st.hits, st.misses, st.dodges, st.blocks, st.parries, st.deflected, st.severed, st.unconscious, st.bled_out
    );
    println!("deaths ({}):", deaths.len());
    let mut causes: std::collections::BTreeMap<String, usize> = Default::default();
    for d in &deaths {
        let c = d.split(" has ").nth(1).unwrap_or("?").to_string();
        *causes.entry(c).or_default() += 1;
    }
    println!("death causes: {causes:?}");
    for d in deaths.iter().take(25) {
        println!("  {d}");
    }
    let log = sim.combat_log();
    println!("combat log: {} lines; sample:", log.len());
    let step = (log.len() / nlog.max(1)).max(1);
    for l in log.iter().step_by(step).take(nlog) {
        println!("  {l}");
    }
    println!("state hash: {:016x}", sim.state_hash());
}
