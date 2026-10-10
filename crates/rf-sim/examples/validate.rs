//! [validation] How close is Rustfort to real DF? Replays Gordon's own history.
//!
//!   cargo run -p rf-sim --release --example validate -- --from <export A> --to <export B>
//!       [--seeds 1,2,3,4,5] [--jobs 3] [--ticks N] [--no-invasions] [--no-standdown]
//!       [--gamelog <DF gamelog.txt> --gl-from <line> --gl-to <line>] [--md <out.md>]
//!       [--deaths <day:n,day:n,...>]
//!
//! [skills9] `--deaths` kills n citizens (soldiers first) on the given day of the run, to
//! replay the real fort's losses (e.g. `25:11,580:1,650:3` for r4 -> r5) when the sim's own
//! sieges are weaker than DF's; used to calibrate grief and fear stress.
//!
//! A and B are two real DF exports of the same fort (e.g. `tileshtulon-r4`, year 277, and
//! `tileshtulon`, year 279). The tool
//!
//! 1. measures the real change A -> B with one census function (population flow, item stocks
//!    and new items by kind, wealth, buildings, tile changes, stockpile fill, stress, skill
//!    experience of the dwarves present in both),
//! 2. runs Rustfort from A for exactly the in-game time between A and B (the calendar
//!    difference) once per seed, with the `bench --standdown` player emulation and invasions
//!    on, and applies the same census to the simulated end state,
//! 3. prints per metric the real value, the seeds' min / median / max and a verdict.
//!
//! Gordon and his scripts gave orders in DF between the snapshots (work orders, digging,
//! buildings). Rustfort only has what A contained, so compare rates and orders of magnitude,
//! not exact states. `--gamelog` adds real event counts (sieges, migrant waves, births, deaths,
//! caravans, moods) from DF's `gamelog.txt` between two line numbers (read only; see
//! docs/VALIDATION.md for how the lines were found).

#[path = "validate_support/gamelog.rs"]
mod gamelog;

use rf_core::{Item, Shape, TtMaterial, TtSpecial, World};
use rf_sim::{AnnouncementKind, Sim};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

const TICKS_PER_YEAR: f64 = rf_core::TICKS_PER_YEAR as f64;

type Metrics = BTreeMap<String, f64>;

// ------------------------------------------------------------------ census

/// Item kind -> comparison group.
fn group_of(it: &Item, w: &World) -> &'static str {
    match it.kind.as_str() {
        "DRINK" => "drink",
        "FOOD" => "meal",
        "PLANT" | "PLANT_GROWTH" => "plant",
        "SEEDS" => "seed",
        "MEAT" | "FISH" | "FISH_RAW" | "EGG" | "CHEESE" | "GLOB" | "LIQUID_MISC" | "POWDER_MISC" => "other_food",
        "BAR" => "bar",
        "BLOCKS" => "block",
        "BOULDER" => "boulder",
        "WOOD" => "log",
        "BED" | "CHAIR" | "TABLE" | "DOOR" | "CABINET" | "BOX" | "COFFIN" | "STATUE" | "ARMORSTAND" | "WEAPONRACK"
        | "SLAB" | "HATCH_COVER" | "FLOODGATE" | "GRATE" | "WINDOW" | "QUERN" | "MILLSTONE" | "TRACTION_BENCH"
        | "BOOKCASE" | "PEDESTAL" | "DISPLAY_CASE" | "ANVIL" => "furniture",
        "BARREL" | "BIN" | "BAG" | "BUCKET" | "CAGE" | "ANIMALTRAP" | "FLASK" | "BACKPACK" => "container",
        "AMULET" | "BRACELET" | "CROWN" | "EARRING" | "FIGURINE" | "RING" | "SCEPTER" | "GOBLET" | "TOY" | "TOTEM"
        | "INSTRUMENT" => "craft",
        "SHOES" | "GLOVES" | "PANTS" | "ARMOR" | "HELM" => {
            if w.material(it.mat).map(|m| m.is_metal).unwrap_or(false) {
                "armor_metal"
            } else {
                "clothing"
            }
        }
        "WEAPON" | "SHIELD" | "QUIVER" => "weapon",
        "AMMO" => "ammo",
        "CLOTH" | "THREAD" | "SKIN_TANNED" => "cloth_leather",
        "CORPSE" | "CORPSEPIECE" | "REMAINS" => "refuse",
        "ROUGH" | "SMALLGEM" | "GEM" => "gem",
        "TRAPPARTS" | "TRAPCOMP" | "CHAIN" => "mechanism",
        "TOOL" => "tool",
        "SPLINT" | "CRUTCH" => "medical",
        _ => "other",
    }
}

/// Skill -> group for the experience comparison.
fn skill_group(id: &str) -> &'static str {
    const MIL: &[&str] = &[
        "AXE", "SWORD", "DAGGER", "MACE", "HAMMER", "SPEAR", "CROSSBOW", "BOW", "PIKE", "WHIP", "BLOWGUN", "DODGING",
        "ARMOR", "SHIELD", "MELEE_COMBAT", "RANGED_COMBAT", "WRESTLING", "BITE", "GRASP_STRIKE", "STANCE_STRIKE",
        "DISCIPLINE", "SITUATIONAL_AWARENESS", "MILITARY_TACTICS", "THROW",
    ];
    const SOC: &[&str] = &[
        "CONVERSATION", "COMEDY", "FLATTERY", "LYING", "INTIMIDATION", "PERSUASION", "NEGOTIATION", "JUDGING_INTENT",
        "CONSOLE", "PACIFY", "SPEAKING", "LEADERSHIP", "TEACHING", "KNOWLEDGE_ACQUISITION", "CONCENTRATION", "POETRY",
        "DANCE", "MAKE_MUSIC", "SING_MUSIC", "PLAY_KEYBOARD_INSTRUMENT", "PLAY_STRINGED_INSTRUMENT",
        "PLAY_WIND_INSTRUMENT", "PLAY_PERCUSSION_INSTRUMENT", "WRITING", "PROSE", "CRITICAL_THINKING", "LOGIC",
        "MATHEMATICS", "ASTRONOMY", "CHEMISTRY", "GEOGRAPHY", "OPTICS_ENGINEER", "FLUID_ENGINEER", "RECORD_KEEPING",
        "ORGANIZATION", "APPRAISAL", "READING",
    ];
    if MIL.contains(&id) {
        "military"
    } else if SOC.contains(&id) {
        "social"
    } else {
        "work"
    }
}

/// DF total experience of a skill: level n -> n+1 needs 500 + 100 n (w:Skill).
fn total_xp(rating: i32, xp: i32) -> f64 {
    let n = rating.max(0) as f64;
    500.0 * n + 50.0 * n * (n - 1.0) + xp.max(0) as f64
}

fn stress_level(s: i32) -> usize {
    [-50_000, -25_000, -10_000, 10_000, 25_000, 50_000].iter().filter(|&&c| s >= c).count()
}

const STRESS_NAMES: [&str; 7] = ["ecstatic", "happy", "content", "fine", "stressed", "very_stressed", "breaking"];

fn alive_citizen(u: &rf_core::Unit) -> bool {
    u.flags.citizen && !u.flags.dead && !u.flags.animal
}

/// Everything we compare, measured on `b` relative to the start world `a`.
/// `value` gives an item's own value (without contents).
fn measure(a: &World, b: &World, years: f64, value: &dyn Fn(&Item) -> f64) -> Metrics {
    let mut m = Metrics::new();
    let put = |m: &mut Metrics, k: &str, v: f64| {
        *m.entry(k.to_string()).or_insert(0.0) += v;
    };

    // Population flow.
    let a_cit: HashMap<i32, &rf_core::Unit> = a.units.iter().filter(|u| alive_citizen(u)).map(|u| (u.id, u)).collect();
    let b_cit: HashMap<i32, &rf_core::Unit> = b.units.iter().filter(|u| alive_citizen(u)).map(|u| (u.id, u)).collect();
    put(&mut m, "pop.citizens", b_cit.len() as f64);
    put(&mut m, "pop.children", b_cit.values().filter(|u| u.age < 13.0).count() as f64);
    let (mut births, mut migrants) = (0, 0);
    for (id, u) in &b_cit {
        if !a_cit.contains_key(id) {
            if (u.age as f64) <= years + 0.05 {
                births += 1;
            } else {
                migrants += 1;
            }
        }
    }
    put(&mut m, "pop.migrants_arrived", migrants as f64);
    put(&mut m, "pop.births_alive", births as f64);
    put(&mut m, "pop.citizens_lost", a_cit.keys().filter(|id| !b_cit.contains_key(id)).count() as f64);
    put(&mut m, "pop.hostiles_on_map", b.units.iter().filter(|u| !u.flags.dead && (u.flags.hostile || u.invader_civ >= 0)).count() as f64);
    put(
        &mut m,
        "pop.animals",
        b.units.iter().filter(|u| !u.flags.dead && !u.flags.inactive && !u.flags.citizen && (u.flags.animal || u.flags.tame) && !u.flags.hostile).count() as f64,
    );

    // Stress of the citizens at the end.
    let mut lv = [0usize; 7];
    let mut sum = 0.0;
    for u in b_cit.values() {
        lv[stress_level(u.stress)] += 1;
        sum += u.stress as f64;
    }
    for (k, n) in STRESS_NAMES.iter().zip(lv) {
        put(&mut m, &format!("stress.{k}"), n as f64);
    }
    put(&mut m, "stress.mean", sum / b_cit.len().max(1) as f64);
    put(&mut m, "stress.stressed_or_worse", (lv[4] + lv[5] + lv[6]) as f64);

    // Skills of the dwarves present at both ends.
    let mut xp_by_skill: BTreeMap<String, f64> = BTreeMap::new();
    let mut lvl_by_group: BTreeMap<&'static str, f64> = BTreeMap::new();
    let mut persisting = 0usize;
    for (id, ub) in &b_cit {
        let Some(ua) = a_cit.get(id) else { continue };
        persisting += 1;
        for sb in &ub.skills {
            let sa = ua.skills.iter().find(|s| s.id == sb.id);
            let before = sa.map(|s| total_xp(s.rating, s.xp)).unwrap_or(0.0);
            let gain = (total_xp(sb.rating, sb.xp) - before).max(0.0);
            *xp_by_skill.entry(sb.id.clone()).or_default() += gain;
            let up = (sb.rating - sa.map(|s| s.rating).unwrap_or(0)).max(0) as f64;
            *lvl_by_group.entry(skill_group(&sb.id)).or_default() += up;
        }
    }
    put(&mut m, "xp.persisting_dwarves", persisting as f64);
    let per = |x: f64| x / persisting.max(1) as f64 / years.max(1e-9);
    let mut groups: BTreeMap<&'static str, f64> = BTreeMap::new();
    for (k, v) in &xp_by_skill {
        *groups.entry(skill_group(k)).or_default() += v;
        put(&mut m, &format!("xpskill.{k}"), per(*v));
    }
    for g in ["work", "military", "social"] {
        put(&mut m, &format!("xp.{g}_per_dwarf_year"), per(groups.get(g).copied().unwrap_or(0.0)));
        put(&mut m, &format!("xp.{g}_levels_gained"), lvl_by_group.get(g).copied().unwrap_or(0.0));
    }
    put(
        &mut m,
        "xp.skills_above_20",
        b_cit.values().flat_map(|u| u.skills.iter()).filter(|s| s.rating > 20).count() as f64,
    );

    // Items: stocks (stack sums) by group at the end, new items (id above A's highest).
    let max_a = a.items.iter().map(|i| i.id).max().unwrap_or(0);
    let mut wealth = 0.0;
    let mut wealth_new = 0.0;
    let mut zero_value_new = 0usize;
    for it in &b.items {
        let g = group_of(it, b);
        put(&mut m, &format!("stock.{g}"), it.stack.max(1) as f64);
        let v = value(it);
        wealth += v;
        if it.id > max_a {
            if !matches!(g, "drink" | "meal" | "plant" | "seed" | "other_food" | "refuse" | "boulder" | "log" | "gem" | "other") {
                put(&mut m, "quality.new_durable_items", 1.0);
                put(&mut m, "quality.new_masterwork", if it.quality == 5 { 1.0 } else { 0.0 });
                put(&mut m, "quality.new_exceptional_or_better", if it.quality >= 4 && it.quality <= 5 { 1.0 } else { 0.0 });
                put(&mut m, "quality.new_sum", it.quality.min(5) as f64);
            }
            if g == "craft" {
                put(&mut m, "quality.craft_new_masterwork", if it.quality == 5 { 1.0 } else { 0.0 });
                put(&mut m, "quality.craft_new_sum", it.quality.min(5) as f64);
            }
            put(&mut m, &format!("new.{g}"), it.stack.max(1) as f64);
            put(&mut m, &format!("newkind.{}", it.kind), it.stack.max(1) as f64);
            wealth_new += v;
            if it.value <= 0 {
                zero_value_new += 1;
            }
        }
    }
    put(&mut m, "stock.items_total", b.items.len() as f64);
    if let Some(n) = m.get("quality.new_durable_items").copied() {
        let q = m.get("quality.new_sum").copied().unwrap_or(0.0);
        m.insert("quality.new_mean".into(), q / n.max(1.0));
        m.remove("quality.new_sum");
    }
    if let Some(q) = m.remove("quality.craft_new_sum") {
        let n = m.get("new.craft").copied().unwrap_or(0.0);
        m.insert("quality.craft_new_mean".into(), q / n.max(1.0));
    }
    put(&mut m, "wealth.items", wealth);
    put(&mut m, "wealth.new_items", wealth_new);
    put(&mut m, "wealth.new_items_without_stored_value", zero_value_new as f64);

    // Buildings.
    for bl in &b.buildings {
        let k = match bl.kind.as_str() {
            "Workshop" | "Furnace" => "bld.workshops",
            "Stockpile" => "bld.stockpiles",
            "Civzone" => "bld.zones",
            "Bed" | "Chair" | "Table" | "Door" | "Cabinet" | "Box" | "Coffin" | "Statue" | "Armorstand" | "Weaponrack"
            | "Slab" | "Hatch" | "Floodgate" | "GrateWall" | "GrateFloor" | "Bookcase" | "DisplayFurniture"
            | "TractionBench" | "WindowGlass" | "WindowGem" => "bld.furniture",
            "Trap" => "bld.traps",
            _ => "bld.other",
        };
        put(&mut m, k, 1.0);
        put(&mut m, "bld.total", 1.0);
    }

    // Tile changes against the start map.
    if a.tiles.len() == b.tiles.len() {
        let (mut dug, mut built, mut removed, mut smoothed) = (0, 0, 0, 0);
        for (ta, tb) in a.tiles.iter().zip(&b.tiles) {
            if ta.tt == tb.tt {
                continue;
            }
            let (ia, ib) = (a.tiletypes.get(ta.tt), b.tiletypes.get(tb.tt));
            let natural = matches!(
                ia.material,
                TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::Soil | TtMaterial::LavaStone | TtMaterial::Feature | TtMaterial::FrozenLiquid
            );
            if natural && ia.shape == Shape::Wall && !ib.shape.is_solid() && ib.material != TtMaterial::Construction {
                dug += 1;
            }
            if ib.material == TtMaterial::Construction && ia.material != TtMaterial::Construction {
                built += 1;
            }
            if ia.material == TtMaterial::Construction && ib.material != TtMaterial::Construction {
                removed += 1;
            }
            if matches!(ib.special, TtSpecial::Smooth | TtSpecial::Track) && !matches!(ia.special, TtSpecial::Smooth | TtSpecial::Track) {
                smoothed += 1;
            }
        }
        put(&mut m, "tile.dug", dug as f64);
        put(&mut m, "tile.constructed", built as f64);
        put(&mut m, "tile.deconstructed", removed as f64);
        put(&mut m, "tile.smoothed", smoothed as f64);
    }
    if !a.engravings.is_empty() || b.engravings.is_empty() {
        // (Exports before round 5 have no engravings, so the change is unknown there.)
        put(&mut m, "tile.engravings_added", b.engravings.len() as f64 - a.engravings.len() as f64);
    }
    put(
        &mut m,
        "tile.trees_lost",
        a.plants.iter().filter(|p| p.tree && !p.dead).count() as f64 - b.plants.iter().filter(|p| p.tree && !p.dead).count() as f64,
    );

    // Stockpile fill: tiles of stockpiles holding at least one loose item (or container).
    let mut occupied: BTreeSet<usize> = BTreeSet::new();
    for it in &b.items {
        if it.flags.on_ground && it.container < 0 && it.holder < 0 && it.building < 0 && !it.flags.construction {
            if let Some(i) = b.idx(it.pos) {
                occupied.insert(i);
            }
        }
    }
    let (mut tiles, mut full) = (0usize, 0usize);
    for bl in b.buildings.iter().filter(|bl| bl.kind == "Stockpile") {
        for y in bl.y1..=bl.y2 {
            for x in bl.x1..=bl.x2 {
                let p = rf_core::Pos::new(x, y, bl.z);
                if !bl.contains(p) {
                    continue;
                }
                tiles += 1;
                if b.idx(p).map(|i| occupied.contains(&i)).unwrap_or(false) {
                    full += 1;
                }
            }
        }
    }
    put(&mut m, "pile.tiles", tiles as f64);
    put(&mut m, "pile.fill_pct", 100.0 * full as f64 / tiles.max(1) as f64);
    put(&mut m, "pile.loose_items_outside", {
        let pile_tiles: BTreeSet<usize> = b
            .buildings
            .iter()
            .filter(|bl| bl.kind == "Stockpile")
            .flat_map(|bl| {
                let mut v = Vec::new();
                for y in bl.y1..=bl.y2 {
                    for x in bl.x1..=bl.x2 {
                        let p = rf_core::Pos::new(x, y, bl.z);
                        if bl.contains(p) {
                            v.extend(b.idx(p));
                        }
                    }
                }
                v
            })
            .collect();
        b.items
            .iter()
            .filter(|it| it.flags.on_ground && it.container < 0 && it.holder < 0 && it.building < 0 && !it.flags.construction)
            .filter(|it| b.idx(it.pos).map(|i| !pile_tiles.contains(&i)).unwrap_or(true))
            .count() as f64
    });
    m
}

// ------------------------------------------------------------------ simulation

struct Opts {
    invasions: bool,
    standdown: bool,
    standing: bool,
    /// After a fight, go back to the squads' schedules of the start export (Gordon's routine,
    /// e.g. constant training) instead of `Off duty` for everyone.
    restore: bool,
    /// [skills9] Scripted deaths: (day of the run, citizens killed).
    deaths: Vec<(u64, usize)>,
}

struct RunOut {
    seed: u64,
    metrics: Metrics,
    secs: f64,
    hash: u64,
    deaths: BTreeMap<String, usize>,
}

/// Run Rustfort from `start` for `ticks` with the `bench --standdown` player emulation.
fn run_seed(start: &World, ticks: u64, seed: u64, opt: &Opts, years: f64) -> RunOut {
    let (invasions, standdown) = (opt.invasions, opt.standdown);
    let mut sim = Sim::new(start.clone(), seed);
    if opt.standing {
        // Gordon's food-watch scripts add brew/cook orders when stocks run low; the sim's
        // fallback standing orders are the closest equivalent (brew, cook, barrels, pots).
        sim.set_standing_orders(true);
    }
    if !invasions {
        sim.set_invasions(false);
    }
    let mut calm_since: Option<u64> = None;
    let routine: Vec<(i32, String, bool)> = start
        .squads
        .iter()
        .map(|q| (q.id, q.schedule.clone(), q.orders.iter().any(|o| o.eq_ignore_ascii_case("Train"))))
        .collect();
    let mut alerted = false;
    let mut deaths: BTreeMap<String, usize> = BTreeMap::new();
    let mut death_citizens = 0usize;
    let t0 = Instant::now();
    let mut done = 0u64;
    while done < ticks {
        let n = 100u64.min(ticks - done);
        sim.run_ticks(n as u32);
        done += n;
        // [skills9] scripted deaths (soldiers first, then the oldest citizens by slot order)
        for &(day, k) in &opt.deaths {
            let at = day * rf_core::TICKS_PER_DAY as u64;
            if at > done - n && at <= done {
                let in_squad: BTreeSet<i32> = sim.world.squads.iter().flat_map(|q| q.members.iter().copied()).collect();
                let mut ids: Vec<(bool, i32)> = sim.world.citizens().filter(|u| u.age >= 13.0).map(|u| (!in_squad.contains(&u.id), u.id)).collect();
                ids.sort();
                let step = (ids.len() / k.max(1)).max(1);
                let pick: Vec<i32> = if ids.iter().filter(|x| !x.0).count() >= k { ids.iter().take(k).map(|x| x.1).collect() } else { ids.iter().step_by(step).take(k).map(|x| x.1).collect() };
                for id in pick {
                    sim.kill_unit(id, "has been found dead");
                }
            }
        }
        if standdown {
            let cits: Vec<rf_core::Pos> = sim.world.citizens().map(|u| u.pos).collect();
            let threat = sim.hostiles().iter().any(|&id| {
                sim.unit_status(id).activity != "Lurk"
                    && sim.world.unit(id).map(|u| cits.iter().any(|&c| c.dist(u.pos) <= 30 && sim.reachable(u.pos, c))).unwrap_or(false)
            });
            let idle = if opt.restore { !alerted } else { sim.world.squads.iter().all(|q| q.schedule.eq_ignore_ascii_case("Off duty")) };
            if threat && idle {
                let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
                for id in ids {
                    sim.set_squad_schedule(id, "Ready");
                }
                alerted = true;
            }
            if threat {
                calm_since = None;
            } else {
                let since = *calm_since.get_or_insert(done);
                let on_duty = sim.world.squads.iter().any(|q| !q.schedule.eq_ignore_ascii_case("Off duty") || !q.orders.is_empty());
                if opt.restore {
                    if alerted && done - since >= rf_core::TICKS_PER_DAY as u64 {
                        for (id, sched, train) in &routine {
                            sim.order_squad(*id, rf_sim::SquadOrder::Cancel);
                            sim.set_squad_schedule(*id, sched);
                            if *train {
                                sim.order_squad(*id, rf_sim::SquadOrder::Train);
                            }
                        }
                        alerted = false;
                    }
                } else if on_duty && done - since >= rf_core::TICKS_PER_DAY as u64 {
                    let ids: Vec<i32> = sim.world.squads.iter().map(|q| q.id).collect();
                    for id in ids {
                        sim.order_squad(id, rf_sim::SquadOrder::Cancel);
                        sim.set_squad_schedule(id, "Off duty");
                    }
                }
            }
        }
        for a in sim.drain_announcements() {
            if a.kind == AnnouncementKind::Death {
                let cause = a.text.split(" has ").nth(1).unwrap_or(&a.text).trim_end_matches('.').to_string();
                *deaths.entry(cause).or_default() += 1;
                death_citizens += 1;
            }
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    // Own value of an item: DF's exported value, else the trade formula minus the contents.
    let world = &sim.world;
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    for it in &world.items {
        if it.container >= 0 {
            children.entry(it.container).or_default().push(it.id);
        }
    }
    let own = |it: &Item| -> f64 {
        if it.value > 0 {
            return it.value as f64;
        }
        let mut v = sim.item_trade_value(it.id);
        if let Some(c) = children.get(&it.id) {
            for &cid in c {
                v -= sim.item_trade_value(cid);
            }
        }
        v.max(0) as f64
    };
    let mut m = measure(start, world, years, &own);
    // Event counters from the sim's own reports.
    let pr = sim.population_report();
    let cs = sim.combat_stats();
    let tr = sim.trade_report();
    let sr = sim.social_report();
    let moods = sim.mood_records();
    m.insert("ev.sieges".into(), cs.sieges as f64);
    m.insert("ev.invaders".into(), cs.invaders_spawned as f64);
    m.insert("ev.ambushes".into(), cs.ambushes as f64); // [war9] ambush squads (DF: the spotted ones)
    m.insert("ev.mounts".into(), cs.mounts_spawned as f64); // [war9]
    m.insert("ev.buildings_wrecked".into(), cs.buildings_wrecked as f64); // [war9]
    m.insert("ev.migrant_waves".into(), pr.waves.len() as f64);
    m.insert("ev.migrants".into(), pr.waves.iter().map(|w| w.1).sum::<usize>() as f64);
    m.insert("ev.dwarf_births".into(), pr.births.len() as f64);
    m.insert("ev.caravans".into(), tr.caravans.len() as f64);
    m.insert("ev.death_announcements".into(), death_citizens as f64);
    m.insert(
        "ev.citizen_deaths".into(),
        world.units.iter().filter(|u| u.flags.citizen && u.flags.dead && !u.flags.animal).count() as f64,
    );
    m.insert("ev.moods".into(), moods.len() as f64);
    m.insert("ev.artifacts".into(), moods.iter().filter(|r| r.artifact_id >= 0).count() as f64);
    m.insert("ev.tantrums".into(), sr.tantrums as f64);
    m.insert("ev.depressions".into(), (sr.depressions + sr.oblivious) as f64);
    m.insert("ev.beasts".into(), sim.cavern_report().beasts.len() as f64);
    m.insert("ev.trades".into(), tr.trades.len() as f64);
    m.insert("perf.ticks_per_s".into(), ticks as f64 / secs.max(1e-9));
    for (label, n) in sim.production_report().jobs_done {
        *m.entry(format!("simjob.{label}")).or_insert(0.0) += n as f64;
    }
    for (kind, n) in sim.industry_report().made {
        *m.entry(format!("simmade.{kind}")).or_insert(0.0) += n as f64;
    }
    RunOut { seed, metrics: m, secs, hash: sim.state_hash(), deaths }
}

// ------------------------------------------------------------------ report

fn verdict(real: f64, sims: &[f64]) -> (&'static str, f64) {
    if real.is_nan() || sims.is_empty() {
        return ("-", f64::NAN);
    }
    let mut s = sims.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let (lo, hi, med) = (s[0], s[s.len() - 1], s[s.len() / 2]);
    let ratio = if real.abs() > 1e-9 { med / real } else { f64::NAN };
    let tol = (0.25 * real.abs()).max(2.0);
    if real >= lo - 1e-9 && real <= hi + 1e-9 {
        ("in range", ratio)
    } else if (med - real).abs() <= tol {
        ("close", ratio)
    } else if med < real {
        ("LOW", ratio)
    } else {
        ("HIGH", ratio)
    }
}

fn fmt(v: f64) -> String {
    if v.is_nan() {
        "-".into()
    } else if v.abs() >= 100_000.0 {
        format!("{:.0}k", v / 1000.0)
    } else if v.fract().abs() < 1e-9 {
        format!("{v:.0}")
    } else if v.abs() >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

fn load(dir: &Path) -> World {
    let (mut w, _) = rf_import::load_export(dir).unwrap_or_else(|e| panic!("load {}: {e:?}", dir.display()));
    if w.meta.source.is_empty() {
        w.meta.source = format!("export:{}", dir.display());
    }
    w
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let from = PathBuf::from(get("--from").expect("--from <export dir>"));
    let to = PathBuf::from(get("--to").expect("--to <export dir>"));
    let seeds: Vec<u64> = get("--seeds").map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_else(|| vec![1, 2, 3, 4, 5]);
    let jobs: usize = get("--jobs").and_then(|s| s.parse().ok()).unwrap_or(3).max(1);
    let invasions = !flag("--no-invasions");
    let standdown = !flag("--no-standdown");
    let deaths: Vec<(u64, usize)> = get("--deaths")
        .map(|s| s.split(',').filter_map(|p| p.split_once(':')).filter_map(|(d, n)| Some((d.trim().parse().ok()?, n.trim().parse().ok()?))).collect())
        .unwrap_or_default(); // [skills9]
    let opts = Opts { invasions, standdown, standing: flag("--standing-orders"), restore: flag("--restore-schedules"), deaths };

    let t0 = Instant::now();
    let mut a = load(&from);
    let b = load(&to);
    if !from.join("settings.json").exists() && to.join("settings.json").exists() {
        // Exports before round 4 have no d_init settings; the importer's defaults (no
        // population cap) would flood the fort with migrants. Gordon's caps did not change.
        println!("note: {} has no settings.json; using the caps of {}", from.display(), to.display());
        a.settings = b.settings.clone();
    }
    let dt_cal = (b.calendar.year - a.calendar.year) as i64 * rf_core::TICKS_PER_YEAR as i64 + (b.calendar.year_tick - a.calendar.year_tick) as i64;
    let ticks: u64 = get("--ticks").and_then(|s| s.parse().ok()).unwrap_or(dt_cal.max(0) as u64);
    let years = ticks as f64 / TICKS_PER_YEAR;
    println!(
        "A {} ({} {}), B {} ({} {}): {} ticks = {:.2} years; loaded in {:.1}s",
        from.display(),
        a.calendar.year,
        a.calendar.year_tick,
        to.display(),
        b.calendar.year,
        b.calendar.year_tick,
        ticks,
        years,
        t0.elapsed().as_secs_f64()
    );
    let mut real = measure(&a, &b, years, &|it: &Item| it.value.max(0) as f64);
    let start = measure(&a, &a, years, &|it: &Item| it.value.max(0) as f64);
    real.insert("start.dig_designations".into(), a.tiles.iter().filter(|t| t.desig != 0).count() as f64);
    real.insert("start.active_work_orders".into(), a.work_orders.iter().filter(|o| o.active).count() as f64);
    real.insert("start.queued_workshop_jobs".into(), a.buildings.iter().map(|b| b.jobs.len()).sum::<usize>() as f64);
    if let (Some(gl), Some(f), Some(t)) = (get("--gamelog"), get("--gl-from"), get("--gl-to")) {
        let ev = gamelog::scan(Path::new(&gl), f.parse().unwrap_or(0), t.parse().unwrap_or(0));
        println!("gamelog lines {f}..{t}: {} seasons crossed, {} loads", ev.seasons, ev.loads);
        for (k, v) in &ev.counts {
            real.insert(format!("ev.{k}"), *v as f64);
        }
        for d in &ev.citizen_deaths {
            println!("  real death: {d}");
        }
    }

    // Seeds in parallel (each sim is deterministic on its own).
    let mut outs: Vec<RunOut> = Vec::new();
    for chunk in seeds.chunks(jobs) {
        let res: Vec<RunOut> = std::thread::scope(|sc| {
            let (a, opts) = (&a, &opts);
            let hs: Vec<_> = chunk.iter().map(|&s| sc.spawn(move || run_seed(a, ticks, s, opts, years))).collect();
            hs.into_iter().map(|h| h.join().expect("sim thread")).collect()
        });
        for r in &res {
            println!(
                "seed {}: {:.1}s ({:.0} ticks/s, {} parallel), {} -> {} citizens, deaths {:?}, hash {:016x}",
                r.seed,
                r.secs,
                ticks as f64 / r.secs,
                chunk.len(),
                a.citizens().count(),
                r.metrics.get("pop.citizens").copied().unwrap_or(0.0),
                r.deaths,
                r.hash
            );
        }
        outs.extend(res);
    }

    // Table.
    let mut keys: BTreeSet<String> = real.keys().cloned().collect();
    for o in &outs {
        keys.extend(o.metrics.keys().cloned());
    }
    let order = [
        "start.", "pop.", "ev.", "stress.", "xp.", "stock.", "new.", "quality.", "wealth.", "bld.", "tile.", "pile.", "newkind.", "xpskill.",
        "simjob.", "simmade.", "perf.",
    ];
    let mut rows: Vec<(String, f64, f64, Vec<f64>, &'static str, f64)> = Vec::new();
    for pre in order {
        for k in keys.iter().filter(|k| k.starts_with(pre)) {
            let sim_only = ["ev.", "perf.", "simjob.", "simmade.", "start."].iter().any(|p| k.starts_with(p));
            let r = real.get(k).copied().unwrap_or(if sim_only || k == "tile.engravings_added" { f64::NAN } else { 0.0 });
            let s: Vec<f64> = outs.iter().map(|o| o.metrics.get(k).copied().unwrap_or(0.0)).collect();
            // Skip per-kind rows that are tiny on both sides.
            if (pre == "newkind." || pre == "xpskill." || pre == "simjob." || pre == "simmade.") && !(r.abs() >= 20.0) && s.iter().all(|v| v.abs() < 20.0) {
                continue;
            }
            let (v, ratio) = if k.starts_with("start.") { ("-", f64::NAN) } else { verdict(r, &s) };
            let s = if k.starts_with("start.") { vec![f64::NAN; s.len()] } else { s };
            let st = if k.starts_with("start.") { r } else { start.get(k).copied().unwrap_or(f64::NAN) };
            rows.push((k.clone(), st, r, s, v, ratio));
        }
    }
    println!();
    println!("{:<44} {:>9} {:>10} {:>10} {:>10} {:>10}  {:<9} {:>6}", "metric", "DF start", "DF real", "sim min", "sim med", "sim max", "verdict", "ratio");
    let mut md = String::new();
    md.push_str("| metric | DF start | DF end / change | sim min | sim median | sim max | verdict | sim/DF |\n|---|---|---|---|---|---|---|---|\n");
    for (k, st, r, s, v, ratio) in &rows {
        let mut t = s.clone();
        t.sort_by(|a, b| a.total_cmp(b));
        let (lo, med, hi) = (t[0], t[t.len() / 2], t[t.len() - 1]);
        let rs = if ratio.is_nan() { "-".to_string() } else { format!("{ratio:.2}") };
        println!("{:<44} {:>9} {:>10} {:>10} {:>10} {:>10}  {:<9} {:>6}", k, fmt(*st), fmt(*r), fmt(lo), fmt(med), fmt(hi), v, rs);
        md.push_str(&format!("| {k} | {} | {} | {} | {} | {} | {v} | {rs} |\n", fmt(*st), fmt(*r), fmt(lo), fmt(med), fmt(hi)));
    }
    let n_in = rows.iter().filter(|r| r.4 == "in range" || r.4 == "close").count();
    let n_cmp = rows.iter().filter(|r| r.4 != "-").count();
    println!("\n{n_in} of {n_cmp} metrics in range or close; total {:.1}s", t0.elapsed().as_secs_f64());
    if let Some(p) = get("--md") {
        std::fs::write(&p, md).expect("write --md");
        println!("markdown table written to {p}");
    }
}
