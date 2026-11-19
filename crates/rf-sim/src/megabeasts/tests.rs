//! [mega13] Tests on synthetic maps: arrival, triggers, pool, dragonfire, slaying, destroyers.

use super::*;
use rf_core::{Building, HistFigName, Pos, World};

const SZ: i16 = 13; // surface z of `World::synthetic(_, _, 20)`

fn sim_with(w: World, seed: u64) -> Sim {
    let mut sim = Sim::new(w, seed);
    sim.set_invasions(false);
    sim.set_moods_enabled(false);
    sim
}

fn fort(seed: u64) -> Sim {
    let mut w = World::synthetic(48, 48, 20);
    for u in &mut w.units {
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
    }
    sim_with(w, seed)
}

fn on_edge(sim: &Sim, p: Pos) -> bool {
    p.x == 0 || p.y == 0 || p.x == sim.world.dims.x as i16 - 1 || p.y == sim.world.dims.y as i16 - 1
}

#[test]
fn dragon_arrives_at_the_map_edge_and_hunts() {
    let mut sim = fort(1);
    let id = sim.spawn_megabeast("DRAGON").expect("dragon");
    let ann = sim.drain_announcements();
    assert!(ann.iter().any(|a| a.text.starts_with("The dragon ") && a.text.contains(" has come! ")), "{:?}", ann.iter().map(|a| &a.text).collect::<Vec<_>>());
    let u = sim.world.unit(id).unwrap();
    assert!(u.flags.hostile && u.body_size >= 10_000_000);
    assert!(on_edge(&sim, u.pos), "{:?}", u.pos);
    assert!(sim.hostiles().contains(&id));
    let m = sim.megabeasts();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].kind, ThreatKind::Megabeast);
    assert!(m[0].alive);
    assert!(sim.unit_curse(id).unwrap().megabeast);
    let t = sim.threats();
    assert!(t.iter().any(|t| t.kind == ThreatKind::Megabeast && t.units == vec![id] && t.pos.is_some()));
    assert_eq!(sim.megabeast_report().attacks, 1);
    // Not a siege: the war history is untouched.
    assert!(sim.world.fort_history.sieges.is_empty());
    assert!(sim.combat.inv.sieges.is_empty());
    // It closes in on the fort.
    let s = sim.unit_slot[&id];
    let d0 = sim.upos(s);
    sim.run_ticks(600);
    let d1 = sim.upos(s);
    assert!(d0 != d1 || sim.combat_stats().attacks > 0, "it moves");
}

#[test]
fn semi_megabeasts_and_unknown_races() {
    let mut sim = fort(2);
    let id = sim.spawn_megabeast("ETTIN").expect("ettin");
    assert_eq!(sim.megabeasts()[0].kind, ThreatKind::SemiMegabeast);
    assert!(!sim.world.unit(id).unwrap().flags.animal, "humanoid");
    assert!(sim.spawn_megabeast("NOT_A_MEGABEAST").is_none());
}

#[test]
fn natural_arrival_needs_tier_interval_and_the_flag() {
    let mut sim = fort(3);
    sim.combat.inv.enabled = true;
    let tries = |sim: &mut Sim| -> bool {
        let n0 = sim.mega.beasts.len();
        for k in 0..40u64 {
            let s = sim.world.calendar.month() / 3;
            sim.mega.last_season = Some((s + 1) % 4);
            sim.megabeasts_day(1_000 + k * 1_200);
            if sim.mega.beasts.len() > n0 {
                return true;
            }
        }
        false
    };
    // 7 citizens, no wealth: below the third enemy tier.
    sim.world.settings.other.push("MEGABEAST_INTERVAL=1".into());
    assert!(!tries(&mut sim), "tier too low");
    // Third tier reached (custom triggers), but megabeasts are off by difficulty flag.
    sim.world.settings.other.push("ENEMY_POP_TRIGGER=1,2,3,4,5".into());
    sim.world.settings.other.push("ENEMY_PROD_TRIGGER=0,0,0,0,0".into());
    sim.world.settings.other.push("ENEMY_TRADE_TRIGGER=0,0,0,0,0".into());
    sim.world.settings.other.push("FLAG_MEGABEASTS=true".into());
    assert!(!tries(&mut sim), "flag off");
    sim.world.settings.other.retain(|s| s != "FLAG_MEGABEASTS=true");
    sim.set_megabeasts_enabled(false);
    assert!(!tries(&mut sim), "switched off");
    sim.set_megabeasts_enabled(true);
    assert!(tries(&mut sim), "comes once eligible");
    assert_eq!(sim.mega.since_attack, 0);
    // One at a time, and the interval counts from the attack.
    let n = sim.mega.beasts.len();
    assert!(!tries(&mut sim));
    assert_eq!(sim.mega.beasts.len(), n);
}

#[test]
fn interval_is_counted_in_seasons() {
    let mut sim = fort(4);
    sim.combat.inv.enabled = true;
    sim.world.settings.other.push("ENEMY_POP_TRIGGER=1,2,3,4,5".into());
    sim.world.settings.other.push("ENEMY_PROD_TRIGGER=0,0,0,0,0".into());
    sim.world.settings.other.push("ENEMY_TRADE_TRIGGER=0,0,0,0,0".into());
    sim.world.settings.other.push("MEGABEAST_INTERVAL=10".into());
    sim.mega.last_season = Some(9);
    sim.mega.since_attack = 0;
    for k in 0..9u64 {
        sim.mega.last_season = Some(9); // a new season each call
        sim.megabeasts_day(k * 1_200);
    }
    assert_eq!(sim.mega.since_attack, 9);
    assert!(sim.mega.beasts.is_empty(), "nine seasons are not enough");
}

#[test]
fn world_pool_uses_living_historical_figures_once() {
    let mut w = World::synthetic(48, 48, 20);
    w.hist_figs.push(HistFigName { id: 77, name: "Ixrath Goldscale".into(), race: "DRAGON".into(), alive: true, unit_id: -1 });
    w.hist_figs.push(HistFigName { id: 78, name: "Dead Hydra".into(), race: "HYDRA".into(), alive: false, unit_id: -1 });
    let mut sim = sim_with(w, 5);
    let mut r = Rng::new(9);
    let id = sim.mega_spawn_with(&mut r, "", 10).expect("the dragon of the world");
    assert!(sim.world.unit(id).unwrap().name.starts_with("Ixrath Goldscale"));
    assert_eq!(sim.mega.came_hf, vec![77]);
    // The dead hydra cannot come, the dragon came already: the pool is empty.
    for k in 0..10 {
        let mut r = Rng::new(100 + k);
        assert!(sim.mega_spawn_with(&mut r, "", 20).is_none());
    }
}

#[test]
fn titans_are_generated_and_wreck_buildings() {
    let mut sim = fort(63);
    let mut b = Building { id: 9_001, kind: "Workshop".into(), subtype: "Carpenters".into(), x1: 11, y1: 9, x2: 13, y2: 11, z: SZ, center: Pos::new(12, 10, SZ), built: true, ..Default::default() };
    b.stage = -1;
    sim.world.buildings.push(b);
    let id = sim.spawn_megabeast("TITAN").expect("titan");
    let race = sim.world.unit(id).unwrap().race.clone();
    assert!(race.starts_with("TITAN_"), "{race}");
    assert_eq!(Sim::mega_destroyer_level(&race), 2);
    assert_eq!(sim.megabeasts()[0].kind, ThreatKind::Titan);
    let s = sim.unit_slot[&id];
    let q = sim.world.idx(Pos::new(6, 10, SZ)).unwrap() as u32;
    let now = sim.ticks;
    sim.move_to(s, q, now);
    for _ in 0..100 {
        sim.run_ticks(20);
        if sim.megabeast_report().buildings_destroyed > 0 {
            break;
        }
    }
    assert_eq!(sim.megabeast_report().buildings_destroyed, 1, "{:?}", sim.combat_log());
    assert!(!sim.world.buildings.iter().any(|b| b.id == 9_001));
}

#[test]
fn dragonfire_burns_dwarves_to_death() {
    let mut w = World::synthetic(64, 64, 20);
    w.units.retain(|u| u.id == 1);
    w.units[0].pos = Pos::new(20, 30, SZ);
    let mut sim = sim_with(w, 11);
    let id = sim.spawn_megabeast("DRAGON").expect("dragon");
    let b = sim.unit_slot[&id];
    let q = sim.world.idx(Pos::new(26, 30, SZ)).unwrap() as u32;
    let now = sim.ticks;
    sim.move_to(b, q, now);
    let d = sim.unit_slot[&1];
    for _ in 0..200 {
        sim.run_ticks(10);
        if sim.ai[d].kind == Kind::Dead {
            break;
        }
    }
    let log = sim.combat_log();
    assert!(sim.megabeast_report().fire_breaths >= 1, "{log:?}");
    assert!(log.iter().any(|l| l.contains("breathes fire at")), "{log:?}");
    assert_eq!(sim.ai[d].kind, Kind::Dead, "{log:?}");
    assert!(sim.megabeasts()[0].kills >= 1);
}

#[test]
fn fire_immune_and_burn_limit_scale_with_size() {
    let mut w = World::synthetic(48, 48, 20);
    w.units.retain(|u| u.id == 1 || u.id == 2);
    let mut sim = sim_with(w, 12);
    let id = sim.spawn_megabeast("DRAGON").unwrap();
    let dr = sim.unit_slot[&id];
    let d = sim.unit_slot[&1];
    // A big creature takes many burns.
    let big = sim.unit_slot[&2];
    let ui = sim.ai[big].uidx;
    sim.world.units[ui].body_size = 2_000_000;
    let mut r = Rng::new(1);
    let now = sim.ticks;
    sim.mega_ignite(dr, dr, now, &mut r);
    assert!(sim.mega.burning.is_empty(), "the dragon does not burn");
    sim.mega_ignite(d, dr, now, &mut r);
    sim.mega_ignite(big, dr, now, &mut r);
    assert_eq!(sim.mega.burning.len(), 2);
    for k in 1..4 {
        sim.mega_burn_tick(now + k * BURN_PERIOD);
    }
    assert_eq!(sim.ai[d].kind, Kind::Dead, "a dwarf dies after 3 burns");
    assert!(sim.combat_log().iter().any(|l| l.contains("has burned to death")));
    assert_ne!(sim.ai[big].kind, Kind::Dead, "the big one survives");
}

#[test]
fn slaying_is_announced_with_history_and_thoughts() {
    let mut sim = fort(7);
    let id = sim.spawn_megabeast("GIANT").expect("giant");
    let s = sim.unit_slot[&id];
    let d = sim.unit_slot[&1];
    sim.drain_announcements();
    sim.mega_blow(d, s, false);
    let now = sim.ticks;
    sim.combat_kill(s, now, "has been struck down");
    sim.megabeasts_day(now + 1);
    let m = &sim.megabeasts()[0];
    assert!(!m.alive);
    assert_eq!(m.slain_by, 1);
    assert_eq!(sim.megabeast_report().slain, 1);
    let ann = sim.drain_announcements();
    assert!(ann.iter().any(|a| a.text.starts_with("The giant ") && a.text.contains(" has been slain by ")), "{:?}", ann.iter().map(|a| &a.text).collect::<Vec<_>>());
    assert_eq!(sim.megabeast_history().len(), 2, "arrival and death");
    let u = sim.world.unit(1).unwrap();
    assert!(u.emotions.iter().any(|e| e.thought == "SlewMegabeast"), "the slayer is proud");
    let other = sim.world.units.iter().find(|u| u.id != 1 && u.flags.citizen).unwrap();
    assert!(other.emotions.iter().any(|e| e.thought == "MegabeastSlain"));
    assert!(sim.mega.slots.is_empty());
    assert!(!sim.threats().iter().any(|t| t.kind == ThreatKind::Megabeast));
}

#[test]
fn threats_list_forgotten_beasts() {
    let mut sim = fort(8);
    let id = sim.spawn_forgotten_beast().expect("beast");
    let t = sim.threats();
    assert!(t.iter().any(|t| t.kind == ThreatKind::ForgottenBeast && t.units == vec![id]), "{t:?}");
}

#[test]
fn hash_is_quiet_without_megabeasts() {
    let sim = fort(9);
    let mut n = 0;
    sim.megabeasts_hash(&mut |_| n += 1);
    assert_eq!(n, 0);
}

#[test]
fn megabeast_fight_continues_bit_identically_after_save_and_load() {
    for (n, m) in [(300u32, 1_500u32), (900, 1_200)] {
        let mut w = World::synthetic(64, 64, 20);
        w.units.retain(|u| u.id <= 3);
        let mut a = sim_with(w, 21);
        let id = a.spawn_megabeast("DRAGON").expect("dragon");
        let s = a.unit_slot[&id];
        let q = a.world.idx(Pos::new(20, 20, SZ)).unwrap() as u32;
        let now = a.ticks;
        a.move_to(s, q, now);
        a.run_ticks(n);
        let w = a.world_for_save();
        let raw = rmp_serde::to_vec_named(&w).unwrap();
        let mut w: World = rmp_serde::from_slice(&raw).unwrap();
        w.tiletypes.reindex();
        let mut b = Sim::new(w, 21);
        assert_eq!(b.restore_mode(), "exact");
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(a.megabeasts().len(), b.megabeasts().len());
        a.run_ticks(m);
        b.run_ticks(m);
        assert_eq!(a.state_hash(), b.state_hash(), "N {n} M {m}");
        assert_eq!(a.megabeast_report().fire_breaths, b.megabeast_report().fire_breaths);
    }
}

// ---------------------------------------------------------------- verifier [mega13]

#[test]
fn vanilla_table_uses_raw_tokens_and_adult_sizes() {
    // Vanilla raws (creature_standard): the roc is BIRD_ROC; adult BODY_SIZE values.
    for (race, size) in [("DRAGON", 25_000_000), ("HYDRA", 8_000_000), ("BIRD_ROC", 20_000_000), ("COLOSSUS_BRONZE", 20_000_000), ("GIANT", 9_000_000), ("ETTIN", 8_000_000), ("CYCLOPS", 8_000_000), ("MINOTAUR", 220_000)] {
        let mut sim = fort(31);
        let id = sim.spawn_megabeast(race).unwrap_or_else(|| panic!("{race}"));
        assert_eq!(sim.world.unit(id).unwrap().body_size, size, "{race}");
    }
    // Kinds: the roc is a megabeast.
    let mut sim = fort(32);
    sim.spawn_megabeast("BIRD_ROC").unwrap();
    assert_eq!(sim.megabeasts()[0].kind, ThreatKind::Megabeast);
}

#[test]
fn hydra_and_colossus_feel_no_pain() {
    for race in ["HYDRA", "COLOSSUS_BRONZE"] {
        let mut sim = fort(33);
        let id = sim.spawn_megabeast(race).unwrap();
        let s = sim.unit_slot[&id];
        assert!(sim.combat.bodies[sim.combat.f[s].body as usize].nopain, "{race}");
    }
    let mut sim = fort(33);
    let id = sim.spawn_megabeast("DRAGON").unwrap();
    let s = sim.unit_slot[&id];
    assert!(!sim.combat.bodies[sim.combat.f[s].body as usize].nopain);
}

#[test]
fn population_alone_does_not_bring_megabeasts() {
    // wiki Megabeast: population 80 AND created wealth 100 000 AND exported wealth 10 000.
    let mut sim = fort(34);
    sim.combat.inv.enabled = true;
    sim.world.settings.other.push("MEGABEAST_INTERVAL=1".into());
    sim.world.settings.other.push("ENEMY_POP_TRIGGER=1,2,3,4,5".into());
    assert!(!sim.mega_trigger_met(), "wealth is missing");
    sim.world.settings.other.push("ENEMY_PROD_TRIGGER=0,0,0,0,0".into());
    assert!(!sim.mega_trigger_met(), "exported wealth is missing");
    sim.world.settings.other.push("ENEMY_TRADE_TRIGGER=0,0,0,0,0".into());
    assert!(sim.mega_trigger_met());
}

#[test]
fn a_caged_megabeast_is_caught_not_slain() {
    let mut sim = fort(35);
    let id = sim.spawn_megabeast("HYDRA").unwrap();
    let s = sim.unit_slot[&id];
    let d = sim.unit_slot[&1];
    sim.mega_blow(d, s, false);
    sim.drain_announcements();
    // What `defense::cage_unit` does.
    let ui = sim.ai[s].uidx;
    sim.world.units[ui].flags.caged = true;
    sim.ai[s].kind = Kind::Dead;
    let now = sim.ticks;
    sim.megabeasts_day(now + 1);
    let m = &sim.megabeasts()[0];
    assert!(m.caught && m.alive, "{m:?}");
    assert_eq!(m.slain_by, -1);
    assert_eq!(sim.megabeast_report().slain, 0);
    let ann = sim.drain_announcements();
    assert!(!ann.iter().any(|a| a.text.contains("slain")), "{:?}", ann.iter().map(|a| &a.text).collect::<Vec<_>>());
    assert!(!sim.threats().iter().any(|t| t.kind == ThreatKind::Megabeast));
    assert!(!sim.world.units.iter().any(|u| u.emotions.iter().any(|e| e.thought == "SlewMegabeast")));
}

#[test]
fn killing_blow_is_announced_at_once() {
    let mut sim = fort(36);
    let id = sim.spawn_megabeast("MINOTAUR").unwrap();
    let s = sim.unit_slot[&id];
    let d = sim.unit_slot[&1];
    sim.drain_announcements();
    let now = sim.ticks;
    sim.combat_kill(s, now, "has been struck down");
    sim.mega_blow(d, s, true); // the strike hook after a kill
    assert!(!sim.megabeasts()[0].alive);
    assert_eq!(sim.megabeasts()[0].slain_by, 1);
    assert!(sim.drain_announcements().iter().any(|a| a.text.contains("has been slain by")));
    assert!(sim.mega.slots.is_empty());
}

/// Run season starts until something comes (at most 40); true if it did.
fn season_tries(sim: &mut Sim) -> bool {
    let n0 = sim.mega.beasts.len();
    for k in 0..40u64 {
        let s = sim.world.calendar.month() / 3;
        sim.mega.last_season = Some((s + 1) % 4);
        sim.megabeasts_day(1_000 + k * 1_200);
        if sim.mega.beasts.len() > n0 {
            return true;
        }
    }
    false
}

#[test]
fn titans_ignore_exported_wealth() {
    // wiki Megabeast: "Titans ignore exported wealth". Population and created wealth met,
    // nothing exported: only a titan may come.
    let mut sim = fort(37);
    sim.combat.inv.enabled = true;
    sim.world.settings.other.push("MEGABEAST_INTERVAL=1".into());
    sim.world.settings.other.push("ENEMY_POP_TRIGGER=1,2,3,4,5".into());
    sim.world.settings.other.push("ENEMY_PROD_TRIGGER=0,0,0,0,0".into());
    sim.world.settings.other.push("ENEMY_TRADE_TRIGGER=0,0,1000000000,0,0".into());
    assert_eq!(sim.mega_triggers(), (false, true));
    assert!(season_tries(&mut sim), "a titan comes");
    assert_eq!(sim.megabeasts()[0].kind, ThreatKind::Titan);
    // Without the population nothing comes.
    let mut sim = fort(38);
    sim.combat.inv.enabled = true;
    sim.world.settings.other.push("MEGABEAST_INTERVAL=1".into());
    sim.world.settings.other.push("ENEMY_PROD_TRIGGER=0,0,0,0,0".into());
    sim.world.settings.other.push("ENEMY_TRADE_TRIGGER=0,0,0,0,0".into());
    assert_eq!(sim.mega_triggers(), (false, false));
    assert!(!season_tries(&mut sim));
}

#[test]
fn titans_of_the_world_list_come_by_name_once() {
    let mut w = World::synthetic(48, 48, 20);
    w.creature_raws.push(rf_core::CreatureRaw { token: "HFR_TITAN_7".into(), name: "titan".into(), flags: vec!["TITAN".into()], ..Default::default() });
    w.hist_figs.push(HistFigName { id: 90, name: "Oddom Stonejaws".into(), race: "HFR_TITAN_7".into(), alive: true, unit_id: -1 });
    let mut sim = sim_with(w, 39);
    let mut r = Rng::new(3);
    let id = sim.mega_spawn_with(&mut r, "", 10).expect("the titan of the world");
    assert_eq!(sim.megabeasts()[0].kind, ThreatKind::Titan);
    assert!(sim.world.unit(id).unwrap().name.starts_with("Oddom Stonejaws"), "{}", sim.world.unit(id).unwrap().name);
    assert_eq!(sim.mega.came_hf, vec![90]);
    for k in 0..10 {
        let mut r = Rng::new(50 + k);
        assert!(sim.mega_spawn_with(&mut r, "", 20).is_none(), "came already");
        assert!(sim.mega_spawn_with(&mut r, "TITAN?", 20).is_none(), "came already");
    }
}

#[test]
fn a_megabeast_already_on_the_map_does_not_come_again() {
    // A historical figure with a unit on the map (e.g. caged in the fort) is not in the pool.
    let mut w = World::synthetic(48, 48, 20);
    w.hist_figs.push(HistFigName { id: 91, name: "Caged Hydra".into(), race: "HYDRA".into(), alive: true, unit_id: 4242 });
    let mut sim = sim_with(w, 40);
    for k in 0..10 {
        let mut r = Rng::new(70 + k);
        assert!(sim.mega_spawn_with(&mut r, "", 20).is_none());
    }
}
