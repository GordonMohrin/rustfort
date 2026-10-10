//! Industry chains: wood furnace, smelter, metalsmith's forge, glass furnace, quern/millstone,
//! butcher, tanner, leather works, loom, clothier and craftsdwarf crafts.
//!
//! The recipe rows live in `production::RECIPES` (marked `[industry]`) with `Input::Ind(F_*)`;
//! this module picks the reagents and makes the products.
//!
//! # DF rules implemented (wiki: Wood furnace, Smelter, Ore, Metalsmith's forge, Fuel,
//! Glass furnace, Quern, Butcher's shop, Tanner's shop, Leather works, Loom, Clothier's shop,
//! Craftsdwarf's workshop, Item token MATERIAL_SIZE)
//! - **Make charcoal / ash**: 1 log -> 1 charcoal bar / 1 ash bar (labor Wood burning).
//! - **Smelt ore**: 1 ore boulder + 1 fuel -> 4 bars of each metal the ore yields, rolled with
//!   the ore's METAL_ORE probability (vanilla raws: tetrahedrite copper 100 / silver 20,
//!   galena lead 100 / silver 50, ...). Fuel is 1 charcoal or coke bar; magma furnaces need none.
//! - **Metalsmith's forge**: bars of the ordered metal + 1 fuel -> item of the ordered subtype.
//!   Bars needed = MATERIAL_SIZE / 3 (at least 1), MATERIAL_SIZE from the vanilla item raws
//!   (pick 4 -> 1 bar, breastplate 9 -> 3, greaves 6 -> 2, gauntlets 2 -> 1). Gloves and shoes
//!   come in pairs (2 items), bolts in stacks of 25. Labors per item class: Weaponsmithing,
//!   Armoring, Blacksmithing, Metal crafting.
//! - **Make raw green glass**: 1 bag of sand + 1 fuel -> 1 raw green glass (bag stays).
//! - **Mill plants**: 1 millable plant + 1 empty bag -> its flour/dye powder in the bag.
//! - **Butcher**: a dead animal (not a sapient's corpse) -> meat, fat, bones, skull and skin;
//!   a skeleton gives only bones and the skull (wiki "Butcher's shop").
//! - **Tan a hide**: 1 raw skin -> 1 leather. **Leather works / clothier**: 1 leather / 1 cloth ->
//!   bag, armor, gloves (pair), shoes (pair), trousers, caps/hoods.
//! - **Loom**: weave 1 thread -> 1 cloth (webs must be collected first: "Collect webs" turns
//!   a web into thread at the loom).
//! - **Crafts**: 1 boulder / log / bone / bar -> 1 craft of a random kind (figurine, amulet,
//!   scepter, crown, ring, earring, bracelet). Rock mugs from 1 boulder.
//! - **Engrave memorial slab**: an unengraved slab becomes engraved (Mason's workshop, as in
//!   the real fort's queue).
//! - **Melt a metal object** (smelter, labor Furnace operating, wiki "Melt item"): the nearest
//!   item designated for melting (`ItemFlags::melt`, metal, no artifact) + 1 fuel -> metal in
//!   tenths of a bar ([`melt_tenths`]: 0.3 x MATERIAL_SIZE for weapons/armor/trap parts, 1 bar
//!   for furniture, 0.5 blocks, 0.2 goblets, 0.3 per 25 bolts, 1.1 per 500 coins). Tenths
//!   accumulate per smelter and metal; every 10/10 becomes a bar. Decorations are not
//!   recovered.
//!
//! # Estimates (not on the wiki)
//! - Work times (300-600 ticks before skill) and butchering yields: meat = body size / 10 000
//!   cm³, fat / 40 000, bones / 20 000 (all at least 1) plus 1 skull and 1 skin; the body size
//!   comes from the exported creature raws (castes), 50 000 cm³ if unknown. Organs are counted
//!   as meat.
//! - 1 bar per metal craft, goblet, bucket, cage, anvil and chain.
//! - Flour material is `PLANT:<id>:MILL` named "<plant> flour" (DF names vary: flour, sugar,
//!   dye).
//! - Fuel and bars are not reserved across queued jobs: a job that cannot find its metal blocks
//!   its workshop for 300 ticks.

use crate::production::{pc, Recipe, WJob, RECIPES};
use crate::rng::Rng;
use crate::Sim;
use rf_core::{BuildingJob, Item, ItemFlags, MaterialInfo, NO_MAT};
use std::collections::HashMap;

pub const F_CHARCOAL: u8 = 1;
pub const F_ASH: u8 = 2;
pub const F_SMELT: u8 = 3;
pub const F_FORGE: u8 = 4;
pub const F_RAWGLASS: u8 = 5;
pub const F_MILL: u8 = 6;
pub const F_BUTCHER: u8 = 7;
pub const F_TAN: u8 = 8;
pub const F_LEATHER: u8 = 9;
pub const F_CLOTH_ITEM: u8 = 10;
pub const F_WEAVE: u8 = 11;
pub const F_WEBS: u8 = 12;
pub const F_CRAFTS_STONE: u8 = 13;
pub const F_CRAFTS_WOOD: u8 = 14;
pub const F_CRAFTS_BONE: u8 = 15;
pub const F_CRAFTS_METAL: u8 = 16;
pub const F_ENGRAVE: u8 = 17;
pub const F_WOOD_ITEM: u8 = 18;
/// [audit-eco] "Melt a metal object" at a smelter (wiki "Melt item").
pub const F_MELT: u8 = 19;

const CRAFT_KINDS: [&str; 7] = ["FIGURINE", "AMULET", "SCEPTER", "CROWN", "RING", "EARRING", "BRACELET"];

/// Vanilla metal tokens (for telling metal from stone in job materials).
const METALS: &[&str] = &[
    "IRON", "PIG_IRON", "STEEL", "BRONZE", "BISMUTH_BRONZE", "BLACK_BRONZE", "BRASS", "COPPER", "SILVER", "GOLD", "TIN", "ZINC", "NICKEL", "LEAD",
    "PLATINUM", "ALUMINUM", "BISMUTH", "ELECTRUM", "ROSE_GOLD", "STERLING_SILVER", "BILLON", "NICKEL_SILVER", "PEWTER_FINE", "PEWTER_TRIFLE",
    "PEWTER_LAY", "ADAMANTINE", "DIVINE_1",
];

/// Vanilla METAL_ORE tokens (inorganic_stone_mineral.txt): ore -> (metal, probability %).
const ORES: &[(&str, &[(&str, u8)])] = &[
    ("HEMATITE", &[("IRON", 100)]),
    ("LIMONITE", &[("IRON", 100)]),
    ("MAGNETITE", &[("IRON", 100)]),
    ("GARNIERITE", &[("NICKEL", 100)]),
    ("NATIVE_GOLD", &[("GOLD", 100)]),
    ("NATIVE_SILVER", &[("SILVER", 100)]),
    ("HORN_SILVER", &[("SILVER", 100)]),
    ("NATIVE_COPPER", &[("COPPER", 100)]),
    ("MALACHITE", &[("COPPER", 100)]),
    ("GALENA", &[("LEAD", 100), ("SILVER", 50)]),
    ("SPHALERITE", &[("ZINC", 100)]),
    ("CASSITERITE", &[("TIN", 100)]),
    ("NATIVE_PLATINUM", &[("PLATINUM", 100)]),
    ("TETRAHEDRITE", &[("COPPER", 100), ("SILVER", 20)]),
    ("BISMUTHINITE", &[("BISMUTH", 100)]),
    ("NATIVE_ALUMINUM", &[("ALUMINUM", 100)]),
];

/// Vanilla MATERIAL_SIZE of forgeable items (item_*.txt).
const MATERIAL_SIZE: &[(&str, i32)] = &[
    ("ITEM_WEAPON_WHIP", 1),
    ("ITEM_WEAPON_AXE_BATTLE", 4),
    ("ITEM_WEAPON_HAMMER_WAR", 3),
    ("ITEM_WEAPON_SWORD_SHORT", 3),
    ("ITEM_WEAPON_SPEAR", 3),
    ("ITEM_WEAPON_MACE", 3),
    ("ITEM_WEAPON_CROSSBOW", 3),
    ("ITEM_WEAPON_PICK", 4),
    ("ITEM_WEAPON_PIKE", 4),
    ("ITEM_WEAPON_HALBERD", 5),
    ("ITEM_WEAPON_SWORD_2H", 5),
    ("ITEM_WEAPON_SWORD_LONG", 4),
    ("ITEM_WEAPON_MAUL", 5),
    ("ITEM_WEAPON_AXE_GREAT", 5),
    ("ITEM_WEAPON_DAGGER_LARGE", 1),
    ("ITEM_WEAPON_SCOURGE", 2),
    ("ITEM_WEAPON_FLAIL", 4),
    ("ITEM_WEAPON_MORNINGSTAR", 3),
    ("ITEM_WEAPON_SCIMITAR", 3),
    ("ITEM_WEAPON_PICK_GREAT", 12),
    ("ITEM_ARMOR_BREASTPLATE", 9),
    ("ITEM_ARMOR_MAIL_SHIRT", 6),
    ("ITEM_GLOVES_GAUNTLETS", 2),
    ("ITEM_HELM_HELM", 2),
    ("ITEM_HELM_CAP", 1),
    ("ITEM_PANTS_GREAVES", 6),
    ("ITEM_PANTS_LEGGINGS", 5),
    ("ITEM_SHOES_BOOTS", 2),
    ("ITEM_SHOES_BOOTS_LOW", 1),
    ("ITEM_SHIELD_SHIELD", 4),
    ("ITEM_SHIELD_BUCKLER", 2),
    ("ITEM_TRAPCOMP_GIANTAXEBLADE", 5),
    ("ITEM_TRAPCOMP_ENORMOUSCORKSCREW", 5),
    ("ITEM_TRAPCOMP_SPIKEDBALL", 4),
    ("ITEM_TRAPCOMP_LARGESERRATEDDISC", 4),
    ("ITEM_TRAPCOMP_MENACINGSPIKE", 5),
];

/// Body size used for butchering when the creature raws do not know the race (estimate).
const DEFAULT_BODY_SIZE: i32 = 50_000;

/// FNV-1a hash of an item kind (index key for order conditions).
pub(crate) fn kind_key(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Is `mat` (a job material token like `INORGANIC:IRON`) a metal?
pub fn is_metal_token(mat: &str) -> bool {
    mat.strip_prefix("INORGANIC:").map(|m| METALS.contains(&m)).unwrap_or(false)
}

/// Bars a forge job needs (MATERIAL_SIZE / 3, at least 1).
pub fn bars_needed(kind: &str, subtype: &str) -> i32 {
    match kind {
        "WEAPON" | "ARMOR" | "HELM" | "GLOVES" | "SHOES" | "PANTS" | "SHIELD" | "TRAPCOMP" => {
            let size = MATERIAL_SIZE.iter().find(|(t, _)| *t == subtype).map(|e| e.1).unwrap_or(3);
            (size / 3).max(1)
        }
        _ => 1,
    }
}

/// Free reagent counts, refreshed with the production census.
#[derive(Clone, Debug, Default)]
pub(crate) struct IndCensus {
    pub fuel: i32,
    pub ores: i32,
    pub metal_bars: i32,
    pub sand: i32,
    pub millable: i32,
    pub empty_bags: i32,
    pub corpses: i32,
    pub hides: i32,
    pub leather: i32,
    pub cloth: i32,
    pub thread: i32,
    pub webs: i32,
    pub bones: i32,
    pub slabs: i32,
    /// Free metal items designated for melting.
    pub melt: i32,
}

/// What a running industry job will make (filled at claim time).
#[derive(Clone, Debug, Default)]
pub(crate) struct IJob {
    pub kind: String,
    pub subtype: String,
}

#[derive(Default)]
pub(crate) struct Ind {
    pub census: IndCensus,
    pub jobs: Vec<IJob>,
    pub ready: bool,
    pub charcoal: u16,
    pub ash: u16,
    pub green_glass: u16,
    /// Palette index -> metals it smelts into (metal palette index, probability).
    pub ore_metals: HashMap<u16, Vec<(u16, u8)>>,
    /// Plant index -> millable / flour material.
    pub millable: Vec<bool>,
    pub flour: Vec<u16>,
    /// Material cache for butchering products: "RACE:PART" -> palette index.
    pub cmat: HashMap<String, u16>,
    /// Items made, by DF item kind.
    pub made: Vec<(String, i64)>,
    pub last_census: u64,
    /// [audit-eco] Melted metal in tenths of a bar per (smelter id, metal palette index); a bar
    /// is made when 10/10 have accumulated at the same smelter (wiki "Melt item").
    pub melt_tenths: std::collections::BTreeMap<(i32, u16), i32>,
    /// Items melted since the start.
    pub melted: u64,
}

/// [audit-eco] Metal recovered by melting one item, in tenths of a bar (wiki "Melt item"):
/// 0.3 bar x MATERIAL_SIZE for weapons, armor and trap components; 1 bar for furniture, buckets,
/// chains, cages, anvils and instruments; 0.5 for blocks, 0.2 for goblets and flasks; ammo 0.3 per
/// 25 (at least 0.1), coins 1.1 per 500 (at least 0.1). Crafts and other small items: 0.3
/// (estimate, the wiki lists no value).
pub fn melt_tenths(kind: &str, subtype: &str, stack: i32) -> i32 {
    let stack = stack.max(1);
    match kind {
        "WEAPON" | "ARMOR" | "HELM" | "GLOVES" | "SHOES" | "PANTS" | "SHIELD" | "TRAPCOMP" => {
            let size = MATERIAL_SIZE.iter().find(|(t, _)| *t == subtype).map(|e| e.1).unwrap_or(3);
            3 * size * stack
        }
        "AMMO" => (3 * stack / 25).max(1),
        "COIN" => (11 * stack / 500).max(1),
        "BLOCKS" => 5 * stack,
        "GOBLET" | "FLASK" => 2 * stack,
        "ANVIL" | "BARREL" | "BIN" | "CABINET" | "CHAIR" | "BOX" | "COFFIN" | "DOOR" | "FLOODGATE" | "GRATE" | "HATCH_COVER" | "PIPE_SECTION"
        | "STATUE" | "TABLE" | "TRACTION_BENCH" | "WEAPONRACK" | "ARMORSTAND" | "BUCKET" | "CHAIN" | "ANIMALTRAP" | "CAGE" | "INSTRUMENT" | "BED"
        | "TRAPPARTS" | "TOOL" | "SLAB" => 10 * stack,
        _ => 3 * stack,
    }
}

/// Industry overview for the bench/acceptance report.
#[derive(Clone, Debug, Default)]
pub struct IndustryReport {
    /// Items made since the start by item kind.
    pub made: Vec<(String, i64)>,
    /// Current stock of items by kind (bars also by metal, e.g. `BAR:iron`).
    pub stock: Vec<(String, i32)>,
}

fn race_of(desc: &str) -> String {
    let mut d = desc.trim().trim_matches(|c: char| !c.is_alphanumeric() && c != ' ').to_string();
    for suffix in [" corpse", " skeleton", " mangled", " mutilated", " remains", " body"] {
        while let Some(p) = d.find(suffix) {
            d.replace_range(p..p + suffix.len(), "");
        }
    }
    let d = d.trim().strip_prefix("stray ").unwrap_or(d.trim()).to_string();
    d.trim().to_string()
}

impl Sim {
    fn mat_by_token(&self, token: &str) -> Option<u16> {
        self.world.materials.iter().position(|m| m.token == token).map(|i| i as u16)
    }

    fn ensure_mat(&mut self, token: &str, name: &str, rgb: [u8; 3], f: impl FnOnce(&mut MaterialInfo)) -> u16 {
        if let Some(i) = self.mat_by_token(token) {
            return i;
        }
        let mut m = MaterialInfo { token: token.into(), name: name.into(), rgb, mat_type: -1, mat_index: -1, ..Default::default() };
        f(&mut m);
        self.world.materials.push(m);
        (self.world.materials.len() - 1) as u16
    }

    /// Resolve the industry materials once (appending missing ones to the palette).
    fn ind_setup(&mut self) {
        if self.ind.ready {
            return;
        }
        self.ind.ready = true;
        self.ind.charcoal = self.ensure_mat("COAL:CHARCOAL", "charcoal", [40, 40, 40], |_| {});
        self.ind.ash = self.ensure_mat("ASH", "ash", [180, 180, 180], |_| {});
        self.ind.green_glass = self.ensure_mat("GLASS_GREEN", "green glass", [60, 160, 60], |m| m.is_glass = true);
        let n = self.world.materials.len();
        for mi in 0..n {
            let tok = self.world.materials[mi].token.clone();
            let Some(ore) = tok.strip_prefix("INORGANIC:") else { continue };
            let Some((_, metals)) = ORES.iter().find(|(o, _)| *o == ore) else { continue };
            let mut out = Vec::new();
            for &(metal, prob) in metals.iter() {
                let name = metal.to_lowercase().replace('_', " ");
                let m = self.ensure_mat(&format!("INORGANIC:{metal}"), &name, [160, 160, 170], |m| m.is_metal = true);
                out.push((m, prob));
            }
            self.ind.ore_metals.insert(mi as u16, out);
        }
        let np = self.prod.plants.len();
        self.ind.millable = vec![false; np];
        self.ind.flour = vec![NO_MAT; np];
        for pi in 0..np {
            let tok = self.prod.plants[pi].token.clone();
            let mill = self.world.plant_raws.iter().any(|r| r.token == tok && r.millable);
            if mill {
                let name = format!("{} flour", self.prod.plants[pi].name);
                self.ind.millable[pi] = true;
                self.ind.flour[pi] = self.ensure_mat(&format!("PLANT:{tok}:MILL"), &name, [230, 220, 190], |_| {});
            }
        }
        // Keep the plant-material map as long as the palette (new entries are not plants).
        let len = self.world.materials.len();
        self.prod.mat_plant.resize(len, u32::MAX);
    }

    fn is_fuel(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "BAR" && self.world.material(it.mat).map(|m| m.token.starts_with("COAL:")).unwrap_or(false)
    }

    fn is_metal_bar(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "BAR" && self.world.material(it.mat).map(|m| m.is_metal).unwrap_or(false)
    }

    fn is_ore(&self, idx: usize) -> bool {
        self.imeta[idx].pc == pc::ORE && self.ind.ore_metals.contains_key(&self.world.items[idx].mat)
    }

    fn is_sand(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "POWDER_MISC" && self.world.material(it.mat).map(|m| m.tags.iter().any(|t| t == "IS_SAND")).unwrap_or(false)
    }

    fn is_millable(&self, idx: usize) -> bool {
        let m = &self.imeta[idx];
        m.pc == pc::PLANT && !self.world.items[idx].flags.rotten && self.ind.millable.get(m.plant as usize).copied().unwrap_or(false)
    }

    fn is_empty_bag(&self, idx: usize) -> bool {
        self.imeta[idx].pc == pc::BAG && self.world.items[idx].container < 0 && self.container_empty(self.world.items[idx].id)
    }

    fn is_butcherable(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        // Named corpses belong to historical figures (sapients): DF never butchers them.
        it.kind == "CORPSE" && !it.flags.rotten && !it.desc.contains("'s ") && it.name.is_empty()
    }

    fn is_hide(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "CORPSEPIECE" && !it.flags.rotten && (it.desc.ends_with(" skin") || it.desc.contains(" hide"))
    }

    fn is_bone(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "CORPSEPIECE" && (it.desc.contains(" bone") || it.desc.contains(" skull"))
    }

    fn is_web(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "THREAD" && it.desc.contains(" web")
    }

    fn is_slab_blank(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.kind == "SLAB" && it.name.is_empty()
    }

    /// [industry] Count free reagents (once per production pass).
    pub(crate) fn ind_census(&mut self, now: u64) {
        self.ind_setup();
        // Reagent availability is refreshed every 4th production pass (DF checks jobs
        // periodically too); the claim itself always looks at the real items.
        if self.ind.last_census > 0 && now < self.ind.last_census + 4 * crate::production::PROD_PERIOD {
            return;
        }
        self.ind.last_census = now;
        let mut c = IndCensus::default();
        for idx in 0..self.world.items.len() {
            let p = self.imeta[idx].pc;
            if self.world.items[idx].flags.melt && self.is_meltable(idx) && self.item_free(idx, now) {
                c.melt += 1;
            }
            if !matches!(p, pc::ORE | pc::BAR | pc::POWDER | pc::CORPSE | pc::BODYPART | pc::LEATHER | pc::CLOTH | pc::THREAD | pc::BAG | pc::PLANT | pc::FURNITURE) {
                continue;
            }
            if !self.item_free(idx, now) {
                continue;
            }
            let units = self.world.items[idx].stack - self.imeta[idx].reserved;
            match p {
                pc::ORE => c.ores += self.is_ore(idx) as i32,
                pc::BAR => {
                    if self.is_fuel(idx) {
                        c.fuel += units;
                    } else if self.is_metal_bar(idx) {
                        c.metal_bars += units;
                    }
                }
                pc::POWDER => c.sand += self.is_sand(idx) as i32,
                pc::PLANT => c.millable += self.is_millable(idx) as i32,
                pc::BAG => c.empty_bags += self.is_empty_bag(idx) as i32,
                pc::CORPSE => c.corpses += self.is_butcherable(idx) as i32,
                pc::BODYPART => {
                    if self.is_hide(idx) {
                        c.hides += 1;
                    } else if self.is_bone(idx) {
                        c.bones += units;
                    }
                }
                pc::LEATHER => c.leather += 1,
                pc::CLOTH => c.cloth += 1,
                pc::THREAD => {
                    if self.is_web(idx) {
                        c.webs += 1;
                    } else {
                        c.thread += 1;
                    }
                }
                pc::FURNITURE => c.slabs += self.is_slab_blank(idx) as i32,
                _ => {}
            }
        }
        self.ind.census = c;
    }

    fn needs_fuel(&self, r: &Recipe) -> bool {
        // Magma furnaces need no fuel (their recipes are listed under the plain workshop).
        matches!(r.workshop, "Smelter" | "MetalsmithsForge" | "GlassFurnace" | "Kiln")
    }

    /// [industry] Are the reagents of an industry recipe in stock?
    pub(crate) fn ind_available(&self, f: u8, r: &Recipe) -> bool {
        let c = &self.ind.census;
        let p = &self.prod.census;
        if f >= crate::animals::F_FIRST {
            return self.animals_available(f); // [animals]
        }
        let fuel = !self.needs_fuel(r) || c.fuel > 0;
        fuel && match f {
            F_CHARCOAL | F_ASH | F_CRAFTS_WOOD | F_WOOD_ITEM => p.logs > 0,
            F_CRAFTS_STONE => p.boulders > 0,
            F_SMELT => c.ores > 0,
            F_FORGE | F_CRAFTS_METAL => c.metal_bars > 0,
            F_RAWGLASS => c.sand > 0,
            F_MILL => c.millable > 0 && c.empty_bags > 0,
            F_BUTCHER => c.corpses > 0,
            F_TAN => c.hides > 0,
            F_LEATHER => c.leather > 0,
            F_CLOTH_ITEM => c.cloth > 0,
            F_WEAVE => c.thread > 0,
            F_WEBS => c.webs > 0,
            F_CRAFTS_BONE => c.bones > 0,
            F_ENGRAVE => c.slabs > 0,
            F_MELT => c.melt > 0,
            _ => false,
        }
    }

    /// [audit-eco] A metal item that may be melted: designated, of a metal, no artifact, not a
    /// bar (DF never melts bars back).
    pub(crate) fn is_meltable(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        it.flags.melt
            && it.quality < 6
            && it.kind != "BAR"
            && self.world.material(it.mat).map(|m| is_metal_token(&m.token)).unwrap_or(false)
    }

    /// [audit-eco] Nearest free item designated for melting (any item class), into `wj`.
    fn ind_pick_melt(&mut self, site: u32, c: u32, now: u64, wj: &mut WJob) -> bool {
        let sp = self.nav.xyz(site as usize);
        let mut best: Option<(i32, i32)> = None;
        for idx in 0..self.world.items.len() {
            if !self.world.items[idx].flags.melt || !self.is_meltable(idx) || !self.item_free(idx, now) {
                continue;
            }
            let it = &self.world.items[idx];
            let Some(t) = self.world.idx(it.pos) else { continue };
            if self.nav.comp_near(t) != c {
                continue;
            }
            let d = crate::manhattan(sp, self.nav.xyz(t));
            if best.map(|b| (d, it.id) < b).unwrap_or(true) {
                best = Some((d, it.id));
            }
        }
        let Some((_, id)) = best else { return false };
        let idx = self.item_idx[&id] as usize;
        let k = wj.n as usize;
        wj.items[k] = id;
        wj.need[k] = self.world.items[idx].stack - self.imeta[idx].reserved;
        wj.n += 1;
        true
    }

    /// Material indices matching a job material token (`INORGANIC:IRON`; `INORGANIC` = any).
    fn job_mats(&self, mat: &str) -> Option<Vec<u16>> {
        if mat.is_empty() || mat == "INORGANIC" || !mat.contains(':') {
            return None;
        }
        Some(self.world.materials.iter().enumerate().filter(|(_, m)| m.token == mat).map(|(i, _)| i as u16).collect())
    }

    /// Pick `units` units of items matching `pred` (several stacks if needed) into `wj`.
    fn ind_pick(&mut self, site: u32, c: u32, now: u64, units: i32, wj: &mut WJob, pred: impl Fn(&Sim, usize) -> bool) -> bool {
        let mut found = Vec::new();
        let slots = 4 - wj.n as usize;
        if slots == 0 {
            return false;
        }
        let (taken, nt) = (wj.items, wj.n as usize);
        self.find_items(site, c, now, slots.min(units.max(1) as usize), &mut found, |sim, i| pred(sim, i) && !taken[..nt].contains(&sim.world.items[i].id));
        let mut left = units;
        let mut picked = Vec::new();
        for &id in &found {
            if left <= 0 {
                break;
            }
            let idx = self.item_idx[&id] as usize;
            let avail = self.world.items[idx].stack - self.imeta[idx].reserved;
            let take = avail.min(left);
            if take <= 0 {
                continue;
            }
            picked.push((id, take));
            left -= take;
        }
        if left > 0 {
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

    /// [industry] Choose reagents for an industry job. Fills `wj`; false = not possible now.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn ind_claim(&mut self, s: usize, r: u8, qjob: &BuildingJob, site: u32, c: u32, now: u64, wj: &mut WJob) -> bool {
        let rec = &RECIPES[r as usize];
        let crate::production::Input::Ind(f) = rec.input else { return false };
        if f >= crate::animals::F_FIRST {
            return self.animals_claim(s, f, site, c, now, wj); // [animals]
        }
        let kind = if qjob.item_type.is_empty() { rec.item_type.to_string() } else { qjob.item_type.clone() };
        let subtype = if qjob.item_subtype.is_empty() { rec.item_subtype.to_string() } else { qjob.item_subtype.clone() };
        let mats = self.job_mats(&qjob.mat);
        let mat_ok = |sim: &Sim, i: usize| mats.as_ref().map(|v| v.contains(&sim.world.items[i].mat)).unwrap_or(true);
        let ok = match f {
            F_CHARCOAL | F_ASH | F_CRAFTS_WOOD | F_WOOD_ITEM => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::LOG),
            F_CRAFTS_STONE => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::BOULDER && mat_ok(sim, i)),
            F_SMELT => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.is_ore(i) && mat_ok(sim, i)),
            F_FORGE | F_CRAFTS_METAL => {
                let n = if f == F_FORGE { bars_needed(&kind, &subtype) } else { 1 };
                if n > 3 {
                    false
                } else {
                    // The ordered metal, or the metal of the nearest bar.
                    let mut found = Vec::new();
                    self.find_items(site, c, now, 1, &mut found, |sim, i| sim.is_metal_bar(i) && mat_ok(sim, i));
                    match found.first().and_then(|id| self.item_idx.get(id)).map(|&i| self.world.items[i as usize].mat) {
                        Some(m) => self.ind_pick(site, c, now, n, wj, move |sim, i| sim.is_metal_bar(i) && sim.world.items[i].mat == m),
                        None => false,
                    }
                }
            }
            F_RAWGLASS => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.is_sand(i)),
            F_MILL => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.is_millable(i)) && self.ind_pick(site, c, now, 1, wj, |sim, i| sim.is_empty_bag(i)),
            F_BUTCHER => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::CORPSE && sim.is_butcherable(i)),
            F_TAN => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::BODYPART && sim.is_hide(i)),
            F_LEATHER => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::LEATHER && mat_ok(sim, i)),
            F_CLOTH_ITEM => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::CLOTH && mat_ok(sim, i)),
            F_WEAVE => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::THREAD && !sim.is_web(i)),
            F_WEBS => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::THREAD && sim.is_web(i)),
            F_CRAFTS_BONE => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::BODYPART && sim.is_bone(i)),
            F_ENGRAVE => self.ind_pick(site, c, now, 1, wj, |sim, i| sim.imeta[i].pc == pc::FURNITURE && sim.is_slab_blank(i)),
            F_MELT => self.ind_pick_melt(site, c, now, wj),
            _ => false,
        };
        if !ok {
            return false;
        }
        if self.needs_fuel(rec) && !self.ind_pick(site, c, now, 1, wj, |sim, i| sim.is_fuel(i)) {
            return false;
        }
        if self.ind.jobs.len() <= s {
            self.ind.jobs.resize(self.ai.len().max(s + 1), IJob::default());
        }
        self.ind.jobs[s] = IJob { kind, subtype };
        true
    }

    fn count_made(&mut self, kind: &str, n: i32) {
        match self.ind.made.iter_mut().find(|(k, _)| k == kind) {
            Some(e) => e.1 += n as i64,
            None => self.ind.made.push((kind.to_string(), n as i64)),
        }
    }

    /// Create a product item at `tile`.
    fn ind_make(&mut self, kind: &str, subtype: &str, mat: u16, tile: usize, stack: i32, desc: String) -> i32 {
        let id = self.create_item(kind, mat, tile, desc);
        if let Some(&i) = self.item_idx.get(&id) {
            let i = i as usize;
            self.world.items[i].subtype = subtype.to_string();
            self.world.items[i].stack = stack.max(1);
            let (pcl, pl) = self.classify(&self.world.items[i]);
            self.imeta[i].pc = pcl;
            self.imeta[i].plant = pl;
        }
        self.count_made(kind, stack.max(1));
        id
    }

    fn mat_name(&self, mat: u16) -> String {
        self.world.material(mat).map(|m| m.name.clone()).unwrap_or_default()
    }

    /// Palette entry for a butchering product of a race (cached).
    fn creature_mat(&mut self, race: &str, part: &str, rgb: [u8; 3]) -> u16 {
        let key = format!("{race}:{part}");
        if let Some(&m) = self.ind.cmat.get(&key) {
            return m;
        }
        let tok = format!("CREATURE:{}:{part}", race.to_uppercase().replace(' ', "_"));
        let name = format!("{race} {}", part.to_lowercase());
        let m = self.ensure_mat(&tok, &name, rgb, |_| {});
        self.prod.mat_plant.resize(self.world.materials.len(), u32::MAX);
        self.ind.cmat.insert(key, m);
        m
    }

    fn body_size_of(&self, race: &str) -> i32 {
        for cr in &self.world.creature_raws {
            let hit = cr.name.eq_ignore_ascii_case(race) || cr.castes.iter().any(|c| c.name.eq_ignore_ascii_case(race));
            if hit {
                if let Some(c) = cr.castes.iter().find(|c| c.body_size > 0) {
                    return c.body_size;
                }
            }
        }
        DEFAULT_BODY_SIZE
    }

    /// [industry] Make the products of a finished industry job and use up its reagents.
    pub(crate) fn ind_finish(&mut self, s: usize, wj: &WJob, recipe: u8, now: u64) {
        let rec = &RECIPES[recipe as usize];
        let crate::production::Input::Ind(f) = rec.input else { return };
        if f >= crate::animals::F_FIRST {
            self.animals_finish(s, wj, f, now); // [animals]
            return;
        }
        let site = wj.site as usize;
        let ij = self.ind.jobs.get(s).cloned().unwrap_or_default();
        let mut r = Rng::for_unit(self.seed, s, now, 31);
        let first = wj.items[0];
        let Some(&fi) = self.item_idx.get(&first) else { return };
        let fi = fi as usize;
        let (fmat, fdesc) = (self.world.items[fi].mat, self.world.items[fi].desc.clone());
        let mut keep: Vec<i32> = Vec::new();
        match f {
            F_CHARCOAL => {
                let m = self.ind.charcoal;
                self.ind_make("BAR", "", m, site, 1, "charcoal".into());
            }
            F_ASH => {
                let m = self.ind.ash;
                self.ind_make("BAR", "", m, site, 1, "ash".into());
            }
            F_SMELT => {
                let metals = self.ind.ore_metals.get(&fmat).cloned().unwrap_or_default();
                for (m, prob) in metals {
                    if prob >= 100 || r.below(100) < prob as u32 {
                        let name = format!("{} bars", self.mat_name(m));
                        self.ind_make("BAR", "", m, site, 4, name);
                    }
                }
            }
            F_FORGE => {
                let noun = rec.noun;
                let desc = format!("{} {noun}", self.mat_name(fmat));
                let (kind, sub) = (ij.kind.clone(), ij.subtype.clone());
                if rec.count > 1 && kind == "AMMO" {
                    self.ind_make(&kind, &sub, fmat, site, rec.count as i32, desc);
                } else {
                    for _ in 0..rec.count.max(1) {
                        self.ind_make(&kind, &sub, fmat, site, 1, desc.clone());
                    }
                }
            }
            F_CRAFTS_STONE | F_CRAFTS_WOOD | F_CRAFTS_BONE | F_CRAFTS_METAL => {
                let kind = CRAFT_KINDS[r.below(CRAFT_KINDS.len() as u32) as usize];
                let base = if f == F_CRAFTS_BONE { race_of(&fdesc).replace(" bone", "").replace(" skull", "") + " bone" } else { self.mat_name(fmat) };
                self.ind_make(kind, "", fmat, site, 1, format!("{base} {}", kind.to_lowercase()));
            }
            F_WOOD_ITEM => {
                let desc = format!("{} {}", self.mat_name(fmat), rec.noun);
                let (kind, sub) = (ij.kind.clone(), ij.subtype.clone());
                self.ind_make(&kind, &sub, fmat, site, 1, desc);
            }
            F_RAWGLASS => {
                let m = self.ind.green_glass;
                self.ind_make("ROUGH", "", m, site, 1, "raw green glass".into());
            }
            F_MILL => {
                let plant = self.imeta[fi].plant;
                let flour = self.ind.flour.get(plant as usize).copied().unwrap_or(NO_MAT);
                let bag = wj.items[1];
                keep.push(bag);
                self.finish_in_container(bag, "POWDER_MISC", flour, 1, format!("{} flour", self.plant(plant).map(|p| p.name.clone()).unwrap_or_default()));
                self.count_made("POWDER_MISC", 1);
            }
            F_BUTCHER => {
                let race = race_of(&fdesc);
                let size = self.body_size_of(&race);
                let skeleton = fdesc.contains("skeleton");
                let bone = self.creature_mat(&race, "BONE", [230, 230, 210]);
                if !skeleton {
                    let meat = self.creature_mat(&race, "MUSCLE", [160, 40, 40]);
                    let fat = self.creature_mat(&race, "FAT", [230, 210, 120]);
                    let skin = self.creature_mat(&race, "SKIN", [170, 120, 80]);
                    let id = self.ind_make("MEAT", "", meat, site, (size / 10_000).max(1), format!("{race} meat"));
                    self.list_consumable(id);
                    self.ind_make("GLOB", "", fat, site, (size / 40_000).max(1), format!("{race} fat"));
                    self.ind_make("CORPSEPIECE", "", skin, site, 1, format!("{race} skin"));
                }
                self.ind_make("CORPSEPIECE", "", bone, site, (size / 20_000).max(1), format!("{race} bone"));
                self.ind_make("CORPSEPIECE", "", bone, site, 1, format!("{race} skull"));
            }
            F_TAN => {
                let race = race_of(&fdesc.replace(" skin", "").replace(" hide", ""));
                let m = self.creature_mat(&race, "LEATHER", [150, 100, 60]);
                self.ind_make("SKIN_TANNED", "", m, site, 1, format!("{race} leather"));
            }
            F_LEATHER | F_CLOTH_ITEM => {
                let base = fdesc.replace(" leather", "").replace(" cloth", "");
                let base = base.trim_matches(|c: char| !c.is_alphanumeric() && c != ' ').to_string();
                let what = if f == F_LEATHER { "leather" } else { "cloth" };
                let (kind, sub) = (ij.kind.clone(), ij.subtype.clone());
                for _ in 0..rec.count.max(1) {
                    self.ind_make(&kind, &sub, fmat, site, 1, format!("{base} {what} {}", rec.noun));
                }
            }
            F_WEAVE => {
                let base = fdesc.replace(" thread", "").replace(" yarn", "");
                let base = base.trim_matches(|c: char| !c.is_alphanumeric() && c != ' ').to_string();
                let base = base.split(" (").next().unwrap_or(&base).to_string();
                self.ind_make("CLOTH", "", fmat, site, 1, format!("{base} cloth"));
            }
            F_WEBS => {
                let base = fdesc.replace(" web", " thread");
                self.ind_make("THREAD", "", fmat, site, 1, base);
            }
            F_MELT => {
                // Fractions stay at this smelter until a whole bar is reached.
                let bid = self.prod.shops.iter().find(|sh| sh.site == wj.site).map(|sh| sh.bid).unwrap_or(-1);
                let (kind, sub, stack) = {
                    let it = &self.world.items[fi];
                    (it.kind.clone(), it.subtype.clone(), it.stack)
                };
                let e = self.ind.melt_tenths.entry((bid, fmat)).or_insert(0);
                *e += melt_tenths(&kind, &sub, stack);
                let bars = *e / 10;
                *e %= 10;
                if bars > 0 {
                    let name = format!("{} bars", self.mat_name(fmat));
                    self.ind_make("BAR", "", fmat, site, bars, name);
                }
                self.ind.melted += 1;
            }
            F_ENGRAVE => {
                keep.push(first);
                self.world.items[fi].name = "memorial".into();
                self.world.items[fi].desc = format!("engraved {}", fdesc);
            }
            _ => {}
        }
        for k in 0..wj.n as usize {
            let id = wj.items[k];
            if keep.contains(&id) {
                self.release_reagent(id);
            } else {
                self.destroy_item(id);
            }
        }
        if let Some(j) = self.ind.jobs.get_mut(s) {
            *j = IJob::default();
        }
    }

    /// Put a new item into a container standing at a job site (milling into a bag).
    fn finish_in_container(&mut self, cont: i32, kind: &str, mat: u16, stack: i32, desc: String) {
        let Some(&ci) = self.item_idx.get(&cont) else { return };
        let ci = ci as usize;
        let pos = self.world.items[ci].pos;
        let id = self.next_item_id;
        self.next_item_id += 1;
        self.push_item(Item {
            id,
            kind: kind.into(),
            mat,
            pos,
            stack,
            container: cont,
            holder: -1,
            building: -1,
            desc,
            flags: ItemFlags { in_container: true, ..Default::default() },
            ..Default::default()
        });
        self.contents.entry(cont).or_default().push(id);
        // A bag with flour belongs to a food stockpile.
        self.world.items[ci].flags.in_job = false;
        self.imeta[ci].reserved = 0;
        self.set_item_cat(ci, crate::items::Cat::Food);
    }

    /// [industry] Is this underground floor part of a cavern (wiki "Farming": cavern soil is
    /// not "poor")? Estimate: DF's cavern feature flag is not exported, so a tile counts as
    /// cavern when it has natural grass/moss (only caverns have it underground) or no ceiling
    /// for two levels (caverns are tall open spaces; dug rooms have rock or a floor above).
    pub(crate) fn is_cavern_floor(&self, i: usize) -> bool {
        let t = self.world.tiles[i];
        if t.flags & rf_core::tile_flags::SUBTERRANEAN == 0 {
            return false;
        }
        if self.world.tiletypes.get(t.tt).material.is_grass() {
            return true;
        }
        let layer = self.nav.layer;
        let open = |q: usize| q < self.world.tiles.len() && self.world.tiletypes.get(self.world.tiles[q].tt).shape == rf_core::Shape::Empty;
        open(i + layer) && open(i + 2 * layer)
    }

    // ------------------------------------------------------------------ public API

    /// Items made by the industries and the current stock by item kind.
    pub fn industry_report(&self) -> IndustryReport {
        let mut stock: Vec<(String, i32)> = Vec::new();
        let mut add = |k: String, n: i32| match stock.iter_mut().find(|e| e.0 == k) {
            Some(e) => e.1 += n,
            None => stock.push((k, n)),
        };
        for (idx, it) in self.world.items.iter().enumerate() {
            if self.imeta[idx].dead || it.flags.construction || it.building >= 0 {
                continue;
            }
            add(it.kind.clone(), it.stack.max(1));
            if it.kind == "BAR" {
                add(format!("BAR:{}", self.mat_name(it.mat)), it.stack.max(1));
            }
        }
        stock.sort();
        let mut made = self.ind.made.clone();
        made.sort();
        IndustryReport { made, stock }
    }
}

#[cfg(test)]
mod melt_tests {
    use super::melt_tenths;

    #[test]
    fn melt_yields_follow_the_wiki_table() {
        // wiki "Melt item": breastplate 2.7, greaves 1.8, leggings 1.5, helm 0.6, low boot 0.3,
        // pick 1.2, crossbow 0.9, bolts 0.3 per 25 (single 0.1), coins 1.1 per 500, table 1.0,
        // blocks 0.5, goblet 0.2.
        assert_eq!(melt_tenths("ARMOR", "ITEM_ARMOR_BREASTPLATE", 1), 27);
        assert_eq!(melt_tenths("PANTS", "ITEM_PANTS_GREAVES", 1), 18);
        assert_eq!(melt_tenths("PANTS", "ITEM_PANTS_LEGGINGS", 1), 15);
        assert_eq!(melt_tenths("HELM", "ITEM_HELM_HELM", 1), 6);
        assert_eq!(melt_tenths("SHOES", "ITEM_SHOES_BOOTS_LOW", 1), 3);
        assert_eq!(melt_tenths("WEAPON", "ITEM_WEAPON_PICK", 1), 12);
        assert_eq!(melt_tenths("WEAPON", "ITEM_WEAPON_CROSSBOW", 1), 9);
        assert_eq!(melt_tenths("AMMO", "ITEM_AMMO_BOLTS", 25), 3);
        assert_eq!(melt_tenths("AMMO", "ITEM_AMMO_BOLTS", 1), 1);
        assert_eq!(melt_tenths("COIN", "", 500), 11);
        assert_eq!(melt_tenths("TABLE", "", 1), 10);
        assert_eq!(melt_tenths("BLOCKS", "", 1), 5);
        assert_eq!(melt_tenths("GOBLET", "", 1), 2);
    }
}
