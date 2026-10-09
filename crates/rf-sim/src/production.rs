//! Production: workshop jobs, farming, wells, manager work orders and building construction.
//!
//! # DF rules implemented (wiki: Brewing, Kitchen, Farming, Seed, Well, Manager)
//! - **Brew drink from plant** (still, labor Brewing): 1 unrotten brewable plant + 1 empty
//!   barrel or large pot -> 5 drinks of the plant's DRINK material inside the container plus 1
//!   seed (reaction raw `BREW_DRINK_FROM_PLANT`). Without an empty container nothing is brewed.
//!   "Brew drink from fruit" does the same with a brewable fruit growth (no seed).
//! - **Prepare meal** (kitchen, labor Cooking): easy 2 / fine 3 / lavish 4 ingredient stacks;
//!   the meal stack is the sum of the ingredient stacks; the meal takes the material of the last
//!   ingredient. Cooking destroys seeds. Brewable plants are only cooked when
//!   [`Sim::set_cook_brewables`] allows it (default **off**: Gordon's rule, plump helmets are
//!   for brewing).
//! - **Carpenter / mason / craftsdwarf**: furniture and containers from 1 log or 1 boulder,
//!   4 blocks per boulder, rock large pots at the craftsdwarf's workshop. Economic stone (ore)
//!   is not used.
//! - **Farming** (labor Farming (fields)): one seed per farm tile, growth = the plant's
//!   `growdur` (plump helmet 30 000 ticks), only in the plant's seasons; when a season starts in
//!   which the planted crop cannot grow, it is destroyed. Harvest yield follows the wiki's
//!   procedure (1 plant + 50 % chances on skill-in-5/10/15/20/75 checks, planter skill at
//!   planting time; no fertilizer yet); plots on soil layers underground are "poor" (-75 %, at
//!   least 1). Only soil/grass tiles of a plot are farmed. Seeds come back from brewing and from
//!   eating plants raw, capped at 200 seeds per species.
//! - **Wells** work when water at least 3/7 deep (or any 7/7) lies directly below; dwarves drink
//!   from them when there is no booze (DF prefers alcohol).
//! - **Work orders**: checked once a day; conditions are compared against the item stock;
//!   matching workshops get jobs queued (at most 10 per workshop, like DF). `amount_left` drops
//!   when a job finishes; repeating orders reset at the start of their period; finished one-time
//!   orders are removed. Workshop queues (incl. repeat jobs) run in order, one worker per
//!   workshop.
//! - **Construction**: buildings placed with `place_building` get a Construct Building job; the
//!   worker hauls the material (boulder/log/block for workshops, the furniture item for
//!   furniture) and builds it.
//!
//! # Estimates (not on the wiki)
//! - Work times: brewing/cooking 300 ticks, furniture 400, blocks/pots 300, planting and
//!   harvesting 100, building a workshop 300, installing furniture 100, a farm plot 200; all
//!   scaled by skill like the v0.1 designation jobs.
//! - Cavern soil is treated like any soil (all subterranean soil plots are "poor"); crops
//!   planted before the export are assumed half grown and planted by a Dabbling farmer.
//! - Brewable plants with a FRUIT growth are brewed from the fruit, others from the plant
//!   (the exported plant raws have no per-material DRINK_MAT flag in rf-core yet).
//! - Leaves and flowers are not food; other growths (fruit, bulbs, pods) are eaten raw.
//! - Construction labor: stone/block material -> Masonry, wood -> Carpentry, furniture ->
//!   Furniture hauling, farm plot -> Farming.
//! - Without exported plant raws (v0.1 exports) plant facts come from the material palette
//!   (`PLANT:X:DRINK` = brewable, `PLANT:X:SEED` = has seeds) plus the vanilla seasons/growdur
//!   of the six underground crops; other crops grow all year in 30 000 ticks (DF default
//!   GROWDUR 300).
//! - Without exported work orders the sim adds a few fallback standing orders (brew when drinks
//!   are low, cook, make barrels/pots); this is announced and listed in `production_notes()`.
//!   Without exported crops, farm plots get crops from the available seeds (also announced).

use crate::path::Goals;
use crate::rng::Rng;
use crate::{labor, manhattan, AnnouncementKind, Cat, ItemMeta, Sim, Task, NONE};
use rf_core::{Building, BuildingJob, Item, ItemFlags, MaterialInfo, OrderCondition, Pos, TtMaterial, WorkOrder, NO_MAT};
use std::collections::HashMap;

/// How often (ticks) farms, workshop queues and the stock census are refreshed.
pub const PROD_PERIOD: u64 = 100;
/// DF: at most 10 jobs in a workshop queue.
pub const MAX_QUEUE: usize = 10;
/// DF: seed-producing activities stop at 200 seeds of a species (wiki "Seed").
pub const SEED_CAP: i32 = 200;

/// Production class of an item (cached per item).
pub mod pc {
    pub const NONE: u8 = 0;
    pub const PLANT: u8 = 1;
    pub const GROWTH: u8 = 2;
    pub const SEED: u8 = 3;
    pub const LOG: u8 = 4;
    pub const BOULDER: u8 = 5;
    pub const CONTAINER: u8 = 6;
    pub const DRINK: u8 = 7;
    pub const MEAL: u8 = 8;
    pub const COOKABLE: u8 = 9;
    pub const BLOCKS: u8 = 10;
    pub const BUCKET: u8 = 11;
    pub const CHAIN: u8 = 12;
    pub const MECHANISM: u8 = 13;
    pub const FURNITURE: u8 = 14;
    // [industry] reagent classes of the furnace/forge/animal/cloth industries.
    pub const ORE: u8 = 15;
    pub const BAR: u8 = 16;
    pub const POWDER: u8 = 17;
    pub const CORPSE: u8 = 18;
    pub const BODYPART: u8 = 19;
    pub const LEATHER: u8 = 20;
    pub const CLOTH: u8 = 21;
    pub const THREAD: u8 = 22;
    pub const BAG: u8 = 23;
}

const PART_STRUCT: u8 = 0;
const PART_DRINK: u8 = 1;
const PART_SEED: u8 = 2;
const PART_WOOD: u8 = 3;
const PART_FRUIT: u8 = 4;
const PART_LEAF: u8 = 5;
const PART_OTHER: u8 = 6;

pub const NO_PLANT: u16 = u16::MAX;
/// Construction need: any building material the worker's labors allow (boulder/block = Masonry,
/// log = Carpentry).
const ANY_MAT: u8 = 200;

/// What the simulation knows about a plant species.
#[derive(Clone, Debug, Default)]
pub struct PlantInfo {
    pub token: String,
    pub name: String,
    pub brewable: bool,
    pub edible_raw: bool,
    pub edible_cooked: bool,
    pub has_seed: bool,
    pub underground: bool,
    pub aboveground: bool,
    pub is_tree: bool,
    /// Bit 0 spring .. bit 3 winter.
    pub seasons: u8,
    pub growdur: i32,
    pub mat_struct: u16,
    pub mat_drink: u16,
    pub mat_seed: u16,
    /// Has a FRUIT growth material in the palette. A brewable plant with fruit is brewed from
    /// the fruit (DF puts the DRINK_MAT reaction product on the fruit material), otherwise from
    /// the plant itself.
    pub fruit: bool,
}

impl PlantInfo {
    /// The plant item itself can be brewed ("Brew drink from plant").
    pub fn brew_plant(&self) -> bool {
        self.brewable && !self.fruit && self.mat_drink != NO_MAT
    }
    /// Its fruit can be brewed ("Brew drink from fruit").
    pub fn brew_fruit(&self) -> bool {
        self.brewable && self.fruit && self.mat_drink != NO_MAT
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input {
    BrewPlant,
    BrewFruit,
    Meal(u8),
    Log,
    Boulder,
    /// [industry] Furnace, forge, animal, cloth and craft jobs (`industry::F_*` family).
    Ind(u8),
}

/// A workshop job the simulation can perform.
pub struct Recipe {
    pub job: &'static str,
    pub reaction: &'static str,
    pub item_type: &'static str,
    pub item_subtype: &'static str,
    pub mat_category: &'static str,
    pub label: &'static str,
    pub workshop: &'static str,
    pub labor: u64, // [industry] u64 labor bits
    pub skill: &'static str,
    pub input: Input,
    pub ticks: u32,
    pub count: u8,
    pub noun: &'static str,
}

macro_rules! r {
    ($job:expr, $re:expr, $it:expr, $sub:expr, $cat:expr, $label:expr, $ws:expr, $lab:expr, $sk:expr, $inp:expr, $t:expr, $n:expr, $noun:expr) => {
        Recipe {
            job: $job,
            reaction: $re,
            item_type: $it,
            item_subtype: $sub,
            mat_category: $cat,
            label: $label,
            workshop: $ws,
            labor: $lab,
            skill: $sk,
            input: $inp,
            ticks: $t,
            count: $n,
            noun: $noun,
        }
    };
}

use labor::{BREW, CARPENTER, COOK, MASON, STONE_CRAFT};

/// All workshop jobs (index = recipe id). Labels follow DF's workshop menus.
pub const RECIPES: &[Recipe] = &[
    r!("CustomReaction", "BREW_DRINK_FROM_PLANT", "DRINK", "", "", "Brew drink from plant", "Still", BREW, "BREWING", Input::BrewPlant, 300, 5, "drink"),
    r!("CustomReaction", "BREW_DRINK_FROM_PLANT_GROWTH", "DRINK", "", "", "Brew drink from fruit", "Still", BREW, "BREWING", Input::BrewFruit, 300, 5, "drink"),
    r!("PrepareMeal", "", "FOOD", "EASY", "", "Prepare easy meal", "Kitchen", COOK, "COOK", Input::Meal(2), 300, 1, "biscuits"),
    r!("PrepareMeal", "", "FOOD", "FINE", "", "Prepare fine meal", "Kitchen", COOK, "COOK", Input::Meal(3), 300, 1, "stew"),
    r!("PrepareMeal", "", "FOOD", "LAVISH", "", "Prepare lavish meal", "Kitchen", COOK, "COOK", Input::Meal(4), 300, 1, "roast"),
    r!("ConstructBarrel", "", "BARREL", "", "wood", "Make wooden barrel", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "barrel"),
    r!("ConstructBin", "", "BIN", "", "wood", "Make wooden bin", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "bin"),
    r!("ConstructBed", "", "BED", "", "wood", "Construct wooden bed", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "bed"),
    r!("ConstructTable", "", "TABLE", "", "wood", "Construct wooden table", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "table"),
    r!("ConstructThrone", "", "CHAIR", "", "wood", "Construct wooden chair", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "chair"),
    r!("ConstructDoor", "", "DOOR", "", "wood", "Construct wooden door", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "door"),
    r!("ConstructCabinet", "", "CABINET", "", "wood", "Construct wooden cabinet", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "cabinet"),
    r!("ConstructChest", "", "BOX", "", "wood", "Construct wooden chest", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "chest"),
    r!("ConstructBucket", "", "BUCKET", "", "wood", "Make wooden bucket", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "bucket"),
    r!("ConstructTable", "", "TABLE", "", "stone", "Construct rock table", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "table"),
    r!("ConstructThrone", "", "CHAIR", "", "stone", "Construct rock throne", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "throne"),
    r!("ConstructDoor", "", "DOOR", "", "stone", "Construct rock door", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "door"),
    r!("ConstructCabinet", "", "CABINET", "", "stone", "Construct rock cabinet", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "cabinet"),
    r!("ConstructChest", "", "BOX", "", "stone", "Construct rock coffer", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "coffer"),
    r!("ConstructCoffin", "", "COFFIN", "", "stone", "Construct rock coffin", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "coffin"),
    r!("ConstructBlocks", "", "BLOCKS", "", "stone", "Make rock blocks", "Masons", MASON, "MASONRY", Input::Boulder, 300, 4, "block"),
    r!("MakeTool", "", "TOOL", "ITEM_TOOL_LARGE_POT", "stone", "Make rock large pot", "Craftsdwarfs", STONE_CRAFT, "STONECRAFT", Input::Boulder, 300, 1, "large pot"),
    // [industry] furnaces, forge, animal/cloth industries and mason/carpenter extras.
    r!("ConstructSlab", "", "SLAB", "", "stone", "Construct rock slab", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "slab"),
    r!("ConstructWeaponRack", "", "WEAPONRACK", "", "stone", "Construct rock weapon rack", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "weapon rack"),
    r!("ConstructArmorStand", "", "ARMORSTAND", "", "stone", "Construct rock armor stand", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "armor stand"),
    r!("ConstructHatchCover", "", "HATCH_COVER", "", "stone", "Construct rock hatch cover", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "hatch cover"),
    r!("ConstructGrate", "", "GRATE", "", "stone", "Construct rock grate", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "grate"),
    r!("ConstructFloodgate", "", "FLOODGATE", "", "stone", "Construct rock floodgate", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "floodgate"),
    r!("ConstructStatue", "", "STATUE", "", "stone", "Construct rock statue", "Masons", MASON, "MASONRY", Input::Boulder, 600, 1, "statue"),
    r!("ConstructQuern", "", "QUERN", "", "stone", "Construct rock quern", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "quern"),
    r!("ConstructMillstone", "", "MILLSTONE", "", "stone", "Construct rock millstone", "Masons", MASON, "MASONRY", Input::Boulder, 400, 1, "millstone"),
    r!("EngraveSlab", "", "SLAB", "", "", "Engrave memorial slab", "Masons", labor::ENGRAVER, "ENGRAVE_STONE", Input::Ind(crate::industry::F_ENGRAVE), 400, 1, "slab"),
    r!("ConstructMechanisms", "", "TRAPPARTS", "", "stone", "Construct rock mechanisms", "Mechanics", labor::MECHANIC, "MECHANICS", Input::Boulder, 300, 1, "mechanism"),
    r!("MakeCage", "", "CAGE", "", "wood", "Make wooden cage", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "cage"),
    r!("ConstructCoffin", "", "COFFIN", "", "wood", "Construct wooden casket", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "casket"),
    r!("ConstructWeaponRack", "", "WEAPONRACK", "", "wood", "Construct wooden weapon rack", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "weapon rack"),
    r!("ConstructArmorStand", "", "ARMORSTAND", "", "wood", "Construct wooden armor stand", "Carpenters", CARPENTER, "CARPENTRY", Input::Log, 400, 1, "armor stand"),
    r!("MakeGoblet", "", "GOBLET", "", "stone", "Make rock mug", "Craftsdwarfs", STONE_CRAFT, "STONECRAFT", Input::Boulder, 300, 1, "mug"),
    r!("MakeCrafts", "", "", "", "stone", "Make rock crafts", "Craftsdwarfs", STONE_CRAFT, "STONECRAFT", Input::Ind(crate::industry::F_CRAFTS_STONE), 300, 1, "craft"),
    r!("MakeCrafts", "", "", "", "wood", "Make wooden crafts", "Craftsdwarfs", labor::WOOD_CRAFT, "WOODCRAFT", Input::Ind(crate::industry::F_CRAFTS_WOOD), 300, 1, "craft"),
    r!("MakeCrafts", "", "", "", "bone", "Make bone crafts", "Craftsdwarfs", labor::BONE_CARVE, "BONECARVE", Input::Ind(crate::industry::F_CRAFTS_BONE), 300, 1, "craft"),
    r!("MakeCharcoal", "", "BAR", "", "", "Make charcoal", "WoodFurnace", labor::BURN_WOOD, "WOOD_BURNING", Input::Ind(crate::industry::F_CHARCOAL), 300, 1, "charcoal"),
    r!("MakeAsh", "", "BAR", "", "", "Make ash", "WoodFurnace", labor::BURN_WOOD, "WOOD_BURNING", Input::Ind(crate::industry::F_ASH), 300, 1, "ash"),
    r!("SmeltOre", "", "BAR", "", "", "Smelt ore", "Smelter", labor::SMELT, "SMELT", Input::Ind(crate::industry::F_SMELT), 400, 4, "bars"),
    r!("MakeRawGlass", "", "ROUGH", "", "", "Make raw green glass", "GlassFurnace", labor::GLASSMAKER, "GLASSMAKER", Input::Ind(crate::industry::F_RAWGLASS), 300, 1, "raw glass"),
    r!("MakeWeapon", "", "WEAPON", "ITEM_WEAPON_PICK", "metal", "Forge metal weapon", "MetalsmithsForge", labor::FORGE_WEAPON, "FORGE_WEAPON", Input::Ind(crate::industry::F_FORGE), 500, 1, "weapon"),
    r!("MakeAmmo", "", "AMMO", "ITEM_AMMO_BOLTS", "metal", "Forge metal bolts", "MetalsmithsForge", labor::FORGE_WEAPON, "FORGE_WEAPON", Input::Ind(crate::industry::F_FORGE), 500, 25, "bolts"),
    r!("MakeArmor", "", "ARMOR", "ITEM_ARMOR_BREASTPLATE", "metal", "Forge metal armor", "MetalsmithsForge", labor::FORGE_ARMOR, "FORGE_ARMOR", Input::Ind(crate::industry::F_FORGE), 600, 1, "armor"),
    r!("MakeHelm", "", "HELM", "ITEM_HELM_HELM", "metal", "Forge metal helm", "MetalsmithsForge", labor::FORGE_ARMOR, "FORGE_ARMOR", Input::Ind(crate::industry::F_FORGE), 500, 1, "helm"),
    r!("MakeGloves", "", "GLOVES", "ITEM_GLOVES_GAUNTLETS", "metal", "Forge metal gauntlets", "MetalsmithsForge", labor::FORGE_ARMOR, "FORGE_ARMOR", Input::Ind(crate::industry::F_FORGE), 500, 2, "gauntlets"),
    r!("MakeShoes", "", "SHOES", "ITEM_SHOES_BOOTS", "metal", "Forge metal high boots", "MetalsmithsForge", labor::FORGE_ARMOR, "FORGE_ARMOR", Input::Ind(crate::industry::F_FORGE), 500, 2, "high boots"),
    r!("MakePants", "", "PANTS", "ITEM_PANTS_GREAVES", "metal", "Forge metal greaves", "MetalsmithsForge", labor::FORGE_ARMOR, "FORGE_ARMOR", Input::Ind(crate::industry::F_FORGE), 500, 1, "greaves"),
    r!("MakeShield", "", "SHIELD", "ITEM_SHIELD_SHIELD", "metal", "Forge metal shield", "MetalsmithsForge", labor::FORGE_ARMOR, "FORGE_ARMOR", Input::Ind(crate::industry::F_FORGE), 500, 1, "shield"),
    r!("ConstructBucket", "", "BUCKET", "", "metal", "Forge metal bucket", "MetalsmithsForge", labor::FORGE_FURNITURE, "FORGE_FURNITURE", Input::Ind(crate::industry::F_FORGE), 400, 1, "bucket"),
    r!("MakeCage", "", "CAGE", "", "metal", "Forge metal cage", "MetalsmithsForge", labor::FORGE_FURNITURE, "FORGE_FURNITURE", Input::Ind(crate::industry::F_FORGE), 400, 1, "cage"),
    r!("MakeAnvil", "", "ANVIL", "", "metal", "Forge metal anvil", "MetalsmithsForge", labor::FORGE_FURNITURE, "FORGE_FURNITURE", Input::Ind(crate::industry::F_FORGE), 500, 1, "anvil"),
    r!("MakeChain", "", "CHAIN", "", "metal", "Forge metal chain", "MetalsmithsForge", labor::FORGE_FURNITURE, "FORGE_FURNITURE", Input::Ind(crate::industry::F_FORGE), 400, 1, "chain"),
    r!("MakeGoblet", "", "GOBLET", "", "metal", "Forge metal goblet", "MetalsmithsForge", labor::METAL_CRAFT, "METALCRAFT", Input::Ind(crate::industry::F_FORGE), 400, 1, "goblet"),
    r!("MakeCrafts", "", "", "", "metal", "Forge metal crafts", "MetalsmithsForge", labor::METAL_CRAFT, "METALCRAFT", Input::Ind(crate::industry::F_CRAFTS_METAL), 400, 1, "craft"),
    r!("MakeWeapon", "", "WEAPON", "ITEM_WEAPON_AXE_TRAINING", "wood", "Make wooden training weapon", "Carpenters", CARPENTER, "CARPENTRY", Input::Ind(crate::industry::F_WOOD_ITEM), 400, 1, "training axe"),
    r!("MillPlants", "", "POWDER_MISC", "", "", "Mill plants", "Quern", labor::MILLER, "MILLING", Input::Ind(crate::industry::F_MILL), 300, 1, "flour"),
    r!("MillPlants", "", "POWDER_MISC", "", "", "Mill plants", "Millstone", labor::MILLER, "MILLING", Input::Ind(crate::industry::F_MILL), 200, 1, "flour"),
    r!("ButcherAnimal", "", "", "", "", "Butcher a dead animal", "Butchers", labor::BUTCHER, "BUTCHER", Input::Ind(crate::industry::F_BUTCHER), 400, 1, "meat"),
    r!("TanAHide", "", "SKIN_TANNED", "", "", "Tan a hide", "Tanners", labor::TANNER, "TANNER", Input::Ind(crate::industry::F_TAN), 300, 1, "leather"),
    r!("ConstructBag", "", "BAG", "", "leather", "Make leather bag", "Leatherworks", labor::LEATHER, "LEATHERWORK", Input::Ind(crate::industry::F_LEATHER), 300, 1, "bag"),
    r!("MakeArmor", "", "ARMOR", "ITEM_ARMOR_LEATHER", "leather", "Make leather armor", "Leatherworks", labor::LEATHER, "LEATHERWORK", Input::Ind(crate::industry::F_LEATHER), 400, 1, "armor"),
    r!("MakeGloves", "", "GLOVES", "ITEM_GLOVES_GLOVES", "leather", "Make leather gloves", "Leatherworks", labor::LEATHER, "LEATHERWORK", Input::Ind(crate::industry::F_LEATHER), 300, 2, "gloves"),
    r!("MakeShoes", "", "SHOES", "ITEM_SHOES_SHOES", "leather", "Make leather shoes", "Leatherworks", labor::LEATHER, "LEATHERWORK", Input::Ind(crate::industry::F_LEATHER), 300, 2, "shoes"),
    r!("MakePants", "", "PANTS", "ITEM_PANTS_PANTS", "leather", "Make leather trousers", "Leatherworks", labor::LEATHER, "LEATHERWORK", Input::Ind(crate::industry::F_LEATHER), 300, 1, "trousers"),
    r!("MakeHelm", "", "HELM", "ITEM_HELM_CAP", "leather", "Make leather cap", "Leatherworks", labor::LEATHER, "LEATHERWORK", Input::Ind(crate::industry::F_LEATHER), 300, 1, "cap"),
    r!("MakeQuiver", "", "QUIVER", "", "leather", "Make leather quiver", "Leatherworks", labor::LEATHER, "LEATHERWORK", Input::Ind(crate::industry::F_LEATHER), 300, 1, "quiver"),
    r!("MakeBackpack", "", "BACKPACK", "", "leather", "Make leather backpack", "Leatherworks", labor::LEATHER, "LEATHERWORK", Input::Ind(crate::industry::F_LEATHER), 300, 1, "backpack"),
    r!("WeaveCloth", "", "CLOTH", "", "", "Weave thread into cloth", "Loom", labor::WEAVER, "WEAVING", Input::Ind(crate::industry::F_WEAVE), 300, 1, "cloth"),
    r!("CollectWebs", "", "THREAD", "", "", "Collect webs", "Loom", labor::WEAVER, "WEAVING", Input::Ind(crate::industry::F_WEBS), 100, 1, "thread"),
    r!("ConstructBag", "", "BAG", "", "cloth", "Make cloth bag", "Clothiers", labor::CLOTHESMAKER, "CLOTHESMAKING", Input::Ind(crate::industry::F_CLOTH_ITEM), 300, 1, "bag"),
    r!("MakeArmor", "", "ARMOR", "ITEM_ARMOR_SHIRT", "cloth", "Make cloth shirt", "Clothiers", labor::CLOTHESMAKER, "CLOTHESMAKING", Input::Ind(crate::industry::F_CLOTH_ITEM), 300, 1, "shirt"),
    r!("MakePants", "", "PANTS", "ITEM_PANTS_PANTS", "cloth", "Make cloth trousers", "Clothiers", labor::CLOTHESMAKER, "CLOTHESMAKING", Input::Ind(crate::industry::F_CLOTH_ITEM), 300, 1, "trousers"),
    r!("MakeGloves", "", "GLOVES", "ITEM_GLOVES_GLOVES", "cloth", "Make cloth gloves", "Clothiers", labor::CLOTHESMAKER, "CLOTHESMAKING", Input::Ind(crate::industry::F_CLOTH_ITEM), 300, 2, "gloves"),
    r!("MakeShoes", "", "SHOES", "ITEM_SHOES_SOCKS", "cloth", "Make cloth socks", "Clothiers", labor::CLOTHESMAKER, "CLOTHESMAKING", Input::Ind(crate::industry::F_CLOTH_ITEM), 300, 2, "socks"),
    r!("MakeHelm", "", "HELM", "ITEM_HELM_HOOD", "cloth", "Make cloth hood", "Clothiers", labor::CLOTHESMAKER, "CLOTHESMAKING", Input::Ind(crate::industry::F_CLOTH_ITEM), 300, 1, "hood"),
];

/// Job kinds counted in the report (`jobs_done`): recipes first, then these.
pub const DONE_PLANT: usize = RECIPES.len();
pub const DONE_HARVEST: usize = RECIPES.len() + 1;
pub const DONE_CONSTRUCT: usize = RECIPES.len() + 2;
pub const DONE_KINDS: usize = RECIPES.len() + 3;

fn done_label(k: usize) -> &'static str {
    match k {
        k if k < RECIPES.len() => RECIPES[k].label,
        DONE_PLANT => "Plant seeds",
        DONE_HARVEST => "Harvest plants",
        _ => "Construct building",
    }
}

/// Vanilla facts for the underground crops (raws `plant_standard.txt`: BIOME:SUBTERRANEAN_WATER,
/// seasons, GROWDUR x 100 ticks). Used only when the export has no plant raws.
const UNDERGROUND_CROPS: &[(&str, u8, i32)] = &[
    ("MUSHROOM_HELMET_PLUMP", 0b1111, 30_000),
    ("GRASS_TAIL_PIG", 0b0110, 30_000),
    ("GRASS_WHEAT_CAVE", 0b0110, 50_000),
    ("POD_SWEET", 0b0011, 50_000),
    ("BUSH_QUARRY", 0b0111, 50_000),
    ("MUSHROOM_CUP_DIMPLE", 0b1111, 50_000),
];
/// DF's default GROWDUR (300) in ticks, for crops without an explicit value.
const DEFAULT_GROWDUR: i32 = 30_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WK {
    #[default]
    None,
    Shop {
        bid: i32,
        recipe: u8,
        order_id: i32,
    },
    Plant {
        bid: i32,
        k: u32,
        plant: u16,
    },
    Harvest {
        bid: i32,
        k: u32,
    },
    Construct {
        bid: i32,
    },
}

/// The production job a unit is working on.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WJob {
    pub kind: WK,
    pub site: u32,
    pub items: [i32; 4],
    pub need: [i32; 4],
    pub n: u8,
    pub got: u8,
}

pub(crate) struct Shop {
    pub bid: i32,
    pub bi: usize,
    pub subtype: String,
    pub site: u32,
    pub busy: u32,
    pub blocked_until: u64,
    /// Runnable queue entries: (queue index, recipe).
    pub avail: Vec<(u16, u8)>,
    pub labors: u64, // [industry] u64 labor bits
}

#[derive(Clone, Copy)]
pub(crate) struct FTile {
    pub tile: u32,
    pub plant: u16,
    pub ready: u64,
    pub claimed: bool,
    pub blocked_until: u64,
    pub poor: bool,
    /// Farming skill of the planter when the seed was planted (decides the yield).
    pub skill: u8,
}

pub(crate) struct Farm {
    pub bid: i32,
    pub bi: usize,
    pub under: bool,
    pub tiles: Vec<FTile>,
}

pub(crate) struct Cons {
    pub bid: i32,
    pub bi: usize,
    pub site: u32,
    pub claimed: bool,
    pub blocked_until: u64,
    /// What has to be brought: material classes (with an item kind for furniture).
    pub needs: Vec<(u8, &'static str)>,
    pub labor: u64, // [industry] u64 labor bits
}

/// Stock counts (free items only) used for job availability and order conditions.
#[derive(Clone, Debug, Default)]
pub struct Census {
    pub drinks: i32,
    pub meals: i32,
    pub raw_food: i32,
    pub seeds: i32,
    pub brew_plants: i32,
    pub brew_fruit: i32,
    pub cookables: i32,
    pub logs: i32,
    pub boulders: i32,
    pub blocks: i32,
    pub empty_containers: i32,
    pub buckets: i32,
    pub chains: i32,
    pub mechanisms: i32,
    pub furniture: Vec<(String, i32)>,
    pub seeds_by_plant: Vec<i32>,
    /// All seeds per plant (free or not), for the 200-per-species cap.
    pub seeds_total: Vec<i32>,
}

#[derive(Default)]
pub(crate) struct Prod {
    pub plants: Vec<PlantInfo>,
    pub plant_by_token: HashMap<String, u16>,
    /// Palette index -> (plant index << 8 | part), or u32::MAX.
    pub mat_plant: Vec<u32>,
    pub shops: Vec<Shop>,
    pub farms: Vec<Farm>,
    pub cons: Vec<Cons>,
    pub wells: Vec<i32>,
    pub wj: Vec<WJob>,
    pub census: Census,
    pub epoch: u64,
    pub season: i32,
    pub order_period: HashMap<i32, i64>,
    pub orders_due: bool,
    pub dirty: bool,
    pub known_buildings: usize,
    pub cook_brewables: bool,
    pub notes: Vec<String>,
    pub done: Vec<u64>,
    pub farm_work: Vec<(u32, u32)>,
    pub unsupported: Vec<(String, u32)>,
    /// Ids of the fallback standing orders the sim added (see `set_standing_orders`).
    pub standing: Vec<i32>,
}

/// Stock and production overview (for the bench report and the UI).
#[derive(Clone, Debug, Default)]
pub struct ProductionReport {
    pub drinks: i32,
    pub meals: i32,
    pub raw_food: i32,
    pub seeds: i32,
    pub brewable_plants: i32,
    pub empty_containers: i32,
    pub logs: i32,
    pub boulders: i32,
    pub open_orders: usize,
    pub queued_jobs: usize,
    pub workshops: usize,
    /// Wells with water below (dwarves drink there when there is no booze).
    pub wells: usize,
    pub farm_tiles: usize,
    pub planted_tiles: usize,
    /// Completed jobs since the start, by DF-style label.
    pub jobs_done: Vec<(&'static str, u64)>,
    /// Work orders / workshop jobs the sim cannot perform yet, by job name.
    pub unsupported: Vec<(String, u32)>,
}

fn season_bit(s: i32) -> u8 {
    1 << (s.clamp(0, 3) as u8)
}

fn split_token(tok: &str) -> Option<(&str, &str)> {
    let rest = tok.strip_prefix("PLANT:")?;
    let (id, part) = rest.rsplit_once(':')?;
    Some((id, part))
}

fn part_of(part: &str) -> u8 {
    match part {
        "STRUCTURAL" => PART_STRUCT,
        "DRINK" => PART_DRINK,
        "SEED" => PART_SEED,
        "WOOD" => PART_WOOD,
        "FRUIT" | "BERRY" => PART_FRUIT,
        // Leaves and flowers are not food (vanilla growths have no EDIBLE flags).
        "LEAF" | "LEAVES" | "FLOWER" | "FLOWERS" => PART_LEAF,
        _ => PART_OTHER,
    }
}

fn furniture_kind(kind: &str) -> bool {
    matches!(kind, "BED" | "TABLE" | "CHAIR" | "DOOR" | "CABINET" | "BOX" | "COFFIN" | "ARMORSTAND" | "WEAPONRACK" | "STATUE" | "SLAB")
}

/// Building kind -> furniture item kind it is built from.
fn furniture_item_for(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "Bed" => "BED",
        "Table" => "TABLE",
        "Chair" => "CHAIR",
        "Door" => "DOOR",
        "Cabinet" => "CABINET",
        "Box" => "BOX",
        "Coffin" => "COFFIN",
        "Armorstand" => "ARMORSTAND",
        "Weaponrack" => "WEAPONRACK",
        "Statue" => "STATUE",
        "Slab" => "SLAB",
        _ => return None,
    })
}

/// Harvest yield of one farm tile (wiki "Farming", unfertilized): always 1 plant, plus one more
/// with 50 % chance for each passed check: skill-in-5, skill-in-10, skill-in-15, skill-in-20 and
/// (skill capped at 25)-in-75, where `skill` is the planter's Farming rating at planting time.
pub fn harvest_yield(skill: u32, r: &mut Rng) -> i32 {
    let mut n = 1;
    for (d, cap) in [(5, 5), (10, 10), (15, 15), (20, 20), (75, 25)] {
        let sk = skill.min(cap);
        if r.below(d) < sk && r.chance(1, 2) {
            n += 1;
        }
    }
    n
}

pub(crate) fn compare(op: &str, n: i32, v: i32) -> bool {
    match op {
        "AtLeast" | "GreaterOrEqual" => n >= v,
        "AtMost" | "LessOrEqual" => n <= v,
        "GreaterThan" | "Greater" => n > v,
        "LessThan" | "Less" => n < v,
        "Exactly" | "Equal" => n == v,
        "Not" | "NotEqual" => n != v,
        _ => true,
    }
}

impl Sim {
    // ------------------------------------------------------------------ init

    /// Build plant facts from the exported plant raws, or from the material palette.
    pub(crate) fn init_plants(&mut self) {
        let mut p = Prod { cook_brewables: false, done: vec![0; DONE_KINDS], season: -1, orders_due: true, dirty: true, ..Default::default() };
        let from_raws = !self.world.plant_raws.is_empty();
        for r in &self.world.plant_raws {
            let seasons = (r.spring as u8) | (r.summer as u8) << 1 | (r.autumn as u8) << 2 | (r.winter as u8) << 3;
            let info = PlantInfo {
                token: r.token.clone(),
                name: r.name.clone(),
                brewable: r.brewable,
                edible_raw: r.edible_raw,
                edible_cooked: r.edible_cooked,
                has_seed: r.has_seed,
                underground: r.underground,
                aboveground: r.aboveground || !r.underground,
                is_tree: r.is_tree,
                seasons: if seasons == 0 { 0b1111 } else { seasons },
                growdur: if r.growdur > 0 { r.growdur } else { DEFAULT_GROWDUR },
                mat_struct: r.mat_structural,
                mat_drink: r.mat_drink,
                mat_seed: r.mat_seed,
                fruit: false,
            };
            p.plant_by_token.insert(info.token.clone(), p.plants.len() as u16);
            p.plants.push(info);
        }
        // Palette-derived plants (all plants without raws; fallback only).
        let mut added = 0;
        for (mi, m) in self.world.materials.iter().enumerate() {
            let Some((id, part)) = split_token(&m.token) else { continue };
            let pi = match p.plant_by_token.get(id) {
                Some(&pi) => pi as usize,
                None => {
                    let und = UNDERGROUND_CROPS.iter().find(|c| c.0 == id);
                    p.plant_by_token.insert(id.to_string(), p.plants.len() as u16);
                    p.plants.push(PlantInfo {
                        token: id.to_string(),
                        name: id.to_lowercase().replace('_', " "),
                        edible_raw: id != "MUSHROOM_CUP_DIMPLE" && id != "OATS",
                        edible_cooked: true,
                        underground: und.is_some(),
                        aboveground: und.is_none(),
                        seasons: und.map(|c| c.1).unwrap_or(0b1111),
                        growdur: und.map(|c| c.2).unwrap_or(DEFAULT_GROWDUR),
                        mat_struct: NO_MAT,
                        mat_drink: NO_MAT,
                        mat_seed: NO_MAT,
                        ..Default::default()
                    });
                    added += 1;
                    p.plants.len() - 1
                }
            };
            let pl = &mut p.plants[pi];
            match part_of(part) {
                PART_STRUCT => {
                    if pl.mat_struct == NO_MAT {
                        pl.mat_struct = mi as u16;
                        if !from_raws || pl.name.is_empty() {
                            pl.name = m.name.clone();
                        }
                    }
                }
                PART_DRINK => {
                    if pl.mat_drink == NO_MAT {
                        pl.mat_drink = mi as u16;
                    }
                    if !from_raws {
                        pl.brewable = true;
                    }
                }
                PART_SEED => {
                    if pl.mat_seed == NO_MAT {
                        pl.mat_seed = mi as u16;
                    }
                    if !from_raws {
                        pl.has_seed = true;
                    }
                }
                PART_WOOD => {
                    if !from_raws {
                        pl.is_tree = true;
                    }
                }
                PART_FRUIT => pl.fruit = true,
                _ => {}
            }
        }
        if !from_raws {
            for pl in &mut p.plants {
                if UNDERGROUND_CROPS.iter().any(|c| c.0 == pl.token) {
                    pl.has_seed = true;
                    pl.is_tree = false;
                }
                if pl.is_tree {
                    pl.has_seed = false;
                }
            }
            if !p.plants.is_empty() {
                p.notes.push(format!(
                    "no plant raws in the export: {} plant species derived from the material palette (PLANT:X:DRINK = brewable, PLANT:X:SEED = seeds)",
                    added
                ));
            }
        }
        // Brewable plants without a drink material in the palette (or seeds without a seed
        // material): append palette entries so products have a material.
        let mut new_mats: Vec<MaterialInfo> = Vec::new();
        let base = self.world.materials.len();
        for pl in &mut p.plants {
            if pl.brewable && pl.mat_drink == NO_MAT {
                pl.mat_drink = (base + new_mats.len()) as u16;
                new_mats.push(MaterialInfo {
                    token: format!("PLANT:{}:DRINK", pl.token),
                    name: format!("{} drink", pl.name),
                    rgb: [150, 60, 90],
                    mat_type: -1,
                    mat_index: -1,
                    ..Default::default()
                });
            }
            if pl.has_seed && pl.mat_seed == NO_MAT {
                pl.mat_seed = (base + new_mats.len()) as u16;
                new_mats.push(MaterialInfo {
                    token: format!("PLANT:{}:SEED", pl.token),
                    name: format!("{} seed", pl.name),
                    rgb: [140, 110, 60],
                    mat_type: -1,
                    mat_index: -1,
                    ..Default::default()
                });
            }
        }
        self.world.materials.extend(new_mats);
        p.mat_plant = vec![u32::MAX; self.world.materials.len()];
        for (mi, m) in self.world.materials.iter().enumerate() {
            if let Some((id, part)) = split_token(&m.token) {
                if let Some(&pi) = p.plant_by_token.get(id) {
                    p.mat_plant[mi] = (pi as u32) << 8 | part_of(part) as u32;
                }
            }
        }
        p.census.seeds_by_plant = vec![0; p.plants.len()];
        p.census.seeds_total = vec![0; p.plants.len()];
        self.prod = p;
    }

    /// Production class and plant of an item.
    pub(crate) fn classify(&self, it: &Item) -> (u8, u16) {
        let mp = self.prod.mat_plant.get(it.mat as usize).copied().unwrap_or(u32::MAX);
        let (plant, part) = if mp == u32::MAX { (NO_PLANT, 255u8) } else { ((mp >> 8) as u16, (mp & 0xFF) as u8) };
        let c = match it.kind.as_str() {
            "PLANT" => pc::PLANT,
            "PLANT_GROWTH" => {
                if part == PART_LEAF {
                    pc::NONE
                } else {
                    pc::GROWTH
                }
            }
            "SEEDS" => pc::SEED,
            "WOOD" => pc::LOG,
            "BOULDER" => {
                let ore = self.world.material(it.mat).map(|m| m.is_ore || !m.is_stone).unwrap_or(true);
                if self.world.material(it.mat).map(|m| m.is_ore).unwrap_or(false) {
                    pc::ORE // [industry]
                } else if ore {
                    pc::NONE
                } else {
                    pc::BOULDER
                }
            }
            "BARREL" => pc::CONTAINER,
            "TOOL" if it.subtype == "ITEM_TOOL_LARGE_POT" => pc::CONTAINER,
            "DRINK" => pc::DRINK,
            "FOOD" => pc::MEAL,
            "MEAT" | "FISH" | "CHEESE" | "EGG" => pc::COOKABLE,
            "BLOCKS" => pc::BLOCKS,
            "BUCKET" => pc::BUCKET,
            "CHAIN" => pc::CHAIN,
            "TRAPPARTS" => pc::MECHANISM,
            k if furniture_kind(k) => pc::FURNITURE,
            // [industry]
            "BAR" => pc::BAR,
            "POWDER_MISC" => pc::POWDER,
            "CORPSE" => pc::CORPSE,
            "CORPSEPIECE" => pc::BODYPART,
            "SKIN_TANNED" => pc::LEATHER,
            "CLOTH" => pc::CLOTH,
            "THREAD" => pc::THREAD,
            "BAG" => pc::BAG,
            _ => pc::NONE,
        };
        (c, if c == pc::PLANT || c == pc::GROWTH || c == pc::SEED || c == pc::DRINK { plant } else { NO_PLANT })
    }

    pub(crate) fn plant(&self, p: u16) -> Option<&PlantInfo> {
        self.prod.plants.get(p as usize)
    }

    /// Can a dwarf eat this item as it is (wiki "Food")?
    pub(crate) fn edible_now(&self, idx: usize) -> bool {
        let it = &self.world.items[idx];
        let m = &self.imeta[idx];
        match m.pc {
            pc::PLANT => self.plant(m.plant).map(|p| p.edible_raw).unwrap_or(true),
            // Fruit and other non-leaf growths (bulbs, pods, nuts) are eaten raw.
            pc::GROWTH => true,
            pc::MEAL | pc::COOKABLE => true,
            _ => crate::items::is_edible(&it.kind) && it.kind != "PLANT_GROWTH",
        }
    }

    pub(crate) fn brewable_idx(&self, idx: usize) -> bool {
        let m = &self.imeta[idx];
        let it = &self.world.items[idx];
        if it.flags.rotten {
            return false;
        }
        match m.pc {
            pc::PLANT => self.plant(m.plant).map(|p| p.brew_plant()).unwrap_or(false),
            pc::GROWTH => {
                let part = self.prod.mat_plant.get(it.mat as usize).map(|&mp| (mp & 0xFF) as u8).unwrap_or(255);
                part == PART_FRUIT && self.plant(m.plant).map(|p| p.brew_fruit()).unwrap_or(false)
            }
            _ => false,
        }
    }

    fn cookable_idx(&self, idx: usize) -> bool {
        let m = &self.imeta[idx];
        if self.world.items[idx].flags.rotten {
            return false;
        }
        match m.pc {
            pc::COOKABLE => true,
            pc::PLANT => match self.plant(m.plant) {
                Some(p) => p.edible_cooked && (self.prod.cook_brewables || !p.brew_plant()),
                None => true,
            },
            pc::GROWTH => self.prod.cook_brewables || !self.brewable_idx(idx),
            _ => false,
        }
    }

    pub(crate) fn container_empty(&self, id: i32) -> bool {
        match self.contents.get(&id) {
            None => true,
            Some(v) => v.iter().all(|c| self.item_idx.get(c).map(|&i| self.imeta[i as usize].dead).unwrap_or(true)),
        }
    }

    /// Free for a job: not forbidden, held, built in, reserved, in a job or in a busy container.
    pub(crate) fn item_free(&self, idx: usize, now: u64) -> bool {
        let it = &self.world.items[idx];
        let m = &self.imeta[idx];
        let f = &it.flags;
        if m.dead || m.hauling || m.reserved >= it.stack || f.forbid || f.in_job || f.in_building || f.construction || it.holder >= 0 || it.building >= 0 || m.blocked_until > now || it.stack <= 0 {
            return false;
        }
        if it.container >= 0 {
            match self.item_idx.get(&it.container) {
                Some(&ci) => {
                    let c = &self.world.items[ci as usize];
                    let cm = &self.imeta[ci as usize];
                    if cm.hauling || c.holder >= 0 || c.building >= 0 || c.flags.in_building || c.flags.forbid || cm.dead {
                        return false;
                    }
                }
                None => return false,
            }
        }
        true
    }

    /// Register workshops, farms, wells and unbuilt buildings; set up fallback orders/crops.
    pub(crate) fn init_production(&mut self) {
        for it in &mut self.world.items {
            // DF jobs are not carried over: items reserved by them are free again.
            it.flags.in_job = false;
        }
        self.rebuild_prod_index();
        self.census_now(0);
        self.init_farm_state();
        self.fallback_crops();
        if self.world.work_orders.is_empty() && self.world.citizens().next().is_some() && !self.prod.shops.is_empty() {
            self.fallback_orders();
        }
        let notes = self.prod.notes.clone();
        for n in notes {
            self.announce(0, format!("rf-sim: {n}"), None, AnnouncementKind::Info);
        }
    }

    /// Seeds lying in a farm plot (DF keeps planted seeds as items in the plot) become planted
    /// crops; their growth progress is unknown, so they are assumed half grown (estimate).
    fn init_farm_state(&mut self) {
        let now = self.ticks;
        for fi in 0..self.prod.farms.len() {
            let bid = self.prod.farms[fi].bid;
            for idx in 0..self.world.items.len() {
                let it = &self.world.items[idx];
                if it.building != bid || it.kind != "SEEDS" || self.imeta[idx].dead {
                    continue;
                }
                let Some(t) = self.world.idx(it.pos) else { continue };
                let plant = self.imeta[idx].plant;
                let grow = self.plant(plant).map(|p| p.growdur).unwrap_or(DEFAULT_GROWDUR) as u64;
                let id = it.id;
                if let Some(ft) = self.prod.farms[fi].tiles.iter_mut().find(|ft| ft.tile == t as u32 && ft.plant == NO_PLANT) {
                    ft.plant = plant;
                    ft.ready = now + grow / 2;
                    self.destroy_item(id);
                }
            }
        }
    }

    fn fallback_crops(&mut self) {
        let mut changed = 0;
        for fi in 0..self.prod.farms.len() {
            let bi = self.prod.farms[fi].bi;
            if !self.world.buildings[bi].crops.is_empty() {
                continue;
            }
            let under = self.prod.farms[fi].under;
            let mut crops = vec![String::new(); 4];
            for (s, crop) in crops.iter_mut().enumerate() {
                let mut best = (0i64, NO_PLANT);
                for (pi, p) in self.prod.plants.iter().enumerate() {
                    if !p.has_seed || p.is_tree || p.seasons & season_bit(s as i32) == 0 || (under && !p.underground) || (!under && !p.aboveground) {
                        continue;
                    }
                    let seeds = self.prod.census.seeds_by_plant.get(pi).copied().unwrap_or(0) as i64;
                    if seeds == 0 {
                        continue;
                    }
                    // Prefer brewable crops, then the most seeds.
                    let score = seeds + if p.brewable { 1_000_000 } else { 0 };
                    if score > best.0 {
                        best = (score, pi as u16);
                    }
                }
                if best.1 != NO_PLANT {
                    *crop = self.prod.plants[best.1 as usize].token.clone();
                }
            }
            self.world.buildings[bi].crops = crops;
            changed += 1;
        }
        if changed > 0 {
            self.prod.notes.push(format!("{changed} farm plots had no crop settings in the export: crops chosen from the available seeds"));
        }
    }

    fn fallback_orders(&mut self) {
        let citizens = self.world.citizens().count() as i32;
        let has = |sim: &Sim, sub: &str| sim.prod.shops.iter().filter(|s| s.subtype == sub).count() as i32;
        let (stills, kitchens, carps, crafts) = (has(self, "Still"), has(self, "Kitchen"), has(self, "Carpenters"), has(self, "Craftsdwarfs"));
        let cond = |item_type: &str, sub: &str, flags: &[&str], value: i32| OrderCondition {
            compare: if value < 0 { "AtLeast".into() } else { "LessThan".into() },
            value: value.abs(),
            item_type: item_type.into(),
            item_subtype: sub.into(),
            flags: flags.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let mut added = Vec::new();
        let mut push = |sim: &mut Sim, recipe: usize, amount: i32, freq: &str, conds: Vec<OrderCondition>| {
            let r = &RECIPES[recipe];
            let id = sim.world.work_orders.iter().map(|o| o.id).max().unwrap_or(0) + 1;
            sim.world.work_orders.push(WorkOrder {
                id,
                job: r.job.into(),
                reaction: r.reaction.into(),
                item_type: r.item_type.into(),
                item_subtype: r.item_subtype.into(),
                mat_category: if r.mat_category.is_empty() { vec![] } else { vec![r.mat_category.into()] },
                amount_left: amount,
                amount_total: amount,
                frequency: freq.into(),
                workshop_id: -1,
                active: true,
                validated: true,
                conditions: conds,
                ..Default::default()
            });
            sim.prod.standing.push(id);
            added.push(format!("{} x{amount} {freq}", r.label));
        };
        if stills > 0 {
            // Brew while drinks are low, but only from a surplus of brewable plants (one per
            // citizen in stock), so brewing never eats the food supply: without booze dwarves
            // drink water, without food they starve.
            push(
                self,
                0,
                (2 * stills).max(2),
                "Daily",
                vec![cond("DRINK", "", &[], (10 * citizens).max(50)), cond("PLANT", "", &["BREWABLE"], -citizens.max(5))],
            );
        }
        if kitchens > 0 {
            push(self, 2, (2 * kitchens).max(2), "Daily", vec![cond("FOOD", "", &[], (3 * citizens).max(20))]);
        }
        if carps > 0 {
            push(self, 5, 3, "Monthly", vec![cond("BARREL", "", &["EMPTY"], 5)]);
        }
        if crafts > 0 {
            push(self, 21, 3, "Monthly", vec![cond("TOOL", "ITEM_TOOL_LARGE_POT", &["EMPTY"], 5)]);
        }
        if !added.is_empty() {
            let why = if self.world.work_orders.len() == added.len() { "no manager work orders in the save: " } else { "" };
            self.prod.notes.push(format!("{why}added fallback standing orders (rf-sim heuristic, not DF behavior): {}", added.join(", ")));
        }
    }

    /// Rebuild the workshop / farm / construction / well index from `world.buildings`,
    /// keeping per-building state.
    pub(crate) fn rebuild_prod_index(&mut self) {
        let mut old_shops: HashMap<i32, Shop> = self.prod.shops.drain(..).map(|s| (s.bid, s)).collect();
        let mut old_farms: HashMap<i32, Farm> = self.prod.farms.drain(..).map(|f| (f.bid, f)).collect();
        let mut old_cons: HashMap<i32, Cons> = self.prod.cons.drain(..).map(|c| (c.bid, c)).collect();
        let mut wells_new = Vec::new();
        for bi in 0..self.world.buildings.len() {
            let b = &self.world.buildings[bi];
            let bid = b.id;
            if !b.built {
                if let Some(mut c) = old_cons.remove(&bid) {
                    c.bi = bi;
                    self.prod.cons.push(c);
                } else if let Some(c) = self.new_cons(bi) {
                    self.prod.cons.push(c);
                }
                continue;
            }
            match b.kind.as_str() {
                "Workshop" | "Furnace" => {
                    let site = self.world.idx(b.center).unwrap_or(0) as u32;
                    let shop = match old_shops.remove(&bid) {
                        Some(mut s) => {
                            s.bi = bi;
                            s
                        }
                        None => Shop { bid, bi, subtype: b.subtype.clone(), site, busy: NONE, blocked_until: 0, avail: Vec::new(), labors: 0 },
                    };
                    self.prod.shops.push(shop);
                }
                "FarmPlot" => {
                    let farm = match old_farms.remove(&bid) {
                        Some(mut f) => {
                            f.bi = bi;
                            f
                        }
                        None => self.new_farm(bi),
                    };
                    self.prod.farms.push(farm);
                }
                "Well" => wells_new.push(bi),
                _ => {}
            }
        }
        for bi in wells_new {
            let bid = self.world.buildings[bi].id;
            if !self.prod.wells.contains(&bid) && self.register_well(bi) {
                self.prod.wells.push(bid);
            }
        }
        self.prod.known_buildings = self.world.buildings.len();
        self.prod.dirty = false;
    }

    fn new_farm(&self, bi: usize) -> Farm {
        let b = &self.world.buildings[bi];
        let mut tiles = Vec::new();
        let mut under = false;
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                let p = Pos::new(x, y, b.z);
                let Some(i) = self.world.idx(p) else { continue };
                if !b.contains(p) {
                    continue;
                }
                let t = self.world.tiles[i];
                let info = self.world.tiletypes.get(t.tt);
                let sub = t.flags & rf_core::tile_flags::SUBTERRANEAN != 0;
                under |= sub;
                // DF farms only soil (or muddy stone; mud is not tracked).
                let soil = info.material == TtMaterial::Soil || info.material.is_grass();
                if !info.shape.has_floor() || !soil {
                    continue;
                }
                tiles.push(FTile { tile: i as u32, plant: NO_PLANT, ready: 0, claimed: false, blocked_until: 0, poor: sub && info.material == TtMaterial::Soil && !self.is_cavern_floor(i), skill: 0 }); // [industry] cavern soil is not poor
            }
        }
        Farm { bid: b.id, bi, under, tiles }
    }

    fn new_cons(&self, bi: usize) -> Option<Cons> {
        let b = &self.world.buildings[bi];
        let site = self.world.idx(b.center)? as u32;
        let (needs, labor): (Vec<(u8, &'static str)>, u64) = match b.kind.as_str() {
            "Workshop" | "Furnace" => {
                if b.subtype == "Masons" {
                    (vec![(pc::BOULDER, "")], labor::MASON)
                } else if b.subtype == "Carpenters" {
                    (vec![(pc::LOG, "")], labor::CARPENTER)
                } else {
                    (vec![(ANY_MAT, "")], labor::MASON | labor::CARPENTER)
                }
            }
            "FarmPlot" => (vec![], labor::PLANT),
            "Well" => (vec![(pc::BLOCKS, ""), (pc::BUCKET, ""), (pc::CHAIN, ""), (pc::MECHANISM, "")], labor::MASON),
            k => match furniture_item_for(k) {
                Some(item) => (vec![(pc::FURNITURE, item)], labor::HAUL_FURNITURE),
                None => return None,
            },
        };
        Some(Cons { bid: b.id, bi, site, claimed: false, blocked_until: 0, needs, labor })
    }

    /// Wells draw water when the first liquid below is at least 3/7 deep, or any 7/7 tile lies
    /// directly below (wiki "Well"). Dwarves drink at the well.
    fn register_well(&mut self, bi: usize) -> bool {
        let b = &self.world.buildings[bi];
        let Some(top) = self.world.idx(b.center) else { return false };
        let layer = self.nav.layer;
        let mut i = top;
        let mut ok = false;
        let mut first: Option<u8> = None;
        while i >= layer {
            i -= layer;
            let t = self.world.tiles[i];
            let d = t.liquid_depth();
            if d > 0 && !t.is_magma() {
                if first.is_none() {
                    first = Some(d);
                }
                if d >= 7 || first.unwrap_or(0) >= 3 {
                    ok = true;
                    break;
                }
            }
            if self.nav.shape(t.tt).is_solid() {
                break;
            }
        }
        if !ok {
            return false;
        }
        let (x, y, z) = self.nav.xyz(top);
        for yy in y.saturating_sub(1)..=(y + 1).min(self.nav.dy - 1) {
            for xx in x.saturating_sub(1)..=(x + 1).min(self.nav.dx - 1) {
                let q = self.nav.idx(xx, yy, z);
                if self.nav.walkable(q) && self.water_set.insert(q as u32) {
                    self.water.push(q as u32);
                }
            }
        }
        true
    }

    // ------------------------------------------------------------------ periodic pass

    pub(crate) fn production_pass(&mut self, now: u64) {
        if self.prod.dirty || self.world.buildings.len() != self.prod.known_buildings {
            self.rebuild_prod_index();
        }
        let season = self.world.calendar.month() / 3;
        if season != self.prod.season {
            if self.prod.season >= 0 {
                self.season_change(season);
            }
            self.prod.season = season;
        }
        self.census_now(now);
        self.ind_census(now); // [industry]
        if self.prod.orders_due || now % rf_core::TICKS_PER_DAY as u64 == 0 {
            self.prod.orders_due = false;
            self.run_orders();
        }
        self.refresh_shops();
        self.refresh_farm_work(now);
        self.storage_pass(now); // [industry]
        if !self.prod.farm_work.is_empty() || self.prod.shops.iter().any(|s| !s.avail.is_empty() && s.busy == NONE) || self.prod.cons.iter().any(|c| !c.claimed) {
            self.prod.epoch += 1;
        }
    }

    /// Wiki "Farming": when the season changes and the planted crop cannot grow any more, the
    /// immature plants are destroyed.
    fn season_change(&mut self, season: i32) {
        let bit = season_bit(season);
        let plants = &self.prod.plants;
        for f in &mut self.prod.farms {
            for t in &mut f.tiles {
                if t.plant != NO_PLANT && !t.claimed && plants.get(t.plant as usize).map(|p| p.seasons & bit == 0).unwrap_or(true) {
                    t.plant = NO_PLANT;
                }
            }
        }
    }

    fn census_now(&mut self, now: u64) {
        let mut c = std::mem::take(&mut self.prod.census);
        let np = self.prod.plants.len();
        c.seeds_by_plant.clear();
        c.seeds_by_plant.resize(np, 0);
        c.seeds_total.clear();
        c.seeds_total.resize(np, 0);
        c.furniture.clear();
        let (mut drinks, mut meals, mut raw, mut seeds, mut bp, mut bf, mut cook, mut logs, mut bould, mut blocks, mut cont, mut buck, mut chain, mut mech) =
            (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        for idx in 0..self.world.items.len() {
            let m = self.imeta[idx];
            if m.pc == pc::NONE || m.dead {
                continue;
            }
            if m.pc == pc::SEED && (m.plant as usize) < np {
                c.seeds_total[m.plant as usize] += self.world.items[idx].stack;
            }
            if !self.item_free(idx, now) {
                continue;
            }
            let it = &self.world.items[idx];
            let units = it.stack - m.reserved;
            match m.pc {
                pc::DRINK => drinks += units,
                pc::MEAL => {
                    meals += units;
                }
                pc::SEED => {
                    seeds += units;
                    if (m.plant as usize) < np {
                        c.seeds_by_plant[m.plant as usize] += units;
                    }
                }
                pc::LOG => logs += 1,
                pc::BOULDER => bould += 1,
                pc::BLOCKS => blocks += 1,
                pc::BUCKET => buck += 1,
                pc::CHAIN => chain += 1,
                pc::MECHANISM => mech += 1,
                pc::CONTAINER => {
                    if it.container < 0 && self.container_empty(it.id) {
                        cont += 1;
                    }
                }
                pc::FURNITURE => match c.furniture.iter_mut().find(|(k, _)| *k == it.kind) {
                    Some(e) => e.1 += 1,
                    None => c.furniture.push((it.kind.clone(), 1)),
                },
                _ => {}
            }
            if matches!(m.pc, pc::PLANT | pc::GROWTH | pc::COOKABLE | pc::MEAL) && self.edible_now(idx) && !it.flags.rotten {
                if m.pc != pc::MEAL {
                    raw += units;
                }
            }
            if self.brewable_idx(idx) {
                if m.pc == pc::PLANT {
                    bp += units;
                } else {
                    bf += units;
                }
            }
            if self.cookable_idx(idx) {
                cook += 1;
            }
        }
        c.drinks = drinks;
        c.meals = meals;
        c.raw_food = raw;
        c.seeds = seeds;
        c.brew_plants = bp;
        c.brew_fruit = bf;
        c.cookables = cook;
        c.logs = logs;
        c.boulders = bould;
        c.blocks = blocks;
        c.empty_containers = cont;
        c.buckets = buck;
        c.chains = chain;
        c.mechanisms = mech;
        self.prod.census = c;
    }

    fn inputs_available(&self, r: &Recipe) -> bool {
        let c = &self.prod.census;
        match r.input {
            Input::BrewPlant => c.brew_plants > 0 && c.empty_containers > 0,
            Input::BrewFruit => c.brew_fruit > 0 && c.empty_containers > 0,
            Input::Meal(n) => c.cookables >= n as i32,
            Input::Log => c.logs > 0,
            Input::Boulder => c.boulders > 0,
            Input::Ind(f) => self.ind_available(f, r), // [industry]
        }
    }

    /// Map a DF job description to a recipe (`shop` = workshop subtype it runs in, if known).
    pub(crate) fn recipe_of(job: &str, reaction: &str, item_type: &str, item_subtype: &str, mat_cat: &[String], mat: &str, shop: Option<&str>) -> Option<u8> {
        let job = match job {
            "BrewDrink" => "CustomReaction",
            "ConstructChair" => "ConstructThrone",
            "MakeBarrel" => "ConstructBarrel",
            "MakeBucket" => "ConstructBucket",
            "MakeChest" => "ConstructChest",
            j => j,
        };
        let reaction = if job == "CustomReaction" && reaction.is_empty() { "BREW_DRINK_FROM_PLANT" } else { reaction };
        let mut best: Option<(i32, u8)> = None;
        for (i, r) in RECIPES.iter().enumerate() {
            if r.job != job || (job == "CustomReaction" && r.reaction != reaction) {
                continue;
            }
            if job == "MakeTool" && item_subtype != r.item_subtype {
                continue;
            }
            if !item_type.is_empty() && job != "PrepareMeal" && item_type != r.item_type && job != "CustomReaction" {
                continue;
            }
            let mut score = 0;
            if job == "PrepareMeal" {
                // DF encodes the meal kind in the job's mat_type 2/3/4, which the exporter
                // decodes as the builtin materials CORAL / GLASS_GREEN / GLASS_CLEAR.
                let want = [item_subtype, mat].iter().find_map(|s| {
                    let s = s.to_ascii_uppercase();
                    if s.contains("LAVISH") || s == "4" || s == "GLASS_CLEAR" {
                        Some("LAVISH")
                    } else if s.contains("FINE") || s == "3" || s == "GLASS_GREEN" {
                        Some("FINE")
                    } else if s.contains("EASY") || s == "2" || s == "CORAL" {
                        Some("EASY")
                    } else {
                        None
                    }
                });
                score += if want.unwrap_or("EASY") == r.item_subtype { 10 } else { -10 };
            }
            if !r.mat_category.is_empty() {
                if mat_cat.iter().any(|c| c == r.mat_category) {
                    score += 5;
                } else if !mat_cat.is_empty() {
                    score -= 5;
                }
                let metal_mat = crate::industry::is_metal_token(mat); // [industry]
                let stone_mat = (mat == "INORGANIC" || mat.starts_with("INORGANIC:")) && !metal_mat;
                let wood_mat = mat.starts_with("PLANT:") && mat.ends_with(":WOOD");
                if (stone_mat && r.mat_category == "stone") || (wood_mat && r.mat_category == "wood") || (metal_mat && r.mat_category == "metal") {
                    score += 5;
                }
            }
            if let Some(s) = shop {
                if s == r.workshop {
                    score += 20;
                } else {
                    score -= 20;
                }
            }
            if best.map(|b| score > b.0).unwrap_or(true) {
                best = Some((score, i as u8));
            }
        }
        let (score, i) = best?;
        if shop.is_some() && score < 0 {
            return None;
        }
        Some(i)
    }

    fn recipe_of_job(j: &BuildingJob, shop: &str) -> Option<u8> {
        Sim::recipe_of(&j.job, &j.reaction, &j.item_type, &j.item_subtype, &j.mat_category, &j.mat, Some(shop))
    }

    fn note_unsupported(&mut self, job: &str) {
        if !self.prod.unsupported.iter().any(|(k, _)| k == job) {
            self.prod.unsupported.push((job.to_string(), 1));
        }
    }

    fn refresh_shops(&mut self) {
        for si in 0..self.prod.shops.len() {
            let (bi, bid) = (self.prod.shops[si].bi, self.prod.shops[si].bid);
            let mut avail = std::mem::take(&mut self.prod.shops[si].avail);
            avail.clear();
            let mut labors = 0;
            if self.world.buildings.get(bi).map(|b| b.id) != Some(bid) {
                self.prod.dirty = true;
                self.prod.shops[si].avail = avail;
                continue;
            }
            if self.prod.shops[si].busy == NONE {
                let sub = self.prod.shops[si].subtype.clone();
                let n = self.world.buildings[bi].jobs.len();
                for qi in 0..n.min(MAX_QUEUE) {
                    let j = &self.world.buildings[bi].jobs[qi];
                    if j.suspended {
                        continue;
                    }
                    match Sim::recipe_of_job(j, &sub) {
                        Some(r) => {
                            if self.inputs_available(&RECIPES[r as usize]) {
                                avail.push((qi as u16, r));
                                labors |= RECIPES[r as usize].labor;
                            }
                        }
                        None => {
                            let name = j.job.clone();
                            self.note_unsupported(&name);
                        }
                    }
                }
            }
            let s = &mut self.prod.shops[si];
            s.avail = avail;
            s.labors = labors;
        }
    }

    fn crop_now(&self, bi: usize) -> u16 {
        let s = self.prod.season.clamp(0, 3) as usize;
        match self.world.buildings.get(bi).and_then(|b| b.crops.get(s)) {
            Some(tok) if !tok.is_empty() => self.prod.plant_by_token.get(tok.as_str()).copied().unwrap_or(NO_PLANT),
            _ => NO_PLANT,
        }
    }

    fn refresh_farm_work(&mut self, now: u64) {
        let mut work = std::mem::take(&mut self.prod.farm_work);
        work.clear();
        let bit = season_bit(self.prod.season);
        for (fi, f) in self.prod.farms.iter().enumerate() {
            let crop = self.crop_now(f.bi);
            let can_plant = crop != NO_PLANT
                && self.plant(crop).map(|p| p.seasons & bit != 0 && p.has_seed).unwrap_or(false)
                && self.prod.census.seeds_by_plant.get(crop as usize).copied().unwrap_or(0) > 0;
            for (k, t) in f.tiles.iter().enumerate() {
                if t.claimed || t.blocked_until > now {
                    continue;
                }
                if (t.plant == NO_PLANT && can_plant) || (t.plant != NO_PLANT && now >= t.ready) {
                    work.push((fi as u32, k as u32));
                }
            }
        }
        self.prod.farm_work = work;
    }

    // ------------------------------------------------------------------ work orders

    fn period_index(&self, freq: &str) -> Option<i64> {
        let c = self.world.calendar;
        let y = c.year as i64;
        let m = c.month() as i64;
        Some(match freq {
            "Daily" => (y * 12 + m) * 28 + (c.day() as i64 - 1),
            "Monthly" => y * 12 + m,
            "Seasonally" => y * 4 + m / 3,
            "Yearly" => y,
            _ => return None,
        })
    }

    /// Count the items an order condition refers to, over candidate item indices (all items if
    /// None). [industry] indexed by kind.
    fn count_condition_in(&self, c: &OrderCondition, cands: Option<&[u32]>) -> i32 {
        let pre = format!("{}:", c.mat);
        let mat_idx: Option<Vec<u16>> = if c.mat.is_empty() {
            None
        } else {
            Some(
                self.world
                    .materials
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.token == c.mat || m.token.starts_with(&pre))
                    .map(|(i, _)| i as u16)
                    .collect(),
            )
        };
        let mut n = 0;
        let all = cands.is_none();
        let len = cands.map(|v| v.len()).unwrap_or(self.world.items.len());
        for k in 0..len {
            let idx = if all { k } else { cands.unwrap()[k] as usize };
            let it = &self.world.items[idx];
            let m = &self.imeta[idx];
            if m.dead || it.flags.in_building || it.building >= 0 || it.flags.construction {
                continue;
            }
            if !c.item_type.is_empty() && it.kind != c.item_type {
                continue;
            }
            if !c.item_subtype.is_empty() && it.subtype != c.item_subtype {
                continue;
            }
            if let Some(ms) = &mat_idx {
                if !ms.contains(&it.mat) {
                    continue;
                }
            }
            let mut ok = true;
            for f in &c.flags {
                ok &= match f.as_str() {
                    "BREWABLE" | "brewable" => self.brewable_idx(idx),
                    "UNROTTEN" | "unrotten" => !it.flags.rotten,
                    "EMPTY" | "empty" => self.container_empty(it.id),
                    "COOKABLE" | "EDIBLE_COOKED" | "cookable" => self.cookable_idx(idx),
                    "EDIBLE_RAW" => self.edible_now(idx),
                    // Ore, flux and the like are economic stone; boulders the sim would use are not.
                    "NON_ECONOMIC" => it.kind != "BOULDER" || m.pc == pc::BOULDER,
                    _ => true,
                };
            }
            if ok {
                n += it.stack.max(1);
            }
        }
        n
    }

    fn run_orders(&mut self) {
        let mut done_orders: Vec<i32> = Vec::new();
        // [industry] Many orders share conditions (e.g. "at least 22 boulders"): count each once.
        let mut memo: HashMap<String, i32> = HashMap::new();
        let mut by_kind: HashMap<u64, Vec<u32>> = HashMap::new();
        for (i, it) in self.world.items.iter().enumerate() {
            by_kind.entry(crate::industry::kind_key(&it.kind)).or_default().push(i as u32);
        }
        for oi in 0..self.world.work_orders.len() {
            let (id, freq) = (self.world.work_orders[oi].id, self.world.work_orders[oi].frequency.clone());
            if let Some(p) = self.period_index(&freq) {
                match self.prod.order_period.get(&id).copied() {
                    None => {
                        self.prod.order_period.insert(id, p);
                    }
                    Some(old) if old != p => {
                        self.prod.order_period.insert(id, p);
                        let o = &mut self.world.work_orders[oi];
                        o.amount_left = o.amount_total;
                    }
                    _ => {}
                }
            }
            let o = &self.world.work_orders[oi];
            if o.amount_left <= 0 {
                if self.period_index(&freq).is_none() {
                    done_orders.push(id);
                }
                continue;
            }
            let Some(r) = Sim::recipe_of(&o.job, &o.reaction, &o.item_type, &o.item_subtype, &o.mat_category, &o.mat, None) else {
                let name = o.job.clone();
                self.note_unsupported(&name);
                continue;
            };
            let conds_ok = o.conditions.iter().all(|c| {
                let key = format!("{}|{}|{}|{}", c.item_type, c.item_subtype, c.mat, c.flags.join(","));
                let n = match memo.get(&key) {
                    Some(&n) => n,
                    None => {
                        let cands = if c.item_type.is_empty() { None } else { Some(by_kind.get(&crate::industry::kind_key(&c.item_type)).map(|v| v.as_slice()).unwrap_or(&[])) };
                        let n = self.count_condition_in(c, cands);
                        memo.insert(key, n);
                        n
                    }
                };
                compare(&c.compare, n, c.value)
            }); // [industry] memoized
            if !conds_ok {
                continue;
            }
            let o = &self.world.work_orders[oi];
            let workshop = RECIPES[r as usize].workshop;
            let queued: i32 = self.prod.shops.iter().map(|s| self.world.buildings[s.bi].jobs.iter().filter(|j| j.order_id == id).count() as i32).sum();
            let mut want = o.amount_left - queued;
            if want <= 0 {
                continue;
            }
            let job = BuildingJob {
                job: o.job.clone(),
                reaction: o.reaction.clone(),
                item_type: o.item_type.clone(),
                item_subtype: o.item_subtype.clone(),
                mat: o.mat.clone(),
                mat_category: o.mat_category.clone(),
                repeat: false,
                suspended: false,
                order_id: id,
            };
            let ws_id = o.workshop_id;
            let shops: Vec<usize> = self
                .prod
                .shops
                .iter()
                .filter(|s| if ws_id >= 0 { s.bid == ws_id } else { s.subtype == workshop })
                .map(|s| s.bi)
                .collect();
            if shops.is_empty() {
                continue;
            }
            let mut progress = true;
            while want > 0 && progress {
                progress = false;
                for &bi in &shops {
                    if want > 0 && self.world.buildings[bi].jobs.len() < MAX_QUEUE {
                        self.world.buildings[bi].jobs.push(job.clone());
                        want -= 1;
                        progress = true;
                    }
                }
            }
        }
        if !done_orders.is_empty() {
            self.world.work_orders.retain(|o| !done_orders.contains(&o.id));
            for id in done_orders {
                self.prod.order_period.remove(&id);
            }
        }
    }

    fn order_job_done(&mut self, order_id: i32) {
        if order_id < 0 {
            return;
        }
        if let Some(o) = self.world.work_orders.iter_mut().find(|o| o.id == order_id) {
            o.amount_left = (o.amount_left - 1).max(0);
            if o.amount_left == 0 && !matches!(o.frequency.as_str(), "Daily" | "Monthly" | "Seasonally" | "Yearly") {
                self.world.work_orders.retain(|o| o.id != order_id);
                self.prod.order_period.remove(&order_id);
            }
        }
    }

    // ------------------------------------------------------------------ job search

    fn site_goals(&self, site: u32, c: u32) -> Goals {
        let ok = |q: usize| self.nav.walkable(q) && (c == 0 || self.nav.comp_root(q) == c);
        if ok(site as usize) {
            return Goals::one(site);
        }
        let mut g = Goals::default();
        let (x, y, z) = self.nav.xyz(site as usize);
        for &(ddx, ddy) in &crate::nav::DIRS8 {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= self.nav.dx as isize || ny >= self.nav.dy as isize {
                continue;
            }
            let q = self.nav.idx(nx as usize, ny as usize, z);
            if ok(q) {
                g.push(q as u32);
            }
        }
        g
    }

    /// Find the nearest free item(s) matching `pred` near `site` within component `c`.
    pub(crate) fn find_items(&mut self, site: u32, c: u32, now: u64, want: usize, out: &mut Vec<i32>, pred: impl Fn(&Sim, usize) -> bool) {
        self.stats.prod_item_scans += 1;
        out.clear();
        let sp = self.nav.xyz(site as usize);
        let mut best: Vec<(i32, i32)> = Vec::new();
        for idx in 0..self.world.items.len() {
            if self.imeta[idx].pc == pc::NONE || !pred(self, idx) || !self.item_free(idx, now) {
                continue;
            }
            let it = &self.world.items[idx];
            let Some(t) = self.world.idx(it.pos) else { continue };
            let d = manhattan(sp, self.nav.xyz(t));
            if best.len() == want && d >= best[want - 1].0 {
                continue;
            }
            if self.nav.comp_near(t) != c {
                continue;
            }
            let pos = best.partition_point(|e| *e <= (d, it.id));
            best.insert(pos, (d, it.id));
            best.truncate(want);
        }
        out.extend(best.iter().map(|e| e.1));
    }

    pub(crate) fn try_prod_timed(&mut self, s: usize, now: u64) -> bool {
        if self.ai[s].labors & labor::ANY_PROD == 0 {
            return false;
        }
        let t = std::time::Instant::now();
        self.stats.prod_searches += 1;
        let r = self.try_prod(s, now);
        self.stats.prod_search_secs += t.elapsed().as_secs_f64();
        r
    }

    /// Look for workshop, farm or construction work (called by idle workers).
    pub(crate) fn try_prod(&mut self, s: usize, now: u64) -> bool {
        let labors = self.ai[s].labors;
        if labors & labor::ANY_PROD == 0 {
            return false;
        }
        let pos = self.ai[s].pos as usize;
        let c = self.nav.comp_near(pos);
        if c == 0 {
            return false;
        }
        let up = self.nav.xyz(pos);
        // 0 = shop (index, avail entry), 1 = farm work entry, 2 = construction.
        let mut best = (i32::MAX, 0u8, 0usize, 0usize);
        for (si, sh) in self.prod.shops.iter().enumerate() {
            if sh.busy != NONE || sh.blocked_until > now || sh.labors & labors == 0 {
                continue;
            }
            let d = manhattan(up, self.nav.xyz(sh.site as usize));
            if d >= best.0 {
                continue;
            }
            if let Some(k) = sh.avail.iter().position(|&(_, r)| RECIPES[r as usize].labor & labors != 0) {
                if self.site_goals(sh.site, c).n > 0 {
                    best = (d, 0, si, k);
                }
            }
        }
        if labors & labor::PLANT != 0 {
            for (wi, &(fi, k)) in self.prod.farm_work.iter().enumerate() {
                let t = &self.prod.farms[fi as usize].tiles[k as usize];
                if t.claimed || t.blocked_until > now {
                    continue;
                }
                let d = manhattan(up, self.nav.xyz(t.tile as usize));
                if d < best.0 && self.nav.walkable(t.tile as usize) && self.nav.comp_root(t.tile as usize) == c {
                    best = (d, 1, wi, 0);
                }
            }
        }
        for (ci, cs) in self.prod.cons.iter().enumerate() {
            if cs.claimed || cs.blocked_until > now || cs.labor & labors == 0 {
                continue;
            }
            let d = manhattan(up, self.nav.xyz(cs.site as usize));
            if d < best.0 && self.site_goals(cs.site, c).n > 0 {
                best = (d, 2, ci, 0);
            }
        }
        if best.0 == i32::MAX {
            return false;
        }
        match best.1 {
            0 => self.claim_shop(s, best.2, best.3, c, now),
            1 => {
                let (fi, k) = self.prod.farm_work[best.2];
                self.claim_farm(s, fi as usize, k as usize, c, now)
            }
            _ => self.claim_cons(s, best.2, c, now),
        }
    }

    fn start_wjob(&mut self, s: usize, wj: WJob, now: u64) {
        for k in 0..wj.n as usize {
            if let Some(&i) = self.item_idx.get(&wj.items[k]) {
                self.imeta[i as usize].reserved += wj.need[k];
            }
        }
        if self.prod.wj.len() <= s {
            self.prod.wj.resize(self.ai.len().max(s + 1), WJob::default());
        }
        self.prod.wj[s] = wj;
        self.next_wjob_step(s, now);
    }

    /// Fetch the next reagent, or walk to the site and work.
    fn next_wjob_step(&mut self, s: usize, now: u64) {
        let wj = self.prod.wj[s];
        if wj.got < wj.n {
            let id = wj.items[wj.got as usize];
            self.ai[s].task = Task::Fetch(id);
            let g = self.item_goals(id);
            self.request_path(s, g, now);
        } else {
            self.ai[s].task = Task::GoSite;
            let c = self.nav.comp_near(self.ai[s].pos as usize);
            let g = self.site_goals(wj.site, c);
            self.request_path(s, g, now);
        }
    }

    fn claim_shop(&mut self, s: usize, si: usize, k: usize, c: u32, now: u64) -> bool {
        let (_qi, r) = self.prod.shops[si].avail[k];
        let rec = &RECIPES[r as usize];
        let site = self.prod.shops[si].site;
        let bi = self.prod.shops[si].bi;
        let mut wj = WJob { site, ..Default::default() };
        let mut found: Vec<i32> = Vec::new();
        let qjob = self.world.buildings[bi].jobs.get(_qi as usize).cloned().unwrap_or_default();
        if Sim::recipe_of_job(&qjob, &self.prod.shops[si].subtype) != Some(r) {
            // The queue changed since the last refresh.
            self.prod.shops[si].avail.clear();
            return false;
        }
        let mat_filter: Option<u16> =
            if qjob.mat.is_empty() { None } else { self.world.materials.iter().position(|m| m.token == qjob.mat).map(|m| m as u16) };
        let ok = match rec.input {
            Input::BrewPlant | Input::BrewFruit => {
                let want_pc = if rec.input == Input::BrewPlant { pc::PLANT } else { pc::GROWTH };
                self.find_items(site, c, now, 1, &mut found, |sim, i| sim.imeta[i].pc == want_pc && sim.brewable_idx(i));
                if let Some(&p) = found.first() {
                    wj.items[0] = p;
                    wj.need[0] = 1;
                    self.find_items(site, c, now, 1, &mut found, |sim, i| {
                        sim.imeta[i].pc == pc::CONTAINER && sim.world.items[i].container < 0 && sim.container_empty(sim.world.items[i].id)
                    });
                    if let Some(&b) = found.first() {
                        wj.items[1] = b;
                        wj.need[1] = 1;
                        wj.n = 2;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Input::Meal(n) => {
                self.find_items(site, c, now, n as usize, &mut found, |sim, i| sim.cookable_idx(i));
                if found.len() == n as usize {
                    for (k, &id) in found.iter().enumerate() {
                        let idx = self.item_idx[&id] as usize;
                        wj.items[k] = id;
                        wj.need[k] = self.world.items[idx].stack - self.imeta[idx].reserved;
                    }
                    wj.n = n;
                    true
                } else {
                    false
                }
            }
            Input::Ind(_) => self.ind_claim(s, r, &qjob, site, c, now, &mut wj), // [industry]
            Input::Log | Input::Boulder => {
                let want_pc = if rec.input == Input::Log { pc::LOG } else { pc::BOULDER };
                self.find_items(site, c, now, 1, &mut found, |sim, i| sim.imeta[i].pc == want_pc && mat_filter.map(|m| sim.world.items[i].mat == m).unwrap_or(true));
                if let Some(&id) = found.first() {
                    wj.items[0] = id;
                    wj.need[0] = 1;
                    wj.n = 1;
                    true
                } else {
                    false
                }
            }
        };
        if !ok {
            // Census said yes but nothing reachable: try again later.
            self.prod.shops[si].blocked_until = now + PROD_PERIOD * 3;
            return false;
        }
        wj.kind = WK::Shop { bid: self.prod.shops[si].bid, recipe: r, order_id: qjob.order_id };
        self.prod.shops[si].busy = s as u32;
        self.prod.shops[si].avail.clear();
        self.start_wjob(s, wj, now);
        true
    }

    fn claim_farm(&mut self, s: usize, fi: usize, k: usize, c: u32, now: u64) -> bool {
        let t = self.prod.farms[fi].tiles[k];
        let bid = self.prod.farms[fi].bid;
        let mut wj = WJob { site: t.tile, ..Default::default() };
        if t.plant == NO_PLANT {
            let crop = self.crop_now(self.prod.farms[fi].bi);
            let mut found = Vec::new();
            self.find_items(t.tile, c, now, 1, &mut found, |sim, i| sim.imeta[i].pc == pc::SEED && sim.imeta[i].plant == crop);
            match found.first() {
                Some(&id) => {
                    wj.items[0] = id;
                    wj.need[0] = 1;
                    wj.n = 1;
                    wj.kind = WK::Plant { bid, k: k as u32, plant: crop };
                }
                None => {
                    self.prod.farms[fi].tiles[k].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
                    return false;
                }
            }
        } else {
            wj.kind = WK::Harvest { bid, k: k as u32 };
        }
        self.prod.farms[fi].tiles[k].claimed = true;
        self.start_wjob(s, wj, now);
        true
    }

    fn claim_cons(&mut self, s: usize, ci: usize, c: u32, now: u64) -> bool {
        let site = self.prod.cons[ci].site;
        let labors = self.ai[s].labors;
        let needs = self.prod.cons[ci].needs.clone();
        let mut wj = WJob { site, ..Default::default() };
        let mut found = Vec::new();
        for (k, &(class, kind)) in needs.iter().enumerate().take(4) {
            match class {
                ANY_MAT => {
                    // Workshop material: whatever the worker's labors allow (stone or wood).
                    let stone = labors & labor::MASON != 0;
                    let wood = labors & labor::CARPENTER != 0;
                    self.find_items(site, c, now, 1, &mut found, |sim, i| {
                        let p = sim.imeta[i].pc;
                        (stone && (p == pc::BOULDER || p == pc::BLOCKS)) || (wood && p == pc::LOG)
                    });
                }
                pc::BOULDER => self.find_items(site, c, now, 1, &mut found, |sim, i| matches!(sim.imeta[i].pc, pc::BOULDER | pc::BLOCKS)),
                pc::FURNITURE => self.find_items(site, c, now, 1, &mut found, |sim, i| sim.imeta[i].pc == pc::FURNITURE && sim.world.items[i].kind == kind),
                cl => self.find_items(site, c, now, 1, &mut found, |sim, i| sim.imeta[i].pc == cl),
            }
            match found.first() {
                Some(&id) if !wj.items[..k].contains(&id) => {
                    wj.items[k] = id;
                    wj.need[k] = 1;
                    wj.n += 1;
                }
                _ => {
                    self.prod.cons[ci].blocked_until = now + PROD_PERIOD * 5;
                    return false;
                }
            }
        }
        wj.kind = WK::Construct { bid: self.prod.cons[ci].bid };
        self.prod.cons[ci].claimed = true;
        self.start_wjob(s, wj, now);
        true
    }

    // ------------------------------------------------------------------ task steps

    /// Arrived next to a reagent: take it (splitting a stack if only part is needed).
    pub(crate) fn prod_arrive_fetch(&mut self, s: usize, id: i32, now: u64) {
        let k = self.prod.wj[s].got as usize;
        let need = self.prod.wj[s].need[k];
        if !self.item_reachable_here(s, id) {
            if let Some(&i) = self.item_idx.get(&id) {
                self.imeta[i as usize].blocked_until = now + rf_core::TICKS_PER_DAY as u64;
            }
            self.cancel_task(s);
            self.ai[s].wake = now + 1;
            return;
        }
        let idx = self.item_idx[&id] as usize;
        self.imeta[idx].reserved = (self.imeta[idx].reserved - need).max(0);
        let stack = self.world.items[idx].stack;
        if stack <= 0 {
            self.cancel_task(s);
            self.ai[s].wake = now + 1;
            return;
        }
        let uid = self.ai[s].id;
        let carried = if stack > need {
            self.world.items[idx].stack -= need;
            let mut it = self.world.items[idx].clone();
            it.id = self.next_item_id;
            self.next_item_id += 1;
            it.stack = need;
            it.container = -1;
            it.holder = uid;
            it.flags.on_ground = false;
            it.flags.in_container = false;
            it.flags.in_inventory = true;
            it.pos = self.world.pos_of(self.ai[s].pos as usize);
            let nid = it.id;
            self.push_item(it);
            if let Some(&ni) = self.item_idx.get(&nid) {
                self.imeta[ni as usize].reserved = 0;
            }
            self.list_consumable(nid);
            nid
        } else {
            let it = &mut self.world.items[idx];
            let (cont, pos, loose) = (it.container, it.pos, it.container < 0 && it.flags.on_ground);
            it.container = -1;
            it.flags.in_container = false;
            it.flags.on_ground = false;
            it.flags.in_inventory = true;
            it.holder = uid;
            if cont >= 0 {
                if let Some(v) = self.contents.get_mut(&cont) {
                    v.retain(|&x| x != id);
                }
            } else if loose {
                if let Some(t) = self.world.idx(pos) {
                    self.pile_occ_add(t as u32, -1);
                }
            }
            self.haul_remove(id);
            id
        };
        self.prod.wj[s].items[k] = carried;
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.push(carried);
        self.ai[s].carrying = carried;
        // Bring it to the site.
        self.ai[s].task = Task::GoSite;
        let c = self.nav.comp_near(self.ai[s].pos as usize);
        let g = self.site_goals(self.prod.wj[s].site, c);
        self.request_path(s, g, now);
    }

    pub(crate) fn prod_arrive_site(&mut self, s: usize, now: u64) {
        let carrying = self.ai[s].carrying;
        let site = self.prod.wj[s].site;
        if carrying >= 0 {
            let drop = if self.nav.walkable(site as usize) { site } else { self.ai[s].pos };
            self.place_reagent(s, carrying, drop);
            self.prod.wj[s].got += 1;
            if self.prod.wj[s].got < self.prod.wj[s].n {
                self.next_wjob_step(s, now);
                return;
            }
        } else if self.prod.wj[s].got < self.prod.wj[s].n {
            self.next_wjob_step(s, now);
            return;
        }
        let wj = self.prod.wj[s];
        let (base, skill) = self.wjob_work(&wj);
        let rating = self.world.units[self.ai[s].uidx].skill(skill);
        let dur = (base as i32 * 10 / (10 + 2 * rating.clamp(0, 20))).max(10) as u64;
        let cap = self.need_cap(s);
        let ai = &mut self.ai[s];
        ai.task = Task::Craft;
        ai.act_until = now + dur;
        ai.wake = (now + dur).min(now + cap);
    }

    fn wjob_work(&self, wj: &WJob) -> (u32, &'static str) {
        match wj.kind {
            WK::Shop { recipe, .. } => (RECIPES[recipe as usize].ticks, RECIPES[recipe as usize].skill),
            WK::Plant { .. } | WK::Harvest { .. } => (100, "PLANT"),
            WK::Construct { bid } => match self.world.building(bid).map(|b| b.kind.as_str()) {
                Some("Workshop") | Some("Furnace") | Some("Well") => (300, "MASONRY"),
                Some("FarmPlot") => (200, "PLANT"),
                _ => (100, "MASONRY"),
            },
            WK::None => (10, ""),
        }
    }

    /// Put a carried reagent down at the job site, reserved for the job.
    fn place_reagent(&mut self, s: usize, id: i32, tile: u32) {
        let uidx = self.ai[s].uidx;
        self.world.units[uidx].inventory.retain(|&x| x != id);
        self.ai[s].carrying = -1;
        let p = self.world.pos_of(tile as usize);
        self.set_item_pos(id, p);
        if let Some(&i) = self.item_idx.get(&id) {
            let it = &mut self.world.items[i as usize];
            it.flags.on_ground = true;
            it.flags.in_inventory = false;
            it.flags.in_job = true;
            it.holder = -1;
            self.imeta[i as usize].hauling = false;
            // Reserved as a whole while it waits at the site (nobody eats or hauls it).
            self.imeta[i as usize].reserved = it.stack.max(1);
            self.pile_occ_add(tile, 1);
        }
    }

    /// Release an item held or placed for a job (cancelled job).
    pub(crate) fn release_reagent(&mut self, id: i32) {
        let Some(&i) = self.item_idx.get(&id) else { return };
        let i = i as usize;
        self.world.items[i].flags.in_job = false;
        self.imeta[i].reserved = 0;
        if !self.imeta[i].dead && self.haulable(i) && !self.is_stored(i) {
            self.haul_add(id);
        }
    }

    /// Remove an item from the world (used up by a job).
    pub(crate) fn destroy_item(&mut self, id: i32) {
        let Some(&i) = self.item_idx.get(&id) else { return };
        let i = i as usize;
        if self.imeta[i].dead {
            return;
        }
        let it = &self.world.items[i];
        let (pos, loose, holder) = (it.pos, it.container < 0 && it.flags.on_ground, it.holder);
        self.imeta[i].dead = true;
        self.items_dirty = true;
        self.haul_remove(id);
        if loose {
            if let Some(t) = self.world.idx(pos) {
                self.pile_occ_add(t as u32, -1);
            }
        }
        if holder >= 0 {
            if let Some(&s) = self.unit_slot.get(&holder) {
                let uidx = self.ai[s].uidx;
                self.world.units[uidx].inventory.retain(|&x| x != id);
            }
        }
    }

    /// Append an item with its sim metadata (no pile/haul bookkeeping).
    pub(crate) fn push_item(&mut self, it: Item) -> usize {
        let (pcl, plant) = self.classify(&it);
        let id = it.id;
        self.next_item_id = self.next_item_id.max(id + 1);
        self.world.items.push(it);
        let idx = self.world.items.len() - 1;
        let cat = Cat::of_kind(&self.world.items[idx].kind);
        self.imeta.push(ItemMeta { reserved: 0, hauling: false, dead: false, cat, blocked_until: 0, pc: pcl, plant });
        self.item_idx.insert(id, idx as u32);
        idx
    }

    /// Put a new food/drink item on the consumable lists.
    pub(crate) fn list_consumable(&mut self, id: i32) {
        let Some(&i) = self.item_idx.get(&id) else { return };
        let i = i as usize;
        if self.imeta[i].pc == pc::DRINK || crate::items::is_drink(&self.world.items[i].kind) {
            self.drinks.push(id);
        } else if self.edible_now(i) {
            self.foods.push(id);
        }
    }

    /// Create a seed of plant `p` at `tile` (if the species is below the seed cap).
    pub(crate) fn make_seed(&mut self, p: u16, tile: usize) {
        let Some(pl) = self.plant(p) else { return };
        if !pl.has_seed || pl.mat_seed == NO_MAT {
            return;
        }
        let (mat, name) = (pl.mat_seed, format!("{} seed", pl.name));
        let n = &mut self.prod.census.seeds_total;
        if (p as usize) < n.len() {
            if n[p as usize] >= SEED_CAP {
                return;
            }
            n[p as usize] += 1;
        }
        self.create_item("SEEDS", mat, tile, name);
    }

    pub(crate) fn prod_complete(&mut self, s: usize, now: u64) {
        let wj = self.prod.wj[s];
        let site = wj.site as usize;
        let mut r = Rng::for_unit(self.seed, s, now, 21);
        match wj.kind {
            WK::Shop { bid, recipe, order_id } => {
                let rec = &RECIPES[recipe as usize];
                match rec.input {
                    Input::BrewPlant | Input::BrewFruit => self.finish_brew(&wj, rec.count as i32, rec.input == Input::BrewPlant),
                    Input::Meal(_) => self.finish_meal(&wj, rec.noun),
                    Input::Ind(_) => self.ind_finish(s, &wj, recipe, now), // [industry]
                    Input::Log | Input::Boulder => {
                        let mat = self.item_idx.get(&wj.items[0]).map(|&i| self.world.items[i as usize].mat).unwrap_or(NO_MAT);
                        self.destroy_item(wj.items[0]);
                        let name = self.world.material(mat).map(|m| m.name.clone()).unwrap_or_default();
                        for _ in 0..rec.count {
                            let id = self.create_item(rec.item_type, mat, site, format!("{name} {}", rec.noun));
                            if let Some(&i) = self.item_idx.get(&id) {
                                self.world.items[i as usize].subtype = rec.item_subtype.to_string();
                                let (pcl, pl) = self.classify(&self.world.items[i as usize]);
                                self.imeta[i as usize].pc = pcl;
                                self.imeta[i as usize].plant = pl;
                            }
                        }
                    }
                }
                self.add_xp(s, rec.skill, 30);
                self.prod.done[recipe as usize] += 1;
                // Remove the queue entry (repeat jobs stay), count the order.
                if let Some(si) = self.prod.shops.iter().position(|sh| sh.bid == bid) {
                    let bi = self.prod.shops[si].bi;
                    let sub = self.prod.shops[si].subtype.clone();
                    if let Some(b) = self.world.buildings.get_mut(bi).filter(|b| b.id == bid) {
                        if let Some(q) = b.jobs.iter().position(|j| j.order_id == order_id && Sim::recipe_of_job(j, &sub) == Some(recipe)) {
                            if !b.jobs[q].repeat {
                                b.jobs.remove(q);
                            }
                        }
                    }
                    self.prod.shops[si].busy = NONE;
                }
                self.order_job_done(order_id);
            }
            WK::Plant { bid, k, plant } if self.plant(plant).map(|p| p.seasons & season_bit(self.prod.season) == 0).unwrap_or(true) => {
                // The season changed while the seed was being brought: keep the seed.
                self.release_reagent(wj.items[0]);
                if let Some(t) = self.prod.farms.iter_mut().find(|f| f.bid == bid).and_then(|f| f.tiles.get_mut(k as usize)) {
                    t.claimed = false;
                }
            }
            WK::Plant { bid, k, plant } => {
                self.destroy_item(wj.items[0]);
                let grow = self.plant(plant).map(|p| p.growdur).unwrap_or(DEFAULT_GROWDUR) as u64;
                let skill = self.world.units[self.ai[s].uidx].skill("PLANT").clamp(0, 255) as u8;
                if let Some(f) = self.prod.farms.iter_mut().find(|f| f.bid == bid) {
                    if let Some(t) = f.tiles.get_mut(k as usize) {
                        t.plant = plant;
                        t.ready = now + grow;
                        t.claimed = false;
                        t.skill = skill;
                    }
                }
                self.add_xp(s, "PLANT", 20);
                self.prod.done[DONE_PLANT] += 1;
            }
            WK::Harvest { bid, k } => {
                let mut n = 0;
                let mut plant = NO_PLANT;
                if let Some(f) = self.prod.farms.iter_mut().find(|f| f.bid == bid) {
                    if let Some(t) = f.tiles.get_mut(k as usize) {
                        plant = t.plant;
                        n = harvest_yield(t.skill as u32, &mut r);
                        if t.poor {
                            // Poor soil (soil layers outside caverns): -75 %, at least 1.
                            n = (n / 4).max(1);
                        }
                        t.plant = NO_PLANT;
                        t.claimed = false;
                    }
                }
                if let Some(pl) = self.plant(plant) {
                    if n > 0 && pl.mat_struct != NO_MAT {
                        let (mat, name) = (pl.mat_struct, pl.name.clone());
                        let id = self.create_item("PLANT", mat, site, name);
                        if let Some(&i) = self.item_idx.get(&id) {
                            self.world.items[i as usize].stack = n;
                        }
                        self.list_consumable(id);
                    }
                }
                self.add_xp(s, "PLANT", 20);
                self.prod.done[DONE_HARVEST] += 1;
            }
            WK::Construct { bid } => {
                self.finish_construction(s, &wj, bid);
                self.prod.done[DONE_CONSTRUCT] += 1;
            }
            WK::None => {}
        }
        self.prod.wj[s] = WJob::default();
        self.ai[s].task = Task::Idle;
        self.prod.epoch += 1;
    }

    fn finish_brew(&mut self, wj: &WJob, count: i32, seed: bool) {
        let (plant_id, barrel) = (wj.items[0], wj.items[1]);
        let Some(&pi) = self.item_idx.get(&plant_id) else { return };
        let plant = self.imeta[pi as usize].plant;
        let Some(pl) = self.plant(plant) else { return };
        let (drink_mat, plant_name) = (pl.mat_drink, pl.name.clone());
        self.destroy_item(plant_id);
        let Some(&bi) = self.item_idx.get(&barrel) else { return };
        let bi = bi as usize;
        let bpos = self.world.items[bi].pos;
        let tile = self.world.idx(bpos).unwrap_or(wj.site as usize);
        self.world.items[bi].flags.in_job = false;
        self.imeta[bi].reserved = 0;
        let dname = self.world.material(drink_mat).map(|m| m.name.clone()).unwrap_or_else(|| format!("{plant_name} drink"));
        let id = self.next_item_id;
        self.next_item_id += 1;
        self.push_item(Item {
            id,
            kind: "DRINK".into(),
            mat: drink_mat,
            pos: bpos,
            stack: count,
            container: barrel,
            holder: -1,
            building: -1,
            desc: dname,
            flags: ItemFlags { in_container: true, ..Default::default() },
            ..Default::default()
        });
        self.contents.entry(barrel).or_default().push(id);
        self.drinks.push(id);
        // The barrel now holds food: it goes to a food stockpile.
        self.haul_remove(barrel);
        self.imeta[bi].cat = Cat::Food;
        if self.haulable(bi) && !self.is_stored(bi) {
            self.haul_add(barrel);
        }
        if seed {
            self.make_seed(plant, tile);
        }
    }

    fn finish_meal(&mut self, wj: &WJob, noun: &str) {
        let mut total = 0;
        let mut mat = NO_MAT;
        for k in 0..wj.n as usize {
            if let Some(&i) = self.item_idx.get(&wj.items[k]) {
                let it = &self.world.items[i as usize];
                total += it.stack.max(1);
                mat = it.mat;
            }
            self.destroy_item(wj.items[k]);
        }
        if total == 0 {
            return;
        }
        let name = self.world.material(mat).map(|m| m.name.clone()).unwrap_or_default();
        let id = self.create_item("FOOD", mat, wj.site as usize, format!("{name} {noun}"));
        if let Some(&i) = self.item_idx.get(&id) {
            self.world.items[i as usize].stack = total;
        }
        self.list_consumable(id);
    }

    fn finish_construction(&mut self, s: usize, wj: &WJob, bid: i32) {
        let Some(bi) = self.world.buildings.iter().position(|b| b.id == bid) else { return };
        let mut mat = NO_MAT;
        let mut skill = "MASONRY";
        let mut ids = Vec::new();
        for k in 0..wj.n as usize {
            let id = wj.items[k];
            if let Some(&i) = self.item_idx.get(&id) {
                let i = i as usize;
                let it = &mut self.world.items[i];
                if k == 0 {
                    mat = it.mat;
                    if it.kind == "WOOD" {
                        skill = "CARPENTRY";
                    }
                }
                it.flags.in_job = false;
                it.flags.in_building = true;
                it.building = bid;
                self.imeta[i].reserved = 0;
                self.haul_remove(id);
                ids.push(id);
            }
        }
        let b = &mut self.world.buildings[bi];
        b.built = true;
        b.stage = 1;
        b.mat = mat;
        b.items.extend(ids);
        let kind = b.kind.clone();
        let center = b.center;
        let owner = b.owner;
        if kind == "Bed" {
            if let Some(t) = self.world.idx(center) {
                self.beds.push(crate::Bed { tile: t as u32, owner, occupant: NONE });
            }
        }
        if !matches!(kind.as_str(), "FarmPlot") {
            self.add_xp(s, skill, 20);
        }
        self.prod.dirty = true;
        self.rebuild_prod_index();
    }

    /// Undo a production job (need interrupt, path failure, death, removed building).
    pub(crate) fn prod_cancel(&mut self, s: usize) {
        let Some(wj) = self.prod.wj.get(s).copied() else { return };
        if wj.kind == WK::None {
            return;
        }
        let carrying = self.ai[s].carrying;
        if carrying >= 0 {
            let here = self.ai[s].pos;
            let uidx = self.ai[s].uidx;
            self.world.units[uidx].inventory.retain(|&x| x != carrying);
            self.ai[s].carrying = -1;
            let p = self.world.pos_of(here as usize);
            self.set_item_pos(carrying, p);
            if let Some(&i) = self.item_idx.get(&carrying) {
                let it = &mut self.world.items[i as usize];
                it.flags.on_ground = true;
                it.flags.in_inventory = false;
                it.holder = -1;
                self.pile_occ_add(here, 1);
            }
            self.release_reagent(carrying);
        }
        for k in 0..wj.n as usize {
            let id = wj.items[k];
            if (k as u8) < wj.got {
                self.release_reagent(id);
            } else if id != carrying {
                if let Some(&i) = self.item_idx.get(&id) {
                    let m = &mut self.imeta[i as usize];
                    m.reserved = (m.reserved - wj.need[k]).max(0);
                }
            }
        }
        match wj.kind {
            WK::Shop { bid, .. } => {
                if let Some(sh) = self.prod.shops.iter_mut().find(|sh| sh.bid == bid) {
                    sh.busy = NONE;
                }
            }
            WK::Plant { bid, k, .. } | WK::Harvest { bid, k } => {
                if let Some(t) = self.prod.farms.iter_mut().find(|f| f.bid == bid).and_then(|f| f.tiles.get_mut(k as usize)) {
                    t.claimed = false;
                }
            }
            WK::Construct { bid } => {
                if let Some(c) = self.prod.cons.iter_mut().find(|c| c.bid == bid) {
                    c.claimed = false;
                }
            }
            WK::None => {}
        }
        self.prod.wj[s] = WJob::default();
        self.prod.epoch += 1;
    }

    /// The site could not be reached: block that job for a day.
    pub(crate) fn prod_block_site(&mut self, s: usize, now: u64) {
        let Some(wj) = self.prod.wj.get(s).copied() else { return };
        let until = now + rf_core::TICKS_PER_DAY as u64;
        match wj.kind {
            WK::Shop { bid, .. } => {
                if let Some(sh) = self.prod.shops.iter_mut().find(|sh| sh.bid == bid) {
                    sh.blocked_until = until;
                }
            }
            WK::Plant { bid, k, .. } | WK::Harvest { bid, k } => {
                if let Some(t) = self.prod.farms.iter_mut().find(|f| f.bid == bid).and_then(|f| f.tiles.get_mut(k as usize)) {
                    t.blocked_until = until;
                }
            }
            WK::Construct { bid } => {
                if let Some(c) = self.prod.cons.iter_mut().find(|c| c.bid == bid) {
                    c.blocked_until = until;
                }
            }
            WK::None => {}
        }
    }

    pub(crate) fn wjob_caption(&self, s: usize) -> (&'static str, Option<Pos>) {
        let Some(wj) = self.prod.wj.get(s) else { return ("No Job", None) };
        let pos = Some(self.world.pos_of(wj.site as usize));
        let cap = match wj.kind {
            WK::Shop { recipe, .. } => RECIPES[recipe as usize].label,
            WK::Plant { .. } => "Plant Seeds",
            WK::Harvest { .. } => "Harvest Plants",
            WK::Construct { .. } => "Construct Building",
            WK::None => "No Job",
        };
        (cap, pos)
    }

    /// Hash of the production state (farm tiles, orders, queues) for `state_hash`.
    pub(crate) fn prod_hash(&self, mix: &mut dyn FnMut(u64)) {
        for f in &self.prod.farms {
            for t in &f.tiles {
                mix(((t.plant as u64) << 48) ^ t.ready ^ ((t.claimed as u64) << 63));
            }
        }
        for o in &self.world.work_orders {
            mix(((o.id as u64) << 32) | o.amount_left as u32 as u64);
        }
        for sh in &self.prod.shops {
            mix(self.world.buildings.get(sh.bi).map(|b| b.jobs.len() as u64).unwrap_or(0) ^ ((sh.busy as u64) << 20));
        }
        for d in &self.prod.done {
            mix(*d);
        }
    }

    // ------------------------------------------------------------------ public API

    /// Gordon's rule by default: brewable plants are not cooked. `true` restores DF's default
    /// kitchen settings (brewables may be cooked).
    pub fn set_cook_brewables(&mut self, on: bool) {
        self.prod.cook_brewables = on;
    }

    /// Fallback standing orders (rf-sim heuristic, not DF behavior): brew when drinks are low,
    /// cook easy meals, make barrels / large pots. Added automatically only when the save has
    /// no work orders at all; this switches them on or off for any world.
    pub fn set_standing_orders(&mut self, on: bool) {
        if on && self.prod.standing.is_empty() {
            self.fallback_orders();
            if let Some(n) = self.prod.notes.last().cloned() {
                self.announce(self.ticks, format!("rf-sim: {n}"), None, AnnouncementKind::Info);
            }
        } else if !on && !self.prod.standing.is_empty() {
            for id in std::mem::take(&mut self.prod.standing) {
                self.remove_work_order(id);
            }
        }
        self.mark_prod_dirty();
    }

    /// Whether the fallback standing orders are active.
    pub fn standing_orders(&self) -> bool {
        !self.prod.standing.is_empty()
    }

    /// Notes about approximations made at load time (fallback orders, derived plant data).
    pub fn production_notes(&self) -> Vec<String> {
        self.prod.notes.clone()
    }

    /// Plant facts the simulation uses.
    pub fn plant_infos(&self) -> &[PlantInfo] {
        &self.prod.plants
    }

    /// Current stock and production counters.
    pub fn production_report(&self) -> ProductionReport {
        let c = &self.prod.census;
        let mut jobs_done = Vec::new();
        for (k, &n) in self.prod.done.iter().enumerate() {
            if n > 0 {
                jobs_done.push((done_label(k), n));
            }
        }
        ProductionReport {
            drinks: c.drinks,
            meals: c.meals,
            raw_food: c.raw_food,
            seeds: c.seeds,
            brewable_plants: c.brew_plants + c.brew_fruit,
            empty_containers: c.empty_containers,
            logs: c.logs,
            boulders: c.boulders,
            open_orders: self.world.work_orders.iter().filter(|o| o.amount_left > 0).count(),
            queued_jobs: self.prod.shops.iter().map(|s| self.world.buildings.get(s.bi).map(|b| b.jobs.len()).unwrap_or(0)).sum(),
            workshops: self.prod.shops.len(),
            wells: self.prod.wells.len(),
            farm_tiles: self.prod.farms.iter().map(|f| f.tiles.len()).sum(),
            planted_tiles: self.prod.farms.iter().map(|f| f.tiles.iter().filter(|t| t.plant != NO_PLANT).count()).sum(),
            jobs_done,
            unsupported: self.prod.unsupported.clone(),
        }
    }

    /// Farm tile states of a farm plot: (position, planted plant token or "", ticks until ripe).
    pub fn farm_tiles(&self, building_id: i32) -> Vec<(Pos, String, i64)> {
        let Some(f) = self.prod.farms.iter().find(|f| f.bid == building_id) else { return Vec::new() };
        f.tiles
            .iter()
            .map(|t| {
                let tok = self.plant(t.plant).map(|p| p.token.clone()).unwrap_or_default();
                (self.world.pos_of(t.tile as usize), tok, if t.plant == NO_PLANT { 0 } else { t.ready as i64 - self.ticks as i64 })
            })
            .collect()
    }

    pub(crate) fn is_underground_plot(&self, building_id: i32) -> Option<bool> {
        if let Some(f) = self.prod.farms.iter().find(|f| f.bid == building_id) {
            return Some(f.under);
        }
        let b = self.world.building(building_id)?;
        let mut under = false;
        for y in b.y1..=b.y2 {
            for x in b.x1..=b.x2 {
                if let Some(t) = self.world.tile(Pos::new(x, y, b.z)) {
                    under |= t.flags & rf_core::tile_flags::SUBTERRANEAN != 0;
                }
            }
        }
        Some(under)
    }

    pub(crate) fn mark_prod_dirty(&mut self) {
        self.prod.dirty = true;
        self.prod.orders_due = true;
        self.prod.epoch += 1;
    }

    pub(crate) fn release_building_items(&mut self, b: &Building) {
        let ids: Vec<i32> = self.world.items.iter().filter(|it| it.building == b.id).map(|it| it.id).collect();
        for id in ids {
            if let Some(&i) = self.item_idx.get(&id) {
                let it = &mut self.world.items[i as usize];
                // Planted seeds in a farm plot just go away with the plot.
                it.building = -1;
                it.flags.in_building = false;
                it.flags.in_job = false;
                it.flags.on_ground = it.container < 0 && it.holder < 0;
                if self.haulable(i as usize) && !self.is_stored(i as usize) {
                    self.haul_add(id);
                }
            }
        }
    }

    /// Cancel every worker whose production job uses building `bid`.
    pub(crate) fn cancel_jobs_at(&mut self, bid: i32) {
        for s in 0..self.prod.wj.len() {
            let uses = match self.prod.wj[s].kind {
                WK::Shop { bid: b, .. } | WK::Plant { bid: b, .. } | WK::Harvest { bid: b, .. } | WK::Construct { bid: b } => b == bid,
                WK::None => false,
            };
            if uses {
                self.cancel_task(s);
                self.ai[s].wake = self.ticks + 1;
            }
        }
    }
}
