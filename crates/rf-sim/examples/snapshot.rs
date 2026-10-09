//! Run the synthetic fort and print an ASCII view of one z-level plus unit statuses.
//!   cargo run -p rf-sim --release --example snapshot -- [--ticks 3000] [--z 9]

use rf_core::{Pos, Shape, TtMaterial, World};
use rf_sim::{DesignationKind, Sim};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).and_then(|s| s.parse::<i64>().ok());
    let ticks = get("--ticks").unwrap_or(3000) as u32;
    let (dx, dy, dz) = (48u16, 48u16, 20u16);
    let mut sim = Sim::new(World::synthetic(dx, dy, dz), 7);
    let hz = ((dz as i16 * 2 / 3).max(2) - 4).max(0);
    let z = get("--z").unwrap_or(hz as i64) as i16;
    let (cx, cy) = (dx as i16 / 2, dy as i16 / 2);
    sim.designate(Pos::new(cx - 16, cy - 4, hz), Pos::new(cx - 7, cy - 1, hz), DesignationKind::Mine);
    sim.designate(Pos::new(cx + 3, cy - 3, hz), Pos::new(cx + 3, cy - 3, hz - 3), DesignationKind::UpDownStair);
    sim.create_stockpile(Pos::new(cx + 1, cy + 1, hz), Pos::new(cx + 6, cy + 4, hz), &["stone", "wood"]);
    sim.designate(Pos::new(0, 0, hz + 5), Pos::new(dx as i16 - 1, dy as i16 - 1, hz + 5), DesignationKind::ChopTrees);
    sim.run_ticks(ticks);
    let w = &sim.world;
    println!("tick {} ({} {} {}), z {z}", sim.ticks(), w.calendar.day(), w.calendar.month_name(), w.calendar.year);
    for y in cy - 8..=cy + 8 {
        let mut line = String::new();
        for x in cx - 20..=cx + 14 {
            let p = Pos::new(x, y, z);
            let unit = w.units.iter().find(|u| u.pos == p && !u.flags.dead);
            let item = w.items.iter().find(|i| i.pos == p && i.flags.on_ground);
            let t = w.tile(p).unwrap();
            let info = w.tt(p);
            let c = if let Some(u) = unit {
                if u.flags.animal { 'c' } else { '@' }
            } else if let Some(i) = item {
                match i.kind.as_str() {
                    "BOULDER" => '*',
                    "WOOD" => '=',
                    "DRINK" => '!',
                    "PLANT" => '%',
                    _ => '&',
                }
            } else if t.desig != 0 {
                'd'
            } else {
                match info.shape {
                    Shape::Wall if info.material == TtMaterial::Mineral => '$',
                    Shape::Wall => '#',
                    Shape::Floor if t.flags & rf_core::tile_flags::PILE != 0 => '_',
                    Shape::Floor => '.',
                    Shape::StairUp => '<',
                    Shape::StairDown => '>',
                    Shape::StairUpdown => 'X',
                    Shape::Ramp => '^',
                    Shape::RampTop => 'v',
                    Shape::Empty => ' ',
                    _ => '?',
                }
            };
            line.push(c);
        }
        println!("{line}");
    }
    for u in &w.units {
        let st = sim.unit_status(u.id);
        println!(
            "{:<18} {:<26} at ({:>2},{:>2},{:>2}) thirst {:>3} hunger {:>3} tired {:>3}  {:?}",
            u.name, st.activity, u.pos.x, u.pos.y, u.pos.z, st.thirst, st.hunger, st.tiredness, st.job_pos
        );
    }
    let s = sim.stats();
    println!("open jobs {}, jobs done {}, hauled {}, items {}", s.open_jobs, s.jobs_done, s.items_hauled, s.items);
    for a in sim.drain_announcements().iter().rev().take(5) {
        println!("[{}] {}", a.tick, a.text);
    }
}
