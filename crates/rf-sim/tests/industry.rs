//! Storage (bins, barrels) and industry chains (charcoal, smelting, forging, butchery).

use rf_core::{Building, CasteRaw, CreatureRaw, Item, ItemFlags, MaterialInfo, Pos, StockpileSettings, World};
use rf_sim::Sim;

const DX: u16 = 48;
const DY: u16 = 48;
const DZ: u16 = 20;

fn hall() -> (i16, i16, i16) {
    let surface_z = (DZ as i16 * 2 / 3).max(2);
    (DX as i16 / 2, DY as i16 / 2, (surface_z - 4).max(0))
}

fn item(id: i32, kind: &str, mat: u16, p: Pos, stack: i32) -> Item {
    Item {
        id,
        kind: kind.into(),
        mat,
        pos: p,
        stack,
        container: -1,
        holder: -1,
        building: -1,
        flags: ItemFlags { on_ground: true, ..Default::default() },
        ..Default::default()
    }
}

fn shop(id: i32, kind: &str, subtype: &str, cx: i16, cy: i16, z: i16) -> Building {
    Building {
        id,
        kind: kind.into(),
        subtype: subtype.into(),
        x1: cx - 1,
        y1: cy - 1,
        x2: cx + 1,
        y2: cy + 1,
        z,
        center: Pos::new(cx, cy, z),
        built: true,
        owner: -1,
        ..Default::default()
    }
}

fn pile(id: i32, x1: i16, y: i16, x2: i16, z: i16, cats: &[&str], bins: i32, barrels: i32) -> Building {
    Building {
        id,
        kind: "Stockpile".into(),
        x1,
        y1: y,
        x2,
        y2: y,
        z,
        center: Pos::new(x1, y, z),
        built: true,
        owner: -1,
        stockpile: Some(StockpileSettings { categories: cats.iter().map(|s| s.to_string()).collect(), max_bins: bins, max_barrels: barrels, max_wheelbarrows: 0 }),
        ..Default::default()
    }
}

fn give_labors(w: &mut World, labors: &[&str]) {
    for u in w.units.iter_mut().filter(|u| u.flags.citizen) {
        for l in labors {
            u.labors.push(l.to_string());
        }
    }
}

fn template(sim: &Sim, label: &str) -> usize {
    sim.order_templates().iter().position(|t| t.label == label).unwrap_or_else(|| panic!("no template {label}"))
}

fn done(sim: &Sim, label: &str) -> u64 {
    sim.production_report().jobs_done.iter().filter(|(l, _)| *l == label).map(|(_, n)| *n).sum()
}

#[test]
fn bins_and_barrels_hold_many_items_per_tile() {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    // A 2-tile bars/blocks pile with one bin, a 2-tile food pile with one barrel.
    w.buildings.push(pile(900, cx - 6, cy + 4, cx - 5, hz, &["bars_blocks"], 1, 0));
    w.buildings.push(pile(901, cx + 5, cy + 4, cx + 6, hz, &["food"], 0, 1));
    let mut id = 7000;
    w.items.push(item(id, "BIN", 3, Pos::new(cx - 6, cy + 4, hz), 1));
    id += 1;
    w.items.push(item(id, "BARREL", 3, Pos::new(cx + 6, cy + 4, hz), 1));
    id += 1;
    for k in 0..6 {
        w.items.push(item(id, "BLOCKS", 0, Pos::new(cx - 3 + k, cy + 2, hz), 1));
        id += 1;
    }
    // Remove the synthetic drinks/plants so the food pile only gets these 6 plant stacks.
    w.items.retain(|i| i.kind != "PLANT" && i.kind != "DRINK");
    for k in 0..6 {
        w.items.push(item(id, "PLANT", 6, Pos::new(cx - 3 + k, cy + 1, hz), 3));
        id += 1;
    }
    give_labors(&mut w, &["HAUL_ITEM"]);
    let mut sim = Sim::new(w, 5);
    sim.run_ticks(4_000);
    let in_bin = sim.world.items.iter().filter(|i| i.kind == "BLOCKS" && i.container == 7000).count();
    // Wiki capacity: a bin holds 60 000 cm³; blocks take 5 400 (11 per bin) -> all 6 fit.
    assert_eq!(in_bin, 6, "blocks stored into the bin");
    let plants_in_barrel: i32 = sim.world.items.iter().filter(|i| i.kind == "PLANT" && i.container == 7001).map(|i| i.stack).sum();
    assert!(plants_in_barrel >= 12, "plants stored into the barrel: {plants_in_barrel}");
    let rep = sim.storage_report();
    let bp = rep.piles.iter().find(|p| p.building_id == 900).unwrap();
    assert_eq!(bp.containers, 1);
    assert_eq!(bp.contained_items, 6);
    assert!(rep.stored_into_containers >= 10);
}

#[test]
fn barrel_capacity_follows_the_wiki() {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    w.buildings.push(pile(901, cx + 5, cy + 4, cx + 5, hz, &["food"], 0, 1));
    w.items.retain(|i| i.kind != "PLANT" && i.kind != "DRINK");
    w.items.push(item(7001, "BARREL", 3, Pos::new(cx + 5, cy + 4, hz), 1));
    // 7 stacks of 10 plants: 60 fit into one barrel (wiki "Barrel"), the 7th stack does not.
    for k in 0..7 {
        w.items.push(item(7100 + k, "PLANT", 6, Pos::new(cx - 3 + k as i16, cy + 1, hz), 10));
    }
    let mut sim = Sim::new(w, 6);
    sim.run_ticks(5_000);
    // Dwarves may eat from the barrel; count the stacks that went in.
    let inside = sim.world.items.iter().filter(|i| i.kind == "PLANT" && i.container == 7001).count();
    let outside = sim.world.items.iter().filter(|i| i.kind == "PLANT" && i.container < 0).count();
    assert_eq!(inside, 6);
    assert_eq!(outside, 1, "the 7th stack does not fit");
}

/// Wood furnace, smelter and forge next to each other in the synthetic hall.
fn metal_world(with_logs: bool) -> World {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    if !with_logs {
        w.items.retain(|i| i.kind != "WOOD");
    }
    w.buildings.push(shop(800, "Furnace", "WoodFurnace", cx - 4, cy - 2, hz));
    w.buildings.push(shop(801, "Furnace", "Smelter", cx, cy + 2, hz));
    w.buildings.push(shop(802, "Workshop", "MetalsmithsForge", cx + 4, cy - 2, hz));
    // Two magnetite (ore) boulders.
    w.items.push(item(7200, "BOULDER", 2, Pos::new(cx + 2, cy + 3, hz), 1));
    w.items.push(item(7201, "BOULDER", 2, Pos::new(cx + 3, cy + 3, hz), 1));
    give_labors(&mut w, &["BURN_WOOD", "SMELT", "FORGE_WEAPON", "FORGE_ARMOR", "HAUL_ITEM"]);
    w
}

#[test]
fn charcoal_smelting_and_forging_chain() {
    let w = metal_world(true);
    let mut sim = Sim::new(w, 7);
    let charcoal = template(&sim, "Make charcoal");
    let smelt = template(&sim, "Smelt ore");
    let pick = template(&sim, "Forge metal weapon");
    sim.add_building_job(800, charcoal, false);
    sim.add_building_job(800, charcoal, false);
    sim.add_building_job(800, charcoal, false);
    sim.add_building_job(801, smelt, false);
    sim.add_building_job(801, smelt, false);
    sim.add_building_job(802, pick, false);
    sim.run_ticks(20_000);
    assert_eq!(done(&sim, "Make charcoal"), 3);
    assert_eq!(done(&sim, "Smelt ore"), 2);
    assert_eq!(done(&sim, "Forge metal weapon"), 1);
    let iron = sim.world.materials.iter().position(|m| m.token == "INORGANIC:IRON").expect("iron added") as u16;
    // Magnetite smelts to 4 iron bars each (8), a pick (MATERIAL_SIZE 4) uses 1.
    let bars: i32 = sim.world.items.iter().filter(|i| i.kind == "BAR" && i.mat == iron).map(|i| i.stack).sum();
    assert_eq!(bars, 7);
    let picks: Vec<&Item> = sim.world.items.iter().filter(|i| i.kind == "WEAPON").collect();
    assert_eq!(picks.len(), 1);
    assert_eq!(picks[0].subtype, "ITEM_WEAPON_PICK");
    assert_eq!(picks[0].mat, iron);
    // 3 charcoal made, 2 burnt by smelting, 1 by forging.
    let fuel: i32 = sim.world.items.iter().filter(|i| i.kind == "BAR" && sim.world.materials[i.mat as usize].token == "COAL:CHARCOAL").map(|i| i.stack).sum();
    assert_eq!(fuel, 0);
}

#[test]
fn forge_needs_fuel() {
    let (cx, cy, hz) = hall();
    let mut w = metal_world(false);
    let iron = w.materials.len() as u16;
    w.materials.push(MaterialInfo { token: "INORGANIC:IRON".into(), name: "iron".into(), is_metal: true, ..Default::default() });
    for k in 0..3 {
        w.items.push(item(7300 + k, "BAR", iron, Pos::new(cx + 2 + k as i16, cy - 3, hz), 1));
    }
    let mut sim = Sim::new(w, 8);
    let armor = template(&sim, "Forge metal armor");
    sim.add_building_job(802, armor, false);
    sim.run_ticks(6_000);
    assert_eq!(done(&sim, "Forge metal armor"), 0, "no charcoal or coke: no forging");
    // Add one coke bar: a breastplate (MATERIAL_SIZE 9) takes 3 bars + 1 fuel.
    let coke = sim.world.materials.len() as u16;
    sim.world.materials.push(MaterialInfo { token: "COAL:COKE".into(), name: "coke".into(), ..Default::default() });
    let mut w2 = sim.world.clone();
    w2.items.push(item(7400, "BAR", coke, Pos::new(cx + 2, cy - 1, hz), 1));
    let mut sim = Sim::new(w2, 8);
    sim.run_ticks(8_000);
    assert_eq!(done(&sim, "Forge metal armor"), 1);
    assert_eq!(sim.world.items.iter().filter(|i| i.kind == "BAR").count(), 0, "3 iron bars + 1 coke used");
    let bp = sim.world.items.iter().find(|i| i.kind == "ARMOR").expect("breastplate");
    assert_eq!(bp.subtype, "ITEM_ARMOR_BREASTPLATE");
}

#[test]
fn butcher_yields_follow_body_size() {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    w.buildings.push(shop(810, "Workshop", "Butchers", cx + 4, cy - 2, hz));
    w.creature_raws.push(CreatureRaw {
        token: "COW".into(),
        name: "cow".into(),
        castes: vec![CasteRaw { token: "FEMALE".into(), name: "cow".into(), body_size: 500_000, ..Default::default() }],
        ..Default::default()
    });
    let mut c = item(7500, "CORPSE", rf_core::NO_MAT, Pos::new(cx + 2, cy + 3, hz), 1);
    c.desc = "cow corpse".into();
    w.items.push(c);
    let mut sk = item(7501, "CORPSE", rf_core::NO_MAT, Pos::new(cx + 3, cy + 3, hz), 1);
    sk.desc = "cow skeleton".into();
    w.items.push(sk);
    let mut dwarf = item(7502, "CORPSE", rf_core::NO_MAT, Pos::new(cx + 4, cy + 3, hz), 1);
    dwarf.desc = "Urist McDead's corpse".into();
    w.items.push(dwarf);
    give_labors(&mut w, &["BUTCHER"]);
    let mut sim = Sim::new(w, 9);
    let t = template(&sim, "Butcher a dead animal");
    for _ in 0..3 {
        sim.add_building_job(810, t, false);
    }
    sim.run_ticks(10_000);
    assert_eq!(done(&sim, "Butcher a dead animal"), 2, "the dwarf's corpse is never butchered");
    let sum = |k: &str, d: &str| -> i32 { sim.world.items.iter().filter(|i| i.kind == k && i.desc.contains(d)).map(|i| i.stack).sum() };
    assert_eq!(sum("MEAT", "cow meat"), 50);
    assert_eq!(sum("GLOB", "cow fat"), 12);
    assert_eq!(sum("CORPSEPIECE", "cow bone"), 50, "25 bones each from the corpse and the skeleton");
    assert_eq!(sum("CORPSEPIECE", "cow skull"), 2);
    assert_eq!(sum("CORPSEPIECE", "cow skin"), 1, "skeletons give no skin");
    assert!(sim.world.items.iter().any(|i| i.desc.contains("McDead")));
}

#[test]
fn industry_is_deterministic_across_thread_counts() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let (cx, cy, hz) = hall();
            let mut w = metal_world(true);
            w.buildings.push(pile(900, cx - 6, cy + 4, cx - 3, hz, &["bars_blocks"], 2, 0));
            w.items.push(item(7600, "BIN", 3, Pos::new(cx - 6, cy + 4, hz), 1));
            w.items.push(item(7601, "BIN", 3, Pos::new(cx + 6, cy - 4, hz), 1));
            let mut sim = Sim::new(w, 21);
            sim.set_parallel_threshold(1);
            let charcoal = template(&sim, "Make charcoal");
            let smelt = template(&sim, "Smelt ore");
            sim.add_building_job(800, charcoal, true);
            sim.add_building_job(801, smelt, true);
            sim.run_ticks(30_000);
            (sim.state_hash(), sim.production_report().jobs_done.iter().map(|j| j.1).sum::<u64>(), sim.storage_report().stored_into_containers)
        })
    };
    let a = run(1);
    let b = run(4);
    assert!(a.1 >= 4, "jobs done: {}", a.1);
    assert!(a.2 >= 1, "bars stored into bins: {}", a.2);
    assert_eq!(a, b);
}
