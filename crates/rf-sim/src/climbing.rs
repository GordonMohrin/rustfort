//! [siege12] Climbing invaders (DF v50; wiki Climbing, Jumping).
//!
//! # DF rules implemented
//! - Invaders that have a grasping body part (a hand) climb: up natural rough walls, constructed
//!   walls and fortifications, ice and trees; **not smoothed or engraved natural walls** (smoothed
//!   pillars yes) and not carved fortifications. A floor sticking out over the wall top (or a raised
//!   drawbridge) stops them; a one-tile overhanging wall is climbed from one level below, a
//!   two-tile one is safe (see [`crate::nav::ClimbGraph`] for the exact moves).
//! - They jump across one-tile gaps ("one-tile moats are unlikely to be a significant obstacle").
//! - Climbers path over the climbing graph (A* and shared flows) and pick targets in their
//!   **climbing group**: walk components joined by climbing moves ([`crate::nav::ClimbNet`]), so
//!   `find_target` and `pursue` see the fort behind its walls.
//! - Dwarves and other fort units do not climb in normal pathing (wiki: they only climb when
//!   stuck or chased); wild animals do not climb here either.
//! - **Losing hold**: unskilled climbers often lose their hold climbing walls (not trees) while in
//!   the air (at least one level above the floor below); higher Climber skill lowers the chance.
//!   A climber that falls lands on the first floor below; helpless climbers (unconscious,
//!   paralysed, without a working hand) fall at once.
//! - Climbing is a gait: speed does not depend on skill.
//!
//! # Estimates (not on the wiki)
//! - Climbing moves take `CLIMB_SPEED` times the walking time per tile (the vanilla climbing
//!   gaits are about six times slower than walking); path costs use the same factor.
//! - Losing hold: `SLIP_PERMILLE * (15 - skill) / 15` per climbing step in the air next to a wall
//!   (unskilled 20 %, Competent 16 %, Legendary 0).
//! - Fall damage as for falls from retracting bridges (`defense`): 2+ levels knock out for 100
//!   ticks per level, 10+ levels kill.
//! - 1 Climber experience per climbing step (wiki: climbers improve by climbing walls).
//! - Invaders the sim generates get Climber 1-3 at the rates of Gordon's r6 goblins.

use crate::combat::bp;
use crate::nav::{climb_classes, ClimbGraph, ClimbNet, SOLID, WALK};
use crate::path::Goals;
use crate::rng::Rng;
use crate::{Kind, Sim};

/// A* node limit of one climbing search (no landmark heuristic: kept below the walkers' 400k).
pub(crate) const CLIMB_MAX_NODES: u32 = 150_000;
/// The climbing net is rebuilt after map changes at most this often (a day), at once after a
/// walk-component rebuild.
const NET_PERIOD: u64 = 1200;
/// The net is kept up to date while a climber thought within this many ticks.
const NET_ACTIVE: u64 = 1200;
/// Climbing step time and cost factor (estimate, see the module docs).
pub(crate) const CLIMB_SPEED: u64 = 6;
/// Losing hold per climbing step in the air next to a wall, unskilled (per mille).
const SLIP_PERMILLE: u32 = 200;

/// Counters for reports and tests.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ClimbStats {
    /// Units found able to climb.
    pub climbers: u64,
    /// Climbing steps, falls after losing hold, falls of helpless climbers, deaths from falls.
    pub moves: u64,
    pub slips: u64,
    pub dropped: u64,
    pub fall_deaths: u64,
    /// Climbing net builds and the hanging tiles / linked roots of the last one.
    pub net_builds: u64,
    pub net_holds: u64,
    pub net_roots: u64,
    /// Highest level climbers reached above their start (debug).
    pub max_height: u64,
    /// Wall-clock seconds spent building climbing nets (debug; not part of the state hash).
    pub net_secs: f64,
}

/// Climbing state of the sim (inside `combat::Combat`).
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub(crate) struct Climbing {
    /// `nav::climb_class` per tiletype id (filled when the first hostile shows up).
    pub cls: Vec<u8>,
    pub net: ClimbNet,
    /// Last tick a climber thought.
    pub seen: u64,
    pub stats: ClimbStats,
}

impl Sim {
    /// [siege12] Per-tick climbing upkeep while hostiles are about.
    pub(crate) fn climb_tick(&mut self, now: u64) {
        if self.combat.climb.cls.is_empty() {
            self.combat.climb.cls = climb_classes(&self.world.tiletypes);
        }
        let c = &self.combat.climb;
        if c.seen == 0 || now > c.seen + NET_ACTIVE {
            return;
        }
        let net = &c.net;
        let stale = !net.built || net.rebuilds != self.nav.rebuilds || (net.version != self.nav.version && now >= net.built_at + NET_PERIOD);
        if stale {
            let t0 = std::time::Instant::now();
            let g = ClimbGraph { nav: &self.nav, tiles: &self.world.tiles, cls: &self.combat.climb.cls };
            let mut net = std::mem::take(&mut self.combat.climb.net);
            net.build(&g, now);
            let st = &mut self.combat.climb.stats;
            st.net_secs += t0.elapsed().as_secs_f64();
            st.net_builds += 1;
            st.net_holds = net.holds;
            st.net_roots = net.groups.len() as u64;
            self.combat.climb.net = net;
        }
        if now % 10 == 5 {
            // Helpless climbers lose their hold.
            for k in 0..self.combat.hostiles.len() {
                let s = self.combat.hostiles[k] as usize;
                if self.combat.f[s].climb != 2 || self.ai[s].kind == Kind::Dead || self.nav.walkable(self.ai[s].pos as usize) {
                    continue;
                }
                if !self.can_fight(s, now) || !self.climb_has_hand(s) {
                    self.combat.climb.stats.dropped += 1;
                    self.climb_fall(s, now, "falls");
                }
            }
        }
    }

    /// [siege12] Decide once whether unit `s` climbs: invaders with a grasping part.
    pub(crate) fn climb_classify(&mut self, s: usize) {
        let u = &self.world.units[self.ai[s].uidx];
        let invader = self.combat.f[s].siege > 0 || u.invader_civ >= 0 || self.combat.f[s].raid != 0;
        let mut can = invader && !u.flags.animal && !u.flags.citizen;
        if can {
            let race = u.race.clone();
            let flier = self.world.creature_raws.iter().find(|c| c.token == race).map(|c| c.flags.iter().chain(c.castes.iter().flat_map(|k| k.flags.iter())).any(|f| f == "FLIER" || f == "CANNOT_CLIMB")).unwrap_or(false);
            self.ensure_body(s);
            let b = self.combat.f[s].body;
            let hands = b != u16::MAX && self.combat.bodies[b as usize].parts.iter().any(|p| p.flags & bp::GRASP != 0);
            can = !flier && hands;
        }
        self.combat.f[s].climb = if can { 2 } else { 1 };
        if can {
            self.combat.climb.stats.climbers += 1;
        }
    }

    /// [siege12] A working hand (grasping part not severed or broken).
    fn climb_has_hand(&self, s: usize) -> bool {
        let f = &self.combat.f[s];
        if f.body == u16::MAX {
            return true;
        }
        let b = &self.combat.bodies[f.body as usize];
        b.parts.iter().enumerate().any(|(i, p)| p.flags & bp::GRASP != 0 && f.part_state.get(i).map_or(true, |&st| st & 3 == 0))
    }

    /// [siege12] Climbing group of the unit or item on tile `i` (0 = none). A climber hanging on
    /// a wall belongs to the group of the floor below it.
    pub(crate) fn climb_group_at(&self, i: usize) -> u32 {
        let mut c = self.nav.comp_near(i);
        if c == 0 {
            let (x, y, z) = self.nav.xyz(i);
            for zz in (z.saturating_sub(16)..z).rev() {
                let q = self.nav.idx(x, y, zz);
                if self.nav.flags[q] & WALK != 0 {
                    c = self.nav.comp_root(q);
                    break;
                }
                if self.nav.flags[q] & SOLID != 0 {
                    break;
                }
            }
        }
        self.combat.climb.net.group(c)
    }

    /// [siege12] Tile `i` is a place a climber can be (walkable or hanging).
    pub(crate) fn climb_node(&self, i: usize) -> bool {
        ClimbGraph { nav: &self.nav, tiles: &self.world.tiles, cls: &self.combat.climb.cls }.node(i)
    }

    /// [siege12] A climbing hostile takes the next step of its path onto `next` when that is not
    /// a plain walking step (hanging tiles, climbing off a wall). False for non-climbers.
    pub(crate) fn climb_step(&mut self, s: usize, next: u32, now: u64) -> bool {
        if self.combat.f.get(s).map_or(true, |f| f.climb != 2) {
            return false;
        }
        let (hold_next, wall_only, in_air) = {
            let g = ClimbGraph { nav: &self.nav, tiles: &self.world.tiles, cls: &self.combat.climb.cls };
            let n = next as usize;
            if !g.node(n) {
                // The wall changed under the path: plan again (climbing).
                let dest = *self.ai[s].path.last().unwrap();
                self.request_path_climb(s, Goals::one(dest), now);
                return true;
            }
            let hold = !self.nav.walkable(n);
            let layer = self.nav.layer;
            let air = hold && n >= layer && self.nav.flags[n - layer] & (WALK | SOLID) == 0;
            (hold, hold && g.hold_wall_only(n), air)
        };
        let from = self.ai[s].pos as usize;
        self.ai[s].path_i += 1;
        self.move_to(s, next, now);
        let ai = &mut self.ai[s];
        ai.wake = now + (ai.speed.max(1) as u64 * CLIMB_SPEED / 100).max(1);
        self.combat.climb.stats.moves += 1;
        self.add_xp(s, "CLIMBING", 1);
        let (_, _, z0) = self.nav.xyz(from);
        let (_, _, z1) = self.nav.xyz(next as usize);
        if z1 > z0 {
            self.combat.climb.stats.max_height = self.combat.climb.stats.max_height.max((z1 - z0) as u64);
        }
        if hold_next && wall_only && in_air {
            let skill = self.world.units[self.ai[s].uidx].skill("CLIMBING").clamp(0, 15) as u32;
            let p = SLIP_PERMILLE * (15 - skill) / 15;
            if Rng::for_unit(self.seed, s, now, 0xC11B).below(1000) < p {
                self.combat.climb.stats.slips += 1;
                self.climb_fall(s, now, "loses hold and falls");
            }
        }
        true
    }

    /// [siege12] Before a climber thinks: one left hanging without a climbing path (its plan was
    /// replaced, or the wall it held is gone) lets go. True if it fell.
    pub(crate) fn climb_failsafe(&mut self, s: usize, now: u64) -> bool {
        let pos = self.ai[s].pos as usize;
        if self.nav.walkable(pos) {
            return false;
        }
        let ai = &self.ai[s];
        let next_ok = ai.path_i < ai.path.len() && self.climb_node(ai.path[ai.path_i] as usize);
        if self.climb_node(pos) && next_ok {
            return false;
        }
        if self.climb_node(pos) && ai.path_i >= ai.path.len() && self.combat.f[s].target != crate::NONE {
            return false; // pursue plans a new climbing path from here
        }
        self.combat.climb.stats.dropped += 1;
        self.climb_fall(s, now, "falls");
        true
    }

    /// [siege12] Unit `s` drops to the first floor below (fall damage as in `defense`).
    pub(crate) fn climb_fall(&mut self, s: usize, now: u64, verb: &str) {
        let from = self.ai[s].pos as usize;
        let (x, y, z) = self.nav.xyz(from);
        let mut land = None;
        for zz in (0..z).rev() {
            let q = self.nav.idx(x, y, zz);
            if self.nav.walkable(q) {
                land = Some((q as u32, z - zz));
                break;
            }
            if self.nav.flags[q] & SOLID != 0 {
                break;
            }
        }
        let Some((q, levels)) = land else { return };
        let name = self.uname(s);
        self.cancel_task(s);
        let p = self.world.pos_of(q as usize);
        self.ai[s].pos = q;
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].pos = p;
        self.ai[s].path.clear();
        self.ai[s].path_i = 0;
        self.combat.log_line(format!("{} {verb}!", crate::combat::cap_pub(&name)));
        self.combat_sync_units();
        if levels >= 10 {
            self.combat.climb.stats.fall_deaths += 1;
            self.combat_kill(s, now, "has died from a fall");
        } else if levels >= 2 {
            let f = &mut self.combat.f[s];
            f.uncon_until = f.uncon_until.max(now + 100 * levels as u64);
            self.ai[s].wake = now + 100 * levels as u64 + 1;
        } else {
            self.ai[s].wake = now + 1;
        }
    }

    /// [siege12] Climbing counters.
    pub fn climb_stats(&self) -> ClimbStats {
        self.combat.climb.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::{Pos, TiletypeInfo, TiletypeTable, TtMaterial, TtSpecial};

    #[derive(Clone, Copy, PartialEq)]
    enum Wall {
        Rough,
        Constructed,
        Smooth,
        Overhang,
        OverhangWall,
    }

    /// A 3-high ring wall around (16, 16) with dwarf 1 inside and goblin 100 (an invader)
    /// outside. Returns the sim, the slots and the surface level.
    fn walled(kind: Wall) -> (Sim, usize, usize, i16) {
        let mut w = rf_core::World::synthetic(48, 48, 20);
        let mut infos = w.tiletypes.infos.clone();
        let sid = infos.len() as u16;
        infos.push(TiletypeInfo { id: sid, name: "StoneWallSmoothLRUD".into(), shape: rf_core::Shape::Wall, material: TtMaterial::Stone, special: TtSpecial::Smooth, variant: 0, direction: "LRUD".into() });
        w.tiletypes = TiletypeTable::new(infos);
        let sz = (20i16 * 2 / 3).max(2);
        let tt = |n: &str| w.tiletypes.by_name(n).unwrap();
        let wall = match kind {
            Wall::Rough | Wall::Overhang | Wall::OverhangWall => tt("StoneWall"),
            Wall::Constructed => tt("ConstructedWall"),
            Wall::Smooth => sid,
        };
        let (cfloor, cwall) = (tt("ConstructedFloor"), tt("ConstructedWall"));
        for y in 8..=24i16 {
            for x in 8..=24i16 {
                let ring = x == 8 || x == 24 || y == 8 || y == 24;
                if !ring {
                    continue;
                }
                for z in sz..sz + 3 {
                    let t = w.tile_mut(Pos::new(x, y, z)).unwrap();
                    t.tt = wall;
                    t.mat = 0;
                }
            }
        }
        if matches!(kind, Wall::Overhang | Wall::OverhangWall) {
            // One tile sticking out over the outside at the level of the wall top.
            for y in 7..=25i16 {
                for x in 7..=25i16 {
                    if x == 7 || x == 25 || y == 7 || y == 25 {
                        let t = w.tile_mut(Pos::new(x, y, sz + 3)).unwrap();
                        t.tt = if kind == Wall::Overhang { cfloor } else { cwall };
                        t.mat = 0;
                    }
                }
            }
        }
        w.units.retain(|u| u.id == 1 || u.id == 100);
        for u in &mut w.units {
            u.thirst_timer = 0;
            u.hunger_timer = 0;
            u.sleepiness_timer = 0;
            if u.id == 1 {
                u.pos = Pos::new(16, 16, sz);
                u.invader_civ = -1;
            } else {
                u.pos = Pos::new(40, 16, sz);
                u.race = "GOBLIN".into();
                u.flags.animal = false;
                u.flags.tame = false;
                u.body_size = 60_000;
                u.invader_civ = 77;
            }
        }
        let mut sim = Sim::new(w, 5);
        sim.set_invasions(false);
        sim.make_hostile(100);
        let (d, g) = (sim.unit_slot[&1], sim.unit_slot[&100]);
        (sim, d, g, sz)
    }

    /// Ticks until the goblin stands next to the dwarf (None: not within `limit`).
    fn reach(sim: &mut Sim, d: usize, g: usize, limit: u32) -> Option<u32> {
        for t in 0..limit / 10 {
            sim.run_ticks(10);
            if sim.upos(g).dist(sim.upos(d)) <= 1 || sim.combat.stats.attacks > 0 {
                return Some(t * 10);
            }
        }
        None
    }

    #[test]
    fn invader_climbs_a_rough_wall() {
        let (mut sim, d, g, _) = walled(Wall::Rough);
        assert!(!sim.reachable(sim.upos(g), sim.upos(d)), "no walking way in");
        let t = reach(&mut sim, d, g, 6_000);
        assert!(t.is_some(), "the goblin climbs over the wall: {:?}", sim.climb_stats());
        let st = sim.climb_stats();
        assert_eq!(sim.combat.f[g].climb, 2);
        assert!(st.moves >= 4 && st.max_height >= 1, "{st:?}");
    }

    #[test]
    fn invader_climbs_a_constructed_wall() {
        let (mut sim, d, g, _) = walled(Wall::Constructed);
        assert!(reach(&mut sim, d, g, 6_000).is_some(), "{:?}", sim.climb_stats());
    }

    #[test]
    fn smoothed_wall_stops_climbers() {
        let (mut sim, d, g, _) = walled(Wall::Smooth);
        assert!(reach(&mut sim, d, g, 4_000).is_none(), "{:?}", sim.climb_stats());
        assert_eq!(sim.climb_stats().moves, 0);
    }

    #[test]
    fn overhanging_floor_stops_climbers() {
        let (mut sim, d, g, _) = walled(Wall::Overhang);
        assert!(reach(&mut sim, d, g, 4_000).is_none(), "{:?}", sim.climb_stats());
    }

    #[test]
    fn one_tile_overhanging_wall_is_climbed_from_below() {
        let (mut sim, d, g, _) = walled(Wall::OverhangWall);
        assert!(reach(&mut sim, d, g, 8_000).is_some(), "{:?}", sim.climb_stats());
    }

    #[test]
    fn climb_moves_are_symmetric() {
        let (sim, _, _, sz) = walled(Wall::OverhangWall);
        let cls = crate::nav::climb_classes(&sim.world.tiletypes);
        let g = ClimbGraph { nav: &sim.nav, tiles: &sim.world.tiles, cls: &cls };
        let mut a = [(0u32, 0u32); crate::nav::CLIMB_MAXN];
        let mut b = [(0u32, 0u32); crate::nav::CLIMB_MAXN];
        let mut edges = 0;
        for z in sz - 1..sz + 5 {
            for y in 4..28i16 {
                for x in 4..28i16 {
                    let i = sim.world.idx(Pos::new(x, y, z)).unwrap();
                    let n = g.edges(i, &mut a);
                    for &(q, c) in &a[..n] {
                        let m = g.edges(q as usize, &mut b);
                        let back = b[..m].iter().find(|e| e.0 == i as u32);
                        assert!(back.is_some(), "move {:?} -> {:?} has no way back", sim.world.pos_of(i), sim.world.pos_of(q as usize));
                        let cb = back.unwrap().1;
                        assert_eq!(c / sim.nav.cost(q as usize), cb / sim.nav.cost(i), "factor of {:?} <-> {:?}", sim.world.pos_of(i), sim.world.pos_of(q as usize));
                        edges += 1;
                    }
                }
            }
        }
        assert!(edges > 1000);
    }
}
