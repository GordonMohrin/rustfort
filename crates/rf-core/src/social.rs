//! Personality, needs, emotions/thoughts (DF v50 psychology), as exported from DF.
//!
//! Owner: the social simulation (rf-sim `social` module). Additive changes only.

use serde::{Deserialize, Serialize};

/// One personality need (DF `unit_personality.needs`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UnitNeed {
    /// DF `need_type` name, e.g. `DrinkAlcohol`, `Socialize`, `PrayOrMeditate`, `Family`.
    pub id: String,
    /// Deity histfig for prayer needs, else -1.
    pub deity_id: i32,
    /// DF focus level: positive = satisfied, negative = distracted (DF thresholds per wiki).
    pub focus_level: i32,
    /// Importance 1..=10 (DF `need_level`).
    pub need_level: i32,
}

/// One remembered emotion/thought (DF `unit_personality.emotions`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Emotion {
    /// DF `emotion_type` name, e.g. `SATISFACTION`, `ANNOYANCE`, `GRIEF`.
    pub emotion: String,
    /// DF `unit_thought_type` name, e.g. `AteFineMeal`, `SleptOnGround`, `SawDeadBody`.
    pub thought: String,
    pub subthought: i32,
    pub strength: i32,
    pub severity: i32,
    /// When it happened.
    pub year: i32,
    pub year_tick: i32,
    pub flags: Vec<String>,
}

/// A personality value / belief (DF `value_type` name -> strength -50..=50).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ValueStrength {
    pub id: String,
    pub strength: i32,
}

/// A relationship to another historical figure (spouse, friend, ...).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Relationship {
    /// e.g. `Spouse`, `Mother`, `Father`, `Child`, `Friend`, `Grudge`, `Lover`.
    pub kind: String,
    pub hist_fig_id: i32,
    /// Unit id of that figure if it is on the map, else -1.
    pub unit_id: i32,
}
