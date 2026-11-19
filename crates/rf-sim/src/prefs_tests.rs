//! [prefs15] Tests for preferences, the quality roll, value and preference thoughts.

use super::*;
use crate::rng::Rng;
use crate::skills::quality_points_roll;
use rf_core::{Item, ItemFlags, Pos, UnitNeed, World};

fn sim() -> Sim {
    let mut s = Sim::new(World::synthetic(48, 48, 20), 5);
    s.set_invasions(false);
    s
}

fn like(kind: &str, f: impl FnOnce(&mut UnitPreference)) -> UnitPreference {
    let mut p = UnitPreference { kind: kind.into(), active: true, mat: NO_MAT, ..Default::default() };
    f(&mut p);
    p
}

/// Give unit slot `s` exactly these preferences and rebuild the cache.
fn set_prefs(sim: &mut Sim, s: usize, prefs: Vec<UnitPreference>) {
    let u = sim.ai[s].uidx;
    sim.world.units[u].preferences = prefs;
    sim.prefs_rebuild();
}

fn add_item(sim: &mut Sim, kind: &str, mat: u16, value: i32) -> i32 {
    let p = sim.world.units[0].pos;
    let t = sim.world.idx(p).unwrap();
    let id = sim.create_item(kind, mat, t, format!("test {kind}"));
    let i = sim.item_idx[&id] as usize;
    sim.world.items[i].value = value;
    id
}

fn thoughts(sim: &Sim, s: usize, t: &str) -> Vec<i32> {
    sim.world.units[sim.ai[s].uidx].emotions.iter().filter(|e| e.thought == t).map(|e| e.severity).collect()
}

fn mean_quality(level: i32, prefs: i32, n: u32, seed: u64) -> f64 {
    let mut r = Rng::new(seed);
    let a = [1000, 0, 0];
    let m = [1000, 1000, 1000];
    let mut sum = 0u64;
    for _ in 0..n {
        sum += quality_points_roll(level, a, m, 100, 100, prefs, &mut r) as u64;
    }
    sum as f64 / n as f64
}

#[test]
fn quality_roll_inclusive_bounds() {
    // Level 0, no attributes: points = rand(0..=10); 10 must be reachable (inclusive dice) but
    // never more; a quality level needs 22 points, so it stays ordinary.
    let mut r = Rng::new(3);
    let mut max_q = 0;
    for _ in 0..50_000 {
        max_q = max_q.max(quality_points_roll(0, [0; 3], [0; 3], 100, 100, 0, &mut r));
    }
    assert_eq!(max_q, 0);
    // With 2 preferences the minimum is 20 and the maximum 30: 22-29 is well-crafted, 30 finely.
    let mut h = [0u32; 6];
    for _ in 0..50_000 {
        h[quality_points_roll(0, [0; 3], [0; 3], 100, 100, 2, &mut r) as usize] += 1;
    }
    assert!(h[2] > 0, "30 points need the inclusive rand(0..=10): {h:?}");
    assert!(h[3] == 0 && h[4] == 0 && h[5] == 0, "{h:?}");
    assert!(h[0] > 0 && h[1] > 0, "{h:?}");
}

#[test]
fn preference_is_worth_about_four_levels() {
    // Wiki Skill: one preference ~ four skill levels (spec table: level 0 + 1 pref ~ level 5).
    let a = mean_quality(0, 1, 40_000, 1);
    let b = mean_quality(5, 0, 40_000, 2);
    assert!((a - b).abs() < 0.35, "level 0 + pref {a} vs level 5 {b}");
    let base = mean_quality(0, 0, 40_000, 3);
    assert!(a > base + 0.7, "{a} vs {base}");
}

#[test]
fn derived_preferences_follow_the_wiki_minimum() {
    let s = sim();
    let mut derived = 0;
    for u in s.world.units.iter().filter(|u| u.race == "DWARF") {
        derived += 1;
        assert!(u.preferences.len() >= 4, "{:?}", u.preferences);
        assert!(u.preferences.iter().all(|p| p.derived && p.active));
        let mats = u.preferences.iter().filter(|p| p.kind == "LikeMaterial").count();
        assert!(mats >= 3, "stone, metal, gem: {mats}");
        let drinks = u.preferences.iter().filter(|p| p.kind == "LikeFood" && p.item_type == "DRINK").count();
        assert_eq!(drinks, 1, "exactly one alcohol");
        // The only booze in the synthetic palette is dwarven wine.
        assert!(u.preferences.iter().any(|p| p.item_type == "DRINK" && p.mat == 5));
    }
    assert!(derived >= 7);
    assert!(s.world.units.iter().filter(|u| u.race != "DWARF").all(|u| u.preferences.is_empty()));
    // Deterministic per unit (id + facets), independent of the sim seed.
    let s2 = Sim::new(World::synthetic(48, 48, 20), 99);
    for (a, b) in s.world.units.iter().zip(&s2.world.units) {
        assert_eq!(a.preferences, b.preferences);
    }
    let mut u = s.world.units[0].clone();
    u.preferences.clear();
    u.id += 1000;
    let other = derive_preferences(&s.world, &u);
    assert_ne!(other, s.world.units[0].preferences, "another unit, other tastes");
}

#[test]
fn exported_preferences_are_kept() {
    let mut w = World::synthetic(48, 48, 20);
    w.units[0].preferences = vec![like("LikeMaterial", |p| {
        p.mat = 0;
        p.mat_token = "INORGANIC:GRANITE".into();
    })];
    let s = Sim::new(w, 5);
    assert_eq!(s.world.units[0].preferences.len(), 1);
    assert!(!s.world.units[0].preferences[0].derived);
    assert!(s.prefs_likes_mat(0, 0));
    assert_eq!(s.prefs_report().exported_units, 1);
}

#[test]
fn crafter_with_liked_material_makes_better_goods() {
    let mut s = sim();
    set_prefs(&mut s, 1, vec![like("LikeMaterial", |p| p.mat = 0)]);
    set_prefs(&mut s, 2, vec![]);
    let id = add_item(&mut s, "FIGURINE", 0, 10);
    let i = s.item_idx[&id] as usize;
    assert_eq!(s.prefs_quality_bonus(1, i), 1);
    assert_eq!(s.prefs_quality_bonus(2, i), 0);
    let (mut r1, mut r2) = (Rng::new(8), Rng::new(9));
    let (mut q1, mut q2) = (0u32, 0u32);
    for _ in 0..200 {
        q1 += quality_points_roll(2, [900, 0, 0], [1250, 1500, 1000], 100, 100, 1, &mut r1) as u32;
        q2 += quality_points_roll(2, [900, 0, 0], [1250, 1500, 1000], 100, 100, 0, &mut r2) as u32;
    }
    assert!(q1 as f64 / 200.0 >= q2 as f64 / 200.0 + 0.8, "{q1} vs {q2}");
}

#[test]
fn item_preference_matches_kind_and_subtype() {
    let mut s = sim();
    set_prefs(
        &mut s,
        1,
        vec![
            like("LikeItem", |p| p.item_type = "CHAIR".into()),
            like("LikeItem", |p| {
                p.item_type = "WEAPON".into();
                p.item_subtype = "ITEM_WEAPON_AXE_BATTLE".into();
            }),
        ],
    );
    assert!(s.prefs_likes_item(1, "CHAIR", ""));
    assert!(s.prefs_likes_item(1, "WEAPON", "ITEM_WEAPON_AXE_BATTLE"));
    assert!(!s.prefs_likes_item(1, "WEAPON", "ITEM_WEAPON_SPEAR"));
    assert!(!s.prefs_likes_item(1, "TABLE", ""));
    assert!(!s.prefs_likes_item(2, "CHAIR", ""));
}

#[test]
fn admire_value_is_value_in_the_dwarfs_eyes() {
    let mut s = sim();
    set_prefs(&mut s, 1, vec![like("LikeMaterial", |p| p.mat = 3)]);
    set_prefs(&mut s, 2, vec![]);
    let chair = add_item(&mut s, "CHAIR", 3, 10);
    assert_eq!(s.prefs_perceived_value(1, chair, 10), 20, "oak lover: ordinary oak chair x2");
    assert_eq!(s.prefs_perceived_value(2, chair, 10), 10);
    assert_eq!(admire_tier(10), 0);
    assert_eq!(admire_tier(551), 2);
}

#[test]
fn liked_material_gives_work_satisfaction() {
    let mut s = sim();
    set_prefs(&mut s, 1, vec![like("LikeMaterial", |p| p.mat = 0)]);
    let now = s.ticks;
    s.prefs_work(1, 0, "", "", "", now);
    assert_eq!(thoughts(&s, 1, "SatisfiedAtWork").len(), 1);
    // Once a day at most.
    s.prefs_work(1, 0, "", "", "", now + 10);
    assert_eq!(s.prefs_report().work_satisfied, 1);
    // No preference: nothing.
    set_prefs(&mut s, 2, vec![]);
    s.prefs_work(2, 0, "", "", "", now);
    assert!(thoughts(&s, 2, "SatisfiedAtWork").is_empty());
}

#[test]
fn cheerless_dwarf_feels_nothing_at_work() {
    let mut s = sim();
    let u = s.ai[3].uidx;
    s.world.units[u].facets.retain(|(k, _)| k != "CHEER_PROPENSITY");
    s.world.units[u].facets.push(("CHEER_PROPENSITY".into(), 20));
    set_prefs(&mut s, 3, vec![like("LikeMaterial", |p| p.mat = 0)]);
    let now = s.ticks;
    s.prefs_work(3, 0, "", "", "", now);
    assert!(thoughts(&s, 3, "SatisfiedAtWork").is_empty());
}

#[test]
fn liked_booze_gives_good_drink_and_no_same_booze() {
    let mut s = sim();
    set_prefs(&mut s, 1, vec![like("LikeFood", |p| p.mat = 5)]);
    set_prefs(&mut s, 2, vec![]);
    let now = s.ticks;
    assert!(s.prefs_drank(1, 5, 2, true, now));
    assert_eq!(thoughts(&s, 1, "GoodDrink"), vec![1]);
    assert!(thoughts(&s, 1, "SameBooze").is_empty(), "liked booze is exempt");
    s.prefs_drank(2, 5, 2, true, now);
    assert!(thoughts(&s, 2, "GoodDrink").is_empty());
    assert_eq!(thoughts(&s, 2, "SameBooze").len(), 1);
}

#[test]
fn unliked_cheap_meal_gives_no_good_meal() {
    let mut s = sim();
    set_prefs(&mut s, 1, vec![]);
    set_prefs(&mut s, 2, vec![like("LikeFood", |p| p.mat = 6)]);
    let now = s.ticks;
    s.prefs_ate(1, 6, 3, true, false, now);
    assert!(thoughts(&s, 1, "GoodMeal").is_empty());
    s.prefs_ate(1, 6, 120, true, false, now + 1);
    assert_eq!(thoughts(&s, 1, "GoodMeal"), vec![3], "worth 120: 'wonderful' tier");
    s.prefs_ate(2, 6, 3, true, true, now);
    assert_eq!(thoughts(&s, 2, "GoodMeal"), vec![1]);
    assert!(thoughts(&s, 2, "SameFood").is_empty(), "liked food is exempt");
    assert_eq!(value_tier(4), 0);
    assert_eq!(value_tier(400), 5);
}

#[test]
fn storing_finished_goods_satisfies_acquire_object() {
    let mut s = sim();
    let u = s.ai[1].uidx;
    s.world.units[u].needs = vec![UnitNeed { id: "AcquireObject".into(), focus_level: -500, need_level: 2, ..Default::default() }];
    let ring = add_item(&mut s, "RING", 0, 10);
    let boulder = add_item(&mut s, "BOULDER", 0, 3);
    let now = s.ticks;
    s.prefs_hauled(1, boulder, now);
    assert!(s.world.units[u].needs[0].focus_level < 0, "raw stone is no trade good");
    s.prefs_hauled(1, ring, now);
    assert!(s.world.units[u].needs[0].focus_level > 0);
    assert_eq!(thoughts(&s, 1, "AcquiredItem").len(), 1);
}

#[test]
fn wearing_quality_clothes_satisfies_be_extravagant() {
    let mut s = sim();
    let u = s.ai[1].uidx;
    s.world.units[u].needs = vec![UnitNeed { id: "BeExtravagant".into(), focus_level: -500, need_level: 2, ..Default::default() }];
    s.prefs_wore(1, 0);
    assert!(s.world.units[u].needs[0].focus_level < 0);
    s.prefs_wore(1, 2);
    assert!(s.world.units[u].needs[0].focus_level > 0);
}

#[test]
fn mood_uses_liked_item_type() {
    let mut s = sim();
    set_prefs(&mut s, 1, vec![like("LikeItem", |p| p.item_type = "FLOODGATE".into())]);
    set_prefs(&mut s, 2, vec![]);
    let list = crate::mood_prefs::artifact_list("CARPENTRY", false, false);
    let mut r = Rng::new(4);
    for _ in 0..20 {
        let (a, forced) = s.prefs_mood_art(1, list, &mut r);
        assert!(forced);
        assert_eq!(a.0, "FLOODGATE");
    }
    let mut kinds = std::collections::BTreeSet::new();
    for _ in 0..200 {
        kinds.insert(s.prefs_mood_art(2, list, &mut r).0 .0);
    }
    assert!(kinds.len() > 10, "no preference: the whole carpenter list {kinds:?}");
    // A liked weapon subtype makes a bone carver's "any weapon" entry that weapon.
    set_prefs(&mut s, 3, vec![like("LikeItem", |p| {
        p.item_type = "WEAPON".into();
        p.item_subtype = "ITEM_WEAPON_SPEAR".into();
    })]);
    let (a, _) = s.prefs_mood_art(3, crate::mood_prefs::artifact_list("BONECARVE", false, false), &mut r);
    assert_eq!(a, ("WEAPON", "ITEM_WEAPON_SPEAR", "spear"));
}

#[test]
fn metalworker_wants_liked_smelted_metal() {
    let mut s = sim();
    let mut w = s.world.materials[0].clone();
    w.token = "INORGANIC:SILVER".into();
    w.is_stone = false;
    w.is_metal = true;
    s.world.materials.push(w);
    let silver = (s.world.materials.len() - 1) as u16;
    set_prefs(&mut s, 1, vec![like("LikeMaterial", |p| p.mat = silver)]);
    assert_eq!(s.prefs_mood_metal(1, "FORGE_WEAPON"), NO_MAT, "never smelted here");
    s.prefs_smelted(silver);
    assert_eq!(s.prefs_mood_metal(1, "FORGE_WEAPON"), silver);
    assert_eq!(s.prefs_mood_metal(1, "CARPENTRY"), NO_MAT);
}

#[test]
fn bone_carver_who_likes_shells_uses_shells() {
    let mut s = sim();
    set_prefs(&mut s, 1, vec![like("LikeMaterial", |p| p.mat_token = "CREATURE:TURTLE:SHELL".into())]);
    assert!(s.prefs_likes_shell(1));
    assert!(!s.prefs_likes_shell(2) || s.world.units[s.ai[2].uidx].preferences.iter().any(|p| p.mat_token.ends_with(":SHELL")));
}

#[test]
fn prefs_off_restores_old_rules() {
    let mut s = sim();
    set_prefs(&mut s, 1, vec![like("LikeMaterial", |p| p.mat = 0)]);
    s.set_prefs_enabled(false);
    let id = add_item(&mut s, "FIGURINE", 0, 10);
    let i = s.item_idx[&id] as usize;
    assert_eq!(s.prefs_quality_bonus(1, i), 0);
    assert!(!s.prefs_ate(1, 0, 3, true, false, 0));
}

#[test]
fn unit_preferences_api_lists_lines() {
    let s = sim();
    let uid = s.world.units[0].id;
    let lines = s.unit_preferences(uid);
    assert!(lines.len() >= 4);
    assert!(lines.iter().all(|l| l.derived));
    assert!(lines.iter().any(|l| l.text == "likes to drink dwarven wine"), "{lines:?}");
    let _ = Pos::new(0, 0, 0);
    let _ = Item { flags: ItemFlags::default(), ..Default::default() };
}
