//! [round 6] The title screen, like DF's: Continue, Load game, Import from Dwarf Fortress,
//! New synthetic fort (dev), Settings, Quit. It uses DF's own title art from the install
//! (`data/art/title_background.png`, `df_logo.png`) when it is there.
//!
//! `app_main` runs the title screen and the game in turn: "Save and return to title" in the
//! game's Esc menu comes back here. `play --export/--load/--synthetic/--screenshot` start the
//! game directly (no title screen) unless `--menu` is given.

use crate::font::{Font, GH, GW};
use crate::game::Exit;
use crate::importer::{ExportInfo, ExportJob, FortInfo, JobState, Probe};
use crate::keys::{Bindings, InputFrame, InputState};
use crate::load::Loaded;
use crate::save::SaveInfo;
use crate::settings::Settings;
use crate::ui::{checkbox, clip, col, list_row, panel, text_button, wheel_over, BtnState, UiCtx};
use crate::Args;
use macroquad::prelude::*;
use std::path::PathBuf;
use std::thread::JoinHandle;

/// What "Continue" / a list click loads.
#[derive(Clone, Debug)]
pub enum Target {
    Save(SaveInfo),
    Export(PathBuf),
    Synthetic,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Title,
    Load,
    Import,
    Settings,
    Loading,
}

enum ImportState {
    Checking(JoinHandle<Probe>),
    Probed(Probe),
    Starting(JoinHandle<Result<ExportJob, String>>),
    Running(ExportJob),
    Failed(String),
}

struct Loading {
    what: String,
    started: f64,
    handle: JoinHandle<Result<Loaded, String>>,
}

pub struct Menu {
    font: Font,
    ui: UiCtx,
    input_state: InputState,
    input: InputFrame,
    binds: Bindings,
    bg: Option<Texture2D>,
    logo: Option<Texture2D>,
    art_dir: PathBuf,
    pub page: Page,
    saves: Vec<SaveInfo>,
    exports: Vec<ExportInfo>,
    /// Load page tab: 0 saves, 1 DF exports.
    load_tab: usize,
    scroll: usize,
    import: Option<ImportState>,
    loading: Option<Loading>,
    /// Error of the last load attempt (shown on the title screen).
    error: Option<String>,
    continue_target: Option<Target>,
    pub settings: Settings,
    /// The DF folder field has the keyboard.
    editing_dir: bool,
    dir_text: String,
    ui_scale_cli: Option<f32>,
    settings_writable: bool,
    quit: bool,
    /// `--menu-page import:go`: start the import once DF answers (end-to-end test).
    auto_import: bool,
}

fn load_tex(path: &std::path::Path) -> Option<Texture2D> {
    let bytes = std::fs::read(path).ok()?;
    let img = Image::from_file_with_format(&bytes, Some(ImageFormat::Png)).ok()?;
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Linear);
    Some(t)
}

/// Newest save, else the newest complete export, else the default export (`RUSTFORT_EXPORT`,
/// `export/tileshtulon`).
pub fn continue_target() -> Option<Target> {
    if let Some(s) = crate::save::list_saves(&crate::save::save_dir()).into_iter().find(|s| s.header.saved_unix > 0) {
        return Some(Target::Save(s));
    }
    if let Some(e) = crate::importer::list_exports(&crate::importer::export_root()).into_iter().next() {
        return Some(Target::Export(e.dir));
    }
    rf_import::find_export(None).map(Target::Export)
}

/// One line about a target for the title screen.
fn describe(t: &Target) -> String {
    match t {
        Target::Save(s) => {
            let auto = if crate::save::is_autosave(&s.path) { "autosave" } else { "saved" };
            format!("{}, {} ({auto} {})", s.header.fort, s.header.date, crate::save::fmt_unix(s.header.saved_unix).trim_end_matches(" UTC"))
        }
        Target::Export(d) => match rf_import::load_manifest(d) {
            Ok(m) => format!("{}, year {} (DF export {})", m.fort_name, m.year, d.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()),
            Err(_) => format!("DF export {}", d.display()),
        },
        Target::Synthetic => "a synthetic test fort".into(),
    }
}

/// Load a target in a thread (world + graphics). `first_save`: write an autosave right away
/// (fresh imports, so "Continue" finds them).
fn spawn_load(t: Target, size: (u16, u16, u16), seed: u64, first_save: bool) -> JoinHandle<Result<Loaded, String>> {
    std::thread::spawn(move || {
        let mut l = match &t {
            Target::Save(s) => crate::load::load_save_file(&s.path)?,
            Target::Export(d) => {
                let l = crate::load::load_world(Some(d), false, size);
                if l.report.is_none() {
                    return Err(l.warning.unwrap_or_else(|| format!("could not import {}", d.display())));
                }
                l
            }
            Target::Synthetic => crate::load::load_world(None, true, size),
        };
        if first_save {
            if let Err(e) = crate::save::save_world_tagged(&l.world, seed, &crate::save::save_dir(), crate::save::AUTO_TAG) {
                l.warning = Some(format!("first save of the import failed: {e}"));
            }
        }
        crate::load::preload_gfx(&mut l);
        Ok(l)
    })
}

impl Menu {
    pub fn new(args: &Args, settings: Settings) -> Menu {
        let df = rf_gfx::df_dir();
        let art = df.join("data").join("art");
        let dir_text = settings.df_dir.clone();
        let mut m = Menu {
            font: Font::load(&df),
            ui: UiCtx::default(),
            input_state: InputState::default(),
            input: InputFrame::default(),
            binds: Bindings::load(&df),
            bg: load_tex(&art.join("title_background.png")),
            logo: load_tex(&art.join("df_logo.png")),
            art_dir: art,
            page: Page::Title,
            saves: Vec::new(),
            exports: Vec::new(),
            load_tab: 0,
            scroll: 0,
            import: None,
            loading: None,
            error: None,
            continue_target: continue_target(),
            settings,
            editing_dir: false,
            dir_text,
            ui_scale_cli: args.ui_scale_cli.then_some(args.ui_scale),
            settings_writable: args.screenshot.is_none(),
            quit: false,
            auto_import: false,
        };
        match args.menu_page.as_deref() {
            Some("load") => m.open_load(0),
            Some("exports") => m.open_load(1),
            Some("import") => m.open_import(),
            Some("import:go") => {
                m.auto_import = true;
                m.open_import();
            }
            Some("settings") => m.page = Page::Settings,
            _ => {}
        }
        m
    }

    fn scale(&self) -> f32 {
        self.ui_scale_cli.unwrap_or(self.settings.ui_scale)
    }

    fn open_load(&mut self, tab: usize) {
        self.saves = crate::save::list_saves(&crate::save::save_dir());
        self.exports = crate::importer::list_exports(&crate::importer::export_root());
        self.load_tab = tab;
        self.scroll = 0;
        self.page = Page::Load;
    }

    fn open_import(&mut self) {
        let df = rf_gfx::df_dir();
        self.import = Some(ImportState::Checking(std::thread::spawn(move || crate::importer::probe(&df))));
        self.page = Page::Import;
    }

    fn start(&mut self, t: Target, args: &Args, first_save: bool) {
        self.error = None;
        let what = match &t {
            Target::Synthetic => "a synthetic fort".to_string(),
            other => describe(other),
        };
        self.loading = Some(Loading { what, started: get_time(), handle: spawn_load(t, args.size, args.seed, first_save) });
        self.page = Page::Loading;
    }

    fn back_to_title(&mut self) {
        if let Some(ImportState::Running(job)) = &self.import {
            job.cancel();
        }
        self.import = None;
        self.editing_dir = false;
        self.page = Page::Title;
        self.continue_target = continue_target();
    }

    fn store_settings(&mut self) {
        let had = !self.settings.df_dir.is_empty();
        self.settings.df_dir = self.dir_text.trim().to_string();
        // Back to the default: drop the folder this page set before (an env var set outside
        // Rustfort, with no folder in the settings, stays).
        if self.settings.df_dir.is_empty() && had {
            std::env::remove_var("RUSTFORT_DF_DIR");
        }
        self.settings.apply_df_dir();
        if self.settings_writable {
            if let Err(e) = self.settings.save() {
                self.error = Some(format!("settings not saved: {e}"));
            }
        }
        // A changed DF folder brings other art and fonts.
        let art = rf_gfx::df_dir().join("data").join("art");
        if art != self.art_dir {
            self.font = Font::load(&rf_gfx::df_dir());
            self.binds = Bindings::load(&rf_gfx::df_dir());
            self.bg = load_tex(&art.join("title_background.png"));
            self.logo = load_tex(&art.join("df_logo.png"));
            self.art_dir = art;
        }
    }

    /// One frame. Returns a loaded world when the player picked one.
    fn frame(&mut self, args: &Args) -> Option<Loaded> {
        let s = self.scale();
        self.ui.begin(&self.input, s);
        self.keys();
        clear_background(Color::from_rgba(8, 7, 6, 255));
        self.draw_background();
        let mut picked = None;
        match self.page {
            Page::Title => self.draw_title(args),
            Page::Load => self.draw_load(args),
            Page::Import => self.draw_import(args),
            Page::Settings => self.draw_settings(),
            Page::Loading => picked = self.draw_loading(),
        }
        if let Some((t, p)) = self.ui.tooltip.take() {
            let w = self.font.width(&t, s) + 14.0 * s;
            let x = p.x.min(screen_width() - w - 4.0).max(2.0);
            let y = (p.y - GH * s - 10.0 * s).max(2.0);
            draw_rectangle(x, y, w, GH * s + 10.0 * s, Color::new(0.04, 0.04, 0.05, 0.95));
            draw_rectangle_lines(x, y, w, GH * s + 10.0 * s, 1.0, col::BORDER_HI);
            self.font.draw(&t, x + 7.0 * s, y + 5.0 * s, s, col::TEXT);
        }
        picked
    }

    fn keys(&mut self) {
        let esc = self.input.fired.contains(&KeyCode::Escape);
        if self.editing_dir {
            let ctrl = self.input.mods & crate::keys::CTRL != 0;
            for c in self.input.chars.clone() {
                if !c.is_control() && !ctrl {
                    self.dir_text.push(c);
                }
            }
            if ctrl && self.input.fired.contains(&KeyCode::V) {
                if let Some(t) = macroquad::miniquad::window::clipboard_get() {
                    self.dir_text.push_str(t.trim().trim_matches('"'));
                }
            }
            if self.input.fired.contains(&KeyCode::Backspace) {
                self.dir_text.pop();
            }
            if esc || self.input.fired.contains(&KeyCode::Enter) || self.input.fired.contains(&KeyCode::KpEnter) {
                self.editing_dir = false;
            }
            return;
        }
        if esc {
            match self.page {
                Page::Title | Page::Loading => {}
                Page::Settings => {
                    self.store_settings();
                    self.back_to_title();
                }
                _ => self.back_to_title(),
            }
        }
    }

    fn draw_background(&mut self) {
        let (sw, sh) = (screen_width(), screen_height());
        if let Some(t) = &self.bg {
            // Cover the window, keep the aspect ratio (crop the overflow), anchored right like
            // DF so the dwarves stay in view.
            let (tw, th) = (t.width(), t.height());
            let k = (sw / tw).max(sh / th);
            let (w, h) = (tw * k, th * k);
            draw_texture_ex(t, sw - w, (sh - h) / 2.0, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
        } else {
            for i in 0..24 {
                let f = i as f32 / 24.0;
                draw_rectangle(0.0, sh * f, sw, sh / 24.0 + 1.0, Color::new(0.06 + 0.05 * f, 0.045 + 0.03 * f, 0.03 + 0.01 * f, 1.0));
            }
        }
        if self.page != Page::Title {
            draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
        }
    }

    /// Logo and the "Rustfort" line; returns the y below them.
    fn draw_logo(&mut self, top: f32, max_w: f32) -> f32 {
        let s = self.scale();
        let sw = screen_width();
        let mut y = top;
        if let Some(t) = &self.logo {
            let w = max_w.min(t.width() * s * 0.8);
            let h = w * t.height() / t.width();
            draw_texture_ex(t, (sw - w) / 2.0, y, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
            y += h + 4.0 * s;
        } else {
            let t = "DWARF FORTRESS";
            self.font.draw_shadow(t, (sw - self.font.width(t, 4.0 * s)) / 2.0, y, 4.0 * s, col::TEXT_HI);
            y += 4.0 * GH * s + 10.0 * s;
        }
        let sub = "R U S T F O R T";
        let k = 2.5 * s;
        self.font.draw_shadow(sub, (sw - self.font.width(sub, k)) / 2.0, y, k, Color::from_rgba(235, 200, 120, 255));
        y += GH * k + 6.0 * s;
        let tag = "your fortress, continued - fast";
        self.font.draw_shadow(tag, (sw - self.font.width(tag, s)) / 2.0, y, s, Color::from_rgba(200, 190, 170, 255));
        y + GH * s + 18.0 * s
    }

    fn draw_title(&mut self, args: &Args) {
        let s = self.scale();
        let (sw, sh) = (screen_width(), screen_height());
        let y0 = self.draw_logo(sh * 0.05, (sw * 0.42).min(760.0 * s));
        let bw = (440.0 * s).min(sw - 40.0);
        let bh = 32.0 * s;
        let gap = 8.0 * s;
        let n = 6.0;
        let ph = n * (bh + gap) + 18.0 * s + if self.continue_target.is_some() { GH * s + 6.0 * s } else { 0.0 };
        let px = (sw - bw) / 2.0 - 16.0 * s;
        let py = y0.min(sh - ph - 60.0 * s).max(8.0);
        let pr = Rect::new(px, py, bw + 32.0 * s, ph);
        draw_rectangle(pr.x, pr.y, pr.w, pr.h, Color::new(0.03, 0.025, 0.02, 0.78));
        draw_rectangle_lines(pr.x, pr.y, pr.w, pr.h, 2.0, col::BORDER);
        let bx = px + 16.0 * s;
        let mut by = py + 12.0 * s;
        let ct = self.continue_target.clone();
        let enter = self.input.fired.contains(&KeyCode::Enter) || self.input.fired.contains(&KeyCode::KpEnter);
        let cont = text_button(&mut self.ui, &self.font, Rect::new(bx, by, bw, bh), "Continue", if ct.is_some() { BtnState::Normal } else { BtnState::Disabled }) || enter;
        by += bh + 3.0 * s;
        if let Some(t) = &ct {
            let d = clip(&describe(t), ((bw + 28.0 * s) / (GW * s)) as usize);
            self.font.draw(&d, bx + (bw - self.font.width(&d, s)) / 2.0, by, s, col::TEXT_DIM);
            by += GH * s + 3.0 * s;
        }
        by += gap - 3.0 * s;
        let click = |m: &mut Menu, label: &str, tip: &str, by: &mut f32| {
            let r = Rect::new(bx, *by, bw, bh);
            let c = text_button(&mut m.ui, &m.font, r, label, BtnState::Normal);
            if m.ui.hovered(r) && !tip.is_empty() {
                m.ui.tooltip = Some((tip.to_string(), vec2(r.x, r.y + r.h + GH * s + 16.0 * s)));
            }
            *by += bh + gap;
            c
        };
        let load = click(self, "Load game", "Your saves (and autosaves) and earlier DF exports", &mut by);
        let import = click(self, "Import from Dwarf Fortress", "Copy the fort that is running in DF right now (read-only, DF keeps running)", &mut by);
        let synth = click(self, "New synthetic fort (dev)", "A small generated test fort", &mut by);
        let sett = click(self, "Settings", "Interface size, frame rate, autosave, DF folder", &mut by);
        let quit = click(self, "Quit", "", &mut by);
        if let Some(e) = &self.error {
            let e = clip(&format!("Could not load: {e}"), (sw / (GW * s)) as usize - 4);
            self.font.draw_shadow(&e, (sw - self.font.width(&e, s)) / 2.0, by + 10.0 * s, s, col::TEXT_BAD);
        }
        // Footer: where things are.
        let df = rf_gfx::df_dir();
        let df_ok = df.join("data").join("art").exists();
        let left = "F1 in the game: all controls";
        self.font.draw_shadow(left, 10.0 * s, sh - GH * s - 10.0 * s, s, Color::from_rgba(190, 180, 160, 255));
        let right = format!("DF: {} {}", crate::importer::plain(&df).display(), if df_ok { "√" } else { "(not found - see Settings)" });
        let right = clip(&right, ((sw * 0.6) / (GW * s)) as usize);
        self.font.draw_shadow(&right, sw - self.font.width(&right, s) - 10.0 * s, sh - GH * s - 10.0 * s, s, if df_ok { Color::from_rgba(190, 180, 160, 255) } else { col::TEXT_BAD });
        if cont {
            if let Some(t) = ct {
                self.start(t, args, false);
            }
        } else if load {
            self.open_load(0);
        } else if import {
            self.open_import();
        } else if synth {
            self.start(Target::Synthetic, args, false);
        } else if sett {
            self.dir_text = self.settings.df_dir.clone();
            self.page = Page::Settings;
        } else if quit {
            self.quit = true;
        }
    }

    /// A centered page panel with a title; returns the content rectangle.
    fn page_panel(&mut self, title: &str, w: f32, h: f32) -> Rect {
        let s = self.scale();
        let (sw, sh) = (screen_width(), screen_height());
        let (w, h) = ((w * s).min(sw - 24.0), (h * s).min(sh - 24.0));
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        panel(&mut self.ui, r);
        let pad = 16.0 * s;
        self.font.draw(title, r.x + pad, r.y + pad, 1.5 * s, col::TEXT_HI);
        Rect::new(r.x + pad, r.y + pad + 1.5 * GH * s + 14.0 * s, w - 2.0 * pad, h - 2.0 * pad - 1.5 * GH * s - 14.0 * s)
    }

    fn back_button(&mut self, c: Rect) -> bool {
        let s = self.scale();
        let bh = 26.0 * s;
        text_button(&mut self.ui, &self.font, Rect::new(c.x, c.y + c.h - bh, 110.0 * s, bh), "Back", BtnState::Normal)
    }

    fn draw_load(&mut self, args: &Args) {
        let s = self.scale();
        let lh = (GH + 3.0) * s;
        let c = self.page_panel("Load game", 820.0, 540.0);
        // Tabs.
        let tabs = [format!("Saves ({})", self.saves.len()), format!("DF exports ({})", self.exports.len())];
        let mut tx = c.x;
        for (i, t) in tabs.iter().enumerate() {
            let w = (t.chars().count() as f32 + 4.0) * GW * s;
            if text_button(&mut self.ui, &self.font, Rect::new(tx, c.y, w, 24.0 * s), t, if self.load_tab == i { BtnState::Active } else { BtnState::Normal }) && self.load_tab != i {
                self.load_tab = i;
                self.scroll = 0;
            }
            tx += w + 6.0 * s;
        }
        let dir = if self.load_tab == 0 { crate::save::save_dir() } else { crate::importer::export_root() };
        let dir = std::fs::canonicalize(&dir).map(|p| crate::importer::plain(&p)).unwrap_or(dir);
        let dt = clip(&dir.display().to_string(), ((c.x + c.w - tx) / (GW * s)) as usize - 2);
        self.font.draw(&dt, c.x + c.w - self.font.width(&dt, s), c.y + 6.0 * s, s, col::TEXT_DIM);
        let mut y = c.y + 34.0 * s;
        let cols: [f32; 4] = if self.load_tab == 0 { [0.0, 0.36, 0.60, 0.86] } else { [0.0, 0.34, 0.44, 0.66] };
        let heads = if self.load_tab == 0 { ["Fort", "Game date", "Saved", "Size"] } else { ["Fort", "Year", "Exported", "Folder"] };
        for (i, hd) in heads.iter().enumerate() {
            self.font.draw(hd, c.x + cols[i] * c.w + 4.0 * s, y, s, col::BORDER_HI);
        }
        y += lh;
        draw_line(c.x, y, c.x + c.w, y, 1.0, col::BORDER);
        y += 4.0 * s;
        let row_h = lh + 6.0 * s;
        let area = Rect::new(c.x, y, c.w, c.y + c.h - 36.0 * s - y);
        let rows = ((area.h / row_h) as usize).max(1);
        let n = if self.load_tab == 0 { self.saves.len() } else { self.exports.len() };
        let step = wheel_over(&self.ui, &mut self.input, area);
        self.scroll = (self.scroll as i32 + step * 2).clamp(0, n.saturating_sub(rows) as i32) as usize;
        if n == 0 {
            let t = if self.load_tab == 0 { "No saves yet. In the game: F5 or Esc > Save game." } else { "No exports yet: use \"Import from Dwarf Fortress\" on the title screen." };
            self.font.draw(t, c.x + 4.0 * s, y + 4.0 * s, s, col::TEXT_DIM);
        }
        let mut pick: Option<Target> = None;
        for k in self.scroll..(self.scroll + rows).min(n) {
            let rr = Rect::new(c.x, y, c.w, row_h - 2.0 * s);
            if k % 2 == 0 {
                draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.04));
            }
            let clicked = list_row(&mut self.ui, rr, false);
            let ty = y + 3.0 * s;
            let cx = |i: usize| c.x + cols[i] * c.w + 4.0 * s;
            let cw = |i: usize| (((cols.get(i + 1).copied().unwrap_or(1.0) - cols[i]) * c.w / (GW * s)) as usize).saturating_sub(1).max(4);
            if self.load_tab == 0 {
                let sv = &self.saves[k];
                let mut fort = if sv.header.fort_native.is_empty() { sv.header.fort.clone() } else { format!("{} ({})", sv.header.fort, sv.header.fort_native) };
                if crate::save::is_autosave(&sv.path) {
                    fort = format!("{fort} [auto]");
                }
                self.font.draw(&clip(&fort, cw(0)), cx(0), ty, s, col::TEXT);
                self.font.draw(&clip(&sv.header.date, cw(1)), cx(1), ty, s, col::TEXT);
                self.font.draw(&crate::save::fmt_unix(sv.header.saved_unix), cx(2), ty, s, col::TEXT_DIM);
                self.font.draw(&crate::save::fmt_bytes(sv.bytes), cx(3), ty, s, col::TEXT_DIM);
                if self.ui.hovered(rr) {
                    self.ui.tooltip = Some((format!("{} - click to load", sv.path.display()), vec2(rr.x, rr.y)));
                }
                if clicked {
                    pick = Some(Target::Save(sv.clone()));
                }
            } else {
                let ex = &self.exports[k];
                let fort = if ex.fort_native.is_empty() { ex.fort.clone() } else { format!("{} ({})", ex.fort, ex.fort_native) };
                let folder = ex.dir.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                self.font.draw(&clip(&fort, cw(0)), cx(0), ty, s, col::TEXT);
                self.font.draw(&format!("{}", ex.year), cx(1), ty, s, col::TEXT);
                self.font.draw(&crate::save::fmt_unix(ex.when), cx(2), ty, s, col::TEXT_DIM);
                self.font.draw(&clip(&folder, cw(3)), cx(3), ty, s, col::TEXT_DIM);
                if self.ui.hovered(rr) {
                    self.ui.tooltip = Some((format!("{} - click to start from this export", ex.dir.display()), vec2(rr.x, rr.y)));
                }
                if clicked {
                    pick = Some(Target::Export(ex.dir.clone()));
                }
            }
            y += row_h;
        }
        if self.back_button(c) {
            self.back_to_title();
        }
        if let Some(t) = pick {
            self.start(t, args, false);
        }
    }

    fn draw_import(&mut self, args: &Args) {
        let s = self.scale();
        let lh = (GH + 4.0) * s;
        let c = self.page_panel("Import from Dwarf Fortress", 720.0, 400.0);
        // Advance the state machine.
        let mut next: Option<ImportState> = None;
        let mut done_dir: Option<PathBuf> = None;
        match self.import.as_mut() {
            Some(ImportState::Checking(h)) if h.is_finished() => {
                if let Some(ImportState::Checking(h)) = self.import.take() {
                    next = Some(ImportState::Probed(h.join().unwrap_or(Probe::NoDfhack("the check crashed".into()))));
                }
            }
            Some(ImportState::Starting(h)) if h.is_finished() => {
                if let Some(ImportState::Starting(h)) = self.import.take() {
                    next = Some(match h.join().unwrap_or_else(|_| Err("crashed".into())) {
                        Ok(job) => ImportState::Running(job),
                        Err(e) => ImportState::Failed(e),
                    });
                }
            }
            Some(ImportState::Running(job)) => match job.poll() {
                JobState::Done => done_dir = Some(job.dir.clone()),
                JobState::Failed(e) => next = Some(ImportState::Failed(e)),
                JobState::Running(_) => {}
            },
            _ => {}
        }
        if let Some(n) = next {
            self.import = Some(n);
        }
        if let Some(d) = done_dir {
            self.import = None;
            self.start(Target::Export(d), args, true);
            return;
        }
        let maxc = (c.w / (GW * s)) as usize;
        let mut y = c.y;
        let line = |f: &Font, t: &str, color: Color, y: &mut f32| {
            for part in wrap(t, maxc) {
                f.draw(&part, c.x, *y, s, color);
                *y += lh;
            }
        };
        let bh = 28.0 * s;
        let by = c.y + c.h - bh;
        let mut retry = false;
        let mut go: Option<FortInfo> = None;
        let mut to_settings = false;
        let mut cancel = false;
        let df = rf_gfx::df_dir();
        match &self.import {
            None | Some(ImportState::Checking(_)) => {
                let dots = ".".repeat(((get_time() * 3.0) as usize) % 4);
                line(&self.font, &format!("Looking for Dwarf Fortress and DFHack{dots}"), col::TEXT, &mut y);
            }
            Some(ImportState::Probed(p)) => match p.clone() {
                Probe::NoDfhackRun(path) => {
                    line(&self.font, "DFHack was not found.", col::TEXT_BAD, &mut y);
                    line(&self.font, &format!("Expected {}", path.display()), col::TEXT_DIM, &mut y);
                    y += 6.0 * s;
                    line(&self.font, "Check the Dwarf Fortress folder in Settings, and install DFHack (on Steam it is the free \"DFHack\" app).", col::TEXT, &mut y);
                    to_settings = true;
                }
                Probe::NoTools => {
                    line(&self.font, "The Rustfort DFHack scripts were not found (tools/dfhack/rustfort-export.lua).", col::TEXT_BAD, &mut y);
                    line(&self.font, "Start Rustfort with rustfort.cmd from its folder.", col::TEXT, &mut y);
                }
                Probe::NotRunning => {
                    line(&self.font, "Dwarf Fortress is not running.", col::TEXT_BAD, &mut y);
                    y += 6.0 * s;
                    line(&self.font, "Start Dwarf Fortress with DFHack, load your fort, then press Retry.", col::TEXT, &mut y);
                    line(&self.font, "Rustfort copies the fort while DF keeps running.", col::TEXT_DIM, &mut y);
                }
                Probe::NoDfhack(msg) => {
                    line(&self.font, "Dwarf Fortress is running, but DFHack does not answer.", col::TEXT_BAD, &mut y);
                    line(&self.font, &msg, col::TEXT_DIM, &mut y);
                    y += 6.0 * s;
                    line(&self.font, "Is DFHack installed and enabled? On Steam it is the free \"DFHack\" app; DF shows its version in the corner of the title screen.", col::TEXT, &mut y);
                }
                Probe::NoFort(msg) => {
                    line(&self.font, "DFHack answers, but no fort is loaded.", col::TEXT_BAD, &mut y);
                    line(&self.font, &msg, col::TEXT_DIM, &mut y);
                    y += 6.0 * s;
                    line(&self.font, "Load your fort in Dwarf Fortress, then press Retry.", col::TEXT, &mut y);
                }
                Probe::Ready(info) => {
                    let native = if info.fort_native.is_empty() { String::new() } else { format!(" ({})", info.fort_native) };
                    line(&self.font, &format!("Found {}{native} in the world {}.", info.fort, info.world), col::TEXT_GOOD, &mut y);
                    line(&self.font, &format!("Year {}, {} citizens, DFHack {}.", info.year, info.citizens, info.dfhack), col::TEXT, &mut y);
                    y += 8.0 * s;
                    line(&self.font, "The exporter only reads: Dwarf Fortress keeps running and is never paused or changed. It takes about half a minute to a minute; DF runs a bit slower meanwhile.", col::TEXT, &mut y);
                    y += 8.0 * s;
                    let into = crate::importer::export_root().join(crate::importer::export_dir_name(&info.fort_native, &info.fort, crate::save::now_unix()));
                    line(&self.font, &format!("Into: {}  (earlier exports are kept)", into.display()), col::TEXT_DIM, &mut y);
                    go = Some(info);
                }
            },
            Some(ImportState::Starting(_)) => line(&self.font, "Starting the exporter in DFHack...", col::TEXT, &mut y),
            Some(ImportState::Running(job)) => {
                let p = job.progress.clone();
                let el = job.started.elapsed().as_secs_f64();
                line(&self.font, &format!("Exporting from Dwarf Fortress ... {:.0} s", el), col::TEXT, &mut y);
                y += 8.0 * s;
                let bar = Rect::new(c.x, y, c.w, 22.0 * s);
                draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(0.12, 0.12, 0.14, 1.0));
                draw_rectangle(bar.x, bar.y, bar.w * p.frac.clamp(0.0, 1.0), bar.h, Color::from_rgba(70, 150, 60, 255));
                draw_rectangle_lines(bar.x, bar.y, bar.w, bar.h, 1.0, col::BORDER_HI);
                let pct = format!("{:.0} %", p.frac * 100.0);
                self.font.draw(&pct, bar.x + (bar.w - self.font.width(&pct, s)) / 2.0, bar.y + (bar.h - GH * s) / 2.0, s, col::TEXT_HI);
                y += bar.h + 10.0 * s;
                let ph = if p.phase.is_empty() { "Waiting for DF".to_string() } else { crate::importer::phase_label(&p.phase) };
                let det = if p.detail.is_empty() { String::new() } else { format!(" ({})", p.detail) };
                line(&self.font, &format!("{ph}{det}"), col::TEXT_DIM, &mut y);
                line(&self.font, &format!("Into {}", short_path(&job.dir)), col::TEXT_DIM, &mut y);
                cancel = true;
            }
            Some(ImportState::Failed(e)) => {
                line(&self.font, "The import failed.", col::TEXT_BAD, &mut y);
                line(&self.font, e, col::TEXT, &mut y);
                retry = true;
            }
        }
        let _ = y;
        let mut bx = c.x;
        if self.back_button(c) {
            self.back_to_title();
            return;
        }
        bx += 110.0 * s + 10.0 * s;
        let show_retry = retry || matches!(&self.import, Some(ImportState::Probed(p)) if !matches!(p, Probe::Ready(_)));
        if show_retry && text_button(&mut self.ui, &self.font, Rect::new(bx, by, 110.0 * s, bh), "Retry", BtnState::Normal) {
            self.open_import();
            return;
        }
        if show_retry {
            bx += 120.0 * s;
        }
        if to_settings && text_button(&mut self.ui, &self.font, Rect::new(bx, by, 130.0 * s, bh), "Settings", BtnState::Normal) {
            self.dir_text = self.settings.df_dir.clone();
            self.page = Page::Settings;
            return;
        }
        if cancel && text_button(&mut self.ui, &self.font, Rect::new(c.x + c.w - 150.0 * s, by, 150.0 * s, bh), "Cancel export", BtnState::Normal) {
            if let Some(ImportState::Running(job)) = &self.import {
                job.cancel();
            }
        }
        if let Some(info) = go {
            let clicked = text_button(&mut self.ui, &self.font, Rect::new(c.x + c.w - 200.0 * s, by, 200.0 * s, bh), "Import and play", BtnState::Active);
            // Test hook `--menu-page import:go`: start as soon as DF is found (once).
            if clicked || std::mem::take(&mut self.auto_import) {
                let dfc = df.clone();
                self.import = Some(ImportState::Starting(std::thread::spawn(move || ExportJob::start(&dfc, &info))));
            }
        }
    }

    fn draw_settings(&mut self) {
        let s = self.scale();
        let lh = (GH + 3.0) * s;
        let c = self.page_panel("Settings", 720.0, 560.0);
        let bh = GH * s + 10.0 * s;
        let lx = c.x;
        let vx = c.x + 230.0 * s;
        let mut y = c.y;
        let small = |m: &mut Menu, x: f32, y: f32, t: &str| text_button(&mut m.ui, &m.font, Rect::new(x, y, 34.0 * s, bh), t, BtnState::Normal);
        let value = |m: &mut Menu, x: f32, y: f32, t: &str| {
            let w = 12.0 * GW * s;
            m.font.draw(t, x + (w - m.font.width(t, s)) / 2.0, y + 5.0 * s, s, col::TEXT_HI);
            w
        };
        // Interface size.
        self.font.draw("Interface size", lx, y + 5.0 * s, s, col::TEXT);
        let mut x = vx;
        if small(self, x, y, "-") {
            self.settings.ui_scale = ((self.settings.ui_scale - 0.25).max(0.5) * 4.0).round() / 4.0;
        }
        x += 38.0 * s;
        x += value(self, x, y, &format!("{:.0} %", self.settings.ui_scale * 100.0));
        if small(self, x, y, "+") {
            self.settings.ui_scale = ((self.settings.ui_scale + 0.25).min(3.0) * 4.0).round() / 4.0;
        }
        if self.ui_scale_cli.is_some() {
            self.font.draw("(--ui-scale given)", x + 44.0 * s, y + 5.0 * s, s, col::TEXT_DIM);
        }
        y += bh + 10.0 * s;
        // Frame cap.
        self.font.draw("Frame rate cap", lx, y + 5.0 * s, s, col::TEXT);
        let mut x = vx;
        if small(self, x, y, "-") {
            self.settings.step_gfps(false);
        }
        x += 38.0 * s;
        x += value(self, x, y, &format!("{} FPS", self.settings.gfps));
        if small(self, x, y, "+") {
            self.settings.step_gfps(true);
        }
        self.font.draw("(used when vsync is off)", x + 44.0 * s, y + 5.0 * s, s, col::TEXT_DIM);
        y += bh + 10.0 * s;
        let vs = self.settings.vsync;
        if checkbox(&mut self.ui, &self.font, vx, y, vs, "Vsync (smooth; applies at the next start)") {
            self.settings.vsync = !vs;
        }
        self.font.draw("Vsync", lx, y, s, col::TEXT);
        y += lh + 14.0 * s;
        // Autosave.
        self.font.draw("Autosave", lx, y + 5.0 * s, s, col::TEXT);
        let label = self.settings.autosave_label();
        if text_button(&mut self.ui, &self.font, Rect::new(vx, y, 18.0 * GW * s, bh), &label, BtnState::Normal) {
            self.settings.cycle_autosave();
        }
        let mut x = vx + 18.0 * GW * s + 16.0 * s;
        self.font.draw("keep", x, y + 5.0 * s, s, col::TEXT_DIM);
        x += 5.0 * GW * s;
        if small(self, x, y, "-") {
            self.settings.autosave_keep = self.settings.autosave_keep.saturating_sub(1).max(1);
        }
        x += 38.0 * s;
        let kt = format!("{}", self.settings.autosave_keep);
        self.font.draw(&kt, x + 4.0 * s, y + 5.0 * s, s, col::TEXT_HI);
        x += 4.0 * GW * s;
        if small(self, x, y, "+") {
            self.settings.autosave_keep = (self.settings.autosave_keep + 1).min(50);
        }
        y += bh + 14.0 * s;
        // Defaults for new imports.
        self.font.draw("For new imports:", lx, y, s, col::BORDER_HI);
        y += lh + 4.0 * s;
        self.font.draw("Population cap", lx, y + 5.0 * s, s, col::TEXT);
        let mut x = vx;
        let from_save = self.settings.pop_cap.is_none();
        if text_button(&mut self.ui, &self.font, Rect::new(x, y, 16.0 * GW * s, bh), if from_save { "From the save" } else { "Set:" }, if from_save { BtnState::Active } else { BtnState::Normal }) {
            self.settings.pop_cap = if from_save { Some(120) } else { None };
        }
        x += 16.0 * GW * s + 8.0 * s;
        if let Some(cap) = self.settings.pop_cap {
            let mut nc = cap;
            for (t, d) in [("-10", -10), ("-1", -1)] {
                if small(self, x, y, t) {
                    nc = (cap + d).max(0);
                }
                x += 38.0 * s;
            }
            x += value(self, x, y, &if cap > 0 { format!("{cap}") } else { "no cap".into() });
            for (t, d) in [("+1", 1), ("+10", 10)] {
                if small(self, x, y, t) {
                    nc = cap + d;
                }
                x += 38.0 * s;
            }
            self.settings.pop_cap = Some(nc);
        }
        y += bh + 8.0 * s;
        self.font.draw("Invasions", lx, y + 5.0 * s, s, col::TEXT);
        let inv = match self.settings.invasions {
            None => "From the save",
            Some(true) => "On",
            Some(false) => "Off",
        };
        if text_button(&mut self.ui, &self.font, Rect::new(vx, y, 16.0 * GW * s, bh), inv, BtnState::Normal) {
            self.settings.invasions = match self.settings.invasions {
                None => Some(true),
                Some(true) => Some(false),
                Some(false) => None,
            };
        }
        y += bh + 14.0 * s;
        // DF folder.
        self.font.draw("Dwarf Fortress folder", lx, y + 5.0 * s, s, col::TEXT);
        let fr = Rect::new(vx, y, c.x + c.w - vx - 96.0 * s, bh);
        draw_rectangle(fr.x, fr.y, fr.w, fr.h, Color::new(0.08, 0.08, 0.09, 1.0));
        draw_rectangle_lines(fr.x, fr.y, fr.w, fr.h, 1.0, if self.editing_dir { col::ACTIVE_BORDER } else { Color::new(0.4, 0.4, 0.44, 1.0) });
        let shown = if self.dir_text.is_empty() && !self.editing_dir { format!("(default) {}", crate::importer::plain(&rf_gfx::df_dir()).display()) } else { self.dir_text.clone() };
        let maxc = ((fr.w - 10.0 * s) / (GW * s)) as usize;
        let tail: String = {
            let n = shown.chars().count();
            if n > maxc {
                shown.chars().skip(n - maxc).collect()
            } else {
                shown.clone()
            }
        };
        let caret = if self.editing_dir && (get_time() * 2.0) as i64 % 2 == 0 { "_" } else { "" };
        self.font.draw(&format!("{tail}{caret}"), fr.x + 5.0 * s, fr.y + 5.0 * s, s, if self.dir_text.is_empty() && !self.editing_dir { col::TEXT_DIM } else { col::TEXT });
        if self.ui.clicked(fr) {
            self.editing_dir = true;
        } else if self.ui.lpressed && !self.ui.hovered(fr) {
            self.editing_dir = false;
        }
        if text_button(&mut self.ui, &self.font, Rect::new(fr.x + fr.w + 8.0 * s, y, 88.0 * s, bh), "Default", BtnState::Normal) {
            self.dir_text.clear();
            self.editing_dir = false;
        }
        y += bh + 4.0 * s;
        let probe_dir = if self.dir_text.trim().is_empty() { rf_gfx::df_dir() } else { PathBuf::from(self.dir_text.trim()) };
        let has_df = probe_dir.join("Dwarf Fortress.exe").exists() || probe_dir.join("data").join("art").exists();
        let has_hack = crate::importer::dfhack_run(&probe_dir).exists();
        let (st, sc) = match (has_df, has_hack) {
            (true, true) => ("Found Dwarf Fortress and DFHack.", col::TEXT_GOOD),
            (true, false) => ("Found Dwarf Fortress, but no DFHack (needed for the import).", col::TEXT_HI),
            _ => ("No Dwarf Fortress there (graphics, font and import need it).", col::TEXT_BAD),
        };
        self.font.draw(st, vx, y, s, sc);
        y += lh;
        self.font.draw("Click to edit, Ctrl+V pastes; applies on Back.", vx, y, s, col::TEXT_DIM);
        y += lh + 14.0 * s;
        // Hints.
        self.font.draw("First-time hints", lx, y + 5.0 * s, s, col::TEXT);
        let n_seen = self.settings.hints_seen.len();
        let hl = if self.settings.hints_off || n_seen >= crate::ui::help::HINTS.len() { "Show them again" } else { "Showing" };
        if text_button(&mut self.ui, &self.font, Rect::new(vx, y, 18.0 * GW * s, bh), hl, BtnState::Normal) {
            self.settings.hints_off = false;
            self.settings.hints_seen.clear();
        }
        let _ = y;
        let path = crate::importer::plain(&std::fs::canonicalize(crate::settings::path()).unwrap_or_else(|_| crate::settings::path())).display().to_string();
        let note = clip(&format!("Saved in {path}"), ((c.w - 130.0 * s) / (GW * s)) as usize);
        self.font.draw(&note, c.x + 124.0 * s, c.y + c.h - 18.0 * s, s, col::TEXT_DIM);
        if self.back_button(c) {
            self.store_settings();
            self.back_to_title();
        }
    }

    fn draw_loading(&mut self) -> Option<Loaded> {
        let s = self.scale();
        let (sw, sh) = (screen_width(), screen_height());
        let what = self.loading.as_ref().map(|l| l.what.clone()).unwrap_or_default();
        let el = self.loading.as_ref().map(|l| get_time() - l.started).unwrap_or(0.0);
        let t = clip(&format!("Loading {what}"), (sw / (GW * s)) as usize - 4);
        let k = 1.5 * s;
        self.font.draw_shadow(&t, (sw - self.font.width(&t, k)) / 2.0, sh * 0.46, k, col::TEXT_HI);
        // Indeterminate bar.
        let bw = (420.0 * s).min(sw - 40.0);
        let bx = (sw - bw) / 2.0;
        let by = sh * 0.46 + 2.0 * GH * k;
        draw_rectangle(bx, by, bw, 10.0 * s, Color::new(0.12, 0.12, 0.14, 1.0));
        let p = (el * 0.8).fract() as f32;
        let seg = bw * 0.25;
        let x0 = bx + (bw + seg) * p - seg;
        let (a, b) = (x0.max(bx), (x0 + seg).min(bx + bw));
        if b > a {
            draw_rectangle(a, by, b - a, 10.0 * s, Color::from_rgba(200, 160, 70, 255));
        }
        draw_rectangle_lines(bx, by, bw, 10.0 * s, 1.0, col::BORDER);
        let finished = self.loading.as_ref().map(|l| l.handle.is_finished()).unwrap_or(true);
        if !finished {
            return None;
        }
        let Some(l) = self.loading.take() else {
            self.page = Page::Title;
            return None;
        };
        match l.handle.join().unwrap_or_else(|_| Err("the loader crashed".into())) {
            Ok(loaded) => Some(loaded),
            Err(e) => {
                self.error = Some(e);
                self.back_to_title();
                None
            }
        }
    }
}

/// A path relative to the working directory when it is inside it (`export\tileshtulon-...`).
pub fn short_path(p: &std::path::Path) -> String {
    let cwd = std::env::current_dir().ok().and_then(|c| std::fs::canonicalize(c).ok()).map(|c| crate::importer::plain(&c));
    match cwd.as_ref().and_then(|c| p.strip_prefix(c).ok()) {
        Some(rel) => rel.display().to_string(),
        None => p.display().to_string(),
    }
}

/// Word-wrap to at most `n` characters per line.
pub fn wrap(t: &str, n: usize) -> Vec<String> {
    let n = n.max(10);
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in t.split(' ') {
        let len = cur.chars().count();
        if len > 0 && len + 1 + w.chars().count() > n {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
        while cur.chars().count() > n {
            let head: String = cur.chars().take(n).collect();
            cur = cur.chars().skip(n).collect();
            out.push(head);
        }
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

/// The title screen loop. Returns the world to play and the frame count so far, or `None`
/// to quit.
pub async fn run_menu(args: &Args, settings: Settings, start_frame: u32) -> (Option<(Loaded, u32)>, Settings) {
    let mut m = Menu::new(args, settings);
    let mut script = match (&args.script, args.mouse) {
        (Some(sc), mo) => crate::script::Script::parse(sc, mo).ok(),
        (None, Some(mo)) => crate::script::Script::parse("", Some(mo)).ok(),
        (None, None) => None,
    };
    let mut frame = start_frame;
    let mut deadline = std::time::Instant::now();
    loop {
        m.input = m.input_state.collect(&m.binds);
        if let Some(sc) = script.as_mut() {
            sc.apply(frame + 1, &mut m.input);
        }
        let picked = m.frame(args);
        frame += 1;
        if let Some(l) = picked {
            return (Some((l, frame)), m.settings);
        }
        if let Some(path) = &args.screenshot {
            if args.shots_at.contains(&frame) {
                let p = crate::game::extra_shot_path(path, frame);
                crate::game::save_screenshot(&p);
                println!("saved {} (title screen, page {:?})", p.display(), m.page);
            }
            if frame >= args.frames.max(1) {
                let (w, h) = crate::game::save_screenshot(path);
                println!("saved {} ({w}x{h}) after {frame} frames (title screen, page {:?})", path.display(), m.page);
                return (None, m.settings);
            }
        }
        if m.quit {
            return (None, m.settings);
        }
        // Without vsync, pace to the frame cap (also keeps screenshot runs in real time).
        if !args.vsync {
            let cap = if args.gfps_cli { args.gfps } else { m.settings.gfps }.max(10);
            let period = std::time::Duration::from_secs_f64(1.0 / cap as f64);
            deadline += period;
            let now = std::time::Instant::now();
            if deadline > now {
                std::thread::sleep(deadline - now);
            } else if now - deadline > period {
                deadline = now;
            }
        }
        next_frame().await;
    }
}

/// Title screen and game in turn. `initial`: a world loaded from the command line (no title
/// screen first).
pub async fn app_main(mut args: Args, mut settings: Settings, mut initial: Option<Loaded>) {
    let mut frame = 0;
    loop {
        let loaded = match initial.take() {
            Some(l) => l,
            None => {
                let (r, st) = run_menu(&args, settings, frame).await;
                settings = st;
                // `--menu-page` (and `import:go`) only for the first title screen.
                args.menu_page = None;
                match r {
                    Some((l, f)) => {
                        frame = f;
                        l
                    }
                    None => return,
                }
            }
        };
        // Settings changed on the title screen apply to the game (unless given on the CLI).
        if !args.ui_scale_cli {
            args.ui_scale = settings.ui_scale;
        }
        if !args.gfps_cli {
            args.gfps = settings.gfps;
        }
        let (exit, st, f) = crate::game::run(args.clone(), loaded, settings, frame).await;
        settings = st;
        if exit == Exit::Quit {
            return;
        }
        if args.screenshot.is_some() {
            // Scripted runs go on through the title screen with the same frame count.
            frame = f;
        } else {
            // Back to the title screen; a fresh frame count (scripts are for the first run only).
            args.script = None;
            frame = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words() {
        assert_eq!(wrap("aaa bbb ccc", 10), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap("", 10), vec![""]);
        let long = "x".repeat(25);
        let w = wrap(&long, 10);
        assert_eq!(w.len(), 3);
        assert!(w.iter().all(|l| l.chars().count() <= 10));
    }
}
