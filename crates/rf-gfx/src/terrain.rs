//! Terrain, item and building sprite lookup (DF Premium mapping, see README.md).

use crate::plants::{self, TreeTt};
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

/// One workshop/furnace/depot: sprites by [stage 0..=3][ty 0..=h][tx 0..w] (ty 0 = the row
/// above the footprint). 3x3 workshops use `stage:tx:ty`, 5x5 ones (trade depot, siege
/// workshop) too; 1x1 ones (quern, millstone) use `stage:ty`.
#[derive(Clone, Debug, Default)]
pub(crate) struct WorkshopGfx {
    pub w: usize,
    pub h: usize,
    /// Highest stage with sprites (the built look).
    pub max_stage: usize,
    pub base: Vec<Option<Sprite>>,
    pub overlay: Vec<Option<Sprite>>,
}

impl WorkshopGfx {
    pub(crate) fn idx(&self, stage: usize, ty: usize, tx: usize) -> usize {
        (stage * (self.h + 1) + ty) * self.w + tx
    }
}

/// Sprites of simple (non-workshop) buildings, resolved once at load.
#[derive(Clone, Debug, Default)]
pub(crate) struct BldGfx {
    pub farmplot: Option<Sprite>,
    pub farmplot_planted: Option<Sprite>,
    pub trap_cage: Option<Sprite>,
    pub trap_stone: Option<Sprite>,
    pub trap_weapon: Option<Sprite>,
    pub plate: Option<Sprite>,
    pub lever: Option<Sprite>,
    pub well: Option<Sprite>,
    /// Stone, wood, metal.
    pub archery: [Option<Sprite>; 3],
    pub support: [Option<Sprite>; 3],
    pub chain: [Option<Sprite>; 2],
    pub slab: Option<Sprite>,
    /// Lowered bridges per family (stone, wood, metal, glass):
    /// [NS_W, NS_CENTER, NS_E, NS_1, WE_N, WE_CENTER, WE_S, WE_1, 1x1].
    pub bridge: [[Option<Sprite>; 9]; 4],
    /// Raised bridges per family and raise direction (N, S, W, E): the wall pieces along the
    /// raised edge [first end, middle, last end, one tile] (N/S edges run west-east: W, CENTER,
    /// E, 1; W/E edges run north-south: N, CENTER, S, 1).
    pub bridge_raised: [[[Option<Sprite>; 4]; 4]; 4],
    /// Raised 1x1 bridges per family and direction (N, S, W, E).
    pub bridge_raised_1x1: [[Option<Sprite>; 4]; 4],
    /// Occupied cage items per family (wood, metal, glass): the cage a caught creature sits in.
    pub cage_occupied: [Option<Sprite>; 3],
    pub road_dirt: Option<Sprite>,
    pub road_paved: Option<Sprite>,
}

/// Wall shadows on floors. Sides in order N, S, W, E; `near[side]` = the two tapered ends
/// (N: [OPEN_NW, OPEN_NE], S: [OPEN_SW, OPEN_SE], W: [OPEN_NW, OPEN_SW], E: [OPEN_NE, OPEN_SE]);
/// corners NW, NE, SW, SE.
#[derive(Clone, Debug, Default)]
pub(crate) struct ShadowSet {
    pub straight: [Option<Sprite>; 4],
    pub near: [[Option<Sprite>; 2]; 4],
    pub corner: [Option<Sprite>; 4],
}

/// Graphics of one typed item subtype (`ITEM_WEAPON_PICK`, `ITEM_ARMOR_COAT`, ...).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TypedItem {
    pub default: Option<Sprite>,
    pub wood: Option<Sprite>,
    pub metal: Option<Sprite>,
    pub artifact: Option<Sprite>,
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
    pub bld: BldGfx,
    pub shadows: ShadowSet,
    /// Typed item graphics (`ARMOR_GRAPHICS`, `WEAPON_GRAPHICS`, ...) by item subtype token.
    pub typed: HashMap<String, TypedItem>,
    pub remains: Option<Sprite>,
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
    ("Quern", "WORKSHOP_QUERN"),
    ("Millstone", "WORKSHOP_MILLSTONE"),
    ("Siege", "WORKSHOP_SIEGE"),
    ("TradeDepot", "TRADE_DEPOT"),
];

/// Premium graphics name of a workshop/furnace subtype (or `TradeDepot`).
pub(crate) fn workshop_gfx_name(sub: &str) -> Option<&'static str> {
    WORKSHOP_NAMES.iter().find(|(s, _)| *s == sub).map(|(_, n)| *n).or(match sub {
        "Kennels" => Some("WORKSHOP_KENNEL"),
        _ => None,
    })
}

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
            let ov = format!("{name}_OVERLAY");
            // (target, stage, tx, ty, sprite)
            let mut parts: Vec<(u8, usize, usize, usize, Sprite)> = Vec::new();
            for (target, nm) in [(0u8, name), (1, ov.as_str())] {
                for td in g.db.tiles_named(nm) {
                    let Some(page) = g.page_id(&td.page) else { continue };
                    // Only the leading numeric args (comments may follow).
                    let a: Vec<usize> = td.extra.iter().map_while(|e| e.trim().parse().ok()).collect();
                    let (stage, tx, ty) = match a.len() {
                        0 | 1 => continue,
                        2 => (a[0], 0, a[1]),
                        _ => (a[0], a[1], a[2]),
                    };
                    if stage > 3 || tx > 8 || ty > 9 {
                        continue;
                    }
                    parts.push((target, stage, tx, ty, Sprite::simple(page, td.x, td.y)));
                }
            }
            if parts.is_empty() {
                continue;
            }
            let w = parts.iter().map(|p| p.2).max().unwrap_or(0) + 1;
            let h = parts.iter().map(|p| p.3).max().unwrap_or(1).max(1);
            let max_stage = parts.iter().filter(|p| p.0 == 0).map(|p| p.1).max().unwrap_or(0);
            let n = 4 * (h + 1) * w;
            let mut wg = WorkshopGfx { w, h, max_stage, base: vec![None; n], overlay: vec![None; n] };
            for (target, stage, tx, ty, s) in parts {
                let i = wg.idx(stage, ty, tx);
                if target == 0 {
                    wg.base[i] = Some(s);
                } else {
                    wg.overlay[i] = Some(s);
                }
            }
            t.workshops.insert(sub, wg);
        }
        // Simple buildings.
        let mut b = BldGfx {
            farmplot: n("FARMPLOT"),
            farmplot_planted: n("FARMPLOT_PLANTED"),
            trap_cage: n("TRAP_CAGE"),
            trap_stone: n("TRAP_STONE"),
            trap_weapon: n("TRAP_WEAPON"),
            plate: n("TRAP_PLATE_READY"),
            lever: n("LEVER_SET"),
            well: n("BLD_WELL_WITH_ROPE").or(n("BLD_WELL")),
            archery: [n("BLD_ARCHERY_TARGET_STONE"), n("BLD_ARCHERY_TARGET_WOOD"), n("BLD_ARCHERY_TARGET_METAL")],
            support: [n("BLD_SUPPORT_STONE"), n("BLD_SUPPORT_WOOD"), n("BLD_SUPPORT_METAL")],
            chain: [n("BLD_CHAIN_METAL"), n("BLD_CHAIN_ROPE")],
            slab: n("ITEM_SLAB_BLANK"),
            road_dirt: n("BLD_DIRT_ROAD"),
            road_paved: n("BLD_PAVED_ROAD"),
            ..Default::default()
        };
        for (fi, fam) in ["STONE", "WOOD", "METAL", "GLASS"].iter().enumerate() {
            for (i, suf) in ["NS_W", "NS_CENTER", "NS_E", "NS_1", "WE_N", "WE_CENTER", "WE_S", "WE_1", "1"].iter().enumerate() {
                b.bridge[fi][i] = n(&format!("BLD_BRIDGE_{fam}_{suf}"));
            }
            for (di, d) in ["N", "S", "W", "E"].iter().enumerate() {
                let ends: [&str; 2] = if di < 2 { ["W", "E"] } else { ["N", "S"] };
                for (pi, piece) in [ends[0], "CENTER", ends[1], "1"].iter().enumerate() {
                    b.bridge_raised[fi][di][pi] = n(&format!("BLD_BRIDGE_{fam}_RAISED_{d}_{piece}"));
                }
                b.bridge_raised_1x1[fi][di] = n(&format!("BLD_BRIDGE_{fam}_1x1_RAISE_{d}"));
            }
        }
        for (i, fam) in ["WOOD", "METAL", "GLASS"].iter().enumerate() {
            b.cage_occupied[i] = n(&format!("ITEM_CAGE_{fam}_OCCUPIED")).or(n(&format!("ITEM_CAGE_{fam}")));
        }
        t.bld = b;
        t.remains = n("ITEM_REMAINS");
        let sides = [("N", "NW", "NE"), ("S", "SW", "SE"), ("W", "NW", "SW"), ("E", "NE", "SE")];
        for (i, (d, a, b2)) in sides.iter().enumerate() {
            t.shadows.straight[i] = n(&format!("WALL_SHADOW_STRAIGHT_{d}"));
            t.shadows.near[i] = [n(&format!("WALL_SHADOW_NEAR_{d}_OPEN_{a}")), n(&format!("WALL_SHADOW_NEAR_{d}_OPEN_{b2}"))];
        }
        for (i, c) in ["NW", "NE", "SW", "SE"].iter().enumerate() {
            t.shadows.corner[i] = n(&format!("WALL_SHADOW_AT_CORNER_{c}"));
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

/// Typed item graphics from the item graphics raws:
/// - `[ARMOR_GRAPHICS:PAGE:x:y:ITEM_ARMOR_COAT]` (also GLOVES/HELM/PANTS/SHOES/SHIELD/FOOD/
///   TOY/TRAPCOMP/TOOL) with optional material variants like `[SHOES_GRAPHICS_METAL:PAGE:x:y:..]`
///   or `[TOOL_GRAPHICS_WOOD:ALL:PAGE:x:y]`;
/// - `[WEAPON_GRAPHICS:ITEM_WEAPON_PICK]` / `[AMMO_GRAPHICS:..]` with `*_DEFAULT`, `*_WOOD`,
///   `*_ARTIFACT` sub-tokens (`PAGE:x:y`).
pub(crate) fn typed_items(g: &Gfx, raws: &rf_raws::Raws) -> HashMap<String, TypedItem> {
    const KINDS: &[&str] = &[
        "ARMOR_GRAPHICS",
        "GLOVES_GRAPHICS",
        "HELM_GRAPHICS",
        "PANTS_GRAPHICS",
        "SHOES_GRAPHICS",
        "SHIELD_GRAPHICS",
        "FOOD_GRAPHICS",
        "TOY_GRAPHICS",
        "TRAPCOMP_GRAPHICS",
        "TOOL_GRAPHICS",
        "WEAPON_GRAPHICS",
        "AMMO_GRAPHICS",
        "SIEGEAMMO_GRAPHICS",
    ];
    // Sprite from the first arg that names a page, followed by x:y.
    let sp = |t: &rf_raws::Token| -> Option<Sprite> {
        let i = t.args.iter().position(|a| g.page_id(a).is_some())?;
        let page = g.page_id(t.arg(i))?;
        Some(Sprite::simple(page, t.arg_i32(i + 1)?.max(0) as u16, t.arg_i32(i + 2)?.max(0) as u16))
    };
    let mut out: HashMap<String, TypedItem> = HashMap::new();
    for kind in KINDS {
        for o in raws.of_kind(kind) {
            let h = o.header();
            // Header either `SUBTYPE` or `PAGE:x:y:SUBTYPE[:extra]`.
            let (sub, head) = if h.args.len() >= 4 && g.page_id(h.arg(0)).is_some() { (h.arg(3), sp(h)) } else { (h.arg(0), None) };
            if sub.is_empty() {
                continue;
            }
            let e = out.entry(sub.to_string()).or_default();
            e.default = e.default.or(head);
            for t in &o.tokens[1..] {
                let n = t.name.as_str();
                if n.contains("UPRIGHT") || n.contains("WEAPON_TRAP") || n.contains("DIAGONAL") || n.contains("_HEAD") {
                    continue;
                }
                let Some(s) = sp(t) else { continue };
                if n.ends_with("_WOOD") || n.ends_with("_WOODEN") {
                    e.wood = e.wood.or(Some(s));
                } else if n.ends_with("_METAL") || n.ends_with("_MATERIAL") {
                    e.metal = e.metal.or(Some(s));
                } else if n.ends_with("_ARTIFACT") {
                    e.artifact = e.artifact.or(Some(s));
                } else if n.ends_with("_DEFAULT") {
                    e.default = e.default.or(Some(s));
                }
            }
        }
    }
    out.retain(|_, v| v.default.is_some() || v.wood.is_some() || v.metal.is_some());
    out
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
    floor_kind_tt(world.tiletypes.get(t.tt))
}

fn floor_kind_tt(tt: &rf_core::TiletypeInfo) -> FloorKind {
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
    let cls = g.tree_tt(tile.tt);
    if cls != TreeTt::None {
        tree_tile(g, world, p, tile, tt, cls, h, out, depth);
    } else {
        match_shape(g, world, p, tile, tt, h, out, depth);
    }
    if depth > 0 {
        return;
    }
    overlays(g, world, tile, tt, out);
}

/// Look down through open space at `p` (one level, darkened), like DF.
fn look_down(g: &Gfx, world: &World, p: Pos, out: &mut Vec<Sprite>) {
    if p.z <= 0 {
        return;
    }
    let s0 = out.len();
    terrain_at(g, world, p.offset(0, 0, -1), out, 1);
    for s in &mut out[s0..] {
        *s = s.mul_tint(DARKEN);
    }
}

#[allow(clippy::too_many_arguments)]
fn tree_tile(g: &Gfx, world: &World, p: Pos, tile: &rf_core::Tile, tt: &rf_core::TiletypeInfo, cls: TreeTt, h: u32, out: &mut Vec<Sprite>, depth: u8) {
    let plant = g.plant_of_mat(tile.mat);
    let set = &g.plants_t.trees[plant.map(|p| p.tree as usize).unwrap_or(0).min(g.plants_t.trees.len().saturating_sub(1))];
    let dead = plants::is_dead(tt);
    let nb = |dx: i16, dy: i16| g.tree_tt_at(world, p.offset(dx, dy, 0));
    let mask_of = |f: &dyn Fn(TreeTt) -> bool| -> u8 { (f(nb(0, -1)) as u8) | (f(nb(0, 1)) as u8) << 1 | (f(nb(-1, 0)) as u8) << 2 | (f(nb(1, 0)) as u8) << 3 };
    match cls {
        TreeTt::Trunk(_) | TreeTt::Thick(_) => {
            let below = world.tile(p.offset(0, 0, -1)).map(|t| g.tree_tt(t.tt)).unwrap_or_default();
            let base = !below.is_trunk();
            if base {
                // Ground level: the trunk with its roots on the floor of this level.
                floor_under_tree(g, world, p, h, out);
                let i = match cls {
                    TreeTt::Thick(i) => i as usize,
                    _ => 4,
                };
                if let Some(s) = set.base_trunk[i].or(set.trunk[0]) {
                    out.push(s);
                }
            } else {
                if depth == 0 {
                    look_down(g, world, p, out);
                }
                let s = match cls {
                    TreeTt::Thick(i) => set.trunk_thick[i as usize],
                    TreeTt::Trunk(m) => set.trunk[m as usize].or(set.trunk[0]),
                    _ => None,
                };
                out.extend(s);
            }
            if !dead {
                if let TreeTt::Trunk(m) = cls {
                    let leafy = mask_of(&|c| matches!(c, TreeTt::Twigs | TreeTt::Branches | TreeTt::Heavy(_)));
                    if m != 0 && leafy != 0 && !base {
                        out.extend(set.over_trunk[m as usize]);
                    }
                }
            }
        }
        TreeTt::Heavy(m) => {
            if depth == 0 {
                look_down(g, world, p, out);
            }
            out.extend(set.heavy_branch[m as usize].or(set.heavy_branch[15]));
            if !dead {
                out.extend(set.over_heavy[m as usize]);
            }
        }
        TreeTt::Branches => {
            if depth == 0 {
                look_down(g, world, p, out);
            }
            let m = mask_of(&|c| c.is_tree_solid());
            out.extend(set.branch[m as usize].or(set.branch[0]));
            if !dead {
                out.extend(set.twigs_full[(h as usize >> 3) % 4]);
            }
        }
        TreeTt::Twigs => {
            // Open tile: the renderer draws the level below first (DF's look-down).
            let m = mask_of(&|c| c.is_tree_solid());
            let s = if dead {
                set.leafless[m as usize].or(set.leafless[0])
            } else if m == 0 {
                set.twigs_full[(h as usize >> 3) % 4].or(set.twigs[0])
            } else {
                set.twigs[m as usize].or(set.twigs[0])
            };
            out.extend(s);
        }
        TreeTt::CapWall(m) => out.extend(set.cap_wall[m as usize].or(set.cap_pillar)),
        TreeTt::CapThick(i) => out.extend(set.cap_thick[i as usize].or(set.cap_pillar)),
        TreeTt::CapPillar => out.extend(set.cap_pillar),
        TreeTt::CapFloor(v) => out.extend(set.cap_floor[v as usize].or(set.cap_floor[0])),
        TreeTt::CapRamp => {
            if depth == 0 {
                look_down(g, world, p, out);
            }
        }
        TreeTt::TrunkSlope => {
            floor_under_tree(g, world, p, h, out);
            out.extend(set.slope_top.or(set.trunk[0]));
        }
        TreeTt::None => {}
    }
}

/// The floor a tree base stands on: grass outside, dirt below ground.
fn floor_under_tree(g: &Gfx, world: &World, p: Pos, h: u32, out: &mut Vec<Sprite>) {
    let t = &g.terrain_t;
    let outside = world.tile(p).map(|t| t.flags & rf_core::tile_flags::OUTSIDE != 0).unwrap_or(true);
    let s = if outside { t.grass.center(0, h).or(t.dirt.center(0, h)) } else { t.dirt.center(0, h) };
    out.extend(s);
}

#[allow(clippy::too_many_arguments)]
fn match_shape(g: &Gfx, world: &World, p: Pos, tile: &rf_core::Tile, tt: &rf_core::TiletypeInfo, h: u32, out: &mut Vec<Sprite>, depth: u8) {
    let t = &g.terrain_t;
    match tt.shape {
        Shape::Empty | Shape::RampTop | Shape::EndlessPit | Shape::None => {
            // Open space: show the level below, darkened (DF's look-down view).
            if depth == 0 && tt.shape != Shape::None {
                look_down(g, world, p, out);
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
                Shape::Shrub => {
                    let pg = g.plant_of_mat(tile.mat);
                    let dead = matches!(tt.special, TtSpecial::Dead | TtSpecial::SmoothDead);
                    let s = pg.and_then(|pg| if dead { pg.shrub_dead.or(pg.shrub) } else { pg.shrub });
                    out.extend(s.or(t.shrub));
                }
                Shape::Sapling => out.extend(g.plant_of_mat(tile.mat).and_then(|pg| pg.sapling).or(t.sapling)),
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
            // Only without `prepare` (tree tiles are classified there).
            if tt.shape != Shape::Twig {
                out.extend(t.tree_branch);
            }
        }
    }
}

/// Liquids and designation markers on top of a tile.
fn overlays(g: &Gfx, _world: &World, tile: &rf_core::Tile, tt: &rf_core::TiletypeInfo, out: &mut Vec<Sprite>) {
    let t = &g.terrain_t;
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
        // Shrubs/saplings: the ground under the plant is not exported; grass outside, dirt
        // underground (estimate).
        TtMaterial::Plant => {
            if world.tile(p).map(|t| t.flags & rf_core::tile_flags::OUTSIDE != 0).unwrap_or(false) {
                t.grass.center(0, h)
            } else {
                t.dirt.center(0, h)
            }
        }
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
    // One pass over the 8 neighbors (index (dy+1)*3 + (dx+1), 4 = this tile).
    let mine = floor_kind(world, p);
    let mut walls = [false; 9];
    for dy in -1..=1i16 {
        for dx in -1..=1i16 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let q = p.offset(dx, dy, 0);
            let ni = ((dy + 1) * 3 + (dx + 1)) as usize;
            let nt = match world.tile(q) {
                Some(nt) if !nt.hidden() => nt,
                _ => {
                    walls[ni] = true;
                    continue;
                }
            };
            let ntt = world.tiletypes.get(nt.tt);
            let k = floor_kind_tt(ntt);
            let cls = g.tree_tt(nt.tt);
            // Tree trunks cast their own (base) shadows, not wall shadows.
            walls[ni] = cls == TreeTt::None && matches!(ntt.shape, Shape::Wall | Shape::None);
            // Edge bleeding from higher-priority neighbor floors (grass over dirt over stone):
            // the piece of the neighbor's 3x3 that lies on this tile.
            if k > mine && k != FloorKind::None {
                let set = match k {
                    FloorKind::Grass => &t.grass,
                    FloorKind::Dirt => &t.dirt,
                    _ => &t.stone,
                };
                if let Some(s) = set.pieces[((1 - dy) * 3 + (1 - dx)) as usize] {
                    out.push(s);
                }
            }
            // Shadows of tree bases next to this floor (TREE_BASE_SHADOW_<dir of this tile>).
            if cls.is_trunk() && !g.tree_tt_at(world, q.offset(0, 0, -1)).is_trunk() {
                let pg = world.tile(q).and_then(|t| g.plant_of_mat(t.mat));
                let set = &g.plants_t.trees[pg.map(|p| p.tree as usize).unwrap_or(0).min(g.plants_t.trees.len().saturating_sub(1))];
                if let Some(s) = set.base_shadow[((1 - dy) * 3 + (1 - dx)) as usize] {
                    out.push(s);
                }
            }
        }
    }
    wall_shadows(g, &walls, out);
}

/// Shadows cast by neighboring walls onto a floor (`SHADOWS_WALL` page): straight pieces for
/// orthogonal walls (tapered `NEAR_<d>_OPEN_<diag>` where the wall run ends), corner pieces
/// for diagonal walls whose two adjacent sides are open.
fn wall_shadows(g: &Gfx, w: &[bool; 9], out: &mut Vec<Sprite>) {
    let sh = &g.terrain_t.shadows;
    // Neighbor indices: 0 NW, 1 N, 2 NE, 3 W, 5 E, 6 SW, 7 S, 8 SE.
    let (nw, n, ne, wv, e, sw, s, se) = (w[0], w[1], w[2], w[3], w[5], w[6], w[7], w[8]);
    // [straight, near_open_a, near_open_b] per side; a/b = the two diagonals of that side.
    let sides = [(n, nw, ne, 0usize), (s, sw, se, 1), (wv, nw, sw, 2), (e, ne, se, 3)];
    for (wall, da, db, i) in sides {
        if !wall {
            continue;
        }
        match (da, db) {
            (true, true) => out.extend(sh.straight[i]),
            (false, true) => out.extend(sh.near[i][0].or(sh.straight[i])),
            (true, false) => out.extend(sh.near[i][1].or(sh.straight[i])),
            (false, false) => {
                out.extend(sh.near[i][0].or(sh.straight[i]));
                out.extend(sh.near[i][1]);
            }
        }
    }
    // Corners: NW, NE, SW, SE.
    for (i, (d, a, b)) in [(nw, n, wv), (ne, n, e), (sw, s, wv), (se, s, e)].into_iter().enumerate() {
        if d && !a && !b {
            out.extend(sh.corner[i]);
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
    // DF item type / building type (upper-cased) -> Premium name stem.
    let k = match kind {
        "ARMORSTAND" => "ARMOR_STAND",
        "WEAPONRACK" => "WEAPON_RACK",
        "BAR" => "BARS",
        "TRAPPARTS" => "MECHANISMS",
        "ROUGH" => "ROUGH_GEM",
        "SMALLGEM" => "GEMS",
        "GEM" => "LARGE_GEM",
        "ANIMALTRAP" => "ANIMAL_TRAP",
        "HATCH" => "HATCH_COVER",
        "TRACTIONBENCH" => "TRACTION_BENCH",
        "SKIN_TANNED" => "TANNED_SKIN",
        "TRAPCOMP" => "TRAP_COMPONENT",
        "GRATEWALL" | "GRATEFLOOR" => "GRATE",
        "WINDOWGLASS" => "WINDOW_GLASS",
        "WINDOWGEM" => "WINDOW_GEM",
        "BOOKCASE" => "CABINET",
        "COIN" => "COINS_PILE",
        "EGG" => "EGG_SIZE2",
        "FOOD" => "PREPARED_MEAL",
        "POWDER_MISC" => "POWDER",
        "LIQUID_MISC" => "LIQUID",
        // Estimate: Premium has no glob (fat/tallow) sprite; a liquid blob is closest.
        "GLOB" => "LIQUID",
        "INSTRUMENT" => "INSTRUMENT_STRINGED_HANDHELD",
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
        let wm = if *c == MatClass::Metal { "METAL" } else { "WOOD" };
        out[i] = match k {
            "DRINK" => n("ITEM_BARREL_WOOD_LIQUID"),
            "BARREL" => n(&format!("ITEM_BARREL_{wm}_EMPTY")),
            "BIN" => n(&format!("ITEM_BIN_{wm}_EMPTY")),
            "BUCKET" => n(&format!("ITEM_BUCKET_{wm}")),
            "ANIMAL_TRAP" => n(&format!("ITEM_ANIMAL_TRAP_{wm}")),
            "BAG" => n("ITEM_BAG_EMPTY"),
            "SEEDS" => n("SEEDS_FOR_BAG").or(n("ITEM_BAG_IN_USE")),
            "CAGE" => n(&format!("ITEM_CAGE_{}", if *c == MatClass::Stone || *c == MatClass::Other { "WOOD" } else { suffix })),
            "ARMOR_STAND" | "WEAPON_RACK" => n(&format!("ITEM_{k}_{suffix}_EMPTY")).or(n(&format!("ITEM_{k}_STONE_EMPTY"))),
            "TRACTION_BENCH" => n(&format!("ITEM_TRACTION_BENCH_{}_ROPE", if *c == MatClass::Wood { "WOODEN" } else { suffix }))
                .or(n("ITEM_TRACTION_BENCH_STONE_ROPE")),
            "SLAB" => n("ITEM_SLAB_BLANK"),
            "COFFIN" => n(&format!("ITEM_COFFIN_{suffix}")).or(n("ITEM_COFFIN")),
            // Material families: own suffix, else metal (palette-recolored), else wood
            // (crafts like amulets/crowns have no stone version).
            _ => n(&format!("ITEM_{k}_{suffix}"))
                .or(n(&format!("ITEM_{k}")))
                .or(n(&format!("ITEM_{k}S")))
                .or(n(&format!("ITEM_{k}_{}", if *c == MatClass::Wood { "WOOD" } else { "METAL" })))
                .or(n(&format!("ITEM_{k}_WOOD"))),
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
    // Plant items use their plant's own graphics (PLANT_GRAPHICS PICKED / SEED / GROWTH_PICKED).
    match it.kind.as_str() {
        "PLANT" | "SEEDS" | "PLANT_GROWTH" => {
            let s = match it.kind.as_str() {
                "PLANT" => g.plant_of_mat(it.mat).and_then(|p| p.picked),
                "SEEDS" => g.plant_of_mat(it.mat).and_then(|p| p.seed),
                _ => g.mat_growth.get(it.mat as usize).copied().flatten(),
            };
            if let Some(s) = s {
                out.push(s);
                return;
            }
        }
        "CORPSE" | "CORPSEPIECE" | "REMAINS" => {
            if let Some(s) = g.corpse_cache.get(&it.id) {
                out.push(*s);
                return;
            }
            out.extend(g.terrain_t.remains);
            return;
        }
        _ => {}
    }
    // Typed items (weapons, armor, tools, ...) by subtype, in their material.
    if !it.subtype.is_empty() {
        if let Some(ti) = g.terrain_t.typed.get(it.subtype.as_str()) {
            let class = mat_class(world.material(it.mat));
            let s = if it.flags.artifact && ti.artifact.is_some() {
                ti.artifact
            } else {
                match class {
                    MatClass::Wood => ti.wood.or(ti.default),
                    MatClass::Metal => ti.metal.or(ti.default),
                    _ => ti.default.or(ti.metal),
                }
            };
            if let Some(s) = s.or(ti.wood) {
                out.push(g.recolor(s, world, it.mat));
                return;
            }
        }
    }
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
    let bl = &t.bld;
    let class = mat_class(world.material(b.mat));
    // Stone / wood / metal family index for buildings with three material looks.
    let swm = match class {
        MatClass::Wood => 1,
        MatClass::Metal => 2,
        _ => 0,
    };
    let rc = |s: Option<Sprite>, out: &mut Vec<Sprite>| {
        if let Some(s) = s {
            out.push(g.recolor(s, world, b.mat));
        }
    };
    let ws_key = match b.kind.as_str() {
        "Workshop" | "Furnace" => Some(b.subtype.as_str()),
        "TradeDepot" => Some("TradeDepot"),
        _ => None,
    };
    if let Some(key) = ws_key {
        let Some(wg) = t.workshops.get(key) else { return };
        let lx = (p.x - b.x1) as usize;
        let ly = (p.y - b.y1) as usize;
        if lx >= wg.w || ly >= wg.h {
            return;
        }
        let built = b.built || b.stage < 0;
        let stage = if built { wg.max_stage } else { (b.stage.max(0) as usize).min(wg.max_stage.saturating_sub(1)) };
        let emit = |ty: usize, dy: i16, out: &mut Vec<Sprite>| {
            let i = wg.idx(stage, ty, lx);
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
        return;
    }
    match b.kind.as_str() {
        "Stockpile" => out.extend(t.stockpile),
        "Civzone" | "Construction" => {}
        "FarmPlot" => {
            out.extend(bl.farmplot);
            if let Some(s) = g.crops_at.get(&p) {
                out.push(*s);
            }
        }
        "Trap" => {
            let s = match b.subtype.as_str() {
                "CageTrap" => bl.trap_cage,
                "StoneFallTrap" => bl.trap_stone,
                "WeaponTrap" => bl.trap_weapon,
                "Lever" => bl.lever,
                "PressurePlate" => bl.plate,
                _ => None,
            };
            rc(s, out);
        }
        "Well" => {
            rc(bl.well, out);
        }
        "ArcheryTarget" => rc(bl.archery[swm], out),
        "Support" => rc(bl.support[swm], out),
        "Chain" => out.extend(bl.chain[0]),
        "Slab" => rc(bl.slab, out),
        "RoadDirt" => out.extend(bl.road_dirt),
        "RoadPaved" => rc(bl.road_paved, out),
        "Bridge" => {
            let fam = match class {
                MatClass::Wood => 1,
                MatClass::Metal => 2,
                MatClass::Glass => 3,
                _ => 0,
            };
            match bridge_piece(b, p) {
                BridgePiece::Lowered(i) => {
                    let set = &bl.bridge[fam];
                    rc(set[i].or(set[1]).or(bl.bridge[0][i]), out);
                }
                BridgePiece::Raised(d, k) => {
                    let s = bl.bridge_raised[fam][d][k].or(bl.bridge_raised[fam][d][1]).or(bl.bridge_raised[0][d][k]);
                    rc(s, out);
                }
                BridgePiece::Raised1x1(d) => rc(bl.bridge_raised_1x1[fam][d].or(bl.bridge_raised_1x1[0][d]), out),
                // Raised bridges leave the rest of their area open; retracted ones all of it.
                BridgePiece::Open => {}
            }
        }
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

/// [round 5] Which part of a bridge to draw at a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BridgePiece {
    /// Lowered span piece: index into `BldGfx::bridge` (NS_W, NS_CENTER, NS_E, NS_1, WE_N,
    /// WE_CENTER, WE_S, WE_1, 1x1).
    Lowered(usize),
    /// Raised wall piece: direction (N, S, W, E) and piece (first end, middle, last end, one).
    Raised(usize, usize),
    Raised1x1(usize),
    /// Nothing of the bridge here (raised away from this tile, or retracted).
    Open,
}

fn has_flag(b: &Building, f: &str) -> bool {
    b.flags.iter().any(|x| x == f)
}

/// The bridge piece at `p`. DF: a raised drawbridge stands as a wall on the edge in its raise
/// direction (the whole bridge if it is one tile across that way) and the rest of its area is
/// whatever lies below; a retracted bridge is gone; a lowered one is a floor whose planks run
/// along its direction. Same edge rule as the sim (`rf-sim` defense).
pub(crate) fn bridge_piece(b: &Building, p: Pos) -> BridgePiece {
    let (w, h) = (b.x2 - b.x1 + 1, b.y2 - b.y1 + 1);
    let dir = b.direction.to_ascii_uppercase();
    let retract = dir.starts_with("RETRACT") || b.subtype.to_ascii_uppercase().contains("RETRACT");
    let up = has_flag(b, "RAISED") || has_flag(b, "CLOSED") || has_flag(b, "RETRACTED");
    let d = match dir.as_str() {
        "N" | "UP" | "NORTH" => Some(0usize),
        "S" | "DOWN" | "SOUTH" => Some(1),
        "W" | "LEFT" | "WEST" => Some(2),
        "E" | "RIGHT" | "EAST" => Some(3),
        _ => None,
    };
    if up {
        if retract {
            return BridgePiece::Open;
        }
        let Some(d) = d else { return BridgePiece::Open };
        if w == 1 && h == 1 {
            return BridgePiece::Raised1x1(d);
        }
        let on_edge = match d {
            0 => h == 1 || p.y == b.y1,
            1 => h == 1 || p.y == b.y2,
            2 => w == 1 || p.x == b.x1,
            _ => w == 1 || p.x == b.x2,
        };
        if !on_edge {
            return BridgePiece::Open;
        }
        // Position along the edge.
        let (along, lo, hi) = if d < 2 { (p.x, b.x1, b.x2) } else { (p.y, b.y1, b.y2) };
        let k = if lo == hi {
            3
        } else if along == lo {
            0
        } else if along == hi {
            2
        } else {
            1
        };
        return BridgePiece::Raised(d, k);
    }
    if w == 1 && h == 1 {
        return BridgePiece::Lowered(8);
    }
    // Lowered: the span runs in the raise direction (a bridge raising north spans north-south);
    // without a direction, along the longer side (estimate).
    let ns = match d {
        Some(0) | Some(1) => true,
        Some(_) => false,
        None => h >= w,
    };
    let i = if ns {
        if w == 1 {
            3
        } else if p.x == b.x1 {
            0
        } else if p.x == b.x2 {
            2
        } else {
            1
        }
    } else if h == 1 {
        7
    } else if p.y == b.y1 {
        4
    } else if p.y == b.y2 {
        6
    } else {
        5
    };
    BridgePiece::Lowered(i)
}

#[cfg(test)]
mod bridge_tests {
    use super::*;

    fn br(x1: i16, y1: i16, x2: i16, y2: i16, dir: &str, flags: &[&str]) -> Building {
        Building { kind: "Bridge".into(), x1, y1, x2, y2, direction: dir.into(), flags: flags.iter().map(|s| s.to_string()).collect(), built: true, ..Default::default() }
    }

    #[test]
    fn raised_bridge_is_a_wall_on_its_edge() {
        // Gordon's bridge 10972: 3x3, raises west.
        let b = br(99, 126, 101, 128, "W", &["HAS_SUPPORT", "RAISED"]);
        assert_eq!(bridge_piece(&b, Pos::new(99, 126, 0)), BridgePiece::Raised(2, 0));
        assert_eq!(bridge_piece(&b, Pos::new(99, 127, 0)), BridgePiece::Raised(2, 1));
        assert_eq!(bridge_piece(&b, Pos::new(99, 128, 0)), BridgePiece::Raised(2, 2));
        assert_eq!(bridge_piece(&b, Pos::new(100, 127, 0)), BridgePiece::Open);
        // One tile across in its direction: the whole bridge stands up.
        let b = br(5, 5, 7, 5, "N", &["RAISED"]);
        assert_eq!(bridge_piece(&b, Pos::new(5, 5, 0)), BridgePiece::Raised(0, 0));
        assert_eq!(bridge_piece(&b, Pos::new(6, 5, 0)), BridgePiece::Raised(0, 1));
        let b = br(5, 5, 5, 5, "S", &["RAISED"]);
        assert_eq!(bridge_piece(&b, Pos::new(5, 5, 0)), BridgePiece::Raised1x1(1));
        // Lowered: planks along the direction.
        let b = br(99, 126, 101, 128, "W", &["LOWERED"]);
        assert_eq!(bridge_piece(&b, Pos::new(100, 126, 0)), BridgePiece::Lowered(4));
        let b = br(10, 10, 10, 10, "S", &["LOWERED"]);
        assert_eq!(bridge_piece(&b, Pos::new(10, 10, 0)), BridgePiece::Lowered(8));
        // Retracted: gone.
        let b = br(1, 1, 3, 1, "Retracting", &["RETRACTED"]);
        assert_eq!(bridge_piece(&b, Pos::new(2, 1, 0)), BridgePiece::Open);
    }
}
