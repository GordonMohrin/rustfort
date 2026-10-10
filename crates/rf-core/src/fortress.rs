//! Fortress management data (DF v50): work details, kitchen settings, locations,
//! justice, hauling routes. Exported from DF; owned by the round-8 simulation modules.
//! Additive changes only.

use crate::tiles::Pos;
use serde::{Deserialize, Serialize};

/// A v50 work detail (DF `work_detail`): a named group of labors with assigned units.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkDetail {
    pub name: String,
    /// DF `unit_labor` names enabled by this detail.
    pub labors: Vec<String>,
    /// Unit ids assigned.
    pub assigned: Vec<i32>,
    /// `EverybodyDoesThis`, `OnlySelectedDoesThis`, `NobodyDoesThis` (DF work_detail_mode).
    pub mode: String,
    /// Icon index / name as DF stores it (for the UI).
    pub icon: String,
    /// Assigned units do no other work (DF "no other work" flag).
    pub no_other_work: bool,
}

/// Kitchen permission for an item type/material (DF `kitchen` exclusions).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KitchenSetting {
    /// DF item type name, e.g. `PLANT`, `DRINK`, `SEEDS`, `MEAT`.
    pub item_type: String,
    pub item_subtype: String,
    /// Material token, e.g. `PLANT:MUSHROOM_HELMET_PLUMP:STRUCTURAL`.
    pub mat: String,
    /// Excluded from cooking / brewing.
    pub no_cook: bool,
    pub no_brew: bool,
}

/// An occupation inside a location (e.g. Tavern keeper, Performer, Priest, Scholar).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Occupation {
    /// DF occupation type name, e.g. `TAVERN_KEEPER`, `PERFORMER`, `PRIEST`, `SCHOLAR`.
    pub kind: String,
    /// Unit holding it, -1 = vacant.
    pub unit: i32,
}

/// A v50 location (temple, tavern, library, guildhall, hospital) built on civzones.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Location {
    pub id: i32,
    /// `Temple`, `Tavern`, `Library`, `Guildhall`, `Hospital`.
    pub kind: String,
    pub name: String,
    /// Temple: deity histfig id or -1 (no specific deity).
    pub deity: i32,
    /// Guildhall: profession name.
    pub profession: String,
    /// Civzone building ids belonging to the location.
    pub zones: Vec<i32>,
    pub occupations: Vec<Occupation>,
    /// Visitors allowed (DF: residents / citizens only / visitors).
    pub allow_visitors: bool,
}

/// A crime / justice case (DF `crime`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Crime {
    pub id: i32,
    /// e.g. `Theft`, `Vandalism`, `Murder`, `DisorderlyBehavior`, `ViolatingProductionOrder`.
    pub kind: String,
    pub criminal: i32,
    pub victim: i32,
    pub site_tick: i32,
    pub year: i32,
    pub convicted: i32,
    /// Punishment given, e.g. `Jail 3 months`, `Beating`, `Hammerstrikes 20`.
    pub punishment: String,
    pub witnesses: Vec<i32>,
    /// [society2] Case state: `Unsolved`, `Accused` (culprit identified), `Convicted`,
    /// `Serving` (jail time), `Punished`, `Closed` (culprit gone or dead). Empty = `Unsolved`.
    pub status: String,
    /// [society2] Remaining jail days (or hammerstrikes / beatings still owed) of the sentence.
    pub sentence_days: i32,
}

/// A hauling route (minecarts / wheelbarrows) with stops.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HaulRoute {
    pub id: i32,
    pub name: String,
    pub stops: Vec<HaulStop>,
    /// Vehicle item ids (minecarts) assigned.
    pub vehicles: Vec<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HaulStop {
    pub name: String,
    pub pos: Pos,
    /// Linked stockpile building ids (take from / give to).
    pub take_from: Vec<i32>,
    pub give_to: Vec<i32>,
    /// Departure conditions as DF-like text, e.g. `Leave when full`, `Wait 1 day`.
    pub conditions: Vec<String>,
}
