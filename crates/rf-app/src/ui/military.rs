//! Squads screen (DF hotkey `c`): squads with members, orders and uniforms. Orders go to
//! `Sim::order_squad` (Station: click a tile, Kill: click a creature or pick a hostile, Train,
//! Cancel); members are assigned with a citizen picker (`Sim::assign_squad_member`).

use super::orders::screen_rect;
use super::panels::{unit_name, unit_profession};
use super::{clip, col, list_row, panel, pretty, text_button, wheel_over, BtnState};
use crate::game::{Game, Screen, Tool};
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

pub fn draw_military(g: &mut Game) {
    if g.screen != Screen::Military {
        return;
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let r = screen_rect(g, 1180.0);
    panel(&mut g.ui, r);
    let pad = 12.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        g.screen = Screen::None;
        return;
    }
    let n = g.sim.world.squads.len();
    g.font.draw(&format!("Squads ({n})"), x, y, 1.5 * s, col::TEXT_HI);
    let hn = g.hostiles.iter().filter(|id| g.sim.world.unit(**id).map(|u| !u.flags.caged).unwrap_or(false)).count();
    if hn > 0 {
        let t = format!("{hn} hostile creature{} on the map", if hn == 1 { "" } else { "s" });
        g.font.draw(&t, x + 20.0 * 8.0 * s, y + 4.0 * s, s, col::TEXT_BAD);
    }
    y += 2.0 * lh;
    // ---- left: squad list
    let lw = (r.w * 0.34).floor();
    let list = Rect::new(x, y, lw - pad, r.y + r.h - pad - y - 34.0 * s);
    draw_line(r.x + lw, y - 4.0 * s, r.x + lw, r.y + r.h - pad, 1.0, col::BORDER);
    let row_h = 2.0 * lh + 4.0 * s;
    let rows = ((list.h / row_h) as usize).max(1);
    let step = wheel_over(&g.ui, &mut g.input, list);
    g.mil.scroll = (g.mil.scroll as i32 + step).clamp(0, n.saturating_sub(rows) as i32) as usize;
    if n == 0 {
        g.font.draw("No squads yet.", x, y, s, col::TEXT_DIM);
    }
    let mut pick = None;
    let squads: Vec<(i32, String, usize, usize, String)> = g
        .sim
        .world
        .squads
        .iter()
        .map(|q| {
            let filled = q.members.iter().filter(|m| **m >= 0).count();
            (q.id, squad_name(&g.sim.world, q.id), filled, q.members.len().max(10), orders_summary(&g.sim.world, q))
        })
        .collect();
    let maxc = (list.w / (8.0 * s)) as usize;
    for (id, name, filled, slots, ord) in squads.iter().skip(g.mil.scroll).take(rows) {
        let rr = Rect::new(list.x - 4.0 * s, y - 2.0 * s, list.w + 4.0 * s, row_h - 2.0 * s);
        if list_row(&mut g.ui, rr, g.mil.squad == Some(*id)) {
            pick = Some(*id);
        }
        g.font.draw(&clip(name, maxc.saturating_sub(7)), x, y, s, col::TEXT);
        let cnt = format!("{filled}/{slots}");
        g.font.draw(&cnt, list.x + list.w - g.font.width(&cnt, s), y, s, if *filled == 0 { col::TEXT_DIM } else { col::TEXT_GOOD });
        let oc = if ord == "No orders" { col::TEXT_DIM } else { col::TEXT_CYAN };
        g.font.draw(&clip(ord, maxc), x + 8.0 * s, y + lh, s, oc);
        y += row_h;
    }
    if let Some(id) = pick {
        g.mil.squad = Some(id);
        g.mil.picker = None;
        g.mil.kill_list = false;
    }
    let bh = 26.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(x, r.y + r.h - pad - bh, 18.0 * 8.0 * s, bh), "+ Create squad", BtnState::Normal) {
        let k = g.sim.world.squads.len() + 1;
        let id = g.sim.create_squad(&format!("Squad {k}"));
        g.mil.squad = Some(id);
        g.mil.picker = Some(0);
        g.set_status(format!("Created Squad {k}: pick its first member"));
    }
    // ---- right: the selected squad
    let rx = r.x + lw + pad;
    let rw = r.x + r.w - pad - rx;
    let Some(sid) = g.mil.squad.filter(|id| g.sim.world.squads.iter().any(|q| q.id == *id)) else {
        g.font.draw("Select a squad on the left, or create one.", rx, r.y + pad + 2.0 * lh, s, col::TEXT_DIM);
        return;
    };
    let squad = g.sim.world.squads.iter().find(|q| q.id == sid).cloned().unwrap();
    let mut y = r.y + pad + 2.0 * lh;
    g.font.draw(&squad_name(&g.sim.world, sid), rx, y, 1.5 * s, col::TEXT_HI);
    if !squad.alias.is_empty() && squad.alias != squad.name && !squad.name.is_empty() {
        let t = format!("({})", squad.name);
        g.font.draw(&clip(&t, 40), rx + g.font.width(&squad.alias, 1.5 * s) + 10.0 * s, y + 4.0 * s, s, col::TEXT_DIM);
    }
    y += 2.0 * lh;
    let sched = if squad.schedule.is_empty() { "-".to_string() } else { squad.schedule.clone() };
    g.font.draw(&format!("Schedule: {sched}"), rx, y, s, col::TEXT_DIM);
    y += lh + 2.0 * s;
    // Routine buttons (DF v50: Off duty / Staggered / Constant training / Ready).
    {
        let mut sx = rx;
        let sbw = 19.0 * 8.0 * s;
        let options = g.sim.squad_schedules();
        for name in options {
            let on = squad.schedule.eq_ignore_ascii_case(name);
            if text_button(&mut g.ui, &g.font, Rect::new(sx, y, sbw, bh), name, if on { BtnState::Active } else { BtnState::Normal }) {
                g.sim.set_squad_schedule(sid, name);
                g.set_status(format!("{}: {}", squad_name(&g.sim.world, sid), name.to_lowercase()));
            }
            sx += sbw + 6.0 * s;
        }
        y += bh + 6.0 * s;
    }
    let ord = orders_summary(&g.sim.world, &squad);
    g.font.draw("Orders:", rx, y, s, col::TEXT);
    g.font.draw(&clip(&ord, ((rw - 9.0 * 8.0 * s) / (8.0 * s)) as usize), rx + 9.0 * 8.0 * s, y, s, if squad.orders.is_empty() { col::TEXT_DIM } else { col::TEXT_CYAN });
    y += lh + 4.0 * s;
    // Order buttons.
    let bw = 15.0 * 8.0 * s;
    let mut bx = rx;
    let orders: [(&str, u8); 4] = [("Station", 0), ("Kill", 1), ("Train", 2), ("Cancel orders", 3)];
    for (label, k) in orders {
        let active = (k == 1 && g.mil.kill_list) || (k == 0 && matches!(g.tool, Tool::SquadStation(q) if q == sid));
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y, bw, bh), label, if active { BtnState::Active } else { BtnState::Normal }) {
            match k {
                0 => {
                    g.tool = Tool::SquadStation(sid);
                    g.screen = Screen::None;
                    g.set_status(format!("{}: click a tile to station the squad there", squad_name(&g.sim.world, sid)));
                    return;
                }
                1 => {
                    g.mil.kill_list = !g.mil.kill_list;
                    g.mil.picker = None;
                    g.mil.pscroll = 0;
                }
                2 => {
                    g.sim.order_squad(sid, SquadOrder::Train);
                    g.set_status(format!("{}: train", squad_name(&g.sim.world, sid)));
                }
                _ => {
                    g.sim.order_squad(sid, SquadOrder::Cancel);
                    g.set_status(format!("{}: orders cancelled", squad_name(&g.sim.world, sid)));
                }
            }
        }
        bx += bw + 6.0 * s;
    }
    if let Some(p) = station_pos(&squad) {
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y, 12.0 * 8.0 * s, bh), "Show", BtnState::Normal) {
            g.cam.focus(p);
            g.screen = Screen::None;
            return;
        }
    }
    y += bh + 10.0 * s;
    // [app9] Tabs: members, equipment (per-position uniforms), ammunition, patrol and barracks.
    let y = crate::ui::round9::squad_tabs(g, rx, y);
    let mut y = y;
    // Overlay lists (picker / kill targets) replace the member list while open.
    let area = Rect::new(rx, y, rw, r.y + r.h - pad - y);
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
    // Members.
    g.font.draw("Members", rx, y, s, col::BORDER_HI);
    y += lh;
    let slots = squad.members.len().max(10);
    let mut action: Option<(usize, bool)> = None;
    let mut jump = None;
    for i in 0..slots {
        let m = squad.members.get(i).copied().unwrap_or(-1);
        let rr = Rect::new(rx - 4.0 * s, y - 2.0 * s, rw + 4.0 * s, lh + 2.0 * s);
        if i % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.03));
        }
        g.font.draw(&format!("{:>2}.", i + 1), rx, y, s, col::TEXT_DIM);
        let tx = rx + 4.0 * 8.0 * s;
        let btn = Rect::new(rx + rw - 10.0 * 8.0 * s, y - 2.0 * s, 10.0 * 8.0 * s, lh);
        if m >= 0 {
            match g.sim.world.unit(m) {
                Some(u) => {
                    let dead = u.flags.dead;
                    let name = format!("{}{}", unit_name(u), if dead { " (dead)" } else { "" });
                    let nr = Rect::new(tx - 2.0 * s, y - 2.0 * s, 30.0 * 8.0 * s, lh);
                    let hov = g.ui.hovered(nr);
                    g.font.draw(&clip(&name, 30), tx, y, s, if dead { col::TEXT_BAD } else if hov { col::TEXT_HI } else { col::TEXT });
                    if g.ui.clicked(nr) && !dead {
                        jump = Some((u.id, u.pos));
                    }
                    let sk = best_combat_skill(u).map(|(t, _)| t).unwrap_or_else(|| unit_profession(u));
                    let maxc = ((btn.x - tx - 31.0 * 8.0 * s) / (8.0 * s)) as usize;
                    g.font.draw(&clip(&sk, maxc), tx + 31.0 * 8.0 * s, y, s, col::TEXT_DIM);
                }
                None => {
                    g.font.draw(&format!("unit #{m} (not on the map)"), tx, y, s, col::TEXT_DIM);
                }
            }
            if text_button(&mut g.ui, &g.font, btn, "Remove", BtnState::Normal) {
                action = Some((i, false));
            }
        } else {
            g.font.draw("(empty)", tx, y, s, col::TEXT_DIM);
            if text_button(&mut g.ui, &g.font, btn, "+ Assign", BtnState::Normal) {
                action = Some((i, true));
            }
        }
        y += lh + 2.0 * s;
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
        g.select_unit(id);
        g.unit_tab = 0;
        g.cam.focus(p);
        g.screen = Screen::None;
        return;
    }
    y += 8.0 * s;
    g.font.draw("Uniform", rx, y, s, col::BORDER_HI);
    y += lh;
    if squad.uniform.is_empty() {
        g.font.draw("No uniform: members fight with what they carry.", rx, y, s, col::TEXT_DIM);
    } else {
        let maxc = (rw / (8.0 * s)) as usize;
        for e in &squad.uniform {
            if y > r.y + r.h - pad - lh {
                break;
            }
            g.font.draw(&clip(&crate::ui::round9::entry_text(&g.sim.world, e), maxc), rx, y, s, col::TEXT);
            y += lh;
        }
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
    for (id, name, sk, _, sq) in c.iter().skip(g.mil.pscroll).take(rows) {
        let rr = Rect::new(x - 4.0 * s, y - 2.0 * s, cw + 4.0 * s, lh);
        if list_row(&mut g.ui, rr, false) {
            chosen = Some(*id);
        }
        g.font.draw(&clip(name, 28), x, y, s, if sq.is_empty() { col::TEXT } else { col::TEXT_DIM });
        g.font.draw(&clip(sk, 30), x + 29.0 * 8.0 * s, y, s, col::TEXT_GOOD);
        if !sq.is_empty() {
            let t = clip(sq, 18);
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

/// Kill order targets: the hostile creatures on the map, or "pick on the map".
fn draw_kill_list(g: &mut Game, sid: i32, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (x, mut y) = (area.x, area.y);
    g.font.draw("Kill: choose a target", x, y, s, col::BORDER_HI);
    let bw = 16.0 * 8.0 * s;
    let br = Rect::new(area.x + area.w - bw, y - 4.0 * s, bw, lh + 2.0 * s);
    if text_button(&mut g.ui, &g.font, br, "Pick on map", BtnState::Normal) {
        g.tool = Tool::SquadKill(sid);
        g.screen = Screen::None;
        g.mil.kill_list = false;
        g.set_status(format!("{}: click a creature to attack", squad_name(&g.sim.world, sid)));
        return;
    }
    y += lh + 6.0 * s;
    let w = &g.sim.world;
    let mut t: Vec<(i32, String, String, Pos, bool)> = g
        .hostiles
        .iter()
        .filter_map(|id| w.unit(*id))
        .filter(|u| !u.flags.dead)
        .map(|u| (u.id, unit_name(u), pretty(&u.race).to_lowercase(), u.pos, u.flags.caged))
        .collect();
    t.sort_by(|a, b| a.4.cmp(&b.4).then(a.1.cmp(&b.1)));
    if t.is_empty() {
        g.font.draw("No hostile creatures on the map. Use \"Pick on map\" for any other creature.", x, y, s, col::TEXT_DIM);
        return;
    }
    let mut chosen = None;
    for (id, name, race, p, caged) in t.iter().take((((area.y + area.h) - y) / lh) as usize) {
        let rr = Rect::new(x - 4.0 * s, y - 2.0 * s, area.w + 4.0 * s, lh);
        if list_row(&mut g.ui, rr, false) {
            chosen = Some(*id);
        }
        g.font.draw("!", x, y, s, col::TEXT_BAD);
        g.font.draw(&clip(name, 30), x + 16.0 * s, y, s, if *caged { col::TEXT_DIM } else { col::TEXT });
        g.font.draw(&clip(race, 20), x + 33.0 * 8.0 * s, y, s, col::TEXT_DIM);
        let loc = format!("{}{}, {}, z {}", if *caged { "caged  " } else { "" }, p.x, p.y, p.z);
        g.font.draw(&loc, x + area.w - g.font.width(&loc, s), y, s, col::TEXT_DIM);
        y += lh;
    }
    if let Some(id) = chosen {
        g.sim.order_squad(sid, SquadOrder::Kill(id));
        let who = g.sim.world.unit(id).map(unit_name).unwrap_or_default();
        g.set_status(format!("{}: kill {who}", squad_name(&g.sim.world, sid)));
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
}
