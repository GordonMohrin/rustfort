//! [saves10] Save/load continuation on a real export: run N ticks, take the save copy of the
//! world (`Sim::world_for_save`, i.e. world + module state, exactly what a save file holds),
//! build a new sim from it (what loading does), then run M more ticks on both and compare with
//! the uninterrupted run.
//!
//!   cargo run -p rf-sim --release --example saveload -- [--export <dir>] [--n 20000] [--m 20000]
//!       [--seed 1] [--every 0]
//!
//! Prints per part of the state hash (`Sim::state_hash_parts`) whether it is identical right
//! after the load and after M ticks, whether every module's saved state survives the reload
//! unchanged (restore -> save again gives the same JSON), and summary numbers of both runs.
//! `--every K` also compares the full state hash every K ticks and prints the first divergence.

use rf_sim::Sim;
use std::path::PathBuf;

fn summary(s: &Sim) -> Vec<(&'static str, i64)> {
    let st = s.stats();
    vec![
        ("citizens", s.world.citizens().filter(|u| !u.flags.dead).count() as i64),
        ("units", s.world.units.len() as i64),
        ("dead units", st.dead_units as i64),
        ("items", s.world.items.len() as i64),
        ("open jobs", st.open_jobs as i64),
        ("mood records", s.mood_records().len() as i64),
        ("petitions", s.petitions().len() as i64),
        ("buildings", s.world.buildings.len() as i64),
        ("constructions", s.world.constructions.len() as i64),
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let n: u32 = get("--n").and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let m: u32 = get("--m").and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let seed: u64 = get("--seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    let every: u32 = get("--every").and_then(|s| s.parse().ok()).unwrap_or(0);
    let explicit = get("--export").map(PathBuf::from);
    let Some(dir) = rf_import::find_export(explicit.as_deref()) else {
        eprintln!("no export found (use --export <dir> or RUSTFORT_EXPORT)");
        std::process::exit(2);
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let t0 = std::time::Instant::now();
    let mut a = Sim::new(world, seed);
    a.run_ticks(n);
    let ts = std::time::Instant::now();
    let saved = a.world_for_save();
    let save_secs = ts.elapsed().as_secs_f64();
    let modules = saved.sim_state.clone();
    // [saves11] Through the save file's encoding (MessagePack, named fields, as rf-app's
    // format 2) unless --memory: what a real save + load gives.
    let exact_len = saved.sim_exact.len();
    let ts = std::time::Instant::now();
    let saved = if args.iter().any(|a| a == "--memory") {
        saved
    } else {
        let raw = rmp_serde::to_vec_named(&saved).expect("encode world");
        let mut w: rf_core::World = rmp_serde::from_slice(&raw).expect("decode world");
        w.tiletypes.reindex();
        println!("world body {:.1} MB raw (snapshot {:.1} MB)", raw.len() as f64 / 1e6, exact_len as f64 / 1e6);
        w
    };
    let codec_secs = ts.elapsed().as_secs_f64();
    let ts = std::time::Instant::now();
    let mut b = Sim::new(saved, seed);
    let load_secs = ts.elapsed().as_secs_f64();
    println!("export {}  seed {seed}  N {n}  M {m}  ({:.1}s)", dir.display(), t0.elapsed().as_secs_f64());
    println!("restore: {}  world_for_save {save_secs:.3}s  encode+decode {codec_secs:.3}s  Sim::new {load_secs:.3}s", b.restore_mode());
    if args.iter().any(|a| a == "--sizes") {
        for (k, v) in a.exact_sizes() {
            println!("  snapshot part {k:<11} {:>10} bytes", v);
        }
    }

    // 1. Module state survives the reload unchanged.
    let again = b.save_modules();
    let mut exact = 0;
    for (k, v) in &modules {
        let same = again.get(k) == Some(v);
        exact += same as usize;
        println!("module state {k:<10} {:>7} bytes  restored {}", v.len(), if same { "exactly" } else { "DIFFERENT" });
    }
    println!("{exact}/{} module states restored exactly", modules.len());

    // 2. State hash parts right after the load.
    let pa = a.state_hash_parts();
    let pb = b.state_hash_parts();
    let line = |pa: &[(&str, u64)], pb: &[(&str, u64)]| -> String {
        pa.iter().zip(pb).map(|((k, x), (_, y))| format!("{k}={}", if x == y { "=" } else { "X" })).collect::<Vec<_>>().join(" ")
    };
    println!("after load:   {}", line(&pa, &pb));
    let mut shown = 0;
    for u in &a.world.units {
        let (x, y) = (a.unit_needs(u.id), b.unit_needs(u.id));
        if x != y && shown < 5 {
            shown += 1;
            println!("  needs differ: unit {} {} citizen={} animal={}: {:?} vs {:?} ({})", u.id, u.race, u.flags.citizen, u.flags.animal, x, y, a.unit_status(u.id).activity);
        }
    }
    for (k, v) in &modules {
        if again.get(k) != Some(v) {
            let w = again.get(k).cloned().unwrap_or_default();
            let i = v.bytes().zip(w.bytes()).position(|(p, q)| p != q).unwrap_or(0);
            println!("  {k} differs at byte {i}: ...{}... vs ...{}...", &v[i.saturating_sub(60)..(i + 60).min(v.len())], &w[i.saturating_sub(60)..(i + 60).min(w.len())]);
        }
    }

    // 3. Continue both.
    let mut first_div = None;
    if every > 0 {
        let mut done = 0;
        while done < m {
            let k = every.min(m - done);
            a.run_ticks(k);
            b.run_ticks(k);
            done += k;
            if first_div.is_none() && a.state_hash() != b.state_hash() {
                first_div = Some(done);
            }
        }
    } else {
        a.run_ticks(m);
        b.run_ticks(m);
    }
    let pa = a.state_hash_parts();
    let pb = b.state_hash_parts();
    println!("after +M:     {}", line(&pa, &pb));
    if let Some(d) = first_div {
        println!("full state hash first differs {d} ticks after the load");
    }
    println!("full state hash: direct {:016x}  reloaded {:016x}  {}", a.state_hash(), b.state_hash(), if a.state_hash() == b.state_hash() { "IDENTICAL" } else { "different" });
    println!("{:<14} {:>10} {:>10}", "", "direct", "reloaded");
    for ((k, x), (_, y)) in summary(&a).iter().zip(summary(&b)) {
        println!("{k:<14} {x:>10} {y:>10}");
    }
}
