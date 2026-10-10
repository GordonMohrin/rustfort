//! [saves10] FROZEN copy of `rf-core` as of Rustfort v0.9.0 (commit 1c68b92).
//!
//! Rustfort v0.9 and older wrote `.rfsave` files as `bincode(rf_core::World)` (save format 1).
//! bincode is positional: any field added to any rf-core type breaks those files. This crate
//! keeps the v0.9 layout so `rf-app`'s save loader can still read format-1 saves and convert
//! them (through serde, field by field) into the current `rf_core::World`.
//!
//! **Never change the types in this crate.** New fields belong in `rf-core` only; the
//! self-describing save format 2 tolerates added and missing fields.

#![allow(dead_code, clippy::all)]

pub mod entities;
pub mod fortress;
pub mod military;
pub mod social;
pub mod society;
pub mod tiles;
pub mod world;

pub use entities::*;
pub use fortress::*;
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
