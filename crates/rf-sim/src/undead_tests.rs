//! [undead13] Tests of the undead / necromancy module on synthetic maps.

use super::*;
use crate::round13_api::WeatherKind;
use rf_core::World;

const SURFACE_Z: i16 = 13; // World::synthetic(48, 48, 20): surface at dz*2/3

fn world() -> World {
    let mut w = World::synthetic(48, 48, 20);
    for u in &mut w.units {
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
    }
    w
}

fn mk(w: World, seed: u64) -> Sim {
    let mut sim = Sim::new(w, seed);
    sim.set_invasions(false);
    sim.set_moods_enabled(false);
    sim
}

fn run(sim: &mut Sim, n: u64) {
    for _ in 0..n {
        sim.tick();
    }
}

fn victim(sim: &Sim) -> i32 {
    sim.world.units.iter().find(|u| u.flags.citizen && !u.flags.dead).map(|u| u.id).expect("citizen")
}

fn undead_units(sim: &Sim) -> Vec<i32> {
    sim.world.units.iter().filter(|u| u.undead && !u.flags.dead).map(|u| u.id).collect()
}

#[test]
fn inactive_by_default_and_hash_silent() {
    let mut sim = mk(world(), 7);
    assert!(!sim.reanimation_enabled());
    let mut n = 0;
    sim.undead_hash(&mut |_| n += 1);
    assert_eq!(n, 0, "idle module must mix nothing");
    let id = victim(&sim);
    assert!(sim.kill_unit(id, "test"));
    run(&mut sim, 3000);
    assert_eq!(sim.undead_report().reanimated, 0);
    assert!(undead_units(&sim).is_empty());
    let mut n = 0;
    sim.undead_hash(&mut |_| n += 1);
    assert_eq!(n, 0, "no reanimation: still silent");
}

#[test]
fn corpse_rises_in_reanimating_region() {
    let mut w = world();
    w.climate.reanimating = true;
    let mut sim = mk(w, 11);
    assert!(sim.reanimation_enabled());
    let id = victim(&sim);
    let race = sim.world.unit(id).unwrap().race.clone();
    assert!(sim.kill_unit(id, "test"));
    let mut t = 0;
    while sim.undead_report().reanimated == 0 && t < 20_000 {
        sim.tick();
        t += 1;
    }
    assert!(sim.undead_report().reanimated >= 1, "corpse never rose");
    let z = undead_units(&sim);
    assert!(!z.is_empty());
    let u = sim.world.unit(z[0]).unwrap();
    assert_eq!(u.race, race);
    assert!(sim.unit_curse(z[0]).unwrap().undead);
    // Hostile and counted as a threat.
    assert!(sim.threats().iter().any(|t| t.kind == ThreatKind::UndeadHorde && t.units.contains(&z[0])));
}

#[test]
fn toggle_overrides_climate() {
    let mut w = world();
    w.climate.reanimating = true;
    let mut sim = mk(w, 3);
    sim.set_reanimation_enabled(false);
    assert!(!sim.reanimation_enabled());
    let id = victim(&sim);
    sim.kill_unit(id, "test");
    run(&mut sim, 4000);
    assert_eq!(sim.undead_report().reanimated, 0);
}

#[test]
fn contained_corpse_stays_down() {
    let mut sim = mk(world(), 5);
    sim.set_reanimation_enabled(true);
    let id = victim(&sim);
    sim.kill_unit(id, "test");
    let i = sim.world.items.iter().position(|it| it.kind == "CORPSE").expect("corpse");
    assert!(sim.undead_can_rise(i));
    sim.world.items[i].flags.in_building = true; // in a coffin
    assert!(!sim.undead_can_rise(i));
}

#[test]
fn undead_combat_traits() {
    let mut sim = mk(world(), 9);
    let p = sim.world.units[0].pos;
    let s = sim.undead_spawn("DWARF", "zombie".into(), p, 1, 0).expect("spawned");
    assert!(sim.undead_nostun(s) && sim.undead_bloodless(s));
    // Brain and heart wounds do not stop the undead; losing the head does.
    assert_eq!(sim.undead_blow(s, Some("brain")), None);
    assert_eq!(sim.undead_blow(s, Some("heart")), None);
    assert_eq!(sim.undead_blow(s, Some("head")), Some("destroyed"));
    // A living dwarf is neither.
    let d = sim.unit_slot[&victim(&sim)];
    assert!(!sim.undead_nostun(d) && !sim.undead_bloodless(d));
    // Destroying an undead is counted.
    let zid = sim.ai[s].id;
    sim.kill_unit(zid, "test");
    assert_eq!(sim.undead_report().destroyed, 1);
}

#[test]
fn necromancer_siege() {
    let mut sim = mk(world(), 13);
    sim.set_invasions(true);
    let n = sim.spawn_necromancer_siege(6);
    assert!(n > 0, "siege spawned");
    assert_eq!(sim.undead_report().necro_sieges, 1);
    let necro = sim.undead.necros[0];
    let c = sim.unit_curse(necro).unwrap();
    assert!(c.necromancer && !c.undead);
    assert!(undead_units(&sim).len() >= 5);
    let th = sim.threats();
    let t = th.iter().find(|t| t.kind == ThreatKind::NecromancerSiege).expect("threat");
    assert!(t.units.contains(&necro));
    run(&mut sim, 500);
    let mut a = 0u64;
    sim.undead_hash(&mut |v| a = a.wrapping_mul(31).wrapping_add(v));
    assert_ne!(a, 0);
}

#[test]
fn natural_necro_sieges_need_tower_or_toggle() {
    let mut sim = mk(world(), 17);
    assert!(!sim.necro_sieges_possible());
    sim.set_necromancer_sieges(true);
    assert!(sim.necro_sieges_possible());
    let mut w = world();
    w.entities.push(rf_core::EntityInfo { id: 900, name: "the tower of Necromancers".into(), race: "HUMAN".into(), kind: "SiteGovernment".into(), ..Default::default() });
    let sim = mk(w, 17);
    assert!(sim.necro_sieges_possible());
}

#[test]
fn husk_cloud_turns_units_outside() {
    let mut w = world();
    let out = w.units[0].id;
    w.units[0].pos = Pos::new(10, 10, SURFACE_Z);
    let inside = w.units[1].id;
    let mut sim = mk(w, 21);
    let ew = EvilWeather { kind: WeatherKind::EvilCloud, name: "a cloud of husk dust".into(), effect: "HUSK".into(), syndrome: String::new(), center: None, radius: 0 };
    let mut t = 0;
    while sim.undead_report().husks == 0 && t < 50 {
        sim.undead_evil_exposure(&ew, t);
        t += 1;
    }
    let c = sim.unit_curse(out).unwrap();
    assert!(c.husk, "unit outside became a husk");
    assert!(!sim.unit_curse(inside).unwrap().husk, "unit indoors untouched");
    let s = sim.unit_slot[&out];
    assert!(sim.undead_nostun(s) && sim.undead_bloodless(s));
    assert!(!sim.world.unit(out).unwrap().flags.citizen);
    // Thralls too; husks are not hit twice.
    let ew2 = EvilWeather { effect: "THRALL".into(), ..ew };
    sim.undead_evil_exposure(&ew2, 99);
    assert_eq!(sim.undead_report().husks, 1);
}

#[test]
fn deterministic_rise() {
    let go = |seed| {
        let mut w = world();
        w.climate.reanimating = true;
        let mut sim = mk(w, seed);
        let id = victim(&sim);
        sim.kill_unit(id, "test");
        run(&mut sim, 3000);
        (sim.undead_report().reanimated, sim.state_hash())
    };
    assert_eq!(go(4), go(4));
}
