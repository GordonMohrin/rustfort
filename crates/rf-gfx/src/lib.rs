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
        g.db = db;
        g.terrain_t = terrain::Tables::build(&g);
        g.plants_t = plants::PlantTables::build(&g, raws);
        g.creatures = creature::parse_all(&mut g, raws);
        Ok(g)
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
        // Crops: planted seeds inside farm plots.
        self.crops_at.clear();
        let has_farms = world.buildings.iter().any(|b| b.kind == "FarmPlot");
        if has_farms {
            for it in world.items.iter().filter(|i| i.kind == "SEEDS" && i.flags.in_building) {
                let Some(pg) = self.plant_of_mat(it.mat) else { continue };
                // Without growth data the ripe crop is shown (estimate; DF shows CROP_SPROUT
                // until the plant has grown).
                if let Some(s) = pg.crop.or(pg.crop_sprout) {
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
    }
}
