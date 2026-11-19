//! [decay15] Tests on synthetic maps: rot, corpse stages, miasma, refuse, vermin, hunters, wear.

use super::*;
use crate::locations::tests::{slot, world_with_zone};
use rf_core::{Building, CasteRaw, CreatureRaw, Item, ItemFlags, MaterialInfo, Pos, Unit, UnitFlags, World};

fn hz() -> i16 {
    (20i16 * 2 / 3) - 4
}

fn sz() -> i16 {
    20i16 * 2 / 3
}

fn item(id: i32, kind: &str, mat: u16, pos: Pos, desc: &str) -> Item {
    Item {
        id,
        kind: kind.into(),
        mat,
        pos,
        stack: 1,
        container: -1,
        holder: -1,
        building: -1,
        desc: desc.into(),
        value: 100,
        // Forbidden so that no dwarf eats or hauls it during the test.
        flags: ItemFlags { on_ground: true, forbid: true, ..Default::default() },
        ..Default::default()
    }
}

fn add_mat(w: &mut World, token: &str, name: &str, f: impl FnOnce(&mut MaterialInfo)) -> u16 {
    let mut m = MaterialInfo { token: token.into(), name: name.into(), mat_type: -1, mat_index: -1, ..Default::default() };
    f(&mut m);
    w.materials.push(m);
    (w.materials.len() - 1) as u16
}

struct Mats {
    cow_muscle: u16,
    cow_skin: u16,
    crawler_tooth: u16,
    bat_skin: u16,
    olm_skin: u16,
    turtle_muscle: u16,
    iron: u16,
}

fn mats(w: &mut World) -> Mats {
    let cow_muscle = add_mat(w, "CREATURE:COW:MUSCLE", "cow muscle", |_| {});
    let cow_skin = add_mat(w, "CREATURE:COW:SKIN", "cow skin", |_| {});
    add_mat(w, "CREATURE:COW:BONE", "cow bone", |_| {});
    let crawler_tooth = add_mat(w, "CREATURE:CAVE_CRAWLER:TOOTH", "tooth", |_| {});
    let bat_skin = add_mat(w, "CREATURE:BAT:SKIN", "bat skin", |_| {});
    add_mat(w, "CREATURE:BAT:HAIR", "bat hair", |_| {});
    let olm_skin = add_mat(w, "CREATURE:OLM:SKIN", "olm skin", |_| {});
    let turtle_muscle = add_mat(w, "CREATURE:POND_TURTLE:MUSCLE", "turtle", |_| {});
    add_mat(w, "CREATURE:POND_TURTLE:SHELL", "turtle shell", |_| {});
    let iron = add_mat(w, "INORGANIC:IRON", "iron", |m| m.is_metal = true);
    Mats { cow_muscle, cow_skin, crawler_tooth, bat_skin, olm_skin, turtle_muscle, iron }
}

fn world() -> (World, Mats) {
    let mut w = world_with_zone("Halle");
    let m = mats(&mut w);
    for u in &mut w.units {
        u.labors.clear();
    }
    (w, m)
}

/// Run `n` decay days directly (no dwarves moving in between).
fn days(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        let now = (sim.ticks / DAY + 1) * DAY + DECAY_DAY_PHASE;
        sim.ticks = now;
        sim.decay_day(now);
    }
}

fn sim_of(w: World, seed: u64) -> Sim {
    let mut sim = Sim::new(w, seed);
    sim.run_ticks(1);
    sim.decay_scan();
    sim
}

fn alive(sim: &Sim, id: i32) -> bool {
    sim.item_idx.get(&id).map(|&i| !sim.imeta[i as usize].dead).unwrap_or(false)
}

fn it(sim: &Sim, id: i32) -> &Item {
    &sim.world.items[sim.item_idx[&id] as usize]
}

fn tile(sim: &Sim, p: Pos) -> u32 {
    sim.world.idx(p).unwrap() as u32
}

fn miasma(sim: &Sim, p: Pos) -> bool {
    sim.decay.cloud.contains_key(&tile(sim, p))
}

// ------------------------------------------------------------------ food rot (M1)

#[test]
fn meal_on_floor_rots_after_12_days_and_becomes_refuse() {
    let (mut w, _) = world();
    w.items.push(item(9000, "FOOD", 6, Pos::new(22, 22, hz()), "plump helmet roast"));
    let mut sim = sim_of(w, 1);
    let limit = limits(C_FOOD, 9000).0 as u32;
    assert!((12..=15).contains(&limit));
    days(&mut sim, limit - 1);
    assert!(!it(&sim, 9000).flags.rotten);
    days(&mut sim, 1);
    let i = it(&sim, 9000);
    assert!(i.flags.rotten);
    assert!(i.desc.starts_with("rotten "), "{}", i.desc);
    assert_eq!(sim.imeta[sim.item_idx[&9000] as usize].cat, Cat::Refuse);
    assert!(!sim.foods.contains(&9000));
    // Rots away 28 days later.
    days(&mut sim, ROT_AWAY_DAYS as u32);
    assert!(!alive(&sim, 9000));
    assert!(sim.decay_report().rotted_away >= 1);
}

fn barrel_with_meal(w: &mut World, pos: Pos, barrel_mat: u16) {
    let mut b = item(9100, "BARREL", barrel_mat, pos, "barrel");
    b.flags.forbid = true;
    w.items.push(b);
    let mut m = item(9101, "MEAT", 6, pos, "meat");
    m.container = 9100;
    m.flags = ItemFlags { in_container: true, forbid: true, ..Default::default() };
    w.items.push(m);
}

#[test]
fn meal_in_barrel_outside_pile_rots() {
    let (mut w, _) = world();
    barrel_with_meal(&mut w, Pos::new(22, 22, hz()), 3);
    let mut sim = sim_of(w, 2);
    days(&mut sim, 16);
    assert!(it(&sim, 9101).flags.rotten, "a barrel alone does not protect (w:Miasma)");
}

#[test]
fn meal_in_barrel_on_pile_keeps() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    barrel_with_meal(&mut w, p, 3);
    let mut sim = sim_of(w, 3);
    sim.create_stockpile(p, p, &["food"]);
    days(&mut sim, 40);
    assert!(!it(&sim, 9101).flags.rotten);
}

#[test]
fn raw_fish_rots_even_on_pile() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9200, "FISH_RAW", m.cow_muscle, p, "raw fish"));
    let mut sim = sim_of(w, 4);
    sim.create_stockpile(p, p, &["food"]);
    days(&mut sim, 12);
    assert!(it(&sim, 9200).flags.rotten);
}

#[test]
fn food_at_trade_depot_does_not_rot() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    w.buildings.push(Building { id: 950, kind: "TradeDepot".into(), x1: 21, y1: 21, x2: 23, y2: 23, z: hz(), center: p, built: true, owner: -1, ..Default::default() });
    w.items.push(item(9300, "MEAT", 6, p, "meat"));
    let mut sim = sim_of(w, 5);
    days(&mut sim, 30);
    assert!(!it(&sim, 9300).flags.rotten);
}

#[test]
fn food_in_minecart_does_not_rot() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    let mut cart = item(9310, "TOOL", m.iron, p, "minecart");
    cart.subtype = "ITEM_TOOL_MINECART".into();
    w.items.push(cart);
    let mut f = item(9311, "MEAT", 6, p, "meat");
    f.container = 9310;
    f.flags = ItemFlags { in_container: true, forbid: true, ..Default::default() };
    w.items.push(f);
    let mut sim = sim_of(w, 6);
    days(&mut sim, 30);
    assert!(!it(&sim, 9311).flags.rotten);
}

#[test]
fn seeds_drinks_eggs_never_rot() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    for (k, kind) in ["SEEDS", "DRINK", "EGG", "GLOB", "LIQUID_MISC"].iter().enumerate() {
        w.items.push(item(9400 + k as i32, kind, 6, p, kind));
    }
    let mut sim = sim_of(w, 7);
    days(&mut sim, 60);
    for k in 0..5 {
        assert!(!it(&sim, 9400 + k).flags.rotten);
    }
    assert!(sim.decay.rot.is_empty());
}

#[test]
fn withered_plant_makes_no_miasma() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9500, "PLANT", 6, p, "plump helmet"));
    let mut sim = sim_of(w, 8);
    days(&mut sim, 16);
    let i = it(&sim, 9500);
    assert!(i.flags.rotten && i.desc.starts_with("withered "), "{}", i.desc);
    assert!(sim.decay.miasma_src.is_empty());
    assert!(!miasma(&sim, p));
}

#[test]
fn old_store_rot_migrates() {
    let (mut w, _) = world();
    w.items.push(item(9600, "MEAT", 6, Pos::new(22, 22, hz()), "meat"));
    let mut sim = Sim::new(w, 9);
    sim.store.rot.insert(9600, 10);
    sim.run_ticks(1);
    days(&mut sim, 1);
    assert!(sim.store.rot.is_empty());
    assert!(sim.decay.rot[&9600].days >= 10);
}

// ------------------------------------------------------------------ corpses (M2)

#[test]
fn corpse_rots_at_14_and_is_skeleton_at_70() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9700, "CORPSE", m.cow_skin, p, "cow corpse"));
    let mut sim = sim_of(w, 10);
    // Imported corpses get a spread age; reset it to test the timer from zero.
    sim.decay.rot.get_mut(&9700).map(|s| s.days = 0);
    days(&mut sim, 13);
    assert_eq!(sim.corpse_stage(9700), CorpseStage::Fresh);
    days(&mut sim, 1);
    assert_eq!(sim.corpse_stage(9700), CorpseStage::Rotten);
    assert_eq!(it(&sim, 9700).desc, "rotten cow corpse");
    assert!(sim.decay.miasma_src.contains(&tile(&sim, p)), "rotting flesh underground makes miasma");
    days(&mut sim, 56);
    assert_eq!(sim.corpse_stage(9700), CorpseStage::Skeleton);
    let c = it(&sim, 9700);
    assert_eq!(c.desc, "cow skeleton");
    assert!(!c.flags.rotten);
    assert!(sim.decay.rot.get(&9700).is_none());
}

#[test]
fn teeth_and_chitin_corpses_never_rot() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9710, "CORPSE", m.crawler_tooth, p, "voracious cave crawler tooth"));
    w.items.push(item(9711, "CORPSE", m.cow_skin, p, "giant cave spider mangled chitin"));
    let mut sim = sim_of(w, 11);
    days(&mut sim, 100);
    assert!(!it(&sim, 9710).flags.rotten && !it(&sim, 9711).flags.rotten);
    assert_eq!(sim.corpse_stage(9710), CorpseStage::NotRotting);
}

#[test]
fn corpse_on_refuse_pile_still_rots() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9720, "CORPSE", m.cow_skin, p, "cow corpse"));
    let mut sim = sim_of(w, 12);
    sim.create_stockpile(p, p, &["refuse"]);
    days(&mut sim, 15);
    assert!(it(&sim, 9720).flags.rotten);
}

#[test]
fn severed_arm_rots_to_two_bones() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9730, "CORPSEPIECE", m.cow_muscle, p, "cow left arm"));
    let mut sim = sim_of(w, 13);
    sim.decay.rot.get_mut(&9730).map(|s| s.days = 0);
    days(&mut sim, 14 + 28);
    assert!(!alive(&sim, 9730));
    let b = sim.world.items.iter().enumerate().find(|(i, x)| x.kind == "CORPSEPIECE" && x.desc == "cow bones" && !sim.imeta[*i].dead).map(|(i, x)| (i, x.stack));
    let (bi, n) = b.expect("bones left behind");
    assert_eq!(n, 2);
    assert_eq!(sim.imeta[bi].cat, Cat::Refuse);
    assert_eq!(sim.decay_report().parts_to_bones, 1);
}

#[test]
fn severed_head_rots_to_skull() {
    let (mut w, m) = world();
    w.items.push(item(9740, "CORPSEPIECE", m.cow_muscle, Pos::new(22, 22, hz()), "cow head"));
    let mut sim = sim_of(w, 14);
    days(&mut sim, 42);
    assert!(sim.world.items.iter().any(|x| x.desc == "cow skull"));
}

#[test]
fn raw_hide_rots_cannot_be_tanned_and_vanishes_without_miasma() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9750, "CORPSEPIECE", m.cow_skin, p, "cow skin"));
    let mut sim = sim_of(w, 15);
    sim.decay.rot.get_mut(&9750).map(|s| s.days = 0);
    days(&mut sim, 14);
    assert!(it(&sim, 9750).flags.rotten);
    assert_eq!(sim.imeta[sim.item_idx[&9750] as usize].cat, Cat::Refuse);
    assert!(sim.decay.miasma_src.is_empty(), "skin rots without miasma");
    days(&mut sim, 28);
    assert!(!alive(&sim, 9750));
}

#[test]
fn turtle_raw_fish_rots_to_shell() {
    let (mut w, m) = world();
    w.items.push(item(9760, "FISH_RAW", m.turtle_muscle, Pos::new(22, 22, hz()), "raw pond turtle"));
    let mut sim = sim_of(w, 16);
    days(&mut sim, 12 + 28);
    assert!(!alive(&sim, 9760));
    assert!(sim.world.items.iter().any(|x| x.desc == "pond turtle shell"));
}

#[test]
fn bat_remains_vanish_olm_remains_stay() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9770, "REMAINS", m.bat_skin, p, "bat remains"));
    w.items.push(item(9771, "REMAINS", m.olm_skin, p, "olm remains"));
    let mut sim = sim_of(w, 17);
    days(&mut sim, 60);
    assert!(!alive(&sim, 9770));
    assert!(alive(&sim, 9771) && !it(&sim, 9771).flags.rotten);
}

#[test]
fn reserved_rotten_item_is_not_destroyed() {
    let (mut w, _) = world();
    w.items.push(item(9780, "MEAT", 6, Pos::new(22, 22, hz()), "meat"));
    let mut sim = sim_of(w, 18);
    days(&mut sim, 16);
    assert!(it(&sim, 9780).flags.rotten);
    let (a, b) = limits(C_FOOD, 9780);
    sim.decay.rot.get_mut(&9780).unwrap().days = a + b;
    let i = sim.item_idx[&9780] as usize;
    sim.imeta[i].reserved = 1;
    days(&mut sim, 3);
    assert!(alive(&sim, 9780), "a job holds it");
    sim.imeta[i].reserved = 0;
    days(&mut sim, 1);
    assert!(!alive(&sim, 9780));
}

// ------------------------------------------------------------------ miasma (M3)

fn rotten_meat(w: &mut World, id: i32, p: Pos) {
    let mut c = item(id, "MEAT", 6, p, "rotten meat");
    c.flags.rotten = true;
    w.items.push(c);
}

#[test]
fn rotten_meat_on_surface_makes_no_miasma() {
    let (mut w, _) = world();
    let p = Pos::new(10, 10, sz());
    rotten_meat(&mut w, 9800, p);
    let mut sim = sim_of(w, 20);
    days(&mut sim, 2);
    assert!(sim.decay.miasma_src.is_empty());
}

#[test]
fn miasma_does_not_pass_closed_door() {
    let (mut w, _) = world();
    let src = Pos::new(20, 22, hz());
    rotten_meat(&mut w, 9810, src);
    let mut sim = sim_of(w.clone(), 21);
    days(&mut sim, 1);
    assert!(miasma(&sim, Pos::new(22, 22, hz())), "2 tiles without a door");
    // Same with a door at (21, 22).
    w.buildings.push(Building { id: 951, kind: "Door".into(), x1: 21, y1: 22, x2: 21, y2: 22, z: hz(), center: Pos::new(21, 22, hz()), built: true, owner: -1, ..Default::default() });
    let mut sim = sim_of(w, 21);
    for a in &mut sim.ai {
        a.pos = 0;
    }
    days(&mut sim, 1);
    assert!(miasma(&sim, src));
    assert!(!miasma(&sim, Pos::new(21, 22, hz())), "the closed door blocks");
    assert!(!miasma(&sim, Pos::new(22, 22, hz())));
}

#[test]
fn miasma_passes_open_door_with_dwarf_in_it() {
    let (mut w, _) = world();
    rotten_meat(&mut w, 9820, Pos::new(20, 22, hz()));
    let door = Pos::new(21, 22, hz());
    w.buildings.push(Building { id: 952, kind: "Door".into(), x1: 21, y1: 22, x2: 21, y2: 22, z: hz(), center: door, built: true, owner: -1, ..Default::default() });
    let mut sim = sim_of(w, 22);
    let t = tile(&sim, door);
    for a in &mut sim.ai {
        a.pos = 0;
    }
    let s = slot(&sim, 1);
    sim.ai[s].pos = t;
    days(&mut sim, 1);
    assert!(miasma(&sim, door));
    assert!(miasma(&sim, Pos::new(22, 22, hz())));
}

#[test]
fn miasma_spreads_up_stairs() {
    let (mut w, _) = world();
    let (cx, cy) = (24, 24);
    rotten_meat(&mut w, 9830, Pos::new(cx, cy, hz()));
    let mut sim = sim_of(w, 23);
    days(&mut sim, 1);
    assert!(miasma(&sim, Pos::new(cx, cy, hz() + 1)), "up the stair shaft");
}

#[test]
fn miasma_lingers_two_days_after_source_removed() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    rotten_meat(&mut w, 9840, p);
    let mut sim = sim_of(w, 24);
    days(&mut sim, 1);
    assert!(miasma(&sim, p));
    sim.destroy_item(9840);
    days(&mut sim, 1);
    assert!(miasma(&sim, p));
    days(&mut sim, 1);
    assert!(miasma(&sim, p));
    days(&mut sim, 1);
    assert!(!miasma(&sim, p));
}

#[test]
fn lifestyle_takes_the_decay_miasma_set() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    rotten_meat(&mut w, 9850, p);
    let mut sim = Sim::new(w, 25);
    sim.run_ticks(2 * 1200);
    assert!(sim.miasma_at(p));
    assert!(sim.lifestyle_report().miasma_sources_max >= 1);
}

// ------------------------------------------------------------------ refuse (M4)

#[test]
fn animal_corpse_and_bones_are_refuse_dwarf_corpse_is_not() {
    let (mut w, m) = world();
    let p = Pos::new(22, 22, hz());
    w.items.push(item(9900, "CORPSE", m.cow_skin, p, "cow corpse"));
    w.items.push(item(9901, "CORPSE", 7, p, "Urist's corpse"));
    w.items.push(item(9902, "CORPSEPIECE", m.crawler_tooth, p, "cow bone"));
    let mut sim = sim_of(w, 30);
    sim.create_stockpile(Pos::new(26, 26, hz()), Pos::new(27, 27, hz()), &["refuse", "corpses"]);
    days(&mut sim, 1);
    let cat = |sim: &Sim, id: i32| sim.imeta[sim.item_idx[&id] as usize].cat;
    assert_eq!(cat(&sim, 9900), Cat::Refuse);
    assert_eq!(cat(&sim, 9901), Cat::Corpses);
    assert_eq!(cat(&sim, 9902), Cat::Refuse);
}

#[test]
fn tattered_unworn_clothing_becomes_refuse() {
    let (mut w, _) = world();
    let mut c = item(9910, "SHOES", 7, Pos::new(22, 22, hz()), "shoes");
    c.wear = 3;
    w.items.push(c);
    let mut sim = sim_of(w, 31);
    days(&mut sim, 7);
    assert_eq!(sim.imeta[sim.item_idx[&9910] as usize].cat, Cat::Refuse);
}

#[test]
fn outdoor_refuse_is_ignored_by_default_and_gather_off_stops_refuse_hauling() {
    let (mut w, m) = world();
    let mut a = item(9920, "CORPSE", m.cow_skin, Pos::new(10, 10, sz()), "cow corpse");
    a.flags.forbid = false;
    let mut b = item(9921, "CORPSE", m.cow_skin, Pos::new(22, 22, hz()), "cow corpse");
    b.flags.forbid = false;
    w.items.push(a);
    w.items.push(b);
    let mut sim = sim_of(w, 32);
    sim.create_stockpile(Pos::new(26, 26, hz()), Pos::new(27, 27, hz()), &["refuse"]);
    days(&mut sim, 1);
    sim.rebuild_haul();
    assert!(!sim.haul_pos.contains_key(&9920), "outdoor refuse is not gathered by default");
    assert!(sim.haul_pos.contains_key(&9921));
    let mut o = sim.refuse_orders();
    o.outdoor = true;
    sim.set_refuse_orders(o);
    assert!(sim.haul_pos.contains_key(&9920));
    o.gather = false;
    sim.set_refuse_orders(o);
    assert!(!sim.haul_pos.contains_key(&9920) && !sim.haul_pos.contains_key(&9921));
}

#[test]
fn dump_bones_order_marks_bones_for_dumping() {
    let (mut w, m) = world();
    let mut b = item(9930, "CORPSEPIECE", m.crawler_tooth, Pos::new(22, 22, hz()), "cow bone");
    b.flags.forbid = false;
    w.items.push(b);
    w.buildings.push(Building { id: 953, kind: "Civzone".into(), subtype: "Dump".into(), x1: 28, y1: 26, x2: 29, y2: 27, z: hz(), center: Pos::new(28, 26, hz()), built: true, owner: -1, ..Default::default() });
    let mut sim = sim_of(w, 33);
    let mut o = sim.refuse_orders();
    o.dump_bones = true;
    sim.set_refuse_orders(o);
    days(&mut sim, 1);
    assert!(it(&sim, 9930).flags.dump);
}

// ------------------------------------------------------------------ vermin (M5, M6)

fn rat() -> VSp {
    VSp { token: "RAT".into(), name: "rat".into(), eater: true, hateable: true, pen: 2, surface: true, ..Default::default() }
}

fn put_vermin(sim: &mut Sim, race: &str, p: Pos, kind: u8) {
    let t = tile(sim, p);
    let id = sim.decay.next_vermin_id;
    sim.decay.next_vermin_id += 1;
    sim.decay.vermin.push(Vermin { id, race: race.into(), tile: t, born: sim.ticks, next_meal: 0, kind });
}

#[test]
fn rat_eats_loose_food() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    let mut f = item(10000, "MEAT", 6, p, "meat");
    f.stack = 3;
    w.items.push(f);
    let mut sim = sim_of(w, 40);
    sim.set_vermin_species(vec![rat()]);
    put_vermin(&mut sim, "RAT", Pos::new(23, 22, hz()), V_EATER);
    let now = sim.ticks;
    sim.decay_pass(now);
    assert_eq!(it(&sim, 10000).stack, 2);
    assert_eq!(sim.decay_report().vermin_ate, 1);
}

#[test]
fn rat_opens_wooden_barrel_but_not_stone_or_metal() {
    for (mat, eaten) in [(3u16, true), (0u16, false)] {
        let (mut w, _) = world();
        barrel_with_meal(&mut w, Pos::new(22, 22, hz()), mat);
        let mut sim = sim_of(w, 41);
        sim.set_vermin_species(vec![rat()]);
        put_vermin(&mut sim, "RAT", Pos::new(22, 22, hz()), V_EATER);
        let now = sim.ticks;
        sim.decay_pass(now);
        assert_eq!(!alive(&sim, 9101), eaten, "barrel mat {mat}");
    }
}

#[test]
fn lizard_only_eats_loose_food() {
    let (mut w, _) = world();
    barrel_with_meal(&mut w, Pos::new(22, 22, hz()), 3);
    let mut sim = sim_of(w, 42);
    sim.set_vermin_species(vec![VSp { token: "LIZARD".into(), name: "lizard".into(), eater: true, pen: 1, surface: true, ..Default::default() }]);
    put_vermin(&mut sim, "LIZARD", Pos::new(22, 22, hz()), V_EATER);
    let now = sim.ticks;
    sim.decay_pass(now);
    assert!(alive(&sim, 9101));
}

#[test]
fn no_eater_species_means_no_food_eaten_and_cavern_roamers_only() {
    let (mut w, _) = world();
    w.items.push(item(10010, "MEAT", 6, Pos::new(22, 22, hz()), "meat"));
    let mut sim = sim_of(w, 43);
    sim.set_vermin_species(vec![VSp { token: "OLM".into(), name: "olm".into(), cavern: true, ..Default::default() }]);
    sim.create_stockpile(Pos::new(22, 22, hz()), Pos::new(22, 22, hz()), &["food"]);
    days(&mut sim, 200);
    assert_eq!(sim.decay_report().vermin_spawned, 0, "no caverns, no eaters: nothing spawns");
    assert!(alive(&sim, 10010));
}

#[test]
fn eaters_spawn_at_food_piles_and_flies_at_rot() {
    let (mut w, _) = world();
    rotten_meat(&mut w, 10020, Pos::new(20, 21, hz()));
    let mut sim = sim_of(w, 44);
    sim.set_vermin_species(vec![rat(), VSp { token: "FLY".into(), name: "fly".into(), rotter: true, surface: true, ..Default::default() }]);
    sim.create_stockpile(Pos::new(26, 26, hz()), Pos::new(27, 27, hz()), &["food"]);
    days(&mut sim, 120);
    let r = sim.decay_report();
    assert!(r.vermin_spawned >= 3, "{r:?}");
    assert!(sim.decay.vermin.len() <= MAX_VERMIN);
    assert!(sim.decay.vermin.iter().all(|v| sim.ticks < v.born + VERMIN_LIFE_DAYS * DAY), "they leave after 14 days");
}

fn cat_world(race: &str, flags: &[&str]) -> (World, Mats) {
    let (mut w, m) = world();
    let caste = CasteRaw { token: "FEMALE".into(), name: race.to_lowercase(), body_size: 5_000, flags: flags.iter().map(|s| s.to_string()).collect(), ..Default::default() };
    w.creature_raws.push(CreatureRaw { token: race.into(), name: race.to_lowercase(), castes: vec![caste], ..Default::default() });
    w.units.retain(|u| u.race != "CAT");
    w.units.push(Unit {
        id: 500,
        race: race.into(),
        caste: "FEMALE".into(),
        name: "hunter".into(),
        profession: "STANDARD".into(),
        pos: Pos::new(22, 22, hz()),
        flags: UnitFlags { animal: true, tame: true, ..Default::default() },
        age: 3.0,
        walk_speed: 900,
        hist_fig_id: -1,
        squad_id: -1,
        squad_position: -1,
        invader_civ: -1,
        pet_owner: -1,
        pasture: -1,
        body_size: 5_000,
        ..Default::default()
    });
    (w, m)
}

fn hunt_passes(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        let now = (sim.ticks / DECAY_PERIOD + 1) * DECAY_PERIOD + DECAY_PHASE;
        sim.ticks = now;
        sim.decay_pass(now);
    }
}

fn place_hunter(sim: &mut Sim, p: Pos) {
    let s = slot(sim, 500);
    let t = tile(sim, p);
    sim.ai[s].pos = t;
}

#[test]
fn cat_kills_vermin_and_remains_appear() {
    let (w, _) = cat_world("CAT", &["PET", "HUNTS_VERMIN", "ADOPTS_OWNER", "RETURNS_VERMIN_KILLS_TO_OWNER"]);
    let mut sim = sim_of(w, 50);
    sim.set_vermin_species(vec![rat()]);
    place_hunter(&mut sim, Pos::new(22, 22, hz()));
    put_vermin(&mut sim, "RAT", Pos::new(23, 23, hz()), V_ROAMER);
    hunt_passes(&mut sim, 20);
    assert!(sim.decay.vermin.is_empty());
    assert_eq!(sim.decay_report().vermin_killed, 1);
    let t = tile(&sim, Pos::new(23, 23, hz()));
    assert!(sim.world.items.iter().any(|x| x.kind == "REMAINS" && x.desc == "rat remains" && sim.world.idx(x.pos) == Some(t as usize)));
}

#[test]
fn owned_cat_brings_remains_to_owner() {
    let (w, _) = cat_world("CAT", &["PET", "HUNTS_VERMIN", "RETURNS_VERMIN_KILLS_TO_OWNER"]);
    let mut sim = sim_of(w, 51);
    sim.set_vermin_species(vec![rat()]);
    place_hunter(&mut sim, Pos::new(22, 22, hz()));
    let s = slot(&sim, 500);
    let uidx = sim.ai[s].uidx;
    sim.world.units[uidx].pet_owner = 1;
    let os = slot(&sim, 1);
    let owner_t = tile(&sim, Pos::new(26, 26, hz()));
    sim.ai[os].pos = owner_t;
    put_vermin(&mut sim, "RAT", Pos::new(23, 23, hz()), V_ROAMER);
    hunt_passes(&mut sim, 20);
    assert!(sim.world.items.iter().any(|x| x.kind == "REMAINS" && sim.world.idx(x.pos) == Some(owner_t as usize)));
}

#[test]
fn dog_does_not_hunt_vermin() {
    let (w, _) = cat_world("DOG", &["PET", "TRAINABLE_WAR", "TRAINABLE_HUNTING"]);
    let mut sim = sim_of(w, 52);
    sim.set_vermin_species(vec![rat()]);
    place_hunter(&mut sim, Pos::new(22, 22, hz()));
    put_vermin(&mut sim, "RAT", Pos::new(22, 23, hz()), V_ROAMER);
    hunt_passes(&mut sim, 50);
    assert_eq!(sim.decay.vermin.len(), 1);
}

#[test]
fn tame_falcon_hunts_wild_falcon_does_not() {
    for tame in [true, false] {
        let (w, _) = cat_world("BIRD_FALCON_PEREGRINE", &["DIVE_HUNTS_VERMIN"]);
        let mut sim = sim_of(w, 53);
        sim.set_vermin_species(vec![rat()]);
        place_hunter(&mut sim, Pos::new(22, 22, hz()));
        let s = slot(&sim, 500);
        let uidx = sim.ai[s].uidx;
        sim.world.units[uidx].flags.tame = tame;
        put_vermin(&mut sim, "RAT", Pos::new(22, 23, hz()), V_ROAMER);
        hunt_passes(&mut sim, 30);
        assert_eq!(sim.decay.vermin.is_empty(), tame);
    }
}

#[test]
fn no_vermin_no_remains() {
    let (w, _) = cat_world("CAT", &["PET", "HUNTS_VERMIN"]);
    let mut sim = Sim::new(w, 54);
    sim.run_ticks(60 * 1200);
    assert!(!sim.world.items.iter().any(|x| x.kind == "REMAINS"));
    assert_eq!(sim.animal_report().vermin_killed, 0);
}

// ------------------------------------------------------------------ wear (M7)

#[test]
fn wear_value_drops_25_percent_per_level() {
    let (mut w, _) = world();
    w.items.push(item(10100, "HELM", 0, Pos::new(22, 22, hz()), "helm"));
    let mut sim = sim_of(w, 60);
    let i = sim.item_idx[&10100] as usize;
    sim.decay_apply_wear_level(i, 0, 1);
    assert_eq!(sim.world.items[i].value, 75);
    sim.decay_apply_wear_level(i, 1, 2);
    assert_eq!(sim.world.items[i].value, 50);
    sim.decay_apply_wear_level(i, 2, 3);
    assert_eq!(sim.world.items[i].value, 25);
    assert_eq!(sim.world.items[i].wear, 3);
}

#[test]
fn xx_armor_on_refuse_pile_falls_apart_in_8_weeks() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    let mut a = item(10110, "ARMOR", 0, p, "breastplate");
    a.wear = 3;
    w.items.push(a);
    let mut sim = sim_of(w, 61);
    sim.create_stockpile(p, p, &["refuse"]);
    days(&mut sim, 7);
    assert_eq!(sim.imeta[sim.item_idx[&10110] as usize].cat, Cat::Refuse);
    days(&mut sim, 56);
    assert!(!alive(&sim, 10110));
    assert_eq!(sim.decay_report().items_worn_out, 1);
}

#[test]
fn artifact_never_wears() {
    let (mut w, _) = world();
    let p = Pos::new(22, 22, hz());
    let mut a = item(10120, "ARMOR", 0, p, "artifact mail");
    a.wear = 3;
    a.flags.artifact = true;
    w.items.push(a);
    let mut sim = sim_of(w, 62);
    sim.create_stockpile(p, p, &["refuse"]);
    days(&mut sim, 120);
    assert!(alive(&sim, 10120));
}

// ------------------------------------------------------------------ switch, save, determinism

#[test]
fn decay_off_keeps_old_rules() {
    let (mut w, _) = world();
    w.items.push(item(10200, "MEAT", 6, Pos::new(22, 22, hz()), "meat"));
    let mut a = Sim::new(w.clone(), 70);
    a.set_decay_enabled(false);
    a.run_ticks(20 * 1200);
    assert!(!a.world.item(10200).unwrap().flags.rotten, "the old 28-day rule");
    assert!(a.decay.rot.is_empty());
    let mut h = Vec::new();
    a.decay_hash(&mut |v| h.push(v));
    assert!(h.is_empty(), "nothing mixed into the hash while off");
}

#[test]
fn rot_and_vermin_state_round_trip_exact_save() {
    let (mut w, m) = world();
    w.items.push(item(10300, "CORPSE", m.cow_skin, Pos::new(22, 22, hz()), "cow corpse"));
    rotten_meat(&mut w, 10301, Pos::new(20, 21, hz()));
    let mut sim = Sim::new(w, 71);
    sim.run_ticks(5 * 1200);
    put_vermin(&mut sim, "RAT", Pos::new(23, 22, hz()), V_ROAMER);
    let mut b = Sim::new(sim.world_for_save(), 71);
    assert_eq!(b.restore_mode(), "exact");
    assert_eq!(b.decay.rot.len(), sim.decay.rot.len());
    assert_eq!(b.decay.vermin.len(), 1);
    assert_eq!(b.decay.cloud, sim.decay.cloud);
    assert_eq!(sim.state_hash(), b.state_hash());
    sim.run_ticks(10 * 1200);
    b.run_ticks(10 * 1200);
    assert_eq!(sim.state_hash(), b.state_hash());
}
