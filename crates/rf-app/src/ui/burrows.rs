//! Burrows (DF hotkey `Shift+u`): the burrow list above the toolbar, "+ New burrow" (drag a
//! box, `Sim::create_burrow`), the civilian alert toggle (`Sim::set_civilian_alert`) and the
//! burrow tile overlay on the map, which DF only shows while the burrow menu is open.

use super::{clip, col, list_row, panel, text_button, wheel_over, BtnState};
use crate::font::GH;
use crate::game::{Game, Tool};
use macroquad::prelude::*;
use rf_core::Pos;
use std::collections::HashSet;

/// Burrow colors by list index (DF paints burrows in distinct colors).
const COLORS: [[u8; 3]; 6] = [[230, 120, 230], [110, 200, 245], [245, 205, 90], [130, 225, 130], [245, 135, 110], [190, 160, 250]];

pub fn burrow_color(index: usize) -> Color {
    let c = COLORS[index % COLORS.len()];
    Color::from_rgba(c[0], c[1], c[2], 255)
}

/// Turn the civilian alert on for `id` (or off with None).
pub fn set_alert(g: &mut Game, id: Option<i32>) {
    g.sim.set_civilian_alert(id);
    g.civ_alert = id;
    let text = match id {
        Some(b) => {
            let name = g.sim.world.burrows.iter().find(|x| x.id == b).map(|x| x.name.clone()).unwrap_or_default();
            format!("Civilian alert: all citizens go to {name}")
        }
        None => "Civilian alert lifted".to_string(),
    };
    g.set_status(text);
}

/// Burrow menu above the middle toolbar. Returns its top y.
pub fn draw_burrow_menu(g: &mut Game, toolbar_y: f32) -> f32 {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let pad = 8.0 * s;
    let n = g.sim.world.burrows.len();
    let rows_max = 8usize;
    let row_h = lh + 4.0 * s;
    let w = 60.0 * 8.0 * s;
    let h = pad * 2.0 + lh + 6.0 * s + rows_max.min(n.max(1)) as f32 * row_h + 32.0 * s + lh;
    let x = ((screen_width() - w) / 2.0).max(4.0);
    let r = Rect::new(x, toolbar_y - h - 4.0 * s, w, h);
    panel(&mut g.ui, r);
    let (tx, mut y) = (r.x + pad, r.y + pad);
    let painting = g.tool == Tool::Burrow;
    let title = if painting { "Burrows: drag a box for the new burrow (change z while dragging = 3D)".to_string() } else { format!("Burrows ({n})") };
    g.font.draw(&clip(&title, ((w - 2.0 * pad) / (8.0 * s)) as usize), tx, y, s, col::TEXT_HI);
    y += lh + 6.0 * s;
    let list = Rect::new(tx, y, w - 2.0 * pad, rows_max.min(n.max(1)) as f32 * row_h);
    let step = wheel_over(&g.ui, &mut g.input, list);
    g.burrow_ui.scroll = (g.burrow_ui.scroll as i32 + step).clamp(0, n.saturating_sub(rows_max) as i32) as usize;
    if n == 0 {
        g.font.draw("No burrows. \"+ New burrow\" and drag a box on the map.", tx, y, s, col::TEXT_DIM);
    }
    let items: Vec<(usize, i32, String, usize, usize)> = g
        .sim
        .world
        .burrows
        .iter()
        .enumerate()
        .map(|(i, b)| (i, b.id, b.name.clone(), b.tiles.len(), b.units.len()))
        .collect();
    let mut select = None;
    let mut toggle = None;
    for (i, id, name, tiles, units) in items.iter().skip(g.burrow_ui.scroll).take(rows_max) {
        let rr = Rect::new(tx - 3.0 * s, y - 2.0 * s, list.w + 6.0 * s, row_h - 2.0 * s);
        let ab = Rect::new(rr.x + rr.w - 15.0 * 8.0 * s, y - 2.0 * s, 15.0 * 8.0 * s, row_h - 4.0 * s);
        let alert_on = g.civ_alert == Some(*id);
        // The alert button first so its click is not taken by the row.
        let label = if alert_on { "! Alert ON" } else { "Alert" };
        if text_button(&mut g.ui, &g.font, ab, label, if alert_on { BtnState::Active } else { BtnState::Normal }) {
            toggle = Some(*id);
        }
        if g.ui.hovered(ab) {
            g.ui.tooltip = Some(("Civilian alert: restrict all citizens to this burrow".into(), vec2(ab.x, ab.y - 4.0)));
        }
        let rsel = Rect::new(rr.x, rr.y, rr.w - ab.w - 4.0 * s, rr.h);
        if list_row(&mut g.ui, rsel, g.burrow_ui.selected == Some(*id)) {
            select = Some(*id);
        }
        let c = burrow_color(*i);
        draw_rectangle(tx, y + 1.0 * s, GH * s - 2.0 * s, GH * s - 2.0 * s, c);
        g.font.draw(&clip(name, 26), tx + GH * s + 6.0 * s, y, s, col::TEXT);
        let info = if *tiles == 0 { "tiles not exported".to_string() } else { format!("{tiles} tiles") };
        let info = format!("{info}, {units} units");
        g.font.draw(&info, ab.x - 8.0 * s - g.font.width(&info, s), y, s, col::TEXT_DIM);
        y += row_h;
    }
    if let Some(id) = select {
        g.burrow_ui.selected = Some(id);
    }
    if let Some(id) = toggle {
        let next = if g.civ_alert == Some(id) { None } else { Some(id) };
        set_alert(g, next);
    }
    let by = r.y + r.h - pad - 26.0 * s - lh;
    let bw = 18.0 * 8.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(tx, by, bw, 26.0 * s), "+ New burrow", if painting { BtnState::Active } else { BtnState::Normal }) {
        g.tool = if painting { Tool::None } else { Tool::Burrow };
        g.drag = None;
    }
    if let Some(sel) = g.burrow_ui.selected {
        let first = g.sim.world.burrows.iter().find(|b| b.id == sel).and_then(|b| b.tiles.first().copied());
        if let Some(p) = first {
            if text_button(&mut g.ui, &g.font, Rect::new(tx + bw + 8.0 * s, by, 14.0 * 8.0 * s, 26.0 * s), "Show", BtnState::Normal) {
                g.cam.focus(p);
            }
        }
    }
    let alert_txt = match g.civ_alert {
        Some(id) => format!("Civilian alert: {}", g.sim.world.burrows.iter().find(|b| b.id == id).map(|b| b.name.as_str()).unwrap_or("?")),
        None => "No civilian alert".to_string(),
    };
    g.font.draw(&alert_txt, tx, r.y + r.h - pad - lh + 4.0 * s, s, if g.civ_alert.is_some() { col::TEXT_BAD } else { col::TEXT_DIM });
    r.y
}

/// Draw burrow tiles of the current z-level (all burrows while the menu is open, the selected
/// one brighter with its outline). Called from the map renderer.
pub fn draw_burrow_overlay(g: &Game, x0: i16, y0: i16, x1: i16, y1: i16) {
    if g.submenu != crate::game::Submenu::Burrow {
        return;
    }
    let z = g.cam.z as i16;
    let inside = |p: &Pos| p.z == z && p.x >= x0 && p.x < x1 && p.y >= y0 && p.y < y1;
    for (i, b) in g.sim.world.burrows.iter().enumerate() {
        let sel = g.burrow_ui.selected == Some(b.id);
        let c = burrow_color(i);
        let fill = Color::new(c.r, c.g, c.b, if sel { 0.30 } else { 0.14 });
        let on_z: Vec<Pos> = b.tiles.iter().filter(|p| inside(p)).copied().collect();
        if on_z.is_empty() {
            continue;
        }
        let set: HashSet<(i16, i16)> = if sel { on_z.iter().map(|p| (p.x, p.y)).collect() } else { HashSet::new() };
        for p in &on_z {
            let r = crate::render::tile_rect(g, p.x, p.y);
            draw_rectangle(r.x, r.y, r.w, r.h, fill);
            if sel {
                let t = (r.w / 16.0).max(1.5);
                let edge = Color::new(c.r, c.g, c.b, 0.95);
                if !set.contains(&(p.x, p.y - 1)) {
                    draw_line(r.x, r.y + t / 2.0, r.x + r.w, r.y + t / 2.0, t, edge);
                }
                if !set.contains(&(p.x, p.y + 1)) {
                    draw_line(r.x, r.y + r.h - t / 2.0, r.x + r.w, r.y + r.h - t / 2.0, t, edge);
                }
                if !set.contains(&(p.x - 1, p.y)) {
                    draw_line(r.x + t / 2.0, r.y, r.x + t / 2.0, r.y + r.h, t, edge);
                }
                if !set.contains(&(p.x + 1, p.y)) {
                    draw_line(r.x + r.w - t / 2.0, r.y, r.x + r.w - t / 2.0, r.y + r.h, t, edge);
                }
            }
        }
    }
}
