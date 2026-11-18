//! [weather13] Weather and climate (DF v50; wiki Weather, Temperature, Ice, Snow, Murky pool,
//! Evil weather).
//!
//! # Rules
//! - **Temperature** (wiki Temperature): degrees Urist, 10 000 U = water freezes. The surface
//!   temperature is the region's mean (`World::climate.temperature`) plus a seasonal swing
//!   (`climate.temperature_swing`): coldest in mid winter, warmest in mid summer. Tiles that are
//!   not under the sky keep the mean (no seasonal swing); magma is 12 000 U.
//! - **Precipitation** (wiki Weather): the daily roll that used to live in `lifestyle` (same RNG
//!   stream, so default runs keep their rain): outside winter it rains on 1 day in 5 for 2-8
//!   hours. If the surface is below freezing, the precipitation falls as **snow** instead.
//!   In winter (formerly always dry) a second roll on its own stream makes snow when it is
//!   freezing. Known rainfall (`climate.rainfall` 1..=100) scales the chance (rainfall / 250).
//!   Rain (not snow) feeds the lifestyle "Rain" thoughts.
//! - **Clouds and fog**: overcast before precipitation and on some dry days; fog on some
//!   mornings, more often in wet regions.
//! - **Snow cover**: snow that has fallen lies on the surface until the temperature rises
//!   above freezing (`Sim::snow_at`).
//! - **Ice** (wiki Ice): when the surface temperature drops below 10 000 U, the topmost water
//!   tile of every outdoor water column freezes; deep water (4/7 and more) becomes an ice wall,
//!   shallow water an ice floor. Creatures swimming in water that freezes are encased in the
//!   ice and die. When it rises to 10 000 U again (spring), the ice thaws back into the water it
//!   was (same depth, salt/stagnant flags). Ice that was removed in between does not come back.
//! - **Murky pools** (wiki Murky pool): rain refills outdoor pool tiles.
//! - **Evil weather** (wiki Evil weather): regions with evil weather (`climate.evil_weather`)
//!   get evil rain (blood rain, syndrome rain) over the whole surface or an evil cloud that
//!   drifts with the wind. Effects are left to [undead13] through `Sim::evil_weather`.
//!
//! # Estimates (not on the wiki)
//! - Unknown climate: mean 10 020 U (52 F), seasonal swing 30 U: the surface freezes in early
//!   winter and thaws in early spring. Unknown swing with a known mean: 30 U.
//! - Shallow water (1-3/7) freezes into floor, deeper water into a wall; only the topmost water
//!   tile of a column freezes; river sources do not freeze; water that appears during a frost
//!   stays liquid until the next frost.
//! - Snow on 1 day in 5 in a freezing winter; clouds on 1 day in 4 and 3 hours before
//!   precipitation; fog on 1 morning in 12 (1 in 4 in wet regions) for 2-6 hours.
//! - Rain adds 1/7 to an outdoor murky pool tile with chance 1/4 every 100 ticks.
//! - Evil weather on 1 day in 8 in evil regions, 2-6 hours; clouds have radius 4-8 and drift
//!   one tile per 100 ticks.

use crate::rng::Rng;
use crate::round13_api::{EvilWeather, WeatherInfo, WeatherKind, WeatherReport};
use crate::{Kind, Sim};
use rf_core::{liquid, tile_flags, Pos, Shape, TtMaterial, TtSpecial, TICKS_PER_DAY};
use serde::{Deserialize, Serialize};

/// `weather_pass` runs when `now % WEATHER_PERIOD == WEATHER_PHASE` (owner may change both).
pub(crate) const WEATHER_PERIOD: u64 = 100;
pub(crate) const WEATHER_PHASE: u64 = 41;

const DAY: u64 = TICKS_PER_DAY as u64;
const HOUR: u64 = DAY / 24;
/// Water freezes below this (degrees Urist).
pub const FREEZING: i32 = 10_000;
/// Default climate when the export has none (estimates, see module docs).
pub const DEFAULT_MEAN: i32 = 10_020;
pub const DEFAULT_SWING: i32 = 30;
/// Magma temperature (wiki Magma: 12 000 U).
pub const MAGMA_TEMP: i32 = 12_000;
/// Seasonal offset per month in per-mille of the swing (month 0 = early spring).
const SEASON_OFF: [i32; 12] = [-500, 0, 500, 850, 1000, 850, 500, 0, -500, -850, -1000, -850];

/// One frozen water tile and what it was before.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Frozen {
    pub i: u32,
    pub depth: u8,
    pub tt: u16,
    pub mat: u16,
    /// SALT / STAGNANT flags of the water.
    pub flags: u16,
    /// Ice tiletype it became (thaw only if it is still that).
    pub ice_tt: u16,
}

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
    /// Non-evil weather under an evil cloud.
    pub base: WeatherKind,
    /// Last day (tick / DAY + 1) whose daily rolls were made.
    pub day: u64,
    /// Precipitation window; `snow` = it falls as snow.
    pub precip_start: u64,
    pub precip_until: u64,
    pub precip_snow: bool,
    pub cloudy_until: u64,
    pub fog_start: u64,
    pub fog_until: u64,
    pub evil_until: u64,
    /// Evil cloud drift per pass.
    pub wind: (i8, i8),
    /// Surface water is frozen; the frozen tiles.
    pub frozen: bool,
    pub ice: Vec<Frozen>,
    pub snow_cover: bool,
    /// Outdoor murky pool tiles (rebuilt monthly) and the month key they were built for.
    pub pools: Vec<u32>,
    pub pools_key: i64,
}

impl Sim {
    /// Hook from `tick_inner` (every `WEATHER_PERIOD` ticks, before the society2 pass).
    pub(crate) fn weather_pass(&mut self, now: u64) {
        if !self.weather_enabled() {
            if self.weather.frozen {
                self.weather_thaw();
            }
            if now >= self.weather.forced_until {
                self.weather.kind = WeatherKind::Clear;
                self.weather.base = WeatherKind::Clear;
            }
            self.weather.snow_cover = false;
            self.weather.evil = None;
            return;
        }
        let day = now / DAY + 1;
        if day != self.weather.day {
            self.weather.day = day;
            self.weather_new_day(now);
        }
        let t = self.surface_temperature();
        if !self.weather.frozen && t < FREEZING {
            self.weather_freeze(now);
        } else if self.weather.frozen && t >= FREEZING {
            self.weather_thaw();
        }
        // Evil weather ends; clouds drift.
        if self.weather.evil.is_some() && now >= self.weather.evil_until && now >= self.weather.forced_until {
            self.weather.evil = None;
        }
        if let Some(e) = self.weather.evil.as_mut() {
            if let Some(c) = e.center.as_mut() {
                let (wx, wy) = self.weather.wind;
                c.x = (c.x + wx as i16).clamp(0, self.world.dims.x as i16 - 1);
                c.y = (c.y + wy as i16).clamp(0, self.world.dims.y as i16 - 1);
            }
        }
        // Current weather.
        let w = &self.weather;
        let base = if now < w.precip_until && now >= w.precip_start {
            if w.precip_snow {
                WeatherKind::Snow
            } else {
                WeatherKind::Rain
            }
        } else if now >= w.fog_start && now < w.fog_until {
            WeatherKind::Fog
        } else if now < w.cloudy_until || (w.precip_start > now && w.precip_start - now <= 3 * HOUR) {
            WeatherKind::Cloudy
        } else {
            WeatherKind::Clear
        };
        let forced_until = w.forced_until;
        let forced = now < forced_until;
        let kind = if forced {
            w.kind
        } else if let Some(e) = &w.evil {
            e.kind
        } else {
            base
        };
        self.weather.base = if forced && !matches!(kind, WeatherKind::EvilRain | WeatherKind::EvilCloud) { kind } else { base };
        self.weather.kind = kind;
        self.weather.until = if forced { forced_until } else { now + WEATHER_PERIOD };
        let rep = &mut self.weather.rep;
        match kind {
            WeatherKind::Rain => rep.rain_ticks += WEATHER_PERIOD,
            WeatherKind::Snow => rep.snow_ticks += WEATHER_PERIOD,
            WeatherKind::Fog => rep.fog_ticks += WEATHER_PERIOD,
            WeatherKind::EvilRain | WeatherKind::EvilCloud => rep.evil_ticks += WEATHER_PERIOD,
            _ => {}
        }
        let surf = self.weather.base;
        if surf == WeatherKind::Snow {
            self.weather.snow_cover = true;
        } else if self.weather.snow_cover && t >= FREEZING {
            self.weather.snow_cover = false; // melts
        }
        if surf == WeatherKind::Rain {
            self.weather_refill_pools(now);
        }
    }

    /// Daily rolls of the weather pass: clouds, fog, evil weather, the pool list.
    fn weather_new_day(&mut self, now: u64) {
        let key = self.world.calendar.year as i64 * 12 + self.world.calendar.month() as i64 + 1;
        if key != self.weather.pools_key {
            self.weather.pools_key = key;
            self.weather_scan_pools();
        }
        let mut r = Rng::for_unit(self.seed, 1, now, 0x4A3);
        if r.chance(1, 4) {
            self.weather.cloudy_until = now + (6 + r.below(13) as u64) * HOUR;
        }
        let c = &self.world.climate;
        let b = c.biome.to_ascii_uppercase();
        let wet = c.rainfall >= 66 || ["SWAMP", "MARSH", "LAKE", "OCEAN", "RIVER"].iter().any(|k| b.contains(k));
        if r.chance(1, if wet { 4 } else { 12 }) {
            // Morning fog: starts within the first 3 hours of the day.
            self.weather.fog_start = now + r.below(3 * HOUR as u32) as u64;
            self.weather.fog_until = self.weather.fog_start + (2 + r.below(5) as u64) * HOUR;
        }
        if c.evil && !c.evil_weather.is_empty() && self.weather.evil.is_none() {
            let mut e = Rng::for_unit(self.seed, 1, now, 0x4A5);
            if e.chance(1, 8) {
                let name = c.evil_weather[e.below(c.evil_weather.len() as u32) as usize].clone();
                let hours = 2 + e.below(5) as u64;
                let ev = self.weather_make_evil(&name, &mut e);
                self.weather.evil = Some(ev);
                self.weather.evil_until = now + hours * HOUR;
            }
        }
    }

    /// An evil rain or cloud from a region weather name.
    fn weather_make_evil(&mut self, name: &str, r: &mut Rng) -> EvilWeather {
        let low = name.to_ascii_lowercase();
        let effect = if low.contains("blood") {
            "BLOOD"
        } else if low.contains("husk") {
            "HUSK"
        } else if low.contains("thrall") {
            "THRALL"
        } else {
            "SYNDROME"
        };
        let rain = low.contains("rain") || effect == "BLOOD";
        let center = if rain {
            None
        } else {
            let x = r.below(self.world.dims.x.max(1) as u32) as i16;
            let y = r.below(self.world.dims.y.max(1) as u32) as i16;
            Some(Pos::new(x, y, self.weather_surface_z(x, y)))
        };
        self.weather.wind = (r.below(3) as i8 - 1, r.below(3) as i8 - 1);
        EvilWeather {
            kind: if rain { WeatherKind::EvilRain } else { WeatherKind::EvilCloud },
            name: name.to_string(),
            effect: effect.into(),
            syndrome: if effect == "SYNDROME" { name.to_string() } else { String::new() },
            center,
            radius: if rain { 0 } else { 4 + r.below(5) as i32 },
        }
    }

    /// Highest outdoor tile with a floor in a column (map top if none).
    fn weather_surface_z(&self, x: i16, y: i16) -> i16 {
        for z in (0..self.world.dims.z as i16).rev() {
            let p = Pos::new(x, y, z);
            if let Some(t) = self.world.tile(p) {
                if t.flags & tile_flags::OUTSIDE != 0 && self.world.shape(p).has_floor() {
                    return z;
                }
                if t.flags & tile_flags::OUTSIDE == 0 {
                    return z;
                }
            }
        }
        self.world.dims.z as i16 - 1
    }

    /// [hook] From lifestyle's daily weather roll (society2 pass): schedule the day's
    /// precipitation. Returns the rain window for the lifestyle "Rain" thoughts (None if it
    /// does not rain, or if it snows).
    pub(crate) fn weather_precip_day(&mut self, now: u64) -> Option<(u64, u64)> {
        if !self.weather_enabled() {
            return None;
        }
        let rainfall = self.world.climate.rainfall;
        if self.world.calendar.month() / 3 == 3 {
            // Winter: dry unless it freezes, then it snows (own stream).
            if self.surface_temperature() >= FREEZING {
                return None;
            }
            let mut r = Rng::for_unit(self.seed, 1, now, 0x5A0);
            let hit = if rainfall > 0 { r.chance(rainfall.min(100) as u32, 250) } else { r.chance(1, 5) };
            if hit {
                let start = now + r.below(DAY as u32) as u64;
                self.weather_set_precip(start, start + (2 + r.below(7) as u64) * HOUR, true);
            }
            return None;
        }
        // Same stream and draws as the round-8 lifestyle roll.
        let mut r = Rng::for_unit(self.seed, 1, now, 0x4A1);
        let hit = if rainfall > 0 { r.chance(rainfall.min(100) as u32, 250) } else { r.chance(1, 5) };
        if !hit {
            return None;
        }
        let start = now + r.below(DAY as u32) as u64;
        let until = start + (2 + r.below(7) as u64) * DAY / 24;
        let snow = self.surface_temperature() < FREEZING;
        self.weather_set_precip(start, until, snow);
        if snow {
            None
        } else {
            Some((start, until))
        }
    }

    fn weather_set_precip(&mut self, start: u64, until: u64, snow: bool) {
        self.weather.precip_start = start;
        self.weather.precip_until = until;
        self.weather.precip_snow = snow;
    }

    /// Surface temperature now (degrees Urist).
    pub(crate) fn surface_temperature(&self) -> i32 {
        let (mean, swing) = self.climate_mean_swing();
        let m = self.world.calendar.month().clamp(0, 11) as usize;
        mean + swing * SEASON_OFF[m] / 1000
    }

    fn climate_mean_swing(&self) -> (i32, i32) {
        let c = &self.world.climate;
        let mean = if c.temperature != 0 { c.temperature } else { DEFAULT_MEAN };
        let swing = if c.temperature_swing != 0 { c.temperature_swing } else { DEFAULT_SWING };
        (mean, swing)
    }

    /// Ice tiletypes of the world's table: (wall, floor).
    fn ice_tiletypes(&self) -> (Option<u16>, Option<u16>) {
        let infos = &self.world.tiletypes.infos;
        let pick = |want: Shape| {
            infos
                .iter()
                .filter(|i| i.material == TtMaterial::FrozenLiquid && i.shape == want && i.direction.is_empty())
                .min_by_key(|i| (i.special != TtSpecial::Normal, i.variant, i.id))
                .map(|i| i.id)
        };
        (pick(Shape::Wall), pick(Shape::Floor))
    }

    /// Winter: the topmost water tile of each outdoor column turns into ice.
    fn weather_freeze(&mut self, now: u64) {
        self.weather.frozen = true;
        self.weather.rep.freezes += 1;
        let (wall, floor) = self.ice_tiletypes();
        let (Some(wall), Some(floor)) = (wall, floor) else { return };
        let water_mat = self.world.materials.iter().position(|m| m.token == "WATER" || m.token.ends_with(":WATER")).map(|m| m as u16);
        let layer = self.world.dims.x as usize * self.world.dims.y as usize;
        let n = self.world.tiles.len();
        let mut ice: Vec<Frozen> = Vec::new();
        for i in 0..n {
            let t = &self.world.tiles[i];
            let d = t.liquid & liquid::DEPTH_MASK;
            if d == 0 || t.liquid & liquid::MAGMA != 0 || t.flags & tile_flags::OUTSIDE == 0 {
                continue;
            }
            if i + layer < n && self.world.tiles[i + layer].liquid & liquid::DEPTH_MASK != 0 {
                continue; // not the top of the column
            }
            if self.world.tiletypes.get(t.tt).special == TtSpecial::RiverSource {
                continue;
            }
            ice.push(Frozen {
                i: i as u32,
                depth: d,
                tt: t.tt,
                mat: t.mat,
                flags: t.flags & (tile_flags::SALT | tile_flags::STAGNANT),
                ice_tt: if d >= crate::fluids::DEEP { wall } else { floor },
            });
        }
        if ice.is_empty() {
            return;
        }
        // Creatures swimming in water that freezes die (wiki Ice).
        for s in 0..self.ai.len() {
            if self.ai[s].kind == Kind::Dead {
                continue;
            }
            let p = self.ai[s].pos;
            if let Ok(k) = ice.binary_search_by_key(&p, |f| f.i) {
                if ice[k].depth >= crate::fluids::DEEP {
                    self.weather.rep.frozen_deaths += 1;
                    self.combat_kill(s, now, "has been encased in ice");
                }
            }
        }
        for f in &ice {
            let i = f.i as usize;
            let p = self.world.pos_of(i);
            self.set_liquid(p, 0, false);
            let t = &mut self.world.tiles[i];
            t.tt = f.ice_tt;
            if let Some(m) = water_mat {
                t.mat = m;
            }
            t.flags = (t.flags & !(tile_flags::SALT | tile_flags::STAGNANT)) | tile_flags::DIRTY;
            self.after_tile_change(i);
        }
        self.weather.ice = ice;
    }

    /// Spring: the ice turns back into the water it was.
    fn weather_thaw(&mut self) {
        self.weather.frozen = false;
        self.weather.rep.thaws += 1;
        let ice = std::mem::take(&mut self.weather.ice);
        for f in &ice {
            let i = f.i as usize;
            if i >= self.world.tiles.len() || self.world.tiles[i].tt != f.ice_tt {
                continue; // mined or built over
            }
            let t = &mut self.world.tiles[i];
            t.tt = f.tt;
            t.mat = f.mat;
            t.flags |= f.flags | tile_flags::DIRTY;
            self.after_tile_change(i);
            let p = self.world.pos_of(i);
            self.set_liquid(p, f.depth, false);
        }
    }

    /// Outdoor murky pool tiles (monthly; a cheap scan of the tiletype per tile).
    fn weather_scan_pools(&mut self) {
        let tt = &self.world.tiletypes;
        let mut pool_tt = vec![false; tt.infos.iter().map(|i| i.id as usize + 1).max().unwrap_or(0)];
        let mut any = false;
        for i in &tt.infos {
            if i.material == TtMaterial::Pool {
                pool_tt[i.id as usize] = true;
                any = true;
            }
        }
        self.weather.pools.clear();
        if !any {
            return;
        }
        for (i, t) in self.world.tiles.iter().enumerate() {
            if t.flags & tile_flags::OUTSIDE != 0 && pool_tt.get(t.tt as usize).copied().unwrap_or(false) {
                self.weather.pools.push(i as u32);
            }
        }
    }

    /// Rain refills murky pools.
    fn weather_refill_pools(&mut self, now: u64) {
        for k in 0..self.weather.pools.len() {
            let i = self.weather.pools[k] as usize;
            let t = self.world.tiles[i];
            let d = t.liquid & liquid::DEPTH_MASK;
            if d >= 7 || (d > 0 && t.liquid & liquid::MAGMA != 0) {
                continue;
            }
            if Rng::for_unit(self.seed, i, now, 0x4A4).chance(1, 4) {
                let p = self.world.pos_of(i);
                self.set_liquid(p, d + 1, false);
                self.weather.rep.pool_refills += 1; // [weather13]
            }
        }
    }

    /// Part of `state_hash`; mix nothing while the module is inactive.
    pub(crate) fn weather_hash(&self, mix: &mut dyn FnMut(u64)) {
        let w = &self.weather;
        let r = &w.rep;
        let idle = r.rain_ticks | r.snow_ticks | r.fog_ticks | r.evil_ticks | r.freezes | r.thaws | r.pool_refills == 0;
        if idle && !w.frozen && w.forced_until == 0 {
            return;
        }
        mix(w.kind as u64 | (w.frozen as u64) << 8 | (w.snow_cover as u64) << 9 | (w.ice.len() as u64) << 16);
        mix(r.rain_ticks ^ r.snow_ticks << 21 ^ r.fog_ticks << 42);
        mix(r.evil_ticks ^ r.freezes << 24 ^ r.thaws << 32 ^ r.frozen_deaths << 40 ^ r.pool_refills << 48);
    }

    pub(crate) fn weather_enabled(&self) -> bool {
        self.weather.enabled.unwrap_or(self.world.settings.weather)
    }

    pub(crate) fn weather_summary_impl(&self) -> WeatherInfo {
        WeatherInfo {
            kind: self.weather.kind,
            temperature: self.surface_temperature(),
            season: self.season_label(),
            frozen: self.weather.frozen,
            snow_cover: self.weather.snow_cover,
            enabled: self.weather_enabled(),
        }
    }

    fn outside(&self, p: Pos) -> bool {
        self.world.tile(p).map(|t| t.flags & tile_flags::OUTSIDE != 0).unwrap_or(false)
    }

    pub(crate) fn weather_at_impl(&self, p: Pos) -> WeatherKind {
        if !self.outside(p) {
            return WeatherKind::Clear;
        }
        match self.weather.kind {
            WeatherKind::EvilCloud => match self.weather.evil.as_ref().and_then(|e| e.center.map(|c| (c, e.radius))) {
                Some((c, r)) if (p.x as i32 - c.x as i32).abs().max((p.y as i32 - c.y as i32).abs()) > r => self.weather.base,
                _ => WeatherKind::EvilCloud,
            },
            k => k,
        }
    }

    pub(crate) fn temperature_at_impl(&self, p: Pos) -> i32 {
        match self.world.tile(p) {
            Some(t) if t.liquid & liquid::MAGMA != 0 && t.liquid & liquid::DEPTH_MASK > 0 => MAGMA_TEMP,
            Some(t) if t.flags & tile_flags::OUTSIDE == 0 => self.climate_mean_swing().0,
            _ => self.surface_temperature(),
        }
    }

    pub(crate) fn snow_at_impl(&self, p: Pos) -> bool {
        if !self.weather.snow_cover {
            return false;
        }
        match self.world.tile(p) {
            Some(t) => t.flags & tile_flags::OUTSIDE != 0 && t.liquid & liquid::DEPTH_MASK == 0 && self.world.shape(p).has_floor(),
            None => false,
        }
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
        match kind {
            WeatherKind::EvilRain | WeatherKind::EvilCloud => {
                if self.weather.evil.as_ref().map(|e| e.kind) != Some(kind) {
                    let name = self.world.climate.evil_weather.first().cloned().unwrap_or_else(|| if kind == WeatherKind::EvilRain { "blood rain".into() } else { "evil cloud".into() });
                    let mut r = Rng::for_unit(self.seed, 1, self.ticks, 0x4A6);
                    let mut e = self.weather_make_evil(&name, &mut r);
                    if kind == WeatherKind::EvilCloud && e.center.is_none() {
                        let (x, y) = (self.world.dims.x as i16 / 2, self.world.dims.y as i16 / 2);
                        e.center = Some(Pos::new(x, y, self.weather_surface_z(x, y)));
                        e.radius = 6;
                    }
                    e.kind = kind;
                    self.weather.evil = Some(e);
                }
                self.weather.evil_until = self.weather.forced_until;
            }
            _ => self.weather.base = kind,
        }
    }

    /// [weather13] Start a given evil rain/cloud for `ticks` ticks (tests, [undead13]).
    pub(crate) fn force_evil_weather_impl(&mut self, e: EvilWeather, ticks: u64) {
        self.weather.kind = e.kind;
        self.weather.evil = Some(e);
        self.weather.forced_until = self.ticks + ticks;
        self.weather.evil_until = self.weather.forced_until;
        self.weather.until = self.weather.forced_until;
    }

    pub(crate) fn weather_report_impl(&self) -> WeatherReport {
        self.weather.rep
    }
}

#[cfg(test)]
mod tests;
