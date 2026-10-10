//! [metal12] Raw reactions: steel, coke, alloys in a magma smelter, splints, tallow -> soap,
//! and the wiki item-quality formula.

use rf_core::{Building, Item, ItemFlags, MaterialInfo, Pos, World};
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

fn give_labors(w: &mut World, labors: &[&str]) {
    for u in w.units.iter_mut().filter(|u| u.flags.citizen) {
        for l in labors {
            u.labors.push(l.to_string());
        }
    }
}

fn mat(w: &mut World, token: &str, name: &str, f: impl FnOnce(&mut MaterialInfo)) -> u16 {
    let mut m = MaterialInfo { token: token.into(), name: name.into(), ..Default::default() };
    f(&mut m);
    w.materials.push(m);
    (w.materials.len() - 1) as u16
}

fn template(sim: &Sim, label: &str) -> usize {
    sim.order_templates().iter().position(|t| t.label == label).unwrap_or_else(|| panic!("no template {label}"))
}

fn done(sim: &Sim, label: &str) -> u64 {
    sim.production_report().jobs_done.iter().filter(|(l, _)| *l == label).map(|(_, n)| *n).sum()
}

fn bars(sim: &Sim, token: &str) -> i32 {
    sim.world.items.iter().filter(|i| i.kind == "BAR" && sim.world.materials[i.mat as usize].token == token).map(|i| i.stack).sum()
}

#[test]
fn pig_iron_and_steel_from_iron_flux_and_coke() {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    w.buildings.push(shop(801, "Furnace", "Smelter", cx, cy + 2, hz));
    let iron = mat(&mut w, "INORGANIC:IRON", "iron", |m| m.is_metal = true);
    let lime = mat(&mut w, "INORGANIC:LIMESTONE", "limestone", |m| {
        m.is_stone = true;
        m.tags = vec!["RC:FLUX".into()];
    });
    let coke = mat(&mut w, "COAL:COKE", "coke", |_| {});
    for k in 0..2 {
        w.items.push(item(7300 + k, "BAR", iron, Pos::new(cx + 2 + k as i16, cy - 3, hz), 1));
        w.items.push(item(7310 + k, "BOULDER", lime, Pos::new(cx - 2 - k as i16, cy - 3, hz), 1));
    }
    for k in 0..4 {
        w.items.push(item(7320 + k, "BAR", coke, Pos::new(cx + k as i16, cy - 4, hz), 1));
    }
    give_labors(&mut w, &["SMELT"]);
    let mut sim = Sim::new(w, 7);
    sim.add_building_job(801, template(&sim, "Make pig iron bars"), false);
    sim.add_building_job(801, template(&sim, "Make steel bars"), false);
    sim.run_ticks(20_000);
    assert_eq!(done(&sim, "Make pig iron bars"), 1);
    assert_eq!(done(&sim, "Make steel bars"), 1);
    // Pig iron: iron + flux + coal (+ fuel); steel: iron + pig iron + flux + coal (+ fuel) -> 2.
    assert_eq!(bars(&sim, "INORGANIC:STEEL"), 2);
    assert_eq!(bars(&sim, "INORGANIC:PIG_IRON"), 0);
    assert_eq!(bars(&sim, "INORGANIC:IRON"), 0);
    assert_eq!(bars(&sim, "COAL:COKE"), 0, "2 coal reagents + 2 fuel");
}

#[test]
fn coke_from_bituminous_coal_and_order_from_the_real_queue() {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    let mut sm = shop(801, "Furnace", "Smelter", cx, cy + 2, hz);
    // The job as DF exports it from the reference fort's smelter queue.
    sm.jobs.push(rf_core::BuildingJob { job: "CustomReaction".into(), reaction: "BITUMINOUS_COAL_TO_COKE".into(), order_id: -1, ..Default::default() });
    w.buildings.push(sm);
    let coal = mat(&mut w, "INORGANIC:COAL_BITUMINOUS", "bituminous coal", |m| m.is_stone = true);
    let charcoal = mat(&mut w, "COAL:CHARCOAL", "charcoal", |_| {});
    w.items.push(item(7300, "BOULDER", coal, Pos::new(cx + 2, cy - 3, hz), 1));
    w.items.push(item(7301, "BAR", charcoal, Pos::new(cx - 2, cy - 3, hz), 1));
    give_labors(&mut w, &["SMELT"]);
    let mut sim = Sim::new(w, 9);
    sim.run_ticks(10_000);
    assert_eq!(done(&sim, "Make coke from bituminous coal"), 1);
    assert_eq!(bars(&sim, "COAL:COKE"), 9);
    assert_eq!(bars(&sim, "COAL:CHARCOAL"), 0);
    assert!(sim.production_notes().iter().all(|n| !n.contains("CustomReaction")), "{:?}", sim.production_notes());
}

fn magma_world(magma: bool) -> (World, u16) {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    w.buildings.push(shop(801, "Furnace", "MagmaSmelter", cx, cy + 2, hz));
    if magma {
        // Magma 7/7 below the north-west tile (one of the 8 non-center tiles).
        let t = w.idx(Pos::new(cx - 1, cy + 1, hz - 1)).unwrap();
        w.tiles[t].liquid = rf_core::liquid::MAGMA | 7;
    }
    let tin = mat(&mut w, "INORGANIC:TIN", "tin", |m| m.is_metal = true);
    let copper = mat(&mut w, "INORGANIC:COPPER", "copper", |m| m.is_metal = true);
    w.items.push(item(7300, "BAR", tin, Pos::new(cx + 2, cy - 3, hz), 1));
    w.items.push(item(7301, "BAR", copper, Pos::new(cx - 2, cy - 3, hz), 1));
    give_labors(&mut w, &["SMELT"]);
    let bronze = w.materials.len();
    (w, bronze as u16)
}

#[test]
fn magma_smelter_needs_magma_but_no_fuel() {
    for magma in [true, false] {
        let (w, _) = magma_world(magma);
        let mut sim = Sim::new(w, 3);
        sim.add_building_job(801, template(&sim, "Make bronze bars (use bars)"), false);
        sim.run_ticks(10_000);
        let made = done(&sim, "Make bronze bars (use bars)");
        if magma {
            assert_eq!(made, 1, "no fuel needed over magma");
            assert_eq!(bars(&sim, "INORGANIC:BRONZE"), 2);
        } else {
            assert_eq!(made, 0, "a magma smelter without magma does nothing");
        }
    }
}

#[test]
fn splints_crutches_and_soap_from_tallow() {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    w.buildings.push(shop(800, "Workshop", "Carpenters", cx - 4, cy - 2, hz));
    w.buildings.push(shop(801, "Workshop", "Kitchen", cx, cy + 3, hz));
    w.buildings.push(shop(802, "Workshop", "Ashery", cx + 4, cy - 2, hz));
    w.buildings.push(shop(803, "Workshop", "SOAP_MAKER", cx - 4, cy + 3, hz));
    let fat = mat(&mut w, "CREATURE:YAK:FAT", "yak fat", |m| m.tags = vec!["RC:FAT".into()]);
    let ash = mat(&mut w, "ASH", "ash", |_| {});
    let wood = w.items.iter().find(|i| i.kind == "WOOD").map(|i| i.mat).expect("synthetic logs");
    w.items.push(item(7300, "GLOB", fat, Pos::new(cx + 2, cy - 3, hz), 3));
    w.items.push(item(7301, "BAR", ash, Pos::new(cx - 2, cy - 3, hz), 1));
    w.items.push(item(7302, "BUCKET", wood, Pos::new(cx - 1, cy - 3, hz), 1));
    give_labors(&mut w, &["CARPENTER", "COOK", "LYE_MAKING", "SOAP_MAKER"]);
    let mut sim = Sim::new(w, 5);
    sim.add_building_job(800, template(&sim, "Construct wooden splint"), false);
    sim.add_building_job(800, template(&sim, "Construct wooden crutch"), false);
    sim.add_building_job(801, template(&sim, "Render fat"), false);
    sim.add_building_job(802, template(&sim, "Make lye"), false);
    sim.run_ticks(12_000);
    sim.add_building_job(803, template(&sim, "Make soap from tallow"), false);
    sim.run_ticks(12_000);
    assert_eq!(done(&sim, "Construct wooden splint"), 1);
    assert_eq!(done(&sim, "Construct wooden crutch"), 1);
    assert_eq!(done(&sim, "Render fat"), 1);
    assert_eq!(done(&sim, "Make lye"), 1);
    assert_eq!(done(&sim, "Make soap from tallow"), 1);
    assert!(sim.world.items.iter().any(|i| i.kind == "SPLINT"));
    assert!(sim.world.items.iter().any(|i| i.kind == "CRUTCH"));
    let soap = sim.world.items.iter().find(|i| i.kind == "BAR" && sim.world.materials[i.mat as usize].token == "CREATURE:YAK:SOAP").expect("yak soap");
    assert_eq!(soap.stack, 1);
    // 3 fat - 1 rendered = 2 fat left; the tallow became soap; the bucket stays (empty).
    let fat_left: i32 = sim.world.items.iter().filter(|i| i.kind == "GLOB" && i.mat == fat).map(|i| i.stack).sum();
    assert_eq!(fat_left, 2);
    assert!(sim.world.items.iter().any(|i| i.kind == "BUCKET"));
    assert!(!sim.world.items.iter().any(|i| i.kind == "LIQUID_MISC"), "lye used up");
}
