//! [health15] Hospital and medicine depth (target v0.15). Spec: `docs/mechanics/health15.md`.
//!
//! # DF rules implemented (wiki: Health care, Hospital, Cast, Splint, Traction bench, Crutch,
//! Crutch walker, Plaster, Wound, Surgeon; raws: tissue HEALING_RATE, gaits, MAKE_PLASTER_POWDER)
//! - **Hospitals** (M1): civzones with the hospital purpose and v50 hospital *locations* on
//!   meeting-area zones; rebuilt daily (zones/locations created at runtime count).
//! - **Per-tissue healing** (M8): DF `HEALING_RATE` (soft tissue 100, bone 1000 = ten times
//!   slower, nerve/brain/cartilage/nail/tooth/horn: never). Wounds that never heal become
//!   permanent damage once they stopped bleeding.
//! - **Fracture classes** (M4/M5): displaced fractures must be set; set bones are immobilized
//!   with a splint or a plaster cast; compound fractures need surgery first; overlapping
//!   fractures need a traction bench and never heal outside one (Toady One).
//! - **Supplies** (M3): suture thread stays sewn into the patient; splints and casts stay on
//!   until the bone has healed; soap is used up (150 units per bar).
//! - **Nerve damage and crutches** (M9/M10): severed motor nerves disable the part and every
//!   part beyond it for good, sensory nerve damage removes pain; patients who lost the use of a
//!   leg get a crutch (not for spinal injuries); crutch walking skill removes the slowdown at
//!   Legendary+5; without a crutch they crawl at the raws' crawl/walk gait ratio (2990/900).
//! - **Outcomes** (M11/M12): suturing, bone setting, cleaning and surgery can fail, less often
//!   with skill; surgery in a hospital without a table uses half the skill.
//!
//! # Estimates (not on the wiki), see `docs/FIDELITY.md` section [health15]
//! Success chances, nerve odds, fracture class thresholds, crutch penalty x2 at skill 0,
//! durations, soap per cleaning (15 units).

mod disab;
mod ortho;
mod outcome;
mod supplies;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod verify; // adversarial verification tests

pub(crate) use ortho::{heal_of_tissue, immobilized, layer_from_raw, mx_of_flags, needs_setting, needs_surgery, scale_heal, settable_now, settable_tissue};
#[allow(unused_imports)]
pub(crate) use outcome::success_pct;
pub use disab::Walk;
#[allow(unused_imports)]
pub(crate) use disab::crawl_delay; // [health15] for health.rs tests

use crate::{Kind, Sim};
use serde::{Deserialize, Serialize};

/// `h15_tick` work runs at `now % 100 == H15_PHASE` (just before `health_tick`'s pass).
pub(crate) const H15_PHASE: u64 = 37;
/// `LayerT::heal` value of a tissue that never heals.
pub(crate) const NO_HEAL: u16 = u16::MAX;
/// Severity added per 100 ticks to an unset, unimmobilized fracture (estimate; the old
/// +30 scaled by the bone healing rate).
pub(crate) const UNSET_WORSEN: i32 = 3;
/// DWARF gaits (raws): crawl 2990, walk 900.
pub(crate) const CRAWL_NUM: i32 = 2990;
pub(crate) const CRAWL_DEN: i32 = 900;

/// Medical extension bits on `combat::WoundS::mx`.
pub mod mx {
    /// Classified by health15.
    pub const SEEN: u16 = 1;
    /// Displaced fracture: must be set before it is immobilized.
    pub const NEEDS_SET: u16 = 2;
    /// Compound fracture (bone through the skin): surgery first.
    pub const COMPOUND: u16 = 4;
    /// Overlapping fracture: traction bench.
    pub const OVERLAP: u16 = 8;
    pub const SPLINT: u16 = 16;
    pub const CAST: u16 = 32;
    pub const TRACTION: u16 = 64;
    pub const IMMOB: u16 = SPLINT | CAST | TRACTION;
    /// Motor nerve severed (permanent).
    pub const MOTOR: u16 = 128;
    /// Sensory nerve severed (permanent).
    pub const SENSORY: u16 = 256;
    /// Cleaning was attempted without success.
    pub const CLEAN_FAIL: u16 = 512;
    /// Botched surgery.
    pub const SURG_FAIL: u16 = 1024;
    /// Cloth sling (minor fracture without a splint).
    pub const SLING: u16 = 2048;
    pub const NAMES: [(u16, &str); 11] = [
        (NEEDS_SET, "NEEDS_SETTING"),
        (COMPOUND, "COMPOUND_FRACTURE"),
        (OVERLAP, "OVERLAPPING_FRACTURE"),
        (SPLINT, "SPLINT"),
        (CAST, "CAST"),
        (TRACTION, "TRACTION"),
        (MOTOR, "MOTOR_NERVE_SEVERED"),
        (SENSORY, "SENSORY_NERVE_SEVERED"),
        (CLEAN_FAIL, "CLEANING_FAILED"),
        (SURG_FAIL, "SURGERY_FAILED"),
        (SLING, "SLING"),
    ];
}

/// Hospital supply kinds (DF hospital screen).
#[repr(u8)]
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sup {
    Thread,
    Cloth,
    Splint,
    Crutch,
    Powder,
    Bucket,
    Soap,
}
pub const NSUP: usize = 7;
pub const SUPS: [Sup; NSUP] = [Sup::Thread, Sup::Cloth, Sup::Splint, Sup::Crutch, Sup::Powder, Sup::Bucket, Sup::Soap];
/// DF screen units per item (wiki Health care: 1 thread = 15 000, 1 cloth = 10 000, soap bar
/// and plaster powder 150).
pub const SUP_UNITS: [i32; NSUP] = [15_000, 10_000, 1, 1, 150, 1, 150];
/// Default desired items per hospital (estimate, from the v0.47 screen).
pub const SUP_DEFAULT: [i32; NSUP] = [5, 5, 5, 5, 5, 2, 5];
pub const SUP_NAMES: [&str; NSUP] = ["thread", "cloth", "splints", "crutches", "plaster powder", "buckets", "soap"];

/// One hospital (a location, or a plain hospital zone).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Hosp {
    /// Location id, or -(zone building id) for a plain hospital zone.
    pub key: i32,
    pub name: String,
    pub zones: Vec<i32>,
    pub rects: Vec<(i16, i16, i16, i16, i16)>,
    /// Chest/cabinet tiles inside the zone (supply storage).
    pub chests: Vec<u32>,
    pub tables: u32,
    pub beds: u32,
    pub want: [i32; NSUP],
    pub have: [i32; NSUP],
}

/// A traction bench inside a hospital.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub(crate) struct Bench {
    pub building: i32,
    pub tile: u32,
    /// Patient slot (reserved or placed), NONE = free.
    pub occupant: u32,
    pub placed: bool,
    pub since: u64,
}

/// Permanent and long-term disabilities of one unit, and what it carries for them.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub(crate) struct Disab {
    pub slot: u32,
    /// Parts with a severed motor / sensory nerve (sorted).
    pub motor: Vec<u16>,
    pub sensory: Vec<u16>,
    /// (part, layer) of permanent damage to tissue that never heals.
    pub perm: Vec<(u16, u16)>,
    /// Held crutch item id, -1 none; tick since it is no longer needed (0 = needed).
    pub crutch: i32,
    pub crutch_free_since: u64,
    /// Last gait applied: 0 normal, 1 crutch, 2 crawl (plus the crutch rating * 4).
    pub walk_key: u16,
    pub last_pos: u32,
    /// Strapped items: (item id, part, kind 0 splint / 1 cast / 2 sling).
    pub attached: Vec<(i32, u16, u8)>,
}

/// Counters and recent events (UI).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Health15Report {
    pub hospitals: usize,
    pub benches: usize,
    pub fractures_classified: u32,
    pub compound: u32,
    pub overlapping: u32,
    pub splints_applied: u32,
    pub slings_applied: u32,
    pub casts_applied: u32,
    pub placed_in_traction: u32,
    pub released_from_traction: u32,
    pub rebroken: u32,
    pub crutches_given: u32,
    pub crutches_returned: u32,
    pub motor_nerves: u32,
    pub sensory_nerves: u32,
    pub permanent: u32,
    pub sutures_failed: u32,
    pub settings_failed: u32,
    pub cleanings_failed: u32,
    pub surgeries_failed: u32,
    pub infections_excised: u32,
    pub soap_bars_used: u32,
    pub threads_sewn: u32,
    pub items_removed: u32,
    pub stocked: u32,
    pub events: Vec<(u64, String)>,
}

/// Free supply counts (daily, from `health_refresh_daily`'s item loop).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub(crate) struct SupCount {
    pub crutches: i32,
    pub powder: i32,
    pub buckets: i32,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct Health15 {
    pub off: bool,
    pub inited: bool,
    pub hosp: Vec<Hosp>,
    pub benches: Vec<Bench>,
    /// Sorted by slot.
    pub dis: Vec<Disab>,
    pub soap_units: i32,
    pub cnt: SupCount,
    /// Pending counts while `health_refresh_daily` scans (copied to `cnt` at its end).
    pub cnt_new: SupCount,
    /// Player-set desired supplies by hospital key (kept across rebuilds).
    pub want_set: Vec<(i32, [i32; NSUP])>,
    /// Open stocking hauls: (item id, hospital key, hauler slot).
    pub stock: Vec<(i32, i32, u32)>,
    /// Tiles of chests/cabinets inside hospitals (sorted; M2 earmark).
    pub chest_tiles: Vec<u32>,
    pub rep: Health15Report,
    /// Scratch for part walks (no allocation in the 100-tick pass).
    #[serde(skip)]
    pub scratch: Vec<u16>,
}

impl Sim {
    /// [health15] Hook from `tick_inner`, right before `health_tick`.
    pub(crate) fn h15_tick(&mut self, now: u64) {
        if self.h15.off || self.health.off || now % 100 != H15_PHASE {
            return;
        }
        if !self.h15.inited {
            self.h15_init();
        }
        if now % rf_core::TICKS_PER_DAY as u64 == 637 {
            self.h15_daily(now);
        }
        if now % 400 == 237 {
            self.h15_stock_pass(now);
        }
        self.h15_pass(now);
    }

    /// First run (also after loading an old save without health15 state).
    pub(crate) fn h15_init(&mut self) {
        self.h15.inited = true;
        self.init_health(); // treatment flags of exported wounds first
        self.h15_rebuild_hospitals();
        if !self.h15.hosp.is_empty() {
            self.health_refresh_daily(); // count the stock already in the hospitals
        }
        self.h15_import_disabilities();
    }

    fn h15_daily(&mut self, now: u64) {
        self.h15_rebuild_hospitals();
        self.h15_cleanup_dis(now);
        if self.h15.rep.events.len() > 400 {
            self.h15.rep.events.drain(..200);
        }
    }

    /// Every 100 ticks: classify new wounds, permanent damage, traction, disabilities and gait.
    fn h15_pass(&mut self, now: u64) {
        let n = self.combat.wounded.len();
        for i in 0..n {
            let Some(&s) = self.combat.wounded.get(i) else { break };
            let s = s as usize;
            if s >= self.ai.len() || self.ai[s].kind == Kind::Dead || self.combat.f[s].body == u16::MAX {
                continue;
            }
            self.h15_classify(s, now);
            self.h15_make_permanent(s, now);
        }
        self.h15_traction_check(now);
        let mut k = 0;
        while k < self.h15.dis.len() {
            let s = self.h15.dis[k].slot as usize;
            k += 1;
            if s >= self.ai.len() || self.ai[s].kind == Kind::Dead {
                continue;
            }
            self.h15_apply_disab(s);
            self.h15_detach(s, now);
            self.h15_crutch_tick(s, now);
            if self.ai[s].kind != Kind::Dead {
                self.health_gait(s);
            }
        }
    }

    /// Index of the disability entry of slot `s`.
    pub(crate) fn h15_dis_idx(&self, s: usize) -> Option<usize> {
        self.h15.dis.binary_search_by_key(&(s as u32), |d| d.slot).ok()
    }

    /// The disability entry of `s`, created if missing.
    pub(crate) fn h15_dis_mut(&mut self, s: usize) -> &mut Disab {
        let k = match self.h15.dis.binary_search_by_key(&(s as u32), |d| d.slot) {
            Ok(k) => k,
            Err(k) => {
                self.h15.dis.insert(k, Disab { slot: s as u32, crutch: -1, ..Default::default() });
                k
            }
        };
        &mut self.h15.dis[k]
    }

    pub(crate) fn h15_event(&mut self, now: u64, text: String) {
        self.h15.rep.events.push((now, text));
    }

    /// Drop entries that hold nothing any more.
    fn h15_cleanup_dis(&mut self, _now: u64) {
        let ai = &self.ai;
        self.h15.dis.retain(|d| {
            let alive = (d.slot as usize) < ai.len() && ai[d.slot as usize].kind != Kind::Dead;
            alive && (!d.motor.is_empty() || !d.sensory.is_empty() || !d.perm.is_empty() || d.crutch >= 0 || d.walk_key != 0 || !d.attached.is_empty())
        });
    }

    /// Part of `state_hash`.
    pub(crate) fn h15_hash(&self, mix: &mut dyn FnMut(u64)) {
        let r = &self.h15.rep;
        mix(((r.splints_applied as u64) << 40) ^ ((r.crutches_given as u64) << 20) ^ (r.motor_nerves as u64) ^ ((r.casts_applied as u64) << 50));
        mix(((r.placed_in_traction as u64) << 32) ^ ((r.sutures_failed as u64) << 16) ^ r.surgeries_failed as u64 ^ ((r.permanent as u64) << 48));
        mix(self.h15.dis.len() as u64 ^ ((self.h15.soap_units as u64) << 24) ^ ((self.h15.hosp.len() as u64) << 40));
        for d in &self.h15.dis {
            mix(((d.slot as u64) << 32) ^ (d.crutch as u32 as u64) ^ ((d.walk_key as u64) << 16));
        }
        for b in &self.h15.benches {
            mix(((b.building as u64) << 32) ^ b.occupant as u64);
        }
    }

    /// [saves11] After an exact load: caches only.
    pub(crate) fn h15_after_load(&mut self) {
        self.h15.scratch.clear();
    }
}
