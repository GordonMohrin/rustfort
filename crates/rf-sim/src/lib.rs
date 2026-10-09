//! rf-sim: deterministic fortress simulation (tick loop, pathfinding, units, jobs, needs).
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//!
//! # Tick structure
//! Every tick runs three phases:
//! 1. **think** (serial, unit index order): only units whose `wake` tick has come run
//!    ("think-wheel"). A moving unit wakes once per step, a working unit once at the end of
//!    its job (or at the next need threshold), an idle unit every few dozen ticks. Units that
//!    need a path push a request instead of searching.
//! 2. **paths** (parallel, rayon): all path requests of the tick are solved with A* on the
//!    read-only navigation grid, each thread with its own reusable buffers.
//! 3. **commit** (serial, request order): results are handed to the units.
//!
//! The result never depends on the thread count, because phase 2 only reads and phase 3 runs
//! in a fixed order. Random numbers are derived from (seed, unit slot, tick).
//!
//! # DF rules implemented (wiki: Thirst, Hunger, Sleep, Gait, Path, Traffic, Ramp, Mining)
//! - 1200 ticks per day; need counters +1 per tick (sleep -19 per tick while asleep).
//! - Thirst: consider drinking at 20 000 (1/120 per idle tick), go at 22 000, cancel the job at
//!   35 000, die at 75 000; a drink takes 50 000 off. Hunger: 40 000 / 45 000 / 65 000, death
//!   at 100 000 (fat reserves are not modelled), a meal takes 50 000 off. Sleep: 50 000 /
//!   54 000, 150 000 = falls asleep on the spot; wakes at 0.
//! - Gait speed in ticks per 100 tiles (900 = 9 ticks per tile), carried as an exact
//!   fractional remainder.
//! - Mining: wall -> floor/stairs/ramp of the same material class (random variant). Layer
//!   stone leaves a boulder 25 % of the time, ore 33 %, gems and special features 100 %,
//!   soil nothing. Channels remove the tile and turn a wall below into a ramp (half the
//!   yield chance). Ramps remove the floor above. Adjacent hidden tiles are revealed.
//! - Tree felling turns the whole tree into open space and drops logs (one per trunk tile).
//! - Hauling: one item at a time to the nearest free tile of a stockpile that accepts the
//!   item's category, using the matching HAUL_* labor.
//! - No booze: dwarves drink from water (any non-magma liquid next to a walkable tile) or a
//!   working well.
//! - Production (round 2, see `production`): stills, kitchens, carpenters, masons and
//!   craftsdwarfs run their job queues and the manager's work orders; farm plots are planted
//!   and harvested by season; placed buildings are constructed from hauled materials.
//!
//! # Estimates and gaps (v0.1)
//! - Work times are estimates (dig 120 ticks, fell 200, gather 60, scaled by skill as
//!   `base * 10 / (10 + 2 * rating)`); drinking 30 and eating 60 ticks. Not on the wiki.
//! - No tools required (picks/axes), no fat reserves, no thoughts/stress, no unit collisions,
//!   no carried-weight slowdown, no dining rooms, no item containers being hauled with
//!   contents into matching piles (contents follow, piles see only the container kind).
//! - Felling removes all TREE tiles within 4 tiles of the trunk that are connected to it, so
//!   touching canopies of neighbouring trees can go with it.
//! - Path cache hit rate is low; batches are usually too small for rayon to pay off
//!   (threshold: 8 requests per tick), so the sim is effectively single-threaded today.

pub mod build_api;
// [combat]
pub mod combat;
pub mod invasion;
pub mod military;
pub use combat::CombatStats;
pub mod round3_api;
pub use round3_api::{SquadOrder, StressLevel};
pub use build_api::{BuildOption, OrderTemplate, ZONE_KINDS};
pub mod gen;
pub mod items;
pub mod nav;
pub mod path;
pub mod production;
pub use production::{Census, PlantInfo, ProductionReport};
mod rng;

use items::{is_drink, is_edible, Cat};
use nav::{Nav, DIRS8};
use path::{AStar, Goals};
use rayon::prelude::*;
use rf_core::{tile_flags, Dig, Item, ItemFlags, Pos, Shape, Skill, TtMaterial, TtSpecial, World, NO_MAT};
use rng::Rng;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

/// DF need thresholds in ticks (wiki pages Thirst, Hunger, Sleep).
pub mod needs {
    pub const THIRST_CONSIDER: i32 = 20_000;
    pub const THIRST_SEEK: i32 = 22_000;
    pub const THIRSTY: i32 = 25_000;
    pub const THIRST_CANCEL: i32 = 35_000;
    pub const DEHYDRATED: i32 = 50_000;
    pub const THIRST_DEATH: i32 = 75_000;
    pub const HUNGER_CONSIDER: i32 = 40_000;
    pub const HUNGER_SEEK: i32 = 45_000;
    pub const HUNGRY: i32 = 50_000;
    pub const HUNGER_CANCEL: i32 = 65_000;
    pub const STARVING: i32 = 75_000;
    pub const HUNGER_DEATH: i32 = 100_000;
    pub const SLEEP_CONSIDER: i32 = 50_000;
    pub const SLEEP_SEEK: i32 = 54_000;
    pub const DROWSY: i32 = 57_600;
    pub const VERY_DROWSY: i32 = 150_000;
    pub const SLEEP_RECOVERY_PER_TICK: i32 = 19;
    pub const DRINK_SATISFIES: i32 = 50_000;
    pub const EAT_SATISFIES: i32 = 50_000;
    /// Duration of the actual drinking / eating (not on the wiki; estimate).
    pub const DRINK_TICKS: u64 = 30;
    pub const EAT_TICKS: u64 = 60;
}

/// Labor bits (subset of DF `unit_labor`).
pub mod labor {
    pub const MINE: u32 = 1 << 0;
    pub const CUTWOOD: u32 = 1 << 1;
    pub const HERBALISM: u32 = 1 << 2;
    pub const HAUL_STONE: u32 = 1 << 3;
    pub const HAUL_WOOD: u32 = 1 << 4;
    pub const HAUL_FOOD: u32 = 1 << 5;
    pub const HAUL_ITEM: u32 = 1 << 6;
    pub const HAUL_REFUSE: u32 = 1 << 7;
    pub const HAUL_FURNITURE: u32 = 1 << 8;
    pub const HAUL_BODY: u32 = 1 << 9;
    pub const HAUL_ANIMALS: u32 = 1 << 10;
    pub const BREW: u32 = 1 << 11;
    pub const COOK: u32 = 1 << 12;
    /// Farming (fields): planting and harvesting.
    pub const PLANT: u32 = 1 << 13;
    pub const CARPENTER: u32 = 1 << 14;
    pub const MASON: u32 = 1 << 15;
    pub const STONE_CRAFT: u32 = 1 << 16;
    pub const WOOD_CRAFT: u32 = 1 << 17;
    pub const PROCESS_PLANT: u32 = 1 << 18;
    pub const MILLER: u32 = 1 << 19;
    pub const BUILD_CONSTRUCTION: u32 = 1 << 20;
    pub const MECHANIC: u32 = 1 << 21;
    pub const ANY_HAUL: u32 = HAUL_STONE | HAUL_WOOD | HAUL_FOOD | HAUL_ITEM | HAUL_REFUSE | HAUL_FURNITURE | HAUL_BODY | HAUL_ANIMALS;
    pub const ANY_JOB: u32 = MINE | CUTWOOD | HERBALISM;
    /// Labors that can take workshop, farm or construction jobs.
    pub const ANY_PROD: u32 = BREW | COOK | PLANT | CARPENTER | MASON | STONE_CRAFT | HAUL_FURNITURE;

    pub fn from_df(s: &str) -> u32 {
        match s {
            "MINE" => MINE,
            "CUTWOOD" => CUTWOOD,
            "HERBALISM" | "HERBALIST" => HERBALISM,
            "BREWER" | "BREW" => BREW,
            "COOK" => COOK,
            "PLANT" => PLANT,
            "CARPENTER" => CARPENTER,
            "MASON" => MASON,
            "STONE_CRAFT" => STONE_CRAFT,
            "WOOD_CRAFT" => WOOD_CRAFT,
            "PROCESS_PLANT" => PROCESS_PLANT,
            "MILLER" => MILLER,
            "BUILD_CONSTRUCTION" => BUILD_CONSTRUCTION,
            "MECHANIC" => MECHANIC,
            "HAUL_STONE" => HAUL_STONE,
            "HAUL_WOOD" => HAUL_WOOD,
            "HAUL_FOOD" => HAUL_FOOD,
            "HAUL_ITEM" => HAUL_ITEM,
            "HAUL_REFUSE" => HAUL_REFUSE,
            "HAUL_FURNITURE" => HAUL_FURNITURE,
            "HAUL_BODY" => HAUL_BODY,
            "HAUL_ANIMALS" => HAUL_ANIMALS,
            _ => 0,
        }
    }
    pub fn bits(labors: &[String]) -> u32 {
        labors.iter().fold(0, |a, l| a | from_df(l))
    }
}

/// Player designations that can be placed with the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DesignationKind {
    Mine,
    Channel,
    UpDownStair,
    UpStair,
    DownStair,
    Ramp,
    ChopTrees,
    GatherPlants,
    /// Erase designations in the area.
    Remove,
}

/// What a unit is doing right now, for the UI (DF shows this as the unit's "job").
#[derive(Clone, Debug, Default)]
pub struct UnitStatus {
    /// Short DF-style activity text, e.g. `Dig`, `Drink`, `Sleep`, `No Job`, `Store Item in Stockpile`.
    pub activity: String,
    /// Target position of the current job, if any.
    pub job_pos: Option<Pos>,
    /// 0..=100 need levels for the UI (100 = urgent). 50 is where DF starts flashing the
    /// indicator (Thirsty 25 000, Hungry 50 000, Drowsy 57 600); 100 = Dehydrated / starving
    /// on fat / about to collapse.
    pub hunger: u8,
    pub thirst: u8,
    pub tiredness: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnouncementKind {
    Info,
    Job,
    Warning,
    Death,
}

#[derive(Clone, Debug)]
pub struct Announcement {
    /// Absolute sim tick.
    pub tick: u64,
    pub text: String,
    pub pos: Option<Pos>,
    pub kind: AnnouncementKind,
}

/// Counters for the UI and the benchmark.
#[derive(Clone, Debug, Default)]
pub struct SimStats {
    pub ticks: u64,
    pub open_jobs: usize,
    pub active_units: usize,
    /// Wall-clock seconds spent per phase in the last `run_ticks` call (name, seconds).
    pub phase_times: Vec<(&'static str, f64)>,
    /// Path searches run (A*), path cache hits and A* node expansions since the start.
    pub paths: u64,
    pub path_cache_hits: u64,
    pub path_nodes: u64,
    pub path_failures: u64,
    /// Full component rebuilds of the navigation grid.
    pub nav_rebuilds: u64,
    pub dead_units: usize,
    pub items: usize,
    pub jobs_done: u64,
    pub items_hauled: u64,
    /// How often idle units searched for designation jobs / hauling / food+drink, and how
    /// often they decided what to do at all.
    pub job_searches: u64,
    pub haul_searches: u64,
    pub need_searches: u64,
    pub decisions: u64,
    pub steps: u64,
    /// Production job searches by idle workers, full item scans for reagents, and the
    /// wall-clock seconds spent in those searches.
    pub prod_searches: u64,
    pub prod_item_scans: u64,
    pub prod_search_secs: f64,
}

/// A designation job (dig, fell tree, gather plants).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Dig(Dig),
    Chop,
    Gather,
}

impl JobKind {
    pub fn labor(self) -> u32 {
        match self {
            JobKind::Dig(_) => labor::MINE,
            JobKind::Chop => labor::CUTWOOD,
            JobKind::Gather => labor::HERBALISM,
        }
    }
    /// DF job caption (`df.job_type.attrs.*.caption`).
    pub fn caption(self) -> &'static str {
        match self {
            JobKind::Dig(Dig::Channel) => "Dig Channel",
            JobKind::Dig(Dig::UpDownStair) => "Carve Up/Down Staircase",
            JobKind::Dig(Dig::UpStair) => "Carve Upward Staircase",
            JobKind::Dig(Dig::DownStair) => "Carve Downward Staircase",
            JobKind::Dig(Dig::Ramp) => "Carve Ramp",
            JobKind::Dig(_) => "Dig",
            JobKind::Chop => "Fell Tree",
            JobKind::Gather => "Gather Plants",
        }
    }
    fn skill(self) -> &'static str {
        match self {
            JobKind::Dig(_) => "MINING",
            JobKind::Chop => "WOODCUTTING",
            JobKind::Gather => "HERBALISM",
        }
    }
}

const NONE: u32 = u32::MAX;
/// A* expansion limit per search.
const MAX_NODES: u32 = 400_000;
/// Re-check for work at least this often even if nothing changed.
const RESEARCH_TICKS: u64 = 600;
/// Solve path batches in parallel only from this size on: waking the rayon pool costs tens
/// of microseconds, more than a few short A* searches.
const PAR_MIN_PATHS: usize = 8;
const PATH_CACHE_MAX: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Worker,
    Animal,
    Other,
    Dead,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Task {
    Idle,
    Wander,
    GoJob(u32),
    Job(u32),
    GoDrink(i32),
    Drink(i32),
    GoEat(i32),
    Eat(i32),
    GoSleep(i32),
    Sleep(i32),
    GoPickup(i32, u32),
    GoStore(i32, u32),
    /// Production job (see `production`): walk to a reagent, walk to the site, work there.
    Fetch(i32),
    GoSite,
    Craft,
    /// [combat] Unit run by the combat/military module (fighting, fleeing, training, resting).
    Combat,
}

struct UnitAi {
    uidx: usize,
    id: i32,
    kind: Kind,
    pos: u32,
    speed: i32,
    frac: i32,
    wake: u64,
    last: u64,
    last_decide: u64,
    act_until: u64,
    thirst: i32,
    hunger: i32,
    sleep: i32,
    sleeping: bool,
    task: Task,
    path: Vec<u32>,
    path_i: usize,
    waiting: bool,
    req: u32,
    carrying: i32,
    labors: u32,
    seen_jobs: u64,
    seen_haul: u64,
    seen_prod: u64,
    next_search: u64,
    no_drink_until: u64,
    no_food_until: u64,
}

#[derive(Clone, Copy, Debug)]
struct Job {
    kind: JobKind,
    tile: u32,
    claimed: u32,
    blocked_until: u64,
    live: bool,
    /// Some walkable tile to work from exists (kept up to date on tile changes), so idle
    /// workers skip designations inside solid rock without looking at their neighbours.
    access: bool,
}

struct JobChunk {
    x0: i32,
    y0: i32,
    z: i32,
    jobs: Vec<u32>,
}

const CHUNK: usize = 16;

struct Pile {
    cats: u32,
    tiles: Vec<u32>,
    occ: Vec<u16>,
    res: Vec<bool>,
    free: u32,
}

struct Bed {
    tile: u32,
    owner: i32,
    occupant: u32,
}

#[derive(Clone, Copy)]
struct Zone {
    x1: i16,
    y1: i16,
    x2: i16,
    y2: i16,
    z: i16,
}

#[derive(Clone, Copy)]
struct ItemMeta {
    reserved: i32,
    hauling: bool,
    dead: bool,
    cat: Cat,
    /// Could not be reached recently; ignored by searches until this tick.
    blocked_until: u64,
    /// Production class (`production::pc`) and plant species (or `NO_PLANT`).
    pc: u8,
    plant: u16,
}

struct PathJob {
    slot: usize,
    req: u32,
    start: u32,
    goals: Goals,
    ok: bool,
    nodes: u32,
    out: Vec<u32>,
}

thread_local! {
    static ASTAR: RefCell<AStar> = RefCell::new(AStar::new());
}

pub struct Sim {
    pub world: World,
    pub seed: u64,
    ticks: u64,
    announcements: Vec<Announcement>,
    nav: Nav,
    ai: Vec<UnitAi>,
    unit_slot: HashMap<i32, usize>,
    jobs: Vec<Job>,
    job_at: HashMap<u32, u32>,
    /// Spatial index of jobs: 16x16 tile chunks per z-level.
    chunks: Vec<JobChunk>,
    chunk_of: HashMap<(u16, u16, u16), u32>,
    chunk_order: Vec<(i32, u32)>,
    free_jobs: Vec<u32>,
    open_jobs: usize,
    epoch_jobs: u64,
    epoch_haul: u64,
    piles: Vec<Pile>,
    pile_at: HashMap<u32, (u32, u32)>,
    pile_cats: u32,
    imeta: Vec<ItemMeta>,
    item_idx: HashMap<i32, u32>,
    next_item_id: i32,
    drinks: Vec<i32>,
    foods: Vec<i32>,
    /// Items waiting to be hauled (exact: not stored, not being hauled), with positions.
    haul: Vec<i32>,
    /// Item index of each `haul` entry (valid until the next compaction, which rebuilds it).
    haul_ix: Vec<u32>,
    haul_pos: HashMap<i32, u32>,
    haul_cnt: [u32; 17],
    contents: HashMap<i32, Vec<i32>>,
    items_dirty: bool,
    beds: Vec<Bed>,
    zones: Vec<Zone>,
    /// Walkable tiles next to (or in shallow) water, for drinking when there is no booze.
    water: Vec<u32>,
    water_set: std::collections::HashSet<u32>,
    path_jobs: Vec<PathJob>,
    n_path: usize,
    failed: Vec<usize>,
    path_cache: HashMap<(u32, u32), Vec<u32>>,
    phase: [f64; 5],
    stats: SimStats,
    par_min_paths: usize,
    /// Workshops, farms, orders, construction (see `production`).
    prod: production::Prod,
    /// [combat] Combat, military and invasions (see `combat`, `military`, `invasion`).
    combat: combat::Combat,
}

#[inline]
fn manhattan(a: (usize, usize, usize), b: (usize, usize, usize)) -> i32 {
    (a.0 as i32 - b.0 as i32).abs() + (a.1 as i32 - b.1 as i32).abs() + (a.2 as i32 - b.2 as i32).abs()
}

fn next_threshold(v: i32, thr: &[i32]) -> u64 {
    for &t in thr {
        if v < t {
            return (t - v) as u64;
        }
    }
    u64::MAX
}

/// Material class a dug tile keeps (grass turns into soil).
fn dig_class(m: TtMaterial) -> TtMaterial {
    if m.is_grass() {
        TtMaterial::Soil
    } else {
        m
    }
}

fn blocks_movement(kind: &str) -> bool {
    matches!(
        kind,
        "Statue" | "Support" | "Floodgate" | "WindowGlass" | "WindowGem" | "GrateWall" | "BarsVertical" | "GearAssembly"
    )
}

impl Sim {
    pub fn new(world: World, seed: u64) -> Sim {
        let mut blocked = Vec::new();
        for b in &world.buildings {
            if blocks_movement(&b.kind) {
                for y in b.y1..=b.y2 {
                    for x in b.x1..=b.x2 {
                        let p = Pos::new(x, y, b.z);
                        if b.contains(p) {
                            if let Some(i) = world.idx(p) {
                                blocked.push(i);
                            }
                        }
                    }
                }
            }
        }
        let nav = Nav::new(&world, &blocked);
        let mut sim = Sim {
            world,
            seed,
            ticks: 0,
            announcements: Vec::new(),
            nav,
            ai: Vec::new(),
            unit_slot: HashMap::new(),
            jobs: Vec::new(),
            job_at: HashMap::new(),
            free_jobs: Vec::new(),
            open_jobs: 0,
            epoch_jobs: 1,
            epoch_haul: 1,
            piles: Vec::new(),
            pile_at: HashMap::new(),
            pile_cats: 0,
            imeta: Vec::new(),
            item_idx: HashMap::new(),
            next_item_id: 1,
            drinks: Vec::new(),
            foods: Vec::new(),
            haul: Vec::new(),
            haul_ix: Vec::new(),
            haul_pos: HashMap::new(),
            haul_cnt: [0; 17],
            chunks: Vec::new(),
            chunk_of: HashMap::new(),
            chunk_order: Vec::new(),
            contents: HashMap::new(),
            items_dirty: false,
            beds: Vec::new(),
            zones: Vec::new(),
            water: Vec::new(),
            water_set: std::collections::HashSet::new(),
            path_jobs: Vec::new(),
            n_path: 0,
            failed: Vec::new(),
            path_cache: HashMap::new(),
            phase: [0.0; 5],
            stats: SimStats::default(),
            par_min_paths: PAR_MIN_PATHS,
            prod: production::Prod::default(),
            combat: combat::Combat::default(), // [combat]
        };
        sim.init_plants();
        sim.init_units();
        sim.init_items();
        sim.init_buildings();
        sim.init_jobs_from_tiles();
        sim.init_water();
        sim.init_production();
        sim.init_combat(); // [combat]
        sim.rebuild_food_lists();
        sim.rebuild_haul();
        sim
    }

    // ------------------------------------------------------------------ init

    fn init_units(&mut self) {
        for i in self.ai.len()..self.world.units.len() {
            let u = &self.world.units[i];
            let kind = if u.flags.dead {
                Kind::Dead
            } else if u.is_worker() {
                Kind::Worker
            } else if u.flags.animal {
                Kind::Animal
            } else if u.flags.inactive || u.flags.caged {
                Kind::Dead
            } else {
                Kind::Other
            };
            let pos = match self.world.idx(u.pos) {
                Some(p) => p as u32,
                None => 0,
            };
            let kind = if self.world.idx(u.pos).is_none() { Kind::Dead } else { kind };
            self.unit_slot.insert(u.id, i);
            self.ai.push(UnitAi {
                uidx: i,
                id: u.id,
                kind,
                pos,
                speed: if u.walk_speed > 0 { u.walk_speed } else { 900 },
                frac: 0,
                wake: if kind == Kind::Dead { u64::MAX } else { 1 + (i as u64 % 16) },
                last: self.ticks,
                last_decide: self.ticks,
                act_until: 0,
                thirst: u.thirst_timer.max(0),
                hunger: u.hunger_timer.max(0),
                sleep: u.sleepiness_timer.max(0),
                sleeping: false,
                task: Task::Idle,
                path: Vec::new(),
                path_i: 0,
                waiting: false,
                req: 0,
                carrying: -1,
                labors: labor::bits(&u.labors),
                seen_jobs: 0,
                seen_haul: 0,
                seen_prod: 0,
                next_search: 0,
                no_drink_until: 0,
                no_food_until: 0,
            });
        }
    }

    fn init_items(&mut self) {
        self.imeta.clear();
        self.item_idx.clear();
        self.contents.clear();
        for (i, it) in self.world.items.iter().enumerate() {
            self.item_idx.insert(it.id, i as u32);
            self.next_item_id = self.next_item_id.max(it.id + 1);
            let (pc, plant) = self.classify(it);
            self.imeta.push(ItemMeta { reserved: 0, hauling: false, dead: false, cat: Cat::of_kind(&it.kind), blocked_until: 0, pc, plant });
            if it.container >= 0 {
                self.contents.entry(it.container).or_default().push(it.id);
            }
        }
        // Containers take the category of their contents (a barrel of wine is food).
        for (cid, ids) in &self.contents {
            if let (Some(&ci), Some(&first)) = (self.item_idx.get(cid), ids.first()) {
                if let Some(&fi) = self.item_idx.get(&first) {
                    self.imeta[ci as usize].cat = self.imeta[fi as usize].cat;
                }
            }
        }
        self.rebuild_food_lists();
    }

    fn rebuild_food_lists(&mut self) {
        self.drinks.clear();
        self.foods.clear();
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            if it.flags.forbid || it.flags.rotten || it.holder >= 0 || it.stack <= 0 || self.imeta[idx].dead {
                continue;
            }
            if is_drink(&it.kind) {
                self.drinks.push(it.id);
            } else if is_edible(&it.kind) && self.edible_now(idx) {
                self.foods.push(it.id);
            }
        }
    }

    fn init_buildings(&mut self) {
        for bi in 0..self.world.buildings.len() {
            let b = &self.world.buildings[bi];
            match b.kind.as_str() {
                "Stockpile" if b.stockpile.is_some() => self.register_pile(bi),
                "Bed" => {
                    if let Some(t) = self.world.idx(b.center) {
                        self.beds.push(Bed { tile: t as u32, owner: b.owner, occupant: NONE });
                    }
                }
                "Civzone" if b.subtype.contains("Meeting") => {
                    self.zones.push(Zone { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2, z: b.z });
                }
                _ => {}
            }
        }
    }

    fn register_pile(&mut self, bi: usize) {
        let b = &self.world.buildings[bi];
        let cats = b
            .stockpile
            .as_ref()
            .map(|s| s.categories.iter().filter_map(|c| Cat::from_df(c)).fold(0, |a, c| a | c.bit()))
            .unwrap_or(0);
        let mut tiles = Vec::new();
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                let p = Pos::new(x, y, b.z);
                if b.contains(p) {
                    if let Some(i) = self.world.idx(p) {
                        // Like DF, a stockpile only covers tiles one can stand on.
                        if self.nav.walkable(i) {
                            tiles.push(i as u32);
                        }
                    }
                }
            }
        }
        let pi = self.piles.len() as u32;
        let n = tiles.len();
        for (k, &t) in tiles.iter().enumerate() {
            self.pile_at.insert(t, (pi, k as u32));
            self.world.tiles[t as usize].flags |= tile_flags::PILE;
        }
        let mut occ = vec![0u16; n];
        for it in &self.world.items {
            if it.flags.on_ground && it.container < 0 && it.holder < 0 {
                if let Some(i) = self.world.idx(it.pos) {
                    if let Some(&(p, k)) = self.pile_at.get(&(i as u32)) {
                        if p == pi {
                            occ[k as usize] += 1;
                        }
                    }
                }
            }
        }
        let free = occ.iter().filter(|&&o| o == 0).count() as u32;
        self.piles.push(Pile { cats, tiles, occ, res: vec![false; n], free });
        self.pile_cats |= cats;
    }

    fn is_water(&self, i: usize) -> bool {
        let t = &self.world.tiles[i];
        t.liquid_depth() > 0 && !t.is_magma()
    }

    fn init_water(&mut self) {
        for i in 0..self.world.tiles.len() {
            if self.is_water(i) {
                self.add_water_access_around(i);
            }
        }
    }

    fn add_water_access_around(&mut self, w: usize) {
        let (x, y, z) = self.nav.xyz(w);
        for yy in y.saturating_sub(1)..=(y + 1).min(self.nav.dy - 1) {
            for xx in x.saturating_sub(1)..=(x + 1).min(self.nav.dx - 1) {
                let q = self.nav.idx(xx, yy, z);
                if self.nav.walkable(q) && self.water_set.insert(q as u32) {
                    self.water.push(q as u32);
                }
            }
        }
    }

    fn nearest_water(&self, s: usize) -> Option<u32> {
        if self.water.is_empty() {
            return None;
        }
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        let up = self.nav.xyz(pos);
        let mut best = (i32::MAX, NONE);
        for &t in &self.water {
            let d = manhattan(up, self.nav.xyz(t as usize));
            if (d, t) < best && self.nav.walkable(t as usize) && self.nav.comp_root(t as usize) == c {
                best = (d, t);
            }
        }
        (best.1 != NONE).then_some(best.1)
    }

    fn init_jobs_from_tiles(&mut self) {
        for i in 0..self.world.tiles.len() {
            let t = self.world.tiles[i];
            if t.desig == 0 || t.flags & tile_flags::MARKER != 0 {
                continue;
            }
            match self.job_kind_for(i, t.dig()) {
                Some(k) => self.add_job(i as u32, k),
                None => self.world.tiles[i].desig = 0,
            }
        }
    }

    // ------------------------------------------------------------------ public API

    /// Advance the simulation by exactly one DF tick.
    pub fn tick(&mut self) {
        self.phase = [0.0; 5];
        self.tick_inner();
        self.sync_units();
    }

    pub fn run_ticks(&mut self, n: u32) {
        self.phase = [0.0; 5];
        for _ in 0..n {
            self.tick_inner();
        }
        self.sync_units();
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Designate the inclusive box between `a` and `b` (any corner order, may span z-levels).
    /// Like DF, only valid tiles get the designation (e.g. Mine only on natural walls,
    /// ChopTrees only on trees, which designates the trunk base).
    pub fn designate(&mut self, a: Pos, b: Pos, kind: DesignationKind) {
        let (lo, hi) = (
            Pos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            Pos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        );
        for z in lo.z..=hi.z {
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    let i = match self.world.idx(Pos::new(x, y, z)) {
                        Some(i) => i,
                        None => continue,
                    };
                    match kind {
                        DesignationKind::Remove => {
                            if let Some(&j) = self.job_at.get(&(i as u32)) {
                                self.remove_job(j);
                            }
                            self.world.tiles[i].desig = 0;
                        }
                        DesignationKind::ChopTrees => {
                            let info = self.world.tiletypes.get(self.world.tiles[i].tt);
                            if info.material == TtMaterial::Tree && info.shape == Shape::Wall {
                                let base = self.tree_base(i);
                                if self.job_kind_for(base, Dig::Dig) == Some(JobKind::Chop) {
                                    self.world.tiles[base].desig = Dig::Dig as u8;
                                    self.add_job(base as u32, JobKind::Chop);
                                }
                            }
                        }
                        DesignationKind::GatherPlants => {
                            if self.job_kind_for(i, Dig::Dig) == Some(JobKind::Gather) {
                                self.world.tiles[i].desig = Dig::Dig as u8;
                                self.add_job(i as u32, JobKind::Gather);
                            }
                        }
                        _ => {
                            let d = match kind {
                                DesignationKind::Mine => Dig::Dig,
                                DesignationKind::Channel => Dig::Channel,
                                DesignationKind::UpDownStair => Dig::UpDownStair,
                                DesignationKind::UpStair => Dig::UpStair,
                                DesignationKind::DownStair => Dig::DownStair,
                                _ => Dig::Ramp,
                            };
                            if self.job_kind_for(i, d) == Some(JobKind::Dig(d)) {
                                self.world.tiles[i].desig = d as u8;
                                self.add_job(i as u32, JobKind::Dig(d));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Create a stockpile over the inclusive box (single z-level `a.z`) accepting `categories`
    /// (DF names, see `rf_core::StockpileSettings`). Returns the new building id.
    pub fn create_stockpile(&mut self, a: Pos, b: Pos, categories: &[&str]) -> i32 {
        let id = self.world.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        let (x1, x2, y1, y2) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        self.world.buildings.push(rf_core::Building {
            id,
            kind: "Stockpile".into(),
            x1,
            y1,
            x2,
            y2,
            z: a.z,
            center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, a.z),
            built: true,
            owner: -1,
            stockpile: Some(rf_core::StockpileSettings {
                categories: categories.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }),
            ..Default::default()
        });
        self.register_pile(self.world.buildings.len() - 1);
        self.rebuild_haul();
        self.epoch_haul += 1;
        id
    }

    pub fn unit_status(&self, unit_id: i32) -> UnitStatus {
        let s = match self.unit_slot.get(&unit_id) {
            Some(&s) => s,
            None => return UnitStatus { activity: "No Job".into(), ..Default::default() },
        };
        let ai = &self.ai[s];
        let (th, hu, sl) = self.needs_now(ai);
        let lvl = |v: i32, full: i32| ((v.max(0) as i64 * 100 / full as i64).min(100)) as u8;
        let pos_of = |t: u32| self.world.pos_of(t as usize);
        let item_pos = |id: i32| self.item_idx.get(&id).map(|&i| self.world.items[i as usize].pos);
        let (activity, job_pos): (&str, Option<Pos>) = match (ai.kind, ai.task) {
            (Kind::Dead, _) => ("Dead", None),
            (_, Task::Idle) | (_, Task::Wander) => ("No Job", None),
            (_, Task::GoJob(j)) | (_, Task::Job(j)) => {
                let job = &self.jobs[j as usize];
                (job.kind.caption(), Some(pos_of(job.tile)))
            }
            (_, Task::GoDrink(id)) | (_, Task::Drink(id)) => ("Drink", item_pos(id)),
            (_, Task::GoEat(id)) | (_, Task::Eat(id)) => ("Eat", item_pos(id)),
            (_, Task::GoSleep(b)) | (_, Task::Sleep(b)) => {
                ("Sleep", if b >= 0 { Some(pos_of(self.beds[b as usize].tile)) } else { None })
            }
            (_, Task::GoPickup(_, d)) | (_, Task::GoStore(_, d)) => ("Store Item in Stockpile", Some(pos_of(d))),
            (_, Task::Fetch(_)) | (_, Task::GoSite) | (_, Task::Craft) => self.wjob_caption(s),
            (_, Task::Combat) => self.combat_caption(s), // [combat]
        };
        UnitStatus {
            activity: activity.into(),
            job_pos,
            hunger: lvl(hu, needs::HUNGRY * 2),
            thirst: lvl(th, needs::THIRSTY * 2),
            tiredness: lvl(sl, needs::DROWSY * 2),
        }
    }

    /// Take all announcements produced since the last call.
    pub fn drain_announcements(&mut self) -> Vec<Announcement> {
        std::mem::take(&mut self.announcements)
    }

    pub fn stats(&self) -> SimStats {
        let mut s = self.stats.clone();
        s.ticks = self.ticks;
        s.open_jobs = self.open_jobs;
        s.active_units = self.ai.iter().filter(|a| a.kind != Kind::Dead).count();
        s.dead_units = self.ai.iter().filter(|a| a.kind == Kind::Dead && self.world.units[a.uidx].flags.dead).count();
        s.items = self.world.items.len();
        s.nav_rebuilds = self.nav.rebuilds;
        s.phase_times = vec![("think", self.phase[0]), ("paths", self.phase[1]), ("commit", self.phase[2]), ("items", self.phase[3]), ("production", self.phase[4])];
        s
    }

    /// Deterministic hash of the world state (for replay/determinism tests).
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        mix(self.ticks);
        for (u, ai) in self.world.units.iter().zip(&self.ai) {
            mix(u.id as u64);
            mix(((u.pos.x as u64) << 32) | ((u.pos.y as u64) << 16) | u.pos.z as u64);
            let (th, hu, sl) = self.needs_now(ai);
            mix(((th as u64) << 40) ^ ((hu as u64) << 20) ^ sl as u64);
            mix(u.flags.dead as u64);
        }
        for t in &self.world.tiles {
            mix(((t.tt as u64) << 16) | t.desig as u64);
        }
        for it in &self.world.items {
            mix(it.id as u64);
            mix(((it.pos.x as u64) << 32) | ((it.pos.y as u64) << 16) | it.pos.z as u64);
            mix(it.stack as u64);
        }
        self.prod_hash(&mut mix);
        self.combat_hash(&mut mix); // [combat]
        h
    }

    // ------------------------------------------------------------------ extra public API

    /// Exact DF need counters (thirst, hunger, sleepiness) of a unit right now.
    pub fn unit_needs(&self, unit_id: i32) -> Option<(i32, i32, i32)> {
        self.unit_slot.get(&unit_id).map(|&s| self.needs_now(&self.ai[s]))
    }

    /// Remaining path of a unit (for debug overlays).
    pub fn unit_path(&self, unit_id: i32) -> Vec<Pos> {
        match self.unit_slot.get(&unit_id) {
            Some(&s) => {
                let ai = &self.ai[s];
                ai.path[ai.path_i.min(ai.path.len())..].iter().map(|&t| self.world.pos_of(t as usize)).collect()
            }
            None => Vec::new(),
        }
    }

    pub fn is_walkable(&self, p: Pos) -> bool {
        self.world.idx(p).map(|i| self.nav.walkable(i)).unwrap_or(false)
    }

    /// O(1) "is there a walking connection" query (connected components).
    pub fn reachable(&self, a: Pos, b: Pos) -> bool {
        match (self.world.idx(a), self.world.idx(b)) {
            (Some(i), Some(j)) => {
                let (ci, cj) = (self.nav.comp_near(i), self.nav.comp_near(j));
                ci != 0 && ci == cj
            }
            _ => false,
        }
    }

    /// One-off A* path (positions after `a` up to `b`), or None.
    pub fn find_path(&self, a: Pos, b: Pos) -> Option<Vec<Pos>> {
        let (i, j) = (self.world.idx(a)?, self.world.idx(b)?);
        let mut out = Vec::new();
        let ok = ASTAR.with(|s| s.borrow_mut().search(&self.nav, i as u32, &Goals::one(j as u32), MAX_NODES, &mut out));
        if ok {
            Some(out.iter().map(|&t| self.world.pos_of(t as usize)).collect())
        } else {
            None
        }
    }

    /// Open designation jobs: (target, DF caption, claiming unit id).
    pub fn jobs(&self) -> Vec<(Pos, &'static str, Option<i32>)> {
        self.jobs
            .iter()
            .filter(|j| j.live)
            .map(|j| {
                let who = if j.claimed == NONE { None } else { Some(self.ai[j.claimed as usize].id) };
                (self.world.pos_of(j.tile as usize), j.kind.caption(), who)
            })
            .collect()
    }

    /// Tell the sim that tiles in the box were changed directly in `world` (e.g. by an editor).
    pub fn notify_tiles_changed(&mut self, a: Pos, b: Pos) {
        for z in a.z.min(b.z)..=a.z.max(b.z) {
            for y in a.y.min(b.y)..=a.y.max(b.y) {
                for x in a.x.min(b.x)..=a.x.max(b.x) {
                    if let Some(i) = self.world.idx(Pos::new(x, y, z)) {
                        self.after_tile_change(i);
                    }
                }
            }
        }
    }

    /// Override the minimum path batch size for parallel solving (default 8). Results do not
    /// depend on it; only speed does.
    pub fn set_parallel_threshold(&mut self, n: usize) {
        self.par_min_paths = n.max(1);
    }

    /// Number of walk components (debug).
    pub fn component_count(&self) -> usize {
        self.nav.component_count()
    }

    // ------------------------------------------------------------------ tick

    fn tick_inner(&mut self) {
        if self.world.units.len() != self.ai.len() {
            self.init_units();
        }
        self.ticks += 1;
        self.world.calendar.advance(1);
        let now = self.ticks;
        if now % production::PROD_PERIOD == 0 {
            let tp = Instant::now();
            self.production_pass(now);
            self.phase[4] += tp.elapsed().as_secs_f64();
        }
        self.combat_tick(now); // [combat]
        let t0 = Instant::now();
        for s in 0..self.ai.len() {
            if self.ai[s].wake <= now {
                self.think(s, now);
            }
        }
        let t1 = Instant::now();
        self.phase[0] += (t1 - t0).as_secs_f64();
        if self.n_path > 0 {
            self.solve_paths();
            let t2 = Instant::now();
            self.phase[1] += (t2 - t1).as_secs_f64();
            self.commit_paths(now);
            self.phase[2] += t2.elapsed().as_secs_f64();
        }
        if self.items_dirty {
            let t3 = Instant::now();
            self.compact_items();
            self.phase[3] += t3.elapsed().as_secs_f64();
        }
    }

    fn solve_paths(&mut self) {
        let nav = &self.nav;
        let jobs = &mut self.path_jobs[..self.n_path];
        let solve = |pj: &mut PathJob| {
            ASTAR.with(|a| {
                let mut a = a.borrow_mut();
                pj.ok = a.search(nav, pj.start, &pj.goals, MAX_NODES, &mut pj.out);
                pj.nodes = a.last_expanded;
            });
        };
        if jobs.len() >= self.par_min_paths {
            jobs.par_iter_mut().for_each(solve);
        } else {
            jobs.iter_mut().for_each(solve);
        }
        self.stats.paths += self.n_path as u64;
        self.stats.path_nodes += self.path_jobs[..self.n_path].iter().map(|p| p.nodes as u64).sum::<u64>();
    }

    fn commit_paths(&mut self, now: u64) {
        self.failed.clear();
        for k in 0..self.n_path {
            let pj = &mut self.path_jobs[k];
            let ai = &mut self.ai[pj.slot];
            if !ai.waiting || ai.req != pj.req {
                continue;
            }
            ai.waiting = false;
            if pj.ok {
                std::mem::swap(&mut ai.path, &mut pj.out);
                ai.path_i = 0;
                ai.wake = now + 1;
                if pj.goals.n == 1 && ai.path.len() >= 8 && self.path_cache.len() < PATH_CACHE_MAX {
                    self.path_cache.insert((pj.start, pj.goals.t[0]), ai.path.clone());
                }
            } else {
                self.failed.push(pj.slot);
            }
        }
        self.n_path = 0;
        if !self.failed.is_empty() {
            self.stats.path_failures += self.failed.len() as u64;
            if self.nav.suspect {
                self.nav.rebuild_components();
                self.path_cache.clear();
                self.epoch_jobs += 1;
                self.epoch_haul += 1;
            }
            let failed = std::mem::take(&mut self.failed);
            for &s in &failed {
                self.path_failed(s, now);
            }
            self.failed = failed;
        }
    }

    fn request_path(&mut self, s: usize, goals: Goals, now: u64) {
        let start = self.ai[s].pos;
        let ai = &mut self.ai[s];
        ai.path.clear();
        ai.path_i = 0;
        ai.req = ai.req.wrapping_add(1);
        if goals.as_slice().contains(&start) {
            ai.wake = now + 1;
            return;
        }
        if goals.n == 1 {
            if let Some(p) = self.path_cache.get(&(start, goals.t[0])) {
                ai.path.extend_from_slice(p);
                ai.wake = now + 1;
                self.stats.path_cache_hits += 1;
                return;
            }
        }
        ai.waiting = true;
        ai.wake = u64::MAX;
        let req = ai.req;
        if self.n_path == self.path_jobs.len() {
            self.path_jobs.push(PathJob { slot: 0, req: 0, start: 0, goals: Goals::default(), ok: false, nodes: 0, out: Vec::new() });
        }
        let pj = &mut self.path_jobs[self.n_path];
        pj.slot = s;
        pj.req = req;
        pj.start = start;
        pj.goals = goals;
        pj.ok = false;
        self.n_path += 1;
    }

    fn path_failed(&mut self, s: usize, now: u64) {
        let task = self.ai[s].task;
        if std::env::var_os("RF_DEBUG_PATHS").is_some() {
            let p = self.world.pos_of(self.ai[s].pos as usize);
            let goal = self.path_jobs.iter().find(|pj| pj.slot == s).map(|pj| self.world.pos_of(pj.goals.t[0] as usize));
            let gw = goal.map(|g| self.is_walkable(g));
            eprintln!("path fail t={now} unit {} at {p:?} walk={} task {task:?} goal {goal:?} walk={gw:?}", self.ai[s].id, self.is_walkable(p));
        }
        if let Task::GoJob(j) = task {
            let job = &mut self.jobs[j as usize];
            job.claimed = NONE;
            job.blocked_until = now + rf_core::TICKS_PER_DAY as u64;
            let (caption, pos) = (job.kind.caption(), self.world.pos_of(job.tile as usize));
            let name = self.world.units[self.ai[s].uidx].name.clone();
            self.announce(now, format!("{name} cancels {caption}: Could not find path."), Some(pos), AnnouncementKind::Job);
            self.ai[s].task = Task::Idle;
        } else if let Task::GoDrink(id) | Task::GoEat(id) | Task::GoPickup(id, _) | Task::Fetch(id) = task {
            if let Some(&i) = self.item_idx.get(&id) {
                self.imeta[i as usize].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
            }
            self.cancel_task(s);
        } else if let Task::GoSite = task {
            self.prod_block_site(s, now);
            self.cancel_task(s);
        } else if let Task::GoSleep(_) = task {
            self.cancel_task(s);
            self.start_sleep_here(s, now);
            return;
        } else {
            self.cancel_task(s);
        }
        let mut r = Rng::for_unit(self.seed, s, now, 99);
        self.ai[s].wake = now + 10 + r.below(20) as u64;
    }

    fn announce(&mut self, tick: u64, text: String, pos: Option<Pos>, kind: AnnouncementKind) {
        self.announcements.push(Announcement { tick, text, pos, kind });
        if self.announcements.len() > 10_000 {
            self.announcements.drain(..5_000);
        }
    }

    // ------------------------------------------------------------------ units

    fn needs_now(&self, ai: &UnitAi) -> (i32, i32, i32) {
        if ai.kind != Kind::Worker {
            return (ai.thirst, ai.hunger, ai.sleep);
        }
        let e = (self.ticks.saturating_sub(ai.last)) as i32;
        let sl = if ai.sleeping { (ai.sleep - needs::SLEEP_RECOVERY_PER_TICK * e).max(0) } else { ai.sleep + e };
        (ai.thirst + e, ai.hunger + e, sl)
    }

    fn update_needs(&mut self, s: usize, now: u64) {
        let ai = &mut self.ai[s];
        let e = now.saturating_sub(ai.last) as i32;
        if e == 0 {
            return;
        }
        ai.last = now;
        ai.thirst += e;
        ai.hunger += e;
        if ai.sleeping {
            ai.sleep = (ai.sleep - needs::SLEEP_RECOVERY_PER_TICK * e).max(0);
        } else {
            ai.sleep += e;
        }
    }

    fn sync_units(&mut self) {
        for k in 0..self.ai.len() {
            let ai = &self.ai[k];
            if ai.kind != Kind::Worker {
                continue;
            }
            let (th, hu, sl) = self.needs_now(ai);
            let u = &mut self.world.units[ai.uidx];
            u.thirst_timer = th;
            u.hunger_timer = hu;
            u.sleepiness_timer = sl;
        }
    }

    /// Ticks until the next need threshold that interrupts work or kills.
    fn need_cap(&self, s: usize) -> u64 {
        let ai = &self.ai[s];
        let a = next_threshold(ai.thirst, &[needs::THIRST_CANCEL, needs::THIRST_DEATH]);
        let b = next_threshold(ai.hunger, &[needs::HUNGER_CANCEL, needs::HUNGER_DEATH]);
        a.min(b).max(1)
    }

    fn think(&mut self, s: usize, now: u64) {
        match self.ai[s].kind {
            Kind::Dead => {
                self.ai[s].wake = u64::MAX;
                return;
            }
            Kind::Animal | Kind::Other => {
                // [combat] hostiles, fighting animals, leaving invaders
                if self.combat_owns(s) {
                    self.combat_think(s, now);
                } else {
                    self.think_simple(s, now);
                }
                return;
            }
            Kind::Worker => {}
        }
        self.update_needs(s, now);
        let (th, hu, sl) = (self.ai[s].thirst, self.ai[s].hunger, self.ai[s].sleep);
        if th >= needs::THIRST_DEATH {
            self.kill(s, now, "has died of dehydration");
            return;
        }
        if hu >= needs::HUNGER_DEATH {
            self.kill(s, now, "has starved to death");
            return;
        }
        let ai = &self.ai[s];
        let urgent = (th >= needs::THIRST_CANCEL && now >= ai.no_drink_until)
            || (hu >= needs::HUNGER_CANCEL && now >= ai.no_food_until)
            || (!ai.sleeping && sl >= needs::VERY_DROWSY);
        let task = ai.task;
        match task {
            Task::Idle => self.decide(s, now),
            Task::Sleep(_) => {
                if sl <= 0 {
                    self.cancel_task(s);
                    self.decide(s, now);
                } else {
                    let until = now + ((sl + needs::SLEEP_RECOVERY_PER_TICK - 1) / needs::SLEEP_RECOVERY_PER_TICK) as u64;
                    let cap = now + self.need_cap(s);
                    self.ai[s].wake = until.min(cap);
                }
            }
            Task::Job(_) | Task::Drink(_) | Task::Eat(_) | Task::Craft => {
                let until = self.ai[s].act_until;
                if now >= until {
                    self.complete(s, now);
                } else if urgent && matches!(task, Task::Job(_) | Task::Craft) {
                    self.cancel_task(s);
                    self.decide(s, now);
                } else {
                    let cap = now + self.need_cap(s);
                    self.ai[s].wake = until.min(cap);
                }
            }
            Task::Combat => self.combat_think(s, now), // [combat]
            _ => {
                let interruptible = matches!(task, Task::GoJob(_) | Task::GoPickup(..) | Task::GoStore(..) | Task::Wander | Task::Fetch(_) | Task::GoSite);
                if urgent && interruptible {
                    self.cancel_task(s);
                    self.decide(s, now);
                    return;
                }
                if self.ai[s].waiting {
                    return;
                }
                if self.ai[s].path_i < self.ai[s].path.len() {
                    self.step(s, now);
                } else {
                    self.arrive(s, now);
                }
            }
        }
    }

    /// Animals and non-citizens: cheap random wandering.
    fn think_simple(&mut self, s: usize, now: u64) {
        let mut r = Rng::for_unit(self.seed, s, now, 3);
        if !self.random_step(s, now, &mut r, None) {
            self.ai[s].wake = now + 200;
        } else {
            self.ai[s].wake += 20 + r.below(80) as u64;
        }
    }

    /// Move to a random walkable neighbour on the same z-level (optionally inside a zone).
    fn random_step(&mut self, s: usize, now: u64, r: &mut Rng, zone: Option<Zone>) -> bool {
        let pos = self.ai[s].pos as usize;
        let (x, y, z) = self.nav.xyz(pos);
        let start = r.below(8) as usize;
        for k in 0..8 {
            let (ddx, ddy) = DIRS8[(start + k) % 8];
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
            let q = self.nav.idx(nx as usize, ny as usize, z);
            if self.nav.walkable(q) {
                self.move_to(s, q as u32, now);
                return true;
            }
        }
        false
    }

    fn move_to(&mut self, s: usize, q: u32, now: u64) {
        self.stats.steps += 1;
        let p = self.world.pos_of(q as usize);
        let ai = &mut self.ai[s];
        ai.pos = q;
        let t = ai.speed + ai.frac;
        ai.frac = t % 100;
        ai.wake = now + (t / 100).max(1) as u64;
        let (uidx, carrying) = (ai.uidx, ai.carrying);
        self.world.units[uidx].pos = p;
        if carrying >= 0 {
            self.set_item_pos(carrying, p);
        }
    }

    fn step(&mut self, s: usize, now: u64) {
        let ai = &self.ai[s];
        let next = ai.path[ai.path_i];
        if !self.nav.walkable(next as usize) {
            // The way got blocked: search again towards the same destination.
            let dest = *ai.path.last().unwrap();
            self.request_path(s, Goals::one(dest), now);
            return;
        }
        self.ai[s].path_i += 1;
        self.move_to(s, next, now);
    }

    fn decide(&mut self, s: usize, now: u64) {
        self.stats.decisions += 1;
        let mut r = Rng::for_unit(self.seed, s, now, 1);
        let ai = &mut self.ai[s];
        let elapsed = (now.saturating_sub(ai.last_decide)).clamp(1, 120) as u32;
        ai.last_decide = now;
        ai.task = Task::Idle;
        let (th, hu, sl) = (ai.thirst, ai.hunger, ai.sleep);
        if now >= ai.no_drink_until
            && (th >= needs::THIRST_SEEK || (th >= needs::THIRST_CONSIDER && r.chance(elapsed, 120)))
            && self.try_drink(s, now)
        {
            return;
        }
        let ai = &self.ai[s];
        if now >= ai.no_food_until
            && (hu >= needs::HUNGER_SEEK || (hu >= needs::HUNGER_CONSIDER && r.chance(elapsed, 120)))
            && self.try_eat(s, now)
        {
            return;
        }
        if sl >= needs::SLEEP_SEEK || (sl >= needs::SLEEP_CONSIDER && r.chance(elapsed, 120)) {
            self.start_sleep(s, now);
            return;
        }
        // [combat] soldiers on duty, civilian alert, wounded resting
        if self.combat_decide(s, now) {
            return;
        }
        let ai = &self.ai[s];
        let labors = ai.labors;
        let jobs_changed = labors & labor::ANY_JOB != 0 && ai.seen_jobs != self.epoch_jobs;
        let haul_changed = labors & labor::ANY_HAUL != 0 && ai.seen_haul != self.epoch_haul;
        let prod_changed = labors & labor::ANY_PROD != 0 && ai.seen_prod != self.prod.epoch;
        let periodic = now >= ai.next_search;
        if jobs_changed || haul_changed || prod_changed || periodic {
            // Each search only runs when its own work list changed (or periodically).
            if (jobs_changed || periodic) && self.try_job(s, now) {
                return;
            }
            if (prod_changed || periodic) && self.try_prod_timed(s, now) {
                return;
            }
            if (haul_changed || periodic) && self.try_haul(s, now) {
                return;
            }
            let (ej, eh, ep) = (self.epoch_jobs, self.epoch_haul, self.prod.epoch);
            let ai = &mut self.ai[s];
            ai.seen_jobs = ej;
            ai.seen_haul = eh;
            ai.seen_prod = ep;
            ai.next_search = now + RESEARCH_TICKS + (s as u64 % 64);
        }
        self.wander(s, now, &mut r);
    }

    fn wander(&mut self, s: usize, now: u64, r: &mut Rng) {
        if !self.zones.is_empty() {
            let pos = self.ai[s].pos as usize;
            let (x, y, z) = self.nav.xyz(pos);
            let inside = self.zones.iter().copied().find(|zn| {
                zn.z as usize == z
                    && (zn.x1 as usize..=zn.x2 as usize).contains(&x)
                    && (zn.y1 as usize..=zn.y2 as usize).contains(&y)
            });
            match inside {
                Some(zn) => {
                    if r.chance(1, 3) {
                        self.random_step(s, now, r, Some(zn));
                    } else {
                        self.ai[s].wake = now + 1;
                    }
                    self.ai[s].wake += 10 + r.below(40) as u64;
                    return;
                }
                None => {
                    let zn = self.zones[r.below(self.zones.len() as u32) as usize];
                    let tx = zn.x1 + r.below((zn.x2 - zn.x1 + 1) as u32) as i16;
                    let ty = zn.y1 + r.below((zn.y2 - zn.y1 + 1) as u32) as i16;
                    if let Some(t) = self.world.idx(Pos::new(tx, ty, zn.z)) {
                        let c = self.nav.comp_near(pos);
                        if self.nav.walkable(t) && c != 0 && self.nav.comp_root(t) == c {
                            self.ai[s].task = Task::Wander;
                            self.request_path(s, Goals::one(t as u32), now);
                            return;
                        }
                    }
                }
            }
        }
        if r.chance(1, 2) && self.random_step(s, now, r, None) {
            self.ai[s].wake += 10 + r.below(40) as u64;
        } else {
            self.ai[s].wake = now + 10 + r.below(40) as u64;
        }
    }

    fn arrive(&mut self, s: usize, now: u64) {
        let task = self.ai[s].task;
        match task {
            Task::Wander | Task::Idle => {
                self.ai[s].task = Task::Idle;
                let mut r = Rng::for_unit(self.seed, s, now, 5);
                self.ai[s].wake = now + 1 + r.below(30) as u64;
            }
            Task::GoJob(j) => {
                let job = self.jobs[j as usize];
                if job.live && job.claimed == s as u32 {
                    let rating = self.world.units[self.ai[s].uidx].skill(job.kind.skill());
                    let base = match job.kind {
                        JobKind::Dig(_) => 120,
                        JobKind::Chop => 200,
                        JobKind::Gather => 60,
                    };
                    let dur = (base * 10 / (10 + 2 * rating.clamp(0, 20))).max(10) as u64;
                    let cap = self.need_cap(s);
                    let ai = &mut self.ai[s];
                    ai.task = Task::Job(j);
                    ai.act_until = now + dur;
                    ai.wake = (now + dur).min(now + cap);
                } else {
                    self.ai[s].task = Task::Idle;
                    self.decide(s, now);
                }
            }
            Task::GoDrink(id) | Task::GoEat(id) => {
                if id < 0 || self.item_reachable_here(s, id) {
                    let drink = matches!(task, Task::GoDrink(_));
                    let dur = if drink { needs::DRINK_TICKS } else { needs::EAT_TICKS };
                    let ai = &mut self.ai[s];
                    ai.task = if drink { Task::Drink(id) } else { Task::Eat(id) };
                    ai.act_until = now + dur;
                    ai.wake = now + dur;
                } else {
                    self.cancel_task(s);
                    self.ai[s].wake = now + 1;
                }
            }
            Task::GoSleep(b) => {
                self.ai[s].task = Task::Sleep(b);
                self.ai[s].sleeping = true;
                self.ai[s].wake = now + 1;
            }
            Task::GoPickup(id, dest) => {
                if self.item_reachable_here(s, id) {
                    self.pickup(s, id);
                    self.ai[s].task = Task::GoStore(id, dest);
                    self.request_path(s, Goals::one(dest), now);
                } else {
                    self.cancel_task(s);
                    self.ai[s].wake = now + 1;
                }
            }
            Task::GoStore(id, dest) => {
                self.drop_item(s, id, dest);
                self.ai[s].task = Task::Idle;
                self.stats.items_hauled += 1;
                self.decide(s, now);
            }
            Task::Fetch(id) => self.prod_arrive_fetch(s, id, now),
            Task::GoSite => self.prod_arrive_site(s, now),
            Task::Job(_) | Task::Drink(_) | Task::Eat(_) | Task::Sleep(_) | Task::Craft => {}
            Task::Combat => self.combat_think(s, now), // [combat]
        }
    }

    fn complete(&mut self, s: usize, now: u64) {
        match self.ai[s].task {
            Task::Job(j) => {
                let kind = self.jobs[j as usize].kind;
                let tile = self.jobs[j as usize].tile as usize;
                if self.jobs[j as usize].live {
                    match kind {
                        JobKind::Dig(d) => self.dig_tile(s, tile, d, now),
                        JobKind::Chop => self.fell_tree(s, tile, now),
                        JobKind::Gather => self.gather(s, tile, now),
                    }
                    self.add_xp(s, kind.skill(), 30);
                    self.stats.jobs_done += 1;
                    self.jobs[j as usize].claimed = NONE;
                    self.remove_job(j);
                }
                self.ai[s].task = Task::Idle;
            }
            Task::Drink(id) => {
                let ai = &mut self.ai[s];
                ai.thirst = (ai.thirst - needs::DRINK_SATISFIES).max(0);
                if id >= 0 {
                    self.unreserve(id);
                    self.consume(id);
                }
                self.ai[s].task = Task::Idle;
            }
            Task::Eat(id) => {
                let ai = &mut self.ai[s];
                ai.hunger = (ai.hunger - needs::EAT_SATISFIES).max(0);
                self.unreserve(id);
                // Wiki "Seed": eating a plant raw leaves a seed.
                let seed_plant = self.item_idx.get(&id).map(|&i| &self.imeta[i as usize]).filter(|m| m.pc == production::pc::PLANT).map(|m| m.plant);
                self.consume(id);
                if let Some(p) = seed_plant {
                    let here = self.ai[s].pos as usize;
                    self.make_seed(p, here);
                }
                self.ai[s].task = Task::Idle;
            }
            Task::Craft => {
                self.prod_complete(s, now);
            }
            _ => {}
        }
        self.decide(s, now);
    }

    fn kill(&mut self, s: usize, now: u64, why: &str) {
        self.cancel_task(s);
        let ai = &mut self.ai[s];
        ai.kind = Kind::Dead;
        ai.wake = u64::MAX;
        let uidx = ai.uidx;
        let tile = ai.pos;
        let u = &mut self.world.units[uidx];
        u.flags.dead = true;
        let (name, pos) = (u.name.clone(), u.pos);
        self.announce(now, format!("{name} {why}."), Some(pos), AnnouncementKind::Death);
        let mat = self
            .world
            .materials
            .iter()
            .position(|m| m.token.ends_with(":SKIN") || m.token.starts_with("CREATURE:"))
            .map(|m| m as u16)
            .unwrap_or(NO_MAT);
        self.create_item("CORPSE", mat, tile as usize, format!("{name}'s corpse"));
    }

    fn cancel_task(&mut self, s: usize) {
        let task = self.ai[s].task;
        match task {
            Task::GoJob(j) | Task::Job(j) => {
                if self.jobs[j as usize].claimed == s as u32 {
                    self.jobs[j as usize].claimed = NONE;
                    self.epoch_jobs += 1;
                }
            }
            Task::GoDrink(id) | Task::Drink(id) | Task::GoEat(id) | Task::Eat(id) => self.unreserve(id),
            Task::GoSleep(b) | Task::Sleep(b) => {
                if b >= 0 {
                    self.beds[b as usize].occupant = NONE;
                }
                self.ai[s].sleeping = false;
            }
            Task::GoPickup(id, dest) => {
                self.set_hauling(id, false);
                self.release_pile_tile(dest);
                self.haul_add(id);
            }
            Task::GoStore(id, dest) => {
                let here = self.ai[s].pos;
                self.drop_item(s, id, here);
                self.release_pile_tile(dest);
            }
            Task::Fetch(_) | Task::GoSite | Task::Craft => self.prod_cancel(s),
            Task::Combat => self.combat_cancel(s), // [combat]
            Task::Idle | Task::Wander => {}
        }
        let ai = &mut self.ai[s];
        ai.task = Task::Idle;
        ai.path.clear();
        ai.path_i = 0;
        ai.waiting = false;
    }

    // ------------------------------------------------------------------ needs

    fn try_drink(&mut self, s: usize, now: u64) -> bool {
        match self.nearest_consumable(s, true, now) {
            Some(id) => {
                self.reserve(id);
                self.ai[s].task = Task::GoDrink(id);
                let g = self.item_goals(id);
                self.request_path(s, g, now);
                true
            }
            None => match self.nearest_water(s) {
                // No booze: drink from a water source like DF does (Task id -1 = water).
                Some(t) => {
                    self.ai[s].task = Task::GoDrink(-1);
                    self.request_path(s, Goals::one(t), now);
                    true
                }
                None => {
                    self.ai[s].no_drink_until = now + 1200;
                    false
                }
            },
        }
    }

    fn try_eat(&mut self, s: usize, now: u64) -> bool {
        match self.nearest_consumable(s, false, now) {
            Some(id) => {
                self.reserve(id);
                self.ai[s].task = Task::GoEat(id);
                let g = self.item_goals(id);
                self.request_path(s, g, now);
                true
            }
            None => {
                self.ai[s].no_food_until = now + 1200;
                false
            }
        }
    }

    fn nearest_consumable(&mut self, s: usize, drink: bool, now: u64) -> Option<i32> {
        self.stats.need_searches += 1;
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return None;
        }
        let up = self.nav.xyz(pos);
        let list = if drink { &self.drinks } else { &self.foods };
        let mut best = (i32::MAX, -1);
        for &id in list {
            let idx = match self.item_idx.get(&id) {
                Some(&i) => i as usize,
                None => continue,
            };
            let (it, m) = (&self.world.items[idx], &self.imeta[idx]);
            if m.dead || m.hauling || it.stack - m.reserved <= 0 || it.flags.forbid || it.holder >= 0 || m.blocked_until > now {
                continue;
            }
            let t = match self.world.idx(it.pos) {
                Some(t) => t,
                None => continue,
            };
            let d = manhattan(up, self.nav.xyz(t));
            if d < best.0 && self.nav.comp_near(t) == c {
                best = (d, id);
            }
        }
        if best.1 >= 0 {
            Some(best.1)
        } else {
            None
        }
    }

    fn start_sleep(&mut self, s: usize, now: u64) {
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        let up = self.nav.xyz(pos);
        let uid = self.ai[s].id;
        let mut best: (i32, i32) = (i32::MAX, -1);
        // Own bed first, then the nearest free unowned bed (wiki "Sleep").
        for (b, bed) in self.beds.iter().enumerate() {
            if bed.occupant != NONE || (bed.owner >= 0 && bed.owner != uid) {
                continue;
            }
            if c == 0 || self.nav.comp_near(bed.tile as usize) != c {
                continue;
            }
            let d = if bed.owner == uid { -1 } else { manhattan(up, self.nav.xyz(bed.tile as usize)) };
            if d < best.0 {
                best = (d, b as i32);
            }
        }
        if best.1 >= 0 {
            let b = best.1;
            self.beds[b as usize].occupant = s as u32;
            self.ai[s].task = Task::GoSleep(b);
            let t = self.beds[b as usize].tile;
            self.request_path(s, Goals::one(t), now);
        } else {
            self.start_sleep_here(s, now);
        }
    }

    fn start_sleep_here(&mut self, s: usize, now: u64) {
        let ai = &mut self.ai[s];
        ai.task = Task::Sleep(-1);
        ai.sleeping = true;
        ai.wake = now + 1;
    }

    // ------------------------------------------------------------------ jobs

    fn tree_base(&self, mut i: usize) -> usize {
        let layer = self.nav.layer;
        while i >= layer {
            let below = self.world.tiletypes.get(self.world.tiles[i - layer].tt);
            if below.material == TtMaterial::Tree && below.shape == Shape::Wall {
                i -= layer;
            } else {
                break;
            }
        }
        i
    }

    fn job_kind_for(&self, i: usize, d: Dig) -> Option<JobKind> {
        let info = self.world.tiletypes.get(self.world.tiles[i].tt);
        let (s, m) = (info.shape, info.material);
        let wall = s.is_solid() && m.is_diggable();
        let floor = matches!(s, Shape::Floor | Shape::Pebbles | Shape::Boulder) && m.is_diggable();
        match d {
            Dig::None => None,
            Dig::Dig => {
                if m == TtMaterial::Tree && s == Shape::Wall {
                    Some(JobKind::Chop)
                } else if s == Shape::Shrub {
                    Some(JobKind::Gather)
                } else if wall {
                    Some(JobKind::Dig(Dig::Dig))
                } else {
                    None
                }
            }
            Dig::Ramp => wall.then_some(JobKind::Dig(d)),
            Dig::Channel | Dig::UpDownStair | Dig::UpStair | Dig::DownStair => (wall || floor).then_some(JobKind::Dig(d)),
        }
    }

    fn add_job(&mut self, tile: u32, kind: JobKind) {
        if let Some(&j) = self.job_at.get(&tile) {
            if self.jobs[j as usize].kind == kind {
                return;
            }
            self.remove_job(j);
        }
        let mut job = Job { kind, tile, claimed: NONE, blocked_until: 0, live: true, access: false };
        job.access = self.work_positions(&job, 0).n > 0;
        let j = match self.free_jobs.pop() {
            Some(j) => {
                self.jobs[j as usize] = job;
                j
            }
            None => {
                self.jobs.push(job);
                (self.jobs.len() - 1) as u32
            }
        };
        self.job_at.insert(tile, j);
        let (x, y, z) = self.nav.xyz(tile as usize);
        let key = ((x / CHUNK) as u16, (y / CHUNK) as u16, z as u16);
        let ci = match self.chunk_of.get(&key) {
            Some(&c) => c,
            None => {
                self.chunks.push(JobChunk { x0: (key.0 as usize * CHUNK) as i32, y0: (key.1 as usize * CHUNK) as i32, z: z as i32, jobs: Vec::new() });
                let c = (self.chunks.len() - 1) as u32;
                self.chunk_of.insert(key, c);
                c
            }
        };
        self.chunks[ci as usize].jobs.push(j);
        self.open_jobs += 1;
        self.epoch_jobs += 1;
    }

    fn remove_job(&mut self, j: u32) {
        let job = self.jobs[j as usize];
        if !job.live {
            return;
        }
        if job.claimed != NONE {
            let s = job.claimed as usize;
            if matches!(self.ai[s].task, Task::GoJob(x) | Task::Job(x) if x == j) {
                self.cancel_task(s);
                self.ai[s].wake = self.ticks + 1;
            }
        }
        self.jobs[j as usize].live = false;
        self.jobs[j as usize].claimed = NONE;
        self.job_at.remove(&job.tile);
        let (x, y, z) = self.nav.xyz(job.tile as usize);
        if let Some(&ci) = self.chunk_of.get(&((x / CHUNK) as u16, (y / CHUNK) as u16, z as u16)) {
            let v = &mut self.chunks[ci as usize].jobs;
            if let Some(p) = v.iter().position(|&x| x == j) {
                v.swap_remove(p);
            }
        }
        self.world.tiles[job.tile as usize].desig = 0;
        self.free_jobs.push(j);
        self.open_jobs -= 1;
        self.epoch_jobs += 1;
    }

    /// Where a worker may stand to do the job, limited to component `c` (0 = any).
    fn work_positions(&self, job: &Job, c: u32) -> Goals {
        let mut g = Goals::default();
        let t = job.tile as usize;
        let (x, y, z) = self.nav.xyz(t);
        let on_tile = matches!(job.kind, JobKind::Gather | JobKind::Dig(Dig::UpStair | Dig::DownStair | Dig::UpDownStair));
        let ok = |q: usize| self.nav.walkable(q) && (c == 0 || self.nav.comp_root(q) == c);
        if on_tile && ok(t) {
            g.push(t as u32);
        }
        // Stairs are also dug from the tile directly above (digging a shaft downwards).
        if matches!(job.kind, JobKind::Dig(Dig::UpStair | Dig::DownStair | Dig::UpDownStair)) && z + 1 < self.nav.dz {
            let a = t + self.nav.layer;
            if ok(a) {
                g.push(a as u32);
            }
        }
        let zs: &[isize] = if job.kind == JobKind::Chop { &[0, -1] } else { &[0] };
        for &dz in zs {
            let zz = z as isize + dz;
            if zz < 0 {
                continue;
            }
            for &(ddx, ddy) in &DIRS8 {
                let nx = x as isize + ddx;
                let ny = y as isize + ddy;
                if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                    continue;
                }
                let q = self.nav.idx(nx as usize, ny as usize, zz as usize);
                if ok(q) {
                    g.push(q as u32);
                }
            }
        }
        g
    }

    fn try_job(&mut self, s: usize, now: u64) -> bool {
        let labors = self.ai[s].labors;
        if labors & labor::ANY_JOB == 0 || self.open_jobs == 0 {
            return false;
        }
        self.stats.job_searches += 1;
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        let up = self.nav.xyz(pos);
        let (ux, uy, uz) = (up.0 as i32, up.1 as i32, up.2 as i32);
        // Visit chunks by their Manhattan lower bound, stop once no chunk can beat the best.
        let mut order = std::mem::take(&mut self.chunk_order);
        order.clear();
        let axis = |v: i32, lo: i32| if v < lo { lo - v } else if v > lo + CHUNK as i32 - 1 { v - lo - CHUNK as i32 + 1 } else { 0 };
        for (ci, ch) in self.chunks.iter().enumerate() {
            if !ch.jobs.is_empty() {
                order.push((axis(ux, ch.x0) + axis(uy, ch.y0) + (uz - ch.z).abs(), ci as u32));
            }
        }
        order.sort_unstable();
        let mut best = (i32::MAX, NONE);
        let mut best_goals = Goals::default();
        for &(lb, ci) in &order {
            if lb > best.0 {
                break;
            }
            for &j in &self.chunks[ci as usize].jobs {
                let job = &self.jobs[j as usize];
                if !job.access || job.claimed != NONE || job.blocked_until > now || labors & job.kind.labor() == 0 {
                    continue;
                }
                let d = manhattan(up, self.nav.xyz(job.tile as usize));
                if (d, j) >= best {
                    continue;
                }
                let g = self.work_positions(job, c);
                if g.n == 0 {
                    continue;
                }
                best = (d, j);
                best_goals = g;
            }
        }
        self.chunk_order = order;
        if best.1 == NONE {
            return false;
        }
        let j = best.1;
        self.jobs[j as usize].claimed = s as u32;
        self.ai[s].task = Task::GoJob(j);
        self.request_path(s, best_goals, now);
        true
    }

    fn add_xp(&mut self, s: usize, skill: &str, xp: i32) {
        let u = &mut self.world.units[self.ai[s].uidx];
        let sk = match u.skills.iter_mut().position(|k| k.id == skill) {
            Some(p) => &mut u.skills[p],
            None => {
                u.skills.push(Skill { id: skill.into(), rating: 0, xp: 0 });
                u.skills.last_mut().unwrap()
            }
        };
        sk.xp += xp;
        // DF: the next level needs 500 + 100 * current level experience.
        loop {
            let need = 500 + 100 * sk.rating;
            if sk.rating >= 20 || sk.xp < need {
                break;
            }
            sk.xp -= need;
            sk.rating += 1;
        }
    }

    // ------------------------------------------------------------------ digging

    fn set_tile(&mut self, i: usize, shape: Shape, class: TtMaterial, variant: u8, mat: u16) -> bool {
        let tt = &self.world.tiletypes;
        let id = tt
            .find(shape, class, TtSpecial::Normal, variant)
            .or_else(|| tt.find(shape, TtMaterial::Stone, TtSpecial::Normal, variant))
            .or_else(|| if shape == Shape::Empty { tt.by_name("OpenSpace") } else { None });
        match id {
            Some(id) => {
                let t = &mut self.world.tiles[i];
                t.tt = id;
                t.mat = mat;
                t.desig = 0;
                t.flags |= tile_flags::DIRTY;
                true
            }
            None => false,
        }
    }

    fn after_tile_change(&mut self, i: usize) {
        let (x, y, z) = self.nav.xyz(i);
        if self.nav.update_around(&self.world.tiles, x, y, z) {
            self.path_cache.clear();
        }
        if !self.water.is_empty() || self.is_water(i) {
            for zz in [z] {
                for yy in y.saturating_sub(1)..=(y + 1).min(self.nav.dy - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(self.nav.dx - 1) {
                        let q = self.nav.idx(xx, yy, zz);
                        if self.is_water(q) {
                            self.add_water_access_around(q);
                        }
                    }
                }
            }
        }
        // Jobs next to the change may have become accessible (or not).
        if !self.job_at.is_empty() {
            for zz in z.saturating_sub(1)..=(z + 1).min(self.nav.dz - 1) {
                for yy in y.saturating_sub(1)..=(y + 1).min(self.nav.dy - 1) {
                    for xx in x.saturating_sub(1)..=(x + 1).min(self.nav.dx - 1) {
                        let q = self.nav.idx(xx, yy, zz) as u32;
                        if let Some(&j) = self.job_at.get(&q) {
                            let acc = self.work_positions(&self.jobs[j as usize], 0).n > 0;
                            self.jobs[j as usize].access = acc;
                        }
                    }
                }
            }
        }
        self.epoch_jobs += 1;
        self.epoch_haul += 1;
    }

    fn reveal(&mut self, i: usize) {
        let (x, y, z) = self.nav.xyz(i);
        for zz in z.saturating_sub(1)..=(z + 1).min(self.nav.dz - 1) {
            for yy in y.saturating_sub(1)..=(y + 1).min(self.nav.dy - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(self.nav.dx - 1) {
                    if zz != z && (xx != x || yy != y) {
                        continue;
                    }
                    let q = self.nav.idx(xx, yy, zz);
                    self.world.tiles[q].flags &= !tile_flags::HIDDEN;
                }
            }
        }
    }

    /// Boulder/ore/gem yield of a removed wall (wiki "Mining": stone 25 %, ore 33 %, gems and
    /// special features 100 %, soil nothing).
    fn roll_yield(&self, class: TtMaterial, mat: u16, half: bool, r: &mut Rng) -> Option<&'static str> {
        let gem = self.world.material(mat).map(|m| m.is_gem).unwrap_or(false);
        let mut pct = match class {
            TtMaterial::Stone | TtMaterial::LavaStone => 25,
            TtMaterial::Mineral => {
                if gem {
                    100
                } else {
                    33
                }
            }
            TtMaterial::Feature => 100,
            _ => 0,
        };
        if half {
            pct /= 2;
        }
        if pct > 0 && r.below(100) < pct {
            Some(if gem { "ROUGH" } else { "BOULDER" })
        } else {
            None
        }
    }

    fn dig_tile(&mut self, s: usize, i: usize, d: Dig, now: u64) {
        let t = self.world.tiles[i];
        let info = self.world.tiletypes.get(t.tt);
        let (shape, material) = (info.shape, info.material);
        let solid = shape.is_solid();
        if !material.is_diggable() {
            return;
        }
        let class = dig_class(material);
        let mut r = Rng::for_unit(self.seed, s, now, 7);
        let var = 1 + r.below(4) as u8;
        let layer = self.nav.layer;
        let (_, _, z) = self.nav.xyz(i);
        let mut changed: [usize; 3] = [i, usize::MAX, usize::MAX];
        let mut yields: Vec<(&'static str, u16)> = Vec::new();
        let mut drop_tile = i;
        let roll = |sim: &Sim, r: &mut Rng, yields: &mut Vec<(&'static str, u16)>, class, mat, half| {
            if let Some(k) = sim.roll_yield(class, mat, half, r) {
                yields.push((k, mat));
            }
        };
        match d {
            Dig::None => return,
            Dig::Dig => {
                if !solid {
                    return;
                }
                roll(self, &mut r, &mut yields, class, t.mat, false);
                self.set_tile(i, Shape::Floor, class, var, t.mat);
            }
            Dig::UpStair | Dig::DownStair | Dig::UpDownStair => {
                if solid {
                    roll(self, &mut r, &mut yields, class, t.mat, false);
                }
                let sh = match d {
                    Dig::UpStair => Shape::StairUp,
                    Dig::DownStair => Shape::StairDown,
                    _ => Shape::StairUpdown,
                };
                self.set_tile(i, sh, class, 0, t.mat);
            }
            Dig::Ramp => {
                if !solid {
                    return;
                }
                roll(self, &mut r, &mut yields, class, t.mat, false);
                self.set_tile(i, Shape::Ramp, class, 0, t.mat);
                if z + 1 < self.nav.dz {
                    let above = i + layer;
                    let ash = self.nav.shape(self.world.tiles[above].tt);
                    if !ash.is_solid() {
                        self.set_tile(above, Shape::RampTop, TtMaterial::Air, 0, NO_MAT);
                        changed[1] = above;
                    }
                }
            }
            Dig::Channel => {
                if solid {
                    roll(self, &mut r, &mut yields, class, t.mat, false);
                }
                let mut ramp_below = false;
                if z > 0 {
                    let below = i - layer;
                    let bt = self.world.tiles[below];
                    let binfo = self.world.tiletypes.get(bt.tt);
                    let (bshape, bmat) = (binfo.shape, binfo.material);
                    if bshape.is_solid() && bmat.is_diggable() {
                        let bclass = dig_class(bmat);
                        roll(self, &mut r, &mut yields, bclass, bt.mat, true);
                        self.set_tile(below, Shape::Ramp, bclass, 0, bt.mat);
                        ramp_below = true;
                    }
                    changed[1] = below;
                    drop_tile = below;
                }
                if ramp_below {
                    self.set_tile(i, Shape::RampTop, TtMaterial::Air, 0, NO_MAT);
                } else {
                    self.set_tile(i, Shape::Empty, TtMaterial::Air, 0, NO_MAT);
                }
            }
        }
        self.world.tiles[i].desig = 0;
        self.reveal(i);
        for &c in &changed {
            if c != usize::MAX {
                self.after_tile_change(c);
            }
        }
        if !self.nav.walkable(drop_tile) {
            drop_tile = self.ai[s].pos as usize;
        }
        for (k, mat) in yields {
            let name = self.world.material(mat).map(|m| m.name.clone()).unwrap_or_default();
            let desc = if k == "ROUGH" { format!("rough {name}") } else { format!("{name} boulder") };
            self.create_item(k, mat, drop_tile, desc);
        }
    }

    fn fell_tree(&mut self, s: usize, base: usize, _now: u64) {
        let (bx, by, bz) = self.nav.xyz(base);
        let mut tree: Vec<usize> = vec![base];
        let mut k = 0;
        while k < tree.len() && tree.len() < 2000 {
            let c = tree[k];
            k += 1;
            let (x, y, z) = self.nav.xyz(c);
            for dz in -1i32..=1 {
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                        if nx < 0 || ny < 0 || nz < bz as i32 || nx >= self.nav.dx as i32 || ny >= self.nav.dy as i32 {
                            continue;
                        }
                        if nz >= self.nav.dz as i32 || (nx - bx as i32).abs() > 4 || (ny - by as i32).abs() > 4 || nz > bz as i32 + 25 {
                            continue;
                        }
                        let q = self.nav.idx(nx as usize, ny as usize, nz as usize);
                        if self.world.tiletypes.get(self.world.tiles[q].tt).material == TtMaterial::Tree && !tree.contains(&q) {
                            tree.push(q);
                        }
                    }
                }
            }
        }
        let wood_mat = self.world.tiles[base].mat;
        let trunks = tree.iter().filter(|&&q| self.nav.shape(self.world.tiles[q].tt) == Shape::Wall).count().max(1);
        let layer = self.nav.layer;
        for &q in &tree {
            let (_, _, z) = self.nav.xyz(q);
            let below_solid = z > 0 && self.nav.shape(self.world.tiles[q - layer].tt).is_solid();
            if q == base && below_solid {
                let b = self.world.tiles[q - layer];
                let bclass = dig_class(self.world.tiletypes.get(b.tt).material);
                self.set_tile(q, Shape::Floor, bclass, 1, b.mat);
            } else {
                self.set_tile(q, Shape::Empty, TtMaterial::Air, 0, NO_MAT);
            }
        }
        for &q in &tree {
            self.after_tile_change(q);
        }
        let positions: Vec<Pos> = tree.iter().map(|&q| self.world.pos_of(q)).collect();
        self.world.plants.retain(|p| !positions.contains(&p.pos));
        let drop = if self.nav.walkable(base) { base } else { self.ai[s].pos as usize };
        let name = self.world.material(wood_mat).map(|m| m.name.clone()).unwrap_or_default();
        for _ in 0..trunks {
            self.create_item("WOOD", wood_mat, drop, format!("{name} log"));
        }
    }

    fn gather(&mut self, s: usize, i: usize, _now: u64) {
        let t = self.world.tiles[i];
        let layer = self.nav.layer;
        let (_, _, z) = self.nav.xyz(i);
        let below_mat = if z > 0 { self.world.tiles[i - layer].mat } else { NO_MAT };
        if !self.set_tile(i, Shape::Floor, TtMaterial::GrassLight, 1, t.mat) {
            self.set_tile(i, Shape::Floor, TtMaterial::Soil, 1, below_mat);
        }
        self.after_tile_change(i);
        let p = self.world.pos_of(i);
        self.world.plants.retain(|pl| pl.pos != p);
        let name = self.world.material(t.mat).map(|m| m.name.clone()).unwrap_or_else(|| "plant".into());
        let drop = if self.nav.walkable(i) { i } else { self.ai[s].pos as usize };
        let id = self.create_item("PLANT", t.mat, drop, name);
        self.foods.push(id);
    }

    // ------------------------------------------------------------------ items & hauling

    /// Where to walk to reach an item: its tile, or the tiles next to it if it lies somewhere
    /// one cannot stand (DF lets units pick up items from adjacent tiles).
    fn item_goals(&self, id: i32) -> Goals {
        let t = self.item_tile(id) as usize;
        if self.nav.walkable(t) {
            return Goals::one(t as u32);
        }
        let mut g = Goals::default();
        let (x, y, z) = self.nav.xyz(t);
        for &(ddx, ddy) in &DIRS8 {
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

    fn item_tile(&self, id: i32) -> u32 {
        let idx = self.item_idx[&id] as usize;
        self.world.idx(self.world.items[idx].pos).unwrap_or(0) as u32
    }

    fn item_reachable_here(&self, s: usize, id: i32) -> bool {
        match self.item_idx.get(&id) {
            Some(&idx) => {
                let it = &self.world.items[idx as usize];
                if self.imeta[idx as usize].dead || it.holder >= 0 {
                    return false;
                }
                let here = self.world.pos_of(self.ai[s].pos as usize);
                here.dist(it.pos) <= 1
            }
            None => false,
        }
    }

    fn reserve(&mut self, id: i32) {
        if let Some(&i) = self.item_idx.get(&id) {
            self.imeta[i as usize].reserved += 1;
        }
    }

    fn unreserve(&mut self, id: i32) {
        if let Some(&i) = self.item_idx.get(&id) {
            let m = &mut self.imeta[i as usize];
            m.reserved = (m.reserved - 1).max(0);
        }
    }

    fn set_hauling(&mut self, id: i32, on: bool) {
        if let Some(&i) = self.item_idx.get(&id) {
            self.imeta[i as usize].hauling = on;
        }
    }

    fn set_item_pos(&mut self, id: i32, p: Pos) {
        if let Some(&i) = self.item_idx.get(&id) {
            self.world.items[i as usize].pos = p;
        }
        if let Some(c) = self.contents.get(&id) {
            for cid in c.clone() {
                if let Some(&ci) = self.item_idx.get(&cid) {
                    self.world.items[ci as usize].pos = p;
                }
            }
        }
    }

    fn create_item(&mut self, kind: &str, mat: u16, tile: usize, desc: String) -> i32 {
        let id = self.next_item_id;
        self.next_item_id += 1;
        let pos = self.world.pos_of(tile);
        self.world.items.push(Item {
            id,
            kind: kind.into(),
            mat,
            pos,
            stack: 1,
            container: -1,
            holder: -1,
            building: -1,
            desc,
            flags: ItemFlags { on_ground: true, ..Default::default() },
            ..Default::default()
        });
        let idx = self.world.items.len() - 1;
        let cat = Cat::of_kind(kind);
        let (pc, plant) = self.classify(&self.world.items[idx]);
        self.imeta.push(ItemMeta { reserved: 0, hauling: false, dead: false, cat, blocked_until: 0, pc, plant });
        self.item_idx.insert(id, idx as u32);
        self.pile_occ_add(tile as u32, 1);
        if !self.is_stored(idx) {
            self.haul_add(id);
        }
        id
    }

    fn consume(&mut self, id: i32) {
        let idx = match self.item_idx.get(&id) {
            Some(&i) => i as usize,
            None => return,
        };
        let it = &mut self.world.items[idx];
        it.stack -= 1;
        if it.stack <= 0 {
            let (pos, loose) = (it.pos, it.container < 0 && it.flags.on_ground);
            self.imeta[idx].dead = true;
            self.items_dirty = true;
            self.haul_remove(id);
            if loose {
                if let Some(t) = self.world.idx(pos) {
                    self.pile_occ_add(t as u32, -1);
                }
            }
        }
    }

    fn compact_items(&mut self) {
        self.items_dirty = false;
        let n = self.world.items.len();
        let mut w = 0;
        for r in 0..n {
            if !self.imeta[r].dead {
                self.world.items.swap(w, r);
                self.imeta.swap(w, r);
                w += 1;
            }
        }
        self.world.items.truncate(w);
        self.imeta.truncate(w);
        self.item_idx.clear();
        for (i, it) in self.world.items.iter().enumerate() {
            self.item_idx.insert(it.id, i as u32);
        }
        let idx = &self.item_idx;
        self.drinks.retain(|id| idx.contains_key(id));
        self.foods.retain(|id| idx.contains_key(id));
        self.haul.retain(|id| idx.contains_key(id));
        self.haul_pos.clear();
        self.haul_ix.clear();
        for (p, &id) in self.haul.iter().enumerate() {
            self.haul_pos.insert(id, p as u32);
            self.haul_ix.push(idx[&id]);
        }
        self.contents.retain(|cid, _| idx.contains_key(cid));
        for v in self.contents.values_mut() {
            v.retain(|id| idx.contains_key(id));
        }
    }

    fn pile_occ_add(&mut self, tile: u32, d: i32) {
        if let Some(&(p, k)) = self.pile_at.get(&tile) {
            let pile = &mut self.piles[p as usize];
            let o = &mut pile.occ[k as usize];
            let was_free = *o == 0 && !pile.res[k as usize];
            *o = (*o as i32 + d).max(0) as u16;
            let is_free = *o == 0 && !pile.res[k as usize];
            if was_free && !is_free {
                pile.free -= 1;
            } else if !was_free && is_free {
                pile.free += 1;
                self.epoch_haul += 1;
            }
        }
    }

    fn release_pile_tile(&mut self, tile: u32) {
        if let Some(&(p, k)) = self.pile_at.get(&tile) {
            let pile = &mut self.piles[p as usize];
            if pile.res[k as usize] {
                pile.res[k as usize] = false;
                if pile.occ[k as usize] == 0 {
                    pile.free += 1;
                    self.epoch_haul += 1;
                }
            }
        }
    }

    fn is_stored(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        match self.world.idx(it.pos).and_then(|t| self.pile_at.get(&(t as u32))) {
            Some(&(p, _)) => self.piles[p as usize].cats & self.imeta[idx].cat.bit() != 0,
            None => false,
        }
    }

    fn haulable(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        let f = &it.flags;
        f.on_ground
            && !f.forbid
            && !f.in_job
            && !f.construction
            && !f.in_building
            && it.container < 0
            && it.holder < 0
            && it.building < 0
            && !self.imeta[idx].dead
    }

    fn rebuild_haul(&mut self) {
        self.haul.clear();
        self.haul_ix.clear();
        self.haul_pos.clear();
        self.haul_cnt = [0; 17];
        for idx in 0..self.world.items.len() {
            if self.pile_cats & self.imeta[idx].cat.bit() != 0 && self.haulable(idx) && !self.is_stored(idx) && !self.imeta[idx].hauling {
                let id = self.world.items[idx].id;
                self.haul_add(id);
            }
        }
    }

    fn haul_add(&mut self, id: i32) {
        let idx = match self.item_idx.get(&id) {
            Some(&i) => i as usize,
            None => return,
        };
        let cat = self.imeta[idx].cat;
        if self.pile_cats & cat.bit() == 0 || self.haul_pos.contains_key(&id) {
            return;
        }
        self.haul_pos.insert(id, self.haul.len() as u32);
        self.haul.push(id);
        self.haul_ix.push(idx as u32);
        self.haul_cnt[cat as usize] += 1;
        self.epoch_haul += 1;
    }

    fn haul_remove(&mut self, id: i32) {
        if let Some(p) = self.haul_pos.remove(&id) {
            let p = p as usize;
            self.haul.swap_remove(p);
            self.haul_ix.swap_remove(p);
            if p < self.haul.len() {
                self.haul_pos.insert(self.haul[p], p as u32);
            }
            if let Some(&i) = self.item_idx.get(&id) {
                let cat = self.imeta[i as usize].cat as usize;
                self.haul_cnt[cat] = self.haul_cnt[cat].saturating_sub(1);
            }
        }
    }

    fn find_pile_tile(&self, cat: Cat, near: (usize, usize, usize), c: u32) -> Option<u32> {
        let mut best = (i32::MAX, NONE);
        for pile in &self.piles {
            if pile.cats & cat.bit() == 0 || pile.free == 0 {
                continue;
            }
            for (k, &t) in pile.tiles.iter().enumerate() {
                if pile.occ[k] != 0 || pile.res[k] {
                    continue;
                }
                let d = manhattan(near, self.nav.xyz(t as usize));
                if d < best.0 && self.nav.walkable(t as usize) && self.nav.comp_root(t as usize) == c {
                    best = (d, t);
                }
            }
        }
        (best.1 != NONE).then_some(best.1)
    }

    fn try_haul(&mut self, s: usize, now: u64) -> bool {
        let labors = self.ai[s].labors;
        if labors & labor::ANY_HAUL == 0 || self.haul.is_empty() {
            return false;
        }
        // Only categories with waiting items, a free pile tile and the right labor.
        let mut free_cats = 0u32;
        for p in &self.piles {
            if p.free > 0 {
                free_cats |= p.cats;
            }
        }
        let mut want = 0u32;
        for cat in Cat::ALL {
            if self.haul_cnt[cat as usize] > 0 && labors & cat.haul_labor() != 0 {
                want |= cat.bit();
            }
        }
        want &= free_cats;
        if want == 0 {
            return false;
        }
        self.stats.haul_searches += 1;
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        let up = self.nav.xyz(pos);
        while want != 0 {
            let mut best = (i32::MAX, i32::MAX, 0usize);
            for (k, &id) in self.haul.iter().enumerate() {
                let idx = self.haul_ix[k] as usize;
                let m = &self.imeta[idx];
                if want & m.cat.bit() == 0 || m.reserved > 0 || m.blocked_until > now {
                    continue;
                }
                let t = match self.world.idx(self.world.items[idx].pos) {
                    Some(t) => t,
                    None => continue,
                };
                let d = manhattan(up, self.nav.xyz(t));
                if (d, id) >= (best.0, best.1) || self.nav.comp_near(t) != c {
                    continue;
                }
                best = (d, id, t);
            }
            if best.0 == i32::MAX {
                return false;
            }
            let (id, t) = (best.1, best.2);
            let cat = self.imeta[self.item_idx[&id] as usize].cat;
            let dest = match self.find_pile_tile(cat, self.nav.xyz(t), c) {
                Some(d) => d,
                None => {
                    want &= !cat.bit();
                    continue;
                }
            };
            if let Some(&(p, k)) = self.pile_at.get(&dest) {
                let pile = &mut self.piles[p as usize];
                pile.res[k as usize] = true;
                pile.free -= 1;
            }
            self.set_hauling(id, true);
            self.haul_remove(id);
            self.ai[s].task = Task::GoPickup(id, dest);
            let _ = t;
            let g = self.item_goals(id);
            self.request_path(s, g, now);
            return true;
        }
        false
    }

    fn pickup(&mut self, s: usize, id: i32) {
        let idx = self.item_idx[&id] as usize;
        let uid = self.ai[s].id;
        let it = &mut self.world.items[idx];
        let pos = it.pos;
        it.flags.on_ground = false;
        it.flags.in_inventory = true;
        it.holder = uid;
        if let Some(t) = self.world.idx(pos) {
            self.pile_occ_add(t as u32, -1);
        }
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.push(id);
        self.ai[s].carrying = id;
    }

    fn drop_item(&mut self, s: usize, id: i32, tile: u32) {
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != id);
        self.ai[s].carrying = -1;
        let idx = match self.item_idx.get(&id) {
            Some(&i) => i as usize,
            None => return,
        };
        let p = self.world.pos_of(tile as usize);
        self.set_item_pos(id, p);
        let it = &mut self.world.items[idx];
        it.flags.on_ground = true;
        it.flags.in_inventory = false;
        it.holder = -1;
        self.imeta[idx].hauling = false;
        // Arriving at the reserved pile tile: reservation becomes occupancy.
        if let Some(&(pl, k)) = self.pile_at.get(&tile) {
            let pile = &mut self.piles[pl as usize];
            if pile.res[k as usize] {
                pile.res[k as usize] = false;
                pile.occ[k as usize] += 1;
                if !self.is_stored(idx) {
                    self.haul_add(id);
                }
                return;
            }
        }
        self.pile_occ_add(tile, 1);
        if !self.is_stored(idx) {
            self.haul_add(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gait_900_moves_one_tile_per_9_ticks() {
        let mut w = World::synthetic(48, 48, 20);
        w.units.truncate(1);
        let start = w.units[0].pos;
        let mut sim = Sim::new(w, 1);
        // Walk east along the hall row: 6 tiles.
        let path: Vec<u32> = (1..=6).map(|k| sim.world.idx(start.offset(k, 0, 0)).unwrap() as u32).collect();
        let ai = &mut sim.ai[0];
        ai.task = Task::Wander;
        ai.path = path;
        ai.path_i = 0;
        ai.wake = 1;
        // First step on tick 1, then one every 9 ticks: after tick 1 + 9k the unit made k+1 steps.
        sim.run_ticks(1);
        assert_eq!(sim.world.units[0].pos, start.offset(1, 0, 0));
        sim.run_ticks(9 * 3);
        assert_eq!(sim.world.units[0].pos, start.offset(4, 0, 0));
        sim.run_ticks(8);
        assert_eq!(sim.world.units[0].pos, start.offset(4, 0, 0));
        sim.run_ticks(1);
        assert_eq!(sim.world.units[0].pos, start.offset(5, 0, 0));
    }

    #[test]
    fn gait_fractional_speed_is_exact() {
        // 950 ticks per 100 tiles: 100 steps take exactly 950 ticks.
        let mut w = World::synthetic(48, 48, 20);
        w.units.truncate(1);
        w.units[0].walk_speed = 950;
        let mut sim = Sim::new(w, 1);
        let ai = &mut sim.ai[0];
        let mut total = 0u64;
        for _ in 0..100 {
            let t = ai.speed + ai.frac;
            ai.frac = t % 100;
            total += (t / 100) as u64;
        }
        assert_eq!(total, 950);
    }

    #[test]
    fn herbalist_gathers_shrubs() {
        let mut w = World::synthetic(48, 48, 20);
        let shrub = w.tiletypes.by_name("Shrub").unwrap();
        let hz = (20i16 * 2 / 3) - 4;
        let p = Pos::new(24 - 5, 24 + 4, hz);
        w.tile_mut(p).unwrap().tt = shrub;
        w.units[5].labors.push("HERBALISM".into());
        let mut sim = Sim::new(w, 2);
        let plants0 = sim.world.items.iter().filter(|i| i.kind == "PLANT").count();
        sim.designate(p, p, DesignationKind::GatherPlants);
        assert_eq!(sim.stats().open_jobs, 6);
        sim.run_ticks(3000);
        assert_eq!(sim.world.shape(p), Shape::Floor);
        assert_eq!(sim.world.items.iter().filter(|i| i.kind == "PLANT").count(), plants0 + 1);
    }

    #[test]
    fn nav_basics_on_synthetic() {
        let w = World::synthetic(48, 48, 20);
        let sim = Sim::new(w, 1);
        let u = sim.world.units[0].pos;
        assert!(sim.is_walkable(u));
        let surface = Pos::new(10, 10, (20 * 2 / 3) as i16);
        assert!(sim.is_walkable(surface));
        assert!(sim.reachable(u, surface), "hall and surface are connected by the stair shaft");
        let p = sim.find_path(u, surface).expect("path");
        assert_eq!(*p.last().unwrap(), surface);
    }
}
