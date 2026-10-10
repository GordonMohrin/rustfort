//! [society2] Tests: locations, occupations, visitors, petitions.

use super::*;
use rf_core::{Building, EntityInfo, HistFigName, Item, ItemFlags, UnitNeed, World};

/// Synthetic fort whose dug hall (x 18..30, y 20..28) is a named meeting hall.
pub(crate) fn world_with_zone(name: &str) -> World {
    let mut w = World::synthetic(48, 48, 20);
    let hz = (20i16 * 2 / 3) - 4;
    w.buildings.push(Building {
        id: 700,
        kind: "Civzone".into(),
        subtype: "MeetingHall".into(),
        name: name.into(),
        x1: 18,
        y1: 20,
        x2: 30,
        y2: 28,
        z: hz,
        center: Pos::new(24, 24, hz),
        built: true,
        owner: -1,
        ..Default::default()
    });
    w.entities.push(EntityInfo { id: 1, name: "The Synthetic Union".into(), race: "DWARF".into(), kind: "Civilization".into(), relation: "Own".into(), trades: true });
    w
}

pub(crate) fn slot(sim: &Sim, id: i32) -> usize {
    sim.unit_slot[&id]
}

fn has_thought(sim: &Sim, id: i32, t: &str) -> bool {
    sim.world.unit(id).unwrap().emotions.iter().any(|e| e.thought == t)
}

#[test]
fn temples_are_derived_from_zone_names_with_their_deity() {
    // Zone names typed in DF arrive mangled: "R\u{251c}\u{ab}sen" for "Rîsen".
    let mut w = world_with_zone("Kapelle R\u{251c}\u{ab}sen");
    w.hist_figs.push(HistFigName { id: 42, name: "Rîsen".into(), race: "DWARF".into(), alive: true, unit_id: -1 });
    w.units[0].needs.push(UnitNeed { id: "PrayOrMeditate".into(), deity_id: 42, focus_level: -5_000, need_level: 2 });
    let mut sim = Sim::new(w, 1);
    sim.society2_ensure();
    assert_eq!(sim.world.locations.len(), 1);
    assert_eq!(sim.world.locations[0].kind, "Temple");
    assert_eq!(sim.world.locations[0].deity, 42);
    let p = sim.world.units[0].pos;
    assert_eq!(sim.location_at(p), Some(1));
    let s = slot(&sim, 1);
    sim.locations_prayed(s, p, 42, 10);
    sim.locations_prayed(s, p, 7, 11); // another deity: no dedicated-temple thought
    assert_eq!(sim.location_report().dedicated_prayers, 1);
    assert!(has_thought(&sim, 1, "PrayedDedicatedTemple"));
}

#[test]
fn tavern_keeper_serves_drinks_and_performers_perform() {
    let mut w = world_with_zone("Taverne");
    let hz = (20i16 * 2 / 3) - 4;
    w.items.push(Item {
        id: 9000,
        kind: "DRINK".into(),
        mat: 5,
        pos: Pos::new(20, 21, hz),
        stack: 20,
        container: -1,
        holder: -1,
        building: -1,
        desc: "dwarven wine".into(),
        flags: ItemFlags { on_ground: true, ..Default::default() },
        ..Default::default()
    });
    for u in w.units.iter_mut().filter(|u| u.flags.citizen) {
        u.needs.push(UnitNeed { id: "DrinkAlcohol".into(), deity_id: -1, focus_level: -2_000, need_level: 1 });
        u.needs.push(UnitNeed { id: "MakeMerry".into(), deity_id: -1, focus_level: -2_000, need_level: 1 });
        u.labors.clear();
    }
    let mut sim = Sim::new(w, 3);
    sim.society2_ensure();
    sim.assign_occupation(1, "TAVERN_KEEPER", 1);
    sim.assign_occupation(1, "PERFORMER", 2);
    sim.run_ticks(3 * 1200);
    let r = sim.location_report();
    assert!(r.drinks_served >= 1, "{r:?}");
    assert!(r.performances >= 1 && r.audience >= 1, "{r:?}");
    assert!(has_thought(&sim, 2, "Perform"));
    assert!((3..=7).any(|id| has_thought(&sim, id, "WatchPerform")));
    assert!(r.usage[0].3 > 0);
}

#[test]
fn visitors_petition_and_unanswered_petitions_are_denied() {
    let w = world_with_zone("Taverne");
    let mut sim = Sim::new(w, 5);
    sim.society2_ensure();
    let perf = sim.spawn_visitor_for(VisitorRole::Performer, 1).expect("performer");
    let merc = sim.spawn_visitor_for(VisitorRole::Mercenary, 1).expect("mercenary");
    assert_eq!(sim.visitors().len(), 2);
    assert!(sim.world.unit(perf).unwrap().flags.visitor);
    // Petitions come 7-21 days after arrival.
    sim.run_ticks(23 * 1200);
    let ps = sim.petitions();
    assert_eq!(ps.len(), 2, "{ps:?}");
    let pp = ps.iter().find(|p| p.unit == perf).unwrap().clone();
    assert_eq!(pp.kind, "Residency");
    assert!(sim.answer_petition(pp.id, true));
    assert!(sim.world.unit(perf).unwrap().flags.resident);
    assert!(sim.world.locations[0].occupations.iter().any(|o| o.kind == "PERFORMER" && o.unit == perf));
    // Nobody answers the mercenary: denied after 28 days, and he leaves.
    sim.run_ticks(60 * 1200);
    let pm = sim.petitions().into_iter().find(|p| p.unit == merc).unwrap();
    assert_eq!(pm.status, "Expired");
    assert!(sim.world.unit(merc).unwrap().flags.inactive, "the mercenary left");
    assert!(!sim.world.unit(perf).unwrap().flags.inactive, "the resident stays");
    assert_eq!(sim.location_report().denied, 1);
}

#[test]
fn accepted_citizenship_makes_a_citizen() {
    let w = world_with_zone("Taverne");
    let mut sim = Sim::new(w, 6);
    sim.society2_ensure();
    let merc = sim.spawn_visitor_for(VisitorRole::MonsterSlayer, 1).unwrap();
    sim.run_ticks(23 * 1200);
    let p = sim.petitions().into_iter().find(|p| p.unit == merc).unwrap();
    assert_eq!(p.kind, "Citizenship");
    let before = sim.world.citizens().count();
    assert!(sim.answer_petition(p.id, true));
    assert!(sim.world.unit(merc).unwrap().flags.citizen);
    sim.run_ticks(1200);
    assert_eq!(sim.world.citizens().count(), before + 1);
    assert!(has_thought(&sim, merc, "BecomeCitizen"));
}

#[test]
fn society2_is_deterministic_across_threads() {
    let run = |par: usize| {
        let mut w = world_with_zone("Taverne");
        w.settings.weather = true;
        let mut sim = Sim::new(w, 21);
        sim.set_parallel_threshold(par);
        sim.society2_ensure();
        sim.assign_occupation(1, "PERFORMER", 2);
        sim.spawn_visitor_for(VisitorRole::Performer, 1);
        sim.spawn_visitor_for(VisitorRole::Scholar, 1);
        let s = slot(&sim, 3);
        sim.justice_crime("DisorderlyBehavior", s, 4, false, 1);
        sim.run_ticks(20_000);
        sim.state_hash()
    };
    let a = run(1);
    let b = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap().install(|| run(1));
    let c = run(1000);
    assert_eq!(a, b);
    assert_eq!(a, c);
}
