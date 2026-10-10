//! rf-import: loads a DFHack fort export (docs/EXPORT_FORMAT.md) into an rf-core World.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//!
//! The loader never panics on bad data: malformed JSON lines are skipped, out-of-range
//! material or tiletype ids are replaced, missing z-files become hidden void, and every such
//! repair is listed in [`ImportReport::warnings`]. Only a missing/unreadable `manifest.json`
//! or `tiletypes.json` is a hard error.
//!
//! Optional files are loaded when present: round 2 (`work_orders.jsonl`, `plant_raws.json`) and
//! round 3 (`creature_raws.json`, `weapon_raws.json`, `armor_raws.json`, and `squads.json` /
//! `burrows.json` into `World::squads` / `World::burrows`; the round-2 layout of the last two
//! loads as well) and round 4 (`entities.json`, `nobles.json`, `settings.json`, `hist_figs.json`,
//! `caravans.json` into `World::entities` / `nobles` / `settings` / `hist_figs` / `caravans`;
//! building flags, mechanism links and directions come with `buildings.jsonl`) and round 5
//! (`engravings.json` into `World::engravings`) and round 8 (`work_details.json`, `kitchen.json`,
//! `locations.json`, `crimes.json`, `haul_routes.json` into `World::work_details` / `kitchen` /
//! `locations` / `crimes` / `haul_routes`; stockpile filters and links, animal fields and work
//! detail names come with `buildings.jsonl` / `units.jsonl`; see [`round8`]) and round 9
//! (`stockpile_options.json` into `World::stockpile_options`; wealth statistics and invasion tiers in
//! `settings.other`, `invasions.json` in [`Extras::invasions`]; units DF does not simulate are
//! inactive; see [`round9`]).

use rf_core::{
    tile_flags, ArmorRaw, Building, Burrow, Calendar, Caravan, CreatureRaw, Dims, Engraving, EntityInfo, GameSettings, HistFigName, Item,
    MaterialInfo, Meta, NoblePosition, Plant, Pos, Shape, Squad, Tile, TiletypeInfo, TiletypeTable, TtMaterial, TtSpecial, Unit,
    WeaponRaw, World, NO_MAT,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub mod round8;
pub use round8::{AnimalProducts, MilkInfo, PunishmentInfo, RoomEval, ShearInfo};
pub mod round9;
pub use round9::{load_invasions, options_for, wealth, InvasionInfo, WealthInfo};

/// Folder (relative to the working directory) that holds the fort exports: the app's
/// "Import from Dwarf Fortress" writes `export/<fort>-<date>` there, and [`find_export`] takes
/// the newest complete one by default. Gitignored.
pub const EXPORT_ROOT: &str = "export";

/// Old name of [`EXPORT_ROOT`] (until v0.12 the default export was the fixed folder
/// `export/tileshtulon`; now it is the newest export under `export/`).
#[deprecated(note = "use EXPORT_ROOT and find_export")]
pub const DEFAULT_EXPORT: &str = EXPORT_ROOT;

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
    /// Round-3 data (0 when the export predates it).
    pub creature_raws: usize,
    pub weapon_raws: usize,
    pub armor_raws: usize,
    pub squads: usize,
    pub burrows: usize,
    /// Sum of `Burrow::tiles` (0 for round-2 exports, which only had tile counts).
    pub burrow_tiles: usize,
    /// Round-4 data (0 / false when the export predates it).
    pub entities: usize,
    pub nobles: usize,
    pub hist_figs: usize,
    pub caravans: usize,
    /// `settings.json` was present (else `World::settings` holds DF's defaults, see [`default_settings`]).
    pub settings: bool,
    /// Mechanism links from levers/pressure plates to the buildings they operate.
    pub building_links: usize,
    /// Round-5 data: DF's existing engravings (`engravings.json`; 0 when the export predates it).
    pub engravings: usize,
    /// Round-8 data (0 / false when the export predates it): work details, kitchen exclusions,
    /// locations, crimes at the site, hauling routes, stockpile filter strings.
    pub round8: bool,
    pub work_details: usize,
    pub kitchen: usize,
    pub locations: usize,
    pub crimes: usize,
    pub haul_routes: usize,
    pub stockpile_filters: usize,
    pub warnings: Vec<String>,
    pub seconds: f64,
}

/// Tile flag bit 14 of the export (round 3): DF `tile_occupancy.heavy_aquifer`. Not in
/// `rf_core::tile_flags` yet; see docs/EXPORT_FORMAT.md.
pub const TILE_FLAG_HEAVY_AQUIFER: u16 = 1 << 14;

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

/// A diplomat or outpost liaison of `diplomats.json` (round 4; not part of rf-core).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DiplomatInfo {
    /// Entity the diplomat speaks for.
    pub civ_id: i32,
    pub diplomat_hf: i32,
    /// Unit on the map, -1 if not here (any more).
    pub diplomat_unit: i32,
    pub associate_hf: i32,
    pub associate_unit: i32,
    /// `meeting` (DF `plotinfo.dip_meeting_info`) or `unit` (a diplomat unit without a meeting).
    pub source: String,
    pub flags: Vec<String>,
}

/// A mandate of `nobles.json` (`mandates_detail` extra of a noble position; round 4).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MandateInfo {
    /// Text as in `NoblePosition::mandates`, e.g. `Make 1 spear`, `Ban export of spears`.
    pub text: String,
    /// `Export`, `Make`, `Guild`.
    pub mode: String,
    pub item_type: String,
    pub item_subtype: String,
    pub mat: String,
    pub amount_total: i32,
    pub amount_remaining: i32,
    /// DF counts the mandate down; at `timeout_limit` the noble punishes.
    pub timeout_counter: i32,
    pub timeout_limit: i32,
    /// Holder unit of the issuing position.
    pub noble: i32,
    pub position: String,
}

/// Optional extra data of an export that has no place in `rf_core::World` yet.
#[derive(Clone, Debug, Default)]
pub struct Extras {
    pub burrows: Vec<BurrowInfo>,
    pub squads: Vec<SquadInfo>,
    pub mods: Vec<ModInfo>,
    /// Round 4: diplomats / liaisons and the mandates with their numbers.
    pub diplomats: Vec<DiplomatInfo>,
    pub mandates: Vec<MandateInfo>,
    /// Round 8: DF's room evaluation per civzone, running punishments, caste animal products.
    pub rooms: Vec<RoomEval>,
    pub punishments: Vec<PunishmentInfo>,
    pub animal_products: Vec<AnimalProducts>,
    /// Round 9: every invasion DF sent (`invasions.json`) and DF's wealth statistics / invasion
    /// tiers (from `settings.json`).
    pub invasions: Vec<InvasionInfo>,
    pub wealth: Option<WealthInfo>,
}

/// A nobles.json entry: rf_core::NoblePosition plus the mandate details.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawNoble {
    #[serde(flatten)]
    noble: NoblePosition,
    mandates_detail: Vec<MandateInfo>,
}

/// DF's defaults for an export without `settings.json` (d_init.txt of DF 53: population cap 200,
/// strict cap 220, baby cap 1000 absolute; invasions, temperature, weather and cave-ins on).
pub fn default_settings() -> GameSettings {
    GameSettings {
        population_cap: 200,
        strict_population_cap: 220,
        baby_cap: 1000,
        invasions: true,
        temperature: true,
        weather: true,
        cave_ins: true,
        other: Vec::new(),
    }
}

/// Read `burrows.json`, `squads.json` and the manifest's `mods_detail` (all optional; missing
/// or broken files give empty lists). Since round 3 the burrows and squads are also first-class
/// (`World::burrows`, `World::squads`); this summary view reads both the round-2 and the
/// round-3 file layout.
pub fn load_extras(dir: &Path) -> Extras {
    fn opt<T: DeserializeOwned + Default>(p: &Path) -> T {
        std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(strip_bom(&t)).ok()).unwrap_or_default()
    }
    let burrows: Vec<RawBurrow> = opt(&dir.join("burrows.json"));
    let squads: Vec<RawSquad> = opt(&dir.join("squads.json"));
    Extras {
        burrows: burrows
            .into_iter()
            .map(|b| BurrowInfo {
                id: b.id,
                tiles: b.tile_count(),
                blocks: b.blocks,
                units: b.units.clone(),
                name: b.name,
            })
            .collect(),
        squads: squads.into_iter().map(RawSquad::info).collect(),
        mods: load_manifest(dir).map(|m| m.mods_detail).unwrap_or_default(),
        diplomats: opt(&dir.join("diplomats.json")),
        mandates: opt::<Vec<RawNoble>>(&dir.join("nobles.json"))
            .into_iter()
            .flat_map(|n| {
                let (holder, token) = (n.noble.holder, n.noble.token);
                n.mandates_detail.into_iter().map(move |m| MandateInfo { noble: holder, position: token.clone(), ..m })
            })
            .collect(),
        rooms: round8::load_room_evals(dir),
        punishments: round8::load_punishments(dir),
        animal_products: round8::load_animal_products(dir),
        invasions: round9::load_invasions(dir),
        wealth: opt::<Option<GameSettings>>(&dir.join("settings.json")).as_ref().and_then(round9::wealth),
    }
}

/// `tiles` of a burrows.json entry: a tile count (round 2) or the tile list (round 3).
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum BurrowTiles {
    Count(i32),
    List(Vec<Pos>),
}

impl Default for BurrowTiles {
    fn default() -> Self {
        BurrowTiles::List(Vec::new())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawBurrow {
    id: i32,
    name: String,
    tiles: BurrowTiles,
    units: Vec<i32>,
    /// Round-3 extra: number of tiles (same as `tiles.len()`).
    tile_count: Option<i32>,
    blocks: i32,
}

impl RawBurrow {
    fn tile_count(&self) -> i32 {
        match &self.tiles {
            BurrowTiles::Count(n) => *n,
            BurrowTiles::List(v) => self.tile_count.unwrap_or(v.len() as i32).max(v.len() as i32),
        }
    }
    fn into_burrow(self) -> Burrow {
        let tiles = match self.tiles {
            BurrowTiles::Count(_) => Vec::new(),
            BurrowTiles::List(v) => v,
        };
        Burrow { id: self.id, name: self.name, tiles, units: self.units }
    }
}

/// A squads.json entry: rf_core::Squad plus the extras the summary view needs.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawSquad {
    #[serde(flatten)]
    squad: Squad,
    member_hfs: Vec<i32>,
    positions: i32,
}

impl RawSquad {
    /// Round 2 listed only filled positions; round 3 lists every position (-1 = empty, hf -1).
    fn info(self) -> SquadInfo {
        let s = self.squad;
        let name = if s.alias.is_empty() { s.name.clone() } else { s.alias.clone() };
        let (mut members, mut member_hfs) = (Vec::new(), Vec::new());
        if self.member_hfs.len() == s.members.len() {
            for (u, h) in s.members.iter().zip(&self.member_hfs) {
                if *h >= 0 {
                    members.push(*u);
                    member_hfs.push(*h);
                }
            }
        } else {
            members = s.members.iter().copied().filter(|&u| u >= 0).collect();
            member_hfs = self.member_hfs.clone();
        }
        SquadInfo { id: s.id, name, members, member_hfs, positions: self.positions }
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
    /// Round 3: DF's stress category cutoffs (`dfhack.units.getStressCutoffs`), descending, e.g.
    /// `[50000, 25000, 10000, -10000, -25000, -50000, -100000]`. A unit's category (0 = most
    /// stressed .. 6 = happiest) is the number of cutoffs strictly greater than its stress.
    pub stress_cutoffs: Vec<i32>,
    /// Round 3: longest frame slice per exporter phase (ms).
    pub max_slice_by_phase: BTreeMap<String, u32>,
}

impl Manifest {
    /// DF stress category (0 = most stressed .. `stress_cutoffs.len()` = happiest) of a stress
    /// value, from this export's cutoffs. `None` if the export has no cutoffs (before round 3).
    pub fn stress_category(&self, stress: i32) -> Option<usize> {
        (!self.stress_cutoffs.is_empty()).then(|| self.stress_cutoffs.iter().filter(|&&c| c > stress).count())
    }
}

impl Manifest {
    pub fn count(&self, key: &str) -> Option<i64> {
        self.counts.get(key).copied()
    }
}

/// Find the export folder:
/// 1. the explicit path (`--export DIR`), if it holds a `manifest.json`;
/// 2. else env `RUSTFORT_EXPORT`, if it holds a `manifest.json`;
/// 3. else the newest complete export under `export/` in the working directory
///    ([`newest_export`]); for tests and examples started by cargo from a crate folder also
///    under `export/` of the workspace root;
/// 4. else `None` (the app then starts its built-in synthetic test fort).
pub fn find_export(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return p.join("manifest.json").exists().then(|| p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("RUSTFORT_EXPORT").filter(|v| !v.is_empty()).map(PathBuf::from) {
        if p.join("manifest.json").exists() {
            return Some(p);
        }
    }
    let ws = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join(EXPORT_ROOT);
    newest_export(Path::new(EXPORT_ROOT)).or_else(|| newest_export(&ws))
}

/// A complete export: `manifest.json` (the exporter writes it last) and no `export.lock`
/// (an export still running).
pub fn is_complete_export(dir: &Path) -> bool {
    dir.join("manifest.json").is_file() && !dir.join("export.lock").exists()
}

/// The most recently written complete export directly under `root` (by the modification
/// time of its `manifest.json`; the folder name decides ties), or `None`.
pub fn newest_export(root: &Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(root).ok()?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in rd.flatten() {
        let dir = e.path();
        if !dir.is_dir() || !is_complete_export(&dir) {
            continue;
        }
        let t = std::fs::metadata(dir.join("manifest.json")).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().map_or(true, |(bt, bd)| (t, &dir) > (*bt, bd)) {
            best = Some((t, dir));
        }
    }
    best.map(|(_, d)| d)
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
    // Round-3 files (optional): creature/weapon/armor raws; squads and burrows (round 2 already had
    // these two files with less data, they load too).
    fn optional<T: DeserializeOwned + Default>(dir: &Path, name: &str, kind: &'static str, w: &mut Warnings) -> T {
        let p = dir.join(name);
        if !p.exists() {
            return T::default();
        }
        match read_json::<T>(&p, kind, false) {
            Ok((v, ww)) => {
                w.merge(ww);
                v
            }
            Err(_) => T::default(),
        }
    }
    let creature_raws: Vec<CreatureRaw> = optional(dir, "creature_raws.json", "creature_raws", &mut w);
    let weapon_raws: Vec<WeaponRaw> = optional(dir, "weapon_raws.json", "weapon_raws", &mut w);
    let armor_raws: Vec<ArmorRaw> = optional(dir, "armor_raws.json", "armor_raws", &mut w);
    let squads: Vec<Squad> =
        optional::<Vec<RawSquad>>(dir, "squads.json", "squads", &mut w).into_iter().map(|s| s.squad).collect();
    let raw_burrows: Vec<RawBurrow> = optional(dir, "burrows.json", "burrows", &mut w);
    let mut count_only_burrows = 0usize;
    let burrows: Vec<Burrow> = raw_burrows
        .into_iter()
        .map(|b| {
            if matches!(b.tiles, BurrowTiles::Count(n) if n > 0) {
                count_only_burrows += 1;
            }
            b.into_burrow()
        })
        .collect();
    if count_only_burrows > 0 {
        w.add("burrows", format!("{count_only_burrows} burrows without tile lists (round-2 export); imported with no tiles"));
    }
    // Round-4 files (optional): civilizations, nobles, settings, historical figure names, caravans.
    let entities: Vec<EntityInfo> = optional(dir, "entities.json", "entities", &mut w);
    let nobles: Vec<NoblePosition> =
        optional::<Vec<RawNoble>>(dir, "nobles.json", "nobles", &mut w).into_iter().map(|n| n.noble).collect();
    let has_settings = dir.join("settings.json").exists();
    let settings: GameSettings =
        if has_settings { optional(dir, "settings.json", "settings", &mut w) } else { default_settings() };
    let hist_figs: Vec<HistFigName> = optional(dir, "hist_figs.json", "hist_figs", &mut w);
    let caravans: Vec<Caravan> = optional(dir, "caravans.json", "caravans", &mut w);
    // Round-5 file (optional): DF's existing engravings.
    let engravings: Vec<Engraving> = optional(dir, "engravings.json", "engravings", &mut w);
    // Round-8 files (optional): work details, kitchen, locations, crimes, hauling routes.
    let r8 = round8::load_files(dir, &mut w);
    let r8_present = r8.present;
    // Round-9 file (optional): every stockpile leaf DF offers.
    let stockpile_options = round9::load_stockpile_options(dir, &mut w);

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
        creature_raws,
        weapon_raws,
        armor_raws,
        squads,
        burrows,
        entities,
        nobles,
        settings,
        hist_figs,
        caravans,
        engravings,
        work_details: r8.work_details,
        kitchen: r8.kitchen,
        locations: r8.locations,
        crimes: r8.crimes,
        haul_routes: r8.haul_routes,
        stockpile_options,
        // keeps this compiling when other rounds add World fields
        ..Default::default()
    };
    // [war9] DF's per-civ siege count (`siege_tier` in entities.json) sets the strength of the
    // next siege (wiki Difficulty: "invasion difficulties" of consecutive sieges).
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct SiegeTier {
        id: i32,
        siege_tier: i32,
    }
    let tiers: Vec<SiegeTier> = optional(dir, "entities.json", "entities", &mut Warnings::default());
    world.fort_history.sieges = tiers.iter().filter(|t| t.siege_tier > 0).map(|t| (t.id, t.siege_tier)).collect();
    round8::check_counts(&world, &manifest, &mut w);
    validate(&mut world, &manifest, &mut w);
    round8::repair(&mut world, r8_present, &mut w);
    round9::repair_units(&mut world, &manifest, &mut w);

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
        creature_raws: world.creature_raws.len(),
        weapon_raws: world.weapon_raws.len(),
        armor_raws: world.armor_raws.len(),
        squads: world.squads.len(),
        burrows: world.burrows.len(),
        burrow_tiles: world.burrows.iter().map(|b| b.tiles.len()).sum(),
        entities: world.entities.len(),
        nobles: world.nobles.len(),
        hist_figs: world.hist_figs.len(),
        caravans: world.caravans.len(),
        settings: has_settings,
        building_links: trigger_links(&world),
        engravings: world.engravings.len(),
        round8: r8_present,
        work_details: world.work_details.len(),
        kitchen: world.kitchen.len(),
        locations: world.locations.len(),
        crimes: world.crimes.len(),
        haul_routes: world.haul_routes.len(),
        stockpile_filters: round8::filter_count(&world),
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

/// A lever or pressure plate: the buildings whose `links` point at what they operate.
pub fn is_trigger(b: &Building) -> bool {
    b.kind == "Trap" && (b.subtype == "Lever" || b.subtype == "PressurePlate")
}

/// Number of mechanism links from levers/pressure plates (the manifest's `building_links`).
fn trigger_links(world: &World) -> usize {
    world.buildings.iter().filter(|b| is_trigger(b)).map(|b| b.links.len()).sum()
}

/// Round 4: mechanism links must point at existing buildings and be symmetric (a lever lists
/// its targets, a target lists its levers).
fn repair_links(world: &mut World, w: &mut Warnings) {
    use std::collections::{BTreeSet, HashMap};
    let index: HashMap<i32, usize> = world.buildings.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut dangling = 0usize;
    for b in &mut world.buildings {
        let id = b.id;
        let before = b.links.len();
        let mut seen = BTreeSet::new();
        b.links.retain(|l| *l != id && index.contains_key(l) && seen.insert(*l));
        dangling += before - b.links.len();
    }
    if dangling > 0 {
        w.add("buildings", format!("{dangling} mechanism links to missing buildings (or duplicates) dropped"));
    }
    let mut missing = Vec::new();
    for b in &world.buildings {
        for l in &b.links {
            let other = &world.buildings[index[l]];
            if !other.links.contains(&b.id) {
                missing.push((index[l], b.id));
            }
        }
    }
    if !missing.is_empty() {
        w.add("buildings", format!("{} one-sided mechanism links made symmetric", missing.len()));
        for (i, id) in missing {
            if !world.buildings[i].links.contains(&id) {
                world.buildings[i].links.push(id);
            }
        }
    }
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
        w.add("units", format!("{offmap} units are off the map (animals of departed caravans, citizens away); kept with their position"));
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
    // Round-3 data: tissue materials, body part parents, burrow tiles inside the map.
    let mut bad_tissue = 0usize;
    let mut bad_parent = 0usize;
    for cr in &mut world.creature_raws {
        for c in &mut cr.castes {
            let n = c.body_parts.len() as i32;
            for p in &mut c.body_parts {
                if p.parent < -1 || p.parent >= n {
                    p.parent = -1;
                    bad_parent += 1;
                }
                for l in &mut p.layers {
                    if fix_mat(&mut l.mat) {
                        bad_tissue += 1;
                    }
                }
            }
        }
    }
    if bad_tissue > 0 {
        w.add("creature_raws", format!("{bad_tissue} tissue layer material indices out of palette range (set to none)"));
    }
    if bad_parent > 0 {
        w.add("creature_raws", format!("{bad_parent} body parts with an out-of-range parent (set to root)"));
    }
    let dims = world.dims;
    let inside = |p: &Pos| p.x >= 0 && p.y >= 0 && p.z >= 0 && (p.x as u16) < dims.x && (p.y as u16) < dims.y && (p.z as u16) < dims.z;
    let mut bad_tiles = 0usize;
    for b in &mut world.burrows {
        let before = b.tiles.len();
        b.tiles.retain(|p| inside(p));
        bad_tiles += before - b.tiles.len();
    }
    if bad_tiles > 0 {
        w.add("burrows", format!("{bad_tiles} burrow tiles outside the map dropped"));
    }
    // Round-4 data: mechanism links, noble holders that are not on the map.
    repair_links(world, w);
    // Round-5 data: engravings.
    repair_engravings(world, w);
    let unit_ids: std::collections::HashSet<i32> = world.units.iter().map(|u| u.id).collect();
    let gone = world.nobles.iter().filter(|n| n.holder >= 0 && !unit_ids.contains(&n.holder)).count();
    if gone > 0 {
        w.add("nobles", format!("{gone} noble holders are not among the exported units"));
    }
    let cit = world.units.iter().filter(|u| u.flags.citizen && !u.flags.dead).count();
    let farm_plots = world.buildings.iter().filter(|b| b.kind == "FarmPlot").count();
    let building_jobs = world.buildings.iter().map(|b| b.jobs.len()).sum::<usize>();
    let sum = |f: fn(&Unit) -> usize| world.units.iter().map(f).sum::<usize>();
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
        ("creature_raws", world.creature_raws.len()),
        ("weapon_raws", world.weapon_raws.len()),
        ("armor_raws", world.armor_raws.len()),
        ("squads", world.squads.len()),
        ("burrows", world.burrows.len()),
        ("burrow_tiles", world.burrows.iter().map(|b| b.tiles.len()).sum()),
        ("unit_needs", sum(|u| u.needs.len())),
        ("unit_emotions", sum(|u| u.emotions.len())),
        ("unit_values", sum(|u| u.values.len())),
        ("unit_relationships", sum(|u| u.relationships.len())),
        ("unit_wounds", sum(|u| u.wounds.len())),
        ("entities", world.entities.len()),
        ("nobles", world.nobles.len()),
        ("mandates", world.nobles.iter().map(|n| n.mandates.len()).sum()),
        ("hist_figs", world.hist_figs.len()),
        ("caravans", world.caravans.len()),
        ("building_links", trigger_links(world)),
        ("engravings", world.engravings.len()),
        ("levers", world.buildings.iter().filter(|b| b.kind == "Trap" && b.subtype == "Lever").count()),
        ("traps", world.buildings.iter().filter(|b| b.kind == "Trap").count()),
        ("locked_doors", world.buildings.iter().filter(|b| b.flags.iter().any(|f| f == "LOCKED")).count()),
        ("trade_depots", world.buildings.iter().filter(|b| b.kind == "TradeDepot").count()),
    ] {
        if let Some(want) = m.count(key) {
            if want != got as i64 {
                w.add("counts", format!("{key}: manifest says {want}, imported {got}"));
            }
        }
    }
}

/// Round-5 `engravings.json` repairs: engravings off the map are dropped, a second engraving
/// on the same tile is dropped (DF keeps one per tile; the simulation indexes them by tile),
/// quality is clamped to 0..5 and an unknown or out-of-range material falls back to the
/// material of the engraved tile.
fn repair_engravings(world: &mut World, w: &mut Warnings) {
    if world.engravings.is_empty() {
        return;
    }
    let n_mat = world.materials.len();
    let mut seen = std::collections::HashSet::new();
    let (mut off, mut dup, mut qual, mut mat) = (0usize, 0usize, 0usize, 0usize);
    let mut list = std::mem::take(&mut world.engravings);
    list.retain_mut(|e| {
        let Some(i) = world.idx(e.pos) else {
            off += 1;
            return false;
        };
        if !seen.insert(i) {
            dup += 1;
            return false;
        }
        if e.quality > 5 {
            e.quality = 5;
            qual += 1;
        }
        if e.mat == NO_MAT || e.mat as usize >= n_mat {
            let t = world.tiles[i].mat;
            e.mat = if (t as usize) < n_mat { t } else { NO_MAT };
            mat += 1;
        }
        true
    });
    world.engravings = list;
    if off > 0 {
        w.add("engravings", format!("{off} engravings outside the map dropped"));
    }
    if dup > 0 {
        w.add("engravings", format!("{dup} engravings on an already engraved tile dropped"));
    }
    if qual > 0 {
        w.add("engravings", format!("{qual} engravings with quality above 5 (masterful) clamped"));
    }
    if mat > 0 {
        w.add("engravings", format!("{mat} engravings without a valid material use the tile's material"));
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
    // Round-3 files (optional for the importer; always written here).
    write_json("creature_raws.json", to_json(&world.creature_raws))?;
    write_json("weapon_raws.json", to_json(&world.weapon_raws))?;
    write_json("armor_raws.json", to_json(&world.armor_raws))?;
    write_json("squads.json", to_json(&world.squads))?;
    write_json("burrows.json", to_json(&world.burrows))?;
    // Round-4 files (optional for the importer; always written here).
    write_json("entities.json", to_json(&world.entities))?;
    write_json("nobles.json", to_json(&world.nobles))?;
    write_json("settings.json", to_json(&world.settings))?;
    write_json("hist_figs.json", to_json(&world.hist_figs))?;
    write_json("caravans.json", to_json(&world.caravans))?;
    // Round-5 file (optional for the importer; always written here).
    write_json("engravings.json", to_json(&world.engravings))?;
    // Round-8 files (optional for the importer; always written here).
    write_json("work_details.json", to_json(&world.work_details))?;
    write_json("kitchen.json", to_json(&world.kitchen))?;
    write_json("locations.json", to_json(&world.locations))?;
    write_json("crimes.json", to_json(&world.crimes))?;
    write_json("haul_routes.json", to_json(&world.haul_routes))?;
    // Round-9 file (optional for the importer; written when the world has options).
    if !world.stockpile_options.is_empty() {
        write_json("stockpile_options.json", round9::stockpile_options_json(&world.stockpile_options))?;
    }
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
        ("creature_raws", world.creature_raws.len()),
        ("weapon_raws", world.weapon_raws.len()),
        ("armor_raws", world.armor_raws.len()),
        ("squads", world.squads.len()),
        ("burrows", world.burrows.len()),
        ("burrow_tiles", world.burrows.iter().map(|b| b.tiles.len()).sum()),
        ("unit_needs", world.units.iter().map(|u| u.needs.len()).sum()),
        ("unit_emotions", world.units.iter().map(|u| u.emotions.len()).sum()),
        ("unit_values", world.units.iter().map(|u| u.values.len()).sum()),
        ("unit_relationships", world.units.iter().map(|u| u.relationships.len()).sum()),
        ("unit_wounds", world.units.iter().map(|u| u.wounds.len()).sum()),
        ("entities", world.entities.len()),
        ("nobles", world.nobles.len()),
        ("mandates", world.nobles.iter().map(|n| n.mandates.len()).sum()),
        ("hist_figs", world.hist_figs.len()),
        ("caravans", world.caravans.len()),
        ("building_links", trigger_links(world)),
        ("engravings", world.engravings.len()),
        ("work_details", world.work_details.len()),
        ("kitchen", world.kitchen.len()),
        ("locations", world.locations.len()),
        ("occupations", world.locations.iter().map(|l| l.occupations.len()).sum()),
        ("crimes", world.crimes.len()),
        ("haul_routes", world.haul_routes.len()),
        ("stockpile_filters", round8::filter_count(world)),
        ("pets", world.units.iter().filter(|u| u.pet_owner >= 0).count()),
        ("pastured", world.units.iter().filter(|u| u.pasture >= 0).count()),
        ("slaughter", world.units.iter().filter(|u| u.slaughter).count()),
        ("pregnant", world.units.iter().filter(|u| u.pregnancy > 0).count()),
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

    /// The default export is the newest complete one under the export root.
    #[test]
    fn newest_complete_export_wins() {
        let root = temp_dir("newest");
        assert_eq!(newest_export(&root), None, "missing root");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(newest_export(&root), None, "empty root");
        let t0 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for (name, age, lock) in [("old", 300u64, false), ("new", 100, false), ("running", 0, true), ("loose-file", 0, false)] {
            if name == "loose-file" {
                std::fs::write(root.join("manifest.json"), "{}").unwrap(); // a file, not an export folder
                continue;
            }
            let d = root.join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("manifest.json"), "{}").unwrap();
            let f = std::fs::File::options().write(true).open(d.join("manifest.json")).unwrap();
            f.set_modified(t0 - std::time::Duration::from_secs(age)).unwrap();
            if lock {
                std::fs::write(d.join("export.lock"), "1").unwrap();
            }
        }
        std::fs::create_dir_all(root.join("no-manifest")).unwrap();
        assert_eq!(newest_export(&root), Some(root.join("new")));
        assert!(is_complete_export(&root.join("old")) && !is_complete_export(&root.join("running")));
        // An explicit folder must itself be an export.
        assert_eq!(find_export(Some(&root.join("old"))), Some(root.join("old")));
        assert_eq!(find_export(Some(&root.join("no-manifest"))), None);
        let _ = std::fs::remove_dir_all(&root);
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

    /// A synthetic world with round-3 data: a dwarf raw with a small expanded body, a weapon and
    /// an armor raw, unit psychology/relationships/wounds, a squad, a burrow with tiles, a corpse
    /// with its creature token and a heavy-aquifer tile.
    fn round3_world() -> World {
        use rf_core::{
            AttackRaw, BodyPartRaw, CasteRaw, Emotion, Relationship, TissueLayerRaw, UnitNeed, ValueStrength, Wound,
        };
        let mut w = round2_world();
        let skin = w.materials.iter().position(|m| m.token == "CREATURE:DWARF:SKIN").unwrap() as u16;
        let layer = |t: &str, mat: u16, th: i32| TissueLayerRaw {
            tissue: t.into(),
            material: format!("CREATURE:DWARF:{t}"),
            mat,
            thickness: th,
            flags: vec!["CONNECTS".into()],
        };
        let caste = CasteRaw {
            token: "FEMALE".into(),
            name: "dwarf".into(),
            body_size: 60_000,
            walk_speed: 900,
            attacks: vec![AttackRaw {
                name: "PUNCH".into(),
                verb: "punch".into(),
                body_part: "RH".into(),
                contact_area: 100,
                penetration: 100,
                velocity_multiplier: 1000,
                prepare_ticks: 3,
                recover_ticks: 3,
                main: true,
                flags: vec!["WITH".into()],
            }],
            body_parts: vec![
                BodyPartRaw {
                    token: "UB".into(),
                    name: "upper body".into(),
                    category: "BODY_UPPER".into(),
                    parent: -1,
                    relsize: 1000,
                    flags: vec!["UPPERBODY".into()],
                    layers: vec![layer("SKIN", skin, 17), layer("MUSCLE", NO_MAT, 892)],
                },
                BodyPartRaw {
                    token: "RH".into(),
                    name: "right hand".into(),
                    category: "HAND".into(),
                    parent: 0,
                    relsize: 80,
                    flags: vec!["GRASP".into(), "RIGHT".into()],
                    layers: vec![layer("SKIN", skin, 10)],
                },
            ],
            flags: vec!["CAN_SPEAK".into(), "EQUIPS".into()],
        };
        w.creature_raws = vec![rf_core::CreatureRaw {
            token: "DWARF".into(),
            name: "dwarf".into(),
            name_plural: "dwarves".into(),
            castes: vec![caste.clone(), CasteRaw { token: "MALE".into(), ..caste }],
            flags: vec!["CASTE_CAN_LEARN".into()],
        }];
        w.weapon_raws = vec![WeaponRaw {
            token: "ITEM_WEAPON_AXE_BATTLE".into(),
            name: "battle axe".into(),
            skill: "AXE".into(),
            size: 800,
            two_handed: 47_500,
            minimum_size: 42_500,
            attacks: vec![AttackRaw {
                name: "hack".into(),
                verb: "hack".into(),
                contact_area: 40_000,
                penetration: 6_000,
                velocity_multiplier: 1250,
                prepare_ticks: 3,
                recover_ticks: 3,
                flags: vec!["EDGE".into()],
                ..Default::default()
            }],
            ..Default::default()
        }];
        w.armor_raws = vec![ArmorRaw {
            token: "ITEM_ARMOR_BREASTPLATE".into(),
            name: "breastplate".into(),
            kind: "ARMOR".into(),
            layer: "ARMOR".into(),
            coverage: 100,
            flags: vec!["HARD".into(), "METAL".into()],
        }];
        let u = &mut w.units[0];
        u.needs = vec![UnitNeed { id: "DrinkAlcohol".into(), deity_id: -1, focus_level: 333, need_level: 10 }];
        u.emotions = vec![Emotion {
            emotion: "SATISFACTION".into(),
            thought: "ImproveSkill".into(),
            subthought: 84,
            severity: 0,
            year: 275,
            year_tick: 103_314,
            flags: vec!["VOCALIZED".into()],
            ..Default::default()
        }];
        u.values = vec![ValueStrength { id: "PEACE".into(), strength: 45 }];
        u.relationships = vec![
            Relationship { kind: "Spouse".into(), hist_fig_id: 3888, unit_id: 2 },
            Relationship { kind: "KindredSpirit".into(), hist_fig_id: 3841, unit_id: -1 },
        ];
        u.wounds = vec![Wound {
            body_part: "RH".into(),
            layer: "SKIN".into(),
            flags: vec!["SCAR_CUT".into(), "DIAGNOSED".into()],
            severity: 600,
            age: 15_283_194,
            ..Default::default()
        }];
        u.blood = 4740;
        u.blood_max = 4740;
        u.squad_id = 72;
        u.squad_position = 1;
        u.invader_civ = -1;
        w.squads = vec![Squad {
            id: 72,
            name: String::new(),
            alias: "Wache".into(),
            members: vec![1, -1, 3],
            orders: vec!["Train".into()],
            uniform: vec!["ARMOR@Armor".into(), "WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:IRON".into()],
            schedule: "Constant training".into(),
            ..Default::default() // [military2] new Squad fields
        }];
        w.burrows = vec![Burrow {
            id: 1,
            name: "Zuflucht".into(),
            tiles: vec![Pos::new(1, 1, 2), Pos::new(2, 1, 2), Pos::new(3, 1, 2)],
            units: vec![1, 2],
        }];
        let corpse = rf_core::Item { id: 9000, kind: "CORPSE".into(), subtype: "CAT".into(), mat: NO_MAT, stack: 1, container: -1, holder: -1, building: -1, ..Default::default() };
        w.items.push(corpse);
        w.tiles[5].flags |= TILE_FLAG_HEAVY_AQUIFER | tile_flags::AQUIFER;
        w
    }

    #[test]
    fn round3_data_round_trip() {
        let w = round3_world();
        let dir = temp_dir("round3");
        write_export(&w, &dir).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
        assert_eq!((rep.creature_raws, rep.weapon_raws, rep.armor_raws, rep.squads, rep.burrows, rep.burrow_tiles), (1, 1, 1, 1, 1, 3));
        // creature raws: expanded body with tissue layers and palette materials
        let d = &got.creature_raws[0];
        assert_eq!(d.castes.len(), 2);
        let c = &d.castes[0];
        assert_eq!((c.body_size, c.walk_speed, c.body_parts.len()), (60_000, 900, 2));
        assert_eq!(c.body_parts[1].parent, 0);
        assert_eq!(got.material(c.body_parts[0].layers[0].mat).unwrap().token, "CREATURE:DWARF:SKIN");
        assert_eq!(c.body_parts[0].layers[1].mat, NO_MAT);
        assert!(c.attacks[0].main && c.attacks[0].body_part == "RH");
        assert_eq!(got.weapon_raws[0].attacks[0].flags, vec!["EDGE"]);
        assert_eq!(got.armor_raws[0].layer, "ARMOR");
        // unit psychology, relationships, wounds, blood, squad
        let u = &got.units[0];
        assert_eq!(u.needs[0].need_level, 10);
        assert_eq!(u.emotions[0].thought, "ImproveSkill");
        assert_eq!(u.values[0].strength, 45);
        assert_eq!(u.relationships.len(), 2);
        assert_eq!(u.relationships[0].unit_id, 2);
        assert_eq!(u.wounds[0].body_part, "RH");
        assert_eq!((u.blood, u.blood_max, u.squad_position, u.invader_civ), (4740, 4740, 1, -1));
        // squads and burrows are first-class
        assert_eq!(got.squads[0].alias, "Wache");
        assert_eq!(got.squads[0].members, vec![1, -1, 3]);
        assert_eq!(got.squads[0].orders, vec!["Train"]);
        assert_eq!(got.burrows[0].tiles, w.burrows[0].tiles);
        // corpse token, heavy aquifer bit
        assert_eq!(got.item(9000).unwrap().subtype, "CAT");
        assert_ne!(got.tiles[5].flags & TILE_FLAG_HEAVY_AQUIFER, 0);
        // the summary view still works on round-3 files
        let e = load_extras(&dir);
        assert_eq!((e.burrows[0].tiles, e.squads[0].name.as_str()), (3, "Wache"));
        assert_eq!(e.squads[0].members, vec![1, 3]); // filled positions only, as in round 2
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round3_bad_data_is_repaired() {
        let mut w = round3_world();
        w.creature_raws[0].castes[0].body_parts[1].layers[0].mat = 60_000;
        w.creature_raws[0].castes[0].body_parts[1].parent = 99;
        w.burrows[0].tiles.push(Pos::new(-3, 0, 0));
        w.burrows[0].tiles.push(Pos::new(0, 0, 999));
        let dir = temp_dir("round3bad");
        write_export(&w, &dir).unwrap();
        std::fs::write(dir.join("weapon_raws.json"), "[{broken").unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        let all = rep.warnings.join("\n");
        assert!(all.contains("tissue layer material indices"), "{all}");
        assert!(all.contains("out-of-range parent"), "{all}");
        assert!(all.contains("2 burrow tiles outside the map"), "{all}");
        assert!(all.contains("weapon_raws"), "{all}");
        assert!(got.weapon_raws.is_empty());
        assert_eq!(got.creature_raws[0].castes[0].body_parts[1].layers[0].mat, NO_MAT);
        assert_eq!(got.creature_raws[0].castes[0].body_parts[1].parent, -1);
        assert_eq!(got.burrows[0].tiles.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round2_burrows_and_squads_load_into_world() {
        let w = round2_world();
        let dir = temp_dir("round2bs");
        write_export(&w, &dir).unwrap();
        std::fs::write(dir.join("burrows.json"), r#"[{"id":1,"name":"Zuflucht","tiles":1716,"blocks":18,"units":[5]}]"#).unwrap();
        std::fs::write(dir.join("squads.json"), r#"[{"id":72,"name":"Wache","members":[13957,13834],"member_hfs":[3898,3889],"positions":10}]"#).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        let all = rep.warnings.join("\n");
        assert!(all.contains("burrows without tile lists"), "{all}");
        assert_eq!(got.burrows.len(), 1);
        assert!(got.burrows[0].tiles.is_empty() && got.burrows[0].units == vec![5]);
        assert_eq!(got.squads[0].name, "Wache");
        assert_eq!(got.squads[0].members, vec![13957, 13834]);
        let e = load_extras(&dir);
        assert_eq!(e.burrows[0].tiles, 1716);
        assert_eq!(e.squads[0].member_hfs, vec![3898, 3889]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exporter_style_round3_lines_parse() {
        // As rustfort-export.lua rev 2026-10-10b writes them (extras included, shortened).
        let u: Unit = serde_json::from_str(r#"{"id":13834,"race":"DWARF","caste":"MALE","blood":4740,"blood_max":4740,"squad_position":1,"invader_civ":-1,"body_size":78770,"body_size_raw":7877,"stress_category":6,"needs":[{"id":"BeCreative","deity_id":-1,"focus_level":-20011,"need_level":1}],"emotions":[{"emotion":"PLEASURE","thought":"Spar","subthought":-1,"strength":0,"severity":0,"year":275,"year_tick":293566,"flags":["VOCALIZED","REMEMBERED_SHORTTERM"],"relative_strength":0}],"values":[{"id":"INDEPENDENCE","strength":-8}],"relationships":[{"kind":"KindredSpirit","hist_fig_id":3841,"unit_id":-1,"unit_id_any":13608,"dead":true,"source":"visual","love":100,"trust":0,"respect":0,"loyalty":0,"fear":0,"meet_count":28,"first_year":215,"last_meet_year":258,"last_meet_tick":185940},{"kind":"Deity","hist_fig_id":45,"unit_id":-1,"unit_id_any":-1,"dead":false,"source":"family","strength":14}],"wounds":[{"body_part":"RLA","layer":"SKIN","flags":["DIAGNOSED","SUTURED","SCAR_CUT","EDGED_DAMAGE"],"severity":600,"pain":0,"bleeding":0,"age":15340894,"wound_id":427,"wound_pain":0,"attacker_unit_id":15230,"attacker_hist_figure_id":-1,"syndrome_id":-1,"fever":0,"part_id":6,"layer_idx":0,"global_layer_idx":28,"contact_area":2,"surface_perc":6,"strain":0,"cur_penetration_perc":0,"max_penetration_perc":100,"swelling":0,"impaired":0,"numbness":0,"paralysis":0,"nausea":0,"dizziness":0}]}"#).unwrap();
        assert_eq!((u.needs[0].id.as_str(), u.needs[0].focus_level), ("BeCreative", -20011));
        assert_eq!(u.emotions[0].flags, vec!["VOCALIZED", "REMEMBERED_SHORTTERM"]);
        assert_eq!((u.relationships[0].kind.as_str(), u.relationships[0].hist_fig_id), ("KindredSpirit", 3841));
        assert_eq!((u.wounds[0].body_part.as_str(), u.wounds[0].severity), ("RLA", 600));
        assert_eq!((u.body_size, u.squad_position), (78770, 1));
        let c: CreatureRaw = serde_json::from_str(r#"{"token":"DWARF","name":"dwarf","name_plural":"dwarves","castes":[{"token":"FEMALE","name":"dwarf","body_size":60000,"walk_speed":900,"attacks":[{"name":"PUNCH","verb":"punch","body_part":"LH","contact_area":100,"penetration":100,"velocity_multiplier":1000,"prepare_ticks":3,"recover_ticks":3,"main":true,"flags":["WITH"],"verb_3rd":"punches","skill":"GRASP_STRIKE","perc":true,"body_parts":["LH"],"tissue_layers":[-1]}],"body_parts":[{"token":"UB","name":"upper body","category":"BODY_UPPER","parent":-1,"relsize":1000,"flags":["UPPERBODY","HAS_PREVENTS_PARENT_COLLAPSE_CHILD"],"layers":[{"tissue":"SKIN","material":"CREATURE:DWARF:SKIN","mat":1468,"thickness":17,"flags":["CONNECTS","SCARS"],"tissue_id":"SKIN","healing_rate":100,"vascular":1,"pain_receptors":5,"layer_depth":-1,"relative_thickness":1}],"name_plural":"upper bodies","number":1,"fraction_total":998}],"flags":["CAN_SPEAK"],"body_size_by_age":[[0,3000],[504,15000],[4032,60000]],"total_relsize":5546}],"flags":[],"index":572,"source":["civ","unit"],"tissues":26}"#).unwrap();
        assert_eq!(c.castes[0].body_parts[0].layers[0].mat, 1468);
        assert_eq!(c.castes[0].attacks[0].body_part, "LH");
        let wr: WeaponRaw = serde_json::from_str(r#"{"token":"ITEM_WEAPON_AXE_BATTLE","name":"battle axe","skill":"AXE","size":800,"two_handed":47500,"minimum_size":42500,"ranged_ammo":"","attacks":[{"name":"hack","verb":"hack","body_part":"","contact_area":40000,"penetration":6000,"velocity_multiplier":1250,"prepare_ticks":3,"recover_ticks":3,"main":false,"flags":["EDGE"],"verb_3rd":"hacks","noun":"NO_SUB","edged":true,"perc":false}],"kind":"WEAPON","material_size":4,"value":10,"flags":["HAS_EDGE_ATTACK"]}"#).unwrap();
        assert_eq!((wr.size, wr.attacks[0].contact_area), (800, 40_000));
        let ar: ArmorRaw = serde_json::from_str(r#"{"token":"ITEM_ARMOR_BREASTPLATE","name":"breastplate","kind":"ARMOR","layer":"ARMOR","coverage":100,"flags":["HARD","METAL","SHAPED"],"armorlevel":3,"layer_size":20,"layer_permit":50}"#).unwrap();
        assert_eq!(ar.coverage, 100);
        let s: RawSquad = serde_json::from_str(r#"{"id":72,"name":"","alias":"Wache","members":[13957,13834,-1],"orders":["Train"],"uniform":["ARMOR@Armor","WEAPON:ITEM_WEAPON_AXE_TRAINING"],"schedule":"Constant training","display_name":"Wache","member_hfs":[3898,3889,-1],"positions":3,"explicit_orders":[],"schedule_orders":["Train"],"schedule_month":{"month":8,"name":"","sleep_mode":0,"uniform_mode":0,"orders":[{"order":"Train","min_count":10}]},"routine_id":2,"barracks":[10611]}"#).unwrap();
        assert_eq!(s.squad.members, vec![13957, 13834, -1]);
        let info = s.info();
        assert_eq!((info.name.as_str(), info.members.len()), ("Wache", 2));
        let b: RawBurrow = serde_json::from_str(r#"{"id":0,"name":"Einzel-Versorgung","tiles":[{"x":103,"y":99,"z":101},{"x":104,"y":99,"z":101}],"units":[],"tile_count":2,"blocks":2,"flags":[]}"#).unwrap();
        assert_eq!(b.tile_count(), 2);
        assert_eq!(b.into_burrow().tiles[1], Pos::new(104, 99, 101));
        let m: MaterialInfo = serde_json::from_str(r#"{"mat_type":0,"mat_index":0,"token":"INORGANIC:IRON","name":"iron","density":7850,"shear_yield":155000,"shear_fracture":310000,"shear_strain_at_yield":189,"impact_yield":542500,"impact_fracture":1085000,"impact_strain_at_yield":319,"max_edge":10000,"value":10,"strength":{"SHEAR":[155000,310000,189]},"heat":{"melting_point":12768}}"#).unwrap();
        assert_eq!((m.density, m.impact_fracture, m.max_edge), (7850, 1_085_000, 10_000));
        let man: Manifest = serde_json::from_str(r#"{"stress_cutoffs":[50000,25000,10000,-10000,-25000,-50000,-100000],"max_slice_by_phase":{"units":32}}"#).unwrap();
        assert_eq!(man.stress_category(98_940), Some(0));
        assert_eq!(man.stress_category(-2_850), Some(3));
        assert_eq!(man.stress_category(-82_464), Some(6));
        assert_eq!(man.stress_category(-100_000), Some(6));
        assert_eq!(Manifest::default().stress_category(0), None);
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

    /// A synthetic world with round-4 data, modelled on Trussroads: a locked door into the trap
    /// corridor, the gate bridge with its lever, a loaded weapon trap, civilizations, nobles with a
    /// mandate, settings, historical figure names and a caravan at the depot.
    fn round4_world() -> World {
        let mut w = World::synthetic(24, 24, 10);
        let b = |id: i32, kind: &str, subtype: &str, x: i16, flags: &[&str], links: &[i32], dir: &str| Building {
            id,
            kind: kind.into(),
            subtype: subtype.into(),
            x1: x,
            y1: 3,
            x2: x,
            y2: 3,
            z: 5,
            built: true,
            flags: flags.iter().map(|s| s.to_string()).collect(),
            links: links.to_vec(),
            direction: dir.into(),
            ..Default::default()
        };
        w.buildings.push(b(600, "Door", "", 2, &["LOCKED", "CLOSED"], &[], ""));
        w.buildings.push(b(601, "Trap", "Lever", 4, &["ACTIVE"], &[602], ""));
        w.buildings.push(b(602, "Bridge", "", 6, &["HAS_SUPPORT", "RAISED"], &[601], "S"));
        w.buildings.push(Building { items: vec![1], ..b(603, "Trap", "WeaponTrap", 8, &["LOADED", "ACTIVE"], &[], "") });
        w.buildings.push(b(604, "TradeDepot", "", 10, &["TRADER_REQUESTED"], &[], ""));
        let e = |id: i32, race: &str, kind: &str, relation: &str, trades: bool| EntityInfo {
            id,
            name: format!("Entity {id}"),
            race: race.into(),
            kind: kind.into(),
            relation: relation.into(),
            trades,
        };
        w.entities = vec![
            e(29, "DWARF", "Civilization", "Own", true),
            e(303, "DWARF", "SiteGovernment", "Own", false),
            e(35, "HUMAN", "Civilization", "Peace", true),
            e(39, "GOBLIN", "Civilization", "War", false),
        ];
        let first = w.units[0].id;
        w.nobles = vec![
            NoblePosition {
                token: "MAYOR".into(),
                name: "mayor".into(),
                holder: first,
                entity_id: 303,
                mandates: vec!["Ban export of spears".into()],
            },
            NoblePosition { token: "BROKER".into(), name: "broker".into(), holder: -1, entity_id: 303, mandates: vec![] },
        ];
        w.settings = GameSettings {
            population_cap: 90,
            strict_population_cap: 120,
            baby_cap: 120,
            invasions: true,
            temperature: false,
            weather: true,
            cave_ins: true,
            other: vec!["DIFFICULTY_ENEMIES=Normal".into(), "ENEMY_POP_TRIGGER=20,50,80,110,140".into()],
        };
        w.hist_figs = vec![
            HistFigName { id: 40, name: "Mondul the Dead".into(), race: "DWARF".into(), alive: true, unit_id: -1 },
            HistFigName { id: 3893, name: "Amost Castlelancers".into(), race: "DWARF".into(), alive: true, unit_id: first },
        ];
        w.caravans = vec![Caravan {
            entity_id: 35,
            civ: "Entity 35".into(),
            race: "HUMAN".into(),
            state: "AtDepot".into(),
            leaves_in: 12_000,
            depot: 604,
            goods: vec![1, 2],
            mood: 100,
            ..Default::default()
        }];
        w
    }

    #[test]
    fn round4_data_round_trip() {
        let w = round4_world();
        let dir = temp_dir("round4");
        write_export(&w, &dir).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
        assert_eq!((rep.entities, rep.nobles, rep.hist_figs, rep.caravans, rep.building_links), (4, 2, 2, 1, 1));
        assert!(rep.settings);
        let door = got.building(600).unwrap();
        assert!(door.flags.iter().any(|f| f == "LOCKED"));
        let lever = got.building(601).unwrap();
        assert!(is_trigger(lever) && lever.links == vec![602] && lever.flags == vec!["ACTIVE"]);
        let bridge = got.building(602).unwrap();
        assert_eq!((bridge.links.clone(), bridge.direction.as_str()), (vec![601], "S"));
        assert!(bridge.flags.iter().any(|f| f == "RAISED"));
        assert_eq!(got.entities.iter().filter(|e| e.relation == "War").count(), 1);
        assert!(got.entities.iter().any(|e| e.race == "HUMAN" && e.trades));
        assert_eq!(got.nobles[0].mandates, vec!["Ban export of spears"]);
        assert_eq!(got.nobles[1].holder, -1);
        assert_eq!((got.settings.population_cap, got.settings.temperature, got.settings.invasions), (90, false, true));
        assert_eq!(got.settings.other.len(), 2);
        assert_eq!(got.hist_figs[1].unit_id, got.units[0].id);
        assert_eq!((got.caravans[0].state.as_str(), got.caravans[0].depot, got.caravans[0].leaves_in), ("AtDepot", 604, 12_000));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_without_round4_files_gets_df_defaults() {
        let w = round4_world();
        let dir = temp_dir("noround4");
        write_export(&w, &dir).unwrap();
        for f in ["entities.json", "nobles.json", "settings.json", "hist_figs.json", "caravans.json"] {
            std::fs::remove_file(dir.join(f)).unwrap();
        }
        let (got, rep) = load_export(&dir).unwrap();
        assert!(got.entities.is_empty() && got.nobles.is_empty() && got.hist_figs.is_empty() && got.caravans.is_empty());
        assert!(!rep.settings);
        // DF's d_init defaults, so an older export keeps migrants and invasions
        assert_eq!((got.settings.population_cap, got.settings.invasions, got.settings.temperature), (200, true, true));
        // the manifest counts the round-4 lists, so the importer reports the difference
        assert!(rep.warnings.iter().any(|w| w.contains("entities: manifest says 4, imported 0")), "{:?}", rep.warnings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round4_links_are_repaired() {
        let mut w = round4_world();
        // a dangling link, a self link, a duplicate, and a one-sided link (target does not list the lever)
        w.buildings.iter_mut().find(|b| b.id == 601).unwrap().links = vec![602, 602, 601, 9999, 600];
        let dir = temp_dir("round4links");
        write_export(&w, &dir).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        assert_eq!(got.building(601).unwrap().links, vec![602, 600]);
        assert_eq!(got.building(600).unwrap().links, vec![601]);
        let all = rep.warnings.join("\n");
        assert!(all.contains("links to missing buildings") && all.contains("made symmetric"), "{all}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exporter_style_round4_lines_parse() {
        // Lines as the Lua exporter writes them (rev 2026-10-10c), extras included.
        let b: Building = serde_json::from_str(r#"{"id":11814,"kind":"Bridge","subtype":"","x1":103,"y1":100,"x2":103,"y2":100,"z":131,"center":{"x":103,"y":100,"z":131},"mat":101,"stage":3,"built":true,"owner":-1,"name":"","stockpile":null,"extents":[],"items":[1182366,921623],"jobs":[],"crops":[],"flags":["HAS_SUPPORT","LOWERED"],"links":[11808],"direction":"S","link_targets":[],"link_triggers":[11808],"timer":0,"material_amount":1}"#).unwrap();
        assert_eq!((b.links.clone(), b.direction.as_str(), b.flags.len()), (vec![11808], "S", 2));
        let t: Building = serde_json::from_str(r#"{"id":11753,"kind":"Trap","subtype":"WeaponTrap","x1":94,"y1":99,"x2":94,"y2":99,"z":128,"items":[926057,919630,1165898,921590],"flags":["ACTIVE","LOADED"],"links":[],"direction":"","link_targets":[],"link_triggers":[],"trap_items":[{"id":926057,"kind":"WEAPON","subtype":"ITEM_WEAPON_AXE_BATTLE","use_mode":2}],"trap_state":0,"ready_timeout":0,"fill_timer":0}"#).unwrap();
        assert!(t.flags.iter().any(|f| f == "LOADED") && t.items.len() == 4);
        let e: EntityInfo = serde_json::from_str(r#"{"id":35,"name":"The Mindful Realms","race":"HUMAN","kind":"Civilization","relation":"Peace","trades":true,"name_native":"x","parent":-1,"reasons":["civilization","diplomacy","trade","units"],"diplomacy_flags":["ALLIES"],"war_event_collection":-1,"caravans_sent":12,"first_caravan_year":264,"last_caravan_year":275,"units_on_map":23,"hostility_level":0,"siege_tier":0,"deities":[]}"#).unwrap();
        assert!(e.trades && e.relation == "Peace");
        let n: RawNoble = serde_json::from_str(r#"{"token":"MAYOR","name":"mayor","holder":13937,"entity_id":303,"mandates":["Ban export of spears"],"holder_hf":3893,"holder_unit_any":13937,"holder_name":"Amost Castlelancers","assignment_id":7,"position_id":7,"squad_id":-1,"responsibilities":["MEET_WORKERS"],"flags":[],"precedence":0,"number":1,"mandate_max":1,"demand_max":1,"required_bedroom":0,"mandates_detail":[{"text":"Ban export of spears","mode":"Export","item_type":"WEAPON","item_subtype":"ITEM_WEAPON_SPEAR","mat":"","amount_total":1,"amount_remaining":1,"timeout_counter":10,"timeout_limit":19515,"punishment_hammerstrikes":2,"punishment_prison_time":1}]}"#).unwrap();
        assert_eq!((n.noble.holder, n.mandates_detail[0].timeout_limit), (13937, 19515));
        let s: GameSettings = serde_json::from_str(r#"{"population_cap":90,"strict_population_cap":120,"baby_cap":120,"invasions":true,"temperature":false,"weather":true,"cave_ins":true,"other":["DIFFICULTY_ENEMIES=Normal"],"baby_cap_percent":1000,"visitor_cap":300,"artifacts":true,"difficulty_enemies":"Normal","source":"x"}"#).unwrap();
        assert_eq!((s.population_cap, s.temperature), (90, false));
        let h: HistFigName = serde_json::from_str(r#"{"id":40,"name":"Mondul the Dead","race":"DWARF","alive":true,"unit_id":-1,"name_native":"x","unit_id_any":-1,"sex":1,"born_year":-1,"died_year":-1,"civ_id":-1,"deity":true,"profession":"NONE","spheres":["DEATH"]}"#).unwrap();
        assert!(h.alive && h.unit_id == -1);
        let c: Caravan = serde_json::from_str(r#"{"entity_id":29,"civ":"The Channels of Direction","race":"DWARF","state":"Leaving","leaves_in":0,"depot":11168,"goods":[1,2,3],"animals":[5,6],"merchants":[],"import_value":29536,"export_value":0,"mood":100,"flags":[],"time_remaining":0,"depot_notified":0}"#).unwrap();
        assert_eq!((c.state.as_str(), c.goods.len(), c.depot), ("Leaving", 3, 11168));
        let d: DiplomatInfo = serde_json::from_str(r#"{"civ_id":29,"diplomat_hf":4056,"diplomat_unit":18290,"associate_hf":-1,"associate_unit":-1,"flags":[],"source":"meeting"}"#).unwrap();
        assert_eq!(d.diplomat_unit, 18290);
    }

    #[test]
    fn extras_read_round4_diplomats_and_mandates() {
        let dir = temp_dir("extras4");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("diplomats.json"), r#"[{"civ_id":29,"diplomat_hf":4056,"diplomat_unit":18290,"associate_hf":-1,"associate_unit":-1,"flags":[],"source":"meeting"}]"#).unwrap();
        std::fs::write(dir.join("nobles.json"), r#"[{"token":"MAYOR","holder":13937,"entity_id":303,"mandates":["Make 1 spear"],"mandates_detail":[{"text":"Make 1 spear","mode":"Make","amount_total":1,"amount_remaining":1}]},{"token":"BROKER","holder":-1}]"#).unwrap();
        let x = load_extras(&dir);
        assert_eq!(x.diplomats.len(), 1);
        assert_eq!(x.mandates.len(), 1);
        assert_eq!((x.mandates[0].noble, x.mandates[0].position.as_str(), x.mandates[0].mode.as_str()), (13937, "MAYOR", "Make"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round5_engravings_load_from_the_export() {
        let mut w = World::synthetic(16, 16, 6);
        // the engraved floor tile (3,4,2) is granite (palette 0), the wall (5,4,2) magnetite (2)
        let fi = w.idx(Pos::new(3, 4, 2)).unwrap();
        w.tiles[fi].mat = 0;
        let wi = w.idx(Pos::new(5, 4, 2)).unwrap();
        w.tiles[wi].mat = 2;
        let dir = temp_dir("round5engr");
        write_export(&w, &dir).unwrap();
        // the DFHack exporter's layout, with its extra fields
        std::fs::write(
            dir.join("engravings.json"),
            r#"[
{"pos":{"x":3,"y":4,"z":2},"quality":4,"floor":true,"mat":0,"engraver":7,"skill":12,"artist_hf":3886,"sides":[],"hidden":false,"art_id":28,"art_subid":165,"masterpiece_event":-1},
{"pos":{"x":5,"y":4,"z":2},"quality":9,"floor":false,"mat":65535,"engraver":-1,"skill":0,"artist_hf":3915,"sides":["east"],"hidden":false,"art_id":1,"art_subid":2,"masterpiece_event":-1},
{"pos":{"x":5,"y":4,"z":2},"quality":1,"floor":false,"mat":0,"engraver":-1,"skill":0},
{"pos":{"x":3,"y":40,"z":2},"quality":1,"floor":true,"mat":0,"engraver":-1,"skill":0}
]"#,
        )
        .unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        assert_eq!(rep.engravings, 2);
        assert_eq!(got.engravings.len(), 2);
        let f = &got.engravings[0];
        assert_eq!((f.pos, f.quality, f.floor, f.mat, f.engraver, f.skill), (Pos::new(3, 4, 2), 4, true, 0, 7, 12));
        let wl = &got.engravings[1];
        // quality clamped to masterful, missing material taken from the engraved wall tile
        assert_eq!((wl.pos, wl.quality, wl.floor, wl.mat, wl.engraver), (Pos::new(5, 4, 2), 5, false, 2, -1));
        for frag in ["1 engravings outside the map", "1 engravings on an already engraved tile", "clamped", "tile's material"] {
            assert!(rep.warnings.iter().any(|x| x.contains(frag)), "{frag}: {:?}", rep.warnings);
        }
        // round trip through write_export keeps them
        let dir2 = temp_dir("round5engr2");
        write_export(&got, &dir2).unwrap();
        let (again, rep2) = load_export(&dir2).unwrap();
        assert_eq!(again.engravings.len(), 2);
        assert!(!rep2.warnings.iter().any(|x| x.contains("engravings")), "{:?}", rep2.warnings);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn export_without_engravings_file_loads_none() {
        let w = World::synthetic(16, 16, 6);
        let dir = temp_dir("noengr");
        write_export(&w, &dir).unwrap();
        std::fs::remove_file(dir.join("engravings.json")).unwrap();
        let (got, rep) = load_export(&dir).unwrap();
        assert!(got.engravings.is_empty());
        assert_eq!(rep.engravings, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
