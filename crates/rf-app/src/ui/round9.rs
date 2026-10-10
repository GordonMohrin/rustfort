//! [app9] Round-9 screens and glue.
//!
//! - **Announcement settings** like DF's `announcements.txt` (flags `BOX`, `P`, `R`, `D_D`):
//!   per announcement type a pop-up box (pauses), pause, recenter and "show in the log". The
//!   defaults come from the player's DF prefs (`prefs/announcements.txt`, then the install's
//!   `data/init/announcements.txt`); changes are kept in `saves/settings.json`.
//! - **Pop-up boxes**: a queue of DF announcement boxes (one at a time, Enter closes).
//! - **Raids**: the sim's spotted lines ("Thief! Protect the hoard from skulking filth!") name
//!   the hostile alert; hidden raiders stay invisible until spotted.
//! - **Petitions**: an alert badge, the petitions screen and a petition box with Accept / Deny.
//! - **Squads**: per-position uniform editor, squad ammunition, patrol routes picked on the map,
//!   barracks assignment.
//! - **Combat reports** grouped per fight (DF Reports), with a jump to the place.
//! - **Skills**: DF skill names (Legendary Miner) and experience bars.

use super::military::{squad_name, uniform_text};
use super::panels::unit_name;
use super::{checkbox, clip, col, list_row, panel, pretty, text_button, BtnState};
use crate::font::GH;
use crate::game::{date_string, Game, LogLine, MenuPage, Screen, Tool};
use macroquad::prelude::*;
use rf_core::{Pos, SquadAmmo, Unit, World};
use rf_sim::{Announcement, AnnouncementKind, Petition};
use std::collections::VecDeque;

// ====================================================================== announcement types

/// DF announcement flags, reduced to what fortress mode uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnnFlags {
    /// `BOX`: a pop-up box that pauses the game.
    pub popup: bool,
    /// `P`: pause.
    pub pause: bool,
    /// `R`: recenter the camera.
    pub recenter: bool,
    /// `D_D`: shown in the announcement lines (the Reports screen always keeps it).
    pub show: bool,
}

const fn fl(popup: bool, pause: bool, recenter: bool, show: bool) -> AnnFlags {
    AnnFlags { popup, pause, recenter, show }
}

/// One announcement type of the settings page.
pub struct AnnType {
    pub id: &'static str,
    pub label: &'static str,
    /// Title of its pop-up box.
    pub title: &'static str,
    /// DF `announcements.txt` tokens whose flags are the default (first one found wins).
    pub df: &'static [&'static str],
    /// Default when no DF file has the token (DF's `data/init` values; Rustfort estimates for
    /// types DF has no token for).
    pub default: AnnFlags,
}

const fn at(id: &'static str, label: &'static str, title: &'static str, df: &'static [&'static str], default: AnnFlags) -> AnnType {
    AnnType { id, label, title, df, default }
}

/// All types, in the order of the settings page. `OTHER` must stay last.
pub const ANN_TYPES: &[AnnType] = &[
    at("SIEGE", "Siege arrives", "Siege!", &[], fl(true, true, false, true)),
    at("AMBUSH", "Ambush spotted", "Ambush!", &[], fl(true, true, false, true)),
    at("THIEF", "Thief spotted", "Thief!", &["AMBUSH_THIEF"], fl(false, false, false, true)),
    at("SNATCHER", "Snatcher spotted", "Snatcher!", &["AMBUSH_SNATCHER"], fl(false, false, false, true)),
    at("ABDUCTED", "Citizen abducted", "Abducted!", &["CITIZEN_SNATCHED"], fl(false, false, false, true)),
    at("MEGABEAST", "Forgotten beast, megabeast", "A monster has come!", &["MEGABEAST_ARRIVAL"], fl(true, false, false, true)),
    at("HOSTILE_ANIMAL", "Hostile wildlife", "Hostile creatures", &[], fl(false, false, false, true)),
    at("INVASION_END", "Invaders retreat", "Retreat", &[], fl(false, false, false, true)),
    at("DEATH", "Citizen dies", "Death", &["CITIZEN_DEATH"], fl(false, false, false, true)),
    at("PET_DEATH", "Pet or livestock dies", "Death", &["PET_DEATH"], fl(false, false, false, true)),
    at("ENEMY_DEATH", "Enemy or wild creature dies", "Death", &[], fl(false, false, false, true)),
    at("MIGRANTS", "Migrants arrive", "Migrants", &["MIGRANT_ARRIVAL", "D_MIGRANTS_ARRIVAL"], fl(false, false, false, true)),
    at("CARAVAN", "Caravan arrives", "Caravan", &["CARAVAN_ARRIVAL"], fl(false, false, false, true)),
    at("VISITOR", "Visitor arrives", "Visitor", &["GUEST_ARRIVAL"], fl(false, false, false, true)),
    at("PETITION", "Petition", "Petition", &[], fl(true, false, false, true)),
    at("MOOD", "Strange mood", "Strange mood", &["STRANGE_MOOD"], fl(false, false, false, true)),
    at("MOOD_CLAIM", "Mood: workshop claimed", "Strange mood", &["MOOD_BUILDING_CLAIMED", "ARTIFACT_BEGUN"], fl(false, false, false, true)),
    at("ARTIFACT", "Artifact created", "Artifact", &["MADE_ARTIFACT"], fl(true, false, false, true)),
    at("BIRTH", "Birth, growing up", "Birth", &["BIRTH_CITIZEN"], fl(false, false, false, true)),
    at("ANIMALS", "Animals: hatched, tamed, adopted", "Animals", &["BIRTH_ANIMAL"], fl(false, false, false, true)),
    at("TANTRUM", "Tantrum, breakdown, insanity", "Breakdown", &["CITIZEN_TANTRUM", "BERSERK_CITIZEN"], fl(false, false, false, true)),
    at("JUSTICE", "Crime and punishment", "Justice", &["CRIME_WITNESS_STOLEN"], fl(false, false, false, true)),
    at("NOBLES", "Nobles, mandates", "Nobles", &["NEW_MANDATE"], fl(false, false, false, true)),
    at("GHOST", "Ghosts, burials", "Ghost", &["GHOST_ATTACK"], fl(false, false, false, true)),
    at("CAVE_IN", "Cave-in", "Cave-in", &["CAVE_COLLAPSE"], fl(false, false, false, true)),
    at("TRAPS", "Traps, bridges", "Trap", &["ANIMAL_TRAP_CATCH"], fl(false, false, false, true)),
    at("DESTROYED", "Building destroyed", "Building destroyed", &[], fl(false, false, false, true)),
    at("JOB", "Job cancellations", "Job", &["CANCEL_JOB"], fl(false, false, false, true)),
    at("OTHER", "Everything else", "Announcement", &[], fl(false, false, false, true)),
];

/// Index of a type id (panics on unknown ids: they are compile-time constants).
pub fn ty(id: &str) -> usize {
    ANN_TYPES.iter().position(|t| t.id == id).unwrap_or(ANN_TYPES.len() - 1)
}

/// Types whose pop-up is the hostile alert (`alerts.rs`, with "Center on them").
fn alert_handled(t: usize) -> bool {
    matches!(ANN_TYPES[t].id, "SIEGE" | "AMBUSH" | "THIEF" | "SNATCHER" | "MEGABEAST")
}

/// Announcement type of a sim line (the sim sends text + kind; matched like DF's message
/// families).
pub fn classify(text: &str, kind: AnnouncementKind) -> usize {
    let t = text.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| t.contains(x));
    let id = if has(&["vile force of darkness"]) || (t.starts_with("an army of") && t.contains("has arrived")) {
        "SIEGE"
    } else if has(&["an ambush"]) {
        "AMBUSH"
    } else if has(&["thief!", "is a thief"]) {
        "THIEF"
    } else if has(&["snatcher!"]) {
        "SNATCHER"
    } else if has(&["has been abducted"]) {
        "ABDUCTED"
    } else if has(&["forgotten beast"]) && t.contains("has come") {
        "MEGABEAST"
    } else if has(&["are retreating", "invasion is over", "could not find a way to the fortress"]) {
        "INVASION_END"
    } else if kind == AnnouncementKind::Death {
        "DEATH"
    } else if has(&["petitioned for"]) {
        "PETITION"
    } else if has(&["has come to visit", "has become a citizen", "granted residency"]) {
        "VISITOR"
    } else if has(&["migrants have arrived", "migrant"]) {
        "MIGRANTS"
    } else if has(&["caravan", "merchants"]) {
        "CARAVAN"
    } else if has(&["has created"]) {
        "ARTIFACT"
    } else if has(&["has claimed a", "mysterious construction"]) {
        "MOOD_CLAIM"
    } else if has(&["fey mood", "withdraws from society", "has been possessed", "fell and terrible", "stalk and brood"]) {
        "MOOD"
    } else if has(&["eggs have hatched", "has been tamed", "has adopted", "semi-wild"]) {
        "ANIMALS"
    } else if has(&["has given birth", "has grown up"]) {
        "BIRTH"
    } else if has(&["tantrum", "berserk", "stark raving", "catatonic", "melancholy", "obliviously", "has calmed down", "has toppled", "punches"]) {
        "TANTRUM"
    } else if has(&["accused of", "sentenced", "released from jail", "as punishment"]) {
        "JUSTICE"
    } else if has(&["appointed", "mandate", "export ban", "elected"]) {
        "NOBLES"
    } else if has(&["spirit of", "haunting", "laid to rest"]) {
        "GHOST"
    } else if has(&["collapsed", "cave-in"]) {
        "CAVE_IN"
    } else if has(&["caught in a", "crushed by a drawbridge", "weapon trap"]) {
        "TRAPS"
    } else if has(&["has destroyed a"]) {
        "DESTROYED"
    } else if kind == AnnouncementKind::Job {
        "JOB"
    } else {
        "OTHER"
    };
    ty(id)
}

/// Flags from a DF `announcements.txt` entry (`BOX`/`DO_MEGA`, `P`/`PAUSE`, `R`/`RECENTER`,
/// `D_D`/`D_DISPLAY`).
pub fn flags_from_df(f: &[String]) -> AnnFlags {
    let any = |w: &[&str]| f.iter().any(|x| w.contains(&x.as_str()));
    AnnFlags { popup: any(&["BOX", "DO_MEGA"]), pause: any(&["P", "PAUSE"]), recenter: any(&["R", "RECENTER"]), show: any(&["D_D", "D_DISPLAY"]) }
}

/// Where the current value of a type comes from (settings page).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Src {
    #[default]
    Rustfort,
    /// The player's DF prefs (`prefs/announcements.txt`).
    DfPrefs,
    /// The install's `data/init/announcements.txt`.
    DfDefault,
    /// Changed in Rustfort (saved in settings.json).
    Changed,
}

/// The DF announcement files in priority order: player prefs, install prefs, install defaults.
pub fn df_files(df: &std::path::Path) -> Vec<(std::path::PathBuf, Src)> {
    let mut v = Vec::new();
    if let Some(app) = std::env::var_os("APPDATA") {
        v.push((std::path::PathBuf::from(app).join("Bay 12 Games").join("Dwarf Fortress").join("prefs").join("announcements.txt"), Src::DfPrefs));
    }
    v.push((df.join("prefs").join("announcements.txt"), Src::DfPrefs));
    v.push((df.join("data").join("init").join("announcements.txt"), Src::DfDefault));
    v
}

/// Default flags of every type from DF's files (read-only), with their source.
pub fn df_defaults(texts: &[(String, Src)]) -> Vec<(AnnFlags, Src)> {
    ANN_TYPES
        .iter()
        .map(|t| {
            for (text, src) in texts {
                for tok in t.df {
                    if let Some(f) = super::round4::parse_announcement_flags(text, tok) {
                        return (flags_from_df(&f), *src);
                    }
                }
            }
            (t.default, Src::Rustfort)
        })
        .collect()
}

/// Make sure the per-type flags are resolved (DF files, then the saved changes).
pub fn ensure_ann(g: &mut Game) {
    if g.r9.flags.len() == ANN_TYPES.len() {
        return;
    }
    let texts: Vec<(String, Src)> = df_files(&g.gfx.df_dir).into_iter().filter_map(|(p, s)| std::fs::read_to_string(p).ok().map(|t| (t, s))).collect();
    let base = df_defaults(&texts);
    g.r9.base = base.iter().map(|b| b.0).collect();
    g.r9.flags = Vec::with_capacity(ANN_TYPES.len());
    g.r9.src = Vec::with_capacity(ANN_TYPES.len());
    for (i, t) in ANN_TYPES.iter().enumerate() {
        let (mut f, mut s) = base[i];
        if let Some(o) = g.settings.announcements.iter().find(|o| o.id == t.id) {
            let of = AnnFlags { popup: o.popup, pause: o.pause, recenter: o.recenter, show: o.show };
            if of != f {
                f = of;
                s = Src::Changed;
            }
        }
        g.r9.flags.push(f);
        g.r9.src.push(s);
    }
}

/// Current flags of a type.
pub fn flags(g: &mut Game, t: usize) -> AnnFlags {
    ensure_ann(g);
    g.r9.flags.get(t).copied().unwrap_or_default()
}

pub fn flags_by_id(g: &mut Game, id: &str) -> AnnFlags {
    flags(g, ty(id))
}

/// Change one type and remember it in the settings file.
fn set_flags(g: &mut Game, t: usize, f: AnnFlags) {
    ensure_ann(g);
    g.r9.flags[t] = f;
    let id = ANN_TYPES[t].id;
    g.settings.announcements.retain(|o| o.id != id);
    if f != g.r9.base[t] {
        g.settings.announcements.push(crate::settings::AnnSetting { id: id.into(), popup: f.popup, pause: f.pause, recenter: f.recenter, show: f.show });
        g.r9.src[t] = Src::Changed;
    } else {
        // Back to the DF value.
        g.r9.flags.clear();
        ensure_ann(g);
    }
    g.persist_settings();
}

// ====================================================================== pop-up boxes

/// One DF announcement box.
#[derive(Clone, Debug, Default)]
pub struct Pop {
    pub ty: usize,
    pub title: String,
    pub text: String,
    pub pos: Option<Pos>,
    /// A pending petition answered from the box.
    pub petition: Option<i32>,
    /// More announcements of the same type that arrived while the queue was long.
    pub more: u32,
}

/// A long queue folds further boxes of the same type into the last one.
const QUEUE_FOLD: usize = 4;

pub fn push_pop(g: &mut Game, p: Pop) {
    if g.r9.popups.len() >= QUEUE_FOLD && p.petition.is_none() {
        if let Some(last) = g.r9.popups.iter_mut().rev().find(|q| q.ty == p.ty && q.petition.is_none()) {
            last.more += 1;
            return;
        }
    }
    g.r9.popups.push_back(p);
}

/// Apply a type's flags to one announcement (box, pause, recenter).
fn apply(g: &mut Game, t: usize, text: &str, pos: Option<Pos>) {
    let f = flags(g, t);
    if f.popup {
        g.paused = true;
        push_pop(g, Pop { ty: t, title: ANN_TYPES[t].title.into(), text: text.into(), pos, ..Default::default() });
    } else if f.pause {
        g.paused = true;
    }
    if f.recenter {
        if let Some(p) = pos {
            g.cam.focus(p);
        }
    }
}

/// Called with each batch of sim announcements before they go to the log. Returns per line
/// whether it is shown in the announcement lines (`D_D`).
pub fn on_announcements(g: &mut Game, fresh: &[Announcement]) -> Vec<bool> {
    ensure_ann(g);
    let mut show = Vec::with_capacity(fresh.len());
    for a in fresh {
        let mut t = classify(&a.text, a.kind);
        if ANN_TYPES[t].id == "DEATH" {
            t = death_type(&g.sim.world, &a.text, a.pos);
        }
        let f = g.r9.flags[t];
        show.push(f.show || f.popup);
        if alert_handled(t) {
            // The hostile alert (this frame) uses this line as its title and text.
            if g.r9.raid_hint.is_none() {
                g.r9.raid_hint = Some((t, a.text.clone(), a.pos));
            }
            continue;
        }
        match ANN_TYPES[t].id {
            // The caravan box (with "Trade") comes from `round4::update`.
            "CARAVAN" => {}
            // The petition box needs the petition id: see `update`.
            "PETITION" => {
                g.r9.pet_dirty = true;
                if f.pause && !f.popup {
                    g.paused = true;
                }
            }
            _ => apply(g, t, &a.text, a.pos),
        }
    }
    show
}

/// DF tells citizen deaths (`CITIZEN_DEATH`) from pet deaths (`PET_DEATH`); other deaths
/// (invaders, wildlife) have no box. The sim's line starts with the unit's name.
pub fn death_type(w: &World, text: &str, pos: Option<Pos>) -> usize {
    let named = |u: &&Unit| !u.name.is_empty() && text.starts_with(u.name.as_str());
    let u = w
        .units
        .iter()
        .filter(|u| pos.map(|p| u.pos == p).unwrap_or(true))
        .find(named)
        .or_else(|| w.units.iter().find(named));
    match u {
        Some(u) if u.flags.citizen => ty("DEATH"),
        Some(u) if u.flags.tame && !u.flags.hostile => ty("PET_DEATH"),
        _ => ty("ENEMY_DEATH"),
    }
}

/// The hostile alert's announcement type for a new wave (when the sim sent no raid line).
pub fn alert_type(w: &World, units: &[&Unit]) -> usize {
    if units.iter().any(|u| super::alerts::is_monster(w, u)) {
        return ty("MEGABEAST");
    }
    let people = units.iter().filter(|u| !u.flags.animal).count();
    if people == 0 {
        ty("HOSTILE_ANIMAL")
    } else if people >= 6 {
        ty("SIEGE")
    } else {
        ty("AMBUSH")
    }
}

/// Title of a type's box.
pub fn title_of(t: usize) -> &'static str {
    ANN_TYPES[t].title
}

/// Once per frame after the sim ran (after `update_hostiles`).
pub fn update(g: &mut Game) {
    for a in std::mem::take(&mut g.r9.fake) {
        // `--ui fake-raid` / tests: lines that look like the sim's.
        let show = on_announcements(g, std::slice::from_ref(&a));
        let date = date_string(&g.sim.world.calendar);
        let mut l = LogLine::new(a.text.clone(), a.pos, a.kind, get_time(), date);
        l.quiet = !show[0];
        g.log.push(l);
    }
    // A raid line without new hostiles (spotted thief already listed, a visitor thief): box now.
    if let Some((t, text, pos)) = g.r9.raid_hint.take() {
        apply(g, t, &text, pos);
    }
    update_petitions(g);
    update_fights(g);
}

/// Enter: close the front box / finish the patrol route. True when it did something.
pub fn enter_key(g: &mut Game) -> bool {
    if !g.input.fired.contains(&KeyCode::Enter) {
        return false;
    }
    if g.alert.is_none() && g.r4.popup.is_none() && g.r9.popups.pop_front().is_some() {
        return true;
    }
    if let Tool::Patrol(sq) = g.tool {
        finish_patrol(g, sq);
        return true;
    }
    false
}

/// Esc backs out of round-9 states first. True when it did something.
pub fn escape(g: &mut Game) -> bool {
    if g.alert.is_none() && g.r4.popup.is_none() && g.r9.popups.pop_front().is_some() {
        return true;
    }
    if let Tool::Patrol(_) = g.tool {
        g.tool = Tool::None;
        g.r9.patrol.clear();
        g.screen = Screen::Military;
        return true;
    }
    if g.r9.show_route.take().is_some() {
        return true;
    }
    if g.menu_open && g.menu.page == MenuPage::Announcements {
        g.menu.page = MenuPage::Settings;
        return true;
    }
    if g.screen == Screen::Military && g.r9.eq_pos.is_some() && g.r9.sq_tab == 1 {
        g.r9.eq_pos = None;
        return true;
    }
    false
}

/// Draw the front box (modal; under the hostile alert and the caravan box).
pub fn draw_popups(g: &mut Game) {
    if g.alert.is_some() || g.r4.popup.is_some() {
        return;
    }
    let Some(p) = g.r9.popups.front().cloned() else { return };
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    let maxw = (sw - 60.0).min(760.0 * s);
    let lines = wrap(&p.text, ((maxw - 40.0 * s) / (8.0 * s)) as usize);
    let note = "Paused. Which events open a box: Esc > Settings > Announcements.";
    let tw = lines.iter().map(|l| g.font.width(l, s)).fold(g.font.width(&p.title, 1.5 * s).max(g.font.width(note, s)), f32::max) + 48.0 * s;
    let nbtn = if p.petition.is_some() { 4.0 } else { 3.0 };
    let bw = 15.0 * 8.0 * s;
    let gap = 12.0 * s;
    let w = tw.max(nbtn * bw + (nbtn - 1.0) * gap + 40.0 * s).clamp(420.0 * s, sw - 40.0);
    let extra = if p.more > 0 { lh } else { 0.0 };
    let h = 1.5 * GH * s + 40.0 * s + lines.len() as f32 * lh + extra + 26.0 * s + 2.0 * lh;
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.5, w, h);
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.25));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.04, 0.04, 1.0));
    panel(&mut g.ui, r);
    let danger = matches!(ANN_TYPES[p.ty].id, "DEATH" | "ABDUCTED" | "THIEF" | "SNATCHER" | "AMBUSH" | "SIEGE" | "MEGABEAST" | "TANTRUM");
    if danger {
        draw_rectangle_lines(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, 2.0, Color::from_rgba(200, 40, 30, 255));
    }
    let mut y = r.y + 14.0 * s;
    let tc = if danger { Color::from_rgba(255, 110, 80, 255) } else { col::TEXT_HI };
    g.font.draw(&p.title, r.x + (w - g.font.width(&p.title, 1.5 * s)) / 2.0, y, 1.5 * s, tc);
    let n = g.r9.popups.len();
    if n > 1 {
        let t = format!("1 of {n}");
        g.font.draw(&t, r.x + r.w - g.font.width(&t, s) - 12.0 * s, r.y + 10.0 * s, s, col::TEXT_DIM);
    }
    y += 1.5 * GH * s + 14.0 * s;
    for l in &lines {
        g.font.draw(l, r.x + (w - g.font.width(l, s)) / 2.0, y, s, col::TEXT);
        y += lh;
    }
    if p.more > 0 {
        let t = format!("... and {} more like this (see Reports)", p.more);
        g.font.draw(&t, r.x + (w - g.font.width(&t, s)) / 2.0, y, s, col::TEXT_DIM);
    }
    let note = clip(note, ((w - 20.0 * s) / (8.0 * s)) as usize);
    g.font.draw(&note, r.x + (w - g.font.width(&note, s)) / 2.0, r.y + r.h - 26.0 * s - 14.0 * s - lh - 2.0 * s, s, col::TEXT_DIM);
    let bh = 26.0 * s;
    let by = r.y + r.h - bh - 12.0 * s;
    let mut bx = r.x + (w - nbtn * bw - (nbtn - 1.0) * gap) / 2.0;
    let mut close = false;
    if let Some(pid) = p.petition {
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Accept", BtnState::Normal) {
            answer(g, pid, true);
            close = true;
        }
        bx += bw + gap;
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Deny", BtnState::Normal) {
            answer(g, pid, false);
            close = true;
        }
        bx += bw + gap;
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Petitions", BtnState::Normal) {
            g.r9.pet_sel = Some(pid);
            g.toggle_screen(Screen::None);
            g.screen = Screen::Petitions;
            close = true;
        }
        bx += bw + gap;
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Later (Enter)", BtnState::Normal) {
            close = true;
        }
    } else {
        let st = if p.pos.is_some() { BtnState::Normal } else { BtnState::Disabled };
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Go there", st) {
            if let Some(pos) = p.pos {
                g.cam.focus(pos);
            }
            close = true;
        }
        bx += bw + gap;
        let all = format!("Close all ({n})");
        let st = if n > 1 { BtnState::Normal } else { BtnState::Disabled };
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), &all, st) {
            g.r9.popups.clear();
        }
        bx += bw + gap;
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "OK (Enter)", BtnState::Normal) {
            close = true;
        }
    }
    if close {
        g.r9.popups.pop_front();
    }
    g.ui.block(Rect::new(0.0, 0.0, sw, sh));
    if g.ui.lpressed {
        g.ui.click_used = true;
    }
}

/// Word-wrap at `n` characters.
pub fn wrap(text: &str, n: usize) -> Vec<String> {
    let n = n.max(10);
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > n {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

// ====================================================================== announcement settings

/// Esc menu > Settings > Announcements: DF's announcements.txt as a table.
pub fn draw_ann_settings(g: &mut Game) {
    ensure_ann(g);
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    let (w, h) = ((820.0 * s).min(sw - 40.0), (620.0 * s).min(sh - 60.0));
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    panel(&mut g.ui, r);
    let pad = 16.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    g.font.draw("Announcements", x, y, 1.5 * s, col::TEXT_HI);
    y += 1.5 * GH * s + 8.0 * s;
    let maxc = ((w - 2.0 * pad) / (8.0 * s)) as usize;
    g.font.draw(&clip("Like DF's announcements.txt: Box = pop-up that pauses (BOX), Pause (P), Center (R), Log (D_D).", maxc), x, y, s, col::TEXT_DIM);
    y += lh;
    g.font.draw(&clip("Defaults come from your DF prefs/announcements.txt; changes are saved in saves/settings.json.", maxc), x, y, s, col::TEXT_DIM);
    y += lh + 8.0 * s;
    let cx = [x, x + 0.40 * (w - 2.0 * pad), x + 0.50 * (w - 2.0 * pad), x + 0.60 * (w - 2.0 * pad), x + 0.71 * (w - 2.0 * pad), x + 0.80 * (w - 2.0 * pad)];
    for (i, hd) in ["Event", "Box", "Pause", "Center", "Log", "Default from"].iter().enumerate() {
        g.font.draw(hd, cx[i], y, s, col::BORDER_HI);
    }
    y += lh;
    draw_line(x, y, r.x + r.w - pad, y, 1.0, col::BORDER);
    y += 4.0 * s;
    let row_h = lh + 4.0 * s;
    let area = Rect::new(x, y, w - 2.0 * pad, r.y + r.h - pad - 34.0 * s - y);
    let rows = ((area.h / row_h) as usize).max(1);
    g.r9.ann_scroll = super::round8::scroll(g, area, g.r9.ann_scroll, ANN_TYPES.len(), rows);
    let mut change = None;
    for (i, t) in ANN_TYPES.iter().enumerate().skip(g.r9.ann_scroll).take(rows) {
        if i % 2 == 0 {
            draw_rectangle(x - 4.0 * s, y - 2.0 * s, area.w + 8.0 * s, row_h, Color::new(1.0, 1.0, 1.0, 0.03));
        }
        let f = g.r9.flags[i];
        g.font.draw(&clip(t.label, 36), cx[0], y, s, col::TEXT);
        let mut nf = f;
        if checkbox(&mut g.ui, &g.font, cx[1], y, f.popup, "") {
            nf.popup = !f.popup;
        }
        if checkbox(&mut g.ui, &g.font, cx[2], y, f.pause, "") {
            nf.pause = !f.pause;
        }
        if checkbox(&mut g.ui, &g.font, cx[3], y, f.recenter, "") {
            nf.recenter = !f.recenter;
        }
        if checkbox(&mut g.ui, &g.font, cx[4], y, f.show, "") {
            nf.show = !f.show;
        }
        if nf != f {
            change = Some((i, nf));
        }
        let (src, sc) = match g.r9.src[i] {
            Src::DfPrefs => ("your DF prefs".to_string(), col::TEXT_GOOD),
            Src::DfDefault => ("DF default".to_string(), col::TEXT_DIM),
            Src::Changed => ("changed here".to_string(), col::TEXT_HI),
            Src::Rustfort => ("Rustfort".to_string(), col::TEXT_DIM),
        };
        g.font.draw(&clip(&src, ((r.x + r.w - pad - cx[5]) / (8.0 * s)) as usize), cx[5], y, s, sc);
        let rr = Rect::new(cx[0], y - 2.0 * s, r.x + r.w - pad - cx[0], row_h);
        if g.ui.hovered(rr) && !g.ui.hovered(Rect::new(cx[1], rr.y, cx[5] - cx[1], rr.h)) {
            let toks = if t.df.is_empty() { "no DF token (Rustfort default)".to_string() } else { t.df.join(", ") };
            g.ui.tooltip = Some((format!("{}\nDF: {toks}", t.label), vec2(rr.x, rr.y - 4.0)));
        }
        y += row_h;
    }
    if let Some((i, f)) = change {
        set_flags(g, i, f);
    }
    let bh = 26.0 * s;
    let by = r.y + r.h - pad - bh;
    if text_button(&mut g.ui, &g.font, Rect::new(x, by, 14.0 * 8.0 * s, bh), "Back", BtnState::Normal) {
        g.menu.page = MenuPage::Settings;
    }
    if ANN_TYPES.len() > rows {
        let t = format!("{}-{} of {} (wheel scrolls)", g.r9.ann_scroll + 1, (g.r9.ann_scroll + rows).min(ANN_TYPES.len()), ANN_TYPES.len());
        g.font.draw(&t, r.x + r.w - pad - g.font.width(&t, s), by + 6.0 * s, s, col::TEXT_DIM);
    }
    let changed = !g.settings.announcements.is_empty();
    if text_button(&mut g.ui, &g.font, Rect::new(x + 16.0 * 8.0 * s, by, 24.0 * 8.0 * s, bh), "Reset to DF values", if changed { BtnState::Normal } else { BtnState::Disabled }) {
        g.settings.announcements.clear();
        g.r9.flags.clear();
        ensure_ann(g);
        g.persist_settings();
    }
}

// ====================================================================== petitions

fn update_petitions(g: &mut Game) {
    if g.r9.pet_init && !g.r9.pet_dirty {
        return;
    }
    let ps = g.sim.petitions();
    let first = !g.r9.pet_init;
    g.r9.pet_init = true;
    g.r9.pet_dirty = false;
    g.r9.pet_pending = ps.iter().filter(|p| p.status == "Pending").count();
    for p in &ps {
        if g.r9.pet_seen.contains(&p.id) {
            continue;
        }
        g.r9.pet_seen.push(p.id);
        if first || p.status != "Pending" {
            continue;
        }
        let f = flags_by_id(g, "PETITION");
        if f.popup {
            g.paused = true;
            let text = petition_text(g, p);
            let pos = (p.unit >= 0).then(|| g.sim.world.unit(p.unit).map(|u| u.pos)).flatten();
            push_pop(g, Pop { ty: ty("PETITION"), title: "Petition".into(), text, pos, petition: Some(p.id), more: 0 });
        }
        if f.recenter {
            if let Some(pos) = (p.unit >= 0).then(|| g.sim.world.unit(p.unit).map(|u| u.pos)).flatten() {
                g.cam.focus(pos);
            }
        }
    }
}

fn location_name(w: &World, id: i32) -> String {
    match w.locations.iter().find(|l| l.id == id) {
        Some(l) if !l.name.is_empty() => format!("{} ({})", l.name, l.kind.to_lowercase()),
        Some(l) => format!("{} #{}", l.kind, l.id),
        None => "the fortress".into(),
    }
}

/// "Urist, a performer, asks to stay at The Golden Mug (tavern) as a resident."
pub fn petition_text(g: &Game, p: &Petition) -> String {
    let w = &g.sim.world;
    if p.unit < 0 {
        // A religion or guild asks for a temple / guildhall.
        return format!("{} petition for a {}.", p.name, p.kind.to_lowercase());
    }
    let race = w.unit(p.unit).map(|u| pretty(&u.race).to_lowercase()).unwrap_or_default();
    let what = if p.kind == "Citizenship" { "to join the fortress as a citizen".to_string() } else { format!("to stay at {} as a resident", location_name(w, p.location)) };
    format!("{}, a {race} {}, asks {what}.", if p.name.is_empty() { "A visitor".into() } else { p.name.clone() }, p.role)
}

fn answer(g: &mut Game, id: i32, accept: bool) {
    let ok = g.sim.answer_petition(id, accept);
    g.r9.pet_dirty = true;
    g.set_status(if !ok {
        "That petition is no longer pending".to_string()
    } else if accept {
        "Petition accepted".to_string()
    } else {
        "Petition denied".to_string()
    });
}

/// Left-edge alert badge (DF v50 alert icons): pending petitions.
pub fn draw_badges(g: &mut Game, top: f32) {
    if g.r9.pet_pending == 0 || g.screen == Screen::Petitions {
        return;
    }
    let s = g.ui.scale;
    let t = format!("Petitions ({})", g.r9.pet_pending);
    let r = Rect::new(6.0 * s, top + 4.0 * s, g.font.width(&t, s) + 24.0 * s, GH * s + 12.0 * s);
    let blink = (get_time() * 1.5) as i64 % 2 == 0;
    g.ui.block(r);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.10, 0.08, 0.03, 0.95));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, if blink { col::BORDER_HI } else { col::BORDER });
    g.font.draw("!", r.x + 6.0 * s, r.y + 6.0 * s, s, col::TEXT_HI);
    g.font.draw(&t, r.x + 18.0 * s, r.y + 6.0 * s, s, col::TEXT_HI);
    if g.ui.hovered(r) {
        g.ui.tooltip = Some(("Visitors ask to stay or to join the fortress: click to answer".into(), vec2(r.x, r.y + r.h + 40.0 * s)));
    }
    if g.ui.clicked(r) {
        g.toggle_screen(Screen::Petitions);
    }
}

/// The petitions screen (DF v50 petitions window): list, details, Accept / Deny.
pub fn draw_petitions(g: &mut Game) {
    if g.screen != Screen::Petitions {
        return;
    }
    let Some(c) = super::round8::frame(g, "Petitions", 1000.0) else { return };
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let now = g.sim.ticks();
    let mut ps = g.sim.petitions();
    ps.reverse();
    // Pending first, newest first.
    ps.sort_by_key(|p| p.status != "Pending");
    let mut y = c.y + 2.0 * lh;
    let pending = ps.iter().filter(|p| p.status == "Pending").count();
    let vis = g.sim.visitors().len();
    g.font.draw(&format!("{pending} pending, {} answered; {vis} visitors on the map. Unanswered petitions are denied after 28 days (estimate).", ps.len() - pending), c.x, y, s, col::TEXT_DIM);
    y += lh + 6.0 * s;
    let lw = (c.w * 0.48).floor();
    let row_h = 2.0 * lh + 4.0 * s;
    let list = Rect::new(c.x, y, lw - 8.0 * s, c.y + c.h - y);
    draw_line(c.x + lw, y - 4.0 * s, c.x + lw, c.y + c.h, 1.0, col::BORDER);
    let rows = ((list.h / row_h) as usize).max(1);
    g.r9.pet_scroll = super::round8::scroll(g, list, g.r9.pet_scroll, ps.len(), rows);
    if ps.is_empty() {
        g.font.draw("No petitions yet. Visitors to your taverns, temples and libraries", c.x, y, s, col::TEXT_DIM);
        g.font.draw("petition after 7-21 days.", c.x, y + lh, s, col::TEXT_DIM);
    }
    if g.r9.pet_sel.map(|id| !ps.iter().any(|p| p.id == id)).unwrap_or(true) {
        g.r9.pet_sel = ps.first().map(|p| p.id);
    }
    let maxc = (list.w / (8.0 * s)) as usize;
    let mut pick = None;
    for p in ps.iter().skip(g.r9.pet_scroll).take(rows) {
        let rr = Rect::new(list.x - 4.0 * s, y - 2.0 * s, list.w + 4.0 * s, row_h - 2.0 * s);
        if list_row(&mut g.ui, rr, g.r9.pet_sel == Some(p.id)) {
            pick = Some(p.id);
        }
        let who = if p.name.is_empty() { format!("petition #{}", p.id) } else { p.name.clone() };
        g.font.draw(&clip(&who, maxc.saturating_sub(10)), c.x, y, s, col::TEXT);
        let (st, sc) = status_label(&p.status);
        g.font.draw(st, list.x + list.w - g.font.width(st, s), y, s, sc);
        let age = now.saturating_sub(p.tick) / rf_core::TICKS_PER_DAY as u64;
        let role = if p.unit < 0 { format!("{} guild", pretty(&p.role).to_lowercase()) } else { p.role.clone() };
        let sub = format!("{role} for {} - {} days ago", p.kind.to_lowercase(), age);
        g.font.draw(&clip(&sub, maxc), c.x + 8.0 * s, y + lh, s, col::TEXT_DIM);
        y += row_h;
    }
    if let Some(id) = pick {
        g.r9.pet_sel = Some(id);
    }
    // ---- details
    let Some(p) = g.r9.pet_sel.and_then(|id| ps.iter().find(|p| p.id == id)).cloned() else { return };
    let rx = c.x + lw + 12.0 * s;
    let rw = c.x + c.w - rx;
    let mut y = c.y + 2.0 * lh + lh + 6.0 * s;
    let who = if p.name.is_empty() { format!("Petition #{}", p.id) } else { p.name.clone() };
    g.font.draw(&clip(&who, (rw / (12.0 * s)) as usize), rx, y, 1.5 * s, col::TEXT_HI);
    y += 1.5 * GH * s + 10.0 * s;
    for l in wrap(&petition_text(g, &p), (rw / (8.0 * s)) as usize) {
        g.font.draw(&l, rx, y, s, col::TEXT);
        y += lh;
    }
    y += 6.0 * s;
    let w = &g.sim.world;
    let role = if p.unit < 0 { format!("{} (members in the fortress)", pretty(&p.role).to_lowercase()) } else { p.role.clone() };
    let mut facts: Vec<(String, String)> = vec![("Request".into(), p.kind.clone()), (if p.unit < 0 { "Group" } else { "Visitor role" }.into(), role)];
    if p.unit >= 0 {
        if let Some(u) = w.unit(p.unit) {
            facts.push(("Profession".into(), super::panels::unit_profession(u)));
            let mut sk = u.skills.clone();
            sk.sort_by(|a, b| b.rating.cmp(&a.rating));
            if let Some(k) = sk.first() {
                facts.push(("Best skill".into(), skill_title(k)));
            }
        }
        facts.push(("Location".into(), location_name(w, p.location)));
    }
    let left = (p.tick + rf_sim::locations::PETITION_TIMEOUT).saturating_sub(now) / rf_core::TICKS_PER_DAY as u64;
    let (st, sc) = status_label(&p.status);
    facts.push(("Status".into(), if p.status == "Pending" { format!("{st}, {left} days left to answer") } else { st.to_string() }));
    for (k, v) in &facts {
        g.font.draw(k, rx, y, s, col::TEXT_DIM);
        g.font.draw(&clip(v, ((rw - 15.0 * 8.0 * s) / (8.0 * s)) as usize), rx + 15.0 * 8.0 * s, y, s, if k == "Status" { sc } else { col::TEXT });
        y += lh;
    }
    y += 10.0 * s;
    let bh = 28.0 * s;
    let bw = 16.0 * 8.0 * s;
    let pending = p.status == "Pending";
    let st = if pending { BtnState::Normal } else { BtnState::Disabled };
    if text_button(&mut g.ui, &g.font, Rect::new(rx, y, bw, bh), "Accept", st) {
        answer(g, p.id, true);
    }
    if text_button(&mut g.ui, &g.font, Rect::new(rx + bw + 10.0 * s, y, bw, bh), "Deny", st) {
        answer(g, p.id, false);
    }
    let unit_pos = (p.unit >= 0).then(|| g.sim.world.unit(p.unit).filter(|u| !u.flags.dead).map(|u| (u.id, u.pos))).flatten();
    if let Some((uid, pos)) = unit_pos {
        if text_button(&mut g.ui, &g.font, Rect::new(rx + 2.0 * (bw + 10.0 * s), y, bw, bh), "Show visitor", BtnState::Normal) {
            g.cam.focus(pos);
            g.select_unit(uid);
            g.unit_tab = 0;
            g.screen = Screen::None;
        }
    }
    y += bh + 12.0 * s;
    let notes: &[&str] = if p.unit < 0 {
        &["Accepted: the members are pleased; build the location for them.", "Denied or ignored: the members feel ignored."]
    } else if p.kind == "Citizenship" {
        &["Accepted: joins the fortress as a citizen and takes labors.", "Denied: the visitor is unhappy and leaves within a day."]
    } else {
        &["Accepted: stays as a resident and takes the matching occupation.", "Denied: the visitor is unhappy and leaves within a day."]
    };
    for n in notes {
        for l in wrap(n, (rw / (8.0 * s)) as usize) {
            g.font.draw(&l, rx, y, s, col::TEXT_DIM);
            y += lh;
        }
    }
}

fn status_label(s: &str) -> (&'static str, Color) {
    match s {
        "Pending" => ("Pending", col::TEXT_HI),
        "Accepted" => ("Accepted", col::TEXT_GOOD),
        "Denied" => ("Denied", col::TEXT_BAD),
        "Expired" => ("Expired", col::TEXT_DIM),
        _ => ("?", col::TEXT_DIM),
    }
}

// ====================================================================== squads

pub const SQUAD_TABS: &[&str] = &["Members", "Equipment", "Ammunition", "Patrol and barracks"];

/// Tab row of the squad page. Returns the y below it.
pub fn squad_tabs(g: &mut Game, x: f32, y: f32) -> f32 {
    let s = g.ui.scale;
    let (hit, _) = super::round8::tabs(g, x, y, SQUAD_TABS, g.r9.sq_tab);
    if let Some(i) = hit {
        g.r9.sq_tab = i;
        g.r9.eq_scroll = 0;
        g.mil.picker = None;
        g.mil.kill_list = false;
    }
    y + GH * s + 8.0 * s + 10.0 * s
}

/// Uniform slots of the DF uniform screen: (entry kind, label).
pub const SLOTS: &[(&str, &str)] = &[("ARMOR", "Body"), ("HELM", "Head"), ("PANTS", "Legs"), ("GLOVES", "Hands"), ("SHOES", "Feet"), ("SHIELD", "Shield"), ("WEAPON", "Weapon")];

/// Material choices of the uniform editor (DF: material or material class).
pub const MATERIALS: &[(&str, &str)] = &[("", "any material"), ("METAL", "metal"), ("INORGANIC:STEEL", "steel"), ("INORGANIC:IRON", "iron"), ("INORGANIC:BRONZE", "bronze"), ("INORGANIC:COPPER", "copper"), ("LEATHER", "leather"), ("WOOD", "wood"), ("BONE", "bone")];

/// Split `KIND[:SUBTYPE][@MATERIAL]`.
pub fn split_entry(e: &str) -> (String, String, String) {
    let (head, mat) = e.split_once('@').unwrap_or((e, ""));
    let (k, sub) = head.split_once(':').unwrap_or((head, ""));
    (k.to_string(), sub.to_string(), mat.to_string())
}

pub fn join_entry(kind: &str, sub: &str, mat: &str) -> String {
    let mut e = kind.to_string();
    if !sub.is_empty() {
        e.push(':');
        e.push_str(sub);
    }
    if !mat.is_empty() {
        e.push('@');
        e.push_str(mat);
    }
    e
}

/// "Body: breastplate (steel)", "Weapon: battle axe", "Head: any helm".
pub fn entry_text(w: &World, e: &str) -> String {
    let (k, sub, mat) = split_entry(e);
    let slot = SLOTS.iter().find(|(sk, _)| *sk == k).map(|(_, l)| l.to_string()).unwrap_or_else(|| pretty(&k));
    let item = if sub.is_empty() {
        format!("any {}", if k == "ARMOR" { "body armor".into() } else { pretty(&k).to_lowercase() })
    } else {
        uniform_text(w, &format!("{k}:{sub}")).split_once(": ").map(|(_, n)| n.to_string()).unwrap_or(sub.clone())
    };
    let m = match mat.as_str() {
        "" | "Armor" | "ARMOR" => String::new(),
        m => format!(" ({})", MATERIALS.iter().find(|(t, _)| *t == m).map(|(_, l)| l.to_string()).unwrap_or_else(|| pretty(m.trim_start_matches("INORGANIC:")).to_lowercase())),
    };
    format!("{slot}: {item}{m}")
}

/// Item choices of a slot: "" (none) and "*" (any), then the raws of that kind.
fn slot_choices(w: &World, kind: &str) -> Vec<(String, String)> {
    let mut v = vec![(String::new(), "none".to_string())];
    if kind == "WEAPON" {
        for r in w.weapon_raws.iter().filter(|r| r.kind.is_empty() || r.kind == "WEAPON") {
            v.push((r.token.clone(), if r.name.is_empty() { pretty(&r.token) } else { r.name.clone() }));
        }
    } else {
        v.push(("*".into(), "any".into()));
        for r in w.armor_raws.iter().filter(|r| r.kind == kind) {
            v.push((r.token.clone(), if r.name.is_empty() { pretty(&r.token) } else { r.name.clone() }));
        }
    }
    v
}

/// Effective uniform of a squad position (position override, else the squad's).
fn position_uniform(w: &World, sid: i32, pos: usize) -> (Vec<String>, bool) {
    let Some(q) = w.squads.iter().find(|q| q.id == sid) else { return (Vec::new(), false) };
    match q.position_uniforms.get(pos) {
        Some(v) if !v.is_empty() => (v.clone(), true),
        _ => (q.uniform.clone(), false),
    }
}

/// Draw the selected squad tab (1 equipment, 2 ammunition, 3 patrol and barracks).
pub fn draw_squad_tab(g: &mut Game, sid: i32, area: Rect) {
    match g.r9.sq_tab {
        1 => draw_equipment(g, sid, area),
        2 => draw_ammo(g, sid, area),
        3 => draw_patrol(g, sid, area),
        _ => {}
    }
}

fn draw_equipment(g: &mut Game, sid: i32, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let eq = g.sim.squad_equipment(sid);
    let (x, mut y) = (area.x, area.y);
    // [mil-ui] In the narrow side panel the positions (two columns) sit above the editor.
    let narrow = area.w < 640.0 * s;
    let cols = if narrow { 2 } else { 1 };
    let lw = if narrow { area.w } else { (area.w * 0.42).floor() };
    let cw = (lw / cols as f32).floor();
    let maxc = (cw / (8.0 * s)) as usize;
    g.font.draw("Positions", x, y, s, col::BORDER_HI);
    y += lh;
    if eq.is_empty() {
        g.font.draw("This squad has no positions.", x, y, s, col::TEXT_DIM);
        return;
    }
    if g.r9.eq_pos.map(|p| p >= eq.len()).unwrap_or(true) {
        g.r9.eq_pos = eq.iter().find(|e| e.unit >= 0).map(|e| e.position).or(Some(0));
    }
    let row_h = if narrow { lh + 4.0 * s } else { 2.0 * lh + 2.0 * s };
    let mut pick = None;
    let y0 = y;
    for (k, e) in eq.iter().enumerate() {
        let (x, y) = (x + (k % cols) as f32 * cw, y0 + (k / cols) as f32 * row_h);
        if y + row_h > area.y + area.h {
            break;
        }
        let rr = Rect::new(x - 4.0 * s, y - 2.0 * s, cw - 4.0 * s, row_h - 1.0);
        if list_row(&mut g.ui, rr, g.r9.eq_pos == Some(e.position)) {
            pick = Some(e.position);
        }
        let who = if e.unit >= 0 { g.sim.world.unit(e.unit).map(unit_name).unwrap_or_else(|| format!("unit #{}", e.unit)) } else { "(empty)".into() };
        if narrow {
            // One line: name, then "ok" / "2 missing".
            let (st, sc) = if e.unit < 0 {
                (String::new(), col::TEXT_DIM)
            } else if e.missing.is_empty() {
                ("ok".to_string(), col::TEXT_GOOD)
            } else {
                (format!("{} missing", e.missing.len()), col::TEXT_BAD)
            };
            let nmax = maxc.saturating_sub(st.chars().count() + 2);
            g.font.draw(&clip(&format!("{:>2}. {who}", e.position + 1), nmax), x, y, s, if e.unit >= 0 { col::TEXT } else { col::TEXT_DIM });
            g.font.draw(&st, x + cw - 10.0 * s - g.font.width(&st, s), y, s, sc);
            continue;
        }
        g.font.draw(&clip(&format!("{:>2}. {who}", e.position + 1), maxc), x, y, s, if e.unit >= 0 { col::TEXT } else { col::TEXT_DIM });
        let own = position_uniform(&g.sim.world, sid, e.position).1;
        let (t, c) = if e.unit < 0 {
            ((if own { "own uniform" } else { "squad uniform" }).to_string(), col::TEXT_DIM)
        } else if e.missing.is_empty() {
            (format!("{} {} - complete", e.worn.len(), if own { "own uniform" } else { "squad uniform" }), col::TEXT_GOOD)
        } else {
            (format!("{} missing ({})", e.missing.len(), if own { "own uniform" } else { "squad uniform" }), col::TEXT_BAD)
        };
        g.font.draw(&clip(&t, maxc.saturating_sub(4)), x + 32.0 * s, y + lh, s, c);
    }
    let list_end = y0 + eq.len().div_ceil(cols) as f32 * row_h;
    if let Some(p) = pick {
        g.r9.eq_pos = Some(p);
    }
    // ---- editor of the selected position
    let Some(pos) = g.r9.eq_pos else { return };
    let (rx, mut y) = if narrow { (x, list_end + 8.0 * s) } else { (x + lw + 8.0 * s, area.y) };
    let rw = area.x + area.w - rx;
    let row = eq.iter().find(|e| e.position == pos).cloned().unwrap_or_default();
    let (uni, own) = position_uniform(&g.sim.world, sid, pos);
    let title = format!("Position {} uniform{}", pos + 1, if own { "" } else { " (the squad's)" });
    g.font.draw(&title, rx, y, s, col::BORDER_HI);
    y += lh + 2.0 * s;
    let bw = 3.0 * 8.0 * s;
    let bh = lh;
    let mcol = rx + rw - 13.0 * 8.0 * s;
    let mut new_uni: Option<Vec<String>> = None;
    let w = &g.sim.world;
    let rows: Vec<(usize, &str, &str, Option<String>)> = SLOTS
        .iter()
        .enumerate()
        .map(|(i, (k, l))| (i, *k, *l, uni.iter().find(|e| split_entry(e).0 == *k).cloned()))
        .collect();
    let choices: Vec<Vec<(String, String)>> = SLOTS.iter().map(|(k, _)| slot_choices(w, k)).collect();
    for (i, kind, label, cur) in rows {
        if narrow && y + lh > area.y + area.h {
            break; // [mil-ui] the side panel is full
        }
        let (_, sub, mat) = cur.as_ref().map(|e| split_entry(e)).unwrap_or_default();
        let ch = &choices[i];
        let ci = match &cur {
            None => 0,
            Some(_) if sub.is_empty() => ch.iter().position(|c| c.0 == "*").unwrap_or(0),
            Some(_) => ch.iter().position(|c| c.0 == sub).unwrap_or(0),
        };
        g.font.draw(label, rx, y, s, col::TEXT_DIM);
        let lx = rx + 8.0 * 8.0 * s;
        let mut step = 0i32;
        if text_button(&mut g.ui, &g.font, Rect::new(lx, y - 2.0 * s, bw, bh), "<", BtnState::Normal) {
            step = -1;
        }
        let name = ch.get(ci).map(|c| c.1.clone()).unwrap_or_default();
        let missing = cur.as_ref().map(|e| row.missing.contains(e)).unwrap_or(false) && row.unit >= 0;
        let nc = if cur.is_none() { col::TEXT_DIM } else if missing { col::TEXT_BAD } else { col::TEXT };
        let nmax = ((mcol - lx - 2.0 * bw - 12.0 * s) / (8.0 * s)) as usize;
        g.font.draw(&clip(&format!("{name}{}", if missing { " (missing)" } else { "" }), nmax), lx + bw + 6.0 * s, y, s, nc);
        if text_button(&mut g.ui, &g.font, Rect::new(mcol - bw - 6.0 * s, y - 2.0 * s, bw, bh), ">", BtnState::Normal) {
            step = 1;
        }
        // Material button (cycles).
        let mi = MATERIALS.iter().position(|(t, _)| *t == mat.as_str()).unwrap_or(0);
        let mlabel = if cur.is_none() { "-" } else { MATERIALS[mi].1 };
        let mut mstep = false;
        if text_button(&mut g.ui, &g.font, Rect::new(mcol, y - 2.0 * s, 13.0 * 8.0 * s, bh), mlabel, if cur.is_none() { BtnState::Disabled } else { BtnState::Normal }) {
            mstep = true;
        }
        if step != 0 || mstep {
            let n = ch.len() as i32;
            let ni = if step != 0 { (ci as i32 + step).rem_euclid(n.max(1)) as usize } else { ci };
            let (nsub, _) = ch[ni].clone();
            let nmat = if mstep { MATERIALS[(mi + 1) % MATERIALS.len()].0.to_string() } else { mat.clone() };
            let mut v: Vec<String> = uni.iter().filter(|e| split_entry(e).0 != kind).cloned().collect();
            if !nsub.is_empty() || ni != 0 {
                let sub = if nsub == "*" { String::new() } else { nsub };
                // DF's "any armor": the export writes `KIND@Armor`.
                let m = if nmat.is_empty() && kind != "WEAPON" { "Armor".to_string() } else { nmat };
                v.insert(i.min(v.len()), join_entry(kind, &sub, &m));
            }
            new_uni = Some(v);
        }
        y += lh + 3.0 * s;
    }
    // Entries the editor has no slot for (export oddities) stay as they are.
    for e in uni.iter().filter(|e| !SLOTS.iter().any(|(k, _)| split_entry(e).0 == *k)) {
        g.font.draw(&clip(&entry_text(&g.sim.world, e), (rw / (8.0 * s)) as usize), rx, y, s, col::TEXT_DIM);
        y += lh;
    }
    y += 4.0 * s;
    let bh2 = 24.0 * s;
    let bw2 = 20.0 * 8.0 * s;
    let room = !narrow || y + bh2 <= area.y + area.h; // [mil-ui]
    if room && text_button(&mut g.ui, &g.font, Rect::new(rx, y, bw2, bh2), "Use squad uniform", if own { BtnState::Normal } else { BtnState::Disabled }) {
        g.sim.set_squad_uniform(sid, pos, &[]);
        g.set_status(format!("Position {}: back to the squad uniform", pos + 1));
    }
    if room && text_button(&mut g.ui, &g.font, Rect::new(rx + bw2 + 8.0 * s, y, bw2, bh2), "Copy to all positions", BtnState::Normal) {
        let refs: Vec<&str> = uni.iter().map(|e| e.as_str()).collect();
        let n = eq.len();
        for p in 0..n {
            g.sim.set_squad_uniform(sid, p, &refs);
        }
        g.set_status(format!("{}: all {n} positions use this uniform", squad_name(&g.sim.world, sid)));
    }
    y += bh2 + 8.0 * s;
    if let Some(v) = new_uni {
        let refs: Vec<&str> = v.iter().map(|e| e.as_str()).collect();
        g.sim.set_squad_uniform(sid, pos, &refs);
    }
    // Worn and carried items.
    if row.unit >= 0 && y < area.y + area.h - lh {
        g.font.draw(&format!("Worn and carried ({})", row.worn.len()), rx, y, s, col::BORDER_HI);
        y += lh;
        // [mil-ui] Two columns in the narrow side panel, and the wheel scrolls the list.
        let cols = if narrow { 2 } else { 1 };
        let colw = (rw / cols as f32).floor();
        let vis = (((area.y + area.h - y) / lh) as usize).max(1);
        let total = row.worn.len().div_ceil(cols);
        let wr = Rect::new(rx, y, rw, area.y + area.h - y);
        let step = super::wheel_over(&g.ui, &mut g.input, wr);
        g.r9.eq_scroll = (g.r9.eq_scroll as i32 + step).clamp(0, total.saturating_sub(vis) as i32) as usize;
        let cc = ((colw / (8.0 * s)) as usize).saturating_sub(1);
        for (k, (_, d)) in row.worn.iter().enumerate().skip(g.r9.eq_scroll * cols) {
            let rk = k / cols - g.r9.eq_scroll;
            if rk >= vis {
                break;
            }
            g.font.draw(&clip(d, cc), rx + (k % cols) as f32 * colw, y + rk as f32 * lh, s, col::TEXT);
        }
        y += total.min(vis) as f32 * lh;
        if row.ammo > 0 && y <= area.y + area.h - lh {
            g.font.draw(&format!("{} rounds of ammunition", row.ammo), rx, y, s, col::TEXT_CYAN);
        }
    }
    let _ = y;
}

/// Ammunition items of the save (`weapon_raws` with kind AMMO): (token, name).
fn ammo_raws(w: &World) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = w
        .weapon_raws
        .iter()
        // Generated or odd definitions (tokens with spaces) are not offered.
        .filter(|r| r.kind == "AMMO" && !r.token.contains(' '))
        .map(|r| (r.token.clone(), if r.name_plural.is_empty() { pretty(&r.token) } else { r.name_plural.clone() }))
        .collect();
    if v.is_empty() {
        v.push(("ITEM_AMMO_BOLTS".into(), "bolts".into()));
    }
    v
}

const AMMO_MATS: &[(&str, &str)] = &[("", "any material"), ("WOOD", "wood"), ("BONE", "bone"), ("METAL", "metal"), ("INORGANIC:IRON", "iron"), ("INORGANIC:STEEL", "steel")];

/// "bolts (bone)".
pub fn ammo_text(w: &World, a: &SquadAmmo) -> String {
    let (tok, mat) = a.item.split_once('@').unwrap_or((a.item.as_str(), ""));
    let name = ammo_raws(w).into_iter().find(|(t, _)| t == tok).map(|(_, n)| n).unwrap_or_else(|| pretty(tok.trim_start_matches("ITEM_AMMO_")).to_lowercase());
    let m = AMMO_MATS.iter().find(|(t, _)| *t == mat).map(|(_, l)| l.to_string()).unwrap_or_else(|| pretty(mat.trim_start_matches("INORGANIC:")).to_lowercase());
    if mat.is_empty() {
        name
    } else {
        format!("{name} ({m})")
    }
}

fn draw_ammo(g: &mut Game, sid: i32, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(q) = g.sim.world.squads.iter().find(|q| q.id == sid).cloned() else { return };
    let (x, mut y) = (area.x, area.y);
    // [mil-ui] In the narrow side panel each entry takes two lines.
    let narrow = area.w < 760.0 * s;
    let maxc = (area.w / (8.0 * s)) as usize;
    g.font.draw(&clip("Ammunition (DF squad ammo: item, amount, use in combat / training)", maxc), x, y, s, col::BORDER_HI);
    y += lh + 4.0 * s;
    let mut ammo = q.ammo.clone();
    let mut changed = false;
    if ammo.is_empty() {
        g.font.draw(&clip("No ammunition assigned: marksdwarves take any matching bolts.", maxc), x, y, s, col::TEXT_DIM);
        y += lh;
    }
    let bh = lh;
    let mut remove = None;
    for (i, a) in ammo.iter_mut().enumerate() {
        if y > area.y + area.h - 4.0 * lh {
            break;
        }
        let tok = a.item.split('@').next().unwrap_or("");
        let txt = ammo_raws(&g.sim.world).into_iter().find(|(t, _)| t == tok).map(|(_, n)| n).unwrap_or_else(|| ammo_text(&g.sim.world, a));
        g.font.draw(&clip(&txt, 26), x, y, s, col::TEXT);
        let mut bx = x + 27.0 * 8.0 * s;
        let cur_mat = a.item.split_once('@').map(|(_, m)| m).unwrap_or("");
        let mlabel = AMMO_MATS.iter().find(|(t, _)| *t == cur_mat).map(|(_, l)| *l).unwrap_or("other material");
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y - 2.0 * s, 13.0 * 8.0 * s, bh), mlabel, BtnState::Normal) {
            let (tok, mat) = a.item.split_once('@').map(|(t, m)| (t.to_string(), m.to_string())).unwrap_or((a.item.clone(), String::new()));
            let mi = AMMO_MATS.iter().position(|(t, _)| *t == mat).unwrap_or(0);
            let nm = AMMO_MATS[(mi + 1) % AMMO_MATS.len()].0;
            a.item = if nm.is_empty() { tok } else { format!("{tok}@{nm}") };
            changed = true;
        }
        bx += 14.0 * 8.0 * s;
        if narrow {
            // Remove button on the first line, amounts and flags on the second.
            if text_button(&mut g.ui, &g.font, Rect::new(bx, y - 2.0 * s, 3.0 * 8.0 * s, bh), "x", BtnState::Normal) {
                remove = Some(i);
            }
            y += lh + 4.0 * s;
            bx = x + 2.0 * 8.0 * s;
        }
        for (lbl, d) in [("-100", -100), ("-10", -10)] {
            if text_button(&mut g.ui, &g.font, Rect::new(bx, y - 2.0 * s, 5.0 * 8.0 * s, bh), lbl, BtnState::Normal) {
                a.amount = (a.amount + d).max(0);
                changed = true;
            }
            bx += 5.5 * 8.0 * s;
        }
        let at = format!("{}", a.amount);
        g.font.draw(&at, bx + (6.0 * 8.0 * s - g.font.width(&at, s)) / 2.0, y, s, col::TEXT_HI);
        bx += 6.0 * 8.0 * s;
        for (lbl, d) in [("+10", 10), ("+100", 100)] {
            if text_button(&mut g.ui, &g.font, Rect::new(bx, y - 2.0 * s, 5.0 * 8.0 * s, bh), lbl, BtnState::Normal) {
                a.amount += d;
                changed = true;
            }
            bx += 5.5 * 8.0 * s;
        }
        bx += 4.0 * s;
        if checkbox(&mut g.ui, &g.font, bx, y, a.combat, "combat") {
            a.combat = !a.combat;
            changed = true;
        }
        bx += 10.0 * 8.0 * s;
        if checkbox(&mut g.ui, &g.font, bx, y, a.training, "training") {
            a.training = !a.training;
            changed = true;
        }
        bx += 12.0 * 8.0 * s;
        if !narrow && text_button(&mut g.ui, &g.font, Rect::new(bx, y - 2.0 * s, 3.0 * 8.0 * s, bh), "x", BtnState::Normal) {
            remove = Some(i);
        }
        y += lh + if narrow { 10.0 } else { 4.0 } * s;
    }
    if let Some(i) = remove {
        ammo.remove(i);
        changed = true;
    }
    y += 6.0 * s;
    g.font.draw("Add:", x, y, s, col::TEXT_DIM);
    let mut bx = x + 6.0 * 8.0 * s;
    for (tok, name) in ammo_raws(&g.sim.world) {
        let bw = (name.chars().count() as f32 + 4.0) * 8.0 * s;
        if bx + bw > x + area.w && bx > x + 6.0 * 8.0 * s {
            // [mil-ui] wrap
            bx = x + 6.0 * 8.0 * s;
            y += lh + 4.0 * s;
        }
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y - 2.0 * s, bw, bh), &format!("+ {name}"), BtnState::Normal) {
            // DF's default is the squad's full need; 100 rounds per entry here (estimate).
            ammo.push(SquadAmmo { item: tok.clone(), amount: 100, combat: true, training: true });
            changed = true;
        }
        bx += bw + 6.0 * s;
    }
    y += lh + 10.0 * s;
    if changed {
        g.sim.set_squad_ammo(sid, ammo.clone());
    }
    // Carried ammo per member.
    let eq = g.sim.squad_equipment(sid);
    let members: Vec<(String, i32)> = eq.iter().filter(|e| e.unit >= 0).map(|e| (g.sim.world.unit(e.unit).map(unit_name).unwrap_or_default(), e.ammo)).collect();
    if !members.is_empty() {
        let total: i32 = ammo.iter().map(|a| a.amount).sum();
        let per = (total / members.len() as i32).min(50);
        let t = if total > 0 { format!("Carried now (each soldier keeps up to {per} rounds, a quiver holds 50)") } else { "Carried now".to_string() };
        g.font.draw(&clip(&t, maxc), x, y, s, col::BORDER_HI);
        y += lh;
        let colw = (area.w / 2.0).floor();
        for (k, (n, a)) in members.iter().enumerate() {
            let cx = x + (k % 2) as f32 * colw;
            let cy = y + (k / 2) as f32 * lh;
            if cy > area.y + area.h - lh {
                break;
            }
            let nc = ((colw / (8.0 * s)) as usize).saturating_sub(6).min(26);
            g.font.draw(&clip(n, nc), cx, cy, s, col::TEXT);
            g.font.draw(&format!("{a}"), cx + (nc as f32 + 1.0) * 8.0 * s, cy, s, if *a > 0 { col::TEXT_CYAN } else { col::TEXT_DIM });
        }
    }
}

fn draw_patrol(g: &mut Game, sid: i32, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(q) = g.sim.world.squads.iter().find(|q| q.id == sid).cloned() else { return };
    let (x, mut y) = (area.x, area.y);
    let patrolling = q.orders.iter().any(|o| o == "Patrol");
    let maxc = (area.w / (8.0 * s)) as usize;
    g.font.draw(&clip(&format!("Patrol route ({} waypoints){}", q.patrol.len(), if patrolling { " - patrolling now" } else { "" }), maxc), x, y, s, col::BORDER_HI);
    y += lh + 4.0 * s;
    let bh = 24.0 * s;
    let bw = 26.0 * 8.0 * s;
    // [mil-ui] The squads panel stays open while picking (the map is usable next to it), and
    // the buttons wrap in the narrow panel.
    let wrap = x + bw + 32.0 * s + 40.0 * 8.0 * s > x + area.w;
    if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, bh), "New route (pick on map)", BtnState::Normal) {
        g.r9.patrol.clear();
        g.tool = Tool::Patrol(sid);
        g.set_status(format!("{}: click waypoints, Enter when done, right-click removes the last", squad_name(&g.sim.world, sid)));
        return;
    }
    let has = !q.patrol.is_empty();
    let st = if has { BtnState::Normal } else { BtnState::Disabled };
    let (bx0, by) = if wrap { (x - bw - 8.0 * s, y + bh + 6.0 * s) } else { (x, y) };
    if text_button(&mut g.ui, &g.font, Rect::new(bx0 + bw + 8.0 * s, by, 12.0 * 8.0 * s, bh), "Patrol", if patrolling { BtnState::Active } else { st }) && has {
        g.sim.order_squad_patrol(sid, &q.patrol);
        g.set_status(format!("{}: patrol ({} waypoints)", squad_name(&g.sim.world, sid), q.patrol.len()));
    }
    if text_button(&mut g.ui, &g.font, Rect::new(bx0 + bw + 20.0 * s + 12.0 * 8.0 * s, by, 14.0 * 8.0 * s, bh), "Show route", st) {
        g.r9.show_route = Some(sid);
        if let Some(p) = q.patrol.first() {
            g.cam.focus(*p);
        }
        return;
    }
    if text_button(&mut g.ui, &g.font, Rect::new(bx0 + bw + 32.0 * s + 26.0 * 8.0 * s, by, 14.0 * 8.0 * s, bh), "Clear route", st) {
        g.sim.order_squad_patrol(sid, &[]);
        g.set_status(format!("{}: patrol route cleared", squad_name(&g.sim.world, sid)));
    }
    y = by + bh + 6.0 * s;
    let colw = (area.w / 3.0).floor();
    for (i, p) in q.patrol.iter().enumerate().take(18) {
        let cx = x + (i % 3) as f32 * colw;
        let cy = y + (i / 3) as f32 * lh;
        let rr = Rect::new(cx - 2.0 * s, cy - 2.0 * s, colw - 6.0 * s, lh);
        let hov = g.ui.hovered(rr);
        g.font.draw(&format!("{:>2}. {}, {}, z {}", i + 1, p.x, p.y, p.z), cx, cy, s, if hov { col::TEXT_HI } else { col::TEXT });
        if g.ui.clicked(rr) {
            g.cam.focus(*p);
            g.r9.show_route = Some(sid);
            return; // [mil-ui] the squads panel stays open
        }
    }
    y += ((q.patrol.len().min(18) + 2) / 3) as f32 * lh + 14.0 * s;
    // ---- barracks
    g.font.draw(&clip("Barracks (zones where the squad trains, sleeps on duty and keeps its gear)", maxc), x, y, s, col::BORDER_HI);
    y += lh + 2.0 * s;
    let w = &g.sim.world;
    let mut zones: Vec<(i32, String, i16, i32, bool)> = w
        .buildings
        .iter()
        .filter(|b| b.kind == "Civzone" && (b.subtype == "Barracks" || q.barracks.contains(&b.id)))
        .map(|b| {
            let n = if b.name.is_empty() { format!("{} #{}", super::round8::zone_kind_name(&b.subtype), b.id) } else { b.name.clone() };
            let area = (b.x2 - b.x1 + 1) as i32 * (b.y2 - b.y1 + 1) as i32;
            (b.id, n, b.z, area, q.barracks.contains(&b.id))
        })
        .collect();
    zones.sort_by_key(|z| (!z.4, z.0));
    if zones.is_empty() {
        g.font.draw(&clip("No barracks zones yet: Zones > Barracks (or assign any zone).", maxc), x, y, s, col::TEXT_DIM);
        return;
    }
    let mut toggle = None;
    for (id, name, z, a, on) in &zones {
        if y > area.y + area.h - lh {
            break;
        }
        let nc = if area.w < 640.0 * s { 24 } else { 30 };
        if checkbox(&mut g.ui, &g.font, x, y, *on, &clip(name, nc)) {
            toggle = Some(*id);
        }
        // Other squads using the same barracks.
        let others: Vec<String> = w.squads.iter().filter(|o| o.id != sid && o.barracks.contains(id)).map(|o| squad_name(w, o.id)).collect();
        let info = format!("z {z}, {a} tiles{}", if others.is_empty() { String::new() } else { format!(", also {}", others.join(", ")) });
        let ix = (nc as f32 + 6.0) * 8.0 * s;
        g.font.draw(&clip(&info, ((area.w - ix) / (8.0 * s)) as usize), x + ix, y, s, col::TEXT_DIM);
        y += lh + 3.0 * s;
    }
    if let Some(id) = toggle {
        let mut b = q.barracks.clone();
        if let Some(i) = b.iter().position(|z| *z == id) {
            b.remove(i);
        } else {
            b.push(id);
        }
        g.sim.set_squad_barracks(sid, &b);
    }
}

/// Patrol tool: a map click adds a waypoint.
pub fn patrol_click(g: &mut Game, _sid: i32, p: Pos) {
    g.r9.patrol.push(p);
    g.set_status(format!("Waypoint {} at {}, {}, z {}. Enter: done.", g.r9.patrol.len(), p.x, p.y, p.z));
}

/// Right-click with the patrol tool: remove the last waypoint, or back to the squads.
pub fn patrol_back(g: &mut Game) {
    if g.r9.patrol.pop().is_none() {
        g.tool = Tool::None;
        g.screen = Screen::Military;
    }
}

pub fn finish_patrol(g: &mut Game, sid: i32) {
    let wp = std::mem::take(&mut g.r9.patrol);
    g.tool = Tool::None;
    g.screen = Screen::Military;
    if wp.len() < 2 {
        g.set_status("A patrol route needs at least 2 waypoints".into());
        return;
    }
    g.sim.order_squad_patrol(sid, &wp);
    g.set_status(format!("{}: patrol route with {} waypoints", squad_name(&g.sim.world, sid), wp.len()));
}

pub fn patrol_hint(g: &Game, sid: i32) -> String {
    format!(
        "Patrol route for {}: click waypoints ({} so far). Enter: done. Right-click: remove the last.",
        squad_name(&g.sim.world, sid),
        g.r9.patrol.len()
    )
}

/// Map overlay: the patrol route being picked or shown.
pub fn draw_map_overlay(g: &mut Game) {
    let (pts, live) = match g.tool {
        Tool::Patrol(_) => (g.r9.patrol.clone(), true),
        _ => match g.r9.show_route.and_then(|id| g.sim.world.squads.iter().find(|q| q.id == id)) {
            Some(q) => (q.patrol.clone(), false),
            None => return,
        },
    };
    let z = g.cam.z as i16;
    let tp = g.cam.tile_px();
    let c = Color::from_rgba(90, 200, 255, 230);
    let n = pts.len();
    for i in 0..n {
        let a = pts[i];
        let b = if i + 1 < n { pts[i + 1] } else if !live && n > 1 { pts[0] } else { continue };
        if a.z == z || b.z == z {
            let p1 = g.cam.to_screen(a.x as f32 + 0.5, a.y as f32 + 0.5);
            let p2 = g.cam.to_screen(b.x as f32 + 0.5, b.y as f32 + 0.5);
            draw_line(p1.x, p1.y, p2.x, p2.y, (tp / 14.0).max(1.5), Color::new(c.r, c.g, c.b, if a.z == b.z { 0.9 } else { 0.4 }));
        }
    }
    for (i, p) in pts.iter().enumerate() {
        let sp = g.cam.to_screen(p.x as f32, p.y as f32);
        if p.z == z {
            draw_rectangle_lines(sp.x, sp.y, tp, tp, (tp / 10.0).max(2.0), c);
        }
        let t = format!("{}{}", i + 1, if p.z != z { format!(" z{}", p.z) } else { String::new() });
        g.font.draw(&t, sp.x + 2.0, sp.y + 2.0, (tp / 32.0).max(0.75), if p.z == z { WHITE } else { col::TEXT_DIM });
    }
    if live {
        if let Some(h) = g.hover {
            let sp = g.cam.to_screen(h.x as f32, h.y as f32);
            draw_rectangle_lines(sp.x, sp.y, tp, tp, 1.5, Color::from_rgba(255, 230, 90, 200));
        }
    }
}

// ====================================================================== combat reports

/// One fight (DF combat report): lines that share participants within a short time.
#[derive(Clone, Debug, Default)]
pub struct Fight {
    pub id: u32,
    pub start: u64,
    pub last: u64,
    pub date: String,
    pub lines: Vec<String>,
    /// Named participants (unit id, name, citizen).
    pub who: Vec<(i32, String, bool)>,
    pub pos: Option<Pos>,
    pub deaths: u32,
}

/// A line joins an open fight seen this many ticks ago (estimate: half a day).
pub const FIGHT_GAP: u64 = 600;
const FIGHTS_KEEP: usize = 400;
const DEATH_WORDS: &[&str] = &["struck down", "bled to death", "has been killed", "dies", "is dead", "slain", "has died"];

/// The fight participants named in a line. Names of unnamed creatures are written like the
/// sim writes them ("the goblin lasher") and match in any case.
fn named_in(line: &str, names: &[(i32, String, bool)]) -> Vec<(i32, String, bool)> {
    let mut low = None;
    names
        .iter()
        .filter(|(_, n, _)| {
            if n.starts_with("the ") {
                low.get_or_insert_with(|| line.to_lowercase()).contains(n.as_str())
            } else {
                line.contains(n.as_str())
            }
        })
        .cloned()
        .collect()
}

/// How the sim names a creature in combat lines (`Sim::uname`): its name, else "the goblin"
/// plus a custom profession.
pub fn combat_name(w: &World, u: &Unit) -> String {
    if !u.name.is_empty() {
        return u.name.clone();
    }
    let creature = w
        .creature_raws
        .iter()
        .find(|c| c.token == u.race)
        .map(|c| c.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| u.race.to_ascii_lowercase().replace('_', " "));
    if u.custom_profession.is_empty() {
        format!("the {creature}")
    } else {
        format!("the {creature} {}", u.custom_profession).to_lowercase()
    }
}

/// Group new combat lines into fights (pure; unit-tested).
pub fn group_lines(fights: &mut Vec<Fight>, lines: &[String], tick: u64, date: &str, names: &[(i32, String, bool)], pos_of: &dyn Fn(i32) -> Option<Pos>, next_id: &mut u32) {
    for l in lines {
        let who = named_in(l, names);
        let open = fights.iter().rposition(|f| {
            tick.saturating_sub(f.last) <= FIGHT_GAP && (who.is_empty() || f.who.iter().any(|(id, _, _)| who.iter().any(|w| w.0 == *id)))
        });
        let fi = match open {
            Some(i) => i,
            None => {
                *next_id += 1;
                fights.push(Fight { id: *next_id, start: tick, last: tick, date: date.to_string(), ..Default::default() });
                fights.len() - 1
            }
        };
        let f = &mut fights[fi];
        f.last = tick;
        f.lines.push(l.clone());
        for w in who {
            if !f.who.iter().any(|x| x.0 == w.0) {
                if f.pos.is_none() {
                    f.pos = pos_of(w.0);
                }
                f.who.push(w);
            }
        }
        let low = l.to_lowercase();
        if DEATH_WORDS.iter().any(|d| low.contains(d)) {
            f.deaths += 1;
        }
    }
    if fights.len() > FIGHTS_KEEP {
        let n = fights.len() - FIGHTS_KEEP;
        fights.drain(..n);
    }
}

/// New lines of the combat log since the last poll: the part after the last seen tail.
pub fn new_lines(log: &[String], tail: &[String]) -> usize {
    if tail.is_empty() {
        return 0;
    }
    let k = tail.len();
    let mut i = log.len();
    while i >= k {
        if log[i - k..i] == *tail {
            return i;
        }
        i -= 1;
    }
    0
}

fn update_fights(g: &mut Game) {
    let st = g.sim.combat_stats();
    let key = st.attacks + st.hits + st.misses + st.kills + st.wounds + st.unconscious + st.bled_out + st.syndromes + st.raids_spotted + st.deflected;
    let now = get_time();
    if key == g.r9.cl_key && now - g.r9.cl_at < 2.0 {
        return;
    }
    g.r9.cl_key = key;
    g.r9.cl_at = now;
    let log = g.sim.combat_log();
    if log.is_empty() {
        g.r9.cl_tail.clear();
        return;
    }
    let from = new_lines(&log, &g.r9.cl_tail);
    if from >= log.len() {
        return;
    }
    let k = log.len().min(3);
    g.r9.cl_tail = log[log.len() - k..].to_vec();
    // The participant names are rebuilt at most once a second (or when units arrive).
    let w = &g.sim.world;
    if g.r9.cl_names.is_empty() || now - g.r9.cl_names_at > 1.0 || g.r9.cl_names_n != w.units.len() {
        g.r9.cl_names = fight_names(w);
        g.r9.cl_names_at = now;
        g.r9.cl_names_n = w.units.len();
    }
    let date = date_string(&w.calendar);
    let tick = g.sim.ticks();
    let pos_of = |id: i32| w.unit(id).map(|u| u.pos);
    let mut fights = std::mem::take(&mut g.r9.fights);
    let mut next = g.r9.next_fight;
    group_lines(&mut fights, &log[from..], tick, &date, &g.r9.cl_names, &pos_of, &mut next);
    g.r9.fights = fights;
    g.r9.next_fight = next;
}

/// Fight participants: citizens by name, then everybody else (hostiles, wildlife, pets) by the
/// name the sim writes, longest first.
fn fight_names(w: &World) -> Vec<(i32, String, bool)> {
    let mut names: Vec<(i32, String, bool)> = w.citizens().filter(|u| !u.name.is_empty()).map(|u| (u.id, u.name.clone(), true)).collect();
    // Everybody else on the map (hostiles, also ones killed this frame, wildlife, pets); unnamed
    // ones of the same kind ("the goblin") count as one participant.
    for u in w.units.iter().filter(|u| !u.flags.citizen && !u.flags.inactive) {
        let n = combat_name(w, u);
        if n.len() > 4 && !names.iter().any(|x| x.1 == n) {
            names.push((u.id, n, false));
        }
    }
    names.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    names
}

/// "Urist McAxe, Kib Ustuth vs Snang Skulkspider" (citizens first).
pub fn fight_title(f: &Fight) -> String {
    let ours: Vec<&str> = f.who.iter().filter(|w| w.2).map(|w| w.1.as_str()).collect();
    let theirs: Vec<&str> = f.who.iter().filter(|w| !w.2).map(|w| w.1.as_str()).collect();
    let list = |v: &[&str]| {
        if v.len() > 2 {
            format!("{}, {} and {} more", v[0], v[1], v.len() - 2)
        } else {
            v.join(", ")
        }
    };
    match (ours.is_empty(), theirs.is_empty()) {
        (false, false) => format!("{} vs {}", list(&ours), list(&theirs)),
        (false, true) => format!("{} fighting", list(&ours)),
        (true, false) => format!("{} attacking", list(&theirs)),
        (true, true) => f.lines.first().map(|l| clip(l, 48)).unwrap_or_default(),
    }
}

/// Reports > Combat: fights newest first; the selected fight's lines on the right.
pub fn draw_fights(g: &mut Game, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let n = g.r9.fights.len();
    let (x, mut y) = (area.x, area.y);
    if n == 0 {
        g.font.draw("No fights yet. Each fight collects the combat lines of the creatures involved.", x, y, s, col::TEXT_DIM);
        return;
    }
    let lw = (area.w * 0.44).floor();
    let row_h = 2.0 * lh + 4.0 * s;
    let list = Rect::new(x, y, lw - 8.0 * s, area.h);
    draw_line(x + lw, y - 4.0 * s, x + lw, area.y + area.h, 1.0, col::BORDER);
    let rows = ((list.h / row_h) as usize).max(1);
    g.r9.fight_scroll = super::round8::scroll(g, list, g.r9.fight_scroll, n, rows);
    if g.r9.fight_sel.map(|id| !g.r9.fights.iter().any(|f| f.id == id)).unwrap_or(true) {
        g.r9.fight_sel = g.r9.fights.last().map(|f| f.id);
    }
    let maxc = (list.w / (8.0 * s)) as usize;
    let mut pick = None;
    let fights: Vec<Fight> = g.r9.fights.iter().rev().skip(g.r9.fight_scroll).take(rows).cloned().collect();
    for f in &fights {
        let rr = Rect::new(x - 4.0 * s, y - 2.0 * s, list.w + 4.0 * s, row_h - 2.0 * s);
        if list_row(&mut g.ui, rr, g.r9.fight_sel == Some(f.id)) {
            pick = Some(f.id);
        }
        g.font.draw(&clip(&fight_title(f), maxc), x, y, s, if f.deaths > 0 { Color::from_rgba(255, 130, 110, 255) } else { col::TEXT });
        let sub = format!("{} - {} lines{}", f.date, f.lines.len(), if f.deaths > 0 { format!(", {} dead", f.deaths) } else { String::new() });
        g.font.draw(&clip(&sub, maxc), x + 8.0 * s, y + lh, s, col::TEXT_DIM);
        y += row_h;
    }
    if let Some(id) = pick {
        g.r9.fight_sel = Some(id);
        g.r9.fight_lscroll = 0;
    }
    let Some(f) = g.r9.fight_sel.and_then(|id| g.r9.fights.iter().find(|f| f.id == id)).cloned() else { return };
    let rx = x + lw + 12.0 * s;
    let rw = area.x + area.w - rx;
    let mut y = area.y;
    g.font.draw(&clip(&fight_title(&f), (rw / (8.0 * s)) as usize), rx, y, s, col::TEXT_HI);
    y += lh;
    let mins = f.last.saturating_sub(f.start) * 24 * 60 / rf_core::TICKS_PER_DAY as u64;
    g.font.draw(&format!("{}, about {} min of game time, {} lines", f.date, mins, f.lines.len()), rx, y, s, col::TEXT_DIM);
    if let Some(p) = f.pos {
        let br = Rect::new(rx + rw - 14.0 * 8.0 * s, y - 4.0 * s, 14.0 * 8.0 * s, lh + 2.0 * s);
        if text_button(&mut g.ui, &g.font, br, "Go there", BtnState::Normal) {
            g.cam.focus(p);
            g.screen = Screen::None;
            return;
        }
    }
    y += lh + 6.0 * s;
    let la = Rect::new(rx, y, rw, area.y + area.h - y);
    let maxc = (rw / (8.0 * s)) as usize;
    let wrapped: Vec<(String, bool)> = f
        .lines
        .iter()
        .flat_map(|l| {
            let dead = DEATH_WORDS.iter().any(|d| l.to_lowercase().contains(d));
            wrap(l, maxc).into_iter().enumerate().map(move |(i, w)| (if i == 0 { w } else { format!("  {w}") }, dead))
        })
        .collect();
    let rows = ((la.h / lh) as usize).max(1);
    g.r9.fight_lscroll = super::round8::scroll(g, la, g.r9.fight_lscroll, wrapped.len(), rows);
    for (l, dead) in wrapped.iter().skip(g.r9.fight_lscroll).take(rows) {
        g.font.draw(l, rx, y, s, if *dead { col::TEXT_BAD } else { Color::from_rgba(255, 170, 140, 255) });
        y += lh;
    }
}

// ====================================================================== skills

/// DF skill names (the profession-style nouns of the unit sheet, DF wiki "Skill").
pub fn skill_noun(id: &str) -> String {
    let n = match id {
        "MINING" => "Miner",
        "WOODCUTTING" => "Wood Cutter",
        "CARPENTRY" => "Carpenter",
        "DETAILSTONE" | "ENGRAVE_STONE" => "Engraver",
        "MASONRY" => "Mason",
        "CUT_STONE" => "Stone Cutter",
        "CARVE_STONE" => "Stone Carver",
        "ANIMALTRAIN" => "Animal Trainer",
        "ANIMALCARE" => "Animal Caretaker",
        "DISSECT_FISH" => "Fish Dissector",
        "DISSECT_VERMIN" => "Animal Dissector",
        "PROCESSFISH" => "Fish Cleaner",
        "BUTCHER" => "Butcher",
        "TRAPPING" => "Trapper",
        "TANNER" => "Tanner",
        "WEAVING" => "Weaver",
        "BREWING" => "Brewer",
        "ALCHEMY" => "Alchemist",
        "CLOTHESMAKING" => "Clothier",
        "MILLING" => "Miller",
        "PROCESSPLANTS" => "Thresher",
        "CHEESEMAKING" => "Cheese Maker",
        "MILK" => "Milker",
        "COOK" => "Cook",
        "PLANT" => "Grower",
        "HERBALISM" => "Herbalist",
        "FISH" => "Fisherdwarf",
        "SMELT" => "Furnace Operator",
        "EXTRACT_STRAND" => "Strand Extractor",
        "FORGE_WEAPON" => "Weaponsmith",
        "FORGE_ARMOR" => "Armorsmith",
        "FORGE_FURNITURE" => "Blacksmith",
        "CUTGEM" => "Gem Cutter",
        "ENCRUSTGEM" => "Gem Setter",
        "WOODCRAFT" => "Wood Crafter",
        "STONECRAFT" => "Stone Crafter",
        "METALCRAFT" => "Metal Crafter",
        "GLASSMAKER" => "Glassmaker",
        "LEATHERWORK" => "Leatherworker",
        "BONECARVE" => "Bone Carver",
        "AXE" => "Axe Dwarf",
        "SWORD" => "Swordsdwarf",
        "DAGGER" => "Knife User",
        "MACE" => "Mace Dwarf",
        "HAMMER" => "Hammer Dwarf",
        "SPEAR" => "Spear Dwarf",
        "CROSSBOW" => "Crossbow Dwarf",
        "SHIELD" => "Shield User",
        "ARMOR" => "Armor User",
        "SIEGECRAFT" => "Siege Engineer",
        "SIEGEOPERATE" => "Siege Operator",
        "BOWYER" => "Bowyer",
        "PIKE" => "Pike Dwarf",
        "WHIP" => "Lasher",
        "BOW" => "Bow Dwarf",
        "BLOWGUN" => "Blowgunner",
        "THROW" => "Thrower",
        "MECHANICS" => "Mechanic",
        "MAGIC_NATURE" => "Druid",
        "SNEAK" => "Ambusher",
        "DESIGNBUILDING" => "Building Designer",
        "DRESS_WOUNDS" => "Wound Dresser",
        "DIAGNOSE" => "Diagnostician",
        "SURGERY" => "Surgeon",
        "SET_BONE" => "Bone Doctor",
        "SUTURE" => "Suturer",
        "CRUTCH_WALK" => "Crutch-walker",
        "WOOD_BURNING" => "Wood Burner",
        "LYE_MAKING" => "Lye Maker",
        "SOAP_MAKING" => "Soaper",
        "POTASH_MAKING" => "Potash Maker",
        "DYER" => "Dyer",
        "OPERATE_PUMP" => "Pump Operator",
        "SWIMMING" => "Swimmer",
        "PERSUASION" => "Persuader",
        "NEGOTIATION" => "Negotiator",
        "JUDGING_INTENT" => "Judge of Intent",
        "APPRAISAL" => "Appraiser",
        "ORGANIZATION" => "Organizer",
        "RECORD_KEEPING" => "Record Keeper",
        "LYING" => "Liar",
        "INTIMIDATION" => "Intimidator",
        "CONVERSATION" => "Conversationalist",
        "COMEDY" => "Comedian",
        "FLATTERY" => "Flatterer",
        "CONSOLE" => "Consoler",
        "PACIFY" => "Pacifier",
        "TRACKING" => "Tracker",
        "KNOWLEDGE_ACQUISITION" => "Student",
        "CONCENTRATION" => "Concentration",
        "DISCIPLINE" => "Discipline",
        "SITUATIONAL_AWARENESS" => "Observer",
        "WRITING" => "Writer",
        "PROSE" => "Wordsmith",
        "POETRY" => "Poet",
        "READING" => "Reader",
        "SPEAKING" => "Speaker",
        "COORDINATION" => "Coordination",
        "BALANCE" => "Balance",
        "LEADERSHIP" => "Leader",
        "TEACHING" => "Teacher",
        "MELEE_COMBAT" => "Fighter",
        "RANGED_COMBAT" => "Archer",
        "WRESTLING" => "Wrestler",
        "BITE" => "Biter",
        "GRASP_STRIKE" => "Striker",
        "STANCE_STRIKE" => "Kicker",
        "DODGING" => "Dodger",
        "MISC_WEAPON" => "Misc. Object User",
        "KNAPPING" => "Knapper",
        "MILITARY_TACTICS" => "Military Tactics",
        "SHEARING" => "Shearer",
        "SPINNING" => "Spinner",
        "POTTERY" => "Potter",
        "GLAZING" => "Glazer",
        "PRESSING" => "Presser",
        "BEEKEEPING" => "Bee Keeper",
        "WAX_WORKING" => "Wax Worker",
        "CLIMBING" => "Climber",
        "GELD" => "Gelder",
        "DANCE" => "Dancer",
        "MAKE_MUSIC" => "Musician",
        "SING_MUSIC" => "Singer",
        "PLAY_KEYBOARD_INSTRUMENT" => "Keyboardist",
        "PLAY_STRINGED_INSTRUMENT" => "Stringed Instrumentalist",
        "PLAY_WIND_INSTRUMENT" => "Wind Instrumentalist",
        "PLAY_PERCUSSION_INSTRUMENT" => "Percussionist",
        "CRITICAL_THINKING" => "Critical Thinker",
        "LOGIC" => "Logician",
        "MATHEMATICS" => "Mathematician",
        "ASTRONOMY" => "Astronomer",
        "CHEMISTRY" => "Chemist",
        "GEOGRAPHY" => "Geographer",
        "OPTICS_ENGINEER" => "Optics Engineer",
        "FLUID_ENGINEER" => "Fluid Engineer",
        "PAPERMAKING" => "Papermaker",
        "BOOKBINDING" => "Bookbinder",
        "INTRIGUE" => "Intriguer",
        "RIDING" => "Rider",
        _ => return pretty(id),
    };
    n.to_string()
}

/// DF level names, with Legendary +1..+5 above 15.
pub fn level_name(r: i32) -> String {
    if r > 15 {
        format!("Legendary +{}", (r - 15).min(5))
    } else {
        super::skill_level_name(r).to_string()
    }
}

/// "Legendary Miner".
pub fn skill_title(k: &rf_core::Skill) -> String {
    format!("{} {}", level_name(k.rating), skill_noun(&k.id))
}

/// Experience needed for the next level (DF: 500 + 100 x level).
pub fn xp_needed(rating: i32) -> i32 {
    500 + 100 * rating.max(0)
}

/// Skills tab of the unit sheet: DF names, level, experience bar towards the next level.
pub fn draw_skills(g: &mut Game, u: &Unit, x: f32, y0: f32, w: f32, bottom: f32) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let mut sk = u.skills.clone();
    sk.sort_by(|a, b| b.rating.cmp(&a.rating).then(b.xp.cmp(&a.xp)).then(a.id.cmp(&b.id)));
    let mut y = y0;
    if sk.is_empty() {
        g.font.draw("No skills.", x, y, s, col::TEXT_DIM);
        return;
    }
    let row_h = lh + 3.0 * s;
    let area = Rect::new(x, y, w, bottom - y);
    let rows = ((area.h / row_h) as usize).max(1);
    g.unit_scroll = super::round8::scroll(g, area, g.unit_scroll, sk.len(), rows);
    let bar_w = (w * 0.30).floor();
    let bx = x + w - bar_w;
    let maxc = ((bx - x - 6.0 * s) / (8.0 * s)) as usize;
    for k in sk.iter().skip(g.unit_scroll).take(rows) {
        let title = skill_title(k);
        let lc = if k.rating >= 15 {
            col::TEXT_HI
        } else if k.rating >= 10 {
            Color::from_rgba(240, 210, 120, 255)
        } else if k.rating >= 5 {
            col::TEXT_GOOD
        } else {
            col::TEXT
        };
        g.font.draw(&clip(&title, maxc), x, y, s, lc);
        let need = xp_needed(k.rating);
        let frac = if k.rating >= 20 { 1.0 } else { (k.xp as f32 / need as f32).clamp(0.0, 1.0) };
        let bh = GH * s - 4.0 * s;
        let rr = Rect::new(bx, y + 2.0 * s, bar_w, bh);
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.12, 0.12, 0.14, 1.0));
        draw_rectangle(rr.x, rr.y, rr.w * frac, rr.h, Color::new(0.30, 0.55, 0.85, 1.0));
        draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, Color::new(0.35, 0.35, 0.38, 1.0));
        let hr = Rect::new(x, y - 2.0 * s, w, row_h);
        if g.ui.hovered(hr) {
            let next = if k.rating >= 20 { "maximum level".to_string() } else { format!("{} / {need} experience to {}", k.xp, level_name(k.rating + 1)) };
            g.ui.tooltip = Some((format!("{} ({}), level {}\n{next}", skill_noun(&k.id), k.id, k.rating), vec2(hr.x, hr.y - 4.0)));
        }
        y += row_h;
    }
    if sk.len() > rows {
        let t = format!("{}-{} of {} (wheel)", g.unit_scroll + 1, (g.unit_scroll + rows).min(sk.len()), sk.len());
        g.font.draw(&t, x, bottom - lh + 4.0 * s, s, col::TEXT_DIM);
    }
}

// ====================================================================== state and hooks

#[derive(Default)]
pub struct R9Ui {
    // ---- announcements
    /// Resolved flags per `ANN_TYPES` entry (empty = not loaded yet), their DF base and source.
    pub flags: Vec<AnnFlags>,
    pub base: Vec<AnnFlags>,
    pub src: Vec<Src>,
    pub ann_scroll: usize,
    pub popups: VecDeque<Pop>,
    /// The sim's raid line of this frame (type, text, position) for the hostile alert.
    pub raid_hint: Option<(usize, String, Option<Pos>)>,
    /// Test lines fed through the announcement path next frame (`--ui fake-raid`).
    pub fake: Vec<Announcement>,
    // ---- petitions
    pub pet_init: bool,
    pub pet_dirty: bool,
    pub pet_seen: Vec<i32>,
    pub pet_pending: usize,
    pub pet_sel: Option<i32>,
    pub pet_scroll: usize,
    // ---- squads
    pub sq_tab: usize,
    pub eq_pos: Option<usize>,
    pub eq_scroll: usize,
    /// Waypoints of the patrol route being picked.
    pub patrol: Vec<Pos>,
    /// Squad whose patrol route is drawn on the map.
    pub show_route: Option<i32>,
    // ---- combat reports
    pub fights: Vec<Fight>,
    pub next_fight: u32,
    pub cl_tail: Vec<String>,
    pub cl_key: u64,
    pub cl_at: f64,
    /// Cached participant names, when they were built and the unit count then.
    pub cl_names: Vec<(i32, String, bool)>,
    pub cl_names_at: f64,
    pub cl_names_n: usize,
    pub fight_sel: Option<u32>,
    pub fight_scroll: usize,
    pub fight_lscroll: usize,
}

/// `--ui` states of round 9 (screenshots/tests). Returns false for unknown keys.
pub fn ui_arg(g: &mut Game, k: &str, v: &str) -> bool {
    match k {
        "petitions" => {
            g.toggle_screen(Screen::None);
            g.screen = Screen::Petitions;
        }
        "fake-petition" => fake_petition(g, v),
        "fake-raid" => {
            let text = match v {
                "snatcher" => "Snatcher! Protect the children!",
                "ambush" => "An ambush! Curse them!",
                "abducted" => "Urist McTest has been abducted!",
                "death" => "Urist McTest has been struck down.",
                "mood" => "Urist McTest is taken by a fey mood!",
                _ => "Thief! Protect the hoard from skulking filth!",
            };
            let kind = if matches!(v, "abducted" | "death") { AnnouncementKind::Death } else { AnnouncementKind::Warning };
            let c = g.cam;
            let pos = Some(Pos::new(c.x as i16, c.y as i16, c.z as i16));
            g.r9.fake.push(Announcement { tick: g.sim.ticks(), text: text.into(), pos, kind });
        }
        "force-raid" => {
            // A real raid through the sim (the boxes then come from its announcements).
            let kind = match v {
                "thief" => rf_sim::RaidKind::Thief,
                "snatcher" => rf_sim::RaidKind::Snatcher,
                "ambush" => rf_sim::RaidKind::Ambush,
                _ => rf_sim::RaidKind::Siege,
            };
            let n = g.sim.force_raid(kind, 0, 0);
            eprintln!("force-raid: {n} raiders arrived");
        }
        "announcements" => {
            g.menu_open = true;
            g.menu.page = MenuPage::Announcements;
        }
        "squad-tab" => {
            g.screen = Screen::Military;
            if g.mil.squad.is_none() {
                g.mil.squad = g.sim.world.squads.first().map(|s| s.id);
            }
            g.r9.sq_tab = v.parse::<usize>().unwrap_or(1).min(SQUAD_TABS.len() - 1);
        }
        "eq-pos" => g.r9.eq_pos = v.parse().ok(),
        "patrol-tool" => {
            // A made-up route around the camera (screenshots).
            let sq = g.mil.squad.or_else(|| g.sim.world.squads.first().map(|s| s.id));
            if let Some(sq) = sq {
                let c = g.cam;
                let (x, y, z) = (c.x as i16, c.y as i16, c.z as i16);
                g.r9.patrol = vec![Pos::new(x - 6, y - 3, z), Pos::new(x + 5, y - 3, z), Pos::new(x + 5, y + 4, z)];
                g.tool = Tool::Patrol(sq);
                g.screen = Screen::None;
            }
        }
        "fights" => {
            g.screen = Screen::Reports;
            g.reports.filter = super::reports::FIGHTS_TAB;
        }
        "fake-fights" => fake_fights(g),
        _ => return false,
    }
    true
}

/// `--ui fake-petition[:role]`: a visitor arrives at the first location and the sim runs until
/// it petitions (at most 40 days), so the petition box and screen can be tested.
fn fake_petition(g: &mut Game, v: &str) {
    let role = match v {
        "mercenary" => rf_sim::VisitorRole::Mercenary,
        "scholar" => rf_sim::VisitorRole::Scholar,
        "slayer" => rf_sim::VisitorRole::MonsterSlayer,
        _ => rf_sim::VisitorRole::Performer,
    };
    g.sim.society2_ensure();
    let want = if role == rf_sim::VisitorRole::Scholar { "Library" } else { "Tavern" };
    let loc = g.sim.world.locations.iter().find(|l| l.kind == want).or_else(|| g.sim.world.locations.first()).map(|l| l.id);
    let Some(loc) = loc else {
        eprintln!("fake-petition: no locations in this fort");
        return;
    };
    let Some(vid) = g.sim.spawn_visitor_for(role, loc) else {
        eprintln!("fake-petition: no visitor could arrive");
        return;
    };
    for _ in 0..40 {
        g.sim.run_ticks(rf_core::TICKS_PER_DAY as u32);
        if g.sim.petitions().iter().any(|p| p.unit == vid) {
            break;
        }
    }
    // Only the petition: the other lines of those days stay in the log without boxes.
    g.r9.pet_init = true;
    g.r9.pet_dirty = true;
}

/// `--ui fake-fights`: two made-up fights with the fort's own names (layout tests).
fn fake_fights(g: &mut Game) {
    let w = &g.sim.world;
    let cit: Vec<(i32, String, bool)> = w.citizens().filter(|u| !u.name.is_empty()).take(3).map(|u| (u.id, u.name.clone(), true)).collect();
    if cit.len() < 2 {
        return;
    }
    let date = date_string(&w.calendar);
    let mut cit = cit;
    cit.push((-5, "the goblin lasher".into(), false));
    cit.push((-6, "the kobold thief".into(), false));
    let a = &cit[0].1;
    let b = &cit[1].1;
    let lines = vec![
        format!("{a} hacks the goblin lasher in the upper body with her steel battle axe, tearing the muscle!"),
        format!("The goblin lasher lashes {a} in the left hand with his leather whip, bruising the muscle!"),
        format!("{a} blocks the goblin lasher's attack with her shield!"),
        format!("{a} hacks the goblin lasher in the head, tearing apart the skull and the brain!"),
        "The goblin lasher has been struck down.".to_string(),
    ];
    let lines2 = vec![format!("The kobold thief misses {b}!"), format!("{b} punches the kobold thief in the head, bruising the muscle!")];
    let pos_of = |id: i32| w.unit(id).map(|u| u.pos);
    let mut fights = std::mem::take(&mut g.r9.fights);
    let mut next = g.r9.next_fight;
    group_lines(&mut fights, &lines, 1000, &date, &cit, &pos_of, &mut next);
    group_lines(&mut fights, &lines2, 5000, &date, &cit, &pos_of, &mut next);
    g.r9.fights = fights;
    g.r9.next_fight = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_sim_lines() {
        let w = AnnouncementKind::Warning;
        let i = AnnouncementKind::Info;
        assert_eq!(ANN_TYPES[classify("Thief! Protect the hoard from skulking filth!", w)].id, "THIEF");
        assert_eq!(ANN_TYPES[classify("Snatcher! Protect the children!", w)].id, "SNATCHER");
        assert_eq!(ANN_TYPES[classify("An ambush! Curse them!", w)].id, "AMBUSH");
        assert_eq!(ANN_TYPES[classify("A vile force of darkness has arrived!", w)].id, "SIEGE");
        assert_eq!(ANN_TYPES[classify("Urist has been abducted!", AnnouncementKind::Death)].id, "ABDUCTED");
        assert_eq!(ANN_TYPES[classify("Urist has been struck down.", AnnouncementKind::Death)].id, "DEATH");
        let mut wd = World::synthetic(8, 8, 4);
        let cit = wd.units.iter().find(|u| u.flags.citizen).map(|u| u.name.clone()).unwrap();
        wd.units.push(Unit { id: 9001, name: "Smunstu Monstrousgazes".into(), race: "GOBLIN".into(), ..Default::default() });
        assert_eq!(ANN_TYPES[death_type(&wd, &format!("{cit} has been struck down."), None)].id, "DEATH");
        assert_eq!(ANN_TYPES[death_type(&wd, "Smunstu Monstrousgazes has been struck down.", None)].id, "ENEMY_DEATH");
        assert_eq!(ANN_TYPES[death_type(&wd, "The goblin has bled to death.", None)].id, "ENEMY_DEATH");
        assert_eq!(ANN_TYPES[classify("The Forgotten Beast Ngokang has come! A huge spider.", w)].id, "MEGABEAST");
        assert_eq!(ANN_TYPES[classify("Ber has petitioned for residency.", i)].id, "PETITION");
        assert_eq!(ANN_TYPES[classify("Some migrants have arrived.", i)].id, "MIGRANTS");
        assert_eq!(ANN_TYPES[classify("A caravan from Kogan has arrived.", i)].id, "CARAVAN");
        assert_eq!(ANN_TYPES[classify("Urist is taken by a fey mood!", w)].id, "MOOD");
        assert_eq!(ANN_TYPES[classify("Urist has created Ablelgoden, a steel axe!", i)].id, "ARTIFACT");
        assert_eq!(ANN_TYPES[classify("Urist cancels Dig: Could not find path.", AnnouncementKind::Job)].id, "JOB");
        assert_eq!(ANN_TYPES[classify("The goblins are retreating!", i)].id, "INVASION_END");
        assert_eq!(ANN_TYPES[classify("Saved the game.", i)].id, "OTHER");
        assert_eq!(ANN_TYPES.last().unwrap().id, "OTHER");
    }

    #[test]
    fn df_flags_and_defaults() {
        let f = flags_from_df(&["BOX".into(), "A_D".into(), "D_D".into(), "ALERT".into()]);
        assert_eq!(f, AnnFlags { popup: true, pause: false, recenter: false, show: true });
        let f = flags_from_df(&["P".into(), "R".into()]);
        assert!(f.pause && f.recenter && !f.popup && !f.show);
        // Gordon's prefs: thieves and deaths pop up; the install default does not.
        let prefs = "[AMBUSH_THIEF:BOX:A_D:D_D:ALERT]\n[CITIZEN_DEATH:BOX:A_D:D_D:UCR_A:ALERT]\n".to_string();
        let init = "[AMBUSH_THIEF:A_D:D_D:ALERT]\n[MOOD_BUILDING_CLAIMED:A_D:D_D]\n".to_string();
        let d = df_defaults(&[(prefs, Src::DfPrefs), (init, Src::DfDefault)]);
        let thief = d[ty("THIEF")];
        assert!(thief.0.popup && thief.1 == Src::DfPrefs);
        assert!(d[ty("DEATH")].0.popup);
        assert_eq!(d[ty("MOOD_CLAIM")].1, Src::DfDefault);
        assert_eq!(d[ty("SIEGE")], (ANN_TYPES[ty("SIEGE")].default, Src::Rustfort));
    }

    #[test]
    fn fights_group_by_participants_and_time() {
        let names = vec![(1, "Urist McAxe".to_string(), true), (2, "Kib Ustuth".to_string(), true), (9, "Snang".to_string(), false)];
        let pos = |id: i32| Some(Pos::new(id as i16, 0, 0));
        let mut f = Vec::new();
        let mut next = 0;
        let l = |s: &str| s.to_string();
        group_lines(&mut f, &[l("Urist McAxe hacks Snang in the head!"), l("Snang bites Urist McAxe!")], 100, "1st Granite, 279", &names, &pos, &mut next);
        // Another dwarf fighting elsewhere at the same time: a second fight.
        group_lines(&mut f, &[l("Kib Ustuth punches the kobold!")], 120, "1st Granite, 279", &names, &pos, &mut next);
        // A line without names joins the newest open fight.
        group_lines(&mut f, &[l("The kobold has been struck down.")], 130, "1st Granite, 279", &names, &pos, &mut next);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].lines.len(), 2);
        assert_eq!(f[1].lines.len(), 2);
        assert_eq!(f[1].deaths, 1);
        assert_eq!(fight_title(&f[0]), "Urist McAxe vs Snang");
        assert_eq!(f[0].pos, Some(Pos::new(1, 0, 0)));
        // Much later: a new fight even with the same dwarf.
        group_lines(&mut f, &[l("Urist McAxe misses Snang!")], 100 + FIGHT_GAP + 50, "2nd Granite, 279", &names, &pos, &mut next);
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn combat_log_diff() {
        let log: Vec<String> = ["a", "b", "c", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let tail: Vec<String> = ["b", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(new_lines(&log, &tail), 6);
        let tail: Vec<String> = ["c", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(new_lines(&log, &tail), 5);
        // Tail scrolled out of the buffer: everything is new.
        let tail: Vec<String> = ["x"].iter().map(|s| s.to_string()).collect();
        assert_eq!(new_lines(&log, &tail), 0);
    }

    #[test]
    fn uniform_entries_and_skills() {
        let mut w = World::synthetic(8, 8, 4);
        w.armor_raws.push(rf_core::ArmorRaw { token: "ITEM_ARMOR_BREASTPLATE".into(), name: "breastplate".into(), kind: "ARMOR".into(), ..Default::default() });
        assert_eq!(split_entry("WEAPON:ITEM_WEAPON_AXE_BATTLE@INORGANIC:STEEL"), ("WEAPON".into(), "ITEM_WEAPON_AXE_BATTLE".into(), "INORGANIC:STEEL".into()));
        assert_eq!(join_entry("HELM", "", "Armor"), "HELM@Armor");
        assert_eq!(entry_text(&w, "ARMOR:ITEM_ARMOR_BREASTPLATE@INORGANIC:STEEL"), "Body: breastplate (steel)");
        assert_eq!(entry_text(&w, "HELM@Armor"), "Head: any helm");
        assert_eq!(entry_text(&w, "ARMOR@Armor"), "Body: any body armor");
        assert_eq!(slot_choices(&w, "ARMOR").len(), 3);
        assert_eq!(skill_noun("MINING"), "Miner");
        assert_eq!(skill_noun("ENGRAVE_STONE"), "Engraver");
        assert_eq!(skill_title(&rf_core::Skill { id: "MINING".into(), rating: 17, xp: 0 }), "Legendary +2 Miner");
        assert_eq!(xp_needed(0), 500);
        assert_eq!(xp_needed(15), 2000);
        assert_eq!(wrap("aaa bbb ccc ddd", 8), vec!["aaa bbb", "ccc ddd"]);
    }
}
