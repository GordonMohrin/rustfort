//! UI entry point plus overlays: announcements, hover tooltip, Esc menu.

use super::panels::{unit_name, unit_profession};
use super::{col, panel, text_button, BtnState};
use crate::font::GH;
use crate::game::{Game, MenuPage, Screen, Tool};
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
    // Announcements stay out of the way of the big screens.
    // Announcements stay left of the right-hand sheet.
    let ann_right = if g.selected != crate::game::Selected::None { right - super::panels::SHEET_W * s - 8.0 * s } else { screen_width() - 40.0 * s };
    let ann_h = if g.screen == Screen::None { draw_announcements(g, toolbar_top.min(tb_y), ann_right) } else { 0.0 };
    super::panels::draw_citizens(g, top, toolbar_top.min(tb_y) - ann_h - 6.0 * s);
    super::orders::draw_orders(g);
    super::labor::draw_labor(g);
    super::military::draw_military(g);
    super::reports::draw_reports(g);
    draw_status(g, toolbar_top.min(tb_y));
    if g.menu_open {
        draw_menu(g);
    }
    super::alerts::draw_alert(g);
    // Button tooltips win over the map tooltip.
    if let Some((t, p)) = g.ui.tooltip.take() {
        tooltip_box(g, &[(t, col::TEXT)], p, true);
    } else if !g.ui.over_ui && !g.menu_open && g.alert.is_none() && g.screen == Screen::None && !matches!(g.tool, Tool::Build(_)) {
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
        let hostile = g.hostiles.binary_search(&u.id).is_ok();
        let tag = if hostile { if u.flags.caged { " (hostile, caged)" } else { " (hostile)" } } else { "" };
        lines.push((format!("{}, {}{tag}", unit_name(u), unit_profession(u)), if hostile { col::TEXT_BAD } else { col::TEXT_HI }));
    }
    for b in w.buildings.iter().filter(|b| b.contains(p)).take(2) {
        let n = if b.kind == "Stockpile" {
            let c = b.stockpile.as_ref().map(|s| s.categories.len()).unwrap_or(0);
            if c > 3 { "Stockpile (all)".to_string() } else { format!("Stockpile ({})", b.stockpile.as_ref().map(|s| s.categories.join(", ")).unwrap_or_default()) }
        } else if b.kind == "Civzone" {
            format!("{} zone", super::words(&b.subtype))
        } else {
            let t = super::building::building_title(g, b);
            if b.built { t } else { format!("{t} (construction)") }
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
fn draw_announcements(g: &mut Game, bottom: f32, right: f32) -> f32 {
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
        #[allow(unreachable_patterns)]
        let c = match l.kind {
            AnnouncementKind::Info => col::TEXT,
            AnnouncementKind::Job => col::TEXT_CYAN,
            AnnouncementKind::Warning => col::TEXT_HI,
            AnnouncementKind::Death => col::TEXT_BAD,
            // Kinds the sim adds later (e.g. combat) use the Reports colors.
            k => super::reports::cat_color(k, l.cat as usize),
        };
        let maxc = (((right - 8.0 * s) / (crate::font::GW * s)) as usize).clamp(10, 90);
        let text = super::clip(&l.text, maxc);
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
    if g.menu.page == MenuPage::Load {
        draw_load_menu(g);
    } else {
        let (w, h) = (380.0 * s, 270.0 * s);
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
        let saving = g.saving.is_some();
        let save_label = if saving { "Saving..." } else { "Save game (F5)" };
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), save_label, if saving { BtnState::Disabled } else { BtnState::Normal }) {
            g.start_save();
        }
        by += bh + 10.0 * s;
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Load game", BtnState::Normal) {
            g.open_load_menu();
        }
        by += bh + 10.0 * s;
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Quit to desktop", BtnState::Normal) {
            g.quit = true;
        }
        by += bh + 8.0 * s;
        let dir = crate::save::save_dir();
        let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        let shown = dir.display().to_string();
        let shown = shown.strip_prefix(r"\\?\").unwrap_or(&shown).to_string();
        let maxc = ((w - 20.0 * s) / (8.0 * s)) as usize;
        let note = if shown.chars().count() + 7 > maxc {
            let tail: String = shown.chars().rev().take(maxc.saturating_sub(10)).collect::<Vec<_>>().into_iter().rev().collect();
            format!("Saves: ...{tail}")
        } else {
            format!("Saves: {shown}")
        };
        g.font.draw(&note, r.x + (w - g.font.width(&note, s)) / 2.0, by, s, col::TEXT_DIM);
    }
    // Swallow clicks on the dimmed background.
    if g.ui.lpressed {
        g.ui.click_used = true;
    }
}

/// "Load game": the saves folder, newest first.
fn draw_load_menu(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    let (w, h) = ((720.0 * s).min(sw - 40.0), (480.0 * s).min(sh - 80.0));
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    panel(&mut g.ui, r);
    let pad = 14.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    g.font.draw("Load game", x, y, 1.5 * s, col::TEXT_HI);
    let dir = crate::save::save_dir();
    let dt = crate::ui::clip(&format!("{}", dir.display()), 50);
    g.font.draw(&dt, r.x + r.w - pad - g.font.width(&dt, s), y + 4.0 * s, s, col::TEXT_DIM);
    y += 2.0 * lh;
    for (i, hd) in ["Fort", "Game date", "Saved", "Size"].iter().enumerate() {
        g.font.draw(hd, x + [0.0, 0.32, 0.56, 0.86][i] * (w - 2.0 * pad), y, s, col::BORDER_HI);
    }
    y += lh;
    draw_line(x, y, r.x + r.w - pad, y, 1.0, col::BORDER);
    y += 4.0 * s;
    let row_h = lh + 6.0 * s;
    let area = Rect::new(x, y, w - 2.0 * pad, r.y + r.h - pad - 36.0 * s - y);
    let rows = ((area.h / row_h) as usize).max(1);
    let n = g.menu.saves.len();
    let step = crate::ui::wheel_over(&g.ui, &mut g.input, area);
    g.menu.scroll = (g.menu.scroll as i32 + step * 2).clamp(0, n.saturating_sub(rows) as i32) as usize;
    if n == 0 {
        g.font.draw("No saves yet. Save with F5 or \"Save game\".", x, y + 4.0 * s, s, col::TEXT_DIM);
    }
    let mut load = None;
    for (k, sv) in g.menu.saves.iter().enumerate().skip(g.menu.scroll).take(rows) {
        let rr = Rect::new(x, y, area.w, row_h - 2.0 * s);
        if k % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        if crate::ui::list_row(&mut g.ui, rr, false) {
            load = Some(sv.path.clone());
        }
        let ty = y + 3.0 * s;
        let cx = |i: usize| x + [0.0, 0.32, 0.56, 0.86][i] * area.w;
        let fort = if sv.header.fort_native.is_empty() { sv.header.fort.clone() } else { format!("{} ({})", sv.header.fort, sv.header.fort_native) };
        g.font.draw(&crate::ui::clip(&fort, 26), cx(0) + 4.0 * s, ty, s, col::TEXT);
        g.font.draw(&crate::ui::clip(&sv.header.date, 20), cx(1), ty, s, col::TEXT);
        g.font.draw(&crate::save::fmt_unix(sv.header.saved_unix), cx(2), ty, s, col::TEXT_DIM);
        g.font.draw(&crate::save::fmt_bytes(sv.bytes), cx(3), ty, s, col::TEXT_DIM);
        if g.ui.hovered(rr) {
            g.ui.tooltip = Some((format!("{} - click to load", sv.path.display()), vec2(rr.x, rr.y - 4.0)));
        }
        y += row_h;
    }
    let bh = 26.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(x, r.y + r.h - pad - bh, 14.0 * 8.0 * s, bh), "Back", BtnState::Normal) {
        g.menu.page = MenuPage::Main;
    }
    if let Some(p) = load {
        g.load_save(&p);
    }
}
