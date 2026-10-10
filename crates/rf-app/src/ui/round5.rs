//! [round 5] Constructions (Build > Constructions), stone detailing (Smooth / Engrave / Carve),
//! traffic and item designations with their DF Premium map markers, planned constructions in
//! DF's blueprint look, caged creatures drawn inside their cage, and the mood and treatment
//! readouts of the unit sheet.
//!
//! The sim side is `rf_sim::round5_api` (`build_construction`, `designate_detail`,
//! `set_traffic`, `set_item_flag(s_in)`, `mood_info`).
//!
//! Planned constructions are read from `World::buildings` (kind `Construction`, not built,
//! subtype = DF construction type such as `Wall`, `UpStair`, `TrackNS`), like DF's
//! `building_constructionst`. Smoothing and engraving designations come from the tile flags
//! (`DESIG_SMOOTH`, `DESIG_ENGRAVE`); carve designations (fortification, track) have no tile
//! flag in rf-core, so the UI remembers where it placed them until the tile changes.

use super::{clip, col, icon_button, panel, text_button, BtnState, Ico};
use crate::font::GH;
use crate::game::{Game, Selected, Submenu, Tool};
use crate::render::{tile_rect, MapRenderer};
use macroquad::prelude::*;
use rf_core::{tile_flags, Building, Item, Pos, Shape, TtMaterial, TtSpecial, Unit, World, NO_MAT};
use rf_gfx::{Gfx, Sprite};
use rf_sim::{ConstructionKind, DetailKind, Traffic};
use std::collections::HashMap;

/// DF item designations (`DESIGNATE_CLAIM`, `_UNCLAIM` (forbid), `_DUMP`, `_MELT`, `_HIDE`
/// and their removals).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ItemDes {
    Claim,
    Forbid,
    Dump,
    Undump,
    Melt,
    Unmelt,
    Hide,
    Unhide,
}

impl ItemDes {
    /// (`Sim::set_item_flag` flag, on).
    pub fn flag(self) -> (&'static str, bool) {
        match self {
            ItemDes::Claim => ("forbid", false),
            ItemDes::Forbid => ("forbid", true),
            ItemDes::Dump => ("dump", true),
            ItemDes::Undump => ("dump", false),
            ItemDes::Melt => ("melt", true),
            ItemDes::Unmelt => ("melt", false),
            ItemDes::Hide => ("hidden", true),
            ItemDes::Unhide => ("hidden", false),
        }
    }
}

/// One entry of a designation submenu.
pub struct Entry<K: 'static> {
    pub kind: K,
    pub label: &'static str,
    /// DF interface token (`_ACTIVE`/`_INACTIVE` tried first).
    pub icon: &'static str,
    pub fallback: Ico,
    /// DF keybinding inside the submenu.
    pub bind: &'static str,
    /// Tooltip detail (what it does in DF).
    pub help: &'static str,
}

/// Build > Constructions, in DF's order.
pub struct ConstrEntry {
    pub kind: ConstructionKind,
    pub label: &'static str,
    pub icon: &'static str,
    pub needs: &'static str,
}

/// Any building material: DF constructions take one boulder, block, log or bar.
pub const CONSTR_MAT: &str = "1 building material (boulder, block, log or bar)";

pub const CONSTRUCTIONS: &[ConstrEntry] = &[
    ConstrEntry { kind: ConstructionKind::Wall, label: "Wall", icon: "BUILDING_ICON_WALL", needs: CONSTR_MAT },
    ConstrEntry { kind: ConstructionKind::Floor, label: "Floor", icon: "BUILDING_ICON_FLOOR", needs: CONSTR_MAT },
    ConstrEntry { kind: ConstructionKind::Ramp, label: "Ramp", icon: "BUILDING_ICON_RAMP", needs: CONSTR_MAT },
    ConstrEntry { kind: ConstructionKind::UpStair, label: "Up stair", icon: "BUILDING_ICON_STAIRS", needs: CONSTR_MAT },
    ConstrEntry { kind: ConstructionKind::DownStair, label: "Down stair", icon: "BUILDING_ICON_STAIRS", needs: CONSTR_MAT },
    ConstrEntry { kind: ConstructionKind::UpDownStair, label: "Up/down stair", icon: "BUILDING_ICON_STAIRS", needs: CONSTR_MAT },
    ConstrEntry { kind: ConstructionKind::Fortification, label: "Fortification", icon: "BUILDING_ICON_FORTIFICATION", needs: CONSTR_MAT },
    ConstrEntry { kind: ConstructionKind::Track, label: "Track", icon: "BUILDING_ICON_TRACK", needs: CONSTR_MAT },
    ConstrEntry { kind: ConstructionKind::Remove, label: "Remove construction", icon: "BUTTON_DES_ERASE", needs: "nothing (the material is returned)" },
];

/// The build-menu category that holds [`CONSTRUCTIONS`].
pub const CONSTR_CAT: &str = "Constructions";

pub const DETAIL_MENU: &[Entry<DetailKind>] = &[
    Entry { kind: DetailKind::Smooth, label: "Smooth stone", icon: "BUTTON_DES_SMOOTH_SMOOTH", fallback: Ico::Smooth, bind: "DESIGNATE_SMOOTH", help: "Engravers smooth natural walls and floors" },
    Entry { kind: DetailKind::Engrave, label: "Engrave", icon: "BUTTON_DES_SMOOTH_ENGRAVE", fallback: Ico::Glyph(b'*', [230, 220, 150]), bind: "DESIGNATE_ENGRAVE", help: "Engrave smoothed stone (makes rooms more valuable)" },
    Entry { kind: DetailKind::CarveTrack, label: "Carve track", icon: "BUTTON_DES_SMOOTH_TRACK", fallback: Ico::Glyph(205, [200, 200, 210]), bind: "DESIGNATE_TRACK", help: "Carve minecart track into a stone floor" },
    Entry { kind: DetailKind::CarveFortification, label: "Carve fortifications", icon: "BUTTON_DES_SMOOTH_FORTIFY", fallback: Ico::Glyph(206, [200, 200, 210]), bind: "DESIGNATE_FORTIFY", help: "Carve arrow slits into a smoothed natural wall" },
    Entry { kind: DetailKind::Remove, label: "Remove detailing designation", icon: "BUTTON_DES_ERASE", fallback: Ico::Eraser, bind: "DESIGNATE_DIG_REMOVE_STAIRS_RAMPS", help: "Clear smooth/engrave/carve designations" },
];

pub const TRAFFIC_MENU: &[Entry<Traffic>] = &[
    Entry { kind: Traffic::High, label: "High traffic", icon: "BUTTON_DES_TRAFFIC_HIGH", fallback: Ico::Glyph(b'H', [90, 220, 90]), bind: "DESIGNATE_TRAFFIC_HIGH", help: "Path cost 1: dwarves prefer these tiles" },
    Entry { kind: Traffic::Normal, label: "Normal traffic", icon: "BUTTON_DES_TRAFFIC_NORMAL", fallback: Ico::Glyph(b'N', [120, 160, 240]), bind: "DESIGNATE_TRAFFIC_NORMAL", help: "Path cost 2 (the default)" },
    Entry { kind: Traffic::Low, label: "Low traffic", icon: "BUTTON_DES_TRAFFIC_LOW", fallback: Ico::Glyph(b'L', [230, 220, 80]), bind: "DESIGNATE_TRAFFIC_LOW", help: "Path cost 5: dwarves avoid these tiles" },
    Entry { kind: Traffic::Restricted, label: "Restricted traffic", icon: "BUTTON_DES_TRAFFIC_RESTRICTED", fallback: Ico::Glyph(b'R', [230, 80, 70]), bind: "DESIGNATE_TRAFFIC_RESTRICTED", help: "Path cost 25: only crossed when there is no other way" },
];

pub const ITEM_MENU: &[Entry<ItemDes>] = &[
    Entry { kind: ItemDes::Claim, label: "Claim items", icon: "BUTTON_DES_CLAIM", fallback: Ico::Glyph(b'+', [120, 220, 120]), bind: "DESIGNATE_CLAIM", help: "Remove the forbidden mark: dwarves may use and haul them" },
    Entry { kind: ItemDes::Forbid, label: "Forbid items", icon: "BUTTON_DES_FORBID", fallback: Ico::Glyph(b'X', [230, 80, 70]), bind: "DESIGNATE_UNCLAIM", help: "Dwarves leave forbidden items alone" },
    Entry { kind: ItemDes::Dump, label: "Dump items", icon: "BUTTON_DES_DUMP", fallback: Ico::Glyph(b'D', [200, 150, 90]), bind: "DESIGNATE_DUMP", help: "Haul them to a garbage dump zone" },
    Entry { kind: ItemDes::Undump, label: "Remove dump mark", icon: "BUTTON_DES_UNDUMP", fallback: Ico::Glyph(b'd', [200, 150, 90]), bind: "DESIGNATE_NO_DUMP", help: "" },
    Entry { kind: ItemDes::Melt, label: "Melt items", icon: "BUTTON_DES_MELT", fallback: Ico::Glyph(b'M', [240, 140, 60]), bind: "DESIGNATE_MELT", help: "Metal items: melt them down at a smelter" },
    Entry { kind: ItemDes::Unmelt, label: "Remove melt mark", icon: "BUTTON_DES_UNMELT", fallback: Ico::Glyph(b'm', [240, 140, 60]), bind: "DESIGNATE_NO_MELT", help: "" },
    Entry { kind: ItemDes::Hide, label: "Hide items", icon: "BUTTON_DES_HIDE", fallback: Ico::Glyph(b'h', [170, 170, 170]), bind: "DESIGNATE_HIDE", help: "Do not draw them on the map" },
    Entry { kind: ItemDes::Unhide, label: "Unhide items", icon: "BUTTON_DES_UNHIDE", fallback: Ico::Glyph(b'H', [220, 220, 220]), bind: "DESIGNATE_NO_HIDE", help: "" },
];

/// Round-5 UI state.
#[derive(Default)]
pub struct R5Ui {
    /// Carve designations placed this session (no tile flag in rf-core): tile -> kind.
    pub carve: HashMap<Pos, DetailKind>,
    /// Caged unit -> (its position, cage material, real time of the lookup).
    pub cages: HashMap<i32, (Pos, u16, f64)>,
}

/// DF Premium sprites used by the round-5 overlays (looked up once).
#[derive(Default)]
pub struct R5Sprites {
    pub smooth: Option<Sprite>,
    pub engrave: Option<Sprite>,
    pub fortify: Option<Sprite>,
    /// DESIGNATION_TRACK_* by connection mask (bit 0 N, 1 S, 2 W, 3 E).
    pub track: [Option<Sprite>; 16],
    /// Traffic: low, high, restricted (tile traffic bits 1, 2, 3).
    pub traffic: [Option<Sprite>; 4],
    pub forbidden: Option<Sprite>,
    pub dump: Option<Sprite>,
    pub forbidden_dump: Option<Sprite>,
    pub hidden: Option<Sprite>,
    pub remove_construction: Option<Sprite>,
    /// PLANNED_CONSTRUCTION_*: wall, floor, ramp, up, down, up/down, fortification,
    /// reinforced wall.
    pub planned: [Option<Sprite>; 8],
    /// PLANNED_CONSTRUCTION_TRACK_* by connection mask.
    pub planned_track: [Option<Sprite>; 16],
}

/// Track sprite suffix for a connection mask (N=1, S=2, W=4, E=8) in DF's naming order.
fn track_suffix(mask: u8) -> &'static str {
    const S: [&str; 16] = ["NSWE", "N", "S", "NS", "W", "NW", "SW", "NSW", "E", "NE", "SE", "NSE", "WE", "NWE", "SWE", "NSWE"];
    S[(mask & 15) as usize]
}

impl R5Sprites {
    pub fn load(gfx: &Gfx) -> R5Sprites {
        let n = |s: &str| gfx.named(s);
        let mut r = R5Sprites {
            smooth: n("DESIGNATION_SMOOTH"),
            engrave: n("DESIGNATION_ENGRAVE"),
            fortify: n("DESIGNATION_FORTIFY"),
            traffic: [None, n("DESIGNATION_TRAFFIC_LOW"), n("DESIGNATION_TRAFFIC_HIGH"), n("DESIGNATION_TRAFFIC_RESTRICTED")],
            forbidden: n("DESIGNATION_ITEM_FORBIDDEN"),
            dump: n("DESIGNATION_ITEM_DUMP"),
            forbidden_dump: n("DESIGNATION_ITEM_FORBIDDEN_DUMP"),
            hidden: n("DESIGNATION_ITEM_HIDDEN"),
            remove_construction: n("DESIGNATION_DIG_REMOVE_CONSTRUCTION"),
            ..Default::default()
        };
        for (i, k) in ["WALL", "FLOOR", "RAMP", "STAIR_UP", "STAIR_DOWN", "STAIR_UPDOWN", "FORTIFICATION", "REINFORCED_WALL"].iter().enumerate() {
            r.planned[i] = n(&format!("PLANNED_CONSTRUCTION_{k}"));
        }
        for m in 0..16u8 {
            let suf = track_suffix(m);
            r.track[m as usize] = n(&format!("DESIGNATION_TRACK_{suf}"));
            r.planned_track[m as usize] = n(&format!("PLANNED_CONSTRUCTION_TRACK_{suf}"));
        }
        r
    }
}

impl MapRenderer {
    /// [round 5] The round-5 overlay sprites (looked up on first use).
    pub fn r5(&mut self, gfx: &Gfx) -> &R5Sprites {
        self.r5.get_or_insert_with(|| R5Sprites::load(gfx))
    }
}

pub fn constr_label(k: ConstructionKind) -> &'static str {
    CONSTRUCTIONS.iter().find(|c| c.kind == k).map(|c| c.label).unwrap_or("Construction")
}

pub fn detail_label(k: DetailKind) -> &'static str {
    DETAIL_MENU.iter().find(|c| c.kind == k).map(|c| c.label).unwrap_or("Detail")
}

pub fn traffic_label(k: Traffic) -> &'static str {
    TRAFFIC_MENU.iter().find(|c| c.kind == k).map(|c| c.label).unwrap_or("Traffic")
}

pub fn item_des_label(k: ItemDes) -> &'static str {
    ITEM_MENU.iter().find(|c| c.kind == k).map(|c| c.label).unwrap_or("Items")
}

// ------------------------------------------------------------------ menus and tools

pub fn open_smooth(g: &mut Game) {
    toggle_submenu(g, Submenu::Smooth, Tool::Detail(DetailKind::Smooth));
}

pub fn open_traffic(g: &mut Game) {
    toggle_submenu(g, Submenu::Traffic, Tool::Traffic(Traffic::High));
}

pub fn open_items(g: &mut Game) {
    toggle_submenu(g, Submenu::Items, Tool::ItemDes(ItemDes::Forbid));
}

fn toggle_submenu(g: &mut Game, m: Submenu, first: Tool) {
    if g.submenu == m {
        g.submenu = Submenu::None;
        g.tool = Tool::None;
    } else {
        g.submenu = m;
        g.tool = first;
        g.screen = crate::game::Screen::None;
    }
    g.drag = None;
}

/// Submenu hotkeys (DF's `DESIGNATE_*` bindings). Returns true if one fired.
pub fn submenu_keys(g: &mut Game) -> bool {
    fn pick<K: Copy>(g: &Game, list: &[Entry<K>]) -> Option<K> {
        list.iter().find(|e| !e.bind.is_empty() && g.input.hit(&g.binds, e.bind)).map(|e| e.kind)
    }
    let t = match g.submenu {
        Submenu::Smooth => pick(g, DETAIL_MENU).map(Tool::Detail),
        Submenu::Traffic => pick(g, TRAFFIC_MENU).map(Tool::Traffic),
        Submenu::Items => pick(g, ITEM_MENU).map(Tool::ItemDes),
        _ => None,
    };
    if let Some(t) = t {
        g.tool = t;
        g.drag = None;
        return true;
    }
    false
}

/// Apply a finished rectangle of a round-5 tool. Returns true if the tool was one of ours.
pub fn apply(g: &mut Game, a: Pos, b: Pos) -> bool {
    let (dx, dy, dz) = ((a.x - b.x).abs() + 1, (a.y - b.y).abs() + 1, (a.z - b.z).abs() + 1);
    let area = dx as i32 * dy as i32 * dz as i32;
    match g.tool {
        Tool::Construct(k) => {
            // Constructions are placed on one level, like DF.
            let b2 = Pos::new(b.x, b.y, a.z);
            let n = g.sim.build_construction(k, a, b2);
            let tiles = dx as i32 * dy as i32;
            let msg = if n == 0 {
                format!("{}: nothing designated in {dx}x{dy} (needs a revealed, buildable spot)", constr_label(k))
            } else if k == ConstructionKind::Remove {
                format!("Remove construction: {n} of {tiles} tiles")
            } else {
                format!("{}: {n} of {tiles} tiles planned, needs {CONSTR_MAT} each", constr_label(k))
            };
            g.set_status(msg);
            g.build_check = None;
        }
        Tool::Detail(k) => {
            let n = g.sim.designate_detail(a, b, k);
            remember_carve(g, a, b, k);
            g.set_status(format!("{}: {n} of {area} tiles ({dx}x{dy}x{dz})", detail_label(k)));
        }
        Tool::Traffic(k) => {
            g.sim.set_traffic(a, b, k);
            g.set_status(format!("{}: {dx}x{dy}x{dz} = {area} tiles", traffic_label(k)));
        }
        Tool::ItemDes(k) => {
            let (flag, on) = k.flag();
            let n = g.sim.set_items_flag_in(a, b, flag, on);
            g.set_status(format!("{}: {n} item{} in {dx}x{dy}x{dz}", item_des_label(k), if n == 1 { "" } else { "s" }));
        }
        _ => return false,
    }
    true
}

/// Remember carve designations for the overlay (they have no tile flag in rf-core).
fn remember_carve(g: &mut Game, a: Pos, b: Pos, k: DetailKind) {
    let w = &g.sim.world;
    for z in a.z.min(b.z)..=a.z.max(b.z) {
        for y in a.y.min(b.y)..=a.y.max(b.y) {
            for x in a.x.min(b.x)..=a.x.max(b.x) {
                let p = Pos::new(x, y, z);
                match k {
                    DetailKind::CarveFortification | DetailKind::CarveTrack => {
                        let Some(t) = w.tile(p) else { continue };
                        let tt = w.tiletypes.get(t.tt);
                        let ok = !t.hidden()
                            && tt.material.is_diggable()
                            && match k {
                                DetailKind::CarveFortification => tt.shape == Shape::Wall,
                                _ => tt.shape == Shape::Floor,
                            };
                        if ok {
                            g.r5.carve.insert(p, k);
                        }
                    }
                    _ => {
                        g.r5.carve.remove(&p);
                    }
                }
            }
        }
    }
}

/// Carve marks whose tile changed (carved, dug or built over) are dropped.
fn prune_carve(g: &mut Game) {
    if g.r5.carve.is_empty() {
        return;
    }
    let w = &g.sim.world;
    g.r5.carve.retain(|p, k| {
        let Some(t) = w.tile(*p) else { return false };
        let tt = w.tiletypes.get(t.tt);
        match k {
            DetailKind::CarveFortification => tt.shape == Shape::Wall,
            _ => tt.shape == Shape::Floor && tt.special != TtSpecial::Track,
        }
    });
}

/// Draw an icon menu row above the toolbar; returns (top y, clicked index).
fn icon_menu<K: Copy + PartialEq>(g: &mut Game, x: f32, toolbar_y: f32, title: &str, list: &[Entry<K>], cur: Option<K>) -> (f32, Option<usize>) {
    let s = g.ui.scale;
    let pad = 6.0 * s;
    let bw = super::toolbar::BTN_W * s;
    let bh = super::toolbar::BTN_H * s;
    let n = list.len();
    let roww = n as f32 * (bw + 4.0 * s) - 4.0 * s;
    let w = (roww + 2.0 * pad).max(g.font.width(title, s) + 2.0 * pad);
    let h = bh + 2.0 * pad + GH * s + 6.0 * s;
    let r = Rect::new(x.min(screen_width() - w - 4.0).max(4.0), toolbar_y - h - 4.0 * s, w, h);
    panel(&mut g.ui, r);
    g.font.draw(title, r.x + pad, r.y + pad, s, col::TEXT_HI);
    let mut bx = r.x + pad;
    let by = r.y + pad + GH * s + 6.0 * s;
    let mut clicked = None;
    for (i, e) in list.iter().enumerate() {
        let rr = Rect::new(bx, by, bw, bh);
        let st = if cur == Some(e.kind) { BtnState::Active } else { BtnState::Normal };
        let key = if e.bind.is_empty() { String::new() } else { g.binds.label(e.bind) };
        let mut tip = if key.is_empty() { e.label.to_string() } else { format!("{} ({key})", e.label) };
        if !e.help.is_empty() {
            tip.push('\n');
            tip.push_str(e.help);
        }
        if icon_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, rr, e.icon, e.fallback, st, &tip) {
            clicked = Some(i);
        }
        bx += bw + 4.0 * s;
    }
    (r.y, clicked)
}

/// The Smooth / Traffic / Items submenu above the toolbar. Returns its top y.
pub fn draw_submenu(g: &mut Game, x: f32, toolbar_y: f32) -> f32 {
    match g.submenu {
        Submenu::Smooth => {
            let cur = if let Tool::Detail(k) = g.tool { Some(k) } else { None };
            let title = match cur {
                Some(k) => format!("{}: drag a box (change z while dragging = 3D)", detail_label(k)),
                None => "Smooth / engrave / carve".to_string(),
            };
            let (t, pick) = icon_menu(g, x, toolbar_y, &title, DETAIL_MENU, cur);
            if let Some(i) = pick {
                g.tool = Tool::Detail(DETAIL_MENU[i].kind);
                g.drag = None;
            }
            t
        }
        Submenu::Traffic => {
            let cur = if let Tool::Traffic(k) = g.tool { Some(k) } else { None };
            let title = match cur {
                Some(k) => format!("{}: drag a box. Colors: green high, yellow low, red restricted", traffic_label(k)),
                None => "Traffic".to_string(),
            };
            let (t, pick) = icon_menu(g, x, toolbar_y, &title, TRAFFIC_MENU, cur);
            if let Some(i) = pick {
                g.tool = Tool::Traffic(TRAFFIC_MENU[i].kind);
                g.drag = None;
            }
            t
        }
        Submenu::Items => {
            let cur = if let Tool::ItemDes(k) = g.tool { Some(k) } else { None };
            let title = match cur {
                Some(k) => format!("{}: drag a box over the items", item_des_label(k)),
                None => "Item designations".to_string(),
            };
            let (t, pick) = icon_menu(g, x, toolbar_y, &title, ITEM_MENU, cur);
            if let Some(i) = pick {
                g.tool = Tool::ItemDes(ITEM_MENU[i].kind);
                g.drag = None;
            }
            t
        }
        _ => toolbar_y,
    }
}

/// Options of the build menu's Constructions category: draws the grid, returns nothing.
pub fn draw_constructions(g: &mut Game, r: Rect, gy: f32, cell_w: f32, cols: usize, row_h: f32) {
    let s = g.ui.scale;
    let pad = 6.0 * s;
    for (k, c) in CONSTRUCTIONS.iter().enumerate() {
        let (cx, cy) = ((k % cols) as f32, (k / cols) as f32);
        let br = Rect::new(r.x + pad + cx * (cell_w + 4.0 * s), gy + cy * (row_h + 4.0 * s), cell_w, row_h);
        let st = if g.tool == Tool::Construct(c.kind) { BtnState::Active } else { BtnState::Normal };
        if g.ui.hovered(br) {
            g.ui.tooltip = Some((format!("{} - drag a rectangle, needs {}", c.label, c.needs), vec2(br.x, br.y - 4.0)));
        }
        let fb = if c.kind == ConstructionKind::Remove { Ico::Eraser } else { Ico::Hammer };
        if super::icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, br, c.icon, fb, c.label, st) {
            g.tool = if g.tool == Tool::Construct(c.kind) { Tool::None } else { Tool::Construct(c.kind) };
            g.drag = None;
        }
    }
}

// ------------------------------------------------------------------ map overlays

/// Tile designation overlays of the visible area: smooth/engrave markers, carve marks and,
/// while the traffic menu is open, the traffic colors (DF shows them only then).
pub fn draw_overlays(g: &mut Game, x0: i16, y0: i16, x1: i16, y1: i16) {
    prune_carve(g);
    let z = g.cam.z as i16;
    let tp = g.cam.tile_px();
    let show_traffic = g.submenu == Submenu::Traffic || matches!(g.tool, Tool::Traffic(_));
    let sp = std::mem::take(&mut g.rend.r5);
    let sp = sp.unwrap_or_else(|| R5Sprites::load(&g.gfx));
    for y in y0..y1 {
        for x in x0..x1 {
            let p = Pos::new(x, y, z);
            let Some(t) = g.sim.world.tile(p).copied() else { continue };
            if t.hidden() {
                continue;
            }
            let f = t.flags;
            let marker = f & tile_flags::MARKER != 0;
            let tint: [u8; 4] = if marker { [120, 170, 255, 220] } else { [255; 4] };
            let d = if f & tile_flags::DESIG_ENGRAVE != 0 {
                Some((sp.engrave, [235, 200, 90]))
            } else if f & tile_flags::DESIG_SMOOTH != 0 {
                Some((sp.smooth, [235, 220, 120]))
            } else {
                None
            };
            if let Some((s, c)) = d {
                let r = tile_rect(g, x, y);
                if !s.map(|s| g.rend.draw_sprite(&g.gfx, &s.mul_tint(tint), r.x, r.y, tp)).unwrap_or(false) {
                    fallback_marker(r, c);
                }
            }
            if show_traffic {
                let lv = t.traffic() as usize;
                if lv > 0 {
                    let r = tile_rect(g, x, y);
                    let ok = sp.traffic[lv].map(|s| g.rend.draw_sprite(&g.gfx, &s, r.x, r.y, tp)).unwrap_or(false);
                    if !ok {
                        let c = [[0, 0, 0], [230, 220, 60], [80, 220, 80], [230, 70, 60]][lv];
                        draw_rectangle(r.x, r.y, r.w, r.h, Color::from_rgba(c[0], c[1], c[2], 70));
                    }
                }
            }
        }
    }
    // Carve marks (this session).
    if !g.r5.carve.is_empty() {
        let marks: Vec<(Pos, DetailKind)> = g.r5.carve.iter().filter(|(p, _)| p.z == z && p.x >= x0 && p.x < x1 && p.y >= y0 && p.y < y1).map(|(p, k)| (*p, *k)).collect();
        for (p, k) in marks {
            let r = tile_rect(g, p.x, p.y);
            let s = match k {
                DetailKind::CarveFortification => sp.fortify,
                _ => {
                    let has = |dx: i16, dy: i16| g.r5.carve.get(&Pos::new(p.x + dx, p.y + dy, p.z)) == Some(&DetailKind::CarveTrack);
                    let m = (has(0, -1) as u8) | (has(0, 1) as u8) << 1 | (has(-1, 0) as u8) << 2 | (has(1, 0) as u8) << 3;
                    sp.track[if m == 0 { 12 } else { m as usize }]
                }
            };
            if !s.map(|s| g.rend.draw_sprite(&g.gfx, &s, r.x, r.y, tp)).unwrap_or(false) {
                fallback_marker(r, [200, 200, 220]);
            }
        }
    }
    g.rend.r5 = Some(sp);
}

fn fallback_marker(r: Rect, c: [u8; 3]) {
    let col = Color::from_rgba(c[0], c[1], c[2], 200);
    let th = (r.w / 16.0).max(1.0);
    draw_rectangle_lines(r.x + th, r.y + th, r.w - 2.0 * th, r.h - 2.0 * th, th, col);
    draw_line(r.x + r.w * 0.25, r.y + r.h * 0.5, r.x + r.w * 0.75, r.y + r.h * 0.5, th, col);
}

/// Forbidden / dump / hidden markers on an item drawn at `r` (DF's `DESIGNATION_ITEM_*`).
pub fn item_marker(rend: &mut MapRenderer, gfx: &Gfx, it: &Item, r: Rect, tp: f32) {
    let f = &it.flags;
    if !f.forbid && !f.dump && !f.hidden {
        return;
    }
    let sp = rend.r5(gfx);
    let s = match (f.forbid, f.dump) {
        (true, true) => sp.forbidden_dump.or(sp.forbidden),
        (true, false) => sp.forbidden,
        (false, true) => sp.dump,
        _ => sp.hidden,
    };
    match s {
        Some(s) => {
            rend.draw_sprite(gfx, &s, r.x, r.y, tp);
        }
        None => {
            // Fallback: DF's classic look (forbidden items red, dump marked brown).
            let c = if f.forbid { Color::from_rgba(230, 60, 50, 230) } else { Color::from_rgba(200, 140, 70, 230) };
            let k = r.w * 0.28;
            draw_line(r.x + r.w - k, r.y + 2.0, r.x + r.w - 2.0, r.y + k, 2.0, c);
            draw_line(r.x + r.w - 2.0, r.y + 2.0, r.x + r.w - k, r.y + k, 2.0, c);
        }
    }
}

/// Hidden items are only drawn while the item designation menu is open (so they can be
/// unhidden), with the hidden marker.
pub fn show_hidden_items(g: &Game) -> bool {
    g.submenu == Submenu::Items || matches!(g.tool, Tool::ItemDes(_))
}

/// Index into `R5Sprites::planned` for a construction subtype (DF `construction_type` names
/// and the `ConstructionKind` names).
fn planned_index(sub: &str) -> Option<usize> {
    Some(match sub {
        "Wall" => 0,
        "Floor" => 1,
        "Ramp" => 2,
        "UpStair" | "StairUp" => 3,
        "DownStair" | "StairDown" => 4,
        "UpDownStair" | "StairUpdown" | "StairUpDown" => 5,
        "Fortification" => 6,
        "ReinforcedWall" => 7,
        _ => return None,
    })
}

/// A planned (not yet built) construction: DF's blueprint sprite, translucent, tinted with
/// the material once one is assigned (else neutral). Removal designations get DF's
/// remove-construction marker. Returns false if the building is not a construction.
pub fn draw_construction(g: &mut Game, bi: usize) -> bool {
    let b = &g.sim.world.buildings[bi];
    if b.kind != "Construction" {
        return false;
    }
    if b.built && !is_removal(b) {
        // Built constructions are terrain (constructed tiletypes).
        return true;
    }
    let tp = g.cam.tile_px();
    let sp = std::mem::take(&mut g.rend.r5).unwrap_or_else(|| R5Sprites::load(&g.gfx));
    let b = &g.sim.world.buildings[bi];
    let removal = is_removal(b);
    let tint = construction_tint(&g.sim.world, b);
    let (x1, y1, x2, y2, z, sub) = (b.x1, b.y1, b.x2, b.y2, b.z, b.subtype.clone());
    for y in y1..=y2 {
        for x in x1..=x2 {
            let p = Pos::new(x, y, z);
            if !g.sim.world.buildings[bi].contains(p) {
                continue;
            }
            let r = tile_rect(g, x, y);
            let s = if removal {
                sp.remove_construction
            } else if sub.starts_with("Track") {
                let m = track_mask(&sub).unwrap_or_else(|| track_neighbors(&g.sim.world, p));
                sp.planned_track[if m == 0 { 12 } else { m as usize }]
            } else {
                planned_index(&sub).and_then(|i| sp.planned[i]).or(sp.planned[0])
            };
            let ok = s.map(|s| g.rend.draw_sprite(&g.gfx, &s.mul_tint(if removal { [255; 4] } else { tint }), r.x, r.y, tp)).unwrap_or(false);
            if !ok {
                let c = Color::from_rgba(tint[0], tint[1], tint[2], 120);
                draw_rectangle(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, c);
                draw_rectangle_lines(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, 1.0, Color::from_rgba(200, 220, 255, 200));
            }
        }
    }
    g.rend.r5 = Some(sp);
    true
}

fn is_removal(b: &Building) -> bool {
    let s = b.subtype.to_ascii_uppercase();
    s.contains("REMOVE") || s.contains("DECONSTRUCT") || b.flags.iter().any(|f| f == "REMOVE" || f == "DECONSTRUCT" || f == "TO_BE_REMOVED")
}

/// Blueprint tint: the material color once a material is assigned, else DF's neutral
/// translucent blueprint.
fn construction_tint(w: &World, b: &Building) -> [u8; 4] {
    if b.mat != NO_MAT {
        if let Some(m) = w.material(b.mat) {
            if m.rgb != [0, 0, 0] {
                let k = |c: u8| (c as u16 / 2 + 128).min(255) as u8;
                return [k(m.rgb[0]), k(m.rgb[1]), k(m.rgb[2]), 170];
            }
        }
    }
    [235, 240, 255, 150]
}

/// Connection mask from a DF track construction name (`TrackNS`, `TrackNSEW`, `TrackRampN`).
fn track_mask(sub: &str) -> Option<u8> {
    let d = sub.strip_prefix("Track")?.trim_start_matches("Ramp");
    if d.is_empty() {
        return None;
    }
    let mut m = 0u8;
    for c in d.chars() {
        m |= match c {
            'N' => 1,
            'S' => 2,
            'W' => 4,
            'E' => 8,
            _ => 0,
        };
    }
    Some(m)
}

/// Track connections to neighboring planned tracks or built track tiles.
fn track_neighbors(w: &World, p: Pos) -> u8 {
    let is_track = |q: Pos| {
        w.buildings.iter().any(|b| b.kind == "Construction" && b.subtype.starts_with("Track") && b.contains(q))
            || w.tile(q).map(|t| w.tiletypes.get(t.tt).special == TtSpecial::Track).unwrap_or(false)
    };
    (is_track(p.offset(0, -1, 0)) as u8) | (is_track(p.offset(0, 1, 0)) as u8) << 1 | (is_track(p.offset(-1, 0, 0)) as u8) << 2 | (is_track(p.offset(1, 0, 0)) as u8) << 3
}

/// Rough check whether DF would let you place construction `k` at `p` (for the preview
/// colors only; the sim decides).
pub fn can_construct(w: &World, k: ConstructionKind, p: Pos) -> bool {
    let Some(t) = w.tile(p) else { return false };
    if t.hidden() {
        return false;
    }
    let tt = w.tiletypes.get(t.tt);
    if k == ConstructionKind::Remove {
        return tt.material == TtMaterial::Construction || w.buildings.iter().any(|b| b.kind == "Construction" && b.contains(p));
    }
    if w.buildings.iter().any(|b| b.kind != "Stockpile" && b.kind != "Civzone" && b.contains(p)) {
        return false;
    }
    match k {
        ConstructionKind::Floor => matches!(tt.shape, Shape::Empty | Shape::RampTop | Shape::Floor | Shape::Pebbles) || tt.shape.has_floor() && tt.shape != Shape::Boulder,
        _ => matches!(tt.shape, Shape::Floor | Shape::Pebbles | Shape::Shrub | Shape::Sapling) || (tt.shape.has_floor() && !matches!(tt.shape, Shape::Boulder | Shape::Ramp)),
    }
}

/// Preview of the construction tool: the blueprint under the cursor or over the dragged
/// rectangle (on the level of the first corner), green where it fits, red where not.
pub fn draw_construct_preview(g: &mut Game) {
    let Tool::Construct(k) = g.tool else { return };
    let Some(h) = g.hover else { return };
    let z = g.cam.z as i16;
    let a = g.drag.map(|d| d.a).unwrap_or(h);
    if a.z != z {
        return;
    }
    let (lo, hi) = (Pos::new(a.x.min(h.x), a.y.min(h.y), z), Pos::new(a.x.max(h.x), a.y.max(h.y), z));
    let tp = g.cam.tile_px();
    let sp = std::mem::take(&mut g.rend.r5).unwrap_or_else(|| R5Sprites::load(&g.gfx));
    let (mut good, mut bad) = (0, 0);
    let sub = format!("{k:?}");
    for y in lo.y..=hi.y {
        for x in lo.x..=hi.x {
            let p = Pos::new(x, y, z);
            let ok = can_construct(&g.sim.world, k, p);
            if ok {
                good += 1;
            } else {
                bad += 1;
            }
            let r = tile_rect(g, x, y);
            let tint = if ok { [150, 255, 150, 170] } else { [255, 110, 110, 170] };
            let s = if k == ConstructionKind::Remove {
                sp.remove_construction
            } else if k == ConstructionKind::Track {
                let m = (y > lo.y) as u8 | ((y < hi.y) as u8) << 1 | ((x > lo.x) as u8) << 2 | ((x < hi.x) as u8) << 3;
                sp.planned_track[if m == 0 { 12 } else { m as usize }]
            } else {
                planned_index(&sub).and_then(|i| sp.planned[i])
            };
            if !s.map(|s| g.rend.draw_sprite(&g.gfx, &s.mul_tint(tint), r.x, r.y, tp)).unwrap_or(false) {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::from_rgba(tint[0], tint[1], tint[2], 60));
            }
        }
    }
    g.rend.r5 = Some(sp);
    let a2 = g.cam.to_screen(lo.x as f32, lo.y as f32);
    let b2 = g.cam.to_screen(hi.x as f32 + 1.0, hi.y as f32 + 1.0);
    let line = if bad == 0 { Color::new(0.45, 1.0, 0.45, 0.95) } else if good == 0 { Color::new(1.0, 0.4, 0.35, 0.95) } else { Color::new(1.0, 0.85, 0.3, 0.95) };
    draw_rectangle_lines(a2.x, a2.y, b2.x - a2.x, b2.y - a2.y, 2.0, line);
    let m = g.mouse();
    let s = g.ui.scale;
    let msg = format!("{} {}x{}: {good} ok{}", constr_label(k), hi.x - lo.x + 1, hi.y - lo.y + 1, if bad > 0 { format!(", {bad} blocked") } else { String::new() });
    let tw = g.font.width(&msg, s);
    let (mx, my) = ((m.x + 20.0).min(screen_width() - tw - 12.0), m.y + 22.0);
    draw_rectangle(mx - 5.0, my - 4.0, tw + 10.0, GH * s + 8.0, Color::new(0.03, 0.03, 0.04, 0.88));
    draw_rectangle_lines(mx - 5.0, my - 4.0, tw + 10.0, GH * s + 8.0, 1.0, line);
    g.font.draw(&msg, mx, my, s, line);
}

// ------------------------------------------------------------------ caged creatures

/// Material of the cage `u` sits in: the cage of a cage trap or cage building on its tile,
/// else an occupied cage item there (DF names it "<race> cage"), else any cage there.
fn find_cage_mat(w: &World, u: &Unit) -> u16 {
    let p = u.pos;
    for b in w.buildings.iter().filter(|b| b.z == p.z && b.contains(p) && (b.kind == "Cage" || (b.kind == "Trap" && b.subtype == "CageTrap"))) {
        if let Some(it) = b.items.iter().filter_map(|id| w.item(*id)).find(|it| it.kind == "CAGE") {
            return it.mat;
        }
        return b.mat;
    }
    let race = u.race.to_ascii_lowercase().replace('_', " ");
    let cages: Vec<&Item> = w.items.iter().filter(|it| it.kind == "CAGE" && it.pos == p).collect();
    if let Some(it) = cages.iter().find(|it| it.desc.to_ascii_lowercase().contains(&race)) {
        return it.mat;
    }
    cages.first().map(|it| it.mat).unwrap_or(NO_MAT)
}

/// Draw a caged creature: the occupied cage, the creature a little smaller inside it, and the
/// front bars over it. `sprites` holds the unit's sprites (from `Gfx::unit`).
pub fn draw_caged(g: &mut Game, uid: i32, r: Rect, sprites: &[Sprite]) -> bool {
    let now = get_time();
    let Some(u) = g.sim.world.unit(uid) else { return false };
    let pos = u.pos;
    let mat = match g.r5.cages.get(&uid) {
        Some(&(p, m, t)) if p == pos && now - t < 3.0 => m,
        _ => {
            let m = find_cage_mat(&g.sim.world, u);
            g.r5.cages.insert(uid, (pos, m, now));
            m
        }
    };
    let tp = g.cam.tile_px();
    let cage = g.gfx.cage_sprite(&g.sim.world, mat);
    if let Some(c) = cage {
        g.rend.draw_sprite(&g.gfx, &c, r.x, r.y, tp);
    } else {
        draw_rectangle(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, Color::from_rgba(40, 32, 26, 220));
    }
    // The creature, 80 % size, standing on the cage floor.
    let k = 0.8;
    let (ix, iy) = (r.x + r.w * (1.0 - k) / 2.0, r.y + r.h * (1.0 - k) - r.h * 0.04);
    let mut drawn = false;
    for s in sprites {
        let off = s.unit_center_px(32) as f32 * tp * k / 32.0;
        drawn |= g.rend.draw_sprite(&g.gfx, s, ix + off, iy, tp * k);
    }
    if !drawn {
        if let Some(u) = g.sim.world.unit(uid) {
            let code = u.race.bytes().next().unwrap_or(b'?');
            let (gw, gh) = (r.w * 0.5, r.h * 0.7);
            g.font.glyph(code, r.x + (r.w - gw) / 2.0, r.y + (r.h - gh) / 2.0, gw, gh, Color::from_rgba(230, 200, 160, 255));
        }
    }
    // Front bars in the cage's color.
    let c = g.sim.world.material(mat).map(|m| m.rgb).filter(|c| *c != [0, 0, 0]).unwrap_or([150, 110, 70]);
    let bar = Color::from_rgba((c[0] as u16 * 3 / 4 + 40).min(255) as u8, (c[1] as u16 * 3 / 4 + 40).min(255) as u8, (c[2] as u16 * 3 / 4 + 40).min(255) as u8, 235);
    let th = (r.w / 16.0).max(1.0);
    let (top, bot) = (r.y + r.h * 0.10, r.y + r.h * 0.94);
    for i in 0..5 {
        let x = r.x + r.w * (0.16 + 0.17 * i as f32);
        draw_line(x, top, x, bot, th, bar);
    }
    draw_line(r.x + r.w * 0.06, top, r.x + r.w * 0.94, top, th, bar);
    true
}

// ------------------------------------------------------------------ unit sheet

/// DF's announcement wording for a strange mood (paraphrased).
pub fn mood_words(m: &str) -> &'static str {
    match m.to_ascii_lowercase().as_str() {
        "fey" => "is taken by a fey mood!",
        "secretive" => "withdraws from society...",
        "possessed" => "has been possessed!",
        "macabre" => "begins to stalk and brood...",
        "fell" => "looses a roaring laughter, fell and terrible!",
        "melancholy" | "depressed" => "is stricken by melancholy.",
        "insane" | "raving" | "berserk" | "stark raving mad" => "has gone insane.",
        _ => "is in a strange mood.",
    }
}

/// One-line mood summary for the overview, e.g. `Fey mood: Craftsdwarf's workshop, needs
/// rough gems`. Empty if the unit is not in a mood.
pub fn mood_line(g: &Game, u: &Unit) -> (String, Color) {
    let mi = g.sim.mood_info(u.id);
    let mood = if mi.mood.is_empty() { u.mood.clone() } else { mi.mood.clone() };
    if mood.is_empty() {
        return (String::new(), col::TEXT);
    }
    let mut t = format!("{} mood", super::pretty(&mood));
    if mi.workshop >= 0 {
        if let Some(b) = g.sim.world.building(mi.workshop) {
            t.push_str(&format!(": claimed {}", super::building::building_title(g, b)));
        }
    } else {
        t.push_str(": looking for a workshop");
    }
    if !mi.needs.is_empty() {
        t.push_str(&format!(", needs {}", mi.needs.join(", ")));
    }
    (t, mood_color(&mood))
}

fn mood_color(m: &str) -> Color {
    match m.to_ascii_lowercase().as_str() {
        "fey" => Color::from_rgba(120, 220, 255, 255),
        "secretive" => Color::from_rgba(200, 160, 255, 255),
        "possessed" => Color::from_rgba(255, 200, 80, 255),
        "macabre" | "fell" => col::TEXT_BAD,
        _ => col::TEXT_HI,
    }
}

/// The Mood tab: mood, claimed workshop (button), needed materials, time left.
pub fn draw_mood_tab(g: &mut Game, u: &Unit, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let maxc = ((area.w / (8.0 * s)) as usize).max(10);
    let (x, mut y) = (area.x, area.y);
    let mi = g.sim.mood_info(u.id);
    let mood = if mi.mood.is_empty() { u.mood.clone() } else { mi.mood.clone() };
    g.font.draw("Strange mood", x, y, s, col::BORDER_HI);
    y += lh;
    if mood.is_empty() {
        g.font.draw("Not in a strange mood.", x, y, s, col::TEXT_DIM);
        y += lh * 1.5;
        for l in super::psyche::wrap("Dwarves with a craft skill can be taken by a mood (fey, secretive, possessed, macabre or fell). They claim a workshop, gather materials and make an artifact; without the materials they go insane.", maxc, 2) {
            g.font.draw(&l, x, y, s, col::TEXT_DIM);
            y += lh;
        }
        return;
    }
    let mc = mood_color(&mood);
    let head = format!("{} {}", super::psyche::pronoun(u), mood_words(&mood));
    for l in super::psyche::wrap(&head, maxc, 2) {
        g.font.draw(&l, x, y, s, mc);
        y += lh;
    }
    g.font.draw(&format!("Mood: {}", super::pretty(&mood)), x, y, s, mc);
    y += lh * 1.5;
    g.font.draw("Workshop", x, y, s, col::BORDER_HI);
    y += lh;
    let ws = (mi.workshop >= 0).then(|| g.sim.world.building(mi.workshop).map(|b| (b.id, super::building::building_title(g, b), b.center))).flatten();
    match ws {
        Some((id, title, c)) => {
            g.font.draw(&clip(&format!("Claimed: {title} (#{id})"), maxc), x, y, s, col::TEXT);
            y += lh;
            let bw = 20.0 * 8.0 * s;
            if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, GH * s + 8.0 * s), "Go to workshop", BtnState::Normal) {
                g.cam.focus(c);
                g.selected = Selected::Building(id);
            }
            y += GH * s + 14.0 * s;
        }
        None => {
            g.font.draw("No workshop claimed yet (needs a free one of the right kind).", x, y, s, col::TEXT_HI);
            y += lh * 1.5;
        }
    }
    g.font.draw("Materials needed", x, y, s, col::BORDER_HI);
    y += lh;
    if mi.needs.is_empty() {
        g.font.draw("Nothing more (or not known yet).", x, y, s, col::TEXT_DIM);
        y += lh;
    }
    for n in &mi.needs {
        g.font.draw(&clip(&format!("· {n}"), maxc), x, y, s, col::TEXT);
        y += lh;
    }
    if let Some(t) = mi.ticks_left {
        y += lh * 0.5;
        let c = if t < rf_core::TICKS_PER_MONTH as u64 / 2 { col::TEXT_BAD } else { col::TEXT_HI };
        g.font.draw(&format!("Goes insane in about {} without them.", super::round4::ticks_span(t)), x, y, s, c);
    }
}

/// Treatment section of the Health tab, like DF's health screen ("Diagnosis required",
/// "Suturing required", ...), derived from DF's wound flags (`DIAGNOSED`, `SUTURED`,
/// `INFECTION`, ...) and what the sim reports the unit doing.
pub fn treatment_rows(g: &Game, u: &Unit) -> Vec<super::psyche::Row> {
    use super::psyche::{is_active_wound, Row};
    let active: Vec<&rf_core::Wound> = u.wounds.iter().filter(|w| is_active_wound(w)).collect();
    let mut rows = vec![Row::Head("Treatment".into())];
    if active.is_empty() {
        rows.push(Row::Text("No treatment needed.".into(), col::TEXT_DIM));
        rows.push(Row::Gap);
        return rows;
    }
    let has = |w: &rf_core::Wound, f: &[&str]| w.flags.iter().any(|x| f.contains(&x.as_str()));
    let any = |f: &[&str]| active.iter().any(|w| has(w, f));
    let all = |f: &[&str]| active.iter().all(|w| has(w, f));
    let bone = |w: &rf_core::Wound| w.layer.to_ascii_uppercase().contains("BONE") || has(w, &["BROKEN", "FRACTURED", "SMASHED", "SMASHED_APART"]);
    let mut need: Vec<(&str, Color)> = Vec::new();
    let mut done: Vec<&str> = Vec::new();
    if !all(&["DIAGNOSED"]) {
        need.push(("Diagnosis required", col::TEXT_HI));
    } else {
        done.push("diagnosed");
    }
    if any(&["INFECTION", "INFECTED"]) {
        need.push(("Cleaning required (infection)", Color::from_rgba(200, 220, 80, 255)));
    }
    let bleeding_open = active.iter().any(|w| (w.bleeding > 0 || has(w, &["BLEEDING", "CUT", "EDGED_DAMAGE"])) && !has(w, &["SUTURED"]));
    if bleeding_open {
        need.push(("Suturing required", col::TEXT_BAD));
    } else if any(&["SUTURED"]) {
        done.push("sutured");
    }
    if active.iter().any(|w| (w.bleeding > 0 || has(w, &["CUT", "EDGED_DAMAGE"])) && !has(w, &["BANDAGED", "DRESSED", "DRESSING"])) {
        need.push(("Dressing required", col::TEXT_HI));
    } else if any(&["BANDAGED", "DRESSED", "DRESSING"]) {
        done.push("dressed");
    }
    let broken: Vec<&&rf_core::Wound> = active.iter().filter(|w| bone(w)).collect();
    if broken.iter().any(|w| !has(w, &["SET", "SETTING_DONE"])) {
        need.push(("Setting required (broken bone)", col::TEXT_BAD));
    }
    if broken.iter().any(|w| !has(w, &["SPLINT", "CAST", "IMMOBILIZED"])) {
        need.push(("Immobilization required", col::TEXT_HI));
    } else if !broken.is_empty() {
        done.push("immobilized");
    }
    if any(&["SEVERED", "SMASHED_APART", "SENSORY_NERVE_SEVERED", "MOTOR_NERVE_SEVERED"]) {
        rows.push(Row::Text("Permanent damage (severed part or nerve): cannot be treated.".into(), col::TEXT_DIM));
    }
    for (t, c) in need {
        rows.push(Row::Text(t.into(), c));
    }
    if !done.is_empty() {
        rows.push(Row::Text(format!("Done: {}", done.join(", ")), col::TEXT_GOOD));
    }
    let act = g.sim.unit_status(u.id).activity;
    let al = act.to_ascii_lowercase();
    if al.contains("rest") || al.contains("hospital") || al.contains("bed") || al.contains("treat") || al.contains("diagnos") || al.contains("sutur") || al.contains("surgery") {
        rows.push(Row::Note("Now".into(), col::TEXT_CYAN, act));
    }
    rows.push(Row::Gap);
    rows
}

/// Small toggle button (forbid `F`, dump `D`) for item lists; returns true when clicked.
pub fn flag_toggle(g: &mut Game, x: f32, y: f32, label: &str, on: bool, tip: &str) -> bool {
    let s = g.ui.scale;
    let r = Rect::new(x, y - 1.0 * s, 16.0 * s, GH * s + 2.0 * s);
    let hov = g.ui.hovered(r);
    let (bg, fg) = match (on, label) {
        (true, "F") => (Color::from_rgba(150, 40, 35, 255), WHITE),
        (true, _) => (Color::from_rgba(120, 85, 40, 255), WHITE),
        _ => (if hov { col::BUTTON_HOVER } else { col::BUTTON }, col::TEXT_DIM),
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, if hov { col::BORDER_HI } else { Color::new(0.35, 0.35, 0.38, 1.0) });
    g.font.draw(label, r.x + (r.w - 8.0 * s) / 2.0, y, s, fg);
    if hov {
        g.ui.tooltip = Some((tip.to_string(), vec2(r.x, r.y - 4.0)));
    }
    g.ui.clicked(r)
}

/// An item row with forbid/dump toggles at the right edge. `right` = right edge of the row.
pub fn item_row_toggles(g: &mut Game, id: i32, right: f32, y: f32) {
    let s = g.ui.scale;
    let (forbid, dump) = g.sim.world.item(id).map(|it| (it.flags.forbid, it.flags.dump)).unwrap_or((false, false));
    if flag_toggle(g, right - 16.0 * s, y, "D", dump, if dump { "Dump: on (click to remove)" } else { "Mark for dumping" }) {
        g.sim.set_item_flag(id, "dump", !dump);
    }
    if flag_toggle(g, right - 36.0 * s, y, "F", forbid, if forbid { "Forbidden (click to claim)" } else { "Forbid" }) {
        g.sim.set_item_flag(id, "forbid", !forbid);
    }
}

/// Suffix for an item label: `(forbidden)`, `(dump)`, `(hidden)`.
pub fn item_tags(it: &Item) -> String {
    let mut t = Vec::new();
    if it.flags.forbid {
        t.push("forbidden");
    }
    if it.flags.dump {
        t.push("dump");
    }
    if it.flags.hidden {
        t.push("hidden");
    }
    if t.is_empty() {
        String::new()
    } else {
        format!(" ({})", t.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_names_and_masks() {
        assert_eq!(track_suffix(1 | 2), "NS");
        assert_eq!(track_suffix(4 | 8), "WE");
        assert_eq!(track_suffix(1 | 2 | 4 | 8), "NSWE");
        assert_eq!(track_mask("TrackNS"), Some(3));
        assert_eq!(track_mask("TrackNSEW"), Some(15));
        assert_eq!(track_mask("TrackRampN"), Some(1));
        assert_eq!(track_mask("Track"), None);
        assert_eq!(planned_index("UpDownStair"), Some(5));
        assert_eq!(planned_index(&format!("{:?}", ConstructionKind::DownStair)), Some(4));
    }

    #[test]
    fn item_designations_map_to_sim_flags() {
        assert_eq!(ItemDes::Claim.flag(), ("forbid", false));
        assert_eq!(ItemDes::Forbid.flag(), ("forbid", true));
        assert_eq!(ItemDes::Dump.flag(), ("dump", true));
        assert_eq!(ItemDes::Unhide.flag(), ("hidden", false));
        // Every kind has a menu entry and a label.
        for e in ITEM_MENU {
            assert_eq!(item_des_label(e.kind), e.label);
        }
        assert_eq!(CONSTRUCTIONS.len(), 9);
    }

    #[test]
    fn construction_check_on_synthetic_world() {
        let w = World::synthetic(24, 24, 12);
        // Some open floor exists where a wall may go.
        let mut found = false;
        'o: for z in 0..12 {
            for y in 0..24 {
                for x in 0..24 {
                    let p = Pos::new(x, y, z);
                    if w.shape(p) == Shape::Floor && !w.tile(p).unwrap().hidden() {
                        assert!(can_construct(&w, ConstructionKind::Wall, p));
                        found = true;
                        break 'o;
                    }
                }
            }
        }
        assert!(found);
    }
}
