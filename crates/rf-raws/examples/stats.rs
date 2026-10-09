//! Prints raw object counts per kind for the DF install.
//! Usage: cargo run -p rf-raws --release --example stats [-- --mods]
use rf_raws::{df_dir, GraphicsDb, LoadOptions, Raws};
use std::time::Instant;

fn main() {
    let mods = std::env::args().any(|a| a == "--mods");
    let dir = df_dir();
    let t0 = Instant::now();
    let raws = Raws::load_df_dir(&dir, &LoadOptions { include_mods: mods, ..Default::default() }).expect("load raws");
    let dt = t0.elapsed();
    let t1 = Instant::now();
    let gfx = GraphicsDb::from_raws(&raws);
    let dt_g = t1.elapsed();
    println!("DF dir: {}", dir.display());
    println!(
        "{} files, {} objects, {} tokens parsed in {:.1} ms (graphics index {:.1} ms)",
        raws.files.len(),
        raws.objects.len(),
        raws.total_tokens(),
        dt.as_secs_f64() * 1000.0,
        dt_g.as_secs_f64() * 1000.0
    );
    println!("\nBy [OBJECT:TYPE]:");
    for (k, n) in raws.stats_by_object_type() {
        println!("  {k:24} {n:6}");
    }
    println!("\nBy object kind:");
    for (k, n) in raws.stats() {
        println!("  {k:32} {n:6}");
    }
    println!(
        "\nGraphics: {} tile pages, {} tile graphics, {} palettes, {} descriptor colors",
        gfx.pages.len(),
        gfx.tiles.len(),
        gfx.palettes.len(),
        gfx.colors.len()
    );
    // Note: a plain grep finds 981 `[CREATURE:` and 163 `[REACTION:` tokens, but 14 of the
    // former are creature references inside entity_default.txt and 4 of the latter live in
    // "examples and notes" (not an objects folder, DF does not load it). Distinct objects:
    for (k, want) in [
        ("CREATURE", 967),
        ("INORGANIC", 265),
        ("PLANT", 225),
        ("REACTION", 159),
        ("MATERIAL_TEMPLATE", 70),
        ("TISSUE_TEMPLATE", 38),
    ] {
        let n = raws.count(k);
        println!("  check {k:20} {n:5} (expected {want}) {}", if n == want { "OK" } else { "MISMATCH" });
    }
}
