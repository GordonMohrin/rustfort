//! Build menu (DF hotkey `b`), zone menu (`z`), the placement ghost and the zone overlay.
//!
//! Everything the menu offers comes from `Sim::build_options()`; placement is checked with
//! `Sim::can_place` (cached per footprint) and done with `Sim::place_building`.

use super::{clip, col, draw_icon, icon_label_button, panel, words, BtnState, Ico};
use crate::font::GH;
use crate::game::{Game, Selected, Submenu, Tool};
use macroquad::prelude::*;
use rf_core::{Building, Pos, NO_MAT};
use rf_sim::{BuildOption, ZONE_KINDS};

/// Menu categories in the order the sim lists them.
pub fn build_categories(opts: &[BuildOption]) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for o in opts {
        if !v.contains(&o.category) {
            v.push(o.category);
        }
    }
    v
}

/// Footprint (top-left, bottom-right) of option `o` placed at the mouse tile `a`, or dragged
/// from `a` to `b` for dragged options. Fixed footprints are centered on the mouse like DF.
pub fn footprint(o: &BuildOption, a: Pos, b: Pos) -> (Pos, Pos) {
    if o.dragged {
        (Pos::new(a.x.min(b.x), a.y.min(b.y), a.z), Pos::new(a.x.max(b.x), a.y.max(b.y), a.z))
    } else {
        let (w, h) = (o.size.0.max(1), o.size.1.max(1));
        let lo = Pos::new(a.x - (w - 1) / 2, a.y - (h - 1) / 2, a.z);
        (lo, Pos::new(lo.x + w - 1, lo.y + h - 1, a.z))
    }
}

/// DF interface icon (`BUILDING_ICON_*`) for a build option.
pub fn option_icon(kind: &str, subtype: &str) -> String {
    let s = match (kind, subtype) {
        ("Workshop", "Carpenters") => "WORKSHOP_CARPENTER",
        ("Workshop", "Masons") => "WORKSHOP_MASON",
        ("Workshop", "MetalsmithsForge") | ("Workshop", "MagmaForge") => "WORKSHOP_METALSMITH",
        ("Workshop", "Craftsdwarfs") => "WORKSHOP_CRAFTS",
        ("Workshop", "Jewelers") => "WORKSHOP_JEWELER",
        ("Workshop", "Bowyers") => "WORKSHOP_BOWYER",
        ("Workshop", "Mechanics") => "WORKSHOP_MECHANIC",
        ("Workshop", "Siege") => "WORKSHOP_SIEGE",
        ("Workshop", "Ashery") => "WORKSHOP_ASHERY",
        ("Workshop", "Leatherworks") => "WORKSHOP_LEATHER",
        ("Workshop", "Loom") => "WORKSHOP_LOOM",
        ("Workshop", "Clothiers") => "WORKSHOP_CLOTHES",
        ("Workshop", "Dyers") => "WORKSHOP_DYER",
        ("Workshop", "Still") => "WORKSHOP_STILL",
        ("Workshop", "Butchers") => "WORKSHOP_BUTCHER",
        ("Workshop", "Tanners") => "WORKSHOP_TANNER",
        ("Workshop", "Fishery") => "WORKSHOP_FISHERY",
        ("Workshop", "Kitchen") => "WORKSHOP_KITCHEN",
        ("Workshop", "Farmers") => "WORKSHOP_FARMER",
        ("Workshop", "Quern") => "WORKSHOP_QUERN",
        ("Workshop", "Millstone") => "WORKSHOP_MILLSTONE",
        ("Workshop", "Kennels") => "WORKSHOP_KENNEL",
        ("Workshop", _) => "WORKSHOPS",
        ("Furnace", "WoodFurnace") => "FURNACE_WOOD",
        ("Furnace", "Smelter") | ("Furnace", "MagmaSmelter") => "FURNACE_SMELTER",
        ("Furnace", "GlassFurnace") | ("Furnace", "MagmaGlassFurnace") => "FURNACE_GLASS",
        ("Furnace", "Kiln") | ("Furnace", "MagmaKiln") => "FURNACE_KILN",
        ("Furnace", _) => "WORKSHOPS_FURNACES",
        ("FarmPlot", _) => "FARM_PLOT",
        ("Weaponrack", _) => "WEAPON_RACK",
        ("Armorstand", _) => "ARMOR_STAND",
        ("TradeDepot", _) => "TRADE_DEPOT",
        ("Chain", _) => "RESTRAINT",
        ("TractionBench", _) => "TRACTION_BENCH",
        ("ArcheryTarget", _) => "ARCHERY_TARGET",
        ("NestBox", _) => "NEST_BOX",
        ("Construction", "Wall") => "WALL",
        ("Construction", "Floor") => "FLOOR",
        ("Construction", "Ramp") => "RAMP",
        ("Construction", s) if s.contains("Stair") => "STAIRS",
        ("Construction", "Fortification") => "FORTIFICATION",
        ("Trap", "StoneFallTrap") => "TRAP_STONE",
        ("Trap", "WeaponTrap") => "TRAP_WEAPON",
        ("Trap", "CageTrap") => "TRAP_CAGE",
        ("Trap", "Lever") => "LEVER",
        ("Trap", "PressurePlate") => "PRESSURE_PLATE",
        _ => "",
    };
    if s.is_empty() {
        format!("BUILDING_ICON_{}", kind.to_uppercase())
    } else {
        format!("BUILDING_ICON_{s}")
    }
}

pub fn category_icon(cat: &str) -> &'static str {
    match cat {
        "Workshops" => "BUILDING_ICON_WORKSHOPS",
        "Furnaces" => "BUILDING_ICON_WORKSHOPS_FURNACES",
        "Furniture" => "BUILDING_ICON_FURNITURE",
        "Doors" | "Doors/hatches" => "BUILDING_ICON_DOORS_HATCHES",
        "Constructions" => "BUILDING_ICON_WALLS_FLOORS",
        "Farming" => "BUILDING_ICON_WORKSHOPS_FARMING",
        "Machines" | "Fluids" => "BUILDING_ICON_MACHINES_FLUIDS",
        "Cages" => "BUILDING_ICON_CAGES_RESTRAINT",
        "Traps" => "BUILDING_ICON_TRAPS",
        "Military" => "BUILDING_ICON_MILITARY",
        _ => "BUILDING_ICON_WORKSHOPS",
    }
}

/// DF interface icon (`ZONE_*`) of a civzone type.
pub fn zone_icon(kind: &str) -> &'static str {
    match kind {
        "MeetingHall" => "ZONE_MEETING",
        "Bedroom" => "ZONE_BEDROOM",
        "DiningHall" => "ZONE_DINING_HALL",
        "WaterSource" => "ZONE_WATER_SOURCE",
        "Dump" => "ZONE_DUMP",
        "Pen" => "ZONE_PEN",
        "Tomb" => "ZONE_TOMB",
        "FishingArea" => "ZONE_FISHING",
        "PlantGathering" => "ZONE_GATHER",
        "Pond" | "Pit" => "ZONE_PIT",
        "SandCollection" => "ZONE_SAND",
        "ClayCollection" => "ZONE_CLAY",
        "Hospital" => "ZONE_HOSPITAL",
        "AnimalTraining" => "ZONE_ANIMAL_TRAINING",
        "Office" => "ZONE_OFFICE",
        "ArcheryRange" => "ZONE_ARCHERY_RANGE",
        "Barracks" => "ZONE_BARRACKS",
        "Dormitory" => "ZONE_DORMITORY",
        "Dungeon" => "ZONE_DUNGEON",
        "Tavern" => "ZONE_TAVERN",
        "Library" => "ZONE_LIBRARY",
        "Temple" => "ZONE_TEMPLE",
        "Guildhall" => "ZONE_GUILDHALL",
        _ => "ZONE_MULTI",
    }
}

/// Zone tint for the fallback overlay (no sprites).
fn zone_color(kind: &str) -> Color {
    match kind {
        "Bedroom" | "Dormitory" => Color::from_rgba(120, 160, 230, 255),
        "DiningHall" | "MeetingHall" | "Tavern" => Color::from_rgba(230, 200, 90, 255),
        "Tomb" => Color::from_rgba(170, 170, 190, 255),
        "WaterSource" | "FishingArea" => Color::from_rgba(80, 150, 240, 255),
        "Dump" => Color::from_rgba(170, 120, 70, 255),
        "Pen" => Color::from_rgba(150, 210, 90, 255),
        _ => Color::from_rgba(100, 210, 120, 255),
    }
}

fn row_h(s: f32) -> f32 {
    36.0 * s
}

/// The build menu above the toolbar. Returns its top y.
pub fn draw_build_menu(g: &mut Game, toolbar_y: f32) -> f32 {
    let s = g.ui.scale;
    let pad = 6.0 * s;
    let cats = build_categories(&g.build_opts);
    if cats.is_empty() {
        return toolbar_y;
    }
    g.build_cat = g.build_cat.min(cats.len() - 1);
    let cur_cat = cats[g.build_cat];
    let opts: Vec<usize> = (0..g.build_opts.len()).filter(|&i| g.build_opts[i].category == cur_cat).collect();
    let cell_w = 214.0 * s;
    let cols = 4usize.min(opts.len().max(1));
    let rows = opts.len().div_ceil(cols).max(1);
    let tab_w: Vec<f32> = cats.iter().map(|c| 32.0 * s + 14.0 * s + g.font.width(c, s)).collect();
    let tabs_w: f32 = tab_w.iter().sum::<f32>() + (cats.len() as f32 - 1.0) * 4.0 * s;
    let w = (cols as f32 * (cell_w + 4.0 * s) - 4.0 * s).max(tabs_w).max(62.0 * 8.0 * s) + 2.0 * pad;
    let lh = g.ui.line_h();
    let h = pad + 2.0 * lh + row_h(s) + 8.0 * s + rows as f32 * (row_h(s) + 4.0 * s) + pad;
    let x = ((screen_width() - w) / 2.0).max(4.0);
    let r = Rect::new(x, toolbar_y - h - 4.0 * s, w, h);
    panel(&mut g.ui, r);
    let (title, title_c, hint) = match g.tool {
        Tool::Build(i) if i < g.build_opts.len() => {
            let o = &g.build_opts[i];
            let size = if o.dragged { "drag a rectangle".to_string() } else { format!("{}x{}", o.size.0, o.size.1) };
            (
                format!("Build: {} ({size}), needs {}", o.label, o.needs),
                col::TEXT_HI,
                if o.dragged {
                    "Drag on the map to place. Right-click: back to the list.".to_string()
                } else {
                    "Click on the map to place (green = fits). Right-click: back to the list.".to_string()
                },
            )
        }
        _ => ("Build".to_string(), col::TEXT_HI, "Choose a building. Dwarves bring the materials and construct it.".to_string()),
    };
    let maxc = ((w - 2.0 * pad) / (8.0 * s)) as usize;
    g.font.draw(&clip(&title, maxc), r.x + pad, r.y + pad, s, title_c);
    g.font.draw(&clip(&hint, maxc), r.x + pad, r.y + pad + lh, s, col::TEXT_DIM);
    // Category tabs.
    let mut tx = r.x + pad;
    let ty = r.y + pad + 2.0 * lh;
    for (ci, c) in cats.iter().enumerate() {
        let tr = Rect::new(tx, ty, tab_w[ci], row_h(s));
        let st = if ci == g.build_cat { BtnState::Active } else { BtnState::Normal };
        if icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, tr, category_icon(c), Ico::Hammer, c, st) {
            g.build_cat = ci;
            g.tool = Tool::None;
            g.drag = None;
        }
        tx += tab_w[ci] + 4.0 * s;
    }
    // Options of the current category.
    let gy = ty + row_h(s) + 8.0 * s;
    draw_line(r.x + pad, gy - 4.0 * s, r.x + w - pad, gy - 4.0 * s, 1.0, col::BORDER);
    for (k, &i) in opts.iter().enumerate() {
        let (cx, cy) = ((k % cols) as f32, (k / cols) as f32);
        let br = Rect::new(r.x + pad + cx * (cell_w + 4.0 * s), gy + cy * (row_h(s) + 4.0 * s), cell_w, row_h(s));
        let o = &g.build_opts[i];
        let icon = option_icon(o.kind, o.subtype);
        let label = o.label;
        let st = if g.tool == Tool::Build(i) { BtnState::Active } else { BtnState::Normal };
        if g.ui.hovered(br) {
            let o = &g.build_opts[i];
            let size = if o.dragged { "dragged".to_string() } else { format!("{}x{}", o.size.0, o.size.1) };
            g.ui.tooltip = Some((format!("{} - {size}, needs {}", o.label, o.needs), vec2(br.x, br.y - 4.0)));
        }
        if icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, br, &icon, Ico::Hammer, label, st) {
            g.tool = if g.tool == Tool::Build(i) { Tool::None } else { Tool::Build(i) };
            g.drag = None;
            g.build_check = None;
        }
    }
    r.y
}

/// The zone menu above the toolbar. Returns its top y.
pub fn draw_zone_menu(g: &mut Game, toolbar_y: f32) -> f32 {
    let s = g.ui.scale;
    let pad = 6.0 * s;
    let lh = g.ui.line_h();
    let cell_w = 150.0 * s;
    let cols = 4usize;
    let rows = ZONE_KINDS.len().div_ceil(cols);
    let w = cols as f32 * (cell_w + 4.0 * s) - 4.0 * s + 2.0 * pad;
    let h = pad + 2.0 * lh + rows as f32 * (row_h(s) + 4.0 * s) + pad;
    let x = ((screen_width() - w) / 2.0).max(4.0);
    let r = Rect::new(x, toolbar_y - h - 4.0 * s, w, h);
    panel(&mut g.ui, r);
    let (title, hint) = match g.tool {
        Tool::Zone(k) => (format!("Zone: {} - drag a rectangle", words(ZONE_KINDS[k.min(ZONE_KINDS.len() - 1)])), "Right-click: back to the list."),
        _ => ("Zones".to_string(), "Pick a zone type and drag on the map, or click a zone to select it."),
    };
    let maxc = ((w - 2.0 * pad) / (8.0 * s)) as usize;
    g.font.draw(&clip(&title, maxc), r.x + pad, r.y + pad, s, col::TEXT_HI);
    g.font.draw(&clip(hint, maxc), r.x + pad, r.y + pad + lh, s, col::TEXT_DIM);
    let gy = r.y + pad + 2.0 * lh;
    for (k, z) in ZONE_KINDS.iter().enumerate() {
        let (cx, cy) = ((k % cols) as f32, (k / cols) as f32);
        let br = Rect::new(r.x + pad + cx * (cell_w + 4.0 * s), gy + cy * (row_h(s) + 4.0 * s), cell_w, row_h(s));
        let st = if g.tool == Tool::Zone(k) { BtnState::Active } else { BtnState::Normal };
        if icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, br, zone_icon(z), Ico::Zone, &words(z), st) {
            g.tool = if g.tool == Tool::Zone(k) { Tool::None } else { Tool::Zone(k) };
            g.drag = None;
        }
    }
    r.y
}

/// Temporary building for the placement preview.
pub fn ghost_building(o: &BuildOption, lo: Pos, hi: Pos) -> Building {
    Building {
        id: -1,
        kind: o.kind.into(),
        subtype: o.subtype.into(),
        x1: lo.x,
        y1: lo.y,
        x2: hi.x,
        y2: hi.y,
        z: lo.z,
        center: Pos::new((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z),
        mat: NO_MAT,
        stage: -1,
        built: true,
        owner: -1,
        ..Default::default()
    }
}

/// Placement ghost under the mouse: the building's sprites tinted green (fits) or red.
pub fn draw_ghost(g: &mut Game) {
    let Tool::Build(i) = g.tool else { return };
    let Some(h) = g.hover else { return };
    let Some(o) = g.build_opts.get(i).cloned() else { return };
    let a = match (o.dragged, g.drag) {
        (true, Some(d)) => Pos::new(d.a.x, d.a.y, h.z),
        _ => h,
    };
    let (lo, hi) = footprint(&o, a, h);
    let ok = match &g.build_check {
        Some((ci, ca, cb, res)) if *ci == i && *ca == lo && *cb == hi => res.clone(),
        _ => {
            let res = g.sim.can_place(i, lo, hi);
            g.build_check = Some((i, lo, hi, res.clone()));
            res
        }
    };
    let good = ok.is_ok();
    let tint = if good { [150, 255, 150, 200] } else { [255, 110, 110, 200] };
    let fill = if good { Color::new(0.3, 0.9, 0.3, 0.22) } else { Color::new(0.95, 0.25, 0.2, 0.28) };
    let line = if good { Color::new(0.45, 1.0, 0.45, 0.95) } else { Color::new(1.0, 0.4, 0.35, 0.95) };
    let tmp = ghost_building(&o, lo, hi);
    let tp = g.cam.tile_px();
    let mut sprites = std::mem::take(&mut g.rend.buf);
    let mut any = false;
    for y in lo.y..=hi.y {
        for x in lo.x..=hi.x {
            let p = Pos::new(x, y, lo.z);
            let r = crate::render::tile_rect(g, x, y);
            draw_rectangle(r.x, r.y, r.w, r.h, fill);
            sprites.clear();
            g.gfx.building(&g.sim.world, &tmp, p, &mut sprites);
            for sp in sprites.iter() {
                any |= g.rend.draw_sprite(&g.gfx, &sp.mul_tint(tint), r.x, r.y, tp);
            }
        }
    }
    g.rend.buf = sprites;
    let a = g.cam.to_screen(lo.x as f32, lo.y as f32);
    let b = g.cam.to_screen(hi.x as f32 + 1.0, hi.y as f32 + 1.0);
    if !any && !o.dragged {
        // No map sprite (e.g. furniture without item graphics): show the menu icon.
        let sz = (b.x - a.x).min(b.y - a.y);
        let ir = Rect::new((a.x + b.x - sz) / 2.0, (a.y + b.y - sz) / 2.0, sz, sz);
        draw_icon(&g.gfx, &mut g.rend, &option_icon(o.kind, o.subtype), ir, Color::new(1.0, 1.0, 1.0, 0.8));
    }
    draw_rectangle_lines(a.x, a.y, b.x - a.x, b.y - a.y, 2.0, line);
    let m = g.mouse();
    let msg = match &ok {
        Ok(()) if o.dragged => format!("{} {}x{}", o.label, hi.x - lo.x + 1, hi.y - lo.y + 1),
        Ok(()) => o.label.to_string(),
        Err(e) => format!("{}: {e}", o.label),
    };
    let s = g.ui.scale;
    let tw = g.font.width(&msg, s);
    let (mx, my) = ((m.x + 20.0).min(screen_width() - tw - 12.0), m.y + 22.0);
    draw_rectangle(mx - 5.0, my - 4.0, tw + 10.0, GH * s + 8.0, Color::new(0.03, 0.03, 0.04, 0.88));
    draw_rectangle_lines(mx - 5.0, my - 4.0, tw + 10.0, GH * s + 8.0, 1.0, line);
    g.font.draw(&msg, mx, my, s, line);
}

/// Zone overlay (DF shows zones only in zone mode or for the selected zone): translucent
/// border graphics from DF's `ACTIVITY_ZONES` page plus the zone-type icon.
pub fn draw_zones(g: &mut Game, x0: i16, y0: i16, x1: i16, y1: i16) {
    let z = g.cam.z as i16;
    let zone_mode = g.submenu == Submenu::Zone;
    let sel = match g.selected {
        Selected::Building(id) => id,
        _ => i32::MIN,
    };
    let list: Vec<usize> = g
        .sim
        .world
        .buildings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == "Civzone" && b.z == z && b.x2 >= x0 && b.x1 < x1 && b.y2 >= y0 && b.y1 < y1 && (zone_mode || b.id == sel))
        .map(|(i, _)| i)
        .collect();
    // Draw the selected zone last so its border wins.
    let mut order = list;
    order.sort_by_key(|&i| g.sim.world.buildings[i].id == sel);
    for bi in order {
        let b = &g.sim.world.buildings[bi];
        let selected = b.id == sel;
        let base = if selected { "ZONE_SELECTED" } else { "ZONE_ACTIVE" };
        let zc = zone_color(&b.subtype);
        let (bx1, by1, bx2, by2, bz) = (b.x1, b.y1, b.x2, b.y2, b.z);
        let icon = zone_icon(&b.subtype);
        let mut name = String::with_capacity(24);
        for y in by1.max(y0)..=by2.min(y1 - 1) {
            for x in bx1.max(x0)..=bx2.min(x1 - 1) {
                let b = &g.sim.world.buildings[bi];
                let p = Pos::new(x, y, bz);
                if !b.contains(p) {
                    continue;
                }
                let inz = |dx: i16, dy: i16| b.contains(Pos::new(x + dx, y + dy, bz));
                name.clear();
                name.push_str(base);
                for (on, d) in [(!inz(0, -1), "_N"), (!inz(0, 1), "_S"), (!inz(-1, 0), "_W"), (!inz(1, 0), "_E")] {
                    if on {
                        name.push_str(d);
                    }
                }
                let r = crate::render::tile_rect(g, x, y);
                if !draw_icon(&g.gfx, &mut g.rend, &name, r, WHITE) {
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(zc.r, zc.g, zc.b, if selected { 0.30 } else { 0.18 }));
                }
            }
        }
        // Zone type icon in the middle.
        let a = g.cam.to_screen(bx1 as f32, by1 as f32);
        let e = g.cam.to_screen(bx2 as f32 + 1.0, by2 as f32 + 1.0);
        let sz = g.cam.tile_px().min(e.x - a.x).min(e.y - a.y);
        let ir = Rect::new((a.x + e.x - sz) / 2.0, (a.y + e.y - sz) / 2.0, sz, sz);
        if !draw_icon(&g.gfx, &mut g.rend, icon, ir, Color::new(1.0, 1.0, 1.0, 0.9)) {
            draw_rectangle_lines(a.x, a.y, e.x - a.x, e.y - a.y, 2.0, zc);
        }
        if selected && g.ui.scale > 0.0 {
            let label = words(&g.sim.world.buildings[bi].subtype);
            g.font.draw_shadow(&label, a.x + 4.0, a.y + 3.0, g.ui.scale, col::TEXT_HI);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(dragged: bool, size: (i16, i16)) -> BuildOption {
        BuildOption { kind: "Workshop", subtype: "Still", label: "Still", category: "Workshops", size, needs: "", dragged }
    }

    #[test]
    fn footprints() {
        let p = Pos::new(10, 10, 5);
        assert_eq!(footprint(&opt(false, (3, 3)), p, p), (Pos::new(9, 9, 5), Pos::new(11, 11, 5)));
        assert_eq!(footprint(&opt(false, (1, 1)), p, p), (p, p));
        let q = Pos::new(7, 12, 5);
        assert_eq!(footprint(&opt(true, (1, 1)), p, q), (Pos::new(7, 10, 5), Pos::new(10, 12, 5)));
    }

    #[test]
    fn icons_and_categories() {
        assert_eq!(option_icon("Workshop", "Still"), "BUILDING_ICON_WORKSHOP_STILL");
        assert_eq!(option_icon("Bed", ""), "BUILDING_ICON_BED");
        assert_eq!(option_icon("FarmPlot", ""), "BUILDING_ICON_FARM_PLOT");
        let s = rf_sim::Sim::new(rf_core::World::synthetic(24, 24, 12), 1);
        let cats = build_categories(&s.build_options());
        assert!(cats.contains(&"Workshops") && cats.contains(&"Furniture"));
        assert_eq!(zone_icon("MeetingHall"), "ZONE_MEETING");
    }
}
