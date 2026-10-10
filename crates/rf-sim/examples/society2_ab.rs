//! [society2] A/B speed of the round-8 society modules on a real fort: the same world runs with
//! locations/justice/lifestyle on and off, interleaved in one-day chunks so both see the same
//! machine load.
//!
//!   cargo run -p rf-sim --release --example society2_ab -- [--export <dir>] [--ticks 67200] [--extra 0]
//!
//! `--extra N` clones N citizens like `examples/scale.rs` (high-population check).

use rf_sim::Sim;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(67_200);
    let extra: usize = get("--extra").and_then(|s| s.parse().ok()).unwrap_or(0);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(1);
    };
    let (mut world, _) = rf_import::load_export(&dir).expect("load export");
    if extra > 0 {
        let cits: Vec<usize> = (0..world.units.len()).filter(|&i| world.units[i].is_worker()).collect();
        let mut next = world.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        for k in 0..extra {
            let mut u = world.units[cits[k % cits.len()]].clone();
            u.id = next;
            u.hist_fig_id = -1;
            u.inventory.clear();
            u.squad_id = -1;
            u.squad_position = -1;
            next += 1;
            world.units.push(u);
        }
    }
    let mut on = Sim::new(world.clone(), 1);
    let mut off = Sim::new(world, 1);
    off.set_society2_enabled(false);
    on.run_ticks(10);
    off.run_ticks(10);
    let (mut t_on, mut t_off) = (0.0f64, 0.0f64);
    let mut done = 0u64;
    while done < ticks {
        let n = 1200u64.min(ticks - done);
        let t = Instant::now();
        on.run_ticks(n as u32);
        t_on += t.elapsed().as_secs_f64();
        let t = Instant::now();
        off.run_ticks(n as u32);
        t_off += t.elapsed().as_secs_f64();
        done += n;
    }
    println!(
        "{} citizens, {} ticks: society2 on {:.0} ticks/s, off {:.0} ticks/s ({:+.1} %)",
        on.world.citizens().count(),
        done,
        done as f64 / t_on,
        done as f64 / t_off,
        (t_on / t_off - 1.0) * 100.0
    );
}
