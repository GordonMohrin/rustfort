//! [app12] Round-12 screens and tools: hauling routes and minecarts, machines, siege engines.
//!
//! - **Hauling** (DF `D_HAULING`) is an editor now: new / delete route, add stops by clicking a
//!   track stop or track tile on the map, link stockpiles to a stop (take from / give to) by
//!   clicking them, the departure condition per stop (DF's push / ride / guide, direction,
//!   when full / empty, wait), minecarts assigned from the fort's carts and wheelbarrows, and
//!   the live state of each route from `Sim::route_status`. Route stops and carts are drawn on
//!   the map while the screen or one of its map tools is open.
//! - **Build > Machines** (screw pump, water wheel, windmill, gear assembly, horizontal and
//!   vertical axle, roller, track stop) and **Build > Siege engines** (ballista, catapult, bolt
//!   thrower), placed through `Sim::can_place_dir` / `Sim::place_building_dir` with a
//!   direction (Tab or the buttons in the build menu): the pump's "pump from" side, the
//!   wheel's axis, the roller's push direction, the track stop's dump direction, the engine's
//!   facing. The placement ghost shows the direction as an arrow.
//! - **Machine sheet**: powered or not (`Sim::machine_powered`), what it makes or uses (wiki
//!   power numbers), the fort's networks (`Sim::power_report`), "Pump by hand" for screw pumps
//!   (`Sim::set_pump_manual`), and the **power overlay** (machines on the map green = running,
//!   red = no power, grey = not built).
//! - **Siege engine sheet**: aim at a tile (`Sim::aim_siege_engine`), fire at will, ammo in
//!   the fort and loaded.
//! - Track stops and rollers: dump / push direction buttons.

use super::building::building_title;
use super::round8::{frame, scroll};
use super::{clip, col, list_row, text_button, BtnState};
use crate::font::{GH, GW};
use crate::game::{Game, Screen, Selected, Tool};
use macroquad::prelude::*;
use rf_core::{Building, Pos};

// ====================================================================== state

#[derive(Default)]
pub struct R12Ui {
    /// Chosen direction index per build option.
    pub dirs: Vec<(usize, usize)>,
    /// Machines highlighted on the map by power state.
    pub power_overlay: bool,
    // ---- hauling editor
    pub stop_sel: Option<usize>,
    pub stop_scroll: usize,
    pub cart_pick: bool,
    pub cart_scroll: usize,
    /// Last aim ordered per siege engine from this interface (None = fire at will).
    pub aims: Vec<(i32, Option<Pos>)>,
}

/// Machine and minecart building kinds that get a round-12 sheet section.
pub fn is_machine(b: &Building) -> bool {
    matches!(b.kind.as_str(), "ScrewPump" | "WaterWheel" | "Windmill" | "GearAssembly" | "AxleHorizontal" | "AxleVertical" | "Rollers") || (b.kind == "Workshop" && b.subtype == "Millstone")
}

pub fn has_section(b: &Building) -> bool {
    is_machine(b) || b.kind == "SiegeEngine" || (b.kind == "Trap" && b.subtype == "TrackStop")
}

// ====================================================================== build directions

/// The chosen direction of build option `i` ("" if it has none).
pub fn cur_dir(g: &Game, i: usize) -> &'static str {
    let dirs = g.sim.build_directions(i);
    if dirs.is_empty() {
        return "";
    }
    let k = g.r12.dirs.iter().find(|e| e.0 == i).map(|e| e.1).unwrap_or(0);
    dirs[k % dirs.len()]
}

fn set_dir(g: &mut Game, i: usize, k: usize) {
    match g.r12.dirs.iter_mut().find(|e| e.0 == i) {
        Some(e) => e.1 = k,
        None => g.r12.dirs.push((i, k)),
    }
    g.build_check = None;
}

/// Words for a direction string of option `i`.
pub fn dir_label(kind: &str, d: &str) -> String {
    let full = |c: &str| match c {
        "N" => "north",
        "S" => "south",
        "E" => "east",
        "W" => "west",
        "NE" => "north-east",
        "NW" => "north-west",
        "SE" => "south-east",
        "SW" => "south-west",
        _ => "",
    };
    match kind {
        "ScrewPump" => format!("pump from {}", d.trim_start_matches("From").to_lowercase()),
        "WaterWheel" | "AxleHorizontal" => if d == "NS" { "north-south".into() } else { "east-west".into() },
        "Rollers" => format!("push {}", full(d)),
        "Trap" => if d.is_empty() { "no dumping".into() } else { format!("dump {}", full(d)) },
        "SiegeEngine" | "Windmill" => format!("facing {}", full(d)),
        _ => d.to_string(),
    }
}

/// Footprint of option `i` at the mouse tile `a` (dragged: from `a` to `b`), with its direction.
pub fn footprint(g: &Game, i: usize, a: Pos, b: Pos) -> (Pos, Pos) {
    let Some(o) = g.build_opts.get(i) else { return (a, a) };
    if o.dragged {
        return super::build::footprint(o, a, b);
    }
    let (w, h) = g.sim.build_size(i, cur_dir(g, i));
    let lo = Pos::new(a.x - (w - 1) / 2, a.y - (h - 1) / 2, a.z);
    (lo, Pos::new(lo.x + w - 1, lo.y + h - 1, a.z))
}

/// Place option `i` (the Build tool's click or drag).
pub fn place(g: &mut Game, i: usize, a: Pos, b: Pos) {
    let Some(o) = g.build_opts.get(i).cloned() else { return };
    let (lo, hi) = footprint(g, i, a, Pos::new(b.x, b.y, a.z));
    let dir = cur_dir(g, i);
    match g.sim.place_building_dir(i, lo, hi, dir) {
        Ok(id) => {
            let d = if dir.is_empty() { String::new() } else { format!(", {}", dir_label(o.kind, dir)) };
            g.set_status(format!("{} placed (#{id}{d}), needs {}", o.label, o.needs));
        }
        Err(e) => g.set_status(format!("Cannot place {}: {e}", o.label)),
    }
    g.build_check = None;
}

/// Placement check for the ghost (cached like `build::draw_ghost`).
pub fn check(g: &mut Game, i: usize, lo: Pos, hi: Pos) -> Result<(), String> {
    match &g.build_check {
        Some((ci, ca, cb, res)) if *ci == i && *ca == lo && *cb == hi => res.clone(),
        _ => {
            let res = g.sim.can_place_dir(i, lo, hi, cur_dir(g, i));
            g.build_check = Some((i, lo, hi, res.clone()));
            res
        }
    }
}

/// The direction a ghost or building points to, as a unit vector (for the arrow), if any.
fn dir_vec(kind: &str, d: &str) -> Option<(f32, f32)> {
    let v = |c: &str| -> Option<(f32, f32)> {
        Some(match c {
            "N" => (0.0, -1.0),
            "S" => (0.0, 1.0),
            "E" => (1.0, 0.0),
            "W" => (-1.0, 0.0),
            "NE" => (0.707, -0.707),
            "NW" => (-0.707, -0.707),
            "SE" => (0.707, 0.707),
            "SW" => (-0.707, 0.707),
            _ => return None,
        })
    };
    match kind {
        // Water flows from the input side to the output side.
        "ScrewPump" => v(match d {
            "FromNorth" => "S",
            "FromSouth" => "N",
            "FromEast" => "W",
            "FromWest" => "E",
            _ => "",
        }),
        "Rollers" | "SiegeEngine" | "Trap" => v(d),
        _ => None,
    }
}

/// An arrow over the rectangle (screen coordinates) pointing along `(dx, dy)`.
pub fn arrow(a: Vec2, b: Vec2, dv: (f32, f32), c: Color) {
    let m = vec2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let len = ((b.x - a.x).max(b.y - a.y) * 0.42).max(10.0);
    let d = vec2(dv.0, dv.1);
    let p1 = m - d * len;
    let p2 = m + d * len;
    let th = (len / 9.0).max(2.0);
    draw_line(p1.x, p1.y, p2.x, p2.y, th, c);
    let n = vec2(-d.y, d.x);
    let hl = (len * 0.45).max(6.0);
    let q1 = p2 - d * hl + n * hl * 0.6;
    let q2 = p2 - d * hl - n * hl * 0.6;
    draw_triangle(p2, q1, q2, c);
}

/// Ghost extras: the direction arrow and the direction name under the label.
pub fn ghost_extras(g: &Game, i: usize, lo: Pos, hi: Pos) {
    let Some(o) = g.build_opts.get(i) else { return };
    let d = cur_dir(g, i);
    if let Some(v) = dir_vec(o.kind, d) {
        let a = g.cam.to_screen(lo.x as f32, lo.y as f32);
        let b = g.cam.to_screen(hi.x as f32 + 1.0, hi.y as f32 + 1.0);
        arrow(a, b, v, Color::new(0.45, 0.85, 1.0, 0.95));
    }
}

/// The direction chooser in the build menu header (`y` = the hint line). Returns true when it
/// was drawn (the caller skips its own hint).
pub fn draw_dir_buttons(g: &mut Game, i: usize, x: f32, y: f32, w: f32) -> bool {
    let dirs = g.sim.build_directions(i);
    let Some(kind) = g.build_opts.get(i).map(|o| o.kind) else { return false };
    if dirs.is_empty() {
        return false;
    }
    let s = g.ui.scale;
    let cur = cur_dir(g, i);
    let mut bx = x;
    g.font.draw("Direction (Tab):", bx, y, s, col::TEXT_DIM);
    bx += 17.0 * GW * s;
    let mut hit = None;
    for (k, d) in dirs.iter().enumerate() {
        let l = if kind == "SiegeEngine" { d.to_string() } else { dir_label(kind, d) };
        let bw = (l.chars().count() as f32 + 2.0) * GW * s;
        if bx + bw > x + w {
            break;
        }
        let st = if *d == cur { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y - 3.0 * s, bw, GH * s + 4.0 * s), &l, st) {
            hit = Some(k);
        }
        bx += bw + 3.0 * s;
    }
    if let Some(k) = hit {
        set_dir(g, i, k);
    }
    true
}

/// Round-12 keys: Tab turns the building being placed. Returns true when used.
pub fn keys(g: &mut Game) -> bool {
    let Tool::Build(i) = g.tool else { return false };
    if !g.input.fired.contains(&KeyCode::Tab) {
        return false;
    }
    let n = g.sim.build_directions(i).len();
    if n == 0 {
        return false;
    }
    let k = g.r12.dirs.iter().find(|e| e.0 == i).map(|e| e.1).unwrap_or(0);
    let back = g.input.mods & crate::keys::SHIFT != 0;
    set_dir(g, i, if back { (k + n - 1) % n } else { (k + 1) % n });
    true
}

// ====================================================================== hauling editor

/// One DF departure condition (`stop_depart_condition`), in the exporter's text form:
/// `<Push|Ride|Guide> <North|South|East|West>[ after N ticks][ when >= P% full | when <= P% full]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Cond {
    pub mode: &'static str,
    pub dir: &'static str,
    pub timeout: u32,
    /// 0 = leave any time, 1 = when full (>= 100 %), 2 = when empty (<= 0 %).
    pub when: u8,
}

pub const MODES: &[&str] = &["Push", "Ride", "Guide"];
pub const CDIRS: &[&str] = &["North", "South", "East", "West"];
/// A DF day in ticks.
pub const DAY: u32 = 1200;

impl Default for Cond {
    fn default() -> Cond {
        Cond { mode: "Push", dir: "North", timeout: 0, when: 1 }
    }
}

impl Cond {
    pub fn parse(s: &str) -> Cond {
        let mut c = Cond { when: 0, ..Default::default() };
        let mut it = s.split_whitespace();
        if let Some(m) = it.next() {
            c.mode = MODES.iter().copied().find(|x| x.eq_ignore_ascii_case(m)).unwrap_or("Push");
        }
        if let Some(d) = it.next() {
            c.dir = CDIRS.iter().copied().find(|x| x.eq_ignore_ascii_case(d)).unwrap_or("North");
        }
        if let Some(k) = s.find(" after ") {
            c.timeout = s[k + 7..].split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
        }
        if s.contains("when >=") {
            c.when = 1;
        } else if s.contains("when <=") {
            c.when = 2;
        }
        c
    }

    pub fn text(&self) -> String {
        let mut t = format!("{} {}", self.mode, self.dir);
        if self.timeout > 0 {
            t.push_str(&format!(" after {} ticks", self.timeout));
        }
        match self.when {
            1 => t.push_str(" when >= 100% full"),
            2 => t.push_str(" when <= 0% full"),
            _ => {}
        }
        t
    }

    /// Plain words: "push north when full, after 1 day".
    pub fn words(&self) -> String {
        let mut t = format!("{} {}", self.mode.to_lowercase(), self.dir.to_lowercase());
        match self.when {
            1 => t.push_str(" when full"),
            2 => t.push_str(" when empty"),
            _ => {}
        }
        if self.timeout > 0 {
            let d = self.timeout as f32 / DAY as f32;
            t.push_str(&if (d - d.round()).abs() < 0.01 { format!(", after {} day{}", d.round(), if d.round() == 1.0 { "" } else { "s" }) } else { format!(", after {} ticks", self.timeout) });
        }
        t
    }
}

fn route_name(r: &rf_core::HaulRoute) -> String {
    if r.name.is_empty() {
        format!("Route {}", r.id)
    } else {
        r.name.clone()
    }
}

fn stop_name(st: &rf_core::HaulStop, n: usize) -> String {
    if st.name.is_empty() {
        format!("Stop {}", n + 1)
    } else {
        st.name.clone()
    }
}

/// Minecarts and wheelbarrows of the fort (DF route vehicles).
pub fn is_vehicle(it: &rf_core::Item) -> bool {
    it.kind == "TOOL" && matches!(it.subtype.as_str(), "ITEM_TOOL_MINECART" | "ITEM_TOOL_WHEELBARROW")
}

/// Vehicles not assigned to any route.
fn free_vehicles(g: &Game) -> Vec<(i32, String)> {
    let used: Vec<i32> = g.sim.world.haul_routes.iter().flat_map(|r| r.vehicles.iter().copied()).collect();
    g.sim.world.items.iter().filter(|it| is_vehicle(it) && !used.contains(&it.id)).map(|it| (it.id, super::panels::item_label(g, it))).collect()
}

enum HAct {
    None,
    New,
    Delete(i32),
    AddStop(i32),
    RemoveStop(i32, usize),
    Go(Pos),
    Link(i32, usize, bool),
    Unlink(i32, usize, i32),
    Cond(i32, usize, Cond),
    ClearCond(i32, usize),
    Assign(i32, i32),
    Unassign(i32, i32),
}

pub fn draw_hauling(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(c) = frame(g, "Hauling", 1180.0) else { return };
    let top = c.y + 2.0 * lh + 4.0 * s;
    draw_line(c.x, top - 6.0 * s, c.x + c.w, top - 6.0 * s, 1.0, col::BORDER);
    let routes = g.sim.world.haul_routes.clone();
    let maxc = (c.w / (GW * s)) as usize;
    let bh = GH * s + 8.0 * s;
    let mut act = HAct::None;
    if g.r10.haul_sel.map(|id| !routes.iter().any(|r| r.id == id)).unwrap_or(true) {
        g.r10.haul_sel = routes.first().map(|r| r.id);
        g.r12.stop_sel = None;
    }
    // Header: count and the route buttons.
    let carts = g.sim.world.items.iter().filter(|it| is_vehicle(it)).count();
    let head = format!("{} routes, {carts} minecarts and wheelbarrows in the fort", routes.len());
    g.font.draw(&clip(&head, maxc / 2), c.x, top, s, col::TEXT_DIM);
    let bw = 14.0 * GW * s;
    if text_button(&mut g.ui, &g.font, Rect::new(c.x + c.w - 2.0 * bw - 6.0 * s, top - 4.0 * s, bw, bh), "New route", BtnState::Normal) {
        act = HAct::New;
    }
    let del_st = if g.r10.haul_sel.is_some() { BtnState::Normal } else { BtnState::Disabled };
    if text_button(&mut g.ui, &g.font, Rect::new(c.x + c.w - bw, top - 4.0 * s, bw, bh), "Delete route", del_st) {
        if let Some(id) = g.r10.haul_sel {
            act = HAct::Delete(id);
        }
    }
    let y0 = top + lh + 10.0 * s;
    if routes.is_empty() {
        let mut y = y0;
        for l in [
            "No hauling routes yet.",
            "A route moves minecarts (or wheelbarrows) between stops: build track (carve it into the floor with",
            "Smooth > Carve track, or Build > Constructions > Track), put a track stop at each end",
            "(Build > Machines > Track stop), then press New route, add the stops by clicking them on the map,",
            "link a stockpile to each stop (take from / give to) and assign a minecart.",
        ] {
            g.font.draw(&clip(l, maxc), c.x, y, s, if y == y0 { col::TEXT } else { col::TEXT_DIM });
            y += lh;
        }
        apply(g, act);
        return;
    }
    // Route list.
    let lw = (c.w * 0.26).max(200.0 * s);
    let list = Rect::new(c.x, y0, lw, c.y + c.h - y0);
    let row_h = 2.0 * lh + 6.0 * s;
    let vis = ((list.h / row_h) as usize).max(1);
    g.r10.haul_scroll = scroll(g, list, g.r10.haul_scroll, routes.len(), vis);
    for (k, r) in routes.iter().enumerate().skip(g.r10.haul_scroll).take(vis) {
        let y = list.y + (k - g.r10.haul_scroll) as f32 * row_h;
        let sel = g.r10.haul_sel == Some(r.id);
        if list_row(&mut g.ui, Rect::new(list.x, y, list.w, row_h - 2.0 * s), sel) {
            g.r10.haul_sel = Some(r.id);
            g.r12.stop_sel = None;
            g.r12.cart_pick = false;
        }
        let lc = ((list.w - 8.0 * s) / (GW * s)) as usize;
        g.font.draw(&clip(&route_name(r), lc), list.x + 4.0 * s, y + 2.0 * s, s, if sel { col::TEXT_HI } else { col::TEXT });
        let st = g.sim.route_status(r.id);
        let t = format!("{} stops, {} carts, {} trips", r.stops.len(), r.vehicles.len(), st.trips);
        g.font.draw(&clip(&t, lc), list.x + 4.0 * s, y + 2.0 * s + lh, s, col::TEXT_DIM);
    }
    let Some(r) = routes.iter().find(|r| Some(r.id) == g.r10.haul_sel).cloned() else {
        apply(g, act);
        return;
    };
    // Route details.
    let dx = c.x + lw + 16.0 * s;
    let dw = c.x + c.w - dx;
    let dc = (dw / (GW * s)) as usize;
    let bottom = c.y + c.h;
    let mut y = y0;
    g.font.draw(&clip(&route_name(&r), dc), dx, y, 1.25 * s, col::TEXT_HI);
    y += lh * 1.3 + 4.0 * s;
    // Live status.
    let st = g.sim.route_status(r.id);
    let mut status = format!("Trips made: {}", st.trips);
    if !st.vehicles.is_empty() {
        let parts: Vec<String> = st.vehicles.iter().enumerate().map(|(k, (id, wh))| format!("cart #{id} {wh}{}", st.loaded.get(k).map(|n| format!(" ({n} items)")).unwrap_or_default())).collect();
        status.push_str(&format!("; {}", parts.join(", ")));
    }
    g.font.draw(&clip(&status, dc), dx, y, s, col::TEXT_CYAN);
    y += lh + 4.0 * s;
    // Vehicles.
    g.font.draw("Vehicles:", dx, y, s, col::BORDER_HI);
    let mut vx = dx + 10.0 * GW * s;
    if r.vehicles.is_empty() {
        g.font.draw("none", vx, y, s, col::TEXT_DIM);
        vx += 6.0 * GW * s;
    }
    for &v in &r.vehicles {
        let name = g.sim.world.items.iter().find(|it| it.id == v).map(|it| format!("{} #{v}", super::panels::item_label(g, it))).unwrap_or_else(|| format!("#{v} (gone)"));
        let tw = g.font.width(&name, s);
        if vx + tw + 30.0 * s > dx + dw - 22.0 * GW * s {
            break;
        }
        g.font.draw(&name, vx, y, s, col::TEXT);
        if text_button(&mut g.ui, &g.font, Rect::new(vx + tw + 3.0 * s, y - 3.0 * s, 2.5 * GW * s, GH * s + 4.0 * s), "x", BtnState::Normal) {
            act = HAct::Unassign(r.id, v);
        }
        vx += tw + 4.0 * GW * s;
    }
    let free = free_vehicles(g);
    let ab = Rect::new(dx + dw - 20.0 * GW * s, y - 4.0 * s, 20.0 * GW * s, bh);
    let ast = if free.is_empty() { BtnState::Disabled } else if g.r12.cart_pick { BtnState::Active } else { BtnState::Normal };
    if text_button(&mut g.ui, &g.font, ab, &format!("Assign minecart ({})", free.len()), ast) {
        g.r12.cart_pick = !g.r12.cart_pick;
    }
    y += lh + 8.0 * s;
    if g.r12.cart_pick && !free.is_empty() {
        // The picker list of free carts.
        let n = free.len().min(6);
        let pr = Rect::new(dx + dw - 40.0 * GW * s, y, 40.0 * GW * s, n as f32 * (lh + 2.0 * s) + 6.0 * s);
        draw_rectangle(pr.x, pr.y, pr.w, pr.h, Color::new(0.08, 0.08, 0.09, 0.98));
        draw_rectangle_lines(pr.x, pr.y, pr.w, pr.h, 1.0, col::BORDER_HI);
        g.r12.cart_scroll = scroll(g, pr, g.r12.cart_scroll, free.len(), n);
        for (k, (id, name)) in free.iter().enumerate().skip(g.r12.cart_scroll).take(n) {
            let ry = pr.y + 3.0 * s + (k - g.r12.cart_scroll) as f32 * (lh + 2.0 * s);
            if list_row(&mut g.ui, Rect::new(pr.x + 2.0 * s, ry, pr.w - 4.0 * s, lh), false) {
                act = HAct::Assign(r.id, *id);
            }
            g.font.draw(&clip(&format!("{name} #{id}"), 38), pr.x + 6.0 * s, ry + 1.0 * s, s, col::TEXT);
        }
    }
    // Stops.
    g.font.draw(&format!("Stops ({})", r.stops.len()), dx, y, s, col::BORDER_HI);
    if text_button(&mut g.ui, &g.font, Rect::new(dx + 12.0 * GW * s, y - 4.0 * s, 26.0 * GW * s, bh), "Add stop (click on the map)", BtnState::Normal) {
        act = HAct::AddStop(r.id);
    }
    y += lh + 8.0 * s;
    if r.stops.is_empty() {
        g.font.draw("No stops. A stop is normally a track stop at the end of a track.", dx, y, s, col::TEXT_DIM);
    }
    let pile_name = |g: &Game, id: i32| g.sim.world.building(id).map(|b| building_title(g, b)).unwrap_or_else(|| format!("#{id} (gone)"));
    let per = 4.0 * lh + 16.0 * s;
    let vis = (((bottom - y) / per) as usize).max(1);
    g.r12.stop_scroll = g.r12.stop_scroll.min(r.stops.len().saturating_sub(vis));
    let sarea = Rect::new(dx, y, dw, bottom - y);
    g.r12.stop_scroll = scroll(g, sarea, g.r12.stop_scroll, r.stops.len(), vis);
    let small = |l: &str| (l.chars().count() as f32 + 1.5) * GW * s;
    for (n, stp) in r.stops.iter().enumerate().skip(g.r12.stop_scroll).take(vis) {
        let sy = y;
        let sel = g.r12.stop_sel == Some(n);
        draw_rectangle(dx - 4.0 * s, sy - 3.0 * s, dw + 4.0 * s, per - 6.0 * s, Color::new(1.0, 1.0, 1.0, if sel { 0.07 } else if n % 2 == 0 { 0.035 } else { 0.0 }));
        let on_track = track_here(g, stp.pos);
        let head = format!("{}. {}  at {}, {}, z {}{}", n + 1, stop_name(stp, n), stp.pos.x, stp.pos.y, stp.pos.z, on_track);
        g.font.draw(&clip(&head, dc.saturating_sub(14)), dx, y, s, col::TEXT);
        let gw = 5.0 * GW * s;
        if text_button(&mut g.ui, &g.font, Rect::new(dx + dw - 2.0 * gw - 4.0 * s, y - 3.0 * s, gw, GH * s + 4.0 * s), "Go", BtnState::Normal) {
            act = HAct::Go(stp.pos);
        }
        if text_button(&mut g.ui, &g.font, Rect::new(dx + dw - gw, y - 3.0 * s, gw, GH * s + 4.0 * s), "x", BtnState::Normal) {
            act = HAct::RemoveStop(r.id, n);
        }
        y += lh + 2.0 * s;
        // Departure condition chips.
        let cond = stp.conditions.first().map(|t| Cond::parse(t));
        let mut cx = dx + 2.0 * GW * s;
        g.font.draw("Leave:", cx, y, s, col::TEXT_DIM);
        cx += 7.0 * GW * s;
        let chip = |g: &mut Game, cx: &mut f32, y: f32, l: &str, on: bool| -> bool {
            let w = small(l);
            let hit = text_button(&mut g.ui, &g.font, Rect::new(*cx, y - 3.0 * s, w, GH * s + 4.0 * s), l, if on { BtnState::Active } else { BtnState::Normal });
            *cx += w + 2.0 * s;
            hit
        };
        let base = cond.clone().unwrap_or_default();
        if chip(g, &mut cx, y, "right away", cond.is_none()) {
            act = HAct::ClearCond(r.id, n);
        }
        for (k, l) in ["when full", "when empty", "any time"].iter().enumerate() {
            let w = [1u8, 2, 0][k];
            if chip(g, &mut cx, y, l, cond.as_ref().map(|c| c.when == w).unwrap_or(false)) {
                act = HAct::Cond(r.id, n, Cond { when: w, ..base.clone() });
            }
        }
        cx += 6.0 * s;
        for (k, l) in ["no wait", "wait 1 day", "wait 7 days"].iter().enumerate() {
            let t = [0u32, DAY, 7 * DAY][k];
            if chip(g, &mut cx, y, l, cond.as_ref().map(|c| c.timeout == t).unwrap_or(false)) {
                act = HAct::Cond(r.id, n, Cond { timeout: t, ..base.clone() });
            }
        }
        y += lh + 2.0 * s;
        let mut cx = dx + 2.0 * GW * s;
        g.font.draw("Cart:", cx, y, s, col::TEXT_DIM);
        cx += 7.0 * GW * s;
        for m in MODES {
            if chip(g, &mut cx, y, m, cond.as_ref().map(|c| c.mode == *m).unwrap_or(false)) {
                act = HAct::Cond(r.id, n, Cond { mode: m, ..base.clone() });
            }
        }
        cx += 6.0 * s;
        for d in CDIRS {
            if chip(g, &mut cx, y, &d[..1], cond.as_ref().map(|c| c.dir == *d).unwrap_or(false)) {
                act = HAct::Cond(r.id, n, Cond { dir: d, ..base.clone() });
            }
        }
        if let Some(c0) = &cond {
            let t = format!("  = {}", c0.words());
            g.font.draw(&clip(&t, ((dx + dw - cx) / (GW * s)) as usize), cx, y, s, col::TEXT_CYAN);
        }
        y += lh + 2.0 * s;
        // Stockpile links.
        let mut cx = dx + 2.0 * GW * s;
        g.font.draw("Piles:", cx, y, s, col::TEXT_DIM);
        cx += 7.0 * GW * s;
        for (take, ids) in [(true, &stp.take_from), (false, &stp.give_to)] {
            for &id in ids.iter() {
                let l = format!("{} {}", if take { "take from" } else { "give to" }, pile_name(g, id));
                let w = g.font.width(&l, s);
                if cx + w + 40.0 * GW * s > dx + dw {
                    break;
                }
                g.font.draw(&l, cx, y, s, if take { col::TEXT_GOOD } else { col::TEXT_HI });
                if text_button(&mut g.ui, &g.font, Rect::new(cx + w + 2.0 * s, y - 3.0 * s, 2.5 * GW * s, GH * s + 4.0 * s), "x", BtnState::Normal) {
                    act = HAct::Unlink(r.id, n, id);
                }
                cx += w + 4.0 * GW * s;
            }
        }
        let lb = 17.0 * GW * s;
        if text_button(&mut g.ui, &g.font, Rect::new(dx + dw - 2.0 * lb - 4.0 * s, y - 3.0 * s, lb, GH * s + 4.0 * s), "+ Take from pile", BtnState::Normal) {
            act = HAct::Link(r.id, n, true);
        }
        if text_button(&mut g.ui, &g.font, Rect::new(dx + dw - lb, y - 3.0 * s, lb, GH * s + 4.0 * s), "+ Give to pile", BtnState::Normal) {
            act = HAct::Link(r.id, n, false);
        }
        y = sy + per;
        if g.ui.lpressed && Rect::new(dx - 4.0 * s, sy - 3.0 * s, dw, per - 6.0 * s).contains(g.mouse()) {
            g.r12.stop_sel = Some(n);
        }
    }
    apply(g, act);
}

/// Look line of a track tile: "Carved track: north, east (corner)."
pub fn track_line(tt: &rf_core::TiletypeInfo) -> Option<String> {
    if tt.special != rf_core::TtSpecial::Track {
        return None;
    }
    let names: Vec<&str> = [('N', "north"), ('S', "south"), ('E', "east"), ('W', "west")].iter().filter(|(c, _)| tt.direction.contains(*c)).map(|e| e.1).collect();
    let ns = tt.direction.contains('N') || tt.direction.contains('S');
    let ew = tt.direction.contains('E') || tt.direction.contains('W');
    let shape = match names.len() {
        1 => "end",
        2 if ns && ew => "corner",
        2 => "straight",
        3 => "T-junction",
        4 => "crossing",
        _ => "",
    };
    let kind = if tt.material == rf_core::TtMaterial::Construction { "Constructed track" } else { "Carved track" };
    Some(format!("{kind}: {} ({shape}).", names.join(", ")))
}

/// " (track stop)" / " (track)" / " (no track)" for a stop position.
pub fn track_here(g: &Game, p: Pos) -> &'static str {
    let w = &g.sim.world;
    if w.buildings.iter().any(|b| b.kind == "Trap" && b.subtype == "TrackStop" && b.contains(p)) {
        " (track stop)"
    } else if w.tile(p).is_some() && w.tt(p).special == rf_core::TtSpecial::Track {
        " (track)"
    } else {
        " (no track)"
    }
}

fn apply(g: &mut Game, act: HAct) {
    match act {
        HAct::None => {}
        HAct::New => {
            let n = g.sim.world.haul_routes.len() + 1;
            let id = g.sim.create_haul_route(&format!("Route {n}"));
            g.r10.haul_sel = Some(id);
            g.r12.stop_sel = None;
        }
        HAct::Delete(id) => {
            g.sim.remove_haul_route(id);
            g.r10.haul_sel = None;
        }
        HAct::AddStop(r) => {
            g.screen = Screen::None;
            g.tool = Tool::RouteStop(r);
        }
        HAct::RemoveStop(r, k) => {
            g.sim.remove_route_stop(r, k);
            g.r12.stop_sel = None;
        }
        HAct::Go(p) => {
            g.screen = Screen::None;
            g.cam.focus(p);
            g.selected = Selected::Tile(p);
        }
        HAct::Link(r, k, take) => {
            g.screen = Screen::None;
            g.tool = Tool::RoutePile(r, k, take);
            g.r12.stop_sel = Some(k);
        }
        HAct::Unlink(r, k, pile) => {
            // No unlink call in the API: the stop's lists are plain world data.
            if let Some(st) = g.sim.world.haul_routes.iter_mut().find(|x| x.id == r).and_then(|x| x.stops.get_mut(k)) {
                st.take_from.retain(|&p| p != pile);
                st.give_to.retain(|&p| p != pile);
            }
        }
        HAct::Cond(r, k, c) => {
            let t = c.text();
            g.sim.set_stop_conditions(r, k, &[t.as_str()]);
            g.r12.stop_sel = Some(k);
        }
        HAct::ClearCond(r, k) => g.sim.set_stop_conditions(r, k, &[]),
        HAct::Assign(r, item) => {
            g.sim.assign_vehicle(r, item);
            g.r12.cart_pick = false;
        }
        HAct::Unassign(r, item) => {
            if let Some(x) = g.sim.world.haul_routes.iter_mut().find(|x| x.id == r) {
                x.vehicles.retain(|&v| v != item);
            }
        }
    }
}

// ====================================================================== map tools

/// Hint line for the round-12 map tools.
pub fn tool_hint(g: &Game) -> Option<String> {
    let rname = |id: i32| g.sim.world.haul_routes.iter().find(|r| r.id == id).map(route_name).unwrap_or_default();
    Some(match g.tool {
        Tool::RouteStop(r) => format!("{}: click a track stop or track tile for the new stop. Right-click: back to Hauling.", rname(r)),
        Tool::RoutePile(r, k, take) => format!("{}, stop {}: click a stockpile to {}. Right-click: back to Hauling.", rname(r), k + 1, if take { "take from" } else { "give to" }),
        Tool::AimSiege(b) => {
            let n = g.sim.world.building(b).map(|b| building_title(g, b)).unwrap_or_default();
            format!("{n}: click the tile to aim at. Right-click cancels.")
        }
        _ => return None,
    })
}

/// A left click on the map with a round-12 tool.
pub fn tool_click(g: &mut Game, h: Pos) {
    match g.tool {
        Tool::RouteStop(r) => {
            let shape = g.sim.world.tt(h).shape;
            if shape.is_solid() {
                g.set_status("A stop needs an open floor tile (a track stop or track).".into());
                return;
            }
            let n = g.sim.world.haul_routes.iter().find(|x| x.id == r).map(|x| x.stops.len()).unwrap_or(0);
            let k = g.sim.add_route_stop(r, &format!("Stop {}", n + 1), h);
            g.set_status(format!("Stop {} added{}", n + 1, track_here(g, h)));
            g.tool = Tool::None;
            g.screen = Screen::Hauling;
            g.r10.haul_sel = Some(r);
            g.r12.stop_sel = (k != usize::MAX).then_some(k);
        }
        Tool::RoutePile(r, k, take) => {
            let pile = g.sim.world.buildings.iter().find(|b| b.kind == "Stockpile" && b.contains(h)).map(|b| b.id);
            let Some(pile) = pile else {
                g.set_status("Click a stockpile.".into());
                return;
            };
            g.sim.link_stop_pile(r, k, pile, take);
            let n = g.sim.world.building(pile).map(|b| building_title(g, b)).unwrap_or_default();
            g.set_status(format!("Stop {} {} {n}", k + 1, if take { "takes from" } else { "gives to" }));
            g.tool = Tool::None;
            g.screen = Screen::Hauling;
            g.r10.haul_sel = Some(r);
        }
        Tool::AimSiege(b) => {
            aim(g, b, Some(h));
            g.tool = Tool::None;
            g.selected = Selected::Building(b);
        }
        _ => {}
    }
}

fn aim(g: &mut Game, b: i32, target: Option<Pos>) {
    g.sim.aim_siege_engine(b, target);
    match g.r12.aims.iter_mut().find(|e| e.0 == b) {
        Some(e) => e.1 = target,
        None => g.r12.aims.push((b, target)),
    }
    g.set_status(match target {
        Some(p) => format!("Aimed at {}, {}, z {}", p.x, p.y, p.z),
        None => "Fire at will".into(),
    });
}

/// Right-click with a round-12 tool: back where it came from. Returns true when used.
pub fn back(g: &mut Game) -> bool {
    match g.tool {
        Tool::RouteStop(r) | Tool::RoutePile(r, ..) => {
            g.tool = Tool::None;
            g.screen = Screen::Hauling;
            g.r10.haul_sel = Some(r);
        }
        Tool::AimSiege(b) => {
            g.tool = Tool::None;
            g.selected = Selected::Building(b);
        }
        _ => return false,
    }
    true
}

// ====================================================================== building sheets

/// Wiki power numbers per machine: (makes, uses).
fn power_numbers(b: &Building) -> (i32, i32) {
    use rf_sim::power as pw;
    let tiles = ((b.x2 - b.x1 + 1) * (b.y2 - b.y1 + 1)) as i32;
    match b.kind.as_str() {
        "WaterWheel" => (pw::WHEEL_POWER, pw::WHEEL_USE),
        "Windmill" => (40, 0),
        "GearAssembly" => (0, pw::GEAR_USE),
        "AxleHorizontal" => (0, pw::AXLE_USE * tiles),
        "AxleVertical" => (0, pw::AXLE_USE),
        "ScrewPump" => (0, pw::PUMP_USE),
        "Rollers" => (0, pw::ROLLER_USE * tiles),
        _ if b.subtype == "Millstone" => (0, pw::MILLSTONE_USE),
        _ => (0, 0),
    }
}

/// Ammunition item kind of a siege engine and its DF name.
fn ammo_of(sub: &str) -> (&'static str, &'static str, &'static str) {
    match sub {
        "Catapult" => ("BOULDER", "", "stones (boulders)"),
        "BoltThrower" => ("AMMO", "ITEM_AMMO_BOLTS", "bolts"),
        _ => ("SIEGEAMMO", "", "ballista arrows"),
    }
}

/// The round-12 part of the building sheet. Returns the new y.
pub fn sheet_section(g: &mut Game, b: &Building, x: f32, mut y: f32, bw: f32, _bottom: f32) -> f32 {
    if !has_section(b) {
        return y;
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let maxc = (bw / (GW * s)) as usize;
    let bh = GH * s + 8.0 * s;
    let line = |g: &Game, y: &mut f32, t: &str, c: Color| {
        for l in super::round9::wrap(t, maxc) {
            g.font.draw(&l, x, *y, s, c);
            *y += lh;
        }
    };
    if is_machine(b) {
        let (makes, uses) = power_numbers(b);
        if !b.built {
            line(g, &mut y, "Not built yet: no power.", col::TEXT_DIM);
        } else if g.sim.machine_powered(b.id) {
            line(g, &mut y, "Powered: its network makes enough power.", col::TEXT_GOOD);
        } else {
            let why = match b.kind.as_str() {
                "WaterWheel" => "Not turning: it needs flowing water right below it (still water does nothing).",
                "Windmill" => "Not turning: it needs wind (built outside, under the open sky).",
                _ => "Not powered: connect it to a water wheel or windmill with axles and gear assemblies.",
            };
            line(g, &mut y, why, col::TEXT_BAD);
        }
        let t = if makes > 0 { format!("Makes {makes} power when turning, uses {uses} itself.") } else { format!("Uses {uses} power.") };
        line(g, &mut y, &t, col::TEXT);
        match b.kind.as_str() {
            "ScrewPump" => {
                let d = b.direction.trim_start_matches("From").to_lowercase();
                let opp = match d.as_str() {
                    "north" => "south",
                    "south" => "north",
                    "east" => "west",
                    _ => "east",
                };
                line(g, &mut y, &format!("Pumps from the {d}: takes water or magma from one level below the {d} end and lets it out past the {opp} end."), col::TEXT_DIM);
                let manual = b.jobs.iter().any(|j| j.job == "OperatePump");
                let st = if manual { BtnState::Active } else { BtnState::Normal };
                if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, bh), if manual { "Pump by hand: on (a pump operator works it)" } else { "Pump by hand: off" }, st) {
                    g.sim.set_pump_manual(b.id, !manual);
                }
                y += bh + 4.0 * s;
            }
            "AxleHorizontal" | "WaterWheel" => line(g, &mut y, &format!("Axis: {}.", dir_label(&b.kind, if b.direction == "NS" || (b.direction.is_empty() && b.y2 > b.y1) { "NS" } else { "EW" })), col::TEXT_DIM),
            "GearAssembly" => line(g, &mut y, "Connects to machines on all six sides (also above and below); a lever can disengage it.", col::TEXT_DIM),
            "Rollers" => {
                line(g, &mut y, &format!("Pushes minecarts {}.", dir_label("Rollers", &b.direction).trim_start_matches("push ")), col::TEXT_DIM);
                y = dir_buttons(g, b, x, y, bw, rf_sim::build_api::ROLLER_DIRECTIONS);
            }
            _ => {}
        }
        let (nets, stats) = g.sim.power_report();
        let running = nets.iter().filter(|n| n.powered).count();
        line(g, &mut y, &format!("Fort: {} machines in {} networks ({running} running), {} sevenths pumped.", stats.machines, stats.networks, stats.pumped), col::TEXT_DIM);
        let st = if g.r12.power_overlay { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, bh), "Power overlay on the map", st) {
            g.r12.power_overlay = !g.r12.power_overlay;
        }
        y += bh + 6.0 * s;
    } else if b.kind == "SiegeEngine" {
        let (kind, sub, name) = ammo_of(&b.subtype);
        let w = &g.sim.world;
        let matches = |it: &rf_core::Item| it.kind == kind && (sub.is_empty() || it.subtype == sub);
        let fort: i32 = w.items.iter().filter(|it| matches(it) && !it.flags.forbid && it.building < 0 && it.holder < 0).map(|it| it.stack.max(1)).sum();
        let loaded: i32 = w.items.iter().filter(|it| matches(it) && it.building == b.id).map(|it| it.stack.max(1)).sum();
        let face = if b.direction.is_empty() { "N" } else { b.direction.as_str() };
        line(g, &mut y, &format!("Facing {}.", dir_label("SiegeEngine", face).trim_start_matches("facing ")), col::TEXT);
        let ac = if fort + loaded > 0 { col::TEXT } else { col::TEXT_BAD };
        line(g, &mut y, &format!("Ammo: {loaded} {name} loaded, {fort} more in the fort."), ac);
        let aim_now = g.r12.aims.iter().find(|e| e.0 == b.id).map(|e| e.1);
        let t = match aim_now {
            Some(Some(p)) => format!("Target: {}, {}, z {}", p.x, p.y, p.z),
            _ => "Fire at will (the operator picks targets).".into(),
        };
        line(g, &mut y, &t, col::TEXT_CYAN);
        if !b.built {
            line(g, &mut y, "Not built yet.", col::TEXT_DIM);
        }
        let half = (bw - 6.0 * s) / 2.0;
        let st = if g.tool == Tool::AimSiege(b.id) { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(x, y, half, bh), "Aim at a tile...", st) {
            g.tool = Tool::AimSiege(b.id);
        }
        let fst = if matches!(aim_now, None | Some(None)) { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(x + half + 6.0 * s, y, half, bh), "Fire at will", fst) {
            aim(g, b.id, None);
        }
        y += bh + 6.0 * s;
        line(g, &mut y, "A siege operator works the engine; catapults throw stones, ballistas fire ballista arrows.", col::TEXT_DIM);
    } else {
        // Track stop.
        line(g, &mut y, &format!("Track stop: {}.", dir_label("Trap", &b.direction)), col::TEXT);
        y = dir_buttons(g, b, x, y, bw, rf_sim::build_api::DUMP_DIRECTIONS);
        let routes: Vec<String> = g
            .sim
            .world
            .haul_routes
            .iter()
            .flat_map(|r| r.stops.iter().enumerate().filter(|(_, st)| b.contains(st.pos)).map(move |(k, _)| format!("{} stop {}", route_name(r), k + 1)))
            .collect();
        let t = if routes.is_empty() { "No hauling route stops here.".to_string() } else { format!("Route stops here: {}", routes.join(", ")) };
        line(g, &mut y, &t, col::TEXT_DIM);
        if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, bh), "Hauling routes...", BtnState::Normal) {
            g.toggle_screen(Screen::Hauling);
        }
        y += bh + 6.0 * s;
    }
    y
}

/// Direction buttons that set `Building::direction` (rollers, track stops).
fn dir_buttons(g: &mut Game, b: &Building, x: f32, y: f32, bw: f32, dirs: &[&str]) -> f32 {
    let s = g.ui.scale;
    let n = dirs.len() as f32;
    let w = (bw - (n - 1.0) * 4.0 * s) / n;
    let mut hit = None;
    for (k, d) in dirs.iter().enumerate() {
        let l = if d.is_empty() { "none".to_string() } else { d.to_string() };
        let st = if b.direction == *d { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(x + k as f32 * (w + 4.0 * s), y, w, GH * s + 6.0 * s), &l, st) {
            hit = Some(*d);
        }
    }
    if let Some(d) = hit {
        if let Some(bm) = g.sim.world.buildings.iter_mut().find(|x| x.id == b.id) {
            bm.direction = d.to_string();
        }
    }
    y + GH * s + 12.0 * s
}

// ====================================================================== map overlay

fn routes_visible(g: &Game) -> bool {
    g.screen == Screen::Hauling || matches!(g.tool, Tool::RouteStop(_) | Tool::RoutePile(..)) || matches!(g.selected, Selected::Building(id) if g.sim.world.building(id).map(|b| b.kind == "Trap" && b.subtype == "TrackStop").unwrap_or(false))
}

/// Route stops, assigned carts, the power overlay and siege engine targets.
pub fn draw_map_overlay(g: &mut Game) {
    let z = g.cam.z as i16;
    let tp = g.cam.tile_px();
    let (x0, y0, x1, y1) = crate::render::visible_range(g);
    // Power overlay.
    if g.r12.power_overlay {
        let list: Vec<(i16, i16, i16, i16, u8)> = g
            .sim
            .world
            .buildings
            .iter()
            .filter(|b| b.z == z && is_machine(b) && b.x2 >= x0 && b.x1 < x1 && b.y2 >= y0 && b.y1 < y1)
            .map(|b| (b.x1, b.y1, b.x2, b.y2, if !b.built { 0 } else if g.sim.machine_powered(b.id) { 1 } else { 2 }))
            .collect();
        for (bx1, by1, bx2, by2, st) in list {
            let c = match st {
                1 => Color::new(0.3, 0.95, 0.35, 1.0),
                2 => Color::new(1.0, 0.3, 0.25, 1.0),
                _ => Color::new(0.6, 0.6, 0.6, 1.0),
            };
            let a = g.cam.to_screen(bx1 as f32, by1 as f32);
            let e = g.cam.to_screen(bx2 as f32 + 1.0, by2 as f32 + 1.0);
            draw_rectangle(a.x, a.y, e.x - a.x, e.y - a.y, Color::new(c.r, c.g, c.b, 0.22));
            draw_rectangle_lines(a.x, a.y, e.x - a.x, e.y - a.y, (tp / 12.0).max(2.0), c);
        }
        let msg = "Power overlay: green = running, red = no power, grey = not built";
        let s = g.ui.scale;
        let w = g.font.width(msg, s) + 12.0 * s;
        let r = Rect::new((screen_width() - w) / 2.0, super::topbar::TOPBAR_H * s + 6.0 * s, w, GH * s + 8.0 * s);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.03, 0.03, 0.04, 0.85));
        g.font.draw(msg, r.x + 6.0 * s, r.y + 4.0 * s, s, col::TEXT);
    }
    // Track directions (while working with tracks or routes): a line from each track tile's
    // centre to every side it connects to.
    let track_mode = routes_visible(g)
        || matches!(g.tool, Tool::Construct(rf_sim::ConstructionKind::Track) | Tool::Detail(rf_sim::DetailKind::CarveTrack))
        || matches!(g.tool, Tool::Build(i) if g.build_opts.get(i).map(|o| matches!(o.kind, "Rollers") || o.subtype == "TrackStop").unwrap_or(false));
    if track_mode {
        let c = Color::new(0.45, 0.85, 1.0, 0.75);
        let th = (tp / 12.0).max(1.5);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = Pos::new(x, y, z);
                let Some(t) = g.sim.world.tile(p) else { continue };
                if t.hidden() {
                    continue;
                }
                let tt = g.sim.world.tiletypes.get(t.tt);
                if tt.special != rf_core::TtSpecial::Track {
                    continue;
                }
                let m = g.cam.to_screen(x as f32 + 0.5, y as f32 + 0.5);
                draw_circle(m.x, m.y, th, c);
                for (ch, dx, dy) in [('N', 0.0, -0.5), ('S', 0.0, 0.5), ('E', 0.5, 0.0), ('W', -0.5, 0.0)] {
                    if tt.direction.contains(ch) {
                        draw_line(m.x, m.y, m.x + dx * tp, m.y + dy * tp, th, c);
                    }
                }
            }
        }
    }
    // Carts assigned to routes: the moving ones (not lying on the ground) are drawn here,
    // all of them get a marker.
    let carts: Vec<(usize, i32)> = g.sim.world.haul_routes.iter().enumerate().flat_map(|(k, r)| r.vehicles.iter().map(move |&v| (k, v))).collect();
    let mut sprites = std::mem::take(&mut g.rend.buf);
    for (ri, v) in carts {
        let Some(it) = g.sim.world.items.iter().find(|it| it.id == v) else { continue };
        if it.pos.z != z || it.container >= 0 || it.flags.in_inventory {
            continue;
        }
        let r = crate::render::tile_rect(g, it.pos.x, it.pos.y);
        if !it.flags.on_ground {
            sprites.clear();
            g.gfx.item(&g.sim.world, it, &mut sprites);
            for sp in sprites.iter() {
                g.rend.draw_sprite(&g.gfx, sp, r.x, r.y, tp);
            }
        }
        let c = Color::new(0.45, 0.85, 1.0, 0.95);
        draw_rectangle_lines(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, (tp / 16.0).max(1.5), c);
        if tp >= 20.0 {
            g.font.draw_shadow(&format!("R{}", ri + 1), r.x + 2.0, r.y + r.h - GH * (tp / 32.0).max(0.75) - 1.0, (tp / 32.0).max(0.75), c);
        }
    }
    g.rend.buf = sprites;
    // Route stops.
    if routes_visible(g) {
        let sel = g.r10.haul_sel;
        let stops: Vec<(i32, usize, Pos, bool)> = g.sim.world.haul_routes.iter().flat_map(|r| r.stops.iter().enumerate().map(move |(k, st)| (r.id, k, st.pos, Some(r.id) == sel))).collect();
        let mut prev: Option<(i32, Pos)> = None;
        for (rid, k, p, is_sel) in stops {
            let c = if is_sel { Color::new(1.0, 0.85, 0.3, 0.95) } else { Color::new(0.75, 0.75, 0.8, 0.7) };
            if let Some((pr, pp)) = prev {
                if pr == rid && is_sel && (pp.z == z || p.z == z) {
                    let a = g.cam.to_screen(pp.x as f32 + 0.5, pp.y as f32 + 0.5);
                    let b = g.cam.to_screen(p.x as f32 + 0.5, p.y as f32 + 0.5);
                    draw_line(a.x, a.y, b.x, b.y, (tp / 16.0).max(1.5), Color::new(c.r, c.g, c.b, 0.5));
                }
            }
            prev = Some((rid, p));
            if p.z != z {
                continue;
            }
            let sp = g.cam.to_screen(p.x as f32, p.y as f32);
            draw_rectangle_lines(sp.x, sp.y, tp, tp, (tp / 10.0).max(2.0), c);
            g.font.draw_shadow(&format!("{}", k + 1), sp.x + 3.0, sp.y + 2.0, (tp / 32.0).max(0.75), c);
        }
    }
    // Aim target of the selected siege engine (or while aiming).
    let eng = match (g.tool, g.selected) {
        (Tool::AimSiege(b), _) => Some(b),
        (_, Selected::Building(b)) if g.sim.world.building(b).map(|x| x.kind == "SiegeEngine").unwrap_or(false) => Some(b),
        _ => None,
    };
    if let Some(bid) = eng {
        let target = match g.tool {
            Tool::AimSiege(_) => g.hover,
            _ => g.r12.aims.iter().find(|e| e.0 == bid).and_then(|e| e.1),
        };
        if let (Some(t), Some(b)) = (target, g.sim.world.building(bid)) {
            let c = Color::new(1.0, 0.3, 0.25, 0.95);
            let from = g.cam.to_screen(b.center.x as f32 + 0.5, b.center.y as f32 + 0.5);
            let to = g.cam.to_screen(t.x as f32 + 0.5, t.y as f32 + 0.5);
            if b.z == z || t.z == z {
                draw_line(from.x, from.y, to.x, to.y, (tp / 18.0).max(1.5), Color::new(c.r, c.g, c.b, 0.6));
            }
            if t.z == z {
                let r = tp * 0.4;
                draw_circle_lines(to.x, to.y, r, (tp / 14.0).max(2.0), c);
                draw_line(to.x - r * 1.3, to.y, to.x + r * 1.3, to.y, 2.0, c);
                draw_line(to.x, to.y - r * 1.3, to.x, to.y + r * 1.3, 2.0, c);
            }
        }
    }
}

// ====================================================================== --ui states

/// `--ui` states of round 12 (screenshots). Returns false for unknown keys.
pub fn ui_arg(g: &mut Game, k: &str, v: &str) -> bool {
    match k {
        "power" => g.r12.power_overlay = true,
        "r12-route" => demo_route(g),
        "r12-dir" => {
            // Direction index for the option of the active Build tool (`place:` before).
            if let Tool::Build(i) = g.tool {
                set_dir(g, i, v.parse().unwrap_or(0));
            }
        }
        "r12-machines" => demo_machines(g),
        "r12-track" => demo_track(g),
        "r12-stoptool" => {
            // The "add stop" map tool of the first route (shows stops and track directions).
            if let Some(r) = g.sim.world.haul_routes.first().map(|r| r.id) {
                g.screen = Screen::None;
                g.r10.haul_sel = Some(r);
                g.tool = Tool::RouteStop(r);
            }
        }
        "r12-select" => {
            // Select the first building of a kind (`r12-select:ScrewPump`).
            if let Some(b) = g.sim.world.buildings.iter().rev().find(|b| b.kind == v || b.subtype == v) {
                g.selected = Selected::Building(b.id);
                let p = b.center;
                g.cam.focus(p);
                g.cam.x = g.cam.tx;
                g.cam.y = g.cam.ty;
            }
        }
        "r12-aim" => {
            if let Selected::Building(b) = g.selected {
                let p = g.sim.world.building(b).map(|b| b.center.offset(0, -8, 0));
                if let Some(p) = p {
                    aim(g, b, Some(p));
                }
            }
        }
        _ => return false,
    }
    true
}

/// A demo route through the real API between the two stockpiles nearest the camera, with a
/// minecart (screenshots: an unused tool item of the fort is turned into a minecart, since
/// Trussroads has none).
fn demo_route(g: &mut Game) {
    let c = Pos::new(g.cam.x as i16, g.cam.y as i16, g.cam.z as i16);
    let mut piles: Vec<(i32, Pos, i32)> = g.sim.world.buildings.iter().filter(|b| b.kind == "Stockpile").map(|b| (b.id, b.center, (b.center.x - c.x).abs() as i32 + (b.center.y - c.y).abs() as i32 + 30 * (b.z - c.z).abs() as i32)).collect();
    piles.sort_by_key(|p| (p.2, p.0));
    let (Some(a), Some(b)) = (piles.first().copied(), piles.get(1).copied()) else { return };
    let r = g.sim.create_haul_route("Stone to the masons");
    let s0 = g.sim.add_route_stop(r, "Quarry", a.1);
    let s1 = g.sim.add_route_stop(r, "Workshop pile", b.1);
    g.sim.link_stop_pile(r, s0, a.0, true);
    g.sim.link_stop_pile(r, s1, b.0, false);
    g.sim.set_stop_conditions(r, s0, &["Push North when >= 100% full"]);
    g.sim.set_stop_conditions(r, s1, &["Push South after 1200 ticks when <= 0% full"]);
    if !g.sim.world.items.iter().any(is_vehicle) {
        if let Some(it) = g.sim.world.items.iter_mut().find(|it| it.kind == "TOOL" && it.subtype == "ITEM_TOOL_LARGE_POT" && it.flags.on_ground && it.container < 0) {
            it.subtype = "ITEM_TOOL_MINECART".into();
            it.desc = "minecart".into();
        }
    }
    if let Some(v) = g.sim.world.items.iter().find(|it| is_vehicle(it)).map(|it| it.id) {
        g.sim.assign_vehicle(r, v);
    }
    g.r10.haul_sel = Some(r);
}

/// A carved track loop around the camera (screenshots: the floor tiles' tiletypes are set to
/// DF's `*FloorTrack<dirs>`, like a finished "carve track" designation).
fn demo_track(g: &mut Game) {
    let c = Pos::new(g.cam.x as i16, g.cam.y as i16, g.cam.z as i16);
    let (x0, y0, x1, y1) = (c.x - 6, c.y - 1, c.x + 6, c.y + 2);
    let on = |x: i16, y: i16| (x0..=x1).contains(&x) && (y0..=y1).contains(&y) && (x == x0 || x == x1 || y == y0 || y == y1);
    let mut n = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if !on(x, y) {
                continue;
            }
            let p = Pos::new(x, y, c.z);
            let mut d = String::new();
            for (dx, dy, l) in [(0, -1, 'N'), (0, 1, 'S'), (1, 0, 'E'), (-1, 0, 'W')] {
                if on(x + dx, y + dy) {
                    d.push(l);
                }
            }
            let w = &g.sim.world;
            if w.tile(p).is_none() || w.tt(p).shape != rf_core::Shape::Floor {
                continue;
            }
            let pre = match w.tt(p).material {
                rf_core::TtMaterial::Construction => "Constructed",
                rf_core::TtMaterial::Mineral => "Mineral",
                _ => "Stone",
            };
            let Some(tt) = w.tiletypes.by_name(&format!("{pre}FloorTrack{d}")).or_else(|| w.tiletypes.by_name(&format!("StoneFloorTrack{d}"))) else { continue };
            if let Some(t) = g.sim.world.tile_mut(p) {
                t.tt = tt;
                n += 1;
            }
        }
    }
    g.set_status(format!("{n} track tiles"));
}

/// Demo machines around the camera (screenshots): a pump, a wheel, axles, a gear, rollers on
/// a track stop line, a ballista, a catapult and a bolt thrower, placed through the real
/// placement API and marked built.
fn demo_machines(g: &mut Game) {
    let opts = g.sim.build_options();
    let find = |k: &str, sub: &str| opts.iter().position(|o| o.kind == k && o.subtype == sub);
    let c = Pos::new(g.cam.x as i16, g.cam.y as i16, g.cam.z as i16);
    let plan: &[(&str, &str, i16, i16, i16, i16, &str)] = &[
        ("ScrewPump", "", -6, -3, -5, -3, "FromWest"),
        ("GearAssembly", "", -4, -3, -4, -3, ""),
        ("AxleHorizontal", "", -3, -3, 0, -3, ""),
        ("GearAssembly", "", 1, -3, 1, -3, ""),
        ("WaterWheel", "", 2, -3, 4, -3, "EW"),
        ("Rollers", "", -6, 0, -3, 0, "E"),
        ("Trap", "TrackStop", -2, 0, -2, 0, "E"),
        ("SiegeEngine", "Ballista", 0, 0, 2, 2, "N"),
        ("SiegeEngine", "Catapult", 4, 0, 6, 2, "NE"),
        ("SiegeEngine", "BoltThrower", 8, 0, 8, 0, "N"),
        ("AxleVertical", "", 8, -3, 8, -3, ""),
    ];
    let mut n = 0;
    for &(k, sub, ax, ay, bx, by, d) in plan {
        let Some(i) = find(k, sub) else { continue };
        let (mut lo, mut hi) = (c.offset(ax, ay, 0), c.offset(bx, by, 0));
        // The planned spot, else the nearest free one (rings around it).
        'search: for r in 0..14i16 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (l2, h2) = (lo.offset(dx, dy, 0), hi.offset(dx, dy, 0));
                    if g.sim.can_place_dir(i, l2, h2, d).is_ok() {
                        (lo, hi) = (l2, h2);
                        break 'search;
                    }
                }
            }
        }
        match g.sim.place_building_dir(i, lo, hi, d) {
            Ok(id) => {
                if let Some(b) = g.sim.world.buildings.iter_mut().find(|b| b.id == id) {
                    b.built = true;
                    b.stage = 1;
                }
                n += 1;
            }
            Err(e) => eprintln!("r12-machines: {k} {sub} at {lo:?}: {e}"),
        }
    }
    g.set_status(format!("{n} demo machines placed"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_round_trip_the_exporter_text() {
        for t in ["Push North when >= 100% full", "Guide East after 1200 ticks", "Ride West after 8400 ticks when <= 0% full", "Push South"] {
            assert_eq!(Cond::parse(t).text(), t);
        }
        let c = Cond::parse("Push South after 1200 ticks when <= 0% full");
        assert_eq!((c.mode, c.dir, c.timeout, c.when), ("Push", "South", 1200, 2));
        assert_eq!(c.words(), "push south when empty, after 1 day");
    }

    #[test]
    fn direction_words() {
        assert_eq!(dir_label("ScrewPump", "FromNorth"), "pump from north");
        assert_eq!(dir_label("Rollers", "E"), "push east");
        assert_eq!(dir_label("Trap", ""), "no dumping");
        assert_eq!(dir_label("SiegeEngine", "SW"), "facing south-west");
        assert_eq!(dir_vec("ScrewPump", "FromWest"), Some((1.0, 0.0)));
    }
}
