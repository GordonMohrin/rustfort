//! Production: brewing, farming, work orders, construction, determinism.

use rf_core::{tile_flags, Building, Item, ItemFlags, MaterialInfo, OrderCondition, PlantRaw, Pos, World, WorkOrder, TICKS_PER_YEAR};
use rf_sim::{AnnouncementKind, Sim};

const DX: u16 = 48;
const DY: u16 = 48;
const DZ: u16 = 20;

fn hall() -> (i16, i16, i16) {
    let surface_z = (DZ as i16 * 2 / 3).max(2);
    (DX as i16 / 2, DY as i16 / 2, (surface_z - 4).max(0))
}

fn item(id: i32, kind: &str, mat: u16, p: Pos, stack: i32) -> Item {
    Item {
        id,
        kind: kind.into(),
        mat,
        pos: p,
        stack,
        container: -1,
        holder: -1,
        building: -1,
        flags: ItemFlags { on_ground: true, ..Default::default() },
        ..Default::default()
    }
}

fn workshop(id: i32, subtype: &str, cx: i16, cy: i16, z: i16) -> Building {
    Building {
        id,
        kind: "Workshop".into(),
        subtype: subtype.into(),
        x1: cx - 1,
        y1: cy - 1,
        x2: cx + 1,
        y2: cy + 1,
        z,
        center: Pos::new(cx, cy, z),
        built: true,
        owner: -1,
        ..Default::default()
    }
}

/// The synthetic fort plus a still, a kitchen, a 4x4 underground plump helmet farm on soil,
/// 20 seeds and 8 empty barrels, a plant raw for the plump helmet and planter/cook labors.
fn production_world() -> World {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    let seed_mat = w.materials.len() as u16;
    w.materials.push(MaterialInfo { token: "PLANT:PLUMP_HELMET:SEED".into(), name: "plump helmet spawn".into(), ..Default::default() });
    w.plant_raws.push(PlantRaw {
        token: "PLUMP_HELMET".into(),
        name: "plump helmet".into(),
        name_plural: "plump helmets".into(),
        growdur: 30_000,
        spring: true,
        summer: true,
        autumn: true,
        winter: true,
        underground: true,
        brewable: true,
        edible_raw: true,
        edible_cooked: true,
        has_seed: true,
        mat_structural: 6,
        mat_drink: 5,
        mat_seed: seed_mat,
        ..Default::default()
    });
    let soil = w.tiletypes.by_name("SoilFloor1").unwrap();
    for y in cy + 1..=cy + 4 {
        for x in cx - 6..=cx - 3 {
            let t = w.tile_mut(Pos::new(x, y, hz)).unwrap();
            t.tt = soil;
            t.mat = 1;
            t.flags |= tile_flags::SUBTERRANEAN;
        }
    }
    w.buildings.push(workshop(500, "Still", cx + 2, cy + 3, hz));
    w.buildings.push(workshop(501, "Kitchen", cx + 5, cy + 3, hz));
    w.buildings.push(Building {
        id: 502,
        kind: "FarmPlot".into(),
        x1: cx - 6,
        y1: cy + 1,
        x2: cx - 3,
        y2: cy + 4,
        z: hz,
        center: Pos::new(cx - 5, cy + 2, hz),
        built: true,
        owner: -1,
        crops: vec!["PLUMP_HELMET".into(); 4],
        ..Default::default()
    });
    let mut id = 5000;
    for k in 0..20 {
        w.items.push(item(id, "SEEDS", seed_mat, Pos::new(cx - 2, cy - 4 + (k % 4) as i16, hz), 1));
        id += 1;
    }
    for k in 0..8 {
        w.items.push(item(id, "BARREL", 3, Pos::new(cx - 6 + k as i16, cy - 4, hz), 1));
        id += 1;
    }
    for u in w.units.iter_mut().filter(|u| u.flags.citizen) {
        u.labors.push("PLANT".into());
        u.labors.push("COOK".into());
        u.labors.push("BREWER".into());
    }
    w
}

fn brew_order(id: i32, amount: i32, freq: &str, conds: Vec<OrderCondition>) -> WorkOrder {
    WorkOrder {
        id,
        job: "CustomReaction".into(),
        reaction: "BREW_DRINK_FROM_PLANT".into(),
        amount_left: amount,
        amount_total: amount,
        frequency: freq.into(),
        workshop_id: -1,
        active: true,
        validated: true,
        conditions: conds,
        ..Default::default()
    }
}

fn drinks_below(n: i32) -> OrderCondition {
    OrderCondition { compare: "LessThan".into(), value: n, item_type: "DRINK".into(), ..Default::default() }
}

fn done(sim: &Sim, label: &str) -> u64 {
    sim.production_report().jobs_done.iter().find(|(l, _)| *l == label).map(|(_, n)| *n).unwrap_or(0)
}

fn units_of(sim: &Sim, kind: &str) -> i32 {
    sim.world.items.iter().filter(|i| i.kind == kind).map(|i| i.stack).sum()
}

#[test]
fn production_fort_survives_two_years() {
    let mut w = production_world();
    w.work_orders.push(brew_order(1, 2, "Daily", vec![drinks_below(40)]));
    // An easy-meal order: plump helmets are brewable, so (Gordon's rule) nothing gets cooked.
    w.work_orders.push(WorkOrder {
        id: 2,
        job: "PrepareMeal".into(),
        item_type: "FOOD".into(),
        mat: "CORAL".into(),
        amount_left: 1,
        amount_total: 1,
        frequency: "Daily".into(),
        workshop_id: -1,
        ..Default::default()
    });
    let mut sim = Sim::new(w, 9);
    assert!(sim.production_notes().iter().all(|n| !n.contains("standing orders")), "real orders: no fallback");
    sim.run_ticks(2 * TICKS_PER_YEAR as u32);
    let deaths: Vec<_> = sim.drain_announcements().into_iter().filter(|a| a.kind == AnnouncementKind::Death).map(|a| a.text).collect();
    assert!(deaths.is_empty(), "deaths: {deaths:?}");
    assert_eq!(sim.world.citizens().count(), 7);
    eprintln!("after 2 years: {:?}", sim.production_report());
    let brews = done(&sim, "Brew drink from plant");
    let harvests = done(&sim, "Harvest plants");
    assert!(brews >= 20, "brewed {brews}");
    assert!(harvests >= 100, "harvested {harvests}");
    assert!(done(&sim, "Plant seeds") >= harvests);
    assert_eq!(done(&sim, "Prepare easy meal"), 0, "brewables are not cooked by default");
    // Drinks end up in barrels.
    for d in sim.world.items.iter().filter(|i| i.kind == "DRINK" && i.mat == 5 && i.container >= 0) {
        let c = sim.world.item(d.container).expect("container exists");
        assert_eq!(c.kind, "BARREL");
        assert_eq!(c.pos, d.pos);
    }
    assert!(units_of(&sim, "DRINK") > 0, "the order keeps drinks in stock");
    for u in sim.world.citizens() {
        assert!(u.thirst_timer < rf_sim::needs::DEHYDRATED, "{} thirst {}", u.name, u.thirst_timer);
        assert!(u.hunger_timer < rf_sim::needs::STARVING, "{} hunger {}", u.name, u.hunger_timer);
    }
}

#[test]
fn cook_brewables_switch_allows_cooking() {
    let mut w = production_world();
    w.work_orders.push(WorkOrder {
        id: 2,
        job: "PrepareMeal".into(),
        item_type: "FOOD".into(),
        mat: "CORAL".into(),
        amount_left: 2,
        amount_total: 2,
        frequency: "OneTime".into(),
        workshop_id: -1,
        ..Default::default()
    });
    for u in &mut w.units {
        u.hunger_timer = 0;
    }
    let mut sim = Sim::new(w, 4);
    sim.set_cook_brewables(true);
    sim.run_ticks(6_000);
    assert_eq!(done(&sim, "Prepare easy meal"), 2);
    let meals: Vec<_> = sim.world.items.iter().filter(|i| i.kind == "FOOD").collect();
    assert!(!meals.is_empty());
    // Two ingredient stacks of 5 plump helmets -> one stack of 10 easy meals each.
    assert!(meals.iter().all(|m| m.stack == 10), "meal stacks: {:?}", meals.iter().map(|m| m.stack).collect::<Vec<_>>());
    assert!(sim.world.work_orders.is_empty(), "finished one-time order is removed");
}

#[test]
fn work_order_condition_gates_brewing() {
    let mut w = production_world();
    // 25 drinks in stock: "only when fewer than 10 drinks" must not brew.
    w.work_orders.push(brew_order(7, 2, "OneTime", vec![drinks_below(10)]));
    let mut sim = Sim::new(w, 3);
    sim.run_ticks(4_000);
    assert_eq!(done(&sim, "Brew drink from plant"), 0);
    assert_eq!(sim.world.work_orders[0].amount_left, 2);
    assert!(sim.world.buildings.iter().all(|b| b.jobs.is_empty()), "no jobs queued while the condition fails");
    sim.world.work_orders[0].conditions[0].value = 1000;
    sim.run_ticks(8_000);
    assert_eq!(done(&sim, "Brew drink from plant"), 2);
    assert!(sim.world.work_orders.is_empty(), "one-time order done and removed");
    let in_barrels: Vec<i32> = sim.world.items.iter().filter(|i| i.kind == "DRINK" && i.container >= 0).map(|i| i.stack).collect();
    // The synthetic plump helmets lie in stacks of 5: DF brews the whole stack, 5 drinks per
    // plant ("Plump helmet [5]" -> "Dwarven wine [25]").
    assert!(!in_barrels.is_empty() && in_barrels.len() <= 2 && in_barrels.iter().all(|&n| (1..=25).contains(&n)), "5 drinks per plant, in barrels: {in_barrels:?}");
}

#[test]
fn brewing_uses_the_whole_plant_stack_and_returns_its_seeds() {
    let mut w = production_world();
    let (cx, cy, hz) = hall();
    // Only one brewable stack (7 plump helmets): the synthetic stacks are removed.
    w.items.retain(|i| i.kind != "PLANT");
    w.items.push(item(9100, "PLANT", 6, Pos::new(cx + 3, cy + 1, hz), 7));
    let seeds_before: i32 = w.items.iter().filter(|i| i.kind == "SEEDS").map(|i| i.stack).sum();
    w.work_orders.push(brew_order(8, 1, "OneTime", vec![]));
    let mut sim = Sim::new(w, 4);
    sim.set_standing_orders(false);
    sim.run_ticks(6_000);
    assert_eq!(done(&sim, "Brew drink from plant"), 1);
    let brewed: Vec<i32> = sim.world.items.iter().filter(|i| i.kind == "DRINK" && i.container >= 0 && i.mat == 5 && i.id >= 9101).map(|i| i.stack).collect();
    // Hungry dwarves may have reserved a few helmets to eat raw; the rest of the stack is
    // brewed in one job (before: 1 plant -> 5 drinks per job).
    assert_eq!(brewed.len(), 1, "one barrel: {brewed:?}");
    let units = brewed[0] / 5;
    assert!(brewed[0] % 5 == 0 && units >= 3, "5 drinks per plant of the stack: {brewed:?}");
    assert!(sim.world.items.iter().filter(|i| i.kind == "PLANT").all(|i| i.id != 9100), "the stack is used up");
    let new_seeds: i32 = sim.world.items.iter().filter(|i| i.kind == "SEEDS" && i.id > 9100).map(|i| i.stack).sum();
    let planted = done(&sim, "Plant seeds") as i32;
    assert!(new_seeds + planted >= units, "1 seed per brewed plant: {new_seeds} new + {planted} planted vs {units} (before {seeds_before})");
}

#[test]
fn repeating_order_resets_each_day() {
    let mut w = production_world();
    w.work_orders.push(brew_order(3, 1, "Daily", vec![]));
    let mut sim = Sim::new(w, 5);
    sim.run_ticks(3 * rf_core::TICKS_PER_DAY as u32 + 50);
    let brews = done(&sim, "Brew drink from plant");
    assert!((3..=4).contains(&brews), "1 per day for 3 days, got {brews}");
    assert_eq!(sim.world.work_orders.len(), 1, "daily orders stay");
}

#[test]
fn workshop_queue_and_repeat_jobs_run() {
    let mut w = production_world();
    w.buildings.iter_mut().find(|b| b.id == 500).unwrap().jobs.push(rf_core::BuildingJob {
        job: "CustomReaction".into(),
        reaction: "BREW_DRINK_FROM_PLANT".into(),
        repeat: true,
        order_id: -1,
        ..Default::default()
    });
    let mut sim = Sim::new(w, 6);
    sim.set_standing_orders(false);
    sim.run_ticks(12_000);
    assert!(done(&sim, "Brew drink from plant") >= 4, "repeat job keeps brewing");
    assert_eq!(sim.world.building(500).unwrap().jobs.len(), 1, "repeat job stays queued");
}

#[test]
fn placed_workshop_gets_constructed_and_removed() {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    w.items.push(item(7000, "BOULDER", 0, Pos::new(cx - 1, cy + 3, hz), 1));
    let mut sim = Sim::new(w, 2);
    let opt = sim.build_options().iter().position(|o| o.subtype == "Still").unwrap();
    let at = Pos::new(cx + 2, cy + 1, hz);
    assert!(sim.can_place(opt, Pos::new(cx + 5, cy + 3, hz), Pos::new(0, 0, 0)).is_err(), "3x3 does not fit at the wall");
    let id = sim.place_building(opt, at, at).expect("placed");
    assert!(!sim.world.building(id).unwrap().built);
    assert!(sim.place_building(opt, at, at).is_err(), "occupied");
    sim.run_ticks(4_000);
    let b = sim.world.building(id).unwrap();
    assert!(b.built, "the mason built the still");
    assert_eq!(b.items.len(), 1, "one building material");
    let bid = b.items[0];
    let boulder = sim.world.item(bid).unwrap();
    assert!(matches!(boulder.kind.as_str(), "BOULDER" | "WOOD"), "a boulder (mason) or a log (carpenter): {}", boulder.kind);
    assert_eq!(b.mat, boulder.mat);
    assert!(boulder.flags.in_building && boulder.building == id);
    assert_eq!(boulder.pos, b.center);
    assert!(sim.production_report().jobs_done.iter().any(|(l, n)| *l == "Construct building" && *n == 1));
    sim.remove_building(id);
    assert!(sim.world.building(id).is_none());
    let boulder = sim.world.item(bid).unwrap();
    assert!(!boulder.flags.in_building && boulder.building < 0, "material is returned");
}

#[test]
fn placed_bed_is_installed_and_used() {
    let (cx, cy, hz) = hall();
    let mut w = World::synthetic(DX, DY, DZ);
    w.items.push(item(7001, "BED", 3, Pos::new(cx - 1, cy + 3, hz), 1));
    for u in w.units.iter_mut() {
        u.labors.push("HAUL_FURNITURE".into());
        u.sleepiness_timer = 0;
    }
    let mut sim = Sim::new(w, 2);
    let opt = sim.build_options().iter().position(|o| o.kind == "Bed").unwrap();
    let at = Pos::new(cx + 4, cy + 4, hz);
    let id = sim.place_building(opt, at, at).unwrap();
    sim.run_ticks(2_000);
    assert!(sim.world.building(id).unwrap().built);
    assert_eq!(sim.world.item(7001).unwrap().pos, at);
}

#[test]
fn farm_plot_needs_soil() {
    let (cx, cy, hz) = hall();
    let sim = Sim::new(World::synthetic(DX, DY, DZ), 1);
    let opt = sim.build_options().iter().position(|o| o.kind == "FarmPlot").unwrap();
    assert!(sim.can_place(opt, Pos::new(cx - 3, cy - 3, hz), Pos::new(cx - 2, cy - 2, hz)).is_err(), "stone floor");
    let surface = Pos::new(5, 5, hz + 4);
    assert!(sim.can_place(opt, surface, surface.offset(3, 2, 0)).is_ok(), "grass/soil surface");
}

#[test]
fn crops_follow_plot_location_and_seasons() {
    let mut w = production_world();
    w.plant_raws.push(PlantRaw { token: "SURFACE_GRAIN".into(), aboveground: true, has_seed: true, summer: true, mat_structural: 6, ..Default::default() });
    let sim = Sim::new(w, 1);
    let opts = sim.crop_options(502);
    assert!(opts.contains(&"PLUMP_HELMET".to_string()));
    assert!(!opts.contains(&"SURFACE_GRAIN".to_string()), "underground plot: no surface crops");
}

#[test]
fn season_change_destroys_crops_that_cannot_grow() {
    let mut w = production_world();
    w.plant_raws[0].winter = false;
    w.plant_raws[0].spring = false;
    w.plant_raws[0].summer = false;
    // Autumn only; start a few days before winter.
    w.calendar.year_tick = 9 * rf_core::TICKS_PER_MONTH - 4 * rf_core::TICKS_PER_DAY;
    let mut sim = Sim::new(w, 1);
    sim.run_ticks(3 * rf_core::TICKS_PER_DAY as u32);
    let planted = sim.farm_tiles(502).iter().filter(|t| !t.1.is_empty()).count();
    assert!(planted > 0, "planted in autumn");
    sim.run_ticks(2 * rf_core::TICKS_PER_DAY as u32);
    assert_eq!(sim.world.calendar.season_name(), "Winter");
    let planted = sim.farm_tiles(502).iter().filter(|t| !t.1.is_empty()).count();
    assert_eq!(planted, 0, "winter destroyed the autumn crop");
}

#[test]
fn fallback_standing_orders_only_without_orders() {
    let sim = Sim::new(production_world(), 1);
    assert!(sim.standing_orders(), "no orders in the world: fallback added");
    assert!(sim.production_notes().iter().any(|n| n.contains("fallback standing orders")));
    let mut w = production_world();
    w.work_orders.push(brew_order(1, 1, "OneTime", vec![]));
    let mut sim = Sim::new(w, 1);
    assert!(!sim.standing_orders());
    sim.set_standing_orders(true);
    assert!(sim.standing_orders());
    sim.set_standing_orders(false);
    assert_eq!(sim.world.work_orders.len(), 1);
}

#[test]
fn labor_toggle_and_templates() {
    let mut sim = Sim::new(production_world(), 1);
    let still = sim.templates_for_workshop("Still");
    assert_eq!(still.len(), 2);
    let tpl = sim.order_templates();
    assert!(tpl.iter().any(|t| t.label == "Make wooden barrel" && t.workshop == "Carpenters"));
    let id = sim.add_work_order(still[0], 3, "Monthly");
    assert!(sim.world.work_orders.iter().any(|o| o.id == id && o.amount_left == 3));
    sim.remove_work_order(id);
    sim.set_labor(1, "BREWER", true);
    assert!(sim.world.unit(1).unwrap().has_labor("BREWER"));
    sim.set_labor(1, "BREWER", false);
    assert!(!sim.world.unit(1).unwrap().has_labor("BREWER"));
    let z = sim.create_zone(Pos::new(20, 20, 9), Pos::new(22, 22, 9), "MeetingHall");
    assert_eq!(sim.world.building(z).unwrap().subtype, "MeetingHall");
}

#[test]
fn production_is_deterministic_across_thread_counts() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let mut w = production_world();
            w.work_orders.push(brew_order(1, 2, "Daily", vec![drinks_below(40)]));
            let mut sim = Sim::new(w, 21);
            sim.set_parallel_threshold(1);
            sim.run_ticks(60_000);
            (sim.state_hash(), sim.production_report().jobs_done.iter().map(|j| j.1).sum::<u64>())
        })
    };
    let a = run(1);
    let b = run(4);
    assert!(a.1 > 20, "jobs done: {}", a.1);
    assert_eq!(a, b);
}
