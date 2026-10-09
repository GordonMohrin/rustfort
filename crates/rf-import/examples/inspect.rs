//! Print a summary of a fort export:
//!   cargo run -p rf-import --release --example inspect -- [dir] [--z N] [--all-citizens]
//! dims, counts, citizens, item/building histograms, mods, work orders, workshop job queues,
//! farm plots with crops, wells, drink/food stock, burrows/squads, walk speeds, tile histograms
//! per z-range and an ASCII map of the z-level with the most citizens (or `--z N`).

use rf_core::{Pos, Shape, TtMaterial, World, NO_MAT};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn top<K: Ord + Clone>(m: &BTreeMap<K, usize>, n: usize) -> Vec<(K, usize)> {
    let mut v: Vec<(K, usize)> = m.iter().map(|(k, c)| (k.clone(), *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

fn shape_char(w: &World, p: Pos) -> char {
    let t = match w.tile(p) {
        Some(t) => t,
        None => return ' ',
    };
    if t.hidden() {
        return ' ';
    }
    if t.liquid_depth() > 0 {
        return if t.is_magma() { '!' } else { '~' };
    }
    let info = w.tiletypes.get(t.tt);
    match info.shape {
        Shape::Wall if info.material == TtMaterial::Tree || info.material == TtMaterial::Mushroom => 'T',
        Shape::Wall if info.material == TtMaterial::Construction => 'H',
        Shape::Wall => '#',
        Shape::Fortification => 'F',
        Shape::Floor if info.material.is_grass() => ',',
        Shape::Floor => '.',
        Shape::Boulder => 'o',
        Shape::Pebbles => ':',
        Shape::StairUp => '<',
        Shape::StairDown => '>',
        Shape::StairUpdown => 'X',
        Shape::Ramp => '^',
        Shape::RampTop => 'v',
        Shape::Branch | Shape::TrunkBranch | Shape::Twig => 't',
        Shape::Shrub => '"',
        Shape::Sapling => '\'',
        Shape::BrookBed | Shape::BrookTop => '~',
        Shape::Empty => ' ',
        Shape::EndlessPit => 'O',
        Shape::None => '?',
    }
}

fn building_char(kind: &str) -> Option<char> {
    Some(match kind {
        "Workshop" | "Furnace" => 'W',
        "Bed" => 'b',
        "Table" => 'n',
        "Chair" => 'h',
        "Door" => '+',
        "Hatch" => '=',
        "Cabinet" | "Box" => 'k',
        "Coffin" => '0',
        "Statue" => 'S',
        "Well" => 'w',
        "Weaponrack" | "Armorstand" => 'a',
        "Trap" => '^',
        "Bridge" => '_',
        "FarmPlot" => 'f',
        "Lever" => 'L',
        "Cage" => 'C',
        _ => return None,
    })
}

/// Round-2 data: mods, work orders, workshop queues, farm plots, wells, drink/food stock,
/// burrows and squads.
fn production(w: &World, dir: &std::path::Path) {
    let extras = rf_import::load_extras(dir);
    // mods
    if extras.mods.is_empty() {
        println!("\nmods ({}): {}", w.meta.mods.len(), w.meta.mods.join(", "));
    } else {
        println!("\nmods ({}, load order):", extras.mods.len());
        for m in &extras.mods {
            println!("  {:<36} {:<34} {}", m.id, m.name, m.version);
        }
    }

    // work orders
    println!("\nwork orders ({}):", w.work_orders.len());
    for o in &w.work_orders {
        let what = if !o.reaction.is_empty() { o.reaction.clone() } else { o.job.clone() };
        let mut prod = o.item_type.clone();
        if !o.item_subtype.is_empty() {
            prod = format!("{prod}/{}", o.item_subtype);
        }
        let mat = if !o.mat.is_empty() { o.mat.clone() } else { o.mat_category.join("+") };
        let conds: Vec<String> = o
            .conditions
            .iter()
            .map(|c| {
                let it = if c.item_subtype.is_empty() { c.item_type.clone() } else { format!("{}/{}", c.item_type, c.item_subtype) };
                let mut s = format!("{} {} {}", it, c.compare, c.value);
                if !c.mat.is_empty() {
                    s += &format!(" {}", c.mat);
                }
                if !c.flags.is_empty() {
                    s += &format!(" [{}]", c.flags.join(","));
                }
                s
            })
            .collect();
        println!(
            "  {:>5} {:<26} {:<22} {:<14} {:>3}/{:<3} {:<10} {}{}{}",
            o.id,
            what,
            prod,
            mat,
            o.amount_left,
            o.amount_total,
            o.frequency,
            if o.active { "active" } else { "inactive" },
            if o.workshop_id >= 0 { format!(" @{}", o.workshop_id) } else { String::new() },
            if conds.is_empty() { String::new() } else { format!("  if {}", conds.join(" and ")) }
        );
    }

    // workshops / furnaces with their queues
    let shops: Vec<_> = w.buildings.iter().filter(|b| b.kind == "Workshop" || b.kind == "Furnace").collect();
    let busy = shops.iter().filter(|b| !b.jobs.is_empty()).count();
    println!(
        "\nworkshops/furnaces ({}, {} with queued jobs, {} jobs in total):",
        shops.len(),
        busy,
        shops.iter().map(|b| b.jobs.len()).sum::<usize>()
    );
    for b in &shops {
        if b.jobs.is_empty() {
            continue;
        }
        let mut q: Vec<(String, usize)> = Vec::new();
        for j in &b.jobs {
            let mut s = if !j.reaction.is_empty() { j.reaction.clone() } else { j.job.clone() };
            if !j.item_subtype.is_empty() {
                s += &format!("({})", j.item_subtype);
            }
            if j.repeat {
                s += " R";
            }
            if j.suspended {
                s += " S";
            }
            if j.order_id >= 0 {
                s += &format!(" #{}", j.order_id);
            }
            match q.last_mut() {
                Some((last, n)) if *last == s => *n += 1,
                _ => q.push((s, 1)),
            }
        }
        let q: Vec<String> = q.into_iter().map(|(s, n)| if n > 1 { format!("{n}x {s}") } else { s }).collect();
        println!("  {:>6} {:<14} z{:<3} ({:>3},{:>3})  {}", b.id, b.subtype, b.z, b.x1, b.y1, q.join(", "));
    }
    let idle: Vec<String> = shops.iter().filter(|b| b.jobs.is_empty()).map(|b| b.subtype.clone()).collect();
    let mut idle_k: BTreeMap<String, usize> = BTreeMap::new();
    for s in idle {
        *idle_k.entry(s).or_default() += 1;
    }
    let idle_s: Vec<String> = top(&idle_k, 30).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("  idle: {}", idle_s.join(", "));

    // farm plots
    let farms: Vec<_> = w.buildings.iter().filter(|b| b.kind == "FarmPlot").collect();
    let raw = |t: &str| w.plant_raws.iter().find(|p| p.token == t);
    println!("\nfarm plots ({}; crops spring/summer/autumn/winter, - = fallow):", farms.len());
    for b in &farms {
        let crops: Vec<String> = b.crops.iter().map(|c| if c.is_empty() { "-".to_string() } else { c.clone() }).collect();
        let area = (b.x2 - b.x1 + 1) as i32 * (b.y2 - b.y1 + 1) as i32;
        let under = w.tile(b.center).map(|t| t.flags & rf_core::tile_flags::SUBTERRANEAN != 0).unwrap_or(false);
        let notes: Vec<String> = b
            .crops
            .iter()
            .filter(|c| !c.is_empty())
            .filter_map(|c| raw(c).map(|p| (c, p)))
            .filter(|(_, p)| if under { !p.underground } else { !p.aboveground })
            .map(|(c, _)| format!("{c} not {}", if under { "subterranean" } else { "surface" }))
            .collect();
        println!(
            "  {:>6} z{:<3} ({:>3},{:>3}) {:>2} tiles {:<5} {}{}",
            b.id,
            b.z,
            b.x1,
            b.y1,
            area,
            if under { "under" } else { "above" },
            crops.join(","),
            if notes.is_empty() { String::new() } else { format!("  (! {})", notes.join("; ")) }
        );
    }

    // plant raws: what can be farmed/brewed here
    let farmable_u: Vec<&str> = w.plant_raws.iter().filter(|p| p.has_seed && !p.is_tree && p.underground).map(|p| p.token.as_str()).collect();
    let brew: usize = w.plant_raws.iter().filter(|p| p.brewable).count();
    println!(
        "\nplant raws: {} ({} trees, {} brewable, {} with seeds); underground crops: {}",
        w.plant_raws.len(),
        w.plant_raws.iter().filter(|p| p.is_tree).count(),
        brew,
        w.plant_raws.iter().filter(|p| p.has_seed).count(),
        farmable_u.join(", ")
    );

    // wells
    let wells: Vec<_> = w.buildings.iter().filter(|b| b.kind == "Well").collect();
    println!("\nwells ({}):", wells.len());
    for b in &wells {
        // DF draws from the first liquid below the well (estimate: scan straight down)
        let mut water = String::from("no water found below");
        let mut p = b.center;
        while p.z > 0 {
            p.z -= 1;
            match w.tile(p) {
                Some(t) if t.liquid_depth() > 0 => {
                    water = format!(
                        "{} depth {} at z{} ({} levels down)",
                        if t.is_magma() { "magma" } else { "water" },
                        t.liquid_depth(),
                        p.z,
                        b.z - p.z
                    );
                    break;
                }
                Some(_) if !w.shape(p).is_open() => {
                    water = format!("blocked at z{}", p.z);
                    break;
                }
                None => break,
                _ => {}
            }
        }
        println!("  {:>6} z{:<3} ({:>3},{:>3}) built={} {}", b.id, b.z, b.center.x, b.center.y, b.built, water);
    }

    // drinks and food
    let mut drinks = (0i64, 0i64);
    let mut food: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    for i in &w.items {
        let avail = !i.flags.forbid && !i.flags.rotten && !i.flags.dump && !i.flags.construction;
        let n = i.stack.max(1) as i64;
        let edible_plant = |i: &rf_core::Item| {
            let tok = w.material(i.mat).map(|m| m.token.as_str()).unwrap_or("");
            let id = tok.strip_prefix("PLANT:").and_then(|r| r.split(':').next()).unwrap_or("");
            raw(id).map(|p| p.edible_raw).unwrap_or(false)
        };
        let key = match i.kind.as_str() {
            "DRINK" => {
                drinks.0 += n;
                if avail {
                    drinks.1 += n;
                }
                continue;
            }
            "FOOD" => "FOOD (prepared meals)",
            "MEAT" | "FISH" | "CHEESE" | "EGG" => "MEAT/FISH/CHEESE/EGG",
            "PLANT" if edible_plant(i) => "PLANT (edible raw)",
            "PLANT" => "PLANT (not edible raw)",
            "PLANT_GROWTH" => "PLANT_GROWTH (fruit, leaves)",
            "FISH_RAW" => "FISH_RAW (needs cleaning)",
            "SEEDS" => "SEEDS",
            _ => continue,
        };
        let e = food.entry(key).or_default();
        e.0 += n;
        if avail {
            e.1 += n;
        }
    }
    let cits = w.citizens().count().max(1) as i64;
    println!("\ndrink units: {} ({} available = not forbidden/rotten/dumped; {:.1} per citizen)", drinks.0, drinks.1, drinks.1 as f64 / cits as f64);
    println!("food units (total / available):");
    for (k, (t, a)) in &food {
        println!("  {k:<30} {t:>6} / {a:<6}");
    }

    // burrows, squads
    if !extras.burrows.is_empty() {
        let b: Vec<String> = extras.burrows.iter().map(|b| format!("{} ({} tiles, {} units)", b.name, b.tiles, b.units.len())).collect();
        println!("\nburrows ({}): {}", extras.burrows.len(), b.join(", "));
    }
    if !extras.squads.is_empty() {
        let s: Vec<String> = extras
            .squads
            .iter()
            .map(|s| {
                let alive = s.members.iter().filter(|id| w.unit(**id).map(|u| !u.flags.dead).unwrap_or(false)).count();
                format!("{} ({}/{} members on record, {alive} alive here)", s.name, s.members.len(), s.positions)
            })
            .collect();
        println!("squads ({}): {}", extras.squads.len(), s.join(", "));
    }

    // gait speeds
    let mut sp: BTreeMap<i32, usize> = BTreeMap::new();
    for u in &w.units {
        *sp.entry(u.walk_speed).or_default() += 1;
    }
    let s: Vec<String> = top(&sp, 10).into_iter().map(|(k, c)| format!("{k}: {c}")).collect();
    println!("walk_speed (units): {}", s.join(", "));
}

/// Round-3 data: needs and stress, thoughts, relationships, wounds, squads, burrows, creature,
/// weapon and armor raws, material mechanics, corpses, heavy aquifer.
fn round3(w: &World, dir: &std::path::Path, rep: &rf_import::ImportReport) {
    let manifest = rf_import::load_manifest(dir).unwrap_or_default();
    let cits: Vec<_> = w.citizens().collect();
    if cits.is_empty() {
        return;
    }
    // stress
    let n = cits.len() as f64;
    let avg = cits.iter().map(|u| u.stress as f64).sum::<f64>() / n;
    const CAT: [&str; 7] = ["0 haggard", "1 stressed", "2 unhappy", "3 fine", "4 content", "5 happy", "6 ecstatic"];
    let mut cat: BTreeMap<usize, usize> = BTreeMap::new();
    for u in &cits {
        if let Some(c) = manifest.stress_category(u.stress) {
            *cat.entry(c).or_default() += 1;
        }
    }
    let cats: Vec<String> = cat.iter().map(|(c, k)| format!("{} {k}", CAT.get(*c).copied().unwrap_or("?"))).collect();
    println!("\nstress (citizens): avg {avg:.0}, min {}, max {}; DF categories (cutoffs {:?}): {}",
        cits.iter().map(|u| u.stress).min().unwrap_or(0),
        cits.iter().map(|u| u.stress).max().unwrap_or(0),
        manifest.stress_cutoffs,
        if cats.is_empty() { "n/a (export before round 3)".into() } else { cats.join(", ") });
    // needs: how many citizens have each need and how many of them are unmet (focus < 0)
    let mut needs: BTreeMap<String, (usize, usize, i64)> = BTreeMap::new();
    for u in &cits {
        for nd in &u.needs {
            let e = needs.entry(nd.id.clone()).or_default();
            e.0 += 1;
            if nd.focus_level < 0 {
                e.1 += 1;
            }
            e.2 += nd.need_level as i64;
        }
    }
    let mut nv: Vec<_> = needs.into_iter().collect();
    nv.sort_by(|a, b| (b.1).1.cmp(&(a.1).1).then(a.0.cmp(&b.0)));
    println!("needs (citizens having it / unmet = focus < 0 / avg importance):");
    for (k, (have, unmet, lvl)) in nv.iter().take(14) {
        println!("  {k:<22} {have:>3} have, {unmet:>3} unmet, importance {:.1}", *lvl as f64 / (*have).max(1) as f64);
    }
    // thoughts
    let mut th: BTreeMap<String, usize> = BTreeMap::new();
    let mut recent: BTreeMap<String, usize> = BTreeMap::new();
    let mut n_emo = 0usize;
    for u in &cits {
        for e in &u.emotions {
            n_emo += 1;
            let k = format!("{} {}", e.emotion, e.thought);
            *th.entry(k.clone()).or_default() += 1;
            if e.year == w.calendar.year {
                *recent.entry(k).or_default() += 1;
            }
        }
    }
    println!("thoughts: {n_emo} remembered emotions; most common (all / this year):");
    for (k, c) in top(&th, 15) {
        println!("  {c:>5} / {:<5} {k}", recent.get(&k).copied().unwrap_or(0));
    }
    let vals: usize = cits.iter().map(|u| u.values.len()).sum();
    println!("values: {vals} ({:.1} per citizen)", vals as f64 / n);
    // relationships
    let mut rk: BTreeMap<String, usize> = BTreeMap::new();
    for u in &cits {
        for r in &u.relationships {
            *rk.entry(r.kind.clone()).or_default() += 1;
        }
    }
    let rs: Vec<String> = top(&rk, 16).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("relationships (citizens): {}", rs.join(", "));
    // wounds
    let wounded: Vec<_> = w.units.iter().filter(|u| !u.wounds.is_empty()).collect();
    let mut wp: BTreeMap<String, usize> = BTreeMap::new();
    let mut wf: BTreeMap<String, usize> = BTreeMap::new();
    for u in &wounded {
        for wd in &u.wounds {
            *wp.entry(if wd.body_part.is_empty() { "(systemic)".into() } else { wd.body_part.clone() }).or_default() += 1;
            for f in &wd.flags {
                *wf.entry(f.clone()).or_default() += 1;
            }
        }
    }
    let bleeding = w.units.iter().filter(|u| u.wounds.iter().any(|wd| wd.bleeding > 0)).count();
    let low_blood = w.units.iter().filter(|u| u.blood_max > 0 && u.blood < u.blood_max).count();
    println!(
        "wounds: {} units with {} wound layers ({} citizens), {bleeding} bleeding, {low_blood} below full blood",
        wounded.len(),
        wounded.iter().map(|u| u.wounds.len()).sum::<usize>(),
        wounded.iter().filter(|u| u.flags.citizen).count()
    );
    let s: Vec<String> = top(&wp, 10).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("  parts: {}", s.join(", "));
    let s: Vec<String> = top(&wf, 10).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("  flags: {}", s.join(", "));
    // hostiles
    let hostile: Vec<_> = w.units.iter().filter(|u| u.flags.hostile && !u.flags.dead).collect();
    let mut hr: BTreeMap<String, usize> = BTreeMap::new();
    for u in &hostile {
        *hr.entry(if u.invader_civ >= 0 { format!("{} (civ {})", u.race, u.invader_civ) } else { u.race.clone() }).or_default() += 1;
    }
    let s: Vec<String> = top(&hr, 10).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("hostile units ({}): {}", hostile.len(), if s.is_empty() { "none".into() } else { s.join(", ") });

    // squads
    println!("\nsquads ({}):", w.squads.len());
    for s in &w.squads {
        let label = if s.alias.is_empty() { s.name.clone() } else { s.alias.clone() };
        let filled = s.members.iter().filter(|&&m| m >= 0).count();
        let orders = if s.orders.is_empty() { "-".into() } else { s.orders.join("; ") };
        println!("  {:>4} {:<14} {:>2}/{:<2} schedule {:<18} orders {orders}", s.id, label, filled, s.members.len(), s.schedule);
        let names: Vec<String> = s
            .members
            .iter()
            .enumerate()
            .filter(|(_, &m)| m >= 0)
            .map(|(i, &m)| format!("{i}:{}", w.unit(m).map(|u| u.name.split(' ').next().unwrap_or("").to_string()).unwrap_or_else(|| format!("#{m}"))))
            .collect();
        println!("       members {}", names.join(" "));
        println!("       uniform {}", s.uniform.join(", "));
    }
    // burrows
    println!("burrows ({}):", w.burrows.len());
    for b in &w.burrows {
        let (zmin, zmax) = b.tiles.iter().fold((i16::MAX, i16::MIN), |(a, c), p| (a.min(p.z), c.max(p.z)));
        let zr = if b.tiles.is_empty() { "-".into() } else { format!("z {zmin}..{zmax}") };
        println!("  {:>3} {:<20} {:>6} tiles {zr:<12} {} units", b.id, b.name, b.tiles.len(), b.units.len());
    }

    // raws
    let castes: usize = w.creature_raws.iter().map(|c| c.castes.len()).sum();
    println!("\ncreature raws: {} creatures, {castes} castes (by body part count, first caste):", rep.creature_raws);
    let mut cl: Vec<_> = w.creature_raws.iter().filter(|c| !c.castes.is_empty()).collect();
    cl.sort_by_key(|c| std::cmp::Reverse(c.castes[0].body_parts.len()));
    let line: Vec<String> = cl
        .iter()
        .map(|c| {
            let k = &c.castes[0];
            format!("{} {}p/{}l/{}a", c.token, k.body_parts.len(), k.body_parts.iter().map(|p| p.layers.len()).sum::<usize>(), k.attacks.len())
        })
        .collect();
    for chunk in line.chunks(6) {
        println!("  {}", chunk.join(", "));
    }
    if let Some(d) = w.creature_raws.iter().find(|c| c.token == "DWARF") {
        let k = &d.castes[0];
        println!(
            "dwarf ({}): body size {} cm3, walk {}, {} parts, attacks: {}",
            k.token,
            k.body_size,
            k.walk_speed,
            k.body_parts.len(),
            k.attacks.iter().map(|a| format!("{} ({})", a.name, a.body_part)).collect::<Vec<_>>().join(", ")
        );
        let parts: Vec<String> = k
            .body_parts
            .iter()
            .map(|p| {
                let parent = if p.parent >= 0 { k.body_parts.get(p.parent as usize).map(|x| x.token.as_str()).unwrap_or("?") } else { "-" };
                let layers: Vec<&str> = p.layers.iter().map(|l| l.tissue.as_str()).collect();
                format!("{}<{} [{}]", p.token, parent, layers.join("/"))
            })
            .collect();
        for chunk in parts.chunks(6) {
            println!("  {}", chunk.join("  "));
        }
    }
    let mut wk: BTreeMap<String, usize> = BTreeMap::new();
    for x in &w.weapon_raws {
        *wk.entry(x.skill.clone()).or_default() += 1;
    }
    let s: Vec<String> = wk.iter().map(|(k, c)| format!("{} {c}", if k.is_empty() { "-" } else { k })).collect();
    println!("weapon raws ({}) by skill: {}", w.weapon_raws.len(), s.join(", "));
    let mut ak: BTreeMap<String, usize> = BTreeMap::new();
    for x in &w.armor_raws {
        *ak.entry(x.kind.clone()).or_default() += 1;
    }
    let s: Vec<String> = ak.iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("armor raws ({}): {}", w.armor_raws.len(), s.join(", "));
    let mech = w.materials.iter().filter(|m| m.density > 0).count();
    let show: Vec<String> = ["INORGANIC:IRON", "INORGANIC:STEEL", "INORGANIC:COPPER", "CREATURE:DWARF:BONE"]
        .iter()
        .filter_map(|t| w.materials.iter().find(|m| m.token == *t))
        .map(|m| format!("{} dens {} shear {}/{} impact {}/{} edge {}", m.token, m.density, m.shear_yield, m.shear_fracture, m.impact_yield, m.impact_fracture, m.max_edge))
        .collect();
    println!("materials with mechanics: {mech}/{}", w.materials.len());
    for s in show {
        println!("  {s}");
    }
    // corpses, heavy aquifer
    let mut co: BTreeMap<String, usize> = BTreeMap::new();
    for i in w.items.iter().filter(|i| i.kind == "CORPSE" || i.kind == "CORPSEPIECE" || i.kind == "REMAINS") {
        *co.entry(format!("{}:{}", i.kind, i.subtype)).or_default() += 1;
    }
    let s: Vec<String> = top(&co, 8).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("corpses/remains: {}", s.join(", "));
    let heavy = w.tiles.iter().filter(|t| t.flags & rf_import::TILE_FLAG_HEAVY_AQUIFER != 0).count();
    let aq = w.tiles.iter().filter(|t| t.flags & rf_core::tile_flags::AQUIFER != 0).count();
    println!("aquifer tiles: {aq}, heavy: {heavy}");
}

fn main() {
    let mut dir: Option<PathBuf> = None;
    let mut zsel: Option<i16> = None;
    let mut all_cit = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--z" => zsel = args.next().and_then(|s| s.parse().ok()),
            "--all-citizens" => all_cit = true,
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let Some(dir) = rf_import::find_export(dir.as_deref()) else {
        eprintln!("no export found (pass a folder, set RUSTFORT_EXPORT, or create {})", rf_import::DEFAULT_EXPORT);
        std::process::exit(1);
    };
    let (w, rep) = match rf_import::load_export(&dir) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("import failed: {e}");
            std::process::exit(1);
        }
    };
    println!("export   {}", dir.display());
    println!(
        "fort     {} ({}), world {}, site {}, DF {} / DFHack {}",
        w.meta.fort_name, w.meta.fort_name_native, w.meta.world_name, w.meta.site_id, w.meta.df_version, w.meta.dfhack_version
    );
    println!(
        "date     {} {} year {} (tick {}), {}",
        w.calendar.day(),
        w.calendar.month_name(),
        w.calendar.year,
        w.calendar.year_tick,
        w.calendar.season_name()
    );
    println!("dims     {} x {} x {} = {} tiles", w.dims.x, w.dims.y, w.dims.z, w.tiles.len());
    println!(
        "counts   units {} (citizens {}), items {}, buildings {}, plants {}, materials {}, tiletypes {}",
        rep.units,
        rep.citizens,
        rep.items,
        rep.buildings,
        rep.plants,
        w.materials.len(),
        w.tiletypes.infos.len()
    );
    println!("loaded   in {:.3} s", rep.seconds);
    println!("warnings ({}):", rep.warnings.len());
    for wr in &rep.warnings {
        println!("  - {wr}");
    }

    // citizens
    let cits: Vec<_> = w.units.iter().filter(|u| u.flags.citizen && !u.flags.dead).collect();
    println!("\ncitizens ({}):", cits.len());
    let shown = if all_cit { cits.len() } else { cits.len().min(15) };
    for u in &cits[..shown] {
        let prof = if u.custom_profession.is_empty() { u.profession.clone() } else { u.custom_profession.clone() };
        println!("  {:>6}  {:<32} {:<18} ({:>3},{:>3},{:>3})", u.id, u.name, prof, u.pos.x, u.pos.y, u.pos.z);
    }
    if shown < cits.len() {
        println!("  ... {} more (--all-citizens)", cits.len() - shown);
    }
    let mut races: BTreeMap<String, usize> = BTreeMap::new();
    for u in &w.units {
        *races.entry(u.race.clone()).or_default() += 1;
    }
    let races: Vec<String> = top(&races, 12).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("units by race: {}", races.join(", "));

    // items / buildings
    let mut ik: BTreeMap<String, usize> = BTreeMap::new();
    for i in &w.items {
        *ik.entry(i.kind.clone()).or_default() += 1;
    }
    println!("\nitem kinds ({} kinds):", ik.len());
    for (k, c) in top(&ik, 20) {
        println!("  {c:>6}  {k}");
    }
    let mut bk: BTreeMap<String, usize> = BTreeMap::new();
    for b in &w.buildings {
        let k = if b.subtype.is_empty() { b.kind.clone() } else { format!("{}/{}", b.kind, b.subtype) };
        *bk.entry(k).or_default() += 1;
    }
    println!("\nbuilding kinds ({} kinds):", bk.len());
    for (k, c) in top(&bk, 25) {
        println!("  {c:>6}  {k}");
    }

    production(&w, &dir);
    round3(&w, &dir, &rep);

    // tiles per z-range
    println!("\ntiles per z-range (shape %, material class %, top materials; hidden = undiscovered):");
    let plane = w.dims.x as usize * w.dims.y as usize;
    let step = 15usize;
    let mut z0 = 0usize;
    while z0 < w.dims.z as usize {
        let z1 = (z0 + step).min(w.dims.z as usize);
        let (mut shapes, mut classes, mut mats): (BTreeMap<String, usize>, BTreeMap<String, usize>, BTreeMap<String, usize>) =
            Default::default();
        let mut hidden = 0usize;
        for t in &w.tiles[z0 * plane..z1 * plane] {
            if t.hidden() {
                hidden += 1;
            }
            let info = w.tiletypes.get(t.tt);
            *shapes.entry(format!("{:?}", info.shape)).or_default() += 1;
            *classes.entry(format!("{:?}", info.material)).or_default() += 1;
            if t.mat != NO_MAT {
                let n = w.material(t.mat).map(|m| m.name.clone()).unwrap_or_else(|| "?".into());
                *mats.entry(n).or_default() += 1;
            }
        }
        let n = ((z1 - z0) * plane).max(1) as f64;
        let fmt = |v: Vec<(String, usize)>| {
            v.into_iter().map(|(k, c)| format!("{k} {:.0}%", c as f64 * 100.0 / n)).collect::<Vec<_>>().join(", ")
        };
        println!("  z{:>3}-{:<3} hidden {:>3.0}%", z0, z1 - 1, hidden as f64 * 100.0 / n);
        println!("      shape: {}", fmt(top(&shapes, 5)));
        println!("      class: {}", fmt(top(&classes, 5)));
        println!("      mats:  {}", fmt(top(&mats, 6)));
        z0 = z1;
    }

    // tiles without material, by tiletype material class (open air etc. excluded)
    let mut nomat: BTreeMap<String, usize> = BTreeMap::new();
    for t in &w.tiles {
        let info = w.tiletypes.get(t.tt);
        if t.mat == NO_MAT && !info.shape.is_open() && info.material != TtMaterial::Air {
            *nomat.entry(format!("{:?}/{:?}", info.material, info.shape)).or_default() += 1;
        }
    }
    let nm: Vec<String> = top(&nomat, 12).into_iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("\nnon-open tiles without material: {}", if nm.is_empty() { "none".into() } else { nm.join(", ") });

    // ASCII map
    let mut per_z: BTreeMap<i16, usize> = BTreeMap::new();
    for u in &cits {
        if w.in_bounds(u.pos) {
            *per_z.entry(u.pos.z).or_default() += 1;
        }
    }
    let z = zsel.unwrap_or_else(|| top(&per_z, 1).first().map(|x| x.0).unwrap_or(w.dims.z as i16 / 2));
    let here: Vec<_> = cits.iter().filter(|u| u.pos.z == z).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (i16::MAX, i16::MAX, i16::MIN, i16::MIN);
    for u in &here {
        x0 = x0.min(u.pos.x);
        y0 = y0.min(u.pos.y);
        x1 = x1.max(u.pos.x);
        y1 = y1.max(u.pos.y);
    }
    if here.is_empty() {
        (x0, y0, x1, y1) = (0, 0, w.dims.x as i16 - 1, w.dims.y as i16 - 1);
    }
    let (mw, mh) = (150i16, 70i16);
    let cx = (x0 + x1) / 2;
    let cy = (y0 + y1) / 2;
    let x0 = (cx - mw / 2).clamp(0, (w.dims.x as i16 - mw).max(0));
    let y0 = (cy - mh / 2).clamp(0, (w.dims.y as i16 - mh).max(0));
    let x1 = (x0 + mw - 1).min(w.dims.x as i16 - 1);
    let y1 = (y0 + mh - 1).min(w.dims.y as i16 - 1);
    let width = (x1 - x0 + 1) as usize;
    let mut grid: Vec<Vec<char>> = (y0..=y1).map(|y| (x0..=x1).map(|x| shape_char(&w, Pos::new(x, y, z))).collect()).collect();
    for b in &w.buildings {
        if b.z != z || b.kind == "Construction" || b.kind == "Civzone" || b.kind == "Stockpile" {
            continue;
        }
        if let Some(ch) = building_char(&b.kind) {
            for y in b.y1.max(y0)..=b.y2.min(y1) {
                for x in b.x1.max(x0)..=b.x2.min(x1) {
                    if b.contains(Pos::new(x, y, z)) {
                        grid[(y - y0) as usize][(x - x0) as usize] = ch;
                    }
                }
            }
        }
    }
    for u in &w.units {
        if u.pos.z == z && u.pos.x >= x0 && u.pos.x <= x1 && u.pos.y >= y0 && u.pos.y <= y1 && !u.flags.dead {
            let ch = if u.flags.citizen { '@' } else if u.flags.hostile { '!' } else if u.flags.animal { 'a' } else { 'u' };
            grid[(u.pos.y - y0) as usize][(u.pos.x - x0) as usize] = ch;
        }
    }
    println!(
        "\nz {z}: {} citizens here; x {x0}..{x1}, y {y0}..{y1}  (@ citizen, a animal, ! hostile, # wall, . floor, , grass, X/</> stairs, ^ ramp, T tree, W workshop, b bed, + door, ~ water)",
        here.len()
    );
    println!("+{}+", "-".repeat(width));
    for row in grid {
        println!("|{}|", row.into_iter().collect::<String>());
    }
    println!("+{}+", "-".repeat(width));
}
