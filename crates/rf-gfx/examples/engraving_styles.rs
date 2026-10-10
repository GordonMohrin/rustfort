//! Preview of procedural engraving styles (Gordon's request: graceful patterns instead of
//! DF's dwarf-face picture, patterns that interlock across adjacent engraved tiles).
//!
//! Every pattern is a function of *world* pixel coordinates, so it runs seamlessly over tile
//! borders; every engraved region gets a carved frame that follows its outline (computed from
//! the 8 neighbours). Grooves are shaded like chiselled stone (shadow on the upper-left wall,
//! light on the lower-right wall).
//!
//!   cargo run -p rf-gfx --release --example engraving_styles -- <out_dir>
//!
//! Writes one PNG per style (A..F) plus `styles_sheet.png` and `quality.png`.

use image::{Rgba, RgbaImage};

const T: i32 = 32;
const SS: i32 = 4;

/// '#' wall, '.' smooth floor, 'E' engraved floor.
const ROOM: [&str; 10] = [
    "##############",
    "#............#",
    "#.EEEEEEE....#",
    "#.EEEEEEE..E.#",
    "#.EEE.EEE....#",
    "#.EEEEEEE.EE.#",
    "#.EEEEEEE.EE.#",
    "#....EE......#",
    "#....EEEEEE..#",
    "##############",
];

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Lattice,
    Rings,
    Meander,
    Vines,
    Chevron,
    Rosette,
}

const STYLES: [(Style, &str, char); 6] = [
    (Style::Lattice, "A_rautengitter", 'A'),
    (Style::Rings, "B_ringgeflecht", 'B'),
    (Style::Meander, "C_maeander", 'C'),
    (Style::Vines, "D_wellenranken", 'D'),
    (Style::Chevron, "E_zahnfries", 'E'),
    (Style::Rosette, "F_rosettenmosaik", 'F'),
];

struct Map {
    w: i32,
    h: i32,
    cells: Vec<u8>,
}

impl Map {
    fn new(rows: &[&str]) -> Map {
        let h = rows.len() as i32;
        let w = rows[0].len() as i32;
        let cells = rows.iter().flat_map(|r| r.bytes()).collect();
        Map { w, h, cells }
    }
    fn at(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            b'#'
        } else {
            self.cells[(y * self.w + x) as usize]
        }
    }
    fn eng(&self, x: i32, y: i32) -> bool {
        self.at(x, y) == b'E'
    }
}

fn hash(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE3D);
    h ^ (h >> 13)
}

/// 1 inside a line of half-width `w` around `c`.
fn line(x: f32, c: f32, w: f32) -> f32 {
    if (x - c).abs() < w {
        1.0
    } else {
        0.0
    }
}

fn rem(a: f32, p: f32) -> f32 {
    a.rem_euclid(p)
}

/// Distance to the outline of the engraved region inside tile (tx,ty) at local (lx,ly), and the
/// direction index of the nearest open side (0 N, 1 S, 2 W, 3 E, 4 corner).
fn border(m: &Map, tx: i32, ty: i32, lx: f32, ly: f32) -> (f32, usize, f32) {
    let t = T as f32;
    let mut best = (f32::MAX, 4usize, 0.0f32);
    let mut take = |d: f32, side: usize, u: f32| {
        if d < best.0 {
            best = (d, side, u);
        }
    };
    let wx = tx as f32 * t + lx;
    let wy = ty as f32 * t + ly;
    if !m.eng(tx, ty - 1) {
        take(ly, 0, wx);
    }
    if !m.eng(tx, ty + 1) {
        take(t - ly, 1, wx);
    }
    if !m.eng(tx - 1, ty) {
        take(lx, 2, wy);
    }
    if !m.eng(tx + 1, ty) {
        take(t - lx, 3, wy);
    }
    // Inner corners: both orthogonal neighbours engraved, the diagonal one not.
    for (dx, dy, cx, cy) in [(-1, -1, 0.0, 0.0), (1, -1, t, 0.0), (-1, 1, 0.0, t), (1, 1, t, t)] {
        if m.eng(tx + dx, ty) && m.eng(tx, ty + dy) && !m.eng(tx + dx, ty + dy) {
            take(((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt(), 4, 0.0);
        }
    }
    best
}

/// Groove depth 0..1 of the inner pattern of a style at world pixel (wx, wy).
fn pattern(style: Style, m: &Map, tx: i32, ty: i32, wx: f32, wy: f32, lx: f32, ly: f32) -> f32 {
    let t = T as f32;
    match style {
        Style::Lattice => {
            let p = 16.0;
            let u = rem(wx + wy, p);
            let v = rem(wx - wy, p);
            let du = u.min(p - u);
            let dv = v.min(p - v);
            let mut g: f32 = if du.min(dv) < 0.9 { 1.0 } else { 0.0 };
            // A small drilled boss in every diamond.
            let (cu, cv) = (u - p / 2.0, v - p / 2.0);
            if cu * cu + cv * cv < 2.2 * 2.2 {
                g = 1.0;
            }
            g
        }
        Style::Rings => {
            let p = 16.0;
            let r = p / 2.0f32.sqrt();
            let (ci, cj) = ((wx / p).floor() as i32, (wy / p).floor() as i32);
            let mut g: f32 = 0.0;
            for j in cj - 1..=cj + 2 {
                for i in ci - 1..=ci + 2 {
                    let (cx, cy) = (i as f32 * p, j as f32 * p);
                    let d = ((wx - cx).powi(2) + (wy - cy).powi(2)).sqrt();
                    g = g.max(line(d, r, 0.75));
                    if d < 1.6 {
                        g = 1.0;
                    }
                }
            }
            g
        }
        Style::Meander => {
            // Interior: sparse carved lozenges on a 32 px grid, offset half a tile.
            let p = 32.0;
            let u = rem(wx + 16.0, p) - p / 2.0;
            let v = rem(wy + 16.0, p) - p / 2.0;
            let d = u.abs() + v.abs();
            line(d, 6.0, 0.8).max(line(d, 2.0, 0.9))
        }
        Style::Vines => {
            // Truchet quarter arcs between edge midpoints: continuous waves across tiles.
            let o = hash(tx, ty) & 1;
            let (a, b) = if o == 0 { ((0.0, 0.0), (t, t)) } else { ((t, 0.0), (0.0, t)) };
            let mut g: f32 = 0.0;
            for (cx, cy) in [a, b] {
                let d = ((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt();
                g = g.max(line(d, t / 2.0, 1.0)).max(line(d, t / 2.0 - 4.5, 0.55)).max(line(d, t / 2.0 + 4.5, 0.55));
            }
            // Small leaves where the arcs pass the tile centre region.
            let leaf = hash(tx * 7 + 3, ty * 5 + 1) % 4;
            let (lcx, lcy) = match leaf {
                0 => (t * 0.5, t * 0.5),
                1 => (t * 0.3, t * 0.7),
                2 => (t * 0.7, t * 0.3),
                _ => (-100.0, -100.0),
            };
            let (dx, dy) = (lx - lcx, ly - lcy);
            let e = (dx * 0.8 + dy * 0.6).powi(2) / 9.0 + (dx * -0.6 + dy * 0.8).powi(2) / 2.2;
            if (e - 1.0).abs() < 0.35 {
                g = 1.0;
            }
            let _ = (wx, wy, m);
            g
        }
        Style::Chevron => {
            let p = 16.0;
            let tri = (rem(wx, p) - p / 2.0).abs() * 0.75;
            let s = rem(wy - tri, 8.0);
            let row = ((wy - tri) / 8.0).floor() as i32;
            let w = if row.rem_euclid(2) == 0 { 0.9 } else { 0.5 };
            line(s, 4.0, w)
        }
        Style::Rosette => {
            let (cx, cy) = (lx - t / 2.0, ly - t / 2.0);
            let r = (cx * cx + cy * cy).sqrt();
            let th = cy.atan2(cx);
            let rp = 8.5 * (0.62 + 0.38 * (8.0 * th).cos());
            let mut g = line(r, rp, 0.75).max(line(r, 11.5, 0.6));
            if r < 1.8 {
                g = 1.0;
            }
            // Connect to engraved neighbours with a double band.
            let band = |along: f32, across: f32| -> f32 {
                if along > 11.5 {
                    line(across, -2.8, 0.55).max(line(across, 2.8, 0.55))
                } else {
                    0.0
                }
            };
            if m.eng(tx + 1, ty) {
                g = g.max(band(cx, cy));
            }
            if m.eng(tx - 1, ty) {
                g = g.max(band(-cx, cy));
            }
            if m.eng(tx, ty + 1) {
                g = g.max(band(cy, cx));
            }
            if m.eng(tx, ty - 1) {
                g = g.max(band(-cy, cx));
            }
            // Diamonds where four engraved tiles meet.
            for (dx, dy, kx, ky) in [(-1, -1, 0.0, 0.0), (1, -1, t, 0.0), (-1, 1, 0.0, t), (1, 1, t, t)] {
                if m.eng(tx + dx, ty) && m.eng(tx, ty + dy) && m.eng(tx + dx, ty + dy) {
                    let d = (lx - kx).abs() + (ly - ky).abs();
                    g = g.max(line(d, 5.0, 0.7));
                }
            }
            g
        }
    }
}

/// The carved frame along the region outline (style C uses a meander band instead).
fn frame(style: Style, d: f32, side: usize, u: f32, lx: f32, ly: f32, m: &Map, tx: i32, ty: i32) -> (f32, f32) {
    // Returns (groove, inner limit for the pattern).
    if style == Style::Meander {
        let t = T as f32;
        if d < 12.0 {
            // Corner squares where two open sides meet: a square with a cross.
            let open = |dx: i32, dy: i32| !m.eng(tx + dx, ty + dy);
            let corner = [(0.0, 0.0, open(0, -1) && open(-1, 0)), (t, 0.0, open(0, -1) && open(1, 0)), (0.0, t, open(0, 1) && open(-1, 0)), (t, t, open(0, 1) && open(1, 0))];
            for (cx, cy, on) in corner {
                if on && (lx - cx).abs() < 12.0 && (ly - cy).abs() < 12.0 {
                    let (qx, qy) = ((lx - cx).abs(), (ly - cy).abs());
                    let sq = (qx.max(qy) - 7.0).abs() < 0.8 || (qx.max(qy) - 11.0).abs() < 0.9 || (qx.max(qy) < 7.0 && (qx - qy).abs() < 0.8);
                    return (if sq { 1.0 } else { 0.0 }, 13.0);
                }
            }
            if side < 4 {
                let v = d - 1.5;
                let mm = rem(u, 10.0);
                let rails = v < 1.0 || v > 9.0;
                let key = (mm < 1.2 && v > 1.0 && v < 7.0)
                    || (v > 5.8 && v < 7.0 && mm < 7.0)
                    || (mm > 5.8 && mm < 7.0 && v > 3.0 && v < 7.0)
                    || (v > 3.0 && v < 4.2 && mm > 3.0 && mm < 7.0)
                    || (mm > 3.0 && mm < 4.2 && v > 3.0 && v < 5.0);
                return (if (rails || key) && v > -0.5 { 1.0 } else { 0.0 }, 13.0);
            }
            return (line(d, 6.0, 0.9), 13.0);
        }
        return (0.0, 13.0);
    }
    (line(d, 3.5, 1.0).max(line(d, 6.5, 0.55)), 9.0)
}

fn base_color(rgb: [u8; 3], x: i32, y: i32) -> [f32; 3] {
    let n = (hash(x, y) % 9) as f32 - 4.0;
    [rgb[0] as f32 * 0.62 + n, rgb[1] as f32 * 0.62 + n, rgb[2] as f32 * 0.62 + n]
}

fn render(style: Style, m: &Map, rgb: [u8; 3], inlay: bool) -> RgbaImage {
    let (pw, ph) = ((m.w * T) as usize, (m.h * T) as usize);
    // Groove field at output resolution (supersampled).
    let mut g = vec![0.0f32; pw * ph];
    let mut core = vec![0.0f32; pw * ph]; // groove centre (for the inlay)
    for ty in 0..m.h {
        for tx in 0..m.w {
            if !m.eng(tx, ty) {
                continue;
            }
            for py in 0..T {
                for px in 0..T {
                    let mut acc = 0.0;
                    for sy in 0..SS {
                        for sx in 0..SS {
                            let lx = px as f32 + (sx as f32 + 0.5) / SS as f32;
                            let ly = py as f32 + (sy as f32 + 0.5) / SS as f32;
                            let wx = (tx * T) as f32 + lx;
                            let wy = (ty * T) as f32 + ly;
                            let (d, side, u) = border(m, tx, ty, lx, ly);
                            let (fg, inner) = frame(style, d, side, u, lx, ly, m, tx, ty);
                            let pg = if d > inner { pattern(style, m, tx, ty, wx, wy, lx, ly) } else { 0.0 };
                            acc += fg.max(pg);
                        }
                    }
                    let i = ((ty * T + py) as usize) * pw + (tx * T + px) as usize;
                    g[i] = acc / (SS * SS) as f32;
                    core[i] = if g[i] > 0.8 { 1.0 } else { 0.0 };
                }
            }
        }
    }
    let mut img = RgbaImage::new(pw as u32, ph as u32);
    let gat = |x: i32, y: i32| -> f32 {
        if x < 0 || y < 0 || x >= pw as i32 || y >= ph as i32 {
            0.0
        } else {
            g[y as usize * pw + x as usize]
        }
    };
    for y in 0..ph as i32 {
        for x in 0..pw as i32 {
            let (tx, ty) = (x / T, y / T);
            let c = m.at(tx, ty);
            let mut col = if c == b'#' {
                // Wall: dark rock mass, lighter rim towards floors.
                let n = (hash(x, y) % 7) as f32;
                let mut k = [44.0 + n, 39.0 + n, 36.0 + n];
                let (lx, ly) = (x - tx * T, y - ty * T);
                let rim = (m.at(tx, ty + 1) != b'#' && ly >= T - 3) || (m.at(tx, ty - 1) != b'#' && ly < 2) || (m.at(tx - 1, ty) != b'#' && lx < 2) || (m.at(tx + 1, ty) != b'#' && lx >= T - 2);
                if rim {
                    k = [rgb[0] as f32 * 0.45, rgb[1] as f32 * 0.45, rgb[2] as f32 * 0.45];
                }
                k
            } else {
                let mut b = base_color(rgb, x, y);
                if c == b'.' {
                    // Smooth floor: faint slab joints.
                    let (lx, ly) = (x - tx * T, y - ty * T);
                    if lx == 0 || ly == 0 {
                        for v in &mut b {
                            *v *= 0.9;
                        }
                    }
                }
                b
            };
            if c == b'E' {
                let gv = gat(x, y);
                let lit = (gat(x - 1, y - 1) - gv).max(0.0);
                let shade = (gat(x + 1, y + 1) - gv).max(0.0);
                for v in &mut col {
                    *v = *v * (1.0 - 0.55 * gv) + 70.0 * lit - 25.0 * shade;
                }
                if inlay && core[y as usize * pw + x as usize] > 0.5 {
                    // Masterwork: gold inlay in the groove bottoms.
                    col = [col[0] * 0.3 + 205.0 * 0.7, col[1] * 0.3 + 165.0 * 0.7, col[2] * 0.3 + 60.0 * 0.7];
                }
            }
            img.put_pixel(x as u32, y as u32, Rgba([col[0].clamp(0.0, 255.0) as u8, col[1].clamp(0.0, 255.0) as u8, col[2].clamp(0.0, 255.0) as u8, 255]));
        }
    }
    img
}

fn scale(img: &RgbaImage, k: u32) -> RgbaImage {
    let mut out = RgbaImage::new(img.width() * k, img.height() * k);
    for (x, y, p) in out.enumerate_pixels_mut() {
        *p = *img.get_pixel(x / k, y / k);
    }
    out
}

/// 5x7 glyphs for the panel letters.
fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        _ => [0; 7],
    }
}

fn label(img: &mut RgbaImage, c: char, x0: u32, y0: u32, k: u32) {
    for y in 0..(9 * k) {
        for x in 0..(7 * k) {
            img.put_pixel(x0 + x, y0 + y, Rgba([15, 12, 10, 255]));
        }
    }
    let g = glyph(c);
    for (r, bits) in g.iter().enumerate() {
        for col in 0..5u32 {
            if bits & (0x10 >> col) != 0 {
                for yy in 0..k {
                    for xx in 0..k {
                        img.put_pixel(x0 + k + col * k + xx, y0 + k + r as u32 * k + yy, Rgba([235, 215, 160, 255]));
                    }
                }
            }
        }
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "target/engraving_styles".into());
    std::fs::create_dir_all(&out).unwrap();
    let m = Map::new(&ROOM);
    let chert = [188, 152, 126];
    let k = 2;
    let (pw, ph) = ((m.w * T) as u32 * k, (m.h * T) as u32 * k);
    let gap = 16u32;
    let mut sheet = RgbaImage::from_pixel(pw * 2 + gap * 3, ph * 3 + gap * 4, Rgba([20, 18, 16, 255]));
    for (n, (style, name, letter)) in STYLES.iter().enumerate() {
        let img = scale(&render(*style, &m, chert, false), k);
        img.save(format!("{out}/{name}.png")).unwrap();
        let (cx, cy) = ((n as u32 % 2) * (pw + gap) + gap, (n as u32 / 2) * (ph + gap) + gap);
        for (x, y, p) in img.enumerate_pixels() {
            sheet.put_pixel(cx + x, cy + y, *p);
        }
        label(&mut sheet, *letter, cx + 6, cy + 6, 5);
    }
    sheet.save(format!("{out}/styles_sheet.png")).unwrap();
    // Quality: style F and A ordinary vs masterwork (gold inlay), in rock salt for contrast.
    let salt = [255, 192, 203];
    let a = scale(&render(Style::Rosette, &m, chert, false), k);
    let b = scale(&render(Style::Rosette, &m, chert, true), k);
    let c = scale(&render(Style::Lattice, &m, salt, false), k);
    let d = scale(&render(Style::Lattice, &m, salt, true), k);
    let mut q = RgbaImage::from_pixel(pw * 2 + gap * 3, ph * 2 + gap * 3, Rgba([20, 18, 16, 255]));
    for (n, img) in [a, b, c, d].iter().enumerate() {
        let (cx, cy) = ((n as u32 % 2) * (pw + gap) + gap, (n as u32 / 2) * (ph + gap) + gap);
        for (x, y, p) in img.enumerate_pixels() {
            q.put_pixel(cx + x, cy + y, *p);
        }
    }
    q.save(format!("{out}/quality.png")).unwrap();
    println!("wrote {out}/styles_sheet.png, quality.png and one PNG per style");
}
