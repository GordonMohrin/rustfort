//! [carts12] Minecarts and hauling routes, tracks, track stops, rollers, wheelbarrows and the
//! weight of carried items (round 12).
//!
//! # DF rules implemented (wiki v50: Minecart, Hauling, Track stop, Roller, Track, Wheelbarrow)
//! - **Routes** (`World::haul_routes`) have stops (a tile, normally a track stop), stockpile
//!   links per stop (`take_from`: the stop loads from the pile, `give_to`: it unloads into it)
//!   and departure conditions; minecart items are assigned as vehicles. A cart is first brought
//!   to the route's first stop by a dwarf.
//! - **Loading / unloading:** at a stop, dwarves with the hauling labor of the item (Stone,
//!   Wood, Item ...) carry items one at a time from the linked piles into the cart, or from the
//!   cart into free tiles of the linked piles. Capacity 500 000 cm³ ("five times a
//!   wheelbarrow": 5 stones, 10 logs, 83 bars or blocks, 500 meals, 2 500 cloth).
//! - **Departure:** a dwarf with **Push/Haul Vehicles** (`HANDLE_VEHICLES`) pushes, rides or
//!   guides the cart once the stop's conditions hold: fill level (`Leave when full`, `... 75 %
//!   full`, `Leave when empty`) and timers (`Wait N days`). A push "teleports the cart to the
//!   middle of the next tile with 19990 speed"; a rider gives the same push and rides along; a
//!   guided cart moves with the walking dwarf and "ignores all laws of physics" (ramps too).
//! - **Physics:** speed in 1/100 000 tile per tick (100 000 = one tile per tick); every tick
//!   the cart moves by its speed, then friction is taken off: track 10, floor 200, track stops
//!   by setting (highest 50 000, high 10 000, medium 500, low 50, lowest 10), up a ramp 4 910,
//!   down a ramp -4 890 (it accelerates). A ramp is 141 420 long. Corners turn the cart and
//!   cost 1 000 speed; above 50 000 a cart **derails** at a corner (keeps going straight)
//!   unless a wall blocks that way. At a track end or blocked T-junction the cart "leaves the
//!   track and continues straight" until it stops (floor friction 200), hits a wall or falls.
//! - **Track stops** dump the cart's contents onto the tile in their dump direction when the
//!   cart enters them (items fall down to the first floor).
//! - **Rollers** (powered through `power`, 2 power per tile) give a cart on them their set
//!   speed in their direction (lowest 10 000 .. highest 50 000) when it is slower.
//! - **Minecarts are items** (`TOOL:ITEM_TOOL_MINECART`, 40 000 cm³ of material: oak 28Γ,
//!   iron 314Γ). The weight matters where a dwarf moves the cart: guiding it or carrying it
//!   back to the route (see "Carried weight").
//! - **Wheelbarrows** (`TOOL:ITEM_TOOL_WHEELBARROW`, 100 000 cm³) are assigned to stockpiles
//!   with `max_wheelbarrows` > 0. Items of **75Γ or more** hauled to such a pile go by
//!   wheelbarrow: the hauler fetches it, loads the item and "moves always at top speed",
//!   ignoring the weight; the wheelbarrow stays at the pile. "A stockpile's wheelbarrows are
//!   only used to carry items to that stockpile".
//! - **Item weight** = density (kg/m³) x volume (cm³) / 1 000 000 Γ.
//!
//! # Estimates (not on the wiki)
//! - **Carried weight:** DF slows a creature that carries more than it can, without a
//!   published formula. Here a dwarf carrying a stockpile haul, a cart or a guided cart above
//!   75Γ walks `1 + (weight - 75) / 300` times slower (at most 4x): a granite boulder (260Γ)
//!   1.6x, an empty iron cart 1.8x, a full cart of stone 4x.
//! - Item volumes besides the wiki's table (boulder 100 000, log 50 000, bar/block 6 000,
//!   meal 1 000, cloth 200): gems 1 200, other items 5 000 cm³.
//! - Stops load anything from their linked piles (DF's per-stop item filter is not
//!   modelled); without a fill condition a stop departs when nothing more can be loaded or
//!   unloaded. Guided carts follow the walking path, not the track.
//! - A cart that stops away from a stop (derailed, fell, ran out of speed) is carried by a
//!   dwarf to the stop it was heading for. Cart collisions and the checkpoint effect are not
//!   modelled; a ridden cart that falls just stops.

use crate::path::Goals;
use crate::{labor, Sim, Task, NONE};
use rf_core::{Pos, Shape, TtSpecial};
use serde::{Deserialize, Serialize};

/// Length of a tile and of a ramp in cart position units (wiki).
pub const TILE_LEN: i32 = 100_000;
pub const RAMP_LEN: i32 = 141_420;
/// A dwarf's push (wiki: "19990 speed").
pub const PUSH_SPEED: i32 = 19_990;
/// Corners derail carts above this speed (wiki).
pub const DERAIL_SPEED: i32 = 50_000;
/// Speed lost on a corner (wiki).
pub const CORNER_LOSS: i32 = 1_000;
/// Friction per tick (wiki).
pub const FRICTION_TRACK: i32 = 10;
pub const FRICTION_FLOOR: i32 = 200;
pub const FRICTION_RAMP_UP: i32 = 4_910;
pub const FRICTION_RAMP_DOWN: i32 = -4_890;
/// Speed cap (ESTIMATE; keeps a cart below 2.7 tiles per tick).
pub const MAX_SPEED: i32 = 270_000;
/// Capacities in cm³ (wiki: minecart 500 000, wheelbarrow one fifth).
pub const CART_CAPACITY: i32 = 500_000;
pub const BARROW_CAPACITY: i32 = 100_000;
/// Material volume of a minecart (oak 28Γ, iron 314Γ on the wiki).
pub const CART_VOLUME: i32 = 40_000;
/// Items of at least this weight go by wheelbarrow (wiki).
pub const BARROW_MIN_WEIGHT: i32 = 75;
/// Carried weight without slowdown and the slope of the slowdown (ESTIMATE, see module docs).
pub const CARRY_FREE: i32 = 75;
pub const CARRY_SCALE: i32 = 300;
pub const CARRY_MAX_PCT: i32 = 400;
const PLAN_PERIOD: u64 = 50;
const PLAN_PHASE: u64 = 17;
const BARROW_PERIOD: u64 = 400;
const BARROW_PHASE: u64 = 233;
/// Concurrent load/unload jobs per cart.
const MAX_JOBS_PER_CART: usize = 3;

pub(crate) const DN: u8 = 1;
pub(crate) const DS: u8 = 2;
pub(crate) const DE: u8 = 4;
pub(crate) const DW: u8 = 8;

fn dvec(d: u8) -> (i16, i16) {
    match d {
        DN => (0, -1),
        DS => (0, 1),
        DE => (1, 0),
        _ => (-1, 0),
    }
}

fn opposite(d: u8) -> u8 {
    match d {
        DN => DS,
        DS => DN,
        DE => DW,
        _ => DE,
    }
}

/// Track direction bits of a DF direction string (`NSE`, ...).
fn dir_bits(s: &str) -> u8 {
    s.chars().fold(0, |a, c| {
        a | match c {
            'N' => DN,
            'S' => DS,
            'E' => DE,
            'W' => DW,
            _ => 0,
        }
    })
}

/// One direction from text: `N`, `North`, `E` ... `FromSouth` = towards the north.
pub fn parse_dir(s: &str) -> u8 {
    let u = s.trim().to_ascii_uppercase();
    let (from, rest) = match u.strip_prefix("FROM") {
        Some(r) => (true, r.trim_start_matches([' ', '_']).to_string()),
        None => (false, u.trim_start_matches("TO").trim_start_matches([' ', '_']).to_string()),
    };
    let d = match rest.chars().next() {
        Some('N') => DN,
        Some('S') => DS,
        Some('E') => DE,
        Some('W') => DW,
        _ => return 0,
    };
    if from {
        opposite(d)
    } else {
        d
    }
}

/// Track stop friction by DF's setting name (wiki); default highest.
pub fn stop_friction(label: &str) -> i32 {
    match label.trim().to_ascii_lowercase().as_str() {
        "high" => 10_000,
        "medium" => 500,
        "low" => 50,
        "lowest" => 10,
        "none" => 0,
        _ => 50_000,
    }
}

/// Roller speed by DF's setting name (wiki); default highest.
pub fn roller_speed(label: &str) -> i32 {
    match label.trim().to_ascii_lowercase().as_str() {
        "lowest" => 10_000,
        "low" => 20_000,
        "medium" => 30_000,
        "high" => 40_000,
        _ => 50_000,
    }
}

pub fn is_minecart(kind: &str, subtype: &str) -> bool {
    kind == "MINECART" || (kind == "TOOL" && subtype == "ITEM_TOOL_MINECART")
}

pub fn is_wheelbarrow(kind: &str, subtype: &str) -> bool {
    kind == "WHEELBARROW" || (kind == "TOOL" && subtype == "ITEM_TOOL_WHEELBARROW")
}

/// Item volume in cm³ for carts and wheelbarrows (wiki table, else ESTIMATE).
pub fn item_volume(kind: &str, subtype: &str, stack: i32) -> i32 {
    let n = stack.max(1);
    match kind {
        "BOULDER" => 100_000,
        "WOOD" => 50_000,
        "BAR" | "BLOCKS" => 6_000 * n,
        "FOOD" => 1_000 * n,
        "CLOTH" | "THREAD" => 200 * n,
        "ROUGH" | "SMALLGEM" | "GEM" => 1_200 * n,
        _ if is_minecart(kind, subtype) => CART_VOLUME,
        _ if is_wheelbarrow(kind, subtype) => BARROW_CAPACITY / 2,
        _ => 5_000 * n,
    }
}

/// How a cart leaves a stop.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DepartMode {
    #[default]
    Push,
    Ride,
    Guide,
}

/// A stop's departure conditions, parsed from its DF-like text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Conditions {
    pub mode: DepartMode,
    /// Push/ride direction bit, 0 = towards the next stop along the track.
    pub dir: u8,
    /// Required fill in percent (`Some(100)` = full), or None.
    pub fill: Option<i32>,
    /// Leave only when empty.
    pub empty: bool,
    /// Wait at least this many ticks after arriving.
    pub wait: u64,
}

/// Parse departure conditions like `Leave when full`, `Leave when 75% full`, `Leave when empty`,
/// `Wait 2 days`, `Push`, `Push East`, `Ride N`, `Guide`.
pub fn parse_conditions(conds: &[String]) -> Conditions {
    let mut c = Conditions::default();
    for raw in conds {
        let s = raw.to_ascii_lowercase();
        let words: Vec<&str> = s.split(|ch: char| ch.is_whitespace() || ch == ',').filter(|w| !w.is_empty()).collect();
        for (i, w) in words.iter().enumerate() {
            match *w {
                "push" | "ride" | "guide" => {
                    c.mode = match *w {
                        "ride" => DepartMode::Ride,
                        "guide" => DepartMode::Guide,
                        _ => DepartMode::Push,
                    };
                    if let Some(n) = words.get(i + 1) {
                        let d = parse_dir(n);
                        if d != 0 {
                            c.dir = d;
                        }
                    }
                }
                "full" => {
                    let pct = (i > 0).then(|| words[i - 1].trim_end_matches('%').parse::<i32>().ok()).flatten();
                    c.fill = Some(pct.unwrap_or(100).clamp(0, 100));
                }
                "empty" => c.empty = true,
                "day" | "days" | "month" | "months" | "week" | "weeks" => {
                    let n = (i > 0).then(|| words[i - 1].parse::<u64>().ok()).flatten().unwrap_or(1);
                    let per = match *w {
                        "month" | "months" => 28 * rf_core::TICKS_PER_DAY as u64,
                        "week" | "weeks" => 7 * rf_core::TICKS_PER_DAY as u64,
                        _ => rf_core::TICKS_PER_DAY as u64,
                    };
                    c.wait = c.wait.max(n * per);
                }
                _ => {}
            }
        }
    }
    c
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CartState {
    /// Assigned, not yet at the route: a dwarf brings it to the first stop.
    #[default]
    Idle,
    /// Standing at stop `stop`.
    AtStop,
    /// Rolling on its own (pushed or ridden), heading for stop `dest`.
    Moving,
    /// Carried or guided by a dwarf.
    Carried,
    /// Came to rest away from a stop; a dwarf carries it to `dest`.
    Stalled,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(default)]
pub(crate) struct Cart {
    pub item: i32,
    pub route: i32,
    pub state: CartState,
    pub stop: u32,
    pub dest: u32,
    pub tile: u32,
    /// Position inside the tile along the direction of travel.
    pub prog: i32,
    pub dir: u8,
    pub v: i32,
    pub arrived: u64,
    pub rider: u32,
    pub derailed: bool,
}

impl Default for Cart {
    fn default() -> Self {
        Cart { item: -1, route: -1, state: CartState::Idle, stop: 0, dest: 0, tile: 0, prog: 0, dir: DE, v: 0, arrived: 0, rider: NONE, derailed: false }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum JobK {
    #[default]
    Load,
    Unload,
    Push,
    Ride,
    Guide,
    /// Carry the cart to stop `stop` (first placement or after stalling).
    Fetch,
    /// Haul `item` by wheelbarrow `cart` to pile tile `dest`.
    Barrow,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(default)]
pub(crate) struct CJob {
    pub id: u32,
    pub kind: JobK,
    pub cart: i32,
    pub item: i32,
    pub dest: u32,
    pub stop: u32,
    pub claimed: u32,
    pub phase: u8,
    pub blocked_until: u64,
}

impl Default for CJob {
    fn default() -> Self {
        CJob { id: 0, kind: JobK::Load, cart: -1, item: -1, dest: NONE, stop: 0, claimed: NONE, phase: 0, blocked_until: 0 }
    }
}

/// A track stop or roller on a tile.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub(crate) struct TrackBld {
    pub tile: u32,
    pub bid: i32,
    pub roller: bool,
    /// Track stop friction or roller speed.
    pub value: i32,
    /// Dump direction (track stop) or push direction (roller), 0 = none.
    pub dir: u8,
}

/// Counters for reports and tests.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub struct CartStats {
    pub carts: usize,
    pub trips: u64,
    pub loaded: u64,
    pub unloaded: u64,
    pub dumped: u64,
    pub derailed: u64,
    pub stalled: u64,
    pub pushes: u64,
    pub rides: u64,
    pub guided: u64,
    pub barrow_hauls: u64,
    /// Tiles rolled by carts.
    pub tiles: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Carts {
    pub carts: Vec<Cart>,
    pub jobs: Vec<CJob>,
    pub next_job: u32,
    /// Units slowed by carried weight: (slot, step time in percent).
    pub slow: Vec<(u32, u16)>,
    /// Wheelbarrows assigned to stockpiles: (item id, pile building id).
    pub barrows: Vec<(i32, i32)>,
    /// Track stops and rollers by tile (sorted).
    pub blds: Vec<TrackBld>,
    pub bld_sig: u64,
    pub moving: u32,
    pub stats: CartStats,
    /// Finished trips per route id.
    pub route_trips: Vec<(i32, u64)>,
    /// Track bits per tiletype (pure cache of the tiletype table).
    #[serde(skip)]
    tt_bits: Vec<u8>,
}

impl Sim {
    // ------------------------------------------------------------------ public API

    /// [carts12] Minecart and wheelbarrow counters.
    pub fn carts_report(&self) -> CartStats {
        let mut s = self.carts.stats;
        s.carts = self.carts.carts.len();
        s
    }

    /// [carts12] A route vehicle: position, state text, speed and number of items inside.
    pub fn vehicle_info(&self, item: i32) -> Option<(Pos, String, i32, usize)> {
        let c = self.carts.carts.iter().find(|c| c.item == item)?;
        Some((self.world.pos_of(c.tile as usize), self.cart_state_text(c), c.v, self.contents.get(&item).map(|v| v.len()).unwrap_or(0)))
    }

    /// [carts12] Make a vehicle item (minecart or wheelbarrow) of material `mat` at `pos`
    /// (debug/demo helper; normally made at a carpenter's workshop or forge).
    pub fn spawn_vehicle(&mut self, wheelbarrow: bool, pos: Pos, mat: u16) -> i32 {
        let Some(t) = self.world.idx(pos) else { return -1 };
        let mname = self.world.materials.get(mat as usize).map(|m| m.name.clone()).unwrap_or_default();
        let (sub, what) = if wheelbarrow { ("ITEM_TOOL_WHEELBARROW", "wheelbarrow") } else { ("ITEM_TOOL_MINECART", "minecart") };
        let id = self.create_item("TOOL", mat, t, format!("{mname} {what}").trim().to_string());
        if let Some(&i) = self.item_idx.get(&id) {
            self.world.items[i as usize].subtype = sub.into();
        }
        id
    }

    pub(crate) fn route_status_impl(&self, route: i32) -> crate::RouteStatus {
        let mut st = crate::RouteStatus { id: route, ..Default::default() };
        for c in self.carts.carts.iter().filter(|c| c.route == route) {
            st.vehicles.push((c.item, self.cart_state_text(c)));
            st.loaded.push(self.contents.get(&c.item).map(|v| v.len()).unwrap_or(0));
        }
        st.trips = self.carts.trip_count(route);
        st
    }

    fn cart_state_text(&self, c: &Cart) -> String {
        let stop_name = |k: u32| {
            self.world.haul_routes.iter().find(|r| r.id == c.route).and_then(|r| r.stops.get(k as usize)).map(|s| s.name.clone()).unwrap_or_default()
        };
        let p = self.world.pos_of(c.tile as usize);
        match c.state {
            CartState::Idle => "waiting for a dwarf to bring it to the route".into(),
            CartState::AtStop => format!("stop {}: {}", c.stop + 1, stop_name(c.stop)),
            CartState::Moving => format!("moving ({},{},{}) speed {} to stop {}", p.x, p.y, p.z, c.v, c.dest + 1),
            CartState::Carried => format!("guided/carried to stop {}", c.dest + 1),
            CartState::Stalled => format!("{} at ({},{},{}), waiting for retrieval", if c.derailed { "derailed" } else { "stopped" }, p.x, p.y, p.z),
        }
    }

    // ------------------------------------------------------------------ helpers

    fn carts_tt_bits(&mut self) {
        if self.carts.tt_bits.len() == self.world.tiletypes.infos.len() {
            return;
        }
        let mut v = vec![0u8; self.world.tiletypes.infos.len()];
        for i in &self.world.tiletypes.infos {
            if i.special == TtSpecial::Track {
                if let Some(x) = v.get_mut(i.id as usize) {
                    *x = dir_bits(&i.direction);
                }
            }
        }
        self.carts.tt_bits = v;
    }

    /// Track direction bits of a tile (0 = no track).
    pub(crate) fn track_bits(&self, t: usize) -> u8 {
        let tt = self.world.tiles[t].tt as usize;
        match self.carts.tt_bits.get(tt) {
            Some(&b) => b,
            None => {
                let i = self.world.tiletypes.get(self.world.tiles[t].tt);
                if i.special == TtSpecial::Track {
                    dir_bits(&i.direction)
                } else {
                    0
                }
            }
        }
    }

    fn cart_shape_at(&self, p: Pos) -> Option<Shape> {
        self.world.idx(p).map(|i| self.world.tiletypes.get(self.world.tiles[i].tt).shape)
    }

    fn open_shape(s: Shape) -> bool {
        matches!(s, Shape::Empty | Shape::RampTop | Shape::None)
    }

    fn solid_shape(s: Shape) -> bool {
        matches!(s, Shape::Wall | Shape::Fortification | Shape::TrunkBranch)
    }

    /// Is `d` the uphill direction of the ramp at `p` (wall beside it with a floor above)?
    fn ramp_up(&self, p: Pos, d: u8) -> bool {
        let (dx, dy) = dvec(d);
        let side = p.offset(dx, dy, 0);
        matches!(self.cart_shape_at(side), Some(s) if Sim::solid_shape(s)) && matches!(self.cart_shape_at(side.offset(0, 0, 1)), Some(s) if !Sim::solid_shape(s) && !Sim::open_shape(s))
    }

    fn item_weight(&self, idx: usize) -> i32 {
        let it = &self.world.items[idx];
        let dens = self.world.materials.get(it.mat as usize).map(|m| m.density).filter(|&d| d > 0).unwrap_or(match it.kind.as_str() {
            "WOOD" => 600,
            "BOULDER" | "BLOCKS" => 2_600,
            "BAR" => 7_850,
            _ => 1_000,
        });
        ((dens as i64 * item_volume(&it.kind, &it.subtype, it.stack) as i64) / 1_000_000) as i32
    }

    /// Weight of an item and everything inside it.
    fn load_weight(&self, id: i32) -> i32 {
        let mut w = self.item_idx.get(&id).map(|&i| self.item_weight(i as usize)).unwrap_or(0);
        if let Some(c) = self.contents.get(&id) {
            for x in c {
                if let Some(&i) = self.item_idx.get(x) {
                    w += self.item_weight(i as usize);
                }
            }
        }
        w
    }

    fn cart_volume(&self, cart: i32) -> i32 {
        self.contents
            .get(&cart)
            .map(|c| c.iter().filter_map(|x| self.item_idx.get(x)).map(|&i| { let it = &self.world.items[i as usize]; item_volume(&it.kind, &it.subtype, it.stack) }).sum())
            .unwrap_or(0)
    }

    /// Step time percentage for a carried weight (100 = unhindered).
    pub fn carry_pct(weight: i32) -> u16 {
        if weight <= CARRY_FREE {
            100
        } else {
            (100 + (weight - CARRY_FREE) * 100 / CARRY_SCALE).min(CARRY_MAX_PCT) as u16
        }
    }

    fn set_slow(&mut self, s: usize, weight: i32) {
        self.carts.slow.retain(|x| x.0 != s as u32);
        let pct = Sim::carry_pct(weight);
        if pct > 100 {
            self.carts.slow.push((s as u32, pct));
        }
    }

    pub(crate) fn carts_clear_slow(&mut self, s: usize) {
        self.carts.slow.retain(|x| x.0 != s as u32);
    }

    /// [move hook] Walking time per 100 tiles of slot `s` with its carried weight.
    pub(crate) fn carts_speed(&self, s: usize) -> i32 {
        let sp = self.ai[s].speed;
        if self.ai[s].carrying < 0 {
            return sp;
        }
        match self.carts.slow.iter().find(|x| x.0 == s as u32) {
            Some(&(_, pct)) => sp * pct as i32 / 100,
            None => sp,
        }
    }

    /// [haul hook] A dwarf picked up `id` for a stockpile: heavy items slow him down.
    pub(crate) fn carts_carry_start(&mut self, s: usize, id: i32) {
        if let Some(&i) = self.item_idx.get(&id) {
            let w = self.item_weight(i as usize);
            if w > CARRY_FREE {
                self.set_slow(s, w);
            }
        }
    }

    fn hold(&mut self, s: usize, id: i32) {
        let Some(&idx) = self.item_idx.get(&id) else { return };
        let uid = self.ai[s].id;
        let it = &mut self.world.items[idx as usize];
        it.flags.on_ground = false;
        it.flags.in_inventory = true;
        it.holder = uid;
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.push(id);
        self.ai[s].carrying = id;
        let p = self.world.pos_of(self.ai[s].pos as usize);
        self.set_item_pos(id, p);
    }

    /// Put a held vehicle down on `tile` (vehicles never count as pile occupancy).
    fn put_down(&mut self, s: usize, id: i32, tile: u32) {
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != id);
        if self.ai[s].carrying == id {
            self.ai[s].carrying = -1;
        }
        let Some(&idx) = self.item_idx.get(&id) else { return };
        let it = &mut self.world.items[idx as usize];
        it.flags.on_ground = true;
        it.flags.in_inventory = false;
        it.holder = -1;
        let p = self.world.pos_of(tile as usize);
        self.set_item_pos(id, p);
        self.carts_clear_slow(s);
    }

    fn put_in(&mut self, cont: i32, id: i32) {
        let (Some(&ci), Some(&idx)) = (self.item_idx.get(&cont), self.item_idx.get(&id)) else { return };
        let p = self.world.items[ci as usize].pos;
        let it = &mut self.world.items[idx as usize];
        it.container = cont;
        it.holder = -1;
        it.flags.in_container = true;
        it.flags.on_ground = false;
        it.flags.in_inventory = false;
        it.pos = p;
        self.imeta[idx as usize].hauling = true;
        self.contents.entry(cont).or_default().push(id);
    }

    fn take_out(&mut self, cont: i32, id: i32) {
        if let Some(v) = self.contents.get_mut(&cont) {
            v.retain(|&x| x != id);
            if v.is_empty() {
                self.contents.remove(&cont);
            }
        }
        if let Some(&idx) = self.item_idx.get(&id) {
            let it = &mut self.world.items[idx as usize];
            it.container = -1;
            it.flags.in_container = false;
            self.imeta[idx as usize].hauling = false;
        }
    }

    /// Item out of a container, loose on `tile` (dumped items, released loads).
    fn place_loose(&mut self, id: i32, tile: u32) {
        let Some(&idx) = self.item_idx.get(&id) else { return };
        let it = &mut self.world.items[idx as usize];
        it.container = -1;
        it.holder = -1;
        it.flags.in_container = false;
        it.flags.in_inventory = false;
        it.flags.on_ground = true;
        self.imeta[idx as usize].hauling = false;
        let p = self.world.pos_of(tile as usize);
        self.set_item_pos(id, p);
        self.pile_occ_add(tile, 1);
        if !self.is_stored(idx as usize) {
            self.haul_add(id);
        }
    }

    /// Unit slot holds the item: it goes back to the ground at the unit's tile.
    fn drop_here(&mut self, s: usize, id: i32) {
        let here = self.ai[s].pos;
        self.carts_clear_slow(s);
        self.drop_item(s, id, here);
    }

    fn reserve_vehicle(&mut self, id: i32, on: bool) {
        let Some(&idx) = self.item_idx.get(&id) else { return };
        let (pos, ground, was) = {
            let it = &self.world.items[idx as usize];
            (it.pos, it.flags.on_ground && it.holder < 0 && it.container < 0, it.flags.in_job)
        };
        if was == on {
            return;
        }
        self.world.items[idx as usize].flags.in_job = on;
        let tile = self.world.idx(pos).map(|t| t as u32);
        if on {
            self.haul_remove(id);
            if let (Some(t), true) = (tile, ground) {
                self.pile_occ_add(t, -1);
            }
        } else if ground {
            if let Some(t) = tile {
                self.pile_occ_add(t, 1);
            }
            if !self.is_stored(idx as usize) && self.haulable(idx as usize) {
                self.haul_add(id);
            }
        }
    }

    fn pile_index_of(&self, bid: i32) -> Option<u32> {
        let b = self.world.building(bid)?;
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                if let Some(t) = self.world.idx(Pos::new(x, y, b.z)) {
                    if let Some(&(p, _)) = self.pile_at.get(&(t as u32)) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    fn route_stop(&self, route: i32, k: u32) -> Option<&rf_core::HaulStop> {
        self.world.haul_routes.iter().find(|r| r.id == route).and_then(|r| r.stops.get(k as usize))
    }

    fn stop_tile(&self, route: i32, k: u32) -> Option<u32> {
        self.route_stop(route, k).and_then(|s| self.world.idx(s.pos)).map(|t| t as u32)
    }

    fn stop_count(&self, route: i32) -> u32 {
        self.world.haul_routes.iter().find(|r| r.id == route).map(|r| r.stops.len() as u32).unwrap_or(0)
    }

    fn cart_idx(&self, item: i32) -> Option<usize> {
        self.carts.carts.iter().position(|c| c.item == item)
    }

    fn new_job(&mut self, j: CJob) -> u32 {
        self.carts.next_job = self.carts.next_job.wrapping_add(1).max(1);
        let id = self.carts.next_job;
        self.carts.jobs.push(CJob { id, ..j });
        id
    }

    fn cart_remove_job(&mut self, id: u32) {
        if let Some(k) = self.carts.jobs.iter().position(|j| j.id == id) {
            let j = self.carts.jobs.remove(k);
            if j.kind == JobK::Load && j.item >= 0 {
                self.unreserve(j.item);
            }
            if j.kind == JobK::Unload && j.dest != NONE && j.phase == 0 {
                self.release_pile_tile(j.dest);
            }
        }
    }

    fn walk_goals(&self, t: u32) -> Goals {
        if self.nav.walkable(t as usize) {
            return Goals::one(t);
        }
        let mut g = Goals::default();
        let p = self.world.pos_of(t as usize);
        for (dx, dy) in [(0, -1), (0, 1), (1, 0), (-1, 0), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
            if let Some(q) = self.world.idx(p.offset(dx, dy, 0)) {
                if self.nav.walkable(q) {
                    g.push(q as u32);
                }
            }
        }
        g
    }

    // ------------------------------------------------------------------ tick

    /// [carts12] Every tick: plan route work, move rolling carts; wheelbarrows every 400 ticks.
    pub(crate) fn carts_tick(&mut self, now: u64) {
        if now % BARROW_PERIOD == BARROW_PHASE {
            self.barrows_pass();
        }
        if now % PLAN_PERIOD == PLAN_PHASE && !self.carts.slow.is_empty() {
            // Drop weight entries of units that left their haul some other way.
            let ai = &self.ai;
            self.carts.slow.retain(|&(s, _)| ai.get(s as usize).map(|a| a.carrying >= 0 && matches!(a.task, Task::GoStore(..) | Task::Cart(_))).unwrap_or(false));
        }
        if self.carts.carts.is_empty() && self.world.haul_routes.is_empty() {
            return;
        }
        if now % PLAN_PERIOD == PLAN_PHASE {
            self.carts_plan(now);
        }
        if self.carts.moving > 0 {
            self.carts_tt_bits();
            for k in 0..self.carts.carts.len() {
                if self.carts.carts[k].state == CartState::Moving {
                    self.cart_physics(k, now);
                }
            }
        }
    }

    fn carts_bld_rebuild(&mut self) {
        let mut h = 0xCA27u64;
        for b in &self.world.buildings {
            if b.built && (b.kind == "TrackStop" || b.kind == "Rollers") {
                h = crate::rng::splitmix(h ^ (b.id as u64) << 8 ^ (b.kind.len() as u64) << 40 ^ (b.subtype.len() as u64) << 44 ^ (b.direction.len() as u64) << 48);
            }
        }
        if h == self.carts.bld_sig && !self.carts.blds.is_empty() || h == 0xCA27 && self.carts.blds.is_empty() {
            self.carts.bld_sig = h;
            return;
        }
        self.carts.bld_sig = h;
        let mut v = Vec::new();
        for b in &self.world.buildings {
            if !b.built || (b.kind != "TrackStop" && b.kind != "Rollers") {
                continue;
            }
            let roller = b.kind == "Rollers";
            let (value, dir) = if roller { (roller_speed(&b.subtype), parse_dir(&b.direction)) } else { (stop_friction(&b.subtype), parse_dir(&b.direction)) };
            for y in b.y1..=b.y2 {
                for x in b.x1..=b.x2 {
                    if let Some(t) = self.world.idx(Pos::new(x, y, b.z)) {
                        v.push(TrackBld { tile: t as u32, bid: b.id, roller, value, dir });
                    }
                }
            }
        }
        v.sort_by_key(|x| (x.tile, x.bid));
        self.carts.blds = v;
    }

    fn bld_at(&self, t: u32) -> Option<TrackBld> {
        if self.carts.blds.is_empty() {
            return None;
        }
        let k = self.carts.blds.partition_point(|b| b.tile < t);
        self.carts.blds.get(k).filter(|b| b.tile == t).copied()
    }

    /// Bring route vehicles in line with the routes and create the work at the stops.
    fn carts_plan(&mut self, now: u64) {
        self.carts_tt_bits();
        self.carts_bld_rebuild();
        // 1. Vehicles of the routes.
        let mut want: Vec<(i32, i32)> = Vec::new();
        for r in &self.world.haul_routes {
            for &v in &r.vehicles {
                if !want.iter().any(|w| w.1 == v) {
                    want.push((r.id, v));
                }
            }
        }
        let mut k = 0;
        while k < self.carts.carts.len() {
            let c = self.carts.carts[k];
            let alive = self.item_idx.get(&c.item).map(|&i| !self.imeta[i as usize].dead).unwrap_or(false);
            let keep = alive && want.iter().any(|w| *w == (c.route, c.item));
            if keep || c.state == CartState::Carried && alive && want.iter().any(|w| w.1 == c.item) {
                k += 1;
                continue;
            }
            self.release_cart(k);
        }
        for (route, v) in want {
            if self.cart_idx(v).is_some() {
                continue;
            }
            let Some(&idx) = self.item_idx.get(&v) else { continue };
            let it = &self.world.items[idx as usize];
            if self.imeta[idx as usize].dead || !is_minecart(&it.kind, &it.subtype) || it.holder >= 0 || it.container >= 0 {
                continue;
            }
            let tile = self.world.idx(it.pos).unwrap_or(0) as u32;
            self.reserve_vehicle(v, true);
            self.carts.carts.push(Cart { item: v, route, tile, ..Default::default() });
        }
        // Unclaimed jobs that no longer fit (item gone, cart gone or not at its stop).
        let dead = |sim: &Sim, id: i32| sim.item_idx.get(&id).map(|&i| sim.imeta[i as usize].dead).unwrap_or(true);
        let stale: Vec<u32> = self
            .carts
            .jobs
            .iter()
            .filter(|j| j.claimed == NONE && j.kind != JobK::Barrow)
            .filter(|j| {
                let cart = self.carts.carts.iter().find(|c| c.item == j.cart);
                let at_stop = cart.map(|c| c.state == CartState::AtStop).unwrap_or(false);
                cart.is_none() || dead(self, j.cart) || (matches!(j.kind, JobK::Load | JobK::Unload) && (!at_stop || dead(self, j.item)))
            })
            .map(|j| j.id)
            .collect();
        for id in stale {
            self.cart_remove_job(id);
        }
        // 2. Work at the stops.
        let mut load_piles: Vec<u32> = Vec::new();
        for k in 0..self.carts.carts.len() {
            let c = self.carts.carts[k];
            let n = self.stop_count(c.route);
            if n == 0 {
                continue;
            }
            match c.state {
                CartState::Idle | CartState::Stalled => {
                    let target = if c.state == CartState::Idle { 0 } else { c.dest.min(n - 1) };
                    let Some(st) = self.stop_tile(c.route, target) else { continue };
                    let on_ground = self.item_idx.get(&c.item).map(|&i| self.world.items[i as usize].flags.on_ground).unwrap_or(false);
                    if on_ground && c.tile == st {
                        self.arrive_stop(k, target, now);
                    } else if !self.carts.jobs.iter().any(|j| j.cart == c.item) {
                        self.new_job(CJob { kind: JobK::Fetch, cart: c.item, dest: st, stop: target, ..Default::default() });
                    }
                }
                CartState::AtStop => {
                    let Some(stop) = self.route_stop(c.route, c.stop).cloned() else { continue };
                    let n_items = self.contents.get(&c.item).map(|v| v.len()).unwrap_or(0);
                    let jobs_here = self.carts.jobs.iter().filter(|j| j.cart == c.item).count();
                    if !stop.give_to.is_empty() && n_items > 0 {
                        self.plan_unload(k, &stop, jobs_here);
                        continue;
                    }
                    let cond = parse_conditions(&stop.conditions);
                    if !stop.take_from.is_empty() && !cond.empty {
                        for &b in &stop.take_from {
                            if let Some(p) = self.pile_index_of(b) {
                                if !load_piles.contains(&p) {
                                    load_piles.push(p);
                                }
                            }
                        }
                    }
                }
                CartState::Moving | CartState::Carried => {}
            }
        }
        let cands = if load_piles.is_empty() { Vec::new() } else { self.load_candidates(&load_piles) };
        for k in 0..self.carts.carts.len() {
            let c = self.carts.carts[k];
            if c.state != CartState::AtStop {
                continue;
            }
            let Some(stop) = self.route_stop(c.route, c.stop).cloned() else { continue };
            let n_items = self.contents.get(&c.item).map(|v| v.len()).unwrap_or(0);
            if !stop.give_to.is_empty() && n_items > 0 {
                continue;
            }
            let cond = parse_conditions(&stop.conditions);
            let (mut more, mut waiting) = (false, false);
            if !stop.take_from.is_empty() && !cond.empty {
                (more, waiting) = self.plan_load(k, &stop, &cands);
            }
            let busy = self.carts.jobs.iter().any(|j| j.cart == c.item);
            if busy {
                continue;
            }
            if self.depart_ok(k, &cond, more, waiting, now) {
                let kind = match cond.mode {
                    DepartMode::Push => JobK::Push,
                    DepartMode::Ride => JobK::Ride,
                    DepartMode::Guide => JobK::Guide,
                };
                let next = (c.stop + 1) % self.stop_count(c.route).max(1);
                let dest = self.stop_tile(c.route, next).unwrap_or(NONE);
                self.new_job(CJob { kind, cart: c.item, dest, stop: next, ..Default::default() });
            }
        }
    }

    fn release_cart(&mut self, k: usize) {
        let c = self.carts.carts.remove(k);
        let ids: Vec<u32> = self.carts.jobs.iter().filter(|j| j.cart == c.item).map(|j| j.id).collect();
        for id in ids {
            if let Some(j) = self.carts.jobs.iter().find(|j| j.id == id).copied() {
                if j.claimed != NONE {
                    let s = j.claimed as usize;
                    self.cancel_task(s);
                }
            }
            self.cart_remove_job(id);
        }
        if c.rider != NONE {
            self.dismount(c.rider as usize, self.ticks);
        }
        if c.state == CartState::Moving {
            self.carts.moving = self.carts.moving.saturating_sub(1);
        }
        self.reserve_vehicle(c.item, false);
    }

    /// Unload jobs: items in the cart to free tiles of the linked piles.
    fn plan_unload(&mut self, k: usize, stop: &rf_core::HaulStop, jobs_here: usize) {
        let c = self.carts.carts[k];
        if jobs_here >= MAX_JOBS_PER_CART {
            return;
        }
        let items: Vec<i32> = self.contents.get(&c.item).cloned().unwrap_or_default();
        let mut room = MAX_JOBS_PER_CART - jobs_here;
        let piles: Vec<u32> = stop.give_to.iter().filter_map(|&b| self.pile_index_of(b)).collect();
        for id in items {
            if room == 0 {
                break;
            }
            if self.carts.jobs.iter().any(|j| j.item == id) {
                continue;
            }
            let Some(&idx) = self.item_idx.get(&id) else { continue };
            let cat = self.imeta[idx as usize].cat;
            // A free tile of a linked pile that takes the item.
            let near = self.nav.xyz(c.tile as usize);
            let mut best = (i32::MAX, NONE);
            for &p in &piles {
                let pile = &self.piles[p as usize];
                if pile.cats & cat.bit() == 0 || pile.free == 0 {
                    continue;
                }
                for (q, &t) in pile.tiles.iter().enumerate() {
                    if pile.occ[q] == 0 && !pile.res[q] {
                        let d = crate::manhattan(near, self.nav.xyz(t as usize));
                        if d < best.0 {
                            best = (d, t);
                        }
                    }
                }
            }
            if best.1 == NONE {
                continue;
            }
            if let Some(&(p, q)) = self.pile_at.get(&best.1) {
                let pile = &mut self.piles[p as usize];
                pile.res[q as usize] = true;
                pile.free -= 1;
            }
            self.new_job(CJob { kind: JobK::Unload, cart: c.item, item: id, dest: best.1, ..Default::default() });
            room -= 1;
        }
    }

    /// Items lying loose on the given piles (sorted by item index: deterministic).
    fn load_candidates(&self, piles: &[u32]) -> Vec<(u32, i32)> {
        let mut v = Vec::new();
        for (idx, it) in self.world.items.iter().enumerate() {
            let m = &self.imeta[idx];
            let f = &it.flags;
            if m.dead || m.hauling || m.reserved > 0 || !f.on_ground || f.forbid || f.in_job || f.construction || f.in_building || it.container >= 0 || it.holder >= 0 || it.building >= 0 {
                continue;
            }
            if is_minecart(&it.kind, &it.subtype) || is_wheelbarrow(&it.kind, &it.subtype) {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if let Some(&(p, _)) = self.pile_at.get(&(t as u32)) {
                if piles.contains(&p) {
                    v.push((p, it.id));
                }
            }
        }
        v
    }

    /// Load jobs for cart `k`: (more items that fit are waiting, any items are waiting). Only
    /// items that a later stop's linked piles take are loaded (stand-in for DF's stop filter).
    fn plan_load(&mut self, k: usize, stop: &rf_core::HaulStop, cands: &[(u32, i32)]) -> (bool, bool) {
        let c = self.carts.carts[k];
        let piles: Vec<u32> = stop.take_from.iter().filter_map(|&b| self.pile_index_of(b)).collect();
        let mut accept = 0u32;
        if let Some(r) = self.world.haul_routes.iter().find(|r| r.id == c.route) {
            for (i, st) in r.stops.iter().enumerate() {
                if i as u32 != c.stop {
                    for &b in &st.give_to {
                        if let Some(p) = self.pile_index_of(b) {
                            accept |= self.piles[p as usize].cats;
                        }
                    }
                }
            }
        }
        if accept == 0 {
            accept = u32::MAX;
        }
        let mut vol = self.cart_volume(c.item);
        let mut jobs_here = 0;
        for j in self.carts.jobs.iter().filter(|j| j.cart == c.item && j.kind == JobK::Load) {
            jobs_here += 1;
            if let Some(&i) = self.item_idx.get(&j.item) {
                let it = &self.world.items[i as usize];
                vol += item_volume(&it.kind, &it.subtype, it.stack);
            }
        }
        let (mut more, mut waiting) = (false, false);
        for &(p, id) in cands {
            if !piles.contains(&p) {
                continue;
            }
            let Some(&idx) = self.item_idx.get(&id) else { continue };
            if self.imeta[idx as usize].reserved > 0 || accept & self.imeta[idx as usize].cat.bit() == 0 {
                continue;
            }
            waiting = true;
            let it = &self.world.items[idx as usize];
            let v = item_volume(&it.kind, &it.subtype, it.stack);
            if vol + v > CART_CAPACITY {
                continue;
            }
            more = true;
            if jobs_here >= MAX_JOBS_PER_CART {
                break;
            }
            vol += v;
            jobs_here += 1;
            self.reserve(id);
            self.new_job(CJob { kind: JobK::Load, cart: c.item, item: id, ..Default::default() });
        }
        (more, waiting)
    }

    fn depart_ok(&self, k: usize, cond: &Conditions, more: bool, waiting: bool, now: u64) -> bool {
        let c = self.carts.carts[k];
        if self.stop_count(c.route) < 2 && cond.mode != DepartMode::Push {
            return false;
        }
        if now < c.arrived + cond.wait {
            return false;
        }
        let n_items = self.contents.get(&c.item).map(|v| v.len()).unwrap_or(0);
        if cond.empty {
            return n_items == 0;
        }
        if let Some(pct) = cond.fill {
            let vol = self.cart_volume(c.item) as i64;
            // Also "full" when items wait but none of them fits any more.
            return vol * 100 >= pct as i64 * CART_CAPACITY as i64 || (waiting && !more && n_items > 0);
        }
        // No fill condition: leave when nothing more can be loaded here.
        !more && (n_items > 0 || cond.wait > 0 || self.route_stop(c.route, c.stop).map(|s| s.take_from.is_empty()).unwrap_or(true))
    }

    fn arrive_stop(&mut self, k: usize, stop: u32, now: u64) {
        let c = &mut self.carts.carts[k];
        if c.state == CartState::Moving {
            self.carts.moving = self.carts.moving.saturating_sub(1);
        }
        let counted = matches!(c.state, CartState::Moving | CartState::Carried | CartState::Stalled);
        c.state = CartState::AtStop;
        c.stop = stop;
        c.dest = stop;
        c.v = 0;
        c.prog = 0;
        c.arrived = now;
        c.derailed = false;
        let (route, rider) = (c.route, c.rider);
        c.rider = NONE;
        if counted {
            self.carts.stats.trips += 1;
            self.carts.add_trip(route);
        }
        if rider != NONE {
            self.dismount(rider as usize, now);
        }
    }

    // ------------------------------------------------------------------ physics

    fn cart_set_tile(&mut self, k: usize, t: u32) {
        let c = &mut self.carts.carts[k];
        c.tile = t;
        let (item, rider) = (c.item, c.rider);
        let p = self.world.pos_of(t as usize);
        self.set_item_pos(item, p);
        if rider != NONE {
            let s = rider as usize;
            self.ai[s].pos = t;
            let uidx = self.ai[s].uidx;
            self.world.units[uidx].pos = p;
        }
        self.carts.stats.tiles += 1;
    }

    /// Where a cart leaving tile `t` in direction `d` goes: Ok(next tile) or Err(fall-to tile,
    /// or NONE when a wall or the map edge stops it).
    fn cart_next(&self, t: u32, d: u8) -> Result<u32, u32> {
        let p = self.world.pos_of(t as usize);
        let (dx, dy) = dvec(d);
        let q = p.offset(dx, dy, 0);
        let Some(sq) = self.cart_shape_at(q) else { return Err(NONE) };
        if self.cart_shape_at(p) == Some(Shape::Ramp) && self.ramp_up(p, d) {
            let up = q.offset(0, 0, 1);
            return match self.world.idx(up) {
                Some(u) if !Sim::solid_shape(self.cart_shape_at(up).unwrap_or(Shape::Wall)) => Ok(u as u32),
                _ => Err(NONE),
            };
        }
        if Sim::solid_shape(sq) {
            return Err(NONE);
        }
        if Sim::open_shape(sq) {
            let below = q.offset(0, 0, -1);
            if self.cart_shape_at(below) == Some(Shape::Ramp) {
                return Ok(self.world.idx(below).unwrap() as u32);
            }
            // Falls down to the first tile with a floor.
            let mut z = q;
            loop {
                let b = z.offset(0, 0, -1);
                match self.cart_shape_at(b) {
                    Some(s) if Sim::open_shape(s) => z = b,
                    Some(s) if Sim::solid_shape(s) => break,
                    Some(_) => {
                        z = b;
                        break;
                    }
                    None => break,
                }
            }
            return Err(self.world.idx(z).map(|i| i as u32).unwrap_or(NONE));
        }
        Ok(self.world.idx(q).unwrap() as u32)
    }

    fn tile_len(&self, t: u32) -> i32 {
        if self.world.tiletypes.get(self.world.tiles[t as usize].tt).shape == Shape::Ramp {
            RAMP_LEN
        } else {
            TILE_LEN
        }
    }

    /// Friction of the cart's tile per tick (negative: accelerates down a ramp).
    fn friction(&self, t: u32, d: u8) -> i32 {
        if let Some(b) = self.bld_at(t).filter(|b| !b.roller) {
            return b.value.max(FRICTION_TRACK);
        }
        let on_track = self.track_bits(t as usize) != 0;
        let p = self.world.pos_of(t as usize);
        if self.cart_shape_at(p) == Some(Shape::Ramp) && on_track {
            if self.ramp_up(p, d) {
                return FRICTION_RAMP_UP;
            }
            if self.ramp_up(p, opposite(d)) {
                return FRICTION_RAMP_DOWN;
            }
        }
        if on_track {
            FRICTION_TRACK
        } else {
            FRICTION_FLOOR
        }
    }

    /// Entering tile `t`: follow the track (corners, derailing), dump at track stops.
    fn cart_enter(&mut self, k: usize, t: u32, now: u64) {
        self.cart_set_tile(k, t);
        let c = self.carts.carts[k];
        let bits = self.track_bits(t as usize);
        if bits != 0 && bits & c.dir == 0 {
            let other = bits & !opposite(c.dir);
            if other.count_ones() == 1 {
                let straight_blocked = matches!(self.cart_next(t, c.dir), Err(NONE));
                if c.v > DERAIL_SPEED && !straight_blocked {
                    self.carts.carts[k].derailed = true;
                    self.carts.stats.derailed += 1;
                } else {
                    let c = &mut self.carts.carts[k];
                    c.dir = other;
                    c.v = (c.v - CORNER_LOSS).max(0);
                }
            }
        }
        if let Some(b) = self.bld_at(t).filter(|b| !b.roller && b.dir != 0) {
            self.cart_dump(k, b.dir, now);
        }
    }

    fn cart_dump(&mut self, k: usize, dir: u8, _now: u64) {
        let c = self.carts.carts[k];
        let items: Vec<i32> = self.contents.get(&c.item).cloned().unwrap_or_default();
        if items.is_empty() {
            return;
        }
        let p = self.world.pos_of(c.tile as usize);
        let (dx, dy) = dvec(dir);
        let mut q = p.offset(dx, dy, 0);
        match self.cart_shape_at(q) {
            Some(s) if Sim::solid_shape(s) => q = p,
            None => q = p,
            _ => {}
        }
        // Items fall down to the first floor.
        while let Some(s) = self.cart_shape_at(q) {
            if !Sim::open_shape(s) {
                break;
            }
            let b = q.offset(0, 0, -1);
            match self.cart_shape_at(b) {
                Some(bs) if Sim::solid_shape(bs) => break,
                Some(_) => q = b,
                None => break,
            }
        }
        let Some(t) = self.world.idx(q) else { return };
        for id in items {
            self.take_out(c.item, id);
            self.place_loose(id, t as u32);
            self.carts.stats.dumped += 1;
        }
    }

    fn cart_physics(&mut self, k: usize, now: u64) {
        // Rollers give a slower cart their speed in their direction.
        let t0 = self.carts.carts[k].tile;
        if let Some(b) = self.bld_at(t0).filter(|b| b.roller && b.dir != 0) {
            if self.machine_powered(b.bid) {
                let c = &mut self.carts.carts[k];
                let along = if c.dir == b.dir { c.v } else if c.dir == opposite(b.dir) { -c.v } else { 0 };
                if along < b.value {
                    c.dir = b.dir;
                    c.v = b.value;
                }
            }
        }
        let c = &mut self.carts.carts[k];
        c.prog += c.v;
        let mut stop_here = false;
        for _ in 0..4 {
            let c = self.carts.carts[k];
            let len = self.tile_len(c.tile);
            if c.prog < len {
                break;
            }
            match self.cart_next(c.tile, c.dir) {
                Ok(t) => {
                    self.carts.carts[k].prog -= len;
                    self.cart_enter(k, t, now);
                }
                Err(fall) => {
                    let c = &mut self.carts.carts[k];
                    c.prog = len - 1;
                    c.v = 0;
                    if fall != NONE {
                        c.derailed = true;
                        self.cart_set_tile(k, fall);
                    }
                    stop_here = true;
                    break;
                }
            }
        }
        let c = self.carts.carts[k];
        if !stop_here {
            let f = self.friction(c.tile, c.dir);
            let c = &mut self.carts.carts[k];
            c.v = (c.v - f).clamp(0, MAX_SPEED);
        }
        if self.carts.carts[k].v == 0 {
            self.cart_stopped(k, now);
        }
    }

    fn cart_stopped(&mut self, k: usize, now: u64) {
        let c = self.carts.carts[k];
        let n = self.stop_count(c.route);
        // The stop it was heading for first, then any other stop of the route.
        let mut hit = None;
        for i in 0..n {
            let s = (c.dest + i) % n.max(1);
            if self.stop_tile(c.route, s) == Some(c.tile) {
                hit = Some(s);
                break;
            }
        }
        match hit {
            Some(s) => self.arrive_stop(k, s, now),
            None => {
                let rider = c.rider;
                let c = &mut self.carts.carts[k];
                c.state = CartState::Stalled;
                c.rider = NONE;
                self.carts.moving = self.carts.moving.saturating_sub(1);
                self.carts.stats.stalled += 1;
                if rider != NONE {
                    self.dismount(rider as usize, now);
                }
            }
        }
    }

    /// Start a cart from its stop in direction `d` with a push (wiki: to the middle of the
    /// next tile, 19 990 speed).
    fn launch(&mut self, k: usize, dest_stop: u32, now: u64) {
        let c = self.carts.carts[k];
        let d = self.depart_dir(&c, dest_stop);
        {
            let c = &mut self.carts.carts[k];
            c.state = CartState::Moving;
            c.dest = dest_stop;
            c.dir = d;
            c.v = PUSH_SPEED;
            c.prog = 0;
            c.derailed = false;
        }
        self.carts.moving += 1;
        self.carts.stats.pushes += 1;
        self.carts_tt_bits();
        match self.cart_next(c.tile, d) {
            Ok(t) => {
                self.cart_enter(k, t, now);
                self.carts.carts[k].prog = TILE_LEN / 2;
            }
            Err(_) => {
                self.carts.carts[k].v = 0;
                self.cart_stopped(k, now);
            }
        }
    }

    /// Push direction: the stop's condition, else the track direction that leads to the next
    /// stop (followed along the track), else towards it.
    fn depart_dir(&self, c: &Cart, dest_stop: u32) -> u8 {
        let cond = self.route_stop(c.route, c.stop).map(|s| parse_conditions(&s.conditions)).unwrap_or_default();
        if cond.dir != 0 {
            return cond.dir;
        }
        let bits = self.track_bits(c.tile as usize);
        let goal = self.stop_tile(c.route, dest_stop);
        for d in [DN, DS, DE, DW] {
            if bits & d == 0 {
                continue;
            }
            let (mut t, mut dir) = (c.tile, d);
            for _ in 0..4_000 {
                let Ok(nt) = self.cart_next(t, dir) else { break };
                t = nt;
                if Some(t) == goal {
                    return d;
                }
                let b = self.track_bits(t as usize);
                if b == 0 {
                    break;
                }
                if b & dir == 0 {
                    let o = b & !opposite(dir);
                    if o.count_ones() != 1 {
                        break;
                    }
                    dir = o;
                }
            }
        }
        if bits != 0 {
            return [DN, DS, DE, DW].into_iter().find(|&d| bits & d != 0).unwrap_or(DE);
        }
        let (a, b) = (self.world.pos_of(c.tile as usize), goal.map(|g| self.world.pos_of(g as usize)).unwrap_or(self.world.pos_of(c.tile as usize)));
        if (b.x - a.x).abs() >= (b.y - a.y).abs() {
            if b.x >= a.x {
                DE
            } else {
                DW
            }
        } else if b.y >= a.y {
            DS
        } else {
            DN
        }
    }

    fn dismount(&mut self, s: usize, now: u64) {
        if let Task::Cart(id) = self.ai[s].task {
            if let Some(j) = self.carts.jobs.iter().find(|j| j.id == id).copied() {
                if j.kind == JobK::Ride {
                    self.carts.jobs.retain(|x| x.id != id);
                    self.ai[s].task = Task::Idle;
                    self.ai[s].wake = now + 1;
                }
            }
        }
    }

    // ------------------------------------------------------------------ dwarves

    /// [decide hook] An idle dwarf takes route work: loading, unloading, pushing, riding,
    /// guiding or bringing a cart back.
    pub(crate) fn carts_try_job(&mut self, s: usize, now: u64) -> bool {
        if self.carts.jobs.is_empty() {
            return false;
        }
        let labors = self.ai[s].labors;
        if labors & (labor::ANY_HAUL | labor::HANDLE_VEHICLES) == 0 {
            return false;
        }
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return false;
        }
        for k in 0..self.carts.jobs.len() {
            let j = self.carts.jobs[k];
            if j.claimed != NONE || j.blocked_until > now || j.kind == JobK::Barrow {
                continue;
            }
            let need = match j.kind {
                JobK::Load | JobK::Unload => self.item_idx.get(&j.item).map(|&i| self.imeta[i as usize].cat.haul_labor()).unwrap_or(labor::HAUL_ITEM),
                _ => labor::HANDLE_VEHICLES,
            };
            if labors & need == 0 {
                continue;
            }
            let first = match j.kind {
                JobK::Load => j.item,
                _ => j.cart,
            };
            if !self.item_idx.contains_key(&first) {
                continue;
            }
            let goals = self.item_goals(first);
            if !goals.as_slice().iter().any(|&t| self.nav.comp_root(t as usize) == c && !self.alert_blocks(s, t as usize)) {
                continue;
            }
            self.carts.jobs[k].claimed = s as u32;
            self.carts.jobs[k].phase = 0;
            self.ai[s].task = Task::Cart(j.id);
            self.request_path(s, goals, now);
            return true;
        }
        false
    }

    pub(crate) fn cart_caption(&self, id: u32) -> (&'static str, Option<Pos>) {
        let Some(j) = self.carts.jobs.iter().find(|j| j.id == id) else { return ("No Job", None) };
        let at = |item: i32| self.item_idx.get(&item).map(|&i| self.world.items[i as usize].pos);
        match j.kind {
            JobK::Load => ("Load Item into Vehicle", at(j.cart)),
            JobK::Unload => ("Store Item in Stockpile", (j.dest != NONE).then(|| self.world.pos_of(j.dest as usize))),
            JobK::Push => ("Push Vehicle", at(j.cart)),
            JobK::Ride => ("Ride Vehicle", at(j.cart)),
            JobK::Guide => ("Guide Vehicle", (j.dest != NONE).then(|| self.world.pos_of(j.dest as usize))),
            JobK::Fetch => ("Store Vehicle", (j.dest != NONE).then(|| self.world.pos_of(j.dest as usize))),
            JobK::Barrow => ("Store Item in Stockpile", (j.dest != NONE).then(|| self.world.pos_of(j.dest as usize))),
        }
    }

    fn job_done(&mut self, s: usize, id: u32, now: u64) {
        self.carts.jobs.retain(|j| j.id != id);
        self.carts_clear_slow(s);
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    /// [arrive hook] A dwarf on route work reached the end of his path.
    pub(crate) fn carts_arrive(&mut self, s: usize, id: u32, now: u64) {
        let Some(k) = self.carts.jobs.iter().position(|j| j.id == id && j.claimed == s as u32) else {
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        };
        let j = self.carts.jobs[k];
        let here = self.ai[s].pos;
        let near = |sim: &Sim, item: i32| sim.item_reachable_here(s, item) || sim.item_idx.get(&item).map(|&i| sim.world.items[i as usize].holder == sim.ai[s].id).unwrap_or(false);
        let ci = self.cart_idx(j.cart);
        match (j.kind, j.phase) {
            (JobK::Load, 0) => {
                let ok = ci.map(|c| self.carts.carts[c].state == CartState::AtStop).unwrap_or(false);
                if !ok || !near(self, j.item) {
                    self.cart_remove_job(id);
                    self.ai[s].task = Task::Idle;
                    self.decide(s, now);
                    return;
                }
                self.unreserve(j.item);
                self.carts.jobs[k].item = j.item;
                self.haul_remove(j.item);
                self.pickup(s, j.item);
                self.reserve(j.item); // keeps it the job's item until it is in the cart
                self.carts.jobs[k].phase = 1;
                let g = self.item_goals(j.cart);
                self.request_path(s, g, now);
            }
            (JobK::Load, _) => {
                let ok = ci.map(|c| self.carts.carts[c].state == CartState::AtStop).unwrap_or(false) && near(self, j.cart);
                self.unreserve(j.item);
                self.carts.jobs[k].item = -1;
                if ok {
                    let uidx = self.ai[s].uidx;
                    self.world.units[uidx].inventory.retain(|&x| x != j.item);
                    self.ai[s].carrying = -1;
                    self.put_in(j.cart, j.item);
                    self.carts.stats.loaded += 1;
                    self.add_xp(s, "HANDLE_VEHICLES", crate::skills::XP_USE / 3);
                } else {
                    self.drop_here(s, j.item);
                }
                self.job_done(s, id, now);
            }
            (JobK::Unload, 0) => {
                let inside = self.contents.get(&j.cart).map(|v| v.contains(&j.item)).unwrap_or(false);
                if !inside || !near(self, j.cart) {
                    self.cart_remove_job(id);
                    self.ai[s].task = Task::Idle;
                    self.decide(s, now);
                    return;
                }
                self.take_out(j.cart, j.item);
                self.hold(s, j.item);
                self.carts.jobs[k].phase = 1;
                self.request_path(s, Goals::one(j.dest), now);
            }
            (JobK::Unload, _) => {
                self.drop_item(s, j.item, j.dest);
                self.carts.stats.unloaded += 1;
                self.stats.items_hauled += 1;
                self.job_done(s, id, now);
            }
            (JobK::Push | JobK::Ride, 0) => {
                let Some(c) = ci.filter(|&c| self.carts.carts[c].state == CartState::AtStop) else {
                    self.job_done(s, id, now);
                    return;
                };
                if !near(self, j.cart) {
                    self.cart_remove_job(id);
                    self.ai[s].task = Task::Idle;
                    self.decide(s, now);
                    return;
                }
                self.add_xp(s, "HANDLE_VEHICLES", crate::skills::XP_USE);
                if j.kind == JobK::Ride {
                    self.carts.stats.rides += 1;
                    self.carts.jobs[k].phase = 1;
                    self.carts.carts[c].rider = s as u32;
                    let t = self.carts.carts[c].tile;
                    self.ai[s].pos = t;
                    let uidx = self.ai[s].uidx;
                    self.world.units[uidx].pos = self.world.pos_of(t as usize);
                    self.ai[s].wake = u64::MAX;
                    self.launch(c, j.stop, now);
                } else {
                    self.launch(c, j.stop, now);
                    self.job_done(s, id, now);
                }
            }
            (JobK::Push, _) => self.job_done(s, id, now),
            (JobK::Ride, _) => {
                // Riding: the cart moves the rider; arrival dismounts.
                self.ai[s].wake = u64::MAX;
            }
            (JobK::Guide | JobK::Fetch, 0) => {
                let Some(c) = ci else {
                    self.job_done(s, id, now);
                    return;
                };
                let st = self.carts.carts[c].state;
                let ok = if j.kind == JobK::Guide { st == CartState::AtStop } else { matches!(st, CartState::Idle | CartState::Stalled) };
                if !ok || !near(self, j.cart) {
                    self.cart_remove_job(id);
                    self.ai[s].task = Task::Idle;
                    self.decide(s, now);
                    return;
                }
                if st == CartState::Moving {
                    self.carts.moving = self.carts.moving.saturating_sub(1);
                }
                {
                    let cart = &mut self.carts.carts[c];
                    cart.state = CartState::Carried;
                    cart.dest = j.stop;
                }
                self.hold(s, j.cart);
                let w = self.load_weight(j.cart);
                self.set_slow(s, w);
                if j.kind == JobK::Guide {
                    self.carts.stats.guided += 1;
                    self.add_xp(s, "HANDLE_VEHICLES", crate::skills::XP_USE);
                }
                self.carts.jobs[k].phase = 1;
                self.request_path(s, self.walk_goals(j.dest), now);
            }
            (JobK::Guide | JobK::Fetch, _) => {
                let Some(c) = ci else {
                    self.job_done(s, id, now);
                    return;
                };
                let at = if self.walk_goals(j.dest).as_slice().contains(&here) { j.dest } else { here };
                self.put_down(s, j.cart, at);
                self.carts.carts[c].tile = at;
                if at == j.dest {
                    self.arrive_stop(c, j.stop, now);
                } else {
                    self.carts.carts[c].state = CartState::Stalled;
                }
                self.job_done(s, id, now);
            }
            (JobK::Barrow, 0) => {
                if !near(self, j.cart) {
                    self.cancel_task(s);
                    self.ai[s].wake = now + 1;
                    return;
                }
                self.hold(s, j.cart);
                self.carts.jobs[k].phase = 1;
                let g = self.item_goals(j.item);
                self.request_path(s, g, now);
            }
            (JobK::Barrow, 1) => {
                if !self.item_reachable_here(s, j.item) {
                    self.cancel_task(s);
                    self.ai[s].wake = now + 1;
                    return;
                }
                // Load the item into the wheelbarrow (it leaves the ground).
                if let Some(t) = self.item_idx.get(&j.item).and_then(|&i| self.world.idx(self.world.items[i as usize].pos)) {
                    self.pile_occ_add(t as u32, -1);
                }
                self.put_in(j.cart, j.item);
                self.set_item_pos(j.cart, self.world.pos_of(here as usize));
                self.carts.jobs[k].phase = 2;
                self.request_path(s, Goals::one(j.dest), now);
            }
            (JobK::Barrow, _) => {
                self.take_out(j.cart, j.item);
                self.hold(s, j.item);
                self.drop_item(s, j.item, j.dest);
                self.society_stored(j.item, j.dest);
                self.put_down(s, j.cart, j.dest);
                self.carts.stats.barrow_hauls += 1;
                self.stats.items_hauled += 1;
                self.job_done(s, id, now);
            }
        }
    }

    /// [cancel hook] The dwarf gave up route work.
    pub(crate) fn carts_cancel(&mut self, s: usize, id: u32) {
        self.carts_clear_slow(s);
        let Some(k) = self.carts.jobs.iter().position(|j| j.id == id && j.claimed == s as u32) else { return };
        let j = self.carts.jobs[k];
        let now = self.ticks;
        let here = self.ai[s].pos;
        match (j.kind, j.phase) {
            (JobK::Load, 0) | (JobK::Unload, 0) | (JobK::Push, 0) | (JobK::Ride, 0) | (JobK::Guide, 0) | (JobK::Fetch, 0) => {
                self.carts.jobs[k].claimed = NONE;
                self.carts.jobs[k].blocked_until = now + 600;
            }
            (JobK::Load, _) => {
                self.unreserve(j.item);
                self.carts.jobs[k].item = -1;
                self.drop_here(s, j.item);
                self.carts.jobs.remove(k);
            }
            (JobK::Unload, _) => {
                self.drop_here(s, j.item);
                self.release_pile_tile(j.dest);
                self.carts.jobs.remove(k);
            }
            (JobK::Push, _) => {
                self.carts.jobs.remove(k);
            }
            (JobK::Ride, _) => {
                if let Some(c) = self.cart_idx(j.cart) {
                    self.carts.carts[c].rider = NONE;
                }
                self.carts.jobs.remove(k);
                self.ai[s].wake = now + 1; // riders sleep with wake u64::MAX
            }
            (JobK::Guide | JobK::Fetch, _) => {
                self.put_down(s, j.cart, here);
                if let Some(c) = self.cart_idx(j.cart) {
                    let cart = &mut self.carts.carts[c];
                    cart.tile = here;
                    cart.state = CartState::Stalled;
                    cart.dest = j.stop;
                }
                self.carts.jobs.remove(k);
            }
            (JobK::Barrow, ph) => {
                if ph >= 2 {
                    self.take_out(j.cart, j.item);
                    self.place_loose(j.item, here);
                } else {
                    self.store_release(j.item);
                    self.set_hauling(j.item, false);
                    self.haul_add(j.item);
                }
                if ph >= 1 {
                    self.put_down(s, j.cart, here);
                }
                self.release_pile_tile(j.dest);
                self.carts.jobs.remove(k);
            }
        }
    }

    // ------------------------------------------------------------------ wheelbarrows

    /// Assign free wheelbarrows to stockpiles with `max_wheelbarrows` > 0 (and release those of
    /// piles that no longer want them).
    fn barrows_pass(&mut self) {
        let mut want: Vec<(i32, i32)> = Vec::new();
        for b in &self.world.buildings {
            if let Some(sp) = b.stockpile.as_ref().filter(|_| b.kind == "Stockpile") {
                if sp.max_wheelbarrows > 0 {
                    want.push((b.id, sp.max_wheelbarrows));
                }
            }
        }
        if want.is_empty() && self.carts.barrows.is_empty() {
            return;
        }
        // Release barrows of piles gone or over their maximum (not while in use).
        let mut keep = Vec::new();
        let mut drop = Vec::new();
        for &(item, pile) in &self.carts.barrows {
            let alive = self.item_idx.get(&item).map(|&i| !self.imeta[i as usize].dead).unwrap_or(false);
            let max = want.iter().find(|w| w.0 == pile).map(|w| w.1).unwrap_or(0);
            let used = self.carts.jobs.iter().any(|j| j.kind == JobK::Barrow && j.cart == item);
            let n = keep.iter().filter(|x: &&(i32, i32)| x.1 == pile).count() as i32;
            if alive && (used || n < max) {
                keep.push((item, pile));
            } else if alive {
                drop.push(item);
            }
        }
        self.carts.barrows = keep;
        for item in drop {
            self.reserve_vehicle(item, false);
        }
        for (pile, max) in want {
            let mut n = self.carts.barrows.iter().filter(|x| x.1 == pile).count() as i32;
            if n >= max {
                continue;
            }
            for idx in 0..self.world.items.len() {
                if n >= max {
                    break;
                }
                let it = &self.world.items[idx];
                let f = &it.flags;
                if !is_wheelbarrow(&it.kind, &it.subtype) || self.imeta[idx].dead || self.imeta[idx].hauling || f.forbid || f.in_job || !f.on_ground || it.holder >= 0 || it.container >= 0 || it.building >= 0 {
                    continue;
                }
                let id = it.id;
                self.reserve_vehicle(id, true);
                self.carts.barrows.push((id, pile));
                n += 1;
            }
        }
    }

    /// [haul hook] Haul `id` to pile tile `dest` by wheelbarrow if the pile has a free one and
    /// the item weighs 75Γ or more. The caller reserved the tile and took the item off the haul
    /// list.
    pub(crate) fn carts_barrow_haul(&mut self, s: usize, id: i32, dest: u32, now: u64) -> bool {
        if self.carts.barrows.is_empty() || self.store_assigned(id) {
            return false;
        }
        let Some(&idx) = self.item_idx.get(&id) else { return false };
        if self.item_weight(idx as usize) < BARROW_MIN_WEIGHT {
            return false;
        }
        let Some(&(p, _)) = self.pile_at.get(&dest) else { return false };
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        let mut best: Option<(i32, i32)> = None;
        for &(item, pile) in &self.carts.barrows {
            if self.pile_index_of(pile) != Some(p) || self.carts.jobs.iter().any(|j| j.cart == item) {
                continue;
            }
            let Some(&bi) = self.item_idx.get(&item) else { continue };
            let it = &self.world.items[bi as usize];
            if !it.flags.on_ground || it.holder >= 0 {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.nav.comp_near(t) != c {
                continue;
            }
            let d = crate::manhattan(self.nav.xyz(self.ai[s].pos as usize), self.nav.xyz(t));
            if best.map(|b| d < b.0).unwrap_or(true) {
                best = Some((d, item));
            }
        }
        let Some((_, barrow)) = best else { return false };
        let jid = self.new_job(CJob { kind: JobK::Barrow, cart: barrow, item: id, dest, claimed: s as u32, ..Default::default() });
        self.ai[s].task = Task::Cart(jid);
        let g = self.item_goals(barrow);
        self.request_path(s, g, now);
        true
    }

    // ------------------------------------------------------------------ hash

    pub(crate) fn carts_hash(&self, mix: &mut impl FnMut(u64)) {
        if self.carts.carts.is_empty() && self.carts.jobs.is_empty() && self.carts.slow.is_empty() {
            return;
        }
        let st = &self.carts.stats;
        mix(st.trips ^ st.loaded << 16 ^ st.unloaded << 32 ^ st.tiles << 40);
        for c in &self.carts.carts {
            mix((c.item as u64) << 32 ^ c.tile as u64 ^ (c.state as u64) << 60);
            mix((c.v as u64) << 32 ^ c.prog as u32 as u64 ^ (c.dir as u64) << 56);
        }
        for j in &self.carts.jobs {
            mix((j.id as u64) << 32 ^ j.claimed as u64 ^ (j.phase as u64) << 24);
        }
        for &(s, p) in &self.carts.slow {
            mix((s as u64) << 16 ^ p as u64);
        }
    }
}

#[cfg(test)]
#[path = "minecarts_tests.rs"]
mod tests;

impl Carts {
    fn trip_count(&self, _route: i32) -> u64 {
        // Trips per route are kept in the route counters below.
        self.route_trips.iter().find(|r| r.0 == _route).map(|r| r.1).unwrap_or(0)
    }

    fn add_trip(&mut self, route: i32) {
        match self.route_trips.iter_mut().find(|r| r.0 == route) {
            Some(r) => r.1 += 1,
            None => self.route_trips.push((route, 1)),
        }
    }
}
