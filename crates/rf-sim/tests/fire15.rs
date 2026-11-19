//! [fire15] Integration tests: exact save/load in the middle of a fire, determinism, and a
//! grass fire on the real fort (skipped without `RUSTFORT_EXPORT`).
//!
//!   cargo test -p rf-sim --release --test fire15 -- --nocapture

use rf_core::{Pos, World};
use rf_sim::Sim;

fn through_file(w: &World) -> World {
    let raw = rmp_serde::to_vec_named(w).expect("encode");
    let mut w: World = rmp_serde::from_slice(&raw).expect("decode");
    w.tiletypes.reindex();
    w
}

/// Synthetic fort with temperature on, a grass fire on the surface and a log in magma.
fn burning(seed: u64) -> Sim {
    let mut w = World::synthetic(48, 48, 20);
    w.settings.temperature = true;
    let surf = 20 * 2 / 3;
    let mut sim = Sim::new(w, seed);
    assert!(sim.ignite_tile(Pos::new(10, 10, surf)), "surface grass burns");
    assert!(sim.ignite_tile(Pos::new(30, 30, surf)));
    sim
}

#[test]
fn fire_state_survives_an_exact_save() {
    let mut a = burning(3);
    a.run_ticks(400);
    let r = a.fire_report();
    assert!(r.burning_tiles > 0, "mid-fire: {r:?}");
    let mut b = Sim::new(through_file(&a.world_for_save()), 3);
    assert_eq!(b.restore_mode(), "exact");
    assert_eq!(a.state_hash(), b.state_hash(), "right after the load");
    assert_eq!(a.fire_report().burning_tiles, b.fire_report().burning_tiles);
    a.run_ticks(1_500);
    b.run_ticks(1_500);
    assert_eq!(a.state_hash(), b.state_hash(), "continues bit-identically");
    assert_eq!(a.fire_report().tiles_burnt, b.fire_report().tiles_burnt);
}

#[test]
fn fire_is_deterministic_across_thread_counts() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let mut sim = burning(11);
            sim.run_ticks(3_000);
            (sim.state_hash(), sim.fire_report().tiles_burnt)
        })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn grass_fire_on_the_real_fort() {
    let Some(dir) = std::env::var_os("RUSTFORT_EXPORT") else {
        eprintln!("skip: RUSTFORT_EXPORT not set");
        return;
    };
    let Ok((w, _)) = rf_import::load_export(std::path::Path::new(&dir)) else {
        eprintln!("skip: no export at {dir:?}");
        return;
    };
    let mut sim = Sim::new(w, 1);
    assert!(!sim.temperature_enabled(), "the reference fort has temperature off");
    sim.set_temperature_enabled(true);
    // First flammable outside grass tile, scanning from the map centre outward.
    let d = sim.world.dims;
    let mut start = None;
    'find: for r in 0..(d.x.max(d.y) as i16 / 2) {
        for z in (0..d.z as i16).rev() {
            for (x, y) in [(d.x as i16 / 2 + r, d.y as i16 / 2), (d.x as i16 / 2 - r, d.y as i16 / 2), (d.x as i16 / 2, d.y as i16 / 2 + r), (d.x as i16 / 2, d.y as i16 / 2 - r)] {
                let p = Pos::new(x, y, z);
                if sim.world.in_bounds(p) && sim.ignite_tile(p) {
                    start = Some(p);
                    break 'find;
                }
            }
        }
    }
    let Some(p) = start else {
        eprintln!("skip: no burnable grass found");
        return;
    };
    sim.run_ticks(5_000);
    let r = sim.fire_report();
    eprintln!("fire from {p:?}: {r:?}");
    assert!(r.tiles_ignited >= 1 && r.cells_max >= 1);
}
