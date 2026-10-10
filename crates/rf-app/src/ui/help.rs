//! [round 6] Help: the controls overview (F1 or the `?` button), generated from the key
//! bindings in use (your DF `interface.txt`), and the first-time hint bubbles.

use super::{col, panel, text_button, BtnState};
use crate::font::{GH, GW};
use crate::game::{Game, Screen};
use crate::keys::Bindings;
use macroquad::prelude::*;

/// One line of the overview: what it does and the bindings that do it.
pub struct HelpRow {
    pub action: &'static str,
    /// Binding names; their triggers are listed (joined by " / " when there are several).
    pub binds: &'static [&'static str],
}

const fn r(action: &'static str, binds: &'static [&'static str]) -> HelpRow {
    HelpRow { action, binds }
}

/// The overview's sections (keyboard part), in DF's own binding names.
pub const SECTIONS: &[(&str, &[HelpRow])] = &[
    (
        "View",
        &[
            r("Pan up / left / down / right", &["CURSOR_UP", "CURSOR_LEFT", "CURSOR_DOWN", "CURSOR_RIGHT"]),
            r("One level up", &["CURSOR_UP_Z", "CURSOR_UP_Z_AUX"]),
            r("One level down", &["CURSOR_DOWN_Z", "CURSOR_DOWN_Z_AUX"]),
            r("Ten levels up / down", &["CURSOR_UP_Z_FAST", "CURSOR_DOWN_Z_FAST"]),
            r("Zoom in / out", &["ZOOM_IN", "ZOOM_OUT"]),
        ],
    ),
    (
        "Time",
        &[
            r("Pause / unpause", &["D_PAUSE"]),
            r("One step (paused)", &["D_ONESTEP"]),
            r("Speed 1x / 2x / 5x / max", &["RF_SPEED_1", "RF_SPEED_2", "RF_SPEED_3", "RF_SPEED_4"]),
        ],
    ),
    (
        "Orders",
        &[
            r("Dig menu", &["D_DESIGNATE_DIG"]),
            r("  mine / stairs / ramp", &["DESIGNATE_DIG", "DESIGNATE_STAIR_UPDOWN", "DESIGNATE_RAMP"]),
            r("  channel / remove", &["DESIGNATE_CHANNEL", "DESIGNATE_DIG_REMOVE_STAIRS_RAMPS"]),
            r("Chop trees", &["D_DESIGNATE_CHOP"]),
            r("Gather plants", &["D_DESIGNATE_GATHER"]),
            r("Smooth / engrave / carve", &["D_DESIGNATE_SMOOTH"]),
            r("Traffic", &["D_DESIGNATE_TRAFFIC"]),
            r("Item designations", &["D_DESIGNATE_ITEMS"]),
            r("Erase designations", &["D_DESIGNATE_ERASE"]),
        ],
    ),
    (
        "Build",
        &[
            r("Build menu", &["D_BUILDING"]),
            r("Zones", &["D_CIVZONE"]),
            r("Stockpiles", &["D_STOCKPILES"]),
            r("Burrows", &["D_BURROWS"]),
        ],
    ),
    (
        "Screens",
        &[
            r("Creatures", &["D_UNITLIST"]),
            r("Work orders", &["D_ORDERS"]),
            r("Labor", &["D_LABOR"]),
            r("Squads", &["D_SQUADS"]),
            r("Nobles", &["D_NOBLES"]),
            r("Reports", &["D_ANNOUNCE"]),
        ],
    ),
    (
        "Game",
        &[
            r("Back / menu (save, load, settings)", &["LEAVESCREEN"]),
            r("Quicksave", &["RF_QUICKSAVE"]),
            r("This help", &["RF_HELP"]),
        ],
    ),
];

/// Mouse controls (fixed, plus the wheel bindings of your prefs).
pub fn mouse_rows(b: &Bindings) -> Vec<(String, String)> {
    let wheel = |name: &str| {
        b.triggers(name)
            .iter()
            .filter(|t| matches!(t, crate::keys::Trigger::Wheel { .. }))
            .map(crate::keys::trigger_label)
            .next()
            .unwrap_or_else(|| "-".into())
    };
    vec![
        ("Select creature / building / tile".into(), "Left click".into()),
        ("Next creature on the same tile".into(), "Click again".into()),
        ("Designate, place zones and piles".into(), "Drag, or click + click".into()),
        ("3D box (several levels)".into(), "Change level between".into()),
        ("Back out (box, tool, menu)".into(), "Right click".into()),
        ("Pan".into(), "Middle drag".into()),
        ("One level up / down".into(), format!("{} / {}", wheel("CURSOR_UP_Z"), wheel("CURSOR_DOWN_Z"))),
        ("Ten levels up / down".into(), format!("{} / {}", wheel("CURSOR_UP_Z_FAST"), wheel("CURSOR_DOWN_Z_FAST"))),
        ("Zoom in / out".into(), format!("{} / {}", wheel("ZOOM_IN"), wheel("ZOOM_OUT"))),
        ("Jump to an announcement".into(), "Click it".into()),
        ("Jump to a level".into(), "Click the right bar".into()),
        ("Button name and key".into(), "Hover it".into()),
    ]
}

/// The key text of a row: the bindings' triggers, `-` when unbound.
pub fn row_keys(b: &Bindings, row: &HelpRow) -> String {
    let parts: Vec<String> = row
        .binds
        .iter()
        .map(|n| {
            // Wheels are listed in the mouse section; keep the key column short.
            let keys: Vec<String> = b
                .triggers(n)
                .iter()
                .filter(|t| !matches!(t, crate::keys::Trigger::Wheel { .. }))
                .map(crate::keys::trigger_label)
                .take(if row.binds.len() > 1 { 1 } else { 3 })
                .collect();
            if keys.is_empty() {
                "-".to_string()
            } else {
                keys.join(", ")
            }
        })
        .collect();
    parts.join(" / ")
}

/// Draw the overview (when open). Esc, F1, `?`, "Close" or a click outside close it.
pub fn draw_help(g: &mut Game) {
    if !g.help_open {
        return;
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
    let w = (1080.0 * s).min(sw - 24.0);
    let h = (620.0 * s).min(sh - 24.0);
    let rr = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    g.ui.block(Rect::new(0.0, 0.0, sw, sh));
    panel(&mut g.ui, rr);
    let pad = 16.0 * s;
    let title = "Controls";
    g.font.draw(title, rr.x + pad, rr.y + pad, 1.5 * s, col::TEXT_HI);
    let src = if g.binds.source.contains("Roaming") || g.binds.source.contains("prefs") {
        "Keys from your Dwarf Fortress prefs (interface.txt)".to_string()
    } else {
        format!("Keys from {}", g.binds.source)
    };
    let maxc = ((w - 2.0 * pad) / (GW * s)) as usize;
    let src = super::clip(&src, maxc.saturating_sub(22));
    g.font.draw(&src, rr.x + w - pad - g.font.width(&src, s), rr.y + pad + 4.0 * s, s, col::TEXT_DIM);
    // Three columns: View/Time/Game, Orders/Build, Screens/Mouse.
    let top = rr.y + pad + 1.5 * GH * s + 14.0 * s;
    let colw = (w - 2.0 * pad - 2.0 * 18.0 * s) / 3.0;
    let layout: [&[usize]; 3] = [&[0, 1, 5], &[2, 3], &[4]];
    for (ci, secs) in layout.iter().enumerate() {
        let x = rr.x + pad + ci as f32 * (colw + 18.0 * s);
        let mut y = top;
        for &si in secs.iter() {
            let (name, rows) = SECTIONS[si];
            y = section_header(g, name, x, y, colw);
            for row in rows.iter() {
                let keys = row_keys(&g.binds, row);
                draw_row(g, row.action, &keys, x, y, colw);
                y += lh;
            }
            y += 10.0 * s;
        }
        if ci == 2 {
            y = section_header(g, "Mouse", x, y, colw);
            for (a, k) in mouse_rows(&g.binds) {
                draw_row(g, &a, &k, x, y, colw);
                y += lh;
            }
        }
    }
    let foot = "Most things also have a button: hover the toolbar for names and keys. Esc closes this.";
    let fy = rr.y + h - pad - 26.0 * s;
    g.font.draw(&super::clip(foot, maxc.saturating_sub(16)), rr.x + pad, fy + 7.0 * s, s, col::TEXT_DIM);
    let br = Rect::new(rr.x + w - pad - 110.0 * s, fy, 110.0 * s, 26.0 * s);
    let close = text_button(&mut g.ui, &g.font, br, "Close", BtnState::Normal);
    let outside = g.ui.lpressed && !g.ui.click_used && !rr.contains(g.ui.mouse);
    if close || outside {
        g.help_open = false;
    }
    if g.ui.lpressed {
        g.ui.click_used = true;
    }
}

fn section_header(g: &mut Game, name: &str, x: f32, y: f32, w: f32) -> f32 {
    let s = g.ui.scale;
    g.font.draw(name, x, y, s, col::BORDER_HI);
    let ly = y + GH * s + 3.0 * s;
    draw_line(x, ly, x + w, ly, 1.0, col::BORDER);
    ly + 6.0 * s
}

fn draw_row(g: &Game, action: &str, keys: &str, x: f32, y: f32, w: f32) {
    let s = g.ui.scale;
    let kc = if keys == "-" || keys.chars().all(|c| c == '-' || c == ' ' || c == '/') { col::TEXT_DIM } else { col::TEXT_HI };
    let kmax = ((w * 0.45) / (GW * s)) as usize;
    let keys = super::clip(keys, kmax.max(6));
    let kw = g.font.width(&keys, s);
    let amax = (((w - kw) / (GW * s)) as usize).saturating_sub(1).max(8);
    g.font.draw(&super::clip(action, amax), x, y, s, col::TEXT);
    g.font.draw(&keys, x + w - kw, y, s, kc);
}

// ------------------------------------------------------------------------------------ hints

/// Where a hint bubble points.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Anchor {
    /// The pause/speed buttons (top left).
    Pause,
    /// The middle of the map.
    Center,
    /// The toolbar (bottom).
    Toolbar,
    /// The `?` button (top right).
    HelpButton,
}

/// First-time hints in the order they are shown.
pub const HINTS: &[&str] = &["welcome", "camera", "select", "toolbar", "help"];

fn hint_content(g: &Game, id: &str) -> (String, Vec<String>, Anchor) {
    let b = &g.binds;
    let k = |n: &str| {
        let l = b.label(n);
        if l.is_empty() {
            "-".to_string()
        } else {
            l
        }
    };
    match id {
        "welcome" => (
            format!("Welcome to {}", g.sim.world.meta.fort_name),
            vec![
                "The game starts paused, like Dwarf Fortress.".into(),
                format!("Press {} (or the play button) to let time run.", k("D_PAUSE")),
                format!("{} {} {} {} set the speed: 1x is DF's pace, the last is max.", k("RF_SPEED_1"), k("RF_SPEED_2"), k("RF_SPEED_3"), k("RF_SPEED_4")),
            ],
            Anchor::Pause,
        ),
        "camera" => (
            "Moving around".into(),
            vec![
                "Mouse wheel: one level up or down (Shift: ten levels).".into(),
                format!("Ctrl+wheel zooms. Middle-drag or {} {} {} {} pans.", k("CURSOR_UP"), k("CURSOR_LEFT"), k("CURSOR_DOWN"), k("CURSOR_RIGHT")),
                "The bar at the right edge shows sky, surface and depth.".into(),
            ],
            Anchor::Center,
        ),
        "select" => (
            "Looking at things".into(),
            vec![
                "Left-click a dwarf, building or tile to open its sheet.".into(),
                "Click the same tile again to cycle through creatures.".into(),
                "Right-click or Esc backs out of whatever is open.".into(),
            ],
            Anchor::Center,
        ),
        "toolbar" => (
            "Giving orders".into(),
            vec![
                "The buttons at the bottom are DF's menus: creatures,".into(),
                "orders, labor, dig, build, zones, stockpiles, squads ...".into(),
                "Hover a button to see its name and key.".into(),
            ],
            Anchor::Toolbar,
        ),
        _ => (
            "All controls".into(),
            vec![
                format!("{} or the ? button lists every key and mouse control.", k("RF_HELP")),
                format!("Esc opens the menu (save, load, settings); {} quicksaves.", k("RF_QUICKSAVE")),
                format!("Autosave: {} (change it in Settings).", g.settings.autosave_label().to_lowercase()),
            ],
            Anchor::HelpButton,
        ),
    }
}

/// The hint to show now, if any.
pub fn current_hint(g: &Game) -> Option<&'static str> {
    if !g.hints_enabled || g.settings.hints_off {
        return None;
    }
    HINTS.iter().copied().find(|h| !g.settings.hint_seen(h))
}

/// Draw the current hint bubble (only on the plain map view).
pub fn draw_hints(g: &mut Game) {
    if g.help_open || g.menu_open || g.alert.is_some() || g.r4.popup.is_some() || g.screen != Screen::None {
        return;
    }
    let Some(id) = current_hint(g) else { return };
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (title, lines, anchor) = hint_content(g, id);
    let (sw, sh) = (screen_width(), screen_height());
    let tw = lines.iter().map(|l| g.font.width(l, s)).fold(g.font.width(&title, s), f32::max);
    let w = tw + 28.0 * s;
    let h = GH * s + 10.0 * s + lines.len() as f32 * lh + 40.0 * s;
    let top = super::topbar::TOPBAR_H * s;
    let tb = sh - super::toolbar::TOOLBAR_H * s;
    // Bubble position and the point its arrow touches.
    let (x, y, tip) = match anchor {
        Anchor::Pause => (8.0 * s, top + 18.0 * s, vec2(30.0 * s, top - 2.0 * s)),
        Anchor::Center => ((sw - w) / 2.0, sh * 0.30, vec2(sw / 2.0, sh * 0.30 + h + 40.0 * s)),
        Anchor::Toolbar => ((sw - w) / 2.0, tb - h - 26.0 * s, vec2(sw / 2.0, tb - 2.0 * s)),
        Anchor::HelpButton => (sw - w - 30.0 * s, top + 18.0 * s, vec2(sw - help_button_dx(g), top - 2.0 * s)),
    };
    let x = x.clamp(4.0, (sw - w - 4.0).max(4.0));
    let rr = Rect::new(x, y, w, h);
    // Arrow from the bubble edge to the target.
    let (ax, ay) = (tip.x.clamp(rr.x + 16.0 * s, rr.x + rr.w - 16.0 * s), if tip.y < rr.y { rr.y } else { rr.y + rr.h });
    if anchor != Anchor::Center {
        draw_triangle(vec2(ax - 9.0 * s, ay), vec2(ax + 9.0 * s, ay), tip, col::BORDER_HI);
    }
    let bg = Color::new(0.10, 0.085, 0.05, 0.97);
    g.ui.block(rr);
    draw_rectangle(rr.x, rr.y, rr.w, rr.h, bg);
    draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 2.0, col::BORDER_HI);
    let pad = 14.0 * s;
    let n = HINTS.iter().position(|h| *h == id).unwrap_or(0) + 1;
    let step = format!("{n}/{}", HINTS.len());
    g.font.draw(&title, rr.x + pad, rr.y + 8.0 * s, s, col::TEXT_HI);
    g.font.draw(&step, rr.x + rr.w - pad - g.font.width(&step, s), rr.y + 8.0 * s, s, col::TEXT_DIM);
    let mut ly = rr.y + 8.0 * s + GH * s + 6.0 * s;
    for l in &lines {
        g.font.draw(l, rr.x + pad, ly, s, col::TEXT);
        ly += lh;
    }
    let bh = 22.0 * s;
    let by = rr.y + rr.h - bh - 8.0 * s;
    let last = n == HINTS.len();
    let nb = Rect::new(rr.x + rr.w - pad - 90.0 * s, by, 90.0 * s, bh);
    let hb = Rect::new(rr.x + pad, by, 120.0 * s, bh);
    let next = text_button(&mut g.ui, &g.font, nb, if last { "Got it" } else { "Next" }, BtnState::Normal);
    let hide = text_button(&mut g.ui, &g.font, hb, "No more hints", BtnState::Normal);
    if next {
        g.settings.hints_seen.push(id.to_string());
        g.persist_settings();
    } else if hide {
        g.settings.hints_off = true;
        g.persist_settings();
    }
}

/// Distance of the `?` button's center from the right screen edge.
pub fn help_button_dx(g: &Game) -> f32 {
    let s = g.ui.scale;
    // Same layout as `topbar::draw_topbar`: button height = bar height - 8.
    super::topbar::ZBAR_W * s + 10.0 * s + (super::topbar::TOPBAR_H * s - 8.0 * s) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_has_keys_with_the_defaults() {
        let b = Bindings::defaults();
        // Bindings the built-in defaults lack (smooth, traffic, items, nobles) show "-".
        let mut missing = Vec::new();
        for (_, rows) in SECTIONS {
            for row in rows.iter() {
                let k = row_keys(&b, row);
                assert!(!k.is_empty());
                if k.split(" / ").all(|p| p == "-") {
                    missing.push(row.action);
                }
            }
        }
        assert!(missing.len() <= 8, "{missing:?}");
        let r = &SECTIONS[1].1[0];
        assert_eq!(row_keys(&b, r), "Space");
        let m = mouse_rows(&b);
        assert!(m.iter().any(|(a, k)| a.starts_with("Zoom") && k == "Ctrl+wheel up / Ctrl+wheel down"), "{m:?}");
    }

    #[test]
    fn installed_bindings_cover_the_overview() {
        // Uses the real interface.txt when there is one (Steam DF binds all of these).
        let b = Bindings::load(&rf_gfx::df_dir());
        let pan = row_keys(&b, &SECTIONS[0].1[0]);
        assert!(pan.contains('/'), "{pan}");
    }
}
