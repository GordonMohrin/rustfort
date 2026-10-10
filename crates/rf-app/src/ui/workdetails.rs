//! [app8] Labor screen of DF v50 (Steam): Work details, Kitchen, and the old labor grid as the
//! "Advanced" tab.
//!
//! Work details: the list on the left (DF's `WORK_DETAIL_*` icons), the selected detail on the
//! right with its mode (everybody / only selected / nobody does this), "assigned workers only
//! do this", its labors (editable for your own details) and the citizens with a toggle each
//! (`Sim::set_work_detail_member`, `set_work_detail_mode`, `set_work_detail_labors`,
//! `create_work_detail`).
//!
//! Kitchen: every cookable or brewable item kind in the fort with DF's cook/brew toggles
//! (`Sim::set_kitchen`).

use super::labor::GROUPS;
use super::panels::{unit_name, unit_profession};
use super::round8::{frame, scroll, tabs, LaborTab};
use super::{clip, col, draw_icon, icon_label_button, pretty, skill_level_name, text_button, BtnState, Ico};
use crate::font::GH;
use crate::game::{Game, Selected};
use macroquad::prelude::*;
use rf_core::{KitchenSetting, WorkDetail};

/// DF's eleven built-in work details (observed in DF 50 via DFHack, read-only): name, icon,
/// mode, labors. Built-in details cannot be renamed or get other labors (DF `no_modify`).
pub const DEFAULT_DETAILS: &[(&str, &str, &str, &[&str])] = &[
    ("Miners", "WORK_DETAIL_MINERS", "OnlySelectedDoesThis", &["MINE"]),
    ("Woodcutters", "WORK_DETAIL_WOODCUTTERS", "OnlySelectedDoesThis", &["CUTWOOD"]),
    ("Hunters", "WORK_DETAIL_HUNTERS", "OnlySelectedDoesThis", &["HUNT"]),
    ("Planters", "WORK_DETAIL_PLANTERS", "OnlySelectedDoesThis", &["PLANT"]),
    ("Fisherdwarves", "WORK_DETAIL_FISHERMEN", "OnlySelectedDoesThis", &["FISH"]),
    ("Plant gatherers", "WORK_DETAIL_PLANT_GATHERERS", "OnlySelectedDoesThis", &["HERBALIST"]),
    ("Stonecutters", "WORK_DETAIL_STONECUTTERS", "EverybodyDoesThis", &["STONECUTTER"]),
    ("Engravers", "WORK_DETAIL_ENGRAVERS", "EverybodyDoesThis", &["ENGRAVER"]),
    (
        "Haulers",
        "WORK_DETAIL_HAULERS",
        "EverybodyDoesThis",
        &[
            "HAUL_STONE", "HAUL_WOOD", "HAUL_BODY", "HAUL_FOOD", "HAUL_REFUSE", "HAUL_ITEM", "HAUL_FURNITURE", "HAUL_ANIMALS", "HANDLE_VEHICLES",
            "HAUL_TRADE", "HAUL_WATER",
        ],
    ),
    ("Orderlies", "WORK_DETAIL_ORDERLIES", "EverybodyDoesThis", &["SUTURING", "DRESSING_WOUNDS", "FEED_WATER_CIVILIANS", "RECOVER_WOUNDED"]),
    ("Siege operators", "WORK_DETAIL_SIEGE_OPERATORS", "EverybodyDoesThis", &["SIEGEOPERATE"]),
];

/// DF's `work_detail_icon` order (the export may store the index or the name).
const ICONS: &[&str] = &[
    "WORK_DETAIL_MINERS",
    "WORK_DETAIL_WOODCUTTERS",
    "WORK_DETAIL_HUNTERS",
    "WORK_DETAIL_PLANTERS",
    "WORK_DETAIL_FISHERMEN",
    "WORK_DETAIL_STONECUTTERS",
    "WORK_DETAIL_ENGRAVERS",
    "WORK_DETAIL_PLANT_GATHERERS",
    "WORK_DETAIL_HAULERS",
    "WORK_DETAIL_ORDERLIES",
    "WORK_DETAIL_CUSTOM_1",
    "WORK_DETAIL_CUSTOM_2",
    "WORK_DETAIL_CUSTOM_3",
    "WORK_DETAIL_CUSTOM_4",
    "WORK_DETAIL_CUSTOM_5",
    "WORK_DETAIL_CUSTOM_6",
    "WORK_DETAIL_CUSTOM_7",
    "WORK_DETAIL_CUSTOM_8",
    "WORK_DETAIL_SIEGE_OPERATORS",
];

pub const MODES: &[(&str, &str, &str)] = &[
    ("EverybodyDoesThis", "Everybody does this", "LABOR_WORKER_ASSIGNED_EVERYBODY"),
    ("OnlySelectedDoesThis", "Only selected do this", "LABOR_WORKER_ASSIGNED"),
    ("NobodyDoesThis", "Nobody does this", "LABOR_WORKER_ASSIGNED_NOBODY"),
];

/// Interface sprite of a detail: its stored icon (index or name), else by its DF name.
pub fn detail_icon(d: &WorkDetail, idx: usize) -> String {
    let ic = d.icon.trim();
    if let Ok(n) = ic.parse::<usize>() {
        if let Some(t) = ICONS.get(n) {
            return t.to_string();
        }
    }
    if !ic.is_empty() {
        let up = ic.to_uppercase();
        if up.starts_with("WORK_DETAIL_") {
            return up;
        }
        return format!("WORK_DETAIL_{up}");
    }
    if let Some((_, icon, _, _)) = DEFAULT_DETAILS.iter().find(|(n, ..)| *n == d.name) {
        return icon.to_string();
    }
    format!("WORK_DETAIL_CUSTOM_{}", idx % 8 + 1)
}

/// The mode, defaulting like DF (`Default` = only selected).
pub fn mode_of(d: &WorkDetail) -> &'static str {
    match d.mode.as_str() {
        "EverybodyDoesThis" => "EverybodyDoesThis",
        "NobodyDoesThis" => "NobodyDoesThis",
        _ => "OnlySelectedDoesThis",
    }
}

pub fn is_builtin(d: &WorkDetail) -> bool {
    DEFAULT_DETAILS.iter().any(|(n, ..)| *n == d.name)
}

/// The Labor screen frame with its tabs; returns the content rect below them.
pub fn labor_frame(g: &mut Game) -> Option<Rect> {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let c = frame(g, "Labor", 1500.0)?;
    let cur = match g.r8.labor_tab {
        LaborTab::WorkDetails => 0,
        LaborTab::Kitchen => 1,
        LaborTab::Advanced => 2,
    };
    let (hit, _) = tabs(g, c.x + 9.0 * 8.0 * s, c.y - 3.0 * s, &["Work details", "Kitchen", "Advanced: all labors"], cur);
    if let Some(i) = hit {
        g.r8.labor_tab = [LaborTab::WorkDetails, LaborTab::Kitchen, LaborTab::Advanced][i];
        g.labor.typing = false;
        g.r8.wd_new = None;
    }
    let top = c.y + 2.0 * lh + 4.0 * s;
    draw_line(c.x, top - 6.0 * s, c.x + c.w, top - 6.0 * s, 1.0, col::BORDER);
    Some(Rect::new(c.x, top, c.w, c.y + c.h - top))
}

/// Create DF's built-in work details (old exports have none). Units whose labors already
/// contain all labors of an "only selected" detail are assigned to it.
pub fn create_defaults(g: &mut Game) {
    for (i, (name, _icon, mode, labors)) in DEFAULT_DETAILS.iter().enumerate() {
        if g.sim.world.work_details.iter().any(|d| d.name == *name) {
            continue;
        }
        g.sim.create_work_detail(name, labors);
        g.sim.set_work_detail_mode(name, mode);
        if let Some(d) = g.sim.world.work_details.iter_mut().find(|d| d.name == *name) {
            d.icon = [0, 1, 2, 3, 4, 7, 5, 6, 8, 9, 18][i].to_string();
        }
        if *mode == "OnlySelectedDoesThis" {
            let ids: Vec<i32> = g.sim.world.citizens().filter(|u| labors.iter().all(|l| u.has_labor(l))).map(|u| u.id).collect();
            for id in ids {
                g.sim.set_work_detail_member(name, id, true);
            }
        }
    }
    g.set_status(format!("{} work details", g.sim.world.work_details.len()));
}

/// Skill token of a labor (from the labor grid table).
fn labor_skill(labor: &str) -> &'static str {
    GROUPS.iter().flat_map(|(_, cols)| cols.iter()).find(|c| c.labor == labor).map(|c| c.skill).unwrap_or("")
}

/// All labors once, grouped (group name, labors) in the grid's order.
fn labor_groups() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for (name, cols) in GROUPS {
        let mut v = Vec::new();
        for c in cols.iter() {
            if !seen.contains(&c.labor) {
                seen.push(c.labor);
                v.push(c.labor);
            }
        }
        if !v.is_empty() {
            out.push((if *name == "Core" { "Basic & hauling" } else { *name }, v));
        }
    }
    out.push(("Military & other", vec!["SIEGEOPERATE", "HAUL_TRADE", "BUILD_ROAD"].into_iter().filter(|l| !seen.contains(l)).collect()));
    out.retain(|(_, v)| !v.is_empty());
    out
}

fn labor_label(l: &str) -> String {
    match l {
        "MINE" => "Mining".into(),
        "CUTWOOD" => "Wood cutting".into(),
        "HUNT" => "Hunting".into(),
        "PLANT" => "Farming (fields)".into(),
        "FISH" => "Fishing".into(),
        "HERBALIST" => "Plant gathering".into(),
        "SIEGEOPERATE" => "Siege operating".into(),
        "FEED_WATER_CIVILIANS" => "Feed patients/prisoners".into(),
        "RECOVER_WOUNDED" => "Recovering wounded".into(),
        "HANDLE_VEHICLES" => "Push/haul vehicles".into(),
        _ => pretty(l),
    }
}

pub fn draw(g: &mut Game, c: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let n = g.sim.world.work_details.len();
    if n == 0 {
        g.font.draw("This save has no work details (exported before DF's v50 work details were exported).", c.x, c.y, s, col::TEXT);
        g.font.draw("Labors are set per dwarf in the Advanced tab, or create DF's eleven default details:", c.x, c.y + lh, s, col::TEXT_DIM);
        let br = Rect::new(c.x, c.y + 2.5 * lh, 44.0 * 8.0 * s, GH * s + 10.0 * s);
        if text_button(&mut g.ui, &g.font, br, "Set up DF's default work details", BtnState::Normal) {
            create_defaults(g);
        }
        return;
    }
    g.r8.detail = g.r8.detail.min(n - 1);
    // ---- left: the list of details
    let lw = 30.0 * 8.0 * s;
    let row_h = 40.0 * s;
    let list = Rect::new(c.x, c.y, lw, c.h - 2.0 * (GH * s + 10.0 * s));
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.wd_scroll = scroll(g, list, g.r8.wd_scroll, n, vis);
    let mut pick = None;
    for i in g.r8.wd_scroll..(g.r8.wd_scroll + vis).min(n) {
        let d = g.sim.world.work_details[i].clone();
        let y = list.y + (i - g.r8.wd_scroll) as f32 * row_h;
        let rr = Rect::new(list.x, y, lw - 6.0 * s, row_h - 3.0 * s);
        if super::list_row(&mut g.ui, rr, i == g.r8.detail) {
            pick = Some(i);
        }
        draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, Color::new(0.30, 0.30, 0.33, 1.0));
        let ir = Rect::new(rr.x + 2.0 * s, rr.y + (rr.h - 32.0 * s) / 2.0, 32.0 * s * 32.0 / 36.0, 32.0 * s);
        if !draw_icon(&g.gfx, &mut g.rend, &detail_icon(&d, i), ir, WHITE) {
            super::draw_fallback_icon(&g.font, Ico::Hammer, ir.x, ir.y, ir.w, ir.h);
        }
        let tx = ir.x + ir.w + 6.0 * s;
        g.font.draw(&clip(&d.name, 22), tx, rr.y + 4.0 * s, s, if i == g.r8.detail { col::TEXT_HI } else { col::TEXT });
        let m = mode_of(&d);
        let (mt, mc) = match m {
            "EverybodyDoesThis" => ("everybody".to_string(), col::TEXT_GOOD),
            "NobodyDoesThis" => ("nobody".to_string(), col::TEXT_BAD),
            _ => (format!("{} assigned", d.assigned.len()), col::TEXT_DIM),
        };
        g.font.draw(&mt, tx, rr.y + 4.0 * s + lh, s, mc);
    }
    if let Some(i) = pick {
        g.r8.detail = i;
        g.r8.wd_edit = false;
        g.r8.wd_uscroll = 0;
    }
    // New work detail.
    let by = c.y + c.h - (GH * s + 10.0 * s);
    if let Some(name) = g.r8.wd_new.clone() {
        let mut name = name;
        for ch in g.input.chars.clone() {
            if !ch.is_control() && name.chars().count() < 24 {
                name.push(ch);
            }
        }
        if g.input.fired.contains(&KeyCode::Backspace) {
            name.pop();
        }
        let fr = Rect::new(c.x, by - (GH * s + 14.0 * s), lw - 6.0 * s, GH * s + 8.0 * s);
        draw_rectangle(fr.x, fr.y, fr.w, fr.h, Color::new(0.0, 0.0, 0.0, 0.6));
        draw_rectangle_lines(fr.x, fr.y, fr.w, fr.h, 1.0, col::ACTIVE_BORDER);
        let caret = if (get_time() * 2.0) as i64 % 2 == 0 { "_" } else { "" };
        let shown = if name.is_empty() { "type a name".to_string() } else { format!("{name}{caret}") };
        g.font.draw(&shown, fr.x + 4.0 * s, fr.y + 4.0 * s, s, if name.is_empty() { col::TEXT_DIM } else { col::TEXT_HI });
        let half = (lw - 12.0 * s) / 2.0;
        let enter = g.input.fired.contains(&KeyCode::Enter);
        let ok = !name.trim().is_empty() && !g.sim.world.work_details.iter().any(|d| d.name == name.trim());
        let create = text_button(&mut g.ui, &g.font, Rect::new(c.x, by, half, GH * s + 8.0 * s), "Create", if ok { BtnState::Normal } else { BtnState::Disabled }) || (enter && ok);
        if text_button(&mut g.ui, &g.font, Rect::new(c.x + half + 6.0 * s, by, half, GH * s + 8.0 * s), "Cancel", BtnState::Normal) {
            g.r8.wd_new = None;
        } else if create {
            let nm = name.trim().to_string();
            g.sim.create_work_detail(&nm, &[]);
            let idx = g.sim.world.work_details.len() - 1;
            let used: Vec<String> = g.sim.world.work_details.iter().map(|d| d.icon.clone()).collect();
            let custom = (10..18).find(|k| !used.contains(&k.to_string())).unwrap_or(10);
            g.sim.world.work_details[idx].icon = custom.to_string();
            g.r8.detail = idx;
            g.r8.wd_edit = true;
            g.r8.wd_new = None;
            g.set_status(format!("Created work detail {nm}: pick its labors"));
        } else {
            g.r8.wd_new = Some(name);
        }
    } else if icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, Rect::new(c.x, by, lw - 6.0 * s, GH * s + 10.0 * s), "", Ico::Glyph(b'+', [120, 220, 120]), "New work detail", BtnState::Normal) {
        g.r8.wd_new = Some(String::new());
    }
    // ---- right: the selected detail
    let i = g.r8.detail;
    let d = g.sim.world.work_details[i].clone();
    let rx = c.x + lw + 10.0 * s;
    let rw = c.x + c.w - rx;
    draw_line(rx - 6.0 * s, c.y, rx - 6.0 * s, c.y + c.h, 1.0, col::BORDER);
    let mut y = c.y;
    let ir = Rect::new(rx, y, 32.0 * s, 36.0 * s);
    if !draw_icon(&g.gfx, &mut g.rend, &detail_icon(&d, i), ir, WHITE) {
        super::draw_fallback_icon(&g.font, Ico::Hammer, ir.x, ir.y, ir.w, ir.h);
    }
    g.font.draw(&d.name, rx + 40.0 * s, y + 2.0 * s, 1.5 * s, col::TEXT_HI);
    let labs: Vec<String> = d.labors.iter().map(|l| labor_label(l)).collect();
    let lt = if labs.is_empty() { "No labors yet: Edit labors".to_string() } else { format!("Labors: {}", labs.join(", ")) };
    g.font.draw(&clip(&lt, ((rw - 44.0 * s) / (8.0 * s)) as usize), rx + 40.0 * s, y + 2.0 * s + 1.5 * GH * s + 4.0 * s, s, if labs.is_empty() { col::TEXT_BAD } else { col::TEXT_DIM });
    y += 44.0 * s;
    // Mode buttons.
    let bh = 30.0 * s;
    let bw = ((rw - 12.0 * s) / 3.0).min(30.0 * 8.0 * s);
    let cur = mode_of(&d);
    let mut set_mode = None;
    for (k, (m, label, icon)) in MODES.iter().enumerate() {
        let r = Rect::new(rx + k as f32 * (bw + 6.0 * s), y, bw, bh);
        let st = if cur == *m { BtnState::Active } else { BtnState::Normal };
        if icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, r, icon, Ico::Smiley, label, st) {
            set_mode = Some(*m);
        }
    }
    if let Some(m) = set_mode {
        g.sim.set_work_detail_mode(&d.name, m);
    }
    y += bh + 6.0 * s;
    // "Assigned workers only do this" / Edit labors.
    let (ot, oi) = if d.no_other_work {
        ("Assigned workers only do this", "WORKER_ONLY_DO_ASSIGNED_JOBS")
    } else {
        ("Assigned workers also do other jobs", "WORKER_DO_ANY_AVAILABLE_JOB")
    };
    let r1 = Rect::new(rx, y, bw * 1.4, bh);
    if icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, r1, oi, Ico::Hammer, ot, if d.no_other_work { BtnState::Active } else { BtnState::Normal }) {
        // No sim API for this flag (DF `work_detail.flags`): set it in the world.
        g.sim.world.work_details[i].no_other_work = !d.no_other_work;
    }
    let builtin = is_builtin(&d);
    let r2 = Rect::new(r1.x + r1.w + 6.0 * s, y, bw * 0.9, bh);
    let est = if builtin { BtnState::Disabled } else if g.r8.wd_edit { BtnState::Active } else { BtnState::Normal };
    if icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, r2, "LABOR_EDIT_WORK_DETAIL", Ico::Hammer, "Edit labors", est) {
        g.r8.wd_edit = !g.r8.wd_edit;
        g.r8.wd_escroll = 0;
    }
    if builtin && g.ui.hovered(r2) {
        g.ui.tooltip = Some(("DF's built-in work details keep their labors".into(), vec2(r2.x, r2.y - 4.0)));
    }
    if !builtin {
        let r3 = Rect::new(r2.x + r2.w + 6.0 * s, y, 10.0 * 8.0 * s, bh);
        if text_button(&mut g.ui, &g.font, r3, "Delete", BtnState::Normal) {
            // No sim API: remove it from the world (the sim recomputes labors from details).
            g.sim.world.work_details.remove(i);
            for u in g.sim.world.units.iter_mut() {
                u.work_details.retain(|n| *n != d.name);
            }
            g.r8.detail = i.saturating_sub(1);
            g.r8.wd_edit = false;
            return;
        }
    }
    y += bh + 8.0 * s;
    let area = Rect::new(rx, y, rw, c.y + c.h - y);
    if g.r8.wd_edit && !builtin {
        draw_labor_editor(g, &d, area);
    } else {
        draw_members(g, &d, area);
    }
}

fn draw_labor_editor(g: &mut Game, d: &WorkDetail, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    g.font.draw("Labors of this work detail (click to toggle):", area.x, area.y, s, col::BORDER_HI);
    let groups = labor_groups();
    let colw = 24.0 * 8.0 * s;
    let ncols = ((area.w / colw) as usize).max(1);
    let row = lh + 3.0 * s;
    let top = area.y + lh + 6.0 * s;
    let mut toggle = None;
    // Flow the groups into columns.
    let (mut cx, mut cy, mut col_i) = (area.x, top, 0usize);
    for (gname, labs) in &groups {
        let need = (labs.len() + 1) as f32 * row;
        if cy + need > area.y + area.h && cy > top {
            col_i += 1;
            if col_i >= ncols {
                break;
            }
            cx = area.x + col_i as f32 * colw;
            cy = top;
        }
        g.font.draw(gname, cx, cy, s, col::TEXT_HI);
        cy += row;
        for l in labs {
            if cy > area.y + area.h - row {
                break;
            }
            let on = d.labors.iter().any(|x| x == l);
            if super::checkbox(&mut g.ui, &g.font, cx, cy, on, &clip(&labor_label(l), 21)) {
                toggle = Some(*l);
            }
            cy += row;
        }
        cy += 4.0 * s;
    }
    if let Some(l) = toggle {
        let mut v: Vec<String> = d.labors.clone();
        if let Some(k) = v.iter().position(|x| x == l) {
            v.remove(k);
        } else {
            v.push(l.to_string());
        }
        let refs: Vec<&str> = v.iter().map(|x| x.as_str()).collect();
        g.sim.set_work_detail_labors(&d.name, &refs);
    }
}

fn draw_members(g: &mut Game, d: &WorkDetail, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let mode = mode_of(d);
    let skills: Vec<&str> = d.labors.iter().map(|l| labor_skill(l)).filter(|k| !k.is_empty()).collect();
    // Citizens: best skill of the detail's labors first.
    let mut rows: Vec<(i32, String, String, i32, String)> = g
        .sim
        .world
        .citizens()
        .map(|u| {
            let best = skills.iter().map(|k| u.skill(k)).max().unwrap_or(0);
            let others: Vec<&str> = g.sim.world.work_details.iter().filter(|o| o.name != d.name && o.assigned.contains(&u.id)).map(|o| o.name.as_str()).collect();
            (u.id, unit_name(u), unit_profession(u), best, others.join(", "))
        })
        .collect();
    rows.sort_by(|a, b| b.3.cmp(&a.3).then(a.1.cmp(&b.1)));
    let assigned = |id: i32| match mode {
        "EverybodyDoesThis" => true,
        "NobodyDoesThis" => false,
        _ => d.assigned.contains(&id),
    };
    let n_on = rows.iter().filter(|r| assigned(r.0)).count();
    let head = match mode {
        "EverybodyDoesThis" => format!("Everybody does this ({} citizens). Click a dwarf to switch to \"only selected\" without them.", rows.len()),
        "NobodyDoesThis" => "Nobody does this. Click a dwarf to make it \"only selected\" with them.".to_string(),
        _ => format!("{n_on} of {} citizens assigned. Click to toggle.", rows.len()),
    };
    g.font.draw(&clip(&head, (area.w / (8.0 * s)) as usize), area.x, area.y, s, col::BORDER_HI);
    let hy = area.y + lh + 2.0 * s;
    let cname = area.x + 30.0 * s;
    let cprof = cname + 22.0 * 8.0 * s;
    let cskill = cprof + 16.0 * 8.0 * s;
    let cother = cskill + 16.0 * 8.0 * s;
    for (x, t) in [(cname, "Name"), (cprof, "Profession"), (cskill, "Skill"), (cother, "Other work details")] {
        g.font.draw(t, x, hy, s, col::TEXT_DIM);
    }
    let row_h = 26.0 * s;
    let list = Rect::new(area.x, hy + lh + 2.0 * s, area.w, area.y + area.h - (hy + lh + 2.0 * s));
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.wd_uscroll = scroll(g, list, g.r8.wd_uscroll, rows.len(), vis);
    let mut click = None;
    let maxo = ((list.x + list.w - cother) / (8.0 * s)).max(4.0) as usize;
    for (k, (id, name, prof, best, others)) in rows.iter().enumerate().skip(g.r8.wd_uscroll).take(vis) {
        let y = list.y + (k - g.r8.wd_uscroll) as f32 * row_h;
        let rr = Rect::new(list.x, y, list.w, row_h - 2.0 * s);
        if k % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        let on = assigned(*id);
        let hit = super::list_row(&mut g.ui, rr, false);
        let icon = match (mode, on) {
            ("EverybodyDoesThis", _) => "LABOR_WORKER_ASSIGNED_EVERYBODY",
            ("NobodyDoesThis", _) => "LABOR_WORKER_ASSIGNED_NOBODY",
            (_, true) => "LABOR_WORKER_ASSIGNED",
            _ => "LABOR_WORKER_UNASSIGNED",
        };
        let ir = Rect::new(rr.x + 2.0 * s, rr.y + 1.0 * s, 22.0 * s, rr.h - 2.0 * s);
        if !draw_icon(&g.gfx, &mut g.rend, icon, ir, WHITE) {
            super::draw_fallback_icon(&g.font, if on { Ico::Glyph(251, [120, 230, 120]) } else { Ico::Glyph(250, [140, 140, 140]) }, ir.x, ir.y, ir.w, ir.h);
        }
        let ty = rr.y + (rr.h - GH * s) / 2.0;
        let sel = g.selected == Selected::Unit(*id);
        g.font.draw(&clip(name, 21), cname, ty, s, if sel { col::TEXT_HI } else if on { col::TEXT } else { col::TEXT_DIM });
        g.font.draw(&clip(prof, 15), cprof, ty, s, col::TEXT_DIM);
        if !skills.is_empty() {
            let t = if *best > 0 { format!("{} ({best})", skill_level_name(*best)) } else { "-".to_string() };
            g.font.draw(&clip(&t, 15), cskill, ty, s, if *best >= 10 { col::TEXT_HI } else if *best > 0 { col::TEXT_GOOD } else { col::TEXT_DIM });
        }
        g.font.draw(&clip(others, maxo), cother, ty, s, col::TEXT_DIM);
        if hit {
            click = Some(*id);
        }
    }
    if let Some(id) = click {
        match mode {
            "EverybodyDoesThis" => {
                // Like DF: everybody minus this one, as "only selected".
                g.sim.set_work_detail_mode(&d.name, "OnlySelectedDoesThis");
                let ids: Vec<i32> = rows.iter().map(|r| r.0).filter(|&u| u != id).collect();
                for u in ids {
                    g.sim.set_work_detail_member(&d.name, u, true);
                }
                g.sim.set_work_detail_member(&d.name, id, false);
            }
            "NobodyDoesThis" => {
                g.sim.set_work_detail_mode(&d.name, "OnlySelectedDoesThis");
                for &u in &d.assigned {
                    g.sim.set_work_detail_member(&d.name, u, false);
                }
                g.sim.set_work_detail_member(&d.name, id, true);
            }
            _ => {
                let on = d.assigned.contains(&id);
                g.sim.set_work_detail_member(&d.name, id, !on);
            }
        }
    }
}

// ------------------------------------------------------------------ kitchen

/// One kitchen line: an item kind and material in the fort.
#[derive(Clone, Debug)]
pub struct KRow {
    pub item_type: String,
    pub subtype: String,
    pub mat: String,
    pub name: String,
    pub count: i32,
    pub can_cook: bool,
    pub can_brew: bool,
}

/// Item types DF's kitchen screen lists.
const KITCHEN_TYPES: &[&str] = &["PLANT", "PLANT_GROWTH", "SEEDS", "DRINK", "MEAT", "FISH", "FISH_RAW", "EGG", "CHEESE", "LIQUID_MISC", "POWDER_MISC", "GLOB"];

fn type_label(t: &str) -> &'static str {
    match t {
        "PLANT" => "Plant",
        "PLANT_GROWTH" => "Fruit/leaves",
        "SEEDS" => "Seeds",
        "DRINK" => "Drink",
        "MEAT" => "Meat",
        "FISH" => "Fish",
        "FISH_RAW" => "Unprepared fish",
        "EGG" => "Egg",
        "CHEESE" => "Cheese",
        "LIQUID_MISC" => "Liquid",
        "POWDER_MISC" => "Powder",
        "GLOB" => "Fat",
        _ => "Other",
    }
}

/// The kitchen lines of the fort's current stocks (not rotten), sorted by type and name.
pub fn kitchen_rows(g: &Game) -> Vec<KRow> {
    let w = &g.sim.world;
    let mut rows: Vec<KRow> = Vec::new();
    for it in &w.items {
        if it.flags.rotten || it.flags.construction || !KITCHEN_TYPES.contains(&it.kind.as_str()) {
            continue;
        }
        let tok = w.material(it.mat).map(|m| m.token.as_str()).unwrap_or("");
        if let Some(r) = rows.iter_mut().find(|r| r.item_type == it.kind && r.mat == tok && r.subtype == it.subtype) {
            r.count += it.stack.max(1);
            continue;
        }
        let mname = w.material(it.mat).map(|m| m.name.trim_start_matches("frozen ").to_string()).unwrap_or_default();
        let mut parts = tok.split(':');
        let (kind, who) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let plant = (kind == "PLANT").then(|| w.plant_raws.iter().find(|p| p.token == who)).flatten();
        let pname = plant.map(|p| p.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| pretty(who).to_lowercase());
        let creature = || {
            w.creature_raws.iter().find(|c| c.token == who).map(|c| c.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| pretty(who).to_lowercase())
        };
        let name = match it.kind.as_str() {
            "PLANT" => pname.clone(),
            "PLANT_GROWTH" => format!("{pname} {}", pretty(&it.subtype).to_lowercase()),
            "SEEDS" => format!("{pname} seeds"),
            "DRINK" => mname.clone(),
            "MEAT" | "FISH" | "FISH_RAW" | "EGG" if kind == "CREATURE" => format!("{} {}", creature(), type_label(&it.kind).to_lowercase()),
            _ if kind == "CREATURE" => format!("{} {mname}", creature()),
            _ => {
                if mname.is_empty() {
                    pretty(&it.kind)
                } else {
                    mname.clone()
                }
            }
        };
        let edible = plant.map(|p| p.edible_cooked || p.edible_raw).unwrap_or(true);
        let brewable = plant.map(|p| p.brewable).unwrap_or(false);
        let (can_cook, can_brew) = match it.kind.as_str() {
            "PLANT" => (edible, brewable),
            "PLANT_GROWTH" => (edible, brewable),
            "FISH_RAW" => (false, false),
            _ => (true, false),
        };
        rows.push(KRow {
            item_type: it.kind.clone(),
            subtype: if it.kind == "PLANT_GROWTH" { it.subtype.clone() } else { String::new() },
            mat: tok.to_string(),
            name,
            count: it.stack.max(1),
            can_cook,
            can_brew,
        });
    }
    rows.sort_by(|a, b| {
        let ka = KITCHEN_TYPES.iter().position(|t| *t == a.item_type);
        let kb = KITCHEN_TYPES.iter().position(|t| *t == b.item_type);
        ka.cmp(&kb).then(a.name.cmp(&b.name))
    });
    rows
}

/// The kitchen setting of a line (no entry = cook and brew allowed).
pub fn kitchen_of<'a>(w: &'a rf_core::World, r: &KRow) -> Option<&'a KitchenSetting> {
    w.kitchen.iter().find(|k| k.item_type == r.item_type && k.mat == r.mat && (k.item_subtype == r.subtype || k.item_subtype.is_empty()))
}

pub fn draw_kitchen(g: &mut Game, c: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let rows = kitchen_rows(g);
    let filters = ["All", "Plants", "Drinks", "Seeds", "Animal products"];
    let (hit, _) = tabs(g, c.x, c.y, &filters, kitchen_filter(g));
    if let Some(i) = hit {
        set_kitchen_filter(g, i);
        g.r8.k_scroll = 0;
    }
    let f = kitchen_filter(g);
    let rows: Vec<KRow> = rows
        .into_iter()
        .filter(|r| match f {
            1 => r.item_type == "PLANT" || r.item_type == "PLANT_GROWTH",
            2 => r.item_type == "DRINK",
            3 => r.item_type == "SEEDS",
            4 => !matches!(r.item_type.as_str(), "PLANT" | "PLANT_GROWTH" | "DRINK" | "SEEDS"),
            _ => true,
        })
        .collect();
    let hy = c.y + GH * s + 16.0 * s;
    let (cn, ct, cc, ccook, cbrew) = (c.x + 4.0 * s, c.x + 34.0 * 8.0 * s, c.x + 50.0 * 8.0 * s, c.x + 58.0 * 8.0 * s, c.x + 66.0 * 8.0 * s);
    for (x, t) in [(cn, "Item"), (ct, "Type"), (cc, "Count"), (ccook, "Cook"), (cbrew, "Brew")] {
        g.font.draw(t, x, hy, s, col::TEXT_DIM);
    }
    let row_h = 30.0 * s;
    let list = Rect::new(c.x, hy + lh + 2.0 * s, c.w, c.y + c.h - (hy + lh + 2.0 * s) - lh - 4.0 * s);
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.k_scroll = scroll(g, list, g.r8.k_scroll, rows.len(), vis);
    let mut set = None;
    for (k, r) in rows.iter().enumerate().skip(g.r8.k_scroll).take(vis) {
        let y = list.y + (k - g.r8.k_scroll) as f32 * row_h;
        if k % 2 == 0 {
            draw_rectangle(list.x, y, list.w.min(76.0 * 8.0 * s), row_h - 2.0 * s, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        let ks = kitchen_of(&g.sim.world, r);
        let (no_cook, no_brew) = ks.map(|k| (k.no_cook, k.no_brew)).unwrap_or((false, false));
        let ty = y + (row_h - 2.0 * s - GH * s) / 2.0;
        g.font.draw(&clip(&r.name, 32), cn, ty, s, col::TEXT);
        g.font.draw(type_label(&r.item_type), ct, ty, s, col::TEXT_DIM);
        g.font.draw(&r.count.to_string(), cc, ty, s, col::TEXT_DIM);
        for (x, can, off, pre, cook) in [(ccook, r.can_cook, no_cook, "LABOR_KITCHEN_COOK_", true), (cbrew, r.can_brew, no_brew, "LABOR_KITCHEN_BREW_", false)] {
            let ir = Rect::new(x, y, 24.0 * s, 27.0 * s);
            let state = if !can { "CANNOT" } else if off { "RESTRICTED" } else { "ALLOWED" };
            if !draw_icon(&g.gfx, &mut g.rend, &format!("{pre}{state}"), ir, WHITE) {
                let gl = if !can { (b'-', [120, 120, 120]) } else if off { (b'x', [230, 90, 80]) } else { (251u8, [120, 230, 120]) };
                super::draw_fallback_icon(&g.font, Ico::Glyph(gl.0, gl.1), ir.x, ir.y, ir.w, ir.h);
            }
            if can && g.ui.hovered(ir) {
                let what = if cook { "Cooking" } else { "Brewing" };
                g.ui.tooltip = Some((format!("{what} {} - click to {}", if off { "forbidden" } else { "allowed" }, if off { "allow" } else { "forbid" }), vec2(ir.x, ir.y - 4.0)));
            }
            if can && g.ui.clicked(ir) {
                set = Some(KitchenSetting {
                    item_type: r.item_type.clone(),
                    item_subtype: r.subtype.clone(),
                    mat: r.mat.clone(),
                    no_cook: if cook { !no_cook } else { no_cook },
                    no_brew: if cook { no_brew } else { !no_brew },
                });
            }
        }
    }
    if let Some(k) = set {
        g.sim.set_kitchen(k);
    }
    let fy = c.y + c.h - GH * s;
    let n_off = g.sim.world.kitchen.len();
    g.font.draw(
        &format!("{} kinds in stock, {n_off} kitchen restrictions. Plump helmets are brewed, never cooked (your rule, set when the save had none).", rows.len()),
        c.x,
        fy,
        s,
        col::TEXT_DIM,
    );
}

fn kitchen_filter(g: &Game) -> usize {
    g.r8.k_filter
}

fn set_kitchen_filter(g: &mut Game, i: usize) {
    g.r8.k_filter = i;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_details_like_df() {
        assert_eq!(DEFAULT_DETAILS.len(), 11);
        assert_eq!(DEFAULT_DETAILS.iter().filter(|d| d.2 == "OnlySelectedDoesThis").count(), 6);
        let d = WorkDetail { name: "Smelters".into(), icon: "0".into(), ..Default::default() };
        assert_eq!(detail_icon(&d, 11), "WORK_DETAIL_MINERS");
        let d = WorkDetail { name: "Siege operators".into(), icon: "18".into(), ..Default::default() };
        assert_eq!(detail_icon(&d, 10), "WORK_DETAIL_SIEGE_OPERATORS");
        let d = WorkDetail { name: "Haulers".into(), ..Default::default() };
        assert_eq!(detail_icon(&d, 8), "WORK_DETAIL_HAULERS");
        assert_eq!(mode_of(&WorkDetail::default()), "OnlySelectedDoesThis");
        // Every labor appears once in the editor.
        let all: Vec<&str> = labor_groups().into_iter().flat_map(|(_, v)| v).collect();
        let mut dedup = all.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(all.len(), dedup.len());
        assert!(all.contains(&"SIEGEOPERATE") && all.contains(&"HAUL_WATER"));
    }
}
