//! Round-2 player API: building placement, work orders, labors, zones, farm crops.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The bodies are minimal stubs; the real production simulation replaces them.

use crate::Sim;
use rf_core::{Building, Pos, WorkOrder};

/// Something the Build menu can place.
#[derive(Clone, Debug)]
pub struct BuildOption {
    /// rf-core building kind (DF `building_type` name), e.g. `Workshop`, `Furnace`, `Bed`.
    pub kind: &'static str,
    /// Subtype (DF workshop/furnace type name), e.g. `Still`, `Kitchen`, `Carpenters`; "" if none.
    pub subtype: &'static str,
    /// Menu label as in DF, e.g. `Still`, `Carpenter's workshop`, `Bed`.
    pub label: &'static str,
    /// Menu group: `Workshops`, `Furnaces`, `Furniture`, `Constructions`, `Farming`.
    pub category: &'static str,
    /// Footprint in tiles (w, h). Farm plots are dragged (size (1,1) = per tile).
    pub size: (i16, i16),
    /// Human-readable material requirement, e.g. `1 boulder or log`.
    pub needs: &'static str,
    /// Whether the footprint is dragged as a rectangle (farm plots) instead of fixed.
    pub dragged: bool,
}

/// A work order the player can add from the Work orders screen.
#[derive(Clone, Debug)]
pub struct OrderTemplate {
    /// DF `job_type` name, e.g. `BrewDrink`, `PrepareMeal`, `ConstructBarrel`.
    pub job: &'static str,
    pub reaction: &'static str,
    pub item_type: &'static str,
    /// Material category (`wood`, `stone`, ...) or "".
    pub mat_category: &'static str,
    /// DF-style label, e.g. `Brew drink`, `Prepare easy meal`, `Make wooden barrel`.
    pub label: &'static str,
    /// Workshop subtype that performs it, e.g. `Still`.
    pub workshop: &'static str,
}

/// Civzone kinds the player can create.
pub const ZONE_KINDS: &[&str] = &["MeetingHall", "Bedroom", "DiningHall", "WaterSource", "Dump", "Pen", "Tomb"];

impl Sim {
    /// What the Build menu offers.
    pub fn build_options(&self) -> Vec<BuildOption> {
        vec![
            BuildOption { kind: "Workshop", subtype: "Still", label: "Still", category: "Workshops", size: (3, 3), needs: "1 boulder or log", dragged: false },
            BuildOption { kind: "Workshop", subtype: "Kitchen", label: "Kitchen", category: "Workshops", size: (3, 3), needs: "1 boulder or log", dragged: false },
            BuildOption { kind: "Workshop", subtype: "Carpenters", label: "Carpenter's workshop", category: "Workshops", size: (3, 3), needs: "1 log", dragged: false },
            BuildOption { kind: "Workshop", subtype: "Masons", label: "Mason's workshop", category: "Workshops", size: (3, 3), needs: "1 boulder", dragged: false },
            BuildOption { kind: "FarmPlot", subtype: "", label: "Farm plot", category: "Farming", size: (1, 1), needs: "soil or muddy floor", dragged: true },
            BuildOption { kind: "Bed", subtype: "", label: "Bed", category: "Furniture", size: (1, 1), needs: "1 bed", dragged: false },
            BuildOption { kind: "Table", subtype: "", label: "Table", category: "Furniture", size: (1, 1), needs: "1 table", dragged: false },
            BuildOption { kind: "Chair", subtype: "", label: "Chair", category: "Furniture", size: (1, 1), needs: "1 chair", dragged: false },
            BuildOption { kind: "Door", subtype: "", label: "Door", category: "Furniture", size: (1, 1), needs: "1 door", dragged: false },
        ]
    }

    /// Check whether `opt` (index into `build_options()`) fits with its top-left at `pos`
    /// (`b` = opposite corner for dragged options).
    pub fn can_place(&self, opt: usize, pos: Pos, b: Pos) -> Result<(), String> {
        let o = self.build_options().get(opt).cloned().ok_or("unknown build option")?;
        let (x2, y2) = if o.dragged { (b.x.max(pos.x), b.y.max(pos.y)) } else { (pos.x + o.size.0 - 1, pos.y + o.size.1 - 1) };
        for y in pos.y..=y2 {
            for x in pos.x..=x2 {
                let p = Pos::new(x, y, pos.z);
                if !self.world.in_bounds(p) || !self.world.shape(p).has_floor() {
                    return Err("needs open floor".into());
                }
                if self.world.buildings.iter().any(|bl| bl.kind != "Stockpile" && bl.kind != "Civzone" && bl.contains(p)) {
                    return Err("blocked by a building".into());
                }
            }
        }
        Ok(())
    }

    /// Place a building (unbuilt; dwarves construct it). Returns the new building id.
    pub fn place_building(&mut self, opt: usize, pos: Pos, b: Pos) -> Result<i32, String> {
        self.can_place(opt, pos, b)?;
        let o = self.build_options()[opt].clone();
        let (x2, y2) = if o.dragged { (b.x.max(pos.x), b.y.max(pos.y)) } else { (pos.x + o.size.0 - 1, pos.y + o.size.1 - 1) };
        let id = self.world.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        self.world.buildings.push(Building {
            id,
            kind: o.kind.into(),
            subtype: o.subtype.into(),
            x1: pos.x,
            y1: pos.y,
            x2,
            y2,
            z: pos.z,
            center: Pos::new((pos.x + x2) / 2, (pos.y + y2) / 2, pos.z),
            mat: rf_core::NO_MAT,
            stage: 0,
            built: false,
            owner: -1,
            crops: if o.kind == "FarmPlot" { vec![String::new(); 4] } else { Vec::new() },
            ..Default::default()
        });
        Ok(id)
    }

    /// Remove (deconstruct) a building.
    pub fn remove_building(&mut self, id: i32) {
        self.world.buildings.retain(|b| b.id != id);
    }

    pub fn order_templates(&self) -> Vec<OrderTemplate> {
        vec![
            OrderTemplate { job: "BrewDrink", reaction: "BREW_DRINK_FROM_PLANT", item_type: "DRINK", mat_category: "", label: "Brew drink from plant", workshop: "Still" },
            OrderTemplate { job: "PrepareMeal", reaction: "", item_type: "FOOD", mat_category: "", label: "Prepare easy meal", workshop: "Kitchen" },
            OrderTemplate { job: "ConstructBarrel", reaction: "", item_type: "BARREL", mat_category: "wood", label: "Make wooden barrel", workshop: "Carpenters" },
            OrderTemplate { job: "ConstructBed", reaction: "", item_type: "BED", mat_category: "wood", label: "Construct wooden bed", workshop: "Carpenters" },
        ]
    }

    /// Add a work order from a template. `frequency`: `OneTime`, `Daily`, `Monthly`, ...
    pub fn add_work_order(&mut self, template: usize, amount: i32, frequency: &str) -> i32 {
        let t = self.order_templates().get(template).cloned();
        let id = self.world.work_orders.iter().map(|o| o.id).max().unwrap_or(0) + 1;
        if let Some(t) = t {
            self.world.work_orders.push(WorkOrder {
                id,
                job: t.job.into(),
                reaction: t.reaction.into(),
                item_type: t.item_type.into(),
                mat_category: if t.mat_category.is_empty() { vec![] } else { vec![t.mat_category.into()] },
                amount_left: amount,
                amount_total: amount,
                frequency: frequency.into(),
                workshop_id: -1,
                active: true,
                validated: true,
                ..Default::default()
            });
        }
        id
    }

    /// Queue a job directly in a workshop (like DF's workshop "Add job" menu).
    pub fn add_building_job(&mut self, building_id: i32, template: usize, repeat: bool) {
        let Some(t) = self.order_templates().get(template).cloned() else { return };
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == building_id) {
            b.jobs.push(rf_core::BuildingJob {
                job: t.job.into(),
                reaction: t.reaction.into(),
                item_type: t.item_type.into(),
                mat_category: if t.mat_category.is_empty() { vec![] } else { vec![t.mat_category.into()] },
                repeat,
                order_id: -1,
                ..Default::default()
            });
        }
    }

    /// Order templates a given workshop can perform (indices into `order_templates()`).
    pub fn templates_for_workshop(&self, subtype: &str) -> Vec<usize> {
        self.order_templates().iter().enumerate().filter(|(_, t)| t.workshop == subtype).map(|(i, _)| i).collect()
    }

    pub fn remove_work_order(&mut self, id: i32) {
        self.world.work_orders.retain(|o| o.id != id);
    }

    /// Enable/disable a labor (DF `unit_labor` name, e.g. `MINE`, `BREW`) for a unit.
    pub fn set_labor(&mut self, unit_id: i32, labor: &str, on: bool) {
        if let Some(u) = self.world.units.iter_mut().find(|u| u.id == unit_id) {
            u.labors.retain(|l| l != labor);
            if on {
                u.labors.push(labor.to_string());
            }
        }
    }

    /// Create a civzone over the inclusive box on `a.z` (`zone` from [`ZONE_KINDS`]).
    pub fn create_zone(&mut self, a: Pos, b: Pos, zone: &str) -> i32 {
        let id = self.world.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        let (x1, x2, y1, y2) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        self.world.buildings.push(Building {
            id,
            kind: "Civzone".into(),
            subtype: zone.into(),
            x1,
            y1,
            x2,
            y2,
            z: a.z,
            center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, a.z),
            built: true,
            owner: -1,
            ..Default::default()
        });
        id
    }

    /// Set the crop of a farm plot for a season (0 spring .. 3 winter); "" = fallow.
    pub fn set_farm_crop(&mut self, building_id: i32, season: usize, plant_token: &str) {
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == building_id) {
            if b.crops.len() < 4 {
                b.crops.resize(4, String::new());
            }
            if season < 4 {
                b.crops[season] = plant_token.to_string();
            }
        }
    }

    /// Plants that can be planted on farm plot `building_id` (underground/aboveground match).
    pub fn crop_options(&self, _building_id: i32) -> Vec<String> {
        self.world.plant_raws.iter().filter(|p| !p.is_tree && p.has_seed).map(|p| p.token.clone()).collect()
    }
}
