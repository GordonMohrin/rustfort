//! [construct] Constructions, stone detailing, traffic and item designations (round 5).
//!
//! # DF rules implemented (wiki: Construction, Wall, Floor, Ramp, Stairs, Fortification, Track,
//! Smoothing, Engraving, Carve fortification, Traffic, Garbage dump, Forbid)
//! - **Constructions** ([`Sim::build_construction`]): every designated tile becomes a planned
//!   construction, a DF `Construction` building whose subtype is the DF `construction_type`
//!   (`Wall`, `Floor`, `Ramp`, `UpStair`, `DownStair`, `UpDownStair`, `Fortification`,
//!   `TrackNS`, ...), exactly as the exporter writes planned constructions. A dwarf with the
//!   Construction labor (`BUILD_CONSTRUCTION`) brings one building material and builds it from
//!   a neighbouring tile (floors and tracks also from the tile itself):
//!   - the material is the nearest free blocks, boulder of non-economic stone (no ore) or log;
//!   - the tile becomes the DF constructed tiletype (`ConstructedWall*`, `ConstructedFloor`,
//!     `ConstructedRamp`, `ConstructedStair*`, `ConstructedFortification`,
//!     `ConstructedFloorTrack*`) with the item's material; a ramp turns open space above into a
//!     ramp top;
//!   - DF 53 keeps the building material in the world with `flags.construction` (the export
//!     shows the blocks of every construction at its tile), so removing gives the item back.
//!   - Placement: walls, ramps, stairs and fortifications on a flat floor or open space, floors
//!     and tracks over open space; not hidden, no construction there yet, no building
//!     (stockpiles and zones are fine), no deep water or magma. Open space needs support: a
//!     solid tile below, or a wall or floor next to it (a planned construction next to it counts
//!     at designation time, so lines can be built out from an edge; at build time the support
//!     must exist, otherwise the job waits).
//!   - A creature on the tile suspends a wall or fortification (DF "construction interrupted");
//!     loose items on the tile are pushed onto the builder's tile.
//!   - Navigation updates incrementally (walls block, floors carry, stairs and ramps connect).
//! - **Remove construction**: planned constructions in the box are cancelled; finished ones get
//!   a removal job that restores the tile under it (recorded in `World::constructions`; for
//!   constructions from the export: a floor of the rock below, or open space) and drops the item.
//! - **Smoothing** (labor Engraving `ENGRAVER`, skill `ENGRAVE_STONE`): natural stone, mineral,
//!   obsidian, feature stone and ice walls and floors become the SMOOTH tiletypes. Smoothed and
//!   constructed walls connect to each other with DF's direction strings (`N-S-W-E-`). Soil and
//!   constructions cannot be smoothed.
//! - **Engraving**: only on smoothed natural tiles. The quality comes from the engraver's skill
//!   (DF item quality levels 0..5); engravings are kept in `World::engravings`, and
//!   [`Sim::engraving_value`] / [`Sim::engraving_value_in`] give their value for room value.
//! - **Carve fortification**: smoothed natural walls become a fortification of the same stone.
//! - **Carve track**: natural stone floors and constructed floors get `*FloorTrack*` tiletypes;
//!   the directions follow the dragged line (ends get one direction, like DF).
//! - Pending carve designations are kept in `World::carve_designations` (smooth/engrave ones in
//!   the tile flags), so they survive save/load.
//! - **Traffic** ([`Sim::set_traffic`]): the tile bits feed the A* costs (high 1, normal 2, low 5,
//!   restricted 25); changing them recomputes the navigation flags and clears the path cache.
//! - **Item designations** ([`Sim::set_item_flag`]): forbidden items are not hauled, eaten, used
//!   or dumped, and units already going for them give up; claim unforbids. Items marked for
//!   dumping are not stockpiled: dwarves with Refuse hauling carry them to the nearest Garbage
//!   dump zone (civzone `Dump`), where they are forbidden on arrival (as observed in the export:
//!   dumped items lie in the dump forbidden). Melt sets the flag; a "Melt a metal object" job
//!   at a smelter melts it (see `industry`).
//!
//! # Estimates (not on the wiki)
//! - Work times: construct 100 ticks with blocks, 150 with boulders, logs or bars; remove 100;
//!   smooth 150, engrave 300, carve fortification 250, carve track 150; scaled by skill like
//!   digging (`base * 10 / (10 + 2 * rating)`). Constructions use the material's skill
//!   (Masonry, Carpentry) for speed and experience; DF v50 has no construction skill.
//! - Removal uses the Construction labor too (DF 53 exports no separate removal labor).
//! - Engraving quality: `min(5, (s + rand(0..=s)) / 3)` for skill `s`; engraving value =
//!   10 x material value x DF quality multiplier (1, 2, 3, 4, 5, 12).
//! - Support counts all 8 neighbours on the same z-level.

use crate::nav::{DIRS8, TRAFFIC_COST, WALK};
use crate::path::Goals;
use crate::rng::Rng;
use crate::round5_api::{ConstructionKind, DetailKind, Traffic};
use crate::{labor, manhattan, Kind, Sim, Task, NONE};
use rf_core::{tile_flags, Building, Construction, Engraving, Item, Pos, Shape, TtMaterial, TtSpecial, NO_MAT};
use std::collections::HashMap;

const PH_FETCH: u8 = 0;
const PH_SITE: u8 = 1;
const PH_WORK: u8 = 2;

/// Direction bits (DF y grows to the south).
const DN: u8 = 1;
const DS: u8 = 2;
const DE: u8 = 4;
const DW: u8 = 8;

/// DF quality value multipliers (ordinary .. masterful).
pub const QUALITY_MULT: [i32; 6] = [1, 2, 3, 4, 5, 12];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CK {
    Build(ConstructionKind),
    Deconstruct,
    Smooth,
    Engrave,
    CarveFort,
    CarveTrack,
    Dump,
}

impl CK {
    fn labor(self) -> u64 {
        match self {
            CK::Build(_) | CK::Deconstruct => labor::BUILD_CONSTRUCTION,
            CK::Dump => labor::HAUL_REFUSE,
            _ => labor::ENGRAVER,
        }
    }
    fn solid(self) -> bool {
        matches!(self, CK::Build(ConstructionKind::Wall) | CK::Build(ConstructionKind::Fortification))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CJob {
    kind: CK,
    tile: u32,
    /// Planned construction building (Build), else -1.
    bid: i32,
    /// Building material (Build, once chosen) or the dumped item (Dump), else -1.
    item: i32,
    /// Track direction bits.
    track: u8,
    claimed: u32,
    blocked_until: u64,
    live: bool,
}

/// Counters for the UI and the bench.
#[derive(Clone, Debug, Default)]
pub struct ConstructReport {
    pub open_jobs: usize,
    pub planned_constructions: usize,
    pub built: u64,
    pub removed: u64,
    pub smoothed: u64,
    pub engraved: u64,
    pub fortifications: u64,
    pub tracks: u64,
    pub dumped: u64,
    /// Jobs suspended (creature in the way, no support yet).
    pub suspended: u64,
    /// Searches that found no building material.
    pub no_material: u64,
    /// Workers that could not reach an item or a site.
    pub path_failures: u64,
    pub engravings: usize,
    /// Items waiting for a garbage dump.
    pub dump_waiting: usize,
}

#[derive(Default)]
pub(crate) struct Cx {
    jobs: Vec<CJob>,
    free: Vec<u32>,
    /// Tile -> designation job (one per tile; dump jobs are keyed by item instead).
    at: HashMap<u32, u32>,
    dump_of: HashMap<i32, u32>,
    open: usize,
    epoch: u64,
    seen: Vec<u64>,
    next: Vec<u64>,
    phase: Vec<u8>,
    no_mat_until: u64,
    /// Walkable tiles of garbage dump zones (sorted).
    dump_tiles: Vec<u32>,
    dump_dirty: bool,
    known_buildings: usize,
    engr_at: HashMap<u32, u32>,
    /// (tiletype material, NSEW bits) -> smooth wall tiletype with that direction string.
    wall_dirs: HashMap<(TtMaterial, u8), u16>,
    /// (tiletype material, NSEW bits) -> floor track tiletype.
    track_dirs: HashMap<(TtMaterial, u8), u16>,
    stats: ConstructReport,
}

fn stone_like(m: TtMaterial) -> bool {
    matches!(m, TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::LavaStone | TtMaterial::Feature | TtMaterial::FrozenLiquid)
}

/// DF `construction_type` name for a planned construction building.
fn subtype_name(k: ConstructionKind, track: u8) -> String {
    match k {
        ConstructionKind::Wall => "Wall".into(),
        ConstructionKind::Floor => "Floor".into(),
        ConstructionKind::Ramp => "Ramp".into(),
        ConstructionKind::UpStair => "UpStair".into(),
        ConstructionKind::DownStair => "DownStair".into(),
        ConstructionKind::UpDownStair => "UpDownStair".into(),
        ConstructionKind::Fortification => "Fortification".into(),
        ConstructionKind::Track => format!("Track{}", dir_letters(track)),
        ConstructionKind::Remove => String::new(),
    }
}

fn kind_from_df(s: &str) -> Option<(ConstructionKind, u8)> {
    Some(match s {
        "Wall" => (ConstructionKind::Wall, 0),
        "Floor" => (ConstructionKind::Floor, 0),
        "Ramp" => (ConstructionKind::Ramp, 0),
        "UpStair" => (ConstructionKind::UpStair, 0),
        "DownStair" => (ConstructionKind::DownStair, 0),
        "UpDownStair" => (ConstructionKind::UpDownStair, 0),
        "Fortification" => (ConstructionKind::Fortification, 0),
        t if t.starts_with("Track") && !t.starts_with("TrackRamp") => (ConstructionKind::Track, dir_bits(&t[5..])),
        _ => return None,
    })
}

/// Track direction string in DF's order (`N`, `S`, `E`, `W`), e.g. `NSE`.
fn dir_letters(b: u8) -> String {
    let mut s = String::new();
    for (bit, c) in [(DN, 'N'), (DS, 'S'), (DE, 'E'), (DW, 'W')] {
        if b & bit != 0 {
            s.push(c);
        }
    }
    s
}

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

/// Bits of a single-line smooth wall direction string (`N-S-W-E-` layout), None for the
/// double-line variants.
fn wall_dir_bits(s: &str) -> Option<u8> {
    let b = s.as_bytes();
    if b.len() != 8 || b[1] != b'-' || b[3] != b'-' || b[5] != b'-' || b[7] != b'-' {
        return None;
    }
    let mut v = 0;
    if b[0] == b'N' {
        v |= DN;
    }
    if b[2] == b'S' {
        v |= DS;
    }
    if b[4] == b'W' {
        v |= DW;
    }
    if b[6] == b'E' {
        v |= DE;
    }
    Some(v)
}

/// Track directions of tile (x, y) inside a dragged box: connect to the neighbours in the box.
fn box_dirs(x: i16, y: i16, x0: i16, x1: i16, y0: i16, y1: i16) -> u8 {
    let mut d = 0;
    if y > y0 {
        d |= DN;
    }
    if y < y1 {
        d |= DS;
    }
    if x < x1 {
        d |= DE;
    }
    if x > x0 {
        d |= DW;
    }
    d
}

/// Engraving quality from the engraver's skill (estimate, see module docs).
pub(crate) fn roll_quality(rating: i32, r: &mut Rng) -> u8 {
    let s = rating.clamp(0, 20) as u32;
    let v = s + r.below(s + 1);
    (v / 3).min(5) as u8
}

impl Sim {
    // ------------------------------------------------------------------ init

    pub(crate) fn init_construct(&mut self) {
        self.cx.epoch = 1;
        self.cx.dump_dirty = true;
        for info in &self.world.tiletypes.infos {
            if info.shape == Shape::Wall && info.special == TtSpecial::Smooth {
                if let Some(b) = wall_dir_bits(&info.direction) {
                    self.cx.wall_dirs.entry((info.material, b)).or_insert(info.id);
                }
            }
            if info.shape == Shape::Floor && info.special == TtSpecial::Track && !info.direction.is_empty() {
                self.cx.track_dirs.entry((info.material, dir_bits(&info.direction))).or_insert(info.id);
            }
        }
        for (k, e) in self.world.engravings.iter().enumerate() {
            if let Some(i) = self.world.idx(e.pos) {
                self.cx.engr_at.insert(i as u32, k as u32);
            }
        }
        // Planned constructions (from the export or a save).
        let planned: Vec<(i32, Pos, String)> = self
            .world
            .buildings
            .iter()
            .filter(|b| b.kind == "Construction" && !b.built)
            .map(|b| (b.id, Pos::new(b.x1, b.y1, b.z), b.subtype.clone()))
            .collect();
        for (bid, p, sub) in planned {
            if let (Some((k, track)), Some(i)) = (kind_from_df(&sub), self.world.idx(p)) {
                if !self.cx.at.contains_key(&(i as u32)) {
                    self.cx_add(CK::Build(k), i as u32, bid, -1, track);
                }
            }
        }
        // Smooth / engrave designations kept in the tile flags.
        let mask = tile_flags::DESIG_SMOOTH | tile_flags::DESIG_ENGRAVE;
        for i in 0..self.world.tiles.len() {
            let f = self.world.tiles[i].flags;
            if f & mask == 0 {
                continue;
            }
            let k = if f & tile_flags::DESIG_SMOOTH != 0 { CK::Smooth } else { CK::Engrave };
            if self.cx_detail_ok(k, i) && !self.cx.at.contains_key(&(i as u32)) {
                self.cx_add(k, i as u32, -1, -1, 0);
            } else {
                self.world.tiles[i].flags &= !mask;
            }
        }
        // [audit-eco] Carve fortification / track designations saved in the world.
        let carve = std::mem::take(&mut self.world.carve_designations);
        for d in carve {
            let k = if d.track == 0 { CK::CarveFort } else { CK::CarveTrack };
            if let Some(i) = self.world.idx(d.pos) {
                if self.cx_detail_ok(k, i) && !self.cx.at.contains_key(&(i as u32)) {
                    self.cx_add(k, i as u32, -1, -1, d.track);
                }
            }
        }
        // Room values count engravings, which are indexed only now.
        self.social_revalue_rooms();
    }

    fn cx_ensure(&mut self, s: usize) {
        let n = self.ai.len().max(s + 1);
        if self.cx.seen.len() < n {
            self.cx.seen.resize(n, 0);
            self.cx.next.resize(n, 0);
            self.cx.phase.resize(n, 0);
        }
    }

    fn cx_add(&mut self, kind: CK, tile: u32, bid: i32, item: i32, track: u8) -> u32 {
        let job = CJob { kind, tile, bid, item, track, claimed: NONE, blocked_until: 0, live: true };
        let j = match self.cx.free.pop() {
            Some(j) => {
                self.cx.jobs[j as usize] = job;
                j
            }
            None => {
                self.cx.jobs.push(job);
                (self.cx.jobs.len() - 1) as u32
            }
        };
        if kind == CK::Dump {
            self.cx.dump_of.insert(item, j);
        } else {
            self.cx.at.insert(tile, j);
        }
        if matches!(kind, CK::CarveFort | CK::CarveTrack) {
            // [audit-eco] Kept in the world so that a save keeps the designation.
            let pos = self.world.pos_of(tile as usize);
            let track = if kind == CK::CarveTrack { track.max(1) } else { 0 };
            self.world.carve_designations.push(rf_core::CarveDesignation { pos, track });
        }
        self.cx.open += 1;
        self.cx.epoch += 1;
        j
    }

    /// Remove a job: cancels the worker, releases a reserved material, deletes a planned
    /// construction building and clears the tile designation.
    fn cx_remove_job(&mut self, j: usize) {
        let job = self.cx.jobs[j];
        if !job.live {
            return;
        }
        if job.claimed != NONE {
            let s = job.claimed as usize;
            if matches!(self.ai[s].task, Task::Construct(x) if x as usize == j) {
                self.cancel_task(s);
                self.ai[s].wake = self.ticks + 1;
            }
        }
        let job = self.cx.jobs[j];
        if let CK::Build(_) = job.kind {
            if job.item >= 0 {
                self.unreserve(job.item);
            }
        }
        self.cx.jobs[j].live = false;
        self.cx.jobs[j].claimed = NONE;
        if job.kind == CK::Dump {
            self.cx.dump_of.remove(&job.item);
        } else if self.cx.at.get(&job.tile) == Some(&(j as u32)) {
            self.cx.at.remove(&job.tile);
        }
        let t = &mut self.world.tiles[job.tile as usize];
        match job.kind {
            CK::Smooth => t.flags &= !tile_flags::DESIG_SMOOTH,
            CK::Engrave => t.flags &= !tile_flags::DESIG_ENGRAVE,
            CK::CarveFort | CK::CarveTrack => {
                let pos = self.world.pos_of(job.tile as usize);
                self.world.carve_designations.retain(|d| d.pos != pos);
            }
            _ => {}
        }
        if job.bid >= 0 {
            if let Some(bi) = self.world.buildings.iter().position(|b| b.id == job.bid) {
                self.world.buildings.remove(bi);
                // Production keeps building indices: refresh them.
                self.mark_prod_dirty();
                self.rebuild_prod_index();
            }
            self.cx.known_buildings = usize::MAX;
        }
        self.cx.free.push(j as u32);
        self.cx.open -= 1;
        self.cx.epoch += 1;
    }

    // ------------------------------------------------------------------ validity

    fn shape_at(&self, i: usize) -> Shape {
        self.nav.shape(self.world.tiles[i].tt)
    }

    /// Open space here has something to hold on to (`planned`: planned constructions count).
    fn cx_supported(&self, i: usize, planned: bool) -> bool {
        let (x, y, z) = self.nav.xyz(i);
        if z > 0 && self.shape_at(i - self.nav.layer).is_solid() {
            return true;
        }
        for &(ddx, ddy) in &DIRS8 {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let q = self.nav.idx(nx as usize, ny as usize, z);
            let sh = self.shape_at(q);
            if (sh.is_solid() && sh != Shape::None) || sh.has_floor() || self.nav.walkable(q) {
                return true;
            }
            if planned && self.cx.at.get(&(q as u32)).map(|&j| matches!(self.cx.jobs[j as usize].kind, CK::Build(_))).unwrap_or(false) {
                return true;
            }
        }
        false
    }

    fn cx_building_on(&self, p: Pos, own: i32) -> bool {
        self.world.buildings.iter().any(|b| b.id != own && b.kind != "Stockpile" && b.kind != "Civzone" && b.contains(p))
    }

    /// Can construction `k` go on tile `i` (`own` = the job's own planned building)?
    fn cx_can_build(&self, k: ConstructionKind, i: usize, own: i32, planned: bool) -> bool {
        let t = self.world.tiles[i];
        if t.hidden() || k == ConstructionKind::Remove {
            return false;
        }
        let info = self.world.tiletypes.get(t.tt);
        if info.material == TtMaterial::Construction {
            return false;
        }
        let open = match info.shape {
            Shape::Floor | Shape::Pebbles => false,
            Shape::Empty => true,
            _ => return false,
        };
        // DF: floors and tracks go over open space; on a floor one carves a track instead.
        if !open && matches!(k, ConstructionKind::Floor | ConstructionKind::Track) {
            return false;
        }
        if t.liquid_depth() >= 4 || (t.is_magma() && t.liquid_depth() > 0) {
            return false;
        }
        if open && !self.cx_supported(i, planned) {
            return false;
        }
        !self.cx_building_on(self.world.pos_of(i), own)
    }

    fn cx_detail_ok(&self, k: CK, i: usize) -> bool {
        let t = self.world.tiles[i];
        if t.hidden() {
            return false;
        }
        let info = self.world.tiletypes.get(t.tt);
        let (sh, m, sp) = (info.shape, info.material, info.special);
        let flat = matches!(sh, Shape::Floor | Shape::Pebbles);
        let tt = &self.world.tiletypes;
        match k {
            CK::Smooth => {
                stone_like(m)
                    && (sh == Shape::Wall || flat)
                    && sp != TtSpecial::Smooth
                    && sp != TtSpecial::Track
                    // The smoothed tiletype must exist for this material class.
                    && if sh == Shape::Wall {
                        self.cx.wall_dirs.keys().any(|&(wm, _)| wm == m) || tt.find(Shape::Wall, m, TtSpecial::Smooth, 0).map(|t| tt.get(t).special == TtSpecial::Smooth).unwrap_or(false)
                    } else {
                        tt.find(Shape::Floor, m, TtSpecial::Smooth, 0).map(|t| tt.get(t).special == TtSpecial::Smooth).unwrap_or(false)
                    }
            }
            CK::Engrave => stone_like(m) && matches!(sh, Shape::Wall | Shape::Floor) && sp == TtSpecial::Smooth && !self.cx.engr_at.contains_key(&(i as u32)),
            CK::CarveFort => {
                stone_like(m) && sh == Shape::Wall && sp == TtSpecial::Smooth && tt.find(Shape::Fortification, m, TtSpecial::None, 0).map(|t| tt.get(t).shape == Shape::Fortification).unwrap_or(false)
            }
            CK::CarveTrack => (stone_like(m) || m == TtMaterial::Construction) && flat && self.cx.track_dirs.keys().any(|&(tm, _)| tm == m),
            _ => false,
        }
    }

    fn unit_on(&self, tile: u32, except: usize) -> bool {
        self.ai.iter().enumerate().any(|(k, a)| k != except && a.kind != Kind::Dead && a.pos == tile)
    }

    // ------------------------------------------------------------------ public designations

    pub(crate) fn cx_build(&mut self, kind: ConstructionKind, a: Pos, b: Pos) -> usize {
        let (x0, x1, y0, y1, z) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y), a.z);
        if kind == ConstructionKind::Remove {
            return self.cx_designate_remove(x0, x1, y0, y1, z);
        }
        let mut next_id = self.world.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        let mut n = 0;
        // Closest to support first, so a line dragged out over open space is designated in order.
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = Pos::new(x, y, z);
                let Some(i) = self.world.idx(p) else { continue };
                if self.cx.at.contains_key(&(i as u32)) || !self.cx_can_build(kind, i, -1, true) {
                    continue;
                }
                let track = if kind == ConstructionKind::Track { box_dirs(x, y, x0, x1, y0, y1) } else { 0 };
                if kind == ConstructionKind::Track && track == 0 {
                    continue;
                }
                self.world.buildings.push(Building {
                    id: next_id,
                    kind: "Construction".into(),
                    subtype: subtype_name(kind, track),
                    x1: x,
                    y1: y,
                    x2: x,
                    y2: y,
                    z,
                    center: p,
                    mat: NO_MAT,
                    stage: 0,
                    built: false,
                    owner: -1,
                    ..Default::default()
                });
                self.cx_add(CK::Build(kind), i as u32, next_id, -1, track);
                next_id += 1;
                n += 1;
            }
        }
        n
    }

    fn cx_designate_remove(&mut self, x0: i16, x1: i16, y0: i16, y1: i16, z: i16) -> usize {
        let mut n = 0;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(i) = self.world.idx(Pos::new(x, y, z)) else { continue };
                if let Some(&j) = self.cx.at.get(&(i as u32)) {
                    if matches!(self.cx.jobs[j as usize].kind, CK::Build(_)) {
                        self.cx_remove_job(j as usize);
                        n += 1;
                    }
                    continue;
                }
                if self.world.tiletypes.get(self.world.tiles[i].tt).material == TtMaterial::Construction && !self.world.tiles[i].hidden() {
                    self.cx_add(CK::Deconstruct, i as u32, -1, -1, 0);
                    n += 1;
                }
            }
        }
        n
    }

    pub(crate) fn cx_detail(&mut self, a: Pos, b: Pos, kind: DetailKind) -> usize {
        let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        let mut n = 0;
        for z in a.z.min(b.z)..=a.z.max(b.z) {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let Some(i) = self.world.idx(Pos::new(x, y, z)) else { continue };
                    let existing = self.cx.at.get(&(i as u32)).copied();
                    let detail_job = existing.map(|j| matches!(self.cx.jobs[j as usize].kind, CK::Smooth | CK::Engrave | CK::CarveFort | CK::CarveTrack)).unwrap_or(false);
                    if kind == DetailKind::Remove {
                        if detail_job {
                            self.cx_remove_job(existing.unwrap() as usize);
                            n += 1;
                        }
                        continue;
                    }
                    let k = match kind {
                        DetailKind::Smooth => CK::Smooth,
                        DetailKind::Engrave => CK::Engrave,
                        DetailKind::CarveFortification => CK::CarveFort,
                        _ => CK::CarveTrack,
                    };
                    if existing.is_some() || !self.cx_detail_ok(k, i) {
                        continue;
                    }
                    let track = if k == CK::CarveTrack { box_dirs(x, y, x0, x1, y0, y1) } else { 0 };
                    if k == CK::CarveTrack && track == 0 {
                        continue;
                    }
                    match k {
                        CK::Smooth => self.world.tiles[i].flags |= tile_flags::DESIG_SMOOTH,
                        CK::Engrave => self.world.tiles[i].flags |= tile_flags::DESIG_ENGRAVE,
                        _ => {}
                    }
                    self.cx_add(k, i as u32, -1, -1, track);
                    n += 1;
                }
            }
        }
        n
    }

    pub(crate) fn cx_set_traffic(&mut self, a: Pos, b: Pos, level: Traffic) {
        let v: u16 = match level {
            Traffic::Normal => 0,
            Traffic::Low => 1,
            Traffic::High => 2,
            Traffic::Restricted => 3,
        };
        let mut changed = false;
        for z in a.z.min(b.z)..=a.z.max(b.z) {
            for y in a.y.min(b.y)..=a.y.max(b.y) {
                for x in a.x.min(b.x)..=a.x.max(b.x) {
                    let Some(i) = self.world.idx(Pos::new(x, y, z)) else { continue };
                    let t = &mut self.world.tiles[i];
                    let nf = (t.flags & !tile_flags::TRAFFIC_MASK) | (v << tile_flags::TRAFFIC_SHIFT);
                    if nf == t.flags {
                        continue;
                    }
                    t.flags = nf;
                    let f = self.nav.compute(&self.world.tiles, i);
                    self.nav.flags[i] = f;
                    if f & WALK != 0 {
                        let c = TRAFFIC_COST[(f >> crate::nav::TRAFFIC_SHIFT) as usize];
                        self.nav.min_cost = self.nav.min_cost.min(c);
                    }
                    changed = true;
                }
            }
        }
        if changed {
            self.path_cache.clear();
        }
    }

    pub(crate) fn cx_set_item_flag(&mut self, item_id: i32, flag: &str, on: bool) {
        let Some(&idx) = self.item_idx.get(&item_id) else { return };
        let idx = idx as usize;
        if self.imeta[idx].dead {
            return;
        }
        match flag {
            "forbid" => self.cx_forbid(idx, on),
            "claim" => self.cx_forbid(idx, !on),
            "dump" => {
                if self.world.items[idx].flags.dump == on {
                    return;
                }
                self.world.items[idx].flags.dump = on;
                if on {
                    self.cx_drop_users(item_id, false);
                    self.haul_remove(item_id);
                    self.cx.dump_dirty = true;
                } else {
                    if let Some(&j) = self.cx.dump_of.get(&item_id) {
                        self.cx_remove_job(j as usize);
                    }
                    if self.haulable(idx) && !self.is_stored(idx) && !self.imeta[idx].hauling {
                        self.haul_add(item_id);
                    }
                }
            }
            "melt" => self.world.items[idx].flags.melt = on,
            "hidden" => self.world.items[idx].flags.hidden = on,
            _ => {}
        }
    }

    fn cx_forbid(&mut self, idx: usize, on: bool) {
        let id = self.world.items[idx].id;
        if self.world.items[idx].flags.forbid == on {
            return;
        }
        if on {
            self.world.items[idx].flags.forbid = true;
            self.cx_drop_users(id, true);
            if let Some(&j) = self.cx.dump_of.get(&id) {
                self.cx_remove_job(j as usize);
            }
            self.haul_remove(id);
        } else {
            self.world.items[idx].flags.forbid = false;
            if self.haulable(idx) && !self.is_stored(idx) && !self.imeta[idx].hauling {
                self.haul_add(id);
            }
            if !self.drinks.contains(&id) && !self.foods.contains(&id) && !self.world.items[idx].flags.rotten && self.world.items[idx].holder < 0 {
                self.list_consumable(id);
            }
            if self.world.items[idx].flags.dump {
                self.cx.dump_dirty = true;
            }
        }
    }

    /// Units going for (or carrying) this item give up (forbidden or marked for dumping).
    fn cx_drop_users(&mut self, id: i32, all: bool) {
        let now = self.ticks;
        for s in 0..self.ai.len() {
            let task = self.ai[s].task;
            let uses = match task {
                Task::GoPickup(x, _) | Task::GoStore(x, _) => x == id,
                Task::Fetch(x) | Task::GoDrink(x) | Task::GoEat(x) | Task::GoDine(x, _) => all && x == id,
                Task::Construct(j) => {
                    let job = self.cx.jobs[j as usize];
                    all && job.item == id
                }
                _ => false,
            };
            if uses {
                self.cancel_task(s);
                self.ai[s].wake = now + 1;
            }
        }
        // A planned construction that picked this item as its material looks again.
        if all {
            for j in 0..self.cx.jobs.len() {
                let job = self.cx.jobs[j];
                if job.live && job.item == id && matches!(job.kind, CK::Build(_)) && job.claimed == NONE {
                    self.unreserve(id);
                    self.cx.jobs[j].item = -1;
                }
            }
        }
    }

    pub(crate) fn cx_set_items_flag_in(&mut self, a: Pos, b: Pos, flag: &str, on: bool) -> usize {
        let (lo, hi) = (Pos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)), Pos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)));
        let ids: Vec<i32> = self
            .world
            .items
            .iter()
            .enumerate()
            .filter(|(i, it)| {
                let p = it.pos;
                !self.imeta[*i].dead && p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y && p.z >= lo.z && p.z <= hi.z
            })
            .map(|(_, it)| it.id)
            .collect();
        for &id in &ids {
            self.cx_set_item_flag(id, flag, on);
        }
        ids.len()
    }

    // ------------------------------------------------------------------ garbage dumps

    fn cx_refresh_dumps(&mut self) {
        self.cx.dump_dirty = false;
        self.cx.known_buildings = self.world.buildings.len();
        let mut tiles = Vec::new();
        for b in &self.world.buildings {
            if b.kind != "Civzone" || !matches!(b.subtype.as_str(), "Dump" | "GarbageDump") {
                continue;
            }
            for y in b.y1..=b.y2 {
                for x in b.x1..=b.x2 {
                    let p = Pos::new(x, y, b.z);
                    if let Some(i) = self.world.idx(p) {
                        if b.contains(p) && self.nav.walkable(i) {
                            tiles.push(i as u32);
                        }
                    }
                }
            }
        }
        tiles.sort_unstable();
        tiles.dedup();
        self.cx.dump_tiles = tiles;
        if self.cx.dump_tiles.is_empty() {
            return;
        }
        let mut add = Vec::new();
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            if !it.flags.dump || !self.cx_dumpable(idx) || self.cx.dump_of.contains_key(&it.id) {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.cx.dump_tiles.binary_search(&(t as u32)).is_ok() {
                continue;
            }
            add.push((it.id, t as u32));
        }
        for (id, t) in add {
            self.cx_add(CK::Dump, t, -1, id, 0);
        }
    }

    fn cx_dumpable(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        let f = &it.flags;
        let m = &self.imeta[idx];
        !m.dead && !m.hauling && m.reserved == 0 && m.blocked_until <= self.ticks && !f.forbid && f.on_ground && !f.in_job && !f.construction && !f.in_building && it.container < 0 && it.holder < 0 && it.building < 0
    }

    fn cx_dump_goals(&self, c: u32, near: (usize, usize, usize)) -> Goals {
        let mut best = (i32::MAX, NONE);
        for &t in &self.cx.dump_tiles {
            if !self.nav.walkable(t as usize) || self.nav.comp_root(t as usize) != c {
                continue;
            }
            let d = manhattan(near, self.nav.xyz(t as usize));
            if (d, t) < best {
                best = (d, t);
            }
        }
        if best.1 == NONE {
            Goals::default()
        } else {
            Goals::one(best.1)
        }
    }

    // ------------------------------------------------------------------ job search

    /// Where a worker may stand to do the job, within component `c` (0 = any).
    fn cx_goals(&self, job: &CJob, c: u32) -> Goals {
        let mut g = Goals::default();
        let t = job.tile as usize;
        let ok = |q: usize| self.nav.walkable(q) && (c == 0 || self.nav.comp_root(q) == c);
        let flat = matches!(self.shape_at(t), Shape::Floor | Shape::Pebbles);
        let on_tile = match job.kind {
            CK::Build(k) => matches!(k, ConstructionKind::Floor | ConstructionKind::Track),
            CK::Smooth | CK::Engrave | CK::CarveTrack => flat,
            _ => false,
        };
        if on_tile && ok(t) {
            g.push(t as u32);
        }
        let (x, y, z) = self.nav.xyz(t);
        for &(ddx, ddy) in &DIRS8 {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let q = self.nav.idx(nx as usize, ny as usize, z);
            if ok(q) {
                g.push(q as u32);
            }
        }
        g
    }

    /// Item tile goals restricted to component `c`.
    fn cx_item_goals(&self, id: i32, c: u32) -> Goals {
        let g = self.item_goals(id);
        let mut out = Goals::default();
        for &t in g.as_slice() {
            if self.nav.comp_root(t as usize) == c {
                out.push(t);
            }
        }
        out
    }

    /// Nearest free building material for a construction at `site` (blocks, boulders of
    /// non-economic stone, logs).
    fn cx_find_material(&self, site: u32, c: u32, now: u64) -> Option<i32> {
        let sp = self.nav.xyz(site as usize);
        let mut best = (i32::MAX, i32::MAX);
        for idx in 0..self.world.items.len() {
            let it = &self.world.items[idx];
            let ok = match it.kind.as_str() {
                "BLOCKS" | "WOOD" => true,
                "BOULDER" => !self.world.material(it.mat).map(|m| m.is_ore || m.is_gem).unwrap_or(false),
                _ => false,
            };
            if !ok || it.flags.dump || it.flags.melt || !self.item_free(idx, now) {
                continue;
            }
            let Some(t) = self.world.idx(it.pos) else { continue };
            let d = manhattan(sp, self.nav.xyz(t));
            if (d, it.id) >= best || self.nav.comp_near(t) != c {
                continue;
            }
            best = (d, it.id);
        }
        (best.0 != i32::MAX).then_some(best.1)
    }

    /// [decide hook] Look for construction, detailing or dumping work.
    pub(crate) fn construct_try(&mut self, s: usize, now: u64) -> bool {
        if self.cx.dump_dirty || self.world.buildings.len() != self.cx.known_buildings {
            self.cx_refresh_dumps();
        }
        if self.cx.open == 0 {
            return false;
        }
        let labors = self.ai[s].labors;
        if labors & (labor::BUILD_CONSTRUCTION | labor::ENGRAVER | labor::HAUL_REFUSE) == 0 {
            return false;
        }
        self.cx_ensure(s);
        if self.cx.seen[s] == self.cx.epoch && now < self.cx.next[s] {
            return false;
        }
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        let up = self.nav.xyz(pos);
        let no_mat = now < self.cx.no_mat_until;
        let mut best = (i32::MAX, NONE);
        let mut best_goals = Goals::default();
        for j in 0..self.cx.jobs.len() {
            let job = &self.cx.jobs[j];
            if !job.live || job.claimed != NONE || job.blocked_until > now || labors & job.kind.labor() == 0 {
                continue;
            }
            if no_mat && matches!(job.kind, CK::Build(_)) && job.item < 0 {
                continue;
            }
            let target = if job.kind == CK::Dump {
                match self.item_idx.get(&job.item) {
                    Some(&ii) if self.cx_dumpable(ii as usize) => match self.world.idx(self.world.items[ii as usize].pos) {
                        Some(t) => t,
                        None => continue,
                    },
                    _ => continue,
                }
            } else {
                job.tile as usize
            };
            let d = manhattan(up, self.nav.xyz(target));
            if (d, j as u32) >= best {
                continue;
            }
            let g = if job.kind == CK::Dump {
                if self.cx_dump_goals(c, self.nav.xyz(target)).n == 0 {
                    continue;
                }
                self.cx_item_goals(job.item, c)
            } else {
                self.cx_goals(job, c)
            };
            if g.n == 0 {
                continue;
            }
            best = (d, j as u32);
            best_goals = g;
        }
        if best.1 == NONE {
            self.cx.seen[s] = self.cx.epoch;
            self.cx.next[s] = now + crate::RESEARCH_TICKS + (s as u64 % 64);
            return false;
        }
        let j = best.1 as usize;
        let job = self.cx.jobs[j];
        if let CK::Build(_) = job.kind {
            if job.item < 0 {
                match self.cx_find_material(job.tile, c, now) {
                    Some(id) => {
                        self.reserve(id);
                        self.cx.jobs[j].item = id;
                    }
                    None => {
                        self.cx.no_mat_until = now + rf_core::TICKS_PER_DAY as u64;
                        self.cx.stats.no_material += 1;
                        return self.construct_try(s, now);
                    }
                }
            }
        }
        if job.kind == CK::Dump {
            self.reserve(job.item);
        }
        let item = self.cx.jobs[j].item;
        self.cx.jobs[j].claimed = s as u32;
        self.ai[s].task = Task::Construct(j as u32);
        if item >= 0 {
            self.cx.phase[s] = PH_FETCH;
            let g = self.cx_item_goals(item, c);
            let g = if g.n == 0 { self.item_goals(item) } else { g };
            self.request_path(s, g, now);
        } else {
            self.cx.phase[s] = PH_SITE;
            self.request_path(s, best_goals, now);
        }
        true
    }

    // ------------------------------------------------------------------ task steps

    /// [think hook] Walk, work and finish a construction/detailing/dump job.
    pub(crate) fn construct_think(&mut self, s: usize, j: u32, now: u64, urgent: bool) {
        self.cx_ensure(s);
        if self.cx.phase[s] == PH_WORK {
            let until = self.ai[s].act_until;
            if now >= until {
                self.cx_complete(s, j as usize, now);
                if self.ai[s].task == Task::Idle {
                    self.decide(s, now);
                }
            } else if urgent {
                self.cancel_task(s);
                self.decide(s, now);
            } else {
                let cap = now + self.need_cap(s);
                self.ai[s].wake = until.min(cap);
            }
            return;
        }
        if urgent {
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
            self.cx_arrive(s, j as usize, now);
        }
    }

    fn cx_abandon(&mut self, s: usize, now: u64) {
        self.cancel_task(s);
        self.ai[s].wake = now + 1;
    }

    fn cx_arrive(&mut self, s: usize, j: usize, now: u64) {
        let job = self.cx.jobs[j];
        if !job.live || job.claimed != s as u32 {
            self.cx_abandon(s, now);
            return;
        }
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        if self.cx.phase[s] == PH_FETCH {
            let id = job.item;
            if !self.item_reachable_here(s, id) {
                if let Some(&i) = self.item_idx.get(&id) {
                    self.imeta[i as usize].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
                }
                self.cx.jobs[j].blocked_until = now + 100;
                self.cx_abandon(s, now);
                return;
            }
            let Some(carried) = self.cx_take(s, id, job.kind == CK::Dump) else {
                self.cx_abandon(s, now);
                return;
            };
            self.cx.jobs[j].item = carried;
            self.cx.phase[s] = PH_SITE;
            let g = if job.kind == CK::Dump { self.cx_dump_goals(c, self.nav.xyz(self.ai[s].pos as usize)) } else { self.cx_goals(&job, c) };
            if g.n == 0 {
                self.cx.jobs[j].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
                self.cx_abandon(s, now);
                return;
            }
            self.request_path(s, g, now);
            return;
        }
        // At the site.
        if job.kind == CK::Dump {
            self.cx_dump_drop(s, j, now);
            return;
        }
        if !self.cx_job_valid(j) {
            self.cx_remove_job(j);
            self.ai[s].task = Task::Idle;
            self.ai[s].wake = now + 1;
            return;
        }
        if let CK::Build(_) = job.kind {
            let tile = job.tile as usize;
            let blocked = (job.kind.solid() && self.unit_on(job.tile, s)) || (self.shape_at(tile) == Shape::Empty && !self.cx_supported(tile, false));
            if blocked {
                self.cx.stats.suspended += 1;
                self.cx.jobs[j].blocked_until = now + 100;
                self.cx_abandon(s, now);
                return;
            }
        }
        let (base, skill) = self.cx_work(&job);
        let rating = self.social_rating(s, self.world.units[self.ai[s].uidx].skill(skill)); // [social] focus
        let dur = (base * 10 / (10 + 2 * rating.clamp(0, 20))).max(10) as u64;
        let cap = self.need_cap(s);
        self.cx.phase[s] = PH_WORK;
        let ai = &mut self.ai[s];
        ai.act_until = now + dur;
        ai.wake = (now + dur).min(now + cap);
    }

    /// Base work ticks and skill of a job.
    fn cx_work(&self, job: &CJob) -> (i32, &'static str) {
        match job.kind {
            CK::Build(_) => {
                let (kind, mat) = match self.item_idx.get(&job.item) {
                    Some(&i) => (self.world.items[i as usize].kind.as_str(), self.world.items[i as usize].mat),
                    None => ("", NO_MAT),
                };
                let skill = if kind == "WOOD" || self.world.material(mat).map(|m| m.is_wood).unwrap_or(false) { "CARPENTRY" } else { "MASONRY" };
                (if kind == "BLOCKS" { 100 } else { 150 }, skill)
            }
            CK::Deconstruct => (100, "MASONRY"),
            CK::Smooth => (150, "ENGRAVE_STONE"),
            CK::Engrave => (300, "ENGRAVE_STONE"),
            CK::CarveFort => (250, "ENGRAVE_STONE"),
            CK::CarveTrack => (150, "ENGRAVE_STONE"),
            CK::Dump => (10, ""),
        }
    }

    fn cx_job_valid(&self, j: usize) -> bool {
        let job = self.cx.jobs[j];
        let i = job.tile as usize;
        match job.kind {
            CK::Build(k) => self.cx_can_build(k, i, job.bid, false) || (self.shape_at(i) == Shape::Empty && self.cx_can_build(k, i, job.bid, true)),
            CK::Deconstruct => self.world.tiletypes.get(self.world.tiles[i].tt).material == TtMaterial::Construction,
            CK::Dump => true,
            k => self.cx_detail_ok(k, i),
        }
    }

    /// Pick up an item (one unit of a stack unless `whole`). Returns the carried item id.
    fn cx_take(&mut self, s: usize, id: i32, whole: bool) -> Option<i32> {
        let idx = *self.item_idx.get(&id)? as usize;
        self.imeta[idx].reserved = (self.imeta[idx].reserved - 1).max(0);
        let stack = self.world.items[idx].stack;
        if stack <= 0 || self.imeta[idx].dead {
            return None;
        }
        let uid = self.ai[s].id;
        let here = self.world.pos_of(self.ai[s].pos as usize);
        let carried = if !whole && stack > 1 {
            self.world.items[idx].stack -= 1;
            let mut it: Item = self.world.items[idx].clone();
            it.id = self.next_item_id;
            self.next_item_id += 1;
            it.stack = 1;
            it.container = -1;
            it.holder = uid;
            it.flags.on_ground = false;
            it.flags.in_container = false;
            it.flags.in_inventory = true;
            it.pos = here;
            let nid = it.id;
            self.push_item(it);
            nid
        } else {
            let it = &mut self.world.items[idx];
            let (cont, pos, loose) = (it.container, it.pos, it.container < 0 && it.flags.on_ground);
            it.container = -1;
            it.flags.in_container = false;
            it.flags.on_ground = false;
            it.flags.in_inventory = true;
            it.holder = uid;
            if cont >= 0 {
                if let Some(v) = self.contents.get_mut(&cont) {
                    v.retain(|&x| x != id);
                }
            } else if loose {
                if let Some(t) = self.world.idx(pos) {
                    self.pile_occ_add(t as u32, -1);
                }
            }
            self.haul_remove(id);
            id
        };
        if let Some(&ci) = self.item_idx.get(&carried) {
            self.imeta[ci as usize].hauling = true;
        }
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.push(carried);
        self.ai[s].carrying = carried;
        Some(carried)
    }

    /// Put the carried item down on the unit's tile (loose, free again).
    fn cx_put_down(&mut self, s: usize, forbid: bool) {
        let id = self.ai[s].carrying;
        if id < 0 {
            return;
        }
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != id);
        self.ai[s].carrying = -1;
        let tile = self.ai[s].pos;
        let p = self.world.pos_of(tile as usize);
        self.set_item_pos(id, p);
        let Some(&i) = self.item_idx.get(&id) else { return };
        let i = i as usize;
        let it = &mut self.world.items[i];
        it.flags.on_ground = true;
        it.flags.in_inventory = false;
        it.holder = -1;
        if forbid {
            it.flags.forbid = true;
        }
        self.imeta[i].hauling = false;
        self.imeta[i].reserved = 0;
        self.pile_occ_add(tile, 1);
        if !forbid && self.haulable(i) && !self.is_stored(i) {
            self.haul_add(id);
        }
    }

    /// [cancel hook] Need interrupt, path failure, death or a removed job.
    pub(crate) fn construct_cancel(&mut self, s: usize, j: u32) {
        let carrying = self.ai[s].carrying;
        self.cx_put_down(s, false);
        if let Some(job) = self.cx.jobs.get(j as usize).copied() {
            if job.live && job.claimed == s as u32 {
                self.cx.jobs[j as usize].claimed = NONE;
                match job.kind {
                    CK::Build(_) => {
                        if job.item >= 0 && job.item != carrying {
                            self.unreserve(job.item);
                        }
                        self.cx.jobs[j as usize].item = -1;
                    }
                    CK::Dump => {
                        if job.item != carrying {
                            self.unreserve(job.item);
                        }
                    }
                    _ => {}
                }
                self.cx.epoch += 1;
            }
        }
        self.cx_ensure(s);
        self.cx.phase[s] = PH_FETCH;
    }

    /// [path_failed hook] The worker could not reach the item or the site.
    pub(crate) fn construct_path_failed(&mut self, s: usize, j: u32, now: u64) {
        self.cx_ensure(s);
        self.cx.stats.path_failures += 1;
        if let Some(job) = self.cx.jobs.get(j as usize).copied() {
            if job.live {
                let fetch = self.cx.phase[s] == PH_FETCH && job.item >= 0;
                if fetch {
                    if let Some(&i) = self.item_idx.get(&job.item) {
                        self.imeta[i as usize].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
                    }
                }
                // A construction looks for other material at once; everything else waits a day.
                if !(fetch && matches!(job.kind, CK::Build(_))) {
                    self.cx.jobs[j as usize].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
                }
            }
        }
        self.cancel_task(s);
    }

    /// [unit_status hook] DF job caption and target.
    pub(crate) fn construct_caption(&self, j: u32) -> (&'static str, Option<Pos>) {
        let Some(job) = self.cx.jobs.get(j as usize) else { return ("No Job", None) };
        let wall = self.shape_at(job.tile as usize) == Shape::Wall;
        let cap = match job.kind {
            CK::Build(k) => match k {
                ConstructionKind::Wall => "Construct Wall",
                ConstructionKind::Floor => "Construct Floor",
                ConstructionKind::Ramp => "Construct Ramp",
                ConstructionKind::UpStair => "Construct Up Stair",
                ConstructionKind::DownStair => "Construct Down Stair",
                ConstructionKind::UpDownStair => "Construct Up/Down Stair",
                ConstructionKind::Fortification => "Construct Fortification",
                ConstructionKind::Track => "Construct Track",
                ConstructionKind::Remove => "Remove Construction",
            },
            CK::Deconstruct => "Remove Construction",
            CK::Smooth => {
                if wall {
                    "Smooth Wall"
                } else {
                    "Smooth Floor"
                }
            }
            CK::Engrave => {
                if wall {
                    "Detail Wall"
                } else {
                    "Detail Floor"
                }
            }
            CK::CarveFort => "Carve Fortification",
            CK::CarveTrack => "Carve Track",
            CK::Dump => "Dump Item",
        };
        (cap, Some(self.world.pos_of(job.tile as usize)))
    }

    // ------------------------------------------------------------------ completion

    fn cx_complete(&mut self, s: usize, j: usize, now: u64) {
        let job = self.cx.jobs[j];
        self.ai[s].task = Task::Idle;
        self.cx.phase[s] = PH_FETCH;
        if !job.live || job.claimed != s as u32 {
            self.cx_put_down(s, false);
            return;
        }
        if !self.cx_job_valid(j) {
            self.cx.jobs[j].claimed = NONE;
            self.cx_put_down(s, false);
            self.cx.jobs[j].item = -1;
            self.cx_remove_job(j);
            return;
        }
        let (_, skill) = self.cx_work(&job);
        let done = match job.kind {
            CK::Build(k) => self.cx_finish_build(s, j, k, now),
            CK::Deconstruct => self.cx_finish_remove(s, job.tile as usize),
            CK::Smooth => self.cx_finish_smooth(job.tile as usize),
            CK::Engrave => self.cx_finish_engrave(s, job.tile as usize, now),
            CK::CarveFort => self.cx_finish_fort(job.tile as usize),
            CK::CarveTrack => self.cx_finish_track(job.tile as usize, job.track),
            CK::Dump => true,
        };
        if !done {
            // Suspended (creature in the way, no tiletype): the job waits, the worker moves on.
            self.cx.jobs[j].claimed = NONE;
            self.cx.jobs[j].blocked_until = now + 100;
            self.cx.stats.suspended += 1;
            self.cx.epoch += 1;
            if let CK::Build(_) = job.kind {
                self.cx_put_down(s, false);
                self.cx.jobs[j].item = -1;
            }
            return;
        }
        if !skill.is_empty() {
            self.add_xp(s, skill, if matches!(job.kind, CK::Build(_) | CK::Deconstruct) { 20 } else { 30 });
        }
        self.stats.jobs_done += 1;
        self.cx.jobs[j].claimed = NONE;
        self.cx.jobs[j].item = -1; // the material is part of the construction now
        self.cx_remove_job(j);
    }

    /// Lookup for a smooth wall (natural or constructed) connected to its neighbours.
    fn cx_wall_tt(&self, m: TtMaterial, i: usize) -> Option<u16> {
        let (x, y, z) = self.nav.xyz(i);
        let mut d = 0u8;
        for (bit, ddx, ddy) in [(DN, 0isize, -1isize), (DS, 0, 1), (DE, 1, 0), (DW, -1, 0)] {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let info = self.world.tiletypes.get(self.world.tiles[self.nav.idx(nx as usize, ny as usize, z)].tt);
            if info.shape == Shape::Wall && (info.special == TtSpecial::Smooth || info.material == TtMaterial::Construction) && (stone_like(info.material) || info.material == TtMaterial::Construction) {
                d |= bit;
            }
        }
        if d == DN || d == DS {
            d = DN | DS;
        }
        if d == DE || d == DW {
            d = DE | DW;
        }
        let tt = &self.world.tiletypes;
        self.cx.wall_dirs.get(&(m, d)).copied().or_else(|| tt.find(Shape::Wall, m, TtSpecial::Smooth, 0)).filter(|&id| {
            let inf = tt.get(id);
            inf.shape == Shape::Wall && inf.material == m
        })
    }

    /// Re-connect smoothed/constructed walls around `i` after it changed.
    fn cx_refresh_wall_dirs(&mut self, i: usize) {
        let (x, y, z) = self.nav.xyz(i);
        for (ddx, ddy) in [(0isize, -1isize), (0, 1), (1, 0), (-1, 0)] {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let q = self.nav.idx(nx as usize, ny as usize, z);
            let info = self.world.tiletypes.get(self.world.tiles[q].tt);
            let m = info.material;
            if info.shape != Shape::Wall || info.special != TtSpecial::Smooth || !(stone_like(m) || m == TtMaterial::Construction) {
                continue;
            }
            if let Some(tt) = self.cx_wall_tt(m, q) {
                self.world.tiles[q].tt = tt;
            }
        }
    }

    fn cx_drop_engraving(&mut self, i: usize) {
        if let Some(k) = self.cx.engr_at.remove(&(i as u32)) {
            let k = k as usize;
            self.world.engravings.swap_remove(k);
            if k < self.world.engravings.len() {
                if let Some(t) = self.world.idx(self.world.engravings[k].pos) {
                    self.cx.engr_at.insert(t as u32, k as u32);
                }
            }
        }
    }

    fn cx_finish_build(&mut self, s: usize, j: usize, k: ConstructionKind, _now: u64) -> bool {
        let job = self.cx.jobs[j];
        let i = job.tile as usize;
        let id = self.ai[s].carrying;
        if id < 0 || id != job.item {
            return false;
        }
        if job.kind.solid() && self.unit_on(job.tile, s) {
            return false;
        }
        if self.shape_at(i) == Shape::Empty && !self.cx_supported(i, false) {
            return false;
        }
        let Some(&ii) = self.item_idx.get(&id) else { return false };
        let (mat, item_kind) = (self.world.items[ii as usize].mat, self.world.items[ii as usize].kind.clone());
        let tt = &self.world.tiletypes;
        let c = TtMaterial::Construction;
        let new_tt = match k {
            ConstructionKind::Wall => self.cx_wall_tt(c, i).or_else(|| tt.find(Shape::Wall, c, TtSpecial::Normal, 0)),
            ConstructionKind::Floor => tt.find(Shape::Floor, c, TtSpecial::Smooth, 0),
            ConstructionKind::Ramp => tt.find(Shape::Ramp, c, TtSpecial::None, 0),
            ConstructionKind::UpStair => tt.find(Shape::StairUp, c, TtSpecial::None, 0),
            ConstructionKind::DownStair => tt.find(Shape::StairDown, c, TtSpecial::None, 0),
            ConstructionKind::UpDownStair => tt.find(Shape::StairUpdown, c, TtSpecial::None, 0),
            ConstructionKind::Fortification => tt.find(Shape::Fortification, c, TtSpecial::None, 0),
            ConstructionKind::Track => self.cx.track_dirs.get(&(c, job.track)).copied().or_else(|| tt.find(Shape::Floor, c, TtSpecial::Smooth, 0)),
            ConstructionKind::Remove => None,
        };
        let Some(new_tt) = new_tt.filter(|&t| tt.get(t).material == c) else { return false };
        let p = self.world.pos_of(i);
        let here = self.ai[s].pos;
        // Loose items are pushed off a tile that becomes solid.
        if job.kind.solid() {
            let ids: Vec<i32> = self
                .world
                .items
                .iter()
                .enumerate()
                .filter(|(x, it)| it.pos == p && it.flags.on_ground && it.container < 0 && it.holder < 0 && !it.flags.construction && !self.imeta[*x].dead)
                .map(|(_, it)| it.id)
                .collect();
            let hp = self.world.pos_of(here as usize);
            for iid in ids {
                self.set_item_pos(iid, hp);
                self.pile_occ_add(i as u32, -1);
                self.pile_occ_add(here, 1);
            }
        }
        let orig = self.world.tiles[i];
        {
            let t = &mut self.world.tiles[i];
            t.tt = new_tt;
            t.mat = mat;
            t.desig = 0;
            t.flags &= !(tile_flags::DESIG_SMOOTH | tile_flags::DESIG_ENGRAVE);
            t.flags |= tile_flags::DIRTY;
        }
        self.cx_drop_engraving(i);
        // The building material becomes part of the construction (DF 53 keeps the item).
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != id);
        self.ai[s].carrying = -1;
        self.set_item_pos(id, p);
        {
            let it = &mut self.world.items[ii as usize];
            it.flags.on_ground = false;
            it.flags.in_inventory = false;
            it.flags.in_job = false;
            it.flags.construction = true;
            it.holder = -1;
        }
        self.imeta[ii as usize].reserved = 0;
        self.imeta[ii as usize].hauling = false;
        self.haul_remove(id);
        self.world.constructions.push(Construction { pos: p, item: id, item_kind, mat, orig_tt: orig.tt, orig_mat: orig.mat });
        let layer = self.nav.layer;
        let mut above_changed = false;
        if k == ConstructionKind::Ramp && i + layer < self.world.tiles.len() && self.shape_at(i + layer) == Shape::Empty {
            if let Some(rt) = self.world.tiletypes.find(Shape::RampTop, TtMaterial::Air, TtSpecial::Normal, 0) {
                self.world.tiles[i + layer].tt = rt;
                self.world.tiles[i + layer].mat = NO_MAT;
                above_changed = true;
            }
        }
        self.after_tile_change(i);
        if above_changed {
            self.after_tile_change(i + layer);
        }
        if k == ConstructionKind::Wall {
            self.cx_refresh_wall_dirs(i);
        }
        self.cx.stats.built += 1;
        true
    }

    fn cx_finish_remove(&mut self, s: usize, i: usize) -> bool {
        let p = self.world.pos_of(i);
        let layer = self.nav.layer;
        let was_ramp = self.shape_at(i) == Shape::Ramp;
        let rec = self.world.constructions.iter().rposition(|c| c.pos == p);
        let rec = rec.map(|k| self.world.constructions.remove(k));
        let (mat, _) = (self.world.tiles[i].mat, ());
        match &rec {
            Some(r) => {
                let t = &mut self.world.tiles[i];
                t.tt = r.orig_tt;
                t.mat = r.orig_mat;
                t.flags |= tile_flags::DIRTY;
            }
            None => {
                // Exported construction: a floor of the rock below, else open space.
                let (_, _, z) = self.nav.xyz(i);
                let below = if z > 0 { Some(self.world.tiles[i - layer]) } else { None };
                let ok = below.map(|b| {
                    let inf = self.world.tiletypes.get(b.tt);
                    inf.shape.is_solid() && inf.material.is_diggable()
                });
                if ok == Some(true) {
                    let b = below.unwrap();
                    let class = crate::dig_class(self.world.tiletypes.get(b.tt).material);
                    if !self.set_tile(i, Shape::Floor, class, 1, b.mat) {
                        self.set_tile(i, Shape::Empty, TtMaterial::Air, 0, NO_MAT);
                    }
                } else {
                    self.set_tile(i, Shape::Empty, TtMaterial::Air, 0, NO_MAT);
                }
            }
        }
        let mut above_changed = false;
        if was_ramp && i + layer < self.world.tiles.len() && self.shape_at(i + layer) == Shape::RampTop {
            self.set_tile(i + layer, Shape::Empty, TtMaterial::Air, 0, NO_MAT);
            above_changed = true;
        }
        self.after_tile_change(i);
        if above_changed {
            self.after_tile_change(i + layer);
        }
        self.cx_refresh_wall_dirs(i);
        // Give the building material back.
        let drop = if self.nav.walkable(i) { i as u32 } else { self.ai[s].pos };
        let dp = self.world.pos_of(drop as usize);
        let item = rec.as_ref().map(|r| r.item).filter(|&id| self.item_idx.get(&id).map(|&x| self.world.items[x as usize].flags.construction).unwrap_or(false));
        let item = item.or_else(|| self.world.items.iter().enumerate().find(|(x, it)| it.pos == p && it.flags.construction && !self.imeta[*x].dead).map(|(_, it)| it.id));
        match item {
            Some(id) => {
                let ii = self.item_idx[&id] as usize;
                self.set_item_pos(id, dp);
                let it = &mut self.world.items[ii];
                it.flags.construction = false;
                it.flags.on_ground = true;
                self.pile_occ_add(drop, 1);
                if self.haulable(ii) && !self.is_stored(ii) {
                    self.haul_add(id);
                }
            }
            None => {
                let m = self.world.material(mat);
                let (kind, noun) = match m {
                    Some(m) if m.is_wood => ("WOOD", "log"),
                    Some(m) if m.is_metal => ("BAR", "bar"),
                    Some(m) if m.is_stone => ("BOULDER", "boulder"),
                    _ => ("BLOCKS", "blocks"),
                };
                let name = m.map(|m| m.name.clone()).unwrap_or_default();
                self.create_item(kind, mat, drop as usize, format!("{name} {noun}"));
            }
        }
        self.cx.stats.removed += 1;
        true
    }

    fn cx_finish_smooth(&mut self, i: usize) -> bool {
        let info = self.world.tiletypes.get(self.world.tiles[i].tt);
        let (sh, m) = (info.shape, info.material);
        let new = if sh == Shape::Wall { self.cx_wall_tt(m, i) } else { self.world.tiletypes.find(Shape::Floor, m, TtSpecial::Smooth, 0) };
        let Some(new) = new.filter(|&t| self.world.tiletypes.get(t).special == TtSpecial::Smooth) else { return false };
        let t = &mut self.world.tiles[i];
        t.tt = new;
        t.flags &= !tile_flags::DESIG_SMOOTH;
        t.flags |= tile_flags::DIRTY;
        if sh == Shape::Wall {
            self.cx_refresh_wall_dirs(i);
        } else if sh != Shape::Floor {
            self.after_tile_change(i);
        }
        self.cx.stats.smoothed += 1;
        true
    }

    fn cx_finish_engrave(&mut self, s: usize, i: usize, now: u64) -> bool {
        let rating = self.social_rating(s, self.world.units[self.ai[s].uidx].skill("ENGRAVE_STONE")); // [social] focus
        let mut r = Rng::for_unit(self.seed, s, now, 51);
        let quality = roll_quality(rating, &mut r);
        let t = self.world.tiles[i];
        let floor = self.world.tiletypes.get(t.tt).shape != Shape::Wall;
        let e = Engraving { pos: self.world.pos_of(i), quality, floor, mat: t.mat, engraver: self.ai[s].id, skill: rating };
        self.world.engravings.push(e);
        self.cx.engr_at.insert(i as u32, (self.world.engravings.len() - 1) as u32);
        self.world.tiles[i].flags &= !tile_flags::DESIG_ENGRAVE;
        self.cx.stats.engraved += 1;
        true
    }

    fn cx_finish_fort(&mut self, i: usize) -> bool {
        let m = self.world.tiletypes.get(self.world.tiles[i].tt).material;
        let Some(new) = self.world.tiletypes.find(Shape::Fortification, m, TtSpecial::None, 0).filter(|&t| self.world.tiletypes.get(t).shape == Shape::Fortification) else {
            return false;
        };
        self.world.tiles[i].tt = new;
        self.world.tiles[i].flags |= tile_flags::DIRTY;
        self.cx_drop_engraving(i);
        self.after_tile_change(i);
        self.cx_refresh_wall_dirs(i);
        self.cx.stats.fortifications += 1;
        true
    }

    fn cx_finish_track(&mut self, i: usize, bits: u8) -> bool {
        let info = self.world.tiletypes.get(self.world.tiles[i].tt);
        let m = info.material;
        let old = if info.special == TtSpecial::Track { dir_bits(&info.direction) } else { 0 };
        let Some(&new) = self.cx.track_dirs.get(&(m, bits | old)) else { return false };
        self.world.tiles[i].tt = new;
        self.world.tiles[i].flags |= tile_flags::DIRTY;
        if info.shape != Shape::Floor {
            self.after_tile_change(i);
        }
        self.cx.stats.tracks += 1;
        true
    }

    fn cx_dump_drop(&mut self, s: usize, j: usize, now: u64) {
        let here = self.ai[s].pos;
        if self.cx.dump_tiles.binary_search(&here).is_err() || self.ai[s].carrying < 0 {
            self.cx_abandon(s, now);
            return;
        }
        // DF: dumped items lie forbidden in the dump.
        self.cx_put_down(s, true);
        self.cx.jobs[j].claimed = NONE;
        self.cx_remove_job(j);
        self.cx.stats.dumped += 1;
        self.stats.items_hauled += 1;
        self.ai[s].task = Task::Idle;
        self.decide(s, now);
    }

    // ------------------------------------------------------------------ readouts

    /// Value of an engraving (estimate: 10 x material value x quality multiplier).
    pub fn engraving_value(&self, e: &Engraving) -> i32 {
        let mv = self.world.material(e.mat).map(|m| m.value).unwrap_or(1).max(1);
        10 * mv * QUALITY_MULT[e.quality.min(5) as usize]
    }

    /// Changes with every finished construction, removal, smoothing, engraving or carving
    /// (room values are recomputed when it moves).
    pub(crate) fn detail_epoch(&self) -> u64 {
        let st = &self.cx.stats;
        st.built + st.removed + st.smoothed + st.engraved + st.fortifications + st.tracks
    }

    /// Engraving at a tile, if any.
    pub fn engraving_at(&self, p: Pos) -> Option<&Engraving> {
        let i = self.world.idx(p)?;
        self.cx.engr_at.get(&(i as u32)).and_then(|&k| self.world.engravings.get(k as usize))
    }

    /// Sum of engraving values inside the inclusive rectangle on level `z` (for room value).
    pub fn engraving_value_in(&self, x1: i16, y1: i16, x2: i16, y2: i16, z: i16) -> i32 {
        self.world
            .engravings
            .iter()
            .filter(|e| e.pos.z == z && e.pos.x >= x1.min(x2) && e.pos.x <= x1.max(x2) && e.pos.y >= y1.min(y2) && e.pos.y <= y1.max(y2))
            .map(|e| self.engraving_value(e))
            .sum()
    }

    /// Open construction / detailing / dump jobs: (target, DF caption, claiming unit id).
    pub fn construction_jobs(&self) -> Vec<(Pos, &'static str, Option<i32>)> {
        (0..self.cx.jobs.len())
            .filter(|&j| self.cx.jobs[j].live)
            .map(|j| {
                let job = &self.cx.jobs[j];
                let (cap, pos) = self.construct_caption(j as u32);
                let who = if job.claimed == NONE { None } else { Some(self.ai[job.claimed as usize].id) };
                (pos.unwrap_or_default(), cap, who)
            })
            .collect()
    }

    pub fn construct_report(&self) -> ConstructReport {
        let mut r = self.cx.stats.clone();
        r.open_jobs = self.cx.open;
        r.planned_constructions = self.cx.jobs.iter().filter(|j| j.live && matches!(j.kind, CK::Build(_))).count();
        r.engravings = self.world.engravings.len();
        r.dump_waiting = self.cx.jobs.iter().filter(|j| j.live && j.kind == CK::Dump).count();
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::round5_api::{ConstructionKind as CKind, DetailKind, Traffic};
    use rf_core::{Building, Item, ItemFlags, TiletypeInfo, World};

    /// The synthetic world plus the DF tiletypes this module needs (DF names and attributes).
    fn world() -> World {
        let mut w = World::synthetic(48, 48, 20);
        let add: &[(&str, Shape, TtMaterial, TtSpecial, &str)] = &[
            ("ConstructedPillar", Shape::Wall, TtMaterial::Construction, TtSpecial::Smooth, ""),
            ("ConstructedWallLR", Shape::Wall, TtMaterial::Construction, TtSpecial::Smooth, "----W-E-"),
            ("ConstructedWallUD", Shape::Wall, TtMaterial::Construction, TtSpecial::Smooth, "N-S-----"),
            ("ConstructedStairUD", Shape::StairUpdown, TtMaterial::Construction, TtSpecial::None, ""),
            ("ConstructedStairD", Shape::StairDown, TtMaterial::Construction, TtSpecial::None, ""),
            ("ConstructedStairU", Shape::StairUp, TtMaterial::Construction, TtSpecial::None, ""),
            ("ConstructedRamp", Shape::Ramp, TtMaterial::Construction, TtSpecial::None, ""),
            ("ConstructedFortification", Shape::Fortification, TtMaterial::Construction, TtSpecial::None, ""),
            ("StonePillar", Shape::Wall, TtMaterial::Stone, TtSpecial::Smooth, ""),
            ("StoneWallSmoothLR", Shape::Wall, TtMaterial::Stone, TtSpecial::Smooth, "----W-E-"),
            ("StoneWallSmoothUD", Shape::Wall, TtMaterial::Stone, TtSpecial::Smooth, "N-S-----"),
            ("StoneWallSmoothLRUD", Shape::Wall, TtMaterial::Stone, TtSpecial::Smooth, "N-S-W-E-"),
            ("StoneFortification", Shape::Fortification, TtMaterial::Stone, TtSpecial::None, ""),
            ("StoneFloorTrackEW", Shape::Floor, TtMaterial::Stone, TtSpecial::Track, "EW"),
            ("StoneFloorTrackE", Shape::Floor, TtMaterial::Stone, TtSpecial::Track, "E"),
            ("StoneFloorTrackW", Shape::Floor, TtMaterial::Stone, TtSpecial::Track, "W"),
            ("MineralPillar", Shape::Wall, TtMaterial::Mineral, TtSpecial::Smooth, ""),
            ("MineralWallSmoothLR", Shape::Wall, TtMaterial::Mineral, TtSpecial::Smooth, "----W-E-"),
            ("MineralFloorSmooth", Shape::Floor, TtMaterial::Mineral, TtSpecial::Smooth, ""),
        ];
        for (n, s, m, p, d) in add {
            let id = w.tiletypes.infos.len() as u16;
            w.tiletypes.infos.push(TiletypeInfo { id, name: (*n).into(), shape: *s, material: *m, special: *p, variant: 0, direction: (*d).into() });
        }
        w.tiletypes.reindex();
        for u in &mut w.units {
            u.labors.extend(["BUILD_CONSTRUCTION".to_string(), "ENGRAVER".into(), "HAUL_REFUSE".into()]);
        }
        w
    }

    fn hall_z() -> i16 {
        (20i16 * 2 / 3) - 4
    }

    fn boulder(w: &mut World, id: i32, p: Pos, kind: &str) {
        w.items.push(Item {
            id,
            kind: kind.into(),
            mat: 0,
            pos: p,
            stack: 1,
            container: -1,
            holder: -1,
            building: -1,
            desc: "granite boulder".into(),
            flags: ItemFlags { on_ground: true, ..Default::default() },
            ..Default::default()
        });
    }

    fn run_until(sim: &mut Sim, max: u32, done: impl Fn(&Sim) -> bool) -> bool {
        for _ in 0..max / 100 {
            sim.run_ticks(100);
            if done(sim) {
                return true;
            }
        }
        done(sim)
    }

    #[test]
    fn wall_from_boulder_blocks_pathing() {
        let mut w = world();
        let hz = hall_z();
        // A 1-wide corridor: the hall row y = 24 - 4 is closed by a wall at x = 18 so that the
        // west end (x 18..) is only reachable through (18, 20).
        let (cx, cy) = (24i16, 24i16);
        let site = Pos::new(cx - 6, cy - 4, hz);
        boulder(&mut w, 5000, Pos::new(cx + 3, cy, hz), "BOULDER");
        w.items.retain(|i| i.kind != "WOOD");
        let mut sim = Sim::new(w, 3);
        assert!(sim.is_walkable(site));
        let n = sim.build_construction(CKind::Wall, site, site);
        assert_eq!(n, 1);
        assert!(sim.world.buildings.iter().any(|b| b.kind == "Construction" && b.subtype == "Wall"));
        assert!(run_until(&mut sim, 20_000, |s| s.world.shape(site) == Shape::Wall), "wall gets built");
        let info = sim.world.tt(site);
        assert_eq!(info.material, TtMaterial::Construction);
        assert_eq!(sim.world.tile(site).unwrap().mat, 0, "granite wall");
        assert!(!sim.is_walkable(site), "walls block movement");
        assert!(sim.find_path(Pos::new(cx - 6, cy - 3, hz), site).is_none());
        let b = sim.world.items.iter().find(|i| i.id == 5000).unwrap();
        assert!(b.flags.construction && !b.flags.on_ground && b.pos == site, "DF 53 keeps the boulder in the wall");
        assert!(!sim.world.buildings.iter().any(|b| b.kind == "Construction"), "planned construction is gone");
        assert_eq!(sim.construct_report().built, 1);
        // Remove it again: the tile is floor again and the boulder comes back.
        assert_eq!(sim.build_construction(CKind::Remove, site, site), 1);
        assert!(run_until(&mut sim, 20_000, |s| s.world.shape(site) == Shape::Floor), "wall removed");
        assert!(sim.is_walkable(site));
        let b = sim.world.items.iter().find(|i| i.id == 5000).unwrap();
        assert!(!b.flags.construction && b.flags.on_ground);
    }

    #[test]
    fn creature_on_tile_suspends_wall() {
        let mut w = world();
        let hz = hall_z();
        let site = Pos::new(20, 22, hz);
        boulder(&mut w, 5000, Pos::new(21, 22, hz), "BOULDER");
        let mut sim = Sim::new(w, 3);
        // The cat sits on the site and never moves.
        let cat = sim.ai.iter().position(|a| a.id == 100).unwrap();
        sim.ai[cat].pos = sim.world.idx(site).unwrap() as u32;
        sim.ai[cat].wake = u64::MAX;
        sim.build_construction(CKind::Wall, site, site);
        sim.run_ticks(3000);
        assert_ne!(sim.world.shape(site), Shape::Wall);
        assert!(sim.construct_report().suspended > 0);
        sim.ai[cat].pos = sim.world.idx(Pos::new(24, 26, hz)).unwrap() as u32;
        assert!(run_until(&mut sim, 20_000, |s| s.world.shape(site) == Shape::Wall));
    }

    #[test]
    fn floor_over_open_space() {
        let mut w = world();
        let hz = hall_z();
        // Channel-like hole in the hall: open space above a dug-out floor below.
        let hole = Pos::new(20, 25, hz);
        let below = Pos::new(20, 25, hz - 1);
        let open = w.tiletypes.by_name("OpenSpace").unwrap();
        let floor = w.tiletypes.by_name("StoneFloor1").unwrap();
        w.tile_mut(hole).unwrap().tt = open;
        w.tile_mut(below).unwrap().tt = floor;
        boulder(&mut w, 5000, Pos::new(22, 25, hz), "BLOCKS");
        // Floating open space with no support at all is rejected.
        let mut sim = Sim::new(w, 4);
        assert!(!sim.is_walkable(hole));
        let sky = Pos::new(5, 5, 19);
        assert_eq!(sim.build_construction(CKind::Floor, sky, sky), 0, "no support in mid-air");
        assert_eq!(sim.build_construction(CKind::Floor, hole, hole), 1);
        assert!(run_until(&mut sim, 20_000, |s| s.world.shape(hole) == Shape::Floor));
        assert_eq!(sim.world.tt(hole).material, TtMaterial::Construction);
        assert!(sim.is_walkable(hole));
        assert!(sim.find_path(Pos::new(21, 25, hz), hole).is_some());
    }

    #[test]
    fn stair_construction_connects_levels() {
        let mut w = world();
        let hz = hall_z();
        let (x, y) = (20i16, 23i16);
        let open = w.tiletypes.by_name("OpenSpace").unwrap();
        let floor = w.tiletypes.by_name("StoneFloor1").unwrap();
        // A room one level below with the floor above opened up: the only link is a stair pair.
        for dy in -1..=1 {
            for dx in -1..=1 {
                w.tile_mut(Pos::new(x + dx, y + dy, hz - 1)).unwrap().tt = floor;
            }
        }
        w.tile_mut(Pos::new(x, y, hz)).unwrap().tt = open;
        boulder(&mut w, 5000, Pos::new(x + 2, y, hz), "BLOCKS");
        boulder(&mut w, 5001, Pos::new(x + 3, y, hz), "BLOCKS");
        let mut sim = Sim::new(w, 5);
        let low = Pos::new(x + 1, y, hz - 1);
        assert!(!sim.reachable(Pos::new(x + 1, y, hz), low));
        // Up stair below, down stair above (open space above a stair is supported).
        assert_eq!(sim.build_construction(CKind::DownStair, Pos::new(x, y, hz), Pos::new(x, y, hz)), 1);
        assert!(run_until(&mut sim, 20_000, |s| s.world.shape(Pos::new(x, y, hz)) == Shape::StairDown));
        // Reaching the low room needs the up stair there; a dwarf below builds it.
        let d = sim.ai.iter().position(|a| a.kind == Kind::Worker).unwrap();
        let t = sim.world.idx(low).unwrap() as u32;
        sim.cancel_task(d);
        sim.ai[d].pos = t;
        sim.world.units[sim.ai[d].uidx].pos = low;
        let item = sim.world.items.iter().find(|i| i.id == 5001).unwrap().id;
        sim.set_item_flag(item, "forbid", false);
        let ip = sim.world.idx(Pos::new(x - 1, y, hz - 1)).unwrap();
        sim.set_item_pos(item, sim.world.pos_of(ip));
        assert_eq!(sim.build_construction(CKind::UpStair, Pos::new(x, y, hz - 1), Pos::new(x, y, hz - 1)), 1);
        assert!(run_until(&mut sim, 20_000, |s| s.world.shape(Pos::new(x, y, hz - 1)) == Shape::StairUp));
        assert!(sim.reachable(Pos::new(x + 1, y, hz), low), "stairs connect the levels");
        assert!(sim.find_path(Pos::new(x + 1, y, hz), low).is_some());
    }

    #[test]
    fn smoothing_engraving_and_fortification() {
        let mut w = world();
        let hz = hall_z();
        // Hall spans x 18..=30, y 20..=28; walls at y = 19 (north side).
        for u in &mut w.units {
            u.skills.push(rf_core::Skill { id: "ENGRAVE_STONE".into(), rating: 15, xp: 0 });
        }
        let mut sim = Sim::new(w, 6);
        let walls = (Pos::new(20, 19, hz), Pos::new(24, 19, hz));
        let floors = (Pos::new(20, 22, hz), Pos::new(22, 22, hz));
        assert_eq!(sim.designate_detail(walls.0, walls.1, DetailKind::Smooth), 5);
        assert_eq!(sim.designate_detail(floors.0, floors.1, DetailKind::Smooth), 3);
        assert!(sim.world.tile(walls.0).unwrap().flags & tile_flags::DESIG_SMOOTH != 0);
        // Soil and open floors that are already smooth are not designated.
        assert_eq!(sim.designate_detail(Pos::new(5, 5, 13), Pos::new(5, 5, 13), DetailKind::Smooth), 0);
        assert!(run_until(&mut sim, 40_000, |s| s.construct_report().smoothed == 8));
        let mid = sim.world.tt(Pos::new(22, 19, hz));
        assert_eq!((mid.shape, mid.special, mid.direction.as_str()), (Shape::Wall, TtSpecial::Smooth, "----W-E-"));
        assert_eq!(sim.world.tt(Pos::new(21, 22, hz)).special, TtSpecial::Smooth);
        assert_eq!(sim.world.tile(walls.0).unwrap().flags & tile_flags::DESIG_SMOOTH, 0);
        // Engrave a smoothed wall and floor.
        assert_eq!(sim.designate_detail(Pos::new(21, 19, hz), Pos::new(21, 19, hz), DetailKind::Engrave), 1);
        assert_eq!(sim.designate_detail(Pos::new(21, 22, hz), Pos::new(21, 22, hz), DetailKind::Engrave), 1);
        // Rough rock cannot be engraved.
        assert_eq!(sim.designate_detail(Pos::new(27, 19, hz), Pos::new(27, 19, hz), DetailKind::Engrave), 0);
        assert!(run_until(&mut sim, 40_000, |s| s.world.engravings.len() == 2));
        let e = sim.engraving_at(Pos::new(21, 22, hz)).unwrap();
        assert!(e.floor && e.quality >= 4, "a legendary engraver makes superior work or better (q {})", e.quality);
        assert!(sim.engraving_value_in(18, 18, 30, 28, hz) > 0);
        // Carve a fortification into a smoothed wall.
        assert_eq!(sim.designate_detail(Pos::new(23, 19, hz), Pos::new(23, 19, hz), DetailKind::CarveFortification), 1);
        assert!(run_until(&mut sim, 40_000, |s| s.world.shape(Pos::new(23, 19, hz)) == Shape::Fortification));
        assert_eq!(sim.world.tt(Pos::new(23, 19, hz)).material, TtMaterial::Stone);
        assert!(!sim.is_walkable(Pos::new(23, 19, hz)));
    }

    #[test]
    fn carve_track_follows_drag() {
        let w = world();
        let hz = hall_z();
        let mut sim = Sim::new(w, 7);
        assert_eq!(sim.designate_detail(Pos::new(20, 26, hz), Pos::new(22, 26, hz), DetailKind::CarveTrack), 3);
        assert!(run_until(&mut sim, 40_000, |s| s.construct_report().tracks == 3));
        assert_eq!(sim.world.tt(Pos::new(20, 26, hz)).direction, "E");
        assert_eq!(sim.world.tt(Pos::new(21, 26, hz)).direction, "EW");
        assert_eq!(sim.world.tt(Pos::new(22, 26, hz)).direction, "W");
    }

    #[test]
    fn carve_designations_survive_save_and_load() {
        // [audit-eco] Before: carve fortification/track designations lived only in the job list
        // and were lost on save/load.
        let w = world();
        let hz = hall_z();
        let mut sim = Sim::new(w, 7);
        assert_eq!(sim.designate_detail(Pos::new(20, 26, hz), Pos::new(22, 26, hz), DetailKind::CarveTrack), 3);
        assert_eq!(sim.world.carve_designations.len(), 3);
        // "Save" right away and load into a fresh simulation.
        let saved = sim.world.clone();
        let mut sim = Sim::new(saved, 7);
        assert_eq!(sim.construct_report().open_jobs, 3, "designations restored as jobs");
        assert!(run_until(&mut sim, 40_000, |s| s.construct_report().tracks == 3));
        assert_eq!(sim.world.tt(Pos::new(20, 26, hz)).direction, "E");
        assert_eq!(sim.world.tt(Pos::new(21, 26, hz)).direction, "EW");
        assert_eq!(sim.world.tt(Pos::new(22, 26, hz)).direction, "W");
        assert!(sim.world.carve_designations.is_empty(), "finished designations are removed");
    }

    #[test]
    fn forbid_stops_hauling() {
        let mut w = world();
        let hz = hall_z();
        let p = Pos::new(27, 26, hz);
        boulder(&mut w, 5000, p, "BOULDER");
        boulder(&mut w, 5001, Pos::new(27, 27, hz), "BOULDER");
        let mut sim = Sim::new(w, 8);
        sim.create_stockpile(Pos::new(18, 20, hz), Pos::new(20, 21, hz), &["stone"]);
        sim.set_item_flag(5000, "forbid", true);
        sim.run_ticks(5000);
        let a = sim.world.item(5000).unwrap();
        let b = sim.world.item(5001).unwrap();
        assert_eq!(a.pos, p, "forbidden boulder stays");
        assert!(b.pos.x <= 20 && b.pos.y <= 21, "the other one is stockpiled");
        // Claim it: now it gets hauled too.
        assert_eq!(sim.set_items_flag_in(p, p, "claim", true), 1);
        assert!(!sim.world.item(5000).unwrap().flags.forbid);
        assert!(run_until(&mut sim, 10_000, |s| s.world.item(5000).unwrap().pos.x <= 20));
    }

    #[test]
    fn dump_moves_items_to_dump_zone() {
        let mut w = world();
        let hz = hall_z();
        w.buildings.push(Building { id: 900, kind: "Civzone".into(), subtype: "Dump".into(), x1: 28, y1: 26, x2: 29, y2: 27, z: hz, center: Pos::new(28, 26, hz), built: true, owner: -1, ..Default::default() });
        boulder(&mut w, 5000, Pos::new(19, 21, hz), "BOULDER");
        boulder(&mut w, 5001, Pos::new(20, 21, hz), "BOULDER");
        let mut sim = Sim::new(w, 9);
        sim.create_stockpile(Pos::new(18, 26, hz), Pos::new(20, 28, hz), &["stone"]);
        assert_eq!(sim.set_items_flag_in(Pos::new(19, 21, hz), Pos::new(20, 21, hz), "dump", true), 2);
        assert!(run_until(&mut sim, 20_000, |s| s.construct_report().dumped == 2));
        for id in [5000, 5001] {
            let it = sim.world.item(id).unwrap();
            assert!((28..=29).contains(&it.pos.x) && (26..=27).contains(&it.pos.y), "item {id} in the dump at {:?}", it.pos);
            assert!(it.flags.forbid && it.flags.on_ground, "dumped items are forbidden");
        }
        // They are not hauled back to the stone pile.
        sim.run_ticks(3000);
        assert!((28..=29).contains(&sim.world.item(5000).unwrap().pos.x));
    }

    #[test]
    fn traffic_changes_paths() {
        let w = world();
        let hz = hall_z();
        let mut sim = Sim::new(w, 10);
        let (a, b) = (Pos::new(18, 24, hz), Pos::new(30, 24, hz));
        let straight = sim.find_path(a, b).unwrap();
        assert_eq!(straight.len(), 12, "straight across the hall");
        // Restrict the middle rows: the path detours around them.
        sim.set_traffic(Pos::new(19, 23, hz), Pos::new(29, 25, hz), Traffic::Restricted);
        sim.set_traffic(Pos::new(19, 21, hz), Pos::new(29, 21, hz), Traffic::High);
        let detour = sim.find_path(a, b).unwrap();
        assert!(detour.iter().any(|p| p.y == 21), "takes the high-traffic row");
        assert!(!detour.iter().any(|p| (19..=29).contains(&p.x) && (23..=25).contains(&p.y)), "avoids the restricted rows");
        assert_eq!(sim.world.tile(Pos::new(20, 24, hz)).unwrap().traffic(), 3);
    }

    #[test]
    fn construction_is_deterministic_across_threads() {
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            pool.install(|| {
                let mut w = world();
                let hz = hall_z();
                for k in 0..6 {
                    boulder(&mut w, 5000 + k, Pos::new(26, 21 + k as i16, hz), "BLOCKS");
                }
                let mut sim = Sim::new(w, 11);
                sim.set_parallel_threshold(1);
                sim.build_construction(CKind::Wall, Pos::new(19, 27, hz), Pos::new(23, 27, hz));
                sim.designate_detail(Pos::new(20, 19, hz), Pos::new(26, 19, hz), DetailKind::Smooth);
                sim.run_ticks(15_000);
                (sim.state_hash(), sim.construct_report().built, sim.construct_report().smoothed)
            })
        };
        let a = run(1);
        let b = run(4);
        assert_eq!(a, b);
        assert!(a.1 > 0 && a.2 > 0, "work got done: {a:?}");
    }
}
