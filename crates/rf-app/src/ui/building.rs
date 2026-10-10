//! Building sheet (right side, like the Steam version's building window): workshop task
//! queue with "Add job", farm plot crops per season, stockpile categories, zone info and
//! "Remove".

use super::build::option_icon;
use super::{checkbox, clip, col, draw_icon, list_row, pretty, text_button, wheel_over, words, BtnState};
use crate::font::GH;
use crate::game::{Game, Selected, ALL_STOCK_CATEGORIES};
use macroquad::prelude::*;
use rf_core::{Building, BuildingJob, NO_MAT};

pub const SEASONS: [&str; 4] = ["Spring", "Summer", "Autumn", "Winter"];

/// Display name of a building like DF shows it (`Still`, `Carpenter's workshop`, `Bedroom`).
pub fn building_title(g: &Game, b: &Building) -> String {
    if !b.name.is_empty() {
        return b.name.clone();
    }
    if let Some(o) = g.build_opts.iter().find(|o| o.kind == b.kind && o.subtype == b.subtype) {
        return o.label.to_string();
    }
    match b.kind.as_str() {
        "Civzone" => words(&b.subtype),
        "Stockpile" => format!("Stockpile #{}", b.id),
        "Workshop" | "Furnace" if !b.subtype.is_empty() => words(&b.subtype),
        "FarmPlot" => "Farm plot".into(),
        "Weaponrack" => "Weapon rack".into(),
        "Armorstand" => "Armor stand".into(),
        "Box" => "Chest".into(),
        "Trap" => super::devices::trap_name(&b.subtype), // [round 4]
        "TradeDepot" => "Trade depot".into(),
        k => words(k),
    }
}

/// What a job or work order makes, in DF's words.
pub struct JobDesc<'a> {
    pub job: &'a str,
    pub reaction: &'a str,
    pub item_type: &'a str,
    pub item_subtype: &'a str,
    /// Material token (`INORGANIC:IRON`) or class (`INORGANIC`) or "".
    pub mat: &'a str,
    pub mat_category: &'a [String],
}

impl<'a> JobDesc<'a> {
    pub fn of_job(j: &'a BuildingJob) -> Self {
        JobDesc { job: &j.job, reaction: &j.reaction, item_type: &j.item_type, item_subtype: &j.item_subtype, mat: &j.mat, mat_category: &j.mat_category }
    }
    pub fn of_order(o: &'a rf_core::WorkOrder) -> Self {
        JobDesc { job: &o.job, reaction: &o.reaction, item_type: &o.item_type, item_subtype: &o.item_subtype, mat: &o.mat, mat_category: &o.mat_category }
    }
}

/// Label of a job/order like DF's task list: the sim's order template label if one matches,
/// else e.g. `Make weapon (pick, iron)`, `Brew drink from plant growth`.
pub fn job_label(templates: &[rf_sim::OrderTemplate], d: &JobDesc) -> String {
    let m = templates.iter().find(|t| {
        t.job == d.job
            && (d.reaction.is_empty() || t.reaction == d.reaction)
            && (d.item_type.is_empty() || t.item_type == d.item_type)
            && (d.mat_category.is_empty() || d.mat_category.iter().any(|c| c == t.mat_category))
    });
    let base = match m {
        Some(t) => t.label.to_string(),
        None if !d.reaction.is_empty() => pretty(d.reaction),
        None => words(d.job),
    };
    let mut extra: Vec<String> = Vec::new();
    if !d.item_subtype.is_empty() {
        // ITEM_WEAPON_PICK -> pick, ITEM_TOOL_LARGE_POT -> large pot.
        let t = d.item_subtype.splitn(3, '_').nth(2).unwrap_or(d.item_subtype);
        extra.push(t.replace('_', " ").to_lowercase());
    }
    if let Some((_, m)) = d.mat.split_once(':') {
        extra.push(m.replace('_', " ").to_lowercase());
    } else if m.is_none() {
        extra.extend(d.mat_category.iter().take(1).cloned());
    }
    if extra.is_empty() {
        base
    } else {
        format!("{base} ({})", extra.join(", "))
    }
}

fn job_text(templates: &[rf_sim::OrderTemplate], j: &BuildingJob) -> String {
    job_label(templates, &JobDesc::of_job(j))
}

fn title_of(g: &Game, b: &Building) -> String {
    building_title(g, b)
}

/// Plant display name from the save's plant raws.
pub fn plant_name(g: &Game, token: &str) -> String {
    g.sim.world.plant_raws.iter().find(|p| p.token == token).map(|p| p.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| pretty(token).to_lowercase())
}

enum Act {
    None,
    Close,
    Remove,
    Center,
    RemoveJob(usize),
    ToggleRepeat(usize),
    ToggleSuspend(usize),
    AddJob(usize),
    SetCrop(usize, String),
    ToggleCat(&'static str),
}

pub fn building_sheet(g: &mut Game, id: i32, r: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let pad = 10.0 * s;
    let maxc = ((r.w - 2.0 * pad) / (8.0 * s)) as usize;
    let Some(b) = g.sim.world.building(id).cloned() else {
        g.selected = Selected::None;
        return;
    };
    let mut act = Act::None;
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        act = Act::Close;
    }
    let (x, mut y) = (r.x + pad, r.y + pad);
    // Header: icon + name + kind/material + state.
    let ir = Rect::new(x, y, 36.0 * s, 36.0 * s);
    draw_rectangle(ir.x, ir.y, ir.w, ir.h, Color::from_rgba(30, 28, 26, 255));
    draw_rectangle_lines(ir.x, ir.y, ir.w, ir.h, 1.0, col::BORDER);
    let icon = if b.kind == "Civzone" { super::build::zone_icon(&b.subtype).to_string() } else { option_icon(&b.kind, &b.subtype) };
    let inner = Rect::new(ir.x + 2.0 * s, ir.y + 2.0 * s, 32.0 * s, 32.0 * s);
    if !draw_icon(&g.gfx, &mut g.rend, &icon, inner, WHITE) {
        super::draw_fallback_icon(&g.font, super::Ico::Hammer, inner.x, inner.y, inner.w, inner.h);
    }
    let tx = x + ir.w + 8.0 * s;
    let tmax = ((r.x + r.w - pad - tx) / (8.0 * s)) as usize;
    g.font.draw(&clip(&title_of(g, &b), tmax.saturating_sub(3)), tx, y, s, col::TEXT_HI);
    let has_mat = b.mat != NO_MAT && !matches!(b.kind.as_str(), "Civzone" | "Stockpile" | "FarmPlot");
    let mat = if has_mat { g.sim.world.material(b.mat).map(|m| m.name.clone()).unwrap_or_default() } else { String::new() };
    let what = match b.kind.as_str() {
        "Civzone" => "Zone".to_string(),
        "Trap" => super::devices::trap_name(&b.subtype), // [round 4]
        k => words(k),
    };
    let title = building_title(g, &b);
    let sub = if mat.is_empty() { what } else { format!("{mat} {}", what.to_lowercase()) };
    let sub = if sub.eq_ignore_ascii_case(&title) { format!("{}x{} tiles", b.x2 - b.x1 + 1, b.y2 - b.y1 + 1) } else { sub };
    g.font.draw(&clip(&sub, tmax), tx, y + lh, s, col::TEXT);
    let (state, sc) = if b.kind == "Civzone" || b.kind == "Stockpile" {
        (format!("{}x{} tiles, z {}", b.x2 - b.x1 + 1, b.y2 - b.y1 + 1, b.z), col::TEXT_DIM)
    } else if b.built {
        ("Built".to_string(), col::TEXT_GOOD)
    } else {
        (format!("Waiting for construction (stage {}/3)", b.stage.clamp(0, 3)), col::TEXT_HI)
    };
    g.font.draw(&clip(&state, tmax), tx, y + 2.0 * lh, s, sc);
    y += 3.0 * lh + 8.0 * s;
    let bw = r.w - 2.0 * pad;
    let bottom = r.y + r.h - pad - GH * s - 10.0 * s - 6.0 * s;
    let templates = g.sim.order_templates();
    match b.kind.as_str() {
        "Workshop" | "Furnace" => {
            g.font.draw(&format!("Tasks ({})", b.jobs.len()), x, y, s, col::BORDER_HI);
            y += lh;
            if b.jobs.is_empty() {
                g.font.draw("No tasks. Add one, or use work orders (o).", x, y, s, col::TEXT_DIM);
                y += lh;
            }
            for (k, j) in b.jobs.iter().enumerate() {
                if y > bottom - 3.0 * lh {
                    g.font.draw(&format!("... {} more", b.jobs.len() - k), x, y, s, col::TEXT_DIM);
                    y += lh;
                    break;
                }
                let rh = lh + 4.0 * s;
                let rr = Rect::new(x, y - 2.0 * s, bw, rh);
                draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, if k % 2 == 0 { 0.04 } else { 0.0 }));
                let mut label = job_text(&templates, j);
                if j.order_id >= 0 {
                    label.push_str(" (order)");
                }
                let lc = if j.suspended { col::TEXT_DIM } else { col::TEXT };
                g.font.draw(&clip(&label, maxc.saturating_sub(14)), x + 4.0 * s, y, s, lc);
                // Buttons on the right: repeat, suspend, remove.
                let bwid = 24.0 * s;
                let bh = lh;
                let bx = rr.x + rr.w - 3.0 * (bwid + 3.0 * s);
                let rst = if j.repeat { BtnState::Active } else { BtnState::Normal };
                if text_button(&mut g.ui, &g.font, Rect::new(bx, y - 1.0 * s, bwid, bh), "R", rst) {
                    act = Act::ToggleRepeat(k);
                }
                let sst = if j.suspended { BtnState::Active } else { BtnState::Normal };
                if text_button(&mut g.ui, &g.font, Rect::new(bx + bwid + 3.0 * s, y - 1.0 * s, bwid, bh), "S", sst) {
                    act = Act::ToggleSuspend(k);
                }
                if text_button(&mut g.ui, &g.font, Rect::new(bx + 2.0 * (bwid + 3.0 * s), y - 1.0 * s, bwid, bh), "x", BtnState::Normal) {
                    act = Act::RemoveJob(k);
                }
                for (bxx, tip) in [(bx, "Repeat (R)"), (bx + bwid + 3.0 * s, "Suspend (S)"), (bx + 2.0 * (bwid + 3.0 * s), "Cancel task")] {
                    if g.ui.hovered(Rect::new(bxx, y, bwid, bh)) {
                        g.ui.tooltip = Some((tip.to_string(), vec2(bxx, y - 4.0)));
                    }
                }
                y += rh;
            }
            y += 6.0 * s;
            let ab = Rect::new(x, y, 16.0 * 8.0 * s, GH * s + 8.0 * s);
            let ast = if g.bui.add_open { BtnState::Active } else { BtnState::Normal };
            if text_button(&mut g.ui, &g.font, ab, "+ Add job", ast) {
                g.bui.add_open = !g.bui.add_open;
            }
            let rep = g.bui.repeat;
            if checkbox(&mut g.ui, &g.font, ab.x + ab.w + 12.0 * s, y + 4.0 * s, rep, "Repeat") {
                g.bui.repeat = !rep;
            }
            y += ab.h + 6.0 * s;
            if g.bui.add_open {
                let list = g.sim.templates_for_workshop(&b.subtype);
                g.font.draw("Choose a task (click to queue it):", x, y, s, col::BORDER_HI);
                y += lh;
                if list.is_empty() {
                    g.font.draw("No jobs for this workshop yet.", x, y, s, col::TEXT_DIM);
                }
                let area = Rect::new(x, y, bw, (bottom - y).max(lh));
                draw_rectangle(area.x, area.y, area.w, area.h, Color::new(0.0, 0.0, 0.0, 0.35));
                draw_rectangle_lines(area.x, area.y, area.w, area.h, 1.0, col::BORDER);
                let rows = ((area.h / lh) as usize).max(1);
                let step = wheel_over(&g.ui, &mut g.input, area);
                g.bui.scroll = (g.bui.scroll as i32 + step * 3).clamp(0, list.len().saturating_sub(rows) as i32) as usize;
                for (k, &ti) in list.iter().enumerate().skip(g.bui.scroll).take(rows) {
                    let rr = Rect::new(x, y, bw, lh);
                    if list_row(&mut g.ui, rr, false) {
                        act = Act::AddJob(ti);
                    }
                    let t = &templates[ti];
                    g.font.draw(&clip(&format!("{}", t.label), maxc - 1), x + 4.0 * s, y + 1.5 * s, s, col::TEXT);
                    let _ = k;
                    y += lh;
                }
            }
        }
        "FarmPlot" => {
            g.font.draw("Crops by season", x, y, s, col::BORDER_HI);
            y += lh + 2.0 * s;
            let cur_season = (g.sim.world.calendar.month() / 3).clamp(0, 3) as usize;
            for (si, sn) in SEASONS.iter().enumerate() {
                let crop = b.crops.get(si).cloned().unwrap_or_default();
                let name = if crop.is_empty() { "Fallow".to_string() } else { plant_name(g, &crop) };
                let lc = if si == cur_season { col::TEXT_HI } else { col::TEXT };
                g.font.draw(sn, x, y + 4.0 * s, s, lc);
                let br = Rect::new(x + 9.0 * 8.0 * s, y, bw - 9.0 * 8.0 * s, GH * s + 8.0 * s);
                let st = if g.bui.crop_season == Some(si) { BtnState::Active } else { BtnState::Normal };
                if text_button(&mut g.ui, &g.font, br, &clip(&name, ((br.w / (8.0 * s)) as usize).saturating_sub(2)), st) {
                    g.bui.crop_season = if g.bui.crop_season == Some(si) { None } else { Some(si) };
                    g.bui.scroll = 0;
                }
                y += br.h + 4.0 * s;
            }
            if let Some(si) = g.bui.crop_season {
                y += 4.0 * s;
                let opts = g.sim.crop_options(id);
                let raws = &g.sim.world.plant_raws;
                let mut list: Vec<(String, String, bool)> = vec![(String::new(), "Fallow (nothing)".into(), true)];
                for t in opts {
                    let pr = raws.iter().find(|p| p.token == t);
                    let ok = pr.map(|p| [p.spring, p.summer, p.autumn, p.winter][si]).unwrap_or(true);
                    let mut n = pr.map(|p| p.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| pretty(&t).to_lowercase());
                    if let Some(p) = pr {
                        let mut tags = Vec::new();
                        if p.brewable {
                            tags.push("brew");
                        }
                        if p.edible_raw || p.edible_cooked {
                            tags.push("food");
                        }
                        if !tags.is_empty() {
                            n.push_str(&format!(" ({})", tags.join(", ")));
                        }
                    }
                    list.push((t, n, ok));
                }
                // Plantable this season first, like DF's list.
                list[1..].sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(&b.1)));
                g.font.draw(&format!("{} crop:", SEASONS[si]), x, y, s, col::BORDER_HI);
                y += lh;
                if list.len() == 1 {
                    g.font.draw("No plant raws in this export (round-2 exporter).", x, y, s, col::TEXT_DIM);
                    y += lh;
                }
                let area = Rect::new(x, y, bw, (bottom - y).max(lh));
                draw_rectangle(area.x, area.y, area.w, area.h, Color::new(0.0, 0.0, 0.0, 0.35));
                draw_rectangle_lines(area.x, area.y, area.w, area.h, 1.0, col::BORDER);
                let rows = ((area.h / lh) as usize).max(1);
                let step = wheel_over(&g.ui, &mut g.input, area);
                g.bui.scroll = (g.bui.scroll as i32 + step * 3).clamp(0, list.len().saturating_sub(rows) as i32) as usize;
                let cur = b.crops.get(si).cloned().unwrap_or_default();
                for (tok, name, ok) in list.iter().skip(g.bui.scroll).take(rows) {
                    let rr = Rect::new(x, y, bw, lh);
                    if list_row(&mut g.ui, rr, *tok == cur) {
                        act = Act::SetCrop(si, tok.clone());
                    }
                    let c = if *ok { col::TEXT } else { col::TEXT_DIM };
                    let suffix = if *ok { "" } else { "  (not this season)" };
                    g.font.draw(&clip(&format!("{name}{suffix}"), maxc - 1), x + 4.0 * s, y + 1.5 * s, s, c);
                    y += lh;
                }
            }
        }
        "Stockpile" => {
            g.font.draw("Accepts", x, y, s, col::BORDER_HI);
            y += lh + 2.0 * s;
            let cats = b.stockpile.as_ref().map(|sp| sp.categories.clone()).unwrap_or_default();
            let colw = (bw / 2.0).floor();
            for (k, c) in ALL_STOCK_CATEGORIES.iter().enumerate() {
                let cx = x + (k % 2) as f32 * colw;
                let cy = y + (k / 2) as f32 * (lh + 2.0 * s);
                if cy > bottom - lh {
                    break;
                }
                let on = cats.iter().any(|x| x == c);
                if checkbox(&mut g.ui, &g.font, cx, cy, on, &pretty(c)) {
                    act = Act::ToggleCat(c);
                }
            }
            y += ALL_STOCK_CATEGORIES.len().div_ceil(2) as f32 * (lh + 2.0 * s);
            let n = g.sim.world.items.iter().filter(|it| b.contains(it.pos) && it.flags.on_ground).count();
            g.font.draw(&format!("{n} items stored"), x, y + 4.0 * s, s, col::TEXT_DIM);
        }
        _ => {
            // [round 4] Doors, levers, bridges, traps; the trade depot.
            if super::devices::is_device(&b) {
                y = super::devices::sheet_section(g, &b, x, y, bw, bottom) + 4.0 * s;
            }
            if b.kind == "TradeDepot" && b.built {
                let bh = GH * s + 8.0 * s;
                let caravan = g.r4.trade.is_some();
                let t = if caravan { "A caravan is at the depot." } else { "No caravan at the depot." };
                g.font.draw(t, x, y, s, if caravan { col::TEXT_GOOD } else { col::TEXT_DIM });
                y += lh + 2.0 * s;
                let half = (bw - 6.0 * s) / 2.0;
                if text_button(&mut g.ui, &g.font, Rect::new(x, y, half, bh), "Trade", if caravan { BtnState::Normal } else { BtnState::Disabled }) {
                    super::round4::open_trade(g);
                }
                if text_button(&mut g.ui, &g.font, Rect::new(x + half + 6.0 * s, y, half, bh), "Move goods to depot", BtnState::Normal) {
                    super::round4::open_trade(g);
                    g.r4.depot_open = true;
                }
                y += bh + 8.0 * s;
            }
            let mut lines: Vec<String> = Vec::new();
            if b.owner >= 0 {
                let o = g.sim.world.unit(b.owner).map(super::panels::unit_name).unwrap_or_else(|| format!("unit #{}", b.owner));
                lines.push(format!("Assigned to {o}"));
            }
            if b.kind == "Civzone" {
                let furn = g.sim.world.buildings.iter().filter(|o| o.kind != "Civzone" && o.kind != "Stockpile" && o.z == b.z && b.contains(o.center)).count();
                lines.push(format!("{furn} pieces of furniture inside"));
            }
            if !b.items.is_empty() {
                let n = b.items.len();
                lines.push(format!("{n} item{} inside", if n == 1 { "" } else { "s" }));
            }
            for l in lines {
                g.font.draw(&clip(&l, maxc), x, y, s, col::TEXT);
                y += lh;
            }
        }
    }
    // Bottom row: center + remove.
    let by = r.y + r.h - pad - GH * s - 8.0 * s;
    let bw2 = 16.0 * 8.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(x, by, bw2, GH * s + 8.0 * s), "Center camera", BtnState::Normal) {
        act = Act::Center;
    }
    let rl = match b.kind.as_str() {
        "Civzone" => "Remove zone",
        "Stockpile" => "Remove stockpile",
        _ if !b.built => "Cancel construction",
        _ => "Remove building",
    };
    let rw = (rl.len() as f32 + 3.0) * 8.0 * s;
    let rr = Rect::new(r.x + r.w - pad - rw, by, rw, GH * s + 8.0 * s);
    if g.ui.hovered(rr) {
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.45, 0.12, 0.10, 1.0));
    }
    if text_button(&mut g.ui, &g.font, rr, rl, BtnState::Normal) {
        act = Act::Remove;
    }
    apply(g, &b, act);
}

fn apply(g: &mut Game, b: &Building, act: Act) {
    let id = b.id;
    fn bm(g: &mut Game, id: i32) -> Option<&mut Building> {
        g.sim.world.buildings.iter_mut().find(|x| x.id == id)
    }
    match act {
        Act::None => {}
        Act::Close => g.selected = Selected::None,
        Act::Center => g.cam.focus(b.center),
        Act::Remove => {
            g.sim.remove_building(id);
            g.selected = Selected::None;
            g.build_check = None;
            g.set_status(format!("Removed {}", building_title(g, b)));
        }
        Act::AddJob(t) => {
            let rep = g.bui.repeat;
            g.sim.add_building_job(id, t, rep);
        }
        // No sim API for these yet: edit the job queue in the world directly.
        Act::RemoveJob(k) => {
            if let Some(bb) = bm(g, id) {
                if k < bb.jobs.len() {
                    bb.jobs.remove(k);
                }
            }
        }
        Act::ToggleRepeat(k) => {
            if let Some(j) = bm(g, id).and_then(|bb| bb.jobs.get_mut(k)) {
                j.repeat = !j.repeat;
            }
        }
        Act::ToggleSuspend(k) => {
            if let Some(j) = bm(g, id).and_then(|bb| bb.jobs.get_mut(k)) {
                j.suspended = !j.suspended;
            }
        }
        Act::SetCrop(season, tok) => {
            g.sim.set_farm_crop(id, season, &tok);
            g.bui.crop_season = None;
        }
        Act::ToggleCat(c) => {
            if let Some(bb) = bm(g, id) {
                let sp = bb.stockpile.get_or_insert_with(Default::default);
                if let Some(i) = sp.categories.iter().position(|x| x == c) {
                    sp.categories.remove(i);
                } else {
                    sp.categories.push(c.to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_labels_use_templates() {
        let s = rf_sim::Sim::new(rf_core::World::synthetic(24, 24, 12), 1);
        let t = s.order_templates();
        let d = |job, reaction, it, sub, mat, cat: &'static [String]| JobDesc { job, reaction, item_type: it, item_subtype: sub, mat, mat_category: cat };
        assert_eq!(job_label(&t, &d("CustomReaction", "BREW_DRINK_FROM_PLANT", "", "", "", &[])), "Brew drink from plant");
        let stone = vec!["stone".to_string()];
        let stone: &'static [String] = Box::leak(stone.into_boxed_slice());
        // [industry] crafts and forge jobs now have templates (DF menu labels).
        assert_eq!(job_label(&t, &d("MakeCrafts", "", "", "", "", stone)), "Make rock crafts");
        assert_eq!(job_label(&t, &d("MakeWeapon", "", "WEAPON", "ITEM_WEAPON_PICK", "INORGANIC:IRON", &[])), "Forge metal weapon (pick, iron)");
        assert_eq!(job_label(&t, &d("CustomReaction", "BREW_DRINK_FROM_PLANT_GROWTH", "", "", "", &[])), "Brew drink from fruit");
    }
}
