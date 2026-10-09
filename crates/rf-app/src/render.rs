//! Map drawing: terrain of the current z-level (with the levels below visible through open
//! space, fogged like DF), designations, stockpiles, buildings, items, units, cursor.
//!
//! Sprites come from rf-gfx (Premium tileset); wherever it returns nothing we draw a clean
//! fallback from material colors and DF's CP437 glyphs.

use crate::font::Font;
use crate::game::{Game, Selected, Tool, STOCK_PRESETS};
use macroquad::prelude::*;
use rf_core::{Building, Dig, Item, Pos, Shape, Tile, TtMaterial, TtSpecial, Unit, World, NO_MAT};
use rf_gfx::{Gfx, Sprite};

/// How many levels below open space we look through.
const LOOK_DOWN: i16 = 4;
const FOG: [f32; 5] = [0.0, 0.42, 0.60, 0.72, 0.82];

#[derive(Default)]
pub struct MapRenderer {
    tex: Vec<Option<Texture2D>>,
    tried: Vec<bool>,
    pub buf: Vec<Sprite>,
}

impl MapRenderer {
    /// Texture of a gfx page, uploaded on first use (nearest filtering).
    pub fn texture(&mut self, gfx: &Gfx, page: u16) -> Option<Texture2D> {
        let i = page as usize;
        if i >= self.tex.len() {
            self.tex.resize(i + 1, None);
            self.tried.resize(i + 1, false);
        }
        if !self.tried[i] {
            self.tried[i] = true;
            match gfx.page_rgba(page) {
                Ok((w, h, px)) if w > 0 && h > 0 && w <= u16::MAX as u32 && h <= u16::MAX as u32 => {
                    let t = Texture2D::from_rgba8(w as u16, h as u16, &px);
                    t.set_filter(FilterMode::Nearest);
                    self.tex[i] = Some(t);
                }
                Ok(_) => {}
                Err(e) => eprintln!("gfx page {page}: {e}"),
            }
        }
        self.tex[i].clone()
    }

    /// Draw a map sprite whose (0,0) tile lands at screen (x, y); `tp` = screen size of one map tile.
    pub fn draw_sprite(&mut self, gfx: &Gfx, s: &Sprite, x: f32, y: f32, tp: f32) -> bool {
        let Some(info) = gfx.pages.get(s.page as usize) else { return false };
        let (tw, th) = (info.tile_w as f32, info.tile_h as f32);
        let Some(t) = self.texture(gfx, s.page) else { return false };
        // Map tiles are 32 px: scale page tiles relative to that.
        let k = tp / 32.0;
        let dx = x + s.dx as f32 * tp;
        let dy = y + s.dy as f32 * tp;
        let w = s.tw as f32 * tw * k;
        let h = s.th as f32 * th * k;
        let c = Color::from_rgba(s.tint[0], s.tint[1], s.tint[2], s.tint[3]);
        draw_texture_ex(
            &t,
            dx,
            dy,
            c,
            DrawTextureParams {
                dest_size: Some(vec2(w.ceil(), h.ceil())),
                source: Some(Rect::new(s.tx as f32 * tw, s.ty as f32 * th, s.tw as f32 * tw, s.th as f32 * th)),
                ..Default::default()
            },
        );
        true
    }

    /// Draw an interface sprite (e.g. a 4x3-cell toolbar icon) stretched into `r`.
    pub fn draw_ui_sprite(&mut self, gfx: &Gfx, s: &Sprite, r: Rect) -> bool {
        let Some(info) = gfx.pages.get(s.page as usize) else { return false };
        let (tw, th) = (info.tile_w as f32, info.tile_h as f32);
        let Some(t) = self.texture(gfx, s.page) else { return false };
        let c = Color::from_rgba(s.tint[0], s.tint[1], s.tint[2], s.tint[3]);
        draw_texture_ex(
            &t,
            r.x,
            r.y,
            c,
            DrawTextureParams {
                dest_size: Some(vec2(r.w, r.h)),
                source: Some(Rect::new(s.tx as f32 * tw, s.ty as f32 * th, s.tw as f32 * tw, s.th as f32 * th)),
                ..Default::default()
            },
        );
        true
    }
}

fn rgb(c: [u8; 3], k: f32) -> Color {
    Color::new(
        (c[0] as f32 / 255.0 * k).min(1.0),
        (c[1] as f32 / 255.0 * k).min(1.0),
        (c[2] as f32 / 255.0 * k).min(1.0),
        1.0,
    )
}

fn hash(x: i16, y: i16, z: i16) -> u32 {
    let mut h = (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663) ^ (z as u32).wrapping_mul(83_492_791);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^ (h >> 15)
}

/// Display color of a tile's material (palette color, else a default per tiletype material).
pub fn tile_rgb(w: &World, t: &Tile, m: TtMaterial) -> [u8; 3] {
    if t.mat != NO_MAT {
        if let Some(mi) = w.material(t.mat) {
            if mi.rgb != [0, 0, 0] {
                return mi.rgb;
            }
        }
    }
    match m {
        TtMaterial::Soil => [130, 95, 60],
        TtMaterial::Stone | TtMaterial::LavaStone => [125, 125, 128],
        TtMaterial::Mineral => [95, 90, 105],
        TtMaterial::Feature => [140, 90, 160],
        TtMaterial::FrozenLiquid => [170, 210, 235],
        TtMaterial::Construction => [150, 150, 150],
        TtMaterial::GrassLight => [70, 150, 55],
        TtMaterial::GrassDark => [45, 115, 45],
        TtMaterial::GrassDry => [150, 145, 70],
        TtMaterial::GrassDead => [120, 105, 70],
        TtMaterial::Plant | TtMaterial::Mushroom => [70, 140, 60],
        TtMaterial::Tree | TtMaterial::Root | TtMaterial::Driftwood => [120, 85, 45],
        TtMaterial::Magma => [230, 80, 20],
        TtMaterial::Ashes => [80, 80, 80],
        TtMaterial::Hfs => [200, 40, 40],
        _ => [110, 110, 110],
    }
}

/// Short DF-like description of a tile, e.g. `smooth granite wall`, `open space`.
pub fn tile_desc(w: &World, p: Pos) -> String {
    let Some(t) = w.tile(p) else { return "outside the map".into() };
    if t.hidden() {
        return "unexplored".into();
    }
    let tt = w.tiletypes.get(t.tt);
    let shape = match tt.shape {
        Shape::Empty => return liquid_suffix("open space", t),
        Shape::None => return "void".into(),
        Shape::Wall if tt.material == TtMaterial::Tree => "tree trunk",
        Shape::Wall => "wall",
        Shape::Floor => "floor",
        Shape::Boulder => "boulder",
        Shape::Pebbles => "pebbles",
        Shape::Fortification => "fortification",
        Shape::StairUp => "up stair",
        Shape::StairDown => "down stair",
        Shape::StairUpdown => "up/down stair",
        Shape::Ramp => "ramp",
        Shape::RampTop => "downward ramp",
        Shape::BrookBed => "brook bed",
        Shape::BrookTop => "brook",
        Shape::Branch => "branches",
        Shape::TrunkBranch => "trunk and branches",
        Shape::Twig => "twigs",
        Shape::Sapling => "sapling",
        Shape::Shrub => "shrub",
        Shape::EndlessPit => "endless pit",
    };
    let mat = if t.mat != NO_MAT {
        w.material(t.mat).map(|m| m.name.clone()).unwrap_or_default()
    } else {
        String::new()
    };
    let mat = if mat.is_empty() {
        match tt.material {
            TtMaterial::Soil => "soil".to_string(),
            TtMaterial::Stone => "stone".to_string(),
            TtMaterial::Mineral => "mineral".to_string(),
            m if m.is_grass() => "grass".to_string(),
            TtMaterial::Construction => "constructed".to_string(),
            TtMaterial::FrozenLiquid => "ice".to_string(),
            _ => String::new(),
        }
    } else {
        mat
    };
    let smooth = if matches!(tt.special, TtSpecial::Smooth | TtSpecial::SmoothDead) { "smooth " } else { "" };
    let s = if mat.is_empty() { format!("{smooth}{shape}") } else { format!("{smooth}{mat} {shape}") };
    liquid_suffix(&s, t)
}

fn liquid_suffix(s: &str, t: &Tile) -> String {
    let d = t.liquid_depth();
    if d == 0 {
        s.to_string()
    } else {
        format!("{s}, {} {d}/7", if t.is_magma() { "magma" } else { "water" })
    }
}

/// Visible tile range for the camera.
pub fn visible_range(g: &Game) -> (i16, i16, i16, i16) {
    let (x0, y0) = g.cam.to_tile(vec2(0.0, 0.0));
    let (x1, y1) = g.cam.to_tile(vec2(screen_width(), screen_height()));
    let d = g.sim.world.dims;
    (
        (x0.floor() as i32).clamp(0, d.x as i32) as i16,
        (y0.floor() as i32).clamp(0, d.y as i32) as i16,
        (x1.ceil() as i32 + 1).clamp(0, d.x as i32) as i16,
        (y1.ceil() as i32 + 1).clamp(0, d.y as i32) as i16,
    )
}

/// Integer screen rect of map tile (x, y), seamless at any zoom.
#[inline]
pub fn tile_rect(g: &Game, x: i16, y: i16) -> Rect {
    let a = g.cam.to_screen(x as f32, y as f32);
    let b = g.cam.to_screen(x as f32 + 1.0, y as f32 + 1.0);
    let (ax, ay) = (a.x.floor(), a.y.floor());
    Rect::new(ax, ay, b.x.floor() - ax, b.y.floor() - ay)
}

pub fn draw_map(g: &mut Game) {
    let (x0, y0, x1, y1) = visible_range(g);
    let z = g.cam.z as i16;
    let mut sprites = std::mem::take(&mut g.rend.buf);
    // ---- terrain
    for y in y0..y1 {
        for x in x0..x1 {
            let r = tile_rect(g, x, y);
            draw_terrain(g, &mut sprites, Pos::new(x, y, z), r);
        }
    }
    // ---- buildings & stockpiles
    let world = &g.sim.world;
    let blds: Vec<usize> = world
        .buildings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.z == z && b.kind != "Civzone" && b.x2 >= x0 && b.x1 < x1 && b.y2 >= y0 && b.y1 < y1)
        .map(|(i, _)| i)
        .collect();
    for bi in blds {
        draw_building(g, &mut sprites, bi);
    }
    // ---- zones (only in zone mode / when selected, like DF)
    g.rend.buf = std::mem::take(&mut sprites);
    crate::ui::build::draw_zones(g, x0, y0, x1, y1);
    sprites = std::mem::take(&mut g.rend.buf);
    // ---- items on the ground
    let tp = g.cam.tile_px();
    let world = &g.sim.world;
    let items: Vec<usize> = world
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| {
            it.pos.z == z
                && it.flags.on_ground
                && !it.flags.hidden
                && it.pos.x >= x0
                && it.pos.x < x1
                && it.pos.y >= y0
                && it.pos.y < y1
        })
        .map(|(i, _)| i)
        .collect();
    for ii in items {
        let it = &g.sim.world.items[ii];
        if g.sim.world.tile(it.pos).map(|t| t.hidden()).unwrap_or(true) {
            continue;
        }
        let r = tile_rect(g, it.pos.x, it.pos.y);
        sprites.clear();
        g.gfx.item(&g.sim.world, it, &mut sprites);
        let mut drawn = false;
        for s in &sprites {
            drawn |= g.rend.draw_sprite(&g.gfx, s, r.x, r.y, tp);
        }
        if !drawn {
            draw_item_fallback(&g.font, &g.sim.world, it, r);
        }
    }
    // ---- units
    let units: Vec<usize> = g
        .sim
        .world
        .units
        .iter()
        .enumerate()
        .filter(|(_, u)| {
            u.pos.z == z && !u.flags.dead && !u.flags.inactive && u.pos.x >= x0 && u.pos.x < x1 && u.pos.y >= y0 && u.pos.y < y1
        })
        .map(|(i, _)| i)
        .collect();
    for ui in units {
        let u = &g.sim.world.units[ui];
        // Like DF: creatures in unexplored areas stay invisible.
        if g.sim.world.tile(u.pos).map(|t| t.hidden()).unwrap_or(true) {
            continue;
        }
        let r = tile_rect(g, u.pos.x, u.pos.y);
        sprites.clear();
        g.gfx.unit(&g.sim.world, u, &mut sprites);
        let mut drawn = false;
        for s in &sprites {
            // Wide unit sprites (2-tile weapons, big creatures) are centered on their tile.
            let off = s.unit_center_px(32) as f32 * tp / 32.0;
            drawn |= g.rend.draw_sprite(&g.gfx, s, r.x + off, r.y, tp);
        }
        if !drawn {
            draw_unit_fallback(&g.font, u, r);
        }
        if g.selected == Selected::Unit(u.id) {
            corners(r, Color::from_rgba(255, 230, 80, 255), (r.w / 8.0).max(2.0));
        }
    }
    g.rend.buf = sprites;
    crate::ui::build::draw_ghost(g);
    draw_selection_and_cursor(g);
}

fn draw_terrain(g: &mut Game, sprites: &mut Vec<Sprite>, p: Pos, r: Rect) {
    let w = &g.sim.world;
    let Some(t) = w.tile(p).copied() else { return };
    if t.hidden() {
        draw_rectangle(r.x, r.y, r.w, r.h, BLACK);
        return;
    }
    let tt = w.tiletypes.get(t.tt);
    let shape = tt.shape;
    let tp = g.cam.tile_px();
    let open = matches!(shape, Shape::Empty | Shape::RampTop | Shape::Twig);
    if open {
        // Look down through open space; lower levels get darker, like DF.
        let mut drawn = false;
        for d in 1..=LOOK_DOWN {
            let q = Pos::new(p.x, p.y, p.z - d);
            let Some(bt) = w.tile(q).copied() else { break };
            if bt.hidden() {
                draw_rectangle(r.x, r.y, r.w, r.h, BLACK);
                drawn = true;
                break;
            }
            let bs = w.tiletypes.get(bt.tt).shape;
            if bs == Shape::Empty {
                continue;
            }
            sprites.clear();
            g.gfx.terrain(w, q, sprites);
            let mut ok = false;
            for s in sprites.iter() {
                ok |= g.rend.draw_sprite(&g.gfx, s, r.x, r.y, tp);
            }
            if !ok {
                fallback_terrain(&g.font, w, q, &bt, r, false);
            }
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.03, 0.05, 0.10, FOG[d as usize]));
            drawn = true;
            break;
        }
        if !drawn {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::from_rgba(6, 8, 14, 255));
        }
        if shape == Shape::Empty && t.liquid_depth() == 0 && t.desig == 0 {
            return;
        }
    }
    sprites.clear();
    if shape != Shape::Empty {
        g.gfx.terrain(w, p, sprites);
    }
    let mut ok = false;
    for s in sprites.iter() {
        ok |= g.rend.draw_sprite(&g.gfx, s, r.x, r.y, tp);
    }
    if !ok {
        fallback_terrain(&g.font, w, p, &t, r, open);
        draw_designation(&g.font, w, &t, r);
    } else if t.desig != 0 {
        // rf-gfx drew the tile (and normally its designation sprite); keep a thin marker so
        // designations stay visible either way.
        draw_rectangle_lines(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, 1.0, Color::new(1.0, 0.82, 0.22, 0.55));
    }
}

/// Clean material-colored terrain without sprites.
fn fallback_terrain(font: &Font, w: &World, p: Pos, t: &Tile, r: Rect, overlay_only: bool) {
    let tt = w.tiletypes.get(t.tt);
    let base = tile_rgb(w, t, tt.material);
    let h = hash(p.x, p.y, p.z);
    let detail = r.w >= 12.0;
    let smooth = matches!(tt.special, TtSpecial::Smooth | TtSpecial::SmoothDead);
    let is_wall = |q: Pos| w.tile(q).map(|x| w.tiletypes.get(x.tt).shape == Shape::Wall && !x.hidden()).unwrap_or(false);
    let floor = |base: [u8; 3]| {
        let k = if tt.material.is_grass() { 0.62 } else { 0.42 };
        draw_rectangle(r.x, r.y, r.w, r.h, rgb(base, k));
        if detail && !smooth {
            let n = if tt.material.is_grass() { 4 } else { 3 };
            for i in 0..n {
                let hh = h.rotate_left(i * 7);
                let sx = r.x + (hh % 1000) as f32 / 1000.0 * (r.w - r.w / 8.0);
                let sy = r.y + ((hh >> 10) % 1000) as f32 / 1000.0 * (r.h - r.h / 6.0);
                if tt.material.is_grass() {
                    draw_line(sx, sy + r.h / 6.0, sx + r.w / 24.0, sy, (r.w / 24.0).max(1.0), rgb(base, 0.95));
                } else {
                    draw_rectangle(sx, sy, r.w / 10.0, r.h / 12.0, rgb(base, 0.55 + (hh >> 20 & 3) as f32 * 0.05));
                }
            }
        }
        if smooth && detail {
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, rgb(base, 0.5));
        }
    };
    let glyph = |code: u8, k: f32, sc: f32| {
        let gw = r.w * sc * 0.75;
        let gh = r.h * sc;
        font.glyph(code, r.x + (r.w - gw) / 2.0, r.y + (r.h - gh) / 2.0, gw, gh, rgb(base, k));
    };
    match tt.shape {
        Shape::Wall if tt.material == TtMaterial::Tree => {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::from_rgba(28, 44, 22, 255));
            draw_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 0.42, rgb(base, 0.55));
            draw_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 0.34, rgb(base, 0.85));
            if detail {
                draw_circle_lines(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 0.2, 1.0, rgb(base, 0.6));
            }
        }
        Shape::Wall => {
            let k = if smooth { 0.80 } else { 0.70 };
            draw_rectangle(r.x, r.y, r.w, r.h, rgb(base, k));
            if detail {
                if tt.material == TtMaterial::Construction {
                    // Block masonry.
                    let c = rgb(base, 0.5);
                    draw_line(r.x, r.y + r.h / 2.0, r.x + r.w, r.y + r.h / 2.0, 1.0, c);
                    draw_line(r.x + r.w / 2.0, r.y, r.x + r.w / 2.0, r.y + r.h / 2.0, 1.0, c);
                    draw_line(r.x + r.w / 4.0, r.y + r.h / 2.0, r.x + r.w / 4.0, r.y + r.h, 1.0, c);
                } else if !smooth {
                    for i in 0..4u32 {
                        let hh = h.rotate_left(i * 8);
                        let sx = r.x + (hh % 997) as f32 / 997.0 * (r.w * 0.8);
                        let sy = r.y + ((hh >> 11) % 991) as f32 / 991.0 * (r.h * 0.8);
                        let k2 = if hh & 1 == 0 { 0.82 } else { 0.58 };
                        draw_rectangle(sx, sy, r.w * 0.18, r.h * 0.12, rgb(base, k2));
                    }
                    if tt.material == TtMaterial::Mineral {
                        // Ore/gem veins sparkle.
                        for i in 0..3u32 {
                            let hh = h.rotate_left(5 + i * 9);
                            let sx = r.x + (hh % 991) as f32 / 991.0 * (r.w * 0.85);
                            let sy = r.y + ((hh >> 9) % 983) as f32 / 983.0 * (r.h * 0.85);
                            draw_rectangle(sx, sy, r.w * 0.12, r.h * 0.12, rgb(base, 1.5));
                        }
                    }
                }
            }
            // Fake depth: shadow toward open tiles below, light rim above.
            let south = Pos::new(p.x, p.y + 1, p.z);
            let north = Pos::new(p.x, p.y - 1, p.z);
            if !is_wall(south) && w.in_bounds(south) {
                draw_rectangle(r.x, r.y + r.h * 0.80, r.w, r.h * 0.20, rgb(base, 0.40));
            }
            if !is_wall(north) && w.in_bounds(north) {
                draw_rectangle(r.x, r.y, r.w, (r.h * 0.07).max(1.0), rgb(base, 1.05));
            }
        }
        Shape::Fortification => {
            draw_rectangle(r.x, r.y, r.w, r.h, rgb(base, 0.42));
            glyph(206, 0.85, 1.0);
        }
        Shape::Floor | Shape::BrookBed => floor(base),
        Shape::Pebbles => {
            floor(base);
            for i in 0..3u32 {
                let hh = h.rotate_left(3 + i * 6);
                draw_circle(
                    r.x + r.w * (0.2 + (hh % 60) as f32 / 100.0),
                    r.y + r.h * (0.2 + ((hh >> 8) % 60) as f32 / 100.0),
                    r.w * 0.07,
                    rgb(base, 0.85),
                );
            }
        }
        Shape::Boulder => {
            floor(base);
            draw_circle(r.x + r.w / 2.0, r.y + r.h * 0.55, r.w * 0.36, rgb(base, 0.45));
            draw_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 0.34, rgb(base, 0.85));
            draw_circle(r.x + r.w * 0.42, r.y + r.h * 0.40, r.w * 0.10, rgb(base, 1.1));
        }
        Shape::StairUp => {
            floor(base);
            glyph(b'<', 1.25, 0.9);
        }
        Shape::StairDown => {
            floor(base);
            glyph(b'>', 1.25, 0.9);
        }
        Shape::StairUpdown => {
            floor(base);
            glyph(b'X', 1.25, 0.9);
        }
        Shape::Ramp => {
            floor(base);
            glyph(30, 1.2, 0.95);
        }
        Shape::RampTop => glyph(31, 1.3, 0.9),
        Shape::Shrub => {
            if !overlay_only {
                floor([70, 120, 50]);
            }
            let g = rgb(base, 0.9);
            draw_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 0.28, g);
            if detail && h & 3 == 0 {
                draw_circle(r.x + r.w * 0.42, r.y + r.h * 0.45, r.w * 0.06, Color::from_rgba(200, 40, 60, 255));
                draw_circle(r.x + r.w * 0.58, r.y + r.h * 0.56, r.w * 0.06, Color::from_rgba(200, 40, 60, 255));
            }
        }
        Shape::Sapling => {
            floor([70, 120, 50]);
            glyph(231, 1.0, 0.8);
        }
        Shape::Branch | Shape::TrunkBranch | Shape::Twig => {
            let leaves = Color::from_rgba(52, 120, 45, 235);
            if tt.shape == Shape::Twig {
                for i in 0..4u32 {
                    let hh = h.rotate_left(i * 5);
                    draw_circle(r.x + r.w * (0.15 + (hh % 70) as f32 / 100.0), r.y + r.h * (0.15 + ((hh >> 8) % 70) as f32 / 100.0), r.w * 0.09, leaves);
                }
            } else {
                draw_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 0.52, Color::from_rgba(38, 92, 34, 235));
                draw_circle(r.x + r.w * 0.45, r.y + r.h * 0.45, r.w * 0.36, leaves);
                if tt.shape == Shape::TrunkBranch {
                    draw_circle(r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 0.18, rgb(base, 0.8));
                }
            }
        }
        Shape::BrookTop => {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::from_rgba(40, 80, 160, 255));
        }
        Shape::EndlessPit | Shape::None => draw_rectangle(r.x, r.y, r.w, r.h, BLACK),
        Shape::Empty => {}
    }
    // Liquids.
    let d = t.liquid_depth();
    if d > 0 {
        let c = if t.is_magma() { Color::new(0.95, 0.35, 0.05, 0.45 + d as f32 * 0.07) } else { Color::new(0.10, 0.30, 0.85, 0.30 + d as f32 * 0.07) };
        draw_rectangle(r.x, r.y, r.w, r.h, c);
        if detail {
            let gw = r.w * 0.6;
            let lc = if t.is_magma() { Color::from_rgba(255, 200, 60, 220) } else { Color::from_rgba(140, 190, 255, 200) };
            font.glyph(247, r.x + (r.w - gw) / 2.0, r.y + r.h * 0.1, gw, r.h * 0.8, lc);
        }
    }
}

fn draw_designation(font: &Font, w: &World, t: &Tile, r: Rect) {
    let d = t.dig();
    if d == Dig::None {
        return;
    }
    let tt = w.tiletypes.get(t.tt);
    let marker = t.flags & rf_core::tile_flags::MARKER != 0;
    let plantish = matches!(tt.material, TtMaterial::Tree | TtMaterial::Plant) || matches!(tt.shape, Shape::Shrub | Shape::Branch | Shape::TrunkBranch);
    let base = if marker {
        Color::new(0.35, 0.6, 1.0, 1.0)
    } else if plantish {
        Color::new(0.75, 0.95, 0.30, 1.0)
    } else {
        Color::new(1.0, 0.82, 0.22, 1.0)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(base.r, base.g, base.b, 0.22));
    let th = (r.w / 16.0).max(1.0);
    // Hatching like DF's designation overlay.
    if r.w >= 10.0 {
        let c = Color::new(base.r, base.g, base.b, 0.55);
        draw_line(r.x, r.y + r.h * 0.5, r.x + r.w * 0.5, r.y, th, c);
        draw_line(r.x, r.y + r.h, r.x + r.w, r.y, th, c);
        draw_line(r.x + r.w * 0.5, r.y + r.h, r.x + r.w, r.y + r.h * 0.5, th, c);
    }
    draw_rectangle_lines(r.x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0, th, Color::new(base.r, base.g, base.b, 0.85));
    let g = match d {
        Dig::UpDownStair => Some(b'X'),
        Dig::UpStair => Some(b'<'),
        Dig::DownStair => Some(b'>'),
        Dig::Ramp => Some(30),
        Dig::Channel => Some(31),
        _ => None,
    };
    if let Some(code) = g {
        let gw = r.w * 0.6;
        font.glyph(code, r.x + (r.w - gw) / 2.0, r.y + r.h * 0.1, gw, r.h * 0.8, Color::new(1.0, 0.95, 0.6, 1.0));
    }
}

fn stockpile_color(b: &Building) -> [u8; 3] {
    let cats = b.stockpile.as_ref().map(|s| s.categories.as_slice()).unwrap_or(&[]);
    if cats.len() > 3 {
        return STOCK_PRESETS[4].color;
    }
    for p in STOCK_PRESETS {
        if cats.iter().any(|c| p.categories.contains(&c.as_str())) {
            return p.color;
        }
    }
    [200, 190, 150]
}

/// Not yet constructed: workshops show DF's construction-stage sprites; everything else is
/// drawn translucent. Both get a small hammer marker.
fn unbuilt(b: &Building) -> bool {
    !b.built && b.kind != "Stockpile" && b.kind != "Civzone"
}

fn draw_building(g: &mut Game, sprites: &mut Vec<Sprite>, bi: usize) {
    let tp = g.cam.tile_px();
    let b = &g.sim.world.buildings[bi];
    let ghost = unbuilt(b);
    let staged = matches!(b.kind.as_str(), "Workshop" | "Furnace");
    let fade = if ghost && !staged { [255, 255, 255, 130] } else { [255; 4] };
    let selected = g.selected == Selected::Building(b.id);
    // Ask rf-gfx for every tile first; if it knows the building, it draws all of it.
    let mut any = false;
    for y in b.y1..=b.y2 {
        for x in b.x1..=b.x2 {
            let p = Pos::new(x, y, b.z);
            if !b.contains(p) {
                continue;
            }
            sprites.clear();
            g.gfx.building(&g.sim.world, b, p, sprites);
            if sprites.is_empty() && b.kind == "FarmPlot" {
                if let Some(sp) = g.gfx.named("FARMPLOT") {
                    sprites.push(sp);
                }
            }
            let r = tile_rect(g, x, y);
            for s in sprites.iter() {
                any |= g.rend.draw_sprite(&g.gfx, &s.mul_tint(fade), r.x, r.y, tp);
            }
        }
    }
    if any {
        let b = &g.sim.world.buildings[bi];
        if b.kind == "FarmPlot" {
            draw_crop_marker(g, bi);
        }
        let b = &g.sim.world.buildings[bi];
        if ghost {
            construction_marker(g, b.x1, b.y1, b.x2, b.y2);
        }
        if selected {
            let b = &g.sim.world.buildings[bi];
            outline(g, b.x1, b.y1, b.x2, b.y2, Color::from_rgba(255, 230, 80, 255));
        }
        return;
    }
    let is_pile = b.kind == "Stockpile";
    let is_zone = b.kind == "Civzone";
    let c = if is_pile {
        stockpile_color(b)
    } else if is_zone {
        [90, 200, 120]
    } else {
        [150, 110, 60]
    };
    let col = rgb(c, 1.0);
    for y in b.y1..=b.y2 {
        for x in b.x1..=b.x2 {
            let p = Pos::new(x, y, b.z);
            if !b.contains(p) {
                continue;
            }
            let r = tile_rect(g, x, y);
            if is_pile || is_zone {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(col.r, col.g, col.b, if is_pile { 0.20 } else { 0.12 }));
                if r.w >= 12.0 && is_pile {
                    let k = r.w / 8.0;
                    draw_rectangle(r.x + k, r.y + k, r.w - 2.0 * k, r.h - 2.0 * k, Color::new(col.r, col.g, col.b, 0.10));
                }
            } else {
                draw_rectangle(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, Color::from_rgba(70, 50, 30, 235));
                draw_rectangle_lines(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, (r.w / 16.0).max(1.0), col);
            }
        }
    }
    let a = g.cam.to_screen(b.x1 as f32, b.y1 as f32);
    let e = g.cam.to_screen(b.x2 as f32 + 1.0, b.y2 as f32 + 1.0);
    draw_rectangle_lines(a.x, a.y, e.x - a.x, e.y - a.y, if is_pile { 2.0 } else { 1.0 }, Color::new(col.r, col.g, col.b, 0.9));
    // Label glyph in the middle.
    let code = if is_pile {
        let cats = b.stockpile.as_ref().map(|s| s.categories.clone()).unwrap_or_default();
        if cats.len() > 3 {
            b'*'
        } else {
            STOCK_PRESETS.iter().find(|p| cats.iter().any(|c| p.categories.contains(&c.as_str()))).map(|p| p.glyph).unwrap_or(b'=')
        }
    } else {
        building_glyph(b)
    };
    let (cw, ch) = (tp * 0.66, tp);
    let cx = (a.x + e.x) / 2.0 - cw / 2.0;
    let cy = (a.y + e.y) / 2.0 - ch / 2.0;
    g.font.glyph(code, cx, cy, cw, ch, Color::new(col.r, col.g, col.b, if is_pile { 0.75 } else { 1.0 }));
    let b = &g.sim.world.buildings[bi];
    let (bx1, by1, bx2, by2) = (b.x1, b.y1, b.x2, b.y2);
    if unbuilt(b) {
        construction_marker(g, bx1, by1, bx2, by2);
    }
    if g.selected == Selected::Building(b.id) {
        outline(g, bx1, by1, bx2, by2, Color::from_rgba(255, 230, 80, 255));
    }
}

/// Yellow corner brackets around a building's footprint (selection).
fn outline(g: &Game, x1: i16, y1: i16, x2: i16, y2: i16, c: Color) {
    let a = g.cam.to_screen(x1 as f32, y1 as f32);
    let e = g.cam.to_screen(x2 as f32 + 1.0, y2 as f32 + 1.0);
    let r = Rect::new(a.x, a.y, e.x - a.x, e.y - a.y);
    corners(r, c, (g.cam.tile_px() / 2.5).max(4.0));
}

/// Dashed yellow frame plus a hammer glyph: "waiting for construction".
fn construction_marker(g: &Game, x1: i16, y1: i16, x2: i16, y2: i16) {
    let a = g.cam.to_screen(x1 as f32, y1 as f32);
    let e = g.cam.to_screen(x2 as f32 + 1.0, y2 as f32 + 1.0);
    let c = Color::new(1.0, 0.82, 0.25, 0.9);
    let dash = (g.cam.tile_px() / 4.0).max(3.0);
    let mut x = a.x;
    while x < e.x {
        let x2 = (x + dash).min(e.x);
        draw_line(x, a.y + 1.0, x2, a.y + 1.0, 2.0, c);
        draw_line(x, e.y - 1.0, x2, e.y - 1.0, 2.0, c);
        x += dash * 2.0;
    }
    let mut y = a.y;
    while y < e.y {
        let y2 = (y + dash).min(e.y);
        draw_line(a.x + 1.0, y, a.x + 1.0, y2, 2.0, c);
        draw_line(e.x - 1.0, y, e.x - 1.0, y2, 2.0, c);
        y += dash * 2.0;
    }
    let sz = (g.cam.tile_px() * 0.55).max(8.0);
    crate::ui::draw_fallback_icon(&g.font, crate::ui::Ico::Hammer, e.x - sz - 2.0, a.y + 2.0, sz, sz);
}

/// Farm plots: the current season's crop as a tiny label/glyph (DF shows planted crops).
fn draw_crop_marker(g: &Game, bi: usize) {
    let b = &g.sim.world.buildings[bi];
    let season = (g.sim.world.calendar.month() / 3).clamp(0, 3) as usize;
    let crop = b.crops.get(season).map(|s| s.as_str()).unwrap_or("");
    if crop.is_empty() || g.cam.tile_px() < 16.0 {
        return;
    }
    let a = g.cam.to_screen(b.x1 as f32, b.y1 as f32);
    let name = g.sim.world.plant_raws.iter().find(|p| p.token == crop).map(|p| p.name.clone()).unwrap_or_else(|| crate::ui::pretty(crop).to_lowercase());
    g.font.draw_shadow(&crate::ui::clip(&name, 18), a.x + 2.0, a.y + 2.0, 1.0, Color::from_rgba(170, 240, 120, 255));
}

fn building_glyph(b: &Building) -> u8 {
    match b.kind.as_str() {
        "Door" => 8,
        "Bed" => 233,
        "Table" => 210,
        "Chair" => 210,
        "Coffin" => 48,
        "Box" | "Chest" => 146,
        "Cabinet" => 240,
        "Statue" => 234,
        "Well" => b'O',
        "Hatch" => b'#',
        "Bridge" => 240,
        "Trap" => b'^',
        "Floodgate" => 15,
        "Lever" => b'/',
        "FarmPlot" => b'~',
        "Workshop" | "Furnace" => b.subtype.bytes().next().unwrap_or(b'W').to_ascii_uppercase(),
        k => k.bytes().next().unwrap_or(b'?'),
    }
}

fn item_glyph(it: &Item) -> u8 {
    match it.kind.as_str() {
        "BOULDER" => 7,
        "ROCK" => 250,
        "WOOD" => b'/',
        "BAR" => 22,
        "BLOCKS" => 240,
        "DRINK" | "BARREL" => 237,
        "PLANT" | "PLANT_GROWTH" => 5,
        "SEEDS" => 250,
        "MEAT" | "FISH" | "FISH_RAW" | "FOOD" => b'%',
        "CORPSE" | "CORPSEPIECE" | "REMAINS" => b'&',
        "BIN" | "BOX" => 146,
        "BUCKET" => 150,
        "BED" => 233,
        "TABLE" | "CHAIR" => 210,
        "DOOR" => 8,
        "WEAPON" | "AMMO" => 24,
        "ARMOR" | "HELM" | "PANTS" | "SHOES" | "GLOVES" | "SHIELD" => b'[',
        "TOOL" => b'(',
        "GEM" | "ROUGH" | "SMALLGEM" => 15,
        "CLOTH" | "THREAD" => 247,
        "COIN" => b'$',
        _ => b'?',
    }
}

fn draw_item_fallback(font: &Font, w: &World, it: &Item, r: Rect) {
    let c = w.material(it.mat).map(|m| m.rgb).unwrap_or([200, 200, 200]);
    let code = item_glyph(it);
    let (gw, gh) = (r.w * 0.55, r.h * 0.75);
    let (x, y) = (r.x + (r.w - gw) / 2.0, r.y + (r.h - gh) / 2.0);
    font.glyph(code, x + 1.0, y + 1.0, gw, gh, Color::new(0.0, 0.0, 0.0, 0.7));
    font.glyph(code, x, y, gw, gh, rgb(c, 1.15));
}

/// DF-like profession color for the fallback unit glyph.
fn unit_color(u: &Unit) -> Color {
    if u.flags.hostile {
        return Color::from_rgba(255, 70, 60, 255);
    }
    if u.flags.animal {
        return Color::from_rgba(220, 200, 160, 255);
    }
    let c: [u8; 3] = match u.profession.as_str() {
        "MINER" | "STONEWORKER" | "MASON" | "ENGRAVER" | "STONECUTTER" | "STONE_CARVER" => [200, 200, 205],
        "WOODCUTTER" | "CARPENTER" | "WOODWORKER" | "BOWYER" => [210, 160, 70],
        "FARMER" | "PLANTER" | "BREWER" | "COOK" | "HERBALIST" | "THRESHER" | "MILLER" => [110, 210, 90],
        "SMITH" | "METALSMITH" | "WEAPONSMITH" | "ARMORER" | "BLACKSMITH" | "FURNACE_OPERATOR" | "METALCRAFTER" => [90, 200, 230],
        "CHILD" | "BABY" => [230, 230, 120],
        "PEASANT" => [235, 235, 235],
        p if p.contains("DWARF") || p.contains("SOLDIER") || p.contains("RECRUIT") => [230, 90, 90],
        _ => [240, 190, 120],
    };
    Color::from_rgba(c[0], c[1], c[2], 255)
}

fn draw_unit_fallback(font: &Font, u: &Unit, r: Rect) {
    draw_circle(r.x + r.w / 2.0, r.y + r.h * 0.55, r.w * 0.40, Color::new(0.0, 0.0, 0.0, 0.55));
    let code = if u.race == "DWARF" {
        1
    } else {
        let c = u.race.chars().next().unwrap_or('?');
        let c = if u.flags.animal { c.to_ascii_lowercase() } else { c.to_ascii_uppercase() };
        c as u8
    };
    let (gw, gh) = (r.w * 0.66, r.h * 0.9);
    font.glyph(code, r.x + (r.w - gw) / 2.0, r.y + (r.h - gh) / 2.0, gw, gh, unit_color(u));
}

fn corners(r: Rect, c: Color, len: f32) {
    let t = (r.w / 16.0).max(2.0);
    for (x, y, sx, sy) in [(r.x, r.y, 1.0, 1.0), (r.x + r.w, r.y, -1.0, 1.0), (r.x, r.y + r.h, 1.0, -1.0), (r.x + r.w, r.y + r.h, -1.0, -1.0)] {
        draw_line(x, y, x + sx * len, y, t, c);
        draw_line(x, y, x, y + sy * len, t, c);
    }
}

fn draw_selection_and_cursor(g: &mut Game) {
    let z = g.cam.z as i16;
    // Selected tile.
    if let Selected::Tile(p) = g.selected {
        if p.z == z {
            let r = tile_rect(g, p.x, p.y);
            corners(r, Color::from_rgba(255, 230, 80, 255), (r.w / 3.0).max(3.0));
        }
    }
    // Job target of the selected unit.
    if let Selected::Unit(id) = g.selected {
        if let Some(jp) = g.sim.unit_status(id).job_pos {
            if jp.z == z {
                let r = tile_rect(g, jp.x, jp.y);
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::from_rgba(120, 220, 255, 200));
            }
        }
    }
    let tool_on = g.tool != Tool::None;
    // Rectangle being placed.
    let build_tool = matches!(g.tool, Tool::Build(_));
    if let (Some(d), Some(h), false) = (g.drag, g.hover, build_tool) {
        let (lo, hi) = (
            Pos::new(d.a.x.min(h.x), d.a.y.min(h.y), d.a.z.min(h.z)),
            Pos::new(d.a.x.max(h.x), d.a.y.max(h.y), d.a.z.max(h.z)),
        );
        let a = g.cam.to_screen(lo.x as f32, lo.y as f32);
        let b = g.cam.to_screen(hi.x as f32 + 1.0, hi.y as f32 + 1.0);
        let c = match g.tool {
            Tool::Stockpile(_) => Color::new(0.85, 0.80, 0.55, 1.0),
            Tool::Zone(_) => Color::new(0.45, 0.95, 0.45, 1.0),
            Tool::Designate(rf_sim::DesignationKind::Remove) => Color::new(1.0, 0.45, 0.45, 1.0),
            _ => Color::new(1.0, 0.85, 0.25, 1.0),
        };
        if z >= lo.z && z <= hi.z {
            draw_rectangle(a.x, a.y, b.x - a.x, b.y - a.y, Color::new(c.r, c.g, c.b, 0.18));
            draw_rectangle_lines(a.x, a.y, b.x - a.x, b.y - a.y, 2.0, c);
        }
        let label = format!(
            "{}x{}x{}",
            hi.x - lo.x + 1,
            hi.y - lo.y + 1,
            hi.z - lo.z + 1
        );
        let m = g.mouse();
        g.font.draw_shadow(&label, m.x + 18.0, m.y + 4.0, 1.0, c);
        // Mark the first corner, even on another z-level.
        let ra = tile_rect(g, d.a.x, d.a.y);
        draw_rectangle_lines(ra.x, ra.y, ra.w, ra.h, 1.0, Color::new(c.r, c.g, c.b, 0.6));
    }
    // Hover cursor.
    if let Some(h) = g.hover {
        let r = tile_rect(g, h.x, h.y);
        if build_tool {
            // The placement ghost is the cursor.
        } else if tool_on {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 0.85, 0.25, 0.18));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(1.0, 0.85, 0.25, 1.0));
        } else {
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, Color::new(1.0, 1.0, 1.0, 0.75));
        }
    }
}
