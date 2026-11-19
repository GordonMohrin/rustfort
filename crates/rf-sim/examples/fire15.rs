//! [fire15] Demo: a grass fire on the synthetic surface, a magma dump onto items in the hall
//! and an obsidian casting; prints the fire report and ticks/s.
//!
//!   cargo run --release -p rf-sim --example fire15 [-- --ticks N]

use rf_core::{Item, ItemFlags, MaterialInfo, Pos, World};
use rf_sim::Sim;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ticks: u32 = args.iter().position(|a| a == "--ticks").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(5_000);
    let (dx, dy, dz) = (64u16, 64u16, 20u16);
    let mut w = World::synthetic(dx, dy, dz);
    w.settings.temperature = true;
    let surf = dz as i16 * 2 / 3;
    let hall = (surf - 4).max(0);
    // 200 items for the magma dump: logs, tin and iron bars.
    for (tok, metal) in [("INORGANIC:TIN", true), ("INORGANIC:IRON", true)] {
        w.materials.push(MaterialInfo { token: tok.into(), name: tok[10..].to_ascii_lowercase(), is_metal: metal, mat_type: -1, mat_index: -1, ..Default::default() });
    }
    let (tin, iron) = (w.materials.len() as u16 - 2, w.materials.len() as u16 - 1);
    let (cx, cy) = (dx as i16 / 2, dy as i16 / 2);
    for k in 0..200 {
        let mat = [3u16, tin, iron][k % 3];
        let kind = if mat == 3 { "WOOD" } else { "BAR" };
        let p = Pos::new(cx - 6 + (k % 12) as i16, cy - 4 + (k / 12 % 9) as i16, hall);
        w.items.push(Item { id: 50_000 + k as i32, kind: kind.into(), mat, pos: p, stack: 1, container: -1, holder: -1, building: -1, flags: ItemFlags { on_ground: true, ..Default::default() }, ..Default::default() });
    }
    let mut sim = Sim::new(w, 42);
    sim.ignite_tile(Pos::new(10, 10, surf));
    sim.ignite_tile(Pos::new(50, 40, surf));
    for y in cy - 4..=cy + 4 {
        for x in [cx - 6, cx + 6] {
            sim.add_liquid_source(Pos::new(x, y, hall), true);
        }
    }
    let t0 = std::time::Instant::now();
    let step = (ticks / 10).max(1);
    let mut done = 0;
    while done < ticks {
        sim.run_ticks(step);
        done += step;
        let r = sim.fire_report();
        println!(
            "tick {done:>6}: burning tiles {:>5} burnt {:>5} hot items {:>4} melted {:>3} burned up {:>3} units on fire {} smoke {}",
            r.burning_tiles, r.tiles_burnt, r.hot_items, r.items_melted, r.items_burned_up, r.burning_units, r.smoke_tiles
        );
    }
    let secs = t0.elapsed().as_secs_f64();
    println!("{:#?}", sim.fire_report());
    println!("{} ticks in {:.2}s = {:.0} ticks/s; state hash: {:016x}", done, secs, done as f64 / secs, sim.state_hash());
}
