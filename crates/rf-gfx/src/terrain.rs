//! Terrain, item and building sprite lookup (DF Premium mapping, see README.md).

use crate::{hash_pos, mat_class, Gfx, MatClass, Sprite};
use rf_core::{Building, Dig, Item, Pos, Shape, TtMaterial, TtSpecial, World};
use std::collections::HashMap;

// Direction bits.
const N: u8 = 1;
const S: u8 = 2;
const W: u8 = 4;
const E: u8 = 8;
const NW: u8 = 16;
const NE: u8 = 32;
const SW: u8 = 64;
const SE: u8 = 128;

fn dir_bit(t: &str) -> Option<u8> {
    Some(match t {
        "N" => N,
        "S" => S,
        "W" => W,
        "E" => E,
        "NW" => NW,
        "NE" => NE,
        "SW" => SW,
        "SE" => SE,
        _ => return None,
    })
}

/// Parse "N_S_W_E" (optionally with a trailing variant number) into a direction mask.
fn parse_dirs(s: &str) -> Option<(u8, Option<u32>)> {
    let mut mask = 0u8;
    let mut variant = None;
    for t in s.split('_') {
        if let Some(b) = dir_bit(t) {
            mask |= b;
        } else if let Ok(v) = t.parse::<u32>() {
            variant = Some(v);
        } else {
            return None;
        }
    }
    Some((mask, variant))
}

/// Wall edge pieces of one wall page: indexed by the mask of *open* orthogonal sides.
#[derive(Clone, Debug, Default)]
pub(crate) struct WallSet {
    pub orth: Vec<Vec<Sprite>>, // 16 entries
    /// Inner-corner pieces for an open diagonal: NW, NE, SW, SE.
    pub diag: [Option<Sprite>; 4],
}

impl WallSet {
    fn from_page(g: &Gfx, token: &str) -> Option<WallSet> {
        let page = g.page_id(token)?;
        let mut ws = WallSet { orth: vec![Vec::new(); 16], diag: [None; 4] };
        let mut any = false;
        for t in g.db.tiles.iter().filter(|t| t.page == token) {
            let Some(i) = t.name.rfind("_WALL_") else { continue };
            let Some((mask, _)) = parse_dirs(&t.name[i + 6..]) else { continue };
            let sp = Sprite::simple(page, t.x, t.y);
            any = true;
            match mask {
                NW => ws.diag[0] = Some(sp),
                NE => ws.diag[1] = Some(sp),
                SW => ws.diag[2] = Some(sp),
                SE => ws.diag[3] = Some(sp),
                m if m < 16 => {
                    if !ws.orth[m as usize].contains(&sp) {
                        ws.orth[m as usize].push(sp)
                    }
                }
                _ => {}
            }
        }
        any.then_some(ws)
    }
}

/// Ramps of one material: wall-neighbor mask (8 bits) -> sprite.
#[derive(Clone, Debug, Default)]
pub(crate) struct RampSet {
    pub by_mask: HashMap<u8, Sprite>,
    pub other: Option<Sprite>,
}

impl RampSet {
    fn from_page(g: &Gfx, token: &str) -> Option<RampSet> {
        let page = g.page_id(token)?;
        let mut rs = RampSet::default();
        for t in g.db.tiles.iter().filter(|t| t.page == token) {
            let sp = Sprite::simple(page, t.x, t.y);
            if let Some(i) = t.name.find("_RAMP_WITH_WALL_") {
                if let Some((m, _)) = parse_dirs(&t.name[i + 16..]) {
                    rs.by_mask.entry(m).or_insert(sp);
                }
            } else if t.name.ends_with("_RAMP_OTHER") {
                rs.other = Some(sp);
            }
        }
        (rs.other.is_some() || !rs.by_mask.is_empty()).then_some(rs)
    }
    fn get(&self, mask: u8) -> Option<Sprite> {
        self.by_mask.get(&mask).or_else(|| self.by_mask.get(&(mask & 0x0F))).copied().or(self.other)
    }
}

/// A floor with its 3x3 bleed pieces (`X_1..X_9`, 5 = the tile itself) and alt centers (5B..5D).
#[derive(Clone, Debug, Default)]
pub(crate) struct FloorSet {
    pub pieces: [Option<Sprite>; 9],
    pub alts: [Option<Sprite>; 3],
}

impl FloorSet {
    fn new(g: &Gfx, prefix: &str) -> FloorSet {
        let mut f = FloorSet::default();
        for i in 0..9 {
            f.pieces[i] = g.named.get(&format!("{prefix}_{}", i + 1)).copied();
        }
        for (i, s) in ["5B", "5C", "5D"].iter().enumerate() {
            f.alts[i] = g.named.get(&format!("{prefix}_{s}")).copied();
        }
        f
    }
    /// Center sprite for tiletype variant 0..=4.
    fn center(&self, variant: u8, h: u32) -> Option<Sprite> {
        let v = if variant == 0 { (h % 4) as u8 + 1 } else { variant };
        match v {
            2..=4 => self.alts[(v - 2) as usize].or(self.pieces[4]),
            _ => self.pieces[4],
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StairSet {
    pub up: Option<Sprite>,
    pub down: Option<Sprite>,
    pub updown: Option<Sprite>,
}

impl StairSet {
    fn new(g: &Gfx, prefix: &str) -> StairSet {
        StairSet {
            up: g.named.get(&format!("{prefix}_STAIR_UP")).copied(),
            down: g.named.get(&format!("{prefix}_STAIR_DOWN")).copied(),
            updown: g.named.get(&format!("{prefix}_STAIR_UPDOWN")).copied(),
        }
    }
}

/// One workshop/furnace: sprites by [stage 0..=3][ty 0..=3][tx 0..=2] (ty 0 = row above).
#[derive(Clone, Debug, Default)]
pub(crate) struct WorkshopGfx {
    pub base: Vec<Option<Sprite>>,
    pub overlay: Vec<Option<Sprite>>,
}

impl WorkshopGfx {
    fn idx(stage: usize, ty: usize, tx: usize) -> usize {
        (stage * 4 + ty) * 3 + tx
    }
}

#[derive(Default)]
pub(crate) struct Tables {
    pub hidden: Vec<Sprite>,
    pub grass: FloorSet,
    pub stone: FloorSet,
    pub dirt: FloorSet,
    pub sand: FloorSet,
    pub pebbles: FloorSet,
    pub boulders: Vec<Sprite>,
    pub smooth_floor: Option<Sprite>,
    pub wood_floor: Option<Sprite>,
    pub metal_floor: Option<Sprite>,
    pub block_floor: Option<Sprite>,
    pub glass_floor: Option<Sprite>,
    pub ice_rough: Option<Sprite>,
    pub ice_smooth: Option<Sprite>,
    pub ashes: Option<Sprite>,
    pub furrowed: Vec<Sprite>,
    pub driftwood: Option<Sprite>,
    pub water: Option<Sprite>,
    pub magma: Option<Sprite>,
    pub walls: HashMap<&'static str, WallSet>,
    pub ramps: HashMap<&'static str, RampSet>,
    pub stairs: HashMap<&'static str, StairSet>,
    pub fortification: Option<Sprite>,
    pub tree_trunk: Option<Sprite>,
    pub tree_branch: Option<Sprite>,
    pub shrub: Option<Sprite>,
    pub sapling: Option<Sprite>,
    pub desig: HashMap<&'static str, Sprite>,
    pub stockpile: Option<Sprite>,
    pub workshops: HashMap<&'static str, WorkshopGfx>,
    /// Palette-recolorable terrain pages per material class.
    pub pal_pages: HashMap<MatClass, Vec<u16>>,
    pub item_pages: Vec<u16>,
    pub building_pages: Vec<u16>,
}

/// DF workshop/furnace subtype name -> Premium graphics name.
const WORKSHOP_NAMES: &[(&str, &str)] = &[
    ("Carpenters", "WORKSHOP_CARPENTER"),
    ("Masons", "WORKSHOP_MASON"),
    ("Craftsdwarfs", "WORKSHOP_CRAFTS"),
    ("Still", "WORKSHOP_STILL"),
    ("Kitchen", "WORKSHOP_KITCHEN"),
    ("Butchers", "WORKSHOP_BUTCHER"),
    ("Tanners", "WORKSHOP_TANNER"),
    ("Fishery", "WORKSHOP_FISHERY"),
    ("Farmers", "WORKSHOP_FARMER"),
    ("Jewelers", "WORKSHOP_JEWELER"),
    ("Mechanics", "WORKSHOP_MECHANIC"),
    ("MetalsmithsForge", "WORKSHOP_METALSMITH"),
    ("MagmaForge", "WORKSHOP_MAGMAFORGE"),
    ("Loom", "WORKSHOP_LOOM"),
    ("Leatherworks", "WORKSHOP_LEATHER"),
    ("Clothiers", "WORKSHOP_CLOTHES"),
    ("Dyers", "WORKSHOP_DYER"),
    ("Bowyers", "WORKSHOP_BOWYER"),
    ("Ashery", "WORKSHOP_ASHERY"),
    ("Custom", "WORKSHOP_CUSTOM"),
    ("Smelter", "FURNACE_SMELTER"),
    ("MagmaSmelter", "FURNACE_SMELTER_LAVA"),
    ("WoodFurnace", "FURNACE_WOOD"),
    ("Kiln", "FURNACE_KILN"),
    ("MagmaKiln", "FURNACE_KILN_LAVA"),
    ("GlassFurnace", "FURNACE_GLASS"),
    ("MagmaGlassFurnace", "FURNACE_GLASS_LAVA"),
];

const WALL_PAGES: &[&str] = &[
    "WALL_STONE",
    "WALL_WORN1_STONE",
    "WALL_WORN2_STONE",
    "WALL_WORN3_STONE",
    "WALL_STONE_SMOOTHED",
    "WALL_STONE_ENGRAVED",
    "WALL_SOIL",
    "WALL_ORE_VEIN",
    "WALL_GEM",
    "WALL_ROCK_BLOCKS",
    "WALL_WOODEN",
    "WALL_REINFORCED_METAL",
    "WALL_METAL_BARS",
    "WALL_ICE",
    "WALL_ICE_SMOOTHED",
    "WALL_MOSS",
    "WALL_ROOT",
    "WALL_MAGMA",
    "WALL_SAND",
];

const RAMP_PAGES: &[&str] = &["STONE_RAMPS", "SOIL_RAMPS", "GRASS_RAMPS", "SAND_BEIGE_RAMPS", "SAND_RED_RAMPS"];

impl Tables {
    pub(crate) fn build(g: &Gfx) -> Tables {
        let n = |s: &str| g.named.get(s).copied();
        let mut t = Tables {
            hidden: (1..=5).filter_map(|i| n(&format!("HIDDEN_ROCK_{i}"))).collect(),
            grass: FloorSet::new(g, "GRASS"),
            stone: FloorSet::new(g, "STONE_FLOOR"),
            dirt: FloorSet::new(g, "DIRT_FLOOR"),
            sand: FloorSet::new(g, "SAND_FLOOR"),
            pebbles: FloorSet::new(g, "PEBBLES_FLOOR"),
            boulders: g.db.tiles_named("BOULDER").filter_map(|t| g.page_id(&t.page).map(|p| Sprite::simple(p, t.x, t.y))).collect(),
            smooth_floor: n("SMOOTH_FLOOR"),
            wood_floor: n("WOOD_FLOOR"),
            metal_floor: n("METAL_FLOOR"),
            block_floor: first_on_page(g, "FLOOR_STONE_BLOCK"),
            glass_floor: n("GLASS_CLEAR_FLOOR"),
            ice_rough: n("ROUGH_ICE_FLOOR"),
            ice_smooth: n("SMOOTH_ICE_FLOOR"),
            ashes: first_on_page(g, "FLOOR_ASHES"),
            furrowed: (1..=4).filter_map(|i| n(&format!("FURROWED_SOIL_{i}"))).collect(),
            driftwood: n("DRIFTWOOD"),
            water: n("WATER"),
            magma: n("MAGMA_1"),
            fortification: n("FORTIFICATION"),
            tree_trunk: n("TREE_TRUNK_PILLAR").or(n("TREE_BASE_TRUNK")),
            tree_branch: n("TREE_BRANCH_NSWE").or(n("TREE_BRANCH")),
            shrub: n("SHRUB"),
            sapling: n("SAPLING"),
            stockpile: n("STOCKPILE_FLOOR"),
            ..Default::default()
        };
        for p in WALL_PAGES {
            if let Some(ws) = WallSet::from_page(g, p) {
                t.walls.insert(p, ws);
            }
        }
        for p in RAMP_PAGES {
            if let Some(rs) = RampSet::from_page(g, p) {
                t.ramps.insert(p, rs);
            }
        }
        for p in ["GRASS", "STONE", "DIRT", "SAND", "PALETTE"] {
            t.stairs.insert(p, StairSet::new(g, p));
        }
        for (k, name) in [
            ("dig", "DESIGNATION_DIG_STANDARD"),
            ("chop", "DESIGNATION_CHOP"),
            ("gather", "DESIGNATION_GATHER"),
            ("channel", "DESIGNATION_DIG_CHANNEL"),
            ("ramp", "DESIGNATION_DIG_RAMP"),
            ("updown", "DESIGNATION_DIG_STAIR_UPDOWN"),
            ("down", "DESIGNATION_DIG_STAIR_DOWN"),
            ("up", "DESIGNATION_DIG_STAIR_UP"),
            ("smooth", "DESIGNATION_SMOOTH"),
            ("engrave", "DESIGNATION_ENGRAVE"),
        ] {
            if let Some(s) = n(name) {
                t.desig.insert(k, s);
            }
        }
        // Workshops.
        for &(sub, name) in WORKSHOP_NAMES {
            let mut wg = WorkshopGfx { base: vec![None; 4 * 4 * 3], overlay: vec![None; 4 * 4 * 3] };
            let mut any = false;
            let ov = format!("{name}_OVERLAY");
            for (target, nm) in [(0, name), (1, ov.as_str())] {
                for td in g.db.tiles_named(nm) {
                    let Some(page) = g.page_id(&td.page) else { continue };
                    let a: Vec<usize> = td.extra.iter().filter_map(|e| e.parse().ok()).collect();
                    if a.len() < 3 || a[0] > 3 || a[1] > 2 || a[2] > 3 {
                        continue;
                    }
                    let i = WorkshopGfx::idx(a[0], a[2], a[1]);
                    let sp = Some(Sprite::simple(page, td.x, td.y));
                    if target == 0 {
                        wg.base[i] = sp;
                    } else {
                        wg.overlay[i] = sp;
                    }
                    any = true;
                }
            }
            if any {
                t.workshops.insert(sub, wg);
            }
        }
        // Palette pages per class.
        let ids = |list: &[&str]| -> Vec<u16> { list.iter().filter_map(|p| g.page_id(p)).collect() };
        t.pal_pages.insert(
            MatClass::Stone,
            ids(&[
                "WALL_STONE",
                "WALL_WORN1_STONE",
                "WALL_WORN2_STONE",
                "WALL_WORN3_STONE",
                "WALL_STONE_SMOOTHED",
                "WALL_STONE_ENGRAVED",
                "WALL_ORE_VEIN",
                "WALL_GEM",
                "WALL_ROCK_BLOCKS",
                "TERRAIN_BOULDERS",
                "FLOOR_PEBBLES",
                "STAIRS",
                "FLOOR_STONE_BLOCK",
                "FLOOR_STONE_ENGRAVED_PALETTE",
                "FORTIFICATION",
            ]),
        );
        t.pal_pages.insert(MatClass::Soil, ids(&["WALL_SOIL", "TERRAIN_BOULDERS", "FLOOR_PEBBLES", "STAIRS"]));
        t.pal_pages.insert(MatClass::Wood, ids(&["WALL_WOODEN", "FLOOR_WOOD", "FORTIFICATION_WOOD", "STAIRS", "TREES"]));
        t.pal_pages.insert(MatClass::Metal, ids(&["FLOOR_METAL", "WALL_REINFORCED_METAL", "WALL_METAL_BARS", "STAIRS"]));
        let mut ip: Vec<u16> = Vec::new();
        let mut bp: Vec<u16> = Vec::new();
        for (i, p) in g.pages.iter().enumerate() {
            let s = p.path.to_string_lossy().replace('\\', "/");
            if s.contains("_items_graphics/") {
                ip.push(i as u16);
                bp.push(i as u16);
            } else if s.contains("_buildings_graphics/") {
                bp.push(i as u16);
            }
        }
        t.item_pages = ip;
        t.building_pages = bp;
        t
    }

    pub(crate) fn palette_pages(&self, c: MatClass) -> &[u16] {
        self.pal_pages.get(&c).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

fn first_on_page(g: &Gfx, token: &str) -> Option<Sprite> {
    let page = g.page_id(token)?;
    g.db.tiles.iter().find(|t| t.page == token).map(|t| Sprite::simple(page, t.x, t.y))
}

#[inline]
fn is_wallish(world: &World, p: Pos) -> bool {
    match world.tile(p) {
        // Outside the map counts as solid rock (no border drawn towards the map edge).
        None => true,
        Some(t) => t.hidden() || matches!(world.tiletypes.get(t.tt).shape, Shape::Wall | Shape::None),
    }
}

/// Floor class priority for edge bleeding (higher bleeds over lower).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FloorKind {
    None = 0,
    Stone = 1,
    Dirt = 2,
    Grass = 3,
}

fn floor_kind(world: &World, p: Pos) -> FloorKind {
    let Some(t) = world.tile(p) else { return FloorKind::None };
    if t.hidden() {
        return FloorKind::None;
    }
    let tt = world.tiletypes.get(t.tt);
    if !matches!(tt.shape, Shape::Floor | Shape::Pebbles | Shape::Boulder | Shape::Shrub | Shape::Sapling) {
        return FloorKind::None;
    }
    if tt.special == TtSpecial::Smooth {
        return FloorKind::None;
    }
    match tt.material {
        m if m.is_grass() => FloorKind::Grass,
        TtMaterial::Soil => FloorKind::Dirt,
        TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::LavaStone | TtMaterial::Feature => FloorKind::Stone,
        _ => FloorKind::None,
    }
}

const DARKEN: [u8; 4] = [150, 150, 165, 255];

pub(crate) fn terrain(g: &Gfx, world: &World, p: Pos, out: &mut Vec<Sprite>) {
    terrain_at(g, world, p, out, 0);
}

fn terrain_at(g: &Gfx, world: &World, p: Pos, out: &mut Vec<Sprite>, depth: u8) {
    let t = &g.terrain_t;
    let Some(tile) = world.tile(p) else { return };
    let h = hash_pos(p, 0);
    if tile.hidden() {
        if !t.hidden.is_empty() && depth == 0 {
            out.push(t.hidden[(h as usize) % t.hidden.len()]);
        }
        return;
    }
    let tt = world.tiletypes.get(tile.tt);
    let start = out.len();
    match tt.shape {
        Shape::Empty | Shape::RampTop | Shape::EndlessPit | Shape::None => {
            // Open space: show the level below, darkened (DF's look-down view).
            if depth == 0 && p.z > 0 && tt.shape != Shape::None {
                let below = p.offset(0, 0, -1);
                let s0 = out.len();
                terrain_at(g, world, below, out, 1);
                for s in &mut out[s0..] {
                    *s = s.mul_tint(DARKEN);
                }
            }
        }
        Shape::Floor | Shape::Pebbles | Shape::Boulder | Shape::Shrub | Shape::Sapling | Shape::BrookBed | Shape::BrookTop => {
            floor(g, world, p, tile.mat, tt, h, out);
            match tt.shape {
                Shape::Pebbles => {
                    if let Some(s) = t.pebbles.pieces[4] {
                        out.push(g.recolor(s, world, tile.mat));
                    }
                }
                Shape::Boulder => {
                    if !t.boulders.is_empty() {
                        out.push(g.recolor(t.boulders[(h as usize >> 3) % t.boulders.len()], world, tile.mat));
                    }
                }
                Shape::Shrub => out.extend(t.shrub),
                Shape::Sapling => out.extend(t.sapling),
                _ => {}
            }
        }
        Shape::Wall => wall(g, world, p, tile.mat, tt, h, out),
        Shape::Fortification => {
            floor(g, world, p, tile.mat, tt, h, out);
            if let Some(s) = t.fortification {
                out.push(g.recolor(s, world, tile.mat));
            }
        }
        Shape::StairUp | Shape::StairDown | Shape::StairUpdown => {
            let fam = match tt.material {
                m if m.is_grass() => "GRASS",
                TtMaterial::Soil => "DIRT",
                TtMaterial::Construction => "PALETTE",
                _ => "STONE",
            };
            let fam = if mat_class(world.material(tile.mat)) == MatClass::Sand { "SAND" } else { fam };
            if let Some(st) = t.stairs.get(fam) {
                let s = match tt.shape {
                    Shape::StairUp => st.up,
                    Shape::StairDown => st.down,
                    _ => st.updown,
                };
                if let Some(s) = s {
                    out.push(if fam == "PALETTE" { g.recolor(s, world, tile.mat) } else { s });
                }
            }
        }
        Shape::Ramp => {
            let fam = match tt.material {
                m if m.is_grass() => "GRASS_RAMPS",
                TtMaterial::Soil => {
                    if mat_class(world.material(tile.mat)) == MatClass::Sand {
                        "SAND_BEIGE_RAMPS"
                    } else {
                        "SOIL_RAMPS"
                    }
                }
                _ => "STONE_RAMPS",
            };
            let mut mask = 0u8;
            let o = |dx: i16, dy: i16| is_wallish(world, p.offset(dx, dy, 0)) && world.in_bounds(p.offset(dx, dy, 0));
            let (n, s, w, e) = (o(0, -1), o(0, 1), o(-1, 0), o(1, 0));
            if n {
                mask |= N
            }
            if s {
                mask |= S
            }
            if w {
                mask |= W
            }
            if e {
                mask |= E
            }
            if !n && !w && o(-1, -1) {
                mask |= NW
            }
            if !n && !e && o(1, -1) {
                mask |= NE
            }
            if !s && !w && o(-1, 1) {
                mask |= SW
            }
            if !s && !e && o(1, 1) {
                mask |= SE
            }
            if let Some(rs) = t.ramps.get(fam) {
                if let Some(sp) = rs.get(mask) {
                    out.push(sp);
                }
            }
        }
        Shape::Branch | Shape::TrunkBranch | Shape::Twig => {
            if tt.shape != Shape::Twig {
                out.extend(t.tree_branch);
            }
        }
    }
    if depth > 0 {
        return;
    }
    // Liquids.
    let d = tile.liquid_depth();
    if d > 0 {
        let s = if tile.is_magma() { t.magma } else { t.water };
        if let Some(s) = s {
            let a = (110 + d as u16 * 20).min(255) as u8;
            out.push(s.with_tint([255, 255, 255, a]));
        }
    }
    // Designations.
    let key = match tile.dig() {
        Dig::None => None,
        Dig::Dig => Some(match tt.material {
            TtMaterial::Tree => "chop",
            TtMaterial::Plant if matches!(tt.shape, Shape::Shrub) => "gather",
            _ => "dig",
        }),
        Dig::Channel => Some("channel"),
        Dig::Ramp => Some("ramp"),
        Dig::UpDownStair => Some("updown"),
        Dig::DownStair => Some("down"),
        Dig::UpStair => Some("up"),
    };
    if let Some(k) = key.and_then(|k| t.desig.get(k)) {
        out.push(*k);
    }
    if tile.flags & rf_core::tile_flags::DESIG_SMOOTH != 0 {
        out.extend(t.desig.get("smooth").copied());
    }
    if tile.flags & rf_core::tile_flags::DESIG_ENGRAVE != 0 {
        out.extend(t.desig.get("engrave").copied());
    }
    let _ = start;
}

fn floor(g: &Gfx, world: &World, p: Pos, mat: u16, tt: &rf_core::TiletypeInfo, h: u32, out: &mut Vec<Sprite>) {
    let t = &g.terrain_t;
    let class = mat_class(world.material(mat));
    let base: Option<Sprite> = match tt.material {
        m if m.is_grass() => t.grass.center(tt.variant, h),
        TtMaterial::Soil => {
            if tt.special == TtSpecial::Furrowed && !t.furrowed.is_empty() {
                Some(t.furrowed[(h as usize) % t.furrowed.len()])
            } else if class == MatClass::Sand {
                t.sand.center(tt.variant, h)
            } else {
                t.dirt.center(tt.variant, h)
            }
        }
        TtMaterial::Construction => match class {
            MatClass::Wood => t.wood_floor.map(|s| g.recolor(s, world, mat)),
            MatClass::Metal => t.metal_floor.map(|s| g.recolor(s, world, mat)),
            MatClass::Glass => t.glass_floor,
            _ => t.block_floor.map(|s| g.recolor(s, world, mat)),
        },
        TtMaterial::FrozenLiquid => {
            if tt.special == TtSpecial::Smooth {
                t.ice_smooth
            } else {
                t.ice_rough
            }
        }
        TtMaterial::Ashes => t.ashes,
        TtMaterial::Driftwood => t.dirt.center(tt.variant, h),
        _ => {
            if matches!(tt.special, TtSpecial::Smooth | TtSpecial::SmoothDead) {
                t.smooth_floor
            } else {
                t.stone.center(tt.variant, h)
            }
        }
    };
    out.extend(base);
    if tt.material == TtMaterial::Driftwood {
        out.extend(t.driftwood);
    }
    // Edge bleeding from higher-priority neighbor floors (grass over dirt over stone).
    let mine = floor_kind(world, p);
    for dy in -1..=1i16 {
        for dx in -1..=1i16 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let k = floor_kind(world, p.offset(dx, dy, 0));
            if k <= mine || k == FloorKind::None {
                continue;
            }
            let set = match k {
                FloorKind::Grass => &t.grass,
                FloorKind::Dirt => &t.dirt,
                _ => &t.stone,
            };
            // The piece of the neighbor's 3x3 that lies on this tile.
            let gx = (1 - dx) as usize;
            let gy = (1 - dy) as usize;
            if let Some(s) = set.pieces[gy * 3 + gx] {
                out.push(s);
            }
        }
    }
}

fn wall(g: &Gfx, world: &World, p: Pos, mat: u16, tt: &rf_core::TiletypeInfo, h: u32, out: &mut Vec<Sprite>) {
    let t = &g.terrain_t;
    let class = mat_class(world.material(mat));
    if matches!(tt.material, TtMaterial::Tree | TtMaterial::Mushroom | TtMaterial::Plant) {
        // Tree trunks stand on the floor of this level.
        out.extend(t.grass.center(1, h));
        if let Some(s) = t.tree_trunk {
            out.push(s);
        }
        return;
    }
    let fam: &str = match tt.material {
        TtMaterial::Soil => {
            if class == MatClass::Sand {
                "WALL_SAND"
            } else {
                "WALL_SOIL"
            }
        }
        TtMaterial::Construction => match class {
            MatClass::Wood => "WALL_WOODEN",
            MatClass::Metal => "WALL_REINFORCED_METAL",
            _ => "WALL_ROCK_BLOCKS",
        },
        TtMaterial::FrozenLiquid => {
            if tt.special == TtSpecial::Smooth {
                "WALL_ICE_SMOOTHED"
            } else {
                "WALL_ICE"
            }
        }
        TtMaterial::Root => "WALL_ROOT",
        _ => match tt.special {
            TtSpecial::Smooth | TtSpecial::SmoothDead => "WALL_STONE_SMOOTHED",
            TtSpecial::Worn1 => "WALL_WORN1_STONE",
            TtSpecial::Worn2 => "WALL_WORN2_STONE",
            TtSpecial::Worn3 => "WALL_WORN3_STONE",
            _ => "WALL_STONE",
        },
    };
    let Some(ws) = t.walls.get(fam).or_else(|| t.walls.get("WALL_STONE")) else { return };
    // Base: the rock mass (DF draws revealed walls over the dark rock texture).
    if !t.hidden.is_empty() {
        out.push(t.hidden[(h as usize) % t.hidden.len()]);
    }
    let open = |dx: i16, dy: i16| !is_wallish(world, p.offset(dx, dy, 0));
    let (n, s, w, e) = (open(0, -1), open(0, 1), open(-1, 0), open(1, 0));
    let mask = (n as usize) | (s as usize) << 1 | (w as usize) << 2 | (e as usize) << 3;
    let push = |sp: Sprite, out: &mut Vec<Sprite>| out.push(g.recolor(sp, world, mat));
    if mask != 0 {
        let v = &ws.orth[mask];
        if !v.is_empty() {
            push(v[(h as usize >> 4) % v.len()], out);
        }
    }
    let corners = [(!n && !w && open(-1, -1)), (!n && !e && open(1, -1)), (!s && !w && open(-1, 1)), (!s && !e && open(1, 1))];
    for (i, c) in corners.iter().enumerate() {
        if *c {
            if let Some(sp) = ws.diag[i] {
                push(sp, out);
            }
        }
    }
    // Ore veins and gems get their overlay on top of the host-rock edges.
    if tt.material == TtMaterial::Mineral {
        let ov = if world.material(mat).map(|m| m.is_gem).unwrap_or(false) { "WALL_GEM" } else { "WALL_ORE_VEIN" };
        if let Some(ovs) = t.walls.get(ov) {
            if mask != 0 && !ovs.orth[mask].is_empty() {
                let v = &ovs.orth[mask];
                push(v[(h as usize >> 5) % v.len()], out);
            }
        }
    }
}

/// Item sprites per material class for an item (or building) kind.
pub(crate) fn item_sprites_for_kind(g: &Gfx, kind: &str) -> [Option<Sprite>; 7] {
    let n = |s: &str| g.named.get(s).copied();
    let classes = [MatClass::Stone, MatClass::Soil, MatClass::Sand, MatClass::Wood, MatClass::Metal, MatClass::Glass, MatClass::Other];
    let k = match kind {
        "ARMORSTAND" => "ARMOR_STAND",
        "WEAPONRACK" => "WEAPON_RACK",
        "BAR" => "BARS",
        "TRAPPARTS" => "MECHANISMS",
        k => k,
    };
    let mut out = [None; 7];
    for (i, c) in classes.iter().enumerate() {
        let suffix = match c {
            MatClass::Wood => "WOOD",
            MatClass::Metal => "METAL",
            MatClass::Glass => "GLASS",
            _ => "STONE",
        };
        out[i] = match k {
            "DRINK" => n("ITEM_BARREL_WOOD_LIQUID"),
            "BARREL" => n(&format!("ITEM_BARREL_{}_EMPTY", if *c == MatClass::Metal { "METAL" } else { "WOOD" })),
            "BIN" => n(&format!("ITEM_BIN_{}_EMPTY", if *c == MatClass::Metal { "METAL" } else { "WOOD" })),
            "BUCKET" => n(&format!("ITEM_BUCKET_{}", if *c == MatClass::Metal { "METAL" } else { "WOOD" })),
            "BAG" => n("ITEM_BAG_EMPTY"),
            "SEEDS" => n("SEEDS_FOR_BAG").or(n("ITEM_BAG_IN_USE")),
            "CAGE" => n(&format!("ITEM_CAGE_{}", if *c == MatClass::Stone || *c == MatClass::Other { "WOOD" } else { suffix })),
            _ => n(&format!("ITEM_{k}_{suffix}")).or(n(&format!("ITEM_{k}"))).or(n(&format!("ITEM_{k}S"))),
        };
    }
    out
}

fn class_index(c: MatClass) -> usize {
    match c {
        MatClass::Stone => 0,
        MatClass::Soil => 1,
        MatClass::Sand => 2,
        MatClass::Wood => 3,
        MatClass::Metal => 4,
        MatClass::Glass => 5,
        MatClass::Other => 6,
    }
}

pub(crate) fn item(g: &Gfx, world: &World, it: &Item, out: &mut Vec<Sprite>) {
    let Some(set) = g.item_cache(&it.kind) else { return };
    let c = class_index(mat_class(world.material(it.mat)));
    if let Some(s) = set[c].or(set[0]).or(set[6]) {
        // Drinks are drawn as their (wooden) barrel; everything else in its own material.
        if it.kind == "DRINK" {
            out.push(s);
        } else {
            out.push(g.recolor(s, world, it.mat));
        }
    }
}

pub(crate) fn building(g: &Gfx, world: &World, b: &Building, p: Pos, out: &mut Vec<Sprite>) {
    let t = &g.terrain_t;
    match b.kind.as_str() {
        "Workshop" | "Furnace" => {
            let Some(wg) = t.workshops.get(b.subtype.as_str()) else { return };
            let lx = (p.x - b.x1) as usize;
            let ly = (p.y - b.y1) as usize;
            if lx > 2 || ly > 2 {
                return;
            }
            let stage = if b.built || b.stage < 0 { 3 } else { (b.stage.clamp(0, 2)) as usize };
            let emit = |ty: usize, dy: i16, out: &mut Vec<Sprite>| {
                let i = WorkshopGfx::idx(stage, ty, lx);
                if let Some(s) = wg.base[i] {
                    out.push(g.recolor(s, world, b.mat).at(0, dy));
                }
                if let Some(s) = wg.overlay[i] {
                    out.push(s.at(0, dy));
                }
            };
            if ly == 0 {
                emit(0, -1, out);
            }
            emit(ly + 1, 0, out);
        }
        "Stockpile" => out.extend(t.stockpile),
        "Civzone" | "FarmPlot" | "Construction" => {}
        k => {
            // Furniture: same sprites as the item lying around. Item cache is keyed by upper-case kind.
            let mut buf = [0u8; 32];
            let kb = k.as_bytes();
            if kb.len() > buf.len() {
                return;
            }
            for (i, c) in kb.iter().enumerate() {
                buf[i] = c.to_ascii_uppercase();
            }
            let Ok(key) = std::str::from_utf8(&buf[..kb.len()]) else { return };
            if let Some(set) = g.item_cache(key) {
                let c = class_index(mat_class(world.material(b.mat)));
                if let Some(s) = set[c].or(set[0]).or(set[6]) {
                    out.push(g.recolor(s, world, b.mat));
                }
            }
        }
    }
}
