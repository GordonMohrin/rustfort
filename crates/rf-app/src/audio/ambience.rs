//! [audio14] Ambience: looping beds from DF's fortress-mode `ambience/` folder, mixed by
//! what the camera looks at (docs/mechanics/audio14.md section 5). `targets` is pure; the
//! inputs are gathered from the world about twice a second.
//!
//! Every number here is an ESTIMATE (DF 53.17 mixes these through FMOD parameters whose
//! curves are not observable).

use rf_core::{liquid, tile_flags, Pos, World};

/// The layers, by file stem in `data/sound/ambience/`.
pub const LAYERS: &[&str] = &[
    "Forest",
    "Grasslands",
    "Desert",
    "Glacier",
    "Swamp",
    "Rainforest",
    "Outside",
    "Neutral_Winds",
    "Neutral_Winds_2",
    "Evil",
    "Good",
    "Terrifying",
    "Blizzard",
    "Thunderstorm",
    "Cavern",
    "Neutral_Cavern",
    "Magma_Close",
    "Magma_Far",
    "Magma_Low",
    "River_Low",
    "River_Medium",
    "River_High",
    "Siege",
    "Combat",
    "Tavern",
    "Trade_Depot",
    "Workshop",
];
pub const N_LAYERS: usize = 27;

/// Gain change per second while a layer moves to its target (full fade in 2 s).
pub const AMB_RAMP_PER_S: f32 = 0.5;
/// Most layers audible at once.
pub const AMB_MAX_LAYERS: usize = 4;
/// Inputs are recomputed this often.
pub const AMB_PERIOD_S: f64 = 0.5;
/// Ambience under music.
pub const AMBIENCE_DUCK: f32 = 0.6;
/// Surface beds fade out over this many z-levels below the surface.
pub const SURFACE_FADE_Z: f32 = 4.0;
/// Active and neutral outdoor beds swap every 60..120 s.
pub const ALT_SWAP_S: (f64, f64) = (60.0, 120.0);
/// Rooms count within this many tiles of the view centre.
pub const ROOM_RADIUS: i32 = 6;

fn li(name: &str) -> usize {
    LAYERS.iter().position(|l| *l == name).unwrap_or(0)
}

/// Weather as far as ambience cares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sky {
    #[default]
    Clear,
    Rain,
    Snow,
}

/// What the camera sees (computed every [`AMB_PERIOD_S`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmbInputs {
    /// z-levels below the surface at the view centre (0 = at or above).
    pub depth: i32,
    pub outside: bool,
    pub cavern: bool,
    pub in_magma_sea: bool,
    /// Distance to the nearest magma (z counts 3x), `None` = none near.
    pub magma_d: Option<i32>,
    /// Flowing outdoor water tiles in view (sampled).
    pub river: u32,
    pub sky: Sky,
    pub siege: bool,
    pub combat: bool,
    pub tavern: bool,
    pub depot: bool,
    pub workshop: bool,
    /// Bed of the embark biome (`Forest`, ...), `None` = unknown.
    pub biome: Option<&'static str>,
    pub evil: bool,
    pub good: bool,
    pub savagery: i32,
}

/// Biome bed of a DF biome token (unknown, mountain and others: `None` -> `Outside`).
pub fn biome_bed(token: &str) -> Option<&'static str> {
    let t = token.to_ascii_uppercase();
    if t.starts_with("FOREST_TROPICAL") {
        Some("Rainforest")
    } else if t.starts_with("FOREST_") {
        Some("Forest")
    } else if t.starts_with("GRASSLAND_") || t.starts_with("SAVANNA_") || t.starts_with("SHRUBLAND_") {
        Some("Grasslands")
    } else if t.starts_with("DESERT_") {
        Some("Desert")
    } else if t == "GLACIER" || t == "TUNDRA" {
        Some("Glacier")
    } else if t.starts_with("SWAMP_") || t.starts_with("MARSH_") {
        Some("Swamp")
    } else {
        None
    }
}

/// Target gain of every layer. `alt`: the outdoor/cavern beds are in their neutral phase.
pub fn targets(inp: &AmbInputs, alt: bool) -> [f32; N_LAYERS] {
    let mut t = [0.0f32; N_LAYERS];
    let surface_w = if inp.depth <= 0 { 1.0 } else { (1.0 - inp.depth as f32 / SURFACE_FADE_Z).max(0.0) };
    if surface_w > 0.0 && !inp.cavern {
        let bed = if inp.evil {
            if inp.savagery >= super::sfx::SAVAGE_MIN {
                "Terrifying"
            } else {
                "Evil"
            }
        } else if inp.good {
            "Good"
        } else {
            inp.biome.unwrap_or("Outside")
        };
        if alt && bed != "Terrifying" && bed != "Evil" {
            t[li("Neutral_Winds")] = surface_w;
        } else {
            t[li(bed)] = surface_w;
        }
        match inp.sky {
            Sky::Snow => t[li("Blizzard")] = surface_w,
            Sky::Rain => t[li("Thunderstorm")] = surface_w * 0.8,
            Sky::Clear => {}
        }
        if inp.river > 0 {
            let r = match inp.river {
                1..=15 => "River_Low",
                16..=60 => "River_Medium",
                _ => "River_High",
            };
            t[li(r)] = surface_w;
        }
    }
    if inp.cavern {
        t[li(if alt { "Neutral_Cavern" } else { "Cavern" })] = 1.0;
    } else if inp.depth > 0 && surface_w < 1.0 {
        t[li("Neutral_Cavern")] = 0.35 * (1.0 - surface_w).max(0.0);
    }
    if inp.in_magma_sea {
        t[li("Magma_Low")] = 1.0;
    }
    match inp.magma_d {
        Some(d) if d <= 3 => t[li("Magma_Close")] = 1.0,
        Some(d) if d <= 10 => t[li("Magma_Far")] = 1.0 - (d - 3) as f32 / 7.0,
        _ => {}
    }
    if inp.siege {
        t[li("Siege")] = 0.8;
    }
    if inp.combat {
        t[li("Combat")] = 1.0;
    }
    if inp.tavern {
        t[li("Tavern")] = 0.9;
    }
    if inp.depot {
        t[li("Trade_Depot")] = 0.8;
    }
    if inp.workshop {
        t[li("Workshop")] = 0.6;
    }
    // Only the loudest few layers play.
    let mut order: [usize; N_LAYERS] = std::array::from_fn(|i| i);
    order.sort_by(|&a, &b| t[b].partial_cmp(&t[a]).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
    for &i in &order[AMB_MAX_LAYERS..] {
        t[i] = 0.0;
    }
    t
}

/// Move `cur` toward `target` at [`AMB_RAMP_PER_S`].
pub fn ramp(cur: f32, target: f32, dt: f32) -> f32 {
    let step = AMB_RAMP_PER_S * dt.max(0.0);
    if cur < target {
        (cur + step).min(target)
    } else {
        (cur - step).max(target)
    }
}

/// Gather the camera inputs from the world (about 5 000 tile reads at most).
/// `caverns`: discovered non-magma cavern z-ranges; `magma_sea`: its z-range if discovered.
pub fn gather(w: &World, c: Pos, view: (i32, i32, i32, i32), caverns: &[(i16, i16)], magma_sea: Option<(i16, i16)>) -> AmbInputs {
    let mut inp = AmbInputs::default();
    let (dx, dy, dz) = (w.dims.x as i32, w.dims.y as i32, w.dims.z as i32);
    if dx <= 0 || dy <= 0 || dz <= 0 {
        return inp; // an empty map (`clamp(0, -1)` below would panic)
    }
    let cx = (c.x as i32).clamp(0, dx - 1);
    let cy = (c.y as i32).clamp(0, dy - 1);
    let vz = (c.z as i32).clamp(0, dz - 1);
    let at = |x: i32, y: i32, z: i32| w.tile(Pos::new(x as i16, y as i16, z as i16));
    // Surface: first z from the top that is not open air (outside and empty).
    let mut surface = dz - 1;
    for z in (0..dz).rev() {
        if let Some(t) = at(cx, cy, z) {
            let info = w.tiletypes.get(t.tt);
            if info.shape.is_solid() || t.flags & tile_flags::OUTSIDE == 0 {
                surface = z;
                break;
            }
        }
    }
    inp.depth = (surface - vz).max(0);
    inp.outside = at(cx, cy, vz).map(|t| t.flags & tile_flags::OUTSIDE != 0).unwrap_or(false);
    if inp.outside {
        inp.depth = 0;
    }
    inp.cavern = caverns.iter().any(|&(a, b)| (a as i32..=b as i32).contains(&vz));
    inp.in_magma_sea = magma_sea.map(|(a, b)| (a as i32..=b as i32).contains(&vz)).unwrap_or(false);
    // Magma within a 31x31x3 box.
    let mut best: Option<i32> = None;
    for z in (vz - 1).max(0)..=(vz + 1).min(dz - 1) {
        for y in (cy - 15).max(0)..=(cy + 15).min(dy - 1) {
            for x in (cx - 15).max(0)..=(cx + 15).min(dx - 1) {
                if let Some(t) = at(x, y, z) {
                    if t.liquid & liquid::DEPTH_MASK > 0 && t.liquid & liquid::MAGMA != 0 {
                        let d = (x - cx).abs().max((y - cy).abs()).max((z - vz).abs() * 3);
                        best = Some(best.map_or(d, |b: i32| b.min(d)));
                    }
                }
            }
        }
    }
    inp.magma_d = best;
    // Flowing outdoor water in view (every 2nd tile, view z and one below).
    let (x0, y0, x1, y1) = view;
    let mut river = 0;
    for z in [vz, vz - 1] {
        if z < 0 {
            continue;
        }
        let mut y = y0.max(0);
        while y <= y1.min(dy - 1) {
            let mut x = x0.max(0);
            while x <= x1.min(dx - 1) {
                if let Some(t) = at(x, y, z) {
                    if t.liquid & liquid::MAGMA == 0 && t.liquid & liquid::DEPTH_MASK >= 1 && t.flags & tile_flags::OUTSIDE != 0 && t.flags & tile_flags::STAGNANT == 0 {
                        river += 1;
                    }
                }
                x += 2;
            }
            y += 2;
        }
    }
    inp.river = river;
    // Rooms near the view centre.
    for b in &w.buildings {
        if b.z as i32 != vz {
            continue;
        }
        let near = (b.x1 as i32 - ROOM_RADIUS..=b.x2 as i32 + ROOM_RADIUS).contains(&cx) && (b.y1 as i32 - ROOM_RADIUS..=b.y2 as i32 + ROOM_RADIUS).contains(&cy);
        if !near {
            continue;
        }
        if b.kind == "TradeDepot" {
            inp.depot = true;
        } else if b.kind == "Workshop" || b.kind == "Furnace" {
            inp.workshop = true;
        }
    }
    let cl = &w.climate;
    inp.biome = biome_bed(&cl.biome);
    inp.evil = cl.evil;
    inp.good = cl.good;
    inp.savagery = cl.savagery;
    inp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(t: &[f32; N_LAYERS], n: &str) -> f32 {
        t[li(n)]
    }

    #[test]
    fn layer_table_is_complete() {
        assert_eq!(LAYERS.len(), N_LAYERS);
    }

    #[test]
    fn surface_beds_fade_with_depth() {
        let mut i = AmbInputs { biome: Some("Forest"), outside: true, ..Default::default() };
        let t = targets(&i, false);
        assert_eq!(g(&t, "Forest"), 1.0);
        assert_eq!(g(&targets(&i, true), "Neutral_Winds"), 1.0);
        i.depth = 2;
        i.outside = false;
        let t = targets(&i, false);
        assert_eq!(g(&t, "Forest"), 0.5);
        assert!(g(&t, "Neutral_Cavern") > 0.0);
        i.depth = 6;
        assert_eq!(g(&targets(&i, false), "Forest"), 0.0);
        assert_eq!(biome_bed("FOREST_TROPICAL_MOIST_BROADLEAF"), Some("Rainforest"));
        assert_eq!(biome_bed("GRASSLAND_TEMPERATE"), Some("Grasslands"));
        assert_eq!(biome_bed("MOUNTAIN"), None);
        let t = targets(&AmbInputs::default(), false);
        assert_eq!(g(&t, "Outside"), 1.0);
    }

    #[test]
    fn caverns_magma_rivers_and_rooms() {
        let i = AmbInputs { depth: 30, cavern: true, ..Default::default() };
        let t = targets(&i, false);
        assert_eq!((g(&t, "Cavern"), g(&t, "Forest"), g(&t, "Outside")), (1.0, 0.0, 0.0));
        let m = |d| g(&targets(&AmbInputs { depth: 40, magma_d: d, ..Default::default() }, false), if d.unwrap_or(99) <= 3 { "Magma_Close" } else { "Magma_Far" });
        assert_eq!(m(Some(2)), 1.0);
        assert!((m(Some(7)) - (1.0 - 4.0 / 7.0)).abs() < 1e-6);
        assert_eq!(m(None), 0.0);
        let r = |n| {
            let t = targets(&AmbInputs { river: n, ..Default::default() }, false);
            [g(&t, "River_Low"), g(&t, "River_Medium"), g(&t, "River_High")]
        };
        assert_eq!(r(0), [0.0, 0.0, 0.0]);
        assert_eq!(r(5), [1.0, 0.0, 0.0]);
        assert_eq!(r(30), [0.0, 1.0, 0.0]);
        assert_eq!(r(61), [0.0, 0.0, 1.0]);
        let t = targets(&AmbInputs { evil: true, savagery: 80, sky: Sky::Rain, ..Default::default() }, true);
        assert_eq!(g(&t, "Terrifying"), 1.0);
        assert_eq!(g(&t, "Thunderstorm"), 0.8);
        // At most four layers.
        let busy = AmbInputs { river: 70, siege: true, combat: true, tavern: true, depot: true, workshop: true, magma_d: Some(1), ..Default::default() };
        let t = targets(&busy, false);
        assert_eq!(t.iter().filter(|&&x| x > 0.0).count(), AMB_MAX_LAYERS);
        assert_eq!(ramp(0.0, 1.0, 1.0), 0.5);
        assert_eq!(ramp(1.0, 0.0, 4.0), 0.0);
    }

    #[test]
    fn empty_maps_do_not_panic() {
        let mut w = World::default();
        w.dims.x = 10;
        w.dims.z = 5;
        let i = gather(&w, Pos::new(3, 3, 2), (0, 0, 9, 9), &[], None);
        assert_eq!(i.depth, 0);
    }

    #[test]
    fn gathers_from_a_synthetic_world() {
        let w = World::synthetic(40, 40, 20);
        let top = Pos::new(20, 20, 19);
        let i = gather(&w, top, (0, 0, 39, 39), &[], None);
        assert_eq!(i.depth, 0);
        let deep = Pos::new(20, 20, 2);
        let j = gather(&w, deep, (0, 0, 39, 39), &[(1, 3)], None);
        assert!(j.depth > 0 && j.cavern);
    }
}
