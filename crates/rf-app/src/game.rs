//! Game state, the main loop, camera and map input.

use crate::font::Font;
use crate::keys::{Bindings, InputFrame, InputState, SHIFT};
use crate::load::Loaded;
use crate::render::MapRenderer;
use crate::ui::UiCtx;
use crate::Args;
use macroquad::prelude::*;
use rf_core::{tile_flags, Pos, Shape, World};
use rf_gfx::Gfx;
use rf_sim::{AnnouncementKind, DesignationKind, Sim};
use std::collections::VecDeque;
use std::time::Instant;

/// Base tile size in pixels (Premium tiles are 32x32).
pub const BASE_TILE: f32 = 32.0;
/// Real-time budget per frame for simulation ticks (keeps 60 fps at "max").
pub const SIM_BUDGET_S: f64 = 0.012;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Speed {
    X1,
    X2,
    X5,
    Max,
}

impl Speed {
    pub const ALL: [Speed; 4] = [Speed::X1, Speed::X2, Speed::X5, Speed::Max];
    /// Target ticks per second (`None` = as fast as possible). 1x = DF's default 100 FPS cap.
    pub fn tps(self) -> Option<f64> {
        match self {
            Speed::X1 => Some(100.0),
            Speed::X2 => Some(200.0),
            Speed::X5 => Some(500.0),
            Speed::Max => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Speed::X1 => "1x",
            Speed::X2 => "2x",
            Speed::X5 => "5x",
            Speed::Max => "max",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    None,
    Designate(DesignationKind),
    /// Index into [`STOCK_PRESETS`].
    Stockpile(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Submenu {
    None,
    Dig,
    Stockpile,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Selected {
    None,
    Unit(i32),
    Tile(Pos),
}

pub struct StockPreset {
    pub label: &'static str,
    pub categories: &'static [&'static str],
    pub icon: &'static str,
    pub glyph: u8,
    pub color: [u8; 3],
}

pub const ALL_STOCK_CATEGORIES: &[&str] = &[
    "animals", "food", "furniture", "corpses", "refuse", "stone", "ammo", "coins", "bars_blocks", "gems",
    "finished_goods", "leather", "cloth", "wood", "weapons", "armor", "sheet",
];

pub const STOCK_PRESETS: &[StockPreset] = &[
    StockPreset { label: "Stone", categories: &["stone"], icon: "STOCKPILE_ICON_STONE", glyph: 7, color: [170, 170, 175] },
    StockPreset { label: "Wood", categories: &["wood"], icon: "STOCKPILE_ICON_WOOD", glyph: b'=', color: [170, 120, 60] },
    StockPreset { label: "Food", categories: &["food"], icon: "STOCKPILE_ICON_FOOD", glyph: b'%', color: [110, 190, 80] },
    StockPreset { label: "Furniture", categories: &["furniture"], icon: "STOCKPILE_ICON_FURNITURE", glyph: 233, color: [170, 120, 200] },
    StockPreset { label: "All", categories: ALL_STOCK_CATEGORIES, icon: "STOCKPILE_ICON_ALL", glyph: b'*', color: [220, 200, 120] },
];

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Tile coordinate at the screen center (smoothed).
    pub x: f32,
    pub y: f32,
    pub z: i32,
    /// Zoom factor: 1.0 = 32 px tiles.
    pub zoom: f32,
    pub tx: f32,
    pub ty: f32,
    pub tzoom: f32,
    /// Screen position the zoom is anchored at.
    pub zoom_anchor: Vec2,
}

impl Camera {
    pub fn tile_px(&self) -> f32 {
        BASE_TILE * self.zoom
    }
    pub fn to_screen(&self, tx: f32, ty: f32) -> Vec2 {
        let tp = self.tile_px();
        vec2((tx - self.x) * tp + screen_width() / 2.0, (ty - self.y) * tp + screen_height() / 2.0)
    }
    pub fn to_tile(&self, s: Vec2) -> (f32, f32) {
        let tp = self.tile_px();
        ((s.x - screen_width() / 2.0) / tp + self.x, (s.y - screen_height() / 2.0) / tp + self.y)
    }
    /// Jump (smoothly) to a map position.
    pub fn focus(&mut self, p: Pos) {
        self.tx = p.x as f32 + 0.5;
        self.ty = p.y as f32 + 0.5;
        self.z = p.z as i32;
    }
}

pub struct LogLine {
    pub text: String,
    pub pos: Option<Pos>,
    pub kind: AnnouncementKind,
    /// Real time (seconds) when it arrived.
    pub born: f64,
}

/// Drag state of a rectangle tool (designations, stockpiles).
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub a: Pos,
    /// Mouse button still held since the first corner.
    pub held: bool,
}

/// Class of a z-level for the elevation bar at the right edge.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZClass {
    Sky,
    Surface,
    Underground,
    Unknown,
}

pub struct Game {
    pub sim: Sim,
    pub gfx: Gfx,
    pub font: Font,
    pub binds: Bindings,
    input_state: InputState,
    pub input: InputFrame,
    pub ui: UiCtx,
    pub rend: MapRenderer,
    pub cam: Camera,
    pub paused: bool,
    pub speed: Speed,
    tick_accum: f64,
    chunk: u32,
    tps_hist: VecDeque<(f64, u64)>,
    /// Ticks per second actually achieved over the last second.
    pub tps: f64,
    pub fps: f32,
    pub sim_ms: f64,
    pub frame_ms: f64,
    pub tool: Tool,
    pub submenu: Submenu,
    pub drag: Option<Drag>,
    pub selected: Selected,
    pub unit_tab: usize,
    pub citizens_open: bool,
    pub citizens_scroll: usize,
    pub menu_open: bool,
    pub log: Vec<LogLine>,
    pub hover: Option<Pos>,
    pub zclass: Vec<ZClass>,
    pub label: String,
    pub ui_scale: f32,
    pub quit: bool,
    pub mouse_override: Option<Vec2>,
    pan_drag: Option<(Vec2, f32, f32)>,
    /// Last feedback message of a tool (shown above the toolbar).
    pub status: Option<(String, f64)>,
}

pub fn ordinal(d: i32) -> String {
    let suf = match (d % 10, d % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{d}{suf}")
}

/// DF-style date: `15th Granite, 250`.
pub fn date_string(c: &rf_core::Calendar) -> String {
    format!("{} {}, {}", ordinal(c.day()), c.month_name(), c.year)
}

/// DF-style season: `Early Spring`.
pub fn season_string(c: &rf_core::Calendar) -> String {
    let part = ["Early", "Mid", "Late"][(c.month().rem_euclid(3)) as usize];
    format!("{part} {}", c.season_name())
}

pub fn compute_zclass(w: &World) -> Vec<ZClass> {
    let (dx, dy) = (w.dims.x as usize, w.dims.y as usize);
    let n = dx * dy;
    let mut out = Vec::with_capacity(w.dims.z as usize);
    for z in 0..w.dims.z as usize {
        let layer = &w.tiles[z * n..(z + 1) * n];
        let (mut sky, mut surf, mut hidden) = (0usize, 0usize, 0usize);
        for t in layer {
            if t.flags & tile_flags::HIDDEN != 0 {
                hidden += 1;
                continue;
            }
            if t.flags & tile_flags::OUTSIDE != 0 {
                let sh = w.tiletypes.get(t.tt).shape;
                if sh.is_open() {
                    sky += 1;
                } else if sh.has_floor() {
                    surf += 1;
                }
            }
        }
        let c = if hidden * 10 > n * 9 {
            ZClass::Unknown
        } else if surf * 50 > n {
            ZClass::Surface
        } else if sky * 2 > n {
            ZClass::Sky
        } else {
            ZClass::Underground
        };
        out.push(c);
    }
    out
}

impl Game {
    pub fn new(args: &Args, loaded: Loaded) -> Game {
        let df = rf_gfx::df_dir();
        let gfx = match Gfx::load(&df) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("graphics: {e}; using fallback tiles");
                Gfx::default()
            }
        };
        let font = Font::load(&df);
        let binds = Bindings::load(&df);
        let world = loaded.world;
        let zclass = compute_zclass(&world);
        let cam = initial_camera(&world);
        let sim = Sim::new(world, args.seed);
        let mut g = Game {
            sim,
            gfx,
            font,
            binds,
            input_state: InputState::default(),
            input: InputFrame::default(),
            ui: UiCtx::default(),
            rend: MapRenderer::default(),
            cam,
            paused: true,
            speed: Speed::X1,
            tick_accum: 0.0,
            chunk: 1,
            tps_hist: VecDeque::new(),
            tps: 0.0,
            fps: 60.0,
            sim_ms: 0.0,
            frame_ms: 0.0,
            tool: Tool::None,
            submenu: Submenu::None,
            drag: None,
            selected: Selected::None,
            unit_tab: 0,
            citizens_open: false,
            citizens_scroll: 0,
            menu_open: false,
            log: Vec::new(),
            hover: None,
            zclass,
            label: loaded.label,
            ui_scale: 1.0,
            quit: false,
            mouse_override: args.mouse.map(|(x, y)| vec2(x, y)),
            pan_drag: None,
            status: None,
        };
        let now = get_time();
        if let Some(w) = loaded.warning {
            g.log.push(LogLine { text: w, pos: None, kind: AnnouncementKind::Warning, born: now });
        }
        if let Some(r) = &loaded.report {
            g.log.push(LogLine {
                text: format!(
                    "Imported {} units ({} citizens), {} items, {} buildings in {:.1}s; {} warnings",
                    r.units, r.citizens, r.items, r.buildings, r.seconds, r.warnings.len()
                ),
                pos: None,
                kind: AnnouncementKind::Info,
                born: now,
            });
        }
        g.log.push(LogLine {
            text: format!("Welcome to {}. Space unpauses; keys from {}", g.sim.world.meta.fort_name, g.binds.source),
            pos: None,
            kind: AnnouncementKind::Info,
            born: now,
        });
        if g.gfx.pages.is_empty() {
            g.log.push(LogLine {
                text: "Premium tileset not loaded yet (rf-gfx stub): drawing fallback tiles".into(),
                pos: None,
                kind: AnnouncementKind::Info,
                born: now,
            });
        }
        g.apply_args(args);
        g
    }

    fn apply_args(&mut self, args: &Args) {
        if let Some((x, y, z, zoom)) = args.view {
            self.cam.x = x;
            self.cam.y = y;
            self.cam.tx = x;
            self.cam.ty = y;
            self.cam.z = z.clamp(0, self.sim.world.dims.z as i32 - 1);
            self.cam.zoom = zoom.clamp(0.2, 4.0);
            self.cam.tzoom = self.cam.zoom;
        }
        if let Some(s) = &args.speed {
            self.paused = false;
            self.speed = match s.as_str() {
                "2" | "2x" => Speed::X2,
                "5" | "5x" => Speed::X5,
                "max" => Speed::Max,
                _ => Speed::X1,
            };
        }
        for u in &args.ui {
            let (k, v) = u.split_once(':').unwrap_or((u.as_str(), ""));
            match k {
                "citizens" => self.citizens_open = true,
                "dig" => {
                    self.submenu = Submenu::Dig;
                    self.tool = Tool::Designate(DesignationKind::Mine);
                }
                "chop" => self.tool = Tool::Designate(DesignationKind::ChopTrees),
                "stockpile" => {
                    self.submenu = Submenu::Stockpile;
                    self.tool = Tool::Stockpile(0);
                }
                "menu" => self.menu_open = true,
                "unit" => {
                    let id = v.parse::<i32>().ok().or_else(|| self.sim.world.citizens().next().map(|u| u.id));
                    if let Some(id) = id {
                        self.selected = Selected::Unit(id);
                        self.unit_tab = 0;
                    }
                }
                "skills" => self.unit_tab = 1,
                "labor" => self.unit_tab = 2,
                "items" => self.unit_tab = 3,
                "tile" => {
                    let c = self.cam;
                    self.selected = Selected::Tile(Pos::new(c.x as i16, c.y as i16, c.z as i16));
                }
                "drag" => {
                    // Pending rectangle from the camera center to the mouse.
                    let c = self.cam;
                    self.drag = Some(Drag { a: Pos::new(c.x as i16 - 3, c.y as i16 - 2, c.z as i16), held: false });
                }
                _ => eprintln!("unknown --ui state {u}"),
            }
        }
    }

    pub fn mouse(&self) -> Vec2 {
        self.mouse_override.unwrap_or_else(|| {
            let (x, y) = mouse_position();
            vec2(x, y)
        })
    }

    /// Tile under a screen position at the current z-level.
    pub fn tile_at(&self, s: Vec2) -> Option<Pos> {
        let (tx, ty) = self.cam.to_tile(s);
        let p = Pos::new(tx.floor() as i16, ty.floor() as i16, self.cam.z as i16);
        (tx >= 0.0 && ty >= 0.0 && self.sim.world.in_bounds(p)).then_some(p)
    }

    pub fn set_status(&mut self, s: String) {
        self.status = Some((s, get_time()));
    }

    // ---------------------------------------------------------------- simulation

    fn update_sim(&mut self, dt: f32) {
        let t0 = Instant::now();
        if !self.paused && !self.menu_open {
            match self.speed.tps() {
                Some(tps) => {
                    self.tick_accum = (self.tick_accum + dt as f64 * tps).min(tps * 0.25);
                    let mut todo = self.tick_accum.floor() as u32;
                    while todo > 0 && t0.elapsed().as_secs_f64() < SIM_BUDGET_S {
                        let n = todo.min(self.chunk.max(1));
                        let c0 = Instant::now();
                        self.sim.run_ticks(n);
                        self.adapt_chunk(n, c0.elapsed().as_secs_f64());
                        todo -= n;
                        self.tick_accum -= n as f64;
                    }
                }
                None => {
                    self.tick_accum = 0.0;
                    while t0.elapsed().as_secs_f64() < SIM_BUDGET_S {
                        let n = self.chunk.max(1);
                        let c0 = Instant::now();
                        self.sim.run_ticks(n);
                        self.adapt_chunk(n, c0.elapsed().as_secs_f64());
                    }
                }
            }
        }
        self.sim_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let now = get_time();
        self.tps_hist.push_back((now, self.sim.ticks()));
        while self.tps_hist.len() > 2 && now - self.tps_hist[0].0 > 1.0 {
            self.tps_hist.pop_front();
        }
        if let (Some(a), Some(b)) = (self.tps_hist.front(), self.tps_hist.back()) {
            if b.0 - a.0 > 0.2 {
                self.tps = (b.1 - a.1) as f64 / (b.0 - a.0);
            }
        }
        for a in self.sim.drain_announcements() {
            self.log.push(LogLine { text: a.text, pos: a.pos, kind: a.kind, born: now });
        }
        if self.log.len() > 500 {
            self.log.drain(..self.log.len() - 500);
        }
    }

    /// Keep each `run_ticks` call around 1 ms so the frame budget is respected.
    fn adapt_chunk(&mut self, n: u32, secs: f64) {
        let per_tick = secs / n.max(1) as f64;
        let want = if per_tick > 0.0 { (0.001 / per_tick) as u32 } else { 10_000 };
        self.chunk = want.clamp(1, 10_000);
    }

    // ---------------------------------------------------------------- input

    fn global_keys(&mut self) {
        let b = &self.binds;
        let inp = &self.input;
        if inp.hit(b, "LEAVESCREEN") {
            self.escape();
            return;
        }
        if self.menu_open {
            return;
        }
        if inp.hit(b, "D_PAUSE") {
            self.paused = !self.paused;
        }
        if self.paused && inp.hit(b, "D_ONESTEP") {
            self.sim.run_ticks(1);
        }
        for (i, sp) in Speed::ALL.iter().enumerate() {
            if inp.hit(b, &format!("RF_SPEED_{}", i + 1)) {
                self.speed = *sp;
                self.paused = false;
            }
        }
        // Context: inside the dig menu the DESIGNATE_* keys pick the dig type.
        if self.submenu == Submenu::Dig {
            for d in crate::ui::toolbar::DIG_MENU {
                if !d.bind.is_empty() && inp.hit(b, d.bind) {
                    self.tool = Tool::Designate(d.kind);
                    return;
                }
            }
        }
        if inp.hit(b, "D_DESIGNATE_DIG") {
            self.open_dig();
        } else if inp.hit(b, "D_DESIGNATE_CHOP") {
            self.submenu = Submenu::None;
            self.tool = Tool::Designate(DesignationKind::ChopTrees);
        } else if inp.hit(b, "D_DESIGNATE_GATHER") {
            self.submenu = Submenu::None;
            self.tool = Tool::Designate(DesignationKind::GatherPlants);
        } else if inp.hit(b, "D_DESIGNATE_ERASE") {
            self.submenu = Submenu::None;
            self.tool = Tool::Designate(DesignationKind::Remove);
        } else if inp.hit(b, "D_STOCKPILES") {
            self.open_stockpiles();
        } else if inp.hit(b, "D_UNITLIST") {
            self.citizens_open = !self.citizens_open;
        }
    }

    pub fn open_dig(&mut self) {
        if self.submenu == Submenu::Dig {
            self.submenu = Submenu::None;
            self.tool = Tool::None;
        } else {
            self.submenu = Submenu::Dig;
            self.tool = Tool::Designate(DesignationKind::Mine);
        }
        self.drag = None;
    }

    pub fn open_stockpiles(&mut self) {
        if self.submenu == Submenu::Stockpile {
            self.submenu = Submenu::None;
            self.tool = Tool::None;
        } else {
            self.submenu = Submenu::Stockpile;
            self.tool = Tool::Stockpile(0);
        }
        self.drag = None;
    }

    /// Esc backs out one level, like DF: menu, rectangle, tool, panels, selection, then the
    /// options menu.
    pub fn escape(&mut self) {
        if self.menu_open {
            self.menu_open = false;
        } else if self.drag.is_some() {
            self.drag = None;
        } else if self.tool != Tool::None || self.submenu != Submenu::None {
            self.tool = Tool::None;
            self.submenu = Submenu::None;
        } else if self.citizens_open {
            self.citizens_open = false;
        } else if self.selected != Selected::None {
            self.selected = Selected::None;
        } else {
            self.menu_open = true;
        }
    }

    fn update_camera(&mut self, dt: f32) {
        let dims = self.sim.world.dims;
        if !self.menu_open {
            let b = &self.binds;
            let inp = &self.input;
            // Pan: WASD / arrows (held), Shift = fast.
            let mut v = Vec2::ZERO;
            if b.held("CURSOR_UP", inp) {
                v.y -= 1.0;
            }
            if b.held("CURSOR_DOWN", inp) {
                v.y += 1.0;
            }
            if b.held("CURSOR_LEFT", inp) {
                v.x -= 1.0;
            }
            if b.held("CURSOR_RIGHT", inp) {
                v.x += 1.0;
            }
            if v != Vec2::ZERO {
                let fast = if inp.mods & SHIFT != 0 { 3.5 } else { 1.0 };
                let px_per_s = 950.0 * fast;
                let d = v.normalize() * px_per_s * dt / self.cam.tile_px();
                self.cam.tx += d.x;
                self.cam.ty += d.y;
            }
            // Z-levels (bindings from DF's interface.txt, incl. the mouse wheel).
            let mut dz = 0;
            dz += inp.count(b, "CURSOR_UP_Z") + inp.count(b, "CURSOR_UP_Z_AUX");
            dz -= inp.count(b, "CURSOR_DOWN_Z") + inp.count(b, "CURSOR_DOWN_Z_AUX");
            dz += 10 * inp.count(b, "CURSOR_UP_Z_FAST");
            dz -= 10 * inp.count(b, "CURSOR_DOWN_Z_FAST");
            if dz != 0 {
                self.cam.z = (self.cam.z + dz).clamp(0, dims.z as i32 - 1);
            }
            // Zoom around the mouse.
            let zin = inp.count(b, "ZOOM_IN");
            let zout = inp.count(b, "ZOOM_OUT");
            if zin + zout > 0 {
                let f = 1.25f32.powi(zin - zout);
                self.cam.tzoom = (self.cam.tzoom * f).clamp(0.2, 4.0);
                self.cam.zoom_anchor = self.mouse();
            }
        }
        // Middle mouse drag pans 1:1.
        let m = self.mouse();
        if is_mouse_button_pressed(MouseButton::Middle) && !self.ui.over_ui {
            self.pan_drag = Some((m, self.cam.x, self.cam.y));
        }
        if let Some((start, cx, cy)) = self.pan_drag {
            if is_mouse_button_down(MouseButton::Middle) {
                let tp = self.cam.tile_px();
                self.cam.x = cx - (m.x - start.x) / tp;
                self.cam.y = cy - (m.y - start.y) / tp;
                self.cam.tx = self.cam.x;
                self.cam.ty = self.cam.y;
            } else {
                self.pan_drag = None;
            }
        }
        // Smooth zoom anchored at the mouse, smooth pan.
        let k = 1.0 - (-dt * 16.0).exp();
        if (self.cam.zoom - self.cam.tzoom).abs() > 1e-4 {
            let a = self.cam.zoom_anchor;
            let (wx, wy) = self.cam.to_tile(a);
            let nz = if (self.cam.zoom - self.cam.tzoom).abs() < 0.002 {
                self.cam.tzoom
            } else {
                self.cam.zoom + (self.cam.tzoom - self.cam.zoom) * k
            };
            self.cam.zoom = nz;
            let (wx2, wy2) = self.cam.to_tile(a);
            self.cam.x += wx - wx2;
            self.cam.y += wy - wy2;
            self.cam.tx += wx - wx2;
            self.cam.ty += wy - wy2;
        }
        self.cam.tx = self.cam.tx.clamp(0.0, dims.x as f32);
        self.cam.ty = self.cam.ty.clamp(0.0, dims.y as f32);
        self.cam.x += (self.cam.tx - self.cam.x) * k;
        self.cam.y += (self.cam.ty - self.cam.y) * k;
    }

    /// Map clicks (after the UI had its chance).
    fn map_input(&mut self) {
        let m = self.mouse();
        self.hover = if self.ui.over_ui || self.menu_open { None } else { self.tile_at(m) };
        if self.menu_open {
            return;
        }
        let lp = self.ui.lpressed && !self.ui.click_used && !self.ui.over_ui;
        let lr = is_mouse_button_released(MouseButton::Left);
        let rp = self.ui.rpressed && !self.ui.over_ui;
        if rp {
            // Right click backs out: pending rectangle, then tool, then selection.
            if self.drag.is_some() {
                self.drag = None;
            } else if self.tool != Tool::None {
                self.tool = Tool::None;
                self.submenu = Submenu::None;
            } else {
                self.selected = Selected::None;
            }
            return;
        }
        let Some(h) = self.hover else { return };
        match self.tool {
            Tool::None => {
                if lp {
                    self.select_at(h);
                }
            }
            Tool::Designate(_) | Tool::Stockpile(_) => {
                if lp {
                    match self.drag {
                        None => self.drag = Some(Drag { a: h, held: true }),
                        Some(d) => {
                            self.drag = None;
                            self.apply_tool(d.a, h);
                        }
                    }
                } else if lr {
                    if let Some(d) = self.drag {
                        if d.held {
                            if d.a != h {
                                self.drag = None;
                                self.apply_tool(d.a, h);
                            } else {
                                // A plain click: wait for the second corner (Steam style).
                                self.drag = Some(Drag { a: d.a, held: false });
                            }
                        }
                    }
                }
            }
        }
    }

    fn select_at(&mut self, p: Pos) {
        let w = &self.sim.world;
        let units: Vec<i32> = w.units.iter().filter(|u| u.pos == p && !u.flags.dead).map(|u| u.id).collect();
        if !units.is_empty() {
            // Clicking the same tile again cycles through the units on it.
            let next = match self.selected {
                Selected::Unit(cur) => match units.iter().position(|&i| i == cur) {
                    Some(k) => units[(k + 1) % units.len()],
                    None => units[0],
                },
                _ => units[0],
            };
            self.selected = Selected::Unit(next);
        } else {
            self.selected = Selected::Tile(p);
        }
    }

    fn apply_tool(&mut self, a: Pos, b: Pos) {
        let (dx, dy, dz) = ((a.x - b.x).abs() + 1, (a.y - b.y).abs() + 1, (a.z - b.z).abs() + 1);
        match self.tool {
            Tool::Designate(kind) => {
                self.sim.designate(a, b, kind);
                self.set_status(format!("{kind:?}: {dx}x{dy}x{dz} = {} tiles", dx as i32 * dy as i32 * dz as i32));
            }
            Tool::Stockpile(i) => {
                let p = &STOCK_PRESETS[i.min(STOCK_PRESETS.len() - 1)];
                let a2 = Pos::new(a.x, a.y, b.z);
                let id = self.sim.create_stockpile(a2, b, p.categories);
                self.set_status(format!("{} stockpile #{id}: {dx}x{dy}", p.label));
            }
            Tool::None => {}
        }
    }
}

fn initial_camera(w: &World) -> Camera {
    let (mut x, mut y, mut z) = (w.dims.x as f32 / 2.0, w.dims.y as f32 / 2.0, w.dims.z as i32 / 2);
    let cits: Vec<_> = w.citizens().collect();
    if !cits.is_empty() {
        // Center on the z-level with the most citizens.
        let mut best = (0, cits[0].pos.z);
        for c in &cits {
            let n = cits.iter().filter(|o| o.pos.z == c.pos.z).count();
            if n > best.0 {
                best = (n, c.pos.z);
            }
        }
        let on: Vec<_> = cits.iter().filter(|c| c.pos.z == best.1).collect();
        x = on.iter().map(|c| c.pos.x as f32).sum::<f32>() / on.len() as f32 + 0.5;
        y = on.iter().map(|c| c.pos.y as f32).sum::<f32>() / on.len() as f32 + 0.5;
        z = best.1 as i32;
    } else {
        // Topmost z-level with ground in the middle.
        for zz in (0..w.dims.z as i16).rev() {
            let s = w.shape(Pos::new(x as i16, y as i16, zz));
            if s != Shape::Empty && s != Shape::None {
                z = zz as i32;
                break;
            }
        }
    }
    Camera { x, y, z, zoom: 1.0, tx: x, ty: y, tzoom: 1.0, zoom_anchor: Vec2::ZERO }
}

pub async fn run(args: Args, loaded: Loaded) {
    let mut g = Game::new(&args, loaded);
    let mut frame: u32 = 0;
    loop {
        let f0 = Instant::now();
        let dt = get_frame_time().min(0.1);
        g.fps = g.fps * 0.95 + (1.0 / dt.max(1e-4)) * 0.05;
        g.input = g.input_state.collect(&g.binds);
        // The wheel over a UI panel scrolls the panel, not the z-level (state of last frame).
        if g.ui.over_ui {
            g.input.wheel_used = true;
        }
        g.global_keys();
        g.update_sim(dt);
        g.update_camera(dt);

        clear_background(Color::from_rgba(0, 0, 0, 255));
        crate::render::draw_map(&mut g);
        let m = g.mouse();
        let scale = g.ui_scale;
        g.ui.begin(m, scale);
        if g.mouse_override.is_some() {
            g.ui.lpressed = false;
            g.ui.rpressed = false;
        }
        crate::ui::overlay::draw_ui(&mut g);
        g.map_input();
        g.frame_ms = f0.elapsed().as_secs_f64() * 1000.0;

        frame += 1;
        if let Some(path) = &args.screenshot {
            if frame >= args.frames.max(1) {
                let img = get_screen_data();
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                img.export_png(&path.to_string_lossy());
                println!(
                    "saved {} ({}x{}) after {frame} frames, sim tick {}",
                    path.display(),
                    img.width,
                    img.height,
                    g.sim.ticks()
                );
                break;
            }
        }
        if g.quit {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_like_df() {
        let mut c = rf_core::Calendar { year: 250, year_tick: 0 };
        assert_eq!(date_string(&c), "1st Granite, 250");
        assert_eq!(season_string(&c), "Early Spring");
        c.year_tick = rf_core::TICKS_PER_DAY * 14;
        assert_eq!(date_string(&c), "15th Granite, 250");
        c.year_tick = rf_core::TICKS_PER_MONTH * 4 + rf_core::TICKS_PER_DAY * 21;
        assert_eq!(date_string(&c), "22nd Malachite, 250");
        assert_eq!(season_string(&c), "Mid Summer");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(23), "23rd");
    }

    #[test]
    fn zclass_of_synthetic_world() {
        let w = World::synthetic(32, 32, 20);
        let zc = compute_zclass(&w);
        assert_eq!(zc.len(), 20);
        assert_eq!(*zc.last().unwrap(), ZClass::Sky);
        assert_eq!(zc[0], ZClass::Underground);
        assert!(zc.contains(&ZClass::Surface));
    }

    #[test]
    fn bench_sim_runs_through_the_api() {
        let w = World::synthetic(32, 32, 20);
        let mut s = Sim::new(w, 1);
        s.run_ticks(50);
        assert_eq!(s.ticks(), 50);
    }
}
