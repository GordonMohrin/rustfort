//! [prefs15] Unit preferences (wiki "Preferences") and what they change: the item quality roll
//! (+10 per matching item / material preference, wiki "Skill"), the value of things in a
//! dwarf's eyes (admiring furniture), work satisfaction, food and drink thoughts, the
//! AcquireObject / BeExtravagant needs and strange mood artifacts (`mood_prefs`).
//!
//! Preferences come from the export (`Unit::preferences`, exporter round 15). Units without any
//! (older exports, sim-born units) get derived ones (`derive_preferences`): the wiki's chances,
//! seeded by unit id and personality facets (not by the sim seed), stored back into the unit
//! with `derived: true` so saves keep them. Spec: docs/mechanics/prefs15.md.
//!
//! State: `Prefs` (in the exact snapshot); the per-unit `PrefSet` lookup tables are a cache
//! rebuilt by `prefs_rebuild` (init and after an exact load).

use crate::rng::{splitmix, Rng};
use crate::social::Class;
use crate::Sim;
use rf_core::{Unit, UnitPreference, World, NO_MAT};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub use crate::prefs15_api::PrefsReport;

/// Thought tiers by value (wiki "Thought", meals and drinks: 5 / 25 / 100 / 200 / 400).
pub(crate) fn value_tier(v: i64) -> i32 {
    match v {
        i64::MIN..=4 => 0,
        5..=24 => 1,
        25..=99 => 2,
        100..=199 => 3,
        200..=399 => 4,
        _ => 5,
    }
}

/// Admire tiers by perceived value (ESTIMATE thresholds for the four DF captions).
pub(crate) fn admire_tier(v: i64) -> i32 {
    match v {
        i64::MIN..=49 => 0,
        50..=249 => 1,
        250..=999 => 2,
        _ => 3,
    }
}

/// Admire chance per daily social think when furniture is near (ESTIMATE, calibrated to the
/// export's ~2.5 admire thoughts per citizen per 28 days).
pub(crate) const ADMIRE_NUM: u32 = 1;
pub(crate) const ADMIRE_DEN: u32 = 7;
const DAY: u64 = 1200;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Prefs {
    /// Off = the pre-v0.15 behaviour (A/B switch, `set_prefs_enabled`).
    pub off: bool,
    /// Metals the fort has smelted itself (strange mood demands).
    pub smelted: BTreeSet<u16>,
    pub rep: PrefsReport,
    /// Next tick a unit (by unit index) may feel "satisfied at work" again.
    pub next_work: Vec<u64>,
    #[serde(skip)]
    pub sets: Vec<PrefSet>,
    #[serde(skip)]
    pub intern: BTreeMap<String, u32>,
}

/// Lookup tables of one unit's active preferences (sorted, binary searched).
#[derive(Clone, Debug, Default)]
pub(crate) struct PrefSet {
    pub mats: Vec<u16>,
    pub foods: Vec<u16>,
    /// (kind, subtype) interned; subtype `ANY` = any subtype.
    pub items: Vec<(u32, u32)>,
    pub creatures: Vec<u32>,
    pub hated: Vec<u32>,
    pub cheer: i32,
}

const ANY: u32 = u32::MAX;
const NO_SUB: u32 = u32::MAX - 1;

// ---------------------------------------------------------------------------- derivation

/// Item kinds a dwarf may like (wiki Preferences), with their chance per group (num, den).
const WEAPONS: &[(&str, &str)] = &[
    ("WEAPON", "ITEM_WEAPON_AXE_BATTLE"),
    ("WEAPON", "ITEM_WEAPON_HAMMER_WAR"),
    ("WEAPON", "ITEM_WEAPON_SWORD_SHORT"),
    ("WEAPON", "ITEM_WEAPON_SPEAR"),
    ("WEAPON", "ITEM_WEAPON_MACE"),
    ("WEAPON", "ITEM_WEAPON_CROSSBOW"),
    ("WEAPON", "ITEM_WEAPON_PICK"),
];
const AMMO: &[(&str, &str)] = &[("AMMO", "ITEM_AMMO_BOLTS")];
const ARMOR: &[(&str, &str)] = &[
    ("ARMOR", "ITEM_ARMOR_BREASTPLATE"),
    ("ARMOR", "ITEM_ARMOR_MAIL_SHIRT"),
    ("HELM", "ITEM_HELM_HELM"),
    ("HELM", "ITEM_HELM_CAP"),
    ("PANTS", "ITEM_PANTS_GREAVES"),
    ("PANTS", "ITEM_PANTS_LEGGINGS"),
    ("GLOVES", "ITEM_GLOVES_GAUNTLETS"),
    ("SHOES", "ITEM_SHOES_BOOTS"),
    ("SHOES", "ITEM_SHOES_BOOTS_LOW"),
    ("SHIELD", "ITEM_SHIELD_SHIELD"),
    ("SHIELD", "ITEM_SHIELD_BUCKLER"),
];
const CLOTHING: &[(&str, &str)] = &[
    ("ARMOR", "ITEM_ARMOR_CLOAK"),
    ("ARMOR", "ITEM_ARMOR_COAT"),
    ("ARMOR", "ITEM_ARMOR_SHIRT"),
    ("ARMOR", "ITEM_ARMOR_ROBE"),
    ("ARMOR", "ITEM_ARMOR_DRESS"),
    ("ARMOR", "ITEM_ARMOR_TUNIC"),
    ("ARMOR", "ITEM_ARMOR_TOGA"),
    ("ARMOR", "ITEM_ARMOR_VEST"),
    ("PANTS", "ITEM_PANTS_TROUSERS"),
    ("PANTS", "ITEM_PANTS_LOINCLOTH"),
    ("PANTS", "ITEM_PANTS_BRAIES"),
    ("HELM", "ITEM_HELM_HOOD"),
    ("HELM", "ITEM_HELM_SCARF_HEAD"),
    ("GLOVES", "ITEM_GLOVES_GLOVES"),
    ("GLOVES", "ITEM_GLOVES_MITTENS"),
    ("SHOES", "ITEM_SHOES_SOCKS"),
    ("SHOES", "ITEM_SHOES_SHOES"),
    ("SHOES", "ITEM_SHOES_SANDAL"),
    ("BACKPACK", ""),
    ("QUIVER", ""),
];
const FURNITURE: &[(&str, &str)] = &[
    ("DOOR", ""),
    ("FLOODGATE", ""),
    ("BED", ""),
    ("CHAIR", ""),
    ("WINDOW", ""),
    ("CAGE", ""),
    ("BARREL", ""),
    ("TABLE", ""),
    ("COFFIN", ""),
    ("STATUE", ""),
    ("BOX", ""),
    ("ARMORSTAND", ""),
    ("WEAPONRACK", ""),
    ("CABINET", ""),
    ("BIN", ""),
    ("HATCH_COVER", ""),
    ("GRATE", ""),
    ("QUERN", ""),
    ("MILLSTONE", ""),
    ("TRACTION_BENCH", ""),
    ("SLAB", ""),
];
const CRAFTS: &[(&str, &str)] =
    &[("FIGURINE", ""), ("AMULET", ""), ("SCEPTER", ""), ("CROWN", ""), ("RING", ""), ("EARRING", ""), ("BRACELET", ""), ("GEM", "")];
const MISC: &[(&str, &str)] = &[
    ("CATAPULTPARTS", ""),
    ("BALLISTAPARTS", ""),
    ("SIEGEAMMO", ""),
    ("TRAPCOMP", ""),
    ("COIN", ""),
    ("ANVIL", ""),
    ("TOTEM", ""),
    ("CHAIN", ""),
    ("FLASK", ""),
    ("GOBLET", ""),
    ("BUCKET", ""),
    ("ANIMALTRAP", ""),
    ("INSTRUMENT", ""),
    ("TOY", ""),
    ("SPLINT", ""),
    ("CRUTCH", ""),
    ("TOOL", ""),
];

/// World counts from the vanilla raws (stone = layer 25 + mineral 58 + 3 ceramics, metals 26,
/// gems 127; wood: trees in the plant raws, else 60). The palette only holds what the fort has
/// seen; drawing over the world count keeps DF's odds that the liked material is here (ESTIMATE).
const N_STONE: usize = 86;
const N_METAL: usize = 26;
const N_GEM: usize = 127;
const N_WOOD: usize = 60;

/// Candidate pools for derived preferences, built once per derivation pass.
#[derive(Default)]
struct Pools {
    stone: Vec<u16>,
    metal: Vec<u16>,
    gem: Vec<u16>,
    wood: Vec<u16>,
    n_wood: usize,
    /// (pool, chance num, den) for the optional material classes, wiki order.
    optional: Vec<(Vec<u16>, u32, u32)>,
    drink: Vec<u16>,
    /// (pool, item_type, num, den) for the optional food classes.
    foods: Vec<(Vec<u16>, &'static str, u32, u32)>,
    domestic: Vec<String>,
    wild: Vec<String>,
    hateable: Vec<String>,
    trees: Vec<String>,
    plants: Vec<String>,
}

fn pools(world: &World) -> Pools {
    let mut p = Pools::default();
    let mut opt: Vec<Vec<u16>> = vec![Vec::new(); 11];
    let mut food: Vec<Vec<u16>> = vec![Vec::new(); 5];
    for (i, m) in world.materials.iter().enumerate() {
        let i = i as u16;
        let t = m.token.as_str();
        if m.is_gem {
            p.gem.push(i);
        } else if m.is_metal && !m.is_ore {
            p.metal.push(i);
        } else if m.is_stone {
            p.stone.push(i);
        } else if m.is_wood {
            p.wood.push(i);
        }
        if m.is_glass {
            opt[0].push(i);
        }
        let c = |suf: &str| t.ends_with(suf);
        let creature = t.starts_with("CREATURE:");
        if creature && c(":LEATHER") {
            opt[1].push(i);
        } else if creature && c(":HORN") {
            opt[2].push(i);
        } else if creature && (c(":IVORY") || c(":TOOTH")) {
            opt[3].push(i);
        } else if creature && (t.contains("CORAL") || t.contains("AMBER")) {
            opt[4].push(i);
        } else if creature && c(":BONE") {
            opt[5].push(i);
        } else if creature && c(":SHELL") {
            opt[6].push(i);
        } else if creature && c(":SILK") {
            opt[7].push(i);
        } else if creature && (c(":YARN") || c(":HAIR") || c(":WOOL")) {
            opt[8].push(i);
        } else if t.starts_with("PLANT:") && c(":THREAD") {
            opt[9].push(i);
        } else if c(":PAPER") || c(":PARCHMENT") {
            opt[10].push(i);
        }
        if c(":DRINK") || (t.starts_with("PLANT:") && t.contains("DRINK")) {
            p.drink.push(i);
        } else if creature && c(":MUSCLE") {
            food[0].push(i);
        } else if c(":CHEESE") {
            food[1].push(i);
        } else if t.starts_with("PLANT:") && c(":STRUCTURAL") {
            food[2].push(i);
        } else if c(":MILL") {
            food[3].push(i);
        } else if c(":SEED") {
            food[4].push(i);
        }
    }
    // Material classes in wiki order: glass 1/7, leather 1/3, horn 1/7, ivory 1/7, coral/amber
    // 1/15, bone 1/7, shell 1/14, silk 1/14, yarn 1/14, plant cloth 1/7, paper 1/7.
    let chances = [(1, 7), (1, 3), (1, 7), (1, 7), (1, 15), (1, 7), (1, 14), (1, 14), (1, 14), (1, 7), (1, 7)];
    for (k, pool) in opt.into_iter().enumerate() {
        p.optional.push((pool, chances[k].0, chances[k].1));
    }
    // Foods: meat 1/2, cheese 1/10, edible plant 1/7, mill powder 1/5, seed 1/7.
    let fch: [(&str, u32, u32); 5] = [("MEAT", 1, 2), ("CHEESE", 1, 10), ("PLANT", 1, 7), ("POWDER_MISC", 1, 5), ("SEEDS", 1, 7)];
    for (k, pool) in food.into_iter().enumerate() {
        p.foods.push((pool, fch[k].0, fch[k].1, fch[k].2));
    }
    let prefstring = world.creature_raws.iter().any(|c| c.flags.iter().any(|f| f == "HAS_PREFSTRING"));
    for c in &world.creature_raws {
        // The exporter writes DFHack creature flags: caste tokens appear as `HAS_ANY_<TOKEN>`
        // (`HAS_ANY_VERMIN_HATEABLE`, `HAS_ANY_COMMON_DOMESTIC`); raws use the bare token.
        let has = |f: &str| c.flags.iter().any(|x| x == f || x.strip_prefix("HAS_ANY_") == Some(f));
        if has("VERMIN_HATEABLE") {
            p.hateable.push(c.token.clone());
        }
        if c.token == "DWARF" || has("MEGABEAST") || has("TITAN") || has("UNIQUE_DEMON") || has("DEMON") || has("FEATURE_BEAST") || has("EQUIPMENT_WAGON") {
            continue;
        }
        if has("COMMON_DOMESTIC") || has("PET") {
            p.domestic.push(c.token.clone());
        } else if has("HAS_PREFSTRING") || (!prefstring && !has("INTELLIGENT_SPEAKS")) {
            // Without prefstring data: no civilized races (they have no [PREFSTRING]) ESTIMATE.
            p.wild.push(c.token.clone());
        }
    }
    for pr in &world.plant_raws {
        if pr.is_tree {
            p.trees.push(pr.token.clone());
        } else {
            p.plants.push(pr.token.clone());
        }
    }
    p.n_wood = if p.trees.is_empty() { N_WOOD } else { p.trees.len() };
    p
}

fn like_mat(world: &World, kind: &str, mat: u16, item_type: &str) -> UnitPreference {
    let tok = if mat == NO_MAT { "?".to_string() } else { world.materials.get(mat as usize).map(|m| m.token.clone()).unwrap_or_default() };
    UnitPreference {
        kind: kind.into(),
        mat,
        mat_token: tok,
        mat_state: if kind == "LikeMaterial" { "Solid".into() } else { String::new() },
        item_type: item_type.into(),
        active: true,
        derived: true,
        ..Default::default()
    }
}

fn pick<'a, T>(r: &mut Rng, v: &'a [T]) -> Option<&'a T> {
    if v.is_empty() {
        None
    } else {
        Some(&v[r.below(v.len() as u32) as usize])
    }
}

/// Seed of a unit's derived preferences: unit id and its facets (export order).
fn pref_seed(u: &Unit) -> u64 {
    let mut h = 0x9F5E_15u64 ^ u.id as u64;
    for (_, v) in &u.facets {
        h = splitmix(h ^ *v as u64);
    }
    splitmix(h)
}

/// Generate a dwarf's preferences with the wiki's chances (deterministic per unit).
fn derive_with(world: &World, u: &Unit, p: &Pools) -> Vec<UnitPreference> {
    let mut r = Rng::new(pref_seed(u));
    let mut out = Vec::new();
    let world_draw = |r: &mut Rng, pool: &[u16], n_world: usize| -> u16 {
        let n = n_world.max(pool.len()).max(1);
        let k = r.below(n as u32) as usize;
        if k < pool.len() {
            pool[k]
        } else {
            NO_MAT
        }
    };
    // Exactly one stone, metal and gem.
    let class = |mut u: UnitPreference, c: &str| {
        if u.mat == NO_MAT {
            u.mat_token = format!("?{c}");
        }
        u
    };
    for (pool, n, c) in [(&p.stone, N_STONE, "stone"), (&p.metal, N_METAL, "metal"), (&p.gem, N_GEM, "gem")] {
        let m = world_draw(&mut r, pool, n);
        out.push(class(like_mat(world, "LikeMaterial", m, ""), c));
    }
    if r.chance(1, 5) {
        let m = world_draw(&mut r, &p.wood, p.n_wood);
        out.push(class(like_mat(world, "LikeMaterial", m, ""), "wood"));
    }
    for (pool, num, den) in &p.optional {
        if r.chance(*num, *den) {
            if let Some(&m) = pick(&mut r, pool) {
                out.push(like_mat(world, "LikeMaterial", m, ""));
            }
        }
    }
    if r.chance(1, 2) {
        out.push(UnitPreference { kind: "LikeColor".into(), descriptor: format!("COLOR#{}", r.below(136)), active: true, derived: true, ..Default::default() });
    }
    if r.chance(1, 5) {
        out.push(UnitPreference { kind: "LikeShape".into(), descriptor: format!("SHAPE#{}", r.below(43)), active: true, derived: true, ..Default::default() });
    }
    let item = |r: &mut Rng, list: &[(&str, &str)], out: &mut Vec<UnitPreference>| {
        if let Some(&(k, s)) = pick(r, list) {
            out.push(UnitPreference { kind: "LikeItem".into(), item_type: k.into(), item_subtype: s.into(), active: true, derived: true, ..Default::default() });
        }
    };
    if r.chance(1, 3) {
        let list = if r.chance(4, 5) { WEAPONS } else { AMMO };
        item(&mut r, list, &mut out);
    }
    if r.chance(1, 3) {
        item(&mut r, ARMOR, &mut out);
    }
    if r.chance(1, 7) {
        item(&mut r, CLOTHING, &mut out);
    }
    for list in [FURNITURE, CRAFTS, MISC] {
        if r.chance(1, 3) {
            item(&mut r, list, &mut out);
        }
    }
    let mut liked: Vec<&str> = Vec::new();
    for pool in [&p.domestic, &p.wild] {
        if r.chance(1, 2) {
            if let Some(t) = pick(&mut r, pool) {
                liked.push(t);
                out.push(UnitPreference { kind: "LikeCreature".into(), token: t.clone(), active: true, derived: true, ..Default::default() });
            }
        }
    }
    if r.chance(1, 50) {
        if let Some(t) = pick(&mut r, &p.trees) {
            out.push(UnitPreference { kind: "LikeTree".into(), token: t.clone(), active: true, derived: true, ..Default::default() });
        }
    }
    if r.chance(1, 10) {
        if let Some(t) = pick(&mut r, &p.plants) {
            out.push(UnitPreference { kind: "LikePlant".into(), token: t.clone(), active: true, derived: true, ..Default::default() });
        }
    }
    for form in ["LikePoeticForm", "LikeMusicalForm", "LikeDanceForm"] {
        if r.chance(1, 2) {
            out.push(UnitPreference { kind: form.into(), descriptor: format!("FORM#{}", r.below(20)), active: true, derived: true, ..Default::default() });
        }
    }
    for (pool, it, num, den) in &p.foods {
        if r.chance(*num, *den) {
            if let Some(&m) = pick(&mut r, pool) {
                out.push(like_mat(world, "LikeFood", m, it));
            }
        }
    }
    // Exactly one alcohol.
    let m = pick(&mut r, &p.drink).copied().unwrap_or(NO_MAT);
    out.push(like_mat(world, "LikeFood", m, "DRINK"));
    // Always one hated vermin not already liked.
    let hate: Vec<&String> = p.hateable.iter().filter(|t| !liked.contains(&t.as_str())).collect();
    if let Some(t) = pick(&mut r, &hate) {
        out.push(UnitPreference { kind: "HateCreature".into(), token: (*t).clone(), active: true, derived: true, ..Default::default() });
    }
    out
}

/// Derived preferences for one unit (public for tests and the API).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn derive_preferences(world: &World, u: &Unit) -> Vec<UnitPreference> {
    derive_with(world, u, &pools(world))
}

/// Does this unit get derived preferences? Intelligent non-animals (dwarves in fort mode).
fn wants_prefs(u: &Unit) -> bool {
    !u.flags.animal && u.race == "DWARF" && u.preferences.is_empty()
}

fn intern(map: &mut BTreeMap<String, u32>, s: &str) -> u32 {
    if let Some(&v) = map.get(s) {
        return v;
    }
    let v = map.len() as u32;
    map.insert(s.to_string(), v);
    v
}

fn build_set(u: &Unit, map: &mut BTreeMap<String, u32>) -> PrefSet {
    let mut ps = PrefSet { cheer: 50, ..Default::default() };
    if let Some((_, v)) = u.facets.iter().find(|(k, _)| k == "CHEER_PROPENSITY") {
        ps.cheer = *v;
    }
    for p in u.preferences.iter().filter(|p| p.active) {
        match p.kind.as_str() {
            "LikeMaterial" if p.mat != NO_MAT => ps.mats.push(p.mat),
            "LikeFood" if p.mat != NO_MAT => ps.foods.push(p.mat),
            "LikeItem" if !p.item_type.is_empty() => {
                let k = intern(map, &p.item_type);
                let s = if p.item_subtype.is_empty() { ANY } else { intern(map, &p.item_subtype) };
                ps.items.push((k, s));
            }
            "LikeCreature" if !p.token.is_empty() => ps.creatures.push(intern(map, &p.token)),
            "HateCreature" if !p.token.is_empty() => ps.hated.push(intern(map, &p.token)),
            _ => {}
        }
    }
    for v in [&mut ps.mats, &mut ps.foods] {
        v.sort_unstable();
        v.dedup();
    }
    ps.items.sort_unstable();
    ps.items.dedup();
    ps.creatures.sort_unstable();
    ps.hated.sort_unstable();
    ps
}

impl Sim {
    /// Hook from `init_units`: derive missing preferences of new units and extend the cache.
    pub(crate) fn prefs_init_units(&mut self) {
        let from = self.prefs.sets.len();
        let n = self.world.units.len();
        if from >= n {
            return;
        }
        let pools = if self.world.units[from..].iter().any(wants_prefs) { Some(pools(&self.world)) } else { None };
        for i in from..n {
            match &pools {
                Some(pools) if wants_prefs(&self.world.units[i]) => {
                    let v = derive_with(&self.world, &self.world.units[i], pools);
                    self.world.units[i].preferences = v;
                    self.prefs.rep.derived_units += 1;
                }
                _ => {
                    // Counted also when no unit of the batch needs derived ones.
                    if self.world.units[i].preferences.iter().any(|p| !p.derived) {
                        self.prefs.rep.exported_units += 1;
                    }
                }
            }
        }
        for i in from..n {
            let set = build_set(&self.world.units[i], &mut self.prefs.intern);
            self.prefs.sets.push(set);
        }
        if self.prefs.next_work.len() < n {
            self.prefs.next_work.resize(n, 0);
        }
    }

    /// Rebuild the per-unit cache (after an exact load).
    pub(crate) fn prefs_rebuild(&mut self) {
        self.prefs.sets.clear();
        self.prefs.intern.clear();
        for i in 0..self.world.units.len() {
            let set = build_set(&self.world.units[i], &mut self.prefs.intern);
            self.prefs.sets.push(set);
        }
    }

    /// Part of `state_hash`.
    pub(crate) fn prefs_hash(&self, mix: &mut dyn FnMut(u64)) {
        if self.prefs.off {
            return;
        }
        for &m in &self.prefs.smelted {
            mix(m as u64);
        }
        let r = &self.prefs.rep;
        mix(r.quality_bonus ^ r.work_satisfied << 16 ^ r.admired << 32 ^ r.acquired << 48);
        mix(r.good_meals ^ r.good_drinks << 20 ^ r.mood_forced << 40);
    }

    #[inline]
    fn pset(&self, s: usize) -> Option<&PrefSet> {
        let u = self.ai.get(s)?.uidx;
        self.prefs.sets.get(u)
    }

    pub(crate) fn prefs_on(&self) -> bool {
        !self.prefs.off
    }

    pub(crate) fn prefs_likes_mat(&self, s: usize, mat: u16) -> bool {
        self.pset(s).map(|p| p.mats.binary_search(&mat).is_ok()).unwrap_or(false)
    }

    pub(crate) fn prefs_likes_food(&self, s: usize, mat: u16) -> bool {
        self.pset(s).map(|p| p.foods.binary_search(&mat).is_ok()).unwrap_or(false)
    }

    pub(crate) fn prefs_likes_item(&self, s: usize, kind: &str, sub: &str) -> bool {
        let Some(p) = self.pset(s) else { return false };
        if p.items.is_empty() {
            return false;
        }
        let Some(&k) = self.prefs.intern.get(kind) else { return false };
        let sid = if sub.is_empty() { NO_SUB } else { self.prefs.intern.get(sub).copied().unwrap_or(NO_SUB) };
        p.items.iter().any(|&(pk, ps)| pk == k && (ps == ANY || ps == sid))
    }

    pub(crate) fn prefs_likes_creature(&self, s: usize, token: &str) -> bool {
        let Some(p) = self.pset(s) else { return false };
        let Some(&c) = self.prefs.intern.get(token) else { return false };
        p.creatures.binary_search(&c).is_ok()
    }

    #[allow(dead_code)] // hated vermin: no vermin simulated yet
    pub(crate) fn prefs_hates_creature(&self, s: usize, token: &str) -> bool {
        let Some(p) = self.pset(s) else { return false };
        let Some(&c) = self.prefs.intern.get(token) else { return false };
        p.hated.binary_search(&c).is_ok()
    }

    /// Number of preferences an item matches for slot `s`: its kind/subtype and its material.
    pub(crate) fn prefs_matches(&self, s: usize, kind: &str, sub: &str, mat: u16) -> i32 {
        if self.prefs.off {
            return 0;
        }
        self.prefs_likes_item(s, kind, sub) as i32 + self.prefs_likes_mat(s, mat) as i32
    }

    /// Quality roll bonus count (wiki Skill steps 8/9) for the maker in slot `s`.
    pub(crate) fn prefs_quality_bonus(&mut self, s: usize, item_idx: usize) -> i32 {
        let it = &self.world.items[item_idx];
        let n = self.prefs_matches(s, &it.kind, &it.subtype, it.mat);
        if n > 0 {
            self.prefs.rep.quality_bonus += 1;
        }
        n
    }

    /// Value of an item in the eyes of slot `s` (wiki Preferences, room value): the whole value
    /// times (1 + matching preferences). `fallback` if the item has no value.
    pub(crate) fn prefs_perceived_value(&self, s: usize, item_id: i32, fallback: i64) -> i64 {
        let Some(&i) = self.item_idx.get(&item_id) else { return fallback };
        let it = &self.world.items[i as usize];
        let v = if it.value > 0 { it.value as i64 } else { fallback };
        v * (1 + self.prefs_matches(s, &it.kind, &it.subtype, it.mat) as i64)
    }

    /// Work with a liked material, item kind or creature: "satisfied at work" (wiki Thought,
    /// Job satisfaction), never for CHEER_PROPENSITY <= 24 (wiki Personality facet); at most
    /// once a day (ESTIMATE).
    pub(crate) fn prefs_work(&mut self, s: usize, mat: u16, kind: &str, sub: &str, creature: &str, now: u64) {
        if self.prefs.off || !self.is_citizen_slot(s) {
            return;
        }
        let u = self.ai[s].uidx;
        let Some(p) = self.prefs.sets.get(u) else { return };
        if p.cheer <= 24 || self.prefs.next_work.get(u).copied().unwrap_or(0) > now {
            return;
        }
        let liked = (mat != NO_MAT && self.prefs_likes_mat(s, mat))
            || (!kind.is_empty() && self.prefs_likes_item(s, kind, sub))
            || (!creature.is_empty() && self.prefs_likes_creature(s, creature));
        if !liked {
            return;
        }
        if self.prefs.next_work.len() <= u {
            self.prefs.next_work.resize(u + 1, 0);
        }
        self.prefs.next_work[u] = now + DAY;
        self.prefs.rep.work_satisfied += 1;
        self.feel(s, "SatisfiedAtWork", Class::PosMild, 60, -1, 0, now);
    }

    /// Products of a finished job: work satisfaction for liked products.
    pub(crate) fn prefs_job_products(&mut self, s: usize, first: i32, now: u64) {
        if self.prefs.off {
            return;
        }
        for id in first..self.next_item_id {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let (mat, liked) = {
                let it = &self.world.items[i as usize];
                (it.mat, self.prefs_likes_item(s, &it.kind, &it.subtype))
            };
            if liked {
                let (k, sb) = {
                    let it = &self.world.items[i as usize];
                    (it.kind.clone(), it.subtype.clone())
                };
                self.prefs_work(s, mat, &k, &sb, "", now);
                return;
            }
            if self.prefs_likes_mat(s, mat) {
                self.prefs_work(s, mat, "", "", "", now);
                return;
            }
        }
    }

    /// A unit stored item `id` in a stockpile, bin or the depot (wiki Personality need,
    /// AcquireObject: "Store item in stockpile/bin", "Bring item to depot" with a trade good).
    pub(crate) fn prefs_hauled(&mut self, s: usize, id: i32, now: u64) {
        if self.prefs.off || !self.is_citizen_slot(s) {
            return;
        }
        let Some(&i) = self.item_idx.get(&id) else { return };
        if !crate::skills::has_quality(&self.world.items[i as usize].kind) {
            return;
        }
        let uidx = self.ai[s].uidx;
        if !self.world.units[uidx].needs.iter().any(|n| n.id == "AcquireObject") {
            return;
        }
        let before = self.satisfy(s, "AcquireObject");
        if before < 0 {
            self.prefs.rep.acquired += 1;
            self.feel(s, "AcquiredItem", Class::PosMild, 60, -1, 0, now);
        }
    }

    /// A unit put on a worn item of quality `q` (wiki Personality need, BeExtravagant).
    pub(crate) fn prefs_wore(&mut self, s: usize, q: u8) {
        if self.prefs.off || q < 1 {
            return;
        }
        if self.world.units[self.ai[s].uidx].needs.iter().any(|n| n.id == "BeExtravagant") {
            self.satisfy(s, "BeExtravagant");
            self.prefs.rep.extravagant += 1;
        }
    }

    /// Drink thought (wiki Thought, Nourishment): GoodDrink only for a liked booze, level by
    /// value; SameBooze never for a liked one. Returns false when prefs are off (old rule).
    pub(crate) fn prefs_drank(&mut self, s: usize, mat: u16, value_per_unit: i64, same: bool, now: u64) -> bool {
        if self.prefs.off {
            return false;
        }
        if self.prefs_likes_food(s, mat) {
            let t = value_tier(value_per_unit).max(1);
            self.prefs.rep.good_drinks += 1;
            self.feel(s, "GoodDrink", if t >= 4 { Class::Pos } else { Class::PosMild }, 60 + 20 * t, -1, t, now);
        } else if same {
            self.feel(s, "SameBooze", Class::NegMild, 50, -1, 0, now);
        }
        true
    }

    /// Meal thought: GoodMeal if the food is liked or worth >= 20, level by value; the
    /// EatGoodMeal need likewise; SameFood never for a liked food.
    pub(crate) fn prefs_ate(&mut self, s: usize, mat: u16, value_per_unit: i64, meal: bool, same: bool, now: u64) -> bool {
        if self.prefs.off {
            return false;
        }
        let liked = self.prefs_likes_food(s, mat);
        if meal && (liked || value_per_unit >= 20) {
            let t = value_tier(value_per_unit).max(1);
            let class = match t {
                0..=1 => Class::PosMild,
                2..=4 => Class::Pos,
                _ => Class::PosStrong,
            };
            self.prefs.rep.good_meals += 1;
            self.feel(s, "GoodMeal", class, 80 + 20 * t, -1, t, now);
            self.satisfy(s, "EatGoodMeal");
        } else if same && !liked {
            self.feel(s, "SameFood", Class::NegMild, 50, -1, 0, now);
        }
        true
    }

    /// The fort smelted a bar of metal `mat` (mood metal demands).
    pub(crate) fn prefs_smelted(&mut self, mat: u16) {
        if mat != NO_MAT {
            self.prefs.smelted.insert(mat);
        }
    }

    /// Bars made by a smelter job (items `first..`): their metals count as smelted here.
    pub(crate) fn prefs_smelted_from(&mut self, first: i32) {
        for id in first..self.next_item_id {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let it = &self.world.items[i as usize];
            if it.kind == "BAR" && self.world.material(it.mat).map(|m| m.is_metal).unwrap_or(false) {
                let m = it.mat;
                self.prefs_smelted(m);
            }
        }
    }
}

#[cfg(test)]
#[path = "prefs_tests.rs"]
mod tests;
