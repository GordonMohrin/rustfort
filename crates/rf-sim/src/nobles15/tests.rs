//! [nobles15] Tests on synthetic forts: position numbers, mandates, justice, export bans,
//! noble room thoughts, rooms and requirements, demands, elections, manager, bookkeeper.

use super::*;
use crate::locations::tests::{slot, world_with_zone};
use rf_core::{Building, Item, MandateDetail, Skill, World};

const HZ: i16 = (20i16 * 2 / 3) - 4;

fn noble(token: &str, holder: i32) -> NoblePosition {
    NoblePosition { token: token.into(), name: token.to_lowercase().replace('_', " "), holder, entity_id: 1, ..Default::default() }
}

fn thought(sim: &Sim, id: i32, t: &str) -> bool {
    sim.world.unit(id).unwrap().emotions.iter().any(|e| e.thought == t)
}

/// A sim whose noble module is ready without running the population module.
fn ready(w: World, seed: u64) -> Sim {
    let mut sim = Sim::new(w, seed);
    sim.n15.ready = true;
    sim.nobles15_refresh_holders();
    sim
}

fn mandate(noble: i32, ban: bool, kind: &str, sub: &str, amount: i32, deadline: u64) -> MandateRt {
    MandateRt {
        id: 900,
        noble,
        position: "MAYOR".into(),
        export_ban: ban,
        item_type: kind.into(),
        item_subtype: sub.into(),
        amount_total: amount,
        amount_remaining: if ban { 0 } else { amount },
        deadline,
        strikes: 3,
        jail_months: 2,
        text: if ban { format!("Ban export of {kind}") } else { format!("Make {amount} {kind}") },
        ..Default::default()
    }
}

fn zone(id: i32, sub: &str, owner: i32, x1: i16, y1: i16, x2: i16, y2: i16) -> Building {
    Building { id, kind: "Civzone".into(), subtype: sub.into(), x1, y1, x2, y2, z: HZ, center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, HZ), built: true, owner, ..Default::default() }
}

fn furniture(id: i32, kind: &str, p: Pos, mat: u16) -> Building {
    Building { id, kind: kind.into(), x1: p.x, y1: p.y, x2: p.x, y2: p.y, z: p.z, center: p, built: true, owner: -1, mat, ..Default::default() }
}

// ------------------------------------------------------------------ positions

#[test]
fn mandate_limits_follow_raws() {
    assert_eq!(pos_req(&noble("MAYOR", 1)).mandate_max, 1);
    assert_eq!(pos_req(&noble("COUNT", 1)).mandate_max, 2);
    assert_eq!(pos_req(&noble("COUNTESS", 1)).mandate_max, 2);
    assert_eq!(pos_req(&noble("MONARCH", 1)).demand_max, 10);
    assert_eq!(pos_req(&noble("EXPEDITION_LEADER", 1)).mandate_max, 0);
    let mayor = pos_req(&noble("MAYOR", 1));
    assert_eq!((mayor.office, mayor.bedroom, mayor.dining, mayor.tomb, mayor.boxes), (500, 500, 500, 0, 2));
    assert!(!mayor.punishment_exempt && !mayor.sleep_pretension && mayor.meets_workers);
    assert!(pos_req(&noble("BARON", 1)).punishment_exempt);
    // Exported numbers win over the table.
    let mut n = noble("MAYOR", 1);
    n.mandate_max = 3;
    n.required_bedroom = 1500;
    let r = pos_req(&n);
    assert_eq!((r.mandate_max, r.bedroom, r.office), (3, 1500, 0));
}

#[test]
fn several_positions_use_the_maximum() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("EXPEDITION_LEADER", 1), noble("MAYOR", 1), noble("MANAGER", 2)];
    let sim = ready(w, 1);
    let h = sim.n15_holder(1).unwrap();
    assert_eq!((h.req.bedroom, h.req.mandate_max), (500, 1));
    assert_eq!(sim.world.nobles[h.ni].token, "MAYOR", "best rank names the holder");
    assert_eq!(sim.n15.req_tier[slot(&sim, 1)], [4, 4, 4, 0]);
    assert_eq!(sim.n15.req_tier[slot(&sim, 2)], [0, 1, 0, 0], "manager: any office");
}

// ------------------------------------------------------------------ mandates

#[test]
fn deadline_in_three_to_six_months() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1)];
    let mut sim = ready(w, 2);
    let (mut lo, mut hi, mut bans) = (u64::MAX, 0, 0);
    for k in 0..1000u64 {
        sim.n15.mandates.clear();
        let mut r = Rng::for_unit(k, 0, k, 7);
        sim.n15_issue_mandate(0, &mut r, 1000);
        let m = &sim.n15.mandates[0];
        assert!((1..=5).contains(&m.strikes) && (1..=4).contains(&m.jail_months));
        if m.export_ban {
            bans += 1;
            assert_eq!(m.deadline, 1000 + BAN_DURATION);
        } else {
            lo = lo.min(m.deadline - 1000);
            hi = hi.max(m.deadline - 1000);
            assert!((1..=5).contains(&m.amount_total));
        }
    }
    assert!(lo >= 3 * MONTH && hi <= 6 * MONTH + 10, "{lo} {hi}");
    assert!(hi > 5 * MONTH && lo < 4 * MONTH, "spread over the range");
    assert!((200..450).contains(&bans), "about one in three are bans: {bans}");
}

#[test]
fn mandates_respect_max_and_cooldown() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1)];
    w.settings.other.push("MANDATE_PERIOD=1".into());
    let mut sim = ready(w, 3);
    sim.n15_mandates_tick(1000);
    assert_eq!(sim.n15.mandates.len(), 1, "chance 1 in 1");
    sim.n15.mandates.clear();
    sim.n15_mandates_tick(2000);
    assert!(sim.n15.mandates.is_empty(), "cooldown of a month");
    sim.n15_mandates_tick(1000 + MANDATE_COOLDOWN);
    assert_eq!(sim.n15.mandates.len(), 1);
    sim.n15_mandates_tick(1000 + 3 * MANDATE_COOLDOWN);
    assert_eq!(sim.n15.mandates.len(), 1, "MANDATE_MAX 1");
    assert_eq!(sim.world.nobles[0].mandates.len(), 1, "texts kept in sync");
}

#[test]
fn crafted_items_fulfil_caravan_items_do_not() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1)];
    let mut sim = ready(w, 4);
    sim.n15.mandates.push(mandate(1, false, "WEAPON", "ITEM_WEAPON_SPEAR", 2, 10 * DAY));
    let spear = |value| Item { id: 5000, kind: "WEAPON".into(), subtype: "ITEM_WEAPON_SPEAR".into(), stack: 1, value, holder: -1, container: -1, ..Default::default() };
    sim.nobles15_item_made(&spear(500)); // caravan goods arrive valued
    assert_eq!(sim.n15.mandates[0].amount_remaining, 2);
    let axe = Item { subtype: "ITEM_WEAPON_AXE_BATTLE".into(), ..spear(0) };
    sim.nobles15_item_made(&axe);
    assert_eq!(sim.n15.mandates[0].amount_remaining, 2, "subtype must match");
    sim.nobles15_item_made(&spear(0));
    sim.nobles15_item_made(&spear(0));
    assert_eq!(sim.n15.mandates[0].amount_remaining, 0);
    sim.n15_mandates_tick(DAY);
    assert!(sim.n15.mandates.is_empty());
    assert_eq!(sim.n15.rep.mandates_met, 1);
    assert!(thought(&sim, 1, "MandateDeadlineMet"));
}

fn justice_world() -> World {
    let mut w = world_with_zone("Halle");
    let jail = Pos::new(28, 27, HZ);
    w.buildings.push(Building { id: 800, kind: "Civzone".into(), subtype: "Dungeon".into(), name: "Gefaengnis".into(), x1: 27, y1: 26, x2: 30, y2: 28, z: HZ, center: jail, built: true, owner: -1, ..Default::default() });
    w.buildings.push(Building { id: 801, kind: "Chain".into(), x1: jail.x, y1: jail.y, x2: jail.x, y2: jail.y, z: HZ, center: jail, built: true, owner: -1, ..Default::default() });
    let p = w.units[0].pos;
    w.items.push(Item { id: 7000, kind: "WEAPON".into(), subtype: "ITEM_WEAPON_HAMMER_WAR".into(), stack: 1, pos: p, holder: -1, container: -1, ..Default::default() });
    w.nobles = vec![noble("MAYOR", 1), noble("HAMMERER", 5), noble("SHERIFF", 6)];
    w
}

#[test]
fn missed_mandate_sentences_both() {
    let mut sim = ready(justice_world(), 5);
    let m = mandate(1, false, "WEAPON", "ITEM_WEAPON_SPEAR", 3, 100);
    sim.n15.mandates.push(m);
    sim.n15_mandates_tick(200);
    assert_eq!(sim.n15.rep.mandates_missed, 1);
    assert!(thought(&sim, 1, "MandateIgnored"));
    let c = sim.crimes().iter().find(|c| c.kind == "ProductionOrderViolation").expect("crime").clone();
    assert_eq!(c.victim, 1);
    assert!(c.witnesses.is_empty(), "OBSERVED: no witnesses");
    assert_eq!(c.status, "Serving", "{c:?}");
    assert_eq!(c.punishment, "Hammerstrikes 3, jail 56 days");
    assert_eq!(c.sentence_days, 56);
    assert!(thought(&sim, c.convicted, "GotHammered"));
    assert!(sim.is_prisoner(c.convicted));
    assert_eq!(sim.n15.rep.sentences.len(), 1);
}

#[test]
fn exempt_positions_never_blamed() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1)];
    for id in 2..=7 {
        w.nobles.push(noble("BARON", id)); // everyone else is exempt
    }
    let mut sim = ready(w, 6);
    let m = mandate(1, false, "CHAIR", "", 1, 0);
    let ns = slot(&sim, 1);
    assert_eq!(sim.n15_pick_culprit(&m, ns, 10), None);
    sim.n15.mandates.push(m);
    sim.n15_mandates_tick(10);
    assert_eq!(sim.n15.rep.no_punishment, 1);
    assert!(thought(&sim, 1, "NoPunishment"));
    assert!(sim.crimes().is_empty());
}

#[test]
fn culprits_tend_to_have_the_skill() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1)];
    w.units[3].skills.push(Skill { id: "FORGE_WEAPON".into(), rating: 3, xp: 0 });
    let sim = ready(w, 7);
    let m = mandate(1, false, "WEAPON", "ITEM_WEAPON_SPEAR", 1, 0);
    let ns = slot(&sim, 1);
    let smith = slot(&sim, 4);
    let picks = (0..400u64).filter(|&t| sim.n15_pick_culprit(&m, ns, t) == Some(smith)).count();
    // weight 4 of 4 + 5 others = 4/9 ~ 178 of 400.
    assert!((120..240).contains(&picks), "{picks}");
    assert!((0..400u64).all(|t| sim.n15_pick_culprit(&m, ns, t) != Some(ns)), "never the noble");
}

#[test]
fn exempt_nobles_are_not_punished() {
    let mut w = justice_world();
    w.nobles.push(noble("BARON", 3));
    let mut sim = ready(w, 8);
    let s = slot(&sim, 3);
    let id = sim.justice_crime("Theft", s, 2, true, 1);
    sim.convict(id, 3);
    let c = sim.crimes().iter().find(|c| c.id == id).unwrap();
    assert_eq!((c.status.as_str(), c.punishment.as_str()), ("Closed", "None (exempt)"));
    assert!(!sim.is_prisoner(3));
}

#[test]
fn imported_mandate_keeps_counter() {
    let mut w = World::synthetic(48, 48, 20);
    let mut n = noble("MAYOR", 1);
    n.mandates = vec!["Make 3 spears".into()];
    n.mandates_detail = vec![MandateDetail {
        text: "Make 3 spears".into(),
        mode: "Make".into(),
        item_type: "WEAPON".into(),
        item_subtype: "ITEM_WEAPON_SPEAR".into(),
        amount_total: 3,
        amount_remaining: 2,
        timeout_counter: 5552,
        timeout_limit: 10762,
        punishment_hammerstrikes: 1,
        punishment_prison_time: 3,
        ..Default::default()
    }];
    w.nobles = vec![n];
    let mut sim = Sim::new(w, 9);
    sim.nobles15_init(500);
    assert_eq!(sim.n15.mandates.len(), 1, "the parsed text is not doubled");
    let m = &sim.n15.mandates[0];
    assert_eq!(m.deadline, 500 + (10762 - 5552) * 10);
    assert_eq!((m.amount_remaining, m.strikes, m.jail_months), (2, 1, 3));
    assert!(sim.pop.mandates.is_empty());
}

#[test]
fn mandates_dropped_on_death_and_lost_election() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1), noble("BARON", 2)];
    let mut sim = ready(w, 10);
    sim.n15.mandates.push(mandate(1, false, "CHAIR", "", 1, 10 * DAY));
    sim.n15.mandates.push(mandate(2, false, "TABLE", "", 1, 10 * DAY));
    // Mayor voted out: the winner (3) feels Elected, the loser's mandates end.
    sim.world.nobles[0].holder = 3;
    let s3 = slot(&sim, 3);
    sim.nobles15_elected(s3, 1, true, 100);
    assert!(thought(&sim, 3, "Elected"));
    assert!(sim.n15.mandates.iter().all(|m| m.noble != 1));
    // Re-election of the incumbent.
    sim.nobles15_elected(s3, 3, false, 200);
    assert!(thought(&sim, 3, "Reelected"));
    // The baron dies.
    let s2 = slot(&sim, 2);
    sim.kill(s2, 300, "has died");
    sim.n15_mandates_tick(400);
    assert!(sim.n15.mandates.is_empty());
}

#[test]
fn export_ban_hauler_blamed_and_bin_unblamed() {
    let mut w = justice_world();
    let p = Pos::new(10, 10, HZ);
    w.buildings.push(Building { id: 950, kind: "TradeDepot".into(), x1: 8, y1: 8, x2: 12, y2: 12, z: HZ, center: p, built: true, owner: -1, ..Default::default() });
    let spear = |id, container| Item { id, kind: "WEAPON".into(), subtype: "ITEM_WEAPON_SPEAR".into(), stack: 1, pos: p, holder: -1, container, building: 950, ..Default::default() };
    w.items.push(spear(6001, -1));
    w.items.push(Item { id: 6002, kind: "BIN".into(), stack: 1, pos: p, holder: -1, container: -1, building: 950, ..Default::default() });
    w.items.push(spear(6003, 6002));
    let mut sim = ready(w, 11);
    sim.n15.mandates.push(mandate(1, true, "WEAPON", "ITEM_WEAPON_SPEAR", 0, 10 * DAY));
    assert!(sim.banned_goods().contains(&6001));
    // Unit 4 hauled the loose spear to the depot.
    let s4 = slot(&sim, 4);
    sim.nobles15_depot_hauled(6001, s4 as u32);
    sim.nobles15_items_traded(&[6001], 100);
    let c = sim.crimes().iter().find(|c| c.kind == "ExportViolation").expect("crime").clone();
    assert_eq!(c.criminal, 4);
    assert_eq!(sim.n15.rep.bans_broken, 1);
    // A second ban broken only through a bin: nobody punished.
    sim.n15.mandates.push(mandate(1, true, "WEAPON", "ITEM_WEAPON_SPEAR", 0, 10 * DAY));
    sim.nobles15_items_traded(&[6002], 200);
    assert_eq!(sim.crimes().iter().filter(|c| c.kind == "ExportViolation").count(), 1);
    assert_eq!(sim.n15.rep.no_punishment, 1);
    assert!(thought(&sim, 1, "NoPunishment"));
}

#[test]
fn export_ban_runs_out_quietly() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1)];
    let mut sim = ready(w, 12);
    sim.n15.mandates.push(mandate(1, true, "WEAPON", "", 0, 50));
    sim.n15_mandates_tick(100);
    assert!(sim.n15.mandates.is_empty());
    assert_eq!((sim.n15.rep.bans_expired, sim.n15.rep.mandates_missed), (1, 0));
    assert!(!thought(&sim, 1, "MandateIgnored"));
}

// ------------------------------------------------------------------ rooms and thoughts

#[test]
fn noble_room_thoughts_are_shifted() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1), noble("MANAGER", 2)];
    let mut sim = ready(w, 13);
    // H1: a great hall (tier 5) is rated like tier 3 by a mayor (required 500 = tier 3).
    let s1 = slot(&sim, 1);
    assert!(sim.nobles15_room_feel(s1, "DiningQuality", 5, 100));
    let e = sim.world.unit(1).unwrap().emotions.iter().rev().find(|e| e.thought == "DiningQuality").unwrap().clone();
    assert_eq!(e.severity, 1, "commoners: 3");
    // A modest bedroom (tier 1) becomes -1 -> severity -3.
    assert!(sim.nobles15_room_feel(s1, "BedroomQuality", 1, 200));
    let e = sim.world.unit(1).unwrap().emotions.iter().rev().find(|e| e.thought == "BedroomQuality").unwrap().clone();
    assert_eq!(e.severity, -3);
    // Commoners and "any office" holders keep the commoner rule.
    assert!(!sim.nobles15_room_feel(slot(&sim, 3), "DiningQuality", 5, 300));
    assert!(!sim.nobles15_room_feel(slot(&sim, 2), "OfficeQuality", 2, 300));
}

#[test]
fn bedroom_calibration_adds_the_shift() {
    let nobles = vec![noble("MAYOR", 13937), noble("SHERIFF", 5), noble("BROKER", 6)];
    let sh = bedroom_shifts(&nobles);
    assert_eq!(sh, vec![(13937, 2), (5, 0), (6, 0)]);
    let mut c = vec![noble("COUNT", 9)];
    c.push(noble("MAYOR", 9));
    assert_eq!(bedroom_shifts(&c), vec![(9, 4)]);
}

fn room_world() -> World {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MAYOR", 1), noble("MANAGER", 2), noble("BOOKKEEPER", 3)];
    w.buildings.push(zone(500, "Bedroom", 1, 30, 30, 32, 32));
    w.buildings.push(furniture(501, "Box", Pos::new(31, 31, HZ), 0));
    w.buildings.push(furniture(502, "Cabinet", Pos::new(30, 30, HZ), 0));
    w
}

#[test]
fn requirement_status_per_noble() {
    let sim = ready(room_world(), 14);
    let reqs = sim.noble_requirements();
    let mayor = reqs.iter().find(|r| r.unit == 1).unwrap();
    assert_eq!(mayor.state, crate::nobles15_api::ReqState::Missing, "no office, no dining room");
    let office = mayor.rooms.iter().find(|r| r.kind == "office").unwrap();
    assert_eq!((office.state, office.required, office.required_name), (crate::nobles15_api::ReqState::Missing, 500, "Decent Office"));
    let bed = mayor.rooms.iter().find(|r| r.kind == "bedroom").unwrap();
    assert_eq!(bed.zone, 500);
    let chests = mayor.furniture.iter().find(|f| f.kind == "chests").unwrap();
    assert_eq!((chests.have, chests.need), (1, 2));
    let cabinets = mayor.furniture.iter().find(|f| f.kind == "cabinets").unwrap();
    assert_eq!((cabinets.have, cabinets.need), (1, 1));
    assert!(reqs.iter().any(|r| r.unit == 2), "the manager needs an office");
}

#[test]
fn office_zones_can_be_assigned() {
    let mut w = room_world();
    w.buildings.push(zone(600, "Office", -1, 20, 20, 22, 22));
    w.buildings.push(zone(601, "Pen", -1, 23, 23, 25, 25));
    let mut sim = ready(w, 15);
    assert!(crate::build_api::ZONE_KINDS.contains(&"Office"));
    assert!(!sim.assign_zone(601, 2), "only rooms");
    assert!(!sim.assign_zone(600, 4242), "unknown unit");
    assert!(sim.assign_zone(600, 2));
    let m = sim.noble_requirements().into_iter().find(|r| r.unit == 2).unwrap();
    assert_eq!(m.state, crate::nobles15_api::ReqState::Met, "any office will do");
    assert!(sim.assign_zone(600, -1));
    assert!(sim.nobles15_best_room(2, RK_OFFICE).is_none());
}

#[test]
fn meetings_happen_where_the_noble_has_a_room() {
    let mut sim = ready(room_world(), 16);
    let s1 = slot(&sim, 1);
    sim.nobles15_meeting(s1, 1, 100);
    assert!(thought(&sim, 1, "MeetingInBedroom"), "no office, no dining hall");
    sim.world.buildings.push(zone(610, "DiningHall", -1, 10, 10, 14, 14));
    sim.nobles15_meeting(s1, 1, 200);
    assert!(thought(&sim, 1, "MeetingInDiningRoom"));
    sim.world.buildings.push(zone(611, "Office", 1, 20, 20, 22, 22));
    sim.nobles15_meeting(s1, 1, 300);
    assert!(thought(&sim, 1, "OfficeQuality"));
    assert_eq!(sim.n15.rep.meetings, 3);
}

#[test]
fn tomb_thoughts_on_birthdays() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("BARON", 1)];
    let mut sim = ready(w, 17);
    sim.n15_birthdays(100);
    assert!(!thought(&sim, 1, "TombLack"), "first look only records the age");
    sim.world.units[0].age += 1.0;
    sim.n15_birthdays(200);
    assert!(thought(&sim, 1, "TombLack"));
    sim.world.buildings.push(zone(620, "Tomb", 1, 20, 20, 21, 21));
    sim.world.units[0].age += 1.0;
    sim.n15_birthdays(300);
    assert!(thought(&sim, 1, "TombQuality"));
}

#[test]
fn sleep_pretension_resents_better_commoner_bedrooms() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("BARON", 1)];
    w.buildings.push(zone(630, "Bedroom", 1, 30, 30, 30, 30));
    w.buildings.push(zone(631, "Bedroom", 2, 33, 33, 36, 36));
    for (k, x) in (33..=36).enumerate() {
        w.buildings.push(furniture(640 + k as i32, "Statue", Pos::new(x, 34, HZ), 0));
    }
    let mut sim = ready(w, 18);
    // Make the commoner's room clearly better: valuable statues.
    for k in 0..4 {
        let id = 7100 + k;
        let p = Pos::new(33 + k as i16, 34, HZ);
        sim.world.items.push(Item { id, kind: "STATUE".into(), stack: 1, value: 400, pos: p, holder: -1, container: -1, ..Default::default() });
        sim.item_idx.insert(id, (sim.world.items.len() - 1) as u32);
        sim.imeta.push(sim.imeta[0].clone());
        let b = sim.world.buildings.iter_mut().find(|b| b.id == 640 + k).unwrap();
        b.items.push(id);
    }
    sim.social_force_rebuild();
    sim.social_pass(50);
    assert!(sim.nobles15_best_room(2, RK_BEDROOM).unwrap().1 > sim.nobles15_best_room(1, RK_BEDROOM).unwrap().1);
    sim.n15_pretension(DAY);
    assert!(thought(&sim, 1, "RoomPretension"));
    let n = sim.n15.rep.pretension;
    sim.n15_pretension(2 * DAY);
    assert_eq!(sim.n15.rep.pretension, n, "at most weekly");
}

// ------------------------------------------------------------------ demands

fn demand_sim(seed: u64) -> Sim {
    let mut sim = ready(room_world(), seed);
    sim.n15.demands.push(DemandRt { id: 1, noble: 1, building: "Statue".into(), mat: 0, room: RK_BEDROOM, issued: 0, due: 10 * DAY, forget: 3 * YEAR, met: 0, next_thought: 0, text: "granite statue in bedroom".into() });
    sim
}

#[test]
fn demand_met_by_furniture_in_room() {
    let mut sim = demand_sim(19);
    sim.n15_demands_day(DAY);
    assert_eq!(sim.n15.rep.demands_met, 0);
    sim.world.buildings.push(furniture(700, "Statue", Pos::new(32, 30, HZ), 0));
    sim.n15_demands_day(2 * DAY);
    assert_eq!(sim.n15.rep.demands_met, 1);
    assert_eq!(sim.noble_demands()[0].status, "met");
    sim.n15_demands_day(4 * DAY);
    assert!(sim.n15.demands.is_empty(), "removed a day later");
}

#[test]
fn wrong_material_or_room_does_not_count() {
    let mut sim = demand_sim(20);
    sim.world.buildings.push(furniture(701, "Statue", Pos::new(31, 30, HZ), 3)); // oak
    sim.world.buildings.push(furniture(702, "Statue", Pos::new(5, 5, HZ), 0)); // outside the room
    sim.n15_demands_day(DAY);
    assert_eq!(sim.n15.rep.demands_met, 0);
}

#[test]
fn overdue_demand_angers_weekly() {
    let mut sim = demand_sim(21);
    assert_eq!(sim.noble_demands()[0].status, "running out");
    sim.n15_demands_day(11 * DAY);
    sim.ticks = 11 * DAY;
    assert_eq!(sim.noble_demands()[0].status, "overdue");
    sim.n15_demands_day(12 * DAY);
    sim.n15_demands_day(18 * DAY);
    assert_eq!(sim.n15.rep.demands_angry, 2, "days 11 and 18");
    assert!(thought(&sim, 1, "Demands"));
}

#[test]
fn forgotten_after_three_years() {
    let mut sim = demand_sim(22);
    sim.n15_demands_day(3 * YEAR);
    assert!(sim.n15.demands.is_empty());
    assert_eq!(sim.n15.rep.demands_forgotten, 1);
}

#[test]
fn demand_max_respected() {
    let mut w = room_world();
    w.settings.other.push("DEMAND_PERIOD=1".into());
    let mut sim = ready(w, 23);
    for d in 1..20u64 {
        sim.n15_demands_day(d * 2 * MANDATE_COOLDOWN);
    }
    let open = sim.n15.demands.iter().filter(|d| d.noble == 1 && d.met == 0).count();
    assert_eq!(open, 2, "mayor DEMAND_MAX 2");
    assert!(sim.n15.demands.iter().all(|d| d.room == RK_BEDROOM), "only rooms he owns");
    assert!(sim.n15.demands.iter().all(|d| d.noble == 1), "manager/bookkeeper: DEMAND_MAX 0");
}

// ------------------------------------------------------------------ positions over time

fn big_world(n: usize) -> World {
    let mut w = World::synthetic(48, 48, 20);
    let base = w.units[0].clone();
    for k in 0..n {
        let mut u = base.clone();
        u.id = 200 + k as i32;
        u.name = format!("Clone {k}");
        w.units.push(u);
    }
    w
}

#[test]
fn sheriff_becomes_captain_of_the_guard_at_fifty() {
    let mut w = big_world(45);
    w.nobles = vec![noble("SHERIFF", 2)];
    let mut sim = ready(w, 24);
    sim.n15_sheriff_replaced();
    assert!(sim.world.nobles.iter().any(|n| n.token == "CAPTAIN_OF_THE_GUARD" && n.holder == 2));
    let sh = sim.world.nobles.iter().find(|n| n.token == "SHERIFF").unwrap();
    assert_eq!(sh.holder, -1);
    let mut w = big_world(10);
    w.nobles = vec![noble("SHERIFF", 2)];
    let mut sim = ready(w, 24);
    sim.n15_sheriff_replaced();
    assert!(!sim.world.nobles.iter().any(|n| n.token == "CAPTAIN_OF_THE_GUARD"));
}

#[test]
fn orders_need_manager_office_above_twenty() {
    let mut w = big_world(20);
    w.nobles = vec![noble("MANAGER", 2)];
    let mut sim = ready(w, 25);
    let id = sim.add_work_order(0, 5, "OneTime");
    assert!(sim.nobles15_order_pending(id));
    assert!(!sim.world.work_orders.iter().find(|o| o.id == id).unwrap().validated);
    sim.n15_manager_tick();
    assert!(sim.nobles15_order_pending(id), "no office yet");
    sim.world.buildings.push(zone(800, "Office", 2, 20, 20, 21, 21));
    for _ in 0..3 {
        sim.n15_manager_tick();
    }
    assert!(!sim.nobles15_order_pending(id));
    assert!(sim.world.work_orders.iter().find(|o| o.id == id).unwrap().validated);
    assert_eq!(sim.n15.rep.orders_validated, 1);
}

#[test]
fn small_fort_orders_auto_valid() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("MANAGER", 2)];
    let mut sim = ready(w, 26);
    let id = sim.add_work_order(0, 5, "OneTime");
    assert!(!sim.nobles15_order_pending(id));
    assert!(sim.world.work_orders.iter().find(|o| o.id == id).unwrap().validated);
}

// ------------------------------------------------------------------ bookkeeper, broker

#[test]
fn level1_needs_no_office() {
    let mut sim = ready(room_world(), 27);
    sim.set_bookkeeper_level(1);
    assert_eq!(sim.stock_count("BAR").shown, "?");
    for d in 1..=3 {
        sim.n15_bookkeeper_day(d * DAY);
    }
    assert_eq!(sim.bookkeeper_level(), (1, 1));
    assert_ne!(sim.stock_count("DRINK").shown, "?");
}

#[test]
fn level3_waits_for_office() {
    let mut sim = ready(room_world(), 28);
    sim.set_bookkeeper_level(3);
    for d in 1..=10 {
        sim.n15_bookkeeper_day(d * DAY);
    }
    assert_eq!(sim.bookkeeper_level(), (3, 1), "only level 1 without an office");
    sim.world.buildings.push(zone(810, "Office", 3, 20, 20, 21, 21));
    for d in 11..=20 {
        sim.n15_bookkeeper_day(d * DAY);
    }
    assert_eq!(sim.bookkeeper_level(), (3, 3));
}

#[test]
fn counts_round_by_level() {
    assert_eq!(shown_count(5, 123), "123");
    assert_eq!(shown_count(4, 123), "125");
    assert_eq!(shown_count(3, 123), "120");
    assert_eq!(shown_count(2, 123), "~100");
    assert_eq!(shown_count(1, 123), "100+");
    assert_eq!(shown_count(1, 3), "<10");
    assert_eq!(shown_count(0, 3), "?");
}

#[test]
fn snapshot_is_stale_until_next_update() {
    let mut sim = ready(room_world(), 29);
    for d in 1..=3 {
        sim.n15_bookkeeper_day(d * DAY);
    }
    let before = sim.stock_count("WEAPON").recorded;
    let t = sim.world.idx(sim.world.units[0].pos).unwrap();
    sim.create_item("WEAPON", 0, t, "granite spear".into());
    assert_eq!(sim.stock_count("WEAPON").recorded, before, "records are not live");
    sim.n15_bookkeeper_day(3 * DAY + RECORDS_EVERY);
    assert_eq!(sim.stock_count("WEAPON").recorded, before + 1);
}

#[test]
fn appraisal_needs_a_skilled_broker() {
    let mut w = World::synthetic(48, 48, 20);
    w.nobles = vec![noble("BROKER", 2)];
    let id = w.items[0].id;
    w.items[0].value = 100;
    let mut sim = ready(w, 30);
    assert_eq!(sim.appraised_value(id), None);
    let u = sim.world.units.iter_mut().find(|u| u.id == 2).unwrap();
    u.skills.push(Skill { id: "APPRAISAL".into(), rating: 13, xp: 0 });
    assert_eq!(sim.appraised_value(id), Some(100), "skill 13: exact");
}

// ------------------------------------------------------------------ whole runs

#[test]
fn runs_are_deterministic_and_save_cleanly() {
    let make = || {
        let mut w = room_world();
        w.settings.other.push("MANDATE_PERIOD=20".into());
        w.settings.other.push("DEMAND_PERIOD=5".into());
        let mut sim = Sim::new(w, 31);
        sim.run_ticks(20 * DAY as u32);
        sim
    };
    let (a, b) = (make(), make());
    assert!(a.n15.ready, "the module started");
    assert!(a.n15.rep.mandates_issued > 0, "{:?}", a.n15.rep);
    assert_eq!(a.state_hash(), b.state_hash());
    let parts = a.exact_parts();
    assert!(parts.iter().any(|(k, v)| *k == "nobles15" && !v.is_empty()));
}
