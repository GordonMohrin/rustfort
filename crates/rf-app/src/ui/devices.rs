//! [round 4] Building sheet sections for doors, hatches, floodgates, levers, pressure
//! plates, bridges and traps (like the Steam building window), the lever "Link to..." tool
//! and the map highlight of linked buildings.
//!
//! The actions go through the round-4 API: `Sim::set_building_flag` (doors and hatches),
//! `Sim::pull_lever`, `Sim::link_building`.

use super::{checkbox, clip, col, pretty, text_button, BtnState};
use crate::font::GH;
use crate::game::{Game, Selected, Tool};
use macroquad::prelude::*;
use rf_core::{Building, Pos, World};

/// Door-like buildings with Lock / Forbid / Pet-passable (DF door and hatch flags).
pub fn is_door(b: &Building) -> bool {
    matches!(b.kind.as_str(), "Door" | "Hatch")
}

/// Levers and pressure plates trigger linked buildings.
pub fn is_trigger(b: &Building) -> bool {
    b.kind == "Trap" && matches!(b.subtype.as_str(), "Lever" | "PressurePlate")
}

/// Buildings a lever can be linked to (DF wiki "Lever": bridges, doors, floodgates, hatches,
/// cages, chains, grates, bars, supports, spikes, gear assemblies, track stops).
pub fn linkable(b: &Building) -> bool {
    matches!(
        b.kind.as_str(),
        "Bridge" | "Door" | "Floodgate" | "Hatch" | "Cage" | "Chain" | "GrateWall" | "GrateFloor" | "BarsVertical" | "BarsFloor" | "Support" | "Weapon" | "GearAssembly"
    ) || (b.kind == "Trap" && b.subtype == "TrackStop")
}

/// Has a device section on the building sheet.
pub fn is_device(b: &Building) -> bool {
    is_door(b) || linkable(b) || b.kind == "Trap"
}

pub fn has_flag(b: &Building, f: &str) -> bool {
    b.flags.iter().any(|x| x == f)
}

/// Short state of a device, DF words: `raised`, `closed, locked`, `pulled`.
pub fn device_state(b: &Building) -> String {
    let f = |x: &str| has_flag(b, x);
    let mut v: Vec<&str> = Vec::new();
    match b.kind.as_str() {
        "Bridge" => v.push(if f("RAISED") || f("ACTIVE") { "raised" } else { "lowered" }),
        "Door" | "Hatch" | "Floodgate" => {
            if f("CLOSED") {
                v.push("closed");
            } else if f("OPEN") || f("ACTIVE") {
                v.push("open");
            }
            if f("LOCKED") {
                v.push("locked");
            }
            if f("FORBIDDEN") {
                v.push("forbidden");
            }
            if f("PET_PASSABLE") {
                v.push("pet-passable");
            }
        }
        "Trap" if is_trigger(b) => v.push(if f("ACTIVE") { "pulled" } else { "not pulled" }),
        "Trap" => {
            if f("LOADED") || f("ACTIVE") {
                v.push("armed");
            } else if f("TRIGGERED") {
                v.push("triggered");
            }
        }
        _ => {
            if f("ACTIVE") {
                v.push("active");
            }
        }
    }
    v.join(", ")
}

/// Bridge raise direction in DF's words.
pub fn bridge_direction(d: &str) -> &'static str {
    match d.to_ascii_uppercase().as_str() {
        "N" | "NORTH" | "UP" => "raises to the north",
        "S" | "SOUTH" | "DOWN" => "raises to the south",
        "E" | "EAST" | "RIGHT" => "raises to the east",
        "W" | "WEST" | "LEFT" => "raises to the west",
        "RETRACT" | "RETRACTING" => "retracts",
        "" => "direction unknown",
        _ => "raises",
    }
}

/// Trap subtype name like DF: `Cage trap`, `Stone-fall trap`, `Weapon trap`, `Lever`.
pub fn trap_name(sub: &str) -> String {
    match sub {
        "CageTrap" => "Cage trap".into(),
        "StoneFallTrap" => "Stone-fall trap".into(),
        "WeaponTrap" => "Weapon trap".into(),
        "PressurePlate" => "Pressure plate".into(),
        "TrackStop" => "Track stop".into(),
        "Lever" => "Lever".into(),
        s => super::words(s),
    }
}

/// Mechanisms in stock (DF: linking needs two, one for each end).
pub fn mechanisms(w: &World) -> usize {
    w.items.iter().filter(|it| it.kind == "TRAPPARTS" && !it.flags.construction && !it.flags.in_building && !it.flags.forbid).count()
}

/// Building ids linked to `b` in either direction (lever -> targets, target -> levers).
pub fn links_of(w: &World, b: &Building) -> Vec<i32> {
    let mut v: Vec<i32> = b.links.iter().copied().filter(|id| *id != b.id).collect();
    for o in &w.buildings {
        if o.id != b.id && o.links.contains(&b.id) && !v.contains(&o.id) {
            v.push(o.id);
        }
    }
    v
}

enum DevAct {
    None,
    Flag(&'static str, bool),
    Pull,
    LinkMode,
    Select(i32),
}

/// The device part of the building sheet. Returns the new y.
pub fn sheet_section(g: &mut Game, b: &Building, x: f32, mut y: f32, bw: f32, bottom: f32) -> f32 {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let maxc = (bw / (8.0 * s)) as usize;
    let bh = GH * s + 8.0 * s;
    let mut act = DevAct::None;
    let st = device_state(b);
    if b.kind == "Bridge" {
        let d = bridge_direction(&b.direction);
        let mut c = d.chars();
        let d = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
        g.font.draw(&clip(&format!("{d} when pulled"), maxc), x, y, s, col::BORDER_HI);
        y += lh;
    }
    if !st.is_empty() {
        let c = if st.contains("locked") || st.contains("raised") || st.contains("forbidden") { col::TEXT_HI } else { col::TEXT_GOOD };
        g.font.draw(&clip(&format!("State: {st}"), maxc), x, y, s, c);
        y += lh;
    }
    y += 4.0 * s;
    if is_door(b) {
        let locked = has_flag(b, "LOCKED");
        let forb = has_flag(b, "FORBIDDEN");
        let half = (bw - 6.0 * s) / 2.0;
        let r1 = Rect::new(x, y, half, bh);
        if text_button(&mut g.ui, &g.font, r1, if locked { "Unlock" } else { "Lock" }, if locked { BtnState::Active } else { BtnState::Normal }) {
            act = DevAct::Flag("LOCKED", !locked);
        }
        if g.ui.hovered(r1) {
            g.ui.tooltip = Some(("Locked: nobody of the fort passes; building destroyers can still break it".into(), vec2(r1.x, r1.y - 4.0)));
        }
        let r2 = Rect::new(x + half + 6.0 * s, y, half, bh);
        if text_button(&mut g.ui, &g.font, r2, if forb { "Allow passage" } else { "Forbid passage" }, if forb { BtnState::Active } else { BtnState::Normal }) {
            act = DevAct::Flag("FORBIDDEN", !forb);
        }
        if g.ui.hovered(r2) {
            g.ui.tooltip = Some(("Forbidden: your dwarves path around it; invaders avoid forbidden doors".into(), vec2(r2.x, r2.y - 4.0)));
        }
        y += bh + 6.0 * s;
        let pet = has_flag(b, "PET_PASSABLE");
        if checkbox(&mut g.ui, &g.font, x, y, pet, "Pet-passable") {
            act = DevAct::Flag("PET_PASSABLE", !pet);
        }
        let internal = has_flag(b, "INTERNAL");
        if checkbox(&mut g.ui, &g.font, x + bw / 2.0, y, internal, "Internal") {
            act = DevAct::Flag("INTERNAL", !internal);
        }
        y += lh + 6.0 * s;
    }
    if is_trigger(b) {
        let half = (bw - 6.0 * s) / 2.0;
        if b.subtype == "Lever" {
            let pr = Rect::new(x, y, half, bh);
            if text_button(&mut g.ui, &g.font, pr, "Pull lever", BtnState::Normal) {
                act = DevAct::Pull;
            }
            if g.ui.hovered(pr) {
                g.ui.tooltip = Some(("Toggles every linked bridge, door, floodgate and hatch".into(), vec2(pr.x, pr.y - 4.0)));
            }
        }
        let linking = g.tool == Tool::Link(b.id);
        let lx = if b.subtype == "Lever" { x + half + 6.0 * s } else { x };
        if text_button(&mut g.ui, &g.font, Rect::new(lx, y, half, bh), "Link to...", if linking { BtnState::Active } else { BtnState::Normal }) {
            act = DevAct::LinkMode;
        }
        y += bh + 6.0 * s;
    }
    // Contents (traps: weapons, stone, caged creature).
    if b.kind == "Trap" && !is_trigger(b) {
        let w = &g.sim.world;
        let mut lines: Vec<(String, Color)> = Vec::new();
        for id in &b.items {
            if let Some(it) = w.items.iter().find(|it| it.id == *id) {
                if it.kind == "TRAPPARTS" {
                    continue;
                }
                let d = if !it.desc.is_empty() { it.desc.clone() } else { pretty(&it.kind).to_lowercase() };
                lines.push((d, col::TEXT));
            }
        }
        for u in w.units.iter().filter(|u| u.pos == b.center && u.flags.caged && !u.flags.dead) {
            let hostile = g.hostiles.binary_search(&u.id).is_ok();
            let t = format!("Caught: {}{}", super::panels::unit_name(u), if hostile { " (hostile)" } else { "" });
            lines.push((t, if hostile { col::TEXT_BAD } else { col::TEXT_HI }));
        }
        g.font.draw("Contents", x, y, s, col::BORDER_HI);
        y += lh;
        if lines.is_empty() {
            let empty = match b.subtype.as_str() {
                "CageTrap" => "No cage loaded / nothing caught",
                "WeaponTrap" => "No weapons",
                "StoneFallTrap" => "No stone loaded",
                _ => "Empty",
            };
            g.font.draw(empty, x, y, s, col::TEXT_DIM);
            y += lh;
        }
        for (l, c) in lines.iter().take(8) {
            if y > bottom - 2.0 * lh {
                break;
            }
            g.font.draw(&clip(l, maxc), x, y, s, *c);
            y += lh;
        }
        y += 4.0 * s;
    }
    // Links (both directions).
    let links = links_of(&g.sim.world, b);
    if is_trigger(b) || !links.is_empty() || linkable(b) {
        let head = if is_trigger(b) { format!("Linked buildings ({})", links.len()) } else { format!("Linked levers / plates ({})", links.len()) };
        g.font.draw(&head, x, y, s, col::BORDER_HI);
        y += lh;
        if links.is_empty() {
            let t = if is_trigger(b) { "Nothing linked. Use \"Link to...\" and click a target." } else { "Not linked to a lever." };
            g.font.draw(&clip(t, maxc), x, y, s, col::TEXT_DIM);
            y += lh;
        }
        for id in links.iter() {
            if y > bottom - 2.0 * lh {
                g.font.draw("...", x, y, s, col::TEXT_DIM);
                y += lh;
                break;
            }
            let Some(o) = g.sim.world.building(*id).cloned() else { continue };
            let name = if o.kind == "Trap" { trap_name(&o.subtype) } else { super::building::building_title(g, &o) };
            let st = device_state(&o);
            let t = if st.is_empty() { format!("{name} at {},{} z{}", o.center.x, o.center.y, o.z) } else { format!("{name} ({st}) at {},{} z{}", o.center.x, o.center.y, o.z) };
            let rr = Rect::new(x, y - 1.0 * s, bw, lh);
            if super::list_row(&mut g.ui, rr, false) {
                act = DevAct::Select(o.id);
            }
            g.font.draw(&clip(&t, maxc), x + 4.0 * s, y, s, col::TEXT_CYAN);
            y += lh;
        }
        if is_trigger(b) {
            let n = mechanisms(&g.sim.world);
            g.font.draw(&clip(&format!("Mechanisms in stock: {n} (a link needs 2)"), maxc), x, y + 2.0 * s, s, if n >= 2 { col::TEXT_DIM } else { col::TEXT_BAD });
            y += lh;
        }
    }
    apply(g, b, act);
    y
}

fn apply(g: &mut Game, b: &Building, act: DevAct) {
    match act {
        DevAct::None => {}
        DevAct::Flag(f, on) => {
            g.sim.set_building_flag(b.id, f, on);
            let what = match (f, on) {
                ("LOCKED", true) => "locked",
                ("LOCKED", false) => "unlocked",
                ("FORBIDDEN", true) => "forbidden",
                ("FORBIDDEN", false) => "allowed",
                ("PET_PASSABLE", true) => "pet-passable",
                ("PET_PASSABLE", false) => "not pet-passable",
                ("INTERNAL", true) => "internal",
                _ => "not internal",
            };
            g.set_status(format!("{} {what}", super::words(&b.kind)));
        }
        DevAct::Pull => {
            g.sim.pull_lever(b.id);
            let n = b.links.len();
            g.set_status(format!("Lever pulled ({n} linked building{})", if n == 1 { "" } else { "s" }));
        }
        DevAct::LinkMode => {
            g.tool = if g.tool == Tool::Link(b.id) { Tool::None } else { Tool::Link(b.id) };
            g.submenu = crate::game::Submenu::None;
            g.drag = None;
        }
        DevAct::Select(id) => {
            if let Some(o) = g.sim.world.building(id) {
                let c = o.center;
                g.selected = Selected::Building(id);
                g.cam.focus(c);
            }
        }
    }
}

/// Link mode: a click on the map links the lever to the building there.
pub fn link_click(g: &mut Game, lever: i32, p: Pos) {
    let target = g.sim.world.buildings.iter().filter(|b| b.contains(p) && b.id != lever && linkable(b)).map(|b| b.id).next();
    let Some(t) = target else {
        g.set_status("Nothing to link there: pick a bridge, door, floodgate, hatch, cage, grate, bars or spikes".into());
        return;
    };
    let name = g.sim.world.building(t).map(|b| super::building::building_title(g, b)).unwrap_or_default();
    match g.sim.link_building(lever, t) {
        Ok(()) => {
            g.set_status(format!("Linked to {name} (a mechanic installs 2 mechanisms)"));
            g.tool = Tool::None;
            g.selected = Selected::Building(lever);
        }
        Err(e) => g.set_status(format!("Cannot link {name}: {e}")),
    }
}

fn outline_tiles(g: &Game, b: &Building, c: Color, w: f32) {
    let a = crate::render::tile_rect(g, b.x1, b.y1);
    let e = crate::render::tile_rect(g, b.x2, b.y2);
    draw_rectangle_lines(a.x, a.y, e.x + e.w - a.x, e.y + e.h - a.y, w, c);
}

/// Map overlay: linked buildings of the selected device (with lines to them), and the
/// link-mode targets.
pub fn draw_map_overlay(g: &mut Game) {
    let z = g.cam.z as i16;
    let t = get_time();
    let pulse = 0.6 + 0.4 * ((t * 5.0).sin() as f32);
    let tp = g.cam.tile_px();
    let lw = (tp / 12.0).max(2.0);
    if let Tool::Link(lever) = g.tool {
        let hov = g.hover;
        for b in g.sim.world.buildings.iter().filter(|b| b.z == z && linkable(b) && b.id != lever) {
            let h = hov.map(|p| b.contains(p)).unwrap_or(false);
            let c = if h { Color::new(1.0, 0.9, 0.3, 1.0) } else { Color::new(0.3, 0.85, 1.0, 0.5 + 0.4 * pulse) };
            outline_tiles(g, b, c, if h { lw * 1.5 } else { lw });
        }
    }
    let Selected::Building(id) = g.selected else { return };
    let Some(b) = g.sim.world.building(id).cloned() else { return };
    if !is_device(&b) {
        return;
    }
    let links = links_of(&g.sim.world, &b);
    let from = g.cam.to_screen(b.center.x as f32 + 0.5, b.center.y as f32 + 0.5);
    for lid in links {
        let Some(o) = g.sim.world.building(lid) else { continue };
        let c = Color::new(0.3, 0.85, 1.0, 0.55 + 0.4 * pulse);
        if o.z == z {
            outline_tiles(g, o, c, lw);
        }
        if b.z == z {
            let to = g.cam.to_screen(o.center.x as f32 + 0.5, o.center.y as f32 + 0.5);
            draw_line(from.x, from.y, to.x, to.y, (lw * 0.6).max(1.5), Color::new(0.3, 0.85, 1.0, 0.45));
            if o.z != z {
                // Target on another level: mark the spot and its z.
                let r = Rect::new(to.x - tp * 0.3, to.y - tp * 0.3, tp * 0.6, tp * 0.6);
                draw_rectangle_lines(r.x, r.y, r.w, r.h, lw * 0.7, c);
                let s = g.ui.scale.max(1.0);
                g.font.draw(&format!("z{}", o.z), r.x + r.w + 2.0, r.y, s, col::TEXT_CYAN);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bld(id: i32, kind: &str, sub: &str) -> Building {
        Building { id, kind: kind.into(), subtype: sub.into(), ..Default::default() }
    }

    #[test]
    fn device_kinds_and_states() {
        let mut lever = bld(1, "Trap", "Lever");
        assert!(is_trigger(&lever) && is_device(&lever) && !linkable(&lever));
        assert_eq!(device_state(&lever), "not pulled");
        lever.flags.push("ACTIVE".into());
        assert_eq!(device_state(&lever), "pulled");
        let mut door = bld(2, "Door", "");
        assert!(is_door(&door) && linkable(&door));
        door.flags = vec!["CLOSED".into(), "LOCKED".into(), "PET_PASSABLE".into()];
        assert_eq!(device_state(&door), "closed, locked, pet-passable");
        let mut br = bld(3, "Bridge", "");
        assert_eq!(device_state(&br), "lowered");
        br.flags.push("RAISED".into());
        assert_eq!(device_state(&br), "raised");
        assert_eq!(bridge_direction("N"), "raises to the north");
        assert_eq!(bridge_direction("RETRACT"), "retracts");
        assert!(!linkable(&bld(4, "Trap", "CageTrap")));
        assert_eq!(trap_name("StoneFallTrap"), "Stone-fall trap");
    }

    #[test]
    fn links_both_ways() {
        let mut w = World::synthetic(16, 16, 8);
        let mut lever = bld(10, "Trap", "Lever");
        lever.links = vec![11];
        w.buildings.push(lever);
        w.buildings.push(bld(11, "Bridge", ""));
        let l = w.buildings[0].clone();
        let b = w.buildings[1].clone();
        assert_eq!(links_of(&w, &l), vec![11]);
        assert_eq!(links_of(&w, &b), vec![10]);
    }
}
