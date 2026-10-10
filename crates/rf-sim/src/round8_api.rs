//! Round-8 player API: work details, stockpile settings, kitchen, animals, locations, justice.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies are minimal stubs; the round-8 simulation modules replace them.

use crate::Sim;
use rf_core::{Crime, KitchenSetting, Location, Occupation, StockpileSettings, WorkDetail};

/// Animal overview row for the UI (DF Steam "Animals" screen).
#[derive(Clone, Debug, Default)]
pub struct AnimalInfo {
    pub unit: i32,
    pub race: String,
    pub caste: String,
    pub training: String,
    pub owner: i32,
    pub pasture: i32,
    pub slaughter: bool,
    pub pregnant: bool,
    pub grazer: bool,
    pub milkable: bool,
    pub shearable: bool,
    pub egg_layer: bool,
    /// Assigned trainer/hunter role, if any (e.g. `War`, `Hunting`).
    pub role: String,
}

impl Sim {
    // ---------------------------------------------------------------- work details (v50)
    pub fn create_work_detail(&mut self, name: &str, labors: &[&str]) {
        self.world.work_details.push(WorkDetail {
            name: name.into(),
            labors: labors.iter().map(|s| s.to_string()).collect(),
            mode: "OnlySelectedDoesThis".into(),
            ..Default::default()
        });
    }

    /// Assign/unassign a unit to a work detail by name; the sim derives labors from details.
    pub fn set_work_detail_member(&mut self, detail: &str, unit_id: i32, on: bool) {
        if let Some(d) = self.world.work_details.iter_mut().find(|d| d.name == detail) {
            d.assigned.retain(|&u| u != unit_id);
            if on {
                d.assigned.push(unit_id);
            }
        }
    }

    /// `EverybodyDoesThis`, `OnlySelectedDoesThis`, `NobodyDoesThis`.
    pub fn set_work_detail_mode(&mut self, detail: &str, mode: &str) {
        if let Some(d) = self.world.work_details.iter_mut().find(|d| d.name == detail) {
            d.mode = mode.into();
        }
    }

    pub fn set_work_detail_labors(&mut self, detail: &str, labors: &[&str]) {
        if let Some(d) = self.world.work_details.iter_mut().find(|d| d.name == detail) {
            d.labors = labors.iter().map(|s| s.to_string()).collect();
        }
    }

    // ---------------------------------------------------------------- stockpiles & kitchen
    /// Replace a stockpile's settings (categories, detailed filters, containers, links).
    pub fn set_stockpile_settings(&mut self, building_id: i32, settings: StockpileSettings) {
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == building_id) {
            b.stockpile = Some(settings);
        }
    }

    /// All leaf filters DF offers for a stockpile category (for the settings editor),
    /// as `category/group/token` strings with a display label.
    pub fn stockpile_filter_options(&self, _category: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    pub fn set_kitchen(&mut self, setting: KitchenSetting) {
        self.world.kitchen.retain(|k| !(k.item_type == setting.item_type && k.mat == setting.mat && k.item_subtype == setting.item_subtype));
        if setting.no_cook || setting.no_brew {
            self.world.kitchen.push(setting);
        }
    }

    // ---------------------------------------------------------------- animals
    pub fn animals(&self) -> Vec<AnimalInfo> {
        self.world
            .units
            .iter()
            .filter(|u| u.flags.animal && u.flags.tame && !u.flags.dead)
            .map(|u| AnimalInfo {
                unit: u.id,
                race: u.race.clone(),
                caste: u.caste.clone(),
                training: u.training.clone(),
                owner: u.pet_owner,
                pasture: u.pasture,
                slaughter: u.slaughter,
                pregnant: u.pregnancy > 0,
                ..Default::default()
            })
            .collect()
    }

    pub fn set_slaughter(&mut self, unit_id: i32, on: bool) {
        if let Some(u) = self.world.units.iter_mut().find(|u| u.id == unit_id) {
            u.slaughter = on;
        }
    }

    /// Assign an animal to a pasture/pen civzone (-1 = none).
    pub fn assign_pasture(&mut self, unit_id: i32, zone_id: i32) {
        if let Some(u) = self.world.units.iter_mut().find(|u| u.id == unit_id) {
            u.pasture = zone_id;
        }
    }

    /// Request training (`War`, `Hunting`, `Tame`) for an animal; a trainer does it.
    pub fn request_training(&mut self, _unit_id: i32, _kind: &str) {}

    // ---------------------------------------------------------------- locations
    /// Turn a civzone into a location (`Temple`, `Tavern`, `Library`, `Guildhall`, `Hospital`).
    pub fn create_location(&mut self, kind: &str, zone_id: i32, name: &str) -> i32 {
        let id = self.world.locations.iter().map(|l| l.id).max().unwrap_or(0) + 1;
        self.world.locations.push(Location {
            id,
            kind: kind.into(),
            name: name.into(),
            deity: -1,
            zones: vec![zone_id],
            ..Default::default()
        });
        id
    }

    pub fn assign_occupation(&mut self, location_id: i32, kind: &str, unit_id: i32) {
        if let Some(l) = self.world.locations.iter_mut().find(|l| l.id == location_id) {
            match l.occupations.iter_mut().find(|o| o.kind == kind) {
                Some(o) => o.unit = unit_id,
                None => l.occupations.push(Occupation { kind: kind.into(), unit: unit_id }),
            }
        }
    }

    // ---------------------------------------------------------------- justice
    pub fn crimes(&self) -> &[Crime] {
        &self.world.crimes
    }

    /// Convict a unit for a crime (the captain of the guard / sheriff punishes per DF).
    pub fn convict(&mut self, crime_id: i32, unit_id: i32) {
        if let Some(c) = self.world.crimes.iter_mut().find(|c| c.id == crime_id) {
            c.convicted = unit_id;
        }
    }
}
