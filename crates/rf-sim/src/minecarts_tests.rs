//! [carts12] Tests of the minecart module (one per mechanic).

use super::*;
use rf_core::{Building, TiletypeInfo, World};

const Z: i16 = 9; // synthetic hall level
const ROW: i16 = 27;

/// The synthetic world with DF track tiletypes and a straight E-W track along the hall's row
/// 27 from x 19 to 30.
fn world() -> World {
    let mut w = World::synthetic(48, 48, 20);
    let mut add = Vec::new();
    for d in ["N", "S", "E", "W", "NS", "EW", "NE", "NW", "SE", "SW", "NSE", "NSW", "NEW", "SEW", "NSEW"] {
        add.push((format!("StoneFloorTrack{d}"), Shape::Floor, d));
        add.push((format!("StoneRampTrack{d}"), Shape::Ramp, d));
    }
    for (n, s, d) in add {
        let id = w.tiletypes.infos.len() as u16;
        w.tiletypes.infos.push(TiletypeInfo { id, name: n, shape: s, material: rf_core::TtMaterial::Stone, special: TtSpecial::Track, variant: 0, direction: d.into() });
    }
    w.tiletypes.reindex();
    for x in 19..=30 {
        let d = if x == 19 {
            "E"
        } else if x == 30 {
            "W"
        } else {
            "EW"
        };
        set(&mut w, x, ROW, Z, &format!("StoneFloorTrack{d}"));
    }
    for u in &mut w.units {
        u.thirst_timer = 0;
        u.hunger_timer = 0;
        u.sleepiness_timer = 0;
        u.labors.push("HANDLE_VEHICLES".into());
    }
    w
}

fn set(w: &mut World, x: i16, y: i16, z: i16, tt: &str) {
    let id = w.tiletypes.by_name(tt).unwrap_or_else(|| panic!("{tt}"));
    let i = w.idx(Pos::new(x, y, z)).unwrap();
    w.tiles[i].tt = id;
}

fn mk(w: World, seed: u64) -> Sim {
    let mut sim = Sim::new(w, seed);
    sim.set_invasions(false);
    sim.set_moods_enabled(false);
    // No digging in the hall (boulders would mix with the test's).
    sim.designate(Pos::new(30, 24, Z), Pos::new(40, 24, Z), crate::DesignationKind::Remove);
    sim.carts_tt_bits();
    sim
}

fn bld(id: i32, kind: &str, sub: &str, x: i16, y: i16, z: i16, dir: &str) -> Building {
    Building { id, kind: kind.into(), subtype: sub.into(), x1: x, y1: y, x2: x, y2: y, z, center: Pos::new(x, y, z), built: true, owner: -1, direction: dir.into(), ..Default::default() }
}

fn boulder(sim: &mut Sim, p: Pos) -> i32 {
    let t = sim.world.idx(p).unwrap();
    sim.create_item("BOULDER", 0, t, "granite boulder".into())
}

/// A cart rolling freely from `p` in `dir` with speed `v` (no route).
fn free_cart(sim: &mut Sim, p: Pos, dir: u8, v: i32) -> usize {
    let id = sim.spawn_vehicle(false, p, 0);
    sim.reserve_vehicle(id, true);
    let tile = sim.world.idx(p).unwrap() as u32;
    sim.carts.carts.push(Cart { item: id, route: -1, state: CartState::Moving, tile, dir, v, ..Default::default() });
    sim.carts.moving += 1;
    sim.carts.carts.len() - 1
}

fn roll(sim: &mut Sim, k: usize, max: u64) -> u64 {
    let mut n = 0;
    while sim.carts.carts[k].state == CartState::Moving && n < max {
        n += 1;
        let now = sim.ticks + n;
        sim.cart_physics(k, now);
    }
    n
}

fn cart_pos(sim: &Sim, k: usize) -> Pos {
    sim.world.pos_of(sim.carts.carts[k].tile as usize)
}

#[test]
fn conditions_parse_like_df_text() {
    let c = parse_conditions(&["Leave when 75% full".into(), "Wait 2 days".into(), "Ride East".into()]);
    assert_eq!((c.mode, c.dir, c.fill, c.empty, c.wait), (DepartMode::Ride, DE, Some(75), false, 2_400));
    let c = parse_conditions(&["Guide".into(), "Leave when empty".into()]);
    assert_eq!((c.mode, c.empty), (DepartMode::Guide, true));
    assert_eq!(parse_conditions(&["Push N".into(), "Leave when full".into()]).fill, Some(100));
    assert_eq!((parse_dir("FromSouth"), parse_dir("West"), parse_dir("E")), (DN, DW, DE));
    assert_eq!((stop_friction("Highest"), stop_friction("medium"), roller_speed("Lowest")), (50_000, 500, 10_000));
    assert_eq!((item_volume("BOULDER", "", 1), item_volume("BAR", "", 1)), (100_000, 6_000));
    assert_eq!(CART_CAPACITY / item_volume("BAR", "", 1), 83, "wiki: 83 bars per cart");
}

#[test]
fn a_push_rolls_along_the_track_and_stops_at_its_end() {
    let mut sim = mk(world(), 1);
    // Friction 10 per tick on track: a 19 990 push would roll ~200 tiles; the track ends at x 30
    // (wall beyond), so the cart stops there.
    let k = free_cart(&mut sim, Pos::new(19, ROW, Z), DE, PUSH_SPEED);
    let n = roll(&mut sim, k, 10_000);
    assert_eq!(cart_pos(&sim, k), Pos::new(30, ROW, Z));
    assert_eq!(sim.carts.carts[k].state, CartState::Stalled);
    // 11 tiles at ~0.2 tiles per tick: about 55 ticks.
    assert!((50..=62).contains(&n), "{n} ticks");
    // Off the track the floor friction (200) stops a 19 990 cart after ~10 tiles.
    let k2 = free_cart(&mut sim, Pos::new(19, 22, Z), DE, PUSH_SPEED);
    roll(&mut sim, k2, 10_000);
    let p = cart_pos(&sim, k2);
    assert!(p.x >= 27 && p.x <= 30, "floor friction: stopped at {p:?}");
}

#[test]
fn track_stop_friction_stops_and_dumps_the_cart() {
    let mut w = world();
    w.buildings.push(bld(9001, "TrackStop", "Highest", 25, ROW, Z, "S"));
    let mut sim = mk(w, 2);
    sim.carts_bld_rebuild();
    let k = free_cart(&mut sim, Pos::new(19, ROW, Z), DE, PUSH_SPEED);
    let cart = sim.carts.carts[k].item;
    let b = boulder(&mut sim, Pos::new(19, ROW, Z));
    sim.haul_remove(b);
    sim.put_in(cart, b);
    roll(&mut sim, k, 1_000);
    assert_eq!(cart_pos(&sim, k), Pos::new(25, ROW, Z), "highest friction stops it on the stop");
    let it = &sim.world.items[sim.item_idx[&b] as usize];
    assert_eq!((it.pos, it.container, it.flags.on_ground), (Pos::new(25, ROW + 1, Z), -1, true), "dumped to the south");
    assert_eq!(sim.carts.stats.dumped, 1);
    // Lowest friction (10) lets it roll on.
    let mut w = world();
    w.buildings.push(bld(9002, "TrackStop", "Lowest", 25, ROW, Z, ""));
    let mut s2 = mk(w, 2);
    s2.carts_bld_rebuild();
    let k = free_cart(&mut s2, Pos::new(19, ROW, Z), DE, PUSH_SPEED);
    roll(&mut s2, k, 1_000);
    assert_eq!(cart_pos(&s2, k), Pos::new(30, ROW, Z));
}

#[test]
fn corners_turn_slow_carts_and_derail_fast_ones() {
    let mut w = world();
    // Corner at (25,27): track from the west turning north up to (25,22).
    set(&mut w, 25, ROW, Z, "StoneFloorTrackNW");
    for y in 22..ROW {
        set(&mut w, 25, y, Z, if y == 22 { "StoneFloorTrackS" } else { "StoneFloorTrackNS" });
    }
    let mut s1 = mk(w.clone(), 3);
    let k = free_cart(&mut s1, Pos::new(19, ROW, Z), DE, PUSH_SPEED);
    roll(&mut s1, k, 5_000);
    let p = cart_pos(&s1, k);
    assert!(p.x == 25 && p.y <= 22, "turned north at the corner (then off the track end): {p:?}");
    assert!(!s1.carts.carts[k].derailed);
    let mut s2 = mk(w, 3);
    let k = free_cart(&mut s2, Pos::new(19, ROW, Z), DE, 60_000);
    roll(&mut s2, k, 5_000);
    assert!(s2.carts.carts[k].derailed, "above 50 000 a corner derails");
    assert_eq!(cart_pos(&s2, k).y, ROW, "kept going east");
    assert!(cart_pos(&s2, k).x > 25);
}

/// Ramp at (27,27,9) going up east: wall at (28,27,9), track on z 10 from x 28 to 31.
fn ramp_world() -> World {
    let mut w = world();
    set(&mut w, 27, ROW, Z, "StoneRampTrackEW");
    set(&mut w, 28, ROW, Z, "StoneWall");
    set(&mut w, 27, ROW, Z + 1, "RampTop");
    for x in 28..=31 {
        set(&mut w, x, ROW, Z + 1, if x == 31 { "StoneFloorTrackW" } else { "StoneFloorTrackEW" });
    }
    w
}

#[test]
fn ramps_need_speed_to_climb() {
    let mut sim = mk(ramp_world(), 4);
    // A push (19 990) loses 4 910 per tick on the ramp (141 420 long): it does not make it.
    let k = free_cart(&mut sim, Pos::new(25, ROW, Z), DE, PUSH_SPEED);
    roll(&mut sim, k, 5_000);
    assert_eq!(cart_pos(&sim, k).z, Z, "a pushed cart does not climb");
    // 50 000 (a highest roller) climbs one ramp.
    let k = free_cart(&mut sim, Pos::new(25, ROW, Z), DE, 50_000);
    roll(&mut sim, k, 5_000);
    assert_eq!(cart_pos(&sim, k).z, Z + 1, "fast cart climbs");
    // Going down the ramp accelerates (-4 890 per tick).
    let k = free_cart(&mut sim, Pos::new(29, ROW, Z + 1), DW, PUSH_SPEED);
    let mut fastest = 0;
    for n in 0..2_000 {
        if sim.carts.carts[k].state != CartState::Moving {
            break;
        }
        sim.cart_physics(k, n);
        fastest = fastest.max(sim.carts.carts[k].v);
    }
    assert!(fastest > PUSH_SPEED + 15_000, "accelerated down the ramp: {fastest}");
    assert_eq!(cart_pos(&sim, k).z, Z);
}

#[test]
fn powered_rollers_give_carts_their_speed() {
    let mut w = world();
    w.buildings.push(bld(9010, "Rollers", "Medium", 21, ROW, Z, "E"));
    let mut sim = mk(w, 5);
    sim.carts_bld_rebuild();
    let k = free_cart(&mut sim, Pos::new(21, ROW, Z), DE, 0);
    // Unpowered: nothing happens (the cart stays).
    sim.cart_physics(k, 1);
    assert_eq!(sim.carts.carts[k].state, CartState::Stalled);
    // Powered: the roller sets 30 000 (minus friction).
    sim.power.machines = vec![crate::power::Machine { bid: 9010, kind: crate::MachineKind::Rollers, tiles: vec![], ports: vec![], net: 0, intake: NONE, output: NONE, under: vec![] }];
    sim.power.nets = vec![crate::PowerNet { produced: 100, used: 2, powered: true }];
    let k = free_cart(&mut sim, Pos::new(21, ROW, Z), DW, 1_000);
    sim.cart_physics(k, 2);
    assert_eq!((sim.carts.carts[k].dir, sim.carts.carts[k].v), (DE, 30_000 - FRICTION_TRACK));
}

/// Route A (19,27) -> B (29,27): track stops at both ends, stone pile P1 beside A with three
/// boulders, stone pile P2 beside B.
fn route_sim(seed: u64, cart_at: Pos) -> (Sim, i32, i32, Vec<i32>) {
    let mut w = world();
    w.buildings.push(bld(9101, "TrackStop", "Highest", 19, ROW, Z, ""));
    w.buildings.push(bld(9102, "TrackStop", "Highest", 29, ROW, Z, ""));
    let mut sim = mk(w, seed);
    let p1 = sim.create_stockpile(Pos::new(18, 20, Z), Pos::new(19, 21, Z), &["stone"]);
    let p2 = sim.create_stockpile(Pos::new(29, 20, Z), Pos::new(30, 21, Z), &["stone"]);
    let stones: Vec<i32> = (0..3).map(|i| boulder(&mut sim, Pos::new(18 + (i % 2), 20 + i / 2, Z))).collect();
    let cart = sim.spawn_vehicle(false, cart_at, 0);
    let r = sim.create_haul_route("Stone");
    let a = sim.add_route_stop(r, "Mine", Pos::new(19, ROW, Z));
    let b = sim.add_route_stop(r, "Smelter", Pos::new(29, ROW, Z));
    sim.link_stop_pile(r, a, p1, true);
    sim.link_stop_pile(r, b, p2, false);
    sim.assign_vehicle(r, cart);
    (sim, r, cart, stones)
}

#[test]
fn a_route_loads_pushes_unloads_and_returns() {
    let (mut sim, r, cart, stones) = route_sim(6, Pos::new(19, ROW, Z));
    let mut seen_moving = false;
    for _ in 0..400 {
        sim.run_ticks(25);
        seen_moving |= sim.carts.carts.iter().any(|c| c.state == CartState::Moving);
        if sim.route_status(r).trips >= 2 {
            break;
        }
    }
    let st = sim.carts_report();
    assert!(seen_moving, "the cart rolled");
    assert_eq!(st.loaded, 3, "{st:?}");
    assert_eq!(st.unloaded, 3, "{st:?}");
    assert!(sim.route_status(r).trips >= 2, "{:?} {:?}", sim.route_status(r), st);
    for id in stones {
        let it = &sim.world.items[sim.item_idx[&id] as usize];
        assert!((29..=30).contains(&it.pos.x) && (20..=21).contains(&it.pos.y), "stone {id} in P2: {:?}", it.pos);
    }
    let (p, _, _, n) = sim.vehicle_info(cart).unwrap();
    assert_eq!((p, n), (Pos::new(19, ROW, Z), 0), "back at the first stop, empty");
}

#[test]
fn a_cart_elsewhere_is_brought_to_the_route_and_guided_carts_follow_the_dwarf() {
    let (mut sim, r, cart, _) = route_sim(7, Pos::new(24, 25, Z));
    sim.set_stop_conditions(r, 0, &["Guide"]);
    sim.set_stop_conditions(r, 1, &["Guide"]);
    for _ in 0..400 {
        sim.run_ticks(25);
        if sim.route_status(r).trips >= 3 {
            break;
        }
    }
    let st = sim.carts_report();
    assert!(sim.route_status(r).trips >= 3, "fetched + 2 guided trips: {st:?} {:?}", sim.route_status(r));
    assert!(st.guided >= 2 && st.pushes == 0, "{st:?}");
    assert!(sim.vehicle_info(cart).is_some());
}

#[test]
fn heavier_carts_are_slower_to_guide() {
    // Oak 28Γ (no slowdown), iron 314Γ: 1.8x the step time.
    assert_eq!(Sim::carry_pct(28), 100);
    assert_eq!(Sim::carry_pct(314), 179);
    assert_eq!(Sim::carry_pct(260), 161, "granite boulder");
    assert_eq!(Sim::carry_pct(5_000), CARRY_MAX_PCT as u16);
    let mut sim = mk(world(), 8);
    sim.set_slow(0, 314);
    assert_eq!(sim.carts_speed(0), sim.ai[0].speed, "only while carrying");
    sim.ai[0].carrying = 1;
    assert_eq!(sim.carts_speed(0), sim.ai[0].speed * 179 / 100);
}

#[test]
fn heavy_items_go_by_wheelbarrow_to_piles_that_have_one() {
    let run = |barrow: bool| -> (u64, u64, Option<Pos>) {
        let mut sim = mk(world(), 9);
        let p = sim.create_stockpile(Pos::new(29, 20, Z), Pos::new(30, 21, Z), &["stone"]);
        if barrow {
            let b = sim.world.buildings.iter_mut().find(|b| b.id == p).unwrap();
            b.stockpile.as_mut().unwrap().max_wheelbarrows = 1;
        }
        let wb = sim.spawn_vehicle(true, Pos::new(30, 21, Z), 0);
        sim.run_ticks(400); // the pile takes its wheelbarrow (every 400 ticks)
        let stone = boulder(&mut sim, Pos::new(19, 26, Z));
        let mut t = 0;
        while t < 6_000 {
            sim.run_ticks(10);
            t += 10;
            let it = &sim.world.items[sim.item_idx[&stone] as usize];
            if it.flags.on_ground && (29..=30).contains(&it.pos.x) && (20..=21).contains(&it.pos.y) {
                break;
            }
        }
        let wpos = sim.item_idx.get(&wb).map(|&i| sim.world.items[i as usize].pos);
        (t, sim.carts_report().barrow_hauls, wpos)
    };
    let (_, hauls, at) = run(true);
    assert_eq!(hauls, 1, "hauled by wheelbarrow");
    let at = at.unwrap();
    assert!((29..=30).contains(&at.x) && (20..=21).contains(&at.y), "the wheelbarrow stays at its pile: {at:?}");
    let (_, hauls, _) = run(false);
    assert_eq!(hauls, 0, "no wheelbarrows: carried by hand (slowed by the weight)");
}

#[test]
fn routes_continue_bit_identically_after_save_and_load() {
    for (n, m) in [(700u32, 2_000u32), (1_900, 1_500), (2_600, 900)] {
        let (mut a, _, _, _) = route_sim(10, Pos::new(24, 25, Z));
        a.run_ticks(n);
        let w = a.world_for_save();
        let raw = rmp_serde::to_vec_named(&w).unwrap();
        let mut w: World = rmp_serde::from_slice(&raw).unwrap();
        w.tiletypes.reindex();
        let mut b = Sim::new(w, 10);
        assert_eq!(b.restore_mode(), "exact");
        assert_eq!(a.state_hash(), b.state_hash());
        a.run_ticks(m);
        b.run_ticks(m);
        assert_eq!(a.state_hash(), b.state_hash(), "N {n} M {m}");
        assert_eq!(a.carts_report().trips, b.carts_report().trips);
    }
}
