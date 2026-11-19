//! [nobles15] Requirements, mandates, demands and bookkeeper panel of the nobles screen.
//!
//! A "Requirements" button in the nobles screen's title bar opens it over the position list:
//! per noble the room requirements (red = missing, yellow = too low, green = met; w:Noble)
//! and furniture counts, then the active mandates with their deadline and punishment, the
//! demands (w:Demand colours) and the bookkeeper's precision level 1-5 (w:Bookkeeper).

use super::{clip, col, panel, text_button, BtnState};
use crate::font::GH;
use crate::game::Game;
use macroquad::prelude::*;
use rf_sim::nobles15_api::ReqState;
use std::cell::Cell;

thread_local! {
    static OPEN: Cell<bool> = const { Cell::new(false) };
}

const YELLOW: Color = Color::new(0.95, 0.85, 0.35, 1.0);
const ORANGE: Color = Color::new(0.98, 0.6, 0.25, 1.0);

fn state_col(s: ReqState) -> Color {
    match s {
        ReqState::Missing => col::TEXT_BAD,
        ReqState::Low => YELLOW,
        ReqState::Met => col::TEXT_GOOD,
        ReqState::None => col::TEXT_DIM,
    }
}

fn days(ticks: u64) -> String {
    let d = ticks / rf_core::TICKS_PER_DAY as u64;
    if d >= 28 {
        format!("{} months", d / 28)
    } else {
        format!("{d} days")
    }
}

/// Draw the toggle button (top right of the nobles screen `r`) and, when open, the panel.
pub fn draw(g: &mut Game, r: Rect) {
    let s = g.ui.scale;
    let open = OPEN.with(|o| o.get());
    let bw = 15.0 * 8.0 * s;
    let br = Rect::new(r.x + r.w - bw - 34.0 * s, r.y + 30.0 * s, bw, GH * s + 6.0 * s);
    if text_button(&mut g.ui, &g.font, br, "Requirements", if open { BtnState::Active } else { BtnState::Normal }) {
        OPEN.with(|o| o.set(!open));
    }
    if open {
        draw_panel(g, Rect::new(r.x + 12.0 * s, br.y + br.h + 8.0 * s, r.w - 24.0 * s, r.y + r.h - br.y - br.h - 20.0 * s));
    }
}

/// Open the panel (`--ui nobles15` screenshots).
pub fn open() {
    OPEN.with(|o| o.set(true));
}

/// Close the panel (the nobles screen closed).
pub fn close() {
    OPEN.with(|o| o.set(false));
}

fn draw_panel(g: &mut Game, r: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    panel(&mut g.ui, r);
    let pad = 10.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let maxc = ((r.w - 2.0 * pad) / (8.0 * s)) as usize;
    let bottom = r.y + r.h - pad;
    let reqs = g.sim.noble_requirements();
    g.font.draw("Room requirements", x, y, s, col::TEXT_HI);
    y += lh;
    if reqs.is_empty() {
        g.font.draw("No noble with requirements on the map.", x + 8.0 * s, y, s, col::TEXT_DIM);
        y += lh;
    }
    for n in &reqs {
        if y + 2.0 * lh > bottom {
            break;
        }
        let head = format!("{}, {}", n.name, n.position);
        g.font.draw(&clip(&head, maxc), x + 4.0 * s, y, s, state_col(n.state));
        y += lh;
        let mut parts: Vec<(String, Color)> = Vec::new();
        for rq in &n.rooms {
            let have = if rq.zone < 0 { "none".to_string() } else { rq.tier_name.to_string() };
            parts.push((format!("{}: {} (needs {})", rq.kind, have, rq.required_name), state_col(rq.state)));
        }
        for f in &n.furniture {
            let st = if f.have >= f.need { ReqState::Met } else { ReqState::Low };
            parts.push((format!("{} {}/{}", f.kind, f.have, f.need), state_col(st)));
        }
        let mut cx = x + 16.0 * s;
        for (t, c) in parts {
            let w = g.font.width(&t, s) + 14.0 * s;
            if cx + w > r.x + r.w - pad {
                cx = x + 16.0 * s;
                y += lh;
                if y + lh > bottom {
                    break;
                }
            }
            g.font.draw(&t, cx, y, s, c);
            cx += w;
        }
        y += lh + 2.0 * s;
    }
    // Mandates.
    y += 4.0 * s;
    if y + 2.0 * lh < bottom {
        g.font.draw("Mandates", x, y, s, col::TEXT_HI);
        y += lh;
        let ms = g.sim.mandates();
        if ms.is_empty() {
            g.font.draw("none", x + 8.0 * s, y, s, col::TEXT_DIM);
            y += lh;
        }
        for m in ms {
            if y + lh > bottom {
                break;
            }
            let who = g.sim.world.unit(m.noble).map(|u| u.name.clone()).unwrap_or_default();
            let t = if m.export_ban {
                format!("{who}: {} - ends in {}", m.text, days(m.ticks_left))
            } else {
                format!("{who}: {} ({} left) - due in {}; punishment {} hammerstrikes, {} months jail", m.text, m.amount_remaining.max(0), days(m.ticks_left), m.strikes, m.jail_months)
            };
            g.font.draw(&clip(&t, maxc), x + 8.0 * s, y, s, col::TEXT_CYAN);
            y += lh;
        }
    }
    // Demands.
    y += 4.0 * s;
    if y + 2.0 * lh < bottom {
        g.font.draw("Demands", x, y, s, col::TEXT_HI);
        y += lh;
        let ds = g.sim.noble_demands();
        if ds.is_empty() {
            g.font.draw("none", x + 8.0 * s, y, s, col::TEXT_DIM);
            y += lh;
        }
        for d in ds {
            if y + lh > bottom {
                break;
            }
            let who = g.sim.world.unit(d.noble).map(|u| u.name.clone()).unwrap_or_default();
            let c = match d.status {
                "met" => col::TEXT_GOOD,
                "pending" => YELLOW,
                "running out" => ORANGE,
                _ => col::TEXT_BAD,
            };
            g.font.draw(&clip(&format!("{who}: {} ({})", d.text, d.status), maxc), x + 8.0 * s, y, s, c);
            y += lh;
        }
    }
    // Bookkeeper.
    y += 4.0 * s;
    if y + lh < bottom {
        let (level, reached) = g.sim.bookkeeper_level();
        let t = if reached == 0 { format!("Bookkeeper precision: {level} (no records yet)") } else { format!("Bookkeeper precision: {level} (records at {reached})") };
        g.font.draw(&t, x, y, s, col::TEXT);
        let mut bx = x + g.font.width(&t, s) + 12.0 * s;
        for l in 1..=5u8 {
            let br = Rect::new(bx, y - 3.0 * s, 3.0 * 8.0 * s, GH * s + 6.0 * s);
            if text_button(&mut g.ui, &g.font, br, &l.to_string(), if l == level { BtnState::Active } else { BtnState::Normal }) {
                g.sim.set_bookkeeper_level(l);
            }
            bx += br.w + 4.0 * s;
        }
    }
}
