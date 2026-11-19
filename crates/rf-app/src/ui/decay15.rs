//! [decay15] Labor > Standing orders: DF's refuse standing orders (w:Standing orders) and the
//! fort's decay statistics (rot, miasma, vermin, wear) from `Sim::decay_report`.

use super::col;
use crate::font::GH;
use crate::game::Game;
use macroquad::prelude::*;

/// Draw the standing-orders tab inside the content rect `c` of the Labor screen.
pub fn draw_standing_orders(g: &mut Game, c: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let x = c.x + 8.0 * s;
    let mut y = c.y + 4.0 * s;
    g.font.draw("Refuse", x, y, 1.2 * s, col::TEXT_HI);
    y += lh + 6.0 * s;
    let mut o = g.sim.refuse_orders();
    let before = o;
    {
        let rows: [(&str, &mut bool); 3] = [
            ("Workers gather refuse", &mut o.gather),
            ("Workers gather refuse from outside", &mut o.outdoor),
            ("Workers ignore outdoor vermin remains", &mut o.ignore_outdoor_remains),
        ];
        for (label, v) in rows {
            if super::checkbox(&mut g.ui, &g.font, x, y, *v, label) {
                *v = !*v;
            }
            y += GH * s + 8.0 * s;
        }
    }
    y += 6.0 * s;
    g.font.draw("Dump instead of save (needs a garbage dump zone):", x, y, s, col::TEXT_DIM);
    y += lh + 2.0 * s;
    {
        let rows: [(&str, &mut bool); 7] = [
            ("Corpses and body parts", &mut o.dump_corpses),
            ("Skulls", &mut o.dump_skulls),
            ("Bones", &mut o.dump_bones),
            ("Shells", &mut o.dump_shells),
            ("Skins, chitin and scales", &mut o.dump_skins),
            ("Hair and wool", &mut o.dump_hair),
            ("Other (remains, rotten food, tattered clothes)", &mut o.dump_other),
        ];
        for (label, v) in rows {
            if super::checkbox(&mut g.ui, &g.font, x + 12.0 * s, y, *v, label) {
                *v = !*v;
            }
            y += GH * s + 8.0 * s;
        }
    }
    if o != before {
        g.sim.set_refuse_orders(o);
    }
    // Statistics (right column).
    let rx = c.x + c.w * 0.55;
    let mut ry = c.y + 4.0 * s;
    g.font.draw("Decay", rx, ry, 1.2 * s, col::TEXT_HI);
    ry += lh + 6.0 * s;
    let r = g.sim.decay_report();
    let lines = [
        format!("Food rotted: {}   withered: {}", r.rotted_food, r.withered),
        format!("Rotted away: {}", r.rotted_away),
        format!("Corpses rotted: {}   skeletons: {}", r.corpses_rotted, r.skeletons),
        format!("Body parts to bones: {}   hides rotted: {}", r.parts_to_bones, r.hides_rotted),
        format!("Moved to refuse: {}   marked for dumping: {}", r.to_refuse, r.dump_marked),
        format!("Miasma: up to {} sources, {} tiles", r.miasma_sources_max, r.miasma_tiles_max),
        format!("Vermin now: {}   seen: {}   ate: {}   killed: {}", r.vermin_live, r.vermin_spawned, r.vermin_ate, r.vermin_killed),
        format!("Worn on refuse piles: {} levels, {} items gone", r.wear_levels_refuse, r.items_worn_out),
        format!("Armor worn in combat: {} levels", r.wear_levels_combat),
        format!("Items with a rot timer: {}", r.rot_tracked),
    ];
    for l in &lines {
        g.font.draw(l, rx, ry, s, col::TEXT);
        ry += lh;
    }
    if !g.sim.decay_enabled() {
        g.font.draw("Decay is switched off (round-13 rules).", rx, ry + lh, s, col::TEXT_DIM);
    }
}
