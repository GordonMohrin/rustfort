//! [round 4] Nobles and administrators screen (toolbar "Nobles"): every position with its
//! holder, vacant positions with "Appoint" (citizen picker, best skill first), and the
//! mandates of the nobles.
//!
//! Who appoints whom follows DF's vanilla dwarf civilization (entity_default.txt,
//! `APPOINTED_BY`, `ELECTED`, `SUCCESSION`).

use super::panels::{unit_name, unit_profession};
use super::{clip, col, list_row, panel, text_button, wheel_over, BtnState};
use crate::font::GH;
use crate::game::{Game, Screen};
use macroquad::prelude::*;
use rf_core::NoblePosition;

/// How a position is filled in DF.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filled {
    /// `APPOINTED_BY:EXPEDITION_LEADER` / `MAYOR`: you appoint it here.
    ByFort,
    /// `ELECTED` (mayor).
    Elected,
    /// `APPOINTED_BY:MONARCH` or `SUCCESSION:BY_HEIR` (baron, count, duke, monarch, ...).
    ByMonarch,
    /// `APPOINTED_BY:MILITIA_COMMANDER` (squad leaders, set in the squads screen).
    BySquads,
    /// `APPOINTED_BY:BARON/COUNT/DUKE` (champion).
    ByNoble,
    /// The expedition leader comes with the embark.
    Embark,
}

pub fn filled_by(token: &str) -> Filled {
    match token {
        "MANAGER" | "BOOKKEEPER" | "BROKER" | "CHIEF_MEDICAL_DWARF" | "MILITIA_COMMANDER" | "SHERIFF" | "CAPTAIN_OF_THE_GUARD" | "HAMMERER" => Filled::ByFort,
        "MAYOR" => Filled::Elected,
        "MILITIA_CAPTAIN" => Filled::BySquads,
        "CHAMPION" => Filled::ByNoble,
        "EXPEDITION_LEADER" => Filled::Embark,
        _ => Filled::ByMonarch,
    }
}

fn filled_text(f: Filled) -> &'static str {
    match f {
        Filled::ByFort => "appointed by the fort",
        Filled::Elected => "elected by the citizens",
        Filled::ByMonarch => "appointed by the monarch",
        Filled::BySquads => "set in the squads screen",
        Filled::ByNoble => "appointed by the baron",
        Filled::Embark => "leads the expedition",
    }
}

/// The skill that matters for a position (DF responsibilities).
pub fn position_skill(token: &str) -> Option<&'static str> {
    Some(match token {
        "BROKER" => "APPRAISAL",
        "MANAGER" => "ORGANIZATION",
        "BOOKKEEPER" => "RECORD_KEEPING",
        "CHIEF_MEDICAL_DWARF" => "DIAGNOSE",
        "MILITIA_COMMANDER" | "MILITIA_CAPTAIN" => "LEADERSHIP",
        "HAMMERER" => "HAMMER",
        "SHERIFF" | "CAPTAIN_OF_THE_GUARD" => "OBSERVATION",
        _ => return None,
    })
}

fn position_name(n: &NoblePosition) -> String {
    if !n.name.is_empty() {
        let mut c = n.name.chars();
        return match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        };
    }
    super::pretty(&n.token)
}

/// Unit id of a holder, -1 = vacant; a holder that is no longer on the map counts as vacant.
fn holder_name(g: &Game, id: i32) -> Option<String> {
    if id < 0 {
        return None;
    }
    if let Some(u) = g.sim.world.unit(id) {
        let dead = if u.flags.dead { " (dead)" } else { "" };
        return Some(format!("{}, {}{dead}", unit_name(u), unit_profession(u)));
    }
    g.sim.world.hist_figs.iter().find(|h| h.unit_id == id || h.id == id).map(|h| format!("{} (not here)", h.name)).or_else(|| Some(format!("unit #{id}")))
}

enum NAct {
    None,
    Close,
    Picker(String),
    Appoint(String, i32),
    Center(i32),
}

pub fn draw_nobles(g: &mut Game) {
    if g.screen != Screen::Nobles {
        return;
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let r = super::orders::screen_rect(g, 980.0);
    panel(&mut g.ui, r);
    let pad = 12.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let mut act = NAct::None;
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        act = NAct::Close;
    }
    g.font.draw("Nobles and administrators", x, y, 1.5 * s, col::TEXT_HI);
    let pop = g.sim.world.citizens().count();
    let sub = format!("{} - {pop} citizens", g.sim.world.meta.fort_name);
    g.font.draw(&sub, r.x + r.w - pad - 30.0 * s - g.font.width(&sub, s), y + 4.0 * s, s, col::TEXT_DIM);
    y += 1.5 * GH * s + 12.0 * s;
    let nobles = g.sim.world.nobles.clone();
    let picker_w = if g.r4.appoint.is_some() { (r.w * 0.42).max(300.0 * s) } else { 0.0 };
    let lw = r.w - 2.0 * pad - if picker_w > 0.0 { picker_w + 12.0 * s } else { 0.0 };
    let maxc = (lw / (8.0 * s)) as usize;
    if nobles.is_empty() {
        g.font.draw("No noble positions in this export (re-export with the round-4 exporter).", x, y, s, col::TEXT_DIM);
    }
    // Positions: holder, how it is filled, mandates.
    let area = Rect::new(x, y, lw, r.y + r.h - pad - y);
    let row_lines = |n: &NoblePosition| 2 + n.mandates.len().min(4);
    let total: usize = nobles.iter().map(row_lines).sum();
    let rows_fit = ((area.h / lh) as usize).max(1);
    let step = wheel_over(&g.ui, &mut g.input, area);
    g.r4.nobles_scroll = (g.r4.nobles_scroll as i32 + step).clamp(0, nobles.len().saturating_sub(1) as i32) as usize;
    let _ = (total, rows_fit);
    for (k, n) in nobles.iter().enumerate().skip(g.r4.nobles_scroll) {
        let h = row_lines(n) as f32 * lh + 6.0 * s;
        if y + h > area.y + area.h {
            g.font.draw("... (wheel scrolls)", x, y, s, col::TEXT_DIM);
            break;
        }
        let rr = Rect::new(x, y - 2.0 * s, lw, h - 2.0 * s);
        if k % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.035));
        }
        if g.r4.appoint.as_deref() == Some(n.token.as_str()) {
            draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, col::ACTIVE_BORDER);
        }
        let how = filled_by(&n.token);
        g.font.draw(&position_name(n), x + 4.0 * s, y, s, col::TEXT_HI);
        let ht = format!("({})", filled_text(how));
        g.font.draw(&ht, x + 26.0 * 8.0 * s, y, s, col::TEXT_DIM);
        let holder = holder_name(g, n.holder);
        let (t, c) = match &holder {
            Some(h) => (h.clone(), col::TEXT),
            None => ("Vacant".to_string(), col::TEXT_BAD),
        };
        g.font.draw(&clip(&t, maxc.saturating_sub(16)), x + 16.0 * s, y + lh, s, c);
        if holder.is_some() && n.holder >= 0 && g.sim.world.unit(n.holder).is_some() {
            let hr = Rect::new(x + 16.0 * s, y + lh, g.font.width(&t, s), lh);
            if g.ui.hovered(hr) {
                g.ui.tooltip = Some(("Click to show this dwarf".into(), vec2(hr.x, hr.y - 4.0)));
            }
            if g.ui.clicked(hr) {
                act = NAct::Center(n.holder);
            }
        }
        if how == Filled::ByFort {
            let label = if holder.is_some() { "Replace" } else { "Appoint" };
            let bw = 10.0 * 8.0 * s;
            let br = Rect::new(x + lw - bw - 6.0 * s, y + lh - 3.0 * s, bw, GH * s + 6.0 * s);
            let st = if g.r4.appoint.as_deref() == Some(n.token.as_str()) { BtnState::Active } else { BtnState::Normal };
            if text_button(&mut g.ui, &g.font, br, label, st) {
                act = NAct::Picker(n.token.clone());
            }
        }
        for (i, m) in n.mandates.iter().take(4).enumerate() {
            let mt = if i == 3 && n.mandates.len() > 4 { format!("... {} more mandates", n.mandates.len() - 3) } else { format!("Mandate: {m}") };
            g.font.draw(&clip(&mt, maxc.saturating_sub(6)), x + 32.0 * s, y + (2 + i) as f32 * lh, s, col::TEXT_CYAN);
        }
        y += h;
    }
    if let Some(tok) = g.r4.appoint.clone() {
        let pr = Rect::new(r.x + r.w - pad - picker_w, r.y + pad + 1.5 * GH * s + 12.0 * s, picker_w, r.h - 2.0 * pad - 1.5 * GH * s - 12.0 * s);
        if let Some(a) = draw_picker(g, &tok, pr) {
            act = a;
        }
    }
    match act {
        NAct::None => {}
        NAct::Close => {
            g.screen = Screen::None;
            g.r4.appoint = None;
        }
        NAct::Picker(t) => {
            g.r4.appoint = if g.r4.appoint.as_deref() == Some(t.as_str()) { None } else { Some(t) };
            g.r4.appoint_scroll = 0;
        }
        NAct::Appoint(t, uid) => {
            g.sim.appoint(&t, uid);
            let who = g.sim.world.unit(uid).map(unit_name).unwrap_or_default();
            let pos = g.sim.world.nobles.iter().find(|n| n.token == t).map(position_name).unwrap_or_else(|| super::pretty(&t));
            g.set_status(format!("{who} is now {}", pos.to_lowercase()));
            g.r4.appoint = None;
        }
        NAct::Center(uid) => {
            if let Some(p) = g.sim.world.unit(uid).map(|u| u.pos) {
                g.cam.focus(p);
                g.select_unit(uid);
                g.screen = Screen::None;
            }
        }
    }
}

/// Citizen picker for a position: best in the position's skill first.
fn draw_picker(g: &mut Game, token: &str, r: Rect) -> Option<NAct> {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.4));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, col::BORDER_HI);
    let pad = 8.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let skill = position_skill(token);
    let name = g.sim.world.nobles.iter().find(|n| n.token == token).map(position_name).unwrap_or_else(|| super::pretty(token));
    g.font.draw(&format!("Appoint {}", name.to_lowercase()), x, y, s, col::TEXT_HI);
    y += lh;
    let how = match skill {
        Some(sk) => format!("Sorted by {}; * holds another position", super::pretty(sk).to_lowercase()),
        None => "* holds another position".to_string(),
    };
    g.font.draw(&clip(&how, ((r.w - 2.0 * pad) / (8.0 * s)) as usize), x, y, s, col::TEXT_DIM);
    y += lh;
    y += 4.0 * s;
    let holders: Vec<i32> = g.sim.world.nobles.iter().filter(|n| n.token != token).map(|n| n.holder).collect();
    let mut list: Vec<(i32, String, i32)> = g
        .sim
        .world
        .citizens()
        .filter(|u| !u.flags.animal && u.profession != "CHILD" && u.profession != "BABY")
        .map(|u| (u.id, unit_name(u), skill.map(|sk| u.skill(sk)).unwrap_or(0)))
        .collect();
    list.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(&b.1)));
    let area = Rect::new(r.x + 2.0 * s, y, r.w - 4.0 * s, r.y + r.h - pad - y);
    let rows = ((area.h / lh) as usize).max(1);
    let step = wheel_over(&g.ui, &mut g.input, area);
    g.r4.appoint_scroll = (g.r4.appoint_scroll as i32 + step * 3).clamp(0, list.len().saturating_sub(rows) as i32) as usize;
    let maxc = ((r.w - 2.0 * pad) / (8.0 * s)) as usize;
    let mut out = None;
    for (id, nm, lvl) in list.iter().skip(g.r4.appoint_scroll).take(rows) {
        let rr = Rect::new(area.x, y, area.w, lh);
        if list_row(&mut g.ui, rr, false) {
            out = Some(NAct::Appoint(token.to_string(), *id));
        }
        let busy = if holders.contains(id) { " *" } else { "" };
        let lt = if skill.is_some() && *lvl > 0 { super::skill_level_name(*lvl).to_string() } else if skill.is_some() { "-".into() } else { String::new() };
        g.font.draw(&clip(&format!("{nm}{busy}"), maxc.saturating_sub(lt.len() + 2)), x, y + 1.5 * s, s, col::TEXT);
        g.font.draw(&lt, r.x + r.w - pad - g.font.width(&lt, s), y + 1.5 * s, s, col::TEXT_DIM);
        y += lh;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_fills_positions() {
        assert_eq!(filled_by("BROKER"), Filled::ByFort);
        assert_eq!(filled_by("MAYOR"), Filled::Elected);
        assert_eq!(filled_by("BARON"), Filled::ByMonarch);
        assert_eq!(filled_by("MILITIA_CAPTAIN"), Filled::BySquads);
        assert_eq!(position_skill("BROKER"), Some("APPRAISAL"));
        let n = NoblePosition { token: "CHIEF_MEDICAL_DWARF".into(), ..Default::default() };
        assert_eq!(position_name(&n), "Chief medical dwarf");
        let n = NoblePosition { token: "MAYOR".into(), name: "mayor".into(), ..Default::default() };
        assert_eq!(position_name(&n), "Mayor");
    }
}
