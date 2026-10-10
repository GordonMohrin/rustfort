//! Plants: per-plant graphics (`[PLANT_GRAPHICS:<plant>]`): shrubs, saplings, crops, picked
//! plants/seeds/growth items, and tree tiles (trunks, branches, twigs with leaves, mushroom
//! caps). Plants without their own `TREE_TILE`s use the standard tree pieces of the `TREES`
//! page (`TREE_TRUNK_*`, `TREE_BRANCH_*`, `TREE_TWIGS_*`, ...).
//!
//! Which plant a tile belongs to comes from the tile's material (`PLANT:<ID>:WOOD`,
//! `PLANT:<ID>:STRUCTURAL`, ...), resolved per world material in `Gfx::prepare`.

use crate::{Gfx, Sprite};
use rf_core::{Shape, TiletypeInfo, TtMaterial, TtSpecial};
use rf_raws::Raws;
use std::collections::HashMap;

/// Direction-set parsing for names like `NSWE`, `N_S_W_E`, `NS`, `` (empty = 0).
/// Bits: N=1, S=2, W=4, E=8. Lower-case parts (secondary connections, e.g. `S_nwe`) reject.
pub(crate) fn dir_mask(s: &str) -> Option<u8> {
    let mut m = 0u8;
    for c in s.chars() {
        m |= match c {
            'N' => 1,
            'S' => 2,
            'W' => 4,
            'E' => 8,
            '_' => 0,
            _ => return None,
        };
    }
    Some(m)
}

/// Position in a 3x3 block for `NW,N,NE,W,(center),E,SW,S,SE` suffixes.
fn pos9(s: &str) -> Option<usize> {
    Some(match s {
        "NW" => 0,
        "N" => 1,
        "NE" => 2,
        "W" => 3,
        "" | "INTERIOR" => 4,
        "E" => 5,
        "SW" => 6,
        "S" => 7,
        "SE" => 8,
        _ => return None,
    })
}

/// One set of tree pieces (standard, or a plant's own page).
#[derive(Clone, Debug, Default)]
pub(crate) struct TreeSet {
    /// By connection mask (0 = pillar).
    pub trunk: [Option<Sprite>; 16],
    pub trunk_thick: [Option<Sprite>; 9],
    pub base_trunk: [Option<Sprite>; 9],
    pub base_shadow: [Option<Sprite>; 9],
    pub heavy_branch: [Option<Sprite>; 16],
    pub branch: [Option<Sprite>; 16],
    pub twigs: [Option<Sprite>; 16],
    pub twigs_full: [Option<Sprite>; 4],
    pub leafless: [Option<Sprite>; 16],
    pub over_trunk: [Option<Sprite>; 16],
    pub over_heavy: [Option<Sprite>; 16],
    pub cap_wall: [Option<Sprite>; 16],
    pub cap_thick: [Option<Sprite>; 9],
    pub cap_pillar: Option<Sprite>,
    pub cap_floor: [Option<Sprite>; 4],
    pub slope_top: Option<Sprite>,
}

impl TreeSet {
    /// Put one named piece (`TREE_TRUNK_NSWE`, `TREE_CAP_WALL_N_S`, ...) into the set.
    fn put(&mut self, name: &str, s: Sprite) {
        fn r(rest: &str) -> &str {
            rest.strip_prefix('_').unwrap_or(rest)
        }
        if let Some(rest) = name.strip_prefix("TREE_TRUNK_THICK") {
            if let Some(i) = pos9(r(rest)) {
                self.trunk_thick[i].get_or_insert(s);
            }
        } else if name == "TREE_TRUNK_PILLAR" {
            self.trunk[0].get_or_insert(s);
        } else if let Some(rest) = name.strip_prefix("TREE_TRUNK_SLOPE") {
            if rest == "_TOP" {
                self.slope_top.get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_TRUNK_") {
            if let Some(m) = dir_mask(rest).filter(|&m| m != 0) {
                self.trunk[m as usize].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_BASE_TRUNK") {
            if let Some(i) = pos9(r(rest)) {
                self.base_trunk[i].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_BASE_SHADOW") {
            if let Some(i) = pos9(r(rest)) {
                self.base_shadow[i].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_HEAVY_BRANCH") {
            if let Some(m) = dir_mask(r(rest)) {
                self.heavy_branch[m as usize].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_BRANCH") {
            if let Some(m) = dir_mask(r(rest)) {
                self.branch[m as usize].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_TWIGS_FULL") {
            if let Ok(n) = rest.parse::<usize>() {
                if (1..=4).contains(&n) {
                    self.twigs_full[n - 1].get_or_insert(s);
                }
            }
        } else if let Some(rest) = name.strip_prefix("TREE_TWIGS") {
            if let Some(m) = dir_mask(r(rest)) {
                self.twigs[m as usize].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_LEAFLESS_TWIGS") {
            if let Some(m) = dir_mask(r(rest)) {
                self.leafless[m as usize].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_OVERLEAVES_TRUNK_") {
            if let Some(m) = dir_mask(rest) {
                self.over_trunk[m as usize].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_OVERLEAVES_HEAVY_BRANCH_") {
            if let Some(m) = dir_mask(rest) {
                self.over_heavy[m as usize].get_or_insert(s);
            }
        } else if let Some(rest) = name.strip_prefix("TREE_CAP_WALL_THICK_") {
            if let Some(i) = pos9(rest) {
                self.cap_thick[i].get_or_insert(s);
            }
        } else if name == "TREE_CAP_THICK_INTERIOR" {
            self.cap_thick[4].get_or_insert(s);
        } else if let Some(rest) = name.strip_prefix("TREE_CAP_WALL_") {
            if let Some(m) = dir_mask(rest) {
                self.cap_wall[m as usize].get_or_insert(s);
            }
        } else if name == "TREE_CAP_PILLAR" {
            self.cap_pillar.get_or_insert(s);
        } else if let Some(rest) = name.strip_prefix("TREE_CAP_FLOOR_") {
            if let Ok(n) = rest.parse::<usize>() {
                if (1..=4).contains(&n) {
                    self.cap_floor[n - 1].get_or_insert(s);
                }
            }
        }
    }

    /// Fill gaps of `self` from `std` (plants with partial own pages).
    fn fill_from(&mut self, std: &TreeSet) {
        fn f<const N: usize>(a: &mut [Option<Sprite>; N], b: &[Option<Sprite>; N]) {
            for i in 0..N {
                if a[i].is_none() {
                    a[i] = b[i];
                }
            }
        }
        f(&mut self.trunk, &std.trunk);
        f(&mut self.trunk_thick, &std.trunk_thick);
        f(&mut self.base_trunk, &std.base_trunk);
        f(&mut self.base_shadow, &std.base_shadow);
        f(&mut self.heavy_branch, &std.heavy_branch);
        f(&mut self.branch, &std.branch);
        f(&mut self.twigs, &std.twigs);
        f(&mut self.twigs_full, &std.twigs_full);
        f(&mut self.leafless, &std.leafless);
        f(&mut self.over_trunk, &std.over_trunk);
        f(&mut self.over_heavy, &std.over_heavy);
        f(&mut self.cap_wall, &std.cap_wall);
        f(&mut self.cap_thick, &std.cap_thick);
        f(&mut self.cap_floor, &std.cap_floor);
        self.cap_pillar = self.cap_pillar.or(std.cap_pillar);
        self.slope_top = self.slope_top.or(std.slope_top);
    }
}

/// Graphics of one plant.
#[derive(Clone, Debug, Default)]
pub(crate) struct PlantGfx {
    pub shrub: Option<Sprite>,
    pub shrub_dead: Option<Sprite>,
    pub picked: Option<Sprite>,
    pub seed: Option<Sprite>,
    pub crop: Option<Sprite>,
    pub crop_sprout: Option<Sprite>,
    pub sapling: Option<Sprite>,
    /// Growth id (`LEAVES`, `FRUIT`, ...) -> picked growth item sprite.
    pub growths: Vec<(String, Sprite)>,
    /// Index into `PlantTables::trees` (0 = standard set).
    pub tree: u16,
    /// [gfx6] Grass plants: `GRASS_1..4` (the floor look by tiletype variant 1..4).
    pub grass: [Option<Sprite>; 4],
    /// [gfx6] `CUSTOM_EDGING:n` (0 = none; lower n wins, all win over regular grass) and its
    /// `CUSTOM_EDGE_<dir>` pieces in order N, S, W, E, NW, NE, SW, SE (cavern grasses).
    pub edging: u8,
    pub edges: [Option<Sprite>; 8],
    /// [gfx6] `CUSTOM_RAMP`: ramps are the plant's `GRASS_1` plus the ramp overlay.
    pub custom_ramp: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PlantTables {
    pub by_id: HashMap<String, u16>,
    pub plants: Vec<PlantGfx>,
    /// Tree sets: 0 = the standard `TREES` page pieces.
    pub trees: Vec<TreeSet>,
    /// Plant id -> (local material id -> growth id), from the PLANT object raws.
    pub growth_mats: HashMap<String, Vec<(String, String)>>,
}

impl PlantTables {
    pub(crate) fn build(g: &Gfx, raws: &Raws) -> PlantTables {
        let mut t = PlantTables::default();
        let mut std = TreeSet::default();
        for (name, s) in &g.named {
            if name.starts_with("TREE_") {
                // Only the TREES page (the leaves/flowers/fruit pages hold seasonal variants).
                if g.pages.get(s.page as usize).map(|p| p.token == "TREES").unwrap_or(false) {
                    std.put(name, *s);
                }
            }
        }
        t.trees.push(std.clone());
        let sp = |g: &Gfx, tok: &rf_raws::Token, i: usize| -> Option<Sprite> {
            let page = g.page_id(tok.arg(i))?;
            Some(Sprite::simple(page, tok.arg_i32(i + 1)?.max(0) as u16, tok.arg_i32(i + 2)?.max(0) as u16))
        };
        for o in raws.of_kind("PLANT_GRAPHICS") {
            let idx = *t.by_id.entry(o.id.clone()).or_insert_with(|| {
                t.plants.push(PlantGfx::default());
                (t.plants.len() - 1) as u16
            });
            let mut pg = t.plants[idx as usize].clone();
            let mut own = TreeSet::default();
            let mut has_own = false;
            let mut growth: Option<String> = None;
            for tok in &o.tokens[1..] {
                match tok.name.as_str() {
                    "SHRUB" => pg.shrub = pg.shrub.or(sp(g, tok, 0)),
                    "SHRUB_DEAD" => pg.shrub_dead = pg.shrub_dead.or(sp(g, tok, 0)),
                    "PICKED" => pg.picked = pg.picked.or(sp(g, tok, 0)),
                    "SEED" => pg.seed = pg.seed.or(sp(g, tok, 0)),
                    "CROP" => pg.crop = pg.crop.or(sp(g, tok, 0)),
                    "CROP_SPROUT" => pg.crop_sprout = pg.crop_sprout.or(sp(g, tok, 0)),
                    "SAPLING" => pg.sapling = pg.sapling.or(sp(g, tok, 0)),
                    "GRASS_1" | "GRASS_2" | "GRASS_3" | "GRASS_4" => {
                        let i = (tok.name.as_bytes()[6] - b'1') as usize;
                        pg.grass[i] = pg.grass[i].or(sp(g, tok, 0));
                    }
                    "CUSTOM_EDGING" => pg.edging = tok.arg_i32(0).unwrap_or(0).clamp(0, 255) as u8,
                    "CUSTOM_RAMP" => pg.custom_ramp = true,
                    n if n.starts_with("CUSTOM_EDGE_") => {
                        let i = match &n[12..] {
                            "N" => 0,
                            "S" => 1,
                            "W" => 2,
                            "E" => 3,
                            "NW" => 4,
                            "NE" => 5,
                            "SW" => 6,
                            "SE" => 7,
                            _ => 8,
                        };
                        if i < 8 {
                            pg.edges[i] = pg.edges[i].or(sp(g, tok, 0));
                        }
                    }
                    "GROWTH" => growth = Some(tok.arg(0).to_string()),
                    "GROWTH_PICKED" => {
                        if let (Some(gid), Some(s)) = (&growth, sp(g, tok, 0)) {
                            if !pg.growths.iter().any(|(id, _)| id == gid) {
                                pg.growths.push((gid.clone(), s));
                            }
                        }
                    }
                    "TREE_TILE" => {
                        if let Some(s) = sp(g, tok, 1) {
                            own.put(tok.arg(0), s);
                            has_own = true;
                        }
                    }
                    _ => {}
                }
            }
            if has_own {
                own.fill_from(&std);
                pg.tree = t.trees.len() as u16;
                t.trees.push(own);
            }
            t.plants[idx as usize] = pg;
        }
        // Growth materials: [GROWTH:LEAVES] ... [GROWTH_ITEM:PLANT_GROWTH:NONE:LOCAL_PLANT_MAT:LEAF].
        for o in raws.of_kind("PLANT") {
            let mut cur: Option<&str> = None;
            let mut v: Vec<(String, String)> = Vec::new();
            for tok in &o.tokens[1..] {
                match tok.name.as_str() {
                    "GROWTH" => cur = Some(tok.arg(0)),
                    "GROWTH_ITEM" => {
                        if let Some(gid) = cur {
                            if let Some(i) = tok.args.iter().position(|a| a == "LOCAL_PLANT_MAT") {
                                v.push((tok.arg(i + 1).to_string(), gid.to_string()));
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !v.is_empty() {
                t.growth_mats.insert(o.id.clone(), v);
            }
        }
        t
    }

    /// Plant graphics index for a material token `PLANT:<ID>:<MAT>`.
    pub(crate) fn plant_of_token<'a>(&self, token: &'a str) -> Option<(u16, &'a str, &'a str)> {
        let rest = token.strip_prefix("PLANT:")?;
        let (id, mat) = rest.split_once(':').unwrap_or((rest, ""));
        self.by_id.get(id).map(|&i| (i, id, mat))
    }
}

/// What a tree tiletype is (precomputed per tiletype id in `prepare`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TreeTt {
    #[default]
    None,
    /// Trunk wall with connection mask (0 = pillar).
    Trunk(u8),
    /// Part of a thick (multi-tile) trunk, 3x3 position 0..=8 (4 = interior).
    Thick(u8),
    /// Directional (heavy) branch.
    Heavy(u8),
    /// Light branches (no direction).
    Branches,
    Twigs,
    CapWall(u8),
    CapThick(u8),
    CapPillar,
    CapFloor(u8),
    CapRamp,
    TrunkSlope,
}

impl TreeTt {
    pub(crate) fn is_tree_solid(self) -> bool {
        matches!(self, TreeTt::Trunk(_) | TreeTt::Thick(_) | TreeTt::Heavy(_) | TreeTt::Branches | TreeTt::TrunkSlope)
    }
    pub(crate) fn is_trunk(self) -> bool {
        matches!(self, TreeTt::Trunk(_) | TreeTt::Thick(_))
    }
}

/// Classify a tiletype by its DF name (the `direction` attribute has a few known typos in
/// DF's enum, e.g. `TreeCapWallThickS` says `N`, so the name is the source of truth).
pub(crate) fn classify(tt: &TiletypeInfo) -> TreeTt {
    if !matches!(tt.material, TtMaterial::Tree | TtMaterial::Mushroom) {
        return TreeTt::None;
    }
    let n = tt.name.as_str();
    let n = n.strip_prefix("TreeDead").map(|r| r).or_else(|| n.strip_prefix("Tree")).unwrap_or(n);
    let dirs = |s: &str| -> u8 {
        let mut m = 0;
        for c in s.chars() {
            m |= match c {
                'N' => 1,
                'S' => 2,
                'W' => 4,
                'E' => 8,
                _ => 0,
            };
        }
        m
    };
    match tt.shape {
        Shape::Twig => TreeTt::Twigs,
        Shape::Ramp => {
            if tt.material == TtMaterial::Mushroom {
                TreeTt::CapRamp
            } else {
                TreeTt::TrunkSlope
            }
        }
        Shape::Floor if tt.material == TtMaterial::Mushroom => TreeTt::CapFloor(tt.variant.clamp(1, 4) - 1),
        Shape::Branch => match n.strip_prefix("Branch") {
            Some("") => TreeTt::Heavy(15),
            Some(r) if !r.starts_with("es") => TreeTt::Heavy(dirs(r)),
            _ => TreeTt::Branches,
        },
        Shape::TrunkBranch => TreeTt::Trunk(dirs(n.strip_prefix("TrunkBranch").unwrap_or(""))),
        Shape::Wall => {
            if let Some(r) = n.strip_prefix("TrunkThick") {
                TreeTt::Thick(pos9(r).unwrap_or(4) as u8)
            } else if n == "TrunkInterior" {
                TreeTt::Thick(4)
            } else if n == "TrunkPillar" {
                TreeTt::Trunk(0)
            } else if let Some(r) = n.strip_prefix("Trunk") {
                TreeTt::Trunk(dirs(r))
            } else if let Some(r) = n.strip_prefix("CapWallThick") {
                TreeTt::CapThick(pos9(r).unwrap_or(4) as u8)
            } else if n == "CapInterior" {
                TreeTt::CapThick(4)
            } else if n == "CapPillar" {
                TreeTt::CapPillar
            } else if let Some(r) = n.strip_prefix("CapWall") {
                TreeTt::CapWall(dirs(r))
            } else if tt.material == TtMaterial::Mushroom {
                TreeTt::CapPillar
            } else {
                TreeTt::Trunk(0)
            }
        }
        _ => TreeTt::None,
    }
}

/// Is this a dead tree tile?
pub(crate) fn is_dead(tt: &TiletypeInfo) -> bool {
    matches!(tt.special, TtSpecial::Dead | TtSpecial::SmoothDead) || tt.name.starts_with("TreeDead")
}
