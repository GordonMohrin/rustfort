//! Save games: the whole `rf_core::World` as bincode, compressed with zstd.
//!
//! File layout (`saves/<fort>-<yyyymmdd-hhmmss>.rfsave`):
//!
//! ```text
//! b"RFSAVE\x01\n"            8 bytes magic + format version
//! u32 LE                     length of the header
//! bincode(SaveHeader)        fort name, game date, ... (read without decompressing the world)
//! zstd(bincode(World))       the rest of the file
//! ```
//!
//! The simulation state (jobs in progress, paths, caches) is not saved: loading rebuilds it
//! with `Sim::new(world, seed)`, like a fresh import. Designations, stockpiles, buildings,
//! work orders and unit needs live in the world and survive.

use rf_core::World;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const MAGIC: &[u8; 8] = b"RFSAVE\x01\n";
pub const EXT: &str = "rfsave";
/// zstd level: 3 is zstd's default (fast, good ratio).
const LEVEL: i32 = 3;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SaveHeader {
    pub fort: String,
    pub fort_native: String,
    /// DF-style game date, e.g. `3rd Opal, 273`.
    pub date: String,
    pub year: i32,
    pub year_tick: i32,
    pub citizens: u32,
    /// Seconds since the Unix epoch (UTC) when the file was written.
    pub saved_unix: u64,
    pub seed: u64,
    /// Where the world originally came from (`export:<dir>` / `synthetic`).
    pub source: String,
}

#[derive(Clone, Debug, Default)]
pub struct SaveInfo {
    pub path: PathBuf,
    pub header: SaveHeader,
    pub bytes: u64,
    /// Seconds spent (serialize + compress + write, or read + decompress + deserialize).
    pub secs: f64,
}

/// Folder for save games: `$RUSTFORT_SAVE_DIR`, else `saves/` in the working directory.
pub fn save_dir() -> PathBuf {
    std::env::var_os("RUSTFORT_SAVE_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("saves"))
}

/// Civil UTC date-time from Unix seconds (Howard Hinnant's days-from-civil inverse).
pub fn utc_parts(unix: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (unix / 86_400) as i64;
    let rem = unix % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d, (rem / 3600) as u32, (rem / 60 % 60) as u32, (rem % 60) as u32)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `2026-10-10 14:32` (UTC).
pub fn fmt_unix(unix: u64) -> String {
    let (y, m, d, h, mi, _) = utc_parts(unix);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02} UTC")
}

/// File-name-safe fort name.
fn slug(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let t = t.trim_matches('_').to_string();
    if t.is_empty() {
        "fort".into()
    } else {
        t
    }
}

pub fn header_for(world: &World, seed: u64) -> SaveHeader {
    SaveHeader {
        fort: world.meta.fort_name.clone(),
        fort_native: world.meta.fort_name_native.clone(),
        date: crate::game::date_string(&world.calendar),
        year: world.calendar.year,
        year_tick: world.calendar.year_tick,
        citizens: world.citizens().count() as u32,
        saved_unix: now_unix(),
        seed,
        source: world.meta.source.clone(),
    }
}

/// Write `world` to a new file in `dir` (created if needed). Returns where and how big.
pub fn save_world(world: &World, seed: u64, dir: &Path) -> Result<SaveInfo, String> {
    save_world_tagged(world, seed, dir, "")
}

/// [round 6] Name tag of autosave files: `<fort>-auto-<yyyymmdd-hhmmss>.rfsave`.
pub const AUTO_TAG: &str = "auto";

/// [round 6] Like [`save_world`], with a tag in the file name (`<fort>-<tag>-<stamp>`).
pub fn save_world_tagged(world: &World, seed: u64, dir: &Path, tag: &str) -> Result<SaveInfo, String> {
    let t0 = Instant::now();
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let header = header_for(world, seed);
    let (y, m, d, h, mi, s) = utc_parts(header.saved_unix);
    let tag = if tag.is_empty() { String::new() } else { format!("{tag}-") };
    let base = format!("{}-{tag}{y:04}{m:02}{d:02}-{h:02}{mi:02}{s:02}", slug(&header.fort));
    let mut path = dir.join(format!("{base}.{EXT}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base}-{n}.{EXT}"));
        n += 1;
    }
    let bytes = write_save(world, &header, &path)?;
    Ok(SaveInfo { path, header, bytes, secs: t0.elapsed().as_secs_f64() })
}

/// Serialize into `path` (via a temp file, so a crash never leaves half a save).
pub fn write_save(world: &World, header: &SaveHeader, path: &Path) -> Result<u64, String> {
    let raw = bincode::serialize(world).map_err(|e| format!("serialize: {e}"))?;
    let hdr = bincode::serialize(header).map_err(|e| format!("serialize header: {e}"))?;
    let tmp = path.with_extension("tmp");
    {
        let f = std::fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        let mut w = std::io::BufWriter::with_capacity(1 << 20, f);
        w.write_all(MAGIC).map_err(|e| e.to_string())?;
        w.write_all(&(hdr.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
        w.write_all(&hdr).map_err(|e| e.to_string())?;
        let mut enc = zstd::stream::Encoder::new(w, LEVEL).map_err(|e| format!("zstd: {e}"))?;
        // Use a few cores for compression (the world is ~50 MB raw for a 192x192x150 fort).
        let threads = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1).clamp(1, 6);
        let _ = enc.multithread(threads);
        enc.write_all(&raw).map_err(|e| format!("write: {e}"))?;
        let mut w = enc.finish().map_err(|e| format!("zstd finish: {e}"))?;
        w.flush().map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("rename to {}: {e}", path.display()))?;
    Ok(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}

fn read_header_from(r: &mut impl Read) -> Result<SaveHeader, String> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic).map_err(|e| format!("read: {e}"))?;
    if &magic != MAGIC {
        return Err("not a Rustfort save (bad magic)".into());
    }
    let mut len = [0u8; 4];
    r.read_exact(&mut len).map_err(|e| format!("read: {e}"))?;
    let len = u32::from_le_bytes(len) as usize;
    if len > 1 << 20 {
        return Err("corrupt save header".into());
    }
    let mut hdr = vec![0u8; len];
    r.read_exact(&mut hdr).map_err(|e| format!("read header: {e}"))?;
    bincode::deserialize(&hdr).map_err(|e| format!("header: {e}"))
}

/// Read only the header (for the load menu).
pub fn read_header(path: &Path) -> Result<SaveHeader, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    read_header_from(&mut f)
}

/// Load a save. The tiletype lookup maps are rebuilt (`reindex`); call `Sim::new` afterwards.
pub fn load_world(path: &Path) -> Result<(World, SaveInfo), String> {
    let t0 = Instant::now();
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut cur = std::io::Cursor::new(&data[..]);
    let header = read_header_from(&mut cur)?;
    let body = &data[cur.position() as usize..];
    let raw = zstd::stream::decode_all(body).map_err(|e| format!("decompress: {e}"))?;
    let mut world: World = bincode::deserialize(&raw).map_err(|e| format!("deserialize: {e}"))?;
    world.tiletypes.reindex();
    let info = SaveInfo { path: path.to_path_buf(), header, bytes: data.len() as u64, secs: t0.elapsed().as_secs_f64() };
    Ok((world, info))
}

/// Saves in `dir`, newest first.
pub fn list_saves(dir: &Path) -> Vec<SaveInfo> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return v };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some(EXT) {
            continue;
        }
        let bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
        let header = read_header(&p).unwrap_or_else(|err| SaveHeader { fort: format!("(unreadable: {err})"), ..Default::default() });
        v.push(SaveInfo { path: p, header, bytes, secs: 0.0 });
    }
    v.sort_by(|a, b| b.header.saved_unix.cmp(&a.header.saved_unix).then(b.path.cmp(&a.path)));
    v
}

/// [round 6] The file is an autosave (`<fort>-auto-...`).
pub fn is_autosave(path: &Path) -> bool {
    path.file_stem().and_then(|f| f.to_str()).map(|f| f.contains(&format!("-{AUTO_TAG}-"))).unwrap_or(false)
}

/// [round 6] Keep only the newest `keep` autosaves of `fort` in `dir` (like DF's rolling
/// autosave backups). Returns the deleted files. Manual saves are never touched.
pub fn rotate_autosaves(dir: &Path, fort: &str, keep: usize) -> Vec<PathBuf> {
    let prefix = format!("{}-{AUTO_TAG}-", slug(fort));
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut autos: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some(EXT))
        .filter(|p| p.file_stem().and_then(|f| f.to_str()).map(|f| f.starts_with(&prefix)).unwrap_or(false))
        .collect();
    // The UTC stamp in the name sorts chronologically; newest first.
    autos.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    let mut gone = Vec::new();
    for p in autos.into_iter().skip(keep.max(1)) {
        if std::fs::remove_file(&p).is_ok() {
            gone.push(p);
        }
    }
    gone
}

/// `12.3 MB`.
pub fn fmt_bytes(n: u64) -> String {
    if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.0} KB", n as f64 / 1024.0)
    }
}

/// `rustfort savebench`: save + load the world and report time and size.
pub fn bench(world: World, seed: u64) {
    let dir = std::env::temp_dir().join("rustfort-savebench");
    let t = Instant::now();
    let raw = bincode::serialize(&world).map(|v| v.len()).unwrap_or(0);
    let ser = t.elapsed().as_secs_f64();
    let info = match save_world(&world, seed, &dir) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("save failed: {e}");
            return;
        }
    };
    println!(
        "save: {} ({} raw bincode, serialize {:.3}s), total {:.3}s -> {}",
        fmt_bytes(info.bytes),
        fmt_bytes(raw as u64),
        ser,
        info.secs,
        info.path.display()
    );
    match load_world(&info.path) {
        Ok((w2, li)) => {
            let t = Instant::now();
            let sim = rf_sim::Sim::new(w2, seed);
            let sim_s = t.elapsed().as_secs_f64();
            println!(
                "load: {:.3}s (read + decompress + deserialize + reindex), Sim::new {:.3}s; {} units, {} items, {} buildings, {} work orders",
                li.secs,
                sim_s,
                sim.world.units.len(),
                sim.world.items.len(),
                sim.world.buildings.len(),
                sim.world.work_orders.len()
            );
            let same = sim.world.tiles == world.tiles && sim.world.items.len() == world.items.len();
            println!("roundtrip tiles identical: {same}");
        }
        Err(e) => eprintln!("load failed: {e}"),
    }
    let _ = std::fs::remove_file(&info.path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_dates() {
        assert_eq!(utc_parts(0), (1970, 1, 1, 0, 0, 0));
        // 2026-10-10 14:32:05 UTC
        assert_eq!(utc_parts(1_791_642_725), (2026, 10, 10, 14, 32, 5));
        assert_eq!(utc_parts(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn roundtrip_synthetic_world() {
        let mut w = World::synthetic(32, 32, 16);
        w.work_orders.push(rf_core::WorkOrder { id: 7, job: "BrewDrink".into(), amount_left: 3, amount_total: 5, frequency: "Monthly".into(), ..Default::default() });
        w.buildings.push(rf_core::Building { id: 99, kind: "FarmPlot".into(), crops: vec!["MUSHROOM_HELMET_PLUMP".into(), String::new(), String::new(), String::new()], ..Default::default() });
        let dir = std::env::temp_dir().join(format!("rustfort-save-test-{}", std::process::id()));
        let info = save_world(&w, 42, &dir).expect("save");
        assert!(info.bytes > 100);
        let h = read_header(&info.path).unwrap();
        assert_eq!(h.fort, "Synthfort");
        assert_eq!(h.seed, 42);
        let (w2, _) = load_world(&info.path).expect("load");
        assert_eq!(w2.tiles, w.tiles);
        assert_eq!(w2.units.len(), w.units.len());
        assert_eq!(w2.work_orders[0].amount_left, 3);
        assert_eq!(w2.buildings.last().unwrap().crops[0], "MUSHROOM_HELMET_PLUMP");
        // Lookup maps are rebuilt after loading.
        assert_eq!(w2.tiletypes.by_name("StoneWall"), w.tiletypes.by_name("StoneWall"));
        assert!(w2.tiletypes.by_name("StoneWall").is_some());
        let list = list_saves(&dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].header.citizens, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_rotation_keeps_the_newest() {
        let dir = std::env::temp_dir().join(format!("rustfort-autosave-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in [
            "Synthfort-auto-20261001-100000",
            "Synthfort-auto-20261002-100000",
            "Synthfort-auto-20261003-100000",
            "Synthfort-auto-20261004-100000",
            "Synthfort-20260901-100000",
            "Other-auto-20260101-000000",
        ] {
            std::fs::write(dir.join(format!("{n}.{EXT}")), b"x").unwrap();
        }
        let gone = rotate_autosaves(&dir, "Synthfort", 3);
        assert_eq!(gone.len(), 1);
        assert!(gone[0].ends_with("Synthfort-auto-20261001-100000.rfsave"));
        assert!(dir.join("Synthfort-20260901-100000.rfsave").exists(), "manual saves stay");
        assert!(dir.join("Other-auto-20260101-000000.rfsave").exists(), "other forts stay");
        assert!(is_autosave(&dir.join("Synthfort-auto-20261004-100000.rfsave")));
        assert!(!is_autosave(&dir.join("Synthfort-20260901-100000.rfsave")));
        // A tagged save carries the tag in its name and takes part in the rotation.
        let w = World::synthetic(16, 16, 10);
        let info = save_world_tagged(&w, 1, &dir, AUTO_TAG).unwrap();
        assert!(is_autosave(&info.path), "{}", info.path.display());
        assert_eq!(rotate_autosaves(&dir, "Synthfort", 3).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
