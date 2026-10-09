//! Spot-check an export against the live game (with tools/dfhack/rustfort-verify.lua):
//!   cargo run -p rf-import --release --example spotcheck -- pick    <dir> <samples.txt> [per_class]
//!   dfhack-run lua -f <repo>/tools/dfhack/rustfort-verify.lua <samples.txt> <result.txt>
//!   cargo run -p rf-import --release --example spotcheck -- compare <dir> <result.txt>
//! `pick` chooses tiles stratified by tiletype material class (deterministic) plus 3 citizens.

use rf_core::{Pos, TtMaterial, World, NO_MAT};
use std::collections::BTreeMap;
use std::path::Path;

fn load(dir: &str) -> World {
    let d = rf_import::find_export(Some(Path::new(dir))).unwrap_or_else(|| panic!("no export in {dir}"));
    rf_import::load_export(&d).expect("load").0
}

fn tok(w: &World, mat: u16) -> String {
    if mat == NO_MAT {
        return "NONE".into();
    }
    w.material(mat).map(|m| m.token.clone()).unwrap_or_else(|| "?".into())
}

fn pick(dir: &str, out: &str, per_class: usize) {
    let w = load(dir);
    let mut by_class: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, t) in w.tiles.iter().enumerate() {
        let c = w.tiletypes.get(t.tt).material;
        if c != TtMaterial::Air && c != TtMaterial::None {
            by_class.entry(format!("{c:?}")).or_default().push(i);
        }
    }
    let mut lines = Vec::new();
    for (class, idx) in &by_class {
        let n = per_class.min(idx.len());
        // deterministic spread over the class: golden-ratio stride
        let mut k = 0usize;
        for j in 0..n {
            k = (k + (idx.len() as f64 * 0.618_034) as usize + j) % idx.len();
            let p = w.pos_of(idx[k]);
            lines.push(format!("tile {} {} {}", p.x, p.y, p.z));
        }
        eprintln!("{class}: {} tiles, {n} sampled", idx.len());
    }
    let cits: Vec<_> = w.citizens().filter(|u| w.in_bounds(u.pos)).collect();
    for u in cits.iter().step_by((cits.len() / 3).max(1)).take(3) {
        lines.push(format!("unit {}", u.id));
    }
    std::fs::write(out, lines.join("\n") + "\n").expect("write samples");
    eprintln!("{} samples -> {out}", lines.len());
}

fn compare(dir: &str, result: &str) {
    let w = load(dir);
    let text = std::fs::read_to_string(result).expect("result file");
    let (mut ok, mut bad) = (0, 0);
    let mut by_class: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    // round-2 checks: (match, differ, missing in import)
    let mut r2: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.first().copied() {
            Some("tile") if f.len() >= 6 => {
                let p = Pos::new(f[1].parse().unwrap(), f[2].parse().unwrap(), f[3].parse().unwrap());
                let t = w.tile(p).copied().unwrap_or_default();
                let info = w.tiletypes.get(t.tt);
                let (name, token) = (info.name.clone(), tok(&w, t.mat));
                let e = by_class.entry(format!("{:?}", info.material)).or_default();
                let df_token = f[5..].join(" "); // tokens may contain spaces (e.g. `PLANT:CAVE MOSS:STRUCTURAL`)
                if name == f[4] && token == df_token {
                    ok += 1;
                    e.0 += 1;
                } else {
                    bad += 1;
                    e.1 += 1;
                    println!("MISMATCH ({},{},{}): import {name} {token} | DF {} {df_token}", p.x, p.y, p.z, f[4]);
                }
            }
            Some("unit") if f.len() >= 5 => {
                let id: i32 = f[1].parse().unwrap();
                let df_pos = Pos::new(f[2].parse().unwrap(), f[3].parse().unwrap(), f[4].parse().unwrap());
                let rest = f[5..].join(" ");
                match w.unit(id) {
                    Some(u) => println!(
                        "unit {id}: import {}|{} at ({},{},{}) | DF {rest} at ({},{},{}) -> name/profession {}, moved {} tiles",
                        u.profession,
                        u.name,
                        u.pos.x,
                        u.pos.y,
                        u.pos.z,
                        df_pos.x,
                        df_pos.y,
                        df_pos.z,
                        if rest == format!("{}|{}", u.profession, u.name) { "MATCH" } else { "DIFFER" },
                        u.pos.dist(df_pos)
                    ),
                    None => println!("unit {id}: not in import"),
                }
            }
            Some("count") if f.len() >= 3 => {
                let got = match f[1] {
                    "units" => w.units.len(),
                    "citizens" => w.citizens().count(),
                    "items" => w.items.len(),
                    "buildings" => w.buildings.len(),
                    "plants" => w.plants.len(),
                    _ => 0,
                };
                println!("count {:<10} import {got:>6} | DF now {:>6}", f[1], f[2]);
            }
            Some("year_tick") => println!("DF time now: year {} tick {} (export end tick {})", f[1], f[2], w.calendar.year_tick),
            // round 2
            Some("orders") if f.len() >= 2 => {
                println!("work orders: import {} | DF now {}", w.work_orders.len(), f[1]);
            }
            Some("order") if f.len() >= 7 => {
                let id: i32 = f[1].parse().unwrap_or(-1);
                let e = r2.entry("orders").or_default();
                match w.work_orders.iter().find(|o| o.id == id) {
                    Some(o) => {
                        let same = o.job == f[2]
                            && o.amount_total.to_string() == f[4]
                            && o.frequency == f[5]
                            && o.conditions.len().to_string() == f[6];
                        if same {
                            e.0 += 1;
                        } else {
                            e.1 += 1;
                            println!(
                                "order {id}: import {} {}/{} {} {} conds | DF {} {}/{} {} {} conds",
                                o.job,
                                o.amount_left,
                                o.amount_total,
                                o.frequency,
                                o.conditions.len(),
                                f[2],
                                f[3],
                                f[4],
                                f[5],
                                f[6]
                            );
                        }
                        if o.amount_left.to_string() != f[3] {
                            println!("order {id} ({}): amount_left {} -> DF now {} (progress since export)", o.job, o.amount_left, f[3]);
                        }
                    }
                    None => {
                        e.2 += 1;
                        println!("order {id} ({}): not in import (created after the export?)", f[2]);
                    }
                }
            }
            Some("wsjobs") if f.len() >= 3 => {
                let id: i32 = f[1].parse().unwrap_or(-1);
                let e = r2.entry("workshop job counts").or_default();
                match w.building(id) {
                    Some(b) if b.jobs.len().to_string() == f[2] => e.0 += 1,
                    Some(b) => {
                        e.1 += 1;
                        let kinds: Vec<&str> = b.jobs.iter().map(|j| j.job.as_str()).collect();
                        println!("workshop {id} {}: import {} jobs [{}] | DF now {}", b.subtype, b.jobs.len(), kinds.join(","), f[2]);
                    }
                    None => e.2 += 1,
                }
            }
            Some("farm") if f.len() >= 3 => {
                let id: i32 = f[1].parse().unwrap_or(-1);
                let e = r2.entry("farm crops").or_default();
                match w.building(id) {
                    Some(b) => {
                        let got: Vec<&str> = b.crops.iter().map(|c| if c.is_empty() { "-" } else { c.as_str() }).collect();
                        if got.join(",") == f[2] {
                            e.0 += 1;
                        } else {
                            e.1 += 1;
                            println!("farm {id}: import {} | DF now {}", got.join(","), f[2]);
                        }
                    }
                    None => e.2 += 1,
                }
            }
            Some("mod") if f.len() >= 3 => {
                let i: usize = f[1].parse().unwrap_or(usize::MAX);
                let e = r2.entry("mods").or_default();
                match w.meta.mods.get(i) {
                    Some(m) if m == f[2] => e.0 += 1,
                    Some(m) => {
                        e.1 += 1;
                        println!("mod {i}: import {m} | DF {}", f[2]);
                    }
                    None => e.2 += 1,
                }
            }
            Some("plant_raws") if f.len() >= 2 => {
                println!("plant raws: import {} | DF {}", w.plant_raws.len(), f[1]);
            }
            _ => {}
        }
    }
    for (c, (o, b)) in &by_class {
        println!("  {c:<14} {o:>4} ok {b:>3} mismatch");
    }
    println!("tiles: {ok} match, {bad} mismatch");
    for (k, (o, b, m)) in &r2 {
        println!("{k}: {o} match, {b} differ, {m} missing in import");
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(|s| s.as_str()) {
        Some("pick") if a.len() >= 3 => pick(&a[1], &a[2], a.get(3).and_then(|s| s.parse().ok()).unwrap_or(20)),
        Some("compare") if a.len() >= 3 => compare(&a[1], &a[2]),
        _ => eprintln!("usage: spotcheck pick <dir> <samples.txt> [per_class] | compare <dir> <result.txt>"),
    }
}
