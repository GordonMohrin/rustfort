//! [round 6] "Import from Dwarf Fortress": finds the running DF, asks DFHack (read-only) which
//! fort is loaded, runs `tools/dfhack/rustfort-export.lua` into `export/<fort>-<date>/` (under
//! the working directory, or `$RUSTFORT_EXPORT_ROOT`) and follows its `progress.json` until
//! `manifest.json` (the done-marker) appears. Needs DF running with a fort loaded and DFHack
//! installed (a free, separate Steam app that installs into the DF folder).
//!
//! Everything here talks to DF only through `hack/dfhack-run.exe`, with two scripts that never
//! pause the game or write to its memory: `rustfort-probe.lua` (one JSON line about the loaded
//! fort) and the exporter. Earlier exports are kept; every import gets its own folder.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Image name of the DF process (`RUSTFORT_DF_PROCESS` overrides it, for tests).
pub fn df_process_name() -> String {
    std::env::var("RUSTFORT_DF_PROCESS").unwrap_or_else(|_| "Dwarf Fortress.exe".into())
}

/// `hack/dfhack-run.exe` of the DF install.
pub fn dfhack_run(df_dir: &Path) -> PathBuf {
    let exe = if cfg!(windows) { "dfhack-run.exe" } else { "dfhack-run" };
    df_dir.join("hack").join(exe)
}

/// The folder with the DFHack scripts (`rustfort-export.lua`, `rustfort-probe.lua`):
/// `$RUSTFORT_TOOLS_DIR`, else `tools/dfhack` in the working directory, next to the exe's
/// repository (`target/release/../../tools/dfhack`), or in the source tree.
pub fn tools_dir() -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("RUSTFORT_TOOLS_DIR") {
        c.push(PathBuf::from(p));
    }
    c.push(PathBuf::from("tools").join("dfhack"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            c.push(d.join("tools").join("dfhack"));
            c.push(d.join("..").join("..").join("tools").join("dfhack"));
        }
    }
    c.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("tools").join("dfhack"));
    c.into_iter().find(|d| d.join("rustfort-export.lua").exists()).map(|d| std::fs::canonicalize(&d).map(|p| plain(&p)).unwrap_or(d))
}

/// Root folder of the exports: `$RUSTFORT_EXPORT_ROOT`, else `export/` (`rf_import::EXPORT_ROOT`).
pub fn export_root() -> PathBuf {
    std::env::var_os("RUSTFORT_EXPORT_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(rf_import::EXPORT_ROOT))
}

/// Strip Windows' `\\?\` prefix from a canonical path (DF/Lua and humans prefer plain paths).
pub fn plain(p: &Path) -> PathBuf {
    let s = p.display().to_string();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

/// Path for Lua: forward slashes.
fn lua_path(p: &Path) -> String {
    plain(p).display().to_string().replace('\\', "/")
}

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console window flashes up.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// Run a command with a timeout; returns (success, stdout + stderr).
fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<(bool, String), String> {
    let mut child = hidden(cmd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    // Read the pipes in threads, so a chatty child can never block on a full pipe.
    let ot = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let et = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let t0 = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if t0.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no answer within {} s", timeout.as_secs()));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut text = ot.join().unwrap_or_default();
    let e = et.join().unwrap_or_default();
    if !e.trim().is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&e);
    }
    Ok((status.success(), text))
}

/// Is the DF process running? `None` when that cannot be checked (not Windows, no tasklist).
pub fn df_running() -> Option<bool> {
    if !cfg!(windows) {
        return None;
    }
    let name = df_process_name();
    let (_, out) = run_with_timeout(Command::new("tasklist").args(["/FI", &format!("IMAGENAME eq {name}"), "/NH", "/FO", "CSV"]), Duration::from_secs(5)).ok()?;
    Some(out.to_lowercase().contains(&name.to_lowercase()))
}

/// What DFHack says about the loaded game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FortInfo {
    pub fort: String,
    pub fort_native: String,
    pub world: String,
    pub year: i32,
    pub year_tick: i32,
    pub citizens: i32,
    pub paused: bool,
    pub dfhack: String,
}

/// Result of [`probe`].
#[derive(Clone, Debug, PartialEq)]
pub enum Probe {
    /// `hack/dfhack-run.exe` is missing: wrong DF folder or no DFHack.
    NoDfhackRun(PathBuf),
    /// The Rustfort DFHack scripts were not found.
    NoTools,
    /// `Dwarf Fortress.exe` is not running.
    NotRunning,
    /// DF runs, but DFHack does not answer (message of dfhack-run).
    NoDfhack(String),
    /// DFHack answers, but no fort is loaded (title screen, world gen, adventure mode).
    NoFort(String),
    Ready(FortInfo),
}

/// One JSON object line from the output of a DFHack script.
pub fn json_line(out: &str) -> Option<serde_json::Value> {
    out.lines().map(str::trim).filter(|l| l.starts_with('{')).find_map(|l| serde_json::from_str(l).ok())
}

/// Interpret the probe script's line.
pub fn parse_probe(v: &serde_json::Value) -> Probe {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let i = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0) as i32;
    if v.get("map_loaded").and_then(|x| x.as_bool()) != Some(true) {
        return Probe::NoFort("no map is loaded (title screen or world generation?)".into());
    }
    if v.get("fort_mode").and_then(|x| x.as_bool()) != Some(true) {
        return Probe::NoFort("the loaded game is not in fortress mode".into());
    }
    Probe::Ready(FortInfo {
        fort: s("fort"),
        fort_native: s("fort_native"),
        world: s("world"),
        year: i("year"),
        year_tick: i("year_tick"),
        citizens: i("citizens"),
        paused: v.get("paused").and_then(|x| x.as_bool()).unwrap_or(false),
        dfhack: s("dfhack"),
    })
}

/// dfhack-run crashes without a word when nothing listens on DFHack's port.
fn no_connect() -> String {
    let port = std::env::var("DFHACK_PORT").unwrap_or_else(|_| "5000".into());
    format!("dfhack-run could not connect to DFHack (localhost:{port}).")
}

/// First non-empty line of a tool's output, for error messages.
fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("(no output)").chars().take(140).collect()
}

/// Ask DF (read-only) whether a fort can be exported. Blocking (up to ~10 s); run it in a thread.
pub fn probe(df_dir: &Path) -> Probe {
    let run = dfhack_run(df_dir);
    if !run.exists() {
        return Probe::NoDfhackRun(run);
    }
    let Some(tools) = tools_dir() else { return Probe::NoTools };
    // `RUSTFORT_DF_PROCESS` (tests) checks the process list first; normally DFHack is asked
    // directly (0.1 s) and the slow process list (up to seconds on a busy machine) is only
    // read to tell "DF is not running" from "DFHack does not answer".
    let forced = std::env::var_os("RUSTFORT_DF_PROCESS").is_some();
    if forced && df_running() == Some(false) {
        return Probe::NotRunning;
    }
    let script = tools.join("rustfort-probe.lua");
    let res = run_with_timeout(Command::new(&run).arg("lua").arg("-f").arg(lua_path(&script)), Duration::from_secs(10));
    let failed = |msg: String| if df_running() == Some(false) { Probe::NotRunning } else { Probe::NoDfhack(msg) };
    match res {
        Err(e) => failed(format!("dfhack-run: {e}")),
        Ok((_, out)) => match json_line(&out) {
            Some(v) => parse_probe(&v),
            // dfhack-run crashes without a word when nothing listens on DFHack's port.
            None if out.trim().is_empty() => failed(no_connect()),
            None => failed(first_line(&out)),
        },
    }
}

/// `tileshtulon-20261010-153205` (UTC, like the save names).
pub fn export_dir_name(fort_native: &str, fort: &str, unix: u64) -> String {
    let name = if fort_native.trim().is_empty() { fort } else { fort_native };
    let slug: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let slug = slug.trim_matches('_');
    let slug = if slug.is_empty() { "fort" } else { slug };
    let (y, m, d, h, mi, s) = crate::save::utc_parts(unix);
    format!("{slug}-{y:04}{m:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// Exporter phases in order, with their rough share of the export time (Trussroads, round 4).
const PHASES: &[(&str, f32)] = &[
    ("start", 0.2),
    ("plants", 1.0),
    ("map", 7.0),
    ("units", 1.5),
    ("items", 1.5),
    ("buildings", 0.5),
    ("work_orders", 0.1),
    ("plant_raws", 0.1),
    ("burrows", 0.2),
    ("squads", 0.1),
    ("creature_raws", 1.0),
    ("item_raws", 0.2),
    ("history", 0.6),
    ("entities", 0.1),
    ("nobles", 0.1),
    ("settings", 0.05),
    ("trade", 0.1),
    ("hist_figs", 0.2),
    ("tiletypes", 0.05),
    ("materials", 0.6),
];

/// Overall progress 0..1 from the exporter's `phase` and `detail` (`"123/456"`, `"z 12/149"`).
pub fn phase_fraction(phase: &str, detail: &str) -> Option<f32> {
    if phase == "done" {
        return Some(1.0);
    }
    let total: f32 = PHASES.iter().map(|p| p.1).sum();
    let mut acc = 0.0;
    for (p, w) in PHASES {
        if *p == phase {
            let inner = detail
                .split_whitespace()
                .last()
                .and_then(|t| t.split_once('/'))
                .and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?)))
                .map(|(a, b)| if b > 0.0 { (a / b).clamp(0.0, 1.0) } else { 0.0 })
                .unwrap_or(0.0);
            return Some(((acc + w * inner) / total).clamp(0.0, 1.0));
        }
        acc += w;
    }
    None
}

/// Human name of an exporter phase.
pub fn phase_label(phase: &str) -> String {
    match phase {
        "start" => "Starting".into(),
        "map" => "Map tiles".into(),
        "work_orders" => "Work orders".into(),
        "plant_raws" => "Plant raws".into(),
        "creature_raws" => "Creature raws".into(),
        "item_raws" => "Item raws".into(),
        "hist_figs" => "Historical figures".into(),
        "done" => "Done".into(),
        other => crate::ui::pretty(other),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub phase: String,
    pub detail: String,
    pub elapsed_s: f64,
    pub frac: f32,
}

/// Parse `progress.json`.
pub fn parse_progress(text: &str) -> Option<Progress> {
    let v: serde_json::Value = serde_json::from_str(text.trim().trim_start_matches('\u{feff}')).ok()?;
    let phase = v.get("phase")?.as_str()?.to_string();
    let detail = v.get("detail").and_then(|d| d.as_str()).unwrap_or("").to_string();
    let elapsed_s = v.get("elapsed_s").and_then(|d| d.as_f64()).unwrap_or(0.0);
    let frac = phase_fraction(&phase, &detail).unwrap_or(0.0);
    Some(Progress { phase, detail, elapsed_s, frac })
}

/// State of a running export.
#[derive(Clone, Debug, PartialEq)]
pub enum JobState {
    Running(Progress),
    Done,
    Failed(String),
}

/// A running export (started by [`ExportJob::start`]).
pub struct ExportJob {
    pub dir: PathBuf,
    pub started: Instant,
    pub progress: Progress,
    last_text: String,
    last_change: Instant,
}

/// Without a new `progress.json` for this long, the export counts as stalled.
const STALL: Duration = Duration::from_secs(90);

impl ExportJob {
    /// Start the exporter (returns after DFHack accepted the job, usually ~0.1 s). Blocking.
    pub fn start(df_dir: &Path, info: &FortInfo) -> Result<ExportJob, String> {
        let run = dfhack_run(df_dir);
        let tools = tools_dir().ok_or("tools/dfhack/rustfort-export.lua not found")?;
        let root = export_root();
        std::fs::create_dir_all(&root).map_err(|e| format!("create {}: {e}", root.display()))?;
        let root = std::fs::canonicalize(&root).map(|p| plain(&p)).unwrap_or(root);
        let mut dir = root.join(export_dir_name(&info.fort_native, &info.fort, crate::save::now_unix()));
        let mut n = 2;
        while dir.exists() {
            dir = root.join(format!("{}-{n}", export_dir_name(&info.fort_native, &info.fort, crate::save::now_unix())));
            n += 1;
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let script = tools.join("rustfort-export.lua");
        // Explicit options: the output folder (the exporter's own default would be
        // <DF folder>/rustfort-export/<fort>) and --any-fort (no fort-name check; that is the
        // exporter's default too, the probe just named the fort).
        let (ok, out) = run_with_timeout(
            Command::new(&run).arg("lua").arg("-f").arg(lua_path(&script)).arg(lua_path(&dir)).arg("--any-fort"),
            Duration::from_secs(20),
        )
        .map_err(|e| format!("dfhack-run: {e}"))?;
        match json_line(&out) {
            Some(v) if v.get("ok").and_then(|x| x.as_bool()) == Some(true) => {}
            Some(v) => return Err(v.get("error").and_then(|x| x.as_str()).unwrap_or("the exporter refused").to_string()),
            None if !ok && out.trim().is_empty() => return Err(no_connect()),
            None if !ok => return Err(format!("dfhack-run: {}", first_line(&out))),
            None => return Err(format!("unexpected answer: {}", first_line(&out))),
        }
        let now = Instant::now();
        Ok(ExportJob { dir, started: now, progress: Progress { phase: "start".into(), ..Default::default() }, last_text: String::new(), last_change: now })
    }

    /// Follow an export that is already running in `dir` (tests, or a restarted Rustfort).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn follow(dir: PathBuf) -> ExportJob {
        let now = Instant::now();
        ExportJob { dir, started: now, progress: Progress::default(), last_text: String::new(), last_change: now }
    }

    /// Read `progress.json` / `manifest.json` / `error.json`.
    pub fn poll(&mut self) -> JobState {
        if self.dir.join("manifest.json").exists() && !self.dir.join("export.lock").exists() {
            self.progress.frac = 1.0;
            self.progress.phase = "done".into();
            return JobState::Done;
        }
        if let Ok(t) = std::fs::read_to_string(self.dir.join("error.json")) {
            let msg = serde_json::from_str::<serde_json::Value>(t.trim())
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                .unwrap_or(t);
            return JobState::Failed(format!("the exporter stopped: {msg}"));
        }
        if let Ok(t) = std::fs::read_to_string(self.dir.join("progress.json")) {
            if t != self.last_text {
                self.last_text = t.clone();
                self.last_change = Instant::now();
                if let Some(p) = parse_progress(&t) {
                    // Never step back (a phase the table does not know keeps the last value).
                    let frac = if phase_fraction(&p.phase, &p.detail).is_some() { p.frac.max(self.progress.frac) } else { self.progress.frac };
                    self.progress = Progress { frac, ..p };
                }
            }
        }
        if self.last_change.elapsed() > STALL {
            return JobState::Failed(format!("no progress for {} s (is DF still running?)", STALL.as_secs()));
        }
        JobState::Running(self.progress.clone())
    }

    /// Ask the exporter to stop (it checks for this file about once a second).
    pub fn cancel(&self) {
        let _ = std::fs::write(self.dir.join("cancel"), "cancel\n");
    }
}

/// An export folder on disk.
#[derive(Clone, Debug)]
pub struct ExportInfo {
    pub dir: PathBuf,
    pub fort: String,
    pub fort_native: String,
    pub year: i32,
    /// Modification time of `manifest.json` (Unix seconds).
    pub when: u64,
}

/// Complete exports (with `manifest.json`) under `root`, newest first.
pub fn list_exports(root: &Path) -> Vec<ExportInfo> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else { return v };
    for e in rd.flatten() {
        let dir = e.path();
        let mf = dir.join("manifest.json");
        let Ok(meta) = std::fs::metadata(&mf) else { continue };
        if dir.join("export.lock").exists() {
            continue;
        }
        let when = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        let (fort, fort_native, year) = match rf_import::load_manifest(&dir) {
            Ok(m) => (m.fort_name.clone(), m.fort_name_native.clone(), m.year),
            Err(_) => (dir.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(), String::new(), 0),
        };
        v.push(ExportInfo { dir, fort, fort_native, year, when });
    }
    v.sort_by(|a, b| b.when.cmp(&a.when).then(b.dir.cmp(&a.dir)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_fractions() {
        assert_eq!(phase_fraction("done", ""), Some(1.0));
        assert_eq!(phase_fraction("start", ""), Some(0.0));
        let a = phase_fraction("map", "z 0/149").unwrap();
        let b = phase_fraction("map", "z 75/149").unwrap();
        let c = phase_fraction("units", "10/300").unwrap();
        let d = phase_fraction("materials", "2000/2087").unwrap();
        assert!(a < b && b < c && c < d && d < 1.0, "{a} {b} {c} {d}");
        assert_eq!(phase_fraction("something_new", ""), None);
        let p = parse_progress(r#"{"phase":"items","detail":"16/14400","elapsed_s":3.5,"frames":40,"max_slice_ms":31,"year_tick":12,"rev":"x"}"#).unwrap();
        assert_eq!((p.phase.as_str(), p.elapsed_s), ("items", 3.5));
        assert!(p.frac > 0.5 && p.frac < 0.7, "{}", p.frac);
        assert!(parse_progress("garbage").is_none());
        assert_eq!(phase_label("work_orders"), "Work orders");
        assert_eq!(phase_label("buildings"), "Buildings");
    }

    #[test]
    fn probe_lines() {
        let out = "some banner\n{\"ok\":true,\"map_loaded\":true,\"fort_mode\":true,\"fort\":\"Trussroads\",\"fort_native\":\"Tileshtulon\",\"world\":\"Xyz\",\"year\":277,\"year_tick\":1200,\"citizens\":91,\"paused\":false,\"dfhack\":\"53.16-r2\"}\n";
        let v = json_line(out).unwrap();
        match parse_probe(&v) {
            Probe::Ready(f) => {
                assert_eq!((f.fort.as_str(), f.fort_native.as_str(), f.year, f.citizens), ("Trussroads", "Tileshtulon", 277, 91));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(parse_probe(&json_line(r#"{"ok":true,"map_loaded":false}"#).unwrap()), Probe::NoFort(_)));
        assert!(matches!(parse_probe(&json_line(r#"{"ok":true,"map_loaded":true,"fort_mode":false}"#).unwrap()), Probe::NoFort(_)));
        assert!(json_line("Could not connect to localhost:5000").is_none());
        assert_eq!(first_line("\n  Could not connect\nmore"), "Could not connect");
    }

    #[test]
    fn export_names_and_listing() {
        // 2026-10-10 14:32:05 UTC
        assert_eq!(export_dir_name("Tileshtulon", "Trussroads", 1_791_642_725), "tileshtulon-20261010-143205");
        assert_eq!(export_dir_name("", "Trussroads", 0), "trussroads-19700101-000000");
        assert_eq!(export_dir_name("", "", 0), "fort-19700101-000000");
        let root = std::env::temp_dir().join(format!("rf-exports-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (d, done) in [("a-1", true), ("b-2", false), ("c-3", true)] {
            std::fs::create_dir_all(root.join(d)).unwrap();
            if done {
                std::fs::write(root.join(d).join("manifest.json"), "{}").unwrap();
            }
        }
        // A running export (lock present) is not listed.
        std::fs::write(root.join("c-3").join("export.lock"), "1").unwrap();
        let l = list_exports(&root);
        assert_eq!(l.len(), 1);
        assert!(l[0].dir.ends_with("a-1"));
        // Following an export: progress, then done.
        let mut job = ExportJob::follow(root.join("b-2"));
        std::fs::write(root.join("b-2").join("progress.json"), r#"{"phase":"map","detail":"z 10/149","elapsed_s":2.0}"#).unwrap();
        match job.poll() {
            JobState::Running(p) => assert!(p.frac > 0.0 && p.frac < 0.5),
            other => panic!("{other:?}"),
        }
        std::fs::write(root.join("b-2").join("manifest.json"), "{}").unwrap();
        assert_eq!(job.poll(), JobState::Done);
        let mut job = ExportJob::follow(root.join("a-1"));
        std::fs::remove_file(root.join("a-1").join("manifest.json")).unwrap();
        std::fs::write(root.join("a-1").join("error.json"), r#"{"error":"cancelled by user (cancel file)","phase":"map"}"#).unwrap();
        assert!(matches!(job.poll(), JobState::Failed(m) if m.contains("cancelled")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_dfhack_is_reported() {
        let p = probe(Path::new("Z:/no/such/df"));
        assert!(matches!(p, Probe::NoDfhackRun(_)), "{p:?}");
        assert!(tools_dir().is_some(), "tools/dfhack must be found from the source tree");
    }
}
