//! Labor screen (DF hotkey `y`): citizens x labors grid, grouped like DF's labor tabs.
//! Click a cell to toggle (`Sim::set_labor`), click a column header to toggle it for all
//! shown citizens, type to filter by name.

use super::panels::{unit_name, unit_profession};
use super::{clip, col, panel, skill_level_name, text_button, wheel_over, BtnState};
use crate::font::GH;
use crate::game::{Game, Screen, Selected};
use crate::keys::SHIFT;
use macroquad::prelude::*;

/// One labor column: DF `unit_labor` name, two header lines, related skill.
pub struct LaborCol {
    pub labor: &'static str,
    pub l1: &'static str,
    pub l2: &'static str,
    pub skill: &'static str,
}

const fn lc(labor: &'static str, l1: &'static str, l2: &'static str, skill: &'static str) -> LaborCol {
    LaborCol { labor, l1, l2, skill }
}

/// Tabs of the labor screen. "Core" holds what a young fort needs to survive.
pub const GROUPS: &[(&str, &[LaborCol])] = &[
    (
        "Core",
        &[
            lc("MINE", "Mine", "", "MINING"),
            lc("CUTWOOD", "Wood", "cut", "WOODCUTTING"),
            lc("CARPENTER", "Carpen", "-ter", "CARPENTRY"),
            lc("MASON", "Mason", "", "MASONRY"),
            lc("BREWER", "Brew", "", "BREWING"),
            lc("COOK", "Cook", "", "COOK"),
            lc("PLANT", "Farm", "(field)", "PLANT"),
            lc("HERBALIST", "Herb-", "alist", "HERBALISM"),
            lc("FISH", "Fish", "", "FISH"),
            lc("BUILD_CONSTRUCTION", "Build", "", "CONSTRUCTION"),
            lc("HAUL_STONE", "Haul", "stone", ""),
            lc("HAUL_WOOD", "Haul", "wood", ""),
            lc("HAUL_FOOD", "Haul", "food", ""),
            lc("HAUL_ITEM", "Haul", "item", ""),
            lc("HAUL_FURNITURE", "Haul", "furn.", ""),
            lc("HAUL_REFUSE", "Haul", "refuse", ""),
            lc("HAUL_BODY", "Haul", "body", ""),
            lc("HAUL_ANIMALS", "Haul", "animal", ""),
        ],
    ),
    (
        "Farming & food",
        &[
            lc("PLANT", "Farm", "(field)", "PLANT"),
            lc("HERBALIST", "Herb-", "alist", "HERBALISM"),
            lc("BREWER", "Brew", "", "BREWING"),
            lc("COOK", "Cook", "", "COOK"),
            lc("MILLER", "Mill", "", "MILLING"),
            lc("PROCESS_PLANT", "Plant", "process", "PROCESSPLANTS"),
            lc("BUTCHER", "Butch", "-er", "BUTCHER"),
            lc("TANNER", "Tan", "", "TANNER"),
            lc("FISH", "Fish", "", "FISH"),
            lc("CLEAN_FISH", "Clean", "fish", "PROCESSFISH"),
            lc("DISSECT_FISH", "Dissect", "fish", "DISSECT_FISH"),
            lc("MAKE_CHEESE", "Cheese", "", "CHEESEMAKING"),
            lc("MILK", "Milk", "", "MILK"),
            lc("BEEKEEPING", "Bee-", "keep", "BEEKEEPING"),
            lc("PRESSING", "Press", "", "PRESSING"),
            lc("HUNT", "Hunt", "", ""),
            lc("TRAPPER", "Trap", "", "TRAPPING"),
            lc("SHEARER", "Shear", "", "SHEARING"),
        ],
    ),
    (
        "Wood & stone",
        &[
            lc("MINE", "Mine", "", "MINING"),
            lc("CUTWOOD", "Wood", "cut", "WOODCUTTING"),
            lc("CARPENTER", "Carpen", "-ter", "CARPENTRY"),
            lc("BOWYER", "Bowyer", "", "BOWYER"),
            lc("WOOD_CRAFT", "Wood", "craft", "WOODCRAFT"),
            lc("BURN_WOOD", "Burn", "wood", "WOOD_BURNING"),
            lc("MASON", "Mason", "", "MASONRY"),
            lc("STONECUTTER", "Stone", "cut", ""),
            lc("STONE_CARVER", "Stone", "carve", ""),
            lc("ENGRAVER", "Engr-", "ave", "ENGRAVE_STONE"),
            lc("STONE_CRAFT", "Stone", "craft", "STONECRAFT"),
            lc("BUILD_CONSTRUCTION", "Build", "", "CONSTRUCTION"),
            lc("BUILD_ROAD", "Build", "road", ""),
            lc("MECHANIC", "Mech-", "anic", "MECHANICS"),
            lc("SIEGECRAFT", "Siege", "craft", "SIEGECRAFT"),
        ],
    ),
    (
        "Crafts & metal",
        &[
            lc("SMELT", "Smelt", "", "SMELT"),
            lc("FORGE_WEAPON", "Weapon", "smith", "FORGE_WEAPON"),
            lc("FORGE_ARMOR", "Armor", "smith", "FORGE_ARMOR"),
            lc("FORGE_FURNITURE", "Black", "smith", "FORGE_FURNITURE"),
            lc("METAL_CRAFT", "Metal", "craft", "METALCRAFT"),
            lc("CUT_GEM", "Gem", "cut", "CUTGEM"),
            lc("ENCRUST_GEM", "Gem", "set", "ENCRUSTGEM"),
            lc("GLASSMAKER", "Glass", "", "GLASSMAKER"),
            lc("BONE_CARVE", "Bone", "carve", "BONECARVE"),
            lc("LEATHER", "Leath-", "er", "LEATHERWORK"),
            lc("WEAVER", "Weave", "", "WEAVING"),
            lc("CLOTHESMAKER", "Cloth-", "es", "CLOTHESMAKING"),
            lc("SPINNER", "Spin", "", "SPINNING"),
            lc("DYER", "Dye", "", "DYER"),
            lc("POTTERY", "Pot-", "tery", "POTTERY"),
            lc("GLAZING", "Glaze", "", "GLAZING"),
            lc("WAX_WORKING", "Wax", "", "WAX_WORKING"),
            lc("PAPERMAKING", "Paper", "", "PAPERMAKING"),
            lc("BOOKBINDING", "Book", "bind", "BOOKBINDING"),
        ],
    ),
    (
        "Health & other",
        &[
            lc("DIAGNOSE", "Diag-", "nose", "DIAGNOSE"),
            lc("SURGERY", "Surg-", "ery", "SURGERY"),
            lc("BONE_SETTING", "Set", "bone", "SET_BONE"),
            lc("SUTURING", "Suture", "", "SUTURE"),
            lc("DRESSING_WOUNDS", "Dress", "wound", "DRESS_WOUNDS"),
            lc("FEED_WATER_CIVILIANS", "Feed", "patient", ""),
            lc("RECOVER_WOUNDED", "Rescue", "", ""),
            lc("CLEAN", "Clean", "", ""),
            lc("PULL_LEVER", "Pull", "lever", ""),
            lc("OPERATE_PUMP", "Pump", "", "OPERATE_PUMP"),
            lc("ANIMALCARE", "Animal", "care", "ANIMALCARE"),
            lc("ANIMALTRAIN", "Animal", "train", "ANIMALTRAIN"),
            lc("HAUL_TRADE", "Haul", "trade", ""),
            lc("HAUL_WATER", "Haul", "water", ""),
            lc("HANDLE_VEHICLES", "Vehic-", "les", ""),
            lc("GELD", "Geld", "", "GELD"),
            lc("SOAP_MAKER", "Soap", "", "SOAP_MAKING"),
            lc("POTASH_MAKING", "Potash", "", "POTASH_MAKING"),
            lc("LYE_MAKING", "Lye", "", "LYE_MAKING"),
        ],
    ),
];

pub fn draw_labor(g: &mut Game) {
    if g.screen != Screen::Labor {
        return;
    }
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let r = super::orders::screen_rect(g, 1500.0);
    panel(&mut g.ui, r);
    let pad = 12.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let cb = Rect::new(r.x + r.w - 24.0 * s, r.y + 6.0 * s, 18.0 * s, 16.0 * s);
    if text_button(&mut g.ui, &g.font, cb, "x", BtnState::Normal) {
        g.screen = Screen::None;
        return;
    }
    g.font.draw("Labor", x, y, 1.5 * s, col::TEXT_HI);
    // Group tabs.
    let mut tx = x + 9.0 * 8.0 * s;
    for (i, (name, _)) in GROUPS.iter().enumerate() {
        let tw = (name.len() as f32 + 2.0) * 8.0 * s;
        let st = if g.labor.group == i { BtnState::Active } else { BtnState::Normal };
        if text_button(&mut g.ui, &g.font, Rect::new(tx, y - 3.0 * s, tw, GH * s + 8.0 * s), name, st) {
            g.labor.group = i;
        }
        tx += tw + 4.0 * s;
    }
    // Name filter.
    tx += 16.0 * s;
    g.font.draw("Filter:", tx, y + 1.0 * s, s, col::TEXT);
    tx += 8.0 * 8.0 * s;
    let fr = Rect::new(tx, y - 3.0 * s, (r.x + r.w - pad - 30.0 * s - tx).clamp(60.0 * s, 26.0 * 8.0 * s), GH * s + 8.0 * s);
    draw_rectangle(fr.x, fr.y, fr.w, fr.h, Color::new(0.0, 0.0, 0.0, 0.55));
    let focus = g.labor.typing;
    draw_rectangle_lines(fr.x, fr.y, fr.w, fr.h, 1.0, if focus { col::ACTIVE_BORDER } else if g.ui.hovered(fr) { col::BORDER_HI } else { col::BORDER });
    if g.ui.clicked(fr) {
        g.labor.typing = true;
    } else if g.ui.lpressed && !g.ui.click_used && !fr.contains(g.ui.mouse) {
        g.labor.typing = false;
    }
    if g.labor.typing {
        for c in g.input.chars.clone() {
            if !c.is_control() && g.labor.filter.chars().count() < 24 {
                g.labor.filter.push(c);
                g.labor.scroll = 0;
            }
        }
        if g.input.fired.contains(&KeyCode::Backspace) {
            g.labor.filter.pop();
        }
        if g.input.fired.contains(&KeyCode::Enter) {
            g.labor.typing = false;
        }
    }
    let shown = if g.labor.filter.is_empty() && !focus { "type a name".to_string() } else { g.labor.filter.clone() };
    let caret = if focus && (get_time() * 2.0) as i64 % 2 == 0 { "_" } else { "" };
    g.font.draw(&format!("{shown}{caret}"), fr.x + 4.0 * s, fr.y + 4.0 * s, s, if g.labor.filter.is_empty() { col::TEXT_DIM } else { col::TEXT_HI });
    y += 2.0 * lh;
    let gi = g.labor.group.min(GROUPS.len() - 1);
    let cols = GROUPS[gi].1;
    // Citizens, filtered and sorted by name.
    let filt = g.labor.filter.to_lowercase();
    let mut cits: Vec<(i32, String, String)> = g
        .sim
        .world
        .citizens()
        .map(|u| (u.id, unit_name(u), unit_profession(u)))
        .filter(|(_, n, p)| filt.is_empty() || n.to_lowercase().contains(&filt) || p.to_lowercase().contains(&filt))
        .collect();
    cits.sort_by(|a, b| a.1.cmp(&b.1));
    let name_w = 30.0 * 8.0 * s;
    let avail = r.w - 2.0 * pad - name_w;
    let cw = (avail / cols.len() as f32).clamp(40.0 * s, 70.0 * s).floor();
    // Header.
    let hy = y;
    let mut hdr_click = None;
    for (ci, c) in cols.iter().enumerate() {
        let cx = x + name_w + ci as f32 * cw;
        let hr = Rect::new(cx, hy - 2.0 * s, cw - 2.0 * s, 2.0 * lh + 2.0 * s);
        let hov = g.ui.hovered(hr);
        if hov {
            draw_rectangle(hr.x, hr.y, hr.w, hr.h, col::BUTTON_HOVER);
            g.ui.tooltip = Some((format!("{} - click: toggle for everyone shown", super::pretty(c.labor)), vec2(hr.x, hr.y - 4.0)));
        }
        let maxc = ((cw - 4.0 * s) / (8.0 * s)) as usize;
        for (k, l) in [c.l1, c.l2].iter().enumerate() {
            let t = clip(l, maxc);
            let tw = g.font.width(&t, s);
            g.font.draw(&t, cx + (cw - tw) / 2.0, hy + k as f32 * lh, s, if hov { col::TEXT_HI } else { col::BORDER_HI });
        }
        if g.ui.clicked(hr) {
            hdr_click = Some(ci);
        }
    }
    g.font.draw(&format!("Citizens ({})", cits.len()), x, hy + lh, s, col::BORDER_HI);
    y += 2.0 * lh + 4.0 * s;
    draw_line(x, y - 2.0 * s, r.x + r.w - pad, y - 2.0 * s, 1.0, col::BORDER);
    // Rows.
    let row_h = lh + 4.0 * s;
    let area = Rect::new(x, y, r.w - 2.0 * pad, r.y + r.h - pad - lh - y);
    let rows = ((area.h / row_h) as usize).max(1);
    let step = wheel_over(&g.ui, &mut g.input, area);
    let n = cits.len();
    g.labor.scroll = (g.labor.scroll as i32 + step * 3).clamp(0, n.saturating_sub(rows) as i32) as usize;
    let mut toggles: Vec<(i32, &'static str, bool)> = Vec::new();
    let mut select = None;
    for (k, (id, name, prof)) in cits.iter().enumerate().skip(g.labor.scroll).take(rows) {
        let Some(u) = g.sim.world.unit(*id) else { continue };
        let rr = Rect::new(x, y, area.w, row_h - 1.0);
        if k % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        let nr = Rect::new(x, y, name_w - 6.0 * s, row_h - 1.0);
        if g.ui.hovered(nr) {
            draw_rectangle(nr.x, nr.y, nr.w, nr.h, col::BUTTON_HOVER);
        }
        g.font.draw(&clip(name, 17), x + 2.0 * s, y + 2.0 * s, s, if g.selected == Selected::Unit(*id) { col::TEXT_HI } else { col::TEXT });
        g.font.draw(&clip(prof, 11), x + 18.0 * 8.0 * s, y + 2.0 * s, s, col::TEXT_DIM);
        if g.ui.clicked(nr) {
            select = Some(*id);
        }
        for (ci, c) in cols.iter().enumerate() {
            let cx = x + name_w + ci as f32 * cw;
            let cr = Rect::new(cx + 2.0 * s, y + 1.0 * s, cw - 6.0 * s, row_h - 3.0 * s);
            let on = u.has_labor(c.labor);
            let hov = g.ui.hovered(cr);
            let bg = if on { Color::new(0.20, 0.42, 0.18, 1.0) } else { Color::new(0.10, 0.10, 0.11, 1.0) };
            draw_rectangle(cr.x, cr.y, cr.w, cr.h, bg);
            draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, 1.0, if hov { col::BORDER_HI } else { Color::new(0.25, 0.25, 0.28, 1.0) });
            let lvl = if c.skill.is_empty() { 0 } else { u.skill(c.skill) };
            let txt = if lvl > 0 { lvl.to_string() } else if on { "√".to_string() } else { String::new() };
            if !txt.is_empty() {
                let tc = if !on { col::TEXT_DIM } else if lvl >= 10 { col::TEXT_HI } else { col::TEXT_GOOD };
                let tw = g.font.width(&txt, s);
                g.font.draw(&txt, cr.x + (cr.w - tw) / 2.0, cr.y + (cr.h - GH * s) / 2.0, s, tc);
            }
            if hov {
                let sk = if c.skill.is_empty() { String::new() } else { format!(", {} {}", skill_level_name(lvl), super::pretty(c.skill).to_lowercase()) };
                g.ui.tooltip = Some((format!("{}: {} {}{sk}", clip(name, 24), super::pretty(c.labor), if on { "on" } else { "off" }), vec2(cr.x, cr.y - 4.0)));
            }
            if g.ui.clicked(cr) {
                toggles.push((*id, c.labor, !on));
            }
        }
        y += row_h;
    }
    if let Some(ci) = hdr_click {
        let c = &cols[ci];
        let all_on = cits.iter().all(|(id, _, _)| g.sim.world.unit(*id).map(|u| u.has_labor(c.labor)).unwrap_or(true));
        for (id, _, _) in &cits {
            toggles.push((*id, c.labor, !all_on));
        }
    }
    for (id, l, on) in toggles {
        g.sim.set_labor(id, l, on);
    }
    if let Some(id) = select {
        g.selected = Selected::Unit(id);
        g.unit_tab = 2;
        if g.input.mods & SHIFT != 0 {
            if let Some(u) = g.sim.world.unit(id) {
                let p = u.pos;
                g.cam.focus(p);
            }
        }
    }
    // Footer: counts per column.
    let fy = r.y + r.h - pad - GH * s;
    let range = if n > rows { format!("{}-{} of {n}", g.labor.scroll + 1, (g.labor.scroll + rows).min(n)) } else { format!("{n} shown") };
    g.font.draw(&range, x, fy, s, col::TEXT_DIM);
    g.font.draw("enabled:", x + name_w - 10.0 * 8.0 * s, fy, s, col::TEXT_DIM);
    for (ci, c) in cols.iter().enumerate() {
        let cnt = cits.iter().filter(|(id, _, _)| g.sim.world.unit(*id).map(|u| u.has_labor(c.labor)).unwrap_or(false)).count();
        let t = cnt.to_string();
        let cx = x + name_w + ci as f32 * cw;
        g.font.draw(&t, cx + (cw - g.font.width(&t, s)) / 2.0, fy, s, if cnt == 0 { col::TEXT_BAD } else { col::TEXT_DIM });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_group_has_the_survival_labors() {
        let core: Vec<&str> = GROUPS[0].1.iter().map(|c| c.labor).collect();
        for l in ["MINE", "CUTWOOD", "CARPENTER", "MASON", "BREWER", "COOK", "PLANT", "HERBALIST", "HAUL_STONE", "HAUL_FOOD"] {
            assert!(core.contains(&l), "{l}");
        }
    }
}
