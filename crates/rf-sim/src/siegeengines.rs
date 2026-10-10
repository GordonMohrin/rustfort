//! [siege12] Siege engines (placeholder, filled below).

/// Counters for reports and tests.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SiegeStats {
    pub shots: u64,
}

/// One siege engine for the UI.
#[derive(Clone, Debug, Default)]
pub struct EngineInfo {
    pub building: i32,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub(crate) struct Engines {
    pub list: Vec<u32>,
    pub stats: SiegeStats,
}

impl crate::Sim {
    pub(crate) fn engines_tick(&mut self, _now: u64) {}
}
