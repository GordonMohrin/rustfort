//! rf-core: the shared data model of Rustfort.
//!
//! Everything here mirrors what the DFHack exporter writes (see `docs/EXPORT_FORMAT.md`).
//! Field names are the serde names used in the export's JSON files, so the importer can
//! deserialize them directly. IDs are DF's own IDs, so a migrated fort keeps its identity.
//!
//! This crate is the contract between the other crates. Keep it small and dependency-free
//! (serde only). Changes should be additive.

pub mod entities;
pub mod military;
pub mod social;
pub mod society;
pub mod tiles;
pub mod world;

pub use entities::*;
pub use military::*;
pub use social::*;
pub use society::*;
pub use tiles::*;
pub use world::*;

/// DF time base: ticks per day / month / year (Tier A, exact).
pub const TICKS_PER_DAY: i32 = 1200;
pub const DAYS_PER_MONTH: i32 = 28;
pub const TICKS_PER_MONTH: i32 = TICKS_PER_DAY * DAYS_PER_MONTH; // 33_600
pub const TICKS_PER_YEAR: i32 = TICKS_PER_MONTH * 12; // 403_200
