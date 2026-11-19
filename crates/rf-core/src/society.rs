//! Civilizations, nobles, game settings and historical figure names (exported from DF).
//!
//! Owner: the population/trade simulation (rf-sim `population` and `trade` modules).
//! Additive changes only.

use serde::{Deserialize, Serialize};

/// A civilization or other entity relevant to the fort (DF `historical_entity`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EntityInfo {
    pub id: i32,
    pub name: String,
    /// Creature token of the main race, e.g. `DWARF`, `GOBLIN`, `ELF`, `HUMAN`.
    pub race: String,
    /// DF entity type, e.g. `Civilization`, `SiteGovernment`, `Religion`.
    pub kind: String,
    /// Relation to the fort's civilization: `Own`, `Peace`, `War`, `Unknown`.
    pub relation: String,
    /// Sends caravans (trade partner).
    pub trades: bool,
    /// [contract13] Attitude towards the fort, -100 (hostile) ..= 100 (friendly), 0 = neutral or
    /// unknown. Shifted by trade value, killings and refused demands; `relation` flips to `War`
    /// / `Peace` on declarations. Owner: rf-sim `diplomacy` [diplo13].
    pub attitude: i32, // [contract13]
}

/// A noble/administrator position of the fort or its civilization.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NoblePosition {
    /// DF position token, e.g. `EXPEDITION_LEADER`, `MAYOR`, `MANAGER`, `BROKER`, `BARON`.
    pub token: String,
    pub name: String,
    /// Unit holding it, -1 = vacant.
    pub holder: i32,
    pub entity_id: i32,
    /// Active mandates as text, e.g. `Make 3 goblets (iron)`, `Ban export of steel`.
    pub mandates: Vec<String>,
    /// [nobles15] Position numbers as exported (`nobles.json`, raws `MANDATE_MAX` etc.); 0 =
    /// not exported (the sim falls back to the vanilla raws table).
    pub mandate_max: i32, // [nobles15]
    pub demand_max: i32, // [nobles15]
    pub required_office: i32, // [nobles15]
    pub required_bedroom: i32, // [nobles15]
    pub required_dining: i32, // [nobles15]
    pub required_tomb: i32, // [nobles15]
    pub required_boxes: i32, // [nobles15]
    pub required_cabinets: i32, // [nobles15]
    pub required_racks: i32, // [nobles15]
    pub required_stands: i32, // [nobles15]
    /// [nobles15] Assignment/position flags (`HAS_BEEN_REPLACED`, ...).
    pub flags: Vec<String>, // [nobles15]
    /// [nobles15] Responsibilities (`MEET_WORKERS`, `TRADE`, ...).
    pub responsibilities: Vec<String>, // [nobles15]
    /// [nobles15] Raws PRECEDENCE (lower = higher rank), 0 = unknown.
    pub precedence: i32, // [nobles15]
    /// [nobles15] Mandates with their numbers (`mandates_detail` of `nobles.json`).
    pub mandates_detail: Vec<MandateDetail>, // [nobles15]
}

/// [nobles15] A mandate with DF's counters (export `nobles.json` `mandates_detail`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)] // [nobles15]
#[serde(default)] // [nobles15]
pub struct MandateDetail { // [nobles15]
    /// e.g. `Make 3 spears`, `Ban export of spears`.
    pub text: String, // [nobles15]
    /// `Make`, `Export`, `Guild`.
    pub mode: String, // [nobles15]
    pub item_type: String, // [nobles15]
    pub item_subtype: String, // [nobles15]
    pub mat: String, // [nobles15]
    pub amount_total: i32, // [nobles15]
    pub amount_remaining: i32, // [nobles15]
    /// DF counter in units of 10 ticks; at `timeout_limit` the mandate is due.
    pub timeout_counter: i32, // [nobles15]
    pub timeout_limit: i32, // [nobles15]
    pub punishment_hammerstrikes: i32, // [nobles15]
    /// Months of jail.
    pub punishment_prison_time: i32, // [nobles15]
} // [nobles15]

/// Relevant DF game settings (d_init / world settings).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    pub population_cap: i32,
    pub strict_population_cap: i32,
    pub baby_cap: i32,
    pub invasions: bool,
    pub temperature: bool,
    pub weather: bool,
    pub cave_ins: bool,
    /// Other raw settings worth keeping, `KEY=value`.
    pub other: Vec<String>,
}

/// Name lookup for historical figures referenced by relationships, nobles, etc.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HistFigName {
    pub id: i32,
    pub name: String,
    pub race: String,
    pub alive: bool,
    /// Unit id on the map, -1 if not here.
    pub unit_id: i32,
}

/// A merchant caravan of the fort (DF `plotinfo.caravans`, exported to `caravans.json`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Caravan {
    /// Merchant entity (civilization) id, see `World::entities`.
    pub entity_id: i32,
    /// Civilization name as DF shows it, e.g. `The Channels of Direction`.
    pub civ: String,
    /// Creature token of the merchants' race, e.g. `DWARF`, `HUMAN`, `ELF`.
    pub race: String,
    /// DF trade state: `None`, `Approaching`, `AtDepot`, `Leaving`, `Stuck`.
    pub state: String,
    /// Ticks until the merchants leave (DF `time_remaining` x 10; 0 = leaving now).
    pub leaves_in: i64,
    /// Trade depot building id, -1 = none.
    pub depot: i32,
    /// Item ids of the caravan's goods.
    pub goods: Vec<i32>,
    /// Unit ids of the pack animals / wagons.
    pub animals: Vec<i32>,
    /// Unit ids of the merchants on the map.
    pub merchants: Vec<i32>,
    /// DF bookkeeping: value of the goods brought / bought from the fort so far.
    pub import_value: i32,
    pub export_value: i32,
    /// Merchant mood (DF starts at 100; failed haggling lowers it).
    pub mood: i32,
    /// Set `plot_merchant_flag` bits uppercased.
    pub flags: Vec<String>,
}
