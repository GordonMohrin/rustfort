//! Top bar (date, season, pause/speed, ticks/s, population, z-level) and the elevation bar
//! at the right edge.

use super::{col, panel, text_button, BtnState};
use crate::font::GH;
use crate::game::{date_string, season_string, Game, Speed, ZClass};
use macroquad::prelude::*;

pub const TOPBAR_H: f32 = 30.0;
pub const ZBAR_W: f32 = 14.0;

pub fn draw_topbar(g: &mut Game) {
    let s = g.ui.scale;
    let sw = screen_width();
    let h = TOPBAR_H * s;
    let r = Rect::new(0.0, 0.0, sw, h);
    g.ui.block(r);
    draw_rectangle(r.x, r.y, r.w, r.h, col::PANEL_BG);
    draw_line(0.0, h, sw, h, 2.0, col::BORDER);
    let ty = (h - GH * s) / 2.0;

    // Pause/play + speed buttons.
    let mut x = 6.0 * s;
    let bw = 34.0 * s;
    let bh = h - 8.0 * s;
    let by = 4.0 * s;
    let pause_label = if g.paused { "►" } else { "║" };
    let pr = Rect::new(x, by, bh * 1.2, bh);
    if text_button(&mut g.ui, &g.font, pr, pause_label, if g.paused { BtnState::Active } else { BtnState::Normal }) {
        g.paused = !g.paused;
    }
    x += pr.w + 4.0 * s;
    for sp in Speed::ALL {
        let st = if g.speed == sp { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(x, by, bw, bh), sp.label(), st) {
            g.speed = sp;
            g.paused = false;
        }
        x += bw + 3.0 * s;
    }
    x += 8.0 * s;
    if g.paused {
        let blink = (get_time() * 2.0) as i64 % 2 == 0;
        g.font.draw("PAUSED", x, ty, s, if blink { col::TEXT_HI } else { col::BORDER_HI });
    } else {
        g.font.draw("running", x, ty, s, col::TEXT_GOOD);
    }
    x += 9.0 * 8.0 * s;

    // Date and season (center-left).
    let cal = g.sim.world.calendar;
    let date = date_string(&cal);
    x += g.font.draw(&date, x, ty, s, col::TEXT) + 14.0 * s;
    let season = season_string(&cal);
    let scol = match cal.month() / 3 {
        0 => Color::from_rgba(130, 220, 110, 255),
        1 => Color::from_rgba(250, 220, 90, 255),
        2 => Color::from_rgba(230, 140, 60, 255),
        _ => Color::from_rgba(170, 210, 255, 255),
    };
    x += g.font.draw(&season, x, ty, s, scol) + 18.0 * s;

    // Right side first: population and z-level / elevation.
    let w = &g.sim.world;
    let pop = w.citizens().count();
    let z = g.cam.z;
    let elev = if w.meta.source.starts_with("export") {
        format!("  Elev {}", z + w.meta.region_z - 100)
    } else {
        String::new()
    };
    let ztxt = format!("z {z}{elev}");
    let popt = format!("☺ {pop}");
    let zw = g.font.width(&ztxt, s);
    let pw = g.font.width(&popt, s);
    let rx = sw - ZBAR_W * s - 12.0 * s - zw;
    g.font.draw(&ztxt, rx, ty, s, col::TEXT_HI);
    let px = rx - pw - 22.0 * s;
    g.font.draw(&popt, px, ty, s, col::TEXT);

    // Speed telemetry: ticks/s actually achieved vs target (dropped when there is no room).
    let target = if g.paused { 0.0 } else { g.speed.tps().unwrap_or(f64::INFINITY) };
    let tps_txt = format!("{:.0} ticks/s", g.tps);
    let good = g.paused || g.tps >= target * 0.95 || target.is_infinite();
    let tcol = if g.paused { col::TEXT_DIM } else if good { col::TEXT_GOOD } else { col::TEXT_BAD };
    let room = px - 16.0 * s;
    if x + g.font.width(&tps_txt, s) < room {
        x += g.font.draw(&tps_txt, x, ty, s, tcol) + 6.0 * s;
        let detail = format!("({:.0} FPS, sim {:.1} ms)", g.fps, g.sim_ms);
        if x + g.font.width(&detail, s) < room {
            g.font.draw(&detail, x, ty, s, col::TEXT_DIM);
        }
    }
}

/// Vertical z-level bar at the right edge: sky / surface / underground, with the current
/// level marked. Click to jump.
pub fn draw_zbar(g: &mut Game, bottom: f32) {
    let s = g.ui.scale;
    let top = TOPBAR_H * s + 6.0 * s;
    let x = screen_width() - ZBAR_W * s - 4.0 * s;
    let h = (bottom - top).max(40.0);
    let nz = g.zclass.len().max(1);
    let r = Rect::new(x, top, ZBAR_W * s, h);
    panel(&mut g.ui, Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0));
    let zh = h / nz as f32;
    for (z, c) in g.zclass.iter().enumerate() {
        // z = 0 is the bottom of the map.
        let y = top + h - (z as f32 + 1.0) * zh;
        let color = match c {
            ZClass::Sky => Color::from_rgba(70, 110, 165, 255),
            ZClass::Surface => Color::from_rgba(80, 150, 60, 255),
            ZClass::Underground => Color::from_rgba(105, 78, 50, 255),
            ZClass::Unknown => Color::from_rgba(35, 30, 28, 255),
        };
        draw_rectangle(x, y, r.w, zh.max(1.0), color);
    }
    let cz = g.cam.z as f32;
    let cy = top + h - (cz + 0.5) * zh;
    draw_rectangle(x - 2.0, cy - 1.5, r.w + 4.0, 3.0, col::TEXT_HI);
    g.font.glyph(16, x - 10.0 * s, cy - 6.0 * s, 8.0 * s, 12.0 * s, col::TEXT_HI);
    if g.ui.hovered(r) {
        let hz = ((top + h - g.ui.mouse.y) / zh).floor().clamp(0.0, nz as f32 - 1.0) as i32;
        g.ui.tooltip = Some((format!("z {hz}"), vec2(x - 60.0 * s, g.ui.mouse.y - 8.0)));
        if g.ui.ldown && !g.ui.click_used {
            g.cam.z = hz;
            g.ui.click_used = true;
        }
    }
}
