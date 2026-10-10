//! Procedural engravings in style "C, Mäander", picked in playtesting (replaces DF Premium's
//! engraving picture). Rustfort's own art, generated at runtime, not DF data.
//!
//! - **Floors:** a Greek-key band runs along the outline of every engraved region, with square
//!   corner rosettes where two open sides meet, and a small carved lozenge in the middle of
//!   every tile inside the region. The key's phase comes from *world* pixel coordinates, so
//!   bands continue seamlessly across neighbouring tiles.
//! - **Walls:** the same key band as a frieze along the sides that face open floor.
//! - **Quality:** ordinary..superior = carved only; exceptional = silver inlay; masterful =
//!   gold inlay in the groove bottoms.
//!
//! The result is an RGBA *overlay* (dark grooves, lit rims, inlay) drawn over the smooth
//! stone, so it keeps the stone's own color. Overlays are cached per [`Key`] on generated
//! pages (see `Gfx::prepare`).

/// Tile size in pixels.
pub(crate) const T: i32 = 32;
/// Supersampling per axis.
const SS: i32 = 4;
/// Period of the key pattern in pixels.
const PERIOD: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    /// Engraved floor (true) or wall frieze (false).
    pub floor: bool,
    /// Open sides (no engraved neighbour of the same kind / facing floor for walls):
    /// bit 0 N, 1 S, 2 W, 3 E.
    pub open: u8,
    /// Floors: inner corners (both orthogonal neighbours engraved, the diagonal not):
    /// bit 0 NW, 1 NE, 2 SW, 3 SE.
    pub inner: u8,
    /// World pixel phase of the tile's left/top edge modulo the key period.
    pub phx: u8,
    pub phy: u8,
    /// 0 carved, 1 silver inlay, 2 gold inlay.
    pub tier: u8,
}

impl Key {
    pub fn phase(tile_x: i32, tile_y: i32) -> (u8, u8) {
        (((tile_x * T).rem_euclid(PERIOD as i32)) as u8, ((tile_y * T).rem_euclid(PERIOD as i32)) as u8)
    }
    pub fn tier_of(quality: u8) -> u8 {
        match quality {
            5.. => 2,
            4 => 1,
            _ => 0,
        }
    }
}

fn line(x: f32, c: f32, w: f32) -> f32 {
    if (x - c).abs() < w {
        1.0
    } else {
        0.0
    }
}

/// The key band: `u` along the edge (world px), `v` = depth into the band (0 at the edge).
fn key_band(u: f32, v: f32) -> f32 {
    let v = v - 1.5;
    if v < -0.5 || v > 9.6 {
        return 0.0;
    }
    let m = u.rem_euclid(PERIOD);
    let rails = v < 1.0 || v > 8.6;
    let key = (m < 1.2 && v > 1.0 && v < 7.0)
        || (v > 5.8 && v < 7.0 && m < 7.0)
        || (m > 5.8 && m < 7.0 && v > 3.0 && v < 7.0)
        || (v > 3.0 && v < 4.2 && m > 3.0 && m < 7.0)
        || (m > 3.0 && m < 4.2 && v > 3.0 && v < 5.0);
    if rails || key {
        1.0
    } else {
        0.0
    }
}

/// Square corner rosette (a framed square with a cross) in the corner at (cx, cy).
fn corner_square(lx: f32, ly: f32, cx: f32, cy: f32) -> Option<f32> {
    let (qx, qy) = ((lx - cx).abs(), (ly - cy).abs());
    if qx >= 12.0 || qy >= 12.0 {
        return None;
    }
    let q = qx.max(qy);
    let on = (q - 7.0).abs() < 0.8 || (q - 11.0).abs() < 0.9 || (q < 7.0 && (qx - qy).abs() < 0.8);
    Some(if on { 1.0 } else { 0.0 })
}

/// Groove depth 0..1 at tile-local (lx, ly).
fn groove(k: Key, lx: f32, ly: f32) -> f32 {
    let t = T as f32;
    let open = |bit: u8| k.open & (1 << bit) != 0;
    // Corner rosettes where two open sides meet.
    for (cx, cy, a, b) in [(0.0, 0.0, 0, 2), (t, 0.0, 0, 3), (0.0, t, 1, 2), (t, t, 1, 3)] {
        if open(a) && open(b) {
            if let Some(g) = corner_square(lx, ly, cx, cy) {
                return g;
            }
        }
    }
    // Nearest open side (band) or inner corner.
    let mut best = (f32::MAX, 4u8, 0.0f32);
    let mut take = |d: f32, side: u8, u: f32| {
        if d < best.0 {
            best = (d, side, u);
        }
    };
    let (ux, uy) = (k.phx as f32 + lx, k.phy as f32 + ly);
    if open(0) {
        take(ly, 0, ux);
    }
    if open(1) {
        take(t - ly, 1, ux);
    }
    if open(2) {
        take(lx, 2, uy);
    }
    if open(3) {
        take(t - lx, 3, uy);
    }
    if k.floor {
        for (bit, cx, cy) in [(0u8, 0.0, 0.0), (1, t, 0.0), (2, 0.0, t), (3, t, t)] {
            if k.inner & (1 << bit) != 0 {
                take(((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt(), 4, 0.0);
            }
        }
    }
    let (d, side, u) = best;
    if d < 12.0 {
        return if side < 4 { key_band(u, d) } else { line(d, 6.0, 0.9) };
    }
    if !k.floor {
        return 0.0;
    }
    // Inside the region: a carved lozenge in the middle of the tile.
    if d > 13.0 {
        let dd = (lx - t / 2.0).abs() + (ly - t / 2.0).abs();
        return line(dd, 6.0, 0.8).max(line(dd, 2.0, 0.9));
    }
    0.0
}

/// Render one 32x32 RGBA overlay tile for `k`.
pub(crate) fn render(k: Key) -> Vec<u8> {
    let n = T as usize;
    // Groove field with a 1 px margin for the bevel shading.
    let m = n + 2;
    let mut g = vec![0.0f32; m * m];
    for py in 0..m {
        for px in 0..m {
            let mut acc = 0.0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let lx = px as f32 - 1.0 + (sx as f32 + 0.5) / SS as f32;
                    let ly = py as f32 - 1.0 + (sy as f32 + 0.5) / SS as f32;
                    if lx < 0.0 || ly < 0.0 || lx >= T as f32 || ly >= T as f32 {
                        continue;
                    }
                    acc += groove(k, lx, ly);
                }
            }
            g[py * m + px] = acc / (SS * SS) as f32;
        }
    }
    let at = |x: usize, y: usize| g[(y + 1) * m + (x + 1)];
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let gv = at(x, y);
            let up = g[y * m + x]; // (x-1, y-1)
            let down = g[(y + 2) * m + (x + 2)]; // (x+1, y+1)
            let lit = (up - gv).max(0.0);
            let shade = (down - gv).max(0.0);
            // Black groove + extra shadow, then a white highlight on the lit wall.
            let ad = (0.52 * gv + 0.18 * shade).min(1.0);
            let al = (0.30 * lit).min(1.0);
            let a = ad + al - ad * al;
            let (mut r, mut gg, mut b) = if a > 0.0 {
                let w = 255.0 * al / a;
                (w, w, w)
            } else {
                (0.0, 0.0, 0.0)
            };
            let mut alpha = a;
            if k.tier > 0 && gv > 0.85 {
                let (ir, ig, ib) = if k.tier >= 2 { (214.0, 172.0, 62.0) } else { (205.0, 210.0, 220.0) };
                r = ir;
                gg = ig;
                b = ib;
                alpha = 0.78;
            }
            let i = (y * n + x) * 4;
            out[i] = r.clamp(0.0, 255.0) as u8;
            out[i + 1] = gg.clamp(0.0, 255.0) as u8;
            out[i + 2] = b.clamp(0.0, 255.0) as u8;
            out[i + 3] = (alpha * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_continue_across_tiles() {
        // Two horizontally adjacent floor tiles with an open north side: the key band rows at
        // the shared edge must match (seamless pattern).
        let (p0x, p0y) = Key::phase(3, 5);
        let (p1x, _) = Key::phase(4, 5);
        let a = Key { floor: true, open: 1, inner: 0, phx: p0x, phy: p0y, tier: 0 };
        let b = Key { floor: true, open: 1, inner: 0, phx: p1x, phy: p0y, tier: 0 };
        // The same world pixel seen from both tiles gives the same groove.
        for ly in 0..12 {
            let ly = ly as f32 + 0.5;
            assert_eq!(groove(a, 31.9, ly), groove(b, -0.1, ly), "row {ly}");
        }
        let ta = render(a);
        assert_eq!(ta.len(), 32 * 32 * 4);
        assert!(ta.chunks_exact(4).any(|p| p[3] > 100), "grooves are visible");
    }

    #[test]
    fn tiers_inlay() {
        let k = Key { floor: true, open: 15, inner: 0, phx: 0, phy: 0, tier: 2 };
        let px = render(k);
        assert!(px.chunks_exact(4).any(|p| p[0] > 200 && p[2] < 90), "gold inlay present");
        let k0 = Key { tier: 0, ..k };
        assert!(!render(k0).chunks_exact(4).any(|p| p[0] > 200 && p[2] < 90));
    }
}
