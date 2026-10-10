//! [app8] Minimap of the current z-level (top right, toggled with the top bar's map button):
//! terrain colors (walls darker, open space shows the level below dimmed, water and magma),
//! buildings, creatures as dots (citizens yellow, hostiles red), and the camera's view as a
//! rectangle. Click or drag on it to move the camera there.
//!
//! The terrain texture is rebuilt when the level changes and at most once a second while the
//! game runs (192x192 tiles cost about 0.3 ms).

use super::{col, panel};
use crate::font::GH;
use crate::game::Game;
use macroquad::prelude::*;
use rf_core::{tile_flags, Pos, Shape, World};

#[derive(Default)]
pub struct MinimapCache {
    tex: Option<Texture2D>,
    z: i32,
    at: f64,
    tick: u64,
}

/// Minimap size in screen pixels (before the UI scale).
pub const MM_W: f32 = 240.0;

fn shade(c: [u8; 3], k: f32) -> [u8; 3] {
    [(c[0] as f32 * k).min(255.0) as u8, (c[1] as f32 * k).min(255.0) as u8, (c[2] as f32 * k).min(255.0) as u8]
}

/// Color of one map tile for the minimap (`None` = unknown / hidden).
pub fn tile_color(w: &World, x: i16, y: i16, z: i16) -> Option<[u8; 3]> {
    let mut zz = z;
    let mut dim = 1.0f32;
    // Open space shows what is below (up to 3 levels, darker each level), like the map view.
    for _ in 0..4 {
        let p = Pos::new(x, y, zz);
        let t = w.tile(p)?;
        if t.hidden() {
            return if zz == z { None } else { Some(shade([18, 18, 22], dim)) };
        }
        let d = t.liquid_depth();
        if d > 0 {
            let c = if t.is_magma() { [230, 90, 20] } else { [40, 90, 200] };
            return Some(shade(c, dim * (0.6 + 0.06 * d as f32)));
        }
        let info = w.tiletypes.get(t.tt);
        let base = crate::render::tile_rgb(w, t, info.material);
        match info.shape {
            Shape::Wall | Shape::Fortification => return Some(shade(base, 0.45 * dim)),
            s if s.is_open() => {
                if zz == 0 {
                    return Some([10, 10, 14]);
                }
                zz -= 1;
                dim *= 0.6;
                continue;
            }
            Shape::None => return Some(shade([30, 30, 34], dim)),
            _ => {
                let k = if t.flags & tile_flags::OUTSIDE != 0 { 1.0 } else { 0.85 };
                return Some(shade(base, k * dim));
            }
        }
    }
    Some([12, 12, 16])
}

fn build_texture(w: &World, z: i32) -> Texture2D {
    let (dx, dy) = (w.dims.x as usize, w.dims.y as usize);
    let mut bytes = vec![0u8; dx * dy * 4];
    for y in 0..dy {
        for x in 0..dx {
            let i = (y * dx + x) * 4;
            let c = tile_color(w, x as i16, y as i16, z as i16).unwrap_or([0, 0, 0]);
            bytes[i] = c[0];
            bytes[i + 1] = c[1];
            bytes[i + 2] = c[2];
            bytes[i + 3] = 255;
        }
    }
    // Buildings (not zones): a light tint on their footprint.
    for b in w.buildings.iter().filter(|b| b.z as i32 == z && b.kind != "Civzone") {
        let tint: [u8; 3] = match b.kind.as_str() {
            "Stockpile" => [170, 150, 90],
            "FarmPlot" => [110, 160, 70],
            "Workshop" | "Furnace" => [200, 170, 120],
            _ => [175, 175, 185],
        };
        for y in b.y1.max(0)..=b.y2.min(dy as i16 - 1) {
            for x in b.x1.max(0)..=b.x2.min(dx as i16 - 1) {
                if !b.contains(Pos::new(x, y, b.z)) {
                    continue;
                }
                let i = (y as usize * dx + x as usize) * 4;
                for k in 0..3 {
                    bytes[i + k] = ((bytes[i + k] as u16 + tint[k] as u16 * 2) / 3) as u8;
                }
            }
        }
    }
    let t = Texture2D::from_rgba8(dx as u16, dy as u16, &bytes);
    t.set_filter(FilterMode::Nearest);
    t
}

/// The minimap's rectangle for the current layout (top right, left of an open sheet).
pub fn rect(g: &Game, top: f32, right: f32) -> Rect {
    let s = g.ui.scale;
    let dims = g.sim.world.dims;
    let w = MM_W * s;
    let h = (w * dims.y as f32 / dims.x.max(1) as f32).min(MM_W * 1.5 * s) + GH * s + 10.0 * s;
    let sheet = g.selected != crate::game::Selected::None;
    let rx = if sheet { right - super::panels::SHEET_W * s - 8.0 * s } else { right };
    Rect::new(rx - w - 8.0 * s, top, w + 8.0 * s, h)
}

/// Draw the minimap (if on). Returns its rect.
pub fn draw(g: &mut Game, top: f32, right: f32) -> Option<Rect> {
    if !g.r8.minimap || g.screen != crate::game::Screen::None {
        return None;
    }
    let s = g.ui.scale;
    let r = rect(g, top, right);
    panel(&mut g.ui, r);
    let now = get_time();
    let z = g.cam.z;
    let tick = g.sim.ticks();
    let stale = g.r8.mm.tex.is_none() || g.r8.mm.z != z || (tick != g.r8.mm.tick && now - g.r8.mm.at > 1.0);
    if stale {
        g.r8.mm.tex = Some(build_texture(&g.sim.world, z));
        g.r8.mm.z = z;
        g.r8.mm.at = now;
        g.r8.mm.tick = tick;
    }
    let dims = g.sim.world.dims;
    let inner = Rect::new(r.x + 4.0 * s, r.y + GH * s + 6.0 * s, r.w - 8.0 * s, r.h - GH * s - 10.0 * s);
    g.font.draw(&format!("z {z}"), r.x + 6.0 * s, r.y + 3.0 * s, s, col::TEXT_HI);
    let hint = "click: go";
    g.font.draw(hint, r.x + r.w - g.font.width(hint, s) - 6.0 * s, r.y + 3.0 * s, s, col::TEXT_DIM);
    if let Some(t) = &g.r8.mm.tex {
        draw_texture_ex(t, inner.x, inner.y, WHITE, DrawTextureParams { dest_size: Some(vec2(inner.w, inner.h)), ..Default::default() });
    }
    let (kx, ky) = (inner.w / dims.x.max(1) as f32, inner.h / dims.y.max(1) as f32);
    // Creatures on this level.
    let zz = z as i16;
    for u in g.sim.world.units.iter().filter(|u| u.pos.z == zz && !u.flags.dead && u.pos.x >= 0) {
        if g.sim.world.tile(u.pos).map(|t| t.hidden()).unwrap_or(true) {
            continue;
        }
        let c = if g.hostiles.binary_search(&u.id).is_ok() {
            Color::from_rgba(255, 60, 50, 255)
        } else if u.flags.citizen {
            Color::from_rgba(255, 230, 80, 255)
        } else if u.flags.animal && u.flags.tame {
            Color::from_rgba(160, 230, 255, 255)
        } else {
            Color::from_rgba(200, 200, 200, 200)
        };
        let d = (kx * 1.6).max(2.0);
        draw_rectangle(inner.x + u.pos.x as f32 * kx - d / 4.0, inner.y + u.pos.y as f32 * ky - d / 4.0, d, d, c);
    }
    // Camera view.
    let (tx0, ty0) = g.cam.to_tile(vec2(0.0, 0.0));
    let (tx1, ty1) = g.cam.to_tile(vec2(screen_width(), screen_height()));
    let vx0 = (inner.x + tx0 * kx).max(inner.x);
    let vy0 = (inner.y + ty0 * ky).max(inner.y);
    let vx1 = (inner.x + tx1 * kx).min(inner.x + inner.w);
    let vy1 = (inner.y + ty1 * ky).min(inner.y + inner.h);
    if vx1 > vx0 && vy1 > vy0 {
        draw_rectangle_lines(vx0, vy0, vx1 - vx0, vy1 - vy0, 1.5, Color::from_rgba(255, 255, 255, 230));
    }
    // Click / drag: center the camera there.
    if inner.contains(g.ui.mouse) && (g.ui.lpressed || g.ui.ldown) && !g.ui.click_used {
        if g.ui.lpressed {
            g.ui.click_used = true;
        }
        let x = ((g.ui.mouse.x - inner.x) / kx).clamp(0.0, dims.x as f32 - 1.0);
        let y = ((g.ui.mouse.y - inner.y) / ky).clamp(0.0, dims.y as f32 - 1.0);
        g.cam.tx = x;
        g.cam.ty = y;
        g.cam.x = x;
        g.cam.y = y;
    }
    if inner.contains(g.ui.mouse) {
        let x = ((g.ui.mouse.x - inner.x) / kx) as i16;
        let y = ((g.ui.mouse.y - inner.y) / ky) as i16;
        g.ui.tooltip = Some((format!("{x}, {y}, z {z}: click to go there"), vec2(g.ui.mouse.x - 120.0 * s, g.ui.mouse.y + 18.0 * s)));
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimap_colors_of_synthetic_world() {
        let w = World::synthetic(32, 32, 20);
        // The top level is open sky: it shows the ground below (dimmed), never panics.
        let top = (w.dims.z - 1) as i16;
        let mut seen = 0;
        for y in 0..32 {
            for x in 0..32 {
                if tile_color(&w, x, y, top).is_some() {
                    seen += 1;
                }
            }
        }
        assert!(seen > 0);
        assert!(tile_color(&w, 40, 40, 0).is_none());
    }
}
