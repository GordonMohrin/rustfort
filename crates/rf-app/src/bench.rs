//! `rustfort bench`: headless ticks/s measurement through the rf_sim::Sim API.

use std::time::Instant;

pub fn run(args: &crate::Args) {
    let (ticks, seed) = (args.ticks, args.seed);
    let t0 = Instant::now();
    let loaded = crate::load::load(args);
    let load_s = t0.elapsed().as_secs_f64();
    if let Some(w) = &loaded.warning {
        eprintln!("note: {w}");
    }
    let w = &loaded.world;
    println!(
        "world: {}  |  {} units ({} citizens), {} items, {} buildings, {} tiles  |  loaded in {:.2}s",
        loaded.label,
        w.units.len(),
        w.citizens().count(),
        w.items.len(),
        w.buildings.len(),
        w.tiles.len(),
        load_s
    );
    let mut sim = rf_sim::Sim::new(loaded.world, seed);
    // Warm-up (caches, lazy init).
    sim.run_ticks(10);
    let mut phases: Vec<(&'static str, f64)> = Vec::new();
    let chunk: u64 = 100;
    let start = Instant::now();
    let mut done = 0u64;
    let mut last_report = Instant::now();
    let citizens0 = sim.world.citizens().count();
    let mut deaths: Vec<String> = Vec::new();
    let mut ann_counts: std::collections::BTreeMap<String, usize> = Default::default();
    while done < ticks {
        let n = chunk.min(ticks - done);
        sim.run_ticks(n as u32);
        done += n;
        for a in sim.drain_announcements() {
            if a.kind == rf_sim::AnnouncementKind::Death {
                deaths.push(format!("tick {}: {}", a.tick, a.text));
            }
            // Group by the text after the unit name (e.g. "cancels Dig: Could not find path.").
            let key = a.text.split_once(" cancels ").map(|(_, r)| format!("cancels {r}")).unwrap_or_else(|| format!("{:?}", a.kind));
            *ann_counts.entry(key).or_default() += 1;
        }
        for (name, s) in sim.stats().phase_times {
            match phases.iter_mut().find(|(p, _)| *p == name) {
                Some(e) => e.1 += s,
                None => phases.push((name, s)),
            }
        }
        if last_report.elapsed().as_secs_f64() > 2.0 {
            let el = start.elapsed().as_secs_f64();
            eprintln!("  {done}/{ticks} ticks, {:.0} ticks/s", done as f64 / el);
            last_report = Instant::now();
        }
    }
    let el = start.elapsed().as_secs_f64().max(1e-9);
    let st = sim.stats();
    println!(
        "ran {done} ticks in {el:.3}s = {:.0} ticks/s  (DF 1x = 100 ticks/s -> {:.1}x)",
        done as f64 / el,
        done as f64 / el / 100.0
    );
    println!(
        "sim: tick {}, {} active units, {} open jobs, date {} {} {}",
        st.ticks,
        st.active_units,
        st.open_jobs,
        crate::game::ordinal(sim.world.calendar.day()),
        sim.world.calendar.month_name(),
        sim.world.calendar.year
    );
    if !phases.is_empty() {
        let total: f64 = phases.iter().map(|p| p.1).sum::<f64>().max(1e-12);
        println!("phase times:");
        for (name, s) in &phases {
            println!("  {name:<16} {:>9.3} ms  {:>5.1}%  ({:.2} us/tick)", s * 1000.0, s / total * 100.0, s / done as f64 * 1e6);
        }
    }
    let alive = sim.world.citizens().count();
    println!("citizens: {citizens0} at start, {alive} alive at end, {} deaths", deaths.len());
    for d in deaths.iter().take(15) {
        println!("  {d}");
    }
    if !ann_counts.is_empty() {
        println!("announcements:");
        for (k, n) in &ann_counts {
            println!("  {n:>6}  {k}");
        }
    }
    println!("state hash: {:016x}", sim.state_hash());
}
