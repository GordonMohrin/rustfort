//! Item categories (stockpile categories, hauling labors, food/drink classes).

/// Top-level stockpile category (DF names in `rf_core::StockpileSettings::categories`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Cat {
    Animals,
    Food,
    Furniture,
    Corpses,
    Refuse,
    Stone,
    Ammo,
    Coins,
    BarsBlocks,
    Gems,
    FinishedGoods,
    Leather,
    Cloth,
    Wood,
    Weapons,
    Armor,
    Sheet,
}

impl Cat {
    pub fn bit(self) -> u32 {
        1 << self as u32
    }
    pub fn from_df(s: &str) -> Option<Cat> {
        use Cat::*;
        Some(match s {
            "animals" => Animals,
            "food" => Food,
            "furniture" => Furniture,
            "corpses" => Corpses,
            "refuse" => Refuse,
            "stone" => Stone,
            "ammo" => Ammo,
            "coins" => Coins,
            "bars_blocks" => BarsBlocks,
            "gems" => Gems,
            "finished_goods" => FinishedGoods,
            "leather" => Leather,
            "cloth" => Cloth,
            "wood" => Wood,
            "weapons" => Weapons,
            "armor" => Armor,
            "sheet" => Sheet,
            _ => return None,
        })
    }
    pub fn of_kind(kind: &str) -> Cat {
        use Cat::*;
        match kind {
            "BOULDER" => Stone,
            "ROUGH" | "SMALLGEM" => Gems,
            "WOOD" => Wood,
            "BAR" | "BLOCKS" => BarsBlocks,
            "DRINK" | "PLANT" | "MEAT" | "FISH" | "FISH_RAW" | "FOOD" | "CHEESE" | "EGG" | "PLANT_GROWTH" | "SEEDS"
            | "POWDER_MISC" | "LIQUID_MISC" | "GLOB" => Food,
            "CORPSE" | "CORPSEPIECE" => Corpses,
            "REMAINS" | "VERMIN" => Refuse,
            "WEAPON" | "TRAPCOMP" => Weapons,
            "ARMOR" | "SHOES" | "HELM" | "GLOVES" | "PANTS" | "SHIELD" => Armor,
            "AMMO" => Ammo,
            "COIN" => Coins,
            "CLOTH" | "THREAD" => Cloth,
            "SKIN_TANNED" => Leather,
            "SHEET" => Sheet,
            "BED" | "CHAIR" | "TABLE" | "DOOR" | "BOX" | "BARREL" | "BIN" | "CABINET" | "ARMORSTAND" | "WEAPONRACK"
            | "STATUE" | "COFFIN" | "BUCKET" | "ANVIL" | "HATCH_COVER" | "GRATE" | "QUERN" | "MILLSTONE" | "WINDOW"
            | "CAGE" | "TRACTION_BENCH" | "SLAB" | "FLOODGATE" | "ANIMALTRAP" | "CHAIN" | "PIPE_SECTION" | "BAG"
            | "WHEELBARROW" | "TOOL" => Furniture,
            _ => FinishedGoods,
        }
    }
    /// DF hauling labor bit for items of this category.
    pub fn haul_labor(self) -> u32 {
        use crate::labor::*;
        match self {
            Cat::Stone => HAUL_STONE,
            Cat::Wood => HAUL_WOOD,
            Cat::Food => HAUL_FOOD,
            Cat::Corpses => HAUL_BODY,
            Cat::Refuse => HAUL_REFUSE,
            Cat::Furniture => HAUL_FURNITURE,
            Cat::Animals => HAUL_ANIMALS,
            _ => HAUL_ITEM,
        }
    }
}

/// Item kinds a dwarf can eat directly (wiki "Food").
pub fn is_edible(kind: &str) -> bool {
    matches!(kind, "PLANT" | "MEAT" | "FISH" | "FOOD" | "CHEESE" | "EGG" | "PLANT_GROWTH")
}

pub fn is_drink(kind: &str) -> bool {
    kind == "DRINK"
}
