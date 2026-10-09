//! Tiles, tiletypes and positions.
//!
//! A tile stores DF's tiletype id (`tt`) plus an index into the export's material palette.
//! What a tiletype *means* (wall/floor/ramp, stone/soil/grass, smooth, variant) lives in the
//! [`TiletypeTable`], which comes from DF's own tiletype enum via the exporter, so the ids
//! are exactly DF's ids.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Map position in local embark coordinates (z = 0 is the bottom of the map).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Pos {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

impl Pos {
    pub const fn new(x: i16, y: i16, z: i16) -> Self {
        Self { x, y, z }
    }
    pub const fn offset(self, dx: i16, dy: i16, dz: i16) -> Self {
        Self { x: self.x + dx, y: self.y + dy, z: self.z + dz }
    }
    /// Chebyshev distance in the xy plane plus |dz| (DF-style rough distance).
    pub fn dist(self, o: Pos) -> i32 {
        let dx = (self.x as i32 - o.x as i32).abs();
        let dy = (self.y as i32 - o.y as i32).abs();
        let dz = (self.z as i32 - o.z as i32).abs();
        dx.max(dy) + dz
    }
    /// Manhattan distance, which DF uses for "nearest item" searches.
    pub fn manhattan(self, o: Pos) -> i32 {
        (self.x as i32 - o.x as i32).abs()
            + (self.y as i32 - o.y as i32).abs()
            + (self.z as i32 - o.z as i32).abs()
    }
}

/// DF `tiletype_shape`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u8)]
pub enum Shape {
    #[default]
    None,
    Empty,
    Floor,
    Boulder,
    Pebbles,
    Wall,
    Fortification,
    StairUp,
    StairDown,
    StairUpdown,
    Ramp,
    RampTop,
    BrookBed,
    BrookTop,
    Branch,
    TrunkBranch,
    Twig,
    Sapling,
    Shrub,
    EndlessPit,
}

impl Shape {
    /// Parse DF's enum name (`"WALL"`, `"STAIR_UPDOWN"`, ...). Unknown -> `None`.
    pub fn from_df(s: &str) -> Shape {
        use Shape::*;
        match s {
            "EMPTY" => Empty,
            "FLOOR" => Floor,
            "BOULDER" => Boulder,
            "PEBBLES" => Pebbles,
            "WALL" => Wall,
            "FORTIFICATION" => Fortification,
            "STAIR_UP" => StairUp,
            "STAIR_DOWN" => StairDown,
            "STAIR_UPDOWN" => StairUpdown,
            "RAMP" => Ramp,
            "RAMP_TOP" => RampTop,
            "BROOK_BED" => BrookBed,
            "BROOK_TOP" => BrookTop,
            "BRANCH" => Branch,
            "TRUNK_BRANCH" => TrunkBranch,
            "TWIG" => Twig,
            "SAPLING" => Sapling,
            "SHRUB" => Shrub,
            "ENDLESS_PIT" => EndlessPit,
            _ => None,
        }
    }
    /// Blocks movement completely (walls, fortifications and tree trunks are `Wall`).
    pub fn is_solid(self) -> bool {
        matches!(self, Shape::Wall | Shape::Fortification | Shape::None)
    }
    /// Has a walkable surface a unit can stand on (without flying/climbing).
    pub fn has_floor(self) -> bool {
        use Shape::*;
        matches!(
            self,
            Floor | Boulder | Pebbles | StairUp | StairDown | StairUpdown | Ramp | BrookBed | Sapling | Shrub
                | Branch | TrunkBranch | BrookTop
        )
    }
    pub fn is_open(self) -> bool {
        matches!(self, Shape::Empty | Shape::RampTop | Shape::Twig | Shape::EndlessPit)
    }
    /// Allows moving up from this tile to z+1 (the tile above must allow moving down).
    pub fn allows_up(self) -> bool {
        matches!(self, Shape::StairUp | Shape::StairUpdown)
    }
    /// Allows arriving here from z+1 / leaving to z-1 via stairs.
    pub fn allows_down(self) -> bool {
        matches!(self, Shape::StairDown | Shape::StairUpdown)
    }
}

/// DF `tiletype_material`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u8)]
pub enum TtMaterial {
    #[default]
    None,
    Air,
    Soil,
    Stone,
    Feature,
    LavaStone,
    Mineral,
    FrozenLiquid,
    Construction,
    GrassLight,
    GrassDark,
    GrassDry,
    GrassDead,
    Plant,
    Hfs,
    Campfire,
    Fire,
    Ashes,
    Magma,
    Driftwood,
    Pool,
    Brook,
    River,
    Root,
    Tree,
    Mushroom,
    UnderworldGate,
}

impl TtMaterial {
    pub fn from_df(s: &str) -> TtMaterial {
        use TtMaterial::*;
        match s {
            "AIR" => Air,
            "SOIL" => Soil,
            "STONE" => Stone,
            "FEATURE" => Feature,
            "LAVA_STONE" => LavaStone,
            "MINERAL" => Mineral,
            "FROZEN_LIQUID" => FrozenLiquid,
            "CONSTRUCTION" => Construction,
            "GRASS_LIGHT" => GrassLight,
            "GRASS_DARK" => GrassDark,
            "GRASS_DRY" => GrassDry,
            "GRASS_DEAD" => GrassDead,
            "PLANT" => Plant,
            "HFS" => Hfs,
            "CAMPFIRE" => Campfire,
            "FIRE" => Fire,
            "ASHES" => Ashes,
            "MAGMA" => Magma,
            "DRIFTWOOD" => Driftwood,
            "POOL" => Pool,
            "BROOK" => Brook,
            "RIVER" => River,
            "ROOT" => Root,
            "TREE" => Tree,
            "MUSHROOM" => Mushroom,
            "UNDERWORLD_GATE" => UnderworldGate,
            _ => None,
        }
    }
    pub fn is_grass(self) -> bool {
        matches!(self, TtMaterial::GrassLight | TtMaterial::GrassDark | TtMaterial::GrassDry | TtMaterial::GrassDead)
    }
    /// Natural rock/soil that mining turns into a floor (and possibly a boulder).
    pub fn is_diggable(self) -> bool {
        matches!(
            self,
            TtMaterial::Soil
                | TtMaterial::Stone
                | TtMaterial::Mineral
                | TtMaterial::LavaStone
                | TtMaterial::FrozenLiquid
                | TtMaterial::Feature
        ) || self.is_grass()
    }
}

/// DF `tiletype_special`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u8)]
pub enum TtSpecial {
    #[default]
    None,
    Normal,
    RiverSource,
    Waterfall,
    Smooth,
    Furrowed,
    Wet,
    Dead,
    #[serde(rename = "WORN_1", alias = "WORN1")]
    Worn1,
    #[serde(rename = "WORN_2", alias = "WORN2")]
    Worn2,
    #[serde(rename = "WORN_3", alias = "WORN3")]
    Worn3,
    Track,
    SmoothDead,
    /// DF 53: badly built constructions.
    Shoddy,
}

impl TtSpecial {
    pub fn from_df(s: &str) -> TtSpecial {
        use TtSpecial::*;
        match s {
            "NORMAL" => Normal,
            "RIVER_SOURCE" => RiverSource,
            "WATERFALL" => Waterfall,
            "SMOOTH" => Smooth,
            "FURROWED" => Furrowed,
            "WET" => Wet,
            "DEAD" => Dead,
            "WORN_1" => Worn1,
            "WORN_2" => Worn2,
            "WORN_3" => Worn3,
            "TRACK" => Track,
            "SMOOTH_DEAD" => SmoothDead,
            "SHODDY" => Shoddy,
            _ => None,
        }
    }
}

/// One entry of DF's tiletype enum (exported from `df.tiletype.attrs`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TiletypeInfo {
    pub id: u16,
    /// DF enum name, e.g. `StoneWall`, `StoneFloor2`, `OpenSpace`, `ConstructedWallLRUD`.
    pub name: String,
    pub shape: Shape,
    pub material: TtMaterial,
    pub special: TtSpecial,
    /// 0 = none, 1..=4 = VAR_1..VAR_4.
    pub variant: u8,
    /// DF direction string for smoothed walls/tracks, e.g. `"--NS"`; empty if none.
    #[serde(default)]
    pub direction: String,
}

/// Lookup table over DF tiletypes. Index = DF tiletype id.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TiletypeTable {
    pub infos: Vec<TiletypeInfo>,
    #[serde(skip)]
    by_name: HashMap<String, u16>,
    #[serde(skip)]
    by_key: HashMap<(Shape, TtMaterial, TtSpecial, u8), u16>,
}

impl TiletypeTable {
    pub fn new(mut infos: Vec<TiletypeInfo>) -> Self {
        infos.sort_by_key(|i| i.id);
        let mut t = Self { infos, by_name: HashMap::new(), by_key: HashMap::new() };
        t.reindex();
        t
    }

    /// Rebuild the lookup maps (call after deserializing).
    pub fn reindex(&mut self) {
        self.by_name.clear();
        self.by_key.clear();
        for i in &self.infos {
            self.by_name.insert(i.name.clone(), i.id);
            if i.direction.is_empty() {
                self.by_key.entry((i.shape, i.material, i.special, i.variant)).or_insert(i.id);
            }
        }
    }

    /// Info for a tiletype id. Ids are dense in DF's enum; unknown ids get a `None` dummy.
    pub fn get(&self, id: u16) -> &TiletypeInfo {
        static DUMMY: TiletypeInfo = TiletypeInfo {
            id: u16::MAX,
            name: String::new(),
            shape: Shape::None,
            material: TtMaterial::None,
            special: TtSpecial::None,
            variant: 0,
            direction: String::new(),
        };
        match self.infos.get(id as usize) {
            Some(i) if i.id == id => i,
            _ => self.infos.iter().find(|i| i.id == id).unwrap_or(&DUMMY),
        }
    }

    pub fn by_name(&self, name: &str) -> Option<u16> {
        self.by_name.get(name).copied()
    }

    /// Find a tiletype by its attributes. Falls back to any variant, then to `special`
    /// NORMAL, so callers can ask e.g. for "stone floor, variant 3" and always get something.
    pub fn find(&self, shape: Shape, material: TtMaterial, special: TtSpecial, variant: u8) -> Option<u16> {
        if let Some(&id) = self.by_key.get(&(shape, material, special, variant)) {
            return Some(id);
        }
        for v in 0..=4u8 {
            if let Some(&id) = self.by_key.get(&(shape, material, special, v)) {
                return Some(id);
            }
        }
        for sp in [TtSpecial::Normal, TtSpecial::None] {
            for v in 0..=4u8 {
                if let Some(&id) = self.by_key.get(&(shape, material, sp, v)) {
                    return Some(id);
                }
            }
        }
        None
    }
}

/// Tile flag bits (`Tile::flags`).
pub mod tile_flags {
    pub const HIDDEN: u16 = 1 << 0;
    pub const OUTSIDE: u16 = 1 << 1;
    pub const SUBTERRANEAN: u16 = 1 << 2;
    pub const LIGHT: u16 = 1 << 3;
    pub const AQUIFER: u16 = 1 << 4;
    /// Tile belongs to a stockpile.
    pub const PILE: u16 = 1 << 5;
    /// Traffic designation in bits 6..=7: 0 normal, 1 low, 2 high, 3 restricted.
    pub const TRAFFIC_SHIFT: u16 = 6;
    pub const TRAFFIC_MASK: u16 = 0b11 << 6;
    pub const DESIG_SMOOTH: u16 = 1 << 8;
    pub const DESIG_ENGRAVE: u16 = 1 << 9;
    /// Designation is in marker (blueprint) mode and not active yet.
    pub const MARKER: u16 = 1 << 10;
    /// A building occupies this tile.
    pub const OCC_BUILDING: u16 = 1 << 11;
    pub const STAGNANT: u16 = 1 << 12;
    pub const SALT: u16 = 1 << 13;
    /// Tile was dug/changed this session (internal, not exported).
    pub const DIRTY: u16 = 1 << 15;
}

/// Liquid byte: bits 0..=2 depth (0..=7), bit 3 = magma (else water).
pub mod liquid {
    pub const DEPTH_MASK: u8 = 0b111;
    pub const MAGMA: u8 = 1 << 3;
}

/// DF `tile_dig_designation` (+ tree chopping / plant gathering, which DF expresses as a
/// `Dig` designation on a tree or shrub tile).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Dig {
    #[default]
    None = 0,
    /// "Mine" on walls, "chop" on trees, "gather" on shrubs.
    Dig = 1,
    UpDownStair = 2,
    Channel = 3,
    Ramp = 4,
    DownStair = 5,
    UpStair = 6,
}

impl Dig {
    pub fn from_u8(v: u8) -> Dig {
        match v {
            1 => Dig::Dig,
            2 => Dig::UpDownStair,
            3 => Dig::Channel,
            4 => Dig::Ramp,
            5 => Dig::DownStair,
            6 => Dig::UpStair,
            _ => Dig::None,
        }
    }
}

/// Palette index meaning "no material".
pub const NO_MAT: u16 = u16::MAX;

/// One map tile, 8 bytes. Binary layout in the export is exactly this (little endian):
/// `tt u16, mat u16, flags u16, liquid u8, desig u8`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct Tile {
    /// DF tiletype id, see [`TiletypeTable`].
    pub tt: u16,
    /// Index into `World::materials` ([`NO_MAT`] if none, e.g. open air).
    pub mat: u16,
    /// [`tile_flags`] bits.
    pub flags: u16,
    /// [`liquid`] byte.
    pub liquid: u8,
    /// [`Dig`] designation as u8.
    pub desig: u8,
}

impl Tile {
    pub fn hidden(&self) -> bool {
        self.flags & tile_flags::HIDDEN != 0
    }
    pub fn liquid_depth(&self) -> u8 {
        self.liquid & liquid::DEPTH_MASK
    }
    pub fn is_magma(&self) -> bool {
        self.liquid & liquid::MAGMA != 0
    }
    pub fn dig(&self) -> Dig {
        Dig::from_u8(self.desig)
    }
    pub fn traffic(&self) -> u8 {
        ((self.flags & tile_flags::TRAFFIC_MASK) >> tile_flags::TRAFFIC_SHIFT) as u8
    }
}
