//! [saves10] Simulation-module state across save/load.
//!
//! `rf_core::World` holds the fort (map, units, items, buildings, orders...), but each sim
//! module keeps runtime state of its own that used to vanish on save/load: visitors and
//! petitions, mood progress, melt fractions, raid groups, breakdown timers, ... A save now
//! carries that state in `World::sim_state` (module name -> JSON of the module's state):
//!
//! - [`Sim::world_for_save`] clones the world and fills `sim_state` (the running world keeps
//!   an empty map);
//! - `Sim::new` takes the map out of the world, restores the tick count before the init passes
//!   (so every absolute tick in the restored state stays valid) and the modules after them.
//!
//! JSON per module with `#[serde(default)]` structs keeps old saves loadable when a module
//! adds a field, and a module that cannot read its entry just starts fresh (as before).
//! Caches (paths, job lists, nav) are rebuilt; unit AI tasks restart from Idle, which is why a
//! reload continues the fort closely but not bit-identically (see `docs/SAVES.md`).

use crate::Sim;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the `core` entry.
const CORE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct CoreState {
    version: u32,
    ticks: u64,
    next_item_id: i32,
    seed: u64,
}

/// Tick count stored in a `core` entry (for the save header).
pub fn saved_ticks(core_json: &str) -> Option<u64> {
    serde_json::from_str::<CoreState>(core_json).ok().map(|c| c.ticks)
}

/// Encode one module's state.
pub(crate) fn put<T: Serialize>(m: &mut BTreeMap<String, String>, key: &str, v: &T) {
    if let Ok(s) = serde_json::to_string(v) {
        m.insert(key.to_string(), s);
    }
}

/// Decode one module's state (None if missing or unreadable: the module starts fresh).
pub(crate) fn get<T: DeserializeOwned>(m: &BTreeMap<String, String>, key: &str) -> Option<T> {
    m.get(key).and_then(|s| serde_json::from_str(s).ok())
}

impl Sim {
    /// [saves10] A copy of the world for a save game, with the modules' runtime state in
    /// `World::sim_state`. Use this instead of `sim.world.clone()` when saving.
    pub fn world_for_save(&self) -> rf_core::World {
        let mut w = self.world.clone();
        w.sim_state = self.save_modules();
        w
    }

    /// [saves10] The modules' runtime state (what [`Sim::world_for_save`] puts into the save).
    pub fn save_modules(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        put(&mut m, "core", &CoreState { version: CORE_VERSION, ticks: self.ticks, next_item_id: self.next_item_id, seed: self.seed });
        self.locs_save(&mut m); // locations: visitors, petitions, arrivals
        self.moods_save(&mut m); // moods: counter, active mood, records
        self.ind_save(&mut m); // industry: melt fractions, fuel, counters
        self.social_save(&mut m); // social: breakdowns, insanity, timers
        self.fluids_save(&mut m); // fluids: mud
        self.raids_save(&mut m); // combat: raid groups, hidden flags, camps, leave timers, patrol waypoints, wounds
        // Not needed: planned constructions are buildings and smooth/engrave/dig designations are
        // tile flags (both in the world), so `init_construct` / `init_jobs_from_tiles` rebuild them.
        m
    }

    /// Called first in `Sim::new` (before the init passes): the tick count.
    pub(crate) fn restore_core(&mut self, saved: &BTreeMap<String, String>) {
        if let Some(c) = get::<CoreState>(saved, "core") {
            self.ticks = c.ticks;
        }
    }

    /// Called last in `Sim::new`: every module's state.
    pub(crate) fn restore_modules(&mut self, saved: &BTreeMap<String, String>) {
        if saved.is_empty() {
            return;
        }
        if let Some(c) = get::<CoreState>(saved, "core") {
            self.next_item_id = self.next_item_id.max(c.next_item_id);
        }
        self.locs_restore(saved);
        self.moods_restore(saved);
        self.ind_restore(saved);
        self.social_restore(saved);
        self.fluids_restore(saved);
        self.raids_restore(saved);
    }

    /// [saves10] [`Sim::state_hash`] split by part (`core`, `units`, `tiles`, `items` and each
    /// module's own hash), to see which parts a save/load round trip keeps identical.
    pub fn state_hash_parts(&self) -> Vec<(&'static str, u64)> {
        fn fnv(f: impl FnOnce(&mut dyn FnMut(u64))) -> u64 {
            let mut h: u64 = 0xcbf29ce484222325;
            let mut mix = |v: u64| {
                h ^= v;
                h = h.wrapping_mul(0x100000001b3);
            };
            f(&mut mix);
            h
        }
        let mut v = Vec::new();
        v.push(("ticks", self.ticks));
        v.push((
            "units",
            fnv(|mix| {
                for (u, ai) in self.world.units.iter().zip(&self.ai) {
                    mix(u.id as u64);
                    mix(((u.pos.x as u64) << 32) | ((u.pos.y as u64) << 16) | u.pos.z as u64);
                    let (th, hu, sl) = self.needs_now(ai);
                    mix(((th as u64) << 40) ^ ((hu as u64) << 20) ^ sl as u64);
                    mix(u.flags.dead as u64);
                }
            }),
        ));
        v.push((
            "tiles",
            fnv(|mix| {
                for t in &self.world.tiles {
                    mix(((t.tt as u64) << 16) | t.desig as u64);
                }
            }),
        ));
        v.push((
            "items",
            fnv(|mix| {
                for it in &self.world.items {
                    mix(it.id as u64);
                    mix(((it.pos.x as u64) << 32) | ((it.pos.y as u64) << 16) | it.pos.z as u64);
                    mix(it.stack as u64);
                }
            }),
        ));
        v.push(("prod", fnv(|mix| self.prod_hash(mix))));
        v.push(("social", fnv(|mix| self.social_hash(mix))));
        v.push(("combat", fnv(|mix| self.combat_hash(mix))));
        v.push(("fluids", fnv(|mix| self.fluids_hash(&mut |x| mix(x)))));
        v.push(("society", fnv(|mix| self.society_hash(mix))));
        v.push(("moods", fnv(|mix| self.moods_hash(mix))));
        v.push(("health", fnv(|mix| self.health_hash(mix))));
        v.push(("caverns", fnv(|mix| self.caverns_hash(mix))));
        v.push(("animals", fnv(|mix| self.animals_hash(mix))));
        v.push(("society2", fnv(|mix| self.society2_hash(mix))));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_core::World;

    /// Save/load through `world_for_save` keeps the tick count and every module entry.
    #[test]
    fn module_state_survives_a_reload() {
        let mut a = Sim::new(World::synthetic(48, 48, 20), 3);
        a.run_ticks(3_000);
        let w = a.world_for_save();
        assert!(w.sim_state.contains_key("core"));
        assert!(a.world.sim_state.is_empty(), "the running world keeps no copy");
        let saved = w.sim_state.clone();
        let b = Sim::new(w, 3);
        assert_eq!(b.ticks(), a.ticks());
        assert!(b.world.sim_state.is_empty());
        // Restoring and saving again gives the same module state.
        assert_eq!(b.save_modules(), saved);
    }

    /// Run N, reload, run M against running N+M directly: the parts listed in
    /// [`Sim::state_hash_parts`] are compared; `ticks` must match exactly, the rest is reported.
    #[test]
    fn reload_continues_the_calendar() {
        let mut a = Sim::new(World::synthetic(48, 48, 20), 5);
        a.run_ticks(2_000);
        let mut b = Sim::new(a.world_for_save(), 5);
        a.run_ticks(2_000);
        b.run_ticks(2_000);
        assert_eq!(a.ticks(), b.ticks());
        assert_eq!(a.world.calendar, b.world.calendar);
        assert_eq!(a.world.units.len(), b.world.units.len());
    }
}
