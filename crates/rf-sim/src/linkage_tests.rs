//! [machines14] Tests of `linkage` on synthetic maps (wiki Lever, Pressure plate, Mechanism).

use super::*;
use rf_core::{Item, ItemFlags, Unit, UnitFlags, World};

const DZ: u16 = 20;

fn hall_z() -> i16 {
    ((DZ as i16 * 2 / 3).max(2) - 4).max(0)
}

fn building(id: i32, kind: &str, subtype: &str, x1: i16, y1: i16, x2: i16, y2: i16, z: i16) -> Building {
    Building { id, kind: kind.into(), subtype: subtype.into(), x1, y1, x2, y2, z, center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, z), stage: 1, built: true, owner: -1, ..Default::default() }
}

fn mech_item(w: &mut World, id: i32, pos: Pos, bld: i32) {
    if w.materials.is_empty() {
        w.materials.push(rf_core::MaterialInfo { token: "INORGANIC:GRANITE".into(), name: "granite".into(), mat_type: 0, mat_index: -1, ..Default::default() });
    }
    w.items.push(Item {
        id,
        kind: "TRAPPARTS".into(),
        pos,
        stack: 1,
        container: -1,
        holder: -1,
        building: bld,
        desc: "granite mechanism".into(),
        flags: if bld >= 0 { ItemFlags { in_building: true, ..Default::default() } } else { ItemFlags { on_ground: true, ..Default::default() } },
        ..Default::default()
    });
    if let Some(b) = w.buildings.iter_mut().find(|b| b.id == bld) {
        b.items.push(id);
    }
}

/// The hall split by a wall at x = 25 with one gap at (25, 26).
fn walled_hall() -> (World, Pos) {
    let mut w = World::synthetic(48, 48, DZ);
    let (cx, cy, hz) = (24i16, 24i16, hall_z());
    let wall = w.tiletypes.by_name("StoneWall").unwrap();
    for y in cy - 4..=cy + 4 {
        if y == cy + 2 {
            continue;
        }
        w.tile_mut(Pos::new(cx + 1, y, hz)).unwrap().tt = wall;
    }
    for (k, u) in w.units.iter_mut().enumerate() {
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
        u.pos = Pos::new(27 + (k % 3) as i16, 20 + (k / 3) as i16, hz);
    }
    (w, Pos::new(cx + 1, cy + 2, hz))
}

fn quiet(w: World) -> Sim {
    let mut sim = Sim::new(w, 7);
    sim.set_invasions(false);
    sim.set_moods_enabled(false);
    sim
}

/// A creature that stays where it is put (wild animal, asleep in its AI).
fn creature(sim: &mut Sim, pos: Pos, size: i32, citizen: bool) -> usize {
    let id = sim.world.units.iter().map(|u| u.id).max().unwrap() + 1;
    sim.world.units.push(Unit {
        id,
        race: if citizen { "DWARF".into() } else { "TROLL".into() },
        caste: "MALE".into(),
        pos,
        flags: UnitFlags { animal: !citizen, citizen, ..Default::default() },
        civ_id: if citizen { sim.world.units[0].civ_id } else { 77 },
        hist_fig_id: -1,
        squad_id: -1,
        squad_position: -1,
        body_size: size,
        walk_speed: 900,
        ..Default::default()
    });
    sim.run_ticks(1);
    let s = sim.unit_slot[&id];
    sim.ai[s].wake = u64::MAX / 2;
    s
}

fn step(sim: &mut Sim, s: usize, p: Pos) {
    let t = sim.world.idx(p).unwrap() as u32;
    let now = sim.ticks();
    sim.move_to(s, t, now);
    sim.ai[s].wake = u64::MAX / 2;
}

fn door_lever() -> (Sim, Pos) {
    let (mut w, gap) = walled_hall();
    w.buildings.push(building(800, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
    w.buildings.push(building(801, "Trap", "Lever", 20, 20, 20, 20, gap.z));
    let mut sim = quiet(w);
    sim.link_building_now(801, 800).unwrap();
    (sim, gap)
}

#[test]
fn lever_on_opens_linked_door_at_once() {
    let (mut sim, gap) = door_lever();
    assert!(!sim.is_walkable(gap), "a linked door is shut and locked");
    sim.pull_lever_now(801);
    sim.run_ticks(1);
    assert!(sim.is_walkable(gap), "on opens the door in the same tick");
    assert!(sim.world.building(800).unwrap().flags.iter().all(|f| f != "CLOSED"));
    sim.pull_lever_now(801);
    sim.run_ticks(1);
    assert!(!sim.is_walkable(gap), "off closes it again");
}

#[test]
fn imported_closed_linked_door_opens_on_first_pull() {
    let (mut w, gap) = walled_hall();
    let mut d = building(800, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z);
    d.flags = vec!["CLOSED".into(), "OPERATED_BY_MECHANISMS".into()];
    d.links = vec![801];
    let mut l = building(801, "Trap", "Lever", 20, 20, 20, 20, gap.z);
    l.links = vec![800];
    w.buildings.push(d);
    w.buildings.push(l);
    let mut sim = quiet(w);
    assert!(!sim.is_walkable(gap));
    sim.pull_lever_now(801);
    sim.run_ticks(1);
    assert!(sim.is_walkable(gap), "F1 regression: the first pull opens the door");
}

fn bridge_lever() -> (Sim, Pos, Pos) {
    let (mut w, _) = walled_hall();
    let hz = hall_z();
    let mut br = building(600, "Bridge", "", 26, 21, 28, 23, hz);
    br.direction = "N".into();
    w.buildings.push(br);
    w.buildings.push(building(601, "Trap", "Lever", 20, 20, 20, 20, hz));
    w.buildings.push(building(602, "Trap", "Lever", 20, 21, 20, 21, hz));
    let mut sim = quiet(w);
    sim.link_building_now(601, 600).unwrap();
    (sim, Pos::new(26, 21, hz), Pos::new(27, 23, hz))
}

#[test]
fn bridge_raises_after_100_and_ignores_signals_meanwhile() {
    let (mut sim, edge, _) = bridge_lever();
    sim.pull_lever_now(601); // on
    sim.run_ticks(10);
    sim.pull_lever_now(601); // off, while the bridge is still moving: ignored
    sim.run_ticks(85);
    assert!(sim.is_walkable(edge), "not raised before 100 ticks");
    sim.run_ticks(10);
    assert!(!sim.is_walkable(edge), "raised after 100 ticks");
    sim.run_ticks(200);
    assert!(!sim.is_walkable(edge), "the off signal was ignored: the bridge stays raised");
    assert!(sim.link_stats().ignored >= 1);
    sim.pull_lever_now(601); // on: already raised
    sim.run_ticks(5);
    sim.pull_lever_now(601); // off: lowers after 100
    sim.run_ticks(95);
    assert!(!sim.is_walkable(edge));
    sim.run_ticks(10);
    assert!(sim.is_walkable(edge), "a new off lowers it");
}

#[test]
fn same_tick_on_and_off_toggles() {
    let (mut w, gap) = walled_hall();
    w.buildings.push(building(1200, "Floodgate", "", gap.x, gap.y, gap.x, gap.y, gap.z));
    w.buildings.push(building(1201, "Trap", "Lever", 20, 20, 20, 20, gap.z));
    let mut b = building(1202, "Trap", "Lever", 20, 21, 20, 21, gap.z);
    b.flags = vec!["ACTIVE".into()]; // this one is on: its pull sends off
    w.buildings.push(b);
    let mut sim = quiet(w);
    sim.link_building_now(1201, 1200).unwrap();
    sim.link_building_now(1202, 1200).unwrap();
    assert!(!sim.is_walkable(gap), "floodgates start closed");
    sim.pull_lever_now(1201);
    sim.pull_lever_now(1202);
    sim.run_ticks(102);
    assert!(sim.is_walkable(gap), "on + off in one tick toggles the gate open");
    assert_eq!(sim.link_stats().toggled_same_tick, 1);
}

#[test]
fn spikes_retract_on_and_extend_off_after_40() {
    let (mut w, gap) = walled_hall();
    w.buildings.push(building(1300, "Weapon", "", 22, 22, 22, 22, gap.z));
    w.buildings.push(building(1301, "Trap", "Lever", 20, 20, 20, 20, gap.z));
    let mut sim = quiet(w);
    sim.link_building_now(1301, 1300).unwrap();
    assert_eq!(sim.def_on(1300), Some(false), "retracted");
    sim.pull_lever_now(1301); // on = retract: already retracted
    sim.run_ticks(50);
    assert_eq!(sim.def_on(1300), Some(false));
    sim.pull_lever_now(1301); // off = extend after 40
    sim.run_ticks(38);
    assert_eq!(sim.def_on(1300), Some(false), "not yet");
    sim.run_ticks(3);
    assert_eq!(sim.def_on(1300), Some(true), "extended 40 ticks after off");
    sim.pull_lever_now(1301);
    sim.run_ticks(41);
    assert_eq!(sim.def_on(1300), Some(false), "on retracts them");
}

#[test]
fn gear_toggles_on_every_signal_and_cuts_power() {
    let (mut w, _) = walled_hall();
    let z = hall_z();
    w.buildings.push(building(9001, "GearAssembly", "", 18, 22, 18, 22, z));
    w.buildings.push(building(9002, "AxleHorizontal", "", 19, 22, 21, 22, z));
    w.buildings.push(building(9003, "GearAssembly", "", 22, 22, 22, 22, z));
    let mut p = building(9004, "ScrewPump", "", 22, 23, 22, 24, z);
    p.direction = "FromNorth".into();
    w.buildings.push(p);
    w.buildings.push(building(9005, "Trap", "Lever", 20, 27, 20, 27, z));
    let mut sim = quiet(w);
    assert!(sim.is_walkable(Pos::new(18, 22, z)), "gear assemblies do not block movement");
    sim.link_building_now(9005, 9003).unwrap();
    sim.run_ticks(2);
    assert_eq!(sim.machine_net(9002), sim.machine_net(9004), "engaged: one network");
    sim.pull_lever_now(9005);
    sim.run_ticks(2);
    assert_eq!(sim.device_state(9003).unwrap().open, true, "disengaged");
    assert_ne!(sim.machine_net(9002), sim.machine_net(9004), "a disengaged gear transmits nothing");
    sim.pull_lever_now(9005); // off also toggles
    sim.run_ticks(2);
    assert_eq!(sim.machine_net(9002), sim.machine_net(9004), "engaged again");
}

#[test]
fn track_stop_disabled_by_lever() {
    let (mut w, _) = walled_hall();
    let z = hall_z();
    let mut st = building(1400, "Trap", "TrackStop", 22, 22, 22, 22, z);
    st.friction = 500;
    w.buildings.push(st);
    w.buildings.push(building(1401, "Trap", "Lever", 20, 20, 20, 20, z));
    let mut sim = quiet(w);
    let t = sim.world.idx(Pos::new(22, 22, z)).unwrap() as u32;
    sim.carts_bld_rebuild();
    assert_eq!(sim.carts.blds.iter().find(|b| b.tile == t).map(|b| b.value), Some(500), "exported Trap/TrackStop with its friction (F10)");
    sim.link_building_now(1401, 1400).unwrap();
    sim.pull_lever_now(1401);
    sim.run_ticks(1);
    assert!(sim.carts.blds.iter().all(|b| b.tile != t), "switched off at once");
    sim.pull_lever_now(1401);
    sim.run_ticks(38);
    assert!(sim.carts.blds.iter().all(|b| b.tile != t));
    sim.run_ticks(3);
    assert!(sim.carts.blds.iter().any(|b| b.tile == t), "back on 40 ticks after off");
}

#[test]
fn cage_lever_releases_occupant_and_support_lever_removes_support() {
    let (mut w, _) = walled_hall();
    let z = hall_z();
    w.buildings.push(building(1500, "Cage", "", 22, 22, 22, 22, z));
    w.buildings.push(building(1501, "Support", "", 22, 18, 22, 18, z));
    w.buildings.push(building(1502, "Trap", "Lever", 20, 20, 20, 20, z));
    let mut sim = quiet(w);
    let s = creature(&mut sim, Pos::new(22, 22, z), 60_000, false);
    let ui = sim.ai[s].uidx;
    sim.world.units[ui].flags.caged = true;
    sim.ai[s].kind = Kind::Dead;
    sim.link_building_now(1502, 1500).unwrap();
    sim.link_building_now(1502, 1501).unwrap();
    sim.pull_lever_now(1502);
    sim.run_ticks(2);
    assert!(!sim.world.units[ui].flags.caged, "released");
    assert!(sim.world.building(1500).is_none(), "the cage deconstructs");
    assert!(sim.world.building(1501).is_none(), "the support comes down");
    assert_eq!(sim.link_stats().one_shots, 2);
}

#[test]
fn floodgate_jams_on_item_and_closes_when_cleared() {
    let (mut w, gap) = walled_hall();
    w.buildings.push(building(1200, "Floodgate", "", gap.x, gap.y, gap.x, gap.y, gap.z));
    w.buildings.push(building(1201, "Trap", "Lever", 20, 20, 20, 20, gap.z));
    let mut sim = quiet(w);
    sim.link_building_now(1201, 1200).unwrap();
    sim.pull_lever_now(1201);
    sim.run_ticks(102);
    assert!(sim.is_walkable(gap));
    let t = sim.world.idx(gap).unwrap();
    let junk = sim.create_item("BOULDER", 0, t, "granite boulder".into());
    sim.pull_lever_now(1201);
    sim.run_ticks(102);
    assert!(sim.is_walkable(gap), "an item keeps the gate from closing");
    assert!(sim.device_state(1200).unwrap().jammed);
    sim.destroy_item(junk);
    sim.run_ticks(12);
    assert!(!sim.is_walkable(gap), "it grinds shut once the item is gone");
}

#[test]
fn jammed_door_needs_a_new_signal() {
    let (mut sim, gap) = door_lever();
    sim.pull_lever_now(801);
    sim.run_ticks(1);
    let t = sim.world.idx(gap).unwrap();
    let junk = sim.create_item("BOULDER", 0, t, "granite boulder".into());
    sim.pull_lever_now(801);
    sim.run_ticks(1);
    assert!(sim.is_walkable(gap), "propped open");
    sim.destroy_item(junk);
    sim.run_ticks(30);
    assert!(sim.is_walkable(gap), "a door does not close by itself");
    sim.pull_lever_now(801); // on: stays open (jam cleared, door still open)
    sim.run_ticks(1);
    sim.pull_lever_now(801); // off: now it closes
    sim.run_ticks(1);
    assert!(!sim.is_walkable(gap));
}

#[test]
fn bridge_does_not_move_with_a_huge_creature_on_it() {
    let (mut sim, edge, inner) = bridge_lever();
    let s = creature(&mut sim, inner, 1_300_000, false);
    sim.pull_lever_now(601);
    sim.run_ticks(105);
    assert!(sim.is_walkable(edge), "a huge creature stops the raising");
    assert_eq!(sim.link_stats().bridges_stopped, 1);
    // Raise it without the creature, then lower onto it: the bridge comes apart.
    let away = Pos::new(29, 27, inner.z);
    step(&mut sim, s, away);
    sim.pull_lever_now(601); // off (no change)
    sim.run_ticks(5);
    sim.pull_lever_now(601); // on
    sim.run_ticks(105);
    assert!(!sim.is_walkable(edge), "raised");
    step(&mut sim, s, inner);
    sim.pull_lever_now(601); // off: lower onto the creature
    sim.run_ticks(105);
    assert!(sim.world.building(600).is_none(), "the bridge deconstructs");
    assert!(!sim.world.units[sim.ai[s].uidx].flags.dead);
}

// ---------------------------------------------------------------------- plates

fn plate_door(flags: &[&str], r: Option<PlateInfo>) -> (Sim, Pos, Pos) {
    let (mut w, gap) = walled_hall();
    let plate = Pos::new(20, 26, gap.z);
    w.buildings.push(building(900, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
    let mut p = building(901, "Trap", "PressurePlate", plate.x, plate.y, plate.x, plate.y, plate.z);
    p.flags = flags.iter().map(|s| s.to_string()).collect();
    p.plate = r;
    w.buildings.push(p);
    mech_item(&mut w, 9500, plate, 901);
    let mut sim = quiet(w);
    sim.link_building_now(901, 900).unwrap();
    (sim, gap, plate)
}

#[test]
fn plate_holds_while_occupied_and_closes_99_after() {
    let (mut sim, gap, plate) = plate_door(&[], None);
    let s = creature(&mut sim, plate.offset(-1, 0, 0), 60_000, false);
    step(&mut sim, s, plate);
    sim.run_ticks(1);
    assert!(sim.is_walkable(gap), "plate on: the door opens");
    sim.run_ticks(300);
    assert!(sim.is_walkable(gap), "held down: stays open");
    assert_eq!(sim.link_stats().plates_on, 1, "exactly one on");
    step(&mut sim, s, plate.offset(-1, -1, 0));
    sim.run_ticks(98);
    assert!(sim.is_walkable(gap), "closing after 99");
    sim.run_ticks(3);
    assert!(!sim.is_walkable(gap), "closed 99 ticks after the creature left");
    assert_eq!(sim.link_stats().plates_off, 1);
}

#[test]
fn plate_weight_range() {
    let r = PlateInfo { unit_min: 100_000, unit_max: 2_000_000, ..Default::default() };
    let (mut sim, gap, plate) = plate_door(&["PLATE_UNITS", "PLATE_CITIZENS", "PLATE_RESETS"], Some(r));
    let d = creature(&mut sim, plate.offset(-1, 0, 0), 60_000, false);
    step(&mut sim, d, plate);
    sim.run_ticks(2);
    assert!(!sim.is_walkable(gap), "60 000 is below the range");
    step(&mut sim, d, plate.offset(-1, 0, 0));
    let t = creature(&mut sim, plate.offset(0, -1, 0), 700_000, false);
    step(&mut sim, t, plate);
    sim.run_ticks(2);
    assert!(sim.is_walkable(gap), "a troll is heavy enough");
}

#[test]
fn citizens_flag_and_merchants_and_unconscious() {
    let (mut sim, gap, plate) = plate_door(&["PLATE_UNITS", "PLATE_RESETS"], None);
    let c = creature(&mut sim, plate.offset(-1, 0, 0), 60_000, true);
    step(&mut sim, c, plate);
    sim.run_ticks(2);
    assert!(!sim.is_walkable(gap), "citizens do not count without the citizens setting");
    step(&mut sim, c, plate.offset(-1, 0, 0));
    let m = creature(&mut sim, plate.offset(0, -1, 0), 60_000, false);
    let ui = sim.ai[m].uidx;
    sim.world.units[ui].flags.merchant = true;
    step(&mut sim, m, plate);
    sim.run_ticks(2);
    assert!(!sim.is_walkable(gap), "merchants never count");
    step(&mut sim, m, plate.offset(1, -1, 0));
    step(&mut sim, c, plate);
    let now = sim.ticks();
    sim.combat_sync_units();
    sim.combat.f[c].uncon_until = now + 1000;
    sim.run_ticks(2);
    assert!(sim.is_walkable(gap), "an unconscious citizen always counts");
}

#[test]
fn water_plate_range() {
    let r = PlateInfo { water_min: 2, water_max: 5, ..Default::default() };
    let (mut sim, gap, plate) = plate_door(&["PLATE_WATER", "PLATE_RESETS"], Some(r));
    // Walls around the plate keep the water in place.
    let wall = sim.world.tiletypes.by_name("StoneWall").unwrap();
    for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
        let i = sim.world.idx(plate.offset(dx, dy, 0)).unwrap();
        sim.world.tiles[i].tt = wall;
        sim.after_tile_change(i);
    }
    sim.set_liquid(plate, 1, false);
    sim.run_ticks(2);
    assert!(!sim.is_walkable(gap), "1/7 is below the range");
    sim.set_liquid(plate, 3, false);
    sim.run_ticks(2);
    assert!(sim.is_walkable(gap), "3/7 is inside");
    sim.set_liquid(plate, 7, false);
    sim.run_ticks(102);
    assert!(!sim.is_walkable(gap), "7/7 is above the range: closed after 99");
}

#[test]
fn one_use_plate_deconstructs_and_destroys_mechanisms() {
    let (mut sim, gap, plate) = plate_door(&["PLATE_UNITS"], None);
    let s = creature(&mut sim, plate.offset(-1, 0, 0), 60_000, false);
    step(&mut sim, s, plate);
    sim.run_ticks(2);
    assert!(sim.is_walkable(gap), "fired once");
    assert!(sim.world.building(901).is_none(), "the plate deconstructed");
    let gone = sim.world.item(9500).map(|_| sim.imeta[sim.item_idx[&9500] as usize].dead).unwrap_or(true);
    assert!(gone, "its mechanism is destroyed");
    assert_eq!(sim.link_stats().plates_spent, 1);
}

// ---------------------------------------------------------------------- linking job

#[test]
fn mechanic_links_with_two_mechanisms() {
    let (mut w, gap) = walled_hall();
    w.buildings.push(building(800, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
    w.buildings.push(building(801, "Trap", "Lever", 28, 26, 28, 26, gap.z));
    mech_item(&mut w, 9601, Pos::new(29, 25, gap.z), -1);
    mech_item(&mut w, 9602, Pos::new(29, 27, gap.z), -1);
    w.units[0].labors.push("MECHANIC".into());
    let mut sim = quiet(w);
    assert_eq!(sim.free_mechanisms(), 2);
    sim.link_building(801, 800).unwrap();
    assert!(sim.link_building(801, 800).is_err(), "already queued");
    let signals = sim.link_stats().signals;
    for _ in 0..60 {
        sim.run_ticks(50);
        if sim.link_jobs().is_empty() {
            break;
        }
    }
    assert!(sim.link_jobs().is_empty(), "the link job is done: {:?}", sim.link_jobs());
    assert!(sim.world.building(801).unwrap().links.contains(&800));
    let (a, b) = (sim.world.item(9601).unwrap().building, sim.world.item(9602).unwrap().building);
    assert_eq!((a, b), (800, 801), "first mechanism in the target, second in the trigger");
    assert_eq!(sim.link_stats().signals, signals, "no signal on completion");
}

#[test]
fn link_waits_without_mechanisms_and_locked_doors_cannot_be_linked() {
    let (mut w, gap) = walled_hall();
    w.buildings.push(building(800, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
    w.buildings.push(building(801, "Trap", "Lever", 28, 26, 28, 26, gap.z));
    w.units[0].labors.push("MECHANIC".into());
    let mut sim = quiet(w);
    sim.link_building(801, 800).unwrap();
    sim.run_ticks(1000);
    assert_eq!(sim.link_jobs().len(), 1, "no mechanisms: the job waits");
    assert!(sim.link_jobs()[0].worker.is_none());
    // Mechanisms appear, but the door is locked: still no link.
    let t = sim.world.idx(Pos::new(29, 25, gap.z)).unwrap();
    for _ in 0..2 {
        sim.create_item("TRAPPARTS", 0, t, "mechanism".into());
    }
    sim.set_building_flag(800, "FORBIDDEN", true);
    sim.run_ticks(1500);
    assert_eq!(sim.link_jobs().len(), 1, "a locked door cannot be linked");
    sim.set_building_flag(800, "FORBIDDEN", false);
    for _ in 0..60 {
        sim.run_ticks(50);
        if sim.link_jobs().is_empty() {
            break;
        }
    }
    assert!(sim.link_jobs().is_empty(), "linked once the door is unlocked");
}

#[test]
fn linkage_is_saved_mid_delay_and_deterministic() {
    let run = |par: usize| {
        let (mut sim, edge, _) = bridge_lever();
        sim.set_parallel_threshold(par);
        sim.pull_lever_now(601);
        sim.run_ticks(50);
        let snap = sim.exact_snapshot();
        let mut w = sim.world.clone();
        w.sim_exact = snap;
        let mut b = Sim::new(w, 7);
        assert_eq!(b.restore_mode(), "exact");
        sim.run_ticks(100);
        b.run_ticks(100);
        assert!(!b.is_walkable(edge), "the pending raise survived the save");
        assert_eq!(sim.state_hash(), b.state_hash());
        sim.state_hash()
    };
    assert_eq!(run(1), run(1_000_000));
}

// ---------------------------------------------------------------- [machines14] verifier

#[test]
fn reopened_jammed_floodgate_stays_open_when_cleared() {
    let (mut w, gap) = walled_hall();
    w.buildings.push(building(1200, "Floodgate", "", gap.x, gap.y, gap.x, gap.y, gap.z));
    w.buildings.push(building(1201, "Trap", "Lever", 20, 20, 20, 20, gap.z));
    let mut sim = quiet(w);
    sim.link_building_now(1201, 1200).unwrap();
    sim.pull_lever_now(1201); // on: open
    sim.run_ticks(102);
    let t = sim.world.idx(gap).unwrap();
    let junk = sim.create_item("BOULDER", 0, t, "granite boulder".into());
    sim.pull_lever_now(1201); // off: jams
    sim.run_ticks(102);
    assert!(sim.device_state(1200).unwrap().jammed);
    sim.pull_lever_now(1201); // on again: it should stay open
    sim.run_ticks(5);
    sim.destroy_item(junk);
    sim.run_ticks(150);
    assert!(sim.is_walkable(gap), "the lever is on: the cleared floodgate stays open");
    assert!(!sim.device_state(1200).unwrap().jammed);
}

#[test]
fn link_job_with_a_vanished_end_is_dropped() {
    let (mut w, gap) = walled_hall();
    w.buildings.push(building(800, "Door", "", gap.x, gap.y, gap.x, gap.y, gap.z));
    w.buildings.push(building(801, "Trap", "Lever", 28, 26, 28, 26, gap.z));
    let mut sim = quiet(w);
    sim.link_building(801, 800).unwrap();
    assert_eq!(sim.link_jobs().len(), 1);
    sim.remove_building(801);
    sim.run_ticks(201);
    assert!(sim.link_jobs().is_empty(), "the trigger is gone: the job is dropped");
}

#[test]
fn support_brought_down_by_a_lever_frees_its_tile() {
    let (mut w, _) = walled_hall();
    let z = hall_z();
    let p = Pos::new(22, 21, z);
    w.buildings.push(building(1501, "Support", "", p.x, p.y, p.x, p.y, z));
    w.buildings.push(building(1502, "Trap", "Lever", 20, 20, 20, 20, z));
    let mut sim = quiet(w);
    assert!(!sim.is_walkable(p), "a support blocks its tile");
    sim.link_building_now(1502, 1501).unwrap();
    sim.pull_lever_now(1502);
    sim.run_ticks(2);
    assert!(sim.world.building(1501).is_none());
    assert!(sim.is_walkable(p), "the support is gone: its tile is free again");
}

#[test]
fn imported_open_wall_grate_is_passable_and_closes_on_off() {
    let (mut w, gap) = walled_hall();
    let mut g = building(1600, "GrateWall", "", gap.x, gap.y, gap.x, gap.y, gap.z);
    g.flags = vec!["OPEN".into()];
    g.links = vec![1601];
    let mut l = building(1601, "Trap", "Lever", 20, 20, 20, 20, gap.z);
    l.flags = vec!["ACTIVE".into()];
    l.links = vec![1600];
    w.buildings.push(g);
    w.buildings.push(l);
    let mut sim = quiet(w);
    sim.run_ticks(1);
    assert!(sim.is_walkable(gap), "an open grate is no wall");
    sim.pull_lever_now(1601); // off: closes after 100
    sim.run_ticks(102);
    assert!(!sim.is_walkable(gap), "closed");
    sim.remove_building(1600);
    sim.run_ticks(2);
    assert!(sim.is_walkable(gap), "a removed grate gives its tile back");
}

#[test]
fn closed_floor_bars_carry_walkers_over_a_gap() {
    let (mut w, _) = walled_hall();
    let z = hall_z();
    let p = Pos::new(22, 18, z);
    let open = w.tiletypes.by_name("OpenSpace").unwrap();
    w.tile_mut(p).unwrap().tt = open;
    w.tile_mut(p.offset(0, 0, -1)).unwrap().tt = open; // a two-level drop
    w.buildings.push(building(1700, "BarsFloor", "", p.x, p.y, p.x, p.y, z));
    w.buildings.push(building(1701, "Trap", "Lever", 20, 20, 20, 20, z));
    let mut sim = quiet(w);
    sim.run_ticks(1);
    assert!(sim.is_walkable(p), "closed floor bars are a floor");
    sim.link_building_now(1701, 1700).unwrap();
    sim.pull_lever_now(1701); // on: opens after 100
    sim.run_ticks(102);
    assert!(!sim.is_walkable(p), "open floor bars leave the gap");
}

#[test]
fn plate_closing_and_half_done_link_job_survive_a_save() {
    let (mut sim, _gap, plate) = plate_door(&[], None);
    let hz = plate.z;
    sim.world.buildings.push(building(805, "Door", "", 29, 27, 29, 27, hz));
    sim.world.buildings.push(building(806, "Trap", "Lever", 28, 22, 28, 22, hz));
    let t = sim.world.idx(Pos::new(29, 25, hz)).unwrap();
    for _ in 0..2 {
        sim.create_item("TRAPPARTS", 0, t, "mechanism".into());
    }
    let uidx = sim.ai[0].uidx;
    sim.world.units[uidx].labors.push("MECHANIC".into());
    sim.ai[0].labors |= crate::labor::MECHANIC;
    sim.run_ticks(2);
    sim.link_building(806, 805).unwrap();
    let s = creature(&mut sim, plate.offset(-1, 0, 0), 60_000, false);
    step(&mut sim, s, plate);
    sim.run_ticks(3);
    step(&mut sim, s, plate.offset(-1, -1, 0));
    // Run until the plate is closing and the mechanic is busy with the job.
    let mut ok = false;
    for _ in 0..400 {
        sim.run_ticks(1);
        let busy = sim.link_jobs().first().map(|j| j.worker.is_some()).unwrap_or(false);
        let closing = sim.plate_state(901).map(|p| p.closing_in.is_some()).unwrap_or(false);
        if busy && closing {
            ok = true;
            break;
        }
        if sim.plate_state(901).map(|p| !p.pressed).unwrap_or(true) {
            // The plate closed before the mechanic started: press it again.
            step(&mut sim, s, plate);
            sim.run_ticks(2);
            step(&mut sim, s, plate.offset(-1, -1, 0));
        }
    }
    assert!(ok, "a plate closing while a link job is in progress");
    let snap = sim.exact_snapshot();
    let mut w = sim.world.clone();
    w.sim_exact = snap;
    let mut b = Sim::new(w, 7);
    assert_eq!(b.restore_mode(), "exact");
    sim.run_ticks(500);
    b.run_ticks(500);
    assert_eq!(sim.state_hash(), b.state_hash());
}
