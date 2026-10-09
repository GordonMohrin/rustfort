//! DF bitmap font: `data/art/curses_640x300.png` from the user's install (16x16 CP437 glyphs
//! of 8x12 px, magenta = transparent), exactly the font the Steam version uses for its UI.
//! Falls back to macroquad's built-in font if the file is missing.

use macroquad::prelude::*;
use std::path::Path;

pub const GW: f32 = 8.0;
pub const GH: f32 = 12.0;

const CP437_LOW: &str = "\u{0}☺☻♥♦♣♠•◘○◙♂♀♪♫☼►◄↕‼¶§▬↨↑↓→←∟↔▲▼";
const CP437_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

/// Map a Unicode char to its CP437 code (DF's font layout). Unknown -> '?'.
pub fn cp437(c: char) -> u8 {
    let u = c as u32;
    if (32..127).contains(&u) {
        return u as u8;
    }
    if c == '⌂' {
        return 127;
    }
    if let Some(i) = CP437_LOW.chars().position(|x| x == c) {
        return i as u8;
    }
    if let Some(i) = CP437_HIGH.chars().position(|x| x == c) {
        return 128 + i as u8;
    }
    // Common Latin letters outside CP437: strip the accent.
    match c {
        'À' | 'Á' | 'Â' | 'Ã' => b'A',
        'È' | 'Ê' | 'Ë' => b'E',
        'Ì' | 'Í' | 'Î' | 'Ï' => b'I',
        'Ò' | 'Ó' | 'Ô' | 'Õ' => b'O',
        'Ù' | 'Ú' | 'Û' => b'U',
        'ã' => b'a',
        'õ' => b'o',
        '–' | '—' => b'-',
        '’' | '‘' => b'\'',
        '“' | '”' => b'"',
        _ => b'?',
    }
}

pub struct Font {
    tex: Option<Texture2D>,
}

impl Font {
    pub fn load(df_dir: &Path) -> Font {
        let path = df_dir.join("data").join("art").join("curses_640x300.png");
        let tex = std::fs::read(&path).ok().and_then(|bytes| {
            let mut img = Image::from_file_with_format(&bytes, Some(ImageFormat::Png)).ok()?;
            for px in img.bytes.chunks_exact_mut(4) {
                let magenta = px[0] > 200 && px[1] < 60 && px[2] > 200;
                if magenta {
                    px.copy_from_slice(&[0, 0, 0, 0]);
                }
            }
            let t = Texture2D::from_image(&img);
            t.set_filter(FilterMode::Nearest);
            Some(t)
        });
        Font { tex }
    }

    /// Width in pixels of `s` at `scale`.
    pub fn width(&self, s: &str, scale: f32) -> f32 {
        s.chars().count() as f32 * GW * scale
    }

    /// Draw one CP437 glyph stretched into the given rectangle.
    pub fn glyph(&self, code: u8, x: f32, y: f32, w: f32, h: f32, color: Color) {
        match &self.tex {
            Some(t) => {
                let (gx, gy) = ((code % 16) as f32 * GW, (code / 16) as f32 * GH);
                draw_texture_ex(
                    t,
                    x,
                    y,
                    color,
                    DrawTextureParams {
                        dest_size: Some(vec2(w, h)),
                        source: Some(Rect::new(gx, gy, GW, GH)),
                        ..Default::default()
                    },
                );
            }
            None => {
                let ch = if (32..127).contains(&code) { (code as char).to_string() } else { "*".to_string() };
                draw_text(&ch, x + w * 0.2, y + h * 0.85, h * 1.1, color);
            }
        }
    }

    /// Draw text with its top-left corner at (x, y). Returns the advance width.
    pub fn draw(&self, s: &str, x: f32, y: f32, scale: f32, color: Color) -> f32 {
        let (w, h) = (GW * scale, GH * scale);
        match &self.tex {
            Some(_) => {
                let mut cx = x.round();
                let y = y.round();
                for c in s.chars() {
                    if c != ' ' {
                        self.glyph(cp437(c), cx, y, w, h, color);
                    }
                    cx += w;
                }
                cx - x
            }
            None => {
                draw_text(s, x, y + h * 0.8, h * 1.25, color);
                self.width(s, scale)
            }
        }
    }

    /// Draw text with a 1px dark shadow (readable on top of the map).
    pub fn draw_shadow(&self, s: &str, x: f32, y: f32, scale: f32, color: Color) -> f32 {
        self.draw(s, x + scale, y + scale, scale, Color::new(0.0, 0.0, 0.0, color.a * 0.9));
        self.draw(s, x, y, scale, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp437_tables_have_the_right_size() {
        assert_eq!(CP437_LOW.chars().count(), 32);
        assert_eq!(CP437_HIGH.chars().count(), 128);
        assert_eq!(cp437('A'), 65);
        assert_eq!(cp437('☺'), 1);
        assert_eq!(cp437('▼'), 31);
        assert_eq!(cp437('ó'), 162);
        assert_eq!(cp437('≈'), 247);
        assert_eq!(cp437('■'), 254);
    }
}
