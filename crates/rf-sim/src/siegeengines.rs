//! [siege12] Siege engines (DF v50; wiki Siege engine, Ballista, Catapult, Bolt thrower,
//! Siege operator, Siege engineer).
//!
//! # DF rules implemented
//! - **Building**: a siege engine is a `SiegeEngine` building (subtype `Ballista`, `Catapult`
//!   or `BoltThrower`) built from parts made of wood at the siege workshop (labor Siege
//!   engineering): 3 ballista parts / 3 catapult parts (3x3), or bolt thrower parts with a
//!   mechanism, a chain and a bin (1x1). Engines do not block movement and can stand on top of
//!   walls. Catapults and ballistae keep the direction they were built in; bolt throwers can be
//!   turned at will (and turn toward an aimed target).
//! - **Operation**: one dwarf with the Siege operating labor crews an engine. He fetches ammo
//!   and loads it (stones for catapults, any stone but no clay; ballista arrows for ballistae;
//!   crossbow bolts for bolt throwers, a whole stack at once, "a large amount of ammo
//!   internally"), then fires. Loading time falls steeply with skill: "a whole month for an
//!   unskilled dwarf to load a catapult; a Legendary operator with nearby ammo will get several
//!   shots at a running enemy"; ballista arrows load faster.
//! - **Field of fire**: about 19 degrees across, at most 4 z-levels up or down; catapults have a
//!   minimum range of 30 tiles and fire over creatures in between. Ranges: catapult 80-100,
//!   ballista about 150, bolt thrower about 60 tiles. All three fire through fortifications.
//!   *Fire at will*: the operator fires while a visible hostile is in the field of fire.
//! - **Accuracy**: the engine aims at a point 60 tiles out along its facing, up to 10 tiles to
//!   either side when the shot is unskilled; three random numbers 0..14 are rolled and if any is
//!   below the operator's skill the shot goes straight (Novice 18.7 %, Competent 48.8 %,
//!   Legendary 100 %).
//! - **Projectiles** go through the combat module (material, momentum, armor and tissue layers):
//!   a ballista arrow strikes **every unit in every tile its head passes** (friend or foe) until
//!   it is spent after about 5-6 goblin-sized bodies or hits a wall; a catapult stone lands and
//!   bursts into shrapnel that hits whoever stands next to the landing tile; bolt thrower bolts
//!   hit the first creature in their line. Ammo is used up when it lands.
//! - Experience for loading and firing (Siege operating).
//!
//! # Player API
//! [`Sim::aim_siege_engine`] (round-12 contract): `Some(tile)` fires at that tile (the shot's
//! center line goes through it; a ballista or catapult only if it lies in its field of fire, a
//! bolt thrower turns to it), `None` = fire at will. [`Sim::siege_engines`] lists the engines.
//!
//! # Estimates (not on the wiki)
//! - Load time `base / (1 + skill² / 5)`: catapult 4 800 ticks unskilled, ballista 1 200, bolt
//!   thrower 600 per stack (Legendary: 104 / 26 / 13).
//! - Momentum: ballista arrow 1 500 (edged, contact 60), catapult stone 600 (blunt, contact 400),
//!   shrapnel 80 (blunt, contact 50, 50 % per neighbour), bolt thrower bolt 1.4 times a
//!   crossbow's (`min(F/20, maxvel/10 * weight)` with F 1.4 * 1 000).
//! - A ballista arrow's "body budget" is 5.5 goblins (60 000 cm³ each).
//! - Bolt thrower: one bolt every `40 * 10 / (10 + skill)` ticks while a target is in the field.
//! - Catapult stones land at the distance of the hostile that triggered the shot (fire at will)
//!   or of the aimed tile, clamped to 30..100.
//! - Experience 20 per load, 30 per shot.

use crate::combat::{AtkT, Blow, Role};
use crate::path::Goals;
use crate::rng::Rng;
use crate::{Kind, Sim, Task, NONE};
use rf_core::Pos;

/// Engine kinds (index into the tables below), named as in [`crate::SIEGE_ENGINES`].
pub const BALLISTA: u8 = 0;
pub const CATAPULT: u8 = 1;
pub const BOLT_THROWER: u8 = 2;

/// Range in tiles (wiki: ballista 130-200, "about 150 for an ordinary ballista"; catapult
/// 80-100; bolt thrower about 60).
const RANGE: [i32; 3] = [150, 100, 60];
/// Catapult minimum range.
const CATAPULT_MIN: i32 = 30;
/// Field of fire: half-angle 9.5 degrees -> lateral / forward <= tan(9.5 deg) = 0.1673.
const ARC_NUM: i32 = 1673;
const ARC_DEN: i32 = 10_000;
/// Height limit of the line of fire.
const MAX_DZ: i32 = 4;
/// Load time base (unskilled) per load.
const LOAD_BASE: [u64; 3] = [1_200, 4_800, 600];
/// Ballista arrow "body budget" in cm³ (5.5 goblins).
const ARROW_BUDGET: i64 = 330_000;
/// Re-check for targets while standing at a loaded engine.
const STANDBY: u64 = 50;

/// Counters for reports and tests.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SiegeStats {
    pub engines: u64,
    pub loads: u64,
    pub shots: u64,
    /// Shots that flew straight (passed the accuracy check).
    pub straight: u64,
    /// Blows struck on units by siege projectiles; on fort units (friendly fire); kills.
    pub hits: u64,
    pub friendly_hits: u64,
    pub kills: u64,
    pub shrapnel_hits: u64,
}

/// One siege engine for the UI.
#[derive(Clone, Debug, Default)]
pub struct EngineInfo {
    pub building: i32,
    /// `Ballista`, `Catapult` or `BoltThrower`.
    pub kind: &'static str,
    pub pos: Pos,
    /// Facing: `N`, `S`, `E` or `W`.
    pub facing: &'static str,
    /// Aimed target tile (None: fire at will).
    pub aim: Option<Pos>,
    /// Rounds loaded.
    pub loaded: u32,
    /// Unit id of the operator, if any.
    pub operator: Option<i32>,
    pub shots: u64,
}

/// Runtime state of one built engine.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Engine {
    pub bid: i32,
    pub kind: u8,
    /// Center tile.
    pub tile: u32,
    /// Facing (unit step, cardinal).
    pub fx: i8,
    pub fy: i8,
    /// Aimed tile, u32::MAX = fire at will.
    pub aim: u32,
    /// Rounds loaded, and the material and quality of the loaded ammo.
    pub rounds: u32,
    pub mat: u16,
    pub quality: u8,
    /// Ammo item kind subtype of the loaded bolts (bolt throwers).
    pub sub: String,
    /// Operator slot (NONE = none), the ammo item he fetches (-1 none), loading done at.
    pub op: u32,
    pub fetch: i32,
    pub load_until: u64,
    pub next_shot: u64,
    /// No ammo found: do not search again before this tick.
    pub no_ammo_until: u64,
    pub shots: u64,
}

/// Siege engine state of the sim (inside `combat::Combat`).
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub(crate) struct Engines {
    pub list: Vec<Engine>,
    pub stats: SiegeStats,
}

fn kind_of(sub: &str) -> Option<u8> {
    match sub {
        "Ballista" => Some(BALLISTA),
        "Catapult" => Some(CATAPULT),
        "BoltThrower" | "Bolt thrower" | "BOLT_THROWER" => Some(BOLT_THROWER),
        _ => None,
    }
}

fn kind_name(k: u8) -> &'static str {
    crate::round12_api::SIEGE_ENGINES.get(k as usize).copied().unwrap_or("Ballista")
}

fn facing_of(dir: &str) -> (i8, i8) {
    match dir.trim().to_ascii_uppercase().as_str() {
        "S" | "SOUTH" | "DOWN" | "D" => (0, 1),
        "E" | "EAST" | "RIGHT" | "R" => (1, 0),
        "W" | "WEST" | "LEFT" | "L" => (-1, 0),
        _ => (0, -1),
    }
}

fn facing_name(fx: i8, fy: i8) -> &'static str {
    match (fx, fy) {
        (0, 1) => "S",
        (1, 0) => "E",
        (-1, 0) => "W",
        _ => "N",
    }
}

/// Load time for one load at operator skill `skill` (estimate, see the module docs).
pub(crate) fn load_ticks(kind: u8, skill: i32) -> u64 {
    let s = skill.clamp(0, 20) as u64;
    (LOAD_BASE[kind as usize % 3] * 5 / (5 + s * s)).max(5)
}

/// Chance in 1/1000 that a shot goes straight: any of three rolls 0..14 below the skill.
pub(crate) fn straight_permille(skill: i32) -> u32 {
    let s = skill.clamp(0, 15) as f64;
    ((1.0 - (1.0 - s / 15.0).powi(3)) * 1000.0).round() as u32
}

/// Offset of point (dx, dy) from an engine facing (fx, fy): (forward, lateral).
fn local(fx: i8, fy: i8, dx: i32, dy: i32) -> (i32, i32) {
    (dx * fx as i32 + dy * fy as i32, dx * -(fy as i32) + dy * fx as i32)
}

fn in_arc(fwd: i32, lat: i32) -> bool {
    fwd > 0 && lat.abs() * ARC_DEN <= fwd * ARC_NUM
}

fn ammo_attack(contact: f32, pen: f32, edge: bool) -> AtkT {
    AtkT { verb2: "strike".into(), verb3: "strikes".into(), part: -1, contact, pen, vel: 1.0, prep: 3, rec: 3, edge, weight: 1, skill: String::new() }
}

impl Sim {
    /// [siege12] Every 100 ticks: register newly built engines, drop wrecked ones.
    pub(crate) fn engines_tick(&mut self, _now: u64) {
        let n = self.world.buildings.iter().filter(|b| b.built && b.kind == "SiegeEngine").count();
        if n != self.combat.engines.list.len() || n > 0 && self.combat.engines.list.iter().any(|e| self.world.building(e.bid).is_none()) {
            self.engines_scan();
        }
    }

    /// [siege12] Register built siege engines, drop ones whose building is gone.
    pub(crate) fn engines_scan(&mut self) {
        let mut found: Vec<(i32, u8, u32, (i8, i8))> = Vec::new();
        for b in &self.world.buildings {
            if b.kind != "SiegeEngine" || !b.built {
                continue;
            }
            let Some(k) = kind_of(&b.subtype) else { continue };
            let Some(t) = self.world.idx(b.center) else { continue };
            found.push((b.id, k, t as u32, facing_of(&b.direction)));
        }
        let before = self.combat.engines.list.len();
        let gone: Vec<u32> = self.combat.engines.list.iter().filter(|e| !found.iter().any(|f| f.0 == e.bid)).map(|e| e.op).collect();
        for op in gone {
            if op != NONE && (op as usize) < self.ai.len() {
                if let Task::SiegeOperate(_) = self.ai[op as usize].task {
                    self.cancel_task(op as usize);
                    self.ai[op as usize].task = Task::Idle;
                }
            }
        }
        self.combat.engines.list.retain(|e| found.iter().any(|f| f.0 == e.bid));
        for (bid, k, t, (fx, fy)) in found {
            if self.combat.engines.list.iter().any(|e| e.bid == bid) {
                continue;
            }
            self.combat.engines.list.push(Engine { bid, kind: k, tile: t, fx, fy, aim: u32::MAX, op: NONE, fetch: -1, ..Default::default() });
            self.combat.engines.stats.engines += 1;
        }
        self.combat.engines.list.sort_by_key(|e| e.bid);
        let _ = before;
    }

    /// Where the operator stands: behind the engine (bolt thrower: the tile opposite its firing
    /// direction), a ballista's crew beside it; else any walkable tile of or next to it.
    fn engine_goals(&self, e: &Engine) -> Goals {
        let mut g = Goals::default();
        let (x, y, z) = self.nav.xyz(e.tile as usize);
        let (fx, fy) = (e.fx as isize, e.fy as isize);
        let mut prefs: Vec<(isize, isize)> = Vec::new();
        match e.kind {
            BALLISTA => prefs.extend([(-fy, fx), (fy, -fx), (-fx, -fy)]),
            _ => prefs.extend([(-fx, -fy), (-fy, fx), (fy, -fx)]),
        }
        prefs.push((0, 0));
        for (dx, dy) in prefs {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
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

    /// Ammo item kinds an engine takes.
    fn engine_ammo_ok(&self, kind: u8, it: &rf_core::Item) -> bool {
        match kind {
            CATAPULT => it.kind == "BOULDER" && self.world.material(it.mat).map(|m| m.is_stone && !m.token.contains("CLAY")).unwrap_or(false),
            BALLISTA => it.kind == "SIEGEAMMO",
            _ => it.kind == "AMMO" && it.subtype.contains("BOLT"),
        }
    }

    /// Nearest free ammo item for engine kind `kind` in walk component `c`.
    fn engine_find_ammo(&self, kind: u8, c: u32, from: (usize, usize, usize)) -> Option<i32> {
        let mut best = (i32::MAX, -1);
        for (i, it) in self.world.items.iter().enumerate() {
            if !self.engine_ammo_ok(kind, it) {
                continue;
            }
            let m = &self.imeta[i];
            let f = &it.flags;
            if m.dead || m.reserved > 0 || m.hauling || it.holder >= 0 || it.building >= 0 || it.container >= 0 || !f.on_ground || f.forbid || f.in_job || f.construction {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.nav.comp_near(t) != c {
                continue;
            }
            let d = crate::manhattan(from, self.nav.xyz(t));
            if (d, it.id) < best {
                best = (d, it.id);
            }
        }
        (best.1 >= 0).then_some(best.1)
    }

    /// Rounds an engine holds when full.
    fn engine_cap(kind: u8) -> u32 {
        if kind == BOLT_THROWER {
            100
        } else {
            1
        }
    }

    /// A visible hostile the engine would fire at now: (slot, forward distance, z) of the
    /// nearest one in its field of fire, range and line of fire.
    fn engine_target(&self, e: &Engine, now: u64) -> Option<(usize, i32, usize)> {
        let (ex, ey, ez) = self.nav.xyz(e.tile as usize);
        let mut best: Option<(i32, usize, usize)> = None;
        for &h in &self.combat.hostiles {
            let h = h as usize;
            if self.ai[h].kind == Kind::Dead || self.combat.f[h].hidden || self.combat.f[h].role == Role::Leave && self.combat.f[h].raid < 3 {
                continue;
            }
            let (hx, hy, hz) = self.nav.xyz(self.ai[h].pos as usize);
            if (hz as i32 - ez as i32).abs() > MAX_DZ {
                continue;
            }
            let (dx, dy) = (hx as i32 - ex as i32, hy as i32 - ey as i32);
            let (fwd, lat) = if e.kind == BOLT_THROWER {
                // Turns toward its target: any direction, field measured from the target.
                (dx.abs().max(dy.abs()), 0)
            } else {
                local(e.fx, e.fy, dx, dy)
            };
            if !in_arc(fwd, lat) || fwd > RANGE[e.kind as usize] || e.kind == CATAPULT && fwd < CATAPULT_MIN {
                continue;
            }
            if e.kind != CATAPULT && !self.line_of_fire(self.world.pos_of(e.tile as usize), self.world.pos_of(self.ai[h].pos as usize), 20) {
                continue;
            }
            let _ = now;
            if best.map_or(true, |b| (fwd, h) < (b.0, b.1)) {
                best = Some((fwd, h, hz));
            }
        }
        best.map(|(fwd, h, z)| (h, fwd, z))
    }

    /// [siege12] decide hook: an idle siege operator takes an engine that needs loading (ammo
    /// at hand) or firing (loaded, target in the field or aimed).
    pub(crate) fn engines_try_job(&mut self, s: usize, now: u64) -> bool {
        if self.combat.engines.list.is_empty() || self.ai[s].labors & crate::labor::SIEGEOPERATE == 0 {
            return false;
        }
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if c == 0 {
            return false;
        }
        let from = self.nav.xyz(self.ai[s].pos as usize);
        for k in 0..self.combat.engines.list.len() {
            let e = self.combat.engines.list[k].clone();
            if e.op != NONE {
                continue;
            }
            let goals = self.engine_goals(&e);
            if !goals.as_slice().iter().any(|&t| self.nav.comp_root(t as usize) == c && !self.alert_blocks(s, t as usize)) {
                continue;
            }
            let fire = e.rounds > 0 && (e.aim != u32::MAX || self.engine_target(&e, now).is_some());
            let mut fetch = -1;
            if !fire {
                if e.rounds >= Self::engine_cap(e.kind) || now < e.no_ammo_until {
                    continue;
                }
                match self.engine_find_ammo(e.kind, c, from) {
                    Some(id) => fetch = id,
                    None => {
                        self.combat.engines.list[k].no_ammo_until = now + 600;
                        continue;
                    }
                }
            }
            let en = &mut self.combat.engines.list[k];
            en.op = s as u32;
            en.fetch = fetch;
            en.load_until = 0;
            self.ai[s].task = Task::SiegeOperate(e.bid);
            if fetch >= 0 {
                self.reserve(fetch);
                let g = self.item_goals(fetch);
                self.request_path(s, g, now);
            } else {
                self.request_path(s, goals, now);
            }
            return true;
        }
        false
    }

    fn engine_index(&self, bid: i32, s: usize) -> Option<usize> {
        self.combat.engines.list.iter().position(|e| e.bid == bid && e.op == s as u32)
    }

    /// [siege12] arrive hook of `Task::SiegeOperate`: pick up the ammo, carry it to the engine,
    /// load it, then fire while there is something to fire at.
    pub(crate) fn engines_arrive(&mut self, s: usize, bid: i32, now: u64) {
        let Some(k) = self.engine_index(bid, s) else {
            self.ai[s].task = Task::Idle;
            self.decide(s, now);
            return;
        };
        let e = self.combat.engines.list[k].clone();
        let goals = self.engine_goals(&e);
        // 1. Fetch the ammo.
        if e.fetch >= 0 && self.ai[s].carrying != e.fetch {
            if !self.item_reachable_here(s, e.fetch) {
                self.engines_cancel(s, bid);
                self.ai[s].task = Task::Idle;
                self.decide(s, now);
                return;
            }
            self.pickup(s, e.fetch);
            self.request_path(s, goals, now);
            return;
        }
        if !goals.as_slice().contains(&self.ai[s].pos) {
            if goals.n == 0 {
                self.engines_cancel(s, bid);
                self.ai[s].task = Task::Idle;
                self.decide(s, now);
                return;
            }
            self.request_path(s, goals, now);
            return;
        }
        let skill = self.world.units[self.ai[s].uidx].skill("SIEGEOPERATE");
        // 2. Load it.
        if e.fetch >= 0 {
            if e.load_until == 0 {
                let t = load_ticks(e.kind, skill);
                self.combat.engines.list[k].load_until = now + t;
                self.ai[s].wake = now + t;
                return;
            }
            if now < e.load_until {
                self.ai[s].wake = e.load_until;
                return;
            }
            let id = e.fetch;
            let (n, mat, q, sub) = match self.item_idx.get(&id) {
                Some(&i) => {
                    let it = &self.world.items[i as usize];
                    (if e.kind == BOLT_THROWER { it.stack.max(1) as u32 } else { 1 }, it.mat, it.quality, it.subtype.clone())
                }
                None => (0, 0, 0, String::new()),
            };
            let uidx = self.ai[s].uidx;
            self.world.units[uidx].inventory.retain(|&x| x != id);
            self.ai[s].carrying = -1;
            self.unreserve(id);
            if n > 0 {
                self.destroy_item(id);
            }
            let en = &mut self.combat.engines.list[k];
            en.rounds += n;
            en.mat = mat;
            en.quality = q;
            en.sub = sub;
            en.fetch = -1;
            en.load_until = 0;
            self.combat.engines.stats.loads += 1;
            self.add_xp(s, "SIEGEOPERATE", 20);
        }
        // 3. Fire while loaded and there is a target; else free the engine.
        let e = self.combat.engines.list[k].clone();
        if e.rounds > 0 && now >= e.next_shot {
            let aim = if e.aim != u32::MAX { Some((e.aim as usize, None)) } else { self.engine_target(&e, now).map(|(h, _, _)| (self.ai[h].pos as usize, Some(h))) };
            if let Some((tile, _)) = aim {
                self.engine_fire(k, s, tile, now);
                let e = &self.combat.engines.list[k];
                if e.rounds > 0 {
                    let wait = if e.kind == BOLT_THROWER { (40 * 10 / (10 + skill.max(0)) as u64).max(4) } else { 1 };
                    self.combat.engines.list[k].next_shot = now + wait;
                    self.ai[s].wake = now + wait;
                    return;
                }
            } else if e.kind == BOLT_THROWER || e.rounds >= Self::engine_cap(e.kind) {
                // Loaded, nothing to fire at: stand by a while, then go back to work.
                if e.next_shot + 20 * STANDBY > now {
                    self.ai[s].wake = now + STANDBY;
                    if e.next_shot == 0 {
                        self.combat.engines.list[k].next_shot = now;
                    }
                    return;
                }
            }
        } else if e.rounds > 0 {
            self.ai[s].wake = e.next_shot.max(now + 1);
            return;
        }
        let en = &mut self.combat.engines.list[k];
        en.op = NONE;
        en.next_shot = 0;
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    /// [siege12] cancel hook of `Task::SiegeOperate`.
    pub(crate) fn engines_cancel(&mut self, s: usize, bid: i32) {
        let Some(k) = self.engine_index(bid, s) else { return };
        let fetch = self.combat.engines.list[k].fetch;
        if fetch >= 0 {
            if self.ai[s].carrying == fetch {
                let t = self.ai[s].pos;
                self.drop_item(s, fetch, t);
            }
            self.unreserve(fetch);
        }
        let e = &mut self.combat.engines.list[k];
        e.op = NONE;
        e.fetch = -1;
        e.load_until = 0;
    }

    /// Fire engine `k` (operator `s`) at tile `aim`.
    fn engine_fire(&mut self, k: usize, s: usize, aim: usize, now: u64) {
        let e = self.combat.engines.list[k].clone();
        let skill = self.world.units[self.ai[s].uidx].skill("SIEGEOPERATE");
        let mut r = Rng::for_unit(self.seed, s, now, 0x5E6E);
        let ep = self.world.pos_of(e.tile as usize);
        let tp = self.world.pos_of(aim);
        let (dx, dy) = (tp.x as i32 - ep.x as i32, tp.y as i32 - ep.y as i32);
        let (mut fx, mut fy) = (e.fx, e.fy);
        if e.kind == BOLT_THROWER && (dx, dy) != (0, 0) {
            // Bolt throwers turn toward their target.
            (fx, fy) = if dx.abs() >= dy.abs() { (dx.signum() as i8, 0) } else { (0, dy.signum() as i8) };
            self.combat.engines.list[k].fx = fx;
            self.combat.engines.list[k].fy = fy;
        }
        let (fwd, lat) = local(fx, fy, dx, dy);
        let range = RANGE[e.kind as usize];
        if !in_arc(fwd, lat) && e.kind != BOLT_THROWER || fwd <= 0 {
            // Aimed outside the field of fire: nothing happens (the order waits).
            self.combat.engines.list[k].next_shot = now + STANDBY;
            return;
        }
        // Center line: through the aimed tile when aimed (bolt throwers always aim at their
        // target), else straight along the facing; then the drift of an unskilled shot.
        let aimed = e.aim != u32::MAX || e.kind == BOLT_THROWER;
        let base_lat = if aimed { lat } else { 0 };
        let straight = r.below(1000) < straight_permille(skill);
        let drift = if straight { 0 } else { r.below(21) as i32 - 10 };
        self.combat.engines.stats.shots += 1;
        if straight {
            self.combat.engines.stats.straight += 1;
        }
        self.combat.engines.list[k].rounds -= 1;
        self.combat.engines.list[k].shots += 1;
        self.add_xp(s, "SIEGEOPERATE", 30);
        // Point 60 tiles out (or the target's distance) along the center line, with drift.
        let dist = match e.kind {
            CATAPULT => fwd.clamp(CATAPULT_MIN, range),
            _ => fwd.max(1),
        };
        let lat_at = |d: i32| base_lat * d / fwd.max(1) + drift * d / 60;
        let to_xy = |d: i32, l: i32| -> (i32, i32) {
            // forward along (fx, fy), lateral along (-fy, fx)
            (ep.x as i32 + d * fx as i32 - l * fy as i32, ep.y as i32 + d * fy as i32 + l * fx as i32)
        };
        let dz = (tp.z as i32 - ep.z as i32).clamp(-MAX_DZ, MAX_DZ);
        let z_at = |d: i32| ep.z as i32 + dz * d.min(fwd.max(1)) / fwd.max(1);
        let name = kind_name(e.kind);
        let what = match e.kind {
            BALLISTA => "the flying ballista arrow",
            CATAPULT => "the flying rock",
            _ => "the flying bolt",
        };
        self.combat.log_line(format!("{} fires the {}!", crate::combat::cap_pub(&self.uname(s)), name.to_ascii_lowercase()));
        match e.kind {
            CATAPULT => {
                let (x, y) = to_xy(dist, lat_at(dist));
                let p = Pos::new(x.clamp(0, self.world.dims.x as i32 - 1) as i16, y.clamp(0, self.world.dims.y as i32 - 1) as i16, z_at(dist).clamp(0, self.world.dims.z as i32 - 1) as i16);
                let land = self.engine_land(p);
                let wmech = self.mech_of_mat(e.mat);
                let units = self.units_near(land, 0);
                for t in units {
                    self.engine_hit(s, t, &e, what, ammo_attack(400.0, 200.0, false), wmech, 600.0, 1500.0, now, &mut r);
                }
                for t in self.units_near(land, 1) {
                    if self.upos(t) == land || !r.chance(1, 2) {
                        continue;
                    }
                    self.combat.engines.stats.shrapnel_hits += 1;
                    self.engine_hit(s, t, &e, "the flying shrapnel", ammo_attack(50.0, 100.0, false), wmech, 80.0, 100.0, now, &mut r);
                }
            }
            _ => {
                // Ballista arrows and bolts: walk the line tile by tile.
                let mut budget = ARROW_BUDGET;
                let mut prev = ep;
                for d in 1..=range {
                    let (x, y) = to_xy(d, lat_at(d));
                    let z = z_at(d);
                    if x < 0 || y < 0 || z < 0 || x >= self.world.dims.x as i32 || y >= self.world.dims.y as i32 || z >= self.world.dims.z as i32 {
                        break;
                    }
                    let cur = Pos::new(x as i16, y as i16, z as i16);
                    // Walls, floors between levels and closed buildings stop it; fortifications
                    // do not.
                    if !self.line_of_fire(prev, cur, crate::ranged::FORTIFICATION_SKILL) {
                        break;
                    }
                    prev = cur;
                    let mut stop = false;
                    for t in self.units_near(cur, 0) {
                        if t == s {
                            continue;
                        }
                        let size = self.world.units[self.ai[t].uidx].body_size.max(1000) as i64;
                        if e.kind == BALLISTA {
                            let wmech = self.mech_of_mat(e.mat);
                            self.engine_hit(s, t, &e, what, ammo_attack(60.0, 3000.0, true), wmech, 1500.0, 15_000.0, now, &mut r);
                            budget -= size;
                            if budget <= 0 {
                                stop = true;
                                break;
                            }
                        } else {
                            let wmech = self.mech_of_mat(e.mat);
                            let weight = 20.0 * wmech.dens.max(1.0) / 100_000.0;
                            let m = (1400.0f32 / 20.0).min(200.0 / 10.0 * weight) * 1.4;
                            self.engine_hit(s, t, &e, what, ammo_attack(5.0, 1000.0, true), wmech, m, 20.0, now, &mut r);
                            stop = true;
                            break;
                        }
                    }
                    if stop {
                        break;
                    }
                }
            }
        }
    }

    /// The tile a falling catapult stone ends on (first floor at or below `p`).
    fn engine_land(&self, mut p: Pos) -> Pos {
        for _ in 0..20 {
            let sh = self.world.shape(p);
            if sh.has_floor() || p.z == 0 {
                break;
            }
            if sh.is_solid() {
                p = p.offset(0, 0, 1);
                break;
            }
            p = p.offset(0, 0, -1);
        }
        p
    }

    /// Living units within `r` tiles (same level) of `p`, slot order.
    fn units_near(&self, p: Pos, r: i32) -> Vec<usize> {
        (0..self.ai.len())
            .filter(|&t| {
                if self.ai[t].kind == Kind::Dead {
                    return false;
                }
                let q = self.world.units[self.ai[t].uidx].pos;
                q.z == p.z && (q.x as i32 - p.x as i32).abs() <= r && (q.y as i32 - p.y as i32).abs() <= r
            })
            .collect()
    }

    /// One siege projectile blow on unit `t` through the combat module.
    #[allow(clippy::too_many_arguments)]
    fn engine_hit(&mut self, s: usize, t: usize, e: &Engine, what: &str, atk: AtkT, wmech: crate::combat::Mech, momentum: f32, size: f32, now: u64, r: &mut Rng) {
        self.ensure_body(s);
        self.ensure_body(t);
        let fort = self.combat.f[t].role != Role::Hostile && !self.world.units[self.ai[t].uidx].flags.hostile;
        self.combat.engines.stats.hits += 1;
        if fort {
            self.combat.engines.stats.friendly_hits += 1;
        }
        let dn = self.uname(t);
        let skill = self.world.units[self.ai[s].uidx].skill("SIEGEOPERATE");
        let m = momentum * (80 + r.below(21)) as f32 / 100.0;
        let killed = self.strike(s, t, now, r, Blow { atk, armed: true, wmech, wsize: size, wquality: e.quality.min(5) as f32, wdesc: None, skill, an: what.to_string(), dn, momentum: Some(m) });
        if killed {
            self.combat.engines.stats.kills += 1;
        } else if self.ai[t].kind != Kind::Dead && !fort {
            self.after_attacked(t, s, now);
        }
    }

    // ------------------------------------------------------------------ player API

    /// [siege12] Turn a siege engine: catapults and ballistae only before they are built (they
    /// "stay at their built orientation forever"), bolt throwers at any time. `dir` = N/S/E/W.
    pub fn set_siege_engine_facing(&mut self, building: i32, dir: &str) -> bool {
        let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == building && b.kind == "SiegeEngine") else { return false };
        let k = kind_of(&b.subtype);
        if b.built && k != Some(BOLT_THROWER) {
            return false;
        }
        let (fx, fy) = facing_of(dir);
        b.direction = facing_name(fx, fy).into();
        if let Some(e) = self.combat.engines.list.iter_mut().find(|e| e.bid == building) {
            e.fx = fx;
            e.fy = fy;
        }
        true
    }

    /// [siege12] The siege engines of the fort.
    pub fn siege_engines(&self) -> Vec<EngineInfo> {
        self.combat
            .engines
            .list
            .iter()
            .map(|e| EngineInfo {
                building: e.bid,
                kind: kind_name(e.kind),
                pos: self.world.pos_of(e.tile as usize),
                facing: facing_name(e.fx, e.fy),
                aim: (e.aim != u32::MAX).then(|| self.world.pos_of(e.aim as usize)),
                loaded: e.rounds,
                operator: (e.op != NONE).then(|| self.ai.get(e.op as usize).map(|a| a.id)).flatten(),
                shots: e.shots,
            })
            .collect()
    }

    /// [siege12] Siege engine counters.
    pub fn siege_stats(&self) -> SiegeStats {
        self.combat.engines.stats.clone()
    }

    /// [siege12] Body of the round-12 `aim_siege_engine`.
    pub(crate) fn engines_aim(&mut self, building: i32, target: Option<Pos>) {
        if self.combat.engines.list.iter().all(|e| e.bid != building) {
            self.engines_scan();
        }
        let tile = target.and_then(|p| self.world.idx(p)).map(|i| i as u32).unwrap_or(u32::MAX);
        if let Some(e) = self.combat.engines.list.iter_mut().find(|e| e.bid == building) {
            e.aim = tile;
            e.next_shot = 0;
        }
    }

    /// [siege12] Caption of the operator's task.
    pub(crate) fn engines_caption(&self, bid: i32) -> &'static str {
        match self.combat.engines.list.iter().find(|e| e.bid == bid) {
            Some(e) if e.fetch >= 0 => "Load Siege Engine",
            _ => "Fire Siege Engine",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accuracy_matches_wiki_table() {
        // wiki Siege operator: Novice 18.7 %, Adequate 34.9 %, Competent 48.8 %, Expert 89.8 %,
        // Accomplished 96.3 %, Master (12) 99.2 %, Legendary 100 %, Dabbling 0.
        assert_eq!(straight_permille(0), 0);
        assert_eq!(straight_permille(1), 187);
        assert_eq!(straight_permille(2), 349);
        assert_eq!(straight_permille(3), 488);
        assert_eq!(straight_permille(8), 898);
        assert_eq!(straight_permille(10), 963);
        assert_eq!(straight_permille(12), 992);
        assert_eq!(straight_permille(15), 1000);
    }

    #[test]
    fn field_of_fire_is_19_degrees() {
        // 60 tiles out, 10 to either side is inside (the unskilled spread), 11 is outside.
        assert!(in_arc(60, 10) && in_arc(60, -10));
        assert!(!in_arc(60, 11));
        assert!(!in_arc(-5, 0));
        assert_eq!(local(0, -1, 3, -40), (40, 3));
        assert_eq!(local(1, 0, 40, 3), (40, 3));
    }

    /// A ballista (facing north) at (24, 40), its operator next to it with ballista arrows at
    /// hand, and a goblin `dist` tiles north of it on the firing axis.
    fn range(kind: &str, skill: i32, dist: i16, ammo: (&str, &str, i32)) -> (Sim, usize, usize) {
        let mut w = rf_core::World::synthetic(48, 64, 20);
        let sz = (20i16 * 2 / 3).max(2);
        w.units.retain(|u| u.id <= 2 || u.id == 100);
        let big = kind != "BoltThrower";
        let (x1, y1, x2, y2) = if big { (23, 39, 25, 41) } else { (24, 40, 24, 40) };
        w.buildings.push(rf_core::Building { id: 9001, kind: "SiegeEngine".into(), subtype: kind.into(), x1, y1, x2, y2, z: sz, center: Pos::new(24, 40, sz), built: true, direction: "N".into(), owner: -1, mat: 3, ..Default::default() });
        for k in 0..3 {
            w.items.push(rf_core::Item { id: 50_000 + k, kind: ammo.0.into(), subtype: ammo.1.into(), mat: if ammo.0 == "BOULDER" { 0 } else { 3 }, pos: Pos::new(27, 44, sz), stack: ammo.2, flags: rf_core::ItemFlags { on_ground: true, ..Default::default() }, container: -1, holder: -1, building: -1, ..Default::default() });
        }
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
            if u.id == 2 {
                u.pos = Pos::new(30, 50, sz);
                u.labors.clear();
            } else if u.id == 1 {
                u.pos = Pos::new(26, 43, sz);
                u.invader_civ = -1;
                u.labors = vec!["SIEGEOPERATE".into()];
                u.skills.push(rf_core::Skill { id: "SIEGEOPERATE".into(), rating: skill, xp: 0 });
            } else {
                u.pos = Pos::new(24, 40 - dist, sz);
                u.race = "GOBLIN".into();
                u.flags.animal = false;
                u.flags.tame = false;
                u.body_size = 60_000;
                u.invader_civ = 77;
            }
        }
        let mut sim = Sim::new(w, 9);
        sim.set_invasions(false);
        sim.make_hostile(100);
        let (d, g) = (sim.unit_slot[&1], sim.unit_slot[&100]);
        sim.combat.f[g].paralyzed = true; // stays on the firing axis
        (sim, d, g)
    }

    #[test]
    fn ballista_arrow_strikes_the_goblin_on_its_axis() {
        let (mut sim, _d, g) = range("Ballista", 15, 40, ("SIEGEAMMO", "ITEM_SIEGEAMMO_BALLISTA", 1));
        for _ in 0..200 {
            sim.run_ticks(10);
            if std::env::var_os("SIEGE_DEBUG").is_some() {
                eprintln!("t={} op={:?} task={:?} gob={:?} {:?} {:?}", sim.ticks, sim.upos(_d), sim.ai[_d].task, sim.upos(g), sim.siege_engines(), sim.siege_stats());
            }
            if sim.siege_stats().hits > 0 {
                break;
            }
        }
        let st = sim.siege_stats();
        assert_eq!(st.engines, 1, "{st:?}");
        assert!(st.loads >= 1 && st.shots >= 1, "{st:?} {:?}", sim.siege_engines());
        assert!(st.hits >= 1, "a legendary shot along the axis hits: {st:?}");
        assert_eq!(st.friendly_hits, 0);
        let _ = g;
    }

    #[test]
    fn ballista_arrow_hits_a_dwarf_in_its_path_too() {
        let (mut sim, _d, _g) = range("Ballista", 15, 40, ("SIEGEAMMO", "ITEM_SIEGEAMMO_BALLISTA", 1));
        // A second citizen standing still on the axis between the engine and the goblin.
        let other = (0..sim.ai.len()).find(|&s| s != _d && s != _g && sim.ai[s].kind == Kind::Worker);
        if let Some(o) = other {
            let sz = sim.upos(_d).z;
            let q = sim.world.idx(Pos::new(24, 25, sz)).unwrap() as u32;
            sim.move_to(o, q, sim.ticks);
            sim.ai[o].wake = u64::MAX;
            sim.ai[o].path.clear();
        } else {
            panic!("no second citizen");
        }
        for _ in 0..200 {
            sim.run_ticks(10);
            if sim.siege_stats().shots > 0 {
                break;
            }
        }
        let st = sim.siege_stats();
        assert!(st.friendly_hits >= 1, "{st:?}");
    }

    #[test]
    fn catapult_stone_lands_and_bursts() {
        let (mut sim, _d, _g) = range("Catapult", 15, 40, ("BOULDER", "", 1));
        sim.run_ticks(3000);
        let st = sim.siege_stats();
        assert!(st.shots >= 1, "{st:?} {:?}", sim.siege_engines());
        assert!(st.hits >= 1, "a straight shot lands on the goblin 40 tiles out: {st:?}");
    }

    #[test]
    fn bolt_thrower_turns_and_fires_bolts() {
        let (mut sim, _d, _g) = range("BoltThrower", 10, 30, ("AMMO", "ITEM_AMMO_BOLTS", 25));
        sim.run_ticks(1500);
        let st = sim.siege_stats();
        assert!(st.shots >= 3, "{st:?} {:?}", sim.siege_engines());
        assert!(st.hits >= 1, "{st:?}");
    }

    #[test]
    fn aimed_ballista_fires_at_the_tile() {
        let (mut sim, _d, _g) = range("Ballista", 15, 50, ("SIEGEAMMO", "ITEM_SIEGEAMMO_BALLISTA", 1));
        let sz = sim.world.units[sim.ai[_d].uidx].pos.z;
        sim.run_ticks(200); // registered and loaded (the goblin is out of the field: fire at will waits)
        sim.aim_siege_engine(9001, Some(Pos::new(26, 0, sz)));
        sim.run_ticks(600);
        let e = &sim.siege_engines()[0];
        assert_eq!(e.aim, Some(Pos::new(26, 0, sz)));
        assert!(sim.siege_stats().shots >= 1, "{:?}", sim.siege_stats());
    }

    #[test]
    fn loading_gets_faster_with_skill() {
        assert_eq!(load_ticks(CATAPULT, 0), 4800);
        assert!(load_ticks(CATAPULT, 15) < 120, "{}", load_ticks(CATAPULT, 15));
        assert!(load_ticks(BALLISTA, 0) < load_ticks(CATAPULT, 0));
    }
}
