//! Room values of an imported fort compared with the room levels DF reported (audit-eco).
//!
//!   cargo run -p rf-sim --release --example rooms -- [--export <dir>]
//!
//! For every owned bedroom and every dining hall: the furniture value alone (v0.5 room value),
//! the full value (furniture + floor + bordering walls + engravings, wiki "Room"/"Engraving"),
//! the tier of each under the wiki thresholds, and the severity of the owner's last
//! `BedroomQuality` / the diners' `DiningQuality` thoughts in the export (DF's own verdict).

use rf_sim::Sim;
use std::collections::BTreeMap;
use std::path::PathBuf;

const T_NEW: [i32; 7] = [100, 250, 500, 1000, 1500, 2500, 10_000];
const T_OLD: [i32; 7] = [1, 100, 250, 500, 1000, 1500, 2500];

fn tier(v: i32, t: &[i32; 7]) -> i32 {
    t.iter().filter(|&&x| v >= x).count() as i32
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let explicit = args.iter().position(|a| a == "--export").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let sim = Sim::new(world, 1);
    let w = &sim.world;
    let furn_kinds = ["Bed", "Table", "Chair", "Door", "Cabinet", "Box", "Coffin", "Statue", "Armorstand", "Weaponrack", "Slab", "Bookcase", "Well"];
    let mut rows = 0;
    let mut hist: BTreeMap<(i32, i32), u32> = BTreeMap::new(); // (DF severity + 2, our new tier)
    let mut hist_old: BTreeMap<(i32, i32), u32> = BTreeMap::new();
    for z in w.buildings.iter().filter(|b| b.kind == "Civzone" && (b.subtype == "Bedroom" || b.subtype == "DiningHall")) {
        let Some(full) = sim.room_value(z.id) else { continue };
        let inside = |p: rf_core::Pos| {
            if p.z != z.z || p.x < z.x1 || p.x > z.x2 || p.y < z.y1 || p.y > z.y2 {
                return false;
            }
            if z.extents.is_empty() {
                return true;
            }
            let wd = (z.x2 - z.x1 + 1) as usize;
            z.extents.get((p.y - z.y1) as usize * wd + (p.x - z.x1) as usize).copied().unwrap_or(0) != 0
        };
        let furn: i32 = w
            .buildings
            .iter()
            .filter(|b| b.built && furn_kinds.contains(&b.kind.as_str()) && inside(b.center))
            .map(|b| if b.items.is_empty() { 10 } else { b.items.iter().filter_map(|&id| w.item(id)).map(|i| i.value.max(0)).sum() })
            .sum();
        let sev: Option<i32> = if z.subtype == "Bedroom" {
            w.units.iter().find(|u| u.id == z.owner).and_then(|u| u.emotions.iter().rev().find(|e| e.thought == "BedroomQuality")).map(|e| e.severity)
        } else {
            let mut c: BTreeMap<i32, u32> = BTreeMap::new();
            for u in &w.units {
                if let Some(e) = u.emotions.iter().rev().find(|e| e.thought == "DiningQuality") {
                    *c.entry(e.severity).or_default() += 1;
                }
            }
            c.into_iter().max_by_key(|&(s, n)| (n, -s)).map(|(s, _)| s)
        };
        if z.subtype == "Bedroom" && z.owner < 0 {
            continue;
        }
        rows += 1;
        if rows <= 40 {
            println!(
                "{:<10} {:>6} {:<20} furn {:>6} (old tier {}) full {:>6} (tier {})  DF severity {:?}",
                z.subtype,
                z.id,
                z.name.chars().take(20).collect::<String>(),
                furn,
                tier(furn, &T_OLD),
                full,
                tier(full, &T_NEW),
                sev
            );
        }
        if let Some(sv) = sev {
            *hist.entry((sv + 2, tier(full, &T_NEW))).or_default() += 1;
            *hist_old.entry((sv + 2, tier(furn, &T_OLD))).or_default() += 1;
        }
    }
    let score = |h: &BTreeMap<(i32, i32), u32>| -> (u32, u32, u32) {
        let (mut exact, mut near, mut all) = (0, 0, 0);
        for (&(d, o), &n) in h {
            all += n;
            if d == o {
                exact += n;
            }
            if (d - o).abs() <= 1 {
                near += n;
            }
        }
        (exact, near, all)
    };
    println!("rooms: {rows}");
    println!("v0.5 (furniture only, old thresholds): exact/within-1/total = {:?}", score(&hist_old));
    println!("now  (furniture+tiles+engravings, wiki thresholds): exact/within-1/total = {:?}", score(&hist));
    println!("(DF tier, our tier) -> rooms: {hist:?}");
}
