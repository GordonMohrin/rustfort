//! [construct] Constructions, smoothing, engraving and dumping on an imported fort.
//!
//!   cargo run -p rf-sim --release --example construct -- [--export <dir>] [--ticks 67200] [--seed 1]
//!
//! Picks spots the citizens can reach near the fort's centre: natural walls to smooth (then
//! engrave), free floor tiles in open rooms for constructed walls, open space next to a floor
//! for constructed floors, and a short carved track. Prints progress every 5 days and the
//! construction report at the end (items marked for dumping in the export get dumped too).

use rf_core::{Pos, Shape, TtMaterial, TtSpecial};
use rf_sim::{ConstructionKind, DetailKind, Sim};
use std::path::PathBuf;
use std::time::Instant;

fn stone_like(m: TtMaterial) -> bool {
    matches!(m, TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::LavaStone | TtMaterial::Feature)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(67_200);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let mut sim = Sim::new(world, seed);
    // Centre: the median citizen position.
    let mut cits: Vec<Pos> = sim.world.citizens().map(|u| u.pos).collect();
    cits.sort_by_key(|p| (p.z, p.y, p.x));
    let centre = cits[cits.len() / 2];
    let walk_near = |sim: &Sim, p: Pos| {
        [(0i16, -1i16), (0, 1), (1, 0), (-1, 0)].iter().any(|&(dx, dy)| {
            let q = p.offset(dx, dy, 0);
            sim.is_walkable(q) && sim.reachable(centre, q)
        })
    };
    let busy = |sim: &Sim, p: Pos| sim.world.buildings.iter().any(|b| b.contains(p)) || sim.world.items.iter().any(|i| i.pos == p && i.flags.on_ground);
    let (mut smooth, mut walls, mut floors, mut track) = (Vec::new(), Vec::new(), Vec::new(), None);
    for r in 1..40i16 {
        for z in [centre.z, centre.z - 1, centre.z + 1] {
            for y in centre.y - r..=centre.y + r {
                for x in centre.x - r..=centre.x + r {
                    if (x - centre.x).abs() != r && (y - centre.y).abs() != r {
                        continue;
                    }
                    let p = Pos::new(x, y, z);
                    let Some(t) = sim.world.tile(p) else { continue };
                    if t.hidden() {
                        continue;
                    }
                    let tt = sim.world.tt(p).clone();
                    if smooth.len() < 12 && tt.shape == Shape::Wall && stone_like(tt.material) && tt.special != TtSpecial::Smooth && walk_near(&sim, p) {
                        smooth.push(p);
                    }
                    let open_room = (-1..=1).all(|dy| (-1..=1).all(|dx| sim.is_walkable(p.offset(dx, dy, 0))));
                    if walls.len() < 3 && tt.shape == Shape::Floor && tt.material != TtMaterial::Construction && open_room && sim.reachable(centre, p) && !busy(&sim, p) && walls.iter().all(|w: &Pos| w.dist(p) > 3) {
                        walls.push(p);
                    }
                    if floors.len() < 3 && tt.shape == Shape::Empty && !sim.is_walkable(p) && walk_near(&sim, p) {
                        floors.push(p);
                    }
                    if track.is_none() && stone_like(tt.material) && tt.shape == Shape::Floor && (0..4).all(|k| {
                        let q = p.offset(k, 0, 0);
                        let qt = sim.world.tt(q);
                        qt.shape == Shape::Floor && stone_like(qt.material) && sim.reachable(centre, q) && !busy(&sim, q)
                    }) {
                        track = Some(p);
                    }
                }
            }
        }
    }
    let mut n_smooth = 0;
    for &p in &smooth {
        n_smooth += sim.designate_detail(p, p, DetailKind::Smooth);
    }
    let mut n_walls = 0;
    for &p in &walls {
        n_walls += sim.build_construction(ConstructionKind::Wall, p, p);
    }
    let mut n_floors = 0;
    for &p in &floors {
        n_floors += sim.build_construction(ConstructionKind::Floor, p, p);
    }
    let n_track = track.map(|p| sim.designate_detail(p, p.offset(3, 0, 0), DetailKind::CarveTrack)).unwrap_or(0);
    println!(
        "{} ({}): centre {:?}; designated {n_smooth} smooth, {n_walls} walls {walls:?}, {n_floors} floors {floors:?}, {n_track} track tiles",
        sim.world.meta.fort_name, sim.world.meta.fort_name_native, centre
    );
    let r = sim.construct_report();
    println!("open construct jobs {} (dump jobs from the export: {})", r.open_jobs, r.dump_waiting);
    let t0 = Instant::now();
    let mut engraved_designated = false;
    let step = 6000u64;
    let mut done = 0u64;
    while done < ticks {
        let n = step.min(ticks - done);
        sim.run_ticks(n as u32);
        done += n;
        let r = sim.construct_report();
        if !engraved_designated && r.smoothed >= 4 {
            // Engrave what got smoothed so far.
            let mut k = 0;
            for &p in &smooth {
                k += sim.designate_detail(p, p, DetailKind::Engrave);
            }
            println!("  designated {k} engravings");
            engraved_designated = true;
        }
        println!(
            "  {:>3} {:<10} built {} smoothed {} engraved {} tracks {} dumped {} suspended {} no-material {} open {}",
            sim.world.calendar.day(),
            sim.world.calendar.month_name(),
            r.built,
            r.smoothed,
            r.engraved,
            r.tracks,
            r.dumped,
            r.suspended,
            r.no_material,
            r.open_jobs
        );
    }
    let secs = t0.elapsed().as_secs_f64();
    for e in &sim.world.engravings {
        println!("  engraving at {:?}: quality {} (skill {}), value {}", e.pos, e.quality, e.skill, sim.engraving_value(e));
    }
    for &p in walls.iter().chain(&floors) {
        let tt = sim.world.tt(p);
        println!("  {:?}: {} ({})", p, tt.name, sim.world.material(sim.world.tile(p).unwrap().mat).map(|m| m.name.as_str()).unwrap_or("-"));
    }
    let st = sim.stats();
    println!(
        "paths {} ({} failed, {:.0} nodes/path); construct path failures {}",
        st.paths,
        st.path_failures,
        st.path_nodes as f64 / st.paths.max(1) as f64,
        sim.construct_report().path_failures
    );
    let alive = sim.world.citizens().count();
    println!("{} ticks in {:.2}s = {:.0} ticks/s, {} citizens alive", ticks, secs, ticks as f64 / secs, alive);
}
