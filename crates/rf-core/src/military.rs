//! Bodies, wounds, combat-relevant raws, squads and burrows, as exported from DF.
//!
//! Owner: the combat/military simulation (rf-sim `combat` module). Additive changes only.
//! Creature bodies come from DF's fully expanded `caste.body_info` (parts with tissue
//! layers), so we do not have to re-implement BODY_DETAIL_PLAN expansion for imported races.

use crate::tiles::Pos;
use serde::{Deserialize, Serialize};

/// One wound on a unit (DF `unit_wound` + its body-part entries, flattened per part).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Wound {
    /// Body part token (from the caste's body), e.g. `UB`, `LH`, `HEAD`.
    pub body_part: String,
    /// Tissue layer name, e.g. `SKIN`, `MUSCLE`, `BONE`.
    pub layer: String,
    /// DF wound flags, e.g. `BLEEDING`, `BROKEN`, `SEVERED`, `INFECTED`, `CUT`, `BRUISED`.
    pub flags: Vec<String>,
    /// Damage fraction 0..=10000 of the layer (DF `strain`/`contact_area`-based, estimated).
    pub severity: i32,
    pub pain: i32,
    pub bleeding: i32,
    /// Ticks since the wound was inflicted.
    pub age: i32,
}

/// Attack of a creature body part (DF `caste_attack`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AttackRaw {
    pub name: String,
    /// Verb (2nd person), e.g. `punch`, `bite`.
    pub verb: String,
    /// Body part token performing the attack.
    pub body_part: String,
    pub contact_area: i32,
    pub penetration: i32,
    /// Velocity multiplier x1000 (1000 = 1.0).
    pub velocity_multiplier: i32,
    pub prepare_ticks: i32,
    pub recover_ticks: i32,
    pub main: bool,
    /// e.g. `EDGE`, `BLUNT`? (DF `ATTACK_FLAG_*` names), `SPECIALATTACK_*` kinds.
    pub flags: Vec<String>,
}

/// One tissue layer of a body part.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TissueLayerRaw {
    /// Tissue name, e.g. `SKIN`, `FAT`, `MUSCLE`, `BONE`, `HAIR`.
    pub tissue: String,
    /// Material token of the tissue, e.g. `CREATURE:DWARF:BONE` (resolve via material palette
    /// if present: `mat` = palette index or NO_MAT).
    pub material: String,
    pub mat: u16,
    /// Relative thickness (DF `layer_depth`/`part_fraction`).
    pub thickness: i32,
    /// e.g. `CONNECTS`, `VASCULAR`, `STRUCTURAL`, `FUNCTIONAL`, `SETTABLE`, `SPLINTABLE`.
    pub flags: Vec<String>,
}

/// One body part of a caste (expanded DF body).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyPartRaw {
    pub token: String,
    pub name: String,
    pub category: String,
    /// Index of the parent part in the caste's `body_parts`, -1 for the root.
    pub parent: i32,
    /// Relative size (DF `relsize`).
    pub relsize: i32,
    /// DF body part flags, e.g. `VITAL`, `GRASP`, `STANCE`, `THOUGHT`, `NERVOUS`, `CIRCULATION`,
    /// `SKELETON`, `BREATHE`, `HEAD`, `UPPERBODY`, `LOWERBODY`, `LIMB`, `INTERNAL`.
    pub flags: Vec<String>,
    pub layers: Vec<TissueLayerRaw>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CasteRaw {
    pub token: String,
    pub name: String,
    /// Adult body size in cm³.
    pub body_size: i32,
    pub walk_speed: i32,
    pub attacks: Vec<AttackRaw>,
    pub body_parts: Vec<BodyPartRaw>,
    /// Caste/creature flags worth keeping, e.g. `FLIER`, `NOPAIN`, `NOBREATHE`, `LARGE_PREDATOR`.
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatureRaw {
    pub token: String,
    pub name: String,
    pub name_plural: String,
    pub castes: Vec<CasteRaw>,
    pub flags: Vec<String>,
}

/// Weapon item definition (DF `itemdef_weaponst`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WeaponRaw {
    /// e.g. `ITEM_WEAPON_AXE_BATTLE`.
    pub token: String,
    pub name: String,
    /// DF `job_skill` name used, e.g. `AXE`, `SWORD`, `HAMMER`, `CROSSBOW`.
    pub skill: String,
    pub size: i32,
    pub two_handed: i32,
    pub minimum_size: i32,
    pub ranged_ammo: String,
    pub attacks: Vec<AttackRaw>,
}

/// Armor/clothing definition (helms, armor, gloves, shoes, pants, shields).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ArmorRaw {
    pub token: String,
    pub name: String,
    /// `ARMOR`, `HELM`, `GLOVES`, `SHOES`, `PANTS`, `SHIELD`.
    pub kind: String,
    /// `UNDER`, `OVER`, `ARMOR`, `COVER`.
    pub layer: String,
    pub coverage: i32,
    pub flags: Vec<String>,
}

/// A military squad (DF `squad`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Squad {
    pub id: i32,
    pub name: String,
    pub alias: String,
    /// Unit ids by position (-1 = empty position).
    pub members: Vec<i32>,
    /// Current orders, DF-like strings: `Train`, `Station x,y,z`, `Kill <unit id>`, `Patrol`.
    pub orders: Vec<String>,
    /// Uniform item kinds per position (e.g. `WEAPON:ITEM_WEAPON_AXE_BATTLE`, `ARMOR:...`).
    pub uniform: Vec<String>,
    /// Name of the active alert/schedule routine, e.g. `Active/Training`, `Inactive`.
    pub schedule: String,
}

/// A burrow (DF `burrow`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Burrow {
    pub id: i32,
    pub name: String,
    pub tiles: Vec<Pos>,
    pub units: Vec<i32>,
}
