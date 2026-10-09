//! rf-import: loads a DFHack fort export (docs/EXPORT_FORMAT.md) into an rf-core World.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//!
//! The loader never panics on bad data: malformed JSON lines are skipped, out-of-range
//! material or tiletype ids are replaced, missing z-files become hidden void, and every such
//! repair is listed in [`ImportReport::warnings`]. Only a missing/unreadable `manifest.json`
//! or `tiletypes.json` is a hard error.

use rf_core::{
    tile_flags, Building, Calendar, Dims, Item, MaterialInfo, Meta, Plant, Shape, Tile, TiletypeInfo, TiletypeTable,
    TtMaterial, TtSpecial, Unit, World, NO_MAT,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Default export folder of Gordon's fort (gitignored, outside of git).
pub const DEFAULT_EXPORT: &str = "export/tileshtulon";

/// Export format version this importer understands.
pub const FORMAT_VERSION: u32 = 1;

/// Size of one map record in `map/zNNN.bin` (layout of [`rf_core::Tile`]).
pub const TILE_RECORD_BYTES: usize = 8;

/// At most this many individual warnings per category; the rest is summarized.
const MAX_WARNINGS_PER_KIND: usize = 5;

#[derive(Debug)]
pub struct ImportError(pub String);

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ImportError {}

/// What was imported and what was approximated or dropped.
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub units: usize,
    pub citizens: usize,
    pub items: usize,
    pub buildings: usize,
    pub plants: usize,
    pub tiles: usize,
    /// Round-2 data (0 when the export predates it).
    pub work_orders: usize,
    pub plant_raws: usize,
    /// Jobs in workshop/furnace queues (sum of `Building::jobs`).
    pub building_jobs: usize,
    pub warnings: Vec<String>,
    pub seconds: f64,
}

/// One mod of `manifest.json` `mods_detail` (display data; `Meta::mods` holds `id:numeric_version`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    /// Displayed version, e.g. `53.16`.
    pub version: String,
    pub numeric_version: i32,
}

/// A burrow from `burrows.json` (optional file; not part of rf-core yet).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BurrowInfo {
    pub id: i32,
    pub name: String,
    /// Number of tiles assigned to the burrow.
    pub tiles: i32,
    /// Number of 16x16 map blocks touched.
    pub blocks: i32,
    /// Unit ids assigned to the burrow.
    pub units: Vec<i32>,
}

/// A squad of the fort from `squads.json` (optional file; not part of rf-core yet).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SquadInfo {
    pub id: i32,
    /// Alias if set, else the translated squad name.
    pub name: String,
    /// Unit ids of the members (-1 if the historical figure has no unit).
    pub members: Vec<i32>,
    /// Historical figure ids of the members (same order).
    pub member_hfs: Vec<i32>,
    /// Number of positions (filled or not).
    pub positions: i32,
}

/// Optional extra data of an export that has no place in `rf_core::World` yet.
#[derive(Clone, Debug, Default)]
pub struct Extras {
    pub burrows: Vec<BurrowInfo>,
    pub squads: Vec<SquadInfo>,
    pub mods: Vec<ModInfo>,
}

/// Read `burrows.json`, `squads.json` and the manifest's `mods_detail` (all optional; missing
/// or broken files give empty lists).
pub fn load_extras(dir: &Path) -> Extras {
    fn opt<T: DeserializeOwned + Default>(p: &Path) -> T {
        std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(strip_bom(&t)).ok()).unwrap_or_default()
    }
    Extras {
        burrows: opt(&dir.join("burrows.json")),
        squads: opt(&dir.join("squads.json")),
        mods: load_manifest(dir).map(|m| m.mods_detail).unwrap_or_default(),
    }
}

/// `manifest.json` of an export (see docs/EXPORT_FORMAT.md). Unknown fields are ignored,
/// missing ones default.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub format_version: u32,
    pub exporter_rev: String,
    pub df_version: String,
    pub dfhack_version: String,
    pub world_name: String,
    pub world_name_english: String,
    pub fort_name: String,
    pub fort_name_native: String,
    pub site_id: i32,
    pub civ_id: i32,
    pub group_id: i32,
    pub region_x: i32,
    pub region_y: i32,
    pub region_z: i32,
    pub dims: Dims,
    /// Calendar when the export started (the map is exported first).
    pub year: i32,
    pub year_tick_start: i32,
    /// Calendar when the export finished (entities are exported after the map).
    pub year_end: i32,
    pub year_tick_end: i32,
    /// `units`, `citizens`, `items`, `buildings`, `plants`, `materials`, `tiletypes`, `tiles`, `z_exported`.
    pub counts: BTreeMap<String, i64>,
    pub export_seconds: f64,
    pub lua_seconds: f64,
    pub frames: u32,
    pub max_slice_ms: u32,
    pub budget_ms: u32,
    /// False if the export was made with `--skip-map` / `--zmin` / `--zmax`.
    pub map_complete: bool,
    /// Approximations and skipped data reported by the exporter.
    pub notes: Vec<String>,
    /// Active mods of the save in load order (`id:numeric_version`).
    pub mods: Vec<String>,
    /// The same mods with display name and version.
    pub mods_detail: Vec<ModInfo>,
}

impl Manifest {
    pub fn count(&self, key: &str) -> Option<i64> {
        self.counts.get(key).copied()
    }
}

/// Find the export folder: explicit path, env `RUSTFORT_EXPORT`, or `export/tileshtulon`
/// relative to the current dir or the workspace root.
pub fn find_export(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return p.join("manifest.json").exists().then(|| p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("RUSTFORT_EXPORT").map(PathBuf::from) {
        if p.join("manifest.json").exists() {
            return Some(p);
        }
    }
    let ws = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(DEFAULT_EXPORT);
    [PathBuf::from(DEFAULT_EXPORT), ws].into_iter().find(|p| p.join("manifest.json").exists())
}

/// Read only `manifest.json` (cheap; e.g. to show what an export contains before loading it).
pub fn load_manifest(dir: &Path) -> Result<Manifest, ImportError> {
    let path = dir.join("manifest.json");
    let text = std::fs::read_to_string(&path).map_err(|e| ImportError(format!("{}: {e}", path.display())))?;
    serde_json::from_str(strip_bom(&text)).map_err(|e| ImportError(format!("{}: {e}", path.display())))
}

/// Collects warnings, capped per category so a broken file cannot produce millions of lines.
#[derive(Default)]
struct Warnings {
    list: Vec<String>,
    per_kind: BTreeMap<&'static str, usize>,
}

impl Warnings {
    fn add(&mut self, kind: &'static str, msg: impl Into<String>) {
        let n = self.per_kind.entry(kind).or_insert(0);
        *n += 1;
        if *n <= MAX_WARNINGS_PER_KIND {
            self.list.push(format!("{kind}: {}", msg.into()));
        }
    }
    fn merge(&mut self, other: Warnings) {
        for (k, n) in other.per_kind {
            *self.per_kind.entry(k).or_insert(0) += n;
        }
        self.list.extend(other.list);
    }
    fn finish(mut self) -> Vec<String> {
        for (k, n) in &self.per_kind {
            if *n > MAX_WARNINGS_PER_KIND {
                self.list.push(format!("{k}: {n} occurrences in total ({} shown)", MAX_WARNINGS_PER_KIND));
            }
        }
        self.list
    }
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// Parse a JSON-lines file; bad lines are skipped with a warning. A missing file yields an
/// empty list plus a warning.
fn read_jsonl<T: DeserializeOwned>(path: &Path, kind: &'static str) -> (Vec<T>, Warnings) {
    let mut w = Warnings::default();
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            w.add(kind, format!("{} not readable ({e}); imported none", path.display()));
            return (Vec::new(), w);
        }
    };
    let mut out = Vec::with_capacity(text.len() / 256);
    for (lineno, line) in strip_bom(&text).lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(v) => out.push(v),
            Err(e) => w.add(kind, format!("{}:{}: skipped bad line ({e})", path.display(), lineno + 1)),
        }
    }
    (out, w)
}

/// `tiletypes.json` entry with the enum fields as plain strings. DF's names (e.g. `WORN_1`) are
/// mapped with rf-core's `from_df`; serde names of the rf-core enums are accepted too, and an
/// unknown name becomes `None` with a warning instead of failing the whole import.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawTiletype {
    id: u16,
    name: String,
    shape: String,
    material: String,
    special: String,
    variant: u8,
    direction: String,
}

fn enum_or<T: DeserializeOwned + Default + PartialEq>(s: &str, from_df: fn(&str) -> T, bad: &mut Vec<String>) -> T {
    let v = from_df(s);
    if v != T::default() || s == "NONE" || s.is_empty() {
        return v;
    }
    match serde_json::from_value::<T>(serde_json::Value::String(s.to_string())) {
        Ok(v) => v,
        Err(_) => {
            if !bad.iter().any(|b| b == s) {
                bad.push(s.to_string());
            }
            T::default()
        }
    }
}

fn load_tiletypes(path: &Path) -> Result<(Vec<TiletypeInfo>, Warnings), ImportError> {
    let (raw, mut w) = read_json::<Vec<RawTiletype>>(path, "tiletypes", true)?;
    let mut bad = Vec::new();
    let infos = raw
        .into_iter()
        .map(|r| TiletypeInfo {
            id: r.id,
            shape: enum_or::<Shape>(&r.shape, Shape::from_df, &mut bad),
            material: enum_or::<TtMaterial>(&r.material, TtMaterial::from_df, &mut bad),
            special: enum_or::<TtSpecial>(&r.special, TtSpecial::from_df, &mut bad),
            name: r.name,
            variant: r.variant,
            direction: r.direction,
        })
        .collect();
    if !bad.is_empty() {
        w.add("tiletypes", format!("unknown enum names mapped to NONE: {}", bad.join(", ")));
    }
    Ok((infos, w))
}

/// Result of a loader thread; a panic becomes a warning and an empty list.
fn joined<T>(r: std::thread::Result<(Vec<T>, Warnings)>, kind: &'static str) -> (Vec<T>, Warnings) {
    r.unwrap_or_else(|_| {
        let mut w = Warnings::default();
        w.add(kind, "loader thread panicked; imported none");
        (Vec::new(), w)
    })
}

fn read_json<T: DeserializeOwned + Default>(path: &Path, kind: &'static str, required: bool) -> Result<(T, Warnings), ImportError> {
    let mut w = Warnings::default();
    match std::fs::read_to_string(path) {
        Ok(t) => match serde_json::from_str::<T>(strip_bom(&t)) {
            Ok(v) => Ok((v, w)),
            Err(e) if required => Err(ImportError(format!("{}: {e}", path.display()))),
            Err(e) => {
                w.add(kind, format!("{}: {e}; using defaults", path.display()));
                Ok((T::default(), w))
            }
        },
        Err(e) if required => Err(ImportError(format!("{}: {e}", path.display()))),
        Err(e) => {
            w.add(kind, format!("{} not readable ({e}); using defaults", path.display()));
            Ok((T::default(), w))
        }
    }
}

/// The tile used for map areas that are missing in the export.
fn void_tile() -> Tile {
    Tile { tt: 0, mat: NO_MAT, flags: tile_flags::HIDDEN, liquid: 0, desig: 0 }
}

/// Decode one z-level file into `out` (exactly dims.x*dims.y tiles). Validates ids.
fn decode_level(bytes: &[u8], out: &mut [Tile], n_tt: usize, n_mat: usize, bad_tt: &mut usize, bad_mat: &mut usize) {
    for (t, r) in out.iter_mut().zip(bytes.chunks_exact(TILE_RECORD_BYTES)) {
        let mut tile = Tile {
            tt: u16::from_le_bytes([r[0], r[1]]),
            mat: u16::from_le_bytes([r[2], r[3]]),
            flags: u16::from_le_bytes([r[4], r[5]]),
            liquid: r[6],
            desig: r[7],
        };
        if tile.tt as usize >= n_tt {
            *bad_tt += 1;
            tile.tt = 0;
        }
        if tile.mat != NO_MAT && tile.mat as usize >= n_mat {
            *bad_mat += 1;
            tile.mat = NO_MAT;
        }
        // DIRTY is internal to Rustfort and never comes from DF.
        tile.flags &= !tile_flags::DIRTY;
        *t = tile;
    }
}

/// Load all `map/zNNN.bin` files into one tile vector, several z-levels per thread.
fn load_map(dir: &Path, dims: Dims, n_tt: usize, n_mat: usize) -> (Vec<Tile>, Warnings) {
    let plane = dims.x as usize * dims.y as usize;
    let nz = dims.z as usize;
    let mut tiles = vec![void_tile(); plane * nz];
    let mut w = Warnings::default();
    if plane == 0 || nz == 0 {
        w.add("map", "manifest dims are empty; no tiles imported");
        return (tiles, w);
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 12);
    let per = nz.div_ceil(threads);
    let results: Vec<Warnings> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for (ci, chunk) in tiles.chunks_mut(plane * per).enumerate() {
            let z0 = ci * per;
            handles.push(s.spawn(move || {
                let mut w = Warnings::default();
                let (mut bad_tt, mut bad_mat) = (0usize, 0usize);
                for (k, level) in chunk.chunks_mut(plane).enumerate() {
                    let z = z0 + k;
                    let path = dir.join("map").join(format!("z{z:03}.bin"));
                    match std::fs::read(&path) {
                        Ok(bytes) if bytes.len() == plane * TILE_RECORD_BYTES => {
                            decode_level(&bytes, level, n_tt, n_mat, &mut bad_tt, &mut bad_mat)
                        }
                        Ok(bytes) => w.add(
                            "map",
                            format!(
                                "{}: {} bytes, expected {} ({}x{}x{TILE_RECORD_BYTES}); z-level left as hidden void",
                                path.display(),
                                bytes.len(),
                                plane * TILE_RECORD_BYTES,
                                dims.x,
                                dims.y
                            ),
                        ),
                        Err(e) => w.add("map", format!("{} missing ({e}); z-level left as hidden void", path.display())),
                    }
                }
                if bad_tt > 0 {
                    w.add("map", format!("{bad_tt} tiles with unknown tiletype id (set to 0) in z {z0}.."));
                }
                if bad_mat > 0 {
                    w.add("map", format!("{bad_mat} tiles with material index out of palette range (set to none) in z {z0}.."));
                }
                w
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    for r in results {
        w.merge(r);
    }
    (tiles, w)
}

/// Load an export folder written by `tools/dfhack/rustfort-export.lua`.
pub fn load_export(dir: &Path) -> Result<(World, ImportReport), ImportError> {
    let t0 = Instant::now();
    let manifest = load_manifest(dir)?;
    let mut w = Warnings::default();
    if manifest.format_version != FORMAT_VERSION {
        w.add(
            "manifest",
            format!("format_version {} (importer knows {FORMAT_VERSION}); trying anyway", manifest.format_version),
        );
    }
    if !manifest.map_complete {
        w.add("manifest", "export was made without the full map (--skip-map/--zmin/--zmax)");
    }
    let (infos, tw) = load_tiletypes(&dir.join("tiletypes.json"))?;
    w.merge(tw);
    if infos.is_empty() {
        return Err(ImportError(format!("{}: no tiletypes", dir.join("tiletypes.json").display())));
    }
    let n_tt = infos.iter().map(|i| i.id as usize + 1).max().unwrap_or(0);
    let tiletypes = TiletypeTable::new(infos);
    let (materials, mw) = read_json::<Vec<MaterialInfo>>(&dir.join("materials.json"), "materials", false)?;
    w.merge(mw);
    let n_mat = materials.len();
    let dims = manifest.dims;

    // Map and entity files are independent: load them in parallel.
    let ((tiles, mapw), (units, uw), (items, iw), (buildings, bw), (plants, pw)) = std::thread::scope(|s| {
        let map = s.spawn(|| load_map(dir, dims, n_tt, n_mat));
        let items = s.spawn(|| read_jsonl::<Item>(&dir.join("items.jsonl"), "items"));
        let plants = s.spawn(|| read_jsonl::<Plant>(&dir.join("plants.jsonl"), "plants"));
        let buildings = s.spawn(|| read_jsonl::<Building>(&dir.join("buildings.jsonl"), "buildings"));
        let units = read_jsonl::<Unit>(&dir.join("units.jsonl"), "units");
        let map = map.join().unwrap_or_else(|_| {
            let mut w = Warnings::default();
            w.add("map", "map loader thread panicked; map is void");
            (vec![void_tile(); dims.x as usize * dims.y as usize * dims.z as usize], w)
        });
        (map, units, joined(items.join(), "items"), joined(buildings.join(), "buildings"), joined(plants.join(), "plants"))
    });
    for x in [mapw, uw, iw, bw, pw] {
        w.merge(x);
    }
    // Optional files (format v1 additions): work orders and plant raws.
    let work_orders = if dir.join("work_orders.jsonl").exists() {
        let (v, ww) = read_jsonl::<rf_core::WorkOrder>(&dir.join("work_orders.jsonl"), "work_orders");
        w.merge(ww);
        v
    } else {
        Vec::new()
    };
    let plant_raws = if dir.join("plant_raws.json").exists() {
        let (v, pw2) = read_json::<Vec<rf_core::PlantRaw>>(&dir.join("plant_raws.json"), "plant_raws", false)?;
        w.merge(pw2);
        v
    } else {
        Vec::new()
    };

    let mut world = World {
        meta: Meta {
            fort_name: manifest.fort_name.clone(),
            fort_name_native: manifest.fort_name_native.clone(),
            world_name: manifest.world_name.clone(),
            site_id: manifest.site_id,
            civ_id: manifest.civ_id,
            group_id: manifest.group_id,
            df_version: manifest.df_version.clone(),
            dfhack_version: manifest.dfhack_version.clone(),
            region_x: manifest.region_x,
            region_y: manifest.region_y,
            region_z: manifest.region_z,
            source: format!("export:{}", dir.display()),
            mods: manifest.mods.clone(),
        },
        // Entities were exported last, so the end tick is the state they describe.
        calendar: if manifest.year_end > 0 {
            Calendar { year: manifest.year_end, year_tick: manifest.year_tick_end }
        } else {
            Calendar { year: manifest.year, year_tick: manifest.year_tick_start }
        },
        dims,
        tiles,
        tiletypes,
        materials,
        units,
        items,
        buildings,
        plants,
        work_orders,
        plant_raws,
        ..Default::default()
    };
    validate(&mut world, &manifest, &mut w);

    let report = ImportReport {
        units: world.units.len(),
        citizens: world.units.iter().filter(|u| u.flags.citizen && !u.flags.dead).count(),
        items: world.items.len(),
        buildings: world.buildings.len(),
        plants: world.plants.len(),
        tiles: world.tiles.len(),
        work_orders: world.work_orders.len(),
        plant_raws: world.plant_raws.len(),
        building_jobs: world.buildings.iter().map(|b| b.jobs.len()).sum(),
        warnings: {
            let mut list = w.finish();
            for n in &manifest.notes {
                list.push(format!("exporter: {n}"));
            }
            list
        },
        seconds: t0.elapsed().as_secs_f64(),
    };
    Ok((world, report))
}

/// Repair references that would make other crates index out of bounds, and compare the
/// loaded counts with the manifest.
fn validate(world: &mut World, m: &Manifest, w: &mut Warnings) {
    let n_mat = world.materials.len();
    let fix_mat = |mat: &mut u16| -> bool {
        if *mat != NO_MAT && *mat as usize >= n_mat {
            *mat = NO_MAT;
            true
        } else {
            false
        }
    };
    let bad_items = world.items.iter_mut().map(|i| fix_mat(&mut i.mat)).filter(|&b| b).count();
    if bad_items > 0 {
        w.add("items", format!("{bad_items} items with material index out of palette range (set to none)"));
    }
    let bad_blds = world.buildings.iter_mut().map(|b| fix_mat(&mut b.mat)).filter(|&b| b).count();
    if bad_blds > 0 {
        w.add("buildings", format!("{bad_blds} buildings with material index out of palette range (set to none)"));
    }
    let offmap = world.units.iter().filter(|u| !world.in_bounds(u.pos)).count();
    if offmap > 0 {
        w.add("units", format!("{offmap} units are off the map (e.g. citizens away on missions); kept with their position"));
    }
    for b in &mut world.buildings {
        let area = (b.x2 - b.x1 + 1).max(0) as usize * (b.y2 - b.y1 + 1).max(0) as usize;
        if !b.extents.is_empty() && b.extents.len() != area {
            w.add("buildings", format!("building {} extents length {} != area {area}; using the full rectangle", b.id, b.extents.len()));
            b.extents.clear();
        }
    }
    // Round-2 data: palette indices of plant raws, 4 crop slots per farm plot.
    let mut bad_raw_mats = 0usize;
    for p in &mut world.plant_raws {
        for m in [&mut p.mat_structural, &mut p.mat_drink, &mut p.mat_seed] {
            if fix_mat(m) {
                bad_raw_mats += 1;
            }
        }
    }
    if bad_raw_mats > 0 {
        w.add("plant_raws", format!("{bad_raw_mats} plant raw material indices out of palette range (set to none)"));
    }
    for b in &mut world.buildings {
        if !b.crops.is_empty() && b.crops.len() != 4 {
            w.add("buildings", format!("building {} has {} crop slots, expected 4; padded/truncated", b.id, b.crops.len()));
            b.crops.resize(4, String::new());
        }
    }
    let cit = world.units.iter().filter(|u| u.flags.citizen && !u.flags.dead).count();
    let farm_plots = world.buildings.iter().filter(|b| b.kind == "FarmPlot").count();
    let building_jobs = world.buildings.iter().map(|b| b.jobs.len()).sum::<usize>();
    for (key, got) in [
        ("units", world.units.len()),
        ("citizens", cit),
        ("items", world.items.len()),
        ("buildings", world.buildings.len()),
        ("plants", world.plants.len()),
        ("materials", world.materials.len()),
        ("tiletypes", world.tiletypes.infos.len()),
        ("work_orders", world.work_orders.len()),
        ("plant_raws", world.plant_raws.len()),
        ("building_jobs", building_jobs),
        ("farm_plots", farm_plots),
    ] {
        if let Some(want) = m.count(key) {
            if want != got as i64 {
                w.add("counts", format!("{key}: manifest says {want}, imported {got}"));
            }
        }
    }
}

/// Write `world` as an export folder (same format the DFHack exporter writes). Used for
/// tests and to produce small synthetic exports; also handy as a round-trip check.
pub fn write_export(world: &World, dir: &Path) -> Result<(), ImportError> {
    use std::io::Write;
    let err = |p: &Path, e: std::io::Error| ImportError(format!("{}: {e}", p.display()));
    std::fs::create_dir_all(dir.join("map")).map_err(|e| err(dir, e))?;
    let write_json = |name: &str, s: String| -> Result<(), ImportError> {
        let p = dir.join(name);
        std::fs::write(&p, s).map_err(|e| err(&p, e))
    };
    fn to_json<T: Serialize>(v: &T) -> String {
        serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".into())
    }
    write_json("tiletypes.json", to_json(&world.tiletypes.infos))?;
    write_json("materials.json", to_json(&world.materials))?;
    fn jsonl<T: Serialize>(dir: &Path, name: &str, list: &[T]) -> Result<(), ImportError> {
        let p = dir.join(name);
        let f = std::fs::File::create(&p).map_err(|e| ImportError(format!("{}: {e}", p.display())))?;
        let mut f = std::io::BufWriter::new(f);
        for v in list {
            let line = serde_json::to_string(v).map_err(|e| ImportError(format!("{}: {e}", p.display())))?;
            writeln!(f, "{line}").map_err(|e| ImportError(format!("{}: {e}", p.display())))?;
        }
        f.flush().map_err(|e| ImportError(format!("{}: {e}", p.display())))
    }
    jsonl(dir, "units.jsonl", &world.units)?;
    jsonl(dir, "items.jsonl", &world.items)?;
    jsonl(dir, "buildings.jsonl", &world.buildings)?;
    jsonl(dir, "plants.jsonl", &world.plants)?;
    // Round-2 files (optional for the importer; always written here).
    jsonl(dir, "work_orders.jsonl", &world.work_orders)?;
    write_json("plant_raws.json", to_json(&world.plant_raws))?;
    let plane = world.dims.x as usize * world.dims.y as usize;
    for z in 0..world.dims.z as usize {
        let mut buf = Vec::with_capacity(plane * TILE_RECORD_BYTES);
        for t in &world.tiles[z * plane..(z + 1) * plane] {
            buf.extend_from_slice(&t.tt.to_le_bytes());
            buf.extend_from_slice(&t.mat.to_le_bytes());
            buf.extend_from_slice(&t.flags.to_le_bytes());
            buf.push(t.liquid);
            buf.push(t.desig);
        }
        let p = dir.join("map").join(format!("z{z:03}.bin"));
        std::fs::write(&p, buf).map_err(|e| err(&p, e))?;
    }
    let cit = world.units.iter().filter(|u| u.flags.citizen && !u.flags.dead).count();
    let mut counts = BTreeMap::new();
    for (k, v) in [
        ("units", world.units.len()),
        ("citizens", cit),
        ("items", world.items.len()),
        ("buildings", world.buildings.len()),
        ("plants", world.plants.len()),
        ("materials", world.materials.len()),
        ("tiletypes", world.tiletypes.infos.len()),
        ("tiles", world.tiles.len()),
        ("z_exported", world.dims.z as usize),
        ("work_orders", world.work_orders.len()),
        ("plant_raws", world.plant_raws.len()),
        ("building_jobs", world.buildings.iter().map(|b| b.jobs.len()).sum()),
        ("farm_plots", world.buildings.iter().filter(|b| b.kind == "FarmPlot").count()),
    ] {
        counts.insert(k.to_string(), v as i64);
    }
    let m = Manifest {
        format_version: FORMAT_VERSION,
        exporter_rev: "rf-import::write_export".into(),
        df_version: world.meta.df_version.clone(),
        dfhack_version: world.meta.dfhack_version.clone(),
        world_name: world.meta.world_name.clone(),
        fort_name: world.meta.fort_name.clone(),
        fort_name_native: world.meta.fort_name_native.clone(),
        site_id: world.meta.site_id,
        civ_id: world.meta.civ_id,
        group_id: world.meta.group_id,
        region_x: world.meta.region_x,
        region_y: world.meta.region_y,
        region_z: world.meta.region_z,
        dims: world.dims,
        year: world.calendar.year,
        year_tick_start: world.calendar.year_tick,
        year_end: world.calendar.year,
        year_tick_end: world.calendar.year_tick,
        counts,
        map_complete: true,
        mods: world.meta.mods.clone(),
        ..Default::default()
    };
    // manifest.json last: it is the done-marker, as in the DFHack exporter.
    write_json("manifest.json", to_json(&m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rf-import-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn synthetic_round_trip() {
        let w = World::synthetic(32, 32, 12);
        let dir = temp_dir("roundtrip");
        write_export(&w, &dir).unwrap();
        assert!(find_export(Some(&dir)).is_some());
        let (got, rep) = load_export(&dir).unwrap();
        assert_eq!(got.dims, w.dims);
        assert_eq!(got.tiles, w.tiles);
        assert_eq!(got.units.len(), w.units.len());
        assert_eq!(got.items.len(), w.items.len());
        assert_eq!(got.plants.len(), w.plants.len());
        assert_eq!(got.materials.len(), w.materials.len());
        assert_eq!(got.citizens().count(), 7);
        assert_eq!(got.units[0].name, w.units[0].name);
        assert_eq!(got.calendar, w.calendar);
        assert_eq!(rep.citizens, 7);
        assert_eq!(rep.tiles, 32 * 32 * 12);
        assert_eq!(got.tiletypes.by_name("StoneWall"), w.tiletypes.by_name("StoneWall"));
        assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_data_is_repaired_not_fatal() {
        let w = World::synthetic(16, 16, 6);
        let dir = temp_dir("baddata");
        write_export(&w, &dir).unwrap();
        // a broken unit line, a truncated z-file, a missing z-file, an out-of-range material
        let units = std::fs::read_to_string(dir.join("units.jsonl")).unwrap();
        std::fs::write(dir.join("units.jsonl"), format!("{{not json\n{units}\n\n")).unwrap();
        std::fs::write(dir.join("map/z002.bin"), [0u8; 10]).unwrap();
        std::fs::remove_file(dir.join("map/z003.bin")).unwrap();
        let mut z0 = std::fs::read(dir.join("map/z000.bin")).unwrap();
        z0[2] = 0xF0; // mat = 0x00F0 > palette size
        z0[3] = 0x00;
        std::fs::write(dir.join("map/z000.bin"), z0).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        assert_eq!(got.units.len(), w.units.len());
        assert_eq!(got.tiles.len(), 16 * 16 * 6);
        assert!(got.tiles[0].mat == NO_MAT);
        let plane = 16 * 16;
        assert!(got.tiles[2 * plane].hidden() && got.tiles[3 * plane].hidden());
        let all = rep.warnings.join("\n");
        assert!(all.contains("units:") && all.contains("skipped bad line"), "{all}");
        assert!(all.contains("bytes, expected"), "{all}");
        assert!(all.contains("missing"), "{all}");
        assert!(all.contains("out of palette range"), "{all}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_manifest_is_an_error() {
        let dir = temp_dir("nomanifest");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(find_export(Some(&dir)).is_none());
        assert!(load_export(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A synthetic world with round-2 data: a brewery with two queued jobs, a farm plot,
    /// a well, two work orders (one with conditions), plant raws and a mod list.
    fn round2_world() -> World {
        use rf_core::{BuildingJob, OrderCondition, PlantRaw, WorkOrder};
        let mut w = World::synthetic(24, 24, 10);
        let still = BuildingJob {
            job: "CustomReaction".into(),
            reaction: "BREW_DRINK_FROM_PLANT".into(),
            repeat: true,
            order_id: 7,
            ..Default::default()
        };
        w.buildings.push(Building {
            id: 500,
            kind: "Workshop".into(),
            subtype: "Still".into(),
            x1: 2,
            y1: 2,
            x2: 4,
            y2: 4,
            built: true,
            jobs: vec![
                still.clone(),
                BuildingJob { job: "ConstructBarrel".into(), item_type: "BARREL".into(), mat_category: vec!["wood".into()], suspended: true, order_id: -1, ..Default::default() },
            ],
            ..Default::default()
        });
        w.buildings.push(Building {
            id: 501,
            kind: "FarmPlot".into(),
            x1: 6,
            y1: 6,
            x2: 8,
            y2: 8,
            built: true,
            crops: vec!["MUSHROOM_HELMET_PLUMP".into(), "GRASS_WHEAT_CAVE".into(), "".into(), "MUSHROOM_HELMET_PLUMP".into()],
            ..Default::default()
        });
        w.buildings.push(Building { id: 502, kind: "Well".into(), x1: 10, y1: 10, x2: 10, y2: 10, built: true, ..Default::default() });
        w.work_orders = vec![
            WorkOrder {
                id: 7,
                job: "CustomReaction".into(),
                reaction: "BREW_DRINK_FROM_PLANT".into(),
                amount_left: 3,
                amount_total: 5,
                frequency: "Daily".into(),
                workshop_id: -1,
                active: true,
                validated: true,
                conditions: vec![
                    OrderCondition { compare: "LessThan".into(), value: 100, item_type: "DRINK".into(), ..Default::default() },
                    OrderCondition {
                        compare: "AtLeast".into(),
                        value: 5,
                        item_type: "PLANT".into(),
                        flags: vec!["UNROTTEN".into(), "HAS_MATERIAL_REACTION_PRODUCT:DRINK_MAT".into()],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            WorkOrder { id: 8, job: "ConstructBed".into(), item_type: "BED".into(), mat_category: vec!["wood".into()], amount_left: 1, amount_total: 1, frequency: "OneTime".into(), workshop_id: 500, ..Default::default() },
        ];
        w.plant_raws = vec![
            PlantRaw {
                token: "MUSHROOM_HELMET_PLUMP".into(),
                name: "plump helmet".into(),
                name_plural: "plump helmets".into(),
                growdur: 30_000,
                spring: true,
                summer: true,
                autumn: true,
                winter: true,
                underground: true,
                is_shrub: true,
                brewable: true,
                edible_raw: true,
                edible_cooked: true,
                has_seed: true,
                mat_structural: 6,
                mat_drink: 5,
                mat_seed: NO_MAT,
                ..Default::default()
            },
            PlantRaw { token: "OAK".into(), name: "oak".into(), is_tree: true, aboveground: true, mat_structural: 3, mat_drink: NO_MAT, mat_seed: NO_MAT, ..Default::default() },
        ];
        w.meta.mods = vec!["vanilla_text:5001".into(), "vanilla_creatures:5316".into()];
        w
    }

    #[test]
    fn round2_data_round_trip() {
        let w = round2_world();
        let dir = temp_dir("round2");
        write_export(&w, &dir).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
        assert_eq!(rep.work_orders, 2);
        assert_eq!(rep.plant_raws, 2);
        assert_eq!(rep.building_jobs, 2);
        // work orders with conditions
        let o = got.work_orders.iter().find(|o| o.id == 7).unwrap();
        assert_eq!((o.job.as_str(), o.reaction.as_str(), o.frequency.as_str()), ("CustomReaction", "BREW_DRINK_FROM_PLANT", "Daily"));
        assert_eq!((o.amount_left, o.amount_total, o.workshop_id, o.active, o.validated), (3, 5, -1, true, true));
        assert_eq!(o.conditions.len(), 2);
        assert_eq!(o.conditions[0].compare, "LessThan");
        assert_eq!(o.conditions[1].flags, vec!["UNROTTEN", "HAS_MATERIAL_REACTION_PRODUCT:DRINK_MAT"]);
        assert_eq!(got.work_orders[1].workshop_id, 500);
        assert_eq!(got.work_orders[1].mat_category, vec!["wood"]);
        // workshop job queue, farm crops, well
        let still = got.building(500).unwrap();
        assert_eq!(still.jobs.len(), 2);
        assert_eq!(still.jobs[0].reaction, "BREW_DRINK_FROM_PLANT");
        assert!(still.jobs[0].repeat && still.jobs[0].order_id == 7);
        assert!(still.jobs[1].suspended && still.jobs[1].item_type == "BARREL");
        let farm = got.building(501).unwrap();
        assert_eq!(farm.crops, vec!["MUSHROOM_HELMET_PLUMP", "GRASS_WHEAT_CAVE", "", "MUSHROOM_HELMET_PLUMP"]);
        assert!(got.building(502).unwrap().jobs.is_empty() && got.building(502).unwrap().crops.is_empty());
        assert_eq!(got.buildings.iter().filter(|b| b.kind == "Well").count(), 1);
        // plant raws
        let ph = got.plant_raws.iter().find(|p| p.token == "MUSHROOM_HELMET_PLUMP").unwrap();
        assert_eq!(ph.growdur, 30_000);
        assert!(ph.brewable && ph.underground && !ph.aboveground && ph.has_seed && ph.edible_raw);
        assert_eq!((ph.mat_structural, ph.mat_drink, ph.mat_seed), (6, 5, NO_MAT));
        assert_eq!(got.material(ph.mat_drink).unwrap().token, "PLANT:PLUMP_HELMET:DRINK");
        // mods
        assert_eq!(got.meta.mods, vec!["vanilla_text:5001", "vanilla_creatures:5316"]);
        assert_eq!(load_manifest(&dir).unwrap().mods, got.meta.mods);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_without_round2_files_still_loads() {
        // a v0.1 export: no work_orders.jsonl / plant_raws.json, no mods, no jobs/crops fields
        let mut w = round2_world();
        for b in &mut w.buildings {
            b.jobs.clear();
            b.crops.clear();
        }
        w.work_orders.clear();
        w.plant_raws.clear();
        w.meta.mods.clear();
        let dir = temp_dir("v01");
        write_export(&w, &dir).unwrap();
        std::fs::remove_file(dir.join("work_orders.jsonl")).unwrap();
        std::fs::remove_file(dir.join("plant_raws.json")).unwrap();
        let b = std::fs::read_to_string(dir.join("buildings.jsonl")).unwrap();
        std::fs::write(dir.join("buildings.jsonl"), b.replace(r#","jobs":[],"crops":[]"#, "")).unwrap();
        let m = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
        let mut m: serde_json::Value = serde_json::from_str(&m).unwrap();
        for k in ["work_orders", "plant_raws", "building_jobs", "farm_plots"] {
            m["counts"].as_object_mut().unwrap().remove(k);
        }
        m.as_object_mut().unwrap().remove("mods");
        std::fs::write(dir.join("manifest.json"), m.to_string()).unwrap();
        assert!(!std::fs::read_to_string(dir.join("buildings.jsonl")).unwrap().contains("\"jobs\""));
        let (got, rep) = load_export(&dir).unwrap();
        assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
        assert!(got.work_orders.is_empty() && got.plant_raws.is_empty() && got.meta.mods.is_empty());
        assert_eq!(got.buildings.len(), w.buildings.len());
        assert!(got.buildings.iter().all(|b| b.jobs.is_empty() && b.crops.is_empty()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round2_bad_data_is_repaired() {
        let mut w = round2_world();
        w.plant_raws[0].mat_drink = 9_999; // out of palette range
        w.buildings.iter_mut().find(|b| b.id == 501).unwrap().crops.truncate(2);
        let dir = temp_dir("round2bad");
        write_export(&w, &dir).unwrap();
        // one broken work order line
        let o = std::fs::read_to_string(dir.join("work_orders.jsonl")).unwrap();
        std::fs::write(dir.join("work_orders.jsonl"), format!("{o}{{\"id\":\n")).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        let all = rep.warnings.join("\n");
        assert!(all.contains("plant_raws: 1 plant raw material indices"), "{all}");
        assert!(all.contains("crop slots"), "{all}");
        assert!(all.contains("work_orders:") && all.contains("skipped bad line"), "{all}");
        assert_eq!(got.plant_raws[0].mat_drink, NO_MAT);
        assert_eq!(got.building(501).unwrap().crops.len(), 4);
        assert_eq!(got.work_orders.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exporter_style_round2_lines_parse() {
        // Lines as rustfort-export.lua (rev 2026-10-10a) writes them, extras included.
        let o: rf_core::WorkOrder = serde_json::from_str(r#"{"job":"ConstructSlab","reaction":"","item_type":"SLAB","item_subtype":"","mat":"INORGANIC","mat_category":[],"id":61,"amount_left":1,"amount_total":1,"frequency":"Daily","workshop_id":-1,"active":true,"validated":true,"conditions":[{"compare":"AtLeast","value":22,"item_type":"BOULDER","item_subtype":"","mat":"INORGANIC","flags":["NON_ECONOMIC","HARD"]},{"compare":"LessThan","value":2,"item_type":"SLAB","item_subtype":"","mat":"","flags":["UNENGRAVED"]}],"order_conditions":[],"max_workshops":0,"finished_year":-1,"finished_year_tick":-1}"#).unwrap();
        assert_eq!((o.id, o.job.as_str(), o.mat.as_str()), (61, "ConstructSlab", "INORGANIC"));
        assert_eq!(o.conditions[0].flags, vec!["NON_ECONOMIC", "HARD"]);
        let b: Building = serde_json::from_str(r#"{"id":10978,"kind":"Workshop","subtype":"Still","x1":1,"y1":1,"x2":3,"y2":3,"z":130,"stockpile":null,"extents":[],"items":[],"jobs":[{"job":"CustomReaction","reaction":"BREW_DRINK_FROM_PLANT","item_type":"","item_subtype":"","mat":"","mat_category":[],"repeat":false,"suspended":false,"order_id":-1,"id":761553,"worker":-1,"working":false,"do_now":false}],"crops":[]}"#).unwrap();
        assert_eq!(b.jobs.len(), 1);
        assert_eq!(b.jobs[0].reaction, "BREW_DRINK_FROM_PLANT");
        assert_eq!(b.jobs[0].order_id, -1);
        let f: Building = serde_json::from_str(r#"{"id":10609,"kind":"FarmPlot","jobs":[],"crops":["MUSHROOM_HELMET_PLUMP","GRASS_WHEAT_CAVE","GRASS_WHEAT_CAVE","MUSHROOM_HELMET_PLUMP"]}"#).unwrap();
        assert_eq!(f.crops[1], "GRASS_WHEAT_CAVE");
        let p: rf_core::PlantRaw = serde_json::from_str(r#"{"token":"MUSHROOM_HELMET_PLUMP","name":"plump helmet","name_plural":"plump helmets","growdur":30000,"spring":true,"summer":true,"autumn":true,"winter":true,"underground":true,"aboveground":false,"is_tree":false,"is_shrub":true,"brewable":true,"edible_raw":true,"edible_cooked":true,"millable":false,"threshable":false,"has_seed":true,"mat_structural":1,"mat_drink":142,"mat_seed":200,"growdur_raw":300,"value":2,"is_grass":false,"biomes":["SUBTERRANEAN_WATER"],"underground_depth":[1,3],"brew_from":"plant","mill":"","thread":"","extract_barrel":false,"extract_vial":false,"extract_still_vial":false,"seed_name":"plump helmet spawn","growths":[]}"#).unwrap();
        assert!(p.brewable && p.underground && p.growdur == 30_000 && p.mat_seed == 200);
        let m: Manifest = serde_json::from_str(r#"{"format_version":1,"mods":["vanilla_text:5001","vanilla_procedural:5316"],"mods_detail":[{"id":"vanilla_text","name":"Vanilla Text","version":"50.01","numeric_version":5001}],"counts":{"work_orders":40}}"#).unwrap();
        assert_eq!(m.mods.len(), 2);
        assert_eq!(m.mods_detail[0].numeric_version, 5001);
        assert_eq!(m.count("work_orders"), Some(40));
    }

    #[test]
    fn extras_load_and_default() {
        let w = round2_world();
        let dir = temp_dir("extras");
        write_export(&w, &dir).unwrap();
        assert!(load_extras(&dir).burrows.is_empty());
        std::fs::write(dir.join("burrows.json"), r#"[{"id":1,"name":"Zuflucht","tiles":1716,"blocks":18,"units":[]}]"#).unwrap();
        std::fs::write(dir.join("squads.json"), r#"[{"id":72,"name":"Wache","members":[13957,-1],"member_hfs":[3898,3889],"positions":10}]"#).unwrap();
        let e = load_extras(&dir);
        assert_eq!(e.burrows[0].tiles, 1716);
        assert_eq!(e.squads[0].members, vec![13957, -1]);
        std::fs::write(dir.join("squads.json"), "{broken").unwrap();
        assert!(load_extras(&dir).squads.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exporter_style_json_parses() {
        // One line each as the Lua exporter writes them (extra fields must be ignored).
        let u: Unit = serde_json::from_str(r#"{"id":1,"race":"DWARF","caste":"FEMALE","name":"Vabôk Papertaught","pos":{"x":1,"y":2,"z":3},"flags":{"citizen":true},"skills":[{"id":"MINING","rating":17,"xp":277}],"labors":[],"phys_attrs":[["STRENGTH",1250]],"inventory":[],"age":80.0775,"profession_name":"Doctor"}"#).unwrap();
        assert_eq!(u.phys_attrs[0], ("STRENGTH".to_string(), 1250));
        assert!(u.flags.citizen && u.skill("MINING") == 17);
        let b: Building = serde_json::from_str(r#"{"id":5,"kind":"Stockpile","x1":1,"y1":1,"x2":2,"y2":2,"z":0,"stockpile":{"categories":["food"],"max_barrels":0},"extents":[1,0,1,1],"items":[]}"#).unwrap();
        assert_eq!(b.stockpile.unwrap().categories, vec!["food"]);
        let b2: Building = serde_json::from_str(r#"{"id":6,"kind":"Bed","stockpile":null,"extents":[]}"#).unwrap();
        assert!(b2.stockpile.is_none());
    }
}
