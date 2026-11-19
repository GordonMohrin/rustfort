//! [fire15] Public API: temperature, fire and magma (for rf-app and for [mega13]'s dragonfire).
//! See `fire15` and `docs/mechanics/fire15.md`.

use crate::Sim;
use rf_core::Pos;
use serde::{Deserialize, Serialize};

/// Counters since the sim started (fire panel, bench, tests).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FireReport {
    pub tiles_ignited: u64,
    pub tiles_burnt: u64,
    pub items_ignited: u64,
    pub items_burned_up: u64,
    pub items_melted: u64,
    pub items_boiled: u64,
    pub units_ignited: u64,
    pub units_burned_to_death: u64,
    pub burn_wounds: u64,
    pub extinguished: u64,
    pub steam_puffs: u64,
    pub smoke_thoughts: u64,
    pub obsidian_kills: u64,
    pub buildings_melted: u64,
    pub magma_jobs_cancelled: u64,
    pub ice_melted: u64,
    pub hot_items_max: u32,
    pub cells_max: u32,
    /// Live numbers (filled by `fire_report`).
    pub hot_items: u32,
    pub burning_tiles: u32,
    pub burning_units: u32,
    pub smoke_tiles: u32,
    /// Wall-clock seconds in the fire tick (not part of the simulated state).
    pub time_s: f64,
}

impl FireReport {
    /// Nothing happened yet (keeps the state hash of forts without fire unchanged).
    pub fn is_zero(&self) -> bool {
        let z = FireReport { time_s: self.time_s, ..Default::default() };
        *self == z
    }
}

/// What the fire overlay draws on a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMark {
    Fire,
    Smoke,
}

impl Sim {
    /// Temperature is simulated (world setting `temperature`, or the player's override).
    pub fn temperature_enabled(&self) -> bool {
        self.fire15_on()
    }

    /// Player/test toggle for the temperature model.
    pub fn set_temperature_enabled(&mut self, on: bool) {
        self.fire.enabled = Some(on);
    }

    /// Set a flammable tile on fire (false if temperature is off or nothing burns there).
    pub fn ignite_tile(&mut self, p: Pos) -> bool {
        if !self.fire15_on() {
            return false;
        }
        let Some(i) = self.world.idx(p) else { return false };
        if !self.fire.ready || self.fire.heat.len() != self.world.materials.len() {
            self.fire15_init();
        }
        if !self.fire15_try_ignite_tile(i, super::fire15::heat::GRASS_FIRE_MAX) {
            return false;
        }
        let now = self.ticks;
        self.fire15_merge_ignitions(now);
        true
    }

    /// Set body parts of a unit on fire (`parts == 0`: all external parts).
    pub fn ignite_unit(&mut self, unit_id: i32, parts: u64) -> bool {
        if !self.fire15_on() {
            return false;
        }
        let Some(&s) = self.unit_slot.get(&unit_id) else { return false };
        if !self.fire.ready || self.fire.heat.len() != self.world.materials.len() {
            self.fire15_init();
        }
        let now = self.ticks;
        if parts == 0 {
            return self.fire15_ignite_unit(s, super::fire15::heat::GRASS_FIRE_MAX, 0, false, now);
        }
        // Parts already burning stay on fire; only the masked parts are added.
        let was = self.fire.burning.binary_search_by_key(&(s as u32), |b| b.slot).ok().map(|k| self.fire.burning[k].parts);
        let ok = self.fire15_ignite_unit(s, super::fire15::heat::GRASS_FIRE_MAX, 0, false, now);
        if !ok {
            return false;
        }
        let Ok(k) = self.fire.burning.binary_search_by_key(&(s as u32), |b| b.slot) else { return false };
        let added = self.fire.burning[k].parts & parts & !was.unwrap_or(0);
        self.fire.burning[k].parts = was.unwrap_or(0) | added;
        if was.is_none() && added == 0 {
            self.fire.burning.remove(k);
            self.fire.rep.units_ignited -= 1;
        }
        added != 0
    }

    /// Expose everything on tile `p` to `temp` U at once (dragonfire = 50 000): items heat up
    /// and run their thresholds, units catch fire unless immune at that temperature or
    /// FIREIMMUNE_SUPER, grass burns to ashes. No-op with temperature off.
    pub fn heat_blast(&mut self, p: Pos, temp: i32) {
        if !self.fire15_on() {
            return;
        }
        let Some(i) = self.world.idx(p) else { return };
        let now = self.ticks;
        self.fire15_heat_blast(i, temp, now);
    }

    pub fn is_burning_tile(&self, p: Pos) -> bool {
        self.world.idx(p).map_or(false, |i| self.fire.cells.binary_search_by_key(&(i as u32), |c| c.tile).is_ok())
    }

    pub fn is_burning_unit(&self, unit_id: i32) -> bool {
        self.unit_slot.get(&unit_id).map_or(false, |&s| self.fire15_burning_slot(s))
    }

    pub fn is_burning_item(&self, item_id: i32) -> bool {
        self.fire.hot.binary_search_by_key(&item_id, |e| e.id).map_or(false, |k| self.fire.hot[k].burning)
    }

    /// Temperature of an item in U, if it is being heated/cooled (None = ambient).
    pub fn item_temperature(&self, item_id: i32) -> Option<i32> {
        self.fire15_item_temp(item_id)
    }

    /// Temperature at a tile in U (environment: ambient, magma, fire).
    pub fn tile_temperature(&self, p: Pos) -> Option<i32> {
        self.world.idx(p).map(|i| self.fire15_env(i))
    }

    pub fn is_fire_safe(&self, mat: u16) -> bool {
        super::fire15::heat::fire_safe(&self.fire15_heat_any(mat))
    }

    pub fn is_magma_safe(&self, mat: u16) -> bool {
        super::fire15::heat::magma_safe(&self.fire15_heat_any(mat))
    }

    pub fn fire_report(&self) -> FireReport {
        let mut r = self.fire.rep;
        r.hot_items = self.fire.hot.len() as u32;
        r.burning_tiles = self.fire.cells.len() as u32;
        r.burning_units = self.fire.burning.len() as u32;
        r.smoke_tiles = self.fire.smoke.len() as u32;
        r
    }

    /// Burning and smoky tiles on level `z` for the overlay (appended to `out`).
    pub fn fire_marks(&self, z: i16, out: &mut Vec<(Pos, FireMark)>) {
        let lo = (z as usize * self.nav.layer) as u32;
        let hi = lo + self.nav.layer as u32;
        let a = self.fire.cells.partition_point(|c| c.tile < lo);
        for c in &self.fire.cells[a..] {
            if c.tile >= hi {
                break;
            }
            out.push((self.world.pos_of(c.tile as usize), FireMark::Fire));
        }
        let a = self.fire.smoke.partition_point(|c| c.0 < lo);
        for c in &self.fire.smoke[a..] {
            if c.0 >= hi {
                break;
            }
            out.push((self.world.pos_of(c.0 as usize), FireMark::Smoke));
        }
    }
}
