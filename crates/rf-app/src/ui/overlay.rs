//! UI entry point plus overlays: announcements, hover tooltip, Esc menu.

use super::panels::{unit_name, unit_profession};
use super::{col, panel, text_button, BtnState};
use crate::font::GH;
use crate::game::{Game, Tool};
use crate::render::tile_desc;
use macroquad::prelude::*;
use rf_sim::AnnouncementKind;

/// Draw the whole UI for this frame (after the map).
pub fn draw_ui(g: &mut Game) {
    let s = g.ui.scale;
    super::topbar::draw_topbar(g);
    let toolbar_top = super::toolbar::draw_toolbar(g);
    let tb_y = screen_height() - super::toolbar::TOOLBAR_H * s;
    super::topbar::draw_zbar(g, tb_y - 8.0 * s);
    let top = super::topbar::TOPBAR_H * s + 6.0 * s;
    let right = screen_width() - super::topbar::ZBAR_W * s - 14.0 * s;
    super::panels::draw_sheet(g, top, toolbar_top.min(tb_y) - 6.0 * s, right);
    let ann_h = draw_announcements(g, toolbar_top.min(tb_y));
    super::panels::draw_citizens(g, top, toolbar_top.min(tb_y) - ann_h - 6.0 * s);
    draw_status(g, toolbar_top.min(tb_y));
    if g.menu_open {
        draw_menu(g);
    }
    // Button tooltips win over the map tooltip.
    if let Some((t, p)) = g.ui.tooltip.take() {
        tooltip_box(g, &[(t, col::TEXT)], p, true);
    } else if !g.ui.over_ui && !g.menu_open {
        map_tooltip(g);
    }
}

fn tooltip_box(g: &Game, lines: &[(String, Color)], at: Vec2, above: bool) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let w = lines.iter().map(|(l, _)| g.font.width(l, s)).fold(0.0, f32::max) + 14.0 * s;
    let h = lines.len() as f32 * lh + 10.0 * s;
    let mut x = at.x;
    let mut y = if above { at.y - h } else { at.y };
    x = x.min(screen_width() - w - 4.0).max(2.0);
    y = y.min(screen_height() - h - 4.0).max(2.0);
    draw_rectangle(x, y, w, h, Color::new(0.04, 0.04, 0.05, 0.95));
    draw_rectangle_lines(x, y, w, h, 1.0, col::BORDER_HI);
    for (i, (l, c)) in lines.iter().enumerate() {
        g.font.draw(l, x + 7.0 * s, y + 5.0 * s + i as f32 * lh, s, *c);
    }
}

/// Hover tooltip over the map: tile description, creatures, items, building.
fn map_tooltip(g: &mut Game) {
    let Some(p) = g.hover else { return };
    let w = &g.sim.world;
    let mut lines: Vec<(String, Color)> = Vec::new();
    for u in w.units.iter().filter(|u| u.pos == p && !u.flags.dead).take(3) {
        lines.push((format!("{}, {}", unit_name(u), unit_profession(u)), col::TEXT_HI));
    }
    for b in w.buildings.iter().filter(|b| b.contains(p)).take(2) {
        let n = if b.kind == "Stockpile" {
            let c = b.stockpile.as_ref().map(|s| s.categories.len()).unwrap_or(0);
            if c > 3 { "Stockpile (all)".to_string() } else { format!("Stockpile ({})", b.stockpile.as_ref().map(|s| s.categories.join(", ")).unwrap_or_default()) }
        } else if b.subtype.is_empty() {
            b.kind.clone()
        } else {
            format!("{} ({})", b.subtype, b.kind)
        };
        lines.push((n, col::TEXT_GOOD));
    }
    let items: Vec<&rf_core::Item> = w.items.iter().filter(|it| it.pos == p && it.flags.on_ground).collect();
    for it in items.iter().take(4) {
        let d = if !it.desc.is_empty() { it.desc.clone() } else { super::pretty(&it.kind).to_lowercase() };
        lines.push((super::clip(&d, 48), col::TEXT));
    }
    if items.len() > 4 {
        lines.push((format!("... and {} more items", items.len() - 4), col::TEXT_DIM));
    }
    let mut desc = super::pretty(&tile_desc(w, p));
    if let Some(t) = w.tile(p) {
        if t.desig != 0 {
            desc.push_str(&format!(" [{:?}]", t.dig()));
        }
    }
    lines.push((desc, col::TEXT_DIM));
    // Only show after the mouse rests a moment would be nicer; keep it immediate like DF's hover box.
    let m = g.mouse();
    let off = if g.tool != Tool::None { 34.0 } else { 18.0 };
    tooltip_box(g, &lines, vec2(m.x + off, m.y + 18.0), false);
}

/// Announcement lines above the toolbar (bottom left). Click a line to jump there.
/// Returns the height used.
fn draw_announcements(g: &mut Game, bottom: f32) -> f32 {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let now = get_time();
    let max_lines = 7;
    let visible: Vec<usize> = g
        .log
        .iter()
        .enumerate()
        .filter(|(_, l)| now - l.born < 25.0)
        .map(|(i, _)| i)
        .collect();
    let start = visible.len().saturating_sub(max_lines);
    let shown = &visible[start..];
    if shown.is_empty() {
        return 0.0;
    }
    let x = 8.0 * s;
    let mut y = bottom - 8.0 * s - shown.len() as f32 * lh;
    let h = shown.len() as f32 * lh + 8.0 * s;
    let mut jump = None;
    for &i in shown {
        let l = &g.log[i];
        let age = now - l.born;
        let alpha = if age > 20.0 { ((25.0 - age) / 5.0) as f32 } else { 1.0 };
        let c = match l.kind {
            AnnouncementKind::Info => col::TEXT,
            AnnouncementKind::Job => col::TEXT_CYAN,
            AnnouncementKind::Warning => col::TEXT_HI,
            AnnouncementKind::Death => col::TEXT_BAD,
        };
        let text = super::clip(&l.text, 90);
        let w = g.font.width(&text, s);
        let rr = Rect::new(x - 3.0, y - 2.0, w + 6.0, lh);
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.0, 0.0, 0.0, 0.55 * alpha));
        let hov = l.pos.is_some() && g.ui.hovered(rr);
        if hov {
            draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, col::BORDER_HI);
        }
        g.font.draw(&text, x, y, s, Color::new(c.r, c.g, c.b, alpha));
        if l.pos.is_some() {
            g.ui.block(rr);
            if g.ui.clicked(rr) {
                jump = l.pos;
            }
        }
        y += lh;
    }
    if let Some(p) = jump {
        g.cam.focus(p);
    }
    h
}

fn draw_status(g: &mut Game, bottom: f32) {
    let Some((t, born)) = &g.status else { return };
    let age = get_time() - born;
    if age > 4.0 {
        g.status = None;
        return;
    }
    let s = g.ui.scale;
    let w = g.font.width(t, s);
    let x = (screen_width() - w) / 2.0;
    let y = bottom - 90.0 * s;
    let a = if age > 3.0 { (4.0 - age) as f32 } else { 1.0 };
    draw_rectangle(x - 8.0, y - 4.0, w + 16.0, GH * s + 8.0, Color::new(0.0, 0.0, 0.0, 0.6 * a));
    g.font.draw(t, x, y, s, Color::new(1.0, 0.9, 0.5, a));
}

fn draw_menu(g: &mut Game) {
    let s = g.ui.scale;
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
    g.ui.block(Rect::new(0.0, 0.0, sw, sh));
    let (w, h) = (360.0 * s, 200.0 * s);
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    panel(&mut g.ui, r);
    let title = "Rustfort";
    g.font.draw(title, r.x + (w - g.font.width(title, 2.0 * s)) / 2.0, r.y + 14.0 * s, 2.0 * s, col::TEXT_HI);
    let bw = w - 60.0 * s;
    let bh = 28.0 * s;
    let bx = r.x + 30.0 * s;
    let sub = crate::ui::clip(&g.label, ((w - 20.0 * s) / (8.0 * s)) as usize);
    g.font.draw(&sub, r.x + (w - g.font.width(&sub, s)) / 2.0, r.y + 42.0 * s, s, col::TEXT_DIM);
    let mut by = r.y + 64.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Return to game", BtnState::Normal) {
        g.menu_open = false;
    }
    by += bh + 10.0 * s;
    text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Save game (not yet)", BtnState::Disabled);
    by += bh + 10.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Quit to desktop", BtnState::Normal) {
        g.quit = true;
    }
    // Swallow clicks on the dimmed background.
    if g.ui.lpressed {
        g.ui.click_used = true;
    }
}
