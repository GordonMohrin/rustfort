//! Bottom toolbar like the Steam version (info buttons left, designation/building tools in the
//! middle) plus the Dig and Stockpile submenus. Data-driven: add a row to a table to add a
//! button.

use super::{col, icon_button, panel, BtnState, Ico};
use crate::font::GH;
use crate::game::{Game, Screen, Submenu, Tool, STOCK_PRESETS};
use macroquad::prelude::*;
use rf_sim::DesignationKind;

pub const BTN_W: f32 = 32.0;
pub const BTN_H: f32 = 36.0;
pub const TOOLBAR_H: f32 = BTN_H + 12.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Citizens,
    Tasks,
    Places,
    Labor,
    WorkOrders,
    Nobles,
    Objects,
    Justice,
    Dig,
    Chop,
    Gather,
    Smooth,
    Erase,
    /// [round 5]
    Traffic,
    /// [round 5] Item designations (claim, forbid, dump, melt, hide).
    Items,
    Build,
    Stockpile,
    Zone,
    Burrow,
    Hauling,
    Squads,
    Reports,
    World,
}

pub struct ToolButton {
    pub action: Action,
    pub label: &'static str,
    /// DF interface graphics token (vanilla_interface).
    pub icon: &'static str,
    pub fallback: Ico,
    /// DF keybinding name (for the tooltip label).
    pub bind: &'static str,
    pub enabled: bool,
}

const fn tb(action: Action, label: &'static str, icon: &'static str, fallback: Ico, bind: &'static str, enabled: bool) -> ToolButton {
    ToolButton { action, label, icon, fallback, bind, enabled }
}

pub const LEFT: &[ToolButton] = &[
    tb(Action::Citizens, "Citizens", "BUTTON_INFO_CREATURES", Ico::Smiley, "D_UNITLIST", true),
    tb(Action::Tasks, "Tasks (not yet)", "BUTTON_INFO_TASKS", Ico::Glyph(19, [200, 200, 200]), "D_JOBLIST", false),
    tb(Action::Places, "Places: locations, zones, stockpiles, workshops", "BUTTON_INFO_PLACES", Ico::Glyph(127, [200, 200, 200]), "D_LOCATIONS", true), // [app8]
    tb(Action::Labor, "Labor: work details, kitchen", "BUTTON_INFO_LABOR", Ico::Hammer, "D_LABOR", true),
    tb(Action::WorkOrders, "Work orders", "BUTTON_INFO_WORK_ORDERS", Ico::Glyph(240, [220, 200, 150]), "D_ORDERS", true),
    tb(Action::Nobles, "Nobles and administrators", "BUTTON_INFO_NOBLES", Ico::Glyph(157, [240, 210, 90]), "D_NOBLES", true),
    tb(Action::Objects, "Objects: artifacts", "BUTTON_INFO_OBJECTS", Ico::Glyph(254, [180, 160, 120]), "D_ARTLIST", true), // [app8]
    tb(Action::Justice, "Justice", "BUTTON_INFO_JUSTICE", Ico::Glyph(241, [200, 200, 220]), "D_JUSTICE", true), // [app8]
];

pub const CENTER: &[ToolButton] = &[
    tb(Action::Dig, "Dig", "BUTTON_DIG", Ico::Pick, "D_DESIGNATE_DIG", true),
    tb(Action::Chop, "Chop down trees", "BUTTON_DES_CHOP", Ico::Axe, "D_DESIGNATE_CHOP", true),
    tb(Action::Gather, "Gather plants", "BUTTON_DES_GATHER", Ico::Plant, "D_DESIGNATE_GATHER", true),
    tb(Action::Smooth, "Smooth, engrave, carve", "BUTTON_DES_SMOOTH", Ico::Smooth, "D_DESIGNATE_SMOOTH", true),
    tb(Action::Erase, "Erase designations", "BUTTON_DES_ERASE", Ico::Eraser, "D_DESIGNATE_ERASE", true),
    tb(Action::Traffic, "Traffic", "BUTTON_DES_TRAFFIC", Ico::Glyph(b'T', [120, 220, 120]), "D_DESIGNATE_TRAFFIC", true),
    tb(Action::Items, "Item designations: claim, forbid, dump, melt, hide", "BUTTON_DES_ITEM_BUILDING", Ico::Glyph(b'X', [230, 90, 80]), "D_DESIGNATE_ITEMS", true),
    tb(Action::Build, "Build", "BUTTON_BUILDING", Ico::Hammer, "D_BUILDING", true),
    tb(Action::Stockpile, "Stockpiles", "BUTTON_STOCKPILE", Ico::Crate, "D_STOCKPILES", true),
    tb(Action::Zone, "Zones", "BUTTON_ZONE", Ico::Zone, "D_CIVZONE", true),
    tb(Action::Burrow, "Burrows", "BUTTON_BURROW", Ico::Glyph(176, [200, 160, 120]), "D_BURROWS", true),
    tb(Action::Hauling, "Hauling routes (not yet)", "BUTTON_HAULING", Ico::Glyph(26, [200, 200, 200]), "D_HAULING", false),
];

pub const RIGHT: &[ToolButton] = &[
    tb(Action::Squads, "Squads", "BUTTON_SQUADS", Ico::Glyph(24, [230, 100, 90]), "D_SQUADS", true),
    tb(Action::Reports, "Reports", "", Ico::Glyph(19, [240, 200, 90]), "D_ANNOUNCE", true),
    tb(Action::World, "World: civilizations, caravans", "BUTTON_WORLD", Ico::Glyph(9, [120, 180, 240]), "D_WORLD", true), // [app8]
];

pub struct DigEntry {
    pub kind: DesignationKind,
    pub label: &'static str,
    pub icon: &'static str,
    pub fallback: Ico,
    pub bind: &'static str,
}

pub const DIG_MENU: &[DigEntry] = &[
    DigEntry { kind: DesignationKind::Mine, label: "Dig (mine)", icon: "BUTTON_DIG_DIG", fallback: Ico::Pick, bind: "DESIGNATE_DIG" },
    DigEntry { kind: DesignationKind::UpDownStair, label: "Up/down stairs", icon: "BUTTON_DIG_STAIRS", fallback: Ico::Glyph(b'X', [230, 230, 230]), bind: "DESIGNATE_STAIR_UPDOWN" },
    DigEntry { kind: DesignationKind::UpStair, label: "Up stair", icon: "", fallback: Ico::Glyph(b'<', [230, 230, 230]), bind: "DESIGNATE_STAIR_UP" },
    DigEntry { kind: DesignationKind::DownStair, label: "Down stair", icon: "", fallback: Ico::Glyph(b'>', [230, 230, 230]), bind: "DESIGNATE_STAIR_DOWN" },
    DigEntry { kind: DesignationKind::Ramp, label: "Ramp", icon: "BUTTON_DIG_RAMP", fallback: Ico::Glyph(30, [230, 230, 230]), bind: "DESIGNATE_RAMP" },
    DigEntry { kind: DesignationKind::Channel, label: "Channel", icon: "BUTTON_DIG_CHANNEL", fallback: Ico::Glyph(31, [230, 230, 230]), bind: "DESIGNATE_CHANNEL" },
    DigEntry { kind: DesignationKind::Remove, label: "Remove designation", icon: "BUTTON_DES_ERASE", fallback: Ico::Eraser, bind: "DESIGNATE_DIG_REMOVE_STAIRS_RAMPS" },
];

fn tip(g: &Game, label: &str, bind: &str) -> String {
    let k = if bind.is_empty() { String::new() } else { g.binds.label(bind) };
    // [app9] Second line: the DF key binding name, like DF's own key hints.
    match (k.is_empty(), bind.is_empty()) {
        (false, _) => format!("{label} ({k})
DF key {bind}"),
        (true, false) => format!("{label}
DF key {bind} (not bound)"),
        _ => label.to_string(),
    }
}

fn is_active(g: &Game, a: Action) -> bool {
    match a {
        Action::Citizens => g.citizens_open,
        Action::Dig => g.submenu == Submenu::Dig,
        Action::Chop => g.tool == Tool::Designate(DesignationKind::ChopTrees),
        Action::Gather => g.tool == Tool::Designate(DesignationKind::GatherPlants),
        Action::Erase => g.tool == Tool::Designate(DesignationKind::Remove) && g.submenu == Submenu::None,
        Action::Stockpile => g.submenu == Submenu::Stockpile,
        Action::Build => g.submenu == Submenu::Build,
        Action::Zone => g.submenu == Submenu::Zone,
        Action::Labor => g.screen == Screen::Labor,
        Action::WorkOrders => g.screen == Screen::WorkOrders,
        Action::Burrow => g.submenu == Submenu::Burrow,
        Action::Squads => g.screen == Screen::Military || matches!(g.tool, Tool::SquadStation(_) | Tool::SquadKill(_)),
        Action::Reports => g.screen == Screen::Reports,
        Action::Nobles => g.screen == Screen::Nobles,
        Action::Smooth => g.submenu == Submenu::Smooth,
        Action::Traffic => g.submenu == Submenu::Traffic,
        Action::Items => g.submenu == Submenu::Items,
        // [app8]
        Action::Places => g.screen == Screen::Places,
        Action::Objects => g.screen == Screen::Objects,
        Action::Justice => g.screen == Screen::Justice,
        Action::World => g.screen == Screen::World,
        _ => false,
    }
}

fn press(g: &mut Game, a: Action) {
    let tool = |g: &mut Game, k: DesignationKind| {
        g.submenu = Submenu::None;
        g.drag = None;
        g.tool = if g.tool == Tool::Designate(k) { Tool::None } else { Tool::Designate(k) };
    };
    match a {
        Action::Citizens => g.citizens_open = !g.citizens_open,
        Action::Dig => g.open_dig(),
        Action::Chop => tool(g, DesignationKind::ChopTrees),
        Action::Gather => tool(g, DesignationKind::GatherPlants),
        Action::Erase => tool(g, DesignationKind::Remove),
        Action::Stockpile => g.open_stockpiles(),
        Action::Build => g.open_build(),
        Action::Zone => g.open_zones(),
        Action::Labor => g.toggle_screen(Screen::Labor),
        Action::WorkOrders => g.toggle_screen(Screen::WorkOrders),
        Action::Burrow => g.open_burrows(),
        Action::Squads => {
            if matches!(g.tool, Tool::SquadStation(_) | Tool::SquadKill(_)) {
                g.tool = Tool::None;
            }
            g.toggle_screen(Screen::Military)
        }
        Action::Reports => g.toggle_screen(Screen::Reports),
        Action::Nobles => g.toggle_screen(Screen::Nobles),
        Action::Smooth => super::round5::open_smooth(g),
        Action::Traffic => super::round5::open_traffic(g),
        Action::Items => super::round5::open_items(g),
        // [app8]
        Action::Places => g.toggle_screen(Screen::Places),
        Action::Objects => g.toggle_screen(Screen::Objects),
        Action::Justice => g.toggle_screen(Screen::Justice),
        Action::World => g.toggle_screen(Screen::World),
        _ => {}
    }
}

fn button_row(g: &mut Game, buttons: &[ToolButton], x0: f32, y: f32, s: f32) -> f32 {
    let mut x = x0;
    for b in buttons {
        let r = Rect::new(x, y, BTN_W * s, BTN_H * s);
        let st = if !b.enabled {
            BtnState::Disabled
        } else if is_active(g, b.action) {
            BtnState::Active
        } else {
            BtnState::Normal
        };
        let t = tip(g, b.label, b.bind);
        if icon_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, r, b.icon, b.fallback, st, &t) {
            press(g, b.action);
        }
        x += (BTN_W + 4.0) * s;
    }
    x
}

fn row_w(n: usize, s: f32) -> f32 {
    n as f32 * (BTN_W + 4.0) * s - 4.0 * s
}

/// Draws the toolbar; returns its top y (for panels above it).
pub fn draw_toolbar(g: &mut Game) -> f32 {
    let s = g.ui.scale;
    let (sw, sh) = (screen_width(), screen_height());
    let h = TOOLBAR_H * s;
    let y = sh - h;
    let pad = 6.0 * s;
    // Left: info buttons.
    let lw = row_w(LEFT.len(), s) + 2.0 * pad;
    panel(&mut g.ui, Rect::new(0.0, y, lw, h));
    button_row(g, LEFT, pad, y + pad, s);
    // Middle: designations etc.
    let cw = row_w(CENTER.len(), s) + 2.0 * pad;
    let cx = ((sw - cw) / 2.0).max(lw + 8.0 * s);
    panel(&mut g.ui, Rect::new(cx, y, cw, h));
    button_row(g, CENTER, cx + pad, y + pad, s);
    // Right.
    let rw = row_w(RIGHT.len(), s) + 2.0 * pad;
    let rx = sw - rw;
    if rx > cx + cw + 8.0 * s {
        panel(&mut g.ui, Rect::new(rx, y, rw, h));
        button_row(g, RIGHT, rx + pad, y + pad, s);
    }
    let mut top = y;
    // Submenus open above the middle toolbar.
    match g.submenu {
        Submenu::Dig => top = draw_dig_menu(g, cx, y),
        Submenu::Stockpile => top = draw_stock_menu(g, cx, y),
        Submenu::Build => top = super::build::draw_build_menu(g, y),
        Submenu::Zone => top = super::build::draw_zone_menu(g, y),
        Submenu::Burrow => top = super::burrows::draw_burrow_menu(g, y),
        Submenu::Smooth | Submenu::Traffic | Submenu::Items => top = super::round5::draw_submenu(g, cx, y),
        Submenu::None => match g.tool {
            Tool::Designate(k) => {
                top = draw_hint(g, cx, y, &format!("{}: drag a rectangle. Right-click cancels.", kind_label(k)));
            }
            Tool::SquadStation(sq) => {
                let n = super::military::squad_name(&g.sim.world, sq);
                top = draw_hint(g, cx, y, &format!("Station {n}: click a tile. Right-click: back to the squads."));
            }
            Tool::SquadKill(sq) => {
                let n = super::military::squad_name(&g.sim.world, sq);
                top = draw_hint(g, cx, y, &format!("{n}: click a creature to kill. Right-click: back to the squads."));
            }
            Tool::PileLink(..) => {
                let h = super::round8::link_hint(g).unwrap_or_default(); // [app8]
                top = draw_hint(g, cx, y, &h);
            }
            Tool::Patrol(sq) => {
                let h = super::round9::patrol_hint(g, sq); // [app9]
                top = draw_hint(g, cx, y, &h);
            }
            Tool::Link(_) => {
                let n = super::devices::mechanisms(&g.sim.world);
                top = draw_hint(g, cx, y, &format!("Link lever: click a door, bridge, floodgate, hatch, cage or grate ({n} mechanisms). Right-click cancels."));
            }
            _ => {}
        },
    }
    top
}

pub fn kind_label(k: DesignationKind) -> &'static str {
    match k {
        DesignationKind::Mine => "Mine",
        DesignationKind::Channel => "Channel",
        DesignationKind::UpDownStair => "Up/down stairs",
        DesignationKind::UpStair => "Up stair",
        DesignationKind::DownStair => "Down stair",
        DesignationKind::Ramp => "Ramp",
        DesignationKind::ChopTrees => "Chop down trees",
        DesignationKind::GatherPlants => "Gather plants",
        DesignationKind::Remove => "Remove designation",
    }
}

fn draw_hint(g: &mut Game, x: f32, toolbar_y: f32, text: &str) -> f32 {
    let s = g.ui.scale;
    let w = g.font.width(text, s) + 16.0 * s;
    let h = GH * s + 12.0 * s;
    let r = Rect::new(x, toolbar_y - h - 4.0 * s, w, h);
    panel(&mut g.ui, r);
    g.font.draw(text, r.x + 8.0 * s, r.y + 6.0 * s, s, col::TEXT);
    r.y
}

fn draw_dig_menu(g: &mut Game, x: f32, toolbar_y: f32) -> f32 {
    let s = g.ui.scale;
    let pad = 6.0 * s;
    let n = DIG_MENU.len();
    let cur = match g.tool {
        Tool::Designate(k) => Some(k),
        _ => None,
    };
    let title = match cur {
        Some(k) => format!("Dig: {} - drag a box (change z while dragging = 3D)", kind_label(k)),
        None => "Dig".to_string(),
    };
    let w = (row_w(n, s) + 2.0 * pad).max(g.font.width(&title, s) + 2.0 * pad);
    let h = BTN_H * s + 2.0 * pad + GH * s + 6.0 * s;
    let r = Rect::new(x, toolbar_y - h - 4.0 * s, w, h);
    panel(&mut g.ui, r);
    g.font.draw(&title, r.x + pad, r.y + pad, s, col::TEXT_HI);
    let mut bx = r.x + pad;
    let by = r.y + pad + GH * s + 6.0 * s;
    for d in DIG_MENU {
        let rr = Rect::new(bx, by, BTN_W * s, BTN_H * s);
        let st = if cur == Some(d.kind) { BtnState::Active } else { BtnState::Normal };
        let t = tip(g, d.label, d.bind);
        if icon_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, rr, d.icon, d.fallback, st, &t) {
            g.tool = Tool::Designate(d.kind);
            g.drag = None;
        }
        bx += (BTN_W + 4.0) * s;
    }
    r.y
}

fn draw_stock_menu(g: &mut Game, x: f32, toolbar_y: f32) -> f32 {
    let s = g.ui.scale;
    let pad = 6.0 * s;
    let n = STOCK_PRESETS.len();
    let w = (row_w(n, s) + 2.0 * pad).max(46.0 * 8.0 * s);
    let h = BTN_H * s + 2.0 * pad + GH * s + 6.0 * s;
    let r = Rect::new(x, toolbar_y - h - 4.0 * s, w, h);
    panel(&mut g.ui, r);
    let cur = match g.tool {
        Tool::Stockpile(i) => Some(i),
        _ => None,
    };
    let title = match cur {
        Some(i) => format!("Stockpile: {} - drag a rectangle", STOCK_PRESETS[i].label),
        None => "Stockpile".into(),
    };
    g.font.draw(&title, r.x + pad, r.y + pad, s, col::TEXT_HI);
    let mut bx = r.x + pad;
    let by = r.y + pad + GH * s + 6.0 * s;
    for (i, p) in STOCK_PRESETS.iter().enumerate() {
        let rr = Rect::new(bx, by, BTN_W * s, BTN_H * s);
        let st = if cur == Some(i) { BtnState::Active } else { BtnState::Normal };
        if icon_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, rr, p.icon, Ico::Glyph(p.glyph, p.color), st, p.label) {
            g.tool = Tool::Stockpile(i);
            g.drag = None;
        }
        bx += (BTN_W + 4.0) * s;
    }
    r.y
}
