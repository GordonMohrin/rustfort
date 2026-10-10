//! Round-12 player API: hauling routes / minecarts, machines, siege engines.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies are minimal stubs; the minecart, machine and siege-engine modules replace them.

use crate::Sim;
use rf_core::{HaulRoute, HaulStop, Pos};

/// Live state of a hauling route for the UI.
#[derive(Clone, Debug, Default)]
pub struct RouteStatus {
    pub id: i32,
    /// Vehicle item ids and where they are (stop index or "moving").
    pub vehicles: Vec<(i32, String)>,
    /// Items currently loaded per vehicle.
    pub loaded: Vec<usize>,
    pub trips: u64,
}

/// Siege engine kinds (DF v50).
pub const SIEGE_ENGINES: &[&str] = &["Ballista", "Catapult", "BoltThrower"];

impl Sim {
    /// Create a hauling route; returns its id.
    pub fn create_haul_route(&mut self, name: &str) -> i32 {
        let id = self.world.haul_routes.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        self.world.haul_routes.push(HaulRoute { id, name: name.into(), ..Default::default() });
        id
    }

    pub fn remove_haul_route(&mut self, id: i32) {
        self.world.haul_routes.retain(|r| r.id != id);
    }

    /// Append a stop at `pos` (normally on a track stop or a track tile).
    pub fn add_route_stop(&mut self, route: i32, name: &str, pos: Pos) -> usize {
        match self.world.haul_routes.iter_mut().find(|r| r.id == route) {
            Some(r) => {
                r.stops.push(HaulStop { name: name.into(), pos, ..Default::default() });
                r.stops.len() - 1
            }
            None => usize::MAX,
        }
    }

    pub fn remove_route_stop(&mut self, route: i32, stop: usize) {
        if let Some(r) = self.world.haul_routes.iter_mut().find(|r| r.id == route) {
            if stop < r.stops.len() {
                r.stops.remove(stop);
            }
        }
    }

    /// Link a stockpile to a stop: `take` = the stop loads from the pile, else it unloads into it.
    pub fn link_stop_pile(&mut self, route: i32, stop: usize, pile: i32, take: bool) {
        if let Some(s) = self.world.haul_routes.iter_mut().find(|r| r.id == route).and_then(|r| r.stops.get_mut(stop)) {
            let v = if take { &mut s.take_from } else { &mut s.give_to };
            if !v.contains(&pile) {
                v.push(pile);
            }
        }
    }

    /// Departure conditions as DF-like text (e.g. `Leave when full`, `Wait 1 day`, `Push`, `Ride`, `Guide`).
    pub fn set_stop_conditions(&mut self, route: i32, stop: usize, conditions: &[&str]) {
        if let Some(s) = self.world.haul_routes.iter_mut().find(|r| r.id == route).and_then(|r| r.stops.get_mut(stop)) {
            s.conditions = conditions.iter().map(|c| c.to_string()).collect();
        }
    }

    /// Assign a minecart (or wheelbarrow) item to a route.
    pub fn assign_vehicle(&mut self, route: i32, item: i32) {
        // [carts12] A vehicle belongs to one route.
        if !self.world.haul_routes.iter().any(|r| r.id == route) {
            return;
        }
        for r in self.world.haul_routes.iter_mut().filter(|r| r.id != route) {
            r.vehicles.retain(|&v| v != item);
        }
        if let Some(r) = self.world.haul_routes.iter_mut().find(|r| r.id == route) {
            if !r.vehicles.contains(&item) {
                r.vehicles.push(item);
            }
        }
    }

    pub fn route_status(&self, route: i32) -> RouteStatus {
        self.route_status_impl(route) // [carts12]
    }

    /// Order a built siege engine to fire at a target tile (or `None` = fire at will).
    pub fn aim_siege_engine(&mut self, building: i32, target: Option<Pos>) {
        self.engines_aim(building, target); // [siege12] see `siegeengines`
    }
}
