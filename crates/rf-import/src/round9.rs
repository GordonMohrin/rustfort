//! Round-9 export data (exporter rev `2026-10-10f`): `stockpile_options.json` into
//! `World::stockpile_options`; DF's wealth statistics, invasion tiers and invasion history (in
//! `settings.json` `other` as `WEALTH_CREATED=...` etc., and `invasions.json`); and the unit
//! repair for units DF does not simulate.
//!
//! Units: DF keeps the animals of departed caravans in `world.units.all` with `flags1.forest`,
//! position -30000 and `flags1.inactive` unset, but no longer in `world.units.active`. Exports
//! before round 9 wrote them as active (Gordon's r5 export: 80 of 87 tame animals). Since round 9
//! the exporter sets `flags.inactive` for every unit outside `world.units.active` and places a
//! caged unit at its cage item; for older exports [`repair_units`] marks off-map units that are
//! not caged as inactive.

use super::{Manifest, Warnings};
use rf_core::{GameSettings, StockpileOption, World};
use serde::Deserialize;
use std::path::Path;

/// One `stockpile_options.json` group: leaves as `[token, label]` pairs.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawGroup {
    group: String,
    leaves: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawCategory {
    category: String,
    options: Vec<String>,
    groups: Vec<RawGroup>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawOptions {
    misc: Vec<String>,
    categories: Vec<RawCategory>,
}

/// `stockpile_options.json` as a flat list of `category/group/token` leaves (category options
/// as `category/options/<name>`, the pile-wide organic/inorganic switches as
/// `misc/options/<name>`). Missing file: empty.
pub(crate) fn load_stockpile_options(dir: &Path, w: &mut Warnings) -> Vec<StockpileOption> {
    let p = dir.join("stockpile_options.json");
    if !p.exists() {
        return Vec::new();
    }
    let raw: RawOptions = match super::read_json::<RawOptions>(&p, "stockpile_options", false) {
        Ok((v, ww)) => {
            w.merge(ww);
            v
        }
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut bad = 0usize;
    for c in &raw.categories {
        for o in &c.options {
            out.push(StockpileOption { path: format!("{}/options/{o}", c.category), label: o.replace('_', " ") });
        }
        for g in &c.groups {
            for (tok, label) in &g.leaves {
                if tok.is_empty() || tok.contains('/') {
                    bad += 1;
                    continue;
                }
                out.push(StockpileOption { path: format!("{}/{}/{tok}", c.category, g.group), label: label.clone() });
            }
        }
    }
    for m in &raw.misc {
        out.push(StockpileOption { path: format!("misc/options/{m}"), label: m.replace('_', " ") });
    }
    if bad > 0 {
        w.add("stockpile_options", format!("{bad} leaves with an empty token or a '/' in the token dropped"));
    }
    out
}

/// The exporter's nested `stockpile_options.json` layout for a flat option list (used by
/// `write_export`): categories and groups in first-seen order.
pub(crate) fn stockpile_options_json(options: &[StockpileOption]) -> String {
    use serde_json::{json, Value};
    let mut cats: Vec<(String, Vec<String>, Vec<(String, Vec<Value>)>)> = Vec::new();
    let mut misc = Vec::new();
    for o in options {
        let mut it = o.path.splitn(3, '/');
        let (Some(cat), Some(group), Some(tok)) = (it.next(), it.next(), it.next()) else { continue };
        if cat == "misc" {
            misc.push(tok.to_string());
            continue;
        }
        let ci = match cats.iter().position(|c| c.0 == cat) {
            Some(i) => i,
            None => {
                cats.push((cat.to_string(), Vec::new(), Vec::new()));
                cats.len() - 1
            }
        };
        let c = &mut cats[ci];
        if group == "options" {
            c.1.push(tok.to_string());
            continue;
        }
        let gi = match c.2.iter().position(|g| g.0 == group) {
            Some(i) => i,
            None => {
                c.2.push((group.to_string(), Vec::new()));
                c.2.len() - 1
            }
        };
        c.2[gi].1.push(json!([tok, o.label]));
    }
    let categories: Vec<Value> = cats
        .into_iter()
        .map(|(c, opts, groups)| {
            let groups: Vec<Value> = groups.into_iter().map(|(g, leaves)| json!({ "group": g, "leaves": leaves })).collect();
            json!({ "category": c, "options": opts, "groups": groups })
        })
        .collect();
    json!({ "scheme": "category/group/token", "misc": misc, "categories": categories, "source": "rf-import::write_export" }).to_string()
}

/// Leaves of `options` in category `cat` as `(path, label)` (the shape of
/// `Sim::stockpile_filter_options`).
pub fn options_for(options: &[StockpileOption], cat: &str) -> Vec<(String, String)> {
    let prefix = format!("{cat}/");
    options.iter().filter(|o| o.path.starts_with(&prefix)).map(|o| (o.path.clone(), o.label.clone())).collect()
}

/// DF's wealth statistics and invasion tiers of the fort (round 9, from `settings.other`;
/// `None` for older exports). Values are DF's `plotinfo.tasks.wealth` (created = `total`,
/// imported/exported = trade totals) and `plotinfo.progress_*` at export time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WealthInfo {
    pub created: i64,
    pub exported: i64,
    pub imported: i64,
    pub offered: i64,
    pub architecture: i64,
    pub displayed: i64,
    /// DF's invasion tier indices (0-5) reached by population, trade and production; DF
    /// compares them with the `ENEMY_POP/TRADE/PROD_TRIGGER` difficulty lists.
    pub progress_population: i32,
    pub progress_trade: i32,
    pub progress_production: i32,
    pub fortress_rank: i32,
}

fn other_value(s: &GameSettings, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    s.other.iter().find_map(|o| o.strip_prefix(&prefix).map(|v| v.to_string()))
}

/// Parse [`WealthInfo`] from `settings.other` (`WEALTH_CREATED=...`); `None` without it.
pub fn wealth(s: &GameSettings) -> Option<WealthInfo> {
    let num = |k: &str| other_value(s, k).and_then(|v| v.trim().parse::<i64>().ok());
    let created = num("WEALTH_CREATED")?;
    Some(WealthInfo {
        created,
        exported: num("WEALTH_EXPORTED").unwrap_or(0),
        imported: num("WEALTH_IMPORTED").unwrap_or(0),
        offered: num("WEALTH_OFFERED").unwrap_or(0),
        architecture: num("WEALTH_ARCHITECTURE").unwrap_or(0),
        displayed: num("WEALTH_DISPLAYED").unwrap_or(0),
        progress_population: num("PROGRESS_POPULATION").unwrap_or(-1) as i32,
        progress_trade: num("PROGRESS_TRADE").unwrap_or(-1) as i32,
        progress_production: num("PROGRESS_PRODUCTION").unwrap_or(-1) as i32,
        fortress_rank: num("FORTRESS_RANK").unwrap_or(-1) as i32,
    })
}

/// One `invasions.json` entry (DF `plotinfo.invasions.list`): every siege/raid DF sent to the
/// fort, with its size.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct InvasionInfo {
    pub id: i32,
    pub civ_id: i32,
    /// Units DF planned for the invasion.
    pub size: i32,
    pub year: i32,
    pub year_tick: i32,
    pub siege: bool,
    /// Still under way at export time.
    pub active: bool,
    pub mission: i32,
    pub active_size1: i32,
    pub active_size2: i32,
    pub total_death_cage_number: i32,
}

/// `invasions.json` (missing: empty).
pub fn load_invasions(dir: &Path) -> Vec<InvasionInfo> {
    std::fs::read_to_string(dir.join("invasions.json"))
        .ok()
        .and_then(|t| serde_json::from_str(super::strip_bom(&t)).ok())
        .unwrap_or_default()
}

/// Exporter revisions before round 9 marked only `flags1.inactive` units as inactive.
fn before_round9(m: &Manifest) -> bool {
    m.exporter_rev.as_str() < "2026-10-10f"
}

/// Units DF does not simulate. Older exports: an off-map unit that is not caged (caravan animals
/// that left, units away) becomes inactive. Every export: count the caravan animals that left.
pub(crate) fn repair_units(world: &mut World, m: &Manifest, w: &mut Warnings) {
    let dims = world.dims;
    let inside = |p: rf_core::Pos| p.x >= 0 && p.y >= 0 && p.z >= 0 && (p.x as u16) < dims.x && (p.y as u16) < dims.y && (p.z as u16) < dims.z;
    let old = before_round9(m);
    let (mut marked, mut caravan) = (0usize, 0usize);
    for u in &mut world.units {
        if u.flags.dead || inside(u.pos) {
            continue;
        }
        if u.flags.forest && u.flags.tame && !u.flags.citizen {
            caravan += 1;
        }
        if old && !u.flags.inactive && !u.flags.caged {
            u.flags.inactive = true;
            marked += 1;
        }
    }
    if marked > 0 {
        w.add(
            "units",
            format!(
                "{marked} off-map units marked inactive (pre-round-9 export; DF does not simulate them: {caravan} are animals of departed caravans)"
            ),
        );
    }
}
