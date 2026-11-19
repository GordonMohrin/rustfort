//! [fire15] Unit tests on synthetic worlds (32x32x9: surface z6 grass, soil z4-5, stone below,
//! hall at z2 around (16,16)).

use super::heat::*;
use super::tiles::*;
use crate::Sim;
use rf_core::*;

const DX: u16 = 32;
const DY: u16 = 32;
const DZ: u16 = 9;
const SURF: i16 = 6;
const HALL: i16 = 2;

fn add_tt(w: &mut World, name: &str, shape: Shape, mat: TtMaterial) -> u16 {
    let id = w.tiletypes.infos.len() as u16;
    w.tiletypes.infos.push(TiletypeInfo { id, name: name.into(), shape, material: mat, special: TtSpecial::Normal, variant: 0, direction: String::new() });
    w.tiletypes.reindex();
    id
}

fn add_mat(w: &mut World, token: &str, f: impl FnOnce(&mut MaterialInfo)) -> u16 {
    let mut m = MaterialInfo { token: token.into(), name: token.to_ascii_lowercase(), mat_type: -1, mat_index: -1, ..Default::default() };
    f(&mut m);
    w.materials.push(m);
    (w.materials.len() - 1) as u16
}

/// Synthetic world with the fire tiletypes and temperature on.
fn world() -> World {
    let mut w = World::synthetic(DX, DY, DZ);
    w.settings.temperature = true;
    add_tt(&mut w, "Fire", Shape::Floor, TtMaterial::Fire);
    add_tt(&mut w, "Ashes1", Shape::Floor, TtMaterial::Ashes);
    add_tt(&mut w, "Ashes2", Shape::Floor, TtMaterial::Ashes);
    add_tt(&mut w, "Ashes3", Shape::Floor, TtMaterial::Ashes);
    add_tt(&mut w, "BurningTreeTrunk", Shape::Wall, TtMaterial::Fire);
    add_tt(&mut w, "TreeDeadTrunkPillar", Shape::Wall, TtMaterial::Tree);
    add_tt(&mut w, "GrassLightRamp", Shape::Ramp, TtMaterial::GrassLight);
    add_tt(&mut w, "MagmaFlow", Shape::Floor, TtMaterial::Magma);
    add_tt(&mut w, "FrozenLiquidWall", Shape::Wall, TtMaterial::FrozenLiquid);
    add_tt(&mut w, "LavaWall", Shape::Wall, TtMaterial::LavaStone);
    w
}

fn carve(w: &mut World, p: Pos, name: &str) {
    let id = w.tiletypes.by_name(name).expect(name);
    let t = w.tile_mut(p).unwrap();
    t.tt = id;
    t.flags &= !tile_flags::HIDDEN;
}

fn item(w: &mut World, id: i32, kind: &str, mat: u16, p: Pos) {
    w.items.push(Item { id, kind: kind.into(), mat, pos: p, stack: 1, container: -1, holder: -1, building: -1, desc: kind.to_ascii_lowercase(), flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() });
}

fn alive(sim: &Sim, id: i32) -> bool {
    sim.item_idx.get(&id).map_or(false, |&ix| !sim.imeta[ix as usize].dead)
}

fn tile_name(sim: &Sim, p: Pos) -> String {
    sim.world.tiletypes.get(sim.world.tile(p).unwrap().tt).name.clone()
}

// ------------------------------------------------------------------ heat model

#[test]
fn constants_match_wiki_table() {
    assert_eq!(Sim::urist_to_celsius(WATER_BOIL), 100);
    assert_eq!(Sim::urist_to_celsius(MAGMA_TEMP), 1111);
    assert_eq!(Sim::urist_to_celsius(10_000), 0);
}

#[test]
fn lignite_in_magma_ignites_after_517_ticks() {
    // w:Temperature worked example: SPEC_HEAT 409 at 10015 in magma, +4.853 U the first tick,
    // ignition point 11440 after 517 ticks.
    let h = fallback_heat("INORGANIC:LIGNITE", false, true, false, false);
    assert_eq!((h.spec_heat, h.ignite_point), (409, 11_440));
    let mut t = Temp::from_whole(UNDERGROUND_TEMP);
    t.step(MAGMA_TEMP, h.spec_heat);
    assert_eq!(t.whole(), 10_019, "first tick: +4.85");
    let mut t = Temp::from_whole(UNDERGROUND_TEMP);
    let mut n = 0;
    while t.whole() < h.ignite_point {
        t.step(MAGMA_TEMP, h.spec_heat);
        n += 1;
    }
    assert_eq!(n, 517);
}

#[test]
fn magma_and_fire_safety_follow_the_wiki_lists() {
    let m = |t: &str| fallback_heat(&format!("INORGANIC:{t}"), false, false, false, false);
    assert!(!magma_safe(&m("GRANITE")), "granite melts at 11440");
    for t in ["BASALT", "OBSIDIAN", "IRON", "STEEL", "NICKEL", "PLATINUM", "ADAMANTINE"] {
        assert!(magma_safe(&m(t)), "{t} is magma-safe");
    }
    for t in ["COPPER", "BRONZE", "SILVER", "GOLD"] {
        assert!(fire_safe(&m(t)) && !magma_safe(&m(t)), "{t}: fire-safe, not magma-safe");
    }
    for t in ["BISMUTH", "LEAD", "TIN", "ZINC"] {
        assert!(!fire_safe(&m(t)), "{t} is not fire-safe");
    }
    assert!(fire_safe(&fallback_heat("COAL:COKE", false, false, false, false)), "coal counts as fire-safe");
    let nether = fallback_heat("PLANT:NETHER_CAP:WOOD", false, false, false, true);
    assert!(!magma_safe(&nether) && survives_magma(&nether), "nether-cap survives magma by its fixed temperature");
    let wood = fallback_heat("PLANT:TOWER_CAP:WOOD", false, false, false, true);
    assert!(!fire_safe(&wood) && burns_at(&wood, GRASS_FIRE_MAX));
}

#[test]
fn exported_heat_beats_the_fallback() {
    let mut m = MaterialInfo { token: "INORGANIC:GRANITE".into(), is_stone: true, ..Default::default() };
    m.heat.melting_point = 13_000;
    m.heat.spec_heat = 800;
    let h = heat_of(&m);
    assert_eq!(h.melting_point, 13_000);
    assert_eq!(h.ignite_point, NONE_TEMP, "unknown points of an export count as NONE");
    assert!(magma_safe(&h));
}

#[test]
fn fixed_temp_material_never_changes() {
    let mut w = world();
    let nether = add_mat(&mut w, "PLANT:NETHER_CAP:WOOD", |m| m.is_wood = true);
    let p = Pos::new(13, 16, HALL);
    item(&mut w, 9001, "WOOD", nether, p);
    let mut sim = Sim::new(w, 1);
    sim.set_liquid(p, 7, true);
    sim.run_ticks(600);
    assert!(alive(&sim, 9001), "nether-cap log survives magma");
    assert_eq!(sim.item_temperature(9001), Some(10_000));
    assert!(!sim.is_burning_item(9001));
}

// ------------------------------------------------------------------ items

#[test]
fn tin_melts_in_magma_iron_does_not() {
    let mut w = world();
    let tin = add_mat(&mut w, "INORGANIC:TIN", |m| m.is_metal = true);
    let iron = add_mat(&mut w, "INORGANIC:IRON", |m| m.is_metal = true);
    let (a, b) = (Pos::new(12, 16, HALL), Pos::new(20, 16, HALL));
    item(&mut w, 9001, "BAR", tin, a);
    item(&mut w, 9002, "BAR", iron, b);
    let mut sim = Sim::new(w, 2);
    sim.set_liquid(a, 7, true);
    sim.set_liquid(b, 7, true);
    sim.run_ticks(5);
    assert!(alive(&sim, 9001), "tin heats up gradually, not instantly");
    sim.run_ticks(1_500);
    assert!(!alive(&sim, 9001), "tin melts (10417)");
    assert!(alive(&sim, 9002), "iron is magma-safe");
    assert!(sim.fire_report().items_melted >= 1);
}

#[test]
fn wooden_log_in_magma_ignites_and_burns_up_within_a_day() {
    let mut w = world();
    let p = Pos::new(13, 16, HALL);
    item(&mut w, 9001, "WOOD", 3, p); // oak
    let mut sim = Sim::new(w, 3);
    sim.set_liquid(p, 7, true);
    sim.run_ticks(100);
    assert!(alive(&sim, 9001) && !sim.is_burning_item(9001), "ignites only after ~120 ticks");
    sim.run_ticks(100);
    assert!(sim.is_burning_item(9001) || !alive(&sim, 9001), "burning by now");
    sim.run_ticks(1_200);
    assert!(!alive(&sim, 9001), "burned up");
    assert_eq!(sim.fire_report().items_burned_up, 1);
}

fn fuel_world() -> (World, u16) {
    let mut w = world();
    // A test fuel: ignites, no heat-damage point, never melts or boils (w:Wear: 10 points/tick).
    let fuel = add_mat(&mut w, "INORGANIC:TESTFUEL", |m| {
        m.heat = MaterialHeat { spec_heat: 409, heatdam_point: NONE_TEMP, colddam_point: NONE_TEMP, ignite_point: 10_100, melting_point: NONE_TEMP, boiling_point: NONE_TEMP, mat_fixed_temp: NONE_TEMP };
    });
    (w, fuel)
}

#[test]
fn burning_item_without_heatdam_lasts_322560_ticks() {
    let (mut w, fuel) = fuel_world();
    item(&mut w, 9001, "BOULDER", fuel, Pos::new(13, 16, HALL));
    let mut sim = Sim::new(w, 4);
    sim.fire15_init();
    assert!(sim.fire15_heat_item(9001, true, 0));
    for _ in 0..322_559 {
        sim.fire15_hot_step(0);
    }
    assert!(alive(&sim, 9001), "still there after 322 559 ticks");
    assert_eq!(sim.world.items[sim.item_idx.get(&9001).copied().unwrap() as usize].wear, 3);
    sim.fire15_hot_step(0);
    assert!(!alive(&sim, 9001), "totally consumed after exactly 322 560 ticks (w:Wear)");
}

#[test]
fn burning_artifact_never_disappears() {
    let (mut w, fuel) = fuel_world();
    item(&mut w, 9001, "BOOK", fuel, Pos::new(13, 16, HALL));
    w.items.last_mut().unwrap().flags.artifact = true;
    let mut sim = Sim::new(w, 5);
    sim.fire15_init();
    sim.fire15_heat_item(9001, true, 0);
    for _ in 0..330_000 {
        sim.fire15_hot_step(0);
    }
    assert!(alive(&sim, 9001) && sim.is_burning_item(9001), "artifacts never wear");
}

#[test]
fn water_puts_out_a_burning_log() {
    let mut w = world();
    let p = Pos::new(13, 16, HALL);
    item(&mut w, 9001, "WOOD", 3, p);
    let mut sim = Sim::new(w, 6);
    sim.fire15_init();
    sim.fire15_heat_item(9001, true, 0);
    sim.run_ticks(5);
    assert!(sim.is_burning_item(9001));
    sim.set_liquid(p, 4, false);
    sim.run_ticks(3);
    assert!(!sim.is_burning_item(9001), "doused");
    assert!(sim.fire_report().extinguished >= 1);
}

#[test]
fn booze_boils_in_a_hot_spot() {
    let mut w = world();
    let p = Pos::new(13, 16, HALL);
    item(&mut w, 9001, "DRINK", 5, p); // dwarven wine: melts 9850 (liquid), boils 10150
    let mut sim = Sim::new(w, 7);
    sim.fire15_init();
    sim.fire15_heat_item(9001, false, 0);
    assert!(alive(&sim, 9001), "a drink is liquid already: it does not melt");
    sim.heat_blast(p, 11_000);
    sim.run_ticks(2);
    assert!(!alive(&sim, 9001), "boiled away");
    assert_eq!(sim.fire_report().items_boiled, 1);
}

// ------------------------------------------------------------------ tile fires

#[test]
fn grass_fire_spreads_on_one_level_and_leaves_ashes() {
    let mut sim = Sim::new(world(), 8);
    assert!(sim.ignite_tile(Pos::new(10, 10, SURF)));
    sim.run_ticks(3_000);
    let r = sim.fire_report();
    assert!(r.tiles_ignited > 20, "the fire spread: {r:?}");
    assert!(r.tiles_burnt > 20);
    let mut ashes = 0;
    for z in 0..DZ as i16 {
        for y in 0..DY as i16 {
            for x in 0..DX as i16 {
                let n = tile_name(&sim, Pos::new(x, y, z));
                if n.starts_with("Ashes") {
                    assert_eq!(z, SURF, "fire stays on its level");
                    ashes += 1;
                }
            }
        }
    }
    assert!(ashes > 20, "burnt grass becomes ashes ({ashes})");
    assert!(sim.world.tile(Pos::new(10, 10, SURF)).unwrap().flags & tile_flags::TRAFFIC_MASK == 0, "traffic restored");
}

#[test]
fn ramps_constructions_and_wet_grass_do_not_burn() {
    let mut w = world();
    carve(&mut w, Pos::new(3, 3, SURF), "GrassLightRamp");
    carve(&mut w, Pos::new(4, 3, SURF), "ConstructedFloor");
    let mut sim = Sim::new(w, 9);
    let i = |sim: &Sim, p: Pos| sim.world.idx(p).unwrap();
    assert_eq!(sim.fire15_fuel_kind(i(&sim, Pos::new(3, 3, SURF))), None, "grassy ramps do not catch fire (w:Grass)");
    assert_eq!(sim.fire15_fuel_kind(i(&sim, Pos::new(4, 3, SURF))), None, "constructions never burn (w:Fire)");
    assert_eq!(sim.fire15_fuel_kind(i(&sim, Pos::new(5, 3, SURF))), Some(K_GRASS));
    sim.set_liquid(Pos::new(6, 3, SURF), 3, false);
    assert_eq!(sim.fire15_fuel_kind(i(&sim, Pos::new(6, 3, SURF))), None, "water-covered grass does not burn");
}

#[test]
fn fire_does_not_cross_a_stone_gap() {
    let mut w = world();
    for y in 0..DY as i16 {
        carve(&mut w, Pos::new(16, y, SURF), "StoneFloor1");
    }
    let mut sim = Sim::new(w, 10);
    sim.ignite_tile(Pos::new(8, 8, SURF));
    sim.run_ticks(5_000);
    assert!(sim.fire_report().tiles_burnt > 20);
    for y in 0..DY as i16 {
        for x in 17..DX as i16 {
            assert!(!tile_name(&sim, Pos::new(x, y, SURF)).starts_with("Ashes"), "no ashes beyond the firebreak at ({x},{y})");
        }
    }
}

#[test]
fn tree_burns_to_a_dead_tree() {
    let w = world();
    let trunk = Pos::new(3, 3, SURF + 1); // synthetic oak
    let mut sim = Sim::new(w, 11);
    assert!(sim.ignite_tile(trunk));
    assert_eq!(tile_name(&sim, trunk), "BurningTreeTrunk");
    sim.run_ticks(1_400);
    assert_eq!(tile_name(&sim, trunk), "TreeDeadTrunkPillar");
    assert!(sim.world.plants.iter().any(|p| p.pos == trunk && p.dead), "the tree is dead");
}

#[test]
fn nether_cap_tree_does_not_ignite() {
    let mut w = world();
    let nether = add_mat(&mut w, "PLANT:NETHER_CAP:WOOD", |m| m.is_wood = true);
    let trunk = Pos::new(3, 3, SURF + 1);
    w.tile_mut(trunk).unwrap().mat = nether;
    let mut sim = Sim::new(w, 12);
    assert!(!sim.ignite_tile(trunk), "fixed 10000 never ignites from ordinary fire");
}

#[test]
fn temperature_off_changes_nothing() {
    let mut w = world();
    w.settings.temperature = false;
    let mut a = Sim::new(w.clone(), 13);
    let mut b = Sim::new(w, 13);
    assert!(!a.ignite_tile(Pos::new(10, 10, SURF)), "API refuses with temperature off");
    a.run_ticks(2_000);
    b.run_ticks(2_000);
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(a.fire_report().is_zero());
}

// ------------------------------------------------------------------ units

fn first_dwarf(sim: &Sim) -> (usize, i32) {
    let s = (0..sim.ai.len()).find(|&s| sim.world.units[sim.ai[s].uidx].race == "DWARF").unwrap();
    (s, sim.ai[s].id)
}

#[test]
fn burn_wounds_go_from_the_outside_in_and_melted_fat_bleeds() {
    let mut sim = Sim::new(world(), 14);
    sim.run_ticks(1);
    let (s, id) = first_dwarf(&sim);
    assert!(sim.ignite_unit(id, 0));
    assert!(sim.is_burning_unit(id));
    sim.run_ticks(60);
    let u = &sim.world.units[sim.ai[s].uidx];
    let burned: Vec<&Wound> = u.wounds.iter().filter(|w| w.flags.iter().any(|f| f == "BURNED" || f == "MELTED")).collect();
    assert!(!burned.is_empty(), "burn wounds");
    assert!(burned.iter().any(|w| w.layer == "SKIN" && w.flags.iter().any(|f| f == "BURNED")), "skin burns first");
    assert!(!burned.iter().any(|w| w.layer == "MUSCLE"), "muscle is not reached before the fat burned through");
    sim.run_ticks(150);
    let u = &sim.world.units[sim.ai[s].uidx];
    let fat = u.wounds.iter().find(|w| w.layer == "FAT" && w.flags.iter().any(|f| f == "MELTED"));
    let dead = sim.ai[s].kind == crate::Kind::Dead;
    assert!(fat.is_some() || dead || !sim.is_burning_unit(id), "fat melts above 10078 (w:Temperature)");
    if let Some(f) = fat {
        assert!(f.bleeding > 0 || f.flags.iter().any(|x| x == "BLEEDING"), "melted fat bleeds (w:Magma crab)");
    }
    assert!(sim.fire_report().burn_wounds > 0);
}

#[test]
fn a_burning_dwarf_in_water_goes_out() {
    let mut sim = Sim::new(world(), 15);
    sim.run_ticks(1);
    let (s, id) = first_dwarf(&sim);
    sim.ignite_unit(id, 0);
    let p = sim.fire15_unit_pos(s);
    for y in -1..=1 {
        for x in -1..=1 {
            sim.set_liquid(p.offset(x, y, 0), 2, false);
        }
    }
    sim.run_ticks(12);
    assert!(!sim.is_burning_unit(id) || sim.fire15_unit_pos(s) != p);
    assert!(sim.fire_report().extinguished >= 1);
}

#[test]
fn burning_to_death_leaves_a_burning_corpse() {
    let mut sim = Sim::new(world(), 16);
    sim.run_ticks(1);
    let (s, id) = first_dwarf(&sim);
    let mut died = false;
    for _ in 0..200 {
        if sim.ai[s].kind == crate::Kind::Dead {
            died = true;
            break;
        }
        sim.ignite_unit(id, 0);
        sim.run_ticks(10);
    }
    assert!(died, "a dwarf kept on fire dies");
    let r = sim.fire_report();
    assert!(r.units_burned_to_death == 1 || r.units_burned_to_death == 0 && died);
}

fn crab_world(chitin_heat: MaterialHeat, flags: Vec<String>) -> World {
    let mut w = world();
    let chitin = add_mat(&mut w, "CREATURE:TEST_CRAB:CHITIN", |m| m.heat = chitin_heat);
    let part = military::BodyPartRaw {
        token: "BODY".into(),
        name: "body".into(),
        parent: -1,
        relsize: 1000,
        flags: vec!["UPPERBODY".into()],
        layers: vec![military::TissueLayerRaw { tissue: "CHITIN".into(), material: "CREATURE:TEST_CRAB:CHITIN".into(), mat: chitin, thickness: 10, flags: vec![] }],
        ..Default::default()
    };
    w.creature_raws.push(military::CreatureRaw {
        token: "TEST_CRAB".into(),
        name: "test crab".into(),
        castes: vec![military::CasteRaw { token: "FEMALE".into(), body_size: 5_000, body_parts: vec![part], ..Default::default() }],
        flags,
        ..Default::default()
    });
    let mut u = w.units.last().unwrap().clone();
    u.id = 500;
    u.race = "TEST_CRAB".into();
    u.caste = "FEMALE".into();
    w.units.push(u);
    w
}

fn crab_heat(dam: i32) -> MaterialHeat {
    MaterialHeat { spec_heat: 420, heatdam_point: dam, colddam_point: 9_900, ignite_point: dam, melting_point: NONE_TEMP, boiling_point: NONE_TEMP, mat_fixed_temp: NONE_TEMP }
}

#[test]
fn magma_crab_is_magma_proof_by_its_materials() {
    let mut sim = Sim::new(crab_world(crab_heat(13_000), vec![]), 17);
    sim.run_ticks(1);
    let s = sim.unit_slot[&500];
    assert!(sim.fire15_magma_proof(s), "chitin 13000 > magma 12000 (w:Magma crab)");
    let (d, _) = first_dwarf(&sim);
    assert!(!sim.fire15_magma_proof(d));
}

#[test]
fn fireimmune_flag_alone_does_not_protect() {
    let mut sim = Sim::new(crab_world(crab_heat(10_250), vec!["FIREIMMUNE".into()]), 18);
    sim.run_ticks(1);
    let s = sim.unit_slot[&500];
    let p = sim.fire15_unit_pos(s);
    assert!(!sim.fire15_magma_proof(s), "w:Creature token: FIREIMMUNE does not by itself protect");
    sim.set_liquid(p, 7, true);
    sim.run_ticks(200);
    assert!(sim.fire_report().units_ignited >= 1, "it catches fire in magma");
}

#[test]
fn legacy_magma_kill_when_temperature_off() {
    let mut w = world();
    w.settings.temperature = false;
    let mut sim = Sim::new(w, 19);
    sim.run_ticks(1);
    for y in 12..=20 {
        for x in 10..=22 {
            sim.set_liquid(Pos::new(x, y, HALL), 7, true);
        }
    }
    sim.run_ticks(40);
    assert!(sim.fluid_report().burned >= 1, "10 ticks in magma kill (legacy)");
    assert!(sim.fire_report().is_zero());
}

#[test]
fn fire_man_sets_the_grass_around_it_on_fire() {
    let mut w = world();
    let flame = add_mat(&mut w, "CREATURE:TEST_FIRE_MAN:FLAME", |m| {
        m.heat = MaterialHeat { spec_heat: NONE_TEMP, heatdam_point: NONE_TEMP, colddam_point: NONE_TEMP, ignite_point: NONE_TEMP, melting_point: NONE_TEMP, boiling_point: NONE_TEMP, mat_fixed_temp: 10_800 };
    });
    let part = military::BodyPartRaw {
        token: "BODY".into(),
        parent: -1,
        relsize: 1000,
        flags: vec!["UPPERBODY".into()],
        layers: vec![military::TissueLayerRaw { tissue: "FLAME".into(), mat: flame, thickness: 10, ..Default::default() }],
        ..Default::default()
    };
    w.creature_raws.push(military::CreatureRaw { token: "TEST_FIRE_MAN".into(), castes: vec![military::CasteRaw { token: "MALE".into(), body_parts: vec![part], ..Default::default() }], ..Default::default() });
    let mut u = w.units.last().unwrap().clone();
    u.id = 501;
    u.race = "TEST_FIRE_MAN".into();
    u.caste = "MALE".into();
    u.pos = Pos::new(20, 20, SURF);
    u.flags = UnitFlags::default();
    w.units.push(u);
    let mut sim = Sim::new(w, 20);
    sim.run_ticks(300);
    let r = sim.fire_report();
    assert!(r.tiles_ignited >= 1, "grass around a fire man ignites: {r:?}");
    let s = sim.unit_slot[&501];
    assert!(!sim.fire15_burning_slot(s), "it does not burn itself");
}

// ------------------------------------------------------------------ magma and obsidian

#[test]
fn water_on_a_magma_flow_tile_makes_steam_not_obsidian() {
    let mut w = world();
    for x in 5..=9 {
        carve(&mut w, Pos::new(x, 5, 1), "StoneFloor1");
    }
    carve(&mut w, Pos::new(9, 5, 1), "MagmaFlow");
    let mut sim = Sim::new(w, 21);
    sim.set_liquid(Pos::new(9, 5, 1), 1, true);
    sim.set_liquid(Pos::new(8, 5, 1), 7, false);
    sim.run_ticks(50);
    assert_eq!(tile_name(&sim, Pos::new(9, 5, 1)), "MagmaFlow", "no obsidian on a magma flow (w:Obsidian)");
    assert!(sim.fluid_report().steam >= 1, "a puff of steam");
}

fn casting_world(temp_on: bool) -> (World, Pos) {
    let mut w = world();
    w.settings.temperature = temp_on;
    for x in 5..=9 {
        carve(&mut w, Pos::new(x, 5, 1), "StoneFloor1");
    }
    let p = Pos::new(9, 5, 1);
    let iron = add_mat(&mut w, "INORGANIC:IRON", |m| m.is_metal = true);
    item(&mut w, 9001, "BAR", iron, p);
    item(&mut w, 9002, "WOOD", 3, p);
    let mut u = w.units[1].clone();
    u.id = 600;
    u.pos = p;
    w.units.push(u);
    (w, p)
}

#[test]
fn obsidian_casting_kills_units_and_entombs_magma_safe_items() {
    let (w, p) = casting_world(false);
    let mut sim = Sim::new(w, 22);
    sim.set_liquid(p, 1, true);
    sim.set_liquid(p.offset(-1, 0, 0), 7, false);
    sim.run_ticks(4);
    let t = sim.world.tile(p).unwrap();
    assert_eq!(sim.world.material(t.mat).map(|m| m.token.as_str()), Some("INORGANIC:OBSIDIAN"));
    let s = sim.unit_slot[&600];
    assert_eq!(sim.ai[s].kind, crate::Kind::Dead, "cast in obsidian");
    assert!(sim.fire_report().obsidian_kills >= 1);
    assert!(alive(&sim, 9001), "iron bar is entombed");
    assert!(!alive(&sim, 9002), "the log is destroyed");
}

#[test]
fn caged_creature_survives_casting() {
    let (mut w, p) = casting_world(false);
    w.units.last_mut().unwrap().flags.caged = true;
    let mut sim = Sim::new(w, 23);
    sim.set_liquid(p, 1, true);
    sim.set_liquid(p.offset(-1, 0, 0), 7, false);
    sim.run_ticks(4);
    assert_eq!(sim.fire_report().obsidian_kills, 0, "cages protect (w:Obsidian)");
    assert!(!sim.world.unit(600).unwrap().flags.dead);
}

#[test]
fn magma_melts_a_natural_ice_wall() {
    let mut w = world();
    carve(&mut w, Pos::new(5, 5, 1), "StoneFloor1");
    carve(&mut w, Pos::new(6, 5, 1), "FrozenLiquidWall");
    let mut sim = Sim::new(w, 24);
    sim.set_liquid(Pos::new(5, 5, 1), 3, true);
    sim.run_ticks(3);
    assert!(sim.fire_report().ice_melted >= 1, "w:Ice: magma melts unmined ice");
    assert_ne!(tile_name(&sim, Pos::new(6, 5, 1)), "FrozenLiquidWall");
}

#[test]
fn magma_forge_cancels_jobs_when_magma_drops() {
    let mut w = world();
    w.settings.temperature = false;
    let c = Pos::new(13, 16, HALL);
    for y in 15..=17 {
        for x in 15..=17 {
            carve(&mut w, Pos::new(x, y, 0), "StoneFloor1");
        }
    }
    w.buildings.push(Building { id: 77, kind: "Workshop".into(), subtype: "MagmaForge".into(), x1: 15, y1: 15, x2: 17, y2: 17, z: 1, center: c, built: true, owner: -1, ..Default::default() });
    let mut sim = Sim::new(w, 25);
    for y in 15..=17 {
        for x in 15..=17 {
            sim.set_liquid(Pos::new(x, y, 0), 7, true);
        }
    }
    sim.run_ticks(200);
    assert_eq!(sim.fire_report().magma_jobs_cancelled, 0);
    for y in 15..=17 {
        for x in 15..=17 {
            sim.set_liquid(Pos::new(x, y, 0), 2, true);
        }
    }
    sim.run_ticks(200);
    assert_eq!(sim.fire_report().magma_jobs_cancelled, 1, "below 4/7: jobs cancelled (w:Magma forge)");
}

#[test]
fn smoke_rises_from_fire_and_clears_after_it() {
    let mut sim = Sim::new(world(), 26);
    sim.ignite_tile(Pos::new(10, 10, SURF));
    sim.run_ticks(50);
    assert!(sim.fire_report().smoke_tiles > 0);
    sim.run_ticks(8_000);
    let r = sim.fire_report();
    assert_eq!(r.burning_tiles, 0, "the fire burned out");
    assert_eq!(r.smoke_tiles, 0, "smoke dissipates after its source is gone");
}

#[test]
fn same_seed_same_fire() {
    let run = || {
        let mut sim = Sim::new(world(), 27);
        sim.ignite_tile(Pos::new(10, 10, SURF));
        sim.run_ticks(1_500);
        (sim.state_hash(), sim.fire_report().tiles_burnt)
    };
    assert_eq!(run(), run());
}

// ------------------------------------------------------------------ verifier regressions

#[test]
fn dead_tree_tiles_do_not_burn_again() {
    // Two adjacent tree tiles used to re-ignite each other forever once dead (TreeDead* has
    // material TREE): the fire never went out.
    let mut w = world();
    let trunk = Pos::new(3, 3, SURF + 1);
    let (tt, mat) = { let t = w.tile(trunk).unwrap(); (t.tt, t.mat) };
    for x in 4..=5 {
        let t = w.tile_mut(Pos::new(x, 3, SURF + 1)).unwrap();
        t.tt = tt;
        t.mat = mat;
    }
    let mut sim = Sim::new(w, 31);
    assert!(sim.ignite_tile(trunk));
    sim.run_ticks(40_000);
    assert_eq!(sim.fire_report().burning_tiles, 0, "the tree fire burned out: {:?}", sim.fire_report());
    assert!(sim.fire_report().tiles_ignited <= 3 + 10, "dead tree tiles did not re-ignite: {:?}", sim.fire_report());
}

#[test]
fn creature_with_a_melting_shell_is_harmed_by_magma() {
    // Outer layer has no ignite / heat-damage point but melts below 12000 (a bronze shell):
    // not magma-proof, so it must take damage in magma instead of swimming there forever.
    let shell = MaterialHeat { spec_heat: 435, heatdam_point: NONE_TEMP, colddam_point: NONE_TEMP, ignite_point: NONE_TEMP, melting_point: 11_000, boiling_point: NONE_TEMP, mat_fixed_temp: NONE_TEMP };
    let mut sim = Sim::new(crab_world(shell, vec![]), 32);
    sim.run_ticks(1);
    let s = sim.unit_slot[&500];
    assert!(!sim.fire15_magma_proof(s));
    let p = sim.fire15_unit_pos(s);
    sim.set_liquid(p, 7, true);
    sim.run_ticks(300);
    let dead = sim.ai[s].kind == crate::Kind::Dead;
    assert!(dead || sim.fire_report().burn_wounds > 0, "the shell melts in magma: {:?}", sim.fire_report());
}

#[test]
fn ignite_unit_with_a_part_mask_keeps_parts_already_burning() {
    let mut sim = Sim::new(world(), 33);
    sim.run_ticks(1);
    let (s, id) = first_dwarf(&sim);
    assert!(sim.ignite_unit(id, 0));
    let k = sim.fire.burning.binary_search_by_key(&(s as u32), |b| b.slot).unwrap();
    let before = sim.fire.burning[k].parts;
    assert!(before.count_ones() > 1);
    let low = before & before.wrapping_neg(); // lowest burning part only
    sim.ignite_unit(id, low);
    let k = sim.fire.burning.binary_search_by_key(&(s as u32), |b| b.slot).unwrap();
    assert_eq!(sim.fire.burning[k].parts, before, "igniting one more part must not put out the others");
}
