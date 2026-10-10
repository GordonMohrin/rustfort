//! Squads panel (DF hotkey `c`): squads with members, orders and uniforms. Orders go to
//! `Sim::order_squad` (Station: click a tile, Kill: click a creature or pick a hostile, Train,
//! Cancel); members are assigned with a citizen picker (`Sim::assign_squad_member`).
//!
//! [mil-ui] Like DF Steam's squads panel it is a side panel in the left column: the map stays
//! visible and usable (pan, zoom, z-levels, clicks), and a member's unit sheet opens at the
//! right next to it. Squads have checkboxes, so one order (Station, Kill, Train, Cancel, a
//! routine) goes to all ticked squads; the civilian alert ("everyone into the burrow") sits at
//! the top; member rows show the unit's map sprite and its uniform slots.

use super::panels::{unit_name, unit_profession};
use super::{checkbox, clip, col, list_row, panel, pretty, text_button, wheel_over, BtnState};
use crate::font::GH;
use crate::game::{Game, Screen, Selected, Tool};
use macroquad::prelude::*;
use rf_core::{Pos, Squad, Unit, World};
use rf_sim::SquadOrder;

/// Weapon skills (DF `job_skill`): the "best skill" column prefers these.
pub const WEAPON_SKILLS: &[&str] =
    &["AXE", "SWORD", "DAGGER", "MACE", "HAMMER", "SPEAR", "PIKE", "WHIP", "CROSSBOW", "BOW", "BLOWGUN", "WRESTLING"];
/// Other combat skills, used when a unit has no weapon skill.
pub const COMBAT_SKILLS: &[&str] = &["SHIELD", "ARMOR", "DODGING", "THROW", "MELEE_COMBAT", "RANGED_COMBAT", "MILITARY_TACTICS"];

/// Squad display name (alias first, like DF).
pub fn squad_name(w: &World, id: i32) -> String {
    match w.squads.iter().find(|s| s.id == id) {
        Some(s) if !s.alias.is_empty() => s.alias.clone(),
        Some(s) if !s.name.is_empty() => s.name.clone(),
        _ => format!("Squad #{id}"),
    }
}

/// Best combat skill of a unit as "Skilled axeman"-style text (level + skill).
pub fn best_combat_skill(u: &Unit) -> Option<(String, i32)> {
    let best = |set: &[&str]| u.skills.iter().filter(|k| set.contains(&k.id.as_str()) && k.rating > 0).max_by_key(|k| (k.rating, k.xp));
    best(WEAPON_SKILLS)
        .or_else(|| best(COMBAT_SKILLS))
        .map(|k| (crate::ui::round9::skill_title(k), k.rating)) // [app9] DF skill names
}

/// Human text of one DF-like order string (`Station x,y,z`, `Kill <id>`, `Train`).
pub fn order_text(w: &World, o: &str) -> String {
    let (k, rest) = o.split_once(' ').unwrap_or((o, ""));
    match k {
        "Station" => {
            let p: Vec<&str> = rest.split(',').collect();
            if p.len() == 3 {
                format!("Station at {}, {}, z {}", p[0].trim(), p[1].trim(), p[2].trim())
            } else {
                "Station".into()
            }
        }
        "Kill" => match rest.trim().parse::<i32>().ok().and_then(|id| w.unit(id)) {
            Some(u) if u.flags.dead => format!("Kill {} (dead)", unit_name(u)),
            Some(u) => format!("Kill {}", unit_name(u)),
            None => format!("Kill creature #{}", rest.trim()),
        },
        _ => o.to_string(),
    }
}

pub fn orders_summary(w: &World, s: &Squad) -> String {
    if s.orders.is_empty() {
        "No orders".into()
    } else {
        s.orders.iter().map(|o| order_text(w, o)).collect::<Vec<_>>().join("; ")
    }
}

/// Position of a station order, if the squad has one (to show and jump to).
pub fn station_pos(s: &Squad) -> Option<Pos> {
    s.orders.iter().find_map(|o| {
        let rest = o.strip_prefix("Station ")?;
        let p: Vec<i16> = rest.split(',').filter_map(|t| t.trim().parse().ok()).collect();
        (p.len() == 3).then(|| Pos::new(p[0], p[1], p[2]))
    })
}

/// "WEAPON:ITEM_WEAPON_AXE_BATTLE" -> "Weapon: battle axe" (names from the save's raws).
pub fn uniform_text(w: &World, e: &str) -> String {
    // "ARMOR@Armor": any item of that kind (DF uniform without a specific type).
    if let Some((kind, what)) = e.split_once('@') {
        let k = match kind {
            "ARMOR" => "Body armor",
            "HELM" => "Helm",
            "PANTS" => "Leg armor",
            "GLOVES" => "Gloves",
            "SHOES" => "Shoes",
            "SHIELD" => "Shield",
            "WEAPON" => "Weapon",
            other => return format!("{}: any {}", pretty(other), what.to_lowercase()),
        };
        return format!("{k}: any {}", what.to_lowercase());
    }
    let (kind, tok) = e.split_once(':').unwrap_or(("", e));
    let name = w
        .weapon_raws
        .iter()
        .find(|r| r.token == tok)
        .map(|r| r.name.clone())
        .or_else(|| w.armor_raws.iter().find(|r| r.token == tok).map(|r| r.name.clone()))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            let t = tok.trim_start_matches("ITEM_");
            let t = t.split_once('_').map(|(_, r)| r).unwrap_or(t);
            pretty(t).to_lowercase()
        });
    if kind.is_empty() {
        name
    } else {
        format!("{}: {name}", pretty(kind))
    }
}

/// The creature a Kill order targets when clicking `p`: a hostile there first, else any
/// living non-citizen (DF lets you target any creature that is not a citizen).
pub fn kill_target_at(g: &Game, p: Pos) -> Option<i32> {
    let w = &g.sim.world;
    let here: Vec<&Unit> = w.units.iter().filter(|u| u.pos == p && !u.flags.dead && !u.flags.citizen).collect();
    here.iter().find(|u| g.hostiles.binary_search(&u.id).is_ok()).or(here.first()).map(|u| u.id)
}

/// Assign `uid` to `squad` position `pos` (moves it out of its old squad first).
pub fn assign(g: &mut Game, squad: i32, pos: usize, uid: i32) {
    let old: Vec<(i32, usize)> = g
        .sim
        .world
        .squads
        .iter()
        .flat_map(|s| s.members.iter().enumerate().filter(|(_, m)| **m == uid).map(move |(i, _)| (s.id, i)))
        .collect();
    for (s, i) in old {
        unassign(g, s, i);
    }
    unassign(g, squad, pos);
    g.sim.assign_squad_member(squad, pos, uid);
}

/// Empty a squad position.
pub fn unassign(g: &mut Game, squad: i32, pos: usize) {
    let prev = g.sim.world.squads.iter().find(|s| s.id == squad).and_then(|s| s.members.get(pos).copied()).unwrap_or(-1);
    if prev < 0 {
        return;
    }
    g.sim.assign_squad_member(squad, pos, -1);
    // The removed member leaves the squad (in case the sim left its unit fields alone).
    if let Some(u) = g.sim.world.units.iter_mut().find(|u| u.id == prev && u.squad_id == squad) {
        u.squad_id = -1;
        u.squad_position = -1;
    }
}

// ============================================================ [mil-ui] side panel

/// Width of the squads side panel in 8-px characters (at 100 %).
pub const PANEL_CHARS: f32 = 66.0;

/// [mil-ui] State of the squads side panel (kept in `MilitaryUi::panel`).
#[derive(Default)]
pub struct PanelState {
    /// Squads ticked for one shared order (squad ids).
    pub checked: Vec<i32>,
    /// Burrow picked for the civilian alert.
    pub alert_burrow: Option<i32>,
    /// The burrow dropdown is open; where it was drawn last frame.
    pub dd_open: bool,
    pub dd_rect: Option<Rect>,
    /// Squads the pending Station / Kill map tool gives its order to.
    pub pending: Vec<i32>,
    /// Scroll of the member list (the squad list uses `MilitaryUi::scroll`).
    pub mscroll: usize,
    /// The panel was open last frame / the citizens list was open last frame (they share the
    /// left column: the one opened last wins).
    pub was_open: bool,
    pub citizens_last: bool,
}

/// [mil-ui] The squads screen is a side panel: the map stays usable (hover, clicks, tools)
/// while it is open. False while a full-window screen covers the map.
pub fn map_open(g: &Game) -> bool {
    g.screen == Screen::None || g.screen == Screen::Military
}

/// The side panel's rectangle (left column, between the top bar and the toolbar).
pub fn panel_rect(g: &Game) -> Rect {
    let s = g.ui.scale;
    let top = super::topbar::TOPBAR_H * s + 6.0 * s;
    let bottom = screen_height() - super::toolbar::TOOLBAR_H * s - 6.0 * s;
    let w = (PANEL_CHARS * 8.0 * s + 24.0 * s).min(screen_width() * 0.48);
    Rect::new(6.0 * s, top, w, (bottom - top).max(200.0))
}

/// Squads an order button acts on: the ticked squads, else the open one.
pub fn order_targets(g: &Game) -> Vec<i32> {
    let w = &g.sim.world;
    let checked: Vec<i32> = g.mil.panel.checked.iter().copied().filter(|id| w.squads.iter().any(|q| q.id == *id)).collect();
    if !checked.is_empty() {
        return checked;
    }
    g.mil.squad.filter(|id| w.squads.iter().any(|q| q.id == *id)).into_iter().collect()
}

/// "Wache" or "3 squads".
fn targets_name(g: &Game, t: &[i32]) -> String {
    match t {
        [one] => squad_name(&g.sim.world, *one),
        _ => format!("{} squads", t.len()),
    }
}

/// Squads the map tool of squad `sq` orders: the squads ticked when the tool was picked.
fn tool_targets(g: &Game, sq: i32) -> Vec<i32> {
    let p = &g.mil.panel.pending;
    if p.contains(&sq) {
        p.iter().copied().filter(|id| g.sim.world.squads.iter().any(|q| q.id == *id)).collect()
    } else {
        vec![sq]
    }
}

/// "Wache" / "3 squads": who the Station / Kill map tool of squad `sq` orders (tool hint).
pub fn tool_label(g: &Game, sq: i32) -> String {
    targets_name(g, &tool_targets(g, sq))
}

/// Station tool: a map click stations the squad (or all ticked squads) there.
pub fn station_click(g: &mut Game, sq: i32, h: Pos) {
    let t = tool_targets(g, sq);
    for id in &t {
        g.sim.order_squad(*id, SquadOrder::Station(h));
    }
    g.set_status(format!("{}: station at {}, {}, z {}", targets_name(g, &t), h.x, h.y, h.z));
    g.tool = Tool::None;
    g.screen = Screen::Military;
    g.mil.panel.pending.clear();
}

/// Kill tool: a map click on a creature orders the squad (or all ticked squads) to kill it.
pub fn kill_click(g: &mut Game, sq: i32, h: Pos) {
    match kill_target_at(g, h) {
        Some(uid) => {
            let t = tool_targets(g, sq);
            for id in &t {
                g.sim.order_squad(*id, SquadOrder::Kill(uid));
            }
            let who = g.sim.world.unit(uid).map(unit_name).unwrap_or_default();
            g.set_status(format!("{}: kill {who}", targets_name(g, &t)));
            g.tool = Tool::None;
            g.screen = Screen::Military;
            g.mil.panel.pending.clear();
        }
        None => g.set_status("No creature to attack there (citizens cannot be targeted)".into()),
    }
}

/// `--ui` states of the side panel (screenshot tests). True when `k` was one of them.
/// `mil-check:all` or `mil-check:72+73` ticks squads, `mil-alert` opens the burrow dropdown,
/// `mil-alert-on[:burrow id]` sounds the civilian alert, `mil-member[:position]` opens a
/// member's unit sheet, `mil-tool:station|kill` picks the map tool for the ticked squads.
pub fn ui_arg(g: &mut Game, k: &str, v: &str) -> bool {
    let open = |g: &mut Game| {
        g.screen = Screen::Military;
        if g.mil.squad.is_none() {
            g.mil.squad = g.sim.world.squads.first().map(|s| s.id);
        }
    };
    match k {
        "mil-check" => {
            open(g);
            let ids: Vec<i32> = g.sim.world.squads.iter().map(|q| q.id).collect();
            g.mil.panel.checked = if v.is_empty() || v == "all" { ids } else { v.split('+').filter_map(|t| t.trim().parse().ok()).filter(|id| ids.contains(id)).collect() };
        }
        "mil-alert" => {
            open(g);
            g.mil.panel.dd_open = true;
            if let Ok(id) = v.parse::<i32>() {
                g.mil.panel.alert_burrow = Some(id);
            }
        }
        "mil-alert-on" => {
            open(g);
            let id = v.parse::<i32>().ok().or_else(|| default_burrow(g));
            if id.is_some() {
                super::burrows::set_alert(g, id);
            }
        }
        "mil-member" => {
            open(g);
            let pos = v.parse::<usize>().unwrap_or(0);
            let sq = g.mil.squad.and_then(|id| g.sim.world.squads.iter().find(|q| q.id == id));
            let uid = sq.and_then(|q| q.members.get(pos).copied().filter(|m| *m >= 0).or_else(|| q.members.iter().copied().find(|m| *m >= 0)));
            if let Some(uid) = uid.filter(|u| g.sim.world.unit(*u).is_some()) {
                g.select_unit(uid);
                g.unit_tab = 0;
            }
        }
        "mil-tool" => {
            open(g);
            let t = order_targets(g);
            if let Some(&first) = t.first() {
                g.mil.panel.pending = t;
                g.tool = if v == "kill" { Tool::SquadKill(first) } else { Tool::SquadStation(first) };
            }
        }
        _ => return false,
    }
    true
}

/// The burrow the alert button offers first: the one picked, else the largest burrow.
fn default_burrow(g: &Game) -> Option<i32> {
    let w = &g.sim.world;
    g.mil.panel.alert_burrow.filter(|id| w.burrows.iter().any(|b| b.id == *id)).or_else(|| w.burrows.iter().max_by_key(|b| b.tiles.len()).map(|b| b.id))
}

pub fn draw_military(g: &mut Game) {
    if g.screen != Screen::Military {
        g.mil.panel.was_open = false;
        g.mil.panel.dd_open = false;
        return;
    }
    // The squads panel and the citizens list share the left column: the newer one wins.
    if !g.mil.panel.was_open {
        g.citizens_open = false;
    } else if g.citizens_open && !g.mil.panel.citizens_last {
        g.screen = Screen::None;
        g.mil.panel.was_open = false;
        return;
    }
    g.mil.panel.was_open = true;
    g.mil.panel.citizens_last = g.citizens_open;
    let w = &g.sim.world;
    g.mil.panel.checked.retain(|id| w.squads.iter().any(|q| q.id == *id));
    if g.mil.squad.map(|id| !w.squads.iter().any(|q| q.id == id)).unwrap_or(true) {
        g.mil.squad = w.squads.first().map(|q| q.id);
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let r = panel_rect(g);
    // The open dropdown lies over the panel: it takes the click before anything under it.
    let mut dd_click = false;
    if g.mil.panel.dd_open {
        if let Some(dr) = g.mil.panel.dd_rect {
            g.ui.block(dr);
            if g.ui.lpressed && !g.ui.click_used && dr.contains(g.ui.mouse) {
                g.ui.click_used = true;
                dd_click = true;
            } else if g.ui.lpressed || g.ui.rpressed {
                g.mil.panel.dd_open = false;
            }
        }
    }
    panel(&mut g.ui, r);
    let pad = 10.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let iw = r.w - 2.0 * pad;
    let bh = 22.0 * s;
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        g.screen = Screen::None;
        return;
    }
    let nb = Rect::new(cb.x - 14.0 * 8.0 * s - 6.0 * s, r.y + 5.0 * s, 14.0 * 8.0 * s, 18.0 * s);
    if text_button(&mut g.ui, &g.font, nb, "+ New squad", BtnState::Normal) {
        let k = g.sim.world.squads.len() + 1;
        let id = g.sim.create_squad(&format!("Squad {k}"));
        g.mil.squad = Some(id);
        g.mil.picker = Some(0);
        g.r9.sq_tab = 0;
        g.set_status(format!("Created Squad {k}: pick its first member"));
    }
    let n = g.sim.world.squads.len();
    g.font.draw(&format!("Squads ({n})"), x, y, 1.5 * s, col::TEXT_HI);
    y += 1.5 * GH * s + 8.0 * s;
    let hn = g.hostiles.iter().filter(|id| g.sim.world.unit(**id).map(|u| !u.flags.caged).unwrap_or(false)).count();
    if hn > 0 {
        let t = format!("! {hn} hostile creature{} on the map", if hn == 1 { "" } else { "s" });
        g.font.draw(&t, x, y, s, col::TEXT_BAD);
        y += lh + 2.0 * s;
    }
    // ---- defense: everyone into the burrows (civilian alert)
    let dd_anchor;
    (y, dd_anchor) = draw_alert(g, x, y, iw, bh);
    y += 8.0 * s;
    // ---- squad list with checkboxes
    y = draw_squad_list(g, x, y, iw);
    // ---- one order for the ticked squads (or the open one)
    y = draw_orders(g, x, y, iw, bh);
    draw_line(r.x + 6.0 * s, y, r.x + r.w - 6.0 * s, y, 1.0, col::BORDER);
    y += 8.0 * s;
    // ---- the open squad
    if let Some(sid) = g.mil.squad {
        draw_squad(g, sid, Rect::new(x, y, iw, r.y + r.h - pad - y));
    } else {
        g.font.draw("No squads yet: \"+ New squad\" makes one.", x, y, s, col::TEXT_DIM);
    }
    // ---- the burrow dropdown (drawn last, over the list)
    if g.mil.panel.dd_open {
        draw_dropdown(g, dd_anchor, dd_click);
    } else {
        g.mil.panel.dd_rect = None;
    }
}

/// The civilian alert block. Returns the y below it and the dropdown button (anchor).
fn draw_alert(g: &mut Game, x: f32, y: f32, w: f32, bh: f32) -> (f32, Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let active = g.civ_alert.filter(|id| g.sim.world.burrows.iter().any(|b| b.id == *id));
    if let Some(b) = active {
        // Active: a red banner that cannot be missed, one click lifts it.
        let name = g.sim.world.burrows.iter().find(|x| x.id == b).map(|x| x.name.clone()).unwrap_or_default();
        let h = 2.0 * lh + 12.0 * s;
        let br = Rect::new(x - 4.0 * s, y, w + 8.0 * s, h);
        let pulse = 0.75 + 0.25 * ((get_time() * 3.0).sin() as f32 * 0.5 + 0.5);
        draw_rectangle(br.x, br.y, br.w, br.h, Color::new(0.42 * pulse, 0.05, 0.04, 0.96));
        draw_rectangle_lines(br.x, br.y, br.w, br.h, 2.0, col::TEXT_BAD);
        let lw = 13.0 * 8.0 * s;
        let lb = Rect::new(br.x + br.w - lw - 6.0 * s, br.y + (h - bh) / 2.0, lw, bh);
        let maxc = ((lb.x - x - 8.0 * s) / (8.0 * s)) as usize;
        g.font.draw("CIVILIAN ALERT", x + 4.0 * s, y + 6.0 * s, s, col::TEXT_HI);
        let inside = g.sim.world.burrows.iter().find(|x| x.id == b).map(|x| x.tiles.len()).unwrap_or(0);
        g.font.draw(&clip(&format!("Everyone into {name}"), maxc), x + 4.0 * s, y + 6.0 * s + lh, s, col::TEXT);
        if g.ui.hovered(br) {
            g.ui.tooltip = Some((format!("All citizens stay inside the burrow {name} ({inside} tiles)"), vec2(br.x, br.y - 4.0)));
        }
        if text_button(&mut g.ui, &g.font, lb, "Lift alert", BtnState::Normal) {
            super::burrows::set_alert(g, None);
        }
        return (y + h, Rect::new(x, y, 0.0, 0.0));
    }
    g.font.draw("Defense", x, y, s, col::BORDER_HI);
    let y = y + lh;
    let pick = default_burrow(g);
    let has = pick.is_some();
    let ddw = (w * 0.48).floor();
    let dd = Rect::new(x, y, ddw, bh);
    let label = match pick.and_then(|id| g.sim.world.burrows.iter().find(|b| b.id == id)) {
        Some(b) => format!("Burrow: {}", b.name),
        None => "No burrows yet".to_string(),
    };
    let lmax = ((ddw - 3.0 * 8.0 * s) / (8.0 * s)) as usize;
    let st = if !has { BtnState::Disabled } else if g.mil.panel.dd_open { BtnState::Active } else { BtnState::Normal };
    // A dropdown button: label left, arrow right.
    let hov = g.ui.hovered(dd) && has;
    draw_rectangle(dd.x, dd.y, dd.w, dd.h, if st == BtnState::Active { col::BUTTON_ACTIVE } else if hov { col::BUTTON_HOVER } else { col::BUTTON });
    draw_rectangle_lines(dd.x, dd.y, dd.w, dd.h, 1.0, if hov { col::BORDER_HI } else { Color::new(0.35, 0.35, 0.38, 1.0) });
    let ty = dd.y + (bh - GH * s) / 2.0;
    g.font.draw(&clip(&label, lmax), dd.x + 6.0 * s, ty, s, if has { col::TEXT } else { col::TEXT_DIM });
    g.font.draw("▼", dd.x + dd.w - 14.0 * s, ty, s, if has { col::TEXT_HI } else { col::TEXT_DIM });
    if has && g.ui.clicked(dd) {
        g.mil.panel.dd_open = !g.mil.panel.dd_open;
    }
    let ab = Rect::new(dd.x + dd.w + 6.0 * s, y, w - ddw - 6.0 * s, bh);
    let hov = g.ui.hovered(ab) && has;
    let bg = if !has { col::BUTTON } else if hov { Color::new(0.62, 0.14, 0.10, 1.0) } else { Color::new(0.45, 0.10, 0.08, 1.0) };
    draw_rectangle(ab.x, ab.y, ab.w, ab.h, bg);
    draw_rectangle_lines(ab.x, ab.y, ab.w, ab.h, 1.0, if hov { col::TEXT_HI } else if has { col::TEXT_BAD } else { Color::new(0.35, 0.35, 0.38, 1.0) });
    let at = clip("Everyone into the burrow!", ((ab.w - 8.0 * s) / (8.0 * s)) as usize);
    g.font.draw(&at, ab.x + (ab.w - g.font.width(&at, s)) / 2.0, ty, s, if has { col::TEXT_HI } else { col::TEXT_DIM });
    if hov {
        g.ui.tooltip = Some(("Civilian alert: all citizens go into the burrow and stay there".into(), vec2(ab.x, ab.y - 4.0)));
    }
    if has && g.ui.clicked(ab) {
        g.mil.panel.dd_open = false;
        super::burrows::set_alert(g, pick);
    }
    let mut y = y + bh + 2.0 * s;
    if !has {
        g.font.draw("Burrows (Shift+u) > + New burrow, then sound the alert here.", x, y, s, col::TEXT_DIM);
        y += lh;
    }
    (y, dd)
}

/// The burrow list under the dropdown button; `click` = this frame's click landed on it.
fn draw_dropdown(g: &mut Game, anchor: Rect, click: bool) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    if anchor.w <= 0.0 {
        g.mil.panel.dd_open = false;
        return;
    }
    let rows: Vec<(i32, String, usize)> = g.sim.world.burrows.iter().map(|b| (b.id, b.name.clone(), b.tiles.len())).collect();
    let row_h = lh + 6.0 * s;
    let w = anchor.w * 1.6;
    let h = rows.len().min(12) as f32 * row_h + 8.0 * s;
    let r = Rect::new(anchor.x, anchor.y + anchor.h + 2.0 * s, w, h);
    g.mil.panel.dd_rect = Some(r);
    g.ui.block(r);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.07, 0.065, 0.06, 1.0));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, col::BORDER_HI);
    let cur = default_burrow(g);
    let mut chosen = None;
    let maxc = ((w - 18.0 * 8.0 * s) / (8.0 * s)) as usize;
    for (k, (id, name, tiles)) in rows.iter().enumerate().take(12) {
        let rr = Rect::new(r.x + 4.0 * s, r.y + 4.0 * s + k as f32 * row_h, w - 8.0 * s, row_h - 2.0 * s);
        let hov = rr.contains(g.ui.mouse);
        if Some(*id) == cur {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, col::BUTTON_ACTIVE);
        } else if hov {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, col::BUTTON_HOVER);
        }
        let ty = rr.y + (rr.h - GH * s) / 2.0;
        let ci = g.sim.world.burrows.iter().position(|b| b.id == *id).unwrap_or(0);
        draw_rectangle(rr.x + 4.0 * s, ty + 2.0 * s, 8.0 * s, 8.0 * s, super::burrows::burrow_color(ci));
        g.font.draw(&clip(name, maxc), rr.x + 18.0 * s, ty, s, if hov { col::TEXT_HI } else { col::TEXT });
        let t = format!("{tiles} tiles");
        g.font.draw(&t, rr.x + rr.w - g.font.width(&t, s) - 4.0 * s, ty, s, col::TEXT_DIM);
        if click && hov {
            chosen = Some(*id);
        }
    }
    if let Some(id) = chosen {
        g.mil.panel.alert_burrow = Some(id);
        g.mil.panel.dd_open = false;
    }
}

/// Short routine names for the routine buttons (tooltips give the full DF name).
fn routine_short(name: &str) -> &str {
    match name {
        "Staggered training" => "Staggered",
        "Constant training" => "Constant",
        n => n,
    }
}

/// The squad list: a checkbox per squad (shared orders), click a row to open the squad.
fn draw_squad_list(g: &mut Game, x: f32, y: f32, w: f32) -> f32 {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let mut y = y;
    let ids: Vec<i32> = g.sim.world.squads.iter().map(|q| q.id).collect();
    let n = ids.len();
    let nchk = g.mil.panel.checked.len();
    let all = n > 0 && nchk == n;
    if n > 0 && checkbox(&mut g.ui, &g.font, x, y, all, "Select all") {
        g.mil.panel.checked = if all { Vec::new() } else { ids.clone() };
    }
    if nchk > 0 {
        let t = format!("{nchk} selected");
        g.font.draw(&t, x + w - g.font.width(&t, s), y, s, col::TEXT_HI);
    } else if n > 0 {
        let t = "tick squads to order several";
        g.font.draw(t, x + w - g.font.width(t, s), y, s, col::TEXT_DIM);
    }
    y += lh + 4.0 * s;
    let row_h = lh + 5.0 * s;
    // Leave the open squad room for its header and about five member rows (more for the
    // uniform editor of the Equipment tab).
    let pr = panel_rect(g);
    let reserve = if g.r9.sq_tab == 1 { 480.0 } else { 360.0 } * s;
    let max_rows = (((pr.y + pr.h - y - reserve) / row_h).max(0.0) as usize).clamp(3, 10);
    let rows = n.min(max_rows).max(1);
    let list = Rect::new(x - 4.0 * s, y, w + 8.0 * s, rows as f32 * row_h);
    let step = wheel_over(&g.ui, &mut g.input, list);
    g.mil.scroll = (g.mil.scroll as i32 + step).clamp(0, n.saturating_sub(rows) as i32) as usize;
    let data: Vec<(i32, String, usize, usize, String, String)> = g
        .sim
        .world
        .squads
        .iter()
        .map(|q| {
            let filled = q.members.iter().filter(|m| **m >= 0).count();
            let sched = if q.schedule.is_empty() { "-".to_string() } else { routine_short(&q.schedule).to_string() };
            (q.id, squad_name(&g.sim.world, q.id), filled, q.members.len().max(10), sched, orders_summary(&g.sim.world, q))
        })
        .collect();
    let bs = GH * s;
    let name_c = 17usize;
    let mut open = None;
    let mut toggle = None;
    for (id, name, filled, slots, sched, ord) in data.iter().skip(g.mil.scroll).take(rows) {
        let rr = Rect::new(x - 4.0 * s, y - 2.0 * s, w + 8.0 * s, row_h - 1.0 * s);
        let cbr = Rect::new(x - 2.0 * s, y - 2.0 * s, bs + 6.0 * s, row_h - 1.0 * s);
        if g.ui.clicked(cbr) {
            toggle = Some(*id);
        }
        if list_row(&mut g.ui, rr, g.mil.squad == Some(*id)) {
            open = Some(*id);
        }
        let on = g.mil.panel.checked.contains(id);
        let hov = g.ui.hovered(cbr);
        draw_rectangle(x, y, bs, bs, if on { col::BUTTON_ACTIVE } else { col::BUTTON });
        draw_rectangle_lines(x, y, bs, bs, 1.0, if hov { col::BORDER_HI } else { Color::new(0.45, 0.45, 0.5, 1.0) });
        if on {
            g.font.draw("√", x + (bs - 8.0 * s) / 2.0, y, s, col::TEXT_GOOD);
        }
        let mut tx = x + bs + 8.0 * s;
        g.font.draw(&clip(name, name_c), tx, y, s, col::TEXT);
        tx += (name_c as f32 + 1.0) * 8.0 * s;
        let cnt = format!("{filled:>2}/{slots}");
        g.font.draw(&cnt, tx, y, s, if *filled == 0 { col::TEXT_DIM } else { col::TEXT_GOOD });
        tx += 6.0 * 8.0 * s;
        g.font.draw(&clip(sched, 9), tx, y, s, col::TEXT_DIM);
        tx += 10.0 * 8.0 * s;
        let oc = if ord == "No orders" { col::TEXT_DIM } else { col::TEXT_CYAN };
        let maxc = ((x + w - tx) / (8.0 * s)).max(1.0) as usize;
        g.font.draw(&clip(ord, maxc), tx, y, s, oc);
        y += row_h;
    }
    if n > rows {
        // Scroll hint: "3-8 of 9".
        let t = format!("{}-{} of {n} (wheel scrolls)", g.mil.scroll + 1, (g.mil.scroll + rows).min(n));
        g.font.draw(&t, x + w - g.font.width(&t, s), y, s, col::TEXT_DIM);
        y += lh;
    }
    if let Some(id) = toggle {
        let c = &mut g.mil.panel.checked;
        if let Some(i) = c.iter().position(|q| *q == id) {
            c.remove(i);
        } else {
            c.push(id);
        }
    } else if let Some(id) = open {
        if g.mil.squad != Some(id) {
            g.mil.panel.mscroll = 0;
        }
        g.mil.squad = Some(id);
        g.mil.picker = None;
        g.mil.kill_list = false;
    }
    y + 4.0 * s
}

/// Order and routine buttons for the order targets (ticked squads, else the open squad).
fn draw_orders(g: &mut Game, x: f32, y: f32, w: f32, bh: f32) -> f32 {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let mut y = y;
    let t = order_targets(g);
    let multi = !g.mil.panel.checked.is_empty();
    let head = match (t.len(), multi) {
        (0, _) => "Orders: no squad".to_string(),
        (_, true) => format!("Orders for the {} ticked squad{}", t.len(), if t.len() == 1 { "" } else { "s" }),
        _ => format!("Orders for {}", targets_name(g, &t)),
    };
    g.font.draw(&clip(&head, (w / (8.0 * s)) as usize), x, y, s, if multi { col::TEXT_HI } else { col::BORDER_HI });
    y += lh + 2.0 * s;
    let gap = 6.0 * s;
    let bw = ((w - 3.0 * gap) / 4.0).floor();
    let st_on = |q: i32| t.contains(&q);
    let station_on = matches!(g.tool, Tool::SquadStation(q) if st_on(q));
    let kill_on = g.mil.kill_list || matches!(g.tool, Tool::SquadKill(q) if st_on(q));
    let none = t.is_empty();
    let state = |on: bool| if none { BtnState::Disabled } else if on { BtnState::Active } else { BtnState::Normal };
    let labels: [(&str, bool, &str); 4] = [
        ("Station", station_on, "Station: click a tile on the map (right-click cancels)"),
        ("Kill", kill_on, "Kill: pick a hostile from the list or a creature on the map"),
        ("Train", false, "Train (follow the routine)"),
        ("Cancel orders", false, "Cancel all orders"),
    ];
    for (k, (label, on, tip)) in labels.iter().enumerate() {
        let br = Rect::new(x + k as f32 * (bw + gap), y, bw, bh);
        if g.ui.hovered(br) {
            g.ui.tooltip = Some((tip.to_string(), vec2(br.x, br.y - 4.0)));
        }
        if !text_button(&mut g.ui, &g.font, br, label, state(*on)) {
            continue;
        }
        let who = targets_name(g, &t);
        match k {
            0 => {
                if station_on {
                    g.tool = Tool::None;
                } else {
                    g.mil.panel.pending = t.clone();
                    g.mil.kill_list = false;
                    g.tool = Tool::SquadStation(t[0]);
                    g.set_status(format!("{who}: click a tile to station {} there", if t.len() == 1 { "the squad" } else { "them" }));
                }
            }
            1 => {
                g.mil.kill_list = !kill_on;
                if matches!(g.tool, Tool::SquadKill(_) | Tool::SquadStation(_)) {
                    g.tool = Tool::None;
                }
                g.mil.picker = None;
                g.mil.pscroll = 0;
            }
            2 => {
                for id in &t {
                    g.sim.order_squad(*id, SquadOrder::Train);
                }
                g.set_status(format!("{who}: train"));
            }
            _ => {
                for id in &t {
                    g.sim.order_squad(*id, SquadOrder::Cancel);
                }
                g.set_status(format!("{who}: orders cancelled"));
            }
        }
    }
    y += bh + 6.0 * s;
    // Routine (DF v50: Off duty / Staggered / Constant training / Ready).
    let options = g.sim.squad_schedules();
    let rw = ((w - 3.0 * gap) / 4.0).floor();
    for (k, name) in options.iter().enumerate().take(4) {
        let all_on = !none && t.iter().all(|id| g.sim.world.squads.iter().find(|q| q.id == *id).map(|q| q.schedule.eq_ignore_ascii_case(name)).unwrap_or(false));
        let br = Rect::new(x + k as f32 * (rw + gap), y, rw, bh);
        if g.ui.hovered(br) {
            g.ui.tooltip = Some((format!("Routine: {name}"), vec2(br.x, br.y - 4.0)));
        }
        if text_button(&mut g.ui, &g.font, br, routine_short(name), state(all_on)) {
            for id in &t {
                g.sim.set_squad_schedule(*id, name);
            }
            g.set_status(format!("{}: {}", targets_name(g, &t), name.to_lowercase()));
        }
    }
    y + bh + 8.0 * s
}

/// The open squad: name, orders, tabs, and the members (or the picker / kill list / a tab).
fn draw_squad(g: &mut Game, sid: i32, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(squad) = g.sim.world.squads.iter().find(|q| q.id == sid).cloned() else { return };
    let (x, mut y) = (area.x, area.y);
    let w = area.w;
    let name = squad_name(&g.sim.world, sid);
    g.font.draw(&clip(&name, 30), x, y, 1.5 * s, col::TEXT_HI);
    if !squad.alias.is_empty() && squad.alias != squad.name && !squad.name.is_empty() {
        let t = format!("({})", squad.name);
        g.font.draw(&clip(&t, 24), x + g.font.width(&clip(&name, 30), 1.5 * s) + 10.0 * s, y + 4.0 * s, s, col::TEXT_DIM);
    }
    if let Some(p) = station_pos(&squad) {
        let br = Rect::new(x + w - 14.0 * 8.0 * s, y - 2.0 * s, 14.0 * 8.0 * s, 18.0 * s);
        if text_button(&mut g.ui, &g.font, br, "Show station", BtnState::Normal) {
            g.cam.focus(p);
        }
    }
    y += 1.5 * GH * s + 6.0 * s;
    let sched = if squad.schedule.is_empty() { "no routine".to_string() } else { squad.schedule.clone() };
    let ord = orders_summary(&g.sim.world, &squad);
    g.font.draw(&clip(&sched, 20), x, y, s, col::TEXT_DIM);
    let ox = x + (sched.chars().count().min(20) as f32 + 2.0) * 8.0 * s;
    g.font.draw(&clip(&ord, ((x + w - ox) / (8.0 * s)) as usize), ox, y, s, if squad.orders.is_empty() { col::TEXT_DIM } else { col::TEXT_CYAN });
    y += lh + 6.0 * s;
    // [app9] Members, Equipment, Ammunition, Patrol and barracks.
    y = crate::ui::round9::squad_tabs(g, x, y) - 4.0 * s;
    let area = Rect::new(x, y, w, area.y + area.h - y);
    if g.mil.kill_list {
        draw_kill_list(g, sid, area);
        return;
    }
    if let Some(pos) = g.mil.picker {
        draw_picker(g, sid, pos, area);
        return;
    }
    if g.r9.sq_tab > 0 {
        crate::ui::round9::draw_squad_tab(g, sid, area); // [app9]
        return;
    }
    draw_members(g, &squad, area);
}

/// Uniform slots shown per member: (item kind, label).
pub const PREVIEW_SLOTS: &[(&str, &str)] = &[("ARMOR", "Body"), ("HELM", "Head"), ("PANTS", "Legs"), ("GLOVES", "Hands"), ("SHOES", "Feet"), ("SHIELD", "Shield"), ("WEAPON", "Weapon")];

/// What a member has in one uniform slot.
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    /// Real armor (or the shield / weapon): item index into `world.items`.
    Worn(usize),
    /// Only clothing there (a shirt, a cap): item index, and whether the uniform wants armor
    /// there (then it is a warning: the squad uniform takes any item of the kind).
    Clothing(usize, bool),
    /// The uniform asks for it and nothing fitting is worn (index of what is worn instead).
    Missing(Option<usize>),
    /// Not in the uniform and not worn.
    Empty,
}

/// Armor rather than clothing: DF's armor layer, hard non-soft items (mail), or metal.
pub fn is_armor(w: &World, it: &rf_core::Item) -> bool {
    if matches!(it.kind.as_str(), "SHIELD" | "WEAPON") {
        return true;
    }
    let metal = w.material(it.mat).map(|m| m.is_metal).unwrap_or(false);
    match w.armor_raws.iter().find(|r| r.token == it.subtype) {
        Some(r) => r.layer == "ARMOR" || metal || (r.flags.iter().any(|f| f == "HARD") && !r.flags.iter().any(|f| f == "SOFT")),
        None => metal,
    }
}

/// Item kind of a uniform entry (`KIND[:SUBTYPE][@material]`).
fn entry_kind(e: &str) -> &str {
    e.split(|c| c == '@' || c == ':').next().unwrap_or("")
}

/// The slots of one member: worn item indices (into `world.items`), the uniform entries
/// still missing and the position's uniform (`Sim::squad_equipment`).
pub fn member_slots(w: &World, worn: &[usize], missing: &[String], uniform: &[String]) -> Vec<Slot> {
    PREVIEW_SLOTS
        .iter()
        .map(|(kind, _)| {
            let items: Vec<usize> = worn.iter().copied().filter(|&i| w.items[i].kind == *kind).collect();
            let armor = items.iter().copied().find(|&i| is_armor(w, &w.items[i]));
            let miss = missing.iter().any(|e| entry_kind(e) == *kind);
            let wanted = uniform.iter().any(|e| entry_kind(e) == *kind);
            match (miss, armor, items.first().copied()) {
                (true, a, first) => Slot::Missing(a.or(first)),
                (false, Some(i), _) => Slot::Worn(i),
                (false, None, Some(i)) => Slot::Clothing(i, wanted),
                (false, None, None) => Slot::Empty,
            }
        })
        .collect()
}

/// Member rows: the unit as drawn on the map, name, combat skill, and the uniform slots.
fn draw_members(g: &mut Game, squad: &Squad, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let sid = squad.id;
    let (x, mut y) = (area.x, area.y);
    let w = area.w;
    // Equipment of every position; worn item ids -> item indices in one pass.
    let eq = g.sim.squad_equipment(sid);
    let ids: std::collections::HashSet<i32> = eq.iter().flat_map(|e| e.worn.iter().map(|(id, _)| *id)).collect();
    let mut idx: std::collections::HashMap<i32, usize> = std::collections::HashMap::with_capacity(ids.len());
    if !ids.is_empty() {
        for (i, it) in g.sim.world.items.iter().enumerate() {
            if ids.contains(&it.id) {
                idx.insert(it.id, i);
            }
        }
    }
    let filled = squad.members.iter().filter(|m| **m >= 0).count();
    let lacking = eq.iter().filter(|e| e.unit >= 0 && !e.missing.is_empty()).count();
    g.font.draw(&format!("Members ({filled}/{})", squad.members.len().max(10)), x, y, s, col::BORDER_HI);
    if lacking > 0 {
        let t = format!("{lacking} without full uniform");
        g.font.draw(&t, x + w - g.font.width(&t, s), y, s, col::TEXT_BAD);
    } else if filled > 0 {
        let t = "all fully equipped";
        g.font.draw(t, x + w - g.font.width(t, s), y, s, col::TEXT_GOOD);
    }
    y += lh + 2.0 * s;
    let slots_n = squad.members.len().max(10);
    let row_h = 36.0 * s;
    // Empty positions get a short row, so a full squad of 10 fits.
    let hts: Vec<f32> = (0..slots_n).map(|i| if squad.members.get(i).copied().unwrap_or(-1) >= 0 { row_h } else { lh + 8.0 * s }).collect();
    let list = Rect::new(x - 4.0 * s, y, w + 8.0 * s, area.y + area.h - y);
    let fits_from = |room: f32| {
        let (mut acc, mut k) = (0.0, slots_n);
        while k > 0 && acc + hts[k - 1] <= room {
            acc += hts[k - 1];
            k -= 1;
        }
        k
    };
    let mut max_scroll = fits_from(list.h);
    if max_scroll > 0 {
        max_scroll = fits_from(list.h - lh); // room for the "1-8 of 10" line
    }
    let bottom = list.y + list.h - if max_scroll > 0 { lh } else { 0.0 };
    let step = wheel_over(&g.ui, &mut g.input, list);
    g.mil.panel.mscroll = (g.mil.panel.mscroll as i32 + step).clamp(0, max_scroll as i32) as usize;
    let ssz = 22.0 * s;
    let sgap = 3.0 * s;
    let slots_w = PREVIEW_SLOTS.len() as f32 * (ssz + sgap);
    let xb = Rect::new(x + w - 20.0 * s, 0.0, 20.0 * s, 18.0 * s);
    let sx0 = xb.x - 8.0 * s - slots_w;
    let tx = x + 40.0 * s;
    let maxc = ((sx0 - tx - 6.0 * s) / (8.0 * s)).max(4.0) as usize;
    let mut sprites = std::mem::take(&mut g.rend.buf);
    let mut action: Option<(usize, bool)> = None;
    let mut jump = None;
    let mut last = g.mil.panel.mscroll;
    for i in g.mil.panel.mscroll..slots_n {
        let row_h = hts[i];
        if y + row_h > bottom + 0.5 {
            break;
        }
        last = i + 1;
        let m = squad.members.get(i).copied().unwrap_or(-1);
        let rr = Rect::new(x - 4.0 * s, y, w + 8.0 * s, row_h - 2.0 * s);
        let unit = (m >= 0).then(|| g.sim.world.unit(m)).flatten().cloned();
        let sel = unit.as_ref().map(|u| g.selected == Selected::Unit(u.id)).unwrap_or(false);
        if sel {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.22, 0.42, 0.20, 0.55));
            draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, col::ACTIVE_BORDER);
        } else if i % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.035));
        }
        let ty = y + (row_h - 2.0 * s - 2.0 * lh) / 2.0 + 1.0 * s;
        let xr = Rect::new(xb.x, y + (row_h - 2.0 * s - xb.h) / 2.0, xb.w, xb.h);
        match &unit {
            Some(u) => {
                // The unit's map sprite (body, clothing and worn equipment layers).
                let pr = Rect::new(x, y + (row_h - 2.0 * s - 32.0 * s) / 2.0, 32.0 * s, 32.0 * s);
                draw_rectangle(pr.x, pr.y, pr.w, pr.h, Color::new(0.0, 0.0, 0.0, 0.35));
                sprites.clear();
                g.gfx.unit(&g.sim.world, u, &mut sprites);
                let mut drawn = false;
                for sp in &sprites {
                    let off = sp.unit_center_px(32) as f32 * pr.w / 32.0;
                    drawn |= g.rend.draw_sprite(&g.gfx, sp, pr.x + off, pr.y, pr.w);
                }
                if !drawn {
                    let code = if u.race == "DWARF" { 1 } else { u.race.bytes().next().unwrap_or(b'?') };
                    g.font.glyph(code, pr.x + 6.0 * s, pr.y + 2.0 * s, 20.0 * s, 28.0 * s, col::TEXT_HI);
                }
                let dead = u.flags.dead;
                let hov = g.ui.hovered(Rect::new(rr.x, rr.y, sx0 - rr.x, rr.h));
                let name = format!("{:>2}. {}{}", i + 1, unit_name(u), if dead { " (dead)" } else { "" });
                g.font.draw(&clip(&name, maxc), tx, ty, s, if dead { col::TEXT_BAD } else if hov || sel { col::TEXT_HI } else { col::TEXT });
                let sk = best_combat_skill(u).map(|(t, _)| t).unwrap_or_else(|| unit_profession(u));
                g.font.draw(&clip(&sk, maxc.saturating_sub(4)), tx + 4.0 * 8.0 * s, ty + lh, s, col::TEXT_DIM);
                // Uniform slots.
                let row = eq.iter().find(|e| e.position == i);
                let worn: Vec<usize> = row.map(|e| e.worn.iter().filter_map(|(id, _)| idx.get(id).copied()).collect()).unwrap_or_default();
                let missing: Vec<String> = row.map(|e| e.missing.clone()).unwrap_or_default();
                let uniform: Vec<String> = row.map(|e| e.uniform.clone()).unwrap_or_default();
                let st = member_slots(&g.sim.world, &worn, &missing, &uniform);
                let sy = y + (row_h - 2.0 * s - ssz) / 2.0;
                for (k, slot) in st.iter().enumerate() {
                    let r = Rect::new(sx0 + k as f32 * (ssz + sgap), sy, ssz, ssz);
                    draw_slot(g, &mut sprites, r, k, slot, &missing);
                }
                if text_button(&mut g.ui, &g.font, xr, "x", BtnState::Normal) {
                    action = Some((i, false));
                } else if g.ui.hovered(xr) {
                    g.ui.tooltip = Some(("Remove from the squad".into(), vec2(xr.x, xr.y - 4.0)));
                }
                if !dead && g.ui.clicked(Rect::new(rr.x, rr.y, sx0 - rr.x, rr.h)) {
                    jump = Some((u.id, u.pos));
                }
            }
            None => {
                let t = if m >= 0 { format!("{:>2}. unit #{m} (not on the map)", i + 1) } else { format!("{:>2}. (empty)", i + 1) };
                g.font.draw(&t, tx, y + (row_h - 2.0 * s - GH * s) / 2.0, s, col::TEXT_DIM);
                let ab = Rect::new(x + w - 11.0 * 8.0 * s, y + (row_h - 2.0 * s - 18.0 * s) / 2.0, 11.0 * 8.0 * s, 18.0 * s);
                if m >= 0 {
                    if text_button(&mut g.ui, &g.font, xr, "x", BtnState::Normal) {
                        action = Some((i, false));
                    }
                } else if text_button(&mut g.ui, &g.font, ab, "+ Assign", BtnState::Normal) {
                    action = Some((i, true));
                }
            }
        }
        y += row_h;
    }
    g.rend.buf = sprites;
    if max_scroll > 0 {
        let t = format!("{}-{last} of {slots_n} (wheel scrolls)", g.mil.panel.mscroll + 1);
        g.font.draw(&t, x + w - g.font.width(&t, s), area.y + area.h - lh + 2.0 * s, s, col::TEXT_DIM);
    } else if y + 2.0 * lh < area.y + area.h {
        // Room left: the squad uniform in one line.
        y += 6.0 * s;
        let u = if squad.uniform.is_empty() {
            "No uniform: members fight with what they carry.".to_string()
        } else {
            let parts: Vec<String> = squad.uniform.iter().map(|e| crate::ui::round9::entry_text(&g.sim.world, e)).collect();
            format!("Uniform: {}", parts.join(", "))
        };
        let maxc = (w / (8.0 * s)) as usize;
        let mut line = String::new();
        for word in u.split(' ') {
            if line.chars().count() + word.chars().count() + 1 > maxc && !line.is_empty() {
                if y + lh > area.y + area.h {
                    break;
                }
                g.font.draw(&line, x, y, s, col::TEXT_DIM);
                y += lh;
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() && y + lh <= area.y + area.h {
            g.font.draw(&line, x, y, s, col::TEXT_DIM);
        }
    }
    match action {
        Some((i, true)) => {
            g.mil.picker = Some(i);
            g.mil.pscroll = 0;
        }
        Some((i, false)) => unassign(g, sid, i),
        None => {}
    }
    if let Some((id, p)) = jump {
        // The unit sheet opens at the right, next to the panel; the map follows.
        g.select_unit(id);
        if g.unit_tab == 9 {
            g.unit_tab = 0;
        }
        g.cam.focus(p);
    }
}

/// One uniform slot box: the worn item's sprite, green when it is armor, red when missing.
fn draw_slot(g: &mut Game, sprites: &mut Vec<rf_gfx::Sprite>, r: Rect, k: usize, slot: &Slot, missing: &[String]) {
    let s = g.ui.scale;
    let (kind, label) = PREVIEW_SLOTS[k];
    let (border, item, alpha) = match slot {
        Slot::Worn(i) => (col::TEXT_GOOD, Some(*i), 1.0),
        Slot::Clothing(i, true) => (Color::new(0.95, 0.62, 0.20, 1.0), Some(*i), 0.7),
        Slot::Clothing(i, false) => (Color::new(0.45, 0.45, 0.48, 1.0), Some(*i), 0.55),
        Slot::Missing(i) => (col::TEXT_BAD, *i, 0.45),
        Slot::Empty => (Color::new(0.25, 0.25, 0.27, 1.0), None, 1.0),
    };
    draw_rectangle(r.x, r.y, r.w, r.h, match slot {
        Slot::Missing(_) => Color::new(0.30, 0.06, 0.05, 0.9),
        _ => Color::new(0.0, 0.0, 0.0, 0.45),
    });
    let mut drawn = false;
    if let Some(i) = item {
        sprites.clear();
        let w = &g.sim.world;
        g.gfx.item(w, &w.items[i], sprites);
        for sp in sprites.iter() {
            let sp = if alpha < 1.0 { sp.mul_tint([255, 255, 255, (alpha * 255.0) as u8]) } else { *sp };
            drawn |= g.rend.draw_sprite(&g.gfx, &sp, r.x + 1.0 * s, r.y + 1.0 * s, r.w - 2.0 * s);
        }
    }
    if !drawn && item.is_some() {
        let glyph = match kind {
            "ARMOR" | "PANTS" | "GLOVES" | "SHOES" => b'[',
            "HELM" => b']',
            "SHIELD" => b')',
            _ => b'/',
        };
        g.font.glyph(glyph, r.x + 5.0 * s, r.y + 2.0 * s, r.w - 10.0 * s, r.h - 4.0 * s, Color::new(border.r, border.g, border.b, alpha));
    }
    if matches!(slot, Slot::Missing(None)) {
        // A red cross over the empty slot.
        let m = 5.0 * s;
        draw_line(r.x + m, r.y + m, r.x + r.w - m, r.y + r.h - m, 2.0 * s, col::TEXT_BAD);
        draw_line(r.x + r.w - m, r.y + m, r.x + m, r.y + r.h - m, 2.0 * s, col::TEXT_BAD);
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, if matches!(slot, Slot::Worn(_) | Slot::Missing(_) | Slot::Clothing(_, true)) { 2.0 } else { 1.0 }, border);
    if g.ui.hovered(r) {
        let w = &g.sim.world;
        let desc = |i: usize| {
            let it = &w.items[i];
            if it.desc.is_empty() { pretty(&it.kind).to_lowercase() } else { it.desc.clone() }
        };
        let need: Vec<String> = missing.iter().filter(|e| entry_kind(e) == kind).map(|e| crate::ui::round9::entry_text(w, e)).collect();
        let t = match slot {
            Slot::Worn(i) => format!("{label}: {}", desc(*i)),
            Slot::Clothing(i, true) => format!("{label}: only {} (clothing, no armor)", desc(*i)),
            Slot::Clothing(i, false) => format!("{label}: {} (clothing)", desc(*i)),
            Slot::Missing(Some(i)) => format!("{label}: missing, wears {} instead\nUniform: {}", desc(*i), need.join(", ")),
            Slot::Missing(None) => format!("{label}: missing\nUniform: {}", need.join(", ")),
            Slot::Empty => format!("{label}: nothing (not in the uniform)"),
        };
        g.ui.tooltip = Some((t, vec2(r.x, r.y - 4.0)));
    }
}

/// Citizen picker for one squad position: citizens not in a squad first, best fighters first.
fn draw_picker(g: &mut Game, sid: i32, pos: usize, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (x, mut y) = (area.x, area.y);
    g.font.draw(&format!("Choose a citizen for position {}", pos + 1), x, y, s, col::BORDER_HI);
    let br = Rect::new(area.x + area.w - 10.0 * 8.0 * s, y - 4.0 * s, 10.0 * 8.0 * s, lh + 2.0 * s);
    if text_button(&mut g.ui, &g.font, br, "Back", BtnState::Normal) {
        g.mil.picker = None;
        return;
    }
    y += lh + 4.0 * s;
    let w = &g.sim.world;
    let mut c: Vec<(i32, String, String, i32, String)> = w
        .citizens()
        .filter(|u| u.profession != "CHILD" && u.profession != "BABY")
        .map(|u| {
            let (sk, rating) = best_combat_skill(u).unwrap_or_else(|| ("-".into(), 0));
            let sq = if u.squad_id >= 0 { squad_name(w, u.squad_id) } else { String::new() };
            (u.id, unit_name(u), sk, rating, sq)
        })
        .collect();
    c.sort_by(|a, b| a.4.is_empty().cmp(&b.4.is_empty()).reverse().then(b.3.cmp(&a.3)).then(a.1.cmp(&b.1)));
    let rows = (((area.y + area.h) - y) / lh) as usize;
    let step = wheel_over(&g.ui, &mut g.input, area);
    g.mil.pscroll = (g.mil.pscroll as i32 + step * 3).clamp(0, c.len().saturating_sub(rows) as i32) as usize;
    let mut chosen = None;
    let cw = area.w;
    // [mil-ui] Columns follow the width (the side panel is narrower than the old screen).
    let cc = (cw / (8.0 * s)) as usize;
    let (nc, kc, qc) = if cc >= 80 { (28, 30, 18) } else { (cc * 40 / 100, cc * 34 / 100, cc * 22 / 100) };
    for (id, name, sk, _, sq) in c.iter().skip(g.mil.pscroll).take(rows) {
        let rr = Rect::new(x - 4.0 * s, y - 2.0 * s, cw + 4.0 * s, lh);
        if list_row(&mut g.ui, rr, false) {
            chosen = Some(*id);
        }
        g.font.draw(&clip(name, nc), x, y, s, if sq.is_empty() { col::TEXT } else { col::TEXT_DIM });
        g.font.draw(&clip(sk, kc), x + (nc as f32 + 1.0) * 8.0 * s, y, s, col::TEXT_GOOD);
        if !sq.is_empty() {
            let t = clip(sq, qc);
            g.font.draw(&t, x + cw - g.font.width(&t, s), y, s, col::TEXT_DIM);
        }
        y += lh;
    }
    if let Some(id) = chosen {
        assign(g, sid, pos, id);
        let who = g.sim.world.unit(id).map(unit_name).unwrap_or_default();
        g.set_status(format!("{who} joins {} (position {})", squad_name(&g.sim.world, sid), pos + 1));
        g.mil.picker = None;
    }
}

/// Kill order targets: the hostile creatures on the map, or "pick on the map". [mil-ui] The
/// order goes to every ticked squad (else the open one).
fn draw_kill_list(g: &mut Game, sid: i32, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (x, mut y) = (area.x, area.y);
    let mut t = order_targets(g);
    if t.is_empty() {
        t = vec![sid];
    }
    let bw = 13.0 * 8.0 * s;
    let title = format!("Kill: a target for {}", targets_name(g, &t));
    g.font.draw(&clip(&title, ((area.w - bw - 8.0 * s) / (8.0 * s)) as usize), x, y, s, col::BORDER_HI);
    let br = Rect::new(area.x + area.w - bw, y - 4.0 * s, bw, lh + 2.0 * s);
    if text_button(&mut g.ui, &g.font, br, "Pick on map", BtnState::Normal) {
        g.mil.panel.pending = t.clone();
        g.tool = Tool::SquadKill(t[0]);
        g.mil.kill_list = false;
        g.set_status(format!("{}: click a creature to attack", targets_name(g, &t)));
        return;
    }
    y += lh + 6.0 * s;
    let w = &g.sim.world;
    let mut list: Vec<(i32, String, String, Pos, bool)> = g
        .hostiles
        .iter()
        .filter_map(|id| w.unit(*id))
        .filter(|u| !u.flags.dead)
        .map(|u| (u.id, unit_name(u), pretty(&u.race).to_lowercase(), u.pos, u.flags.caged))
        .collect();
    list.sort_by(|a, b| a.4.cmp(&b.4).then(a.1.cmp(&b.1)));
    if list.is_empty() {
        let maxc = (area.w / (8.0 * s)) as usize;
        g.font.draw(&clip("No hostile creatures on the map.", maxc), x, y, s, col::TEXT_DIM);
        g.font.draw(&clip("\"Pick on map\" targets any other creature.", maxc), x, y + lh, s, col::TEXT_DIM);
        return;
    }
    let cc = (area.w / (8.0 * s)) as usize;
    let (nc, rc) = if cc >= 80 { (30, 20) } else { (cc * 40 / 100, cc * 22 / 100) };
    let mut chosen = None;
    let mut go = None;
    for (id, name, race, p, caged) in list.iter().take((((area.y + area.h) - y) / lh) as usize) {
        let rr = Rect::new(x - 4.0 * s, y - 2.0 * s, area.w + 4.0 * s, lh);
        if list_row(&mut g.ui, rr, false) {
            chosen = Some(*id);
        }
        if g.ui.hovered(rr) {
            go = Some(*p);
        }
        g.font.draw("!", x, y, s, col::TEXT_BAD);
        g.font.draw(&clip(name, nc), x + 16.0 * s, y, s, if *caged { col::TEXT_DIM } else { col::TEXT });
        g.font.draw(&clip(race, rc), x + 16.0 * s + (nc as f32 + 1.0) * 8.0 * s, y, s, col::TEXT_DIM);
        let loc = format!("{}{}, {}, z {}", if *caged { "caged " } else { "" }, p.x, p.y, p.z);
        g.font.draw(&loc, x + area.w - g.font.width(&loc, s), y, s, col::TEXT_DIM);
        y += lh;
    }
    if let Some(p) = go {
        g.ui.tooltip = Some((format!("At {}, {}, z {}", p.x, p.y, p.z), vec2(g.ui.mouse.x + 12.0, g.ui.mouse.y)));
    }
    if let Some(id) = chosen {
        for q in &t {
            g.sim.order_squad(*q, SquadOrder::Kill(id));
        }
        let who = g.sim.world.unit(id).map(unit_name).unwrap_or_default();
        g.set_status(format!("{}: kill {who}", targets_name(g, &t)));
        g.mil.kill_list = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_and_uniform_texts() {
        let mut w = World::synthetic(16, 16, 10);
        let id = w.units[0].id;
        assert_eq!(order_text(&w, "Station 10,20,5"), "Station at 10, 20, z 5");
        assert_eq!(order_text(&w, "Train"), "Train");
        assert!(order_text(&w, &format!("Kill {id}")).starts_with("Kill "));
        assert_eq!(uniform_text(&w, "WEAPON:ITEM_WEAPON_AXE_BATTLE"), "Weapon: axe battle");
        w.weapon_raws.push(rf_core::WeaponRaw { token: "ITEM_WEAPON_AXE_BATTLE".into(), name: "battle axe".into(), ..Default::default() });
        assert_eq!(uniform_text(&w, "WEAPON:ITEM_WEAPON_AXE_BATTLE"), "Weapon: battle axe");
        assert_eq!(uniform_text(&w, "HELM@Armor"), "Helm: any armor");
        let sq = Squad { id: 3, orders: vec!["Station 1,2,3".into()], ..Default::default() };
        assert_eq!(station_pos(&sq), Some(Pos::new(1, 2, 3)));
        w.squads.push(sq);
        assert_eq!(squad_name(&w, 3), "Squad #3");
        assert_eq!(squad_name(&w, 99), "Squad #99");
    }

    /// [mil-ui] Uniform slots of a member: armor vs clothing, missing pieces.
    #[test]
    fn member_slot_states() {
        let mut w = World::synthetic(16, 16, 10);
        w.armor_raws.push(rf_core::ArmorRaw { token: "ITEM_ARMOR_BREASTPLATE".into(), kind: "ARMOR".into(), layer: "ARMOR".into(), ..Default::default() });
        w.armor_raws.push(rf_core::ArmorRaw { token: "ITEM_ARMOR_SHIRT".into(), kind: "ARMOR".into(), layer: "UNDER".into(), flags: vec!["SOFT".into()], ..Default::default() });
        w.armor_raws.push(rf_core::ArmorRaw { token: "ITEM_ARMOR_MAIL_SHIRT".into(), kind: "ARMOR".into(), layer: "OVER".into(), flags: vec!["HARD".into(), "METAL".into()], ..Default::default() });
        let base = w.items.len();
        let mk = |id: i32, kind: &str, sub: &str| rf_core::Item { id: 900_000 + id, kind: kind.into(), subtype: sub.into(), ..Default::default() };
        w.items.push(mk(1, "ARMOR", "ITEM_ARMOR_BREASTPLATE"));
        w.items.push(mk(2, "ARMOR", "ITEM_ARMOR_SHIRT"));
        w.items.push(mk(3, "WEAPON", "ITEM_WEAPON_AXE_BATTLE"));
        w.items.push(mk(4, "ARMOR", "ITEM_ARMOR_MAIL_SHIRT"));
        let k = |kind: &str| PREVIEW_SLOTS.iter().position(|(x, _)| *x == kind).unwrap();
        let uni: Vec<String> = vec!["ARMOR@Armor".into(), "HELM@Armor".into(), "WEAPON:ITEM_WEAPON_AXE_BATTLE".into()];
        // Breastplate over a shirt and an axe; the helm is missing; no shield in the uniform.
        let st = member_slots(&w, &[base, base + 1, base + 2], &["HELM@Armor".into()], &uni);
        assert_eq!(st[k("ARMOR")], Slot::Worn(base));
        assert_eq!(st[k("WEAPON")], Slot::Worn(base + 2));
        assert_eq!(st[k("HELM")], Slot::Missing(None));
        assert_eq!(st[k("SHIELD")], Slot::Empty);
        // Only a shirt where the uniform wants body armor: a warning, not "worn".
        let st = member_slots(&w, &[base + 1], &[], &uni);
        assert_eq!(st[k("ARMOR")], Slot::Clothing(base + 1, true));
        let st = member_slots(&w, &[base + 1], &[], &[]);
        assert_eq!(st[k("ARMOR")], Slot::Clothing(base + 1, false));
        // Mail (hard, not soft) counts as armor even on the OVER layer.
        assert!(is_armor(&w, &w.items[base + 3]));
        assert!(!is_armor(&w, &w.items[base + 1]));
        assert_eq!(entry_kind("WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:STEEL"), "WEAPON");
        assert_eq!(entry_kind("HELM@Armor"), "HELM");
    }
}
