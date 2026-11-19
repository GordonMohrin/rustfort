//! [ui13] Round-13 screens: diplomacy, threats, weather and curses. Uses only the round-13 sim
//! API (`rf_sim::round13_api`), plus `Sim::season_label` and the plain world data the sheet
//! already reads (unit names, tile flags).
//!
//! - **Top bar**: weather (`Sim::weather_summary`) and the surface temperature in degrees
//!   Celsius (degrees Urist in the tooltip), snow / ice and the moon in the tooltip.
//! - **Weather on the map**: rain streaks, snowflakes, fog, evil rain and evil clouds over the
//!   outside tiles of the shown level (`Sim::weather_at`), snow cover (`Sim::snow_at`).
//! - **Diplomacy** (F2): the civilizations with relation, attitude and wars
//!   (`Sim::diplomacy_entities`), the demands with **Accept** / **Refuse**
//!   (`Sim::diplomacy_demands`, `Sim::answer_demand`), the meetings with envoys
//!   (`Sim::diplomacy_meetings`), the land title and the outpost liaison's requests for the next
//!   caravan (`Sim::set_liaison_requests`).
//! - **Threats** (F3): active threats (`Sim::threats`) with Go buttons, the megabeasts that came
//!   (`Sim::megabeasts`), sky and moon, the world-event toggles and the module counters.
//! - **Unit sheet markers** (`Sim::unit_curse`): undead, necromancer, husk, thrall, unmasked or
//!   suspected vampire (with **Accuse of vampirism**, `Sim::accuse_vampire`), werebeast in
//!   were-form, megabeast.
//! - **Alerts**: a box when a new demand needs an answer (pauses, "Answer now" opens
//!   Diplomacy), a blinking "Threats" / "Demands" badge under the top bar, a status line for new
//!   threats and evil weather. Monsters and sieges keep their own hostile alert (`alerts.rs`).

use super::round8::{frame, scroll, tabs, tabs_x};
use super::{checkbox, clip, col, list_row, panel, pretty, text_button, BtnState};
use crate::font::{GH, GW};
use crate::game::{Game, Screen, Selected};
use macroquad::prelude::*;
use rf_core::Pos;
use rf_sim::{Demand, DemandKind, DiplomacyEntity, EvilWeather, Meeting, MeetingKind, MegabeastInfo, Sim, Threat, ThreatKind, WeatherInfo, WeatherKind};

/// Real seconds between two polls of the round-13 API (threats, demands, weather).
const POLL_S: f64 = 0.4;
const DAY: u64 = rf_core::TICKS_PER_DAY as u64;

/// Item kinds the outpost liaison can be asked for (wiki Outpost liaison: the requests screen
/// lists item categories for next year's caravan).
pub const REQUEST_KINDS: &[&str] = &["Metal bars", "Ores", "Weapons", "Armor", "Ammunition", "Cloth", "Thread", "Leather", "Wood", "Food", "Drinks", "Seeds", "Animals", "Gems", "Crafts", "Instruments", "Tools", "Furniture"];

const DIPLO_TABS: &[&str] = &["Civilizations", "Demands", "Meetings"];

// ====================================================================== state

#[derive(Default)]
pub struct R13Ui {
    /// Diplomacy tab: 0 civilizations, 1 demands, 2 meetings.
    pub tab: usize,
    pub ent_sel: Option<i32>,
    pub scroll: usize,
    pub scroll2: usize,
    // ---- cached API answers (polled every `POLL_S`)
    pub polled: Option<f64>,
    pub weather: Option<WeatherInfo>,
    pub evil: Option<EvilWeather>,
    pub moon: u8,
    pub threats: Vec<Threat>,
    pub demands: Vec<Demand>,
    pub meetings: Vec<Meeting>,
    pub entities: Vec<DiplomacyEntity>,
    pub beasts: Vec<MegabeastInfo>,
    // ---- alerts
    primed: bool,
    seen_threats: Vec<(ThreatKind, String, u64)>,
    seen_demands: Vec<i32>,
    seen_evil: bool,
    /// Threats that came since the Threats screen was last open.
    pub new_threats: usize,
    /// A demand box: (title, text, demand id).
    pub popup: Option<(String, String, i32)>,
    /// Liaison requests chosen here per entity (the API has no getter for them).
    pub requests: Vec<(i32, Vec<String>)>,
    // ---- `--ui r13-demo` sample data (screenshots while the modules are still stubs)
    pub demo: bool,
    demo_threats: Vec<Threat>,
    demo_demands: Vec<Demand>,
    demo_meetings: Vec<Meeting>,
    demo_beasts: Vec<MegabeastInfo>,
}

// ====================================================================== words

/// Moon phase name for `Sim::moon_phase` (0..=27, 0 new, 14 full).
pub fn moon_name(p: u8) -> &'static str {
    match p % 28 {
        0 => "New moon",
        1..=6 => "Waxing crescent",
        7 => "First quarter",
        8..=13 => "Waxing gibbous",
        14 => "Full moon",
        15..=20 => "Waning gibbous",
        21 => "Last quarter",
        _ => "Waning crescent",
    }
}

/// "12°C" from degrees Urist (0 = unknown).
pub fn temp_text(u: i32) -> String {
    if u == 0 {
        "?°C".into()
    } else {
        format!("{}°C", Sim::urist_to_celsius(u))
    }
}

pub fn threat_label(k: ThreatKind) -> &'static str {
    match k {
        ThreatKind::Megabeast => "Megabeast",
        ThreatKind::SemiMegabeast => "Semi-megabeast",
        ThreatKind::Titan => "Titan",
        ThreatKind::ForgottenBeast => "Forgotten beast",
        ThreatKind::Siege => "Siege",
        ThreatKind::UndeadHorde => "Undead",
        ThreatKind::NecromancerSiege => "Necromancer siege",
        ThreatKind::Werebeast => "Werebeast",
        ThreatKind::Vampire => "Vampire",
    }
}

fn threat_color(k: ThreatKind) -> Color {
    match k {
        ThreatKind::UndeadHorde | ThreatKind::NecromancerSiege => Color::from_rgba(170, 220, 150, 255),
        ThreatKind::Werebeast | ThreatKind::Vampire => Color::from_rgba(220, 120, 230, 255),
        ThreatKind::Siege => col::TEXT_BAD,
        _ => Color::from_rgba(255, 150, 60, 255),
    }
}

pub fn demand_label(k: DemandKind) -> &'static str {
    match k {
        DemandKind::Request => "Request",
        DemandKind::TreeQuota => "Tree quota",
        DemandKind::Tribute => "Tribute",
        DemandKind::Peace => "Peace offer",
    }
}

/// What answering means (wiki Diplomat / Outpost liaison; consequences are the sim's).
fn demand_hint(d: &Demand) -> String {
    match d.kind {
        DemandKind::TreeQuota => format!("Accept: fell no more than {} trees a year. Refusing or felling more angers them and can mean war.", d.amount),
        DemandKind::Tribute => format!("Accept: hand over goods worth {}☼. Refusing angers them.", d.amount),
        DemandKind::Peace => "Accept: the war ends. Refuse: it goes on.".into(),
        DemandKind::Request => "Accept: the liaison takes the requests home for next year's caravan.".into(),
    }
}

pub fn meeting_label(k: MeetingKind) -> &'static str {
    match k {
        MeetingKind::Liaison => "Outpost liaison",
        MeetingKind::Diplomat => "Diplomat",
        MeetingKind::Elevation => "Elevation",
        MeetingKind::LandHolder => "Land holder",
    }
}

fn relation_color(r: &str) -> Color {
    match r {
        "Own" => col::TEXT_HI,
        "War" => col::TEXT_BAD,
        "Peace" => col::TEXT_GOOD,
        _ => col::TEXT_DIM,
    }
}

/// "Barony" etc.; the plain outpost without a title.
fn title_text(t: &str) -> &str {
    if t.is_empty() {
        "Outpost"
    } else {
        t
    }
}

fn ago(now: u64, t: u64) -> String {
    if t == 0 || t > now {
        return String::new();
    }
    let d = (now - t) / DAY;
    match d {
        0 => "today".into(),
        1 => "yesterday".into(),
        _ => format!("{d} days ago"),
    }
}

fn weather_glyph(k: WeatherKind) -> (&'static str, Color) {
    match k {
        WeatherKind::Clear => ("☼", Color::from_rgba(250, 220, 90, 255)),
        WeatherKind::Cloudy => ("≈", Color::from_rgba(180, 180, 190, 255)),
        WeatherKind::Rain => ("÷", Color::from_rgba(110, 160, 250, 255)),
        WeatherKind::Snow => ("*", Color::from_rgba(235, 240, 255, 255)),
        WeatherKind::Fog => ("░", Color::from_rgba(170, 170, 175, 255)),
        WeatherKind::EvilRain => ("÷", Color::from_rgba(230, 70, 70, 255)),
        WeatherKind::EvilCloud => ("≈", Color::from_rgba(200, 90, 220, 255)),
    }
}

fn unit_label(g: &Game, id: i32) -> String {
    if id < 0 {
        return String::new();
    }
    g.sim.world.unit(id).map(super::panels::unit_name).unwrap_or_else(|| format!("#{id}"))
}

fn entity_name(g: &Game, id: i32) -> String {
    let Some(e) = g.r13.entities.iter().find(|e| e.id == id) else {
        return if id < 0 { "unknown".into() } else { format!("entity #{id}") };
    };
    if !e.name.is_empty() {
        e.name.clone()
    } else if !e.race.is_empty() {
        format!("the {} (#{id})", super::pretty(&e.race).to_lowercase())
    } else {
        format!("entity #{id}")
    }
}

/// Inside the map (units off site sit at -30000).
fn on_map(g: &Game, p: Pos) -> bool {
    g.sim.world.in_bounds(p)
}

// ====================================================================== polling and alerts

fn poll(g: &mut Game) {
    let r = &mut g.r13;
    let sim = &g.sim;
    r.weather = Some(sim.weather_summary());
    r.evil = sim.evil_weather();
    r.moon = sim.moon_phase();
    r.threats = sim.threats();
    r.demands = sim.diplomacy_demands();
    r.meetings = sim.diplomacy_meetings();
    r.entities = sim.diplomacy_entities();
    r.beasts = sim.megabeasts();
    if r.demo {
        r.threats.extend(r.demo_threats.iter().cloned());
        r.demands.extend(r.demo_demands.iter().cloned());
        r.meetings.extend(r.demo_meetings.iter().cloned());
        r.beasts.extend(r.demo_beasts.iter().cloned());
    }
}

/// Once per frame (after the sim ran): poll the API and raise alerts for new things.
pub fn update(g: &mut Game) {
    let now = get_time();
    if g.r13.polled.map(|t| now - t < POLL_S).unwrap_or(false) {
        return;
    }
    g.r13.polled = Some(now);
    poll(g);
    let prime = !g.r13.primed;
    g.r13.primed = true;
    // New threats: a status line and the badge (the hostile alert covers monsters and sieges).
    let mut fresh: Vec<String> = Vec::new();
    for t in g.r13.threats.clone() {
        let key = (t.kind, t.name.clone(), t.since);
        if g.r13.seen_threats.contains(&key) {
            continue;
        }
        g.r13.seen_threats.push(key);
        if !prime {
            fresh.push(if t.name.is_empty() { threat_label(t.kind).to_string() } else { format!("{} ({})", t.name, threat_label(t.kind).to_lowercase()) });
        }
    }
    if g.r13.seen_threats.len() > 256 {
        let n = g.r13.seen_threats.len() - 256;
        g.r13.seen_threats.drain(..n);
    }
    if !fresh.is_empty() && g.screen != Screen::Threats {
        g.r13.new_threats += fresh.len();
        g.set_status(format!("New threat: {}", fresh.join(", ")));
    }
    // Evil weather starting.
    let evil = g.r13.evil.clone();
    if evil.is_some() && !g.r13.seen_evil && !prime {
        let name = evil.as_ref().map(|e| e.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Evil weather".into());
        g.set_status(format!("{} is coming over the land.", super::pretty(&name)));
    }
    g.r13.seen_evil = evil.is_some();
    // New open demands: a box that pauses the game.
    let open: Vec<Demand> = g.r13.demands.iter().filter(|d| d.answer.is_none()).cloned().collect();
    for d in open {
        if g.r13.seen_demands.contains(&d.id) {
            continue;
        }
        g.r13.seen_demands.push(d.id);
        if prime || g.r13.popup.is_some() {
            continue;
        }
        let who = entity_name(g, d.entity_id);
        let title = format!("{} from {who}", demand_label(d.kind));
        let text = if d.text.is_empty() { demand_hint(&d) } else { d.text.clone() };
        g.r13.popup = Some((title, text, d.id));
        g.paused = true;
    }
}

/// Close the demand box (right click, Esc, Enter). True when there was one.
pub fn close_popup(g: &mut Game) -> bool {
    g.r13.popup.take().is_some()
}

fn answer(g: &mut Game, id: i32, accept: bool) {
    let ok = if let Some(d) = g.r13.demo_demands.iter_mut().find(|d| d.id == id && d.answer.is_none()) {
        d.answer = Some(accept);
        true
    } else {
        g.sim.answer_demand(id, accept)
    };
    g.set_status(match (ok, accept) {
        (false, _) => "That demand is no longer open.".into(),
        (true, true) => "Demand accepted.".into(),
        (true, false) => "Demand refused.".into(),
    });
    g.r13.polled = None;
    if g.r13.popup.as_ref().map(|p| p.2 == id).unwrap_or(false) {
        g.r13.popup = None;
    }
}

// ====================================================================== keys and glue

/// F2 Diplomacy, F3 Threats, Enter closes the demand box. True when used.
pub fn keys(g: &mut Game) -> bool {
    if g.r13.popup.is_some() && g.input.fired.contains(&KeyCode::Enter) {
        g.r13.popup = None;
        return true;
    }
    let (b, inp) = (&g.binds, &g.input);
    if inp.hit(b, "RF_DIPLOMACY") {
        open_diplomacy(g, None);
        return true;
    }
    if inp.hit(b, "RF_THREATS") {
        open_threats(g);
        return true;
    }
    false
}

pub fn open_diplomacy(g: &mut Game, tab: Option<usize>) {
    if tab.is_none() && g.screen == Screen::Diplomacy {
        g.screen = Screen::None;
        return;
    }
    if g.screen != Screen::Diplomacy {
        g.toggle_screen(Screen::Diplomacy);
    }
    if let Some(t) = tab {
        g.r13.tab = t.min(DIPLO_TABS.len() - 1);
    }
    g.r13.scroll = 0;
    g.r13.polled = None;
}

pub fn open_threats(g: &mut Game) {
    g.toggle_screen(Screen::Threats);
    g.r13.new_threats = 0;
    g.r13.scroll = 0;
    g.r13.scroll2 = 0;
    g.r13.polled = None;
}

/// The unit/building sheet is not drawn under these screens.
pub fn hides_sheet(g: &Game) -> bool {
    matches!(g.screen, Screen::Diplomacy | Screen::Threats)
}

pub fn draw_screens(g: &mut Game) {
    match g.screen {
        Screen::Diplomacy => draw_diplomacy(g),
        Screen::Threats => {
            g.r13.new_threats = 0;
            draw_threats(g)
        }
        _ => {}
    }
}

// ====================================================================== top bar

/// Weather and temperature after the season in the top bar. Returns the new x.
pub fn draw_topbar_weather(g: &mut Game, x: f32, ty: f32, h: f32) -> f32 {
    let Some(w) = g.r13.weather.clone() else { return x };
    let s = g.ui.scale;
    let kind = if w.enabled { w.kind } else { WeatherKind::Clear };
    let (gl, gc) = weather_glyph(kind);
    let label = if w.enabled { kind.label() } else { "No weather" };
    let temp = temp_text(w.temperature);
    let mut cx = x;
    cx += g.font.draw(gl, cx, ty, s, gc) + 4.0 * s;
    cx += g.font.draw(label, cx, ty, s, if w.enabled { col::TEXT } else { col::TEXT_DIM }) + 8.0 * s;
    let tc = if w.temperature != 0 && w.temperature <= 10_000 { col::TEXT_CYAN } else { col::TEXT };
    cx += g.font.draw(&temp, cx, ty, s, tc);
    if w.snow_cover || w.frozen {
        cx += 6.0 * s;
        cx += g.font.draw(if w.frozen { "ice" } else { "snow" }, cx, ty, s, Color::from_rgba(220, 235, 255, 255));
    }
    let r = Rect::new(x - 4.0 * s, 0.0, cx - x + 8.0 * s, h);
    if g.ui.hovered(r) {
        let mut lines = vec![format!("{}: {}", w.season, if w.enabled { kind.label() } else { "weather is off" })];
        if w.temperature != 0 {
            lines.push(format!("Surface {} ({} degrees Urist; water freezes at 10000)", temp, w.temperature));
        }
        if w.snow_cover {
            lines.push("Snow lies on the ground.".into());
        }
        if w.frozen {
            lines.push("Surface water is frozen.".into());
        }
        if let Some(e) = &g.r13.evil {
            lines.push(format!("Evil weather: {}", if e.name.is_empty() { e.kind.label() } else { e.name.as_str() }));
        }
        lines.push(format!("Moon: {}", moon_name(g.r13.moon).to_lowercase()));
        lines.push("Threats and weather: F3".into());
        g.ui.tooltip = Some((lines.join("\n"), vec2(x, h + 6.0 * s + lines.len() as f32 * g.ui.line_h() + 10.0 * s)));
    }
    cx + 18.0 * s
}

// ====================================================================== badges and box

/// Blinking "Threats" / "Demands" badges under the top bar (below the petitions badge).
pub fn draw_badges(g: &mut Game, top: f32) {
    if g.screen != Screen::None {
        return; // over a big screen the badge would cover its title; the tabs show the counts
    }
    let s = g.ui.scale;
    let mut y = top + 4.0 * s;
    if g.r9.pet_pending > 0 && g.screen != Screen::Petitions {
        y += GH * s + 16.0 * s;
    }
    let open = g.r13.demands.iter().filter(|d| d.answer.is_none()).count();
    let blink = (get_time() * 1.5) as i64 % 2 == 0;
    let mut items: Vec<(String, u8)> = Vec::new();
    if open > 0 && g.screen != Screen::Diplomacy {
        items.push((format!("Demands ({open})"), 0));
    }
    if g.r13.new_threats > 0 && g.screen != Screen::Threats {
        items.push((format!("Threats ({})", g.r13.new_threats), 1));
    }
    for (t, which) in items {
        let r = Rect::new(6.0 * s, y, g.font.width(&t, s) + 24.0 * s, GH * s + 12.0 * s);
        g.ui.block(r);
        let (bg, hi) = if which == 1 { (Color::new(0.20, 0.03, 0.03, 0.95), col::TEXT_BAD) } else { (Color::new(0.10, 0.08, 0.03, 0.95), col::TEXT_HI) };
        draw_rectangle(r.x, r.y, r.w, r.h, bg);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, if blink { hi } else { col::BORDER });
        g.font.draw("!", r.x + 6.0 * s, r.y + 6.0 * s, s, hi);
        g.font.draw(&t, r.x + 18.0 * s, r.y + 6.0 * s, s, hi);
        if g.ui.hovered(r) {
            let tip = if which == 1 { "New threats to the fortress: click for the Threats screen (F3)" } else { "Envoys wait for an answer: click for Diplomacy (F2)" };
            g.ui.tooltip = Some((tip.into(), vec2(r.x, r.y + r.h + 40.0 * s)));
        }
        if g.ui.clicked(r) {
            if which == 1 {
                open_threats(g);
            } else {
                open_diplomacy(g, Some(1));
            }
        }
        y += r.h + 4.0 * s;
    }
}

/// The demand box (modal like the caravan box).
pub fn draw_popup(g: &mut Game) {
    let Some((title, text, id)) = g.r13.popup.clone() else { return };
    let s = g.ui.scale;
    let (sw, sh) = (screen_width(), screen_height());
    let w = (620.0 * s).min(sw - 40.0);
    let maxc = ((w - 40.0 * s) / (GW * s)) as usize;
    let lines = super::round9::wrap(&text, maxc);
    let lh = g.ui.line_h();
    let h = 1.5 * GH * s + 40.0 * s + lines.len().min(6) as f32 * lh + 26.0 * s + 24.0 * s;
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.5, w, h);
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.25));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.04, 0.04, 1.0));
    panel(&mut g.ui, r);
    let mut y = r.y + 14.0 * s;
    g.font.draw(&title, r.x + (w - g.font.width(&title, 1.5 * s)) / 2.0, y, 1.5 * s, col::TEXT_HI);
    y += 1.5 * GH * s + 12.0 * s;
    for l in lines.iter().take(6) {
        g.font.draw(l, r.x + (w - g.font.width(l, s)) / 2.0, y, s, col::TEXT);
        y += lh;
    }
    let bw = 15.0 * GW * s;
    let bh = 26.0 * s;
    let gap = 12.0 * s;
    let by = r.y + r.h - bh - 14.0 * s;
    let mut bx = r.x + (w - 4.0 * bw - 3.0 * gap) / 2.0;
    if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Accept", BtnState::Normal) {
        answer(g, id, true);
    }
    bx += bw + gap;
    if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Refuse", BtnState::Normal) {
        answer(g, id, false);
    }
    bx += bw + gap;
    if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Diplomacy...", BtnState::Normal) {
        g.r13.popup = None;
        open_diplomacy(g, Some(1));
    }
    bx += bw + gap;
    if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Later (Enter)", BtnState::Normal) {
        g.r13.popup = None;
    }
    g.ui.block(Rect::new(0.0, 0.0, sw, sh));
    if g.ui.lpressed {
        g.ui.click_used = true;
    }
}

// ====================================================================== diplomacy screen

fn draw_diplomacy(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(c) = frame(g, "Diplomacy", 1180.0) else { return };
    let open = g.r13.demands.iter().filter(|d| d.answer.is_none()).count();
    let labels: Vec<String> = vec![DIPLO_TABS[0].to_string(), if open > 0 { format!("Demands ({open})") } else { DIPLO_TABS[1].to_string() }, format!("Meetings ({})", g.r13.meetings.len())];
    let refs: Vec<&str> = labels.iter().map(|l| l.as_str()).collect();
    let (hit, _) = tabs(g, tabs_x(g, c, "Diplomacy"), c.y - 3.0 * s, &refs, g.r13.tab.min(2));
    if let Some(t) = hit {
        g.r13.tab = t;
        g.r13.scroll = 0;
    }
    let top = c.y + 2.0 * lh + 4.0 * s;
    let maxc = (c.w / (GW * s)) as usize;
    let title = g.sim.land_title();
    let rep = g.sim.diplomacy_report();
    let head = format!(
        "{}. {} civilizations known, {} at war. {} meetings, {} demands ({} accepted, {} refused).",
        title_text(title),
        g.r13.entities.len(),
        g.r13.entities.iter().filter(|e| e.relation == "War").count(),
        rep.meetings,
        rep.demands,
        rep.accepted,
        rep.refused
    );
    g.font.draw(&clip(&head, maxc), c.x, top, s, col::TEXT_DIM);
    draw_line(c.x, top + lh + 2.0 * s, c.x + c.w, top + lh + 2.0 * s, 1.0, col::BORDER);
    let body = Rect::new(c.x, top + lh + 10.0 * s, c.w, c.y + c.h - top - lh - 10.0 * s);
    match g.r13.tab {
        0 => draw_civs(g, body),
        1 => draw_demands(g, body),
        _ => draw_meetings(g, body, None),
    }
}

fn attitude_bar(g: &Game, x: f32, y: f32, w: f32, a: i32) {
    let s = g.ui.scale;
    let h = GH * s - 3.0 * s;
    draw_rectangle(x, y + 1.0, w, h, Color::new(0.12, 0.12, 0.14, 1.0));
    let mid = x + w / 2.0;
    let v = (a.clamp(-100, 100) as f32) / 100.0 * (w / 2.0);
    let c = if a < -30 { col::TEXT_BAD } else if a > 30 { col::TEXT_GOOD } else { col::TEXT_HI };
    if v >= 0.0 {
        draw_rectangle(mid, y + 1.0, v, h, c);
    } else {
        draw_rectangle(mid + v, y + 1.0, -v, h, c);
    }
    draw_line(mid, y, mid, y + h + 2.0, 1.0, col::TEXT_DIM);
    draw_rectangle_lines(x, y + 1.0, w, h, 1.0, Color::new(0.35, 0.35, 0.38, 1.0));
}

fn draw_civs(g: &mut Game, body: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let mut ents = g.r13.entities.clone();
    // Own civilization first, then the named ones (real civilizations), then nameless entities.
    ents.sort_by_key(|e| (e.relation != "Own", e.name.is_empty()));
    if ents.is_empty() {
        g.font.draw("No civilizations known yet (the world's entities come with the import).", body.x, body.y, s, col::TEXT_DIM);
        return;
    }
    if g.r13.ent_sel.map(|id| !ents.iter().any(|e| e.id == id)).unwrap_or(true) {
        g.r13.ent_sel = ents.iter().find(|e| e.relation == "Own").or(ents.first()).map(|e| e.id);
        g.r13.scroll = ents.iter().position(|e| Some(e.id) == g.r13.ent_sel).unwrap_or(0); // scrolled into view
    }
    let lw = (body.w * 0.5).floor();
    let list = Rect::new(body.x, body.y, lw - 10.0 * s, body.h);
    draw_line(body.x + lw, body.y - 4.0 * s, body.x + lw, body.y + body.h, 1.0, col::BORDER);
    let row_h = 2.0 * lh + 6.0 * s;
    let vis = ((list.h / row_h) as usize).max(1);
    g.r13.scroll = scroll(g, list, g.r13.scroll, ents.len(), vis);
    let lc = (list.w / (GW * s)) as usize;
    let bar_w = 14.0 * GW * s;
    let mut pick = None;
    for (k, e) in ents.iter().enumerate().skip(g.r13.scroll).take(vis) {
        let y = list.y + (k - g.r13.scroll) as f32 * row_h;
        let sel = g.r13.ent_sel == Some(e.id);
        if list_row(&mut g.ui, Rect::new(list.x - 4.0 * s, y - 2.0 * s, list.w + 4.0 * s, row_h - 2.0 * s), sel) {
            pick = Some(e.id);
        }
        let name = if e.name.is_empty() { format!("Entity #{}", e.id) } else { e.name.clone() };
        g.font.draw(&clip(&name, lc.saturating_sub(16)), list.x, y, s, if sel { col::TEXT_HI } else { col::TEXT });
        attitude_bar(g, list.x + list.w - bar_w, y, bar_w, e.attitude);
        let rel = match (e.relation.as_str(), e.at_war_since) {
            ("Own", _) => "your civilization".to_string(),
            ("War", Some(y)) => format!("at war since {y}"),
            ("War", None) => "at war".to_string(),
            ("Peace", _) => "at peace".to_string(),
            (r, _) if r.is_empty() => "no contact".to_string(),
            (r, _) => r.to_lowercase(),
        };
        let race = if e.race.is_empty() { String::new() } else { format!("{}, ", pretty(&e.race)) };
        let t = format!("{race}{rel}{}", if e.trades { ", trades" } else { "" });
        g.font.draw(&clip(&t, lc.saturating_sub(16)), list.x + 2.0 * GW * s, y + lh, s, relation_color(&e.relation));
        let at = format!("{:+}", e.attitude);
        g.font.draw(&at, list.x + list.w - g.font.width(&at, s), y + lh, s, col::TEXT_DIM);
    }
    if let Some(id) = pick {
        g.r13.ent_sel = Some(id);
    }
    // Details of the selected one.
    let Some(e) = ents.iter().find(|e| Some(e.id) == g.r13.ent_sel).cloned() else { return };
    let dx = body.x + lw + 12.0 * s;
    let dw = body.x + body.w - dx;
    let dc = (dw / (GW * s)) as usize;
    let mut y = body.y;
    g.font.draw(&clip(if e.name.is_empty() { "Unnamed" } else { &e.name }, dc / 2), dx, y, 1.25 * s, col::TEXT_HI);
    y += 1.25 * lh + 4.0 * s;
    let att = match e.attitude {
        a if a <= -60 => "hostile",
        a if a <= -20 => "unfriendly",
        a if a < 20 => "neutral",
        a if a < 60 => "friendly",
        _ => "very friendly",
    };
    for (t, c) in [
        (format!("Race: {}", if e.race.is_empty() { "unknown".into() } else { pretty(&e.race) }), col::TEXT),
        (format!("Relation: {}{}", if e.relation.is_empty() { "unknown" } else { &e.relation }, e.at_war_since.map(|y| format!(" (war since year {y})")).unwrap_or_default()), relation_color(&e.relation)),
        (format!("Attitude toward you: {att} ({:+})", e.attitude), col::TEXT),
        (format!("Trades with you: {}", if e.trades { "yes" } else { "no" }), col::TEXT),
    ] {
        g.font.draw(&clip(&t, dc), dx, y, s, c);
        y += lh;
    }
    y += 6.0 * s;
    // Outpost liaison requests (own civilization).
    if e.relation == "Own" {
        g.font.draw("Requests for next year's caravan (outpost liaison):", dx, y, s, col::BORDER_HI);
        y += lh + 4.0 * s;
        let cur: Vec<String> = g.r13.requests.iter().find(|r| r.0 == e.id).map(|r| r.1.clone()).unwrap_or_default();
        let mut cx = dx;
        let mut toggled: Option<String> = None;
        for k in REQUEST_KINDS {
            let w = (k.chars().count() as f32 + 2.0) * GW * s;
            if cx + w > dx + dw {
                cx = dx;
                y += GH * s + 10.0 * s;
            }
            let on = cur.iter().any(|c| c == k);
            if text_button(&mut g.ui, &g.font, Rect::new(cx, y, w, GH * s + 6.0 * s), k, if on { BtnState::Active } else { BtnState::Normal }) {
                toggled = Some(k.to_string());
            }
            cx += w + 4.0 * s;
        }
        y += GH * s + 14.0 * s;
        if let Some(k) = toggled {
            let mut v = cur.clone();
            if let Some(i) = v.iter().position(|c| *c == k) {
                v.remove(i);
            } else {
                v.push(k);
            }
            let refs: Vec<&str> = v.iter().map(|x| x.as_str()).collect();
            g.sim.set_liaison_requests(e.id, &refs);
            match g.r13.requests.iter_mut().find(|r| r.0 == e.id) {
                Some(r) => r.1 = v,
                None => g.r13.requests.push((e.id, v)),
            }
        }
        for l in super::round9::wrap("The liaison arrives with the autumn caravan and takes these home.", dc) {
            g.font.draw(&l, dx, y, s, col::TEXT_DIM);
            y += lh;
        }
        y += 6.0 * s;
    }
    // Their demands and meetings.
    let open: Vec<Demand> = g.r13.demands.iter().filter(|d| d.entity_id == e.id && d.answer.is_none()).cloned().collect();
    if !open.is_empty() {
        let t = format!("{} open demand{}", open.len(), if open.len() == 1 { "" } else { "s" });
        if text_button(&mut g.ui, &g.font, Rect::new(dx, y, (t.len() as f32 + 12.0) * GW * s, GH * s + 8.0 * s), &format!("{t}: answer..."), BtnState::Normal) {
            g.r13.tab = 1;
            g.r13.scroll = 0;
        }
        y += GH * s + 14.0 * s;
    }
    let rest = Rect::new(dx, y, dw, body.y + body.h - y);
    if rest.h > 3.0 * lh {
        g.font.draw("Meetings:", dx, y, s, col::BORDER_HI);
        draw_meetings(g, Rect::new(dx, y + lh + 4.0 * s, dw, rest.h - lh - 4.0 * s), Some(e.id));
    }
}

fn draw_demands(g: &mut Game, body: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let now = g.sim.ticks();
    let mut ds = g.r13.demands.clone();
    // Open first, newest first.
    ds.sort_by(|a, b| a.answer.is_some().cmp(&b.answer.is_some()).then(b.id.cmp(&a.id)));
    if ds.is_empty() {
        g.font.draw("No demands or offers.", body.x, body.y, s, col::TEXT);
        let mut y = body.y + lh;
        let t = "Diplomats of other civilizations come with their caravans: elves set tree quotas, humans may ask for tribute, and a civilization at war may offer peace. Your own civilization's outpost liaison asks what the fortress wants with next year's caravan (Civilizations tab).";
        for l in super::round9::wrap(t, (body.w / (GW * s)) as usize) {
            g.font.draw(&l, body.x, y, s, col::TEXT_DIM);
            y += lh;
        }
        return;
    }
    let maxc = ((body.w - 34.0 * GW * s) / (GW * s)) as usize;
    let per = 4.0 * lh + 12.0 * s;
    let vis = ((body.h / per) as usize).max(1);
    g.r13.scroll = scroll(g, body, g.r13.scroll, ds.len(), vis);
    let mut act: Option<(i32, bool)> = None;
    for (k, d) in ds.iter().enumerate().skip(g.r13.scroll).take(vis) {
        let y0 = body.y + (k - g.r13.scroll) as f32 * per;
        draw_rectangle(body.x - 4.0 * s, y0 - 3.0 * s, body.w + 4.0 * s, per - 6.0 * s, Color::new(1.0, 1.0, 1.0, if d.answer.is_none() { 0.06 } else { 0.02 }));
        let mut y = y0;
        let who = entity_name(g, d.entity_id);
        let head = format!("{} from {who}", demand_label(d.kind));
        g.font.draw(&clip(&head, maxc), body.x, y, s, if d.answer.is_none() { col::TEXT_HI } else { col::TEXT_DIM });
        let mut extra = Vec::new();
        if d.amount != 0 {
            extra.push(match d.kind {
                DemandKind::TreeQuota => format!("{} trees/year", d.amount),
                DemandKind::Tribute => format!("{}☼", d.amount),
                _ => format!("{}", d.amount),
            });
        }
        if d.due > 0 {
            extra.push(if d.due > now { format!("due in {} days", (d.due - now).div_ceil(DAY)) } else { "overdue".into() });
        }
        let room = maxc.saturating_sub(head.chars().count() + 2);
        if !extra.is_empty() && room > 3 {
            let t = clip(&extra.join(", "), room);
            g.font.draw(&t, body.x + g.font.width(&clip(&head, maxc), s) + 12.0 * s, y, s, col::TEXT_CYAN);
        }
        y += lh;
        // Text and hint share the three lines under the head (the hint wraps when the text is short).
        let tc = maxc.saturating_sub(2);
        let text = super::round9::wrap(&d.text, tc);
        let hint = super::round9::wrap(&demand_hint(d), tc);
        let nt = text.len().min(if hint.len() > 1 { 1 } else { 2 });
        for l in text.iter().take(nt) {
            g.font.draw(l, body.x + 2.0 * GW * s, y, s, col::TEXT);
            y += lh;
        }
        for (i, l) in hint.iter().take(3 - nt).enumerate() {
            let l = if i + 1 == 3 - nt && hint.len() > 3 - nt { clip(&format!("{l} ..."), tc) } else { l.clone() };
            g.font.draw(&l, body.x + 2.0 * GW * s, y, s, col::TEXT_DIM);
            y += lh;
        }
        let _ = y;
        // Buttons or the answer.
        let bw = 14.0 * GW * s;
        let bx = body.x + body.w - 2.0 * bw - 6.0 * s;
        match d.answer {
            None => {
                if text_button(&mut g.ui, &g.font, Rect::new(bx, y0, bw, GH * s + 10.0 * s), "Accept", BtnState::Normal) {
                    act = Some((d.id, true));
                }
                if text_button(&mut g.ui, &g.font, Rect::new(bx + bw + 6.0 * s, y0, bw, GH * s + 10.0 * s), "Refuse", BtnState::Normal) {
                    act = Some((d.id, false));
                }
            }
            Some(a) => {
                let t = if a { "Accepted" } else { "Refused" };
                g.font.draw(t, bx + bw + 6.0 * s, y0 + 4.0 * s, s, if a { col::TEXT_GOOD } else { col::TEXT_BAD });
            }
        }
    }
    if let Some((id, a)) = act {
        answer(g, id, a);
    }
}

fn draw_meetings(g: &mut Game, area: Rect, entity: Option<i32>) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let now = g.sim.ticks();
    let mut ms: Vec<Meeting> = g.r13.meetings.iter().filter(|m| entity.map(|e| m.entity_id == e).unwrap_or(true)).cloned().collect();
    ms.sort_by(|a, b| b.tick.cmp(&a.tick).then(b.id.cmp(&a.id)));
    if ms.is_empty() {
        let t = if entity.is_some() { "None yet." } else { "No meetings yet. Envoys arrive with the caravans; a noble (the expedition leader or broker, later the mayor) meets them in an office." };
        for l in super::round9::wrap(t, (area.w / (GW * s)) as usize).into_iter().take(3).enumerate() {
            g.font.draw(&l.1, area.x, area.y + l.0 as f32 * lh, s, col::TEXT_DIM);
        }
        return;
    }
    let maxc = (area.w / (GW * s)) as usize;
    let per = 3.0 * lh + 8.0 * s;
    let vis = ((area.h / per) as usize).max(1);
    let cur = if entity.is_some() { g.r13.scroll2 } else { g.r13.scroll };
    let sc = scroll(g, area, cur, ms.len(), vis);
    if entity.is_some() {
        g.r13.scroll2 = sc;
    } else {
        g.r13.scroll = sc;
    }
    for (k, m) in ms.iter().enumerate().skip(sc).take(vis) {
        let mut y = area.y + (k - sc) as f32 * per;
        let state = if m.state.is_empty() { "Waiting" } else { m.state.as_str() };
        let sc_col = match state {
            "Done" => col::TEXT_DIM,
            "InProgress" => col::TEXT_GOOD,
            _ => col::TEXT_HI,
        };
        let head = format!("{} of {}", meeting_label(m.kind), entity_name(g, m.entity_id));
        g.font.draw(&clip(&head, maxc.saturating_sub(24)), area.x, y, s, col::TEXT);
        let st = format!("{} {}", super::words(state).to_lowercase(), ago(now, m.tick));
        g.font.draw(&st, area.x + area.w - g.font.width(&st, s), y, s, sc_col);
        y += lh;
        let envoy = unit_label(g, m.envoy);
        let host = unit_label(g, m.host);
        let mut who = String::new();
        if !envoy.is_empty() {
            who.push_str(&format!("Envoy {envoy}"));
        }
        if !host.is_empty() {
            who.push_str(&format!("{}meets {host}", if who.is_empty() { "" } else { ", " }));
        }
        if !m.demands.is_empty() {
            who.push_str(&format!("{}{} demand{}", if who.is_empty() { "" } else { "; " }, m.demands.len(), if m.demands.len() == 1 { "" } else { "s" }));
        }
        g.font.draw(&clip(&who, maxc.saturating_sub(2)), area.x + 2.0 * GW * s, y, s, col::TEXT_CYAN);
        y += lh;
        g.font.draw(&clip(&m.text, maxc.saturating_sub(2)), area.x + 2.0 * GW * s, y, s, col::TEXT_DIM);
    }
}

// ====================================================================== threats screen

fn draw_threats(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(c) = frame(g, "Threats", 1180.0) else { return };
    let now = g.sim.ticks();
    let top = c.y + 2.0 * lh + 4.0 * s;
    let lw = (c.w * 0.62).floor();
    let left = Rect::new(c.x, top, lw - 12.0 * s, c.y + c.h - top);
    draw_line(c.x + lw, top - 4.0 * s, c.x + lw, c.y + c.h, 1.0, col::BORDER);
    let lc = (left.w / (GW * s)) as usize;
    // Active threats.
    let ts = g.r13.threats.clone();
    g.font.draw(&format!("Active threats ({})", ts.len()), left.x, left.y, s, col::BORDER_HI);
    let mut y = left.y + lh + 4.0 * s;
    let list_h = if ts.is_empty() { 2.0 * lh } else { (left.h * 0.55).max(4.0 * lh) }; // no gap when empty
    let mut go: Option<(Option<Pos>, i32)> = None;
    if ts.is_empty() {
        g.font.draw("None reported (hostile arrivals also show in the top bar).", left.x, y, s, col::TEXT_DIM);
    } else {
        let row_h = 2.0 * lh + 6.0 * s;
        let area = Rect::new(left.x, y, left.w, list_h);
        let vis = ((area.h / row_h) as usize).max(1);
        g.r13.scroll = scroll(g, area, g.r13.scroll, ts.len(), vis);
        for (k, t) in ts.iter().enumerate().skip(g.r13.scroll).take(vis) {
            let ry = y + (k - g.r13.scroll) as f32 * row_h;
            draw_rectangle(left.x - 4.0 * s, ry - 2.0 * s, left.w + 4.0 * s, row_h - 3.0 * s, Color::new(1.0, 1.0, 1.0, if k % 2 == 0 { 0.04 } else { 0.0 }));
            let kc = threat_color(t.kind);
            let kl = threat_label(t.kind);
            g.font.draw(kl, left.x, ry, s, kc);
            let name = if t.name.is_empty() { pretty(&t.race) } else { t.name.clone() };
            g.font.draw(&clip(&name, lc.saturating_sub(kl.len() + 10)), left.x + (kl.len() as f32 + 2.0) * GW * s, ry, s, col::TEXT);
            let mut parts = Vec::new();
            if !t.race.is_empty() && !t.name.is_empty() {
                parts.push(pretty(&t.race).to_lowercase());
            }
            let n = t.units.len();
            if n > 0 {
                parts.push(format!("{n} creature{}", if n == 1 { "" } else { "s" }));
            }
            if t.entity_id >= 0 {
                parts.push(format!("from {}", entity_name(g, t.entity_id)));
            }
            let a = ago(now, t.since);
            if !a.is_empty() {
                parts.push(format!("came {a}"));
            }
            match t.pos {
                Some(p) if on_map(g, p) => parts.push(format!("at {}, {}, z {}", p.x, p.y, p.z)),
                Some(_) => parts.push("off the map".into()),
                None => {}
            }
            g.font.draw(&clip(&parts.join(", "), lc.saturating_sub(8)), left.x + 2.0 * GW * s, ry + lh, s, col::TEXT_DIM);
            let bw = 5.0 * GW * s;
            let st = if t.pos.map(|p| on_map(g, p)).unwrap_or(false) || t.units.iter().any(|&id| g.sim.world.unit(id).map(|u| on_map(g, u.pos)).unwrap_or(false)) { BtnState::Normal } else { BtnState::Disabled };
            if text_button(&mut g.ui, &g.font, Rect::new(left.x + left.w - bw, ry, bw, GH * s + 6.0 * s), "Go", st) {
                go = Some((t.pos, t.units.first().copied().unwrap_or(-1)));
            }
        }
    }
    // Megabeasts that came.
    y = left.y + lh + 4.0 * s + list_h + 8.0 * s;
    draw_line(left.x, y - 4.0 * s, left.x + left.w, y - 4.0 * s, 1.0, col::BORDER);
    let bs = g.r13.beasts.clone();
    let rep = g.sim.megabeast_report();
    g.font.draw(&format!("Megabeasts, titans and semi-megabeasts ({} came, {} slain)", bs.len(), bs.iter().filter(|b| !b.alive).count()), left.x, y, s, col::BORDER_HI);
    y += lh + 4.0 * s;
    if bs.is_empty() {
        g.font.draw("None have come yet. They come when the fortress grows rich and populous.", left.x, y, s, col::TEXT_DIM);
    } else {
        let area = Rect::new(left.x, y, left.w, left.y + left.h - y);
        let vis = ((area.h / lh) as usize).max(1);
        g.r13.scroll2 = scroll(g, area, g.r13.scroll2, bs.len(), vis);
        for (k, b) in bs.iter().enumerate().skip(g.r13.scroll2).take(vis) {
            let ry = y + (k - g.r13.scroll2) as f32 * lh;
            let fate = if b.alive {
                "alive".to_string()
            } else if b.slain_by >= 0 {
                format!("slain by {}", unit_label(g, b.slain_by))
            } else {
                "slain".to_string()
            };
            let kills = if b.kills > 0 { format!(", {} kills", b.kills) } else { String::new() };
            let name = if b.name.is_empty() { pretty(&b.race) } else { b.name.clone() };
            let a = ago(now, b.arrived);
            let came = if a.is_empty() { String::new() } else { format!(" came {a}") };
            let t = format!("{name} ({}, {}){came}: {fate}{kills}", threat_label(b.kind).to_lowercase(), pretty(&b.race).to_lowercase());
            if list_row(&mut g.ui, Rect::new(left.x - 4.0 * s, ry - 1.0 * s, left.w + 4.0 * s, lh), false) && b.alive {
                go = Some((None, b.unit_id));
            }
            g.font.draw(&clip(&t, lc), left.x, ry, s, if b.alive { threat_color(b.kind) } else { col::TEXT_DIM });
        }
    }
    // Right side: sky, toggles, counters.
    let rx = c.x + lw + 12.0 * s;
    let rw = c.x + c.w - rx;
    let rc = (rw / (GW * s)) as usize;
    let mut y = top;
    g.font.draw("Sky and moon", rx, y, s, col::BORDER_HI);
    y += lh + 4.0 * s;
    let w = g.r13.weather.clone().unwrap_or_default();
    let (gl, gcol) = weather_glyph(w.kind);
    let mut lines: Vec<(String, Color)> = vec![(format!("{gl} {}, {}", if w.enabled { w.kind.label() } else { "no weather" }, w.season), gcol), (format!("Surface {} ({} °U)", temp_text(w.temperature), w.temperature), col::TEXT)];
    if w.snow_cover || w.frozen {
        lines.push((format!("{}{}", if w.snow_cover { "Snow on the ground. " } else { "" }, if w.frozen { "Water is frozen." } else { "" }), Color::from_rgba(220, 235, 255, 255)));
    }
    if let Some(e) = &g.r13.evil {
        let what = if e.name.is_empty() { e.kind.label().to_string() } else { e.name.clone() };
        let eff = match e.effect.as_str() {
            "HUSK" => " (turns creatures into husks)",
            "THRALL" => " (enthralls creatures)",
            "SYNDROME" => " (causes a syndrome)",
            "BLOOD" => " (stains)",
            _ => "",
        };
        lines.push((format!("Evil weather: {what}{eff}"), Color::from_rgba(220, 110, 230, 255)));
    }
    let moon = g.r13.moon;
    lines.push((format!("Moon: {} ({}/28)", moon_name(moon), moon), if moon == 14 { Color::from_rgba(250, 240, 180, 255) } else { col::TEXT }));
    if moon == 14 {
        lines.push(("Werebeasts take their animal form tonight.".into(), Color::from_rgba(220, 120, 230, 255)));
    }
    for (l, cc) in lines {
        for part in super::round9::wrap(&l, rc) {
            g.font.draw(&part, rx, y, s, cc);
            y += lh;
        }
    }
    y += 8.0 * s;
    g.font.draw("World events", rx, y, s, col::BORDER_HI);
    y += lh + 4.0 * s;
    let toggles: [(&str, bool, u8); 6] = [
        ("Weather", w.enabled, 0),
        ("Corpses rise (reanimating region)", g.sim.reanimation_enabled(), 1),
        ("Megabeasts and titans", g.sim.megabeasts_enabled(), 2),
        ("Vampires", g.sim.vampires_enabled(), 3),
        ("Werebeasts", g.sim.werebeasts_enabled(), 4),
        ("Diplomats and demands", g.sim.diplomacy_enabled(), 5),
    ];
    let mut flip: Option<(u8, bool)> = None;
    for (l, on, k) in toggles {
        if checkbox(&mut g.ui, &g.font, rx, y, on, l) {
            flip = Some((k, !on));
        }
        y += lh + 4.0 * s;
    }
    if let Some((k, on)) = flip {
        match k {
            0 => g.sim.set_weather_enabled(on),
            1 => g.sim.set_reanimation_enabled(on),
            2 => g.sim.set_megabeasts_enabled(on),
            3 => g.sim.set_vampires_enabled(on),
            4 => g.sim.set_werebeasts_enabled(on),
            _ => g.sim.set_diplomacy_enabled(on),
        }
        g.r13.polled = None;
    }
    y += 6.0 * s;
    g.font.draw("Counters", rx, y, s, col::BORDER_HI);
    y += lh + 4.0 * s;
    let (u, v, wr) = (g.sim.undead_report(), g.sim.vampire_report(), g.sim.weather_report());
    let rows = [
        format!("Undead: {} risen, {} destroyed, {} necromancer sieges", u.reanimated, u.destroyed, u.necro_sieges),
        format!("Husks {}, thralls {}", u.husks, u.thralls),
        format!("Vampires: {} came, {} feedings, {} drained, {} unmasked", v.vampires_arrived, v.feedings, v.drained, v.unmasked),
        format!("Werebeasts: {} attacks, {} transformations, {} infected", v.were_attacks, v.transformations, v.infected),
        format!("Megabeasts: {} attacks, {} slain, {} fire breaths, {} buildings destroyed", rep.attacks, rep.slain, rep.fire_breaths, rep.buildings_destroyed),
        format!("Weather: rain {} d, snow {} d, fog {} d, evil {} d; {} freezes, {} thaws", wr.rain_ticks / DAY, wr.snow_ticks / DAY, wr.fog_ticks / DAY, wr.evil_ticks / DAY, wr.freezes, wr.thaws),
    ];
    for l in rows {
        for part in super::round9::wrap(&l, rc) {
            if y + lh > c.y + c.h {
                break;
            }
            g.font.draw(&part, rx, y, s, col::TEXT_DIM);
            y += lh;
        }
    }
    if let Some((p, unit)) = go {
        let pos = p.or_else(|| g.sim.world.unit(unit).map(|u| u.pos));
        g.screen = Screen::None;
        if let Some(p) = pos {
            g.cam.focus(p);
        }
        if unit >= 0 && g.sim.world.unit(unit).is_some() {
            g.select_unit(unit);
            g.unit_tab = 0;
        } else if let Some(p) = pos {
            g.selected = Selected::Tile(p);
        }
    }
}

// ====================================================================== unit sheet

/// Curse and undead markers on the unit sheet's Overview tab. Returns the new y.
pub fn unit_markers(g: &mut Game, u: &rf_core::Unit, x: f32, mut y: f32, bw: f32) -> f32 {
    let Some(f) = g.sim.unit_curse(u.id) else { return y };
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let maxc = (bw / (GW * s)) as usize;
    let undead_c = Color::from_rgba(170, 220, 150, 255);
    let night_c = Color::from_rgba(220, 120, 230, 255);
    let mut lines: Vec<(String, Color)> = Vec::new();
    if f.necromancer {
        lines.push(("Necromancer: raises the dead, does not age.".into(), undead_c));
    }
    if f.undead {
        lines.push((if f.intelligent_undead { "Intelligent undead: keeps its mind." } else { "Undead: a risen corpse, feels no pain." }.into(), undead_c));
    }
    if f.husk {
        lines.push(("Husk: an empty shell made by an evil cloud.".into(), undead_c));
    }
    if f.thrall {
        lines.push(("Thrall: enslaved by an evil cloud.".into(), undead_c));
    }
    if f.vampire && f.vampire_unmasked {
        let tn = if u.true_name.is_empty() { String::new() } else { format!(" True name: {}.", u.true_name) };
        lines.push((format!("Vampire (unmasked): drinks blood, never eats or sleeps.{tn}"), night_c));
    } else if f.vampire_suspected {
        lines.push(("Suspected of vampirism (dead dwarves drained of blood).".into(), night_c));
    }
    if f.werebeast && f.transformed {
        let r = if f.were_race.is_empty() { "a beast".into() } else { pretty(&f.were_race).to_lowercase() };
        lines.push((format!("Werebeast: transformed into {r} under the full moon."), night_c));
    }
    if f.megabeast {
        lines.push(("Megabeast: an ancient monster.".into(), Color::from_rgba(255, 150, 60, 255)));
    }
    for (l, c) in lines {
        for part in super::round9::wrap(&l, maxc) {
            g.font.draw(&part, x, y, s, c);
            y += lh;
        }
    }
    if f.vampire_suspected && !f.vampire_unmasked {
        let r = Rect::new(x, y, bw, GH * s + 8.0 * s);
        if text_button(&mut g.ui, &g.font, r, "Accuse of vampirism (justice)", BtnState::Normal) {
            let hit = g.sim.accuse_vampire(u.id);
            g.set_status(if hit { format!("{} is a vampire and has been unmasked!", super::panels::unit_name(u)) } else { format!("{} is not a vampire.", super::panels::unit_name(u)) });
            g.r13.polled = None;
        }
        y += r.h + 4.0 * s;
    }
    y
}

// ====================================================================== map overlay

fn hash2(x: i16, y: i16) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// Weather on the outside tiles of the shown level, and snow cover.
pub fn draw_map_overlay(g: &mut Game) {
    let Some(w) = g.r13.weather.clone() else { return };
    let evil = g.r13.evil.clone();
    let sky = w.enabled && (w.kind != WeatherKind::Clear || evil.is_some());
    if !sky && !w.snow_cover {
        return;
    }
    let z = g.cam.z as i16;
    let tp = g.cam.tile_px();
    let t = get_time() as f32;
    let (x0, y0, x1, y1) = crate::render::visible_range(g);
    let fine = tp >= 10.0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = Pos::new(x, y, z);
            let Some(tile) = g.sim.world.tile(p) else { continue };
            if tile.hidden() || tile.flags & rf_core::tile_flags::OUTSIDE == 0 {
                continue;
            }
            let r = crate::render::tile_rect(g, x, y);
            if w.snow_cover && g.sim.snow_at(p) {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.93, 0.95, 1.0, 0.42));
            }
            if !sky {
                continue;
            }
            let mut k = g.sim.weather_at(p);
            if k == WeatherKind::Clear {
                // The map-wide weather over open sky (`weather_at` may only know the cloud).
                k = match w.kind {
                    WeatherKind::EvilCloud => WeatherKind::Clear,
                    other => other,
                };
                if let Some(c) = evil.as_ref().and_then(|e| e.center.map(|c| (c, e.radius, e.kind))) {
                    let d = (c.0.x as i32 - x as i32).abs().max((c.0.y as i32 - y as i32).abs());
                    if d <= c.1 {
                        k = c.2;
                    }
                }
            }
            let h = hash2(x, y);
            let hf = (h & 0xFFFF) as f32 / 65536.0;
            let hx = ((h >> 16) & 0xFF) as f32 / 256.0;
            match k {
                WeatherKind::Clear => {}
                WeatherKind::Cloudy => draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.05, 0.10)),
                WeatherKind::Fog => draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.75, 0.76, 0.78, 0.30 + 0.06 * (t * 0.4 + hf * 6.0).sin())),
                WeatherKind::EvilCloud => draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.55, 0.15, 0.60, 0.30 + 0.08 * (t * 0.6 + hf * 6.0).sin())),
                WeatherKind::Rain | WeatherKind::EvilRain => {
                    let c = if k == WeatherKind::Rain { Color::new(0.55, 0.70, 1.0, 0.75) } else { Color::new(0.85, 0.12, 0.10, 0.80) };
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.02, 0.08, 0.10));
                    if fine {
                        let ph = (t * 2.2 + hf).fract();
                        let dx = r.x + hx * r.w;
                        let dy = r.y + ph * r.h;
                        draw_line(dx, dy, dx - r.w * 0.12, dy + r.h * 0.38, (tp / 22.0).max(1.0), c);
                    }
                }
                WeatherKind::Snow => {
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.85, 0.88, 0.95, 0.08));
                    if fine {
                        let ph = (t * 0.45 + hf).fract();
                        let dx = r.x + (hx + 0.12 * (t * 1.3 + hf * 9.0).sin()).rem_euclid(1.0) * r.w;
                        let dy = r.y + ph * r.h;
                        draw_circle(dx, dy, (tp / 14.0).max(1.0), Color::new(1.0, 1.0, 1.0, 0.9));
                    }
                }
            }
        }
    }
}

// ====================================================================== --ui states

/// `--ui` states of round 13 (screenshots). Returns false for unknown keys.
pub fn ui_arg(g: &mut Game, k: &str, v: &str) -> bool {
    match k {
        "diplomacy" => {
            g.toggle_screen(Screen::None);
            open_diplomacy(g, Some(v.parse().unwrap_or(0)));
        }
        "threats" => {
            g.toggle_screen(Screen::None);
            open_threats(g);
        }
        "r13-demo" => demo(g),
        "r13-weather" => {
            let kind = match v.to_ascii_lowercase().as_str() {
                "snow" => WeatherKind::Snow,
                "fog" => WeatherKind::Fog,
                "cloudy" => WeatherKind::Cloudy,
                "evilrain" => WeatherKind::EvilRain,
                "evilcloud" => WeatherKind::EvilCloud,
                "clear" => WeatherKind::Clear,
                _ => WeatherKind::Rain,
            };
            g.sim.set_weather_enabled(true);
            g.sim.force_weather(kind, 10 * DAY);
            g.r13.polled = None;
        }
        "r13-popup" => {
            demo(g);
            if let Some(d) = g.r13.demo_demands.iter().find(|d| d.answer.is_none()).cloned() {
                update(g);
                let who = entity_name(g, d.entity_id);
                g.r13.popup = Some((format!("{} from {who}", demand_label(d.kind)), d.text.clone(), d.id));
            }
        }
        "r13-unit" => {
            demo(g);
            if let Some(id) = g.r13.demo_threats.iter().find(|t| t.kind == ThreatKind::NecromancerSiege).and_then(|t| t.units.first().copied()) {
                g.select_unit(id);
                g.unit_tab = 0;
                if let Some(p) = g.sim.world.unit(id).map(|u| u.pos) {
                    g.cam.focus(p);
                    g.cam.x = g.cam.tx;
                    g.cam.y = g.cam.ty;
                }
            }
        }
        _ => return false,
    }
    true
}

/// Sample data for screenshots while the round-13 modules are stubs: threats around real
/// non-citizen units (two of them marked undead / necromancer through the rf-core fields the
/// API reads), demands and meetings of the first civilizations, two megabeasts.
fn demo(g: &mut Game) {
    if g.r13.demo {
        return;
    }
    g.r13.demo = true;
    let now = g.sim.ticks();
    let civ = g.sim.world.meta.civ_id;
    let ents = g.sim.diplomacy_entities();
    let own = ents.iter().find(|e| e.id == civ || e.relation == "Own").map(|e| e.id).unwrap_or(-1);
    let others: Vec<i32> = ents.iter().filter(|e| e.id != own).map(|e| e.id).take(3).collect();
    let other = |k: usize| others.get(k).copied().unwrap_or(own);
    let mut pool: Vec<(i32, Pos, String)> = g.sim.world.units.iter().filter(|u| !u.flags.dead && u.civ_id != civ && on_map(g, u.pos)).map(|u| (u.id, u.pos, u.race.clone())).take(6).collect();
    if pool.is_empty() {
        pool = g.sim.world.units.iter().filter(|u| !u.flags.dead).map(|u| (u.id, u.pos, u.race.clone())).take(6).collect();
    }
    let unit = |k: usize| pool.get(k).cloned();
    let mut threats = Vec::new();
    if let Some((id, p, _)) = unit(0) {
        if let Some(u) = g.sim.world.units.iter_mut().find(|u| u.id == id) {
            u.curse = "NECROMANCER".into();
        }
        let undead: Vec<i32> = pool.iter().skip(1).take(2).map(|e| e.0).collect();
        for &uid in &undead {
            if let Some(u) = g.sim.world.units.iter_mut().find(|u| u.id == uid) {
                u.undead = true;
            }
        }
        let mut units = vec![id];
        units.extend(undead);
        threats.push(Threat { kind: ThreatKind::NecromancerSiege, name: "Ngom Ashenthrall".into(), race: "HUMAN".into(), units, pos: Some(p), since: now.saturating_sub(2 * DAY), entity_id: -1 });
    }
    if let Some((id, p, r)) = unit(3) {
        threats.push(Threat { kind: ThreatKind::Megabeast, name: "Uthros the Fiery".into(), race: if r.is_empty() { "DRAGON".into() } else { "DRAGON".into() }, units: vec![id], pos: Some(p), since: now.saturating_sub(DAY / 2), entity_id: -1 });
    }
    if let Some((id, p, _)) = unit(4) {
        threats.push(Threat { kind: ThreatKind::Werebeast, name: "A werewolf".into(), race: "WEREWOLF".into(), units: vec![id], pos: Some(p), since: now, entity_id: -1 });
    }
    threats.push(Threat { kind: ThreatKind::Siege, name: "Goblin siege".into(), race: "GOBLIN".into(), units: vec![], pos: None, since: now.saturating_sub(5 * DAY), entity_id: other(0) });
    g.r13.demo_threats = threats;
    g.r13.demo_beasts = vec![
        MegabeastInfo { unit_id: unit(3).map(|e| e.0).unwrap_or(-1), name: "Uthros the Fiery".into(), race: "DRAGON".into(), kind: ThreatKind::Megabeast, arrived: now.saturating_sub(DAY / 2), alive: true, kills: 2, slain_by: -1 },
        MegabeastInfo { unit_id: -1, name: "Rimtar Bronzehulk".into(), race: "COLOSSUS_BRONZE".into(), kind: ThreatKind::Megabeast, arrived: now.saturating_sub(120 * DAY), alive: false, kills: 5, slain_by: g.sim.world.citizens().next().map(|u| u.id).unwrap_or(-1) },
    ];
    g.r13.demo_demands = vec![
        Demand { id: 9001, kind: DemandKind::TreeQuota, entity_id: other(0), text: "The diplomat demands that you fell no more than 100 trees this year.".into(), amount: 100, due: now + 336 * DAY, answer: None },
        Demand { id: 9002, kind: DemandKind::Tribute, entity_id: other(1), text: "The human envoy asks for tribute to keep the roads safe.".into(), amount: 2500, due: now + 28 * DAY, answer: None },
        Demand { id: 9003, kind: DemandKind::Peace, entity_id: other(2), text: "They offer to end the war.".into(), amount: 0, due: 0, answer: Some(true) },
    ];
    let envoy = g.sim.world.units.iter().find(|u| !u.flags.dead && u.civ_id != civ && !u.flags.animal).map(|u| u.id).unwrap_or(-1);
    let host = g.sim.world.citizens().next().map(|u| u.id).unwrap_or(-1);
    g.r13.demo_meetings = vec![
        Meeting { id: 1, kind: MeetingKind::Diplomat, entity_id: other(0), envoy, host, state: "InProgress".into(), tick: now.saturating_sub(DAY), text: "The diplomat speaks of the trees of the forest.".into(), demands: vec![9001] },
        Meeting { id: 2, kind: MeetingKind::Liaison, entity_id: own, envoy: -1, host, state: "Done".into(), tick: now.saturating_sub(40 * DAY), text: "The outpost liaison took the requests for next year's caravan.".into(), demands: vec![] },
    ];
    g.r13.polled = None;
    update(g);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moon_and_temperature_words() {
        assert_eq!(moon_name(0), "New moon");
        assert_eq!(moon_name(14), "Full moon");
        assert_eq!(moon_name(7), "First quarter");
        assert_eq!(moon_name(27), "Waning crescent");
        assert_eq!(temp_text(10_000), "0°C");
        assert_eq!(temp_text(10_050), "27°C");
        assert_eq!(temp_text(0), "?°C");
    }

    #[test]
    fn labels_cover_every_kind() {
        for k in [ThreatKind::Megabeast, ThreatKind::SemiMegabeast, ThreatKind::Titan, ThreatKind::ForgottenBeast, ThreatKind::Siege, ThreatKind::UndeadHorde, ThreatKind::NecromancerSiege, ThreatKind::Werebeast, ThreatKind::Vampire] {
            assert!(!threat_label(k).is_empty());
        }
        for k in [DemandKind::Request, DemandKind::TreeQuota, DemandKind::Tribute, DemandKind::Peace] {
            assert!(!demand_label(k).is_empty());
            assert!(!demand_hint(&Demand { kind: k, amount: 5, ..Default::default() }).is_empty());
        }
        assert_eq!(title_text(""), "Outpost");
        assert_eq!(title_text("Barony"), "Barony");
        assert_eq!(ago(10 * DAY, 7 * DAY), "3 days ago");
        assert_eq!(ago(10, 0), "");
    }
}
