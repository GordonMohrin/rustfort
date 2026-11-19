//! [machines14] Mechanisms and machines UI: the lever/plate target picker (link jobs with
//! mechanisms), pressure plate settings and live state, pending/jammed device states, the
//! power network overlay (one colour per network, port links, +produced/-used labels,
//! disengaged gears, pump arrows) and the minecart route view polish (track stop friction,
//! roller arrows, track plates, devices that block carts).
//!
//! Everything goes through `rf_sim::machines14_api` and the round-4 API.

use super::{checkbox, clip, col, text_button, BtnState};
use crate::font::GH;
use crate::game::{Game, Screen, Selected};
use macroquad::prelude::*;
use rf_core::{Building, PlateInfo, Pos};
use rf_sim::machines14_api::PlateSettings;
use rf_sim::DevKind;
use std::cell::Cell;

thread_local! {
    /// Target hovered in the picker (outlined on the map).
    static HOVER: Cell<i32> = const { Cell::new(-1) };
    /// Kind filter of the picker (index into `FILTERS`).
    static FILTER: Cell<usize> = const { Cell::new(0) };
    /// The route view polish is shown.
    static ROUTES: Cell<bool> = const { Cell::new(false) };
}

/// Picker filters: label and the device kinds it shows.
const FILTERS: &[(&str, &[DevKind])] = &[
    ("All", &[]),
    ("Doors", &[DevKind::Door, DevKind::Hatch]),
    ("Bridges", &[DevKind::Bridge]),
    ("Floodgates", &[DevKind::Floodgate]),
    ("Grates/bars", &[DevKind::GrateWall, DevKind::GrateFloor, DevKind::BarsVertical, DevKind::BarsFloor]),
    ("Gears", &[DevKind::Gear]),
    ("Track stops", &[DevKind::TrackStop]),
    ("Other", &[DevKind::Spikes, DevKind::Cage, DevKind::Chain, DevKind::Support]),
];

/// Network colours of the power overlay.
const NET_COLORS: [(f32, f32, f32); 8] = [(0.3, 0.9, 0.4), (0.3, 0.7, 1.0), (1.0, 0.75, 0.25), (0.85, 0.45, 1.0), (0.25, 0.95, 0.9), (1.0, 0.5, 0.6), (0.75, 0.9, 0.3), (0.95, 0.95, 0.95)];

fn is_plate(b: &Building) -> bool {
    b.kind == "Trap" && b.subtype == "PressurePlate"
}

fn is_trigger(b: &Building) -> bool {
    b.kind == "Trap" && matches!(b.subtype.as_str(), "Lever" | "PressurePlate")
}

/// DF words for a device state.
pub fn state_word(k: DevKind, open: bool) -> &'static str {
    match (k, open) {
        (DevKind::Bridge, true) => "raised",
        (DevKind::Bridge, false) => "lowered",
        (DevKind::Spikes, true) => "retracted",
        (DevKind::Spikes, false) => "extended",
        (DevKind::Gear, true) => "disengaged",
        (DevKind::Gear, false) => "engaged",
        (DevKind::TrackStop, true) => "switched off",
        (DevKind::TrackStop, false) => "active",
        (DevKind::Cage | DevKind::Chain | DevKind::Support, _) => "ready",
        (_, true) => "open",
        (_, false) => "closed",
    }
}

/// What a pending change does, for "raising in 37 ticks".
fn pending_word(k: DevKind, want: bool) -> &'static str {
    match (k, want) {
        (DevKind::Bridge, true) => "raising",
        (DevKind::Bridge, false) => "lowering",
        (DevKind::Spikes, true) => "retracting",
        (DevKind::Spikes, false) => "extending",
        (DevKind::TrackStop, false) => "switching on",
        (_, true) => "opening",
        (_, false) => "closing",
    }
}

/// Weight range in DF's dialog scale (DF shows the values divided by 10).
pub fn weight_label(v: i32) -> String {
    let v = v / 10;
    if v >= 1000 {
        format!("{}k", v / 1000)
    } else {
        v.to_string()
    }
}

/// Plate settings in one line ("creatures 1k-200k, water 1-7, one-use").
pub fn plate_summary(st: &PlateSettings) -> String {
    let mut v: Vec<String> = Vec::new();
    if st.units {
        v.push(format!("creatures {}-{}{}", weight_label(st.r.unit_min), weight_label(st.r.unit_max), if st.citizens { " incl. citizens" } else { "" }));
    }
    if st.water {
        v.push(format!("water {}-{}", st.r.water_min, st.r.water_max));
    }
    if st.magma {
        v.push(format!("magma {}-{}", st.r.magma_min, st.r.magma_max));
    }
    if st.track {
        let max = if st.r.track_max <= 0 { "any".to_string() } else { st.r.track_max.to_string() };
        v.push(format!("minecart {}-{max}", st.r.track_min.max(1)));
    }
    if !st.resets {
        v.push("one-use".into());
    }
    v.join(", ")
}

/// Ranges of the creature weight steps (DF dialog: 10 000 .. 2 000 000).
const WEIGHT_STEPS: [i32; 8] = [10_000, 50_000, 100_000, 200_000, 500_000, 1_000_000, 1_500_000, 2_000_000];

fn next_step(v: i32, up: bool) -> i32 {
    let k = WEIGHT_STEPS.iter().position(|&s| s >= v).unwrap_or(WEIGHT_STEPS.len() - 1);
    if up {
        WEIGHT_STEPS[(k + 1).min(WEIGHT_STEPS.len() - 1)]
    } else {
        WEIGHT_STEPS[k.saturating_sub(1)]
    }
}

enum Act {
    None,
    Link(i32),
    Cancel(i32),
    Plate(PlateSettings),
    Filter,
}

/// Sheet section for triggers (picker, link jobs, plate settings) and linked devices (pending,
/// jammed). Called at the end of `devices::sheet_section`. Returns the new y.
pub fn sheet_section(g: &mut Game, b: &Building, x: f32, mut y: f32, bw: f32, bottom: f32) -> f32 {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let maxc = (bw / (8.0 * s)) as usize;
    let bh = GH * s + 8.0 * s;
    let mut act = Act::None;
    HOVER.with(|h| h.set(-1));
    // Linked device: pending change, jam, one-shot warning.
    if let Some(d) = g.sim.device_state(b.id) {
        let mut t = format!("Mechanism: {}", state_word(d.kind, d.open));
        if let Some((due, want)) = d.pending {
            t = format!("{t}, {} in {} ticks", pending_word(d.kind, want), due.saturating_sub(g.sim.ticks()));
        }
        g.font.draw(&clip(&t, maxc), x, y, s, col::TEXT_CYAN);
        y += lh;
        if d.jammed {
            g.font.draw(&clip("Jammed: an item on the tile keeps it from closing", maxc), x, y, s, col::TEXT_BAD);
            y += lh;
        }
        if matches!(d.kind, DevKind::Cage | DevKind::Chain | DevKind::Support) {
            let w = match d.kind {
                DevKind::Cage => "One-shot: a linked signal opens the cage and dismantles it",
                DevKind::Chain => "One-shot: a linked signal releases the chained creature",
                _ => "One-shot: a linked signal pulls the support down (cave-in!)",
            };
            g.font.draw(&clip(w, maxc), x, y, s, col::TEXT_HI);
            y += lh;
        }
    }
    // Pressure plate: settings (editable until built) and live state.
    if is_plate(b) {
        if let Some(st) = g.sim.plate_settings(b.id) {
            g.font.draw("Plate triggers", x, y, s, col::BORDER_HI);
            y += lh;
            if b.built {
                g.font.draw(&clip(&plate_summary(&st), maxc), x, y, s, col::TEXT);
                y += lh;
                if let Some(ps) = g.sim.plate_state(b.id) {
                    let t = match (ps.pressed, ps.closing_in) {
                        (true, Some(n)) => format!("Pressed, closing in {n} ticks"),
                        (true, None) => "Pressed".to_string(),
                        _ => "Ready".to_string(),
                    };
                    g.font.draw(&t, x, y, s, if ps.pressed { col::TEXT_HI } else { col::TEXT_GOOD });
                    y += lh;
                }
                g.font.draw(&clip("Settings are fixed once built (DF)", maxc), x, y, s, col::TEXT_DIM);
                y += lh;
            } else {
                let mut n = st;
                let half = bw / 2.0;
                let rows: [(&str, bool, usize); 6] = [("Creatures", st.units, 0), ("Water", st.water, 1), ("Magma", st.magma, 2), ("Minecarts", st.track, 3), ("Citizens", st.citizens, 4), ("Resets", st.resets, 5)];
                for (k, (label, on, which)) in rows.iter().enumerate() {
                    let cx = if k % 2 == 0 { x } else { x + half };
                    if checkbox(&mut g.ui, &g.font, cx, y, *on, label) {
                        match which {
                            0 => n.units = !on,
                            1 => n.water = !on,
                            2 => n.magma = !on,
                            3 => n.track = !on,
                            4 => n.citizens = !on,
                            _ => n.resets = !on,
                        }
                        act = Act::Plate(n);
                    }
                    if k % 2 == 1 {
                        y += lh + 2.0 * s;
                    }
                }
                if st.units {
                    let q = (bw - 12.0 * s) / 4.0;
                    let labels = ["min -", "min +", "max -", "max +"];
                    for (k, l) in labels.iter().enumerate() {
                        if text_button(&mut g.ui, &g.font, Rect::new(x + k as f32 * (q + 4.0 * s), y, q, bh), l, BtnState::Normal) {
                            let mut r: PlateInfo = st.r;
                            match k {
                                0 => r.unit_min = next_step(r.unit_min, false),
                                1 => r.unit_min = next_step(r.unit_min, true).min(r.unit_max),
                                2 => r.unit_max = next_step(r.unit_max, false).max(r.unit_min),
                                _ => r.unit_max = next_step(r.unit_max, true),
                            }
                            act = Act::Plate(PlateSettings { r, ..st });
                        }
                    }
                    y += bh + 4.0 * s;
                }
                g.font.draw(&clip(&plate_summary(&st), maxc), x, y, s, col::TEXT);
                y += lh;
                if !st.resets {
                    g.font.draw(&clip("One-use plates destroy their mechanisms", maxc), x, y, s, col::TEXT_HI);
                    y += lh;
                }
            }
            y += 4.0 * s;
        }
    }
    // Trigger: the target picker and the link jobs.
    if is_trigger(b) && b.built {
        let f = FILTER.with(|c| c.get());
        let (fl, kinds) = FILTERS[f];
        let fr = Rect::new(x, y, bw, bh);
        if text_button(&mut g.ui, &g.font, fr, &format!("Link targets: {fl} (click to change)"), BtnState::Normal) {
            act = Act::Filter;
        }
        y += bh + 4.0 * s;
        let targets: Vec<_> = g.sim.linkable_targets(b.id).into_iter().filter(|t| kinds.is_empty() || kinds.contains(&t.kind)).collect();
        if targets.is_empty() {
            g.font.draw("Nothing to link nearby", x, y, s, col::TEXT_DIM);
            y += lh;
        }
        for t in targets.iter().take(8) {
            if y > bottom - 4.0 * lh {
                break;
            }
            let badge = if t.already_linked {
                " [linked]"
            } else if t.queued {
                " [queued]"
            } else {
                ""
            };
            let txt = format!("{} ({}) {},{} z{}{badge}", t.label, state_word(t.kind, t.open), t.pos.x, t.pos.y, t.pos.z);
            let rr = Rect::new(x, y - 1.0 * s, bw, lh);
            if g.ui.hovered(rr) {
                HOVER.with(|h| h.set(t.bid));
            }
            if super::list_row(&mut g.ui, rr, false) && !t.already_linked && !t.queued {
                act = Act::Link(t.bid);
            }
            let c = if t.already_linked { col::TEXT_DIM } else if t.queued { col::TEXT_HI } else { col::TEXT_CYAN };
            g.font.draw(&clip(&txt, maxc), x + 4.0 * s, y, s, c);
            y += lh;
        }
        let jobs: Vec<_> = g.sim.link_jobs().into_iter().filter(|j| j.trigger == b.id).collect();
        let free = g.sim.free_mechanisms();
        let need = 2 * jobs.iter().filter(|j| j.phase < 2).count() + jobs.iter().filter(|j| j.phase >= 2).count();
        let t = format!("Mechanisms: {free} free, {need} needed by queued links (2 per link)");
        g.font.draw(&clip(&t, maxc), x, y + 2.0 * s, s, if free >= need.max(2) { col::TEXT_DIM } else { col::TEXT_BAD });
        y += lh + 2.0 * s;
        for j in jobs.iter().take(4) {
            let name = g.sim.world.building(j.target).map(|o| super::words(&o.kind).to_lowercase()).unwrap_or_default();
            let step = match (j.phase, j.worker) {
                (_, None) => "waiting for a mechanic",
                (0, _) => "fetching the 1st mechanism",
                (1, _) => "installing at the target",
                (2, _) => "fetching the 2nd mechanism",
                _ => "installing at the trigger",
            };
            let rr = Rect::new(x, y - 1.0 * s, bw, lh);
            if super::list_row(&mut g.ui, rr, false) {
                act = Act::Cancel(j.target);
            }
            if g.ui.hovered(rr) {
                g.ui.tooltip = Some(("Click to cancel this link job".into(), vec2(rr.x, rr.y - 4.0)));
            }
            g.font.draw(&clip(&format!("Link to {name}: {step}"), maxc), x + 4.0 * s, y, s, col::TEXT_HI);
            y += lh;
        }
    }
    match act {
        Act::None => {}
        Act::Filter => FILTER.with(|c| c.set((c.get() + 1) % FILTERS.len())),
        Act::Link(t) => match g.sim.link_building(b.id, t) {
            Ok(()) => g.set_status("Link queued: a mechanic installs 2 mechanisms".into()),
            Err(e) => g.set_status(format!("Cannot link: {e}")),
        },
        Act::Cancel(t) => {
            g.sim.cancel_link_job(b.id, t);
            g.set_status("Link job cancelled".into());
        }
        Act::Plate(st) => {
            if let Err(e) = g.sim.set_plate_settings(b.id, st) {
                g.set_status(format!("Plate: {e}"));
            }
        }
    }
    y
}

fn outline(g: &Game, x1: i16, y1: i16, x2: i16, y2: i16, c: Color, w: f32) {
    let a = crate::render::tile_rect(g, x1, y1);
    let e = crate::render::tile_rect(g, x2, y2);
    draw_rectangle_lines(a.x, a.y, e.x + e.w - a.x, e.y + e.h - a.y, w, c);
}

/// Map drawing: hovered picker target, power networks, route view polish.
pub fn draw_map_overlay(g: &mut Game) {
    let z = g.cam.z as i16;
    let tp = g.cam.tile_px();
    let lw = (tp / 12.0).max(2.0);
    let hov = HOVER.with(|h| h.get());
    if hov >= 0 {
        if let Some(b) = g.sim.world.building(hov) {
            if b.z == z {
                let (x1, y1, x2, y2) = (b.x1, b.y1, b.x2, b.y2);
                outline(g, x1, y1, x2, y2, Color::new(1.0, 0.9, 0.3, 1.0), lw * 1.5);
            }
        }
    }
    if g.r12.power_overlay {
        draw_power(g, z, tp, lw);
    }
    let routes = g.screen == Screen::Hauling
        || ROUTES.with(|r| r.get())
        || matches!(g.selected, Selected::Building(id) if g.sim.world.building(id).map(|b| b.kind == "Trap" && b.subtype == "TrackStop").unwrap_or(false));
    if routes {
        draw_routes(g, z, tp, lw);
    }
}

fn draw_power(g: &mut Game, z: i16, tp: f32, lw: f32) {
    let s = (tp / 32.0).max(0.75);
    let nets = g.sim.power_networks();
    for n in &nets {
        let (r, gr, bl) = NET_COLORS[n.id as usize % NET_COLORS.len()];
        let c = Color::new(r, gr, bl, 0.9);
        for &(a, b) in &n.links {
            let (Some(ba), Some(bb)) = (g.sim.world.building(a), g.sim.world.building(b)) else { continue };
            if ba.z != z && bb.z != z {
                continue;
            }
            let p = g.cam.to_screen(ba.center.x as f32 + 0.5, ba.center.y as f32 + 0.5);
            let q = g.cam.to_screen(bb.center.x as f32 + 0.5, bb.center.y as f32 + 0.5);
            draw_line(p.x, p.y, q.x, q.y, lw * 0.8, c);
        }
        for &m in &n.machines {
            let Some(b) = g.sim.world.building(m) else { continue };
            if b.z != z {
                continue;
            }
            let (x1, y1, x2, y2, center, kind) = (b.x1, b.y1, b.x2, b.y2, b.center, b.kind.clone());
            let disengaged = b.flags.iter().any(|f| f == "DISENGAGED");
            outline(g, x1, y1, x2, y2, Color::new(r, gr, bl, 0.6), lw * 0.6);
            if disengaged {
                let a = crate::render::tile_rect(g, x1, y1);
                let red = Color::new(1.0, 0.3, 0.25, 1.0);
                draw_line(a.x, a.y, a.x + a.w, a.y + a.h, lw, red);
                draw_line(a.x + a.w, a.y, a.x, a.y + a.h, lw, red);
            }
            if kind == "ScrewPump" {
                if let Some((i, o)) = g.sim.pump_ends(m) {
                    let p = g.cam.to_screen(i.x as f32 + 0.5, i.y as f32 + 0.5);
                    let q = g.cam.to_screen(o.x as f32 + 0.5, o.y as f32 + 0.5);
                    draw_line(p.x, p.y, q.x, q.y, lw * 0.7, Color::new(0.4, 0.7, 1.0, 0.9));
                    draw_circle(q.x, q.y, lw * 1.4, Color::new(0.4, 0.7, 1.0, 0.9));
                }
            }
            if kind == "WaterWheel" && !g.sim.machine_powered(m) && tp >= 16.0 {
                let a = g.cam.to_screen(center.x as f32, center.y as f32 + 1.0);
                g.font.draw_shadow("no flow", a.x, a.y, s, col::TEXT_BAD);
            }
        }
        if let Some(b) = n.machines.first().and_then(|&m| g.sim.world.building(m)) {
            if b.z == z && tp >= 12.0 {
                let a = g.cam.to_screen(b.x1 as f32, b.y1 as f32 - 0.6);
                let t = format!("+{} / -{}", n.produced, n.used);
                g.font.draw_shadow(&t, a.x, a.y, s, if n.powered { col::TEXT_GOOD } else { col::TEXT_BAD });
            }
        }
    }
}

fn draw_routes(g: &mut Game, z: i16, tp: f32, lw: f32) {
    let s = (tp / 32.0).max(0.75);
    let list: Vec<Building> = g.sim.world.buildings.iter().filter(|b| b.z == z && b.built).cloned().collect();
    for b in &list {
        let r = crate::render::tile_rect(g, b.x1, b.y1);
        let stop = b.kind == "TrackStop" || (b.kind == "Trap" && b.subtype == "TrackStop");
        if stop {
            let off = b.flags.iter().any(|f| f == "DISABLED");
            let fr = if b.friction > 0 { b.friction } else { rf_sim::minecarts::stop_friction(&b.subtype) };
            let l = match fr {
                f if f >= 50_000 => "H+",
                f if f >= 10_000 => "H",
                f if f >= 500 => "M",
                f if f >= 50 => "L",
                f if f > 0 => "L-",
                _ => "0",
            };
            g.font.draw_shadow(if off { "off" } else { l }, r.x + 2.0, r.y + 2.0, s, if off { col::TEXT_DIM } else { col::TEXT_HI });
        }
        if b.kind == "Rollers" {
            let on = g.sim.machine_powered(b.id);
            let c = if on { Color::new(0.3, 0.95, 0.35, 0.95) } else { Color::new(0.6, 0.6, 0.6, 0.8) };
            let (dx, dy) = match b.direction.to_ascii_uppercase().as_str() {
                "N" | "NORTH" => (0.0, -1.0),
                "S" | "SOUTH" => (0.0, 1.0),
                "W" | "WEST" => (-1.0, 0.0),
                _ => (1.0, 0.0),
            };
            let m = g.cam.to_screen(b.center.x as f32 + 0.5, b.center.y as f32 + 0.5);
            draw_line(m.x - dx * tp * 0.35, m.y - dy * tp * 0.35, m.x + dx * tp * 0.35, m.y + dy * tp * 0.35, lw, c);
            draw_circle(m.x + dx * tp * 0.35, m.y + dy * tp * 0.35, lw * 1.2, c);
        }
        if is_plate(b) {
            if let Some(st) = g.sim.plate_settings(b.id).filter(|st| st.track) {
                let max = if st.r.track_max <= 0 { "any".to_string() } else { st.r.track_max.to_string() };
                g.font.draw_shadow(&format!("T {}-{max}", st.r.track_min.max(1)), r.x + 2.0, r.y + 2.0, s, col::TEXT_CYAN);
            }
        }
        // Devices that stop carts like a wall.
        let blocks = match g.sim.device_state(b.id) {
            Some(d) => match d.kind {
                DevKind::Door | DevKind::Floodgate | DevKind::GrateWall | DevKind::BarsVertical => !d.open,
                DevKind::Bridge => d.open,
                _ => false,
            },
            None => false,
        };
        if blocks {
            outline(g, b.x1, b.y1, b.x2, b.y2, Color::new(1.0, 0.3, 0.25, 0.9), lw);
        }
    }
    // Moving carts: a velocity arrow (length = speed).
    for (pos, dir, v) in g.sim.cart_motion() {
        if pos.z != z {
            continue;
        }
        let (dx, dy) = dir;
        let m = g.cam.to_screen(pos.x as f32 + 0.5, pos.y as f32 + 0.5);
        let len = (v as f32 / 50_000.0).clamp(0.1, 1.5) * tp;
        draw_line(m.x, m.y, m.x + dx as f32 * len, m.y + dy as f32 * len, lw, Color::new(1.0, 0.85, 0.3, 0.95));
    }
}

/// `--ui` states of machines14 (screenshots). Returns false for unknown keys.
pub fn ui_arg(g: &mut Game, k: &str, _v: &str) -> bool {
    match k {
        "m14-power" => g.r12.power_overlay = true,
        "m14-routes" => ROUTES.with(|r| r.set(true)),
        "m14-link" | "m14-plate" => {
            let want = if k == "m14-link" { "Lever" } else { "PressurePlate" };
            if let Some(b) = g.sim.world.buildings.iter().find(|b| b.kind == "Trap" && b.subtype == want) {
                let (id, p) = (b.id, b.center);
                g.selected = Selected::Building(id);
                g.cam.focus(p);
                g.cam.x = g.cam.tx;
                g.cam.y = g.cam.ty;
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plate_summary_and_steps() {
        let st = PlateSettings { units: true, citizens: true, resets: false, r: PlateInfo { unit_min: 10_000, unit_max: 2_000_000, ..Default::default() }, ..Default::default() };
        assert_eq!(plate_summary(&st), "creatures 1k-200k incl. citizens, one-use");
        assert_eq!(next_step(10_000, true), 50_000);
        assert_eq!(next_step(2_000_000, true), 2_000_000);
        assert_eq!(next_step(10_000, false), 10_000);
        assert_eq!(state_word(DevKind::Bridge, true), "raised");
        assert_eq!(pending_word(DevKind::Spikes, false), "extending");
        let _ = Pos::new(0, 0, 0);
    }
}
