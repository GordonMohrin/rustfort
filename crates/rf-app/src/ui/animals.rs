//! [app8] Pets and livestock (DF Steam: Creatures > Pets/Livestock): every tame animal from
//! `Sim::animals()` with race and sex, training, owner, pasture and state, and DF's buttons:
//! slaughter (`Sim::set_slaughter`), war / hunting training (`Sim::request_training`) and the
//! pen/pasture picker (`Sim::assign_pasture`, Pen zones).

use super::round8::{frame, picker, scroll, tabs, zone_label, Pick, PickRow};
use super::{clip, col, draw_icon, pretty, Ico};
use crate::font::GH;
use crate::game::{Game, Selected};
use macroquad::prelude::*;
use rf_sim::AnimalInfo;

const TABS: &[&str] = &["All", "On the map", "Pastured", "For slaughter", "Pets", "Pregnant"];

fn sex_sym(caste: &str) -> &'static str {
    match caste {
        "MALE" => "♂",
        "FEMALE" => "♀",
        _ => "",
    }
}

/// DF's training words.
pub fn training_label(t: &str) -> String {
    match t {
        "" | "Domesticated" => "Domesticated".into(),
        "WildUntamed" => "Wild".into(),
        "SemiWild" => "Semi-wild".into(),
        "WarTrained" | "War" => "War trained".into(),
        "HuntTrained" | "Hunting" => "Hunting trained".into(),
        other => super::words(other),
    }
}

/// The owner, if it is a unit of this world (old exports carry 0 = no field).
pub fn owner_of(g: &Game, a: &AnimalInfo) -> Option<i32> {
    (a.owner != a.unit && g.sim.world.unit(a.owner).is_some()).then_some(a.owner)
}

/// The pen/pasture, if it is a zone of this world.
pub fn pasture_of(g: &Game, a: &AnimalInfo) -> Option<i32> {
    g.sim.world.building(a.pasture).filter(|b| b.kind == "Civzone").map(|b| b.id)
}

/// Creature name from the save's raws (`BIRD_CHICKEN` -> "chicken").
pub fn creature_name(g: &Game, race: &str) -> String {
    g.sim.world.creature_raws.iter().find(|c| c.token == race).map(|c| c.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| pretty(race).to_lowercase())
}

fn on_map(g: &Game, a: &AnimalInfo) -> bool {
    g.sim.world.unit(a.unit).map(|u| u.pos.x >= 0 && !u.flags.inactive).unwrap_or(false)
}

pub fn draw(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(c) = frame(g, "Pets and livestock", 1400.0) else { return };
    let mut all = g.sim.animals();
    // On the map first, then by race and name.
    all.sort_by(|a, b| on_map(g, b).cmp(&on_map(g, a)).then(a.race.cmp(&b.race)).then(a.unit.cmp(&b.unit)));
    let counts: Vec<usize> = (0..TABS.len()).map(|t| all.iter().filter(|a| in_tab(g, a, t)).count()).collect();
    let labels: Vec<String> = TABS.iter().zip(&counts).map(|(t, n)| format!("{t} ({n})")).collect();
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let tx = c.x + g.font.width("Pets and livestock", 1.5 * s) + 16.0 * s;
    let (hit, _) = tabs(g, tx, c.y - 3.0 * s, &refs, g.r8.an_tab.min(TABS.len() - 1));
    if let Some(i) = hit {
        g.r8.an_tab = i;
        g.r8.an_scroll = 0;
    }
    let tab = g.r8.an_tab.min(TABS.len() - 1);
    let rows: Vec<AnimalInfo> = all.into_iter().filter(|a| in_tab(g, a, tab)).collect();
    let y0 = c.y + 1.5 * GH * s + 14.0 * s;
    let xs = [c.x + 40.0 * s, c.x + 40.0 * s + 26.0 * 8.0 * s, c.x + 40.0 * s + 44.0 * 8.0 * s, c.x + 40.0 * s + 60.0 * 8.0 * s, c.x + 40.0 * s + 82.0 * 8.0 * s, c.x + 40.0 * s + 104.0 * 8.0 * s];
    for (x, t) in xs.iter().zip(["Name", "Creature", "Training", "Owner", "Pen/pasture", "State"]) {
        g.font.draw(t, *x, y0, s, col::TEXT_DIM);
    }
    let bx = c.x + c.w - 4.0 * 30.0 * s;
    g.font.draw("Actions", bx, y0, s, col::TEXT_DIM);
    let row_h = 34.0 * s;
    let list = Rect::new(c.x, y0 + lh + 2.0 * s, c.w, c.y + c.h - (y0 + lh + 2.0 * s) - lh);
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.an_scroll = scroll(g, list, g.r8.an_scroll, rows.len(), vis);
    let mut act: Option<(i32, &'static str)> = None;
    let mut sprites = std::mem::take(&mut g.rend.buf);
    for (k, a) in rows.iter().enumerate().skip(g.r8.an_scroll).take(vis) {
        let y = list.y + (k - g.r8.an_scroll) as f32 * row_h;
        let rr = Rect::new(list.x, y, list.w, row_h - 2.0 * s);
        if k % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        let here = on_map(g, a);
        // Portrait: the creature's own sprite.
        if let Some(u) = g.sim.world.unit(a.unit) {
            sprites.clear();
            g.gfx.unit(&g.sim.world, u, &mut sprites);
            let tp = 32.0 * s;
            let mut drawn = false;
            for sp in &sprites {
                drawn |= g.rend.draw_sprite(&g.gfx, sp, rr.x + 2.0 * s, rr.y, tp);
            }
            if !drawn {
                super::draw_fallback_icon(&g.font, Ico::Glyph(b'a', [200, 170, 120]), rr.x + 2.0 * s, rr.y, tp, tp);
            }
        }
        let ty = y + (row_h - 2.0 * s - GH * s) / 2.0;
        let name = g.sim.world.unit(a.unit).map(super::panels::unit_name).unwrap_or_else(|| format!("#{}", a.unit));
        let nr = Rect::new(xs[0], y, 25.0 * 8.0 * s, row_h - 2.0 * s);
        let hov = g.ui.hovered(nr);
        g.font.draw(&clip(&name, 25), xs[0], ty, s, if hov { col::TEXT_HI } else if here { col::TEXT } else { col::TEXT_DIM });
        if here && g.ui.clicked(nr) {
            act = Some((a.unit, "select"));
        }
        g.font.draw(&clip(&format!("{} {}", creature_name(g, &a.race), sex_sym(&a.caste)), 17), xs[1], ty, s, col::TEXT);
        g.font.draw(&clip(&training_label(&a.training), 15), xs[2], ty, s, if a.training.contains("War") || a.training.contains("Hunt") { col::TEXT_HI } else { col::TEXT_DIM });
        let (own, pas) = (owner_of(g, a), pasture_of(g, a));
        let owner = own.map(|o| super::round8::unit_label(g, o)).unwrap_or_else(|| "-".into());
        g.font.draw(&clip(&owner, 21), xs[3], ty, s, if own.is_some() { col::TEXT_CYAN } else { col::TEXT_DIM });
        let pasture = pas.map(|z| zone_label(g, z)).unwrap_or_else(|| "-".into());
        g.font.draw(&clip(&pasture, 21), xs[4], ty, s, if pas.is_some() { col::TEXT_GOOD } else { col::TEXT_DIM });
        let mut st: Vec<&str> = Vec::new();
        if a.pregnant {
            st.push("pregnant");
        }
        if a.slaughter {
            st.push("to slaughter");
        }
        if a.grazer {
            st.push("grazer");
        }
        if !here {
            st.push("not on the map");
        }
        let stt = st.join(", ");
        g.font.draw(&clip(&stt, ((bx - xs[5]) / (8.0 * s)) as usize), xs[5], ty, s, if a.slaughter { col::TEXT_BAD } else { col::TEXT_DIM });
        // Buttons: slaughter, war training, hunting training, pasture.
        let bsz = Rect::new(0.0, 0.0, 26.0 * s, 26.0 * s);
        let buttons: [(&str, bool, &'static str, &str); 4] = [
            (if a.slaughter { "PETS_LIVESTOCK_SLAUGHTER_ACTIVE" } else { "PETS_LIVESTOCK_SLAUGHTER_INACTIVE" }, a.slaughter, "slaughter", "Slaughter"),
            (if a.role == "War" { "PETS_LIVESTOCK_WAR_TRAINING_ACTIVE" } else { "PETS_LIVESTOCK_WAR_TRAINING_INACTIVE" }, a.role == "War", "war", "Train for war"),
            (if a.role == "Hunting" { "PETS_LIVESTOCK_HUNT_TRAINING_ACTIVE" } else { "PETS_LIVESTOCK_HUNT_TRAINING_INACTIVE" }, a.role == "Hunting", "hunt", "Train for hunting"),
            ("ZONE_PICK_ANIMALS", pas.is_some(), "pasture", "Assign to a pen/pasture"),
        ];
        for (i, (icon, on, what, tip)) in buttons.iter().enumerate() {
            let r = Rect::new(bx + i as f32 * 30.0 * s, y + (row_h - 2.0 * s - bsz.h) / 2.0, bsz.w, bsz.h);
            let hov = g.ui.hovered(r);
            if !draw_icon(&g.gfx, &mut g.rend, icon, r, WHITE) {
                let gl = [b'k', b'W', b'H', b'P'][i];
                draw_rectangle(r.x, r.y, r.w, r.h, if *on { col::BUTTON_ACTIVE } else { col::BUTTON });
                super::draw_fallback_icon(&g.font, Ico::Glyph(gl, [220, 220, 220]), r.x, r.y, r.w, r.h);
            }
            if *on {
                draw_rectangle_lines(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, 2.0, col::ACTIVE_BORDER);
            } else if hov {
                draw_rectangle_lines(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, 1.0, col::BORDER_HI);
            }
            if hov {
                g.ui.tooltip = Some((tip.to_string(), vec2(r.x, r.y - 4.0)));
            }
            if g.ui.clicked(r) {
                act = Some((a.unit, what));
            }
        }
    }
    g.rend.buf = sprites;
    let fy = c.y + c.h - GH * s;
    let pens = g.sim.world.buildings.iter().filter(|b| b.kind == "Civzone" && matches!(b.subtype.as_str(), "Pen" | "Pasture")).count();
    g.font.draw(&format!("{} animals shown. {pens} pen/pasture zone(s). Click a name to select it on the map.", rows.len()), c.x, fy, s, col::TEXT_DIM);
    if let Some((id, what)) = act {
        let a = rows.iter().find(|a| a.unit == id).cloned().unwrap_or_default();
        match what {
            "select" => {
                g.selected = Selected::Unit(id);
                if let Some(p) = g.sim.world.unit(id).map(|u| u.pos) {
                    g.cam.focus(p);
                }
                g.screen = crate::game::Screen::None;
            }
            "slaughter" => {
                g.sim.set_slaughter(id, !a.slaughter);
                g.set_status(format!("{}: {}", super::round8::unit_label(g, id), if a.slaughter { "not marked for slaughter" } else { "marked for slaughter" }));
            }
            "war" => g.sim.request_training(id, if a.role == "War" { "" } else { "War" }),
            "hunt" => g.sim.request_training(id, if a.role == "Hunting" { "" } else { "Hunting" }),
            "pasture" => {
                g.r8.an_pick = Some(id);
                g.r8.an_pscroll = 0;
            }
            _ => {}
        }
    }
    // Pasture picker.
    if let Some(aid) = g.r8.an_pick {
        let pens: Vec<PickRow> = g
            .sim
            .world
            .buildings
            .iter()
            .filter(|b| b.kind == "Civzone" && matches!(b.subtype.as_str(), "Pen" | "Pasture"))
            .map(|b| {
                let n = rows.iter().filter(|a| pasture_of(g, a) == Some(b.id)).count();
                PickRow { id: b.id, label: zone_label(g, b.id), info: format!("{}x{} z{}, {n} animals", b.x2 - b.x1 + 1, b.y2 - b.y1 + 1, b.z), color: col::TEXT }
            })
            .collect();
        let who = super::round8::unit_label(g, aid);
        let pr = Rect::new(c.x + c.w / 2.0 - 25.0 * 8.0 * s, c.y + 40.0 * s, 50.0 * 8.0 * s, (c.h * 0.6).max(160.0 * s));
        let (sc, pick) = picker(g, pr, &format!("Pen/pasture for {}", clip(&who, 28)), &pens, g.r8.an_pscroll, Some("No pasture"));
        g.r8.an_pscroll = sc;
        match pick {
            Pick::Picked(z) => {
                g.sim.assign_pasture(aid, z);
                g.r8.an_pick = None;
                g.set_status(format!("{who}: {}", if z < 0 { "no pasture".to_string() } else { zone_label(g, z) }));
            }
            Pick::Cancel => g.r8.an_pick = None,
            Pick::Nothing => {}
        }
        if pens.is_empty() {
            let t = "No pen/pasture zones: make one with Zones (z) > Pen/pasture.";
            g.font.draw(t, pr.x + 10.0 * s, pr.y + pr.h - 2.0 * lh, s, col::TEXT_BAD);
        }
    }

}

fn in_tab(g: &Game, a: &AnimalInfo, t: usize) -> bool {
    match t {
        1 => on_map(g, a),
        2 => pasture_of(g, a).is_some(),
        3 => a.slaughter,
        4 => owner_of(g, a).is_some(),
        5 => a.pregnant,
        _ => true,
    }
}
