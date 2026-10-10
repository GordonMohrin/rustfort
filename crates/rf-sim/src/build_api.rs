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
        OPTIONS.iter().chain(app12_extra()).cloned().collect() // [app12] + machines, siege engines
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
        let o = opt_at(opt).ok_or("unknown build option")?; // [app12]
        if opt >= OPTIONS.len() {
            let (x1, y1, x2, y2) = Sim::footprint(o, pos, b);
            return self.can_place_dir(opt, Pos::new(x1, y1, pos.z), Pos::new(x2, y2, pos.z), ""); // [app12]
        }
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
        if opt >= OPTIONS.len() {
            // [app12] machines and siege engines
            let o = opt_at(opt).ok_or("unknown build option")?;
            let (x1, y1, x2, y2) = Sim::footprint(o, pos, b);
            return self.place_building_dir(opt, Pos::new(x1, y1, pos.z), Pos::new(x2, y2, pos.z), "");
        }
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

// ====================================================================== [app12] machines, siege engines

/// [app12] Round-12 build options: machines (DF `HOTKEY_BUILDING_MACHINE`, plus the track stop)
/// and siege engines. They follow `OPTIONS` in `build_options()`; an entry `OPTIONS` itself
/// lists (same kind and subtype) is skipped, so the indices stay consistent.
const APP12_OPTIONS: &[BuildOption] = &[
    BuildOption { kind: "ScrewPump", subtype: "", label: "Screw pump", category: "Machines", size: (1, 2), needs: "1 block, 1 enormous corkscrew, 1 pipe section", dragged: false },
    BuildOption { kind: "WaterWheel", subtype: "", label: "Water wheel", category: "Machines", size: (3, 1), needs: "3 logs or wooden blocks; flowing water below", dragged: false },
    BuildOption { kind: "Windmill", subtype: "", label: "Windmill", category: "Machines", size: (3, 3), needs: "4 logs or wooden blocks; outside (wind)", dragged: false },
    BuildOption { kind: "GearAssembly", subtype: "", label: "Gear assembly", category: "Machines", size: (1, 1), needs: "1 mechanism", dragged: false },
    BuildOption { kind: "AxleHorizontal", subtype: "", label: "Horizontal axle", category: "Machines", size: (1, 1), needs: "1 log per tile (drag up to 10 tiles)", dragged: true },
    BuildOption { kind: "AxleVertical", subtype: "", label: "Vertical axle", category: "Machines", size: (1, 1), needs: "1 log", dragged: false },
    BuildOption { kind: "Rollers", subtype: "", label: "Roller", category: "Machines", size: (1, 1), needs: "1 mechanism, 1 chain or rope (drag up to 10 tiles)", dragged: true },
    BuildOption { kind: "Trap", subtype: "TrackStop", label: "Track stop", category: "Machines", size: (1, 1), needs: "1 mechanism", dragged: false },
    BuildOption { kind: "SiegeEngine", subtype: "Ballista", label: "Ballista", category: "Siege engines", size: (3, 3), needs: "1 ballista parts", dragged: false },
    BuildOption { kind: "SiegeEngine", subtype: "Catapult", label: "Catapult", category: "Siege engines", size: (3, 3), needs: "1 catapult parts", dragged: false },
    BuildOption { kind: "SiegeEngine", subtype: "BoltThrower", label: "Bolt thrower", category: "Siege engines", size: (1, 1), needs: "1 bolt thrower parts", dragged: false },
];

/// [app12] Pump directions (DF "pump from north" ...): the input end.
pub const PUMP_DIRECTIONS: &[&str] = &["FromNorth", "FromEast", "FromSouth", "FromWest"];
/// [app12] Roller push directions and track stop dump directions ("" = no dumping).
pub const ROLLER_DIRECTIONS: &[&str] = &["N", "E", "S", "W"];
pub const DUMP_DIRECTIONS: &[&str] = &["", "N", "E", "S", "W"];
/// [app12] Siege engine facings.
pub const FACINGS: &[&str] = &["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
/// [app12] Longest dragged axle / roller (wiki: 10 tiles).
pub const MAX_LINE: i16 = 10;

fn app12_extra() -> impl Iterator<Item = &'static BuildOption> {
    APP12_OPTIONS.iter().filter(|o| !OPTIONS.iter().any(|x| x.kind == o.kind && x.subtype == o.subtype))
}

/// [app12] Option `i` of `build_options()`.
fn opt_at(i: usize) -> Option<&'static BuildOption> {
    OPTIONS.get(i).or_else(|| app12_extra().nth(i - OPTIONS.len()))
}

impl Sim {
    /// [app12] The direction choices of a build option (the first is the default; empty = none):
    /// screw pumps "pump from" N/E/S/W, water wheels and 1-tile horizontal axles `EW`/`NS`,
    /// rollers the push direction, track stops the dump direction, siege engines the facing.
    pub fn build_directions(&self, opt: usize) -> &'static [&'static str] {
        match opt_at(opt).map(|o| (o.kind, o.subtype)) {
            Some(("ScrewPump", _)) => PUMP_DIRECTIONS,
            Some(("WaterWheel", _)) | Some(("AxleHorizontal", _)) => &["EW", "NS"],
            Some(("Rollers", _)) => ROLLER_DIRECTIONS,
            Some(("Trap", "TrackStop")) => DUMP_DIRECTIONS,
            Some(("SiegeEngine", _)) => FACINGS,
            _ => &[],
        }
    }

    /// [app12] Footprint size (w, h) of a fixed-size option built with direction `dir`: screw
    /// pumps are 1x2 from north/south and 2x1 from east/west, water wheels 3x1 (`EW`) or 1x3.
    pub fn build_size(&self, opt: usize, dir: &str) -> (i16, i16) {
        let Some(o) = opt_at(opt) else { return (1, 1) };
        match o.kind {
            "ScrewPump" if dir == "FromEast" || dir == "FromWest" => (2, 1),
            "ScrewPump" => (1, 2),
            "WaterWheel" if dir == "NS" => (1, 3),
            "WaterWheel" => (3, 1),
            _ => o.size,
        }
    }

    /// [app12] Whether option `opt` fits on the rectangle `lo..=hi` (one z-level) with direction
    /// `dir`. Old options use `can_place`. Machine rules (wiki v50 Machine component, Screw pump,
    /// Water wheel, Axle, Roller, Track stop, Siege engine): screw pumps, water wheels,
    /// windmills, gear assemblies and axles may hang over open space (floor or open space, no
    /// wall); rollers, track stops and siege engines need flat floor; dragged axles and rollers
    /// are one tile wide and at most 10 long, rollers along their push direction.
    pub fn can_place_dir(&self, opt: usize, lo: Pos, hi: Pos, dir: &str) -> Result<(), String> {
        let o = opt_at(opt).ok_or("unknown build option")?;
        let (x1, y1, x2, y2) = (lo.x.min(hi.x), lo.y.min(hi.y), lo.x.max(hi.x), lo.y.max(hi.y));
        if opt < OPTIONS.len() {
            let a = Pos::new(x1, y1, lo.z);
            return self.can_place(opt, a, if o.dragged { Pos::new(x2, y2, lo.z) } else { a });
        }
        let (w, h) = (x2 - x1 + 1, y2 - y1 + 1);
        if o.dragged {
            if w > 1 && h > 1 {
                return Err("one tile wide (a straight line)".into());
            }
            if w.max(h) > MAX_LINE {
                return Err(format!("at most {MAX_LINE} tiles long"));
            }
            if o.kind == "Rollers" && w.max(h) > 1 && matches!(dir, "N" | "S") != (h > 1) {
                return Err("rollers run along their direction".into());
            }
        } else if (w, h) != self.build_size(opt, dir) {
            return Err("wrong size".into());
        }
        let hanging = matches!(o.kind, "ScrewPump" | "WaterWheel" | "Windmill" | "GearAssembly" | "AxleHorizontal" | "AxleVertical");
        for y in y1..=y2 {
            for x in x1..=x2 {
                let p = Pos::new(x, y, lo.z);
                if !self.world.in_bounds(p) {
                    return Err("outside the map".into());
                }
                if self.world.tile(p).map(|t| t.hidden()).unwrap_or(true) {
                    return Err("not revealed".into());
                }
                let shape = self.world.tt(p).shape;
                let ok = matches!(shape, Shape::Floor | Shape::Pebbles) || (hanging && matches!(shape, Shape::Empty));
                if !ok {
                    return Err(if hanging { "needs floor or open space".into() } else { "needs flat floor".into() });
                }
                if self.world.buildings.iter().any(|bl| bl.kind != "Stockpile" && bl.kind != "Civzone" && bl.contains(p)) {
                    return Err("blocked by a building".into());
                }
            }
        }
        Ok(())
    }

    /// [app12] Place option `opt` on `lo..=hi` with direction `dir` (stored in
    /// `Building::direction`; dragged horizontal axles get `NS`/`EW` from their line). Returns
    /// the new building id; the building starts unbuilt like every other.
    pub fn place_building_dir(&mut self, opt: usize, lo: Pos, hi: Pos, dir: &str) -> Result<i32, String> {
        let (x1, y1, x2, y2) = (lo.x.min(hi.x), lo.y.min(hi.y), lo.x.max(hi.x), lo.y.max(hi.y));
        if opt < OPTIONS.len() {
            let a = Pos::new(x1, y1, lo.z);
            let id = self.place_building(opt, a, if OPTIONS[opt].dragged { Pos::new(x2, y2, lo.z) } else { a })?;
            if !dir.is_empty() {
                if let Some(b) = self.world.buildings.iter_mut().find(|b| b.id == id) {
                    b.direction = dir.into();
                }
            }
            return Ok(id);
        }
        let o = opt_at(opt).ok_or("unknown build option")?;
        let dirs = self.build_directions(opt);
        let dir = if dir.is_empty() && !dirs.is_empty() && o.kind != "Trap" { dirs[0] } else { dir };
        self.can_place_dir(opt, lo, hi, dir)?;
        let dir = match o.kind {
            "AxleHorizontal" if y2 > y1 => "NS",
            "AxleHorizontal" if x2 > x1 => "EW",
            _ => dir,
        };
        let id = self.world.buildings.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        self.world.buildings.push(Building {
            id,
            kind: o.kind.into(),
            subtype: o.subtype.into(),
            x1,
            y1,
            x2,
            y2,
            z: lo.z,
            center: Pos::new((x1 + x2) / 2, (y1 + y2) / 2, lo.z),
            mat: rf_core::NO_MAT,
            stage: 0,
            built: false,
            owner: -1,
            direction: dir.into(),
            ..Default::default()
        });
        self.mark_prod_dirty();
        self.rebuild_prod_index();
        Ok(id)
    }
}

#[cfg(test)]
mod app12_tests {
    use super::*;

    #[test]
    fn machine_options_place_with_directions() {
        let mut s = Sim::new(rf_core::World::synthetic(32, 32, 12), 1);
        let opts = s.build_options();
        let find = |k: &str, sub: &str| opts.iter().position(|o| o.kind == k && o.subtype == sub).unwrap();
        let (pump, axle, roll, bal) = (find("ScrewPump", ""), find("AxleHorizontal", ""), find("Rollers", ""), find("SiegeEngine", "Ballista"));
        assert!(opts.iter().any(|o| o.category == "Machines") && opts.iter().any(|o| o.category == "Siege engines"));
        assert_eq!(s.build_size(pump, "FromWest"), (2, 1));
        assert_eq!(s.build_size(pump, "FromNorth"), (1, 2));
        // A revealed 4x4 floor of the synthetic world.
        let w = &s.world;
        let mut spot = None;
        'f: for z in 0..w.dims.z as i16 {
            for y in 2..27 {
                for x in 2..27 {
                    let ok = (0..4).all(|dy| (0..4).all(|dx| {
                        let q = Pos::new(x + dx, y + dy, z);
                        w.tt(q).shape == Shape::Floor && !w.tile(q).map(|t| t.hidden()).unwrap_or(true) && !w.buildings.iter().any(|b| b.contains(q))
                    }));
                    if ok {
                        spot = Some(Pos::new(x, y, z));
                        break 'f;
                    }
                }
            }
        }
        let p = spot.expect("a 4x4 floor");
        let id = s.place_building_dir(pump, p, p.offset(1, 0, 0), "FromWest").unwrap();
        let b = s.world.building(id).unwrap();
        assert_eq!((b.kind.as_str(), b.direction.as_str(), b.x2 - b.x1, b.y2 - b.y1), ("ScrewPump", "FromWest", 1, 0));
        assert!(s.can_place_dir(axle, p.offset(0, 1, 0), p.offset(2, 2, 0), "").is_err(), "axles are lines");
        let a = s.place_building_dir(axle, p.offset(0, 1, 0), p.offset(0, 3, 0), "").unwrap();
        assert_eq!(s.world.building(a).unwrap().direction, "NS");
        assert!(s.can_place_dir(roll, p.offset(1, 1, 0), p.offset(3, 1, 0), "N").is_err(), "rollers along their direction");
        assert!(s.can_place_dir(roll, p.offset(1, 1, 0), p.offset(3, 1, 0), "E").is_ok());
        assert!(s.can_place_dir(bal, p.offset(1, 1, 0), p.offset(3, 3, 0), "N").is_ok());
        // The old index API works for the new options too.
        assert!(s.can_place(bal, p.offset(1, 1, 0), p.offset(1, 1, 0)).is_ok());
    }
}
