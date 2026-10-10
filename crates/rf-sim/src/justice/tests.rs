//! [society2] Tests: crimes, investigation, conviction, jail, beatings, hammerstrikes.

use crate::locations::tests::{slot, world_with_zone};
use crate::Sim;
use rf_core::{Building, Item, ItemFlags, NoblePosition, Pos};

fn noble(token: &str, holder: i32) -> NoblePosition {
    NoblePosition { token: token.into(), name: token.to_lowercase(), holder, entity_id: 1, mandates: Vec::new() }
}

fn thought(sim: &Sim, id: i32, t: &str) -> bool {
    sim.world.unit(id).unwrap().emotions.iter().any(|e| e.thought == t)
}

#[test]
fn crimes_stay_unsolved_without_a_law_enforcer() {
    let mut sim = Sim::new(world_with_zone("Halle"), 1);
    let s = slot(&sim, 3);
    let id = sim.justice_crime("DisorderlyBehavior", s, 4, false, 1);
    sim.run_ticks(3 * 1200);
    let c = sim.crimes().iter().find(|c| c.id == id).unwrap().clone();
    assert_eq!(c.status, "Unsolved");
    assert_eq!(c.criminal, -1);
    assert!(!c.witnesses.is_empty(), "the hall is small: others saw it");
    assert_eq!(sim.justice_report().no_enforcer, 1);
}

#[test]
fn sheriff_identifies_witnessed_crime_and_beats_without_jail() {
    let mut w = world_with_zone("Halle");
    w.nobles.push(noble("SHERIFF", 5));
    let mut sim = Sim::new(w, 2);
    let s = slot(&sim, 3);
    let id = sim.justice_crime("DisorderlyBehavior", s, 4, false, 1);
    sim.run_ticks(2 * 1200);
    let c = sim.crimes().iter().find(|c| c.id == id).unwrap().clone();
    assert_eq!(c.status, "Accused");
    assert_eq!(c.criminal, 3);
    // No jail: the jail sentence becomes a beating by the sheriff.
    sim.convict(id, 3);
    let c = sim.crimes().iter().find(|c| c.id == id).unwrap().clone();
    assert_eq!(c.punishment, "Beating");
    assert_eq!(c.status, "Punished");
    assert!(thought(&sim, 3, "GotBeaten"));
    assert!(thought(&sim, 5, "GaveBeating"));
    assert!(thought(&sim, 4, "ConvictionJusticeSelf"));
    assert!(!sim.world.unit(3).unwrap().wounds.is_empty());
    let r = sim.justice_report();
    assert_eq!((r.beatings, r.no_jail, r.wrongful), (1, 1, 0));
}

#[test]
fn jail_time_in_a_jail_with_restraints() {
    let mut w = world_with_zone("Halle");
    let hz = (20i16 * 2 / 3) - 4;
    w.nobles.push(noble("CAPTAIN_OF_THE_GUARD", 5));
    let jail = Pos::new(28, 27, hz);
    w.buildings.push(Building { id: 800, kind: "Civzone".into(), subtype: "Dungeon".into(), name: "Gefaengnis".into(), x1: 27, y1: 26, x2: 30, y2: 28, z: hz, center: jail, built: true, owner: -1, ..Default::default() });
    w.buildings.push(Building { id: 801, kind: "Chain".into(), x1: jail.x, y1: jail.y, x2: jail.x, y2: jail.y, z: hz, center: jail, built: true, owner: -1, ..Default::default() });
    let mut sim = Sim::new(w, 3);
    sim.set_auto_convict(true);
    let s = slot(&sim, 2);
    let id = sim.justice_crime("BuildingDestruction", s, -1, false, 1);
    sim.run_ticks(3 * 1200);
    let c = sim.crimes().iter().find(|c| c.id == id).unwrap().clone();
    assert_eq!(c.status, "Serving", "{c:?}");
    assert_eq!(c.punishment, "Jail 14 days");
    assert!(sim.is_prisoner(2));
    sim.run_ticks(5 * 1200);
    let near = sim.world.unit(2).unwrap().pos.dist(jail) <= 1;
    assert!(near, "the prisoner is restrained at the chain: {:?}", sim.world.unit(2).unwrap().pos);
    assert!(thought(&sim, 2, "Jailed"));
    sim.run_ticks(12 * 1200);
    assert!(!sim.is_prisoner(2));
    let c = sim.crimes().iter().find(|c| c.id == id).unwrap().clone();
    assert_eq!(c.status, "Punished");
    assert!(thought(&sim, 2, "JailReleased"));
    assert!(!sim.world.unit(2).unwrap().flags.dead, "fed and watered in jail");
}

#[test]
fn hammerer_punishes_production_order_violations() {
    let mut w = world_with_zone("Halle");
    let hz = (20i16 * 2 / 3) - 4;
    w.nobles.push(noble("SHERIFF", 5));
    w.nobles.push(noble("HAMMERER", 6));
    w.nobles.push(noble("MAYOR", 7));
    w.items.push(Item {
        id: 9100,
        kind: "WEAPON".into(),
        subtype: "ITEM_WEAPON_HAMMER_WAR".into(),
        mat: 2,
        pos: Pos::new(19, 21, hz),
        stack: 1,
        container: -1,
        holder: -1,
        building: -1,
        flags: ItemFlags { on_ground: true, ..Default::default() },
        ..Default::default()
    });
    let mut sim = Sim::new(w, 4);
    sim.justice_mandate_missed("MAYOR", false, 5);
    let c = sim.crimes()[0].clone();
    assert_eq!(c.kind, "ProductionOrderViolation");
    assert_eq!(c.status, "Accused", "the mayor names the culprit");
    sim.convict(c.id, c.criminal);
    let c = sim.crimes()[0].clone();
    assert_eq!(c.punishment, "Hammerstrikes 5");
    assert!(thought(&sim, c.criminal, "GotHammered"));
    assert!(thought(&sim, 6, "GaveHammering"));
    assert_eq!(sim.justice_report().hammerstrikes, 5);
}

#[test]
fn convicting_the_wrong_dwarf_is_wrongful() {
    let mut w = world_with_zone("Halle");
    w.nobles.push(noble("SHERIFF", 5));
    let mut sim = Sim::new(w, 5);
    let s = slot(&sim, 3);
    let id = sim.justice_crime("DisorderlyBehavior", s, 4, false, 1);
    sim.convict(id, 2);
    assert_eq!(sim.justice_report().wrongful, 1);
    assert!(!thought(&sim, 4, "ConvictionJusticeSelf"));
}
