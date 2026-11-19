//! [fire15] Temperature scale, material heat properties and the DF heat-transfer step.
//!
//! Sources: wiki Temperature (scale, "Temperature transfer", "Material values"), Magma,
//! Fire-safe, Magma-safe, Material definition token. Fallback values for materials without
//! exported heat data (synthetic worlds, exports before round 3) come from the vanilla raws
//! (`material_template_default.txt` templates and the `inorganic_*.txt` MELTING/BOILING/SPEC
//! tokens): game facts, compiled in as a small table.

use rf_core::{MaterialHeat, MaterialInfo};
use serde::{Deserialize, Serialize};

/// DF "NONE" for temperatures (u16, capped at 60000; 60001 = no value).
pub const NONE_TEMP: i32 = 60_001;
/// Underground ambient (w:Temperature table).
pub const UNDERGROUND_TEMP: i32 = 10_015;
/// Tiles next to magma ("warm stone", w:Magma).
pub const WARM_STONE_TEMP: i32 = 10_075;
/// Magma (w:Magma "Properties of magma").
pub const MAGMA_TEMP: i32 = 12_000;
pub const FIRE_SAFE_TEMP: i32 = 11_000;
pub const MAGMA_SAFE_TEMP: i32 = 12_000;
/// Environment fire: starts at 10508, rises quickly to 10708 (w:Fire).
pub const GRASS_FIRE_START: i32 = 10_508;
pub const GRASS_FIRE_MAX: i32 = 10_708;
/// Burning items heat to about ignite + 200, slightly below (w:Fire).
pub const BURN_ABOVE_IGNITE: i32 = 200;
/// Equilibrium of a burning item: ignite + 190 (ESTIMATE for "slightly below").
pub const BURN_EQUILIBRIUM: i32 = BURN_ABOVE_IGNITE - 10;
pub const DRAGONFIRE_TEMP: i32 = 50_000;
/// Water never gets hotter than its boiling point (ESTIMATE).
pub const WATER_BOIL: i32 = 10_180;
/// Dwarf/human body temperature (HOMEOTHERM 10067 in the raws).
pub const BODY_TEMP: i32 = 10_067;
/// Wear points per wear level (w:Wear).
pub const WEAR_PER_LEVEL: u32 = 806_400;
/// Wear points per tick of a burning item (w:Wear, w:Material definition token).
pub const BURN_WEAR: u32 = 10;

/// An item temperature: whole degrees in the high 16 bits, the retained fraction in the low 16
/// (DF keeps `temperature.whole` and `.fraction`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Temp(pub u32);

impl Temp {
    pub fn from_whole(t: i32) -> Temp {
        Temp((t.clamp(0, 60_000) as u32) << 16)
    }
    #[inline]
    pub fn whole(self) -> i32 {
        (self.0 >> 16) as i32
    }
    /// One DF tick of exchange with `env`: the temperature moves by (env - whole) / spec_heat.
    /// Fractions accumulate, only the whole part enters the difference (w:Temperature).
    /// Integer fixed point, truncation toward zero (ESTIMATE for DF's rounding).
    #[inline]
    pub fn step(&mut self, env: i32, spec: i32) {
        if spec <= 0 || spec >= NONE_TEMP {
            return;
        }
        let d = (env - self.whole()) as i64;
        let delta = (d << 16) / spec as i64;
        self.0 = (self.0 as i64 + delta).clamp(0, 60_000i64 << 16) as u32;
    }
}

/// A heat point is set (not NONE, not unknown).
#[inline]
pub fn is_set(p: i32) -> bool {
    p > 0 && p < NONE_TEMP
}

fn h(spec: i32, heatdam: i32, colddam: i32, ignite: i32, melt: i32, boil: i32, fixed: i32) -> MaterialHeat {
    MaterialHeat { spec_heat: spec, heatdam_point: heatdam, colddam_point: colddam, ignite_point: ignite, melting_point: melt, boiling_point: boil, mat_fixed_temp: fixed }
}

const N: i32 = NONE_TEMP;

/// Vanilla inorganics with their own SPEC_HEAT / MELTING_POINT / BOILING_POINT (raws
/// `inorganic_metal.txt`, `inorganic_stone_*.txt`, `inorganic_other.txt`; 0 = template value).
const INORGANIC: &[(&str, i32, i32, i32)] = &[
    // metals (spec, melt, boil)
    ("ADAMANTINE", 7500, 25_000, 50_000),
    ("RAW_ADAMANTINE", 7500, 25_000, 50_000),
    ("ALUMINUM", 900, 11_188, 14_534),
    ("BILLON", 385, 11_952, 14_611),
    ("BISMUTH", 123, 10_488, 12_815),
    ("BISMUTH_BRONZE", 435, 11_868, 14_140),
    ("BLACK_BRONZE", 385, 11_952, 14_611),
    ("BRASS", 377, 11_656, 14_068),
    ("BRONZE", 435, 11_868, 14_140),
    ("COPPER", 385, 11_952, 14_611),
    ("ELECTRUM", 180, 11_828, 14_968),
    ("GOLD", 129, 11_915, 15_141),
    ("IRON", 450, 12_768, 15_150),
    ("LEAD", 130, 10_589, 13_148),
    ("NICKEL", 444, 12_619, 15_243),
    ("NICKEL_SILVER", 444, 11_620, 15_243),
    ("PEWTER_FINE", 210, 10_417, 14_684),
    ("PEWTER_LAY", 210, 10_417, 14_684),
    ("PEWTER_TRIFLE", 210, 10_417, 14_684),
    ("PIG_IRON", 500, 12_106, 13_968),
    ("PLATINUM", 130, 13_182, 16_885),
    ("ROSE_GOLD", 129, 11_915, 15_141),
    ("SILVER", 230, 11_731, 13_892),
    ("STEEL", 500, 12_718, 14_968),
    ("STERLING_SILVER", 230, 11_602, 13_892),
    ("TIN", 210, 10_417, 14_684),
    ("ZINC", 390, 10_755, 11_633),
    // stones and minerals (STONE_TEMPLATE spec 800, melt 11500, boil 14000 otherwise)
    ("ALABASTER", 0, 10_261, 0),
    ("ALUNITE", 0, 13_690, 15_364),
    ("ANDESITE", 0, 11_800, 0),
    ("ANHYDRITE", 0, 12_610, 0),
    ("BASALT", 0, 12_160, 0),
    ("BAUXITE", 0, 13_600, 15_000),
    ("BISMUTHINITE", 0, 11_368, 0),
    ("BORAX", 0, 11_334, 12_835),
    ("BRIMSTONE", 0, 10_207, 10_800),
    ("CALCITE", 0, 12_902, 0),
    ("CASSITERITE", 0, 12_025, 0),
    ("CHALK", 0, 11_485, 0),
    ("CHERT", 0, 13_101, 0),
    ("CHROMITE", 0, 13_645, 0),
    ("CINNABAR", 0, 11_044, 0),
    ("CRYOLITE", 0, 11_817, 0),
    ("DIORITE", 0, 11_800, 0),
    ("DOLOMITE", 0, 16_507, 18_000),
    ("GABBRO", 0, 12_160, 0),
    ("GALENA", 0, 12_005, 12_305),
    ("GRANITE", 0, 11_440, 0),
    ("GYPSUM", 0, 10_261, 0),
    ("HEMATITE", 0, 12_736, 0),
    ("HORNBLENDE", 0, 11_890, 0),
    ("HORN_SILVER", 0, 10_819, 0),
    ("ILMENITE", 0, 12_457, 0),
    ("KAOLINITE", 0, 13_150, 0),
    ("LIMESTONE", 0, 11_485, 0),
    ("MAGNETITE", 0, 12_768, 0),
    ("MALACHITE", 0, 10_295, 0),
    ("MARBLE", 0, 11_485, 0),
    ("MARCASITE", 0, 10_810, 0),
    ("MICA", 0, 12_295, 0),
    ("NATIVE_ALUMINUM", 900, 11_188, 14_534),
    ("NATIVE_COPPER", 385, 11_952, 14_611),
    ("NATIVE_GOLD", 129, 11_915, 15_141),
    ("NATIVE_PLATINUM", 130, 13_182, 16_885),
    ("NATIVE_SILVER", 230, 11_731, 13_892),
    ("OBSIDIAN", 0, 13_600, 16_000),
    ("OLIVINE", 0, 13_168, 0),
    ("ORPIMENT", 0, 10_639, 11_017),
    ("ORTHOCLASE", 0, 12_250, 0),
    ("PERICLASE", 0, 15_040, 16_000),
    ("PETRIFIED_WOOD", 0, 12_970, 14_014),
    ("PITCHBLENDE", 0, 12_070, 12_340),
    ("PYROLUSITE", 0, 10_963, 0),
    ("QUARTZITE", 0, 12_970, 14_014),
    ("REALGAR", 0, 10_639, 11_017),
    ("RHYOLITE", 0, 11_440, 0),
    ("ROCK_SALT", 0, 11_442, 12_637),
    ("RUTILE", 0, 13_285, 0),
    ("SALTPETER", 0, 10_601, 10_720),
    ("SANDSTONE", 0, 12_070, 0),
    ("SATINSPAR", 0, 10_261, 0),
    ("SELENITE", 0, 10_225, 0),
    ("SERPENTINE", 0, 10_281, 0),
    ("SPHALERITE", 0, N, 12_133),
    ("STIBNITE", 0, 10_990, 0),
    ("SYLVITE", 0, 11_397, 12_700),
    ("TALC", 0, 12_700, 0),
    ("TETRAHEDRITE", 0, 11_111, 0),
];

/// Fuels (raws: COAL_BITUMINOUS, LIGNITE, GRAPHITE spec 409, ignite 11440, boil 16708).
const FUELS: &[&str] = &["COAL_BITUMINOUS", "LIGNITE", "GRAPHITE"];

/// Heat properties of a palette material: the exported values, else a fallback by token and
/// material class (see module docs). Unknown fields of an export (0) count as NONE.
pub fn heat_of(m: &MaterialInfo) -> MaterialHeat {
    let e = m.heat;
    let known = e.spec_heat != 0 || e.melting_point != 0 || e.ignite_point != 0 || e.boiling_point != 0 || e.heatdam_point != 0 || e.mat_fixed_temp != 0;
    if known {
        let f = |p: i32| if p == 0 { N } else { p };
        return h(f(e.spec_heat), f(e.heatdam_point), f(e.colddam_point), f(e.ignite_point), f(e.melting_point), f(e.boiling_point), f(e.mat_fixed_temp));
    }
    fallback_heat(&m.token, m.is_metal, m.is_stone || m.is_gem || m.is_ore || m.is_soil, m.is_glass, m.is_wood)
}

/// Fallback by token and class (raws templates; ESTIMATE where the class guess is used).
pub fn fallback_heat(token: &str, metal: bool, stone: bool, glass: bool, wood: bool) -> MaterialHeat {
    let t = token.trim_start_matches("INORGANIC:");
    if FUELS.contains(&t) || token == "COAL:COKE" || token == "COAL:CHARCOAL" {
        return h(409, N, N, 11_440, N, if token.starts_with("COAL:") { N } else { 16_708 }, N);
    }
    if let Some(&(_, spec, melt, boil)) = INORGANIC.iter().find(|e| e.0 == t) {
        let metal_like = spec != 0;
        let spec = if metal_like { spec } else { 800 };
        let boil = if boil != 0 { boil } else if metal_like { 15_150 } else { 14_000 };
        return h(spec, N, N, N, melt, boil, N);
    }
    if metal {
        return h(450, N, N, N, 11_800, 14_000, N); // ESTIMATE: unknown metal
    }
    if glass || t.starts_with("GLASS_") {
        return h(700, N, N, N, 13_600, 16_000, N);
    }
    if stone {
        return h(800, N, N, N, 11_500, 14_000, N); // STONE_TEMPLATE / SOIL_TEMPLATE
    }
    if token == "WATER" {
        return h(4181, N, N, N, 10_000, 10_180, N);
    }
    if wood || token.ends_with(":WOOD") {
        let fixed = if token.contains("NETHER_CAP") { 10_000 } else { N }; // plant_standard.txt
        return h(420, 10_250, 9_900, 10_508, N, N, fixed);
    }
    if token.starts_with("CREATURE:") || token.starts_with("PLANT:") {
        if token.ends_with(":FAT") || token.ends_with(":TALLOW") || token.ends_with(":CHEESE") {
            return h(4181, 10_250, 9_900, 10_338, 10_078, N, N); // FAT_TEMPLATE
        }
        if token.ends_with(":DRINK") {
            return h(2440, N, N, 10_800, 9_850, 10_150, N);
        }
        if token.ends_with(":SEED") {
            return h(800, 10_500, 9_900, 10_400, N, N, N);
        }
        if token.ends_with(":HAIR") || token.ends_with(":THREAD") || token.ends_with(":LEATHER") {
            return h(420, 10_250, 9_900, 10_508, N, N, N);
        }
        if token.ends_with(":BONE") {
            return h(1000, 10_250, 9_900, 10_508, N, N, N);
        }
        return h(4181, 10_250, 9_900, 10_508, N, N, N); // organic templates (ESTIMATE)
    }
    h(N, N, N, N, N, N, N)
}

/// Fallback for a body tissue without a palette material, by tissue name (raws templates).
pub fn tissue_heat(tissue: &str) -> MaterialHeat {
    let u = tissue.to_ascii_uppercase();
    if u.contains("FAT") {
        h(4181, 10_250, 9_900, 10_338, 10_078, N, N)
    } else if u.contains("HAIR") || u.contains("FEATHER") || u.contains("CHITIN") || u.contains("SCALE") {
        h(420, 10_250, 9_900, 10_508, N, N, N)
    } else if u.contains("BONE") || u.contains("NAIL") || u.contains("TOOTH") || u.contains("HORN") || u.contains("CLAW") {
        h(1000, 10_250, 9_900, 10_508, N, N, N)
    } else {
        h(4181, 10_250, 9_900, 10_508, N, N, N)
    }
}

/// Stable at temperature `t`: melting, boiling, ignition and heat damage all above it, and cold
/// damage below it (w:Fire-safe, w:Magma-safe "Game calculations"). A fixed temperature that
/// does not itself harm the material makes it stable at any temperature.
pub fn stable_at(h: &MaterialHeat, t: i32) -> bool {
    if is_set(h.mat_fixed_temp) {
        return stable_plain(h, h.mat_fixed_temp);
    }
    stable_plain(h, t)
}

fn stable_plain(h: &MaterialHeat, t: i32) -> bool {
    let above = |p: i32| !is_set(p) || p > t;
    above(h.melting_point) && above(h.boiling_point) && above(h.ignite_point) && above(h.heatdam_point) && (!is_set(h.colddam_point) || h.colddam_point < t)
}

/// Fire-safe: stable at 11000 U (w:Fire-safe). Fixed-temperature materials (nether-cap) are not
/// recognized as fire-safe building materials by the game, so they count by their points.
pub fn fire_safe(h: &MaterialHeat) -> bool {
    stable_plain(h, FIRE_SAFE_TEMP)
}

/// Magma-safe: stable at 12000 U (w:Magma-safe).
pub fn magma_safe(h: &MaterialHeat) -> bool {
    stable_plain(h, MAGMA_SAFE_TEMP)
}

/// Survives magma: magma-safe, or pinned to a harmless fixed temperature (nether-cap, w:Magma-safe).
pub fn survives_magma(h: &MaterialHeat) -> bool {
    stable_at(h, MAGMA_TEMP)
}

/// Does fire at `t` damage (ignite or heat-damage) this material?
pub fn burns_at(h: &MaterialHeat, t: i32) -> bool {
    if is_set(h.mat_fixed_temp) {
        return false;
    }
    (is_set(h.ignite_point) && h.ignite_point <= t) || (is_set(h.heatdam_point) && h.heatdam_point < t)
}

/// Can this material catch fire at all?
pub fn flammable(h: &MaterialHeat) -> bool {
    is_set(h.ignite_point) && !is_set(h.mat_fixed_temp)
}
