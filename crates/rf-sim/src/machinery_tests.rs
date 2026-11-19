//! [machines14] Tests of the power port rules and pump details (wiki Power, Screw pump,
//! Windmill, Water wheel, Roller).

use super::*;
use rf_core::{tile_flags, Building, World};

fn bld(id: i32, kind: &str, x1: i16, y1: i16, x2: i16, y2: i16, z: i16, dir: &str) -> Building {
    Building {
        id,
        kind: kind.into(),
        x1,
        y1,
        x2,
        y2,
        z,
        center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, z),
        stage: -1,
        built: true,
        owner: -1,
        direction: dir.into(),
        ..Default::default()
    }
}

fn sim() -> (Sim, i16) {
    let mut w = World::synthetic(48, 48, 20);
    for u in &mut w.units {
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
    }
    let z = w.units[0].pos.z;
    let mut sim = Sim::new(w, 61);
    sim.set_invasions(false);
    sim.set_moods_enabled(false);
    (sim, z)
}

fn net(sim: &Sim, bid: i32) -> u32 {
    sim.machine_net(bid).unwrap()
}

fn set_tt(sim: &mut Sim, p: Pos, name: &str) {
    let tt = sim.world.tiletypes.by_name(name).unwrap();
    let i = sim.world.idx(p).unwrap();
    sim.world.tiles[i].tt = tt;
    sim.after_tile_change(i);
}

#[test]
fn pump_stack_connects_vertically_and_floors_block() {
    let (mut sim, z) = sim();
    for x in 10..=11 {
        set_tt(&mut sim, Pos::new(x, 10, z + 1), "OpenSpace");
    }
    sim.world.buildings.push(bld(9301, "ScrewPump", 10, 10, 11, 10, z, "FromWest"));
    sim.world.buildings.push(bld(9302, "ScrewPump", 10, 10, 11, 10, z + 1, "FromWest"));
    sim.power_rebuild();
    assert_eq!(net(&sim, 9301), net(&sim, 9302), "a pump stack is one network");
    for x in 10..=11 {
        set_tt(&mut sim, Pos::new(x, 10, z + 1), "StoneFloor1");
    }
    sim.power_rebuild();
    assert_ne!(net(&sim, 9301), net(&sim, 9302), "a floor between the levels blocks power");
}

#[test]
fn windmill_powers_through_side_ring_and_water_wheel_center_side_port() {
    let (mut sim, z) = sim();
    sim.world.buildings.push(bld(9401, "Windmill", 10, 10, 12, 12, z, ""));
    sim.world.buildings.push(bld(9402, "GearAssembly", 13, 11, 13, 11, z, ""));
    sim.world.buildings.push(bld(9403, "WaterWheel", 20, 30, 22, 30, z, ""));
    sim.world.buildings.push(bld(9404, "GearAssembly", 21, 31, 21, 31, z, ""));
    sim.power_rebuild();
    assert_eq!(net(&sim, 9401), net(&sim, 9402), "the 12 tiles around a windmill connect");
    assert_eq!(net(&sim, 9403), net(&sim, 9404), "the tile beside a wheel's centre connects");
}

#[test]
fn long_roller_connects_only_across() {
    let (mut sim, z) = sim();
    sim.world.buildings.push(bld(9501, "Rollers", 10, 20, 12, 20, z, "E"));
    sim.world.buildings.push(bld(9502, "GearAssembly", 13, 20, 13, 20, z, ""));
    sim.world.buildings.push(bld(9503, "GearAssembly", 11, 21, 11, 21, z, ""));
    sim.power_rebuild();
    assert_ne!(net(&sim, 9501), net(&sim, 9502), "not along the roller");
    assert_eq!(net(&sim, 9501), net(&sim, 9503), "across it");
}

#[test]
fn wheel_needs_four_sevenths() {
    let (mut sim, z) = sim();
    let below = z - 1;
    sim.world.buildings.push(bld(9101, "WaterWheel", 20, 30, 22, 30, z, ""));
    sim.world.buildings.push(bld(9102, "GearAssembly", 23, 30, 23, 30, z, ""));
    sim.power_rebuild();
    let p = Pos::new(21, 30, below);
    let t = sim.world.idx(p).unwrap() as u32;
    sim.set_liquid(p, 3, false);
    let now = sim.ticks + 1000;
    sim.power_flow_seen(t, now);
    sim.power_balance(now + 1);
    assert_eq!(sim.power.nets[net(&sim, 9101) as usize].produced, 0, "3/7 turns nothing");
    sim.set_liquid(p, 4, false);
    sim.power_flow_seen(t, now + 2);
    sim.power_balance(now + 3);
    assert_eq!(sim.power.nets[net(&sim, 9101) as usize].produced, 100, "4/7 flowing makes 100");
}

#[test]
fn pump_run_on_order_obsidian_and_clean_water() {
    let (mut sim, z) = sim();
    let below = z - 1;
    sim.world.buildings.push(bld(9101, "WaterWheel", 20, 30, 22, 30, z, ""));
    sim.world.buildings.push(bld(9102, "GearAssembly", 23, 30, 23, 30, z, ""));
    sim.world.buildings.push(bld(9103, "ScrewPump", 24, 30, 25, 30, z, "FromWest"));
    sim.world.buildings.push(bld(9104, "ScrewPump", 24, 34, 25, 34, z, "FromWest"));
    sim.power_rebuild();
    let order: Vec<i32> = sim.power.pump_order.iter().map(|&k| sim.power.machines[k as usize].bid).collect();
    assert_eq!(order, vec![9104, 9103], "pumps run newest first");
    let p = Pos::new(21, 30, below);
    let t = sim.world.idx(p).unwrap() as u32;
    sim.set_liquid(p, 7, false);
    let now = sim.ticks + 1000;
    sim.power_flow_seen(t, now);
    sim.power_balance(now + 1);
    assert!(sim.machine_powered(9103));
    let ru = sim.power.run_until.iter().find(|e| e.0 == 9103).unwrap().1;
    assert_eq!(ru, now + 1 + 100 + PUMP_RUN_ON, "a powered pump keeps 49 ticks of run-on past the next balance");
    sim.set_liquid(p, 0, false);
    sim.power_balance(now + 101);
    assert!(!sim.machine_powered(9103));
    let ru2 = sim.power.run_until.iter().find(|e| e.0 == 9103).unwrap().1;
    assert!(sim.pump_running_on(9103, now + 101 + PUMP_RUN_ON) && !sim.pump_running_on(9103, now + 102 + PUMP_RUN_ON));
    assert_eq!(ru2, ru, "an unpowered balance keeps the run-on");
    // Pumping into the other liquid makes obsidian; pumped water comes out clean.
    let (a, b) = (sim.world.idx(Pos::new(19, 21, z)).unwrap(), sim.world.idx(Pos::new(21, 21, z)).unwrap());
    sim.set_liquid(Pos::new(19, 21, z), 7, false);
    sim.set_liquid(Pos::new(21, 21, z), 3, true);
    let obs = sim.fluid_report().obsidian;
    sim.pump_step(a, b);
    assert_eq!(sim.fluid_report().obsidian, obs + 1, "water onto magma: obsidian");
    let (c, d) = (sim.world.idx(Pos::new(19, 27, z)).unwrap(), sim.world.idx(Pos::new(21, 27, z)).unwrap());
    sim.set_liquid(Pos::new(19, 27, z), 7, false);
    sim.world.tiles[c].flags |= tile_flags::SALT | tile_flags::STAGNANT;
    sim.pump_step(c, d);
    assert!(sim.liquid_at(Pos::new(21, 27, z)).0 > 0);
    assert_eq!(sim.world.tiles[d].flags & (tile_flags::SALT | tile_flags::STAGNANT), 0, "pumped water is clean");
}

#[test]
fn ui_built_machines_get_constructed() {
    let mut w = World::synthetic(48, 48, 20);
    for u in &mut w.units {
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
    }
    w.units[0].labors.push("MECHANIC".into());
    let z = w.units[0].pos.z;
    let mut sim = Sim::new(w, 61);
    sim.set_invasions(false);
    sim.set_moods_enabled(false);
    let opts = sim.build_options();
    let find = |k: &str, sub: &str| opts.iter().position(|o| o.kind == k && o.subtype == sub).unwrap();
    for (k, sub) in [("Trap", "Lever"), ("Trap", "PressurePlate"), ("Bridge", ""), ("Floodgate", ""), ("Hatch", ""), ("GrateWall", ""), ("Support", "")] {
        assert!(opts.iter().any(|o| o.kind == k && o.subtype == sub), "build option {k}/{sub}");
    }
    let t = sim.world.idx(Pos::new(20, 21, z)).unwrap();
    for _ in 0..2 {
        sim.create_item("TRAPPARTS", 0, t, "granite mechanism".into());
    }
    let gear = sim.place_building_dir(find("GearAssembly", ""), Pos::new(21, 23, z), Pos::new(21, 23, z), "").unwrap();
    let lever = sim.place_building_dir(find("Trap", "Lever"), Pos::new(19, 23, z), Pos::new(19, 23, z), "").unwrap();
    for _ in 0..80 {
        sim.run_ticks(100);
        if sim.world.building(gear).unwrap().built && sim.world.building(lever).unwrap().built {
            break;
        }
    }
    assert!(sim.world.building(gear).unwrap().built, "a mechanic built the gear assembly (F9)");
    assert!(sim.world.building(lever).unwrap().built, "and the lever");
    sim.run_ticks(2);
    assert!(sim.machine_net(gear).is_some(), "the built gear is a machine");
    assert!(sim.mechanisms().iter().any(|m| m.0 == lever), "the built lever is a mechanism");
}
