//! Print a summary of a fort export:
//!   cargo run -p rf-import --release --example inspect -- [dir] [--z N] [--all-citizens]
//! dims, counts, citizens, item/building histograms, tile histograms per z-range and an
//! ASCII map of the z-level with the most citizens (or `--z N`).

use rf_core::{Pos, Shape, TtMaterial, World, NO_MAT};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn top<K: Ord + Clone>(m: &BTreeMap<K, usize>, n: usize) -> Vec<(K, usize)> {
    let mut v: Vec<(K, usize)> = m.iter().map(|(k, c)| (k.clone(), *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

fn shape_char(w: &World, p: Pos) -> char {
    let t = match w.tile(p) {
        Some(t) => t,
        None => return ' ',
    };
    if t.hidden() {
        return ' ';
    }
    if t.liquid_depth() > 0 {
        return if t.is_magma() { '!' } else { '~' };
    }
    let info = w.tiletypes.get(t.tt);
    match info.shape {
        Shape::Wall if info.material == TtMaterial::Tree || info.material == TtMaterial::Mushroom => 'T',
        Shape::Wall if info.material == TtMaterial::Construction => 'H',
        Shape::Wall => '#',
        Shape::Fortification => 'F',
        Shape::Floor if info.material.is_grass() => ',',
        Shape::Floor => '.',
        Shape::Boulder => 'o',
        Shape::Pebbles => ':',
        Shape::StairUp => '<',
        Shape::StairDown => '>',
        Shape::StairUpdown => 'X',
        Shape::Ramp => '^',
        Shape::RampTop => 'v',
        Shape::Branch | Shape::TrunkBranch | Shape::Twig => 't',
        Shape::Shrub => '"',
        Shape::Sapling => '\'',
        Shape::BrookBed | Shape::BrookTop => '~',
        Shape::Empty => ' ',
        Shape::EndlessPit => 'O',
        Shape::None => '?',
    }
}

fn building_char(kind: &str) -> Option<char> {
    Some(match kind {
        "Workshop" | "Furnace" => 'W',
        "Bed" => 'b',
        "Table" => 'n',
        "Chair" => 'h',
        "Door" => '+',
        "Hatch" => '=',
        "Cabinet" | "Box" => 'k',
        "Coffin" => '0',
        "Statue" => 'S',
        "Well" => 'w',
        "Weaponrack" | "Armorstand" => 'a',
        "Trap" => '^',
        "Bridge" => '_',
        "FarmPlot" => 'f',
        "Lever" => 'L',
        "Cage" => 'C',
        _ => return None,
    })
}

fn main() {
    let mut dir: Option<PathBuf> = None;
    let mut zsel: Option<i16> = None;
    let mut all_cit = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--z" => zsel = args.next().and_then(|s| s.parse().ok()),
            "--all-citizens" => all_cit = true,
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let Some(dir) = rf_import::find_export(dir.as_deref()) else {
        eprintln!("no export found (pass a folder, set RUSTFORT_EXPORT, or create {})", rf_import::DEFAULT_EXPORT);
        std::process::exit(1);
    };
    let (w, rep) = match rf_import::load_export(&dir) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("import failed: {e}");
            std::process::exit(1);
        }
    };
    println!("export   {}", dir.display());
    println!(
        "fort     {} ({}), world {}, site {}, DF {} / DFHack {}",
        w.meta.fort_name, w.meta.fort_name_native, w.meta.world_name, w.meta.site_id, w.meta.df_version, w.meta.dfhack_version
    );
    println!(
        "date     {} {} year {} (tick {}), {}",
        w.calendar.day(),
        w.calendar.month_name(),
        w.calendar.year,
        w.calendar.year_tick,
        w.calendar.season_name()
    );
    println!("dims     {} x {} x {} = {} tiles", w.dims.x, w.dims.y, w.dims.z, w.tiles.len());
    println!(
        "counts   units {} (citizens {}), items {}, buildings {}, plants {}, materials {}, tiletypes {}",
        rep.units,
        rep.citizens,
        rep.items,
        rep.buildings,
        rep.plants,
        w.materials.len(),
        w.tiletypes.infos.len()
    );
    println!("loaded   in {:.3} s", rep.seconds);
    println!("warnings ({}):", rep.warnings.len());
    for wr in &rep.warnings {
        println!("  - {wr}");
    }

    // citizens
    let cits: Vec<_> = w.units.iter().filter(|u| u.flags.citizen && !u.flags.dead).collect();
    println!("\ncitizens ({}):", cits.len());
    let shown = if all_cit { cits.len() } else { cits.len().min(15) };
    for u in &cits[..shown] {
        let prof = if u.custom_profession.is_empty() { u.profession.clone() } else { u.custom_profession.clone() };
        println!("  {:>6}  {:<32} {:<18} ({:>3},{:>3},{:>3})", u.id, u.name, prof, u.pos.x, u.pos.y, u.pos.z);
    }
    if shown < cits.len() {
        println!("  ... {} more (--all-citizens)", cits.len() - shown);
    }
    let mut races: BTreeMap<String, usize> = BTreeMap::new();
    for u in &w.units {
        *races.entry(u.race.clone()).or_default() += 1;
    }
    let races: Vec<String> = top(&races, 12).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("units by race: {}", races.join(", "));

    // items / buildings
    let mut ik: BTreeMap<String, usize> = BTreeMap::new();
    for i in &w.items {
        *ik.entry(i.kind.clone()).or_default() += 1;
    }
    println!("\nitem kinds ({} kinds):", ik.len());
    for (k, c) in top(&ik, 20) {
        println!("  {c:>6}  {k}");
    }
    let mut bk: BTreeMap<String, usize> = BTreeMap::new();
    for b in &w.buildings {
        let k = if b.subtype.is_empty() { b.kind.clone() } else { format!("{}/{}", b.kind, b.subtype) };
        *bk.entry(k).or_default() += 1;
    }
    println!("\nbuilding kinds ({} kinds):", bk.len());
    for (k, c) in top(&bk, 25) {
        println!("  {c:>6}  {k}");
    }

    // tiles per z-range
    println!("\ntiles per z-range (shape %, material class %, top materials; hidden = undiscovered):");
    let plane = w.dims.x as usize * w.dims.y as usize;
    let step = 15usize;
    let mut z0 = 0usize;
    while z0 < w.dims.z as usize {
        let z1 = (z0 + step).min(w.dims.z as usize);
        let (mut shapes, mut classes, mut mats): (BTreeMap<String, usize>, BTreeMap<String, usize>, BTreeMap<String, usize>) =
            Default::default();
        let mut hidden = 0usize;
        for t in &w.tiles[z0 * plane..z1 * plane] {
            if t.hidden() {
                hidden += 1;
            }
            let info = w.tiletypes.get(t.tt);
            *shapes.entry(format!("{:?}", info.shape)).or_default() += 1;
            *classes.entry(format!("{:?}", info.material)).or_default() += 1;
            if t.mat != NO_MAT {
                let n = w.material(t.mat).map(|m| m.name.clone()).unwrap_or_else(|| "?".into());
                *mats.entry(n).or_default() += 1;
            }
        }
        let n = ((z1 - z0) * plane).max(1) as f64;
        let fmt = |v: Vec<(String, usize)>| {
            v.into_iter().map(|(k, c)| format!("{k} {:.0}%", c as f64 * 100.0 / n)).collect::<Vec<_>>().join(", ")
        };
        println!("  z{:>3}-{:<3} hidden {:>3.0}%", z0, z1 - 1, hidden as f64 * 100.0 / n);
        println!("      shape: {}", fmt(top(&shapes, 5)));
        println!("      class: {}", fmt(top(&classes, 5)));
        println!("      mats:  {}", fmt(top(&mats, 6)));
        z0 = z1;
    }

    // ASCII map
    let mut per_z: BTreeMap<i16, usize> = BTreeMap::new();
    for u in &cits {
        if w.in_bounds(u.pos) {
            *per_z.entry(u.pos.z).or_default() += 1;
        }
    }
    let z = zsel.unwrap_or_else(|| top(&per_z, 1).first().map(|x| x.0).unwrap_or(w.dims.z as i16 / 2));
    let here: Vec<_> = cits.iter().filter(|u| u.pos.z == z).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (i16::MAX, i16::MAX, i16::MIN, i16::MIN);
    for u in &here {
        x0 = x0.min(u.pos.x);
        y0 = y0.min(u.pos.y);
        x1 = x1.max(u.pos.x);
        y1 = y1.max(u.pos.y);
    }
    if here.is_empty() {
        (x0, y0, x1, y1) = (0, 0, w.dims.x as i16 - 1, w.dims.y as i16 - 1);
    }
    let (mw, mh) = (150i16, 70i16);
    let cx = (x0 + x1) / 2;
    let cy = (y0 + y1) / 2;
    let x0 = (cx - mw / 2).clamp(0, (w.dims.x as i16 - mw).max(0));
    let y0 = (cy - mh / 2).clamp(0, (w.dims.y as i16 - mh).max(0));
    let x1 = (x0 + mw - 1).min(w.dims.x as i16 - 1);
    let y1 = (y0 + mh - 1).min(w.dims.y as i16 - 1);
    let width = (x1 - x0 + 1) as usize;
    let mut grid: Vec<Vec<char>> = (y0..=y1).map(|y| (x0..=x1).map(|x| shape_char(&w, Pos::new(x, y, z))).collect()).collect();
    for b in &w.buildings {
        if b.z != z || b.kind == "Construction" || b.kind == "Civzone" || b.kind == "Stockpile" {
            continue;
        }
        if let Some(ch) = building_char(&b.kind) {
            for y in b.y1.max(y0)..=b.y2.min(y1) {
                for x in b.x1.max(x0)..=b.x2.min(x1) {
                    if b.contains(Pos::new(x, y, z)) {
                        grid[(y - y0) as usize][(x - x0) as usize] = ch;
                    }
                }
            }
        }
    }
    for u in &w.units {
        if u.pos.z == z && u.pos.x >= x0 && u.pos.x <= x1 && u.pos.y >= y0 && u.pos.y <= y1 && !u.flags.dead {
            let ch = if u.flags.citizen { '@' } else if u.flags.hostile { '!' } else if u.flags.animal { 'a' } else { 'u' };
            grid[(u.pos.y - y0) as usize][(u.pos.x - x0) as usize] = ch;
        }
    }
    println!(
        "\nz {z}: {} citizens here; x {x0}..{x1}, y {y0}..{y1}  (@ citizen, a animal, ! hostile, # wall, . floor, , grass, X/</> stairs, ^ ramp, T tree, W workshop, b bed, + door, ~ water)",
        here.len()
    );
    println!("+{}+", "-".repeat(width));
    for row in grid {
        println!("|{}|", row.into_iter().collect::<String>());
    }
    println!("+{}+", "-".repeat(width));
}
