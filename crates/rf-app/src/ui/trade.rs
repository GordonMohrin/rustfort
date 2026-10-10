//! [round 4] Trade screen (DF Steam style): the merchants' goods on the left, the fort's goods
//! in the depot on the right, each with its value. Select items on both sides, "Offer" asks
//! the merchants (`Sim::trade(offer, request, false)`), "Trade" makes the deal
//! (`commit = true`). "Move goods to depot" picks fort items to bring (`Sim::move_to_depot`).

use super::{clip, col, list_row, panel, text_button, wheel_over, BtnState};
use crate::font::GH;
use crate::game::{Game, Screen};
use macroquad::prelude::*;
use rf_core::{Item, World};
use std::collections::HashMap;

/// Container id -> (value of the contents, number of items inside).
pub type Contents = HashMap<i32, (i32, usize)>;

pub fn contents_map(w: &World) -> Contents {
    let mut m: Contents = HashMap::new();
    for it in w.items.iter().filter(|it| it.container >= 0) {
        let e = m.entry(it.container).or_insert((0, 0));
        e.0 += it.value.max(0);
        e.1 += 1;
    }
    m
}

/// DF writes values with the ☼ (CP437 15) sign.
pub fn money(v: i32) -> String {
    format!("{v}☼")
}

fn item_text(w: &World, it: &Item) -> String {
    let mut d = if !it.name.is_empty() {
        it.name.clone()
    } else if !it.desc.is_empty() {
        it.desc.clone()
    } else {
        super::pretty(&it.kind).to_lowercase()
    };
    if it.stack > 1 && !d.starts_with(char::is_numeric) {
        d = format!("{} x{}", d, it.stack);
    }
    let q = match it.quality {
        1 => "-",
        2 => "+",
        3 => "*",
        4 => "≡",
        5 => "☼",
        _ => "",
    };
    if !q.is_empty() {
        d = format!("{q}{d}{q}");
    }
    let _ = w;
    d
}

/// Value of an item plus everything inside it (bins, barrels), like DF's trade list.
pub fn total_value(c: &Contents, it: &Item) -> i32 {
    it.value.max(0) + c.get(&it.id).map(|e| e.0).unwrap_or(0)
}

/// The trade depot: from the caravan, else the first built depot of the fort.
pub fn depot_id(g: &Game) -> Option<i32> {
    if let Some(t) = &g.r4.trade {
        if t.depot >= 0 {
            return Some(t.depot);
        }
    }
    g.sim.world.buildings.iter().find(|b| b.kind == "TradeDepot" && b.built).map(|b| b.id)
}

/// Fort items in the depot when no caravan is there (its rectangle).
fn items_in_depot(g: &Game) -> Vec<i32> {
    let Some(d) = depot_id(g).and_then(|id| g.sim.world.building(id)) else { return Vec::new() };
    g.sim.world.items.iter().filter(|it| d.contains(it.pos) && it.container < 0 && !it.flags.in_inventory && !it.flags.construction && it.building != d.id).map(|it| it.id).collect()
}

/// Fort items the "Move goods to depot" picker offers: loose or stockpiled goods (top-level
/// items; a bin comes with its contents), not forbidden, owned, rotten or built in.
pub fn depot_candidates(g: &Game, c: &Contents) -> Vec<i32> {
    let w = &g.sim.world;
    let in_depot = depot_id(g).and_then(|id| w.building(id)).cloned();
    let merchant: Vec<i32> = g.r4.trade.as_ref().map(|t| t.goods.clone()).unwrap_or_default();
    let filter = g.r4.depot_filter.to_lowercase();
    let mut v: Vec<(i32, i32)> = w
        .items
        .iter()
        .filter(|it| {
            let f = &it.flags;
            it.container < 0
                && f.on_ground
                && !f.forbid
                && !f.construction
                && !f.in_building
                && !f.in_inventory
                && !f.owned
                && !f.rotten
                && !f.dump
                && !f.artifact
                && !f.hidden
                && it.value > 0
                && !matches!(it.kind.as_str(), "CORPSE" | "CORPSEPIECE" | "REMAINS" | "BOULDER" | "ROCK" | "SEEDS")
                && in_depot.as_ref().map(|d| !d.contains(it.pos)).unwrap_or(true)
                && !merchant.contains(&it.id)
        })
        .filter(|it| filter.is_empty() || item_text(w, it).to_lowercase().contains(&filter))
        .map(|it| (it.id, total_value(c, it)))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().map(|(id, _)| id).collect()
}

fn refresh_eval(g: &mut Game) {
    let key = (g.r4.offer.clone(), g.r4.request.clone());
    if g.r4.eval.is_some() && g.r4.eval_key == key {
        return;
    }
    if g.r4.trade.is_none() || (key.0.is_empty() && key.1.is_empty()) {
        g.r4.eval = None;
        g.r4.eval_key = key;
        return;
    }
    let ev = g.sim.trade(&key.0, &key.1, false);
    g.r4.eval = Some(ev);
    g.r4.eval_key = key;
}

enum TAct {
    None,
    Close,
    ToggleRequest(i32),
    ToggleOffer(i32),
    Offer,
    Trade,
    OpenDepot,
}

pub fn draw_trade(g: &mut Game) {
    if g.screen != Screen::Trade {
        return;
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let r = super::orders::screen_rect(g, 1180.0);
    panel(&mut g.ui, r);
    let pad = 12.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let mut act = TAct::None;
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        act = TAct::Close;
    }
    let st = g.r4.trade.clone();
    let fort = g.sim.world.meta.fort_name.clone();
    match &st {
        Some(t) => {
            let race = if t.race.is_empty() { String::new() } else { format!(" ({})", super::pretty(&t.race).to_lowercase()) };
            g.font.draw(&format!("Trade with {}{race}", if t.civ.is_empty() { "the caravan" } else { t.civ.as_str() }), x, y, 1.5 * s, col::TEXT_HI);
            let leave = format!("Leaves in {}", super::round4::ticks_span(t.leaves_in));
            g.font.draw(&leave, r.x + r.w - pad - 30.0 * s - g.font.width(&leave, s), y + 4.0 * s, s, col::TEXT_DIM);
            y += 1.5 * GH * s + 8.0 * s;
            let (bt, bc) = if t.broker_present { ("Your broker is at the depot.", col::TEXT_GOOD) } else { ("The broker is not at the depot yet (DF: trading needs the broker there).", col::TEXT_BAD) };
            g.font.draw(bt, x, y, s, bc);
        }
        None => {
            g.font.draw("Trade depot", x, y, 1.5 * s, col::TEXT_HI);
            y += 1.5 * GH * s + 8.0 * s;
            g.font.draw("No caravan at the depot. You can bring goods now, so they are ready when the merchants arrive.", x, y, s, col::TEXT_DIM);
        }
    }
    y += lh + 8.0 * s;
    // Two columns.
    let gap = 16.0 * s;
    let cw = (r.w - 2.0 * pad - gap) / 2.0;
    let bottom_area = 5.0 * lh + 30.0 * s;
    let list_h = (r.y + r.h - pad - bottom_area - y).max(4.0 * lh);
    let goods: Vec<i32> = st.as_ref().map(|t| t.goods.clone()).unwrap_or_default();
    let fort_goods: Vec<i32> = match &st {
        Some(t) => t.fort_goods.clone(),
        None => items_in_depot(g),
    };
    let cols: [(&str, &Vec<i32>, bool); 2] = [("Merchant goods", &goods, true), ("goods in the depot", &fort_goods, false)];
    let cm = contents_map(&g.sim.world);
    for (ci, (title, list, merchant)) in cols.iter().enumerate() {
        let cx = x + ci as f32 * (cw + gap);
        let head = if *merchant { format!("{title} ({})", list.len()) } else { format!("{fort} {title} ({})", list.len()) };
        g.font.draw(&clip(&head, (cw / (8.0 * s)) as usize), cx, y, s, col::BORDER_HI);
        let area = Rect::new(cx, y + lh + 2.0 * s, cw, list_h - lh - 2.0 * s);
        draw_rectangle(area.x, area.y, area.w, area.h, Color::new(0.0, 0.0, 0.0, 0.35));
        draw_rectangle_lines(area.x, area.y, area.w, area.h, 1.0, col::BORDER);
        let rows = ((area.h / lh) as usize).max(1);
        let step = wheel_over(&g.ui, &mut g.input, area);
        g.r4.scroll[ci] = (g.r4.scroll[ci] as i32 + step * 3).clamp(0, list.len().saturating_sub(rows) as i32) as usize;
        if list.is_empty() {
            let t = if *merchant { if st.is_some() { "The merchants have nothing left." } else { "-" } } else { "Nothing in the depot. Use \"Move goods to depot\"." };
            g.font.draw(t, area.x + 6.0 * s, area.y + 4.0 * s, s, col::TEXT_DIM);
        }
        let maxc = ((cw - 16.0 * s) / (8.0 * s)) as usize;
        let mut yy = area.y + 2.0 * s;
        for id in list.iter().skip(g.r4.scroll[ci]).take(rows) {
            let Some(it) = g.sim.world.items.iter().find(|it| it.id == *id) else { continue };
            let sel = if *merchant { g.r4.request.contains(id) } else { g.r4.offer.contains(id) };
            let rr = Rect::new(area.x + 2.0 * s, yy, area.w - 4.0 * s, lh);
            if list_row(&mut g.ui, rr, sel) {
                act = if *merchant { TAct::ToggleRequest(*id) } else { TAct::ToggleOffer(*id) };
            }
            let v = money(total_value(&cm, it));
            let vw = g.font.width(&v, s);
            let mark = if sel { "√ " } else { "  " };
            let t = format!("{mark}{}", item_text(&g.sim.world, it));
            g.font.draw(&clip(&t, maxc.saturating_sub(v.chars().count() + 1)), rr.x + 4.0 * s, yy + 1.5 * s, s, if sel { col::TEXT_HI } else { col::TEXT });
            g.font.draw(&v, rr.x + rr.w - 4.0 * s - vw, yy + 1.5 * s, s, col::TEXT_DIM);
            yy += lh;
        }
        // Column footer: selected value.
        let (n, val) = {
            let sel = if *merchant { &g.r4.request } else { &g.r4.offer };
            let w = &g.sim.world;
            (sel.len(), sel.iter().filter_map(|id| w.items.iter().find(|it| it.id == *id)).map(|it| total_value(&cm, it)).sum::<i32>())
        };
        let ft = format!("Selected: {n} item{}, {}", if n == 1 { "" } else { "s" }, money(val));
        g.font.draw(&ft, cx, area.y + area.h + 4.0 * s, s, if n > 0 { col::TEXT_HI } else { col::TEXT_DIM });
    }
    y += list_h + lh + 8.0 * s;
    // Evaluation and the merchants' reply.
    refresh_eval(g);
    if let Some(ev) = &g.r4.eval {
        let t = format!("The merchants value your offer at {} and their goods at {}.", money(ev.offer_value), money(ev.request_value));
        g.font.draw(&t, x, y, s, col::TEXT);
    } else if st.is_some() {
        g.font.draw("Select goods on both sides, then Offer to hear the merchants, or Trade.", x, y, s, col::TEXT_DIM);
    }
    y += lh + 2.0 * s;
    if let Some((reply, ok)) = &g.r4.reply {
        let c = if *ok { col::TEXT_GOOD } else { col::TEXT_BAD };
        g.font.draw(&clip(&format!("Merchant: \"{reply}\""), ((r.w - 2.0 * pad) / (8.0 * s)) as usize), x, y, s, c);
    }
    // Buttons.
    let bh = 26.0 * s;
    let by = r.y + r.h - pad - bh;
    let bw = 14.0 * 8.0 * s;
    let can = st.is_some() && !(g.r4.offer.is_empty() && g.r4.request.is_empty());
    let cst = if can { BtnState::Normal } else { BtnState::Disabled };
    if text_button(&mut g.ui, &g.font, Rect::new(x, by, bw, bh), "Offer", cst) {
        act = TAct::Offer;
    }
    if text_button(&mut g.ui, &g.font, Rect::new(x + bw + 8.0 * s, by, bw, bh), "Trade", cst) {
        act = TAct::Trade;
    }
    let dw = 22.0 * 8.0 * s;
    let dst = if g.r4.depot_open { BtnState::Active } else { BtnState::Normal };
    if text_button(&mut g.ui, &g.font, Rect::new(x + 2.0 * (bw + 8.0 * s) + 16.0 * s, by, dw, bh), "Move goods to depot", dst) {
        act = TAct::OpenDepot;
    }
    if text_button(&mut g.ui, &g.font, Rect::new(r.x + r.w - pad - bw, by, bw, bh), "Close", BtnState::Normal) {
        act = TAct::Close;
    }
    match act {
        TAct::None => {}
        TAct::Close => {
            g.screen = Screen::None;
            g.r4.depot_open = false;
        }
        TAct::ToggleRequest(id) => toggle(&mut g.r4.request, id),
        TAct::ToggleOffer(id) => toggle(&mut g.r4.offer, id),
        TAct::Offer => {
            let ev = g.sim.trade(&g.r4.offer.clone(), &g.r4.request.clone(), false);
            g.r4.reply = Some((ev.reply.clone(), ev.accepted));
            g.r4.eval = Some(ev);
        }
        TAct::Trade => {
            let (o, q) = (g.r4.offer.clone(), g.r4.request.clone());
            let ev = g.sim.trade(&o, &q, true);
            g.r4.reply = Some((ev.reply.clone(), ev.accepted));
            if ev.accepted {
                g.set_status(format!("Trade done: {} for {}", money(ev.offer_value), money(ev.request_value)));
                g.r4.offer.clear();
                g.r4.request.clear();
                g.r4.eval = None;
                super::round4::poll_trade(g, true);
            } else {
                g.r4.eval = Some(ev);
            }
        }
        TAct::OpenDepot => {
            g.r4.depot_open = !g.r4.depot_open;
            g.r4.depot_scroll = 0;
        }
    }
    if g.r4.depot_open {
        draw_depot_picker(g, r);
    }
}

fn toggle(v: &mut Vec<i32>, id: i32) {
    if let Some(i) = v.iter().position(|x| *x == id) {
        v.remove(i);
    } else {
        v.push(id);
    }
}

/// "Move goods to depot": item list with a name filter (on top of the trade screen).
fn draw_depot_picker(g: &mut Game, under: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let w = (under.w * 0.62).max(480.0 * s).min(under.w - 20.0 * s);
    let h = under.h - 60.0 * s;
    let r = Rect::new(under.x + (under.w - w) / 2.0, under.y + 30.0 * s, w, h);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.03, 0.03, 0.03, 1.0));
    panel(&mut g.ui, r);
    let pad = 12.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    g.font.draw("Move goods to depot", x, y, 1.5 * s, col::TEXT_HI);
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        g.r4.depot_open = false;
        g.r4.depot_typing = false;
        return;
    }
    y += 1.5 * GH * s + 10.0 * s;
    // Filter field.
    g.font.draw("Search:", x, y + 1.0 * s, s, col::TEXT);
    let fr = Rect::new(x + 8.0 * 8.0 * s, y - 3.0 * s, 28.0 * 8.0 * s, GH * s + 8.0 * s);
    draw_rectangle(fr.x, fr.y, fr.w, fr.h, Color::new(0.0, 0.0, 0.0, 0.55));
    let focus = g.r4.depot_typing;
    draw_rectangle_lines(fr.x, fr.y, fr.w, fr.h, 1.0, if focus { col::ACTIVE_BORDER } else if g.ui.hovered(fr) { col::BORDER_HI } else { col::BORDER });
    if g.ui.clicked(fr) {
        g.r4.depot_typing = true;
    } else if g.ui.lpressed && !g.ui.click_used && !fr.contains(g.ui.mouse) {
        g.r4.depot_typing = false;
    }
    if g.r4.depot_typing {
        for c in g.input.chars.clone() {
            if !c.is_control() && g.r4.depot_filter.chars().count() < 28 {
                g.r4.depot_filter.push(c);
                g.r4.depot_scroll = 0;
            }
        }
        if g.input.fired.contains(&KeyCode::Backspace) {
            g.r4.depot_filter.pop();
            g.r4.depot_scroll = 0;
        }
        if g.input.fired.contains(&KeyCode::Enter) {
            g.r4.depot_typing = false;
        }
    }
    let shown = if g.r4.depot_filter.is_empty() && !focus { "e.g. mug, steel, crafts".to_string() } else { g.r4.depot_filter.clone() };
    let caret = if focus && (get_time() * 2.0) as i64 % 2 == 0 { "_" } else { "" };
    g.font.draw(&format!("{shown}{caret}"), fr.x + 4.0 * s, fr.y + 4.0 * s, s, if g.r4.depot_filter.is_empty() { col::TEXT_DIM } else { col::TEXT_HI });
    y += lh + 10.0 * s;
    let cm = contents_map(&g.sim.world);
    let now = get_time();
    if g.r4.depot_cache_key.0 != g.r4.depot_filter || now - g.r4.depot_cache_key.1 > 1.0 || g.r4.depot_cache.is_empty() {
        g.r4.depot_cache = depot_candidates(g, &cm);
        g.r4.depot_cache_key = (g.r4.depot_filter.clone(), now);
    }
    let list = g.r4.depot_cache.clone();
    let bh = 26.0 * s;
    let area = Rect::new(x, y, r.w - 2.0 * pad, r.y + r.h - pad - bh - 10.0 * s - y);
    draw_rectangle(area.x, area.y, area.w, area.h, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_rectangle_lines(area.x, area.y, area.w, area.h, 1.0, col::BORDER);
    let rows = ((area.h / lh) as usize).max(1);
    let step = wheel_over(&g.ui, &mut g.input, area);
    g.r4.depot_scroll = (g.r4.depot_scroll as i32 + step * 3).clamp(0, list.len().saturating_sub(rows) as i32) as usize;
    let maxc = ((area.w - 16.0 * s) / (8.0 * s)) as usize;
    let mut yy = area.y + 2.0 * s;
    let mut toggle_id = None;
    if list.is_empty() {
        g.font.draw("No tradeable goods found.", area.x + 6.0 * s, yy + 2.0 * s, s, col::TEXT_DIM);
    }
    for id in list.iter().skip(g.r4.depot_scroll).take(rows) {
        let w = &g.sim.world;
        let Some(it) = w.items.iter().find(|it| it.id == *id) else { continue };
        let sel = g.r4.depot_sel.contains(id);
        let sent = g.r4.depot_sent.contains(id);
        let rr = Rect::new(area.x + 2.0 * s, yy, area.w - 4.0 * s, lh);
        if list_row(&mut g.ui, rr, sel) {
            toggle_id = Some(*id);
        }
        let n_in = cm.get(&it.id).map(|e| e.1).unwrap_or(0);
        let mut t = item_text(w, it);
        if n_in > 0 {
            t.push_str(&format!(" ({n_in} items inside)"));
        }
        if sent {
            t.push_str("  - on the way");
        }
        let v = money(total_value(&cm, it));
        let vw = g.font.width(&v, s);
        let mark = if sel { "√ " } else { "  " };
        let c = if sent { col::TEXT_CYAN } else if sel { col::TEXT_HI } else { col::TEXT };
        g.font.draw(&clip(&format!("{mark}{t}"), maxc.saturating_sub(v.chars().count() + 1)), rr.x + 4.0 * s, yy + 1.5 * s, s, c);
        g.font.draw(&v, rr.x + rr.w - 4.0 * s - vw, yy + 1.5 * s, s, col::TEXT_DIM);
        yy += lh;
    }
    if let Some(id) = toggle_id {
        toggle(&mut g.r4.depot_sel, id);
    }
    let by = r.y + r.h - pad - bh;
    let n = g.r4.depot_sel.len();
    let val: i32 = g.r4.depot_sel.iter().filter_map(|id| g.sim.world.items.iter().find(|it| it.id == *id)).map(|it| total_value(&cm, it)).sum();
    let label = format!("Bring {n} to the depot ({})", money(val));
    let bw = (label.chars().count() as f32 + 3.0) * 8.0 * s;
    let st = if n > 0 { BtnState::Normal } else { BtnState::Disabled };
    if text_button(&mut g.ui, &g.font, Rect::new(x, by, bw, bh), &label, st) {
        let ids = std::mem::take(&mut g.r4.depot_sel);
        g.sim.move_to_depot(&ids);
        g.set_status(format!("{} item{} marked for the depot: haulers bring them", ids.len(), if ids.len() == 1 { "" } else { "s" }));
        g.r4.depot_sent.extend(ids);
    }
    let t = format!("{} goods (sorted by value)", list.len());
    g.font.draw(&t, r.x + r.w - pad - g.font.width(&t, s), by + 6.0 * s, s, col::TEXT_DIM);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_include_contents() {
        let mut w = World::synthetic(8, 8, 4);
        w.items.clear();
        w.items.push(Item { id: 1, kind: "BIN".into(), value: 10, container: -1, ..Default::default() });
        w.items.push(Item { id: 2, kind: "CRAFTS".into(), value: 25, container: 1, ..Default::default() });
        w.items.push(Item { id: 3, kind: "CRAFTS".into(), value: 5, container: -1, ..Default::default() });
        let c = contents_map(&w);
        assert_eq!(total_value(&c, &w.items[0]), 35);
        assert_eq!(total_value(&c, &w.items[2]), 5);
        assert_eq!(c.get(&1).map(|e| e.1), Some(1));
        assert_eq!(money(120), "120☼");
    }
}
