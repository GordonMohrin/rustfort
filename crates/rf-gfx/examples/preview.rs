//! Renders a PNG of a world level with the Premium sprites, to check the sprite lookups.
//!
//! cargo run -p rf-gfx --release --example preview -- --out target/preview.png [--z N]
//!     [--rect x0,y0,w,h] [--export <dir>]
//!
//! Without `--export` it renders `World::synthetic(64,64,30)` plus a few extra buildings,
//! items and terrain features. `--z` defaults to the dug hall level. With `--export` it loads
//! a minimal subset of a fort export (tiletypes, materials, map, units, items, buildings).
use rf_core::*;
use rf_gfx::{Gfx, Sprite};
use std::collections::HashMap;
use std::path::PathBuf;

/// What a "missing sprite" debug scan found: key -> (count, first position).
type Missing = HashMap<String, (usize, Pos)>;

fn note(m: &mut Missing, key: String, p: Pos) {
    m.entry(key).or_insert((0, p)).0 += 1;
}

/// Look up every visible tile/building/item/unit of level `z` and record those without sprites.
fn scan_level(gfx: &Gfx, world: &World, z: i16, missing: &mut Missing, marks: Option<&mut Vec<Pos>>) {
    let mut sprites = Vec::new();
    let mut marks = marks;
    let mark = |p: Pos, marks: &mut Option<&mut Vec<Pos>>| {
        if let Some(m) = marks.as_deref_mut() {
            m.push(p);
        }
    };
    for y in 0..world.dims.y as i16 {
        for x in 0..world.dims.x as i16 {
            let p = Pos::new(x, y, z);
            let Some(t) = world.tile(p) else { continue };
            if t.hidden() {
                continue;
            }
            let tt = world.tiletypes.get(t.tt);
            if tt.shape == Shape::Empty && t.liquid_depth() == 0 {
                continue;
            }
            sprites.clear();
            gfx.terrain(world, p, &mut sprites);
            if sprites.is_empty() {
                note(missing, format!("terrain {}", tt.name), p);
                mark(p, &mut marks);
            }
        }
    }
    // Zones are drawn by the UI (only in zone mode), not by sprites.
    for b in world.buildings.iter().filter(|b| b.z == z && b.kind != "Civzone") {
        if world.tile(b.center).map(|t| t.hidden()).unwrap_or(true) {
            continue;
        }
        let mut any = false;
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                let p = Pos::new(x, y, z);
                if b.contains(p) {
                    sprites.clear();
                    gfx.building(world, b, p, &mut sprites);
                    any |= !sprites.is_empty();
                }
            }
        }
        if !any {
            note(missing, format!("building {}:{}", b.kind, b.subtype), b.center);
            mark(b.center, &mut marks);
        }
    }
    for it in world.items.iter().filter(|i| i.pos.z == z && i.flags.on_ground && !i.flags.hidden) {
        if world.tile(it.pos).map(|t| t.hidden()).unwrap_or(true) {
            continue;
        }
        sprites.clear();
        gfx.item(world, it, &mut sprites);
        if sprites.is_empty() {
            note(missing, format!("item {}:{}", it.kind, it.subtype), it.pos);
            mark(it.pos, &mut marks);
        }
    }
    for u in world.units.iter().filter(|u| u.pos.z == z && !u.flags.dead && !u.flags.inactive) {
        sprites.clear();
        gfx.unit(world, u, &mut sprites);
        if sprites.is_empty() {
            note(missing, format!("unit {}", u.race), u.pos);
            mark(u.pos, &mut marks);
        }
    }
}

fn print_missing(title: &str, m: &Missing) {
    let mut v: Vec<(&String, &(usize, Pos))> = m.iter().collect();
    v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    let total: usize = v.iter().map(|e| e.1 .0).sum();
    eprintln!("missing sprites ({title}): {total} in {} kinds", v.len());
    for (k, (n, p)) in v {
        eprintln!("  {n:7}  {k:48} e.g. ({},{},{})", p.x, p.y, p.z);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let flag = |k: &str| args.iter().any(|a| a == k);
    let out = PathBuf::from(get("--out").unwrap_or_else(|| "target/preview.png".into()));
    let (mut world, default_z) = match get("--export") {
        Some(dir) => {
            let w = load_export(&PathBuf::from(dir));
            let z = w.units.iter().find(|u| u.flags.citizen).map(|u| u.pos.z).unwrap_or(w.dims.z as i16 / 2);
            (w, z)
        }
        None => synthetic_plus(),
    };
    // Season: `--month 0..11` (0 = Granite, spring) or `--tick <year tick>` sets the calendar
    // (trees show spring flowers, summer leaves, autumn colors, bare winter branches).
    if let Some(m) = get("--month").and_then(|s| s.parse::<i32>().ok()) {
        world.calendar.year_tick = m.clamp(0, 11) * TICKS_PER_MONTH + TICKS_PER_MONTH / 2;
    }
    if let Some(t) = get("--tick").and_then(|s| s.parse::<i32>().ok()) {
        world.calendar.year_tick = t.clamp(0, TICKS_PER_YEAR - 1);
    }
    eprintln!("calendar: year tick {} ({} {})", world.calendar.year_tick, world.calendar.month_name(), world.calendar.season_name());
    // Mods: `--mods a:1,b:2` overrides the world's list; `--mods none` = vanilla only.
    let mods: Vec<String> = match get("--mods") {
        Some(s) if s == "none" => Vec::new(),
        Some(s) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        None => world.meta.mods.clone(),
    };
    let t0 = std::time::Instant::now();
    let mut gfx = Gfx::load_with_mods(&rf_gfx::df_dir(), &mods).expect("load gfx");
    eprintln!(
        "gfx loaded: {} pages in {:.0} ms; mods {:?} {:?}",
        gfx.pages.len(),
        t0.elapsed().as_secs_f64() * 1000.0,
        gfx.mods_loaded,
        gfx.mod_warnings
    );
    let z: i16 = get("--z").and_then(|s| s.parse().ok()).unwrap_or(default_z);
    let (x0, y0, w, h) = match get("--rect") {
        Some(r) => {
            let v: Vec<i16> = r.split(',').filter_map(|s| s.parse().ok()).collect();
            (v[0], v[1], v[2], v[3])
        }
        None => (0, 0, world.dims.x as i16, world.dims.y as i16),
    };
    let t1 = std::time::Instant::now();
    let newp = gfx.prepare(&world);
    eprintln!("prepare: {} variant pages in {:.0} ms", newp, t1.elapsed().as_secs_f64() * 1000.0);
    if args.iter().any(|a| a == "--debug-units") {
        let mut v = Vec::new();
        eprintln!("DWARF: {:?}\nCAT: {:?}", gfx.creature_info("DWARF"), gfx.creature_info("CAT"));
        // `--race R` limits the listing to one race (e.g. DEMON_2).
        let race = get("--race");
        for u in world.units.iter().filter(|u| race.as_deref().map(|r| u.race == r).unwrap_or(true)).take(12) {
            v.clear();
            gfx.unit(&world, u, &mut v);
            eprintln!("unit {} {} {} {} at {:?}: {} sprites {:?}", u.id, u.race, u.caste, u.profession, u.pos, v.len(), v.first());
        }
    }
    let ts = 32i32;
    let (iw, ih) = (w as i32 * ts, h as i32 * ts);
    let mut img = vec![0u8; (iw * ih * 4) as usize];
    for c in img.chunks_exact_mut(4) {
        c.copy_from_slice(&[12, 10, 16, 255]);
    }
    let mut cache: HashMap<u16, (u32, u32, Vec<u8>)> = HashMap::new();
    let mut sprites: Vec<Sprite> = Vec::new();
    let mut nsprites = 0usize;
    let t2 = std::time::Instant::now();
    // Index things by tile.
    let mut items_at: HashMap<(i16, i16), Vec<&Item>> = HashMap::new();
    for it in &world.items {
        if it.pos.z == z && it.flags.on_ground {
            items_at.entry((it.pos.x, it.pos.y)).or_default().push(it);
        }
    }
    for pass in 0..3 {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let p = Pos::new(x, y, z);
                sprites.clear();
                match pass {
                    0 => {
                        // Like the app: twigs (tree canopy) are open, the level below shows through.
                        if world.shape(p) == Shape::Twig && z > 0 {
                            gfx.terrain(&world, p.offset(0, 0, -1), &mut sprites);
                            for s in sprites.iter_mut() {
                                *s = s.mul_tint([150, 150, 165, 255]);
                            }
                        }
                        gfx.terrain(&world, p, &mut sprites);
                        for b in world.buildings.iter().filter(|b| b.contains(p)) {
                            gfx.building(&world, b, p, &mut sprites);
                        }
                    }
                    1 => {
                        if let Some(v) = items_at.get(&(x, y)) {
                            for it in v {
                                gfx.item(&world, it, &mut sprites);
                                // Like the app: forbidden / dump / melt / hidden markers on top.
                                sprites.extend(gfx.item_marker(it));
                            }
                        }
                    }
                    _ => {
                        for u in world.units.iter().filter(|u| u.pos == p) {
                            gfx.unit(&world, u, &mut sprites);
                        }
                    }
                }
                for s in &sprites {
                    nsprites += 1;
                    let page = cache.entry(s.page).or_insert_with(|| gfx.page_rgba(s.page).unwrap_or((0, 0, Vec::new())));
                    let info = &gfx.pages[s.page as usize];
                    let cx = if pass == 2 { s.unit_center_px(ts as u32) } else { 0 };
                    blit(&mut img, iw, ih, page, info.tile_w, info.tile_h, s, (x - x0) as i32 * ts + cx, (y - y0) as i32 * ts, ts);
                }
            }
        }
    }
    eprintln!("composed {nsprites} sprites in {:.0} ms ({} pages decoded)", t2.elapsed().as_secs_f64() * 1000.0, cache.len());
    // Debug: mark everything without a sprite (magenta frame) and print a histogram.
    if flag("--debug-missing") || flag("--scan") {
        let mut missing = Missing::new();
        let mut marks = Vec::new();
        scan_level(&gfx, &world, z, &mut missing, Some(&mut marks));
        for p in marks {
            if p.x < x0 || p.y < y0 || p.x >= x0 + w || p.y >= y0 + h {
                continue;
            }
            let (ox, oy) = ((p.x - x0) as i32 * ts, (p.y - y0) as i32 * ts);
            for i in 0..ts {
                for (xx, yy) in [(ox + i, oy), (ox + i, oy + ts - 1), (ox, oy + i), (ox + ts - 1, oy + i), (ox + i, oy + i), (ox + ts - 1 - i, oy + i)] {
                    let di = ((yy * iw + xx) * 4) as usize;
                    if di + 3 < img.len() {
                        img[di..di + 4].copy_from_slice(&[255, 0, 255, 255]);
                    }
                }
            }
        }
        print_missing(&format!("z={z}"), &missing);
    }
    if flag("--bench") {
        // Lookup cost: terrain() for every tile of 16 levels around z, plus items/buildings.
        let mut v: Vec<Sprite> = Vec::with_capacity(64);
        let (z0, z1) = ((z - 8).max(0), (z + 8).min(world.dims.z as i16));
        let t4 = std::time::Instant::now();
        let mut calls = 0usize;
        let mut total = 0usize;
        for zz in z0..z1 {
            for y in 0..world.dims.y as i16 {
                for x in 0..world.dims.x as i16 {
                    v.clear();
                    gfx.terrain(&world, Pos::new(x, y, zz), &mut v);
                    total += v.len();
                    calls += 1;
                }
            }
        }
        let dt = t4.elapsed().as_secs_f64();
        eprintln!(
            "bench: {calls} terrain lookups in {:.1} ms = {:.0} ns/tile ({} sprites); a 20k-tile frame costs {:.2} ms",
            dt * 1000.0,
            dt * 1e9 / calls as f64,
            total,
            dt * 1000.0 / calls as f64 * 20_000.0
        );
        let t5 = std::time::Instant::now();
        for it in &world.items {
            v.clear();
            gfx.item(&world, it, &mut v);
        }
        for u in &world.units {
            v.clear();
            gfx.unit(&world, u, &mut v);
        }
        eprintln!("bench: {} items + {} units in {:.2} ms", world.items.len(), world.units.len(), t5.elapsed().as_secs_f64() * 1000.0);
    }
    if flag("--scan") {
        // Whole map, every level.
        let mut missing = Missing::new();
        let t3 = std::time::Instant::now();
        for zz in 0..world.dims.z as i16 {
            scan_level(&gfx, &world, zz, &mut missing, None);
        }
        eprintln!("scan of all {} levels took {:.0} ms", world.dims.z, t3.elapsed().as_secs_f64() * 1000.0);
        print_missing("all levels", &missing);
    }
    if let Some(d) = out.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // `--half`: downscale 2x (big rectangles stay readable as PNG).
    if flag("--half") {
        let (hw, hh) = (iw / 2, ih / 2);
        let mut small = vec![0u8; (hw * hh * 4) as usize];
        for y in 0..hh {
            for x in 0..hw {
                for c in 0..4 {
                    let s: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                        .iter()
                        .map(|(dx, dy)| img[(((y * 2 + dy) * iw + x * 2 + dx) * 4 + c) as usize] as u32)
                        .sum();
                    small[((y * hw + x) * 4 + c) as usize] = (s / 4) as u8;
                }
            }
        }
        image::save_buffer(&out, &small, hw as u32, hh as u32, image::ColorType::Rgba8).expect("save");
        println!("wrote {} ({}x{} px, z={z})", out.display(), hw, hh);
        return;
    }
    image::save_buffer(&out, &img, iw as u32, ih as u32, image::ColorType::Rgba8).expect("save");
    println!("wrote {} ({}x{} px, z={z})", out.display(), iw, ih);
}

#[allow(clippy::too_many_arguments)]
fn blit(img: &mut [u8], iw: i32, ih: i32, page: &(u32, u32, Vec<u8>), tw: u32, th: u32, s: &Sprite, ox: i32, oy: i32, ts: i32) {
    let (pw, ph, px) = (page.0 as i32, page.1 as i32, &page.2);
    if px.is_empty() {
        return;
    }
    let (sw, sh) = ((s.tw as u32 * tw) as i32, (s.th as u32 * th) as i32);
    let (sx, sy) = ((s.tx as u32 * tw) as i32, (s.ty as u32 * th) as i32);
    // Scale page tiles to the map tile size (interface pages use other sizes).
    let (scx, scy) = (ts as f32 / tw as f32, ts as f32 / th as f32);
    let (dw, dh) = ((sw as f32 * scx) as i32, (sh as f32 * scy) as i32);
    let dx0 = ox + s.dx as i32 * ts;
    let dy0 = oy + s.dy as i32 * ts;
    for yy in 0..dh {
        let py = sy + (yy as f32 / scy) as i32;
        let dy = dy0 + yy;
        if py >= ph || dy < 0 || dy >= ih {
            continue;
        }
        for xx in 0..dw {
            let pxx = sx + (xx as f32 / scx) as i32;
            let dx = dx0 + xx;
            if pxx >= pw || dx < 0 || dx >= iw {
                continue;
            }
            let si = ((py * pw + pxx) * 4) as usize;
            let a = px[si + 3] as u32 * s.tint[3] as u32 / 255;
            if a == 0 {
                continue;
            }
            let di = ((dy * iw + dx) * 4) as usize;
            for c in 0..3 {
                let sc = px[si + c] as u32 * s.tint[c] as u32 / 255;
                let dc = img[di + c] as u32;
                img[di + c] = ((sc * a + dc * (255 - a)) / 255) as u8;
            }
            img[di + 3] = 255;
        }
    }
}

/// The synthetic world plus buildings, items and terrain features for a richer preview.
fn synthetic_plus() -> (World, i16) {
    let mut w = World::synthetic(64, 64, 30);
    let mut infos = w.tiletypes.infos.clone();
    let mut add_tt = |name: &str, shape: Shape, m: TtMaterial, sp: TtSpecial, v: u8| -> u16 {
        let id = infos.len() as u16;
        infos.push(TiletypeInfo { id, name: name.into(), shape, material: m, special: sp, variant: v, direction: String::new() });
        id
    };
    let smooth_wall = add_tt("StoneWallSmooth", Shape::Wall, TtMaterial::Stone, TtSpecial::Smooth, 0);
    let soil_floor = add_tt("SoilFloor3", Shape::Floor, TtMaterial::Soil, TtSpecial::Normal, 3);
    w.tiletypes = TiletypeTable::new(infos);
    let tt = |w: &World, n: &str| w.tiletypes.by_name(n).unwrap();
    let (cx, cy) = (32i16, 32i16);
    let surface_z = 20i16;
    let hall_z = 16i16;
    // A second room (smoothed) east of the hall, connected by a corridor.
    let floor = tt(&w, "StoneFloor2");
    let smooth_floor = tt(&w, "StoneFloorSmooth");
    for x in cx + 7..cx + 10 {
        w.tile_mut(Pos::new(x, cy + 3, hall_z)).unwrap().tt = floor;
    }
    for y in cy + 1..cy + 6 {
        for x in cx + 10..cx + 16 {
            w.tile_mut(Pos::new(x, y, hall_z)).unwrap().tt = smooth_floor;
        }
    }
    for y in cy..cy + 7 {
        for x in cx + 9..cx + 17 {
            let p = Pos::new(x, y, hall_z);
            if w.shape(p) == Shape::Wall && (y == cy || y == cy + 6 || x == cx + 9 || x == cx + 16) {
                w.tile_mut(p).unwrap().tt = smooth_wall;
            }
        }
    }
    // Boulders, pebbles and a ramp in the hall.
    let boulder = tt(&w, "StoneBoulder");
    let pebbles = tt(&w, "StonePebbles1");
    let ramp = tt(&w, "StoneRamp");
    for (x, y, t) in [(cx - 6, cy - 4, boulder), (cx - 5, cy - 4, pebbles), (cx + 6, cy + 4, ramp), (cx - 6, cy + 4, boulder)] {
        w.tile_mut(Pos::new(x, y, hall_z)).unwrap().tt = t;
    }
    // A small water puddle and an ore vein face into the hall.
    w.tile_mut(Pos::new(cx + 3, cy + 4, hall_z)).unwrap().liquid = 4;
    let ore = tt(&w, "MineralWall");
    for x in cx - 3..cx + 1 {
        let t = w.tile_mut(Pos::new(x, cy - 5, hall_z)).unwrap();
        t.tt = ore;
        t.mat = 2;
    }
    // Surface: some dirt patches next to grass, and a dig designation.
    for y in 8..12 {
        for x in 8..14 {
            let t = w.tile_mut(Pos::new(x, y, surface_z)).unwrap();
            t.tt = soil_floor;
            t.mat = 1;
        }
    }
    // Hide most of the rock that is not next to a dug tile (like DF).
    let dims = w.dims;
    let mut hide = Vec::new();
    for z in 0..dims.z as i16 {
        for y in 0..dims.y as i16 {
            for x in 0..dims.x as i16 {
                let p = Pos::new(x, y, z);
                if w.shape(p) != Shape::Wall || w.tt(p).material == TtMaterial::Tree {
                    continue;
                }
                let mut near_open = false;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let q = p.offset(dx, dy, 0);
                        if w.in_bounds(q) && !matches!(w.shape(q), Shape::Wall) {
                            near_open = true;
                        }
                    }
                }
                if !near_open {
                    hide.push(p);
                }
            }
        }
    }
    for p in hide {
        w.tile_mut(p).unwrap().flags |= tile_flags::HIDDEN;
    }
    // Buildings.
    let mut bid = 500;
    let mut add_b = |w: &mut World, kind: &str, sub: &str, x1: i16, y1: i16, x2: i16, y2: i16, mat: u16| {
        w.buildings.push(Building {
            id: bid,
            kind: kind.into(),
            subtype: sub.into(),
            x1,
            y1,
            x2,
            y2,
            z: hall_z,
            center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, hall_z),
            mat,
            stage: -1,
            built: true,
            owner: -1,
            ..Default::default()
        });
        bid += 1;
    };
    add_b(&mut w, "Workshop", "Carpenters", cx - 6, cy + 1, cx - 4, cy + 3, 3);
    add_b(&mut w, "Workshop", "Masons", cx - 2, cy + 2, cx, cy + 4, 0);
    add_b(&mut w, "Furnace", "Smelter", cx + 12, cy + 1, cx + 14, cy + 3, 0);
    add_b(&mut w, "Bed", "", cx + 11, cy + 5, cx + 11, cy + 5, 3);
    add_b(&mut w, "Table", "", cx + 14, cy + 5, cx + 14, cy + 5, 0);
    add_b(&mut w, "Chair", "", cx + 13, cy + 5, cx + 13, cy + 5, 0);
    add_b(&mut w, "Door", "", cx + 9, cy + 3, cx + 9, cy + 3, 3);
    add_b(&mut w, "Stockpile", "", cx + 1, cy - 3, cx + 3, cy - 1, NO_MAT);
    // Items: boulders and bars on the stockpile.
    let mut iid = 5000;
    for (x, y, kind, mat) in [(cx + 1, cy - 3, "BOULDER", 0u16), (cx + 2, cy - 3, "BOULDER", 2), (cx + 3, cy - 3, "BAR", 2), (cx + 1, cy - 2, "BLOCKS", 0)] {
        w.items.push(Item {
            id: iid,
            kind: kind.into(),
            mat,
            pos: Pos::new(x, y, hall_z),
            stack: 1,
            container: -1,
            holder: -1,
            building: -1,
            flags: ItemFlags { on_ground: true, ..Default::default() },
            ..Default::default()
        });
        iid += 1;
    }
    // A dwarf on the surface and a few more professions for clothing colors.
    let profs = ["MINER", "FARMER", "MECHANIC", "JEWELER", "METALSMITH", "FISHERMAN", "HUNTER", "PEASANT"];
    for (i, prof) in profs.iter().enumerate() {
        let mut u = w.units[0].clone();
        u.id = 200 + i as i32;
        u.profession = (*prof).into();
        u.caste = if i % 2 == 0 { "FEMALE".into() } else { "MALE".into() };
        u.pos = Pos::new(cx - 4 + i as i16, cy + 5 - 1, hall_z);
        if u.pos.y > cy + 4 {
            u.pos.y = cy + 4;
        }
        w.units.push(u);
    }
    // Engravings in the smoothed room: the west wall and two floor tiles.
    for y in cy + 1..cy + 6 {
        w.engravings.push(Engraving { pos: Pos::new(cx + 9, y, hall_z), quality: 3, floor: false, mat: 0, engraver: -1, skill: 5 });
    }
    for x in [cx + 11, cx + 12] {
        w.engravings.push(Engraving { pos: Pos::new(x, cy + 2, hall_z), quality: 4, floor: true, mat: 0, engraver: -1, skill: 8 });
    }
    // A bar marked for melting and a forbidden one.
    if let Some(it) = w.items.iter_mut().find(|i| i.kind == "BAR") {
        it.flags.melt = true;
    }
    // A forgotten beast (no raws: the sim makes them) and a caste-specific animal.
    let mut fb = w.units[0].clone();
    fb.id = 400;
    fb.race = "FORGOTTEN_BEAST_1".into();
    fb.caste = "NEUTER".into();
    fb.flags = UnitFlags { hostile: true, animal: true, ..Default::default() };
    fb.pos = Pos::new(cx - 2, cy - 3, hall_z);
    w.units.push(fb);
    let mut hen = w.units.iter().find(|u| u.race == "CAT").unwrap().clone();
    hen.id = 401;
    hen.race = "BIRD_CHICKEN".into();
    hen.caste = "FEMALE".into();
    hen.pos = Pos::new(cx + 1, cy + 3, hall_z);
    w.units.push(hen);
    let mut dog = w.units.iter().find(|u| u.race == "CAT").unwrap().clone();
    dog.id = 300;
    dog.race = "DOG".into();
    dog.pos = Pos::new(cx + 4, cy + 2, hall_z);
    w.units.push(dog);
    (w, hall_z)
}

/// Minimal private loader for `export/<fort>/` (the real importer is rf-import).
fn load_export(dir: &std::path::Path) -> World {
    use std::fs;
    let manifest: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).expect("manifest")).unwrap();
    let d = &manifest["dims"];
    let dims = Dims { x: d["x"].as_u64().unwrap() as u16, y: d["y"].as_u64().unwrap() as u16, z: d["z"].as_u64().unwrap() as u16 };
    // Parsed by hand with the `from_df` helpers: DF spells some specials `WORN_1`, which the
    // serde derive of `TtSpecial` (expects `WORN1`) rejects.
    let tv: Vec<serde_json::Value> = serde_json::from_str(&fs::read_to_string(dir.join("tiletypes.json")).unwrap()).unwrap();
    let infos: Vec<TiletypeInfo> = tv
        .iter()
        .map(|v| TiletypeInfo {
            id: v["id"].as_u64().unwrap_or(0) as u16,
            name: v["name"].as_str().unwrap_or("").into(),
            shape: Shape::from_df(v["shape"].as_str().unwrap_or("")),
            material: TtMaterial::from_df(v["material"].as_str().unwrap_or("")),
            special: TtSpecial::from_df(v["special"].as_str().unwrap_or("")),
            variant: v["variant"].as_u64().unwrap_or(0) as u8,
            direction: v["direction"].as_str().unwrap_or("").into(),
        })
        .collect();
    let materials: Vec<MaterialInfo> = serde_json::from_str(&fs::read_to_string(dir.join("materials.json")).unwrap()).unwrap();
    let mut tiles = Vec::with_capacity(dims.x as usize * dims.y as usize * dims.z as usize);
    for z in 0..dims.z {
        let b = fs::read(dir.join("map").join(format!("z{z:03}.bin"))).unwrap_or_default();
        for c in b.chunks_exact(8) {
            tiles.push(Tile {
                tt: u16::from_le_bytes([c[0], c[1]]),
                mat: u16::from_le_bytes([c[2], c[3]]),
                flags: u16::from_le_bytes([c[4], c[5]]),
                liquid: c[6],
                desig: c[7],
            });
        }
    }
    tiles.resize(dims.x as usize * dims.y as usize * dims.z as usize, Tile::default());
    fn jsonl<T: serde::de::DeserializeOwned>(p: std::path::PathBuf) -> Vec<T> {
        std::fs::read_to_string(p).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }
    let mods: Vec<String> =
        manifest["mods"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    let plant_raws: Vec<PlantRaw> =
        fs::read_to_string(dir.join("plant_raws.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let creature_raws: Vec<CreatureRaw> =
        fs::read_to_string(dir.join("creature_raws.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    // The export's calendar (`manifest.json` `year` / `year_tick`, if present).
    let calendar = Calendar {
        year: manifest["year"].as_i64().unwrap_or(0) as i32,
        year_tick: manifest["year_tick_end"].as_i64().or(manifest["year_tick_start"].as_i64()).unwrap_or(0) as i32,
    };
    World {
        meta: Meta {
            fort_name: manifest["fort_name"].as_str().unwrap_or("").into(),
            source: format!("export:{}", dir.display()),
            mods,
            ..Default::default()
        },
        plant_raws,
        creature_raws,
        calendar,
        dims,
        tiles,
        tiletypes: TiletypeTable::new(infos),
        materials,
        units: jsonl(dir.join("units.jsonl")),
        items: jsonl(dir.join("items.jsonl")),
        buildings: jsonl(dir.join("buildings.jsonl")),
        plants: jsonl(dir.join("plants.jsonl")),
        ..Default::default()
    }
}
