//! [fluids] Liquid simulation tests on the synthetic fort (48x48x20: surface z13, soil z11-12,
//! stone below, hall at z9 around (24,24)).

use rf_core::{tile_flags, Building, Pos, Shape, Skill, World};
use rf_sim::{AnnouncementKind, DesignationKind, Sim};

const DX: u16 = 48;
const DY: u16 = 48;
const DZ: u16 = 20;

fn world() -> World {
    World::synthetic(DX, DY, DZ)
}

fn carve(w: &mut World, p: Pos, name: &str) {
    let id = w.tiletypes.by_name(name).expect(name);
    let t = w.tile_mut(p).unwrap();
    t.tt = id;
    t.mat = 0;
    t.flags &= !tile_flags::HIDDEN;
}

fn depth(sim: &Sim, x: i16, y: i16, z: i16) -> u8 {
    sim.liquid_at(Pos::new(x, y, z)).0
}

/// 5x5 stone room at z5 (x,y 5..9) with an open shaft above its centre at z6.
fn pit_world() -> World {
    let mut w = world();
    for y in 5..=9 {
        for x in 5..=9 {
            carve(&mut w, Pos::new(x, y, 5), "StoneFloor1");
        }
    }
    carve(&mut w, Pos::new(7, 7, 6), "OpenSpace");
    w
}

#[test]
fn poured_water_falls_spreads_and_levels() {
    let mut sim = Sim::new(pit_world(), 1);
    for _ in 0..9 {
        sim.set_liquid(Pos::new(7, 7, 6), 7, false);
        sim.run_ticks(4);
    }
    assert_eq!(depth(&sim, 7, 7, 6), 0, "the shaft drains into the pit");
    sim.run_ticks(600);
    let r = sim.fluid_report();
    assert_eq!(r.water_volume, 63, "volume is conserved underground");
    assert_eq!(r.flowing, 0, "the pool comes to rest");
    let mut lo = 7;
    let mut hi = 0;
    for y in 5..=9 {
        for x in 5..=9 {
            let d = depth(&sim, x, y, 5);
            lo = lo.min(d);
            hi = hi.max(d);
            for (ox, oy) in [(1, 0), (0, 1), (1, 1), (-1, 1)] {
                let (nx, ny) = (x + ox, y + oy);
                if (5..=9).contains(&nx) && (5..=9).contains(&ny) {
                    let nd = depth(&sim, nx, ny, 5);
                    assert!((d as i32 - nd as i32).abs() <= 1, "neighbours differ by at most 1/7 ({x},{y})={d} ({nx},{ny})={nd}");
                }
            }
        }
    }
    assert!(lo >= 1 && hi - lo <= 2, "level pool: min {lo} max {hi}");
    // Settled water costs nothing: no updates while nothing changes.
    let u = sim.fluid_report().updates;
    sim.run_ticks(1000);
    assert_eq!(sim.fluid_report().updates, u, "settled water is not updated");
}

/// U-bend: tunnel at z2 (x 5..15, y 30), column A (x5) open z3..8, column B (x15) open z3..9.
fn ubend_world() -> World {
    let mut w = world();
    for x in 5..=15 {
        carve(&mut w, Pos::new(x, 30, 2), "StoneFloor1");
    }
    for z in 3..=8 {
        carve(&mut w, Pos::new(5, 30, z), "OpenSpace");
    }
    for z in 3..=9 {
        carve(&mut w, Pos::new(15, 30, z), "OpenSpace");
    }
    w
}

#[test]
fn pressure_fills_a_u_bend_to_one_level_below_the_source() {
    let mut sim = Sim::new(ubend_world(), 2);
    sim.add_liquid_source(Pos::new(5, 30, 8), false);
    sim.run_ticks(3000);
    for z in 2..=7 {
        assert_eq!(depth(&sim, 15, 30, z), 7, "far leg full at z{z}");
    }
    assert_eq!(depth(&sim, 15, 30, 8), 0, "water rises to one level below the source, not to its level");
    assert_eq!(depth(&sim, 15, 30, 9), 0);
    let r = sim.fluid_report();
    assert!(r.pressure_moves > 0, "pressure moved water: {r:?}");
}

#[test]
fn without_pressure_magma_stays_at_the_bottom_of_a_u_bend() {
    let mut sim = Sim::new(ubend_world(), 2);
    sim.add_liquid_source(Pos::new(5, 30, 8), true);
    sim.run_ticks(3000);
    assert!(depth(&sim, 15, 30, 2) >= 6, "magma flows along the tunnel");
    assert!(sim.liquid_at(Pos::new(15, 30, 2)).1, "it is magma");
    assert_eq!(depth(&sim, 15, 30, 3), 0, "magma has no pressure: the far leg stays empty");
    assert_eq!(sim.fluid_report().pressure_moves, 0);
}

#[test]
fn magma_meets_water_makes_obsidian() {
    let mut w = world();
    for x in 5..=12 {
        carve(&mut w, Pos::new(x, 40, 2), "StoneFloor1");
    }
    let mut sim = Sim::new(w, 3);
    sim.set_liquid(Pos::new(5, 40, 2), 7, false);
    sim.set_liquid(Pos::new(12, 40, 2), 7, true);
    sim.run_ticks(300);
    let obsidian: Vec<i16> = (6..=11)
        .filter(|&x| {
            let p = Pos::new(x, 40, 2);
            let t = sim.world.tile(p).unwrap();
            sim.world.shape(p) == Shape::Wall && sim.world.material(t.mat).map(|m| m.token.as_str()) == Some("INORGANIC:OBSIDIAN")
        })
        .collect();
    assert!(!obsidian.is_empty(), "an obsidian wall forms where they meet");
    let r = sim.fluid_report();
    assert!(r.obsidian >= 1);
    assert!(r.water_volume + r.magma_volume < 14, "steam: some liquid is gone ({r:?})");
    assert!(!sim.is_walkable(Pos::new(obsidian[0], 40, 2)), "obsidian is a wall for walkers");
}

#[test]
fn shallow_water_evaporates_outside_and_leaves_mud() {
    let mut sim = Sim::new(world(), 4);
    let out = Pos::new(10, 40, 13);
    let hall = Pos::new(19, 21, 9);
    assert!(sim.world.tile(out).unwrap().flags & tile_flags::OUTSIDE != 0);
    sim.set_liquid(out, 1, false);
    sim.set_liquid(hall, 1, false);
    sim.run_ticks(12_000);
    assert_eq!(sim.liquid_at(out).0, 0, "1/7 water outside evaporates within days");
    assert!(sim.has_mud(out), "it leaves mud on the floor");
    assert_eq!(sim.liquid_at(hall).0, 1, "underground 1/7 water stays");
    assert!(sim.fluid_report().evaporated >= 1);
}

#[test]
fn aquifer_leaks_into_a_dug_tunnel() {
    let mut w = world();
    let (cx, cy, hz) = (24i16, 24i16, 9i16);
    // The synthetic fort designates (cx+7..cx+12, cy) for mining. Make that band an aquifer.
    for x in cx + 6..=cx + 13 {
        for y in cy - 1..=cy + 1 {
            if x == cx + 6 && y == cy {
                continue;
            }
            w.tile_mut(Pos::new(x, y, hz)).unwrap().flags |= tile_flags::AQUIFER;
        }
    }
    let mut sim = Sim::new(w, 5);
    sim.set_invasions(false);
    let mut dug = false;
    for _ in 0..40 {
        sim.run_ticks(200);
        if sim.world.shape(Pos::new(cx + 7, cy, hz)) != Shape::Wall {
            dug = true;
            break;
        }
    }
    assert!(dug, "the miners dig into the aquifer");
    let t = sim.world.tile(Pos::new(cx + 7, cy, hz)).unwrap();
    assert_eq!(t.flags & tile_flags::AQUIFER, 0, "a dug aquifer tile is no aquifer any more");
    sim.run_ticks(3000);
    let r = sim.fluid_report();
    assert!(r.aquifer_added > 0, "water seeps in: {r:?}");
    assert!(r.water_volume > 0);
    assert!(r.leaky_aquifer_tiles > 0);
    // DF: water appears in the dug tile next to the aquifer (it may have spread on).
    let wet = (cx..=cx + 12).any(|x| sim.liquid_at(Pos::new(x, cy, hz)).0 > 0);
    assert!(wet, "the tunnel gets wet");
}

/// Sealed 1x1 cells at z4 with a dwarf in each.
fn cells_world() -> (World, [i32; 3]) {
    let mut w = world();
    let cells = [Pos::new(36, 40, 4), Pos::new(40, 40, 4), Pos::new(44, 40, 4)];
    for c in cells {
        carve(&mut w, c, "StoneFloor1");
    }
    let mut ids = [0; 3];
    for k in 0..3 {
        w.units[k].pos = cells[k];
        ids[k] = w.units[k].id;
    }
    w.units[1].skills.push(Skill { id: "SWIMMING".into(), rating: 2, xp: 0 });
    (w, ids)
}

#[test]
fn dwarves_drown_in_deep_water_unless_they_swim() {
    let (w, ids) = cells_world();
    let mut sim = Sim::new(w, 6);
    sim.set_invasions(false);
    sim.run_ticks(2);
    sim.set_liquid(Pos::new(36, 40, 4), 7, false); // non-swimmer, fully under water
    sim.set_liquid(Pos::new(40, 40, 4), 5, false); // swimmer in 5/7
    sim.set_liquid(Pos::new(44, 40, 4), 5, false); // non-swimmer in 5/7 struggles
    sim.run_ticks(60);
    assert!(!sim.world.unit(ids[0]).unwrap().flags.dead, "holds its breath for a while");
    sim.run_ticks(500);
    let dead = |id: i32| sim.world.unit(id).unwrap().flags.dead;
    assert!(dead(ids[0]), "drowned in 7/7");
    assert!(!dead(ids[1]), "a swimmer keeps its head above 5/7 water");
    assert!(dead(ids[2]), "a non-swimmer drowns in 5/7 water, slower");
    let anns = sim.drain_announcements();
    assert!(anns.iter().any(|a| a.kind == AnnouncementKind::Death && a.text.contains("drowned")), "death is announced");
    assert_eq!(sim.fluid_report().drowned, 2);
}

#[test]
fn magma_burns_units_and_items() {
    let (mut w, ids) = cells_world();
    let wood = w.materials.iter().position(|m| m.is_wood).unwrap() as u16;
    let stone = w.materials.iter().position(|m| m.is_stone && !m.is_ore).unwrap() as u16;
    let mut log = w.items[0].clone();
    log.id = 9001;
    log.kind = "WOOD".into();
    log.mat = wood;
    log.pos = Pos::new(36, 40, 4);
    let mut rock = log.clone();
    rock.id = 9002;
    rock.kind = "BOULDER".into();
    rock.mat = stone;
    w.items.push(log);
    w.items.push(rock);
    let mut sim = Sim::new(w, 7);
    sim.set_invasions(false);
    sim.set_liquid(Pos::new(36, 40, 4), 2, true);
    sim.run_ticks(40);
    assert!(sim.world.unit(ids[0]).unwrap().flags.dead, "magma kills");
    let r = sim.fluid_report();
    assert_eq!(r.burned, 1);
    assert!(r.items_melted >= 1, "the log burns: {r:?}");
    sim.run_ticks(2); // compaction of destroyed items
    assert!(sim.world.items.iter().all(|i| i.id != 9001), "log gone");
    assert!(sim.world.items.iter().any(|i| i.id == 9002), "stone is magma-safe");
}

#[test]
fn closed_door_holds_water_and_opening_it_lets_it_through() {
    let mut w = world();
    for x in 5..=12 {
        carve(&mut w, Pos::new(x, 20, 3), "StoneFloor1");
    }
    w.buildings.push(Building {
        id: 7777,
        kind: "Door".into(),
        x1: 8,
        y1: 20,
        x2: 8,
        y2: 20,
        z: 3,
        center: Pos::new(8, 20, 3),
        built: true,
        stage: -1,
        owner: -1,
        mat: 0,
        ..Default::default()
    });
    let mut sim = Sim::new(w, 8);
    for x in 5..=7 {
        sim.set_liquid(Pos::new(x, 20, 3), 7, false);
    }
    sim.run_ticks(300);
    for x in 8..=12 {
        assert_eq!(depth(&sim, x, 20, 3), 0, "closed door holds the water");
    }
    sim.set_building_flag(7777, "OPEN", true);
    sim.run_ticks(600);
    let row: Vec<u8> = (5..=12).map(|x| depth(&sim, x, 20, 3)).collect();
    assert!(depth(&sim, 12, 20, 3) > 0, "open door lets the water through: {row:?}");
    let (lo, hi) = (row.iter().min().unwrap(), row.iter().max().unwrap());
    assert!(hi - lo <= 1, "the corridor levels out (no 1/7 staircase): {row:?}");
    assert_eq!(sim.fluid_report().flowing, 0, "and comes to rest");
    assert_eq!(sim.fluid_report().water_volume, 21);
}

#[test]
fn settled_lake_stays_settled() {
    // A sealed lake (7/7) whose surface has open air above.
    let mut w = world();
    for y in 30..=36 {
        for x in 30..=36 {
            carve(&mut w, Pos::new(x, y, 3), "StoneFloor1");
            carve(&mut w, Pos::new(x, y, 4), "OpenSpace");
            carve(&mut w, Pos::new(x, y, 5), "OpenSpace");
        }
    }
    for y in 30..=36 {
        for x in 30..=36 {
            for z in 3..=4 {
                let t = w.tile_mut(Pos::new(x, y, z)).unwrap();
                t.liquid = 7;
            }
        }
    }
    let mut sim = Sim::new(w, 9);
    let v0 = sim.fluid_report().water_volume;
    sim.run_ticks(5);
    let r = sim.fluid_report();
    assert_eq!(r.flowing, 0, "nothing flows in a settled lake");
    assert_eq!(r.water_volume, v0);
    sim.run_ticks(20_000);
    assert_eq!(sim.fluid_report().water_volume, v0, "no drift");
}

#[test]
fn flooding_is_deterministic_across_thread_counts() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let mut sim = Sim::new(world(), 11);
            sim.set_parallel_threshold(1);
            sim.set_invasions(false);
            sim.designate(Pos::new(8, 20, 9), Pos::new(16, 24, 9), DesignationKind::Mine);
            // Water pours down the stair shaft into the hall.
            sim.add_liquid_source(Pos::new(24, 23, 13), false);
            sim.run_ticks(6000);
            let r = sim.fluid_report();
            (sim.state_hash(), r.water_volume, r.liquid_tiles, sim.world.units.iter().filter(|u| u.flags.dead).count())
        })
    };
    let a = run(1);
    let b = run(4);
    assert!(a.1 > 100, "the hall floods: {a:?}");
    assert_eq!(a, b, "1 thread vs 4 threads");
}
