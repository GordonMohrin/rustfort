//! [animals] Domestic animals: grazing and pastures, breeding and eggs, milking, shearing,
//! cheese and thread, slaughter, training (taming, war and hunting animals), pets, hunting,
//! fishing and vermin hunting cats (DF v50 fortress mode).
//!
//! # DF rules implemented (wiki v50: Grazer, Pasture, Breeding, Egg, Nest box, Milking, Shearing,
//! Butcher, Animal training, Hunting, Fishing, Pet, Creature token)
//! - **Tame animals** do not eat or drink, except **grazers** (caste token `GRAZER`): their
//!   hunger rises by 1 per tick and they die at 100 000 ("starved to death"). They eat grass
//!   (cave moss and other grass tiletypes count, dead grass does not) where they stand.
//!   `GRAZER = 20000 x 100 x (max size / 10)^-3/4`, capped to 150..3 000 000 (wiki: cow 521.7);
//!   eating 1 % of a tile's grass takes `GRAZER / 100` hunger off, so a full tile feeds a cow 521
//!   ticks and "a creature with ten times the grazer value needs one tenth the amount of grass".
//!   A tile eaten bare reverts to the soil below (wiki).
//! - **Pastures** (`Civzone` `Pen`): an assigned animal is led there by an idle dwarf with the
//!   Animal hauling labor; it then strolls inside the zone. Unassigned animals wander.
//! - **Breeding**: an idle adult tame female and a compatible male of the same race on adjacent
//!   tiles; 5 % of animals are not interested; no pregnancy at 50 animals of a race on the map
//!   or when children are more than 75 % of them (wiki "Breeding"). Gestation of non-sentient
//!   animals is 6 months. Litters: 1-3 young (LITTERSIZE default), `MULTIPLE_LITTER_RARE` = 1,
//!   with a 1/500 chance of 1-3. Young are born tame; adulthood at `CHILD:1` (vanilla raws).
//! - **Eggs**: an adult `LAYS_EGGS` female ready to lay claims an empty nest box, walks there and
//!   lays a clutch (vanilla `CLUTCH_SIZE`); the next clutch can follow exactly 3 months later.
//!   A mating fertilizes the female for 6 months and covers 2 layings; fertile eggs left in the
//!   nest box (e.g. forbidden) hatch exactly 3 months after they were laid. Food haulers collect
//!   eggs that are not forbidden (wiki "Nest box": hauling labor), so they become food.
//! - **Milking** (Farmer's workshop, labor Milking, `MilkCreature`): a `MILKABLE` female whose
//!   milk has recharged (raws: 20 000 ticks for all vanilla animals) walks to the workshop, the
//!   milker fills an empty bucket with milk. **Make cheese** (Cheese making) turns milk into cheese.
//! - **Shearing** (Farmer's workshop, labor Shearing, `ShearCreature`): sheep, llamas and alpacas
//!   (the only shearable vanilla animals) give wool, ready again 300 days after shearing.
//!   **Spin thread** (Spinning) turns wool into yarn thread for the loom.
//! - **Slaughter**: an animal marked for slaughter (not somebody's pet) is led by a butcher to
//!   the butcher's shop and killed there; the shop gets a "Butcher a dead animal" job
//!   (DF's auto-butcher standing order) and the existing butcher recipe makes the products.
//! - **Training** (labor Animal training): levels WildUntamed, SemiWild, Trained, WellTrained,
//!   SkilfullyTrained, ExpertlyTrained, ExceptionallyTrained, MasterfullyTrained, Domesticated.
//!   Taming needs `PET` or `PET_EXOTIC`; a wild animal must be caged while it is tamed. War
//!   training needs `TRAINABLE_WAR`, hunting training `TRAINABLE_HUNTING` (or `TRAINABLE`), and
//!   adults (juveniles cannot be trained for war or hunting). Every training job adds 10 points
//!   of fort knowledge for the species. Trained adults slowly revert unless re-trained; a young
//!   animal trained once is domesticated for good; bought/born tame animals never revert.
//! - **War animals** follow the dwarf they are assigned to and attack hostiles close to them;
//!   **hunting animals** follow their hunter and attack the hunter's prey.
//! - **Pets**: idle citizens near a stray tame `PET` animal may adopt it ("after adopting a new
//!   pet" happy thought); `ADOPTS_OWNER` animals (cats) pick an owner themselves. Pets follow
//!   their owner, cannot be slaughtered and give "after interacting with a pet" thoughts.
//! - **Hunting** (labor Hunting): needs a crossbow and bolts; the hunter stalks a wild,
//!   butcherable animal, shoots (1 bolt per shot, no shooting without bolts), carries the corpse
//!   to the butcher's shop and the auto-butcher job is queued.
//! - **Fishing** (labor Fishing): fishers prefer fishing zones (`FishingArea`), else any
//!   reachable water; catches are raw vermin fish: pond turtles in water that is not a river,
//!   lake or cavern (wiki), cave fish/lobsters in caverns. Overfished water is empty until the
//!   season changes. **Prepare raw fish** (Fishery, labor Fish cleaning) makes edible fish; the
//!   fishery gets the job automatically (auto fishery standing order).
//! - **Cats** (`HUNTS_VERMIN`) kill vermin; the remains are refuse.
//!
//! # Estimates (not on the wiki)
//! - Grass regrows 1 % per 200 ticks (a bare tile is full again after 20 000 ticks), which makes
//!   the wiki's rule of thumb "20000 = GRAZER x required tiles" the steady state; a bare tile
//!   turns back into grass at 20 %. DF's biome rules (no regrowth in mountains) are not modelled.
//! - Grazers start eating at hunger 2 000; a bite takes 1 tick per % eaten (at least 10).
//! - Mating chance 1/5 per 100 ticks of adjacency; newborn size = adult size / 10 growing
//!   linearly to adult size at age 1.
//! - Work times: milking 200, shearing 300, cheese 300, spinning 300, fish cleaning 200, training
//!   500, fishing 600 ticks (all scaled by skill like other jobs), shooting 1 bolt per 50 ticks.
//! - Products: milk 1 per milking, wool 1 per shearing, 1 cheese per milk, 1 thread per wool,
//!   2 fish per raw fish.
//! - Training success: 40 % + 4 % per Animal training level + up to 20 % from fort knowledge
//!   (1 % per 20 points), at most 95 %; trained adults lose one level after 4 months without a
//!   training session (re-trained after 2 months); taming a wild animal makes it Trained.
//! - Hunting: hit chance 30 % + 4 % per crossbow level + 2 % per ambusher level (max 90 %),
//!   1 + body size / 100 000 hits kill; range 10 tiles on the same level; equipment is
//!   reserved from the stockpile, not carried.
//! - Fishing: catch chance 30 % + 5 % per Fishing level (max 90 %); 40 catches per water body
//!   (fishing zone, or the cavern/surface water in general) per season.
//! - Pet adoption: 1/20 per day for an idle citizen within 3 tiles of a stray pet; a pet near
//!   its owner gives an interaction thought 1/7 per day; cats kill vermin 1/14 per day.
//! - Off-map animals in the export (position -30000, 80 in the round-5 export) are not simulated.

use crate::path::Goals;
use crate::production::{Input, WJob, RECIPES, WK};
use crate::rng::Rng;
use crate::{labor, AnnouncementKind, Kind, Sim, Task, NONE};
use rf_core::{tile_flags, BuildingJob, Item, ItemFlags, Pos, Shape, TtMaterial, Unit, UnitFlags, NO_MAT, TICKS_PER_DAY, TICKS_PER_MONTH};
use std::collections::HashMap;

/// Industry reagent families (`production::Input::Ind`) handled by this module.
pub const F_FIRST: u8 = 40;
pub const F_MILK: u8 = 40;
pub const F_SHEAR: u8 = 41;
pub const F_CHEESE: u8 = 42;
pub const F_SPIN: u8 = 43;
pub const F_CLEAN_FISH: u8 = 44;

/// Ticks between the animal passes (breeding, eggs, census, training, pets).
pub const ANIMAL_PERIOD: u64 = 100;
const ANIMAL_PHASE: u64 = 61;
/// DF hunger death (wiki "Grazer").
pub const HUNGER_DEATH: i32 = 100_000;
/// Grazers start grazing at this hunger (estimate).
const GRAZE_AT: i32 = 2_000;
/// Grass: 0..=100 % per tile, +1 % per 200 ticks (estimate, see the module docs).
const GRASS_FULL: u8 = 100;
const GRASS_REGROW_TICKS: u64 = 200;
const GRASS_BACK: u8 = 20;
/// Wiki "Breeding": non-sentient animals carry 6 months.
pub const GESTATION: u64 = 6 * TICKS_PER_MONTH as u64;
/// Wiki "Breeding": per-race cap on the map; children share cap.
pub const POP_CAP: usize = 50;
/// Wiki "Egg": next clutch 3 months after the last; incubation exactly 3 months; a mating
/// fertilizes for about 6 months and covers two layings.
pub const EGG_INTERVAL: u64 = 3 * TICKS_PER_MONTH as u64;
pub const INCUBATION: u64 = 3 * TICKS_PER_MONTH as u64;
const FERTILE: u64 = 6 * TICKS_PER_MONTH as u64;
/// Raws: `MILKABLE:LOCAL_CREATURE_MAT:MILK:20000` for every vanilla milkable animal.
pub const MILK_INTERVAL: u64 = 20_000;
/// Wiki "Shearing": ready 300 days after shearing (`SHEARABLE_TISSUE_LAYER:LENGTH:300`).
pub const SHEAR_REGROW: u64 = 300 * TICKS_PER_DAY as u64;
/// Vanilla raws: `CHILD:1` for all domestic animals (years).
const CHILD_YEARS: f32 = 1.0;
/// Training (estimates).
const TRAIN_TICKS: u32 = 500;
const RETRAIN_AFTER: u64 = 2 * TICKS_PER_MONTH as u64;
const REVERT_AFTER: u64 = 4 * TICKS_PER_MONTH as u64;
/// Hunting / fishing (estimates).
const SHOT_TICKS: u64 = 50;
const SHOOT_RANGE: i32 = 10;
const FISH_TICKS: u32 = 600;
const FISH_PER_SEASON: i32 = 40;

/// DF `animal_training_level` names, least to most trained.
pub const LEVELS: [&str; 9] =
    ["WildUntamed", "SemiWild", "Trained", "WellTrained", "SkilfullyTrained", "ExpertlyTrained", "ExceptionallyTrained", "MasterfullyTrained", "Domesticated"];
const L_WILD: u8 = 0;
const L_SEMI: u8 = 1;
const L_TRAINED: u8 = 2;
const L_MASTER: u8 = 7;
const L_DOMESTIC: u8 = 8;

const ROLE_NONE: u8 = 0;
const ROLE_WAR: u8 = 1;
const ROLE_HUNT: u8 = 2;
const REQ_TAME: u8 = 1;
const REQ_WAR: u8 = 2;
const REQ_HUNT: u8 = 4;

const NO_SP: u16 = u16::MAX;

/// Vanilla clutch sizes (creature raws `CLUTCH_SIZE`); other egg layers use 3-8 (estimate).
const CLUTCHES: &[(&str, u8, u8)] = &[
    ("BIRD_CHICKEN", 4, 15),
    ("BIRD_DUCK", 8, 13),
    ("BIRD_GOOSE", 3, 8),
    ("BIRD_GUINEAFOWL", 4, 15),
    ("BIRD_PEAFOWL_BLUE", 6, 8),
    ("BIRD_TURKEY", 10, 14),
    ("BIRD_FALCON_PEREGRINE", 3, 4),
    ("BIRD_RAVEN", 3, 7),
    ("CRUNDLE", 5, 20),
    ("ELK_BIRD", 2, 10),
];
/// Wiki "Shearing": the only shearable vanilla animals.
const SHEARABLE: &[&str] = &["SHEEP", "LLAMA", "ALPACA"];

/// What the simulation knows about a species (from the exported creature raws).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct Species {
    pub token: String,
    pub name: String,
    /// GRAZER value (0 = no grazer).
    pub grazer: i32,
    pub milkable: bool,
    pub eggs: bool,
    pub clutch: (u8, u8),
    pub shear: bool,
    pub pet: bool,
    pub war: bool,
    pub hunt: bool,
    pub vermin: bool,
    pub adopts: bool,
    pub litter_rare: bool,
    /// Wildlife a hunter goes after: a natural, butcherable, non-sapient animal.
    pub huntable: bool,
    pub adult_size: i32,
    /// (caste token, caste name, sex) for newborns.
    pub castes: Vec<(String, String, i8)>,
}

/// Per-unit animal state (indexed by AI slot).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct AState {
    pub sp: u16,
    pub last: u64,
    pub hunger: i32,
    pub milk_at: u64,
    pub shear_at: u64,
    pub egg_at: u64,
    pub fertile: u8,
    pub fertile_until: u64,
    pub preg_until: u64,
    /// Worker holding (walking to / leading) this animal, or doing a workshop job with it.
    pub held: u32,
    pub led: bool,
    pub prod: u32,
    pub nest: i32,
    pub level: u8,
    pub role: u8,
    pub req: u8,
    pub last_train: u64,
    pub retry: u64,
    pub next_follow: u64,
    pub no_breed: bool,
    pub lead_req: bool,
    pub init: bool,
}

impl Default for AState {
    fn default() -> Self {
        AState {
            sp: NO_SP,
            last: 0,
            hunger: 0,
            milk_at: 0,
            shear_at: 0,
            egg_at: 0,
            fertile: 0,
            fertile_until: 0,
            preg_until: 0,
            held: NONE,
            led: false,
            prod: NONE,
            nest: -1,
            level: L_DOMESTIC,
            role: ROLE_NONE,
            req: 0,
            last_train: 0,
            retry: 0,
            next_follow: 0,
            no_breed: false,
            lead_req: false,
            init: false,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)] // [saves11]
pub(crate) enum WKind {
    #[default]
    None,
    Lead,
    Slaughter,
    Train,
    Hunt,
    Fish,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)] // [saves11]
pub(crate) enum Step {
    #[default]
    None,
    GoAnimal,
    Leading,
    Training,
    Stalk,
    Shoot,
    GoCorpse,
    Carry,
    GoWater,
    Fishing,
}

/// A worker's animal job (indexed by AI slot).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct AWork {
    pub kind: WKind,
    pub step: Step,
    pub target: u32,
    pub dest: u32,
    pub until: u64,
    pub item: i32,
    pub item2: i32,
    pub hits: i32,
    pub zone: i32,
    pub tries: u8,
    /// The bolts are the hunter's own (in the quiver), not reserved from a stockpile.
    pub own: bool,
}

/// Nest box (DF `NestBox` building).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct Nest {
    pub bid: i32,
    pub tile: u32,
    pub claimed: u32,
}

/// A clutch of eggs lying in a nest box.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct Clutch {
    pub item: i32,
    pub tile: u32,
    pub laid: u64,
    pub fertile: bool,
    pub sp: u16,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)] // [saves11]
pub(crate) struct Grass {
    pub tile: u32,
    pub amount: u8,
    pub bare: bool,
    pub tt: u16,
    pub mat: u16,
}

/// Pasture zone (Pen/Pasture civzone).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)] // [saves11]
pub(crate) struct Pen {
    pub bid: i32,
    pub zone: crate::Zone,
}

/// Animal husbandry counters for reports.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub struct AnimalReport {
    /// Living tame animals on the map, by race token.
    pub by_race: Vec<(String, usize)>,
    pub tame: usize,
    pub grazers: usize,
    pub pastured: usize,
    /// Grazers with hunger above 50 000.
    pub hungry_grazers: usize,
    pub pregnant: usize,
    pub pregnancies: u64,
    /// Matings of egg layers (the female lays fertile eggs for 6 months).
    pub fertilizations: u64,
    pub births: u64,
    pub clutches: u64,
    pub eggs_laid: u64,
    pub eggs_hatched: u64,
    pub milkings: u64,
    pub shearings: u64,
    pub cheese: u64,
    pub thread: u64,
    pub slaughtered: u64,
    pub led_to_pasture: u64,
    pub starved: u64,
    pub grass_eaten: u64,
    pub bare_tiles: usize,
    pub training_sessions: u64,
    pub tamed: u64,
    pub war_trained: u64,
    pub hunt_trained: u64,
    pub reverted: u64,
    pub adoptions: u64,
    pub pet_thoughts: u64,
    pub war_attacks: u64,
    pub hunts: u64,
    pub shots: u64,
    pub hunt_kills: u64,
    pub fishing_trips: u64,
    pub fish_caught: u64,
    pub fish_cleaned: u64,
    pub vermin_killed: u64,
    /// Tame animals in the export that are off the map (not simulated).
    pub off_map: usize,
    /// Wild animals a hunter could go after, and whether a free crossbow and bolts exist.
    pub hunt_targets: u32,
    pub hunt_gear: bool,
    /// CPU seconds spent in the animal passes and the animal AI (`animals_tick`, `animal_think`).
    pub cpu_secs: f64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct ACensus {
    pub milk_ready: i32,
    pub shear_ready: i32,
    pub buckets: i32,
    pub milk: i32,
    pub wool: i32,
    pub raw_fish: i32,
}

#[derive(serde::Serialize, serde::Deserialize, Default)] // [saves11]
#[serde(default)] // [saves11]
pub(crate) struct Animals {
    pub ready: bool,
    pub off: bool,
    pub species: Vec<Species>,
    pub sp_of: HashMap<String, u16>,
    pub st: Vec<AState>,
    pub w: Vec<AWork>,
    pub grass: Vec<Grass>,
    pub grass_at: HashMap<u32, u32>,
    pub nests: Vec<Nest>,
    pub clutches: Vec<Clutch>,
    pub pens: Vec<Pen>,
    pub fish_spots: Vec<(u32, i32)>,
    pub fish_left: Vec<(i32, i32)>,
    pub knowledge: Vec<i32>,
    pub census: ACensus,
    pub last_census: u64,
    pub season: i32,
    pub rep: AnimalReport,
    pub next_try: Vec<u64>,
    /// Open work counters (cheap checks in `decide`).
    pub open_lead: u32,
    pub open_slaughter: u32,
    pub open_train: u32,
    pub hunt_targets: u32,
    pub hunt_gear: bool,
    pub soil_mat: u16,
    pub mats: HashMap<String, u16>,
}

/// STANDARD_GRAZER value (wiki "Grazer"): 20000 x 100 x (size/10)^-0.75, clamped.
pub fn grazer_value(max_size: i32) -> i32 {
    let s = (max_size.max(10) as f64) / 10.0;
    let g = 20_000.0 * 100.0 * s.powf(-0.75);
    (g as i64).clamp(150, 3_000_000) as i32
}

fn cap1(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

impl Sim {
    // ------------------------------------------------------------------ setup

    pub(crate) fn init_animals(&mut self) {
        let a = &mut self.animals;
        a.ready = true;
        a.species.clear();
        a.sp_of.clear();
        for cr in &self.world.creature_raws {
            let has = |f: &str| cr.castes.iter().any(|c| c.flags.iter().any(|x| x == f)) || cr.flags.iter().any(|x| x == f);
            let adult = cr.castes.iter().map(|c| c.body_size).max().unwrap_or(0);
            let clutch = CLUTCHES.iter().find(|c| c.0 == cr.token).map(|c| (c.1, c.2)).unwrap_or((3, 8));
            let castes = cr
                .castes
                .iter()
                .map(|c| {
                    let sex = if c.token == "FEMALE" {
                        0
                    } else if c.token == "MALE" {
                        1
                    } else {
                        -1
                    };
                    (c.token.clone(), c.name.clone(), sex)
                })
                .collect();
            let sp = Species {
                token: cr.token.clone(),
                name: cr.name.clone(),
                grazer: if has("GRAZER") || has("STANDARD_GRAZER") { grazer_value(adult) } else { 0 },
                milkable: has("MILKABLE"),
                eggs: has("LAYS_EGGS"),
                clutch,
                shear: SHEARABLE.contains(&cr.token.as_str()),
                pet: has("PET") || has("PET_EXOTIC"),
                war: has("TRAINABLE_WAR") || has("TRAINABLE"),
                hunt: has("TRAINABLE_HUNTING") || has("TRAINABLE"),
                vermin: has("HUNTS_VERMIN"),
                adopts: has("ADOPTS_OWNER"),
                litter_rare: has("MULTIPLE_LITTER_RARE"),
                huntable: !has("NOT_BUTCHERABLE") && has("NATURAL_ANIMAL") && !has("CAN_LEARN") && !has("CAN_SPEAK"),
                adult_size: adult,
                castes,
            };
            let i = a.species.len() as u16;
            a.sp_of.insert(cr.token.clone(), i);
            a.species.push(sp);
        }
        a.knowledge = vec![0; a.species.len()];
        a.pens.clear();
        a.nests.clear();
        a.fish_spots.clear();
        for b in &self.world.buildings {
            if b.kind == "Civzone" && (b.subtype == "Pen" || b.subtype == "Pasture") {
                a.pens.push(Pen { bid: b.id, zone: crate::Zone { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2, z: b.z } });
            }
        }
        a.season = self.world.calendar.month() / 3;
        // A soil material for bare tiles.
        a.soil_mat = self.world.materials.iter().position(|m| m.is_soil).map(|i| i as u16).unwrap_or(NO_MAT);
        self.animals_refresh_buildings();
        self.animals_ensure();
        let n_off = self
            .world
            .units
            .iter()
            .filter(|u| u.flags.animal && u.flags.tame && !u.flags.dead && self.world.idx(u.pos).is_none())
            .count();
        self.animals.rep.off_map = n_off;
    }

    /// Nest boxes and fishing spots (re-read when buildings change).
    fn animals_refresh_buildings(&mut self) {
        let mut nests = Vec::new();
        let mut spots = Vec::new();
        for b in &self.world.buildings {
            if b.kind == "NestBox" && (b.built || b.stage < 0) {
                if let Some(t) = self.world.idx(b.center) {
                    let old = self.animals.nests.iter().find(|n| n.bid == b.id).map(|n| n.claimed).unwrap_or(NONE);
                    nests.push(Nest { bid: b.id, tile: t as u32, claimed: old });
                }
            }
            if b.kind == "Civzone" && b.subtype == "FishingArea" {
                for y in b.y1..=b.y2 {
                    for x in b.x1..=b.x2 {
                        let p = Pos::new(x, y, b.z);
                        if !b.contains(p) {
                            continue;
                        }
                        let Some(i) = self.world.idx(p) else { continue };
                        if self.nav.walkable(i) && self.an_water_next_to(i) && !spots.iter().any(|&(t, _)| t == i as u32) {
                            spots.push((i as u32, b.id));
                        }
                    }
                }
            }
        }
        self.animals.nests = nests;
        self.animals.fish_spots = spots;
    }

    fn an_water_next_to(&self, i: usize) -> bool {
        let (x, y, z) = self.nav.xyz(i);
        for yy in y.saturating_sub(1)..=(y + 1).min(self.nav.dy - 1) {
            for xx in x.saturating_sub(1)..=(x + 1).min(self.nav.dx - 1) {
                let q = self.nav.idx(xx, yy, z);
                let t = &self.world.tiles[q];
                if t.liquid_depth() > 0 && !t.is_magma() {
                    return true;
                }
            }
        }
        false
    }

    /// Grow the per-slot vectors to the unit count and initialise new animals.
    fn animals_ensure(&mut self) {
        let n = self.ai.len();
        if self.animals.st.len() < n {
            self.animals.st.resize(n, AState::default());
            self.animals.w.resize(n, AWork::default());
            self.animals.next_try.resize(n, 0);
        }
        for s in 0..n {
            if self.animals.st[s].init {
                continue;
            }
            self.animals.st[s].init = true;
            let u = &self.world.units[self.ai[s].uidx];
            if !u.flags.animal {
                continue;
            }
            if u.flags.caged && self.ai[s].kind == Kind::Animal {
                // A creature in a cage does not move (like a cage-trap catch, see `defense`).
                self.ai[s].kind = Kind::Dead;
                self.ai[s].wake = u64::MAX;
            }
            let u = &self.world.units[self.ai[s].uidx];
            let sp = self.animals.sp_of.get(&u.race).copied().unwrap_or(NO_SP);
            let level = match LEVELS.iter().position(|l| *l == u.training) {
                Some(l) => l as u8,
                None if u.flags.tame => L_DOMESTIC,
                None => L_WILD,
            };
            let role = match u.profession.as_str() {
                "TRAINED_WAR" => ROLE_WAR,
                "TRAINED_HUNTER" => ROLE_HUNT,
                _ => ROLE_NONE,
            };
            let mut r = Rng::new(self.seed ^ (u.id as u64).wrapping_mul(0x9E37_79B9));
            let st = &mut self.animals.st[s];
            st.sp = sp;
            st.hunger = u.hunger_timer.max(0);
            st.last = self.ticks;
            st.level = level;
            st.role = role;
            // Wiki "Breeding": 5 % are not interested in reproducing.
            st.no_breed = r.below(100) < 5;
            st.preg_until = if u.pregnancy > 0 { self.ticks + u.pregnancy as u64 } else { 0 };
            // The export has no milk/wool/egg timers: imported animals are ready now.
            st.milk_at = self.ticks;
            st.shear_at = self.ticks;
            st.egg_at = self.ticks;
            st.last_train = self.ticks;
        }
    }

    fn an_sp(&self, s: usize) -> Option<&Species> {
        let i = self.animals.st.get(s)?.sp;
        self.animals.species.get(i as usize)
    }

    /// Living tame animal under this module's control (not caged, on the map, not a merchant's).
    fn an_is_managed(&self, s: usize) -> bool {
        let ai = &self.ai[s];
        if ai.kind != Kind::Animal {
            return false;
        }
        let u = &self.world.units[ai.uidx];
        u.flags.tame && !u.flags.hostile && !u.flags.dead && !u.flags.caged && !u.flags.merchant && !u.flags.visitor && !u.flags.inactive
    }

    fn an_is_adult(&self, s: usize) -> bool {
        self.world.units[self.ai[s].uidx].age >= CHILD_YEARS
    }

    fn an_pet_owner_slot(&self, s: usize) -> Option<usize> {
        let o = self.world.units[self.ai[s].uidx].pet_owner;
        if o <= 0 {
            return None;
        }
        let &os = self.unit_slot.get(&o)?;
        (self.ai[os].kind == Kind::Worker).then_some(os)
    }

    fn an_pen_of(&self, s: usize) -> Option<Pen> {
        let z = self.world.units[self.ai[s].uidx].pasture;
        if z <= 0 {
            return None;
        }
        self.animals.pens.iter().copied().find(|p| p.bid == z)
    }

    fn an_in_zone(&self, t: u32, z: crate::Zone) -> bool {
        let (x, y, zz) = self.nav.xyz(t as usize);
        zz == z.z as usize && (z.x1 as usize..=z.x2 as usize).contains(&x) && (z.y1 as usize..=z.y2 as usize).contains(&y)
    }

    pub(crate) fn animals_owns(&self, s: usize) -> bool {
        self.animals.ready && !self.animals.off && s < self.animals.st.len() && self.animals.st[s].sp != NO_SP && self.an_is_managed(s)
    }

    // ------------------------------------------------------------------ grass

    /// Grass left on a tile in % (0 = no grass).
    fn an_grass_amount(&self, t: u32) -> u8 {
        if let Some(&g) = self.animals.grass_at.get(&t) {
            let g = &self.animals.grass[g as usize];
            return if g.bare { 0 } else { g.amount };
        }
        let info = self.world.tiletypes.get(self.world.tiles[t as usize].tt);
        if info.material.is_grass() && info.material != TtMaterial::GrassDead && info.shape.has_floor() {
            GRASS_FULL
        } else {
            0
        }
    }

    /// Eat up to `want` % of the grass on tile `t`; returns what was eaten.
    fn an_eat_grass(&mut self, t: u32, want: u8) -> u8 {
        let have = self.an_grass_amount(t);
        let n = have.min(want);
        if n == 0 {
            return 0;
        }
        let gi = match self.animals.grass_at.get(&t) {
            Some(&g) => g as usize,
            None => {
                let tile = self.world.tiles[t as usize];
                self.animals.grass.push(Grass { tile: t, amount: GRASS_FULL, bare: false, tt: tile.tt, mat: tile.mat });
                let gi = self.animals.grass.len() - 1;
                self.animals.grass_at.insert(t, gi as u32);
                gi
            }
        };
        let g = &mut self.animals.grass[gi];
        g.amount -= n;
        if g.amount == 0 {
            // Wiki: a tile eaten bare reverts to the base layer (soil).
            g.bare = true;
            let below_soil = {
                let (x, y, z) = self.nav.xyz(t as usize);
                if z > 0 {
                    let q = self.nav.idx(x, y, z - 1);
                    let m = self.world.tiles[q].mat;
                    if self.world.material(m).map(|m| m.is_soil).unwrap_or(false) {
                        m
                    } else {
                        self.animals.soil_mat
                    }
                } else {
                    self.animals.soil_mat
                }
            };
            let variant = self.world.tiletypes.get(self.world.tiles[t as usize].tt).variant.max(1);
            let tt = &self.world.tiletypes;
            let id = tt.find(Shape::Floor, TtMaterial::Soil, rf_core::TtSpecial::Normal, variant).or_else(|| tt.find(Shape::Floor, TtMaterial::Soil, rf_core::TtSpecial::Normal, 1));
            if let Some(id) = id {
                let tile = &mut self.world.tiles[t as usize];
                tile.tt = id;
                if below_soil != NO_MAT {
                    tile.mat = below_soil;
                }
                tile.flags |= tile_flags::DIRTY;
            }
        }
        self.animals.rep.grass_eaten += n as u64;
        n
    }

    /// Daily grass regrowth of the eaten tiles (+1 % per 200 ticks).
    fn an_regrow_grass(&mut self, ticks: u64) {
        let add = (ticks / GRASS_REGROW_TICKS).min(100) as u8;
        if add == 0 || self.animals.grass.is_empty() {
            return;
        }
        let mut k = 0;
        while k < self.animals.grass.len() {
            let g = &mut self.animals.grass[k];
            g.amount = (g.amount + add).min(GRASS_FULL);
            if g.bare && g.amount >= GRASS_BACK {
                g.bare = false;
                let (t, tt, mat) = (g.tile, g.tt, g.mat);
                let tile = &mut self.world.tiles[t as usize];
                tile.tt = tt;
                tile.mat = mat;
                tile.flags |= tile_flags::DIRTY;
            }
            if self.animals.grass[k].amount >= GRASS_FULL && !self.animals.grass[k].bare {
                let t = self.animals.grass[k].tile;
                self.animals.grass_at.remove(&t);
                let last = self.animals.grass.len() - 1;
                self.animals.grass.swap_remove(k);
                if k < last {
                    let moved = self.animals.grass[k].tile;
                    self.animals.grass_at.insert(moved, k as u32);
                }
                continue;
            }
            k += 1;
        }
    }

    // ------------------------------------------------------------------ animal AI

    /// Think step of a tame animal (grazing, pasture, nest box, following its owner, being led).
    pub(crate) fn animal_think(&mut self, s: usize, now: u64) {
        let t = std::time::Instant::now();
        self.an_think_inner(s, now);
        self.animals.rep.cpu_secs += t.elapsed().as_secs_f64();
    }

    fn an_think_inner(&mut self, s: usize, now: u64) {
        let st = self.animals.st[s];
        // Grazer hunger (wiki "Grazer": +1 per tick, death at 100 000).
        let grazer0 = self.an_sp(s).map(|p| p.grazer).unwrap_or(0);
        {
            if grazer0 > 0 {
                let e = now.saturating_sub(st.last) as i32;
                let h = st.hunger + e;
                self.animals.st[s].hunger = h;
                self.animals.st[s].last = now;
                if h >= HUNGER_DEATH {
                    self.animal_release_holds(s);
                    self.kill(s, now, "has starved to death");
                    self.animals.rep.starved += 1;
                    return;
                }
            }
        }
        if st.held != NONE {
            let w = st.held as usize;
            let ok = w < self.ai.len() && self.ai[w].task == Task::Animal && self.animals.w[w].target == s as u32 && self.ai[w].kind == Kind::Worker;
            if !ok {
                self.animals.st[s].held = NONE;
                self.animals.st[s].led = false;
            } else if st.led {
                // Led on a rope: stay with the dwarf.
                let lp = self.ai[w].pos;
                if lp != self.ai[s].pos {
                    self.an_place_unit(s, lp);
                }
                self.ai[s].wake = now + 1;
                return;
            } else {
                self.ai[s].wake = now + 20;
                return;
            }
        }
        if self.ai[s].waiting {
            return;
        }
        if self.ai[s].path_i < self.ai[s].path.len() {
            self.step(s, now);
            return;
        }
        if st.prod != NONE {
            // Waiting at the farmer's workshop to be milked or shorn.
            let w = st.prod as usize;
            let ok = w < self.ai.len()
                && matches!(self.ai[w].task, Task::Fetch(_) | Task::GoSite | Task::Craft)
                && matches!(self.prod.wj.get(w).map(|j| j.kind), Some(WK::Shop { recipe, .. }) if matches!(RECIPES[recipe as usize].input, Input::Ind(F_MILK) | Input::Ind(F_SHEAR)));
            if ok {
                self.ai[s].wake = now + 50;
                return;
            }
            self.animals.st[s].prod = NONE;
        }
        // Laying eggs in the claimed nest box.
        if st.nest >= 0 {
            self.animal_at_nest(s, now);
            return;
        }
        let mut r = Rng::for_unit(self.seed, s, now, 0xA1);
        let (grazer, hunger) = (self.an_sp(s).map(|p| p.grazer).unwrap_or(0), self.animals.st[s].hunger);
        if grazer > 0 && hunger >= GRAZE_AT {
            let here = self.ai[s].pos;
            let want = ((hunger as i64 * 100 + grazer as i64 - 1) / grazer as i64).clamp(1, 100) as u8;
            let got = self.an_eat_grass(here, want);
            if got > 0 {
                let h = (hunger as i64 - got as i64 * grazer as i64 / 100).max(0) as i32;
                self.animals.st[s].hunger = h;
                self.ai[s].wake = now + (got as u64).max(10);
                return;
            }
        }
        // Pets, war and hunting animals follow their dwarf.
        if let Some(os) = self.an_pet_owner_slot(s) {
            if self.an_follow_owner(s, os, now, &mut r) {
                return;
            }
        }
        // Stroll: inside the pasture when in it, towards grass when hungry.
        let pen = self.an_pen_of(s).filter(|p| self.an_in_zone(self.ai[s].pos, p.zone)).map(|p| p.zone);
        if grazer > 0 && hunger >= GRAZE_AT && self.an_step_to_grass(s, now, &mut r, pen) {
            return;
        }
        if r.chance(1, 2) && self.random_step(s, now, &mut r, pen) {
            self.ai[s].wake += 20 + r.below(80) as u64;
        } else {
            self.ai[s].wake = now + 30 + r.below(100) as u64;
        }
    }

    /// Move a unit to tile `t` without the walk timing (led animals).
    fn an_place_unit(&mut self, s: usize, t: u32) {
        let p = self.world.pos_of(t as usize);
        self.ai[s].pos = t;
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].pos = p;
    }

    fn an_step_to_grass(&mut self, s: usize, now: u64, r: &mut Rng, zone: Option<crate::Zone>) -> bool {
        let pos = self.ai[s].pos as usize;
        let (x, y, z) = self.nav.xyz(pos);
        let start = r.below(8) as usize;
        for k in 0..8 {
            let (ddx, ddy) = crate::nav::DIRS8[(start + k) % 8];
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            if let Some(zn) = zone {
                if nx < zn.x1 as isize || nx > zn.x2 as isize || ny < zn.y1 as isize || ny > zn.y2 as isize {
                    continue;
                }
            }
            let q = self.nav.idx(nx as usize, ny as usize, z) as u32;
            if self.nav.walkable(q as usize) && self.an_grass_amount(q) > 0 {
                self.move_to(s, q, now);
                return true;
            }
        }
        false
    }

    fn an_follow_owner(&mut self, s: usize, os: usize, now: u64, r: &mut Rng) -> bool {
        let (a, b) = (self.nav.xyz(self.ai[s].pos as usize), self.nav.xyz(self.ai[os].pos as usize));
        let cheb = (a.0 as i32 - b.0 as i32).abs().max((a.1 as i32 - b.1 as i32).abs());
        if a.2 == b.2 && cheb <= 3 {
            return false;
        }
        if now < self.animals.st[s].next_follow {
            self.ai[s].wake = self.animals.st[s].next_follow;
            return true;
        }
        self.animals.st[s].next_follow = now + 200 + r.below(200) as u64;
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        let t = self.ai[os].pos;
        if c == 0 || self.nav.comp_near(t as usize) != c {
            return false;
        }
        let g = self.an_near_goals(t);
        if g.n == 0 {
            return false;
        }
        self.request_path(s, g, now);
        true
    }

    /// Walkable tiles on and around `t` (same level).
    fn an_near_goals(&self, t: u32) -> Goals {
        let mut g = Goals::default();
        if self.nav.walkable(t as usize) {
            g.push(t);
        }
        let (x, y, z) = self.nav.xyz(t as usize);
        for &(ddx, ddy) in &crate::nav::DIRS8 {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let q = self.nav.idx(nx as usize, ny as usize, z);
            if self.nav.walkable(q) {
                g.push(q as u32);
            }
        }
        g
    }

    fn animal_at_nest(&mut self, s: usize, now: u64) {
        let ni = self.animals.st[s].nest as usize;
        self.animals.st[s].nest = -1;
        let Some(nest) = self.animals.nests.get(ni).copied() else { return };
        if nest.claimed == s as u32 {
            self.animals.nests[ni].claimed = NONE;
        }
        if self.ai[s].pos != nest.tile {
            self.ai[s].wake = now + 10;
            return;
        }
        let Some(sp) = self.an_sp(s).cloned() else { return };
        let mut r = Rng::for_unit(self.seed, s, now, 0xE6);
        let n = sp.clutch.0 as u32 + r.below((sp.clutch.1.saturating_sub(sp.clutch.0)) as u32 + 1);
        let st = self.animals.st[s];
        let fertile = st.fertile > 0 && now < st.fertile_until;
        if fertile {
            self.animals.st[s].fertile -= 1;
        }
        self.animals.st[s].egg_at = now + EGG_INTERVAL;
        let mat = self.animal_mat(&sp.token, "EGG_YOLK", &format!("{} egg", sp.name));
        let id = self.create_item("EGG", mat, nest.tile as usize, format!("{} eggs", sp.name));
        if let Some(&i) = self.item_idx.get(&id) {
            self.world.items[i as usize].stack = n as i32;
        }
        self.list_consumable(id);
        self.animals.clutches.push(Clutch { item: id, tile: nest.tile, laid: now, fertile, sp: st.sp });
        self.animals.rep.clutches += 1;
        self.animals.rep.eggs_laid += n as u64;
        self.ai[s].wake = now + 100;
    }

    /// Palette entry `CREATURE:<race>:<part>` with a display name (cached).
    fn animal_mat(&mut self, race: &str, part: &str, name: &str) -> u16 {
        let tok = format!("CREATURE:{race}:{part}");
        if let Some(&m) = self.animals.mats.get(&tok) {
            return m;
        }
        let existed = self.world.materials.iter().any(|m| m.token == tok);
        let m = self.ensure_material(&tok);
        if !existed {
            self.world.materials[m as usize].name = name.to_string();
            self.world.materials[m as usize].is_metal = false;
        }
        self.animals.mats.insert(tok, m);
        m
    }

    /// An animal dies or is removed: free whoever held it.
    fn animal_release_holds(&mut self, s: usize) {
        let st = &mut self.animals.st[s];
        st.held = NONE;
        st.led = false;
        st.prod = NONE;
        if st.nest >= 0 {
            let ni = st.nest as usize;
            st.nest = -1;
            if let Some(n) = self.animals.nests.get_mut(ni) {
                n.claimed = NONE;
            }
        }
    }

    // ------------------------------------------------------------------ periodic pass

    /// Per-tick hook: the periodic pass and the daily work.
    pub(crate) fn animals_tick(&mut self, now: u64) {
        if now % ANIMAL_PERIOD != ANIMAL_PHASE {
            if !self.animals.ready && !self.animals.off {
                self.init_animals();
            }
            return;
        }
        let t = std::time::Instant::now();
        self.an_tick_inner(now);
        self.animals.rep.cpu_secs += t.elapsed().as_secs_f64();
    }

    fn an_tick_inner(&mut self, now: u64) {
        if self.animals.off {
            return;
        }
        if !self.animals.ready {
            self.init_animals();
        }
        if now % ANIMAL_PERIOD != ANIMAL_PHASE {
            return;
        }
        self.animals_ensure();
        if self.animals.pens.is_empty() || now % (ANIMAL_PERIOD * 50) == ANIMAL_PHASE {
            // Zones and nest boxes may have been built or removed.
            let pens: Vec<Pen> = self
                .world
                .buildings
                .iter()
                .filter(|b| b.kind == "Civzone" && (b.subtype == "Pen" || b.subtype == "Pasture"))
                .map(|b| Pen { bid: b.id, zone: crate::Zone { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2, z: b.z } })
                .collect();
            self.animals.pens = pens;
            self.animals_refresh_buildings();
        }
        let season = self.world.calendar.month() / 3;
        if season != self.animals.season {
            // Wiki "Fishing": every water body is available again when the season changes.
            self.animals.season = season;
            self.animals.fish_left.clear();
        }
        self.an_breed_pass(now);
        self.an_egg_pass(now);
        self.an_census_pass(now);
        self.an_war_pass(now);
        if now % TICKS_PER_DAY as u64 == ANIMAL_PHASE + 1100 {
            self.an_daily_pass(now);
        }
    }

    fn an_breed_pass(&mut self, now: u64) {
        let n = self.ai.len();
        // Population per species on the map (tame + managed) and children.
        let ns = self.animals.species.len();
        let mut pop = vec![0usize; ns];
        let mut kids = vec![0usize; ns];
        let mut fem: Vec<usize> = Vec::new();
        let mut male: Vec<usize> = Vec::new();
        for s in 0..n {
            if !self.animals_owns(s) {
                continue;
            }
            let sp = self.animals.st[s].sp as usize;
            pop[sp] += 1;
            if !self.an_is_adult(s) {
                kids[sp] += 1;
                continue;
            }
            let st = self.animals.st[s];
            if st.no_breed || st.held != NONE {
                continue;
            }
            match self.world.units[self.ai[s].uidx].sex {
                0 => fem.push(s),
                1 => male.push(s),
                _ => {}
            }
        }
        // Births.
        for s in 0..n {
            let st = self.animals.st.get(s).copied().unwrap_or_default();
            if st.preg_until > 0 && now >= st.preg_until && self.animals_owns(s) {
                self.animals.st[s].preg_until = 0;
                self.an_give_birth(s, now);
            }
        }
        for &f in &fem {
            let st = self.animals.st[f];
            let sp = st.sp as usize;
            let eggs = self.animals.species[sp].eggs;
            if (!eggs && st.preg_until > 0) || (eggs && st.fertile > 0 && now < st.fertile_until) {
                continue;
            }
            if pop[sp] >= POP_CAP || kids[sp] * 4 > pop[sp] * 3 {
                continue;
            }
            let fp = self.nav.xyz(self.ai[f].pos as usize);
            let mate = male.iter().copied().find(|&m| {
                if self.animals.st[m].sp != st.sp {
                    return false;
                }
                let mp = self.nav.xyz(self.ai[m].pos as usize);
                mp.2 == fp.2 && (mp.0 as i32 - fp.0 as i32).abs() <= 1 && (mp.1 as i32 - fp.1 as i32).abs() <= 1
            });
            if mate.is_none() {
                continue;
            }
            let mut r = Rng::for_unit(self.seed, f, now, 0xB4);
            if !r.chance(1, 5) {
                continue;
            }
            let a = &mut self.animals.st[f];
            if eggs {
                a.fertile = 2;
                a.fertile_until = now + FERTILE;
                self.animals.rep.fertilizations += 1;
            } else {
                a.preg_until = now + GESTATION;
                let uidx = self.ai[f].uidx;
                self.world.units[uidx].pregnancy = GESTATION as i32;
                self.animals.rep.pregnancies += 1;
            }
        }
    }

    fn an_give_birth(&mut self, s: usize, now: u64) {
        let Some(sp) = self.an_sp(s).cloned() else { return };
        let mut r = Rng::for_unit(self.seed, s, now, 0xB1);
        // Creature token LITTERSIZE default 1-3; MULTIPLE_LITTER_RARE: 1, 1/500 chance of 1-3.
        let n = if sp.litter_rare && !r.chance(1, 500) { 1 } else { 1 + r.below(3) as usize };
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].pregnancy = 0;
        let pos = self.world.units[uidx].pos;
        let name = self.world.units[uidx].name.clone();
        for _ in 0..n {
            self.an_spawn_young(&sp, pos, &mut r);
        }
        self.animals.rep.births += n as u64;
        let what = if n == 1 { "a young one".to_string() } else { format!("{n} young") };
        self.announce(now, format!("{name} has given birth to {what}."), Some(pos), AnnouncementKind::Info);
    }

    fn an_spawn_young(&mut self, sp: &Species, pos: Pos, r: &mut Rng) -> Option<usize> {
        let id = self.world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let sexed: Vec<&(String, String, i8)> = sp.castes.iter().filter(|c| c.2 >= 0).collect();
        let caste = if sexed.is_empty() { sp.castes.first() } else { Some(sexed[r.below(sexed.len() as u32) as usize]) };
        let (ctok, cname, sex) = caste.cloned().unwrap_or_else(|| ("DEFAULT".into(), sp.name.clone(), -1));
        let walk = self.world.creature_raws.iter().find(|c| c.token == sp.token).and_then(|c| c.castes.iter().find(|k| k.token == ctok)).map(|c| c.walk_speed).unwrap_or(900);
        let unit = Unit {
            id,
            race: sp.token.clone(),
            caste: ctok,
            name: format!("{} (tame)", cap1(&cname)),
            profession: "CHILD".into(),
            pos,
            flags: UnitFlags { animal: true, tame: true, ..Default::default() },
            civ_id: self.world.meta.civ_id,
            hist_fig_id: -1,
            sex,
            age: 0.0,
            body_size: (sp.adult_size / 10).max(1),
            walk_speed: if walk > 0 { walk } else { 900 },
            squad_id: -1,
            squad_position: -1,
            invader_civ: -1,
            pet_owner: -1,
            pasture: -1,
            training: LEVELS[L_DOMESTIC as usize].into(),
            ..Default::default()
        };
        self.world.units.push(unit);
        self.init_units();
        let s = *self.unit_slot.get(&id)?;
        self.animals_ensure();
        let st = &mut self.animals.st[s];
        st.level = L_DOMESTIC;
        st.milk_at = self.ticks + MILK_INTERVAL;
        st.shear_at = self.ticks + SHEAR_REGROW;
        st.egg_at = self.ticks + EGG_INTERVAL;
        Some(s)
    }

    fn an_egg_pass(&mut self, now: u64) {
        // Hatch or forget clutches.
        let mut k = 0;
        while k < self.animals.clutches.len() {
            let c = self.animals.clutches[k];
            let alive = self.item_idx.get(&c.item).map(|&i| {
                let it = &self.world.items[i as usize];
                !self.imeta[i as usize].dead && it.holder < 0 && it.container < 0 && self.world.idx(it.pos) == Some(c.tile as usize) && !self.imeta[i as usize].hauling
            });
            if alive != Some(true) {
                self.animals.clutches.swap_remove(k);
                continue;
            }
            if c.fertile && now >= c.laid + INCUBATION {
                let i = self.item_idx[&c.item] as usize;
                let n = self.world.items[i].stack.max(1) as usize;
                self.destroy_item(c.item);
                if let Some(sp) = self.animals.species.get(c.sp as usize).cloned() {
                    let pos = self.world.pos_of(c.tile as usize);
                    let mut r = Rng::for_unit(self.seed, c.item as usize, now, 0xE7);
                    for _ in 0..n {
                        self.an_spawn_young(&sp, pos, &mut r);
                    }
                    self.animals.rep.eggs_hatched += n as u64;
                    self.animals.rep.births += n as u64;
                    self.announce(now, format!("{n} {} eggs have hatched.", sp.name), Some(pos), AnnouncementKind::Info);
                }
                self.animals.clutches.swap_remove(k);
                continue;
            }
            k += 1;
        }
        if self.animals.nests.is_empty() {
            return;
        }
        // Females ready to lay go to a free nest box.
        for s in 0..self.ai.len() {
            if !self.animals_owns(s) {
                continue;
            }
            let st = self.animals.st[s];
            if st.nest >= 0 || st.held != NONE || now < st.egg_at || !self.animals.species[st.sp as usize].eggs {
                continue;
            }
            if self.world.units[self.ai[s].uidx].sex != 0 || !self.an_is_adult(s) || self.ai[s].waiting {
                continue;
            }
            let c = self.nav.comp_near(self.ai[s].pos as usize);
            let me = self.nav.xyz(self.ai[s].pos as usize);
            let mut best = (i32::MAX, usize::MAX);
            for (ni, n) in self.animals.nests.iter().enumerate() {
                // A nest box with fertile eggs is taken (the hen broods; estimate for infertile ones).
                if n.claimed != NONE || self.animals.clutches.iter().any(|c| c.tile == n.tile && c.fertile) {
                    continue;
                }
                if c == 0 || self.nav.comp_near(n.tile as usize) != c {
                    continue;
                }
                let d = crate::manhattan(me, self.nav.xyz(n.tile as usize));
                if d < best.0 {
                    best = (d, ni);
                }
            }
            if best.1 == usize::MAX {
                continue;
            }
            let ni = best.1;
            self.animals.nests[ni].claimed = s as u32;
            self.animals.st[s].nest = ni as i32;
            let t = self.animals.nests[ni].tile;
            self.request_path(s, Goals::one(t), now);
        }
    }

    fn an_census_pass(&mut self, now: u64) {
        if self.animals.last_census > 0 && now < self.animals.last_census + 4 * ANIMAL_PERIOD {
            return;
        }
        self.animals.last_census = now;
        let mut c = ACensus::default();
        let mut leads = 0;
        let mut slaughter = 0;
        let mut train = 0;
        let mut targets = 0;
        for s in 0..self.ai.len() {
            let ai = &self.ai[s];
            if ai.kind == Kind::Dead && !(self.world.units[ai.uidx].flags.caged && !self.world.units[ai.uidx].flags.dead) {
                continue;
            }
            let u = &self.world.units[ai.uidx];
            if !u.flags.animal {
                continue;
            }
            let st = self.animals.st[s];
            if self.animals_owns(s) {
                let sp = &self.animals.species[st.sp as usize];
                let free = st.held == NONE && st.prod == NONE;
                if free && sp.milkable && u.sex == 0 && now >= st.milk_at {
                    c.milk_ready += 1;
                }
                if free && sp.shear && now >= st.shear_at {
                    c.shear_ready += 1;
                }
                if free && st.lead_req {
                    leads += 1;
                }
                if free && u.slaughter && self.an_pet_owner_slot(s).is_none() {
                    slaughter += 1;
                }
            }
            if st.sp != NO_SP && st.req != 0 && st.held == NONE && !u.flags.dead {
                train += 1;
            }
            if !u.flags.tame && !u.flags.hostile && !u.flags.caged && ai.kind == Kind::Animal && st.sp != NO_SP && self.animals.species[st.sp as usize].huntable {
                targets += 1;
            }
        }
        let mut gear = (false, false);
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            match it.kind.as_str() {
                "BUCKET" => {
                    if self.item_free(idx, now) && self.container_empty(it.id) {
                        c.buckets += 1;
                    }
                }
                "LIQUID_MISC" => {
                    if self.an_is_milk(idx) && self.item_free(idx, now) {
                        c.milk += 1;
                    }
                }
                "CORPSEPIECE" => {
                    if self.an_is_wool(idx) && self.item_free(idx, now) {
                        c.wool += 1;
                    }
                }
                "FISH_RAW" => {
                    if self.item_free(idx, now) && !it.flags.rotten {
                        c.raw_fish += 1;
                    }
                }
                "WEAPON" if it.subtype == "ITEM_WEAPON_CROSSBOW" => gear.0 |= self.item_free(idx, now),
                "AMMO" if it.subtype == "ITEM_AMMO_BOLTS" => gear.1 |= self.item_free(idx, now),
                _ => {}
            }
        }
        let a = &mut self.animals;
        a.census = c;
        a.open_lead = leads;
        a.open_slaughter = slaughter;
        a.open_train = train;
        a.hunt_targets = targets;
        a.hunt_gear = gear.0 && gear.1;
        self.an_auto_jobs();
    }

    fn an_is_milk(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "LIQUID_MISC" && self.world.material(it.mat).map(|m| m.token.ends_with(":MILK")).unwrap_or(false)
    }

    fn an_is_wool(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "CORPSEPIECE" && !it.flags.rotten && (it.desc.ends_with(" wool") || it.desc.ends_with(" hair"))
    }

    /// DF standing orders "auto butcher" and "auto fishery": queue the job when there is work.
    fn an_auto_jobs(&mut self) {
        let fish = self.animals.census.raw_fish > 0;
        let mut changed = false;
        for si in 0..self.prod.shops.len() {
            let (bi, sub) = (self.prod.shops[si].bi, self.prod.shops[si].subtype.clone());
            let job = match sub.as_str() {
                "Fishery" if fish => "PrepareRawFish",
                _ => continue,
            };
            let Some(b) = self.world.buildings.get_mut(bi) else { continue };
            if b.jobs.len() < crate::production::MAX_QUEUE && !b.jobs.iter().any(|j| j.job == job) {
                b.jobs.push(BuildingJob { job: job.into(), order_id: -1, ..Default::default() });
                changed = true;
            }
        }
        if changed {
            self.mark_prod_dirty();
        }
    }

    /// Queue "Butcher a dead animal" at the shop with tile `site` (auto butcher).
    fn an_queue_butcher(&mut self, site: u32) {
        let Some(si) = self.prod.shops.iter().position(|sh| sh.site == site) else { return };
        let bi = self.prod.shops[si].bi;
        if let Some(b) = self.world.buildings.get_mut(bi) {
            let queued = b.jobs.iter().filter(|j| j.job == "ButcherAnimal").count();
            if b.jobs.len() < crate::production::MAX_QUEUE && queued < 3 {
                b.jobs.push(BuildingJob { job: "ButcherAnimal".into(), order_id: -1, ..Default::default() });
            }
        }
        self.mark_prod_dirty();
    }

    /// War animals attack hostiles near them (and near their dwarf).
    fn an_war_pass(&mut self, now: u64) {
        if self.combat.hostiles.is_empty() {
            return;
        }
        for s in 0..self.ai.len() {
            if !self.animals_owns(s) || self.animals.st[s].role != ROLE_WAR || self.combat_owns(s) {
                continue;
            }
            let me = self.nav.xyz(self.ai[s].pos as usize);
            let c = self.nav.comp_near(self.ai[s].pos as usize);
            let mut best = (i32::MAX, NONE);
            for &h in &self.combat.hostiles {
                let hs = h as usize;
                if self.ai[hs].kind == Kind::Dead || !self.can_fight(hs, now) {
                    continue;
                }
                let hp = self.nav.xyz(self.ai[hs].pos as usize);
                let cheb = (hp.0 as i32 - me.0 as i32).abs().max((hp.1 as i32 - me.1 as i32).abs());
                if cheb > 10 || (hp.2 as i32 - me.2 as i32).abs() > 1 || self.nav.comp_near(self.ai[hs].pos as usize) != c {
                    continue;
                }
                let d = crate::manhattan(me, hp);
                if d < best.0 {
                    best = (d, h);
                }
            }
            if best.1 != NONE {
                self.animal_attack(s, best.1, now);
            }
        }
    }

    fn animal_attack(&mut self, s: usize, target: u32, now: u64) {
        if s >= self.combat.f.len() {
            self.combat_sync_units();
        }
        self.animal_release_holds(s);
        let f = &mut self.combat.f[s];
        f.role = crate::combat::Role::Fighter;
        f.target = target;
        self.ai[s].path.clear();
        self.ai[s].path_i = 0;
        self.ai[s].wake = now + 1;
        self.animals.rep.war_attacks += 1;
    }

    fn an_daily_pass(&mut self, now: u64) {
        self.an_regrow_grass(TICKS_PER_DAY as u64);
        let day = 1.0 / (rf_core::DAYS_PER_MONTH as f32 * 12.0);
        let n = self.ai.len();
        for s in 0..n {
            if !self.animals_owns(s) {
                continue;
            }
            // Age and growth of the young.
            let sp = self.animals.st[s].sp as usize;
            let adult = self.animals.species[sp].adult_size;
            let uidx = self.ai[s].uidx;
            let u = &mut self.world.units[uidx];
            let was_child = u.age < CHILD_YEARS;
            u.age += day;
            if was_child {
                let f = (u.age / CHILD_YEARS).min(1.0);
                u.body_size = ((adult as f32 / 10.0) + f * (adult as f32 * 0.9)) as i32;
                if u.age >= CHILD_YEARS && u.profession == "CHILD" {
                    u.profession = "STANDARD".into();
                }
            }
            u.hunger_timer = self.animals.st[s].hunger;
            // Training wears off (adults, not domesticated).
            let st = self.animals.st[s];
            if st.level >= L_SEMI && st.level < L_DOMESTIC && now >= st.last_train + REVERT_AFTER && !was_child {
                let lvl = st.level - 1;
                self.animals.st[s].level = lvl;
                self.animals.st[s].last_train = now;
                self.world.units[uidx].training = LEVELS[lvl as usize].into();
                if lvl == L_SEMI {
                    let name = self.world.units[uidx].name.clone();
                    let pos = self.world.units[uidx].pos;
                    self.announce(now, format!("{name} has reverted to a semi-wild state."), Some(pos), AnnouncementKind::Warning);
                    self.animals.rep.reverted += 1;
                    if st.req & REQ_TAME == 0 {
                        self.animals.st[s].req |= REQ_TAME;
                    }
                }
                if lvl == L_WILD {
                    self.world.units[uidx].flags.tame = false;
                    self.animals.st[s].role = ROLE_NONE;
                }
            }
            let mut r = Rng::for_unit(self.seed, s, now, 0xDA);
            // Cats kill vermin (estimate 1/14 per day).
            if self.animals.species[sp].vermin && self.an_is_adult(s) && r.chance(1, 14) {
                self.animals.rep.vermin_killed += 1;
                let t = self.ai[s].pos as usize;
                let m = self.animal_mat("RAT", "REMAINS", "rat remains");
                self.create_item("REMAINS", m, t, "rat remains".into());
            }
            self.an_pet_day(s, now, &mut r);
        }
    }

    /// Adoption and pet thoughts (once per day per animal).
    fn an_pet_day(&mut self, s: usize, now: u64, r: &mut Rng) {
        let sp = self.animals.st[s].sp as usize;
        if !self.animals.species[sp].pet {
            return;
        }
        let uidx = self.ai[s].uidx;
        if let Some(os) = self.an_pet_owner_slot(s) {
            let (a, b) = (self.world.units[uidx].pos, self.world.units[self.ai[os].uidx].pos);
            if a.z == b.z && a.dist(b) <= 3 && r.chance(1, 7) {
                self.feel(os, "InteractPet", crate::social::Class::PosMild, 60, -1, 0, now);
                self.animals.rep.pet_thoughts += 1;
            }
            return;
        }
        if self.world.units[uidx].slaughter || self.animals.st[s].role != ROLE_NONE || self.animals.st[s].held != NONE {
            return;
        }
        let adopts = self.animals.species[sp].adopts;
        let me = self.world.units[uidx].pos;
        let mut cand = NONE;
        for (k, ai) in self.ai.iter().enumerate() {
            if ai.kind != Kind::Worker {
                continue;
            }
            let u = &self.world.units[ai.uidx];
            if !u.flags.citizen || u.pos.z != me.z || u.pos.dist(me) > 3 {
                continue;
            }
            // Cats choose anybody; other pets are adopted by idle dwarves.
            if adopts || matches!(ai.task, Task::Idle | Task::Wander) {
                cand = k as u32;
                break;
            }
        }
        if cand == NONE || !r.chance(1, 20) {
            return;
        }
        let os = cand as usize;
        let oid = self.ai[os].id;
        self.world.units[uidx].pet_owner = oid;
        self.feel(os, "AdoptedPet", crate::social::Class::Pos, 150, -1, 0, now);
        self.animals.rep.adoptions += 1;
        let (an, on) = (self.world.units[uidx].name.clone(), self.world.units[self.ai[os].uidx].name.clone());
        self.announce(now, format!("{on} has adopted {an}."), Some(me), AnnouncementKind::Info);
    }

    // ------------------------------------------------------------------ workshop jobs (Ind)

    /// [animals] Are the inputs of a farmer's workshop / fishery job in stock?
    pub(crate) fn animals_available(&self, f: u8) -> bool {
        let c = &self.animals.census;
        match f {
            F_MILK => c.milk_ready > 0 && c.buckets > 0,
            F_SHEAR => c.shear_ready > 0,
            F_CHEESE => c.milk > 0,
            F_SPIN => c.wool > 0,
            F_CLEAN_FISH => c.raw_fish > 0,
            _ => false,
        }
    }

    /// [animals] Pick the animal and items for a farmer's workshop / fishery job.
    pub(crate) fn animals_claim(&mut self, s: usize, f: u8, site: u32, c: u32, now: u64, wj: &mut WJob) -> bool {
        let mut found = Vec::new();
        match f {
            F_MILK | F_SHEAR => {
                let Some(a) = self.an_ready_animal(f, site, c, now) else { return false };
                if f == F_MILK {
                    self.find_items(site, c, now, 1, &mut found, |sim, i| {
                        let it = &sim.world.items[i];
                        it.kind == "BUCKET" && it.container < 0 && sim.container_empty(it.id)
                    });
                    let Some(&b) = found.first() else { return false };
                    wj.items[0] = b;
                    wj.need[0] = 1;
                    wj.n = 1;
                }
                self.animals.st[a].prod = s as u32;
                // The animal is led to the workshop (it walks there itself, see the estimates).
                let g = self.an_near_goals(site);
                if g.n > 0 {
                    self.request_path(a, g, now);
                }
                true
            }
            F_CHEESE | F_SPIN | F_CLEAN_FISH => {
                // Milk and raw fish have no production class, so `find_items` does not see them.
                let id = self.an_find_item(site, c, now, |sim, i| match f {
                    F_CHEESE => sim.an_is_milk(i),
                    F_SPIN => sim.an_is_wool(i),
                    _ => sim.world.items[i].kind == "FISH_RAW" && !sim.world.items[i].flags.rotten,
                });
                let Some(id) = id else { return false };
                wj.items[0] = id;
                wj.need[0] = 1;
                wj.n = 1;
                true
            }
            _ => false,
        }
    }

    /// Nearest free item matching `pred` in component `c` (any production class).
    fn an_find_item(&self, site: u32, c: u32, now: u64, pred: impl Fn(&Sim, usize) -> bool) -> Option<i32> {
        let sp = self.nav.xyz(site as usize);
        let mut best = (i32::MAX, -1);
        for idx in 0..self.world.items.len() {
            if !pred(self, idx) || !self.item_free(idx, now) {
                continue;
            }
            let it = &self.world.items[idx];
            let Some(t) = self.world.idx(it.pos) else { continue };
            let d = crate::manhattan(sp, self.nav.xyz(t));
            if (d, it.id) < best && self.nav.comp_near(t) == c {
                best = (d, it.id);
            }
        }
        (best.1 >= 0).then_some(best.1)
    }

    fn an_ready_animal(&self, f: u8, site: u32, c: u32, now: u64) -> Option<usize> {
        let sp_ = self.nav.xyz(site as usize);
        let mut best = (i32::MAX, usize::MAX);
        for s in 0..self.ai.len() {
            if !self.animals_owns(s) {
                continue;
            }
            let st = self.animals.st[s];
            if st.held != NONE || st.prod != NONE || st.nest >= 0 {
                continue;
            }
            let sp = &self.animals.species[st.sp as usize];
            let ok = if f == F_MILK { sp.milkable && self.world.units[self.ai[s].uidx].sex == 0 && now >= st.milk_at } else { sp.shear && now >= st.shear_at };
            if !ok || (c != 0 && self.nav.comp_near(self.ai[s].pos as usize) != c) {
                continue;
            }
            let d = crate::manhattan(sp_, self.nav.xyz(self.ai[s].pos as usize));
            if d < best.0 {
                best = (d, s);
            }
        }
        (best.1 != usize::MAX).then_some(best.1)
    }

    /// The animal a worker is milking/shearing.
    fn an_prod_animal(&self, w: usize) -> Option<usize> {
        (0..self.animals.st.len()).find(|&a| self.animals.st[a].prod == w as u32)
    }

    /// [animals] Make the products of a finished farmer's workshop / fishery job.
    pub(crate) fn animals_finish(&mut self, s: usize, wj: &WJob, f: u8, now: u64) {
        let site = wj.site as usize;
        match f {
            F_MILK => {
                let bucket = wj.items[0];
                if let Some(a) = self.an_prod_animal(s) {
                    self.animals.st[a].prod = NONE;
                    self.animals.st[a].milk_at = now + MILK_INTERVAL;
                    let race = self.world.units[self.ai[a].uidx].race.clone();
                    let name = self.animals.species[self.animals.st[a].sp as usize].name.clone();
                    let mat = self.animal_mat(&race, "MILK", &format!("{name}'s milk"));
                    if self.item_idx.contains_key(&bucket) {
                        self.an_put_in_container(bucket, "LIQUID_MISC", mat, 1, format!("{name}'s milk"));
                    } else {
                        self.create_item("LIQUID_MISC", mat, site, format!("{name}'s milk"));
                    }
                    self.animals.rep.milkings += 1;
                }
                self.release_reagent(bucket);
            }
            F_SHEAR => {
                if let Some(a) = self.an_prod_animal(s) {
                    self.animals.st[a].prod = NONE;
                    self.animals.st[a].shear_at = now + SHEAR_REGROW;
                    let race = self.world.units[self.ai[a].uidx].race.clone();
                    let name = self.animals.species[self.animals.st[a].sp as usize].name.clone();
                    let mat = self.animal_mat(&race, "HAIR", &format!("{name} wool"));
                    self.create_item("CORPSEPIECE", mat, site, format!("{name} wool"));
                    self.animals.rep.shearings += 1;
                }
            }
            F_CHEESE | F_SPIN | F_CLEAN_FISH => {
                let id = wj.items[0];
                let Some(&i) = self.item_idx.get(&id) else { return };
                let (mat, desc) = (self.world.items[i as usize].mat, self.world.items[i as usize].desc.clone());
                let tok = self.world.material(mat).map(|m| m.token.clone()).unwrap_or_default();
                let race = tok.split(':').nth(1).unwrap_or("ANIMAL").to_string();
                self.destroy_item(id);
                match f {
                    F_CHEESE => {
                        let base = desc.replace("'s milk", "").replace(" milk", "");
                        let m = self.animal_mat(&race, "CHEESE", &format!("{base} cheese"));
                        let nid = self.create_item("CHEESE", m, site, format!("{base} cheese"));
                        self.list_consumable(nid);
                        self.animals.rep.cheese += 1;
                    }
                    F_SPIN => {
                        let nid = self.create_item("THREAD", mat, site, format!("{desc} yarn"));
                        let _ = nid;
                        self.animals.rep.thread += 1;
                    }
                    _ => {
                        let base = desc.trim_start_matches("raw ").trim().trim_end_matches(',').to_string();
                        let m = self.animal_mat(&race, "MUSCLE", &base);
                        let nid = self.create_item("FISH", m, site, base);
                        if let Some(&ni) = self.item_idx.get(&nid) {
                            self.world.items[ni as usize].stack = 2;
                        }
                        self.list_consumable(nid);
                        self.animals.rep.fish_cleaned += 1;
                    }
                }
            }
            _ => {}
        }
    }

    /// Put a new item into a container (milk into a bucket).
    fn an_put_in_container(&mut self, cont: i32, kind: &str, mat: u16, stack: i32, desc: String) {
        let Some(&ci) = self.item_idx.get(&cont) else { return };
        let ci = ci as usize;
        let pos = self.world.items[ci].pos;
        let id = self.next_item_id;
        self.next_item_id += 1;
        self.push_item(Item {
            id,
            kind: kind.into(),
            mat,
            pos,
            stack,
            container: cont,
            holder: -1,
            building: -1,
            desc,
            flags: ItemFlags { in_container: true, ..Default::default() },
            ..Default::default()
        });
        self.contents.entry(cont).or_default().push(id);
        self.set_item_cat(ci, crate::items::Cat::Food);
    }

    // ------------------------------------------------------------------ worker jobs

    /// [animals] Lead to pasture, slaughter, training (job-like work; before other jobs).
    pub(crate) fn animals_decide(&mut self, s: usize, now: u64) -> bool {
        let a = &self.animals;
        if a.off || !a.ready || (a.open_lead == 0 && a.open_slaughter == 0 && a.open_train == 0) {
            return false;
        }
        let labors = self.ai[s].labors;
        let want = (a.open_lead > 0 && labors & labor::HAUL_ANIMALS != 0)
            || (a.open_slaughter > 0 && labors & labor::BUTCHER != 0)
            || (a.open_train > 0 && labors & labor::ANIMALTRAIN != 0);
        if !want || now < self.animals.next_try.get(s).copied().unwrap_or(0) {
            return false;
        }
        if self.animals.st.len() < self.ai.len() {
            self.animals_ensure();
        }
        self.animals.next_try[s] = now + 300 + (s as u64 % 50);
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return false;
        }
        let me = self.nav.xyz(self.ai[s].pos as usize);
        let butcher_site = self.prod.shops.iter().find(|sh| sh.subtype == "Butchers" && self.nav.comp_near(sh.site as usize) == c).map(|sh| sh.site);
        let mut best = (i32::MAX, usize::MAX, WKind::None);
        for a in 0..self.ai.len() {
            let st = self.animals.st[a];
            if st.sp == NO_SP || st.held != NONE || st.prod != NONE || now < st.retry {
                continue;
            }
            let u = &self.world.units[self.ai[a].uidx];
            let caged = u.flags.caged && !u.flags.dead;
            let kind = if self.animals_owns(a) && st.lead_req && labors & labor::HAUL_ANIMALS != 0 {
                WKind::Lead
            } else if self.animals_owns(a) && u.slaughter && butcher_site.is_some() && labors & labor::BUTCHER != 0 && self.an_pet_owner_slot(a).is_none() {
                WKind::Slaughter
            } else if st.req != 0 && labors & labor::ANIMALTRAIN != 0 && (self.animals_owns(a) || caged) && self.an_train_due(a, now) {
                WKind::Train
            } else {
                continue;
            };
            let t = self.ai[a].pos as usize;
            if self.nav.comp_near(t) != c {
                continue;
            }
            let d = crate::manhattan(me, self.nav.xyz(t));
            if d < best.0 {
                best = (d, a, kind);
            }
        }
        if best.1 == usize::MAX {
            return false;
        }
        let (a, kind) = (best.1, best.2);
        let dest = match kind {
            WKind::Slaughter => butcher_site.unwrap_or(0),
            WKind::Lead => match self.an_pen_tile(a, now) {
                Some(t) => t,
                None => {
                    self.animals.st[a].lead_req = false;
                    return false;
                }
            },
            _ => 0,
        };
        self.animals.w[s] = AWork { kind, step: Step::GoAnimal, target: a as u32, dest, ..Default::default() };
        self.animals.st[a].held = s as u32;
        self.ai[s].task = Task::Animal;
        let g = self.an_near_goals(self.ai[a].pos);
        self.request_path(s, g, now);
        true
    }

    /// A walkable tile inside the animal's pasture.
    fn an_pen_tile(&self, a: usize, now: u64) -> Option<u32> {
        let pen = self.an_pen_of(a)?;
        let z = pen.zone;
        let mut r = Rng::for_unit(self.seed, a, now, 0x9E);
        let w = (z.x2 - z.x1 + 1) as u32;
        let h = (z.y2 - z.y1 + 1) as u32;
        let b = self.world.building(pen.bid)?;
        for _ in 0..30 {
            let p = Pos::new(z.x1 + r.below(w) as i16, z.y1 + r.below(h) as i16, z.z);
            if let Some(t) = self.world.idx(p) {
                if self.nav.walkable(t) && b.contains(p) {
                    return Some(t as u32);
                }
            }
        }
        None
    }

    fn an_train_due(&self, a: usize, now: u64) -> bool {
        let st = self.animals.st[a];
        if st.req & (REQ_WAR | REQ_HUNT) != 0 && self.an_is_adult(a) {
            return true;
        }
        if st.req & REQ_TAME != 0 {
            return st.level < L_TRAINED || (st.level < L_DOMESTIC && now >= st.last_train + RETRAIN_AFTER);
        }
        false
    }

    /// [animals] Idle work: hunting and fishing (DF creates these jobs for idle dwarves).
    pub(crate) fn animals_idle_work(&mut self, s: usize, now: u64) -> bool {
        let labors = self.ai[s].labors;
        if labors & (labor::HUNT | labor::FISH) == 0 || self.animals.off || !self.animals.ready {
            return false;
        }
        if now < self.animals.next_try.get(s).copied().unwrap_or(0) {
            return false;
        }
        if self.animals.st.len() < self.ai.len() {
            self.animals_ensure();
        }
        self.animals.next_try[s] = now + 400 + (s as u64 % 64);
        if labors & labor::HUNT != 0 && self.animals.hunt_targets > 0 && (self.animals.hunt_gear || self.an_own_gear(s).0 >= 0) && self.an_start_hunt(s, now) {
            return true;
        }
        labors & labor::FISH != 0 && self.an_start_fishing(s, now)
    }

    fn an_start_hunt(&mut self, s: usize, now: u64) -> bool {
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return false;
        }
        let me = self.nav.xyz(self.ai[s].pos as usize);
        let mut best = (i32::MAX, usize::MAX);
        for t in 0..self.ai.len() {
            if self.ai[t].kind != Kind::Animal || self.combat_owns(t) || self.trade_owns(t) {
                continue;
            }
            let u = &self.world.units[self.ai[t].uidx];
            if u.flags.tame || u.flags.hostile || u.flags.caged || u.flags.dead || u.flags.inactive {
                continue;
            }
            let st = self.animals.st[t];
            if st.sp == NO_SP || !self.animals.species[st.sp as usize].huntable || st.held != NONE || now < st.retry {
                continue;
            }
            let tp = self.ai[t].pos as usize;
            if self.nav.comp_near(tp) != c {
                continue;
            }
            let d = crate::manhattan(me, self.nav.xyz(tp));
            if d < best.0 {
                best = (d, t);
            }
        }
        if best.1 == usize::MAX {
            return false;
        }
        // The hunter's own crossbow and quiver bolts (soldiers carry theirs), else free ones from
        // a stockpile, reserved (see the estimates).
        let (own_bow, own_bolts) = self.an_own_gear(s);
        let (mut bow, mut bolts) = (own_bow, own_bolts);
        if bow < 0 || bolts < 0 {
            for idx in 0..self.world.items.len() {
                let it = &self.world.items[idx];
                if bow < 0 && it.kind == "WEAPON" && it.subtype == "ITEM_WEAPON_CROSSBOW" && self.item_free(idx, now) {
                    bow = it.id;
                } else if bolts < 0 && it.kind == "AMMO" && it.subtype == "ITEM_AMMO_BOLTS" && self.item_free(idx, now) {
                    bolts = it.id;
                }
                if bow >= 0 && bolts >= 0 {
                    break;
                }
            }
        }
        if bow < 0 || bolts < 0 {
            if own_bow < 0 {
                self.animals.hunt_gear = false;
            }
            return false;
        }
        let own = bolts == own_bolts;
        let res_bow = if bow == own_bow { -1 } else { bow };
        if res_bow >= 0 {
            self.reserve(res_bow);
        }
        if !own {
            self.reserve(bolts);
        }
        let t = best.1;
        self.animals.st[t].held = s as u32;
        self.animals.w[s] = AWork { kind: WKind::Hunt, step: Step::Stalk, target: t as u32, item: res_bow, item2: bolts, own, ..Default::default() };
        self.ai[s].task = Task::Animal;
        self.animals.rep.hunts += 1;
        let g = self.an_near_goals(self.ai[t].pos);
        self.request_path(s, g, now);
        true
    }

    /// Crossbow and bolts a unit carries (bolts loose or in a quiver), -1 = none.
    fn an_own_gear(&self, s: usize) -> (i32, i32) {
        let u = &self.world.units[self.ai[s].uidx];
        let (mut bow, mut bolts) = (-1, -1);
        let ok = |id: i32| self.item_idx.get(&id).map(|&i| !self.imeta[i as usize].dead && self.world.items[i as usize].stack > 0).unwrap_or(false);
        for &id in &u.inventory {
            let Some(&i) = self.item_idx.get(&id) else { continue };
            let it = &self.world.items[i as usize];
            if it.kind == "WEAPON" && it.subtype == "ITEM_WEAPON_CROSSBOW" && ok(id) {
                bow = id;
            } else if it.kind == "AMMO" && it.subtype == "ITEM_AMMO_BOLTS" && ok(id) && bolts < 0 {
                bolts = id;
            } else if it.kind == "QUIVER" && bolts < 0 {
                if let Some(c) = self.contents.get(&id) {
                    bolts = c.iter().copied().find(|&b| ok(b) && self.world.items[self.item_idx[&b] as usize].subtype == "ITEM_AMMO_BOLTS").unwrap_or(-1);
                }
            }
        }
        (bow, bolts)
    }

    fn an_start_fishing(&mut self, s: usize, now: u64) -> bool {
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return false;
        }
        let me = self.nav.xyz(self.ai[s].pos as usize);
        // Wiki "Fishing": fishing zones are preferred (default standing order).
        let mut best = (i32::MAX, NONE, -1);
        let spots: &[(u32, i32)] = &self.animals.fish_spots;
        for &(t, z) in spots {
            if !self.nav.walkable(t as usize) || self.nav.comp_root(t as usize) != c || self.an_fish_left(z) <= 0 {
                continue;
            }
            let d = crate::manhattan(me, self.nav.xyz(t as usize));
            if d < best.0 {
                best = (d, t, z);
            }
        }
        if best.1 == NONE && self.animals.fish_spots.is_empty() {
            for &t in &self.water {
                if !self.nav.walkable(t as usize) || self.nav.comp_root(t as usize) != c {
                    continue;
                }
                let z = self.an_water_body(t);
                if self.an_fish_left(z) <= 0 {
                    continue;
                }
                let d = crate::manhattan(me, self.nav.xyz(t as usize));
                if d < best.0 {
                    best = (d, t, z);
                }
            }
        }
        if best.1 == NONE {
            return false;
        }
        self.animals.w[s] = AWork { kind: WKind::Fish, step: Step::GoWater, dest: best.1, zone: best.2, ..Default::default() };
        self.ai[s].task = Task::Animal;
        self.request_path(s, Goals::one(best.1), now);
        true
    }

    /// Water body key for depletion: -2 = cavern water, -3 = other water.
    fn an_water_body(&self, t: u32) -> i32 {
        if self.world.tiles[t as usize].flags & tile_flags::SUBTERRANEAN != 0 && self.is_cavern_floor(t as usize) {
            -2
        } else {
            -3
        }
    }

    fn an_fish_left(&self, zone: i32) -> i32 {
        self.animals.fish_left.iter().find(|e| e.0 == zone).map(|e| e.1).unwrap_or(FISH_PER_SEASON)
    }

    fn an_take_fish(&mut self, zone: i32) {
        match self.animals.fish_left.iter_mut().find(|e| e.0 == zone) {
            Some(e) => e.1 -= 1,
            None => self.animals.fish_left.push((zone, FISH_PER_SEASON - 1)),
        }
    }

    /// [animals] Think step of a worker with `Task::Animal`.
    pub(crate) fn animal_work_think(&mut self, s: usize, now: u64, urgent: bool) {
        if urgent {
            self.cancel_task(s);
            self.decide(s, now);
            return;
        }
        if self.ai[s].waiting {
            return;
        }
        let w = self.animals.w[s];
        if w.kind == WKind::Hunt && w.step == Step::Stalk && self.an_in_range(s, w.target as usize) {
            self.ai[s].path.clear();
            self.ai[s].path_i = 0;
            self.animals.w[s].step = Step::Shoot;
            self.animals.w[s].until = now + SHOT_TICKS;
            self.an_hunting_animals_join(s, w.target, now);
            self.ai[s].wake = now + SHOT_TICKS;
            return;
        }
        if self.ai[s].path_i < self.ai[s].path.len() {
            self.step(s, now);
            return;
        }
        self.animal_work_arrive(s, now);
    }

    fn an_in_range(&self, s: usize, t: usize) -> bool {
        if self.ai[t].kind == Kind::Dead {
            return false;
        }
        let (a, b) = (self.nav.xyz(self.ai[s].pos as usize), self.nav.xyz(self.ai[t].pos as usize));
        a.2 == b.2 && (a.0 as i32 - b.0 as i32).abs().max((a.1 as i32 - b.1 as i32).abs()) <= SHOOT_RANGE
    }

    fn an_adjacent(&self, s: usize, t: usize) -> bool {
        let (a, b) = (self.nav.xyz(self.ai[s].pos as usize), self.nav.xyz(self.ai[t].pos as usize));
        a.2 == b.2 && (a.0 as i32 - b.0 as i32).abs() <= 1 && (a.1 as i32 - b.1 as i32).abs() <= 1
    }

    fn an_work_done(&mut self, s: usize, now: u64) {
        let w = self.animals.w[s];
        if (w.target as usize) < self.animals.st.len() && self.animals.st[w.target as usize].held == s as u32 {
            self.animals.st[w.target as usize].held = NONE;
            self.animals.st[w.target as usize].led = false;
        }
        if w.item >= 0 {
            self.unreserve(w.item);
        }
        if w.item2 >= 0 && !w.own {
            self.unreserve(w.item2);
        }
        self.animals.w[s] = AWork::default();
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    fn animal_work_arrive(&mut self, s: usize, now: u64) {
        let w = self.animals.w[s];
        let t = w.target as usize;
        match w.step {
            Step::GoAnimal => {
                let alive = t < self.ai.len() && (self.ai[t].kind != Kind::Dead || self.world.units[self.ai[t].uidx].flags.caged) && !self.world.units[self.ai[t].uidx].flags.dead;
                if !alive {
                    self.an_work_done(s, now);
                    return;
                }
                if !self.an_adjacent(s, t) {
                    if w.tries >= 3 {
                        self.animals.st[t].retry = now + TICKS_PER_DAY as u64;
                        self.an_work_done(s, now);
                        return;
                    }
                    self.animals.w[s].tries += 1;
                    let g = self.an_near_goals(self.ai[t].pos);
                    self.request_path(s, g, now);
                    return;
                }
                match w.kind {
                    WKind::Lead | WKind::Slaughter => {
                        self.animals.st[t].led = true;
                        self.animals.w[s].step = Step::Leading;
                        let g = if w.kind == WKind::Slaughter { self.an_near_goals(w.dest) } else { Goals::one(w.dest) };
                        self.request_path(s, g, now);
                    }
                    WKind::Train => {
                        let skill = self.world.units[self.ai[s].uidx].skill("ANIMALTRAIN");
                        let rating = self.social_rating(s, skill);
                        let dur = (TRAIN_TICKS as i32 * 10 / (10 + 2 * rating.clamp(0, 20))).max(10) as u64;
                        self.animals.w[s].step = Step::Training;
                        self.animals.w[s].until = now + dur;
                        self.ai[s].wake = now + dur.min(self.need_cap(s));
                    }
                    _ => self.an_work_done(s, now),
                }
            }
            Step::Leading => {
                if !(t < self.ai.len() && self.ai[t].kind != Kind::Dead) {
                    self.an_work_done(s, now);
                    return;
                }
                let here = self.ai[s].pos;
                self.an_place_unit(t, here);
                if w.kind == WKind::Lead {
                    self.animals.st[t].lead_req = false;
                    self.animals.rep.led_to_pasture += 1;
                    self.an_work_done(s, now);
                } else {
                    self.an_slaughter_at_shop(s, t, w.dest, now);
                    self.an_work_done(s, now);
                }
            }
            Step::Training => {
                if now < w.until {
                    self.ai[s].wake = w.until.min(now + self.need_cap(s));
                    return;
                }
                self.an_finish_training(s, t, now);
                self.an_work_done(s, now);
            }
            Step::Stalk => {
                if !(t < self.ai.len() && self.ai[t].kind == Kind::Animal) {
                    self.an_work_done(s, now);
                    return;
                }
                if w.tries >= 8 {
                    self.animals.st[t].retry = now + TICKS_PER_DAY as u64;
                    self.an_work_done(s, now);
                    return;
                }
                self.animals.w[s].tries += 1;
                let g = self.an_near_goals(self.ai[t].pos);
                self.request_path(s, g, now);
            }
            Step::Shoot => self.an_hunt_shoot(s, t, now),
            Step::GoCorpse => {
                let id = w.item2;
                if !self.item_reachable_here(s, id) {
                    self.animals.w[s].item2 = -1;
                    self.an_work_done(s, now);
                    return;
                }
                self.pickup(s, id);
                self.haul_remove(id);
                self.animals.w[s].item2 = -1;
                self.animals.w[s].hits = id;
                self.animals.w[s].step = Step::Carry;
                let c = self.nav.comp_near(self.ai[s].pos as usize);
                match self.prod.shops.iter().find(|sh| sh.subtype == "Butchers" && self.nav.comp_near(sh.site as usize) == c).map(|sh| sh.site) {
                    Some(site) => {
                        self.animals.w[s].dest = site;
                        let g = self.an_site_goals_pub(site, c);
                        self.request_path(s, g, now);
                    }
                    None => {
                        let here = self.ai[s].pos;
                        self.drop_item(s, id, here);
                        self.an_work_done(s, now);
                    }
                }
            }
            Step::Carry => {
                let id = w.hits;
                let site = w.dest;
                let drop = if self.nav.walkable(site as usize) { site } else { self.ai[s].pos };
                self.drop_item(s, id, drop);
                self.an_queue_butcher(site);
                self.animals.w[s].hits = 0;
                self.an_work_done(s, now);
            }
            Step::GoWater => {
                let skill = self.world.units[self.ai[s].uidx].skill("FISH");
                let rating = self.social_rating(s, skill);
                let dur = (FISH_TICKS as i32 * 10 / (10 + 2 * rating.clamp(0, 20))).max(10) as u64;
                self.animals.w[s].step = Step::Fishing;
                self.animals.w[s].until = now + dur;
                self.ai[s].wake = now + dur.min(self.need_cap(s));
                self.animals.rep.fishing_trips += 1;
            }
            Step::Fishing => {
                if now < w.until {
                    self.ai[s].wake = w.until.min(now + self.need_cap(s));
                    return;
                }
                self.an_finish_fishing(s, w.zone, now);
                self.an_work_done(s, now);
            }
            Step::None => self.an_work_done(s, now),
        }
    }

    fn an_site_goals_pub(&self, site: u32, c: u32) -> Goals {
        let ok = |q: usize| self.nav.walkable(q) && (c == 0 || self.nav.comp_root(q) == c);
        if ok(site as usize) {
            return Goals::one(site);
        }
        let mut g = Goals::default();
        for q in self.an_near_goals(site).as_slice() {
            if ok(*q as usize) {
                g.push(*q);
            }
        }
        g
    }

    fn an_slaughter_at_shop(&mut self, s: usize, a: usize, site: u32, now: u64) {
        let sp = self.an_sp(a).cloned();
        self.animal_release_holds(a);
        let n0 = self.world.items.len();
        let name = self.world.units[self.ai[a].uidx].name.clone();
        self.kill(a, now, "has been slaughtered");
        if self.world.items.len() > n0 {
            if let (Some(c), Some(sp)) = (self.world.items.last_mut(), sp.as_ref()) {
                if c.kind == "CORPSE" {
                    c.desc = format!("{} corpse", sp.name);
                }
            }
            let cm = self.corpse_mat(a);
            if let Some(c) = self.world.items.last_mut() {
                if c.kind == "CORPSE" && cm != NO_MAT {
                    c.mat = cm;
                }
            }
        }
        let _ = name;
        self.add_xp(s, "BUTCHER", 20);
        self.an_queue_butcher(site);
        self.animals.rep.slaughtered += 1;
    }

    fn an_finish_training(&mut self, s: usize, a: usize, now: u64) {
        if !(a < self.ai.len()) || self.world.units[self.ai[a].uidx].flags.dead {
            return;
        }
        let st = self.animals.st[a];
        let sp = st.sp as usize;
        self.animals.knowledge[sp] += 10;
        self.animals.rep.training_sessions += 1;
        self.add_xp(s, "ANIMALTRAIN", 30);
        let skill = self.world.units[self.ai[s].uidx].skill("ANIMALTRAIN");
        let chance = (40 + 4 * skill.clamp(0, 20) + (self.animals.knowledge[sp] / 20).min(20)).min(95) as u32;
        let mut r = Rng::for_unit(self.seed, s, now, 0x7A);
        self.animals.st[a].last_train = now;
        let uidx = self.ai[a].uidx;
        let name = self.world.units[uidx].name.clone();
        let pos = self.world.units[uidx].pos;
        if !r.chance(chance, 100) {
            return;
        }
        let child = !self.an_is_adult(a);
        let mut lvl = st.level;
        if st.req & REQ_TAME != 0 || lvl < L_TRAINED {
            if lvl < L_TRAINED {
                if !self.world.units[uidx].flags.tame {
                    self.world.units[uidx].flags.tame = true;
                    self.world.units[uidx].flags.hostile = false;
                    self.animals.rep.tamed += 1;
                    self.announce(now, format!("{name} has been tamed."), Some(pos), AnnouncementKind::Info);
                }
                lvl = L_TRAINED;
            } else if lvl < L_MASTER {
                lvl += 1;
            }
            // Wiki: a young animal trained once never needs training again.
            if child {
                lvl = L_DOMESTIC;
            }
            if lvl >= L_DOMESTIC || lvl >= L_MASTER {
                self.animals.st[a].req &= !REQ_TAME;
            }
        }
        if st.req & REQ_WAR != 0 && !child {
            self.animals.st[a].role = ROLE_WAR;
            self.animals.st[a].req &= !REQ_WAR;
            self.world.units[uidx].profession = "TRAINED_WAR".into();
            self.animals.rep.war_trained += 1;
            lvl = lvl.max(L_TRAINED);
        } else if st.req & REQ_HUNT != 0 && !child {
            self.animals.st[a].role = ROLE_HUNT;
            self.animals.st[a].req &= !REQ_HUNT;
            self.world.units[uidx].profession = "TRAINED_HUNTER".into();
            self.animals.rep.hunt_trained += 1;
            lvl = lvl.max(L_TRAINED);
        }
        self.animals.st[a].level = lvl;
        self.world.units[uidx].training = LEVELS[lvl as usize].into();
    }

    fn an_hunting_animals_join(&mut self, s: usize, target: u32, now: u64) {
        let oid = self.ai[s].id;
        for a in 0..self.ai.len() {
            if self.animals.st.get(a).map(|st| st.role) != Some(ROLE_HUNT) || !self.animals_owns(a) {
                continue;
            }
            if self.world.units[self.ai[a].uidx].pet_owner != oid || self.combat_owns(a) {
                continue;
            }
            self.animal_attack(a, target, now);
        }
    }

    fn an_hunt_shoot(&mut self, s: usize, t: usize, now: u64) {
        let w = self.animals.w[s];
        let target_alive = t < self.ai.len() && self.ai[t].kind == Kind::Animal && !self.world.units[self.ai[t].uidx].flags.dead;
        if !target_alive {
            self.an_hunt_kill_done(s, t, now);
            return;
        }
        if now < w.until {
            self.ai[s].wake = w.until;
            return;
        }
        if !self.an_in_range(s, t) {
            self.animals.w[s].step = Step::Stalk;
            let g = self.an_near_goals(self.ai[t].pos);
            self.request_path(s, g, now);
            return;
        }
        // One bolt per shot; no bolts = the job is cancelled (wiki).
        let bolts = w.item2;
        let Some(&bi) = self.item_idx.get(&bolts) else {
            self.cancel_task(s);
            return;
        };
        let bi = bi as usize;
        if self.imeta[bi].dead || self.world.items[bi].stack <= 0 {
            self.cancel_task(s);
            return;
        }
        if !w.own {
            self.unreserve(bolts);
        }
        self.consume(bolts);
        if let Some(&bi2) = self.item_idx.get(&bolts) {
            if !self.imeta[bi2 as usize].dead {
                if !w.own {
                    self.reserve(bolts);
                }
            } else {
                self.animals.w[s].item2 = -1;
            }
        }
        self.animals.rep.shots += 1;
        let u = &self.world.units[self.ai[s].uidx];
        let (xb, amb) = (u.skill("CROSSBOW"), u.skill("SNEAK"));
        let chance = (30 + 4 * xb.clamp(0, 20) + 2 * amb.clamp(0, 20)).min(90) as u32;
        let mut r = Rng::for_unit(self.seed, s, now, 0x5407);
        self.add_xp(s, "CROSSBOW", 10);
        if r.chance(chance, 100) {
            let size = self.world.units[self.ai[t].uidx].body_size.max(1);
            let need = 1 + size / 100_000;
            let hits = self.animals.w[s].hits + 1;
            self.animals.w[s].hits = hits;
            if hits >= need {
                let sp = self.an_sp(t).cloned();
                self.animal_release_holds(t);
                let n0 = self.world.items.len();
                self.combat_kill(t, now, "has been shot");
                if self.world.items.len() > n0 {
                    if let (Some(c), Some(sp)) = (self.world.items.last_mut(), sp.as_ref()) {
                        if c.kind == "CORPSE" {
                            c.desc = format!("{} corpse", sp.name);
                        }
                    }
                }
                self.animals.rep.hunt_kills += 1;
                self.add_xp(s, "SNEAK", 20);
                self.an_hunt_kill_done(s, t, now);
                return;
            }
        }
        if self.animals.w[s].item2 < 0 {
            self.cancel_task(s);
            return;
        }
        self.animals.w[s].until = now + SHOT_TICKS;
        self.ai[s].wake = now + SHOT_TICKS;
    }

    /// The prey is dead: fetch its corpse for the butcher.
    fn an_hunt_kill_done(&mut self, s: usize, t: usize, now: u64) {
        let w = self.animals.w[s];
        if w.item2 >= 0 && !w.own {
            self.unreserve(w.item2);
        }
        self.animals.w[s].item2 = -1;
        self.animals.w[s].own = false;
        // The corpse lies where the prey died (the newest corpse on that tile).
        let tile = self.ai.get(t).map(|a| a.pos).unwrap_or(NONE);
        let corpse = self
            .world
            .items
            .iter()
            .enumerate()
            .rev()
            .take(200)
            .find(|(i, it)| it.kind == "CORPSE" && !self.imeta[*i].dead && it.holder < 0 && self.world.idx(it.pos) == Some(tile as usize))
            .map(|(_, it)| it.id);
        match corpse {
            Some(id) if tile != NONE => {
                self.haul_remove(id);
                self.animals.w[s].item2 = id;
                self.animals.w[s].step = Step::GoCorpse;
                let g = self.item_goals(id);
                self.request_path(s, g, now);
            }
            _ => self.an_work_done(s, now),
        }
    }

    fn an_finish_fishing(&mut self, s: usize, zone: i32, now: u64) {
        let skill = self.world.units[self.ai[s].uidx].skill("FISH");
        let chance = (30 + 5 * skill.clamp(0, 20)).min(90) as u32;
        let mut r = Rng::for_unit(self.seed, s, now, 0xF15);
        self.add_xp(s, "FISH", 20);
        if self.an_fish_left(zone) <= 0 {
            return;
        }
        if !r.chance(chance, 100) {
            return;
        }
        self.an_take_fish(zone);
        let here = self.ai[s].pos;
        let cavern = self.an_water_body(here) == -2;
        let (race, name) = if cavern {
            if r.chance(1, 3) {
                ("LOBSTER_CAVE", "cave lobster")
            } else {
                ("FISH_CAVE", "cave fish")
            }
        } else {
            ("POND_TURTLE", "pond turtle")
        };
        let m = self.animal_mat(race, "MUSCLE", name);
        self.create_item("FISH_RAW", m, here as usize, format!("raw {name}"));
        self.animals.rep.fish_caught += 1;
        if self.an_fish_left(zone) <= 0 {
            let pos = self.world.pos_of(here as usize);
            self.announce(now, "There is nothing left to catch here this season.".into(), Some(pos), AnnouncementKind::Job);
        }
    }

    /// [animals] `cancel_task` of a worker with `Task::Animal`.
    pub(crate) fn animal_work_cancel(&mut self, s: usize) {
        let Some(w) = self.animals.w.get(s).copied() else { return };
        let t = w.target as usize;
        if w.kind != WKind::Fish && t < self.animals.st.len() && self.animals.st[t].held == s as u32 {
            self.animals.st[t].held = NONE;
            self.animals.st[t].led = false;
            self.animals.st[t].retry = self.ticks + 600;
        }
        if w.item >= 0 {
            self.unreserve(w.item);
        }
        if w.item2 >= 0 && !w.own {
            self.unreserve(w.item2);
            if w.step == Step::GoCorpse {
                self.haul_add(w.item2);
            }
        }
        if w.step == Step::Carry && self.ai[s].carrying >= 0 {
            let id = self.ai[s].carrying;
            let here = self.ai[s].pos;
            self.drop_item(s, id, here);
        }
        self.animals.w[s] = AWork::default();
    }

    pub(crate) fn animal_caption(&self, s: usize) -> (&'static str, Option<Pos>) {
        let Some(w) = self.animals.w.get(s) else { return ("No Job", None) };
        let tp = |t: u32| (t as usize) < self.ai.len();
        let at = if tp(w.target) && w.kind != WKind::Fish { Some(self.world.pos_of(self.ai[w.target as usize].pos as usize)) } else { None };
        match (w.kind, w.step) {
            (WKind::Lead, _) => ("Lead animal to pasture", at),
            (WKind::Slaughter, _) => ("Slaughter Animal", at),
            (WKind::Train, _) => ("Train Animal", at),
            (WKind::Hunt, Step::GoCorpse) | (WKind::Hunt, Step::Carry) => ("Bring corpse to butcher", None),
            (WKind::Hunt, _) => ("Hunt for Small Animal", at),
            (WKind::Fish, _) => ("Fish", Some(self.world.pos_of(w.dest as usize))),
            _ => ("No Job", None),
        }
    }

    pub(crate) fn animals_hash(&self, mix: &mut dyn FnMut(u64)) {
        for st in &self.animals.st {
            if st.sp != NO_SP {
                mix(((st.hunger as u64) << 32) ^ st.preg_until ^ ((st.level as u64) << 56) ^ st.milk_at.rotate_left(17));
            }
        }
        for g in &self.animals.grass {
            mix(((g.tile as u64) << 8) | g.amount as u64);
        }
        mix(self.animals.rep.births ^ (self.animals.rep.fish_caught << 20) ^ (self.animals.rep.hunt_kills << 40));
    }

    // ------------------------------------------------------------------ public API helpers

    pub(crate) fn animal_infos(&self) -> Vec<crate::AnimalInfo> {
        let mut out = Vec::new();
        for (s, ai) in self.ai.iter().enumerate() {
            let u = &self.world.units[ai.uidx];
            if !u.flags.animal || !u.flags.tame || u.flags.dead {
                continue;
            }
            let st = self.animals.st.get(s).copied().unwrap_or_default();
            let sp = self.animals.species.get(st.sp as usize);
            let female = u.sex == 0;
            out.push(crate::AnimalInfo {
                unit: u.id,
                race: u.race.clone(),
                caste: u.caste.clone(),
                training: LEVELS[st.level.min(8) as usize].to_string(),
                owner: if u.pet_owner > 0 { u.pet_owner } else { -1 },
                pasture: if u.pasture > 0 { u.pasture } else { -1 },
                slaughter: u.slaughter,
                pregnant: st.preg_until > 0 || (st.fertile > 0 && self.ticks < st.fertile_until),
                grazer: sp.map(|p| p.grazer > 0).unwrap_or(false),
                milkable: sp.map(|p| p.milkable && female).unwrap_or(false),
                shearable: sp.map(|p| p.shear).unwrap_or(false),
                egg_layer: sp.map(|p| p.eggs && female).unwrap_or(false),
                role: match st.role {
                    ROLE_WAR => "War".into(),
                    ROLE_HUNT => "Hunting".into(),
                    _ => String::new(),
                },
            });
        }
        out
    }

    pub(crate) fn animal_set_slaughter(&mut self, unit_id: i32, on: bool) {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return };
        // Pets cannot be butchered (wiki "Pet").
        if on && self.an_pet_owner_slot(s).is_some() {
            return;
        }
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].slaughter = on;
        self.animals.last_census = 0;
    }

    pub(crate) fn animal_assign_pasture(&mut self, unit_id: i32, zone_id: i32) {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return };
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].pasture = zone_id;
        if !self.animals.ready {
            self.init_animals();
        }
        self.animals_ensure();
        let valid = zone_id > 0 && self.world.buildings.iter().any(|b| b.id == zone_id && b.kind == "Civzone");
        if valid && !self.animals.pens.iter().any(|p| p.bid == zone_id) {
            if let Some(b) = self.world.building(zone_id) {
                self.animals.pens.push(Pen { bid: b.id, zone: crate::Zone { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2, z: b.z } });
            }
        }
        // Wiki "Pasture": a caged animal assigned to a pasture is uncaged and brought there.
        let u = &self.world.units[uidx];
        if valid && u.flags.caged && u.flags.tame && !u.flags.dead {
            self.world.units[uidx].flags.caged = false;
            self.ai[s].kind = Kind::Animal;
            self.ai[s].wake = self.ticks + 1;
        }
        // Already standing in the pasture: nobody needs to lead it.
        let inside = valid && self.an_pen_of(s).map(|p| self.an_in_zone(self.ai[s].pos, p.zone)).unwrap_or(false);
        self.animals.st[s].lead_req = valid && !inside;
        self.animals.last_census = 0;
    }

    pub(crate) fn animal_request_training(&mut self, unit_id: i32, kind: &str) {
        let Some(&s) = self.unit_slot.get(&unit_id) else { return };
        if !self.animals.ready {
            self.init_animals();
        }
        self.animals_ensure();
        let Some(sp) = self.an_sp(s) else { return };
        let bit = match kind {
            "War" if sp.war => REQ_WAR,
            "Hunting" if sp.hunt => REQ_HUNT,
            "Tame" if sp.pet => REQ_TAME,
            "None" | "" => {
                self.animals.st[s].req = 0;
                return;
            }
            _ => return,
        };
        self.animals.st[s].req |= bit;
        self.animals.last_census = 0;
    }

    /// Animal husbandry counters and the herd by race (tame animals on the map).
    pub fn animal_report(&self) -> AnimalReport {
        let mut r = self.animals.rep.clone();
        let mut by: Vec<(String, usize)> = Vec::new();
        for s in 0..self.ai.len().min(self.animals.st.len()) {
            if !self.animals_owns(s) {
                continue;
            }
            let u = &self.world.units[self.ai[s].uidx];
            match by.iter_mut().find(|e| e.0 == u.race) {
                Some(e) => e.1 += 1,
                None => by.push((u.race.clone(), 1)),
            }
            r.tame += 1;
            let st = self.animals.st[s];
            let sp = &self.animals.species[st.sp as usize];
            if sp.grazer > 0 {
                r.grazers += 1;
                if st.hunger > 50_000 {
                    r.hungry_grazers += 1;
                }
            }
            if self.an_pen_of(s).is_some() {
                r.pastured += 1;
            }
            if st.preg_until > 0 {
                r.pregnant += 1;
            }
        }
        by.sort();
        r.by_race = by;
        r.bare_tiles = self.animals.grass.iter().filter(|g| g.bare).count();
        r.hunt_targets = self.animals.hunt_targets;
        r.hunt_gear = self.animals.hunt_gear;
        r
    }

    /// Switch the animal simulation on or off (for comparisons).
    pub fn set_animals_enabled(&mut self, on: bool) {
        self.animals.off = !on;
    }

    /// Grass left on a tile in % (0-100), for tests and the UI.
    pub fn grass_at(&self, p: Pos) -> u8 {
        self.world.idx(p).map(|t| self.an_grass_amount(t as u32)).unwrap_or(0)
    }

    /// Grazer hunger of a unit (DF hunger counter), if it is a tame grazer.
    pub fn grazer_hunger(&self, unit_id: i32) -> Option<i32> {
        let &s = self.unit_slot.get(&unit_id)?;
        let st = self.animals.st.get(s)?;
        let sp = self.animals.species.get(st.sp as usize)?;
        (sp.grazer > 0).then_some(st.hunger)
    }

    /// GRAZER value of a race (0 = no grazer), for tests and the UI.
    pub fn grazer_value_of(&self, race: &str) -> i32 {
        self.animals.sp_of.get(race).map(|&i| self.animals.species[i as usize].grazer).unwrap_or(0)
    }
}
