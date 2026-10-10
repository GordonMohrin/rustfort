//! [app8] DF Steam's Places, Justice, Objects and World screens.
//!
//! - Places (`P`): Locations (temples, taverns, libraries, guildhalls, hospitals with their
//!   occupations and an assign picker: `Sim::assign_occupation`), Zones (with "Make location":
//!   kind, then the deity for a temple: `Sim::create_location`), Stockpiles (opens the settings
//!   editor), Workshops and Farm plots.
//! - Justice (`j`): open and closed cases (`Sim::crimes`) with a convict picker
//!   (`Sim::convict`), and the convicts.
//! - Objects (`O`): the artifacts with material, where they are and who made them.
//! - World (`Y`): the civilizations with their relation to yours, and the caravans.

use super::round8::{frame, hist_fig_name, picker, scroll, tabs, unit_label, zone_label, Pick, PickRow};
use super::{clip, col, draw_icon, pretty, text_button, words, BtnState, Ico};
use crate::font::GH;
use crate::game::{Game, Screen, Selected};
use macroquad::prelude::*;
use rf_core::{Crime, Location};

/// Location kinds of DF v50 with their zone icon.
pub const LOCATION_KINDS: &[(&str, &str)] =
    &[("Tavern", "ZONE_TAVERN"), ("Temple", "ZONE_TEMPLE"), ("Library", "ZONE_LIBRARY"), ("Guildhall", "ZONE_GUILDHALL"), ("Hospital", "ZONE_HOSPITAL")];

/// Occupations a location offers in fortress mode (DF v50 wiki: tavern keeper and performers
/// in taverns, performers in temples, scholars and scribes in libraries, doctors in hospitals).
pub fn occupations(kind: &str) -> &'static [&'static str] {
    match kind {
        "Tavern" => &["TAVERN_KEEPER", "PERFORMER"],
        "Temple" => &["PERFORMER"],
        "Library" => &["SCHOLAR", "SCRIBE"],
        "Hospital" => &["DOCTOR"],
        _ => &[],
    }
}

fn occupation_icon(kind: &str) -> &'static str {
    match kind {
        "TAVERN_KEEPER" => "LOCATION_OCCUPATION_TAVERN_KEEPER",
        "PERFORMER" => "LOCATION_OCCUPATION_PERFORMER",
        "SCHOLAR" => "LOCATION_OCCUPATION_SCHOLAR",
        "SCRIBE" => "LOCATION_OCCUPATION_SCRIBE",
        "DOCTOR" => "LOCATION_OCCUPATION_DOCTOR",
        "MERCENARY" => "LOCATION_OCCUPATION_MERCENARY",
        "MONSTER_SLAYER" => "LOCATION_OCCUPATION_MONSTER_SLAYER",
        _ => "LOCATION_ASSIGN_OCCUPATION",
    }
}

fn kind_icon(kind: &str) -> &'static str {
    LOCATION_KINDS.iter().find(|(k, _)| *k == kind).map(|(_, i)| *i).unwrap_or("ZONE_MEETING")
}

/// The location a zone belongs to.
pub fn location_of_zone(g: &Game, zone: i32) -> Option<&Location> {
    g.sim.world.locations.iter().find(|l| l.zones.contains(&zone))
}

/// Deities worshipped by citizens, most worshippers first (DF's temple dedication list).
pub fn deities(g: &Game) -> Vec<(i32, usize)> {
    let mut v: Vec<(i32, usize)> = Vec::new();
    for u in g.sim.world.citizens() {
        for r in u.relationships.iter().filter(|r| r.kind == "Deity" || r.kind == "Worship") {
            match v.iter_mut().find(|(id, _)| *id == r.hist_fig_id) {
                Some(e) => e.1 += 1,
                None => v.push((r.hist_fig_id, 1)),
            }
        }
    }
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

fn btn_h(g: &Game) -> f32 {
    GH * g.ui.scale + 8.0 * g.ui.scale
}

fn icon_or(g: &mut Game, name: &str, r: Rect, fb: Ico) {
    if !draw_icon(&g.gfx, &mut g.rend, name, r, WHITE) {
        super::draw_fallback_icon(&g.font, fb, r.x, r.y, r.w, r.h);
    }
}

// ------------------------------------------------------------------ Places

const PLACE_TABS: &[&str] = &["Locations", "Zones", "Stockpiles", "Workshops", "Farm plots"];

pub fn draw_places(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(c) = frame(g, "Places", 1300.0) else { return };
    let (hit, _) = tabs(g, super::round8::tabs_x(g, c, "Places"), c.y - 3.0 * s, PLACE_TABS, g.r8.pl_tab.min(PLACE_TABS.len() - 1));
    if let Some(i) = hit {
        g.r8.pl_tab = i;
        g.r8.pl_scroll = 0;
        g.r8.pl_make = None;
        g.r8.pl_kind = None;
        g.r8.pl_occ = None;
    }
    let top = c.y + 2.0 * lh + 4.0 * s;
    draw_line(c.x, top - 6.0 * s, c.x + c.w, top - 6.0 * s, 1.0, col::BORDER);
    let body = Rect::new(c.x, top, c.w, c.y + c.h - top);
    match g.r8.pl_tab {
        0 => draw_locations(g, body),
        1 => draw_zones(g, body),
        2 => draw_buildings(g, body, |k| k == "Stockpile"),
        3 => draw_buildings(g, body, |k| k == "Workshop" || k == "Furnace"),
        _ => draw_buildings(g, body, |k| k == "FarmPlot"),
    }
    // Pickers on top.
    if let Some(zone) = g.r8.pl_make {
        make_location_picker(g, c, zone);
    }
    if let Some((loc, occ)) = g.r8.pl_occ.clone() {
        let mut rows = super::round8::citizen_rows(g);
        // Citizens already holding an occupation go last.
        let busy: Vec<i32> = g.sim.world.locations.iter().flat_map(|l| l.occupations.iter().map(|o| o.unit)).collect();
        rows.sort_by_key(|r| busy.contains(&r.id));
        for r in rows.iter_mut().filter(|r| busy.contains(&r.id)) {
            r.info = format!("{} (has an occupation)", r.info);
            r.color = col::TEXT_DIM;
        }
        let pr = Rect::new(c.x + c.w / 2.0 - 26.0 * 8.0 * s, c.y + 30.0 * s, 52.0 * 8.0 * s, c.h - 60.0 * s);
        let lname = g.sim.world.locations.iter().find(|l| l.id == loc).map(|l| l.name.clone()).unwrap_or_default();
        let (sc, pick) = picker(g, pr, &format!("{} of {}", pretty(&occ), clip(&lname, 24)), &rows, g.r8.pl_oscroll, Some("Nobody (vacant)"));
        g.r8.pl_oscroll = sc;
        match pick {
            Pick::Picked(u) => {
                g.sim.assign_occupation(loc, &occ, u);
                g.r8.pl_occ = None;
            }
            Pick::Cancel => g.r8.pl_occ = None,
            Pick::Nothing => {}
        }
    }
}

fn draw_locations(g: &mut Game, c: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let locs = g.sim.world.locations.clone();
    if locs.is_empty() {
        g.font.draw("No locations yet. Make one from a zone (meeting area) in the Zones tab:", c.x, c.y, s, col::TEXT);
        g.font.draw("a tavern, temple, library or guildhall, or a hospital.", c.x, c.y + lh, s, col::TEXT_DIM);
        if text_button(&mut g.ui, &g.font, Rect::new(c.x, c.y + 2.5 * lh, 20.0 * 8.0 * s, GH * s + 8.0 * s), "Go to Zones", BtnState::Normal) {
            g.r8.pl_tab = 1;
        }
        return;
    }
    // One block per location: header line, then one line per occupation.
    let mut blocks: Vec<(usize, f32)> = Vec::new();
    for (i, l) in locs.iter().enumerate() {
        let n = occupations(&l.kind).len().max(l.occupations.len());
        blocks.push((i, 40.0 * s + n as f32 * (lh + 8.0 * s) + 8.0 * s));
    }
    let total_h: f32 = blocks.iter().map(|b| b.1).sum();
    let avg = (total_h / blocks.len() as f32).max(1.0);
    let vis = ((c.h / avg) as usize).max(1);
    g.r8.pl_scroll = scroll(g, c, g.r8.pl_scroll, locs.len(), vis);
    let mut y = c.y;
    for (i, h) in blocks.iter().skip(g.r8.pl_scroll) {
        if y + h > c.y + c.h {
            break;
        }
        let l = &locs[*i];
        draw_rectangle(c.x, y, c.w, h - 6.0 * s, Color::new(1.0, 1.0, 1.0, 0.035));
        icon_or(g, kind_icon(&l.kind), Rect::new(c.x + 4.0 * s, y + 4.0 * s, 32.0 * s, 32.0 * s), Ico::Zone);
        g.font.draw(&l.name, c.x + 44.0 * s, y + 4.0 * s, s, col::TEXT_HI);
        let mut sub = l.kind.clone();
        if l.kind == "Temple" {
            sub = if l.deity >= 0 { format!("Temple to {}", hist_fig_name(g, l.deity)) } else { "Temple (no specific deity)".into() };
        }
        if l.kind == "Guildhall" && !l.profession.is_empty() {
            sub = format!("Guildhall of the {}", pretty(&l.profession).to_lowercase());
        }
        let zones: Vec<String> = l.zones.iter().map(|z| format!("\"{}\"", zone_label(g, *z))).collect();
        sub = format!(
            "{sub}, in zone {}, {}",
            if zones.is_empty() { "(none)".into() } else { zones.join(", ") },
            if l.allow_visitors { "visitors allowed" } else { "citizens only" }
        );
        g.font.draw(&clip(&sub, ((c.w - 60.0 * s) / (8.0 * s)) as usize), c.x + 44.0 * s, y + 4.0 * s + lh, s, col::TEXT_DIM);
        // Visitors toggle.
        let vr = Rect::new(c.x + c.w - 30.0 * s, y + 4.0 * s, 26.0 * s, 26.0 * s);
        icon_or(g, if l.allow_visitors { "LOCATION_PERMISSION_ON_VISITORS" } else { "LOCATION_PERMISSION_OFF_VISITORS" }, vr, Ico::Glyph(b'V', [200, 200, 200]));
        if g.ui.hovered(vr) {
            g.ui.tooltip = Some((if l.allow_visitors { "Visitors allowed (click: citizens only)" } else { "Citizens only (click: allow visitors)" }.into(), vec2(vr.x - 200.0, vr.y - 4.0)));
        }
        if g.ui.clicked(vr) {
            // No sim API: the flag lives in the world.
            if let Some(ll) = g.sim.world.locations.iter_mut().find(|x| x.id == l.id) {
                ll.allow_visitors = !ll.allow_visitors;
            }
        }
        let mut oy = y + 40.0 * s;
        let mut kinds: Vec<String> = occupations(&l.kind).iter().map(|k| k.to_string()).collect();
        for o in &l.occupations {
            if !kinds.contains(&o.kind) {
                kinds.push(o.kind.clone());
            }
        }
        for k in kinds {
            let holder = l.occupations.iter().find(|o| o.kind == k).map(|o| o.unit).unwrap_or(-1);
            icon_or(g, occupation_icon(&k), Rect::new(c.x + 44.0 * s, oy, 22.0 * s, lh + 4.0 * s), Ico::Smiley);
            g.font.draw(&pretty(&k), c.x + 72.0 * s, oy + 3.0 * s, s, col::TEXT);
            let hn = if holder >= 0 { unit_label(g, holder) } else { "vacant".into() };
            g.font.draw(&clip(&hn, 30), c.x + 72.0 * s + 18.0 * 8.0 * s, oy + 3.0 * s, s, if holder >= 0 { col::TEXT_CYAN } else { col::TEXT_DIM });
            let br = Rect::new(c.x + 72.0 * s + 50.0 * 8.0 * s, oy, 12.0 * 8.0 * s, lh + 4.0 * s);
            if text_button(&mut g.ui, &g.font, br, if holder >= 0 { "Replace" } else { "Assign" }, BtnState::Normal) {
                g.r8.pl_occ = Some((l.id, k.clone()));
                g.r8.pl_oscroll = 0;
            }
            oy += lh + 8.0 * s;
        }
        if occupations(&l.kind).is_empty() && l.occupations.is_empty() {
            g.font.draw("No occupations here.", c.x + 44.0 * s, oy + 3.0 * s, s, col::TEXT_DIM);
        }
        y += h;
    }
}

fn draw_zones(g: &mut Game, c: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let mut zones: Vec<rf_core::Building> = g.sim.world.buildings.iter().filter(|b| b.kind == "Civzone").cloned().collect();
    zones.sort_by(|a, b| a.subtype.cmp(&b.subtype).then(a.id.cmp(&b.id)));
    let row_h = 28.0 * s;
    let hy = c.y;
    for (x, t) in [(c.x + 34.0 * s, "Zone"), (c.x + 34.0 * s + 28.0 * 8.0 * s, "Size"), (c.x + 34.0 * s + 40.0 * 8.0 * s, "Location"), (c.x + 34.0 * s + 72.0 * 8.0 * s, "Owner")] {
        g.font.draw(t, x, hy, s, col::TEXT_DIM);
    }
    let list = Rect::new(c.x, c.y + lh + 2.0 * s, c.w, c.h - lh - 2.0 * s);
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.pl_scroll = scroll(g, list, g.r8.pl_scroll, zones.len(), vis);
    let mut goto = None;
    for (k, z) in zones.iter().enumerate().skip(g.r8.pl_scroll).take(vis) {
        let y = list.y + (k - g.r8.pl_scroll) as f32 * row_h;
        if k % 2 == 0 {
            draw_rectangle(list.x, y, list.w, row_h - 2.0 * s, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        icon_or(g, super::build::zone_icon(&z.subtype), Rect::new(c.x + 4.0 * s, y + 1.0 * s, 24.0 * s, 24.0 * s), Ico::Zone);
        let ty = y + (row_h - 2.0 * s - GH * s) / 2.0;
        let nr = Rect::new(c.x + 34.0 * s, y, 27.0 * 8.0 * s, row_h - 2.0 * s);
        let hov = g.ui.hovered(nr);
        g.font.draw(&clip(&zone_label(g, z.id), 27), nr.x, ty, s, if hov { col::TEXT_HI } else { col::TEXT });
        if g.ui.clicked(nr) {
            goto = Some(z.id);
        }
        g.font.draw(&format!("{}x{} z{}", z.x2 - z.x1 + 1, z.y2 - z.y1 + 1, z.z), c.x + 34.0 * s + 28.0 * 8.0 * s, ty, s, col::TEXT_DIM);
        let loc = location_of_zone(g, z.id).map(|l| l.name.clone());
        let lx = c.x + 34.0 * s + 40.0 * 8.0 * s;
        match loc {
            Some(n) => {
                g.font.draw(&clip(&n, 30), lx, ty, s, col::TEXT_GOOD);
            }
            None if can_host_location(&z.subtype) => {
                if text_button(&mut g.ui, &g.font, Rect::new(lx, y + 1.0 * s, 18.0 * 8.0 * s, row_h - 4.0 * s), "Make location", BtnState::Normal) {
                    g.r8.pl_make = Some(z.id);
                    g.r8.pl_kind = None;
                    g.r8.pl_dscroll = 0;
                }
            }
            None => {
                g.font.draw("-", lx, ty, s, col::TEXT_DIM);
            }
        }
        if z.owner >= 0 {
            g.font.draw(&clip(&unit_label(g, z.owner), 26), c.x + 34.0 * s + 72.0 * 8.0 * s, ty, s, col::TEXT_CYAN);
        }
    }
    if zones.is_empty() {
        g.font.draw("No zones. Make one with Zones (z).", c.x, list.y, s, col::TEXT_DIM);
    }
    if let Some(id) = goto {
        if let Some(b) = g.sim.world.building(id) {
            let p = b.center;
            g.cam.focus(p);
            g.screen = Screen::None;
            g.open_zones();
            g.selected = Selected::Building(id);
        }
    }
}

/// DF v50: locations are made from meeting areas; hospitals are their own zone kind.
fn can_host_location(sub: &str) -> bool {
    matches!(sub, "MeetingHall" | "Hospital" | "DiningHall")
}

fn make_location_picker(g: &mut Game, c: Rect, zone: i32) {
    let s = g.ui.scale;
    let pr = Rect::new(c.x + c.w / 2.0 - 25.0 * 8.0 * s, c.y + 40.0 * s, 50.0 * 8.0 * s, (c.h * 0.7).max(200.0 * s));
    let zname = zone_label(g, zone);
    match g.r8.pl_kind {
        None => {
            let rows: Vec<PickRow> = LOCATION_KINDS
                .iter()
                .enumerate()
                .map(|(i, (k, _))| PickRow { id: i as i32, label: format!("New {}", k.to_lowercase()), info: String::new(), color: col::TEXT })
                .collect();
            let (_, pick) = picker(g, pr, &format!("Make a location in {}", clip(&zname, 24)), &rows, 0, None);
            match pick {
                Pick::Picked(i) => {
                    let kind = LOCATION_KINDS[i.max(0) as usize].0;
                    if kind == "Temple" {
                        g.r8.pl_kind = Some("Temple");
                    } else {
                        create(g, kind, zone, -1);
                    }
                }
                Pick::Cancel => g.r8.pl_make = None,
                Pick::Nothing => {}
            }
        }
        Some(kind) => {
            let rows: Vec<PickRow> = deities(g)
                .into_iter()
                .map(|(hf, n)| {
                    let spheres = g.sim.world.hist_figs.iter().find(|h| h.id == hf).map(|h| h.race.to_lowercase()).unwrap_or_default();
                    let _ = spheres;
                    PickRow { id: hf, label: hist_fig_name(g, hf), info: format!("{n} worshipper{}", if n == 1 { "" } else { "s" }), color: col::TEXT }
                })
                .collect();
            let (sc, pick) = picker(g, pr, "Dedicate the temple to", &rows, g.r8.pl_dscroll, Some("No specific deity"));
            g.r8.pl_dscroll = sc;
            match pick {
                Pick::Picked(hf) => create(g, kind, zone, hf),
                Pick::Cancel => g.r8.pl_kind = None,
                Pick::Nothing => {}
            }
        }
    }
}

fn create(g: &mut Game, kind: &str, zone: i32, deity: i32) {
    let n = g.sim.world.locations.iter().filter(|l| l.kind == kind).count() + 1;
    let name = if kind == "Temple" && deity >= 0 {
        format!("Temple of {}", hist_fig_name(g, deity).split(' ').next().unwrap_or("").to_string())
    } else {
        format!("{kind} {n} of {}", g.sim.world.meta.fort_name)
    };
    let id = g.sim.create_location(kind, zone, &name);
    if kind == "Temple" {
        // `create_location` has no deity argument: set it in the world (DF dedication).
        if let Some(l) = g.sim.world.locations.iter_mut().find(|l| l.id == id) {
            l.deity = deity;
        }
    }
    g.r8.pl_make = None;
    g.r8.pl_kind = None;
    g.r8.pl_tab = 0;
    g.set_status(format!("New location: {name}"));
}

fn draw_buildings(g: &mut Game, c: Rect, want: impl Fn(&str) -> bool) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let list_b: Vec<rf_core::Building> = g.sim.world.buildings.iter().filter(|b| want(&b.kind)).cloned().collect();
    let row_h = 28.0 * s;
    let list = Rect::new(c.x, c.y, c.w, c.h - lh);
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.pl_scroll = scroll(g, list, g.r8.pl_scroll, list_b.len(), vis);
    let mut goto = None;
    let mut settings = None;
    for (k, b) in list_b.iter().enumerate().skip(g.r8.pl_scroll).take(vis) {
        let y = list.y + (k - g.r8.pl_scroll) as f32 * row_h;
        if k % 2 == 0 {
            draw_rectangle(list.x, y, list.w, row_h - 2.0 * s, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        let ty = y + (row_h - 2.0 * s - GH * s) / 2.0;
        let title = super::building::building_title(g, b);
        let nr = Rect::new(c.x + 4.0 * s, y, 34.0 * 8.0 * s, row_h - 2.0 * s);
        let hov = g.ui.hovered(nr);
        g.font.draw(&clip(&title, 34), nr.x, ty, s, if hov { col::TEXT_HI } else { col::TEXT });
        if g.ui.clicked(nr) {
            goto = Some(b.id);
        }
        let info = match b.kind.as_str() {
            "Stockpile" => {
                let sp = b.stockpile.clone().unwrap_or_default();
                let cats: Vec<&str> = sp.categories.iter().map(|c| super::stockpile_ed::cat_info(c).0).collect();
                let links = sp.give_to.len() + sp.take_from.len();
                format!("{}x{} z{}  {}{}", b.x2 - b.x1 + 1, b.y2 - b.y1 + 1, b.z, if cats.len() >= 17 { "everything".to_string() } else { cats.join(", ") }, if links > 0 { format!("  ({links} links)") } else { String::new() })
            }
            "FarmPlot" => format!("{}x{} z{}  {}", b.x2 - b.x1 + 1, b.y2 - b.y1 + 1, b.z, b.crops.iter().map(|c| if c.is_empty() { "fallow".to_string() } else { super::building::plant_name(g, c) }).collect::<Vec<_>>().join(" / ")),
            _ => format!("z{}  {} task{}{}", b.z, b.jobs.len(), if b.jobs.len() == 1 { "" } else { "s" }, if b.built { "" } else { "  (not built)" }),
        };
        let ix = c.x + 36.0 * 8.0 * s;
        let maxc = ((c.x + c.w - ix - 20.0 * 8.0 * s) / (8.0 * s)).max(8.0) as usize;
        g.font.draw(&clip(&info, maxc), ix, ty, s, col::TEXT_DIM);
        if b.kind == "Stockpile" && text_button(&mut g.ui, &g.font, Rect::new(c.x + c.w - 16.0 * 8.0 * s, y + 1.0 * s, 16.0 * 8.0 * s, row_h - 4.0 * s), "Settings", BtnState::Normal) {
            settings = Some(b.id);
        }
    }
    g.font.draw(&format!("{} shown. Click a name to go there.", list_b.len()), c.x, c.y + c.h - GH * s, s, col::TEXT_DIM);
    if let Some(id) = settings {
        super::round8::open_pile(g, id);
    } else if let Some(id) = goto {
        if let Some(b) = g.sim.world.building(id) {
            let p = b.center;
            g.cam.focus(p);
            g.screen = Screen::None;
            g.selected = Selected::Building(id);
        }
    }
}

/// `--ui fake-locations`: a tavern and a temple on the first meeting areas (layout tests).
pub fn debug_locations(g: &mut Game) {
    let zones: Vec<i32> = g.sim.world.buildings.iter().filter(|b| b.kind == "Civzone" && b.subtype == "MeetingHall").map(|b| b.id).take(2).collect();
    if let Some(&z) = zones.first() {
        let id = g.sim.create_location("Tavern", z, "The Copper Tankard");
        let cits: Vec<i32> = g.sim.world.citizens().map(|u| u.id).take(2).collect();
        if let Some(&u) = cits.first() {
            g.sim.assign_occupation(id, "TAVERN_KEEPER", u);
        }
    }
    if let Some(&z) = zones.get(1) {
        let id = g.sim.create_location("Temple", z, "The Hall of Wealth");
        let d = deities(g).first().map(|d| d.0).unwrap_or(-1);
        if let Some(l) = g.sim.world.locations.iter_mut().find(|l| l.id == id) {
            l.deity = d;
        }
    }
}

// ------------------------------------------------------------------ Justice

const JUSTICE_TABS: &[&str] = &["Open cases", "Closed cases", "Convicts"];

pub fn draw_justice(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(c) = frame(g, "Justice", 1200.0) else { return };
    let (hit, _) = tabs(g, super::round8::tabs_x(g, c, "Justice"), c.y - 3.0 * s, JUSTICE_TABS, g.r8.j_tab.min(2));
    if let Some(i) = hit {
        g.r8.j_tab = i;
        g.r8.j_scroll = 0;
    }
    let top = c.y + 2.0 * lh + 4.0 * s;
    draw_line(c.x, top - 6.0 * s, c.x + c.w, top - 6.0 * s, 1.0, col::BORDER);
    let crimes: Vec<Crime> = g.sim.crimes().to_vec();
    let sheriff = g.sim.world.nobles.iter().find(|n| matches!(n.token.as_str(), "SHERIFF" | "CAPTAIN_OF_THE_GUARD") && n.holder >= 0).map(|n| (n.name.clone(), n.holder));
    let law = match &sheriff {
        Some((name, h)) => format!("{}: {}", words(&pretty(name)), unit_label(g, *h)),
        None => "No sheriff or captain of the guard: crimes are not investigated (appoint one in Nobles).".to_string(),
    };
    g.font.draw(&law, c.x, top, s, if sheriff.is_some() { col::TEXT_DIM } else { col::TEXT_BAD });
    let y0 = top + lh + 6.0 * s;
    let tab = g.r8.j_tab.min(2);
    let rows: Vec<&Crime> = crimes
        .iter()
        .filter(|cr| match tab {
            0 => cr.convicted < 0,
            _ => cr.convicted >= 0,
        })
        .collect();
    let row_h = 30.0 * s;
    let xs = [c.x + 4.0 * s, c.x + 26.0 * 8.0 * s, c.x + 52.0 * 8.0 * s, c.x + 64.0 * 8.0 * s, c.x + 78.0 * 8.0 * s];
    let heads = if tab == 0 { ["Crime", "Victim", "Year", "Witnesses", ""] } else if tab == 1 { ["Crime", "Victim", "Year", "Convicted", "Punishment"] } else { ["Convict", "Crime", "Year", "Punishment", ""] };
    for (x, t) in xs.iter().zip(heads) {
        g.font.draw(t, *x, y0, s, col::TEXT_DIM);
    }
    let list = Rect::new(c.x, y0 + lh + 2.0 * s, c.w, c.y + c.h - (y0 + lh + 2.0 * s));
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.j_scroll = scroll(g, list, g.r8.j_scroll, rows.len(), vis);
    let mut open = None;
    for (k, cr) in rows.iter().enumerate().skip(g.r8.j_scroll).take(vis) {
        let y = list.y + (k - g.r8.j_scroll) as f32 * row_h;
        if k % 2 == 0 {
            draw_rectangle(list.x, y, list.w, row_h - 2.0 * s, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        let ty = y + (row_h - 2.0 * s - GH * s) / 2.0;
        let victim = if cr.victim >= 0 { unit_label(g, cr.victim) } else { "-".into() };
        let year = if cr.year != 0 { cr.year.to_string() } else { "-".into() };
        match tab {
            0 => {
                g.font.draw(&clip(&words(&cr.kind), 24), xs[0], ty, s, col::TEXT_BAD);
                g.font.draw(&clip(&victim, 24), xs[1], ty, s, col::TEXT);
                g.font.draw(&year, xs[2], ty, s, col::TEXT_DIM);
                g.font.draw(&cr.witnesses.len().to_string(), xs[3], ty, s, col::TEXT_DIM);
                if text_button(&mut g.ui, &g.font, Rect::new(xs[4], y + 1.0 * s, 14.0 * 8.0 * s, row_h - 4.0 * s), "Convict...", BtnState::Normal) {
                    open = Some(cr.id);
                }
            }
            1 => {
                g.font.draw(&clip(&words(&cr.kind), 24), xs[0], ty, s, col::TEXT);
                g.font.draw(&clip(&victim, 24), xs[1], ty, s, col::TEXT_DIM);
                g.font.draw(&year, xs[2], ty, s, col::TEXT_DIM);
                g.font.draw(&clip(&unit_label(g, cr.convicted), 16), xs[3], ty, s, col::TEXT_CYAN);
                g.font.draw(&clip(if cr.punishment.is_empty() { "pending" } else { &cr.punishment }, 24), xs[4], ty, s, col::TEXT_DIM);
            }
            _ => {
                g.font.draw(&clip(&unit_label(g, cr.convicted), 24), xs[0], ty, s, col::TEXT_CYAN);
                g.font.draw(&clip(&words(&cr.kind), 24), xs[1], ty, s, col::TEXT);
                g.font.draw(&year, xs[2], ty, s, col::TEXT_DIM);
                g.font.draw(&clip(if cr.punishment.is_empty() { "pending" } else { &cr.punishment }, 28), xs[3], ty, s, col::TEXT_DIM);
            }
        }
    }
    if rows.is_empty() {
        g.font.draw(if tab == 0 { "No open cases." } else { "None." }, c.x, list.y, s, col::TEXT_DIM);
    }
    if let Some(id) = open {
        g.r8.j_convict = Some(id);
        g.r8.j_pscroll = 0;
    }
    if let Some(cid) = g.r8.j_convict {
        let Some(cr) = crimes.iter().find(|c| c.id == cid).cloned() else {
            g.r8.j_convict = None;
            return;
        };
        // Witnessed suspects first, like DF's "suspects" list (the criminal is not known to
        // the player unless a witness named them).
        let mut rows = super::round8::citizen_rows(g);
        rows.retain(|r| r.id != cr.victim);
        rows.sort_by_key(|r| !cr.witnesses.contains(&r.id));
        for r in rows.iter_mut().filter(|r| cr.witnesses.contains(&r.id)) {
            r.info = format!("{} - witness", r.info);
            r.color = col::TEXT_HI;
        }
        let pr = Rect::new(c.x + c.w / 2.0 - 26.0 * 8.0 * s, c.y + 30.0 * s, 52.0 * 8.0 * s, c.h - 60.0 * s);
        let (sc, pick) = picker(g, pr, &format!("Convict for {}", words(&cr.kind).to_lowercase()), &rows, g.r8.j_pscroll, None);
        g.r8.j_pscroll = sc;
        match pick {
            Pick::Picked(u) if u >= 0 => {
                g.sim.convict(cid, u);
                g.r8.j_convict = None;
                g.set_status(format!("{} convicted of {}", unit_label(g, u), words(&cr.kind).to_lowercase()));
            }
            Pick::Cancel => g.r8.j_convict = None,
            _ => {}
        }
    }
}

/// `--ui fake-crimes`: two open cases and one closed case (layout tests; the export has none).
pub fn debug_crimes(g: &mut Game) {
    let cits: Vec<i32> = g.sim.world.citizens().map(|u| u.id).take(6).collect();
    if cits.len() < 4 {
        return;
    }
    let year = g.sim.world.calendar.year;
    let base = g.sim.world.crimes.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let mk = |id: i32, kind: &str, victim: i32, witnesses: Vec<i32>, convicted: i32, pun: &str| Crime {
        id,
        kind: kind.into(),
        criminal: -1,
        victim,
        year,
        convicted,
        punishment: pun.into(),
        witnesses,
        ..Default::default()
    };
    g.sim.world.crimes.push(mk(base, "Theft", cits[0], vec![cits[1]], -1, ""));
    g.sim.world.crimes.push(mk(base + 1, "Vandalism", -1, vec![cits[2], cits[3]], -1, ""));
    g.sim.world.crimes.push(mk(base + 2, "DisorderlyBehavior", cits[1], vec![], cits[2], "Jail 2 weeks"));
}

// ------------------------------------------------------------------ Objects (artifacts)

pub fn draw_objects(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(c) = frame(g, "Objects: artifacts", 1400.0) else { return };
    let now = get_time();
    if g.r8.o_rows.is_empty() || now - g.r8.o_at > 2.0 {
        g.r8.o_rows = artifact_rows(g);
        g.r8.o_at = now;
    }
    let rows = g.r8.o_rows.clone();
    let y0 = c.y + 1.5 * GH * s + 14.0 * s;
    let xs = [c.x + 4.0 * s, c.x + 44.0 * 8.0 * s, c.x + 62.0 * 8.0 * s, c.x + 76.0 * 8.0 * s, c.x + 110.0 * 8.0 * s, c.x + c.w - 9.0 * 8.0 * s];
    for (x, t) in xs.iter().zip(["Name", "Item", "Material", "Where", "Made by", "Value"]) {
        g.font.draw(t, *x, y0, s, col::TEXT_DIM);
    }
    let row_h = lh + 6.0 * s;
    let list = Rect::new(c.x, y0 + lh + 2.0 * s, c.w, c.y + c.h - (y0 + lh + 2.0 * s) - lh);
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.o_scroll = scroll(g, list, g.r8.o_scroll, rows.len(), vis);
    let mut goto = None;
    for (k, a) in rows.iter().enumerate().skip(g.r8.o_scroll).take(vis) {
        let (name, kind, mat, loc, maker, pos, value) = (&a.name, &a.kind, &a.mat, &a.place, &a.maker, &a.pos, &a.value);
        let y = list.y + (k - g.r8.o_scroll) as f32 * row_h;
        let rr = Rect::new(list.x, y, list.w, row_h - 1.0);
        if k % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        if pos.is_some() && super::list_row(&mut g.ui, rr, false) {
            goto = *pos;
        }
        let ty = y + 3.0 * s;
        g.font.draw(&clip(name, 42), xs[0], ty, s, col::TEXT_HI);
        g.font.draw(&clip(kind, 17), xs[1], ty, s, col::TEXT);
        g.font.draw(&clip(mat, 13), xs[2], ty, s, col::TEXT_DIM);
        g.font.draw(&clip(loc, 33), xs[3], ty, s, if pos.is_some() { col::TEXT } else { col::TEXT_DIM });
        g.font.draw(&clip(maker, ((xs[5] - xs[4]) / (8.0 * s)) as usize - 1), xs[4], ty, s, col::TEXT_DIM);
        g.font.draw(&format!("{value}☼"), xs[5], ty, s, col::TEXT_DIM);
    }
    g.font.draw(&format!("{} artifacts. Click one to go to it.", rows.len()), c.x, c.y + c.h - GH * s, s, col::TEXT_DIM);
    if let Some(p) = goto {
        g.cam.focus(p);
        g.screen = Screen::None;
        g.selected = Selected::Tile(p);
    }
}

/// The artifacts of the world for the Objects screen, by name.
pub fn artifact_rows(g: &Game) -> Vec<super::round8::ArtifactRow> {
    let w = &g.sim.world;
    let records = g.sim.mood_records();
    let mut v: Vec<super::round8::ArtifactRow> = w
        .items
        .iter()
        .filter(|it| it.flags.artifact)
        .map(|it| {
            // DF's own item name is the last "(...)" of the description: "Gulfnet (Battle Axe)".
            let from_desc = it.desc.trim_end().strip_suffix(')').and_then(|d| d.rfind('(').map(|i| d[i + 1..].to_string()));
            let kind = match from_desc {
                Some(k) if !k.is_empty() => k,
                _ if it.subtype.is_empty() => pretty(&it.kind),
                _ => item_subtype_name(w, &it.subtype),
            };
            let mat = w.material(it.mat).map(|m| m.name.clone()).unwrap_or_default();
            let (mut place, pos) = item_location(g, it);
            // On the ground or in furniture: name the room (a named zone) if there is one.
            if let Some(p) = pos.filter(|_| it.holder < 0) {
                if let Some(z) = w.buildings.iter().find(|b| b.kind == "Civzone" && !b.name.is_empty() && b.contains(p)) {
                    place = format!("{place} ({})", z.name);
                }
            }
            let maker = records.iter().find(|r| r.artifact_id == it.id).map(|r| r.name.clone()).unwrap_or_else(|| "unknown (made in DF)".into());
            let name = if it.name.is_empty() { it.desc.clone() } else { it.name.clone() };
            super::round8::ArtifactRow { id: it.id, name, kind, mat, place, maker, pos, value: it.value }
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    v
}

fn item_subtype_name(w: &rf_core::World, sub: &str) -> String {
    if let Some(r) = w.weapon_raws.iter().find(|r| r.token == sub) {
        if !r.name.is_empty() {
            return r.name.clone();
        }
    }
    if let Some(r) = w.armor_raws.iter().find(|r| r.token == sub) {
        if !r.name.is_empty() {
            return r.name.clone();
        }
    }
    let t = sub.split('_').skip(2).collect::<Vec<_>>().join("_");
    pretty(if t.is_empty() { sub } else { &t }).to_lowercase()
}

/// Where an item is: carried by, in a building, in a container, or on the ground.
pub fn item_location(g: &Game, it: &rf_core::Item) -> (String, Option<rf_core::Pos>) {
    let w = &g.sim.world;
    if it.holder >= 0 {
        let p = w.unit(it.holder).map(|u| u.pos).filter(|p| p.x >= 0);
        return (format!("carried by {}", unit_label(g, it.holder)), p);
    }
    if it.building >= 0 {
        if let Some(b) = w.building(it.building) {
            return (format!("in {}", super::building::building_title(g, b)), Some(b.center));
        }
    }
    if it.container >= 0 {
        let p = (it.pos.x >= 0).then_some(it.pos);
        return ("in a container".into(), p);
    }
    if it.pos.x >= 0 {
        if let Some(b) = w.buildings.iter().find(|b| b.kind == "Stockpile" && b.contains(it.pos)) {
            return (format!("Stockpile #{}, z{}", b.id, it.pos.z), Some(it.pos));
        }
        return (format!("{}, {}, z{}", it.pos.x, it.pos.y, it.pos.z), Some(it.pos));
    }
    ("not on the map".into(), None)
}

// ------------------------------------------------------------------ World

const WORLD_TABS: &[&str] = &["Civilizations", "All entities", "Caravans"];

fn relation_color(r: &str) -> Color {
    match r {
        "War" => col::TEXT_BAD,
        "Peace" => col::TEXT_GOOD,
        "Own" => col::TEXT_HI,
        _ => col::TEXT_DIM,
    }
}

pub fn draw_world(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let title = format!("World: {}", g.sim.world.meta.world_name);
    let Some(c) = frame(g, &title, 1200.0) else { return };
    let tx = c.x + (g.font.width(&title, 1.5 * s) + 24.0 * s).max(10.0 * 8.0 * s);
    let (hit, _) = tabs(g, tx, c.y - 3.0 * s, WORLD_TABS, g.r8.w_tab.min(2));
    if let Some(i) = hit {
        g.r8.w_tab = i;
        g.r8.w_scroll = 0;
    }
    let top = c.y + 2.0 * lh + 4.0 * s;
    draw_line(c.x, top - 6.0 * s, c.x + c.w, top - 6.0 * s, 1.0, col::BORDER);
    let w = &g.sim.world;
    let own = w.entities.iter().find(|e| e.relation == "Own" && e.kind == "Civilization").map(|e| format!("{} ({})", e.name, pretty(&e.race).to_lowercase()));
    g.font.draw(
        &format!("{} of {}, {}", w.meta.fort_name, own.unwrap_or_else(|| "your civilization".into()), w.meta.world_name),
        c.x,
        top,
        s,
        col::TEXT_DIM,
    );
    let y0 = top + lh + 6.0 * s;
    if g.r8.w_tab == 2 {
        draw_caravans(g, Rect::new(c.x, y0, c.w, c.y + c.h - y0));
        return;
    }
    let mut ents: Vec<rf_core::EntityInfo> = w.entities.iter().filter(|e| g.r8.w_tab == 1 || e.kind == "Civilization").cloned().collect();
    let rank = |r: &str| match r {
        "Own" => 0,
        "War" => 1,
        "Peace" => 2,
        _ => 3,
    };
    ents.sort_by(|a, b| rank(&a.relation).cmp(&rank(&b.relation)).then(a.race.cmp(&b.race)).then(a.id.cmp(&b.id)));
    let xs = [c.x + 4.0 * s, c.x + 36.0 * 8.0 * s, c.x + 54.0 * 8.0 * s, c.x + 72.0 * 8.0 * s, c.x + 92.0 * 8.0 * s];
    for (x, t) in xs.iter().zip(["Name", "Race", "Kind", "Relation", "Trade"]) {
        g.font.draw(t, *x, y0, s, col::TEXT_DIM);
    }
    let row_h = lh + 6.0 * s;
    let list = Rect::new(c.x, y0 + lh + 2.0 * s, c.w, c.y + c.h - (y0 + lh + 2.0 * s) - lh);
    let vis = ((list.h / row_h) as usize).max(1);
    g.r8.w_scroll = scroll(g, list, g.r8.w_scroll, ents.len(), vis);
    for (k, e) in ents.iter().enumerate().skip(g.r8.w_scroll).take(vis) {
        let y = list.y + (k - g.r8.w_scroll) as f32 * row_h;
        if k % 2 == 0 {
            draw_rectangle(list.x, y, list.w, row_h - 1.0, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        let ty = y + 3.0 * s;
        let name = if e.name.trim().is_empty() { format!("(unnamed {})", words(&e.kind).to_lowercase()) } else { e.name.clone() };
        g.font.draw(&clip(&name, 34), xs[0], ty, s, if e.name.trim().is_empty() { col::TEXT_DIM } else { col::TEXT });
        g.font.draw(&clip(&pretty(&e.race).to_lowercase(), 17), xs[1], ty, s, col::TEXT);
        g.font.draw(&clip(&words(&e.kind), 17), xs[2], ty, s, col::TEXT_DIM);
        let rel = match e.relation.as_str() {
            "Own" => "Your civilization",
            "War" => "At war",
            "Peace" => "At peace",
            _ => "No contact",
        };
        g.font.draw(rel, xs[3], ty, s, relation_color(&e.relation));
        if e.trades && e.relation != "Own" {
            g.font.draw("sends caravans", xs[4], ty, s, col::TEXT_GOOD);
        }
    }
    let wars = ents.iter().filter(|e| e.relation == "War").count();
    g.font.draw(&format!("{} shown, {wars} at war with you.", ents.len()), c.x, c.y + c.h - GH * s, s, col::TEXT_DIM);
}

fn draw_caravans(g: &mut Game, c: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let mut y = c.y;
    match g.r4.trade.clone().or_else(|| g.sim.trade_status()) {
        Some(t) => {
            g.font.draw(&format!("At the depot now: {} ({}), {} goods offered", t.civ, pretty(&t.race).to_lowercase(), t.goods.len()), c.x, y, s, col::TEXT_GOOD);
            y += lh;
            g.font.draw(&format!("They leave in {}. Broker {}at the depot.", super::round4::ticks_span(t.leaves_in), if t.broker_present { "" } else { "not " }), c.x, y, s, col::TEXT_DIM);
            y += lh;
            if text_button(&mut g.ui, &g.font, Rect::new(c.x, y + 4.0 * s, 14.0 * 8.0 * s, GH * s + 8.0 * s), "Trade", BtnState::Normal) {
                super::round4::open_trade(g);
                return;
            }
            y += btn_h(g) + 10.0 * s;
        }
        None => {
            g.font.draw("No caravan at the depot.", c.x, y, s, col::TEXT_DIM);
            y += lh + 6.0 * s;
        }
    }
    for l in super::round4::population_lines(g).into_iter().filter(|l| l.to_lowercase().contains("caravan")) {
        g.font.draw(&l, c.x, y, s, col::TEXT);
        y += lh;
    }
    y += 6.0 * s;
    let carav = g.sim.world.caravans.clone();
    g.font.draw(&format!("Caravans in the save ({}):", carav.len()), c.x, y, s, col::BORDER_HI);
    y += lh;
    for cv in &carav {
        let ent = g.sim.world.entities.iter().find(|e| e.id == cv.entity_id).map(|e| e.name.clone()).unwrap_or_else(|| cv.civ.clone());
        g.font.draw(&format!("{} ({}): {}, {} goods, {} merchants", ent, pretty(&cv.race).to_lowercase(), words(&cv.state), cv.goods.len(), cv.merchants.len()), c.x, y, s, col::TEXT);
        y += lh;
    }
    let trading: Vec<String> = g.sim.world.entities.iter().filter(|e| e.trades && e.relation != "War").map(|e| format!("{} ({})", e.name, pretty(&e.race).to_lowercase())).collect();
    y += 6.0 * s;
    g.font.draw("Trade partners:", c.x, y, s, col::BORDER_HI);
    y += lh;
    if trading.is_empty() {
        g.font.draw("none", c.x, y, s, col::TEXT_DIM);
    }
    for t in trading {
        g.font.draw(&t, c.x, y, s, col::TEXT);
        y += lh;
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_kinds_have_occupations() {
        assert_eq!(occupations("Tavern"), &["TAVERN_KEEPER", "PERFORMER"]);
        assert!(occupations("Guildhall").is_empty());
        assert_eq!(kind_icon("Temple"), "ZONE_TEMPLE");
        assert!(can_host_location("MeetingHall") && !can_host_location("Bedroom"));
        assert_eq!(super::super::round8::zone_kind_name("Pen"), "Pen/pasture");
    }
}
