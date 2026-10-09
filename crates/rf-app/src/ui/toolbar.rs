//! Bottom toolbar like the Steam version (info buttons left, designation/building tools in the
//! middle) plus the Dig and Stockpile submenus. Data-driven: add a row to a table to add a
//! button.

use super::{col, icon_button, panel, BtnState, Ico};
use crate::font::GH;
use crate::game::{Game, Submenu, Tool, STOCK_PRESETS};
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
    Build,
    Stockpile,
    Zone,
    Burrow,
    Hauling,
    Squads,
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
    tb(Action::Places, "Places (not yet)", "BUTTON_INFO_PLACES", Ico::Glyph(127, [200, 200, 200]), "", false),
    tb(Action::Labor, "Labor (not yet)", "BUTTON_INFO_LABOR", Ico::Hammer, "", false),
    tb(Action::WorkOrders, "Work orders (not yet)", "BUTTON_INFO_WORK_ORDERS", Ico::Glyph(240, [220, 200, 150]), "", false),
    tb(Action::Nobles, "Nobles (not yet)", "BUTTON_INFO_NOBLES", Ico::Glyph(157, [240, 210, 90]), "", false),
    tb(Action::Objects, "Objects (not yet)", "BUTTON_INFO_OBJECTS", Ico::Glyph(254, [180, 160, 120]), "", false),
    tb(Action::Justice, "Justice (not yet)", "BUTTON_INFO_JUSTICE", Ico::Glyph(241, [200, 200, 220]), "", false),
];

pub const CENTER: &[ToolButton] = &[
    tb(Action::Dig, "Dig", "BUTTON_DIG", Ico::Pick, "D_DESIGNATE_DIG", true),
    tb(Action::Chop, "Chop down trees", "BUTTON_DES_CHOP", Ico::Axe, "D_DESIGNATE_CHOP", true),
    tb(Action::Gather, "Gather plants", "BUTTON_DES_GATHER", Ico::Plant, "D_DESIGNATE_GATHER", true),
    tb(Action::Smooth, "Smooth stone (not yet)", "BUTTON_DES_SMOOTH", Ico::Smooth, "D_DESIGNATE_SMOOTH", false),
    tb(Action::Erase, "Erase designations", "BUTTON_DES_ERASE", Ico::Eraser, "D_DESIGNATE_ERASE", true),
    tb(Action::Build, "Build (not yet)", "BUTTON_BUILDING", Ico::Hammer, "D_BUILDJOB", false),
    tb(Action::Stockpile, "Stockpiles", "BUTTON_STOCKPILE", Ico::Crate, "D_STOCKPILES", true),
    tb(Action::Zone, "Zones (not yet)", "BUTTON_ZONE", Ico::Zone, "D_CIVZONE", false),
    tb(Action::Burrow, "Burrows (not yet)", "BUTTON_BURROW", Ico::Glyph(176, [200, 160, 120]), "D_BURROWS", false),
    tb(Action::Hauling, "Hauling (not yet)", "BUTTON_HAULING", Ico::Glyph(26, [200, 200, 200]), "", false),
];

pub const RIGHT: &[ToolButton] = &[
    tb(Action::Squads, "Squads (not yet)", "BUTTON_SQUADS", Ico::Glyph(24, [230, 100, 90]), "D_SQUADS", false),
    tb(Action::World, "World (not yet)", "BUTTON_WORLD", Ico::Glyph(9, [120, 180, 240]), "", false),
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
    if k.is_empty() {
        label.to_string()
    } else {
        format!("{label} ({k})")
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
        Submenu::None => {
            if let Tool::Designate(k) = g.tool {
                top = draw_hint(g, cx, y, &format!("{}: drag a rectangle. Right-click cancels.", kind_label(k)));
            }
        }
    }
    top
}

pub fn kind_label(k: DesignationKind) -> &'static str {
    match k {
        DesignationKind::Mine => "Dig",
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
    let w = row_w(n, s) + 2.0 * pad;
    let w = w.max(46.0 * 8.0 * s);
    let h = BTN_H * s + 2.0 * pad + GH * s + 6.0 * s;
    let r = Rect::new(x, toolbar_y - h - 4.0 * s, w, h);
    panel(&mut g.ui, r);
    let cur = match g.tool {
        Tool::Designate(k) => Some(k),
        _ => None,
    };
    let title = match cur {
        Some(k) => format!("Dig: {} - drag a box (z while dragging = 3D)", kind_label(k)),
        None => "Dig".to_string(),
    };
    g.font.draw(&super::clip(&title, ((w - 2.0 * pad) / (8.0 * s)) as usize), r.x + pad, r.y + pad, s, col::TEXT_HI);
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
