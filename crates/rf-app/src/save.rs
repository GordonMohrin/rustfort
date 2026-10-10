//! Save games: the whole `rf_core::World` (with the simulation modules' runtime state in
//! `World::sim_state`), compressed with zstd.
//!
//! [saves10] **Format 2** (Rustfort 0.10 and later), `saves/<fort>-<yyyymmdd-hhmmss>.rfsave`:
//!
//! ```text
//! b"RFSAVE\x02\n"            8 bytes magic + format version
//! u32 LE                     length of the header
//! msgpack(SaveHeader)        named fields: fort, date, ..., Rustfort version, body length + CRC-32
//! zstd(msgpack(World))       the body; MessagePack with named fields (struct maps)
//! ```
//!
//! MessagePack with field names is self-describing: a field added to an rf-core type later is
//! simply missing in older files (`#[serde(default)]` fills it), and a field an older Rustfort
//! does not know is skipped. The map tiles are stored as byte columns
//! (`rf_core::world::tile_columns`), so the format costs almost nothing over bincode.
//!
//! Integrity: the header carries the body's length and CRC-32 (checked before decompressing),
//! zstd adds its own content checksum, every save is read back and verified after writing,
//! and overwriting a file keeps the previous one as `<name>.rfsave.bak`. Loading a damaged
//! file falls back to its `.bak` if there is one.
//!
//! **Format 1** (Rustfort 0.9 and older: `bincode(World)`) is still read: the frozen v0.9 types
//! of the `rf-save-v09` crate decode it, then serde converts it field by field to the current
//! `World`. Such a save has no simulation state, like before.
//!
//! The simulation rebuilds its caches (paths, job lists, ...) in `Sim::new`; module state that
//! matters (petitions, visitors, moods, ...) comes back from `World::sim_state`.

use rf_core::World;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Magic of the current save format (2).
pub const MAGIC: &[u8; 8] = b"RFSAVE\x02\n";
/// [saves10] Magic of format 1 (bincode, Rustfort 0.9 and older).
pub const MAGIC_V1: &[u8; 8] = b"RFSAVE\x01\n";
/// [saves10] Current save format.
pub const FORMAT: u32 = 2;
/// [saves10] Rustfort version written into save headers.
pub const RUSTFORT_VERSION: &str = "0.10.0";
pub const EXT: &str = "rfsave";
/// zstd level: 3 is zstd's default (fast, good ratio).
const LEVEL: i32 = 3;
/// [saves10] Largest accepted header (sanity check against garbage).
const MAX_HEADER: usize = 1 << 20;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
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
    /// [saves10] Save format of the file (1 = bincode, 2 = MessagePack).
    pub format: u32,
    /// [saves10] Rustfort version that wrote the file (empty for format 1).
    pub version: String,
    /// [saves10] Length and CRC-32 of the compressed body (format 2).
    pub body_len: u64,
    pub body_crc32: u32,
    /// [saves10] Length of the uncompressed body.
    pub raw_len: u64,
    /// [saves10] Simulation tick count when saved (`Sim::ticks`).
    pub sim_ticks: u64,
}

/// [saves10] Header of format 1 (Rustfort 0.9), bincode; frozen.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SaveHeaderV1 {
    fort: String,
    fort_native: String,
    date: String,
    year: i32,
    year_tick: i32,
    citizens: u32,
    saved_unix: u64,
    seed: u64,
    source: String,
}

impl SaveHeaderV1 {
    fn upgrade(self) -> SaveHeader {
        SaveHeader {
            fort: self.fort,
            fort_native: self.fort_native,
            date: self.date,
            year: self.year,
            year_tick: self.year_tick,
            citizens: self.citizens,
            saved_unix: self.saved_unix,
            seed: self.seed,
            source: self.source,
            format: 1,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SaveInfo {
    pub path: PathBuf,
    pub header: SaveHeader,
    pub bytes: u64,
    /// Seconds spent (serialize + compress + write, or read + decompress + deserialize).
    pub secs: f64,
    /// [saves10] Something the player should know about this load, e.g. "converted from a
    /// v0.9 save" or "the save was damaged; loaded its backup".
    pub note: Option<String>,
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
    let sim_ticks = world.sim_state.get("core").and_then(|s| rf_sim::saved_ticks(s)).unwrap_or(0); // [saves10]
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
        format: FORMAT,
        version: RUSTFORT_VERSION.into(),
        sim_ticks,
        ..Default::default()
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
    Ok(SaveInfo { path, header, bytes, secs: t0.elapsed().as_secs_f64(), note: None })
}

/// [saves10] `<path>.bak`: the previous version of a save that was overwritten.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".bak");
    PathBuf::from(s)
}

/// [saves10] The world as format-2 body bytes (MessagePack, named fields), uncompressed.
pub fn encode_world(world: &World) -> Result<Vec<u8>, String> {
    let mut raw = Vec::with_capacity(world.tiles.len() * 8 + world.items.len() * 400 + (1 << 20));
    rmp_serde::encode::write_named(&mut raw, world).map_err(|e| format!("serialize: {e}"))?;
    Ok(raw)
}

/// [saves10] Inverse of [`encode_world`] (rebuilds the tiletype lookup maps).
pub fn decode_world(raw: &[u8]) -> Result<World, String> {
    let mut world: World = rmp_serde::from_slice(raw).map_err(|e| format!("deserialize: {e}"))?;
    world.tiletypes.reindex();
    Ok(world)
}

fn compress(raw: &[u8]) -> Result<Vec<u8>, String> {
    let mut enc = zstd::stream::Encoder::new(Vec::with_capacity(raw.len() / 4), LEVEL).map_err(|e| format!("zstd: {e}"))?;
    // Use a few cores for compression (the world is ~50 MB raw for a 192x192x150 fort).
    let threads = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1).clamp(1, 6);
    let _ = enc.multithread(threads);
    let _ = enc.include_checksum(true);
    let _ = enc.include_contentsize(true);
    let _ = enc.set_pledged_src_size(Some(raw.len() as u64));
    enc.write_all(raw).map_err(|e| format!("compress: {e}"))?;
    enc.finish().map_err(|e| format!("zstd finish: {e}"))
}

/// Serialize into `path` (via a temp file, so a crash never leaves half a save). The written
/// file is read back and checked; an existing file at `path` is kept as `<path>.bak`.
pub fn write_save(world: &World, header: &SaveHeader, path: &Path) -> Result<u64, String> {
    let raw = encode_world(world)?;
    let body = compress(&raw)?;
    let mut header = header.clone();
    header.format = FORMAT;
    if header.version.is_empty() {
        header.version = RUSTFORT_VERSION.into();
    }
    header.body_len = body.len() as u64;
    header.body_crc32 = crc32fast::hash(&body);
    header.raw_len = raw.len() as u64;
    drop(raw);
    let hdr = rmp_serde::to_vec_named(&header).map_err(|e| format!("serialize header: {e}"))?;
    let tmp = path.with_extension("tmp");
    {
        let f = std::fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        let mut w = std::io::BufWriter::with_capacity(1 << 20, f);
        let res = (|| -> std::io::Result<()> {
            w.write_all(MAGIC)?;
            w.write_all(&(hdr.len() as u32).to_le_bytes())?;
            w.write_all(&hdr)?;
            w.write_all(&body)?;
            w.flush()?;
            w.get_ref().sync_all()
        })();
        if let Err(e) = res {
            drop(w);
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("write {}: {e}", tmp.display()));
        }
    }
    // Read back: a save that cannot be read is worse than none (it would replace the old one).
    if let Err(e) = verify_file(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("the written save does not verify ({e}); nothing was replaced"));
    }
    if path.exists() {
        let bak = backup_path(path);
        std::fs::rename(path, &bak).map_err(|e| format!("keep backup {}: {e}", bak.display()))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("rename to {}: {e}", path.display()))?;
    Ok(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}

/// Parsed file start: header and where the body begins.
fn parse_header(data: &[u8]) -> Result<(SaveHeader, usize), String> {
    if data.len() < 12 {
        return Err("not a Rustfort save (too short)".into());
    }
    let magic = &data[..8];
    let len = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if len > MAX_HEADER || 12 + len > data.len() {
        return Err("corrupt save header".into());
    }
    let hdr = &data[12..12 + len];
    let header = if magic == MAGIC {
        let mut h: SaveHeader = rmp_serde::from_slice(hdr).map_err(|e| format!("header: {e}"))?;
        if h.format == 0 {
            h.format = FORMAT;
        }
        h
    } else if magic == MAGIC_V1 {
        bincode::deserialize::<SaveHeaderV1>(hdr).map_err(|e| format!("header: {e}"))?.upgrade()
    } else if magic.starts_with(b"RFSAVE") {
        return Err(format!("save format {} is newer than this Rustfort ({RUSTFORT_VERSION}) can read", magic[6]));
    } else {
        return Err("not a Rustfort save (bad magic)".into());
    };
    Ok((header, 12 + len))
}

/// Check length and CRC-32 of a format-2 body.
fn check_body(header: &SaveHeader, body: &[u8]) -> Result<(), String> {
    if header.format < 2 {
        return Ok(());
    }
    if body.len() as u64 != header.body_len {
        return Err(format!("save is damaged: {} of {} bytes (truncated?)", body.len(), header.body_len));
    }
    let crc = crc32fast::hash(body);
    if crc != header.body_crc32 {
        return Err(format!("save is damaged: checksum {crc:08x}, expected {:08x}", header.body_crc32));
    }
    Ok(())
}

/// [saves10] Integrity check of a save file without decoding the world (length + CRC-32 of the
/// body; format-1 files only get their header checked).
pub fn verify_file(path: &Path) -> Result<SaveHeader, String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let (header, start) = parse_header(&data)?;
    check_body(&header, &data[start..])?;
    Ok(header)
}

fn read_header_from(r: &mut impl Read) -> Result<SaveHeader, String> {
    let mut start = [0u8; 12];
    r.read_exact(&mut start).map_err(|e| format!("read: {e}"))?;
    let len = u32::from_le_bytes([start[8], start[9], start[10], start[11]]) as usize;
    if len > MAX_HEADER {
        return Err("corrupt save header".into());
    }
    let mut data = start.to_vec();
    data.resize(12 + len, 0);
    r.read_exact(&mut data[12..]).map_err(|e| format!("read header: {e}"))?;
    parse_header(&data).map(|(h, _)| h)
}

/// Read only the header (for the load menu).
pub fn read_header(path: &Path) -> Result<SaveHeader, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    read_header_from(&mut f)
}

/// [saves10] Format 1 (Rustfort 0.9, bincode) body -> current world, through the frozen v0.9
/// types and serde (MessagePack with named fields), so every field that still exists keeps its
/// value and new fields get their defaults.
fn convert_v09(raw: &[u8]) -> Result<World, String> {
    let mut old: rf_save_v09::World = bincode::deserialize(raw).map_err(|e| format!("deserialize v0.9 save: {e}"))?;
    let tiles: Vec<rf_core::Tile> = old
        .tiles
        .iter()
        .map(|t| rf_core::Tile { tt: t.tt, mat: t.mat, flags: t.flags, liquid: t.liquid, desig: t.desig })
        .collect();
    old.tiles = Vec::new();
    let bytes = rmp_serde::to_vec_named(&old).map_err(|e| format!("convert v0.9 save: {e}"))?;
    drop(old);
    let mut world: World = rmp_serde::from_slice(&bytes).map_err(|e| format!("convert v0.9 save: {e}"))?;
    world.tiles = tiles;
    Ok(world)
}

/// Decode a whole save file (no fallback).
fn load_bytes(data: &[u8]) -> Result<(World, SaveHeader, Option<String>), String> {
    let (header, start) = parse_header(data)?;
    let body = &data[start..];
    check_body(&header, body)?;
    if header.format >= 2 {
        let cap = (header.raw_len as usize).clamp(1 << 16, 1 << 32);
        let raw = zstd::bulk::decompress(body, cap)
            .or_else(|_| zstd::stream::decode_all(body))
            .map_err(|e| format!("decompress: {e}"))?;
        let world = decode_world(&raw)?;
        Ok((world, header, None))
    } else {
        let raw = zstd::stream::decode_all(body).map_err(|e| format!("decompress: {e}"))?;
        let mut world = convert_v09(&raw)?;
        world.tiletypes.reindex();
        Ok((world, header, Some("converted from a Rustfort 0.9 save (no simulation state); the next save uses the new format".into())))
    }
}

/// Load a save. The tiletype lookup maps are rebuilt (`reindex`); call `Sim::new` afterwards.
/// [saves10] Reads formats 1 and 2; a damaged file falls back to its `.bak` (see `SaveInfo::note`).
pub fn load_world(path: &Path) -> Result<(World, SaveInfo), String> {
    let t0 = Instant::now();
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    match load_bytes(&data) {
        Ok((world, header, note)) => {
            let info = SaveInfo { path: path.to_path_buf(), header, bytes: data.len() as u64, secs: t0.elapsed().as_secs_f64(), note };
            Ok((world, info))
        }
        Err(e) => {
            let bak = backup_path(path);
            let Ok(bdata) = std::fs::read(&bak) else { return Err(e) };
            match load_bytes(&bdata) {
                Ok((world, header, note)) => {
                    let mut text = format!("{e}; loaded the backup {} instead", bak.display());
                    if let Some(n) = note {
                        text.push_str(&format!(" ({n})"));
                    }
                    let info = SaveInfo { path: bak, header, bytes: bdata.len() as u64, secs: t0.elapsed().as_secs_f64(), note: Some(text) };
                    Ok((world, info))
                }
                Err(e2) => Err(format!("{e} (backup: {e2})")),
            }
        }
    }
}

/// Saves in `dir`, newest first.
/// [saves10] Truncated format-2 files are listed as damaged (and `saved_unix` 0, so
/// "Continue" skips them).
pub fn list_saves(dir: &Path) -> Vec<SaveInfo> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return v };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some(EXT) {
            continue;
        }
        let bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
        let header = match read_header(&p) {
            Ok(h) => {
                let hdr_len = std::fs::File::open(&p)
                    .ok()
                    .and_then(|mut f| {
                        let mut s = [0u8; 12];
                        f.read_exact(&mut s).ok()?;
                        Some(12 + u32::from_le_bytes([s[8], s[9], s[10], s[11]]) as u64)
                    })
                    .unwrap_or(0);
                if h.format >= 2 && hdr_len + h.body_len != bytes {
                    SaveHeader { fort: format!("(damaged: {} of {} bytes) {}", bytes, hdr_len + h.body_len, h.fort), ..Default::default() }
                } else {
                    h
                }
            }
            Err(err) => SaveHeader { fort: format!("(unreadable: {err})"), ..Default::default() },
        };
        v.push(SaveInfo { path: p, header, bytes, secs: 0.0, note: None });
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
    // `<stamp>[-N]`: the UTC stamp sorts chronologically, `-N` counts saves within one second.
    let key = |p: &PathBuf| {
        let stem = p.file_stem().and_then(|f| f.to_str()).unwrap_or("");
        let rest = stem.strip_prefix(&prefix).unwrap_or(stem);
        let mut parts = rest.split('-');
        let date = parts.next().unwrap_or("").to_string();
        let time = parts.next().unwrap_or("").to_string();
        let n: u32 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(1);
        (date, time, n)
    };
    autos.sort_by_key(|p| std::cmp::Reverse(key(p)));
    let mut gone = Vec::new();
    for p in autos.into_iter().skip(keep.max(1)) {
        if std::fs::remove_file(&p).is_ok() {
            let _ = std::fs::remove_file(backup_path(&p)); // [saves10]
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

/// [saves10] The world in the frozen v0.9 types (field by field through MessagePack), as
/// Rustfort 0.9 held it in memory; for converter checks (`savebench`, tests).
pub fn to_v09(w: &World) -> Result<rf_save_v09::World, String> {
    fn tc<A: Serialize, B: serde::de::DeserializeOwned>(a: &A) -> Result<B, String> {
        rmp_serde::from_slice(&rmp_serde::to_vec_named(a).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
    Ok(rf_save_v09::World {
        meta: tc(&w.meta)?,
        calendar: tc(&w.calendar)?,
        dims: tc(&w.dims)?,
        tiles: w.tiles.iter().map(|t| rf_save_v09::Tile { tt: t.tt, mat: t.mat, flags: t.flags, liquid: t.liquid, desig: t.desig }).collect(),
        tiletypes: tc(&w.tiletypes)?,
        materials: tc(&w.materials)?,
        units: tc(&w.units)?,
        items: tc(&w.items)?,
        buildings: tc(&w.buildings)?,
        plants: tc(&w.plants)?,
        work_orders: tc(&w.work_orders)?,
        plant_raws: tc(&w.plant_raws)?,
        creature_raws: tc(&w.creature_raws)?,
        weapon_raws: tc(&w.weapon_raws)?,
        armor_raws: tc(&w.armor_raws)?,
        squads: tc(&w.squads)?,
        burrows: tc(&w.burrows)?,
        entities: tc(&w.entities)?,
        nobles: tc(&w.nobles)?,
        settings: tc(&w.settings)?,
        hist_figs: tc(&w.hist_figs)?,
        caravans: tc(&w.caravans)?,
        engravings: tc(&w.engravings)?,
        constructions: tc(&w.constructions)?,
        carve_designations: tc(&w.carve_designations)?,
        work_details: tc(&w.work_details)?,
        kitchen: tc(&w.kitchen)?,
        locations: tc(&w.locations)?,
        crimes: tc(&w.crimes)?,
        haul_routes: tc(&w.haul_routes)?,
        stockpile_options: tc(&w.stockpile_options)?,
        fort_history: tc(&w.fort_history)?,
    })
}

/// [saves10] Write `world` as a Rustfort 0.9 (format 1) file, for converter checks.
pub fn write_v09_file(world: &World, seed: u64, path: &Path) -> Result<(), String> {
    let old = to_v09(world)?;
    let raw = bincode::serialize(&old).map_err(|e| e.to_string())?;
    let h = header_for(world, seed);
    let hdr = SaveHeaderV1 { fort: h.fort, fort_native: h.fort_native, date: h.date, year: h.year, year_tick: h.year_tick, citizens: h.citizens, saved_unix: h.saved_unix, seed, source: h.source };
    let hdr = bincode::serialize(&hdr).map_err(|e| e.to_string())?;
    let mut file = MAGIC_V1.to_vec();
    file.extend_from_slice(&(hdr.len() as u32).to_le_bytes());
    file.extend_from_slice(&hdr);
    file.extend_from_slice(&zstd::stream::encode_all(&raw[..], LEVEL).map_err(|e| e.to_string())?);
    std::fs::write(path, file).map_err(|e| e.to_string())
}

/// `rustfort savebench`: save + load the world and report time and size.
/// [saves10] Runs the sim for `RUSTFORT_SAVEBENCH_TICKS` (default 2000) ticks first so the
/// module state is not empty, and compares with the old bincode format.
pub fn bench(world: World, seed: u64) {
    let dir = std::env::temp_dir().join("rustfort-savebench");
    let ticks: u32 = std::env::var("RUSTFORT_SAVEBENCH_TICKS").ok().and_then(|s| s.parse().ok()).unwrap_or(2000);
    let mut sim = rf_sim::Sim::new(world, seed);
    sim.run_ticks(ticks);
    let t = Instant::now();
    let world = sim.world_for_save();
    let snap = t.elapsed().as_secs_f64();
    let t = Instant::now();
    // (Current types; tiles are columns here too, so this is only a size/speed reference.)
    let raw_v1 = bincode::serialize(&world).map(|v| v.len()).unwrap_or(0);
    let ser_v1 = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let raw = encode_world(&world).map(|v| v.len()).unwrap_or(0);
    let ser = t.elapsed().as_secs_f64();
    let state: usize = world.sim_state.values().map(|s| s.len()).sum();
    println!(
        "world_for_save {:.3}s; sim_state {} modules, {} bytes JSON; raw msgpack {} (encode {:.3}s), old bincode {} ({:.3}s)",
        snap,
        world.sim_state.len(),
        state,
        fmt_bytes(raw as u64),
        ser,
        fmt_bytes(raw_v1 as u64),
        ser_v1
    );
    for (k, v) in &world.sim_state {
        println!("  sim_state[{k}]: {} bytes", v.len());
    }
    let info = match save_world(&world, seed, &dir) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("save failed: {e}");
            return;
        }
    };
    println!("save: {} total {:.3}s (incl. read-back verify) -> {}", fmt_bytes(info.bytes), info.secs, info.path.display());
    let t = Instant::now();
    let v = verify_file(&info.path);
    println!("verify: {:?} in {:.3}s", v.as_ref().map(|h| h.body_crc32), t.elapsed().as_secs_f64());
    match load_world(&info.path) {
        Ok((w2, li)) => {
            let lossless = encode_world(&w2).ok() == encode_world(&world).ok();
            println!("lossless (re-encoding the loaded world gives the same bytes): {lossless}");
            let t = Instant::now();
            let sim2 = rf_sim::Sim::new(w2, seed);
            let sim_s = t.elapsed().as_secs_f64();
            println!(
                "load: {:.3}s (read + check + decompress + deserialize + reindex), Sim::new {:.3}s; {} units, {} items, {} buildings, {} work orders",
                li.secs,
                sim_s,
                sim2.world.units.len(),
                sim2.world.items.len(),
                sim2.world.buildings.len(),
                sim2.world.work_orders.len()
            );
            let same = sim2.world.tiles == world.tiles && sim2.world.items.len() == world.items.len();
            println!("roundtrip tiles identical: {same}; ticks {} -> {}; state hash {:016x} -> {:016x}", sim.ticks(), sim2.ticks(), sim.state_hash(), sim2.state_hash());
            // [saves11] Exact snapshot: the loaded game continues bit-identically.
            let mut sim2 = sim2;
            sim.run_ticks(2000);
            sim2.run_ticks(2000);
            println!(
                "restore: {}; snapshot {} raw; +2000 ticks: state hash {:016x} vs {:016x} ({})",
                sim2.restore_mode(),
                fmt_bytes(world.sim_exact.len() as u64),
                sim.state_hash(),
                sim2.state_hash(),
                if sim.state_hash() == sim2.state_hash() { "identical" } else { "DIFFERENT" }
            );
        }
        Err(e) => eprintln!("load failed: {e}"),
    }
    let _ = std::fs::remove_file(&info.path);
    // [saves10] A Rustfort 0.9 save of this world converts back to the same world.
    let p = dir.join("v09-check.rfsave");
    let mut plain = world.clone();
    plain.sim_state.clear();
    plain.sim_exact.clear(); // [saves11] v0.9 has no snapshot either
    match write_v09_file(&plain, seed, &p) {
        Ok(()) => {
            let bytes = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            match load_world(&p) {
                Ok((w1, li)) => println!(
                    "v0.9 save: {} file, load + convert {:.3}s, identical to the original: {}",
                    fmt_bytes(bytes),
                    li.secs,
                    encode_world(&w1).ok() == encode_world(&plain).ok()
                ),
                Err(e) => eprintln!("v0.9 save did not load: {e}"),
            }
        }
        Err(e) => eprintln!("writing a v0.9 save failed: {e}"),
    }
    let _ = std::fs::remove_file(&p);
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

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustfort-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn roundtrip_synthetic_world() {
        let mut w = World::synthetic(32, 32, 16);
        w.work_orders.push(rf_core::WorkOrder { id: 7, job: "BrewDrink".into(), amount_left: 3, amount_total: 5, frequency: "Monthly".into(), ..Default::default() });
        w.buildings.push(rf_core::Building { id: 99, kind: "FarmPlot".into(), crops: vec!["MUSHROOM_HELMET_PLUMP".into(), String::new(), String::new(), String::new()], ..Default::default() });
        w.sim_state.insert("test".into(), "{\"a\":1}".into());
        let dir = test_dir("save-test");
        let info = save_world(&w, 42, &dir).expect("save");
        assert!(info.bytes > 100);
        let h = read_header(&info.path).unwrap();
        assert_eq!(h.fort, "Synthfort");
        assert_eq!(h.seed, 42);
        assert_eq!(h.format, FORMAT);
        assert_eq!(h.version, RUSTFORT_VERSION);
        assert!(verify_file(&info.path).is_ok());
        let (w2, li) = load_world(&info.path).expect("load");
        assert!(li.note.is_none());
        assert_eq!(w2.tiles, w.tiles);
        assert_eq!(w2.units.len(), w.units.len());
        assert_eq!(w2.work_orders[0].amount_left, 3);
        assert_eq!(w2.buildings.last().unwrap().crops[0], "MUSHROOM_HELMET_PLUMP");
        assert_eq!(w2.sim_state.get("test").map(|s| s.as_str()), Some("{\"a\":1}"));
        // Lookup maps are rebuilt after loading.
        assert_eq!(w2.tiletypes.by_name("StoneWall"), w.tiletypes.by_name("StoneWall"));
        assert!(w2.tiletypes.by_name("StoneWall").is_some());
        let list = list_saves(&dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].header.citizens, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// [saves10] A v0.9 (format 1, bincode) save still loads and converts.
    #[test]
    fn loads_v09_bincode_saves() {
        let w = World::synthetic(24, 20, 12);
        // The frozen v0.9 types build the same synthetic fort; write it the way Rustfort 0.9 did.
        let old = rf_save_v09::World::synthetic(24, 20, 12);
        let raw = bincode::serialize(&old).unwrap();
        let hdr = SaveHeaderV1 { fort: "Synthfort".into(), seed: 9, saved_unix: 5, ..Default::default() };
        let hdr = bincode::serialize(&hdr).unwrap();
        let mut file = MAGIC_V1.to_vec();
        file.extend_from_slice(&(hdr.len() as u32).to_le_bytes());
        file.extend_from_slice(&hdr);
        file.extend_from_slice(&zstd::stream::encode_all(&raw[..], 3).unwrap());
        let dir = test_dir("save-v09");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Synthfort-20261010-120000.rfsave");
        std::fs::write(&p, &file).unwrap();
        assert_eq!(read_header(&p).unwrap().format, 1);
        let (w2, info) = load_world(&p).expect("load v0.9");
        assert!(info.note.as_deref().unwrap_or("").contains("0.9"));
        assert_eq!(info.header.seed, 9);
        assert_eq!(w2.tiles, w.tiles);
        assert_eq!(w2.units.len(), w.units.len());
        assert_eq!(w2.units[0].name, w.units[0].name);
        assert_eq!(w2.items.len(), w.items.len());
        assert_eq!(w2.plants.len(), w.plants.len());
        assert!(w2.tiletypes.by_name("StoneWall").is_some());
        // Saving it again writes format 2.
        let info2 = save_world(&w2, 9, &dir).unwrap();
        assert_eq!(read_header(&info2.path).unwrap().format, FORMAT);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// [saves10] Damaged files are detected; overwriting keeps a `.bak` that loading falls back to.
    #[test]
    fn checksum_and_backup() {
        let dir = test_dir("save-bak");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Synthfort-x.rfsave");
        let mut w = World::synthetic(16, 16, 10);
        let h = header_for(&w, 1);
        write_save(&w, &h, &p).unwrap();
        w.calendar.year = 251;
        write_save(&w, &header_for(&w, 1), &p).unwrap();
        assert!(backup_path(&p).exists(), "overwrite keeps the previous save");
        assert_eq!(load_world(&p).unwrap().0.calendar.year, 251);
        // Flip a byte in the body: the checksum catches it and the backup is loaded.
        let mut data = std::fs::read(&p).unwrap();
        let n = data.len();
        data[n - 10] ^= 0x55;
        std::fs::write(&p, &data).unwrap();
        assert!(verify_file(&p).is_err());
        let (w2, info) = load_world(&p).unwrap();
        assert_eq!(w2.calendar.year, 250, "backup has the older year");
        assert!(info.note.unwrap().contains("backup"));
        // Truncated: listed as damaged, so "Continue" skips it.
        std::fs::write(&p, &data[..n / 2]).unwrap();
        let l = list_saves(&dir);
        assert!(l[0].header.fort.contains("damaged") || l[1].header.fort.contains("damaged"));
        assert!(l.iter().all(|s| !s.path.ends_with("Synthfort-x.rfsave") || s.header.saved_unix == 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// [saves10] Format 2 tolerates fields it does not know (a newer Rustfort's save) and fields
    /// that are missing (an older one's).
    #[test]
    fn tolerates_added_and_missing_fields() {
        use serde::Serialize;
        #[derive(Serialize)]
        struct OlderUnit {
            id: i32,
            name: String,
            future_field: Vec<u32>,
        }
        let u: rf_core::Unit = rmp_serde::from_slice(&rmp_serde::to_vec_named(&OlderUnit { id: 5, name: "Urist".into(), future_field: vec![1, 2] }).unwrap()).unwrap();
        assert_eq!(u.id, 5);
        assert_eq!(u.name, "Urist");
        assert_eq!(u.squad_id, rf_core::Unit::default().squad_id);
        // A world with only a few fields.
        #[derive(Serialize)]
        struct TinyWorld {
            calendar: rf_core::Calendar,
            extra: String,
        }
        let w = decode_world(&rmp_serde::to_vec_named(&TinyWorld { calendar: rf_core::Calendar { year: 300, year_tick: 7 }, extra: "x".into() }).unwrap()).unwrap();
        assert_eq!(w.calendar.year, 300);
        assert!(w.tiles.is_empty());
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
        // Several autosaves within one second: `-2`, `-3` are newer than the plain name.
        for n in ["Synthfort-auto-20991231-235959", "Synthfort-auto-20991231-235959-2", "Synthfort-auto-20991231-235959-3"] {
            std::fs::write(dir.join(format!("{n}.{EXT}")), b"x").unwrap();
        }
        rotate_autosaves(&dir, "Synthfort", 2);
        assert!(dir.join("Synthfort-auto-20991231-235959-3.rfsave").exists());
        assert!(dir.join("Synthfort-auto-20991231-235959-2.rfsave").exists());
        assert!(!dir.join("Synthfort-auto-20991231-235959.rfsave").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
