//! [app8] Round-8 screens: shared state, helpers and the hooks `game.rs` calls.
//!
//! - `workdetails`: Labor > Work details (v50), the old labor grid as "Advanced", Kitchen.
//! - `stockpile_ed`: DF Steam stockpile settings (categories tree, presets, containers, links).
//! - `animals`: pets and livestock (slaughter, pasture, training).
//! - `places`: Places (locations, zones, stockpiles, workshops), Justice, Objects, World.
//! - `minimap`: the minimap of the current level (top right, toggle).

use super::{clip, col, panel, text_button, wheel_over, BtnState};
use crate::font::GH;
use crate::game::{Game, Screen, Selected, Tool};
use macroquad::prelude::*;
use rf_core::Pos;

/// Tabs of the Labor screen (DF Steam: Work details, Kitchen; plus the old grid).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LaborTab {
    #[default]
    WorkDetails,
    Kitchen,
    /// The per-labor grid of rounds 1-7 (DF's old labor screen / Dwarf Therapist style).
    Advanced,
}

#[derive(Default)]
pub struct R8Ui {
    pub labor_tab: LaborTab,
    // ---- work details
    /// Selected work detail (index into `World::work_details`).
    pub detail: usize,
    pub wd_scroll: usize,
    pub wd_uscroll: usize,
    /// The labor editor of the selected detail is open.
    pub wd_edit: bool,
    pub wd_escroll: usize,
    /// "New work detail" name field.
    pub wd_new: Option<String>,
    // ---- kitchen
    pub k_scroll: usize,
    pub k_filter: usize,
    // ---- stockpile editor
    /// Stockpile building shown in the settings editor.
    pub pile: Option<i32>,
    pub sp_cat: usize,
    pub sp_group: usize,
    pub sp_scroll: usize,
    /// Cached leaf options per category: (pile id, category) -> options.
    pub sp_cache: Vec<(String, Vec<(String, String)>)>,
    // ---- animals
    pub an_tab: usize,
    pub an_scroll: usize,
    /// Animal whose pasture picker is open.
    pub an_pick: Option<i32>,
    pub an_pscroll: usize,
    // ---- places
    pub pl_tab: usize,
    pub pl_scroll: usize,
    /// Zone whose "new location" kind picker is open.
    pub pl_make: Option<i32>,
    /// Kind chosen in the picker (a temple then asks for the deity).
    pub pl_kind: Option<&'static str>,
    pub pl_dscroll: usize,
    /// (location id, occupation kind) whose citizen picker is open.
    pub pl_occ: Option<(i32, String)>,
    pub pl_oscroll: usize,
    // ---- justice
    pub j_tab: usize,
    pub j_scroll: usize,
    pub j_convict: Option<i32>,
    pub j_pscroll: usize,
    // ---- objects, world
    pub o_scroll: usize,
    pub w_tab: usize,
    pub w_scroll: usize,
    // ---- minimap
    pub minimap: bool,
    pub mm: super::minimap::MinimapCache,
}

/// A row of a picker list.
pub struct PickRow {
    pub id: i32,
    pub label: String,
    pub info: String,
    pub color: Color,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    Nothing,
    Picked(i32),
    Cancel,
}

/// A full-window screen frame with a title and a close button. Returns the content rect, or
/// `None` when it was closed this frame.
pub fn frame(g: &mut Game, title: &str, max_w: f32) -> Option<Rect> {
    let s = g.ui.scale;
    let r = super::orders::screen_rect(g, max_w);
    panel(&mut g.ui, r);
    let pad = 12.0 * s;
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        g.screen = Screen::None;
        return None;
    }
    g.font.draw(title, r.x + pad, r.y + pad, 1.5 * s, col::TEXT_HI);
    Some(Rect::new(r.x + pad, r.y + pad, r.w - 2.0 * pad, r.h - 2.0 * pad))
}

/// A row of text tabs at (x, y). Returns the clicked tab and the x after the last one.
pub fn tabs(g: &mut Game, x: f32, y: f32, labels: &[&str], cur: usize) -> (Option<usize>, f32) {
    let s = g.ui.scale;
    let mut tx = x;
    let mut hit = None;
    for (i, l) in labels.iter().enumerate() {
        let tw = (l.chars().count() as f32 + 2.0) * 8.0 * s;
        let st = if cur == i { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(tx, y, tw, GH * s + 8.0 * s), l, st) {
            hit = Some(i);
        }
        tx += tw + 4.0 * s;
    }
    (hit, tx)
}

/// Scroll a list of `n` rows with `rows` visible by the wheel over `area`.
pub fn scroll(g: &mut Game, area: Rect, cur: usize, n: usize, rows: usize) -> usize {
    let step = wheel_over(&g.ui, &mut g.input, area);
    (cur as i32 + step * 3).clamp(0, n.saturating_sub(rows) as i32) as usize
}

/// A modal picker list (citizens, zones, deities) over `r`. Returns the new scroll and what
/// was picked. `none_label` adds a first row with id -1 (e.g. "Nobody").
pub fn picker(g: &mut Game, r: Rect, title: &str, rows: &[PickRow], scroll_pos: usize, none_label: Option<&str>) -> (usize, Pick) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    panel(&mut g.ui, r);
    let pad = 10.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    g.font.draw(title, x, y, s, col::TEXT_HI);
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    let mut out = Pick::Nothing;
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        out = Pick::Cancel;
    }
    y += lh + 4.0 * s;
    let row_h = lh + 3.0 * s;
    let area = Rect::new(x, y, r.w - 2.0 * pad, r.y + r.h - pad - y);
    let vis = ((area.h / row_h) as usize).max(1);
    let total = rows.len() + none_label.is_some() as usize;
    let sc = scroll(g, area, scroll_pos, total, vis);
    let maxc = ((area.w - 8.0 * s) / (8.0 * s)) as usize;
    let mut k = 0usize;
    let draw_row = |g: &mut Game, y: f32, id: i32, label: &str, info: &str, c: Color| -> bool {
        let rr = Rect::new(x, y, area.w, row_h - 1.0);
        let hit = super::list_row(&mut g.ui, rr, false);
        let iw = info.chars().count().min(maxc / 2);
        g.font.draw(&clip(label, maxc.saturating_sub(iw + 2)), x + 4.0 * s, y + 2.0 * s, s, c);
        if !info.is_empty() {
            let t = clip(info, iw.max(1));
            g.font.draw(&t, rr.x + rr.w - g.font.width(&t, s) - 4.0 * s, y + 2.0 * s, s, col::TEXT_DIM);
        }
        let _ = id;
        hit
    };
    if let Some(nl) = none_label {
        if k >= sc && k < sc + vis {
            if draw_row(g, y, -1, nl, "", col::TEXT_DIM) {
                out = Pick::Picked(-1);
            }
            y += row_h;
        }
        k += 1;
    }
    for row in rows {
        if k >= sc + vis {
            break;
        }
        if k >= sc {
            if draw_row(g, y, row.id, &row.label, &row.info, row.color) {
                out = Pick::Picked(row.id);
            }
            y += row_h;
        }
        k += 1;
    }
    if rows.is_empty() && none_label.is_none() {
        g.font.draw("Nothing to choose.", x, y, s, col::TEXT_DIM);
    }
    if g.ui.rpressed && r.contains(g.ui.mouse) {
        out = Pick::Cancel;
    }
    (sc, out)
}

/// Citizens as picker rows (name, profession), sorted by name.
pub fn citizen_rows(g: &Game) -> Vec<PickRow> {
    let mut v: Vec<PickRow> = g
        .sim
        .world
        .citizens()
        .map(|u| PickRow { id: u.id, label: super::panels::unit_name(u), info: super::panels::unit_profession(u), color: col::TEXT })
        .collect();
    v.sort_by(|a, b| a.label.cmp(&b.label));
    v
}

/// Name of a unit id ("vacant" for -1, `#id` when it is not on the map).
pub fn unit_label(g: &Game, id: i32) -> String {
    if id < 0 {
        return String::new();
    }
    g.sim.world.unit(id).map(super::panels::unit_name).unwrap_or_else(|| format!("unit #{id}"))
}

/// Name of a historical figure (deities, off-map figures) from the export's names.
pub fn hist_fig_name(g: &Game, hf: i32) -> String {
    g.sim.world.hist_figs.iter().find(|h| h.id == hf).map(|h| h.name.clone()).unwrap_or_else(|| format!("figure #{hf}"))
}

/// "Pen/pasture #12" or the building's name.
pub fn zone_label(g: &Game, id: i32) -> String {
    match g.sim.world.building(id) {
        Some(b) if !b.name.is_empty() => b.name.clone(),
        Some(b) => format!("{} #{}", zone_kind_name(&b.subtype), b.id),
        None => format!("zone #{id}"),
    }
}

/// DF v50 zone names.
pub fn zone_kind_name(sub: &str) -> String {
    match sub {
        "MeetingHall" => "Meeting area".into(),
        "Pen" | "Pasture" => "Pen/pasture".into(),
        "WaterSource" => "Water source".into(),
        "Dump" => "Garbage dump".into(),
        "SandCollection" => "Sand".into(),
        "FishingArea" => "Fishing".into(),
        "Pond" => "Pit/pond".into(),
        "ArcheryRange" => "Archery range".into(),
        "DiningHall" => "Dining hall".into(),
        other => super::words(other),
    }
}

// ------------------------------------------------------------------ hooks for game.rs

/// `--ui` states of round 8 (screenshots/tests). Returns false for unknown keys.
pub fn ui_arg(g: &mut Game, k: &str, v: &str) -> bool {
    match k {
        "workdetails" => {
            g.screen = Screen::Labor;
            g.r8.labor_tab = LaborTab::WorkDetails;
            if let Some(i) = v.parse::<usize>().ok().or_else(|| g.sim.world.work_details.iter().position(|d| d.name.eq_ignore_ascii_case(v))) {
                g.r8.detail = i;
            }
        }
        "wd-edit" => {
            g.screen = Screen::Labor;
            g.r8.labor_tab = LaborTab::WorkDetails;
            g.r8.wd_edit = true;
        }
        "wd-new" => {
            g.screen = Screen::Labor;
            g.r8.wd_new = Some(v.to_string());
        }
        "wd-defaults" => super::workdetails::create_defaults(g),
        "labor-grid" => {
            g.screen = Screen::Labor;
            g.r8.labor_tab = LaborTab::Advanced;
        }
        "kitchen" => {
            g.screen = Screen::Labor;
            g.r8.labor_tab = LaborTab::Kitchen;
        }
        "pile" => {
            // Stockpile settings editor for a pile id (or the largest pile).
            let w = &g.sim.world;
            let id = v.parse::<i32>().ok().or_else(|| {
                w.buildings.iter().filter(|b| b.kind == "Stockpile").max_by_key(|b| (b.x2 - b.x1 + 1) as i32 * (b.y2 - b.y1 + 1) as i32).map(|b| b.id)
            });
            if let Some(id) = id {
                open_pile(g, id);
            }
        }
        "pile-cat" => {
            g.r8.sp_cat = v.parse::<usize>().ok().or_else(|| crate::game::ALL_STOCK_CATEGORIES.iter().position(|c| *c == v)).unwrap_or(0);
            g.r8.sp_group = 0;
        }
        "pile-group" => g.r8.sp_group = v.parse().unwrap_or(0),
        "pile-link" => {
            if let Some(id) = g.r8.pile {
                g.tool = Tool::PileLink(id, v != "take");
                g.screen = Screen::None;
            }
        }
        "animals" => {
            g.screen = Screen::Animals;
            g.r8.an_tab = v.parse().unwrap_or(0);
        }
        "pasture-pick" => {
            g.screen = Screen::Animals;
            g.r8.an_pick = v.parse::<i32>().ok().or_else(|| g.sim.animals().first().map(|a| a.unit));
        }
        "places" => {
            g.screen = Screen::Places;
            g.r8.pl_tab = v.parse().unwrap_or(0);
        }
        "make-location" => {
            g.screen = Screen::Places;
            g.r8.pl_tab = 1;
            g.r8.pl_make = v.parse::<i32>().ok().or_else(|| g.sim.world.buildings.iter().find(|b| b.kind == "Civzone" && b.subtype == "MeetingHall").map(|b| b.id));
        }
        "temple-pick" => {
            g.screen = Screen::Places;
            g.r8.pl_tab = 1;
            g.r8.pl_make = g.sim.world.buildings.iter().find(|b| b.kind == "Civzone" && b.subtype == "MeetingHall").map(|b| b.id);
            g.r8.pl_kind = Some("Temple");
        }
        "occupation-pick" => {
            g.screen = Screen::Places;
            g.r8.pl_tab = 0;
            g.r8.pl_occ = g.sim.world.locations.first().map(|l| (l.id, super::places::occupations(&l.kind).first().copied().unwrap_or("PERFORMER").to_string()));
        }
        "fake-locations" => super::places::debug_locations(g),
        "justice" => {
            g.screen = Screen::Justice;
            g.r8.j_tab = v.parse().unwrap_or(0);
        }
        "fake-crimes" => super::places::debug_crimes(g),
        "convict" => {
            g.screen = Screen::Justice;
            g.r8.j_convict = v.parse::<i32>().ok().or_else(|| g.sim.world.crimes.iter().find(|c| c.convicted < 0).map(|c| c.id));
        }
        "objects" => g.screen = Screen::Objects,
        "world" => {
            g.screen = Screen::World;
            g.r8.w_tab = v.parse().unwrap_or(0);
        }
        "minimap" => g.r8.minimap = v != "off",
        _ => return false,
    }
    true
}

/// Open the stockpile settings editor for `id`.
pub fn open_pile(g: &mut Game, id: i32) {
    g.r8.pile = Some(id);
    g.r8.sp_scroll = 0;
    g.r8.sp_cache.clear();
    g.screen = Screen::StockpileSettings;
    g.tool = Tool::None;
}

/// Esc / right-click backs out of round-8 pickers first. Returns true when it did something.
pub fn escape(g: &mut Game) -> bool {
    let r8 = &mut g.r8;
    if r8.wd_new.is_some() {
        r8.wd_new = None;
    } else if r8.an_pick.is_some() {
        r8.an_pick = None;
    } else if r8.pl_kind.is_some() {
        r8.pl_kind = None;
    } else if r8.pl_make.is_some() {
        r8.pl_make = None;
    } else if r8.pl_occ.is_some() {
        r8.pl_occ = None;
    } else if r8.j_convict.is_some() {
        r8.j_convict = None;
    } else if r8.wd_edit && g.screen == Screen::Labor {
        r8.wd_edit = false;
    } else if let Tool::PileLink(id, _) = g.tool {
        // Back to the stockpile editor, like DF's link mode.
        open_pile(g, id);
    } else {
        return false;
    }
    true
}

/// A text field has focus (hotkeys off).
pub fn typing(g: &Game) -> bool {
    g.screen == Screen::Labor && g.r8.wd_new.is_some()
}

/// The keys of the round-8 screens (DF's `D_LOCATIONS`, `D_JUSTICE`, `D_ARTLIST`, `D_WORLD`).
pub fn global_keys(g: &mut Game) -> bool {
    let (b, inp) = (&g.binds, &g.input);
    let s = if inp.hit(b, "D_LOCATIONS") {
        Screen::Places
    } else if inp.hit(b, "D_JUSTICE") {
        Screen::Justice
    } else if inp.hit(b, "D_ARTLIST") {
        Screen::Objects
    } else if inp.hit(b, "D_WORLD") {
        Screen::World
    } else {
        return false;
    };
    g.toggle_screen(s);
    true
}

/// Draw the round-8 full-window screens (called from `overlay::draw_ui`).
pub fn draw_screens(g: &mut Game) {
    match g.screen {
        Screen::StockpileSettings => super::stockpile_ed::draw(g),
        Screen::Animals => super::animals::draw(g),
        Screen::Places => super::places::draw_places(g),
        Screen::Justice => super::places::draw_justice(g),
        Screen::Objects => super::places::draw_objects(g),
        Screen::World => super::places::draw_world(g),
        _ => {}
    }
}

/// Map click with the stockpile link tool: link the pile to the stockpile/workshop there.
pub fn pile_link_click(g: &mut Game, pile: i32, give: bool, p: Pos) {
    let target = crate::game::building_at(&g.sim.world, p, false);
    let Some(tid) = target.filter(|&t| t != pile) else {
        g.set_status("Click another stockpile or a workshop to link it. Right-click: back.".into());
        return;
    };
    let Some(tb) = g.sim.world.building(tid).cloned() else { return };
    if !matches!(tb.kind.as_str(), "Stockpile" | "Workshop" | "Furnace") {
        g.set_status(format!("{} cannot be linked (only stockpiles and workshops)", super::building::building_title(g, &tb)));
        return;
    }
    super::stockpile_ed::add_link(g, pile, tid, give);
    let what = super::building::building_title(g, &tb);
    g.set_status(format!("Stockpile #{pile} {} {what}", if give { "gives to" } else { "takes from" }));
}

/// Map overlay: the links of the selected stockpile (or the one in the editor / link tool).
pub fn draw_map_overlay(g: &mut Game) {
    let pile = match (g.tool, g.selected, g.screen) {
        (Tool::PileLink(id, _), _, _) => Some(id),
        (_, Selected::Building(id), _) => Some(id),
        _ => None,
    };
    let Some(pid) = pile else { return };
    let w = &g.sim.world;
    let Some(b) = w.building(pid) else { return };
    let Some(sp) = b.stockpile.as_ref() else { return };
    let z = g.cam.z as i16;
    let tp = g.cam.tile_px();
    let c0 = g.cam.to_screen(b.center.x as f32 + 0.5, b.center.y as f32 + 0.5);
    let links: Vec<(i32, bool)> = sp.give_to.iter().map(|&i| (i, true)).chain(sp.take_from.iter().map(|&i| (i, false))).collect();
    for (id, give) in links {
        let Some(o) = w.building(id) else { continue };
        let c = if give { Color::from_rgba(90, 220, 90, 230) } else { Color::from_rgba(240, 160, 60, 230) };
        let p1 = g.cam.to_screen(o.x1 as f32, o.y1 as f32);
        let p2 = g.cam.to_screen(o.x2 as f32 + 1.0, o.y2 as f32 + 1.0);
        if o.z == z {
            draw_rectangle_lines(p1.x, p1.y, p2.x - p1.x, p2.y - p1.y, (tp / 12.0).max(2.0), c);
        }
        if b.z == z {
            let c1 = g.cam.to_screen(o.center.x as f32 + 0.5, o.center.y as f32 + 0.5);
            draw_line(c0.x, c0.y, c1.x, c1.y, (tp / 16.0).max(1.5), c);
            if o.z != z {
                g.font.draw(&format!("z{}", o.z), c1.x + 4.0, c1.y - 6.0, (tp / 32.0).max(0.75), c);
            }
        }
    }
    // Link tool: outline linkable buildings on this level.
    if let Tool::PileLink(_, give) = g.tool {
        let hover = g.hover;
        for o in w.buildings.iter().filter(|o| o.z == z && o.id != pid && matches!(o.kind.as_str(), "Stockpile" | "Workshop" | "Furnace")) {
            let hov = hover.map(|h| o.contains(h)).unwrap_or(false);
            let c = if hov { Color::from_rgba(255, 230, 80, 255) } else if give { Color::from_rgba(90, 220, 90, 120) } else { Color::from_rgba(240, 160, 60, 120) };
            let p1 = g.cam.to_screen(o.x1 as f32, o.y1 as f32);
            let p2 = g.cam.to_screen(o.x2 as f32 + 1.0, o.y2 as f32 + 1.0);
            draw_rectangle_lines(p1.x, p1.y, p2.x - p1.x, p2.y - p1.y, if hov { 3.0 } else { 1.5 }, c);
        }
    }
}

/// Hint above the toolbar while the link tool is active.
pub fn link_hint(g: &Game) -> Option<String> {
    match g.tool {
        Tool::PileLink(id, give) => Some(format!(
            "Stockpile #{id}: click a stockpile or workshop it should {}. Right-click: back to the settings.",
            if give { "give to" } else { "take from" }
        )),
        _ => None,
    }
}

/// [app8] DF's kitchen default for Gordon's fort: plump helmets are brewed, never cooked.
/// Added once when the save has no kitchen setting for them.
pub fn apply_kitchen_default(g: &mut Game) {
    const PH: &str = "PLANT:MUSHROOM_HELMET_PLUMP:STRUCTURAL";
    if g.sim.world.kitchen.iter().any(|k| k.mat == PH && k.item_type == "PLANT") {
        return;
    }
    if !g.sim.world.materials.iter().any(|m| m.token == PH) {
        return;
    }
    g.sim.set_kitchen(rf_core::KitchenSetting { item_type: "PLANT".into(), mat: PH.into(), no_cook: true, ..Default::default() });
}
