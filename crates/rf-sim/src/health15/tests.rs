//! [health15] Unit tests on synthetic maps, one or more per rule (spec `docs/mechanics/health15.md`).

use super::disab::{crawl_delay, crutch_delay, Walk};
use super::supplies::want_from_units;
use super::*;
use crate::combat::{wf, WoundS};
use crate::health::{hf, Treat};
use rf_core::{Building, Item, ItemFlags, Location, LocationSupply, Pos, World};

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

fn slot(sim: &Sim, uid: i32) -> usize {
    sim.unit_slot[&uid]
}

/// Run until the next health15 pass has happened.
fn pass(sim: &mut Sim) {
    sim.run_ticks(100);
}

fn layer_of(sim: &Sim, s: usize, part: &str, tissue: &str) -> (u16, u16) {
    let b = &sim.combat.bodies[sim.combat.f[s].body as usize];
    let pi = b.parts.iter().position(|p| p.token == part).unwrap();
    let p = &b.parts[pi];
    let l = (p.l0..p.l0 + p.nl).find(|&l| b.layers[l as usize].tissue.eq_ignore_ascii_case(tissue)).unwrap();
    (pi as u16, l)
}

// ---------------------------------------------------------------- M1 hospitals

#[test]
fn meeting_zone_location_is_hospital() {
    let mut w = base_world();
    let h = w.units[0].pos;
    w.buildings.push(zone(900, "MeetingHall", "Hospital", Pos::new(h.x - 3, h.y - 3, h.z), Pos::new(h.x + 3, h.y + 3, h.z)));
    w.locations.push(Location { id: 7, kind: "Hospital".into(), name: "Hospital".into(), deity: -1, zones: vec![900], ..Default::default() });
    let mut sim = sim_of(w, 1);
    pass(&mut sim);
    assert_eq!(sim.h15.hosp.len(), 1, "{:?}", sim.h15.hosp);
    assert!(sim.combat.hospitals.contains(&(h.x - 3, h.y - 3, h.x + 3, h.y + 3, h.z)));
    assert_eq!(sim.hospitals()[0].key, 7);
}

#[test]
fn runtime_hospital_zone_is_used() {
    let mut w = base_world();
    let h = w.units[0].pos;
    w.buildings.push(zone(901, "MeetingHall", "Hall", Pos::new(h.x - 2, h.y - 2, h.z), Pos::new(h.x + 2, h.y + 2, h.z)));
    let mut sim = sim_of(w, 2);
    pass(&mut sim);
    assert!(sim.h15.hosp.is_empty());
    sim.create_location("Hospital", 901, "Infirmary");
    sim.run_ticks(rf_core::TICKS_PER_DAY as u32);
    assert_eq!(sim.h15.hosp.len(), 1);
    assert!(!sim.combat.hospitals.is_empty());
}

#[test]
fn desired_from_export_units() {
    assert_eq!(want_from_units(Sup::Thread, 75_000), 5);
    assert_eq!(want_from_units(Sup::Cloth, 50_000), 5);
    assert_eq!(want_from_units(Sup::Powder, 750), 5);
    assert_eq!(want_from_units(Sup::Soap, 151), 2);
    let mut w = base_world();
    let h = w.units[0].pos;
    w.buildings.push(zone(900, "MeetingHall", "Hospital", h, Pos::new(h.x + 2, h.y + 2, h.z)));
    w.locations.push(Location { id: 3, kind: "Hospital".into(), zones: vec![900], supplies: vec![LocationSupply { item: "thread".into(), desired: 30_000, count: 0 }], ..Default::default() });
    let mut sim = sim_of(w, 3);
    pass(&mut sim);
    assert_eq!(sim.h15.hosp[0].want[Sup::Thread as usize], 2);
    assert_eq!(sim.h15.hosp[0].want[Sup::Cloth as usize], SUP_DEFAULT[1]);
}

// ---------------------------------------------------------------- M2 chests

fn chest_world() -> World {
    let mut w = base_world();
    for u in w.units.iter_mut().take(7) {
        u.labors.push("HAUL_ITEM".into());
    }
    let h = w.units[0].pos;
    w.buildings.push(zone(900, "Hospital", "Hospital", Pos::new(h.x - 4, h.y + 3, h.z), Pos::new(h.x + 4, h.y + 5, h.z)));
    w.buildings.push(building(910, "Box", Pos::new(h.x, h.y + 4, h.z)));
    for k in 0..3 {
        w.items.push(item(6000 + k, "CLOTH", 7, Pos::new(h.x + 6, h.y - 2, h.z)));
    }
    w
}

#[test]
fn hospital_chest_is_stocked_to_desired() {
    let mut sim = sim_of(chest_world(), 4);
    pass(&mut sim);
    let key = sim.h15.hosp[0].key;
    sim.set_hospital_supply(key, Sup::Cloth as usize, 2);
    let chest = sim.h15.hosp[0].chests[0];
    let mut n = 0;
    for _ in 0..60 {
        sim.run_ticks(200);
        n = sim.world.items.iter().filter(|i| i.kind == "CLOTH" && sim.world.idx(i.pos) == Some(chest as usize) && i.holder < 0).count();
        if n >= 2 {
            break;
        }
    }
    assert_eq!(n, 2, "two cloths stocked; stock {:?}", sim.h15.stock);
    sim.run_ticks(3_000);
    let n2 = sim.world.items.iter().filter(|i| i.kind == "CLOTH" && sim.world.idx(i.pos) == Some(chest as usize)).count();
    assert_eq!(n2, 2, "not more than desired, and not hauled away again");
}

#[test]
fn no_chest_no_stocking() {
    let mut w = chest_world();
    w.buildings.retain(|b| b.kind != "Box");
    let mut sim = sim_of(w, 5);
    sim.run_ticks(2_000);
    assert!(sim.h15.stock.is_empty());
    assert_eq!(sim.h15.rep.stocked, 0);
}

#[test]
fn earmarked_cloth_not_used_by_production() {
    let mut sim = sim_of(chest_world(), 6);
    pass(&mut sim);
    let chest = sim.h15.hosp[0].chests[0];
    let p = sim.world.pos_of(chest as usize);
    let idx = sim.item_idx[&6000] as usize;
    sim.set_item_pos(6000, p);
    assert!(sim.h15_stored(idx), "cloth in a hospital chest is earmarked");
    assert!(sim.is_stored(idx), "and counts as stored (no hauling)");
    let site = chest;
    let c = sim.nav.comp_near(site as usize);
    let mut out = Vec::new();
    sim.find_items(site, c, sim.ticks, 5, &mut out, |s, i| s.world.items[i].kind == "CLOTH");
    assert!(!out.contains(&6000), "{out:?}");
}

#[test]
fn doctor_prefers_hospital_chest() {
    let mut sim = sim_of(chest_world(), 7);
    pass(&mut sim);
    let chest = sim.h15.hosp[0].chests[0];
    let p = sim.world.pos_of(chest as usize);
    sim.set_item_pos(6002, p);
    // Doctor next to the other cloths: still takes the one in the hospital.
    let s = slot(&sim, 1);
    let got = sim.care_find_supply(s, "CLOTH", sim.ticks);
    assert_eq!(got, Some(6002));
}

// ---------------------------------------------------------------- M3 consumption

fn doctor_world() -> World {
    let mut w = base_world();
    let hall = w.units[0].pos;
    for k in 0..3 {
        w.buildings.push(building(700 + k, "Bed", Pos::new(hall.x + k as i16, hall.y + 4, hall.z)));
    }
    let sp = Pos::new(hall.x + 6, hall.y, hall.z);
    for (k, kind) in ["THREAD", "THREAD", "CLOTH", "CLOTH", "CLOTH", "SPLINT", "SPLINT", "CRUTCH"].iter().enumerate() {
        w.items.push(item(5000 + k as i32, kind, 0, sp));
    }
    w.units[1].labors.extend(["DIAGNOSE", "BONE_SETTING", "SURGERY"].iter().map(|s| s.to_string()));
    w.units[2].labors.extend(["SUTURING", "DRESSING_WOUNDS", "FEED_WATER_CIVILIANS"].iter().map(|s| s.to_string()));
    for u in [1usize, 2] {
        for sk in ["SUTURE", "SET_BONE", "SURGERY", "DRESS_WOUNDS", "DIAGNOSE"] {
            w.units[u].skills.push(rf_core::Skill { id: sk.into(), rating: 20, xp: 0 });
        }
    }
    w
}

#[test]
fn suture_thread_stays_in_patient() {
    let mut w = doctor_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "UB".into(), layer: "MUSCLE".into(), flags: vec!["CUT".into(), "BLEEDING".into()], severity: 4000, pain: 20, bleeding: 3, age: 0 }];
    let mut sim = sim_of(w, 8);
    for _ in 0..200 {
        sim.run_ticks(100);
        if sim.health_report().sutures >= 1 {
            break;
        }
    }
    assert!(sim.health_report().sutures >= 1);
    let u = sim.world.unit(1).unwrap();
    let thread = u.inventory.iter().filter(|&&id| sim.world.item(id).map(|i| i.kind == "THREAD").unwrap_or(false)).count();
    assert_eq!(thread, 1, "the thread is sewn into the patient");
    assert!(sim.h15.rep.threads_sewn >= 1);
}

#[test]
fn soap_bar_used_up_after_ten_cleanings() {
    let mut w = base_world();
    let h = w.units[0].pos;
    w.materials.push(rf_core::MaterialInfo { token: "CREATURE:COW:SOAP".into(), name: "cow soap".into(), ..Default::default() });
    let m = (w.materials.len() - 1) as u16;
    let mut bar = item(7000, "BAR", m, Pos::new(h.x + 3, h.y, h.z));
    bar.stack = 2;
    w.items.push(bar);
    let mut sim = sim_of(w, 9);
    for _ in 0..10 {
        assert!(sim.h15_use_soap());
    }
    assert_eq!(sim.world.item(7000).unwrap().stack, 1, "one bar for ten cleanings");
    assert!(sim.h15_use_soap());
    assert_eq!(sim.h15.rep.soap_bars_used, 2);
}

// ---------------------------------------------------------------- M4/M5 fractures

#[test]
fn imported_fracture_flags_mapped() {
    let f: Vec<String> = ["BROKEN", "NEEDS_SETTING", "COMPOUND_FRACTURE"].iter().map(|s| s.to_string()).collect();
    let m = mx_of_flags(&f);
    assert_eq!(m & (mx::NEEDS_SET | mx::COMPOUND | mx::SEEN), mx::NEEDS_SET | mx::COMPOUND | mx::SEEN);
    let mut w = base_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "LUL".into(), layer: "BONE".into(), flags: vec!["BROKEN".into(), "OVERLAPPING_FRACTURE".into()], severity: 6000, pain: 10, bleeding: 0, age: 0 }];
    let mut sim = sim_of(w, 10);
    pass(&mut sim);
    let s = slot(&sim, 1);
    assert!(sim.combat.f[s].wounds.iter().any(|w| w.mx & mx::OVERLAP != 0), "{:?}", sim.combat.f[s].wounds);
}

#[test]
fn compound_when_skin_cut_with_break() {
    let mut w = base_world();
    w.units[0].wounds = vec![
        rf_core::Wound { body_part: "RLA".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 6000, pain: 10, bleeding: 0, age: 0 },
        rf_core::Wound { body_part: "RLA".into(), layer: "SKIN".into(), flags: vec!["CUT".into()], severity: 3000, pain: 10, bleeding: 1, age: 0 },
    ];
    let mut sim = sim_of(w, 11);
    pass(&mut sim);
    let s = slot(&sim, 1);
    let bw = sim.combat.f[s].wounds.iter().find(|w| w.flags & wf::BROKEN != 0).unwrap();
    assert!(bw.mx & mx::COMPOUND != 0 && bw.mx & mx::NEEDS_SET != 0, "{bw:?}");
    assert_eq!(sim.h15.rep.compound, 1);
}

#[test]
fn overlap_roll_deterministic_per_seed() {
    let run = |seed| {
        let mut w = base_world();
        for u in w.units.iter_mut().take(7) {
            u.wounds = vec![rf_core::Wound { body_part: "RUL".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 9000, pain: 10, bleeding: 0, age: 0 }];
        }
        let mut sim = sim_of(w, seed);
        pass(&mut sim);
        (0..7).map(|k| sim.combat.f[k].wounds.iter().any(|w| w.mx & mx::OVERLAP != 0)).collect::<Vec<bool>>()
    };
    assert_eq!(run(12), run(12));
}

#[test]
fn compound_fracture_needs_surgery_before_setting() {
    let mut sim = sim_of(doctor_world(), 13);
    let s = slot(&sim, 1);
    assert!(sim.h15_break_bone(1, "RLA", "compound"));
    for w in sim.combat.f[s].wounds.iter_mut() {
        w.flags |= hf::DIAGNOSED;
    }
    sim.health_refresh_daily();
    assert_eq!(sim.next_treat(s, false), Some(Treat::Surgery));
    let w = sim.combat.f[s].wounds.iter().find(|w| w.flags & wf::BROKEN != 0).copied().unwrap();
    assert!(!settable_now(&w));
}

#[test]
fn set_bone_uses_no_splint_immobilize_does() {
    let mut w = doctor_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "LLA".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 6000, pain: 30, bleeding: 0, age: 0 }];
    let mut sim = sim_of(w, 14);
    let mut done = false;
    for _ in 0..400 {
        sim.run_ticks(100);
        if sim.h15.rep.splints_applied >= 1 {
            done = true;
            break;
        }
    }
    assert!(done, "{:?} {:?}", sim.health_report(), sim.unit_medical(1));
    let s = slot(&sim, 1);
    let bw = sim.combat.f[s].wounds.iter().find(|w| w.flags & wf::BROKEN != 0).copied().unwrap();
    assert!(bw.flags & hf::SET != 0 && bw.mx & mx::SPLINT != 0, "{bw:?}");
    let held = sim.world.unit(1).unwrap().inventory.iter().filter(|&&id| sim.world.item(id).map(|i| i.kind == "SPLINT").unwrap_or(false)).count();
    assert_eq!(held, 1, "one splint strapped on; the other one still free");
    assert!(sim.world.item(5006).is_some() || sim.world.item(5005).is_some());
}

#[test]
fn unimmobilized_fracture_does_not_heal() {
    let sim = sim_of(base_world(), 15);
    let s = slot(&sim, 1);
    let (part, bone) = layer_of(&sim, s, "LUA", "BONE");
    let layers = &sim.combat.bodies[sim.combat.f[s].body as usize].layers;
    let mut w = WoundS { part, layer: bone, flags: wf::BROKEN, sev: 6000, mx: mx::SEEN | mx::NEEDS_SET, ..Default::default() };
    assert_eq!(scale_heal(100, &w, layers, true, true), 0, "displaced, not set");
    w.flags |= hf::SET;
    w.mx &= !mx::NEEDS_SET;
    assert_eq!(scale_heal(100, &w, layers, true, true), 10, "set: bone rate");
    w.mx |= mx::SPLINT;
    assert_eq!(scale_heal(0, &w, layers, false, false), 6, "splinted: heals out of bed");
}

// ---------------------------------------------------------------- M6 plaster and casts

#[test]
fn kiln_reaction_is_plaster_from_gypsum_into_bag() {
    use crate::industry::reactions::{Pr, Rg, REACTIONS};
    let rx = REACTIONS.iter().find(|r| r.key == "MAKE_PLASTER_POWDER").unwrap();
    assert_eq!(rx.reagents, &[Rg::Gypsum, Rg::EmptyBag]);
    assert_eq!(rx.product, Pr::Plaster);
    assert!(crate::production::RECIPES.iter().any(|r| r.reaction == "MAKE_PLASTER_POWDER" && r.workshop == "Kiln"));
}

#[test]
fn anhydrite_is_not_gypsum() {
    let mut w = base_world();
    let h = w.units[0].pos;
    w.materials.push(rf_core::MaterialInfo { token: "INORGANIC:GYPSUM".into(), name: "gypsum".into(), is_stone: true, tags: vec!["RC:GYPSUM".into()], ..Default::default() });
    w.materials.push(rf_core::MaterialInfo { token: "INORGANIC:ANHYDRITE".into(), name: "anhydrite".into(), is_stone: true, ..Default::default() });
    let n = w.materials.len() as u16;
    w.items.push(item(7100, "BOULDER", n - 2, h));
    w.items.push(item(7101, "BOULDER", n - 1, h));
    w.items.push(item(7102, "BAG", 3, h));
    let sim = sim_of(w, 16);
    assert!(sim.rx_is_gypsum(sim.item_idx[&7100] as usize));
    assert!(!sim.rx_is_gypsum(sim.item_idx[&7101] as usize));
    assert!(sim.rx_is_empty_bag(sim.item_idx[&7102] as usize));
}

#[test]
fn cast_applied_with_powder_bucket_water_cloth() {
    let mut w = doctor_world();
    let hall = w.units[0].pos;
    w.items.retain(|i| i.kind != "SPLINT");
    w.materials.push(rf_core::MaterialInfo { token: "INORGANIC:PLASTER".into(), name: "plaster".into(), is_stone: true, ..Default::default() });
    let pm = (w.materials.len() - 1) as u16;
    w.items.push(item(7200, "POWDER_MISC", pm, Pos::new(hall.x + 6, hall.y + 1, hall.z)));
    w.items.push(item(7201, "BUCKET", 3, Pos::new(hall.x + 6, hall.y + 2, hall.z)));
    w.units[0].wounds = vec![rf_core::Wound { body_part: "LLA".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 3000, pain: 30, bleeding: 0, age: 0 }];
    let mut sim = sim_of(w, 17);
    // A water source next to the hall (the synthetic fort has none).
    let wt = sim.world.idx(Pos::new(hall.x + 7, hall.y + 2, hall.z)).unwrap() as u32;
    sim.water.push(wt);
    let mut done = false;
    for _ in 0..400 {
        sim.run_ticks(100);
        if sim.h15.rep.casts_applied >= 1 {
            done = true;
            break;
        }
    }
    assert!(done, "{:?} {:?} cnt {:?}", sim.health15_report(), sim.unit_medical(1), sim.h15.cnt);
    assert!(sim.world.item(7200).is_none(), "powder used");
    let u = sim.world.unit(1).unwrap();
    assert!(u.inventory.iter().any(|&id| sim.world.item(id).map(|i| i.kind == "ORTHOPEDIC_CAST").unwrap_or(false)));
}

#[test]
fn cast_falls_back_to_splint_without_powder() {
    let mut w = doctor_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "LLA".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 3000, pain: 30, bleeding: 0, age: 0 }];
    let mut sim = sim_of(w, 18);
    for _ in 0..400 {
        sim.run_ticks(100);
        if sim.h15.rep.splints_applied >= 1 {
            break;
        }
    }
    assert_eq!(sim.h15.rep.casts_applied, 0);
    assert!(sim.h15.rep.splints_applied >= 1);
}

// ---------------------------------------------------------------- M7 traction

fn traction_world(benches: usize, patients: usize) -> World {
    let mut w = doctor_world();
    let h = w.units[0].pos;
    w.buildings.push(zone(900, "Hospital", "Hospital", Pos::new(h.x - 4, h.y + 3, h.z), Pos::new(h.x + 4, h.y + 5, h.z)));
    for k in 0..benches {
        w.buildings.push(building(920 + k as i32, "TractionBench", Pos::new(h.x - 3 + k as i16, h.y + 5, h.z)));
    }
    let _ = patients;
    w
}

fn break_overlap(sim: &mut Sim, uid: i32) {
    assert!(sim.h15_break_bone(uid, "RLL", "overlapping"));
}

#[test]
fn overlap_without_bench_never_heals() {
    let sim = sim_of(base_world(), 19);
    let s = slot(&sim, 1);
    let (part, bone) = layer_of(&sim, s, "RLL", "BONE");
    let layers = &sim.combat.bodies[sim.combat.f[s].body as usize].layers;
    let mut w = WoundS { part, layer: bone, flags: wf::BROKEN | hf::SET, sev: 4000, mx: mx::SEEN | mx::OVERLAP | mx::SPLINT, ..Default::default() };
    assert_eq!(scale_heal(100, &w, layers, true, true), 0);
    assert!(!immobilized(&w));
    w.mx |= mx::TRACTION;
    assert_eq!(scale_heal(100, &w, layers, true, true), 10);
}

#[test]
fn overlap_patient_placed_in_traction_and_released() {
    let mut sim = sim_of(traction_world(1, 1), 20);
    break_overlap(&mut sim, 1);
    let s = slot(&sim, 1);
    let mut placed = false;
    for _ in 0..400 {
        sim.run_ticks(100);
        if sim.h15.rep.placed_in_traction >= 1 {
            placed = true;
            break;
        }
    }
    assert!(placed, "{:?} {:?} jobs {:?}", sim.health15_report(), sim.unit_medical(1), sim.health.jobs);
    assert!(sim.h15_in_traction(s));
    assert_eq!(sim.ai[s].pos, sim.h15.benches[0].tile);
    sim.run_ticks(1_000);
    assert!(sim.h15_in_traction(s), "stays in the bench");
    assert_eq!(sim.h15.rep.rebroken, 0);
    // Healed: released.
    sim.combat.f[s].wounds.retain(|w| w.flags & wf::BROKEN == 0);
    pass(&mut sim);
    assert_eq!(sim.h15.benches[0].occupant, crate::NONE);
    assert_eq!(sim.h15.rep.released_from_traction, 1);
}

#[test]
fn bench_removed_rebreaks_bone() {
    let mut sim = sim_of(traction_world(1, 1), 21);
    break_overlap(&mut sim, 1);
    let s = slot(&sim, 1);
    for _ in 0..400 {
        sim.run_ticks(100);
        if sim.h15.rep.placed_in_traction >= 1 {
            break;
        }
    }
    assert!(sim.h15_in_traction(s));
    sim.world.buildings.retain(|b| b.kind != "TractionBench");
    sim.h15_rebuild_hospitals();
    assert_eq!(sim.h15.rep.rebroken, 1);
    let w = sim.combat.f[s].wounds.iter().find(|w| w.mx & mx::OVERLAP != 0).unwrap();
    assert_eq!(w.sev, 10_000);
    assert!(!sim.h15_in_traction(s));
}

#[test]
fn one_patient_per_bench() {
    let mut sim = sim_of(traction_world(1, 2), 22);
    break_overlap(&mut sim, 1);
    break_overlap(&mut sim, 4);
    for _ in 0..600 {
        sim.run_ticks(100);
    }
    let n = [1, 4].iter().filter(|&&u| sim.h15_in_traction(slot(&sim, u))).count();
    assert_eq!(n, 1);
}

// ---------------------------------------------------------------- M8 healing rates

#[test]
fn bone_heals_ten_times_slower_than_muscle() {
    let sim = sim_of(base_world(), 23);
    let s = slot(&sim, 1);
    let (part, bone) = layer_of(&sim, s, "LUA", "BONE");
    let (_, muscle) = layer_of(&sim, s, "LUA", "MUSCLE");
    let layers = &sim.combat.bodies[sim.combat.f[s].body as usize].layers;
    let m = WoundS { part, layer: muscle, flags: wf::BRUISED, sev: 3000, mx: mx::SEEN, ..Default::default() };
    let b = WoundS { part, layer: bone, flags: wf::BRUISED, sev: 3000, mx: mx::SEEN, ..Default::default() };
    assert_eq!(scale_heal(100, &m, layers, true, true), 100);
    assert_eq!(scale_heal(100, &b, layers, true, true), 10);
    assert_eq!(heal_of_tissue("BONE"), 1000);
    assert_eq!(heal_of_tissue("NERVE"), NO_HEAL);
    assert_eq!(heal_of_tissue("HAIR"), 100);
}

#[test]
fn nerve_wound_never_heals_becomes_permanent() {
    let mut w = base_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "BRAIN".into(), layer: "BRAIN".into(), flags: vec!["BRUISED".into()], severity: 1500, pain: 5, bleeding: 0, age: 0 }];
    let mut sim = sim_of(w, 24);
    pass(&mut sim);
    let s = slot(&sim, 1);
    assert!(sim.combat.f[s].wounds.iter().all(|w| sim.combat.f[s].body == u16::MAX || w.part != layer_of(&sim, s, "BRAIN", "BRAIN").0), "left the active wounds");
    assert!(sim.combat.f[s].scars.iter().any(|w| w.flags.iter().any(|f| f == "PERMANENT")));
    assert_eq!(sim.h15.rep.permanent, 1);
    assert_eq!(sim.unit_medical(1).permanent, 1);
}

// ---------------------------------------------------------------- M9 nerves

#[test]
fn imported_motor_nerve_scar_disables_hand() {
    let mut w = base_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "RH".into(), layer: "MUSCLE".into(), flags: vec!["CUT".into(), "SCAR_CUT".into(), "MOTOR_NERVE_SEVERED".into()], severity: 0, pain: 0, bleeding: 0, age: 9_000_000 }];
    let mut sim = sim_of(w, 25);
    pass(&mut sim);
    let s = slot(&sim, 1);
    let (rh, _) = layer_of(&sim, s, "RH", "MUSCLE");
    assert!(sim.combat.f[s].part_state[rh as usize] & 4 != 0);
    assert_eq!(sim.unit_medical(1).motor, vec!["right hand".to_string()]);
    assert!(sim.grasp_ok(s), "the left hand still grasps");
}

#[test]
fn sensory_nerve_part_feels_no_pain() {
    let mut w = base_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "LLA".into(), layer: "SKIN".into(), flags: vec!["CUT".into()], severity: 2000, pain: 30, bleeding: 0, age: 0 }];
    let mut sim = sim_of(w, 26);
    assert!(sim.h15_sever_nerve(1, "LUA", false));
    let s = slot(&sim, 1);
    assert!(sim.combat.f[s].wounds.iter().all(|w| w.pain == 0), "below the damaged nerve: no pain");
    pass(&mut sim);
    assert!(sim.combat.f[s].wounds.iter().all(|w| w.pain <= 1), "heal_pass keeps 1 until the next health15 pass");
}

#[test]
fn nerve_roll_deterministic() {
    let run = |seed| {
        let mut w = base_world();
        for u in w.units.iter_mut().take(7) {
            u.wounds = vec![rf_core::Wound { body_part: "RUA".into(), layer: "MUSCLE".into(), flags: vec!["CUT".into()], severity: 9000, pain: 10, bleeding: 0, age: 0 }];
        }
        let mut sim = sim_of(w, seed);
        pass(&mut sim);
        (sim.h15.rep.motor_nerves, sim.h15.rep.sensory_nerves, sim.h15.dis.iter().map(|d| d.slot).collect::<Vec<_>>())
    };
    assert_eq!(run(27), run(27));
}

// ---------------------------------------------------------------- M10 crutches

fn crutch_world() -> World {
    let mut w = doctor_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "LF".into(), layer: "".into(), flags: vec!["SEVERED".into()], severity: 10_000, pain: 5, bleeding: 0, age: 50_000 }];
    w
}

#[test]
fn lost_foot_gets_crutch_and_walks_slow() {
    let mut sim = sim_of(crutch_world(), 28);
    let s = slot(&sim, 1);
    pass(&mut sim);
    assert!(matches!(sim.h15_walk(s), Some((Walk::Crawl, true, true))), "{:?}", sim.h15_walk(s));
    let mut got = false;
    for _ in 0..300 {
        sim.run_ticks(100);
        if sim.h15.rep.crutches_given >= 1 {
            got = true;
            break;
        }
    }
    assert!(got, "{:?}", sim.health15_report());
    pass(&mut sim);
    assert!(matches!(sim.h15_walk(s), Some((Walk::Crutch, true, true))));
    assert_eq!(sim.ai[s].speed, 1800);
    assert_eq!(sim.unit_medical(1).crutch, Some(5007));
}

#[test]
fn legendary_crutch_walker_not_slowed() {
    assert_eq!(crutch_delay(900, 20), 900);
    assert_eq!(crutch_delay(900, 0), 1800);
    assert_eq!(crutch_delay(900, 10), 1350);
    let mut w = crutch_world();
    w.units[0].skills.push(rf_core::Skill { id: "CRUTCH_WALK".into(), rating: 20, xp: 0 });
    let mut sim = sim_of(w, 29);
    for _ in 0..300 {
        sim.run_ticks(100);
        if sim.h15.rep.crutches_given >= 1 {
            break;
        }
    }
    pass(&mut sim);
    let s = slot(&sim, 1);
    let expect = crutch_delay(900, sim.h15_rating(s, "CRUTCH_WALK"));
    assert_eq!(sim.ai[s].speed, expect);
    assert!(expect <= 1000);
}

#[test]
fn no_crutch_crawls_at_gait_ratio() {
    let mut w = crutch_world();
    w.items.retain(|i| i.kind != "CRUTCH");
    let mut sim = sim_of(w, 30);
    sim.run_ticks(300);
    let s = slot(&sim, 1);
    assert_eq!(crawl_delay(900), 2990);
    assert_eq!(sim.ai[s].speed, 2990);
    assert!(sim.combat.f[s].crawl);
}

#[test]
fn spinal_paralysis_gets_no_crutch() {
    let mut sim = sim_of(crutch_world(), 31);
    let s = slot(&sim, 1);
    sim.combat.f[s].paralyzed = true;
    pass(&mut sim);
    assert!(matches!(sim.h15_walk(s), Some((Walk::Crawl, true, false))));
    assert!(!sim.h15_wants_crutch(s));
}

#[test]
fn motor_nerve_in_leg_needs_crutch() {
    let mut sim = sim_of(base_world(), 32);
    assert!(sim.h15_sever_nerve(1, "LLL", true));
    let s = slot(&sim, 1);
    assert!(matches!(sim.h15_walk(s), Some((Walk::Crawl, true, true))));
    let um = sim.unit_medical(1);
    assert!(um.needs_crutch && um.tiers.iter().any(|t| t.1 == "FUNCTION LOSS"), "{um:?}");
}

#[test]
fn crutch_dropped_after_recovery() {
    let mut sim = sim_of(crutch_world(), 33);
    for _ in 0..300 {
        sim.run_ticks(100);
        if sim.h15.rep.crutches_given >= 1 {
            break;
        }
    }
    let s = slot(&sim, 1);
    assert!(sim.h15.rep.crutches_given >= 1);
    // A miracle: the foot is back.
    sim.combat.f[s].wounds.clear();
    for p in sim.combat.f[s].part_state.iter_mut() {
        *p = 0;
    }
    sim.run_ticks(rf_core::TICKS_PER_DAY as u32 + 300);
    assert_eq!(sim.h15.rep.crutches_returned, 1);
    assert!(sim.world.item(5007).map(|i| i.holder < 0).unwrap_or(false));
    assert_eq!(sim.ai[s].speed, 900);
}

// ---------------------------------------------------------------- M11/M12 outcomes

#[test]
fn success_pct_table() {
    assert_eq!(success_pct(Treat::Suture, 0), 70);
    assert_eq!(success_pct(Treat::Suture, 20), 100);
    assert_eq!(success_pct(Treat::SetBone, 0), 60);
    assert_eq!(success_pct(Treat::SetBone, 20), 100);
    assert_eq!(success_pct(Treat::Surgery, 0), 50);
    assert_eq!(success_pct(Treat::Surgery, 10), 75);
    assert_eq!(success_pct(Treat::Dress, 0), 100);
    assert_eq!(success_pct(Treat::Diagnose, 0), 100);
}

#[test]
fn legendary_surgeon_beats_novice() {
    let mut w = base_world();
    w.units[1].skills.push(rf_core::Skill { id: "SURGERY".into(), rating: 20, xp: 0 });
    let h = w.units[0].pos;
    let _ = h;
    w.buildings.push(zone(900, "Hospital", "Hospital", Pos::new(0, 0, h.z), Pos::new(47, 47, h.z)));
    w.buildings.push(building(930, "Table", Pos::new(h.x + 1, h.y + 1, h.z)));
    let mut sim = sim_of(w, 34);
    pass(&mut sim);
    let (pro, novice, p) = (slot(&sim, 2), slot(&sim, 3), slot(&sim, 1));
    assert!(sim.h15_on_table(p));
    let (mut a, mut b) = (0, 0);
    for t in 0..400u64 {
        a += sim.h15_outcome(pro, p, Treat::Surgery, 1000 + t) as u32;
        b += sim.h15_outcome(novice, p, Treat::Surgery, 1000 + t) as u32;
    }
    assert!(a > b + 80, "legendary {a} vs novice {b}");
}

#[test]
fn surgery_in_hospital_without_table_uses_half_skill() {
    let mut w = base_world();
    w.units[1].skills.push(rf_core::Skill { id: "SURGERY".into(), rating: 20, xp: 0 });
    let h = w.units[0].pos;
    let _ = h;
    w.buildings.push(zone(900, "Hospital", "Hospital", Pos::new(0, 0, h.z), Pos::new(47, 47, h.z)));
    let mut sim = sim_of(w, 35);
    pass(&mut sim);
    let (pro, p) = (slot(&sim, 2), slot(&sim, 1));
    assert!(!sim.h15_on_table(p));
    let r = sim.h15_rating(pro, "SURGERY");
    let full = success_pct(Treat::Surgery, r);
    let half = success_pct(Treat::Surgery, r / 2);
    let n: u32 = (0..1000u64).map(|t| sim.h15_outcome(pro, p, Treat::Surgery, 5000 + t) as u32).sum();
    let pct = n / 10;
    assert!(pct + 6 >= half && pct <= half + 6 && half < full, "{pct}% vs half {half} full {full}");
}

#[test]
fn surgery_clears_infection_on_success() {
    let mut sim = sim_of(base_world(), 36);
    let s = slot(&sim, 1);
    let (part, skin) = layer_of(&sim, s, "UB", "SKIN");
    sim.combat.f[s].wounds.push(WoundS { part, layer: skin, flags: wf::CUT | hf::INFECTED, sev: 4000, age: 10 * rf_core::TICKS_PER_DAY, mx: mx::SEEN, ..Default::default() });
    let w = sim.combat.f[s].wounds[0];
    assert!(needs_surgery(&w, 0));
    sim.h15_surgery_done(slot(&sim, 2), s, 777);
    let w = sim.combat.f[s].wounds[0];
    assert!(w.flags & hf::OPERATED != 0 && w.sev == 3000);
    assert_eq!(sim.h15.rep.infections_excised, 1);
}

// ---------------------------------------------------------------- M13 / saves

#[test]
fn imported_infected_scars_stay_dormant() {
    let mut w = base_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "UB".into(), layer: "SKIN".into(), flags: vec!["CUT".into(), "SCAR_CUT".into(), "INFECTION".into()], severity: 0, pain: 0, bleeding: 0, age: 9_700_000 }];
    let mut sim = sim_of(w, 37);
    sim.run_ticks(2_000);
    let s = slot(&sim, 1);
    assert!(sim.combat.f[s].wounds.is_empty(), "a scar, not an active infection");
    assert_eq!(sim.health_report().infections, 0);
}

#[test]
fn health15_state_roundtrips() {
    let mut sim = sim_of(crutch_world(), 38);
    sim.run_ticks(2_000);
    let a = rmp_serde::to_vec_named(&sim.h15).unwrap();
    let back: Health15 = rmp_serde::from_slice(&a).unwrap();
    let b = rmp_serde::to_vec_named(&back).unwrap();
    assert_eq!(a, b);
}

#[test]
fn health15_off_keeps_old_rules() {
    let mut w = base_world();
    w.units[0].wounds = vec![rf_core::Wound { body_part: "LLL".into(), layer: "BONE".into(), flags: vec!["BROKEN".into()], severity: 6000, pain: 30, bleeding: 0, age: 0 }];
    let mut sim = sim_of(w, 39);
    sim.set_health15_enabled(false);
    sim.run_ticks(300);
    let s = slot(&sim, 1);
    assert!(sim.combat.f[s].crawl);
    assert_eq!(sim.ai[s].speed, 900 * crate::health::CRAWL_FACTOR);
    assert!(sim.combat.f[s].wounds.iter().all(|w| w.mx == 0));
}

#[allow(dead_code)]
fn _uses(_: Walk) {}
