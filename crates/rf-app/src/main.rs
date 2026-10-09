//! Rustfort executable.
//!
//! ```text
//! rustfort play  [--export DIR] [--synthetic] [--seed N]
//!                [--screenshot out.png --frames N] [--view x,y,z,zoom] [--ui a,b,..] [--mouse x,y] [--speed 1|2|5|max]
//! rustfort bench [--export DIR] [--synthetic] [--ticks N] [--seed N]
//! ```

mod bench;
mod font;
mod game;
mod keys;
mod load;
mod render;
mod ui;

use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct Args {
    pub cmd: String,
    pub export: Option<PathBuf>,
    pub synthetic: bool,
    pub seed: u64,
    pub ticks: u64,
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    /// Camera: tile x, tile y, z-level, zoom factor (1.0 = 32 px tiles).
    pub view: Option<(f32, f32, i32, f32)>,
    /// UI states to open at start (for screenshots/tests): `citizens`, `unit`, `unit:<id>`,
    /// `dig`, `stockpile`, `menu`, `tile`, `drag`, `chop`.
    pub ui: Vec<String>,
    /// Fake mouse position (screenshot mode).
    pub mouse: Option<(f32, f32)>,
    pub speed: Option<String>,
}

const USAGE: &str = "usage:
  rustfort play  [--export DIR] [--synthetic] [--seed N] [--speed 1|2|5|max]
                 [--screenshot out.png --frames N] [--view x,y,z,zoom] [--ui citizens,unit,dig,stockpile,menu,tile]
                 [--mouse x,y]
  rustfort bench [--export DIR] [--synthetic] [--ticks N] [--seed N]";

fn parse_args() -> Result<Args, String> {
    let mut a = Args { cmd: "play".into(), seed: 1, ticks: 10_000, frames: 3, ..Default::default() };
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
            "--synthetic" => a.synthetic = true,
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
            if !e.is_empty() {
                eprintln!("error: {e}");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    match args.cmd.as_str() {
        "bench" => bench::run(args.export.as_deref(), args.synthetic, args.ticks, args.seed),
        "play" => {
            let loaded = load::load_world(args.export.as_deref(), args.synthetic);
            let conf = macroquad::window::Conf {
                window_title: format!("Rustfort - {}", loaded.label),
                window_width: 1600,
                window_height: 900,
                window_resizable: true,
                high_dpi: false,
                ..Default::default()
            };
            macroquad::Window::from_config(conf, game::run(args, loaded));
        }
        other => {
            eprintln!("unknown command {other}\n{USAGE}");
            std::process::exit(2);
        }
    }
}
