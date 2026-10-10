//! Side panels: unit sheet and tile info (right), citizens list (left).

use super::{bar, clip, col, panel, pretty, text_button, BtnState};
use crate::font::GH;
use super::psyche::{stress_color, stress_face, stress_rank};
use crate::game::{CitSort, Game, Selected};
use rf_sim::StressLevel;
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
        Selected::Building(id) => {
            if g.sim.world.building(id).is_none() {
                g.selected = Selected::None;
                return;
            }
            panel(&mut g.ui, r);
            super::building::building_sheet(g, id, r);
        }
    }
}

fn close_button(g: &mut Game, r: Rect) -> bool {
    let s = g.ui.scale;
    let b = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    text_button(&mut g.ui, &g.font, b, "x", BtnState::Normal)
}

/// [feel-ui] Screen size of the sheet's portrait: DF's 96 px tiles at a whole-number scale
/// where possible (crisp pixel art), never below 72 px.
pub fn portrait_px(s: f32) -> f32 {
    let k = if s >= 1.0 { s.floor() } else { s };
    (96.0 * k).max(72.0)
}

/// [feel-ui] The unit's portrait in `pr`: DF's portrait background, the layered portrait
/// (`Gfx::portrait`), the frame. Creatures without portrait graphics get their (big) map
/// sprite, centered, and without any sprite the glyph.
pub fn draw_portrait(g: &mut Game, u: &Unit, pr: Rect) {
    let mut v: Vec<rf_gfx::Sprite> = Vec::new();
    let have = g.gfx.portrait(&g.sim.world, u, &mut v);
    match g.gfx.named("DEFAULT_PORTRAIT_BACKGROUND") {
        Some(b) => {
            g.rend.draw_ui_sprite(&g.gfx, &b, pr);
        }
        None => draw_rectangle(pr.x, pr.y, pr.w, pr.h, Color::from_rgba(52, 40, 58, 255)),
    }
    let mut drawn = false;
    if have {
        for sp in &v {
            drawn |= g.rend.draw_ui_sprite(&g.gfx, sp, pr);
        }
    }
    if !drawn {
        v.clear();
        g.gfx.unit(&g.sim.world, u, &mut v);
        // Bounding box in map tiles (large sprites are bottom-aligned and centered).
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for sp in &v {
            let sx = sp.dx as f32 - (sp.tw as f32 - 1.0) / 2.0;
            let sy = sp.dy as f32;
            x0 = x0.min(sx);
            y0 = y0.min(sy);
            x1 = x1.max(sx + sp.tw as f32);
            y1 = y1.max(sy + sp.th as f32);
        }
        if !v.is_empty() {
            let span = (x1 - x0).max(y1 - y0).max(1.0);
            // A 1-tile creature fills 3/4 of the box at a whole multiple of 32 px.
            let fit = pr.w * 0.8 / span;
            let tp = if fit >= 32.0 { (fit / 32.0).floor() * 32.0 } else { fit };
            let ox = pr.x + (pr.w - (x1 - x0) * tp) / 2.0 - x0 * tp;
            let oy = pr.y + (pr.h - (y1 - y0) * tp) / 2.0 - y0 * tp;
            for sp in &v {
                let sx = sp.dx as f32 - (sp.tw as f32 - 1.0) / 2.0;
                let rr = Rect::new(ox + sx * tp, oy + sp.dy as f32 * tp, sp.tw as f32 * tp, sp.th as f32 * tp);
                drawn |= g.rend.draw_ui_sprite(&g.gfx, sp, rr);
            }
        }
    }
    if !drawn {
        let code = if u.race == "DWARF" { 1 } else { u.race.bytes().next().unwrap_or(b'?') };
        let gw = pr.w * 0.6;
        g.font.glyph(code, pr.x + (pr.w - gw) / 2.0, pr.y + pr.h * 0.1, gw, pr.h * 0.8, col::TEXT_HI);
    }
    match g.gfx.named("DEFAULT_PORTRAIT_FRAME") {
        Some(f) => {
            g.rend.draw_ui_sprite(&g.gfx, &f, pr);
        }
        None => draw_rectangle_lines(pr.x, pr.y, pr.w, pr.h, 1.0, col::BORDER),
    }
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
    // [feel-ui] DF Premium portrait + name like the Steam sheet header.
    let ps = portrait_px(s);
    let pr = Rect::new(x, y, ps, ps);
    draw_portrait(g, &u, pr);
    let tx = x + pr.w + 10.0 * s;
    let tmax = ((r.x + r.w - pad - tx) / (8.0 * s)) as usize;
    let ty = y + 4.0 * s;
    g.font.draw(&clip(&unit_name(&u), tmax.saturating_sub(3)), tx, ty, s, col::TEXT_HI);
    g.font.draw(&clip(&unit_profession(&u), tmax), tx, ty + lh, s, col::TEXT);
    let act = if status.activity.is_empty() { "No Job".to_string() } else { status.activity.clone() };
    g.font.draw(&clip(&act, tmax), tx, ty + 2.0 * lh, s, col::TEXT_CYAN);
    // The stress face and level under the job, like the Steam header.
    let lvl = g.sim.stress_level(u.id);
    let sc = stress_color(lvl);
    g.font.draw(stress_face(lvl), tx, ty + 3.0 * lh, s, sc);
    g.font.draw(&clip(lvl.label(), tmax.saturating_sub(2)), tx + 16.0 * s, ty + 3.0 * lh, s, sc);
    y += pr.h.max(4.0 * lh + 4.0 * s) + 8.0 * s;
    // Tabs in two rows like the Steam sheet. Indices: 0 Overview, 1 Skills, 2 Labor, 3 Items,
    // 4 Thoughts, 5 Needs, 6 Personality, 7 Health, 8 Relations, 9 Mood (`--ui` names them).
    const TAB_ROWS: [&[(usize, &str)]; 2] = [
        &[(0, "Overview"), (4, "Thoughts"), (5, "Needs"), (6, "Personality"), (9, "Mood")],
        &[(7, "Health"), (8, "Relations"), (1, "Skills"), (2, "Labor"), (3, "Items")],
    ];
    let th = GH * s + 8.0 * s;
    let in_mood = !g.sim.mood_info(u.id).mood.is_empty() || !u.mood.is_empty(); // [round 5]
    for row in TAB_ROWS {
        // [round 5] Widths follow the labels, so "Personality" fits next to "Mood".
        let n = row.len() as f32;
        let room = r.w - 2.0 * pad - (n - 1.0) * 4.0 * s;
        let chars: f32 = row.iter().map(|(_, t)| t.chars().count() as f32 + 2.0).sum();
        let mut tx = x;
        for (i, t) in row.iter() {
            let tw = (room * (t.chars().count() as f32 + 2.0) / chars).floor();
            let tr = Rect::new(tx, y, tw, th);
            let st = if g.unit_tab == *i {
                BtnState::Active
            } else if *i == 9 && !in_mood {
                BtnState::Disabled
            } else {
                BtnState::Normal
            };
            if text_button(&mut g.ui, &g.font, tr, t, st) && g.unit_tab != *i {
                g.unit_tab = *i;
                g.unit_scroll = 0;
            }
            tx += tw + 4.0 * s;
        }
        y += th + 4.0 * s;
    }
    y += 8.0 * s;
    let bw = r.w - 2.0 * pad;
    // [round 3] Mind and body tabs (scrollable rows).
    if g.unit_tab == 9 {
        // [round 5] Strange mood (Sim::mood_info).
        let area = Rect::new(x, y, bw, r.y + r.h - pad - lh - y);
        super::round5::draw_mood_tab(g, &u, area);
        return;
    }
    if (4..=8).contains(&g.unit_tab) {
        let rows = match g.unit_tab {
            4 => super::psyche::thoughts_rows(g, &u),
            5 => super::psyche::needs_rows(g, &u),
            6 => super::psyche::personality_rows(g, &u),
            7 => {
                // [round 5] Treatment right after the status block, like DF's health screen.
                let mut rows = super::psyche::health_rows(g, &u);
                let at = rows.iter().position(|r| matches!(r, super::psyche::Row::Gap)).map(|i| i + 1).unwrap_or(rows.len());
                let tr = super::round5::treatment_rows(g, &u);
                rows.splice(at..at, tr);
                rows
            }
            _ => super::psyche::relations_rows(g, &u),
        };
        let area = Rect::new(x, y, bw, r.y + r.h - pad - lh - y);
        super::psyche::draw_rows(g, rows, area);
        return;
    }
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
            y += lh + 4.0 * s;
            // [round 3] DF stress category with its face and color (Sim::stress_level).
            let lvl = g.sim.stress_level(u.id);
            let sc = super::psyche::stress_color(lvl);
            g.font.draw("Stress", x, y, s, col::TEXT);
            let sx = x + 12.0 * 8.0 * s;
            g.font.draw(super::psyche::stress_face(lvl), sx, y, s, sc);
            g.font.draw(lvl.label(), sx + 16.0 * s, y, s, sc);
            let sv = format!("{}", u.stress);
            g.font.draw(&sv, x + bw - g.font.width(&sv, s), y, s, col::TEXT_DIM);
            y += lh;
            // Latest thought, like the summary line of DF's overview.
            if let Some(e) = u.emotions.iter().filter(|e| super::psyche::shown_emotion(e)).max_by_key(|e| (e.year, e.year_tick)) {
                let t = g.sim.describe_emotion(e);
                let t = format!("{} {t}.", super::psyche::pronoun(&u));
                for l in super::psyche::wrap(&t, maxc, 2).into_iter().take(2) {
                    g.font.draw(&l, x, y, s, super::psyche::emotion_color(&e.emotion, e.strength));
                    y += lh;
                }
            }
            let open = u.wounds.iter().filter(|w| super::psyche::is_active_wound(w)).count();
            if open > 0 {
                let bleeding = super::psyche::is_bleeding(&u);
                let t = format!("Wounded ({open} wound{}{})", if open == 1 { "" } else { "s" }, if bleeding { ", bleeding" } else { "" });
                g.font.draw(&t, x, y, s, col::TEXT_BAD);
                y += lh;
            }
            y += 6.0 * s;
            let squad = if u.squad_id >= 0 {
                let name = super::military::squad_name(&g.sim.world, u.squad_id);
                if u.squad_position >= 0 { format!("Squad: {name}, position {}", u.squad_position + 1) } else { format!("Squad: {name}") }
            } else {
                String::new()
            };
            let sex = match u.sex {
                0 => "female",
                1 => "male",
                _ => "",
            };
            // [round 5] Strange mood with the claimed workshop and the missing materials.
            let (mood, mood_c) = super::round5::mood_line(g, &u);
            if !mood.is_empty() {
                for l in super::psyche::wrap(&mood, maxc, 2).into_iter().take(2) {
                    g.font.draw(&l, x, y, s, mood_c);
                    y += lh;
                }
            }
            if u.flags.caged {
                g.font.draw("In a cage.", x, y, s, col::TEXT_HI);
                y += lh;
            }
            let lines = [
                format!("{} {sex}, age {:.0}", pretty(&u.race), u.age),
                format!("Position {}, {}, z {}", u.pos.x, u.pos.y, u.pos.z),
                squad,
                format!("Unit id {}{}", u.id, if u.hist_fig_id >= 0 { format!(", hist. figure {}", u.hist_fig_id) } else { String::new() }),
            ];
            for l in lines.iter().filter(|l| !l.is_empty()) {
                if y > r.y + r.h - lh - pad {
                    break;
                }
                g.font.draw(&clip(l, maxc), x, y, s, col::TEXT);
                y += lh;
            }
            y += 6.0 * s;
            let bw2 = 18.0 * 8.0 * s;
            if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw2, GH * s + 8.0 * s), "Center camera", BtnState::Normal) {
                g.cam.focus(u.pos);
            }
            let following = g.follow == Some(u.id);
            let fst = if following { BtnState::Active } else { BtnState::Normal };
            if text_button(&mut g.ui, &g.font, Rect::new(x + bw2 + 8.0 * s, y, bw2, GH * s + 8.0 * s), "Follow", fst) {
                g.follow = if following { None } else { Some(u.id) };
            }
            y += GH * s + 16.0 * s;
            // Top skills preview.
            let mut sk = u.skills.clone();
            sk.sort_by(|a, b| b.rating.cmp(&a.rating).then(b.xp.cmp(&a.xp)));
            if !sk.is_empty() && y < r.y + r.h - 2.0 * lh - pad {
                g.font.draw("Top skills", x, y, s, col::BORDER_HI);
                y += lh;
                for k in sk.iter().take(5) {
                    if y > r.y + r.h - lh - pad {
                        break;
                    }
                    g.font.draw(&clip(&super::round9::skill_title(k), maxc), x, y, s, col::TEXT); // [app9] DF names
                    y += lh;
                }
            }
        }
        1 => {
            // [app9] DF skill names, levels and experience bars (scrolls).
            super::round9::draw_skills(g, &u, x, y, bw, r.y + r.h - pad);
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
                let (txt, known) = match g.sim.world.item(*iid) {
                    Some(it) => (item_label(g, it), true),
                    None => (format!("item #{iid} (unknown)"), false),
                };
                // [round 5] forbid / dump toggles per item, like DF's item sheet.
                g.font.draw(&clip(&txt, maxc.saturating_sub(5)), x, y, s, col::TEXT);
                if known {
                    super::round5::item_row_toggles(g, *iid, x + bw, y);
                }
                y += lh;
            }
        }
    }
}

pub fn item_label(g: &Game, it: &rf_core::Item) -> String {
    // [round 5] DF's own description already carries the quality marks (-x-, +x+, *x*, ≡x≡,
    // ☼x☼); only a generated label gets them added.
    let base = if !it.name.is_empty() {
        it.name.clone()
    } else if !it.desc.is_empty() {
        it.desc.clone()
    } else {
        let m = if it.mat != NO_MAT { g.sim.world.material(it.mat).map(|m| m.name.clone()).unwrap_or_default() } else { String::new() };
        let base = format!("{m} {}", pretty(&it.kind).to_lowercase()).trim().to_string();
        let q = ["", "-", "+", "*", "≡", "☼", "!"][it.quality.min(6) as usize];
        if q.is_empty() { base } else { format!("{q}{base}{q}") }
    };
    // DF's description already ends with the stack size ("[2]").
    if it.stack > 1 && !base.contains(&format!("[{}]", it.stack)) {
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
    // [app10] DF-style look lines: rough/smoothed/constructed, engraving with its quality.
    for (l, c) in super::round10::look_lines(g, p) {
        for part in super::round9::wrap(&l, maxc) {
            g.font.draw(&part, x, y, s, c);
            y += lh;
        }
    }
    y += 4.0 * s;
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
        for pl in w.plants.iter().filter(|pl| pl.pos == p) {
            lines.push((format!("Plant: {}{}", pretty(&pl.token).to_lowercase(), if pl.tree { " tree" } else { "" }), col::TEXT_GOOD));
        }
    }
    for (l, c) in lines {
        g.font.draw(&clip(&l, maxc), x, y, s, c);
        y += lh;
    }
    y += 6.0 * s;
    // Buildings and zones here (clickable: opens the building sheet).
    let blds: Vec<(i32, String)> = g
        .sim
        .world
        .buildings
        .iter()
        .filter(|b| b.contains(p))
        .map(|b| {
            let t = if b.kind == "Civzone" { format!("{} zone", super::words(&b.subtype)) } else { super::building::building_title(g, b) };
            (b.id, format!("{t} (#{})", b.id))
        })
        .collect();
    if !blds.is_empty() {
        g.font.draw("Buildings", x, y, s, col::BORDER_HI);
        y += lh;
        for (id, txt) in blds {
            let rr = Rect::new(x, y - 2.0, r.w - 2.0 * pad, lh);
            if super::list_row(&mut g.ui, rr, false) {
                g.selected = Selected::Building(id);
                g.bui = Default::default();
            }
            g.font.draw(&clip(&txt, maxc), x, y, s, col::TEXT_GOOD);
            y += lh;
        }
        y += 6.0 * s;
    }
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
    let items: Vec<(i32, String, bool)> = g
        .sim
        .world
        .items
        .iter()
        .filter(|it| it.pos == p && it.flags.on_ground)
        .map(|it| (it.id, item_label(g, it), it.flags.forbid))
        .collect();
    if !items.is_empty() {
        g.font.draw(&format!("Items ({})", items.len()), x, y, s, col::BORDER_HI);
        // [round 5] Forbid / claim everything here at once.
        let all_forbidden = items.iter().all(|(_, _, f)| *f);
        let label = if all_forbidden { "Claim all" } else { "Forbid all" };
        let bw2 = (label.len() as f32 + 2.0) * 8.0 * s;
        if text_button(&mut g.ui, &g.font, Rect::new(r.x + r.w - pad - bw2, y - 2.0 * s, bw2, GH * s + 4.0 * s), label, BtnState::Normal) {
            g.sim.set_items_flag_in(p, p, "forbid", !all_forbidden);
        }
        y += lh + 2.0 * s;
        // [app10] Identical items are grouped ("12 granite boulders"); a group's F/D toggles
        // apply to all of its items.
        let groups = super::round10::item_groups(g, p);
        let n = groups.len();
        for (k, (text, ids, forbid)) in groups.into_iter().enumerate() {
            if y > r.y + r.h - 2.0 * lh - pad {
                let more: usize = n - k;
                g.font.draw(&format!("... and {more} more groups"), x, y, s, col::TEXT_DIM);
                break;
            }
            g.font.draw(&clip(&text, maxc.saturating_sub(5)), x, y, s, if forbid { Color::from_rgba(230, 120, 110, 255) } else { col::TEXT });
            if ids.len() == 1 {
                super::round5::item_row_toggles(g, ids[0], r.x + r.w - pad, y);
            } else {
                super::round10::group_toggles(g, &ids, r.x + r.w - pad, y);
            }
            y += lh;
        }
    }
}


/// Creatures list (left side), like the Steam "Creatures" screen with its tabs: Citizens
/// (stress face, name, profession, job, stress category; sortable by name, profession or
/// stress), Pets & livestock, and Others (visitors, wild animals, hostiles).
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
    struct Entry {
        id: i32,
        name: String,
        prof: String,
        pos: Pos,
        lvl: Option<StressLevel>,
        stress: i32,
        tag: (String, Color),
    }
    let world = &g.sim.world;
    let is_pet = |u: &Unit| u.flags.animal && !u.flags.hostile && (u.flags.tame || u.civ_id == world.meta.civ_id);
    let alive = |u: &Unit| !u.flags.dead && !u.flags.inactive;
    let n_cit = world.citizens().count();
    let n_pet = world.units.iter().filter(|u| alive(u) && !u.flags.citizen && is_pet(u)).count();
    let n_oth = world.units.iter().filter(|u| alive(u) && !u.flags.citizen && !is_pet(u)).count();
    let tab = g.creatures_tab.min(2);
    let mut list: Vec<Entry> = match tab {
        0 => world
            .citizens()
            .map(|u| {
                let lvl = g.sim.stress_level(u.id);
                Entry { id: u.id, name: unit_name(u), prof: unit_profession(u), pos: u.pos, lvl: Some(lvl), stress: u.stress, tag: (lvl.label().to_string(), stress_color(lvl)) }
            })
            .collect(),
        _ => world
            .units
            .iter()
            .filter(|u| alive(u) && !u.flags.citizen && (is_pet(u) == (tab == 1)))
            .map(|u| {
                let hostile = g.hostiles.binary_search(&u.id).is_ok();
                let tag = if hostile && u.flags.caged {
                    ("Caged hostile".to_string(), col::TEXT_HI)
                } else if hostile {
                    ("Hostile".to_string(), col::TEXT_BAD)
                } else if u.flags.caged {
                    ("Caged".to_string(), col::TEXT_DIM)
                } else if u.flags.merchant {
                    ("Merchant".to_string(), col::TEXT_CYAN)
                } else if u.flags.visitor {
                    ("Visitor".to_string(), col::TEXT_CYAN)
                } else if tab == 2 && u.flags.animal {
                    ("Wild".to_string(), col::TEXT_DIM)
                } else {
                    (String::new(), col::TEXT_DIM)
                };
                Entry { id: u.id, name: unit_name(u), prof: unit_profession(u), pos: u.pos, lvl: None, stress: 0, tag }
            })
            .collect(),
    };
    let sort = if tab != 0 && g.citizens_sort == CitSort::Stress { CitSort::Name } else { g.citizens_sort };
    match sort {
        CitSort::Name => list.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id))),
        CitSort::Profession => list.sort_by(|a, b| a.prof.cmp(&b.prof).then(a.name.cmp(&b.name)).then(a.id.cmp(&b.id))),
        CitSort::Stress => list.sort_by(|a, b| {
            let (ra, rb) = (a.lvl.map(stress_rank).unwrap_or(0), b.lvl.map(stress_rank).unwrap_or(0));
            rb.cmp(&ra).then(b.stress.cmp(&a.stress)).then(a.name.cmp(&b.name)).then(a.id.cmp(&b.id))
        }),
    }
    // Tabs.
    let mut tx = x;
    let mut new_tab = None;
    for (i, label) in [(0usize, format!("Citizens ({n_cit})")), (1, format!("Pets/livestock ({n_pet})")), (2, format!("Others ({n_oth})"))] {
        let tw = (label.chars().count() as f32 + 2.0) * 8.0 * s;
        let st = if tab == i { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(tx, y - 2.0 * s, tw, GH * s + 8.0 * s), &label, st) {
            new_tab = Some(i);
        }
        tx += tw + 4.0 * s;
    }
    if let Some(t) = new_tab {
        g.creatures_tab = t;
        g.citizens_scroll = 0;
    }
    y += lh + 10.0 * s;
    // Sort buttons.
    g.font.draw("Sort:", x, y + 3.0 * s, s, col::TEXT_DIM);
    let mut bx = x + 6.0 * 8.0 * s;
    let sorts: &[(CitSort, &str)] = if tab == 0 {
        &[(CitSort::Name, "Name"), (CitSort::Profession, "Profession"), (CitSort::Stress, "Stress")]
    } else {
        &[(CitSort::Name, "Name"), (CitSort::Profession, "Kind")]
    };
    for (k, label) in sorts {
        let bw = (label.len() as f32 + 2.0) * 8.0 * s;
        let st = if sort == *k { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y, bw, GH * s + 6.0 * s), label, st) {
            g.citizens_sort = *k;
            g.citizens_scroll = 0;
        }
        bx += bw + 4.0 * s;
    }
    let flagged = if tab == 0 {
        list.iter().filter(|c| c.lvl.map(stress_rank).unwrap_or(0) >= 4).count()
    } else {
        list.iter().filter(|c| c.tag.0 == "Hostile").count()
    };
    if tab == 1 {
        // [app8] The full pets and livestock screen (slaughter, pastures, training).
        let ar = Rect::new(r.x + r.w - pad - 22.0 * 8.0 * s, y, 22.0 * 8.0 * s, GH * s + 6.0 * s);
        if text_button(&mut g.ui, &g.font, ar, "Animals: manage...", BtnState::Normal) {
            g.toggle_screen(crate::game::Screen::Animals);
        }
    } else if flagged > 0 {
        let (t, c) = if tab == 0 { (format!("{flagged} stressed"), col::TEXT_HI) } else { (format!("{flagged} hostile"), col::TEXT_BAD) };
        g.font.draw(&t, r.x + r.w - pad - g.font.width(&t, s), y + 3.0 * s, s, c);
    }
    y += GH * s + 12.0 * s;
    // [round 4] Population header like DF's creatures screen (cap, migrants, caravan).
    if tab == 0 {
        let p = g.sim.population_info();
        let cap = if p.population_cap > 0 { format!("cap {}", p.population_cap) } else { "no cap".into() };
        let wave = p.next_wave_in.map(|t| format!("migrants in {}", super::round4::ticks_span(t))).unwrap_or_else(|| "no migrants expected".into());
        let car = p.caravan_expected.clone().map(|c| format!("caravan: {c}")).unwrap_or_default();
        let maxc = ((r.w - 2.0 * pad) / (8.0 * s)) as usize;
        // [round 5] Only whole parts (the tooltip has everything), no half-cut "ca.".
        let mut t = format!("{} citizens, {} children, {cap}", p.citizens, p.children);
        for part in [wave, car] {
            if !part.is_empty() && t.chars().count() + 2 + part.chars().count() <= maxc {
                t.push_str(&format!("; {part}"));
            }
        }
        let hr = Rect::new(x, y - 2.0 * s, r.w - 2.0 * pad, lh);
        g.font.draw(&clip(&t, maxc), x, y, s, col::TEXT_DIM);
        if g.ui.hovered(hr) {
            let lines = super::round4::population_lines(g);
            g.ui.tooltip = Some((lines.join("\n"), vec2(hr.x, hr.y - 4.0)));
        }
        y += lh + 4.0 * s;
    }
    let rows = (((r.y + r.h - pad) - y) / (lh * 2.0)).floor().max(1.0) as usize;
    // Scroll with the wheel while hovering the list.
    if g.ui.hovered(r) && g.input.wheel != 0 {
        if g.input.wheel < 0 {
            g.citizens_scroll += 3;
        } else {
            g.citizens_scroll = g.citizens_scroll.saturating_sub(3);
        }
    }
    g.citizens_scroll = g.citizens_scroll.min(list.len().saturating_sub(rows));
    let maxc = ((r.w - 2.0 * pad) / (8.0 * s)) as usize;
    if list.is_empty() {
        g.font.draw("Nobody.", x, y, s, col::TEXT_DIM);
    }
    let mut pick = None;
    for e in list.iter().skip(g.citizens_scroll).take(rows) {
        let rr = Rect::new(r.x + 4.0 * s, y - 2.0 * s, r.w - 8.0 * s, lh * 2.0 - 2.0 * s);
        let sel = g.selected == Selected::Unit(e.id);
        if sel {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, col::BUTTON_ACTIVE);
        } else if g.ui.hovered(rr) {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, col::BUTTON_HOVER);
        }
        let act = g.sim.unit_status(e.id).activity;
        match e.lvl {
            Some(lvl) => {
                g.font.draw(stress_face(lvl), x, y, s, stress_color(lvl));
            }
            None => {
                g.font.draw(if e.tag.0.contains("ostile") { "!" } else { "·" }, x + 2.0 * s, y, s, e.tag.1);
            }
        }
        g.font.draw(&clip(&e.name, 30), x + 16.0 * s, y, s, col::TEXT);
        g.font.draw(&clip(&e.prof, maxc.saturating_sub(34)), x + 34.0 * 8.0 * s, y, s, col::TEXT_DIM);
        let act = if e.tag.0.starts_with("Caged") {
            "In a cage".to_string()
        } else if act.is_empty() {
            "No Job".to_string()
        } else {
            act
        };
        let lt = &e.tag.0;
        let lw = g.font.width(lt, s);
        g.font.draw(&clip(&act, maxc.saturating_sub(lt.len() + 4)), x + 16.0 * s, y + lh - 2.0 * s, s, col::TEXT_CYAN);
        g.font.draw(lt, r.x + r.w - pad - lw, y + lh - 2.0 * s, s, e.tag.1);
        if g.ui.clicked(rr) {
            pick = Some((e.id, e.pos));
        }
        y += lh * 2.0;
    }
    if let Some((id, p)) = pick {
        g.select_unit(id);
        g.cam.focus(p);
    }
    if list.len() > rows {
        let t = format!("{}-{} of {} (wheel scrolls)", g.citizens_scroll + 1, (g.citizens_scroll + rows).min(list.len()), list.len());
        g.font.draw(&t, x, r.y + r.h - pad - GH * s + 4.0 * s, s, col::TEXT_DIM);
    }
}
