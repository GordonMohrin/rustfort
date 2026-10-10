//! Top bar (date, season, pause/speed, ticks/s, population, z-level) and the elevation bar
//! at the right edge.

use super::{col, panel, text_button, BtnState};
use crate::font::GH;
use crate::game::{date_string, season_string, Game, Speed, Stocks, ZClass};
use macroquad::prelude::*;
use rf_core::World;

/// Item kinds DF counts as food (prepared meals and raw edibles). PLANT and PLANT_GROWTH
/// only count when the plant raw says they are edible raw or cooked.
const FOOD_KINDS: &[&str] = &["FOOD", "MEAT", "FISH", "CHEESE", "EGG", "PLANT", "PLANT_GROWTH"];

/// Drinks (sum of stack sizes, like DF's stocks screen), food units and food variety
/// (distinct edible materials). Rotten and dumped items do not count.
pub fn count_stocks(w: &World, now: f64) -> Stocks {
    let mut st = Stocks { at: now, ..Default::default() };
    let mut kinds: Vec<(u16, u8)> = Vec::new();
    for it in &w.items {
        if it.flags.rotten || it.flags.dump || it.flags.construction {
            continue;
        }
        let k = it.kind.as_str();
        if k == "DRINK" {
            st.drinks += it.stack.max(1);
            continue;
        }
        let Some(ki) = FOOD_KINDS.iter().position(|f| *f == k) else { continue };
        if (k == "PLANT" || k == "PLANT_GROWTH") && !plant_edible(w, it.mat) {
            continue;
        }
        st.food += it.stack.max(1);
        let key = (it.mat, ki as u8);
        if !kinds.contains(&key) {
            kinds.push(key);
        }
    }
    st.food_kinds = kinds.len();
    st
}

/// Edibility of a plant material (`PLANT:<token>:...`) from the save's plant raws; without
/// plant raws (old exports) every plant counts.
fn plant_edible(w: &World, mat: u16) -> bool {
    if w.plant_raws.is_empty() {
        return true;
    }
    let Some(m) = w.material(mat) else { return true };
    let tok = m.token.split(':').nth(1).unwrap_or("");
    match w.plant_raws.iter().find(|p| p.token == tok) {
        Some(p) => p.edible_raw || p.edible_cooked,
        None => true,
    }
}

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
    // [round 6] '?' opens the controls overview (F1).
    let hw = bh;
    let hr = Rect::new(sw - ZBAR_W * s - 10.0 * s - hw, by, hw, bh);
    if text_button(&mut g.ui, &g.font, hr, "?", if g.help_open { BtnState::Active } else { BtnState::Normal }) {
        g.help_open = !g.help_open;
    }
    if g.ui.hovered(hr) {
        let k = g.binds.describe("RF_HELP");
        g.ui.tooltip = Some((format!("Controls and keys ({k})"), vec2(hr.x - 160.0 * s, h + 26.0 * s)));
    }
    // [app8] Minimap toggle left of '?'.
    let mr = Rect::new(hr.x - hw - 4.0 * s, by, hw, bh);
    if text_button(&mut g.ui, &g.font, mr, "▓", if g.r8.minimap { BtnState::Active } else { BtnState::Normal }) {
        g.r8.minimap = !g.r8.minimap;
        g.settings.minimap = g.r8.minimap;
        g.persist_settings();
    }
    if g.ui.hovered(mr) {
        g.ui.tooltip = Some(("Minimap of this level".into(), vec2(mr.x - 120.0 * s, h + 26.0 * s)));
    }
    let hr = mr;
    let rx = hr.x - 12.0 * s - zw;
    g.font.draw(&ztxt, rx, ty, s, col::TEXT_HI);
    let mut px = rx - pw - 22.0 * s;
    g.font.draw(&popt, px, ty, s, col::TEXT);
    // [round 4] Population tooltip: citizens, children, cap, next migrants, caravan.
    let pr = Rect::new(px - 4.0 * s, 0.0, pw + 8.0 * s, h);
    if g.ui.hovered(pr) {
        let lines = super::round4::population_lines(g);
        g.ui.tooltip = Some((lines.join("\n"), vec2(px - 220.0 * s, h + 6.0 * s + lines.len() as f32 * g.ui.line_h() + 10.0 * s)));
    }
    // [round 3] Hostiles on the map (click: center on the next one) and the civilian alert.
    let active: Vec<(i32, rf_core::Pos)> = g
        .hostiles
        .iter()
        .filter_map(|id| g.sim.world.unit(*id))
        .filter(|u| !u.flags.caged && !u.flags.dead)
        .map(|u| (u.id, u.pos))
        .collect();
    if !active.is_empty() {
        let n = active.len();
        let t = format!("! {n} hostile{}", if n == 1 { "" } else { "s" });
        let tw = g.font.width(&t, s);
        px -= tw + 22.0 * s;
        let blink = (get_time() * 2.0) as i64 % 2 == 0;
        let hr = Rect::new(px - 6.0 * s, 3.0 * s, tw + 12.0 * s, h - 6.0 * s);
        draw_rectangle(hr.x, hr.y, hr.w, hr.h, Color::new(0.45, 0.02, 0.02, if blink { 0.9 } else { 0.6 }));
        draw_rectangle_lines(hr.x, hr.y, hr.w, hr.h, 1.0, col::TEXT_BAD);
        g.font.draw(&t, px, ty, s, Color::from_rgba(255, 220, 210, 255));
        if g.ui.hovered(hr) {
            g.ui.tooltip = Some(("Hostile creatures on the map: click to center on the next one".into(), vec2(hr.x, h + 26.0 * s)));
        }
        if g.ui.clicked(hr) {
            let k = g.hostile_cycle % n;
            g.hostile_cycle = k + 1;
            let (id, p) = active[k];
            g.cam.focus(p);
            g.selected = crate::game::Selected::Unit(id);
            g.unit_tab = 0;
        }
    }
    if let Some(b) = g.civ_alert {
        let name = g.sim.world.burrows.iter().find(|x| x.id == b).map(|x| x.name.clone()).unwrap_or_default();
        let t = format!("Civilian alert: {name}");
        let tw = g.font.width(&t, s);
        px -= tw + 22.0 * s;
        g.font.draw(&t, px, ty, s, col::TEXT_BAD);
    }
    // Stocks: drinks and food (red when below 2 per citizen).
    let st = g.stocks;
    let low = (pop as i32 * 2).max(10);
    let mut food_t = format!("Food {} ({} kinds)", st.food, st.food_kinds);
    let drink_t = format!("Drink {}", st.drinks);
    let dw = g.font.width(&drink_t, s);
    if px - g.font.width(&food_t, s) - dw - 36.0 * s < x + 8.0 * s {
        food_t = format!("Food {} ({})", st.food, st.food_kinds);
    }
    let fw = g.font.width(&food_t, s);
    let sx = px - fw - dw - 36.0 * s;
    if sx > x + 8.0 * s {
        let dc = if st.drinks < low { col::TEXT_BAD } else { col::TEXT_CYAN };
        let fc = if st.food < low { col::TEXT_BAD } else { col::TEXT_GOOD };
        g.font.draw(&drink_t, sx, ty, s, dc);
        g.font.draw(&food_t, sx + dw + 14.0 * s, ty, s, fc);
        let sr = Rect::new(sx, 0.0, dw + fw + 14.0 * s, h);
        if g.ui.hovered(sr) {
            let tip = format!("Stocks: {} drinks, {} food in {} kinds (recounted every second)", st.drinks, st.food, st.food_kinds);
            g.ui.tooltip = Some((tip, vec2(sx, h + 26.0 * s)));
        }
        px = sx;
    }

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
