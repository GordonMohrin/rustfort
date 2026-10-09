//! rf-raws: parser for DF raw files (objects + graphics raws) read from the user's own DF install.
//!
//! DF raw syntax: the first line is the file name, `[OBJECT:TYPE]` declares what the file
//! contains, and every `[TOKEN:arg:arg...]` anywhere in the text is a token. Everything outside
//! brackets is a comment. Objects start at a type-specific token (`[CREATURE:DWARF]`,
//! `[INORGANIC:GRANITE]`, `[TILE_PAGE:FLOORS]`, ...) and run until the next object start.
//!
//! This crate keeps every object's full token list, so later code can interpret more of it
//! without touching the parser. It also has small typed views for the graphics raws that
//! rf-gfx needs (`TILE_PAGE`, `TILE_GRAPHICS`, `PALETTE`, `DESCRIPTOR_COLOR`).
//!
//! Usage:
//! ```no_run
//! let raws = rf_raws::Raws::load_df_dir(&rf_raws::df_dir(), &rf_raws::LoadOptions::default()).unwrap();
//! println!("{} creatures", raws.count("CREATURE"));
//! let dwarf = raws.get("CREATURE", "DWARF").unwrap();
//! println!("{:?}", dwarf.first("NAME"));
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod graphics;
pub use graphics::*;

/// Default DF install path on Gordon's machine; override with env `RUSTFORT_DF_DIR`.
pub const DEFAULT_DF_DIR: &str = r"E:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress";

pub fn df_dir() -> PathBuf {
    std::env::var_os("RUSTFORT_DF_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DF_DIR))
}

/// One `[NAME:arg:arg]` token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub name: String,
    pub args: Vec<String>,
}

impl Token {
    pub fn arg(&self, i: usize) -> &str {
        self.args.get(i).map(|s| s.as_str()).unwrap_or("")
    }
    pub fn arg_i32(&self, i: usize) -> Option<i32> {
        self.args.get(i).and_then(|s| s.trim().parse().ok())
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}", self.name)?;
        for a in &self.args {
            write!(f, ":{a}")?;
        }
        f.write_str("]")
    }
}

/// One raw object, e.g. `[CREATURE:DWARF]` with all tokens up to the next object start.
#[derive(Clone, Debug)]
pub struct RawObject {
    /// The `[OBJECT:TYPE]` of the file, e.g. `CREATURE`, `ITEM`, `GRAPHICS`, `TILE_PAGE`.
    pub object_type: String,
    /// The object start token name, e.g. `CREATURE`, `ITEM_WEAPON`, `TILE_GRAPHICS`, `COLOR`.
    pub kind: String,
    /// First argument of the start token (the object id), e.g. `DWARF`.
    pub id: String,
    /// All tokens of the object, starting with the start token itself.
    pub tokens: Vec<Token>,
    /// Index into [`Raws::files`].
    pub file: u32,
}

impl RawObject {
    pub fn header(&self) -> &Token {
        &self.tokens[0]
    }
    /// First token with this name (excluding the header).
    pub fn first(&self, name: &str) -> Option<&Token> {
        self.tokens[1..].iter().find(|t| t.name == name)
    }
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Token> + 'a {
        self.tokens[1..].iter().filter(move |t| t.name == name)
    }
    pub fn has(&self, name: &str) -> bool {
        self.first(name).is_some()
    }
}

/// One parsed raw file.
#[derive(Clone, Debug)]
pub struct RawFile {
    pub path: PathBuf,
    /// The module folder this file belongs to, e.g. `vanilla_creatures` or `perspective_walls (25)`.
    pub module: String,
    /// The file name from the first line.
    pub name: String,
    /// `[OBJECT:TYPE]` of the file (empty if none).
    pub object_type: String,
}

/// Which raws to load.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// Load `data/installed_mods/*` after vanilla (highest NUMERIC_VERSION per mod ID wins).
    pub include_mods: bool,
    /// If set, only load these mod ids (from each mod's `info.txt` `[ID:...]`).
    pub mod_filter: Option<Vec<String>>,
    /// Load `objects/` folders.
    pub objects: bool,
    /// Load `graphics/` folders.
    pub graphics: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        LoadOptions { include_mods: false, mod_filter: None, objects: true, graphics: true }
    }
}

#[derive(Debug)]
pub struct RawsError(pub String);
impl std::fmt::Display for RawsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RawsError {}

/// The loaded raw database.
#[derive(Clone, Debug, Default)]
pub struct Raws {
    pub files: Vec<RawFile>,
    pub objects: Vec<RawObject>,
    /// (kind, id) -> object index. Later definitions (mods) replace earlier ones.
    index: HashMap<(String, String), usize>,
    /// kind -> object indices in load order (only the live definition of each id).
    by_kind: HashMap<String, Vec<usize>>,
    /// Module search paths in load order (each is a `.../<module>/` folder).
    pub modules: Vec<PathBuf>,
}

/// Object start tokens per `[OBJECT:TYPE]`.
fn is_object_start(object_type: &str, tok: &str) -> bool {
    match object_type {
        "ITEM" => tok.starts_with("ITEM_"),
        "BUILDING" => tok.starts_with("BUILDING_"),
        "BODY" => tok == "BODY" || tok == "BODYGLOSS",
        "DESCRIPTOR_COLOR" => tok == "COLOR",
        "DESCRIPTOR_SHAPE" => tok == "SHAPE",
        "DESCRIPTOR_PATTERN" => tok == "COLOR_PATTERN",
        "LANGUAGE" => matches!(tok, "WORD" | "SYMBOL" | "TRANSLATION"),
        "GRAPHICS" => {
            tok.ends_with("_GRAPHICS")
                || tok.ends_with("_RECTANGLE")
                || tok == "LAYER_SET_TEMPLATE"
                || tok.starts_with("SHAPE_GRAPHICS")
        }
        "TEXT_SET" => tok == "TEXT_SET",
        t => tok == t,
    }
}

const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û',
    'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡',
    '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─',
    '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█',
    '▄', '▌', '▐', '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±', '≥',
    '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

fn decode_cp437(b: &[u8]) -> String {
    if b.is_ascii() {
        // SAFETY-free fast path: ASCII is valid UTF-8.
        return String::from_utf8(b.to_vec()).unwrap_or_default();
    }
    b.iter().map(|&c| if c < 128 { c as char } else { CP437_HIGH[(c - 128) as usize] }).collect()
}

/// Tokenize raw text: returns (first line, tokens). Works on bytes (raws are CP437).
pub fn tokenize(bytes: &[u8]) -> (String, Vec<Token>) {
    let first_end = bytes.iter().position(|&c| c == b'\n' || c == b'\r').unwrap_or(bytes.len());
    let first = decode_cp437(&bytes[..first_end]).trim().trim_start_matches('\u{feff}').to_string();
    let mut toks = Vec::new();
    let mut i = first_end;
    let n = bytes.len();
    while i < n {
        // find '['
        match memchr(b'[', &bytes[i..]) {
            None => break,
            Some(o) => i += o + 1,
        }
        let start = i;
        let end = match memchr(b']', &bytes[i..]) {
            None => break,
            Some(o) => i + o,
        };
        // A '[' inside the token means the earlier '[' was a stray comment char.
        let inner = &bytes[start..end];
        let inner = match inner.iter().rposition(|&c| c == b'[') {
            Some(p) => &inner[p + 1..],
            None => inner,
        };
        i = end + 1;
        if inner.is_empty() {
            continue;
        }
        let mut parts = inner.split(|&c| c == b':');
        let name = decode_cp437(parts.next().unwrap_or_default()).trim().to_string();
        if name.is_empty() {
            continue;
        }
        let args = parts.map(decode_cp437).collect();
        toks.push(Token { name, args });
    }
    (first, toks)
}

#[inline]
fn memchr(needle: u8, hay: &[u8]) -> Option<usize> {
    hay.iter().position(|&c| c == needle)
}

/// Split a token stream into objects. Tokens before the first object start are dropped.
fn split_objects(object_type: &str, toks: Vec<Token>, file: u32, out: &mut Vec<RawObject>) {
    let mut cur: Option<RawObject> = None;
    for t in toks {
        if t.name == "OBJECT" {
            continue;
        }
        if is_object_start(object_type, &t.name) {
            if let Some(o) = cur.take() {
                out.push(o);
            }
            let id = match t.name.as_str() {
                // TILE_GRAPHICS:page:x:y:NAME[...] -> id = NAME
                "TILE_GRAPHICS" => t.arg(3).to_string(),
                "TILE_GRAPHICS_RECTANGLE" => t.arg(5).to_string(),
                _ => t.arg(0).to_string(),
            };
            cur = Some(RawObject { object_type: object_type.to_string(), kind: t.name.clone(), id, tokens: vec![t], file });
        } else if let Some(o) = cur.as_mut() {
            o.tokens.push(t);
        }
    }
    if let Some(o) = cur.take() {
        out.push(o);
    }
}

/// Parse one raw file's bytes.
pub fn parse_file(bytes: &[u8], file: u32) -> (String, String, Vec<RawObject>) {
    let (first, toks) = tokenize(bytes);
    let object_type = toks.iter().find(|t| t.name == "OBJECT").map(|t| t.arg(0).to_string()).unwrap_or_default();
    let mut objs = Vec::new();
    split_objects(&object_type, toks, file, &mut objs);
    (first, object_type, objs)
}

fn list_txt(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut v: Vec<PathBuf> =
            rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("txt")).unwrap_or(false)).collect();
        v.sort();
        out.extend(v);
    }
}

/// Info of an installed mod (from its `info.txt`).
#[derive(Clone, Debug)]
pub struct ModInfo {
    pub dir: PathBuf,
    pub id: String,
    pub numeric_version: i32,
    pub name: String,
}

/// Lists `data/installed_mods`, keeping only the highest numeric version per mod id.
pub fn installed_mods(df_dir: &Path) -> Vec<ModInfo> {
    let mut best: HashMap<String, ModInfo> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir(df_dir.join("data").join("installed_mods")) {
        for e in rd.flatten() {
            let dir = e.path();
            let Ok(bytes) = std::fs::read(dir.join("info.txt")) else { continue };
            let (_, toks) = tokenize(&bytes);
            let get = |n: &str| toks.iter().find(|t| t.name == n).map(|t| t.arg(0).to_string()).unwrap_or_default();
            let mi = ModInfo {
                dir: dir.clone(),
                id: get("ID"),
                numeric_version: get("NUMERIC_VERSION").parse().unwrap_or(0),
                name: get("NAME"),
            };
            if mi.id.is_empty() {
                continue;
            }
            match best.get(&mi.id) {
                Some(b) if b.numeric_version >= mi.numeric_version => {}
                _ => {
                    best.insert(mi.id.clone(), mi);
                }
            }
        }
    }
    let mut v: Vec<ModInfo> = best.into_values().collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

impl Raws {
    /// Load vanilla raws (and optionally installed mods) from a DF install folder.
    pub fn load_df_dir(df_dir: &Path, opts: &LoadOptions) -> Result<Raws, RawsError> {
        let vanilla = df_dir.join("data").join("vanilla");
        let mut modules: Vec<PathBuf> = Vec::new();
        let rd = std::fs::read_dir(&vanilla).map_err(|e| RawsError(format!("{}: {e}", vanilla.display())))?;
        let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        v.sort();
        modules.extend(v);
        if opts.include_mods {
            for m in installed_mods(df_dir) {
                if let Some(f) = &opts.mod_filter {
                    if !f.iter().any(|x| x == &m.id) {
                        continue;
                    }
                }
                modules.push(m.dir);
            }
        }
        Self::load_modules(&modules, opts)
    }

    /// Load the given module folders in order (each may contain `objects/` and `graphics/`).
    pub fn load_modules(modules: &[PathBuf], opts: &LoadOptions) -> Result<Raws, RawsError> {
        let mut paths: Vec<(PathBuf, String)> = Vec::new();
        for m in modules {
            let mname = m.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let mut ps = Vec::new();
            if opts.objects {
                list_txt(&m.join("objects"), &mut ps);
            }
            if opts.graphics {
                list_txt(&m.join("graphics"), &mut ps);
            }
            paths.extend(ps.into_iter().map(|p| (p, mname.clone())));
        }
        // Parse files in parallel (results are put back in load order, so this is deterministic).
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
        let chunk = paths.len().div_ceil(threads.max(1)).max(1);
        let mut results: Vec<Option<(String, String, Vec<RawObject>)>> = Vec::with_capacity(paths.len());
        results.resize_with(paths.len(), || None);
        std::thread::scope(|s| {
            for (ci, (pchunk, rchunk)) in paths.chunks(chunk).zip(results.chunks_mut(chunk)).enumerate() {
                s.spawn(move || {
                    for (j, ((p, _), r)) in pchunk.iter().zip(rchunk.iter_mut()).enumerate() {
                        if let Ok(bytes) = std::fs::read(p) {
                            *r = Some(parse_file(&bytes, (ci * chunk + j) as u32));
                        }
                    }
                });
            }
        });
        let mut raws = Raws { modules: modules.to_vec(), ..Default::default() };
        for ((path, module), r) in paths.into_iter().zip(results) {
            let (name, object_type, objs) = r.unwrap_or_default();
            let fi = raws.files.len() as u32;
            raws.files.push(RawFile { path, module, name, object_type });
            for mut o in objs {
                o.file = fi;
                raws.objects.push(o);
            }
        }
        raws.reindex();
        Ok(raws)
    }

    /// Parse raws from in-memory texts (for tests). Each entry is (file name, contents).
    pub fn from_texts(texts: &[(&str, &str)]) -> Raws {
        let mut raws = Raws::default();
        for (name, text) in texts {
            let fi = raws.files.len() as u32;
            let (first, object_type, objs) = parse_file(text.as_bytes(), fi);
            raws.files.push(RawFile { path: PathBuf::from(name), module: String::new(), name: first, object_type });
            raws.objects.extend(objs);
        }
        raws.reindex();
        raws
    }

    fn reindex(&mut self) {
        self.index.clear();
        self.by_kind.clear();
        for (i, o) in self.objects.iter().enumerate() {
            // Graphics objects add up instead of replacing each other: DF merges e.g. the
            // CREATURE_GRAPHICS:DWARF of the sprite file and of the portrait file, and
            // TILE_GRAPHICS names legitimately repeat. Keep all of those in by_kind.
            let key = (o.kind.clone(), o.id.clone());
            let multi = o.object_type == "GRAPHICS";
            if !multi {
                if let Some(&old) = self.index.get(&key) {
                    // A later definition replaces the earlier one (mod override).
                    if let Some(v) = self.by_kind.get_mut(&o.kind) {
                        v.retain(|&x| x != old);
                    }
                }
            }
            self.index.insert(key, i);
            self.by_kind.entry(o.kind.clone()).or_default().push(i);
        }
    }

    /// Object by start-token kind and id, e.g. `get("CREATURE", "DWARF")`.
    pub fn get(&self, kind: &str, id: &str) -> Option<&RawObject> {
        self.index.get(&(kind.to_string(), id.to_string())).map(|&i| &self.objects[i])
    }

    /// All objects with this kind and id (graphics objects can have several), in load order.
    pub fn all_of<'a>(&'a self, kind: &'a str, id: &'a str) -> impl Iterator<Item = &'a RawObject> + 'a {
        self.of_kind(kind).filter(move |o| o.id == id)
    }

    /// All live objects of a start-token kind, in load order.
    pub fn of_kind<'a>(&'a self, kind: &str) -> impl Iterator<Item = &'a RawObject> + 'a {
        self.by_kind.get(kind).into_iter().flat_map(move |v| v.iter().map(move |&i| &self.objects[i]))
    }

    /// Number of live objects of a kind.
    pub fn count(&self, kind: &str) -> usize {
        self.by_kind.get(kind).map(|v| v.len()).unwrap_or(0)
    }

    /// Counts per start-token kind, sorted by kind.
    pub fn stats(&self) -> Vec<(String, usize)> {
        let mut v: Vec<(String, usize)> = self.by_kind.iter().map(|(k, v)| (k.clone(), v.len())).collect();
        v.sort();
        v
    }

    /// Counts per `[OBJECT:TYPE]`, sorted by type.
    pub fn stats_by_object_type(&self) -> Vec<(String, usize)> {
        let mut m: HashMap<&str, usize> = HashMap::new();
        for v in self.by_kind.values() {
            for &i in v {
                *m.entry(self.objects[i].object_type.as_str()).or_default() += 1;
            }
        }
        let mut v: Vec<(String, usize)> = m.into_iter().map(|(k, n)| (k.to_string(), n)).collect();
        v.sort();
        v
    }

    /// The folder of the file an object came from (image paths in graphics raws are relative to it).
    pub fn file_dir(&self, obj: &RawObject) -> PathBuf {
        self.files[obj.file as usize].path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    }

    pub fn total_tokens(&self) -> usize {
        self.objects.iter().map(|o| o.tokens.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_with_comments() {
        let txt = "creature_test\n\nsome comment [OBJECT:CREATURE]\n[CREATURE:DWARF] blah\n\t[NAME:dwarf:dwarves:dwarven]\n[CASTE:FEMALE]\n[CREATURE:CAT][NAME:cat:cats:cat]";
        let r = Raws::from_texts(&[("t", txt)]);
        assert_eq!(r.files[0].name, "creature_test");
        assert_eq!(r.files[0].object_type, "CREATURE");
        assert_eq!(r.count("CREATURE"), 2);
        let d = r.get("CREATURE", "DWARF").unwrap();
        assert_eq!(d.first("NAME").unwrap().args, vec!["dwarf", "dwarves", "dwarven"]);
        assert!(d.has("CASTE"));
        assert!(!r.get("CREATURE", "CAT").unwrap().has("CASTE"));
    }

    #[test]
    fn later_definition_overrides() {
        let a = "a\n[OBJECT:INORGANIC][INORGANIC:X][STATE_COLOR:ALL_SOLID:GRAY]";
        let b = "b\n[OBJECT:INORGANIC][INORGANIC:X][STATE_COLOR:ALL_SOLID:RED]";
        let r = Raws::from_texts(&[("a", a), ("b", b)]);
        assert_eq!(r.count("INORGANIC"), 1);
        assert_eq!(r.get("INORGANIC", "X").unwrap().first("STATE_COLOR").unwrap().arg(1), "RED");
    }

    #[test]
    fn graphics_objects() {
        let g = "g\n[OBJECT:GRAPHICS]\n[TILE_GRAPHICS:FLOORS:0:3:STONE_FLOOR_1]\n[TILE_GRAPHICS:FLOORS:1:3:STONE_FLOOR_2]***comment\n[CREATURE_GRAPHICS:CAT][DEFAULT:CATS:0:0:AS_IS:DEFAULT]";
        let r = Raws::from_texts(&[("g", g)]);
        assert_eq!(r.count("TILE_GRAPHICS"), 2);
        assert_eq!(r.get("TILE_GRAPHICS", "STONE_FLOOR_2").unwrap().header().arg(1), "1");
        assert_eq!(r.get("CREATURE_GRAPHICS", "CAT").unwrap().tokens.len(), 2);
    }

    #[test]
    fn cp437_decoding() {
        let bytes = b"x\n[OBJECT:LANGUAGE][WORD:A][NOUN:\x81:\x94]";
        let (_, toks) = tokenize(bytes);
        assert_eq!(toks[2].args, vec!["ü", "ö"]);
    }
}
