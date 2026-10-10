//! Round 7: civilian alert keeps burrow work going; siege size by wealth.
//!
//! `cargo test -p rf-sim --release --test r7 -- --nocapture`

use rf_core::{Pos, Shape, World};
use rf_sim::Sim;

const DZ: u16 = 20;

fn shape(sim: &Sim, p: Pos) -> Shape {
    let i = sim.world.idx(p).unwrap();
    sim.world.tiletypes.get(sim.world.tiles[i].tt).shape
}

fn alert_run(seed: u64) -> (Sim, i16) {
    let w = World::synthetic(48, 48, DZ);
    let hz = (DZ as i16 * 2 / 3) - 4;
    let mut sim = Sim::new(w, seed);
    sim.set_invasions(false);
    // The hall (x 18..30, y 20..28) plus the first two designated wall tiles east of it
    // (x 31, 32); the designations at x 33..35 lie outside the burrow.
    let b = sim.create_burrow("Hall", Pos::new(18, 20, hz), Pos::new(32, 28, hz));
    sim.set_civilian_alert(Some(b));
    sim.run_ticks(8000);
    (sim, hz)
}

#[test]
fn civilian_alert_works_inside_the_burrow_only() {
    let (sim, hz) = alert_run(3);
    let inside_dug = [31i16, 32].iter().filter(|&&x| shape(&sim, Pos::new(x, 24, hz)) != Shape::Wall).count();
    let outside_dug = [33i16, 34, 35].iter().filter(|&&x| shape(&sim, Pos::new(x, 24, hz)) != Shape::Wall).count();
    println!("civilian alert: {inside_dug}/2 designations inside the burrow dug, {outside_dug}/3 outside");
    assert_eq!(inside_dug, 2, "burrow designations are worked under the alert");
    assert_eq!(outside_dug, 0, "designations outside the burrow wait");
    // Everybody stayed in the burrow (or on its stair shaft tile).
    let out: Vec<Pos> = sim
        .world
        .units
        .iter()
        .filter(|u| u.race == "DWARF" && !u.flags.dead)
        .map(|u| u.pos)
        .filter(|p| !(p.z == hz && (18..=32).contains(&p.x) && (20..=28).contains(&p.y)))
        .collect();
    println!("dwarves outside the burrow: {out:?}");
    assert!(out.len() <= 1, "civilians stay inside the burrow");
}

#[test]
fn civilian_alert_is_deterministic() {
    let (a, _) = alert_run(9);
    let (b, _) = alert_run(9);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn lifting_the_alert_resumes_outside_work() {
    let (mut sim, hz) = alert_run(4);
    sim.set_civilian_alert(None);
    sim.run_ticks(12000);
    let dug = [33i16, 34, 35].iter().filter(|&&x| shape(&sim, Pos::new(x, 24, hz)) != Shape::Wall).count();
    println!("after the alert: {dug}/3 outside designations dug");
    assert!(dug >= 1);
}

#[test]
fn siege_size_follows_wealth_tiers() {
    // The synthetic fort has 7 dwarves: below every siege tier.
    let sim = Sim::new(World::synthetic(48, 48, DZ), 1);
    assert_eq!(sim.siege_size(), None);
    assert_eq!(rf_sim::invasion::siege_force(89, 150_000, 0), Some((10, 0)));
    assert_eq!(rf_sim::invasion::siege_force(115, 150_000, 20_000), Some((40, 5)));
}
