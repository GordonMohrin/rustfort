//! Rustfort executable.
//!
//! ```text
//! rustfort play  [--export DIR] [--synthetic] [--seed N]
//!                [--screenshot out.png --frames N] [--view x,y,z,zoom] [--ui a,b,..] [--mouse x,y] [--speed 1|2|5|max]
//! rustfort bench [--export DIR] [--synthetic] [--ticks N] [--seed N]
//! rustfort savebench [--export DIR]      # save + load timing and size
//! ```
//! `--load FILE.rfsave` starts from a save instead of the export (play and bench).
//!
//! [round 6] `rustfort` / `rustfort play` without a world argument opens the title screen
//! (Continue, Load game, Import from Dwarf Fortress, ...). `--export`, `--load`,
//! `--synthetic`/`--size` and `--screenshot` start the game directly, unless `--menu` is given.

mod bench;
mod crash; // [app10]
mod desktop; // screenshot/test windows open on virtual desktop 1
mod font;
mod game;
mod importer;
mod keys;
mod load;
mod menu;
mod render;
mod save;
mod script;
mod settings;
mod simthread;
mod ui;

use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct Args {
    pub cmd: String,
    pub export: Option<PathBuf>,
    pub synthetic: bool,
    /// bench: disable invasions (sieges) for survival/regression runs.
    pub no_invasions: bool,
    /// bench: emulate a player standing the military down one day after the last hostile left.
    pub standdown: bool,
    /// Size of the synthetic world (`--size x,y,z` implies `--synthetic`).
    pub size: (u16, u16, u16),
    pub seed: u64,
    pub ticks: u64,
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    /// Camera: tile x, tile y, z-level, zoom factor (1.0 = 32 px tiles).
    pub view: Option<(f32, f32, i32, f32)>,
    /// UI states to open at start (for screenshots/tests): `citizens`, `unit`, `unit:<id>`,
    /// `dig`, `stockpile`, `menu`, `loadmenu`, `tile`, `drag`, `chop`, `build[:cat]`,
    /// `place:<option>`, `zone[:kind]`, `orders`, `neworder[:template]`, `labor-screen`,
    /// `bld:<id|kind|subtype>`, `addjob`, `crop:<season>` (see README).
    pub ui: Vec<String>,
    /// Fake mouse position (screenshot mode).
    pub mouse: Option<(f32, f32)>,
    pub speed: Option<String>,
    /// Scripted input, see `script.rs`.
    pub script: Option<String>,
    /// Frame cap when vsync is off (DF's G_FPS_CAP).
    pub gfps: u32,
    pub vsync: bool,
    /// UI scale (1 = DF's 100 % interface: 8x12 font, 32x36 buttons).
    pub ui_scale: f32,
    /// Start from a save game (`saves/*.rfsave`).
    pub load: Option<PathBuf>,
    /// [round 4] Run the simulation inside the frame (no worker thread), for comparisons.
    pub sim_inline: bool,
    /// [round 6] Show the title screen even with `--screenshot` / a world argument.
    pub menu: bool,
    /// Move the window to this virtual desktop (`--desktop N`, 0 = stay); screenshot runs default to 1.
    pub desktop: Option<usize>,
    /// [round 6] Title screen page to open: `load`, `exports`, `import`, `settings`.
    pub menu_page: Option<String>,
    /// [round 6] Autosave interval in game days (overrides the settings; 0 = off).
    pub autosave_days: Option<u32>,
    /// [round 6] `--ui-scale` / `--gfps` were given (they win over the settings file).
    pub ui_scale_cli: bool,
    pub gfps_cli: bool,
    /// [round 6] Extra screenshots at these frames (`<shot>-f<N>.png`), e.g. an import's progress.
    pub shots_at: Vec<u32>,
}

const USAGE: &str = "usage:
  rustfort                                     title screen (Continue, Load, Import from DF, Settings)
  rustfort play  [--export DIR] [--synthetic] [--seed N] [--speed 1|2|5|max]
                 [--screenshot out.png --frames N] [--view x,y,z,zoom] [--ui citizens,unit,build,orders,labor-screen,..]
                 [--mouse x,y] [--script \"frame:cmd; ...\"] [--ui-scale 2] [--gfps 60] [--size x,y,z] [--sim-inline]
                 [--menu] [--menu-page load|exports|import|settings] [--autosave-days N]
                 [--desktop N] [--shots-at a,b]
  rustfort bench [--export DIR] [--synthetic] [--ticks N] [--seed N] [--no-invasions] [--standdown]
  rustfort savebench [--export DIR]
  --load FILE.rfsave   start from a save game (play, bench)
  --export DIR         a fort export; default: $RUSTFORT_EXPORT, else the newest export under export/,
                       else the built-in synthetic test fort
  --desktop N          move the window to virtual desktop N (Windows; 0 = stay; screenshot runs
                       default to desktop 1, also RUSTFORT_DESKTOP)
environment: RUSTFORT_DF_DIR = Dwarf Fortress folder (else Settings, then Steam auto-detection)";

fn parse_args() -> Result<Args, String> {
    let mut a = Args { cmd: "play".into(), seed: 1, ticks: 10_000, frames: 3, gfps: 60, ui_scale: 1.0, size: (96, 96, 40), ..Default::default() };
    let mut it = std::env::args().skip(1).peekable();
    if let Some(c) = it.peek() {
        if !c.starts_with('-') {
            a.cmd = it.next().unwrap();
        }
    }
    while let Some(arg) = it.next() {
        let mut val = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--export" => a.export = Some(PathBuf::from(val("--export")?)),
            "--load" => a.load = Some(PathBuf::from(val("--load")?)),
            "--synthetic" => a.synthetic = true,
            "--no-invasions" => a.no_invasions = true,
            "--standdown" => a.standdown = true,
            "--sim-inline" => a.sim_inline = true,
            "--size" => {
                let v = val("--size")?;
                let p: Vec<u16> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if p.len() != 3 || p.iter().any(|&n| n < 8) {
                    return Err("--size expects x,y,z (each >= 8)".into());
                }
                a.size = (p[0], p[1], p[2]);
                a.synthetic = true;
            }
            "--seed" => a.seed = val("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?,
            "--ticks" => a.ticks = val("--ticks")?.parse().map_err(|e| format!("--ticks: {e}"))?,
            "--screenshot" => a.screenshot = Some(PathBuf::from(val("--screenshot")?)),
            "--frames" => a.frames = val("--frames")?.parse().map_err(|e| format!("--frames: {e}"))?,
            "--view" => {
                let v = val("--view")?;
                let p: Vec<&str> = v.split(',').collect();
                if p.len() < 3 {
                    return Err("--view expects x,y,z[,zoom]".into());
                }
                let f = |s: &str| s.trim().parse::<f32>().map_err(|e| format!("--view: {e}"));
                a.view = Some((f(p[0])?, f(p[1])?, f(p[2])? as i32, if p.len() > 3 { f(p[3])? } else { 1.0 }));
            }
            "--ui" => a.ui = val("--ui")?.split(',').map(|s| s.trim().to_string()).collect(),
            "--mouse" => {
                let v = val("--mouse")?;
                let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if p.len() != 2 {
                    return Err("--mouse expects x,y".into());
                }
                a.mouse = Some((p[0], p[1]));
            }
            "--speed" => a.speed = Some(val("--speed")?),
            "--script" => a.script = Some(val("--script")?),
            "--ui-scale" => {
                a.ui_scale = val("--ui-scale")?.parse::<f32>().map_err(|e| format!("--ui-scale: {e}"))?.clamp(0.5, 4.0);
                a.ui_scale_cli = true;
            }
            "--gfps" => {
                a.gfps = val("--gfps")?.parse().map_err(|e| format!("--gfps: {e}"))?;
                a.gfps_cli = true;
            }
            "--menu" => a.menu = true,
            "--desktop" => a.desktop = Some(val("--desktop")?.parse().map_err(|e| format!("--desktop: {e}"))?),
            "--shots-at" => a.shots_at = val("--shots-at")?.split(',').filter_map(|s| s.trim().parse().ok()).collect(),
            "--menu-page" => {
                a.menu_page = Some(val("--menu-page")?);
                a.menu = true;
            }
            "--autosave-days" => a.autosave_days = Some(val("--autosave-days")?.parse().map_err(|e| format!("--autosave-days: {e}"))?),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(a)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            // An empty error is -h/--help: print the usage and exit successfully.
            if e.is_empty() {
                println!("{USAGE}");
                std::process::exit(0);
            }
            eprintln!("error: {e}");
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    match args.cmd.as_str() {
        "bench" => bench::run(&args),
        "savebench" => {
            let loaded = load::load(&args);
            if let Some(w) = &loaded.warning {
                eprintln!("note: {w}");
            }
            println!("world: {}", loaded.label);
            save::bench(loaded.world, args.seed);
        }
        "play" => {
            // [app10] Every panic goes to saves/crash.log; the game catches them (crash dialog).
            crash::install_hook();
            // [round 6] Settings file next to the saves (UI scale, frame cap, vsync, DF folder).
            // The DF folder lookup: RUSTFORT_DF_DIR, this folder, Steam, default paths.
            let settings = settings::Settings::load();
            settings.apply_df_dir();
            match rf_gfx::df_dir_status() {
                (Some(d), src) => eprintln!("rustfort: Dwarf Fortress at {} ({src})", d.display()),
                (None, _) => eprintln!("rustfort: {} Without it there are no DF graphics, DF font, DF keys or import.", load::DF_NOT_FOUND_HINT),
            }
            let mut args = args;
            if !args.ui_scale_cli {
                args.ui_scale = settings.ui_scale;
            }
            if !args.gfps_cli {
                args.gfps = settings.gfps;
            }
            // A world on the command line (or a screenshot test) skips the title screen.
            let direct = !args.menu && (args.export.is_some() || args.load.is_some() || args.synthetic || args.screenshot.is_some());
            let loaded = direct.then(|| {
                let mut l = load::load(&args);
                load::add_df_hint(&mut l);
                l
            });
            let title = match &loaded {
                Some(l) => format!("Rustfort - {}", l.label),
                None => "Rustfort".to_string(),
            };
            let mut conf = macroquad::window::Conf {
                window_title: title,
                window_width: 1600,
                window_height: 900,
                window_resizable: true,
                high_dpi: false,
                ..Default::default()
            };
            // Vsync on for play (smooth, no tearing), off for screenshots; RUSTFORT_VSYNC=0/1
            // overrides, then the settings. Without vsync the game paces itself to --gfps.
            let vsync = match std::env::var("RUSTFORT_VSYNC").ok().as_deref() {
                Some("0") => false,
                Some(_) => true,
                None => args.screenshot.is_none() && settings.vsync,
            };
            if !vsync {
                conf.platform.swap_interval = Some(0);
            }
            args.vsync = vsync;
            macroquad::Window::from_config(conf, menu::app_main(args, settings, loaded));
        }
        other => {
            eprintln!("unknown command {other}\n{USAGE}");
            std::process::exit(2);
        }
    }
}
