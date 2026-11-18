//! [weather13] Weather and climate (DF v50; wiki Weather, Temperature, Ice, Snow, Evil weather).
//!
//! SKELETON from the round-13 contract: the state is persisted (part of the exact snapshot via
//! `Sim::weather`), the hook runs, nothing happens yet. Owner [weather13] fills it in:
//! per-biome rain/snow/fog/clouds by season and `World::climate`, surface water freezing and
//! thawing, snow cover, rain refilling murky pools, evil rain/clouds (exposed through
//! `round13_api::EvilWeather` for [undead13]). Rain decisions now live in `lifestyle`
//! (`weather_day`, `Life::rain_start/rain_until`); move them here behind the API but keep the
//! lifestyle "Rain" thoughts and the default-settings bench hash.

use crate::round13_api::{EvilWeather, WeatherInfo, WeatherKind, WeatherReport};
use crate::Sim;
use rf_core::Pos;
use serde::{Deserialize, Serialize};

/// `weather_pass` runs when `now % WEATHER_PERIOD == WEATHER_PHASE` (owner may change both).
pub(crate) const WEATHER_PERIOD: u64 = 100;
pub(crate) const WEATHER_PHASE: u64 = 41;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Weather {
    /// Player toggle; None = follow `World::settings.weather`.
    pub enabled: Option<bool>,
    /// Current weather over the surface and until which tick it lasts.
    pub kind: WeatherKind,
    pub until: u64,
    /// Forced by `force_weather` (debug/tests) until this tick.
    pub forced_until: u64,
    pub evil: Option<EvilWeather>,
    pub rep: WeatherReport,
}

impl Sim {
    /// Hook from `tick_inner` (every `WEATHER_PERIOD` ticks, before the society2 pass).
    pub(crate) fn weather_pass(&mut self, _now: u64) {}

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn weather_hash(&self, _mix: &mut dyn FnMut(u64)) {}

    pub(crate) fn weather_enabled(&self) -> bool {
        self.weather.enabled.unwrap_or(self.world.settings.weather)
    }

    pub(crate) fn weather_summary_impl(&self) -> WeatherInfo {
        WeatherInfo {
            kind: self.weather.kind,
            temperature: self.temperature_at_impl(Pos::new(0, 0, 0)),
            season: self.season_label(),
            frozen: false,
            snow_cover: false,
            enabled: self.weather_enabled(),
        }
    }

    pub(crate) fn weather_at_impl(&self, _p: Pos) -> WeatherKind {
        WeatherKind::Clear
    }

    pub(crate) fn temperature_at_impl(&self, _p: Pos) -> i32 {
        if self.world.climate.temperature != 0 {
            self.world.climate.temperature
        } else {
            10_050 // ESTIMATE: temperate default (18 F above freezing)
        }
    }

    pub(crate) fn snow_at_impl(&self, _p: Pos) -> bool {
        false
    }

    pub(crate) fn evil_weather_impl(&self) -> Option<EvilWeather> {
        self.weather.evil.clone()
    }

    pub(crate) fn set_weather_enabled_impl(&mut self, on: bool) {
        self.weather.enabled = Some(on);
    }

    pub(crate) fn force_weather_impl(&mut self, kind: WeatherKind, ticks: u64) {
        self.weather.kind = kind;
        self.weather.forced_until = self.ticks + ticks;
        self.weather.until = self.weather.forced_until;
    }

    pub(crate) fn weather_report_impl(&self) -> WeatherReport {
        self.weather.rep
    }
}
