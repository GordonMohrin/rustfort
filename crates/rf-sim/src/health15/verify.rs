//! [health15] Adversarial verification tests (bugs found after the first implementation).

use super::disab::Walk;
use super::*;
use crate::combat::{wf, WoundS};
use crate::health::{hf, Treat};
use rf_core::{Building, Item, ItemFlags, Pos, World};

fn base_world() -> World {
    let mut w = World::synthetic(48, 48, 20);
    for u in w.units.iter_mut() {
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
    }
    w
}

fn sim_of(w: World, seed: u64) -> Sim {
    let mut s = Sim::new(w, seed);
    s.set_invasions(false);
    s.set_moods_enabled(false);
    s
}

fn building(id: i32, kind: &str, p: Pos) -> Building {
    Building { id, kind: kind.into(), x1: p.x, y1: p.y, x2: p.x, y2: p.y, z: p.z, center: p, built: true, owner: -1, ..Default::default() }
}

fn zone(id: i32, subtype: &str, name: &str, a: Pos, b: Pos) -> Building {
    Building { id, kind: "Civzone".into(), subtype: subtype.into(), name: name.into(), x1: a.x, y1: a.y, x2: b.x, y2: b.y, z: a.z, center: a, built: true, owner: -1, ..Default::default() }
}

fn item(id: i32, kind: &str, mat: u16, p: Pos) -> Item {
    Item { id, kind: kind.into(), mat, pos: p, stack: 1, container: -1, holder: -1, building: -1, flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() }
}

fn doctor_world() -> World {
    let mut w = base_world();
    let hall = w.units[0].pos;
    for k in 0..3 {
        w.buildings.push(building(700 + k, "Bed", Pos::new(hall.x + k as i16, hall.y + 4, hall.z)));
    }
    w.units[1].labors.extend(["DIAGNOSE", "BONE_SETTING", "SURGERY"].iter().map(|s| s.to_string()));
    w.units[2].labors.extend(["SUTURING", "DRESSING_WOUNDS", "FEED_WATER_CIVILIANS"].iter().map(|s| s.to_string()));
    w
}

/// health15 off: an old infected wound must not ask for surgery forever (the excision rule
/// belongs to health15; off = the round-5 rules).
#[test]
fn off_mode_no_endless_surgery_for_old_infection() {
    let mut sim = sim_of(doctor_world(), 41);
    sim.set_health15_enabled(false);
    let s = sim.unit_slot[&1];
    sim.ensure_body(s);
    sim.combat.f[s].wounds.push(WoundS { part: 0, layer: u16::MAX, flags: wf::CUT | hf::INFECTED | hf::DIAGNOSED | hf::SUTURED | hf::DRESSED | hf::CLEANED, sev: 1_000, age: 8 * rf_core::TICKS_PER_DAY, ..Default::default() });
    sim.list_wounded(s);
    sim.health_refresh_daily();
    assert_ne!(sim.next_treat(s, false), Some(Treat::Surgery));
}

/// Spec M5: only an immobilized broken STANCE *limb* needs a crutch; an immobilized torso
/// fracture lets the dwarf walk (old rule: set fractures stand).
#[test]
fn immobilized_torso_fracture_walks() {
    let mut sim = sim_of(base_world(), 42);
    let s = sim.unit_slot[&1];
    assert!(sim.h15_break_bone(1, "LB", "simple"));
    assert!(matches!(sim.h15_walk(s), Some((Walk::Crawl, _, _))), "loose torso fracture: crawl");
    for w in sim.combat.f[s].wounds.iter_mut() {
        w.mx |= mx::SPLINT;
    }
    assert!(matches!(sim.h15_walk(s), Some((Walk::Normal, false, _))), "{:?}", sim.h15_walk(s));
    // A splinted leg still needs a crutch.
    assert!(sim.h15_break_bone(1, "RLL", "simple"));
    for w in sim.combat.f[s].wounds.iter_mut() {
        w.mx |= mx::SPLINT;
    }
    assert!(matches!(sim.h15_walk(s), Some((_, true, _))), "{:?}", sim.h15_walk(s));
}

fn chest_world() -> World {
    let mut w = base_world();
    for u in w.units.iter_mut().take(7) {
        u.labors.push("HAUL_ITEM".into());
    }
    let h = w.units[0].pos;
    w.buildings.push(zone(900, "Hospital", "Hospital", Pos::new(h.x - 4, h.y + 3, h.z), Pos::new(h.x + 4, h.y + 5, h.z)));
    w.buildings.push(building(910, "Box", Pos::new(h.x, h.y + 4, h.z)));
    w
}

/// Unreachable supplies must not block the stocking of reachable ones.
#[test]
fn unreachable_supplies_do_not_block_stocking() {
    let mut w = chest_world();
    let h = w.units[0].pos;
    // Six cloths sealed in rock (lower ids: picked first), two reachable ones.
    for k in 0..6 {
        w.items.push(item(6100 + k, "CLOTH", 7, Pos::new(h.x + k as i16, h.y, h.z - 4)));
    }
    for k in 0..2 {
        w.items.push(item(6200 + k, "CLOTH", 7, Pos::new(h.x + 6, h.y - 2, h.z)));
    }
    let mut sim = sim_of(w, 43);
    sim.run_ticks(100);
    let key = sim.h15.hosp[0].key;
    sim.set_hospital_supply(key, Sup::Cloth as usize, 2);
    let chest = sim.h15.hosp[0].chests[0] as usize;
    let mut n = 0;
    for _ in 0..80 {
        sim.run_ticks(200);
        n = sim.world.items.iter().filter(|i| i.kind == "CLOTH" && sim.world.idx(i.pos) == Some(chest) && i.holder < 0).count();
        if n >= 2 {
            break;
        }
    }
    assert_eq!(n, 2, "stock {:?}", sim.h15.stock);
}

/// Supplies already in the chest when the game starts count from the first day.
#[test]
fn existing_chest_stock_counts_at_start() {
    let mut w = chest_world();
    let h = w.units[0].pos;
    let chest = Pos::new(h.x, h.y + 4, h.z);
    for k in 0..5 {
        w.items.push(item(6300 + k, "CLOTH", 7, chest));
    }
    for k in 0..5 {
        w.items.push(item(6400 + k, "CLOTH", 7, Pos::new(h.x + 6, h.y - 2, h.z)));
    }
    let mut sim = sim_of(w, 44);
    sim.run_ticks(1_000);
    assert_eq!(sim.h15.hosp[0].have[Sup::Cloth as usize], 5);
    assert!(sim.h15.stock.iter().all(|e| sim.world.item(e.0).map(|i| i.kind != "CLOTH").unwrap_or(true)), "no cloth hauled: {:?}", sim.h15.stock);
}

/// A patient in traction stays there over a long time with normal needs (fed in bed).
#[test]
fn traction_patient_stays_for_long() {
    let mut w = doctor_world();
    let h = w.units[0].pos;
    w.buildings.push(zone(900, "Hospital", "Hospital", Pos::new(h.x - 4, h.y + 3, h.z), Pos::new(h.x + 4, h.y + 5, h.z)));
    w.buildings.push(building(920, "TractionBench", Pos::new(h.x - 3, h.y + 5, h.z)));
    let sp = Pos::new(h.x + 6, h.y, h.z);
    for k in 0..6 {
        w.items.push(item(5100 + k, if k < 3 { "CLOTH" } else { "SPLINT" }, 0, sp));
    }
    let mut sim = sim_of(w, 45);
    assert!(sim.h15_break_bone(1, "RLL", "overlapping"));
    let s = sim.unit_slot[&1];
    for _ in 0..400 {
        sim.run_ticks(100);
        if sim.h15.rep.placed_in_traction >= 1 {
            break;
        }
    }
    assert!(sim.h15_in_traction(s));
    sim.run_ticks(40_000);
    assert_eq!(sim.h15.rep.rebroken, 0, "{:?} sub {:?} thirst {}", sim.health15_report().events, sim.combat.f[s].sub, sim.ai[s].thirst);
}
