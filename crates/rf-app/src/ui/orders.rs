//! Work orders screen (DF hotkey `o`): the manager's order list with progress, frequency and
//! conditions, plus "+ New order" (template, amount, frequency).

use super::building::{building_title, job_label};
use super::{clip, col, list_row, panel, text_button, wheel_over, words, BtnState};
use crate::font::GH;
use crate::game::{Game, Screen, FREQUENCIES};
use macroquad::prelude::*;
use rf_core::{OrderCondition, WorkOrder};

/// Rectangle of the big centered screens (work orders, labor).
pub fn screen_rect(g: &Game, max_w: f32) -> Rect {
    let s = g.ui.scale;
    let (sw, sh) = (screen_width(), screen_height());
    let top = super::topbar::TOPBAR_H * s + 10.0 * s;
    let bottom = sh - super::toolbar::TOOLBAR_H * s - 10.0 * s;
    let w = (sw - 60.0 * s).min(max_w * s);
    Rect::new((sw - w) / 2.0, top, w, (bottom - top).max(200.0))
}

fn cmp_sym(c: &str) -> &'static str {
    match c {
        "AtLeast" => "≥",
        "AtMost" => "≤",
        "GreaterThan" => ">",
        "LessThan" => "<",
        "Exactly" => "=",
        "Not" => "≠",
        _ => "?",
    }
}

/// "drinks < 100" style summary of one condition.
pub fn condition_text(c: &OrderCondition) -> String {
    let what = if !c.item_subtype.is_empty() {
        super::pretty(&c.item_subtype).to_lowercase()
    } else if !c.item_type.is_empty() {
        super::pretty(&c.item_type).to_lowercase()
    } else {
        "items".to_string()
    };
    let flags = if c.flags.is_empty() { String::new() } else { format!(" ({})", c.flags.iter().map(|f| f.to_lowercase()).collect::<Vec<_>>().join(", ")) };
    // `≠` is not in CP437; spell it.
    let sym = cmp_sym(&c.compare);
    let sym = if sym == "≠" { "not" } else { sym };
    format!("{what}{flags} {sym} {}", c.value)
}

pub fn conditions_text(o: &WorkOrder) -> String {
    if o.conditions.is_empty() {
        return "-".into();
    }
    o.conditions.iter().map(condition_text).collect::<Vec<_>>().join("; ")
}

pub fn frequency_label(f: &str) -> &'static str {
    FREQUENCIES.iter().find(|(k, _)| *k == f).map(|(_, l)| *l).unwrap_or("One time")
}

pub fn draw_orders(g: &mut Game) {
    if g.screen != Screen::WorkOrders {
        return;
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let r = screen_rect(g, 1100.0);
    panel(&mut g.ui, r);
    let pad = 12.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        g.screen = Screen::None;
        return;
    }
    let n = g.sim.world.work_orders.len();
    g.font.draw(&format!("Work orders ({n})"), x, y, 1.5 * s, col::TEXT_HI);
    let nb = Rect::new(x + 30.0 * 8.0 * s, y - 2.0 * s, 16.0 * 8.0 * s, GH * s + 10.0 * s);
    let nst = if g.orders.new_open { BtnState::Active } else { BtnState::Normal };
    if text_button(&mut g.ui, &g.font, nb, "+ New order", nst) {
        g.orders.new_open = !g.orders.new_open;
    }
    let bind = g.binds.label("D_ORDERS");
    let hint = format!("Managers turn orders into workshop jobs. Esc{} closes.", if bind.is_empty() { String::new() } else { format!(" or {bind}") });
    g.font.draw(&hint, nb.x + nb.w + 16.0 * s, y + 2.0 * s, s, col::TEXT_DIM);
    y += 2.0 * lh + 4.0 * s;
    let picker_w = if g.orders.new_open { (r.w * 0.42).max(360.0 * s) } else { 0.0 };
    let list_w = r.w - 2.0 * pad - if picker_w > 0.0 { picker_w + pad } else { 0.0 };
    // Column layout of the list.
    let cols = [0.0, 0.40, 0.56, 0.68, 0.88];
    let cx = |i: usize| x + cols[i] * (list_w - 30.0 * s);
    for (i, h) in ["Job", "Progress", "Frequency", "Conditions", "Workshop"].iter().enumerate() {
        g.font.draw(h, cx(i), y, s, col::BORDER_HI);
    }
    y += lh;
    draw_line(x, y, x + list_w, y, 1.0, col::BORDER);
    y += 4.0 * s;
    let row_h = lh + 8.0 * s;
    let list_area = Rect::new(x, y, list_w, r.y + r.h - pad - y);
    let rows = ((list_area.h / row_h) as usize).max(1);
    let step = wheel_over(&g.ui, &mut g.input, list_area);
    g.orders.scroll = (g.orders.scroll as i32 + step * 2).clamp(0, n.saturating_sub(rows) as i32) as usize;
    if n == 0 {
        g.font.draw("No work orders. Click \"+ New order\" to add one (e.g. Brew drink, Prepare easy meal).", x, y + 4.0 * s, s, col::TEXT_DIM);
    }
    let templates = g.sim.order_templates();
    let mut remove = None;
    let mut toggle = None;
    for (k, o) in g.sim.world.work_orders.iter().enumerate().skip(g.orders.scroll).take(rows) {
        let rr = Rect::new(x, y, list_w, row_h - 2.0 * s);
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, if k % 2 == 0 { Color::new(1.0, 1.0, 1.0, 0.05) } else { Color::new(0.0, 0.0, 0.0, 0.0) });
        if g.ui.hovered(rr) {
            draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, col::BORDER);
        }
        let ty = y + 4.0 * s;
        let lc = if o.active { col::TEXT } else { col::TEXT_DIM };
        let label = job_label(&templates, &o.job, &o.reaction, &o.item_type, &o.mat_category);
        let w0 = ((cx(1) - cx(0)) / (8.0 * s)) as usize;
        g.font.draw(&clip(&label, w0.saturating_sub(1)), cx(0) + 4.0 * s, ty, s, lc);
        // Progress: done/total with a bar.
        let done = (o.amount_total - o.amount_left).max(0);
        let ptxt = format!("{}/{}", o.amount_left, o.amount_total);
        let pw = cx(2) - cx(1) - 10.0 * s;
        let frac = if o.amount_total > 0 { done as f32 / o.amount_total as f32 } else { 0.0 };
        draw_rectangle(cx(1), ty + 1.0, pw, GH * s - 2.0, Color::new(0.12, 0.12, 0.14, 1.0));
        draw_rectangle(cx(1), ty + 1.0, pw * frac.clamp(0.0, 1.0), GH * s - 2.0, Color::new(0.25, 0.55, 0.25, 1.0));
        g.font.draw(&ptxt, cx(1) + (pw - g.font.width(&ptxt, s)) / 2.0, ty, s, col::TEXT);
        g.font.draw(frequency_label(&o.frequency), cx(2), ty, s, lc);
        let w3 = ((cx(4) - cx(3)) / (8.0 * s)) as usize;
        let cond = conditions_text(o);
        g.font.draw(&clip(&cond, w3.saturating_sub(1)), cx(3), ty, s, if o.conditions.is_empty() { col::TEXT_DIM } else { col::TEXT_CYAN });
        if g.ui.hovered(Rect::new(cx(3), ty, cx(4) - cx(3), GH * s)) && cond.chars().count() >= w3 {
            g.ui.tooltip = Some((cond.clone(), vec2(cx(3), ty - 4.0)));
        }
        let ws = if o.workshop_id >= 0 {
            g.sim.world.building(o.workshop_id).map(|b| building_title(g, b)).unwrap_or_else(|| format!("#{}", o.workshop_id))
        } else {
            "any".into()
        };
        let w4 = ((x + list_w - 60.0 * s - cx(4)) / (8.0 * s)) as usize;
        g.font.draw(&clip(&ws, w4.max(3)), cx(4), ty, s, col::TEXT_DIM);
        let ab = Rect::new(x + list_w - 54.0 * s, y + 2.0 * s, 24.0 * s, row_h - 6.0 * s);
        if text_button(&mut g.ui, &g.font, ab, if o.active { "√" } else { "-" }, if o.active { BtnState::Active } else { BtnState::Normal }) {
            toggle = Some(o.id);
        }
        if g.ui.hovered(ab) {
            g.ui.tooltip = Some((if o.active { "Active (click to pause)" } else { "Paused (click to activate)" }.into(), vec2(ab.x, ab.y - 4.0)));
        }
        let xb = Rect::new(x + list_w - 26.0 * s, y + 2.0 * s, 24.0 * s, row_h - 6.0 * s);
        if text_button(&mut g.ui, &g.font, xb, "x", BtnState::Normal) {
            remove = Some(o.id);
        }
        if g.ui.hovered(xb) {
            g.ui.tooltip = Some(("Remove order".into(), vec2(xb.x, xb.y - 4.0)));
        }
        y += row_h;
    }
    if n > rows {
        let t = format!("{}-{} of {n} (wheel scrolls)", g.orders.scroll + 1, (g.orders.scroll + rows).min(n));
        g.font.draw(&t, x, r.y + r.h - pad - GH * s + 4.0 * s, s, col::TEXT_DIM);
    }
    if let Some(id) = remove {
        g.sim.remove_work_order(id);
    }
    if let Some(id) = toggle {
        // No sim API for pausing an order: flip the flag in the world.
        if let Some(o) = g.sim.world.work_orders.iter_mut().find(|o| o.id == id) {
            o.active = !o.active;
        }
    }
    if g.orders.new_open {
        let pr = Rect::new(r.x + r.w - pad - picker_w, r.y + pad + 2.0 * lh, picker_w, r.h - 2.0 * pad - 2.0 * lh);
        new_order_picker(g, pr, &templates);
    }
}

fn new_order_picker(g: &mut Game, r: Rect, templates: &[rf_sim::OrderTemplate]) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.09, 0.08, 0.07, 1.0));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, col::BORDER_HI);
    let pad = 8.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let w = r.w - 2.0 * pad;
    let maxc = (w / (8.0 * s)) as usize;
    g.font.draw("New work order", x, y, s, col::TEXT_HI);
    y += lh + 4.0 * s;
    g.orders.template = g.orders.template.min(templates.len().saturating_sub(1));
    // Bottom controls take a fixed height.
    let ctrl_h = 5.0 * lh + 3.0 * (GH * s + 10.0 * s);
    let list = Rect::new(x, y, w, (r.y + r.h - pad - ctrl_h - y).max(lh * 3.0));
    draw_rectangle(list.x, list.y, list.w, list.h, Color::new(0.0, 0.0, 0.0, 0.3));
    let rows = ((list.h / lh) as usize).max(1);
    let step = wheel_over(&g.ui, &mut g.input, list);
    g.orders.tscroll = (g.orders.tscroll as i32 + step * 3).clamp(0, templates.len().saturating_sub(rows) as i32) as usize;
    let mut ly = list.y;
    for (i, t) in templates.iter().enumerate().skip(g.orders.tscroll).take(rows) {
        let rr = Rect::new(list.x, ly, list.w, lh);
        if list_row(&mut g.ui, rr, i == g.orders.template) {
            g.orders.template = i;
        }
        let ws = words(t.workshop);
        let wsw = ws.chars().count().min(16);
        g.font.draw(&clip(t.label, maxc.saturating_sub(wsw + 2)), list.x + 4.0 * s, ly + 1.5 * s, s, col::TEXT);
        g.font.draw(&clip(&ws, 16), list.x + list.w - (wsw as f32 + 1.0) * 8.0 * s, ly + 1.5 * s, s, col::TEXT_DIM);
        ly += lh;
    }
    if templates.is_empty() {
        g.font.draw("No order templates.", list.x + 4.0 * s, list.y + 2.0 * s, s, col::TEXT_DIM);
        return;
    }
    y = list.y + list.h + 8.0 * s;
    let t = &templates[g.orders.template];
    g.font.draw(&clip(&format!("{} ({})", t.label, words(t.workshop)), maxc), x, y, s, col::TEXT_HI);
    y += lh + 4.0 * s;
    // Amount: - [n] +   (Shift = 10, wheel over the number).
    g.font.draw("Amount", x, y + 5.0 * s, s, col::TEXT);
    let bh = GH * s + 10.0 * s;
    let bx = x + 10.0 * 8.0 * s;
    let step_n = if g.input.mods & crate::keys::SHIFT != 0 { 10 } else { 1 };
    if text_button(&mut g.ui, &g.font, Rect::new(bx, y, 28.0 * s, bh), "-", BtnState::Normal) {
        g.orders.amount -= step_n;
    }
    let nr = Rect::new(bx + 32.0 * s, y, 8.0 * 8.0 * s, bh);
    draw_rectangle(nr.x, nr.y, nr.w, nr.h, Color::new(0.0, 0.0, 0.0, 0.5));
    draw_rectangle_lines(nr.x, nr.y, nr.w, nr.h, 1.0, if g.ui.hovered(nr) { col::BORDER_HI } else { col::BORDER });
    let wheel = wheel_over(&g.ui, &mut g.input, nr);
    g.orders.amount -= wheel * step_n;
    if text_button(&mut g.ui, &g.font, Rect::new(nr.x + nr.w + 4.0 * s, y, 28.0 * s, bh), "+", BtnState::Normal) {
        g.orders.amount += step_n;
    }
    g.orders.amount = g.orders.amount.clamp(1, 999);
    let at = g.orders.amount.to_string();
    g.font.draw(&at, nr.x + (nr.w - g.font.width(&at, s)) / 2.0, nr.y + 5.0 * s, s, col::TEXT_HI);
    g.font.draw("Shift = 10, wheel", nr.x + nr.w + 40.0 * s, y + 5.0 * s, s, col::TEXT_DIM);
    y += bh + 8.0 * s;
    // Frequency.
    g.font.draw("Repeat", x, y + 5.0 * s, s, col::TEXT);
    let mut fx = x + 10.0 * 8.0 * s;
    let mut fy = y;
    for (i, (_, l)) in FREQUENCIES.iter().enumerate() {
        let fw = (l.len() as f32 + 2.0) * 8.0 * s;
        if fx + fw > r.x + r.w - pad {
            fx = x + 10.0 * 8.0 * s;
            fy += bh + 4.0 * s;
        }
        let st = if g.orders.freq == i { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(fx, fy, fw, bh), l, st) {
            g.orders.freq = i;
        }
        fx += fw + 4.0 * s;
    }
    y = fy + bh + 10.0 * s;
    let cw = 18.0 * 8.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(x, y, cw, bh), "Create order", BtnState::Normal) {
        let amount = g.orders.amount;
        let freq = FREQUENCIES[g.orders.freq.min(FREQUENCIES.len() - 1)].0;
        let id = g.sim.add_work_order(g.orders.template, amount, freq);
        g.set_status(format!("Work order #{id}: {} x{amount}, {}", t.label, frequency_label(freq)));
    }
    if text_button(&mut g.ui, &g.font, Rect::new(x + cw + 8.0 * s, y, 12.0 * 8.0 * s, bh), "Close", BtnState::Normal) {
        g.orders.new_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_summaries() {
        let c = OrderCondition { compare: "LessThan".into(), value: 100, item_type: "DRINK".into(), ..Default::default() };
        assert_eq!(condition_text(&c), "drink < 100");
        let c = OrderCondition { compare: "AtLeast".into(), value: 5, item_type: "PLANT".into(), flags: vec!["BREWABLE".into()], ..Default::default() };
        assert_eq!(condition_text(&c), "plant (brewable) ≥ 5");
        assert_eq!(frequency_label("Monthly"), "Monthly");
        assert_eq!(frequency_label("OneTime"), "One time");
    }
}
