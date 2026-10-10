//! Social simulation: stress from bad living conditions, relief from good rooms, DF stress
//! buckets, DF-style thought sentences, determinism.

use rf_core::{Building, Emotion, Item, ItemFlags, Pos, World};
use rf_sim::{Sim, StressLevel};

/// The synthetic hall plus plenty of ordinary booze and raw food so nobody dies.
fn world() -> (World, i16, i16, i16) {
    let mut w = World::synthetic(48, 48, 20);
    let (cx, cy, hz) = (24i16, 24i16, (20i16 * 2 / 3) - 4);
    let mut id = 5000;
    for k in 0..40 {
        for (kind, mat, desc) in [("DRINK", 5u16, "dwarven wine"), ("PLANT", 6u16, "plump helmet")] {
            w.items.push(Item {
                id,
                kind: kind.into(),
                mat,
                pos: Pos::new(cx + 2 + (k % 4) as i16, cy - 1 + (k / 20) as i16, hz),
                stack: 3,
                container: -1,
                holder: -1,
                building: -1,
                value: 2,
                desc: desc.into(),
                flags: ItemFlags { on_ground: true, ..Default::default() },
                ..Default::default()
            });
            id += 1;
        }
    }
    // A food stockpile under the supplies: food left outside piles rots (DF rule).
    w.buildings.push(Building {
        id: 4999,
        kind: "Stockpile".into(),
        x1: cx + 2,
        y1: cy - 1,
        x2: cx + 5,
        y2: cy + 1,
        z: hz,
        center: Pos::new(cx + 3, cy, hz),
        built: true,
        owner: -1,
        stockpile: Some(rf_core::StockpileSettings { categories: vec!["food".into()], max_barrels: 0, ..Default::default() }),
        ..Default::default()
    });
    (w, cx, cy, hz)
}

fn add_building(w: &mut World, kind: &str, subtype: &str, a: Pos, b: Pos, owner: i32, item: Option<(&str, i32)>) {
    let bid = w.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
    let mut items = Vec::new();
    if let Some((ik, value)) = item {
        let iid = 90_000 + bid;
        w.items.push(Item {
            id: iid,
            kind: ik.into(),
            mat: 0,
            pos: a,
            stack: 1,
            quality: 4,
            value,
            container: -1,
            holder: -1,
            building: bid,
            desc: ik.to_lowercase(),
            flags: ItemFlags { in_building: true, ..Default::default() },
            ..Default::default()
        });
        items.push(iid);
    }
    w.buildings.push(Building {
        id: bid,
        kind: kind.into(),
        subtype: subtype.into(),
        x1: a.x,
        y1: a.y,
        x2: b.x,
        y2: b.y,
        z: a.z,
        center: Pos::new((a.x + b.x) / 2, (a.y + b.y) / 2, a.z),
        built: true,
        stage: -1,
        owner,
        items,
        mat: 0,
        ..Default::default()
    });
}

/// Dining hall with tables and chairs, an owned great bedroom per dwarf, a meeting hall.
fn furnish(w: &mut World, cx: i16, cy: i16, hz: i16) {
    add_building(w, "Civzone", "DiningHall", Pos::new(cx - 6, cy, hz), Pos::new(cx - 1, cy + 4, hz), -1, None);
    for x in cx - 5..=cx - 1 {
        add_building(w, "Table", "", Pos::new(x, cy + 1, hz), Pos::new(x, cy + 1, hz), -1, Some(("TABLE", 400)));
        add_building(w, "Chair", "", Pos::new(x, cy + 2, hz), Pos::new(x, cy + 2, hz), -1, Some(("CHAIR", 400)));
    }
    let dwarves: Vec<i32> = w.units.iter().filter(|u| u.flags.citizen).map(|u| u.id).collect();
    for (k, uid) in dwarves.iter().enumerate() {
        let p = Pos::new(cx - 6 + 2 * k as i16, cy - 4, hz);
        add_building(w, "Bed", "", p, p, -1, Some(("BED", 1200)));
        add_building(w, "Civzone", "Bedroom", p, p, *uid, None);
    }
    add_building(w, "Civzone", "MeetingHall", Pos::new(cx + 1, cy + 2, hz), Pos::new(cx + 6, cy + 4, hz), -1, None);
}

fn mean_stress(sim: &Sim) -> i32 {
    let v: Vec<i32> = sim.world.citizens().map(|u| u.stress).collect();
    v.iter().sum::<i32>() / v.len().max(1) as i32
}

const MONTHS_4: u32 = 4 * 33_600;

#[test]
fn bad_living_conditions_cause_stress_good_rooms_relieve_it() {
    let (w, cx, cy, hz) = world();
    let mut bad = Sim::new(w.clone(), 7);
    bad.run_ticks(MONTHS_4);
    let mut good_w = w;
    furnish(&mut good_w, cx, cy, hz);
    let mut good = Sim::new(good_w, 7);
    let rep0 = good.social_report();
    assert_eq!(rep0.seats, 5, "five chairs next to tables in the dining hall");
    assert_eq!(rep0.bedrooms, 7);
    good.run_ticks(MONTHS_4);
    let (b, g) = (mean_stress(&bad), mean_stress(&good));
    let rb = bad.social_report();
    let rg = good.social_report();
    let names = |r: &rf_sim::SocialReport| r.thoughts.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();
    println!("bad: mean stress {b}, thoughts {:?}", rb.thoughts);
    println!("good: mean stress {g}, thoughts {:?}", rg.thoughts);
    assert!(sim_alive(&bad) && sim_alive(&good), "nobody dies with food and drink");
    assert!(b > 0, "sleeping on the floor and eating without tables stresses dwarves (mean {b})");
    assert!(g < 0, "good rooms keep dwarves content (mean {g})");
    assert!(b > g + 3000);
    let bn = names(&rb);
    assert!(bn.iter().any(|t| t == "SleptRoughFloor"), "{bn:?}");
    assert!(bn.iter().any(|t| t == "NoDining"), "{bn:?}");
    let gn = names(&rg);
    assert!(gn.iter().any(|t| t == "BedroomQuality"), "{gn:?}");
    assert!(gn.iter().any(|t| t == "DiningQuality"), "{gn:?}");
    assert!(rg.meals_at_table > 0 && rg.meals_without_table < rg.meals_at_table);
    // Beds in owned bedrooms are used by their owners only.
    for u in good.world.citizens() {
        assert!(!good.recent_thoughts(u.id, 64).iter().any(|t| t.contains("without a proper room")), "{}", u.name);
    }
}

fn sim_alive(sim: &Sim) -> bool {
    sim.world.citizens().count() == 7
}

#[test]
fn extreme_stress_leads_to_breakdowns() {
    let (mut w, ..) = world();
    for u in w.units.iter_mut().filter(|u| u.flags.citizen) {
        u.stress = 95_000;
        u.longterm_stress = 90_000;
        u.facets = vec![("ANGER_PROPENSITY".into(), 90), ("STRESS_VULNERABILITY".into(), 80)];
    }
    let mut sim = Sim::new(w, 3);
    sim.run_ticks(2 * 33_600);
    let rep = sim.social_report();
    assert!(rep.tantrums >= 1, "angry, harrowed dwarves throw tantrums: {rep:?}");
    assert_eq!(rep.depressions + rep.oblivious, 0, "anger picks tantrums");
}

#[test]
fn stress_levels_follow_df_cutoffs() {
    use StressLevel::*;
    let cases = [
        (-100_000, Ecstatic),
        (-50_001, Ecstatic),
        (-50_000, Happy),
        (-25_001, Happy),
        (-25_000, Content),
        (-10_001, Content),
        (-10_000, Fine),
        (9_999, Fine),
        (10_000, Stressed),
        (25_000, VeryStressed),
        (49_999, VeryStressed),
        (50_000, Breaking),
    ];
    for (s, l) in cases {
        assert_eq!(StressLevel::from_stress(s), l, "stress {s}");
    }
}

#[test]
fn describes_thoughts_like_df() {
    let (w, ..) = world();
    let sim = Sim::new(w, 1);
    let e = |emotion: &str, thought: &str, sev: i32| Emotion { emotion: emotion.into(), thought: thought.into(), subthought: -1, severity: sev, ..Default::default() };
    assert_eq!(sim.describe_emotion(&e("SATISFACTION", "SatisfiedAtWork", 0)), "felt satisfied at work");
    assert_eq!(sim.describe_emotion(&e("ANNOYANCE", "LackBedroom", 0)), "felt annoyed after sleeping without a proper room");
    assert_eq!(sim.describe_emotion(&e("BLISS", "BedroomQuality", 3)), "felt bliss after sleeping in a great bedroom");
    assert_eq!(sim.describe_emotion(&e("ANNOYANCE", "LackTables", 0)), "felt annoyed at the lack of dining tables");
    assert_eq!(sim.describe_emotion(&e("PLEASURE", "GoodDrink", 2)), "felt pleasure after having a finely-made drink");
    assert_eq!(sim.describe_emotion(&e("ANYTHING", "SawDeadBody", 0)), "felt nothing after seeing somebody's dead body");
}

#[test]
fn social_sim_is_deterministic_across_thread_counts() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let (mut w, cx, cy, hz) = world();
            furnish(&mut w, cx, cy, hz);
            let mut sim = Sim::new(w, 11);
            sim.set_parallel_threshold(1);
            sim.run_ticks(60_000);
            let thoughts: usize = sim.world.citizens().map(|u| u.emotions.len()).sum();
            (sim.state_hash(), mean_stress(&sim), thoughts)
        })
    };
    let a = run(1);
    assert_eq!(a, run(4));
    assert!(a.2 > 0, "thoughts were produced");
}

#[test]
fn room_value_counts_floor_walls_and_engravings() {
    // [audit-eco] wiki "Room"/"Engraving": room value = furniture + floor and bordering wall
    // tiles (origin multiplier x material value) + engravings (10 x material value x quality
    // multiplier).
    let (mut w, cx, cy, hz) = world();
    let uid = w.units.iter().find(|u| u.flags.citizen).unwrap().id;
    let (a, b) = (Pos::new(cx - 6, cy - 6, hz), Pos::new(cx - 4, cy - 4, hz));
    add_building(&mut w, "Bed", "", Pos::new(cx - 5, cy - 5, hz), Pos::new(cx - 5, cy - 5, hz), -1, Some(("BED", 120)));
    add_building(&mut w, "Civzone", "Bedroom", a, b, uid, None);
    let zone = w.buildings.last().unwrap().id;
    let floor_mat = w.tile(Pos::new(cx - 5, cy - 5, hz)).unwrap().mat;
    w.materials[floor_mat as usize].value = 3;
    for y in cy - 6..=cy - 4 {
        for x in cx - 6..=cx - 4 {
            w.tile_mut(Pos::new(x, y, hz)).unwrap().mat = floor_mat;
        }
    }
    let plain = Sim::new(w.clone(), 1).room_value(zone).unwrap();
    let walls = plain - 120 - 9 * 3;
    assert!(walls >= 0 && walls % 3 == 0, "furniture + 9 rough floor tiles (x1) + rough walls (x1): {plain}");
    w.engravings.push(rf_core::Engraving { pos: Pos::new(cx - 6, cy - 6, hz), quality: 0, floor: true, mat: floor_mat, engraver: -1, skill: 0 });
    w.engravings.push(rf_core::Engraving { pos: Pos::new(cx - 4, cy - 4, hz), quality: 5, floor: true, mat: floor_mat, engraver: -1, skill: 0 });
    let engraved = Sim::new(w, 1).room_value(zone).unwrap();
    assert_eq!(engraved - plain, 10 * 3 * (1 + 12), "ordinary (x1) and masterful (x12) engravings");
}
