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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let out = PathBuf::from(get("--out").unwrap_or_else(|| "target/preview.png".into()));
    let t0 = std::time::Instant::now();
    let mut gfx = Gfx::load(&rf_gfx::df_dir()).expect("load gfx");
    eprintln!("gfx loaded: {} pages in {:.0} ms", gfx.pages.len(), t0.elapsed().as_secs_f64() * 1000.0);
    let (world, default_z) = match get("--export") {
        Some(dir) => {
            let w = load_export(&PathBuf::from(dir));
            let z = w.units.iter().find(|u| u.flags.citizen).map(|u| u.pos.z).unwrap_or(w.dims.z as i16 / 2);
            (w, z)
        }
        None => synthetic_plus(),
    };
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
        for u in world.units.iter().take(12) {
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
                        gfx.terrain(&world, p, &mut sprites);
                        for b in world.buildings.iter().filter(|b| b.contains(p)) {
                            gfx.building(&world, b, p, &mut sprites);
                        }
                    }
                    1 => {
                        if let Some(v) = items_at.get(&(x, y)) {
                            for it in v {
                                gfx.item(&world, it, &mut sprites);
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
    if let Some(d) = out.parent() {
        let _ = std::fs::create_dir_all(d);
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
    World {
        meta: Meta { fort_name: manifest["fort_name"].as_str().unwrap_or("").into(), source: format!("export:{}", dir.display()), ..Default::default() },
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
