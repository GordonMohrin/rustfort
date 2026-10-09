//! Side panels: unit sheet and tile info (right), citizens list (left).

use super::{bar, clip, col, panel, pretty, skill_level_name, text_button, BtnState};
use crate::font::GH;
use crate::game::{Game, Selected};
use crate::render::tile_desc;
use macroquad::prelude::*;
use rf_core::{Pos, Unit, NO_MAT};

pub const SHEET_W: f32 = 52.0 * 8.0 + 16.0;

pub fn unit_name(u: &Unit) -> String {
    if u.name.is_empty() {
        pretty(&u.race)
    } else {
        u.name.clone()
    }
}

pub fn unit_profession(u: &Unit) -> String {
    if !u.custom_profession.is_empty() {
        return u.custom_profession.clone();
    }
    if u.flags.animal {
        let tame = if u.flags.tame { "Tame " } else { "" };
        return format!("{tame}{}", pretty(&u.race).to_lowercase());
    }
    pretty(&u.profession)
}

/// Right-side sheet for the selected unit or tile. Returns its rect if drawn.
pub fn draw_sheet(g: &mut Game, top: f32, bottom: f32, right: f32) {
    let s = g.ui.scale;
    let w = SHEET_W * s;
    let r = Rect::new(right - w, top, w, (bottom - top).max(100.0));
    match g.selected {
        Selected::None => {}
        Selected::Unit(id) => {
            if g.sim.world.unit(id).is_none() {
                g.selected = Selected::None;
                return;
            }
            panel(&mut g.ui, r);
            unit_sheet(g, id, r);
        }
        Selected::Tile(p) => {
            panel(&mut g.ui, r);
            tile_sheet(g, p, r);
        }
    }
}

fn close_button(g: &mut Game, r: Rect) -> bool {
    let s = g.ui.scale;
    let b = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    text_button(&mut g.ui, &g.font, b, "x", BtnState::Normal)
}

fn unit_sheet(g: &mut Game, id: i32, r: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let pad = 10.0 * s;
    let maxc = ((r.w - 2.0 * pad) / (8.0 * s)) as usize;
    if close_button(g, r) {
        g.selected = Selected::None;
        return;
    }
    let status = g.sim.unit_status(id);
    let Some(u) = g.sim.world.unit(id).cloned() else { return };
    let (x, mut y) = (r.x + pad, r.y + pad);
    // Portrait box (glyph) + name like the Steam sheet header.
    let pr = Rect::new(x, y, 36.0 * s, 36.0 * s);
    draw_rectangle(pr.x, pr.y, pr.w, pr.h, Color::from_rgba(30, 28, 26, 255));
    draw_rectangle_lines(pr.x, pr.y, pr.w, pr.h, 1.0, col::BORDER);
    let code = if u.race == "DWARF" { 1 } else { u.race.bytes().next().unwrap_or(b'?') };
    g.font.glyph(code, pr.x + 6.0 * s, pr.y + 2.0 * s, 24.0 * s, 32.0 * s, col::TEXT_HI);
    let tx = x + pr.w + 8.0 * s;
    let tmax = ((r.x + r.w - pad - tx) / (8.0 * s)) as usize;
    g.font.draw(&clip(&unit_name(&u), tmax.saturating_sub(3)), tx, y, s, col::TEXT_HI);
    g.font.draw(&clip(&unit_profession(&u), tmax), tx, y + lh, s, col::TEXT);
    let act = if status.activity.is_empty() { "No Job".to_string() } else { status.activity.clone() };
    g.font.draw(&clip(&act, tmax), tx, y + 2.0 * lh, s, col::TEXT_CYAN);
    y += 3.0 * lh + 6.0 * s;
    // Tabs.
    let tabs = ["Overview", "Skills", "Labor", "Items"];
    let tw = (r.w - 2.0 * pad - 3.0 * 4.0 * s) / 4.0;
    for (i, t) in tabs.iter().enumerate() {
        let tr = Rect::new(x + i as f32 * (tw + 4.0 * s), y, tw, GH * s + 8.0 * s);
        let st = if g.unit_tab == i { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, tr, t, st) {
            g.unit_tab = i;
        }
    }
    y += GH * s + 16.0 * s;
    let bw = r.w - 2.0 * pad;
    match g.unit_tab {
        0 => {
            let need = |v: u8| {
                if v >= 75 {
                    col::TEXT_BAD
                } else if v >= 40 {
                    col::TEXT_HI
                } else {
                    col::TEXT_GOOD
                }
            };
            bar(&g.font, s, x, y, bw, "Hunger", status.hunger as f32, need(status.hunger));
            y += lh;
            bar(&g.font, s, x, y, bw, "Thirst", status.thirst as f32, need(status.thirst));
            y += lh;
            bar(&g.font, s, x, y, bw, "Tiredness", status.tiredness as f32, need(status.tiredness));
            y += lh;
            // DF stress: < 0 content, > 10000 stressed (rough UI scale).
            let st = ((u.stress as f32 + 10_000.0) / 500.0).clamp(0.0, 100.0);
            bar(&g.font, s, x, y, bw, "Stress", st, need(st as u8));
            y += lh + 6.0 * s;
            let sex = match u.sex {
                0 => "female",
                1 => "male",
                _ => "",
            };
            let lines = [
                format!("{} {sex}, age {:.0}", pretty(&u.race), u.age),
                format!("Position {}, {}, z {}", u.pos.x, u.pos.y, u.pos.z),
                if u.mood.is_empty() { String::new() } else { format!("Mood: {}", pretty(&u.mood)) },
                if u.squad_id >= 0 { format!("Squad #{}", u.squad_id) } else { String::new() },
                format!("Unit id {}{}", u.id, if u.hist_fig_id >= 0 { format!(", hist. figure {}", u.hist_fig_id) } else { String::new() }),
            ];
            for l in lines.iter().filter(|l| !l.is_empty()) {
                g.font.draw(&clip(l, maxc), x, y, s, col::TEXT);
                y += lh;
            }
            y += 6.0 * s;
            if text_button(&mut g.ui, &g.font, Rect::new(x, y, 20.0 * 8.0 * s, GH * s + 8.0 * s), "Center camera", BtnState::Normal) {
                g.cam.focus(u.pos);
            }
            y += GH * s + 16.0 * s;
            // Top skills preview.
            let mut sk = u.skills.clone();
            sk.sort_by(|a, b| b.rating.cmp(&a.rating).then(b.xp.cmp(&a.xp)));
            if !sk.is_empty() {
                g.font.draw("Top skills", x, y, s, col::BORDER_HI);
                y += lh;
                for k in sk.iter().take(5) {
                    g.font.draw(&clip(&format!("{} {}", skill_level_name(k.rating), pretty(&k.id).to_lowercase()), maxc), x, y, s, col::TEXT);
                    y += lh;
                }
            }
        }
        1 => {
            let mut sk = u.skills.clone();
            sk.sort_by(|a, b| b.rating.cmp(&a.rating).then(b.xp.cmp(&a.xp)));
            if sk.is_empty() {
                g.font.draw("No skills.", x, y, s, col::TEXT_DIM);
            }
            for k in sk {
                if y > r.y + r.h - lh - pad {
                    break;
                }
                let name = pretty(&k.id);
                let lvl = format!("{} ({})", skill_level_name(k.rating), k.rating);
                g.font.draw(&clip(&name, 24), x, y, s, col::TEXT);
                let lc = if k.rating >= 10 { col::TEXT_HI } else { col::TEXT_GOOD };
                g.font.draw(&lvl, x + 26.0 * 8.0 * s, y, s, lc);
                y += lh;
            }
        }
        2 => {
            if u.labors.is_empty() {
                g.font.draw("No labors enabled.", x, y, s, col::TEXT_DIM);
            }
            let colw = (bw / 2.0).floor();
            for (i, l) in u.labors.iter().enumerate() {
                let cx = x + (i % 2) as f32 * colw;
                let cy = y + (i / 2) as f32 * lh;
                if cy > r.y + r.h - lh - pad {
                    break;
                }
                g.font.draw("√", cx, cy, s, col::TEXT_GOOD);
                g.font.draw(&clip(&pretty(l), (colw / (8.0 * s)) as usize - 3), cx + 16.0 * s, cy, s, col::TEXT);
            }
        }
        _ => {
            if u.inventory.is_empty() {
                g.font.draw("Carries nothing.", x, y, s, col::TEXT_DIM);
            }
            for iid in &u.inventory {
                if y > r.y + r.h - lh - pad {
                    break;
                }
                let txt = match g.sim.world.item(*iid) {
                    Some(it) => item_label(g, it),
                    None => format!("item #{iid} (unknown)"),
                };
                g.font.draw(&clip(&txt, maxc), x, y, s, col::TEXT);
                y += lh;
            }
        }
    }
}

fn item_label(g: &Game, it: &rf_core::Item) -> String {
    let base = if !it.name.is_empty() {
        it.name.clone()
    } else if !it.desc.is_empty() {
        it.desc.clone()
    } else {
        let m = if it.mat != NO_MAT { g.sim.world.material(it.mat).map(|m| m.name.clone()).unwrap_or_default() } else { String::new() };
        format!("{m} {}", pretty(&it.kind).to_lowercase()).trim().to_string()
    };
    let q = ["", "-", "+", "*", "≡", "☼", "!"][it.quality.min(6) as usize];
    let base = if q.is_empty() { base } else { format!("{q}{base}{q}") };
    if it.stack > 1 {
        format!("{base} [{}]", it.stack)
    } else {
        base
    }
}

fn tile_sheet(g: &mut Game, p: Pos, r: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let pad = 10.0 * s;
    let maxc = ((r.w - 2.0 * pad) / (8.0 * s)) as usize;
    if close_button(g, r) {
        g.selected = Selected::None;
        return;
    }
    let (x, mut y) = (r.x + pad, r.y + pad);
    let desc = tile_desc(&g.sim.world, p);
    g.font.draw(&clip(&pretty(&desc), maxc - 3), x, y, s, col::TEXT_HI);
    y += lh;
    g.font.draw(&format!("x {}, y {}, z {}", p.x, p.y, p.z), x, y, s, col::TEXT_DIM);
    y += lh + 4.0 * s;
    let mut lines: Vec<(String, Color)> = Vec::new();
    {
        let w = &g.sim.world;
        if let Some(t) = w.tile(p) {
            let tt = w.tiletypes.get(t.tt);
            lines.push((format!("Tiletype: {} (#{})", tt.name, t.tt), col::TEXT));
            lines.push((format!("Shape {:?}, material class {:?}, {:?}", tt.shape, tt.material, tt.special), col::TEXT_DIM));
            if let Some(m) = (t.mat != NO_MAT).then(|| w.material(t.mat)).flatten() {
                lines.push((format!("Material: {} ({})", m.name, m.token), col::TEXT));
                let mut tags = Vec::new();
                for (b, n) in [(m.is_stone, "stone"), (m.is_ore, "ore"), (m.is_gem, "gem"), (m.is_metal, "metal"), (m.is_soil, "soil"), (m.is_wood, "wood")] {
                    if b {
                        tags.push(n);
                    }
                }
                if !tags.is_empty() {
                    lines.push((format!("  {}", tags.join(", ")), col::TEXT_DIM));
                }
            }
            let f = t.flags;
            let mut fl = Vec::new();
            use rf_core::tile_flags as tf;
            for (bit, n) in [(tf::HIDDEN, "hidden"), (tf::OUTSIDE, "outside"), (tf::LIGHT, "light"), (tf::SUBTERRANEAN, "subterranean"), (tf::AQUIFER, "aquifer"), (tf::PILE, "stockpile")] {
                if f & bit != 0 {
                    fl.push(n);
                }
            }
            if !fl.is_empty() {
                lines.push((format!("Flags: {}", fl.join(", ")), col::TEXT_DIM));
            }
            if t.liquid_depth() > 0 {
                lines.push((format!("{} depth {}/7", if t.is_magma() { "Magma" } else { "Water" }, t.liquid_depth()), col::TEXT_CYAN));
            }
            if t.desig != 0 {
                lines.push((format!("Designated: {:?}", t.dig()), col::TEXT_HI));
            }
        }
        for b in w.buildings.iter().filter(|b| b.contains(p)) {
            let what = if b.subtype.is_empty() { b.kind.clone() } else { format!("{} ({})", b.kind, b.subtype) };
            let extra = b.stockpile.as_ref().map(|sp| format!(": {}", sp.categories.join(", "))).unwrap_or_default();
            lines.push((format!("Building #{}: {what}{extra}", b.id), col::TEXT_GOOD));
        }
        for pl in w.plants.iter().filter(|pl| pl.pos == p) {
            lines.push((format!("Plant: {}{}", pretty(&pl.token).to_lowercase(), if pl.tree { " tree" } else { "" }), col::TEXT_GOOD));
        }
    }
    for (l, c) in lines {
        g.font.draw(&clip(&l, maxc), x, y, s, c);
        y += lh;
    }
    y += 6.0 * s;
    // Units here (clickable).
    let units: Vec<(i32, String)> = g.sim.world.units.iter().filter(|u| u.pos == p && !u.flags.dead).map(|u| (u.id, format!("{}, {}", unit_name(u), unit_profession(u)))).collect();
    if !units.is_empty() {
        g.font.draw("Creatures", x, y, s, col::BORDER_HI);
        y += lh;
        for (id, txt) in units {
            let rr = Rect::new(x, y - 2.0, r.w - 2.0 * pad, lh);
            let hov = g.ui.hovered(rr);
            if hov {
                draw_rectangle(rr.x, rr.y, rr.w, rr.h, col::BUTTON_HOVER);
            }
            g.font.draw(&clip(&format!("☺ {txt}"), maxc), x, y, s, col::TEXT);
            if g.ui.clicked(rr) {
                g.selected = Selected::Unit(id);
                g.unit_tab = 0;
            }
            y += lh;
        }
        y += 6.0 * s;
    }
    let items: Vec<String> = g.sim.world.items.iter().filter(|it| it.pos == p && it.flags.on_ground).map(|it| item_label(g, it)).collect();
    if !items.is_empty() {
        g.font.draw(&format!("Items ({})", items.len()), x, y, s, col::BORDER_HI);
        y += lh;
        for it in items {
            if y > r.y + r.h - lh - pad {
                break;
            }
            g.font.draw(&clip(&it, maxc), x, y, s, col::TEXT);
            y += lh;
        }
    }
}

/// Citizens list (left side), like the Steam "Creatures > Citizens" tab.
pub fn draw_citizens(g: &mut Game, top: f32, bottom: f32) {
    if !g.citizens_open {
        return;
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let w = 62.0 * 8.0 * s;
    let r = Rect::new(6.0 * s, top, w, (bottom - top).max(100.0));
    panel(&mut g.ui, r);
    let pad = 10.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let b = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, b, "x", BtnState::Normal) {
        g.citizens_open = false;
        return;
    }
    let mut cits: Vec<(i32, String, String, Pos)> = g
        .sim
        .world
        .citizens()
        .map(|u| (u.id, unit_name(u), unit_profession(u), u.pos))
        .collect();
    cits.sort_by(|a, b| a.1.cmp(&b.1));
    let animals = g.sim.world.units.iter().filter(|u| u.flags.animal && !u.flags.dead).count();
    g.font.draw(&format!("Citizens ({})", cits.len()), x, y, s, col::TEXT_HI);
    g.font.draw(&format!("Pets/livestock: {animals}"), x + 26.0 * 8.0 * s, y, s, col::TEXT_DIM);
    y += lh + 6.0 * s;
    let rows = (((r.y + r.h - pad) - y) / (lh * 2.0)).floor().max(1.0) as usize;
    // Scroll with the wheel while hovering the list.
    if g.ui.hovered(r) && g.input.wheel != 0 {
        if g.input.wheel < 0 {
            g.citizens_scroll += 3;
        } else {
            g.citizens_scroll = g.citizens_scroll.saturating_sub(3);
        }
    }
    g.citizens_scroll = g.citizens_scroll.min(cits.len().saturating_sub(rows));
    let maxc = ((r.w - 2.0 * pad) / (8.0 * s)) as usize;
    for (id, name, prof, pos) in cits.iter().skip(g.citizens_scroll).take(rows) {
        let rr = Rect::new(r.x + 4.0 * s, y - 2.0 * s, r.w - 8.0 * s, lh * 2.0 - 2.0 * s);
        let sel = g.selected == Selected::Unit(*id);
        if sel {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, col::BUTTON_ACTIVE);
        } else if g.ui.hovered(rr) {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, col::BUTTON_HOVER);
        }
        let act = g.sim.unit_status(*id).activity;
        g.font.draw("☺", x, y, s, col::TEXT_HI);
        g.font.draw(&clip(name, 30), x + 16.0 * s, y, s, col::TEXT);
        g.font.draw(&clip(prof, maxc.saturating_sub(34)), x + 34.0 * 8.0 * s, y, s, col::TEXT_DIM);
        let act = if act.is_empty() { "No Job".to_string() } else { act };
        g.font.draw(&clip(&act, maxc - 2), x + 16.0 * s, y + lh - 2.0 * s, s, col::TEXT_CYAN);
        if g.ui.clicked(rr) {
            g.selected = Selected::Unit(*id);
            g.unit_tab = 0;
            g.cam.focus(*pos);
        }
        y += lh * 2.0;
    }
    if cits.len() > rows {
        let t = format!("{}-{} of {} (wheel scrolls)", g.citizens_scroll + 1, (g.citizens_scroll + rows).min(cits.len()), cits.len());
        g.font.draw(&t, x, r.y + r.h - pad - GH * s + 4.0 * s, s, col::TEXT_DIM);
    }
}
