//! Industry/storage acceptance run on an imported fort: jobs done per work-order kind, stockpile
//! fill (items per pile, items outside piles), stock of bars/blocks/furniture, deaths, ticks/s.
//!
//!   cargo run -p rf-sim --release --example industry -- [--export <dir>] [--ticks 806400] [--seed 1]

use rf_sim::{AnnouncementKind, Sim};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(806_400);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    println!("fort {} from {}: {} citizens, {} items, {} work orders", world.meta.fort_name, dir.display(), world.citizens().count(), world.items.len(), world.work_orders.len());
    let mut sim = Sim::new(world, seed);
    let stock0 = sim.industry_report().stock;
    let s0 = sim.storage_report();
    println!("start: {} items outside piles ({} waiting for a pile)", s0.outside_items, s0.waiting_items);
    let t0 = Instant::now();
    let mut done = 0u64;
    let mut deaths = Vec::new();
    let year = rf_core::TICKS_PER_YEAR as u64;
    let mut year1_jobs: Vec<(&'static str, u64)> = Vec::new();
    while done < ticks {
        let n = 1200.min(ticks - done);
        sim.run_ticks(n as u32);
        done += n;
        for a in sim.drain_announcements() {
            if a.kind == AnnouncementKind::Death {
                deaths.push(format!("tick {}: {}", a.tick, a.text));
            }
        }
        if done == year {
            year1_jobs = sim.production_report().jobs_done;
        }
    }
    let el = t0.elapsed().as_secs_f64();
    println!("ran {done} ticks in {el:.1}s = {:.0} ticks/s", done as f64 / el);
    if year1_jobs.is_empty() {
        year1_jobs = sim.production_report().jobs_done;
    }
    println!("jobs done in year 1:");
    for (l, n) in &year1_jobs {
        println!("  {n:>6}  {l}");
    }
    let r = sim.production_report();
    println!("jobs done in total:");
    for (l, n) in &r.jobs_done {
        println!("  {n:>6}  {l}");
    }
    println!("unsupported: {:?}", r.unsupported);
    let s = sim.storage_report();
    println!(
        "storage: {} stored into containers, {} empty containers brought, {} items rotted, {} rotted away; {} items outside piles ({} waiting for a pile)",
        s.stored_into_containers, s.containers_brought, s.rotted, s.rotted_away, s.outside_items, s.waiting_items
    );
    for p in &s.piles {
        println!(
            "  pile {:>6} {:<28} tiles {:>4} used {:>4} loose {:>4} containers {:>3} (+{:>4} inside)  max bins {:>2} barrels {:>2}",
            p.building_id,
            p.categories.join(","),
            p.tiles,
            p.used_tiles,
            p.loose_items,
            p.containers,
            p.contained_items,
            p.max_bins,
            p.max_barrels
        );
    }
    let ind = sim.industry_report();
    println!("made by industries: {:?}", ind.made);
    println!("stock (start -> end) of bars, blocks, furniture, crafts:");
    let interesting = |k: &str| {
        k.starts_with("BAR")
            || matches!(
                k,
                "BLOCKS" | "BOULDER" | "WOOD" | "SLAB" | "WEAPONRACK" | "ARMORSTAND" | "HATCH_COVER" | "GRATE" | "FLOODGATE" | "TRAPPARTS" | "STATUE" | "CAGE"
                    | "BAG" | "BOX" | "CABINET" | "TABLE" | "CHAIR" | "DOOR" | "COFFIN" | "BARREL" | "BIN" | "BUCKET" | "TOOL" | "GOBLET" | "WEAPON"
                    | "AMMO" | "GLOVES" | "ROUGH" | "POWDER_MISC" | "MEAT" | "CLOTH" | "THREAD" | "FIGURINE" | "AMULET" | "RING" | "CROWN" | "SCEPTER"
                    | "EARRING" | "BRACELET" | "PLANT" | "SEEDS" | "DRINK" | "FOOD"
            )
    };
    for (k, n) in &ind.stock {
        if interesting(k) {
            let before = stock0.iter().find(|e| e.0 == *k).map(|e| e.1).unwrap_or(0);
            println!("  {k:<28} {before:>6} -> {n:>6}");
        }
    }
    println!("citizens alive: {}, deaths: {}", sim.world.citizens().count(), deaths.len());
    for d in deaths.iter().take(20) {
        println!("  {d}");
    }
    println!("state hash {:016x}", sim.state_hash());
}
