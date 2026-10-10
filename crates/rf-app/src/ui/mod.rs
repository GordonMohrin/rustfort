//! Immediate-mode UI in the style of the Steam version: dark panels with ochre borders,
//! 32x36 icon buttons, the DF bitmap font on an 8x12 grid.
//!
//! The UI is drawn after the map. Every panel registers its rectangle, so the map input that
//! runs afterwards knows whether the mouse is over the UI (`UiCtx::over_ui`).

pub mod alerts;
pub mod build;
pub mod building;
pub mod burrows;
pub mod devices; // [round 4]
pub mod help; // [round 6]
pub mod labor;
pub mod military;
pub mod nobles; // [round 4]
pub mod orders;
pub mod overlay;
pub mod panels;
pub mod psyche;
pub mod reports;
pub mod round4; // [round 4]
pub mod round5; // [round 5]
pub mod toolbar;
pub mod topbar;
pub mod trade; // [round 4]

use crate::font::{Font, GH, GW};
use crate::render::MapRenderer;
use macroquad::prelude::*;
use rf_gfx::Gfx;

pub mod col {
    use macroquad::prelude::Color;
    pub const PANEL_BG: Color = Color::new(0.055, 0.050, 0.047, 0.94);
    pub const BORDER: Color = Color::new(0.47, 0.36, 0.18, 1.0);
    pub const BORDER_HI: Color = Color::new(0.84, 0.66, 0.27, 1.0);
    pub const BUTTON: Color = Color::new(0.16, 0.16, 0.18, 1.0);
    pub const BUTTON_HOVER: Color = Color::new(0.25, 0.25, 0.28, 1.0);
    pub const BUTTON_ACTIVE: Color = Color::new(0.22, 0.42, 0.20, 1.0);
    pub const ACTIVE_BORDER: Color = Color::new(0.45, 0.85, 0.35, 1.0);
    pub const TEXT: Color = Color::new(0.86, 0.86, 0.86, 1.0);
    pub const TEXT_DIM: Color = Color::new(0.52, 0.52, 0.55, 1.0);
    pub const TEXT_HI: Color = Color::new(1.0, 0.87, 0.35, 1.0);
    pub const TEXT_GOOD: Color = Color::new(0.55, 0.9, 0.45, 1.0);
    pub const TEXT_BAD: Color = Color::new(0.95, 0.40, 0.35, 1.0);
    pub const TEXT_CYAN: Color = Color::new(0.45, 0.85, 0.95, 1.0);
}

/// Per-frame mouse state for the UI.
#[derive(Default)]
pub struct UiCtx {
    pub mouse: Vec2,
    pub lpressed: bool,
    pub rpressed: bool,
    pub ldown: bool,
    /// The mouse is over a UI panel: map input is suppressed.
    pub over_ui: bool,
    /// A UI element consumed this frame's left click.
    pub click_used: bool,
    pub scale: f32,
    /// Tooltip text requested by a hovered button (drawn last).
    pub tooltip: Option<(String, Vec2)>,
}

impl UiCtx {
    pub fn begin(&mut self, input: &crate::keys::InputFrame, scale: f32) {
        self.mouse = input.mouse;
        self.lpressed = input.lpressed;
        self.rpressed = input.rpressed;
        self.ldown = input.ldown;
        self.over_ui = false;
        self.click_used = false;
        self.scale = scale;
        self.tooltip = None;
    }
    pub fn hovered(&self, r: Rect) -> bool {
        r.contains(self.mouse)
    }
    /// Left click inside `r` that nothing else consumed yet.
    pub fn clicked(&mut self, r: Rect) -> bool {
        if self.lpressed && !self.click_used && r.contains(self.mouse) {
            self.click_used = true;
            true
        } else {
            false
        }
    }
    /// Register a UI area that blocks the map.
    pub fn block(&mut self, r: Rect) {
        if r.contains(self.mouse) {
            self.over_ui = true;
        }
    }
    pub fn line_h(&self) -> f32 {
        (GH + 3.0) * self.scale
    }
}

/// Draw a Steam-style panel and register it as UI area.
pub fn panel(ui: &mut UiCtx, r: Rect) {
    ui.block(r);
    draw_rectangle(r.x, r.y, r.w, r.h, col::PANEL_BG);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, col::BORDER);
    draw_rectangle_lines(r.x + 3.0, r.y + 3.0, r.w - 6.0, r.h - 6.0, 1.0, Color::new(0.30, 0.23, 0.12, 0.9));
    // Ornamental corners like the Steam frames.
    let c = 5.0;
    for (x, y) in [(r.x, r.y), (r.x + r.w - c, r.y), (r.x, r.y + r.h - c), (r.x + r.w - c, r.y + r.h - c)] {
        draw_rectangle(x, y, c, c, col::BORDER_HI);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnState {
    Normal,
    Active,
    Disabled,
}

/// A text button. Returns true when clicked.
pub fn text_button(ui: &mut UiCtx, font: &Font, r: Rect, label: &str, state: BtnState) -> bool {
    let hover = ui.hovered(r) && state != BtnState::Disabled;
    let bg = match state {
        BtnState::Active => col::BUTTON_ACTIVE,
        _ if hover => col::BUTTON_HOVER,
        _ => col::BUTTON,
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    let border = match state {
        BtnState::Active => col::ACTIVE_BORDER,
        _ if hover => col::BORDER_HI,
        _ => Color::new(0.35, 0.35, 0.38, 1.0),
    };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, border);
    let s = ui.scale;
    let tw = font.width(label, s);
    let tc = if state == BtnState::Disabled { col::TEXT_DIM } else { col::TEXT };
    font.draw(label, r.x + (r.w - tw) / 2.0, r.y + (r.h - GH * s) / 2.0, s, tc);
    state != BtnState::Disabled && ui.clicked(r)
}

/// Fallback icon shapes (used when rf-gfx has no interface sprite for a button).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ico {
    Pick,
    Axe,
    Plant,
    Smooth,
    Eraser,
    Hammer,
    Crate,
    Zone,
    Smiley,
    Glyph(u8, [u8; 3]),
}

/// An icon button (32x36 like the Steam toolbar at 100 %). `icon` is the DF interface token
/// (e.g. `BUTTON_DIG`); `_ACTIVE`/`_INACTIVE` variants are tried first.
#[allow(clippy::too_many_arguments)]
pub fn icon_button(
    ui: &mut UiCtx,
    font: &Font,
    gfx: &Gfx,
    rend: &mut MapRenderer,
    r: Rect,
    icon: &str,
    fallback: Ico,
    state: BtnState,
    tip: &str,
) -> bool {
    let hover = ui.hovered(r);
    let suffix = if state == BtnState::Active { "_ACTIVE" } else { "_INACTIVE" };
    let sprite = if icon.is_empty() { None } else { gfx.ui_icon(&format!("{icon}{suffix}")).or_else(|| gfx.ui_icon(icon)) };
    let mut drawn = false;
    if let Some(sp) = sprite {
        drawn = rend.draw_ui_sprite(gfx, &sp, r);
    }
    if !drawn {
        let bg = match state {
            BtnState::Active => col::BUTTON_ACTIVE,
            BtnState::Normal if hover => col::BUTTON_HOVER,
            _ => col::BUTTON,
        };
        draw_rectangle(r.x, r.y, r.w, r.h, bg);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.38, 0.38, 0.42, 1.0));
        draw_fallback_icon(font, fallback, r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0);
    }
    if state == BtnState::Active {
        draw_rectangle_lines(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, 2.0, col::ACTIVE_BORDER);
    } else if hover && state != BtnState::Disabled {
        draw_rectangle_lines(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, 2.0, col::BORDER_HI);
    }
    if state == BtnState::Disabled {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
    }
    if hover {
        ui.tooltip = Some((tip.to_string(), vec2(r.x, r.y - 4.0)));
    }
    state != BtnState::Disabled && ui.clicked(r)
}

fn c3(c: [u8; 3]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], 255)
}

pub fn draw_fallback_icon(font: &Font, ico: Ico, x: f32, y: f32, w: f32, h: f32) {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let u = w.min(h) / 10.0;
    let wood = Color::from_rgba(150, 100, 50, 255);
    let steel = Color::from_rgba(185, 190, 200, 255);
    match ico {
        Ico::Pick => {
            draw_line(cx - 3.0 * u, cy + 4.0 * u, cx + 2.5 * u, cy - 2.5 * u, 1.6 * u, wood);
            // Curved head: two segments.
            draw_line(cx - 2.5 * u, cy - 3.5 * u, cx + 2.0 * u, cy - 2.0 * u, 1.3 * u, steel);
            draw_line(cx + 2.0 * u, cy - 2.0 * u, cx + 3.8 * u, cy + 2.2 * u, 1.3 * u, steel);
        }
        Ico::Axe => {
            draw_line(cx - 3.0 * u, cy + 4.0 * u, cx + 2.0 * u, cy - 3.5 * u, 1.5 * u, wood);
            draw_triangle(
                vec2(cx + 0.5 * u, cy - 2.5 * u),
                vec2(cx + 4.0 * u, cy - 3.8 * u),
                vec2(cx + 3.0 * u, cy + 0.5 * u),
                steel,
            );
        }
        Ico::Plant => {
            let g = Color::from_rgba(80, 170, 60, 255);
            draw_line(cx, cy + 4.0 * u, cx, cy - 1.0 * u, 1.0 * u, Color::from_rgba(60, 120, 40, 255));
            draw_circle(cx - 1.8 * u, cy - 1.0 * u, 1.8 * u, g);
            draw_circle(cx + 1.8 * u, cy - 1.5 * u, 1.8 * u, g);
            draw_circle(cx, cy - 3.0 * u, 1.8 * u, g);
            draw_circle(cx + 1.0 * u, cy - 1.2 * u, 0.8 * u, Color::from_rgba(200, 50, 60, 255));
        }
        Ico::Smooth => {
            for k in 0..3 {
                let yy = cy - 2.5 * u + k as f32 * 2.5 * u;
                draw_line(cx - 3.5 * u, yy, cx + 3.5 * u, yy, 0.8 * u, Color::from_rgba(200, 200, 210, 255));
            }
        }
        Ico::Eraser => {
            draw_triangle(vec2(cx - 4.0 * u, cy + 1.0 * u), vec2(cx - 1.0 * u, cy - 2.0 * u), vec2(cx + 1.0 * u, cy + 3.5 * u), Color::from_rgba(240, 150, 170, 255));
            draw_triangle(vec2(cx - 1.0 * u, cy - 2.0 * u), vec2(cx + 1.0 * u, cy + 3.5 * u), vec2(cx + 4.0 * u, cy + 0.5 * u), Color::from_rgba(230, 230, 235, 255));
            draw_line(cx - 1.0 * u, cy - 2.0 * u, cx + 1.0 * u, cy + 3.5 * u, 0.5 * u, Color::from_rgba(90, 90, 100, 255));
        }
        Ico::Hammer => {
            draw_line(cx - 3.0 * u, cy + 4.0 * u, cx + 1.5 * u, cy - 1.5 * u, 1.5 * u, wood);
            draw_rectangle(cx - 0.5 * u, cy - 4.0 * u, 4.5 * u, 2.6 * u, steel);
        }
        Ico::Crate => {
            let c = Color::from_rgba(160, 115, 60, 255);
            draw_rectangle(cx - 3.5 * u, cy - 3.0 * u, 7.0 * u, 6.0 * u, c);
            draw_rectangle_lines(cx - 3.5 * u, cy - 3.0 * u, 7.0 * u, 6.0 * u, 0.8 * u, Color::from_rgba(90, 60, 30, 255));
            draw_line(cx - 3.5 * u, cy - 3.0 * u, cx + 3.5 * u, cy + 3.0 * u, 0.7 * u, Color::from_rgba(90, 60, 30, 255));
            draw_line(cx + 3.5 * u, cy - 3.0 * u, cx - 3.5 * u, cy + 3.0 * u, 0.7 * u, Color::from_rgba(90, 60, 30, 255));
        }
        Ico::Zone => {
            let c = Color::from_rgba(120, 200, 90, 255);
            for k in 0..4 {
                let t = k as f32 * 2.0 * u;
                draw_line(cx - 3.5 * u + t, cy - 3.0 * u, cx - 2.5 * u + t, cy - 3.0 * u, 0.7 * u, c);
                draw_line(cx - 3.5 * u + t, cy + 3.0 * u, cx - 2.5 * u + t, cy + 3.0 * u, 0.7 * u, c);
            }
            draw_line(cx - 3.5 * u, cy - 3.0 * u, cx - 3.5 * u, cy + 3.0 * u, 0.7 * u, c);
            draw_line(cx + 3.5 * u, cy - 3.0 * u, cx + 3.5 * u, cy + 3.0 * u, 0.7 * u, c);
        }
        Ico::Smiley => {
            let gs = (h * 0.7).min(w * 0.9);
            font.glyph(1, cx - gs * 0.55, cy - gs * 0.5, gs * 0.66, gs, Color::from_rgba(240, 220, 120, 255));
            font.glyph(1, cx - gs * 0.1, cy - gs * 0.4, gs * 0.66, gs, Color::from_rgba(200, 200, 200, 255));
        }
        Ico::Glyph(g, c) => {
            let gs = (h * 0.75).min(w * 1.0);
            font.glyph(g, cx - gs * 0.33, cy - gs * 0.5, gs * 0.66, gs, c3(c));
        }
    }
}

/// A labelled 0..=100 bar (needs, stress).
pub fn bar(font: &Font, s: f32, x: f32, y: f32, w: f32, label: &str, value: f32, color: Color) {
    font.draw(label, x, y, s, col::TEXT);
    let bx = x + 12.0 * GW * s;
    let bw = w - 12.0 * GW * s;
    let bh = GH * s - 2.0;
    draw_rectangle(bx, y + 1.0, bw, bh, Color::new(0.12, 0.12, 0.14, 1.0));
    draw_rectangle(bx, y + 1.0, bw * (value / 100.0).clamp(0.0, 1.0), bh, color);
    draw_rectangle_lines(bx, y + 1.0, bw, bh, 1.0, Color::new(0.35, 0.35, 0.38, 1.0));
}

/// "MINER" -> "Miner", "HAUL_STONE" -> "Haul stone".
pub fn pretty(token: &str) -> String {
    let lower = token.replace('_', " ").to_lowercase();
    let mut c = lower.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn skill_level_name(r: i32) -> &'static str {
    const N: [&str; 16] = [
        "Dabbling", "Novice", "Adequate", "Competent", "Skilled", "Proficient", "Talented", "Adept", "Expert",
        "Professional", "Accomplished", "Great", "Master", "High Master", "Grand Master", "Legendary",
    ];
    N[r.clamp(0, 15) as usize]
}

/// "MeetingHall" -> "Meeting hall", "BrewDrink" -> "Brew drink".
pub fn words(camel: &str) -> String {
    let mut out = String::with_capacity(camel.len() + 4);
    for (i, c) in camel.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Draw a named interface sprite (`Gfx::ui_icon`) into `r`. Returns false if there is none.
pub fn draw_icon(gfx: &Gfx, rend: &mut MapRenderer, name: &str, r: Rect, tint: Color) -> bool {
    let Some(mut sp) = gfx.ui_icon(name) else { return false };
    sp = sp.mul_tint([(tint.r * 255.0) as u8, (tint.g * 255.0) as u8, (tint.b * 255.0) as u8, (tint.a * 255.0) as u8]);
    rend.draw_ui_sprite(gfx, &sp, r)
}

/// A button with an icon on the left and a label (build menu entries, zone kinds).
#[allow(clippy::too_many_arguments)]
pub fn icon_label_button(
    ui: &mut UiCtx,
    font: &Font,
    gfx: &Gfx,
    rend: &mut MapRenderer,
    r: Rect,
    icon: &str,
    fallback: Ico,
    label: &str,
    state: BtnState,
) -> bool {
    let hover = ui.hovered(r) && state != BtnState::Disabled;
    let bg = match state {
        BtnState::Active => col::BUTTON_ACTIVE,
        _ if hover => col::BUTTON_HOVER,
        _ => col::BUTTON,
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    let border = match state {
        BtnState::Active => col::ACTIVE_BORDER,
        _ if hover => col::BORDER_HI,
        _ => Color::new(0.30, 0.30, 0.33, 1.0),
    };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, border);
    let s = ui.scale;
    let isz = (r.h - 4.0 * s).min(32.0 * s);
    let ir = Rect::new(r.x + 2.0 * s, r.y + (r.h - isz) / 2.0, isz, isz);
    if icon.is_empty() || !draw_icon(gfx, rend, icon, ir, WHITE) {
        draw_fallback_icon(font, fallback, ir.x, ir.y, ir.w, ir.h);
    }
    let tx = ir.x + ir.w + 6.0 * s;
    let maxc = ((r.x + r.w - tx - 4.0 * s) / (GW * s)).max(1.0) as usize;
    let tc = if state == BtnState::Disabled { col::TEXT_DIM } else { col::TEXT };
    font.draw(&clip(label, maxc), tx, r.y + (r.h - GH * s) / 2.0, s, tc);
    state != BtnState::Disabled && ui.clicked(r)
}

/// A clickable list row (hover highlight). Returns true when clicked.
pub fn list_row(ui: &mut UiCtx, r: Rect, selected: bool) -> bool {
    if selected {
        draw_rectangle(r.x, r.y, r.w, r.h, col::BUTTON_ACTIVE);
    } else if ui.hovered(r) {
        draw_rectangle(r.x, r.y, r.w, r.h, col::BUTTON_HOVER);
    }
    ui.clicked(r)
}

/// A small checkbox; returns true when clicked.
pub fn checkbox(ui: &mut UiCtx, font: &Font, x: f32, y: f32, on: bool, label: &str) -> bool {
    let s = ui.scale;
    let bs = GH * s;
    let w = bs + 6.0 * s + font.width(label, s);
    let r = Rect::new(x, y, w, bs);
    let hov = ui.hovered(r);
    draw_rectangle(x, y, bs, bs, if on { col::BUTTON_ACTIVE } else { col::BUTTON });
    draw_rectangle_lines(x, y, bs, bs, 1.0, if hov { col::BORDER_HI } else { Color::new(0.4, 0.4, 0.44, 1.0) });
    if on {
        font.draw("√", x + (bs - GW * s) / 2.0, y, s, col::TEXT_GOOD);
    }
    font.draw(label, x + bs + 6.0 * s, y, s, if hov { col::TEXT_HI } else { col::TEXT });
    ui.clicked(r)
}

/// Mouse wheel over `r` as a scroll step (+1 = down), and mark the wheel as used.
pub fn wheel_over(ui: &UiCtx, input: &mut crate::keys::InputFrame, r: Rect) -> i32 {
    if r.contains(ui.mouse) && input.wheel != 0 {
        input.wheel_used = true;
        -input.wheel
    } else {
        0
    }
}

/// Truncate to at most `n` characters (with an ellipsis).
pub fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('.');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pretty_names() {
        assert_eq!(pretty("MINER"), "Miner");
        assert_eq!(pretty("HAUL_STONE"), "Haul stone");
        assert_eq!(skill_level_name(5), "Proficient");
        assert_eq!(skill_level_name(20), "Legendary");
        assert_eq!(clip("abcdef", 4), "abc.");
        assert_eq!(words("MeetingHall"), "Meeting hall");
        assert_eq!(words("BrewDrink"), "Brew drink");
        assert_eq!(words("Still"), "Still");
    }
}
