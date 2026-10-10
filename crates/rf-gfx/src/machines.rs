//! [app12] Machines, minecart tracks, rollers, track stops and siege engines from DF Premium's
//! `graphics_machines.txt` / `graphics_buildings.txt` (`SCREWPUMP_*`, `WATER_WHEEL_*`,
//! `WINDMILL_*`, `AXLE_*`, `GEAR_ASSEMBLY_*`, `ROLLERS_*`, `TRACK_STOP_*`, `TRACK_*`,
//! `BALLISTA_*`, `CATAPULT_*`, `BOLT_THROWER_*`).
//!
//! Direction strings (`Building::direction`):
//! - screw pumps: `FromNorth` / `FromSouth` / `FromEast` / `FromWest` (DF "pump from north");
//! - rollers: the direction carts are pushed, `N` / `S` / `E` / `W`;
//! - track stops: the dump direction (`N`, `SE`, ...; "" = no dumping), drawn on the track;
//! - siege engines and windmills: the facing `N`, `NE`, `E`, `SE`, `S`, `SW`, `W`, `NW`;
//! - horizontal axles and water wheels: `NS` / `EW`, else the longer side of the footprint.
//!
//! Tracks: a floor tile with the `TRACK` special gets `TRACK_CARVED_*` (natural floors) or
//! `TRACK_CONSTRUCTED_STONE_*` / `_WOOD_*` (constructed floors) by its track directions.

use crate::{Gfx, Sprite};
use rf_core::{Building, Pos, World};

/// Track direction bits (N, S, E, W).
pub const TN: u8 = 1;
pub const TS: u8 = 2;
pub const TE: u8 = 4;
pub const TW: u8 = 8;

/// Direction bits of a DF track direction string (`NS`, `NSEW`, `EW`, ...).
pub fn track_bits(dir: &str) -> u8 {
    let mut b = 0;
    for c in dir.bytes() {
        b |= match c {
            b'N' => TN,
            b'S' => TS,
            b'E' => TE,
            b'W' => TW,
            _ => 0,
        };
    }
    b
}

/// DF graphics suffix of track bits, in DF's sprite order N, S, W, E (`NSWE`, `SE`, ...).
pub fn track_suffix(bits: u8) -> String {
    let mut s = String::new();
    for (bit, c) in [(TN, 'N'), (TS, 'S'), (TW, 'W'), (TE, 'E')] {
        if bits & bit != 0 {
            s.push(c);
        }
    }
    s
}

/// The 8 facings in DF's order for siege engines and windmills.
pub const FACINGS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

fn facing_index(d: &str) -> usize {
    let u = d.to_ascii_uppercase();
    let u = match u.as_str() {
        "UP" | "NORTH" => "N",
        "DOWN" | "SOUTH" => "S",
        "LEFT" | "WEST" => "W",
        "RIGHT" | "EAST" => "E",
        "UPLEFT" => "NW",
        "UPRIGHT" => "NE",
        "DOWNLEFT" => "SW",
        "DOWNRIGHT" => "SE",
        s => s,
    };
    FACINGS.iter().position(|f| *f == u).unwrap_or(0)
}

/// Cardinal index N, S, E, W of a pump/roller direction (`FromNorth`, `N`, ...).
pub fn cardinal_index(d: &str) -> Option<usize> {
    let l = d.to_ascii_lowercase();
    let l = l.trim_start_matches("from");
    Some(match l {
        "north" | "n" => 0,
        "south" | "s" => 1,
        "east" | "e" => 2,
        "west" | "w" => 3,
        _ => return None,
    })
}

#[derive(Clone, Debug, Default)]
pub struct MachineTables {
    /// Tracks by bits: carved, constructed stone, constructed wood.
    pub track: [[Option<Sprite>; 16]; 3],
    /// Track stops by the track bits under them: stone, wood.
    pub track_stop: [[Option<Sprite>; 16]; 2],
    /// Rollers: stone/wood, N S E W, frame.
    pub rollers: [[[Option<Sprite>; 2]; 4]; 2],
    /// Screw pump (first tile; the second tile is next to it on the page): N S E W, frame.
    pub pump: [[Option<Sprite>; 2]; 4],
    /// Water wheels: NS / WE, frame (multi-tile, one row above the footprint).
    pub wheel: [[Option<Sprite>; 2]; 2],
    /// Windmills by facing (FACINGS order), frame (3x4: a row above the 3x3 footprint).
    pub windmill: [[Option<Sprite>; 2]; 8],
    pub axle_ns: [Option<Sprite>; 2],
    pub axle_we: [Option<Sprite>; 2],
    pub axle_v: [Option<Sprite>; 2],
    pub gear: [Option<Sprite>; 2],
    /// Ballista / catapult (3x3) by facing; bolt thrower (1x1) ready by facing.
    pub ballista: [Option<Sprite>; 8],
    pub catapult: [Option<Sprite>; 8],
    pub bolt_thrower: [Option<Sprite>; 8],
    /// Construction stages 0..3 of ballista and catapult.
    pub ballista_const: [Option<Sprite>; 4],
    pub catapult_const: [Option<Sprite>; 4],
}

impl MachineTables {
    pub fn build(g: &Gfx) -> MachineTables {
        let n = |s: &str| g.named.get(s).copied();
        let mut t = MachineTables::default();
        for bits in 1..16u8 {
            let suf = track_suffix(bits);
            t.track[0][bits as usize] = n(&format!("TRACK_CARVED_{suf}"));
            t.track[1][bits as usize] = n(&format!("TRACK_CONSTRUCTED_STONE_{suf}"));
            t.track[2][bits as usize] = n(&format!("TRACK_CONSTRUCTED_WOOD_{suf}"));
            t.track_stop[0][bits as usize] = n(&format!("TRACK_STOP_STONE_{suf}"));
            t.track_stop[1][bits as usize] = n(&format!("TRACK_STOP_WOOD_{suf}"));
        }
        for (k, d) in ["N", "S", "E", "W"].iter().enumerate() {
            for f in 0..2 {
                t.rollers[0][k][f] = n(&format!("ROLLERS_STONE_{d}_{}", f + 1));
                t.rollers[1][k][f] = n(&format!("ROLLERS_WOOD_{d}_{}", f + 1));
                t.pump[k][f] = n(&format!("SCREWPUMP_{d}_{}", f + 1));
            }
        }
        for f in 0..2 {
            t.wheel[0][f] = n(&format!("WATER_WHEEL_NS_{}", f + 1));
            t.wheel[1][f] = n(&format!("WATER_WHEEL_WE_{}", f + 1));
            for (k, d) in FACINGS.iter().enumerate() {
                t.windmill[k][f] = n(&format!("WINDMILL_{d}_{}", f + 1));
            }
            t.axle_ns[f] = n(&format!("AXLE_HORIZONTAL_NS_{}", f + 1));
            t.axle_we[f] = n(&format!("AXLE_HORIZONTAL_WE_{}", f + 1));
            t.axle_v[f] = n(&format!("AXLE_VERTICAL_{}", f + 1));
            t.gear[f] = n(&format!("GEAR_ASSEMBLY_{}", f + 1));
        }
        for (k, d) in FACINGS.iter().enumerate() {
            t.ballista[k] = n(&format!("BALLISTA_{d}"));
            t.catapult[k] = n(&format!("CATAPULT_{d}"));
            t.bolt_thrower[k] = n(&format!("BOLT_THROWER_READY_{d}"));
        }
        for s in 0..4 {
            t.ballista_const[s] = n(&format!("BALLISTA_CONST_{s}"));
            t.catapult_const[s] = n(&format!("CATAPULT_CONST_{s}"));
        }
        t
    }
}

/// Push the 1x1 piece (lx, ly) of a multi-tile sprite covering a `w` x `h` footprint (plus
/// `s.th - h` rows above it, drawn from the top row at `dy = -1, -2, ...`).
fn slice(s: Sprite, lx: i16, ly: i16, w: i16, h: i16, out: &mut Vec<Sprite>) {
    if s.tw as i16 != w || (s.th as i16) < h {
        // Unexpected layout: the whole picture from the top-left tile.
        if lx == 0 && ly == 0 {
            let roof = (s.th as i16 - h).max(0);
            out.push(s.at(0, -roof));
        }
        return;
    }
    let roof = s.th as i16 - h;
    let one = |ox: i16, oy: i16, dy: i16| Sprite { tx: s.tx + ox as u16, ty: s.ty + oy as u16, tw: 1, th: 1, dx: 0, dy, ..s };
    if ly == 0 {
        for r in 0..roof {
            out.push(one(lx, r, r - roof));
        }
    }
    out.push(one(lx, ly + roof, 0));
}

/// Track sprite of a track floor tile (`kind`: 0 carved, 1 constructed stone, 2 wood).
pub(crate) fn track_sprite(g: &Gfx, kind: usize, bits: u8) -> Option<Sprite> {
    let t = &g.machines_t.track;
    t[kind.min(2)][(bits & 15) as usize].or(t[0][(bits & 15) as usize])
}

/// Sprites of the machine / siege engine / track stop part of `b` at `p`. Returns false when
/// `b` is none of these (the caller draws it its own way).
pub(crate) fn building(g: &Gfx, world: &World, b: &Building, p: Pos, out: &mut Vec<Sprite>) -> bool {
    let t = &g.machines_t;
    let (lx, ly) = (p.x - b.x1, p.y - b.y1);
    let (w, h) = (b.x2 - b.x1 + 1, b.y2 - b.y1 + 1);
    let wood = crate::mat_class(world.material(b.mat)) == crate::MatClass::Wood;
    let rc = |s: Option<Sprite>| s.map(|s| g.recolor(s, world, b.mat));
    let faded = |s: Sprite| if b.built || b.stage < 0 { s } else { s.mul_tint([255, 255, 255, 130]) };
    let dir = b.direction.as_str();
    let ns_axis = |d: &str| d.eq_ignore_ascii_case("NS") || (!d.eq_ignore_ascii_case("EW") && !d.eq_ignore_ascii_case("WE") && h > w);
    match (b.kind.as_str(), b.subtype.as_str()) {
        ("ScrewPump", _) => {
            let k = cardinal_index(dir).unwrap_or(if w > h { 3 } else { 0 });
            if let Some(s) = rc(t.pump[k][0]) {
                let s = if k >= 2 { Sprite { tw: 2, th: 1, ..s } } else { Sprite { tw: 1, th: 2, ..s } };
                let n0 = out.len();
                slice(s, lx, ly, w, h, out);
                for x in &mut out[n0..] {
                    *x = faded(*x);
                }
            }
        }
        ("WaterWheel", _) => {
            let s = rc(t.wheel[if ns_axis(dir) { 0 } else { 1 }][0]);
            if let Some(s) = s {
                let n0 = out.len();
                slice(s, lx, ly, w, h, out);
                for x in &mut out[n0..] {
                    *x = faded(*x);
                }
            }
        }
        ("Windmill", _) => {
            if let Some(s) = rc(t.windmill[facing_index(if dir.is_empty() { "S" } else { dir })][0]) {
                let n0 = out.len();
                slice(s, lx, ly, w, h, out);
                for x in &mut out[n0..] {
                    *x = faded(*x);
                }
            }
        }
        ("GearAssembly", _) => out.extend(rc(t.gear[0]).map(faded)),
        ("AxleVertical", _) => out.extend(rc(t.axle_v[0]).map(faded)),
        ("AxleHorizontal", _) => {
            let s = if ns_axis(dir) { t.axle_ns[0] } else { t.axle_we[0] };
            out.extend(rc(s).map(faded));
        }
        ("Rollers", _) => {
            let k = cardinal_index(dir).unwrap_or(if w > h { 2 } else { 0 });
            out.extend(rc(t.rollers[wood as usize][k][0].or(t.rollers[0][k][0])).map(faded));
        }
        ("Trap", "TrackStop") => {
            let under = world.tiletypes.get(world.tile(p).map(|x| x.tt).unwrap_or(0));
            let bits = if under.special == rf_core::TtSpecial::Track { track_bits(&under.direction) } else { 0 };
            let bits = if bits == 0 { TN | TS } else { bits };
            out.extend(rc(t.track_stop[wood as usize][bits as usize].or(t.track_stop[0][bits as usize])).map(faded));
        }
        ("SiegeEngine", sub) => {
            let f = facing_index(if dir.is_empty() { "N" } else { dir });
            let (set, cons) = match sub {
                "Catapult" => (&t.catapult, &t.catapult_const),
                "BoltThrower" => (&t.bolt_thrower, &t.ballista_const),
                _ => (&t.ballista, &t.ballista_const),
            };
            let built = b.built || b.stage < 0;
            let s = if built || sub == "BoltThrower" { set[f] } else { cons[(b.stage.max(0) as usize).min(3)].or(set[f]) };
            if let Some(s) = rc(s) {
                let n0 = out.len();
                slice(s, lx, ly, w, h, out);
                for x in &mut out[n0..] {
                    *x = faded(*x);
                }
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_names_follow_df_order() {
        assert_eq!(track_bits("NSEW"), 15);
        assert_eq!(track_suffix(15), "NSWE");
        assert_eq!(track_suffix(track_bits("EN")), "NE");
        assert_eq!(track_suffix(track_bits("WS")), "SW");
        assert_eq!(cardinal_index("FromWest"), Some(3));
        assert_eq!(cardinal_index("N"), Some(0));
        assert_eq!(facing_index("SE"), 3);
    }

    #[test]
    fn slices_cover_roof_and_body() {
        let s = Sprite { page: 0, tx: 3, ty: 4, tw: 3, th: 4, tint: [255; 4], dx: 0, dy: 0 };
        let mut out = Vec::new();
        slice(s, 1, 0, 3, 3, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].tx, out[0].ty, out[0].dy), (4, 4, -1));
        assert_eq!((out[1].tx, out[1].ty, out[1].dy), (4, 5, 0));
        out.clear();
        slice(s, 2, 2, 3, 3, &mut out);
        assert_eq!((out[0].tx, out[0].ty, out[0].dy), (5, 7, 0));
    }
}
