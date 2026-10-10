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
    /// [app8] Names of the criminal and the victim as exported (the units may be dead or gone).
    pub criminal_name: String,
    pub victim_name: String,
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

/// [r8-import] Semantics of [`crate::StockpileSettings::filters`] (docs/EXPORT_FORMAT.md, "Stockpile
/// filters"). Each string is `category/group/token`:
/// - `category/group/TOKEN` enables one leaf, `category/group/*` the whole group, and
///   `category/group/!TOKEN` excludes one leaf of a `*` group;
/// - `category/options/NAME` is a switch such as `food/options/prepared_meals`;
/// - `category/none/-` marks an enabled category with nothing enabled in it;
/// - `misc/options/allow_organic` / `allow_inorganic` are pile-wide.
///
/// A category without any filter string accepts everything in it (exports before round 8 and
/// piles made without detailed settings).
impl crate::StockpileSettings {
    fn has_category_filters(&self, category: &str) -> bool {
        self.filters.iter().any(|f| f.len() > category.len() && f.starts_with(category) && f.as_bytes()[category.len()] == b'/')
    }

    /// Does this pile accept the leaf `token` of `category/group`? False if the category is off.
    pub fn filter_allows(&self, category: &str, group: &str, token: &str) -> bool {
        if !self.categories.iter().any(|c| c == category) {
            return false;
        }
        if !self.has_category_filters(category) {
            return true;
        }
        let (mut all, mut one, mut excluded) = (false, false, false);
        for f in &self.filters {
            let mut parts = f.splitn(3, '/');
            let (Some(c), Some(g), Some(t)) = (parts.next(), parts.next(), parts.next()) else { continue };
            if c != category || g != group {
                continue;
            }
            if t == "*" {
                all = true;
            } else if let Some(x) = t.strip_prefix('!') {
                excluded |= x == token;
            } else {
                one |= t == token;
            }
        }
        one || (all && !excluded)
    }

    /// Is the switch `category/options/option` on? A category without filter strings counts as
    /// fully enabled (true); so do the pile-wide `misc` options (`allow_organic`,
    /// `allow_inorganic`) when there is no `misc/` string.
    pub fn filter_option(&self, category: &str, option: &str) -> bool {
        if category != "misc" && !self.categories.iter().any(|c| c == category) {
            return false;
        }
        if !self.has_category_filters(category) {
            return true;
        }
        let want = format!("{category}/options/{option}");
        self.filters.iter().any(|f| *f == want)
    }
}
