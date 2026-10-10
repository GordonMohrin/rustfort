//! [carts12] Hauling routes on Gordon's real fort (Trussroads, export r5).
//!
//!   cargo run -p rf-sim --release --example minecarts -- [--export <dir>] [--ticks 40000]
//!
//! Two demo routes, both built on the imported fort before the run:
//! 1. **Erz** (guided): ore boulders from the mining level's stone pile "Steinlager Sued A"
//!    (z 123) up to the smelters' ore pile "W6 Erz" (z 126). A guided cart moves with its
//!    dwarf, so it can change levels (DF: guided carts ignore physics).
//! 2. **Gleis** (pushed): a straight track carved next to "Steinlager Sued A" with a track
//!    stop at each end; the first stop loads from that pile, the cart is pushed (19 990) and
//!    rolls to the other stop, where it is unloaded into a new stone pile, then pushed back.
//!
//! Prints per day: cart states, trips, items loaded/unloaded, ore in "W6 Erz", ticks/s.

use rf_core::{Building, Pos, Shape};
use rf_sim::Sim;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(40_000);
    let Some(dir) = rf_import::find_export(get("--export").map(PathBuf::from).as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let mut sim = Sim::new(world, 1);
    sim.set_invasions(false);
    let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
    for id in ids {
        sim.order_squad(id, rf_sim::SquadOrder::Cancel);
        sim.set_squad_schedule(id, "Off duty");
    }
    let pile = |sim: &Sim, name: &str| sim.world.buildings.iter().find(|b| b.kind == "Stockpile" && b.name == name).cloned();
    let (Some(mine), Some(erz)) = (pile(&sim, "Steinlager Sued A"), pile(&sim, "W6 Erz")) else {
        eprintln!("this demo needs Trussroads' piles \"Steinlager Sued A\" and \"W6 Erz\"");
        std::process::exit(1);
    };
    let oak = sim.world.materials.iter().position(|m| m.is_wood && m.token.contains("OAK")).unwrap_or(0) as u16;
    let ore_in = |sim: &Sim, b: &Building| sim.world.items.iter().filter(|it| it.kind == "BOULDER" && b.contains(it.pos) && sim.world.materials.get(it.mat as usize).map(|m| m.is_ore).unwrap_or(false)).count();
    let stones_in = |sim: &Sim, b: &Building| sim.world.items.iter().filter(|it| it.kind == "BOULDER" && it.flags.on_ground && b.contains(it.pos)).count();
    println!("Steinlager Sued A (z{}): {} boulders, {} of them ore | W6 Erz (z{}): {} ore", mine.z, stones_in(&sim, &mine), ore_in(&sim, &mine), erz.z, ore_in(&sim, &erz));

    // Route 1: guided ore cart from the mining level to the smelters.
    let stop_near = |sim: &Sim, b: &Building| -> Option<Pos> {
        let mut best = None;
        for r in 1..6i16 {
            for y in b.y1 - r..=b.y2 + r {
                for x in b.x1 - r..=b.x2 + r {
                    let p = Pos::new(x, y, b.z);
                    if !b.contains(p) && sim.is_walkable(p) && sim.world.shape(p) == Shape::Floor && !sim.world.buildings.iter().any(|o| o.contains(p)) && sim.reachable(p, b.center) {
                        best = best.or(Some(p));
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        best
    };
    let (Some(a), Some(b)) = (stop_near(&sim, &mine), stop_near(&sim, &erz)) else {
        eprintln!("no free floor next to the piles");
        std::process::exit(1);
    };
    let r1 = sim.create_haul_route("Erz");
    let s0 = sim.add_route_stop(r1, "Mine", a);
    let s1 = sim.add_route_stop(r1, "Schmelze", b);
    sim.link_stop_pile(r1, s0, mine.id, true);
    sim.link_stop_pile(r1, s1, erz.id, false);
    sim.set_stop_conditions(r1, s0, &["Guide", "Leave when full"]);
    sim.set_stop_conditions(r1, s1, &["Guide", "Leave when empty"]);
    let cart1 = sim.spawn_vehicle(false, a, oak);
    sim.assign_vehicle(r1, cart1);
    println!("route Erz: stop 1 {a:?} -> stop 2 {b:?}, oak minecart {cart1}, guided");

    // Route 2: a pushed cart on a carved track on the mining level.
    let track = find_run(&sim, &mine, 8, 24);
    let mut r2 = -1;
    if let Some((y, x0, x1)) = track {
        let z = mine.z;
        let tt = |n: &str| sim.world.tiletypes.by_name(n);
        for x in x0..=x1 {
            let d = if x == x0 { "E" } else if x == x1 { "W" } else { "EW" };
            if let (Some(id), Some(i)) = (tt(&format!("StoneFloorTrack{d}")), sim.world.idx(Pos::new(x, y, z))) {
                sim.world.tiles[i].tt = id;
            }
        }
        sim.notify_tiles_changed(Pos::new(x0, y, z), Pos::new(x1, y, z));
        let next = sim.world.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        for (k, x) in [x0, x1].into_iter().enumerate() {
            sim.world.buildings.push(Building {
                id: next + k as i32,
                kind: "TrackStop".into(),
                subtype: "Highest".into(),
                x1: x,
                y1: y,
                x2: x,
                y2: y,
                z,
                center: Pos::new(x, y, z),
                built: true,
                owner: -1,
                ..Default::default()
            });
        }
        // Unload pile beside the far end.
        let side = [y - 1, y + 1].into_iter().map(|yy| Pos::new(x1, yy, z)).find(|p| sim.is_walkable(*p) && !sim.world.buildings.iter().any(|o| o.contains(*p)));
        if let Some(sp) = side {
            let dump = sim.create_stockpile(Pos::new(sp.x - 5, sp.y, z), sp, &["stone"]);
            r2 = sim.create_haul_route("Gleis");
            let t0 = sim.add_route_stop(r2, "West", Pos::new(x0, y, z));
            let t1 = sim.add_route_stop(r2, "Ost", Pos::new(x1, y, z));
            sim.link_stop_pile(r2, t0, mine.id, true);
            sim.link_stop_pile(r2, t1, dump, false);
            sim.set_stop_conditions(r2, t0, &["Push", "Leave when 40% full"]);
            sim.set_stop_conditions(r2, t1, &["Push", "Leave when empty"]);
            let cart2 = sim.spawn_vehicle(false, Pos::new(x0, y, z), oak);
            sim.assign_vehicle(r2, cart2);
            println!("route Gleis: track y {y} x {x0}..{x1} z {z} ({} tiles), unload pile x {}..{} y {}, cart {cart2}, pushed", x1 - x0 + 1, sp.x - 5, sp.x, sp.y);
        }
    } else {
        println!("route Gleis: no straight free floor run found next to the pile");
    }

    let t0 = Instant::now();
    let mut done = 0;
    while done < ticks {
        let step = 1_200.min(ticks - done);
        sim.run_ticks(step as u32);
        done += step;
        let st = sim.carts_report();
        let s1 = sim.route_status(r1);
        let s2 = if r2 >= 0 { sim.route_status(r2) } else { Default::default() };
        println!(
            "day {:>3} | Erz trips {:>3} loaded {:?} [{}] | Gleis trips {:>3} loaded {:?} [{}] | loaded {} unloaded {} pushes {} guided {} tiles {} | W6 Erz ore {}",
            sim.ticks() / 1_200,
            s1.trips,
            s1.loaded,
            s1.vehicles.first().map(|v| v.1.as_str()).unwrap_or("-"),
            s2.trips,
            s2.loaded,
            s2.vehicles.first().map(|v| v.1.as_str()).unwrap_or("-"),
            st.loaded,
            st.unloaded,
            st.pushes,
            st.guided,
            st.tiles,
            ore_in(&sim, &erz),
        );
    }
    let secs = t0.elapsed().as_secs_f64();
    for r in [r1, r2] {
        if r < 0 {
            continue;
        }
        for (cart, _) in sim.route_status(r).vehicles {
            let inside: Vec<String> = sim.world.items.iter().filter(|it| it.container == cart).map(|it| it.desc.clone()).collect();
            println!("route {r} cart {cart}: {} items inside {:?}", inside.len(), inside);
        }
    }
    println!("{ticks} ticks in {secs:.1}s = {:.0} ticks/s | carts {:?}", ticks as f64 / secs, sim.carts_report());
}

/// The longest straight E-W run of free walkable floor (no buildings, not in the pile) within
/// 4 tiles of the pile's edge, `min..=max` tiles long: (y, x0, x1).
fn find_run(sim: &Sim, b: &Building, min: i16, max: i16) -> Option<(i16, i16, i16)> {
    let free = |p: Pos| sim.is_walkable(p) && sim.world.shape(p) == Shape::Floor && !sim.world.buildings.iter().any(|o| o.contains(p)) && sim.reachable(p, b.center);
    let mut best: Option<(i16, i16, i16)> = None;
    for y in (b.y1 - 4..=b.y2 + 4).filter(|&y| y < b.y1 || y > b.y2) {
        let mut x = b.x1 - 4;
        while x <= b.x2 + 4 {
            if !free(Pos::new(x, y, b.z)) {
                x += 1;
                continue;
            }
            let start = x;
            while x <= b.x2 + 4 && free(Pos::new(x, y, b.z)) && x - start < max {
                x += 1;
            }
            let len = x - start;
            if len >= min && best.map(|(_, a, e)| len > e - a + 1).unwrap_or(true) {
                best = Some((y, start, x - 1));
            }
        }
    }
    best
}
