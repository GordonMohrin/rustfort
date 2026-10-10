//! Round-8 export data (exporter rev `2026-10-10e`): work details, kitchen, locations, crimes,
//! hauling routes into `World`; stockpile filters/links and the animal fields come with
//! `buildings.jsonl` / `units.jsonl`. Plus the extras that have no place in `rf_core::World`:
//! DF's room evaluation of civzones, running punishments, animal products of the castes.
//!
//! Everything is optional: an export without these files loads with empty lists, and its units
//! get `pet_owner = pasture = -1` (serde's default 0 would be a unit / building id).

use super::{Manifest, Warnings};
use rf_core::{Crime, HaulRoute, KitchenSetting, Location, World, WorkDetail};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

/// The round-8 files of an export.
#[derive(Default)]
pub(crate) struct Round8Files {
    pub work_details: Vec<WorkDetail>,
    pub kitchen: Vec<KitchenSetting>,
    pub locations: Vec<Location>,
    pub crimes: Vec<Crime>,
    pub haul_routes: Vec<HaulRoute>,
    /// `work_details.json` existed (the export has round-8 data).
    pub present: bool,
}

pub(crate) fn load_files(dir: &Path, w: &mut Warnings) -> Round8Files {
    fn opt<T: DeserializeOwned + Default>(dir: &Path, name: &str, kind: &'static str, w: &mut Warnings) -> T {
        let p = dir.join(name);
        if !p.exists() {
            return T::default();
        }
        match super::read_json::<T>(&p, kind, false) {
            Ok((v, ww)) => {
                w.merge(ww);
                v
            }
            Err(_) => T::default(),
        }
    }
    Round8Files {
        present: dir.join("work_details.json").exists(),
        work_details: opt(dir, "work_details.json", "work_details", w),
        kitchen: opt(dir, "kitchen.json", "kitchen", w),
        locations: opt(dir, "locations.json", "locations", w),
        crimes: opt(dir, "crimes.json", "crimes", w),
        haul_routes: opt(dir, "haul_routes.json", "haul_routes", w),
    }
}

/// Total number of stockpile filter strings (the manifest's `stockpile_filters`).
pub(crate) fn filter_count(world: &World) -> usize {
    world.buildings.iter().filter_map(|b| b.stockpile.as_ref()).map(|s| s.filters.len()).sum()
}

/// Compare the manifest's round-8 counts with what was loaded (before any repair).
pub(crate) fn check_counts(world: &World, m: &Manifest, w: &mut Warnings) {
    let units = &world.units;
    for (key, got) in [
        ("work_details", world.work_details.len()),
        ("kitchen", world.kitchen.len()),
        ("locations", world.locations.len()),
        ("occupations", world.locations.iter().map(|l| l.occupations.len()).sum()),
        ("crimes", world.crimes.len()),
        ("haul_routes", world.haul_routes.len()),
        ("stockpile_filters", filter_count(world)),
        ("pets", units.iter().filter(|u| u.pet_owner >= 0).count()),
        ("pastured", units.iter().filter(|u| u.pasture >= 0).count()),
        ("slaughter", units.iter().filter(|u| u.slaughter).count()),
        ("pregnant", units.iter().filter(|u| u.pregnancy > 0).count()),
    ] {
        if let Some(want) = m.count(key) {
            if want != got as i64 {
                w.add("counts", format!("{key}: manifest says {want}, imported {got}"));
            }
        }
    }
}

/// Round-8 repairs. Without round-8 data the units get `pet_owner = pasture = -1`. With it:
/// work-detail members, pet owners and occupation holders must be exported units; pastures,
/// pile links, location zones and haul-route stops must be existing buildings; filter strings
/// must have three parts; `Unit::work_details` must match the details' member lists; kitchen
/// entries are unique per item type/subtype/material.
pub(crate) fn repair(world: &mut World, present: bool, w: &mut Warnings) {
    if !present {
        for u in &mut world.units {
            u.pet_owner = -1;
            u.pasture = -1;
        }
        return;
    }
    let unit_ids: HashSet<i32> = world.units.iter().map(|u| u.id).collect();
    let bld_kind: HashMap<i32, String> = world.buildings.iter().map(|b| (b.id, b.kind.clone())).collect();
    let item_ids: HashSet<i32> = world.items.iter().map(|i| i.id).collect();

    // work details: members must be exported units (DF keeps dead members listed for a while)
    let mut dropped = 0usize;
    for d in &mut world.work_details {
        let before = d.assigned.len();
        let mut seen = BTreeSet::new();
        d.assigned.retain(|u| unit_ids.contains(u) && seen.insert(*u));
        dropped += before - d.assigned.len();
    }
    if dropped > 0 {
        w.add("work_details", format!("{dropped} work detail members are not among the exported units (dead or gone); dropped"));
    }
    // Unit::work_details = the names of the details that list the unit, in detail order
    let mut names: HashMap<i32, Vec<String>> = HashMap::new();
    for d in &world.work_details {
        for &u in &d.assigned {
            names.entry(u).or_default().push(d.name.clone());
        }
    }
    let mut fixed = 0usize;
    for u in &mut world.units {
        let want = names.remove(&u.id).unwrap_or_default();
        if u.work_details != want {
            u.work_details = want;
            fixed += 1;
        }
    }
    if fixed > 0 {
        w.add("work_details", format!("{fixed} units' work_details did not match the work detail member lists; rebuilt"));
    }

    // animals: owners must be exported units, pastures existing civzones
    let (mut bad_owner, mut bad_pasture) = (0usize, 0usize);
    for u in &mut world.units {
        // 0 without such a unit/zone is `Unit::default()` (worlds built in code), not a DF id: no warning
        if u.pet_owner >= 0 && !unit_ids.contains(&u.pet_owner) {
            bad_owner += (u.pet_owner != 0) as usize;
            u.pet_owner = -1;
        }
        if u.pasture >= 0 && bld_kind.get(&u.pasture).map(|k| k.as_str()) != Some("Civzone") {
            bad_pasture += (u.pasture != 0) as usize;
            u.pasture = -1;
        }
        if u.pregnancy < 0 {
            u.pregnancy = 0;
        }
    }
    if bad_owner > 0 {
        w.add("units", format!("{bad_owner} pet owners are not among the exported units; set to -1"));
    }
    if bad_pasture > 0 {
        w.add("units", format!("{bad_pasture} pasture assignments point at no civzone; set to -1"));
    }

    // stockpiles: filter strings `category/group/token`, links to existing buildings
    let (mut bad_filter, mut bad_link) = (0usize, 0usize);
    for b in &mut world.buildings {
        let id = b.id;
        let Some(s) = b.stockpile.as_mut() else { continue };
        let before = s.filters.len();
        s.filters.retain(|f| f.splitn(3, '/').count() == 3 && !f.contains("//"));
        bad_filter += before - s.filters.len();
        for list in [&mut s.give_to, &mut s.take_from] {
            let before = list.len();
            let mut seen = BTreeSet::new();
            list.retain(|l| *l != id && bld_kind.contains_key(l) && seen.insert(*l));
            bad_link += before - list.len();
        }
    }
    if bad_filter > 0 {
        w.add("buildings", format!("{bad_filter} malformed stockpile filter strings dropped (expected category/group/token)"));
    }
    if bad_link > 0 {
        w.add("buildings", format!("{bad_link} stockpile links to missing buildings (or duplicates/self) dropped"));
    }

    // locations: zones must be existing buildings, occupation holders exported units
    let (mut bad_zone, mut gone_holder) = (0usize, 0usize);
    for l in &mut world.locations {
        let before = l.zones.len();
        let mut seen = BTreeSet::new();
        l.zones.retain(|z| bld_kind.contains_key(z) && seen.insert(*z));
        bad_zone += before - l.zones.len();
        for o in &mut l.occupations {
            if o.unit >= 0 && !unit_ids.contains(&o.unit) {
                o.unit = -1;
                gone_holder += 1;
            }
        }
    }
    if bad_zone > 0 {
        w.add("locations", format!("{bad_zone} location zones are not among the exported buildings; dropped"));
    }
    if gone_holder > 0 {
        w.add("locations", format!("{gone_holder} occupation holders are not among the exported units; now vacant"));
    }

    // hauling routes: stop links and vehicles
    let (mut bad_stop, mut bad_vehicle) = (0usize, 0usize);
    for r in &mut world.haul_routes {
        for s in &mut r.stops {
            for list in [&mut s.take_from, &mut s.give_to] {
                let before = list.len();
                list.retain(|l| bld_kind.contains_key(l));
                bad_stop += before - list.len();
            }
        }
        let before = r.vehicles.len();
        r.vehicles.retain(|v| item_ids.contains(v));
        bad_vehicle += before - r.vehicles.len();
    }
    if bad_stop + bad_vehicle > 0 {
        w.add("haul_routes", format!("{bad_stop} stop links to missing buildings and {bad_vehicle} missing vehicles dropped"));
    }

    // kitchen: one entry per (item type, subtype, material); flags of duplicates are merged
    let mut idx: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    let mut merged: Vec<KitchenSetting> = Vec::with_capacity(world.kitchen.len());
    let mut dups = 0usize;
    for k in std::mem::take(&mut world.kitchen) {
        let key = (k.item_type.clone(), k.item_subtype.clone(), k.mat.clone());
        match idx.get(&key) {
            Some(&i) => {
                merged[i].no_cook |= k.no_cook;
                merged[i].no_brew |= k.no_brew;
                dups += 1;
            }
            None => {
                idx.insert(key, merged.len());
                merged.push(k);
            }
        }
    }
    world.kitchen = merged;
    if dups > 0 {
        w.add("kitchen", format!("{dups} duplicate kitchen entries merged"));
    }
}

// ------------------------------------------------------------------------------------- extras

/// DF's room evaluation of a civzone (extras of `buildings.jsonl`, round 8).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct RoomEval {
    /// Civzone building id and type (`Bedroom`, `MeetingHall`, `Tomb`, ...).
    pub id: i32,
    pub subtype: String,
    pub owner: i32,
    /// DF's value: the location's `contents.location_value` for a location's meeting-area zone,
    /// else -1 (DF 53.16 keeps no value for personal rooms).
    pub room_value: i32,
    /// DF's name: `dfhack.buildings.getRoomDescription` when it gives one (empty in DFHack
    /// 53.16-r2 for v50 zones), else the location's tier name (`Temple`, `Temple Complex`,
    /// `Meeting Hall`, `Guildhall`, `Grand Guildhall`, `Tavern`, `Library`, `Hospital`).
    pub room_name: String,
    pub room_description: String,
    pub location_id: i32,
    pub location_kind: String,
    pub location_tier: i32,
    /// The owner's latest room quality thought (`BedroomQuality`, `DiningQuality`, `OfficeQuality`,
    /// `TombQuality`) and its severity: DF's own verdict on personal rooms. Empty / -1 if none.
    pub room_thought: String,
    pub room_thought_severity: i32,
    pub room_thought_year: i32,
    pub room_thought_tick: i32,
    pub room_collision: bool,
}

/// A punishment DF is carrying out (`punishments.json`, `plotinfo.punishments`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PunishmentInfo {
    pub criminal: i32,
    pub officer: i32,
    pub beating: i32,
    pub hammerstrikes: i32,
    /// Remaining jail time in ticks.
    pub prison_counter: i32,
    pub time_to_assign: i32,
    pub chain: i32,
    pub victims: Vec<i32>,
}

/// Milk of a caste (`creature_raws.json` caste extra `milkable`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct MilkInfo {
    /// Material token, e.g. `CREATURE:COW:MILK`.
    pub mat: String,
    /// `[MILKABLE:mat:frequency]` in ticks.
    pub frequency: i32,
}

/// One `[SHEARABLE_TISSUE_LAYER]` of a caste.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ShearInfo {
    pub length: i32,
    /// Body part tokens that carry the layer.
    pub parts: Vec<String>,
}

/// Husbandry facts of a caste (extras of `creature_raws.json`, round 8). The caste flags
/// (`MILKABLE`, `GRAZER`, `LAYS_EGGS`, pseudo-flag `SHEARABLE`) are in `rf_core::CasteRaw::flags`.
#[derive(Clone, Debug, Default)]
pub struct AnimalProducts {
    pub race: String,
    pub caste: String,
    pub milkable: Option<MilkInfo>,
    /// `[GRAZER:n]`, -1 = not a grazer.
    pub grazer: i32,
    pub shearable: Vec<ShearInfo>,
    pub egg_materials: Vec<String>,
    /// [min, max] eggs per clutch / young per birth.
    pub clutch_size: [i32; 2],
    pub litter_size: [i32; 2],
    pub egg_size: i32,
    pub pet_value: i32,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct RawCasteProducts {
    token: String,
    flags: Vec<String>,
    milkable: Option<MilkInfo>,
    grazer: i32,
    shearable: Vec<ShearInfo>,
    egg_materials: Vec<String>,
    clutch_size: Vec<i32>,
    litter_size: Vec<i32>,
    egg_size: i32,
    pet_value: i32,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct RawCreatureProducts {
    token: String,
    castes: Vec<RawCasteProducts>,
}

fn pair(v: &[i32]) -> [i32; 2] {
    [v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0)]
}

/// Room evaluations of every civzone in `buildings.jsonl` (only round-8 exports carry them).
pub(crate) fn load_room_evals(dir: &Path) -> Vec<RoomEval> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(default)]
        kind: String,
        #[serde(flatten)]
        eval: RoomEval,
    }
    let Ok(text) = std::fs::read_to_string(dir.join("buildings.jsonl")) else { return Vec::new() };
    super::strip_bom(&text)
        .lines()
        .filter(|l| l.contains("\"room_value\""))
        .filter_map(|l| serde_json::from_str::<Line>(l).ok())
        .filter(|l| l.kind == "Civzone")
        .map(|l| l.eval)
        .collect()
}

pub(crate) fn load_punishments(dir: &Path) -> Vec<PunishmentInfo> {
    std::fs::read_to_string(dir.join("punishments.json"))
        .ok()
        .and_then(|t| serde_json::from_str(super::strip_bom(&t)).ok())
        .unwrap_or_default()
}

pub(crate) fn load_animal_products(dir: &Path) -> Vec<AnimalProducts> {
    let raws: Vec<RawCreatureProducts> = std::fs::read_to_string(dir.join("creature_raws.json"))
        .ok()
        .and_then(|t| serde_json::from_str(super::strip_bom(&t)).ok())
        .unwrap_or_default();
    let mut out = Vec::new();
    for c in raws {
        for k in c.castes {
            let grazes = k.grazer > 0 && k.flags.iter().any(|f| f == "GRAZER");
            if k.milkable.is_none() && !grazes && k.shearable.is_empty() && k.egg_materials.is_empty() {
                continue;
            }
            out.push(AnimalProducts {
                race: c.token.clone(),
                caste: k.token,
                milkable: k.milkable,
                // DF fills misc.grazer for every caste; it only matters with the GRAZER flag
                grazer: if k.grazer > 0 && k.flags.iter().any(|f| f == "GRAZER") { k.grazer } else { -1 },
                shearable: k.shearable,
                egg_materials: k.egg_materials,
                clutch_size: pair(&k.clutch_size),
                litter_size: pair(&k.litter_size),
                egg_size: k.egg_size,
                pet_value: k.pet_value,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{load_export, load_extras, write_export};
    use rf_core::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rf-import-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn zone(id: i32, subtype: &str) -> Building {
        Building { id, kind: "Civzone".into(), subtype: subtype.into(), x1: 2, y1: 2, x2: 4, y2: 4, z: 2, built: true, owner: -1, ..Default::default() }
    }

    /// A synthetic world with round-8 data and a few broken references.
    fn round8_world() -> World {
        let mut w = World::synthetic(24, 24, 8);
        let (a, b) = (w.units[0].id, w.units[1].id);
        let cat = w.units.iter().find(|u| u.flags.animal).unwrap().id;
        let vehicle = w.items[0].id;
        w.buildings.push(zone(700, "Pen"));
        w.buildings.push(zone(701, "MeetingHall"));
        w.buildings.push(Building {
            id: 702,
            kind: "Stockpile".into(),
            x1: 5,
            y1: 5,
            x2: 6,
            y2: 6,
            z: 2,
            built: true,
            owner: -1,
            stockpile: Some(StockpileSettings {
                categories: vec!["food".into(), "stone".into()],
                filters: vec![
                    "food/options/prepared_meals".into(),
                    "food/meat/*".into(),
                    "food/meat/!CREATURE:COW:FAT".into(),
                    "food/plants/PLANT:MUSHROOM_HELMET_PLUMP:STRUCTURAL".into(),
                    "stone/mats/INORGANIC:HEMATITE".into(),
                    "broken".into(),
                    "misc/options/allow_organic".into(),
                ],
                give_to: vec![703, 9999, 702],
                take_from: vec![703],
                use_links_only: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        w.buildings.push(Building { id: 703, kind: "Stockpile".into(), z: 2, stockpile: Some(StockpileSettings::default()), owner: -1, ..Default::default() });
        w.work_details = vec![
            WorkDetail { name: "Miners".into(), labors: vec!["MINE".into()], assigned: vec![a, 424242, a], mode: "OnlySelectedDoesThis".into(), icon: "0".into(), no_other_work: false },
            WorkDetail { name: "Haulers".into(), labors: vec!["HAUL_STONE".into()], assigned: vec![a, b], mode: "EverybodyDoesThis".into(), ..Default::default() },
        ];
        w.kitchen = vec![
            KitchenSetting { item_type: "PLANT".into(), mat: "PLANT:MUSHROOM_HELMET_PLUMP:STRUCTURAL".into(), no_cook: true, ..Default::default() },
            KitchenSetting { item_type: "PLANT".into(), mat: "PLANT:MUSHROOM_HELMET_PLUMP:STRUCTURAL".into(), no_brew: true, ..Default::default() },
            KitchenSetting { item_type: "SEEDS".into(), mat: "PLANT:BARLEY:SEED".into(), no_cook: true, ..Default::default() },
        ];
        w.locations = vec![Location {
            id: 14,
            kind: "Tavern".into(),
            name: "The Confusing Pick".into(),
            deity: -1,
            zones: vec![701, 8888],
            occupations: vec![Occupation { kind: "TAVERN_KEEPER".into(), unit: a }, Occupation { kind: "PERFORMER".into(), unit: 31337 }],
            allow_visitors: true,
            ..Default::default()
        }];
        w.crimes = vec![Crime {
            id: 1546,
            kind: "ProductionOrderViolation".into(),
            criminal: a,
            victim: b,
            site_tick: 1200,
            year: 280,
            convicted: a,
            punishment: "Hammerstrikes 4, Jail 2".into(),
            witnesses: vec![b],
        }];
        w.haul_routes = vec![HaulRoute {
            id: 1,
            name: "Stone quantum".into(),
            stops: vec![HaulStop {
                name: "Stop 1".into(),
                pos: Pos::new(5, 5, 2),
                take_from: vec![702, 5555],
                give_to: vec![],
                conditions: vec!["Push North when >= 100% full".into()],
            }],
            vehicles: vec![vehicle, 7777777],
        }];
        for u in &mut w.units {
            u.pet_owner = -1;
            u.pasture = -1;
        }
        let u = w.units.iter_mut().find(|u| u.id == cat).unwrap();
        u.pet_owner = a;
        u.pasture = 700;
        u.training = "Domesticated".into();
        u.slaughter = true;
        u.pregnancy = 3000;
        let u = w.units.iter_mut().find(|u| u.id == b).unwrap();
        u.pet_owner = 123456; // dead owner
        u.pasture = 702; // not a civzone
        w
    }

    #[test]
    fn round8_files_load_and_are_repaired() {
        let w = round8_world();
        let (a, b) = (w.units[0].id, w.units[1].id);
        let cat = w.units.iter().find(|u| u.flags.animal).unwrap().id;
        let dir = temp_dir("round8");
        write_export(&w, &dir).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        assert!(rep.round8);
        assert_eq!((rep.work_details, rep.locations, rep.crimes, rep.haul_routes), (2, 1, 1, 1));
        // work details: unknown and duplicate members dropped, unit names rebuilt in detail order
        assert_eq!(got.work_details[0].assigned, vec![a]);
        assert_eq!(got.unit(a).unwrap().work_details, vec!["Miners".to_string(), "Haulers".to_string()]);
        assert_eq!(got.unit(b).unwrap().work_details, vec!["Haulers".to_string()]);
        // kitchen duplicates merged
        assert_eq!(got.kitchen.len(), 2);
        assert!(got.kitchen[0].no_cook && got.kitchen[0].no_brew);
        // pile: malformed filter dropped, links to missing/self dropped
        let s = got.building(702).unwrap().stockpile.as_ref().unwrap();
        assert_eq!(s.filters.len(), 6);
        assert_eq!((s.give_to.clone(), s.take_from.clone(), s.use_links_only), (vec![703], vec![703], true));
        assert_eq!(rep.stockpile_filters, 6);
        // location zone and holder repairs
        assert_eq!(got.locations[0].zones, vec![701]);
        assert_eq!((got.locations[0].occupations[0].unit, got.locations[0].occupations[1].unit), (a, -1));
        // animals
        let c = got.unit(cat).unwrap();
        assert_eq!((c.pet_owner, c.pasture, c.training.as_str(), c.slaughter, c.pregnancy), (a, 700, "Domesticated", true, 3000));
        let u = got.unit(b).unwrap();
        assert_eq!((u.pet_owner, u.pasture), (-1, -1));
        // crimes and routes
        assert_eq!(got.crimes[0].punishment, "Hammerstrikes 4, Jail 2");
        assert_eq!(got.haul_routes[0].stops[0].take_from, vec![702]);
        assert_eq!(got.haul_routes[0].vehicles.len(), 1);
        let all = rep.warnings.join("\n");
        for frag in [
            "work detail members are not among the exported units",
            "work_details did not match",
            "duplicate kitchen entries merged",
            "malformed stockpile filter",
            "stockpile links to missing buildings",
            "location zones are not among",
            "occupation holders are not among",
            "pet owners are not among",
            "pasture assignments point at no civzone",
            "missing vehicles dropped",
        ] {
            assert!(all.contains(frag), "{frag}: {all}");
        }
        assert!(!all.contains("counts:"), "{all}");
        // a second round trip is clean
        let dir2 = temp_dir("round8b");
        write_export(&got, &dir2).unwrap();
        let (again, rep2) = load_export(&dir2).unwrap();
        assert!(rep2.warnings.is_empty(), "{:?}", rep2.warnings);
        assert_eq!(again.unit(cat).unwrap().pasture, 700);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn export_without_round8_files_has_no_owners_or_pastures() {
        let w = World::synthetic(16, 16, 6);
        let dir = temp_dir("noround8");
        write_export(&w, &dir).unwrap();
        for f in ["work_details.json", "kitchen.json", "locations.json", "crimes.json", "haul_routes.json"] {
            std::fs::remove_file(dir.join(f)).unwrap();
        }
        let (got, rep) = load_export(&dir).unwrap();
        assert!(!rep.round8);
        assert!(got.work_details.is_empty() && got.kitchen.is_empty() && got.locations.is_empty() && got.crimes.is_empty());
        assert!(got.units.iter().all(|u| u.pet_owner == -1 && u.pasture == -1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stockpile_filter_semantics() {
        let w = round8_world();
        let s = w.buildings.iter().find(|b| b.id == 702).unwrap().stockpile.clone().unwrap();
        assert!(s.filter_allows("food", "meat", "CREATURE:COW:MUSCLE"));
        assert!(!s.filter_allows("food", "meat", "CREATURE:COW:FAT"));
        assert!(s.filter_allows("food", "plants", "PLANT:MUSHROOM_HELMET_PLUMP:STRUCTURAL"));
        assert!(!s.filter_allows("food", "plants", "PLANT:CAVE_WHEAT:STRUCTURAL"));
        assert!(!s.filter_allows("food", "seeds", "PLANT:BARLEY:SEED"));
        assert!(s.filter_allows("stone", "mats", "INORGANIC:HEMATITE") && !s.filter_allows("stone", "mats", "INORGANIC:GRANITE"));
        assert!(!s.filter_allows("wood", "mats", "PLANT:TOWER_CAP:WOOD"), "category off");
        assert!(s.filter_option("food", "prepared_meals"));
        assert!(s.filter_option("misc", "allow_organic") && !s.filter_option("misc", "allow_inorganic"));
        // a pile without filter strings accepts everything in its categories (pre-round-8 exports)
        let plain = StockpileSettings { categories: vec!["wood".into()], ..Default::default() };
        assert!(plain.filter_allows("wood", "mats", "PLANT:TOWER_CAP:WOOD") && plain.filter_option("misc", "allow_inorganic"));
        // an enabled category with nothing enabled
        let none = StockpileSettings { categories: vec!["gems".into()], filters: vec!["gems/none/-".into()], ..Default::default() };
        assert!(!none.filter_allows("gems", "rough_mats", "INORGANIC:MICROCLINE"));
    }

    #[test]
    fn exporter_style_round8_lines_parse() {
        // As the Lua exporter writes them (rev 2026-10-10e), extras included.
        let u: Unit = serde_json::from_str(r#"{"id":19241,"race":"BIRD_TURKEY","caste":"FEMALE","flags":{"animal":true,"tame":true},"pet_owner":19239,"training":"Domesticated","pasture":-1,"slaughter":false,"pregnancy":0,"work_details":[],"gelded":false,"only_do_assigned_jobs":false,"pregnancy_caste":-1,"pregnancy_spouse":-1}"#).unwrap();
        assert_eq!((u.pet_owner, u.training.as_str(), u.pasture), (19239, "Domesticated", -1));
        let c: Unit = serde_json::from_str(r#"{"id":14028,"race":"DWARF","work_details":["Miners","Haulers"],"pregnancy":120000,"pet_owner":-1,"pasture":-1}"#).unwrap();
        assert_eq!((c.work_details.len(), c.pregnancy), (2, 120000));
        let b: Building = serde_json::from_str(r#"{"id":10605,"kind":"Stockpile","subtype":"","x1":1,"y1":1,"x2":2,"y2":2,"z":131,"stockpile":{"categories":["food"],"max_barrels":2,"max_bins":0,"max_wheelbarrows":0,"filters":["food/options/prepared_meals","food/meat/*","misc/options/allow_organic","misc/options/allow_inorganic"],"use_links_only":false,"give_to":[12219],"take_from":[],"give_to_pile":[12219],"take_from_pile":[],"give_to_workshop":[],"take_from_workshop":[],"stockpile_number":3,"filters_complete":true},"flags":[],"links":[],"direction":""}"#).unwrap();
        let s = b.stockpile.unwrap();
        assert_eq!((s.filters.len(), s.give_to.clone()), (4, vec![12219]));
        let z: Building = serde_json::from_str(r#"{"id":12075,"kind":"Civzone","subtype":"MeetingHall","owner":-1,"room_value":11604,"room_name":"Temple Complex","room_description":"","location_id":7,"location_kind":"Temple","location_tier":0,"room_thought":"","room_thought_severity":-1,"room_thought_year":-1,"room_thought_tick":-1,"room_collision":false}"#).unwrap();
        assert_eq!(z.subtype, "MeetingHall");
        let d: WorkDetail = serde_json::from_str(r#"{"name":"Miners","labors":["MINE"],"assigned":[13831,13832],"mode":"OnlySelectedDoesThis","icon":"0","no_other_work":false,"index":0,"no_modify":true,"cannot_be_everybody":true}"#).unwrap();
        assert_eq!((d.assigned.len(), d.mode.as_str()), (2, "OnlySelectedDoesThis"));
        let k: KitchenSetting = serde_json::from_str(r#"{"item_type":"SEEDS","item_subtype":"","mat":"PLANT:BARLEY:SEED","no_cook":true,"no_brew":false,"mat_type":423,"mat_index":5,"item_subtype_index":-1}"#).unwrap();
        assert!(k.no_cook && !k.no_brew);
        let l: Location = serde_json::from_str(r#"{"id":5,"kind":"Temple","name":"The Grand Drool","deity":42,"profession":"","zones":[12066],"occupations":[],"allow_visitors":true,"type":"TEMPLE","name_native":"x","religion":-1,"deity_type":0,"flags":["VISITORS_ALLOWED","NON_CITIZENS_ALLOWED"],"non_citizens_allowed":true,"members_only":false,"exists":true,"location_value":8534,"location_tier":0,"tier_name":"Temple","supplies":[],"occupations_detail":[]}"#).unwrap();
        assert_eq!((l.deity, l.zones.clone()), (42, vec![12066]));
        let g: Location = serde_json::from_str(r#"{"id":16,"kind":"Hospital","name":"The Hopeful Blackness","deity":-1,"profession":"","zones":[14276],"occupations":[{"kind":"BONE_DOCTOR","unit":14100},{"kind":"SURGEON","unit":18447}],"allow_visitors":true}"#).unwrap();
        assert_eq!(g.occupations[0].unit, 14100);
        let cr: Crime = serde_json::from_str(r#"{"id":1219,"kind":"ProductionOrderViolation","criminal":13937,"victim":13959,"site_tick":1940,"year":215,"convicted":13937,"punishment":"Hammerstrikes 1, Jail 3","witnesses":[],"accused":13937,"criminal_hf":3893,"victim_hf":3900,"criminal_name":"Amost Castlelancers","victim_name":"Fath Painttunnels","flags":["sentenced","discovered"],"hammerstrikes":1,"prison_time":3,"punishment_flags":[],"discovered_year":215,"discovered_tick":1940,"entity":303,"item_id":-1,"incident_id":-1}"#).unwrap();
        assert_eq!((cr.convicted, cr.year), (13937, 215));
        let r: HaulRoute = serde_json::from_str(r#"{"id":0,"name":"Stone quantum","stops":[{"name":"Stop 1","pos":{"x":1,"y":2,"z":3},"take_from":[5],"give_to":[],"conditions":["Push North when >= 100% full"],"id":1,"cart_id":-1,"time_waiting":0,"links":[{"building_id":5,"mode":["take"]}]}],"vehicles":[77],"vehicle_stops":[0]}"#).unwrap();
        assert_eq!((r.stops[0].take_from.clone(), r.vehicles.clone()), (vec![5], vec![77]));
    }

    #[test]
    fn extras_read_rooms_punishments_and_animal_products() {
        let dir = temp_dir("extras8");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("buildings.jsonl"),
            concat!(
                r#"{"id":10722,"kind":"Civzone","subtype":"Bedroom","owner":18139,"room_value":-1,"room_name":"","room_description":"","location_id":-1,"location_kind":"","location_tier":-1,"room_thought":"BedroomQuality","room_thought_severity":3,"room_thought_year":280,"room_thought_tick":119658,"room_collision":false}"#,
                "\n",
                r#"{"id":12075,"kind":"Civzone","subtype":"MeetingHall","owner":-1,"room_value":11604,"room_name":"Temple Complex","location_id":7,"location_kind":"Temple","location_tier":0,"room_thought":"","room_thought_severity":-1}"#,
                "\n",
                r#"{"id":10568,"kind":"Stockpile","stockpile":{"categories":["food"]}}"#,
                "\n"
            ),
        )
        .unwrap();
        std::fs::write(dir.join("punishments.json"), r#"[{"criminal":18126,"officer":-1,"beating":0,"hammerstrikes":0,"prison_counter":6720,"time_to_assign":2363,"chain":-1,"victims":[14151]}]"#).unwrap();
        std::fs::write(
            dir.join("creature_raws.json"),
            r#"[{"token":"SHEEP","castes":[{"token":"FEMALE","flags":["MILKABLE","GRAZER","SHEARABLE"],"milkable":{"mat":"CREATURE:SHEEP:MILK","frequency":20000},"grazer":3363,"shearable":[{"length":300,"parts":["UB","LB"]}],"egg_materials":[],"clutch_size":[1,3],"litter_size":[1,3],"egg_size":50,"pet_value":100}]},
                {"token":"DOG","castes":[{"token":"MALE","flags":["PET"],"milkable":null,"grazer":-1,"shearable":[],"egg_materials":[]}]},
                {"token":"CAT","castes":[{"token":"MALE","flags":["PET"],"grazer":120}]}]"#,
        )
        .unwrap();
        let x = load_extras(&dir);
        assert_eq!(x.rooms.len(), 2);
        assert_eq!((x.rooms[0].room_thought.as_str(), x.rooms[0].room_thought_severity, x.rooms[0].owner), ("BedroomQuality", 3, 18139));
        assert_eq!((x.rooms[1].room_value, x.rooms[1].room_name.as_str()), (11604, "Temple Complex"));
        assert_eq!(x.punishments[0].prison_counter, 6720);
        assert_eq!(x.animal_products.len(), 1, "only the sheep has products (a grazer value without the GRAZER flag does not count)");
        let p = &x.animal_products[0];
        assert_eq!((p.race.as_str(), p.grazer, p.shearable[0].length, p.milkable.as_ref().unwrap().frequency), ("SHEEP", 3363, 300, 20000));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
