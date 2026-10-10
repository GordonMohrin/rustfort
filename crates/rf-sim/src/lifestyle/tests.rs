//! [society2] Tests: clothing wear and replacement, meal quality, kitchen, miasma.

use super::*;
use crate::locations::tests::{slot, world_with_zone};
use rf_core::{Item, ItemFlags, KitchenSetting, Pos};

fn hz() -> i16 {
    (20i16 * 2 / 3) - 4
}

fn item(id: i32, kind: &str, sub: &str, mat: u16, pos: Pos) -> Item {
    Item { id, kind: kind.into(), subtype: sub.into(), mat, pos, stack: 1, container: -1, holder: -1, building: -1, flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() }
}

#[test]
fn worn_clothing_is_replaced_from_a_stockpile() {
    let mut w = world_with_zone("Halle");
    // Shoes of dwarf skin (not metal): worn (x) on unit 1, a new pair on a pile.
    let mut old = item(9200, "SHOES", "ITEM_SHOES_SHOES", 7, w.units[0].pos);
    old.wear = 1;
    old.holder = 1;
    old.flags = ItemFlags { in_inventory: true, ..Default::default() };
    w.items.push(old);
    w.units[0].inventory.push(9200);
    w.items.push(item(9201, "SHOES", "ITEM_SHOES_SHOES", 7, Pos::new(29, 27, hz())));
    for u in &mut w.units {
        u.labors.clear();
    }
    let mut sim = Sim::new(w, 1);
    sim.create_stockpile(Pos::new(29, 27, hz()), Pos::new(30, 28, hz()), &["ARMOR"]);
    sim.run_ticks(3 * 1200);
    let u = sim.world.unit(1).unwrap();
    assert!(u.inventory.contains(&9201), "new shoes worn: {:?}", u.inventory);
    assert!(!u.inventory.contains(&9200));
    let old = sim.world.item(9200).unwrap();
    assert!(old.flags.on_ground && old.holder < 0);
    assert_eq!(sim.lifestyle_report().replacements, 1);
}

#[test]
fn clothing_wears_out_and_falls_apart() {
    let mut w = world_with_zone("Halle");
    let mut sh = item(9300, "SHOES", "ITEM_SHOES_SHOES", 7, w.units[0].pos);
    sh.wear = 3;
    sh.holder = 1;
    sh.flags = ItemFlags { in_inventory: true, ..Default::default() };
    w.items.push(sh);
    w.units[0].inventory.push(9300);
    let mut sim = Sim::new(w, 2);
    sim.run_ticks(1200);
    sim.life.wear.insert(9300, WEAR_DAYS - 1);
    sim.run_ticks(1200);
    assert!(!sim.world.unit(1).unwrap().inventory.contains(&9300));
    let r = sim.lifestyle_report();
    assert_eq!(r.rotted_off, 1);
    assert!(r.tattered_clothing >= 1);
    assert!(sim.world.unit(1).unwrap().emotions.iter().any(|e| e.thought == "RottedClothing"));
}

#[test]
fn meal_quality_follows_the_cooks_skill() {
    let w = world_with_zone("Halle");
    let mut sim = Sim::new(w, 3);
    let s = slot(&sim, 1);
    let uidx = sim.ai[s].uidx;
    sim.world.units[uidx].skills.push(rf_core::Skill { id: "COOK".into(), rating: 15, xp: 0 });
    let t = sim.ai[s].pos as usize;
    let first = sim.next_item_id;
    let id = sim.create_item("FOOD", 6, t, "plump helmet roast".into());
    let i = sim.item_idx[&id] as usize;
    sim.world.items[i].stack = 3;
    sim.lifestyle_meal_cooked(s, first, 10);
    let it = &sim.world.items[i];
    assert_eq!(it.quality, 5, "legendary cook: masterwork");
    assert_eq!(it.value, 2 * 3 * 12);
    // A cook without the skill makes ordinary meals.
    let s2 = slot(&sim, 2);
    let mut qs = Vec::new();
    for k in 0..20 {
        let first = sim.next_item_id;
        let id = sim.create_item("FOOD", 6, t, "biscuits".into());
        sim.lifestyle_meal_cooked(s2, first, 100 + k);
        qs.push(sim.world.items[sim.item_idx[&id] as usize].quality);
    }
    assert!(qs.iter().all(|&q| q == 0), "{qs:?}");
    assert_eq!(sim.lifestyle_report().meal_quality[0], 20);
}

#[test]
fn kitchen_exclusions_stop_cooking_and_brewing() {
    let mut w = world_with_zone("Halle");
    w.items.push(item(9400, "PLANT", "", 6, Pos::new(20, 21, hz())));
    let mut sim = Sim::new(w, 4);
    let i = sim.item_idx[&9400] as usize;
    sim.set_kitchen(KitchenSetting { item_type: "PLANT".into(), mat: "PLANT:PLUMP_HELMET:STRUCTURAL".into(), no_brew: true, ..Default::default() });
    assert!(sim.kitchen_excluded(i, false));
    assert!(!sim.kitchen_excluded(i, true), "cooking stays allowed");
    assert!(!sim.brewable_idx(i));
    sim.set_kitchen(KitchenSetting { item_type: "PLANT".into(), mat: "PLANT:PLUMP_HELMET:STRUCTURAL".into(), ..Default::default() });
    assert!(!sim.kitchen_excluded(i, false), "setting cleared");
    sim.set_kitchen(KitchenSetting { item_type: "".into(), mat: "PLANT:PLUMP_HELMET:STRUCTURAL".into(), no_cook: true, ..Default::default() });
    assert!(sim.kitchen_excluded(i, true), "an empty item type matches every type");
}

#[test]
fn rotting_corpses_indoors_make_miasma() {
    let mut w = world_with_zone("Halle");
    let p = Pos::new(22, 22, hz());
    let mut c = item(9500, "CORPSE", "", 7, p);
    c.flags.rotten = true;
    c.desc = "rotten dwarf corpse".into();
    w.items.push(c);
    for u in &mut w.units {
        u.labors.clear();
    }
    let mut sim = Sim::new(w, 5);
    sim.run_ticks(2 * 1200);
    assert!(sim.miasma_at(p));
    assert!(sim.miasma_at(Pos::new(24, 22, hz())), "spreads 2 tiles");
    assert!(!sim.miasma_at(Pos::new(26, 22, hz())));
    // A dwarf standing in it retches.
    let s = slot(&sim, 1);
    let t = sim.world.idx(p).unwrap() as u32;
    sim.ai[s].pos = t;
    sim.world.units[sim.ai[s].uidx].pos = p;
    sim.life.next_miasma[s] = 0;
    let now = sim.ticks;
    sim.lifestyle_tick(now, 3);
    assert!(sim.lifestyle_report().miasma_events >= 1);
    assert!(sim.world.unit(1).unwrap().emotions.iter().any(|e| e.thought == "Miasma"));
}

#[test]
fn rotting_corpses_outdoors_make_no_miasma() {
    let mut w = world_with_zone("Halle");
    let sz = 20i16 * 2 / 3 + 1;
    let out = Pos::new(10, 10, sz);
    let mut c = item(9501, "CORPSE", "", 7, out);
    c.flags.rotten = true;
    w.items.push(c);
    let outdoors = w.tile(out).map(|t| t.flags & tile_flags::OUTSIDE != 0).unwrap_or(false);
    let mut sim = Sim::new(w, 6);
    sim.run_ticks(2 * 1200);
    if outdoors {
        assert!(!sim.miasma_at(out));
        assert_eq!(sim.lifestyle_report().miasma_sources_max, 0);
    }
}
