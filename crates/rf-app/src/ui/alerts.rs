//! Hostiles: red markers on the map, and DF's pop-up alert that pauses the game when invaders
//! or monsters arrive ("A vile force of darkness has arrived!").
//!
//! An alert is raised when `Sim::hostiles()` contains creatures that were not hostile before
//! (and are not caged). Creatures arriving within a day of the first one belong to the same
//! wave and raise no second alert. Plain wild animals only get an announcement, as in DF.

use super::panels::unit_name;
use super::{col, panel, pretty, text_button, BtnState};
use crate::font::GH;
use crate::game::{date_string, Alert, Game, LogLine, Selected};
use macroquad::prelude::*;
use rf_core::{Unit, World};
use rf_sim::AnnouncementKind;

/// Hostiles arriving within this many ticks of an alert join that wave (1 day).
pub const WAVE_TICKS: u64 = rf_core::TICKS_PER_DAY as u64;

/// Megabeast-like creature (DF pauses with "<Name> has come!" for those).
pub fn is_monster(w: &World, u: &Unit) -> bool {
    let r = u.race.as_str();
    if r.starts_with("FORGOTTEN_BEAST") || r.starts_with("TITAN") || r.starts_with("DEMON") || r.starts_with("NIGHT_CREATURE") || r.starts_with("HF") {
        return true;
    }
    w.creature_raws
        .iter()
        .find(|c| c.token == u.race)
        .map(|c| c.flags.iter().any(|f| matches!(f.as_str(), "MEGABEAST" | "SEMIMEGABEAST" | "FEATURE_BEAST" | "TITAN" | "UNIQUE_DEMON" | "DEMON")))
        .unwrap_or(false)
}

fn race_plural(w: &World, race: &str) -> String {
    w.creature_raws
        .iter()
        .find(|c| c.token == race)
        .map(|c| c.name_plural.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("{}s", pretty(race).to_lowercase()))
}

/// Title, DF-style message, and whether the game pauses, for a wave of new hostiles.
pub fn alert_text(w: &World, units: &[&Unit]) -> (String, String, bool) {
    if let Some(m) = units.iter().find(|u| is_monster(w, u)) {
        let name = unit_name(m);
        return (format!("{name} has come!"), format!("A {} has come to the fortress! Beware!", pretty(&m.race).to_lowercase()), true);
    }
    let people: Vec<&&Unit> = units.iter().filter(|u| !u.flags.animal).collect();
    if people.is_empty() {
        let race = units.first().map(|u| u.race.clone()).unwrap_or_default();
        let n = units.len();
        let what = if n == 1 { format!("A hostile {}", pretty(&race).to_lowercase()) } else { format!("{n} hostile {}", race_plural(w, &race)) };
        return ("Hostile creatures".into(), format!("{what} has been sighted."), false);
    }
    // Most common invader race.
    let mut races: Vec<(String, usize)> = Vec::new();
    for u in &people {
        match races.iter_mut().find(|(r, _)| *r == u.race) {
            Some(e) => e.1 += 1,
            None => races.push((u.race.clone(), 1)),
        }
    }
    races.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let race = races[0].0.clone();
    if people.len() >= 6 {
        let text = if race == "GOBLIN" {
            "A vile force of darkness has arrived!".to_string()
        } else {
            format!("An army of {} has arrived!", race_plural(w, &race))
        };
        ("Siege!".into(), text, true)
    } else if people.len() >= 2 {
        ("Ambush!".into(), "An ambush! Curse them!".into(), true)
    } else {
        let u = people[0];
        (format!("{} spotted!", pretty(&race)), format!("{}, a hostile {}, has been spotted!", unit_name(u), pretty(&race).to_lowercase()), true)
    }
}

impl Game {
    /// Forget the hostiles of the old world (new game / loaded save): no alert for them.
    pub fn reset_hostiles(&mut self) {
        let mut h = self.sim.hostiles();
        h.sort_unstable();
        self.hostiles_seen = h.clone();
        self.hostiles = h;
        self.alert_tick = None;
        // A siege already in progress gets an announcement (no pop-up, like loading in DF).
        let w = &self.sim.world;
        let active: Vec<&Unit> = self.hostiles.iter().filter_map(|id| w.unit(*id)).filter(|u| !u.flags.caged && !u.flags.dead).collect();
        if !active.is_empty() {
            let n = active.len();
            let text = format!("{n} hostile creature{} on the map!", if n == 1 { " is" } else { "s are" });
            let pos = active.first().map(|u| u.pos);
            let date = date_string(&w.calendar);
            self.log.push(LogLine::new(text, pos, AnnouncementKind::Warning, get_time(), date));
        }
    }

    /// Called once per frame after the sim ran.
    pub fn update_hostiles(&mut self) {
        let mut h = self.sim.hostiles();
        h.sort_unstable();
        let new: Vec<i32> = h.iter().copied().filter(|id| self.hostiles_seen.binary_search(id).is_err()).collect();
        if !new.is_empty() {
            for id in &new {
                if let Err(i) = self.hostiles_seen.binary_search(id) {
                    self.hostiles_seen.insert(i, *id);
                }
            }
            let active: Vec<i32> = new
                .into_iter()
                .filter(|id| self.sim.world.unit(*id).map(|u| !u.flags.caged && !u.flags.dead).unwrap_or(false))
                .collect();
            if !active.is_empty() {
                let now = self.sim.ticks();
                let same_wave = self.alert_tick.map(|t| now.saturating_sub(t) < WAVE_TICKS).unwrap_or(false);
                if same_wave {
                    if let Some(a) = self.alert.as_mut() {
                        a.units.extend(active);
                    }
                } else {
                    self.raise_alert(active);
                }
            }
        }
        self.hostiles = h;
    }

    fn raise_alert(&mut self, ids: Vec<i32>) {
        let w = &self.sim.world;
        let units: Vec<&Unit> = ids.iter().filter_map(|id| w.unit(*id)).collect();
        let (title, text, pause) = alert_text(w, &units);
        let pos = units.first().map(|u| u.pos);
        let date = date_string(&w.calendar);
        self.log.push(LogLine::new(text.clone(), pos, AnnouncementKind::Warning, get_time(), date));
        self.alert_tick = Some(self.sim.ticks());
        if pause {
            self.paused = true;
            self.alert = Some(Alert { title, text, units: ids, pos });
        }
    }
}

/// Pulsing red frame around a hostile creature's tile.
pub fn hostile_marker(r: Rect) {
    let t = get_time();
    let a = 0.65 + 0.35 * ((t * 4.0).sin() as f32);
    let w = (r.w / 14.0).max(1.5);
    draw_rectangle_lines(r.x - w * 0.5, r.y - w * 0.5, r.w + w, r.h + w, w, Color::new(1.0, 0.15, 0.1, a));
    // Small "!" badge in the corner.
    let b = (r.w / 4.0).max(5.0);
    draw_rectangle(r.x + r.w - b, r.y - b * 0.4, b, b, Color::new(0.75, 0.05, 0.05, 0.95));
    let lw = (b / 6.0).max(1.0);
    draw_line(r.x + r.w - b / 2.0, r.y - b * 0.25, r.x + r.w - b / 2.0, r.y + b * 0.3, lw, WHITE);
    draw_rectangle(r.x + r.w - b / 2.0 - lw / 2.0, r.y + b * 0.42, lw, lw, WHITE);
}

/// Small blood drop for wounded creatures (red = bleeding, orange = wounded).
pub fn wound_marker(r: Rect, bleeding: bool) {
    let c = if bleeding { Color::from_rgba(220, 20, 20, 240) } else { Color::from_rgba(240, 150, 40, 230) };
    let rad = (r.w / 10.0).max(2.0);
    draw_circle(r.x + rad * 1.4, r.y + rad * 1.6, rad, c);
    draw_triangle(vec2(r.x + rad * 0.45, r.y + rad * 1.3), vec2(r.x + rad * 2.35, r.y + rad * 1.3), vec2(r.x + rad * 1.4, r.y), c);
}

/// The pop-up (drawn last, above everything).
pub fn draw_alert(g: &mut Game) {
    let Some(a) = g.alert.clone() else { return };
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    let tw = g.font.width(&a.title, 2.0 * s).max(g.font.width(&a.text, s)) + 60.0 * s;
    let (w, h) = (tw.clamp(460.0 * s, sw - 40.0), 168.0 * s);
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.5, w, h);
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.15, 0.0, 0.0, 0.25));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.03, 0.03, 1.0));
    panel(&mut g.ui, r);
    draw_rectangle_lines(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, 2.0, Color::from_rgba(200, 40, 30, 255));
    let mut y = r.y + 14.0 * s;
    let blink = (get_time() * 2.0) as i64 % 2 == 0;
    let tc = if blink { Color::from_rgba(255, 70, 50, 255) } else { Color::from_rgba(255, 140, 90, 255) };
    g.font.draw(&a.title, r.x + (w - g.font.width(&a.title, 2.0 * s)) / 2.0, y, 2.0 * s, tc);
    y += 2.0 * GH * s + 10.0 * s;
    g.font.draw(&a.text, r.x + (w - g.font.width(&a.text, s)) / 2.0, y, s, col::TEXT_HI);
    y += lh + 2.0 * s;
    let n = a.units.len();
    let sub = format!("{n} hostile creature{} - the game is paused", if n == 1 { "" } else { "s" });
    g.font.draw(&sub, r.x + (w - g.font.width(&sub, s)) / 2.0, y, s, col::TEXT_DIM);
    let bh = 28.0 * s;
    let bw = 20.0 * 8.0 * s;
    let by = r.y + r.h - bh - 14.0 * s;
    let gap = 16.0 * s;
    let bx = r.x + (w - 2.0 * bw - gap) / 2.0;
    if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Center on them", BtnState::Normal) {
        if let Some(u) = a.units.iter().find_map(|id| g.sim.world.unit(*id)) {
            let (id, p) = (u.id, u.pos);
            g.cam.focus(p);
            g.selected = Selected::Unit(id);
            g.unit_tab = 0;
        } else if let Some(p) = a.pos {
            g.cam.focus(p);
        }
        g.alert = None;
    }
    if text_button(&mut g.ui, &g.font, Rect::new(bx + bw + gap, by, bw, bh), "OK (Enter)", BtnState::Normal) {
        g.alert = None;
    }
    // Modal: swallow other clicks.
    g.ui.block(Rect::new(0.0, 0.0, sw, sh));
    if g.ui.lpressed {
        g.ui.click_used = true;
    }
}

/// `--ui spawn-hostile`: turn the living non-citizens nearest to the camera (up to 8, not
/// caged) into invaders, so the arrival alert can be tested without a real siege.
pub fn debug_spawn_hostile(g: &mut Game) {
    let (cx, cy, cz) = (g.cam.x as i32, g.cam.y as i32, g.cam.z);
    let w = &mut g.sim.world;
    let mut cand: Vec<(i32, usize)> = w
        .units
        .iter()
        .enumerate()
        .filter(|(_, u)| !u.flags.citizen && !u.flags.dead && !u.flags.caged && !u.flags.hostile && !u.flags.inactive)
        .map(|(i, u)| {
            let d = (u.pos.x as i32 - cx).abs() + (u.pos.y as i32 - cy).abs() + 3 * (u.pos.z as i32 - cz).abs();
            (d, i)
        })
        .collect();
    cand.sort();
    for (_, i) in cand.into_iter().take(8) {
        let u = &mut w.units[i];
        u.flags.hostile = true;
        u.flags.animal = false;
        u.invader_civ = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siege_and_ambush_texts() {
        let w = World::synthetic(16, 16, 10);
        let gob = |id: i32| Unit { id, race: "GOBLIN".into(), invader_civ: 5, ..Default::default() };
        let many: Vec<Unit> = (0..8).map(gob).collect();
        let refs: Vec<&Unit> = many.iter().collect();
        let (t, m, p) = alert_text(&w, &refs);
        assert_eq!((t.as_str(), m.as_str(), p), ("Siege!", "A vile force of darkness has arrived!", true));
        let (t, m, _) = alert_text(&w, &refs[..3]);
        assert_eq!((t.as_str(), m.as_str()), ("Ambush!", "An ambush! Curse them!"));
        let wolf = Unit { id: 99, race: "WOLF".into(), flags: rf_core::UnitFlags { animal: true, ..Default::default() }, ..Default::default() };
        let (_, m, p) = alert_text(&w, &[&wolf]);
        assert!(!p && m.contains("wolf"));
        let fb = Unit { id: 7, race: "FORGOTTEN_BEAST_3".into(), name: "Ngokang".into(), ..Default::default() };
        let (t, _, p) = alert_text(&w, &[&fb]);
        assert!(p && t == "Ngokang has come!");
    }
}
