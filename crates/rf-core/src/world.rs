//! The world: map tiles + entities + metadata.

use crate::entities::*;
use crate::military::*;
use crate::society::*;
use crate::tiles::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dims {
    pub x: u16,
    pub y: u16,
    pub z: u16,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    /// Fort name as DF shows it, e.g. `Trussroads`.
    pub fort_name: String,
    /// Native name, e.g. `Tileshtulon`.
    pub fort_name_native: String,
    pub world_name: String,
    pub site_id: i32,
    pub civ_id: i32,
    pub group_id: i32,
    pub df_version: String,
    pub dfhack_version: String,
    /// World-region origin of the embark (DF `region_x/y/z`).
    pub region_x: i32,
    pub region_y: i32,
    pub region_z: i32,
    /// Where this world came from (`export:<path>` or `synthetic`).
    pub source: String,
    /// Active mods of the save in load order (DF mod ids with version, e.g. `perspective_walls:25`).
    pub mods: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    pub year: i32,
    /// Ticks since the start of the year, 0..TICKS_PER_YEAR.
    pub year_tick: i32,
}

impl Calendar {
    pub fn advance(&mut self, ticks: i32) {
        self.year_tick += ticks;
        while self.year_tick >= crate::TICKS_PER_YEAR {
            self.year_tick -= crate::TICKS_PER_YEAR;
            self.year += 1;
        }
    }
    /// 0-based month (0 = Granite).
    pub fn month(&self) -> i32 {
        self.year_tick / crate::TICKS_PER_MONTH
    }
    /// 1-based day of month.
    pub fn day(&self) -> i32 {
        (self.year_tick % crate::TICKS_PER_MONTH) / crate::TICKS_PER_DAY + 1
    }
    pub fn month_name(&self) -> &'static str {
        const N: [&str; 12] = [
            "Granite", "Slate", "Felsite", "Hematite", "Malachite", "Galena", "Limestone", "Sandstone", "Timber",
            "Moonstone", "Opal", "Obsidian",
        ];
        N[(self.month().clamp(0, 11)) as usize]
    }
    pub fn season_name(&self) -> &'static str {
        ["Spring", "Summer", "Autumn", "Winter"][(self.month() / 3).clamp(0, 3) as usize]
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct World {
    pub meta: Meta,
    pub calendar: Calendar,
    pub dims: Dims,
    /// Index = `(z * dims.y + y) * dims.x + x`.
    pub tiles: Vec<Tile>,
    pub tiletypes: TiletypeTable,
    /// Material palette; `Tile::mat` and `Item::mat` index into it.
    pub materials: Vec<MaterialInfo>,
    pub units: Vec<Unit>,
    pub items: Vec<Item>,
    pub buildings: Vec<Building>,
    pub plants: Vec<Plant>,
    /// Manager work orders.
    #[serde(default)]
    pub work_orders: Vec<WorkOrder>,
    /// Plant raws (crops, brewables, trees) of the save.
    #[serde(default)]
    pub plant_raws: Vec<PlantRaw>,
    /// Creature raws (expanded bodies, attacks) for races on the map and likely visitors/invaders.
    #[serde(default)]
    pub creature_raws: Vec<CreatureRaw>,
    #[serde(default)]
    pub weapon_raws: Vec<WeaponRaw>,
    #[serde(default)]
    pub armor_raws: Vec<ArmorRaw>,
    #[serde(default)]
    pub squads: Vec<Squad>,
    #[serde(default)]
    pub burrows: Vec<Burrow>,
    /// Civilizations and other entities (trade partners, invaders).
    #[serde(default)]
    pub entities: Vec<EntityInfo>,
    #[serde(default)]
    pub nobles: Vec<NoblePosition>,
    #[serde(default)]
    pub settings: GameSettings,
    #[serde(default)]
    pub hist_figs: Vec<HistFigName>,
    /// Merchant caravans present when the export was made (DF `plotinfo.caravans`).
    #[serde(default)]
    pub caravans: Vec<Caravan>,
    /// [construct] Engravings on smoothed tiles (DF `world.engravings`).
    #[serde(default)]
    pub engravings: Vec<Engraving>,
    /// [construct] Finished constructions built in the sim (DF `world.constructions`).
    #[serde(default)]
    pub constructions: Vec<Construction>,
}

impl World {
    pub fn in_bounds(&self, p: Pos) -> bool {
        p.x >= 0
            && p.y >= 0
            && p.z >= 0
            && (p.x as u16) < self.dims.x
            && (p.y as u16) < self.dims.y
            && (p.z as u16) < self.dims.z
    }
    #[inline]
    pub fn idx(&self, p: Pos) -> Option<usize> {
        if self.in_bounds(p) {
            Some(self.idx_unchecked(p))
        } else {
            None
        }
    }
    #[inline]
    pub fn idx_unchecked(&self, p: Pos) -> usize {
        (p.z as usize * self.dims.y as usize + p.y as usize) * self.dims.x as usize + p.x as usize
    }
    pub fn pos_of(&self, idx: usize) -> Pos {
        let x = idx % self.dims.x as usize;
        let rest = idx / self.dims.x as usize;
        let y = rest % self.dims.y as usize;
        let z = rest / self.dims.y as usize;
        Pos::new(x as i16, y as i16, z as i16)
    }
    pub fn tile(&self, p: Pos) -> Option<&Tile> {
        self.idx(p).map(|i| &self.tiles[i])
    }
    pub fn tile_mut(&mut self, p: Pos) -> Option<&mut Tile> {
        self.idx(p).map(move |i| &mut self.tiles[i])
    }
    pub fn tt(&self, p: Pos) -> &TiletypeInfo {
        match self.tile(p) {
            Some(t) => self.tiletypes.get(t.tt),
            None => self.tiletypes.get(u16::MAX),
        }
    }
    pub fn shape(&self, p: Pos) -> Shape {
        self.tt(p).shape
    }
    pub fn material(&self, mat: u16) -> Option<&MaterialInfo> {
        self.materials.get(mat as usize)
    }
    pub fn unit(&self, id: i32) -> Option<&Unit> {
        self.units.iter().find(|u| u.id == id)
    }
    pub fn item(&self, id: i32) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
    pub fn building(&self, id: i32) -> Option<&Building> {
        self.buildings.iter().find(|b| b.id == id)
    }
    pub fn citizens(&self) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(|u| u.is_worker())
    }

    /// A small synthetic fort for tests and for working without a DF export:
    /// grass surface at `surface_z`, soil below, then stone with an ore vein, a stair shaft,
    /// a dug-out hall, 7 dwarves, a cat, some food/drink/wood.
    pub fn synthetic(dx: u16, dy: u16, dz: u16) -> World {
        let tiletypes = TiletypeTable::synthetic();
        let tt_table = tiletypes.clone();
        let tt = move |n: &str| tt_table.by_name(n).expect(n);
        let materials = vec![
            mat("INORGANIC:GRANITE", "granite", [120, 120, 125], |m| m.is_stone = true),
            mat("INORGANIC:CLAY_LOAM", "clay loam", [130, 90, 60], |m| m.is_soil = true),
            mat("INORGANIC:MAGNETITE", "magnetite", [60, 60, 70], |m| {
                m.is_stone = true;
                m.is_ore = true
            }),
            mat("PLANT:OAK:WOOD", "oak", [140, 100, 50], |m| m.is_wood = true),
            mat("PLANT:GRASS_MEADOW:STRUCTURAL", "meadow-grass", [60, 140, 50], |_| {}),
            mat("PLANT:PLUMP_HELMET:DRINK", "dwarven wine", [150, 40, 90], |_| {}),
            mat("PLANT:PLUMP_HELMET:STRUCTURAL", "plump helmet", [150, 40, 150], |_| {}),
            mat("CREATURE:DWARF:SKIN", "dwarf skin", [200, 160, 130], |_| {}),
        ];
        let (granite, soil, ore, oak, grass, wine, helmet) = (0u16, 1u16, 2u16, 3u16, 4u16, 5u16, 6u16);
        let surface_z = (dz as i16 * 2 / 3).max(2);
        let mut w = World {
            meta: Meta { fort_name: "Synthfort".into(), source: "synthetic".into(), ..Default::default() },
            calendar: Calendar { year: 250, year_tick: 0 },
            dims: Dims { x: dx, y: dy, z: dz },
            tiles: vec![Tile::default(); dx as usize * dy as usize * dz as usize],
            tiletypes,
            materials,
            ..Default::default()
        };
        let (open, grass_floor, soil_wall, stone_wall, ore_wall, stone_floor) = (
            tt("OpenSpace"),
            tt("GrassLightFloor1"),
            tt("SoilWall"),
            tt("StoneWall"),
            tt("MineralWall"),
            tt("StoneFloor1"),
        );
        for z in 0..dz as i16 {
            for y in 0..dy as i16 {
                for x in 0..dx as i16 {
                    let p = Pos::new(x, y, z);
                    let i = w.idx_unchecked(p);
                    let t = &mut w.tiles[i];
                    if z > surface_z {
                        *t = Tile { tt: open, mat: NO_MAT, flags: tile_flags::OUTSIDE | tile_flags::LIGHT, ..Default::default() };
                    } else if z == surface_z {
                        *t = Tile { tt: grass_floor, mat: grass, flags: tile_flags::OUTSIDE | tile_flags::LIGHT, ..Default::default() };
                    } else if z >= surface_z - 2 {
                        *t = Tile { tt: soil_wall, mat: soil, flags: tile_flags::SUBTERRANEAN, ..Default::default() };
                    } else {
                        let vein = (x + y * 2 + z * 3) % 17 == 0 || (x - y).abs() % 23 == 0 && z % 3 == 0;
                        *t = if vein {
                            Tile { tt: ore_wall, mat: ore, flags: tile_flags::SUBTERRANEAN, ..Default::default() }
                        } else {
                            Tile { tt: stone_wall, mat: granite, flags: tile_flags::SUBTERRANEAN, ..Default::default() }
                        };
                    }
                }
            }
        }
        // Stair shaft from the surface down to a dug hall two levels below the soil.
        let (cx, cy) = (dx as i16 / 2, dy as i16 / 2);
        let hall_z = (surface_z - 4).max(0);
        for z in hall_z..=surface_z {
            let name = if z == surface_z { "StoneStairD" } else if z == hall_z { "StoneStairU" } else { "StoneStairUD" };
            let id = tt(name);
            let t = w.tile_mut(Pos::new(cx, cy, z)).unwrap();
            t.tt = id;
            t.mat = granite;
        }
        for y in cy - 4..=cy + 4 {
            for x in cx - 6..=cx + 6 {
                let p = Pos::new(x, y, hall_z);
                if (x, y) == (cx, cy) || !w.in_bounds(p) {
                    continue;
                }
                let t = w.tile_mut(p).unwrap();
                t.tt = stone_floor;
                t.mat = granite;
            }
        }
        // Mark a few walls next to the hall for digging.
        for x in cx + 7..cx + 12 {
            if let Some(t) = w.tile_mut(Pos::new(x, cy, hall_z)) {
                t.desig = Dig::Dig as u8;
            }
        }
        // Some trees on the surface.
        let trunk = tt("TreeTrunkPillar");
        for (i, (x, y)) in [(3, 3), (dx as i16 - 5, 4), (5, dy as i16 - 6), (dx as i16 - 8, dy as i16 - 4)].iter().enumerate() {
            let p = Pos::new(*x, *y, surface_z + 1);
            if let Some(t) = w.tile_mut(p) {
                t.tt = trunk;
                t.mat = oak;
            }
            w.plants.push(Plant { pos: p, token: "OAK".into(), tree: true, grow_counter: 100_000 + i as i32, ..Default::default() });
        }
        // Dwarves.
        let names = ["Urist", "Kadol", "Doren", "Ingish", "Mafol", "Zon", "Litast"];
        let profs = [("MINER", "MINE"), ("MINER", "MINE"), ("WOODCUTTER", "CUTWOOD"), ("CARPENTER", "CARPENTER"), ("MASON", "MASON"), ("PEASANT", "HAUL_STONE"), ("BREWER", "BREW")];
        for (i, n) in names.iter().enumerate() {
            let (prof, labor) = profs[i];
            w.units.push(Unit {
                id: i as i32 + 1,
                race: "DWARF".into(),
                caste: if i % 2 == 0 { "MALE".into() } else { "FEMALE".into() },
                name: format!("{n} Synthson"),
                profession: prof.into(),
                pos: Pos::new(cx - 3 + i as i16, cy - 2, hall_z),
                flags: UnitFlags { citizen: true, resident: true, ..Default::default() },
                sex: (i % 2 == 0) as i8,
                age: 40.0 + i as f32,
                hunger_timer: 10_000 * i as i32,
                thirst_timer: 5_000 * i as i32,
                sleepiness_timer: 8_000 * i as i32,
                skills: vec![Skill { id: "MINING".into(), rating: if labor == "MINE" { 5 } else { 0 }, xp: 0 }],
                labors: vec![labor.into(), "HAUL_STONE".into(), "HAUL_WOOD".into(), "HAUL_FOOD".into()],
                walk_speed: 900,
                hist_fig_id: -1,
                squad_id: -1,
                ..Default::default()
            });
        }
        w.units.push(Unit {
            id: 100,
            race: "CAT".into(),
            caste: "FEMALE".into(),
            name: "Stray cat".into(),
            profession: "STANDARD".into(),
            pos: Pos::new(cx + 2, cy + 2, hall_z),
            flags: UnitFlags { animal: true, tame: true, ..Default::default() },
            walk_speed: 600,
            hist_fig_id: -1,
            squad_id: -1,
            ..Default::default()
        });
        // Supplies in the hall.
        let mut next_id = 1000;
        let mut add = |w: &mut World, kind: &str, mat: u16, stack: i32, p: Pos, desc: &str| {
            w.items.push(Item {
                id: next_id,
                kind: kind.into(),
                mat,
                pos: p,
                stack,
                container: -1,
                holder: -1,
                building: -1,
                desc: desc.into(),
                flags: ItemFlags { on_ground: true, ..Default::default() },
                ..Default::default()
            });
            next_id += 1;
        };
        for k in 0..5 {
            add(&mut w, "DRINK", wine, 5, Pos::new(cx + 4, cy - 3 + k, hall_z), "dwarven wine");
            add(&mut w, "PLANT", helmet, 5, Pos::new(cx + 5, cy - 3 + k, hall_z), "plump helmet");
        }
        for k in 0..4 {
            add(&mut w, "WOOD", oak, 1, Pos::new(cx - 5, cy + 3, hall_z).offset(k, 0, 0), "oak log");
        }
        w
    }
}

fn mat(token: &str, name: &str, rgb: [u8; 3], f: impl FnOnce(&mut MaterialInfo)) -> MaterialInfo {
    let mut m = MaterialInfo { token: token.into(), name: name.into(), rgb, mat_type: -1, mat_index: -1, ..Default::default() };
    f(&mut m);
    m
}

impl TiletypeTable {
    /// A minimal DF-named tiletype set for synthetic worlds. Ids are NOT DF's ids; never
    /// hardcode ids, always look tiletypes up by name or attributes.
    pub fn synthetic() -> TiletypeTable {
        use Shape as S;
        use TtMaterial as M;
        use TtSpecial as P;
        let defs: &[(&str, Shape, TtMaterial, TtSpecial, u8)] = &[
            ("Void", S::None, M::None, P::None, 0),
            ("OpenSpace", S::Empty, M::Air, P::Normal, 0),
            ("RampTop", S::RampTop, M::Air, P::Normal, 0),
            ("GrassLightFloor1", S::Floor, M::GrassLight, P::Normal, 1),
            ("GrassLightFloor2", S::Floor, M::GrassLight, P::Normal, 2),
            ("SoilWall", S::Wall, M::Soil, P::Normal, 0),
            ("SoilFloor1", S::Floor, M::Soil, P::Normal, 1),
            ("SoilFloor2", S::Floor, M::Soil, P::Normal, 2),
            ("SoilStairU", S::StairUp, M::Soil, P::Normal, 0),
            ("SoilStairD", S::StairDown, M::Soil, P::Normal, 0),
            ("SoilStairUD", S::StairUpdown, M::Soil, P::Normal, 0),
            ("SoilRamp", S::Ramp, M::Soil, P::Normal, 0),
            ("StoneWall", S::Wall, M::Stone, P::Normal, 0),
            ("StoneFloor1", S::Floor, M::Stone, P::Normal, 1),
            ("StoneFloor2", S::Floor, M::Stone, P::Normal, 2),
            ("StoneFloor3", S::Floor, M::Stone, P::Normal, 3),
            ("StoneFloor4", S::Floor, M::Stone, P::Normal, 4),
            ("StoneFloorSmooth", S::Floor, M::Stone, P::Smooth, 0),
            ("StoneStairU", S::StairUp, M::Stone, P::Normal, 0),
            ("StoneStairD", S::StairDown, M::Stone, P::Normal, 0),
            ("StoneStairUD", S::StairUpdown, M::Stone, P::Normal, 0),
            ("StoneRamp", S::Ramp, M::Stone, P::Normal, 0),
            ("StoneBoulder", S::Boulder, M::Stone, P::Normal, 0),
            ("StonePebbles1", S::Pebbles, M::Stone, P::Normal, 1),
            ("MineralWall", S::Wall, M::Mineral, P::Normal, 0),
            ("MineralFloor1", S::Floor, M::Mineral, P::Normal, 1),
            ("MineralStairU", S::StairUp, M::Mineral, P::Normal, 0),
            ("MineralStairD", S::StairDown, M::Mineral, P::Normal, 0),
            ("MineralStairUD", S::StairUpdown, M::Mineral, P::Normal, 0),
            ("MineralRamp", S::Ramp, M::Mineral, P::Normal, 0),
            ("ConstructedFloor", S::Floor, M::Construction, P::Normal, 0),
            ("ConstructedWall", S::Wall, M::Construction, P::Normal, 0),
            ("TreeTrunkPillar", S::Wall, M::Tree, P::Normal, 0),
            ("TreeBranches", S::Branch, M::Tree, P::Normal, 0),
            ("Shrub", S::Shrub, M::Plant, P::Normal, 0),
            ("Sapling", S::Sapling, M::Plant, P::Normal, 0),
        ];
        let infos = defs
            .iter()
            .enumerate()
            .map(|(i, (n, s, m, p, v))| TiletypeInfo {
                id: i as u16,
                name: (*n).into(),
                shape: *s,
                material: *m,
                special: *p,
                variant: *v,
                direction: String::new(),
            })
            .collect();
        TiletypeTable::new(infos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_world_is_consistent() {
        let w = World::synthetic(48, 48, 20);
        assert_eq!(w.tiles.len(), 48 * 48 * 20);
        assert_eq!(w.citizens().count(), 7);
        for u in &w.units {
            assert!(w.shape(u.pos).has_floor(), "unit {} stands on {:?}", u.name, w.shape(u.pos));
        }
        let p = Pos::new(5, 7, 3);
        assert_eq!(w.pos_of(w.idx_unchecked(p)), p);
    }

    #[test]
    fn calendar_rolls_over() {
        let mut c = Calendar { year: 1, year_tick: crate::TICKS_PER_YEAR - 10 };
        c.advance(20);
        assert_eq!(c.year, 2);
        assert_eq!(c.year_tick, 10);
        assert_eq!(c.month_name(), "Granite");
    }
}
