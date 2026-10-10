//! [chaos10] Shared chaos/fuzz harness: random but valid player actions through the public
//! `Sim` API, UI-style queries, invariant checks and save/load round trips.
//!
//! Used by `examples/chaos.rs` (long runs, many seeds, all cores) and `tests/chaos.rs` (short
//! runs plus one regression test per crash found). Everything here is deterministic: the
//! action stream depends only on the chaos seed and the sim state, so a failure is reproduced
//! by its (world, sim seed, chaos seed) triple.

#![allow(dead_code)]

use rf_core::{KitchenSetting, Pos, SquadAmmo, StockpileSettings, World};
use rf_sim::{ConstructionKind, DesignationKind, DetailKind, RaidKind, Sim, SquadOrder, Traffic, VisitorRole};
use std::collections::{HashMap, HashSet};

/// splitmix64; independent from the sim's own RNG streams.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1).max(1) as usize) as i32
    }
    pub fn chance(&mut self, pct: u32) -> bool {
        (self.next() % 100) < pct as u64
    }
    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        if v.is_empty() {
            None
        } else {
            Some(&v[self.below(v.len())])
        }
    }
}

const LABORS: &[&str] = &[
    "MINE", "CUTWOOD", "HERBALISM", "BREWER", "COOK", "PLANT", "CARPENTER", "MASON", "STONE_CRAFT", "WOOD_CRAFT",
    "PROCESS_PLANT", "MILLER", "BUILD_CONSTRUCTION", "MECHANIC", "BURN_WOOD", "SMELT", "FORGE_WEAPON", "FORGE_ARMOR",
    "FORGE_FURNITURE", "METAL_CRAFT", "GLASSMAKER", "BUTCHER", "TANNER", "LEATHER", "WEAVER", "CLOTHESMAKER",
    "CLEAN_FISH", "BONE_CARVE", "ENGRAVER", "SHEARER", "SPINNER", "MILK", "MAKE_CHEESE", "HUNT", "FISH", "ANIMALTRAIN",
    "HAUL_STONE", "HAUL_WOOD", "HAUL_FOOD", "HAUL_ITEM", "HAUL_REFUSE", "HAUL_FURNITURE", "HAUL_BODY", "HAUL_ANIMALS",
    "DIAGNOSE", "SURGERY", "BONE_SETTING", "SUTURING", "DRESSING_WOUNDS", "FEED_WATER_CIVILIANS", "RECOVER_WOUNDED",
];
const STOCK_CATS: &[&str] = &[
    "animals", "food", "furniture", "corpses", "refuse", "stone", "ammo", "coins", "bars_blocks", "gems", "finished_goods",
    "leather", "cloth", "wood", "weapons", "armor", "sheet",
];
const ZONES: &[&str] = &[
    "MeetingHall", "Bedroom", "DiningHall", "WaterSource", "Dump", "Pen", "Tomb", "Barracks", "Hospital", "Office",
    "Dormitory", "FishingArea", "PlantGathering", "Pond", "Temple", "Tavern", "Library",
];
const LOC_KINDS: &[&str] = &["Temple", "Tavern", "Library", "Guildhall", "Hospital"];
const OCCUPATIONS: &[&str] = &["TavernKeeper", "Performer", "Priest", "Scholar", "Scribe", "Doctor", "Diagnostician", "Surgeon", "BoneDoctor", "Messenger"];
const UNIFORM_ITEMS: &[&str] = &["WEAPON", "ARMOR", "HELM", "SHOES", "GLOVES", "PANTS", "SHIELD", "QUIVER", "FLASK", "BACKPACK", "WEAPON:ITEM_WEAPON_AXE_BATTLE", "WEAPON:ITEM_WEAPON_CROSSBOW", "ARMOR:ITEM_ARMOR_MAIL_SHIRT"];
const NOBLES: &[&str] = &["MANAGER", "BOOKKEEPER", "BROKER", "CHIEF_MEDICAL_DWARF", "SHERIFF", "CAPTAIN_OF_THE_GUARD", "MILITIA_COMMANDER", "MILITIA_CAPTAIN", "EXPEDITION_LEADER", "MAYOR", "HAMMERER", "BARON"];
const MOOD_KINDS: &[&str] = &["Fey", "Secretive", "Possessed", "Macabre", "Fell", "fey", "unknown"];
const ITEM_FLAGS: &[&str] = &["forbid", "claim", "dump", "melt", "hidden"];
const BUILDING_FLAGS: &[&str] = &["FORBIDDEN", "LOCKED", "PET_PASSABLE", "CITIZENS", "OPEN", "CLOSED", "forbid", "nonsense"];
const TRAINING: &[&str] = &["War", "Hunting", "Tame", "None", ""];
const FREQ: &[&str] = &["OneTime", "Daily", "Monthly", "Seasonally", "Yearly"];
const WD_MODES: &[&str] = &["EverybodyDoesThis", "OnlySelectedDoesThis", "NobodyDoesThis", "bogus"];

/// What a chaos run did, for the report.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub steps: u64,
    pub actions: u64,
    pub saves: u64,
    pub ticks: u64,
    pub citizens_start: usize,
    pub citizens_end: usize,
    pub per_action: HashMap<&'static str, u64>,
    /// Citizen deaths by cause (from the death announcements).
    pub citizen_deaths: std::collections::BTreeMap<String, u64>,
    pub violations: Vec<String>,
    /// Steps whose ticks ran slower than `slow_tps` (step, ticks/s, stats, recent actions).
    pub slow: Vec<String>,
    pub sim_secs: f64,
}

/// One chaos driver over a sim.
pub struct Chaos {
    pub rng: Rng,
    /// Recent action log (ring of the last entries) for crash reports.
    pub log: Vec<String>,
    pub baseline: HashSet<String>,
    pub out: Outcome,
    /// Allow the actions that throw hostiles at the fort (sieges, beasts, raids).
    pub violent: bool,
    /// Steps below this speed are reported in `Outcome::slow`.
    pub slow_tps: f64,
    /// Player emulation: squads go Ready when hostiles appear.
    pub guard: bool,
    /// Keep the sim's own invasions off (and never toggle them back on).
    pub no_invasions: bool,
    alert_since: Option<u64>,
    /// Player emulation: lift a civilian alert after half a month.
    pub lift_alert: bool,
}

fn clamp_pos(w: &World, x: i32, y: i32, z: i32) -> Pos {
    Pos::new(
        x.clamp(0, w.dims.x as i32 - 1) as i16,
        y.clamp(0, w.dims.y as i32 - 1) as i16,
        z.clamp(0, w.dims.z as i32 - 1) as i16,
    )
}

impl Chaos {
    pub fn new(seed: u64) -> Chaos {
        Chaos { rng: Rng::new(seed), log: Vec::new(), baseline: HashSet::new(), out: Outcome::default(), violent: true, slow_tps: 0.0, guard: true, no_invasions: false, alert_since: None, lift_alert: true }
    }

    fn note(&mut self, tick: u64, what: &'static str, detail: String) {
        *self.out.per_action.entry(what).or_default() += 1;
        self.out.actions += 1;
        if self.log.len() >= 40 {
            self.log.remove(0);
        }
        self.log.push(format!("t{tick} {what} {detail}"));
    }

    /// A position near a random living citizen (or anywhere when there are none), where the
    /// player would actually click.
    pub fn near_pos(&mut self, sim: &Sim, spread: i32) -> Pos {
        let w = &sim.world;
        let cits: Vec<Pos> = w.units.iter().filter(|u| u.flags.citizen && !u.flags.dead && !u.flags.inactive && w.in_bounds(u.pos)).map(|u| u.pos).collect();
        match self.rng.pick(&cits).copied() {
            Some(c) if !self.rng.chance(10) => {
                let dz = if self.rng.chance(70) { 0 } else { self.rng.range(-2, 2) };
                clamp_pos(w, c.x as i32 + self.rng.range(-spread, spread), c.y as i32 + self.rng.range(-spread, spread), c.z as i32 + dz)
            }
            _ => {
                let (dx, dy, dz) = (w.dims.x as i32, w.dims.y as i32, w.dims.z as i32);
                clamp_pos(w, self.rng.range(0, dx - 1), self.rng.range(0, dy - 1), self.rng.range(0, dz - 1))
            }
        }
    }

    fn near_box(&mut self, sim: &Sim, spread: i32, size: i32, zs: i32) -> (Pos, Pos) {
        let a = self.near_pos(sim, spread);
        let b = clamp_pos(&sim.world, a.x as i32 + self.rng.range(-size, size), a.y as i32 + self.rng.range(-size, size), a.z as i32 + self.rng.range(0, zs));
        (a, b)
    }

    fn unit_id(&mut self, sim: &Sim, filter: impl Fn(&rf_core::Unit) -> bool) -> i32 {
        let ids: Vec<i32> = sim.world.units.iter().filter(|u| filter(u)).map(|u| u.id).collect();
        if self.rng.chance(3) {
            return [-1, 0, i32::MAX, 999_999][self.rng.below(4)];
        }
        self.rng.pick(&ids).copied().unwrap_or(-1)
    }

    fn citizen(&mut self, sim: &Sim) -> i32 {
        self.unit_id(sim, |u| u.flags.citizen && !u.flags.dead)
    }

    fn building_id(&mut self, sim: &Sim, filter: impl Fn(&rf_core::Building) -> bool) -> i32 {
        let ids: Vec<i32> = sim.world.buildings.iter().filter(|b| filter(b)).map(|b| b.id).collect();
        if self.rng.chance(3) {
            return [-1, 0, i32::MAX][self.rng.below(3)];
        }
        self.rng.pick(&ids).copied().unwrap_or(-1)
    }

    fn item_id(&mut self, sim: &Sim) -> i32 {
        let n = sim.world.items.len();
        if n == 0 || self.rng.chance(3) {
            return [-1, 0, i32::MAX][self.rng.below(3)];
        }
        sim.world.items[self.rng.below(n)].id
    }

    fn squad_id(&mut self, sim: &Sim) -> i32 {
        let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
        if self.rng.chance(3) {
            return -1;
        }
        self.rng.pick(&ids).copied().unwrap_or(-1)
    }

    /// Fire one random player action.
    pub fn act(&mut self, sim: &mut Sim) {
        let t = sim.ticks();
        let k = self.rng.below(100);
        match k {
            0..=11 => {
                let kinds = [
                    DesignationKind::Mine,
                    DesignationKind::Channel,
                    DesignationKind::UpDownStair,
                    DesignationKind::UpStair,
                    DesignationKind::DownStair,
                    DesignationKind::Ramp,
                    DesignationKind::ChopTrees,
                    DesignationKind::GatherPlants,
                    DesignationKind::Remove,
                ];
                let kind = kinds[self.rng.below(kinds.len())];
                let zs = if self.rng.chance(20) { 3 } else { 0 };
                let (a, b) = self.near_box(sim, 12, 5, zs);
                self.note(t, "designate", format!("{kind:?} {a:?} {b:?}"));
                sim.designate(a, b, kind);
            }
            12..=19 => {
                let opts = sim.build_options();
                let o = self.rng.below(opts.len());
                // Try a few spots like a player hovering the cursor.
                for _ in 0..12 {
                    let p = self.near_pos(sim, 10);
                    let b = clamp_pos(&sim.world, p.x as i32 + self.rng.range(0, 4), p.y as i32 + self.rng.range(0, 4), p.z as i32);
                    if sim.can_place(o, p, b).is_ok() {
                        let r = sim.place_building(o, p, b);
                        self.note(t, "place_building", format!("{} {p:?} {b:?} -> {r:?}", opts[o].label));
                        break;
                    }
                }
            }
            20..=22 => {
                // Remove a building the sim placed (or, rarely, one from the export).
                let fresh = self.rng.chance(70);
                let id = if fresh {
                    let ids: Vec<i32> = sim.world.buildings.iter().rev().take(30).map(|b| b.id).collect();
                    self.rng.pick(&ids).copied().unwrap_or(-1)
                } else {
                    self.building_id(sim, |_| true)
                };
                let kind = sim.world.building(id).map(|b| format!("{}/{}", b.kind, b.subtype)).unwrap_or_default();
                self.note(t, "remove_building", format!("{id} {kind}"));
                sim.remove_building(id);
            }
            23..=25 => {
                let (a, b) = self.near_box(sim, 10, 4, 0);
                let n = self.rng.range(0, 4) as usize;
                let cats: Vec<&str> = (0..n).map(|_| STOCK_CATS[self.rng.below(STOCK_CATS.len())]).collect();
                let id = sim.create_stockpile(a, b, &cats);
                self.note(t, "create_stockpile", format!("{a:?} {b:?} {cats:?} -> {id}"));
            }
            26..=27 => {
                let (a, b) = self.near_box(sim, 10, 4, 0);
                let z = ZONES[self.rng.below(ZONES.len())];
                let id = sim.create_zone(a, b, z);
                self.note(t, "create_zone", format!("{z} {a:?} {b:?} -> {id}"));
            }
            28 => {
                let (a, b) = self.near_box(sim, 10, 6, 2);
                let id = sim.create_burrow("chaos", a, b);
                self.note(t, "create_burrow", format!("{a:?} {b:?} -> {id}"));
            }
            29 => {
                let ids: Vec<i32> = sim.world.burrows.iter().map(|b| b.id).collect();
                let pick = if self.rng.chance(50) { None } else { self.rng.pick(&ids).copied().or(Some(12345)) };
                self.note(t, "civilian_alert", format!("{pick:?}"));
                sim.set_civilian_alert(pick);
                self.alert_since = pick.map(|_| t);
            }
            30 => {
                let zone = self.building_id(sim, |b| b.kind == "Civzone");
                let kind = LOC_KINDS[self.rng.below(LOC_KINDS.len())];
                let id = sim.create_location(kind, zone, "Chaos hall");
                let u = self.citizen(sim);
                let occ = OCCUPATIONS[self.rng.below(OCCUPATIONS.len())];
                sim.assign_occupation(id, occ, u);
                self.note(t, "create_location", format!("{kind} zone {zone} -> {id}, {occ} {u}"));
            }
            31..=33 => {
                let n = sim.order_templates().len();
                let tpl = self.rng.below(n + 2);
                let amount = [0, 1, 5, 20, -3, 1000][self.rng.below(6)];
                let f = FREQ[self.rng.below(FREQ.len())];
                let id = sim.add_work_order(tpl, amount, f);
                self.note(t, "add_work_order", format!("{tpl} x{amount} {f} -> {id}"));
            }
            34 => {
                let ids: Vec<i32> = sim.world.work_orders.iter().map(|o| o.id).collect();
                let id = self.rng.pick(&ids).copied().unwrap_or(-1);
                self.note(t, "remove_work_order", format!("{id}"));
                sim.remove_work_order(id);
            }
            35..=37 => {
                let ws: Vec<(i32, String)> = sim.world.buildings.iter().filter(|b| b.kind == "Workshop" || b.kind == "Furnace").map(|b| (b.id, b.subtype.clone())).collect();
                if let Some((id, sub)) = self.rng.pick(&ws).cloned() {
                    let tpls = sim.templates_for_workshop(&sub);
                    let tpl = self.rng.pick(&tpls).copied().unwrap_or(self.rng.below(sim.order_templates().len()));
                    let rep = self.rng.chance(30);
                    self.note(t, "add_building_job", format!("{id} {sub} tpl {tpl} repeat {rep}"));
                    sim.add_building_job(id, tpl, rep);
                }
            }
            38..=40 => {
                let u = self.unit_id(sim, |u| !u.flags.dead);
                let l = LABORS[self.rng.below(LABORS.len())];
                let on = self.rng.chance(60);
                self.note(t, "set_labor", format!("{u} {l} {on}"));
                sim.set_labor(u, l, on);
            }
            41..=42 => {
                let names: Vec<String> = sim.world.work_details.iter().map(|d| d.name.clone()).collect();
                let name = self.rng.pick(&names).cloned().unwrap_or_else(|| "Chaos detail".into());
                match self.rng.below(5) {
                    0 => {
                        let ls: Vec<&str> = (0..self.rng.range(0, 4)).map(|_| LABORS[self.rng.below(LABORS.len())]).collect();
                        sim.create_work_detail(&format!("Chaos {}", self.rng.below(5)), &ls);
                        self.note(t, "work_detail", format!("create {ls:?}"));
                    }
                    1 => {
                        let u = self.citizen(sim);
                        let on = self.rng.chance(60);
                        sim.set_work_detail_member(&name, u, on);
                        self.note(t, "work_detail", format!("member {name} {u} {on}"));
                    }
                    2 => {
                        let m = WD_MODES[self.rng.below(WD_MODES.len())];
                        sim.set_work_detail_mode(&name, m);
                        self.note(t, "work_detail", format!("mode {name} {m}"));
                    }
                    3 => {
                        let ls: Vec<&str> = (0..self.rng.range(0, 5)).map(|_| LABORS[self.rng.below(LABORS.len())]).collect();
                        sim.set_work_detail_labors(&name, &ls);
                        self.note(t, "work_detail", format!("labors {name} {ls:?}"));
                    }
                    _ => {
                        // What the Work details screen's Delete button does (no sim API).
                        if let Some(i) = sim.world.work_details.iter().position(|d| d.name == name) {
                            sim.world.work_details.remove(i);
                            for u in sim.world.units.iter_mut() {
                                u.work_details.retain(|n| *n != name);
                            }
                            self.note(t, "work_detail", format!("delete {name}"));
                        }
                    }
                }
            }
            43 => {
                let types = ["PLANT", "DRINK", "SEEDS", "MEAT", "FISH", "PLANT_GROWTH", "LIQUID_MISC", "CHEESE", ""];
                let it = types[self.rng.below(types.len())];
                let mats: Vec<String> = sim.world.plant_raws.iter().map(|p| format!("PLANT:{}:STRUCTURAL", p.token)).collect();
                let mat = self.rng.pick(&mats).cloned().unwrap_or_default();
                let s = KitchenSetting { item_type: it.into(), item_subtype: String::new(), mat: mat.clone(), no_cook: self.rng.chance(50), no_brew: self.rng.chance(50) };
                self.note(t, "set_kitchen", format!("{it} {mat} cook-{} brew-{}", s.no_cook, s.no_brew));
                sim.set_kitchen(s);
            }
            44..=46 => {
                let id = self.building_id(sim, |b| b.kind == "Stockpile");
                let n = self.rng.range(0, 5) as usize;
                let cats: Vec<String> = (0..n).map(|_| STOCK_CATS[self.rng.below(STOCK_CATS.len())].to_string()).collect();
                let mut filters = Vec::new();
                for c in &cats {
                    let opts = sim.stockpile_filter_options(c);
                    for _ in 0..self.rng.range(0, 6) {
                        if let Some(o) = self.rng.pick(&opts) {
                            filters.push(o.0.clone());
                        }
                    }
                }
                let piles: Vec<i32> = sim.world.buildings.iter().filter(|b| b.kind == "Stockpile").map(|b| b.id).collect();
                let links = |r: &mut Rng| -> Vec<i32> { (0..r.range(0, 2)).filter_map(|_| r.pick(&piles).copied().or(Some(-7))).collect() };
                let s = StockpileSettings {
                    categories: cats.clone(),
                    max_barrels: self.rng.range(-1, 30),
                    max_bins: self.rng.range(-1, 30),
                    max_wheelbarrows: self.rng.range(0, 3),
                    filters,
                    use_links_only: self.rng.chance(20),
                    give_to: links(&mut self.rng),
                    take_from: links(&mut self.rng),
                    ..Default::default()
                };
                self.note(t, "set_stockpile_settings", format!("{id} {cats:?} links {:?}/{:?}", s.give_to, s.take_from));
                sim.set_stockpile_settings(id, s);
            }
            47..=52 => self.military(sim, t),
            53..=55 => {
                // Levers, doors, links.
                let mechs = sim.mechanisms();
                if let Some(&(id, kind)) = self.rng.pick(&mechs) {
                    match self.rng.below(4) {
                        0 => {
                            self.note(t, "pull_lever", format!("{id} {kind:?}"));
                            sim.pull_lever(id);
                        }
                        1 => {
                            self.note(t, "pull_lever_now", format!("{id} {kind:?}"));
                            sim.pull_lever_now(id);
                        }
                        2 => {
                            let f = BUILDING_FLAGS[self.rng.below(BUILDING_FLAGS.len())];
                            let on = self.rng.chance(50);
                            self.note(t, "set_building_flag", format!("{id} {kind:?} {f} {on}"));
                            sim.set_building_flag(id, f, on);
                        }
                        _ => {
                            let (other, _) = *self.rng.pick(&mechs).unwrap();
                            let r = sim.link_building(id, other);
                            self.note(t, "link_building", format!("{id} -> {other}: {r:?}"));
                        }
                    }
                }
            }
            56..=57 => {
                let (a, b) = self.near_box(sim, 12, 6, 1);
                let lv = [Traffic::High, Traffic::Normal, Traffic::Low, Traffic::Restricted][self.rng.below(4)];
                self.note(t, "set_traffic", format!("{lv:?} {a:?} {b:?}"));
                sim.set_traffic(a, b, lv);
            }
            58..=61 => {
                let f = ITEM_FLAGS[self.rng.below(ITEM_FLAGS.len())];
                let on = self.rng.chance(60);
                if self.rng.chance(60) {
                    for _ in 0..self.rng.range(1, 10) {
                        let id = self.item_id(sim);
                        sim.set_item_flag(id, f, on);
                    }
                    self.note(t, "set_item_flag", format!("{f} {on}"));
                } else {
                    let (a, b) = self.near_box(sim, 10, 4, 0);
                    let n = sim.set_items_flag_in(a, b, f, on);
                    self.note(t, "set_items_flag_in", format!("{f} {on} {a:?} {b:?} -> {n}"));
                }
            }
            62..=64 => {
                let animals = sim.animals();
                if let Some(a) = self.rng.pick(&animals).cloned() {
                    match self.rng.below(3) {
                        0 => {
                            let on = self.rng.chance(50);
                            self.note(t, "set_slaughter", format!("{} {} {on}", a.unit, a.race));
                            sim.set_slaughter(a.unit, on);
                        }
                        1 => {
                            let zone = self.building_id(sim, |b| b.kind == "Civzone");
                            self.note(t, "assign_pasture", format!("{} {} -> {zone}", a.unit, a.race));
                            sim.assign_pasture(a.unit, zone);
                        }
                        _ => {
                            let k = TRAINING[self.rng.below(TRAINING.len())];
                            self.note(t, "request_training", format!("{} {} {k}", a.unit, a.race));
                            sim.request_training(a.unit, k);
                        }
                    }
                }
            }
            65 => {
                let ps = sim.petitions();
                let id = self.rng.pick(&ps).map(|p| p.id).unwrap_or(-1);
                let acc = self.rng.chance(60);
                let r = sim.answer_petition(id, acc);
                self.note(t, "answer_petition", format!("{id} {acc} -> {r}"));
            }
            66 => {
                let crimes: Vec<i32> = sim.crimes().iter().map(|c| c.id).collect();
                let c = self.rng.pick(&crimes).copied().unwrap_or(-1);
                let u = if self.rng.chance(50) { sim.crimes().iter().find(|x| x.id == c).map(|x| x.criminal).unwrap_or(-1) } else { self.citizen(sim) };
                self.note(t, "convict", format!("{c} {u}"));
                sim.convict(c, u);
            }
            67..=69 => self.trade(sim, t),
            70..=72 => {
                let kinds = [
                    ConstructionKind::Wall,
                    ConstructionKind::Floor,
                    ConstructionKind::Ramp,
                    ConstructionKind::UpStair,
                    ConstructionKind::DownStair,
                    ConstructionKind::UpDownStair,
                    ConstructionKind::Fortification,
                    ConstructionKind::Track,
                    ConstructionKind::Remove,
                ];
                let kind = kinds[self.rng.below(kinds.len())];
                let (a, b) = self.near_box(sim, 10, 3, 0);
                let n = sim.build_construction(kind, a, b);
                self.note(t, "build_construction", format!("{kind:?} {a:?} {b:?} -> {n}"));
            }
            73..=74 => {
                let kinds = [DetailKind::Smooth, DetailKind::Engrave, DetailKind::CarveFortification, DetailKind::CarveTrack, DetailKind::Remove];
                let kind = kinds[self.rng.below(kinds.len())];
                let (a, b) = self.near_box(sim, 10, 4, 0);
                let n = sim.designate_detail(a, b, kind);
                self.note(t, "designate_detail", format!("{kind:?} {a:?} {b:?} -> {n}"));
            }
            75 => {
                let farms: Vec<i32> = sim.world.buildings.iter().filter(|b| b.kind == "FarmPlot").map(|b| b.id).collect();
                if let Some(&f) = self.rng.pick(&farms) {
                    let season = self.rng.below(5);
                    let opts = sim.crop_options_for_season(f, season.min(3));
                    let crop = if self.rng.chance(20) { String::new() } else { self.rng.pick(&opts).cloned().unwrap_or_else(|| "NOT_A_PLANT".into()) };
                    self.note(t, "set_farm_crop", format!("{f} s{season} {crop}"));
                    sim.set_farm_crop(f, season, &crop);
                }
            }
            76 => {
                let tok = NOBLES[self.rng.below(NOBLES.len())];
                let u = self.citizen(sim);
                self.note(t, "appoint", format!("{tok} {u}"));
                sim.appoint(tok, u);
            }
            77 => {
                match self.rng.below(7) {
                    0 => {
                        let on = self.rng.chance(50);
                        sim.set_standing_orders(on);
                        self.note(t, "toggle", format!("standing_orders {on}"));
                    }
                    1 => {
                        let on = self.rng.chance(50);
                        sim.set_cook_brewables(on);
                        self.note(t, "toggle", format!("cook_brewables {on}"));
                    }
                    2 => {
                        let on = self.rng.chance(50);
                        sim.set_forbid_used_ammo(on);
                        self.note(t, "toggle", format!("forbid_used_ammo {on}"));
                    }
                    3 => {
                        let on = self.rng.chance(50);
                        sim.set_auto_convict(on);
                        self.note(t, "toggle", format!("auto_convict {on}"));
                    }
                    4 => {
                        let cap = [0, 10, 90, 200, -1][self.rng.below(5)];
                        sim.set_population_cap(cap);
                        self.note(t, "toggle", format!("population_cap {cap}"));
                    }
                    5 => {
                        let on = self.rng.chance(50);
                        sim.request_broker(on);
                        self.note(t, "toggle", format!("request_broker {on}"));
                    }
                    _ => {
                        let on = self.rng.chance(80) && !self.no_invasions;
                        sim.set_invasions(on);
                        self.note(t, "toggle", format!("invasions {on}"));
                    }
                }
            }
            78..=83 if self.violent => self.events(sim, t),
            84..=86 => {
                // The player's unit-level actions from the debug/test hooks of the UI.
                match self.rng.below(4) {
                    0 => {
                        let u = self.citizen(sim);
                        let k = MOOD_KINDS[self.rng.below(MOOD_KINDS.len())];
                        let r = sim.force_mood(u, k);
                        self.note(t, "force_mood", format!("{u} {k} -> {r}"));
                    }
                    1 => {
                        let u = self.unit_id(sim, |u| !u.flags.dead);
                        let kinds = [("WEAPON", "ITEM_WEAPON_AXE_BATTLE"), ("ARMOR", "ITEM_ARMOR_MAIL_SHIRT"), ("SHIELD", "ITEM_SHIELD_SHIELD"), ("WEAPON", "ITEM_WEAPON_CROSSBOW"), ("AMMO", "ITEM_AMMO_BOLTS"), ("DRINK", "")];
                        let (kd, st) = kinds[self.rng.below(kinds.len())];
                        let r = sim.give_item(u, kd, st, "INORGANIC:IRON", self.rng.below(6) as u8);
                        self.note(t, "give_item", format!("{u} {kd} {st} -> {r:?}"));
                    }
                    2 => {
                        let u = self.unit_id(sim, |u| !u.flags.dead);
                        let n = sim.infect_unit(u);
                        self.note(t, "infect_unit", format!("{u} -> {n}"));
                    }
                    _ => {
                        let n = sim.spawn_migrants(self.rng.range(0, 6) as usize);
                        self.note(t, "spawn_migrants", format!("{n}"));
                    }
                }
            }
            87..=88 => {
                let locs: Vec<i32> = sim.world.locations.iter().map(|l| l.id).collect();
                let l = self.rng.pick(&locs).copied().unwrap_or(-1);
                let roles = [VisitorRole::Performer, VisitorRole::Scholar, VisitorRole::Mercenary, VisitorRole::MonsterSlayer, VisitorRole::Pilgrim];
                let role = roles[self.rng.below(roles.len())];
                let r = sim.spawn_visitor_for(role, l);
                self.note(t, "spawn_visitor", format!("{role:?} {l} -> {r:?}"));
            }
            89 => {
                // Rare: a player floods something by channeling into water / magma.
                if self.rng.chance(30) {
                    let p = self.near_pos(sim, 8);
                    let d = self.rng.range(1, 7) as u8;
                    let magma = self.rng.chance(20);
                    self.note(t, "set_liquid", format!("{p:?} {d} magma {magma}"));
                    sim.set_liquid(p, d, magma);
                }
            }
            _ => {
                // Most of the time the player just watches.
            }
        }
    }

    fn military(&mut self, sim: &mut Sim, t: u64) {
        let q = self.squad_id(sim);
        match self.rng.below(10) {
            0 => {
                let id = sim.create_squad("Chaos squad");
                self.note(t, "create_squad", format!("-> {id}"));
            }
            1 | 2 => {
                let u = if self.rng.chance(15) { -1 } else { self.citizen(sim) };
                let pos = self.rng.below(12);
                self.note(t, "assign_squad_member", format!("{q} pos {pos} unit {u}"));
                sim.assign_squad_member(q, pos, u);
            }
            3 | 4 => {
                let order = match self.rng.below(4) {
                    0 => SquadOrder::Station(self.near_pos(sim, 15)),
                    1 => {
                        let hs = sim.hostiles();
                        let target = if self.rng.chance(70) { self.rng.pick(&hs).copied().unwrap_or(-1) } else { self.unit_id(sim, |_| true) };
                        SquadOrder::Kill(target)
                    }
                    2 => SquadOrder::Train,
                    _ => SquadOrder::Cancel,
                };
                self.note(t, "order_squad", format!("{q} {order:?}"));
                sim.order_squad(q, order);
            }
            5 | 6 => {
                let sch = sim.squad_schedules();
                let s = if self.rng.chance(5) { "Nonsense" } else { sch[self.rng.below(sch.len())] };
                self.note(t, "set_squad_schedule", format!("{q} {s}"));
                sim.set_squad_schedule(q, s);
            }
            7 => {
                let n = self.rng.range(0, 4);
                let items: Vec<&str> = (0..n).map(|_| UNIFORM_ITEMS[self.rng.below(UNIFORM_ITEMS.len())]).collect();
                let pos = self.rng.below(12);
                self.note(t, "set_squad_uniform", format!("{q} pos {pos} {items:?}"));
                sim.set_squad_uniform(q, pos, &items);
            }
            8 => {
                if self.rng.chance(50) {
                    let ammo = (0..self.rng.range(0, 2))
                        .map(|_| SquadAmmo { item: ["AMMO:ITEM_AMMO_BOLTS", "AMMO", "ITEM_AMMO_ARROWS"][self.rng.below(3)].into(), amount: self.rng.range(0, 500), combat: self.rng.chance(70), training: self.rng.chance(70), ..Default::default() })
                        .collect();
                    self.note(t, "set_squad_ammo", format!("{q}"));
                    sim.set_squad_ammo(q, ammo);
                } else {
                    let zones: Vec<i32> = (0..self.rng.range(0, 2)).map(|_| self.building_id(sim, |b| b.kind == "Civzone")).collect();
                    self.note(t, "set_squad_barracks", format!("{q} {zones:?}"));
                    sim.set_squad_barracks(q, &zones);
                }
            }
            _ => {
                let n = self.rng.range(0, 5);
                let wps: Vec<Pos> = (0..n).map(|_| self.near_pos(sim, 20)).collect();
                self.note(t, "order_squad_patrol", format!("{q} {} waypoints", wps.len()));
                sim.order_squad_patrol(q, &wps);
            }
        }
    }

    fn trade(&mut self, sim: &mut Sim, t: u64) {
        match sim.trade_status() {
            Some(st) => {
                if self.rng.chance(40) {
                    // Haul some fort goods to the depot.
                    let ids: Vec<i32> = (0..self.rng.range(1, 8)).map(|_| self.item_id(sim)).collect();
                    self.note(t, "move_to_depot", format!("{ids:?}"));
                    sim.move_to_depot(&ids);
                } else {
                    let offer: Vec<i32> = (0..self.rng.range(0, 6)).filter_map(|_| self.rng.pick(&st.fort_goods).copied()).collect();
                    let mut request: Vec<i32> = (0..self.rng.range(0, 4)).filter_map(|_| self.rng.pick(&st.goods).copied()).collect();
                    if self.rng.chance(10) {
                        request.push(self.item_id(sim));
                    }
                    let commit = self.rng.chance(60);
                    let r = sim.trade(&offer, &request, commit);
                    self.note(t, "trade", format!("offer {} request {} commit {commit} -> {}", offer.len(), request.len(), r.accepted));
                }
            }
            None => {
                if self.rng.chance(30) {
                    let races = ["DWARF", "HUMAN", "ELF", "GOBLIN", "NOTARACE"];
                    let r = races[self.rng.below(races.len())];
                    let ok = sim.force_caravan(r);
                    self.note(t, "force_caravan", format!("{r} -> {ok}"));
                }
            }
        }
    }

    fn events(&mut self, sim: &mut Sim, t: u64) {
        // Keep the fort survivable: about one violent event every month or two, and none
        // while many hostiles are around.
        if !self.rng.chance(20) || sim.hostiles().len() > 60 {
            return;
        }
        match self.rng.below(10) {
            0 => {
                let n = sim.force_siege();
                self.note(t, "force_siege", format!("-> {n}"));
            }
            1 | 2 => {
                let kinds = [RaidKind::Siege, RaidKind::Ambush, RaidKind::Thief, RaidKind::Snatcher];
                let k = kinds[self.rng.below(4)];
                let n = self.rng.range(0, 12) as usize;
                let m = self.rng.range(0, 2) as usize;
                let r = sim.force_raid(k, n, m);
                self.note(t, "force_raid", format!("{k:?} {n} {m} -> {r}"));
            }
            3 => {
                let r = sim.spawn_forgotten_beast();
                self.note(t, "spawn_forgotten_beast", format!("-> {r:?}"));
            }
            4 => {
                let races = ["GOBLIN", "TROLL", "KOBOLD", "ELF", "NOTARACE"];
                let r = races[self.rng.below(races.len())];
                let n = sim.spawn_siege(r, self.rng.range(0, 10) as usize);
                self.note(t, "spawn_siege", format!("{r} -> {n}"));
            }
            5 => {
                let u = self.unit_id(sim, |u| !u.flags.dead && !u.flags.citizen);
                self.note(t, "make_hostile", format!("{u}"));
                sim.make_hostile(u);
            }
            6 => {
                // Kill a hostile (or anything) like the UI's debug kill.
                let hs = sim.hostiles();
                let u = if self.rng.chance(80) { self.rng.pick(&hs).copied().unwrap_or(-1) } else { self.unit_id(sim, |u| !u.flags.dead) };
                let r = sim.kill_unit(u, "chaos");
                self.note(t, "kill_unit", format!("{u} -> {r}"));
            }
            _ => {}
        }
    }

    /// Everything the UI reads every frame or on a screen: must never panic.
    pub fn ui_queries(&mut self, sim: &mut Sim) {
        let _ = sim.stats();
        let _ = sim.state_hash();
        let _ = sim.jobs();
        let _ = sim.construction_jobs();
        let _ = sim.hostiles();
        let _ = sim.petitions();
        let _ = sim.visitors();
        let _ = sim.animals();
        let _ = sim.crimes().len();
        let _ = sim.trade_status();
        let _ = sim.population_info();
        let _ = sim.population_caps();
        let _ = sim.production_report();
        let _ = sim.production_notes();
        let _ = sim.storage_report();
        let _ = sim.industry_report();
        let _ = sim.combat_stats();
        let _ = sim.defense_stats();
        let _ = sim.ranged_stats();
        let _ = sim.construct_report();
        let _ = sim.health_report();
        let _ = sim.cavern_report();
        let _ = sim.cavein_report();
        let _ = sim.fluid_report();
        let _ = sim.growth_report();
        let _ = sim.animal_report();
        let _ = sim.population_report();
        let _ = sim.trade_report();
        let _ = sim.location_report();
        let _ = sim.justice_report();
        let _ = sim.lifestyle_report();
        let _ = sim.social_report();
        let _ = sim.skills_report();
        let _ = sim.mood_records().len();
        let _ = sim.mood_demands();
        let _ = sim.mood_counts();
        let _ = sim.siege_plan();
        let _ = sim.invasion_tier_now();
        let _ = sim.siege_active();
        let _ = sim.siege_size();
        let _ = sim.combat_log();
        let _ = sim.raids();
        let _ = sim.mechanisms();
        let _ = sim.broker_unit();
        let _ = sim.liquid_stats();
        let _ = sim.order_templates();
        let _ = sim.build_options();
        let ids: Vec<i32> = sim.world.units.iter().map(|u| u.id).collect();
        for _ in 0..12 {
            let Some(&u) = self.rng.pick(&ids) else { break };
            let _ = sim.unit_status(u);
            let _ = sim.unit_needs(u);
            let _ = sim.unit_path(u);
            let _ = sim.stress_level(u);
            let _ = sim.need_report(u);
            let _ = sim.breakdown(u);
            let _ = sim.recent_thoughts(u, 8);
            let _ = sim.focus_percent(u);
            let _ = sim.longterm_stress(u);
            let _ = sim.unit_condition(u);
            let _ = sim.health_info(u);
            let _ = sim.mood_info(u);
            let _ = sim.is_prisoner(u);
            let _ = sim.unit_hidden(u);
            let _ = sim.grazer_hunger(u);
            let _ = sim.unit_ammo(u);
        }
        let bids: Vec<(i32, String)> = sim.world.buildings.iter().map(|b| (b.id, b.kind.clone())).collect();
        for _ in 0..12 {
            let Some((b, kind)) = self.rng.pick(&bids).cloned() else { break };
            let _ = sim.room_value(b);
            let _ = sim.lever_jobs_pending(b);
            let _ = sim.trap_loaded(b);
            if kind == "FarmPlot" {
                let _ = sim.farm_tiles(b);
                let _ = sim.crop_options(b);
            }
            if kind == "TradeDepot" {
                let _ = sim.fort_goods_in_depot(b);
                let _ = sim.wagon_access(b);
            }
        }
        let qs: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
        for q in qs {
            let _ = sim.squad_equipment(q);
        }
        for _ in 0..12 {
            let p = self.near_pos(sim, 10);
            let _ = sim.is_walkable(p);
            let _ = sim.engraving_at(p);
            let _ = sim.location_at(p);
            let _ = sim.miasma_at(p);
            let _ = sim.grass_at(p);
            let _ = sim.liquid_at(p);
            let _ = sim.has_mud(p);
        }
        for _ in 0..6 {
            let i = self.item_id(sim);
            let _ = sim.item_trade_value(i);
        }
    }
}

/// Structural invariants of the public world state. Returns one line per violation.
pub fn violations(sim: &Sim) -> Vec<String> {
    let w = &sim.world;
    let mut out = Vec::new();
    let n = w.dims.x as usize * w.dims.y as usize * w.dims.z as usize;
    if w.tiles.len() != n {
        out.push(format!("tiles {} != dims {n}", w.tiles.len()));
    }
    let mut uids = HashSet::new();
    for u in &w.units {
        if !uids.insert(u.id) {
            out.push(format!("duplicate unit id {}", u.id));
        }
        if !u.age.is_finite() {
            out.push(format!("unit {} age {}", u.id, u.age));
        }
        if !u.flags.dead && !u.flags.inactive && !w.in_bounds(u.pos) {
            out.push(format!("unit {} ({}) out of bounds at {:?}", u.id, u.race, u.pos));
        }
    }
    let mut iids = HashSet::new();
    for it in &w.items {
        if !iids.insert(it.id) {
            out.push(format!("duplicate item id {}", it.id));
        }
    }
    let mut bids = HashSet::new();
    for b in &w.buildings {
        if !bids.insert(b.id) {
            out.push(format!("duplicate building id {} ({})", b.id, b.kind));
        }
        if b.x1 > b.x2 || b.y1 > b.y2 {
            out.push(format!("building {} inverted box", b.id));
        }
        if !w.in_bounds(Pos::new(b.x1, b.y1, b.z)) || !w.in_bounds(Pos::new(b.x2, b.y2, b.z)) {
            out.push(format!("building {} ({}) out of bounds", b.id, b.kind));
        }
        for &i in &b.items {
            if !iids.contains(&i) {
                out.push(format!("building {} ({}) holds missing item {i}", b.id, b.kind));
            }
        }
    }
    for u in &w.units {
        if u.flags.dead {
            continue;
        }
        for &i in &u.inventory {
            if !iids.contains(&i) {
                out.push(format!("unit {} carries missing item {i}", u.id));
            }
        }
    }
    for it in &w.items {
        if it.container >= 0 && !iids.contains(&it.container) {
            out.push(format!("item {} ({} {}, holder {}, ground {}) in missing container {}", it.id, it.kind, it.subtype, it.holder, it.flags.on_ground, it.container));
        }
        if it.container == it.id && it.id >= 0 {
            out.push(format!("item {} contains itself", it.id));
        }
        if it.holder >= 0 && !uids.contains(&it.holder) {
            out.push(format!("item {} held by missing unit {}", it.id, it.holder));
        }
        if it.flags.on_ground && it.container < 0 && it.holder < 0 && !w.in_bounds(it.pos) {
            out.push(format!("item {} on ground out of bounds {:?}", it.id, it.pos));
        }
    }
    for q in &w.squads {
        for &m in &q.members {
            if m >= 0 && !uids.contains(&m) {
                out.push(format!("squad {} member {m} missing", q.id));
            }
        }
    }
    for (p, _, _) in sim.jobs() {
        if !w.in_bounds(p) {
            out.push(format!("job out of bounds {p:?}"));
        }
    }
    out
}

/// Save/load round trip exactly like the app (bincode of the world, `reindex`, `Sim::new`).
pub fn save_load(sim: Sim) -> Sim {
    let seed = sim.seed;
    let bytes = bincode::serialize(&sim.world).expect("serialize world");
    drop(sim);
    let mut w: World = bincode::deserialize(&bytes).expect("deserialize world");
    w.tiletypes.reindex();
    Sim::new(w, seed)
}

impl Chaos {
    /// Record the violations that exist before any action (import quirks), so only new ones
    /// count.
    pub fn set_baseline(&mut self, sim: &Sim) {
        self.baseline = violations(sim).into_iter().collect();
    }

    /// New violations since the baseline.
    pub fn check(&mut self, sim: &Sim) -> Vec<String> {
        violations(sim).into_iter().filter(|v| !self.baseline.contains(v)).collect()
    }

    /// Run `steps` chaos steps of `step_ticks` ticks each. Panics propagate (callers wrap it).
    pub fn run(&mut self, mut sim: Sim, steps: u64, step_ticks: u32, save_every: u64) -> Sim {
        self.out.citizens_start = sim.world.citizens().count();
        if self.no_invasions {
            sim.set_invasions(false);
        }
        if self.baseline.is_empty() {
            self.set_baseline(&sim);
        }
        let trace = std::env::var_os("CHAOS_TRACE").is_some();
        for s in 0..steps {
            if trace {
                eprintln!("trace step {s} tick {} act", sim.ticks());
            }
            let n = self.rng.range(0, 3);
            for _ in 0..n {
                self.act(&mut sim);
                if trace {
                    eprintln!("trace   {}", self.log.last().cloned().unwrap_or_default());
                }
            }
            // Like a player: lift a civilian alert after half a month (burrowed civilians
            // without food starve, as in DF).
            if self.lift_alert && self.alert_since.map(|t| sim.ticks() >= t + rf_core::TICKS_PER_MONTH as u64 / 2).unwrap_or(false) {
                sim.set_civilian_alert(None);
                self.alert_since = None;
                self.note(sim.ticks(), "civilian_alert", "lifted".into());
            }
            // Like a player: when hostiles show up, the military gets ready (usually).
            if self.guard && self.rng.chance(60) && !sim.hostiles().is_empty() && sim.world.squads.iter().all(|q| q.schedule.eq_ignore_ascii_case("Off duty") || q.schedule.is_empty()) {
                let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
                for id in ids {
                    sim.set_squad_schedule(id, "Ready");
                }
                self.note(sim.ticks(), "guard_ready", String::new());
            }
            let before = sim.stats();
            let t0 = std::time::Instant::now();
            if trace {
                eprintln!("trace step {s} run_ticks");
            }
            sim.run_ticks(step_ticks);
            let dt = t0.elapsed().as_secs_f64();
            self.out.sim_secs += dt;
            if dt > 0.0 && (step_ticks as f64 / dt) < self.slow_tps && self.out.slow.len() < 30 {
                let st = sim.stats();
                let recent: Vec<String> = self.log.iter().rev().take(4).cloned().collect();
                self.out.slow.push(format!(
                    "step {s} tick {}: {:.0} ticks/s, paths {} nodes {} fails {}, units {}, hostiles {}, jobs {}; recent: {}",
                    sim.ticks(),
                    step_ticks as f64 / dt,
                    st.paths.saturating_sub(before.paths),
                    st.path_nodes.saturating_sub(before.path_nodes),
                    st.path_failures.saturating_sub(before.path_failures),
                    st.active_units,
                    sim.hostiles().len(),
                    st.open_jobs,
                    recent.join(" | ")
                ));
            }
            self.out.ticks += step_ticks as u64;
            self.out.steps += 1;
            if s % 8 == 0 {
                if trace {
                    eprintln!("trace step {s} ui_queries");
                }
                self.ui_queries(&mut sim);
            }
            if trace {
                eprintln!("trace step {s} check");
            }
            let cits: HashSet<String> = sim.world.units.iter().filter(|u| u.flags.citizen).map(|u| u.name.clone()).collect();
            for a in sim.drain_announcements() {
                if a.kind == rf_sim::AnnouncementKind::Death {
                    if let Some((who, cause)) = a.text.split_once(" has ") {
                        if cits.contains(who) {
                            *self.out.citizen_deaths.entry(cause.trim_end_matches('.').to_string()).or_default() += 1;
                        }
                    }
                }
            }
            let v = self.check(&sim);
            if !v.is_empty() {
                for x in &v {
                    if self.out.violations.len() < 50 {
                        self.out.violations.push(format!("step {s} tick {}: {x}", sim.ticks()));
                    }
                }
                // Accept them as the new baseline so one bug is reported once.
                self.baseline.extend(v);
            }
            if save_every > 0 && s > 0 && s % save_every == 0 {
                self.log.push(format!("t{} save_load", sim.ticks()));
                sim = save_load(sim);
                if self.no_invasions {
                    sim.set_invasions(false);
                }
                self.out.saves += 1;
            }
        }
        self.out.citizens_end = sim.world.citizens().count();
        sim
    }
}
