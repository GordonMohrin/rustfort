//! [metal12] Reactions from the save's raws plus DF's hard-coded ashery, glass and kiln jobs.
//!
//! # DF rules implemented (vanilla raws `reaction_smelter.txt`, `reaction_other.txt`; wiki:
//! Smelter, Alloy, Steel, Pig iron, Flux, Coke, Kiln, Pottery, Glass furnace, Ashery, Lye,
//! Potash, Pearlash, Soap maker's workshop, Tallow, Magma forge)
//! - **Coke**: 1 bituminous coal boulder -> 9 coke, 1 lignite -> 5 coke (smelter, fuel).
//! - **Alloys from bars** (smelter, fuel; reagent 150 = 1 bar): bronze 1 tin + 1 copper -> 2,
//!   brass 1 zinc + 1 copper -> 2, electrum 1 silver + 1 gold -> 2, billon 1 silver + 1 copper
//!   -> 2, fine pewter 3 tin + 1 copper -> 4, trifle pewter 2 tin + 1 copper -> 3, lay pewter
//!   2 tin + 1 copper + 1 lead -> 4, nickel silver 2 nickel + 1 copper + 1 zinc -> 4, black
//!   bronze 2 copper + 1 silver + 1 gold -> 4, sterling silver 3 silver + 1 copper -> 4, rose
//!   gold 3 gold + 1 copper -> 4, bismuth bronze 1 tin + 2 copper + 1 bismuth -> 4.
//! - **Alloys from ore** (METAL_ORE reagents: any ore boulder with that METAL_ORE, e.g.
//!   tetrahedrite counts as copper and silver ore): bronze/brass/electrum/billon 1 + 1 ore -> 8
//!   bars, fine pewter 3 tin + 1 copper ore -> 16, trifle pewter 2 + 1 -> 12.
//! - **Pig iron**: 1 iron bar + 1 flux boulder (reaction class FLUX: calcite, chalk, dolomite,
//!   limestone, marble) + 1 coal bar -> 1 pig iron bar, plus fuel. **Steel**: 1 iron bar + 1 pig
//!   iron bar + 1 flux boulder + 1 coal bar -> 2 steel bars, plus fuel. The coal reagent is
//!   needed in a magma smelter too; only the fuel is saved.
//! - **Magma furnaces** (magma smelter, magma forge, magma glass furnace, magma kiln) run the
//!   jobs of their plain counterpart without fuel, but only while at least one tile directly
//!   below one of their eight non-center tiles holds magma 4/7 deep or more (wiki Magma forge).
//! - **Kiln** (fuel): clay boulders (any material with a FIRED_MAT: clay, silty clay, sandy
//!   clay, clay loam -> earthenware; fire clay -> stoneware; kaolinite -> porcelain) -> 1 jug,
//!   1 brick, 1 statue, 1 large pot or 1 craft (labor Pottery); **pearlash** 1 potash -> 1
//!   pearlash (Furnace operating).
//! - **Glass furnace** (fuel): raw clear glass from 1 bag of sand + 1 pearlash bar, raw crystal
//!   glass from 1 uncut rock crystal + 1 pearlash bar (the sand's bag stays).
//! - **Ashery** (no fuel): make lye 1 ash bar + 1 empty bucket -> lye in the bucket (Lye making);
//!   potash from lye: lye -> 1 potash bar, the bucket is emptied; potash from ash: 1 ash -> 1
//!   potash (Potash making).
//! - **Kitchen**: render fat 1 fat (reaction class FAT) -> 1 tallow of the same creature (Cook).
//! - **Soap maker's workshop**: 1 lye + 1 tallow -> 1 soap bar of the tallow's creature (the
//!   lye's bucket stays; Soap making). Doctors clean wounds with soap (`health.rs`).
//!
//! # Estimates
//! - Work times 300-400 ticks before skill; products are one stack per job (the sim's bars are
//!   stacks, DF makes single bars).
//! - The magma check does not cancel a job already being worked (DF cancels all jobs the moment
//!   the magma falls below 4/7); the forge simply stops taking jobs.
//! - Glazing, plaster, milk of lime, dyeing and the screw press are not implemented.

use super::CRAFT_KINDS;
use crate::production::{Recipe, WJob};
use crate::rng::Rng;
use crate::Sim;
use rf_core::{Item, ItemFlags, MaterialInfo, NO_MAT};

/// One reagent of a reaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rg {
    /// Bars of `INORGANIC:<metal>`, count (raw quantity / 150).
    Bars(&'static str, i32),
    /// Ore boulders with `METAL_ORE:<metal>`, count.
    Ore(&'static str, i32),
    /// A boulder of exactly `INORGANIC:<token>`.
    Stone(&'static str),
    /// A boulder of a stone with reaction class FLUX.
    Flux,
    /// A coal bar (charcoal or coke) as a reagent, besides any fuel.
    Coal,
    /// A boulder of a material with a FIRED_MAT (clays, kaolinite).
    Clay,
    /// A unit of unrotten fat (reaction class FAT).
    Fat,
    /// A unit of tallow (reaction class TALLOW).
    Tallow,
    Ash,
    Potash,
    Pearlash,
    /// A unit of lye (its bucket stays where it is).
    Lye,
    /// An empty bucket that receives the product.
    EmptyBucket,
    /// A bag of sand (the sand is used, the bag stays).
    Sand,
    /// An uncut rock crystal.
    Crystal,
}

/// The product of a reaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pr {
    /// `n` bars of `INORGANIC:<metal>`, or of coke for `COAL:COKE`.
    Bars(&'static str, i32),
    /// One item (kind, subtype, noun) of the clay reagent's fired material; kind `CRAFTS` = a
    /// random craft.
    Fired(&'static str, &'static str, &'static str),
    Tallow,
    Soap,
    Potash,
    Pearlash,
    Lye,
    /// Raw glass of this builtin glass material.
    Glass(&'static str, &'static str),
}

pub(crate) struct Reaction {
    /// Raw reaction token, or the DF job name / glass material for hard-coded jobs.
    pub key: &'static str,
    pub reagents: &'static [Rg],
    pub product: Pr,
}

use Rg::*;

/// The reactions (vanilla raws; checked against the install by `reaction_table_matches_raws`).
pub(crate) const REACTIONS: &[Reaction] = &[
    Reaction { key: "BITUMINOUS_COAL_TO_COKE", reagents: &[Stone("COAL_BITUMINOUS")], product: Pr::Bars("COAL:COKE", 9) },
    Reaction { key: "LIGNITE_TO_COKE", reagents: &[Stone("LIGNITE")], product: Pr::Bars("COAL:COKE", 5) },
    Reaction { key: "BRASS_MAKING", reagents: &[Ore("ZINC", 1), Ore("COPPER", 1)], product: Pr::Bars("BRASS", 8) },
    Reaction { key: "BRASS_MAKING2", reagents: &[Bars("ZINC", 1), Bars("COPPER", 1)], product: Pr::Bars("BRASS", 2) },
    Reaction { key: "BRONZE_MAKING", reagents: &[Ore("TIN", 1), Ore("COPPER", 1)], product: Pr::Bars("BRONZE", 8) },
    Reaction { key: "BRONZE_MAKING2", reagents: &[Bars("TIN", 1), Bars("COPPER", 1)], product: Pr::Bars("BRONZE", 2) },
    Reaction { key: "ELECTRUM_MAKING", reagents: &[Ore("SILVER", 1), Ore("GOLD", 1)], product: Pr::Bars("ELECTRUM", 8) },
    Reaction { key: "ELECTRUM_MAKING2", reagents: &[Bars("SILVER", 1), Bars("GOLD", 1)], product: Pr::Bars("ELECTRUM", 2) },
    Reaction { key: "BILLON_MAKING", reagents: &[Ore("SILVER", 1), Ore("COPPER", 1)], product: Pr::Bars("BILLON", 8) },
    Reaction { key: "BILLON_MAKING2", reagents: &[Bars("SILVER", 1), Bars("COPPER", 1)], product: Pr::Bars("BILLON", 2) },
    Reaction { key: "PEWTER_FINE_MAKING", reagents: &[Ore("TIN", 3), Ore("COPPER", 1)], product: Pr::Bars("PEWTER_FINE", 16) },
    Reaction { key: "PEWTER_FINE_MAKING2", reagents: &[Bars("TIN", 3), Bars("COPPER", 1)], product: Pr::Bars("PEWTER_FINE", 4) },
    Reaction { key: "PEWTER_TRIFLE_MAKING", reagents: &[Ore("TIN", 2), Ore("COPPER", 1)], product: Pr::Bars("PEWTER_TRIFLE", 12) },
    Reaction { key: "PEWTER_TRIFLE_MAKING2", reagents: &[Bars("TIN", 2), Bars("COPPER", 1)], product: Pr::Bars("PEWTER_TRIFLE", 3) },
    Reaction { key: "PEWTER_LAY_MAKING", reagents: &[Bars("TIN", 2), Bars("COPPER", 1), Bars("LEAD", 1)], product: Pr::Bars("PEWTER_LAY", 4) },
    Reaction { key: "PIG_IRON_MAKING", reagents: &[Bars("IRON", 1), Flux, Coal], product: Pr::Bars("PIG_IRON", 1) },
    Reaction { key: "STEEL_MAKING", reagents: &[Bars("IRON", 1), Bars("PIG_IRON", 1), Flux, Coal], product: Pr::Bars("STEEL", 2) },
    Reaction { key: "NICKEL_SILVER_MAKING", reagents: &[Bars("NICKEL", 2), Bars("COPPER", 1), Bars("ZINC", 1)], product: Pr::Bars("NICKEL_SILVER", 4) },
    Reaction { key: "BLACK_BRONZE_MAKING", reagents: &[Bars("COPPER", 2), Bars("SILVER", 1), Bars("GOLD", 1)], product: Pr::Bars("BLACK_BRONZE", 4) },
    Reaction { key: "STERLING_SILVER_MAKING", reagents: &[Bars("SILVER", 3), Bars("COPPER", 1)], product: Pr::Bars("STERLING_SILVER", 4) },
    Reaction { key: "ROSE_GOLD_MAKING", reagents: &[Bars("GOLD", 3), Bars("COPPER", 1)], product: Pr::Bars("ROSE_GOLD", 4) },
    Reaction { key: "BISMUTH_BRONZE_MAKING", reagents: &[Bars("TIN", 1), Bars("COPPER", 2), Bars("BISMUTH", 1)], product: Pr::Bars("BISMUTH_BRONZE", 4) },
    // reaction_other.txt
    Reaction { key: "RENDER_FAT", reagents: &[Fat], product: Pr::Tallow },
    Reaction { key: "MAKE_SOAP_FROM_TALLOW", reagents: &[Lye, Tallow], product: Pr::Soap },
    Reaction { key: "MAKE_PEARLASH", reagents: &[Potash], product: Pr::Pearlash },
    Reaction { key: "MAKE_CLAY_JUG", reagents: &[Clay], product: Pr::Fired("TOOL", "ITEM_TOOL_JUG", "jug") },
    Reaction { key: "MAKE_CLAY_BRICKS", reagents: &[Clay], product: Pr::Fired("BLOCKS", "", "bricks") },
    Reaction { key: "MAKE_CLAY_STATUE", reagents: &[Clay], product: Pr::Fired("STATUE", "", "statue") },
    Reaction { key: "MAKE_LARGE_CLAY_POT", reagents: &[Clay], product: Pr::Fired("TOOL", "ITEM_TOOL_LARGE_POT", "large pot") },
    Reaction { key: "MAKE_CLAY_CRAFTS", reagents: &[Clay], product: Pr::Fired("CRAFTS", "", "craft") },
    // Hard-coded DF jobs.
    Reaction { key: "MakeLye", reagents: &[Ash, EmptyBucket], product: Pr::Lye },
    Reaction { key: "MakePotashFromLye", reagents: &[Lye], product: Pr::Potash },
    Reaction { key: "MakePotashFromAsh", reagents: &[Ash], product: Pr::Potash },
    Reaction { key: "GLASS_CLEAR", reagents: &[Sand, Pearlash], product: Pr::Glass("GLASS_CLEAR", "clear glass") },
    Reaction { key: "GLASS_CRYSTAL", reagents: &[Crystal, Pearlash], product: Pr::Glass("GLASS_CRYSTAL", "crystal glass") },
];

/// The reaction a recipe row performs.
pub(crate) fn reaction_of(rec: &Recipe) -> Option<&'static Reaction> {
    let key = if rec.reaction.is_empty() { rec.job } else { rec.reaction };
    REACTIONS.iter().find(|x| x.key == key)
}

/// Fired material of a clay (vanilla `FIRED_MAT` reaction products): token, name, value.
fn fired_of(token: &str) -> Option<(&'static str, &'static str, i32)> {
    match token.strip_prefix("INORGANIC:")? {
        "CLAY" | "SILTY_CLAY" | "SANDY_CLAY" | "CLAY_LOAM" => Some(("INORGANIC:CERAMIC_EARTHENWARE", "earthenware", 3)),
        "FIRE_CLAY" => Some(("INORGANIC:CERAMIC_STONEWARE", "stoneware", 4)),
        "KAOLINITE" => Some(("INORGANIC:CERAMIC_PORCELAIN", "porcelain", 10)),
        _ => None,
    }
}

/// Free reagent counts of the reactions (part of the industry census).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct RxCensus {
    /// Free metal bar units by material.
    pub bars: Vec<(u16, i32)>,
    /// Free ore boulders by the metal they yield.
    pub ore: Vec<(u16, i32)>,
    /// Free boulders of the reaction stones (coal) by material.
    pub stone: Vec<(u16, i32)>,
    pub flux: i32,
    pub clay: i32,
    pub fat: i32,
    pub tallow: i32,
    pub ash: i32,
    pub potash: i32,
    pub pearlash: i32,
    pub lye: i32,
    pub buckets: i32,
    pub crystal: i32,
}

fn add(v: &mut Vec<(u16, i32)>, m: u16, n: i32) {
    match v.iter_mut().find(|e| e.0 == m) {
        Some(e) => e.1 += n,
        None => v.push((m, n)),
    }
}

fn get(v: &[(u16, i32)], m: u16) -> i32 {
    v.iter().find(|e| e.0 == m).map(|e| e.1).unwrap_or(0)
}

fn has_tag(m: &MaterialInfo, tag: &str) -> bool {
    m.tags.iter().any(|t| t == tag)
}

impl Sim {
    /// Palette index of a token (`NO_MAT` if the fort has none).
    fn rx_find_mat(&self, token: &str) -> u16 {
        self.world.materials.iter().position(|m| m.token == token).map(|i| i as u16).unwrap_or(NO_MAT)
    }

    /// The palette index of `INORGANIC:<metal>` (or `COAL:COKE`), created if missing.
    fn rx_metal(&mut self, metal: &str) -> u16 {
        let tok = if metal.contains(':') { metal.to_string() } else { format!("INORGANIC:{metal}") };
        let i = self.rx_find_mat(&tok);
        if i != NO_MAT {
            return i;
        }
        let coal = tok.starts_with("COAL:");
        let name = if coal { "coke".to_string() } else { metal.to_lowercase().replace('_', " ") };
        let m = self.ensure_mat(&tok, &name, if coal { [40, 40, 40] } else { [160, 160, 170] }, |m| m.is_metal = !coal);
        self.prod.mat_plant.resize(self.world.materials.len(), u32::MAX);
        m
    }

    fn rx_metal_known(&self, metal: &str) -> u16 {
        if metal.contains(':') {
            self.rx_find_mat(metal)
        } else {
            self.rx_find_mat(&format!("INORGANIC:{metal}"))
        }
    }

    fn rx_is_fat(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "GLOB" && !it.flags.rotten && self.world.material(it.mat).map(|m| has_tag(m, "RC:FAT") || m.token.ends_with(":FAT")).unwrap_or(false)
    }

    fn rx_is_tallow(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "GLOB" && !it.flags.rotten && self.world.material(it.mat).map(|m| has_tag(m, "RC:TALLOW") || m.token.ends_with(":TALLOW")).unwrap_or(false)
    }

    fn rx_bar_of(&self, idx: usize, token: &str) -> bool {
        let it = &self.world.items[idx];
        it.kind == "BAR" && self.world.material(it.mat).map(|m| m.token == token).unwrap_or(false)
    }

    fn rx_is_lye(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "LIQUID_MISC" && self.world.material(it.mat).map(|m| m.token == "LYE").unwrap_or(false)
    }

    fn rx_is_flux(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "BOULDER" && self.world.material(it.mat).map(|m| has_tag(m, "RC:FLUX")).unwrap_or(false)
    }

    fn rx_is_clay(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "BOULDER" && self.world.material(it.mat).map(|m| fired_of(&m.token).is_some()).unwrap_or(false)
    }

    fn rx_is_crystal(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "ROUGH" && self.world.material(it.mat).map(|m| m.token == "INORGANIC:CRYSTAL_ROCK").unwrap_or(false)
    }

    fn rx_is_empty_bucket(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "BUCKET" && it.container < 0 && self.container_empty(it.id)
    }

    /// [metal12] Count one item into the reaction census (`free` = not in use).
    pub(crate) fn rx_census_item(&self, c: &mut RxCensus, idx: usize, now: u64) {
        let it = &self.world.items[idx];
        let k = it.kind.as_str();
        if !matches!(k, "BAR" | "BOULDER" | "GLOB" | "LIQUID_MISC" | "BUCKET" | "ROUGH") || it.flags.artifact {
            return;
        }
        if !self.item_free(idx, now) {
            return;
        }
        let units = (it.stack - self.imeta[idx].reserved).max(0);
        let Some(m) = self.world.material(it.mat) else { return };
        match k {
            "BAR" => {
                if m.is_metal {
                    add(&mut c.bars, it.mat, units);
                } else if m.token == "ASH" {
                    c.ash += units;
                } else if m.token == "POTASH" {
                    c.potash += units;
                } else if m.token == "PEARLASH" {
                    c.pearlash += units;
                }
            }
            "BOULDER" => {
                if let Some(metals) = self.ind.ore_metals.get(&it.mat) {
                    for &(mm, _) in metals {
                        add(&mut c.ore, mm, 1);
                    }
                } else if has_tag(m, "RC:FLUX") {
                    c.flux += 1;
                } else if fired_of(&m.token).is_some() {
                    c.clay += 1;
                } else if m.token == "INORGANIC:COAL_BITUMINOUS" || m.token == "INORGANIC:LIGNITE" {
                    add(&mut c.stone, it.mat, 1);
                }
            }
            "GLOB" => {
                if self.rx_is_fat(idx) {
                    c.fat += units;
                } else if self.rx_is_tallow(idx) {
                    c.tallow += units;
                }
            }
            "LIQUID_MISC" => c.lye += self.rx_is_lye(idx) as i32 * units,
            "BUCKET" => c.buckets += self.rx_is_empty_bucket(idx) as i32,
            _ => c.crystal += self.rx_is_crystal(idx) as i32,
        }
    }

    /// [metal12] Are the reagents of a reaction recipe in stock (`magma`: no fuel needed)?
    pub(crate) fn rx_available(&self, rec: &Recipe, magma: bool) -> bool {
        let Some(rx) = reaction_of(rec) else { return false };
        let c = &self.ind.census.rx;
        let mut coal = if !magma && self.needs_fuel(rec) { 1 } else { 0 };
        for g in rx.reagents {
            let ok = match *g {
                Bars(m, n) => {
                    let mi = self.rx_metal_known(m);
                    mi != NO_MAT && get(&c.bars, mi) >= n
                }
                Ore(m, n) => {
                    let mi = self.rx_metal_known(m);
                    mi != NO_MAT && get(&c.ore, mi) >= n
                }
                Stone(t) => {
                    let mi = self.rx_find_mat(&format!("INORGANIC:{t}"));
                    mi != NO_MAT && get(&c.stone, mi) > 0
                }
                Flux => c.flux > 0,
                Coal => {
                    coal += 1;
                    true
                }
                Clay => c.clay > 0,
                Fat => c.fat > 0,
                Tallow => c.tallow > 0,
                Ash => c.ash > 0,
                Potash => c.potash > 0,
                Pearlash => c.pearlash > 0,
                Lye => c.lye > 0,
                EmptyBucket => c.buckets > 0,
                Sand => self.ind.census.sand > 0,
                Crystal => c.crystal > 0,
            };
            if !ok {
                return false;
            }
        }
        coal == 0 || self.ind.census.fuel >= coal
    }

    /// Nearest free items matching `pred` (any production class), like `find_items`.
    fn rx_find(&mut self, site: u32, c: u32, now: u64, want: usize, out: &mut Vec<i32>, pred: impl Fn(&Sim, usize) -> bool) {
        self.stats.prod_item_scans += 1;
        out.clear();
        let sp = self.nav.xyz(site as usize);
        let mut best: Vec<(i32, i32)> = Vec::new();
        for idx in 0..self.world.items.len() {
            if self.imeta[idx].dead || !pred(self, idx) || !self.item_free(idx, now) {
                continue;
            }
            let it = &self.world.items[idx];
            let Some(t) = self.world.idx(it.pos) else { continue };
            let d = crate::manhattan(sp, self.nav.xyz(t));
            if best.len() == want && d >= best[want - 1].0 {
                continue;
            }
            if self.nav.comp_near(t) != c || self.alert_blocks_cur(t) {
                continue;
            }
            let pos = best.partition_point(|e| *e <= (d, it.id));
            best.insert(pos, (d, it.id));
            best.truncate(want);
        }
        out.extend(best.iter().map(|e| e.1));
    }

    /// Pick `units` units of items matching `pred` into `wj` (several stacks if needed).
    fn rx_pick(&mut self, site: u32, c: u32, now: u64, units: i32, wj: &mut WJob, pred: impl Fn(&Sim, usize) -> bool) -> bool {
        let slots = wj.items.len() - wj.n as usize;
        if slots == 0 {
            return false;
        }
        let (taken, nt) = (wj.items, wj.n as usize);
        let mut found = Vec::new();
        self.rx_find(site, c, now, slots.min(units.max(1) as usize), &mut found, |sim, i| pred(sim, i) && !taken[..nt].contains(&sim.world.items[i].id));
        let mut left = units;
        let mut picked = Vec::new();
        for &id in &found {
            if left <= 0 {
                break;
            }
            let idx = self.item_idx[&id] as usize;
            let take = (self.world.items[idx].stack - self.imeta[idx].reserved).min(left);
            if take > 0 {
                picked.push((id, take));
                left -= take;
            }
        }
        if left > 0 || picked.len() > slots {
            return false;
        }
        for (id, take) in picked {
            let k = wj.n as usize;
            wj.items[k] = id;
            wj.need[k] = take;
            wj.n += 1;
        }
        true
    }

    /// [metal12] Choose the reagents of a reaction (the fuel is picked by `ind_claim`).
    pub(crate) fn rx_claim(&mut self, rec: &Recipe, site: u32, c: u32, now: u64, wj: &mut WJob) -> bool {
        let Some(rx) = reaction_of(rec) else { return false };
        for g in rx.reagents {
            let ok = match *g {
                Bars(m, n) => {
                    let mi = self.rx_metal_known(m);
                    mi != NO_MAT && self.rx_pick(site, c, now, n, wj, move |sim, i| sim.world.items[i].kind == "BAR" && sim.world.items[i].mat == mi)
                }
                Ore(m, n) => {
                    let mi = self.rx_metal_known(m);
                    mi != NO_MAT
                        && self.rx_pick(site, c, now, n, wj, move |sim, i| {
                            sim.world.items[i].kind == "BOULDER" && sim.ind.ore_metals.get(&sim.world.items[i].mat).map(|v| v.iter().any(|e| e.0 == mi)).unwrap_or(false)
                        })
                }
                Stone(t) => {
                    let mi = self.rx_find_mat(&format!("INORGANIC:{t}"));
                    mi != NO_MAT && self.rx_pick(site, c, now, 1, wj, move |sim, i| sim.world.items[i].kind == "BOULDER" && sim.world.items[i].mat == mi)
                }
                Flux => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_is_flux(i)),
                Coal => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.is_fuel(i)),
                Clay => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_is_clay(i)),
                Fat => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_is_fat(i)),
                Tallow => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_is_tallow(i)),
                Ash => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_bar_of(i, "ASH")),
                Potash => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_bar_of(i, "POTASH")),
                Pearlash => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_bar_of(i, "PEARLASH")),
                Lye => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_is_lye(i)),
                EmptyBucket => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_is_empty_bucket(i)),
                Sand => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.is_sand(i)),
                Crystal => self.rx_pick(site, c, now, 1, wj, |sim, i| sim.rx_is_crystal(i)),
            };
            if !ok {
                return false;
            }
        }
        true
    }

    /// Material derived from a creature material token (`CREATURE:X:FAT` -> `CREATURE:X:TALLOW`).
    fn rx_derived(&mut self, from: u16, old: &str, new: &str, tag: &str, rgb: [u8; 3]) -> u16 {
        let (tok, name) = match self.world.material(from) {
            Some(m) => {
                let base = m.token.rsplit_once(':').map(|(b, _)| b.to_string()).unwrap_or_else(|| m.token.clone());
                let n = m.name.replace(old, "").trim().to_string();
                (format!("{base}:{}", new.to_uppercase()), if n.is_empty() { new.to_string() } else { format!("{n} {new}") })
            }
            None => (format!("CREATURE:UNKNOWN:{}", new.to_uppercase()), new.to_string()),
        };
        let tag = tag.to_string();
        let m = self.ensure_mat(&tok, &name, rgb, move |m| m.tags.push(tag));
        self.prod.mat_plant.resize(self.world.materials.len(), u32::MAX);
        m
    }

    /// [metal12] Make the product of a finished reaction; `keep` gets reagents that stay.
    pub(crate) fn rx_finish(&mut self, wj: &WJob, rec: &Recipe, site: usize, keep: &mut Vec<i32>, r: &mut Rng) {
        let Some(rx) = reaction_of(rec) else { return };
        // Reagent i of the reaction is wj.items[i] when every reagent took one stack (the
        // usual case); look the special ones up by kind instead.
        let item = |sim: &Sim, pred: &dyn Fn(&Sim, usize) -> bool| -> Option<usize> {
            (0..wj.n as usize).filter_map(|k| sim.item_idx.get(&wj.items[k]).map(|&i| i as usize)).find(|&i| pred(sim, i))
        };
        match rx.product {
            Pr::Bars(metal, n) => {
                let m = self.rx_metal(metal);
                let name = if metal.starts_with("COAL:") { "coke".to_string() } else { format!("{} bars", self.mat_name(m)) };
                self.ind_make("BAR", "", m, site, n, name);
            }
            Pr::Fired(kind, sub, noun) => {
                let Some(ci) = item(self, &|sim, i| sim.rx_is_clay(i)) else { return };
                let tok = self.world.material(self.world.items[ci].mat).map(|m| m.token.clone()).unwrap_or_default();
                let Some((ftok, fname, value)) = fired_of(&tok) else { return };
                let fm = self.ensure_mat(ftok, fname, [190, 110, 70], |m| {
                    m.is_stone = true;
                    m.value = value;
                });
                self.prod.mat_plant.resize(self.world.materials.len(), u32::MAX);
                let kind = if kind == "CRAFTS" { CRAFT_KINDS[r.below(CRAFT_KINDS.len() as u32) as usize] } else { kind };
                let noun = if kind != "CRAFTS" && noun == "craft" { kind.to_lowercase() } else { noun.to_string() };
                self.ind_make(kind, sub, fm, site, 1, format!("{fname} {noun}"));
            }
            Pr::Tallow => {
                let Some(fi) = item(self, &|sim, i| sim.rx_is_fat(i)) else { return };
                let fm = self.world.items[fi].mat;
                let m = self.rx_derived(fm, "fat", "tallow", "RC:TALLOW", [240, 230, 180]);
                let name = format!("{}", self.mat_name(m));
                self.ind_make("GLOB", "", m, site, 1, name);
            }
            Pr::Soap => {
                let Some(ti) = item(self, &|sim, i| sim.rx_is_tallow(i)) else { return };
                let tm = self.world.items[ti].mat;
                let m = self.rx_derived(tm, "tallow", "soap", "SOAP", [240, 240, 220]);
                let name = format!("{} bar", self.mat_name(m));
                self.ind_make("BAR", "", m, site, 1, name);
            }
            Pr::Potash | Pr::Pearlash => {
                let (tok, name) = if rx.product == Pr::Potash { ("POTASH", "potash") } else { ("PEARLASH", "pearlash") };
                let m = self.ensure_mat(tok, name, [220, 220, 220], |_| {});
                self.prod.mat_plant.resize(self.world.materials.len(), u32::MAX);
                self.ind_make("BAR", "", m, site, 1, name.into());
            }
            Pr::Lye => {
                let Some(bi) = item(self, &|sim, i| sim.world.items[i].kind == "BUCKET") else { return };
                let bucket = self.world.items[bi].id;
                keep.push(bucket);
                let m = self.ensure_mat("LYE", "lye", [200, 200, 160], |_| {});
                self.prod.mat_plant.resize(self.world.materials.len(), u32::MAX);
                self.rx_fill(bucket, "LIQUID_MISC", m, "lye".into());
                self.count_made("LIQUID_MISC", 1);
            }
            Pr::Glass(tok, name) => {
                let m = self.ensure_mat(tok, name, [200, 230, 230], |m| m.is_glass = true);
                self.prod.mat_plant.resize(self.world.materials.len(), u32::MAX);
                self.ind_make("ROUGH", "", m, site, 1, format!("raw {name}"));
            }
        }
    }

    /// A new item inside container `cont` (lye into its bucket).
    fn rx_fill(&mut self, cont: i32, kind: &str, mat: u16, desc: String) {
        let Some(&ci) = self.item_idx.get(&cont) else { return };
        let pos = self.world.items[ci as usize].pos;
        let id = self.next_item_id;
        self.next_item_id += 1;
        self.push_item(Item {
            id,
            kind: kind.into(),
            mat,
            pos,
            stack: 1,
            container: cont,
            holder: -1,
            building: -1,
            desc,
            flags: ItemFlags { in_container: true, ..Default::default() },
            ..Default::default()
        });
        self.contents.entry(cont).or_default().push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is a transcription of the vanilla raws: check it against the DF install when
    /// it is there (`RUSTFORT_DF_DIR` or the default Steam path); skipped otherwise.
    #[test]
    fn reaction_table_matches_raws() {
        let dir = std::env::var("RUSTFORT_DF_DIR").unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress".into());
        let p = std::path::Path::new(&dir).join("data/vanilla/vanilla_reactions/objects/reaction_smelter.txt");
        let Ok(text) = std::fs::read_to_string(&p) else { return };
        let mut checked = 0;
        for block in text.split("[REACTION:").skip(1) {
            let key = block.split(']').next().unwrap_or("");
            let Some(rx) = REACTIONS.iter().find(|r| r.key == key) else { continue };
            let mut reag: Vec<String> = Vec::new();
            let mut prod = String::new();
            for tok in block.split('[').skip(1) {
                let tok = tok.split(']').next().unwrap_or("");
                if let Some(r) = tok.strip_prefix("REAGENT:") {
                    reag.push(r.to_string());
                } else if let Some(p) = tok.strip_prefix("PRODUCT:") {
                    prod = p.to_string();
                }
            }
            let want: Vec<String> = rx
                .reagents
                .iter()
                .map(|g| match *g {
                    Bars(m, n) => format!("{}:BAR:NO_SUBTYPE:METAL:{m}", n * 150),
                    Ore(m, n) => format!("{n}:METAL_ORE:{m}"),
                    Stone(t) => format!("1:BOULDER:NO_SUBTYPE:INORGANIC:{t}"),
                    Flux => "1:BOULDER:NO_SUBTYPE:NONE:NONE".into(),
                    Coal => "150:BAR:NO_SUBTYPE:COAL:NO_MATGLOSS".into(),
                    _ => String::new(),
                })
                .collect();
            let got: Vec<String> = reag.iter().map(|r| r.splitn(2, ':').nth(1).unwrap_or("").to_string()).collect();
            assert_eq!(got, want, "{key}");
            let Pr::Bars(m, n) = rx.product else { panic!("{key}") };
            let tail = if m.starts_with("COAL:") { format!("BAR:NO_SUBTYPE:{m}") } else { format!("BAR:NO_SUBTYPE:METAL:{m}") };
            assert_eq!(prod, format!("100:{n}:{tail}"), "{key}");
            checked += 1;
        }
        assert!(checked >= 20, "only {checked} reactions checked");
    }
}
