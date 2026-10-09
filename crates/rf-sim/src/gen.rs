//! Synthetic large fort for benchmarks and stress tests.
//!
//! Built on top of `rf_core::World::synthetic` (same tiletypes and materials): a 120x120 dug
//! hall with pillars, five stair shafts to the surface, a ring of mining designations around
//! the hall, stair designations going down, designated trees on the surface, stone/wood/food
//! stockpiles, drinks and food, loose boulders to haul, beds, a meeting hall, N dwarves and
//! N/4 cats.

use crate::rng::Rng;
use rf_core::*;

fn tt(w: &World, name: &str) -> u16 {
    w.tiletypes.by_name(name).unwrap_or_else(|| panic!("tiletype {name}"))
}

/// Surface z-level of `World::synthetic(_, _, dz)`.
pub fn synthetic_surface_z(dz: u16) -> i16 {
    (dz as i16 * 2 / 3).max(2)
}

pub struct BenchLayout {
    pub hall_z: i16,
    pub surface_z: i16,
    pub hall: (i16, i16, i16, i16),
    pub stone_pile: (Pos, Pos),
    pub wood_pile: (Pos, Pos),
    pub food_pile: (Pos, Pos),
}

pub fn bench_layout(dx: u16, dy: u16, dz: u16) -> BenchLayout {
    let surface_z = synthetic_surface_z(dz);
    let hall_z = (surface_z - 4).max(0);
    let (cx, cy) = (dx as i16 / 2, dy as i16 / 2);
    let hw = (dx.min(dy) as i16 * 5 / 16).max(8);
    let hall = (cx - hw, cy - hw, cx + hw - 1, cy + hw - 1);
    let stone_pile = (Pos::new(hall.0 + 2, hall.1 + 2, hall_z), Pos::new(hall.0 + 2 + hw - 4, hall.1 + 2 + hw / 3, hall_z));
    let wood_pile = (Pos::new(hall.2 - 12, hall.1 + 2, hall_z), Pos::new(hall.2 - 3, hall.1 + 11, hall_z));
    let food_pile = (Pos::new(hall.0 + 2, hall.3 - 21, hall_z), Pos::new(hall.0 + 21, hall.3 - 2, hall_z));
    BenchLayout { hall_z, surface_z, hall, stone_pile, wood_pile, food_pile }
}

/// Build the benchmark fort. `units` dwarves (plus units/4 cats). Deterministic in `seed`.
pub fn bench_world(dx: u16, dy: u16, dz: u16, units: usize, seed: u64) -> World {
    let mut w = World::synthetic(dx, dy, dz);
    w.units.clear();
    w.items.clear();
    w.meta.fort_name = "Benchfort".into();
    let l = bench_layout(dx, dy, dz);
    let mut r = Rng::new(seed ^ 0xBE4C);
    let (granite, oak, wine, helmet) = (0u16, 3u16, 5u16, 6u16);
    let floors = [tt(&w, "StoneFloor1"), tt(&w, "StoneFloor2"), tt(&w, "StoneFloor3"), tt(&w, "StoneFloor4")];
    let (x1, y1, x2, y2) = l.hall;
    let hz = l.hall_z;
    // The hall, with a pillar every 8 tiles.
    for y in y1..=y2 {
        for x in x1..=x2 {
            let p = Pos::new(x, y, hz);
            let pillar = (x - x1) % 8 == 7 && (y - y1) % 8 == 7;
            if !w.in_bounds(p) || pillar {
                continue;
            }
            let f = floors[r.below(4) as usize];
            let t = w.tile_mut(p).unwrap();
            t.tt = f;
            t.mat = granite;
            t.desig = 0;
        }
    }
    // Stair shafts (the centre one exists in the synthetic world already).
    let shafts = [(x1 + 3, y1 + 3), (x2 - 3, y1 + 3), (x1 + 3, y2 - 3), (x2 - 3, y2 - 3), (dx as i16 / 2, dy as i16 / 2)];
    let (st_u, st_d, st_ud) = (tt(&w, "StoneStairU"), tt(&w, "StoneStairD"), tt(&w, "StoneStairUD"));
    for &(sx, sy) in &shafts {
        for z in hz..=l.surface_z {
            let id = if z == l.surface_z {
                st_d
            } else if z == hz {
                st_u
            } else {
                st_ud
            };
            let t = w.tile_mut(Pos::new(sx, sy, z)).unwrap();
            t.tt = id;
            t.mat = granite;
            t.desig = 0;
        }
    }
    // Mining ring around the hall (6 tiles thick).
    for y in y1 - 6..=y2 + 6 {
        for x in x1 - 6..=x2 + 6 {
            if x >= x1 && x <= x2 && y >= y1 && y <= y2 {
                continue;
            }
            if let Some(t) = w.tile_mut(Pos::new(x, y, hz)) {
                t.desig = Dig::Dig as u8;
            }
        }
    }
    // Up/down stair columns going down from the hall floor.
    for k in 0..4i16 {
        let (sx, sy) = (x1 + 12 + k * 20, y1 + 30);
        for z in (hz - 8).max(0)..=hz {
            if let Some(t) = w.tile_mut(Pos::new(sx, sy, z)) {
                t.desig = Dig::UpDownStair as u8;
            }
        }
    }
    // Trees on the surface (trunk at ground level and one above), designated for felling.
    let trunk = tt(&w, "TreeTrunkPillar");
    let branch = tt(&w, "TreeBranches");
    let sz = l.surface_z;
    let mut placed = 0;
    let mut guard = 0;
    while placed < 300 && guard < 10_000 {
        guard += 1;
        let x = 2 + r.below(dx as u32 - 4) as i16;
        let y = 2 + r.below(dy as u32 - 4) as i16;
        let base = Pos::new(x, y, sz);
        if shafts.iter().any(|&(sx, sy)| (sx - x).abs() <= 2 && (sy - y).abs() <= 2) {
            continue;
        }
        let ok = (-2..=2).all(|dy| (-2..=2).all(|dx| w.shape(base.offset(dx, dy, 0)) == Shape::Floor && w.shape(base.offset(dx, dy, 1)) == Shape::Empty));
        if !ok {
            continue;
        }
        for (p, id) in [(base, trunk), (base.offset(0, 0, 1), trunk)] {
            let t = w.tile_mut(p).unwrap();
            t.tt = id;
            t.mat = oak;
            t.desig = 0;
        }
        for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            if let Some(t) = w.tile_mut(base.offset(ox, oy, 1)) {
                t.tt = branch;
                t.mat = oak;
            }
        }
        w.tile_mut(base).unwrap().desig = Dig::Dig as u8;
        w.plants.push(Plant { pos: base, token: "OAK".into(), tree: true, grow_counter: 200_000, ..Default::default() });
        placed += 1;
    }
    // Stockpiles.
    let mut bid = 1;
    for (a, b, cat) in [(l.stone_pile.0, l.stone_pile.1, "stone"), (l.wood_pile.0, l.wood_pile.1, "wood"), (l.food_pile.0, l.food_pile.1, "food")] {
        w.buildings.push(Building {
            id: bid,
            kind: "Stockpile".into(),
            x1: a.x,
            y1: a.y,
            x2: b.x,
            y2: b.y,
            z: hz,
            center: Pos::new((a.x + b.x) / 2, (a.y + b.y) / 2, hz),
            built: true,
            owner: -1,
            stockpile: Some(StockpileSettings { categories: vec![cat.into()], ..Default::default() }),
            ..Default::default()
        });
        bid += 1;
    }
    // Meeting hall in the middle of the hall.
    let (cx, cy) = (dx as i16 / 2, dy as i16 / 2);
    w.buildings.push(Building {
        id: bid,
        kind: "Civzone".into(),
        subtype: "MeetingHall".into(),
        x1: cx - 10,
        y1: cy - 10,
        x2: cx + 10,
        y2: cy + 10,
        z: hz,
        center: Pos::new(cx, cy, hz),
        built: true,
        owner: -1,
        ..Default::default()
    });
    bid += 1;
    // Floor tiles of the hall for placing things.
    let free_floor = |w: &World, r: &mut Rng| -> Pos {
        loop {
            let x = x1 + r.below((x2 - x1 + 1) as u32) as i16;
            let y = y1 + r.below((y2 - y1 + 1) as u32) as i16;
            let p = Pos::new(x, y, hz);
            if w.shape(p) == Shape::Floor {
                return p;
            }
        }
    };
    // Beds along the east part of the hall; half of them owned.
    let bed_count = units / 2;
    for k in 0..bed_count {
        let p = Pos::new(x2 - 1 - (k as i16 % 30), y1 + 20 + (k as i16 / 30) * 2, hz);
        if w.shape(p) != Shape::Floor {
            continue;
        }
        w.buildings.push(Building {
            id: bid,
            kind: "Bed".into(),
            x1: p.x,
            y1: p.y,
            x2: p.x,
            y2: p.y,
            z: hz,
            center: p,
            built: true,
            owner: if k % 2 == 0 { k as i32 + 1 } else { -1 },
            ..Default::default()
        });
        bid += 1;
    }
    // Items.
    let mut next_id = 1000;
    let mut add = |w: &mut World, kind: &str, mat: u16, stack: i32, p: Pos, desc: &str| {
        w.items.push(Item {
            id: next_id,
            kind: kind.into(),
            mat,
            pos: p,
            stack,
            container: -1,
            holder: -1,
            building: -1,
            desc: desc.into(),
            flags: ItemFlags { on_ground: true, ..Default::default() },
            ..Default::default()
        });
        next_id += 1;
    };
    let (fa, fb) = l.food_pile;
    let fw = (fb.x - fa.x + 1) as u32;
    let fh = (fb.y - fa.y + 1) as u32;
    for k in 0..units * 2 {
        let p = Pos::new(fa.x + r.below(fw) as i16, fa.y + r.below(fh) as i16, hz);
        add(&mut w, "DRINK", wine, 5, p, "dwarven wine");
        let p = if k % 2 == 0 { free_floor(&w, &mut r) } else { Pos::new(fa.x + r.below(fw) as i16, fa.y + r.below(fh) as i16, hz) };
        add(&mut w, "PLANT", helmet, 5, p, "plump helmet");
    }
    for _ in 0..units {
        let p = free_floor(&w, &mut r);
        add(&mut w, "BOULDER", granite, 1, p, "granite boulder");
    }
    for _ in 0..units / 4 {
        let p = free_floor(&w, &mut r);
        add(&mut w, "WOOD", oak, 1, p, "oak log");
    }
    // Dwarves and cats.
    let names = ["Urist", "Kadol", "Doren", "Ingish", "Mafol", "Zon", "Litast", "Bomrek", "Sodel", "Datan"];
    for i in 0..units {
        let p = free_floor(&w, &mut r);
        let mut labors: Vec<String> = ["HAUL_STONE", "HAUL_WOOD", "HAUL_FOOD", "HAUL_ITEM", "HAUL_REFUSE", "HAUL_BODY"].iter().map(|s| s.to_string()).collect();
        let prof = if i % 4 == 0 {
            labors.push("MINE".into());
            "MINER"
        } else if i % 20 == 1 {
            labors.push("CUTWOOD".into());
            "WOODCUTTER"
        } else {
            "PEASANT"
        };
        w.units.push(Unit {
            id: i as i32 + 1,
            race: "DWARF".into(),
            caste: if i % 2 == 0 { "MALE".into() } else { "FEMALE".into() },
            name: format!("{} Benchson {}", names[i % names.len()], i + 1),
            profession: prof.into(),
            pos: p,
            flags: UnitFlags { citizen: true, resident: true, ..Default::default() },
            sex: (i % 2 == 0) as i8,
            age: 30.0 + (i % 50) as f32,
            hunger_timer: r.below(50_000) as i32,
            thirst_timer: r.below(30_000) as i32,
            sleepiness_timer: r.below(60_000) as i32,
            skills: vec![Skill { id: "MINING".into(), rating: r.below(10) as i32, xp: 0 }],
            labors,
            walk_speed: 900,
            hist_fig_id: -1,
            squad_id: -1,
            ..Default::default()
        });
    }
    for i in 0..units / 4 {
        let p = free_floor(&w, &mut r);
        w.units.push(Unit {
            id: 100_000 + i as i32,
            race: "CAT".into(),
            caste: "FEMALE".into(),
            name: "Stray cat".into(),
            profession: "STANDARD".into(),
            pos: p,
            flags: UnitFlags { animal: true, tame: true, ..Default::default() },
            walk_speed: 600,
            hist_fig_id: -1,
            squad_id: -1,
            ..Default::default()
        });
    }
    w
}
