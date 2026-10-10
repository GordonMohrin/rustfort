//! [saves11] Exact save/load: run N ticks, save (`Sim::world_for_save` through MessagePack with
//! named fields, as rf-app's save format 2), load (`Sim::new`), run M ticks == run N+M
//! directly (full state hash). The long runs on the real fort are `examples/saveload.rs`.
//!
//!   cargo test -p rf-sim --release --test saves11 -- --nocapture

use rf_core::World;
use rf_sim::{gen, Sim};

/// What a save file holds, decoded again (rf-app: `encode_world` / `decode_world`).
fn through_file(w: &World) -> World {
    let raw = rmp_serde::to_vec_named(w).expect("encode");
    let mut w: World = rmp_serde::from_slice(&raw).expect("decode");
    w.tiletypes.reindex();
    w
}

/// Run `n`, save + load, run `m` on both; the hashes after the load and at the end.
fn continue_after_load(world: World, seed: u64, n: u32, m: u32) -> (Sim, Sim) {
    let mut a = Sim::new(world, seed);
    a.run_ticks(n);
    let mut b = Sim::new(through_file(&a.world_for_save()), seed);
    assert_eq!(b.restore_mode(), "exact");
    assert_eq!(a.state_hash(), b.state_hash(), "right after the load");
    a.run_ticks(m);
    b.run_ticks(m);
    (a, b)
}

#[test]
fn reload_continues_bit_identically_on_synthetic_worlds() {
    for (seed, n, m) in [(1u64, 100u32, 100u32), (2, 3_000, 3_000), (3, 7_000, 1)] {
        let (a, b) = continue_after_load(World::synthetic(48, 48, 20), seed, n, m);
        assert_eq!(a.state_hash(), b.state_hash(), "synthetic seed {seed} N {n} M {m}");
        assert_eq!(a.state_hash_parts(), b.state_hash_parts());
    }
    let (a, b) = continue_after_load(gen::bench_world(48, 48, 24, 40, 4), 4, 4_000, 4_000);
    assert_eq!(a.state_hash(), b.state_hash(), "bench world");
}

#[test]
fn several_reloads_in_a_row_stay_on_track() {
    let world = gen::bench_world(40, 40, 20, 30, 7);
    let mut a = Sim::new(world.clone(), 7);
    let mut b = Sim::new(world, 7);
    for _ in 0..4 {
        a.run_ticks(1_500);
        b.run_ticks(1_500);
        b = Sim::new(through_file(&b.world_for_save()), 7);
        assert_eq!(a.state_hash(), b.state_hash());
    }
    a.run_ticks(2_000);
    b.run_ticks(2_000);
    assert_eq!(a.state_hash(), b.state_hash());
}

/// A snapshot that does not fit (edited world, foreign layout) is not used: the sim falls back
/// to the round-10 module state and still runs.
#[test]
fn unfitting_snapshots_fall_back_to_module_state() {
    let mut a = Sim::new(World::synthetic(40, 40, 16), 5);
    a.run_ticks(1_000);
    // World changed after the snapshot was taken (one item less).
    let mut w = through_file(&a.world_for_save());
    w.items.pop();
    let mut b = Sim::new(w, 5);
    assert!(b.restore_mode().starts_with("close"), "{}", b.restore_mode());
    assert_eq!(b.ticks(), a.ticks());
    b.run_ticks(500);
    // Snapshot with a field this build does not know (a newer Rustfort's state): append a key
    // to the top-level map (fixmap header 0x83, 3 keys -> 0x84).
    let mut w = a.world_for_save();
    assert_eq!(w.sim_exact.first(), Some(&0x83));
    w.sim_exact[0] = 0x84;
    w.sim_exact.extend_from_slice(&[0xad]);
    w.sim_exact.extend_from_slice(b"unknown_state");
    w.sim_exact.extend_from_slice(&[0x91, 0x07]);
    let mut c = Sim::new(through_file(&w), 5);
    assert!(c.restore_mode().starts_with("close"), "{}", c.restore_mode());
    c.run_ticks(500);
    // Garbage snapshot.
    let mut w = a.world_for_save();
    w.sim_exact.truncate(w.sim_exact.len() / 2);
    let mut d = Sim::new(w, 5);
    assert!(d.restore_mode().starts_with("close"));
    d.run_ticks(500);
    // A bare world (import, old save) starts fresh.
    let mut w = a.world_for_save();
    w.sim_state.clear();
    w.sim_exact.clear();
    assert_eq!(Sim::new(w, 5).restore_mode(), "");
}

/// The real fort, short: N 100 / M 100 and N 2000 / M 1000 (long runs: `examples/saveload`).
#[test]
fn reload_continues_bit_identically_on_the_real_fort() {
    let Some(dir) = rf_import::find_export(None) else {
        println!("no export found (set RUSTFORT_EXPORT): skipped");
        return;
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    for (n, m) in [(100u32, 100u32), (2_000, 1_000)] {
        let (a, b) = continue_after_load(world.clone(), 1, n, m);
        assert_eq!(a.state_hash(), b.state_hash(), "real fort N {n} M {m}");
    }
}
