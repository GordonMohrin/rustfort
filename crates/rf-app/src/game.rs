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
use rf_sim::{AnnouncementKind, BuildOption, DesignationKind, Sim};
use std::collections::VecDeque;
use std::time::Instant;

/// Base tile size in pixels (Premium tiles are 32x32).
pub const BASE_TILE: f32 = 32.0;
/// Real-time budget per frame for simulation ticks (keeps 60 fps at "max").
pub const SIM_BUDGET_S: f64 = 0.012;
/// Announcements kept for the Reports screen.
pub const LOG_KEEP: usize = 5000;

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
    /// Index into `Game::build_opts` (= `Sim::build_options()`).
    Build(usize),
    /// Index into [`rf_sim::ZONE_KINDS`].
    Zone(usize),
    /// Drag a box to create a burrow.
    Burrow,
    /// Click a tile to station this squad there.
    SquadStation(i32),
    /// Click a creature for this squad to kill.
    SquadKill(i32),
    /// [round 4] Click a bridge/door/floodgate/hatch/... to link it to this lever or plate.
    Link(i32),
    /// [round 5] Build > Constructions: drag a rectangle (one level).
    Construct(rf_sim::ConstructionKind),
    /// [round 5] Smooth / engrave / carve: drag a box (may span levels).
    Detail(rf_sim::DetailKind),
    /// [round 5] Traffic designation: drag a box.
    Traffic(rf_sim::Traffic),
    /// [round 5] Item designations (claim, forbid, dump, melt, hide): drag a box.
    ItemDes(crate::ui::round5::ItemDes),
    /// [app8] Click a stockpile or workshop to link this stockpile to (give = true) or from.
    PileLink(i32, bool),
    /// [app9] Click the waypoints of this squad's patrol route (Enter finishes).
    Patrol(i32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Submenu {
    None,
    Dig,
    Stockpile,
    Build,
    Zone,
    Burrow,
    /// [round 5] Smooth / engrave / carve.
    Smooth,
    /// [round 5] Traffic designations.
    Traffic,
    /// [round 5] Item designations.
    Items,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Selected {
    None,
    Unit(i32),
    Tile(Pos),
    Building(i32),
}

/// Full-window info screens (DF's work orders and labor windows).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    None,
    WorkOrders,
    Labor,
    /// Squads (DF hotkey `c`).
    Military,
    /// Reports / announcement history (DF `Shift+n`).
    Reports,
    /// [round 4] Trade screen at the depot.
    Trade,
    /// [round 4] Nobles and administrators.
    Nobles,
    /// [app8] Stockpile settings editor (`R8Ui::pile`).
    StockpileSettings,
    /// [app8] Pets and livestock.
    Animals,
    /// [app8] Places: locations, zones, stockpiles, workshops, farm plots (DF `P`).
    Places,
    /// [app8] Justice (DF `j`).
    Justice,
    /// [app8] Objects: artifacts (DF `O`).
    Objects,
    /// [app8] World: civilizations, caravans (DF `Y`).
    World,
    /// [app9] Visitor petitions (DF v50 petitions window).
    Petitions,
}

/// Sort order of the citizens list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CitSort {
    #[default]
    Name,
    Profession,
    /// Most stressed first.
    Stress,
}

/// Military (squads) screen state.
#[derive(Default)]
pub struct MilitaryUi {
    /// Selected squad id.
    pub squad: Option<i32>,
    /// Squad position whose citizen picker is open.
    pub picker: Option<usize>,
    pub scroll: usize,
    pub pscroll: usize,
    /// The "Kill" target list is open.
    pub kill_list: bool,
}

/// Burrow menu state.
#[derive(Default)]
pub struct BurrowUi {
    pub selected: Option<i32>,
    pub scroll: usize,
}

/// Reports screen state.
#[derive(Default)]
pub struct ReportsUi {
    /// Index into `ui::reports::FILTERS`.
    pub filter: usize,
    /// Lines scrolled up from the newest.
    pub scroll: usize,
}

/// A DF-style pop-up alert (siege, ambush, monster): the game pauses until it is closed.
#[derive(Clone, Debug)]
pub struct Alert {
    pub title: String,
    pub text: String,
    /// Hostile units of this wave.
    pub units: Vec<i32>,
    pub pos: Option<Pos>,
}

/// Work orders screen state.
#[derive(Default)]
pub struct OrdersUi {
    /// The "new order" picker is open.
    pub new_open: bool,
    /// Selected template (index into `Sim::order_templates()`).
    pub template: usize,
    pub amount: i32,
    /// Index into [`FREQUENCIES`].
    pub freq: usize,
    pub scroll: usize,
    pub tscroll: usize,
}

pub const FREQUENCIES: &[(&str, &str)] =
    &[("OneTime", "One time"), ("Daily", "Daily"), ("Monthly", "Monthly"), ("Seasonally", "Seasonally"), ("Yearly", "Yearly")];

/// Labor screen state.
#[derive(Default)]
pub struct LaborUi {
    pub scroll: usize,
    /// Index into `ui::labor::GROUPS`.
    pub group: usize,
    pub filter: String,
    /// The name filter has keyboard focus.
    pub typing: bool,
}

/// Building sheet state.
#[derive(Default)]
pub struct BuildingUi {
    /// The workshop "Add job" list is open.
    pub add_open: bool,
    /// New workshop jobs are added as repeating.
    pub repeat: bool,
    /// Farm plot: season whose crop list is open.
    pub crop_season: Option<usize>,
    pub scroll: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MenuPage {
    #[default]
    Main,
    Load,
    /// [round 4] Population cap, invasions.
    Settings,
    /// [app9] Announcement settings (DF announcements.txt).
    Announcements,
}

/// Esc menu state.
#[derive(Default)]
pub struct MenuUi {
    pub page: MenuPage,
    pub saves: Vec<crate::save::SaveInfo>,
    pub scroll: usize,
}

/// Stocks readout of the top bar (recounted once per second).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Stocks {
    pub drinks: i32,
    pub food: i32,
    /// Distinct edible materials (DF's "food variety").
    pub food_kinds: usize,
    pub at: f64,
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
    /// Game date when it arrived (`15th Granite, 274`).
    pub date: String,
    /// Reports category (`ui::reports::category`), computed once.
    pub cat: u8,
    /// [app9] Not shown in the announcement lines (DF `D_D` off); Reports keep it.
    pub quiet: bool,
}

impl LogLine {
    pub fn new(text: String, pos: Option<Pos>, kind: AnnouncementKind, born: f64, date: String) -> LogLine {
        let cat = crate::ui::reports::category(kind, &text) as u8;
        LogLine { text, pos, kind, born, date, cat, quiet: false }
    }
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
    /// [round 4] The simulation; runs on a worker thread between frames (see `simthread`).
    pub sim: crate::simthread::SimHandle,
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
    pan_drag: Option<(Vec2, f32, f32)>,
    /// Last feedback message of a tool (shown above the toolbar).
    pub status: Option<(String, f64)>,
    /// Camera follows this unit (unit sheet "Follow").
    pub follow: Option<i32>,
    /// Real-time budget per frame for sim ticks (seconds).
    pub sim_budget: f64,
    /// Time of the last `Gfx::prepare` (new items/units need palette variants).
    pub prepared_at: f64,
    pub seed: u64,
    pub screen: Screen,
    /// `Sim::build_options()`, refreshed when the build menu opens.
    pub build_opts: Vec<BuildOption>,
    /// Selected category in the build menu (index into `ui::build::menu_categories()`).
    pub build_cat: usize,
    /// Cached `Sim::can_place` answer for (option, a, b).
    pub build_check: Option<(usize, Pos, Pos, Result<(), String>)>,
    pub orders: OrdersUi,
    pub labor: LaborUi,
    pub bui: BuildingUi,
    pub menu: MenuUi,
    pub stocks: Stocks,
    /// Save running in a background thread.
    pub saving: Option<std::thread::JoinHandle<Result<crate::save::SaveInfo, String>>>,
    /// Scroll offset of the current unit sheet tab.
    pub unit_scroll: usize,
    pub citizens_sort: CitSort,
    pub mil: MilitaryUi,
    pub burrow_ui: BurrowUi,
    pub reports: ReportsUi,
    /// `Sim::hostiles()` of this frame, sorted.
    pub hostiles: Vec<i32>,
    /// Hostile ids already seen (so only new arrivals raise an alert), sorted.
    pub hostiles_seen: Vec<i32>,
    /// Sim tick of the last hostile alert (waves arriving over a few ticks raise one alert).
    pub alert_tick: Option<u64>,
    pub alert: Option<Alert>,
    /// Burrow of the active civilian alert.
    pub civ_alert: Option<i32>,
    /// Next hostile the top bar's hostile counter centers on.
    pub hostile_cycle: usize,
    /// Tab of the creatures list: 0 citizens, 1 pets/livestock, 2 others.
    pub creatures_tab: usize,
    // [round 4] sim worker thread bookkeeping, devices, trade, nobles, settings.
    /// Sim tick when the sim was handed to the worker (to book the ticks it ran).
    released_at_tick: u64,
    /// Worker busy time at the last frame (seconds).
    busy_prev: f64,
    pub r4: crate::ui::round4::R4Ui,
    /// [round 5] Construction/detailing/traffic/item tools, cage lookups.
    pub r5: crate::ui::round5::R5Ui,
    // [round 6] settings, help, hints, autosave, leaving to the title screen.
    pub settings: crate::settings::Settings,
    /// The controls overview (F1) is open.
    pub help_open: bool,
    /// First-time hints are shown (off in screenshot runs unless `--ui hints`).
    pub hints_enabled: bool,
    /// Settings changes are written to disk (not in screenshot runs).
    pub settings_writable: bool,
    /// Absolute game day (year * 336 + day of year) of the next autosave.
    pub autosave_next: Option<i64>,
    /// Autosave interval in game days (0 = off).
    pub autosave_days: u32,
    /// The running background save is an autosave.
    saving_auto: bool,
    /// Set to leave the game loop.
    pub exit: Option<Exit>,
    /// [app8] Work details, kitchen, stockpile settings, animals, places, justice, objects,
    /// world, minimap.
    pub r8: crate::ui::round8::R8Ui,
    /// [app9] Announcement settings and boxes, petitions, squad equipment, combat reports.
    pub r9: crate::ui::round9::R9Ui,
}

/// [round 6] How the game loop ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exit {
    Quit,
    /// Back to the title screen.
    MainMenu,
}

/// [round 6] Absolute game day: `year * 336 + day of the year`.
pub fn game_day(c: &rf_core::Calendar) -> i64 {
    c.year as i64 * (rf_core::TICKS_PER_YEAR / rf_core::TICKS_PER_DAY) as i64 + (c.year_tick / rf_core::TICKS_PER_DAY) as i64
}

/// [round 6] First autosave day after `day` for an interval of `every` days: the next multiple
/// of `every` (for 84 days that is the first day of the next season, like DF's seasonal autosave).
pub fn next_autosave_day(day: i64, every: u32) -> Option<i64> {
    (every > 0).then(|| (day.div_euclid(every as i64) + 1) * every as i64)
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
    pub fn new(args: &Args, mut loaded: Loaded, settings: crate::settings::Settings) -> Game {
        let df = rf_gfx::df_dir();
        let world = std::mem::take(&mut loaded.world);
        // [round 6] The loading thread may have loaded (and prepared) the graphics already.
        let gfx = match loaded.gfx.take() {
            Some(g) => g,
            None => {
                // Graphics of exactly the save's active mods (vanilla only if the list is empty).
                let mut gfx = match Gfx::load_for_world(&df, &world) {
                    Ok(g) => g,
                    Err(e) => {
                        eprintln!("graphics: {e}; using fallback tiles");
                        Gfx::default()
                    }
                };
                // Bake the palette variants (stone colors, dwarf looks) this world needs.
                gfx.prepare(&world);
                gfx
            }
        };
        let font = Font::load(&df);
        let binds = Bindings::load(&df);
        let zclass = compute_zclass(&world);
        let cam = initial_camera(&world);
        let threaded = !args.sim_inline && std::env::var("RUSTFORT_SIM_THREAD").map(|v| v != "0").unwrap_or(true);
        let seed = if loaded.seed != 0 { loaded.seed } else { args.seed };
        let sim = crate::simthread::SimHandle::new(Sim::new(world, seed), threaded);
        let shot = args.screenshot.is_some();
        let autosave_days = match args.autosave_days {
            Some(d) => d,
            None if shot => 0,
            None => settings.autosave_days,
        };
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
            label: std::mem::take(&mut loaded.label),
            ui_scale: args.ui_scale,
            quit: false,
            pan_drag: None,
            status: None,
            follow: None,
            sim_budget: SIM_BUDGET_S.min(0.75 / args.gfps.max(10) as f64),
            prepared_at: 0.0,
            seed,
            screen: Screen::None,
            build_opts: Vec::new(),
            build_cat: 0,
            build_check: None,
            orders: OrdersUi { amount: 10, ..Default::default() },
            labor: LaborUi::default(),
            bui: BuildingUi::default(),
            menu: MenuUi::default(),
            stocks: Stocks::default(),
            saving: None,
            unit_scroll: 0,
            citizens_sort: CitSort::Name,
            mil: MilitaryUi::default(),
            burrow_ui: BurrowUi::default(),
            reports: ReportsUi::default(),
            hostiles: Vec::new(),
            hostiles_seen: Vec::new(),
            alert_tick: None,
            alert: None,
            civ_alert: None,
            hostile_cycle: 0,
            creatures_tab: 0,
            released_at_tick: 0,
            busy_prev: 0.0,
            r4: Default::default(),
            r5: Default::default(),
            settings,
            help_open: false,
            hints_enabled: !shot,
            settings_writable: !shot,
            autosave_next: None,
            autosave_days,
            saving_auto: false,
            exit: None,
            r8: Default::default(),
            r9: Default::default(),
        };
        // [round 6] Defaults from the settings for a fresh start (imports, synthetic forts).
        if loaded.new_game {
            if let Some(cap) = g.settings.pop_cap {
                g.sim.set_population_cap(cap);
            }
            if let Some(on) = g.settings.invasions {
                g.sim.set_invasions(on);
            }
        }
        g.autosave_next = next_autosave_day(game_day(&g.sim.world.calendar), g.autosave_days);
        crate::ui::round8::apply_kitchen_default(&mut g); // [app8]
        g.r8.minimap = g.settings.minimap && !shot; // [app8]
        g.reset_hostiles();
        g.build_opts = g.sim.build_options();
        g.stocks = crate::ui::topbar::count_stocks(&g.sim.world, 0.0);
        let now = get_time();
        if let Some(w) = loaded.warning {
            g.log.push(LogLine::new(w, None, AnnouncementKind::Warning, now, date_string(&g.sim.world.calendar)));
        }
        if let Some(r) = &loaded.report {
            g.log.push(LogLine::new(
                format!(
                    "Imported {} units ({} citizens), {} items, {} buildings in {:.1}s; {} warnings",
                    r.units, r.citizens, r.items, r.buildings, r.seconds, r.warnings.len()
                ),
                None,
                AnnouncementKind::Info,
                now,
                date_string(&g.sim.world.calendar),
            ));
        }
        g.log.push(LogLine::new(
            format!(
                "Welcome to {}. Space unpauses. Keys: {}",
                g.sim.world.meta.fort_name,
                if g.binds.source.contains("Roaming") { "your DF prefs" } else { g.binds.source.as_str() }
            ),
            None,
            AnnouncementKind::Info,
            now,
            date_string(&g.sim.world.calendar),
        ));
        if g.gfx.pages.is_empty() {
            g.log.push(LogLine::new(
                "Premium tileset not loaded yet (rf-gfx stub): drawing fallback tiles".into(),
                None,
                AnnouncementKind::Info,
                now,
                date_string(&g.sim.world.calendar),
            ));
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
                "pets" => {
                    self.citizens_open = true;
                    self.creatures_tab = 1;
                }
                "others" => {
                    self.citizens_open = true;
                    self.creatures_tab = 2;
                }
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
                "loadmenu" => self.open_load_menu(),
                "build" => {
                    self.open_build();
                    if let Ok(c) = v.parse::<usize>() {
                        self.build_cat = c;
                    }
                }
                "place" => {
                    // Build tool with option `v` (index or label) selected.
                    self.open_build();
                    let i = v.parse::<usize>().ok().or_else(|| self.build_opts.iter().position(|o| o.label == v || o.subtype == v || o.kind == v));
                    if let Some(i) = i.filter(|&i| i < self.build_opts.len()) {
                        let cats = crate::ui::build::menu_categories(&self.build_opts);
                        self.build_cat = cats.iter().position(|c| *c == self.build_opts[i].category).unwrap_or(0);
                        self.tool = Tool::Build(i);
                    }
                }
                "zone" => {
                    self.open_zones();
                    if let Ok(k) = v.parse::<usize>() {
                        self.tool = Tool::Zone(k.min(rf_sim::ZONE_KINDS.len() - 1));
                    }
                }
                "orders" => self.screen = Screen::WorkOrders,
                "neworder" => {
                    self.screen = Screen::WorkOrders;
                    self.orders.new_open = true;
                    if let Ok(t) = v.parse::<usize>() {
                        self.orders.template = t;
                    }
                }
                "labor-screen" => self.screen = Screen::Labor,
                "bld" => {
                    // Select a building by id or by kind/subtype (first match).
                    let w = &self.sim.world;
                    let id = v.parse::<i32>().ok().or_else(|| w.buildings.iter().find(|b| b.kind == v || b.subtype == v).map(|b| b.id));
                    if let Some(b) = id.and_then(|id| w.building(id)) {
                        let c = b.center;
                        self.selected = Selected::Building(b.id);
                        self.cam.x = c.x as f32 + 0.5;
                        self.cam.y = c.y as f32 + 0.5;
                        self.cam.tx = self.cam.x;
                        self.cam.ty = self.cam.y;
                        self.cam.z = c.z as i32;
                    }
                }
                "addjob" => self.bui.add_open = true,
                "crop" => self.bui.crop_season = Some(v.parse::<usize>().unwrap_or(0).min(3)),
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
                "thoughts" => self.unit_tab = 4,
                "needs" => self.unit_tab = 5,
                "personality" => self.unit_tab = 6,
                "health" => self.unit_tab = 7,
                "relations" => self.unit_tab = 8,
                "sort" => {
                    self.citizens_sort = match v {
                        "profession" => CitSort::Profession,
                        "stress" => CitSort::Stress,
                        _ => CitSort::Name,
                    }
                }
                "squads" | "military" => {
                    self.screen = Screen::Military;
                    let w = &self.sim.world;
                    self.mil.squad = v.parse::<i32>().ok().filter(|id| w.squads.iter().any(|s| s.id == *id)).or_else(|| w.squads.first().map(|s| s.id));
                }
                "assign" => {
                    // Citizen picker for squad position `v` (screenshot tests).
                    self.screen = Screen::Military;
                    if self.mil.squad.is_none() {
                        self.mil.squad = self.sim.world.squads.first().map(|s| s.id);
                    }
                    self.mil.picker = Some(v.parse::<usize>().unwrap_or(0));
                }
                "killlist" => {
                    self.screen = Screen::Military;
                    if self.mil.squad.is_none() {
                        self.mil.squad = self.sim.world.squads.first().map(|s| s.id);
                    }
                    self.mil.kill_list = true;
                }
                "station" => {
                    if let Some(sq) = self.mil.squad.or_else(|| self.sim.world.squads.first().map(|s| s.id)) {
                        self.tool = Tool::SquadStation(sq);
                        self.screen = Screen::None;
                    }
                }
                "burrows" => {
                    self.open_burrows();
                    if let Ok(id) = v.parse::<i32>() {
                        self.burrow_ui.selected = Some(id);
                    }
                }
                "burrow-tool" => {
                    if self.submenu != Submenu::Burrow {
                        self.open_burrows();
                    }
                    self.tool = Tool::Burrow;
                }
                "reports" => {
                    self.screen = Screen::Reports;
                    self.reports.filter = v.parse::<usize>().unwrap_or(0);
                }
                "spawn-hostile" => crate::ui::alerts::debug_spawn_hostile(self),
                // [round 4] screens for screenshots/tests
                "trade" => crate::ui::round4::open_trade(self),
                "fake-caravan" => crate::ui::round4::debug_caravan(self),
                "depot" => {
                    crate::ui::round4::open_trade(self);
                    self.r4.depot_open = true;
                    self.r4.depot_filter = v.to_string();
                }
                "nobles" => self.screen = Screen::Nobles,
                "appoint" => {
                    self.screen = Screen::Nobles;
                    let tok = if v.is_empty() { self.sim.world.nobles.iter().find(|n| crate::ui::nobles::filled_by(&n.token) == crate::ui::nobles::Filled::ByFort).map(|n| n.token.clone()) } else { Some(v.to_string()) };
                    self.r4.appoint = tok;
                }
                "settings" => {
                    self.menu_open = true;
                    self.menu.page = MenuPage::Settings;
                }
                "link" => {
                    if let Selected::Building(id) = self.selected {
                        self.tool = Tool::Link(id);
                    }
                }
                "caravan-popup" => {
                    self.r4.popup = Some(crate::ui::round4::Popup { title: "Caravan".into(), text: "A caravan from the mountainhomes has arrived.".into(), trade: true, pos: None });
                }
                // [round 5] tools and tabs for screenshots/tests
                "construct" => {
                    self.open_build();
                    let cats = crate::ui::build::menu_categories(&self.build_opts);
                    self.build_cat = cats.iter().position(|c| *c == crate::ui::round5::CONSTR_CAT).unwrap_or(0);
                    if let Some(c) = crate::ui::round5::CONSTRUCTIONS.iter().find(|c| format!("{:?}", c.kind).eq_ignore_ascii_case(v) || c.label.eq_ignore_ascii_case(v)) {
                        self.tool = Tool::Construct(c.kind);
                    }
                }
                "smooth" => {
                    crate::ui::round5::open_smooth(self);
                    if let Some(e) = crate::ui::round5::DETAIL_MENU.iter().find(|e| format!("{:?}", e.kind).eq_ignore_ascii_case(v)) {
                        self.tool = Tool::Detail(e.kind);
                    }
                }
                "traffic" => {
                    crate::ui::round5::open_traffic(self);
                    if let Some(e) = crate::ui::round5::TRAFFIC_MENU.iter().find(|e| format!("{:?}", e.kind).eq_ignore_ascii_case(v)) {
                        self.tool = Tool::Traffic(e.kind);
                    }
                }
                "items-tool" => {
                    crate::ui::round5::open_items(self);
                    if let Some(e) = crate::ui::round5::ITEM_MENU.iter().find(|e| format!("{:?}", e.kind).eq_ignore_ascii_case(v)) {
                        self.tool = Tool::ItemDes(e.kind);
                    }
                }
                "mood" => self.unit_tab = 9,
                "fake-constructions" => crate::ui::round5::debug_constructions(self),
                "fake-mood" => {
                    // Test hook: the selected unit is taken by a mood (`Unit::mood`).
                    if let Selected::Unit(id) = self.selected {
                        if let Some(u) = self.sim.world.units.iter_mut().find(|u| u.id == id) {
                            u.mood = if v.is_empty() { "Fey".into() } else { v.to_string() };
                        }
                    }
                }
                // [round 6] help overlay and first-time hints (`hints:3` starts at the third)
                "help" => self.help_open = true,
                "hints" => {
                    self.hints_enabled = true;
                    self.settings.hints_off = false;
                    let n = v.parse::<usize>().unwrap_or(1).max(1) - 1;
                    self.settings.hints_seen = crate::ui::help::HINTS.iter().take(n).map(|h| h.to_string()).collect();
                }
                "tile" => {
                    let c = self.cam;
                    self.selected = Selected::Tile(Pos::new(c.x as i16, c.y as i16, c.z as i16));
                }
                "drag" => {
                    // Pending rectangle from the camera center to the mouse.
                    let c = self.cam;
                    self.drag = Some(Drag { a: Pos::new(c.x as i16 - 3, c.y as i16 - 2, c.z as i16), held: false });
                }
                // [app8] work details, kitchen, stockpile settings, animals, places, ...
                _ if crate::ui::round8::ui_arg(self, k, v) => {}
                _ if crate::ui::round9::ui_arg(self, k, v) => {} // [app9]
                _ => eprintln!("unknown --ui state {u}"),
            }
        }
    }

    pub fn mouse(&self) -> Vec2 {
        self.input.mouse
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

    /// [round 6] Write the settings file (not in screenshot runs).
    pub fn persist_settings(&mut self) {
        if !self.settings_writable {
            return;
        }
        if let Err(e) = self.settings.save() {
            self.set_status(format!("Settings not saved: {e}"));
        }
    }

    /// [round 6] Change the autosave interval (in-game settings page); applies right away.
    pub fn set_autosave_days(&mut self, days: u32) {
        self.settings.autosave_days = days;
        self.autosave_days = days;
        self.autosave_next = next_autosave_day(game_day(&self.sim.world.calendar), days);
        self.persist_settings();
    }

    /// [round 6] Autosave when the game reaches the next autosave day (DF: seasonal autosave).
    fn check_autosave(&mut self) {
        let Some(next) = self.autosave_next else { return };
        let day = game_day(&self.sim.world.calendar);
        if day < next || self.saving.is_some() {
            return;
        }
        self.autosave_next = next_autosave_day(day, self.autosave_days);
        let w = self.sim.world_for_save(); // [saves10] world + module state
        let seed = self.seed;
        let keep = self.settings.autosave_keep.max(1) as usize;
        self.set_status("Autosaving...".into());
        self.saving_auto = true;
        self.saving = Some(std::thread::spawn(move || {
            let dir = crate::save::save_dir();
            let r = crate::save::save_world_tagged(&w, seed, &dir, crate::save::AUTO_TAG);
            if r.is_ok() {
                crate::save::rotate_autosaves(&dir, &w.meta.fort_name, keep);
            }
            r
        }));
    }

    // ---------------------------------------------------------------- simulation

    /// Ticks run this frame (the game is not paused by the player, a menu or a pop-up).
    pub fn sim_running(&self) -> bool {
        !self.paused && !self.menu_open && self.alert.is_none() && self.r4.popup.is_none() && self.r9.popups.is_empty() // [app9]
    }

    /// [round 4] Called right after the sim came back from the worker: book the ticks it ran.
    pub fn after_acquire(&mut self) {
        if !self.sim.threaded() {
            return;
        }
        let ran = self.sim.ticks().saturating_sub(self.released_at_tick);
        self.tick_accum = (self.tick_accum - ran as f64).max(0.0);
        let busy = self.sim.busy_secs();
        self.sim_ms = (busy - self.busy_prev) * 1000.0;
        self.busy_prev = busy;
    }

    /// [round 4] End of the frame: hand the sim to the worker with this frame's allowance.
    pub fn release_sim(&mut self, dt: f32) {
        if !self.sim.threaded() {
            return;
        }
        let allow = if self.sim_running() {
            match self.speed.tps() {
                Some(tps) => {
                    self.tick_accum = (self.tick_accum + dt as f64 * tps).min(tps * 0.25);
                    self.tick_accum.floor() as u64
                }
                None => {
                    self.tick_accum = 0.0;
                    u64::MAX
                }
            }
        } else {
            self.tick_accum = 0.0;
            0
        };
        self.released_at_tick = self.sim.ticks();
        self.sim.release(allow);
    }

    fn update_sim(&mut self, dt: f32) {
        let t0 = Instant::now();
        if !self.sim.threaded() && self.sim_running() {
            match self.speed.tps() {
                Some(tps) => {
                    self.tick_accum = (self.tick_accum + dt as f64 * tps).min(tps * 0.25);
                    let mut todo = self.tick_accum.floor() as u32;
                    while todo > 0 && t0.elapsed().as_secs_f64() < self.sim_budget {
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
                    while t0.elapsed().as_secs_f64() < self.sim_budget {
                        let n = self.chunk.max(1);
                        let c0 = Instant::now();
                        self.sim.run_ticks(n);
                        self.adapt_chunk(n, c0.elapsed().as_secs_f64());
                    }
                }
            }
        }
        if !self.sim.threaded() {
            self.sim_ms = t0.elapsed().as_secs_f64() * 1000.0;
        }
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
        if now - self.prepared_at > 1.0 {
            self.prepared_at = now;
            self.gfx.prepare(&self.sim.world);
        }
        if now - self.stocks.at > 1.0 {
            self.stocks = crate::ui::topbar::count_stocks(&self.sim.world, now);
        }
        self.check_autosave(); // [round 6]
        self.poll_save();
        let date = date_string(&self.sim.world.calendar);
        let fresh = self.sim.drain_announcements();
        crate::ui::round4::on_announcements(self, &fresh); // [round 4]
        let show = crate::ui::round9::on_announcements(self, &fresh); // [app9] boxes, pause, recenter
        for (a, sh) in fresh.into_iter().zip(show) {
            let mut l = LogLine::new(a.text, a.pos, a.kind, now, date.clone());
            l.quiet = !sh;
            self.log.push(l);
        }
        // [round 3] The Reports screen keeps a long history.
        if self.log.len() > LOG_KEEP {
            self.log.drain(..self.log.len() - LOG_KEEP);
        }
        self.update_hostiles();
        crate::ui::round9::update(self); // [app9] raid boxes, petitions, combat reports
        crate::ui::round4::update(self); // [round 4] caravan arrival
    }

    /// Keep each `run_ticks` call around 1 ms so the frame budget is respected.
    fn adapt_chunk(&mut self, n: u32, secs: f64) {
        let per_tick = secs / n.max(1) as f64;
        let want = if per_tick > 0.0 { (0.001 / per_tick) as u32 } else { 10_000 };
        self.chunk = want.clamp(1, 10_000);
    }

    // ---------------------------------------------------------------- input

    fn global_keys(&mut self) {
        if self.input.hit(&self.binds, "RF_QUICKSAVE") && self.saving.is_none() {
            self.start_save();
        }
        // [round 6] F1 / '?' toggles the controls overview; while it is open only Esc counts.
        if !self.typing() && self.input.hit(&self.binds, "RF_HELP") {
            self.help_open = !self.help_open;
            return;
        }
        if self.help_open {
            if self.input.hit(&self.binds, "LEAVESCREEN") {
                self.help_open = false;
            }
            return;
        }
        let b = &self.binds;
        let inp = &self.input;
        if inp.hit(b, "LEAVESCREEN") {
            self.escape();
            return;
        }
        // [round 3] Enter closes a pop-up alert, like DF.
        if self.alert.is_some() && inp.fired.contains(&KeyCode::Enter) {
            self.alert = None;
            return;
        }
        if self.r4.popup.is_some() && inp.fired.contains(&KeyCode::Enter) {
            self.r4.popup = None;
            return;
        }
        if crate::ui::round9::enter_key(self) {
            return; // [app9] boxes, patrol route
        }
        let (b, inp) = (&self.binds, &self.input);
        if self.menu_open || self.typing() {
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
        // [round 5] Inside the smooth/traffic/item menus their DESIGNATE_* keys pick the tool.
        if crate::ui::round5::submenu_keys(self) {
            return;
        }
        let b = &self.binds;
        let inp = &self.input;
        if inp.hit(b, "D_DESIGNATE_SMOOTH") {
            crate::ui::round5::open_smooth(self);
        } else if inp.hit(b, "D_DESIGNATE_TRAFFIC") {
            crate::ui::round5::open_traffic(self);
        } else if inp.hit(b, "D_DESIGNATE_ITEMS") {
            crate::ui::round5::open_items(self);
        } else if inp.hit(b, "D_DESIGNATE_DIG") {
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
        } else if inp.hit(b, "D_BUILDING") {
            self.open_build();
        } else if inp.hit(b, "D_CIVZONE") {
            self.open_zones();
        } else if inp.hit(b, "D_ORDERS") {
            self.toggle_screen(Screen::WorkOrders);
        } else if inp.hit(b, "D_LABOR") {
            self.toggle_screen(Screen::Labor);
        } else if inp.hit(b, "D_SQUADS") {
            self.toggle_screen(Screen::Military);
        } else if inp.hit(b, "D_BURROWS") {
            self.open_burrows();
        } else if inp.hit(b, "D_ANNOUNCE") {
            self.toggle_screen(Screen::Reports);
        } else if inp.hit(b, "D_NOBLES") {
            self.toggle_screen(Screen::Nobles); // [round 4]
        } else {
            crate::ui::round8::global_keys(self); // [app8]
        }
    }

    /// Select a unit and show its sheet (tab kept, scroll reset).
    pub fn select_unit(&mut self, id: i32) {
        if self.selected != Selected::Unit(id) {
            self.unit_scroll = 0;
        }
        self.selected = Selected::Unit(id);
    }

    /// A text field has keyboard focus: hotkeys and WASD are off.
    pub fn typing(&self) -> bool {
        (self.screen == Screen::Labor && self.labor.typing)
            || (self.screen == Screen::Trade && self.r4.depot_open && self.r4.depot_typing)
            || crate::ui::round8::typing(self) // [app8]
    }

    pub fn toggle_screen(&mut self, s: Screen) {
        self.screen = if self.screen == s { Screen::None } else { s };
        self.labor.typing = false;
        self.mil.picker = None;
        self.mil.kill_list = false;
        if self.screen == Screen::Military && self.mil.squad.is_none() {
            self.mil.squad = self.sim.world.squads.first().map(|s| s.id);
        }
        if self.screen != Screen::None {
            self.tool = Tool::None;
            self.submenu = Submenu::None;
            self.drag = None;
        }
    }

    pub fn open_build(&mut self) {
        if self.submenu == Submenu::Build {
            self.submenu = Submenu::None;
            self.tool = Tool::None;
        } else {
            self.build_opts = self.sim.build_options();
            self.submenu = Submenu::Build;
            self.tool = Tool::None;
            self.screen = Screen::None;
        }
        self.build_check = None;
        self.drag = None;
    }

    pub fn open_burrows(&mut self) {
        if self.submenu == Submenu::Burrow {
            self.submenu = Submenu::None;
            self.tool = Tool::None;
        } else {
            self.submenu = Submenu::Burrow;
            self.tool = Tool::None;
            self.screen = Screen::None;
            if self.burrow_ui.selected.is_none() {
                self.burrow_ui.selected = self.sim.world.burrows.first().map(|b| b.id);
            }
        }
        self.drag = None;
    }

    pub fn open_zones(&mut self) {
        if self.submenu == Submenu::Zone {
            self.submenu = Submenu::None;
            self.tool = Tool::None;
        } else {
            self.submenu = Submenu::Zone;
            self.tool = Tool::None;
            self.screen = Screen::None;
        }
        self.drag = None;
    }

    pub fn open_load_menu(&mut self) {
        self.menu_open = true;
        self.menu.page = MenuPage::Load;
        self.menu.scroll = 0;
        self.menu.saves = crate::save::list_saves(&crate::save::save_dir());
    }

    /// Save in a background thread (the world is cloned first, so play continues).
    pub fn start_save(&mut self) {
        if self.saving.is_some() {
            return;
        }
        let w = self.sim.world_for_save(); // [saves10] world + module state
        let seed = self.seed;
        self.set_status("Saving...".into());
        self.saving = Some(std::thread::spawn(move || crate::save::save_world(&w, seed, &crate::save::save_dir())));
    }

    /// Block until a running save is done (screenshot runs, quitting).
    pub fn finish_save(&mut self) {
        if let Some(h) = self.saving.as_ref() {
            while !h.is_finished() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        self.poll_save();
    }

    fn poll_save(&mut self) {
        if !self.saving.as_ref().map(|h| h.is_finished()).unwrap_or(false) {
            return;
        }
        let res = self.saving.take().unwrap().join().unwrap_or_else(|_| Err("save thread panicked".into()));
        let now = get_time();
        let what = if std::mem::take(&mut self.saving_auto) { "Autosaved" } else { "Saved" };
        let (text, kind) = match res {
            Ok(i) => (
                format!("{what} {} ({}, {:.2}s)", i.path.display(), crate::save::fmt_bytes(i.bytes), i.secs),
                AnnouncementKind::Info,
            ),
            Err(e) => (format!("Save failed: {e}"), AnnouncementKind::Warning),
        };
        self.set_status(text.clone());
        self.log.push(LogLine::new(text, None, kind, now, date_string(&self.sim.world.calendar)));
        if self.menu_open && self.menu.page == MenuPage::Load {
            self.menu.saves = crate::save::list_saves(&crate::save::save_dir());
        }
    }

    /// Replace the running game with a save. The simulation state is rebuilt.
    pub fn load_save(&mut self, path: &std::path::Path) {
        let now = get_time();
        match crate::save::load_world(path) {
            Ok((world, info)) => {
                let t = Instant::now();
                let same_fort = world.meta.fort_name == self.sim.world.meta.fort_name && world.dims == self.sim.world.dims;
                self.gfx.prepare(&world);
                self.zclass = compute_zclass(&world);
                if !same_fort {
                    self.cam = initial_camera(&world);
                }
                let seed = if info.header.seed != 0 { info.header.seed } else { self.seed };
                self.seed = seed;
                *self.sim = Sim::new(world, seed);
                self.r4 = Default::default();
                self.r5 = Default::default();
                let minimap = self.r8.minimap;
                self.r8 = crate::ui::round8::R8Ui { minimap, ..Default::default() }; // [app8]
                self.r9 = Default::default(); // [app9]
                let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                self.label = format!("{} - loaded {file}", self.sim.world.meta.fort_name);
                self.selected = Selected::None;
                self.tool = Tool::None;
                self.submenu = Submenu::None;
                self.screen = Screen::None;
                self.drag = None;
                self.follow = None;
                self.paused = true;
                self.menu_open = false;
                self.build_opts = self.sim.build_options();
                self.build_check = None;
                self.stocks = crate::ui::topbar::count_stocks(&self.sim.world, now);
                self.reset_hostiles();
                self.alert = None;
                self.civ_alert = None;
                self.mil = MilitaryUi::default();
                self.burrow_ui = BurrowUi::default();
                self.help_open = false;
                self.autosave_next = next_autosave_day(game_day(&self.sim.world.calendar), self.autosave_days); // [round 6]
                // [saves10] The tick count comes back from the save: restart the tick-rate window.
                self.tps_hist.clear();
                self.released_at_tick = self.sim.ticks();
                let mut text = format!(
                    "Loaded {} ({}): {} in {:.2}s + sim {:.2}s",
                    info.header.fort,
                    info.header.date,
                    crate::save::fmt_bytes(info.bytes),
                    info.secs,
                    t.elapsed().as_secs_f64()
                );
                if let Some(n) = &info.note {
                    text.push_str(&format!(" - {n}")); // [saves10]
                }
                self.set_status(text.clone());
                self.log.push(LogLine::new(text, None, AnnouncementKind::Info, now, date_string(&self.sim.world.calendar)));
            }
            Err(e) => {
                let text = format!("Load failed: {e}");
                self.set_status(text.clone());
                self.log.push(LogLine::new(text, None, AnnouncementKind::Warning, now, date_string(&self.sim.world.calendar)));
            }
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
        if self.alert.is_some() {
            self.alert = None;
        } else if self.r4.popup.is_some() {
            self.r4.popup = None;
        } else if crate::ui::round9::escape(self) {
            // [app9] boxes, patrol tool, announcement settings
        } else if self.screen == Screen::Trade && self.r4.depot_open {
            self.r4.depot_open = false;
            self.r4.depot_typing = false;
        } else if self.screen == Screen::Nobles && self.r4.appoint.is_some() {
            self.r4.appoint = None;
        } else if crate::ui::round8::escape(self) {
            // [app8] pickers, name field, stockpile link tool
        } else if matches!(self.tool, Tool::Link(_)) {
            self.tool = Tool::None;
        } else if self.menu_open {
            if self.menu.page != MenuPage::Main {
                self.menu.page = MenuPage::Main;
            } else {
                self.menu_open = false;
            }
        } else if self.labor.typing {
            self.labor.typing = false;
        } else if self.bui.add_open || self.bui.crop_season.is_some() {
            self.bui.add_open = false;
            self.bui.crop_season = None;
        } else if self.orders.new_open {
            self.orders.new_open = false;
        } else if self.screen == Screen::Military && (self.mil.picker.is_some() || self.mil.kill_list) {
            self.mil.picker = None;
            self.mil.kill_list = false;
        } else if matches!(self.tool, Tool::SquadStation(_) | Tool::SquadKill(_)) {
            // Back to the squads screen, like DF.
            self.tool = Tool::None;
            self.screen = Screen::Military;
        } else if self.screen != Screen::None {
            self.screen = Screen::None;
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
            self.menu.page = MenuPage::Main;
        }
    }

    fn update_camera(&mut self, dt: f32) {
        let dims = self.sim.world.dims;
        if let Some(id) = self.follow {
            match self.sim.world.unit(id).map(|u| u.pos) {
                Some(p) if self.selected == Selected::Unit(id) => self.cam.focus(p),
                _ => self.follow = None,
            }
        }
        if !self.menu_open && !self.typing() {
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
        if self.input.mpressed && !self.ui.over_ui {
            self.pan_drag = Some((m, self.cam.x, self.cam.y));
        }
        if let Some((start, cx, cy)) = self.pan_drag {
            if self.input.mdown {
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
        self.hover = if self.ui.over_ui || self.menu_open || self.screen != Screen::None { None } else { self.tile_at(m) };
        if self.menu_open || self.screen != Screen::None {
            return;
        }
        let lp = self.ui.lpressed && !self.ui.click_used && !self.ui.over_ui;
        let lr = self.input.lreleased;
        let rp = self.ui.rpressed && !self.ui.over_ui;
        if rp {
            // Right click backs out: pending rectangle, then tool, then selection.
            if self.drag.is_some() {
                self.drag = None;
            } else if matches!(self.tool, Tool::Build(_) | Tool::Zone(_) | Tool::Burrow | Tool::Construct(_)) {
                // Back to the build/zone/burrow menu, like DF.
                self.tool = Tool::None;
            } else if matches!(self.tool, Tool::SquadStation(_) | Tool::SquadKill(_)) {
                self.tool = Tool::None;
                self.screen = Screen::Military;
            } else if matches!(self.tool, Tool::Link(_)) {
                self.tool = Tool::None;
            } else if let Tool::PileLink(id, _) = self.tool {
                crate::ui::round8::open_pile(self, id); // [app8] back to the settings
            } else if let Tool::Patrol(_) = self.tool {
                crate::ui::round9::patrol_back(self); // [app9] remove the last waypoint
            } else if self.tool != Tool::None || self.submenu != Submenu::None {
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
            Tool::Designate(_) | Tool::Stockpile(_) | Tool::Zone(_) | Tool::Burrow | Tool::Detail(_) | Tool::Traffic(_) | Tool::ItemDes(_) => {
                let (d, done) = drag_step(self.drag, h, lp, lr);
                self.drag = d;
                if let Some((a, b)) = done {
                    self.apply_tool(a, b);
                }
            }
            Tool::Construct(_) => {
                // [round 5] Constructions stay on the level of the first corner, like DF.
                let h2 = match self.drag {
                    Some(d) => Pos::new(h.x, h.y, d.a.z),
                    None => h,
                };
                let (d, done) = drag_step(self.drag, h2, lp, lr);
                self.drag = d;
                if let Some((a, b)) = done {
                    self.apply_tool(a, b);
                }
            }
            Tool::Build(i) => {
                let dragged = self.build_opts.get(i).map(|o| o.dragged).unwrap_or(false);
                if dragged {
                    let (d, done) = drag_step(self.drag, h, lp, lr);
                    self.drag = d;
                    if let Some((a, b)) = done {
                        self.apply_tool(a, b);
                    }
                } else if lp {
                    self.apply_tool(h, h);
                }
            }
            Tool::SquadStation(sq) => {
                if lp {
                    self.sim.order_squad(sq, rf_sim::SquadOrder::Station(h));
                    let name = crate::ui::military::squad_name(&self.sim.world, sq);
                    self.set_status(format!("{name}: station at {}, {}, z {}", h.x, h.y, h.z));
                    self.tool = Tool::None;
                    self.screen = Screen::Military;
                }
            }
            Tool::Link(lever) => {
                if lp {
                    crate::ui::devices::link_click(self, lever, h);
                }
            }
            Tool::PileLink(pile, give) => {
                if lp {
                    crate::ui::round8::pile_link_click(self, pile, give, h); // [app8]
                }
            }
            Tool::Patrol(sq) => {
                if lp {
                    crate::ui::round9::patrol_click(self, sq, h); // [app9]
                }
            }
            Tool::SquadKill(sq) => {
                if lp {
                    match crate::ui::military::kill_target_at(self, h) {
                        Some(uid) => {
                            self.sim.order_squad(sq, rf_sim::SquadOrder::Kill(uid));
                            let name = crate::ui::military::squad_name(&self.sim.world, sq);
                            let who = self.sim.world.unit(uid).map(crate::ui::panels::unit_name).unwrap_or_default();
                            self.set_status(format!("{name}: kill {who}"));
                            self.tool = Tool::None;
                            self.screen = Screen::Military;
                        }
                        None => self.set_status("No creature to attack there (citizens cannot be targeted)".into()),
                    }
                }
            }
        }
    }

    fn select_at(&mut self, p: Pos) {
        let w = &self.sim.world;
        let zone_mode = self.submenu == Submenu::Zone;
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
            self.select_unit(next);
        } else if let Some(id) = building_at(w, p, zone_mode) {
            self.selected = Selected::Building(id);
            self.bui = BuildingUi { repeat: self.bui.repeat, ..Default::default() };
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
            Tool::Zone(k) => {
                let kind = rf_sim::ZONE_KINDS[k.min(rf_sim::ZONE_KINDS.len() - 1)];
                let a2 = Pos::new(a.x, a.y, b.z);
                let id = self.sim.create_zone(a2, b, kind);
                self.set_status(format!("{} zone #{id}: {dx}x{dy}", crate::ui::words(kind)));
                self.tool = Tool::None;
                self.selected = Selected::Building(id);
            }
            Tool::Burrow => {
                let n = self.sim.world.burrows.len() + 1;
                let id = self.sim.create_burrow(&format!("Burrow {n}"), a, b);
                self.set_status(format!("Burrow {n} (#{id}): {dx}x{dy}x{dz} = {} tiles", dx as i32 * dy as i32 * dz as i32));
                self.burrow_ui.selected = Some(id);
                self.tool = Tool::None;
            }
            Tool::SquadStation(_) | Tool::SquadKill(_) | Tool::Link(_) | Tool::PileLink(..) | Tool::Patrol(_) => {}
            Tool::Construct(_) | Tool::Detail(_) | Tool::Traffic(_) | Tool::ItemDes(_) => {
                crate::ui::round5::apply(self, a, b);
            }
            Tool::Build(i) => {
                let Some(o) = self.build_opts.get(i).cloned() else { return };
                let (lo, hi) = crate::ui::build::footprint(&o, a, Pos::new(b.x, b.y, a.z));
                match self.sim.place_building(i, lo, hi) {
                    Ok(id) => self.set_status(format!("{} placed (#{id}), needs {}", o.label, o.needs)),
                    Err(e) => self.set_status(format!("Cannot place {}: {e}", o.label)),
                }
                self.build_check = None;
            }
            Tool::None => {}
        }
    }
}

/// Rectangle tool state machine (Steam style): press-drag-release, or click + click. The
/// z-level may change between the corners (3D boxes). Returns the new state and a finished
/// rectangle, if any.
pub fn drag_step(drag: Option<Drag>, hover: Pos, lpressed: bool, lreleased: bool) -> (Option<Drag>, Option<(Pos, Pos)>) {
    match drag {
        None if lpressed => (Some(Drag { a: hover, held: true }), None),
        Some(d) if lpressed => (None, Some((d.a, hover))),
        Some(d) if lreleased && d.held => {
            if d.a != hover {
                (None, Some((d.a, hover)))
            } else {
                // A plain click: wait for the second corner.
                (Some(Drag { a: d.a, held: false }), None)
            }
        }
        other => (other, None),
    }
}

/// The building a click on `p` selects: furniture/workshops first, then stockpiles; zones
/// only in zone mode (DF hides zones otherwise).
pub fn building_at(w: &World, p: Pos, zones: bool) -> Option<i32> {
    let rank = |k: &str| match k {
        "Civzone" => 2,
        "Stockpile" => 1,
        _ => 0,
    };
    w.buildings
        .iter()
        .filter(|b| b.contains(p) && (zones || b.kind != "Civzone"))
        .min_by_key(|b| (rank(&b.kind), (b.x2 - b.x1 + 1) as i32 * (b.y2 - b.y1 + 1) as i32))
        .map(|b| b.id)
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

/// The game loop. [round 6] Returns how it ended and the (possibly changed) settings;
/// `start_frame` continues the frame count of the title screen (scripted screenshot runs).
pub async fn run(args: Args, loaded: Loaded, settings: crate::settings::Settings, start_frame: u32) -> (Exit, crate::settings::Settings, u32) {
    let mut g = Game::new(&args, loaded, settings);
    let mut frame: u32 = start_frame;
    let mut deadline = Instant::now();
    // [round 4] Average speed since the first frame (screenshot runs report it).
    let mut started: Option<(Instant, u64, f64)> = None;
    let measure: Option<(u64, u64)> = std::env::var("RUSTFORT_MEASURE").ok().and_then(|v| {
        let p: Vec<u64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (p.len() == 2 && p[1] > p[0]).then(|| (p[0], p[1]))
    });
    let mut window: (Option<(Instant, u64)>, Option<f64>) = (None, None);
    // Scripted input (screenshot tests); `--mouse` alone is a script that only parks the mouse.
    let mut script = match (&args.script, args.mouse) {
        (Some(s), m) => match crate::script::Script::parse(s, m) {
            Ok(sc) => Some(sc),
            Err(e) => {
                eprintln!("{e}");
                return (Exit::Quit, g.settings.clone(), frame);
            }
        },
        (None, Some(m)) => crate::script::Script::parse("", Some(m)).ok(),
        (None, None) => None,
    };
    let exit = loop {
        // [round 4] Take the sim back from the worker thread for this frame.
        g.sim.acquire();
        g.after_acquire();
        if started.is_none() && frame == start_frame + 2 {
            started = Some((Instant::now(), g.sim.ticks(), g.sim.busy_secs()));
        }
        // `RUSTFORT_MEASURE=a,b`: real time from sim tick a to tick b (same game span in any
        // mode), printed once and at the screenshot.
        if let Some((a, b)) = measure {
            let t = g.sim.ticks();
            if window.0.is_none() && t >= a {
                window.0 = Some((Instant::now(), t));
            }
            if let (Some((t0, k0)), None) = (window.0, window.1) {
                if t >= b {
                    let secs = t0.elapsed().as_secs_f64();
                    window.1 = Some((t - k0) as f64 / secs.max(1e-6));
                    eprintln!("measure: ticks {k0}..{t} in {secs:.2}s = {:.0} ticks/s ({})", window.1.unwrap_or(0.0), if g.sim.threaded() { "sim thread" } else { "inline" });
                }
            }
        }
        let f0 = Instant::now();
        let dt = get_frame_time().min(0.1);
        g.fps = g.fps * 0.95 + (1.0 / dt.max(1e-4)) * 0.05;
        g.input = g.input_state.collect(&g.binds);
        if let Some(sc) = script.as_mut() {
            sc.apply(frame + 1, &mut g.input);
        }
        // The wheel over a UI panel scrolls the panel, not the z-level (state of last frame).
        if g.ui.over_ui {
            g.input.wheel_used = true;
        }
        g.global_keys();
        if args.screenshot.is_some() {
            // Screenshot runs are reproducible: saves finish within the frame.
            g.finish_save();
        }
        g.update_sim(dt);
        g.update_camera(dt);

        clear_background(Color::from_rgba(0, 0, 0, 255));
        crate::render::draw_map(&mut g);
        let scale = g.ui_scale;
        g.ui.begin(&g.input, scale);
        crate::ui::overlay::draw_ui(&mut g);
        g.map_input();
        g.frame_ms = f0.elapsed().as_secs_f64() * 1000.0;

        frame += 1;
        if let Some(path) = &args.screenshot {
            if args.shots_at.contains(&frame) {
                let p = extra_shot_path(path, frame);
                save_screenshot(&p);
                println!("saved {} (sim tick {})", p.display(), g.sim.ticks());
            }
            if frame >= args.frames.max(1) {
                let (iw, ih) = save_screenshot(path);
                println!(
                    "saved {} ({}x{}) after {frame} frames, sim tick {}, {:.0} fps, frame {:.2} ms (cpu), {:.0} ticks/s, avg {:.0} ticks/s ({})",
                    path.display(),
                    iw,
                    ih,
                    g.sim.ticks(),
                    g.fps,
                    g.frame_ms,
                    g.tps,
                    started.map(|(t, k, _)| (g.sim.ticks() - k) as f64 / t.elapsed().as_secs_f64().max(1e-3)).unwrap_or(0.0),
                    if g.sim.threaded() {
                        let (el, b0) = started.map(|(t, _, b)| (t.elapsed().as_secs_f64(), b)).unwrap_or((1.0, 0.0));
                        format!("sim thread, worker busy {:.0}%", 100.0 * (g.sim.busy_secs() - b0) / el.max(1e-3))
                    } else {
                        "inline".to_string()
                    }
                );
                break Exit::Quit;
            }
        }
        if g.quit {
            g.finish_save();
            break Exit::Quit;
        }
        if let Some(e) = g.exit {
            g.finish_save();
            break e;
        }
        // [round 4] The worker simulates while the GPU flushes and we wait for vsync.
        g.release_sim(dt);
        if !args.vsync && args.gfps > 0 {
            // Own frame pacing (DF's G_FPS_CAP) when vsync is off.
            let period = std::time::Duration::from_secs_f64(1.0 / args.gfps as f64);
            deadline += period;
            let now = Instant::now();
            if deadline > now {
                std::thread::sleep(deadline - now);
            } else if now - deadline > period {
                deadline = now;
            }
        }
        next_frame().await;
    };
    let settings = g.settings.clone();
    (exit, settings, frame)
}

/// [round 6] `shots/a.png` + frame 300 -> `shots/a-f300.png`.
pub fn extra_shot_path(path: &std::path::Path, frame: u32) -> std::path::PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "shot".into());
    path.with_file_name(format!("{stem}-f{frame}.png"))
}

/// Write the current frame to a PNG (alpha forced to opaque). Returns its size.
pub fn save_screenshot(path: &std::path::Path) -> (u16, u16) {
    let mut img = get_screen_data();
    // [round 4] The window ignores framebuffer alpha; so does the screenshot
    // (translucent UI layers would otherwise look washed out in image viewers).
    for px in img.bytes.chunks_exact_mut(4) {
        px[3] = 255;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    img.export_png(&path.to_string_lossy());
    (img.width, img.height)
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
    fn rectangle_tool_drag_and_click_click() {
        let a = Pos::new(1, 1, 5);
        let b = Pos::new(4, 3, 3);
        // Press, move (z changes), release -> done.
        let (d, done) = drag_step(None, a, true, false);
        assert!(done.is_none() && d.unwrap().held);
        let (d, done) = drag_step(d, b, false, false);
        assert!(done.is_none());
        let (d, done) = drag_step(d, b, false, true);
        assert!(d.is_none());
        assert_eq!(done, Some((a, b)));
        // Click, release on the same tile, click elsewhere -> done.
        let (d, _) = drag_step(None, a, true, false);
        let (d, done) = drag_step(d, a, false, true);
        assert!(done.is_none() && !d.unwrap().held);
        let (d, done) = drag_step(d, b, true, false);
        assert!(d.is_none());
        assert_eq!(done, Some((a, b)));
    }

    #[test]
    fn autosave_days_follow_the_seasons() {
        let mut c = rf_core::Calendar { year: 277, year_tick: 0 };
        assert_eq!(game_day(&c), 277 * 336);
        // Seasonal autosave: the first day of the next season.
        assert_eq!(next_autosave_day(game_day(&c), 84), Some(277 * 336 + 84));
        c.year_tick = rf_core::TICKS_PER_MONTH * 11 + rf_core::TICKS_PER_DAY * 27;
        assert_eq!(next_autosave_day(game_day(&c), 84), Some(278 * 336));
        assert_eq!(next_autosave_day(game_day(&c), 336), Some(278 * 336));
        assert_eq!(next_autosave_day(5, 7), Some(7));
        assert_eq!(next_autosave_day(7, 7), Some(14));
        assert_eq!(next_autosave_day(100, 0), None);
    }

    #[test]
    fn bench_sim_runs_through_the_api() {
        let w = World::synthetic(32, 32, 20);
        let mut s = Sim::new(w, 1);
        s.run_ticks(50);
        assert_eq!(s.ticks(), 50);
    }
}
