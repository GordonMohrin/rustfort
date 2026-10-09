//! Round-2 player API: building placement, work orders, labors, zones, farm crops.
//!
//! PUBLIC API CONTRACT (used by rf-app): keep these signatures stable; add, don't break.
//! The production simulation behind it lives in `production.rs`.

use crate::production::RECIPES;
use crate::{labor, Sim, Task, Zone, NONE};
use rf_core::{Building, Pos, Shape, TtMaterial, WorkOrder};

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
    /// DF `job_type` name, e.g. `CustomReaction` (brewing), `PrepareMeal`, `ConstructBarrel`.
    pub job: &'static str,
    pub reaction: &'static str,
    pub item_type: &'static str,
    /// Material category (`wood`, `stone`, ...) or "".
    pub mat_category: &'static str,
    /// DF-style label, e.g. `Brew drink from plant`, `Prepare easy meal`, `Make wooden barrel`.
    pub label: &'static str,
    /// Workshop subtype that performs it, e.g. `Still`.
    pub workshop: &'static str,
    /// Item subtype (`ITEM_TOOL_LARGE_POT`) or meal kind (`EASY`, `FINE`, `LAVISH`), else "".
    pub item_subtype: &'static str,
}

/// Civzone kinds the player can create.
pub const ZONE_KINDS: &[&str] = &["MeetingHall", "Bedroom", "DiningHall", "WaterSource", "Dump", "Pen", "Tomb"];

const BUILDING_MAT: &str = "1 boulder, block or log";

const OPTIONS: &[BuildOption] = &[
    BuildOption { kind: "Workshop", subtype: "Still", label: "Still", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Kitchen", label: "Kitchen", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Carpenters", label: "Carpenter's workshop", category: "Workshops", size: (3, 3), needs: "1 log", dragged: false },
    BuildOption { kind: "Workshop", subtype: "Masons", label: "Mason's workshop", category: "Workshops", size: (3, 3), needs: "1 boulder or block", dragged: false },
    BuildOption { kind: "Workshop", subtype: "Craftsdwarfs", label: "Craftsdwarf's workshop", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Farmers", label: "Farmer's workshop", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Butchers", label: "Butcher's shop", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Fishery", label: "Fishery", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Furnace", subtype: "Smelter", label: "Smelter", category: "Furnaces", size: (3, 3), needs: "1 boulder or block", dragged: false },
    // [industry] furnaces and the remaining workshops of the industry chains.
    BuildOption { kind: "Furnace", subtype: "WoodFurnace", label: "Wood furnace", category: "Furnaces", size: (3, 3), needs: "1 boulder or block", dragged: false },
    BuildOption { kind: "Furnace", subtype: "GlassFurnace", label: "Glass furnace", category: "Furnaces", size: (3, 3), needs: "1 boulder or block", dragged: false },
    BuildOption { kind: "Furnace", subtype: "Kiln", label: "Kiln", category: "Furnaces", size: (3, 3), needs: "1 boulder or block", dragged: false },
    BuildOption { kind: "Workshop", subtype: "MetalsmithsForge", label: "Metalsmith's forge", category: "Workshops", size: (3, 3), needs: "1 anvil, 1 boulder or block", dragged: false },
    BuildOption { kind: "Workshop", subtype: "Tanners", label: "Tanner's shop", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Leatherworks", label: "Leather works", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Loom", label: "Loom", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Clothiers", label: "Clothier's shop", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Mechanics", label: "Mechanic's workshop", category: "Workshops", size: (3, 3), needs: BUILDING_MAT, dragged: false },
    BuildOption { kind: "Workshop", subtype: "Quern", label: "Quern", category: "Workshops", size: (1, 1), needs: "1 quern", dragged: false },
    BuildOption { kind: "Workshop", subtype: "Millstone", label: "Millstone", category: "Workshops", size: (1, 1), needs: "1 millstone, 1 mechanism", dragged: false },
    BuildOption { kind: "FarmPlot", subtype: "", label: "Farm plot", category: "Farming", size: (1, 1), needs: "soil floor", dragged: true },
    BuildOption { kind: "Bed", subtype: "", label: "Bed", category: "Furniture", size: (1, 1), needs: "1 bed", dragged: false },
    BuildOption { kind: "Table", subtype: "", label: "Table", category: "Furniture", size: (1, 1), needs: "1 table", dragged: false },
    BuildOption { kind: "Chair", subtype: "", label: "Chair", category: "Furniture", size: (1, 1), needs: "1 chair", dragged: false },
    BuildOption { kind: "Door", subtype: "", label: "Door", category: "Furniture", size: (1, 1), needs: "1 door", dragged: false },
    BuildOption { kind: "Cabinet", subtype: "", label: "Cabinet", category: "Furniture", size: (1, 1), needs: "1 cabinet", dragged: false },
    BuildOption { kind: "Box", subtype: "", label: "Chest", category: "Furniture", size: (1, 1), needs: "1 chest or coffer", dragged: false },
    BuildOption { kind: "Coffin", subtype: "", label: "Coffin", category: "Furniture", size: (1, 1), needs: "1 coffin", dragged: false },
    BuildOption { kind: "Well", subtype: "", label: "Well", category: "Other", size: (1, 1), needs: "1 block, 1 bucket, 1 chain or rope, 1 mechanism; open space above water", dragged: false },
];

/// A bed slot that no longer exists (its building was removed).
const DEAD_BED: u32 = u32::MAX - 1;

impl Sim {
    /// What the Build menu offers.
    pub fn build_options(&self) -> Vec<BuildOption> {
        OPTIONS.to_vec()
    }

    fn footprint(o: &BuildOption, pos: Pos, b: Pos) -> (i16, i16, i16, i16) {
        if o.dragged {
            (pos.x.min(b.x), pos.y.min(b.y), pos.x.max(b.x), pos.y.max(b.y))
        } else {
            (pos.x, pos.y, pos.x + o.size.0 - 1, pos.y + o.size.1 - 1)
        }
    }

    /// Check whether `opt` (index into `build_options()`) fits with its top-left at `pos`
    /// (`b` = opposite corner for dragged options).
    pub fn can_place(&self, opt: usize, pos: Pos, b: Pos) -> Result<(), String> {
        let o = OPTIONS.get(opt).ok_or("unknown build option")?;
        let (x1, y1, x2, y2) = Sim::footprint(o, pos, b);
        for y in y1..=y2 {
            for x in x1..=x2 {
                let p = Pos::new(x, y, pos.z);
                if !self.world.in_bounds(p) {
                    return Err("outside the map".into());
                }
                if self.world.tile(p).map(|t| t.hidden()).unwrap_or(true) {
                    return Err("not revealed".into());
                }
                let info = self.world.tt(p);
                if o.kind == "Well" {
                    if !matches!(info.shape, Shape::Empty | Shape::RampTop) || !self.water_below(p) {
                        return Err("needs open space above water at least 3/7 deep".into());
                    }
                } else if !info.shape.has_floor() {
                    return Err("needs open floor".into());
                } else if o.kind == "FarmPlot" && !(info.material == TtMaterial::Soil || info.material.is_grass()) {
                    return Err("needs soil (or muddy stone)".into());
                } else if o.kind != "FarmPlot" && !matches!(info.shape, Shape::Floor | Shape::Pebbles) {
                    return Err("needs flat floor".into());
                }
                if self.world.buildings.iter().any(|bl| bl.kind != "Stockpile" && bl.kind != "Civzone" && bl.contains(p)) {
                    return Err("blocked by a building".into());
                }
            }
        }
        Ok(())
    }

    fn water_below(&self, p: Pos) -> bool {
        let mut z = p.z - 1;
        let mut first = None;
        while z >= 0 {
            let q = Pos::new(p.x, p.y, z);
            let Some(t) = self.world.tile(q) else { break };
            let d = t.liquid_depth();
            if d > 0 && !t.is_magma() {
                let f = *first.get_or_insert(d);
                if d >= 7 || f >= 3 {
                    return true;
                }
            }
            if self.world.shape(q).is_solid() {
                break;
            }
            z -= 1;
        }
        false
    }

    /// Place a building (unbuilt; dwarves construct it). Returns the new building id.
    pub fn place_building(&mut self, opt: usize, pos: Pos, b: Pos) -> Result<i32, String> {
        self.can_place(opt, pos, b)?;
        let o = &OPTIONS[opt];
        let (x1, y1, x2, y2) = Sim::footprint(o, pos, b);
        let id = self.world.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        self.world.buildings.push(Building {
            id,
            kind: o.kind.into(),
            subtype: o.subtype.into(),
            x1,
            y1,
            x2,
            y2,
            z: pos.z,
            center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, pos.z),
            mat: rf_core::NO_MAT,
            stage: 0,
            built: false,
            owner: -1,
            crops: if o.kind == "FarmPlot" { vec![String::new(); 4] } else { Vec::new() },
            ..Default::default()
        });
        self.mark_prod_dirty();
        self.rebuild_prod_index();
        Ok(id)
    }

    /// Remove (deconstruct) a building: its materials drop where it stood, jobs at it are
    /// cancelled.
    pub fn remove_building(&mut self, id: i32) {
        let Some(bi) = self.world.buildings.iter().position(|b| b.id == id) else { return };
        self.cancel_jobs_at(id);
        let b = self.world.buildings[bi].clone();
        self.release_building_items(&b);
        match b.kind.as_str() {
            "Bed" => {
                if let Some(t) = self.world.idx(b.center) {
                    for k in 0..self.beds.len() {
                        if self.beds[k].tile == t as u32 && self.beds[k].occupant != DEAD_BED {
                            for s in 0..self.ai.len() {
                                if matches!(self.ai[s].task, Task::GoSleep(x) | Task::Sleep(x) if x == k as i32) {
                                    self.cancel_task(s);
                                    self.ai[s].wake = self.ticks + 1;
                                }
                            }
                            self.beds[k].occupant = DEAD_BED;
                        }
                    }
                }
            }
            "Stockpile" => {
                for y in b.y1..=b.y2 {
                    for x in b.x1..=b.x2 {
                        if let Some(t) = self.world.idx(Pos::new(x, y, b.z)) {
                            if let Some((p, _)) = self.pile_at.remove(&(t as u32)) {
                                let pile = &mut self.piles[p as usize];
                                pile.cats = 0;
                                pile.free = 0;
                            }
                            self.world.tiles[t].flags &= !rf_core::tile_flags::PILE;
                        }
                    }
                }
                self.pile_cats = self.piles.iter().fold(0, |a, p| a | p.cats);
            }
            _ => {}
        }
        self.world.buildings.remove(bi);
        if b.kind == "Civzone" {
            self.rebuild_zones();
        }
        if b.kind == "Stockpile" {
            self.rebuild_haul();
        }
        self.epoch_haul += 1;
        self.mark_prod_dirty();
        self.rebuild_prod_index();
    }

    fn rebuild_zones(&mut self) {
        self.zones = self
            .world
            .buildings
            .iter()
            .filter(|b| b.kind == "Civzone" && b.subtype.contains("Meeting"))
            .map(|b| Zone { x1: b.x1, y1: b.y1, x2: b.x2, y2: b.y2, z: b.z })
            .collect();
    }

    /// Every job the simulation can perform, as work-order templates (DF labels).
    pub fn order_templates(&self) -> Vec<OrderTemplate> {
        RECIPES
            .iter()
            .map(|r| OrderTemplate {
                job: r.job,
                reaction: r.reaction,
                item_type: r.item_type,
                mat_category: r.mat_category,
                label: r.label,
                workshop: r.workshop,
                item_subtype: r.item_subtype,
            })
            .collect()
    }

    /// Add a work order from a template. `frequency`: `OneTime`, `Daily`, `Monthly`, ...
    pub fn add_work_order(&mut self, template: usize, amount: i32, frequency: &str) -> i32 {
        let id = self.world.work_orders.iter().map(|o| o.id).max().unwrap_or(0) + 1;
        if let Some(t) = RECIPES.get(template) {
            self.world.work_orders.push(WorkOrder {
                id,
                job: t.job.into(),
                reaction: t.reaction.into(),
                item_type: t.item_type.into(),
                item_subtype: t.item_subtype.into(),
                mat_category: if t.mat_category.is_empty() { vec![] } else { vec![t.mat_category.into()] },
                amount_left: amount,
                amount_total: amount,
                frequency: frequency.into(),
                workshop_id: -1,
                active: true,
                validated: true,
                ..Default::default()
            });
            self.mark_prod_dirty();
        }
        id
    }

    /// Queue a job directly in a workshop (like DF's workshop "Add job" menu).
    pub fn add_building_job(&mut self, building_id: i32, template: usize, repeat: bool) {
        let Some(t) = RECIPES.get(template) else { return };
        if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == building_id) {
            if b.jobs.len() >= crate::production::MAX_QUEUE {
                return;
            }
            b.jobs.push(rf_core::BuildingJob {
                job: t.job.into(),
                reaction: t.reaction.into(),
                item_type: t.item_type.into(),
                item_subtype: t.item_subtype.into(),
                mat_category: if t.mat_category.is_empty() { vec![] } else { vec![t.mat_category.into()] },
                repeat,
                order_id: -1,
                ..Default::default()
            });
            self.mark_prod_dirty();
        }
    }

    /// Order templates a given workshop can perform (indices into `order_templates()`).
    pub fn templates_for_workshop(&self, subtype: &str) -> Vec<usize> {
        RECIPES.iter().enumerate().filter(|(_, t)| t.workshop == subtype).map(|(i, _)| i).collect()
    }

    /// Remove a work order and the jobs it queued that nobody has started.
    pub fn remove_work_order(&mut self, id: i32) {
        self.world.work_orders.retain(|o| o.id != id);
        let busy: Vec<i32> = self
            .prod
            .shops
            .iter()
            .filter(|s| s.busy != NONE)
            .map(|s| s.bid)
            .collect();
        for b in &mut self.world.buildings {
            if !b.jobs.is_empty() && !busy.contains(&b.id) {
                b.jobs.retain(|j| j.order_id != id);
            }
        }
        self.mark_prod_dirty();
    }

    /// Enable/disable a labor (DF `unit_labor` name, e.g. `MINE`, `BREWER`) for a unit.
    pub fn set_labor(&mut self, unit_id: i32, labor: &str, on: bool) {
        let Some(u) = self.world.units.iter_mut().find(|u| u.id == unit_id) else { return };
        u.labors.retain(|l| l != labor);
        if on {
            u.labors.push(labor.to_string());
        }
        let bits = labor::bits(&u.labors);
        if let Some(&s) = self.unit_slot.get(&unit_id) {
            self.ai[s].labors = bits;
            self.ai[s].seen_jobs = 0;
            self.ai[s].seen_haul = 0;
            self.ai[s].seen_prod = 0;
        }
    }

    /// Create a civzone over the inclusive box on `a.z` (`zone` from [`ZONE_KINDS`]).
    /// Meeting halls are where idle dwarves gather; the other kinds are stored for the UI.
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
        if zone.contains("Meeting") {
            self.zones.push(Zone { x1, y1, x2, y2, z: a.z });
        }
        self.prod.known_buildings = self.world.buildings.len();
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
            self.mark_prod_dirty();
        }
    }

    /// Plants that can be planted on farm plot `building_id` (underground/aboveground match,
    /// any season; see [`Sim::crop_options_for_season`]).
    pub fn crop_options(&self, building_id: i32) -> Vec<String> {
        self.crop_filter(building_id, None)
    }

    /// Plants that can be planted on farm plot `building_id` in `season` (0 spring .. 3 winter).
    pub fn crop_options_for_season(&self, building_id: i32, season: usize) -> Vec<String> {
        self.crop_filter(building_id, Some(season))
    }

    fn crop_filter(&self, building_id: i32, season: Option<usize>) -> Vec<String> {
        let under = self.is_underground_plot(building_id);
        self.plant_infos()
            .iter()
            .filter(|p| !p.is_tree && p.has_seed && p.mat_struct != rf_core::NO_MAT)
            .filter(|p| match under {
                Some(true) => p.underground,
                Some(false) => p.aboveground,
                None => true,
            })
            .filter(|p| season.map(|s| p.seasons & (1 << s.min(3)) != 0).unwrap_or(true))
            .map(|p| p.token.clone())
            .collect()
    }
}
