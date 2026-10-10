//! [app8] Stockpile settings like DF Steam: categories on the left (with DF's on / partial /
//! off marks), the groups of the selected category in the middle, the leaf toggles on the
//! right; presets (DF's custom-stockpile buttons); containers (barrels, bins, wheelbarrows);
//! links (give to / take from, picked by clicking another pile or workshop on the map) and
//! "take from links only". Everything is written with `Sim::set_stockpile_settings`.
//!
//! Leaves come from `Sim::stockpile_filter_options(category)` as `category/group/token`; while
//! the sim offers none, a list built from the save's raws and materials stands in (marked as
//! an estimate in docs/FIDELITY.md). Filters are kept per category: a category without any
//! `category/...` filter accepts everything in it (the rf-core contract).

use super::round8::{frame, scroll};
use super::{clip, col, draw_icon, icon_button, pretty, text_button, BtnState, Ico};
use crate::font::GH;
use crate::game::{Game, Screen, Tool, ALL_STOCK_CATEGORIES};
use macroquad::prelude::*;
use rf_core::{StockpileSettings, World};

/// Category label and DF icon.
pub fn cat_info(cat: &str) -> (&'static str, &'static str) {
    match cat {
        "animals" => ("Animals", "STOCKPILE_ICON_ANIMALS"),
        "food" => ("Food", "STOCKPILE_ICON_FOOD"),
        "furniture" => ("Furniture", "STOCKPILE_ICON_FURNITURE"),
        "corpses" => ("Corpses", "STOCKPILE_ICON_CORPSES"),
        "refuse" => ("Refuse", "STOCKPILE_ICON_REFUSE"),
        "stone" => ("Stone", "STOCKPILE_ICON_STONE"),
        "ammo" => ("Ammo", "STOCKPILE_ICON_AMMO"),
        "coins" => ("Coins", "STOCKPILE_ICON_COINS"),
        "bars_blocks" => ("Bars/blocks", "STOCKPILE_ICON_BARS"),
        "gems" => ("Gems", "STOCKPILE_ICON_GEMS"),
        "finished_goods" => ("Finished goods", "STOCKPILE_ICON_FINISHED_GOODS"),
        "leather" => ("Leather", "STOCKPILE_ICON_LEATHER"),
        "cloth" => ("Cloth", "STOCKPILE_ICON_CLOTH"),
        "wood" => ("Wood", "STOCKPILE_ICON_WOOD"),
        "weapons" => ("Weapons/trap comps", "STOCKPILE_ICON_WEAPONS"),
        "armor" => ("Armor", "STOCKPILE_ICON_ARMOR"),
        "sheet" => ("Sheets", "STOCKPILE_ICON_SHEETS"),
        _ => ("Other", "STOCKPILE_ICON_CUSTOM"),
    }
}

pub struct Preset {
    pub label: &'static str,
    pub icon: &'static str,
    /// Categories turned on (everything in them) ...
    pub cats: &'static [&'static str],
    /// ... or, for a single category, only the leaves whose path contains one of these.
    pub only: &'static [&'static str],
    /// Leaves excluded (path contains).
    pub except: &'static [&'static str],
}

/// DF's custom-stockpile presets (the `CUSTOM_STOCKPILE_*` buttons). Which categories
/// "organic" and "inorganic" cover is an estimate.
pub const PRESETS: &[Preset] = &[
    Preset { label: "Everything", icon: "STOCKPILE_ICON_ALL", cats: ALL_STOCK_CATEGORIES, only: &[], except: &[] },
    Preset { label: "Organic", icon: "CUSTOM_STOCKPILE_ORGANIC_ON", cats: &["food", "corpses", "refuse", "leather", "cloth", "wood", "sheet"], only: &[], except: &[] },
    Preset {
        label: "Inorganic",
        icon: "CUSTOM_STOCKPILE_INORGANIC_ON",
        cats: &["stone", "ammo", "coins", "bars_blocks", "gems", "finished_goods", "furniture", "weapons", "armor"],
        only: &[],
        except: &[],
    },
    Preset { label: "Empty cages", icon: "CUSTOM_STOCKPILE_EMPTY_CAGE_ON", cats: &["animals"], only: &["animals/cages/"], except: &[] },
    Preset { label: "Empty animal traps", icon: "CUSTOM_STOCKPILE_EMPTY_ANIMAL_TRAP_ON", cats: &["animals"], only: &["animals/traps/"], except: &[] },
    Preset { label: "Prepared food", icon: "CUSTOM_STOCKPILE_PREPARED_FOOD_ON", cats: &["food"], only: &["food/prepared_meals/"], except: &[] },
    Preset { label: "Usable weapons", icon: "CUSTOM_STOCKPILE_USABLE_WEAPON_ON", cats: &["weapons"], only: &[], except: &["/UNUSABLE"] },
    Preset { label: "Unusable weapons", icon: "CUSTOM_STOCKPILE_UNUSABLE_WEAPON_ON", cats: &["weapons"], only: &[], except: &["/USABLE"] },
    Preset { label: "Usable armor", icon: "CUSTOM_STOCKPILE_USABLE_ARMOR_ON", cats: &["armor"], only: &[], except: &["/UNUSABLE"] },
    Preset { label: "Unusable armor", icon: "CUSTOM_STOCKPILE_UNUSABLE_ARMOR_ON", cats: &["armor"], only: &[], except: &["/USABLE"] },
    Preset { label: "Corpses", icon: "CUSTOM_STOCKPILE_CORPSES_ON", cats: &["corpses"], only: &[], except: &[] },
    Preset { label: "Nothing", icon: "STOCKPILE_ICON_BLANK", cats: &[], only: &[], except: &[] },
];

const QUALITY: &[&str] = &["Ordinary", "Well-crafted", "Finely-crafted", "Superior", "Exceptional", "Masterful", "Artifact"];
const FURNITURE: &[&str] = &[
    "ARMORSTAND", "BED", "CHAIR", "COFFIN", "DOOR", "FLOODGATE", "HATCH_COVER", "GRATE", "CABINET", "BOX", "BIN", "BARREL", "BUCKET", "CAGE", "TABLE",
    "STATUE", "SLAB", "QUERN", "MILLSTONE", "WEAPONRACK", "WINDOW", "ANVIL", "TRACTION_BENCH", "BAR", "BLOCKS", "PIPE_SECTION", "CHAIN", "BOOKCASE",
    "PEDESTAL", "DISPLAY_CASE", "SAND_BAG", "OTHER_LARGE_TOOLS",
];
const FINISHED: &[&str] = &[
    "CHAIN", "FLASK", "GOBLET", "INSTRUMENT", "TOY", "ARMOR", "SHOES", "HELM", "GLOVES", "FIGURINE", "AMULET", "SCEPTER", "CROWN", "RING", "EARRING",
    "BRACELET", "GEM", "LARGE_GEM", "TOTEM", "PANTS", "BACKPACK", "QUIVER", "SPLINT", "CRUTCH", "TOOL", "BOOK",
];
const REFUSE_TYPES: &[&str] = &["BAR", "SMALLGEM", "BLOCKS", "ROUGH", "BOULDER", "WOOD", "CORPSE", "CORPSEPIECE", "REMAINS", "MEAT", "FISH", "VERMIN", "SEEDS", "PLANT", "SKIN_TANNED", "THREAD", "CLOTH", "DRINK", "POWDER_MISC", "CHEESE", "FOOD", "LIQUID_MISC", "GLOB", "EGG", "PLANT_GROWTH"];
const TRAPCOMP: &[&str] = &["ITEM_TRAPCOMP_GIANTAXEBLADE", "ITEM_TRAPCOMP_ENORMOUSCORKSCREW", "ITEM_TRAPCOMP_SPIKEDBALL", "ITEM_TRAPCOMP_LARGESERRATEDDISC", "ITEM_TRAPCOMP_MENACINGSPIKE"];

fn mat_names(w: &World, f: impl Fn(&rf_core::MaterialInfo) -> bool) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = w.materials.iter().filter(|m| f(m)).map(|m| (m.token.clone(), m.name.clone())).collect();
    v.sort_by(|a, b| a.1.cmp(&b.1));
    v.dedup_by(|a, b| a.0 == b.0);
    v
}

fn economic(m: &rf_core::MaterialInfo) -> bool {
    m.tags.iter().any(|t| matches!(t.as_str(), "RC:FLUX" | "RC:GYPSUM" | "RC:CALCIUM_CARBONATE" | "IS_CLAY"))
        || ["BITUMINOUS_COAL", "LIGNITE", "KAOLINITE", "RAW_ADAMANTINE", "ALUNITE", "SALTPETER", "CRYOLITE", "PERICLASE", "ILMENITE", "RUTILE", "TALC"].iter().any(|t| m.token.ends_with(t))
}

/// Leaves of a category built from the save's data (used while the sim offers none).
pub fn fallback_options(w: &World, cat: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |group: &str, tok: &str, label: String| out.push((format!("{cat}/{group}/{tok}"), label));
    let creatures: Vec<(String, String)> = w.creature_raws.iter().map(|c| (c.token.clone(), if c.name.is_empty() { pretty(&c.token).to_lowercase() } else { c.name.clone() })).collect();
    let plant_name = |tok: &str| w.plant_raws.iter().find(|p| p.token == tok).map(|p| p.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| pretty(tok).to_lowercase());
    let metals = mat_names(w, |m| m.is_metal);
    let stones = mat_names(w, |m| m.is_stone);
    let gems = mat_names(w, |m| m.is_gem);
    let quality = |add: &mut dyn FnMut(&str, &str, String)| {
        for q in QUALITY {
            add("quality_core", q, q.to_string());
        }
        for q in QUALITY {
            add("quality_total", q, q.to_string());
        }
    };
    let material_groups = |add: &mut dyn FnMut(&str, &str, String)| {
        for (k, l) in [("STONE", "stone"), ("METAL", "metal"), ("WOOD", "wood"), ("GLASS", "glass"), ("BONE", "bone"), ("SHELL", "shell"), ("LEATHER", "leather"), ("CLOTH", "cloth"), ("OTHER", "other materials")] {
            add("material", k, l.to_string());
        }
    };
    match cat {
        "animals" => {
            add("cages", "EMPTY_CAGES", "Empty cages".into());
            add("traps", "EMPTY_ANIMAL_TRAPS", "Empty animal traps".into());
            for (t, n) in &creatures {
                add("creatures", t, n.clone());
            }
        }
        "food" => {
            add("prepared_meals", "FOOD", "Prepared food".into());
            for (t, n) in &creatures {
                add("meat", &format!("CREATURE:{t}:MUSCLE"), format!("{n} meat"));
            }
            for (t, n) in &creatures {
                add("eggs", &format!("CREATURE:{t}:EGG"), format!("{n} eggs"));
            }
            for (t, n) in &creatures {
                add("fat", &format!("CREATURE:{t}:FAT"), format!("{n} fat"));
            }
            for p in &w.plant_raws {
                let n = plant_name(&p.token);
                if p.edible_raw || p.edible_cooked || p.brewable {
                    add("plants", &format!("PLANT:{}:STRUCTURAL", p.token), n.clone());
                }
                if p.brewable {
                    add("drink_plant", &format!("PLANT:{}:DRINK", p.token), format!("{n} drink"));
                }
                if p.has_seed {
                    add("seeds", &format!("PLANT:{}:SEED", p.token), format!("{n} seeds"));
                }
                if p.millable {
                    add("powder_plant", &format!("PLANT:{}:MILL", p.token), format!("{n} flour"));
                }
            }
            add("drink_animal", "CREATURE:MEAD", "mead".into());
            add("cheese_animal", "CHEESE", "cheese".into());
            add("liquid_misc", "LIQUID_MISC", "milk and other liquids".into());
        }
        "furniture" => {
            for f in FURNITURE {
                add("type", f, pretty(f));
            }
            material_groups(&mut add);
            quality(&mut add);
        }
        "corpses" => {
            for (t, n) in &creatures {
                add("creatures", t, n.clone());
            }
        }
        "refuse" => {
            for t in REFUSE_TYPES {
                add("type", t, pretty(t));
            }
            for g in ["corpses", "body_parts", "skulls", "bones", "shells", "teeth", "horns", "hair", "hides_fresh", "hides_rotten"] {
                for (t, n) in &creatures {
                    add(g, t, n.clone());
                }
            }
        }
        "stone" => {
            for (t, n) in mat_names(w, |m| m.is_ore) {
                add("metal_ores", &t, n);
            }
            for (t, n) in mat_names(w, |m| m.is_stone && !m.is_ore && economic(m)) {
                add("economic", &t, n);
            }
            for (t, n) in mat_names(w, |m| m.is_stone && !m.is_ore && !economic(m)) {
                add("other", &t, n);
            }
            for (t, n) in mat_names(w, |m| m.tags.iter().any(|x| x == "IS_CLAY")) {
                add("clay", &t, n);
            }
        }
        "ammo" => {
            for wr in w.weapon_raws.iter().filter(|r| r.kind == "AMMO" && r.token.starts_with("ITEM_")) {
                add("type", &wr.token, if wr.name.is_empty() { pretty(&wr.token) } else { wr.name.clone() });
            }
            for (t, n) in &metals {
                add("material", t, n.clone());
            }
            for (k, l) in [("WOOD", "wood"), ("BONE", "bone"), ("OTHER", "other materials")] {
                add("material", k, l.to_string());
            }
            quality(&mut add);
        }
        "coins" => {
            for (t, n) in &metals {
                add("material", t, n.clone());
            }
        }
        "bars_blocks" => {
            for (t, n) in &metals {
                add("bars_metal", t, format!("{n} bars"));
            }
            for (k, l) in [("COAL", "coal"), ("POTASH", "potash"), ("ASH", "ash"), ("PEARLASH", "pearlash"), ("SOAP", "soap")] {
                add("bars_other", k, l.to_string());
            }
            for (t, n) in &stones {
                add("blocks_stone", t, format!("{n} blocks"));
            }
            for (t, n) in &metals {
                add("blocks_metal", t, format!("{n} blocks"));
            }
            for (k, l) in [("WOOD", "wooden blocks"), ("GLASS", "glass blocks")] {
                add("blocks_other", k, l.to_string());
            }
        }
        "gems" => {
            for (t, n) in &gems {
                add("rough_gems", t, format!("rough {n}"));
            }
            for (k, l) in [("GLASS_GREEN", "rough green glass"), ("GLASS_CLEAR", "rough clear glass"), ("GLASS_CRYSTAL", "rough crystal glass")] {
                add("rough_glass", k, l.to_string());
            }
            for (t, n) in &gems {
                add("cut_gems", t, format!("cut {n}"));
            }
            for (k, l) in [("GLASS_GREEN", "cut green glass"), ("GLASS_CLEAR", "cut clear glass"), ("GLASS_CRYSTAL", "cut crystal glass")] {
                add("cut_glass", k, l.to_string());
            }
            for (t, n) in &stones {
                add("cut_stone", t, format!("cut {n}"));
            }
        }
        "finished_goods" => {
            for f in FINISHED {
                add("type", f, pretty(f));
            }
            material_groups(&mut add);
            quality(&mut add);
        }
        "leather" => {
            for (t, n) in &creatures {
                add("creatures", t, format!("{n} leather"));
            }
        }
        "cloth" => {
            for (g, l) in [("thread", "thread"), ("cloth", "cloth")] {
                for (k, m) in [("SILK", "silk"), ("PLANT", "plant fiber"), ("YARN", "yarn"), ("METAL", "metal")] {
                    add(g, k, format!("{m} {l}"));
                }
            }
        }
        "wood" => {
            let mut v: Vec<(String, String)> =
                w.materials.iter().filter(|m| m.is_wood).map(|m| (m.token.clone(), format!("{} wood", plant_name(m.token.split(':').nth(1).unwrap_or(""))))).collect();
            v.sort_by(|a, b| a.1.cmp(&b.1));
            v.dedup_by(|a, b| a.0 == b.0);
            for (t, n) in v {
                add("type", &t, n);
            }
        }
        "weapons" => {
            for wr in w.weapon_raws.iter().filter(|r| r.kind == "WEAPON" && r.token.starts_with("ITEM_")) {
                add("weapons", &wr.token, if wr.name.is_empty() { pretty(&wr.token) } else { wr.name.clone() });
            }
            for t in TRAPCOMP {
                let n = w.weapon_raws.iter().find(|r| r.token == *t).map(|r| r.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| pretty(t.trim_start_matches("ITEM_TRAPCOMP_")).to_lowercase());
                add("trap_comps", t, n);
            }
            for (t, n) in &metals {
                add("material", t, n.clone());
            }
            for (k, l) in [("STONE", "stone"), ("WOOD", "wood"), ("BONE", "bone"), ("OTHER", "other materials")] {
                add("material", k, l.to_string());
            }
            quality(&mut add);
            add("usable", "USABLE", "Usable".into());
            add("usable", "UNUSABLE", "Unusable".into());
        }
        "armor" => {
            for (kind, group) in [("ARMOR", "body"), ("HELM", "head"), ("SHOES", "feet"), ("GLOVES", "hands"), ("PANTS", "legs"), ("SHIELD", "shield")] {
                for ar in w.armor_raws.iter().filter(|a| a.kind == kind) {
                    add(group, &ar.token, if ar.name.is_empty() { pretty(&ar.token) } else { ar.name.clone() });
                }
            }
            for (t, n) in &metals {
                add("material", t, n.clone());
            }
            for (k, l) in [("LEATHER", "leather"), ("CLOTH", "cloth"), ("BONE", "bone"), ("SHELL", "shell"), ("WOOD", "wood"), ("OTHER", "other materials")] {
                add("material", k, l.to_string());
            }
            quality(&mut add);
            add("usable", "USABLE", "Usable".into());
            add("usable", "UNUSABLE", "Unusable".into());
        }
        "sheet" => {
            add("paper", "PAPER", "paper".into());
            add("parchment", "PARCHMENT", "parchment".into());
        }
        _ => {}
    }
    out
}

/// The leaves of a category: the sim's list, else the stand-in (cached per editor session).
pub fn options(g: &mut Game, cat: &str) -> Vec<(String, String)> {
    if let Some((_, v)) = g.r8.sp_cache.iter().find(|(c, _)| c == cat) {
        return v.clone();
    }
    let mut v = g.sim.stockpile_filter_options(cat);
    if v.is_empty() {
        v = fallback_options(&g.sim.world, cat);
    }
    g.r8.sp_cache.push((cat.to_string(), v.clone()));
    v
}

/// Group of a leaf path `category/group/token`.
pub fn group_of(path: &str) -> &str {
    path.split('/').nth(1).unwrap_or("")
}

fn cat_filters<'a>(sp: &'a StockpileSettings, cat: &str) -> impl Iterator<Item = &'a String> + 'a {
    let pre = format!("{cat}/");
    sp.filters.iter().filter(move |f| f.starts_with(&pre))
}

pub fn leaf_on(sp: &StockpileSettings, cat: &str, path: &str) -> bool {
    sp.categories.iter().any(|c| c == cat) && (cat_filters(sp, cat).next().is_none() || sp.filters.iter().any(|f| f == path))
}

/// 0 = off, 1 = partial, 2 = on (DF's STOCKPILE_OFF / _PARTIAL / _ON).
pub fn cat_state(sp: &StockpileSettings, cat: &str) -> u8 {
    if !sp.categories.iter().any(|c| c == cat) {
        0
    } else if cat_filters(sp, cat).next().is_some() {
        1
    } else {
        2
    }
}

/// Set leaves `paths` of `cat` on or off; `all` are all leaves of the category.
pub fn set_leaves(sp: &mut StockpileSettings, cat: &str, all: &[(String, String)], paths: &[String], on: bool) {
    let cat_on = sp.categories.iter().any(|c| c == cat);
    let pre = format!("{cat}/");
    if !cat_on {
        if !on {
            return;
        }
        sp.categories.push(cat.to_string());
        sp.filters.retain(|f| !f.starts_with(&pre));
        sp.filters.extend(paths.iter().cloned());
    } else {
        if cat_filters(sp, cat).next().is_none() {
            if on {
                return;
            }
            sp.filters.extend(all.iter().map(|(p, _)| p.clone()));
        }
        if on {
            for p in paths {
                if !sp.filters.contains(p) {
                    sp.filters.push(p.clone());
                }
            }
        } else {
            sp.filters.retain(|f| !paths.contains(f));
        }
        let n = cat_filters(sp, cat).count();
        if n == 0 {
            sp.categories.retain(|c| c != cat);
        }
    }
    // Compact: every leaf on = no filters for the category.
    if !all.is_empty() && all.iter().all(|(p, _)| sp.filters.contains(p)) {
        sp.filters.retain(|f| !f.starts_with(&pre));
    }
}

pub fn toggle_cat(sp: &mut StockpileSettings, cat: &str) {
    let pre = format!("{cat}/");
    sp.filters.retain(|f| !f.starts_with(&pre));
    if let Some(i) = sp.categories.iter().position(|c| c == cat) {
        sp.categories.remove(i);
    } else {
        sp.categories.push(cat.to_string());
    }
}

/// Apply a preset (categories and, for single-category presets, the leaves).
pub fn apply_preset(g: &mut Game, sp: &mut StockpileSettings, p: &Preset) {
    sp.categories.clear();
    sp.filters.clear();
    for c in p.cats {
        sp.categories.push(c.to_string());
        if !p.only.is_empty() || !p.except.is_empty() {
            let all = options(g, c);
            let keep: Vec<String> = all
                .iter()
                .map(|(path, _)| path.clone())
                .filter(|path| (p.only.is_empty() || p.only.iter().any(|o| path.starts_with(o))) && !p.except.iter().any(|e| path.ends_with(e)))
                .collect();
            if keep.len() < all.len() {
                sp.filters.extend(keep);
            }
        }
    }
}

/// Add a give-to/take-from link; links between two piles are kept on both sides like DF.
pub fn add_link(g: &mut Game, pile: i32, target: i32, give: bool) {
    let Some(mut sp) = g.sim.world.building(pile).and_then(|b| b.stockpile.clone()) else { return };
    let list = if give { &mut sp.give_to } else { &mut sp.take_from };
    if !list.contains(&target) {
        list.push(target);
    }
    g.sim.set_stockpile_settings(pile, sp);
    if let Some(mut other) = g.sim.world.building(target).filter(|b| b.kind == "Stockpile").and_then(|b| b.stockpile.clone()) {
        let l = if give { &mut other.take_from } else { &mut other.give_to };
        if !l.contains(&pile) {
            l.push(pile);
            g.sim.set_stockpile_settings(target, other);
        }
    }
}

pub fn remove_link(g: &mut Game, pile: i32, target: i32, give: bool) {
    let Some(mut sp) = g.sim.world.building(pile).and_then(|b| b.stockpile.clone()) else { return };
    if give {
        sp.give_to.retain(|&x| x != target);
    } else {
        sp.take_from.retain(|&x| x != target);
    }
    g.sim.set_stockpile_settings(pile, sp);
    if let Some(mut other) = g.sim.world.building(target).filter(|b| b.kind == "Stockpile").and_then(|b| b.stockpile.clone()) {
        if give {
            other.take_from.retain(|&x| x != pile);
        } else {
            other.give_to.retain(|&x| x != pile);
        }
        g.sim.set_stockpile_settings(target, other);
    }
}

fn state_icon(g: &mut Game, st: u8, r: Rect) {
    let name = ["STOCKPILE_OFF", "STOCKPILE_PARTIAL", "STOCKPILE_ON"][st.min(2) as usize];
    if !draw_icon(&g.gfx, &mut g.rend, name, r, WHITE) {
        let (gl, c) = [(b'x', [200, 80, 70]), (b'~', [230, 200, 90]), (251u8, [110, 220, 110])][st.min(2) as usize];
        super::draw_fallback_icon(&g.font, Ico::Glyph(gl, c), r.x, r.y, r.w, r.h);
    }
}

pub fn draw(g: &mut Game) {
    let Some(pid) = g.r8.pile else {
        g.screen = Screen::None;
        return;
    };
    let Some(b) = g.sim.world.building(pid).cloned() else {
        g.screen = Screen::None;
        return;
    };
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let title = format!("Stockpile #{} ({}x{}, z {})", b.id, b.x2 - b.x1 + 1, b.y2 - b.y1 + 1, b.z);
    let Some(c) = frame(g, &title, 1400.0) else { return };
    let sp0 = b.stockpile.clone().unwrap_or_default();
    let mut sp = sp0.clone();
    let mut y = c.y + 1.5 * GH * s + 10.0 * s;
    // ---- presets
    g.font.draw("Presets:", c.x, y + 10.0 * s, s, col::BORDER_HI);
    let mut px = c.x + 9.0 * 8.0 * s;
    let mut preset = None;
    for (i, p) in PRESETS.iter().enumerate() {
        let r = Rect::new(px, y, 32.0 * s, 36.0 * s);
        if icon_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, r, p.icon, Ico::Crate, BtnState::Normal, p.label) {
            preset = Some(i);
        }
        px += 36.0 * s;
    }
    if let Some(i) = preset {
        apply_preset(g, &mut sp, &PRESETS[i]);
        g.set_status(format!("Stockpile #{pid}: preset {}", PRESETS[i].label));
    }
    // ---- containers (right of the presets)
    let tiles = ((b.x2 - b.x1 + 1) as i32 * (b.y2 - b.y1 + 1) as i32).max(1);
    let mut cx = px + 20.0 * s;
    let bh = GH * s + 8.0 * s;
    for (k, label) in ["Barrels", "Bins", "Wheelbarrows"].iter().enumerate() {
        let v = [sp.max_barrels, sp.max_bins, sp.max_wheelbarrows][k];
        g.font.draw(label, cx, y - 1.0 * s, s, col::TEXT_DIM);
        let by = y + lh;
        let step = if g.input.mods & crate::keys::SHIFT != 0 { 10 } else { 1 };
        let mut nv = v;
        if text_button(&mut g.ui, &g.font, Rect::new(cx, by, 18.0 * s, bh), "-", BtnState::Normal) {
            nv = (v - step).max(0);
        }
        let t = v.to_string();
        g.font.draw(&t, cx + 22.0 * s + (28.0 * s - g.font.width(&t, s)) / 2.0, by + 4.0 * s, s, if v > 0 { col::TEXT } else { col::TEXT_DIM });
        if text_button(&mut g.ui, &g.font, Rect::new(cx + 54.0 * s, by, 18.0 * s, bh), "+", BtnState::Normal) {
            nv = (v + step).min(if k == 2 { 3.max(tiles / 4) } else { tiles });
        }
        match k {
            0 => sp.max_barrels = nv,
            1 => sp.max_bins = nv,
            _ => sp.max_wheelbarrows = nv,
        }
        cx += (label.len() as f32 + 2.0).max(10.0) * 8.0 * s;
    }
    y += 36.0 * s + 8.0 * s;
    // ---- links
    let half = 13.0 * 8.0 * s;
    let mut link_tool = None;
    let lr = Rect::new(c.x, y, half + 40.0 * s, 30.0 * s);
    if super::icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, lr, "STOCKPILE_CONNECTIONS_ADD_GIVE_LINK", Ico::Glyph(26, [110, 220, 110]), "Give to...", BtnState::Normal) {
        link_tool = Some(true);
    }
    let lr2 = Rect::new(lr.x + lr.w + 6.0 * s, y, half + 40.0 * s, 30.0 * s);
    if super::icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, lr2, "STOCKPILE_CONNECTIONS_ADD_TAKE_LINK", Ico::Glyph(27, [240, 160, 60]), "Take from...", BtnState::Normal) {
        link_tool = Some(false);
    }
    let lr3 = Rect::new(lr2.x + lr2.w + 6.0 * s, y, 28.0 * 8.0 * s, 30.0 * s);
    let (li, lt) = if sp.use_links_only { ("STOCKPILE_TAKE_FROM_LINKS_ONLY", "Takes from links only") } else { ("STOCKPILE_TAKE_FROM_ANYWHERE", "Takes from anywhere") };
    if super::icon_label_button(&mut g.ui, &g.font, &g.gfx, &mut g.rend, lr3, li, Ico::Crate, lt, if sp.use_links_only { BtnState::Active } else { BtnState::Normal }) {
        sp.use_links_only = !sp.use_links_only;
    }
    // Linked buildings, with x to remove.
    let mut lx = lr3.x + lr3.w + 12.0 * s;
    let mut unlink = None;
    let links: Vec<(i32, bool)> = sp.give_to.iter().map(|&i| (i, true)).chain(sp.take_from.iter().map(|&i| (i, false))).collect();
    if links.is_empty() {
        g.font.draw("No links.", lx, y + 8.0 * s, s, col::TEXT_DIM);
    }
    for (id, give) in links {
        let name = g.sim.world.building(id).map(|o| super::building::building_title(g, o)).unwrap_or_else(|| format!("#{id}"));
        let t = format!("{} {}", if give { "→" } else { "←" }, clip(&name, 18));
        let tw = g.font.width(&t, s);
        if lx + tw + 30.0 * s > c.x + c.w {
            g.font.draw("...", lx, y + 8.0 * s, s, col::TEXT_DIM);
            break;
        }
        g.font.draw(&t, lx, y + 8.0 * s, s, if give { col::TEXT_GOOD } else { Color::from_rgba(240, 170, 80, 255) });
        if text_button(&mut g.ui, &g.font, Rect::new(lx + tw + 4.0 * s, y + 5.0 * s, 16.0 * s, GH * s + 4.0 * s), "x", BtnState::Normal) {
            unlink = Some((id, give));
        }
        lx += tw + 28.0 * s;
    }
    y += 30.0 * s + 10.0 * s;
    draw_line(c.x, y - 5.0 * s, c.x + c.w, y - 5.0 * s, 1.0, col::BORDER);
    // ---- three columns
    let colw = 26.0 * 8.0 * s;
    let row_h = 26.0 * s;
    let bottom = c.y + c.h - bh - 6.0 * s;
    g.r8.sp_cat = g.r8.sp_cat.min(ALL_STOCK_CATEGORIES.len() - 1);
    // Categories.
    let mut cat_click = None;
    let mut cat_toggle = None;
    for (i, cat) in ALL_STOCK_CATEGORIES.iter().enumerate() {
        let ry = y + i as f32 * row_h;
        if ry + row_h > bottom {
            break;
        }
        let rr = Rect::new(c.x, ry, colw - 8.0 * s, row_h - 2.0 * s);
        let st = cat_state(&sp, cat);
        let tr = Rect::new(rr.x + 2.0 * s, ry + 1.0 * s, 21.0 * s, row_h - 4.0 * s);
        let hit_toggle = g.ui.clicked(tr);
        if super::list_row(&mut g.ui, rr, g.r8.sp_cat == i) {
            cat_click = Some(i);
        }
        state_icon(g, st, tr);
        let (label, icon) = cat_info(cat);
        let ir = Rect::new(tr.x + tr.w + 4.0 * s, ry + 1.0 * s, 21.0 * s, row_h - 4.0 * s);
        if !draw_icon(&g.gfx, &mut g.rend, icon, ir, WHITE) {
            super::draw_fallback_icon(&g.font, Ico::Crate, ir.x, ir.y, ir.w, ir.h);
        }
        g.font.draw(label, ir.x + ir.w + 6.0 * s, ry + (row_h - 2.0 * s - GH * s) / 2.0, s, if st == 0 { col::TEXT_DIM } else { col::TEXT });
        if hit_toggle {
            cat_toggle = Some(*cat);
        }
    }
    if let Some(i) = cat_click {
        if g.r8.sp_cat != i {
            g.r8.sp_cat = i;
            g.r8.sp_group = 0;
            g.r8.sp_scroll = 0;
        }
    }
    if let Some(cat) = cat_toggle {
        toggle_cat(&mut sp, cat);
    }
    let cat = ALL_STOCK_CATEGORIES[g.r8.sp_cat];
    let all = options(g, cat);
    let mut groups: Vec<&str> = Vec::new();
    for (p, _) in &all {
        let gname = group_of(p);
        if !groups.contains(&gname) {
            groups.push(gname);
        }
    }
    let gx = c.x + colw;
    draw_line(gx - 4.0 * s, y, gx - 4.0 * s, bottom, 1.0, col::BORDER);
    g.r8.sp_group = g.r8.sp_group.min(groups.len().saturating_sub(1));
    let mut group_click = None;
    let mut group_set: Option<(usize, bool)> = None;
    for (i, gname) in groups.iter().enumerate() {
        let ry = y + i as f32 * row_h;
        if ry + row_h > bottom {
            break;
        }
        let leaves: Vec<&String> = all.iter().filter(|(p, _)| group_of(p) == *gname).map(|(p, _)| p).collect();
        let on = leaves.iter().filter(|p| leaf_on(&sp, cat, p)).count();
        let st = if on == 0 { 0 } else if on < leaves.len() { 1 } else { 2 };
        let rr = Rect::new(gx, ry, colw - 8.0 * s, row_h - 2.0 * s);
        let tr = Rect::new(rr.x + 2.0 * s, ry + 1.0 * s, 21.0 * s, row_h - 4.0 * s);
        let hit_toggle = g.ui.clicked(tr);
        if super::list_row(&mut g.ui, rr, g.r8.sp_group == i) {
            group_click = Some(i);
        }
        state_icon(g, st, tr);
        g.font.draw(&clip(&group_label(gname), 18), tr.x + tr.w + 6.0 * s, ry + (row_h - 2.0 * s - GH * s) / 2.0, s, if st == 0 { col::TEXT_DIM } else { col::TEXT });
        let cnt = format!("{on}/{}", leaves.len());
        g.font.draw(&cnt, rr.x + rr.w - g.font.width(&cnt, s) - 4.0 * s, ry + (row_h - 2.0 * s - GH * s) / 2.0, s, col::TEXT_DIM);
        if hit_toggle {
            group_set = Some((i, st != 2));
        }
    }
    if groups.is_empty() {
        g.font.draw("No detailed settings.", gx, y, s, col::TEXT_DIM);
    }
    if let Some(i) = group_click {
        if g.r8.sp_group != i {
            g.r8.sp_group = i;
            g.r8.sp_scroll = 0;
        }
    }
    if let Some((i, on)) = group_set {
        let paths: Vec<String> = all.iter().filter(|(p, _)| group_of(p) == groups[i]).map(|(p, _)| p.clone()).collect();
        set_leaves(&mut sp, cat, &all, &paths, on);
    }
    // Leaves of the selected group.
    let lx0 = gx + colw;
    draw_line(lx0 - 4.0 * s, y, lx0 - 4.0 * s, bottom, 1.0, col::BORDER);
    if let Some(gname) = groups.get(g.r8.sp_group).copied() {
        let leaves: Vec<(String, String)> = all.iter().filter(|(p, _)| group_of(p) == gname).cloned().collect();
        let lw = c.x + c.w - lx0;
        let paths: Vec<String> = leaves.iter().map(|(p, _)| p.clone()).collect();
        let half = 14.0 * 8.0 * s;
        if text_button(&mut g.ui, &g.font, Rect::new(lx0, y, half, bh), "Enable all", BtnState::Normal) {
            set_leaves(&mut sp, cat, &all, &paths, true);
        }
        if text_button(&mut g.ui, &g.font, Rect::new(lx0 + half + 6.0 * s, y, half, bh), "Disable all", BtnState::Normal) {
            set_leaves(&mut sp, cat, &all, &paths, false);
        }
        let lrow = lh + 3.0 * s;
        let ncol = ((lw / (24.0 * 8.0 * s)) as usize).clamp(1, 3);
        let cw = lw / ncol as f32;
        let area = Rect::new(lx0, y + bh + 6.0 * s, lw, bottom - (y + bh + 6.0 * s));
        let per_col = ((area.h / lrow) as usize).max(1);
        let vis = per_col * ncol;
        // Scroll by whole rows of columns.
        let rows_total = leaves.len().div_ceil(ncol);
        let sc = scroll(g, area, g.r8.sp_scroll, rows_total, per_col);
        g.r8.sp_scroll = sc;
        let mut toggle = None;
        for (k, (p, label)) in leaves.iter().enumerate().skip(sc * ncol).take(vis) {
            let idx = k - sc * ncol;
            let (cxl, ry) = (area.x + (idx % ncol) as f32 * cw, area.y + (idx / ncol) as f32 * lrow);
            let on = leaf_on(&sp, cat, p);
            if super::checkbox(&mut g.ui, &g.font, cxl, ry, on, &clip(label, ((cw / (8.0 * s)) as usize).saturating_sub(4))) {
                toggle = Some((p.clone(), !on));
            }
        }
        if let Some((p, on)) = toggle {
            set_leaves(&mut sp, cat, &all, &[p], on);
        }
        if leaves.len() > vis {
            let t = format!("{}-{} of {} (wheel scrolls)", sc * ncol + 1, (sc * ncol + vis).min(leaves.len()), leaves.len());
            g.font.draw(&t, lx0, bottom - GH * s, s, col::TEXT_DIM);
        }
    }
    // ---- footer: all categories, source of the options, back.
    let fy = c.y + c.h - bh;
    let w3 = 18.0 * 8.0 * s;
    if text_button(&mut g.ui, &g.font, Rect::new(c.x, fy, w3, bh), "All categories on", BtnState::Normal) {
        sp.categories = ALL_STOCK_CATEGORIES.iter().map(|c| c.to_string()).collect();
        sp.filters.clear();
    }
    if text_button(&mut g.ui, &g.font, Rect::new(c.x + w3 + 6.0 * s, fy, w3, bh), "All categories off", BtnState::Normal) {
        sp.categories.clear();
        sp.filters.clear();
    }
    let src = if g.sim.stockpile_filter_options(cat).is_empty() { "detailed lists from the save's raws (stand-in)" } else { "detailed lists from the sim" };
    let n_items = g.sim.world.items.iter().filter(|it| b.contains(it.pos) && it.flags.on_ground).count();
    g.font.draw(&format!("{n_items} items stored; {} filters; {src}", sp.filters.len()), c.x + 2.0 * (w3 + 6.0 * s) + 8.0 * s, fy + 4.0 * s, s, col::TEXT_DIM);
    let br = Rect::new(c.x + c.w - 16.0 * 8.0 * s, fy, 16.0 * 8.0 * s, bh);
    if text_button(&mut g.ui, &g.font, br, "Show on map", BtnState::Normal) {
        g.screen = Screen::None;
        g.selected = crate::game::Selected::Building(pid);
        g.cam.focus(b.center);
    }
    // ---- write back
    if sp.categories != sp0.categories
        || sp.filters != sp0.filters
        || sp.max_barrels != sp0.max_barrels
        || sp.max_bins != sp0.max_bins
        || sp.max_wheelbarrows != sp0.max_wheelbarrows
        || sp.use_links_only != sp0.use_links_only
    {
        g.sim.set_stockpile_settings(pid, sp);
    }
    if let Some((id, give)) = unlink {
        remove_link(g, pid, id, give);
    }
    if let Some(give) = link_tool {
        g.tool = Tool::PileLink(pid, give);
        g.screen = Screen::None;
        g.selected = crate::game::Selected::Building(pid);
    }
}

fn group_label(g: &str) -> String {
    match g {
        "metal_ores" => "Metal ores".into(),
        "quality_core" => "Core quality".into(),
        "quality_total" => "Total quality".into(),
        "prepared_meals" => "Prepared food".into(),
        "drink_plant" => "Drink (plant)".into(),
        "drink_animal" => "Drink (animal)".into(),
        "cheese_animal" => "Cheese (animal)".into(),
        "cheese_plant" => "Cheese (plant)".into(),
        "powder_plant" => "Milled plant".into(),
        "liquid_misc" => "Liquid (misc)".into(),
        "body_parts" => "Body parts".into(),
        "hides_fresh" => "Fresh raw hide".into(),
        "hides_rotten" => "Rotten raw hide".into(),
        "bars_metal" => "Bars (metal)".into(),
        "bars_other" => "Bars (other)".into(),
        "blocks_stone" => "Blocks (stone)".into(),
        "blocks_metal" => "Blocks (metal)".into(),
        "blocks_other" => "Blocks (other)".into(),
        "rough_gems" => "Rough gems".into(),
        "rough_glass" => "Rough glass".into(),
        "cut_gems" => "Cut gems".into(),
        "cut_glass" => "Cut glass".into(),
        "cut_stone" => "Cut stone".into(),
        "trap_comps" => "Trap components".into(),
        "usable" => "Usable/unusable".into(),
        other => pretty(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Vec<(String, String)> {
        ["stone/ores/A", "stone/ores/B", "stone/other/C"].iter().map(|p| (p.to_string(), p.to_string())).collect()
    }

    #[test]
    fn leaf_toggles_keep_per_category_filters() {
        let all = opts();
        let mut sp = StockpileSettings { categories: vec!["stone".into(), "wood".into()], ..Default::default() };
        assert!(leaf_on(&sp, "stone", "stone/ores/A"));
        assert_eq!(cat_state(&sp, "stone"), 2);
        // One leaf off: the others become explicit filters.
        set_leaves(&mut sp, "stone", &all, &["stone/ores/A".into()], false);
        assert!(!leaf_on(&sp, "stone", "stone/ores/A"));
        assert!(leaf_on(&sp, "stone", "stone/ores/B"));
        assert_eq!(cat_state(&sp, "stone"), 1);
        // Wood stays "everything".
        assert_eq!(cat_state(&sp, "wood"), 2);
        // Back on: compacted to no filters.
        set_leaves(&mut sp, "stone", &all, &["stone/ores/A".into()], true);
        assert!(sp.filters.is_empty());
        assert_eq!(cat_state(&sp, "stone"), 2);
        // All off: the category goes off.
        let every: Vec<String> = all.iter().map(|(p, _)| p.clone()).collect();
        set_leaves(&mut sp, "stone", &all, &every, false);
        assert_eq!(cat_state(&sp, "stone"), 0);
        // A single leaf of an off category: only that one.
        set_leaves(&mut sp, "stone", &all, &["stone/other/C".into()], true);
        assert_eq!(cat_state(&sp, "stone"), 1);
        assert!(leaf_on(&sp, "stone", "stone/other/C") && !leaf_on(&sp, "stone", "stone/ores/B"));
        toggle_cat(&mut sp, "stone");
        assert_eq!(cat_state(&sp, "stone"), 0);
        assert!(sp.filters.is_empty());
    }

    #[test]
    fn fallback_lists_from_a_world() {
        let w = World::synthetic(24, 24, 12);
        for c in ALL_STOCK_CATEGORIES {
            for (p, _) in fallback_options(&w, c) {
                assert!(p.starts_with(&format!("{c}/")) && p.split('/').count() >= 3, "{p}");
            }
        }
        assert!(!fallback_options(&w, "furniture").is_empty());
    }
}
