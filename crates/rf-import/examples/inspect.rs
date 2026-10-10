//! Print a summary of a fort export:
//!   cargo run -p rf-import --release --example inspect -- [dir] [--z N] [--all-citizens]
//! dims, counts, citizens, item/building histograms, mods, work orders, workshop job queues,
//! farm plots with crops, wells, drink/food stock, burrows/squads, walk speeds, tile histograms
//! per z-range and an ASCII map of the z-level with the most citizens (or `--z N`).
//! Round 3: stress categories, needs, most common thoughts, relationships, wounds, hostiles,
//! squads (members, orders, uniform), burrows, creature raws with body part counts, the dwarf
//! body part list, weapon/armor raws, material mechanics, corpses and aquifer tiles.

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

/// Round 4: doors, levers and their links, bridges, traps, civilizations, nobles, settings, caravans.
fn round4(w: &World, dir: &std::path::Path, rep: &rf_import::ImportReport) {
    let has = |b: &rf_core::Building, f: &str| b.flags.iter().any(|x| x == f);
    let pos = |b: &rf_core::Building| format!("({},{},{})", b.x1, b.y1, b.z);
    let label = |id: i32| match w.building(id) {
        Some(b) => format!("{} {}{} {}", b.id, b.kind, if b.subtype.is_empty() { String::new() } else { format!("/{}", b.subtype) }, pos(b)),
        None => format!("{id} (missing)"),
    };
    // doors / hatches by flag set
    let mut doors: BTreeMap<String, usize> = BTreeMap::new();
    for b in w.buildings.iter().filter(|b| b.kind == "Door" || b.kind == "Hatch") {
        *doors.entry(format!("{} [{}]", b.kind, b.flags.join(","))).or_default() += 1;
    }
    println!("\ndoors and hatches (by state flags; LOCKED = DF door_flags.forbidden):");
    for (k, c) in &doors {
        println!("  {c:>4}  {k}");
    }
    let locked: Vec<String> = w.buildings.iter().filter(|b| has(b, "LOCKED")).map(|b| format!("{} {}", b.id, pos(b))).collect();
    if !locked.is_empty() {
        println!("  locked: {}", locked.join(", "));
    }
    println!("\nlevers and pressure plates ({} mechanism links):", rep.building_links);
    for b in w.buildings.iter().filter(|b| rf_import::is_trigger(b)) {
        let state = if b.subtype == "Lever" { if has(b, "ACTIVE") { "pulled" } else { "not pulled" } } else { "plate" };
        let targets: Vec<String> = b.links.iter().map(|&l| label(l)).collect();
        println!("  {} {} {state} [{}] -> {}", label(b.id), b.direction, b.flags.join(","), if targets.is_empty() { "nothing".into() } else { targets.join("; ") });
    }
    println!("\nbridges:");
    for b in w.buildings.iter().filter(|b| b.kind == "Bridge") {
        let levers: Vec<String> = b.links.iter().map(|l| l.to_string()).collect();
        println!("  {} {}x{} raises {} [{}] levers [{}]", label(b.id), b.x2 - b.x1 + 1, b.y2 - b.y1 + 1, b.direction, b.flags.join(","), levers.join(","));
    }
    for k in ["Floodgate", "GrateWall", "GrateFloor", "BarsVertical", "BarsFloor", "Weapon", "Support"] {
        let list: Vec<String> = w.buildings.iter().filter(|b| b.kind == k).map(|b| format!("{} [{}] links {:?}", b.id, b.flags.join(","), b.links)).collect();
        if !list.is_empty() {
            println!("  {k}: {}", list.join("; "));
        }
    }
    // traps per type and state, with what is loaded in them
    let mut traps: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    for b in w.buildings.iter().filter(|b| b.kind == "Trap") {
        let e = traps.entry(b.subtype.clone()).or_default();
        e.0 += 1;
        if has(b, "LOADED") {
            e.1 += 1;
        }
        if has(b, "ACTIVE") {
            e.2 += 1;
        }
        e.3 += b.items.iter().filter(|&&i| w.item(i).is_some_and(|it| it.kind == "WEAPON" || it.kind == "TRAPCOMP")).count();
    }
    println!("\ntraps (type: total, loaded, armed/active, weapons+trap components inside):");
    for (k, (n, l, a, wpn)) in &traps {
        println!("  {k:<14} {n:>3}  loaded {l:>3}  active {a:>3}  weapons {wpn}");
    }
    let mut by_z: BTreeMap<i16, usize> = BTreeMap::new();
    for b in w.buildings.iter().filter(|b| b.kind == "Trap" && b.subtype != "Lever") {
        *by_z.entry(b.z).or_default() += 1;
    }
    println!("  per z: {}", by_z.iter().map(|(z, c)| format!("z{z} {c}")).collect::<Vec<_>>().join(", "));
    // civilizations
    println!("\nentities ({}): relation to the fort's civ, sends caravans", w.entities.len());
    for e in w.entities.iter().filter(|e| e.relation != "Unknown" && (e.kind == "Civilization" || e.relation == "Own" || e.trades)) {
        println!("  {:>4} {:<15} {:<8} {:<7} {}{}", e.id, e.kind, e.race, e.relation, e.name, if e.trades { "  [trades]" } else { "" });
    }
    let n_unknown = w.entities.iter().filter(|e| e.relation == "Unknown").count();
    let n_sitegov = w.entities.iter().filter(|e| e.kind == "SiteGovernment" && e.relation != "Own").count();
    println!("  (+ {n_sitegov} other site governments, {n_unknown} civilizations without contact)");
    // nobles
    let hf_name = |uid: i32| w.unit(uid).map(|u| u.name.clone()).unwrap_or_default();
    println!("\nnobles ({} positions):", w.nobles.len());
    for n in &w.nobles {
        if n.entity_id != w.meta.group_id && n.holder < 0 {
            continue;
        }
        let who = if n.holder >= 0 { format!("{} ({})", hf_name(n.holder), n.holder) } else { "vacant".into() };
        let m = if n.mandates.is_empty() { String::new() } else { format!("  mandates: {}", n.mandates.join("; ")) };
        println!("  {:<22} {:<22} {who}{m}", n.token, n.name);
    }
    let x = rf_import::load_extras(dir);
    for m in &x.mandates {
        println!("  mandate {:?} by {} {}: {}/{} left, {}/{} ticks", m.text, m.position, m.noble, m.amount_remaining, m.amount_total, m.timeout_counter, m.timeout_limit);
    }
    // settings
    let s = &w.settings;
    println!(
        "\nsettings ({}): population cap {}, strict cap {}, baby cap {}; invasions {}, temperature {}, weather {}, cave-ins {}",
        if rep.settings { "settings.json" } else { "DF defaults, no settings.json" },
        s.population_cap,
        s.strict_population_cap,
        s.baby_cap,
        s.invasions,
        s.temperature,
        s.weather,
        s.cave_ins
    );
    let pick: Vec<&String> = s
        .other
        .iter()
        .filter(|o| ["DIFFICULTY_", "ENEMY_", "INVASION_", "SIEGE_", "VISITOR_CAP", "FEATURE_ARTIFACTS"].iter().any(|p| o.starts_with(p)))
        .collect();
    if !pick.is_empty() {
        println!("  {}", pick.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("  "));
    }
    // trade
    let depots: Vec<String> = w.buildings.iter().filter(|b| b.kind == "TradeDepot").map(|b| format!("{} {} [{}]", b.id, pos(b), b.flags.join(","))).collect();
    println!("\ntrade: depots {}; {} caravans; {} diplomats", if depots.is_empty() { "none".into() } else { depots.join(", ") }, w.caravans.len(), x.diplomats.len());
    for c in &w.caravans {
        println!(
            "  {} ({}, {}) {} leaves in {} ticks, {} goods, {} animals, {} merchants, mood {}",
            c.civ, c.race, c.entity_id, c.state, c.leaves_in, c.goods.len(), c.animals.len(), c.merchants.len(), c.mood
        );
    }
    for d in &x.diplomats {
        println!("  diplomat of civ {}: hf {} unit {} ({})", d.civ_id, d.diplomat_hf, d.diplomat_unit, d.source);
    }
    // hist figs
    let deities = w.hist_figs.iter().filter(|h| w.units.iter().any(|u| u.relationships.iter().any(|r| r.kind == "Deity" && r.hist_fig_id == h.id))).count();
    println!(
        "\nhistorical figures: {} names ({} alive, {} on the map, {} worshipped deities)",
        w.hist_figs.len(),
        w.hist_figs.iter().filter(|h| h.alive).count(),
        w.hist_figs.iter().filter(|h| h.unit_id >= 0).count(),
        deities
    );
}

/// Round 8: work details, stockpile filters, kitchen, locations, justice, animals, hauling, rooms.
fn round8(w: &rf_core::World, dir: &std::path::Path, rep: &rf_import::ImportReport) {
    if !rep.round8 {
        println!("\nround 8: no work_details.json (export predates exporter rev 2026-10-10e)");
        return;
    }
    let x = rf_import::load_extras(dir);
    let name = |id: i32| w.units.iter().find(|u| u.id == id).map(|u| u.name.clone()).unwrap_or_else(|| format!("#{id}"));
    println!("\nwork details ({}):", w.work_details.len());
    for d in &w.work_details {
        println!("  {:<18} {:<22} {:>3} members  labors {}", d.name, d.mode, d.assigned.len(), d.labors.join(","));
    }
    let piles: Vec<&rf_core::Building> = w.buildings.iter().filter(|b| b.stockpile.is_some()).collect();
    let nfil: usize = piles.iter().map(|b| b.stockpile.as_ref().unwrap().filters.len()).sum();
    let linked = piles.iter().filter(|b| b.stockpile.as_ref().is_some_and(|s| !s.give_to.is_empty() || !s.take_from.is_empty())).count();
    let only = piles.iter().filter(|b| b.stockpile.as_ref().is_some_and(|s| s.use_links_only)).count();
    println!("\nstockpiles: {} with {} filter strings, {} linked, {} take from links only", piles.len(), nfil, linked, only);
    for b in piles.iter().take(6) {
        let s = b.stockpile.as_ref().unwrap();
        let partial = s.filters.iter().filter(|f| !f.ends_with("/*") && !f.contains("/options/")).count();
        println!(
            "  {:>6} {:<22} [{}] {} strings ({} single leaves), give {:?} take {:?}",
            b.id,
            b.name,
            s.categories.join(","),
            s.filters.len(),
            partial,
            s.give_to,
            s.take_from
        );
    }
    let cook = w.kitchen.iter().filter(|k| k.no_cook).count();
    let brew = w.kitchen.iter().filter(|k| k.no_brew).count();
    let mut kt: BTreeMap<String, usize> = BTreeMap::new();
    for k in &w.kitchen {
        *kt.entry(k.item_type.clone()).or_default() += 1;
    }
    println!("\nkitchen: {} entries ({} no cook, {} no brew): {}", w.kitchen.len(), cook, brew, top(&kt, 10).into_iter().map(|(k, c)| format!("{k} {c}")).collect::<Vec<_>>().join(", "));
    println!("\nlocations ({}):", w.locations.len());
    for l in &w.locations {
        let occ: Vec<String> = l.occupations.iter().map(|o| if o.unit >= 0 { format!("{} {}", o.kind, name(o.unit)) } else { format!("{} vacant", o.kind) }).collect();
        let deity = if l.deity >= 0 { w.hist_figs.iter().find(|h| h.id == l.deity).map(|h| h.name.clone()).unwrap_or_else(|| format!("hf {}", l.deity)) } else { String::new() };
        let room = x.rooms.iter().find(|r| l.zones.contains(&r.id) && r.room_value >= 0);
        println!(
            "  {:>3} {:<9} {:<28} zones {:?} {}{}{} {}",
            l.id,
            l.kind,
            l.name,
            l.zones,
            if deity.is_empty() { String::new() } else { format!("deity {deity} ") },
            if l.profession.is_empty() { String::new() } else { format!("{} ", l.profession) },
            room.map(|r| format!("value {} = {}", r.room_value, r.room_name)).unwrap_or_default(),
            occ.join(", ")
        );
    }
    let mut ck: BTreeMap<String, usize> = BTreeMap::new();
    for c in &w.crimes {
        *ck.entry(c.kind.clone()).or_default() += 1;
    }
    println!(
        "\njustice: {} crimes at the site ({} convicted): {}; {} punishments running",
        w.crimes.len(),
        w.crimes.iter().filter(|c| c.convicted >= 0).count(),
        top(&ck, 8).into_iter().map(|(k, c)| format!("{k} {c}")).collect::<Vec<_>>().join(", "),
        x.punishments.len()
    );
    for c in w.crimes.iter().rev().take(3) {
        println!("  #{} year {} {} by {} against {} -> {}", c.id, c.year, c.kind, name(c.criminal), name(c.victim), if c.punishment.is_empty() { "-" } else { &c.punishment });
    }
    let animals: Vec<&rf_core::Unit> = w.units.iter().filter(|u| u.flags.animal && u.flags.tame && !u.flags.dead).collect();
    let mut tr: BTreeMap<String, usize> = BTreeMap::new();
    for u in &animals {
        *tr.entry(u.training.clone()).or_default() += 1;
    }
    println!(
        "\nanimals: {} tame ({} pets with owner, {} pastured, {} for slaughter); training {}; pregnant units {}",
        animals.len(),
        animals.iter().filter(|u| u.pet_owner >= 0).count(),
        animals.iter().filter(|u| u.pasture >= 0).count(),
        animals.iter().filter(|u| u.slaughter).count(),
        top(&tr, 6).into_iter().map(|(k, c)| format!("{k} {c}")).collect::<Vec<_>>().join(", "),
        w.units.iter().filter(|u| u.pregnancy > 0).count()
    );
    let prods: Vec<String> = x
        .animal_products
        .iter()
        .filter(|p| animals.iter().any(|u| u.race == p.race && u.caste == p.caste))
        .map(|p| {
            let mut v = Vec::new();
            if p.milkable.is_some() {
                v.push("milk");
            }
            if p.grazer > 0 {
                v.push("grazer");
            }
            if !p.shearable.is_empty() {
                v.push("shear");
            }
            if !p.egg_materials.is_empty() {
                v.push("eggs");
            }
            format!("{}:{} {}", p.race, p.caste, v.join("+"))
        })
        .collect();
    println!("  products of the castes present: {}", if prods.is_empty() { "none".into() } else { prods.join(", ") });
    println!("\nhauling routes: {}", w.haul_routes.len());
    for r in &w.haul_routes {
        println!("  {} {}: {} stops, {} vehicles", r.id, r.name, r.stops.len(), r.vehicles.len());
    }
    // room evaluation: DF's verdict per personal room (owner thought severity) and per location
    let mut sev: BTreeMap<(String, i32), usize> = BTreeMap::new();
    for r in &x.rooms {
        if r.room_thought_severity >= 0 {
            *sev.entry((r.room_thought.clone(), r.room_thought_severity)).or_default() += 1;
        }
    }
    println!(
        "\nrooms: {} civzones evaluated, {} with DF value (locations), {} with a description; owner thoughts by severity: {}",
        x.rooms.len(),
        x.rooms.iter().filter(|r| r.room_value >= 0).count(),
        x.rooms.iter().filter(|r| !r.room_description.is_empty()).count(),
        sev.iter().map(|((t, s), c)| format!("{t}:{s} x{c}")).collect::<Vec<_>>().join(", ")
    );
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
    round4(&w, &dir, &rep);
    round8(&w, &dir, &rep);

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
