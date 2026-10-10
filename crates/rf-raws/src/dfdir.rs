//! Where the Dwarf Fortress install is.
//!
//! A folder counts as a DF install when it contains `data/vanilla` (the DF v50 layout).
//! Lookup order; the first folder that is a DF install wins:
//!
//! 1. the environment variable `RUSTFORT_DF_DIR` (read again on every call);
//! 2. the folder chosen in the app's Settings ([`set_settings_df_dir`]);
//! 3. the Steam install: on Windows the registry values `HKCU\Software\Valve\Steam`
//!    `SteamPath` and `HKLM\SOFTWARE\WOW6432Node\Valve\Steam` `InstallPath`; on Linux
//!    `~/.steam/steam`, `~/.local/share/Steam` and the Flatpak folder
//!    `~/.var/app/com.valvesoftware.Steam/.local/share/Steam`. For each Steam folder, the folder
//!    itself and every library `path` of its `steamapps/libraryfolders.vdf` are tried as
//!    `<library>/steamapps/common/Dwarf Fortress`;
//! 4. common default paths: `C:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress`,
//!    `C:\Program Files\Steam\steamapps\common\Dwarf Fortress` and
//!    `<drive>:\SteamLibrary\steamapps\common\Dwarf Fortress` for the drives C to H.
//!
//! Steps 3 and 4 run once per process (cached). `RUSTFORT_NO_AUTODETECT=1` switches them off
//! (for tests that must behave as on a machine without DF).

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// [`df_dir_status`] source: the environment variable `RUSTFORT_DF_DIR`.
pub const SRC_ENV: &str = "RUSTFORT_DF_DIR";
/// [`df_dir_status`] source: the folder chosen in the app's Settings.
pub const SRC_SETTINGS: &str = "settings";
/// [`df_dir_status`] source: found through the Steam install (registry or Steam folders).
pub const SRC_STEAM: &str = "Steam";
/// [`df_dir_status`] source: one of the common default paths.
pub const SRC_DEFAULT: &str = "default path";
/// [`df_dir_status`] source: no DF install was found.
pub const SRC_NOT_FOUND: &str = "not found";

/// Steam's folder name of DF inside `steamapps/common`.
const DF_FOLDER: &str = "Dwarf Fortress";

static SETTINGS_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);
static AUTO: OnceLock<Option<(PathBuf, &'static str)>> = OnceLock::new();

/// Does `dir` look like a DF v50 install (`data/vanilla` exists)?
pub fn is_df_install(dir: &Path) -> bool {
    !dir.as_os_str().is_empty() && dir.join("data").join("vanilla").is_dir()
}

/// Set (or clear with `None` / an empty path) the folder chosen in the app's Settings.
pub fn set_settings_df_dir(dir: Option<&Path>) {
    let v = dir.and_then(|d| clean(&d.to_string_lossy()));
    if let Ok(mut g) = SETTINGS_DIR.write() {
        *g = v;
    }
}

/// The folder set with [`set_settings_df_dir`], if any (whether or not it is a DF install).
pub fn settings_df_dir() -> Option<PathBuf> {
    SETTINGS_DIR.read().ok().and_then(|g| g.clone())
}

/// `RUSTFORT_DF_DIR`, if set and not empty (surrounding quotes and spaces are ignored).
pub fn env_df_dir() -> Option<PathBuf> {
    std::env::var_os("RUSTFORT_DF_DIR").and_then(|v| clean(&v.to_string_lossy()))
}

/// `RUSTFORT_NO_AUTODETECT` is set (to anything but `0`): no Steam or default-path lookup.
pub fn autodetect_disabled() -> bool {
    std::env::var("RUSTFORT_NO_AUTODETECT").map(|v| !v.trim().is_empty() && v.trim() != "0").unwrap_or(false)
}

fn clean(s: &str) -> Option<PathBuf> {
    let t = s.trim().trim_matches('"').trim();
    (!t.is_empty()).then(|| PathBuf::from(t))
}

/// The DF install and where it was found: `(Some(folder), source)` with source
/// `"RUSTFORT_DF_DIR"`, `"settings"`, `"Steam"` or `"default path"`, or `(None, "not found")`.
pub fn df_dir_status() -> (Option<PathBuf>, &'static str) {
    resolve(env_df_dir(), settings_df_dir(), || if autodetect_disabled() { None } else { AUTO.get_or_init(autodetect).clone() })
}

/// The DF install folder. When none is found this is still a path (the configured folder, or
/// the usual Steam location) so messages can name it; every read from it then fails, and
/// callers fall back (no graphics, built-in font and keys). Check [`df_dir_status`] to know.
pub fn df_dir() -> PathBuf {
    let (found, _) = df_dir_status();
    found.or_else(env_df_dir).or_else(settings_df_dir).unwrap_or_else(default_guess)
}

/// The lookup order with the inputs given (testable without touching the environment).
fn resolve(
    env: Option<PathBuf>,
    settings: Option<PathBuf>,
    auto: impl FnOnce() -> Option<(PathBuf, &'static str)>,
) -> (Option<PathBuf>, &'static str) {
    if let Some(d) = env.filter(|d| is_df_install(d)) {
        return (Some(d), SRC_ENV);
    }
    if let Some(d) = settings.filter(|d| is_df_install(d)) {
        return (Some(d), SRC_SETTINGS);
    }
    match auto() {
        Some((d, src)) => (Some(d), src),
        None => (None, SRC_NOT_FOUND),
    }
}

/// Steps 3 and 4 of the lookup (uncached; [`df_dir_status`] caches it).
pub fn autodetect() -> Option<(PathBuf, &'static str)> {
    if let Some(d) = steam_candidates(&steam_roots()).into_iter().find(|d| is_df_install(d)) {
        return Some((pretty(&d), SRC_STEAM));
    }
    default_candidates().into_iter().find(|d| is_df_install(d)).map(|d| (pretty(&d), SRC_DEFAULT))
}

/// Where DF would be in a standard Steam install (shown when nothing was found).
fn default_guess() -> PathBuf {
    if cfg!(windows) {
        return PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common").join(DF_FOLDER);
    }
    match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h).join(".local/share/Steam/steamapps/common").join(DF_FOLDER),
        None => PathBuf::from(DF_FOLDER),
    }
}

/// Step 4: common default paths (Windows only).
pub fn default_candidates() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let common = |root: &str| PathBuf::from(root).join("steamapps").join("common").join(DF_FOLDER);
    let mut v = vec![common(r"C:\Program Files (x86)\Steam"), common(r"C:\Program Files\Steam")];
    for d in 'C'..='H' {
        v.push(common(&format!(r"{d}:\SteamLibrary")));
    }
    v
}

/// Steam folders of this machine (registry on Windows, the usual folders on Linux).
pub fn steam_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        if let Some(p) = registry::string(false, r"Software\Valve\Steam", "SteamPath") {
            v.push(PathBuf::from(p));
        }
        if let Some(p) = registry::string(true, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath") {
            v.push(PathBuf::from(p));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
            v.push(h.join(".steam").join("steam"));
            v.push(h.join(".local").join("share").join("Steam"));
            v.push(h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
        }
    }
    v
}

/// Step 3 for the given Steam folders: each folder itself, then every library of its
/// `steamapps/libraryfolders.vdf`, as `<library>/steamapps/common/Dwarf Fortress` (in that
/// order, duplicates removed).
pub fn steam_candidates(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut libs: Vec<PathBuf> = Vec::new();
    for r in roots {
        push_unique(&mut libs, r.clone());
    }
    for r in roots {
        if let Ok(t) = std::fs::read_to_string(r.join("steamapps").join("libraryfolders.vdf")) {
            for p in library_paths(&t) {
                push_unique(&mut libs, PathBuf::from(p));
            }
        }
    }
    libs.into_iter().map(|l| l.join("steamapps").join("common").join(DF_FOLDER)).collect()
}

/// Comparison key of a path: separators unified, trailing separators dropped, and on
/// Windows case-insensitive.
fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.to_string()
    }
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf) {
    if p.as_os_str().is_empty() {
        return;
    }
    let k = path_key(&p);
    if !v.iter().any(|q| path_key(q) == k) {
        v.push(p);
    }
}

/// The path as the file system spells it (Steam's registry value is lower case with `/`),
/// without Windows' `\\?\` prefix. Unchanged when that fails.
fn pretty(p: &Path) -> PathBuf {
    let Ok(c) = std::fs::canonicalize(p) else { return p.to_path_buf() };
    let s = c.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.starts_with(r"UNC\") => p.to_path_buf(),
        Some(rest) => PathBuf::from(rest),
        None => c.clone(),
    }
}

// ------------------------------------------------------------------------------------------ vdf

/// A node of Valve's KeyValues text format (`.vdf`).
#[derive(Clone, Debug, PartialEq)]
pub enum Vdf {
    Str(String),
    Obj(Vec<(String, Vdf)>),
}

#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut it = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(&c) = it.peek() {
        match c {
            c if c.is_whitespace() => {
                it.next();
            }
            '{' => {
                it.next();
                out.push(Tok::Open);
            }
            '}' => {
                it.next();
                out.push(Tok::Close);
            }
            '/' => {
                it.next();
                if it.peek() == Some(&'/') {
                    // Comment to the end of the line.
                    for c in it.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                } else {
                    let mut s = String::from('/');
                    while let Some(&c) = it.peek() {
                        if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                            break;
                        }
                        s.push(c);
                        it.next();
                    }
                    out.push(Tok::Str(s));
                }
            }
            '"' => {
                it.next();
                let mut s = String::new();
                while let Some(c) = it.next() {
                    match c {
                        '"' => break,
                        // Only `\\` and `\"` are escapes: a single backslash in a path stays.
                        '\\' if matches!(it.peek(), Some(&'\\') | Some(&'"')) => s.push(it.next().unwrap_or('\\')),
                        c => s.push(c),
                    }
                }
                out.push(Tok::Str(s));
            }
            _ => {
                let mut s = String::new();
                while let Some(&c) = it.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    it.next();
                }
                // `[$WIN32]`-style conditionals are not keys or values.
                if !(s.starts_with('[') && s.ends_with(']')) {
                    out.push(Tok::Str(s));
                }
            }
        }
    }
    out
}

fn parse_obj(toks: &[Tok], i: &mut usize) -> Vec<(String, Vdf)> {
    let mut out = Vec::new();
    while *i < toks.len() {
        match &toks[*i] {
            Tok::Close => {
                *i += 1;
                return out;
            }
            Tok::Open => {
                // A block without a key: keep its contents under an empty key.
                *i += 1;
                out.push((String::new(), Vdf::Obj(parse_obj(toks, i))));
            }
            Tok::Str(k) => {
                *i += 1;
                match toks.get(*i) {
                    Some(Tok::Str(v)) => {
                        *i += 1;
                        out.push((k.clone(), Vdf::Str(v.clone())));
                    }
                    Some(Tok::Open) => {
                        *i += 1;
                        out.push((k.clone(), Vdf::Obj(parse_obj(toks, i))));
                    }
                    Some(Tok::Close) | None => {}
                }
            }
        }
    }
    out
}

/// Parse a `.vdf` text (never fails: broken input gives what could be read).
pub fn parse_vdf(text: &str) -> Vec<(String, Vdf)> {
    let toks = tokenize(text);
    let mut i = 0;
    let mut out = Vec::new();
    while i < toks.len() {
        out.extend(parse_obj(&toks, &mut i));
    }
    out
}

/// Library folders of a `libraryfolders.vdf`, in file order: the `"path"` values of the v2
/// format (`"0" { "path" "D:\\SteamLibrary" ... }`) and the numbered string entries of the
/// old flat format (`"1" "D:\\SteamLibrary"`).
pub fn library_paths(text: &str) -> Vec<String> {
    fn walk(kv: &[(String, Vdf)], top: bool, out: &mut Vec<String>) {
        for (k, v) in kv {
            match v {
                Vdf::Str(s) if k.eq_ignore_ascii_case("path") || (top && !k.is_empty() && k.chars().all(|c| c.is_ascii_digit())) => {
                    let s = s.trim();
                    if !s.is_empty() && !out.iter().any(|o| o == s) {
                        out.push(s.to_string());
                    }
                }
                Vdf::Obj(inner) => walk(inner, false, out),
                Vdf::Str(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    for (_, v) in parse_vdf(text) {
        if let Vdf::Obj(kv) = v {
            walk(&kv, true, &mut out);
        }
    }
    out
}

// ------------------------------------------------------------------------------------- registry

#[cfg(windows)]
mod registry {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A string value of `HKLM` (`hklm`) or `HKCU`, `None` if it is missing or empty.
    pub fn string(hklm: bool, key: &str, value: &str) -> Option<String> {
        let k = wide(key);
        let v = wide(value);
        let mut buf = vec![0u16; 2048];
        let mut len = (buf.len() * 2) as u32;
        let root = if hklm { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER };
        // SAFETY: the key/value names are NUL-terminated and outlive the call; `len` is the
        // byte size of `buf`, which the call fills at most up to that size.
        let r = unsafe { RegGetValueW(root, PCWSTR(k.as_ptr()), PCWSTR(v.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut len)) };
        if r.is_err() {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]);
        let s = s.trim_end_matches('\0').trim();
        (!s.is_empty()).then(|| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rf-dfdir-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_df(dir: &Path) {
        std::fs::create_dir_all(dir.join("data").join("vanilla")).unwrap();
    }

    #[test]
    fn vdf_v2_format_with_escaped_backslashes() {
        let t = "\u{feff}\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"label\"\t\t\"\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"975370\"\t\t\"1000\"\n\t\t}\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t\t\"contentid\"\t\t\"-112\"\n\t}\n}\n";
        assert_eq!(library_paths(t), vec![r"C:\Program Files (x86)\Steam".to_string(), r"D:\SteamLibrary".to_string()]);
    }

    #[test]
    fn vdf_old_flat_format_and_odd_input() {
        let t = "\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\t\"1700000000\"\n\t\"ContentStatsID\"\t\t\"-4567\"\n\t\"1\"\t\t\"D:\\\\Games\\\\Steam\"\n\t\"2\"\t\t\"F:\\SteamLibrary\"\n}\n";
        // An unescaped single backslash is kept as it is.
        assert_eq!(library_paths(t), vec![r"D:\Games\Steam".to_string(), r"F:\SteamLibrary".to_string()]);
        // Linux paths, comments, a quote inside a value, a conditional, unbalanced braces.
        let t = "// comment\n\"libraryfolders\" {\n \"0\" { \"path\" \"/home/u/.local/share/Steam\" [$LINUX] }\n \"1\" { \"path\" \"/mnt/a \\\"b\\\"\" }\n \"2\" {";
        assert_eq!(library_paths(t), vec!["/home/u/.local/share/Steam".to_string(), "/mnt/a \"b\"".to_string()]);
        assert!(library_paths("").is_empty());
        assert!(library_paths("}}}{{\"").is_empty());
        let kv = parse_vdf("\"a\" { \"b\" \"c\" }");
        assert_eq!(kv, vec![("a".to_string(), Vdf::Obj(vec![("b".to_string(), Vdf::Str("c".to_string()))]))]);
    }

    #[test]
    fn steam_libraries_are_searched() {
        let base = tmp("steam");
        let root = base.join("Steam");
        let lib = base.join("Lib Two");
        std::fs::create_dir_all(root.join("steamapps")).unwrap();
        let vdf = format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
            root.display().to_string().replace('\\', "\\\\"),
            lib.display().to_string().replace('\\', "\\\\")
        );
        std::fs::write(root.join("steamapps").join("libraryfolders.vdf"), vdf).unwrap();
        let c = steam_candidates(&[root.clone()]);
        // The Steam folder itself once (also listed in the vdf), then the second library.
        assert_eq!(c, vec![root.join("steamapps/common/Dwarf Fortress"), lib.join("steamapps/common/Dwarf Fortress")]);
        assert!(!c.iter().any(|d| is_df_install(d)));
        fake_df(&lib.join("steamapps/common/Dwarf Fortress"));
        let found: Vec<_> = c.into_iter().filter(|d| is_df_install(d)).collect();
        assert_eq!(found, vec![lib.join("steamapps/common/Dwarf Fortress")]);
        // A missing vdf only gives the Steam folder itself.
        assert_eq!(steam_candidates(&[base.join("nothing")]).len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn lookup_order() {
        let base = tmp("order");
        let (a, b, c) = (base.join("a"), base.join("b"), base.join("c"));
        for d in [&a, &b, &c] {
            fake_df(d);
        }
        let none = base.join("missing");
        let auto = |p: &Path| {
            let p = p.to_path_buf();
            move || Some((p, SRC_STEAM))
        };
        assert_eq!(resolve(Some(a.clone()), Some(b.clone()), auto(&c)), (Some(a.clone()), SRC_ENV));
        assert_eq!(resolve(None, Some(b.clone()), auto(&c)), (Some(b.clone()), SRC_SETTINGS));
        // A set folder that is no DF install is skipped.
        assert_eq!(resolve(Some(none.clone()), Some(b.clone()), auto(&c)), (Some(b.clone()), SRC_SETTINGS));
        assert_eq!(resolve(Some(none.clone()), Some(none.clone()), auto(&c)), (Some(c.clone()), SRC_STEAM));
        assert_eq!(resolve(Some(none.clone()), None, || None), (None, SRC_NOT_FOUND));
        // Not a DF install: no data/vanilla (e.g. only DF's art folder or an empty path).
        std::fs::create_dir_all(none.join("data").join("art")).unwrap();
        assert!(!is_df_install(&none) && !is_df_install(Path::new("")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn settings_folder_and_defaults() {
        // Only this test touches the settings slot.
        set_settings_df_dir(Some(Path::new("  \"X:/df\"  ")));
        assert_eq!(settings_df_dir(), Some(PathBuf::from("X:/df")));
        set_settings_df_dir(Some(Path::new("")));
        assert_eq!(settings_df_dir(), None);
        set_settings_df_dir(None);
        assert_eq!(settings_df_dir(), None);
        assert_eq!(clean(" \"\" "), None);
        if cfg!(windows) {
            let d = default_candidates();
            assert_eq!(d.len(), 8);
            assert!(d[0].ends_with(r"Steam\steamapps\common\Dwarf Fortress"));
            assert!(d[7].starts_with(r"H:\SteamLibrary"));
        }
        // df_dir() always names a folder, found or not.
        assert!(!df_dir().as_os_str().is_empty());
        let (found, src) = df_dir_status();
        assert_eq!(found.is_some(), src != SRC_NOT_FOUND);
        eprintln!("DF install: {found:?} ({src})");
    }
}
