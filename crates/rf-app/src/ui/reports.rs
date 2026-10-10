//! Reports screen (DF `Shift+n`): the whole announcement history with filters, newest first,
//! plus the combat log (`Sim::combat_log`). Clicking a line with a position jumps there.

use super::orders::screen_rect;
use super::{clip, col, list_row, panel, text_button, wheel_over, BtnState};
use crate::game::{Game, LogLine, Screen};
use macroquad::prelude::*;
use rf_sim::AnnouncementKind;

/// Filter tabs. The last two show the combat reports grouped per fight ([app9]) and the raw
/// `Sim::combat_log()` instead of announcements.
pub const FILTERS: &[&str] = &["All", "Combat", "Deaths", "Jobs", "Info", "Combat reports", "Combat log"];
/// [app9] Index of the per-fight combat reports tab.
pub const FIGHTS_TAB: usize = 5;

const COMBAT_WORDS: &[&str] = &[
    "attack", "strike", "struck", "punch", "kick", "bite", "slash", "stab", "hack", "bash", "kill", "slain",
    "fight", "hostile", "siege", "ambush", "vile force", "invader", "has come", "wound", "bleed", "charge",
    "dodge", "block", "parr", "sever", "squad", "goblin", "beast",
];

/// Category of an announcement: 1 combat, 2 death, 3 job, 4 info (index into `FILTERS`).
/// Works with kinds the sim may add later (matched by name).
pub fn category(kind: AnnouncementKind, text: &str) -> usize {
    let k = format!("{kind:?}");
    match k.as_str() {
        "Death" => 2,
        "Combat" | "Siege" | "Hostile" | "Military" => 1,
        "Job" => 3,
        _ => {
            let t = text.to_lowercase();
            if COMBAT_WORDS.iter().any(|w| t.contains(w)) {
                1
            } else {
                4
            }
        }
    }
}

/// Color of a line of category `cat`.
pub fn cat_color(kind: AnnouncementKind, cat: usize) -> Color {
    match cat {
        1 => Color::from_rgba(255, 110, 90, 255),
        2 => col::TEXT_BAD,
        3 => col::TEXT_CYAN,
        _ => {
            if format!("{kind:?}") == "Warning" {
                col::TEXT_HI
            } else {
                col::TEXT
            }
        }
    }
}

fn matches(f: usize, l: &LogLine) -> bool {
    f == 0 || l.cat as usize == f
}

pub fn draw_reports(g: &mut Game) {
    if g.screen != Screen::Reports {
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
    g.font.draw("Reports", x, y, 1.5 * s, col::TEXT_HI);
    y += 2.0 * lh;
    // Filter tabs with counts.
    let combat = if g.reports.filter == FILTERS.len() - 1 { g.sim.combat_log() } else { Vec::new() };
    let mut bx = x;
    let bh = 24.0 * s;
    for (i, f) in FILTERS.iter().enumerate() {
        let n = if i == FILTERS.len() - 1 {
            g.r9.fights.iter().map(|f| f.lines.len()).sum::<usize>().max(combat.len())
        } else if i == FIGHTS_TAB {
            g.r9.fights.len() // [app9]
        } else {
            g.log.iter().filter(|l| matches(i, l)).count()
        };
        let label = format!("{f} ({n})");
        let bw = (label.chars().count() as f32 + 3.0) * 8.0 * s;
        let st = if g.reports.filter == i { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y, bw, bh), &label, st) {
            g.reports.filter = i;
            g.reports.scroll = 0;
        }
        bx += bw + 6.0 * s;
    }
    y += bh + 10.0 * s;
    let area = Rect::new(x, y, r.w - 2.0 * pad, r.y + r.h - pad - y - lh);
    let rows = ((area.h / lh) as usize).max(1);
    let maxc = (area.w / (8.0 * s)) as usize;
    if g.reports.filter == FIGHTS_TAB {
        // [app9] Combat reports grouped per fight, like DF's Reports screen.
        let fa = Rect::new(area.x, area.y, area.w, area.h + lh);
        crate::ui::round9::draw_fights(g, fa);
        return;
    }
    let step = wheel_over(&g.ui, &mut g.input, area);
    if g.reports.filter == FILTERS.len() - 1 {
        // Combat log: plain lines, newest first.
        let n = combat.len();
        g.reports.scroll = (g.reports.scroll as i32 + step * 3).clamp(0, n.saturating_sub(rows) as i32) as usize;
        if n == 0 {
            g.font.draw("No combat yet.", x, y, s, col::TEXT_DIM);
        }
        for l in combat.iter().rev().skip(g.reports.scroll).take(rows) {
            g.font.draw(&clip(l, maxc), x, y, s, Color::from_rgba(255, 150, 120, 255));
            y += lh;
        }
        footer(g, r, pad, n, rows);
        return;
    }
    let idx: Vec<usize> = (0..g.log.len()).rev().filter(|&i| matches(g.reports.filter, &g.log[i])).collect();
    let n = idx.len();
    g.reports.scroll = (g.reports.scroll as i32 + step * 3).clamp(0, n.saturating_sub(rows) as i32) as usize;
    if n == 0 {
        g.font.draw("Nothing to report.", x, y, s, col::TEXT_DIM);
    }
    let date_w = 22.0 * 8.0 * s;
    let mut jump = None;
    for &i in idx.iter().skip(g.reports.scroll).take(rows) {
        let l = &g.log[i];
        let rr = Rect::new(x - 4.0 * s, y - 2.0 * s, area.w + 8.0 * s, lh);
        if l.pos.is_some() {
            if list_row(&mut g.ui, rr, false) {
                jump = l.pos;
            }
            if g.ui.hovered(rr) {
                g.ui.tooltip = Some(("Click to go there".into(), vec2(g.ui.mouse.x + 12.0, rr.y - 2.0)));
            }
        }
        g.font.draw(&clip(&l.date, 21), x, y, s, col::TEXT_DIM);
        let mark = if l.pos.is_some() { "»" } else { " " };
        g.font.draw(mark, x + date_w - 12.0 * s, y, s, col::BORDER_HI);
        let tc = cat_color(l.kind, l.cat as usize);
        g.font.draw(&clip(&l.text, maxc.saturating_sub(23)), x + date_w, y, s, tc);
        y += lh;
    }
    footer(g, r, pad, n, rows);
    if let Some(p) = jump {
        g.cam.focus(p);
        g.screen = Screen::None;
    }
}

fn footer(g: &mut Game, r: Rect, pad: f32, n: usize, rows: usize) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    if n > rows {
        let t = format!("{}-{} of {} (wheel scrolls, newest first)", g.reports.scroll + 1, (g.reports.scroll + rows).min(n), n);
        g.font.draw(&t, r.x + pad, r.y + r.h - pad - lh + 6.0 * s, s, col::TEXT_DIM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(category(AnnouncementKind::Death, "Urist has died of thirst."), 2);
        assert_eq!(category(AnnouncementKind::Job, "Urist cancels Dig: no path."), 3);
        assert_eq!(category(AnnouncementKind::Warning, "A vile force of darkness has arrived!"), 1);
        assert_eq!(category(AnnouncementKind::Info, "Saved the game."), 4);
    }
}
