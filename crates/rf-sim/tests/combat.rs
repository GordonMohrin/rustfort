//! Combat and military: arena fights, a goblin siege against armored axedwarves, determinism.
//!
//! Run with `cargo test -p rf-sim --release --test combat -- --nocapture` to see the outcome
//! reports and sample combat log lines.

use rf_core::{Pos, World};
use rf_sim::{AnnouncementKind, Sim, SquadOrder};
use std::time::Instant;

const DZ: u16 = 20;

fn surface_z() -> i16 {
    (DZ as i16 * 2 / 3).max(2)
}

fn print_log(sim: &Sim, n: usize) {
    let log = sim.combat_log();
    println!("--- combat log ({} lines, sample) ---", log.len());
    for l in log.iter().take(n) {
        println!("  {l}");
    }
    if log.len() > n {
        println!("  ...");
        for l in log.iter().skip(log.len().saturating_sub(5)) {
            println!("  {l}");
        }
    }
}

/// (a) One unarmed dwarf (ordered to kill) vs one hostile cat.
fn dwarf_vs_cat(seed: u64) -> (Sim, u64) {
    let mut w = World::synthetic(48, 48, DZ);
    let cat_pos = w.units.iter().find(|u| u.race == "CAT").unwrap().pos;
    w.units.retain(|u| u.id == 1 || u.race == "CAT");
    w.units[0].pos = cat_pos.offset(-3, 0, 0);
    w.units[0].thirst_timer = 0;
    w.units[0].hunger_timer = 0;
    w.units[0].sleepiness_timer = 0;
    let mut sim = Sim::new(w, seed);
    sim.set_invasions(false);
    let q = sim.create_squad("Arena");
    sim.assign_squad_member(q, 0, 1);
    sim.order_squad(q, SquadOrder::Kill(100));
    sim.make_hostile(100);
    let mut t = 0;
    while t < 30_000 {
        sim.run_ticks(100);
        t += 100;
        let dwarf_dead = sim.world.units.iter().find(|u| u.id == 1).unwrap().flags.dead;
        let cat_dead = sim.world.units.iter().find(|u| u.id == 100).unwrap().flags.dead;
        if dwarf_dead || cat_dead {
            break;
        }
    }
    (sim, t)
}

#[test]
fn unarmed_dwarf_vs_cat() {
    let t0 = Instant::now();
    let (sim, ticks) = dwarf_vs_cat(7);
    let st = sim.combat_stats();
    let dwarf = sim.world.units.iter().find(|u| u.id == 1).unwrap();
    let cat = sim.world.units.iter().find(|u| u.id == 100).unwrap();
    println!(
        "dwarf vs cat: {ticks} ticks ({:.2}s), dwarf {} ({} wounds, blood {}/{}), cat {} ({} wounds, blood {}/{}); attacks {}, hits {}, dodges {}, misses {}",
        t0.elapsed().as_secs_f64(),
        if dwarf.flags.dead { "dead" } else { "alive" },
        dwarf.wounds.len(),
        dwarf.blood,
        dwarf.blood_max,
        if cat.flags.dead { "dead" } else { "alive" },
        cat.wounds.len(),
        cat.blood,
        cat.blood_max,
        st.attacks,
        st.hits,
        st.dodges,
        st.misses
    );
    print_log(&sim, 12);
    assert!(st.attacks > 0, "they fought");
    assert!(st.hits > 0);
    assert!(dwarf.flags.dead || cat.flags.dead, "the fight ends with a death");
    // A 60 000 cm³ dwarf against a 5 000 cm³ cat: the dwarf wins.
    assert!(cat.flags.dead && !dwarf.flags.dead, "the dwarf should win");
}

/// (b) 10 iron-armored axedwarves (one squad, training) vs a siege of 20 goblins.
fn axedwarves_vs_goblins(seed: u64) -> (Sim, u64, f64) {
    let mut w = World::synthetic(64, 64, DZ);
    let sz = surface_z();
    let template = w.units[0].clone();
    w.units.retain(|u| u.race == "DWARF");
    let n0 = w.units.len();
    for k in n0..10 {
        let mut u = template.clone();
        u.id = 200 + k as i32;
        u.name = format!("Axedwarf {k}");
        w.units.push(u);
    }
    for (k, u) in w.units.iter_mut().enumerate() {
        u.pos = Pos::new(28 + (k % 5) as i16, 30 + (k / 5) as i16, sz);
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
        u.phys_attrs = vec![("STRENGTH".into(), 1250), ("AGILITY".into(), 1000), ("TOUGHNESS".into(), 1250)];
        u.skills.push(rf_core::Skill { id: "AXE".into(), rating: 5, xp: 0 });
        u.skills.push(rf_core::Skill { id: "DODGING".into(), rating: 3, xp: 0 });
        u.skills.push(rf_core::Skill { id: "ARMOR".into(), rating: 3, xp: 0 });
        u.profession = "AXEMAN".into();
    }
    let ids: Vec<i32> = w.units.iter().map(|u| u.id).collect();
    let mut sim = Sim::new(w, seed);
    sim.set_invasions(false);
    let q = sim.create_squad("Axe Lords");
    for (k, &id) in ids.iter().enumerate() {
        sim.assign_squad_member(q, k, id);
        sim.give_item(id, "WEAPON", "ITEM_WEAPON_AXE_BATTLE", "INORGANIC:IRON", 0);
        sim.give_item(id, "ARMOR", "ITEM_ARMOR_BREASTPLATE", "INORGANIC:IRON", 0);
        sim.give_item(id, "ARMOR", "ITEM_ARMOR_MAIL_SHIRT", "INORGANIC:IRON", 0);
        sim.give_item(id, "HELM", "ITEM_HELM_HELM", "INORGANIC:IRON", 0);
        sim.give_item(id, "PANTS", "ITEM_PANTS_GREAVES", "INORGANIC:IRON", 0);
        sim.give_item(id, "GLOVES", "ITEM_GLOVES_GAUNTLETS", "INORGANIC:IRON", 0);
        sim.give_item(id, "SHOES", "ITEM_SHOES_BOOTS", "INORGANIC:IRON", 0);
    }
    sim.order_squad(q, SquadOrder::Train);
    let spawned = sim.spawn_siege("GOBLIN", 20);
    assert_eq!(spawned, 20);
    let t0 = Instant::now();
    let mut t = 0u64;
    while t < 40_000 && sim.siege_active() {
        sim.run_ticks(100);
        t += 100;
    }
    (sim, t, t0.elapsed().as_secs_f64())
}

#[test]
fn armored_axedwarves_repel_goblin_siege() {
    let (mut sim, ticks, secs) = axedwarves_vs_goblins(3);
    let st = sim.combat_stats();
    let dwarves_dead = sim.world.units.iter().filter(|u| u.race == "DWARF" && u.flags.dead).count();
    let goblins_dead = sim.world.units.iter().filter(|u| u.race == "GOBLIN" && u.flags.dead).count();
    let wounded = sim.world.units.iter().filter(|u| u.race == "DWARF" && !u.flags.dead && !u.wounds.is_empty()).count();
    println!(
        "10 axedwarves vs 20 goblins: {ticks} ticks in {secs:.2}s ({:.0} ticks/s); dwarves dead {dwarves_dead}, wounded {wounded}; goblins dead {goblins_dead}, fled {}; attacks {}, hits {}, deflected {}, severed {}, dodged {}, parried {}",
        ticks as f64 / secs.max(1e-9),
        st.invaders_left,
        st.attacks,
        st.hits,
        st.deflected,
        st.severed,
        st.dodges,
        st.parries
    );
    print_log(&sim, 25);
    let lg = sim.combat_log();
    for i in 1..lg.len() {
        if lg[i].contains("struck down") || lg[i].contains("bled to death") {
            println!("killing blow: {}  <=  {}", lg[i], lg[i - 1]);
        }
    }
    let deaths: Vec<String> = sim.drain_announcements().into_iter().filter(|a| a.kind == AnnouncementKind::Death).map(|a| a.text).collect();
    println!("deaths: {deaths:?}");
    assert!(st.attacks > 50);
    assert!(goblins_dead >= 10, "the axedwarves should kill most goblins");
    assert!(dwarves_dead <= 5, "iron armor should protect most dwarves");
    assert!(!sim.siege_active(), "the siege should be over");
    assert!(sim.world.items.iter().any(|i| i.kind == "CORPSE"), "corpses are left");
}

#[test]
fn combat_is_deterministic_across_threads() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let (mut sim, _, _) = {
                let mut w = World::synthetic(64, 64, DZ);
                let sz = surface_z();
                for (k, u) in w.units.iter_mut().enumerate() {
                    u.pos = Pos::new(28 + k as i16, 30, sz);
                }
                let mut sim = Sim::new(w, 11);
                sim.set_parallel_threshold(1);
                sim.set_invasions(false);
                sim.spawn_siege("GOBLIN", 12);
                (sim, 0u64, 0.0f64)
            };
            sim.run_ticks(6000);
            (sim.state_hash(), sim.combat_log().len(), sim.combat_stats().hits)
        })
    };
    let a = run(1);
    let b = run(4);
    println!("determinism: {a:?} vs {b:?}");
    assert!(a.2 > 0, "fighting happened");
    assert_eq!(a, b, "1 thread vs 4 threads");
}

#[test]
fn civilians_flee_and_alert_burrow() {
    let mut w = World::synthetic(64, 64, DZ);
    let sz = surface_z();
    for (k, u) in w.units.iter_mut().enumerate() {
        if u.race == "DWARF" {
            u.pos = Pos::new(30 + k as i16, 30, sz);
        }
    }
    let mut sim = Sim::new(w, 5);
    sim.set_invasions(false);
    let b = sim.create_burrow("Safe", Pos::new(5, 5, sz), Pos::new(9, 9, sz));
    sim.set_civilian_alert(Some(b));
    sim.run_ticks(1500);
    let inside = sim.world.units.iter().filter(|u| u.race == "DWARF" && u.pos.x >= 5 && u.pos.x <= 9 && u.pos.y >= 5 && u.pos.y <= 9 && u.pos.z == sz).count();
    println!("civilian alert: {inside} of 7 dwarves inside the burrow after 1500 ticks");
    assert!(inside >= 5);
    sim.set_civilian_alert(None);
    let n = sim.spawn_siege("GOBLIN", 3);
    assert_eq!(n, 3);
    assert_eq!(sim.hostiles().len(), 3);
    sim.run_ticks(3000);
    let st = sim.combat_stats();
    println!("unarmed civilians vs 3 goblins: fort deaths {}, attacks {}", st.fort_deaths, st.attacks);
    assert!(st.attacks > 0, "goblins reach the civilians");
}
