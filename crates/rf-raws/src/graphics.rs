//! Typed views over the graphics raws (`TILE_PAGE`, `TILE_GRAPHICS`, `PALETTE`, colors).

use crate::{RawObject, Raws, Token};
use std::collections::HashMap;
use std::path::PathBuf;

/// `[TILE_PAGE:ID] [FILE:..] [TILE_DIM:w:h] [PAGE_DIM_PIXELS:w:h] | [PAGE_DIM:tiles_w:tiles_h]`.
#[derive(Clone, Debug)]
pub struct TilePageDef {
    pub id: String,
    /// Absolute path of the PNG.
    pub file: PathBuf,
    pub tile_w: u32,
    pub tile_h: u32,
    /// Page size in pixels (from PAGE_DIM_PIXELS, or PAGE_DIM * TILE_DIM).
    pub width_px: u32,
    pub height_px: u32,
}

/// `[TILE_GRAPHICS:PAGE:x:y:NAME:extra...]`: a named hardcoded graphic (terrain, buildings, ...).
#[derive(Clone, Debug)]
pub struct TileGraphicDef {
    pub page: String,
    pub x: u16,
    pub y: u16,
    pub name: String,
    /// Extra arguments after the name (e.g. workshop `stage:tx:ty`, animation frame).
    pub extra: Vec<String>,
}

/// A `[PALETTE:ID]` (palette image: row 0 = colors used in the images, row N = replacements).
#[derive(Clone, Debug)]
pub struct PaletteDef {
    pub id: String,
    pub file: PathBuf,
    pub default_row: u32,
    /// Descriptor color token -> palette row.
    pub colors: HashMap<String, u32>,
}

/// `[COLOR:ID] [NAME:..] [RGB:r:g:b]` from descriptor_color raws.
#[derive(Clone, Debug)]
pub struct DescriptorColor {
    pub id: String,
    pub name: String,
    pub rgb: [u8; 3],
}

/// Indexed graphics raws.
#[derive(Clone, Debug, Default)]
pub struct GraphicsDb {
    pub pages: Vec<TilePageDef>,
    pub page_index: HashMap<String, usize>,
    pub tiles: Vec<TileGraphicDef>,
    /// name -> indices into `tiles` (several entries per name are possible, e.g. workshop stages).
    pub tile_index: HashMap<String, Vec<usize>>,
    pub palettes: Vec<PaletteDef>,
    pub colors: Vec<DescriptorColor>,
    pub color_index: HashMap<String, usize>,
}

fn u(t: &Token, i: usize) -> u32 {
    t.arg_i32(i).unwrap_or(0).max(0) as u32
}

impl GraphicsDb {
    pub fn from_raws(raws: &Raws) -> GraphicsDb {
        let mut db = GraphicsDb::default();
        for o in raws.of_kind("TILE_PAGE") {
            let dir = raws.file_dir(o);
            let mut p = TilePageDef { id: o.id.clone(), file: PathBuf::new(), tile_w: 32, tile_h: 32, width_px: 0, height_px: 0 };
            let mut dim_tiles = None;
            for t in &o.tokens[1..] {
                match t.name.as_str() {
                    "FILE" => p.file = dir.join(t.arg(0).replace('\\', "/")),
                    "TILE_DIM" => {
                        p.tile_w = u(t, 0);
                        p.tile_h = u(t, 1);
                    }
                    "PAGE_DIM_PIXELS" => {
                        p.width_px = u(t, 0);
                        p.height_px = u(t, 1);
                    }
                    "PAGE_DIM" => dim_tiles = Some((u(t, 0), u(t, 1))),
                    _ => {}
                }
            }
            if let (0, Some((w, h))) = (p.width_px, dim_tiles) {
                p.width_px = w * p.tile_w;
                p.height_px = h * p.tile_h;
            }
            match db.page_index.get(&p.id) {
                Some(&i) => db.pages[i] = p,
                None => {
                    db.page_index.insert(p.id.clone(), db.pages.len());
                    db.pages.push(p);
                }
            }
        }
        for o in raws.of_kind("TILE_GRAPHICS") {
            let h = o.header();
            let def = TileGraphicDef {
                page: h.arg(0).to_string(),
                x: h.arg_i32(1).unwrap_or(0).max(0) as u16,
                y: h.arg_i32(2).unwrap_or(0).max(0) as u16,
                name: h.arg(3).to_string(),
                extra: h.args.iter().skip(4).cloned().collect(),
            };
            db.tile_index.entry(def.name.clone()).or_default().push(db.tiles.len());
            db.tiles.push(def);
        }
        for o in raws.of_kind("PALETTE") {
            let dir = raws.file_dir(o);
            let mut p = PaletteDef { id: o.id.clone(), file: PathBuf::new(), default_row: 0, colors: HashMap::new() };
            for t in &o.tokens[1..] {
                match t.name.as_str() {
                    "FILE" => p.file = dir.join(t.arg(0)),
                    "PALETTE_DEFAULT" => p.default_row = u(t, 0),
                    "PALETTE_COLOR" => {
                        p.colors.insert(t.arg(0).to_string(), u(t, 1));
                    }
                    _ => {}
                }
            }
            db.palettes.push(p);
        }
        for o in raws.of_kind("COLOR") {
            if o.object_type != "DESCRIPTOR_COLOR" {
                continue;
            }
            let mut c = DescriptorColor { id: o.id.clone(), name: String::new(), rgb: [128; 3] };
            if let Some(t) = o.first("NAME") {
                c.name = t.arg(0).to_string();
            }
            if let Some(t) = o.first("RGB") {
                c.rgb = [u(t, 0) as u8, u(t, 1) as u8, u(t, 2) as u8];
            }
            db.color_index.insert(c.id.clone(), db.colors.len());
            db.colors.push(c);
        }
        db
    }

    pub fn page(&self, id: &str) -> Option<&TilePageDef> {
        self.page_index.get(id).map(|&i| &self.pages[i])
    }

    /// First tile graphic with this name.
    pub fn tile(&self, name: &str) -> Option<&TileGraphicDef> {
        self.tile_index.get(name).and_then(|v| v.first()).map(|&i| &self.tiles[i])
    }

    /// All tile graphics with this name.
    pub fn tiles_named<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a TileGraphicDef> + 'a {
        self.tile_index.get(name).into_iter().flat_map(move |v| v.iter().map(move |&i| &self.tiles[i]))
    }

    /// Descriptor color whose RGB is closest to `rgb` (exact match if the export used it).
    pub fn nearest_color(&self, rgb: [u8; 3]) -> Option<&DescriptorColor> {
        self.colors.iter().min_by_key(|c| {
            let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
            d(c.rgb[0], rgb[0]) + d(c.rgb[1], rgb[1]) + d(c.rgb[2], rgb[2])
        })
    }
}

/// Small helper for objects with `[STATE_COLOR:state:COLOR]` (inorganics, materials).
pub fn solid_color_token(obj: &RawObject) -> Option<&str> {
    let mut best = None;
    for t in obj.all("STATE_COLOR") {
        match t.arg(0) {
            "ALL_SOLID" | "SOLID" => return Some(t.arg(1)),
            "ALL" => best = Some(t.arg(1)),
            _ => {}
        }
    }
    best
}
