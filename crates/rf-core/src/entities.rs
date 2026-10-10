//! Units, items, buildings, plants and materials, as exported from DF.
//!
//! Strings for kinds/types are DF's enum names (e.g. item `kind` = `"BOULDER"`, building
//! `kind` = `"Workshop"`, `subtype` = `"Carpenters"`), so nothing is lost on import and new
//! kinds need no schema change. All IDs are DF IDs; `-1` means "none" like in DF.

use crate::tiles::Pos;
use serde::{Deserialize, Serialize};

/// One entry of the export's material palette (index = palette index used by tiles/items).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialInfo {
    /// DF material type/index pair (`dfhack.matinfo` style).
    pub mat_type: i16,
    pub mat_index: i32,
    /// DF token, e.g. `INORGANIC:GRANITE`, `PLANT:TOWER_CAP:WOOD`, `CREATURE:DWARF:SKIN`.
    pub token: String,
    /// Solid state name, e.g. `granite`.
    pub name: String,
    /// Solid state display color as RGB (from the descriptor color).
    pub rgb: [u8; 3],
    pub is_stone: bool,
    pub is_gem: bool,
    pub is_metal: bool,
    /// Inorganic with `METAL_ORE` (smeltable).
    pub is_ore: bool,
    pub is_soil: bool,
    pub is_wood: bool,
    pub is_glass: bool,
    /// Other DF material flags worth keeping (e.g. `IS_SAND`, `FLUX` reaction class).
    pub tags: Vec<String>,
    /// Mechanical properties for combat (DF material `strength`/`solid_density`; 0 = unknown).
    pub density: i32,
    pub shear_yield: i32,
    pub shear_fracture: i32,
    pub shear_strain_at_yield: i32,
    pub impact_yield: i32,
    pub impact_fracture: i32,
    pub impact_strain_at_yield: i32,
    pub max_edge: i32,
    /// Base material value (DF `material_value`).
    pub value: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Skill {
    /// DF `job_skill` enum name, e.g. `MINING`, `WOODCUTTING`.
    pub id: String,
    /// 0 = Dabbling ... 15+ = Legendary.
    pub rating: i32,
    pub xp: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UnitFlags {
    /// Member of the fort (counts as citizen in DF's sense).
    pub citizen: bool,
    pub resident: bool,
    pub animal: bool,
    pub tame: bool,
    pub dead: bool,
    pub hostile: bool,
    pub visitor: bool,
    pub merchant: bool,
    pub caged: bool,
    pub forest: bool,
    /// Off the map / not present (e.g. on a raid).
    pub inactive: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Unit {
    pub id: i32,
    /// Creature token, e.g. `DWARF`, `CAT`, `GOBLIN`.
    pub race: String,
    /// Caste token, e.g. `MALE`, `FEMALE`.
    pub caste: String,
    /// Display name (translated, as DF shows it), e.g. `Urist McMiner`.
    pub name: String,
    /// Native (dwarvish) full name.
    pub name_native: String,
    /// DF `profession` enum name, e.g. `MINER`, `CHILD`.
    pub profession: String,
    pub custom_profession: String,
    pub pos: Pos,
    pub flags: UnitFlags,
    pub civ_id: i32,
    pub hist_fig_id: i32,
    /// 0 female, 1 male, -1 none.
    pub sex: i8,
    pub age: f32,
    /// DF need counters (ticks since last satisfied). Thresholds: see rf-sim.
    pub hunger_timer: i32,
    pub thirst_timer: i32,
    pub sleepiness_timer: i32,
    pub stress: i32,
    /// [social] DF v50 long-term stress (`personality.longterm_stress`, -50 000..=120 000);
    /// 0 if the export has none (the social sim then estimates it from `stress`).
    pub longterm_stress: i32,
    pub skills: Vec<Skill>,
    /// Enabled labors (DF `unit_labor` enum names), e.g. `MINE`, `HAUL_STONE`.
    pub labors: Vec<String>,
    /// Physical attributes by DF name (`STRENGTH`, `AGILITY`, ...): current value.
    pub phys_attrs: Vec<(String, i32)>,
    /// Mental attributes by DF name (`FOCUS`, `WILLPOWER`, ...).
    pub ment_attrs: Vec<(String, i32)>,
    /// Personality facets by DF name (`ANXIETY_PROPENSITY`, ...): 0..=100.
    pub facets: Vec<(String, i32)>,
    /// Item ids carried/worn.
    pub inventory: Vec<i32>,
    pub squad_id: i32,
    /// Current mood (DF `mood_type` name) or empty.
    pub mood: String,
    /// Body size in cm³ (current).
    pub body_size: i32,
    /// Walking speed from the creature's gait, in DF units (ticks per 100 tiles, 900 = dwarf).
    pub walk_speed: i32,
    /// Personality needs (DF v50), see `rf_core::social`.
    pub needs: Vec<crate::UnitNeed>,
    /// Remembered emotions/thoughts, newest last.
    pub emotions: Vec<crate::Emotion>,
    /// Personality values/beliefs.
    pub values: Vec<crate::ValueStrength>,
    pub relationships: Vec<crate::Relationship>,
    /// Current wounds, see `rf_core::military`.
    pub wounds: Vec<crate::Wound>,
    /// Blood volume (DF `body.blood_count` / `blood_max`).
    pub blood: i32,
    pub blood_max: i32,
    /// Position in its squad, -1 if none.
    pub squad_position: i32,
    /// Hostile-side info: entity/civ id the unit belongs to (invaders), -1 if none.
    pub invader_civ: i32,
    /// [r8] Animals: owner unit of a pet, -1 if none.
    pub pet_owner: i32,
    /// [r8] Animals: DF training level name (`WildUntamed`, `Domesticated`, `Trained`, `WarTrained`...).
    pub training: String,
    /// [r8] Animals: pasture/pen civzone id, -1 if none.
    pub pasture: i32,
    /// [r8] Marked for slaughter (DF "Slaughter" animal flag).
    pub slaughter: bool,
    /// [r8] Pregnancy timer in ticks (0 = not pregnant), for animals and citizens.
    pub pregnancy: i32,
    /// [r8] Work detail names the unit is assigned to (v50).
    pub work_details: Vec<String>,
}

impl Unit {
    pub fn skill(&self, id: &str) -> i32 {
        self.skills.iter().find(|s| s.id == id).map(|s| s.rating).unwrap_or(0)
    }
    pub fn has_labor(&self, labor: &str) -> bool {
        self.labors.iter().any(|l| l == labor)
    }
    /// A dwarf-like member of the fort that takes jobs (citizen, alive, not an animal).
    pub fn is_worker(&self) -> bool {
        self.flags.citizen && !self.flags.dead && !self.flags.animal && !self.flags.inactive && !self.flags.caged
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemFlags {
    pub on_ground: bool,
    pub in_inventory: bool,
    pub in_building: bool,
    pub in_container: bool,
    pub in_job: bool,
    pub forbid: bool,
    pub dump: bool,
    pub owned: bool,
    pub rotten: bool,
    pub artifact: bool,
    pub construction: bool,
    pub hidden: bool,
    /// [construct] Designated for melting at a smelter (DF item flag `melt`).
    pub melt: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: i32,
    /// DF `item_type` name, e.g. `BOULDER`, `WOOD`, `DRINK`, `PLANT`, `MEAT`, `BAR`, `BED`.
    pub kind: String,
    /// Subtype token for typed items (e.g. `ITEM_WEAPON_PICK`), else empty.
    pub subtype: String,
    /// Material palette index.
    pub mat: u16,
    /// Position (for contained/carried items: the holder's position).
    pub pos: Pos,
    pub stack: i32,
    /// 0 ordinary .. 5 masterful, 6 artifact.
    pub quality: u8,
    pub wear: u8,
    pub flags: ItemFlags,
    /// Container item id or -1.
    pub container: i32,
    /// Unit holding it or -1.
    pub holder: i32,
    /// Building it belongs to (built into / stored in) or -1.
    pub building: i32,
    /// Artifact or custom name, else empty.
    pub name: String,
    /// Short display description like DF shows it, e.g. `granite boulder`.
    pub desc: String,
    /// Base value (DF `getValue` without trade adjustments).
    pub value: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StockpileSettings {
    /// Enabled top-level categories (DF names: `animals`, `food`, `furniture`, `corpses`,
    /// `refuse`, `stone`, `ammo`, `coins`, `bars_blocks`, `gems`, `finished_goods`,
    /// `leather`, `cloth`, `wood`, `weapons`, `armor`, `sheet`).
    pub categories: Vec<String>,
    pub max_barrels: i32,
    pub max_bins: i32,
    pub max_wheelbarrows: i32,
    /// [r8] Detailed DF filters, one string per enabled leaf: `category/group/token`, e.g.
    /// `food/meat/CREATURE:COW:MUSCLE`, `stone/economic/INORGANIC:HEMATITE`,
    /// `finished_goods/quality_min/ordinary`. Empty = everything in the categories.
    pub filters: Vec<String>,
    /// [r8] Pile takes items only from links (DF "take from links only").
    pub use_links_only: bool,
    /// [r8] Linked stockpiles/workshops this pile gives to.
    pub give_to: Vec<i32>,
    /// [r8] Linked stockpiles this pile takes from.
    pub take_from: Vec<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Building {
    pub id: i32,
    /// DF `building_type` name, e.g. `Workshop`, `Furnace`, `Bed`, `Door`, `Stockpile`, `Civzone`.
    pub kind: String,
    /// Workshop/furnace/civzone/trap subtype name, e.g. `Carpenters`, `Smelter`, `MeetingHall`.
    pub subtype: String,
    pub x1: i16,
    pub y1: i16,
    pub x2: i16,
    pub y2: i16,
    pub z: i16,
    pub center: Pos,
    pub mat: u16,
    /// Construction stage; `-1` or `max` = fully built.
    pub stage: i32,
    pub built: bool,
    /// Owner unit id (rooms/beds) or -1.
    pub owner: i32,
    pub name: String,
    /// Stockpile only.
    pub stockpile: Option<StockpileSettings>,
    /// Row-major mask over (x2-x1+1)*(y2-y1+1), 1 = tile belongs to the building.
    /// Empty = full rectangle.
    pub extents: Vec<u8>,
    /// Item ids contained/built in.
    pub items: Vec<i32>,
    /// Workshop/furnace job queue (DF `bld.jobs`, excluding the construction job).
    pub jobs: Vec<BuildingJob>,
    /// Farm plots: crop plant token per season (spring, summer, autumn, winter), "" = fallow.
    pub crops: Vec<String>,
    /// DF building state flags, e.g. `LOCKED`, `FORBIDDEN`, `PET_PASSABLE` (doors/hatches),
    /// `OPEN`/`CLOSED`, `RAISED` (bridges), `ACTIVE` (lever pulled / trap armed), `LOADED`.
    pub flags: Vec<String>,
    /// Mechanism links: for levers/pressure plates the building ids they trigger;
    /// for triggered buildings the triggering lever/plate ids.
    pub links: Vec<i32>,
    /// Bridge raise direction (`N`, `S`, `E`, `W`, `RETRACT`), trap subtype details, etc.
    pub direction: String,
}

/// One queued job in a workshop/furnace (DF `job` linked to the building).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildingJob {
    /// DF `job_type` name, e.g. `BrewDrink`, `PrepareMeal`, `ConstructBarrel`, `CustomReaction`.
    pub job: String,
    /// Reaction token for `CustomReaction` (e.g. `BREW_DRINK_FROM_PLANT`), else empty.
    pub reaction: String,
    /// Product item type/subtype for item jobs (DF names), else empty.
    pub item_type: String,
    pub item_subtype: String,
    /// Required material token (e.g. `INORGANIC:IRON`), or empty = any.
    pub mat: String,
    /// DF `job_material_category` flags that are set (e.g. `wood`, `stone`, `cloth`).
    pub mat_category: Vec<String>,
    pub repeat: bool,
    pub suspended: bool,
    /// Manager work order this job came from, or -1.
    pub order_id: i32,
}

/// A manager work order (DF `manager_order`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkOrder {
    pub id: i32,
    pub job: String,
    pub reaction: String,
    pub item_type: String,
    pub item_subtype: String,
    pub mat: String,
    pub mat_category: Vec<String>,
    pub amount_left: i32,
    pub amount_total: i32,
    /// `OneTime`, `Daily`, `Monthly`, `Seasonally`, `Yearly`.
    pub frequency: String,
    /// Workshop building id the order is restricted to, or -1.
    pub workshop_id: i32,
    pub active: bool,
    pub validated: bool,
    /// Item-count conditions ("only if fewer than N drinks"), all must hold.
    pub conditions: Vec<OrderCondition>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderCondition {
    /// `AtLeast`, `AtMost`, `GreaterThan`, `LessThan`, `Exactly`, `Not`.
    pub compare: String,
    pub value: i32,
    /// DF item type name the condition counts (e.g. `DRINK`), or empty = any.
    pub item_type: String,
    pub item_subtype: String,
    pub mat: String,
    /// Material flags / traits, e.g. `BREWABLE`, `EDIBLE_COOKED`, `UNROTTEN`.
    pub flags: Vec<String>,
}

/// Plant raw facts the simulation needs (from the save's raws, via the exporter).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlantRaw {
    /// Plant token, e.g. `MUSHROOM_HELMET_PLUMP`.
    pub token: String,
    pub name: String,
    pub name_plural: String,
    /// Ticks from planting to harvest (DF `GROWDUR` x 10 game-ticks... stored in ticks).
    pub growdur: i32,
    /// Seasons it can be planted in.
    pub spring: bool,
    pub summer: bool,
    pub autumn: bool,
    pub winter: bool,
    /// Grows underground (subterranean farm plots) / above ground.
    pub underground: bool,
    pub aboveground: bool,
    pub is_tree: bool,
    pub is_shrub: bool,
    /// Has a DRINK material (can be brewed).
    pub brewable: bool,
    pub edible_raw: bool,
    pub edible_cooked: bool,
    /// Can be milled / processed (thread, etc.).
    pub millable: bool,
    pub threshable: bool,
    /// Has SEED material.
    pub has_seed: bool,
    /// Palette indices of the plant's main materials, NO_MAT if not in the palette.
    pub mat_structural: u16,
    pub mat_drink: u16,
    pub mat_seed: u16,
}

impl Building {
    pub fn contains(&self, p: Pos) -> bool {
        if p.z != self.z || p.x < self.x1 || p.x > self.x2 || p.y < self.y1 || p.y > self.y2 {
            return false;
        }
        if self.extents.is_empty() {
            return true;
        }
        let w = (self.x2 - self.x1 + 1) as usize;
        let i = (p.y - self.y1) as usize * w + (p.x - self.x1) as usize;
        self.extents.get(i).copied().unwrap_or(0) != 0
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Plant {
    pub pos: Pos,
    /// Plant token, e.g. `OAK`, `TOWER_CAP`, `PLUMP_HELMET`.
    pub token: String,
    pub tree: bool,
    pub shrub: bool,
    pub sapling: bool,
    pub dead: bool,
    /// Growth/age in ticks (DF `grow_counter`).
    pub grow_counter: i32,
}

/// [construct] An engraving on a smoothed wall or floor (DF `world.engravings`). The tile keeps
/// its smooth tiletype; the engraving lies on top and adds to the room's value.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Engraving {
    pub pos: Pos,
    /// 0 ordinary .. 5 masterful (DF item quality levels).
    pub quality: u8,
    /// Engraved floor (true) or wall (false).
    pub floor: bool,
    /// Material palette index of the engraved stone.
    pub mat: u16,
    /// Engraver's unit id, -1 if unknown.
    pub engraver: i32,
    /// Engraving skill rating of the engraver when it was made.
    pub skill: i32,
}

/// [audit-eco] A pending carve-fortification or carve-track designation (DF keeps these in the
/// tile designation; saved so that they survive save/load like smooth/engrave designations).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CarveDesignation {
    pub pos: Pos,
    /// 0 = carve fortification, else carve track with these direction bits (N 1, S 2, W 4, E 8).
    pub track: u8,
}

/// [construct] A finished construction (DF `world.constructions`): what was there before and
/// which item it was built from (DF 53 keeps the item in the world with `flags.construction`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Construction {
    pub pos: Pos,
    /// Building material item id, -1 if unknown.
    pub item: i32,
    /// DF item type of the material, e.g. `BLOCKS`, `BOULDER`, `WOOD`, `BAR`.
    pub item_kind: String,
    pub mat: u16,
    /// Tiletype id and material of the tile before the construction was built.
    pub orig_tt: u16,
    pub orig_mat: u16,
}
