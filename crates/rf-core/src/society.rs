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
}

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
