use rf_core::{Pos, Shape, TICKS_PER_MONTH, World};
use rf_sim::{gen, AnnouncementKind, DesignationKind, Sim};

const DX: u16 = 48;
const DY: u16 = 48;
const DZ: u16 = 20;

fn hall() -> (i16, i16, i16) {
    let surface_z = (DZ as i16 * 2 / 3).max(2);
    (DX as i16 / 2, DY as i16 / 2, (surface_z - 4).max(0))
}

fn synth() -> Sim {
    Sim::new(World::synthetic(DX, DY, DZ), 7)
}

fn count_kind(sim: &Sim, kind: &str) -> i32 {
    sim.world.items.iter().filter(|i| i.kind == kind).map(|i| i.stack).sum()
}

fn busy_sim(seed: u64) -> Sim {
    let (cx, cy, hz) = hall();
    let mut sim = Sim::new(World::synthetic(DX, DY, DZ), seed);
    sim.designate(Pos::new(cx - 16, cy - 4, hz), Pos::new(cx - 7, cy - 1, hz), DesignationKind::Mine);
    sim.create_stockpile(Pos::new(cx + 1, cy + 1, hz), Pos::new(cx + 6, cy + 4, hz), &["stone", "wood"]);
    sim.designate(Pos::new(0, 0, hz + 5), Pos::new(DX as i16 - 1, DY as i16 - 1, hz + 5), DesignationKind::ChopTrees);
    sim
}

#[test]
fn deterministic_same_seed_and_thread_counts() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let mut sim = busy_sim(11);
            sim.run_ticks(20_000);
            sim.state_hash()
        })
    };
    let a = run(1);
    let b = run(4);
    let c = run(4);
    assert_eq!(a, b, "1 thread vs 4 threads");
    assert_eq!(b, c, "same seed twice");
    let mut other = busy_sim(12);
    other.run_ticks(20_000);
    assert_ne!(a, other.state_hash(), "different seed should diverge");
}

#[test]
fn deterministic_bench_world_with_parallel_paths() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let mut sim = Sim::new(gen::bench_world(64, 64, 30, 60, 3), 3);
            sim.run_ticks(5_000);
            (sim.state_hash(), sim.stats().paths, sim.stats().jobs_done)
        })
    };
    let a = run(1);
    let b = run(6);
    assert!(a.1 > 100, "expected many path searches, got {}", a.1);
    assert!(a.2 > 10, "expected jobs to be done, got {}", a.2);
    assert_eq!(a, b);
}

#[test]
fn dwarves_survive_two_months() {
    let mut sim = synth();
    let drinks0 = count_kind(&sim, "DRINK");
    let food0 = count_kind(&sim, "PLANT");
    sim.run_ticks(2 * TICKS_PER_MONTH as u32);
    let deaths: Vec<_> = sim.drain_announcements().into_iter().filter(|a| a.kind == AnnouncementKind::Death).collect();
    assert!(deaths.is_empty(), "deaths: {:?}", deaths);
    assert!(sim.world.citizens().count() == 7);
    let drinks1 = count_kind(&sim, "DRINK");
    let food1 = count_kind(&sim, "PLANT");
    assert!(drinks1 < drinks0 - 7, "dwarves drank: {drinks0} -> {drinks1}");
    assert!(food1 < food0, "dwarves ate: {food0} -> {food1}");
    for u in sim.world.citizens() {
        assert!(u.thirst_timer < rf_sim::needs::DEHYDRATED, "{} thirst {}", u.name, u.thirst_timer);
        assert!(u.hunger_timer < rf_sim::needs::STARVING, "{} hunger {}", u.name, u.hunger_timer);
    }
}

#[test]
fn dwarves_die_of_thirst_without_drinks() {
    let mut w = World::synthetic(DX, DY, DZ);
    w.items.retain(|i| i.kind != "DRINK");
    let mut sim = Sim::new(w, 1);
    sim.run_ticks(80_000);
    let deaths = sim.drain_announcements().into_iter().filter(|a| a.kind == AnnouncementKind::Death).count();
    assert_eq!(deaths, 7, "all dwarves die of dehydration by tick 80k");
    assert!(sim.world.units.iter().filter(|u| u.flags.dead).count() == 7);
    assert!(sim.world.items.iter().any(|i| i.kind == "CORPSE"));
}

#[test]
fn designated_walls_get_mined() {
    let (cx, cy, hz) = hall();
    let mut sim = synth();
    let targets: Vec<Pos> = (cx + 7..cx + 12).map(|x| Pos::new(x, cy, hz)).collect();
    for &p in &targets {
        assert_eq!(sim.world.tile(p).unwrap().desig, 1, "synthetic world designates {p:?}");
    }
    sim.run_ticks(4_000);
    for &p in &targets {
        assert_eq!(sim.world.shape(p), Shape::Floor, "{p:?} mined");
        assert_eq!(sim.world.tile(p).unwrap().desig, 0);
    }
    assert!(sim.reachable(sim.world.units[0].pos, targets[4]));
}

#[test]
fn mined_stone_becomes_floor_and_boulders() {
    let (cx, cy, hz) = hall();
    let mut sim = synth();
    let (a, b) = (Pos::new(cx - 16, cy - 4, hz), Pos::new(cx - 7, cy - 1, hz));
    sim.designate(a, b, DesignationKind::Mine);
    assert!(sim.stats().open_jobs >= 45);
    let boulders0 = count_kind(&sim, "BOULDER");
    sim.run_ticks(30_000);
    for y in a.y..=b.y {
        for x in a.x..=b.x {
            let p = Pos::new(x, y, hz);
            assert_eq!(sim.world.shape(p), Shape::Floor, "{p:?}");
            let mat = sim.world.tt(p).material;
            assert!(matches!(mat, rf_core::TtMaterial::Stone | rf_core::TtMaterial::Mineral), "{p:?} keeps its material class: {mat:?}");
        }
    }
    let boulders = count_kind(&sim, "BOULDER") - boulders0;
    // 40 tiles, 25 % (stone) / 33 % (ore): expect about 10.
    assert!((3..=25).contains(&boulders), "boulders: {boulders}");
    for i in sim.world.items.iter().filter(|i| i.kind == "BOULDER") {
        assert!(sim.world.shape(i.pos).has_floor(), "boulder on a floor");
    }
}

#[test]
fn trees_get_chopped() {
    let (_, _, hz) = hall();
    let tree_z = hz + 5;
    let mut sim = synth();
    let trees: Vec<Pos> = sim.world.plants.iter().filter(|p| p.tree).map(|p| p.pos).collect();
    assert_eq!(trees.len(), 4);
    let logs0 = count_kind(&sim, "WOOD");
    sim.designate(Pos::new(0, 0, tree_z), Pos::new(DX as i16 - 1, DY as i16 - 1, tree_z), DesignationKind::ChopTrees);
    assert_eq!(sim.stats().open_jobs, 5 + 4);
    sim.run_ticks(30_000);
    for &p in &trees {
        assert_ne!(sim.world.tt(p).material, rf_core::TtMaterial::Tree, "{p:?} felled");
    }
    assert!(sim.world.plants.iter().all(|p| !p.tree));
    assert_eq!(count_kind(&sim, "WOOD"), logs0 + 4);
}

#[test]
fn items_get_hauled_to_stockpile() {
    let (cx, cy, hz) = hall();
    let mut sim = synth();
    let (a, b) = (Pos::new(cx + 1, cy + 2, hz), Pos::new(cx + 4, cy + 3, hz));
    sim.create_stockpile(a, b, &["wood"]);
    let inside = |p: Pos| p.z == hz && p.x >= a.x && p.x <= b.x && p.y >= a.y && p.y <= b.y;
    assert!(sim.world.items.iter().filter(|i| i.kind == "WOOD").all(|i| !inside(i.pos)));
    sim.run_ticks(3_000);
    let logs: Vec<Pos> = sim.world.items.iter().filter(|i| i.kind == "WOOD").map(|i| i.pos).collect();
    assert_eq!(logs.len(), 4);
    for p in &logs {
        assert!(inside(*p), "log at {p:?} not in the stockpile");
    }
    let mut uniq = logs.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), 4, "one item per stockpile tile");
}

#[test]
fn channel_makes_ramp_below_and_stays_connected() {
    let (cx, cy, hz) = hall();
    let mut sim = synth();
    // Dig a room one level down first? No: channel a hall floor tile; the wall below becomes a ramp.
    let p = Pos::new(cx - 5, cy + 3, hz);
    sim.designate(p, p, DesignationKind::Channel);
    assert_eq!(sim.stats().open_jobs, 6);
    sim.run_ticks(3_000);
    assert_eq!(sim.world.shape(p), Shape::RampTop);
    let below = p.offset(0, 0, -1);
    assert_eq!(sim.world.shape(below), Shape::Ramp);
    assert!(sim.is_walkable(below));
    assert!(sim.reachable(sim.world.units[0].pos, below), "ramp connects to the wall tops next to it");
    let path = sim.find_path(sim.world.units[0].pos, below).expect("path down the ramp");
    assert_eq!(*path.last().unwrap(), below);
}

#[test]
fn stairs_dug_downwards_connect_levels() {
    let (cx, cy, hz) = hall();
    let mut sim = synth();
    let top = Pos::new(cx + 3, cy - 3, hz);
    let bottom = top.offset(0, 0, -3);
    sim.designate(top, bottom, DesignationKind::UpDownStair);
    sim.run_ticks(8_000);
    for z in bottom.z..=top.z {
        assert_eq!(sim.world.shape(Pos::new(top.x, top.y, z)), Shape::StairUpdown, "z {z}");
    }
    assert!(sim.reachable(sim.world.units[0].pos, bottom));
}

#[test]
fn unit_status_reports_activities() {
    let mut sim = busy_sim(5);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..300 {
        sim.run_ticks(50);
        for u in &sim.world.units {
            seen.insert(sim.unit_status(u.id).activity);
        }
    }
    for want in ["No Job", "Dig", "Drink", "Store Item in Stockpile"] {
        assert!(seen.contains(want), "missing {want} in {seen:?}");
    }
}
