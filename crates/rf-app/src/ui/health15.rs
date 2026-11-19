//! [health15] Medicine screens: the fort health overview (Places > Health: hospitals with their
//! furniture and supplies, patients with DF's severity tiers, treatment counters) and the
//! medical rows of the unit health tab (fracture class, splint/cast/traction, nerve damage,
//! crutch). The sim side is `rf_sim::health15_api`.
//!
//! DF shows the fort health screen only with a chief medical dwarf; we always show it and say so
//! (UI choice).

use super::col;
use super::psyche::Row;
use crate::game::Game;
use macroquad::prelude::*;
use rf_sim::health15_api::{Walk, SUPPLY_NAMES};

/// Medical rows for the unit health tab.
pub fn unit_rows(g: &Game, uid: i32) -> Vec<Row> {
    let m = g.sim.unit_medical(uid);
    let mut rows = Vec::new();
    if m.fractures.is_empty() && m.motor.is_empty() && m.sensory.is_empty() && m.missing.is_empty() && m.crutch.is_none() && m.permanent == 0 {
        return rows;
    }
    rows.push(Row::Head("Medical details".into()));
    for f in &m.fractures {
        let state = match (f.set, f.immobilized) {
            (_, "traction") => "in traction".to_string(),
            (_, "") if f.class == "overlapping" => "needs a traction bench".to_string(),
            (false, "") if f.class == "displaced" || f.class == "compound" => "needs setting".to_string(),
            (_, "") => "needs immobilization".to_string(),
            (_, im) => format!("in a {im}"),
        };
        let c = if f.immobilized.is_empty() { col::TEXT_BAD } else { col::TEXT_GOOD };
        rows.push(Row::Note(format!("{} fracture of the {}", cap(f.class), f.part), c, state));
    }
    for p in &m.motor {
        rows.push(Row::Note(format!("Motor nerve severed: {p}"), col::TEXT_BAD, "permanent".into()));
    }
    for p in &m.sensory {
        rows.push(Row::Note(format!("Sensory nerve severed: {p}"), col::TEXT_HI, "no feeling".into()));
    }
    for p in &m.missing {
        rows.push(Row::Note(format!("Missing: {p}"), col::TEXT_BAD, "permanent".into()));
    }
    if m.permanent > 0 {
        rows.push(Row::Text(format!("{} permanent wound(s) to tissue that does not heal.", m.permanent), col::TEXT_DIM));
    }
    let walk = match (m.walk, m.needs_crutch) {
        (Walk::Crutch, _) => ("Walks on a crutch", col::TEXT_HI),
        (Walk::Crawl, true) if m.crutch.is_none() => ("Crawls: needs a crutch", col::TEXT_BAD),
        (Walk::Crawl, _) => ("Crawls (cannot stand)", col::TEXT_BAD),
        _ => ("Walks normally", col::TEXT_GOOD),
    };
    rows.push(Row::Text(walk.0.into(), walk.1));
    if m.diagnosed && !m.tiers.is_empty() {
        let t: Vec<String> = m.tiers.iter().take(6).map(|(p, t)| format!("{p}: {t}")).collect();
        rows.push(Row::Text(t.join(", "), col::TEXT_DIM));
    }
    rows.push(Row::Gap);
    rows
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The fort health overview.
pub fn overview_rows(g: &Game) -> Vec<Row> {
    let mut rows = Vec::new();
    match g.sim.chief_medical_dwarf() {
        Some(u) => {
            let name = g.sim.world.unit(u).map(|x| x.name.clone()).unwrap_or_default();
            rows.push(Row::Link(format!("Chief medical dwarf: {name}"), col::TEXT_CYAN, String::new(), u));
        }
        None => rows.push(Row::Text("No chief medical dwarf: DF hides this screen without one (shown here anyway).".into(), col::TEXT_DIM)),
    }
    rows.push(Row::Gap);
    let hs = g.sim.hospitals();
    rows.push(Row::Head(format!("Hospitals ({})", hs.len())));
    if hs.is_empty() {
        rows.push(Row::Text("No hospital. Make one from a meeting area (Places > Zones) or a hospital zone.".into(), col::TEXT_HI));
    }
    for h in &hs {
        let occ = h.benches.iter().filter(|b| b.1 >= 0).count();
        rows.push(Row::Note(h.name.clone(), col::TEXT_HI, format!("{} beds, {} tables, {} chests, traction benches {}/{}", h.beds, h.tables, h.chests, occ, h.benches.len())));
        let sup: Vec<String> = SUPPLY_NAMES.iter().enumerate().map(|(k, n)| format!("{n} {}/{}", h.have[k], h.want[k])).collect();
        rows.push(Row::Text(format!("Supplies (stocked/desired): {}", sup.join(", ")), if h.chests == 0 { col::TEXT_DIM } else { col::TEXT }));
    }
    rows.push(Row::Gap);
    let pats = g.sim.health15_patients();
    rows.push(Row::Head(format!("Patients and disabled ({})", pats.len())));
    for uid in pats.iter().take(40) {
        let m = g.sim.unit_medical(*uid);
        let name = g.sim.world.unit(*uid).map(|x| x.name.clone()).unwrap_or_default();
        let worst = m.tiers.first().map(|t| format!("{}: {}", t.0, t.1)).unwrap_or_else(|| if m.diagnosed { "minor".into() } else { "not diagnosed".into() });
        let walk = match m.walk {
            Walk::Crutch => ", crutch",
            Walk::Crawl => ", crawls",
            Walk::Normal => "",
        };
        let c = if m.tiers.first().map(|t| t.1 == "BROKEN" || t.1 == "MISSING").unwrap_or(false) { col::TEXT_BAD } else { col::TEXT };
        rows.push(Row::Link(name, c, format!("{worst}{walk}{}", if m.in_traction { ", traction" } else { "" }), *uid));
    }
    rows.push(Row::Gap);
    let r = g.sim.health15_report();
    rows.push(Row::Head("Treatment".into()));
    rows.push(Row::Text(
        format!(
            "Splints {} / slings {} / casts {} / traction {} (rebroken {}), crutches given {}, sutures failed {}, settings failed {}, surgeries failed {}, cleanings failed {}, soap bars used {}",
            r.splints_applied, r.slings_applied, r.casts_applied, r.placed_in_traction, r.rebroken, r.crutches_given, r.sutures_failed, r.settings_failed, r.surgeries_failed, r.cleanings_failed, r.soap_bars_used
        ),
        col::TEXT_DIM,
    ));
    rows.push(Row::Text(format!("Nerve damage: {} motor, {} sensory; permanent wounds {}", r.motor_nerves, r.sensory_nerves, r.permanent), col::TEXT_DIM));
    for (_, e) in r.events.iter().rev().take(8) {
        rows.push(Row::Text(e.clone(), col::TEXT_DIM));
    }
    rows
}

/// Places > Health.
pub fn draw_health(g: &mut Game, body: Rect) {
    let rows = overview_rows(g);
    super::psyche::draw_rows(g, rows, body);
}
