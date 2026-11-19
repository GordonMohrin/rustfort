//! [fire15] Fire on the map: flickering flames on burning tiles, grey smoke, burning units
//! flash red and yellow (w:Fire "Dangers"), and a status line while anything burns
//! (temperature model on/off, burning tiles, units, hot items).
//!
//! Screenshot states (`--ui`): `temperature:on|off`, `ignite:x/y/z` (sets a tile on fire),
//! `burn-unit:ID` (sets a unit on fire), `fire-run:N` (runs N ticks headless).

use super::col;
use crate::game::Game;
use macroquad::prelude::*;
use rf_core::Pos;
use rf_sim::fire15_api::FireMark;

pub fn draw_map_overlay(g: &mut Game) {
    let r = g.sim.fire_report();
    if r.burning_tiles == 0 && r.burning_units == 0 && r.smoke_tiles == 0 {
        return;
    }
    let z = g.cam.z as i16;
    let tp = g.cam.tile_px();
    let (x0, y0, x1, y1) = crate::render::visible_range(g);
    let mut marks: Vec<(Pos, FireMark)> = Vec::new();
    g.sim.fire_marks(z, &mut marks);
    let t = get_time();
    for (p, m) in marks {
        if p.x < x0 || p.x >= x1 || p.y < y0 || p.y >= y1 {
            continue;
        }
        let a = g.cam.to_screen(p.x as f32, p.y as f32);
        match m {
            FireMark::Fire => {
                let f = 0.5 + 0.5 * ((t * 9.0 + (p.x as f64) * 1.7 + (p.y as f64) * 2.3).sin() as f32);
                draw_rectangle(a.x, a.y, tp, tp, Color::new(1.0, 0.35 + 0.4 * f, 0.05, 0.40 + 0.2 * f));
            }
            FireMark::Smoke => draw_rectangle(a.x, a.y, tp, tp, Color::new(0.45, 0.45, 0.48, 0.22)),
        }
    }
    // Burning units flash red and yellow.
    let flash = ((t * 6.0) as i64) % 2 == 0;
    let c = if flash { Color::new(1.0, 0.15, 0.1, 0.95) } else { Color::new(1.0, 0.9, 0.1, 0.95) };
    let ids: Vec<(i32, Pos)> = g.sim.world.units.iter().filter(|u| u.pos.z == z && !u.flags.dead).map(|u| (u.id, u.pos)).collect();
    for (id, p) in ids {
        if p.x < x0 || p.x >= x1 || p.y < y0 || p.y >= y1 || !g.sim.is_burning_unit(id) {
            continue;
        }
        let a = g.cam.to_screen(p.x as f32, p.y as f32);
        draw_rectangle_lines(a.x, a.y, tp, tp, (tp / 8.0).max(2.0), c);
    }
    // Status line.
    let msg = format!(
        "Fire: {} burning tiles, {} burning creatures, {} hot items{}",
        r.burning_tiles,
        r.burning_units,
        r.hot_items,
        if g.sim.temperature_enabled() { "" } else { " (temperature off)" }
    );
    let s = g.ui.scale;
    let w = g.font.width(&msg, s) + 12.0 * s;
    let y = super::topbar::TOPBAR_H * s + 40.0 * s;
    let rect = Rect::new((screen_width() - w) / 2.0, y, w, crate::font::GH * s + 8.0 * s);
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::new(0.08, 0.02, 0.0, 0.85));
    g.font.draw(&msg, rect.x + 6.0 * s, rect.y + 4.0 * s, s, col::TEXT);
}

/// Screenshot/script states (see module docs).
pub fn ui_arg(g: &mut Game, k: &str, v: &str) -> bool {
    match k {
        "temperature" => g.sim.set_temperature_enabled(v != "off"),
        "ignite" => {
            let n: Vec<i16> = v.split(['/', ',']).filter_map(|x| x.trim().parse().ok()).collect();
            if n.len() == 3 {
                let p = Pos::new(n[0], n[1], n[2]);
                if !g.sim.ignite_tile(p) {
                    eprintln!("ignite: nothing burns at {p:?} (temperature on?)");
                }
                g.cam.focus(p);
            }
        }
        "burn-unit" => {
            if let Ok(id) = v.parse::<i32>() {
                g.sim.ignite_unit(id, 0);
            }
        }
        "fire-run" => {
            let n = v.parse::<u32>().unwrap_or(100);
            g.sim.run_ticks(n);
        }
        _ => return false,
    }
    true
}
