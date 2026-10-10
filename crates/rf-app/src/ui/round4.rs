//! [round 4] State and glue of the round-4 screens: caravan arrival (announcement and
//! pop-up box like DF), population info, the settings page of the Esc menu. The screens
//! themselves live in `trade.rs`, `nobles.rs` and `devices.rs`.

use super::{col, panel, text_button, BtnState};
use crate::font::GH;
use crate::game::{date_string, Game, LogLine, MenuPage, Screen};
use macroquad::prelude::*;
use rf_core::Pos;
use rf_sim::{Announcement, AnnouncementKind, PopulationInfo, TradeOffer, TradeStatus};

/// How often (real seconds) the caravan state is polled when the trade screen is closed.
const TRADE_POLL_S: f64 = 0.5;

/// A DF announcement box: the game is paused while it is open.
#[derive(Clone, Debug)]
pub struct Popup {
    pub title: String,
    pub text: String,
    /// Offer a "Trade" button (caravan arrival).
    pub trade: bool,
    pub pos: Option<Pos>,
}

#[derive(Default)]
pub struct R4Ui {
    /// Last polled `Sim::trade_status()` and the real time of the poll.
    pub trade: Option<TradeStatus>,
    pub trade_polled: f64,
    /// A caravan was present at the last poll (arrival = it appears).
    pub caravan_here: bool,
    /// The first poll after start/load happened (a caravan already there is not announced,
    /// like loading a save in DF).
    pub polled_once: bool,
    /// The sim announced the caravan itself in the last batch (no duplicate line).
    pub sim_announced_caravan: bool,
    pub popup: Option<Popup>,
    /// Trade screen: fort items offered, merchant items requested.
    pub offer: Vec<i32>,
    pub request: Vec<i32>,
    /// Merchants' evaluation of the current selection (refreshed when it changes).
    pub eval: Option<TradeOffer>,
    pub eval_key: (Vec<i32>, Vec<i32>),
    /// Last merchant reply and whether the deal went through.
    pub reply: Option<(String, bool)>,
    pub scroll: [usize; 2],
    /// "Move goods to depot" picker.
    pub depot_open: bool,
    pub depot_sel: Vec<i32>,
    pub depot_scroll: usize,
    pub depot_filter: String,
    pub depot_typing: bool,
    /// Items already sent to the depot this session (shown as "on the way").
    pub depot_sent: Vec<i32>,
    /// Cached picker list and its (filter, real time).
    pub depot_cache: Vec<i32>,
    pub depot_cache_key: (String, f64),
    /// Nobles screen.
    pub nobles_scroll: usize,
    /// Position token whose appoint picker is open.
    pub appoint: Option<String>,
    pub appoint_scroll: usize,
    /// `--ui fake-caravan`: a made-up caravan for layout tests while the sim has none.
    pub debug_caravan: Option<TradeStatus>,
}

/// The flags of one entry of a DF `announcements.txt` (`[KIND:BOX:A_D:...]`). [app9] The
/// files are read by `round9::ensure_ann` for every announcement type.
pub fn parse_announcement_flags(text: &str, kind: &str) -> Option<Vec<String>> {
    let tag = format!("[{kind}:");
    let line = text.lines().find(|l| l.trim_start().starts_with(&tag))?;
    let inner = line.trim().trim_start_matches('[').split(']').next()?;
    Some(inner.split(':').skip(1).map(|s| s.to_string()).collect())
}

fn is_caravan_text(t: &str) -> bool {
    let l = t.to_lowercase();
    l.contains("caravan") || l.contains("merchants")
}

/// Called with each batch of sim announcements before they go to the log.
pub fn on_announcements(g: &mut Game, fresh: &[Announcement]) {
    if fresh.iter().any(|a| is_caravan_text(&a.text)) {
        g.r4.sim_announced_caravan = true;
    }
}

/// Re-read the caravan state (throttled unless forced).
pub fn poll_trade(g: &mut Game, force: bool) {
    let now = get_time();
    if !force && now - g.r4.trade_polled < TRADE_POLL_S {
        return;
    }
    g.r4.trade_polled = now;
    g.r4.trade = g.sim.trade_status().or_else(|| g.r4.debug_caravan.clone());
}

/// `--ui fake-caravan`: merchants "offer" some of the fort's own crafts (layout tests only;
/// trading still goes through the sim, which refuses without a real caravan).
pub fn debug_caravan(g: &mut Game) {
    let w = &g.sim.world;
    let depot = w.buildings.iter().find(|b| b.kind == "TradeDepot").map(|b| b.id).unwrap_or(-1);
    let civ = w.entities.iter().find(|e| e.trades && e.race == "DWARF").or_else(|| w.entities.iter().find(|e| e.kind == "Civilization" && e.relation == "Own"));
    let goods: Vec<i32> = w
        .items
        .iter()
        .filter(|it| it.container < 0 && it.value > 0 && !it.flags.artifact && matches!(it.kind.as_str(), "CRAFTS" | "GOBLET" | "INSTRUMENT" | "TOY" | "FIGURINE" | "AMULET" | "RING" | "CROWN" | "BRACELET" | "EARRING" | "SCEPTER"))
        .take(14)
        .map(|it| it.id)
        .collect();
    let fort_goods: Vec<i32> = match w.building(depot) {
        Some(d) => w.items.iter().filter(|it| d.contains(it.pos) && it.container < 0 && it.building != d.id).map(|it| it.id).collect(),
        None => Vec::new(),
    };
    g.r4.debug_caravan = Some(TradeStatus {
        civ: civ.map(|e| e.name.clone()).unwrap_or_else(|| "the mountainhomes".into()),
        race: civ.map(|e| e.race.clone()).unwrap_or_else(|| "DWARF".into()),
        depot,
        broker_present: true,
        goods,
        fort_goods,
        leaves_in: rf_core::TICKS_PER_DAY as u64 * 9,
    });
    poll_trade(g, true);
}

/// Once per frame: caravan arrival like DF ("A caravan from X has arrived.").
pub fn update(g: &mut Game) {
    poll_trade(g, g.screen == Screen::Trade || !g.r4.polled_once);
    let here = g.r4.trade.is_some();
    if here && !g.r4.caravan_here && g.r4.polled_once {
        caravan_arrived(g);
    }
    g.r4.polled_once = true;
    g.r4.caravan_here = here;
    g.r4.sim_announced_caravan = false;
}

fn caravan_arrived(g: &mut Game) {
    let Some(t) = g.r4.trade.clone() else { return };
    let civ = if t.civ.is_empty() { "the mountainhomes".to_string() } else { t.civ.clone() };
    let text = format!("A caravan from {civ} has arrived.");
    let pos = g.sim.world.building(t.depot).map(|b| b.center);
    // [round 5] The sim's own arrival line can come a few frames before the (throttled)
    // caravan poll notices the caravan: do not log it twice.
    let now = get_time();
    let logged = g.log.iter().rev().take(200).any(|l| now - l.born < 2.0 && is_caravan_text(&l.text));
    if !g.r4.sim_announced_caravan && !logged {
        let date = date_string(&g.sim.world.calendar);
        g.log.push(LogLine::new(text.clone(), pos, AnnouncementKind::Warning, get_time(), date));
    }
    // [app9] The announcement settings (DF CARAVAN_ARRIVAL flags, changeable in the game).
    let f = super::round9::flags_by_id(g, "CARAVAN");
    if f.popup {
        g.paused = true;
        let race = if t.race.is_empty() { String::new() } else { format!(" ({})", super::pretty(&t.race).to_lowercase()) };
        g.r4.popup = Some(Popup { title: "Caravan".into(), text: format!("{text}{race}"), trade: true, pos });
    } else if f.pause {
        g.paused = true;
    }
    if f.recenter {
        if let Some(p) = pos {
            g.cam.focus(p);
        }
    }
}

/// The announcement box (modal, drawn above everything).
pub fn draw_popup(g: &mut Game) {
    let Some(p) = g.r4.popup.clone() else { return };
    let s = g.ui.scale;
    let (sw, sh) = (screen_width(), screen_height());
    let bw = 16.0 * 8.0 * s;
    let gap = 16.0 * s;
    let n = if p.trade { 3.0 } else { 1.0 };
    let tw = (g.font.width(&p.text, s) + 60.0 * s).max(n * bw + (n - 1.0) * gap + 48.0 * s);
    let (w, h) = (tw.clamp(420.0 * s, sw - 40.0), 132.0 * s);
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.5, w, h);
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.25));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.04, 0.04, 1.0));
    panel(&mut g.ui, r);
    let mut y = r.y + 14.0 * s;
    g.font.draw(&p.title, r.x + (w - g.font.width(&p.title, 1.5 * s)) / 2.0, y, 1.5 * s, col::TEXT_HI);
    y += 1.5 * GH * s + 12.0 * s;
    g.font.draw(&p.text, r.x + (w - g.font.width(&p.text, s)) / 2.0, y, s, col::TEXT);
    let bh = 26.0 * s;
    let by = r.y + r.h - bh - 14.0 * s;
    let mut bx = r.x + (w - n * bw - (n - 1.0) * gap) / 2.0;
    if p.trade {
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Trade", BtnState::Normal) {
            g.r4.popup = None;
            open_trade(g);
        }
        bx += bw + gap;
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Go to depot", BtnState::Normal) {
            if let Some(pos) = p.pos {
                g.cam.focus(pos);
            }
            g.r4.popup = None;
        }
        bx += bw + gap;
    }
    if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "OK (Enter)", BtnState::Normal) {
        g.r4.popup = None;
    }
    g.ui.block(Rect::new(0.0, 0.0, sw, sh));
    if g.ui.lpressed {
        g.ui.click_used = true;
    }
}

/// Open the trade screen (from the depot, the caravan box or `--ui trade`).
pub fn open_trade(g: &mut Game) {
    g.toggle_screen(Screen::None);
    g.screen = Screen::Trade;
    g.r4.offer.clear();
    g.r4.request.clear();
    g.r4.eval = None;
    g.r4.eval_key = (Vec::new(), Vec::new());
    g.r4.reply = None;
    g.r4.scroll = [0, 0];
    poll_trade(g, true);
}

/// Population line for the top bar tooltip and the creatures screen header.
pub fn population_lines(g: &Game) -> Vec<String> {
    let p: PopulationInfo = g.sim.population_info();
    let mut v = Vec::new();
    let adults = p.citizens.saturating_sub(p.children);
    v.push(format!("Population: {} citizens ({} adults, {} children)", p.citizens, adults, p.children));
    let cap = if p.population_cap > 0 { format!("{}", p.population_cap) } else { "none".into() };
    v.push(format!("Population cap: {cap}"));
    v.push(match p.next_wave_in {
        Some(t) => format!("Next migrant wave check in {}", ticks_span(t)),
        None => "No migrant wave expected".into(),
    });
    v.push(match &p.caravan_expected {
        Some(c) => format!("Caravan expected: {c}"),
        None => "No caravan expected".into(),
    });
    v
}

/// "3 days", "2 months", "5 hours" from ticks (DF: 1200 ticks per day, 28 days per month).
pub fn ticks_span(t: u64) -> String {
    let day = rf_core::TICKS_PER_DAY as u64;
    let month = rf_core::TICKS_PER_MONTH as u64;
    if t >= 2 * month {
        format!("{} months", t / month)
    } else if t >= 2 * day {
        format!("{} days", t / day)
    } else if t >= day {
        "1 day".into()
    } else {
        let h = (t * 24 / day).max(1);
        format!("{h} hour{}", if h == 1 { "" } else { "s" })
    }
}

/// Settings page of the Esc menu: population cap and invasions.
pub fn draw_settings(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    let (w, h) = ((460.0 * s).min(sw - 40.0), (346.0 * s).min(sh - 60.0));
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    panel(&mut g.ui, r);
    let pad = 16.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    g.font.draw("Settings", x, y, 1.5 * s, col::TEXT_HI);
    y += 1.5 * GH * s + 14.0 * s;
    // [round 6] Autosave interval (also on the title screen's settings page).
    {
        g.font.draw("Autosave", x, y + 5.0 * s, s, col::TEXT);
        let bh = GH * s + 10.0 * s;
        let label = crate::settings::autosave_label(g.autosave_days);
        let br = Rect::new(x + 17.0 * 8.0 * s, y, 18.0 * 8.0 * s, bh);
        if text_button(&mut g.ui, &g.font, br, &label, BtnState::Normal) {
            let mut st = crate::settings::Settings { autosave_days: g.autosave_days, ..Default::default() };
            st.cycle_autosave();
            g.set_autosave_days(st.autosave_days);
        }
        y += bh + 4.0 * s;
        let note = match g.autosave_next {
            Some(d) => {
                let left = d - crate::game::game_day(&g.sim.world.calendar);
                format!("Next in {left} days; keeps the last {} autosaves.", g.settings.autosave_keep)
            }
            None => "No autosaves; save with F5 or the menu.".into(),
        };
        g.font.draw(&note, x, y, s, col::TEXT_DIM);
        y += lh + 12.0 * s;
    }
    // Population cap: DF d_init POPULATION_CAP; migrants stop arriving at the cap.
    let cap = g.sim.population_info().population_cap;
    g.font.draw("Population cap", x, y + 5.0 * s, s, col::TEXT);
    let bw = 34.0 * s;
    let bh = GH * s + 10.0 * s;
    let mut bx = x + 17.0 * 8.0 * s;
    let mut new_cap = None;
    for (label, d) in [("-10", -10), ("-1", -1)] {
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y, bw, bh), label, BtnState::Normal) {
            new_cap = Some((cap + d).max(0));
        }
        bx += bw + 4.0 * s;
    }
    let ct = if cap > 0 { format!("{cap}") } else { "none".into() };
    let cw = 7.0 * 8.0 * s;
    g.font.draw(&ct, bx + (cw - g.font.width(&ct, s)) / 2.0, y + 5.0 * s, s, col::TEXT_HI);
    bx += cw;
    for (label, d) in [("+1", 1), ("+10", 10)] {
        if text_button(&mut g.ui, &g.font, Rect::new(bx, y, bw, bh), label, BtnState::Normal) {
            new_cap = Some(cap + d);
        }
        bx += bw + 4.0 * s;
    }
    if let Some(c) = new_cap {
        g.sim.set_population_cap(c);
    }
    y += bh + 4.0 * s;
    let pi = g.sim.population_info();
    let maxc = ((w - 2.0 * pad) / (8.0 * s)) as usize;
    let note = format!("{} citizens now; migrants stop coming at the cap.", pi.citizens);
    g.font.draw(&super::clip(&note, maxc), x, y, s, col::TEXT_DIM);
    y += lh;
    g.font.draw(&super::clip("DF: d_init POPULATION_CAP (0 = no cap).", maxc), x, y, s, col::TEXT_DIM);
    y += lh + 12.0 * s;
    // Invasions (DF world/d_init setting; sieges start at population 80).
    let inv = g.sim.invasions_enabled();
    if super::checkbox(&mut g.ui, &g.font, x, y, inv, "Invasions (sieges and ambushes)") {
        g.sim.set_invasions(!inv);
    }
    y += lh + 4.0 * s;
    g.font.draw("Off: no new sieges arrive; hostiles already here stay.", x, y, s, col::TEXT_DIM);
    y += lh + 12.0 * s;
    let st = &g.sim.world.settings;
    let mut other = Vec::new();
    if st.strict_population_cap > 0 {
        other.push(format!("strict cap {}", st.strict_population_cap));
    }
    if st.baby_cap > 0 {
        other.push(format!("baby cap {}", st.baby_cap));
    }
    if !other.is_empty() {
        g.font.draw(&format!("From your save: {}", other.join(", ")), x, y, s, col::TEXT_DIM);
    }
    let bh2 = 26.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(x, r.y + r.h - pad - bh2, 14.0 * 8.0 * s, bh2), "Back", BtnState::Normal) {
        g.menu.page = MenuPage::Main;
    }
    // [app9] DF announcements.txt: which events pop up, pause, recenter, show.
    let ab = Rect::new(x + 16.0 * 8.0 * s, r.y + r.h - pad - bh2, 20.0 * 8.0 * s, bh2);
    if text_button(&mut g.ui, &g.font, ab, "Announcements...", BtnState::Normal) {
        g.menu.page = MenuPage::Announcements;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caravan_flags_from_prefs() {
        let txt = "[CARAVAN_ARRIVAL:BOX:A_D:D_D:ALERT]\n[NOBLE_ARRIVAL:A_D:D_D]\n";
        let f = parse_announcement_flags(txt, "CARAVAN_ARRIVAL").unwrap();
        assert_eq!(f, vec!["BOX", "A_D", "D_D", "ALERT"]);
        assert!(super::super::round9::flags_from_df(&f).popup);
        let d = parse_announcement_flags("[CARAVAN_ARRIVAL:A_D:D_D:ALERT]", "CARAVAN_ARRIVAL").unwrap();
        assert!(!super::super::round9::flags_from_df(&d).popup);
        assert!(parse_announcement_flags(txt, "MERCHANTS_UNLOADING").is_none());
        assert!(is_caravan_text("Merchants have arrived and are unloading their goods."));
    }

    #[test]
    fn spans() {
        let d = rf_core::TICKS_PER_DAY as u64;
        assert_eq!(ticks_span(d * 3), "3 days");
        assert_eq!(ticks_span(rf_core::TICKS_PER_MONTH as u64 * 3), "3 months");
        assert_eq!(ticks_span(d / 2), "12 hours");
    }
}
