//! Ranged combat: marksdwarves vs goblins, out-of-ammo melee, archery practice, line of fire.
//!
//! Run with `cargo test -p rf-sim --release --test ranged -- --nocapture` for the reports.

use rf_core::{Building, Pos, Shape, TiletypeInfo, TiletypeTable, TtMaterial, TtSpecial, World};
use rf_sim::{Sim, SquadOrder};

const DZ: u16 = 20;

fn surface_z() -> i16 {
    (DZ as i16 * 2 / 3).max(2)
}

fn hall_z() -> i16 {
    (surface_z() - 4).max(0)
}

fn print_log(sim: &Sim, filter: &str, n: usize) {
    let log = sim.combat_log();
    let lines: Vec<&String> = log.iter().filter(|l| l.contains(filter)).collect();
    println!("--- combat log: {} lines, {} with {filter:?} ---", log.len(), lines.len());
    for l in lines.iter().take(n) {
        println!("  {l}");
    }
}

/// Dwarves `ids` become marksdwarves: crossbow, quiver with `bolts` iron bolts, iron armor.
fn arm_marksdwarves(sim: &mut Sim, ids: &[i32], bolts: i32, armor: bool, bolt_mat: &str) {
    for &id in ids {
        sim.give_item(id, "WEAPON", "ITEM_WEAPON_CROSSBOW", "PLANT:OAK:WOOD", 0);
        sim.give_item(id, "QUIVER", "", "CREATURE:COW:LEATHER", 0);
        if bolts > 0 {
            let b = sim.give_item(id, "AMMO", "ITEM_AMMO_BOLTS", bolt_mat, 0).unwrap();
            sim.world.items.iter_mut().find(|i| i.id == b).unwrap().stack = bolts;
        }
        if armor {
            sim.give_item(id, "ARMOR", "ITEM_ARMOR_BREASTPLATE", "INORGANIC:IRON", 0);
            sim.give_item(id, "HELM", "ITEM_HELM_HELM", "INORGANIC:IRON", 0);
            sim.give_item(id, "PANTS", "ITEM_PANTS_GREAVES", "INORGANIC:IRON", 0);
        }
    }
}

fn marksdwarf_world(n: usize, size: u16) -> (World, Vec<i32>) {
    let mut w = World::synthetic(size, size, DZ);
    let sz = surface_z();
    let template = w.units[0].clone();
    w.units.retain(|u| u.race == "DWARF");
    w.units.truncate(n);
    while w.units.len() < n {
        let mut u = template.clone();
        u.id = 300 + w.units.len() as i32;
        w.units.push(u);
    }
    let c = size as i16 / 2;
    for (k, u) in w.units.iter_mut().enumerate() {
        u.name = format!("Marksdwarf {k}");
        u.pos = Pos::new(c - 2 + k as i16, c, sz);
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
        u.profession = "CROSSBOWMAN".into();
        u.skills.push(rf_core::Skill { id: "CROSSBOW".into(), rating: 6, xp: 0 });
        u.skills.push(rf_core::Skill { id: "RANGED_COMBAT".into(), rating: 4, xp: 0 });
        u.skills.push(rf_core::Skill { id: "HAMMER".into(), rating: 2, xp: 0 });
        u.skills.push(rf_core::Skill { id: "DODGING".into(), rating: 3, xp: 0 });
        u.skills.push(rf_core::Skill { id: "ARMOR".into(), rating: 3, xp: 0 });
    }
    let ids = w.units.iter().map(|u| u.id).collect();
    (w, ids)
}

/// A fortification tiletype for the synthetic world (it has none).
fn add_fortification_tt(w: &mut World) -> u16 {
    let mut infos = w.tiletypes.infos.clone();
    let fid = infos.len() as u16;
    infos.push(TiletypeInfo { id: fid, name: "StoneFortification".into(), shape: Shape::Fortification, material: TtMaterial::Stone, special: TtSpecial::Normal, variant: 0, direction: String::new() });
    w.tiletypes = TiletypeTable::new(infos);
    fid
}

/// 10 goblins (2 bowmen) spawned `dist` tiles east of the marksdwarves, armed like a siege.
fn add_goblins(w: &mut World, x: i16, y0: i16, z: i16) -> Vec<i32> {
    let template = w.units[0].clone();
    let mut ids = Vec::new();
    for k in 0..10 {
        let mut g = template.clone();
        g.id = 900 + k;
        g.race = "GOBLIN".into();
        g.name = String::new();
        g.profession = if k < 2 { "BOWMAN".into() } else { "AXEMAN".into() };
        g.custom_profession = if k < 2 { "bowman".into() } else { "axeman".into() };
        g.skills = vec![
            rf_core::Skill { id: if k < 2 { "BOW".into() } else { "AXE".into() }, rating: 4, xp: 0 },
            rf_core::Skill { id: "DODGING".into(), rating: 2, xp: 0 },
            rf_core::Skill { id: "ARMOR".into(), rating: 2, xp: 0 },
        ];
        g.pos = Pos::new(x + (k % 3) as i16, y0 - 4 + k as i16, z);
        g.flags.citizen = false;
        g.flags.resident = false;
        g.invader_civ = 77;
        ids.push(g.id);
        w.units.push(g);
    }
    ids
}

fn arm_goblins(sim: &mut Sim, gobs: &[i32]) {
    for (k, &id) in gobs.iter().enumerate() {
        let metal = ["INORGANIC:COPPER", "INORGANIC:BRONZE", "INORGANIC:IRON"][k % 3];
        if k < 2 {
            sim.give_item(id, "WEAPON", "ITEM_WEAPON_BOW", "PLANT:OAK:WOOD", 0);
            sim.give_item(id, "QUIVER", "", "CREATURE:COW:LEATHER", 0);
            let a = sim.give_item(id, "AMMO", "ITEM_AMMO_ARROWS", metal, 0).unwrap();
            sim.world.items.iter_mut().find(|i| i.id == a).unwrap().stack = 35;
        } else {
            sim.give_item(id, "WEAPON", "ITEM_WEAPON_AXE_BATTLE", metal, 0);
        }
        if k % 2 == 0 {
            sim.give_item(id, "ARMOR", "ITEM_ARMOR_MAIL_SHIRT", metal, 0);
        }
        if k % 3 == 0 {
            sim.give_item(id, "HELM", "ITEM_HELM_HELM", metal, 0);
        }
        sim.make_hostile(id);
    }
}

/// 5 marksdwarves behind a line of fortifications, 10 goblins 12-14 tiles away on the other side.
fn five_vs_ten(seed: u64) -> (Sim, Vec<i32>, Vec<i32>, u64) {
    let (mut w, ids) = marksdwarf_world(5, 64);
    let fid = add_fortification_tt(&mut w);
    let sz = surface_z();
    let c = 32i16;
    // Marksdwarves in a column at x = c, fortifications along x = c + 1 over the whole map.
    for (k, u) in w.units.iter_mut().enumerate() {
        u.pos = Pos::new(c, c - 2 + k as i16, sz);
    }
    for y in 0..64 {
        w.tile_mut(Pos::new(c + 1, y, sz)).unwrap().tt = fid;
    }
    let gobs = add_goblins(&mut w, c + 12, c, sz);
    let mut sim = Sim::new(w, seed);
    sim.set_invasions(false);
    sim.set_forbid_used_ammo(false);
    let q = sim.create_squad("Schuetzen");
    for (k, &id) in ids.iter().enumerate() {
        sim.assign_squad_member(q, k, id);
    }
    arm_marksdwarves(&mut sim, &ids, 50, true, "INORGANIC:IRON");
    let here = sim.world.unit(ids[2]).unwrap().pos;
    sim.order_squad(q, SquadOrder::Station(here));
    arm_goblins(&mut sim, &gobs);
    let mut t = 0;
    while t < 20_000 && gobs.iter().any(|&g| !sim.world.unit(g).unwrap().flags.dead) {
        sim.run_ticks(100);
        t += 100;
    }
    (sim, ids, gobs, t)
}

#[test]
fn five_marksdwarves_shoot_ten_goblins() {
    let t0 = std::time::Instant::now();
    let (mut sim, ids, gobs, ticks) = five_vs_ten(5);
    let rs = sim.ranged_stats();
    let goblins_dead = gobs.iter().filter(|&&g| sim.world.unit(g).unwrap().flags.dead).count();
    let dwarves_dead = ids.iter().filter(|&&d| sim.world.unit(d).unwrap().flags.dead).count();
    let ammo_left: i32 = ids.iter().map(|&id| sim.unit_ammo(id)).sum();
    let secs = t0.elapsed().as_secs_f64();
    println!(
        "5 marksdwarves (fortified) vs 10 goblins: {ticks} ticks ({secs:.2}s, {:.0} ticks/s): goblins dead {goblins_dead}, dwarves dead {dwarves_dead}, bolts left {ammo_left}
  {rs:?}",
        ticks as f64 / secs.max(1e-9)
    );
    print_log(&sim, "flying", 25);
    assert!(rs.fort_shots >= 30, "the marksdwarves fire volleys");
    assert!(rs.hits > 10, "bolts hit");
    assert!(rs.fort_kills >= 3, "bolts kill goblins");
    assert!(goblins_dead >= 6, "most goblins die");
    assert_eq!(dwarves_dead, 0, "the fortifications protect the marksdwarves");
    assert_eq!(250 - ammo_left as u64, rs.fort_shots, "every shot used one bolt");
    assert!(rs.ammo_dropped > 0 && rs.ammo_broken > 0, "bolts land or break");
    let loose = sim.world.items.iter().filter(|i| i.kind == "AMMO" && i.subtype == "ITEM_AMMO_BOLTS" && i.flags.on_ground).count();
    assert!(loose > 0, "fired bolts lie on the ground");
    assert!(sim.combat_log().iter().any(|l| l.starts_with("The flying iron bolt strikes the goblin")), "DF-style projectile log lines");
}

/// Informational: in the open field, 10 goblins overrun 5 marksdwarves (as in DF).
#[test]
fn marksdwarves_in_the_open_fire_then_melee() {
    let (mut w, ids) = marksdwarf_world(5, 64);
    let sz = surface_z();
    let gobs = add_goblins(&mut w, 32 + 18, 32, sz);
    let mut sim = Sim::new(w, 6);
    sim.set_invasions(false);
    let q = sim.create_squad("Schuetzen");
    for (k, &id) in ids.iter().enumerate() {
        sim.assign_squad_member(q, k, id);
    }
    arm_marksdwarves(&mut sim, &ids, 50, true, "INORGANIC:IRON");
    let here = sim.world.unit(ids[2]).unwrap().pos;
    sim.order_squad(q, SquadOrder::Station(here));
    arm_goblins(&mut sim, &gobs);
    sim.run_ticks(15_000);
    let rs = sim.ranged_stats();
    let goblins_dead = gobs.iter().filter(|&&g| sim.world.unit(g).unwrap().flags.dead).count();
    let dwarves_dead = ids.iter().filter(|&&d| sim.world.unit(d).unwrap().flags.dead).count();
    println!("open field: goblins dead {goblins_dead}, dwarves dead {dwarves_dead}; {rs:?}");
    print_log(&sim, "crossbow", 6);
    assert!(rs.fort_shots > 10 && rs.hostile_shots > 0, "both sides shoot");
    assert!(rs.melee_fallbacks > 0, "the goblins close in and the marksdwarves fight in melee");
}

#[test]
fn out_of_ammo_marksdwarf_bashes_with_crossbow() {
    let (mut w, ids) = marksdwarf_world(1, 48);
    let sz = surface_z();
    let c = 24;
    w.units[0].pos = Pos::new(c, c, sz);
    let mut gob = w.units[0].clone();
    gob.id = 900;
    gob.race = "GOBLIN".into();
    gob.name = String::new();
    gob.profession = "STANDARD".into();
    gob.skills.clear();
    gob.pos = Pos::new(c + 10, c, sz);
    gob.flags.citizen = false;
    gob.flags.resident = false;
    w.units.push(gob);
    let mut sim = Sim::new(w, 9);
    sim.set_invasions(false);
    let q = sim.create_squad("Schuetzen");
    sim.assign_squad_member(q, 0, ids[0]);
    arm_marksdwarves(&mut sim, &ids, 2, true, "INORGANIC:IRON");
    sim.order_squad(q, SquadOrder::Kill(900));
    sim.make_hostile(900);
    let mut t = 0;
    while t < 20_000 && !sim.world.unit(900).unwrap().flags.dead && !sim.world.unit(ids[0]).unwrap().flags.dead {
        sim.run_ticks(50);
        t += 50;
    }
    let rs = sim.ranged_stats();
    println!("out of ammo after {t} ticks: {rs:?}, ammo left {}", sim.unit_ammo(ids[0]));
    print_log(&sim, "", 30);
    assert_eq!(rs.ammo_used, 2, "both bolts fired");
    assert!(rs.melee_fallbacks >= 1 && rs.out_of_ammo >= 1, "falls back to melee");
    assert!(sim.combat_log().iter().any(|l| l.starts_with("Marksdwarf 0 bashes") && l.contains("crossbow")), "bashes with the crossbow");
    assert!(sim.world.units.iter().find(|u| u.id == ids[0]).unwrap().skill("HAMMER") >= 2);
}

#[test]
fn archery_practice_trains_crossbow_skill() {
    let (mut w, ids) = marksdwarf_world(1, 48);
    let hz = hall_z();
    let c = 24i16;
    w.units[0].pos = Pos::new(c - 3, c + 2, hz);
    for u in &mut w.units {
        u.skills.retain(|k| k.id != "CROSSBOW");
        u.skills.push(rf_core::Skill { id: "CROSSBOW".into(), rating: 0, xp: 0 });
    }
    let bid = 5000;
    w.buildings.push(Building { id: bid, kind: "ArcheryTarget".into(), x1: c + 6, y1: c, x2: c + 6, y2: c, z: hz, center: Pos::new(c + 6, c, hz), built: true, stage: -1, owner: -1, ..Default::default() });
    w.buildings.push(Building {
        id: bid + 1,
        kind: "Civzone".into(),
        subtype: "ArcheryRange".into(),
        x1: c - 6,
        y1: c,
        x2: c + 5,
        y2: c,
        z: hz,
        center: Pos::new(c, c, hz),
        built: true,
        stage: -1,
        owner: -1,
        mat: rf_core::NO_MAT,
        ..Default::default()
    });
    let mut sim = Sim::new(w, 4);
    sim.set_invasions(false);
    let q = sim.create_squad("Schuetzen");
    sim.assign_squad_member(q, 0, ids[0]);
    // Training bolts (wood) and a few iron bolts: practice uses the wooden ones first.
    arm_marksdwarves(&mut sim, &ids, 60, false, "PLANT:OAK:WOOD");
    let iron = sim.give_item(ids[0], "AMMO", "ITEM_AMMO_BOLTS", "INORGANIC:IRON", 0).unwrap();
    sim.world.items.iter_mut().find(|i| i.id == iron).unwrap().stack = 5;
    sim.order_squad(q, SquadOrder::Train);
    let xp0 = sim.world.unit(ids[0]).unwrap().skills.iter().find(|k| k.id == "CROSSBOW").map(|k| k.xp).unwrap();
    sim.run_ticks(2_400);
    let u = sim.world.unit(ids[0]).unwrap();
    let sk = u.skills.iter().find(|k| k.id == "CROSSBOW").unwrap().clone();
    let rs = sim.ranged_stats();
    let at_target = sim.world.items.iter().filter(|i| i.kind == "AMMO" && i.flags.on_ground && i.pos.z == hz && (i.pos.x - (c + 6)).abs() <= 1 && i.pos.y == c).count();
    let behind = sim.world.items.iter().filter(|i| i.kind == "AMMO" && i.flags.on_ground).count();
    let iron_left = sim.world.items.iter().find(|i| i.id == iron).map(|i| i.stack).unwrap_or(0);
    println!("practice: {rs:?}; CROSSBOW now level {} xp {} (was 0/{xp0}); bolts at the target {at_target}, on the ground {behind}; iron left {iron_left}; unit at {:?}", sk.rating, sk.xp, u.pos);
    assert!(rs.practice_shots >= 20, "shoots at the target");
    assert!(rs.practice_hits > 0);
    assert_eq!(rs.shots, 0, "no combat shots");
    assert!(sk.rating >= 1 || sk.xp > 100, "gains crossbow experience");
    assert!(behind > 0, "used bolts lie at the range");
    assert_eq!(iron_left, 5, "training uses the wooden bolts first");
    assert!(u.pos.y == c && u.pos.x <= c + 4, "stands in the range, away from the target");
}

#[test]
fn line_of_fire_walls_floors_fortifications() {
    let mut w = World::synthetic(48, 48, DZ);
    let fid = add_fortification_tt(&mut w);
    let hz = hall_z();
    let c = 24i16;
    let fort = Pos::new(c, c + 3, hz);
    w.tile_mut(fort).unwrap().tt = fid;
    let sim = Sim::new(w, 1);
    let a = Pos::new(c - 5, c - 2, hz);
    let b = Pos::new(c + 5, c - 2, hz);
    assert!(sim.line_of_fire(a, b, 0), "open hall");
    // Through the hall's rock walls: blocked.
    assert!(!sim.line_of_fire(Pos::new(c, c - 2, hz), Pos::new(c, c - 12, hz), 20), "rock wall");
    // Up through the ceiling to the surface: blocked by the floors in between.
    assert!(!sim.line_of_fire(Pos::new(c + 3, c, hz), Pos::new(c + 3, c + 2, surface_z()), 20), "floors between levels");
    // On the surface across open grass: free.
    let sz = surface_z();
    assert!(sim.line_of_fire(Pos::new(5, 5, sz), Pos::new(20, 12, sz), 0));
    // Fortification: a novice only shoots through it when standing next to it.
    let far = Pos::new(c - 4, c + 3, hz);
    let next = Pos::new(c - 1, c + 3, hz);
    let tgt = Pos::new(c + 4, c + 3, hz);
    assert!(!sim.line_of_fire(far, tgt, 3), "novice, fortification 4 tiles away");
    assert!(sim.line_of_fire(next, tgt, 3), "novice next to the fortification");
    assert!(sim.line_of_fire(far, tgt, 10), "accomplished shoots through");
    assert!(!sim.is_walkable(fort), "fortifications block movement");
}

#[test]
fn ranged_is_deterministic_across_threads() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let (w, ids) = marksdwarf_world(5, 64);
            let mut sim = Sim::new(w, 21);
            sim.set_parallel_threshold(1);
            sim.set_invasions(false);
            let q = sim.create_squad("Schuetzen");
            for (k, &id) in ids.iter().enumerate() {
                sim.assign_squad_member(q, k, id);
            }
            arm_marksdwarves(&mut sim, &ids, 40, true, "INORGANIC:IRON");
            let here = sim.world.unit(ids[2]).unwrap().pos;
            sim.order_squad(q, SquadOrder::Station(here));
            sim.spawn_siege("GOBLIN", 10);
            sim.run_ticks(5_000);
            let rs = sim.ranged_stats();
            (sim.state_hash(), rs.shots, rs.hits, rs.ammo_used)
        })
    };
    let a = run(1);
    let b = run(4);
    println!("determinism: {a:?} vs {b:?}");
    assert!(a.1 > 0, "shots fired");
    assert_eq!(a, b);
}
