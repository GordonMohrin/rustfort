//! rf-gfx: Premium tileset loading and sprite lookup from the user's own DF install.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! Renderer-agnostic: rf-gfx decodes PNG pages to RGBA and answers "which sprites to draw";
//! rf-app uploads the pages as textures and draws the sprites in order (back to front).
//! The bodies below are stubs to be replaced by the real implementation.

use rf_core::{Building, Item, Pos, Unit, World};
use std::path::{Path, PathBuf};

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
}

#[derive(Debug)]
pub struct GfxError(pub String);

impl std::fmt::Display for GfxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for GfxError {}

#[derive(Default)]
pub struct Gfx {
    pub pages: Vec<PageInfo>,
}

impl Gfx {
    /// Parse the graphics raws of the DF install (vanilla + installed mods) and index pages.
    pub fn load(_df_dir: &Path) -> Result<Gfx, GfxError> {
        Ok(Gfx::default())
    }

    /// Decode a page's PNG into RGBA8 (width, height, pixels).
    pub fn page_rgba(&self, page: u16) -> Result<(u32, u32, Vec<u8>), GfxError> {
        Err(GfxError(format!("page {page} not loaded (stub)")))
    }

    /// Sprites for the terrain at `p` (floor/wall/ramp/stairs/liquid/designation overlay...),
    /// pushed back to front. Empty = the renderer draws its fallback.
    pub fn terrain(&self, _world: &World, _p: Pos, _out: &mut Vec<Sprite>) {}

    /// Sprites for a unit (layered body/clothing for dwarves), back to front.
    pub fn unit(&self, _world: &World, _u: &Unit, _out: &mut Vec<Sprite>) {}

    pub fn item(&self, _world: &World, _it: &Item, _out: &mut Vec<Sprite>) {}

    /// Sprites for the part of building `b` at map tile `p`.
    pub fn building(&self, _world: &World, _b: &Building, _p: Pos, _out: &mut Vec<Sprite>) {}

    /// Interface icon by name (e.g. toolbar buttons from `vanilla_interface`), if available.
    pub fn ui_icon(&self, _name: &str) -> Option<Sprite> {
        None
    }
}
