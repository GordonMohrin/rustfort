//! [vampire13] Tests of the vampire and werebeast module (one per mechanic).

use super::*;
use rf_core::World;

fn mk(seed: u64) -> Sim {
    let mut sim = Sim::new(World::synthetic(48, 48, 20), seed);
    sim.set_invasions(false);
    sim.set_moods_enabled(false);
    sim.run_ticks(200);
    sim
}

fn slot(sim: &Sim, id: i32) -> usize {
    sim.unit_slot[&id]
}

/// A citizen of the fort other than `not`.
fn other_citizen(sim: &Sim, not: &[i32]) -> i32 {
    sim.ai.iter().find(|a| a.kind == Kind::Worker && !not.contains(&a.id)).map(|a| a.id).expect("citizen")
}

fn set_day(sim: &mut Sim, day: i32) {
    sim.world.calendar.year_tick = day * TICKS_PER_DAY;
}

#[test]
fn moon_phase_follows_the_calendar() {
    let mut sim = mk(1);
    set_day(&mut sim, 0);
    assert_eq!(sim.moon_phase(), 0);
    set_day(&mut sim, 14);
    assert!(sim.is_full_moon());
    set_day(&mut sim, 28 + 14);
    assert!(sim.is_full_moon(), "one full moon each month");
    set_day(&mut sim, 27);
    assert_eq!(sim.moon_phase(), 27);
}

#[test]
fn vampire_migrant_has_false_identity_and_no_needs() {
    let mut sim = mk(2);
    let id = sim.spawn_vampire_migrant().expect("vampire");
    let u = sim.world.unit(id).unwrap();
    assert_eq!(u.curse, "VAMPIRE");
    assert!(!u.true_name.is_empty() && u.true_name != u.name, "false identity");
    assert!(u.skills.len() >= 4, "many skills");
    let age = u.age;
    let c = sim.unit_curse(id).unwrap();
    assert!(c.vampire && !c.vampire_unmasked);
    sim.set_vampires_enabled(false); // no feeding in this test
    sim.run_ticks(5_000);
    let s = slot(&sim, id);
    assert!(sim.ai[s].thirst < 200 && sim.ai[s].hunger < 200 && sim.ai[s].sleep < 200, "no needs");
    assert_eq!(sim.world.unit(id).unwrap().age, age, "does not age");
    assert_eq!(sim.vampire_report().vampires_arrived, 1);
}

#[test]
fn vampire_drinks_from_a_sleeper_unseen() {
    let mut sim = mk(3);
    let id = sim.spawn_vampire_migrant().expect("vampire");
    for a in sim.ai.iter_mut() {
        if a.kind == Kind::Worker && a.id != id {
            a.sleeping = true; // everyone else sleeps: nobody can see it
        }
    }
    sim.vamp.vamps[0].next_feed = 0;
    let now = sim.ticks;
    sim.vamp_upkeep(now);
    let rep = sim.vampire_report();
    assert_eq!(rep.feedings, 1);
    assert_eq!(rep.unmasked, 0, "no witness");
    assert!(!sim.vamp.vamps[0].unmasked);
    if rep.drained == 1 {
        assert!(sim.world.crimes.iter().any(|c| c.kind == "Murder" && c.status == "Unsolved" && c.witnesses.is_empty()));
        assert!(sim.announcements.iter().any(|a| a.text.ends_with("has been drained of blood.")));
    }
    assert!(sim.vamp.vamps[0].next_feed >= now + FEED_DAYS_MIN * DAY);
}

#[test]
fn vampire_seen_feeding_is_unmasked() {
    // Over several seeds a witnessed feeding must occur and unmask the vampire.
    let mut caught = 0;
    for seed in 10..20 {
        let mut sim = mk(seed);
        let id = sim.spawn_vampire_migrant().expect("vampire");
        let victim = other_citizen(&sim, &[id]);
        let vs = slot(&sim, victim);
        let vpos = sim.ai[vs].pos;
        for k in 0..sim.ai.len() {
            if sim.ai[k].kind == Kind::Worker && sim.ai[k].id != id && k != vs {
                sim.ai[k].sleeping = false;
                sim.ai[k].pos = vpos; // standing right beside the sleeper
                let p = sim.world.pos_of(vpos as usize);
                let ui = sim.ai[k].uidx;
                sim.world.units[ui].pos = p;
            }
        }
        sim.ai[vs].sleeping = true;
        sim.vamp.vamps[0].next_feed = 0;
        let now = sim.ticks;
        sim.vamp_upkeep(now);
        if sim.vampire_report().unmasked == 1 {
            caught += 1;
            let u = sim.world.unit(id).unwrap();
            assert_eq!(u.name, u.true_name, "true name revealed");
            assert!(sim.unit_curse(id).unwrap().vampire_unmasked);
            assert!(sim.world.crimes.iter().any(|c| c.criminal == id && c.status == "Accused"));
            assert!(sim.threats().iter().any(|t| t.kind == ThreatKind::Vampire && t.units == vec![id]));
        }
    }
    assert!(caught >= 5, "witnesses see the vampire (caught {caught} of 10)");
}

#[test]
fn accusing_unmasks_only_real_vampires() {
    let mut sim = mk(4);
    let id = sim.spawn_vampire_migrant().expect("vampire");
    let innocent = other_citizen(&sim, &[id]);
    assert!(!sim.accuse_vampire(innocent));
    assert!(sim.accuse_vampire(id));
    assert_eq!(sim.vampire_report().unmasked, 1);
    let u = sim.world.unit(id).unwrap();
    assert_eq!(u.name, u.true_name);
}

#[test]
fn migrant_waves_bring_vampires_only_when_enabled() {
    let mut sim = mk(5);
    sim.set_vampires_enabled(false);
    for _ in 0..40 {
        sim.spawn_migrants(3);
        sim.run_ticks(100);
    }
    assert_eq!(sim.vampire_report().vampires_arrived, 0);
    let mut sim = mk(5);
    for _ in 0..40 {
        sim.spawn_migrants(3);
        sim.run_ticks(100);
    }
    let n = sim.vampire_report().vampires_arrived;
    assert!(n >= 1 && n <= 12, "about 1 wave in 12 brings one ({n} of 40)");
    assert!(sim.world.units.iter().filter(|u| u.curse == "VAMPIRE").all(|u| u.age >= 18.0 && u.race == "DWARF"));
}

#[test]
fn cursed_citizen_turns_at_full_moon_and_back() {
    let mut sim = mk(6);
    let id = other_citizen(&sim, &[]);
    let s = slot(&sim, id);
    let uidx = sim.ai[s].uidx;
    let now = sim.ticks;
    sim.curse_were(uidx, "WEREWOLF", false, now);
    set_day(&mut sim, 14);
    sim.moon_day_start(now);
    assert!(sim.unit_curse(id).unwrap().transformed);
    assert!(sim.world.units[uidx].flags.hostile);
    assert_eq!(sim.ai[s].kind, Kind::Other);
    assert!(sim.threats().iter().any(|t| t.kind == ThreatKind::Werebeast));
    sim.run_ticks(50);
    set_day(&mut sim, 15);
    let now = sim.ticks;
    sim.moon_day_start(now);
    let c = sim.unit_curse(id).unwrap();
    assert!(c.werebeast && !c.transformed);
    if sim.ai[s].kind != Kind::Dead {
        assert!(!sim.world.units[uidx].flags.hostile);
        assert_eq!(sim.ai[s].kind, Kind::Worker, "back to work");
    }
    assert_eq!(sim.vampire_report().transformations, 1);
}

#[test]
fn werebeast_attack_and_bite_spreads_the_curse() {
    let mut sim = mk(7);
    let wid = sim.spawn_werebeast_attack().expect("werebeast");
    let ws = slot(&sim, wid);
    assert!(sim.world.unit(wid).unwrap().flags.hostile);
    assert_eq!(sim.vampire_report().were_attacks, 1);
    let victim = other_citizen(&sim, &[]);
    let vs = slot(&sim, victim);
    let w0 = sim.combat.stats.wounds;
    sim.combat.stats.wounds += 1; // the attack wounded the victim
    sim.were_after_attack(ws, vs, w0);
    let now = sim.ticks;
    sim.were_infect(now);
    let u = sim.world.unit(victim).unwrap();
    assert_eq!(u.curse, "WEREBEAST");
    assert_eq!(u.curse_race, sim.world.unit(wid).unwrap().curse_race);
    assert_eq!(sim.vampire_report().infected, 1);
    // After the full moon the attacker leaves.
    set_day(&mut sim, 15);
    sim.moon_day_start(now);
    assert!(!sim.vamp.weres.iter().any(|w| w.id == wid));
}

#[test]
fn quiet_module_hashes_nothing_and_roundtrips() {
    let mut sim = mk(8);
    let mut n = 0;
    sim.vampires_hash(&mut |_| n += 1);
    assert_eq!(n, 0, "inactive: nothing mixed");
    sim.spawn_vampire_migrant().expect("vampire");
    sim.vampires_hash(&mut |_| n += 1);
    assert!(n > 0);
    let json = serde_json::to_string(&sim.vamp).unwrap();
    let back: Vamp = serde_json::from_str(&json).unwrap();
    assert_eq!(back.vamps.len(), 1);
    assert_eq!(back.vamps[0].true_name_check(), sim.vamp.vamps[0].true_name_check());
}

impl VampRec {
    fn true_name_check(&self) -> (i32, String, u64) {
        (self.id, self.assumed.clone(), self.next_feed)
    }
}
