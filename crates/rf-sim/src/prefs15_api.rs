//! [prefs15] Public API for preferences, item value and the preference report (UI and later
//! owners such as noble demands). See `prefs` and docs/mechanics/prefs15.md.

use crate::Sim;
use serde::{Deserialize, Serialize};

/// Counters of preference effects since the start (part of the exact snapshot).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PrefsReport {
    /// Products whose quality roll got at least one +10 preference bonus.
    pub quality_bonus: u64,
    pub work_satisfied: u64,
    pub admired: u64,
    pub good_meals: u64,
    pub good_drinks: u64,
    pub acquired: u64,
    pub extravagant: u64,
    /// Strange moods whose artifact type a preference decided.
    pub mood_forced: u64,
    /// Units that got derived (estimated) preferences / units with exported ones.
    pub derived_units: u64,
    pub exported_units: u64,
}

/// One preference line for the unit sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefLine {
    /// Group: "material", "item", "creature", "plant", "food", "other", "dislike".
    pub group: &'static str,
    pub text: String,
    /// Derived by Rustfort (the export had none): shown as an estimate.
    pub derived: bool,
}

fn noun(kind: &str, sub: &str) -> String {
    let t = if sub.is_empty() { kind } else { sub.trim_start_matches("ITEM_").splitn(2, '_').nth(1).unwrap_or(sub) };
    let mut s = t.to_ascii_lowercase().replace('_', " ");
    match s.as_str() {
        "box" => s = "chests".into(),
        "trapparts" => s = "mechanisms".into(),
        "armorstand" => s = "armor stands".into(),
        "weaponrack" => s = "weapon racks".into(),
        "animaltrap" => s = "animal traps".into(),
        "trapcomp" => s = "trap components".into(),
        "gem" => s = "large gems".into(),
        _ => {
            if !s.ends_with('s') {
                s.push('s');
            }
        }
    }
    s
}

impl Sim {
    /// The preferences of a unit in display form (own wording, grouped like DF's sheet).
    pub fn unit_preferences(&self, unit_id: i32) -> Vec<PrefLine> {
        let Some(u) = self.world.unit(unit_id) else { return Vec::new() };
        let mut out = Vec::new();
        for p in &u.preferences {
            let mat = || {
                if p.mat == rf_core::NO_MAT {
                    "something not found here".to_string()
                } else {
                    self.world.material(p.mat).map(|m| m.name.clone()).unwrap_or_else(|| p.mat_token.to_ascii_lowercase())
                }
            };
            let (group, text) = match p.kind.as_str() {
                "LikeMaterial" => ("material", format!("likes {}", mat())),
                "LikeItem" => ("item", format!("likes {}", noun(&p.item_type, &p.item_subtype))),
                "LikeCreature" => ("creature", format!("likes {}", self.creature_name(&p.token))),
                "HateCreature" => ("dislike", format!("hates {}", self.creature_name(&p.token))),
                "LikePlant" | "LikeTree" => ("plant", format!("likes {}", p.token.to_ascii_lowercase().replace('_', " "))),
                "LikeFood" => ("food", if p.item_type == "DRINK" { format!("likes to drink {}", mat()) } else { format!("likes to eat {}", mat()) }),
                "LikeColor" => ("other", format!("likes the color {}", p.descriptor.to_ascii_lowercase().replace('_', " "))),
                "LikeShape" => ("other", format!("likes the shape {}", p.descriptor.to_ascii_lowercase().replace('_', " "))),
                "LikePoeticForm" => ("other", format!("likes a poetic form ({})", p.descriptor)),
                "LikeMusicalForm" => ("other", format!("likes a musical form ({})", p.descriptor)),
                "LikeDanceForm" => ("other", format!("likes a dance form ({})", p.descriptor)),
                k => ("other", k.to_string()),
            };
            out.push(PrefLine { group, text: if p.active { text } else { format!("{text} (inactive)") }, derived: p.derived });
        }
        out
    }

    fn creature_name(&self, token: &str) -> String {
        self.world
            .creature_raws
            .iter()
            .find(|c| c.token == token)
            .map(|c| if c.name_plural.is_empty() { c.name.clone() } else { c.name_plural.clone() })
            .unwrap_or_else(|| token.to_ascii_lowercase().replace('_', " "))
    }

    /// How many preferences of the unit an item matches (kind/subtype, material).
    pub fn unit_likes_item(&self, unit_id: i32, item_id: i32) -> u8 {
        let (Some(&s), Some(&i)) = (self.unit_slot.get(&unit_id), self.item_idx.get(&item_id)) else { return 0 };
        let it = &self.world.items[i as usize];
        self.prefs_matches(s, &it.kind, &it.subtype, it.mat).max(0) as u8
    }

    /// DF v50 value of an item computed from kind, material and quality (wiki Item value).
    pub fn item_value_v50(&self, item_id: i32) -> i64 {
        let Some(&i) = self.item_idx.get(&item_id) else { return 0 };
        let it = &self.world.items[i as usize];
        let mv = self.world.material(it.mat).map(|m| m.value).filter(|&v| v > 0).unwrap_or(1) as i64;
        crate::item_value::item_value_v50(&it.kind, &it.subtype, mv, it.quality, it.stack)
    }

    pub fn prefs_report(&self) -> PrefsReport {
        self.prefs.rep
    }

    pub fn prefs_enabled(&self) -> bool {
        !self.prefs.off
    }

    /// A/B switch: off = the pre-v0.15 rules (quality without preferences, random work
    /// satisfaction, admire by quality, every meal a good meal).
    pub fn set_prefs_enabled(&mut self, on: bool) {
        self.prefs.off = !on;
    }
}
