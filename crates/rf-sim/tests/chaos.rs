//! [chaos10] Chaos/fuzz tests: random but valid player actions with invariant checks and
//! save/load round trips (short versions of `examples/chaos.rs`), plus one regression test
//! per crash or corruption the chaos runs found.
//!
//!   cargo test -p rf-sim --release --test chaos -- --nocapture
//!
//! The real-fort smoke test runs only when an export is found (`RUSTFORT_EXPORT`).

#[path = "../examples/chaos_support/mod.rs"]
mod chaos_support;

use chaos_support::{save_load, violations, Chaos};
use rf_core::{tile_flags, Building, Item, ItemFlags, MaterialInfo, PlantRaw, Pos, World};
use rf_sim::{gen, Sim, SquadOrder};

fn chaos_run(world: World, seed: u64, steps: u64, step: u32, save_every: u64) -> (Sim, chaos_support::Outcome) {
    let mut c = Chaos::new(seed);
    let sim = c.run(Sim::new(world, seed), steps, step, save_every);
    (sim, c.out)
}

#[test]
fn chaos_on_synthetic_worlds_keeps_invariants() {
    for seed in 1..=4u64 {
        let (_, out) = chaos_run(World::synthetic(48, 48, 20), seed, 120, 400, 40);
        assert!(out.violations.is_empty(), "seed {seed}: {:?}", out.violations);
        assert!(out.actions > 50, "seed {seed}: only {} actions", out.actions);
    }
    for seed in 1..=2u64 {
        let (_, out) = chaos_run(gen::bench_world(48, 48, 24, 40, seed), seed, 80, 500, 30);
        assert!(out.violations.is_empty(), "bench seed {seed}: {:?}", out.violations);
    }
}

#[test]
fn chaos_is_deterministic_with_save_load() {
    let run = || {
        let (sim, out) = chaos_run(gen::bench_world(40, 40, 20, 30, 5), 5, 60, 500, 25);
        (sim.state_hash(), out.actions, sim.world.items.len(), sim.world.buildings.len())
    };
    assert_eq!(run(), run());
}

#[test]
fn chaos_on_the_real_fort_smoke() {
    let Some(dir) = rf_import::find_export(None) else {
        println!("no export found (set RUSTFORT_EXPORT): skipped");
        return;
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    for seed in [1u64, 2] {
        let (sim, out) = chaos_run(world.clone(), seed, 40, 1200, 20);
        println!("real fort seed {seed}: {} actions, {} saves, citizens {} -> {}", out.actions, out.saves, out.citizens_start, sim.world.citizens().count());
        assert!(out.violations.is_empty(), "seed {seed}: {:?}", out.violations);
    }
}

// ------------------------------------------------------------------ regressions

/// Round 10 chaos, real fort seed 4: a squad stationed on a wall tile re-ran a failing A*
/// over the whole fort every 60 ticks per soldier (14 M nodes per 1 200 ticks; the game
/// dropped from 50k to 500 ticks/s). Soldiers now go to the nearest standable tile.
#[test]
fn station_on_a_wall_goes_next_to_it_without_failing_searches() {
    let (dx, dy, dz) = (48u16, 48u16, 20u16);
    let surface_z = (dz as i16 * 2 / 3).max(2);
    let (cx, cy, hz) = (dx as i16 / 2, dy as i16 / 2, (surface_z - 4).max(0));
    let mut sim = Sim::new(World::synthetic(dx, dy, dz), 4);
    sim.set_invasions(false);
    // A natural wall tile one step outside the hall.
    let wall = (1..cx)
        .map(|k| Pos::new(cx - k, cy + 1, hz))
        .find(|&p| !sim.is_walkable(p) && sim.is_walkable(Pos::new(p.x + 1, p.y, p.z)))
        .expect("a wall next to the hall");
    let q = sim.create_squad("Wall guards");
    let cits: Vec<i32> = sim.world.citizens().map(|u| u.id).take(3).collect();
    for (k, &u) in cits.iter().enumerate() {
        sim.assign_squad_member(q, k, u);
    }
    sim.order_squad(q, SquadOrder::Station(wall));
    let before = sim.stats().path_failures;
    sim.run_ticks(3_000);
    let fails = sim.stats().path_failures - before;
    let near = cits.iter().filter(|&&u| sim.world.unit(u).map(|x| x.pos.dist(wall) <= 6).unwrap_or(false)).count();
    println!("station on wall {wall:?}: {fails} path failures, {near}/{} soldiers within 6 tiles", cits.len());
    assert!(fails < 20, "{fails} failing searches for a station on a wall");
    assert!(near >= 2, "soldiers went to the station ({near} near)");
}

/// Round 10 chaos, real fort seeds 204 and 108 (war and peace): the game died with "thread
/// has overflowed its stack". Under a civilian alert, `construct_try` recursed once per
/// planned construction without material inside the burrow; with a big planned area that is
/// thousands of frames. It is a loop now. The sim runs on a thread with a small stack here,
/// so the old recursion overflows with 1 600 jobs.
#[test]
fn many_constructions_without_material_under_an_alert_do_not_overflow_the_stack() {
    let h = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let (dx, dy, dz) = (64u16, 64u16, 20u16);
            let surface_z = (dz as i16 * 2 / 3).max(2);
            let (cx, cy, hz) = (dx as i16 / 2, dy as i16 / 2, (surface_z - 4).max(0));
            let mut w = World::synthetic(dx, dy, dz);
            w.items.clear(); // no material anywhere
            let floor = w.tiletypes.by_name("StoneFloor1").unwrap();
            for y in cy - 20..=cy + 20 {
                for x in cx - 20..=cx + 20 {
                    if (x, y) != (cx, cy) {
                        w.tile_mut(Pos::new(x, y, hz)).unwrap().tt = floor;
                    }
                }
            }
            let mut sim = Sim::new(w, 8);
            sim.set_invasions(false);
            let n = sim.build_construction(rf_sim::ConstructionKind::Wall, Pos::new(cx - 20, cy - 20, hz), Pos::new(cx + 20, cy + 20, hz));
            let b = sim.create_burrow("Hall", Pos::new(cx - 20, cy - 20, hz), Pos::new(cx + 20, cy + 20, hz));
            sim.set_civilian_alert(Some(b));
            for u in sim.world.units.iter().map(|u| u.id).collect::<Vec<_>>() {
                sim.set_labor(u, "BUILD_CONSTRUCTION", true);
            }
            sim.run_ticks(3_000);
            n
        })
        .unwrap();
    let n = h.join().expect("the sim thread overflowed or panicked");
    println!("{n} planned walls without material under a civilian alert: no overflow");
    assert!(n > 1_000, "only {n} constructions planned");
}

/// Round 10 chaos, real fort, every seed: seeds planted in DF's farm plots are turned into
/// crops (the seed item is destroyed), but the plot kept the dead item ids in
/// `Building::items` forever, and saves carried them. Compaction now drops them.
#[test]
fn consumed_items_leave_no_ids_in_buildings_or_inventories() {
    let surface_z = (20i16 * 2 / 3).max(2);
    let (cx, cy, hz) = (24i16, 24i16, (surface_z - 4).max(0));
    let mut w = World::synthetic(48, 48, 20);
    let seed_mat = w.materials.len() as u16;
    w.materials.push(MaterialInfo { token: "PLANT:PLUMP_HELMET:SEED".into(), name: "plump helmet spawn".into(), ..Default::default() });
    w.plant_raws.push(PlantRaw {
        token: "PLUMP_HELMET".into(),
        name: "plump helmet".into(),
        growdur: 30_000,
        spring: true,
        summer: true,
        autumn: true,
        winter: true,
        underground: true,
        has_seed: true,
        mat_seed: seed_mat,
        ..Default::default()
    });
    let soil = w.tiletypes.by_name("SoilFloor1").unwrap();
    for y in cy + 1..=cy + 2 {
        for x in cx - 4..=cx - 3 {
            let t = w.tile_mut(Pos::new(x, y, hz)).unwrap();
            t.tt = soil;
            t.flags |= tile_flags::SUBTERRANEAN;
        }
    }
    let planted: Vec<i32> = (0..4).map(|k| 7000 + k).collect();
    for (k, &id) in planted.iter().enumerate() {
        w.items.push(Item {
            id,
            kind: "SEEDS".into(),
            mat: seed_mat,
            pos: Pos::new(cx - 4 + (k % 2) as i16, cy + 1 + (k / 2) as i16, hz),
            stack: 1,
            container: -1,
            holder: -1,
            building: 600,
            flags: ItemFlags { in_building: true, ..Default::default() },
            ..Default::default()
        });
    }
    w.buildings.push(Building {
        id: 600,
        kind: "FarmPlot".into(),
        x1: cx - 4,
        y1: cy + 1,
        x2: cx - 3,
        y2: cy + 2,
        z: hz,
        center: Pos::new(cx - 4, cy + 1, hz),
        built: true,
        owner: -1,
        crops: vec!["PLUMP_HELMET".into(); 4],
        items: planted.clone(),
        ..Default::default()
    });
    let mut sim = Sim::new(w, 3);
    sim.run_ticks(2_000);
    let v = violations(&sim);
    assert!(v.is_empty(), "{v:?}");
    // And the round trip through a save keeps it clean.
    let sim = save_load(sim);
    assert!(violations(&sim).is_empty());
}
