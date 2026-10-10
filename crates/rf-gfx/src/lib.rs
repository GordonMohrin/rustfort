//! rf-gfx: Premium tileset loading and sprite lookup from the user's own DF install.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! Renderer-agnostic: rf-gfx decodes PNG pages to RGBA and answers "which sprites to draw";
//! rf-app uploads the pages as textures and draws the sprites in order (back to front).
//!
//! How it works (details in README.md):
//! - [`Gfx::load`] parses the graphics raws via `rf-raws` and registers one page per
//!   `[TILE_PAGE]`. DF Premium recolors many sprites with *palettes* (row 0 of a palette PNG
//!   holds the colors used in the images, row N the replacement for descriptor color N).
//!   We bake such recolorings into extra *variant pages*.
//! - [`Gfx::prepare`] creates the variant pages a world needs (stone/soil/wood colors,
//!   dwarf skin/hair/clothes). Call it after loading a world and again when new materials or
//!   units appear (cheap when nothing is new); then upload pages `>= old pages.len()`.
//!   Without `prepare`, lookups fall back to the base page plus a multiply tint.
//! - `terrain`/`unit`/`item`/`building` push sprites back to front without allocating.

mod creature;
mod plants;
mod terrain;

use rf_core::{Building, Item, MaterialInfo, Pos, Unit, World};
use rf_raws::{GraphicsDb, LoadOptions, Raws};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub use creature::UnitLook;

/// Default DF install path on Gordon's machine; override with env `RUSTFORT_DF_DIR`.
pub const DEFAULT_DF_DIR: &str = r"E:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress";

pub fn df_dir() -> PathBuf {
    std::env::var_os("RUSTFORT_DF_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DF_DIR))
}

/// One tile page (a PNG sprite sheet declared by `[TILE_PAGE:...]`).
#[derive(Clone, Debug)]
pub struct PageInfo {
    pub token: String,
    pub path: PathBuf,
    /// Size of one tile in pixels (usually 32x32).
    pub tile_w: u32,
    pub tile_h: u32,
    pub width_px: u32,
    pub height_px: u32,
}

/// One sprite to draw: a rectangle of tiles on a page, drawn at the map tile's position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    pub page: u16,
    /// Tile coordinates within the page.
    pub tx: u16,
    pub ty: u16,
    /// Size in tiles (1x1 for most; workshops are 3x3 drawn from their top-left tile).
    pub tw: u16,
    pub th: u16,
    /// Multiply color (material tint), white = none.
    pub tint: [u8; 4],
    /// Offset in map tiles relative to the tile being drawn (for multi-tile sprites).
    pub dx: i16,
    pub dy: i16,
}

impl Sprite {
    pub fn simple(page: u16, tx: u16, ty: u16) -> Sprite {
        Sprite { page, tx, ty, tw: 1, th: 1, tint: [255; 4], dx: 0, dy: 0 }
    }
    pub fn with_tint(mut self, tint: [u8; 4]) -> Sprite {
        self.tint = tint;
        self
    }
    pub fn at(mut self, dx: i16, dy: i16) -> Sprite {
        self.dx = dx;
        self.dy = dy;
        self
    }
    /// Horizontal pixel offset to center a multi-tile *unit* sprite on its tile (DF draws
    /// e.g. 2-tile-wide weapons and large creatures centered). Add it to the x position of
    /// sprites returned by [`Gfx::unit`]; it is 0 for 1-tile sprites.
    pub fn unit_center_px(&self, tile_w: u32) -> i32 {
        -((self.tw as i32 - 1) * tile_w as i32) / 2
    }
    /// Multiply the current tint by another one.
    pub fn mul_tint(mut self, t: [u8; 4]) -> Sprite {
        for i in 0..4 {
            self.tint[i] = ((self.tint[i] as u16 * t[i] as u16) / 255) as u8;
        }
        self
    }
}

#[derive(Debug)]
pub struct GfxError(pub String);

impl std::fmt::Display for GfxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for GfxError {}

/// A palette image: `rows[r][c]` is the color of column `c` in row `r` (row 0 = image colors).
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct PaletteImg {
    pub path: PathBuf,
    pub rows: Vec<Vec<[u8; 3]>>,
}

#[derive(Clone, Debug)]
struct PageSrc {
    /// Palette swap baked into this page: (palette index, row).
    palette: Option<(u16, u16)>,
}

/// Material class used to pick terrain/item families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MatClass {
    Stone,
    Soil,
    Sand,
    Wood,
    Metal,
    Glass,
    Other,
}

pub(crate) fn mat_class(m: Option<&MaterialInfo>) -> MatClass {
    let Some(m) = m else { return MatClass::Other };
    if m.is_wood || m.token.ends_with(":WOOD") {
        MatClass::Wood
    } else if m.is_metal {
        MatClass::Metal
    } else if m.is_glass || m.token.starts_with("GLASS") {
        MatClass::Glass
    } else if m.tags.iter().any(|t| t == "IS_SAND") || m.token.contains("SAND") && m.is_soil {
        MatClass::Sand
    } else if m.is_soil {
        MatClass::Soil
    } else if m.is_stone || m.is_ore || m.is_gem || m.token.starts_with("INORGANIC:") {
        MatClass::Stone
    } else {
        MatClass::Other
    }
}

#[derive(Default)]
pub struct Gfx {
    pub pages: Vec<PageInfo>,
    srcs: Vec<PageSrc>,
    page_ids: HashMap<String, u16>,
    pub(crate) db: GraphicsDb,
    pub(crate) palettes: Vec<PaletteImg>,
    pal_by_path: HashMap<PathBuf, u16>,
    /// The standard palette (`[PALETTE:DEFAULT]`, palettes.png).
    pub(crate) std_pal: Option<u16>,
    /// Descriptor color token -> row in the standard palette.
    pub(crate) std_rows: HashMap<String, u16>,
    /// (base page, palette, row) -> variant page.
    variants: HashMap<(u16, u16, u16), u16>,
    /// First sprite per TILE_GRAPHICS name (multi-tile names are combined into one sprite).
    pub(crate) named: HashMap<String, Sprite>,
    pub(crate) terrain_t: terrain::Tables,
    pub(crate) creatures: HashMap<String, creature::CreatureGfx>,
    /// Per world material palette index: standard palette row (filled by `prepare`).
    mat_rows: Vec<u16>,
    /// Per world material: which material classes the rows were made for.
    prepared_for: HashSet<(u16, u16)>,
    /// Item kind (+ material class) -> sprite, filled by `prepare` (`item()` must not allocate).
    item_cache: HashMap<String, [Option<Sprite>; 7]>,
    /// Unit id -> evaluated look (layers), filled by `prepare`.
    looks: HashMap<i32, creature::UnitLook>,
    pub df_dir: PathBuf,
    pub(crate) plants_t: plants::PlantTables,
    /// Per world tiletype id: tree classification (filled by `prepare`).
    pub(crate) tt_tree: Vec<plants::TreeTt>,
    /// Per world material: plant graphics index (`u16::MAX` = none), filled by `prepare`.
    pub(crate) mat_plant: Vec<u16>,
    /// Per world material: picked-growth item sprite (`PLANT:X:LEAF` -> leaves), by `prepare`.
    pub(crate) mat_growth: Vec<Option<Sprite>>,
    /// Farm plot tile -> crop sprite (planted seeds), rebuilt by `prepare`.
    pub(crate) crops_at: HashMap<Pos, Sprite>,
    /// (lower-case creature name, creature id), longest first.
    creature_names: Vec<(String, String)>,
    /// Item id -> corpse/remains sprite (filled by `prepare`).
    pub(crate) corpse_cache: HashMap<i32, Sprite>,
    /// Engraved tiles (tile index -> engraved floor?), rebuilt by `prepare` from `World::engravings`.
    pub(crate) engr_at: HashMap<u32, bool>,
    /// Non-vanilla mods whose graphics were loaded (`id:numeric_version`, load order).
    pub mods_loaded: Vec<String>,
    /// Mod list entries that could not be matched exactly (see [`Gfx::load_with_mods`]).
    pub mod_warnings: Vec<String>,
}

impl Gfx {
    /// Parse the graphics raws of the DF install (vanilla + installed mods) and index pages.
    ///
    /// Installed mods are not loaded yet (DF's active mod list lives in the save); set
    /// `RUSTFORT_GFX_MODS=1` to also load `data/installed_mods` (newest version per mod).
    pub fn load(df_dir: &Path) -> Result<Gfx, GfxError> {
        let include_mods = std::env::var("RUSTFORT_GFX_MODS").map(|v| v == "1").unwrap_or(false);
        let raws = Raws::load_df_dir(df_dir, &LoadOptions { include_mods, ..Default::default() })
            .map_err(|e| GfxError(format!("raws: {e}")))?;
        Self::from_raws(&raws, df_dir)
    }

    /// Load vanilla plus exactly the given active mods (`"id:numeric_version"` in load order,
    /// as in `World::meta.mods`), each from its matching version folder in
    /// `data/installed_mods`. An empty list loads vanilla only. Unmatched entries are listed
    /// in [`Gfx::mod_warnings`].
    pub fn load_with_mods(df_dir: &Path, mods: &[String]) -> Result<Gfx, GfxError> {
        if mods.is_empty() {
            let raws = Raws::load_df_dir(df_dir, &LoadOptions::default()).map_err(|e| GfxError(format!("raws: {e}")))?;
            return Self::from_raws(&raws, df_dir);
        }
        let res = rf_raws::resolve_mod_list(df_dir, mods);
        if !df_dir.join("data").join("vanilla").is_dir() {
            return Err(GfxError(format!("{}: no data/vanilla", df_dir.display())));
        }
        let raws = Raws::load_modules(&res.modules, &LoadOptions::default()).map_err(|e| GfxError(format!("raws: {e}")))?;
        let mut g = Self::from_raws(&raws, df_dir)?;
        g.mods_loaded = res.mods.iter().map(|m| format!("{}:{}", m.id, m.numeric_version)).collect();
        g.mod_warnings = res.warnings;
        Ok(g)
    }

    /// [`Gfx::load_with_mods`] with the world's active mod list (`world.meta.mods`).
    /// Use this in the app after loading a save so its mods (perspective_walls, ...) apply.
    pub fn load_for_world(df_dir: &Path, world: &World) -> Result<Gfx, GfxError> {
        Self::load_with_mods(df_dir, &world.meta.mods)
    }

    /// Build from already loaded raws (objects + graphics).
    pub fn from_raws(raws: &Raws, df_dir: &Path) -> Result<Gfx, GfxError> {
        let db = GraphicsDb::from_raws(raws);
        let mut g = Gfx { df_dir: df_dir.to_path_buf(), ..Default::default() };
        for p in &db.pages {
            let idx = g.pages.len() as u16;
            g.page_ids.insert(p.id.clone(), idx);
            g.pages.push(PageInfo {
                token: p.id.clone(),
                path: p.file.clone(),
                tile_w: p.tile_w.max(1),
                tile_h: p.tile_h.max(1),
                width_px: p.width_px,
                height_px: p.height_px,
            });
            g.srcs.push(PageSrc { palette: None });
        }
        // Standard palette.
        if let Some(pd) = db.palettes.iter().find(|p| p.id == "DEFAULT").or(db.palettes.first()) {
            if let Some(pi) = g.palette_index(&pd.file) {
                g.std_pal = Some(pi);
                for (k, &v) in &pd.colors {
                    g.std_rows.insert(k.clone(), v as u16);
                }
            }
        }
        // Named sprites: combine multi-part graphics (extra args "col:row") into one rectangle.
        let mut parts: HashMap<String, (u16, u16, u16, u16, u16, i32, i32)> = HashMap::new(); // page, minx, miny, maxx, maxy
        for t in &db.tiles {
            let Some(&page) = g.page_ids.get(&t.page) else { continue };
            let is_part = t.extra.len() == 2 && t.extra.iter().all(|e| e.parse::<i32>().is_ok());
            if is_part {
                let (c, r): (i32, i32) = (t.extra[0].parse().unwrap(), t.extra[1].parse().unwrap());
                let e = parts.entry(t.name.clone()).or_insert((page, u16::MAX, u16::MAX, 0, 0, i32::MAX, i32::MAX));
                if e.0 != page {
                    continue;
                }
                e.1 = e.1.min(t.x);
                e.2 = e.2.min(t.y);
                e.3 = e.3.max(t.x);
                e.4 = e.4.max(t.y);
                e.5 = e.5.min(c);
                e.6 = e.6.min(r);
            } else {
                g.named.entry(t.name.clone()).or_insert(Sprite::simple(page, t.x, t.y));
            }
        }
        for (name, (page, x0, y0, x1, y1, _, _)) in parts {
            g.named.entry(name).or_insert(Sprite { page, tx: x0, ty: y0, tw: x1 - x0 + 1, th: y1 - y0 + 1, tint: [255; 4], dx: 0, dy: 0 });
        }
        // [gfx6] Rectangles `[TILE_GRAPHICS_RECTANGLE:PAGE:x:y:w:h:NAME]` (procedural beasts).
        for o in raws.of_kind("TILE_GRAPHICS_RECTANGLE") {
            let h = o.header();
            let Some(&page) = g.page_ids.get(h.arg(0)) else { continue };
            let (Some(x), Some(y), Some(w), Some(hh)) = (h.arg_i32(1), h.arg_i32(2), h.arg_i32(3), h.arg_i32(4)) else { continue };
            if x < 0 || y < 0 || w < 1 || hh < 1 || o.id.is_empty() {
                continue;
            }
            g.named.entry(o.id.clone()).or_insert(Sprite { page, tx: x as u16, ty: y as u16, tw: w as u16, th: hh as u16, tint: [255; 4], dx: 0, dy: 0 });
        }
        g.db = db;
        g.terrain_t = terrain::Tables::build(&g);
        g.plants_t = plants::PlantTables::build(&g, raws);
        g.terrain_t.typed = terrain::typed_items(&g, raws);
        g.creatures = creature::parse_all(&mut g, raws);
        // Creature display names (singular, plural, child names) for corpses whose race the
        // export does not carry: longest names first so "giant cave spider" beats "spider".
        let mut names: Vec<(String, String)> = Vec::new();
        for o in raws.of_kind("CREATURE") {
            for tn in ["NAME", "GENERAL_CHILD_NAME", "CASTE_NAME"] {
                for t in o.all(tn) {
                    for a in t.args.iter().take(2) {
                        let a = a.trim().to_lowercase();
                        if a.len() >= 3 && !names.iter().any(|(n, id)| n == &a && id == &o.id) {
                            names.push((a, o.id.clone()));
                        }
                    }
                }
            }
        }
        names.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
        g.creature_names = names;
        Ok(g)
    }

    /// Race of a corpse-like item: its `subtype` if that is a creature token (exporters may
    /// put the race there), else the longest creature name found in its description
    /// (e.g. "stray horse skeleton" -> HORSE). The description match is a heuristic.
    fn corpse_race(&self, it: &Item) -> Option<&str> {
        if !it.subtype.is_empty() && self.creatures.contains_key(&it.subtype) {
            return self.creatures.get_key_value(&it.subtype).map(|(k, _)| k.as_str());
        }
        let d = it.desc.to_lowercase();
        let bytes = d.as_bytes();
        for (n, id) in &self.creature_names {
            let mut from = 0;
            while let Some(i) = d[from..].find(n.as_str()) {
                let s = from + i;
                let e = s + n.len();
                let ok_l = s == 0 || !bytes[s - 1].is_ascii_alphabetic();
                let ok_r = e >= bytes.len() || !bytes[e].is_ascii_alphabetic();
                if ok_l && ok_r {
                    return Some(id.as_str());
                }
                from = s + 1;
                if from >= d.len() {
                    break;
                }
            }
        }
        None
    }

    /// Tree classification of a tiletype id (needs `prepare`).
    #[inline]
    pub(crate) fn tree_tt(&self, tt: u16) -> plants::TreeTt {
        self.tt_tree.get(tt as usize).copied().unwrap_or_default()
    }

    #[inline]
    pub(crate) fn tree_tt_at(&self, world: &World, p: Pos) -> plants::TreeTt {
        match world.tile(p) {
            Some(t) if !t.hidden() => self.tree_tt(t.tt),
            _ => plants::TreeTt::None,
        }
    }

    /// Is the tile at `p` engraved? `Some(true)` = engraved floor, `Some(false)` = engraved
    /// wall (needs `prepare`, which indexes `World::engravings`).
    #[inline]
    pub(crate) fn engraved_at(&self, world: &World, p: Pos) -> Option<bool> {
        if self.engr_at.is_empty() {
            return None;
        }
        world.idx(p).and_then(|i| self.engr_at.get(&(i as u32)).copied())
    }

    /// Plant graphics index of a world material (`u16::MAX` = none; needs `prepare`).
    #[inline]
    pub(crate) fn plant_index_of_mat(&self, mat: u16) -> u16 {
        self.mat_plant.get(mat as usize).copied().unwrap_or(u16::MAX)
    }

    /// Plant graphics of a world material (needs `prepare`).
    #[inline]
    pub(crate) fn plant_of_mat(&self, mat: u16) -> Option<&plants::PlantGfx> {
        match self.mat_plant.get(mat as usize) {
            Some(&i) if i != u16::MAX => self.plants_t.plants.get(i as usize),
            _ => None,
        }
    }

    pub(crate) fn page_id(&self, token: &str) -> Option<u16> {
        self.page_ids.get(token).copied()
    }

    /// Load (or reuse) a palette image; returns its index.
    pub(crate) fn palette_index(&mut self, path: &Path) -> Option<u16> {
        if let Some(&i) = self.pal_by_path.get(path) {
            return Some(i);
        }
        let img = image::open(path).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let mut rows = Vec::with_capacity(h as usize);
        for y in 0..h {
            rows.push((0..w).map(|x| {
                let p = img.get_pixel(x, y).0;
                [p[0], p[1], p[2]]
            }).collect());
        }
        let i = self.palettes.len() as u16;
        self.palettes.push(PaletteImg { path: path.to_path_buf(), rows });
        self.pal_by_path.insert(path.to_path_buf(), i);
        Some(i)
    }

    /// Number of pages (base + variants created so far).
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Page index by `[TILE_PAGE]` token.
    pub fn page_by_token(&self, token: &str) -> Option<u16> {
        self.page_id(token)
    }

    /// Decode a page's PNG into RGBA8 (width, height, pixels). Variant pages get their
    /// palette swap applied.
    pub fn page_rgba(&self, page: u16) -> Result<(u32, u32, Vec<u8>), GfxError> {
        let info = self.pages.get(page as usize).ok_or_else(|| GfxError(format!("no page {page}")))?;
        let img = image::open(&info.path).map_err(|e| GfxError(format!("{}: {e}", info.path.display())))?.to_rgba8();
        let (w, h) = img.dimensions();
        let mut px = img.into_raw();
        if let Some((pal, row)) = self.srcs[page as usize].palette {
            if let Some(p) = self.palettes.get(pal as usize) {
                if let (Some(src), Some(dst)) = (p.rows.first(), p.rows.get(row as usize)) {
                    // Small linear lookup with a one-entry cache (palettes have ~18 colors).
                    let mut last: ([u8; 3], Option<[u8; 3]>) = ([0, 0, 0], None);
                    let mut have_last = false;
                    for c in px.chunks_exact_mut(4) {
                        if c[3] == 0 {
                            continue;
                        }
                        let k = [c[0], c[1], c[2]];
                        let rep = if have_last && last.0 == k {
                            last.1
                        } else {
                            let r = src.iter().position(|s| *s == k).and_then(|i| dst.get(i).copied());
                            last = (k, r);
                            have_last = true;
                            r
                        };
                        if let Some(r) = rep {
                            c[0] = r[0];
                            c[1] = r[1];
                            c[2] = r[2];
                        }
                    }
                }
            }
        }
        Ok((w, h, px))
    }

    /// Get or create the palette variant of `base` (palette index, row).
    pub(crate) fn make_variant(&mut self, base: u16, pal: u16, row: u16) -> u16 {
        if let Some(&v) = self.variants.get(&(base, pal, row)) {
            return v;
        }
        let mut info = self.pages[base as usize].clone();
        info.token = format!("{}#p{}r{}", info.token, pal, row);
        let idx = self.pages.len() as u16;
        self.pages.push(info);
        self.srcs.push(PageSrc { palette: Some((pal, row)) });
        self.variants.insert((base, pal, row), idx);
        idx
    }

    /// Look up an existing variant (no creation).
    pub(crate) fn variant(&self, base: u16, pal: u16, row: u16) -> Option<u16> {
        self.variants.get(&(base, pal, row)).copied()
    }

    /// Standard palette row for a material (by its descriptor color RGB).
    pub(crate) fn std_row_for_rgb(&self, rgb: [u8; 3]) -> u16 {
        match self.db.nearest_color(rgb) {
            Some(c) => self.std_rows.get(&c.id).copied().unwrap_or(0),
            None => 0,
        }
    }

    pub(crate) fn mat_row(&self, world: &World, mat: u16) -> u16 {
        if let Some(&r) = self.mat_rows.get(mat as usize) {
            return r;
        }
        world.material(mat).map(|m| self.std_row_for_rgb(m.rgb)).unwrap_or(0)
    }

    /// Recolor a sprite with a standard palette row (baked variant page if `prepare` made one,
    /// else unchanged).
    #[inline]
    pub(crate) fn recolor_row(&self, s: Sprite, row: u16) -> Sprite {
        match self.std_pal {
            Some(pal) if row != 0 => match self.variant(s.page, pal, row) {
                Some(v) => Sprite { page: v, ..s },
                None => s,
            },
            _ => s,
        }
    }

    /// Recolor a sprite with a world material through the standard palette: uses the baked
    /// variant page if `prepare` made one, else a multiply tint (approximation).
    pub(crate) fn recolor(&self, s: Sprite, world: &World, mat: u16) -> Sprite {
        let Some(pal) = self.std_pal else { return s };
        if mat == rf_core::NO_MAT {
            return s;
        }
        let row = self.mat_row(world, mat);
        if row == 0 {
            return s;
        }
        match self.variant(s.page, pal, row) {
            Some(v) => Sprite { page: v, ..s },
            None => {
                // Approximate: tint = palette row's mid color / default mid color.
                let p = &self.palettes[pal as usize];
                let (Some(r0), Some(rn)) = (p.rows.first(), p.rows.get(row as usize)) else { return s };
                let c = 13.min(r0.len().saturating_sub(1));
                let t = |i: usize| ((rn[c][i] as u32 * 255) / (r0[c][i].max(1) as u32)).min(255) as u8;
                s.mul_tint([t(0), t(1), t(2), 255])
            }
        }
    }

    /// Create the variant pages `world` needs and cache per-unit looks and per-item sprites.
    /// Returns the number of new pages. Idempotent and cheap when nothing changed.
    pub fn prepare(&mut self, world: &World) -> usize {
        let before = self.pages.len();
        self.mat_rows = world.materials.iter().map(|m| self.std_row_for_rgb(m.rgb)).collect();
        // Plants and trees.
        if self.tt_tree.len() != world.tiletypes.infos.len() {
            let max = world.tiletypes.infos.iter().map(|i| i.id as usize + 1).max().unwrap_or(0);
            let mut v = vec![plants::TreeTt::None; max];
            for i in &world.tiletypes.infos {
                v[i.id as usize] = plants::classify(i);
            }
            self.tt_tree = v;
        }
        if self.mat_plant.len() != world.materials.len() {
            self.mat_plant = Vec::with_capacity(world.materials.len());
            self.mat_growth = Vec::with_capacity(world.materials.len());
            for m in &world.materials {
                let (pi, growth) = match self.plants_t.plant_of_token(&m.token) {
                    Some((pi, id, local)) => {
                        let gid = self.plants_t.growth_mats.get(id).and_then(|v| v.iter().find(|(lm, _)| lm == local)).map(|(_, g)| g.as_str());
                        let pg = &self.plants_t.plants[pi as usize];
                        let gs = gid.and_then(|gid| pg.growths.iter().find(|(g, _)| g == gid)).map(|(_, s)| *s);
                        (pi, gs)
                    }
                    None => (u16::MAX, None),
                };
                self.mat_plant.push(pi);
                self.mat_growth.push(growth);
            }
        }
        // Engravings (DF keeps the smooth tiletype; the engraving lies on top).
        self.engr_at.clear();
        for e in &world.engravings {
            if let Some(i) = world.idx(e.pos) {
                self.engr_at.insert(i as u32, e.floor);
            }
        }
        // Crops: planted seeds inside farm plots.
        self.crops_at.clear();
        let has_farms = world.buildings.iter().any(|b| b.kind == "FarmPlot");
        if has_farms {
            for it in world.items.iter().filter(|i| i.kind == "SEEDS" && i.flags.in_building) {
                // Without growth data the ripe crop is shown (estimate; DF shows CROP_SPROUT
                // until the plant has grown). Plants without crop graphics: FARMPLOT_PLANTED.
                let s = self.plant_of_mat(it.mat).and_then(|pg| pg.crop.or(pg.crop_sprout)).or(self.terrain_t.bld.farmplot_planted);
                if let Some(s) = s {
                    self.crops_at.insert(it.pos, s);
                }
            }
        }
        let Some(pal) = self.std_pal else { return 0 };
        // Which (material, class-specific page set) combinations are used?
        let mut need: HashSet<(u16, u8)> = HashSet::new(); // (mat, kind 0 terrain 1 item 2 building)
        for t in &world.tiles {
            if t.mat != rf_core::NO_MAT && !t.hidden() {
                need.insert((t.mat, 0));
            }
        }
        for it in &world.items {
            need.insert((it.mat, 1));
        }
        for b in &world.buildings {
            need.insert((b.mat, 2));
        }
        let mut pages_for: Vec<(u16, u16)> = Vec::new();
        for &(mat, kind) in &need {
            let row = self.mat_rows.get(mat as usize).copied().unwrap_or(0);
            if row == 0 || !self.prepared_for.insert((mat, kind as u16)) {
                continue;
            }
            let class = mat_class(world.material(mat));
            let list: &[u16] = match kind {
                0 => self.terrain_t.palette_pages(class),
                1 => &self.terrain_t.item_pages,
                _ => &self.terrain_t.building_pages,
            };
            for &p in list {
                pages_for.push((p, row));
            }
        }
        for (p, row) in pages_for {
            self.make_variant(p, pal, row);
        }
        // [gfx6] Blossom and fruit colors of the world's trees (palette pages per growth color).
        let mut seen: HashSet<u16> = HashSet::new();
        let mut want: Vec<(u16, u16)> = Vec::new();
        for &pi in &self.mat_plant {
            if pi == u16::MAX || !seen.insert(pi) {
                continue;
            }
            let pg = &self.plants_t.plants[pi as usize];
            let Some(ss) = self.plants_t.season.get(pg.tree as usize).or(self.plants_t.season.first()) else { continue };
            for row in [pg.flowers.as_ref().map(|f| f.1.row), pg.fruit.as_ref().map(|f| f.1.row)].into_iter().flatten() {
                if row != 0 {
                    for &p in &ss.pages {
                        want.push((p, row));
                    }
                }
            }
        }
        for (p, row) in want {
            self.make_variant(p, pal, row);
        }
        // Corpses, body parts and remains: the creature's own sprite.
        for it in world.items.iter().filter(|i| matches!(i.kind.as_str(), "CORPSE" | "CORPSEPIECE" | "REMAINS")) {
            if self.corpse_cache.contains_key(&it.id) {
                continue;
            }
            let cg = self.corpse_race(it).and_then(|r| self.creatures.get(r));
            let s = match (it.kind.as_str(), cg) {
                ("REMAINS", Some(c)) => c.remains.or(c.corpse),
                (_, Some(c)) => c.corpse.or(c.remains),
                _ => None,
            }
            .or(self.terrain_t.remains);
            if let Some(s) = s {
                self.corpse_cache.insert(it.id, s);
            }
        }
        // Item sprite cache.
        let kinds: HashSet<String> = world.items.iter().map(|i| i.kind.clone()).chain(world.buildings.iter().map(|b| b.kind.to_uppercase())).collect();
        for k in kinds {
            if !self.item_cache.contains_key(&k) {
                let v = terrain::item_sprites_for_kind(self, &k);
                self.item_cache.insert(k, v);
            }
        }
        // Unit looks.
        for u in &world.units {
            if self.looks.contains_key(&u.id) {
                continue;
            }
            if !self.creatures.contains_key(&u.race) {
                // [gfx6] Procedural creatures (forgotten beasts, demons, ...): DF's beast sprites.
                if let Some(sprites) = creature::procedural_look(self, world, u) {
                    self.looks.insert(u.id, creature::UnitLook { sprites });
                }
                continue;
            }
            if let Some(layers) = creature::evaluate(self, world, u) {
                let mut look = creature::UnitLook::default();
                for (s, pal) in layers {
                    let s = match pal {
                        Some((pl, row)) => Sprite { page: self.make_variant(s.page, pl, row), ..s },
                        None => s,
                    };
                    look.sprites.push(s);
                }
                self.looks.insert(u.id, look);
            }
        }
        self.pages.len() - before
    }

    pub(crate) fn item_cache(&self, kind: &str) -> Option<&[Option<Sprite>; 7]> {
        self.item_cache.get(kind)
    }

    /// Sprites for the terrain at `p` (floor/wall/ramp/stairs/liquid/designation overlay...),
    /// pushed back to front. Empty = the renderer draws its fallback.
    pub fn terrain(&self, world: &World, p: Pos, out: &mut Vec<Sprite>) {
        terrain::terrain(self, world, p, out)
    }

    /// Sprites for a unit (layered body/clothing for dwarves), back to front.
    pub fn unit(&self, world: &World, u: &Unit, out: &mut Vec<Sprite>) {
        creature::unit(self, world, u, out)
    }

    pub fn item(&self, world: &World, it: &Item, out: &mut Vec<Sprite>) {
        terrain::item(self, world, it, out)
    }

    /// Sprites for the part of building `b` at map tile `p`.
    pub fn building(&self, world: &World, b: &Building, p: Pos, out: &mut Vec<Sprite>) {
        terrain::building(self, world, b, p, out)
    }

    /// [round 5] The occupied cage a caught creature sits in (`ITEM_CAGE_*_OCCUPIED`), in the
    /// cage's material (`mat` = the cage item's or the cage trap's material; stone and unknown
    /// materials use the wooden look like loose cages do). The renderer draws it under the
    /// creature, which it draws a little smaller so the frame stays visible, then the bars.
    pub fn cage_sprite(&self, world: &World, mat: u16) -> Option<Sprite> {
        let fam = match mat_class(world.material(mat)) {
            MatClass::Metal => 1,
            MatClass::Glass => 2,
            _ => 0,
        };
        let s = self.terrain_t.bld.cage_occupied[fam].or(self.terrain_t.bld.cage_occupied[0])?;
        Some(self.recolor(s, world, mat))
    }

    /// The item designation marker DF draws over an item (`DESIGNATION_ITEM_*`): forbidden,
    /// dump, melt, the forbidden+dump / forbidden+melt combinations, or hidden. `None` for an
    /// unmarked item. Draw it on top of the item's sprites.
    pub fn item_marker(&self, it: &Item) -> Option<Sprite> {
        let m = &self.terrain_t.item_marks;
        let f = &it.flags;
        match (f.forbid, f.dump, f.melt) {
            (true, true, _) => m[2].or(m[0]),
            (true, false, true) => m[4].or(m[0]),
            (true, false, false) => m[0],
            (false, true, _) => m[1],
            (false, false, true) => m[3],
            _ if f.hidden => m[5],
            _ => None,
        }
    }

    /// Footprint (w, h) in tiles of a building kind/subtype as DF builds it: 3x3 workshops and
    /// furnaces, 1x1 quern/millstone, 5x5 trade depot and siege workshop, 1x1 otherwise
    /// (variable-size buildings like bridges and farm plots report 1x1; the user drags them).
    pub fn building_size(&self, kind: &str, subtype: &str) -> (i16, i16) {
        let key = match kind {
            "Workshop" | "Furnace" => subtype,
            "TradeDepot" => "TradeDepot",
            _ => return (1, 1),
        };
        match self.terrain_t.workshops.get(key) {
            Some(w) => (w.w as i16, w.h as i16),
            None if key == "TradeDepot" || key == "Siege" => (5, 5),
            None if key == "Quern" || key == "Millstone" => (1, 1),
            None => (3, 3),
        }
    }

    /// Sprites of a fully built building for placement previews (Build menu ghost), with
    /// `dx`/`dy` relative to the footprint's top-left tile (workshop roofs reach `dy = -1`).
    /// Drawn in the base (unrecolored) look; the renderer applies its own ghost tint/alpha,
    /// e.g. `s.mul_tint([255, 255, 255, 150])` (red for an invalid spot). Not for the per-tile
    /// hot path (it allocates); cache it per selected building.
    pub fn building_ghost(&self, kind: &str, subtype: &str) -> Vec<Sprite> {
        let (w, h) = self.building_size(kind, subtype);
        let world = World::default();
        let b = Building {
            kind: kind.to_string(),
            subtype: subtype.to_string(),
            x1: 0,
            y1: 0,
            x2: w - 1,
            y2: h - 1,
            mat: rf_core::NO_MAT,
            stage: -1,
            built: true,
            owner: -1,
            ..Default::default()
        };
        let mut out = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let s0 = out.len();
                terrain::building(self, &world, &b, Pos::new(x, y, 0), &mut out);
                for s in &mut out[s0..] {
                    s.dx += x;
                    s.dy += y;
                }
            }
        }
        if out.is_empty() {
            // Furniture whose item sprite was not cached by `prepare` (none in the world yet).
            let set = terrain::item_sprites_for_kind(self, &kind.to_uppercase());
            out.extend(set[0].or(set[3]).or(set[4]).or(set[6]));
        }
        out
    }

    /// Build-menu icon for a building kind/subtype (`BUILDING_ICON_*` from the interface
    /// graphics, 8x12-px tiles combined into one sprite), e.g. `("Workshop", "Masons")` ->
    /// `BUILDING_ICON_WORKSHOP_MASON`, `("Trap", "CageTrap")` -> `BUILDING_ICON_TRAP_CAGE`.
    /// Category icons are plain names for [`Gfx::ui_icon`]: `BUILDING_ICON_WORKSHOPS`,
    /// `_WORKSHOPS_FURNACES`, `_WORKSHOPS_FARMING`, `_WORKSHOPS_CLOTHING`, `_FURNITURE`,
    /// `_DOORS_HATCHES`, `_WALLS_FLOORS`, `_MILITARY`, `_TRAPS`, `_MACHINES_FLUIDS`,
    /// `_CAGES_RESTRAINT`, `_DISPLAY_FURNITURE`.
    pub fn building_icon(&self, kind: &str, subtype: &str) -> Option<Sprite> {
        let ic = |n: &str| self.named.get(&format!("BUILDING_ICON_{n}")).copied();
        match kind {
            "Workshop" | "Furnace" | "TradeDepot" => {
                let key = if kind == "TradeDepot" { "TradeDepot" } else { subtype };
                let gfx = terrain::workshop_gfx_name(key)?;
                if gfx == "TRADE_DEPOT" {
                    return ic("TRADE_DEPOT");
                }
                ic(gfx)
                    .or_else(|| ic(gfx.trim_end_matches("_LAVA")))
                    .or_else(|| if gfx == "WORKSHOP_MAGMAFORGE" { ic("WORKSHOP_METALSMITH") } else { None })
            }
            "Trap" => ic(match subtype {
                "CageTrap" => "TRAP_CAGE",
                "StoneFallTrap" => "TRAP_STONE",
                "WeaponTrap" => "TRAP_WEAPON",
                "Lever" => "LEVER",
                "PressurePlate" => "PRESSURE_PLATE",
                "TrackStop" => "TRACK_STOP",
                _ => "TRAPS",
            }),
            "Armorstand" => ic("ARMOR_STAND"),
            "Weaponrack" => ic("WEAPON_RACK"),
            "Chain" => ic("RESTRAINT"),
            "BarsFloor" => ic("BARS_FLOORS"),
            "Construction" => ic(match subtype {
                "Floor" => "FLOOR",
                "Ramp" => "RAMP",
                "Fortification" => "FORTIFICATION",
                s if s.starts_with("Stair") || s.starts_with("Up") || s.starts_with("Down") => "STAIRS",
                "ReinforcedWall" => "REINFORCED_WALL",
                "Track" | "TrackN" | "TrackS" | "TrackE" | "TrackW" => "TRACK",
                _ => "WALL",
            }),
            k => {
                // CamelCase -> SNAKE_CASE (TractionBench -> TRACTION_BENCH).
                let mut snake = String::new();
                for (i, c) in k.chars().enumerate() {
                    if c.is_ascii_uppercase() && i > 0 {
                        snake.push('_');
                    }
                    snake.push(c.to_ascii_uppercase());
                }
                ic(&snake)
            }
        }
    }

    /// Interface icon by name (e.g. toolbar buttons from `vanilla_interface`), if available.
    /// Any `TILE_GRAPHICS` name works; multi-tile buttons (e.g. `BUTTON_DIG`, 4x3 tiles of
    /// 8x12 px) come back as one sprite with `tw`/`th` set. See README.md for names.
    pub fn ui_icon(&self, name: &str) -> Option<Sprite> {
        self.named.get(name).copied()
    }

    /// Any named tile graphic (`TILE_GRAPHICS` name), e.g. `STONE_FLOOR_5`.
    pub fn named(&self, name: &str) -> Option<Sprite> {
        self.named.get(name).copied()
    }

    /// Debug info for a race: (default sprite, number of layer sets, layers per set).
    pub fn creature_info(&self, race: &str) -> Option<(Option<Sprite>, Vec<(String, usize)>)> {
        self.creatures.get(race).map(|c| {
            (c.default, c.sets.iter().map(|s| (s.state.clone(), s.entries.len())).collect())
        })
    }

    pub(crate) fn look(&self, id: i32) -> Option<&creature::UnitLook> {
        self.looks.get(&id)
    }
}

/// Cheap deterministic hash of a position (for picking sprite variants).
#[inline]
pub(crate) fn hash_pos(p: Pos, salt: u32) -> u32 {
    let mut h = (p.x as u32).wrapping_mul(0x9E37_79B1) ^ (p.y as u32).wrapping_mul(0x85EB_CA77) ^ (p.z as u32).wrapping_mul(0xC2B2_AE3D) ^ salt.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_install_if_present() {
        let dir = df_dir();
        if !dir.join("data").join("vanilla").exists() {
            eprintln!("DF install not found, skipping");
            return;
        }
        let mut g = Gfx::load(&dir).expect("load");
        assert!(g.pages.len() > 100);
        assert!(g.ui_icon("BUTTON_DIG").is_some());
        let w = World::synthetic(32, 32, 12);
        g.prepare(&w);
        let mut out = Vec::new();
        for z in 0..w.dims.z as i16 {
            for y in 0..32 {
                for x in 0..32 {
                    g.terrain(&w, Pos::new(x, y, z), &mut out);
                }
            }
        }
        assert!(!out.is_empty());
        out.clear();
        let dwarf = w.units.iter().find(|u| u.race == "DWARF").unwrap();
        g.unit(&w, dwarf, &mut out);
        assert!(out.len() >= 3, "dwarf should have several layers, got {}", out.len());
        let (pw, ph, px) = g.page_rgba(out[0].page).unwrap();
        assert_eq!(px.len(), (pw * ph * 4) as usize);
        // A floor next to a wall gets a wall shadow.
        let shadow_page = g.page_by_token("SHADOWS_WALL").expect("shadow page");
        let mut found = false;
        'outer: for z in 0..w.dims.z as i16 {
            for y in 1..31 {
                for x in 1..31 {
                    let p = Pos::new(x, y, z);
                    if w.shape(p) == rf_core::Shape::Floor && w.shape(p.offset(0, -1, 0)) == rf_core::Shape::Wall {
                        out.clear();
                        g.terrain(&w, p, &mut out);
                        found = out.iter().any(|s| s.page == shadow_page);
                        break 'outer;
                    }
                }
            }
        }
        assert!(found, "wall shadow expected on a floor south of a wall");
        // Ghosts for the build menu.
        let ghost = g.building_ghost("Workshop", "Masons");
        assert!(ghost.len() >= 9, "3x3 workshop ghost, got {}", ghost.len());
        assert_eq!(g.building_size("TradeDepot", ""), (5, 5));
        assert_eq!(g.building_size("Workshop", "Quern"), (1, 1));
        assert!(!g.building_ghost("Bed", "").is_empty());
    }

    /// Round 6: engravings, item markers, seasons, procedural beasts, caste sprites, webs.
    #[test]
    fn round6_lookups() {
        let dir = df_dir();
        if !dir.join("data").join("vanilla").exists() {
            eprintln!("DF install not found, skipping");
            return;
        }
        let mut g = Gfx::load(&dir).expect("load");
        let mut w = World::synthetic(32, 32, 12);
        // A smoothed floor and wall pair in the hall, both engraved.
        let hall = w.units[0].pos;
        let fl = Pos::new(hall.x, hall.y + 1, hall.z);
        let wall = (0..32).map(|x| Pos::new(x, fl.y, fl.z)).find(|&p| w.shape(p) == rf_core::Shape::Wall && w.shape(p.offset(1, 0, 0)) == rf_core::Shape::Floor).expect("hall wall");
        let smooth_floor = w.tiletypes.by_name("StoneFloorSmooth").unwrap();
        w.tile_mut(fl).unwrap().tt = smooth_floor;
        let mut infos = w.tiletypes.infos.clone();
        let id = infos.len() as u16;
        infos.push(rf_core::TiletypeInfo { id, name: "StoneWallSmoothUD".into(), shape: rf_core::Shape::Wall, material: rf_core::TtMaterial::Stone, special: rf_core::TtSpecial::Smooth, variant: 0, direction: String::new() });
        w.tiletypes = rf_core::TiletypeTable::new(infos);
        w.tile_mut(wall).unwrap().tt = id;
        w.engravings.push(rf_core::Engraving { pos: fl, quality: 2, floor: true, mat: 0, engraver: -1, skill: 3 });
        w.engravings.push(rf_core::Engraving { pos: wall, quality: 2, floor: false, mat: 0, engraver: -1, skill: 3 });
        // A procedural beast and a hen.
        let mut fb = w.units[0].clone();
        fb.id = 900;
        fb.race = "FORGOTTEN_BEAST_3".into();
        fb.flags.animal = true;
        w.units.push(fb);
        let mut hen = w.units.iter().find(|u| u.race == "CAT").unwrap().clone();
        hen.id = 901;
        hen.race = "BIRD_CHICKEN".into();
        hen.caste = "FEMALE".into();
        w.units.push(hen.clone());
        hen.id = 902;
        hen.caste = "MALE".into();
        w.units.push(hen);
        g.prepare(&w);
        let mut out = Vec::new();
        g.terrain(&w, fl, &mut out);
        let eng_pages = [g.page_by_token("FLOOR_STONE_ENGRAVED_PALETTE"), g.page_by_token("FLOOR_STONE_ENGRAVED_NON_PALETTE")];
        let base = |p: u16| g.pages[p as usize].token.split('#').next().unwrap().to_string();
        assert!(out.iter().any(|s| eng_pages.iter().flatten().any(|&e| base(s.page) == base(e))), "engraved floor sprite expected: {out:?}");
        out.clear();
        g.terrain(&w, wall, &mut out);
        assert!(out.iter().any(|s| base(s.page) == "WALL_STONE_ENGRAVED"), "engraved wall pieces expected");
        // Melt marker.
        let mut it = w.items[0].clone();
        assert!(g.item_marker(&it).is_none());
        it.flags.melt = true;
        assert_eq!(g.item_marker(&it), g.named("DESIGNATION_ITEM_MELT"));
        it.flags.forbid = true;
        assert_eq!(g.item_marker(&it), g.named("DESIGNATION_ITEM_FORBIDDEN_MELT"));
        // Webs.
        it.kind = "THREAD".into();
        it.desc = "cave spider silk web".into();
        out.clear();
        g.item(&w, &it, &mut out);
        assert_eq!(out.first().map(|s| g.pages[s.page as usize].token.as_str()), Some("ITEM_WEBS"));
        // Seasons of an oak: green in spring, autumn colors, bare in winter.
        let oak = &g.plants_t.plants[g.plants_t.by_id["OAK"] as usize];
        assert_eq!(oak.season(100_000).leaves, 0);
        assert!((1..=3).contains(&oak.season(250_000).leaves));
        assert_eq!(oak.season(350_000).leaves, 4);
        let cherry = &g.plants_t.plants[g.plants_t.by_id["CHERRY"] as usize];
        assert!(cherry.season(80_000).flowers > 0, "cherry blossoms in spring");
        assert!(cherry.season(150_000).fruit > 0, "cherries in summer");
        // Procedural beast: a 3x2 BEAST sprite; chickens by caste.
        out.clear();
        g.unit(&w, w.units.iter().find(|u| u.id == 900).unwrap(), &mut out);
        assert!(out.iter().any(|s| s.tw == 3 && s.th == 2), "beast sprite expected: {out:?}");
        let mut a = Vec::new();
        let mut b = Vec::new();
        g.unit(&w, w.units.iter().find(|u| u.id == 901).unwrap(), &mut a);
        g.unit(&w, w.units.iter().find(|u| u.id == 902).unwrap(), &mut b);
        assert!(!a.is_empty() && !b.is_empty() && a != b, "hen and rooster differ");
    }
}
